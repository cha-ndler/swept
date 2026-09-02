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
    inventory, inventory_roots, leftovers_for, leftovers_for_named, owner_index, BundleId,
    DisplayName, Kind, Location, MatchedVia, Residence, UninstallConfig, UninstallError,
    CFPREFSD_CAVEAT, CONTAINER_STATE_PARTS, CONTAINER_USER_DATA_PARTS, DEFERRED_LOCATIONS,
    SEARCHED_LOCATIONS,
};
use safety::DirLimits;

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

fn name(s: &str) -> DisplayName {
    DisplayName::parse(s).unwrap_or_else(|| panic!("{s:?} should be a valid display name"))
}

/// An installed `.app` whose file stem and `CFBundleName` differ, for the
/// name-tier tests that need to tell the two routes apart.
fn install_named(root: &Path, stem: &str, id: &str, bundle_name: &str) -> PathBuf {
    let bundle = root.join(format!("{stem}.app"));
    fs::create_dir_all(bundle.join("Contents")).unwrap();
    fs::write(
        bundle.join("Contents/Info.plist"),
        info_plist(id, bundle_name),
    )
    .unwrap();
    bundle
}

/// A sandbox container at `~/Library/Containers/<id>`, scaffolded the way
/// `containermanagerd` lays one out: a redirected home under `Data`, with
/// every state and user-data part present and empty, and the user's folders
/// as real directories.
fn container(home: &Path, id: &str) -> PathBuf {
    let root = home.join(Location::Containers.as_str()).join(id);
    for part in CONTAINER_STATE_PARTS
        .iter()
        .chain(CONTAINER_USER_DATA_PARTS)
    {
        fs::create_dir_all(root.join("Data").join(part)).unwrap();
    }
    for dir in ["Desktop", "Downloads", "Movies", "Music", "Pictures"] {
        fs::create_dir_all(root.join("Data").join(dir)).unwrap();
    }
    root
}

/// Put one file into a container part; returns the part's path.
fn fill(container: &Path, part: &str, bytes: u64) -> PathBuf {
    let part = container.join("Data").join(part);
    write_file(&part.join("blob.bin"), bytes);
    part
}

fn app_support(home: &Path) -> PathBuf {
    home.join(Location::ApplicationSupport.as_str())
}

fn offerable(report: &macclean_core::uninstall::LeftoverReport) -> Vec<PathBuf> {
    report
        .rows
        .iter()
        .filter(|r| r.offerable)
        .map(|r| r.path.clone())
        .collect()
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
            "Library/Containers",
            "Library/HTTPStorages",
            "Library/WebKit",
            "Library/Preferences",
            "Library/Preferences/ByHost",
            "Library/Saved Application State",
            "Library/LaunchAgents",
            "Library/Logs",
            "Library/Application Support",
            "Library/Group Containers",
        ]
    );
    // The container inclusion lists are part of the same canary: adding a part
    // widens what a container row can stand for.
    assert_eq!(
        CONTAINER_STATE_PARTS,
        [
            "Library/Caches",
            "Library/HTTPStorages",
            "Library/Logs",
            "Library/Preferences",
            "Library/Saved Application State",
            "Library/WebKit",
            "tmp",
        ]
    );
    assert_eq!(
        CONTAINER_USER_DATA_PARTS,
        ["Documents", "Library/Application Support"]
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

    // Some surfaces are deliberately not searched. A report that did not say
    // so would read as "this is everything", which is the one thing it is not.
    assert_eq!(report.deferred.len(), DEFERRED_LOCATIONS.len());
    assert!(!report.deferred.is_empty());
    for (surface, reason) in report.deferred {
        assert!(!reason.is_empty(), "a deferral must say why");
        // Containers and Group Containers are searched now; a deferral naming
        // either would be stale.
        assert!(
            !surface.ends_with("/Containers"),
            "{surface} is searched, not deferred"
        );
    }
    // The cookie jar is handed to the Privacy module on purpose, and the
    // report has to keep saying so until that module exists.
    assert!(report
        .deferred
        .iter()
        .any(|(p, _)| *p == "~/Library/Cookies"));
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

// --- containers ------------------------------------------------------------
//
// A container is the app's redirected home. `Data/Documents` is where a
// sandboxed app puts the user's only copy of a file, and Finder does not show
// it. So a container is never one row: it is decomposed into the parts that
// are the app's own state, by an inclusion list, and the parts that are the
// user's are shown but never offered.

#[test]
fn a_container_root_is_never_offered() {
    let (_g, home, apps) = fixture();
    let root = container(&home, "com.acme.App");
    fill(&root, "Library/Caches", 4_000);
    fill(&root, "Library/Preferences", 200);
    fill(&root, "Documents", 9_000);

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();

    assert!(
        !offerable(&report).is_empty(),
        "the state parts are offered"
    );
    for row in &report.rows {
        assert_ne!(row.path, root, "the container root is not a row");
        if row.offerable {
            let rel = row
                .path
                .strip_prefix(root.join("Data"))
                .expect("an offerable container row is strictly inside Data");
            assert!(
                CONTAINER_STATE_PARTS.iter().any(|p| Path::new(p) == rel),
                "{} is not on the inclusion list",
                rel.display()
            );
            assert_eq!(row.location, Location::Containers);
            assert_eq!(row.kind, Kind::Leftover);
        }
    }
}

#[test]
fn an_unknown_directory_under_a_containers_data_library_is_not_offered() {
    let (_g, home, apps) = fixture();
    let root = container(&home, "com.acme.App");
    // Inclusion, not exclusion: a directory Apple adds next year is not a row
    // until someone decides it is. Nor is the cookie jar, which is regenerable
    // but belongs to the Privacy module and its consequence label.
    write_file(
        &root.join("Data/Library/Something Apple Added/blob.bin"),
        4_000,
    );
    write_file(
        &root.join("Data/Library/Cookies/Cookies.binarycookies"),
        4_000,
    );
    write_file(&root.join("Data/Pictures/photo.heic"), 4_000);
    let caches = fill(&root, "Library/Caches", 1_000);

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();

    assert_eq!(
        report
            .rows
            .iter()
            .map(|r| r.path.clone())
            .collect::<Vec<_>>(),
        vec![caches]
    );
}

#[test]
fn container_user_data_is_shown_and_never_offerable() {
    let (_g, home, apps) = fixture();
    let root = container(&home, "com.acme.App");
    let documents = fill(&root, "Documents", 9_000);
    let support = fill(&root, "Library/Application Support", 9_000);
    let caches = fill(&root, "Library/Caches", 1_000);

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();

    assert_eq!(report.rows.len(), 3);
    for row in &report.rows {
        if row.path == caches {
            assert_eq!(row.kind, Kind::Leftover);
            assert!(row.offerable && row.bulk_grantable);
            continue;
        }
        assert!(
            row.path == documents || row.path == support,
            "{:?}",
            row.path
        );
        assert_eq!(row.kind, Kind::UserData);
        assert!(!row.offerable, "user data is shown, not offered");
        assert!(!row.bulk_grantable);
        assert!(row.withheld.as_deref().is_some_and(|w| !w.is_empty()));
        assert_eq!(row.size_bytes, 9_000, "shown with its real size");
    }
    assert_eq!(report.withheld_count, 2);
    assert_eq!(
        report.total_bytes(),
        1_000,
        "the reclaimable figure counts only what may be offered"
    );
    assert!(!report.is_partial());
}

#[test]
fn an_empty_documents_directory_still_yields_the_container_state_rows() {
    let (_g, home, apps) = fixture();
    let root = container(&home, "com.acme.App");
    let caches = fill(&root, "Library/Caches", 1_000);

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();

    // Scaffolding that is empty — which is most of every container — is not
    // a row of either kind.
    assert_eq!(
        report
            .rows
            .iter()
            .map(|r| r.path.clone())
            .collect::<Vec<_>>(),
        vec![caches]
    );
    assert!(report.rows.iter().all(|r| r.kind == Kind::Leftover));
}

#[test]
fn a_container_part_that_resolves_outside_the_container_is_never_offered() {
    let (_g, home, apps) = fixture();
    // The measured shape: in 82 of 822 real containers, entries under `Data`
    // are symlinks back into the real home. Two are placed on parts this
    // module *would* offer, and one container symlinks its whole `Library`
    // to the real `~/Library` — so its `Caches` part canonicalizes to a
    // location root.
    write_file(&home.join("Downloads/tax-return.pdf"), 9_000);
    write_file(&home.join("Documents/thesis.pages"), 9_000);
    let root = container(&home, "com.acme.App");
    fs::remove_dir(root.join("Data/tmp")).unwrap();
    std::os::unix::fs::symlink("../../../../Downloads", root.join("Data/tmp")).unwrap();
    fs::remove_dir(root.join("Data/Documents")).unwrap();
    std::os::unix::fs::symlink("../../../../Documents", root.join("Data/Documents")).unwrap();
    let caches = fill(&root, "Library/Caches", 1_000);

    let helper = home
        .join(Location::Containers.as_str())
        .join("com.acme.App.Helper");
    fs::create_dir_all(helper.join("Data")).unwrap();
    std::os::unix::fs::symlink(home.join("Library"), helper.join("Data/Library")).unwrap();
    leftover_dir(&home, Location::Caches, "unrelated", 100);

    let before = snapshot(&home);
    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();
    assert_eq!(before, snapshot(&home));

    assert_eq!(offerable(&report), vec![caches]);
    for row in &report.rows {
        assert!(!row.path.starts_with(home.join("Downloads")));
        assert!(!row.path.starts_with(home.join("Documents")));
        assert_ne!(row.path, home.join(Location::Caches.as_str()));
        assert!(
            !fs::symlink_metadata(&row.path)
                .unwrap()
                .file_type()
                .is_symlink(),
            "no row is a symlink"
        );
    }
    assert!(
        report.skipped_symlink >= 2,
        "the escapes are counted, not resolved"
    );
}

#[test]
fn a_container_of_a_still_installed_sibling_is_withheld_whole() {
    let (_g, home, apps) = fixture();
    install(&apps, "Reader", "com.acme.Suite.Reader");
    let reader = container(&home, "com.acme.Suite.Reader");
    fill(&reader, "Library/Caches", 4_000);
    fill(&reader, "Documents", 4_000);
    let suite = container(&home, "com.acme.Suite");
    let suite_caches = fill(&suite, "Library/Caches", 1_000);

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.Suite")).unwrap();

    assert_eq!(offerable(&report), vec![suite_caches]);
    let withheld: Vec<&macclean_core::uninstall::Candidate> =
        report.rows.iter().filter(|r| !r.offerable).collect();
    assert_eq!(withheld.len(), 1);
    assert_eq!(withheld[0].path, reader, "one row for the whole container");
    assert!(!withheld[0].bulk_grantable);
    assert!(withheld[0]
        .withheld
        .as_deref()
        .is_some_and(|w| w.contains("com.acme.Suite.Reader")));
    assert!(
        !report
            .rows
            .iter()
            .any(|r| r.path.starts_with(reader.join("Data"))),
        "a live app's container is not decomposed"
    );
}

#[test]
fn an_orphan_sibling_container_is_decomposed_and_never_bulk_grantable() {
    let (_g, home, apps) = fixture();
    let helper = container(&home, "com.acme.App.Helper");
    let caches = fill(&helper, "Library/Caches", 4_000);

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();

    assert_eq!(report.rows.len(), 1);
    let row = &report.rows[0];
    assert_eq!(row.path, caches);
    assert!(row.offerable);
    assert!(
        !row.bulk_grantable,
        "a different id from the one the user named"
    );
    assert_eq!(row.matched_via, MatchedVia::SiblingSegment("Helper".into()));
}

// --- the human-name tier ---------------------------------------------------
//
// Most apps name their `Application Support` directory after themselves, not
// after their id — 89 of 129 entries on the reference machine. The tier that
// matches those is weaker than id matching and is gated three times: byte-exact
// equality with a name the caller supplied, no installed app answering to that
// name, and an id-keyed child inside to corroborate it.

#[test]
fn a_name_keyed_directory_is_offered_only_with_corroboration_and_never_in_bulk() {
    let (_g, home, apps) = fixture();
    let dir = app_support(&home).join("Acme Notes");
    write_file(&dir.join("com.acme.Notes.plist"), 100);
    write_file(&dir.join("store/notes.sqlite"), 9_000);

    let report = leftovers_for_named(
        &cfg(&home, &apps),
        &id("com.acme.Notes"),
        Some(&name("Acme Notes")),
    )
    .unwrap();

    assert_eq!(report.rows.len(), 1);
    let row = &report.rows[0];
    assert_eq!(row.path, dir);
    assert_eq!(row.location, Location::ApplicationSupport);
    assert_eq!(
        row.matched_via,
        MatchedVia::DisplayName("Acme Notes".into())
    );
    assert_eq!(row.kind, Kind::Leftover);
    assert!(row.offerable);
    assert!(
        !row.bulk_grantable,
        "a name match is a judgement call the user makes per row"
    );
    assert_eq!(row.size_bytes, 9_100);

    // Without a name there is no name tier: the same directory is invisible.
    let plain = leftovers_for(&cfg(&home, &apps), &id("com.acme.Notes")).unwrap();
    assert!(plain.rows.is_empty());
}

#[test]
fn an_uncorroborated_name_match_is_counted_and_not_offered() {
    let (_g, home, apps) = fixture();
    // Right name, nothing inside keyed on the id. The caller's word is the only
    // link between this directory and the target, and that is not enough.
    write_file(&app_support(&home).join("Acme Notes/cache/blob.bin"), 4_000);

    let report = leftovers_for_named(
        &cfg(&home, &apps),
        &id("com.acme.Notes"),
        Some(&name("Acme Notes")),
    )
    .unwrap();

    assert!(report.rows.is_empty());
    assert_eq!(report.skipped_uncorroborated_name, 1);
    assert!(
        !report.is_partial(),
        "declining a name match is the tier working, not a gap in what was seen"
    );
}

#[test]
fn corroboration_uses_the_segment_predicate_not_a_byte_prefix() {
    let (_g, home, apps) = fixture();
    // `com.acme.Notes2` is a byte-prefix match and a segment mismatch. If it
    // corroborated, the whole tier would inherit the collision the id matcher
    // was built to avoid.
    write_file(
        &app_support(&home).join("Acme Notes/com.acme.Notes2.plist"),
        100,
    );

    let report = leftovers_for_named(
        &cfg(&home, &apps),
        &id("com.acme.Notes"),
        Some(&name("Acme Notes")),
    )
    .unwrap();

    assert!(report.rows.is_empty());
    assert_eq!(report.skipped_uncorroborated_name, 1);
}

#[test]
fn a_child_owned_by_a_still_installed_app_does_not_corroborate() {
    let (_g, home, apps) = fixture();
    install(&apps, "Helper", "com.acme.Notes.Helper");
    // The only id-keyed child belongs to an installed app. That is evidence the
    // directory is *someone's*, and not evidence it is the target's.
    write_file(
        &app_support(&home).join("Acme Notes/com.acme.Notes.Helper/state.db"),
        100,
    );

    let report = leftovers_for_named(
        &cfg(&home, &apps),
        &id("com.acme.Notes"),
        Some(&name("Acme Notes")),
    )
    .unwrap();

    assert!(report.rows.is_empty());
    assert_eq!(report.skipped_uncorroborated_name, 1);
}

#[test]
fn a_name_answered_to_by_an_installed_app_is_withheld_even_when_corroborated() {
    let (_g, home, apps) = fixture();
    // Two routes to a name, tested separately so dropping either is caught:
    // `CFBundleName`, and the `.app` file stem.
    install_named(&apps, "Whatever", "com.other.Notes", "Acme Notes");
    install_named(&apps, "Notes Deluxe", "com.third.Notes", "Something Else");
    for dir in ["Acme Notes", "Notes Deluxe"] {
        write_file(
            &app_support(&home).join(dir).join("com.acme.Notes.plist"),
            100,
        );
    }

    for (dir, owner) in [
        ("Acme Notes", "com.other.Notes"),
        ("Notes Deluxe", "com.third.Notes"),
    ] {
        let report =
            leftovers_for_named(&cfg(&home, &apps), &id("com.acme.Notes"), Some(&name(dir)))
                .unwrap();

        assert_eq!(
            report.rows.len(),
            1,
            "{dir}: shown so the user knows it exists"
        );
        let row = &report.rows[0];
        assert_eq!(row.path, app_support(&home).join(dir));
        assert!(
            !row.offerable,
            "{dir}: an installed app answers to this name"
        );
        assert!(!row.bulk_grantable);
        assert!(row.withheld.as_deref().is_some_and(|w| w.contains(owner)));
        assert_eq!(report.withheld_count, 1);
    }
}

#[test]
fn a_name_match_is_byte_exact() {
    let (_g, home, apps) = fixture();
    for dir in ["acme notes", "Acme Notes ", "Acme  Notes"] {
        write_file(
            &app_support(&home).join(dir).join("com.acme.Notes.plist"),
            100,
        );
    }

    let report = leftovers_for_named(
        &cfg(&home, &apps),
        &id("com.acme.Notes"),
        Some(&name("Acme Notes")),
    )
    .unwrap();

    assert!(report.rows.is_empty(), "no folding, no trimming, no fuzz");
}

#[test]
fn a_display_name_that_cannot_be_a_match_key_is_refused() {
    for raw in ["", ".", "..", "Acme/Notes", "Acme\0Notes", &"x".repeat(256)] {
        assert!(DisplayName::parse(raw).is_none(), "{raw:?} was accepted");
    }
    for raw in ["Acme Notes", "Notes.app", "Ünïcödé", "x", &"x".repeat(255)] {
        assert!(DisplayName::parse(raw).is_some(), "{raw:?} was refused");
    }
}

#[test]
fn the_name_tier_never_fires_outside_application_support() {
    let (_g, home, apps) = fixture();
    // Each of these is a name match with a corroborating child — in the wrong
    // location. `Logs` has no id-named child to corroborate against in
    // practice, and a container named after an app is not a shape that exists.
    write_file(
        &home
            .join(Location::Logs.as_str())
            .join("Acme Notes/com.acme.Notes.log"),
        100,
    );
    write_file(
        &home
            .join(Location::Caches.as_str())
            .join("Acme Notes/com.acme.Notes/blob.bin"),
        100,
    );
    let named_container = container(&home, "Acme Notes");
    fill(&named_container, "Library/Caches", 100);
    write_file(
        &named_container.join("Data/Library/Caches/com.acme.Notes.db"),
        100,
    );

    let report = leftovers_for_named(
        &cfg(&home, &apps),
        &id("com.acme.Notes"),
        Some(&name("Acme Notes")),
    )
    .unwrap();

    assert!(report.rows.is_empty(), "{:?}", paths(&report));
    assert_eq!(report.skipped_uncorroborated_name, 0, "not even considered");
}

// --- group containers ------------------------------------------------------

#[test]
fn a_group_container_is_shown_as_shared_and_never_offered() {
    let (_g, home, apps) = fixture();
    let groups = home.join(Location::GroupContainers.as_str());
    write_file(&groups.join("group.com.acme.App/shared.db"), 4_000);
    write_file(&groups.join("ABCDE12345.com.acme.App/shared.db"), 4_000);
    write_file(&groups.join("group.com.other.App/shared.db"), 4_000);
    // An arbitrary prefix is not a group-container prefix: the strip is fenced
    // to `group.` and a ten-character team id.
    write_file(&groups.join("vendor.com.acme.App/shared.db"), 4_000);
    // An id-keyed row alongside, so the assertion cannot pass vacuously.
    let caches = leftover_dir(&home, Location::Caches, "com.acme.App", 1_000);

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();

    assert_eq!(offerable(&report), vec![caches]);
    let shared: Vec<&macclean_core::uninstall::Candidate> = report
        .rows
        .iter()
        .filter(|r| r.location == Location::GroupContainers)
        .collect();
    assert_eq!(shared.len(), 2, "shown, so the user knows they exist");
    for row in &shared {
        assert_eq!(row.kind, Kind::Shared);
        assert!(!row.offerable, "shared by construction; never claimable");
        assert!(!row.bulk_grantable);
        assert!(row.withheld.is_some());
        assert_eq!(row.size_bytes, 4_000);
    }
    assert!(shared
        .iter()
        .any(|r| r.matched_via == MatchedVia::IdWithPrefix("group.".into())));
    assert!(shared
        .iter()
        .any(|r| r.matched_via == MatchedVia::IdWithPrefix("ABCDE12345.".into())));
    assert!(!report
        .rows
        .iter()
        .any(|r| r.path.ends_with("group.com.other.App")));
    assert_eq!(report.total_bytes(), 1_000);
    assert!(!report.is_partial());
}

// --- honesty, again --------------------------------------------------------

#[test]
fn an_unrelated_symlink_in_a_location_does_not_make_the_report_partial() {
    let (_g, home, apps) = fixture();
    // A stock machine has a few of these in Saved Application State, none of
    // them the target's. Found by running the scan against a real home: every
    // one of 41 reports came back partial because of the same four entries —
    // a caveat that fires on every scan teaches people to ignore it. A
    // symlink is dropped either way; it is *counted* only when it is ours.
    let elsewhere = home.join("SomeoneElse/data");
    write_file(&elsewhere.join("theirs.bin"), 90);
    std::os::unix::fs::symlink(
        &elsewhere,
        home.join(Location::SavedApplicationState.as_str())
            .join("com.other.App.savedState"),
    )
    .unwrap();
    std::os::unix::fs::symlink(
        &elsewhere,
        home.join(Location::Caches.as_str()).join("com.other.App"),
    )
    .unwrap();
    let caches = leftover_dir(&home, Location::Caches, "com.acme.App", 1_000);

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();

    assert_eq!(offerable(&report), vec![caches]);
    assert_eq!(report.skipped_symlink, 0, "not ours, not a gap");
    assert!(!report.is_partial());
}

// --- an offer the tool cannot honour is not an offer -------------------------
//
// Disposal of a directory goes through `guard_dir`, which refuses a tree with
// a protected path at any depth or one outside `DirLimits`. A row this scan
// already knows `guard_dir` will refuse must not be offered: showing a user a
// checkbox that is certain to fail is a lie of a different shape, and the
// discovery half is the only place that knows in advance.

#[test]
fn a_leftover_tree_containing_a_git_checkout_is_shown_and_not_offered() {
    let (_g, home, apps) = fixture();
    // A vendored checkout inside a cache — an Electron app's plugin, say.
    let dir = leftover_dir(&home, Location::Caches, "com.acme.App", 4_000);
    write_file(&dir.join("plugins/vendor/.git/HEAD"), 100);
    let clean = leftover_dir(&home, Location::Logs, "com.acme.App", 100);

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();

    assert_eq!(offerable(&report), vec![clean]);
    let row = report.rows.iter().find(|r| r.path == dir).expect("shown");
    assert!(!row.offerable, "guard_dir is certain to refuse it");
    assert!(!row.bulk_grantable);
    assert!(row.undisposable.is_some());
    assert!(row
        .withheld
        .as_deref()
        .is_some_and(|w| w.contains("protected")));
    assert_eq!(report.withheld_count, 1);
}

#[test]
fn a_leftover_tree_beyond_the_dir_limits_is_flagged_and_not_offered() {
    let (_g, home, apps) = fixture();
    let dir = leftover_dir(&home, Location::Caches, "com.acme.App", 4_000);
    for i in 0..5 {
        write_file(&dir.join(format!("f{i}.bin")), 10);
    }

    // The limits are injectable only so a fixture can reach them: 50,000
    // files is not a tempdir test.
    let mut too_many = cfg(&home, &apps);
    too_many.dir_limits.max_entries = 3;
    let report = leftovers_for(&too_many, &id("com.acme.App")).unwrap();
    assert!(
        offerable(&report).is_empty(),
        "more entries than guard_dir permits"
    );
    assert!(report.rows[0]
        .undisposable
        .is_some_and(|w| w.contains("entries")));

    let mut too_big = cfg(&home, &apps);
    too_big.dir_limits.max_bytes = 100;
    let report = leftovers_for(&too_big, &id("com.acme.App")).unwrap();
    assert!(
        offerable(&report).is_empty(),
        "larger than guard_dir permits"
    );
    assert!(report.rows[0]
        .undisposable
        .is_some_and(|w| w.contains("larger")));

    // Depth needs no injection: the walk's own ceiling equals guard_dir's.
    let (_g2, home2, apps2) = fixture();
    let deep_root = leftover_dir(&home2, Location::Caches, "com.acme.App", 10);
    let mut deep = deep_root.clone();
    for i in 0..(DirLimits::default().max_depth + 1) {
        deep = deep.join(format!("d{i}"));
    }
    write_file(&deep.join("leaf.bin"), 10);
    let report = leftovers_for(&cfg(&home2, &apps2), &id("com.acme.App")).unwrap();
    assert!(
        offerable(&report).is_empty(),
        "deeper than guard_dir permits"
    );
    assert!(report.rows[0]
        .undisposable
        .is_some_and(|w| w.contains("deep")));
}

#[test]
fn the_flagged_limits_are_the_ones_disposal_will_apply() {
    // If these ever diverge the flag lies in the dangerous direction: a row
    // marked disposable that `guard_dir` then refuses, or the reverse.
    let (_g, home, _apps) = fixture();
    let ours = UninstallConfig::new(home).dir_limits;
    let theirs = DirLimits::default();
    assert_eq!(ours.max_entries, theirs.max_entries);
    assert_eq!(ours.max_bytes, theirs.max_bytes);
    assert_eq!(ours.max_depth, theirs.max_depth);
}

#[test]
fn flagging_a_row_undisposable_does_not_make_the_report_partial() {
    let (_g, home, apps) = fixture();
    let dir = leftover_dir(&home, Location::Caches, "com.acme.App", 4_000);
    write_file(&dir.join("vendor/.git/HEAD"), 100);

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();

    assert!(!report.rows[0].offerable);
    assert!(
        !report.is_partial(),
        "withholding is the module working; a caveat that fires on correct \
         behaviour teaches people to ignore it"
    );
}

#[test]
fn an_undisposable_container_part_is_not_offered_while_its_siblings_are() {
    let (_g, home, apps) = fixture();
    let root = container(&home, "com.acme.App");
    let caches = fill(&root, "Library/Caches", 1_000);
    write_file(&caches.join("repo/.git/config"), 100);
    let logs = fill(&root, "Library/Logs", 1_000);

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();

    assert_eq!(offerable(&report), vec![logs]);
    let row = report
        .rows
        .iter()
        .find(|r| r.path == caches)
        .expect("shown");
    assert!(!row.offerable && row.undisposable.is_some());
}

#[test]
fn an_undisposable_name_keyed_directory_is_not_offered() {
    let (_g, home, apps) = fixture();
    let dir = app_support(&home).join("Acme Notes");
    write_file(&dir.join("com.acme.Notes.plist"), 100);
    write_file(&dir.join("extensions/thing/.git/HEAD"), 100);

    let report = leftovers_for_named(
        &cfg(&home, &apps),
        &id("com.acme.Notes"),
        Some(&name("Acme Notes")),
    )
    .unwrap();

    assert_eq!(report.rows.len(), 1);
    assert!(!report.rows[0].offerable);
    assert!(report.rows[0].undisposable.is_some());
    assert_eq!(report.withheld_count, 1);
}

// --- caveats and bulk gestures ----------------------------------------------

#[test]
fn a_preferences_row_carries_the_cfprefsd_caveat() {
    let (_g, home, apps) = fixture();
    leftover_file(&home, Location::Preferences, "com.acme.App.plist", 100);

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();
    assert!(report.caveats.contains(&CFPREFSD_CAVEAT));

    // And not on a report with nothing cfprefsd would touch.
    let (_g2, home2, apps2) = fixture();
    leftover_dir(&home2, Location::Caches, "com.acme.App", 100);
    let report = leftovers_for(&cfg(&home2, &apps2), &id("com.acme.App")).unwrap();
    assert!(!report.caveats.contains(&CFPREFSD_CAVEAT));

    // A container's own preferences part counts too.
    let (_g3, home3, apps3) = fixture();
    let root = container(&home3, "com.acme.App");
    fill(&root, "Library/Preferences", 100);
    let report = leftovers_for(&cfg(&home3, &apps3), &id("com.acme.App")).unwrap();
    assert!(report.caveats.contains(&CFPREFSD_CAVEAT));
}

#[test]
fn a_license_shaped_file_marks_its_row_and_keeps_it_out_of_bulk() {
    // Names only — nothing is opened. A licence, activation or receipt among a
    // directory's immediate children is a reason a human should look before a
    // select-all sweeps the row up; it is not a reason to withhold it.
    let (_g, home, apps) = fixture();
    let licensed = leftover_dir(&home, Location::ApplicationSupport, "com.acme.App", 4_000);
    write_file(&licensed.join("license.lic"), 100);
    let receipts = leftover_dir(&home, Location::Caches, "com.acme.App", 4_000);
    fs::create_dir_all(receipts.join("Receipts")).unwrap();
    let plain = leftover_dir(&home, Location::Logs, "com.acme.App", 4_000);

    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();

    for row in &report.rows {
        assert!(row.offerable, "{:?}", row.path);
        let expect = row.path == licensed || row.path == receipts;
        assert_eq!(row.license_suspected, expect, "{:?}", row.path);
        assert_eq!(row.bulk_grantable, !expect, "{:?}", row.path);
    }
    assert!(report
        .rows
        .iter()
        .any(|r| r.path == plain && r.bulk_grantable));
}

/// `is_partial` asks whether a floor is *unexplained*, not whether the row was
/// offerable — and this is the case that distinguishes the two. An unreadable
/// subtree floors the measurement and sets no other counter: `truncated` is
/// false, `skipped_unreadable` counts locations rather than subtrees, and the
/// row is withheld, so the old `offerable && size_is_floor` term would report
/// the run as complete while a figure on screen was a floor.
///
/// The row is also correctly withheld: `guard_dir` refuses a tree it cannot
/// read in full, so offering it would be a checkbox certain to fail.
#[test]
fn a_row_floored_by_an_unreadable_subtree_makes_the_report_partial() {
    use std::os::unix::fs::PermissionsExt;

    let (_g, home, apps) = fixture();
    let dir = leftover_dir(&home, Location::Caches, "com.acme.App", 4_000);
    let inner = dir.join("inner");
    write_file(&inner.join("blob.bin"), 100);

    let mut perms = fs::metadata(&inner).unwrap().permissions();
    perms.set_mode(0o000);
    fs::set_permissions(&inner, perms).unwrap();
    let report = leftovers_for(&cfg(&home, &apps), &id("com.acme.App")).unwrap();
    let mut perms = fs::metadata(&inner).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&inner, perms).unwrap();

    assert!(!report.truncated, "nothing hit the entry budget");
    let row = &report.rows[0];
    assert!(row.size_is_floor);
    assert!(row.undisposable.is_none(), "no bound was exceeded");
    assert!(!row.offerable, "a tree we could not measure is not offered");
    assert!(report.is_partial());
}
