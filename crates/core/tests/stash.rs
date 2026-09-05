//! Moving a login item aside, and putting it back.
//!
//! This is the first mutation in the codebase that is **neither a trash nor a
//! delete**, and the safety argument is correspondingly different. Item 4 of
//! the contract — "Trash, not unlink" — has no direct analogue, so it is
//! replaced by a stronger one:
//!
//! > The only `remove_file` here is of a name that provably shares an inode
//! > with a second name created moments before. This module cannot lose a
//! > file's bytes.
//!
//! That is why the primitive is `hard_link` → verify `(dev, ino)` →
//! `remove_file`, and never `rename` (which clobbers silently) or copy-then-
//! remove (which has a window where the only good copy is a partial one).
//! Every failure lands on either "nothing happened" or "two names for one
//! file" — never "no names".
//!
//! The other half is reversibility. Putting an item back needs **no recorded
//! state**: the destination is the store's own parent, so no manifest has to
//! remember a path and no file's *contents* ever name a destination — the
//! invariant `uninstall` and `privacy` both hold.
//!
//! SAFETY CONTRACT item 7: everything here runs against a throwaway tempdir.

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use swept_core::audit::AuditLog;
use swept_core::executor::{
    restore, stash, StashConsent, StashError, SystemStashSink, MAX_STARTUP_GRANTS,
};
use swept_core::loginitems::{store_dir, STORE_NOTE_NAME};
use swept_core::plan::{PlannedMove, StashPlan};

// --- fixtures --------------------------------------------------------------

fn fixture() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(dir.path()).unwrap();
    fs::create_dir_all(home.join("Library/LaunchAgents")).unwrap();
    (dir, home)
}

fn agents(home: &Path) -> PathBuf {
    home.join("Library/LaunchAgents")
}

fn agent(home: &Path, name: &str) -> PathBuf {
    let p = agents(home).join(format!("{name}.plist"));
    fs::write(
        &p,
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict><key>Label</key><string>{name}</string></dict></plist>"#
        ),
    )
    .unwrap();
    p
}

fn audit(home: &Path) -> AuditLog {
    AuditLog::open(&home.join("audit.jsonl")).unwrap()
}

fn log(home: &Path) -> String {
    fs::read_to_string(home.join("audit.jsonl")).unwrap_or_default()
}

/// A plan naming `paths`, with the grants that authorize exactly them.
fn plan_for(home: &Path, paths: &[PathBuf]) -> (StashPlan, StashConsent) {
    let mut moves = Vec::new();
    let mut granted = Vec::new();
    for p in paths {
        let size = fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        let Ok(m) = PlannedMove::new(p.clone(), home, size, "startup".to_string()) else {
            continue;
        };
        granted.push(m.path().clone());
        moves.push(m);
    }
    (
        StashPlan { moves },
        StashConsent {
            execute: true,
            granted,
        },
    )
}

fn ino(path: &Path) -> (u64, u64) {
    let m = fs::symlink_metadata(path).unwrap();
    (m.dev(), m.ino())
}

// --- dry run ---------------------------------------------------------------

#[test]
fn a_dry_run_moves_nothing_and_records_what_it_would_do() {
    let (_d, home) = fixture();
    let p = agent(&home, "com.acme.helper");
    let (plan, mut consent) = plan_for(&home, std::slice::from_ref(&p));
    consent.execute = false;

    let report = stash(
        &plan,
        consent,
        &store_dir(&home),
        &home,
        &SystemStashSink,
        &mut audit(&home),
    )
    .unwrap();

    assert!(report.dry_run);
    assert_eq!(report.planned, 1);
    assert_eq!(report.moved, 0);
    assert!(p.exists(), "a preview moves nothing");
    assert!(!store_dir(&home).exists(), "and creates nothing");
    assert!(log(&home).contains("planned"));
}

/// The preview must refuse exactly what the real run would. A preview that
/// succeeds where the run refuses is a preview that lies.
#[test]
fn a_preview_refuses_exactly_what_the_real_run_would_refuse() {
    let (_d, home) = fixture();
    let outside = home.join("Documents/com.acme.helper.plist");
    fs::create_dir_all(outside.parent().unwrap()).unwrap();
    fs::write(&outside, b"x").unwrap();
    let (plan, mut consent) = plan_for(&home, std::slice::from_ref(&outside));
    consent.execute = false;

    let preview = stash(
        &plan,
        consent,
        &store_dir(&home),
        &home,
        &SystemStashSink,
        &mut audit(&home),
    )
    .unwrap();
    let (plan, consent) = plan_for(&home, std::slice::from_ref(&outside));
    let real = stash(
        &plan,
        consent,
        &store_dir(&home),
        &home,
        &SystemStashSink,
        &mut audit(&home),
    )
    .unwrap();

    assert_eq!(preview.planned, 0);
    assert_eq!(preview.refused, 1);
    assert_eq!(real.moved, 0);
    assert_eq!(real.refused, 1);
    assert!(outside.exists());
}

// --- the happy path, stated as a safety property ---------------------------

#[test]
fn moving_a_plist_aside_leaves_the_file_byte_identical_in_the_store() {
    let (_d, home) = fixture();
    let p = agent(&home, "com.acme.helper");
    let before = fs::read(&p).unwrap();
    let identity = ino(&p);
    let (plan, consent) = plan_for(&home, std::slice::from_ref(&p));

    let report = stash(
        &plan,
        consent,
        &store_dir(&home),
        &home,
        &SystemStashSink,
        &mut audit(&home),
    )
    .unwrap();

    assert_eq!(report.moved, 1);
    assert!(!p.exists(), "it no longer starts at login");
    let moved = store_dir(&home).join("com.acme.helper.plist");
    assert!(moved.is_file());
    assert_eq!(fs::read(&moved).unwrap(), before);
    assert_eq!(ino(&moved), identity, "the same file, not a copy of it");
}

#[test]
fn the_store_explains_itself_to_someone_who_no_longer_has_this_app() {
    let (_d, home) = fixture();
    let p = agent(&home, "com.acme.helper");
    let (plan, consent) = plan_for(&home, &[p]);
    stash(
        &plan,
        consent,
        &store_dir(&home),
        &home,
        &SystemStashSink,
        &mut audit(&home),
    )
    .unwrap();

    let note = store_dir(&home).join(STORE_NOTE_NAME);
    let text = fs::read_to_string(&note).unwrap();
    assert!(
        text.contains("one level"),
        "it says how to undo this by hand"
    );
    assert!(text.contains("log"), "and that it takes effect at login");
}

#[test]
fn a_round_trip_leaves_the_plist_exactly_where_and_as_it_started() {
    let (_d, home) = fixture();
    let p = agent(&home, "com.acme.helper");
    let before = fs::read(&p).unwrap();
    let identity = ino(&p);

    let (plan, consent) = plan_for(&home, std::slice::from_ref(&p));
    stash(
        &plan,
        consent,
        &store_dir(&home),
        &home,
        &SystemStashSink,
        &mut audit(&home),
    )
    .unwrap();

    let moved = store_dir(&home).join("com.acme.helper.plist");
    let (plan, consent) = plan_for(&home, std::slice::from_ref(&moved));
    let report = restore(
        &plan,
        consent,
        &store_dir(&home),
        &home,
        &SystemStashSink,
        &mut audit(&home),
    )
    .unwrap();

    assert_eq!(report.moved, 1);
    assert!(p.is_file());
    assert!(!moved.exists());
    assert_eq!(fs::read(&p).unwrap(), before);
    assert_eq!(ino(&p), identity);
}

// --- the load-bearing refusals ---------------------------------------------

/// The attack this module exists to refuse: a symlink named like a plist,
/// pointing at something precious. `guard` canonicalizes, so it would resolve
/// to the *target*; the "must already be its own canonical spelling" check is
/// what catches it.
#[test]
fn a_symlink_named_like_a_plist_pointing_at_the_keychain_is_refused() {
    let (_d, home) = fixture();
    let precious = home.join("Library/Keychains/login.keychain-db");
    fs::create_dir_all(precious.parent().unwrap()).unwrap();
    fs::write(&precious, b"secrets").unwrap();
    let link = agents(&home).join("com.acme.evil.plist");
    std::os::unix::fs::symlink(&precious, &link).unwrap();

    let mut moves = Vec::new();
    let mut granted = Vec::new();
    if let Ok(m) = PlannedMove::new(link.clone(), &home, 0, "startup".to_string()) {
        granted.push(m.path().clone());
        moves.push(m);
    }
    let report = stash(
        &StashPlan { moves },
        StashConsent {
            execute: true,
            granted,
        },
        &store_dir(&home),
        &home,
        &SystemStashSink,
        &mut audit(&home),
    )
    .unwrap();

    assert_eq!(report.moved, 0);
    assert!(precious.is_file(), "the keychain is untouched");
    assert!(fs::symlink_metadata(&link).is_ok(), "and so is the link");
}

#[test]
fn a_plist_whose_parent_is_not_the_launch_agents_directory_is_refused() {
    let (_d, home) = fixture();
    let elsewhere = home.join("Library/Caches/com.acme.helper.plist");
    fs::create_dir_all(elsewhere.parent().unwrap()).unwrap();
    fs::write(&elsewhere, b"x").unwrap();
    let (plan, consent) = plan_for(&home, std::slice::from_ref(&elsewhere));

    let report = stash(
        &plan,
        consent,
        &store_dir(&home),
        &home,
        &SystemStashSink,
        &mut audit(&home),
    )
    .unwrap();

    assert_eq!(report.moved, 0);
    assert_eq!(report.refused, 1);
    assert!(elsewhere.exists());
}

#[test]
fn a_directory_named_like_a_plist_is_refused() {
    let (_d, home) = fixture();
    let dir = agents(&home).join("com.acme.helper.plist");
    fs::create_dir_all(dir.join("inside")).unwrap();
    let (plan, consent) = plan_for(&home, std::slice::from_ref(&dir));

    let report = stash(
        &plan,
        consent,
        &store_dir(&home),
        &home,
        &SystemStashSink,
        &mut audit(&home),
    )
    .unwrap();

    assert_eq!(report.moved, 0);
    assert!(dir.join("inside").exists());
}

/// `hard_link` fails with `EEXIST` and creates nothing, which is the check —
/// there is deliberately no "does the destination exist?" test followed by a
/// write, because that pattern invites someone to later swap the atomic
/// primitive for a racy one.
#[test]
fn a_name_already_in_the_store_is_refused_and_neither_file_changes() {
    let (_d, home) = fixture();
    let p = agent(&home, "com.acme.helper");
    let store = store_dir(&home);
    fs::create_dir_all(&store).unwrap();
    let squatter = store.join("com.acme.helper.plist");
    fs::write(&squatter, b"a different file entirely").unwrap();

    let (plan, consent) = plan_for(&home, std::slice::from_ref(&p));
    let report = stash(
        &plan,
        consent,
        &store,
        &home,
        &SystemStashSink,
        &mut audit(&home),
    )
    .unwrap();

    assert_eq!(report.moved, 0);
    assert!(p.exists(), "the original is untouched");
    assert_eq!(
        fs::read(&squatter).unwrap(),
        b"a different file entirely",
        "and so is what was already there"
    );
}

#[test]
fn putting_something_back_never_overwrites_a_file_that_returned_on_its_own() {
    let (_d, home) = fixture();
    let p = agent(&home, "com.acme.helper");
    let (plan, consent) = plan_for(&home, std::slice::from_ref(&p));
    stash(
        &plan,
        consent,
        &store_dir(&home),
        &home,
        &SystemStashSink,
        &mut audit(&home),
    )
    .unwrap();

    // The user reinstalled the app, so a fresh plist is back under the name.
    fs::write(&p, b"the new one").unwrap();

    let moved = store_dir(&home).join("com.acme.helper.plist");
    let (plan, consent) = plan_for(&home, std::slice::from_ref(&moved));
    let report = restore(
        &plan,
        consent,
        &store_dir(&home),
        &home,
        &SystemStashSink,
        &mut audit(&home),
    )
    .unwrap();

    assert_eq!(report.moved, 0);
    assert_eq!(fs::read(&p).unwrap(), b"the new one");
    assert!(moved.exists(), "and the moved-aside copy is still there");
}

#[test]
fn a_symlink_dropped_into_the_store_is_never_put_back() {
    let (_d, home) = fixture();
    let store = store_dir(&home);
    fs::create_dir_all(&store).unwrap();
    let precious = home.join("Library/Keychains/login.keychain-db");
    fs::create_dir_all(precious.parent().unwrap()).unwrap();
    fs::write(&precious, b"secrets").unwrap();
    let link = store.join("com.acme.evil.plist");
    std::os::unix::fs::symlink(&precious, &link).unwrap();

    let mut moves = Vec::new();
    let mut granted = Vec::new();
    if let Ok(m) = PlannedMove::new(link.clone(), &home, 0, "startup".to_string()) {
        granted.push(m.path().clone());
        moves.push(m);
    }
    let report = restore(
        &StashPlan { moves },
        StashConsent {
            execute: true,
            granted,
        },
        &store,
        &home,
        &SystemStashSink,
        &mut audit(&home),
    )
    .unwrap();

    assert_eq!(report.moved, 0);
    assert!(precious.is_file());
    assert!(!agents(&home).join("com.acme.evil.plist").exists());
}

// --- whole-run, fail-closed ------------------------------------------------

#[test]
fn a_store_that_is_a_symlink_refuses_the_whole_run() {
    let (_d, home) = fixture();
    let p = agent(&home, "com.acme.helper");
    let elsewhere = home.join("Documents/somewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, store_dir(&home)).unwrap();

    let (plan, consent) = plan_for(&home, std::slice::from_ref(&p));
    let err = stash(
        &plan,
        consent,
        &store_dir(&home),
        &home,
        &SystemStashSink,
        &mut audit(&home),
    )
    .unwrap_err();

    assert!(matches!(err, StashError::Store(_)));
    assert!(p.exists());
    assert!(fs::read_dir(&elsewhere).unwrap().next().is_none());
}

#[test]
fn a_store_that_is_not_a_directory_refuses_the_whole_run() {
    let (_d, home) = fixture();
    let p = agent(&home, "com.acme.helper");
    fs::write(store_dir(&home), b"not a directory").unwrap();

    let (plan, consent) = plan_for(&home, std::slice::from_ref(&p));
    let err = stash(
        &plan,
        consent,
        &store_dir(&home),
        &home,
        &SystemStashSink,
        &mut audit(&home),
    )
    .unwrap_err();

    assert!(matches!(err, StashError::Store(_)));
    assert!(p.exists());
}

/// The store's whole design rests on its parent being the LaunchAgents folder:
/// that is what makes putting an item back need no recorded state.
#[test]
fn a_store_whose_parent_is_not_launch_agents_refuses_the_whole_run() {
    let (_d, home) = fixture();
    let p = agent(&home, "com.acme.helper");
    let bogus = home.join("Documents/Moved aside by Swept");
    fs::create_dir_all(&bogus).unwrap();

    let (plan, consent) = plan_for(&home, std::slice::from_ref(&p));
    let err = stash(
        &plan,
        consent,
        &bogus,
        &home,
        &SystemStashSink,
        &mut audit(&home),
    )
    .unwrap_err();

    assert!(matches!(err, StashError::Store(_)));
    assert!(p.exists());
    assert!(fs::read_dir(&bogus).unwrap().next().is_none());
}

#[test]
fn a_non_canonical_home_refuses_the_whole_run() {
    let (_d, home) = fixture();
    let p = agent(&home, "com.acme.helper");
    let (plan, consent) = plan_for(&home, std::slice::from_ref(&p));
    let bent = home.join("Library").join("..");

    let err = stash(
        &plan,
        consent,
        &store_dir(&home),
        &bent,
        &SystemStashSink,
        &mut audit(&home),
    )
    .unwrap_err();

    assert!(matches!(err, StashError::Home));
    assert!(p.exists());
}

#[test]
fn more_grants_than_the_cap_refuses_the_whole_run_rather_than_truncating() {
    let (_d, home) = fixture();
    let p = agent(&home, "com.acme.helper");
    let (plan, mut consent) = plan_for(&home, std::slice::from_ref(&p));
    let one = consent.granted[0].clone();
    while consent.granted.len() <= MAX_STARTUP_GRANTS {
        consent.granted.push(one.clone());
    }

    let err = stash(
        &plan,
        consent,
        &store_dir(&home),
        &home,
        &SystemStashSink,
        &mut audit(&home),
    )
    .unwrap_err();

    assert!(matches!(err, StashError::TooManyGrants { .. }));
    assert!(p.exists());
}

/// A path nobody granted is not moved, however well-formed the plan is.
#[test]
fn a_plan_entry_nobody_granted_is_refused() {
    let (_d, home) = fixture();
    let p = agent(&home, "com.acme.helper");
    let (plan, mut consent) = plan_for(&home, std::slice::from_ref(&p));
    consent.granted.clear();

    let report = stash(
        &plan,
        consent,
        &store_dir(&home),
        &home,
        &SystemStashSink,
        &mut audit(&home),
    )
    .unwrap();

    assert_eq!(report.moved, 0);
    assert_eq!(report.refused, 1);
    assert!(p.exists());
}

// --- the new window --------------------------------------------------------

/// The window that exists nowhere else in this codebase: between creating the
/// second link and removing the first. If the source is no longer the file we
/// linked, we must not unlink it.
#[test]
fn a_source_replaced_between_the_link_and_the_unlink_is_never_unlinked() {
    use std::io;
    use swept_core::executor::StashSink;

    /// Swaps the source for a different file after the link is made.
    struct Swapper {
        source: PathBuf,
    }
    impl StashSink for Swapper {
        fn link(&self, from: &Path, to: &Path) -> io::Result<()> {
            std::fs::hard_link(from, to)?;
            // Something replaces the original the instant the link exists.
            fs::remove_file(&self.source)?;
            fs::write(&self.source, b"a different file entirely").unwrap();
            Ok(())
        }
        fn unlink(&self, path: &Path) -> io::Result<()> {
            fs::remove_file(path)
        }
    }

    let (_d, home) = fixture();
    let p = agent(&home, "com.acme.helper");
    let (plan, consent) = plan_for(&home, std::slice::from_ref(&p));

    let report = stash(
        &plan,
        consent,
        &store_dir(&home),
        &home,
        &Swapper { source: p.clone() },
        &mut audit(&home),
    )
    .unwrap();

    assert_eq!(report.moved, 0, "the identity check refused the unlink");
    assert_eq!(
        fs::read(&p).unwrap(),
        b"a different file entirely",
        "and the thing that replaced it is still there"
    );
}

#[test]
fn a_failure_to_create_the_link_leaves_the_original_exactly_where_it_was() {
    use std::io;
    use swept_core::executor::StashSink;

    struct NoLink;
    impl StashSink for NoLink {
        fn link(&self, _from: &Path, _to: &Path) -> io::Result<()> {
            Err(io::Error::other("no"))
        }
        fn unlink(&self, path: &Path) -> io::Result<()> {
            fs::remove_file(path)
        }
    }

    let (_d, home) = fixture();
    let p = agent(&home, "com.acme.helper");
    let before = fs::read(&p).unwrap();
    let (plan, consent) = plan_for(&home, std::slice::from_ref(&p));

    let report = stash(
        &plan,
        consent,
        &store_dir(&home),
        &home,
        &NoLink,
        &mut audit(&home),
    )
    .unwrap();

    assert_eq!(report.moved, 0);
    assert_eq!(fs::read(&p).unwrap(), before);
}

/// Both names exist and the item still runs at login. Two lines are written and
/// both are true: the copy was made, and the original was not removed.
#[test]
fn a_failure_to_unlink_the_original_leaves_both_copies_and_records_both_truths() {
    use std::io;
    use swept_core::executor::StashSink;

    struct NoUnlink;
    impl StashSink for NoUnlink {
        fn link(&self, from: &Path, to: &Path) -> io::Result<()> {
            std::fs::hard_link(from, to)
        }
        fn unlink(&self, _path: &Path) -> io::Result<()> {
            Err(io::Error::other("no"))
        }
    }

    let (_d, home) = fixture();
    let p = agent(&home, "com.acme.helper");
    let (plan, consent) = plan_for(&home, std::slice::from_ref(&p));

    let report = stash(
        &plan,
        consent,
        &store_dir(&home),
        &home,
        &NoUnlink,
        &mut audit(&home),
    )
    .unwrap();

    assert_eq!(report.moved, 0);
    assert_eq!(report.refused, 1);
    assert!(p.exists(), "it still runs at login, and the log says so");
    assert!(store_dir(&home).join("com.acme.helper.plist").exists());
    let text = log(&home);
    assert!(text.contains("stashed"));
    assert!(text.contains("refused"));
}

// --- the type-level separation ---------------------------------------------

/// The property the separate types buy: a grant that authorizes moving a plist
/// aside is inert in the disposal executor, because it is a different field of
/// a different struct that `execute` never sees.
#[test]
fn a_grant_to_move_a_plist_aside_cannot_dispose_of_it() {
    use swept_core::executor::{execute, Consent, DirSink};
    use swept_core::plan::{Disposal, Plan, PlannedAction};

    let (_d, home) = fixture();
    let p = agent(&home, "com.acme.helper");
    let safe = safety::guard(&p, &home).unwrap();

    // The disposal executor, handed the same path with a default consent: the
    // stash grant lives on `StashConsent` and has no way to reach here.
    let plan = Plan {
        actions: vec![PlannedAction {
            path: safe,
            size_bytes: 1,
            disposal: Disposal::Trash,
            category: "startup".to_string(),
        }],
        dirs: Vec::new(),
        skipped_protected: 0,
        skipped_unreadable: 0,
    };
    let report = execute(
        &plan,
        Consent {
            execute: true,
            allow_permanent: false,
            confirmed_mass_delete: true,
            granted: Vec::new(),
            granted_dirs: Vec::new(),
        },
        &home,
        &DirSink {
            trash_dir: home.join("FixtureTrash"),
        },
        &mut audit(&home),
    )
    .unwrap();

    assert_eq!(
        report.executed, 0,
        "LaunchAgents is not in the disposal scope"
    );
    assert!(p.exists());
}

// --- the log ---------------------------------------------------------------

#[test]
fn the_audit_log_names_the_original_path_and_where_the_file_went() {
    let (_d, home) = fixture();
    let p = agent(&home, "com.acme.helper");
    let (plan, consent) = plan_for(&home, std::slice::from_ref(&p));
    stash(
        &plan,
        consent,
        &store_dir(&home),
        &home,
        &SystemStashSink,
        &mut audit(&home),
    )
    .unwrap();

    let text = log(&home);
    assert!(text.contains(&p.display().to_string()));
    assert!(text.contains("Moved aside by Swept"));
    assert!(text.contains("startup"), "the category that authorized it");
}

/// Nothing here is a disposal, and the log must not say it was.
#[test]
fn a_moved_aside_item_is_never_recorded_as_trashed_or_permanent() {
    let (_d, home) = fixture();
    let p = agent(&home, "com.acme.helper");
    let (plan, consent) = plan_for(&home, &[p]);
    stash(
        &plan,
        consent,
        &store_dir(&home),
        &home,
        &SystemStashSink,
        &mut audit(&home),
    )
    .unwrap();

    let text = log(&home);
    assert!(text.contains("\"disposition\":\"stashed\""));
    assert!(!text.contains("\"disposition\":\"trash\""));
    assert!(!text.contains("\"disposition\":\"permanent\""));
}

// --- the layers a mutation survived -----------------------------------------
//
// Eight checks in this module were documented as load-bearing and pinned by
// nothing: deleting each left all 431 tests green. They are mutually redundant
// by design, which is exactly why none of them was reachable through a test
// that went in the front door — and a redundant layer nothing pins is how a
// layer quietly stops existing.

/// The nastier ordering, and the one the first version got wrong.
///
/// If the **destination** is replaced between the link and the check, the file
/// now sitting there belongs to somebody else and may have only that one name.
/// The old rollback removed it unconditionally, which is the precise outcome
/// this module claims it cannot produce.
#[test]
fn a_destination_taken_by_someone_else_is_never_removed() {
    use std::io;
    use swept_core::executor::StashSink;

    /// An installer writes a fresh plist over the destination the instant our
    /// link exists.
    struct Squatter {
        dest: PathBuf,
    }
    impl StashSink for Squatter {
        fn link(&self, from: &Path, to: &Path) -> io::Result<()> {
            std::fs::hard_link(from, to)?;
            fs::remove_file(&self.dest)?;
            fs::write(&self.dest, b"a freshly installed login item").unwrap();
            Ok(())
        }
        fn unlink(&self, path: &Path) -> io::Result<()> {
            fs::remove_file(path)
        }
    }

    let (_d, home) = fixture();
    let p = agent(&home, "com.acme.helper");
    let store = store_dir(&home);
    let dest = store.join("com.acme.helper.plist");
    let (plan, consent) = plan_for(&home, std::slice::from_ref(&p));

    let report = stash(
        &plan,
        consent,
        &store,
        &home,
        &Squatter { dest: dest.clone() },
        &mut audit(&home),
    )
    .unwrap();

    assert_eq!(report.moved, 0);
    assert!(p.exists(), "the source is untouched");
    assert_eq!(
        fs::read(&dest).unwrap(),
        b"a freshly installed login item",
        "and so is the file that took the name — it had only this one"
    );
}

/// A plist that was *already* a symlink is the case `guard` hides: it arrives
/// resolved to its target, which is not a link and is its own canonical
/// spelling, so every check downstream sees a perfectly ordinary file. Only the
/// listed spelling refuses it — and the target here is deliberately not
/// denylisted, so nothing else can be doing the work.
#[test]
fn a_plist_that_is_itself_a_symlink_moves_nothing() {
    let (_d, home) = fixture();
    let real = agent(&home, "com.important.backup");
    let decoy = agents(&home).join("com.acme.decoy.plist");
    std::os::unix::fs::symlink(&real, &decoy).unwrap();

    let (plan, consent) = plan_for(&home, std::slice::from_ref(&decoy));
    let report = stash(
        &plan,
        consent,
        &store_dir(&home),
        &home,
        &SystemStashSink,
        &mut audit(&home),
    )
    .unwrap();

    assert_eq!(report.moved, 0);
    assert!(
        real.is_file(),
        "the item the user did not tick is untouched"
    );
    assert!(fs::symlink_metadata(&decoy).is_ok());
    assert!(!store_dir(&home).join("com.important.backup.plist").exists());
}

#[test]
fn a_file_that_is_not_a_plist_is_refused() {
    let (_d, home) = fixture();
    let notes = agents(&home).join("notes.txt");
    fs::write(&notes, b"hello").unwrap();

    let (plan, consent) = plan_for(&home, std::slice::from_ref(&notes));
    let report = stash(
        &plan,
        consent,
        &store_dir(&home),
        &home,
        &SystemStashSink,
        &mut audit(&home),
    )
    .unwrap();

    assert_eq!(report.moved, 0);
    assert!(notes.exists());
}

/// Exactly this directory, never `starts_with`. launchd does not recurse into a
/// subfolder, so neither should the offer — and without the exact comparison,
/// anything nested under LaunchAgents would qualify.
#[test]
fn a_plist_in_a_subfolder_of_launch_agents_is_refused() {
    let (_d, home) = fixture();
    let nested = agents(&home).join("vendor/com.acme.helper.plist");
    fs::create_dir_all(nested.parent().unwrap()).unwrap();
    fs::write(&nested, b"x").unwrap();

    let (plan, consent) = plan_for(&home, std::slice::from_ref(&nested));
    let report = stash(
        &plan,
        consent,
        &store_dir(&home),
        &home,
        &SystemStashSink,
        &mut audit(&home),
    )
    .unwrap();

    assert_eq!(report.moved, 0);
    assert!(nested.exists());
}

/// The note is a courtesy the user may edit or replace. `create_new` is what
/// stops a later run overwriting what they wrote.
#[test]
fn the_note_never_overwrites_one_the_user_has_edited() {
    let (_d, home) = fixture();
    let first = agent(&home, "com.acme.one");
    let (plan, consent) = plan_for(&home, std::slice::from_ref(&first));
    stash(
        &plan,
        consent,
        &store_dir(&home),
        &home,
        &SystemStashSink,
        &mut audit(&home),
    )
    .unwrap();

    let note = store_dir(&home).join(STORE_NOTE_NAME);
    fs::write(&note, b"my own notes about these").unwrap();

    let second = agent(&home, "com.acme.two");
    let (plan, consent) = plan_for(&home, std::slice::from_ref(&second));
    stash(
        &plan,
        consent,
        &store_dir(&home),
        &home,
        &SystemStashSink,
        &mut audit(&home),
    )
    .unwrap();

    assert_eq!(fs::read(&note).unwrap(), b"my own notes about these");
}

/// The store is *the* folder, not merely one inside LaunchAgents — otherwise a
/// caller picks which of the user's own subfolders to act on.
#[test]
fn a_folder_that_is_not_the_store_refuses_the_whole_run() {
    let (_d, home) = fixture();
    let p = agent(&home, "com.acme.helper");
    let theirs = agents(&home).join("Some other folder the user made");
    fs::create_dir_all(&theirs).unwrap();

    let (plan, consent) = plan_for(&home, std::slice::from_ref(&p));
    let err = stash(
        &plan,
        consent,
        &theirs,
        &home,
        &SystemStashSink,
        &mut audit(&home),
    )
    .unwrap_err();

    assert!(matches!(err, StashError::Store(_)));
    assert!(p.exists());
    assert!(fs::read_dir(&theirs).unwrap().next().is_none());
}

// --- every refusal leaves a trace ------------------------------------------

/// A refusal nothing records is indistinguishable from a run that never
/// considered the item. All three whole-run gates used to return without
/// writing anything.
#[test]
fn a_whole_run_refusal_is_recorded_before_it_is_returned() {
    let (_d, home) = fixture();
    let p = agent(&home, "com.acme.helper");
    let (plan, mut consent) = plan_for(&home, std::slice::from_ref(&p));
    let one = consent.granted[0].clone();
    while consent.granted.len() <= MAX_STARTUP_GRANTS {
        consent.granted.push(one.clone());
    }

    let _ = stash(
        &plan,
        consent,
        &store_dir(&home),
        &home,
        &SystemStashSink,
        &mut audit(&home),
    )
    .unwrap_err();

    let text = log(&home);
    assert!(text.contains("refused"), "in:\n{text}");
    assert!(text.contains("more than the"));
}

#[test]
fn a_bad_store_is_recorded_before_the_run_is_refused() {
    let (_d, home) = fixture();
    let p = agent(&home, "com.acme.helper");
    fs::write(store_dir(&home), b"not a directory").unwrap();

    let (plan, consent) = plan_for(&home, std::slice::from_ref(&p));
    let _ = stash(
        &plan,
        consent,
        &store_dir(&home),
        &home,
        &SystemStashSink,
        &mut audit(&home),
    )
    .unwrap_err();

    assert!(log(&home).contains("refused"));
}

/// Both lines name the source, so a reader grepping the path they know finds
/// the whole story rather than only the refusal half of it.
#[test]
fn the_partial_state_names_the_source_on_both_of_its_lines() {
    use std::io;
    use swept_core::executor::StashSink;

    struct NoUnlink;
    impl StashSink for NoUnlink {
        fn link(&self, from: &Path, to: &Path) -> io::Result<()> {
            std::fs::hard_link(from, to)
        }
        fn unlink(&self, _path: &Path) -> io::Result<()> {
            Err(io::Error::other("no"))
        }
    }

    let (_d, home) = fixture();
    let p = agent(&home, "com.acme.helper");
    let (plan, consent) = plan_for(&home, std::slice::from_ref(&p));
    stash(
        &plan,
        consent,
        &store_dir(&home),
        &home,
        &NoUnlink,
        &mut audit(&home),
    )
    .unwrap();

    let source = p.display().to_string();
    let naming_source = log(&home)
        .lines()
        .filter(|l| l.contains(&format!("\"path\":\"{source}\"")))
        .count();
    assert_eq!(naming_source, 2, "one stashed line and one refusal");
}

// --- the two properties on the removal path that mutation could still break --
//
// Both of these are byte-loss properties, and both survived the first round of
// tests: the earlier fixtures reached the same counters by a different route,
// so their assertions could not tell a working check from a broken one. Each
// sink below is written so that it passes against the real code and fails
// against the single-line mutation of the check it names.

/// "We could not tell" must never mean "assume it is the same file".
///
/// The earlier version of this test removed *both* names, so the run ended in
/// the both-names-remain branch and reported the same counters either way.
/// Removing only the **destination** separates them: with the identity check
/// failing open, the source is the last name the file has, and it is removed.
#[test]
fn an_identity_that_cannot_be_read_never_removes_the_last_name() {
    use std::io;
    use swept_core::executor::StashSink;

    struct DestVanishes;
    impl StashSink for DestVanishes {
        fn link(&self, from: &Path, to: &Path) -> io::Result<()> {
            std::fs::hard_link(from, to)?;
            // Our link is gone; `from` is now the only name this file has.
            fs::remove_file(to)?;
            Ok(())
        }
        fn unlink(&self, path: &Path) -> io::Result<()> {
            fs::remove_file(path)
        }
    }

    let (_d, home) = fixture();
    let p = agent(&home, "com.acme.helper");
    let before = fs::read(&p).unwrap();
    let (plan, consent) = plan_for(&home, std::slice::from_ref(&p));

    let report = stash(
        &plan,
        consent,
        &store_dir(&home),
        &home,
        &DestVanishes,
        &mut audit(&home),
    )
    .unwrap();

    assert_eq!(report.moved, 0);
    assert!(
        p.exists(),
        "the only remaining name must not be removed when identity cannot be read"
    );
    assert_eq!(fs::read(&p).unwrap(), before);
}

/// `symlink_metadata`, never `metadata`.
///
/// If the destination is swapped for a symlink pointing back at the source,
/// a call that follows links reports the *source's* inode on both sides. The
/// code would then "prove" a match and remove the source, leaving a dangling
/// link and no copy at all.
#[test]
fn a_destination_swapped_for_a_link_back_to_the_source_is_not_mistaken_for_it() {
    use std::io;
    use swept_core::executor::StashSink;

    struct SymlinkSwap {
        from: PathBuf,
        to: PathBuf,
    }
    impl StashSink for SymlinkSwap {
        fn link(&self, from: &Path, to: &Path) -> io::Result<()> {
            std::fs::hard_link(from, to)?;
            fs::remove_file(&self.to)?;
            std::os::unix::fs::symlink(&self.from, &self.to)?;
            Ok(())
        }
        fn unlink(&self, path: &Path) -> io::Result<()> {
            fs::remove_file(path)
        }
    }

    let (_d, home) = fixture();
    let p = agent(&home, "com.acme.helper");
    let before = fs::read(&p).unwrap();
    let dest = store_dir(&home).join("com.acme.helper.plist");
    let (plan, consent) = plan_for(&home, std::slice::from_ref(&p));

    let report = stash(
        &plan,
        consent,
        &store_dir(&home),
        &home,
        &SymlinkSwap {
            from: p.clone(),
            to: dest,
        },
        &mut audit(&home),
    )
    .unwrap();

    assert_eq!(report.moved, 0);
    assert!(p.is_file(), "the bytes are still here, not a dangling link");
    assert_eq!(fs::read(&p).unwrap(), before);
}

/// `is_file`, not `!is_dir`. A FIFO, socket or device node is neither, and both
/// `hard_link` and a name-removal work on one — so without this check a socket
/// named like a plist is moved. The directory case passes for a different
/// reason (`hard_link` refuses a directory outright), which is why it cannot
/// stand in for this one.
#[test]
fn a_socket_named_like_a_plist_is_refused() {
    use std::os::unix::net::UnixListener;

    let (_d, home) = fixture();
    // Bound in a short path and moved into place: a Unix socket path has a hard
    // length limit that a nested tempdir path exceeds.
    let short = home.join("s");
    fs::create_dir_all(&short).unwrap();
    let _listener = UnixListener::bind(short.join("x")).unwrap();
    let socket = agents(&home).join("com.acme.helper.plist");
    fs::rename(short.join("x"), &socket).unwrap();

    let (plan, consent) = plan_for(&home, std::slice::from_ref(&socket));
    let report = stash(
        &plan,
        consent,
        &store_dir(&home),
        &home,
        &SystemStashSink,
        &mut audit(&home),
    )
    .unwrap();

    assert_eq!(report.moved, 0);
    assert!(
        fs::symlink_metadata(&socket).is_ok(),
        "and it is still there"
    );
}

/// The plan cannot be built with the two spellings supplied independently, so
/// the equality that refuses an already-symlinked plist cannot be made into a
/// tautology by a caller back-filling one from the other.
#[test]
fn the_listed_spelling_cannot_be_derived_from_the_guarded_one() {
    let (_d, home) = fixture();
    let real = agent(&home, "com.important.backup");
    let decoy = agents(&home).join("com.acme.decoy.plist");
    std::os::unix::fs::symlink(&real, &decoy).unwrap();

    // The only constructor guards the *listed* path itself, so `as_listed` is
    // the decoy and `path` is its target — and they differ, which is what `vet`
    // refuses. There is no way to hand it two agreeing values.
    let m = PlannedMove::new(decoy.clone(), &home, 0, "startup".to_string()).unwrap();
    assert_eq!(m.as_listed(), decoy.as_path());
    assert_eq!(m.path().as_path(), real.as_path());
    assert_ne!(m.as_listed(), m.path().as_path());
}

/// A refusal that records nothing is indistinguishable from a run that never
/// considered the item.
#[test]
fn a_per_item_refusal_leaves_a_record() {
    let (_d, home) = fixture();
    let notes = agents(&home).join("notes.txt");
    fs::write(&notes, b"hello").unwrap();

    let (plan, consent) = plan_for(&home, std::slice::from_ref(&notes));
    stash(
        &plan,
        consent,
        &store_dir(&home),
        &home,
        &SystemStashSink,
        &mut audit(&home),
    )
    .unwrap();

    let text = log(&home);
    assert!(text.contains("refused"), "in:\n{text}");
    assert!(text.contains(&notes.display().to_string()));
    assert!(text.contains("not a .plist"));
}

/// The chokepoint's reachable half.
///
/// `vet` calls `guard` before anything else, and the denylist half of that call
/// is unreachable by construction — a `SafePath` cannot hold a protected path,
/// so one can never be in a plan to be re-checked. What is reachable is the
/// re-resolution: a file that vanished between planning and acting.
///
/// The assertion is on the *reason*, not just the refusal. Without the guard
/// call the item is still refused — `symlink_metadata` fails a line later — but
/// it says "it could not be looked at" instead, so only the reason can tell the
/// two apart.
#[test]
fn a_plist_that_vanished_between_planning_and_acting_is_refused_by_the_guard() {
    let (_d, home) = fixture();
    let p = agent(&home, "com.acme.helper");
    let (plan, consent) = plan_for(&home, std::slice::from_ref(&p));

    fs::remove_file(&p).unwrap();

    let report = stash(
        &plan,
        consent,
        &store_dir(&home),
        &home,
        &SystemStashSink,
        &mut audit(&home),
    )
    .unwrap();

    assert_eq!(report.moved, 0);
    assert_eq!(report.refused, 1);
    assert!(
        log(&home).contains("no longer resolves to itself"),
        "the guard must be the one refusing it, and must speak first"
    );
}
