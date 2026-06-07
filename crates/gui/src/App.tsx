import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import CleanView from "./CleanView";
import StartupView from "./StartupView";

type Tab = "clean" | "startup";

export default function App() {
  const [tab, setTab] = useState<Tab>("clean");

  useEffect(() => {
    const t = new URLSearchParams(window.location.search).get("tab");
    if (t === "startup") setTab("startup");
  }, []);

  return (
    <div className="min-h-screen bg-bg text-text">
      <main className="mx-auto max-w-3xl px-8 py-7">
        <header>
          <h1 className="text-2xl font-semibold tracking-tight">mac-cleaner</h1>
          <p className="text-muted mt-1 text-sm">
            Safe, dry-run-first cleanup. Nothing is removed without your consent.
          </p>
        </header>

        <nav className="mt-5 inline-flex gap-1 rounded-xl border border-border bg-surface p-1">
          <TabButton active={tab === "clean"} onClick={() => setTab("clean")}>
            Clean
          </TabButton>
          <TabButton active={tab === "startup"} onClick={() => setTab("startup")}>
            Startup
          </TabButton>
        </nav>

        <div className="mt-4">{tab === "clean" ? <CleanView /> : <StartupView />}</div>
      </main>
    </div>
  );
}

function TabButton({ active, onClick, children }: { active: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button
      onClick={onClick}
      aria-pressed={active}
      className={`rounded-lg px-3 py-1.5 text-sm font-medium transition-colors ${
        active ? "bg-accent text-white" : "text-muted hover:text-text"
      }`}
    >
      {children}
    </button>
  );
}
