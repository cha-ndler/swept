//! Emptying the user Trash: the one permanent gesture the GUI offers.
//!
//! SAFETY CONTRACT item 7: every test builds a throwaway `$HOME` in a tempdir.
//! "The Trash" here is always `<tempdir>/.Trash`, never the real one, and the
//! sink's recoverable destination is a `fixture-bin` beside it.

use std::fs;
use std::io;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};

use swept_core::audit::AuditLog;
use swept_core::emptytrash;
use swept_core::executor::{execute, prune_emptied_trash_dirs, Consent, DirSink, Sink};
use swept_core::plan::{Disposal, PlannedPrune};

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

fn permanent() -> Consent {
    Consent {
        execute: true,
        allow_permanent: true,
        confirmed_mass_delete: true,
        ..Default::default()
    }
}

fn planned_paths(p: &emptytrash::EmptyTrashPlan) -> Vec<PathBuf> {
    p.files
        .actions
        .iter()
        .map(|a| a.path.as_path().to_path_buf())
        .collect()
}

fn pruned_paths(p: &emptytrash::EmptyTrashPlan) -> Vec<PathBuf> {
    p.prune.iter().map(|d| d.path().to_path_buf()).collect()
}

// --- Planner ---

#[test]
fn the_plan_covers_only_the_trash() {
    let (_g, home) = fake_home();
    write(&home.join("Library/Caches/app/c.bin"), b"c");
    write(&home.join("Library/Logs/l.log"), b"l");
    write(&home.join("Documents/precious.txt"), b"p");
    write(&home.join(".Trash/old.bin"), b"0123");
    write(&home.join(".Trash/folder/inner.txt"), b"56");

    let p = emptytrash::plan(&home);
    let paths = planned_paths(&p);

    assert_eq!(paths.len(), 2);
    assert!(paths.iter().all(|x| x.starts_with(home.join(".Trash"))));
    assert!(p
        .files
        .actions
        .iter()
        .all(|a| a.disposal == Disposal::Permanent && a.category == "trash-emptied"));
    assert_eq!(p.items, 2);
    assert_eq!(p.left_behind, 0);
    assert!(!p.unreadable_root);
}

#[test]
fn recent_and_tiny_files_are_included() {
    let (_g, home) = fake_home();
    write(&home.join(".Trash/just-now"), b"");
    let p = emptytrash::plan(&home);
    assert_eq!(p.files.count(), 1, "no age or size filter applies");
}

#[test]
fn folders_are_pruned_deepest_first_and_never_the_root() {
    let (_g, home) = fake_home();
    write(&home.join(".Trash/a/b/c/f"), b"x");
    fs::create_dir_all(home.join(".Trash/empty")).unwrap();

    let p = emptytrash::plan(&home);
    let pruned = pruned_paths(&p);
    let t = home.join(".Trash");

    assert!(
        !pruned.contains(&t),
        "the Trash folder itself is never removed"
    );
    assert_eq!(pruned.len(), 4);
    let pos = |x: &str| pruned.iter().position(|y| y == &t.join(x)).unwrap();
    assert!(pos("a/b/c") < pos("a/b"));
    assert!(pos("a/b") < pos("a"));
    assert!(pruned.contains(&t.join("empty")));
}

#[test]
fn a_folder_holding_a_symlink_is_left_whole_and_the_target_survives() {
    let (_g, home) = fake_home();
    let precious = home.join("Documents/precious.txt");
    write(&precious, b"keep me");
    write(&home.join(".Trash/linky/plain.txt"), b"x");
    symlink(&precious, home.join(".Trash/linky/link")).unwrap();
    write(&home.join(".Trash/other.bin"), b"y");

    let p = emptytrash::plan(&home);

    assert_eq!(planned_paths(&p), vec![home.join(".Trash/other.bin")]);
    assert!(pruned_paths(&p).is_empty());
    assert_eq!(p.left_behind, 1);
}

#[test]
fn a_top_level_symlink_is_left() {
    let (_g, home) = fake_home();
    let precious = home.join("Documents/precious.txt");
    write(&precious, b"keep me");
    symlink(&precious, home.join(".Trash/link")).unwrap();

    let p = emptytrash::plan(&home);
    assert_eq!(p.files.count(), 0);
    assert_eq!(p.left_behind, 1);
}

#[test]
fn a_trashed_repository_is_left_whole() {
    let (_g, home) = fake_home();
    write(&home.join(".Trash/repo/README"), b"r");
    write(&home.join(".Trash/repo/.git/HEAD"), b"h");
    write(&home.join(".Trash/sibling.bin"), b"s");

    let p = emptytrash::plan(&home);
    let repo = home.join(".Trash/repo");

    assert!(planned_paths(&p).iter().all(|x| !x.starts_with(&repo)));
    assert!(pruned_paths(&p).iter().all(|x| !x.starts_with(&repo)));
    assert_eq!(p.left_behind, 1);
    assert_eq!(planned_paths(&p), vec![home.join(".Trash/sibling.bin")]);
}

#[test]
fn an_empty_git_folder_leaves_its_whole_item() {
    let (_g, home) = fake_home();
    fs::create_dir_all(home.join(".Trash/repo/.git")).unwrap();
    write(&home.join(".Trash/repo/README"), b"r");

    let p = emptytrash::plan(&home);
    assert_eq!(p.files.count(), 0);
    assert!(p.prune.is_empty());
    assert_eq!(p.left_behind, 1);
}

#[test]
fn an_unreadable_subfolder_leaves_its_whole_item() {
    let (_g, home) = fake_home();
    write(&home.join(".Trash/item/ok.txt"), b"x");
    let locked = home.join(".Trash/item/locked");
    write(&locked.join("hidden.txt"), b"y");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

    let p = emptytrash::plan(&home);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(p.files.count(), 0);
    assert!(p.prune.is_empty());
    assert_eq!(p.left_behind, 1);
}

#[test]
fn a_trash_that_is_a_symlink_plans_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(dir.path()).unwrap();
    write(&home.join("Documents/precious.txt"), b"keep me");
    symlink(home.join("Documents"), home.join(".Trash")).unwrap();

    let p = emptytrash::plan(&home);
    assert_eq!(p.files.count(), 0);
    assert!(p.prune.is_empty());
    assert!(home.join("Documents/precious.txt").exists());
}

#[test]
fn an_unreadable_trash_root_is_reported_not_empty() {
    let (_g, home) = fake_home();
    write(&home.join(".Trash/old.bin"), b"x");
    let t = home.join(".Trash");
    fs::set_permissions(&t, fs::Permissions::from_mode(0o000)).unwrap();

    let p = emptytrash::plan(&home);
    fs::set_permissions(&t, fs::Permissions::from_mode(0o755)).unwrap();

    assert!(p.unreadable_root);
    assert_eq!(p.files.count(), 0);
}

#[test]
fn a_missing_trash_is_simply_empty() {
    let dir = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(dir.path()).unwrap();
    let p = emptytrash::plan(&home);
    assert!(!p.unreadable_root);
    assert_eq!(p.items, 0);
}

#[test]
fn a_prune_cannot_name_the_trash_root_or_a_folder_outside_it() {
    let (_g, home) = fake_home();
    fs::create_dir_all(home.join("Library/Caches/empty")).unwrap();
    fs::create_dir_all(home.join("Documents/dir")).unwrap();
    symlink(home.join("Documents/dir"), home.join(".Trash/sneaky")).unwrap();
    fs::create_dir_all(home.join(".Trash/fine")).unwrap();

    assert!(PlannedPrune::new(&home.join(".Trash"), &home).is_err());
    assert!(PlannedPrune::new(&home.join("Library/Caches/empty"), &home).is_err());
    assert!(PlannedPrune::new(&home.join(".Trash/sneaky"), &home).is_err());
    assert!(PlannedPrune::new(&home.join(".Trash/missing"), &home).is_err());
    assert!(PlannedPrune::new(&home.join(".Trash/fine"), &home).is_ok());
}

// --- Pruning ---

fn audit(home: &Path) -> (PathBuf, AuditLog) {
    let p = home.join("audit.jsonl");
    let log = AuditLog::open(&p).unwrap();
    (p, log)
}

#[test]
fn files_then_folders_leave_an_empty_trash() {
    let (_g, home) = fake_home();
    write(&home.join(".Trash/a/b/f1"), b"12");
    write(&home.join(".Trash/g"), b"345");
    let p = emptytrash::plan(&home);
    let (log_path, mut log) = audit(&home);

    let files = execute(&p.files, permanent(), &home, &sink(&home), &mut log).unwrap();
    let dirs =
        prune_emptied_trash_dirs(&p.prune, &permanent(), &home, &sink(&home), &mut log).unwrap();

    assert_eq!(files.executed, 2);
    assert_eq!(files.bytes_executed, 5);
    assert_eq!(dirs.removed, 2);
    assert_eq!(dirs.refused, 0);
    assert!(
        home.join(".Trash").is_dir(),
        "the Trash folder itself stays"
    );
    assert_eq!(fs::read_dir(home.join(".Trash")).unwrap().count(), 0);
    assert!(!home.join("fixture-bin").exists(), "nothing was moved");
    let text = fs::read_to_string(log_path).unwrap();
    let g = home.join(".Trash/g").display().to_string();
    let file_line = text.lines().find(|l| l.contains(&g)).unwrap();
    assert!(file_line.contains("\"disposition\":\"permanent\""));
    assert!(file_line.contains("[trash-emptied]"), "got: {file_line}");
    assert!(text
        .lines()
        .any(|l| l.contains("empty folder left by emptying")));
}

#[test]
fn pruning_needs_allow_permanent() {
    let (_g, home) = fake_home();
    fs::create_dir_all(home.join(".Trash/empty")).unwrap();
    let p = emptytrash::plan(&home);
    let (log_path, mut log) = audit(&home);

    let r = prune_emptied_trash_dirs(
        &p.prune,
        &Consent {
            execute: true,
            ..Default::default()
        },
        &home,
        &sink(&home),
        &mut log,
    )
    .unwrap();

    assert_eq!(r.removed, 0);
    assert_eq!(r.refused, 1);
    assert!(home.join(".Trash/empty").is_dir());
    assert!(fs::read_to_string(log_path)
        .unwrap()
        .contains("\"disposition\":\"refused\""));
}

#[test]
fn a_prune_preview_changes_nothing() {
    let (_g, home) = fake_home();
    fs::create_dir_all(home.join(".Trash/empty")).unwrap();
    let p = emptytrash::plan(&home);
    let (log_path, mut log) = audit(&home);

    let r = prune_emptied_trash_dirs(&p.prune, &Consent::default(), &home, &sink(&home), &mut log)
        .unwrap();

    assert!(r.dry_run);
    assert_eq!(r.planned, 1);
    assert_eq!(r.removed, 0);
    assert!(home.join(".Trash/empty").is_dir());
    assert!(fs::read_to_string(log_path)
        .unwrap()
        .contains("\"phase\":\"planned\""));
}

#[test]
fn a_folder_that_gained_a_file_after_planning_survives_with_its_file() {
    let (_g, home) = fake_home();
    fs::create_dir_all(home.join(".Trash/d")).unwrap();
    let p = emptytrash::plan(&home);
    let late = home.join(".Trash/d/late.txt");
    write(&late, b"arrived after the preview");
    let (_lp, mut log) = audit(&home);

    let r =
        prune_emptied_trash_dirs(&p.prune, &permanent(), &home, &sink(&home), &mut log).unwrap();

    assert_eq!(r.removed, 0);
    assert_eq!(r.refused, 1);
    assert!(late.exists());
}

#[test]
fn a_folder_swapped_for_a_symlink_after_planning_is_refused_and_the_target_survives() {
    let (_g, home) = fake_home();
    fs::create_dir_all(home.join(".Trash/d")).unwrap();
    let p = emptytrash::plan(&home);

    let target = home.join("Documents/dir");
    fs::create_dir_all(&target).unwrap();
    fs::remove_dir(home.join(".Trash/d")).unwrap();
    symlink(&target, home.join(".Trash/d")).unwrap();
    let (_lp, mut log) = audit(&home);

    let r =
        prune_emptied_trash_dirs(&p.prune, &permanent(), &home, &sink(&home), &mut log).unwrap();

    assert_eq!(r.removed, 0);
    assert_eq!(r.refused, 1);
    assert!(target.is_dir(), "the symlink's target is untouched");
}

/// A sink whose folder removal always fails, to observe the audit ordering.
struct FailingPrune;

impl Sink for FailingPrune {
    fn trash(&self, _: &Path) -> io::Result<()> {
        Err(io::Error::other("not in this test"))
    }
    fn delete(&self, _: &Path) -> io::Result<()> {
        Err(io::Error::other("not in this test"))
    }
    fn remove_empty_dir(&self, _: &Path) -> io::Result<()> {
        Err(io::Error::other("injected failure"))
    }
}

#[test]
fn each_prune_is_recorded_as_permanent_before_it_happens() {
    let (_g, home) = fake_home();
    fs::create_dir_all(home.join(".Trash/d")).unwrap();
    let p = emptytrash::plan(&home);
    let (log_path, mut log) = audit(&home);

    let r =
        prune_emptied_trash_dirs(&p.prune, &permanent(), &home, &FailingPrune, &mut log).unwrap();

    assert_eq!(r.refused, 1);
    assert!(home.join(".Trash/d").is_dir());
    let text = fs::read_to_string(log_path).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("\"disposition\":\"permanent\""));
    assert!(lines[1].contains("\"disposition\":\"refused\""));
}

/// Deletes for real, but when it deletes `trigger` it first swaps `swap` — a
/// folder in the Trash — for a symlink to `target`, as a hostile or unlucky
/// process might between two unlinks.
struct SwapOnDelete {
    trigger: PathBuf,
    swap: PathBuf,
    target: PathBuf,
}

impl Sink for SwapOnDelete {
    fn trash(&self, _: &Path) -> io::Result<()> {
        Err(io::Error::other("not in this test"))
    }
    fn delete(&self, path: &Path) -> io::Result<()> {
        if path == self.trigger {
            fs::rename(&self.swap, self.swap.with_extension("fixture-moved"))?;
            symlink(&self.target, &self.swap)?;
        }
        fs::remove_file(path)
    }
}

#[test]
fn a_permanent_unlink_is_bound_to_the_planned_path() {
    let (_g, home) = fake_home();
    write(&home.join(".Trash/a.bin"), b"a");
    write(&home.join(".Trash/sub/x.bin"), b"x");
    let victim = home.join("Library/Caches/victim/x.bin");
    write(&victim, b"not in the Trash");

    let mut p = emptytrash::plan(&home);
    // a.bin first, so the swap lands between its unlink and sub/x.bin's.
    p.files
        .actions
        .sort_by_key(|a| a.path.as_path().ends_with("x.bin"));
    let s = SwapOnDelete {
        trigger: home.join(".Trash/a.bin"),
        swap: home.join(".Trash/sub"),
        target: home.join("Library/Caches/victim"),
    };
    let (_lp, mut log) = audit(&home);

    let r = execute(&p.files, permanent(), &home, &s, &mut log).unwrap();

    assert_eq!(r.executed, 1);
    assert_eq!(r.refused, 1);
    assert_eq!(fs::read(&victim).unwrap(), b"not in the Trash");
}

/// A sink that does not override folder removal.
struct NoPrune;

impl Sink for NoPrune {
    fn trash(&self, _: &Path) -> io::Result<()> {
        Ok(())
    }
    fn delete(&self, _: &Path) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_sink_without_the_override_cannot_remove_folders() {
    let (_g, home) = fake_home();
    let d = home.join(".Trash/d");
    fs::create_dir_all(&d).unwrap();
    let err = NoPrune.remove_empty_dir(&d).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::Unsupported);
    assert!(d.is_dir());
}
