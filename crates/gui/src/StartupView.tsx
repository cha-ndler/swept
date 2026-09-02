import { useCallback, useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { call, describeError } from "./backend";
import { Checkbox } from "./Controls";
import {
  ChevronIcon,
  Group,
  InfoIcon,
  LockIcon,
  ShieldIcon,
  Toolbar,
  WrenchIcon,
} from "./Shell";
import type { StartupItem, StartupReport, StartupSummary } from "./types";

/**
 * Startup — what runs when you log in, and the one verb this screen has.
 *
 * Three things shape it, and each is a fact about the machine rather than a
 * presentation choice.
 *
 * **Most login items are not here, and saying so comes first.** On a reference
 * machine this app can act on 5 of 31 launchd jobs, and the ones a person
 * actually recognises now live in a store macOS keeps to itself. A screen that
 * reported five things and stopped would invite the reader to conclude their
 * Mac is clean. So the disclosure sits *above* the count, because it changes
 * how the count should be read, and it carries the route to the pane that does
 * hold the rest.
 *
 * **Nothing here is removed.** The verb is "set aside": the file is linked into
 * a folder beside it and the original name removed, so putting it back is
 * available forever — and available by hand, without this app, which is why the
 * folder's real path is on screen rather than hidden behind a label.
 *
 * **What it cannot change, it shows without a control.** The system agents and
 * daemons are a plain table, collapsed, with no checkbox anywhere near them. A
 * row with a dead control reads as a refusal; a table with no controls reads as
 * information — and the difference matters when the unactionable outnumber the
 * actionable five to one.
 */

/** Sheets centre on the content pane, not the window. */
const OVERLAY = "pl-[256px]";

type Phase = "none" | "confirm" | "working" | "done";
type Verb = "aside" | "back";

export default function StartupView({
  onCount,
}: {
  /** How many things start at login, for the sidebar badge. */
  onCount?: (n: number | null) => void;
}) {
  const [report, setReport] = useState<StartupReport | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(true);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [verb, setVerb] = useState<Verb>("aside");
  const [phase, setPhase] = useState<Phase>("none");
  const [actionError, setActionError] = useState("");
  const [summary, setSummary] = useState<StartupSummary | null>(null);
  const [showSystem, setShowSystem] = useState(false);

  const load = useCallback(async () => {
    setBusy(true);
    setError("");
    setActionError("");
    setSelected(new Set());
    try {
      const r = await call<StartupReport>("startup_report");
      setReport(r);
      onCount?.(r.starts_at_login);
    } catch (e) {
      setReport(null);
      setError(describeError(e));
      onCount?.(null);
    } finally {
      setBusy(false);
    }
  }, [onCount]);

  useEffect(() => {
    void load();
  }, [load]);

  const rows = report?.items ?? [];
  const stored = report?.moved_aside ?? [];
  const chosen = useMemo(
    () => [...rows, ...stored].filter((r) => selected.has(r.path)),
    [rows, stored, selected],
  );

  function toggle(path: string, which: Verb) {
    // A selection belongs to one verb. Ticking a set-aside item while items in
    // LaunchAgents are ticked would ask for two different actions at once, and
    // the backend refuses that anyway — so the screen never builds it.
    setSelected((s) => {
      const next = which === verb ? new Set(s) : new Set<string>();
      setVerb(which);
      if (!next.delete(path)) next.add(path);
      return next;
    });
  }

  async function confirm() {
    setPhase("working");
    setActionError("");
    try {
      const s = await call<StartupSummary>(
        verb === "aside" ? "move_aside" : "put_back",
        {
          paths: chosen.map((r) => r.path),
          expected: { count: chosen.length, bytes: 0 },
        },
      );
      setSummary(s);
      setPhase("done");
      onCount?.(null);
    } catch (e) {
      setActionError(describeError(e));
      setPhase("confirm");
    }
  }

  return (
    <>
      <Toolbar title="Startup">
        {report && phase !== "done" && (
          <button
            className="rounded-control border border-border bg-surface2 px-3 py-1 text-body font-medium text-text transition-colors duration-fast ease-mac disabled:cursor-not-allowed disabled:border-separator disabled:bg-transparent disabled:text-subtle"
            onClick={() => void load()}
            disabled={busy}
          >
            Look again
          </button>
        )}
      </Toolbar>

      <div className="min-h-0 flex-1 overflow-y-auto px-5 pb-28 pt-5">
        {busy && !report && <Skeleton />}
        {!busy && error && (
          <ErrorState message={error} onRetry={() => void load()} />
        )}
        {phase === "done" && summary && (
          <Done summary={summary} verb={verb} onAgain={() => void load()} />
        )}
        {phase !== "done" && report && (
          <Results
            report={report}
            selected={selected}
            onToggle={toggle}
            showSystem={showSystem}
            setShowSystem={setShowSystem}
            onOpenSettings={() =>
              void call("open_login_items_settings").catch(() => {})
            }
          />
        )}
      </div>

      {phase !== "done" && report && chosen.length > 0 && (
        <ActionBar
          count={chosen.length}
          verb={verb}
          onReview={() => {
            setActionError("");
            setPhase("confirm");
          }}
        />
      )}

      {(phase === "confirm" || phase === "working") && (
        <ConfirmSheet
          rows={chosen}
          verb={verb}
          store={report?.store ?? ""}
          busy={phase === "working"}
          error={actionError}
          onCancel={() => (actionError ? void load() : setPhase("none"))}
          onConfirm={() => void confirm()}
        />
      )}
    </>
  );
}

// --- results ---------------------------------------------------------------

function Results({
  report,
  selected,
  onToggle,
  showSystem,
  setShowSystem,
  onOpenSettings,
}: {
  report: StartupReport;
  selected: Set<string>;
  onToggle: (path: string, verb: Verb) => void;
  showSystem: boolean;
  setShowSystem: (v: boolean) => void;
  onOpenSettings: () => void;
}) {
  const offerable = report.items.filter((i) => i.offerable);

  return (
    <>
      {/* Above the count on purpose: it changes how the count should be read.
          Five items is not a clean Mac, it is one of three places macOS keeps
          login items — and the only one this app can reach. */}
      {report.modern_store_present && (
        <div className="flex gap-2.5 rounded-card border border-separator bg-surface px-3.5 py-2.5">
          <span className="text-subtle mt-px flex-none">
            <InfoIcon size={14} />
          </span>
          <p className="text-muted text-caption leading-relaxed">
            Most apps now register their login items with macOS directly. That
            list lives in System Settings, and this app can neither read it nor
            change it — what is below is the older kind, kept as files.{" "}
            <button
              className="font-medium text-accentText underline decoration-accentText/40 underline-offset-2 hover:decoration-accentText"
              onClick={onOpenSettings}
            >
              Open Login Items &amp; Extensions
            </button>
          </p>
        </div>
      )}

      <div className="mt-4 flex items-end justify-between gap-6">
        <div className="min-w-0">
          <p className="text-subtle truncate text-micro font-semibold uppercase">
            Login items kept as files
          </p>
          <h2 className="mt-1 text-title font-semibold">
            {report.starts_at_login.toLocaleString()} start
            {report.starts_at_login === 1 ? "s" : ""} when you log in
          </h2>
          {report.items.length > report.starts_at_login && (
            <p className="text-muted text-caption">
              {(report.items.length - report.starts_at_login).toLocaleString()}{" "}
              more here that do not
            </p>
          )}
        </div>
        <span className="text-muted mt-1 inline-flex flex-none items-center gap-1.5 rounded-full border border-separator px-2 py-0.5 text-micro font-semibold uppercase">
          <span
            className="h-[7px] w-[7px] rounded-full bg-success"
            aria-hidden="true"
          />
          Reversible
        </span>
      </div>

      {report.partial && (
        <div className="mt-3">
          <div className="flex gap-2.5 rounded-card border border-warning/30 bg-warning/[.09] px-3.5 py-2.5">
            <span className="text-warning mt-px flex-none">
              <InfoIcon size={14} />
            </span>
            <p className="text-muted text-caption leading-relaxed">
              Some of this could not be read, so the list is a floor rather than
              a total.
            </p>
          </div>
        </div>
      )}

      {report.items.length === 0 ? (
        <p className="text-subtle mt-5 px-4 text-caption">
          Nothing is kept as a file in your LaunchAgents folder.
        </p>
      ) : (
        <>
          <ColumnHeader />
          <Group role="list" label="Login items" className="mt-2">
            {report.items.map((item, i) => (
              <Row
                key={item.path}
                item={item}
                first={i === 0}
                verb="aside"
                checked={selected.has(item.path)}
                onToggle={() => onToggle(item.path, "aside")}
              />
            ))}
          </Group>
          {offerable.length > 0 && (
            <p className="text-subtle mt-2 px-4 text-caption">
              Setting one aside stops it starting at your next login. Nothing is
              removed, and you can put it back.
            </p>
          )}
        </>
      )}

      {report.moved_aside.length > 0 && (
        <section className="mt-6">
          <h3 className="mb-2 flex flex-wrap items-baseline gap-x-2 px-4">
            <span className="text-muted text-micro font-semibold uppercase">
              Set aside
            </span>
            {/* The real path, not a label. If this app is ever gone, this is
                the sentence that makes the folder findable. */}
            <span className="text-subtle truncate font-mono text-micro normal-case tracking-normal">
              {tilde(report.store)}
            </span>
          </h3>
          <Group role="list" label="Set aside">
            {report.moved_aside.map((item, i) => (
              <Row
                key={item.path}
                item={item}
                first={i === 0}
                verb="back"
                checked={selected.has(item.path)}
                onToggle={() => onToggle(item.path, "back")}
              />
            ))}
          </Group>
        </section>
      )}

      {report.system.length > 0 && (
        <SystemInventory
          items={report.system}
          open={showSystem}
          setOpen={setShowSystem}
        />
      )}

      <NotThisApp />
    </>
  );
}

function ColumnHeader() {
  return (
    <div className="text-subtle mt-5 flex items-center gap-3 px-4 text-micro font-semibold uppercase">
      <span className="w-[14px] flex-none" aria-hidden="true" />
      <span className="flex-1">What it is</span>
      <span className="w-[150px] flex-none text-right">When it starts</span>
    </div>
  );
}

/** `/Users/someone/Library/x` → `~/Library/x`. Display only. */
function tilde(path: string): string {
  const m = path.match(/^\/Users\/[^/]+(\/.*)?$/);
  return m ? `~${m[1] ?? ""}` : path;
}

const CLASS_TONE: Record<StartupItem["class"], string> = {
  starts_at_login: "text-accentText",
  starts_on_demand: "text-muted",
  broken: "text-warning",
  unknown: "text-subtle",
};

/**
 * The column says the short form; `describes` is the sentence.
 *
 * The backend's sentence is the right thing in a sheet, where there is room to
 * read it. In a 150px column, once per row, it restates the heading in full on
 * every line — so the column carries the label and the sentence is the title.
 */
const CLASS_LABEL: Record<StartupItem["class"], string> = {
  starts_at_login: "at login",
  starts_on_demand: "on demand",
  broken: "program missing",
  unknown: "cannot tell",
};

function Row({
  item,
  first,
  verb,
  checked,
  onToggle,
}: {
  item: StartupItem;
  first: boolean;
  verb: Verb;
  checked: boolean;
  onToggle: () => void;
}) {
  // A row's own reason earns the line; otherwise the program, which is what
  // tells a person what this actually is.
  const line = item.withheld ?? item.program ?? tilde(item.path);

  const body = (
    <>
      <span className="min-w-0 flex-1">
        <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
          <span
            className={`truncate text-body font-medium ${item.offerable ? "text-text" : "text-muted"}`}
          >
            {item.label}
          </span>
          {item.duplicate_label && (
            <span className="text-muted flex-none rounded-full bg-white/[.05] px-2 text-micro font-semibold uppercase leading-4">
              duplicate name
            </span>
          )}
          {item.plist_says_disabled && (
            <span
              className="text-muted flex-none rounded-full bg-white/[.05] px-2 text-micro font-semibold uppercase leading-4"
              title="Its file carries a Disabled key. macOS keeps the real answer in a database this app cannot read, so the two can disagree."
            >
              marked disabled in its file
            </span>
          )}
        </span>
        <span
          className="text-muted line-clamp-2 block text-caption"
          title={line}
        >
          {line}
        </span>
      </span>
      <span
        className={`w-[150px] flex-none text-right text-caption ${CLASS_TONE[item.class]}`}
        title={item.describes}
      >
        {verb === "back" ? "set aside" : CLASS_LABEL[item.class]}
      </span>
    </>
  );

  // A row this app cannot act on is information: a lock where the checkbox
  // would be, and its reason where the program would be.
  if (!item.offerable) {
    return (
      <div
        role="listitem"
        className={`flex items-center gap-3 px-4 py-3 ${first ? "" : "border-t border-separator"}`}
      >
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
      <Checkbox
        checked={checked}
        onChange={onToggle}
        label={`${verb === "aside" ? "Set aside" : "Put back"} ${item.label}`}
      />
      {body}
    </label>
  );
}

/**
 * The jobs macOS and your administrator manage.
 *
 * A table, not rows: no checkbox, no lock, no per-item reason. A row with a
 * dead control reads as a refusal — and on a real machine these outnumber the
 * actionable items five to one, so the whole screen would read as one.
 */
function SystemInventory({
  items,
  open,
  setOpen,
}: {
  items: StartupReport["system"];
  open: boolean;
  setOpen: (v: boolean) => void;
}) {
  return (
    <section className="mt-6">
      <button
        onClick={() => setOpen(!open)}
        aria-expanded={open}
        className="text-muted flex items-center gap-1.5 rounded-control px-4 text-micro font-semibold uppercase transition-colors duration-fast ease-mac hover:text-text focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accentText"
      >
        <span className={open ? "rotate-90 transition-transform" : "transition-transform"}>
          <ChevronIcon size={11} />
        </span>
        {items.length} more macOS manages
      </button>
      <p className="text-subtle mt-1 px-4 text-caption">
        macOS and your administrator manage these. This app can read them and
        can never change them.
      </p>
      {open && (
        <Group className="mt-2">
          <table className="w-full text-caption">
            <tbody>
              {items.map((s, i) => (
                <tr
                  key={s.path}
                  className={i > 0 ? "border-t border-separator" : ""}
                >
                  <td className="text-muted truncate px-4 py-2">{s.label}</td>
                  <td className="text-subtle truncate px-4 py-2 font-mono">
                    {s.directory}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </Group>
      )}
    </section>
  );
}

/**
 * The other half of an honest maintenance screen: the things it will not do.
 *
 * No buttons and no greyed rows. Three of these four are one line a person can
 * run themselves, which is more useful than a control that would fail, and the
 * fourth is a myth worth retiring.
 */
function NotThisApp() {
  const rows: [string, ReactNode][] = [
    [
      "Flush the DNS cache",
      <>
        Needs administrator rights.{" "}
        <code className="font-mono text-micro">
          sudo dscacheutil -flushcache; sudo killall -HUP mDNSResponder
        </code>
      </>,
    ],
    [
      "Free inactive memory",
      <>
        Needs administrator rights, and macOS already reclaims it as other apps
        ask for it. <code className="font-mono text-micro">sudo purge</code> if
        you want it now.
      </>,
    ],
    [
      "Rebuild the Spotlight index",
      <>
        System Settings › Siri &amp; Spotlight › Spotlight Privacy — add your
        disk, then remove it again.
      </>,
    ],
    [
      "Repair disk permissions",
      <>This has not existed since OS X El Capitan. macOS maintains them.</>,
    ],
  ];

  return (
    <section className="mt-8">
      <h3 className="text-subtle mb-2 flex items-center gap-1.5 px-4 text-micro font-semibold uppercase">
        <WrenchIcon size={12} />
        What this screen will not do
      </h3>
      <Group className="px-4 py-3">
        <dl className="space-y-2.5">
          {rows.map(([what, why]) => (
            <div key={what}>
              <dt className="text-body font-medium">{what}</dt>
              <dd className="text-muted text-caption leading-relaxed">{why}</dd>
            </div>
          ))}
        </dl>
        <p className="text-subtle mt-3 border-t border-separator pt-2.5 text-caption leading-relaxed">
          Each needs administrator rights that a notarized app can only get by
          installing a privileged background helper. mac-cleaner does not
          install one.
        </p>
      </Group>
    </section>
  );
}

// --- action bar and sheet --------------------------------------------------

function ActionBar({
  count,
  verb,
  onReview,
}: {
  count: number;
  verb: Verb;
  onReview: () => void;
}) {
  return (
    <div className="flex flex-none items-center justify-between gap-4 border-t border-separator bg-surface px-6 py-3">
      <div className="min-w-0" aria-live="polite">
        <p className="text-body font-medium">
          {count.toLocaleString()} selected
        </p>
        {/* The timing caveat lives on the sheet, at the moment of consent —
            saying it here too put the same sentence on screen twice. */}
        <p className="text-muted text-caption">
          Nothing is removed. You can put it back.
        </p>
      </div>
      <button
        onClick={onReview}
        className="flex-none whitespace-nowrap rounded-control bg-accent px-4 py-2 text-body font-semibold text-white transition-colors duration-fast ease-mac"
      >
        {verb === "aside"
          ? `Set ${count.toLocaleString()} aside…`
          : `Put ${count.toLocaleString()} back…`}
      </button>
    </div>
  );
}

/**
 * The sheet, and the sentence that surprises people.
 *
 * No per-consequence acknowledgement here, unlike Privacy: this action is
 * reversible by construction, and a checkbox on a safe action is ceremony that
 * teaches people to click through the ones that are not. What it does carry is
 * the timing, because "I set it aside and it is still running" is the thing a
 * user would otherwise report as a bug.
 */
function ConfirmSheet({
  rows,
  verb,
  store,
  busy,
  error,
  onCancel,
  onConfirm,
}: {
  rows: StartupItem[];
  verb: Verb;
  store: string;
  busy: boolean;
  error: string;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const aside = verb === "aside";
  return (
    <div
      className={`overlay-in fixed inset-0 z-10 flex items-center justify-center bg-black/60 p-6 ${OVERLAY}`}
      role="dialog"
      aria-modal="true"
      aria-labelledby="su-confirm-title"
    >
      <div className="sheet-in flex max-h-full w-full max-w-md flex-col rounded-panel border border-separator bg-surface3 p-6 shadow-e3">
        <div className="flex items-start gap-3">
          <span
            className={`grid h-9 w-9 flex-none place-items-center rounded-card ${
              error
                ? "bg-danger/[.16] text-danger"
                : "bg-cat-build/[.16] text-cat-build"
            }`}
          >
            {error ? <InfoIcon size={18} /> : <WrenchIcon size={18} />}
          </span>
          <div className="min-w-0">
            <h2 id="su-confirm-title" className="text-title font-semibold">
              {error
                ? "Nothing was changed"
                : aside
                  ? `Set ${rows.length} aside?`
                  : `Put ${rows.length} back?`}
            </h2>
            <p className="text-muted mt-1 text-body">
              {aside
                ? "They stop starting when you log in."
                : "They start again when you log in."}
            </p>
          </div>
        </div>

        <div className="mt-4 min-h-0 flex-1 overflow-y-auto">
          {!error && (
            <>
              <p className="text-subtle text-micro font-semibold uppercase">
                {aside ? "What will be set aside" : "What will be put back"}
              </p>
              <ul className="mt-1.5 space-y-1">
                {rows.map((r) => (
                  <li key={r.path} className="flex items-baseline gap-2">
                    <span className="text-muted min-w-0 flex-1 truncate text-caption">
                      {r.label}
                    </span>
                    <span className="text-subtle flex-none text-caption">
                      {r.describes}
                    </span>
                  </li>
                ))}
              </ul>

              <div className="mt-4 flex gap-2.5 rounded-card border border-success/25 bg-success/[.09] px-3.5 py-2.5">
                <span className="text-success mt-px flex-none">
                  <ShieldIcon size={14} />
                </span>
                <p className="text-muted text-caption leading-relaxed">
                  {aside ? (
                    <>
                      Nothing is removed. The file moves to{" "}
                      <span className="font-mono">{tilde(store)}</span>, and you
                      can put it back here or by dragging it up one level.
                    </>
                  ) : (
                    <>
                      The file moves back into your LaunchAgents folder under
                      the name it had.
                    </>
                  )}
                </p>
              </div>

              {/* The sentence people would otherwise report as a bug. */}
              <div className="mt-2 flex gap-2.5 rounded-card border border-warning/30 bg-warning/[.09] px-3.5 py-2.5">
                <span className="text-warning mt-px flex-none">
                  <InfoIcon size={14} />
                </span>
                <p className="text-muted text-caption leading-relaxed">
                  This takes effect at your <b>next login</b>. Anything already
                  running keeps running until you log out.
                </p>
              </div>
            </>
          )}

          {error && (
            <div className="rounded-card border border-danger/30 bg-danger/[.07] px-3.5 py-3">
              <p className="text-body">{sentence(error)}</p>
              <p className="text-muted mt-1 text-caption">
                The list is out of date now, so the only honest next step is to
                look again.
              </p>
            </div>
          )}
        </div>

        <div className="mt-5 flex flex-none items-center justify-end gap-1">
          <button
            onClick={onCancel}
            disabled={busy}
            className="rounded-control px-4 py-2 text-body font-medium text-muted transition-colors duration-fast ease-mac hover:text-text disabled:text-subtle"
          >
            {error ? "Look again" : "Cancel"}
          </button>
          {!error && (
            <button
              onClick={onConfirm}
              disabled={busy}
              className="rounded-control bg-accent px-4 py-2 text-body font-semibold text-white transition-colors duration-fast ease-mac disabled:bg-accent/60"
            >
              {busy ? "Working…" : aside ? "Set aside" : "Put back"}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

/**
 * The backend's refusal, addressed to a person. `refused:` is a prefix for the
 * audit log, not for the one sentence a worried user reads most carefully.
 */
function sentence(raw: string): string {
  const s = raw.replace(/^refused:\s*/i, "").trim();
  return s.charAt(0).toUpperCase() + s.slice(1);
}

// --- terminal states -------------------------------------------------------

function Done({
  summary,
  verb,
  onAgain,
}: {
  summary: StartupSummary;
  verb: Verb;
  onAgain: () => void;
}) {
  return (
    <Outcome
      tone="success"
      icon={<ShieldIcon size={20} />}
      title={
        verb === "aside"
          ? `${summary.moved} set aside`
          : `${summary.moved} put back`
      }
      body={
        <>
          It takes effect at your next login — anything already running keeps
          running until you log out. Nothing was removed.
          {summary.refused > 0 && (
            <>
              {" "}
              {summary.refused} item
              {summary.refused === 1 ? " was" : "s were"} refused at the last
              check and left alone.
            </>
          )}
        </>
      }
      action={{ label: "Look again", onClick: onAgain }}
    />
  );
}

function ErrorState({
  message,
  onRetry,
}: {
  message: string;
  onRetry: () => void;
}) {
  return (
    <Outcome
      tone="danger"
      icon={<InfoIcon size={20} />}
      title="Could not look"
      body={
        <>
          {message}
          <br />
          Nothing was scanned and nothing was changed.
        </>
      }
      action={{ label: "Try again", onClick: onRetry }}
    />
  );
}

function Outcome({
  tone,
  icon,
  title,
  body,
  action,
}: {
  tone: "success" | "danger";
  icon: ReactNode;
  title: string;
  body: ReactNode;
  action: { label: string; onClick: () => void };
}) {
  const tiles = {
    success: "bg-success/[.14] text-success",
    danger: "bg-danger/[.14] text-danger",
  } as const;
  return (
    <div className="mx-auto mt-10 max-w-md">
      <Group className="px-6 py-7 text-center">
        <span
          className={`mx-auto grid h-11 w-11 place-items-center rounded-card ${tiles[tone]}`}
        >
          {icon}
        </span>
        <h2 className="mt-3 text-title font-semibold">{title}</h2>
        <p className="text-muted mx-auto mt-1.5 max-w-sm text-body leading-relaxed">
          {body}
        </p>
        <button
          className="mt-4 rounded-control border border-border bg-surface2 px-3 py-1 text-body font-medium text-text transition-colors duration-fast ease-mac"
          onClick={action.onClick}
        >
          {action.label}
        </button>
      </Group>
    </div>
  );
}

/** A scan in progress, with words — a wordless pulse says nothing. */
function Skeleton() {
  return (
    <div>
      <div className="flex items-end justify-between gap-6">
        <div>
          <p className="text-subtle text-micro font-semibold uppercase">
            Login items kept as files
          </p>
          <h2 className="text-muted mt-1 text-title font-semibold">
            Looking at what starts when you log in…
          </h2>
        </div>
        <span className="text-muted inline-flex flex-none items-center gap-1.5 rounded-full border border-separator px-2 py-0.5 text-micro font-semibold uppercase">
          <span
            className="h-[7px] w-[7px] rounded-full bg-success"
            aria-hidden="true"
          />
          Read-only · nothing is changed by a scan
        </span>
      </div>
      <Group className="mt-6">
        {[0, 1, 2, 3].map((i) => (
          <div
            key={i}
            className={`flex items-center gap-3 px-4 py-3 ${i ? "border-t border-separator" : ""}`}
          >
            <span className="h-[14px] w-[14px] flex-none animate-pulse rounded-[4px] bg-surface2" />
            <span className="h-3 flex-1 animate-pulse rounded bg-surface2" />
            <span className="h-3 w-[150px] flex-none animate-pulse rounded bg-surface2" />
          </div>
        ))}
      </Group>
    </div>
  );
}
