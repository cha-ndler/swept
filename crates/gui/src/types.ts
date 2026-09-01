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

// Mirrors macclean_gui_core::Permissions. Advisory only: it says what the app
// could read just now, not what the user has toggled in System Settings.
export interface Permissions {
  trash_readable: boolean;
  containers_readable: boolean;
  all_readable: boolean;
}
