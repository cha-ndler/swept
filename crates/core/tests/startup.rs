//! Startup — what runs when you log in, read-only.
//!
//! This half changes nothing. What it has to get right is *not claiming more
//! than it knows*, and there are three specific ways it could:
//!
//! 1. **"Disabled" is not this module's word to use.** A plist's `Disabled`
//!    key is only the initial value for a job launchd's own database has never
//!    seen; once an override exists the key is ignored. Measured on the
//!    reference machine: none of the five user agents has the key at all. So
//!    the field says what it is — a key in a file — and the module never
//!    reports a job as disabled.
//! 2. **`RunAtLoad` is not the whole story.** A job with `KeepAlive` starts at
//!    login too, and one with `StartInterval` does not. Counting only
//!    `RunAtLoad` under-reports what starts.
//! 3. **Most login items are not here at all.** The reference machine has 5
//!    user agents against 10 system agents, 16 system daemons, and a modern
//!    `SMAppService` store this module can never read. A report that mentions
//!    only the five invites the user to conclude their Mac is clean.
//!
//! SAFETY CONTRACT item 7: everything here runs against a throwaway tempdir.

use std::fs;
use std::path::{Path, PathBuf};

use macclean_core::loginitems::{
    scan, store_dir, Access, StartClass, StartupConfig, DEFERRED_SOURCES, STORE_DIR_NAME,
    STORE_NOTE_NAME,
};
use safety::allowlist;

// --- fixtures --------------------------------------------------------------

fn fixture() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(dir.path()).unwrap();
    fs::create_dir_all(home.join("Library/LaunchAgents")).unwrap();
    (dir, home)
}

fn cfg(home: &Path) -> StartupConfig {
    let mut c = StartupConfig::new(home.to_path_buf());
    // The system directories are real absolute paths; a test must not read the
    // machine it runs on.
    c.system_dirs = Vec::new();
    c
}

fn agents(home: &Path) -> PathBuf {
    home.join("Library/LaunchAgents")
}

/// A LaunchAgent plist. `body` is the inside of the top-level `<dict>`.
fn agent(home: &Path, name: &str, body: &str) -> PathBuf {
    let p = agents(home).join(format!("{name}.plist"));
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(
        &p,
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>{name}</string>
{body}
</dict></plist>"#
        ),
    )
    .unwrap();
    p
}

fn runs_at_login(home: &Path, name: &str, program: &Path) -> PathBuf {
    agent(
        home,
        name,
        &format!(
            "  <key>Program</key><string>{}</string>\n  <key>RunAtLoad</key><true/>",
            program.display()
        ),
    )
}

fn touch_program(home: &Path, name: &str) -> PathBuf {
    let p = home.join("bin").join(name);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, b"#!/bin/sh\n").unwrap();
    p
}

// --- what starts at login --------------------------------------------------

#[test]
fn a_plist_that_runs_at_login_is_a_row_and_nothing_is_pre_selected() {
    let (_d, home) = fixture();
    let prog = touch_program(&home, "helper");
    runs_at_login(&home, "com.acme.helper", &prog);

    let report = scan(&cfg(&home));
    assert_eq!(report.items.len(), 1);
    let item = &report.items[0];
    assert_eq!(item.label, "com.acme.helper");
    assert_eq!(item.class, StartClass::StartsAtLogin);
    assert!(item.offerable);
    // No `selected` field exists to be true; this pins that the report cannot
    // express a default choice.
    let json = serde_json::to_string(&report.items).unwrap();
    assert!(!json.contains("selected"));
}

/// `RunAtLoad` is not the whole story: a job launchd is told to keep alive is
/// started at load whether or not the key is present.
#[test]
fn a_job_with_keep_alive_and_no_run_at_load_still_starts_at_login() {
    let (_d, home) = fixture();
    let prog = touch_program(&home, "daemonish");
    agent(
        &home,
        "com.acme.keepalive",
        &format!(
            "  <key>Program</key><string>{}</string>\n  <key>KeepAlive</key><true/>",
            prog.display()
        ),
    );

    let report = scan(&cfg(&home));
    assert_eq!(report.items[0].class, StartClass::StartsAtLogin);
    assert!(!report.items[0].run_at_load, "the key really is absent");
}

#[test]
fn a_job_that_only_starts_on_a_schedule_is_not_counted_as_starting_at_login() {
    let (_d, home) = fixture();
    let prog = touch_program(&home, "hourly");
    agent(
        &home,
        "com.acme.hourly",
        &format!(
            "  <key>Program</key><string>{}</string>\n  <key>StartInterval</key><integer>3600</integer>",
            prog.display()
        ),
    );

    let report = scan(&cfg(&home));
    assert_eq!(report.items[0].class, StartClass::StartsOnDemand);
    assert_eq!(report.starts_at_login(), 0);
}

/// A canary on the mapping: every class must mean exactly one thing, so a new
/// one cannot be added without deciding what it tells the user.
#[test]
fn every_start_class_has_exactly_one_description() {
    let mut seen: Vec<&str> = [
        StartClass::StartsAtLogin,
        StartClass::StartsOnDemand,
        StartClass::Broken,
        StartClass::Unknown,
    ]
    .into_iter()
    .map(|c| c.describe())
    .collect();
    let len = seen.len();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), len, "two classes say the same thing");
    assert!(seen.iter().all(|d| !d.is_empty()));
}

// --- broken, and the two ways that claim could be wrong ---------------------

#[test]
fn a_plist_whose_absolute_program_is_missing_is_reported_as_broken() {
    let (_d, home) = fixture();
    runs_at_login(&home, "com.acme.gone", &home.join("bin/not-there"));

    let report = scan(&cfg(&home));
    assert_eq!(report.items[0].class, StartClass::Broken);
    assert!(
        report.items[0].offerable,
        "a job that fails at every login is the safest thing on the screen"
    );
}

/// Absent and denied look alike through a failed lookup and mean opposite
/// things. Calling a working item broken because we could not look at it is
/// the wrong direction to be wrong in.
#[test]
fn a_program_that_cannot_be_looked_at_is_never_called_broken() {
    use std::os::unix::fs::PermissionsExt;
    let (_d, home) = fixture();
    let hidden = home.join("private");
    fs::create_dir_all(&hidden).unwrap();
    let prog = hidden.join("helper");
    fs::write(&prog, b"#!/bin/sh\n").unwrap();
    runs_at_login(&home, "com.acme.hidden", &prog);

    let mut perms = fs::metadata(&hidden).unwrap().permissions();
    perms.set_mode(0o000);
    fs::set_permissions(&hidden, perms).unwrap();
    let report = scan(&cfg(&home));
    let mut perms = fs::metadata(&hidden).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&hidden, perms).unwrap();

    assert_ne!(report.items[0].class, StartClass::Broken);
}

/// A relative program, or a `/bin/sh` wrapper whose real work is in `argv[1]`,
/// cannot support the claim either way. Say nothing rather than guess.
#[test]
fn a_relative_program_path_is_never_called_broken() {
    let (_d, home) = fixture();
    agent(
        &home,
        "com.acme.relative",
        "  <key>ProgramArguments</key><array><string>helper</string></array>\n  <key>RunAtLoad</key><true/>",
    );

    let report = scan(&cfg(&home));
    assert_ne!(report.items[0].class, StartClass::Broken);
}

// --- the vocabulary --------------------------------------------------------

/// The load-bearing honesty test. A plist's `Disabled` key is not launchd's
/// answer, so the module reports the key and never the state.
#[test]
fn the_report_never_claims_an_item_is_disabled() {
    let (_d, home) = fixture();
    let prog = touch_program(&home, "helper");
    agent(
        &home,
        "com.acme.marked",
        &format!(
            "  <key>Program</key><string>{}</string>\n  <key>RunAtLoad</key><true/>\n  <key>Disabled</key><true/>",
            prog.display()
        ),
    );

    let report = scan(&cfg(&home));
    assert!(report.items[0].plist_says_disabled);
    // The field is named for what it is. Nothing in the serialized shape offers
    // a bare `disabled` that a UI could render as a state.
    let json = serde_json::to_string(&report.items).unwrap();
    assert!(json.contains("plist_says_disabled"));
    assert!(!json.contains("\"disabled\""));
}

// --- what is shown and not offered -----------------------------------------

#[test]
fn a_symlinked_plist_is_shown_and_never_offered() {
    let (_d, home) = fixture();
    let prog = touch_program(&home, "helper");
    let real = runs_at_login(&home, "com.acme.real", &prog);
    let link = agents(&home).join("com.acme.link.plist");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    let report = scan(&cfg(&home));
    let row = report
        .items
        .iter()
        .find(|i| i.source == link.display().to_string())
        .unwrap();
    assert!(!row.offerable);
    assert!(row.withheld.is_some());
}

#[test]
fn a_file_in_launch_agents_that_is_not_a_plist_is_shown_with_its_reason() {
    let (_d, home) = fixture();
    fs::write(agents(&home).join("notes.txt"), b"hello").unwrap();

    let report = scan(&cfg(&home));
    assert_eq!(report.items.len(), 1);
    assert!(!report.items[0].offerable);
    assert!(report.items[0]
        .withheld
        .as_ref()
        .unwrap()
        .contains(".plist"));
}

#[test]
fn a_plist_that_does_not_parse_is_shown_and_never_offered() {
    let (_d, home) = fixture();
    fs::write(agents(&home).join("broken.plist"), b"this is not a plist").unwrap();

    let report = scan(&cfg(&home));
    assert_eq!(report.items.len(), 1);
    assert!(!report.items[0].offerable);
    assert!(report.items[0]
        .withheld
        .as_ref()
        .unwrap()
        .contains("property list"));
}

/// Two rows a user cannot tell apart is how consent is given to the wrong one.
#[test]
fn two_plists_sharing_a_label_are_two_rows_and_the_duplication_is_named() {
    let (_d, home) = fixture();
    let prog = touch_program(&home, "helper");
    for file in ["a", "b"] {
        agent(
            &home,
            file,
            &format!(
                "  <key>Program</key><string>{}</string>\n  <key>RunAtLoad</key><true/>",
                prog.display()
            ),
        );
        // Both declare the same Label, which is what launchd keys on.
        let p = agents(&home).join(format!("{file}.plist"));
        let text = fs::read_to_string(&p).unwrap();
        fs::write(
            &p,
            text.replace(
                &format!("<string>{file}</string>"),
                "<string>com.acme.twin</string>",
            ),
        )
        .unwrap();
    }

    let report = scan(&cfg(&home));
    assert_eq!(report.items.len(), 2);
    assert!(report.items.iter().all(|i| i.duplicate_label));
}

// --- the moved-aside folder ------------------------------------------------

#[test]
fn the_moved_aside_folder_is_never_itself_a_login_item() {
    let (_d, home) = fixture();
    fs::create_dir_all(store_dir(&home)).unwrap();

    let report = scan(&cfg(&home));
    assert!(report.items.is_empty());
}

#[test]
fn a_plist_in_the_moved_aside_folder_is_listed_as_moved_aside_and_never_as_running() {
    let (_d, home) = fixture();
    let prog = touch_program(&home, "helper");
    let live = runs_at_login(&home, "com.acme.helper", &prog);
    let store = store_dir(&home);
    fs::create_dir_all(&store).unwrap();
    fs::rename(&live, store.join("com.acme.helper.plist")).unwrap();

    let report = scan(&cfg(&home));
    assert!(
        report.items.is_empty(),
        "it does not start at login any more"
    );
    assert_eq!(report.moved_aside.len(), 1);
    assert_eq!(report.moved_aside[0].label, "com.acme.helper");
    assert_eq!(report.starts_at_login(), 0);
}

/// Read-only means read-only. A scan that created its own store would be
/// writing to the user's `~/Library` to answer a question.
#[test]
fn a_scan_never_creates_the_moved_aside_folder() {
    let (_d, home) = fixture();
    let _ = scan(&cfg(&home));
    assert!(!store_dir(&home).exists());
}

#[test]
fn the_store_lives_inside_the_launch_agents_folder_the_user_already_knows() {
    let (_d, home) = fixture();
    assert_eq!(store_dir(&home).parent().unwrap(), agents(&home));
    assert_eq!(
        store_dir(&home).file_name().unwrap().to_str().unwrap(),
        STORE_DIR_NAME
    );
}

// --- honesty about what is not here ----------------------------------------

#[test]
fn an_unreadable_launch_agents_directory_is_reported_as_denied_not_as_empty() {
    use std::os::unix::fs::PermissionsExt;
    let (_d, home) = fixture();
    let dir = agents(&home);
    let mut perms = fs::metadata(&dir).unwrap().permissions();
    perms.set_mode(0o000);
    fs::set_permissions(&dir, perms).unwrap();
    let report = scan(&cfg(&home));
    let mut perms = fs::metadata(&dir).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&dir, perms).unwrap();

    assert!(matches!(
        report.sources[0].access,
        Access::NeedsPermission | Access::Unreadable(_)
    ));
    assert!(report.partial);
}

#[test]
fn the_system_launch_agent_directories_are_inventory_and_carry_no_control() {
    let (_d, home) = fixture();
    let sys = home.join("FixtureSystemAgents");
    fs::create_dir_all(&sys).unwrap();
    fs::write(
        sys.join("com.vendor.agent.plist"),
        br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict><key>Label</key><string>com.vendor.agent</string></dict></plist>"#,
    )
    .unwrap();

    let mut c = cfg(&home);
    c.system_dirs = vec![sys];
    let report = scan(&c);

    assert_eq!(report.system.len(), 1);
    assert_eq!(report.system[0].label, "com.vendor.agent");
    // `SystemItem` has no `offerable` field at all: a control this module can
    // never honour is not expressible, rather than expressible and false.
    let json = serde_json::to_string(&report.system).unwrap();
    assert!(!json.contains("offerable"));
    assert!(!json.contains("withheld"));
}

#[test]
fn the_deferred_sources_are_named_rather_than_silently_absent() {
    assert!(!DEFERRED_SOURCES.is_empty());
    for (what, why) in DEFERRED_SOURCES {
        assert!(!what.is_empty());
        assert!(why.len() > 20, "{what} needs a real reason, not a shrug");
    }
    // The modern store is the one a user is most likely to be looking for.
    assert!(DEFERRED_SOURCES
        .iter()
        .any(|(what, _)| what.contains("backgrounditems")));
}

/// Its existence is reported so the count can be read correctly. Its contents
/// are never parsed: it is an opaque, versioned, Apple-owned store, and a
/// misparse would fabricate rows about things the user cannot cross-check.
#[test]
fn the_background_task_management_store_is_reported_as_present_and_never_read() {
    let (_d, home) = fixture();
    let btm = home.join(
        "Library/Application Support/com.apple.backgroundtaskmanagementagent/backgrounditems.btm",
    );
    fs::create_dir_all(btm.parent().unwrap()).unwrap();
    fs::write(&btm, b"\x00binary garbage that must never be parsed").unwrap();

    let report = scan(&cfg(&home));
    assert!(report.modern_store_present);
    assert!(report.caveats.iter().any(|c| c.contains("System Settings")));
}

// --- confinement -----------------------------------------------------------

/// Nothing this module reports may be removable without a per-path grant, and
/// nothing it reports may be inside the ordinary cleanup scope.
#[test]
fn no_startup_path_is_inside_the_disposal_allowlist() {
    let (_d, home) = fixture();
    let prog = touch_program(&home, "helper");
    runs_at_login(&home, "com.acme.helper", &prog);

    let report = scan(&cfg(&home));
    let disposal = allowlist::default_roots(&home);
    assert!(!report.items.is_empty());
    for item in &report.items {
        assert!(!allowlist::is_allowed(Path::new(&item.source), &disposal));
    }
}

#[test]
fn the_scan_mutates_nothing() {
    let (_d, home) = fixture();
    let prog = touch_program(&home, "helper");
    let p = runs_at_login(&home, "com.acme.helper", &prog);
    let before = fs::read(&p).unwrap();
    let modified = fs::metadata(&p).unwrap().modified().unwrap();

    let _ = scan(&cfg(&home));
    assert_eq!(fs::read(&p).unwrap(), before);
    assert_eq!(fs::metadata(&p).unwrap().modified().unwrap(), modified);
}

/// The note this app writes into the store explains the folder; it is not
/// something the user put there and it is not a login item. Listing it would
/// put a row on screen saying "this is not a .plist" about a file we created.
#[test]
fn the_note_in_the_store_is_not_listed_as_something_that_was_moved_aside() {
    let (_d, home) = fixture();
    let store = store_dir(&home);
    fs::create_dir_all(&store).unwrap();
    fs::write(store.join(STORE_NOTE_NAME), b"how to put these back").unwrap();
    fs::write(
        store.join("com.acme.helper.plist"),
        br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict><key>Label</key><string>com.acme.helper</string></dict></plist>"#,
    )
    .unwrap();

    let report = scan(&cfg(&home));
    assert_eq!(report.moved_aside.len(), 1);
    assert_eq!(report.moved_aside[0].label, "com.acme.helper");
}

/// The note skip is exact and scoped, and both halves matter. A file named
/// like the note in LaunchAgents proper is an ordinary row — skipping it there
/// would hide something the user put there. And a name that merely *begins*
/// with the note's name is a different file: this is the `.Trash`/`.Trashes`
/// prefix class the contract calls out, and the failure would be an item the
/// app moved aside and can then never show or put back.
#[test]
fn the_note_skip_is_exact_and_only_applies_inside_the_store() {
    let (_d, home) = fixture();

    // In LaunchAgents proper: listed, withheld for not being a plist.
    fs::write(agents(&home).join(STORE_NOTE_NAME), b"the user's own file").unwrap();

    let store = store_dir(&home);
    fs::create_dir_all(&store).unwrap();
    // In the store, a plist whose name merely starts with the note's.
    let prefixed = store.join(format!("{STORE_NOTE_NAME}.plist"));
    fs::write(
        &prefixed,
        br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict><key>Label</key><string>com.acme.oddly.named</string></dict></plist>"#,
    )
    .unwrap();

    let report = scan(&cfg(&home));
    assert_eq!(
        report.items.len(),
        1,
        "the note-named file in LaunchAgents is a row"
    );
    assert!(!report.items[0].offerable);
    assert_eq!(
        report.moved_aside.len(),
        1,
        "and a name that only starts with the note's is not the note"
    );
    assert!(report.moved_aside[0].offerable);
}
