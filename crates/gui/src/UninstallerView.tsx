import { useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { call, describeError } from "./backend";
import { Checkbox } from "./Controls";
import {
  AppsIcon,
  Group,
  Icon,
  InfoIcon,
  LockIcon,
  ShieldIcon,
  Toolbar,
} from "./Shell";
import { formatBytes } from "./format";
import type {
  CleanSummary,
  InstalledApp,
  LeftoverRow,
  UninstallReport,
  UninstallTarget,
} from "./types";

/**
 * Applications — what an app left behind, and the grant-based disposal of it.
 *
 * Three rules shape this screen, and each is the backend's rule made visible:
 *
 *   **Identity comes from a bundle the app saw.** The picker lists installed
 *   applications; choosing one records its identifier *before* the user removes
 *   it, which is the answer to "where does the id come from once the app is
 *   gone". An app that is already gone can be named by its identifier, and a
 *   caveat appears under the field exactly when the identifier looks like a
 *   macOS component rather than an app.
 *
 *   **Nothing is ever pre-selected**, exactly as in Large & Old — and here some
 *   rows cannot be selected at all. The backend marks a row `offerable` or not,
 *   with a reason: the user's own documents inside a sandbox container, a group
 *   container shared with other apps, data a still-installed sibling is using,
 *   a tree disposal is certain to refuse. Those rows are *shown*, so the user
 *   knows they exist, and are drawn as information rather than as controls: a
 *   lock where the checkbox would be, the reason where the path would be, a
 *   quieter figure, and no place in the reclaimable total.
 *
 *   **A folder is a recursive removal**, so any folder in the selection asks for
 *   the extra confirmation — however small the folder. The sheet says so.
 */

type Phase = "none" | "confirm" | "working" | "done";

/** Mirrors `macclean_core::plan`'s thresholds, only to *disclose* them. */
const MASS_COUNT = 100;
const MASS_BYTES = 5 * 1024 ** 3;

/** The sidebar's width; the sheets centre on the content pane, not the window. */
const OVERLAY = "pl-[256px]";

export default function UninstallerView({
  onTotal,
}: {
  onTotal?: (bytes: number | null) => void;
}) {
  const [target, setTarget] = useState<UninstallTarget | null>(null);
  const [report, setReport] = useState<UninstallReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [phase, setPhase] = useState<Phase>("none");
  const [actionError, setActionError] = useState("");
  const [summary, setSummary] = useState<CleanSummary | null>(null);
  const [reloadKey, setReloadKey] = useState(0);

  useEffect(() => {
    if (!target) return;
    let cancelled = false;
    setLoading(true);
    setError("");
    setReport(null);
    // A new search invalidates the selection: the paths it referred to were
    // from the previous list.
    setSelected(new Set());
    setPhase("none");

    call<UninstallReport>("uninstall_leftovers", { target })
      .then((r) => {
        if (cancelled) return;
        setReport(r);
        // An installed app has no leftovers; "0 B" in the sidebar would say
        // there is nothing, when the truth is that nothing was looked for.
        onTotal?.(r.installed ? null : r.offerable_bytes);
      })
      .catch((e) => {
        if (cancelled) return;
        setError(describeError(e));
        onTotal?.(null);
      })
      .finally(() => !cancelled && setLoading(false));

    return () => {
      cancelled = true;
    };
  }, [target, reloadKey, onTotal]);

  const rows = report?.rows ?? [];
  const offerable = useMemo(() => rows.filter((r) => r.offerable), [rows]);
  const selectedRows = useMemo(
    () => offerable.filter((r) => selected.has(r.path)),
    [offerable, selected],
  );
  const selectedBytes = selectedRows.reduce((n, r) => n + r.size_bytes, 0);
  const selectedDirs = selectedRows.filter((r) => r.is_dir).length;
  // Any folder is a recursive removal and always asks; the numeric thresholds
  // apply on top. Only ever asserted for a magnitude the sheet has disclosed.
  const needsConfirmation =
    selectedDirs > 0 ||
    selectedRows.length > MASS_COUNT ||
    selectedBytes > MASS_BYTES;
  const touchesPreferences = selectedRows.some(
    (r) =>
      r.location.startsWith("Library/Preferences") ||
      r.path.endsWith("/Library/Preferences"),
  );

  function choose(t: UninstallTarget) {
    setSummary(null);
    setTarget(t);
  }

  function reset() {
    setTarget(null);
    setReport(null);
    setError("");
    setSelected(new Set());
    setPhase("none");
    setSummary(null);
    onTotal?.(null);
  }

  function again() {
    setPhase("none");
    setSummary(null);
    setReloadKey((k) => k + 1);
  }

  function toggle(path: string) {
    setSelected((s) => {
      const next = new Set(s);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }

  async function dispose() {
    if (!target) return;
    setPhase("working");
    setActionError("");
    try {
      const result = await call<CleanSummary>("dispose_leftovers", {
        target,
        paths: selectedRows.map((r) => r.path),
        // Bind the action to what the sheet showed: rows and their sizes. The
        // backend re-runs the search and refuses if the two disagree.
        expected: { count: selectedRows.length, bytes: selectedBytes },
        confirmMassDelete: needsConfirmation,
      });
      setSummary(result);
      onTotal?.(null);
      setPhase("done");
    } catch (e) {
      setActionError(describeError(e));
      setPhase("confirm");
    }
  }

  // The done state stays inside the shell — toolbar, title, the way back —
  // rather than replacing the pane at the one moment the user most wants
  // the ground to hold still.
  const done = phase === "done" && summary !== null;

  return (
    <div className="flex h-full flex-col">
      <Toolbar title="Applications">
        {target && (
          <button
            onClick={reset}
            className="rounded-control px-3 py-1 text-caption font-medium text-muted transition-colors duration-fast ease-mac hover:text-text"
          >
            Choose another app
          </button>
        )}
      </Toolbar>

      <div className="min-h-0 flex-1 overflow-y-auto px-6 pb-6 pt-5">
        {!target && <Picker onChoose={choose} />}

        {done && summary && (
          <Done summary={summary} onAgain={again} onReset={reset} />
        )}

        {target && !done && error && (
          <Trouble
            title="Couldn’t look for leftovers"
            detail={error}
            onAgain={again}
            onReset={reset}
          />
        )}

        {target && !done && loading && <Skeleton target={target} />}

        {target && !done && !loading && !error && report && (
          <>
            {report.installed ? (
              <Installed
                target={target}
                report={report}
                onAgain={again}
                onReset={reset}
              />
            ) : rows.length === 0 ? (
              <Empty target={target} onAgain={again} onReset={reset} />
            ) : (
              <Results
                target={target}
                report={report}
                selected={selected}
                onToggle={toggle}
              />
            )}
          </>
        )}
      </div>

      {report && !done && !report.installed && offerable.length > 0 && (
        <ActionBar
          count={selectedRows.length}
          total={offerable.length}
          bytes={selectedBytes}
          onAct={() => {
            setActionError("");
            setPhase("confirm");
          }}
        />
      )}

      {target && (phase === "confirm" || phase === "working") && (
        <ConfirmModal
          target={target}
          rows={selectedRows}
          bytes={selectedBytes}
          dirs={selectedDirs}
          mass={
            selectedRows.length > MASS_COUNT || selectedBytes > MASS_BYTES
          }
          preferences={touchesPreferences}
          busy={phase === "working"}
          error={actionError}
          // After a refusal the list behind the sheet is stale against the
          // backend's re-scan, so even Cancel should look again.
          onCancel={actionError ? again : () => setPhase("none")}
          onConfirm={dispose}
          onAgain={again}
        />
      )}
    </div>
  );
}

// --- choosing an application ------------------------------------------------

function Picker({ onChoose }: { onChoose: (t: UninstallTarget) => void }) {
  const [apps, setApps] = useState<InstalledApp[] | null>(null);
  const [appsError, setAppsError] = useState("");
  const [loading, setLoading] = useState(true);
  const [query, setQuery] = useState("");
  const [manual, setManual] = useState("");

  useEffect(() => {
    let cancelled = false;
    call<InstalledApp[]>("installed_apps")
      .then((a) => !cancelled && setApps(a))
      .catch((e) => !cancelled && setAppsError(describeError(e)))
      .finally(() => !cancelled && setLoading(false));
    return () => {
      cancelled = true;
    };
  }, []);

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    const all = apps ?? [];
    return q === ""
      ? all
      : all.filter(
          (a) =>
            a.name.toLowerCase().includes(q) || a.id.toLowerCase().includes(q),
        );
  }, [apps, query]);

  const manualId = manual.trim();
  // The caveat fires exactly when it matters, rather than as boilerplate.
  const looksLikeSystem = manualId.toLowerCase().startsWith("com.apple.");

  return (
    <div className="mx-auto max-w-2xl">
      <h2 className="text-title font-semibold">Which application?</h2>
      <p className="text-muted mt-1 text-body">
        Pick an app to see what it would leave behind. Choosing it now records
        its identity for this session, so the leftovers can be found after you
        move the app to the Trash in Finder.
      </p>

      <div className="mt-4">
        <input
          type="search"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Filter by name or identifier"
          aria-label="Filter applications"
          className="h-7 w-full rounded-control border border-separator bg-white/[.06] px-3 text-body text-text transition-colors duration-fast ease-mac placeholder:text-subtle focus:border-accentText"
        />
      </div>

      {appsError && (
        <div
          className="mt-3 flex items-start gap-3 rounded-card border border-danger/30 bg-danger/[.07] px-4 py-3"
          role="alert"
        >
          <span className="mt-0.5 flex-none text-danger">
            <InfoIcon size={16} />
          </span>
          <div>
            <p className="text-body font-medium">
              Couldn&rsquo;t list your applications
            </p>
            <p className="text-muted mt-1 text-caption">
              {sentence(appsError)} You can still name an app by its identifier
              below.
            </p>
          </div>
        </div>
      )}

      {loading && (
        <div
          className="mt-3 flex flex-col gap-px overflow-hidden rounded-card"
          role="status"
          aria-label="Listing applications"
        >
          {[0, 1, 2, 3].map((i) => (
            <div key={i} className="flex items-center gap-3 bg-surface2 px-4 py-3">
              <div className="h-7 w-7 animate-pulse rounded-control bg-surface3" />
              <div className="flex-1">
                <div className="h-3 w-32 animate-pulse rounded bg-surface3" />
                <div className="mt-2 h-2.5 w-48 animate-pulse rounded bg-surface3" />
              </div>
            </div>
          ))}
        </div>
      )}

      {!loading && !appsError && (
        <Group className="mt-3" role="list" label="Installed applications">
          {shown.length === 0 ? (
            <p className="text-muted px-4 py-6 text-center text-body">
              {apps && apps.length > 0
                ? "No application matches that."
                : "No applications were found in the usual folders."}
            </p>
          ) : (
            shown.map((a, i) => (
              // The list item wraps the button rather than replacing its
              // role: a button that announces itself as a list item is no
              // longer a button to anyone who cannot see it.
              <div
                key={a.bundle_path}
                role="listitem"
                className={i === 0 ? "" : "border-t border-separator"}
              >
                <button
                  onClick={() => onChoose({ id: a.id, display_name: a.name })}
                  className="flex w-full items-center gap-3 px-4 py-3 text-left transition-colors duration-fast ease-mac hover:bg-surface2"
                >
                  {/* A monogram, not the module icon four times over: the
                      real bundle icon would need a command, and a tile that
                      says nothing a row does not is texture. */}
                  <span
                    className="text-muted grid h-7 w-7 flex-none place-items-center rounded-control bg-white/[.05] font-mono text-caption font-semibold"
                    aria-hidden="true"
                  >
                    {a.name.trim().charAt(0).toUpperCase() || "·"}
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-body font-medium text-text">
                      {a.name}
                    </span>
                    <span className="text-muted block truncate font-mono text-caption">
                      {a.id}
                    </span>
                  </span>
                </button>
              </div>
            ))
          )}
        </Group>
      )}

      {/* A section, not a second card: the same screen, one more way in. */}
      <p className="text-subtle mt-8 text-micro font-semibold uppercase">
        Or name it yourself
      </p>
      <p className="text-muted mt-1 text-caption">
        For an app you have already removed, its bundle identifier is enough.
      </p>
      <form
        className="mt-2 flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          if (manualId !== "") onChoose({ id: manualId, display_name: null });
        }}
      >
        <input
          type="text"
          value={manual}
          onChange={(e) => setManual(e.target.value)}
          placeholder="com.example.app"
          aria-label="Bundle identifier"
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          className="h-7 min-w-0 flex-1 rounded-control border border-separator bg-white/[.06] px-3 font-mono text-body text-text transition-colors duration-fast ease-mac placeholder:text-subtle focus:border-accentText"
        />
        <button
          type="submit"
          disabled={manualId === ""}
          className="rounded-control border border-border bg-surface2 px-3 py-1 text-body font-medium text-text transition-colors duration-fast ease-mac disabled:cursor-not-allowed disabled:border-separator disabled:bg-transparent disabled:text-subtle"
        >
          Look for leftovers
        </button>
      </form>
      {looksLikeSystem && (
        <div className="mt-2 flex gap-2 rounded-card border border-cat-trashes/30 bg-cat-trashes/[.07] px-3 py-2">
          <span className="mt-px flex-none text-cat-trashes">
            <InfoIcon size={14} />
          </span>
          <p className="text-muted text-caption">
            Identifiers starting with{" "}
            <span className="font-mono">com.apple.</span> usually belong to
            macOS components rather than apps you removed. mac-cleaner refuses
            anything still installed, but cannot tell a removed app from a
            component it never saw.
          </p>
        </div>
      )}
    </div>
  );
}

// --- the outcomes of a search -----------------------------------------------

/** "cannot determine home directory" → "Cannot determine home directory." */
function sentence(s: string): string {
  const t = s.trim().replace(/[.\s]+$/, "");
  return t === "" ? "" : t.charAt(0).toUpperCase() + t.slice(1) + ".";
}

/**
 * A centred outcome card: icon tile, an optional figure, title, body, up to
 * two actions. Every terminal state of this screen — installed, empty,
 * trouble, done — is one of these, so they share one geometry.
 */
function Outcome({
  tone = "neutral",
  figure,
  title,
  children,
  secondary,
  primary,
  role,
}: {
  tone?: "neutral" | "danger";
  /** Bytes, set as the hero with a smaller unit, above the title. */
  figure?: number;
  title: string;
  children: ReactNode;
  secondary?: { label: string; onClick: () => void };
  primary?: { label: string; onClick: () => void };
  role?: "alert";
}) {
  const split = figure === undefined ? null : splitBytes(figure);
  return (
    <Group className="mx-auto max-w-lg" role={undefined}>
      <div
        className="flex flex-col items-center px-6 py-10 text-center"
        role={role}
      >
        <span
          className={`grid h-11 w-11 place-items-center rounded-card ${
            tone === "danger"
              ? "bg-danger/[.16] text-danger"
              : "bg-cat-caches/[.16] text-cat-caches"
          }`}
        >
          {tone === "danger" ? <InfoIcon size={20} /> : <AppsIcon size={20} />}
        </span>
        {split && (
          <p className="mt-4 font-mono tabular-nums">
            <span className="text-display font-semibold">{split[0]}</span>
            <span className="text-muted ml-2 text-emph font-medium">
              {split[1]}
            </span>
          </p>
        )}
        <h2 className={`${split ? "mt-1" : "mt-4"} text-title font-semibold`}>
          {title}
        </h2>
        <div className="text-muted mx-auto mt-2 max-w-sm text-body">
          {children}
        </div>
        {(secondary || primary) && (
          <div className="mt-5 flex gap-2">
            {secondary && (
              <button
                onClick={secondary.onClick}
                className="rounded-control px-4 py-2 text-body font-medium text-muted transition-colors duration-fast ease-mac hover:text-text"
              >
                {secondary.label}
              </button>
            )}
            {primary && (
              <button
                onClick={primary.onClick}
                className="rounded-control border border-border bg-surface2 px-4 py-2 text-body font-medium text-text transition-colors duration-fast ease-mac"
              >
                {primary.label}
              </button>
            )}
          </div>
        )}
      </div>
    </Group>
  );
}

function Trouble({
  title,
  detail,
  onAgain,
  onReset,
}: {
  title: string;
  detail: string;
  onAgain: () => void;
  onReset: () => void;
}) {
  return (
    <Outcome
      tone="danger"
      title={title}
      role="alert"
      secondary={{ label: "Choose another app", onClick: onReset }}
      primary={{ label: "Try again", onClick: onAgain }}
    >
      <p>{sentence(detail)} Nothing was changed.</p>
    </Outcome>
  );
}

function Installed({
  target,
  report,
  onAgain,
  onReset,
}: {
  target: UninstallTarget;
  report: UninstallReport;
  onAgain: () => void;
  onReset: () => void;
}) {
  const name = target.display_name ?? target.id;
  return (
    <Outcome
      title="Still installed"
      secondary={{ label: "Choose another app", onClick: onReset }}
      primary={{ label: "Look again", onClick: onAgain }}
    >
      <p>
        <span className="font-medium text-text">{name}</span> is installed, and
        its data is in use until it is gone. Move the app to the Trash in
        Finder, then look again.
      </p>
      {report.installed_at[0] && (
        <p className="mt-2 break-all font-mono text-caption">
          {tilde(report.installed_at[0])}
        </p>
      )}
    </Outcome>
  );
}

function Empty({
  target,
  onAgain,
  onReset,
}: {
  target: UninstallTarget;
  onAgain: () => void;
  onReset: () => void;
}) {
  return (
    <Outcome
      title="Nothing left behind"
      secondary={{ label: "Choose another app", onClick: onReset }}
      primary={{ label: "Look again", onClick: onAgain }}
    >
      <p>
        No caches, preferences, saved state or containers for{" "}
        <span className="font-mono text-caption text-text">{target.id}</span>{" "}
        in any of the places this looks.
      </p>
    </Outcome>
  );
}

// --- results -----------------------------------------------------------------

/** Human labels for the backend's location keys. */
const LOCATION_LABELS: Record<string, string> = {
  "Library/Caches": "Caches",
  "Library/Containers": "Sandbox containers",
  "Library/HTTPStorages": "HTTP storage",
  "Library/WebKit": "WebKit data",
  "Library/Preferences": "Preferences",
  "Library/Preferences/ByHost": "Preferences (this Mac)",
  "Library/Saved Application State": "Saved window state",
  "Library/LaunchAgents": "Launch agents",
  "Library/Logs": "Logs",
  "Library/Application Support": "Application Support",
  "Library/Group Containers": "Group containers",
};

/**
 * A hue per location, as whole class strings — Tailwind emits CSS only for
 * classes it can see in the source. Six category hues over eleven locations,
 * so some repeat; the legend names each segment, so the colour only has to
 * separate neighbours. One hue means one thing on this screen: the track, the
 * legend and the row dots all say *location*; what kind of thing a row is,
 * the tags say in words. `dim` is the same hue on a withheld row.
 */
//
// The order below is the report's order, and no two *neighbours* share a hue:
// segments sit side by side on the track with a hairline between them, so a
// repeated hue on adjacent locations would merge two real quantities into one
// band. Caches, containers and logs keep the hue the rest of the app gives
// them; the others are assigned to keep neighbours apart. `dim` is 80% so the
// worst hue still clears 3:1 on a row — the rubric's floor for a dot — while
// reading visibly quieter than the full fill beside a checkbox.
const LOCATION_HUES: Record<string, { fill: string; dim: string }> = {
  "Library/Caches": { fill: "bg-cat-caches", dim: "bg-cat-caches/[.8]" },
  "Library/Containers": { fill: "bg-cat-build", dim: "bg-cat-build/[.8]" },
  "Library/HTTPStorages": { fill: "bg-cat-browser", dim: "bg-cat-browser/[.8]" },
  "Library/WebKit": { fill: "bg-cat-trashes", dim: "bg-cat-trashes/[.8]" },
  "Library/Preferences": { fill: "bg-cat-logs", dim: "bg-cat-logs/[.8]" },
  "Library/Preferences/ByHost": { fill: "bg-cat-large", dim: "bg-cat-large/[.8]" },
  "Library/Saved Application State": { fill: "bg-cat-caches", dim: "bg-cat-caches/[.8]" },
  "Library/LaunchAgents": { fill: "bg-cat-build", dim: "bg-cat-build/[.8]" },
  "Library/Logs": { fill: "bg-cat-logs", dim: "bg-cat-logs/[.8]" },
  "Library/Application Support": { fill: "bg-cat-browser", dim: "bg-cat-browser/[.8]" },
  "Library/Group Containers": { fill: "bg-cat-trashes", dim: "bg-cat-trashes/[.8]" },
};
const FALLBACK_HUE = { fill: "bg-cat-caches", dim: "bg-cat-caches/[.8]" };

function hue(location: string): { fill: string; dim: string } {
  return LOCATION_HUES[location] ?? FALLBACK_HUE;
}

/** One glyph per location, in the app's own icon primitive. */
function LocationIcon({ location, size = 14 }: { location: string; size?: number }) {
  switch (location) {
    case "Library/Caches":
      return (
        <Icon size={size}>
          <ellipse cx="8" cy="4.2" rx="5.4" ry="2.2" />
          <path d="M2.6 4.2v7.6c0 1.2 2.4 2.2 5.4 2.2s5.4-1 5.4-2.2V4.2M2.6 8c0 1.2 2.4 2.2 5.4 2.2s5.4-1 5.4-2.2" />
        </Icon>
      );
    case "Library/Containers":
      return (
        <Icon size={size}>
          <path d="M2.4 5.2 8 2.4l5.6 2.8v6L8 14l-5.6-2.8Z" />
          <path d="M2.4 5.2 8 8l5.6-2.8M8 8v6" />
        </Icon>
      );
    case "Library/Group Containers":
      return (
        <Icon size={size}>
          <path d="M2 6.2 6 4.4l4 1.8v4.2L6 12.2l-4-1.8Z" />
          <path d="M10 5.2 12 4.4l2 .9v4.2l-2 .9" />
          <path d="M2 6.2 6 8l4-1.8M6 8v4.2" />
        </Icon>
      );
    case "Library/HTTPStorages":
    case "Library/WebKit":
      return (
        <Icon size={size}>
          <circle cx="8" cy="8" r="6" />
          <ellipse cx="8" cy="8" rx="2.6" ry="6" />
          <path d="M2 8h12" />
        </Icon>
      );
    case "Library/Preferences":
      return (
        <Icon size={size}>
          <path d="M2 5h12M2 11h12" />
          <circle cx="6" cy="5" r="1.7" fill="currentColor" stroke="none" />
          <circle cx="10.5" cy="11" r="1.7" fill="currentColor" stroke="none" />
        </Icon>
      );
    case "Library/Preferences/ByHost":
      return (
        <Icon size={size}>
          <rect x="3" y="3" width="10" height="7" rx="1.2" />
          <path d="M1.6 12.6h12.8" />
        </Icon>
      );
    case "Library/Saved Application State":
      return (
        <Icon size={size}>
          <rect x="2" y="2.6" width="12" height="10.8" rx="1.4" />
          <path d="M2 5.8h12" />
        </Icon>
      );
    case "Library/LaunchAgents":
      return (
        <Icon size={size}>
          <circle cx="8" cy="8" r="6" />
          <path d="M6.6 5.4v5.2L10.6 8Z" />
        </Icon>
      );
    case "Library/Logs":
      return (
        <Icon size={size}>
          <path d="M3 4h10M3 8h10M3 12h6" />
        </Icon>
      );
    case "Library/Application Support":
      return (
        <Icon size={size}>
          <path d="M2 4.6a1 1 0 0 1 1-1h3.4l1.4 1.6H13a1 1 0 0 1 1 1v6.2a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1Z" />
        </Icon>
      );
    default:
      return <AppsIcon size={size} />;
  }
}

type LocationGroup = {
  location: string;
  rows: LeftoverRow[];
  offerableBytes: number;
  withheldBytes: number;
};

function Results({
  target,
  report,
  selected,
  onToggle,
}: {
  target: UninstallTarget;
  report: UninstallReport;
  selected: Set<string>;
  onToggle: (path: string) => void;
}) {
  // Group by location, preserving the backend's order (location, then path).
  const groups: LocationGroup[] = [];
  for (const row of report.rows) {
    const last = groups[groups.length - 1];
    const g =
      last && last.location === row.location
        ? last
        : (groups.push({
            location: row.location,
            rows: [],
            offerableBytes: 0,
            withheldBytes: 0,
          }),
          groups[groups.length - 1]);
    g.rows.push(row);
    if (row.offerable) g.offerableBytes += row.size_bytes;
    else g.withheldBytes += row.size_bytes;
  }
  const withheldBytes = groups.reduce((n, g) => n + g.withheldBytes, 0);
  const [figure, unit] = splitBytes(report.offerable_bytes);

  return (
    <>
      <div className="flex items-end justify-between gap-6">
        <div className="min-w-0">
          <p className="text-subtle truncate text-micro font-semibold uppercase">
            {target.display_name ? `${target.display_name} · ` : ""}
            <span className="normal-case tracking-normal">{target.id}</span>
          </p>
          <h2 className="mt-1 text-title font-semibold">
            {report.offerable_count.toLocaleString()} item
            {report.offerable_count === 1 ? "" : "s"} to review
          </h2>
          {report.withheld_count > 0 && (
            <p className="text-muted text-caption">
              {report.withheld_count.toLocaleString()} more shown but not
              offered
            </p>
          )}
        </div>
        {/* The number is the hero, as in Cleanup: what this app's absence
            gives back. Beside it, the same consent statement Cleanup makes. */}
        <div className="flex flex-none flex-col items-end">
          <p className="whitespace-nowrap font-mono tabular-nums">
            <span className="text-display font-semibold">{figure}</span>
            <span className="text-muted ml-2 text-emph font-medium">
              {unit}
            </span>
            {report.partial && (
              <span className="text-muted ml-2 font-sans text-body">
                or more
              </span>
            )}
          </p>
          <span className="text-muted mt-1 inline-flex items-center gap-1.5 rounded-full border border-separator px-2 py-0.5 text-micro font-semibold uppercase">
            <span
              className="h-[7px] w-[7px] rounded-full bg-success"
              aria-hidden="true"
            />
            Preview only
          </span>
        </div>
      </div>

      <Track groups={groups} withheldBytes={withheldBytes} />

      {/* One notice at most above the list. The report's caveats — today,
          that cfprefsd may write a preferences file back — are said on the
          sheet at the moment a preferences row is about to go, which is where
          they are actionable; repeating them here buried the first row half a
          pane down. */}
      <CoverageNotice report={report} />

      <ColumnHeader />

      {groups.map((g) => (
        <section key={g.location} className="mt-4 first-of-type:mt-2">
          <h3 className="mb-2 flex items-center gap-2 px-4">
            <span className="text-muted flex-none">
              <LocationIcon location={g.location} />
            </span>
            <span className="text-muted text-micro font-semibold uppercase">
              {LOCATION_LABELS[g.location] ?? g.location}
            </span>
            <span className="text-subtle truncate font-mono text-micro normal-case tracking-normal">
              ~/{g.location}
            </span>
          </h3>
          <Group role="list" label={LOCATION_LABELS[g.location] ?? g.location}>
            {g.rows.map((row, i) => (
              <Row
                key={row.path}
                row={row}
                first={i === 0}
                checked={selected.has(row.path)}
                onToggle={() => onToggle(row.path)}
              />
            ))}
          </Group>
        </section>
      ))}

      {report.deferred.length > 0 && (
        <p className="text-subtle mt-5 text-caption">
          Not searched:{" "}
          {report.deferred.map(([where, why], i) => (
            <span key={where}>
              {i > 0 && "; "}
              <span className="font-mono">{where}</span> — {why}
            </span>
          ))}
          .
        </p>
      )}
    </>
  );
}

/** "595.3 MiB" → ["595.3", "MiB"], so the unit can be set smaller. */
function splitBytes(bytes: number): [string, string] {
  const s = formatBytes(bytes);
  const at = s.lastIndexOf(" ");
  return at > 0 ? [s.slice(0, at), s.slice(at + 1)] : [s, ""];
}

/**
 * The one chart on this screen: how much of what the app left behind can be
 * taken back, and how much is shown but off limits. One proportional track —
 * a segment per location in that location's hue, and the withheld share as a
 * hatched segment on the same track — rather than a bar per row answering
 * "how big relative to the largest", which nobody asked.
 */
function Track({
  groups,
  withheldBytes,
}: {
  groups: LocationGroup[];
  withheldBytes: number;
}) {
  const total =
    groups.reduce((n, g) => n + g.offerableBytes, 0) + withheldBytes;
  if (total === 0) return null;
  const pct = (n: number) => (n / total) * 100;
  // Stripes measured at about 4:1 against the window (3.5:1 against the
  // track) — the rubric's floor for a graphic is 3.0 — and, for the key, a
  // tighter period on a wider swatch so several stripes show rather than one.
  const hatched = {
    backgroundImage:
      "repeating-linear-gradient(135deg, rgb(var(--text-3) / .7) 0 2px, transparent 2px 5px)",
  };
  const key = {
    backgroundImage:
      "repeating-linear-gradient(135deg, rgb(var(--text-3) / .7) 0 1px, transparent 1px 3px)",
  };

  return (
    <div className="mt-4">
      <div
        className="flex h-2 w-full gap-px overflow-hidden rounded-full bg-white/[.05]"
        role="img"
        aria-label={`${formatBytes(total - withheldBytes)} can be taken back; ${formatBytes(withheldBytes)} is shown but not offered`}
      >
        {groups
          .filter((g) => g.offerableBytes > 0)
          .map((g) => (
            <span
              key={g.location}
              className={`block h-full ${hue(g.location).fill}`}
              // A real quantity is never a zero-width segment: 3px, in pixels,
              // whatever the track's width.
              style={{ width: `${pct(g.offerableBytes)}%`, minWidth: "3px" }}
              title={`${LOCATION_LABELS[g.location] ?? g.location}: ${formatBytes(g.offerableBytes)}`}
            />
          ))}
        {withheldBytes > 0 && (
          <span
            className="block h-full"
            style={{ width: `${pct(withheldBytes)}%`, minWidth: "3px", ...hatched }}
            title={`Shown but not offered: ${formatBytes(withheldBytes)}`}
          />
        )}
      </div>
      <ul className="text-muted mt-2 flex flex-wrap gap-x-4 gap-y-1 text-caption">
        {groups
          .filter((g) => g.offerableBytes > 0)
          .map((g) => (
            <li key={g.location} className="flex items-center gap-1.5">
              <span
                className={`h-[7px] w-[7px] flex-none rounded-full ${hue(g.location).fill}`}
                aria-hidden="true"
              />
              {LOCATION_LABELS[g.location] ?? g.location}
              <span className="font-mono tabular-nums">
                {formatBytes(g.offerableBytes)}
              </span>
            </li>
          ))}
        {withheldBytes > 0 && (
          <li className="flex items-center gap-1.5">
            <span
              className="h-2 w-3.5 flex-none rounded-[2px]"
              style={key}
              aria-hidden="true"
            />
            not offered
            <span className="font-mono tabular-nums">
              {formatBytes(withheldBytes)}
            </span>
          </li>
        )}
      </ul>
    </div>
  );
}

/** `/Users/someone/Library/x` → `~/Library/x`. Display only. */
function tilde(path: string): string {
  return path.replace(/^\/Users\/[^/]+\//, "~/");
}

/**
 * What to call a row, and where it is when that is not already said by its
 * section. A container part is named by its container *and* the part, because
 * "Caches" alone under "Sandbox containers" would not say whose.
 */
function rowName(row: LeftoverRow): { name: string; where: string | null } {
  const p = tilde(row.path);
  const marker = "/Data/";
  const at = p.indexOf(marker);
  if (row.location === "Library/Containers" && at > 0) {
    const container = p.slice(p.lastIndexOf("/", at - 1) + 1, at);
    return { name: `${container} › ${p.slice(at + marker.length)}`, where: null };
  }
  const slash = p.lastIndexOf("/");
  const dir = p.slice(0, slash);
  // The section header already says the location; repeat it only when the
  // row is somewhere else, which leaves the secondary line free for a reason.
  return { name: p.slice(slash + 1), where: dir === `~/${row.location}` ? null : dir };
}

/** The secondary line: a reason, a caveat, or a place — never a repeat. */
function secondary(row: LeftoverRow, where: string | null): string | null {
  if (row.withheld) return row.withheld;
  if (row.matched_via.startsWith("name:"))
    return "matched by the app's name only — check it really is this app's";
  return where;
}

/** The few tags that add something the line itself does not say. */
function tags(row: LeftoverRow): string[] {
  const out: string[] = [];
  if (row.kind === "user_data") out.push("your data");
  if (row.kind === "shared") out.push("shared");
  if (row.license_suspected) out.push("may hold a licence");
  return out;
}

function ColumnHeader() {
  return (
    <div className="text-subtle mt-5 flex items-center gap-3 px-4 text-micro font-semibold uppercase">
      <span className="w-[14px] flex-none" aria-hidden="true" />
      <span className="flex-1">Name</span>
      <span className="hidden w-[84px] flex-none text-right md:block">Files</span>
      <span className="w-[76px] flex-none text-right">Size</span>
    </div>
  );
}

function Row({
  row,
  first,
  checked,
  onToggle,
}: {
  row: LeftoverRow;
  first: boolean;
  checked: boolean;
  onToggle: () => void;
}) {
  const { name, where } = rowName(row);
  const line = secondary(row, where);
  const labels = tags(row);
  // The dot says where, in the section's hue, dimmed with the rest of a
  // withheld row. What the row *is*, the tags say.
  const h = hue(row.location);
  const dot = row.offerable ? h.fill : h.dim;

  const body = (
    <>
      <span className="min-w-0 flex-1">
        {/* The name takes the room; the tags wrap beneath it rather than
            pushing it off the row. */}
        <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
          <span className="flex min-w-0 items-center gap-2">
            <span
              className={`h-[7px] w-[7px] flex-none rounded-full ${dot}`}
              aria-hidden="true"
            />
            <span
              className={`truncate text-body font-medium ${row.offerable ? "text-text" : "text-muted"}`}
            >
              {name}
            </span>
          </span>
          {labels.map((t) => (
            <span
              key={t}
              className="text-subtle flex-none rounded-full bg-white/[.05] px-2 text-micro font-semibold uppercase leading-4"
            >
              {t}
            </span>
          ))}
        </span>
        {line && (
          <span
            className="text-muted line-clamp-2 block pl-4 text-caption"
            title={line}
          >
            {line}
          </span>
        )}
      </span>
      <span
        className={`hidden w-[84px] flex-none text-right font-mono text-caption tabular-nums md:block ${row.offerable ? "text-muted" : "text-subtle"}`}
      >
        {row.is_dir
          ? `${row.file_count.toLocaleString()} file${row.file_count === 1 ? "" : "s"}`
          : "1 file"}
      </span>
      <span
        className={`w-[76px] flex-none text-right font-mono text-body tabular-nums ${
          row.offerable ? "font-semibold text-text" : "text-muted"
        }`}
      >
        {formatBytes(row.size_bytes)}
        {row.size_is_floor && (
          <span title="At least this much — part of it could not be measured">
            +
          </span>
        )}
      </span>
    </>
  );

  // A withheld row is information, not a control: a lock in a chip where the
  // checkbox would be, the reason where the path would be, a quieter figure,
  // and a faint wash so the eye can sort the list before reading it.
  if (!row.offerable) {
    return (
      <div
        role="listitem"
        className={`flex items-center gap-3 bg-white/[.03] px-4 py-3 ${first ? "" : "border-t border-separator"}`}
      >
        {/* The same 14px box as the checkbox it stands in for, so the name
            column keeps one edge down the whole list. */}
        <span
          className="text-subtle grid h-[14px] w-[14px] flex-none place-items-center rounded-[4px] bg-white/[.08]"
          aria-hidden="true"
        >
          <LockIcon size={10} />
        </span>
        {body}
      </div>
    );
  }

  return (
    <label
      role="listitem"
      className={`flex cursor-pointer items-center gap-3 px-4 py-3 transition-colors duration-fast ease-mac ${
        first ? "" : "border-t border-separator"
      } ${checked ? "bg-accentTint" : "hover:bg-surface2"}`}
    >
      <Checkbox checked={checked} onChange={onToggle} label={`Select ${name}`} />
      {body}
    </label>
  );
}

/** Say when the figure is a floor, each reason on its own line. */
function CoverageNotice({ report }: { report: UninstallReport }) {
  const reasons: string[] = [];
  if (report.truncated) {
    reasons.push(
      `the search stopped after ${report.examined.toLocaleString()} entries, so there may be more`,
    );
  }
  if (report.skipped_unreadable > 0) {
    const n = report.skipped_unreadable;
    reasons.push(
      `${n.toLocaleString()} folder${n === 1 ? " was" : "s were"} unreadable — granting Full Disk Access would include ${n === 1 ? "it" : "them"}`,
    );
  }
  if (report.skipped_case_variant > 0) {
    const n = report.skipped_case_variant;
    reasons.push(
      `${n.toLocaleString()} folder${n === 1 ? " is" : "s are"} named in a different case from the identifier, so ${n === 1 ? "it was" : "they were"} left alone`,
    );
  }
  if (report.skipped_uncorroborated_name > 0) {
    const n = report.skipped_uncorroborated_name;
    reasons.push(
      `${n.toLocaleString()} folder${n === 1 ? " matches" : "s match"} the app's name only, with nothing inside to prove it, so ${n === 1 ? "it was" : "they were"} left alone`,
    );
  }
  if (report.skipped_unrepresentable + report.dropped_unrepresentable_rows > 0) {
    reasons.push("some entries have names this app cannot handle safely");
  }
  if (!report.partial || reasons.length === 0) return null;

  return (
    <div className="mt-4 flex items-start gap-3 rounded-card border border-cat-trashes/30 bg-cat-trashes/[.07] px-4 py-3">
      <span className="mt-0.5 flex-none text-cat-trashes">
        <InfoIcon size={16} />
      </span>
      <div className="min-w-0">
        <p className="text-body font-medium">This is a floor, not a total</p>
        <ul className="text-muted mt-1 flex list-disc flex-col gap-1 pl-4 text-caption">
          {reasons.map((r) => (
            <li key={r}>{r}</li>
          ))}
        </ul>
      </div>
    </div>
  );
}

function ActionBar({
  count,
  total,
  bytes,
  onAct,
}: {
  count: number;
  total: number;
  bytes: number;
  onAct: () => void;
}) {
  return (
    <div className="flex flex-none items-center justify-between gap-4 border-t border-separator bg-surface px-6 py-3">
      {/* The copy may shrink; the button may not — at 720px a copy block that
          cannot shrink starves the primary onto two lines. */}
      <div className="min-w-0" aria-live="polite">
        {count === 0 ? (
          <>
            <p className="text-body font-medium">Nothing selected</p>
            <p className="text-muted text-caption">
              Tick each item you want gone. mac-cleaner never picks these for
              you.
            </p>
          </>
        ) : (
          <>
            <p className="font-mono text-emph font-semibold tabular-nums">
              {formatBytes(bytes)}
            </p>
            <p className="text-muted text-caption">
              {count.toLocaleString()} of {total.toLocaleString()} items
              selected
            </p>
          </>
        )}
      </div>
      <button
        onClick={onAct}
        disabled={count === 0}
        className="flex-none whitespace-nowrap rounded-control bg-accent px-4 py-2 text-body font-semibold text-white transition-colors duration-fast ease-mac disabled:cursor-not-allowed disabled:border disabled:border-separator disabled:bg-surface2 disabled:text-subtle"
      >
        {count === 0
          ? "Move to Trash…"
          : `Move ${count.toLocaleString()} item${count === 1 ? "" : "s"} to Trash…`}
      </button>
    </div>
  );
}

function ConfirmModal({
  target,
  rows,
  bytes,
  dirs,
  mass,
  preferences,
  busy,
  error,
  onCancel,
  onConfirm,
  onAgain,
}: {
  target: UninstallTarget;
  rows: LeftoverRow[];
  bytes: number;
  dirs: number;
  mass: boolean;
  preferences: boolean;
  busy: boolean;
  error: string;
  onCancel: () => void;
  onConfirm: () => void;
  onAgain: () => void;
}) {
  // The whole manifest, scrolling: consent is to a list, so the list is all
  // there. The cap exists only for a pathological count.
  const shown = rows.slice(0, 200);
  const more = rows.length - shown.length;
  const files = rows
    .filter((r) => r.is_dir)
    .reduce((n, r) => n + r.file_count, 0);

  return (
    <div
      className={`overlay-in fixed inset-0 z-10 flex items-center justify-center bg-black/60 p-6 ${OVERLAY}`}
      role="dialog"
      aria-modal="true"
      aria-labelledby="un-confirm-title"
    >
      {/* A neutral frame: this is the recoverable path, and the tinted border
          stays reserved for an irreversible one. */}
      <div className="sheet-in w-full max-w-md rounded-panel border border-separator bg-surface3 p-6 shadow-e3">
        <div className="flex items-start gap-3">
          <span className="grid h-9 w-9 flex-none place-items-center rounded-card bg-cat-caches/[.16] text-cat-caches">
            <AppsIcon size={18} />
          </span>
          <div className="min-w-0">
            {/* After a refusal the dialog's name is the outcome, not a pending
                question about an action that did not happen. */}
            <h2 id="un-confirm-title" className="text-title font-semibold">
              {error ? (
                "Nothing was removed"
              ) : (
                <>
                  Move {rows.length.toLocaleString()} item
                  {rows.length === 1 ? "" : "s"} to the Trash?
                </>
              )}
            </h2>
            <p className="text-muted mt-1 text-body">
              <span className="font-mono font-semibold tabular-nums text-text">
                {formatBytes(bytes)}
              </span>{" "}
              left behind by{" "}
              <span className="font-mono text-caption text-text">
                {target.id}
              </span>
              {files > 0 && (
                <>
                  {" "}
                  —{" "}
                  <span className="font-mono tabular-nums text-text">
                    {files.toLocaleString()}
                  </span>{" "}
                  files inside the folders.
                </>
              )}
            </p>
          </div>
        </div>

        <ul className="mt-4 max-h-56 overflow-y-auto rounded-card border border-separator">
          {shown.map((row, i) => {
            const { name, where } = rowName(row);
            return (
              <li
                key={row.path}
                className={`flex items-center gap-3 px-3 py-2 ${
                  i === 0 ? "" : "border-t border-separator"
                }`}
              >
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-caption font-medium text-text">
                    {name}
                  </span>
                  <span className="text-subtle block truncate text-micro normal-case tracking-normal">
                    {where ?? `~/${row.location}`}
                  </span>
                </span>
                <span className="text-muted flex-none font-mono text-caption tabular-nums">
                  {formatBytes(row.size_bytes)}
                </span>
              </li>
            );
          })}
          {more > 0 && (
            <li className="text-muted border-t border-separator px-3 py-2 text-caption">
              and {more.toLocaleString()} more
            </li>
          )}
        </ul>

        {/* Two panels, two tones: what is safe about this, and what to know.
            After a refusal neither is prospective any more, so the outcome
            leads alone. */}
        {!error && (
          <div className="mt-4 flex gap-2 rounded-card border border-success/30 bg-success/[.07] px-3 py-3">
            <span className="mt-px flex-none text-success">
              <ShieldIcon size={15} />
            </span>
            <p className="text-muted text-body">
              These belong to an app that is{" "}
              <strong className="font-semibold text-text">
                no longer installed
              </strong>
              . They go to the Trash and are recorded in the audit log, so you
              can put them back.
            </p>
          </div>
        )}

        {!error && (dirs > 0 || mass || preferences) && (
          <div className="mt-2 flex gap-2 rounded-card border border-cat-trashes/45 bg-cat-trashes/[.12] px-3 py-3">
            <span className="mt-px flex-none text-cat-trashes">
              <InfoIcon size={15} />
            </span>
            <div className="text-muted text-body">
              {dirs > 0 && (
                <p>
                  {dirs === 1 ? "One of these is a folder" : `${dirs} of these are folders`}{" "}
                  — a{" "}
                  <strong className="font-semibold text-text">
                    recursive removal
                  </strong>
                  , so it needs the extra confirmation you are giving now,
                  however small.
                </p>
              )}
              {mass && (
                <p className={dirs > 0 ? "mt-2" : ""}>
                  This is also a{" "}
                  <strong className="font-semibold text-text">
                    large action
                  </strong>{" "}
                  — over {MASS_COUNT} items or {formatBytes(MASS_BYTES)}.
                </p>
              )}
              {preferences && (
                <p className={dirs > 0 || mass ? "mt-2" : ""}>
                  A preferences file can be written back moments after removal
                  if the app is still running. Quit it first if it is.
                </p>
              )}
            </div>
          </div>
        )}

        {error && (
          <div
            className="mt-4 flex gap-2 rounded-card border border-danger/30 bg-danger/[.07] px-3 py-3"
            role="alert"
          >
            <span className="mt-px flex-none text-danger">
              <InfoIcon size={15} />
            </span>
            <p className="text-muted text-body">{sentence(error)}</p>
          </div>
        )}

        <div className="mt-6 flex items-center justify-end gap-1">
          <button
            onClick={onCancel}
            disabled={busy}
            className="rounded-control px-4 py-2 text-body font-medium text-muted transition-colors duration-fast ease-mac hover:text-text disabled:text-subtle"
          >
            Cancel
          </button>
          {/* After a refusal the list is stale against the backend's own
              re-scan; the only honest next step is to look again. */}
          {error ? (
            <button
              onClick={onAgain}
              className="rounded-control bg-accent px-4 py-2 text-body font-semibold text-white transition-colors duration-fast ease-mac"
            >
              Look again
            </button>
          ) : (
            <button
              onClick={onConfirm}
              disabled={busy}
              className="rounded-control bg-accent px-4 py-2 text-body font-semibold text-white transition-colors duration-fast ease-mac disabled:bg-accent/60"
            >
              {busy ? "Moving…" : "Move to Trash"}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

function Done({
  summary,
  onAgain,
  onReset,
}: {
  summary: CleanSummary;
  onAgain: () => void;
  onReset: () => void;
}) {
  return (
    <Outcome
      figure={summary.bytes_freed}
      title="Reclaimed"
      secondary={{ label: "Choose another app", onClick: onReset }}
      primary={{ label: "Look again", onClick: onAgain }}
    >
      <p>
        {summary.executed.toLocaleString()} item
        {summary.executed === 1 ? "" : "s"} moved to the Trash
        {summary.entries_freed > 0 && (
          <>
            , standing for{" "}
            <span className="font-mono tabular-nums">
              {summary.entries_freed.toLocaleString()}
            </span>{" "}
            files
          </>
        )}
        {summary.refused > 0 && `, ${summary.refused.toLocaleString()} refused`}
        .
      </p>
      <p className="text-subtle mt-2 text-caption leading-relaxed">
        Recorded in the audit log. Recover anything from the Trash if you
        change your mind.
      </p>
    </Outcome>
  );
}

/**
 * The state a user watches while a real disk walk runs. It says so in words
 * — grey rectangles alone are indistinguishable from a failed render — and
 * repeats the one promise that matters while the app works unattended.
 */
function Skeleton({ target }: { target: UninstallTarget }) {
  return (
    <div role="status" aria-live="polite" aria-label="Looking for leftovers">
      <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
        <p className="text-muted text-body">
          Looking for what{" "}
          <span className="font-mono text-caption text-text">{target.id}</span>{" "}
          left behind…
        </p>
        <span className="text-muted inline-flex items-center gap-1.5 rounded-full border border-separator px-2 py-0.5 text-micro font-semibold uppercase">
          <span className="text-success">
            <ShieldIcon size={12} />
          </span>
          Read-only · nothing is changed by a scan
        </span>
      </div>
      <div className="mt-4 h-2 w-full animate-pulse rounded-full bg-surface2" aria-hidden="true" />
      {/* The shape of what arrives: a section header, then a grouped list. */}
      <div className="mt-6" aria-hidden="true">
        <div className="mb-2 flex items-center gap-2 px-4">
          <div className="h-3.5 w-3.5 animate-pulse rounded bg-surface2" />
          <div className="h-2.5 w-24 animate-pulse rounded bg-surface2" />
        </div>
        {/* The resting list's own surface, hairlines and row height, so the
            content lands in place rather than jumping. */}
        <Group>
          {[0, 1, 2, 3].map((i) => (
            <div
              key={i}
              className={`flex h-[42px] items-center gap-3 px-4 ${i === 0 ? "" : "border-t border-separator"}`}
            >
              <div className="h-[14px] w-[14px] animate-pulse rounded-[4px] bg-surface2" />
              <div className="flex-1">
                <div className="h-3 w-40 animate-pulse rounded bg-surface2" />
              </div>
              <div className="h-3 w-16 animate-pulse rounded bg-surface2" />
            </div>
          ))}
        </Group>
      </div>
    </div>
  );
}
