import { useEffect, useMemo, useState } from "react";
import { call, describeError } from "./backend";
import { Segmented, Checkbox } from "./Controls";
import { FilesIcon, Group, InfoIcon, Toolbar } from "./Shell";
import { formatBytes } from "./format";
import type { CleanSummary, LargeOldItem, LargeOldReport } from "./types";

/**
 * Large & Old Files.
 *
 * The first screen that shows files from outside the cleanup allowlist —
 * `~/Documents`, `~/Downloads`, `~/Desktop` and friends. That makes it the one
 * place in the app where the user can act on something the tool has not vetted,
 * so the whole view is built around a single rule:
 *
 *   **Nothing here is ever pre-selected.**
 *
 * There is no "select all", no default tick, and the primary action is disabled
 * until a human has chosen at least one row. A cache sweep is policy — the tool
 * knows those files regenerate. A file in `~/Documents` is judgement, and the
 * judgement has to be the user's.
 *
 * The other rule is honesty about coverage. The backend reports whether it was
 * truncated, could not read a directory, or skipped hard-linked files, and any
 * of those makes the total a *floor*. Saying so is the same commitment as the
 * under-reporting notice in Cleanup: a figure the user trusts must describe
 * their disk.
 */

type Phase = "none" | "confirm" | "working" | "done";

/** Size floors offered in the toolbar. Values are bytes. */
const SIZE_CHOICES = [
  { value: "104857600", label: "100 MB" },
  { value: "524288000", label: "500 MB" },
  { value: "1073741824", label: "1 GB" },
];

/** Age floors. `""` means "any age". */
const AGE_CHOICES = [
  { value: "", label: "Any age" },
  { value: "180", label: "6 months" },
  { value: "365", label: "1 year" },
];

/** Mirrors `macclean_core::plan`'s thresholds, only to *disclose* them. */
const MASS_COUNT = 100;
const MASS_BYTES = 5 * 1024 ** 3;

export default function LargeOldView({
  onTotal,
}: {
  onTotal?: (bytes: number | null) => void;
}) {
  const [minSize, setMinSize] = useState<string>(SIZE_CHOICES[0].value);
  const [olderThan, setOlderThan] = useState<string>("");
  const [report, setReport] = useState<LargeOldReport | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  // The selection is a set of paths, deliberately keyed by the exact string the
  // backend gave us: it refuses anything that is not byte-identical to what it
  // listed, which is what stops a swapped symlink redirecting the action.
  const [selected, setSelected] = useState<Set<string>>(new Set());

  const [phase, setPhase] = useState<Phase>("none");
  const [actionError, setActionError] = useState("");
  const [summary, setSummary] = useState<CleanSummary | null>(null);
  // Bumped to force a fresh walk after files have been moved — the list we were
  // showing describes a disk that has just changed.
  const [reloadKey, setReloadKey] = useState(0);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError("");
    // A re-scan invalidates the selection: the paths it referred to were from
    // the previous list. Keeping ticks across a filter change would let someone
    // confirm a sheet describing rows they can no longer see.
    setSelected(new Set());
    setPhase("none");

    call<LargeOldReport>("large_and_old", {
      minSizeBytes: Number(minSize),
      olderThanDays: olderThan === "" ? null : Number(olderThan),
    })
      .then((r) => {
        if (cancelled) return;
        setReport(r);
        onTotal?.(r.matched_bytes);
      })
      .catch((e) => {
        if (cancelled) return;
        setError(describeError(e));
        setReport(null);
        onTotal?.(null);
      })
      .finally(() => !cancelled && setLoading(false));

    return () => {
      cancelled = true;
    };
  }, [minSize, olderThan, reloadKey, onTotal]);

  const items = report?.items ?? [];
  const selectedItems = useMemo(
    () => items.filter((i) => selected.has(i.path)),
    [items, selected],
  );
  const selectedBytes = selectedItems.reduce((n, i) => n + i.size_bytes, 0);
  const largest = items.length > 0 ? items[0].size_bytes : 0;
  const crossesMassThreshold =
    selectedItems.length > MASS_COUNT || selectedBytes > MASS_BYTES;

  function toggle(path: string) {
    setSelected((s) => {
      const next = new Set(s);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }

  async function dispose() {
    setPhase("working");
    setActionError("");
    try {
      const result = await call<CleanSummary>("dispose_paths", {
        paths: selectedItems.map((i) => i.path),
        // Bind the action to what the sheet actually showed. The backend
        // re-reads every size from disk and refuses if the two disagree.
        expected: { count: selectedItems.length, bytes: selectedBytes },
        // Only ever asserted for a magnitude the sheet has explicitly told the
        // user about — see `MassNotice` in the sheet below. The flag means "the
        // user confirmed a large action", so the UI must not supply it for a
        // magnitude it kept to itself.
        confirmMassDelete: crossesMassThreshold,
      });
      setSummary(result);
      // The figure in the sidebar described a disk that no longer exists.
      // Blanking it until the re-walk lands is the same commitment as removing
      // the sample-data fallback: never show a number that describes no disk.
      onTotal?.(null);
      setPhase("done");
    } catch (e) {
      setActionError(describeError(e));
      setPhase("confirm");
    }
  }

  if (phase === "done" && summary) {
    return (
      <Done
        summary={summary}
        onBack={() => {
          setPhase("none");
          setSummary(null);
          setReloadKey((k) => k + 1);
        }}
      />
    );
  }

  return (
    <div className="flex h-full flex-col">
      <Toolbar title="Large &amp; Old">
        <Segmented
          label="Minimum size"
          value={minSize}
          options={SIZE_CHOICES}
          onChange={setMinSize}
        />
        <Segmented
          label="Not modified in"
          value={olderThan}
          options={AGE_CHOICES}
          onChange={setOlderThan}
        />
      </Toolbar>

      <div className="min-h-0 flex-1 overflow-y-auto px-6 pb-6 pt-5">
        {error && (
          <div
            className="flex items-start gap-3 rounded-card border border-danger/30 bg-danger/[.07] px-4 py-3"
            role="alert"
          >
            <span className="mt-0.5 flex-none text-danger">
              <InfoIcon size={16} />
            </span>
            <div>
              <p className="text-body font-medium">
                Couldn&rsquo;t look for large files
              </p>
              <p className="text-muted mt-1 text-caption">
                {error} Nothing was scanned, and nothing was changed.
              </p>
            </div>
          </div>
        )}

        {loading && <Skeleton />}

        {!loading && !error && report && (
          <>
            {items.length === 0 ? (
              <Empty
                minSize={minSize}
                olderThan={olderThan}
                onLoosen={() => {
                  if (olderThan !== "") setOlderThan("");
                  else if (minSize !== SIZE_CHOICES[0].value)
                    setMinSize(SIZE_CHOICES[0].value);
                  else setReloadKey((k) => k + 1);
                }}
              />
            ) : (
              <>
                {/* The figure the coverage notice qualifies, and the scope it
                    came from — both rehomed here from a separate prose card
                    that pushed the list a third of the way down the window. */}
                <div className="flex items-end justify-between gap-4">
                  <div>
                    <p className="text-subtle text-micro font-semibold uppercase">
                      Documents · Downloads · Desktop · Movies · Music ·
                      Pictures
                    </p>
                    <h2 className="mt-1 text-title font-semibold">
                      {report.matched.toLocaleString()} file
                      {report.matched === 1 ? "" : "s"}
                    </h2>
                  </div>
                  <p className="text-right">
                    <span className="font-mono text-emph font-semibold tabular-nums">
                      {formatBytes(report.matched_bytes)}
                    </span>
                    {report.partial && (
                      <span className="text-muted text-body"> or more</span>
                    )}
                  </p>
                </div>

                <CoverageNotice report={report} />

                <div className="mt-4">
                  <ColumnHeader />
                  <Group>
                    {items.map((item, i) => (
                      <FileRow
                        key={item.path}
                        item={item}
                        largest={largest}
                        first={i === 0}
                        checked={selected.has(item.path)}
                        onToggle={() => toggle(item.path)}
                      />
                    ))}
                  </Group>
                </div>

                {report.matched > items.length && (
                  <p className="text-muted mt-3 text-caption">
                    Showing the {items.length.toLocaleString()} largest of{" "}
                    {report.matched.toLocaleString()}. Raise the size floor to
                    narrow the list.
                  </p>
                )}
              </>
            )}
          </>
        )}
      </div>

      {items.length > 0 && (
        <ActionBar
          count={selectedItems.length}
          total={items.length}
          bytes={selectedBytes}
          onAct={() => {
            setActionError("");
            setPhase("confirm");
          }}
        />
      )}

      {(phase === "confirm" || phase === "working") && (
        <ConfirmModal
          items={selectedItems}
          bytes={selectedBytes}
          mass={crossesMassThreshold}
          busy={phase === "working"}
          error={actionError}
          onCancel={() => setPhase("none")}
          onConfirm={dispose}
        />
      )}
    </div>
  );
}

/** `/Users/someone/Downloads/x` → `~/Downloads/x`. Display only. */
function tilde(path: string): string {
  return path.replace(/^\/Users\/[^/]+\//, "~/");
}

function split(path: string): { dir: string; name: string } {
  const p = tilde(path);
  const slash = p.lastIndexOf("/");
  return slash > 0
    ? { dir: p.slice(0, slash), name: p.slice(slash + 1) }
    : { dir: "", name: p };
}

/**
 * Say when the figure is a floor.
 *
 * Sits directly under the total it qualifies, because that is where the doubt
 * is actionable. Each reason is listed separately: "we found less than there
 * is" and "we are only showing you some of it" are different things to a
 * reader, and the fixes differ too.
 */
function CoverageNotice({ report }: { report: LargeOldReport }) {
  if (!report.partial) return null;

  const reasons: string[] = [];
  if (report.truncated) {
    reasons.push(
      `the search stopped after ${report.examined.toLocaleString()} files, so there may be more`,
    );
  }
  if (report.skipped_unreadable > 0) {
    reasons.push(
      `${report.skipped_unreadable.toLocaleString()} folder${
        report.skipped_unreadable === 1 ? " was" : "s were"
      } unreadable — granting Full Disk Access would include them`,
    );
  }
  if (report.skipped_hardlinked > 0) {
    reasons.push(
      `${report.skipped_hardlinked.toLocaleString()} file${
        report.skipped_hardlinked === 1 ? " is" : "s are"
      } shared under more than one name, so removing them would not free their space`,
    );
  }
  if (report.skipped_unrepresentable > 0) {
    reasons.push(
      `${report.skipped_unrepresentable.toLocaleString()} file${
        report.skipped_unrepresentable === 1 ? " has" : "s have"
      } a name this app cannot handle safely`,
    );
  }
  if (reasons.length === 0) return null;

  return (
    <div className="mt-3 flex items-start gap-3 rounded-card border border-cat-trashes/30 bg-cat-trashes/[.07] px-4 py-3">
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

/**
 * Declares the sort order, which the rows alone cannot.
 *
 * The leading spacer is deliberately *not* a checkbox: a select-all here would
 * contradict the one rule this screen exists to enforce.
 */
function ColumnHeader() {
  return (
    <div className="text-subtle mb-1.5 flex items-center gap-3 px-4 text-micro font-semibold uppercase">
      <span className="w-[14px] flex-none" aria-hidden="true" />
      <span className="flex-1">Name</span>
      <span className="w-[92px] flex-none text-right">Last modified</span>
      <span className="w-[76px] flex-none text-right">Size &darr;</span>
    </div>
  );
}

function FileRow({
  item,
  largest,
  first,
  checked,
  onToggle,
}: {
  item: LargeOldItem;
  largest: number;
  first: boolean;
  checked: boolean;
  onToggle: () => void;
}) {
  const { dir, name } = split(item.path);
  // Proportional to the biggest match, which is the one question a size-ranked
  // list should answer at a glance: which of these actually matters.
  const share =
    largest > 0 ? Math.max(2, (item.size_bytes / largest) * 100) : 0;

  return (
    <label
      className={`flex cursor-pointer items-center gap-3 px-4 py-2.5 transition-colors duration-fast ease-mac ${
        first ? "" : "border-t border-separator"
      } ${checked ? "bg-accentTint" : "hover:bg-surface2"}`}
    >
      <Checkbox
        checked={checked}
        onChange={onToggle}
        label={`Select ${name}`}
      />
      <span className="min-w-0 flex-1">
        <span className="flex items-center gap-2">
          <span
            className="h-[7px] w-[7px] flex-none rounded-full bg-cat-large"
            aria-hidden="true"
          />
          <span className="truncate text-body font-medium text-text">
            {name}
          </span>
        </span>
        <span
          className="text-muted block truncate pl-[15px] text-caption"
          title={dir}
        >
          {dir}
        </span>
        {/* Proportional to the biggest match. The one question a size-ranked
            list should answer at a glance is which of these actually matters,
            and "18.4 GiB" only answers it if you also read the four rows below
            it. Left-aligned under the name so the column keeps a clean edge. */}
        <span
          className="mt-1.5 block h-[3px] max-w-[220px] pl-[15px]"
          aria-hidden="true"
        >
          <span
            className="block h-full rounded-full bg-cat-large/70"
            style={{ width: `${share}%` }}
          />
        </span>
      </span>
      <span className="text-muted w-[92px] flex-none text-right text-caption tabular-nums">
        {formatWhen(item.modified_ms)}
      </span>
      <span className="w-[76px] flex-none text-right font-mono text-body font-semibold tabular-nums text-text">
        {formatBytes(item.size_bytes)}
      </span>
    </label>
  );
}

/** "3y ago" / "8mo ago". An em dash when the mtime could not be read. */
function formatWhen(ms: number | null): string {
  if (ms === null) return "—";
  const days = Math.floor((Date.now() - ms) / 86_400_000);
  if (days < 1) return "today";
  if (days < 30) return `${days}d ago`;
  const months = Math.floor(days / 30);
  if (months < 12) return `${months}mo ago`;
  return `${Math.floor(days / 365)}y ago`;
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
      <div aria-live="polite">
        {count === 0 ? (
          <>
            <p className="text-body font-medium">Nothing selected</p>
            {/* The policy lives here rather than in a card above the list: this
                is where the eye goes when the button will not press. */}
            <p className="text-muted text-caption">
              mac-cleaner never picks these for you.
            </p>
          </>
        ) : (
          <>
            <p className="font-mono text-emph font-semibold tabular-nums">
              {formatBytes(bytes)}
            </p>
            <p className="text-muted text-caption">
              {count.toLocaleString()} of {total.toLocaleString()} files
              selected
            </p>
          </>
        )}
      </div>
      <button
        onClick={onAct}
        disabled={count === 0}
        // A real disabled style rather than a blanket opacity: this button is
        // the default state of the screen, and the argument the screen is
        // making depends on the user being able to read it.
        className="rounded-control bg-accent px-4 py-2 text-body font-semibold text-white transition-colors duration-fast ease-mac disabled:cursor-not-allowed disabled:border disabled:border-separator disabled:bg-surface2 disabled:text-subtle"
      >
        {count === 0
          ? "Move to Trash…"
          : `Move ${count.toLocaleString()} file${count === 1 ? "" : "s"} to Trash…`}
      </button>
    </div>
  );
}

function ConfirmModal({
  items,
  bytes,
  mass,
  busy,
  error,
  onCancel,
  onConfirm,
}: {
  items: LargeOldItem[];
  bytes: number;
  mass: boolean;
  busy: boolean;
  error: string;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const shown = items.slice(0, 5);
  const more = items.length - shown.length;

  return (
    <div
      className="overlay-in fixed inset-0 z-10 flex items-center justify-center bg-black/60 p-6 pl-[256px]"
      role="dialog"
      aria-modal="true"
      aria-labelledby="lo-confirm-title"
    >
      {/* Framed in the module's own hue, not the app's blue, so the two
          confirmation sheets are different objects at a glance rather than the
          same object with one recoloured glyph. */}
      <div className="sheet-in w-full max-w-md rounded-panel border border-cat-large/40 bg-surface3 p-6 shadow-e3">
        <div className="flex items-start gap-3">
          {/* Deliberately NOT the shield. The shield is the app's "we vetted
              this" mark, and this is the one sheet where it has not. */}
          <span className="grid h-9 w-9 flex-none place-items-center rounded-card bg-cat-large/[.16] text-cat-large">
            <FilesIcon size={18} />
          </span>
          <div className="min-w-0">
            <h2 id="lo-confirm-title" className="text-title font-semibold">
              Move {items.length.toLocaleString()} file
              {items.length === 1 ? "" : "s"} to the Trash?
            </h2>
            <p className="text-muted mt-1 text-body">
              <span className="font-mono font-semibold tabular-nums text-text">
                {formatBytes(bytes)}
              </span>{" "}
              of your own files.
            </p>
          </div>
        </div>

        {/* The user chose these one by one, so the sheet names them one by one.
            An aggregate is enough for Cleanup, where consent was per-category. */}
        <ul className="mt-4 max-h-[168px] overflow-y-auto rounded-card border border-separator">
          {shown.map((item, i) => {
            const { dir, name } = split(item.path);
            return (
              <li
                key={item.path}
                className={`flex items-center gap-3 px-3.5 py-2 ${
                  i === 0 ? "" : "border-t border-separator"
                }`}
              >
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-caption font-medium text-text">
                    {name}
                  </span>
                  <span className="text-subtle block truncate text-micro normal-case tracking-normal">
                    {dir}
                  </span>
                </span>
                <span className="text-muted flex-none font-mono text-caption tabular-nums">
                  {formatBytes(item.size_bytes)}
                </span>
              </li>
            );
          })}
          {more > 0 && (
            <li className="text-muted border-t border-separator px-3.5 py-2 text-caption">
              and {more.toLocaleString()} more
            </li>
          )}
        </ul>

        <div className="mt-4 flex gap-2.5 rounded-card border border-cat-trashes/45 bg-cat-trashes/[.12] px-3.5 py-3">
          <span className="mt-px flex-none text-cat-trashes">
            <InfoIcon size={15} />
          </span>
          <div className="text-muted text-body">
            <p>
              These are{" "}
              <strong className="font-semibold text-text">
                your own files
              </strong>
              , not caches — nothing will recreate them. They go to the Trash
              and are recorded in the audit log, so you can put them back.
            </p>
            {/* The threshold the backend enforces, disclosed rather than
                silently satisfied on the user's behalf. */}
            {mass && (
              <p className="mt-2">
                This is a{" "}
                <strong className="font-semibold text-text">
                  large action
                </strong>{" "}
                — over {MASS_COUNT} files or {formatBytes(MASS_BYTES)} — so it
                needs the extra confirmation you are giving now.
              </p>
            )}
          </div>
        </div>

        {error && (
          <p className="mt-3 text-body text-danger" role="alert">
            {error}
          </p>
        )}

        <div className="mt-6 flex items-center justify-end gap-1">
          <button
            onClick={onCancel}
            disabled={busy}
            className="rounded-control px-4 py-2 text-body font-medium text-muted transition-colors duration-fast ease-mac hover:text-text disabled:opacity-40"
          >
            Cancel
          </button>
          <button
            onClick={onConfirm}
            disabled={busy}
            className="rounded-control bg-accent px-4 py-2 text-body font-semibold text-white transition-colors duration-fast ease-mac disabled:opacity-60"
          >
            {busy ? "Moving…" : "Move to Trash"}
          </button>
        </div>
      </div>
    </div>
  );
}

function Done({
  summary,
  onBack,
}: {
  summary: CleanSummary;
  onBack: () => void;
}) {
  return (
    <div className="flex h-full items-center justify-center px-6">
      <Group className="w-full max-w-sm">
        <div className="flex flex-col items-center px-6 py-8 text-center">
          <span className="grid h-11 w-11 place-items-center rounded-card bg-cat-large/[.16] text-cat-large">
            <FilesIcon size={20} />
          </span>
          <p className="mt-4 font-mono text-display font-semibold tabular-nums">
            {formatBytes(summary.bytes_freed)}
          </p>
          <p className="text-muted mt-1 text-body">
            {summary.executed.toLocaleString()} file
            {summary.executed === 1 ? "" : "s"} moved to the Trash
            {summary.refused > 0 &&
              `, ${summary.refused.toLocaleString()} refused`}
            .
          </p>
          <p className="text-subtle mt-3 text-caption leading-relaxed">
            Recorded in the audit log. Recover anything from the Trash if you
            change your mind.
          </p>
          <button
            onClick={onBack}
            className="mt-5 rounded-control border border-border bg-surface2 px-4 py-2 text-body font-medium text-text transition-colors duration-fast ease-mac"
          >
            Look again
          </button>
        </div>
      </Group>
    </div>
  );
}

function Empty({
  minSize,
  olderThan,
  onLoosen,
}: {
  minSize: string;
  olderThan: string;
  onLoosen: () => void;
}) {
  const size =
    SIZE_CHOICES.find((c) => c.value === minSize)?.label ?? "that size";
  const aged = olderThan !== "";
  const smaller = minSize !== SIZE_CHOICES[0].value;

  // Advice the user can actually act on: never "try a smaller floor" when they
  // are already at the smallest one.
  const advice = aged
    ? "Nothing that old. Setting the age back to “Any age” would widen the search."
    : smaller
      ? `Nothing over ${size}. A smaller size floor would widen the search.`
      : `No files over ${size} anywhere in your documents, downloads or media folders.`;

  return (
    <Group>
      <div className="flex flex-col items-center px-6 py-10 text-center">
        <span className="grid h-11 w-11 place-items-center rounded-card bg-cat-large/[.16] text-cat-large">
          <FilesIcon size={20} />
        </span>
        <h2 className="mt-4 text-title font-semibold">Nothing to review</h2>
        <p className="text-muted mx-auto mt-1.5 max-w-sm text-body">{advice}</p>
        <button
          onClick={onLoosen}
          className="mt-5 rounded-control border border-border bg-surface2 px-4 py-2 text-body font-medium text-text transition-colors duration-fast ease-mac"
        >
          {aged
            ? "Include any age"
            : smaller
              ? "Lower to 100 MB"
              : "Look again"}
        </button>
      </div>
    </Group>
  );
}

function Skeleton() {
  return (
    <div aria-hidden="true">
      <div className="flex items-end justify-between">
        <div>
          <div className="h-[11px] w-56 animate-pulse rounded bg-surface2" />
          <div className="mt-2 h-[18px] w-24 animate-pulse rounded bg-surface2" />
        </div>
        <div className="h-[18px] w-20 animate-pulse rounded bg-surface2" />
      </div>
      <div className="mt-7 flex flex-col gap-px overflow-hidden rounded-card">
        {[0, 1, 2, 3, 4].map((i) => (
          <div key={i} className="h-[54px] animate-pulse bg-surface2" />
        ))}
      </div>
    </div>
  );
}
