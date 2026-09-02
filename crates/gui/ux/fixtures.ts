import type {
  CleanSummary,
  InstalledApp,
  LargeOldReport,
  LeftoverRow,
  LoginItem,
  ScanReport,
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
    disabled: false,
    source: "~/Library/LaunchAgents/com.docker.helper.plist",
  },
  {
    label: "com.google.keystone.agent",
    program:
      "~/Library/Google/GoogleSoftwareUpdate/.../GoogleSoftwareUpdateAgent",
    run_at_load: true,
    disabled: false,
    source: "~/Library/LaunchAgents/com.google.keystone.agent.plist",
  },
  {
    label: "com.spotify.webhelper",
    program: "/Applications/Spotify.app/Contents/MacOS/Spotify",
    run_at_load: true,
    disabled: false,
    source: "~/Library/LaunchAgents/com.spotify.webhelper.plist",
  },
  {
    label: "com.example.oldtool",
    program: "/usr/local/bin/oldtool",
    run_at_load: false,
    disabled: true,
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
