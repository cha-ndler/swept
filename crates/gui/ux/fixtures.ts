import type {
  CleanSummary,
  LargeOldReport,
  LoginItem,
  ScanReport,
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
};
