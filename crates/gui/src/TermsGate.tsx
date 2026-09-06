import { useEffect, useState } from "react";
import { call, describeError, isDesktopApp } from "./backend";
import { Checkbox } from "./Controls";
import { InfoIcon, ShieldIcon } from "./Shell";

/**
 * First-run acceptance of the Terms of Use.
 *
 * Swept ships outside the Mac App Store, so no platform agreement stands
 * between it and the person running it — this screen and `TERMS.md` are the
 * whole contractual surface. `docs/LEGAL.md` sets out why it exists: a
 * disclaimer nobody agreed to carries far less weight than one they did, and
 * this is the difference between the two.
 *
 * Four properties make that difference real, and none of them is decoration:
 *
 *   1. **The terms are readable here, in full.** Not a link — the app grants no
 *      general URL-opening permission and has no network code, so the text is
 *      compiled into the binary and served by `terms_text`. It is therefore
 *      necessarily the text this build was made from.
 *   2. **Two boxes, neither pre-ticked.** The same shape as the Privacy
 *      screen's per-consequence acknowledgements, for the same reason: a tick
 *      the user did not make is not an acknowledgement. They are separate
 *      because they are separate promises — one about what this program does,
 *      one about what the user has in place before it does it.
 *   3. **The primary is disabled until both are ticked**, and says so.
 *   4. **There is no way past this except through it.** No dismiss, no escape
 *      key, no "later". The alternative offered is quitting, which is honest:
 *      declining the terms means not running the app, exactly as `TERMS.md`
 *      says.
 *
 * It renders only in the desktop app. In a browser — the `ux/` harness, a dev
 * server — there is no backend to ask and nothing to record, so gating there
 * would block the screenshot harness on a modal it cannot satisfy.
 */

type Status = {
  accepted: boolean;
  terms_version: string;
  terms_digest: string;
  accepted_version: string | null;
};

export default function TermsGate({ children }: { children: React.ReactNode }) {
  // `null` = still asking. Nothing renders underneath until we know, so the app
  // is never briefly usable before the gate appears.
  const [status, setStatus] = useState<Status | null>(null);
  const [text, setText] = useState<string>("");
  const [understands, setUnderstands] = useState(false);
  const [hasBackups, setHasBackups] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (!isDesktopApp()) {
      // Outside the app there is no record to consult and no way to write one.
      setStatus({
        accepted: true,
        terms_version: "",
        terms_digest: "",
        accepted_version: null,
      });
      return;
    }
    let live = true;
    void (async () => {
      let s: Status;
      try {
        s = await call<Status>("terms_status");
      } catch (e) {
        if (!live) return;
        // Fail closed: if we cannot read the record we must not assume yes, so
        // present the gate with the error rather than the app.
        setStatus({
          accepted: false,
          terms_version: "",
          terms_digest: "",
          accepted_version: null,
        });
        setError(describeError(e));
        return;
      }
      if (!live) return;
      setStatus(s);
      // Only fetched when the gate will actually render. Two reasons: the
      // common launch is an accepted one and does not need the document at
      // all, and asking for it unconditionally would make every screen depend
      // on a second command succeeding — which is how the first version of
      // this blocked the entire UX harness on a `Promise.all` that rejected
      // because only one of the two commands was stubbed.
      if (s.accepted) return;
      try {
        const t = await call<string>("terms_text");
        if (live) setText(t);
      } catch (e) {
        // The gate stays up and the primary stays disabled: nobody can accept
        // terms they were not shown.
        if (live) setError(describeError(e));
      }
    })();
    return () => {
      live = false;
    };
  }, []);

  if (status === null) return null;
  if (status.accepted) return <>{children}</>;

  // `text` is part of the condition, not decoration: accepting a document that
  // failed to load would be an acceptance of nothing.
  const ready = understands && hasBackups && !busy && text !== "";
  // A revision, not a first launch. Worth distinguishing: greeting a returning
  // user as a stranger misdescribes what changed.
  const revised = status.accepted_version !== null;

  async function accept() {
    setBusy(true);
    setError("");
    try {
      const s = await call<Status>("accept_terms");
      setStatus(s);
    } catch (e) {
      // The backend refuses to proceed if it could not write the record, and so
      // do we — there would be no evidence consent was given.
      setError(describeError(e));
      setBusy(false);
    }
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-6"
      role="dialog"
      aria-modal="true"
      aria-labelledby="terms-title"
    >
      {/* The window still has to be movable: the OS title bar is gone. */}
      <div className="titlebar-drag fixed inset-x-0 top-0" data-tauri-drag-region />

      <div className="sheet-in flex max-h-full w-full max-w-xl flex-col rounded-panel border border-separator bg-surface3 p-6 shadow-e3">
        <div className="flex flex-none items-start gap-3">
          <span className="grid h-9 w-9 flex-none place-items-center rounded-card bg-warning/[.16] text-warning">
            <InfoIcon size={18} />
          </span>
          <div className="min-w-0">
            <h1 id="terms-title" className="text-title font-semibold">
              {revised ? "The terms have changed" : "Before you use Swept"}
            </h1>
            <p className="text-muted mt-1 text-body leading-snug">
              {revised ? (
                <>
                  Swept’s terms were revised since you last accepted them
                  (version {status.accepted_version} → {status.terms_version}).
                  Please read them again.
                </>
              ) : (
                <>
                  Swept removes files from this Mac. That is what it is for, so
                  please read this before running it.
                </>
              )}
            </p>
          </div>
        </div>

        {/* The three things that actually matter, above the full text. Nobody
            reads a legal document to find out whether a tool is dangerous, and
            burying it below one would be a formality rather than a warning. */}
        <ul className="mt-4 flex-none space-y-2">
          <Point>
            <b>Removal is not reliably reversible.</b> Items go to the Trash by
            default and can be recovered until you empty it. Anything removed
            permanently is gone.
          </Point>
          <Point>
            <b>Swept comes with no warranty of any kind.</b> It is free and
            provided as is. Its author is not liable for lost data — see
            sections 4 and 5 below.
          </Point>
          <Point>
            <b>Keep a current backup.</b> Time Machine or any backup you have
            actually restored from. This is a condition of use.
          </Point>
        </ul>

        <div className="mt-4 flex min-h-0 flex-1 flex-col">
          <p className="text-subtle mb-1.5 flex-none text-micro font-semibold uppercase">
            Terms of Use · version {status.terms_version}
          </p>
          {/* Scrollable, focusable, and labelled: the full text has to be
              reachable by keyboard and by a screen reader, not only by mouse. */}
          <div
            tabIndex={0}
            role="region"
            aria-label="Terms of Use, full text"
            className="min-h-[8rem] flex-1 overflow-y-auto rounded-card border border-separator bg-surface p-3.5"
          >
            <pre className="text-muted whitespace-pre-wrap font-sans text-caption leading-relaxed">
              {text}
            </pre>
          </div>
        </div>

        <fieldset className="mt-4 flex-none">
          <legend className="text-subtle mb-1.5 text-micro font-semibold uppercase">
            Confirm before continuing
          </legend>
          <div className="space-y-2">
            <label className="flex cursor-pointer items-start gap-2.5 rounded-card border border-separator bg-surface px-3 py-2.5">
              <span className="mt-px flex-none">
                <Checkbox
                  checked={understands}
                  onChange={() => setUnderstands(!understands)}
                  label="I have read and accept the Terms of Use"
                />
              </span>
              <span className="text-body leading-snug">
                I have read and accept the Terms of Use, including the
                disclaimer of warranties and limitation of liability.
              </span>
            </label>
            <label className="flex cursor-pointer items-start gap-2.5 rounded-card border border-separator bg-surface px-3 py-2.5">
              <span className="mt-px flex-none">
                <Checkbox
                  checked={hasBackups}
                  onChange={() => setHasBackups(!hasBackups)}
                  label="I keep a current backup of this Mac"
                />
              </span>
              <span className="text-body leading-snug">
                I keep a current backup of this Mac, and I accept
                responsibility for what I confirm Swept may remove.
              </span>
            </label>
          </div>
          <p className="text-subtle mt-1.5 text-caption">
            Tick both to enable <b>Accept and continue</b>.
          </p>
        </fieldset>

        {error && (
          <div className="mt-3 flex-none rounded-card border border-danger/30 bg-danger/[.07] px-3.5 py-3">
            <p className="text-body">{error}</p>
            <p className="text-muted mt-1 text-caption">
              Swept will not start until your acceptance can be written down.
            </p>
          </div>
        )}

        <div className="mt-4 flex flex-none items-center gap-3">
          <span className="text-success flex-none">
            <ShieldIcon size={13} />
          </span>
          <p className="text-subtle flex-1 text-caption leading-snug">
            Recorded on this Mac only, and never sent anywhere.
          </p>
          <button
            className="rounded-control px-4 py-2 text-body font-medium text-muted transition-colors duration-fast ease-mac hover:text-text"
            onClick={() => void call("quit_app").catch(() => window.close())}
          >
            Quit
          </button>
          <button
            className="rounded-control bg-accent px-4 py-2 text-body font-semibold text-white transition-colors duration-fast ease-mac disabled:bg-white/[.08] disabled:text-subtle"
            onClick={() => void accept()}
            disabled={!ready}
          >
            {busy ? "Recording…" : "Accept and continue"}
          </button>
        </div>
      </div>
    </div>
  );
}

function Point({ children }: { children: React.ReactNode }) {
  return (
    <li className="flex gap-2.5 rounded-card border border-separator bg-surface px-3.5 py-2.5">
      <span className="text-warning mt-px flex-none" aria-hidden="true">
        <InfoIcon size={14} />
      </span>
      <span className="text-muted text-caption leading-relaxed">{children}</span>
    </li>
  );
}
