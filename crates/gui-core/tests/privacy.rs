//! Privacy at the command layer: the read-only report, and the disposal whose
//! ceiling is the `offerable` rows of a scan run inside the call.
//!
//! Two rules carry this file.
//!
//! **A partial run is never acceptable.** If any part of the selection is not
//! exactly an offerable row of a fresh scan, nothing is touched.
//!
//! **Consent here has a second axis.** Cookies sign the user out everywhere,
//! history cannot be brought back, and a session is the tabs they have open.
//! Each is refused unless the request carries an explicit acknowledgement of
//! that specific consequence — the analogue of `confirm_mass_delete`, and
//! refused by default, so a frontend that loses its checkbox state cannot
//! smuggle a cookie jar through.
//!
//! SAFETY CONTRACT item 7: everything here runs against a throwaway tempdir.

use std::fs;
use std::path::{Path, PathBuf};

use macclean_core::audit::AuditLog;
use macclean_core::executor::DirSink;
use macclean_core::privacy::PrivacyConfig;
use macclean_gui_core::{
    dispose_privacy_with_sink, privacy_report_in, probe_permissions, Acknowledged, CleanSummary,
    Expected,
};

// --- fixtures --------------------------------------------------------------

fn fixture() -> (tempfile::TempDir, PrivacyConfig) {
    let dir = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(dir.path()).unwrap();
    fs::create_dir_all(home.join("Documents")).unwrap();
    (dir, PrivacyConfig::new(home))
}

fn write_sized(path: &Path, bytes: u64) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, vec![0u8; bytes as usize]).unwrap();
}

fn chrome_root(cfg: &PrivacyConfig) -> PathBuf {
    cfg.home.join("Library/Application Support/Google/Chrome")
}

fn chromium_profile(cfg: &PrivacyConfig, name: &str) -> PathBuf {
    let p = chrome_root(cfg).join(name);
    write_sized(&p.join("Preferences"), 10);
    p
}

fn firefox_profile(cfg: &PrivacyConfig, name: &str) -> PathBuf {
    let p = cfg
        .home
        .join("Library/Application Support/Firefox/Profiles")
        .join(name);
    write_sized(&p.join("prefs.js"), 10);
    p
}

fn sink(cfg: &PrivacyConfig) -> DirSink {
    DirSink {
        trash_dir: cfg.home.join("FixtureTrash"),
    }
}

fn audit(cfg: &PrivacyConfig) -> AuditLog {
    AuditLog::open(&cfg.home.join("audit.jsonl")).unwrap()
}

fn everything() -> Acknowledged {
    Acknowledged {
        signs_you_out: true,
        erases_history: true,
        loses_open_tabs: true,
    }
}

fn dispose(
    cfg: &PrivacyConfig,
    paths: &[String],
    ack: Acknowledged,
    audit: &mut AuditLog,
) -> Result<CleanSummary, String> {
    dispose_privacy_with_sink(cfg, paths, ack, None, true, &sink(cfg), audit)
}

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn audit_text(cfg: &PrivacyConfig) -> String {
    fs::read_to_string(cfg.home.join("audit.jsonl")).unwrap_or_default()
}

// --- the report ------------------------------------------------------------

/// Nothing this module reports is pre-chosen. The DTO has no field that could
/// express a default selection, which is a stronger statement than a field
/// that happens to be false.
#[test]
fn nothing_the_privacy_report_offers_is_pre_selected() {
    let (_d, cfg) = fixture();
    let p = chromium_profile(&cfg, "Profile 1");
    write_sized(&p.join("Cookies"), 100);

    let report = privacy_report_in(&cfg);
    let json = serde_json::to_string(&report).unwrap();
    assert!(!json.contains("selected"));
    assert!(!report.rows.is_empty());
}

/// The frontend names a row by its own path and nothing else. It never sees
/// the sidecars, so it cannot name one — the backend expands them from the
/// fresh scan at the moment it acts.
#[test]
fn the_frontend_is_never_given_the_paths_of_a_rows_sidecars() {
    let (_d, cfg) = fixture();
    let ff = firefox_profile(&cfg, "abc.default-release");
    write_sized(&ff.join("cookies.sqlite"), 100);
    write_sized(&ff.join("cookies.sqlite-wal"), 50);

    let report = privacy_report_in(&cfg);
    let json = serde_json::to_string(&report).unwrap();
    assert!(!json.contains("cookies.sqlite-wal"));
    let row = &report.rows[0];
    assert_eq!(row.member_count, 2);
    assert_eq!(row.size_bytes, 150);
}

#[test]
fn the_permissions_probe_reports_safari_separately_from_the_trash_and_containers() {
    let (_d, cfg) = fixture();
    let p = probe_permissions(&cfg.home);
    // Absent is not denied — the probe must not send anyone to System Settings
    // to fix something that is not broken.
    assert!(p.safari_readable);
    assert!(p.all_readable);
}

// --- the ceiling -----------------------------------------------------------

#[test]
fn an_empty_selection_disposes_of_nothing_and_records_the_refusal() {
    let (_d, cfg) = fixture();
    let mut a = audit(&cfg);
    let err = dispose(&cfg, &[], everything(), &mut a).unwrap_err();
    assert!(err.contains("nothing was selected"));
    assert!(audit_text(&cfg).contains("refused"));
}

#[test]
fn a_path_this_scan_does_not_offer_is_refused_and_nothing_is_touched() {
    let (_d, cfg) = fixture();
    let p = chromium_profile(&cfg, "Profile 1");
    write_sized(&p.join("GPUCache/blob"), 100);
    let outsider = cfg.home.join("Documents/notes.txt");
    write_sized(&outsider, 100);

    let mut a = audit(&cfg);
    let err = dispose(
        &cfg,
        &[s(&p.join("GPUCache")), s(&outsider)],
        everything(),
        &mut a,
    )
    .unwrap_err();

    assert!(err.contains("not something this scan offers"));
    assert!(outsider.exists());
    assert!(
        p.join("GPUCache/blob").exists(),
        "a partial run is never acceptable"
    );
}

/// The ceiling is the *rows*, not the roots. A file sitting inside a profile
/// this module searched is still not something it offered.
#[test]
fn a_file_merely_inside_a_profile_is_refused_because_it_is_not_a_row() {
    let (_d, cfg) = fixture();
    let p = chromium_profile(&cfg, "Profile 1");
    write_sized(&p.join("Login Data"), 100);

    let mut a = audit(&cfg);
    let err = dispose(&cfg, &[s(&p.join("Login Data"))], everything(), &mut a).unwrap_err();
    assert!(err.contains("not something this scan offers"));
    assert!(p.join("Login Data").exists());
}

#[test]
fn a_sidecar_path_sent_on_its_own_is_refused() {
    let (_d, cfg) = fixture();
    let ff = firefox_profile(&cfg, "abc.default-release");
    write_sized(&ff.join("cookies.sqlite"), 100);
    write_sized(&ff.join("cookies.sqlite-wal"), 50);

    let mut a = audit(&cfg);
    let err = dispose(
        &cfg,
        &[s(&ff.join("cookies.sqlite-wal"))],
        everything(),
        &mut a,
    )
    .unwrap_err();
    assert!(err.contains("not something this scan offers"));
    assert!(ff.join("cookies.sqlite-wal").exists());
}

#[test]
fn a_site_storage_row_cannot_be_disposed_of_even_when_named_directly() {
    let (_d, cfg) = fixture();
    let p = chromium_profile(&cfg, "Profile 1");
    write_sized(&p.join("Local Storage/leveldb/x"), 100);

    let mut a = audit(&cfg);
    let err = dispose(&cfg, &[s(&p.join("Local Storage"))], everything(), &mut a).unwrap_err();
    assert!(err.contains("not something this scan offers"));
    assert!(p.join("Local Storage/leveldb/x").exists());
}

#[test]
fn a_withheld_row_cannot_be_disposed_of_even_when_named_directly() {
    let (_d, cfg) = fixture();
    let jar = cfg
        .home
        .join("Library/Containers/com.apple.Safari/Data/Library/Cookies/Cookies.binarycookies");
    write_sized(&jar, 100);

    let mut a = audit(&cfg);
    let err = dispose(&cfg, &[s(&jar)], everything(), &mut a).unwrap_err();
    assert!(err.contains("not something this scan offers"));
    assert!(jar.exists());
}

/// The scan happens inside the call, so a browser that started up while the
/// sheet was open takes its own rows off the table.
#[test]
fn a_row_whose_browser_became_live_between_scan_and_disposal_is_refused() {
    let (_d, cfg) = fixture();
    let p = chromium_profile(&cfg, "Profile 1");
    write_sized(&p.join("Cookies"), 100);
    let chosen = vec![s(&p.join("Cookies"))];

    // The user launches Chrome while reading the sheet.
    std::os::unix::fs::symlink("host-1", chrome_root(&cfg).join("SingletonLock")).unwrap();

    let mut a = audit(&cfg);
    let err = dispose(&cfg, &chosen, everything(), &mut a).unwrap_err();
    assert!(err.contains("not something this scan offers"));
    assert!(p.join("Cookies").exists());
}

#[test]
fn a_symlink_swapped_in_after_the_scan_is_refused_not_followed() {
    let (_d, cfg) = fixture();
    let p = chromium_profile(&cfg, "Profile 1");
    write_sized(&p.join("Cookies"), 100);
    let chosen = vec![s(&p.join("Cookies"))];

    let elsewhere = cfg.home.join("Documents/secret.txt");
    write_sized(&elsewhere, 100);
    fs::remove_file(p.join("Cookies")).unwrap();
    std::os::unix::fs::symlink(&elsewhere, p.join("Cookies")).unwrap();

    let mut a = audit(&cfg);
    let err = dispose(&cfg, &chosen, everything(), &mut a).unwrap_err();
    assert!(err.contains("not something this scan offers"));
    assert!(elsewhere.exists(), "the symlink's target is untouched");
}

#[test]
fn a_non_canonical_home_refuses_the_whole_request() {
    let (_d, cfg) = fixture();
    let p = chromium_profile(&cfg, "Profile 1");
    write_sized(&p.join("Cookies"), 100);

    // Bent *after* construction: `PrivacyConfig::new` carries a `debug_assert`
    // that a caller passing a non-canonical home is a bug, and this test is
    // about the disposal seam refusing one rather than about that assertion.
    let mut bent = PrivacyConfig::new(cfg.home.clone());
    bent.home = cfg.home.join("Documents").join("..");
    let mut a = audit(&cfg);
    let err = dispose_privacy_with_sink(
        &bent,
        &[s(&p.join("Cookies"))],
        everything(),
        None,
        true,
        &sink(&cfg),
        &mut a,
    )
    .unwrap_err();

    assert!(err.contains("canonical"));
    assert!(p.join("Cookies").exists());
}

// --- the consequence gate --------------------------------------------------

#[test]
fn a_cookie_row_is_refused_without_the_sign_out_acknowledgement() {
    let (_d, cfg) = fixture();
    let p = chromium_profile(&cfg, "Profile 1");
    write_sized(&p.join("Cookies"), 100);

    let mut a = audit(&cfg);
    let err = dispose(
        &cfg,
        &[s(&p.join("Cookies"))],
        Acknowledged::default(),
        &mut a,
    )
    .unwrap_err();

    assert!(
        err.contains("signed out"),
        "the refusal names the consequence: {err}"
    );
    assert!(p.join("Cookies").exists());
    assert!(audit_text(&cfg).contains("refused"));
}

#[test]
fn a_history_row_is_refused_without_the_history_acknowledgement() {
    let (_d, cfg) = fixture();
    let p = chromium_profile(&cfg, "Profile 1");
    write_sized(&p.join("History"), 100);

    let mut a = audit(&cfg);
    let err = dispose(
        &cfg,
        &[s(&p.join("History"))],
        Acknowledged {
            signs_you_out: true,
            ..Acknowledged::default()
        },
        &mut a,
    )
    .unwrap_err();
    assert!(err.contains("history"));
    assert!(p.join("History").exists());
}

/// Each consequence is its own axis. Acknowledging one must not carry the
/// others, or the second checkbox is decoration.
#[test]
fn an_acknowledgement_for_one_consequence_does_not_authorise_another() {
    let (_d, cfg) = fixture();
    let p = chromium_profile(&cfg, "Profile 1");
    write_sized(&p.join("Cookies"), 100);
    write_sized(&p.join("Current Session"), 100);

    let mut a = audit(&cfg);
    let err = dispose(
        &cfg,
        &[s(&p.join("Cookies")), s(&p.join("Current Session"))],
        Acknowledged {
            signs_you_out: true,
            ..Acknowledged::default()
        },
        &mut a,
    )
    .unwrap_err();

    assert!(err.contains("tabs") || err.contains("session"));
    assert!(
        p.join("Cookies").exists(),
        "a partial run is never acceptable"
    );
    assert!(p.join("Current Session").exists());
}

/// A cache needs no acknowledgement: the browser rebuilds it and nothing the
/// user chose is lost.
#[test]
fn a_regenerable_row_needs_no_acknowledgement_at_all() {
    let (_d, cfg) = fixture();
    let p = chromium_profile(&cfg, "Profile 1");
    write_sized(&p.join("GPUCache/blob"), 100);

    let mut a = audit(&cfg);
    let summary = dispose(
        &cfg,
        &[s(&p.join("GPUCache"))],
        Acknowledged::default(),
        &mut a,
    )
    .unwrap();

    assert_eq!(summary.executed, 1);
    assert!(!p.join("GPUCache").exists());
}

// --- what disposal actually does -------------------------------------------

#[test]
fn selecting_a_row_disposes_of_its_sidecars_with_it() {
    let (_d, cfg) = fixture();
    let ff = firefox_profile(&cfg, "abc.default-release");
    write_sized(&ff.join("cookies.sqlite"), 100);
    write_sized(&ff.join("cookies.sqlite-wal"), 50);
    write_sized(&ff.join("cookies.sqlite-shm"), 20);

    let mut a = audit(&cfg);
    let summary = dispose(&cfg, &[s(&ff.join("cookies.sqlite"))], everything(), &mut a).unwrap();

    assert_eq!(summary.executed, 3, "one row, three names");
    assert!(!ff.join("cookies.sqlite").exists());
    assert!(!ff.join("cookies.sqlite-wal").exists());
    assert!(!ff.join("cookies.sqlite-shm").exists());
    // The precious neighbours are untouched, which is the whole point.
    assert!(ff.join("prefs.js").exists());
}

#[test]
fn every_disposal_is_a_move_to_the_trash_and_never_a_permanent_removal() {
    let (_d, cfg) = fixture();
    let ff = firefox_profile(&cfg, "abc.default-release");
    write_sized(&ff.join("cookies.sqlite"), 100);

    let mut a = audit(&cfg);
    dispose(&cfg, &[s(&ff.join("cookies.sqlite"))], everything(), &mut a).unwrap();

    assert!(cfg.home.join("FixtureTrash/cookies.sqlite").exists());
    let log = audit_text(&cfg);
    assert!(log.contains("Trash") || log.contains("trash"));
    assert!(!log.contains("Permanent"));
}

/// The log must say which acknowledgement authorized each row, or a run cannot
/// be reconstructed from it afterwards.
#[test]
fn the_audit_log_names_the_consequence_that_authorised_each_row() {
    let (_d, cfg) = fixture();
    let p = chromium_profile(&cfg, "Profile 1");
    write_sized(&p.join("Cookies"), 100);
    write_sized(&p.join("GPUCache/blob"), 100);

    let mut a = audit(&cfg);
    dispose(
        &cfg,
        &[s(&p.join("Cookies")), s(&p.join("GPUCache"))],
        everything(),
        &mut a,
    )
    .unwrap();

    let log = audit_text(&cfg);
    assert!(log.contains("privacy-cookies"));
    assert!(log.contains("privacy-cache"));
}

#[test]
fn a_selection_that_drifted_since_the_preview_is_refused() {
    let (_d, cfg) = fixture();
    let ff = firefox_profile(&cfg, "abc.default-release");
    write_sized(&ff.join("cookies.sqlite"), 100);

    let mut a = audit(&cfg);
    let err = dispose_privacy_with_sink(
        &cfg,
        &[s(&ff.join("cookies.sqlite"))],
        everything(),
        Some(Expected {
            count: 2,
            bytes: 100,
        }),
        true,
        &sink(&cfg),
        &mut a,
    )
    .unwrap_err();

    assert!(err.contains("not the one you confirmed"));
    assert!(ff.join("cookies.sqlite").exists());
}

/// One profile's selection can never reach into another's, and a row that is
/// offered in two profiles is two rows.
#[test]
fn a_selection_touches_only_the_profile_whose_row_was_named() {
    let (_d, cfg) = fixture();
    let one = chromium_profile(&cfg, "Profile 1");
    let two = chromium_profile(&cfg, "Profile 2");
    write_sized(&one.join("Cookies"), 100);
    write_sized(&two.join("Cookies"), 100);

    let mut a = audit(&cfg);
    dispose(&cfg, &[s(&one.join("Cookies"))], everything(), &mut a).unwrap();

    assert!(!one.join("Cookies").exists());
    assert!(two.join("Cookies").exists());
}

/// SAFETY CONTRACT item 5, on a brand-new entry point. Every browser cache row
/// is a *directory* action, so a recursive move to the Trash is the ordinary
/// case here rather than the exceptional one — and the only thing between a
/// user and an unconfirmed one is a single argument.
#[test]
fn a_directory_row_is_refused_without_the_mass_delete_confirmation() {
    let (_d, cfg) = fixture();
    let p = chromium_profile(&cfg, "Profile 1");
    write_sized(&p.join("GPUCache/blob"), 100);

    let mut a = audit(&cfg);
    let err = dispose_privacy_with_sink(
        &cfg,
        &[s(&p.join("GPUCache"))],
        Acknowledged::default(),
        None,
        false,
        &sink(&cfg),
        &mut a,
    )
    .unwrap_err();

    assert!(err.contains("confirmation"), "{err}");
    assert!(p.join("GPUCache/blob").exists());
    assert!(audit_text(&cfg).contains("refused"));
}

/// The same row with the confirmation given goes through, so the test above is
/// pinning the gate rather than a broken path.
#[test]
fn the_same_directory_row_goes_through_once_the_mass_delete_is_confirmed() {
    let (_d, cfg) = fixture();
    let p = chromium_profile(&cfg, "Profile 1");
    write_sized(&p.join("GPUCache/blob"), 100);

    let mut a = audit(&cfg);
    let summary = dispose(
        &cfg,
        &[s(&p.join("GPUCache"))],
        Acknowledged::default(),
        &mut a,
    )
    .unwrap();
    assert_eq!(summary.executed, 1);
    assert!(!p.join("GPUCache").exists());
}
