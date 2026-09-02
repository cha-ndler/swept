import type {
  CleanSummary,
  LargeOldReport,
  LoginItem,
  ScanReport,
  SpaceLensReport,
  SpaceNode,
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
