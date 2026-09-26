import { useEffect, useState } from "react";
import { formatBytes } from "./format";
import type {
  CategorySummary,
  EmptyTrashRequest,
  EmptyTrashSummary,
  TrashContents,
} from "./types";
import { call, describeError } from "./backend";
import { Checkbox } from "./Controls";
import { Group, InfoIcon, LockIcon, NoUndoIcon } from "./Shell";
import { hue } from "./hues";

/**
 * Empty Trash: the second, permanent stage after a recoverable clean.
 *
 * Everything else Swept does moves files *into* the Trash, so a mistake can be
 * put back. This is the one place it removes things for good, and the screen is
 * built so that it cannot be confused with the first stage:
 *
 * - The Trash is never a checkbox row. It sits in its own group with its own
 *   button, so it can never ride along with a recoverable clean in one click.
 * - The dialog reads the Trash fresh when it opens rather than reusing the
 *   (filtered) scan row, and binds the request to exactly those figures.
 * - A required, never-pre-ticked acknowledgement gates the button. The backend
 *   refuses without it either way.
 */

/** The registry id of the user-Trash category. */
export const TRASH_CATEGORY = "trash";

/** The Trash as a row: figures and a button, never a checkbox. */
export function TrashRow({
  cat,
  onEmpty,
}: {
  cat: CategorySummary;
  onEmpty: () => void;
}) {
  return (
    <section aria-labelledby="bin-label" className="mt-5">
      <h3
        id="bin-label"
        className="text-subtle mb-1.5 px-1 text-micro font-semibold uppercase"
      >
        In the Trash
      </h3>
      <Group>
        <div className="flex items-center gap-3 px-4 py-3">
          {/* Where the list rows' checkbox sits, so the names line up. There
            is deliberately no checkbox here. */}
          <span className="w-[14px] flex-none" aria-hidden="true" />
          <span
            className="h-2 w-2 flex-none rounded-full"
            style={{ background: hue(cat.category) }}
            aria-hidden="true"
          />
          <div className="min-w-0 flex-1">
            <span className="truncate text-body font-medium">{cat.name}</span>
            <p className="text-subtle mt-0.5 truncate text-caption">
              Emptying it is permanent, so it is never part of a clean.
            </p>
          </div>
          <div className="shrink-0 text-right">
            <span className="block font-mono text-body font-semibold tabular-nums">
              {formatBytes(cat.bytes)}
            </span>
            <span className="text-subtle mt-0.5 block font-mono text-caption tabular-nums">
              {cat.count.toLocaleString()} file{cat.count === 1 ? "" : "s"}
            </span>
          </div>
          <button
            onClick={onEmpty}
            className="ml-1 h-7 shrink-0 rounded-control border border-danger/70 px-3 text-body font-medium text-danger transition-colors duration-fast ease-mac hover:bg-danger/10"
          >
            Empty Trash…
          </button>
        </div>
      </Group>
    </section>
  );
}

/**
 * The stage-2 offer on the result screen. Silent if the Trash cannot be read
 * or the probe fails — an optional next step must never turn into an error on
 * a screen reporting success.
 */
export function EmptyTrashPrompt({ onEmpty }: { onEmpty: () => void }) {
  const [c, setC] = useState<TrashContents | null>(null);
  useEffect(() => {
    void call<TrashContents>("trash_contents")
      .then(setC)
      .catch(() => setC(null));
  }, []);
  if (!c) return null;
  if (!c.readable) {
    return (
      <p className="text-subtle mx-auto mt-5 max-w-sm text-caption">
        To empty the Trash from Swept, give it Full Disk Access — or empty it in
        Finder.
      </p>
    );
  }
  if (c.files === 0 && c.folders === 0) return null;
  return (
    <div className="mx-auto mt-6 max-w-sm border-t border-separator pt-5">
      <p className="text-muted text-body">
        The Trash now holds{" "}
        <span className="font-mono font-semibold tabular-nums text-text">
          {formatBytes(c.bytes)}
        </span>
        . When you are sure you won't need any of it, you can empty it — that
        cannot be undone.
      </p>
      <button
        onClick={onEmpty}
        className="mt-3 rounded-control border border-danger/70 px-4 py-2 text-body font-medium text-danger transition-colors duration-fast ease-mac hover:bg-danger/10"
      >
        Empty Trash…
      </button>
    </div>
  );
}

type Load =
  | { state: "loading" }
  | { state: "failed"; message: string }
  | { state: "ready"; contents: TrashContents };

/** A backend refusal as a sentence: the `refused:` tag is for the audit log. */
function sentence(e: unknown): string {
  const m = describeError(e).replace(/^refused:\s*/i, "");
  return m.charAt(0).toUpperCase() + m.slice(1);
}

export function EmptyTrashModal({
  onCancel,
  onDone,
  onOpenSettings,
}: {
  onCancel: () => void;
  onDone: (s: EmptyTrashSummary) => void;
  onOpenSettings: () => void;
}) {
  const [load, setLoad] = useState<Load>({ state: "loading" });
  // Owned by the dialog, which mounts fresh on every open: a tick never
  // carries over from a previous look at a different Trash.
  const [ack, setAck] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  function read() {
    setLoad({ state: "loading" });
    void call<TrashContents>("trash_contents")
      .then((contents) => setLoad({ state: "ready", contents }))
      .catch((e) => setLoad({ state: "failed", message: describeError(e) }));
  }

  useEffect(read, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !busy) onCancel();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [busy, onCancel]);

  const c = load.state === "ready" ? load.contents : null;
  const blind = c !== null && !c.readable;
  const nothing = c !== null && c.readable && c.files === 0 && c.folders === 0;
  const actionable = c !== null && c.readable && !nothing;

  async function confirm() {
    if (!c || !ack) return;
    setBusy(true);
    setError("");
    try {
      const request: EmptyTrashRequest = {
        // Exactly what this dialog showed: the backend refuses if the Trash
        // differs from it in any way, not only if it grew.
        expected: { count: c.files + c.folders, bytes: c.bytes },
        fingerprint: c.fingerprint,
        acknowledged_unrecoverable: ack,
        // Derived from the displayed figures, never pre-satisfied.
        confirm_mass_delete: c.requires_confirmation,
      };
      const s = await call<EmptyTrashSummary>("empty_trash", { request });
      onDone(s);
    } catch (e) {
      // A refusal means the figures above no longer describe the Trash, or
      // never did. Show the fresh ones and ask again: the tick was given for
      // the old figures, so it does not carry over to the new.
      setError(sentence(e));
      setAck(false);
      setBusy(false);
      read();
    }
  }

  return (
    <div
      className="overlay-in fixed inset-0 z-10 flex items-center justify-center bg-black/60 p-6 pl-[256px]"
      role="dialog"
      aria-modal="true"
      aria-labelledby="empty-title"
    >
      <div
        className={`sheet-in w-full max-w-md rounded-panel border bg-surface3 p-6 shadow-e3 ${
          actionable ? "border-danger/40" : "border-border"
        }`}
      >
        <div className="flex items-start gap-3">
          {blind ? (
            <span className="grid h-9 w-9 flex-none place-items-center rounded-[8px] bg-surface2 text-muted">
              <LockIcon size={18} />
            </span>
          ) : (
            <span className="grid h-9 w-9 flex-none place-items-center rounded-[8px] bg-danger/15 text-danger">
              <NoUndoIcon size={18} />
            </span>
          )}
          <div className="min-w-0">
            <h2
              id="empty-title"
              className="text-balance text-title font-semibold"
            >
              {blind
                ? "Swept can't read the Trash"
                : "Permanently delete everything in the Trash?"}
            </h2>
            {c && actionable && (
              <p className="text-muted mt-1 text-body">
                <span className="font-mono font-semibold tabular-nums text-text">
                  {formatBytes(c.bytes)}
                </span>{" "}
                in {counts(c.files, c.folders)}.
              </p>
            )}
          </div>
        </div>

        {error && (
          <div
            role="alert"
            className="mt-4 flex gap-2.5 rounded-card border border-danger/30 bg-danger/[.08] px-3.5 py-3 text-body leading-relaxed"
          >
            <span className="mt-px flex-none text-danger">
              <InfoIcon size={15} />
            </span>
            <div>
              <p className="text-text">{error}</p>
              {actionable && (
                <p className="text-muted mt-1">
                  The figures above are the Trash as it is now. Check them, then
                  confirm again.
                </p>
              )}
            </div>
          </div>
        )}

        {load.state === "loading" && (
          <p className="text-muted mt-4 text-body" role="status">
            Reading the Trash…
          </p>
        )}
        {load.state === "failed" && (
          <p className="mt-4 text-body text-danger" role="alert">
            {load.message}
          </p>
        )}
        {blind && (
          <p className="text-muted mt-4 text-body leading-relaxed">
            Without{" "}
            <strong className="font-semibold text-text">
              Full Disk Access
            </strong>{" "}
            Swept can't see inside the Trash, so it won't delete anything there.
            Grant it in System Settings, or empty the Trash in Finder.
          </p>
        )}
        {nothing && (
          <p className="text-muted mt-4 text-body">The Trash is empty.</p>
        )}

        {c && actionable && (
          <>
            <div className="mt-4 rounded-card border border-danger/30 bg-danger/[.08] px-3.5 py-3 text-body leading-relaxed text-muted">
              <p className="flex gap-2.5">
                <span className="mt-px flex-none text-danger">
                  <NoUndoIcon size={15} />
                </span>
                <span>
                  <strong className="font-semibold text-text">
                    This cannot be undone.
                  </strong>{" "}
                  The files are removed, not moved, and no copy is kept. This is
                  everything in the Trash — not just what Swept put there — and
                  filters don't apply.
                </span>
              </p>
              {c.left_behind > 0 && (
                <p className="mt-2 flex gap-2.5">
                  <span className="mt-px flex-none">
                    <InfoIcon size={15} />
                  </span>
                  <span>
                    {c.left_behind.toLocaleString()} item
                    {c.left_behind === 1 ? "" : "s"} will stay: a link, a
                    repository or something Swept can't read is left whole
                    rather than partly deleted.
                  </span>
                </p>
              )}
            </div>

            {/* Locked while deleting, so the tick cannot be withdrawn from a
                run that is already under way and look as if it stopped it. */}
            <fieldset className="mt-4" disabled={busy}>
              <legend className="text-subtle mb-1.5 text-micro font-semibold uppercase">
                Confirm
              </legend>
              <label className="flex cursor-pointer items-start gap-2.5 rounded-card border border-separator bg-surface px-3 py-2.5">
                <span className="mt-px flex-none">
                  <Checkbox
                    checked={ack}
                    onChange={() => setAck((a) => !a)}
                    label="I understand these files will be permanently deleted"
                  />
                </span>
                <span className="text-body leading-snug">
                  I understand these files will be permanently deleted and
                  cannot be recovered, and I do this at my own risk.
                </span>
              </label>
              {/* Hidden, not removed, once ticked: the sheet is centred, so
                  a shorter one would move the box out from under the cursor
                  that just ticked it. */}
              <p
                className={`text-subtle mt-1.5 text-caption ${ack ? "invisible" : ""}`}
                aria-hidden={ack}
              >
                Tick the box to enable <b>Delete Permanently</b>.
              </p>
            </fieldset>
          </>
        )}

        <div className="mt-6 flex items-center justify-end gap-3">
          <button
            onClick={onCancel}
            disabled={busy}
            autoFocus
            className="rounded-control border border-border bg-surface2 px-4 py-2 text-body font-medium text-text transition-colors duration-fast ease-mac hover:border-borderStrong disabled:opacity-40"
          >
            {blind ? "Close" : "Cancel"}
          </button>
          {blind && (
            <button
              onClick={onOpenSettings}
              className="rounded-control border border-transparent bg-accent px-4 py-2 text-body font-semibold text-white"
            >
              Open System Settings…
            </button>
          )}
          {actionable && (
            // Dimmed red while not yet allowed, full red while working: the
            // same identity throughout, so "blocked" never looks like a
            // neutral button and "working" never looks like "blocked".
            <button
              onClick={() => void confirm()}
              disabled={busy || !ack}
              aria-busy={busy}
              className={`rounded-control border border-transparent bg-dangerFill px-4 py-2 text-body font-semibold text-white transition-opacity duration-fast ease-mac ${
                busy
                  ? "cursor-progress"
                  : "disabled:cursor-not-allowed disabled:opacity-40"
              }`}
            >
              {busy ? "Deleting…" : "Delete Permanently"}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

/** One vocabulary for the Trash's figures, everywhere they appear. */
function counts(files: number, folders: number): string {
  const f = `${files.toLocaleString()} file${files === 1 ? "" : "s"}`;
  return folders > 0
    ? `${f} and ${folders.toLocaleString()} folder${folders === 1 ? "" : "s"}`
    : f;
}

/** The result of emptying the Trash. Never says "moved to the Trash". */
export function EmptyTrashDone({
  summary,
  onBack,
}: {
  summary: EmptyTrashSummary;
  onBack: () => void;
}) {
  const n = summary.files_deleted;
  return (
    <section className="rounded-card border border-separator bg-surface p-10 text-center">
      {/* Neutral, not red: red is this app's error colour, and this is the
          outcome the user asked for. The irreversibility is in the words. */}
      <div className="mx-auto grid h-12 w-12 place-items-center rounded-panel border border-border bg-surface2 text-muted">
        <NoUndoIcon size={24} />
      </div>
      <p className="mt-4 font-mono text-display font-semibold tabular-nums">
        {formatBytes(summary.bytes_deleted)}
      </p>
      <p className="text-muted mt-1 text-body">
        permanently deleted: {counts(n, summary.folders_removed)}.
        {summary.refused > 0
          ? ` ${summary.refused.toLocaleString()} skipped.`
          : ""}
      </p>
      {summary.left_behind > 0 && (
        <p className="text-muted mt-2 text-body">
          {summary.left_behind.toLocaleString()} item
          {summary.left_behind === 1 ? " was" : "s were"} left in the Trash
          (links, repositories, or files Swept couldn't remove).
        </p>
      )}
      <p className="text-subtle mt-2 text-caption">
        Recorded in the audit log.
      </p>
      <button
        onClick={onBack}
        className="mt-5 rounded-control border border-border bg-surface2 px-4 py-2 text-body font-medium text-text transition-colors duration-fast ease-mac hover:border-borderStrong"
      >
        Back to Cleanup
      </button>
    </section>
  );
}
