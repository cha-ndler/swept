import type { LoginItem, ScanReport } from "./types";

const GiB = 1024 * 1024 * 1024;
const MiB = 1024 * 1024;

// Representative data so the UI renders in the browser/Playwright (where the
// Tauri `scan` command isn't available). The real app uses live scan results.
export const SAMPLE_REPORT: ScanReport = {
  total_count: 4213,
  total_bytes: Math.round(6.44 * GiB),
  requires_confirmation: true,
  skipped_protected: 17,
  items: [],
  by_category: [
    {
      category: "xcode-derived-data",
      name: "Xcode derived data",
      description: "Build intermediates and indexes; rebuilt automatically.",
      count: 312,
      bytes: Math.round(4.1 * GiB),
    },
    {
      category: "user-caches",
      name: "Application caches",
      description: "Per-user app caches; apps recreate what they need.",
      count: 3580,
      bytes: Math.round(1.2 * GiB),
    },
    {
      category: "homebrew-downloads",
      name: "Homebrew downloads",
      description: "Cached package downloads; re-downloaded on demand.",
      count: 96,
      bytes: Math.round(812 * MiB),
    },
    {
      category: "user-logs",
      name: "Logs",
      description: "Per-user application and system logs.",
      count: 225,
      bytes: Math.round(348 * MiB),
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
    program: "~/Library/Google/GoogleSoftwareUpdate/.../GoogleSoftwareUpdateAgent",
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
