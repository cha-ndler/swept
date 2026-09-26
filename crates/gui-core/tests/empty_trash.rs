//! The GUI's Empty Trash command: the only permanent removal it offers.
//!
//! Fixtures only. "The Trash" is `<tempdir>/.Trash`, the sink is a `DirSink`
//! whose recoverable destination is a `fixture-bin`, and the audit log lives in
//! the tempdir.

use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};

use swept_core::audit::AuditLog;
use swept_core::executor::DirSink;
use swept_gui_core::{
    empty_trash_at, empty_trash_with_sink, trash_contents, EmptyTrashRequest, Expected,
};

fn fake_home() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(dir.path()).unwrap();
    fs::create_dir_all(home.join(".Trash")).unwrap();
    (dir, home)
}

fn write(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn sink(home: &Path) -> DirSink {
    DirSink {
        trash_dir: home.join("fixture-bin"),
    }
}

/// A request bound to what the preview shows right now, acknowledged.
fn acknowledged(home: &Path) -> EmptyTrashRequest {
    let c = trash_contents(home);
    EmptyTrashRequest {
        expected: Expected {
            count: c.files + c.folders,
            bytes: c.bytes,
        },
        fingerprint: c.fingerprint,
        acknowledged_unrecoverable: true,
        confirm_mass_delete: c.requires_confirmation,
    }
}

fn run(home: &Path, req: &EmptyTrashRequest) -> Result<swept_gui_core::EmptyTrashSummary, String> {
    let mut audit = AuditLog::open(&home.join("audit.jsonl")).unwrap();
    empty_trash_with_sink(home, req, &sink(home), &mut audit)
}

fn audit_text(home: &Path) -> String {
    fs::read_to_string(home.join("audit.jsonl")).unwrap_or_default()
}

fn populate(home: &Path) {
    write(&home.join(".Trash/a.bin"), b"12345");
    write(&home.join(".Trash/folder/b.txt"), b"678");
    write(&home.join(".Trash/folder/deeper/c.txt"), b"9");
}

#[test]
fn the_preview_counts_what_is_in_the_trash() {
    let (_g, home) = fake_home();
    populate(&home);
    let c = trash_contents(&home);
    assert!(c.readable);
    assert_eq!(c.items, 2);
    assert_eq!(c.files, 3);
    assert_eq!(c.folders, 2);
    assert_eq!(c.bytes, 9);
    assert_eq!(c.left_behind, 0);
}

#[test]
fn without_acknowledgement_nothing_is_deleted_and_the_refusal_is_recorded() {
    let (_g, home) = fake_home();
    populate(&home);
    let mut req = acknowledged(&home);
    req.acknowledged_unrecoverable = false;

    let err = run(&home, &req).unwrap_err();

    assert!(err.contains("acknowledge"), "got: {err}");
    assert!(home.join(".Trash/a.bin").exists());
    assert!(audit_text(&home).contains("\"disposition\":\"refused\""));
}

#[test]
fn a_request_that_omits_the_acknowledgement_is_refused() {
    let req: EmptyTrashRequest = serde_json::from_str(
        r#"{"expected":{"count":1,"bytes":1},"fingerprint":"f","confirm_mass_delete":false}"#,
    )
    .unwrap();
    assert!(!req.acknowledged_unrecoverable, "absent means not given");

    let missing_expected = serde_json::from_str::<EmptyTrashRequest>(
        r#"{"fingerprint":"f","acknowledged_unrecoverable":true}"#,
    );
    assert!(missing_expected.is_err(), "the figures are required");

    let missing_fingerprint = serde_json::from_str::<EmptyTrashRequest>(
        r#"{"expected":{"count":1,"bytes":1},"acknowledged_unrecoverable":true}"#,
    );
    assert!(missing_fingerprint.is_err(), "what was shown is required");

    let unknown = serde_json::from_str::<EmptyTrashRequest>(
        r#"{"expected":{"count":1,"bytes":1},"fingerprint":"f","acknowledged_unrecoverable":true,"confirm_mass_delete":false,"scope":"all"}"#,
    );
    assert!(unknown.is_err(), "unknown fields are rejected");
}

#[test]
fn acknowledged_it_deletes_every_file_and_folder_but_keeps_the_trash_folder() {
    let (_g, home) = fake_home();
    populate(&home);

    let s = run(&home, &acknowledged(&home)).unwrap();

    assert_eq!(s.files_deleted, 3);
    assert_eq!(s.folders_removed, 2);
    assert_eq!(s.bytes_deleted, 9);
    assert_eq!(s.refused, 0);
    assert_eq!(s.left_behind, 0);
    assert!(home.join(".Trash").is_dir());
    assert_eq!(fs::read_dir(home.join(".Trash")).unwrap().count(), 0);
    assert!(
        !home.join("fixture-bin").exists(),
        "nothing was moved, only removed"
    );
    assert!(audit_text(&home).contains("\"disposition\":\"permanent\""));
}

#[test]
fn it_never_touches_anything_outside_the_trash() {
    let (_g, home) = fake_home();
    populate(&home);
    let outside = [
        "Documents/precious.txt",
        "Library/Caches/app/blob",
        "Library/Keychains/login.keychain-db",
        "Projects/app/.git/config",
        "Downloads/x",
    ];
    for p in outside {
        write(&home.join(p), p.as_bytes());
    }

    run(&home, &acknowledged(&home)).unwrap();

    for p in outside {
        assert_eq!(fs::read(home.join(p)).unwrap(), p.as_bytes(), "{p} changed");
    }
}

#[test]
fn one_new_file_since_the_preview_refuses_the_whole_run() {
    let (_g, home) = fake_home();
    populate(&home);
    let req = acknowledged(&home);
    write(&home.join(".Trash/just-thrown-away.txt"), b"z");

    let err = run(&home, &req).unwrap_err();

    assert!(err.contains("changed since"), "got: {err}");
    assert!(
        home.join(".Trash/a.bin").exists(),
        "the originals survive too"
    );
    assert!(home.join(".Trash/just-thrown-away.txt").exists());
    assert!(audit_text(&home).contains("changed since"));
}

#[test]
fn a_trash_that_shrank_since_the_preview_is_refused_too() {
    // Put Back is the usual reason, and it is paired with throwing something
    // else away often enough that "smaller" cannot be read as "a subset".
    let (_g, home) = fake_home();
    populate(&home);
    let req = acknowledged(&home);
    fs::remove_file(home.join(".Trash/a.bin")).unwrap();

    let err = run(&home, &req).unwrap_err();
    assert!(err.contains("changed since"), "got: {err}");
    assert!(home.join(".Trash/folder/b.txt").exists());
}

#[test]
fn a_swap_that_keeps_the_count_and_size_is_refused() {
    // Put Back one 5-byte file, throw away a different 5-byte one: the same
    // figures, and a file the user was never shown.
    let (_g, home) = fake_home();
    populate(&home);
    let req = acknowledged(&home);
    fs::remove_file(home.join(".Trash/a.bin")).unwrap();
    let unseen = home.join(".Trash/unseen.doc");
    write(&unseen, b"54321");

    let err = run(&home, &req).unwrap_err();
    assert!(err.contains("changed since"), "got: {err}");
    assert!(unseen.exists(), "the file nobody was shown survives");
}

#[test]
fn a_file_rewritten_in_place_is_refused() {
    let (_g, home) = fake_home();
    populate(&home);
    let req = acknowledged(&home);
    // Same name, same size, different file.
    fs::remove_file(home.join(".Trash/a.bin")).unwrap();
    write(&home.join(".Trash/a.bin"), b"abcde");

    assert!(run(&home, &req).is_err());
    assert_eq!(fs::read(home.join(".Trash/a.bin")).unwrap(), b"abcde");
}

#[test]
fn empty_folders_count_toward_the_mass_delete_threshold() {
    let (_g, home) = fake_home();
    write(&home.join(".Trash/one.bin"), b"x");
    for i in 0..swept_core::plan::MASS_DELETE_COUNT {
        fs::create_dir_all(home.join(format!(".Trash/d{i}"))).unwrap();
    }
    let mut req = acknowledged(&home);
    assert!(
        req.confirm_mass_delete,
        "files and folders together cross it"
    );
    req.confirm_mass_delete = false;

    assert!(run(&home, &req).is_err());
    assert!(home.join(".Trash/d0").is_dir());
}

#[test]
fn a_large_trash_without_mass_delete_confirmation_is_refused() {
    let (_g, home) = fake_home();
    for i in 0..(swept_core::plan::MASS_DELETE_COUNT + 1) {
        write(&home.join(format!(".Trash/f{i}")), b"x");
    }
    let mut req = acknowledged(&home);
    assert!(
        req.confirm_mass_delete,
        "the preview says it needs confirming"
    );
    req.confirm_mass_delete = false;

    assert!(run(&home, &req).is_err());
    assert!(home.join(".Trash/f0").exists());
}

#[test]
fn without_full_disk_access_it_refuses_and_deletes_nothing() {
    let (_g, home) = fake_home();
    populate(&home);
    let req = acknowledged(&home);
    let t = home.join(".Trash");
    fs::set_permissions(&t, fs::Permissions::from_mode(0o000)).unwrap();

    let preview = trash_contents(&home);
    let result = run(&home, &req);
    fs::set_permissions(&t, fs::Permissions::from_mode(0o755)).unwrap();

    assert!(!preview.readable);
    let err = result.unwrap_err();
    assert!(err.contains("Full Disk Access"), "got: {err}");
    assert!(home.join(".Trash/a.bin").exists());
}

#[test]
fn zero_expected_is_refused() {
    let (_g, home) = fake_home();
    let req = EmptyTrashRequest {
        expected: Expected { count: 0, bytes: 0 },
        fingerprint: String::new(),
        acknowledged_unrecoverable: true,
        confirm_mass_delete: false,
    };
    assert!(run(&home, &req).is_err());
}

#[test]
fn a_folder_swapped_for_a_symlink_after_the_preview_deletes_nothing_outside() {
    let (_g, home) = fake_home();
    populate(&home);
    let req = acknowledged(&home);
    let target = home.join("Documents/dir");
    write(&target.join("keep.txt"), b"keep");
    fs::remove_dir_all(home.join(".Trash/folder")).unwrap();
    symlink(&target, home.join(".Trash/folder")).unwrap();

    let _ = run(&home, &req);

    assert_eq!(fs::read(target.join("keep.txt")).unwrap(), b"keep");
}

#[test]
fn trashed_repositories_are_left_and_reported() {
    let (_g, home) = fake_home();
    write(&home.join(".Trash/repo/.git/HEAD"), b"h");
    write(&home.join(".Trash/loose.bin"), b"x");

    let preview = trash_contents(&home);
    assert_eq!(preview.left_behind, 1);
    let s = run(&home, &acknowledged(&home)).unwrap();

    assert_eq!(s.files_deleted, 1);
    assert_eq!(s.left_behind, 1);
    assert!(home.join(".Trash/repo/.git/HEAD").exists());
}

#[test]
fn the_real_entry_point_writes_its_own_audit_log() {
    let (_g, home) = fake_home();
    populate(&home);
    let log = home.join("logs/audit.jsonl");
    fs::create_dir_all(log.parent().unwrap()).unwrap();

    let s = empty_trash_at(&home, &log, &acknowledged(&home), &sink(&home)).unwrap();

    assert_eq!(s.files_deleted, 3);
    assert!(fs::read_to_string(log).unwrap().contains("permanent"));
}
