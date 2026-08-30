//! Space Lens — the read-only size walk behind the treemap.
//!
//! The interesting properties here are not "does it add up" (it does) but the
//! ones a size walk gets wrong in ways nobody notices:
//!
//! 1. **bytes are never lost.** Capping the tree's width and depth is a display
//!    decision; a rolled-up or depth-capped node still carries its bytes, so
//!    the total never disagrees with the picture drawn from it.
//! 2. **bytes are never invented.** Hard links occupy their blocks once and are
//!    counted once; symlinks own nothing and are counted as nothing.
//! 3. **it cannot see what it must not touch.** Protected subtrees are pruned,
//!    so nothing the denylist forbids ever appears in the picture.
//! 4. **it says when it is guessing.** Unreadable directories and a truncated
//!    walk are reported, not silently absorbed into a smaller number.
//!
//! SAFETY CONTRACT item 7: everything here runs against a throwaway tempdir.

use std::fs;
use std::path::{Path, PathBuf};

use macclean_core::spacelens::{measure, LensConfig, Node, MAX_WALK_DEPTH};

/// A fake home with the discovery-scope directories these tests use.
fn fixture_home() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(dir.path()).unwrap();
    for d in ["Documents", "Downloads"] {
        fs::create_dir_all(home.join(d)).unwrap();
    }
    (dir, home)
}

fn write_sized(path: &Path, bytes: u64) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, vec![0u8; bytes as usize]).unwrap();
}

/// Config scoped to the fixture's own directories.
fn cfg(home: &Path) -> LensConfig {
    let mut c = LensConfig::new(home.to_path_buf());
    c.roots = vec![home.join("Documents"), home.join("Downloads")];
    c
}

/// What the filesystem says a path occupies.
///
/// The tests assert against this rather than against the number of bytes
/// written, because allocated size is a property of the filesystem (block size,
/// compression, sparseness) and not of the write. What is being tested is the
/// *aggregation* — that whatever each file occupies is counted exactly once and
/// rolls up correctly — not `stat`'s own semantics.
fn allocated(path: &Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    fs::symlink_metadata(path).unwrap().blocks() * 512
}

fn root_named<'a>(roots: &'a [Node], name: &str) -> &'a Node {
    roots
        .iter()
        .find(|n| n.name == name)
        .unwrap_or_else(|| panic!("no root node named {name}"))
}

fn child_named<'a>(node: &'a Node, name: &str) -> &'a Node {
    node.children
        .iter()
        .find(|n| n.name == name)
        .unwrap_or_else(|| panic!("{} has no child named {name}", node.name))
}

/// The invariant the UI draws on: a node with children is exactly its children.
fn assert_sums_hold(node: &Node) {
    if !node.children.is_empty() {
        let sum: u64 = node.children.iter().map(|c| c.bytes).sum();
        assert_eq!(
            node.bytes, sum,
            "{} claims {} bytes but its children total {sum}",
            node.name, node.bytes
        );
    }
    for child in &node.children {
        assert_sums_hold(child);
    }
}

#[test]
fn it_rolls_sizes_up_from_the_leaves() {
    let (_g, home) = fixture_home();
    let deep = home.join("Documents/project/assets/render.mov");
    let shallow = home.join("Documents/notes.txt");
    write_sized(&deep, 40_000);
    write_sized(&shallow, 8_000);

    let report = measure(&cfg(&home));

    let docs = root_named(&report.roots, "Documents");
    assert_eq!(docs.bytes, allocated(&deep) + allocated(&shallow));
    assert_eq!(docs.files, 2);
    assert_eq!(report.total_bytes, docs.bytes);
    for root in &report.roots {
        assert_sums_hold(root);
    }
    // And the path down to it is materialized, largest first.
    let project = child_named(docs, "project");
    assert_eq!(project.bytes, allocated(&deep));
    assert!(project.is_dir);
    assert_eq!(docs.children[0].name, "project");
}

#[test]
fn roots_are_ordered_largest_first() {
    let (_g, home) = fixture_home();
    write_sized(&home.join("Documents/small.bin"), 4_000);
    write_sized(&home.join("Downloads/large.bin"), 80_000);

    let report = measure(&cfg(&home));

    assert_eq!(report.roots[0].name, "Downloads");
    assert_eq!(report.roots[1].name, "Documents");
}

#[test]
fn a_directory_past_the_depth_cap_still_carries_its_bytes() {
    let (_g, home) = fixture_home();
    let buried = home.join("Documents/a/b/c/d/huge.iso");
    write_sized(&buried, 60_000);

    let mut c = cfg(&home);
    c.max_depth = 2;
    let report = measure(&c);

    let docs = root_named(&report.roots, "Documents");
    // The total is unaffected by where the display stops.
    assert_eq!(docs.bytes, allocated(&buried));
    assert_eq!(docs.files, 1);

    // Two levels of children exist; the third does not.
    let a = child_named(docs, "a");
    let b = child_named(a, "b");
    assert!(b.children.is_empty(), "depth cap should stop at b");
    assert_eq!(b.bytes, allocated(&buried), "but b still owns the bytes");
    assert!(
        b.collapsed,
        "a node with hidden contents must say so, or the UI cannot tell it \
         apart from an empty directory"
    );
    assert!(!report.is_partial(), "a display cap is not an under-count");
}

#[test]
fn children_past_the_width_cap_are_rolled_up_not_dropped() {
    let (_g, home) = fixture_home();
    let mut total = 0;
    for i in 0..6 {
        let p = home.join(format!("Documents/file{i}.bin"));
        write_sized(&p, 4_000 * (i + 1));
        total += allocated(&p);
    }

    let mut c = cfg(&home);
    c.max_children = 2;
    let report = measure(&c);

    let docs = root_named(&report.roots, "Documents");
    assert_eq!(docs.bytes, total, "capping the width must not lose bytes");
    assert_eq!(docs.children.len(), 3, "2 kept + 1 rollup");
    assert_sums_hold(docs);

    // The two largest survive by name, and the rollup carries the rest.
    assert_eq!(docs.children[0].name, "file5.bin");
    assert_eq!(docs.children[1].name, "file4.bin");
    let rollup = &docs.children[2];
    assert_eq!(rollup.name, "4 more items");
    assert_eq!(rollup.files, 4);
    assert!(
        rollup.path.is_none(),
        "a rollup is not a place on disk and must not be addressable"
    );
    assert!(docs.collapsed);
}

#[test]
fn a_hard_linked_file_is_counted_once() {
    let (_g, home) = fixture_home();
    let original = home.join("Documents/original.bin");
    write_sized(&original, 40_000);
    // A second name for the same blocks. Both live under one root, which is
    // what makes this deterministic: the dedup set is shared across the
    // per-root threads, so a file hard-linked into *two* roots is still counted
    // once, but which root is charged for it depends on which thread got there
    // first. The total is the guarantee; the attribution is not.
    fs::hard_link(&original, home.join("Documents/alias.bin")).unwrap();

    let report = measure(&cfg(&home));

    let docs = root_named(&report.roots, "Documents");
    assert_eq!(
        docs.bytes,
        allocated(&original),
        "two names for one file occupy one file's blocks"
    );
    assert_eq!(docs.files, 1);
    assert_eq!(report.deduped_hardlinks, 1);
    assert!(
        !report.is_partial(),
        "deduplicating makes the figure more accurate, not less complete"
    );
}

#[test]
fn a_symlink_contributes_nothing_and_is_never_descended() {
    let (_g, home) = fixture_home();
    let real = home.join("Downloads/real.bin");
    write_sized(&real, 40_000);
    // A link to a file, and a link to a directory that contains it. Following
    // either would double-count; following the second would also loop.
    std::os::unix::fs::symlink(&real, home.join("Documents/link-to-file")).unwrap();
    std::os::unix::fs::symlink(home.join("Downloads"), home.join("Documents/link-to-dir")).unwrap();
    std::os::unix::fs::symlink(home.join("Documents"), home.join("Documents/link-to-self"))
        .unwrap();

    let report = measure(&cfg(&home));

    let docs = root_named(&report.roots, "Documents");
    assert_eq!(docs.bytes, 0, "a symlink owns none of its target's blocks");
    assert_eq!(docs.files, 0);
    assert!(docs.children.is_empty());
    assert_eq!(
        report.total_bytes,
        allocated(&real),
        "counted once, in Downloads"
    );
    assert_eq!(
        report.skipped_too_deep, 0,
        "the self-link must not be walked"
    );
}

// Two separate gates keep protected paths out of the picture, and they are
// tested separately on purpose: `resolve_roots` drops a protected *root* before
// the walk begins, while the per-entry check prunes a protected subtree found
// *during* it. A single test that passed a protected root and then asserted its
// contents were absent would be satisfied entirely by the first gate — and
// would keep passing with the second one deleted.

#[test]
fn a_protected_subtree_inside_a_root_is_pruned_during_the_walk() {
    let (_g, home) = fixture_home();
    let keep = home.join("Documents/keep.bin");
    write_sized(&keep, 8_000);
    // A repository's object store is exactly the kind of large, boring tree a
    // size walk is drawn to — and exactly the one the denylist forbids acting
    // on. Reached through an ordinary, entirely unprotected root, so only the
    // per-entry check can be what excludes it.
    write_sized(
        &home.join("Documents/repo/.git/objects/pack/pack.idx"),
        60_000,
    );

    let report = measure(&cfg(&home));

    assert_eq!(
        report.total_bytes,
        allocated(&keep),
        "only the unprotected file should be counted"
    );
    let names = flatten(&report.roots);
    for forbidden in [".git", "pack.idx"] {
        assert!(
            !names.iter().any(|n| n == forbidden),
            "{forbidden} appeared in the tree: {names:?}"
        );
    }
    // `repo` itself is not protected, so it is still drawn — as an empty
    // directory, which is the honest picture of what may be acted on there.
    assert!(names.iter().any(|n| n == "repo"));
}

#[test]
fn a_protected_root_is_never_walked_at_all() {
    let (_g, home) = fixture_home();
    let keep = home.join("Documents/keep.bin");
    write_sized(&keep, 8_000);
    // `~/Library` is protected as an *ancestor* of Keychains and Mail, so
    // handing it over as a root must produce no node whatsoever — not an empty
    // one, and certainly not a walk that then prunes its way down.
    write_sized(&home.join("Library/Keychains/login.keychain-db"), 40_000);
    write_sized(&home.join("Library/Mail/V10/big.emlx"), 40_000);

    let mut c = cfg(&home);
    c.roots = vec![home.join("Documents"), home.join("Library")];
    let report = measure(&c);

    assert_eq!(report.roots.len(), 1, "Library must not appear as a root");
    assert_eq!(report.roots[0].name, "Documents");
    assert_eq!(report.total_bytes, allocated(&keep));
    assert!(
        !report.is_partial(),
        "refusing to look somewhere forbidden is not the same as failing to \
         read it — this figure is complete for the scope that was asked for"
    );
}

#[test]
fn an_unreadable_directory_is_reported_rather_than_hidden() {
    use std::os::unix::fs::PermissionsExt;
    let (_g, home) = fixture_home();
    let readable = home.join("Documents/readable.bin");
    write_sized(&readable, 8_000);
    let locked = home.join("Documents/locked");
    write_sized(&locked.join("secret.bin"), 40_000);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

    let report = measure(&cfg(&home));

    // Restore before any assertion can fail and leave the tempdir un-removable.
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(report.skipped_unreadable, 1);
    assert!(
        report.is_partial(),
        "a walk that could not read part of the disk must not present its \
         total as complete"
    );
    // The rest of the walk still happened.
    assert_eq!(report.total_bytes, allocated(&readable));
}

#[test]
fn truncation_is_reported_and_bounded() {
    let (_g, home) = fixture_home();
    for i in 0..40 {
        write_sized(&home.join(format!("Documents/f{i}.bin")), 4_000);
    }

    let mut c = cfg(&home);
    c.max_examined = 5;
    let report = measure(&c);

    assert!(report.truncated);
    assert!(report.is_partial());
    assert!(
        report.examined <= 5,
        "the budget is a bound, not a suggestion: examined={}",
        report.examined
    );
}

#[test]
fn a_tree_deeper_than_the_recursion_bound_is_reported_not_silently_short() {
    let (_g, home) = fixture_home();
    let shallow = home.join("Documents/shallow.bin");
    write_sized(&shallow, 8_000);
    // One component past MAX_WALK_DEPTH. Nothing on a real Mac is this deep,
    // but the bound is what stops a pathological tree overflowing the stack,
    // and a bound that under-counts without saying so is the failure mode this
    // module is meant not to have.
    let mut deep = home.join("Documents");
    for i in 0..(MAX_WALK_DEPTH + 1) {
        deep = deep.join(format!("d{i}"));
    }
    write_sized(&deep.join("buried.bin"), 40_000);

    let mut c = cfg(&home);
    c.max_depth = 1;
    let report = measure(&c);

    assert!(report.skipped_too_deep > 0);
    assert!(
        report.is_partial(),
        "bytes below the recursion bound are missing from the total, so the \
         total is a floor and must present itself as one"
    );
    assert_eq!(
        report.total_bytes,
        allocated(&shallow),
        "everything above the bound is still measured"
    );
}

#[test]
fn an_empty_home_measures_zero_without_erroring() {
    let (_g, home) = fixture_home();

    let report = measure(&cfg(&home));

    assert_eq!(report.total_bytes, 0);
    assert_eq!(report.total_files, 0);
    assert!(!report.is_partial());
    assert_eq!(report.roots.len(), 2);
    assert!(report.roots.iter().all(|r| r.children.is_empty()));
}

#[test]
fn a_root_that_does_not_exist_is_simply_absent() {
    let (_g, home) = fixture_home();
    write_sized(&home.join("Documents/a.bin"), 8_000);

    let mut c = cfg(&home);
    c.roots.push(home.join("Movies")); // never created
    let report = measure(&c);

    assert_eq!(report.roots.len(), 2, "Movies contributes no node");
    assert!(
        !report.is_partial(),
        "a root that is not there is not an unreadable one"
    );
}

#[test]
fn the_same_disk_measures_the_same_way_twice() {
    let (_g, home) = fixture_home();
    for i in 0..12 {
        write_sized(&home.join(format!("Documents/d{i}/f.bin")), 4_000 * (i + 1));
        write_sized(&home.join(format!("Downloads/e{i}.bin")), 4_000);
    }

    let first = measure(&cfg(&home));
    let second = measure(&cfg(&home));

    // Threads walk the roots concurrently; the tree they produce must not
    // depend on which one finished first.
    assert_eq!(first, second);

    // Equality across two runs is weaker than it looks: `read_dir` returns the
    // same order for an unchanged directory, so a sort with no tie-break would
    // also satisfy it. The twelve Downloads files are deliberately all the same
    // size, so this pins the tie-break itself — without which the order is
    // whatever the directory happens to be laid out as, and the treemap
    // reshuffles the moment a file is touched.
    let downloads = root_named(&first.roots, "Downloads");
    let names: Vec<&str> = downloads.children.iter().map(|c| c.name.as_str()).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted, "equal-sized siblings must order by name");
}

/// Every node name in the tree, depth-first.
fn flatten(nodes: &[Node]) -> Vec<String> {
    let mut out = Vec::new();
    fn go(nodes: &[Node], out: &mut Vec<String>) {
        for n in nodes {
            out.push(n.name.clone());
            go(&n.children, out);
        }
    }
    go(nodes, &mut out);
    out
}

/// Total nodes in a tree, counting the roots themselves.
fn count_nodes(nodes: &[Node]) -> usize {
    nodes.iter().map(|n| 1 + count_nodes(&n.children)).sum()
}

#[test]
fn the_node_budget_stops_the_tree_growing_without_losing_a_byte() {
    let (_g, home) = fixture_home();
    // Wide and nested: 12 directories of 12 files each, well past a budget of
    // 10 nodes but nowhere near any of the other caps.
    let mut written = 0;
    for d in 0..12 {
        for f in 0..12 {
            write_sized(&home.join(format!("Documents/d{d:02}/f{f:02}.bin")), 4_000);
            written += 1;
        }
    }
    assert_eq!(written, 144);

    let mut unbounded = cfg(&home);
    unbounded.roots = vec![home.join("Documents")];
    let full = measure(&unbounded);

    let mut bounded = cfg(&home);
    bounded.roots = vec![home.join("Documents")];
    bounded.max_nodes = 10;
    let capped = measure(&bounded);

    // The figures are identical. This is the whole claim: the budget stops the
    // drawing, not the measuring.
    assert_eq!(capped.total_bytes, full.total_bytes);
    assert_eq!(capped.total_files, full.total_files);
    assert!(!capped.is_partial(), "a display cap is not an under-count");
    assert!(capped.node_budget_reached);
    assert!(!full.node_budget_reached);

    // And the tree really is smaller.
    assert!(
        capped.nodes < full.nodes,
        "capped {} vs full {}",
        capped.nodes,
        full.nodes
    );
    assert_eq!(
        capped.nodes,
        count_nodes(&capped.roots),
        "the count is real"
    );

    // Bounded, with the documented overshoot: pushes happen on the way back up,
    // so a chain of directories can each decide to materialize before any of
    // them has counted, and each contributes at most `max_children + 1`.
    let slack = bounded.max_depth * (bounded.max_children + 1);
    assert!(
        capped.nodes <= bounded.max_nodes + slack,
        "{} nodes exceeds {} + {slack} of documented slack",
        capped.nodes,
        bounded.max_nodes
    );

    // Every level that is drawn still adds up.
    for root in &capped.roots {
        assert_sums_hold(root);
    }
}

#[test]
fn a_directory_the_budget_stopped_says_there_is_more_inside() {
    let (_g, home) = fixture_home();
    for d in 0..12 {
        for f in 0..12 {
            write_sized(&home.join(format!("Documents/d{d:02}/f{f:02}.bin")), 4_000);
        }
    }

    let mut c = cfg(&home);
    c.roots = vec![home.join("Documents")];
    c.max_nodes = 10;
    let report = measure(&c);

    // Somewhere in the drawn tree is a directory with real bytes, no children,
    // and `collapsed` set — otherwise the UI would render it as empty.
    let mut stopped = 0;
    fn visit(node: &Node, stopped: &mut usize) {
        if node.is_dir && node.children.is_empty() && node.bytes > 0 {
            assert!(
                node.collapsed,
                "{} has bytes and no children but does not admit it",
                node.name
            );
            *stopped += 1;
        }
        for child in &node.children {
            visit(child, stopped);
        }
    }
    for root in &report.roots {
        visit(root, &mut stopped);
    }
    assert!(stopped > 0, "the budget should have stopped something");
}

#[test]
fn an_empty_directory_past_a_cap_does_not_claim_to_be_hiding_anything() {
    let (_g, home) = fixture_home();
    // `hollow` is at the depth cap *and* has nothing in it. The old rule set
    // `collapsed` on anything it declined to expand, which told the UI to print
    // "there is more inside" over a directory with nothing inside.
    fs::create_dir_all(home.join("Documents/a/hollow")).unwrap();
    write_sized(&home.join("Documents/a/full/big.bin"), 40_000);

    let mut c = cfg(&home);
    c.max_depth = 2;
    let report = measure(&c);

    let docs = root_named(&report.roots, "Documents");
    let a = child_named(docs, "a");
    let hollow = child_named(a, "hollow");
    let full = child_named(a, "full");

    assert!(
        !hollow.collapsed,
        "an empty directory is empty, not withholding"
    );
    assert_eq!(hollow.bytes, 0);
    assert!(full.collapsed, "but one with contents still says so");
}
