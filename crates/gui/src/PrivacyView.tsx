import { useCallback, useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { call, describeError } from "./backend";
import { Checkbox, Segmented } from "./Controls";
import { formatBytes } from "./format";
import {
  ChevronIcon,
  Group,
  Icon,
  InfoIcon,
  LockIcon,
  MaskIcon,
  ShieldIcon,
  Toolbar,
} from "./Shell";
import type {
  Acknowledged,
  CleanSummary,
  PrivacyBrowser,
  PrivacyReport,
  PrivacyRow,
} from "./types";

/**
 * Privacy — what browsers remember, and what of it may be removed.
 *
 * Three things shape this screen, and each is a backend rule made visible
 * rather than a presentation choice.
 *
 * **The consequence is the headline, not the size.** Every other module answers
 * "how much space is this?". Here the number a person actually needs is what
 * they lose: cookies sign you out of every site, history cannot be brought
 * back, a session is the tabs you have open. So the chart is by *consequence*,
 * the tags say it on every row, and the confirmation sheet asks for a separate
 * acknowledgement of each one — which is exactly what `dispose_privacy` demands
 * before it will act. A sheet whose checkboxes were decoration would be worse
 * than none: the backend refuses either way, but only one of the two tells the
 * user why.
 *
 * **A withheld row is information, not a control.** Website storage, Safari's
 * container, a browser that looks like it is running, Firefox history that
 * shares a file with the bookmarks — each is on screen with its reason where
 * the path would be and a lock where the checkbox would be. Hiding them would
 * make the module look like it had searched less than it did.
 *
 * **Absent and denied are different states.** Safari is unreadable by default
 * on a stock Mac, and that is a thing the user can fix; a browser that is not
 * installed is not. They get different treatment, because conflating them
 * either sends someone to System Settings for nothing or hides a whole browser
 * behind a shrug.
 */

const MASS_COUNT = 100;
const MASS_BYTES = 5 * 1024 ** 3;

/** Sheets centre on the content pane, not the window. */
const OVERLAY = "pl-[256px]";

type Phase = "none" | "confirm" | "working" | "done";

export default function PrivacyView({
  onCount,
}: {
  /** How many rows are on offer, for the sidebar badge. */
  onCount?: (n: number | null) => void;
}) {
  const [report, setReport] = useState<PrivacyReport | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(true);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [phase, setPhase] = useState<Phase>("none");
  const [ack, setAck] = useState<Acknowledged>(BLANK_ACK);
  const [actionError, setActionError] = useState("");
  const [summary, setSummary] = useState<CleanSummary | null>(null);
  // What the done state describes. Kept because `load()` clears the selection.
  const [removed, setRemoved] = useState<PrivacyRow[]>([]);
  const [filter, setFilter] = useState<Filter>("offerable");

  const load = useCallback(async () => {
    setBusy(true);
    setError("");
    setActionError("");
    setSelected(new Set());
    try {
      const r = await call<PrivacyReport>("privacy_report");
      setReport(r);
      // 0 becomes `—`: every other module shows a dash when it has nothing to
      // say, and a bare zero in that slot reads as a measurement rather than an
      // absence.
      const n = r.rows.filter((x) => x.offerable).length;
      onCount?.(n === 0 ? null : n);
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

  const rows = report?.rows ?? [];
  const chosen = useMemo(
    () => rows.filter((r) => selected.has(r.path)),
    [rows, selected],
  );
  const bytes = chosen.reduce((n, r) => n + r.size_bytes, 0);
  const dirs = chosen.filter((r) => r.is_dir).length;
  const items = chosen.reduce((n, r) => n + r.member_count, 0);
  // The same rule the backend applies: any directory action asks, whatever its
  // size, because a recursive removal is a recursive removal.
  const needsMass = dirs > 0 || items > MASS_COUNT || bytes > MASS_BYTES;

  function toggle(path: string) {
    setSelected((s) => {
      const next = new Set(s);
      if (!next.delete(path)) next.add(path);
      return next;
    });
  }

  function openSheet() {
    // Never carry an acknowledgement across sheets. A tick means "I have read
    // this list", and this is a different list.
    setAck(BLANK_ACK);
    setActionError("");
    setPhase("confirm");
  }

  async function confirm() {
    setPhase("working");
    setActionError("");
    try {
      const s = await call<CleanSummary>("dispose_privacy", {
        paths: chosen.map((r) => r.path),
        acknowledged: ack,
        expected: { count: chosen.length, bytes },
        confirmMassDelete: needsMass,
      });
      setSummary(s);
      setRemoved(chosen);
      setPhase("done");
      onCount?.(null);
    } catch (e) {
      setActionError(describeError(e));
      setPhase("confirm");
    }
  }

  return (
    <>
      <Toolbar title="Privacy">
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
        {!busy && error && <ErrorState message={error} onRetry={() => void load()} />}
        {phase === "done" && summary && (
          <Done summary={summary} removed={removed} onAgain={() => void load()} />
        )}
        {phase !== "done" && report && (
          <Results
            report={report}
            selected={selected}
            onToggle={toggle}
            onGrant={() => void call("open_privacy_settings").catch(() => {})}
            filter={filter}
            setFilter={setFilter}
          />
        )}
      </div>

      {phase !== "done" && report && rows.some((r) => r.offerable) && (
        <ActionBar
          count={chosen.length}
          bytes={bytes}
          onReview={openSheet}
          disabled={chosen.length === 0}
        />
      )}

      {(phase === "confirm" || phase === "working") && (
        <ConfirmModal
          rows={chosen}
          bytes={bytes}
          items={items}
          dirs={dirs}
          mass={needsMass}
          ack={ack}
          setAck={setAck}
          live={
            report?.browsers.some(
              (b) => b.may_be_live && chosen.some((r) => r.browser === b.id),
            ) ?? false
          }
          busy={phase === "working"}
          error={actionError}
          onCancel={() => (actionError ? void load() : setPhase("none"))}
          onConfirm={() => void confirm()}
        />
      )}
    </>
  );
}

const BLANK_ACK: Acknowledged = {
  signs_you_out: false,
  erases_history: false,
  loses_open_tabs: false,
};

// --- consequences ----------------------------------------------------------

/**
 * The five consequences, in the words a person needs at the moment of consent.
 *
 * `ack` names the field of `Acknowledged` that authorizes it, and `null` means
 * the backend needs no acknowledgement — a cache is rebuilt and nothing chosen
 * is lost. The mapping is total, so a new consequence cannot be added without
 * deciding what it costs and who may agree to it.
 */
const CONSEQUENCES: Record<
  PrivacyRow["consequence"],
  {
    tag: string;
    short: string;
    /** What the done state says happened. */
    past: string;
    sentence: string;
    ack: keyof Acknowledged | null;
  }
> = {
  signs_you_out: {
    past: "Signed out",
    tag: "signs you out",
    short: "Signs you out",
    sentence:
      "I understand this signs me out of every site these browsers hold a cookie for.",
    ack: "signs_you_out",
  },
  erases_history: {
    past: "History erased",
    tag: "erases history",
    short: "Erases history",
    sentence:
      "I understand my browsing history will be gone, and that it cannot be brought back.",
    ack: "erases_history",
  },
  loses_open_tabs: {
    past: "Sessions cleared",
    tag: "loses open tabs",
    short: "Loses open tabs",
    sentence:
      "I understand the open tabs and the saved session will be lost.",
    ack: "loses_open_tabs",
  },
  loses_site_data: {
    past: "Site storage cleared",
    tag: "site data",
    short: "Website storage",
    sentence: "",
    ack: null,
  },
  regenerable: {
    past: "Caches cleared",
    tag: "",
    short: "Rebuilt automatically",
    sentence: "",
    ack: null,
  },
};

/**
 * One hue per class, and no two adjacent in the track share one.
 *
 * These are the same six category tokens the rest of the app uses, and that is
 * a compromise worth naming: elsewhere orange means Trashes and purple means
 * Logs, so on this screen those hues carry a second meaning. What keeps it
 * legible is that the hue is never the only signal — every class also has its
 * own glyph, in the track key, on the row and on the acknowledgement — so the
 * distinction survives a reader who has learned the other screens' palette.
 */
const CLASS_HUES: Record<PrivacyRow["class"], { fill: string; text: string }> = {
  cookies: { fill: "bg-cat-trashes", text: "text-cat-trashes" },
  history: { fill: "bg-cat-browser", text: "text-cat-browser" },
  session: { fill: "bg-cat-logs", text: "text-cat-logs" },
  cache: { fill: "bg-cat-caches", text: "text-cat-caches" },
  site_storage: { fill: "bg-cat-large", text: "text-cat-large" },
};

/**
 * A glyph per consequence class.
 *
 * The screen's whole argument is that *what you lose* matters more than how
 * much of it there is, and until this existed that argument was carried by a
 * 7px dot and eleven-pixel uppercase text — legible on inspection, invisible at
 * a glance. Each appears in four places (track key, row, tag, acknowledgement)
 * so one shape means one thing wherever it turns up.
 */
function ClassIcon({
  cls,
  size = 14,
}: {
  cls: PrivacyRow["class"];
  size?: number;
}) {
  switch (cls) {
    case "cookies": // a cookie, bitten
      return (
        <Icon size={size}>
          <path d="M8 1.7a6.3 6.3 0 1 0 6.3 6.3 2.4 2.4 0 0 1-3.2-2.2 2.4 2.4 0 0 1-3.1-4.1Z" />
          <path d="M6 6.4v.1M9.6 9.3v.1M5.6 10.4v.1" />
        </Icon>
      );
    case "history": // a clock turned back
      return (
        <Icon size={size}>
          <path d="M2.2 8a5.8 5.8 0 1 0 1.7-4.1" />
          <path d="M1.7 2.2v2.9h2.9" />
          <path d="M8 5.1V8l2 1.2" />
        </Icon>
      );
    case "session": // stacked windows
      return (
        <Icon size={size}>
          <rect x="1.8" y="4.4" width="9" height="7.4" rx="1.4" />
          <path d="M5.2 4.4V3.6a1.4 1.4 0 0 1 1.4-1.4h6a1.4 1.4 0 0 1 1.4 1.4v6a1.4 1.4 0 0 1-1.4 1.4h-.8" />
        </Icon>
      );
    case "site_storage": // a database
      return (
        <Icon size={size}>
          <ellipse cx="8" cy="3.9" rx="5.3" ry="2.1" />
          <path d="M2.7 3.9v8.2c0 1.2 2.4 2.1 5.3 2.1s5.3-.9 5.3-2.1V3.9" />
          <path d="M2.7 8c0 1.2 2.4 2.1 5.3 2.1s5.3-.9 5.3-2.1" />
        </Icon>
      );
    case "cache": // a disc, rebuilt on demand
      return (
        <Icon size={size}>
          <circle cx="8" cy="8" r="6.2" />
          <circle cx="8" cy="8" r="1.9" />
        </Icon>
      );
  }
}

const CLASS_LABELS: Record<PrivacyRow["class"], string> = {
  cookies: "Cookies",
  history: "History",
  session: "Sessions",
  cache: "Caches",
  site_storage: "Website storage",
};

/**
 * Stripes measured at about 4:1 against the window — the rubric's floor for a
 * graphic is 3.0 — with a tighter period on the wider key swatch so several
 * stripes show rather than one. Taken from the Uninstaller's track rather than
 * guessed again: two hatches that drifted would be two different claims about
 * the same idea.
 */
const HATCH = {
  backgroundImage:
    "repeating-linear-gradient(135deg, rgb(var(--text-3) / .7) 0 2px, transparent 2px 5px)",
};
const HATCH_KEY = {
  backgroundImage:
    "repeating-linear-gradient(135deg, rgb(var(--text-3) / .7) 0 1px, transparent 1px 3px)",
};

/** Track order — also the order rows read in, strongest consequence first. */
const CLASS_ORDER: PrivacyRow["class"][] = [
  "cookies",
  "history",
  "session",
  "site_storage",
  "cache",
];

// --- results ---------------------------------------------------------------

type Filter = "offerable" | "withheld" | "all";

function Results({
  report,
  selected,
  onToggle,
  onGrant,
  filter,
  setFilter,
}: {
  report: PrivacyReport;
  selected: Set<string>;
  onToggle: (path: string) => void;
  onGrant: () => void;
  filter: Filter;
  setFilter: (f: Filter) => void;
}) {
  const offerable = report.rows.filter((r) => r.offerable);
  const withheld = report.rows.filter((r) => !r.offerable);
  const [figure, unit] = splitBytes(report.offerable_bytes);

  const present = report.browsers.filter(
    (b) =>
      b.access === "needs_full_disk_access" ||
      b.access === "unreadable" ||
      report.rows.some((r) => r.browser === b.id),
  );
  const absent = report.browsers.filter((b) => !present.includes(b));

  if (report.rows.length === 0 && !present.some((b) => b.access !== "readable")) {
    return <Empty browsers={report.browsers} />;
  }

  const shown =
    filter === "all"
      ? report.rows
      : report.rows.filter((r) => r.offerable === (filter === "offerable"));

  return (
    <>
      <div className="flex items-end justify-between gap-6">
        <div className="min-w-0">
          <p className="text-subtle truncate text-micro font-semibold uppercase">
            Browser data
          </p>
          <h2 className="mt-1 text-title font-semibold">
            {offerable.length.toLocaleString()} item
            {offerable.length === 1 ? "" : "s"} to review
          </h2>
          {/* "shown" is only true when they are on screen. Under a filter
              that hides them, saying so is a small lie the screen tells about
              itself — and it reached the aria-label too, so it was a lie told
              to screen-reader users in the same breath. */}
          {withheld.length > 0 && (
            <p className="text-muted text-caption">
              {withheld.length.toLocaleString()} more
              {filter === "all" ? " shown but" : ""} not offered
            </p>
          )}
        </div>
        <div className="flex flex-none flex-col items-end">
          <p className="whitespace-nowrap font-mono tabular-nums">
            <span className="text-display font-semibold">{figure}</span>
            <span className="text-muted ml-2 text-emph font-medium">{unit}</span>
            {report.partial && (
              <span className="text-muted ml-2 font-sans text-body">or more</span>
            )}
          </p>
          <span className="text-muted mt-1 inline-flex items-center gap-1.5 rounded-full border border-separator px-2 py-0.5 text-micro font-semibold uppercase">
            <span
              className="h-[7px] w-[7px] rounded-full bg-success"
              aria-hidden="true"
            />
            Preview only
          </span>
          {report.partial && (
            <p className="text-subtle mt-1 max-w-[22rem] text-right text-caption">
              <FloorNote report={report} />
            </p>
          )}
        </div>
      </div>

      <Track rows={report.rows} filter={filter} />

      {/* Without this the first four rows on a real machine are all locked and
          not one of the five actionable ones is above the fold: website storage
          and anything a running browser holds open outnumber what is on offer.
          Defaulting to the offerable rows makes the screen open on what the
          user can do, without hiding the rest behind a decision they have to
          discover. */}
      {withheld.length > 0 && (
        <div className="mt-4 flex items-center gap-3">
          <Segmented
            label="Which rows to show"
            value={filter}
            onChange={setFilter}
            options={[
              { value: "offerable", label: `Offered ${offerable.length}` },
              { value: "withheld", label: `Not offered ${withheld.length}` },
              { value: "all", label: "All" },
            ]}
          />
        </div>
      )}

      <ColumnHeader />

      {present.map((b) => (
        <BrowserSection
          key={b.id}
          browser={b}
          all={report.rows.filter((r) => r.browser === b.id)}
          rows={shown.filter((r) => r.browser === b.id)}
          selected={selected}
          onToggle={onToggle}
          onGrant={onGrant}
          onShowHidden={() => setFilter("all")}
        />
      ))}

      <Footnotes report={report} absent={absent} />
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
 * A stacked track by *consequence*, with the shown-but-not-offered share
 * hatched on the same bar.
 *
 * The question this module asks is not "where does the space go" — that is
 * Space Lens — but "what kind of traces am I about to remove, and how much of
 * what I can see is even on offer". One bar answers both.
 */
function Track({ rows, filter }: { rows: PrivacyRow[]; filter: Filter }) {
  // The same conditional the subhead uses. These two strings are the only form
  // of the claim a sighted reviewer cannot check by looking, which is exactly
  // why they were the pair that stayed wrong when the visible copy was fixed.
  const shownBut = filter === "all" ? "shown but " : "";
  const offerable = rows.filter((r) => r.offerable);
  const withheldBytes = rows
    .filter((r) => !r.offerable)
    .reduce((n, r) => n + r.size_bytes, 0);

  const segments = CLASS_ORDER.map((c) => ({
    cls: c,
    bytes: offerable
      .filter((r) => r.class === c)
      .reduce((n, r) => n + r.size_bytes, 0),
  })).filter((s) => s.bytes > 0);
  const offered = segments.reduce((n, s) => n + s.bytes, 0) || 1;

  // The track is scaled to what is **on offer**, not to everything seen.
  //
  // Scaled to the total, a real machine gives the withheld share 92% of the bar
  // — website storage alone is usually the largest thing here — and the
  // consequence the screen is about comes out three pixels wide. That clears
  // the minimum-width rule on a technicality and communicates nothing. So the
  // bar answers "of what I can act on, what kind is it", and the part that is
  // not on offer is a fixed tail with its real figure in the key beside it.
  return (
    <div className="mt-4">
      <div className="flex items-center gap-2">
        <div
          className="flex h-2 flex-1 gap-px overflow-hidden rounded-full bg-white/[.05]"
          role="img"
          aria-label={`${formatBytes(offered)} offered across ${segments.length} kinds; ${formatBytes(withheldBytes)} ${shownBut}not offered`}
        >
          {segments.map((s) => (
            <span
              key={s.cls}
              className={`block h-full ${CLASS_HUES[s.cls].fill}`}
              style={{ width: `${(s.bytes / offered) * 100}%`, minWidth: "3px" }}
              title={`${CLASS_LABELS[s.cls]}: ${formatBytes(s.bytes)}`}
            />
          ))}
        </div>
        {withheldBytes > 0 && (
          <>
            {/* An axis break. The tail is a fixed width, not a share of the
                bar, and without the convention that says so it could be read
                as proportional — which is the misreading the rescale was
                meant to end, arriving from the other side. */}
            <span
              className="text-subtle flex-none select-none text-caption leading-none"
              aria-hidden="true"
            >
              ⁄⁄
            </span>
            <span
              className="block h-2 w-10 flex-none rounded-full"
              style={HATCH}
              title={`${shownBut ? "Shown but not" : "Not"} offered: ${formatBytes(withheldBytes)}`}
              aria-hidden="true"
            />
          </>
        )}
      </div>
      <ul className="text-muted mt-2 flex flex-wrap gap-x-4 gap-y-1 text-caption">
        {segments.map((s) => (
          <li key={s.cls} className="flex items-center gap-1.5">
            <span className={`flex-none ${CLASS_HUES[s.cls].text}`}>
              <ClassIcon cls={s.cls} size={13} />
            </span>
            {CLASS_LABELS[s.cls]}
            <span className="font-mono tabular-nums">
              {formatBytes(s.bytes)}
            </span>
          </li>
        ))}
        {withheldBytes > 0 && (
          <li className="flex items-center gap-1.5">
            <span
              className="h-2 w-3.5 flex-none rounded-[2px]"
              style={HATCH_KEY}
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

/**
 * The floor caption beside the hero.
 *
 * There used to be a banner here saying the same thing as the Safari card
 * eighty pixels below it, with a second button under a different name — two
 * hundred pixels of viewport, a quarter of the window, spent saying one thing
 * twice. The card stays, because it is where a reader looks for Safari and it
 * owns the action. All that was unique to the banner was *why the headline
 * figure is a floor*, and that belongs next to the figure.
 */
function FloorNote({ report }: { report: PrivacyReport }) {
  const denied = report.browsers.filter(
    (b) => b.access === "needs_full_disk_access" || b.access === "unreadable",
  );
  if (denied.length > 0) {
    return (
      <>
        {list(denied.map((b) => b.name))} not searched — needs Full Disk Access
      </>
    );
  }
  if (report.skipped_symlink > 0) {
    return <>{report.skipped_symlink} item(s) were links and were not followed</>;
  }
  return <>some of this could not be read</>;
}

function ColumnHeader() {
  return (
    <div className="text-subtle mt-4 flex items-center gap-3 px-4 text-micro font-semibold uppercase">
      <span className="w-[14px] flex-none" aria-hidden="true" />
      <span className="flex-1">What it is</span>
      <span className="hidden w-[84px] flex-none text-right md:block">Files</span>
      <span className="w-[76px] flex-none text-right">Size</span>
    </div>
  );
}

function BrowserSection({
  browser,
  all,
  rows,
  selected,
  onToggle,
  onGrant,
  onShowHidden,
}: {
  browser: PrivacyBrowser;
  /** Every row this browser has, whatever the filter shows. */
  all: PrivacyRow[];
  rows: PrivacyRow[];
  selected: Set<string>;
  onToggle: (path: string) => void;
  onGrant: () => void;
  onShowHidden: () => void;
}) {
  const order = (a: PrivacyRow, b: PrivacyRow) =>
    // Offerable first. Under All, class order alone put five locked rows ahead
    // of three actionable ones and pushed the actionable ones back below the
    // fold — the problem the filter exists to solve, returning by another door.
    Number(b.offerable) - Number(a.offerable) ||
    CLASS_ORDER.indexOf(a.class) - CLASS_ORDER.indexOf(b.class) ||
    (a.profile ?? "").localeCompare(b.profile ?? "") ||
    a.path.localeCompare(b.path);
  const ordered = [...rows].sort(order);
  const hidden = all.filter((r) => !rows.includes(r)).sort(order);

  // A reason that applies to several rows is said once. Without it, "Google
  // Chrome looks like it is running (…SingletonLock is present), and it would
  // write this back" renders on three consecutive rows, costing each of them
  // two extra lines and — at 720px — turning the group into four lines of the
  // same sentence.
  const shared = sharedReasons(ordered);

  // Counted over *all* the browser's rows, not the visible subset: a chip that
  // followed the filter would quietly restate the filter as if it were a fact
  // about the browser.
  const counts = CLASS_ORDER.map((c) => ({
    cls: c,
    n: all.filter((r) => r.class === c).length,
  })).filter((c) => c.n > 0);

  return (
    <section className="mt-5 first-of-type:mt-3">
      <div className="mb-2 px-4">
        <h3 className="flex flex-wrap items-center gap-x-2 gap-y-1">
          <span className="text-muted text-micro font-semibold uppercase">
            {browser.name}
          </span>
          {browser.profiles > 1 && (
            <span className="text-subtle text-micro normal-case tracking-normal">
              {browser.profiles} profiles
            </span>
          )}
          {browser.may_be_live && (
            <span className="text-muted rounded-full bg-white/[.05] px-2 text-micro font-semibold uppercase leading-4">
              looks like it is running
            </span>
          )}
          {/* The by-consequence read, without regrouping the list. The digits
              stay in text colour: a category hue is for a graphic, never for
              text carrying its own meaning. */}
          {counts.map((c) => (
            <span
              key={c.cls}
              className="text-muted flex items-center gap-1 text-micro normal-case tracking-normal"
              title={`${c.n} ${CLASS_LABELS[c.cls]}`}
            >
              <span className={`flex-none ${CLASS_HUES[c.cls].text}`}>
                <ClassIcon cls={c.cls} size={12} />
              </span>
              {c.n}
            </span>
          ))}
        </h3>
        {shared.length > 0 && (
          <ul className="text-muted mt-1 space-y-0.5 text-caption leading-snug">
            {shared.map((reason) => (
              <li key={reason} className="flex gap-1.5">
                <span className="text-subtle mt-px flex-none">
                  <LockIcon size={11} />
                </span>
                <span>{tildeAll(reason)}</span>
              </li>
            ))}
          </ul>
        )}
      </div>

      {browser.access === "needs_full_disk_access" ? (
        <Group>
          <div className="flex items-start gap-3 px-4 py-3.5">
            <span className="text-subtle mt-px flex-none">
              <LockIcon size={14} />
            </span>
            <div className="min-w-0 flex-1">
              <p className="text-body">
                macOS will not let this app read {browser.name}&apos;s data
                without Full Disk Access.
              </p>
              <p className="text-muted mt-0.5 text-caption">
                Nothing here has been searched, so nothing is claimed about it
                — and the figure above is a floor.
              </p>
            </div>
            <button
              className="flex-none rounded-control border border-border bg-surface2 px-3 py-1 text-body font-medium text-text transition-colors duration-fast ease-mac"
              onClick={onGrant}
            >
              Open Settings
            </button>
          </div>
        </Group>
      ) : (
        <>
          {ordered.length > 0 && (
            <Group role="list" label={browser.name}>
              {ordered.map((row, i) => (
                <Row
                  key={row.path}
                  row={row}
                  first={i === 0}
                  multiProfile={browser.profiles > 1}
                  hideReason={
                    row.withheld !== null && shared.includes(row.withheld)
                  }
                  checked={selected.has(row.path)}
                  onToggle={() => onToggle(row.path)}
                />
              ))}
            </Group>
          )}
          {/* Say what the filter is holding back, and name it.
              Leaving this to the "looks like it is running" chip covered one
              browser and one reason: Firefox's 66 MiB of website storage
              disappeared with no trace at all, and Safari — with nothing
              offered — got a full explanatory card, so one screen had three
              different answers to "why isn't it here". */}
          {hidden.length > 0 && (
            <button
              onClick={onShowHidden}
              className="text-subtle mt-1.5 flex items-center gap-1 rounded-control px-4 text-caption transition-colors duration-fast ease-mac hover:text-muted focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accentText"
            >
              <ChevronIcon size={11} />
              {hidden.length} not offered —{" "}
              {list(
                Array.from(new Set(hidden.map((r) => CLASS_LABELS[r.class]))).map(
                  (l) => l.toLowerCase(),
                ),
              )}
            </button>
          )}
        </>
      )}
    </section>
  );
}

/**
 * Every withheld reason two or more of these rows share.
 *
 * Not just the commonest: a browser that is running *and* holds website storage
 * has two shared reasons, and hoisting only the winner leaves the other one
 * repeating down the rows — the exact problem this exists to solve, half
 * solved, with the tie broken by map insertion order.
 */
function sharedReasons(rows: PrivacyRow[]): string[] {
  const counts = new Map<string, number>();
  for (const r of rows) {
    if (r.withheld) counts.set(r.withheld, (counts.get(r.withheld) ?? 0) + 1);
  }
  return [...counts.entries()].filter(([, n]) => n > 1).map(([reason]) => reason);
}

/** The path with the home directory folded to `~`. */
function tilde(path: string): string {
  const m = path.match(/^\/Users\/[^/]+(\/.*)?$/);
  return m ? `~${m[1] ?? ""}` : path;
}

/**
 * The same fold, inside a sentence.
 *
 * A withheld reason from the backend names the marker that caused it — the
 * whole point being that the user can go and look at the stale lock file — and
 * it arrives as an absolute path in the middle of prose.
 */
function tildeAll(text: string): string {
  return text.replace(/\/Users\/[^/\s]+\//g, "~/");
}

function Row({
  row,
  first,
  multiProfile,
  hideReason,
  checked,
  onToggle,
}: {
  row: PrivacyRow;
  first: boolean;
  multiProfile: boolean;
  hideReason: boolean;
  checked: boolean;
  onToggle: () => void;
}) {
  const h = CLASS_HUES[row.class];
  const tag = CONSEQUENCES[row.consequence].tag;
  // The reason earns the line only when it is this row's own; a reason the
  // whole group shares is said once, above.
  const line =
    row.withheld && !hideReason
      ? tildeAll(row.withheld)
      : multiProfile && row.profile
        ? row.profile
        : tilde(row.path);

  const body = (
    <>
      <span className="min-w-0 flex-1">
        <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
          <span className="flex min-w-0 items-center gap-2">
            <span
              className={`flex-none ${row.offerable ? h.text : "text-subtle"}`}
              aria-hidden="true"
            >
              <ClassIcon cls={row.class} size={14} />
            </span>
            <span
              className={`truncate text-body font-medium ${row.offerable ? "text-text" : "text-muted"}`}
            >
              {row.label}
            </span>
          </span>
          {tag && (
            <span className="text-muted flex-none rounded-full bg-white/[.05] px-2 text-micro font-semibold uppercase leading-4">
              {tag}
            </span>
          )}
        </span>
        <span
          className="text-muted line-clamp-2 block pl-[22px] text-caption"
          title={line}
        >
          {line}
        </span>
      </span>
      <span
        className={`hidden w-[84px] flex-none text-right font-mono text-caption tabular-nums md:block ${row.offerable ? "text-muted" : "text-subtle"}`}
      >
        {row.is_dir
          ? `${row.file_count.toLocaleString()} file${row.file_count === 1 ? "" : "s"}`
          : `${row.member_count.toLocaleString()} file${row.member_count === 1 ? "" : "s"}`}
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

  // A withheld row recedes rather than being raised. Applications lifts them
  // onto a plate because they are the exception there; here they are usually
  // the majority, and lifting the majority puts the emphasis on what the user
  // cannot do.
  if (!row.offerable) {
    return (
      <div
        role="listitem"
        className={`flex items-center gap-3 px-4 py-3 opacity-90 ${first ? "" : "border-t border-separator"}`}
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
        label={`Select ${row.label}`}
      />
      {body}
    </label>
  );
}

/** What was looked at and found nothing, and what another screen cleans. */
function Footnotes({
  report,
  absent,
}: {
  report: PrivacyReport;
  absent: PrivacyBrowser[];
}) {
  const notes = report.browsers.flatMap((b) => b.notes);
  return (
    <div className="text-subtle mt-6 space-y-1.5 text-caption">
      {absent.length > 0 && (
        <p>
          Also looked at: {list(absent.map((b) => b.name))} — nothing found.
        </p>
      )}
      {report.covered_elsewhere.length > 0 && (
        <p>
          On-disk browser caches are cleaned by <b>Cleanup</b>, under
          Application caches, so they are not listed twice here.
        </p>
      )}
      {notes.map((n) => (
        <p key={n}>{n}</p>
      ))}
      {report.caveats.map((c) => (
        <p key={c}>{c}</p>
      ))}
    </div>
  );
}

function list(names: string[]): string {
  if (names.length <= 1) return names[0] ?? "";
  return `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
}

// --- action bar and sheet --------------------------------------------------

function ActionBar({
  count,
  bytes,
  onReview,
  disabled,
}: {
  count: number;
  bytes: number;
  onReview: () => void;
  disabled: boolean;
}) {
  return (
    <div className="flex flex-none items-center gap-4 border-t border-separator bg-surface2/80 px-5 py-3 backdrop-blur">
      <div className="min-w-0 flex-1">
        <p className="text-body">
          {count === 0 ? (
            <span className="text-muted">Nothing selected</span>
          ) : (
            <>
              <span className="font-mono font-semibold tabular-nums">
                {count.toLocaleString()}
              </span>{" "}
              selected ·{" "}
              <span className="font-mono font-semibold tabular-nums">
                {formatBytes(bytes)}
              </span>
            </>
          )}
        </p>
        <p className="text-subtle text-caption">
          Tick each item you want gone. mac-cleaner never picks these for you.
        </p>
      </div>
      <button
        className="flex-none whitespace-nowrap rounded-control bg-accent px-4 py-2 text-body font-semibold text-white transition-colors duration-fast ease-mac disabled:cursor-not-allowed disabled:border disabled:border-separator disabled:bg-surface2 disabled:text-subtle"
        onClick={onReview}
        disabled={disabled}
      >
        {count === 0
          ? "Move to Trash…"
          : `Move ${count.toLocaleString()} item${count === 1 ? "" : "s"} to Trash…`}
      </button>
    </div>
  );
}

/**
 * The confirmation sheet, and the one screen in this app that asks for more
 * than a yes.
 *
 * Each consequence in the selection gets its own checkbox, phrased as what the
 * user loses rather than what the app does, and the primary action stays
 * disabled until every one is ticked. This is not belt-and-braces on top of the
 * backend gate: `dispose_privacy` refuses an unacknowledged consequence
 * outright, so a sheet that did not ask would produce a refusal the user could
 * not act on.
 */
function ConfirmModal({
  rows,
  bytes,
  items,
  dirs,
  mass,
  ack,
  setAck,
  live,
  busy,
  error,
  onCancel,
  onConfirm,
}: {
  rows: PrivacyRow[];
  bytes: number;
  items: number;
  dirs: number;
  mass: boolean;
  ack: Acknowledged;
  setAck: (a: Acknowledged) => void;
  live: boolean;
  busy: boolean;
  error: string;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  // Only the consequences actually in the selection, so the sheet never asks
  // for a promise about something the user did not choose.
  const needed = Array.from(
    new Set(
      rows
        .map((r) => CONSEQUENCES[r.consequence].ack)
        .filter((a): a is keyof Acknowledged => a !== null),
    ),
  );
  const ready = needed.every((k) => ack[k]);
  const shown = rows.slice(0, 200);
  const more = rows.length - shown.length;

  return (
    <div
      className={`overlay-in fixed inset-0 z-10 flex items-center justify-center bg-black/60 p-6 ${OVERLAY}`}
      role="dialog"
      aria-modal="true"
      aria-labelledby="pv-confirm-title"
    >
      <div className="sheet-in flex max-h-full w-full max-w-md flex-col rounded-panel border border-separator bg-surface3 p-6 shadow-e3">
        <div className="flex items-start gap-3">
          <span
            className={`grid h-9 w-9 flex-none place-items-center rounded-card ${
              error
                ? "bg-danger/[.16] text-danger"
                : "bg-cat-browser/[.16] text-cat-browser"
            }`}
          >
            {error ? <InfoIcon size={18} /> : <MaskIcon size={18} />}
          </span>
          <div className="min-w-0">
            <h2 id="pv-confirm-title" className="text-title font-semibold">
              {error ? (
                "Nothing was removed"
              ) : (
                <>
                  Move {rows.length.toLocaleString()} item
                  {rows.length === 1 ? "" : "s"} to the Trash?
                </>
              )}
            </h2>
            {/* After a refusal the figure must not read as an amount that
                went: it is what stayed. */}
            <p className="text-muted mt-1 text-body">
              {error ? (
                <>
                  <span className="font-mono font-semibold tabular-nums text-text">
                    {formatBytes(bytes)}
                  </span>{" "}
                  was left exactly where it was.
                </>
              ) : (
                <>
                  <span className="font-mono font-semibold tabular-nums text-text">
                    {formatBytes(bytes)}
                  </span>{" "}
                  of browser data
                  {items > rows.length && (
                    <>
                      {" "}
                      —{" "}
                      <span className="font-mono tabular-nums text-text">
                        {items.toLocaleString()}
                      </span>{" "}
                      files in all
                    </>
                  )}
                  .
                </>
              )}
            </p>
          </div>
        </div>

        <div className="mt-4 min-h-0 flex-1 overflow-y-auto">
          {!error && (
            <>
              {/* What is safe about this, stated before what is not. */}
              <div className="flex gap-2.5 rounded-card border border-success/25 bg-success/[.09] px-3.5 py-2.5">
                <span className="text-success mt-px flex-none">
                  <ShieldIcon size={14} />
                </span>
                <p className="text-muted text-caption leading-relaxed">
                  Everything goes to the Trash, so it can be recovered, and
                  every item is written to the audit log.
                </p>
              </div>

              {(mass || live) && (
                <div className="mt-2 flex gap-2.5 rounded-card border border-warning/30 bg-warning/[.09] px-3.5 py-2.5">
                  <span className="text-warning mt-px flex-none">
                    <InfoIcon size={14} />
                  </span>
                  <ul className="text-muted space-y-1 text-caption leading-relaxed">
                    {dirs > 0 && (
                      <li>
                        {dirs === 1 ? "One item is a folder" : `${dirs} items are folders`}
                        , so this is a <b>recursive removal</b> and needs the
                        extra confirmation below however small it is.
                      </li>
                    )}
                    {live && (
                      <li>
                        A browser looks like it is running. Quit it first, or it
                        may write some of this back.
                      </li>
                    )}
                  </ul>
                </div>
              )}

              {/* The manifest first: consent is to a list, so the list comes
                  before the promises about it. Asking for the promise above the
                  thing being promised is how a sheet becomes a formality. */}
              <p className="text-subtle mt-4 text-micro font-semibold uppercase">
                What will be removed
              </p>
              <ul className="mt-1.5 space-y-1">
                {shown.map((r) => (
                  <li key={r.path} className="flex items-baseline gap-2">
                    <span
                      className={`flex-none translate-y-[2px] ${CLASS_HUES[r.class].text}`}
                      aria-hidden="true"
                    >
                      <ClassIcon cls={r.class} size={12} />
                    </span>
                    <span className="text-muted min-w-0 flex-1 truncate text-caption">
                      {r.browser_name} · {r.label}
                      {r.profile ? ` · ${r.profile}` : ""}
                    </span>
                    <span className="text-subtle flex-none font-mono text-caption tabular-nums">
                      {formatBytes(r.size_bytes)}
                    </span>
                  </li>
                ))}
                {more > 0 && (
                  <li className="text-subtle text-caption">and {more} more</li>
                )}
              </ul>

              {/* The second consent axis, in the user's own terms. */}
              {needed.length > 0 && (
                <fieldset className="mt-4">
                  <legend className="text-subtle mb-1.5 text-micro font-semibold uppercase">
                    Confirm what you lose
                  </legend>
                  <div className="space-y-2">
                    {needed.map((k) => (
                      <label
                        key={k}
                        className="flex cursor-pointer items-start gap-2.5 rounded-card border border-separator bg-surface px-3 py-2.5"
                      >
                        <span className="mt-px flex-none">
                          <Checkbox
                            checked={ack[k]}
                            onChange={() => setAck({ ...ack, [k]: !ack[k] })}
                            label={CONSEQUENCES[ackConsequence(k)].short}
                          />
                        </span>
                        <span
                          className={`mt-px flex-none ${CLASS_HUES[ackClass(k)].text}`}
                          aria-hidden="true"
                        >
                          <ClassIcon cls={ackClass(k)} size={14} />
                        </span>
                        <span className="text-body leading-snug">
                          {CONSEQUENCES[ackConsequence(k)].sentence}
                        </span>
                      </label>
                    ))}
                  </div>
                  {/* The primary is disabled until these are ticked, and a
                      control that refuses without saying why is a dead end. */}
                  <p className="text-subtle mt-1.5 text-caption">
                    Tick each box to enable <b>Move to Trash</b>.
                  </p>
                </fieldset>
              )}
            </>
          )}

          {error && (
            <div className="rounded-card border border-danger/30 bg-danger/[.07] px-3.5 py-3">
              {/* The backend's own sentence, with its machine prefix taken off
                  — "refused:" is for the audit log, not for a person. */}
              <p className="text-body">{sentence(error)}</p>
              <p className="text-muted mt-1 text-caption">
                The list is out of date now, so the only honest next step is to
                look again.
              </p>
            </div>
          )}
        </div>

        <div className="mt-5 flex flex-none justify-end gap-2">
          <button className="rounded-control px-4 py-2 text-body font-medium text-muted transition-colors duration-fast ease-mac hover:text-text disabled:text-subtle" onClick={onCancel} disabled={busy}>
            {error ? "Look again" : "Cancel"}
          </button>
          {!error && (
            <button
              className="rounded-control bg-accent px-4 py-2 text-body font-semibold text-white transition-colors duration-fast ease-mac disabled:cursor-not-allowed disabled:border disabled:border-border disabled:bg-surface2 disabled:text-muted"
              onClick={onConfirm}
              disabled={busy || !ready}
            >
              {busy ? "Removing…" : "Move to Trash"}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

/**
 * The backend's refusal, addressed to a person.
 *
 * `refused:` is a prefix for the audit log; leaving it on screen leaks the
 * machine's own bookkeeping into the one sentence a worried user will read
 * most carefully.
 */
function sentence(raw: string): string {
  const s = raw.replace(/^refused:\s*/i, "").trim();
  return s.charAt(0).toUpperCase() + s.slice(1);
}

/** The consequence a given acknowledgement authorizes. */
function ackConsequence(k: keyof Acknowledged): PrivacyRow["consequence"] {
  return k as PrivacyRow["consequence"];
}

/** And the class that wears its glyph. */
function ackClass(k: keyof Acknowledged): PrivacyRow["class"] {
  return {
    signs_you_out: "cookies",
    erases_history: "history",
    loses_open_tabs: "session",
  }[k] as PrivacyRow["class"];
}

// --- terminal states -------------------------------------------------------

function Done({
  summary,
  removed,
  onAgain,
}: {
  summary: CleanSummary;
  removed: PrivacyRow[];
  onAgain: () => void;
}) {
  const files = summary.entries_freed + summary.executed;
  // The headline is what changed for the user, not how many bytes it was. This
  // is the screen that argues size is not the point; ending on a size figure
  // would undo the argument at the last step.
  const kinds = Array.from(new Set(removed.map((r) => r.class))).sort(
    (a, b) => CLASS_ORDER.indexOf(a) - CLASS_ORDER.indexOf(b),
  );
  const said = kinds.map((c) => CONSEQUENCES[classConsequence(c)].past);
  return (
    <Outcome
      tone="success"
      icon={<ShieldIcon size={20} />}
      title={said.length > 0 ? said.join(" · ") : "Removed"}
      body={
        <>
          {files.toLocaleString()} file{files === 1 ? "" : "s"} —{" "}
          <span className="font-mono tabular-nums">
            {formatBytes(summary.bytes_freed)}
          </span>{" "}
          — moved to the Trash, where they can still be recovered. Every one is
          in the audit log.
          {summary.refused > 0 && (
            <>
              {" "}
              {summary.refused} item{summary.refused === 1 ? " was" : "s were"}{" "}
              refused at the last check and left alone.
            </>
          )}
        </>
      }
      action={{ label: "Look again", onClick: onAgain }}
    />
  );
}

/** The consequence a class carries. Mirrors `Class::consequence` in the core. */
function classConsequence(c: PrivacyRow["class"]): PrivacyRow["consequence"] {
  return {
    cookies: "signs_you_out",
    history: "erases_history",
    session: "loses_open_tabs",
    site_storage: "loses_site_data",
    cache: "regenerable",
  }[c] as PrivacyRow["consequence"];
}

function Empty({ browsers }: { browsers: PrivacyBrowser[] }) {
  const installed = browsers.filter((b) => b.access !== "not_installed");
  return (
    <Outcome
      tone="neutral"
      icon={<MaskIcon size={20} />}
      title="Nothing to clear"
      body={
        installed.length > 0
          ? `${list(installed.map((b) => b.name))} left nothing this screen offers.`
          : "No browser data was found on this Mac."
      }
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
  figure,
  body,
  action,
}: {
  tone: "success" | "neutral" | "danger";
  icon: ReactNode;
  title: string;
  figure?: string;
  body: ReactNode;
  action?: { label: string; onClick: () => void };
}) {
  const tiles = {
    success: "bg-success/[.14] text-success",
    neutral: "bg-white/[.06] text-muted",
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
        {figure && (
          <p className="mt-3 font-mono text-display font-semibold tabular-nums">
            {figure}
          </p>
        )}
        <h2 className={`text-title font-semibold ${figure ? "mt-1" : "mt-3"}`}>
          {title}
        </h2>
        <p className="text-muted mx-auto mt-1.5 max-w-sm text-body leading-relaxed">
          {body}
        </p>
        {action && (
          <button className="mt-4 rounded-control border border-border bg-surface2 px-3 py-1 text-body font-medium text-text transition-colors duration-fast ease-mac disabled:cursor-not-allowed disabled:border-separator disabled:bg-transparent disabled:text-subtle" onClick={action.onClick}>
            {action.label}
          </button>
        )}
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
            Browser data
          </p>
          <h2 className="text-muted mt-1 text-title font-semibold">
            Looking through your browsers…
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
      <div className="mt-4 h-2.5 w-full animate-pulse rounded-full bg-surface2" />
      <Group className="mt-6">
        {[0, 1, 2, 3, 4].map((i) => (
          <div
            key={i}
            className={`flex items-center gap-3 px-4 py-3 ${i ? "border-t border-separator" : ""}`}
          >
            <span className="h-[14px] w-[14px] flex-none animate-pulse rounded-[4px] bg-surface2" />
            <span className="h-3 flex-1 animate-pulse rounded bg-surface2" />
            <span className="h-3 w-[76px] flex-none animate-pulse rounded bg-surface2" />
          </div>
        ))}
      </Group>
    </div>
  );
}
