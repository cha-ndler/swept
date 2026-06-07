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
- [x] **Tauri shell + frontend toolchain + design system** (PR #13) — Tauri v2
  app (`crates/gui`, excluded from the core workspace) with Vite + React + TS +
  Tailwind and CSS-variable design tokens; read-only `scan`/`login_items`
  `#[tauri::command]`s delegating to `gui-core`; built by a dedicated CI job
  (npm build → cargo build). (Rich component library deferred to the view tasks.)
- [x] **Visual-eval harness (the UX oracle)** (PR #14) — Playwright (headless
  Chromium) renders the built frontend across viewports → PNG screenshots +
  axe-core a11y gate + visual-regression snapshots, wired into the CI `gui` job;
  committed `.claude/agents/ux-critic.md`, `design/rubric.md`, and
  `design/references/` (manual competitor screenshots). The harness immediately
  caught a real WCAG contrast bug (white-on-accent 3.64:1 → fixed token).
  _Lighthouse deferred: marginal for a local webview and a CI-flakiness risk;
  axe + visual-regression are the objective gates for now._
- [x] **Scan view** (PR #15, awaiting human taste sign-off) — categories with
  name/size/count, per-category selection, reclaimable-space bars, a prominent
  total + primary action, and empty/loading/error states. Critic-iterated
  (caught + fixed two real a11y bugs); axe + visual-regression green.
- [x] **Clean flow + confirmation modal** (PR #16, awaiting human taste sign-off) —
  preview → explicit confirm modal ("Move N items to the Trash?", recoverable,
  audit-logged) → cleaning → done. Trash-only (never permanent); honors
  per-category selection; mass-delete gate server-enforced. deletion-safety
  VERDICT: PASS; axe + visual-regression green for confirm/done states.
- [x] **Filters + login-items view + theming/polish** (PR #17, awaiting human
  taste sign-off) — Clean/Startup tab nav; age + size filter controls (re-scan);
  a read-only Startup (login-items) view with run-at-login badges; consistent
  token-based polish. Critic-iterated; axe + visual-regression green across all
  12 states (results/empty/loading/confirm/done + startup × 2 viewports).
- [ ] **Package + v0.2.0** — `tauri build` produces a `.app`/`.dmg` artifact in
  CI; add a screenshot to the README; tag `v0.2.0`. *Loop-verifiable.*
