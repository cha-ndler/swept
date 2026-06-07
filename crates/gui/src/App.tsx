import { useEffect, useMemo, useState } from "react";
import { formatBytes } from "./format";
import type { CategorySummary, ScanReport } from "./types";
import { SAMPLE_REPORT } from "./sample";

type View = "loading" | "results" | "empty" | "error";

/** Deterministic state override for browser preview / Playwright screenshots. */
function previewState(): View | null {
  const s = new URLSearchParams(window.location.search).get("state");
  return s === "loading" || s === "results" || s === "empty" || s === "error"
    ? s
    : null;
}

export default function App() {
  const [view, setView] = useState<View>("loading");
  const [report, setReport] = useState<ScanReport | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [error, setError] = useState<string>("");

  useEffect(() => {
    const ps = previewState();
    if (ps) {
      if (ps === "results") seed(SAMPLE_REPORT);
      else if (ps === "empty") seed({ ...SAMPLE_REPORT, total_count: 0, total_bytes: 0, by_category: [] });
      else if (ps === "error") setError("Couldn't read ~/Library/Caches (permission denied).");
      setView(ps);
      return;
    }
    void (async () => {
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        const r = await invoke<ScanReport>("scan", { filters: {} });
        seed(r);
        setView(r.by_category.length ? "results" : "empty");
      } catch {
        // Browser dev (no Tauri runtime): show sample data so the UI is usable.
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
  const maxBytes = useMemo(
    () => cats.reduce((m, c) => Math.max(m, c.bytes), 1),
    [cats],
  );
  const sel = useMemo(
    () => cats.filter((c) => selected.has(c.category)),
    [cats, selected],
  );
  const selBytes = sel.reduce((s, c) => s + c.bytes, 0);
  const selCount = sel.reduce((s, c) => s + c.count, 0);

  return (
    <div className="min-h-screen bg-bg text-text">
      <main className="mx-auto max-w-3xl px-8 py-7">
        <header>
          <h1 className="text-2xl font-semibold tracking-tight">mac-cleaner</h1>
          <p className="text-muted mt-1 text-sm">
            Safe, dry-run-first cleanup. Nothing is removed without your consent.
          </p>
        </header>

        {view === "loading" && <LoadingSkeleton />}
        {view === "error" && <ErrorState message={error} />}
        {view === "empty" && <EmptyState />}
        {view === "results" && (
          <>
            <Summary bytes={selBytes} count={selCount} canClean={sel.length > 0} />
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
      </main>
    </div>
  );
}

function Summary({ bytes, count, canClean }: { bytes: number; count: number; canClean: boolean }) {
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
