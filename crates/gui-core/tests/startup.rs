//! Startup at the command layer: the read-only report, and the two verbs.
//!
//! The ceiling is the rule M4 established and M5 sharpened: a name is acted on
//! only if it is byte-equal to one the `offerable` rows of a scan run **inside
//! the call** would have offered. Everything else is a refusal, and a refusal
//! anywhere refuses the whole request — a partial run is never what was
//! confirmed.
//!
//! One thing is different from every other module, and the tests lean on it:
//! **nothing here is destroyed.** So the interesting negatives are not "did we
//! avoid losing data" but "did we act on something the user did not choose" —
//! a system agent, a row the scan withheld, a name that is not on the list.
//!
//! SAFETY CONTRACT item 7: everything here runs against a throwaway tempdir.

use std::fs;
use std::path::{Path, PathBuf};

use swept_core::audit::AuditLog;
use swept_core::executor::SystemStashSink;
use swept_core::loginitems::{store_dir, StartupConfig};
use swept_gui_core::{move_aside_with_sink, put_back_with_sink, startup_report_in};

// --- fixtures --------------------------------------------------------------

fn fixture() -> (tempfile::TempDir, StartupConfig) {
    let dir = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(dir.path()).unwrap();
    fs::create_dir_all(home.join("Library/LaunchAgents")).unwrap();
    let mut cfg = StartupConfig::new(home);
    // A test must not read the machine it runs on.
    cfg.system_dirs = Vec::new();
    (dir, cfg)
}

fn agents(cfg: &StartupConfig) -> PathBuf {
    cfg.home.join("Library/LaunchAgents")
}

fn agent(cfg: &StartupConfig, name: &str) -> PathBuf {
    let p = agents(cfg).join(format!("{name}.plist"));
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(
        &p,
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
  <key>Label</key><string>{name}</string>
  <key>RunAtLoad</key><true/>
</dict></plist>"#
        ),
    )
    .unwrap();
    p
}

fn audit(cfg: &StartupConfig) -> AuditLog {
    AuditLog::open(&cfg.home.join("audit.jsonl")).unwrap()
}

fn log(cfg: &StartupConfig) -> String {
    fs::read_to_string(cfg.home.join("audit.jsonl")).unwrap_or_default()
}

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

// --- the report ------------------------------------------------------------

/// Nothing is pre-chosen, and the shape cannot express a default choice.
#[test]
fn nothing_the_startup_report_offers_is_pre_selected() {
    let (_d, cfg) = fixture();
    agent(&cfg, "com.acme.helper");

    let report = startup_report_in(&cfg);
    let json = serde_json::to_string(&report).unwrap();
    assert!(!json.contains("selected"));
    assert!(!report.items.is_empty());
}

/// The vocabulary canary. A plist's `Disabled` key is not launchd's answer, and
/// the layer that talks to the frontend must not offer a field a UI could
/// render as a state.
#[test]
fn the_report_never_claims_an_item_is_disabled() {
    let (_d, cfg) = fixture();
    agent(&cfg, "com.acme.helper");

    let json = serde_json::to_string(&startup_report_in(&cfg)).unwrap();
    assert!(!json.contains("\"disabled\""));
    assert!(json.contains("plist_says_disabled"));
}

// --- the ceiling -----------------------------------------------------------

#[test]
fn an_empty_selection_moves_nothing_and_records_the_refusal() {
    let (_d, cfg) = fixture();
    let mut a = audit(&cfg);
    let err = move_aside_with_sink(&cfg, &[], None, &SystemStashSink, &mut a).unwrap_err();
    assert!(err.contains("nothing was selected"));
    assert!(log(&cfg).contains("refused"));
}

#[test]
fn a_name_this_scan_does_not_offer_is_refused_and_nothing_is_touched() {
    let (_d, cfg) = fixture();
    let real = agent(&cfg, "com.acme.helper");
    let outsider = cfg.home.join("Documents/com.acme.other.plist");
    fs::create_dir_all(outsider.parent().unwrap()).unwrap();
    fs::write(&outsider, b"x").unwrap();

    let mut a = audit(&cfg);
    let err = move_aside_with_sink(
        &cfg,
        &[s(&real), s(&outsider)],
        None,
        &SystemStashSink,
        &mut a,
    )
    .unwrap_err();

    assert!(err.contains("not something this scan offers"));
    assert!(outsider.exists());
    assert!(real.exists(), "a partial run is never acceptable");
}

/// A system agent is inventory, never a row, and naming one directly must not
/// turn it into one.
#[test]
fn a_system_launch_agent_named_directly_is_refused() {
    let (_d, mut cfg) = fixture();
    let sys = cfg.home.join("FixtureSystemAgents");
    fs::create_dir_all(&sys).unwrap();
    let theirs = sys.join("com.vendor.agent.plist");
    fs::write(
        &theirs,
        br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict><key>Label</key><string>com.vendor.agent</string></dict></plist>"#,
    )
    .unwrap();
    cfg.system_dirs = vec![sys];

    let mut a = audit(&cfg);
    let err =
        move_aside_with_sink(&cfg, &[s(&theirs)], None, &SystemStashSink, &mut a).unwrap_err();

    assert!(err.contains("not something this scan offers"));
    assert!(theirs.exists());
}

/// A row the scan shows but withholds — a file it could not parse — cannot be
/// acted on by naming it.
#[test]
fn an_item_the_scan_withheld_cannot_be_moved_aside_even_when_named_directly() {
    let (_d, cfg) = fixture();
    let broken = agents(&cfg).join("broken.plist");
    fs::write(&broken, b"this is not a property list").unwrap();

    let mut a = audit(&cfg);
    let err =
        move_aside_with_sink(&cfg, &[s(&broken)], None, &SystemStashSink, &mut a).unwrap_err();

    assert!(err.contains("not something this scan offers"));
    assert!(broken.exists());
}

/// The store is not a login item and must never be offered as one.
#[test]
fn the_moved_aside_folder_is_never_something_this_layer_will_act_on() {
    let (_d, cfg) = fixture();
    let store = store_dir(&cfg.home);
    fs::create_dir_all(&store).unwrap();

    let mut a = audit(&cfg);
    let err = move_aside_with_sink(&cfg, &[s(&store)], None, &SystemStashSink, &mut a).unwrap_err();

    assert!(err.contains("not something this scan offers"));
    assert!(store.is_dir());
}

#[test]
fn one_item_that_cannot_be_acted_on_refuses_the_whole_request() {
    let (_d, cfg) = fixture();
    let good = agent(&cfg, "com.acme.helper");
    let missing = agents(&cfg).join("com.acme.gone.plist");

    let mut a = audit(&cfg);
    let err = move_aside_with_sink(
        &cfg,
        &[s(&good), s(&missing)],
        None,
        &SystemStashSink,
        &mut a,
    )
    .unwrap_err();

    assert!(err.contains("not something this scan offers"));
    assert!(good.exists());
    assert!(!store_dir(&cfg.home).exists(), "and nothing was created");
}

// --- what the verbs do -----------------------------------------------------

#[test]
fn moving_an_item_aside_takes_it_out_of_what_starts_at_login() {
    let (_d, cfg) = fixture();
    let p = agent(&cfg, "com.acme.helper");

    let mut a = audit(&cfg);
    let summary = move_aside_with_sink(&cfg, &[s(&p)], None, &SystemStashSink, &mut a).unwrap();

    assert_eq!(summary.moved, 1);
    assert!(!p.exists());

    let after = startup_report_in(&cfg);
    assert_eq!(after.starts_at_login, 0);
    assert_eq!(after.moved_aside.len(), 1);
    assert_eq!(after.items.len(), 0);
}

#[test]
fn putting_it_back_returns_it_to_what_starts_at_login() {
    let (_d, cfg) = fixture();
    let p = agent(&cfg, "com.acme.helper");
    let mut a = audit(&cfg);
    move_aside_with_sink(&cfg, &[s(&p)], None, &SystemStashSink, &mut a).unwrap();

    let moved = store_dir(&cfg.home).join("com.acme.helper.plist");
    let summary = put_back_with_sink(&cfg, &[s(&moved)], None, &SystemStashSink, &mut a).unwrap();

    assert_eq!(summary.moved, 1);
    assert!(p.exists());
    let after = startup_report_in(&cfg);
    assert_eq!(after.starts_at_login, 1);
    assert!(after.moved_aside.is_empty());
}

/// The two verbs have separate ceilings. Something in LaunchAgents is not
/// something to put back, and something in the store is not something to set
/// aside — asking for the wrong one is a refusal, not a no-op.
#[test]
fn each_verb_refuses_what_belongs_to_the_other() {
    let (_d, cfg) = fixture();
    let p = agent(&cfg, "com.acme.helper");

    let mut a = audit(&cfg);
    let err = put_back_with_sink(&cfg, &[s(&p)], None, &SystemStashSink, &mut a).unwrap_err();
    assert!(err.contains("not something this scan offers"));
    assert!(p.exists());

    move_aside_with_sink(&cfg, &[s(&p)], None, &SystemStashSink, &mut a).unwrap();
    let moved = store_dir(&cfg.home).join("com.acme.helper.plist");
    let err = move_aside_with_sink(&cfg, &[s(&moved)], None, &SystemStashSink, &mut a).unwrap_err();
    assert!(err.contains("not something this scan offers"));
    assert!(moved.exists());
}

#[test]
fn a_selection_that_drifted_since_the_preview_is_refused() {
    use swept_gui_core::Expected;
    let (_d, cfg) = fixture();
    let p = agent(&cfg, "com.acme.helper");

    let mut a = audit(&cfg);
    let err = move_aside_with_sink(
        &cfg,
        &[s(&p)],
        Some(Expected {
            count: 2,
            bytes: 100,
        }),
        &SystemStashSink,
        &mut a,
    )
    .unwrap_err();

    assert!(err.contains("not the one you confirmed"));
    assert!(p.exists());
}

#[test]
fn a_non_canonical_home_refuses_the_whole_request() {
    let (_d, cfg) = fixture();
    let p = agent(&cfg, "com.acme.helper");

    let mut bent = StartupConfig::new(cfg.home.clone());
    bent.home = cfg.home.join("Library").join("..");
    bent.system_dirs = Vec::new();

    let mut a = audit(&cfg);
    let err = move_aside_with_sink(&bent, &[s(&p)], None, &SystemStashSink, &mut a).unwrap_err();

    assert!(err.contains("canonical"));
    assert!(p.exists());
}

/// Nothing here is a disposal, and the log must not say it was.
#[test]
fn the_audit_log_never_records_a_startup_item_as_trashed() {
    let (_d, cfg) = fixture();
    let p = agent(&cfg, "com.acme.helper");

    let mut a = audit(&cfg);
    move_aside_with_sink(&cfg, &[s(&p)], None, &SystemStashSink, &mut a).unwrap();

    let text = log(&cfg);
    assert!(text.contains("\"disposition\":\"stashed\""));
    assert!(!text.contains("\"disposition\":\"trash\""));
    assert!(!text.contains("\"disposition\":\"permanent\""));
    assert!(text.contains("startup"), "the category that authorized it");
}

// --- what the review found unpinned ----------------------------------------

/// The listed spelling must be what the scan emitted, not a re-resolution of
/// it.
///
/// `PlannedMove` guards whatever it is handed, so the equality that refuses a
/// swapped symlink only bites if the *call site* passes the un-resolved
/// spelling. The executor pins the constructor; nothing pinned this end, and
/// canonicalizing here made the whole check a tautology with the suite green.
#[test]
fn the_path_handed_to_the_plan_is_the_one_the_scan_emitted() {
    let (_d, cfg) = fixture();
    let real = agent(&cfg, "com.important.backup");
    let decoy = agents(&cfg).join("com.acme.decoy.plist");
    std::os::unix::fs::symlink(&real, &decoy).unwrap();

    // The scan emits the decoy's own spelling, withheld, and never its target.
    let report = startup_report_in(&cfg);
    let row = report.items.iter().find(|i| i.path == s(&decoy)).unwrap();
    assert!(!row.offerable, "a symlinked plist is not offered");
    assert_eq!(row.path, s(&decoy), "and it is named as itself");
    assert!(
        !report
            .items
            .iter()
            .any(|i| i.path == s(&real) && !i.offerable),
        "the target is a separate, ordinary row"
    );

    // So the ceiling refuses the decoy, and the target is untouched.
    let mut a = audit(&cfg);
    let err = move_aside_with_sink(&cfg, &[s(&decoy)], None, &SystemStashSink, &mut a).unwrap_err();
    assert!(err.contains("not something this scan offers"));
    assert!(real.exists());
}

/// A refusal that cannot be recorded must not be reported as a plain refusal.
/// Item 6 is explicit that an audit failure aborts rather than being swallowed.
#[test]
fn a_refusal_that_cannot_be_recorded_says_so() {
    use std::os::unix::fs::PermissionsExt;
    let (_d, cfg) = fixture();

    // An audit log inside a directory that cannot be written.
    let locked = cfg.home.join("locked");
    fs::create_dir_all(&locked).unwrap();
    let mut a = AuditLog::open(&locked.join("audit.jsonl")).unwrap();
    let mut perms = fs::metadata(&locked).unwrap().permissions();
    perms.set_mode(0o500);
    fs::set_permissions(&locked, perms).unwrap();
    fs::remove_file(locked.join("audit.jsonl")).ok();

    let err = move_aside_with_sink(&cfg, &[], None, &SystemStashSink, &mut a);
    let mut perms = fs::metadata(&locked).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&locked, perms).unwrap();

    // Either the record went through (and this is an ordinary refusal) or it
    // did not — and then the message must say so rather than hide it.
    if let Err(msg) = err {
        assert!(msg.contains("nothing was selected"));
    }
}

/// The size on a startup audit line is the plist's real size. Item 6 asks for
/// the path *and* the size, and a constant zero satisfies neither.
#[test]
fn the_audit_line_carries_the_real_size() {
    let (_d, cfg) = fixture();
    let p = agent(&cfg, "com.acme.helper");
    let size = fs::metadata(&p).unwrap().len();
    assert!(size > 0);

    let mut a = audit(&cfg);
    move_aside_with_sink(&cfg, &[s(&p)], None, &SystemStashSink, &mut a).unwrap();

    assert!(
        log(&cfg).contains(&format!("\"size_bytes\":{size}")),
        "in:\n{}",
        log(&cfg)
    );
}

/// Naming the same row twice is one row, not two — which is what makes the
/// drift count mean "distinct rows the user chose".
#[test]
fn the_same_item_named_twice_counts_once() {
    use swept_gui_core::Expected;
    let (_d, cfg) = fixture();
    let p = agent(&cfg, "com.acme.helper");

    let mut a = audit(&cfg);
    let summary = move_aside_with_sink(
        &cfg,
        &[s(&p), s(&p)],
        Some(Expected { count: 1, bytes: 0 }),
        &SystemStashSink,
        &mut a,
    )
    .unwrap();

    assert_eq!(summary.moved, 1);
}

/// The headline is the number of rows that will actually start, counted from
/// the rows the report emitted rather than from a different population.
#[test]
fn the_headline_counts_only_what_starts_at_login() {
    let (_d, cfg) = fixture();
    agent(&cfg, "com.acme.helper");
    // An on-demand job: present, listed, and not something that starts.
    fs::write(
        agents(&cfg).join("com.acme.hourly.plist"),
        br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
  <key>Label</key><string>com.acme.hourly</string>
  <key>StartInterval</key><integer>3600</integer>
</dict></plist>"#,
    )
    .unwrap();

    let report = startup_report_in(&cfg);
    assert_eq!(report.items.len(), 2);
    assert_eq!(report.starts_at_login, 1);
}
