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

**UX verification model (oracle-first).** "Pleasant" has no oracle by default, so
we build one before the UI art. The loop renders the web frontend headlessly
(Playwright) into PNGs, then a `ux-critic` vision subagent (Claude can view
images via the Read tool) scores them against a committed rubric and against
competitor reference screenshots (CleanMyMac, DaisyDisk, …), and proposes the
next fixes. Objective checks (axe a11y, Lighthouse, visual-regression snapshots,
responsive overflow) run in CI as hard gates. Backend/scaffold/harness/packaging
tasks are fully loop-verifiable and auto-merge on green; the **visual view tasks
iterate via the critic loop, then still PAUSE for a final human taste gate**
(the PR carries screenshots + critic scores + reference comparisons). Every
destructive action in the GUI must route through the consent-gated `executor`
(dry-run default, Trash-first, mass-delete confirmation, audit log).

- [x] **GUI command layer** (`crates/gui-core`) — tested wrappers over
  `macclean-core` returning serde DTOs (`scan_report`, `list_login_items`,
  `clean_with_sink`); no new deletion logic. (PR #12)
- [ ] **Tauri shell + frontend toolchain + design system** — scaffold the Tauri
  v2 app (`crates/gui`) with Vite + Tailwind + a component library
  (shadcn/Radix) and design tokens; a minimal window whose button calls a
  `gui-core` command via `#[tauri::command]`. *Goal (loop-verifiable): `cargo
  build` + `npm run build` succeed; the command round-trips; src-tauri builds in
  CI without a display. Auto-merge on green.*
- [ ] **Visual-eval harness (the UX oracle)** — Playwright headless screenshot
  script (multi-viewport × states), a committed `.claude/agents/ux-critic.md`
  vision critic, `design/references/` (competitor screenshots) + a `design/
  rubric.md`, plus axe + Lighthouse + visual-regression checks wired into CI.
  *Goal (loop-verifiable): the harness runs in CI and gates pass on the
  placeholder UI; the critic can score a screenshot. Auto-merge on green.*
- [ ] **Scan view** — categories (name, size, count) with selection + a
  reclaimable-space visualization; iterate via the critic loop vs references.
  *Read-only. Critic-iterated, then PAUSE for the human taste gate.*
- [ ] **Clean flow + confirmation modal** — dry-run preview → explicit confirm →
  execute (Trash default; permanent/mass-delete gated). *Execute path tested via
  injected `DirSink`; UI can't trigger a destructive call without confirmation.
  Critic-iterated, then PAUSE.*
- [ ] **Filters + login-items view + theming/polish** — age/size filters and the
  login-items review; final pleasant-UX polish. *Critic-iterated, then PAUSE.*
- [ ] **Package + v0.2.0** — `tauri build` produces a `.app`/`.dmg` artifact in
  CI; add a screenshot to the README; tag `v0.2.0`. *Loop-verifiable.*
