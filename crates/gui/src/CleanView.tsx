import { useEffect, useMemo, useState } from "react";
import { formatBytes } from "./format";
import type { CategorySummary, CleanSummary, Filters, ScanReport } from "./types";
import { call, describeError, isDesktopApp } from "./backend";

type View = "loading" | "results" | "empty" | "error";
type Phase = "none" | "confirm" | "cleaning" | "done";

export default function CleanView() {
  const [view, setView] = useState<View>("loading");
  const [report, setReport] = useState<ScanReport | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [error, setError] = useState("");
  const [phase, setPhase] = useState<Phase>("none");
  const [summary, setSummary] = useState<CleanSummary | null>(null);
  const [cleanError, setCleanError] = useState("");
  const [olderDays, setOlderDays] = useState("");
  const [minSize, setMinSize] = useState("");

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
    setView("loading");
    try {
      const r = await call<ScanReport>("scan", { filters });
      seed(r);
      setView(r.by_category.length ? "results" : "empty");
    } catch (e) {
      setReport(null);
      setSelected(new Set());
      setError(describeError(e));
      setView("error");
    }
  }

  useEffect(() => {
    void runScan({});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

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
  const maxBytes = useMemo(() => cats.reduce((m, c) => Math.max(m, c.bytes), 1), [cats]);
  const sel = useMemo(() => cats.filter((c) => selected.has(c.category)), [cats, selected]);
  const selBytes = sel.reduce((s, c) => s + c.bytes, 0);
  const selCount = sel.reduce((s, c) => s + c.count, 0);

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
        filters: { older_than_days: olderDays ? Number(olderDays) : undefined, min_size_bytes: minSize ? Number(minSize) : undefined },
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

  if (phase === "done" && summary) return <DoneCard summary={summary} />;

  return (
    <>
      <FiltersBar
        olderDays={olderDays}
        minSize={minSize}
        onChange={applyFilters}
        disabled={view === "loading"}
      />

      {view === "loading" && <LoadingSkeleton />}
      {view === "error" && (
        <ErrorState
          message={error}
          inApp={isDesktopApp()}
          onRetry={() =>
            void runScan({
              older_than_days: olderDays ? Number(olderDays) : undefined,
              min_size_bytes: minSize ? Number(minSize) : undefined,
            })
          }
        />
      )}
      {view === "empty" && <EmptyState />}
      {view === "results" && (
        <>
          <Summary bytes={selBytes} count={selCount} canClean={sel.length > 0} onClean={() => setPhase("confirm")} />
          <ul className="mt-3 space-y-2">
            {cats.map((c) => (
              <CategoryRow
                key={c.category}
                cat={c}
                checked={selected.has(c.category)}
                pct={(c.bytes / maxBytes) * 100}
                onToggle={() => toggle(c.category)}
              />
            ))}
          </ul>
          {report && report.skipped_protected > 0 && (
            <p className="text-muted mt-4 text-xs">
              {report.skipped_protected} protected item
              {report.skipped_protected === 1 ? "" : "s"} skipped by the safety guard.
            </p>
          )}
        </>
      )}

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
    <div className="flex flex-wrap items-center gap-x-5 gap-y-2 rounded-xl border border-border bg-surface px-4 py-3 text-sm">
      <span className="text-muted">Filters</span>
      <label className="flex items-center gap-2">
        Older than
        <input
          type="number"
          min="0"
          inputMode="numeric"
          value={olderDays}
          disabled={disabled}
          placeholder="any"
          onChange={(e) => onChange(e.target.value, minSize)}
          aria-label="Only files older than this many days"
          className="w-16 rounded-md border border-border bg-bg px-2 py-1 disabled:opacity-50"
        />
        <span className="text-muted">days</span>
      </label>
      <label className="flex items-center gap-2">
        Min size
        <select
          value={minSize}
          disabled={disabled}
          onChange={(e) => onChange(olderDays, e.target.value)}
          aria-label="Minimum file size"
          className="rounded-md border border-border bg-bg px-2 py-1 disabled:opacity-50"
        >
          <option value="">Any</option>
          <option value="104857600">100 MiB</option>
          <option value="524288000">500 MiB</option>
          <option value="1073741824">1 GiB</option>
        </select>
      </label>
    </div>
  );
}

function Summary({ bytes, count, canClean, onClean }: { bytes: number; count: number; canClean: boolean; onClean: () => void }) {
  return (
    <section className="mt-3 rounded-xl border border-border bg-surface p-5">
      <div className="flex items-end justify-between gap-4">
        <div>
          <p className="text-3xl font-semibold tabular-nums">{formatBytes(bytes)}</p>
          <p className="text-muted mt-0.5 text-sm">
            reclaimable from {count.toLocaleString()} selected item{count === 1 ? "" : "s"} · preview only
          </p>
        </div>
        <button
          onClick={onClean}
          disabled={!canClean}
          className="shrink-0 rounded-xl bg-accent px-4 py-2 text-sm font-medium text-white transition-opacity disabled:opacity-40"
        >
          Review &amp; Clean…
        </button>
      </div>
    </section>
  );
}

function CategoryRow({ cat, checked, pct, onToggle }: { cat: CategorySummary; checked: boolean; pct: number; onToggle: () => void }) {
  return (
    <li>
      <label className="flex cursor-pointer items-center gap-3 rounded-xl border border-border bg-surface px-4 py-3 hover:border-muted">
        <input type="checkbox" checked={checked} onChange={onToggle} aria-label={`Select ${cat.name}`} className="h-4 w-4 accent-accent" />
        <div className="min-w-0 flex-1">
          <div className="flex items-baseline justify-between gap-3">
            <span className="truncate font-medium">{cat.name}</span>
            <span className="tabular-nums text-sm">{formatBytes(cat.bytes)}</span>
          </div>
          <div className="mt-1.5 h-1.5 overflow-hidden rounded bg-bg" aria-hidden="true">
            <div className="h-full rounded bg-accent" style={{ width: `${pct}%` }} />
          </div>
          <div className="text-muted mt-1.5 flex justify-between gap-3 text-xs">
            <span className="truncate">{cat.description}</span>
            <span className="tabular-nums shrink-0">
              {cat.count.toLocaleString()} item{cat.count === 1 ? "" : "s"}
            </span>
          </div>
        </div>
      </label>
    </li>
  );
}

function ConfirmModal({ count, bytes, categories, busy, error, onCancel, onConfirm }: { count: number; bytes: number; categories: number; busy: boolean; error: string; onCancel: () => void; onConfirm: () => void }) {
  return (
    <div className="fixed inset-0 z-10 flex items-center justify-center bg-black/60 p-6" role="dialog" aria-modal="true" aria-labelledby="confirm-title">
      <div className="w-full max-w-md rounded-xl border border-border bg-surface p-6 shadow-2xl">
        <h2 id="confirm-title" className="text-lg font-semibold">
          Move {count.toLocaleString()} item{count === 1 ? "" : "s"} to the Trash?
        </h2>
        <p className="text-muted mt-2 text-sm">
          {formatBytes(bytes)} across {categories} categor{categories === 1 ? "y" : "ies"} will be moved to the Trash. This is{" "}
          <strong className="text-text">recoverable</strong> — nothing is deleted permanently, and every action is recorded in the audit log.
        </p>
        {error && <p className="text-danger mt-3 text-sm">{error}</p>}
        <div className="mt-6 flex justify-end gap-3">
          <button onClick={onCancel} disabled={busy} className="rounded-xl border border-border px-4 py-2 text-sm font-medium text-text disabled:opacity-40">
            Cancel
          </button>
          <button onClick={onConfirm} disabled={busy} className="rounded-xl bg-accent px-4 py-2 text-sm font-medium text-white disabled:opacity-60">
            {busy ? "Moving…" : "Move to Trash"}
          </button>
        </div>
      </div>
    </div>
  );
}

function DoneCard({ summary }: { summary: CleanSummary }) {
  return (
    <section className="mt-3 rounded-xl border border-border bg-surface p-10 text-center">
      <p className="text-lg font-medium">Cleanup complete</p>
      <p className="text-muted mt-1 text-sm">
        Moved {summary.executed.toLocaleString()} item{summary.executed === 1 ? "" : "s"} ({formatBytes(summary.bytes_freed)}) to the Trash.
        {summary.refused > 0 ? ` ${summary.refused} skipped.` : ""}
      </p>
      <p className="text-muted mt-1 text-xs">Recover anything from the Trash if needed.</p>
    </section>
  );
}

function LoadingSkeleton() {
  return (
    <div className="mt-3 animate-pulse space-y-2" role="status" aria-busy="true" aria-label="Scanning">
      <div className="h-[92px] rounded-xl border border-border bg-surface" />
      {[0, 1, 2, 3].map((i) => (
        <div key={i} className="h-[68px] rounded-xl border border-border bg-surface" />
      ))}
    </div>
  );
}

function EmptyState() {
  return (
    <section className="mt-3 rounded-xl border border-border bg-surface p-10 text-center">
      <p className="text-lg font-medium">Nothing to clean</p>
      <p className="text-muted mt-1 text-sm">No safe-to-remove junk was found. Your Mac is tidy. ✨</p>
    </section>
  );
}

function ErrorState({ message, inApp, onRetry }: { message: string; inApp: boolean; onRetry: () => void }) {
  return (
    <section className="mt-3 rounded-xl border border-border bg-surface p-8 text-center">
      <p className="text-lg font-medium">
        {inApp ? "Scan couldn’t finish" : "mac-cleaner runs as a desktop app"}
      </p>
      <p className="text-muted mx-auto mt-1 max-w-md text-sm">
        {inApp
          ? message
          : "This page is a preview shell with no access to your disk. Open the mac-cleaner app to scan."}
      </p>
      <p className="text-muted mt-3 text-xs">
        Nothing was scanned and nothing was changed. No results are shown because there are none —
        mac-cleaner never shows sample figures in place of your real disk.
      </p>
      {inApp && (
        <button
          onClick={onRetry}
          className="mt-5 rounded-xl border border-border px-4 py-2 text-sm font-medium text-text hover:border-muted"
        >
          Try again
        </button>
      )}
    </section>
  );
}
