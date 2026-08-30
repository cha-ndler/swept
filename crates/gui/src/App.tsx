import { useState } from "react";
import type { ReactNode } from "react";
import CleanView from "./CleanView";
import LargeOldView from "./LargeOldView";
import SpaceLensView from "./SpaceLensView";
import StartupView from "./StartupView";
import { FilesIcon, LensIcon, ShieldIcon, StackIcon, WrenchIcon } from "./Shell";
import { formatBytes } from "./format";

type Module = "cleanup" | "large-old" | "space-lens" | "startup";

/**
 * The app shell: a persistent module sidebar beside a content pane.
 *
 * Three things this fixes over the v0.2 tab row. The traffic lights are inset
 * over the sidebar (`titleBarStyle: "Overlay"`), so the window has no separate
 * title bar taking a strip of height. A module is **mounted on first visit and
 * kept alive** after that — switching tabs used to unmount the view, discarding
 * its state and re-running the entire scan, which on a real home is a ~9 s round
 * trip to see a list you had already loaded. And the starting module is derived
 * during the first render rather than in an effect, so `?tab=startup` no longer
 * mounts Cleanup, fires a full scan, and then throws it away.
 *
 * The sidebar lists only modules that actually exist. `design/references/`
 * shows seven; the other five are the v0.5 backlog, and showing them now as
 * dead rows would promise a capability that isn't there — the same class of
 * dishonesty as the sample-data fallback removed in v0.3.
 */
const MODULES = ["cleanup", "large-old", "space-lens", "startup"] as const;

function initialModule(): Module {
  const tab = new URLSearchParams(window.location.search).get("tab");
  return MODULES.includes(tab as Module) ? (tab as Module) : "cleanup";
}

export default function App() {
  const [active, setActive] = useState<Module>(initialModule);
  // Mount-on-first-visit, then keep alive. Mounting every module up front would
  // run backend work the user never asked for.
  const [visited, setVisited] = useState<Set<Module>>(
    () => new Set([initialModule()]),
  );
  const [reclaimable, setReclaimable] = useState<number | null>(null);
  const [largeOldBytes, setLargeOldBytes] = useState<number | null>(null);
  const [measuredBytes, setMeasuredBytes] = useState<number | null>(null);
  const [loginCount, setLoginCount] = useState<number | null>(null);

  function open(m: Module) {
    setActive(m);
    setVisited((v) => (v.has(m) ? v : new Set(v).add(m)));
  }

  return (
    <div className="flex h-full">
      <nav
        className="sidebar flex w-[232px] flex-none flex-col px-2.5 pb-2.5 pt-3"
        aria-label="Modules"
      >
        {/* Reserved for the inset traffic lights, and the window's drag handle. */}
        <div className="titlebar-drag" data-tauri-drag-region />

        <SideLabel>Clean</SideLabel>
        <ModuleButton
          icon={<StackIcon />}
          name="Cleanup"
          badge={reclaimable === null ? "—" : formatBytes(reclaimable)}
          active={active === "cleanup"}
          onClick={() => open("cleanup")}
        />

        {/* Explore, not Clean. Neither of these two removes anything on its
            own — Large & Old needs a per-file grant and Space Lens cannot act
            at all — so grouping them under the same heading as the sweep would
            misdescribe what pressing them does. */}
        <SideLabel>Explore</SideLabel>
        <ModuleButton
          icon={<FilesIcon />}
          name="Large & Old"
          badge={largeOldBytes === null ? "—" : formatBytes(largeOldBytes)}
          active={active === "large-old"}
          onClick={() => open("large-old")}
        />

        <ModuleButton
          icon={<LensIcon />}
          name="Space Lens"
          badge={measuredBytes === null ? "—" : formatBytes(measuredBytes)}
          active={active === "space-lens"}
          onClick={() => open("space-lens")}
        />

        <SideLabel>Protect</SideLabel>
        <ModuleButton
          icon={<WrenchIcon />}
          name="Startup"
          badge={loginCount === null ? "—" : String(loginCount)}
          active={active === "startup"}
          onClick={() => open("startup")}
        />

        <div className="mt-auto flex-1" data-tauri-drag-region />

        {/* The promise the whole tool rests on, kept on screen rather than
            stated once in a dialog the user has already dismissed. */}
        <div className="flex gap-2 border-t border-separator px-2 pt-2.5">
          <span className="text-success mt-px flex-none">
            <ShieldIcon size={13} />
          </span>
          <p className="text-subtle text-micro normal-case leading-snug tracking-normal">
            Nothing is removed without your explicit confirmation.
          </p>
        </div>
      </nav>

      <main className="content flex min-w-0 flex-1 flex-col">
        {visited.has("cleanup") && (
          <Pane show={active === "cleanup"}>
            <CleanView onReclaimable={setReclaimable} />
          </Pane>
        )}
        {visited.has("large-old") && (
          <Pane show={active === "large-old"}>
            <LargeOldView onTotal={setLargeOldBytes} />
          </Pane>
        )}
        {visited.has("space-lens") && (
          <Pane show={active === "space-lens"}>
            <SpaceLensView onTotal={setMeasuredBytes} />
          </Pane>
        )}
        {visited.has("startup") && (
          <Pane show={active === "startup"}>
            <StartupView onCount={setLoginCount} />
          </Pane>
        )}
      </main>
    </div>
  );
}

/** Keeps a visited module mounted so its state and data survive a switch. */
function Pane({ show, children }: { show: boolean; children: ReactNode }) {
  return (
    <div
      className={show ? "flex min-h-0 flex-1 flex-col" : "hidden"}
      aria-hidden={!show}
    >
      {children}
    </div>
  );
}

function SideLabel({ children }: { children: ReactNode }) {
  return (
    <p className="text-subtle mb-1 mt-4 px-2 text-micro font-semibold uppercase first:mt-0">
      {children}
    </p>
  );
}

function ModuleButton({
  icon,
  name,
  badge,
  active,
  onClick,
}: {
  icon: ReactNode;
  name: string;
  badge: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      aria-current={active ? "page" : undefined}
      className={`flex h-[30px] items-center gap-2 rounded-control px-2 text-left transition-colors duration-fast ease-mac ${
        active
          ? "bg-accentTint text-text"
          : "text-muted hover:bg-white/[.05] hover:text-text"
      }`}
    >
      <span
        className={`flex-none ${active ? "text-accentText" : "text-subtle"}`}
      >
        {icon}
      </span>
      <span
        className={`flex-1 text-body ${active ? "font-semibold" : "font-medium"}`}
      >
        {name}
      </span>
      <span
        className={`font-mono text-micro tabular-nums ${
          active ? "text-accentText" : "text-subtle"
        }`}
      >
        {badge}
      </span>
    </button>
  );
}
