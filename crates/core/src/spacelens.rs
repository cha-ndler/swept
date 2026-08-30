//! Space Lens — a read-only, depth-capped directory-size walk.
//!
//! Answers one question: *where did the space actually go?* It produces a tree
//! of aggregated sizes over the same discovery scope [`crate::largeold`] uses,
//! suitable for a treemap or sunburst.
//!
//! # It cannot authorize anything
//!
//! Like Large & Old, this module never constructs a [`safety::SafePath`] and
//! never reaches the executor. Unlike Large & Old, it does not even produce a
//! list of *candidates* — a treemap node is a place on the disk, not a proposal
//! to act on it. Nothing here is selectable, so the discovery/disposal split
//! costs this module nothing:
//!
//! > Widen what we can see. Never widen what we can dispose of.
//!
//! One consequence is worth naming, because it is the opposite of the rule
//! Large & Old follows: this module renders non-UTF-8 names **lossily**. Large
//! & Old must not, because the disposal path identifies a selection by
//! byte-for-byte string equality with the path it emitted, so a lossy name
//! there would break that identity check. Here there is no disposal path to
//! break — a name is only ever drawn on screen — so a file called `caf\xE9.mov`
//! is shown as `caf<?>.mov` with its size intact rather than being dropped from
//! the picture. Its `path` is `None`, which is what says "you cannot address
//! this one".
//!
//! # Allocated bytes, not apparent bytes
//!
//! Every size here is `st_blocks × 512` — what the file *occupies*, which is
//! what `du` reports and what the volume's free-space figure responds to. Large
//! & Old reports `st_size`, the apparent length. For ordinary files the two
//! agree; for a sparse file or a compressed one they do not, and the same file
//! can legitimately show a different number in the two modules.
//!
//! That divergence is deliberate and each side is right for its own question
//! ("how big is this file?" vs "what is eating my disk?"), but it is a
//! user-visible inconsistency, and M7's single combined total will have to pick
//! one. See `ROADMAP.md`.
//!
//! # Honest, not fail-closed
//!
//! Same posture as Large & Old and for the same reason: a tree that refuses to
//! render because one directory was TCC-gated is useless on a stock Mac. So
//! unreadable directories are counted ([`LensReport::skipped_unreadable`]) and
//! the walk carries on, and [`LensReport::is_partial`] tells the UI to present
//! the figure as a floor.

use std::collections::HashSet;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use safety::allowlist;
use safety::denylist::is_protected;

use crate::largeold::resolve_roots;

/// `st_blocks` is defined in 512-byte units regardless of the filesystem's own
/// block size. This is a property of `stat(2)`, not of APFS.
const BLOCK_BYTES: u64 = 512;

/// How many levels of children below each root are materialized by default.
///
/// Sizes are always computed to the bottom of the tree — this caps the *shape*
/// of the DTO, not the accuracy of the numbers. Four is enough to reach
/// `~/Library/Application Support/<app>/<subdir>` while keeping the payload
/// small enough to hand a webview in one go.
pub const DEFAULT_MAX_DEPTH: usize = 4;

/// Largest children kept per node before the remainder is rolled up into a
/// single synthetic node. The rollup carries the leftover bytes, so capping
/// never loses space — it only stops the UI being handed 40,000 siblings.
pub const DEFAULT_MAX_CHILDREN: usize = 24;

/// Entries examined before the walk stops and says so. Generous — a stock home
/// is ~165k entries — while still guaranteeing the walk ends.
pub const DEFAULT_MAX_EXAMINED: usize = 1_000_000;

/// Nodes materialized into the tree before it stops growing.
///
/// The depth and width caps bound the tree's *shape*, not its *size*: 24
/// children over 4 levels admits a six-figure node count on a disk with enough
/// wide directories — `node_modules` is the obvious way to get one — and the
/// whole tree is serialized to the webview in a single payload. A real home is
/// nowhere near this, which is exactly why the absence of a bound would not be
/// noticed until it was.
///
/// Like the other two caps this is a *display* decision. Sizes are still
/// computed to the bottom of the tree, so nothing below the cap is lost from
/// the totals — the affected directories are simply drawn as `collapsed`.
pub const DEFAULT_MAX_NODES: usize = 20_000;

/// Hard bound on recursion, independent of `max_depth`.
///
/// `max_depth` only stops *materializing* nodes; the size walk keeps
/// descending, so without this a pathologically deep tree would overflow the
/// stack. Symlinks are never followed, so this cannot be reached by a loop —
/// only by a genuinely deep tree, which is why hitting it is reported rather
/// than ignored.
pub const MAX_WALK_DEPTH: usize = 64;

pub struct LensConfig {
    /// Canonical home directory (see [`safety::canonical_home`]).
    pub home: PathBuf,
    /// Roots to measure. Defaults to [`allowlist::discovery_roots`].
    pub roots: Vec<PathBuf>,
    /// Levels of children materialized below each root.
    pub max_depth: usize,
    /// Largest children kept per node before the rollup.
    pub max_children: usize,
    /// Entry budget, shared across all roots.
    pub max_examined: usize,
    /// Materialized-node budget, shared across all roots.
    pub max_nodes: usize,
}

impl LensConfig {
    pub fn new(home: PathBuf) -> Self {
        let roots = allowlist::discovery_roots(&home);
        Self {
            home,
            roots,
            max_depth: DEFAULT_MAX_DEPTH,
            max_children: DEFAULT_MAX_CHILDREN,
            max_examined: DEFAULT_MAX_EXAMINED,
            max_nodes: DEFAULT_MAX_NODES,
        }
    }
}

/// One rectangle in the treemap: a directory, a file, or a rollup of siblings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    /// Display name. Lossy for non-UTF-8 names — see the module docs.
    pub name: String,
    /// Where this is on disk, or `None` for a synthetic rollup node.
    pub path: Option<PathBuf>,
    /// Allocated bytes for this node and everything beneath it.
    pub bytes: u64,
    /// Regular files counted beneath this node.
    pub files: u64,
    pub is_dir: bool,
    /// Largest children first. Empty for files, for rollups, and for
    /// directories at the depth cap.
    pub children: Vec<Node>,
    /// True when `children` is **not** a complete listing of what is inside.
    ///
    /// Set by the depth cap, by the child cap, and on the rollup node itself.
    /// The UI needs it to distinguish "this directory is empty" from "there is
    /// more here than I am showing you".
    pub collapsed: bool,
}

/// The whole measurement.
///
/// Invariant the UI may rely on: for any node with children,
/// `bytes == children.iter().map(|c| c.bytes).sum()`. Capping and rolling up
/// never lose bytes; only the depth cap leaves a node with bytes and no
/// children, and that node is `collapsed`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LensReport {
    /// One node per resolvable, unprotected discovery root, largest first.
    pub roots: Vec<Node>,
    pub total_bytes: u64,
    pub total_files: u64,
    /// Directory entries looked at, including ones that contributed nothing.
    pub examined: usize,
    /// True if the walk stopped at `max_examined` before finishing. When set,
    /// **every** figure in the report is an under-count.
    pub truncated: bool,
    /// Directories that could not be read — almost always TCC.
    pub skipped_unreadable: usize,
    /// Directories deeper than [`MAX_WALK_DEPTH`], whose contents are missing
    /// from the totals.
    pub skipped_too_deep: usize,
    /// Nodes actually materialized into `roots`.
    pub nodes: usize,
    /// True if the tree stopped growing at `max_nodes`.
    ///
    /// Not a reason for [`Self::is_partial`], and deliberately so: like the
    /// depth cap this stops the *drawing*, not the *measuring*. Every byte
    /// below the cap is still in its ancestors' totals, and the directories
    /// that were not expanded are marked `collapsed` so the UI can say there is
    /// more inside without implying the figure is short.
    pub node_budget_reached: bool,
    /// Files seen more than once through different hard links, counted once.
    ///
    /// Not a reason for [`Self::is_partial`]: deduplicating makes the figure
    /// *more* accurate, not less.
    pub deduped_hardlinks: usize,
}

impl LensReport {
    /// True if the tree describes less than what is actually on disk. The UI
    /// must say so rather than presenting the total as complete.
    pub fn is_partial(&self) -> bool {
        self.truncated || self.skipped_unreadable > 0 || self.skipped_too_deep > 0
    }
}

/// Everything the per-root threads share.
struct Ctx<'a> {
    home: &'a Path,
    max_depth: usize,
    max_children: usize,
    max_examined: usize,
    examined: AtomicUsize,
    max_nodes: usize,
    /// Nodes materialized so far, across every root thread.
    nodes: AtomicUsize,
    /// `(dev, ino)` of every multiply-linked file already counted.
    ///
    /// Shared rather than per-root so that a file hard-linked into two
    /// different roots is counted once in the total. The cost is that *which*
    /// root gets charged for it depends on which thread arrived first — the
    /// total is deterministic, the attribution is not. Only locked for files
    /// with `nlink > 1`, which is a rounding error of a real home.
    seen_links: Mutex<HashSet<(u64, u64)>>,
}

/// Per-thread counters, merged into the report at the end.
#[derive(Default)]
struct Tally {
    unreadable: usize,
    too_deep: usize,
    deduped: usize,
    truncated: bool,
    node_budget_hit: bool,
}

/// What one directory contributed.
#[derive(Default)]
struct DirResult {
    bytes: u64,
    files: u64,
    children: Vec<Node>,
    collapsed: bool,
}

/// Measure the discovery roots.
///
/// Never mutates anything, never follows a symlink, and never descends into a
/// path the denylist protects.
pub fn measure(cfg: &LensConfig) -> LensReport {
    let roots = resolve_roots(&cfg.roots, &cfg.home);
    let ctx = Ctx {
        home: &cfg.home,
        max_depth: cfg.max_depth,
        max_children: cfg.max_children,
        max_examined: cfg.max_examined,
        examined: AtomicUsize::new(0),
        max_nodes: cfg.max_nodes,
        nodes: AtomicUsize::new(0),
        seen_links: Mutex::new(HashSet::new()),
    };

    // One thread per root. The walk is dominated by `lstat`, so the roots
    // overlap their I/O instead of queueing behind the largest one. Bounded by
    // the number of discovery roots (currently 8), so there is nothing to pool.
    let measured: Vec<(PathBuf, DirResult, Tally)> = std::thread::scope(|scope| {
        let handles: Vec<_> = roots
            .iter()
            .map(|root| {
                let ctx = &ctx;
                scope.spawn(move || {
                    let mut tally = Tally::default();
                    let result = measure_dir(root, 0, ctx, &mut tally);
                    (root.clone(), result, tally)
                })
            })
            .collect();
        handles
            .into_iter()
            // A panicking walk thread would be a bug in this module, not a
            // condition to paper over. Propagating keeps it loud.
            .map(|h| h.join().expect("space-lens walk thread panicked"))
            .collect()
    });

    let mut report = LensReport {
        // Clamped: every thread that trips the budget increments the counter
        // once more on its way out, and each of its still-unwinding ancestors
        // does the same, so the raw value overshoots by a little. The budget is
        // the ceiling that was actually honoured, and reporting a number above
        // it would be claiming work that never happened.
        examined: ctx.examined.load(Ordering::Relaxed).min(cfg.max_examined),
        ..LensReport::default()
    };
    for (path, result, tally) in measured {
        report.total_bytes = report.total_bytes.saturating_add(result.bytes);
        report.total_files = report.total_files.saturating_add(result.files);
        report.skipped_unreadable += tally.unreadable;
        report.skipped_too_deep += tally.too_deep;
        report.deduped_hardlinks += tally.deduped;
        report.truncated |= tally.truncated;
        report.node_budget_reached |= tally.node_budget_hit;
        report.roots.push(Node {
            name: display_name(&path),
            bytes: result.bytes,
            files: result.files,
            path: Some(path),
            is_dir: true,
            children: result.children,
            collapsed: result.collapsed,
        });
    }
    // The root nodes themselves are part of the tree the webview receives.
    report.nodes = ctx.nodes.load(Ordering::Relaxed) + report.roots.len();
    // Largest root first, so the caller does not have to re-sort and two runs
    // over the same disk produce the same tree despite the threads.
    sort_children(&mut report.roots);
    report
}

/// Convenience for callers that only have a `&Path`.
///
/// Canonicalizes `home` first, for the reason spelled out on
/// [`crate::largeold::find_in`]: `denylist` compares against `home`
/// component-wise, so a non-canonical one silently disables the
/// keychains/mail/home-root rules for the entire walk.
pub fn measure_in(home: &Path) -> LensReport {
    let home = safety::canonical_home(home).unwrap_or_else(|_| home.to_path_buf());
    measure(&LensConfig::new(home))
}

/// Recursive size walk. Returns this directory's totals plus, when within the
/// depth cap, its children as nodes.
fn measure_dir(dir: &Path, depth: usize, ctx: &Ctx, tally: &mut Tally) -> DirResult {
    if depth >= MAX_WALK_DEPTH {
        tally.too_deep += 1;
        return DirResult {
            collapsed: true,
            ..DirResult::default()
        };
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => {
            tally.unreadable += 1;
            return DirResult {
                collapsed: true,
                ..DirResult::default()
            };
        }
    };

    // Below either cap the sizes still roll up; only the nodes stop being built.
    //
    // The node budget is read once, here, rather than checked at each push. A
    // directory that starts materializing finishes doing so, because a
    // half-materialized listing would break the invariant the whole picture
    // rests on: `cap_children` folds the *unlisted* remainder into a rollup, so
    // children dropped after that decision would take their bytes with them and
    // a node would no longer equal the sum of what is drawn beneath it.
    //
    // The cost is a bounded overshoot. Pushes happen on the way back up, so a
    // chain of directories can all decide "yes" before any of them has counted,
    // and each contributes at most `max_children + 1`. The excess is therefore
    // capped at `max_depth * (max_children + 1)` per thread — a few hundred
    // nodes against a budget of twenty thousand.
    let materialize = depth < ctx.max_depth && ctx.nodes.load(Ordering::Relaxed) < ctx.max_nodes;
    if !materialize && depth < ctx.max_depth {
        tally.node_budget_hit = true;
    }
    let mut out = DirResult::default();
    // Whether this directory had anything in it at all, which is what separates
    // "there is more here than I am showing you" from "this is empty".
    let mut had_entries = false;

    for entry in entries {
        if ctx.examined.fetch_add(1, Ordering::Relaxed) >= ctx.max_examined {
            tally.truncated = true;
            break;
        }
        had_entries = true;
        let Ok(entry) = entry else {
            tally.unreadable += 1;
            continue;
        };
        let path = entry.path();
        // Prune protected subtrees at the top rather than at every leaf: this
        // is what keeps `.git` working trees, keychains and mail out of the
        // picture, and it means the walk never spends time inside them.
        if is_protected(&path, ctx.home) {
            continue;
        }
        // `DirEntry::file_type` does not follow symlinks (it comes from
        // `d_type`), so a symlink is neither `is_dir` nor `is_file` here and
        // falls through the bottom of this match contributing nothing. That is
        // both correct — the bytes belong to the target, and counting them here
        // would double-count them where the target lives — and what makes a
        // symlink loop impossible: a link to an ancestor is simply not
        // descended.
        let Ok(file_type) = entry.file_type() else {
            tally.unreadable += 1;
            continue;
        };

        if file_type.is_dir() {
            let sub = measure_dir(&path, depth + 1, ctx, tally);
            out.bytes = out.bytes.saturating_add(sub.bytes);
            out.files = out.files.saturating_add(sub.files);
            if materialize {
                out.children.push(Node {
                    name: display_name(&path),
                    bytes: sub.bytes,
                    files: sub.files,
                    path: addressable(&path),
                    is_dir: true,
                    children: sub.children,
                    collapsed: sub.collapsed,
                });
            }
        } else if file_type.is_file() {
            // `symlink_metadata`, not `entry.metadata()`: the latter *follows*
            // symlinks, so a link swapped in after `file_type` said "file"
            // would have its target's size charged here. Read-only, so the
            // worst case is a wrong number rather than a wrong deletion — but a
            // wrong number is the only thing this module produces.
            //
            // Not covered by a test, and verified so: swapping this for
            // `entry.metadata()` leaves the whole suite green, because a
            // symlink never reaches this branch except through a race that is
            // not reproducible in a fixture. It is a defensive choice, not a
            // pinned invariant, and is recorded here rather than left to look
            // like one.
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                tally.unreadable += 1;
                continue;
            };
            // A hard-linked file occupies its blocks once. Charging every name
            // for them would inflate the total above the volume's own figure,
            // which is the one number this module exists to explain.
            if meta.nlink() > 1 {
                let fresh = ctx
                    .seen_links
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert((meta.dev(), meta.ino()));
                if !fresh {
                    tally.deduped += 1;
                    continue;
                }
            }
            let bytes = meta.blocks().saturating_mul(BLOCK_BYTES);
            out.bytes = out.bytes.saturating_add(bytes);
            out.files += 1;
            if materialize {
                out.children.push(Node {
                    name: display_name(&path),
                    path: addressable(&path),
                    bytes,
                    files: 1,
                    is_dir: false,
                    children: Vec::new(),
                    collapsed: false,
                });
            }
        }
    }

    out.collapsed = if materialize {
        let folded = cap_children(&mut out.children, ctx.max_children);
        // Counted after capping, so each node in the final tree is counted
        // exactly once — the discarded siblings never existed as far as the
        // budget is concerned.
        ctx.nodes.fetch_add(out.children.len(), Ordering::Relaxed);
        folded
    } else {
        // Nodes were never built. Only claim there is more inside if there
        // actually was something to build from — an empty directory that
        // happened to fall past a cap is empty, not withholding.
        had_entries
    };
    out
}

/// Sort largest-first and fold everything past `max` into one rollup node.
///
/// Returns whether anything was folded — i.e. whether the remaining list is an
/// incomplete listing of what is actually there.
fn cap_children(children: &mut Vec<Node>, max: usize) -> bool {
    sort_children(children);
    if children.len() <= max {
        return false;
    }
    let rest = children.split_off(max);
    let bytes = rest.iter().fold(0u64, |a, n| a.saturating_add(n.bytes));
    let files = rest.iter().fold(0u64, |a, n| a.saturating_add(n.files));
    children.push(Node {
        name: format!("{} more items", rest.len()),
        // No path: this is not a place, and nothing may address it.
        path: None,
        bytes,
        files,
        is_dir: false,
        children: Vec::new(),
        collapsed: true,
    });
    true
}

/// Largest first, ties broken by name so the tree is stable across runs.
fn sort_children(children: &mut [Node]) {
    children.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.name.cmp(&b.name)));
}

/// The last component, lossily. Falls back to the whole path for a root with no
/// file name (`/`), which `resolve_roots` cannot currently produce but which
/// would otherwise render as an empty rectangle.
fn display_name(path: &Path) -> String {
    match path.file_name() {
        Some(n) => n.to_string_lossy().into_owned(),
        None => path.to_string_lossy().into_owned(),
    }
}

/// The path, but only if it survives the round trip to the UI intact.
///
/// A non-UTF-8 name still gets a node — its bytes are real and belong in the
/// picture — but no address, so nothing downstream can be handed a path it
/// would have to guess at.
fn addressable(path: &Path) -> Option<PathBuf> {
    path.to_str().map(|_| path.to_path_buf())
}
