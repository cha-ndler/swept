//! Uninstaller leftover discovery.
//!
//! The properties here are ranked, and the ranking is the point: **not claiming
//! a still-installed application's data matters more than finding leftovers at
//! all.** A scan that misses something under-reports, which is recoverable by
//! looking again. A scan that offers somebody else's live data is how a user
//! loses a licence, a database, or a document — and nothing downstream would
//! object, because the denylist has no opinion about who owns
//! `~/Library/Caches/com.acme.Notes`.
//!
//! So the negative tests below carry more weight than the positive ones, and
//! each is written so that deleting its rule makes it fail.
//!
//! SAFETY CONTRACT item 7: everything here runs against a throwaway tempdir.

use std::fs;
use std::path::{Path, PathBuf};

use macclean_core::uninstall::{
    inventory, inventory_roots, leftovers_for, owner_index, BundleId, Location, MatchedVia,
    Residence, UninstallConfig, UninstallError, DEFERRED_LOCATIONS, SEARCHED_LOCATIONS,
};

// --- fixtures --------------------------------------------------------------

/// A fake home with every location this half searches, plus a fake apps root
/// that stands in for `/Applications`.
fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(dir.path()).unwrap();
    for loc in SEARCHED_LOCATIONS {
        fs::create_dir_all(home.join(loc.as_str())).unwrap();
    }
    let apps = home.join("FixtureApplications");
    fs::create_dir_all(&apps).unwrap();
    (dir, home, apps)
}

fn cfg(home: &Path, apps: &Path) -> UninstallConfig {
    let mut c = UninstallConfig::new(home.to_path_buf());
    c.app_roots = vec![apps.to_path_buf()];
    c
}

fn info_plist(id: &str, name: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleIdentifier</key><string>{id}</string>
  <key>CFBundleName</key><string>{name}</string>
</dict></plist>"#
    )
}

/// An installed `.app` at `root/<name>.app` declaring `id`.
fn install(root: &Path, name: &str, id: &str) -> PathBuf {
    let bundle = root.join(format!("{name}.app"));
    fs::create_dir_all(bundle.join("Contents")).unwrap();
    fs::write(bundle.join("Contents/Info.plist"), info_plist(id, name)).unwrap();
    bundle
}

fn write_file(path: &Path, bytes: u64) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, vec![0u8; bytes as usize]).unwrap();
}

/// A leftover directory with one file in it, at `home/<location>/<name>`.
fn leftover_dir(home: &Path, loc: Location, name: &str, bytes: u64) -> PathBuf {
    let p = home.join(loc.as_str()).join(name);
    write_file(&p.join("blob.bin"), bytes);
    p
}

fn leftover_file(home: &Path, loc: Location, name: &str, bytes: u64) -> PathBuf {
    let p = home.join(loc.as_str()).join(name);
    write_file(&p, bytes);
    p
}

fn id(s: &str) -> BundleId {
    BundleId::parse(s).unwrap_or_else(|| panic!("{s} should be a valid bundle id"))
}

fn paths(report: &macclean_core::uninstall::LeftoverReport) -> Vec<String> {
    report
        .rows
        .iter()
        .map(|r| r.path.file_name().unwrap().to_string_lossy().into_owned())
        .collect()
}

/// Every path in a tree, for the "nothing was mutated" comparison.
fn snapshot(root: &Path) -> Vec<PathBuf> {
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
    go(root, &mut out);
    out.sort();
    out
}

// --- the mandated negative -------------------------------------------------

#[test]
fn a_longer_id_is_not_claimed_by_its_shorter_prefix() {
    let (_g, home, apps) = fixture();
    // `com.acme.Note` is a byte-prefix of `com.acme.Notes`. A `starts_with`
    // matcher hands the second app's cache, cookies and preferences to the
    // first, and every check downstream passes — the denylist has no opinion
    // about who owns a directory in `~/Library/Caches`.
    leftover_dir(&home, Location::Caches, "com.acme.Note", 4_000);
    leftover_dir(&home, Location::Caches, "com.acme.Notes", 40_000);
    leftover_file(&home, Location::Preferences, "com.acme.Notes.plist", 900);
    leftover_file(
        &home,
        Location::HttpStorages,
        "com.acme.Notes.binarycookies",
        900,
    );

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.Note")).unwrap();

    assert_eq!(
        paths(&report),
        vec!["com.acme.Note"],
        "only the target's own directory may be claimed"
    );
}

// --- structural ------------------------------------------------------------

#[test]
fn every_emitted_path_is_canonical_and_confined_to_its_location_root() {
    let (_g, home, apps) = fixture();
    leftover_dir(&home, Location::Caches, "com.acme.App", 4_000);
    leftover_file(&home, Location::Preferences, "com.acme.App.plist", 900);

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();

    assert!(!report.rows.is_empty());
    for row in &report.rows {
        assert_eq!(
            fs::canonicalize(&row.path).unwrap(),
            row.path,
            "{} is not already its own canonical spelling",
            row.path.display()
        );
        let root = home.join(row.location.as_str());
        assert_eq!(
            row.path.parent(),
            Some(root.as_path()),
            "{} escaped its location root",
            row.path.display()
        );
    }
}

#[test]
fn no_location_root_is_ever_emitted_as_a_row() {
    let (_g, home, apps) = fixture();
    leftover_dir(&home, Location::Caches, "com.acme.App", 4_000);

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();

    for loc in SEARCHED_LOCATIONS {
        let root = home.join(loc.as_str());
        assert!(
            !report.rows.iter().any(|r| r.path == root),
            "{} was offered as a row",
            root.display()
        );
    }
}

#[test]
fn the_scan_mutates_nothing() {
    let (_g, home, apps) = fixture();
    install(&apps, "Other", "com.other.App");
    leftover_dir(&home, Location::Caches, "com.acme.App", 4_000);
    leftover_file(&home, Location::LaunchAgents, "com.acme.App.plist", 400);

    let before = snapshot(&home);
    let _ = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();
    let after = snapshot(&home);

    assert_eq!(before, after, "the scan changed the filesystem");
}

#[test]
fn the_leftover_location_list_is_pinned() {
    // A canary, mirroring `the_disposal_scope_is_pinned` in the safety kernel.
    // Adding a location is a deliberate act, not something that happens while
    // editing something else.
    let names: Vec<&str> = SEARCHED_LOCATIONS.iter().map(|l| l.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "Library/Caches",
            "Library/HTTPStorages",
            "Library/WebKit",
            "Library/Preferences",
            "Library/Preferences/ByHost",
            "Library/Saved Application State",
            "Library/LaunchAgents",
            "Library/Logs",
            "Library/Application Support",
        ]
    );
}

#[test]
fn finding_leftovers_does_not_widen_the_disposal_scope() {
    let (_g, home, _apps) = fixture();
    // This module reads far more than the app may clean unattended. If a
    // location ever leaked into `default_roots`, everything here would become
    // sweepable without a per-path grant.
    let disposal = safety::allowlist::default_roots(&home);
    assert_eq!(
        disposal,
        vec![
            home.join("Library/Caches"),
            home.join("Library/Logs"),
            home.join("Library/Developer/Xcode/DerivedData"),
            home.join(".Trash"),
        ]
    );
}

#[test]
fn every_report_names_what_it_did_not_search() {
    let (_g, home, apps) = fixture();
    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();

    // Containers, Group Containers and the human-name tier are not searched by
    // this half. A report that did not say so would read as "this is
    // everything", which is the one thing it is not.
    assert_eq!(report.deferred.len(), DEFERRED_LOCATIONS.len());
    assert!(report
        .deferred
        .iter()
        .any(|(p, _)| p.contains("Containers")));
    for (_, reason) in report.deferred {
        assert!(!reason.is_empty(), "a deferral must say why");
    }
}

#[test]
fn rows_are_ordered_deterministically() {
    let (_g, home, apps) = fixture();
    leftover_dir(&home, Location::Logs, "com.acme.App", 1_000);
    leftover_dir(&home, Location::Caches, "com.acme.App", 2_000);
    leftover_file(&home, Location::Preferences, "com.acme.App.plist", 300);

    let a = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();
    let b = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();

    let locs: Vec<Location> = a.rows.iter().map(|r| r.location).collect();
    let mut sorted = locs.clone();
    sorted.sort();
    assert_eq!(locs, sorted, "rows must be ordered by location");
    assert_eq!(
        a.rows.iter().map(|r| &r.path).collect::<Vec<_>>(),
        b.rows.iter().map(|r| &r.path).collect::<Vec<_>>()
    );
}

// --- ownership -------------------------------------------------------------

#[test]
fn a_sibling_id_belonging_to_another_installed_app_is_not_claimed() {
    let (_g, home, apps) = fixture();
    // The suite is gone; the reader from the same vendor is still installed.
    // Its container-shaped cache is segment-prefixed by the suite's id, and it
    // is a live application's data.
    install(&apps, "Reader", "com.acme.Suite.Reader");
    leftover_dir(&home, Location::Caches, "com.acme.Suite", 4_000);
    leftover_dir(&home, Location::Caches, "com.acme.Suite.Reader", 90_000);

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.Suite")).unwrap();

    let reader = report
        .rows
        .iter()
        .find(|r| r.path.ends_with("com.acme.Suite.Reader"))
        .expect("the row should be reported, so the user can see it exists");
    assert!(!reader.offerable, "a live app's data must not be offerable");
    assert!(!reader.bulk_grantable);
    assert!(reader
        .withheld
        .as_deref()
        .unwrap()
        .contains("still installed"));

    let own = report
        .rows
        .iter()
        .find(|r| r.path.ends_with("com.acme.Suite"))
        .unwrap();
    assert!(own.offerable, "the target's own cache is still offerable");
}

#[test]
fn an_orphan_sibling_segment_is_offered_but_never_bulk_grantable() {
    let (_g, home, apps) = fixture();
    // Nothing installed owns `com.acme.Suite.Reader`, so it is the departed
    // suite's own leftover — offerable, but it is a different identifier from
    // the one the user typed, so one gesture must not sweep it up.
    leftover_dir(&home, Location::Caches, "com.acme.Suite.Reader", 4_000);

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.Suite")).unwrap();

    let row = &report.rows[0];
    assert!(row.offerable);
    assert!(
        !row.bulk_grantable,
        "a longer id than the user named is a per-row judgement"
    );
    assert_eq!(row.matched_via, MatchedVia::SiblingSegment("Reader".into()));
}

#[test]
fn a_leftover_owned_by_a_helper_nested_in_another_installed_app_is_withheld() {
    let (_g, home, apps) = fixture();
    // One crash reporter, embedded in two different vendors' apps. Its cache
    // belongs to whichever of them is still installed — not to the departed
    // one, even though the id is segment-prefixed by it.
    let host = install(&apps, "Host", "com.vendor.Host");
    let nested = host.join("Contents/Library/Helpers");
    install(&nested, "Reporter", "com.acme.Tools.Reporter");
    leftover_dir(&home, Location::Caches, "com.acme.Tools.Reporter", 4_000);

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.Tools")).unwrap();

    let row = &report.rows[0];
    assert!(
        !row.offerable,
        "a helper nested inside an installed app still owns its data"
    );
    assert_eq!(report.withheld_count, 1);
}

#[test]
fn a_nested_id_is_never_added_to_the_targets_match_keys() {
    let (_g, home, apps) = fixture();
    // The reverse of the test above: the nested helper is the *target*. Its own
    // cache must be found, and its host's must not.
    let host = install(&apps, "Host", "com.vendor.Host");
    install(
        &host.join("Contents/Library"),
        "Helper",
        "com.vendor.Host.XPC",
    );
    leftover_dir(&home, Location::Caches, "com.gone.App", 4_000);
    leftover_dir(&home, Location::Caches, "com.vendor.Host", 90_000);

    let report = leftovers_for(&cfg(&home, &apps), &id("com.gone.App")).unwrap();

    assert_eq!(paths(&report), vec!["com.gone.App"]);
}

#[test]
fn the_target_being_installed_yields_no_rows_at_all() {
    let (_g, home, apps) = fixture();
    let bundle = install(&apps, "Acme", "com.acme.App");
    leftover_dir(&home, Location::Caches, "com.acme.App", 40_000);

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();

    assert_eq!(report.residence, Residence::Installed(vec![bundle]));
    assert!(
        report.rows.is_empty(),
        "an installed app has no leftovers; it has files"
    );
}

// --- spelling --------------------------------------------------------------

#[test]
fn an_id_that_differs_only_in_case_is_reported_and_never_offered() {
    let (_g, home, apps) = fixture();
    leftover_dir(&home, Location::Caches, "COM.ACME.APP", 4_000);

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();

    assert!(
        report.rows.is_empty(),
        "case folding here would claim a co-tenant's data"
    );
    assert_eq!(report.skipped_case_variant, 1);
    assert!(
        report.is_partial(),
        "an under-match the user could otherwise not explain must be visible"
    );
}

#[test]
fn an_id_with_glob_or_path_metacharacters_is_refused_outright() {
    // `.` is a regex wildcard and `*` is a glob, so an id is not a safe match
    // key until it has been through this. Rejected at construction, so no
    // downstream code has to remember.
    for bad in [
        "com.acme.*",
        "com.acme.N[a-z]tes",
        "com..acme",
        ".com.acme",
        "com.acme.",
        "com.acme/../../Documents",
        "com.acme..",
        "",
        ".",
        "..",
        "single",
        "com.acme.A B",
        "com.acme.A\u{0}x",
        "com.acme.é",
    ] {
        assert!(
            BundleId::parse(bad).is_none(),
            "{bad:?} must never become a match key"
        );
    }
    for good in [
        "com.acme.App",
        "com.acme.App-2",
        "com.acme.app_helper",
        "a.b",
    ] {
        assert!(BundleId::parse(good).is_some(), "{good:?} should parse");
    }
}

#[test]
fn an_unusable_bundle_id_refuses_the_scan_rather_than_matching_loosely() {
    let (_g, home, _apps) = fixture();
    let err = macclean_core::uninstall::leftovers_in(&home, "com.acme.*").unwrap_err();
    assert!(matches!(err, UninstallError::UnmatchableId(_)));
}

#[test]
fn the_byhost_uuid_rule_does_not_strip_a_sibling_segment() {
    let (_g, home, apps) = fixture();
    // The double strip exists only in ByHost, and only for a literal hardware
    // UUID. A helper's plist filed here must stem to the helper's id, never to
    // its parent's — otherwise the parent claims the helper's preferences.
    install(&apps, "Helper", "com.acme.App.Helper");
    leftover_file(
        &home,
        Location::PreferencesByHost,
        "com.acme.App.Helper.plist",
        300,
    );
    leftover_file(
        &home,
        Location::PreferencesByHost,
        "com.acme.App.00000000-1111-2222-3333-444444444444.plist",
        300,
    );

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();

    let offerable: Vec<String> = report
        .rows
        .iter()
        .filter(|r| r.offerable)
        .map(|r| r.path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        offerable,
        vec!["com.acme.App.00000000-1111-2222-3333-444444444444.plist"],
        "only the real ByHost spelling is the target's"
    );
}

#[test]
fn a_suffix_is_only_stripped_in_the_location_that_declares_it() {
    let (_g, home, apps) = fixture();
    // `.savedState` means something in Saved Application State and nothing in
    // Caches. Stripping it everywhere would invent an id boundary.
    leftover_dir(&home, Location::Caches, "com.acme.App.savedState", 4_000);
    leftover_dir(
        &home,
        Location::SavedApplicationState,
        "com.acme.App.savedState",
        4_000,
    );
    // Nor is an unknown extension a candidate at all.
    leftover_file(&home, Location::Preferences, "com.acme.App.plist.bak", 100);

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();

    let locs: Vec<Location> = report
        .rows
        .iter()
        .filter(|r| r.bulk_grantable)
        .map(|r| r.location)
        .collect();
    assert_eq!(
        locs,
        vec![Location::SavedApplicationState],
        "the Caches entry is a sibling id, and the .bak is not a candidate"
    );
    assert!(!report
        .rows
        .iter()
        .any(|r| r.path.ends_with("com.acme.App.plist.bak")));
}

#[test]
fn both_httpstorages_spellings_of_one_id_are_reported() {
    let (_g, home, apps) = fixture();
    // Real machines carry both, and stopping at the first hit leaves half the
    // app's HTTP state behind.
    leftover_dir(&home, Location::HttpStorages, "com.acme.App", 4_000);
    leftover_file(
        &home,
        Location::HttpStorages,
        "com.acme.App.binarycookies",
        900,
    );

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();

    assert_eq!(report.rows.len(), 2);
    assert!(report.rows.iter().all(|r| r.offerable));
}

#[test]
fn a_preferences_subdirectory_is_matched_by_exact_id_only() {
    let (_g, home, apps) = fixture();
    leftover_dir(&home, Location::Preferences, "com.acme.App", 4_000);
    leftover_dir(&home, Location::Preferences, "com.other.App", 4_000);

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();

    assert_eq!(paths(&report), vec!["com.acme.App"]);
    assert_eq!(report.rows[0].matched_via, MatchedVia::Id);
}

// --- symlinks --------------------------------------------------------------

#[test]
fn a_symlinked_leftover_is_dropped_rather_than_resolved() {
    let (_g, home, apps) = fixture();
    // Real machines have these: a Saved Application State entry that is a
    // symlink nine levels into another app's container. Canonicalizing is what
    // would make the row look legitimate, so it is dropped instead.
    let elsewhere = home.join("SomeoneElse/data");
    write_file(&elsewhere.join("theirs.bin"), 90_000);
    std::os::unix::fs::symlink(
        &elsewhere,
        home.join(Location::Caches.as_str()).join("com.acme.App"),
    )
    .unwrap();

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();

    assert!(report.rows.is_empty());
    assert_eq!(report.skipped_symlink, 1);
    assert!(report.is_partial());
    assert!(elsewhere.join("theirs.bin").exists());
}

// --- inventory -------------------------------------------------------------

#[test]
fn an_app_installed_outside_the_main_applications_folder_suppresses_its_leftovers() {
    let (_g, home, apps) = fixture();
    let utilities = apps.join("Utilities");
    fs::create_dir_all(&utilities).unwrap();
    install(&utilities, "Acme", "com.acme.App");
    leftover_dir(&home, Location::Caches, "com.acme.App", 40_000);

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();

    assert!(
        matches!(report.residence, Residence::Installed(_)),
        "an app filed in a subfolder is still installed"
    );
    assert!(report.rows.is_empty());
}

#[test]
fn an_unreadable_app_root_refuses_the_scan_rather_than_reporting_an_orphan() {
    use std::os::unix::fs::PermissionsExt;
    let (_g, home, apps) = fixture();
    leftover_dir(&home, Location::Caches, "com.acme.App", 40_000);
    fs::set_permissions(&apps, fs::Permissions::from_mode(0o000)).unwrap();

    let result = leftovers_for(&cfg(&home, &apps), &id("com.acme.App"));

    fs::set_permissions(&apps, fs::Permissions::from_mode(0o755)).unwrap();

    // "I could not check whether this app is still installed" must never be
    // rendered next to rows a user can be talked into granting.
    assert!(matches!(
        result,
        Err(UninstallError::InventoryIncomplete { .. })
    ));
}

#[test]
fn the_app_inventory_is_not_filtered_through_resolve_roots() {
    let (_g, home, _apps) = fixture();
    // The trap, pinned in both directions. `resolve_roots` drops protected
    // roots and documents that as "nothing to report, not an error" — correct
    // for a size walk, catastrophic for an authority check, because a shrunken
    // inventory makes installed apps look uninstalled.
    let system_apps = PathBuf::from("/System/Applications");
    assert!(
        macclean_core::largeold::resolve_roots(std::slice::from_ref(&system_apps), &home)
            .is_empty(),
        "resolve_roots is expected to drop this — that is why inventory must not use it"
    );
    assert!(
        inventory_roots(&home).contains(&system_apps),
        "the inventory must still look there"
    );
}

#[test]
fn nested_bundles_are_enumerated_to_the_declared_depth() {
    let (_g, home, apps) = fixture();
    let host = install(&apps, "Host", "com.vendor.Host");
    install(
        &host.join("Contents/Library/LoginItems"),
        "Launcher",
        "com.vendor.Host.Launcher",
    );

    let mut c = cfg(&home, &apps);
    c.app_scan_depth = 6;
    let apps_found = inventory(&c).unwrap();
    let index = owner_index(&apps_found);

    assert!(index.contains(&id("com.vendor.Host")));
    assert!(
        index.contains(&id("com.vendor.Host.Launcher")),
        "a nested login item is installed software and owns its data"
    );
}

#[test]
fn a_binary_info_plist_yields_the_same_identity_as_an_xml_one() {
    let (_g, home, apps) = fixture();
    let bundle = apps.join("Binary.app");
    fs::create_dir_all(bundle.join("Contents")).unwrap();
    let mut dict = plist::Dictionary::new();
    dict.insert(
        "CFBundleIdentifier".into(),
        plist::Value::String("com.acme.Binary".into()),
    );
    plist::Value::Dictionary(dict)
        .to_file_binary(bundle.join("Contents/Info.plist"))
        .unwrap();

    let found = inventory(&cfg(&home, &apps)).unwrap();

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, id("com.acme.Binary"));
}

// --- launch agents ---------------------------------------------------------

fn agent_plist(program: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
  <key>ProgramArguments</key><array><string>{}</string></array>
</dict></plist>"#,
        program.display()
    )
}

#[test]
fn a_launch_agent_whose_program_still_exists_is_not_offerable() {
    let (_g, home, apps) = fixture();
    let program = home.join("bin/acme-helper");
    write_file(&program, 100);
    let agent = home
        .join(Location::LaunchAgents.as_str())
        .join("com.acme.App.plist");
    fs::write(&agent, agent_plist(&program)).unwrap();

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();

    let row = &report.rows[0];
    assert!(
        !row.offerable,
        "something installed still runs from this job"
    );
    assert!(row.withheld.as_deref().unwrap().contains("still launches"));
}

#[test]
fn the_same_launch_agent_becomes_offerable_once_its_program_is_gone() {
    let (_g, home, apps) = fixture();
    let agent = home
        .join(Location::LaunchAgents.as_str())
        .join("com.acme.App.plist");
    fs::write(&agent, agent_plist(&home.join("bin/never-existed"))).unwrap();

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();

    assert!(report.rows[0].offerable);
    assert_eq!(report.withheld_count, 0);
}

#[test]
fn a_launch_agent_without_a_label_is_still_matched_by_its_filename_stem() {
    let (_g, home, apps) = fixture();
    // `Label` is absent from real agents in the wild, so the filename stem is
    // the match key and `Label` is display only.
    let agent = home
        .join(Location::LaunchAgents.as_str())
        .join("com.acme.App.plist");
    fs::write(
        &agent,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict><key>RunAtLoad</key><true/></dict></plist>"#,
    )
    .unwrap();

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();

    assert_eq!(paths(&report), vec!["com.acme.App.plist"]);
    assert!(report.rows[0].offerable);
}

// --- honesty ---------------------------------------------------------------

#[test]
fn an_unreadable_location_is_reported_not_treated_as_empty() {
    use std::os::unix::fs::PermissionsExt;
    let (_g, home, apps) = fixture();
    let caches = home.join(Location::Caches.as_str());
    leftover_dir(&home, Location::Caches, "com.acme.App", 4_000);
    fs::set_permissions(&caches, fs::Permissions::from_mode(0o000)).unwrap();

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();

    fs::set_permissions(&caches, fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(report.skipped_unreadable, 1);
    assert!(report.is_partial());
}

#[test]
fn hard_links_are_counted_once_per_name_not_once_per_inode() {
    let (_g, home, apps) = fixture();
    // The opposite of Space Lens, on purpose: a disposal unlinks names, so a
    // deduplicated figure would be a number no action can produce.
    let dir = home.join(Location::Caches.as_str()).join("com.acme.App");
    write_file(&dir.join("a.bin"), 4_000);
    fs::hard_link(dir.join("a.bin"), dir.join("b.bin")).unwrap();

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();

    assert_eq!(
        report.rows[0].file_count, 2,
        "two names, two rows to unlink"
    );
    assert_eq!(report.rows[0].size_bytes, 8_000);
}

#[test]
fn withheld_rows_do_not_make_the_report_partial() {
    let (_g, home, apps) = fixture();
    install(&apps, "Reader", "com.acme.Suite.Reader");
    leftover_dir(&home, Location::Caches, "com.acme.Suite.Reader", 4_000);

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.Suite")).unwrap();

    assert_eq!(report.withheld_count, 1);
    assert!(
        !report.is_partial(),
        "withholding is the module working; a caveat that fires on correct \
         behaviour teaches people to ignore it"
    );
}

#[test]
fn the_scan_never_opens_a_leftover_files_contents() {
    use std::os::unix::fs::PermissionsExt;
    let (_g, home, apps) = fixture();
    // A licence key or a bearer token read for classification would land in a
    // UI row and, later, in an append-only log that is never rotated. A file
    // this scan cannot open must still be sized.
    let dir = home.join(Location::Caches.as_str()).join("com.acme.App");
    let secret = dir.join("license.dat");
    write_file(&secret, 4_000);
    fs::set_permissions(&secret, fs::Permissions::from_mode(0o000)).unwrap();

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();

    fs::set_permissions(&secret, fs::Permissions::from_mode(0o644)).unwrap();

    assert_eq!(report.rows[0].size_bytes, 4_000, "stat, not open");
    assert_eq!(report.skipped_unreadable, 0);
    assert!(!report.rows[0].size_is_floor);
}

#[test]
fn the_walk_stops_at_the_entry_budget_and_says_so() {
    let (_g, home, apps) = fixture();
    let dir = home.join(Location::Caches.as_str()).join("com.acme.App");
    for i in 0..40 {
        write_file(&dir.join(format!("f{i}.bin")), 100);
    }

    let mut c = cfg(&home, &apps);
    c.max_examined = 5;
    let report = leftovers_for(&c, &id("com.acme.App")).unwrap();

    assert!(report.rows[0].size_is_floor, "a truncated size is a floor");
    assert!(report.is_partial());
}

#[test]
fn a_protected_subtree_inside_a_leftover_is_not_counted() {
    let (_g, home, apps) = fixture();
    // A vendored checkout inside a leftover tree is not disposable, so its
    // bytes must not appear in a figure that describes what a disposal frees.
    let dir = home.join(Location::Caches.as_str()).join("com.acme.App");
    write_file(&dir.join("blob.bin"), 4_000);
    write_file(&dir.join("vendor/.git/objects/pack.idx"), 90_000);

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();

    assert_eq!(report.rows[0].size_bytes, 4_000);
    assert!(
        report.rows[0].size_is_floor,
        "something inside was not measured, so the figure is a floor"
    );
}
