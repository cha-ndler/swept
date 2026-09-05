//! The Uninstaller at the command layer: the read-only report, and the
//! disposal whose ceiling is not a set of roots but the `offerable` rows of a
//! scan run inside the call.
//!
//! The rule tested throughout, as for Large & Old: **a partial run is never
//! acceptable.** If any part of the selection is not exactly an offerable row
//! of a fresh scan, nothing is touched. The load-bearing negatives are the ones
//! a frontend bug or a compromised webview would trip.
//!
//! SAFETY CONTRACT item 7: everything here runs against a throwaway tempdir.

use std::fs;
use std::path::{Path, PathBuf};

use swept_core::audit::AuditLog;
use swept_core::executor::DirSink;
use swept_core::plan::MASS_DELETE_COUNT;
use swept_core::uninstall::{
    Location, UninstallConfig, CONTAINER_STATE_PARTS, CONTAINER_USER_DATA_PARTS, SEARCHED_LOCATIONS,
};
use swept_gui_core::{
    dispose_leftovers_with_sink, installed_apps_in, uninstall_leftovers_in, CleanSummary, Expected,
    UninstallTarget,
};

// --- fixtures --------------------------------------------------------------

fn fixture() -> (tempfile::TempDir, UninstallConfig, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(dir.path()).unwrap();
    for loc in SEARCHED_LOCATIONS {
        fs::create_dir_all(home.join(loc.as_str())).unwrap();
    }
    fs::create_dir_all(home.join("Documents")).unwrap();
    let apps = home.join("FixtureApplications");
    fs::create_dir_all(&apps).unwrap();
    let mut cfg = UninstallConfig::new(home);
    cfg.app_roots = vec![apps.clone()];
    (dir, cfg, apps)
}

fn write_sized(path: &Path, bytes: u64) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, vec![0u8; bytes as usize]).unwrap();
}

fn install(apps: &Path, name: &str, id: &str) {
    let bundle = apps.join(format!("{name}.app"));
    fs::create_dir_all(bundle.join("Contents")).unwrap();
    fs::write(
        bundle.join("Contents/Info.plist"),
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
  <key>CFBundleIdentifier</key><string>{id}</string>
  <key>CFBundleName</key><string>{name}</string>
</dict></plist>"#
        ),
    )
    .unwrap();
}

/// A leftover directory with one file, at `<location>/<name>`.
fn leftover_dir(cfg: &UninstallConfig, loc: Location, name: &str, bytes: u64) -> PathBuf {
    let p = cfg.home.join(loc.as_str()).join(name);
    write_sized(&p.join("blob.bin"), bytes);
    p
}

fn leftover_file(cfg: &UninstallConfig, loc: Location, name: &str, bytes: u64) -> PathBuf {
    let p = cfg.home.join(loc.as_str()).join(name);
    write_sized(&p, bytes);
    p
}

fn container(cfg: &UninstallConfig, id: &str) -> PathBuf {
    let root = cfg.home.join(Location::Containers.as_str()).join(id);
    for part in CONTAINER_STATE_PARTS
        .iter()
        .chain(CONTAINER_USER_DATA_PARTS)
    {
        fs::create_dir_all(root.join("Data").join(part)).unwrap();
    }
    root
}

fn fill(container: &Path, part: &str, bytes: u64) -> PathBuf {
    let part = container.join("Data").join(part);
    write_sized(&part.join("blob.bin"), bytes);
    part
}

fn audit_at(cfg: &UninstallConfig) -> (PathBuf, AuditLog) {
    let p = cfg.home.join("audit.jsonl");
    let log = AuditLog::open(&p).unwrap();
    (p, log)
}

/// A flat trash directory: two selected rows sharing a basename would collide
/// on the rename and the second would be refused. The real sink handles that;
/// this one does not, so no test here selects two same-named rows in one
/// request. (Found by the safety reviewer probing the fixture, not the code.)
fn sink(cfg: &UninstallConfig) -> DirSink {
    DirSink {
        trash_dir: cfg.home.join("test-trash"),
    }
}

fn target(id: &str) -> UninstallTarget {
    UninstallTarget {
        id: id.to_string(),
        display_name: None,
    }
}

fn s(p: &Path) -> String {
    p.display().to_string()
}

/// Dispose with mass-delete confirmed, which every directory row needs.
fn dispose(
    cfg: &UninstallConfig,
    t: &UninstallTarget,
    paths: &[String],
    audit: &mut AuditLog,
) -> Result<CleanSummary, String> {
    dispose_leftovers_with_sink(cfg, t, paths, None, true, &sink(cfg), audit)
}

/// Every path under `root`, sorted, minus the audit log.
fn snapshot(cfg: &UninstallConfig) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn go(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            out.push(e.path());
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                go(&e.path(), out);
            }
        }
    }
    go(&cfg.home, &mut out);
    out.retain(|p| p.file_name().is_some_and(|n| n != "audit.jsonl"));
    out.sort();
    out
}

// --- the read-only half -----------------------------------------------------

#[test]
fn the_report_serializes_the_shape_the_ui_expects() {
    let (_g, cfg, _apps) = fixture();
    leftover_dir(&cfg, Location::Caches, "com.acme.App", 4_000);
    let root = container(&cfg, "com.acme.App");
    fill(&root, "Documents", 9_000);
    leftover_file(&cfg, Location::Preferences, "com.acme.App.plist", 100);

    let dto = uninstall_leftovers_in(&cfg, &target("com.acme.App")).unwrap();
    let v: serde_json::Value = serde_json::to_value(&dto).unwrap();

    assert_eq!(v["installed"], false);
    assert!(v["partial"].is_boolean());
    assert_eq!(v["offerable_count"], 2);
    let rows = v["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    for row in rows {
        assert!(
            row.get("selected").is_none(),
            "no row carries a selection state"
        );
        assert!(row["is_dir"].is_boolean());
        assert!(row["kind"].is_string());
        if row["offerable"] == false {
            assert!(row["withheld"].is_string(), "a withheld row says why");
        }
    }
    let user_data = rows.iter().find(|r| r["kind"] == "user_data").unwrap();
    assert_eq!(user_data["offerable"], false);
    assert!(v["caveats"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c.as_str().unwrap().contains("cfprefsd")));
}

#[test]
fn the_offerable_totals_are_computed_from_the_emitted_rows() {
    // Not from `LeftoverReport::total_bytes()`: if a row were ever dropped for
    // being unrepresentable, the header and the list would disagree.
    let (_g, cfg, _apps) = fixture();
    leftover_dir(&cfg, Location::Caches, "com.acme.App", 4_000);
    leftover_dir(&cfg, Location::Logs, "com.acme.App", 1_000);
    let root = container(&cfg, "com.acme.App");
    fill(&root, "Library/Application Support", 9_000);

    let dto = uninstall_leftovers_in(&cfg, &target("com.acme.App")).unwrap();

    let from_rows: u64 = dto
        .rows
        .iter()
        .filter(|r| r.offerable)
        .map(|r| r.size_bytes)
        .sum();
    assert_eq!(dto.offerable_bytes, from_rows);
    assert_eq!(dto.offerable_bytes, 5_000, "user data is not in the total");
    assert_eq!(dto.withheld_count, 1);
}

#[test]
fn an_installed_app_reports_installed_and_no_rows() {
    let (_g, cfg, apps) = fixture();
    install(&apps, "App", "com.acme.App");
    leftover_dir(&cfg, Location::Caches, "com.acme.App", 4_000);

    let dto = uninstall_leftovers_in(&cfg, &target("com.acme.App")).unwrap();

    assert!(dto.installed);
    assert_eq!(dto.installed_at.len(), 1);
    assert!(dto.rows.is_empty());
}

#[test]
fn an_unusable_id_or_name_is_refused_before_anything_is_read() {
    let (_g, cfg, _apps) = fixture();
    assert!(uninstall_leftovers_in(&cfg, &target("com.acme.*")).is_err());
    assert!(uninstall_leftovers_in(&cfg, &target("")).is_err());
    let bad_name = UninstallTarget {
        id: "com.acme.App".into(),
        display_name: Some("a/b".into()),
    };
    assert!(uninstall_leftovers_in(&cfg, &bad_name).is_err());
}

#[test]
fn the_app_picker_lists_top_level_bundles_only_sorted_by_name() {
    // Helpers nested inside an app are in the inventory — they must be, to
    // withhold their data — but they are not things a person removes, so they
    // are not choices. And an unreadable root refuses, as the inventory does.
    let (_g, cfg, apps) = fixture();
    install(&apps, "Zed Notes", "com.acme.Notes");
    install(&apps, "acme Reader", "com.acme.Reader");
    let helper = apps.join("Zed Notes.app/Contents/Library/LoginItems");
    install(&helper, "Helper", "com.acme.Notes.Helper");

    let list = installed_apps_in(&cfg).unwrap();

    assert_eq!(
        list.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
        vec!["acme Reader", "Zed Notes"],
        "top-level only, case-insensitively by name"
    );
    assert_eq!(list[1].id, "com.acme.Notes");
    assert!(list[1].bundle_path.ends_with("Zed Notes.app"));

    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&apps, fs::Permissions::from_mode(0o000)).unwrap();
    let result = installed_apps_in(&cfg);
    fs::set_permissions(&apps, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(result.is_err(), "an unreadable root is not an empty list");
}

// --- disposal: the positive, so the negatives cannot pass vacuously ---------

#[test]
fn a_selected_offerable_directory_is_trashed_as_one_unit() {
    let (_g, cfg, _apps) = fixture();
    let dir = leftover_dir(&cfg, Location::Caches, "com.acme.App", 4_000);
    write_sized(&dir.join("nested/deeper/more.bin"), 500);
    let (_p, mut audit) = audit_at(&cfg);

    let dto = uninstall_leftovers_in(&cfg, &target("com.acme.App")).unwrap();
    assert!(dto.rows[0].is_dir);
    let summary = dispose(&cfg, &target("com.acme.App"), &[s(&dir)], &mut audit).unwrap();

    assert_eq!(summary.executed, 1);
    assert_eq!(summary.refused, 0);
    assert_eq!(summary.bytes_freed, 4_500);
    assert!(summary.entries_freed >= 4, "the tree, counted by name");
    assert!(!dir.exists());
    assert!(cfg
        .home
        .join("test-trash/com.acme.App/nested/deeper/more.bin")
        .exists());
}

#[test]
fn a_selected_offerable_file_is_trashed() {
    let (_g, cfg, _apps) = fixture();
    let plist = leftover_file(&cfg, Location::Preferences, "com.acme.App.plist", 100);
    let (_p, mut audit) = audit_at(&cfg);

    let summary = dispose(&cfg, &target("com.acme.App"), &[s(&plist)], &mut audit).unwrap();

    assert_eq!(summary.executed, 1);
    assert_eq!(summary.bytes_freed, 100);
    assert_eq!(summary.entries_freed, 0);
    assert!(!plist.exists());
}

#[test]
fn a_bulk_ungrantable_row_is_still_individually_disposable() {
    // The flag governs a select-all gesture in the UI. Enforcing it here would
    // break the individual selection it exists to require.
    let (_g, cfg, _apps) = fixture();
    let helper = leftover_dir(&cfg, Location::Caches, "com.acme.App.Helper", 4_000);
    let (_p, mut audit) = audit_at(&cfg);

    let dto = uninstall_leftovers_in(&cfg, &target("com.acme.App")).unwrap();
    assert!(dto.rows[0].offerable && !dto.rows[0].bulk_grantable);
    let summary = dispose(&cfg, &target("com.acme.App"), &[s(&helper)], &mut audit).unwrap();

    assert_eq!(summary.executed, 1);
}

#[test]
fn a_name_keyed_row_is_disposable_only_with_the_name_that_offered_it() {
    let (_g, cfg, _apps) = fixture();
    let dir = cfg
        .home
        .join(Location::ApplicationSupport.as_str())
        .join("Acme Notes");
    write_sized(&dir.join("com.acme.Notes.plist"), 100);
    let (_p, mut audit) = audit_at(&cfg);

    // Without the name the fresh scan does not offer it, so it is refused.
    assert!(dispose(&cfg, &target("com.acme.Notes"), &[s(&dir)], &mut audit).is_err());
    assert!(dir.exists());

    let named = UninstallTarget {
        id: "com.acme.Notes".into(),
        display_name: Some("Acme Notes".into()),
    };
    let summary = dispose(&cfg, &named, &[s(&dir)], &mut audit).unwrap();
    assert_eq!(summary.executed, 1);
    assert!(!dir.exists());
}

// --- the ceiling is the fresh scan's offerable rows ------------------------

#[test]
fn a_container_root_in_the_request_refuses_the_whole_request() {
    // [LB] Never a row, so never offerable, so refused — and the offerable
    // sibling part beside it is untouched because the request is refused
    // wholesale.
    let (_g, cfg, _apps) = fixture();
    let root = container(&cfg, "com.acme.App");
    let caches = fill(&root, "Library/Caches", 1_000);
    let (audit_path, mut audit) = audit_at(&cfg);
    let before = snapshot(&cfg);

    let err = dispose(
        &cfg,
        &target("com.acme.App"),
        &[s(&caches), s(&root)],
        &mut audit,
    )
    .unwrap_err();

    assert!(err.contains("not something this scan offers"), "{err}");
    assert_eq!(snapshot(&cfg), before);
    assert!(fs::read_to_string(&audit_path).unwrap().contains("refused"));
}

#[test]
fn a_documents_row_inside_a_container_refuses_the_whole_request() {
    // [LB] The headline case from the module doc: inside a location root, and
    // must never be acted on. Root confinement would have let it through.
    let (_g, cfg, _apps) = fixture();
    let root = container(&cfg, "com.acme.App");
    let documents = fill(&root, "Documents", 9_000);
    let caches = fill(&root, "Library/Caches", 1_000);
    let (_p, mut audit) = audit_at(&cfg);
    let before = snapshot(&cfg);

    let dto = uninstall_leftovers_in(&cfg, &target("com.acme.App")).unwrap();
    assert!(dto
        .rows
        .iter()
        .any(|r| r.path == s(&documents) && !r.offerable));

    assert!(dispose(
        &cfg,
        &target("com.acme.App"),
        &[s(&caches), s(&documents)],
        &mut audit
    )
    .is_err());
    assert_eq!(snapshot(&cfg), before);
    assert!(documents.join("blob.bin").exists());
}

#[test]
fn a_group_container_refuses_the_whole_request() {
    // [LB] Shown as shared, never claimable.
    let (_g, cfg, _apps) = fixture();
    let group = cfg
        .home
        .join(Location::GroupContainers.as_str())
        .join("group.com.acme.App");
    write_sized(&group.join("shared.db"), 4_000);
    let (_p, mut audit) = audit_at(&cfg);

    assert!(dispose(&cfg, &target("com.acme.App"), &[s(&group)], &mut audit).is_err());
    assert!(group.join("shared.db").exists());
}

#[test]
fn a_withheld_row_refuses_the_whole_request() {
    // [LB] A still-installed sibling's data is shown and withheld; sending it
    // anyway is the frontend asking for something the user was told it could
    // not have.
    let (_g, cfg, apps) = fixture();
    install(&apps, "Reader", "com.acme.Suite.Reader");
    let reader = leftover_dir(&cfg, Location::Caches, "com.acme.Suite.Reader", 4_000);
    let suite = leftover_dir(&cfg, Location::Caches, "com.acme.Suite", 1_000);
    let (_p, mut audit) = audit_at(&cfg);

    assert!(dispose(
        &cfg,
        &target("com.acme.Suite"),
        &[s(&suite), s(&reader)],
        &mut audit
    )
    .is_err());
    assert!(reader.join("blob.bin").exists());
    assert!(suite.join("blob.bin").exists(), "refused wholesale");
}

#[test]
fn an_undisposable_row_refuses_the_whole_request() {
    // [LB] A tree with a `.git` inside is withheld by discovery, so it is not
    // offerable, so it is refused here — before `guard_dir` would have refused
    // it again. Asserted at this layer so a regression in the flag is not
    // masked by the kernel.
    let (_g, cfg, _apps) = fixture();
    let dir = leftover_dir(&cfg, Location::Caches, "com.acme.App", 4_000);
    write_sized(&dir.join("vendor/.git/HEAD"), 10);
    let (_p, mut audit) = audit_at(&cfg);

    let err = dispose(&cfg, &target("com.acme.App"), &[s(&dir)], &mut audit).unwrap_err();

    assert!(err.contains("not something this scan offers"), "{err}");
    assert!(dir.join("vendor/.git/HEAD").exists());
}

#[test]
fn a_path_outside_the_leftover_locations_refuses_the_whole_request() {
    // [LB] A real, guardable, perfectly ordinary file in ~/Documents —
    // exactly the thing the ceiling exists for.
    let (_g, cfg, _apps) = fixture();
    let precious = cfg.home.join("Documents/thesis.pages");
    write_sized(&precious, 9_000);
    let caches = leftover_dir(&cfg, Location::Caches, "com.acme.App", 1_000);
    let (_p, mut audit) = audit_at(&cfg);

    assert!(dispose(
        &cfg,
        &target("com.acme.App"),
        &[s(&caches), s(&precious)],
        &mut audit
    )
    .is_err());
    assert!(precious.exists());
    assert!(caches.exists());
}

#[test]
fn a_child_of_an_offered_row_is_not_itself_disposable() {
    // The intersection is exact: a path *inside* an offered directory was
    // never a row, so it is refused, and no nesting rule is needed to say so.
    let (_g, cfg, _apps) = fixture();
    let dir = leftover_dir(&cfg, Location::Caches, "com.acme.App", 4_000);
    let (_p, mut audit) = audit_at(&cfg);

    assert!(dispose(
        &cfg,
        &target("com.acme.App"),
        &[s(&dir.join("blob.bin"))],
        &mut audit
    )
    .is_err());
    assert!(dir.join("blob.bin").exists());
}

#[test]
fn a_non_canonical_spelling_of_an_offered_row_is_refused() {
    // [LB] Byte equality, not `Path` equality: `Path` compares component-wise,
    // so `<caches>/./com.acme.App` would pass a `==` on paths.
    let (_g, cfg, _apps) = fixture();
    let dir = leftover_dir(&cfg, Location::Caches, "com.acme.App", 4_000);
    let (_p, mut audit) = audit_at(&cfg);

    let dotted = format!(
        "{}/./com.acme.App",
        cfg.home.join(Location::Caches.as_str()).display()
    );
    assert!(Path::new(&dotted) == dir.as_path(), "the trap is real");
    assert!(dispose(&cfg, &target("com.acme.App"), &[dotted], &mut audit).is_err());
    assert!(dir.exists());
}

#[test]
fn a_row_replaced_by_a_symlink_refuses_the_whole_request() {
    // [LB] The fresh scan drops symlinks, so it is not offerable; a swap that
    // happened since the sheet was shown redirects nothing.
    let (_g, cfg, _apps) = fixture();
    let elsewhere = cfg.home.join("Documents/precious");
    write_sized(&elsewhere.join("thesis.pages"), 9_000);
    let dir = leftover_dir(&cfg, Location::Caches, "com.acme.App", 4_000);
    let (_p, mut audit) = audit_at(&cfg);

    fs::rename(&dir, cfg.home.join("moved-aside")).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &dir).unwrap();

    assert!(dispose(&cfg, &target("com.acme.App"), &[s(&dir)], &mut audit).is_err());
    assert!(elsewhere.join("thesis.pages").exists());
}

#[test]
fn the_app_being_installed_at_disposal_time_refuses_everything() {
    // [LB] Installed ⇒ no rows ⇒ nothing is offerable ⇒ refused. A scan from
    // before the app came back does not carry.
    let (_g, cfg, apps) = fixture();
    let dir = leftover_dir(&cfg, Location::Caches, "com.acme.App", 4_000);
    let (_p, mut audit) = audit_at(&cfg);

    let before = uninstall_leftovers_in(&cfg, &target("com.acme.App")).unwrap();
    assert_eq!(before.offerable_count, 1);
    install(&apps, "App", "com.acme.App");

    assert!(dispose(&cfg, &target("com.acme.App"), &[s(&dir)], &mut audit).is_err());
    assert!(dir.exists());
}

#[test]
fn an_unreadable_app_root_refuses_the_disposal() {
    // [LB] "I could not tell whether it is still installed" must refuse, not
    // proceed.
    use std::os::unix::fs::PermissionsExt;
    let (_g, cfg, apps) = fixture();
    let dir = leftover_dir(&cfg, Location::Caches, "com.acme.App", 4_000);
    let (audit_path, mut audit) = audit_at(&cfg);
    fs::set_permissions(&apps, fs::Permissions::from_mode(0o000)).unwrap();

    let result = dispose(&cfg, &target("com.acme.App"), &[s(&dir)], &mut audit);

    fs::set_permissions(&apps, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(result.is_err());
    assert!(dir.exists());
    assert!(fs::read_to_string(&audit_path)
        .unwrap()
        .contains("still installed"));
}

#[test]
fn a_row_that_vanished_between_scan_and_disposal_refuses_the_whole_request() {
    let (_g, cfg, _apps) = fixture();
    let a = leftover_dir(&cfg, Location::Caches, "com.acme.App", 4_000);
    let b = leftover_dir(&cfg, Location::Logs, "com.acme.App", 1_000);
    let (_p, mut audit) = audit_at(&cfg);
    fs::remove_dir_all(&b).unwrap();

    assert!(dispose(&cfg, &target("com.acme.App"), &[s(&a), s(&b)], &mut audit).is_err());
    assert!(a.exists(), "refused wholesale");
}

// --- consent bounds ---------------------------------------------------------

#[test]
fn a_directory_row_needs_the_mass_delete_confirmation_however_small() {
    // [LB] Any directory action is a recursive removal, and item 5 says those
    // need confirmation — the numbers are not consulted first.
    let (_g, cfg, _apps) = fixture();
    let dir = leftover_dir(&cfg, Location::Caches, "com.acme.App", 100);
    let (audit_path, mut audit) = audit_at(&cfg);

    let err = dispose_leftovers_with_sink(
        &cfg,
        &target("com.acme.App"),
        &[s(&dir)],
        None,
        false,
        &sink(&cfg),
        &mut audit,
    )
    .unwrap_err();

    assert!(err.contains("confirmation"), "{err}");
    assert!(dir.exists());
    assert!(fs::read_to_string(&audit_path)
        .unwrap()
        .contains("needs explicit confirmation"));

    let ok = dispose(&cfg, &target("com.acme.App"), &[s(&dir)], &mut audit).unwrap();
    assert_eq!(ok.executed, 1);
}

#[test]
fn the_selection_must_match_what_was_confirmed() {
    let (_g, cfg, _apps) = fixture();
    let a = leftover_dir(&cfg, Location::Caches, "com.acme.App", 4_000);
    let b = leftover_dir(&cfg, Location::Logs, "com.acme.App", 1_000);
    let (_p, mut audit) = audit_at(&cfg);

    // Confirmed one row, sent two.
    let err = dispose_leftovers_with_sink(
        &cfg,
        &target("com.acme.App"),
        &[s(&a), s(&b)],
        Some(Expected {
            count: 1,
            bytes: 5_000,
        }),
        true,
        &sink(&cfg),
        &mut audit,
    )
    .unwrap_err();
    assert!(err.contains("not the one you confirmed"), "{err}");
    assert!(a.exists() && b.exists());

    // Confirmed two rows at a size that has since grown far past tolerance.
    write_sized(&a.join("grown.bin"), 8 * 1024 * 1024);
    let err = dispose_leftovers_with_sink(
        &cfg,
        &target("com.acme.App"),
        &[s(&a), s(&b)],
        Some(Expected {
            count: 2,
            bytes: 5_000,
        }),
        true,
        &sink(&cfg),
        &mut audit,
    )
    .unwrap_err();
    assert!(err.contains("grew"), "{err}");
    assert!(a.exists() && b.exists());
}

#[test]
fn the_same_row_twice_counts_once() {
    let (_g, cfg, _apps) = fixture();
    let dir = leftover_dir(&cfg, Location::Caches, "com.acme.App", 4_000);
    let (_p, mut audit) = audit_at(&cfg);

    let summary = dispose_leftovers_with_sink(
        &cfg,
        &target("com.acme.App"),
        &[s(&dir), s(&dir)],
        Some(Expected {
            count: 1,
            bytes: 4_000,
        }),
        true,
        &sink(&cfg),
        &mut audit,
    )
    .unwrap();

    assert_eq!(summary.executed, 1);
    assert_eq!(summary.refused, 0);
}

#[test]
fn an_empty_selection_acts_on_nothing() {
    let (_g, cfg, _apps) = fixture();
    leftover_dir(&cfg, Location::Caches, "com.acme.App", 4_000);
    let (audit_path, mut audit) = audit_at(&cfg);
    let before = snapshot(&cfg);

    assert!(dispose(&cfg, &target("com.acme.App"), &[], &mut audit).is_err());
    assert_eq!(snapshot(&cfg), before);
    assert!(fs::read_to_string(&audit_path)
        .unwrap()
        .contains("nothing was selected"));
}

#[test]
fn a_refused_request_leaves_the_disk_untouched_and_a_record_behind() {
    // [LB] Every refusal path above, in one place: the fixture is byte-for-byte
    // the same afterwards, and the log grew by at least one refusal each time.
    let (_g, cfg, apps) = fixture();
    install(&apps, "Reader", "com.acme.Suite.Reader");
    let suite = leftover_dir(&cfg, Location::Caches, "com.acme.Suite", 1_000);
    let reader = leftover_dir(&cfg, Location::Caches, "com.acme.Suite.Reader", 4_000);
    let root = container(&cfg, "com.acme.Suite");
    let documents = fill(&root, "Documents", 9_000);
    let precious = cfg.home.join("Documents/thesis.pages");
    write_sized(&precious, 9_000);
    let (audit_path, mut audit) = audit_at(&cfg);
    let before = snapshot(&cfg);

    let attempts: Vec<Vec<String>> = vec![
        vec![s(&reader)],
        vec![s(&suite), s(&root)],
        vec![s(&suite), s(&documents)],
        vec![s(&suite), s(&precious)],
        vec![s(&suite.join("blob.bin"))],
        vec![],
    ];
    let mut refusals = 0usize;
    for paths in &attempts {
        let lines_before = fs::read_to_string(&audit_path).unwrap().lines().count();
        assert!(
            dispose(&cfg, &target("com.acme.Suite"), paths, &mut audit).is_err(),
            "{paths:?}"
        );
        let lines_after = fs::read_to_string(&audit_path).unwrap().lines().count();
        assert!(lines_after > lines_before, "{paths:?} left no record");
        refusals += 1;
    }
    assert_eq!(refusals, attempts.len());
    assert_eq!(snapshot(&cfg), before);
}

#[test]
fn the_disposal_is_never_permanent() {
    let (_g, cfg, _apps) = fixture();
    let dir = leftover_dir(&cfg, Location::Caches, "com.acme.App", 4_000);
    let plist = leftover_file(&cfg, Location::Preferences, "com.acme.App.plist", 100);
    let (audit_path, mut audit) = audit_at(&cfg);

    dispose(
        &cfg,
        &target("com.acme.App"),
        &[s(&dir), s(&plist)],
        &mut audit,
    )
    .unwrap();

    let log = fs::read_to_string(&audit_path).unwrap();
    assert!(!log.contains("\"disposition\":\"permanent\""));
    assert!(log.contains("user-granted directory"));
    assert!(log.contains("user-granted path"));
    assert!(cfg.home.join("test-trash/com.acme.App/blob.bin").exists());
    assert!(cfg.home.join("test-trash/com.acme.App.plist").exists());
}

#[test]
fn nothing_here_widens_the_disposal_allowlist() {
    // [LB] The uninstaller disposes by grant. If a leftover location ever
    // leaked into `default_roots`, everything there would become sweepable
    // without one.
    let (_g, cfg, _apps) = fixture();
    assert_eq!(
        safety::allowlist::default_roots(&cfg.home),
        vec![
            cfg.home.join("Library/Caches"),
            cfg.home.join("Library/Logs"),
            cfg.home.join("Library/Developer/Xcode/DerivedData"),
            cfg.home.join(".Trash"),
        ]
    );
}

// --- from the safety review ------------------------------------------------

#[test]
fn a_container_application_support_row_refuses_the_whole_request() {
    // [LB] The second user-data part. Pinned read-side already; this is the
    // disposal side, where it matters.
    let (_g, cfg, _apps) = fixture();
    let root = container(&cfg, "com.acme.App");
    let support = fill(&root, "Library/Application Support", 9_000);
    let caches = fill(&root, "Library/Caches", 1_000);
    let (_p, mut audit) = audit_at(&cfg);

    assert!(dispose(
        &cfg,
        &target("com.acme.App"),
        &[s(&caches), s(&support)],
        &mut audit
    )
    .is_err());
    assert!(support.join("blob.bin").exists());
    assert!(caches.join("blob.bin").exists(), "refused wholesale");
}

#[test]
fn other_spellings_of_an_offered_row_are_refused() {
    // [LB] A trailing slash (what a JS path join adds), a `..` hop, and a case
    // variant (indistinguishable from the row by `canonicalize` on a
    // case-insensitive volume). Each is byte-different from the row, so each
    // is refused at the intersection — before `guard` is ever reached.
    let (_g, cfg, _apps) = fixture();
    let dir = leftover_dir(&cfg, Location::Caches, "com.acme.App", 4_000);
    let (_p, mut audit) = audit_at(&cfg);
    let caches = cfg.home.join(Location::Caches.as_str());

    let spellings = [
        format!("{}/", dir.display()),
        format!("{}/../com.acme.App", dir.display()),
        format!("{}/COM.ACME.APP", caches.display()),
    ];
    for raw in spellings {
        let err = dispose(
            &cfg,
            &target("com.acme.App"),
            std::slice::from_ref(&raw),
            &mut audit,
        )
        .unwrap_err();
        assert!(
            err.contains("not something this scan offers"),
            "{raw}: {err}"
        );
        assert!(dir.join("blob.bin").exists(), "{raw}");
    }
}

#[test]
fn a_file_only_selection_does_not_need_the_mass_delete_confirmation() {
    // The deliberate asymmetry: a directory always asks; a few files do not.
    let (_g, cfg, _apps) = fixture();
    let plist = leftover_file(&cfg, Location::Preferences, "com.acme.App.plist", 100);
    let (_p, mut audit) = audit_at(&cfg);

    let summary = dispose_leftovers_with_sink(
        &cfg,
        &target("com.acme.App"),
        &[s(&plist)],
        None,
        false,
        &sink(&cfg),
        &mut audit,
    )
    .unwrap();

    assert_eq!(summary.executed, 1);
    assert!(!plist.exists());
}

#[test]
fn many_file_rows_still_need_the_mass_delete_confirmation() {
    // [LB] The count threshold, at this layer, for file rows. ByHost is the
    // one location where one id yields many file rows.
    let (_g, cfg, _apps) = fixture();
    let mut paths = Vec::new();
    for i in 0..(MASS_DELETE_COUNT + 1) {
        let name = format!("com.acme.App.00000000-0000-0000-0000-{i:012x}.plist");
        paths.push(s(&leftover_file(
            &cfg,
            Location::PreferencesByHost,
            &name,
            10,
        )));
    }
    let (_p, mut audit) = audit_at(&cfg);

    let err = dispose_leftovers_with_sink(
        &cfg,
        &target("com.acme.App"),
        &paths,
        None,
        false,
        &sink(&cfg),
        &mut audit,
    )
    .unwrap_err();
    assert!(err.contains("confirmation"), "{err}");
    assert!(Path::new(&paths[0]).exists(), "nothing touched");

    let ok = dispose(&cfg, &target("com.acme.App"), &paths, &mut audit).unwrap();
    assert_eq!(ok.executed, MASS_DELETE_COUNT + 1);
}

#[test]
fn a_non_canonical_home_refuses_the_disposal() {
    // [LB] The denylist's home-relative rules — Keychains, Mail, the home root
    // — compare component-wise against the home, so a non-canonical spelling
    // silently disables all three for the whole run. Found by the safety
    // reviewer with a probe. This seam refuses rather than substitutes,
    // because the scan and `resolved_locations` read the same field and a
    // substitution here would leave them disagreeing.
    let (_g, cfg, _apps) = fixture();
    let dir = leftover_dir(&cfg, Location::Caches, "com.acme.App", 4_000);
    let other = tempfile::tempdir().unwrap();
    let link = other.path().join("home-link");
    std::os::unix::fs::symlink(&cfg.home, &link).unwrap();
    let mut aliased = UninstallConfig::new(link);
    aliased.app_roots = cfg.app_roots.clone();
    let (audit_path, mut audit) = audit_at(&cfg);

    let err = dispose(&aliased, &target("com.acme.App"), &[s(&dir)], &mut audit).unwrap_err();

    assert!(err.contains("canonical"), "{err}");
    assert!(dir.join("blob.bin").exists());
    assert!(fs::read_to_string(&audit_path)
        .unwrap()
        .contains("canonical"));
}

#[test]
fn a_refusal_echoes_only_a_bounded_slice_of_a_frontend_string() {
    // The reason lands in the append-only audit log, which is never rotated.
    // A webview must not be able to fill the disk one refusal at a time.
    let (_g, cfg, _apps) = fixture();
    leftover_dir(&cfg, Location::Caches, "com.acme.App", 4_000);
    let (audit_path, mut audit) = audit_at(&cfg);
    let huge = "x".repeat(200 * 1024);

    let err = dispose(
        &cfg,
        &target("com.acme.App"),
        &[huge.clone(), huge.clone(), huge.clone()],
        &mut audit,
    )
    .unwrap_err();
    assert!(err.len() < 2_000, "{} bytes echoed", err.len());

    let bad_id = UninstallTarget {
        id: huge.clone(),
        display_name: None,
    };
    let err = dispose(&cfg, &bad_id, &["/nowhere".into()], &mut audit).unwrap_err();
    assert!(err.len() < 2_000, "{} bytes echoed", err.len());

    let bad_name = UninstallTarget {
        id: "com.acme.App".into(),
        display_name: Some(format!("a/{huge}")),
    };
    let err = dispose(&cfg, &bad_name, &["/nowhere".into()], &mut audit).unwrap_err();
    assert!(err.len() < 2_000, "{} bytes echoed", err.len());

    let log = fs::read_to_string(&audit_path).unwrap();
    assert!(
        log.lines().all(|l| l.len() < 4_000),
        "a refusal record is unbounded"
    );
}
