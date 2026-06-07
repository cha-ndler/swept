# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/), and this project adheres to
[Semantic Versioning](https://semver.org/).

## [0.2.0] — 2026-06-07

A pleasant desktop GUI (Tauri) over the v0.1 CLI, built oracle-first.

### Added
- **Desktop GUI** (`crates/gui`, Tauri v2 + Vite/React/TS/Tailwind): a thin
  front-end over `macclean-core` — all deletion still routes through the
  consent-gated executor (Trash-only in the GUI; never permanent).
- **Clean view**: categories with selection, reclaimable-space bars, a prominent
  total, and **age/size filters**; honors per-category selection.
- **Clean flow**: a confirmation modal ("Move N items to the Trash?", recoverable,
  audit-logged) → done summary.
- **Startup view**: read-only login-items review with run-at-login badges.
- **`macclean-gui-core`**: tested command layer (scan/clean/login-items DTOs).
- **UX oracle**: Playwright screenshots + axe a11y + visual-regression in CI
  (`design/rubric.md`, `.claude/agents/ux-critic.md`); a `package` job bundles
  `.app`/`.dmg` and attaches the `.dmg` to tagged releases.

### Safety
- The GUI introduces no new deletion logic; the dry-run default, Trash-first
  disposal, mass-delete confirmation, and audit log are unchanged (every clean
  diff passed `deletion-safety-reviewer`).

## [0.1.0] — 2026-06-07

First release: a safe, dry-run-first macOS junk-cleaning CLI built on a
property-tested safety substrate.

### Added
- **Safety substrate** (`crates/safety`): protected-path denylist (checked
  first), path guard (canonicalize → `SafePath`, `..`/TOCTOU defense), scoped
  allowlist. The only chokepoint every destructive op passes through.
- **Engine** (`crates/core`): read-only scanner → dry-run plan → consent-gated
  executor (Trash by default) → append-only JSONL audit log.
- **CLI** (`macclean`): `scan` (preview) and `clean` (dry-run unless
  `--execute`; `--permanent` and `--yes` gate the dangerous paths).
- `--older-than-days` age filter and `--min-size` large-files filter (composable).
- `--json` structured scan output (stable wire contract for tooling / a future GUI).
- Cleaner-category registry (application caches, logs, Xcode derived data, the
  user Trash, Homebrew downloads) with names + descriptions.
- `login-items` — read-only review of `~/Library/LaunchAgents` startup items.
- Property-based tests (`proptest`) fuzzing the safety-kernel invariants.
- CI on macOS (fmt + clippy `-D warnings` + tests + a "no real `$HOME` in tests"
  guard) and a release-build job publishing the `macclean` binary artifact.

### Safety
- Dry-run is the default; destructive actions require explicit consent.
- Recursive/large removals require confirmation; audit failures abort the run.
- Tests run only against throwaway temp-dir fixtures.

[0.1.0]: https://github.com/cha-ndler/mac-cleaner/releases/tag/v0.1.0
