//! Large & Old Files — the first walk that looks outside the disposal scope.
//!
//! Two things are being tested, and the second matters more than the first:
//!
//! 1. it finds the big files (otherwise the feature is pointless);
//! 2. it cannot, by construction, authorize anything — and it never returns a
//!    path that a later `guard` would refuse, so the UI can never offer a row
//!    that silently fails when the user picks it.
//!
//! SAFETY CONTRACT item 7: everything here runs against a throwaway tempdir.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use macclean_core::largeold::{find, Found, LargeOldConfig};

/// A fake home with the discovery-scope directories that matter here.
fn fixture_home() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(dir.path()).unwrap();
    for d in ["Documents", "Downloads", "Library/Caches"] {
        fs::create_dir_all(home.join(d)).unwrap();
    }
    (dir, home)
}

fn write_sized(path: &Path, bytes: u64) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, vec![0u8; bytes as usize]).unwrap();
}

/// Config scoped to the fixture's own directories, with a small threshold so
/// the tests do not need to write hundreds of megabytes.
fn cfg(home: &Path) -> LargeOldConfig {
    let mut c = LargeOldConfig::new(home.to_path_buf());
    c.roots = vec![home.join("Documents"), home.join("Downloads")];
    c.min_size = 1024;
    c
}

fn paths(items: &[Found]) -> Vec<String> {
    items
        .iter()
        .map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn it_finds_files_over_the_threshold_largest_first() {
    let (_g, home) = fixture_home();
    write_sized(&home.join("Documents/big.iso"), 8192);
    write_sized(&home.join("Documents/medium.bin"), 4096);
    write_sized(&home.join("Downloads/small.txt"), 100); // under threshold

    let report = find(&cfg(&home));

    assert_eq!(paths(&report.items), vec!["big.iso", "medium.bin"]);
    assert_eq!(report.matched, 2);
    assert_eq!(report.matched_bytes, 8192 + 4096);
}

#[test]
fn the_age_filter_excludes_recently_touched_files() {
    let (_g, home) = fixture_home();
    let old = home.join("Documents/old.iso");
    let fresh = home.join("Documents/fresh.iso");
    write_sized(&old, 8192);
    write_sized(&fresh, 8192);

    // Backdate one by 400 days.
    let long_ago = filetime::FileTime::from_unix_time(
        filetime::FileTime::now().unix_seconds() - 400 * 24 * 3600,
        0,
    );
    filetime::set_file_mtime(&old, long_ago).unwrap();

    let mut c = cfg(&home);
    c.min_age = Some(Duration::from_secs(365 * 24 * 3600));
    let report = find(&c);

    assert_eq!(paths(&report.items), vec!["old.iso"]);
}

#[test]
fn a_git_working_tree_is_pruned_entirely() {
    // The headline safety property for this walk. A repository's big files are
    // exactly the kind of thing a size-ranked list would surface first — and
    // every one of them would be refused by `guard` if the user picked it. So
    // the row must never appear at all.
    let (_g, home) = fixture_home();
    write_sized(&home.join("Documents/project/.git/objects/pack.bin"), 8192);
    write_sized(&home.join("Documents/project/build.log"), 8192);
    write_sized(&home.join("Documents/loose.iso"), 8192);

    let report = find(&cfg(&home));

    // `build.log` is fine — it is not inside `.git`, only next to it.
    assert_eq!(paths(&report.items), vec!["build.log", "loose.iso"]);
    assert!(
        !report
            .items
            .iter()
            .any(|f| f.path.to_string_lossy().contains(".git")),
        "nothing from inside a .git may be offered"
    );
}

#[test]
fn nothing_it_returns_would_be_refused_by_the_guard() {
    // The invariant that keeps the UI honest: every row is something a later
    // `guard` will accept, so picking one can never silently fail. Asserted
    // over the whole result set rather than a hand-picked case.
    let (_g, home) = fixture_home();
    write_sized(&home.join("Documents/a.iso"), 4096);
    write_sized(&home.join("Documents/nested/deep/b.iso"), 4096);
    write_sized(&home.join("Downloads/c.iso"), 4096);
    write_sized(&home.join("Documents/repo/.git/big.pack"), 4096);

    let report = find(&cfg(&home));

    assert!(!report.items.is_empty());
    for item in &report.items {
        assert!(
            safety::guard(&item.path, &home).is_ok(),
            "{} was offered but the guard refuses it",
            item.path.display()
        );
    }
}

#[test]
fn the_walk_never_mints_a_disposal_authority() {
    // `Found` carries a PathBuf, not a SafePath. This is a compile-time
    // property, so the test documents it and pins the observable half: the
    // returned path is inert until something re-guards it.
    let (_g, home) = fixture_home();
    write_sized(&home.join("Documents/a.iso"), 4096);

    let report = find(&cfg(&home));
    let item = &report.items[0];

    // Nothing in the discovery result is in the *disposal* allowlist.
    assert!(
        !safety::allowlist::is_allowed(&item.path, &safety::allowlist::default_roots(&home)),
        "a discovery result must not be disposable by default"
    );
}

#[test]
fn a_symlink_to_a_big_file_is_not_reported_as_big() {
    // The bytes belong to the target; removing the link reclaims none of them.
    // Reporting it would overstate what the user is about to get back.
    //
    // The threshold here is deliberately 1 byte. At a realistic threshold a
    // symlink is filtered out for being small (its own size is just the length
    // of the target path), so the test would pass whether or not the walk
    // checks `is_file` at all — which is exactly how an earlier version of this
    // test passed against a mutation that removed the check.
    let (_g, home) = fixture_home();
    let target = home.join("Documents/real.iso");
    write_sized(&target, 8192);
    std::os::unix::fs::symlink(&target, home.join("Downloads/link.iso")).unwrap();

    let mut c = cfg(&home);
    c.min_size = 1;
    let report = find(&c);

    assert_eq!(paths(&report.items), vec!["real.iso"]);
    assert_eq!(report.matched_bytes, 8192, "counted once, not twice");
    assert_eq!(report.matched, 1, "the symlink is not a match at any size");
}

#[test]
fn a_directory_is_never_reported() {
    // The executor refuses directory targets outright, so offering one would
    // be offering a row that cannot work.
    let (_g, home) = fixture_home();
    fs::create_dir_all(home.join("Documents/a-big-looking-folder")).unwrap();
    write_sized(&home.join("Documents/a-big-looking-folder/inner.iso"), 4096);

    let report = find(&cfg(&home));

    assert_eq!(paths(&report.items), vec!["inner.iso"]);
}

#[test]
fn results_are_capped_but_the_totals_stay_truthful() {
    // Capping the rows must never understate how much matched — that is the
    // difference between a short list and a wrong number.
    let (_g, home) = fixture_home();
    for i in 0..10 {
        write_sized(&home.join(format!("Documents/f{i}.iso")), 2048 + i * 16);
    }

    let mut c = cfg(&home);
    c.max_results = 3;
    let report = find(&c);

    assert_eq!(report.items.len(), 3, "only three rows kept");
    assert_eq!(report.matched, 10, "but all ten are counted");
    assert_eq!(
        report.matched_bytes,
        (0..10).map(|i| 2048 + i * 16).sum::<u64>(),
        "and all ten bytes are reported"
    );
    assert!(report.is_partial(), "the UI must be told this is a subset");
    // The cap keeps the LARGEST, not the first seen.
    assert_eq!(paths(&report.items), vec!["f9.iso", "f8.iso", "f7.iso"]);
}

#[test]
fn hitting_the_walk_bound_is_reported_not_hidden() {
    let (_g, home) = fixture_home();
    for i in 0..20 {
        write_sized(&home.join(format!("Documents/f{i}.iso")), 2048);
    }

    let mut c = cfg(&home);
    c.max_examined = 5;
    let report = find(&c);

    assert!(report.truncated, "a bounded walk must say it was bounded");
    assert!(report.is_partial());
    assert!(report.examined <= 5);
}

#[test]
fn an_unreadable_directory_is_counted_rather_than_failing_the_walk() {
    // Deliberately unlike `guard_dir`, which fails closed. This walk only
    // decides what to show a human, and TCC makes some directories unreadable
    // on every stock Mac — refusing to show anything would make the feature
    // useless. What it must not do is under-report silently.
    use std::os::unix::fs::PermissionsExt;

    let (_g, home) = fixture_home();
    write_sized(&home.join("Documents/visible.iso"), 4096);
    let locked = home.join("Documents/locked");
    fs::create_dir_all(&locked).unwrap();
    write_sized(&locked.join("hidden.iso"), 4096);

    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let report = find(&cfg(&home));
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(paths(&report.items), vec!["visible.iso"]);
    assert_eq!(report.skipped_unreadable, 1);
    assert!(
        report.is_partial(),
        "the figure is a floor, and must say so"
    );
}

#[test]
fn a_root_symlinked_into_a_protected_location_is_skipped() {
    // This is what canonicalizing the root actually buys. `is_protected` is a
    // component-wise check on the path as given, so an uncanonicalized root
    // named `Downloads` would sail past it while resolving somewhere the
    // denylist forbids — and the walk would then list files it must never
    // offer.
    let (_g, home) = fixture_home();
    let mail = home.join("Library/Mail");
    write_sized(&mail.join("messages.db"), 8192);

    let linked = home.join("Downloads");
    fs::remove_dir_all(&linked).unwrap();
    std::os::unix::fs::symlink(&mail, &linked).unwrap();

    let mut c = cfg(&home);
    c.roots = vec![linked];
    let report = find(&c);

    assert!(
        report.items.is_empty(),
        "a root resolving into a protected location must yield nothing, got {:?}",
        paths(&report.items)
    );
}

#[test]
fn a_symlinked_root_is_followed_to_its_target() {
    // ~/Documents is a symlink on any Mac keeping Documents in iCloud Drive,
    // so the walk has to see through it. (This one holds because `read_dir`
    // follows a symlinked root regardless — it documents the behaviour rather
    // than gating the canonicalize call; the test above is the one that gates
    // it.)
    let (_g, home) = fixture_home();
    let real = home.join("CloudStorage/Docs");
    write_sized(&real.join("big.iso"), 8192);

    let linked = home.join("Downloads");
    fs::remove_dir_all(&linked).unwrap();
    std::os::unix::fs::symlink(&real, &linked).unwrap();

    let mut c = cfg(&home);
    c.roots = vec![linked];
    let report = find(&c);

    assert_eq!(paths(&report.items), vec!["big.iso"]);
}

#[test]
fn a_missing_root_is_not_an_error() {
    let (_g, home) = fixture_home();
    write_sized(&home.join("Documents/a.iso"), 4096);

    let mut c = cfg(&home);
    c.roots.push(home.join("Movies")); // never created
    let report = find(&c);

    assert_eq!(paths(&report.items), vec!["a.iso"]);
    assert!(
        !report.is_partial(),
        "a root that does not exist hides nothing"
    );
}

#[test]
fn the_default_roots_are_the_discovery_scope_not_the_disposal_scope() {
    let (_g, home) = fixture_home();
    let c = LargeOldConfig::new(home.clone());

    assert_eq!(c.roots, safety::allowlist::discovery_roots(&home));
    for root in &c.roots {
        assert!(
            !safety::allowlist::is_allowed(root, &safety::allowlist::default_roots(&home)),
            "{} must not be in the disposal scope",
            root.display()
        );
    }
}

#[test]
fn the_walk_mutates_nothing() {
    let (_g, home) = fixture_home();
    write_sized(&home.join("Documents/a.iso"), 4096);
    write_sized(&home.join("Documents/nested/b.iso"), 4096);

    let before = listing(&home);
    find(&cfg(&home));
    assert_eq!(before, listing(&home), "discovery is read-only");
}

fn listing(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            out.push(p.strip_prefix(root).unwrap().display().to_string());
            if p.is_dir() {
                stack.push(p);
            }
        }
    }
    out.sort();
    out
}
