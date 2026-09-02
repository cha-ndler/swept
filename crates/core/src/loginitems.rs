//! Startup — what runs when you log in.
//!
//! Read-only. This module **never modifies anything**; moving an item aside is
//! a separate, consent-gated action in [`crate::executor`].
//!
//! # Three ways this could claim more than it knows
//!
//! **"Disabled" is not this module's word to use.** A plist's `Disabled` key is
//! only the *initial* value for a job that launchd's own override database has
//! never seen. Once an override exists — which is what `launchctl disable`
//! writes, in a root-owned database this app cannot read — the key is ignored.
//! So [`LoginItem::plist_says_disabled`] is named for what it is, a key in a
//! file, and nothing here reports a job as disabled. The two can diverge, and
//! the field name is what stops a UI from rendering one as the other.
//!
//! **`RunAtLoad` is not the whole story.** A job with `KeepAlive` starts at load
//! whether or not `RunAtLoad` is present, and one with `StartInterval` or
//! `WatchPaths` does not start at login at all. Counting only `RunAtLoad`
//! under-reports what actually runs — see [`StartClass`].
//!
//! **Most login items are not here.** Measured on a reference machine:
//! 5 user agents, 10 system agents, 16 system daemons, and a modern
//! `SMAppService` store holding the items the user actually sees in System
//! Settings. This module can act on the first group only. It reports the second
//! and third as inventory with no controls, reports the fourth's *existence*
//! without ever reading it, and names in [`DEFERRED_SOURCES`] what it does not
//! look at — because a report of five things invites the reader to conclude
//! their Mac is clean.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// Where the store of moved-aside plists lives, and what it is called.
///
/// **Inside `~/Library/LaunchAgents`, one level down.** launchd does not
/// recurse, so a subdirectory is inert and the job is genuinely not loaded —
/// while the file sits two clicks from where the user already goes to look at
/// login items. That siting is the whole answer to "what happens if the app is
/// removed while items are moved aside": nothing is stranded, because the
/// folder is in the place the user would look anyway, and putting an item back
/// is dragging a file up one level.
///
/// It is also what makes putting an item back need no recorded state at all:
/// the destination is the store's own parent, so no manifest has to remember a
/// path, and no content ever names a destination.
pub const STORE_DIR_NAME: &str = "Moved aside by mac-cleaner";

/// Written into the store when it is created, so the folder explains itself to
/// someone who no longer has this app.
pub const STORE_NOTE_NAME: &str = "How to put these back.txt";

/// The modern `SMAppService` store. Its **presence** is reported so a small
/// count can be read correctly; its contents are never parsed. It is opaque,
/// versioned and Apple-owned, and a misparse would fabricate rows about things
/// the user cannot cross-check.
const MODERN_STORE: &str =
    "Library/Application Support/com.apple.backgroundtaskmanagementagent/backgrounditems.btm";

/// Said on any report, because it is what makes a short list readable.
pub const MODERN_STORE_CAVEAT: &str = "most apps now register their login items with macOS \
     directly; that list lives in System Settings › General › Login Items & Extensions, and \
     this app can neither read it nor change it";

/// Read-only inventory. World-readable, root-owned, never actionable.
pub const SYSTEM_DIRS: &[&str] = &["/Library/LaunchAgents", "/Library/LaunchDaemons"];

/// What this module deliberately does not look at, and why — the
/// `uninstall::DEFERRED_LOCATIONS` pattern, because a source that is silently
/// absent is indistinguishable from one that is empty.
pub static DEFERRED_SOURCES: &[(&str, &str)] = &[
    (
        "~/Library/Application Support/com.apple.backgroundtaskmanagementagent/backgrounditems.btm",
        "the modern login-items store: an opaque, versioned, Apple-owned binary that this app \
         reports the existence of and never parses",
    ),
    (
        "~/Library/Preferences/com.apple.loginitems.plist",
        "the legacy login-items store, superseded on modern macOS and absent on current \
         systems; supporting it would mean maintaining a parser for a format Apple has left \
         behind",
    ),
    (
        "/System/Library/LaunchAgents and /System/Library/LaunchDaemons",
        "macOS's own components, hundreds of them, none of which a user chose or may change; \
         listing them would bury the handful that answer the question",
    ),
];

/// What a job actually does at login.
///
/// Derived from the plist, never assigned, and total — a new variant cannot be
/// added without deciding what it tells the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StartClass {
    /// `RunAtLoad`, or `KeepAlive`, which starts at load whether or not
    /// `RunAtLoad` is present.
    StartsAtLogin,
    /// A schedule, a watched path, a socket — launched when something happens,
    /// not when you log in.
    StartsOnDemand,
    /// Its program is named by an absolute path that is not there. It fails at
    /// every login, so moving it aside changes no working behaviour.
    Broken,
    /// It parses, and says nothing this module can turn into a claim.
    Unknown,
}

impl StartClass {
    pub fn describe(self) -> &'static str {
        match self {
            StartClass::StartsAtLogin => "starts when you log in",
            StartClass::StartsOnDemand => "starts when something asks for it",
            StartClass::Broken => "its program is missing, so it fails at every login",
            StartClass::Unknown => "this app cannot tell when it starts",
        }
    }
}

/// Whether a source could be read — and, when it could not, which kind of
/// could-not. Absent and denied look alike and mean opposite things.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    Readable,
    /// No such directory. Not a problem, and not something to send anyone to
    /// System Settings about.
    Absent,
    NeedsPermission,
    Unreadable(String),
}

/// One place this module looked, and how that went.
#[derive(Debug, Clone, Serialize)]
pub struct SourceState {
    pub path: String,
    pub access: Access,
    pub count: usize,
}

/// A launchd job in a directory this app can never write to.
///
/// Note what is absent: no `offerable`, no `withheld`, no path this module
/// would act on. A control it can never honour is not expressible here, rather
/// than expressible and false.
#[derive(Debug, Clone, Serialize)]
pub struct SystemItem {
    pub label: String,
    pub program: Option<String>,
    pub source: String,
    /// Which system directory it came from, for grouping.
    pub directory: String,
}

/// A single login item parsed from a LaunchAgent plist.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LoginItem {
    /// The launchd `Label` (falls back to the file stem if absent).
    pub label: String,
    /// The program path (`Program`, or the first of `ProgramArguments`).
    pub program: Option<String>,
    /// `RunAtLoad` as written in the plist. Kept as a raw fact; what the user
    /// is told comes from [`LoginItem::class`], which also accounts for
    /// `KeepAlive` and for the keys that mean "on demand".
    pub run_at_load: bool,
    /// The plist's `Disabled` key — **a key in a file, not launchd's answer.**
    ///
    /// Named this way on purpose. Once launchd's own override database has an
    /// entry for a label, this key is ignored, and that database is root-owned
    /// and unreadable here. So the two can disagree, and a field called
    /// `disabled` would invite a UI to render a guess as a state.
    pub plist_says_disabled: bool,
    /// What this job does at login.
    pub class: StartClass,
    /// It is in the moved-aside store rather than in `LaunchAgents`.
    pub moved_aside: bool,
    /// Another item in the same directory declares the same `Label`. Two rows
    /// a user cannot tell apart is how consent is given to the wrong one.
    pub duplicate_label: bool,
    /// May this be acted on at all?
    pub offerable: bool,
    /// Why not, when it is not.
    pub withheld: Option<String>,
    /// Absolute path of the source plist.
    pub source: String,
}

/// The default per-user LaunchAgents directory for `home`.
pub fn default_dir(home: &Path) -> PathBuf {
    home.join("Library/LaunchAgents")
}

/// Where moved-aside plists live. See [`STORE_DIR_NAME`].
pub fn store_dir(home: &Path) -> PathBuf {
    default_dir(home).join(STORE_DIR_NAME)
}

/// Parse every entry of one LaunchAgents-shaped directory. Read-only.
///
/// The narrow entry point [`scan`] is built on. Kept public because it is the
/// honest unit to test: one directory in, rows out, nothing else consulted.
pub fn scan_dir(dir: &Path) -> Vec<LoginItem> {
    let (mut items, _) = read_agents(dir, false);
    mark_duplicate_labels(&mut items);
    items
}

/// Serialize a list of login items to pretty JSON.
pub fn to_json_pretty(items: &[LoginItem]) -> String {
    serde_json::to_string_pretty(items).unwrap_or_else(|_| "[]".to_string())
}

pub struct StartupConfig {
    /// Canonical home directory (see [`safety::canonical_home`]).
    pub home: PathBuf,
    /// Read-only inventory directories. A field so a fixture can point them
    /// somewhere that is not the machine the test runs on.
    pub system_dirs: Vec<PathBuf>,
}

impl StartupConfig {
    pub fn new(home: PathBuf) -> Self {
        debug_assert!(
            std::fs::canonicalize(&home)
                .map(|c| c == home)
                .unwrap_or(true),
            "StartupConfig::new needs a canonical home (see safety::canonical_home)"
        );
        Self {
            home,
            system_dirs: SYSTEM_DIRS.iter().map(PathBuf::from).collect(),
        }
    }
}

#[derive(Debug, Default, Serialize)]
pub struct StartupReport {
    /// What is in `~/Library/LaunchAgents` right now.
    pub items: Vec<LoginItem>,
    /// What this app has moved aside, and can put back.
    pub moved_aside: Vec<LoginItem>,
    /// Launchd jobs this app can read and can never change.
    pub system: Vec<SystemItem>,
    pub sources: Vec<SourceState>,
    /// The modern `SMAppService` store exists. Its contents are never read.
    pub modern_store_present: bool,
    pub deferred: Vec<(String, String)>,
    pub caveats: Vec<String>,
    /// A source could not be read, so the picture is incomplete.
    pub partial: bool,
}

impl StartupReport {
    /// How many things will actually start at your next login.
    pub fn starts_at_login(&self) -> usize {
        self.items
            .iter()
            .filter(|i| i.class == StartClass::StartsAtLogin)
            .count()
    }

    pub fn offerable(&self) -> usize {
        self.items.iter().filter(|i| i.offerable).count()
    }
}

/// Read-only. Opens no program, writes nothing, and does not create the store.
pub fn scan(cfg: &StartupConfig) -> StartupReport {
    let mut report = StartupReport::default();

    let agents = default_dir(&cfg.home);
    let (items, access) = read_agents(&agents, false);
    report.sources.push(SourceState {
        path: agents.display().to_string(),
        access: access.clone(),
        count: items.len(),
    });
    report.items = items;

    // The store is only read when it is there. Creating it to answer a
    // question would be writing to the user's Library to look at it.
    let store = store_dir(&cfg.home);
    if store.is_dir() {
        let (moved, store_access) = read_agents(&store, true);
        report.sources.push(SourceState {
            path: store.display().to_string(),
            access: store_access,
            count: moved.len(),
        });
        report.moved_aside = moved;
    }

    for dir in &cfg.system_dirs {
        let (found, access) = read_system(dir);
        report.sources.push(SourceState {
            path: dir.display().to_string(),
            access,
            count: found.len(),
        });
        report.system.extend(found);
    }

    mark_duplicate_labels(&mut report.items);

    report.modern_store_present = cfg.home.join(MODERN_STORE).exists();
    if report.modern_store_present {
        report.caveats.push(MODERN_STORE_CAVEAT.to_string());
    }
    report.deferred = DEFERRED_SOURCES
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
    report.partial = report
        .sources
        .iter()
        .any(|s| !matches!(s.access, Access::Readable | Access::Absent));

    report
}

/// Absent is not denied, and denied is not empty.
fn access_of(dir: &Path) -> Access {
    match std::fs::read_dir(dir) {
        Ok(_) => Access::Readable,
        Err(e) => match e.kind() {
            std::io::ErrorKind::NotFound => Access::Absent,
            std::io::ErrorKind::PermissionDenied => Access::NeedsPermission,
            _ => Access::Unreadable(e.to_string()),
        },
    }
}

fn read_agents(dir: &Path, moved_aside: bool) -> (Vec<LoginItem>, Access) {
    let access = access_of(dir);
    if access != Access::Readable {
        return (Vec::new(), access);
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (Vec::new(), access);
    };
    let mut items = Vec::new();
    for entry in entries.flatten() {
        let path = dir.join(entry.file_name());
        // The store is a directory inside the directory it serves; it is not a
        // login item and launchd does not descend into it.
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            continue;
        }
        if let Some(item) = describe(&path, &meta, moved_aside) {
            items.push(item);
        }
    }
    items.sort_by(|a, b| a.label.cmp(&b.label).then_with(|| a.source.cmp(&b.source)));
    (items, access)
}

fn read_system(dir: &Path) -> (Vec<SystemItem>, Access) {
    let access = access_of(dir);
    if access != Access::Readable {
        return (Vec::new(), access);
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (Vec::new(), access);
    };
    let mut items = Vec::new();
    for entry in entries.flatten() {
        let path = dir.join(entry.file_name());
        if path.extension().and_then(|e| e.to_str()) != Some("plist") {
            continue;
        }
        let Some((label, program)) = read_label_and_program(&path) else {
            continue;
        };
        items.push(SystemItem {
            label,
            program,
            source: path.display().to_string(),
            directory: dir.display().to_string(),
        });
    }
    items.sort_by(|a, b| a.label.cmp(&b.label));
    (items, access)
}

/// One entry in a LaunchAgents directory, as a row — including the ones this
/// module will not act on, because a file that is there and unexplained reads
/// as a file the scan missed.
fn describe(path: &Path, meta: &std::fs::Metadata, moved_aside: bool) -> Option<LoginItem> {
    let name = path.file_name()?.to_str()?.to_string();

    let mut withheld = None;
    if meta.file_type().is_symlink() {
        withheld = Some(
            "this is a link to somewhere else, and this app acts only on the file it can see"
                .to_string(),
        );
    } else if !meta.is_file() {
        withheld = Some("this is not a regular file".to_string());
    } else if path.extension().and_then(|e| e.to_str()) != Some("plist") {
        withheld = Some("this is not a .plist, so this app does not know what it is".to_string());
    }

    let parsed = if withheld.is_none() {
        read_plist(path)
    } else {
        None
    };
    if withheld.is_none() && parsed.is_none() {
        withheld = Some(
            "this file could not be read as a property list, so this app cannot say what it \
             launches"
                .to_string(),
        );
    }

    let (label, program, run_at_load, plist_says_disabled, class) = match parsed {
        Some(p) => p,
        None => (
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&name)
                .to_string(),
            None,
            false,
            false,
            StartClass::Unknown,
        ),
    };

    Some(LoginItem {
        label,
        program,
        run_at_load,
        plist_says_disabled,
        class,
        moved_aside,
        duplicate_label: false,
        offerable: withheld.is_none(),
        withheld,
        source: path.display().to_string(),
    })
}

type Parsed = (String, Option<String>, bool, bool, StartClass);

fn read_plist(path: &Path) -> Option<Parsed> {
    let value = plist::Value::from_file(path).ok()?;
    let dict = value.as_dictionary()?;

    let label = dict
        .get("Label")
        .and_then(|v| v.as_string())
        .map(str::to_string)
        .unwrap_or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string()
        });

    let program = dict
        .get("Program")
        .and_then(|v| v.as_string())
        .map(str::to_string)
        .or_else(|| {
            dict.get("ProgramArguments")
                .and_then(|v| v.as_array())
                .and_then(|a| a.first())
                .and_then(|v| v.as_string())
                .map(str::to_string)
        });

    let run_at_load = dict
        .get("RunAtLoad")
        .and_then(|v| v.as_boolean())
        .unwrap_or(false);
    let plist_says_disabled = dict
        .get("Disabled")
        .and_then(|v| v.as_boolean())
        .unwrap_or(false);

    // `KeepAlive` starts a job at load whether or not `RunAtLoad` is set, and
    // it is a key that may be a boolean *or* a dictionary of conditions.
    let keep_alive = dict
        .get("KeepAlive")
        .is_some_and(|v| v.as_boolean().unwrap_or(true) || v.as_dictionary().is_some());
    let on_demand = [
        "StartInterval",
        "StartCalendarInterval",
        "WatchPaths",
        "QueueDirectories",
        "Sockets",
        "MachServices",
    ]
    .iter()
    .any(|k| dict.contains_key(k));

    let class = if is_broken(program.as_deref()) {
        StartClass::Broken
    } else if run_at_load || keep_alive {
        StartClass::StartsAtLogin
    } else if on_demand {
        StartClass::StartsOnDemand
    } else {
        StartClass::Unknown
    };

    Some((label, program, run_at_load, plist_says_disabled, class))
}

/// Its program is named absolutely and is not there.
///
/// Two guards, both load-bearing. **Only `NotFound`**: absent and denied look
/// alike through a failed lookup and mean opposite things, and calling a
/// working item broken is the wrong direction to be wrong in. **Only an
/// absolute path**: a relative program, or a `/bin/sh` wrapper whose real work
/// is in `argv[1]`, cannot support the claim either way, so say nothing.
fn is_broken(program: Option<&str>) -> bool {
    let Some(program) = program else {
        return false;
    };
    let path = Path::new(program);
    if !path.is_absolute() {
        return false;
    }
    matches!(
        std::fs::symlink_metadata(path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound
    )
}

fn read_label_and_program(path: &Path) -> Option<(String, Option<String>)> {
    let (label, program, _, _, _) = read_plist(path)?;
    Some((label, program))
}

fn mark_duplicate_labels(items: &mut [LoginItem]) {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for item in items.iter() {
        *counts.entry(item.label.clone()).or_default() += 1;
    }
    for item in items.iter_mut() {
        item.duplicate_label = counts.get(&item.label).copied().unwrap_or(0) > 1;
    }
}
