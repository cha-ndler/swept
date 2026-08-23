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
- [x] **Bind confirmation to a magnitude** (PR #20) — the GUI re-scans at execute
  time, so the plan that runs is not the plan the user was shown. The mass-delete
  flag is now derived from the preview rather than hardcoded, and the previewed
  count/bytes travel with the request as `Expected`; a fresh plan that materially
  exceeds them (beyond a small churn allowance) is refused rather than executed.
  An empty selection now disposes of nothing instead of meaning "all categories".
- [x] **Async commands + real scan progress** (PR #21) — the `#[tauri::command]`s
  were synchronous and ran inline on the webview's message loop, so the window
  froze for the whole scan (measured: 165k files, 8.4 s warm / 36.7 s cold at
  56 % CPU). Now `async` + `spawn_blocking`, emitting cumulative counts on
  `scan://progress` batched every 2,000 files, with a Scanning state that shows
  real figures instead of a static pulse. `report.items` (one record per file,
  unread by the UI) is no longer serialized to the GUI: **37.4 MiB → 796 bytes
  per scan**, a ~49,000× reduction. The CLI's `--json` contract is unchanged.
  Verified by sampling the running app — the walk is on `tokio-rt-worker` while
  the main thread sits in `tao::event_loop::run`.

### Hardening (autonomous dev loop)
- [ ] **Batch/quiet disposal** — the executor disposes one file at a time, so
  macOS plays the Trash chime per file and does a Finder round-trip each (noisy +
  slow). Move to `trash::delete_all` (single op → one sound, far faster):
  re-guard each path, batch-dispose the validated set, audit each, decide
  failure semantics. *Deletion-logic change → `deletion-safety-reviewer` required.*
- [ ] **Parallelize the walk** — progress reporting shipped in PR #21; the walk
  itself is still single-threaded. Measured at 56 % CPU on a cold cache, so it is
  I/O-bound and parallelism buys less than it looks like it should — worth doing,
  but not the win the wall-clock number suggests.
- [ ] **CLI per-category scoping** — let the CLI act on a chosen category (the GUI
  already can); parity + safer first real cleans.

## v0.4 — The native Mac shell (prettify loop)

Lift the GUI from "correct/standard" to a **native Mac pro-tool** — the
DaisyDisk / Raycast / Linear register (rubric dimensions 9–10). The old plan here
was "add competitor screenshots to `design/references/` first", which was never
going to happen: they are third-party copyrighted UI and can't be committed or
shipped. U0 solves that by generating our own.

**Every task in v0.4 is visual → PR with screenshots + a `ux-critic` scorecard,
and PAUSES for the human taste gate. Never auto-merged.**

- [x] **U0 — Design canvas + rubric** — `design/canvas/index.html` is a
  10-artboard design target (foundations, shell, Smart Scan idle/scanning/
  results, confirm sheet, Large & Old, Space Lens, onboarding, states), rendered
  to `design/references/artboard-*.png` by `design/canvas/render.mjs`. These are
  **first-party exemplars**: we generate them, so they can be committed,
  versioned and diffed with no copyright question, and the `ux-critic` finally
  has something concrete to compare against. `design/rubric.md` rewritten toward
  the native-Mac-pro-tool target with measurable specs (token table with real
  measured contrast ratios, type scale, 4pt grid, three elevations, two motion
  durations, an automatic MUST-FIX list) and the stale duplicated block removed.
  The palette is verified: worst text pair 4.77:1, worst graphic pair 3.70:1,
  zero AA failures — and the accent is split into three roles (`fill` / `graphic`
  / `text`) because the vivid system blue is only 3.65:1 against white and cannot
  legally carry a white label.
- [x] **U1 — Window + chrome** (PR #23) — `titleBarStyle: "Overlay"` +
  `hiddenTitle` inset the traffic lights over our own content; `minWidth`/
  `minHeight`; `macOSPrivateApi` + `transparent` + `windowEffects: ["sidebar"]`
  for a real `NSVisualEffectView`; a real CSP replacing `null`.
  **Two traps found:** `start_dragging` is *not* in `core:default`
  (`tauri/build.rs` has `("start_dragging", false)`), so with Overlay the window
  could not be moved at all until it was granted explicitly — the ACL manifest
  from #21 is what made that enforced rather than accidentally working. And the
  drag region must sit on non-interactive spacers, since Tauri only treats an
  element as a handle when the attribute is on the event target itself.
  **Note:** `transparent` on macOS requires the private API, which forecloses
  Mac App Store distribution. Consistent with D2 (Developer ID + direct `.dmg`),
  but it is a one-way door.
- [x] **U2 — Module sidebar replaces the tab bar** (PR #23) — a persistent
  sidebar with icons and live size badges. Modules are **mounted on first visit
  and kept alive**, so switching no longer discards state and re-runs the whole
  scan; and the start module is derived during first render rather than in an
  effect, so `?tab=startup` no longer mounts Cleanup, fires a full scan and
  throws it away. Only modules that exist are listed — the other five would be
  dead rows promising a capability that isn't there.
- [x] **U3 — Design system** (PR #24) — token block, type scale, elevations,
  motion and icons landed with the shell in #23; the stock `<select>` and
  `<input type=checkbox>` are now a segmented radiogroup and a 14px accent
  checkbox, both keeping real semantics underneath. The webkit number spinners
  are hidden — they reserve width inside the field and clipped the placeholder.
- [x] **U4 — Smart Scan hero** (PR #24) — the category ring, with the 52px
  tabular total inside it. Sweeping during a scan and filled after, so the two
  states are one continuous object. **Deviates from artboard 05 on purpose:** the
  artboard has a ring *and* a stack, which encodes the composition twice; the
  rubric says "stack *or* ring", so the stack is gone and the per-row dots tie
  each row to its arc.
- [ ] **U5 — Flow polish** — confirm sheet, Done state (currently a dead end with
  no way back to the scan), empty/error/onboarding states, restrained motion.
- [ ] **U6 — Menu-bar extra** — `tauri-plugin-positioner` + a tray icon showing
  reclaimable space with a quick-clean action.

## v0.5 — Modules

The architectural spine, which must not be violated: **widen what we can see;
never widen what we can dispose of — escalate per-path with explicit consent
instead.**

- [ ] **M1 — Discovery/disposal scope split** — `allowlist::default_roots` stays
  exactly as it is (the *disposal* allowlist, every existing invariant test
  untouched). Add a read-only `discovery_roots` that yields plain `PathBuf` and
  never constructs a `SafePath`, plus `Consent.granted: Vec<SafePath>` for
  individually user-picked paths. Grants are enumerated (no globs, no directory
  expansion), capped, separately audited, and never pre-selected. *Requires
  `deletion-safety-reviewer`.*
- [ ] **M2 — Large & Old Files** — read-only walk of `discovery_roots` with
  size/age thresholds, feeding `Consent.granted`. Never pre-selected, never in
  Smart Scan's default selection.
- [ ] **M3 — Space Lens** — parallel depth-capped directory-size walk producing a
  tree DTO. Purely read-only; never touches the executor.
- [ ] **M4 — Uninstaller (leftovers-only)** — leftovers for a chosen bundle id
  under `~/Library/{Application Support,Caches,Preferences,Containers,…}`.
  **Removing the `.app` bundle itself is out of scope** — `/Applications` is on
  `PROTECTED_ABS_ROOTS` and carving it out is a denylist amendment needing
  explicit sign-off. **Depends on `guard_dir`** (see v0.3): leftover trees are
  directory actions, and `guard()` alone only suffices while every target is a
  single file. *Riskiest task in the plan.*
- [ ] **M5 — Privacy** — browser caches/cookies/history. **Cookies sign the user
  out everywhere** — separately opt-in, never pre-selected, never in Smart Scan
  defaults, labelled with that consequence.
- [ ] **M6 — Maintenance (honest scope)** — reversible login-item management
  (move the plist to a managed disabled folder, not disposal). **Explicitly out
  of scope:** flush DNS, purge RAM, rebuild Spotlight, repair permissions — all
  need `sudo`, which under a hardened notarized app means a privileged helper
  (`SMAppService`), a project of its own. Say so rather than ship a button that
  silently fails.
- [ ] **M7 — Smart Scan** — orchestration over M2–M6 plus the existing cleaners:
  one button, one combined result, one total. Defaults include only the
  conservatively-safe categories; Large & Old, Privacy cookies and Uninstaller
  leftovers are opt-in and shown separately.

## v0.6 — Distribution

Nobody can install this today: the repo is private, the `.dmg` is unsigned and
un-notarized, and it is Apple Silicon only.

- [ ] **D1 — Universal binary** — CI runs a bare `cargo tauri build` with no
  `--target`, so it inherits the Apple Silicon runner and Intel Macs get no
  download at all. Add both targets and `--target universal-apple-darwin`.
  **Gotcha:** that moves the bundle output to
  `target/universal-apple-darwin/release/bundle/…`, so the upload and release
  globs must both change or the job fails on `if-no-files-found: error`.
- [ ] **D2 — Signing + notarization behind optional secrets** — a `bundle.macOS`
  block (`signingIdentity` from env, `hardenedRuntime`, `entitlements.plist`,
  `minimumSystemVersion`), CI guarded by `if: secrets.APPLE_CERTIFICATE != ''`.
  **Unsigned builds keep working unchanged when the secret is absent**, so none
  of this is wasted if a certificate never appears.
- [ ] **D3 — Auto-update** — `tauri-plugin-updater`, a signing keypair,
  `createUpdaterArtifacts`, `latest.json` on the release. Without it every user
  is stranded on whatever version they downloaded.
- [ ] **D4 — Install story** — a landing README with a download button, real
  `.app` window screenshots (today's `docs/*.png` are headless browser renders
  with no window chrome), a Homebrew cask, and — while unsigned — the exact
  macOS 15+ walkthrough, since Apple removed the right-click → Open bypass. Add
  `LICENSE` (MIT is claimed in the README but the file doesn't exist),
  `CONTRIBUTING.md`, `SECURITY.md`, issue templates.
- [ ] **D5 — Release hygiene** — single-source the version (currently
  triplicated across `tauri.conf.json`, `src-tauri/Cargo.toml` and
  `package.json` with nothing enforcing agreement); feed `CHANGELOG.md` into
  release notes; add the missing `[0.2.0]` link reference; publish checksums;
  run the `package` job on PRs so bundling regressions surface before merge.
- [ ] **D6 — Public flip** — *(separate, on the human's word.)* Tidy the stale
  merged remote branches, then make the repo public.
