import { useEffect, useMemo, useState } from "react";
import { formatBytes } from "./format";
import type { CategorySummary, CleanSummary, ScanReport } from "./types";
import { SAMPLE_REPORT } from "./sample";

type View = "loading" | "results" | "empty" | "error";
type Phase = "none" | "confirm" | "cleaning" | "done";

/** Deterministic state override for browser preview / Playwright screenshots. */
function previewState(): string | null {
  return new URLSearchParams(window.location.search).get("state");
}

const SAMPLE_SUMMARY: CleanSummary = {
  dry_run: false,
  executed: 4213,
  refused: 0,
  bytes_freed: Math.round(6.44 * 1024 * 1024 * 1024),
};

export default function App() {
  const [view, setView] = useState<View>("loading");
  const [report, setReport] = useState<ScanReport | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [error, setError] = useState<string>("");
  const [phase, setPhase] = useState<Phase>("none");
  const [summary, setSummary] = useState<CleanSummary | null>(null);
  const [cleanError, setCleanError] = useState<string>("");

  useEffect(() => {
    const ps = previewState();
    if (ps) {
      if (ps === "empty") {
        seed({ ...SAMPLE_REPORT, total_count: 0, total_bytes: 0, by_category: [] });
        setView("empty");
      } else if (ps === "error") {
        setError("Couldn’t read ~/Library/Caches (permission denied).");
        setView("error");
      } else if (ps === "loading") {
        setView("loading");
      } else {
        // results / confirm / done all render the results view first.
        seed(SAMPLE_REPORT);
        setView("results");
        if (ps === "confirm") setPhase("confirm");
        if (ps === "done") {
          setSummary(SAMPLE_SUMMARY);
          setPhase("done");
        }
      }
      return;
    }
    void (async () => {
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        const r = await invoke<ScanReport>("scan", { filters: {} });
        seed(r);
        setView(r.by_category.length ? "results" : "empty");
      } catch {
        seed(SAMPLE_REPORT);
        setView("results");
      }
    })();

    function seed(r: ScanReport) {
      setReport(r);
      setSelected(new Set(r.by_category.map((c) => c.category)));
    }
  }, []);

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
    setCleanError("");
    setPhase("cleaning");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const s = await invoke<CleanSummary>("clean", {
        filters: {},
        categories: Array.from(selected),
        confirmMassDelete: true,
      });
      setSummary(s);
      setPhase("done");
    } catch (e) {
      setCleanError(String(e));
      setPhase("confirm");
    }
  }

  if (phase === "done" && summary) {
    return (
      <Shell>
        <DoneCard summary={summary} />
      </Shell>
    );
  }

  return (
    <Shell>
      {view === "loading" && <LoadingSkeleton />}
      {view === "error" && <ErrorState message={error} />}
      {view === "empty" && <EmptyState />}
      {view === "results" && (
        <>
          <Summary
            bytes={selBytes}
            count={selCount}
            canClean={sel.length > 0}
            onClean={() => setPhase("confirm")}
          />
          <ul className="mt-4 space-y-2">
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
    </Shell>
  );
}

function Shell({ children }: { children: React.ReactNode }) {
  return (
    <div className="min-h-screen bg-bg text-text">
      <main className="mx-auto max-w-3xl px-8 py-7">
        <header>
          <h1 className="text-2xl font-semibold tracking-tight">mac-cleaner</h1>
          <p className="text-muted mt-1 text-sm">
            Safe, dry-run-first cleanup. Nothing is removed without your consent.
          </p>
        </header>
        {children}
      </main>
    </div>
  );
}

function Summary({
  bytes,
  count,
  canClean,
  onClean,
}: {
  bytes: number;
  count: number;
  canClean: boolean;
  onClean: () => void;
}) {
  return (
    <section className="mt-6 rounded-xl border border-border bg-surface p-5">
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

function CategoryRow({
  cat,
  checked,
  pct,
  onToggle,
}: {
  cat: CategorySummary;
  checked: boolean;
  pct: number;
  onToggle: () => void;
}) {
  return (
    <li>
      <label className="flex cursor-pointer items-center gap-3 rounded-xl border border-border bg-surface px-4 py-3 hover:border-muted">
        <input
          type="checkbox"
          checked={checked}
          onChange={onToggle}
          aria-label={`Select ${cat.name}`}
          className="h-4 w-4 accent-accent"
        />
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
      className="fixed inset-0 z-10 flex items-center justify-center bg-black/60 p-6"
      role="dialog"
      aria-modal="true"
      aria-labelledby="confirm-title"
    >
      <div className="w-full max-w-md rounded-xl border border-border bg-surface p-6 shadow-2xl">
        <h2 id="confirm-title" className="text-lg font-semibold">
          Move {count.toLocaleString()} item{count === 1 ? "" : "s"} to the Trash?
        </h2>
        <p className="text-muted mt-2 text-sm">
          {formatBytes(bytes)} across {categories} categor{categories === 1 ? "y" : "ies"} will be
          moved to the Trash. This is <strong className="text-text">recoverable</strong> — nothing
          is deleted permanently, and every action is recorded in the audit log.
        </p>
        {error && <p className="text-danger mt-3 text-sm">{error}</p>}
        <div className="mt-6 flex justify-end gap-3">
          <button
            onClick={onCancel}
            disabled={busy}
            className="rounded-xl border border-border px-4 py-2 text-sm font-medium text-text disabled:opacity-40"
          >
            Cancel
          </button>
          <button
            onClick={onConfirm}
            disabled={busy}
            className="rounded-xl bg-accent px-4 py-2 text-sm font-medium text-white disabled:opacity-60"
          >
            {busy ? "Moving…" : "Move to Trash"}
          </button>
        </div>
      </div>
    </div>
  );
}

function DoneCard({ summary }: { summary: CleanSummary }) {
  return (
    <section className="mt-10 rounded-xl border border-border bg-surface p-10 text-center">
      <p className="text-lg font-medium">Cleanup complete</p>
      <p className="text-muted mt-1 text-sm">
        Moved {summary.executed.toLocaleString()} item{summary.executed === 1 ? "" : "s"} (
        {formatBytes(summary.bytes_freed)}) to the Trash.
        {summary.refused > 0 ? ` ${summary.refused} skipped.` : ""}
      </p>
      <p className="text-muted mt-1 text-xs">Recover anything from the Trash if needed.</p>
    </section>
  );
}

function LoadingSkeleton() {
  return (
    <div
      className="mt-6 animate-pulse space-y-2"
      role="status"
      aria-busy="true"
      aria-label="Scanning"
    >
      <div className="h-[92px] rounded-xl border border-border bg-surface" />
      {[0, 1, 2, 3].map((i) => (
        <div key={i} className="h-[68px] rounded-xl border border-border bg-surface" />
      ))}
    </div>
  );
}

function EmptyState() {
  return (
    <section className="mt-10 rounded-xl border border-border bg-surface p-10 text-center">
      <p className="text-lg font-medium">Nothing to clean</p>
      <p className="text-muted mt-1 text-sm">
        No safe-to-remove junk was found. Your Mac is tidy. ✨
      </p>
    </section>
  );
}

function ErrorState({ message }: { message: string }) {
  return (
    <section className="mt-10 rounded-xl border border-border bg-surface p-8 text-center">
      <p className="text-lg font-medium">Scan couldn’t finish</p>
      <p className="text-muted mt-1 text-sm">{message}</p>
    </section>
  );
}
