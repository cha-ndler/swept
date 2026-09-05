// Mirrors swept_core::report (the `--json` / Tauri `scan` contract).
export interface CategorySummary {
  category: string;
  name: string;
  description: string;
  count: number;
  bytes: number;
  /** May a Smart Scan tick this for you? Policy from the backend registry, so
   *  the frontend cannot invent a default it did not sanction. False for the
   *  Trash — it is the recovery mechanism for everything else this app does,
   *  and emptying it by default would destroy the undo for the same gesture's
   *  other modules. Also false for any category the registry does not know.
   *
   *  Not read yet: the Clean screen's selection is manual and stays that way.
   *  Smart Scan is what consumes this. */
  smart_scan_default: boolean;
}

export interface ScanReport {
  total_count: number;
  total_bytes: number;
  requires_confirmation: boolean;
  /** Candidates the scan saw, guarded and refused. A decision, not a gap. */
  skipped_protected: number;
  /** Places the scan could not look into: a root it could not stat, a
   *  directory it could not open, a path it could not resolve. */
  skipped_unreadable: number;
  /** True when the scan describes less than what is there, so `total_bytes`
   *  and `total_count` are floors. The commonest cause is a cleaner root
   *  behind Full Disk Access — `~/.Trash` is both.
   *
   *  NOT YET RENDERED on the Clean screen, which still shows the total
   *  unqualified and its empty state still says the Mac is tidy. That is the
   *  one screen in the app that presents a floor as a total; the notice is a
   *  screenshot change and goes through the visual gate. */
  partial: boolean;
  by_category: CategorySummary[];
  /** Omitted from the GUI payload — the app renders rollups only. */
  items?: unknown[];
}

// Mirrors swept_gui_core::CleanSummary.
export interface CleanSummary {
  dry_run: boolean;
  executed: number;
  refused: number;
  bytes_freed: number;
  /** Names removed by directory actions — how many files one action stood for. */
  entries_freed: number;
}

// Mirrors swept_core::loginitems::LoginItem.
export interface LoginItem {
  label: string;
  program: string | null;
  run_at_load: boolean;
  /**
   * The plist's `Disabled` key — **a key in a file, not launchd's answer.**
   *
   * Named this way on purpose. Once launchd's own override database holds an
   * entry for a label the key is ignored, and that database is root-owned and
   * unreadable by this app. So the two can disagree, and a field called
   * `disabled` would invite this screen to render a guess as a state.
   */
  plist_says_disabled: boolean;
  /** What the job actually does at login. Derived, never assigned. */
  class: "starts_at_login" | "starts_on_demand" | "broken" | "unknown";
  /** It is in the moved-aside store rather than in LaunchAgents. */
  moved_aside: boolean;
  /** Another item in the same directory declares the same launchd `Label`. */
  duplicate_label: boolean;
  offerable: boolean;
  withheld: string | null;
  source: string;
}

// Scan/clean filters sent to the backend (mirrors gui-core Filters).
export interface Filters {
  older_than_days?: number;
  min_size_bytes?: number;
}

// Mirrors swept_gui_core::LargeOldItem.
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

// Mirrors swept_gui_core::LargeOldReportDto.
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

// Mirrors swept_gui_core::SpaceNodeDto.
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

// Mirrors swept_gui_core::SpaceLensReportDto.
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

// Mirrors swept_gui_core::UninstallTarget — the only two things the
// frontend may name. It cannot set the home or the inventory roots, because a
// frontend that could would be able to make an installed app look uninstalled.
export interface UninstallTarget {
  id: string;
  display_name?: string | null;
}

// Mirrors swept_gui_core::InstalledAppDto. Top-level bundles only.
export interface InstalledApp {
  id: string;
  name: string;
  bundle_path: string;
}

// Mirrors swept_gui_core::LeftoverRowDto.
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

// Mirrors swept_gui_core::UninstallReportDto.
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

// Mirrors swept_gui_core::Permissions. Advisory only: it says what the app
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

// Mirrors swept_gui_core::PrivacyRowDto.
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

// Mirrors swept_gui_core::PrivacyBrowserDto.
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

// Mirrors swept_gui_core::PrivacyReportDto.
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

// Mirrors swept_gui_core::Acknowledged.
//
// The second consent axis, and the mirror of the backend's own gate: every
// field defaults to false there, so a request that omits one is refused rather
// than assumed. Nothing here may be pre-ticked.
export interface Acknowledged {
  signs_you_out: boolean;
  erases_history: boolean;
  loses_open_tabs: boolean;
}

// Mirrors swept_gui_core::StartupItemDto.
//
// Distinct from `LoginItem`, which is the core's own shape: this one carries
// `path` (the identity a selection is matched against) and `describes` (the
// sentence the class means), and it is what the Startup screen renders.
export interface StartupItem {
  label: string;
  program: string | null;
  class: "starts_at_login" | "starts_on_demand" | "broken" | "unknown";
  /** What the class means, in the words the backend chose. */
  describes: string;
  run_at_load: boolean;
  /** A key in a file, not launchd's answer. See `LoginItem`. */
  plist_says_disabled: boolean;
  moved_aside: boolean;
  duplicate_label: boolean;
  offerable: boolean;
  withheld: string | null;
  path: string;
}

// A launchd job in a directory this app can never write to.
//
// Note what is absent: no `offerable`, no `withheld`, no path to act on. A
// control it could never honour is not expressible here rather than
// expressible and false.
export interface SystemItem {
  label: string;
  program: string | null;
  path: string;
  directory: string;
}

export interface SourceState {
  path: string;
  access: "readable" | "absent" | "needs_permission" | "unreadable";
  count: number;
}

// Mirrors swept_gui_core::StartupReportDto.
export interface StartupReport {
  items: StartupItem[];
  /** What this app has set aside, and can put back. */
  moved_aside: StartupItem[];
  system: SystemItem[];
  sources: SourceState[];
  /** How many things will actually start at your next login. */
  starts_at_login: number;
  /** The modern SMAppService store exists. Its contents are never read. */
  modern_store_present: boolean;
  /** Where set-aside items live, shown so it is findable without this app. */
  store: string;
  deferred: [string, string][];
  caveats: string[];
  skipped_unrepresentable: number;
  partial: boolean;
}

// Mirrors swept_gui_core::StartupSummary.
//
// No bytes-freed figure, because nothing is freed — the field does not exist
// rather than existing and reading zero.
export interface StartupSummary {
  moved: number;
  refused: number;
}

// Mirrors swept_gui_core::smartscan.

/** Something a source could not see, named by the source that could not see it.
 *  One boolean would say "some figure somewhere is short", which is not
 *  something a notice on screen can be written from. */
export interface Incompleteness {
  source: string;
  reason: string;
}

/** A byte figure that cannot be rendered without its provenance.
 *
 *  There is deliberately no bare `*_bytes` at the top level of a Smart Scan
 *  report: every figure arrives inside one of these, so the UI always has the
 *  completeness in hand when it draws the number. Every reason currently
 *  recorded means the truth is *higher* than `bytes`. */
export interface Total {
  bytes: number;
  /** Which sources contributed, in dispatch order. */
  from: string[];
  /** Empty when this figure describes everything there is. */
  incomplete: Incompleteness[];
}

/** What runs at login, as a finding. No bytes and no selection, by
 *  construction: setting a plist aside is a move, not a disposal. */
export interface StartupFinding {
  starts_at_login: number;
  can_act_on: number;
  modern_store_present: boolean;
  partial: boolean;
}

export interface SmartScanReport {
  /** When the oldest contributing scan started. Backend-stamped. */
  scanned_at_ms: number;
  /** What the default gesture would free. */
  selected: Total;
  /** What every source reported that could be acted on if ticked. */
  found: Total;
  cleanup: CategorySummary[];
  /** Only rows with no consequence. The rest stay on Privacy, which has the
   *  acknowledgement axis for them. */
  privacy: PrivacyRow[];
  large_old: LargeOldReport;
  startup: StartupFinding;
  permissions: Permissions;
}

/** What the user confirmed: the count and byte total shown on the sheet.
 *
 *  Mirrors `swept_gui_core::Expected`. The backend re-scans inside the call and
 *  refuses if the selection has drifted from these, which is what stops a sheet
 *  outliving the report it describes. */
export interface Expected {
  count: number;
  bytes: number;
}

/** What the user confirmed, per source.
 *
 *  There is deliberately no aggregate `Expected`: a combined count could not be
 *  checked against any single verb's rescan, and inventing a combined tolerance
 *  would be inventing a looser one. */
export interface SmartScanExpected {
  cleanup?: Expected | null;
  privacy?: Expected | null;
  large_old?: Expected | null;
}

/** One confirmed Smart Scan gesture.
 *
 *  Three separately named path fields rather than one tagged list, which is the
 *  structural defence against the hazard this screen introduces: three sources
 *  that used to live in three components now share one state object. There is no
 *  field a privacy path can occupy that routes it to the Large & Old verb.
 *  The backend also rejects unknown fields outright. */
export interface SmartScanRequest {
  /** Echoed back from the report, unchanged. */
  scanned_at_ms: number;
  /** The filters the *report* was built with, echoed back unchanged.
   *
   *  Carried here rather than left to the backend's defaults, because otherwise
   *  the preview and the action are built from two different configurations —
   *  and the divergence is always in the widening direction, removing files the
   *  filter excluded and the user never saw.
   *
   *  **Not optional.** Every other omittable field here defaults to the
   *  *refusing* value; an absent `Filters` would mean no age floor and no size
   *  floor — the widest scan there is. A frontend that lost its filter state
   *  gets a refusal. Misspelled keys are rejected too, for the same reason. */
  filters: Filters;
  categories: string[];
  privacy_paths: string[];
  large_old_paths: string[];
  /** Required for every source that names rows. The backend refuses a source
   *  that names rows without saying what was confirmed. */
  expected?: SmartScanExpected;
  confirm_mass_delete?: SmartScanConfirm;
}

/* There is deliberately no `acknowledged` field.
 *
 * Smart Scan offers only rows whose consequence is `regenerable`, and the
 * privacy verb's own ceiling is wider than that — the gap between them is
 * exactly the acknowledgement axis. Sending acknowledgements here would let a
 * routing bug carry a cookie jar, with the Privacy screen's toggles still set,
 * into a gesture whose sheet never used the words "signed out". Consequences
 * are acknowledged on the Privacy screen, which asks about them. */

/** Which sources the user confirmed a mass delete for.
 *
 *  One boolean cannot answer three questions: each verb evaluates the
 *  mass-delete threshold against its own count, so a single flag would let a
 *  person who confirmed one combined figure cross it inside a module whose own
 *  count they never saw. */
export interface SmartScanConfirm {
  cleanup: boolean;
  privacy: boolean;
  large_old: boolean;
}

/** What happened to one source.
 *
 *  `not_attempted` is the one that matters: "we did not try" must not be
 *  rendered like "we tried and there was nothing". */
export type StepOutcome =
  | { outcome: "executed"; summary: CleanSummary }
  | { outcome: "refused"; reason: string }
  | { outcome: "not_attempted"; because: string }
  | { outcome: "not_selected" };

export type SmartScanStep = { source: string } & StepOutcome;

export interface SmartScanRunReport {
  steps: SmartScanStep[];
  /** Every step either executed or had nothing selected, **and** no individual
   *  action inside an executed step was refused. A step can execute and still
   *  leave something behind. */
  completed: boolean;
  bytes_freed: number;
  entries_freed: number;
  /** Individual actions refused inside steps that otherwise executed —
   *  distinct from a step-level `refused`. */
  actions_refused: number;
}
