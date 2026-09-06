import { useMemo, useState } from "react";
import { formatBytes } from "./format";
import type {
  CategorySummary,
  CleanSummary,
  PrivacyRow,
  SmartScanReport,
  SmartScanRunReport,
  SmartScanStep,
  Total,
} from "./types";
import { call, describeError, isDesktopApp } from "./backend";
import {
  AccessNotice,
  Banner,
  InfoIcon,
  ShieldIcon,
  SparkleIcon,
  Toolbar,
} from "./Shell";
import { Checkbox } from "./Controls";
import { ScanRing } from "./ScanRing";
import type { RingSegment } from "./ScanRing";
import { hue, PRIVACY_HUE } from "./hues";

/**
 * Smart Scan — one gesture across the sources that can be acted on together.
 *
 * **What this screen offers is exactly what the dispatcher will act on**, and
 * that sentence is the whole design. `gui-core/src/smartscan.rs` earned it the
 * hard way: three review rounds found three different ways for the set the
 * report *showed* to differ from the set the request *named*, each one a working
 * data-loss path. The backend now refuses every one of them. This screen's job
 * is to never rely on that.
 *
 * Concretely, three rules it keeps:
 *
 * - **Only `smart_scan_default` categories are tickable.** The Trash is on the
 *   report and is never on the gesture: it is the recovery mechanism for
 *   everything else this app does, so emptying it here would destroy the undo
 *   for the same click that made it necessary. It appears below, under a
 *   heading that says so, with no checkbox.
 * - **Large & Old is a finding, not a selection.** The aggregator already
 *   excludes it from `selected` for a reason the UI must not undo: its rows need
 *   a human looking at each one, with the path and the age in front of them,
 *   which is what its own screen is for. Rebuilding a thinner version of that
 *   list inside a screen whose whole affordance is "one button" would be the
 *   worst place in the app to make a per-file decision. So this screen never
 *   sends `large_old_paths`, and the ledger afterwards says `not selected` for
 *   it — which is precisely what that outcome exists to say.
 * - **No acknowledgement axis.** Every privacy row here is `regenerable` — the
 *   backend hands `Acknowledged::default()` to the verb whatever we send, so
 *   there is nothing to ask and asking would imply otherwise.
 *
 * **Two figures, never one.** `selected` is what the gesture would free;
 * `found` is everything the sources reported. They are drawn in two different
 * places on purpose, because a single number covering both would be a promise
 * about bytes no confirmation could release.
 */

type View = "idle" | "scanning" | "results" | "error";
type Phase = "none" | "confirm" | "running" | "done";

// The same thresholds the backend evaluates per source (core/src/plan.rs).
// Mirrored rather than guessed at: each verb applies them to its *own* count,
// which is why the request carries three booleans and not one.
const MASS_COUNT = 100;
const MASS_BYTES = 5 * 1024 ** 3;

/** The sources, in the order the backend dispatches them. */
const SOURCE_LABEL: Record<string, string> = {
  cleanup: "Cleanup",
  privacy: "Browser data",
  "large-old": "Large & old files",
};

export default function SmartScanView({
  onTotal,
  onOpenModule,
}: {
  /**
   * The headline figure as a *string*, for the sidebar badge and the menu bar.
   * A number could not carry "at least", and this figure often has to.
   */
  onTotal?: (label: string | null) => void;
  /** Hand the user to the module that can act on a finding. */
  onOpenModule?: (module: "cleanup" | "large-old" | "privacy" | "startup") => void;
}) {
  // Outside the desktop app there is no disk to scan, and this screen is the
  // first thing a browser would render. An idle "Ready to scan" hero there
  // would look like a working app right up until the button did nothing — the
  // same lie as the sample-data fallback, arriving one click later. So the
  // preview shell starts at the truth instead.
  const [view, setView] = useState<View>(() =>
    isDesktopApp() ? "idle" : "error",
  );
  const [report, setReport] = useState<SmartScanReport | null>(null);
  const [cats, setCats] = useState<Set<string>>(new Set());
  const [rows, setRows] = useState<Set<string>>(new Set());
  const [error, setError] = useState("");
  const [phase, setPhase] = useState<Phase>("none");
  const [run, setRun] = useState<SmartScanRunReport | null>(null);
  const [runError, setRunError] = useState("");

  const offered = useMemo(
    () => (report?.cleanup ?? []).filter((c) => c.smart_scan_default),
    [report],
  );
  const withheld = useMemo(
    () => (report?.cleanup ?? []).filter((c) => !c.smart_scan_default),
    [report],
  );
  const privacyRows = report?.privacy ?? [];

  const chosenCats = useMemo(
    () => offered.filter((c) => cats.has(c.category)),
    [offered, cats],
  );
  const chosenRows = useMemo(
    () => privacyRows.filter((r) => rows.has(r.path)),
    [privacyRows, rows],
  );

  const cleanupBytes = chosenCats.reduce((n, c) => n + c.bytes, 0);
  const cleanupCount = chosenCats.reduce((n, c) => n + c.count, 0);
  const privacyBytes = chosenRows.reduce((n, r) => n + r.size_bytes, 0);
  const privacyItems = chosenRows.reduce((n, r) => n + r.member_count, 0);
  const privacyDirs = chosenRows.filter((r) => r.is_dir).length;
  const privacyFiles = chosenRows.reduce((n, r) => n + r.file_count, 0);
  const selBytes = cleanupBytes + privacyBytes;

  // Derived, not asked. Each verb applies its own threshold to its own count,
  // so this says "the figures on the sheet crossed the line for this source" —
  // and the sheet is the confirmation. A directory action always asks, whatever
  // its size, because a recursive removal is a recursive removal.
  const cleanupMass = cleanupCount > MASS_COUNT || cleanupBytes > MASS_BYTES;
  const privacyMass =
    privacyDirs > 0 || privacyItems > MASS_COUNT || privacyBytes > MASS_BYTES;

  const segments: RingSegment[] = [
    ...chosenCats.map((c) => ({
      id: c.category,
      bytes: c.bytes,
      color: hue(c.category),
    })),
    ...(privacyBytes > 0
      ? [{ id: "privacy", bytes: privacyBytes, color: PRIVACY_HUE }]
      : []),
  ];

  const floor = (report?.selected.incomplete.length ?? 0) > 0;

  async function scan() {
    setPhase("none");
    setRun(null);
    setRunError("");
    setView("scanning");
    try {
      // No filters. Smart Scan is the one-gesture screen, and a filter bar here
      // would be a knob on a button — the Cleanup screen is where a narrowed
      // sweep is chosen and reviewed. The empty object is echoed back on the
      // request unchanged, so the preview and the action cannot be built from
      // two different configurations.
      const r = await call<SmartScanReport>("smart_scan", { filters: {} });
      setReport(r);
      setCats(
        new Set(
          r.cleanup.filter((c) => c.smart_scan_default).map((c) => c.category),
        ),
      );
      setRows(new Set(r.privacy.map((p) => p.path)));
      const label = labelFor(r.selected);
      onTotal?.(label);
      void call("set_tray_label", { label }).catch(() => {});
      setView("results");
    } catch (e) {
      setReport(null);
      setCats(new Set());
      setRows(new Set());
      onTotal?.(null);
      void call("set_tray_label", { label: null }).catch(() => {});
      setError(describeError(e));
      setView("error");
    }
  }

  async function dispatch() {
    if (chosenCats.length === 0 && chosenRows.length === 0) {
      setPhase("none");
      return;
    }
    if (!report) return;
    setRunError("");
    setPhase("running");
    try {
      const result = await call<SmartScanRunReport>("dispatch_smart_scan", {
        request: {
          // Echoed back unchanged. The backend stamped it and compares it
          // against now, so a sheet that outlived its report is refused rather
          // than acted on.
          scanned_at_ms: report.scanned_at_ms,
          filters: {},
          categories: chosenCats.map((c) => c.category),
          privacy_paths: chosenRows.map((r) => r.path),
          // Never populated from here — see the note at the top of this file.
          large_old_paths: [],
          // Three magnitudes, never one. Each goes to its own verb, which
          // re-scans inside the call and refuses if its own selection drifted.
          // `null` where nothing was named, so a source cannot inherit another's
          // confirmation.
          expected: {
            cleanup: chosenCats.length
              ? { count: cleanupCount, bytes: cleanupBytes }
              : null,
            privacy: chosenRows.length
              ? { count: chosenRows.length, bytes: privacyBytes }
              : null,
            large_old: null,
          },
          confirm_mass_delete: {
            cleanup: cleanupMass,
            privacy: privacyMass,
            large_old: false,
          },
        },
      });
      setRun(result);
      setPhase("done");
      // The figures on screen no longer describe the disk. Rather than leave a
      // stale total in the sidebar and the menu bar, clear both: the next scan
      // is one button away, and a wrong number is worse than no number.
      onTotal?.(null);
      void call("set_tray_label", { label: null }).catch(() => {});
    } catch (e) {
      // A rejection here is the whole gesture refusing before any step ran —
      // staleness, a category the scan does not offer, a source that named rows
      // without saying how many. Back to the sheet with the reason on it.
      setRunError(describeError(e));
      setPhase("confirm");
    }
  }

  function toggleCat(id: string) {
    setCats((prev) => {
      const next = new Set(prev);
      if (!next.delete(id)) next.add(id);
      return next;
    });
  }

  function toggleRow(path: string) {
    setRows((prev) => {
      const next = new Set(prev);
      if (!next.delete(path)) next.add(path);
      return next;
    });
  }

  const done = phase === "done" && run !== null;

  return (
    <>
      <Toolbar title="Smart Scan">
        {view === "results" && !done && (
          <button
            onClick={() => void scan()}
            className="rounded-control border border-border bg-surface2 px-3 py-1.5 text-caption font-medium text-text transition-colors duration-fast ease-mac hover:border-borderStrong"
          >
            Rescan
          </button>
        )}
      </Toolbar>

      <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">
        {done && run ? (
          <RunLedger
            run={run}
            onAgain={() => {
              setPhase("none");
              setRun(null);
              void scan();
            }}
          />
        ) : (
          <>
            {view === "idle" && <Idle onScan={() => void scan()} />}
            {view === "scanning" && <Scanning />}
            {view === "error" && (
              <ErrorState
                message={error}
                inApp={isDesktopApp()}
                onRetry={() => void scan()}
              />
            )}
            {view === "results" && report && (
              <>
                {!report.permissions.all_readable && (
                  <AccessNotice perms={report.permissions} />
                )}
                {floor && (
                  <FloorNotice
                    total={report.selected}
                    permsExplain={!report.permissions.all_readable}
                  />
                )}

                <div className="flex flex-col gap-7 md:flex-row md:items-start">
                  <div className="flex flex-none flex-col items-center text-center md:w-[240px]">
                    <ScanRing
                      segments={segments}
                      total={selBytes}
                      caption={floor ? "reclaimable, at least" : "reclaimable"}
                    />

                    <div className="mt-4 inline-flex items-center gap-1.5 rounded-full border border-separator bg-white/[.04] px-2.5 py-1">
                      <span className="text-success">
                        <ShieldIcon size={12} />
                      </span>
                      <span className="text-muted text-caption">
                        Preview only
                      </span>
                    </div>

                    <button
                      onClick={() => setPhase("confirm")}
                      disabled={
                        chosenCats.length === 0 && chosenRows.length === 0
                      }
                      className="mt-4 w-full rounded-control bg-accent px-4 py-2 text-emph font-semibold text-white transition-opacity duration-fast ease-mac disabled:opacity-40"
                    >
                      Review &amp; Clean…
                    </button>
                    {/* Per source, never one combined count. The sheet keeps
                        the same discipline, and so does the request. */}
                    <p className="text-subtle mt-2.5 font-mono text-caption tabular-nums">
                      {chosenCats.length} categor
                      {chosenCats.length === 1 ? "y" : "ies"} ·{" "}
                      {chosenRows.length} location
                      {chosenRows.length === 1 ? "" : "s"}
                    </p>
                  </div>

                  <div className="min-w-0 flex-1 space-y-6">
                    <section>
                      <SectionLabel>Included in this scan</SectionLabel>
                      <ul className="mt-2 space-y-2" aria-label="Selected">
                        {offered.map((c) => (
                          <CategoryRow
                            key={c.category}
                            cat={c}
                            checked={cats.has(c.category)}
                            onToggle={() => toggleCat(c.category)}
                          />
                        ))}
                        {privacyRows.map((r) => (
                          <PrivacyLine
                            key={r.path}
                            row={r}
                            checked={rows.has(r.path)}
                            onToggle={() => toggleRow(r.path)}
                          />
                        ))}
                      </ul>
                      {offered.length === 0 && privacyRows.length === 0 && (
                        <p className="text-muted mt-2 text-body">
                          Nothing this gesture can act on was found.
                        </p>
                      )}
                    </section>

                    <AlsoFound
                      report={report}
                      withheld={withheld}
                      onOpenModule={onOpenModule}
                    />
                  </div>
                </div>
              </>
            )}
          </>
        )}
      </div>

      {(phase === "confirm" || phase === "running") && report && (
        <ConfirmSheet
          cleanup={
            chosenCats.length
              ? {
                  count: cleanupCount,
                  bytes: cleanupBytes,
                  note: cleanupMass ? "a large removal" : null,
                }
              : null
          }
          privacy={
            chosenRows.length
              ? {
                  count: chosenRows.length,
                  files: privacyFiles,
                  bytes: privacyBytes,
                  // Named by its cause, because the two are different rules.
                  // A folder asks whatever its size — a recursive removal is a
                  // recursive removal — and calling that "a large removal"
                  // would put a reason on the sheet that is not the reason.
                  note:
                    privacyDirs > 0
                      ? privacyDirs === 1
                        ? "includes a folder"
                        : `includes ${privacyDirs} folders`
                      : privacyMass
                        ? "a large removal"
                        : null,
                }
              : null
          }
          total={selBytes}
          busy={phase === "running"}
          error={runError}
          onCancel={() => setPhase("none")}
          onConfirm={dispatch}
          onRescan={() => {
            setPhase("none");
            void scan();
          }}
        />
      )}
    </>
  );
}

/** `≥` when the figure is a floor. Same string in the sidebar and the menu bar. */
function labelFor(total: Total): string {
  return total.incomplete.length > 0
    ? `≥ ${formatBytes(total.bytes)}`
    : formatBytes(total.bytes);
}

function SectionLabel({ children }: { children: React.ReactNode }) {
  return (
    <h2 className="text-subtle text-micro font-semibold uppercase tracking-wide">
      {children}
    </h2>
  );
}

function Idle({ onScan }: { onScan: () => void }) {
  return (
    <section className="flex flex-col items-center py-8 text-center">
      {/* 202, not 220. `ScanRing` is a 220px box whose stroke is centred on
          r=92 at width 18, so the circle it actually draws is 202 across. A
          220px ring here made the shape grow by 18px the moment a scan started
          — measured, not guessed — which breaks the one thing that ring is for:
          being the same object at rest, sweeping, and full. */}
      <div
        className="grid h-[202px] w-[202px] place-content-center rounded-full border-[18px] border-white/[.055]"
        aria-hidden="true"
      >
        <span className="text-subtle">
          <SparkleIcon size={52} />
        </span>
      </div>
      <h2 className="mt-6 text-display font-semibold">Ready to scan</h2>
      <p className="text-muted mx-auto mt-2 max-w-sm text-body leading-relaxed">
        Swept looks through caches, logs, build artifacts and what your browsers
        have cached, then shows you exactly what it found.
      </p>
      <button
        onClick={onScan}
        className="mt-6 rounded-control bg-accent px-6 py-2.5 text-emph font-semibold text-white"
      >
        Scan My Mac
      </button>
      <div className="mx-auto mt-8 max-w-md text-left">
        <Banner icon={<ShieldIcon size={15} />} tone="safe">
          A scan is <strong className="font-semibold text-text">read-only</strong>
          . Nothing is moved or removed until you review the results and confirm
          — and even then it goes to the Trash, where you can put it back.
        </Banner>
      </div>
    </section>
  );
}

function Scanning() {
  // Four scans, and none of them reports progress: `smart_scan` builds its own
  // scanner config rather than the one the Clean screen wires an emitter into.
  // So the ring sweeps and the copy promises no counts, rather than showing a
  // zero that would look like a stall.
  return (
    <section
      className="flex flex-col items-center py-6 text-center"
      role="status"
      aria-busy="true"
      aria-live="polite"
      aria-label="Scanning"
    >
      <ScanRing segments={[]} total={0} caption="so far" busy />
      <p className="mt-6 text-title font-semibold">Scanning…</p>
      <p className="text-muted mt-1.5 text-body">
        Caches, logs, build artifacts, browser data and large files.
      </p>
      <p className="text-subtle mt-4 text-caption">
        Read-only. Nothing is changed by a scan.
      </p>
    </section>
  );
}

function CategoryRow({
  cat,
  checked,
  onToggle,
}: {
  cat: CategorySummary;
  checked: boolean;
  onToggle: () => void;
}) {
  return (
    <li>
      <label className="flex cursor-pointer items-center gap-3 rounded-card border border-separator bg-surface px-4 py-3 transition-colors duration-fast ease-mac hover:bg-surface2">
        <Checkbox
          checked={checked}
          onChange={onToggle}
          label={`Include ${cat.name}`}
        />
        {/* A coloured dot on this screen means one thing: this row is an arc in
            the ring. Nothing in "also found" carries one. */}
        <span
          className="h-2 w-2 flex-none rounded-full"
          style={{ background: hue(cat.category) }}
          aria-hidden="true"
        />
        <div className="min-w-0 flex-1">
          <span className="truncate text-body font-medium">{cat.name}</span>
          <p className="text-subtle mt-0.5 truncate text-caption">
            {cat.description}
          </p>
        </div>
        <div className="shrink-0 text-right">
          <span className="block font-mono text-body font-semibold tabular-nums">
            {formatBytes(cat.bytes)}
          </span>
          <span className="text-subtle mt-0.5 block font-mono text-caption tabular-nums">
            {cat.count.toLocaleString()} item{cat.count === 1 ? "" : "s"}
          </span>
        </div>
      </label>
    </li>
  );
}

function PrivacyLine({
  row,
  checked,
  onToggle,
}: {
  row: PrivacyRow;
  checked: boolean;
  onToggle: () => void;
}) {
  return (
    <li>
      <label className="flex cursor-pointer items-center gap-3 rounded-card border border-separator bg-surface px-4 py-3 transition-colors duration-fast ease-mac hover:bg-surface2">
        <Checkbox
          checked={checked}
          onChange={onToggle}
          label={`Include ${row.browser_name} ${row.label}`}
        />
        <span
          className="h-2 w-2 flex-none rounded-full"
          style={{ background: PRIVACY_HUE }}
          aria-hidden="true"
        />
        <div className="min-w-0 flex-1">
          <span className="truncate text-body font-medium">
            {row.browser_name} — {row.label}
          </span>
          {/* The profile is not decoration. A browser with two profiles
              produces two rows whose browser and label are word-for-word
              identical and whose sizes differ — seen in the fixture the moment
              this screen was first rendered — and without it the only way to
              tell which is which is to guess from the number. */}
          <p className="text-subtle mt-0.5 truncate text-caption">
            Regenerated as you browse
            {row.profile ? ` · ${row.profile}` : ""}
          </p>
        </div>
        <div className="shrink-0 text-right">
          <span className="block font-mono text-body font-semibold tabular-nums">
            {row.size_is_floor ? "≥ " : ""}
            {formatBytes(row.size_bytes)}
          </span>
          <span className="text-subtle mt-0.5 block font-mono text-caption tabular-nums">
            {row.file_count.toLocaleString()} file
            {row.file_count === 1 ? "" : "s"}
          </span>
        </div>
      </label>
    </li>
  );
}

/**
 * What the scan saw and this gesture will not touch.
 *
 * The figure is `found − selected`, which is exact: both come from the report,
 * and the difference is by construction everything a source reported that the
 * default set leaves out. It does not move when the user unticks a row — and
 * should not, because this band is about what Smart Scan *never* acts on, not
 * about the current selection.
 *
 * No coloured dots and no checkboxes here. Both would say "this is in the ring".
 */
function AlsoFound({
  report,
  withheld,
  onOpenModule,
}: {
  report: SmartScanReport;
  withheld: CategorySummary[];
  onOpenModule?: (module: "cleanup" | "large-old" | "privacy" | "startup") => void;
}) {
  const extra = Math.max(report.found.bytes - report.selected.bytes, 0);
  const large = report.large_old;
  const startup = report.startup;

  return (
    <section>
      <div className="flex items-baseline justify-between gap-3">
        <SectionLabel>Also found — not part of this scan</SectionLabel>
        {extra > 0 && (
          <span className="text-subtle font-mono text-caption tabular-nums">
            {formatBytes(extra)}
          </span>
        )}
      </div>
      <ul className="mt-2 space-y-2">
        {withheld.map((c) => (
          <FindingRow
            key={c.category}
            name={c.name}
            detail={
              c.category === "trash"
                ? "Emptying it here would remove the way back from everything above"
                : "Reviewed on its own screen"
            }
            figure={formatBytes(c.bytes)}
            action="Cleanup"
            onOpen={() => onOpenModule?.("cleanup")}
          />
        ))}
        {large.matched > 0 && (
          <FindingRow
            name="Large & old files"
            detail="Each one needs your consent, with the path and the age in front of you"
            figure={`${large.matched.toLocaleString()} file${large.matched === 1 ? "" : "s"}`}
            action="Large & Old"
            onOpen={() => onOpenModule?.("large-old")}
          />
        )}
        <FindingRow
          name="Browser data with consequences"
          detail="Cookies sign you out; history cannot be brought back"
          figure="By consequence"
          action="Privacy"
          onOpen={() => onOpenModule?.("privacy")}
        />
        {startup.starts_at_login > 0 && (
          <FindingRow
            name="Starts at login"
            detail={
              startup.can_act_on > 0
                ? `${startup.can_act_on} of them can be set aside — a move, never a removal`
                : "None of them can be changed from here"
            }
            figure={`${startup.starts_at_login} item${startup.starts_at_login === 1 ? "" : "s"}`}
            action="Startup"
            onOpen={() => onOpenModule?.("startup")}
          />
        )}
      </ul>
    </section>
  );
}

function FindingRow({
  name,
  detail,
  figure,
  action,
  onOpen,
}: {
  name: string;
  detail: string;
  figure: string;
  action: string;
  onOpen: () => void;
}) {
  return (
    <li className="flex items-center gap-3 rounded-card border border-separator bg-surface px-4 py-3">
      <div className="min-w-0 flex-1">
        <span className="truncate text-body font-medium">{name}</span>
        <p className="text-subtle mt-0.5 truncate text-caption">{detail}</p>
      </div>
      <span className="text-muted shrink-0 font-mono text-caption tabular-nums">
        {figure}
      </span>
      <button
        onClick={onOpen}
        className="shrink-0 rounded-control border border-border bg-surface2 px-3 py-1.5 text-caption font-medium text-text transition-colors duration-fast ease-mac hover:border-borderStrong"
      >
        {action}
      </button>
    </li>
  );
}

/** The headline is a floor, and this says which source could not see everything. */
function FloorNotice({
  total,
  permsExplain,
}: {
  total: Total;
  permsExplain: boolean;
}) {
  return (
    <div
      role="status"
      className="mb-5 flex items-start gap-3 rounded-card border border-cat-trashes/30 bg-cat-trashes/[.07] px-4 py-3"
    >
      <span className="mt-0.5 flex-none text-cat-trashes">
        <InfoIcon size={16} />
      </span>
      <div className="min-w-0">
        <p className="text-body font-medium">This is a floor, not a total</p>
        <ul className="text-muted mt-1 space-y-0.5 text-body">
          {/* Attributed per source, in that module's own words. One boolean
              saying "some figure somewhere is short" is not something a notice
              can be written from — which is why the backend records these
              individually. */}
          {total.incomplete.map((i) => (
            <li key={`${i.source}:${i.reason}`}>
              <span className="font-medium text-text">
                {SOURCE_LABEL[i.source] ?? i.source}
              </span>
              : {i.reason}.
            </li>
          ))}
        </ul>
        <p className="text-muted mt-1 text-body">
          {permsExplain
            ? "These are on top of the locations macOS is withholding, above. "
            : ""}
          What is shown is real; it is the completeness that is in doubt.
        </p>
      </div>
    </div>
  );
}

function ConfirmSheet({
  cleanup,
  privacy,
  total,
  busy,
  error,
  onCancel,
  onConfirm,
  onRescan,
}: {
  cleanup: { count: number; bytes: number; note: string | null } | null;
  privacy: {
    count: number;
    files: number;
    bytes: number;
    note: string | null;
  } | null;
  total: number;
  busy: boolean;
  error: string;
  onCancel: () => void;
  onConfirm: () => void;
  onRescan: () => void;
}) {
  const sources = [cleanup, privacy].filter(Boolean).length;
  return (
    <div
      className="overlay-in fixed inset-0 z-10 flex items-center justify-center bg-black/60 p-6 pl-[256px]"
      role="dialog"
      aria-modal="true"
      aria-labelledby="smart-confirm-title"
    >
      <div className="sheet-in w-full max-w-md rounded-panel border border-border bg-surface3 p-6 shadow-e3">
        <div className="flex items-start gap-3">
          <span className="grid h-9 w-9 flex-none place-items-center rounded-[8px] bg-accentTint text-accentText">
            <ShieldIcon size={18} />
          </span>
          <div>
            <h2 id="smart-confirm-title" className="text-title font-semibold">
              Move{" "}
              <span className="font-mono tabular-nums">
                {formatBytes(total)}
              </span>{" "}
              to the Trash?
            </h2>
            <p className="text-muted mt-1 text-body">
              From {sources} source{sources === 1 ? "" : "s"}, each confirmed on
              its own figures.
            </p>
          </div>
        </div>

        {/* Per source, because the backend checks per source. A combined count
            could not be checked against any single verb's rescan, and inventing
            a combined tolerance would be inventing a looser one. */}
        <ul className="mt-4 space-y-2">
          {cleanup && (
            <SheetLine
              name="Cleanup"
              detail={`${cleanup.count.toLocaleString()} item${cleanup.count === 1 ? "" : "s"}`}
              bytes={cleanup.bytes}
              note={cleanup.note}
            />
          )}
          {privacy && (
            <SheetLine
              name="Browser data"
              detail={`${privacy.count} location${privacy.count === 1 ? "" : "s"} · ${privacy.files.toLocaleString()} file${privacy.files === 1 ? "" : "s"}`}
              bytes={privacy.bytes}
              note={privacy.note}
            />
          )}
        </ul>

        <div className="mt-4 flex gap-2.5 rounded-card border border-success/25 bg-success/[.08] px-3.5 py-3">
          <span className="mt-px flex-none text-success">
            <ShieldIcon size={15} />
          </span>
          <p className="text-muted text-body leading-relaxed">
            This is{" "}
            <strong className="font-semibold text-text">recoverable</strong>.
            Files go to the Trash, and every action is written to the audit log.
            {/* Said here rather than on every row: the sheet is where the
                consent is given, and it is the one claim a person needs before
                letting a single gesture touch browser data. */}
            {privacy
              ? " Nothing here signs you out or erases your history."
              : ""}
          </p>
        </div>

        {sources > 1 && (
          <p className="text-subtle mt-3 text-caption leading-relaxed">
            Sources run one at a time. If one refuses — because the disk changed
            since the scan — nothing after it is attempted, and the result says
            which.
          </p>
        )}

        {error && <p className="text-danger mt-3 text-body">{error}</p>}

        <div className="mt-6 flex justify-end gap-3">
          <button
            onClick={onCancel}
            disabled={busy}
            className="rounded-control border border-border bg-surface2 px-4 py-2 text-body font-medium text-text transition-colors duration-fast ease-mac disabled:opacity-40"
          >
            Cancel
          </button>
          {/* After a refusal the primary action is a fresh scan, not another
              attempt at the same one. Every refusal that reaches here is the
              backend saying these figures no longer describe the disk — so
              re-sending them is the one thing that cannot help, and leaving
              "Move to Trash" under the reason would invite exactly that. */}
          {error ? (
            <button
              onClick={onRescan}
              className="rounded-control bg-accent px-4 py-2 text-body font-semibold text-white"
            >
              Scan again
            </button>
          ) : (
            <button
              onClick={onConfirm}
              disabled={busy}
              className="rounded-control bg-accent px-4 py-2 text-body font-semibold text-white disabled:opacity-60"
            >
              {busy ? "Moving…" : "Move to Trash"}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

function SheetLine({
  name,
  detail,
  bytes,
  note,
}: {
  name: string;
  detail: string;
  bytes: number;
  note: string | null;
}) {
  return (
    <li className="flex items-center gap-3 rounded-card border border-separator bg-surface px-3.5 py-2.5">
      <div className="min-w-0 flex-1">
        <span className="text-body font-medium">{name}</span>
        {/* Not truncated. This line carries the reason a source will ask —
            "includes 3 folders" — and an ellipsis through the middle of the
            reason is the one place on this sheet where clipping costs the
            reader the thing they are being asked to agree to. */}
        <p className="text-subtle mt-0.5 font-mono text-caption leading-snug tabular-nums">
          {detail}
          {note ? ` · ${note}` : ""}
        </p>
      </div>
      <span className="font-mono text-body font-semibold tabular-nums">
        {formatBytes(bytes)}
      </span>
    </li>
  );
}

/**
 * The ledger.
 *
 * Four outcomes, and the third is the reason this is a list rather than a
 * sentence: **"we did not try" must not read like "we tried and there was
 * nothing"**. A run that stopped after a refusal shows the refusal, and shows
 * every step behind it as never attempted, with the reason it was not.
 */
function RunLedger({
  run,
  onAgain,
}: {
  run: SmartScanRunReport;
  onAgain: () => void;
}) {
  const clean = run.completed && run.actions_refused === 0;
  return (
    <section className="rounded-card border border-separator bg-surface p-8">
      <div className="text-center">
        <div
          className={`mx-auto grid h-12 w-12 place-items-center rounded-panel border ${
            clean
              ? "border-success/25 bg-success/10 text-success"
              : "border-cat-trashes/30 bg-cat-trashes/10 text-cat-trashes"
          }`}
        >
          {clean ? <ShieldIcon size={24} /> : <InfoIcon size={24} />}
        </div>
        <p className="mt-4 font-mono text-display font-semibold tabular-nums">
          {formatBytes(run.bytes_freed)}
        </p>
        <p className="text-muted mt-1 text-body">
          moved to the Trash.
          {run.actions_refused > 0
            ? ` ${run.actions_refused.toLocaleString()} item${run.actions_refused === 1 ? " was" : "s were"} skipped.`
            : ""}
        </p>
      </div>

      <ul className="mx-auto mt-6 max-w-md space-y-2">
        {run.steps.map((s) => (
          <StepLine key={s.source} step={s} />
        ))}
      </ul>

      <p className="text-subtle mt-5 text-center text-caption">
        Recorded in the audit log. Recover anything from the Trash if needed.
      </p>
      <div className="mt-5 text-center">
        <button
          onClick={onAgain}
          className="rounded-control border border-border bg-surface2 px-4 py-2 text-body font-medium text-text transition-colors duration-fast ease-mac hover:border-borderStrong"
        >
          Scan again
        </button>
      </div>
    </section>
  );
}

/** What a step moved, in the same units its line on the sheet used. */
function stepFigures(s: CleanSummary): string {
  const acts = `${s.executed.toLocaleString()} location${s.executed === 1 ? "" : "s"}`;
  if (s.entries_freed === 0) {
    return `${s.executed.toLocaleString()} file${s.executed === 1 ? "" : "s"}`;
  }
  return `${acts} · ${s.entries_freed.toLocaleString()} file${s.entries_freed === 1 ? "" : "s"}`;
}

function StepLine({ step }: { step: SmartScanStep }) {
  const name = SOURCE_LABEL[step.source] ?? step.source;
  // Four tones for four outcomes. `not_attempted` is deliberately not the same
  // grey as `not_selected`: one is a consequence of a refusal and the other is
  // a choice the user made.
  const shown =
    step.outcome === "executed"
      ? {
          tone: "text-success",
          mark: "✓",
          // Two numbers, not their sum, and the same two the sheet showed.
          // `executed` counts the *actions* a verb took — a privacy row is one
          // action over a whole folder — so reporting it alone says "3 items"
          // for a step that moved 412 files, and adding them says 415 where
          // the sheet promised 412. Reporting both agrees with the sheet in
          // the only way that is also true.
          text: `${stepFigures(step.summary)} · ${formatBytes(step.summary.bytes_freed)}${step.summary.refused > 0 ? ` · ${step.summary.refused} skipped` : ""}`,
        }
      : step.outcome === "refused"
        ? { tone: "text-danger", mark: "✗", text: step.reason }
        : step.outcome === "not_attempted"
          ? {
              tone: "text-cat-trashes",
              mark: "—",
              text: `Not attempted: ${step.because}`,
            }
          : {
              tone: "text-subtle",
              mark: "·",
              text: "Nothing selected from this source",
            };

  return (
    <li className="flex items-start gap-3 rounded-card border border-separator bg-surface2 px-3.5 py-2.5">
      <span
        className={`mt-px w-3 flex-none text-center font-mono text-body ${shown.tone}`}
        aria-hidden="true"
      >
        {shown.mark}
      </span>
      <div className="min-w-0 flex-1">
        <span className="text-body font-medium">{name}</span>
        <p className="text-muted mt-0.5 text-caption leading-relaxed">
          {shown.text}
        </p>
      </div>
    </li>
  );
}

function ErrorState({
  message,
  inApp,
  onRetry,
}: {
  message: string;
  inApp: boolean;
  onRetry: () => void;
}) {
  return (
    <section className="rounded-card border border-separator bg-surface p-8 text-center">
      <div className="mx-auto grid h-12 w-12 place-items-center rounded-panel border border-danger/25 bg-danger/10 text-danger">
        <InfoIcon size={24} />
      </div>
      <p className="mt-4 text-title font-semibold">
        {inApp ? "Scan couldn’t finish" : "Swept runs as a desktop app"}
      </p>
      <p className="text-muted mx-auto mt-1.5 max-w-md text-body">
        {inApp
          ? message
          : "This page is a preview shell with no access to your disk. Open the Swept app to scan."}
      </p>
      <p className="text-subtle mx-auto mt-3 max-w-md text-caption">
        Nothing was scanned and nothing was changed. No results are shown because
        there are none — Swept never shows sample figures in place of your real
        disk.
      </p>
      {inApp && (
        <button
          onClick={onRetry}
          className="mt-5 rounded-control border border-border bg-surface2 px-4 py-2 text-body font-medium text-text transition-colors duration-fast ease-mac hover:border-borderStrong"
        >
          Try again
        </button>
      )}
    </section>
  );
}
