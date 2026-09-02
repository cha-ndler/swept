// Mirrors macclean_core::report (the `--json` / Tauri `scan` contract).
export interface CategorySummary {
  category: string;
  name: string;
  description: string;
  count: number;
  bytes: number;
}

export interface ScanReport {
  total_count: number;
  total_bytes: number;
  requires_confirmation: boolean;
  skipped_protected: number;
  by_category: CategorySummary[];
  /** Omitted from the GUI payload — the app renders rollups only. */
  items?: unknown[];
}

// Mirrors macclean_gui_core::CleanSummary.
export interface CleanSummary {
  dry_run: boolean;
  executed: number;
  refused: number;
  bytes_freed: number;
  /** Names removed by directory actions — how many files one action stood for. */
  entries_freed: number;
}

// Mirrors macclean_core::loginitems::LoginItem.
export interface LoginItem {
  label: string;
  program: string | null;
  run_at_load: boolean;
  disabled: boolean;
  source: string;
}

// Scan/clean filters sent to the backend (mirrors gui-core Filters).
export interface Filters {
  older_than_days?: number;
  min_size_bytes?: number;
}

// Mirrors macclean_gui_core::LargeOldItem.
//
// Note what is absent: there is no `selected` field. Rows in this view are
// never pre-ticked and never part of a default clean — the entire safety
// argument for acting outside the cleanup allowlist rests on a human having
// chosen each one, so a selection state arriving from the backend would be the
// wrong shape for it.
export interface LargeOldItem {
  path: string;
  size_bytes: number;
  /** Epoch milliseconds, or null if the mtime could not be read. */
  modified_ms: number | null;
}

// Mirrors macclean_gui_core::LargeOldReportDto.
export interface LargeOldReport {
  items: LargeOldItem[];
  /** Total matches, which may exceed items.length when the list is capped. */
  matched: number;
  matched_bytes: number;
  examined: number;
  truncated: boolean;
  skipped_unreadable: number;
  skipped_hardlinked: number;
  skipped_unrepresentable: number;
  /** True when this describes less than the whole disk, for any reason. */
  partial: boolean;
}

// Mirrors macclean_gui_core::SpaceNodeDto.
//
// Note what is absent, and why it is a stronger absence than Large & Old's:
// there is no `selected` field *and* no command that takes one of these back.
// A rectangle here is a picture of the disk, never a proposal.
export interface SpaceNode {
  name: string;
  /**
   * `null` for a rollup node — which is not a place on disk — and for a name
   * that is not valid UTF-8. The node is still drawn with its real size.
   */
  path: string | null;
  /** Allocated bytes (what the file occupies), for this node and below. */
  bytes: number;
  files: number;
  is_dir: boolean;
  /** True when `children` is not a complete listing of what is inside. */
  collapsed: boolean;
  children: SpaceNode[];
}

// Mirrors macclean_gui_core::SpaceLensReportDto.
export interface SpaceLensReport {
  roots: SpaceNode[];
  total_bytes: number;
  total_files: number;
  examined: number;
  truncated: boolean;
  skipped_unreadable: number;
  skipped_too_deep: number;
  /** How many nodes this payload actually contains. */
  nodes: number;
  /**
   * True when the tree stopped growing at the node budget. Like the depth cap
   * this stops the drawing, not the measuring, so it is deliberately **not** a
   * reason for `partial` — the affected folders are `collapsed` instead.
   */
  node_budget_reached: boolean;
  /** Files reached through more than one name, counted once. */
  deduped_hardlinks: number;
  /** True when the tree describes less than what is on disk, for any reason. */
  partial: boolean;
}

// --- Uninstaller ------------------------------------------------------------

// Mirrors macclean_gui_core::UninstallTarget — the only two things the
// frontend may name. It cannot set the home or the inventory roots, because a
// frontend that could would be able to make an installed app look uninstalled.
export interface UninstallTarget {
  id: string;
  display_name?: string | null;
}

// Mirrors macclean_gui_core::InstalledAppDto. Top-level bundles only.
export interface InstalledApp {
  id: string;
  name: string;
  bundle_path: string;
}

// Mirrors macclean_gui_core::LeftoverRowDto.
//
// As with Large & Old there is no `selected` field: every grant is a human's
// individual choice. `offerable` is the backend's word on whether a row may be
// chosen at all; `bulk_grantable` only says whether a select-all gesture may
// sweep it up, and this screen has no such gesture.
export interface LeftoverRow {
  path: string;
  location: string;
  matched_via: string;
  kind: "leftover" | "user_data" | "shared";
  /** Disposing of this row is a directory action, which always asks. */
  is_dir: boolean;
  size_bytes: number;
  file_count: number;
  size_is_floor: boolean;
  offerable: boolean;
  bulk_grantable: boolean;
  withheld: string | null;
  undisposable: string | null;
  license_suspected: boolean;
}

// Mirrors macclean_gui_core::UninstallReportDto.
export interface UninstallReport {
  target: string;
  /** Still installed — in which case `rows` is empty. */
  installed: boolean;
  installed_at: string[];
  rows: LeftoverRow[];
  offerable_count: number;
  offerable_bytes: number;
  withheld_count: number;
  examined: number;
  truncated: boolean;
  skipped_unreadable: number;
  skipped_symlink: number;
  skipped_case_variant: number;
  skipped_unrepresentable: number;
  skipped_uncorroborated_name: number;
  dropped_unrepresentable_rows: number;
  deferred: [string, string][];
  caveats: string[];
  partial: boolean;
}

// Mirrors macclean_gui_core::Permissions. Advisory only: it says what the app
// could read just now, not what the user has toggled in System Settings.
export interface Permissions {
  trash_readable: boolean;
  containers_readable: boolean;
  // Safari's data is gated separately, and is denied by default on a stock
  // Mac. Deliberately NOT part of `all_readable`, which drives the Cleanup
  // screen's under-reporting notice: folding it in would put a permanent
  // warning on a screen that has nothing to do with Safari. The Privacy
  // report carries Safari's access state itself.
  safari_readable: boolean;
  all_readable: boolean;
}

// Mirrors macclean_gui_core::PrivacyRowDto.
//
// Two absences are the design. There is no `selected` field, because nothing
// here is ever pre-chosen. And there are no **member paths**: a database and
// its `-journal`/`-shm`/`-wal` are one row, and the frontend names a row by its
// own `path` and nothing finer, so it cannot take a row apart. The backend
// expands the members from a fresh scan at the moment it acts.
export interface PrivacyRow {
  browser: string;
  browser_name: string;
  profile: string | null;
  class: "cookies" | "history" | "session" | "site_storage" | "cache";
  consequence:
    | "signs_you_out"
    | "erases_history"
    | "loses_open_tabs"
    | "loses_site_data"
    | "regenerable";
  label: string;
  path: string;
  /** How many names this row stands for. Display only. */
  member_count: number;
  /** Disposing of this row is a directory action, which always asks. */
  is_dir: boolean;
  size_bytes: number;
  file_count: number;
  size_is_floor: boolean;
  offerable: boolean;
  bulk_grantable: boolean;
  smart_scan_eligible: boolean;
  withheld: string | null;
  undisposable: string | null;
}

// Mirrors macclean_gui_core::PrivacyBrowserDto.
export interface PrivacyBrowser {
  id: string;
  name: string;
  access:
    | "readable"
    | "not_installed"
    | "needs_full_disk_access"
    | "unreadable";
  access_detail: string | null;
  profiles: number;
  /** A lock marker is present. Presence, never proof that a process is up. */
  may_be_live: boolean;
  notes: string[];
}

// Something another category already cleans. Deliberately carries no size: a
// number that does not exist cannot be added to a total twice.
export interface CoveredElsewhere {
  path: string;
  category: string;
  browser: string;
}

// Mirrors macclean_gui_core::PrivacyReportDto.
export interface PrivacyReport {
  rows: PrivacyRow[];
  browsers: PrivacyBrowser[];
  covered_elsewhere: CoveredElsewhere[];
  offerable_bytes: number;
  skipped_symlink: number;
  skipped_unrepresentable: number;
  partial: boolean;
  caveats: string[];
}

// Mirrors macclean_gui_core::Acknowledged.
//
// The second consent axis, and the mirror of the backend's own gate: every
// field defaults to false there, so a request that omits one is refused rather
// than assumed. Nothing here may be pre-ticked.
export interface Acknowledged {
  signs_you_out: boolean;
  erases_history: boolean;
  loses_open_tabs: boolean;
}
