# mac-cleaner

## About
A free, open-source equivalent of CleanMyMac. It finds junk files to clean, recommends performance optimizations (e.g. disabling startup items), scans apps for available updates, and assists with clutter removal. It targets only items that are safe to remove, and **never deletes anything automatically** — it shows the user exactly what files/folders it would remove and acts only on explicit consent. The UI should be pleasant, but need not be as extravagant as CleanMyMac.

## Status
Greenfield. No language/framework chosen yet. **Build the safety substrate before any cleanup feature.**

## SAFETY CONTRACT (non-negotiable — this is a data-destroying tool)
Every code path that deletes, trashes, moves, truncates, or overwrites a file MUST satisfy all of:

1. **Dry-run is the default.** Destructive action requires an explicit flag/confirmation. Default behavior previews only.
2. **Protected-path denylist, checked first.** Refuse anything under: `/`, `/System`, `/usr`, `/bin`, `/sbin`, `/Library`, `/Applications`, `~/Library/Keychains`, `~/Library/Mail`, the home root itself, and anything inside a `.git`. Canonicalize paths and re-check after resolving symlinks (no `..` escape, no TOCTOU).
3. **Scoped allowlist.** Cleanup is confined to known-safe locations (caches, logs, trashes, build artifacts) — never arbitrary user paths without confirmation.
4. **Trash, not unlink.** Prefer moving to the system Trash (recoverable). Irreversible deletion only behind explicit per-action consent.
5. **No unconfirmed mass delete.** Recursive removals show count + total size and require confirmation; reject suspiciously broad globs.
6. **Append-only audit log** of every planned (dry-run) and executed action, with absolute paths and sizes.
7. **Tests run against a throwaway temp dir — never the real filesystem.**

Before presenting any diff that adds or changes deletion logic, run the `deletion-safety-reviewer` subagent on it.

## Workflow notes
- This repo does NOT use `acceptEdits` (`.claude/settings.json` sets `defaultMode: default`) — deletion code must be reviewed, not auto-applied.
- A project hook (`.claude/hooks/no-real-deletes.sh`) blocks the agent from executing `rm`/`trash`/`find -delete` against real paths during development. Test against fixtures.

## Architecture decisions (fill in as made)
- Language / UI framework: _TBD — record the choice and rationale here._
- Core modules: scanner, denylist/safety, planner (dry-run), executor (consent-gated), audit log, UI.
