//! `~/Library/Application Support` is readable, not grantable — unless this
//! run was attested.
//!
//! That directory is where an app keeps its own data: a password manager's
//! vault, a messaging database, the only copy of some app's documents. Large &
//! Old lists what is in it, because the picture of the disk should be true;
//! but acting on a row there needs a second, explicit acknowledgement for this
//! run, refused by default. Decided 2026-09-06 (ROADMAP.md, M2).
//!
//! SAFETY CONTRACT item 7: everything here runs against a throwaway tempdir.

use std::fs;
use std::path::{Path, PathBuf};

use swept_core::audit::AuditLog;
use swept_core::executor::DirSink;
use swept_gui_core::{
    dispose_selected_attested_with_sink, dispose_selected_with_sink, large_and_old, AppDataAttested,
};

fn fixture_home() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(dir.path()).unwrap();
    for d in ["Documents", "Downloads", "Library/Application Support"] {
        fs::create_dir_all(home.join(d)).unwrap();
    }
    (dir, home)
}

fn write_sized(path: &Path, bytes: u64) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, vec![0u8; bytes as usize]).unwrap();
}

fn audit_at(home: &Path) -> (PathBuf, AuditLog) {
    let p = home.join("audit.jsonl");
    let log = AuditLog::open(&p).unwrap();
    (p, log)
}

fn sink(home: &Path) -> DirSink {
    DirSink {
        trash_dir: home.join("test-trash"),
    }
}

fn s(p: &Path) -> String {
    p.display().to_string()
}

const ATTESTED: AppDataAttested = AppDataAttested { app_support: true };

fn vault(home: &Path) -> PathBuf {
    home.join("Library/Application Support/Vault/data.db")
}

#[test]
fn an_unattested_path_in_application_support_is_refused_and_recorded() {
    let (_g, home) = fixture_home();
    let f = vault(&home);
    write_sized(&f, 4096);

    let (audit_path, mut log) = audit_at(&home);
    let err = dispose_selected_attested_with_sink(
        &home,
        &[s(&f)],
        None,
        false,
        AppDataAttested::default(),
        &sink(&home),
        &mut log,
    )
    .expect_err("an unattested App Support path must be refused");

    assert!(err.contains("Application Support"), "{err}");
    assert!(f.exists(), "the file must survive a refusal");
    let log_text = fs::read_to_string(audit_path).unwrap();
    assert!(
        log_text.contains("Application Support"),
        "the refusal must be in the audit log: {log_text}"
    );
}

/// The un-attested entry point is the one every other caller uses, so its
/// default has to be the refusal.
#[test]
fn the_plain_entry_point_is_never_attested() {
    let (_g, home) = fixture_home();
    let f = vault(&home);
    write_sized(&f, 4096);

    let (_p, mut log) = audit_at(&home);
    let err = dispose_selected_with_sink(&home, &[s(&f)], None, false, &sink(&home), &mut log)
        .expect_err("refused without attestation");
    assert!(err.contains("Application Support"), "{err}");
    assert!(f.exists());
}

#[test]
fn an_attested_path_in_application_support_is_disposed_and_says_so() {
    let (_g, home) = fixture_home();
    let f = vault(&home);
    write_sized(&f, 4096);

    let (audit_path, mut log) = audit_at(&home);
    let summary = dispose_selected_attested_with_sink(
        &home,
        &[s(&f)],
        None,
        false,
        ATTESTED,
        &sink(&home),
        &mut log,
    )
    .unwrap();

    assert_eq!(summary.executed, 1);
    assert!(!f.exists());
    let log_text = fs::read_to_string(audit_path).unwrap();
    assert!(
        log_text.contains("app-support-attested"),
        "the audit log must name what authorised it: {log_text}"
    );
}

#[test]
fn one_unattested_path_refuses_the_whole_selection() {
    let (_g, home) = fixture_home();
    let ordinary = home.join("Downloads/big.iso");
    let private = vault(&home);
    write_sized(&ordinary, 4096);
    write_sized(&private, 4096);

    let (_p, mut log) = audit_at(&home);
    dispose_selected_with_sink(
        &home,
        &[s(&ordinary), s(&private)],
        None,
        false,
        &sink(&home),
        &mut log,
    )
    .expect_err("mixed selection refused whole");

    assert!(
        ordinary.exists(),
        "a partial run is never what was confirmed"
    );
    assert!(private.exists());
}

/// macOS volumes are case-insensitive, and canonicalization does not promise
/// to fold case. Another spelling of the same directory must not step round
/// the boundary — whichever check catches it.
#[test]
fn a_differently_cased_spelling_is_still_refused() {
    let (_g, home) = fixture_home();
    let f = vault(&home);
    write_sized(&f, 4096);
    let other = home.join("library/application support/Vault/data.db");

    let (_p, mut log) = audit_at(&home);
    dispose_selected_with_sink(&home, &[s(&other)], None, false, &sink(&home), &mut log)
        .expect_err("a case variant must be refused");
    assert!(f.exists());
}

/// A discovery root that is itself a symlink into Application Support: the
/// walk resolves it and offers canonical paths, which are then *inside* App
/// Support. Only the new predicate stands between that and a disposal.
#[test]
fn a_discovery_root_symlinked_into_application_support_is_still_gated() {
    let (_g, home) = fixture_home();
    let target = home.join("Library/Application Support/Foo");
    fs::create_dir_all(&target).unwrap();
    fs::remove_dir(home.join("Documents")).unwrap();
    std::os::unix::fs::symlink(&target, home.join("Documents")).unwrap();
    let f = target.join("data.db");
    write_sized(&f, 4096);

    let (_p, mut log) = audit_at(&home);
    let err = dispose_selected_with_sink(&home, &[s(&f)], None, false, &sink(&home), &mut log)
        .expect_err("refused without attestation");
    assert!(err.contains("Application Support"), "{err}");
    assert!(f.exists());
}

#[test]
fn a_missing_attestation_field_deserializes_as_not_attested() {
    let a: AppDataAttested = serde_json::from_str("{}").unwrap();
    assert!(!a.app_support);
}

/// Attestation widens exactly one thing. A browser's own data stays refused on
/// this screen, attested or not — its consequence is a sign-out, and this
/// screen cannot ask about that.
#[test]
fn attestation_never_opens_a_browsers_data() {
    let (_g, home) = fixture_home();
    let profile = home.join("Library/Application Support/Google/Chrome/Default");
    fs::create_dir_all(&profile).unwrap();
    fs::write(profile.join("Preferences"), b"{}").unwrap();
    let cookies = profile.join("Cookies");
    write_sized(&cookies, 4096);

    let (_p, mut log) = audit_at(&home);
    dispose_selected_attested_with_sink(
        &home,
        &[s(&cookies)],
        None,
        false,
        ATTESTED,
        &sink(&home),
        &mut log,
    )
    .expect_err("a browser's data is refused even when attested");
    assert!(cookies.exists());
}

/// The report says which rows need the attestation, so the UI does not have to
/// reimplement the predicate against a home directory it does not know.
#[test]
fn the_report_marks_application_support_rows() {
    let (_g, home) = fixture_home();
    write_sized(&vault(&home), 4096);
    write_sized(&home.join("Downloads/big.iso"), 4096);

    let dto = large_and_old(&home, 1024, None);
    let v: serde_json::Value = serde_json::to_value(&dto).unwrap();
    let items = v["items"].as_array().unwrap();
    assert_eq!(items.len(), 2, "{v}");
    for item in items {
        let path = item["path"].as_str().unwrap();
        let expect = path.contains("Application Support");
        assert_eq!(item["app_private"].as_bool(), Some(expect), "{path}");
    }
}

/// An app's data folder relocated by a symlink — common for large data such as
/// device backups. The walk lists the file under its canonical path, which is
/// no longer under Application Support, so a check on the canonical path alone
/// would miss it. The browser boundary had exactly this bug (F9).
#[test]
fn app_data_symlinked_out_of_application_support_is_still_gated() {
    let (_g, home) = fixture_home();
    let real = home.join("Documents/SomeAppData");
    let f = real.join("vault.db");
    write_sized(&f, 4096);
    std::os::unix::fs::symlink(&real, home.join("Library/Application Support/SomeApp")).unwrap();

    let (_p, mut log) = audit_at(&home);
    let err = dispose_selected_with_sink(&home, &[s(&f)], None, false, &sink(&home), &mut log)
        .expect_err("relocated app data still needs the attestation");
    assert!(err.contains("Application Support"), "{err}");
    assert!(f.exists());

    let dto = large_and_old(&home, 1024, None);
    let row = dto.items.iter().find(|i| i.path == s(&f)).expect("listed");
    assert!(row.app_private, "the report must mark it too");
}
