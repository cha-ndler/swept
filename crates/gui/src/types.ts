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
  items: unknown[];
}
