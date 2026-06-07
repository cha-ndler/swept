import { useState } from "react";

/** Mirrors the relevant fields of macclean_core::report::ScanReport. */
interface ScanReport {
  total_count: number;
  total_bytes: number;
}

function formatBytes(bytes: number): string {
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let size = bytes;
  let unit = 0;
  while (size >= 1024 && unit < units.length - 1) {
    size /= 1024;
    unit += 1;
  }
  return unit === 0 ? `${bytes} B` : `${size.toFixed(1)} ${units[unit]}`;
}

export default function App() {
  const [report, setReport] = useState<ScanReport | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function runScan() {
    setBusy(true);
    setNote(null);
    try {
      // Imported lazily so a plain `vite build` (no Tauri runtime) still works.
      const { invoke } = await import("@tauri-apps/api/core");
      const r = await invoke<ScanReport>("scan", { filters: {} });
      setReport(r);
    } catch (e) {
      setNote(`Run inside the mac-cleaner app to scan. (${String(e)})`);
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="min-h-screen bg-bg text-text px-8 py-7">
      <header>
        <h1 className="text-2xl font-semibold tracking-tight">mac-cleaner</h1>
        <p className="text-muted mt-1 text-sm">
          Safe, dry-run-first cleanup. Nothing is removed without your consent.
        </p>
      </header>

      <button
        onClick={runScan}
        disabled={busy}
        className="mt-6 rounded-xl bg-accent px-4 py-2 text-sm font-medium text-white disabled:opacity-50"
      >
        {busy ? "Scanning…" : "Scan"}
      </button>

      {report && (
        <section className="mt-6 rounded-xl border border-border bg-surface p-5">
          <p className="text-lg font-medium">
            {report.total_count} item{report.total_count === 1 ? "" : "s"}
          </p>
          <p className="text-muted text-sm">
            {formatBytes(report.total_bytes)} reclaimable (preview only)
          </p>
        </section>
      )}

      {note && <p className="text-muted mt-4 text-sm">{note}</p>}
    </main>
  );
}
