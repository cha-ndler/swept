# mac-cleaner roadmap

This is the autonomous development loop's compass. Each iteration: pick the first
unchecked item, complete it to its **verifiable goal** via the `/loop` workflow
(test-first → clippy/fmt → deletion-safety-reviewer if deletion logic changes →
verifier → PR → CI green → merge), check it off here, then start the next.

**v0.1.0 (shipped) = a complete, safe CLI.** **v0.2 = a pleasant GUI** (Tauri),
reusing the UI-agnostic `macclean-core` crate and its `--json`/`ScanReport`
contract. The CLI was the foundation; the GUI is a second front-end, not a rewrite.

## Done
- [x] Safety substrate + Claude Code harness (PR #1)
- [x] Age-based cleanup filter `--older-than-days` (PR #2)
- [x] `--json` structured scan output (PR #3)
- [x] Cleaner-category registry + Homebrew downloads (PR #4)
- [x] Large-old-files finder (`--min-size`) (PR #5)
- [x] Startup/login-items inspector (`macclean login-items`) (PR #6)
- [x] Property tests for the safety kernel (proptest) (PR #7)
- [x] CI/release hardening (checkout@v5, release artifact, badge) (PR #8)
- [x] Docs + v0.1.0 (README, CHANGELOG, tag) (PR #9)

## Descoped
- [~] Empty-Trash cleaner — **descoped.** `~/.Trash` is already a first-class
  allowlist root + category, so `scan`/`clean` already cover it; a dedicated
  emptier is largely redundant. (The one nuance — items already in the Trash
  want permanent disposal rather than re-trashing — can be folded into the
  category model later if needed.)

## v0.1.0 backlog
_All complete — v0.1.0 shipped. 🎉_

## v0.2 — Pleasant GUI (Tauri)

Framework: **Tauri** (Rust backend + web frontend), chosen so the GUI calls
`macclean-core` directly (no shelling out) and stays a thin front-end over the
same safety substrate. Node 26/npm present; macOS uses the system WKWebView.

**Verification note (important):** an unattended agent has no display and cannot
judge "pleasant." So backend/scaffold/packaging tasks are fully loop-verifiable
(`cargo test` + build), but the **visual frontend tasks build green and then
PAUSE for human visual review** — they are not auto-merged. Every destructive
action in the GUI must still route through the consent-gated `executor` (dry-run
default, Trash-first, mass-delete confirmation, audit log).

- [ ] **Tauri scaffold + backend command layer** — add the Tauri app
  (`src-tauri`) and `#[tauri::command]` wrappers over `macclean-core`
  (`scan_report(filters) -> ScanReport`, `clean(consent) -> ExecReport`,
  `login_items()`). Install `tauri-cli` as needed. *Goal (loop-verifiable): the
  command functions are plain Rust covered by `cargo test`; the workspace + GUI
  crate build; deletion routes through the consent-gated executor. Auto-merge on
  green.*
- [ ] **Scan view (frontend)** — list categories (name, size, count) with
  per-category selection; calls the scan command. *Goal: frontend typechecks +
  builds; backend command tested. Read-only. PAUSE for visual review — do not
  auto-merge.*
- [ ] **Clean flow + confirmation** — dry-run preview → explicit confirmation
  modal → execute (Trash default; permanent/mass-delete gated). *Goal: execute
  command tested via injected `DirSink`; UI cannot trigger a destructive call
  without confirmation. PAUSE for visual review.*
- [ ] **Filters + login-items + theming** — age/size filters and the login-items
  view in the UI; polish for pleasant UX. *Goal: builds; command params tested.
  PAUSE for visual review.*
- [ ] **Package + CI** — `tauri build` produces a `.app`/`.dmg` bundle uploaded
  as a CI artifact; add a screenshot to the README; tag `v0.2.0`. *Goal
  (loop-verifiable): CI builds the app bundle artifact.*
