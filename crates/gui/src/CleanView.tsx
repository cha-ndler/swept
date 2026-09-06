import { useEffect, useMemo, useState } from "react";
import { formatBytes } from "./format";
import type {
  CleanSummary,
  Filters,
  Permissions,
  ScanReport,
} from "./types";
import { call, describeError, isDesktopApp, onScanProgress } from "./backend";
import type { ScanProgress } from "./backend";
import {
  AccessNotice,
  Banner,
  Group,
  InfoIcon,
  LockIcon,
  ShieldIcon,
  Toolbar,
} from "./Shell";
import { hue } from "./hues";
import { NumberField, Segmented } from "./Controls";
import { CategoryRow } from "./CategoryRow";
import { ScanRing } from "./ScanRing";
import type { RingSegment } from "./ScanRing";

type View = "loading" | "results" | "empty" | "error";
type Phase = "none" | "confirm" | "cleaning" | "done";

const SIZE_FILTERS = [
  { value: "", label: "Any" },
  { value: "104857600", label: "100 MB" },
  { value: "524288000", label: "500 MB" },
  { value: "1073741824", label: "1 GB" },
];

export default function CleanView({
  onReclaimable,
}: {
  /** Reports the scan total so the sidebar badge can show it. */
  /**
   * The reclaimable figure **as a string**, not a number to re-format.
   *
   * Same reason the menu-bar label is a string: the sidebar, the window and the
   * menu bar must not be able to drift apart, and a figure that is a floor has
   * to be able to say so. A number cannot carry "at least".
   */
  onReclaimable?: (label: string | null) => void;
}) {
  const [view, setView] = useState<View>("loading");
  const [report, setReport] = useState<ScanReport | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [error, setError] = useState("");
  const [phase, setPhase] = useState<Phase>("none");
  const [summary, setSummary] = useState<CleanSummary | null>(null);
  const [cleanError, setCleanError] = useState("");
  const [olderDays, setOlderDays] = useState("");
  const [minSize, setMinSize] = useState("");
  const [progress, setProgress] = useState<ScanProgress | null>(null);
  const [perms, setPerms] = useState<Permissions | null>(null);

  // Advisory, and deliberately silent on failure: an unavailable probe must not
  // turn into an error state, because the scan itself is unaffected.
  useEffect(() => {
    void call<Permissions>("permissions")
      .then(setPerms)
      .catch(() => setPerms(null));
  }, []);

  // The backend emits cumulative progress while it walks. Subscribe once for
  // the lifetime of the view; `runScan` clears the last reading when it starts.
  useEffect(() => {
    let stop: (() => void) | undefined;
    let cancelled = false;
    void onScanProgress((p) => setProgress(p)).then((off) => {
      if (cancelled) off();
      else stop = off;
    });
    return () => {
      cancelled = true;
      stop?.();
    };
  }, []);

  function seed(r: ScanReport) {
    setReport(r);
    setSelected(new Set(r.by_category.map((c) => c.category)));
  }

  // A scan either succeeds or reports why it failed. It must never fall back to
  // canned data: the fixture category ids are the real ones, so showing them
  // after a failure would invite the user to act on numbers that describe no
  // real disk. See ./backend.ts.
  async function runScan(filters: Filters) {
    // Close any open confirmation up front, for every re-scan — not just failed
    // ones. The sheet describes one specific report. While a re-scan is in
    // flight it would still be showing the previous report's numbers even
    // though `filters` has already changed, so confirming it would apply the
    // user's consent to a set they never saw.
    setPhase("none");
    setProgress(null);
    setView("loading");
    try {
      const r = await call<ScanReport>("scan", { filters });
      seed(r);
      // One string, shown in three places: the sidebar badge, the ring's
      // neighbours and the menu bar. None of the other two has room for a
      // caveat, so the figure carries it — a bare number in either would be the
      // unqualified claim the window itself stopped making. Best-effort on the
      // tray: a scan that succeeded must not surface an error because a label
      // failed to update.
      const label = r.partial
        ? `≥ ${formatBytes(r.total_bytes)}`
        : formatBytes(r.total_bytes);
      onReclaimable?.(label);
      void call("set_tray_label", { label }).catch(() => {});
      setView(r.by_category.length ? "results" : "empty");
    } catch (e) {
      setReport(null);
      setSelected(new Set());
      onReclaimable?.(null);
      // No honest figure to show, so the menu bar shows nothing at all rather
      // than the previous scan's number.
      void call("set_tray_label", { label: null }).catch(() => {});
      setError(describeError(e));
      setView("error");
    }
  }

  useEffect(() => {
    void runScan({});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  function currentFilters(): Filters {
    return {
      older_than_days: olderDays ? Number(olderDays) : undefined,
      min_size_bytes: minSize ? Number(minSize) : undefined,
    };
  }

  function applyFilters(nextOlder: string, nextSize: string) {
    setOlderDays(nextOlder);
    setMinSize(nextSize);
    void runScan({
      older_than_days: nextOlder ? Number(nextOlder) : undefined,
      min_size_bytes: nextSize ? Number(nextSize) : undefined,
    });
  }

  function toggle(id: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  const cats = report?.by_category ?? [];
  const sel = useMemo(
    () => cats.filter((c) => selected.has(c.category)),
    [cats, selected],
  );
  const selBytes = sel.reduce((s, c) => s + c.bytes, 0);
  const selCount = sel.reduce((s, c) => s + c.count, 0);
  const segments: RingSegment[] = sel.map((c) => ({
    id: c.category,
    bytes: c.bytes,
    color: hue(c.category),
  }));

  async function runClean() {
    // Defence in depth: never send a clean for an empty selection. The backend
    // now refuses this too, but the request should not be made at all.
    if (sel.length === 0) {
      setPhase("none");
      return;
    }
    setCleanError("");
    setPhase("cleaning");
    try {
      const s = await call<CleanSummary>("clean", {
        filters: currentFilters(),
        categories: Array.from(selected),
        // Bind the consent to what the sheet actually showed. The backend
        // rebuilds the plan (the disk may have changed since the scan), so it
        // needs both: the mass-delete acknowledgement — derived from the
        // displayed figures rather than pre-satisfied, thresholds per
        // core/src/plan.rs — and the magnitude those figures represented, so a
        // materially larger plan is refused instead of executed.
        expected: { count: selCount, bytes: selBytes },
        confirmMassDelete: selCount > 100 || selBytes > 5 * 1024 ** 3,
      });
      setSummary(s);
      setPhase("done");
    } catch (e) {
      setCleanError(describeError(e));
      setPhase("confirm");
    }
  }

  const done = phase === "done" && summary !== null;

  return (
    <>
      <Toolbar title="Cleanup">
        {!done && (
          <FiltersBar
            olderDays={olderDays}
            minSize={minSize}
            onChange={applyFilters}
            disabled={view === "loading"}
          />
        )}
      </Toolbar>

      <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">
        {done && summary ? (
          <DoneCard
            summary={summary}
            onBack={() => {
              setPhase("none");
              setSummary(null);
              void runScan(currentFilters());
            }}
          />
        ) : (
          <>
            {view === "loading" && <Scanning progress={progress} />}
            {view === "error" && (
              <ErrorState
                message={error}
                inApp={isDesktopApp()}
                onRetry={() => void runScan(currentFilters())}
              />
            )}
            {(view === "results" || view === "empty") &&
              perms &&
              !perms.all_readable && <AccessNotice perms={perms} />}
            {(view === "results" || view === "empty") && report?.partial && (
              <FloorNotice
                report={report}
                alongsideAccessNotice={!(perms?.all_readable ?? true)}
              />
            )}
            {view === "empty" && (
              <EmptyState
                onRescan={() => void runScan(currentFilters())}
                report={report}
              />
            )}
            {view === "results" && (
              <div className="flex flex-col gap-7 md:flex-row md:items-start">
                <div className="flex flex-none flex-col items-center text-center md:w-[240px]">
                  <ScanRing
                    segments={segments}
                    total={selBytes}
                    caption={
                      report?.partial ? "reclaimable, at least" : "reclaimable"
                    }
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
                    disabled={sel.length === 0}
                    className="mt-4 w-full rounded-control bg-accent px-4 py-2 text-emph font-semibold text-white transition-opacity duration-fast ease-mac disabled:opacity-40"
                  >
                    Review &amp; Clean…
                  </button>
                  <p className="text-subtle mt-2.5 font-mono text-caption tabular-nums">
                    {selCount.toLocaleString()} items in {sel.length} categor
                    {sel.length === 1 ? "y" : "ies"}
                  </p>
                </div>

                <div className="min-w-0 flex-1">
                  {/* One card, hairline-separated. Smart Scan draws the same
                      rows, and Space Lens, Privacy and Large & Old were already
                      using `Group`; this screen and its own list were the last
                      place in the app where every row carried its own border. */}
                  <Group role="list" label="Categories">
                    {cats.map((c) => (
                      <CategoryRow
                        key={c.category}
                        cat={c}
                        checked={selected.has(c.category)}
                        onToggle={() => toggle(c.category)}
                      />
                    ))}
                  </Group>
                  {report && report.skipped_protected > 0 && (
                    <div className="mt-4">
                      <Banner icon={<LockIcon size={15} />}>
                        <strong className="font-semibold text-text">
                          {report.skipped_protected} protected item
                          {report.skipped_protected === 1 ? "" : "s"} skipped
                        </strong>{" "}
                        by the safety guard — Keychains, Mail and repositories
                        are never eligible.
                      </Banner>
                    </div>
                  )}
                </div>
              </div>
            )}
          </>
        )}
      </div>

      {(phase === "confirm" || phase === "cleaning") && (
        <ConfirmModal
          count={selCount}
          bytes={selBytes}
          categories={sel.length}
          busy={phase === "cleaning"}
          error={cleanError}
          onCancel={() => setPhase("none")}
          onConfirm={runClean}
        />
      )}
    </>
  );
}

function FiltersBar({
  olderDays,
  minSize,
  onChange,
  disabled,
}: {
  olderDays: string;
  minSize: string;
  onChange: (older: string, size: string) => void;
  disabled: boolean;
}) {
  return (
    <div className="flex items-center gap-3">
      <span className="text-muted hidden text-caption lg:inline">
        Older than
      </span>
      <NumberField
        value={olderDays}
        onChange={(v) => onChange(v, minSize)}
        label="Only files older than this many days"
        placeholder="any"
        disabled={disabled}
        suffix="days"
      />
      <Segmented
        label="Minimum file size"
        value={minSize}
        options={SIZE_FILTERS}
        disabled={disabled}
        onChange={(v) => onChange(olderDays, v)}
      />
    </div>
  );
}

/**
 * macOS gates ~/.Trash and ~/Library/Containers behind Full Disk Access. Without
 * it a scan still runs — it just cannot see inside them, so the total is smaller
 * than the truth. Under-reporting is the same class of problem as showing
 * fixture data: a number the user trusts that does not describe their disk. So
 * this says so, on every scan it applies to, rather than once in a first-run
 * screen the user has already clicked past.
 */
function ConfirmModal({
  count,
  bytes,
  categories,
  busy,
  error,
  onCancel,
  onConfirm,
}: {
  count: number;
  bytes: number;
  categories: number;
  busy: boolean;
  error: string;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  return (
    <div
      className="overlay-in fixed inset-0 z-10 flex items-center justify-center bg-black/60 p-6 pl-[256px]"
      role="dialog"
      aria-modal="true"
      aria-labelledby="confirm-title"
    >
      <div className="sheet-in w-full max-w-md rounded-panel border border-border bg-surface3 p-6 shadow-e3">
        <div className="flex items-start gap-3">
          <span className="grid h-9 w-9 flex-none place-items-center rounded-[8px] bg-accentTint text-accentText">
            <ShieldIcon size={18} />
          </span>
          <div>
            <h2 id="confirm-title" className="text-title font-semibold">
              Move {count.toLocaleString()} item{count === 1 ? "" : "s"} to the
              Trash?
            </h2>
            <p className="text-muted mt-1 text-body">
              <span className="font-mono font-semibold tabular-nums text-text">
                {formatBytes(bytes)}
              </span>{" "}
              across {categories} categor{categories === 1 ? "y" : "ies"}.
            </p>
          </div>
        </div>

        <div className="mt-4 flex gap-2.5 rounded-card border border-success/25 bg-success/[.08] px-3.5 py-3">
          <span className="mt-px flex-none text-success">
            <ShieldIcon size={15} />
          </span>
          <p className="text-muted text-body leading-relaxed">
            This is{" "}
            <strong className="font-semibold text-text">recoverable</strong>.
            Files go to the Trash, and every action is written to the audit log.
          </p>
        </div>

        {error && <p className="text-danger mt-3 text-body">{error}</p>}

        <div className="mt-6 flex justify-end gap-3">
          <button
            onClick={onCancel}
            disabled={busy}
            className="rounded-control border border-border bg-surface2 px-4 py-2 text-body font-medium text-text transition-colors duration-fast ease-mac disabled:opacity-40"
          >
            Cancel
          </button>
          <button
            onClick={onConfirm}
            disabled={busy}
            className="rounded-control bg-accent px-4 py-2 text-body font-semibold text-white disabled:opacity-60"
          >
            {busy ? "Moving…" : "Move to Trash"}
          </button>
        </div>
      </div>
    </div>
  );
}

function StatusIcon({
  tone,
  children,
}: {
  tone: "success" | "danger" | "withheld";
  children: React.ReactNode;
}) {
  // `withheld` is neither good news nor an error: the scan worked, it just could
  // not see everything. It reuses the hue this app already uses for "shown, not
  // acted on" rather than inventing a fourth vocabulary.
  const cls =
    tone === "success"
      ? "border-success/25 bg-success/10 text-success"
      : tone === "withheld"
        ? "border-cat-trashes/30 bg-cat-trashes/10 text-cat-trashes"
        : "border-danger/25 bg-danger/10 text-danger";
  return (
    <div
      className={`mx-auto grid h-12 w-12 place-items-center rounded-panel border ${cls}`}
    >
      {children}
    </div>
  );
}

function DoneCard({
  summary,
  onBack,
}: {
  summary: CleanSummary;
  onBack: () => void;
}) {
  return (
    <section className="rounded-card border border-separator bg-surface p-10 text-center">
      <StatusIcon tone="success">
        <ShieldIcon size={24} />
      </StatusIcon>
      <p className="mt-4 font-mono text-display font-semibold tabular-nums">
        {formatBytes(summary.bytes_freed)}
      </p>
      <p className="text-muted mt-1 text-body">
        moved to the Trash from {summary.executed.toLocaleString()} item
        {summary.executed === 1 ? "" : "s"}.
        {summary.refused > 0 ? ` ${summary.refused} skipped.` : ""}
      </p>
      <p className="text-subtle mt-2 text-caption">
        Recorded in the audit log. Recover anything from the Trash if needed.
      </p>
      <button
        onClick={onBack}
        className="mt-5 rounded-control border border-border bg-surface2 px-4 py-2 text-body font-medium text-text transition-colors duration-fast ease-mac hover:border-borderStrong"
      >
        Back to Cleanup
      </button>
    </section>
  );
}

function Scanning({ progress }: { progress: ScanProgress | null }) {
  // A scan has no knowable total until it finishes, so there is no honest
  // percentage to show — the ring sweeps rather than filling. The counts are
  // real and move immediately, which is what actually tells the user it is
  // working.
  const examined = progress?.examined ?? 0;
  const found = progress?.bytes ?? 0;
  return (
    <section
      className="flex flex-col items-center py-6 text-center"
      role="status"
      aria-busy="true"
      aria-live="polite"
      aria-label="Scanning"
    >
      <ScanRing segments={[]} total={found} caption="so far" busy />
      <p className="mt-6 text-title font-semibold">Scanning…</p>
      <p className="text-muted mt-1.5 font-mono text-body tabular-nums">
        {examined === 0
          ? "Looking through your caches, logs and build artifacts."
          : `${examined.toLocaleString()} files examined`}
      </p>
      <p className="text-subtle mt-4 text-caption">
        Read-only. Nothing is changed by a scan.
      </p>
    </section>
  );
}

/**
 * The figure above is a floor, and this says by how much.
 *
 * Complements `AccessNotice` rather than repeating it: that one fires when the
 * *permission probe* says a gated root is unreadable and offers the way to fix
 * it, while this one fires when the *walk itself* could not see somewhere the
 * probe knows nothing about — an ordinary unreadable directory, a path that
 * would not resolve. Showing both would say the same thing twice, so this one
 * stands down when permissions already explain it.
 */
function FloorNotice({
  report,
  alongsideAccessNotice,
}: {
  report: ScanReport;
  alongsideAccessNotice: boolean;
}) {
  const places = report.skipped_unreadable;
  const many = places === 1 ? "place" : "places";
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
        <p className="text-muted mt-1 text-body">
          {alongsideAccessNotice
            ? // It complements the notice above rather than standing down.
              // Standing down assumed the permission probe accounted for every
              // skipped place, and it only knows about the Trash and sandboxed
              // caches — so an ordinary unreadable directory would have gone
              // unmentioned while the caption still said "at least".
              `Counting the above, the scan could not look into ${places.toLocaleString()} ${many} in total.`
            : `${places.toLocaleString()} ${many} could not be looked into, so there may be more than the figure shows.`}{" "}
          What is shown is real; it is the completeness that is in doubt.
        </p>
      </div>
    </div>
  );
}

/**
 * Nothing was found — which is only good news if the scan could see everything.
 *
 * `partial` means the walk could not open somewhere: a cleaner root behind Full
 * Disk Access (`~/.Trash` is both), a directory it could not read, a path it
 * could not resolve. Showing a green shield and "your Mac is tidy" over that is
 * the one claim this app must never make, and it is the claim it was making.
 */
function EmptyState({
  onRescan,
  report,
}: {
  onRescan: () => void;
  report: ScanReport | null;
}) {
  const blind = report?.partial ?? false;
  const places = report?.skipped_unreadable ?? 0;
  // Info, not a padlock. `partial` has causes that are not permission — a
  // directory that would not open, a path that would not resolve — and a
  // padlock promises a remedy (grant Full Disk Access) that would not help
  // those. The padlock stays with `AccessNotice`, which only fires when the
  // permission probe really is the reason and can offer the button.

  return (
    <section className="rounded-card border border-separator bg-surface p-10 text-center">
      <StatusIcon tone={blind ? "withheld" : "success"}>
        {blind ? <InfoIcon size={24} /> : <ShieldIcon size={24} />}
      </StatusIcon>
      <h2 className="mt-4 text-title font-semibold">
        {blind ? "Nothing found in what could be read" : "Nothing to clean"}
      </h2>
      <p className="text-muted mt-1 text-body">
        {blind
          ? `${places.toLocaleString()} place${
              places === 1 ? "" : "s"
            } could not be looked into, so this is not the same as a clean Mac.`
          : "No safe-to-remove junk was found. Your Mac is tidy."}
      </p>
      <button
        onClick={onRescan}
        className="mt-5 rounded-control border border-border bg-surface2 px-4 py-2 text-body font-medium text-text transition-colors duration-fast ease-mac hover:border-borderStrong"
      >
        Scan again
      </button>
    </section>
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
      <StatusIcon tone="danger">
        <InfoIcon size={24} />
      </StatusIcon>
      <p className="mt-4 text-title font-semibold">
        {inApp ? "Scan couldn’t finish" : "Swept runs as a desktop app"}
      </p>
      <p className="text-muted mx-auto mt-1.5 max-w-md text-body">
        {inApp
          ? message
          : "This page is a preview shell with no access to your disk. Open the Swept app to scan."}
      </p>
      <p className="text-subtle mx-auto mt-3 max-w-md text-caption">
        Nothing was scanned and nothing was changed. No results are shown
        because there are none — Swept never shows sample figures in place
        of your real disk.
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
