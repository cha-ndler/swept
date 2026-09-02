//! Browser privacy data — a read-only search of what browsers remember.
//!
//! # The thing to be frightened of
//!
//! The trust kernel protects almost nothing here. A Firefox profile keeps
//! `cookies.sqlite` and `key4.db` — the key that decrypts every saved password
//! — in the *same flat directory*. Chromium keeps `Cookies` and `Login Data`
//! as byte-adjacent siblings with the same extension-less name shape. Both are
//! ordinary files in an ordinary directory, so both sail through
//! [`safety::guard`]: the denylist has no opinion about them, and neither does
//! the allowlist, because none of this is in the disposal scope to begin with.
//!
//! So the safety argument rests on three things the kernel does not supply:
//!
//! 1. **An inclusion list, consulted by lookup rather than by listing.** This
//!    module never enumerates a profile directory and filters it. It joins a
//!    *constant* name onto a corroborated root and asks whether that exists. A
//!    file this module does not already know by name is therefore not merely
//!    rejected — it is never seen. An exclusion list would fail open the next
//!    time a vendor adds a file, which is the same reasoning that made M4's
//!    `CONTAINER_STATE_PARTS` an inclusion list.
//! 2. **The fresh-scan ceiling at disposal time**, in the command layer: a path
//!    is disposable only if it is byte-equal to a member of an `offerable` row
//!    of a scan run inside that call.
//! 3. **Profile-root confinement**, which is stronger than confining to a
//!    location root: one profile's row can never authorize a path in the next.
//!
//! # No parsed string is ever joined onto a path
//!
//! Chromium records its profiles in `Local State` (JSON) and Firefox in
//! `profiles.ini`. Both would be more *authoritative* than a directory
//! listing, and reading either would put file **content** in charge of naming a
//! directory — content that can say `../../Keychains`. `uninstall.rs` states as
//! one of its own invariants that no parsed or caller-supplied string is ever
//! joined onto a path, and this module keeps that invariant rather than
//! validating its way out of it.
//!
//! Instead, profiles are found with `read_dir` and then **corroborated**: a
//! Chromium profile is a directory holding a `Preferences` file, a Firefox
//! profile one holding `prefs.js` or `times.json`. `read_dir` can never yield
//! `.`, `..`, or a name containing a separator, so the injection surface does
//! not exist rather than being defended against. The cost is that a Firefox
//! profile living outside the Firefox root (an absolute `Path=` in
//! `profiles.ini`) is not found. Under-reporting is the safe direction.
//!
//! # What this module never touches
//!
//! Saved passwords and the keys that decrypt them, autofill and card data,
//! bookmarks, extensions and their data, certificates, per-site permissions,
//! sync state, and browser preferences. Firefox *history* is on that list too,
//! and not out of caution: `places.sqlite` holds history **and bookmarks** in
//! one file, so removing it destroys the bookmarks, and separating them would
//! mean editing rows inside a database — a destructive capability this tool
//! does not have and should not grow by accident.
//!
//! # It cannot authorize anything
//!
//! Like [`crate::largeold`] and [`crate::uninstall`], this yields plain
//! [`PathBuf`]s and never constructs a [`safety::SafePath`]. Nothing here is
//! pre-selected and nothing here is part of a default clean.

use std::path::{Path, PathBuf};

use safety::DirLimits;

use crate::treewalk::{self, Bounds};

/// Entry budget for a whole run, shared across every browser and every row.
pub const DEFAULT_MAX_EXAMINED: usize = 200_000;

/// Surfaced whenever a browser looks like it may be running.
pub const RUNNING_BROWSER_CAVEAT: &str = "a browser looks like it is running: its caches will be \
     rebuilt as soon as it is used again, and anything it is holding open is shown but not \
     offered";

/// Safari has no lock file to read, so its liveness cannot be established the
/// way Chromium's and Firefox's can.
pub const SAFARI_QUIT_CAVEAT: &str = "Safari leaves no marker saying whether it is running, so \
     this cannot tell; quit Safari before acting on its data, or it may write the file back";

const SITE_STORAGE_REASON: &str = "this is website storage — where a site or a local-first web \
     app keeps data, sometimes the only copy of the user's work — so it is shown, not offered";

const SAFARI_CONTAINER_REASON: &str = "this is inside Safari's own sandbox container; no module \
     offers a path inside another app's container yet, so it is shown, not offered";

const FIREFOX_HISTORY_NOTE: &str = "Firefox history is not offered: places.sqlite holds the \
     history and the bookmarks in one file, so removing it would take the bookmarks too";

// ---------------------------------------------------------------------------
// The browser table
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Safari,
    Chromium,
    Firefox,
}

pub struct BrowserSpec {
    /// Stable machine id.
    pub id: &'static str,
    pub name: &'static str,
    pub family: Family,
    /// Home-relative root. Chromium: the directory holding the profiles.
    /// Firefox: the directory holding `Profiles`. Safari: its primary data
    /// root, which is also what the access probe opens.
    pub root: &'static str,
    /// Home-relative cache root, when the ordinary `user-caches` cleaner
    /// already covers it. Named in the report, deliberately without a size.
    pub cache_root: Option<&'static str>,
}

/// Every browser this module knows. Ten of these are one table row each — the
/// Chromium family shares its profile layout exactly, so a table is honest
/// where per-browser code would be repetition. Safari and Firefox get their own
/// recognizers, because a table pretending they were Chromium is the fail-open.
pub static BROWSERS: &[BrowserSpec] = &[
    BrowserSpec {
        id: "safari",
        name: "Safari",
        family: Family::Safari,
        root: "Library/Safari",
        cache_root: Some("Library/Caches/com.apple.Safari"),
    },
    BrowserSpec {
        id: "google-chrome",
        name: "Google Chrome",
        family: Family::Chromium,
        root: "Library/Application Support/Google/Chrome",
        cache_root: Some("Library/Caches/Google/Chrome"),
    },
    BrowserSpec {
        id: "google-chrome-beta",
        name: "Google Chrome Beta",
        family: Family::Chromium,
        root: "Library/Application Support/Google/Chrome Beta",
        cache_root: Some("Library/Caches/Google/Chrome Beta"),
    },
    BrowserSpec {
        id: "google-chrome-canary",
        name: "Google Chrome Canary",
        family: Family::Chromium,
        root: "Library/Application Support/Google/Chrome Canary",
        cache_root: Some("Library/Caches/Google/Chrome Canary"),
    },
    BrowserSpec {
        id: "chromium",
        name: "Chromium",
        family: Family::Chromium,
        root: "Library/Application Support/Chromium",
        cache_root: Some("Library/Caches/Chromium"),
    },
    BrowserSpec {
        id: "microsoft-edge",
        name: "Microsoft Edge",
        family: Family::Chromium,
        root: "Library/Application Support/Microsoft Edge",
        cache_root: Some("Library/Caches/Microsoft Edge"),
    },
    BrowserSpec {
        id: "brave",
        name: "Brave",
        family: Family::Chromium,
        root: "Library/Application Support/BraveSoftware/Brave-Browser",
        cache_root: Some("Library/Caches/BraveSoftware/Brave-Browser"),
    },
    BrowserSpec {
        id: "brave-beta",
        name: "Brave Beta",
        family: Family::Chromium,
        root: "Library/Application Support/BraveSoftware/Brave-Browser-Beta",
        cache_root: Some("Library/Caches/BraveSoftware/Brave-Browser-Beta"),
    },
    BrowserSpec {
        id: "brave-nightly",
        name: "Brave Nightly",
        family: Family::Chromium,
        root: "Library/Application Support/BraveSoftware/Brave-Browser-Nightly",
        cache_root: Some("Library/Caches/BraveSoftware/Brave-Browser-Nightly"),
    },
    BrowserSpec {
        id: "vivaldi",
        name: "Vivaldi",
        family: Family::Chromium,
        root: "Library/Application Support/Vivaldi",
        cache_root: Some("Library/Caches/Vivaldi"),
    },
    BrowserSpec {
        id: "arc",
        name: "Arc",
        family: Family::Chromium,
        // Arc nests its profiles one level deeper. That belongs in the table,
        // not in a branch.
        root: "Library/Application Support/Arc/User Data",
        cache_root: Some("Library/Caches/company.thebrowser.Browser"),
    },
    BrowserSpec {
        id: "firefox",
        name: "Firefox",
        family: Family::Firefox,
        root: "Library/Application Support/Firefox",
        cache_root: Some("Library/Caches/Firefox"),
    },
];

pub struct Unsupported {
    pub name: &'static str,
    pub reason: &'static str,
}

/// Named rather than silently absent, the way `uninstall::DEFERRED_LOCATIONS`
/// names the locations it does not search.
pub static UNSUPPORTED: &[Unsupported] = &[
    Unsupported {
        name: "Opera / Opera GX",
        reason: "Chromium-based but stores profiles under a differently shaped root; unmeasured, \
                 so guessing the layout could name the wrong directory",
    },
    Unsupported {
        name: "Orion",
        reason: "WebKit-based with its own storage layout; unmeasured, and its data shape has no \
                 second implementation to check a guess against",
    },
    Unsupported {
        name: "Tor Browser",
        reason: "deliberately out of scope: it is built to leave nothing behind, and a cleaner \
                 poking at its profile is more likely to break it than to help",
    },
];

// ---------------------------------------------------------------------------
// The inclusion lists
// ---------------------------------------------------------------------------

/// What a row is, which decides both its consequence and whether it may ever
/// be offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Cookies,
    History,
    Session,
    /// Shown, never offered.
    SiteStorage,
    ProfileCache,
}

/// What the user loses. Derived from [`Class`] and never set independently, so
/// there is no code path that can mislabel a cookie jar as regenerable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Consequence {
    SignsYouOut,
    ErasesHistory,
    LosesOpenTabs,
    LosesSiteData,
    Regenerable,
}

impl Class {
    pub fn consequence(self) -> Consequence {
        match self {
            Class::Cookies => Consequence::SignsYouOut,
            Class::History => Consequence::ErasesHistory,
            Class::Session => Consequence::LosesOpenTabs,
            Class::SiteStorage => Consequence::LosesSiteData,
            Class::ProfileCache => Consequence::Regenerable,
        }
    }
}

struct Entry {
    /// A **constant** relative name. Some contain a separator (`Network/
    /// Cookies`); that is safe precisely because it is a literal in this file
    /// and never a value read from disk.
    name: &'static str,
    class: Class,
    is_dir: bool,
    label: &'static str,
}

static CHROMIUM_ENTRIES: &[Entry] = &[
    // Chromium has kept its cookie jar in two places over the years, and which
    // one is live varies by version *and* by profile. Measured: a current
    // Chrome on the reference machine uses the profile-root `Cookies` in all
    // three of its profiles and has no `Network/Cookies` at all. So neither is
    // labelled "old" — both are searched, both are just "Cookies", and the path
    // says which one the user is looking at. Calling one legacy would be a
    // guess presented as a fact, and here it would be the wrong way round.
    Entry {
        name: "Network/Cookies",
        class: Class::Cookies,
        is_dir: false,
        label: "Cookies",
    },
    Entry {
        name: "Cookies",
        class: Class::Cookies,
        is_dir: false,
        label: "Cookies",
    },
    Entry {
        name: "History",
        class: Class::History,
        is_dir: false,
        label: "Browsing history",
    },
    Entry {
        name: "Top Sites",
        class: Class::History,
        is_dir: false,
        label: "Most-visited sites",
    },
    Entry {
        name: "Shortcuts",
        class: Class::History,
        is_dir: false,
        label: "Address-bar shortcuts",
    },
    Entry {
        name: "Visited Links",
        class: Class::History,
        is_dir: false,
        label: "Visited links",
    },
    Entry {
        name: "Network Action Predictor",
        class: Class::History,
        is_dir: false,
        label: "Typing predictions",
    },
    Entry {
        name: "Sessions",
        class: Class::Session,
        is_dir: true,
        label: "Saved sessions",
    },
    Entry {
        name: "Session Storage",
        class: Class::Session,
        is_dir: true,
        label: "Session storage",
    },
    Entry {
        name: "Current Session",
        class: Class::Session,
        is_dir: false,
        label: "Current session",
    },
    Entry {
        name: "Current Tabs",
        class: Class::Session,
        is_dir: false,
        label: "Current tabs",
    },
    Entry {
        name: "Last Session",
        class: Class::Session,
        is_dir: false,
        label: "Previous session",
    },
    Entry {
        name: "Last Tabs",
        class: Class::Session,
        is_dir: false,
        label: "Previous tabs",
    },
    Entry {
        name: "Local Storage",
        class: Class::SiteStorage,
        is_dir: true,
        label: "Local storage",
    },
    Entry {
        name: "IndexedDB",
        class: Class::SiteStorage,
        is_dir: true,
        label: "IndexedDB",
    },
    Entry {
        name: "Service Worker",
        class: Class::SiteStorage,
        is_dir: true,
        label: "Service workers",
    },
    Entry {
        name: "File System",
        class: Class::SiteStorage,
        is_dir: true,
        label: "Site file system",
    },
    Entry {
        name: "blob_storage",
        class: Class::SiteStorage,
        is_dir: true,
        label: "Blob storage",
    },
    Entry {
        name: "databases",
        class: Class::SiteStorage,
        is_dir: true,
        label: "Web SQL databases",
    },
    Entry {
        name: "GPUCache",
        class: Class::ProfileCache,
        is_dir: true,
        label: "GPU cache",
    },
    Entry {
        name: "Code Cache",
        class: Class::ProfileCache,
        is_dir: true,
        label: "Code cache",
    },
    Entry {
        name: "DawnCache",
        class: Class::ProfileCache,
        is_dir: true,
        label: "Graphics cache",
    },
    Entry {
        name: "GraphiteDawnCache",
        class: Class::ProfileCache,
        is_dir: true,
        label: "Graphics cache",
    },
    Entry {
        name: "ShaderCache",
        class: Class::ProfileCache,
        is_dir: true,
        label: "Shader cache",
    },
    Entry {
        name: "Application Cache",
        class: Class::ProfileCache,
        is_dir: true,
        label: "Application cache",
    },
];

static FIREFOX_ENTRIES: &[Entry] = &[
    Entry {
        name: "cookies.sqlite",
        class: Class::Cookies,
        is_dir: false,
        label: "Cookies",
    },
    Entry {
        name: "sessionstore.jsonlz4",
        class: Class::Session,
        is_dir: false,
        label: "Saved session",
    },
    Entry {
        name: "sessionstore-backups",
        class: Class::Session,
        is_dir: true,
        label: "Session backups",
    },
    Entry {
        name: "storage/default",
        class: Class::SiteStorage,
        is_dir: true,
        label: "Site storage",
    },
    Entry {
        name: "webappsstore.sqlite",
        class: Class::SiteStorage,
        is_dir: false,
        label: "Local storage",
    },
    Entry {
        name: "storage.sqlite",
        class: Class::SiteStorage,
        is_dir: false,
        label: "Storage index",
    },
    Entry {
        name: "startupCache",
        class: Class::ProfileCache,
        is_dir: true,
        label: "Startup cache",
    },
    Entry {
        name: "shader-cache",
        class: Class::ProfileCache,
        is_dir: true,
        label: "Shader cache",
    },
    Entry {
        name: "thumbnails",
        class: Class::ProfileCache,
        is_dir: true,
        label: "Page thumbnails",
    },
];

static SAFARI_ENTRIES: &[Entry] = &[
    Entry {
        name: "Cookies.binarycookies",
        class: Class::Cookies,
        is_dir: false,
        label: "Cookies",
    },
    Entry {
        name: "History.db",
        class: Class::History,
        is_dir: false,
        label: "Browsing history",
    },
    Entry {
        name: "Downloads.plist",
        class: Class::History,
        is_dir: false,
        label: "Download history",
    },
    Entry {
        name: "LastSession.plist",
        class: Class::Session,
        is_dir: false,
        label: "Last session",
    },
    Entry {
        name: "RecentlyClosedTabs.plist",
        class: Class::Session,
        is_dir: false,
        label: "Recently closed tabs",
    },
    Entry {
        name: "TopSites.plist",
        class: Class::Session,
        is_dir: false,
        label: "Top sites",
    },
    Entry {
        name: "LocalStorage",
        class: Class::SiteStorage,
        is_dir: true,
        label: "Local storage",
    },
    Entry {
        name: "Databases",
        class: Class::SiteStorage,
        is_dir: true,
        label: "Web databases",
    },
    Entry {
        name: "WebsiteData",
        class: Class::SiteStorage,
        is_dir: true,
        label: "Website data",
    },
];

/// Safari has no profiles; it has several roots, and two of them are inside its
/// own sandbox container.
///
/// The container roots are **shown and never offered**. No module offers a path
/// inside another application's container — M4 withheld a container's user data
/// and its group containers for the same reason — and widening that is a
/// decision for a human, not a side effect of this one. The honest consequence:
/// on recent macOS the container jar is the live one, so Safari cookies may
/// have nothing offerable until that decision is taken.
static SAFARI_ROOTS: &[(&str, bool)] = &[
    ("Library/Safari", true),
    ("Library/Cookies", true),
    (
        "Library/Containers/com.apple.Safari/Data/Library/Cookies",
        false,
    ),
    (
        "Library/Containers/com.apple.Safari/Data/Library/WebKit",
        false,
    ),
];

/// Every name this module is willing to look for, for a family. Pinned by a
/// test so that widening it is a deliberate edit.
pub fn recognized_names(family: Family) -> Vec<&'static str> {
    entries(family).iter().map(|e| e.name).collect()
}

fn entries(family: Family) -> &'static [Entry] {
    match family {
        Family::Chromium => CHROMIUM_ENTRIES,
        Family::Firefox => FIREFOX_ENTRIES,
        Family::Safari => SAFARI_ENTRIES,
    }
}

/// The sidecars SQLite may leave beside a database, in the order they must be
/// disposed of.
///
/// The order is a safety property. `executor::execute` continues past a failed
/// action, so a mid-sequence failure must leave the database **present** with
/// its sidecars gone — recoverable — and never the reverse. A lone `-journal`
/// is the one genuinely bad outcome: SQLite treats it as a hot journal and
/// rolls it back against a newly created empty database.
const SIDECARS: &[&str] = &["-journal", "-shm", "-wal"];

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

/// Why a browser's data could not be read — or that there was none.
///
/// `NeedsFullDiskAccess` and `NotInstalled` look alike through `read_dir` and
/// mean opposite things: "grant access and try again" versus "there is nothing
/// here". Conflating them sends a user to System Settings to fix something that
/// is not broken, or hides a whole browser behind a shrug.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Access {
    Readable,
    NotInstalled,
    NeedsFullDiskAccess,
    Unreadable(String),
}

#[derive(Debug, Clone)]
pub struct BrowserState {
    pub id: &'static str,
    pub name: &'static str,
    pub family: Family,
    pub access: Access,
    /// Corroborated profiles found. Safari has none by construction.
    pub profiles: usize,
    /// A lock marker is present. Presence of a marker, **not** proof of a
    /// running process — see [`live_marker`].
    pub may_be_live: bool,
    pub notes: Vec<&'static str>,
}

/// Something another cleaner already covers. Deliberately carries **no size**:
/// a field that does not exist cannot be added to a total twice, which is what
/// M7 needs when it combines every module into one figure.
#[derive(Debug, Clone)]
pub struct Covered {
    pub path: PathBuf,
    pub category: &'static str,
    pub browser: &'static str,
}

/// One thing a browser remembers.
///
/// Note what is absent: no `selected` field and no default selection.
#[derive(Debug, Clone)]
pub struct Row {
    pub browser: &'static str,
    pub browser_name: &'static str,
    /// The profile's directory name, for browsers that have profiles.
    pub profile: Option<String>,
    /// The confinement root for this row. Disposal requires every member to be
    /// inside it — stronger than confining to a location root, and what stops
    /// one profile's row from authorizing a path in the next.
    pub profile_root: PathBuf,
    pub class: Class,
    pub consequence: Consequence,
    pub label: &'static str,
    /// How the frontend names this row. Always the last member.
    pub path: PathBuf,
    /// Every path this row disposes of, database **last**.
    pub members: Vec<PathBuf>,
    pub is_dir: bool,
    pub size_bytes: u64,
    pub file_count: u64,
    pub size_is_floor: bool,
    pub offerable: bool,
    pub bulk_grantable: bool,
    pub smart_scan_eligible: bool,
    pub withheld: Option<String>,
    pub undisposable: Option<&'static str>,
}

#[derive(Debug, Default)]
pub struct PrivacyReport {
    pub rows: Vec<Row>,
    pub browsers: Vec<BrowserState>,
    pub covered_elsewhere: Vec<Covered>,
    pub skipped_symlink: usize,
    pub examined: usize,
    pub caveats: Vec<&'static str>,
}

impl PrivacyReport {
    pub fn browser(&self, id: &str) -> Option<&BrowserState> {
        self.browsers.iter().find(|b| b.id == id)
    }

    /// True when the figures are a floor rather than a total.
    ///
    /// A browser that is simply not installed is not a gap. A withheld row is
    /// not a gap either — it was seen and reported, just not offered.
    pub fn is_partial(&self) -> bool {
        self.browsers
            .iter()
            .any(|b| !matches!(b.access, Access::Readable | Access::NotInstalled))
            || self.rows.iter().any(|r| r.offerable && r.size_is_floor)
    }

    pub fn offerable_bytes(&self) -> u64 {
        self.rows
            .iter()
            .filter(|r| r.offerable)
            .map(|r| r.size_bytes)
            .sum()
    }
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

pub struct PrivacyConfig {
    /// Canonical home directory (see [`safety::canonical_home`]).
    pub home: PathBuf,
    pub browsers: &'static [BrowserSpec],
    /// The bounds disposal will apply through `guard_dir`, so a row that
    /// already exceeds them is shown and not offered. Injectable only so a
    /// fixture can reach them.
    pub dir_limits: DirLimits,
    pub max_examined: usize,
}

impl PrivacyConfig {
    pub fn new(home: PathBuf) -> Self {
        Self {
            home,
            browsers: BROWSERS,
            dir_limits: DirLimits::default(),
            max_examined: DEFAULT_MAX_EXAMINED,
        }
    }

    fn bounds(&self) -> Bounds {
        Bounds {
            home: self.home.clone(),
            dir_limits: self.dir_limits,
            max_examined: self.max_examined,
        }
    }
}

// ---------------------------------------------------------------------------
// The scan
// ---------------------------------------------------------------------------

/// Read-only. Opens no cookie jar and no database, mutates nothing, and never
/// constructs a `SafePath`.
///
/// Infallible on purpose. M4's scan *refuses* when `/Applications` is
/// unreadable, because there the unanswerable question ("is this app still
/// installed?") fails in the dangerous direction. There is no such question
/// here: an unreadable browser root can only under-report, never mis-offer, so
/// the honest response is a per-browser state rather than a refusal that hides
/// every other browser too.
pub fn scan(cfg: &PrivacyConfig) -> PrivacyReport {
    let mut report = PrivacyReport::default();

    for spec in cfg.browsers {
        let root = cfg.home.join(spec.root);
        let access = access_of(&root);
        let mut state = BrowserState {
            id: spec.id,
            name: spec.name,
            family: spec.family,
            access: access.clone(),
            profiles: 0,
            may_be_live: false,
            notes: Vec::new(),
        };
        if spec.family == Family::Firefox {
            state.notes.push(FIREFOX_HISTORY_NOTE);
        }

        if access == Access::Readable {
            match spec.family {
                Family::Chromium => chromium(spec, &root, cfg, &mut state, &mut report),
                Family::Firefox => firefox(spec, &root, cfg, &mut state, &mut report),
                Family::Safari => safari(spec, cfg, &mut state, &mut report),
            }
        }

        if let Some(cache) = spec.cache_root {
            let path = cfg.home.join(cache);
            if path.is_dir() {
                report.covered_elsewhere.push(Covered {
                    path,
                    category: "user-caches",
                    browser: spec.id,
                });
            }
        }

        report.browsers.push(state);
    }

    if report.browsers.iter().any(|b| b.may_be_live) {
        report.caveats.push(RUNNING_BROWSER_CAVEAT);
    }
    if report
        .rows
        .iter()
        .any(|r| r.browser == "safari" && r.offerable)
    {
        report.caveats.push(SAFARI_QUIT_CAVEAT);
    }

    report.rows.sort_by(|a, b| {
        a.browser
            .cmp(b.browser)
            .then_with(|| a.profile.cmp(&b.profile))
            .then_with(|| a.path.cmp(&b.path))
    });
    report
}

/// A directory that is absent is not a permission problem, and a directory that
/// is denied is not an absence.
fn access_of(root: &Path) -> Access {
    match std::fs::read_dir(root) {
        Ok(_) => Access::Readable,
        Err(e) => match e.kind() {
            std::io::ErrorKind::NotFound => Access::NotInstalled,
            std::io::ErrorKind::PermissionDenied => Access::NeedsFullDiskAccess,
            _ => Access::Unreadable(e.to_string()),
        },
    }
}

/// The presence of a lock marker.
///
/// This is **not** proof that a process is running. Chromium's `SingletonLock`
/// and Firefox's `lock` are created at launch and removed on a clean exit, so a
/// crash leaves a stale one behind. Verifying further would mean reading the
/// pid out of the symlink target and asking whether it is alive — which needs a
/// subprocess or a `libc` dependency, and would put the first `unsafe` FFI into
/// this crate. So the failure mode is deliberate and one-directional: a stale
/// marker makes us withhold a row we could have offered. Annoying, never
/// destructive.
///
/// Firefox's `.parentlock` is deliberately **not** consulted. It is an empty
/// file Firefox keeps on disk permanently and locks with `fcntl`, so it is
/// present whether or not Firefox is running — measured on the reference
/// machine in two profiles, neither of which had a `lock`. Keying on it would
/// withhold every Firefox row forever while looking exactly like it worked.
fn live_marker(dir: &Path, marker: &str) -> Option<PathBuf> {
    let path = dir.join(marker);
    std::fs::symlink_metadata(&path).ok().map(|_| path)
}

/// A directory entry that is its own canonical spelling and not a symlink.
///
/// Counts, rather than resolves, anything that is not: a symlinked profile
/// resolves somewhere the user never saw, and following it would name paths
/// that were never on screen.
fn plain_dir(path: &Path, skipped_symlink: &mut usize) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if meta.file_type().is_symlink() {
        *skipped_symlink += 1;
        return false;
    }
    if !meta.is_dir() {
        return false;
    }
    if std::fs::canonicalize(path).ok().as_deref() != Some(path) {
        *skipped_symlink += 1;
        return false;
    }
    true
}

/// Chromium's internal contexts. `System Profile` is the browser's own network
/// context and `Guest Profile` is ephemeral and browser-managed; neither is a
/// person's browsing, and both carry a `Preferences` file, so corroboration
/// alone would admit them.
const CHROMIUM_NON_PROFILES: &[&str] = &["System Profile", "Guest Profile"];

fn chromium(
    spec: &'static BrowserSpec,
    root: &Path,
    cfg: &PrivacyConfig,
    state: &mut BrowserState,
    report: &mut PrivacyReport,
) {
    let live = live_marker(root, "SingletonLock");
    state.may_be_live = live.is_some();

    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let mut profiles: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        if CHROMIUM_NON_PROFILES.iter().any(|n| *n == name) {
            continue;
        }
        let path = root.join(&name);
        if !plain_dir(&path, &mut report.skipped_symlink) {
            continue;
        }
        // Corroboration: a directory the browser has actually opened.
        if !path.join("Preferences").is_file() {
            continue;
        }
        profiles.push(path);
    }
    profiles.sort();
    state.profiles = profiles.len();

    for profile in profiles {
        let display = profile
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        collect(
            spec,
            &profile,
            Some(display),
            true,
            live.as_deref(),
            cfg,
            report,
        );
    }
}

fn firefox(
    spec: &'static BrowserSpec,
    root: &Path,
    cfg: &PrivacyConfig,
    state: &mut BrowserState,
    report: &mut PrivacyReport,
) {
    let Ok(entries) = std::fs::read_dir(root.join("Profiles")) else {
        return;
    };
    let mut profiles: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let path = root.join("Profiles").join(entry.file_name());
        if !plain_dir(&path, &mut report.skipped_symlink) {
            continue;
        }
        if !path.join("prefs.js").is_file() && !path.join("times.json").is_file() {
            continue;
        }
        profiles.push(path);
    }
    profiles.sort();
    state.profiles = profiles.len();

    for profile in profiles {
        // Firefox's marker is per profile, not per installation.
        let live = live_marker(&profile, "lock");
        state.may_be_live |= live.is_some();
        let display = profile
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        collect(
            spec,
            &profile,
            Some(display),
            true,
            live.as_deref(),
            cfg,
            report,
        );
    }
}

fn safari(
    spec: &'static BrowserSpec,
    cfg: &PrivacyConfig,
    _state: &mut BrowserState,
    report: &mut PrivacyReport,
) {
    for (rel, offerable_root) in SAFARI_ROOTS {
        let root = cfg.home.join(rel);
        if !root.is_dir() {
            continue;
        }
        collect(spec, &root, None, *offerable_root, None, cfg, report);
    }
}

/// Look up every recognized name under one root. Nothing is listed and
/// filtered: a name this module does not already know is never seen.
#[allow(clippy::too_many_arguments)]
fn collect(
    spec: &'static BrowserSpec,
    profile_root: &Path,
    profile: Option<String>,
    root_offerable: bool,
    live: Option<&Path>,
    cfg: &PrivacyConfig,
    report: &mut PrivacyReport,
) {
    for entry in entries(spec.family) {
        let path = profile_root.join(entry.name);
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            report.skipped_symlink += 1;
            continue;
        }
        if meta.is_dir() != entry.is_dir {
            continue;
        }
        // Everything emitted must already be its own canonical spelling — the
        // rule the disposal half re-checks byte for byte.
        if std::fs::canonicalize(&path).ok().as_deref() != Some(path.as_path()) {
            report.skipped_symlink += 1;
            continue;
        }

        let (members, size_bytes, file_count, size_is_floor, undisposable) = if entry.is_dir {
            let m = treewalk::measure(&path, &cfg.bounds(), &mut report.examined);
            (
                vec![path.clone()],
                m.size_bytes,
                m.file_count,
                m.size_is_floor,
                m.undisposable,
            )
        } else {
            let mut members = Vec::new();
            let mut size = meta.len();
            for suffix in SIDECARS {
                let sidecar = sidecar_of(&path, suffix);
                if let Ok(m) = std::fs::symlink_metadata(&sidecar) {
                    if m.is_file() {
                        size = size.saturating_add(m.len());
                        members.push(sidecar);
                    }
                }
            }
            // The database last: a partial run must leave it present with its
            // sidecars gone, never a hot journal beside an empty database.
            members.push(path.clone());
            let count = members.len() as u64;
            (members, size, count, false, None)
        };

        let mut offerable = true;
        let mut withheld = None;
        if entry.class == Class::SiteStorage {
            offerable = false;
            withheld = Some(SITE_STORAGE_REASON.to_string());
        } else if !root_offerable {
            offerable = false;
            withheld = Some(SAFARI_CONTAINER_REASON.to_string());
        } else if let Some(marker) = live {
            // A running browser holds these open and rewrites them on quit, so
            // "removed" would be visibly false a minute later. Caches are the
            // weaker case: the browser rebuilds them and nothing the user cares
            // about is misreported, so they keep a caveat instead.
            if entry.class != Class::ProfileCache {
                offerable = false;
                withheld = Some(format!(
                    "{} looks like it is running ({} is present), and it would write this back",
                    spec.name,
                    marker.display()
                ));
            }
        }
        if offerable {
            if let (false, Some(why)) = treewalk::offer(&treewalk::Measured {
                size_bytes,
                file_count,
                size_is_floor,
                undisposable,
            }) {
                offerable = false;
                withheld = Some(why);
            }
        }

        let consequence = entry.class.consequence();
        // Derived, never assigned: there is no path by which a cookie jar can
        // claim to be sweepable.
        let regenerable = consequence == Consequence::Regenerable;
        report.rows.push(Row {
            browser: spec.id,
            browser_name: spec.name,
            profile: profile.clone(),
            profile_root: profile_root.to_path_buf(),
            class: entry.class,
            consequence,
            label: entry.label,
            path,
            members,
            is_dir: entry.is_dir,
            size_bytes,
            file_count,
            size_is_floor,
            offerable,
            bulk_grantable: offerable && regenerable,
            smart_scan_eligible: offerable && regenerable,
            withheld,
            undisposable,
        });
    }
}

/// `foo.sqlite` + `-wal` → `foo.sqlite-wal`, without going through a string
/// that could carry a separator.
fn sidecar_of(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_ids_are_unique_and_named() {
        let mut ids: Vec<&str> = BROWSERS.iter().map(|b| b.id).collect();
        let len = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), len);
        assert!(BROWSERS.iter().all(|b| !b.name.is_empty()));
    }

    /// Every class maps to exactly one consequence, and the mapping is total —
    /// so a new class cannot be added without deciding what it costs.
    #[test]
    fn every_class_has_a_consequence() {
        for (class, expected) in [
            (Class::Cookies, Consequence::SignsYouOut),
            (Class::History, Consequence::ErasesHistory),
            (Class::Session, Consequence::LosesOpenTabs),
            (Class::SiteStorage, Consequence::LosesSiteData),
            (Class::ProfileCache, Consequence::Regenerable),
        ] {
            assert_eq!(class.consequence(), expected);
        }
    }

    /// The one place a separator may appear in a name is a literal in this
    /// file. Nothing read from disk is ever joined, so nothing can carry `..`.
    #[test]
    fn no_recognised_name_can_escape_its_profile() {
        for family in [Family::Chromium, Family::Firefox, Family::Safari] {
            for name in recognized_names(family) {
                assert!(!name.is_empty());
                assert!(!name.starts_with('/'), "{name} is absolute");
                assert!(
                    !Path::new(name)
                        .components()
                        .any(|c| matches!(c, std::path::Component::ParentDir)),
                    "{name} can climb out of the profile"
                );
            }
        }
    }
}
