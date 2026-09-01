# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/), and this project adheres to
[Semantic Versioning](https://semver.org/).

## [Unreleased]

Real-world hardening after dogfooding v0.2, a native-feeling shell, and the
first two modules that look outside the cleanup allowlist.

### Added
- **Directory actions in the executor** — a plan can now carry a
  `PlannedDirAction`: a tree walked in full by `guard_dir`, moved to the Trash
  as one recoverable unit, by explicit per-path grant only (no allowlist route
  for a tree), re-walked immediately before disposal, refused if it grew since
  it was confirmed, and counted against the mass-delete threshold by every
  name beneath it. The action type carries no permanent variant. Nothing the
  scanner produces uses it. (#38)
- **Uninstaller discovery** — a read-only search for what an application left
  behind, across eleven per-user locations, built around one predicate: never
  claim a still-installed app's data. Matching is by dot-separated segment,
  byte-exact in case, with the longest installed owner winning. Sandbox
  containers are decomposed by an inclusion list — the user's `Documents` and
  `Application Support` inside a container are shown and never offered; group
  containers are shown and never claimed; a human-name match in
  `~/Library/Application Support` is gated three times and never
  bulk-grantable. A row disposal is certain to refuse — a `.git` inside the
  tree, or a tree beyond the disposal bounds — is shown and never offered; a
  licence-shaped name keeps a row out of any bulk gesture; and a report with a
  preferences row carries the `cfprefsd` caveat. No engine command exists yet,
  and nothing here can authorize anything. (#36, #37, #39)
- **Large & Old Files** — a read-only walk of `~/Documents`, `~/Downloads`,
  `~/Desktop`, `~/Movies`, `~/Music` and `~/Pictures`, with size and age
  filters. Nothing is ever pre-selected, there is no select-all, and acting on a
  row requires a **per-path grant**. (#29, #30)
- **Space Lens engine** — a parallel, depth-capped directory-size measurement
  producing a tree for a treemap. It cannot authorize anything: no `SafePath`,
  and no command that accepts a node back. (#31)
- **Per-path grants** (`Consent.granted`) — the mechanism that lets a human
  escalate one specific file outside the disposal allowlist, without widening
  the allowlist itself. Capped, individually enumerated, still denylist-checked,
  and audited with a distinct note. (#27)
- **`guard_dir`** — a bounded, **fail-closed** tree walk that supplies the real
  recursive count and size for a directory, and refuses to vouch for a tree it
  could not fully read. Required before any directory-level disposal can ship.
  (#28)
- **A native Mac shell** — inset traffic lights over a vibrancy sidebar, a
  persistent module sidebar that keeps each module's state alive across
  switches, a design-token system ported from a first-party design canvas, the
  scan ring, and the replacement of the last stock browser controls. (#22, #23,
  #24)
- **Menu-bar extra** showing the reclaimable figure — with **no** quick-clean
  action, because clearing files from a menu means no preview and no
  confirmation. (#26)
- **Full Disk Access preflight** — the app probes the TCC-gated roots and says
  when a scan is under-reporting, instead of quietly showing a smaller number.
  (#25)
- **Real scan progress**, with the scan moved off the UI thread. (#21)

### Fixed
- **The UI could show figures that described no real disk.** `CleanView` and
  `StartupView` wrapped `invoke` in a bare `catch` that fell back to fixture
  data — so *any* backend failure rendered fabricated sizes against the real
  category ids, and a user could then run a real clean against numbers they had
  never scanned. Fixtures moved out of the shipped bundle entirely. (#20)
- **The denylist did not refuse ancestors of protected locations.**
  `guard("~/Library")` succeeded: `PROTECTED_ABS_ROOTS` lists the absolute
  `/Library` and `Path::starts_with` is component-wise. Only the allowlist — a
  scope check, not a safety check — kept it out of reach. (#27)
- **The `.git` rule compared bytes exactly** while every other denylist rule
  folds case, so a repository spelled `.GIT` was invisible to it. (#28)
- **13 of 16 opacity-modified colour classes emitted no CSS**, silently, with no
  build error: Tailwind cannot split a `var()` holding a hex string into
  channels. Tokens are now RGB channels. (#30)
- **Every confirmation-sheet screenshot since the clean flow shipped was a
  mid-animation frame** — `page.screenshot` does not disable animations, while
  `toHaveScreenshot` does, so the reviewed image and the gated image were
  different pictures. (#30)

### Safety
- Established the spine the modules rest on: **widen what we can see, never
  widen what we can act on.** `allowlist::discovery_roots` is read-only and
  wider; `allowlist::default_roots` — the disposal scope — is unchanged, and
  every existing invariant test stayed green untouched. (#27)
- Disposal outside the allowlist re-guards each path, requires it to already be
  its own canonical spelling, confines it to the discovery scope, re-reads sizes
  from disk, and **refuses the whole request** if any item no longer matches
  what was confirmed. (#29)
- Permanent removal is confined to the allowlist even when consented to, and the
  desktop app cannot request it at all. (#27)

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

[Unreleased]: https://github.com/cha-ndler/mac-cleaner/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/cha-ndler/mac-cleaner/releases/tag/v0.2.0
[0.1.0]: https://github.com/cha-ndler/mac-cleaner/releases/tag/v0.1.0
