// The single seam between the UI and the Rust backend.
//
// Two failure modes must stay distinguishable, because conflating them is how
// the UI ends up lying about the user's disk:
//
//   1. Not running inside the desktop app at all (a plain browser). There is no
//      backend to ask, so we say so.
//   2. Running inside the app, but the command failed (permission denied, an
//      unreadable home directory, ...). That is real, actionable information
//      and must reach the user unaltered.
//
// Neither case may ever be answered with fixture data: the sample category ids
// are the *real* ones, so a user shown fabricated sizes could go on to dispose
// of real files. Fixtures live in `ux/` and are injected by the test harness
// through Tauri's own transport; they are not part of the shipped bundle.

/** True when the page is hosted by the Tauri webview (the real app). */
export function isDesktopApp(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** Thrown when a command is attempted outside the desktop app. */
export class NotInAppError extends Error {
  constructor() {
    super("mac-cleaner runs as a desktop app.");
    this.name = "NotInAppError";
  }
}

/**
 * Invoke a Rust command. Rejects with `NotInAppError` outside the app, and
 * propagates the backend's own error otherwise. Never substitutes a fallback.
 */
export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isDesktopApp()) throw new NotInAppError();
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

/** Render a thrown value as a readable sentence. */
export function describeError(e: unknown): string {
  if (e instanceof NotInAppError) return e.message;
  if (e instanceof Error) return e.message;
  return String(e);
}
