import type {
  CleanSummary,
  InstalledApp,
  LargeOldReport,
  LeftoverRow,
  LoginItem,
  PrivacyBrowser,
  PrivacyReport,
  PrivacyRow,
  ScanReport,
  StartupItem,
  StartupReport,
  StartupSummary,
  SpaceLensReport,
  SpaceNode,
  UninstallReport,
} from "../src/types";

const GiB = 1024 * 1024 * 1024;
const MiB = 1024 * 1024;

// Representative data for the UX oracle, injected through Tauri's own transport
// so the screenshots exercise the real data path. Category order matches what
// the backend actually produces: `ScanReport::from_plan` rolls up through a
// BTreeMap keyed by category id, so a real report is always sorted by id.
export const SAMPLE_REPORT: ScanReport = {
  total_count: 4213,
  total_bytes: Math.round(6.44 * GiB),
  requires_confirmation: true,
  skipped_protected: 17,
  // A complete scan, so the screenshots keep showing an unqualified total.
  // A partial fixture is what the visual PR for the floor notice will add.
  skipped_unreadable: 0,
  partial: false,
  items: [],
  by_category: [
    {
      category: "homebrew-downloads",
      name: "Homebrew downloads",
      description: "Cached package downloads; re-downloaded on demand.",
      count: 96,
      bytes: Math.round(812 * MiB),
    },
    {
      category: "user-caches",
      name: "Application caches",
      description: "Per-user app caches; apps recreate what they need.",
      count: 3580,
      bytes: Math.round(1.2 * GiB),
    },
    {
      category: "user-logs",
      name: "Logs",
      description: "Per-user application and system logs.",
      count: 225,
      bytes: Math.round(348 * MiB),
    },
    {
      category: "xcode-derived-data",
      name: "Xcode derived data",
      description: "Build intermediates and indexes; rebuilt automatically.",
      count: 312,
      bytes: Math.round(4.1 * GiB),
    },
  ],
};

export const SAMPLE_LOGIN_ITEMS: LoginItem[] = [
  {
    label: "com.docker.helper",
    program: "/Applications/Docker.app/Contents/MacOS/Docker Desktop.app",
    run_at_load: true,
    plist_says_disabled: false,
    class: "starts_at_login",
    moved_aside: false,
    duplicate_label: false,
    offerable: true,
    withheld: null,
    source: "~/Library/LaunchAgents/com.docker.helper.plist",
  },
  {
    label: "com.google.keystone.agent",
    program:
      "~/Library/Google/GoogleSoftwareUpdate/.../GoogleSoftwareUpdateAgent",
    run_at_load: true,
    plist_says_disabled: false,
    class: "starts_at_login",
    moved_aside: false,
    duplicate_label: false,
    offerable: true,
    withheld: null,
    source: "~/Library/LaunchAgents/com.google.keystone.agent.plist",
  },
  {
    label: "com.spotify.webhelper",
    program: "/Applications/Spotify.app/Contents/MacOS/Spotify",
    run_at_load: true,
    plist_says_disabled: false,
    class: "starts_at_login",
    moved_aside: false,
    duplicate_label: false,
    offerable: true,
    withheld: null,
    source: "~/Library/LaunchAgents/com.spotify.webhelper.plist",
  },
  {
    label: "com.example.oldtool",
    program: "/usr/local/bin/oldtool",
    run_at_load: false,
    plist_says_disabled: true,
    class: "starts_on_demand",
    moved_aside: false,
    duplicate_label: false,
    offerable: true,
    withheld: null,
    source: "~/Library/LaunchAgents/com.example.oldtool.plist",
  },
];

// The outcome the harness shows for the "done" state. Mirrors what a real
// `clean` returns for SAMPLE_REPORT.
export const SAMPLE_SUMMARY: CleanSummary = {
  dry_run: false,
  executed: 4213,
  refused: 0,
  bytes_freed: Math.round(6.44 * GiB),
  entries_freed: 0,
};

// A representative Large & Old result. Deliberately includes the "partial"
// case: the coverage notice is a safety-relevant surface, so it must be in the
// screenshots rather than only in the happy path.
const DAY = 86_400_000;
export const SAMPLE_LARGE_OLD: LargeOldReport = {
  items: [
    {
      path: "/Users/tester/Movies/wedding-master-4k.mov",
      size_bytes: Math.round(18.4 * GiB),
      modified_ms: Date.now() - 1120 * DAY,
    },
    {
      path: "/Users/tester/Downloads/Xcode_15.4.xip",
      size_bytes: Math.round(7.9 * GiB),
      modified_ms: Date.now() - 410 * DAY,
    },
    {
      path: "/Users/tester/Documents/archive/2019-project-backup.zip",
      size_bytes: Math.round(3.2 * GiB),
      modified_ms: Date.now() - 1900 * DAY,
    },
    {
      path: "/Users/tester/Downloads/ubuntu-24.04-desktop-amd64.iso",
      size_bytes: Math.round(2.6 * GiB),
      modified_ms: Date.now() - 240 * DAY,
    },
    {
      path: "/Users/tester/Pictures/lightroom-export-2021.tar",
      size_bytes: Math.round(1.1 * GiB),
      modified_ms: Date.now() - 60 * DAY,
    },
  ],
  matched: 5,
  matched_bytes: Math.round(33.2 * GiB),
  examined: 168_402,
  truncated: false,
  skipped_unreadable: 2,
  skipped_hardlinked: 1,
  skipped_unrepresentable: 0,
  partial: true,
};

// What `dispose_paths` returns after the two largest rows are chosen.
export const SAMPLE_DISPOSE_SUMMARY: CleanSummary = {
  dry_run: false,
  executed: 2,
  refused: 0,
  bytes_freed: Math.round(26.3 * GiB),
  entries_freed: 0,
};

// --- Uninstaller -----------------------------------------------------------
//
// One report that exercises every row shape the backend can produce, because
// each is a distinct claim the screenshot has to carry: an id-keyed leftover,
// a container part, the user's own documents inside that container (shown,
// never offered), a name-keyed directory with a licence-shaped file in it, an
// orphan sibling, a group container (shared, never claimed), a sibling that is
// still installed, and a tree disposal is certain to refuse. Deliberately
// `partial`, with the cfprefsd caveat, for the same reason as the other
// fixtures: the caveats are safety surfaces and belong in the screenshots.

const L = "/Users/tester/Library";
const KiB = 1024;

export const SAMPLE_INSTALLED_APPS: InstalledApp[] = [
  { id: "com.contoso.sync", name: "Contoso Sync", bundle_path: "/Applications/Contoso Sync.app" },
  { id: "com.example.reader", name: "Example Reader", bundle_path: "/Applications/Example Reader.app" },
  { id: "com.northwind.mail", name: "Northwind Mail", bundle_path: "/Applications/Northwind Mail.app" },
  { id: "com.acme.notes.Sync", name: "Notes Sync", bundle_path: "/Applications/Notes Sync.app" },
];

function row(
  path: string,
  location: string,
  overrides: Partial<LeftoverRow> = {},
): LeftoverRow {
  return {
    path,
    location,
    matched_via: "id",
    kind: "leftover",
    is_dir: true,
    size_bytes: 0,
    file_count: 0,
    size_is_floor: false,
    offerable: true,
    bulk_grantable: true,
    withheld: null,
    undisposable: null,
    license_suspected: false,
    ...overrides,
  };
}

const USER_DATA_REASON =
  "a sandboxed app keeps the user's own data here — possibly the only copy — so it is shown, not offered";
const GROUP_REASON =
  "a group container is shared between apps by construction, and the entitlement that would settle who owns it was in the bundle that is gone";
const GIT_REASON =
  "the tree contains a protected path (a .git checkout, most likely)";
export const CFPREFSD_CAVEAT =
  "a preferences file can be written back by cfprefsd moments after it is removed, if the app is running or is launched again; nothing is quit or stopped to prevent that";

const LEFTOVER_ROWS: LeftoverRow[] = [
  row(`${L}/Caches/com.acme.notes`, "Library/Caches", {
    size_bytes: Math.round(412 * MiB),
    file_count: 3_180,
  }),
  row(`${L}/Caches/com.acme.notes.Helper`, "Library/Caches", {
    matched_via: "sibling:Helper",
    size_bytes: Math.round(3.1 * MiB),
    file_count: 42,
    bulk_grantable: false,
  }),
  row(`${L}/Caches/com.acme.notes.Plugins`, "Library/Caches", {
    matched_via: "sibling:Plugins",
    size_bytes: Math.round(66 * MiB),
    file_count: 2_204,
    size_is_floor: true,
    offerable: false,
    bulk_grantable: false,
    withheld: `this tool cannot remove it: ${GIT_REASON}`,
    undisposable: GIT_REASON,
  }),
  row(`${L}/Caches/com.acme.notes.Sync`, "Library/Caches", {
    matched_via: "sibling:Sync",
    size_bytes: Math.round(18 * MiB),
    file_count: 96,
    offerable: false,
    bulk_grantable: false,
    withheld: "com.acme.notes.Sync is still installed and this is its data",
  }),
  row(`${L}/Containers/com.acme.notes/Data/Library/Caches`, "Library/Containers", {
    size_bytes: Math.round(96 * MiB),
    file_count: 1_208,
  }),
  row(`${L}/Containers/com.acme.notes/Data/Documents`, "Library/Containers", {
    kind: "user_data",
    size_bytes: Math.round(1.2 * GiB),
    file_count: 318,
    offerable: false,
    bulk_grantable: false,
    withheld: USER_DATA_REASON,
  }),
  row(`${L}/Preferences/com.acme.notes.plist`, "Library/Preferences", {
    matched_via: "id.plist",
    is_dir: false,
    size_bytes: 12 * KiB,
    file_count: 1,
  }),
  // Adjacent to Preferences in report order, on purpose: the track draws
  // neighbouring locations side by side, so a hue repeated on neighbours would
  // merge two real quantities into one band, and the visual gate should see
  // the pair that is most likely on a real machine.
  row(
    `${L}/Preferences/ByHost/com.acme.notes.00000000-0000-0000-0000-000000000000.plist`,
    "Library/Preferences/ByHost",
    {
      matched_via: "id.<uuid>.plist",
      is_dir: false,
      size_bytes: 4 * KiB,
      file_count: 1,
    },
  ),
  row(
    `${L}/Saved Application State/com.acme.notes.savedState`,
    "Library/Saved Application State",
    {
      matched_via: "id.savedState",
      size_bytes: 210 * KiB,
      file_count: 4,
    },
  ),
  row(`${L}/Application Support/Acme Notes`, "Library/Application Support", {
    matched_via: "name:Acme Notes",
    size_bytes: Math.round(84 * MiB),
    file_count: 612,
    bulk_grantable: false,
    license_suspected: true,
  }),
  row(`${L}/Group Containers/group.com.acme.notes`, "Library/Group Containers", {
    kind: "shared",
    matched_via: "prefix:group.",
    size_bytes: Math.round(41 * MiB),
    file_count: 77,
    offerable: false,
    bulk_grantable: false,
    withheld: GROUP_REASON,
  }),
];

const offerable = LEFTOVER_ROWS.filter((r) => r.offerable);

export const SAMPLE_UNINSTALL: UninstallReport = {
  target: "com.acme.notes",
  installed: false,
  installed_at: [],
  rows: LEFTOVER_ROWS,
  offerable_count: offerable.length,
  offerable_bytes: offerable.reduce((n, r) => n + r.size_bytes, 0),
  withheld_count: LEFTOVER_ROWS.length - offerable.length,
  examined: 1_842,
  truncated: false,
  skipped_unreadable: 1,
  skipped_symlink: 0,
  skipped_case_variant: 0,
  skipped_unrepresentable: 0,
  skipped_uncorroborated_name: 1,
  dropped_unrepresentable_rows: 0,
  deferred: [
    [
      "~/Library/Cookies",
      "a cookie jar signs the user out of things; it belongs to the Privacy module",
    ],
  ],
  caveats: [CFPREFSD_CAVEAT],
  partial: true,
};

/** The same target with nothing missing, for asserting the caveat stays away. */
export const SAMPLE_UNINSTALL_COMPLETE: UninstallReport = {
  ...SAMPLE_UNINSTALL,
  skipped_unreadable: 0,
  skipped_uncorroborated_name: 0,
  partial: false,
};

/** An app the user picked that is still installed: no rows, by construction. */
export const SAMPLE_UNINSTALL_INSTALLED: UninstallReport = {
  ...SAMPLE_UNINSTALL,
  target: "com.example.reader",
  installed: true,
  installed_at: ["/Applications/Example Reader.app"],
  rows: [],
  offerable_count: 0,
  offerable_bytes: 0,
  withheld_count: 0,
  skipped_unreadable: 0,
  skipped_uncorroborated_name: 0,
  caveats: [],
  partial: false,
};

export const SAMPLE_UNINSTALL_EMPTY: UninstallReport = {
  ...SAMPLE_UNINSTALL_INSTALLED,
  target: "com.contoso.sync",
  installed: false,
  installed_at: [],
};

/** What `dispose_leftovers` returns after the offerable rows are chosen. */
export const SAMPLE_UNINSTALL_SUMMARY: CleanSummary = {
  dry_run: false,
  executed: offerable.length,
  refused: 0,
  bytes_freed: offerable.reduce((n, r) => n + r.size_bytes, 0),
  entries_freed: offerable
    .filter((r) => r.is_dir)
    .reduce((n, r) => n + r.file_count, 0),
};

// --- Space Lens ------------------------------------------------------------
//
// Built with the constructors below rather than written out by hand, because
// the backend guarantees `bytes === sum(children)` at every level that has
// children, and the sunburst divides each ring by exactly that. A hand-written
// fixture that drifted from the invariant would produce a picture the real app
// can never draw — and the visual gate would then be protecting a lie.

function dir(name: string, path: string, children: SpaceNode[]): SpaceNode {
  return {
    name,
    path,
    bytes: children.reduce((n, c) => n + c.bytes, 0),
    files: children.reduce((n, c) => n + c.files, 0),
    is_dir: true,
    // The backend sets this when the child list is not a complete listing —
    // i.e. when the width cap folded the remainder into a rollup. A *child*
    // being collapsed says nothing about whether this listing is complete.
    collapsed: children.some((c) => c.path === null && !c.is_dir),
    children,
  };
}

function file(name: string, path: string, bytes: number): SpaceNode {
  return { name, path, bytes, files: 1, is_dir: false, collapsed: false, children: [] };
}

/** A directory at the depth cap: real bytes, no children, and it says so. */
function capped(
  name: string,
  path: string,
  bytes: number,
  files: number,
): SpaceNode {
  return { name, path, bytes, files, is_dir: true, collapsed: true, children: [] };
}

/** The width-cap rollup. Not a place on disk, so it has no path. */
function rollup(count: number, bytes: number, files: number): SpaceNode {
  return {
    name: `${count} more items`,
    path: null,
    bytes,
    files,
    is_dir: false,
    collapsed: true,
    children: [],
  };
}

const H = "/Users/tester";

const SPACE_ROOTS: SpaceNode[] = [
  dir("Library", `${H}/Library/Application Support`, [
    capped("Containers", `${H}/Library/Application Support/Containers`, Math.round(31.4 * GiB), 84_120),
    capped("MobileSync", `${H}/Library/Application Support/MobileSync`, Math.round(22.8 * GiB), 412),
    dir("Steam", `${H}/Library/Application Support/Steam`, [
      capped("steamapps", `${H}/Library/Application Support/Steam/steamapps`, Math.round(12.1 * GiB), 22_400),
      capped("depotcache", `${H}/Library/Application Support/Steam/depotcache`, Math.round(1.9 * GiB), 210),
    ]),
    rollup(38, Math.round(1.2 * GiB), 9_840),
  ]),
  dir("Movies", `${H}/Movies`, [
    dir("Final Cut Libraries", `${H}/Movies/Final Cut Libraries`, [
      capped("Wedding.fcpbundle", `${H}/Movies/Final Cut Libraries/Wedding.fcpbundle`, Math.round(24.6 * GiB), 3_180),
      capped("Reel 2023.fcpbundle", `${H}/Movies/Final Cut Libraries/Reel 2023.fcpbundle`, Math.round(9.4 * GiB), 1_204),
    ]),
    file("wedding-master-4k.mov", `${H}/Movies/wedding-master-4k.mov`, Math.round(18.4 * GiB)),
    rollup(12, Math.round(2.7 * GiB), 12),
  ]),
  dir("Documents", `${H}/Documents`, [
    dir("archive", `${H}/Documents/archive`, [
      file("2019-project-backup.zip", `${H}/Documents/archive/2019-project-backup.zip`, Math.round(3.2 * GiB)),
      capped("scans", `${H}/Documents/archive/scans`, Math.round(2.4 * GiB), 1_902),
    ]),
    capped("projects", `${H}/Documents/projects`, Math.round(14.8 * GiB), 41_300),
    rollup(64, Math.round(890 * MiB), 1_240),
  ]),
  dir("Downloads", `${H}/Downloads`, [
    file("Xcode_15.4.xip", `${H}/Downloads/Xcode_15.4.xip`, Math.round(7.9 * GiB)),
    file("ubuntu-24.04-desktop-amd64.iso", `${H}/Downloads/ubuntu-24.04-desktop-amd64.iso`, Math.round(2.6 * GiB)),
    rollup(31, Math.round(1.8 * GiB), 31),
  ]),
  dir("Pictures", `${H}/Pictures`, [
    capped("Photos Library.photoslibrary", `${H}/Pictures/Photos Library.photoslibrary`, Math.round(9.2 * GiB), 28_400),
    file("lightroom-export-2021.tar", `${H}/Pictures/lightroom-export-2021.tar`, Math.round(1.1 * GiB)),
  ]),
  dir("Music", `${H}/Music`, [
    capped("Music", `${H}/Music/Music`, Math.round(4.3 * GiB), 6_120),
  ]),
];

// Deliberately `partial`: the coverage caveat is a safety-relevant surface, so
// it belongs in the screenshots rather than only in the happy path.
export const SAMPLE_SPACE_LENS: SpaceLensReport = {
  roots: SPACE_ROOTS,
  total_bytes: SPACE_ROOTS.reduce((n, r) => n + r.bytes, 0),
  total_files: SPACE_ROOTS.reduce((n, r) => n + r.files, 0),
  examined: 214_806,
  truncated: false,
  skipped_unreadable: 3,
  skipped_too_deep: 0,
  nodes: countNodes(SPACE_ROOTS),
  node_budget_reached: false,
  deduped_hardlinks: 118,
  partial: true,
};

function countNodes(nodes: SpaceNode[]): number {
  return nodes.reduce((n, c) => n + 1 + countNodes(c.children), 0);
}

/** The same tree with nothing missing, for asserting the caveat stays away. */
export const SAMPLE_SPACE_LENS_COMPLETE: SpaceLensReport = {
  ...SAMPLE_SPACE_LENS,
  skipped_unreadable: 0,
  partial: false,
};

export const SAMPLE_SPACE_LENS_EMPTY: SpaceLensReport = {
  roots: [],
  total_bytes: 0,
  total_files: 0,
  examined: 0,
  truncated: false,
  skipped_unreadable: 0,
  skipped_too_deep: 0,
  nodes: 0,
  node_budget_reached: false,
  deduped_hardlinks: 0,
  partial: false,
};

// --- Privacy ---------------------------------------------------------------
//
// Shaped after a real run on a reference machine, with every name invented:
// Chrome present and *running* (so its cookies, history and session are
// withheld while its caches stay offered), Firefox present with two profiles,
// Safari denied by Full Disk Access, and several Chromium-family vendor
// directories that exist but hold no profile the browser ever opened.

const PROF1 = `${L}/Application Support/Google/Chrome/Profile 1`;
const PROF2 = `${L}/Application Support/Google/Chrome/Profile 2`;
const FF = `${L}/Application Support/Firefox/Profiles/uh8x.default-release`;

// The backend names the marker as an absolute path — the point being that the
// user can go and look at a stale lock file — so the fixture carries one too,
// and the view folds it to `~/`.
const LIVE_REASON = `Google Chrome looks like it is running (${L}/Application Support/Google/Chrome/SingletonLock is present), and it would write this back`;
const STORAGE_REASON =
  "this is website storage — where a site or a local-first web app keeps data, sometimes the only copy of the user's work — so it is shown, not offered";
export const RUNNING_BROWSER_CAVEAT =
  "a browser looks like it is running: its caches will be rebuilt as soon as it is used again, and anything it is holding open is shown but not offered";
const FIREFOX_HISTORY_NOTE =
  "Firefox history is not offered: places.sqlite holds the history and the bookmarks in one file, so removing it would take the bookmarks too";

function prow(
  path: string,
  label: string,
  cls: PrivacyRow["class"],
  over: Partial<PrivacyRow> = {},
): PrivacyRow {
  const consequence: PrivacyRow["consequence"] = {
    cookies: "signs_you_out",
    history: "erases_history",
    session: "loses_open_tabs",
    site_storage: "loses_site_data",
    cache: "regenerable",
  }[cls] as PrivacyRow["consequence"];
  const offerable = over.offerable ?? true;
  return {
    browser: "google-chrome",
    browser_name: "Google Chrome",
    profile: null,
    class: cls,
    consequence,
    label,
    path,
    member_count: 1,
    is_dir: false,
    size_bytes: 0,
    file_count: 1,
    size_is_floor: false,
    offerable,
    bulk_grantable: offerable && consequence === "regenerable",
    smart_scan_eligible: offerable && consequence === "regenerable",
    withheld: null,
    undisposable: null,
    ...over,
  };
}

const PRIVACY_ROWS: PrivacyRow[] = [
  // Chrome is running, so everything it holds open is shown and withheld.
  prow(`${PROF1}/Cookies`, "Cookies", "cookies", {
    profile: "Profile 1",
    size_bytes: Math.round(1.2 * MiB),
    offerable: false,
    withheld: LIVE_REASON,
  }),
  prow(`${PROF1}/History`, "Browsing history", "history", {
    profile: "Profile 1",
    size_bytes: Math.round(8.9 * MiB),
    member_count: 2,
    file_count: 2,
    offerable: false,
    withheld: LIVE_REASON,
  }),
  prow(`${PROF1}/Sessions`, "Saved sessions", "session", {
    profile: "Profile 1",
    is_dir: true,
    size_bytes: Math.round(4.5 * MiB),
    file_count: 38,
    offerable: false,
    withheld: LIVE_REASON,
  }),
  prow(`${PROF1}/Local Storage`, "Local storage", "site_storage", {
    profile: "Profile 1",
    is_dir: true,
    size_bytes: Math.round(8.1 * MiB),
    file_count: 214,
    offerable: false,
    withheld: STORAGE_REASON,
  }),
  prow(`${PROF1}/Service Worker`, "Service workers", "site_storage", {
    profile: "Profile 1",
    is_dir: true,
    size_bytes: Math.round(362 * MiB),
    file_count: 4_106,
    offerable: false,
    withheld: STORAGE_REASON,
  }),
  prow(`${PROF1}/GPUCache`, "GPU cache", "cache", {
    profile: "Profile 1",
    is_dir: true,
    size_bytes: Math.round(14.3 * MiB),
    file_count: 62,
  }),
  prow(`${PROF2}/GPUCache`, "GPU cache", "cache", {
    profile: "Profile 2",
    is_dir: true,
    size_bytes: Math.round(544 * KiB),
    file_count: 9,
  }),
  prow(`${PROF2}/Code Cache`, "Code cache", "cache", {
    profile: "Profile 2",
    is_dir: true,
    size_bytes: Math.round(21.6 * MiB),
    file_count: 341,
    size_is_floor: true,
  }),
  // Firefox is not running, so its cookies and session are on offer. Its
  // history is not, and the report says why in its own words.
  prow(`${FF}/cookies.sqlite`, "Cookies", "cookies", {
    browser: "firefox",
    browser_name: "Firefox",
    size_bytes: Math.round(512 * KiB),
    member_count: 2,
    file_count: 2,
  }),
  prow(`${FF}/sessionstore-backups`, "Session backups", "session", {
    browser: "firefox",
    browser_name: "Firefox",
    is_dir: true,
    size_bytes: Math.round(2.1 * MiB),
    file_count: 11,
  }),
  prow(`${FF}/storage/default`, "Site storage", "site_storage", {
    browser: "firefox",
    browser_name: "Firefox",
    is_dir: true,
    size_bytes: Math.round(66 * MiB),
    file_count: 1_940,
    offerable: false,
    withheld: STORAGE_REASON,
  }),
];

const privacyOfferable = PRIVACY_ROWS.filter((r) => r.offerable);

const PRIVACY_BROWSERS: PrivacyBrowser[] = [
  {
    id: "safari",
    name: "Safari",
    access: "needs_full_disk_access",
    access_detail: null,
    profiles: 0,
    may_be_live: false,
    notes: [],
  },
  {
    id: "google-chrome",
    name: "Google Chrome",
    access: "readable",
    access_detail: null,
    profiles: 3,
    may_be_live: true,
    notes: [],
  },
  {
    id: "microsoft-edge",
    name: "Microsoft Edge",
    access: "readable",
    access_detail: null,
    profiles: 0,
    may_be_live: false,
    notes: [],
  },
  {
    id: "vivaldi",
    name: "Vivaldi",
    access: "not_installed",
    access_detail: null,
    profiles: 0,
    may_be_live: false,
    notes: [],
  },
  {
    id: "firefox",
    name: "Firefox",
    access: "readable",
    access_detail: null,
    profiles: 2,
    may_be_live: false,
    notes: [FIREFOX_HISTORY_NOTE],
  },
];

export const SAMPLE_PRIVACY: PrivacyReport = {
  rows: PRIVACY_ROWS,
  browsers: PRIVACY_BROWSERS,
  covered_elsewhere: [
    {
      path: `${L}/Caches/Google/Chrome`,
      category: "user-caches",
      browser: "google-chrome",
    },
    {
      path: `${L}/Caches/Firefox`,
      category: "user-caches",
      browser: "firefox",
    },
  ],
  offerable_bytes: privacyOfferable.reduce((n, r) => n + r.size_bytes, 0),
  skipped_symlink: 4,
  skipped_unrepresentable: 0,
  partial: true,
  caveats: [RUNNING_BROWSER_CAVEAT],
};

/** Nothing denied, nothing running: the state where no caveat should show. */
export const SAMPLE_PRIVACY_COMPLETE: PrivacyReport = {
  ...SAMPLE_PRIVACY,
  rows: PRIVACY_ROWS.filter((r) => r.withheld !== LIVE_REASON),
  browsers: PRIVACY_BROWSERS.map((b) =>
    b.access === "needs_full_disk_access"
      ? { ...b, access: "readable" as const, profiles: 1 }
      : { ...b, may_be_live: false },
  ),
  skipped_symlink: 0,
  partial: false,
  caveats: [],
};

/** No browser left anything this screen offers. */
export const SAMPLE_PRIVACY_EMPTY: PrivacyReport = {
  rows: [],
  browsers: PRIVACY_BROWSERS.map((b) => ({
    ...b,
    access: "readable" as const,
    may_be_live: false,
  })),
  covered_elsewhere: [],
  offerable_bytes: 0,
  skipped_symlink: 0,
  skipped_unrepresentable: 0,
  partial: false,
  caveats: [],
};

/** What `dispose_privacy` returns once the offerable rows are chosen. */
export const SAMPLE_PRIVACY_SUMMARY: CleanSummary = {
  dry_run: false,
  executed: privacyOfferable.length,
  refused: 0,
  bytes_freed: privacyOfferable.reduce((n, r) => n + r.size_bytes, 0),
  entries_freed: privacyOfferable
    .filter((r) => r.is_dir)
    .reduce((n, r) => n + r.file_count, 0),
};

// --- Startup ---------------------------------------------------------------
//
// Shaped after the reference machine, with every name invented: 5 items this
// app can act on against 26 it can only read, and the modern store present but
// unreadable. That ratio is the design problem the screen is built around.

const LA = `${L}/LaunchAgents`;
const STORE = `${LA}/Moved aside by mac-cleaner`;

function sitem(
  label: string,
  cls: StartupItem["class"],
  over: Partial<StartupItem> = {},
): StartupItem {
  const describes = {
    starts_at_login: "starts when you log in",
    starts_on_demand: "starts when something asks for it",
    broken: "its program is missing, so it fails at every login",
    unknown: "this app cannot tell when it starts",
  }[cls];
  return {
    label,
    program: `/Applications/${label.split(".").pop()}.app/Contents/MacOS/helper`,
    class: cls,
    describes,
    run_at_load: cls === "starts_at_login",
    plist_says_disabled: false,
    moved_aside: false,
    duplicate_label: false,
    offerable: true,
    withheld: null,
    path: `${LA}/${label}.plist`,
    ...over,
  };
}

export const SAMPLE_STARTUP: StartupReport = {
  items: [
    sitem("com.acme.notes.helper", "starts_at_login"),
    sitem("com.contoso.sync", "starts_at_login", {
      plist_says_disabled: true,
    }),
    sitem("com.northwind.updater", "starts_on_demand"),
    sitem("com.acme.oldtool", "broken", {
      program: "/Applications/Old Tool.app/Contents/MacOS/oldtool",
    }),
    // Shown and never offered: a file this app cannot read as a plist.
    sitem("settings-backup", "unknown", {
      program: null,
      offerable: false,
      withheld:
        "this file could not be read as a property list, so this app cannot say what it launches",
      path: `${LA}/settings-backup.txt`,
    }),
  ],
  moved_aside: [
    sitem("com.example.reader.autostart", "starts_at_login", {
      moved_aside: true,
      path: `${STORE}/com.example.reader.autostart.plist`,
    }),
  ],
  // 26 of them, because that is the ratio the screen is built around: on a
  // reference machine this app can act on 5 of 31 launchd jobs. Rendering the
  // design at 5:3 would have shown a shape the real thing never has.
  system: [
    ...Array.from({ length: 10 }, (_, i) => ({
      label: `com.vendor${i + 1}.agent`,
      program: `/Library/Application Support/Vendor ${i + 1}/agent`,
      path: `/Library/LaunchAgents/com.vendor${i + 1}.agent.plist`,
      directory: "/Library/LaunchAgents",
    })),
    ...Array.from({ length: 16 }, (_, i) => ({
      label: `com.vendor${i + 1}.driver`,
      program: `/Library/PrivilegedHelperTools/vendor${i + 1}-driver`,
      path: `/Library/LaunchDaemons/com.vendor${i + 1}.driver.plist`,
      directory: "/Library/LaunchDaemons",
    })),
  ],
  sources: [
    { path: LA, access: "readable", count: 5 },
    { path: "/Library/LaunchAgents", access: "readable", count: 10 },
    { path: "/Library/LaunchDaemons", access: "readable", count: 16 },
  ],
  starts_at_login: 2,
  modern_store_present: true,
  store: STORE,
  deferred: [
    [
      "~/Library/Preferences/com.apple.loginitems.plist",
      "the legacy login-items store, superseded on modern macOS",
    ],
  ],
  caveats: [],
  skipped_unrepresentable: 0,
  partial: false,
};

/** Nothing kept as a file — the state a modern Mac is most likely to be in. */
export const SAMPLE_STARTUP_EMPTY: StartupReport = {
  ...SAMPLE_STARTUP,
  items: [],
  moved_aside: [],
  starts_at_login: 0,
  // Spreading the populated report left "Looked at ~/Library/LaunchAgents (5)"
  // under "Nothing is kept as a file in your LaunchAgents folder" — a pair the
  // real app cannot produce.
  sources: [
    { path: LA, access: "readable", count: 0 },
    { path: "/Library/LaunchAgents", access: "readable", count: 10 },
    { path: "/Library/LaunchDaemons", access: "readable", count: 16 },
  ],
};

export const SAMPLE_STARTUP_SUMMARY: StartupSummary = {
  moved: 1,
  refused: 0,
};
