import { useState } from "react";
import type { ReactNode } from "react";
import CleanView from "./CleanView";
import SmartScanView from "./SmartScanView";
import LargeOldView from "./LargeOldView";
import SpaceLensView from "./SpaceLensView";
import StartupView from "./StartupView";
import PrivacyView from "./PrivacyView";
import UninstallerView from "./UninstallerView";
import {
  AppsIcon,
  FilesIcon,
  LensIcon,
  MaskIcon,
  ShieldIcon,
  SparkleIcon,
  StackIcon,
  WrenchIcon,
} from "./Shell";
import { formatBytes } from "./format";

type Module =
  | "smart-scan"
  | "cleanup"
  | "applications"
  | "large-old"
  | "space-lens"
  | "privacy"
  | "startup";

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
 * The sidebar lists only modules that actually exist, which is now all seven the
 * artboards show. Showing a module before it works would promise a capability
 * that isn't there — the same class of dishonesty as the sample-data fallback
 * removed in v0.3, and the reason Smart Scan was absent from this list until it
 * had a screen rather than only an engine.
 *
 * **Smart Scan opens first, and does not scan on mount.** Every other module
 * runs its scan the moment it is first visited, which is right for a screen you
 * navigated to on purpose. The first screen of the app is not that: it would
 * mean four scans, several seconds and a spinner, for a user who has not yet
 * said what they want. So it opens at rest with a button, which is also what
 * artboard 03 shows.
 */
const MODULES = [
  "smart-scan",
  "cleanup",
  "applications",
  "large-old",
  "space-lens",
  "privacy",
  "startup",
] as const;

function initialModule(): Module {
  const tab = new URLSearchParams(window.location.search).get("tab");
  return MODULES.includes(tab as Module) ? (tab as Module) : "smart-scan";
}

export default function App() {
  const [active, setActive] = useState<Module>(initialModule);
  // Mount-on-first-visit, then keep alive. Mounting every module up front would
  // run backend work the user never asked for.
  const [visited, setVisited] = useState<Set<Module>>(
    () => new Set([initialModule()]),
  );
  // The label the Cleanup screen is showing, not a number to re-format here —
  // so a figure that is a floor arrives already saying so.
  const [reclaimable, setReclaimable] = useState<string | null>(null);
  // Smart Scan's own headline, kept apart from Cleanup's. They are different
  // figures — Smart Scan's excludes what its gesture never touches — so one
  // slot holding whichever ran last would be a badge that changes meaning
  // without changing label.
  const [smartLabel, setSmartLabel] = useState<string | null>(null);
  const [leftoverBytes, setLeftoverBytes] = useState<number | null>(null);
  const [largeOldBytes, setLargeOldBytes] = useState<number | null>(null);
  const [measuredBytes, setMeasuredBytes] = useState<number | null>(null);
  const [privacyCount, setPrivacyCount] = useState<number | null>(null);
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
        {/* First, and the app's front door: the one gesture that spans the
            others. Its badge is what a confirmed run would free, which is not
            the same as Cleanup's total below — Smart Scan leaves the Trash
            alone, and that difference is the point rather than a rounding. */}
        <ModuleButton
          icon={<SparkleIcon />}
          name="Smart Scan"
          badge={smartLabel ?? "—"}
          active={active === "smart-scan"}
          onClick={() => open("smart-scan")}
        />
        <ModuleButton
          icon={<StackIcon />}
          name="Cleanup"
          badge={reclaimable ?? "—"}
          active={active === "cleanup"}
          onClick={() => open("cleanup")}
        />
        {/* Under Clean, because it removes things — but only rows the user
            ticks one by one, for one application they named. The badge is the
            last report's offerable figure, and an em dash until there is one. */}
        <ModuleButton
          icon={<AppsIcon />}
          name="Applications"
          badge={leftoverBytes === null ? "—" : formatBytes(leftoverBytes)}
          active={active === "applications"}
          onClick={() => open("applications")}
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

        {/* Protect, not Clean. The headings are about what a module is *for*:
            Cleanup and Applications are about space, Explore is about seeing
            the disk, and this is about what your machine remembers of you.
            And the badge is a **count**, not bytes — nobody opens this screen
            to reclaim 19 MiB, so a size here would be the one number that does
            not describe why anyone came. The artboard's sidebar already shows
            a bare count in this slot. */}
        <SideLabel>Protect</SideLabel>
        <ModuleButton
          icon={<MaskIcon />}
          name="Privacy"
          badge={privacyCount === null ? "—" : String(privacyCount)}
          active={active === "privacy"}
          onClick={() => open("privacy")}
        />

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
        {visited.has("smart-scan") && (
          <Pane show={active === "smart-scan"}>
            <SmartScanView onTotal={setSmartLabel} onOpenModule={open} />
          </Pane>
        )}
        {visited.has("cleanup") && (
          <Pane show={active === "cleanup"}>
            <CleanView onReclaimable={setReclaimable} />
          </Pane>
        )}
        {visited.has("applications") && (
          <Pane show={active === "applications"}>
            <UninstallerView onTotal={setLeftoverBytes} />
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
        {visited.has("privacy") && (
          <Pane show={active === "privacy"}>
            <PrivacyView onCount={setPrivacyCount} />
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
