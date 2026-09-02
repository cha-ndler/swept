//! Browser privacy data — discovery.
//!
//! The ranking that shapes this file: **never emitting a row for a file that
//! is not browsing data matters more than finding browsing data at all.** The
//! denylist protects almost nothing here. A Firefox profile keeps
//! `cookies.sqlite` and `key4.db` in the *same flat directory*; Chromium keeps
//! `Cookies` and `Login Data` as byte-adjacent siblings. Both pass
//! `safety::guard` cleanly, because both are ordinary files in an ordinary
//! directory.
//!
//! So the entire safety argument rests on the inclusion list: only names this
//! module recognizes are ever emitted, and a name it does not recognize is not
//! a row no matter where it sits. The negative tests below are the ones that
//! would fail if that rule were ever inverted into an exclusion list.
//!
//! SAFETY CONTRACT item 7: everything here runs against a throwaway tempdir.

use std::fs;
use std::path::{Path, PathBuf};

use macclean_core::privacy::{
    scan, Access, Class, Consequence, PrivacyConfig, BROWSERS, UNSUPPORTED,
};
use safety::allowlist;

// --- fixtures --------------------------------------------------------------

/// A throwaway home. Canonicalized because `/var/folders` is a symlink and the
/// denylist compares component-wise.
fn fixture() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(dir.path()).unwrap();
    (dir, home)
}

fn cfg(home: &Path) -> PrivacyConfig {
    PrivacyConfig::new(home.to_path_buf())
}

fn write(path: &Path, bytes: usize) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, vec![b'x'; bytes]).unwrap();
}

fn mkdir(path: &Path) {
    fs::create_dir_all(path).unwrap();
}

/// Chrome's vendor root. Note there is deliberately no `Default` profile in
/// most of these fixtures: the reference machine has none either.
fn chrome_root(home: &Path) -> PathBuf {
    home.join("Library/Application Support/Google/Chrome")
}

/// A corroborated Chromium profile: a directory holding a `Preferences` file.
fn chromium_profile(home: &Path, name: &str) -> PathBuf {
    let p = chrome_root(home).join(name);
    write(&p.join("Preferences"), 10);
    p
}

fn firefox_root(home: &Path) -> PathBuf {
    home.join("Library/Application Support/Firefox")
}

/// A corroborated Firefox profile: a directory holding `prefs.js`.
fn firefox_profile(home: &Path, name: &str) -> PathBuf {
    let p = firefox_root(home).join("Profiles").join(name);
    write(&p.join("prefs.js"), 10);
    p
}

fn rows_at(report: &macclean_core::privacy::PrivacyReport, path: &Path) -> usize {
    report.rows.iter().filter(|r| r.path == path).count()
}

fn has_row_named(report: &macclean_core::privacy::PrivacyReport, file_name: &str) -> bool {
    report
        .rows
        .iter()
        .any(|r| r.path.file_name().is_some_and(|n| n == file_name))
}

// --- enumeration -----------------------------------------------------------

/// The measured shape of the reference machine: no `Default`, three numbered
/// profiles, sitting among ~25 component directories that are not profiles.
#[test]
fn the_absence_of_a_default_profile_is_ordinary_and_profile_n_is_still_found() {
    let (_d, home) = fixture();
    let p1 = chromium_profile(&home, "Profile 1");
    write(&p1.join("Cookies"), 100);

    let report = scan(&cfg(&home));
    assert_eq!(rows_at(&report, &p1.join("Cookies")), 1);
}

/// A vendor directory that exists but holds no corroborated profile is not a
/// browser. Measured: five of the ten vendor directories on the reference
/// machine contain only `NativeMessagingHosts`, written by some other
/// installer — the browser was never run, or never installed at all.
#[test]
fn a_vendor_directory_holding_only_native_messaging_hosts_is_not_a_browser() {
    let (_d, home) = fixture();
    mkdir(&chrome_root(&home).join("NativeMessagingHosts"));

    let report = scan(&cfg(&home));
    assert!(report.rows.is_empty());
    let chrome = report.browser("google-chrome").unwrap();
    assert_eq!(chrome.profiles, 0);
}

/// The component directories beside the profiles — `Crashpad`, `GrShaderCache`,
/// `MEIPreload` and two dozen more — are not profiles, and a name pattern over
/// the directory listing is not what decides that. Corroboration does.
#[test]
fn a_chromium_component_directory_beside_the_profiles_is_never_a_profile() {
    let (_d, home) = fixture();
    chromium_profile(&home, "Profile 1");
    for junk in [
        "Crashpad",
        "GrShaderCache",
        "MEIPreload",
        "OptimizationHints",
    ] {
        // Give each one a cookie jar, so only corroboration can exclude it.
        write(&chrome_root(&home).join(junk).join("Cookies"), 50);
    }

    let report = scan(&cfg(&home));
    assert!(
        report.rows.is_empty(),
        "a component directory is not a profile"
    );
}

/// `System Profile` is Chromium's internal network context and `Guest Profile`
/// is ephemeral and browser-managed. Neither is a person's browsing, and both
/// carry a `Preferences` file, so corroboration alone would admit them.
#[test]
fn the_system_and_guest_profiles_are_never_enumerated() {
    let (_d, home) = fixture();
    for excluded in ["System Profile", "Guest Profile"] {
        let p = chromium_profile(&home, excluded);
        write(&p.join("Cookies"), 100);
    }

    let report = scan(&cfg(&home));
    assert!(report.rows.is_empty());
}

/// A directory that looks like a profile but has never been opened by the
/// browser is not one. Without this, any directory a user happens to create
/// under the vendor root becomes a search target.
#[test]
fn a_chromium_profile_without_a_preferences_file_is_not_corroborated() {
    let (_d, home) = fixture();
    let p = chrome_root(&home).join("Profile 1");
    write(&p.join("Cookies"), 100);

    let report = scan(&cfg(&home));
    assert!(report.rows.is_empty());
}

#[test]
fn firefox_profiles_are_corroborated_by_prefs_or_times() {
    let (_d, home) = fixture();
    let a = firefox_profile(&home, "abc.default-release");
    write(&a.join("cookies.sqlite"), 100);

    let bare = firefox_root(&home).join("Profiles/zzz.nothing");
    write(&bare.join("cookies.sqlite"), 100);

    let report = scan(&cfg(&home));
    assert_eq!(rows_at(&report, &a.join("cookies.sqlite")), 1);
    assert_eq!(rows_at(&report, &bare.join("cookies.sqlite")), 0);
}

/// Measured on the reference machine: one of three Firefox profiles holds only
/// `times.json`. That is an ordinary abandoned profile, not an error, and it
/// contributes nothing.
#[test]
fn an_abandoned_firefox_profile_holding_only_times_json_is_a_profile_with_no_rows() {
    let (_d, home) = fixture();
    let p = firefox_root(&home).join("Profiles/old.default");
    write(&p.join("times.json"), 10);

    let report = scan(&cfg(&home));
    assert!(report.rows.is_empty());
    assert_eq!(report.browser("firefox").unwrap().profiles, 1);
}

/// The whole point of enumerating by `read_dir` rather than by parsing
/// `profiles.ini` and `Local State`: a name that came from a file's *contents*
/// can say `../../Keychains`, and a name that came from a directory listing
/// cannot. This pins that no configuration file is consulted at all.
#[test]
fn a_profile_name_never_comes_from_a_parsed_configuration_file() {
    let (_d, home) = fixture();
    chromium_profile(&home, "Profile 1");
    // A hostile Local State naming a profile outside the vendor root.
    write(&chrome_root(&home).join("Local State"), 0);
    fs::write(
        chrome_root(&home).join("Local State"),
        r#"{"profile":{"info_cache":{"../../../Keychains":{},"../Mail":{}}}}"#,
    )
    .unwrap();
    // A hostile profiles.ini doing the same for Firefox.
    firefox_profile(&home, "real.default");
    fs::write(
        firefox_root(&home).join("profiles.ini"),
        "[Profile0]\nName=evil\nIsRelative=0\nPath=/\n\n[Profile1]\nIsRelative=1\nPath=../../../Keychains\n",
    )
    .unwrap();

    let report = scan(&cfg(&home));
    for row in &report.rows {
        for member in &row.members {
            assert!(
                member.starts_with(&home),
                "{} escaped the fixture home",
                member.display()
            );
        }
    }
    assert_eq!(report.browser("google-chrome").unwrap().profiles, 1);
    assert_eq!(report.browser("firefox").unwrap().profiles, 1);
}

// --- recognition: the negative core ---------------------------------------

/// The headline test. Every one of these is a sibling of something this module
/// does emit, and losing any of them is worse than never cleaning anything.
#[test]
fn a_profile_full_of_precious_files_yields_no_rows_at_all() {
    let (_d, home) = fixture();
    let ff = firefox_profile(&home, "abc.default-release");
    for precious in [
        "key4.db",
        "key3.db",
        "logins.json",
        "logins.db",
        "cert9.db",
        "cert8.db",
        "permissions.sqlite",
        "places.sqlite",
        "places.sqlite-wal",
        "favicons.sqlite",
        "formhistory.sqlite",
        "autofill-profiles.json",
        "containers.json",
        "user.js",
        "addons.json",
        "extension-preferences.json",
    ] {
        write(&ff.join(precious), 100);
    }
    mkdir(&ff.join("bookmarkbackups"));
    mkdir(&ff.join("extensions"));

    let chrome = chromium_profile(&home, "Profile 1");
    for precious in [
        "Login Data",
        "Login Data For Account",
        "Login Data-journal",
        "Web Data",
        "Web Data-journal",
        "Bookmarks",
        "Bookmarks.bak",
        "Secure Preferences",
        "Affiliation Database",
    ] {
        write(&chrome.join(precious), 100);
    }
    for precious in ["Extensions", "Local Extension Settings", "Sync Data"] {
        write(&chrome.join(precious).join("blob"), 100);
    }

    let report = scan(&cfg(&home));
    assert!(
        report.rows.is_empty(),
        "recognized something it must never touch: {:?}",
        report.rows.iter().map(|r| &r.path).collect::<Vec<_>>()
    );
}

/// Stated on its own because it is the single most expensive mistake available:
/// `key4.db` is the key that decrypts `logins.json`, and it sits beside a
/// cookie jar this module *does* emit. A prefix or fuzzy match reaches it.
#[test]
fn the_password_key_is_never_a_row_even_beside_a_cookie_jar_that_is() {
    let (_d, home) = fixture();
    let ff = firefox_profile(&home, "abc.default-release");
    write(&ff.join("cookies.sqlite"), 100);
    write(&ff.join("key4.db"), 100);
    write(&ff.join("logins.json"), 100);

    let report = scan(&cfg(&home));
    assert_eq!(rows_at(&report, &ff.join("cookies.sqlite")), 1);
    assert!(!has_row_named(&report, "key4.db"));
    assert!(!has_row_named(&report, "logins.json"));
}

/// The Chromium half of the same shape: `Login Data` is a byte-adjacent
/// sibling of `Cookies`, in the same directory, with the same extension-less
/// name shape.
#[test]
fn chrome_saved_passwords_are_never_a_row_even_though_they_sit_beside_cookies() {
    let (_d, home) = fixture();
    let p = chromium_profile(&home, "Profile 1");
    write(&p.join("Cookies"), 100);
    write(&p.join("Login Data"), 100);
    write(&p.join("Web Data"), 100);

    let report = scan(&cfg(&home));
    assert_eq!(rows_at(&report, &p.join("Cookies")), 1);
    assert!(!has_row_named(&report, "Login Data"));
    assert!(!has_row_named(&report, "Web Data"));
}

/// Firefox history is not on offer, and the reason is not caution: history and
/// bookmarks live in one file. Removing it takes the bookmarks with it, and
/// separating them would mean editing rows inside a database — a destructive
/// capability this tool does not have and should not grow by accident.
#[test]
fn firefox_history_is_not_offered_because_places_sqlite_also_holds_the_bookmarks() {
    let (_d, home) = fixture();
    let ff = firefox_profile(&home, "abc.default-release");
    write(&ff.join("places.sqlite"), 100);

    let report = scan(&cfg(&home));
    assert!(!has_row_named(&report, "places.sqlite"));
    assert!(report
        .browser("firefox")
        .unwrap()
        .notes
        .iter()
        .any(|n| n.contains("bookmarks")));
}

#[test]
fn a_file_this_module_does_not_recognise_is_not_a_row() {
    let (_d, home) = fixture();
    let p = chromium_profile(&home, "Profile 1");
    write(&p.join("SomethingChromeAddedLastTuesday"), 5_000);

    let report = scan(&cfg(&home));
    assert!(report.rows.is_empty());
}

/// Chromium keeps download history in a table inside `History`. There is no
/// file to offer, and inventing a row for one would be a lie.
#[test]
fn chromium_download_history_has_no_row_of_its_own() {
    let (_d, home) = fixture();
    let p = chromium_profile(&home, "Profile 1");
    write(&p.join("History"), 100);

    let report = scan(&cfg(&home));
    assert!(!has_row_named(&report, "Downloads"));
    assert_eq!(rows_at(&report, &p.join("History")), 1);
}

/// A canary on the inclusion lists themselves. Changing what this module is
/// willing to name must be a deliberate edit to this assertion.
/// A canary on the inclusion lists themselves. These names *are* the safety
/// boundary: everything not on them is invisible to this module. Widening the
/// lists must be a deliberate edit to this assertion, reviewed as such.
#[test]
fn the_recognised_names_are_pinned() {
    use macclean_core::privacy::{recognized_names, Family};

    let mut chromium = recognized_names(Family::Chromium);
    chromium.sort_unstable();
    assert_eq!(
        chromium,
        vec![
            "Application Cache",
            "Code Cache",
            "Cookies",
            "Current Session",
            "Current Tabs",
            "DawnCache",
            "File System",
            "GPUCache",
            "GraphiteDawnCache",
            "History",
            "IndexedDB",
            "Last Session",
            "Last Tabs",
            "Local Storage",
            "Network Action Predictor",
            "Network/Cookies",
            "Service Worker",
            "Session Storage",
            "Sessions",
            "ShaderCache",
            "Shortcuts",
            "Top Sites",
            "Visited Links",
            "blob_storage",
            "databases",
        ]
    );

    let mut firefox = recognized_names(Family::Firefox);
    firefox.sort_unstable();
    assert_eq!(
        firefox,
        vec![
            "cookies.sqlite",
            "sessionstore-backups",
            "sessionstore.jsonlz4",
            "shader-cache",
            "startupCache",
            "storage.sqlite",
            "storage/default",
            "thumbnails",
            "webappsstore.sqlite",
        ]
    );

    let mut safari = recognized_names(Family::Safari);
    safari.sort_unstable();
    assert_eq!(
        safari,
        vec![
            "Cookies.binarycookies",
            "Databases",
            "Downloads.plist",
            "History.db",
            "LastSession.plist",
            "LocalStorage",
            "RecentlyClosedTabs.plist",
            "TopSites.plist",
            "WebsiteData",
        ]
    );

    // The names that must never appear on any of them.
    let all: Vec<&str> = [Family::Chromium, Family::Firefox, Family::Safari]
        .into_iter()
        .flat_map(recognized_names)
        .collect();
    for precious in [
        "key4.db",
        "logins.json",
        "logins.db",
        "cert9.db",
        "places.sqlite",
        "Login Data",
        "Web Data",
        "Bookmarks",
        "Bookmarks.plist",
        "Preferences",
        "prefs.js",
    ] {
        assert!(
            !all.contains(&precious),
            "{precious} must never be recognised"
        );
    }
}

// --- confinement -----------------------------------------------------------

/// One profile's row must never be built from another profile's directory.
#[test]
fn a_neighbouring_profiles_cookie_jar_is_never_claimed_for_this_profile() {
    let (_d, home) = fixture();
    let one = chromium_profile(&home, "Profile 1");
    let two = chromium_profile(&home, "Profile 2");
    write(&one.join("Cookies"), 100);
    write(&two.join("Cookies"), 200);

    let report = scan(&cfg(&home));
    // `members`, not `path`. The disposal half confines every member against
    // `profile_root`, and `path` is only the last of them — asserting on `path`
    // would leave the invariant that actually authorizes disposal unpinned.
    for row in &report.rows {
        for member in &row.members {
            assert!(
                member.starts_with(&row.profile_root),
                "{} is not inside its own profile root {}",
                member.display(),
                row.profile_root.display()
            );
        }
    }
    assert_eq!(rows_at(&report, &one.join("Cookies")), 1);
    assert_eq!(rows_at(&report, &two.join("Cookies")), 1);
}

#[test]
fn every_emitted_path_is_canonical() {
    let (_d, home) = fixture();
    let p = chromium_profile(&home, "Profile 1");
    write(&p.join("Cookies"), 100);
    mkdir(&p.join("Sessions"));

    let report = scan(&cfg(&home));
    assert!(!report.rows.is_empty());
    for row in &report.rows {
        for member in &row.members {
            assert_eq!(
                fs::canonicalize(member).unwrap(),
                *member,
                "{} is not its own canonical spelling",
                member.display()
            );
        }
    }
}

/// A symlinked profile resolves elsewhere, so its rows would name paths the
/// user never saw. Dropped at discovery, so it never becomes a row at all.
#[test]
fn a_symlinked_profile_directory_is_refused_rather_than_resolved() {
    let (_d, home) = fixture();
    let real = home.join("Documents/decoy");
    write(&real.join("Preferences"), 10);
    write(&real.join("Cookies"), 100);
    mkdir(&chrome_root(&home));
    std::os::unix::fs::symlink(&real, chrome_root(&home).join("Profile 1")).unwrap();

    let report = scan(&cfg(&home));
    assert!(report.rows.is_empty());
    assert_eq!(report.skipped_symlink, 1);
}

#[test]
fn a_symlinked_cookie_jar_is_dropped_rather_than_followed() {
    let (_d, home) = fixture();
    let p = chromium_profile(&home, "Profile 1");
    let elsewhere = home.join("Documents/notes.txt");
    write(&elsewhere, 100);
    std::os::unix::fs::symlink(&elsewhere, p.join("Cookies")).unwrap();

    let report = scan(&cfg(&home));
    assert!(report.rows.is_empty());
    assert_eq!(report.skipped_symlink, 1);
}

/// M5's rows are disposed of by per-path grant. If one were inside the
/// disposal allowlist it would also be reachable without a grant, from the
/// ordinary cleaner — two routes to the same bytes, and a double count in the
/// combined total M7 has to produce.
#[test]
fn no_privacy_row_is_inside_the_disposal_allowlist() {
    let (_d, home) = fixture();
    let p = chromium_profile(&home, "Profile 1");
    write(&p.join("Cookies"), 100);
    let ff = firefox_profile(&home, "abc.default-release");
    write(&ff.join("cookies.sqlite"), 100);
    write(
        &home.join("Library/Caches/Google/Chrome/Profile 1/Cache/blob"),
        100,
    );

    let report = scan(&cfg(&home));
    let disposal = allowlist::default_roots(&home);
    assert!(!report.rows.is_empty());
    for row in &report.rows {
        for member in &row.members {
            assert!(
                !allowlist::is_allowed(member, &disposal),
                "{} is already cleanable without a grant",
                member.display()
            );
        }
    }
}

/// The other half of the same rule: what the ordinary cleaner already covers is
/// *named*, so the UI can say where it is handled — and carries no size, so it
/// is structurally impossible for M7 to add it to a total twice.
#[test]
fn the_browser_caches_another_category_cleans_are_named_without_a_size() {
    let (_d, home) = fixture();
    chromium_profile(&home, "Profile 1");
    write(
        &home.join("Library/Caches/Google/Chrome/Profile 1/Cache/blob"),
        100,
    );

    let report = scan(&cfg(&home));
    assert!(report
        .covered_elsewhere
        .iter()
        .any(|c| c.path.ends_with("Library/Caches/Google/Chrome") && c.category == "user-caches"));
}

#[test]
fn the_scan_mutates_nothing() {
    let (_d, home) = fixture();
    let p = chromium_profile(&home, "Profile 1");
    write(&p.join("Cookies"), 100);
    let before = fs::read(p.join("Cookies")).unwrap();
    let modified = fs::metadata(p.join("Cookies")).unwrap().modified().unwrap();

    let _ = scan(&cfg(&home));
    assert_eq!(fs::read(p.join("Cookies")).unwrap(), before);
    assert_eq!(
        fs::metadata(p.join("Cookies")).unwrap().modified().unwrap(),
        modified
    );
}

// --- sidecars --------------------------------------------------------------

#[test]
fn a_database_row_carries_its_wal_and_shm_as_members_of_one_row() {
    let (_d, home) = fixture();
    let ff = firefox_profile(&home, "abc.default-release");
    write(&ff.join("cookies.sqlite"), 100);
    write(&ff.join("cookies.sqlite-wal"), 50);
    write(&ff.join("cookies.sqlite-shm"), 20);

    let report = scan(&cfg(&home));
    let row = report
        .rows
        .iter()
        .find(|r| r.path == ff.join("cookies.sqlite"))
        .unwrap();
    assert_eq!(row.members.len(), 3);
    assert_eq!(row.size_bytes, 170);
}

#[test]
fn a_sidecar_is_never_a_row_of_its_own() {
    let (_d, home) = fixture();
    let ff = firefox_profile(&home, "abc.default-release");
    write(&ff.join("cookies.sqlite"), 100);
    write(&ff.join("cookies.sqlite-wal"), 50);

    let report = scan(&cfg(&home));
    assert_eq!(rows_at(&report, &ff.join("cookies.sqlite-wal")), 0);
}

/// The order is a safety property, not a formatting choice. `execute`
/// continues past a failed action, so a mid-sequence failure must leave the
/// database *present* with its sidecars gone (recoverable) and never a hot
/// `-journal` beside a newly created empty database (the corruption case).
#[test]
fn the_members_of_a_database_row_put_the_database_last() {
    let (_d, home) = fixture();
    let ff = firefox_profile(&home, "abc.default-release");
    write(&ff.join("cookies.sqlite"), 100);
    write(&ff.join("cookies.sqlite-wal"), 50);
    write(&ff.join("cookies.sqlite-shm"), 20);

    let report = scan(&cfg(&home));
    let row = report
        .rows
        .iter()
        .find(|r| r.path == ff.join("cookies.sqlite"))
        .unwrap();
    assert_eq!(*row.members.last().unwrap(), ff.join("cookies.sqlite"));
}

#[test]
fn a_database_with_no_sidecars_is_a_single_member_row() {
    let (_d, home) = fixture();
    let ff = firefox_profile(&home, "abc.default-release");
    write(&ff.join("cookies.sqlite"), 100);

    let report = scan(&cfg(&home));
    let row = report
        .rows
        .iter()
        .find(|r| r.path == ff.join("cookies.sqlite"))
        .unwrap();
    assert_eq!(row.members, vec![ff.join("cookies.sqlite")]);
}

// --- liveness --------------------------------------------------------------

/// A running browser holds the database open with a live write-ahead log, and
/// rewrites it on quit. "History removed" would be visibly false a minute
/// later, which is the failure this project treats as the worst one.
#[test]
fn a_chromium_singleton_lock_withholds_the_cookie_and_history_rows() {
    let (_d, home) = fixture();
    let p = chromium_profile(&home, "Profile 1");
    write(&p.join("Cookies"), 100);
    write(&p.join("History"), 100);
    std::os::unix::fs::symlink("somehost-1234", chrome_root(&home).join("SingletonLock")).unwrap();

    let report = scan(&cfg(&home));
    assert!(!report.rows.is_empty());
    for row in report
        .rows
        .iter()
        .filter(|r| r.class != Class::ProfileCache)
    {
        assert!(
            !row.offerable,
            "{} was offered while Chrome may be live",
            row.path.display()
        );
        assert!(row.withheld.is_some());
    }
}

/// Caches are the weaker case: the browser recreates them, and nothing the
/// user cares about is misreported. A caveat is proportionate.
#[test]
fn a_chromium_singleton_lock_leaves_the_cache_rows_offerable_with_a_caveat() {
    let (_d, home) = fixture();
    let p = chromium_profile(&home, "Profile 1");
    write(&p.join("GPUCache/blob"), 100);
    std::os::unix::fs::symlink("somehost-1234", chrome_root(&home).join("SingletonLock")).unwrap();

    let report = scan(&cfg(&home));
    let row = report
        .rows
        .iter()
        .find(|r| r.class == Class::ProfileCache)
        .unwrap();
    assert!(row.offerable);
    assert!(report.caveats.iter().any(|c| c.contains("running")));
}

#[test]
fn a_firefox_lock_symlink_withholds_the_cookie_row() {
    let (_d, home) = fixture();
    let ff = firefox_profile(&home, "abc.default-release");
    write(&ff.join("cookies.sqlite"), 100);
    std::os::unix::fs::symlink("127.0.0.1:+1234", ff.join("lock")).unwrap();

    let report = scan(&cfg(&home));
    let row = report
        .rows
        .iter()
        .find(|r| r.path == ff.join("cookies.sqlite"))
        .unwrap();
    assert!(!row.offerable);
}

/// The measured trap. `.parentlock` is present in a Firefox profile whether or
/// not Firefox is running — it is locked with `fcntl`, not created and removed
/// — so keying liveness on it would withhold every row forever while looking
/// exactly like it was working.
#[test]
fn a_firefox_parentlock_alone_never_withholds_anything() {
    let (_d, home) = fixture();
    let ff = firefox_profile(&home, "abc.default-release");
    write(&ff.join("cookies.sqlite"), 100);
    write(&ff.join(".parentlock"), 0);

    let report = scan(&cfg(&home));
    let row = report
        .rows
        .iter()
        .find(|r| r.path == ff.join("cookies.sqlite"))
        .unwrap();
    assert!(row.offerable, "`.parentlock` is not a liveness signal");
}

#[test]
fn a_withheld_live_browser_row_says_which_marker_made_it_withheld() {
    let (_d, home) = fixture();
    let p = chromium_profile(&home, "Profile 1");
    write(&p.join("Cookies"), 100);
    std::os::unix::fs::symlink("somehost-1234", chrome_root(&home).join("SingletonLock")).unwrap();

    let report = scan(&cfg(&home));
    let row = &report.rows[0];
    assert!(row.withheld.as_ref().unwrap().contains("SingletonLock"));
}

// --- consequence -----------------------------------------------------------

#[test]
fn no_cookie_row_is_ever_bulk_grantable() {
    let (_d, home) = fixture();
    let p = chromium_profile(&home, "Profile 1");
    write(&p.join("Cookies"), 100);

    let report = scan(&cfg(&home));
    let row = &report.rows[0];
    assert_eq!(row.consequence, Consequence::SignsYouOut);
    assert!(!row.bulk_grantable);
    assert!(!row.smart_scan_eligible);
}

#[test]
fn nothing_but_a_regenerable_row_is_eligible_for_smart_scan() {
    let (_d, home) = fixture();
    let p = chromium_profile(&home, "Profile 1");
    write(&p.join("Cookies"), 100);
    write(&p.join("History"), 100);
    write(&p.join("Current Session"), 100);
    write(&p.join("GPUCache/blob"), 100);

    let report = scan(&cfg(&home));
    for row in &report.rows {
        assert_eq!(
            row.smart_scan_eligible,
            row.consequence == Consequence::Regenerable,
            "{} claims the wrong Smart Scan eligibility",
            row.path.display()
        );
    }
}

/// Site storage is where a local-first web app keeps the user's only copy of
/// their work. Shown so the decision is made knowing it is there; never
/// offered. The same posture M4 took for a container's `Documents`.
#[test]
fn site_storage_is_shown_and_never_offered() {
    let (_d, home) = fixture();
    let p = chromium_profile(&home, "Profile 1");
    write(&p.join("Local Storage/leveldb/000003.log"), 100);
    write(&p.join("IndexedDB/blob"), 100);

    let report = scan(&cfg(&home));
    let storage: Vec<_> = report
        .rows
        .iter()
        .filter(|r| r.class == Class::SiteStorage)
        .collect();
    assert_eq!(storage.len(), 2);
    for row in storage {
        assert!(!row.offerable);
        assert!(row.withheld.is_some());
    }
}

#[test]
fn every_row_carries_a_consequence_that_matches_its_class() {
    let (_d, home) = fixture();
    let p = chromium_profile(&home, "Profile 1");
    write(&p.join("Cookies"), 100);
    write(&p.join("History"), 100);
    write(&p.join("Current Session"), 100);
    write(&p.join("GPUCache/blob"), 100);
    write(&p.join("IndexedDB/blob"), 100);

    let report = scan(&cfg(&home));
    for row in &report.rows {
        let expected = match row.class {
            Class::Cookies => Consequence::SignsYouOut,
            Class::History => Consequence::ErasesHistory,
            Class::Session => Consequence::LosesOpenTabs,
            Class::ProfileCache => Consequence::Regenerable,
            Class::SiteStorage => Consequence::LosesSiteData,
        };
        assert_eq!(row.consequence, expected);
    }
}

// --- access and honesty ----------------------------------------------------

/// These two look alike through `read_dir` and mean opposite things: one says
/// "grant access and try again", the other says "there is nothing here".
#[test]
fn a_missing_browser_root_is_reported_as_not_installed_not_as_denied() {
    let (_d, home) = fixture();
    let report = scan(&cfg(&home));
    assert_eq!(
        report.browser("google-chrome").unwrap().access,
        Access::NotInstalled
    );
    assert!(!report.is_partial());
}

#[test]
fn an_unreadable_browser_root_is_reported_as_needing_full_disk_access() {
    let (_d, home) = fixture();
    let root = chrome_root(&home);
    mkdir(&root);
    let mut perms = fs::metadata(&root).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o000);
    fs::set_permissions(&root, perms).unwrap();

    let report = scan(&cfg(&home));
    let access = report.browser("google-chrome").unwrap().access.clone();

    // Restore before asserting, so a failure cannot leave an unremovable dir.
    let mut perms = fs::metadata(&root).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    fs::set_permissions(&root, perms).unwrap();

    assert_eq!(access, Access::NeedsFullDiskAccess);
}

/// Deny a directory and restore it, whatever the assertions do.
fn while_denied<T>(dir: &Path, f: impl FnOnce() -> T) -> T {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(dir).unwrap().permissions();
    perms.set_mode(0o000);
    fs::set_permissions(dir, perms).unwrap();
    let out = f();
    let mut perms = fs::metadata(dir).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(dir, perms).unwrap();
    out
}

/// Safari being unreadable — its resting state without Full Disk Access — must
/// not suppress everything else. M4's "an unreadable `/Applications` refuses
/// the scan" does not apply: there is no authority question here, so an
/// unreadable root can only under-report, never mis-offer.
///
/// Denied, not absent. Those are the two states this module must never
/// conflate, so a test named "unreadable" that used an absent directory would
/// be checking the opposite branch.
#[test]
fn an_unreadable_safari_does_not_suppress_the_chromium_rows() {
    let (_d, home) = fixture();
    let p = chromium_profile(&home, "Profile 1");
    write(&p.join("Cookies"), 100);
    let safari = home.join("Library/Safari");
    write(&safari.join("History.db"), 100);

    let report = while_denied(&safari, || scan(&cfg(&home)));

    assert_eq!(
        report.browser("safari").unwrap().access,
        Access::NeedsFullDiskAccess
    );
    assert_eq!(rows_at(&report, &p.join("Cookies")), 1);
    assert!(report.is_partial(), "a denied root is a floor, and says so");
}

/// The probe opens the browser's root, but the data is a level below it, and
/// TCC gates each level on its own. A denial down there used to surface as
/// "installed files, but no profile this has ever opened" — a confident
/// falsehood, and exactly the conflation this module promises not to make.
#[test]
fn a_denial_below_the_probed_root_is_a_denial_not_an_absence() {
    let (_d, home) = fixture();
    let ff = firefox_profile(&home, "abc.default-release");
    write(&ff.join("cookies.sqlite"), 100);
    let profiles = firefox_root(&home).join("Profiles");

    let report = while_denied(&profiles, || scan(&cfg(&home)));

    assert_eq!(
        report.browser("firefox").unwrap().access,
        Access::NeedsFullDiskAccess
    );
    assert!(report.is_partial());
}

/// The Chromium half of the same shape: a profile directory that cannot be
/// read is not a directory that holds no profile.
#[test]
fn a_denied_chromium_profile_is_a_denial_not_a_missing_profile() {
    let (_d, home) = fixture();
    let p = chromium_profile(&home, "Profile 1");
    write(&p.join("Cookies"), 100);

    let report = while_denied(&p, || scan(&cfg(&home)));

    assert_eq!(
        report.browser("google-chrome").unwrap().access,
        Access::NeedsFullDiskAccess
    );
    assert!(report.is_partial());
}

/// A symlink that was dropped is something that was there and is not
/// reported. The count existed before and nothing read it.
#[test]
fn a_dropped_symlink_makes_the_report_partial() {
    let (_d, home) = fixture();
    let p = chromium_profile(&home, "Profile 1");
    let elsewhere = home.join("Documents/notes.txt");
    write(&elsewhere, 100);
    std::os::unix::fs::symlink(&elsewhere, p.join("Cookies")).unwrap();

    let report = scan(&cfg(&home));
    assert_eq!(report.skipped_symlink, 1);
    assert!(report.is_partial());
}

#[test]
fn withheld_rows_do_not_make_the_report_partial() {
    let (_d, home) = fixture();
    let p = chromium_profile(&home, "Profile 1");
    write(&p.join("Local Storage/leveldb/x"), 100);

    let report = scan(&cfg(&home));
    assert!(!report.rows.is_empty());
    assert!(!report.is_partial());
}

/// Reusing M4's flag through the shared `treewalk`: a row disposal is certain
/// to refuse is shown and never offered, so no checkbox can be ticked that the
/// executor would then reject.
#[test]
fn a_row_tree_containing_a_git_checkout_is_shown_and_not_offered() {
    let (_d, home) = fixture();
    let p = chromium_profile(&home, "Profile 1");
    write(&p.join("Sessions/blob"), 100);
    mkdir(&p.join("Sessions/vendored/.git"));

    let report = scan(&cfg(&home));
    let row = report
        .rows
        .iter()
        .find(|r| r.path == p.join("Sessions"))
        .unwrap();
    assert!(!row.offerable);
    assert!(row.undisposable.is_some());
}

// --- the tables ------------------------------------------------------------

#[test]
fn the_supported_browser_table_is_pinned() {
    let ids: Vec<&str> = BROWSERS.iter().map(|b| b.id).collect();
    assert_eq!(
        ids,
        vec![
            "safari",
            "google-chrome",
            "google-chrome-beta",
            "google-chrome-canary",
            "chromium",
            "microsoft-edge",
            "brave",
            "brave-beta",
            "brave-nightly",
            "vivaldi",
            "arc",
            "firefox",
        ]
    );
}

#[test]
fn every_unsupported_browser_is_named_with_its_reason() {
    assert!(!UNSUPPORTED.is_empty());
    for entry in UNSUPPORTED {
        assert!(!entry.name.is_empty());
        assert!(
            entry.reason.len() > 20,
            "{} needs a real reason, not a shrug",
            entry.name
        );
    }
}

/// Chromium has kept its cookie jar in two places, and which is live varies by
/// version and by profile. Both are searched, both are labelled the same, and
/// a profile that has both yields two rows — because both are real cookie jars
/// and leaving one behind would be the failure this module exists to prevent.
#[test]
fn both_chromium_cookie_layouts_are_searched_and_neither_is_called_the_old_one() {
    let (_d, home) = fixture();
    let p = chromium_profile(&home, "Profile 1");
    write(&p.join("Cookies"), 100);
    write(&p.join("Network/Cookies"), 200);

    let report = scan(&cfg(&home));
    let cookies: Vec<_> = report
        .rows
        .iter()
        .filter(|r| r.class == Class::Cookies)
        .collect();
    assert_eq!(cookies.len(), 2);
    for row in cookies {
        assert_eq!(row.label, "Cookies");
        assert!(row.offerable);
    }
}

// --- the checks that had no deletion resistance ----------------------------

/// The canonical-spelling re-check is the only thing standing between a
/// symlinked *intermediate component* and a row that looks confined but is not.
/// A row at `<profile>/Network/Cookies` is lexically inside its profile root,
/// so the disposal half's byte-equality ceiling and its root confinement would
/// both accept it — while the file it names lives wherever `Network` points.
#[test]
fn a_symlinked_path_component_inside_a_profile_is_refused() {
    let (_d, home) = fixture();
    let p = chromium_profile(&home, "Profile 1");
    let elsewhere = home.join("Documents/private");
    write(&elsewhere.join("Cookies"), 100);
    std::os::unix::fs::symlink(&elsewhere, p.join("Network")).unwrap();

    let report = scan(&cfg(&home));
    assert!(report.rows.is_empty());
    assert_eq!(report.skipped_symlink, 1);
}

/// The same rule one level higher: a profile reached through a symlinked
/// ancestor is not this profile, however much it looks like one.
#[test]
fn a_profile_reached_through_a_symlinked_ancestor_is_refused() {
    let (_d, home) = fixture();
    let real = home.join("Documents/elsewhere/Chrome");
    let inner = real.join("Profile 1");
    write(&inner.join("Preferences"), 10);
    write(&inner.join("Cookies"), 100);
    let vendor = home.join("Library/Application Support/Google");
    mkdir(&vendor);
    std::os::unix::fs::symlink(&real, vendor.join("Chrome")).unwrap();

    let report = scan(&cfg(&home));
    assert!(report.rows.is_empty());
    // Not merely "no rows" — the directory is not a profile at all. Without
    // this, the assertion above is satisfied by the per-entry canonical check
    // one layer down, and the layer being tested here could be deleted unseen.
    assert_eq!(report.browser("google-chrome").unwrap().profiles, 0);
}

/// A recognized name of the wrong *type* is not the thing this module means.
/// A directory called `Cookies` is not a cookie jar, and a file called
/// `Sessions` is not a session directory — emitting either would give the row
/// the wrong disposal shape, sending a tree through `guard` or a file through
/// `guard_dir`.
#[test]
fn a_recognised_name_of_the_wrong_type_is_not_a_row() {
    let (_d, home) = fixture();
    let p = chromium_profile(&home, "Profile 1");
    mkdir(&p.join("Cookies"));
    write(&p.join("Sessions"), 100);

    let report = scan(&cfg(&home));
    assert!(report.rows.is_empty());
}

/// A sidecar must be a regular file. A symlink named `Cookies-wal` would
/// otherwise become a member of the row, and so a disposal target pointing at
/// whatever it names.
#[test]
fn a_symlinked_sidecar_is_never_a_member_of_a_row() {
    let (_d, home) = fixture();
    let ff = firefox_profile(&home, "abc.default-release");
    write(&ff.join("cookies.sqlite"), 100);
    let elsewhere = home.join("Documents/private.txt");
    write(&elsewhere, 500);
    std::os::unix::fs::symlink(&elsewhere, ff.join("cookies.sqlite-wal")).unwrap();

    let report = scan(&cfg(&home));
    let row = report
        .rows
        .iter()
        .find(|r| r.path == ff.join("cookies.sqlite"))
        .unwrap();
    assert_eq!(row.members, vec![ff.join("cookies.sqlite")]);
    assert_eq!(row.size_bytes, 100);
    assert_eq!(report.skipped_symlink, 1);
}

/// A measurement that ran out of budget describes no tree truthfully. Offering
/// it would put a figure in front of a human that is not the figure they are
/// acting on — and an under-summed tree cannot trip the size threshold that
/// would otherwise have withheld it.
#[test]
fn a_directory_whose_measurement_was_cut_short_is_shown_and_not_offered() {
    let (_d, home) = fixture();
    let p = chromium_profile(&home, "Profile 1");
    for i in 0..40 {
        write(&p.join("GPUCache").join(format!("f{i}.bin")), 100);
    }

    let mut c = cfg(&home);
    c.max_examined = 5;
    let report = scan(&c);
    let row = report
        .rows
        .iter()
        .find(|r| r.path == p.join("GPUCache"))
        .unwrap();
    assert!(row.size_is_floor);
    assert!(!row.offerable, "a floor is not a figure anyone may act on");
    assert!(!row.bulk_grantable);
    assert!(report.is_partial());
}

// --- Safari ----------------------------------------------------------------

/// The loosening, and the shape of it.
///
/// Safari's container cookie jar **is** offered: M4's "no module offers a path
/// inside another app's container" solves a question of ownership that does not
/// arise for an app that is always installed and whose browsing data the user
/// has explicitly asked to clear. Withholding it meant the Safari half of this
/// module could act on nothing at all on a current Mac.
///
/// What is under `WebKit` is still withheld — for being website storage, which
/// is the true reason, rather than for being in a container.
#[test]
fn safaris_container_cookie_jar_is_offered_and_its_website_data_is_not() {
    let (_d, home) = fixture();
    let jar = home.join("Library/Containers/com.apple.Safari/Data/Library/Cookies");
    write(&jar.join("Cookies.binarycookies"), 100);
    let webkit = home.join("Library/Containers/com.apple.Safari/Data/Library/WebKit");
    write(&webkit.join("WebsiteData/blob"), 100);

    let report = scan(&cfg(&home));
    assert_eq!(report.rows.len(), 2);

    let jar_row = report
        .rows
        .iter()
        .find(|r| r.class == Class::Cookies)
        .unwrap();
    assert!(
        jar_row.offerable,
        "the container jar is where Safari's cookies now live"
    );
    assert_eq!(jar_row.consequence, Consequence::SignsYouOut);

    let data = report
        .rows
        .iter()
        .find(|r| r.class == Class::SiteStorage)
        .unwrap();
    assert!(!data.offerable);
    assert!(
        data.withheld.as_ref().unwrap().contains("website storage"),
        "the reason must be the true one, not that it sits in a container: {:?}",
        data.withheld
    );
}

/// `~/Library/Cookies` is not Safari's. It is the shared CFNetwork jar every
/// non-sandboxed app writes to, so offering it under a row that says "Safari"
/// would take consent against a false description.
#[test]
fn the_shared_cookie_jar_is_not_offered_as_safaris() {
    let (_d, home) = fixture();
    write(&home.join("Library/Cookies/Cookies.binarycookies"), 100);

    let report = scan(&cfg(&home));
    let row = report
        .rows
        .iter()
        .find(|r| r.path == home.join("Library/Cookies/Cookies.binarycookies"))
        .unwrap();
    assert!(!row.offerable);
    assert!(row
        .withheld
        .as_ref()
        .unwrap()
        .contains("every non-sandboxed app"));
}

#[test]
fn safaris_own_history_and_session_files_are_found() {
    let (_d, home) = fixture();
    let safari = home.join("Library/Safari");
    write(&safari.join("History.db"), 100);
    write(&safari.join("History.db-wal"), 50);
    write(&safari.join("Downloads.plist"), 10);
    write(&safari.join("TopSites.plist"), 10);

    let report = scan(&cfg(&home));
    let history = report
        .rows
        .iter()
        .find(|r| r.path == safari.join("History.db"))
        .unwrap();
    assert_eq!(history.members.len(), 2);
    assert_eq!(*history.members.last().unwrap(), safari.join("History.db"));
    assert!(history.offerable);

    // Top Sites is history in both families, or the consequence sentence shown
    // at the moment of consent would differ between two identical things.
    let top = report
        .rows
        .iter()
        .find(|r| r.path == safari.join("TopSites.plist"))
        .unwrap();
    assert_eq!(top.class, Class::History);
    assert_eq!(top.consequence, Consequence::ErasesHistory);

    // Safari leaves no marker saying whether it is running, so the report says
    // so rather than implying it checked.
    assert!(report.caveats.iter().any(|c| c.contains("no marker")));
}

/// Safari's four roots are gated independently, and the state must come from
/// the worst of them. A mixed report — some roots readable, one denied — is the
/// state a Mac without Full Disk Access is actually in, and it is the only
/// state in which the precedence rule does any work.
#[test]
fn a_denial_among_safaris_roots_wins_over_the_roots_that_read_fine() {
    let (_d, home) = fixture();
    let safari = home.join("Library/Safari");
    write(&safari.join("History.db"), 100);
    write(&home.join("Library/Cookies/Cookies.binarycookies"), 100);

    let report = while_denied(&safari, || scan(&cfg(&home)));

    assert_eq!(
        report.browser("safari").unwrap().access,
        Access::NeedsFullDiskAccess,
        "one readable root must not present a denied one as complete"
    );
    assert!(report.is_partial());
    // The readable root was still read — a denial elsewhere hides nothing.
    assert_eq!(
        rows_at(&report, &home.join("Library/Cookies/Cookies.binarycookies")),
        1
    );
}

/// A root can be readable and not searchable: mode `r--` lets `read_dir`
/// succeed, so every probe says "fine", and then every lookup inside it fails
/// with `EACCES`. Safari has no deeper probe, so this is the only place the
/// denial can be noticed at all — and reporting it as "holds nothing,
/// completely" is the conflation this module exists to avoid.
#[test]
fn a_root_that_is_readable_but_not_searchable_is_a_denial_not_an_emptiness() {
    use std::os::unix::fs::PermissionsExt;
    let (_d, home) = fixture();
    let safari = home.join("Library/Safari");
    write(&safari.join("History.db"), 100);
    write(&safari.join("Downloads.plist"), 10);

    let mut perms = fs::metadata(&safari).unwrap().permissions();
    perms.set_mode(0o444);
    fs::set_permissions(&safari, perms).unwrap();
    let report = scan(&cfg(&home));
    let mut perms = fs::metadata(&safari).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&safari, perms).unwrap();

    assert!(report.rows.is_empty());
    assert_eq!(
        report.browser("safari").unwrap().access,
        Access::NeedsFullDiskAccess
    );
    assert!(
        report.is_partial(),
        "an empty report from a denied root must never read as complete"
    );
}

/// A recognized name that is neither a directory nor a regular file — a
/// socket, a FIFO, a device node — is not the thing this module means. Without
/// the shape check it takes the file path and becomes an offerable row.
#[test]
fn a_recognised_name_that_is_a_socket_is_not_a_row() {
    use std::os::unix::net::UnixListener;
    let (_d, home) = fixture();
    // At the fixture root: a Unix socket path has a hard length limit
    // (`SUN_LEN`) that a realistic profile path under macOS's temp directory
    // exceeds. Safari's roots are shallow, so this is also a real shape.
    let safari = home.join("Library/Safari");
    mkdir(&safari);
    let short = home.join("s");
    fs::create_dir_all(&short).unwrap();
    let _listener = UnixListener::bind(short.join("History.db")).unwrap();
    fs::rename(short.join("History.db"), safari.join("History.db")).unwrap();

    let report = scan(&cfg(&home));
    assert!(
        !has_row_named(&report, "History.db"),
        "a socket is not a history database"
    );
}
