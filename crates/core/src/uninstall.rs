//! Uninstaller — finding an application's leftovers, read-only.
//!
//! Given a bundle identifier, this reports the per-user files an application
//! left behind: caches, preferences, saved state, launch agents and the like.
//! It is the discovery half. Nothing here disposes of anything, and nothing
//! here can authorize a disposal — the escalation is a per-path grant that runs
//! [`safety::guard`] at that point, exactly as Large & Old does.
//!
//! # The property that matters more than the feature
//!
//! Finding leftovers is easy. Not claiming *somebody else's* data while doing
//! it is the whole problem, and it is why the matching rule in [`claim`] is the
//! most carefully bounded code in this module:
//!
//! > An entry is leftover data of bundle id **X** iff its name, with exactly
//! > the suffix its own location declares stripped exactly once, splits on `.`
//! > into segments of which **X**'s segments are a byte-exact prefix — **and**
//! > the longest *installed* bundle id that is also such a prefix is **X**
//! > itself, or there is none.
//!
//! Three things in that sentence are load-bearing, and each was arrived at by
//! finding the case that breaks the obvious alternative:
//!
//! 1. **Segments, never bytes.** `com.acme.Note` is a byte-prefix of
//!    `com.acme.Notes`, so a `starts_with` matcher hands one vendor's cache to
//!    a different app. Nothing downstream objects: the denylist has no opinion
//!    about who owns `~/Library/Caches/com.acme.Notes`.
//! 2. **Byte-exact case.** Everywhere else in this codebase case folding can
//!    only *protect* more paths, and is right. Here it can only *claim* more
//!    paths, and is wrong — in both directions at once. On a case-insensitive
//!    volume two ids differing by case share one directory, so folding offers a
//!    co-tenant's data; on a case-sensitive one they are two directories owned
//!    by two vendors, so folding offers a stranger's. A near-miss is reported
//!    ([`LeftoverReport::skipped_case_variant`]) rather than silently dropped.
//! 3. **The longest installed owner wins.** `com.acme.Suite.Reader` is
//!    segment-prefixed by `com.acme.Suite`, but if Reader is still installed
//!    that directory is a live app's data. Withheld, and said so.
//!
//! Nested bundle ids — helpers and XPC services inside an installed `.app` —
//! are harvested into the [`OwnerIndex`] and used **only to withhold**, never
//! to claim. That is what disposes of the shared-helper case (one crash
//! reporter embedded in six different vendors' apps) without needing a special
//! rule: its id is in the index, so every target that is not it is refused, and
//! the index is a set, so how many apps embed it does not change the answer.
//!
//! # Three surfaces that are not id-keyed directories
//!
//! **A container is decomposed, never offered whole.** `~/Library/Containers/
//! <id>` is the app's redirected home, and `Data/Documents` is where a
//! sandboxed app puts the user's only copy of a file — Finder does not show
//! it. Rows come from [`CONTAINER_STATE_PARTS`], an *inclusion* list under
//! `Data` (an exclusion list fails open the next time Apple adds a directory),
//! and the parts in [`CONTAINER_USER_DATA_PARTS`] are shown as
//! [`Kind::UserData`] and never offered. The container root itself is never an
//! offerable row.
//!
//! **A human name is a weaker key, and gated three times.** Most apps name
//! their `Application Support` directory after themselves rather than their
//! id, so [`leftovers_for_named`] accepts a [`DisplayName`] and, in that one
//! location, offers a directory whose name is byte-equal to it — provided no
//! installed app answers to the name, and at least one immediate child is
//! keyed on the target's id. Rows carry [`MatchedVia::DisplayName`] and are
//! never bulk-grantable. The corroboration gate is strict enough that on the
//! reference machine it admits 4 of 89 human-named directories; that is the
//! intended trade until a human loosens it.
//!
//! **A group container is shown and never claimed.** It is shared between
//! apps by construction, and the entitlement that would settle ownership is
//! in the bundle that is, by premise, gone. One whose name resembles the id is
//! reported as [`Kind::Shared`] so the user knows it exists.
//!
//! # A note for the disposal half
//!
//! Confining a selection to the resolved location roots is **not** enough once
//! containers are searched: `<container>/Data/Documents` is inside a location
//! root and must never be acted on. The disposal half has to intersect the
//! selection with the `offerable` rows of a fresh scan — the way Large & Old
//! re-walks before it acts — rather than trust a path's prefix.
//!
//! # Sizes are per name, not per inode
//!
//! [`crate::spacelens`] counts a hard-linked file once, because it is
//! explaining where the volume's space went. This module does the opposite and
//! counts every name, because a disposal unlinks *names* — telling the user
//! they will reclaim a deduplicated figure would put a number in front of them
//! that no action can produce. Same data, opposite question.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::largeold::resolve_roots;
use safety::DirLimits;

use crate::treewalk;

/// Entry budget for a whole run, shared across every location and every
/// per-row size walk.
///
/// The locations themselves hold on the order of a thousand entries; what can
/// blow up is sizing one row, since a single Electron cache routinely holds
/// six figures of files.
pub const DEFAULT_MAX_EXAMINED: usize = 200_000;

pub use crate::treewalk::MAX_ROW_DEPTH;

/// How deep the inventory looks for `.app` bundles.
///
/// Not 1. On a stock machine there are roughly three times as many bundles
/// within four levels as at the top level — helpers, `Contents/Library`
/// services, and apps filed in subfolders — and those nested ones own real
/// leftovers. An inventory that misses them reports installed apps as
/// uninstalled, which is the one direction this module must never be wrong in.
pub const APP_SCAN_DEPTH: usize = 4;

/// Surfaces this module deliberately does not search, with the reason.
///
/// Carried on every report so a caller cannot render a leftover list as though
/// it were everything.
pub const DEFERRED_LOCATIONS: &[(&str, &str)] = &[
    (
        "~/Library/Cookies",
        "a cookie jar signs the user out of things; it belongs to the Privacy module and \
         arrives with that module's consequence label, not as a cache-like part",
    ),
    (
        "~/Library/Application Scripts, ~/Library/Autosave Information",
        "not measured on the reference machine, and the location list is closed on purpose — \
         a surface is added deliberately, with a canary edit, or not at all",
    ),
    (
        "/Library",
        "denylisted: the tool can never act there, so a row would be an offer it cannot honour",
    ),
];

/// Under `<container>/Data`, the parts that are the application's own
/// regenerable state — the **only** parts of a container this module offers.
///
/// An inclusion list, not an exclusion list, on purpose: "everything except
/// the user's folders" fails open the next time Apple adds a directory under
/// `Data/Library`. Anything not named here is not a row.
///
/// `Library/Cookies` is deliberately absent even though it is regenerable: a
/// cookie jar is the Privacy module's surface and arrives with that module's
/// consequence label, not silently as a cache-like part.
pub const CONTAINER_STATE_PARTS: &[&str] = &[
    "Library/Caches",
    "Library/HTTPStorages",
    "Library/Logs",
    "Library/Preferences",
    "Library/Saved Application State",
    "Library/WebKit",
    "tmp",
];

/// Under `<container>/Data`, the parts that hold the user's data rather than
/// the application's. Shown when non-empty, never offered.
///
/// `Documents` is where a sandboxed app puts the user's only copy of a file,
/// and Finder does not surface it. `Library/Application Support` is where the
/// same app keeps its databases — for a notes app, the notes. Both are
/// leftovers in the narrow sense once the app is gone, and the last copy of
/// something in the wide one.
pub const CONTAINER_USER_DATA_PARTS: &[&str] = &["Documents", "Library/Application Support"];

const CONTAINER_USER_DATA_REASON: &str = "a sandboxed app keeps the user's own data here — \
     possibly the only copy — so it is shown, not offered";

const GROUP_CONTAINER_REASON: &str = "a group container is shared between apps by \
     construction, and the entitlement that would settle who owns it was in the bundle that \
     is gone";

/// The one caveat this half can know in advance, surfaced on any report that
/// holds a preferences row. Nothing is quit or stopped to prevent it — that
/// would be an action, and this half performs none.
pub const CFPREFSD_CAVEAT: &str = "a preferences file can be written back by cfprefsd moments \
     after it is removed, if the app is running or is launched again; nothing is quit or \
     stopped to prevent that";

// ---------------------------------------------------------------------------
// Identity
// ---------------------------------------------------------------------------

/// A `CFBundleIdentifier` validated as usable as a match key.
///
/// The newtype exists so an unvalidated string cannot reach the matcher.
/// [`BundleId::parse`] rejects the shapes that would turn an identifier into a
/// wildcard or a path separator — which matters because `.` is already a regex
/// metacharacter and `*` is already a glob, and an id is attacker-influenced
/// data the moment it comes from a bundle someone else wrote.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct BundleId(String);

impl BundleId {
    /// `None` for anything that must never become a match key.
    ///
    /// Rejects: empty, longer than 255 bytes, fewer than two dot-separated
    /// segments, any empty segment (`com..x`, `.com.x`, `com.x.`), and any byte
    /// outside `[A-Za-z0-9._-]` — which is what rules out `*`, `[`, `/`, `..`,
    /// NUL and whitespace in one predicate rather than a denylist of characters
    /// somebody has to keep complete.
    pub fn parse(raw: &str) -> Option<Self> {
        if raw.is_empty() || raw.len() > 255 {
            return None;
        }
        let mut segments = 0usize;
        for segment in raw.split('.') {
            if segment.is_empty() {
                return None;
            }
            if !segment
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            {
                return None;
            }
            segments += 1;
        }
        (segments >= 2).then(|| BundleId(raw.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn segments(&self) -> std::str::Split<'_, char> {
        self.0.split('.')
    }
}

impl fmt::Display for BundleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A human-readable application name validated as usable as a match key.
///
/// Compared by byte-exact equality against a directory name and never joined
/// onto a path, so the validation is about shape rather than metacharacters:
/// empty, `.`, `..`, a separator or a NUL can never be a `read_dir` entry name
/// and would only ever be a bug in the caller. Nothing is trimmed or folded —
/// a name that differs from the directory by a byte does not match it, and
/// that is the under-match this tier is built to prefer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayName(String);

impl DisplayName {
    pub fn parse(raw: &str) -> Option<Self> {
        if raw.is_empty() || raw.len() > 255 || raw == "." || raw == ".." {
            return None;
        }
        if raw.bytes().any(|b| b == b'/' || b == 0) {
            return None;
        }
        Some(DisplayName(raw.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One installed application bundle, as the inventory saw it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledApp {
    /// Absolute canonical path of the `.app`. A plain `PathBuf` — this module
    /// never mints a `SafePath`, and `/Applications` is denylisted anyway.
    pub bundle_path: PathBuf,
    pub id: BundleId,
    /// `CFBundleName`, else `CFBundleDisplayName`, else the `.app` file stem.
    pub display_name: Option<String>,
    /// Every name the bundle answers to — `CFBundleName`,
    /// `CFBundleDisplayName` and the `.app` file stem — so the name tier can
    /// withhold a directory any of them would claim.
    pub names: BTreeSet<String>,
}

/// Every bundle id owned by something currently installed.
///
/// Membership answers exactly one question — *is anything still installed using
/// data keyed on this id?* — so a set is the right shape. Which app owns it
/// does not change the answer, which is why a helper embedded in several
/// installed apps needs no special case.
#[derive(Clone, Debug, Default)]
pub struct OwnerIndex {
    ids: BTreeSet<BundleId>,
    /// Every name an installed bundle answers to, and which ids answer to it.
    /// Consulted only to withhold: a name here is never a match key.
    names: BTreeMap<String, BTreeSet<BundleId>>,
}

impl OwnerIndex {
    pub fn contains(&self, id: &BundleId) -> bool {
        self.ids.contains(id)
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Installed apps that answer to `name`, byte-exact.
    pub fn owners_of_name(&self, name: &str) -> Option<&BTreeSet<BundleId>> {
        self.names.get(name)
    }

    /// The longest installed id whose segments prefix `stem`, if any.
    ///
    /// Prefix chains over a fixed string are totally ordered, so "longest" is
    /// unique and there is no tie to break.
    pub fn longest_owner(&self, stem: &str) -> Option<&BundleId> {
        self.ids
            .iter()
            .filter(|id| segment_prefix(stem, id))
            .max_by_key(|id| id.as_str().len())
    }
}

/// Whether the application is still installed — established by looking.
///
/// Deliberately two states, not three. An inventory that could not read one of
/// its roots aborts the run with [`UninstallError::InventoryIncomplete`]
/// instead of reporting a third "unknown" state, because "I could not check
/// whether this app is still installed" must never be rendered next to rows a
/// user can be talked into granting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Residence {
    /// Found, at these paths. There are no leftovers — there are files.
    Installed(Vec<PathBuf>),
    /// Not found under any of `searched`.
    NotFound { searched: Vec<PathBuf> },
}

// ---------------------------------------------------------------------------
// What was found
// ---------------------------------------------------------------------------

/// A place leftovers are looked for. Also the primary sort key, so a report is
/// deterministic across runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Location {
    Caches,
    Containers,
    HttpStorages,
    WebKit,
    Preferences,
    PreferencesByHost,
    SavedApplicationState,
    LaunchAgents,
    Logs,
    ApplicationSupport,
    GroupContainers,
}

impl Location {
    /// Home-relative path. Not `allowlist::discovery_roots` and not
    /// `default_roots`: this is the module's own list, so adding to it cannot
    /// widen the Large & Old walk or the disposal scope by accident.
    fn subpath(self) -> &'static str {
        match self {
            Location::Caches => "Library/Caches",
            Location::Containers => "Library/Containers",
            Location::HttpStorages => "Library/HTTPStorages",
            Location::WebKit => "Library/WebKit",
            Location::Preferences => "Library/Preferences",
            Location::PreferencesByHost => "Library/Preferences/ByHost",
            Location::SavedApplicationState => "Library/Saved Application State",
            Location::LaunchAgents => "Library/LaunchAgents",
            Location::Logs => "Library/Logs",
            Location::ApplicationSupport => "Library/Application Support",
            Location::GroupContainers => "Library/Group Containers",
        }
    }

    pub fn as_str(self) -> &'static str {
        self.subpath()
    }
}

/// Every location this module searches, in report order.
pub const SEARCHED_LOCATIONS: &[Location] = &[
    Location::Caches,
    Location::Containers,
    Location::HttpStorages,
    Location::WebKit,
    Location::Preferences,
    Location::PreferencesByHost,
    Location::SavedApplicationState,
    Location::LaunchAgents,
    Location::Logs,
    Location::ApplicationSupport,
    Location::GroupContainers,
];

/// How an entry came to be attributed to the target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MatchedVia {
    /// The stem is exactly the id.
    Id,
    /// The stem is the id and the name carried the location's own suffix.
    IdWithSuffix(&'static str),
    /// `<id>.<more segments>`, with no installed app owning the longer id — an
    /// orphaned extension or helper of the target.
    SiblingSegment(String),
    /// The name is the id with a group-container prefix (`group.` or a team
    /// id) removed. Only ever on a [`Kind::Shared`] row, which is never
    /// offerable — so this is the one prefix strip in the module, and it can
    /// only show, never claim.
    IdWithPrefix(String),
    /// The directory name is byte-equal to the display name the caller
    /// supplied, and an id-keyed child corroborates it. The weak tier.
    DisplayName(String),
}

/// What a row is, which decides whether this module may ever offer it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// The application's own regenerable state. The only kind that can be
    /// offerable.
    Leftover,
    /// Inside a container's redirected home: the user's data, not the app's.
    /// Shown so a decision about the container is made knowing it is there;
    /// never offerable here.
    UserData,
    /// A group container whose name resembles the id. Shared between apps by
    /// construction and never claimable; shown so the user knows it exists.
    Shared,
}

/// One thing found. Deliberately **not** a `SafePath`: something to show a
/// human, not something anyone may act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    /// Canonical, and always `<a resolved location root>/<a read_dir entry>` —
    /// or, for a container part, that followed by `Data/<an inclusion-list
    /// part>`. Never the container root, and never built from a parsed string.
    pub path: PathBuf,
    pub location: Location,
    pub matched_via: MatchedVia,
    /// What this row is. Only [`Kind::Leftover`] can be offerable.
    pub kind: Kind,
    /// Apparent size (`st_size`), summed over every name beneath this entry.
    ///
    /// Apparent rather than allocated, matching `scanner.rs`, because this
    /// figure feeds the same disposal preview those do — two numbers for the
    /// same pending action would be worse than either.
    pub size_bytes: u64,
    /// Names beneath this entry, counting a hard link once per name.
    pub file_count: u64,
    /// True when the size walk could not see all of this entry, so the figure
    /// is a floor.
    pub size_is_floor: bool,
    /// May a human be offered this row at all?
    pub offerable: bool,
    /// May it be swept up in a single "select everything" gesture?
    ///
    /// Distinct from `offerable` on purpose: a row matched by a *longer* id
    /// than the one the user typed is a judgement call they should have to make
    /// per row, even though it is legitimate to offer.
    pub bulk_grantable: bool,
    /// Why this row may not be acted on, when `offerable` is false.
    pub withheld: Option<String>,
    /// Why [`safety::guard_dir`] is *certain* to refuse this row, if it is: a
    /// protected path inside the tree, or a tree outside the same
    /// [`DirLimits`] disposal applies. Set implies `offerable == false` — a
    /// checkbox that is certain to fail is a lie of a different shape, and
    /// this half is the only place that knows in advance.
    pub undisposable: Option<&'static str>,
    /// A licence, activation or receipt shape among the immediate children,
    /// by **name only** — nothing is opened. Keeps the row out of any bulk
    /// gesture; it is not a reason to withhold it.
    pub license_suspected: bool,
}

/// What a leftover search found.
#[derive(Clone, Debug)]
pub struct LeftoverReport {
    pub target: BundleId,
    pub residence: Residence,
    /// Sorted by location, then path. Empty whenever the app is still
    /// installed — see [`Residence`].
    pub rows: Vec<Candidate>,
    pub examined: usize,
    pub truncated: bool,
    /// Directories that could not be read — almost always TCC.
    pub skipped_unreadable: usize,
    /// Entries dropped for being symlinks. A leftover that is a symlink points
    /// at data somewhere else, and disposing of it there is not what the row
    /// said it would do.
    pub skipped_symlink: usize,
    /// Entries whose stem folds ASCII-equal to the target but is not
    /// byte-equal. Reported so the under-match is visible rather than silent.
    pub skipped_case_variant: usize,
    /// Entries whose names are not valid UTF-8, so they cannot survive the
    /// round trip to a UI and back byte-for-byte — which is what a later
    /// grant's identity check depends on.
    pub skipped_unrepresentable: usize,
    /// Rows found and deliberately not offered (still-installed owner, live
    /// launch agent).
    pub withheld_count: usize,
    /// `Application Support` directories that matched the caller's display
    /// name but held nothing keyed on the id. Not shown: without corroboration
    /// there is no evidence they are related at all, and a "related" row on
    /// the caller's word alone is a guess.
    ///
    /// Deliberately not part of [`Self::is_partial`]: declining a name match
    /// is the tier working.
    pub skipped_uncorroborated_name: usize,
    /// Surfaces not searched at all. See [`DEFERRED_LOCATIONS`].
    pub deferred: &'static [(&'static str, &'static str)],
    /// What a human should know before acting on these rows, that this half
    /// can know in advance. See [`CFPREFSD_CAVEAT`].
    pub caveats: Vec<&'static str>,
}

impl LeftoverReport {
    /// True when the search saw less than it tried to see. The UI must present
    /// the figures as a floor when this is set.
    ///
    /// Withheld rows do **not** make a report partial: withholding is the
    /// module working correctly, and a caveat that fires on correct behaviour
    /// teaches people to ignore it.
    ///
    /// A floor counts only on an *offerable* row. A withheld row's figure is
    /// informational — it is in no total a user can act on — and a tree
    /// withheld *because* it holds a protected path is always a floor, since
    /// the protected part is not measured. Counting that would make every
    /// correctly-withheld `.git` tree a "partial" report.
    pub fn is_partial(&self) -> bool {
        self.truncated
            || self.skipped_unreadable > 0
            || self.skipped_symlink > 0
            || self.skipped_case_variant > 0
            || self.skipped_unrepresentable > 0
            || self.rows.iter().any(|r| r.offerable && r.size_is_floor)
    }

    pub fn total_bytes(&self) -> u64 {
        self.rows
            .iter()
            .filter(|r| r.offerable)
            .fold(0u64, |a, r| a.saturating_add(r.size_bytes))
    }
}

#[derive(Debug)]
pub enum UninstallError {
    /// The target's `CFBundleIdentifier` is not a shape that can be a match
    /// key. Refused rather than matched loosely.
    UnmatchableId(String),
    /// An application root exists but could not be read, so the "is it still
    /// installed?" question has no answer. Refused: reporting leftovers for an
    /// app that might still be installed is the one mistake this module must
    /// not make.
    InventoryIncomplete { root: PathBuf, reason: String },
}

impl fmt::Display for UninstallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UninstallError::UnmatchableId(raw) => write!(
                f,
                "refused: {raw:?} is not a usable bundle identifier, and matching it loosely \
                 could claim another application's data"
            ),
            UninstallError::InventoryIncomplete { root, reason } => write!(
                f,
                "refused: could not read {} ({reason}), so whether this app is still installed \
                 is unknown",
                root.display()
            ),
        }
    }
}

impl Error for UninstallError {}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

pub struct UninstallConfig {
    /// Canonical home directory (see [`safety::canonical_home`]).
    pub home: PathBuf,
    /// Where to look for installed applications.
    pub app_roots: Vec<PathBuf>,
    /// Which leftover locations to search.
    pub locations: Vec<Location>,
    pub max_examined: usize,
    pub app_scan_depth: usize,
    /// The bounds disposal will apply through `guard_dir`, so a row that
    /// already exceeds them is shown and not offered. Injectable **only** so a
    /// fixture can reach them — 50,000 files is not a tempdir test — and pinned
    /// equal to `DirLimits::default()` by a test, because if the two diverge
    /// the flag lies in the dangerous direction.
    pub dir_limits: DirLimits,
}

impl UninstallConfig {
    /// The bounds a per-row measurement is judged against.
    fn bounds(&self) -> crate::treewalk::Bounds {
        crate::treewalk::Bounds {
            home: self.home.clone(),
            dir_limits: self.dir_limits,
            max_examined: self.max_examined,
        }
    }

    pub fn new(home: PathBuf) -> Self {
        let app_roots = inventory_roots(&home);
        Self {
            home,
            app_roots,
            locations: SEARCHED_LOCATIONS.to_vec(),
            max_examined: DEFAULT_MAX_EXAMINED,
            app_scan_depth: APP_SCAN_DEPTH,
            dir_limits: DirLimits::default(),
        }
    }
}

/// Where installed applications live.
///
/// **These are deliberately NOT passed through
/// [`crate::largeold::resolve_roots`], and that is the single most dangerous
/// mistake available in this module.** `resolve_roots` drops any root the
/// denylist protects, and `/System` is protected — so filtering this list would
/// silently return fewer roots, documented as "nothing to report, not an
/// error". That is correct for a size walk and catastrophic here: a shrunken
/// inventory makes still-installed apps look uninstalled, and every one of
/// their leftovers becomes offerable.
///
/// Reading these paths is safe for the same reason `allowlist::discovery_roots`
/// gives for including `/Applications`: this module yields plain `PathBuf`s and
/// cannot mint a `SafePath`, so nothing it returns can be acted on.
pub fn inventory_roots(home: &Path) -> Vec<PathBuf> {
    vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/Applications/Utilities"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Applications/Utilities"),
        home.join("Applications"),
    ]
}

/// The leftover locations, canonicalized, with protected ones dropped.
///
/// Public because the disposal half must confine selections against exactly
/// *this* output. It cannot reuse Large & Old's ceiling: most of these
/// locations sit outside `allowlist::discovery_roots`, so that check would
/// refuse every row this feature offers.
pub fn resolved_locations(cfg: &UninstallConfig) -> Vec<(Location, PathBuf)> {
    cfg.locations
        .iter()
        .filter_map(|&loc| {
            let raw = cfg.home.join(loc.subpath());
            let resolved = resolve_roots(std::slice::from_ref(&raw), &cfg.home);
            resolved.into_iter().next().map(|p| (loc, p))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Inventory
// ---------------------------------------------------------------------------

/// Enumerate installed application bundles.
///
/// A root that does not exist is simply absent — nothing is installed there. A
/// root that exists but cannot be read aborts the whole run: see
/// [`UninstallError::InventoryIncomplete`].
pub fn inventory(cfg: &UninstallConfig) -> Result<Vec<InstalledApp>, UninstallError> {
    let mut apps = Vec::new();
    for root in &cfg.app_roots {
        let root = match std::fs::canonicalize(root) {
            Ok(p) => p,
            // Absent is not incomplete. `~/Applications` does not exist on most
            // machines and never has.
            Err(_) => continue,
        };
        collect_bundles(&root, 0, cfg.app_scan_depth, &mut apps)?;
    }
    apps.sort_by(|a, b| a.bundle_path.cmp(&b.bundle_path));
    apps.dedup_by(|a, b| a.bundle_path == b.bundle_path);
    Ok(apps)
}

fn collect_bundles(
    dir: &Path,
    depth: usize,
    max_depth: usize,
    out: &mut Vec<InstalledApp>,
) -> Result<(), UninstallError> {
    if depth > max_depth {
        return Ok(());
    }
    let entries = std::fs::read_dir(dir).map_err(|e| UninstallError::InventoryIncomplete {
        root: dir.to_path_buf(),
        reason: e.to_string(),
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| UninstallError::InventoryIncomplete {
            root: dir.to_path_buf(),
            reason: e.to_string(),
        })?;
        let path = entry.path();
        // `DirEntry::file_type` does not follow symlinks. An `.app` reached
        // through a symlink is a second name for a bundle we will meet at its
        // real location, so following would double-count rather than find more.
        let Ok(ft) = entry.file_type() else { continue };
        if !ft.is_dir() {
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) == Some("app") {
            if let Some(app) = read_bundle(&path) {
                out.push(app);
            }
            // Keep descending: helpers and XPC services live inside.
        }
        collect_bundles(&path, depth + 1, max_depth, out)?;
    }
    Ok(())
}

/// Read a bundle's identity from `Contents/Info.plist`.
///
/// `None` when the plist is missing, unreadable, or carries no usable
/// identifier. A bundle without an id cannot own leftovers keyed on one, so it
/// contributes nothing to the owner index — but it is also not an error, since
/// plenty of legitimate bundles are shaped that way.
fn read_bundle(bundle: &Path) -> Option<InstalledApp> {
    let plist_path = bundle.join("Contents/Info.plist");
    // `from_file` handles both the XML and the binary encodings, which matters:
    // most shipped Info.plists are binary.
    let value = plist::Value::from_file(&plist_path).ok()?;
    let dict = value.as_dictionary()?;
    let id = BundleId::parse(dict.get("CFBundleIdentifier")?.as_string()?)?;
    let display_name = dict
        .get("CFBundleName")
        .or_else(|| dict.get("CFBundleDisplayName"))
        .and_then(|v| v.as_string())
        .map(str::to_string)
        .or_else(|| {
            bundle
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_string)
        });
    let mut names = BTreeSet::new();
    for key in ["CFBundleName", "CFBundleDisplayName"] {
        if let Some(n) = dict.get(key).and_then(|v| v.as_string()) {
            names.insert(n.to_string());
        }
    }
    if let Some(stem) = bundle.file_stem().and_then(|s| s.to_str()) {
        names.insert(stem.to_string());
    }
    Some(InstalledApp {
        bundle_path: bundle.to_path_buf(),
        id,
        display_name,
        names,
    })
}

/// Build the owner index from an inventory.
pub fn owner_index(apps: &[InstalledApp]) -> OwnerIndex {
    let mut names: BTreeMap<String, BTreeSet<BundleId>> = BTreeMap::new();
    for app in apps {
        for name in &app.names {
            names
                .entry(name.clone())
                .or_default()
                .insert(app.id.clone());
        }
    }
    OwnerIndex {
        ids: apps.iter().map(|a| a.id.clone()).collect(),
        names,
    }
}

// ---------------------------------------------------------------------------
// The matching rule
// ---------------------------------------------------------------------------

/// True when `id`'s dot-separated segments are a byte-exact prefix of `stem`'s.
///
/// Never a string prefix. `segment_prefix("com.acme.Notes", "com.acme.Note")`
/// is false, because the third segments differ — which is the same shape
/// `allowlist::is_allowed` already gets right by comparing path *components*
/// rather than bytes.
fn segment_prefix(stem: &str, id: &BundleId) -> bool {
    let mut have = stem.split('.');
    for want in id.segments() {
        match have.next() {
            Some(got) if got == want => {}
            _ => return false,
        }
    }
    true
}

/// 8-4-4-4-12 hex, hand-rolled.
///
/// Deliberately not a regex: a regex here is one refactor away from being built
/// out of a bundle id, and `.` is a wildcard.
fn is_hardware_uuid(s: &str) -> bool {
    const GROUPS: [usize; 5] = [8, 4, 4, 4, 12];
    let mut parts = s.split('-');
    for want in GROUPS {
        match parts.next() {
            Some(g) if g.len() == want && g.bytes().all(|b| b.is_ascii_hexdigit()) => {}
            _ => return false,
        }
    }
    parts.next().is_none()
}

/// Strip the one suffix this location declares, exactly once, from the end.
///
/// Returns the stem and the suffix that was removed. `None` means the entry is
/// the wrong shape for its location and is not a candidate at all — a
/// `some-file.bak` sitting in `Preferences` never becomes `some-file`.
fn stem(loc: Location, name: &str, is_dir: bool) -> Option<(&str, Option<&'static str>)> {
    match (loc, is_dir) {
        (Location::Preferences, false) => name.strip_suffix(".plist").map(|s| (s, Some(".plist"))),
        (Location::Preferences, true) => Some((name, None)),

        // The only double strip in the module, and it is fenced twice: it
        // exists in this location alone, and the second segment comes off only
        // when it is literally a hardware UUID. So a helper's plist filed here
        // stems to the helper's id, never to its parent's.
        (Location::PreferencesByHost, false) => {
            let base = name.strip_suffix(".plist")?;
            match base.rsplit_once('.') {
                Some((head, tail)) if is_hardware_uuid(tail) => Some((head, Some(".<uuid>.plist"))),
                _ => Some((base, Some(".plist"))),
            }
        }

        (Location::SavedApplicationState, true) => name
            .strip_suffix(".savedState")
            .map(|s| (s, Some(".savedState"))),

        (Location::HttpStorages, false) => name
            .strip_suffix(".binarycookies")
            .map(|s| (s, Some(".binarycookies"))),
        (Location::HttpStorages, true) => Some((name, None)),

        (Location::LaunchAgents, false) => name.strip_suffix(".plist").map(|s| (s, Some(".plist"))),

        (
            Location::Caches
            | Location::Containers
            | Location::WebKit
            | Location::Logs
            | Location::ApplicationSupport,
            true,
        ) => Some((name, None)),

        // Group containers never reach the matcher; see `group_container_row`.
        _ => None,
    }
}

/// What a stem is, relative to the target and to everything installed.
#[derive(Debug, PartialEq, Eq)]
enum Claim {
    /// Not this app's, on any reading.
    NotOurs,
    /// Exactly the target.
    Exact,
    /// `<target>.<more>`, with nothing installed owning the longer id.
    OrphanSibling(String),
    /// `<target>.<more>`, but a still-installed app owns the longer id.
    StillInstalled(BundleId),
}

fn claim(stem: &str, target: &BundleId, index: &OwnerIndex) -> Claim {
    if !segment_prefix(stem, target) {
        return Claim::NotOurs;
    }
    if stem == target.as_str() {
        return Claim::Exact;
    }
    match index.longest_owner(stem) {
        // Somebody installed owns this longer id — including, possibly, the
        // target itself if it is still around, which `leftovers_for` has
        // already refused before reaching here.
        Some(owner) if owner != target => Claim::StillInstalled(owner.clone()),
        _ => Claim::OrphanSibling(
            stem.strip_prefix(target.as_str())
                .and_then(|t| t.strip_prefix('.'))
                .unwrap_or(stem)
                .to_string(),
        ),
    }
}

// ---------------------------------------------------------------------------
// The search
// ---------------------------------------------------------------------------

/// Find `target`'s leftovers, id-keyed only.
///
/// Read-only from top to bottom: it opens no leftover file's contents, follows
/// no symlink, and mutates nothing. See [`leftovers_for_named`] for the
/// human-name tier.
pub fn leftovers_for(
    cfg: &UninstallConfig,
    target: &BundleId,
) -> Result<LeftoverReport, UninstallError> {
    leftovers_for_named(cfg, target, None)
}

/// Find `target`'s leftovers, with the human-name tier enabled when a
/// `display_name` is supplied.
///
/// The name has to come from the caller because the bundle that declared it
/// is, by premise, gone. Nothing is matched by name outside
/// `~/Library/Application Support`, and nothing matched by name is ever
/// bulk-grantable.
pub fn leftovers_for_named(
    cfg: &UninstallConfig,
    target: &BundleId,
    display_name: Option<&DisplayName>,
) -> Result<LeftoverReport, UninstallError> {
    let apps = inventory(cfg)?;
    let index = owner_index(&apps);

    let mut report = LeftoverReport {
        target: target.clone(),
        residence: Residence::NotFound {
            searched: cfg.app_roots.clone(),
        },
        rows: Vec::new(),
        examined: 0,
        truncated: false,
        skipped_unreadable: 0,
        skipped_symlink: 0,
        skipped_case_variant: 0,
        skipped_unrepresentable: 0,
        withheld_count: 0,
        skipped_uncorroborated_name: 0,
        deferred: DEFERRED_LOCATIONS,
        caveats: Vec::new(),
    };

    // An installed app has no leftovers; it has files. Returning rows here
    // would be offering a running application's own data.
    if index.contains(target) {
        report.residence = Residence::Installed(
            apps.iter()
                .filter(|a| &a.id == target)
                .map(|a| a.bundle_path.clone())
                .collect(),
        );
        return Ok(report);
    }

    for (location, root) in resolved_locations(cfg) {
        let entries = match std::fs::read_dir(&root) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => {
                report.skipped_unreadable += 1;
                continue;
            }
        };

        for entry in entries {
            if report.examined >= cfg.max_examined {
                report.truncated = true;
                break;
            }
            report.examined += 1;

            let Ok(entry) = entry else {
                report.skipped_unreadable += 1;
                continue;
            };
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                report.skipped_unreadable += 1;
                continue;
            };

            // The name has to survive a round trip to the UI and back byte for
            // byte, because a later grant identifies a selection by string
            // equality with what was emitted here. Owned, so the borrow of
            // `path` ends here: everything downstream takes it by value.
            let Some(name) = path
                .file_name()
                .and_then(|n| n.to_str())
                .map(str::to_string)
            else {
                report.skipped_unrepresentable += 1;
                continue;
            };
            let is_dir = file_type.is_dir();

            // A leftover that is a symlink points at data somewhere else —
            // frequently, on a real machine, deep inside another app's
            // container. Canonicalizing it is what would make the row *look*
            // legitimate, so it is dropped instead of resolved.
            //
            // Counted only when it is *this app's*, on either shape it might
            // have had. An unrelated symlink elsewhere in a location — a stock
            // Saved Application State holds four — is not a gap in this
            // report, and before this was made target-specific every real
            // scan came back `partial` because of them.
            if file_type.is_symlink() {
                let ours = [true, false].into_iter().any(|as_dir| {
                    stem(location, &name, as_dir)
                        .is_some_and(|(s, _)| claim(s, target, &index) != Claim::NotOurs)
                });
                if ours {
                    report.skipped_symlink += 1;
                }
                continue;
            }

            // Group containers never reach the matcher: nothing in them is
            // ever claimed, so they take a path that can only show.
            if location == Location::GroupContainers {
                group_container_row(path, &name, is_dir, target, cfg, &mut report);
                continue;
            }

            let Some((stem_str, suffix)) =
                stem(location, &name, is_dir).map(|(s, suf)| (s.to_string(), suf))
            else {
                continue;
            };

            match claim(&stem_str, target, &index) {
                Claim::NotOurs => {
                    // Report the near miss rather than dropping it in silence:
                    // an id spelled differently from its own directory is a
                    // real under-match and the user should know it happened.
                    if stem_str.eq_ignore_ascii_case(target.as_str()) {
                        report.skipped_case_variant += 1;
                    } else if location == Location::ApplicationSupport && is_dir {
                        // The weak tier, in the one location it exists.
                        if let Some(display_name) = display_name {
                            name_tier_row(
                                path,
                                &name,
                                display_name,
                                target,
                                &index,
                                cfg,
                                &mut report,
                            );
                        }
                    }
                }
                Claim::StillInstalled(owner) => {
                    // For a container this is a live app's whole redirected
                    // home: one withheld row, never decomposed.
                    report.withheld_count += 1;
                    let m = measure(&path, cfg, &mut report.examined);
                    report.rows.push(Candidate {
                        path,
                        location,
                        matched_via: MatchedVia::SiblingSegment(stem_str.clone()),
                        kind: Kind::Leftover,
                        size_bytes: m.size_bytes,
                        file_count: m.file_count,
                        size_is_floor: m.size_is_floor,
                        offerable: false,
                        bulk_grantable: false,
                        withheld: Some(format!("{owner} is still installed and this is its data")),
                        undisposable: m.undisposable,
                        license_suspected: m.license_suspected,
                    });
                }
                found if location == Location::Containers => {
                    container_rows(path, &found, cfg, &mut report);
                }
                found => {
                    let Some(row) = build_row(path, location, suffix, &found, cfg, &mut report)
                    else {
                        continue;
                    };
                    if !row.offerable {
                        report.withheld_count += 1;
                    }
                    report.rows.push(row);
                }
            }
        }
    }

    report.rows.sort_by(|a, b| {
        a.location
            .cmp(&b.location)
            .then_with(|| a.path.cmp(&b.path))
    });

    // Surfaced whenever any row is a preferences file or a container's
    // preferences part, so the UI can say it before the user acts.
    let touches_preferences = report.rows.iter().any(|r| {
        matches!(
            r.location,
            Location::Preferences | Location::PreferencesByHost
        ) || (r.location == Location::Containers && r.path.ends_with("Library/Preferences"))
    });
    if touches_preferences {
        report.caveats.push(CFPREFSD_CAVEAT);
    }
    Ok(report)
}

fn build_row(
    path: PathBuf,
    location: Location,
    suffix: Option<&'static str>,
    found: &Claim,
    cfg: &UninstallConfig,
    report: &mut LeftoverReport,
) -> Option<Candidate> {
    // The emitted path must already be its own canonical spelling — the same
    // identity rule the Large & Old disposal path enforces, applied when the row
    // is created rather than when it is granted, so a row that could never be
    // acted on is never offered in the first place.
    //
    // Not covered by a test, and verified so: removing this leaves the whole
    // suite green. The reason is structural rather than an oversight. The
    // location root is canonical, the name comes from `read_dir`, and a symlink
    // entry was already dropped above — so a path reaching here cannot fail the
    // check except through a race, where something replaces the entry with a
    // symlink between `file_type()` and this line. That race is not
    // reproducible in a fixture. This is defence in depth against a TOCTOU, not
    // a pinned invariant, and is recorded as one rather than left looking like
    // coverage it does not have.
    if std::fs::canonicalize(&path).ok().as_deref() != Some(path.as_path()) {
        report.skipped_symlink += 1;
        return None;
    }

    let m = measure(&path, cfg, &mut report.examined);

    let (matched_via, bulk_grantable) = match found {
        Claim::Exact => (
            match suffix {
                Some(s) => MatchedVia::IdWithSuffix(s),
                None => MatchedVia::Id,
            },
            true,
        ),
        // Legitimate, but it is a *different* identifier from the one the user
        // named. One gesture should not sweep up a directory they never saw
        // spelled out.
        Claim::OrphanSibling(tail) => (MatchedVia::SiblingSegment(tail.clone()), false),
        _ => return None,
    };

    // A tree `guard_dir` is certain to refuse is shown, not offered. And a
    // launch agent whose program is still on disk is not leftover: something
    // installed still runs from it.
    let (mut offerable, mut withheld) = offer(&m);
    if offerable && location == Location::LaunchAgents {
        if let Some(program) = launch_agent_program(&path) {
            if program.exists() {
                offerable = false;
                withheld = Some(format!(
                    "this job still launches {}, which is on disk",
                    program.display()
                ));
            }
        }
    }

    Some(Candidate {
        path,
        location,
        matched_via,
        kind: Kind::Leftover,
        size_bytes: m.size_bytes,
        file_count: m.file_count,
        size_is_floor: m.size_is_floor,
        offerable,
        bulk_grantable: bulk_grantable && offerable && !m.license_suspected,
        withheld,
        undisposable: m.undisposable,
        license_suspected: m.license_suspected,
    })
}

/// Decompose a matched container into rows. The container root is never one.
///
/// Every emitted path is `<container>/Data/<part>` with `part` from one of the
/// two inclusion lists — constants joined onto a `read_dir`-provenance path.
/// No parsed or caller-supplied string is ever joined here, which is what
/// keeps "a row is never a location root" structural rather than checked.
fn container_rows(
    container: PathBuf,
    found: &Claim,
    cfg: &UninstallConfig,
    report: &mut LeftoverReport,
) {
    let (matched_via, bulk_grantable) = match found {
        Claim::Exact => (MatchedVia::Id, true),
        Claim::OrphanSibling(tail) => (MatchedVia::SiblingSegment(tail.clone()), false),
        _ => return,
    };
    let data = container.join("Data");
    let parts = CONTAINER_STATE_PARTS
        .iter()
        .map(|p| (*p, Kind::Leftover))
        .chain(
            CONTAINER_USER_DATA_PARTS
                .iter()
                .map(|p| (*p, Kind::UserData)),
        );
    for (part, kind) in parts {
        report.examined += 1;
        let path = data.join(part);
        // Absent is ordinary: the scaffold varies by macOS version.
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            report.skipped_symlink += 1;
            continue;
        }
        if !meta.is_dir() {
            continue;
        }
        // Covers `Data` and `Library` too: a symlink anywhere on the way —
        // and 82 of 822 real containers have them, pointing back into the
        // real home — makes the canonical spelling differ.
        if std::fs::canonicalize(&path).ok().as_deref() != Some(path.as_path()) {
            report.skipped_symlink += 1;
            continue;
        }
        // Empty scaffolding — most of every container — is not a row.
        match is_empty_dir(&path) {
            Ok(true) => continue,
            Ok(false) => {}
            Err(_) => {
                report.skipped_unreadable += 1;
                continue;
            }
        }
        let m = measure(&path, cfg, &mut report.examined);
        let (offerable, withheld) = match kind {
            Kind::Leftover => offer(&m),
            _ => (false, Some(CONTAINER_USER_DATA_REASON.to_string())),
        };
        if !offerable {
            report.withheld_count += 1;
        }
        report.rows.push(Candidate {
            path,
            location: Location::Containers,
            matched_via: matched_via.clone(),
            kind,
            size_bytes: m.size_bytes,
            file_count: m.file_count,
            size_is_floor: m.size_is_floor,
            offerable,
            bulk_grantable: bulk_grantable && offerable && !m.license_suspected,
            withheld,
            undisposable: m.undisposable,
            license_suspected: m.license_suspected,
        });
    }
}

fn is_empty_dir(path: &Path) -> std::io::Result<bool> {
    let mut entries = std::fs::read_dir(path)?;
    match entries.next() {
        None => Ok(true),
        Some(Ok(_)) => Ok(false),
        Some(Err(e)) => Err(e),
    }
}

/// A group container whose name resembles the id: shown, never claimed.
fn group_container_row(
    path: PathBuf,
    name: &str,
    is_dir: bool,
    target: &BundleId,
    cfg: &UninstallConfig,
    report: &mut LeftoverReport,
) {
    if !is_dir {
        return;
    }
    let Some((rest, prefix)) = strip_group_prefix(name) else {
        return;
    };
    if !segment_prefix(rest, target) {
        return;
    }
    let m = measure(&path, cfg, &mut report.examined);
    report.withheld_count += 1;
    report.rows.push(Candidate {
        path,
        location: Location::GroupContainers,
        matched_via: MatchedVia::IdWithPrefix(prefix),
        kind: Kind::Shared,
        size_bytes: m.size_bytes,
        file_count: m.file_count,
        size_is_floor: m.size_is_floor,
        offerable: false,
        bulk_grantable: false,
        withheld: Some(GROUP_CONTAINER_REASON.to_string()),
        undisposable: m.undisposable,
        license_suspected: m.license_suspected,
    });
}

/// `group.<rest>` or `<TEAMID>.<rest>`, with the prefix that came off.
///
/// The one place a *prefix* comes off a name, and it is allowed only because
/// the result can never be a claim: a group container is withheld whatever
/// this returns, so the worst a wrong strip can do is show a withheld row.
fn strip_group_prefix(name: &str) -> Option<(&str, String)> {
    if let Some(rest) = name.strip_prefix("group.") {
        return Some((rest, "group.".to_string()));
    }
    let (head, rest) = name.split_once('.')?;
    is_team_id(head).then(|| (rest, format!("{head}.")))
}

/// Ten characters of `[A-Z0-9]`: the shape of an Apple team identifier.
fn is_team_id(s: &str) -> bool {
    s.len() == 10
        && s.bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

/// The human-name tier. `Application Support` only, and reached only for a
/// directory the id matcher has already said is not ours.
///
/// Three gates, each of which can only narrow:
/// 1. the directory name is byte-equal to the name the caller supplied;
/// 2. no installed app answers to that name — else withheld, and shown;
/// 3. an immediate child is keyed on the target's id — else counted, unshown.
fn name_tier_row(
    path: PathBuf,
    dir_name: &str,
    display_name: &DisplayName,
    target: &BundleId,
    index: &OwnerIndex,
    cfg: &UninstallConfig,
    report: &mut LeftoverReport,
) {
    if dir_name != display_name.as_str() {
        return;
    }

    // Gate 2. An installed app that answers to the name — by `CFBundleName`,
    // `CFBundleDisplayName` or its `.app` stem — or that *is* the name, if the
    // directory happens to be spelled as somebody's id.
    let mut owners: Vec<String> = index
        .owners_of_name(dir_name)
        .map(|s| s.iter().map(ToString::to_string).collect())
        .unwrap_or_default();
    if let Some(as_id) = BundleId::parse(dir_name).filter(|i| index.contains(i)) {
        owners.push(as_id.to_string());
    }
    if !owners.is_empty() {
        report.withheld_count += 1;
        let m = measure(&path, cfg, &mut report.examined);
        report.rows.push(Candidate {
            path,
            location: Location::ApplicationSupport,
            matched_via: MatchedVia::DisplayName(dir_name.to_string()),
            kind: Kind::Leftover,
            size_bytes: m.size_bytes,
            file_count: m.file_count,
            size_is_floor: m.size_is_floor,
            offerable: false,
            bulk_grantable: false,
            withheld: Some(format!(
                "{} is still installed and answers to this name",
                owners.join(", ")
            )),
            undisposable: m.undisposable,
            license_suspected: m.license_suspected,
        });
        return;
    }

    // Gate 3.
    if !corroborated(&path, target, index) {
        report.skipped_uncorroborated_name += 1;
        return;
    }

    // The same spelling rule as every other offerable row — see `build_row`.
    if std::fs::canonicalize(&path).ok().as_deref() != Some(path.as_path()) {
        report.skipped_symlink += 1;
        return;
    }
    let m = measure(&path, cfg, &mut report.examined);
    let (offerable, withheld) = offer(&m);
    if !offerable {
        report.withheld_count += 1;
    }
    report.rows.push(Candidate {
        path,
        location: Location::ApplicationSupport,
        matched_via: MatchedVia::DisplayName(dir_name.to_string()),
        kind: Kind::Leftover,
        size_bytes: m.size_bytes,
        file_count: m.file_count,
        size_is_floor: m.size_is_floor,
        offerable,
        bulk_grantable: false,
        withheld,
        undisposable: m.undisposable,
        license_suspected: m.license_suspected,
    });
}

/// At least one immediate child is keyed on the target's id and on nobody
/// installed. The same [`claim`] as everywhere else, on the raw child name —
/// so `com.acme.Notes.plist` corroborates, `com.acme.Notes2` does not, and a
/// child owned by an installed helper says the directory is *someone's*
/// without saying it is the target's.
fn corroborated(dir: &Path, target: &BundleId, index: &OwnerIndex) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|e| {
        e.file_name().to_str().is_some_and(|n| {
            matches!(
                claim(n, target, index),
                Claim::Exact | Claim::OrphanSibling(_)
            )
        })
    })
}

/// The program a launch agent runs, from a fixed key set.
///
/// `Label` is deliberately not the match key — it is absent from real agents in
/// the wild, and the filename stem is what the matcher uses. This reads only
/// `Program` and `ProgramArguments[0]`, and only to decide whether something is
/// still installed behind the job.
fn launch_agent_program(path: &Path) -> Option<PathBuf> {
    let value = plist::Value::from_file(path).ok()?;
    let dict = value.as_dictionary()?;
    if let Some(p) = dict.get("Program").and_then(|v| v.as_string()) {
        return Some(PathBuf::from(p));
    }
    dict.get("ProgramArguments")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.as_string())
        .map(PathBuf::from)
}

/// Apparent size and name count beneath `path`.
///
/// What a per-row size walk found, plus the one judgement that is this
/// module's own.
///
/// The walk and the `undisposable` predicate live in [`crate::treewalk`]
/// because M5 needs the identical decision, and two copies of "will `guard_dir`
/// refuse this?" that drifted would offer a checkbox the executor is certain to
/// refuse. `license_suspected` stays here: it is an Uninstaller concern, and
/// Privacy has no notion of a licence file.
struct Measured {
    size_bytes: u64,
    file_count: u64,
    size_is_floor: bool,
    undisposable: Option<&'static str>,
    license_suspected: bool,
}

fn measure(path: &Path, cfg: &UninstallConfig, examined: &mut usize) -> Measured {
    let m = treewalk::measure(path, &cfg.bounds(), examined);
    Measured {
        size_bytes: m.size_bytes,
        file_count: m.file_count,
        size_is_floor: m.size_is_floor,
        undisposable: m.undisposable,
        license_suspected: license_shaped(path),
    }
}

/// Whether a measured row may be offered at all. A tree `guard_dir` is
/// certain to refuse is shown and withheld, never offered.
fn offer(m: &Measured) -> (bool, Option<String>) {
    treewalk::offer(&treewalk::Measured {
        size_bytes: m.size_bytes,
        file_count: m.file_count,
        size_is_floor: m.size_is_floor,
        undisposable: m.undisposable,
    })
}

/// A licence, activation or receipt shape among `dir`'s immediate children,
/// by name only. Nothing is opened: a key read for classification would land
/// in a UI row and then in an append-only log that is never rotated. Folding
/// case here can only keep *more* rows out of a bulk gesture, so it is safe.
fn license_shaped(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|e| {
        let name = e.file_name();
        let Some(name) = name.to_str() else {
            return false;
        };
        let lower = name.to_ascii_lowercase();
        lower == "receipts"
            || lower.ends_with(".lic")
            || lower.ends_with(".license")
            || lower.ends_with(".activation")
            || (lower.starts_with("license") && lower.ends_with(".plist"))
    })
}

/// Convenience for callers that only have a `&Path` and a raw id string.
///
/// Canonicalizes `home` first, for the reason spelled out on
/// [`crate::largeold::find_in`]: the denylist compares against it
/// component-wise, so a non-canonical home silently disables the
/// keychains/mail/home-root rules for the whole run.
pub fn leftovers_in(home: &Path, raw_id: &str) -> Result<LeftoverReport, UninstallError> {
    let id =
        BundleId::parse(raw_id).ok_or_else(|| UninstallError::UnmatchableId(raw_id.to_string()))?;
    let home = safety::canonical_home(home).unwrap_or_else(|_| home.to_path_buf());
    leftovers_for(&UninstallConfig::new(home), &id)
}
