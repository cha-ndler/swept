import { useEffect, useState } from "react";
import { call, describeError } from "./backend";
import type { UpdateStatus } from "./types";

/**
 * The update check — the app's only network request, and only on request.
 *
 * Nothing here runs unless the user presses the button, or ticked "Check at
 * launch" on an earlier visit. That box is **off by default** and the choice
 * lives only in this webview's local storage; it is a convenience, not a
 * consent record, because the thing it enables reports a version number and
 * changes nothing on disk.
 *
 * It downloads and installs nothing. A newer version is reported with its
 * release page, which the user opens themselves — the app has no URL-opening
 * permission, so the link is shown as text to copy.
 */
const AT_LAUNCH_KEY = "swept.update-check-at-launch";

function readAtLaunch(): boolean {
  try {
    return window.localStorage.getItem(AT_LAUNCH_KEY) === "1";
  } catch {
    return false;
  }
}

function writeAtLaunch(on: boolean) {
  try {
    if (on) window.localStorage.setItem(AT_LAUNCH_KEY, "1");
    else window.localStorage.removeItem(AT_LAUNCH_KEY);
  } catch {
    // Not persisted; the box still reflects this session's choice.
  }
}

type State =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "done"; status: UpdateStatus }
  | { kind: "failed"; message: string };

export function UpdateCheck() {
  const [atLaunch, setAtLaunch] = useState(readAtLaunch);
  const [state, setState] = useState<State>({ kind: "idle" });
  const [copied, setCopied] = useState(false);

  async function check() {
    setState({ kind: "checking" });
    setCopied(false);
    try {
      const status = await call<UpdateStatus>("check_for_update");
      setState({ kind: "done", status });
    } catch (e) {
      setState({ kind: "failed", message: describeError(e) });
    }
  }

  // Only ever on the user's earlier say-so.
  useEffect(() => {
    if (readAtLaunch()) void check();
  }, []);

  return (
    <div className="px-2 pb-2.5">
      <button
        onClick={() => void check()}
        disabled={state.kind === "checking"}
        // A tertiary button, not a label: without a surface and a glyph it
        // read as the heading of the switch below it.
        className="text-muted -mx-1.5 inline-flex h-6 items-center gap-1.5 rounded-[6px] px-1.5 text-caption font-medium transition-colors duration-fast ease-mac hover:bg-white/[.06] hover:text-text focus-visible:ring-2 focus-visible:ring-accent disabled:text-subtle"
      >
        <svg
          width="13"
          height="13"
          viewBox="0 0 16 16"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.5"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden="true"
        >
          <path d="M13.2 8a5.2 5.2 0 1 1-1.5-3.7" />
          <path d="M13.4 2.4v2.9h-2.9" />
        </svg>
        {state.kind === "checking" ? "Checking…" : "Check for updates"}
      </button>

      {state.kind === "done" && !state.status.newer && (
        <p className="text-subtle mt-1 text-micro normal-case leading-snug tracking-normal">
          You have the latest version (
          <span className="font-mono tabular-nums">{state.status.current}</span>
          ).
        </p>
      )}
      {state.kind === "done" && state.status.newer && (
        <div className="mt-1 text-micro normal-case leading-snug tracking-normal">
          {/* Muted, so the footer's brightest line stays the safety promise
              and the accent "Copy link" is the one cue. */}
          <p className="text-muted">
            Swept{" "}
            <span className="font-mono tabular-nums">{state.status.latest}</span>{" "}
            is available — you have{" "}
            <span className="font-mono tabular-nums">{state.status.current}</span>.
          </p>
          <p className="text-subtle mt-1 select-text [overflow-wrap:anywhere]">
            {state.status.url}
          </p>
          <button
            onClick={() => {
              void navigator.clipboard
                ?.writeText(state.status.url)
                .then(() => setCopied(true))
                .catch(() => setCopied(false));
            }}
            className="text-accentText -my-1 mt-0 py-1 font-medium hover:underline"
          >
            {copied ? "Link copied" : "Copy link"}
          </button>
        </div>
      )}
      {state.kind === "failed" && (
        <p
          className="text-subtle mt-1 text-micro normal-case leading-snug tracking-normal"
          role="status"
        >
          Could not check: {state.message}
        </p>
      )}

      {/* A switch, not a checkbox: this is a standing preference, and the
          app's checkboxes are reserved for consent to an action. */}
      <button
        role="switch"
        aria-checked={atLaunch}
        aria-label="Check for updates at launch"
        onClick={() => {
          const next = !atLaunch;
          setAtLaunch(next);
          writeAtLaunch(next);
        }}
        className="text-subtle mt-2 flex items-center gap-2 text-micro normal-case tracking-normal hover:text-muted"
      >
        <span
          aria-hidden="true"
          className={`relative h-[12px] w-[20px] flex-none rounded-full border transition-colors duration-fast ease-mac ${
            atLaunch
              ? "border-accent bg-accent"
              : "border-subtle/70 bg-white/[.08]"
          }`}
        >
          <span
            className={`absolute top-[1px] h-[8px] w-[8px] rounded-full bg-white transition-[left] duration-fast ease-mac ${
              atLaunch ? "left-[9px]" : "left-[1px]"
            }`}
          />
        </span>
        Check at launch
      </button>
    </div>
  );
}
