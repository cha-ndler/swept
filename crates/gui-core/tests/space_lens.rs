//! Space Lens at the command layer.
//!
//! The walk itself is tested in `macclean-core`; what is tested here is the
//! boundary the webview actually sees — that the tree survives the conversion
//! and the JSON round trip with its sizes, its shape, and its honesty flags
//! intact, and that it stays what it is: a picture, with nothing in it that any
//! command will accept back.
//!
//! SAFETY CONTRACT item 7: everything here runs against a throwaway tempdir.

use std::fs;
use std::path::{Path, PathBuf};

use macclean_gui_core::{space_lens, SpaceNodeDto};

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

fn walk(nodes: &[SpaceNodeDto], visit: &mut dyn FnMut(&SpaceNodeDto)) {
    for n in nodes {
        visit(n);
        walk(&n.children, visit);
    }
}

#[test]
fn the_tree_reaches_the_ui_with_its_sizes_and_shape_intact() {
    let (_g, home) = fixture_home();
    write_sized(&home.join("Documents/project/render.mov"), 40_000);
    write_sized(&home.join("Downloads/installer.dmg"), 8_000);

    let report = space_lens(&home);

    assert!(report.total_bytes > 0);
    assert_eq!(report.total_files, 2);
    assert!(!report.partial);

    // The nesting survives the conversion, and every level still adds up.
    let mut seen_names = Vec::new();
    walk(&report.roots, &mut |n| {
        seen_names.push(n.name.clone());
        if !n.children.is_empty() {
            let sum: u64 = n.children.iter().map(|c| c.bytes).sum();
            assert_eq!(n.bytes, sum, "{} does not equal its children", n.name);
        }
    });
    for expected in ["Documents", "project", "render.mov", "installer.dmg"] {
        assert!(
            seen_names.iter().any(|n| n == expected),
            "{expected} missing from {seen_names:?}"
        );
    }
}

#[test]
fn the_payload_serializes_with_the_field_names_the_frontend_reads() {
    let (_g, home) = fixture_home();
    write_sized(&home.join("Documents/a.bin"), 8_000);

    let json = serde_json::to_value(space_lens(&home)).unwrap();

    for key in [
        "roots",
        "total_bytes",
        "total_files",
        "examined",
        "truncated",
        "skipped_unreadable",
        "skipped_too_deep",
        "deduped_hardlinks",
        "partial",
    ] {
        assert!(json.get(key).is_some(), "report is missing `{key}`");
    }
    let node = &json["roots"][0];
    for key in [
        "name",
        "path",
        "bytes",
        "files",
        "is_dir",
        "collapsed",
        "children",
    ] {
        assert!(node.get(key).is_some(), "node is missing `{key}`");
    }
    // No selection state anywhere: there is no command that takes one of these
    // back, and a field the UI could tick would be the wrong shape for that.
    assert!(node.get("selected").is_none());
}

#[test]
fn a_rollup_node_is_serialized_without_an_address() {
    let (_g, home) = fixture_home();
    // More siblings than the default width cap, so a rollup is produced.
    for i in 0..(macclean_core::spacelens::DEFAULT_MAX_CHILDREN + 5) {
        write_sized(&home.join(format!("Documents/f{i:03}.bin")), 4_000);
    }

    let report = space_lens(&home);

    let mut rollups = Vec::new();
    walk(&report.roots, &mut |n| {
        if n.path.is_none() {
            rollups.push(n.clone());
        }
    });
    assert_eq!(rollups.len(), 1, "one rollup, in Documents");
    assert!(rollups[0].collapsed);
    assert!(rollups[0].bytes > 0, "it carries the bytes it stands for");

    // `null`, not an empty string: the UI must be able to tell "no address"
    // from "the empty path", because only one of those is safe to ignore.
    let json = serde_json::to_value(&rollups[0]).unwrap();
    assert!(json["path"].is_null());
}

#[test]
fn a_walk_that_could_not_read_everything_says_so() {
    use std::os::unix::fs::PermissionsExt;
    let (_g, home) = fixture_home();
    let locked = home.join("Documents/locked");
    write_sized(&locked.join("secret.bin"), 40_000);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

    let report = space_lens(&home);

    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(report.skipped_unreadable, 1);
    assert!(
        report.partial,
        "the UI needs this to present the total as a floor rather than a fact"
    );
}
