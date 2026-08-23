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
- [x] **Package + v0.2.0** (PR #18) — `cargo tauri build` produces `.app` + `.dmg`
  (validated locally); a CI `package` job uploads the bundle and attaches the
  `.dmg` to tagged releases; README screenshots + CHANGELOG; tag `v0.2.0`.

## v0.3 — real-world hardening + UI prettification

Surfaced by dogfooding v0.2 on a real Mac (230k files, 11.6 GiB found; a real
Logs clean verified via the audit log). Run via the `.claude/loops/` prompts.

### Trust & truth (do first — the app could show figures that describe no real disk)
- [x] **No fixture fallback in the app** — `CleanView`/`StartupView` wrapped
  `invoke` in a bare `catch` that fell back to `sample.ts`. That was not an
  "am I in Tauri?" check: *any* backend failure (a permission denial, an
  unresolvable home) rendered fabricated sizes/counts, and because the fixture
  category ids are the real ones the user could then run a real clean against
  numbers they had never actually scanned. Fixtures moved out of `src/` to
  `ux/fixtures.ts`; one honest transport in `src/backend.ts`; failures surface
  as an error state that says nothing was scanned or changed. The `?state=`
  preview branch is gone — the UX oracle now injects a fake
  `window.__TAURI_INTERNALS__`, so the screenshots exercise the *real* data
  path instead of a preview-only branch. Guarded by a build-artifact test
  asserting `dist/` contains no fixture strings (can't rot).
- [x] **Denylist: refuse ancestors of protected locations** — `guard("~/Library")`
  *succeeded*: `PROTECTED_ABS_ROOTS` lists the absolute `/Library` and
  `Path::starts_with` is component-wise, so `~/Library` never matched, and
  `PROTECTED_HOME_SUBPATHS` covers only `Keychains`/`Mail`. Only the allowlist
  — a scope check, not a safety check — kept it out of reach. Latent today
  (disposal is file-only), but it would open the moment per-path grants exist.
  Now refused by the denylist itself, with exact-ancestor semantics so
  `Library/Caches` and `Library/Logs` stay cleanable. Unit + property coverage;
  verified RED first.
- [ ] **Directory disposal needs `guard_dir`** — before any directory is ever
  disposed of (uninstaller leftover trees), `guard()` is not enough: it must
  also refuse a directory containing a `.git` anywhere in its subtree, via a
  bounded walk that **fails closed** if it cannot finish.
- [ ] **Audit log should not follow symlinks** — `audit.rs` opens with
  `create(true).append(true)`, which follows a symlink at the final component,
  so a `--audit` path pointing at a link appends JSONL to the link's target.
  Append-only, so nothing is destroyed, and the surrounding directory checks in
  `resolve_audit_path` now refuse protected locations — but the open itself is
  unguarded. A proper fix needs `O_NOFOLLOW` (a new `libc` dependency); a
  `symlink_metadata` pre-check would be racy. Deferred deliberately, recorded
  here so it is not lost.
- [ ] **Bind confirmation to a magnitude** — the GUI re-scans at execute time,
  so the plan that runs is not the plan the user was shown. The mass-delete
  flag is now derived from the preview rather than hardcoded, but a preview
  already over the threshold authorizes an arbitrarily larger fresh plan. Send
  the previewed count/bytes and refuse if the fresh plan materially exceeds them.
- [ ] **Async commands + real scan progress** — the `#[tauri::command]`s are
  synchronous and run inline, so a 12 s scan freezes the window; and the
  loading state is a static pulse. Also stop serializing `report.items` (one
  record per file, unread by the UI) over IPC.

### Hardening (autonomous dev loop)
- [ ] **Batch/quiet disposal** — the executor disposes one file at a time, so
  macOS plays the Trash chime per file and does a Finder round-trip each (noisy +
  slow). Move to `trash::delete_all` (single op → one sound, far faster):
  re-guard each path, batch-dispose the validated set, audit each, decide
  failure semantics. *Deletion-logic change → `deletion-safety-reviewer` required.*
- [ ] **Scan progress + speed** — 12 s for 230k files, single-threaded, no
  progress in the GUI. Parallelize the walk and emit progress events (replace the
  static loading skeleton with real progress).
- [ ] **CLI per-category scoping** — let the CLI act on a chosen category (the GUI
  already can); parity + safer first real cleans.

### UI prettification (prettify loop)
Lift the GUI from "correct/standard" to distinctive + delightful (rubric
dimensions 9–10). Add competitor screenshots to `design/references/` first.
- [ ] **Clean view** — considered type scale, category iconography, a size
  visualization with character (proportional/stacked, not a flat bar), depth.
- [ ] **Confirm modal + Done state** — polish + restrained motion (~150–200ms).
- [ ] **Startup view** — iconography + clearer run-at-login emphasis.
- [ ] **App-wide theming** — refined token palette (hover/active/selected),
  hairline borders; optional light mode.
