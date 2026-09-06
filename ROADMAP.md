# Swept roadmap

This is the autonomous development loop's compass. Each iteration: pick the first
unchecked item, complete it to its **verifiable goal** via the `/loop` workflow
(test-first → clippy/fmt → deletion-safety-reviewer if deletion logic changes →
verifier → PR → CI green → merge), check it off here, then start the next.

**v0.1.0 (shipped) = a complete, safe CLI.** **v0.2 = a pleasant GUI** (Tauri),
reusing the UI-agnostic `swept-core` crate and its `--json`/`ScanReport`
contract. The CLI was the foundation; the GUI is a second front-end, not a rewrite.

## Done
- [x] Safety substrate + Claude Code harness (PR #1)
- [x] Age-based cleanup filter `--older-than-days` (PR #2)
- [x] `--json` structured scan output (PR #3)
- [x] Cleaner-category registry + Homebrew downloads (PR #4)
- [x] Large-old-files finder (`--min-size`) (PR #5)
- [x] Startup/login-items inspector (`swept login-items`) (PR #6)
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
`swept-core` directly (no shelling out) and stays a thin front-end over the
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
  `swept-core` returning serde DTOs (`scan_report`, `list_login_items`,
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
- [x] **Denylist: `.git` must fold case** — every other rule in `denylist.rs`
  compares case-insensitively, with a doc comment explaining why (macOS volumes
  are case-insensitive by default and `realpath` does not case-normalize). The
  `.git` rule did not: it compared byte-exactly, so a working repository spelled
  `.GIT` was invisible to it. Latent while disposal was file-only, but
  `guard_dir` is exactly what turns one missed `.git` component into a recursive
  removal of the tree around it — a vendored checkout inside an uninstaller
  leftover tree would have gone with it. Found by `deletion-safety-reviewer`,
  which demonstrated it empirically before it shipped.
- [ ] **Refuse to run as root** — `Sink::delete` relies on `unlink(2)` returning
  `EPERM` for a directory, which `unlink(2)` documents as conditional on the
  effective user not being the super-user. Nothing checks `geteuid`, and
  `sudo swept clean --execute --permanent --yes` is reachable. The exposure
  is small (a stray directory-entry removal, not a recursive unlink) but the
  guarantee should not be conditional on something unenforced.
- [x] **Directory disposal needs `guard_dir`** — `safety::dir_guard` walks the
  tree with an explicit stack (never recursion, so a pathological tree hits
  `max_depth` rather than overflowing) and consults the denylist at every
  depth, which is what refuses a `.git` — directory *or* file, so git worktrees
  and submodules count — anywhere in the subtree. Fails closed on every
  uncertainty: an unreadable subdirectory refuses the whole tree rather than
  under-counting it, and the entry/size/depth limits are refusals, never
  truncations, because a truncated walk describes a different tree than the one
  about to be removed. Symlinks are counted as entries but never followed and
  never contribute bytes (a recursive removal unlinks the link, not its
  target); entries on a different device are refused as mount points, and
  directories are re-canonicalized as a TOCTOU re-check that they are still
  inside the root.
  Returns the real recursive count and size, which is what makes an informed
  mass-delete confirmation possible (item 5).
  **Also closes the M1 residual:** the executor now refuses *every* directory
  target, allowlisted or granted, and `Sink::delete` is files-only
  (`remove_file`, which returns `EPERM` on a directory) so the unavoidable
  check/use race fails closed instead of recursing. `guard_dir` is deliberately
  not wired in yet — nothing plans directory actions, and adding an unused
  destructive capability to this tool is the wrong trade. M4 turns the blanket
  refusal into a `guard_dir` gate alongside directory-aware planning.
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
- [x] **U5 — Flow polish** (PR #25) — sheet entry motion (220ms, opacity +
  scale .96→1, disabled under `prefers-reduced-motion`); the Done dead end was
  fixed in #23. **Subsumes the open v0.3 Full Disk Access item:**
  `gui_core::probe_permissions` reads the two TCC-gated roots and the UI says
  when a scan is under-reporting.
  **Confirmed on a real machine, not hypothetically:** a read-only CLI scan
  returns only four categories — homebrew-downloads, user-caches, user-logs,
  xcode-derived-data — and the TCC-gated `trash` category is *absent entirely*.
  Every figure the app has ever shown on this Mac was missing it.
  **Deviation from artboard 09, on purpose:** a first-run onboarding screen only
  warns once and is clicked past. A notice attached to the affected scan warns
  every time it applies, including after access is later revoked.
  `open_privacy_settings` takes no arguments and opens one hardcoded URL rather
  than granting the webview a general open-a-URL permission.
- [x] **U6 — Menu-bar extra** (PR #26) — a status item showing the latest
  reclaimable figure, with Open and Quit. **No cleanup action**, per the scope
  correction: disposing from a menu means no preview and no confirmation, which
  the safety contract forbids (item 1: dry-run is the default; item 5: no
  unconfirmed mass delete). The frontend sends the *string it is already
  displaying* rather than a number to re-format, so the menu bar and the window
  cannot drift apart; a failed scan clears it rather than leaving a stale figure.
  Closing the window now hides it instead of quitting, which is both the macOS
  convention and a precondition for a menu-bar extra outliving its window.
  **Verified created, not verified visible.** A temporary diagnostic build
  reported `tray built`, `set_title -> Ok(())` and `tray_by_id -> true`, so the
  item exists. It does not appear in the window server's layer-25 list on this
  Mac, whose menu bar already holds ~10 items on a notched display — macOS
  silently hides status items that do not fit, and one neighbouring item is
  already present at 0x0 for the same reason. Needs a human's eye on a machine
  with menu-bar room.

- [x] **U7 — The Clean screen presented a floor as a total.** `ScanReport`
  gained `skipped_unreadable` and `partial` when the scan learned to tell an
  unreadable directory from an empty one, and `CleanView` ignored both: it
  rendered `total_bytes` unqualified and its empty state showed a **green
  success shield** reading *"Nothing to clean — your Mac is tidy"* even when
  `~/.Trash` could not be read, which on any Mac without Full Disk Access is
  every time. Every other module's view already surfaced its own `partial`, so
  this was the one screen in the app making a claim it had not earned — on a
  tool whose entire pitch is that it does not overstate.
  Fixed before the public flip rather than after. The empty state drops the
  shield for a lock in the "shown, not acted on" hue and names how many places
  could not be looked into; results grow a `FloorNotice` in `LargeOldView`'s
  shape, which **stands down when the permission probe already explains it**
  so `AccessNotice` and its actionable button are not duplicated; the ring
  caption becomes *"reclaimable, at least"*; and the menu-bar label — which has
  no room for a caveat — carries a `≥`.
  Two new fixture states and four new baselines, because the states did not
  exist before and a gate that never saw them is not a gate.

**v0.4 complete.**

## v0.5 — Modules

The architectural spine, which must not be violated: **widen what we can see;
never widen what we can dispose of — escalate per-path with explicit consent
instead.**

- [x] **M1 — Discovery/disposal scope split** — `allowlist::default_roots` is
  unchanged (the *disposal* allowlist; every existing invariant test stayed
  green untouched) and is now pinned by an explicit assertion, so widening it
  can only ever be a deliberate edit. Alongside it, a read-only
  `discovery_roots` — `~/Documents`, `~/Downloads`, `~/Desktop`, `~/Movies`,
  `~/Music`, `~/Pictures`, `~/Library/Application Support`, `/Applications` —
  which yields plain `PathBuf` and can never mint a `SafePath`.
  `Consent.granted: Vec<SafePath>` carries individually user-picked paths;
  matching is exact (never a prefix, so a directory grant confers nothing on its
  contents), capped at `MAX_GRANTS` = 1000 with over-long lists refusing the
  whole run in both dry-run and execute modes, audited with a distinguishing
  note, and evaluated *after* the pre-mutation re-guard so the denylist always
  wins. Directory grants are refused outright until `guard_dir` exists.
  Irreversible removal stays confined to the allowlist — a granted `Permanent`
  action falls back to the Trash even under `--permanent`, because grants cover
  the least replaceable and least vetted data the tool will ever touch. The
  preview runs the same authorization as the real run, so it cannot report
  "would be trashed" for something the executor would refuse, and a wholesale
  refusal (over-long grant list, unconfirmed mass delete) now leaves an audit
  record instead of none.
  **Residual, deliberately not fixed here:** a plan action naming a directory
  *inside* the allowlist would still reach `remove_dir_all`. No caller produces
  one (`scanner.rs` plans files only), and closing it generally is `guard_dir`'s
  job — see the v0.3 item, which M4 depends on.
- [ ] **M2 — Large & Old Files** — *module shipped; one follow-up open.*
  - [x] **Engine + command layer.** `core/src/largeold.rs` walks
    `discovery_roots` read-only with size/age thresholds, prunes protected
    subtrees wholesale (so `.git` working trees and `/Applications` never
    appear), skips symlinks, keeps the largest N while still reporting the true
    totals, and reports `truncated` / `skipped_unreadable` rather than
    under-reporting silently. It yields plain `PathBuf` and never mints a
    `SafePath`. `gui_core::dispose_selected_with_sink` is the only caller that
    populates `Consent.granted`: it re-`guard`s every path, **requires each path
    to already be its own canonical spelling** (so a symlink swapped in after
    the walk cannot redirect a grant onto a file the user never saw),
    **confines disposal to the discovery scope** (the denylist alone would let
    any ordinary file on the volume through), re-reads sizes from
    disk (never trusting the frontend's numbers), de-duplicates, and **refuses
    the whole request** if any item is protected, missing, out of scope, or a
    directory — a partial run is never what the user confirmed. Every such
    refusal is recorded via `executor::record_run_refusal`. Selections use a 1 MiB drift
    tolerance and exact count matching, deliberately *not* the 64 MiB
    cache-tuned `grew_beyond` (see `SELECTION_CHURN_BYTES`).
  - [x] **The module UI.** Sidebar module + `LargeOldView`: rows never
    pre-selected, no select-all, primary action disabled until a human ticks
    something, the `partial` flag surfaced as a "this is a floor, not a total"
    notice, and a confirmation sheet that **names the chosen files** and
    discloses the mass-delete threshold when it is crossed. Two silent failures
    surfaced on the way and are fixed at the root: **13 of 16 opacity-modified
    colour classes app-wide emitted no CSS** (Tailwind cannot alpha a `var()`
    holding hex — tokens are now RGB channels), and **every confirmation-sheet
    screenshot since the clean flow shipped was a mid-animation frame**
    (`page.screenshot` does not disable animations; `toHaveScreenshot` does).
  - [ ] **Is `~/Library/Application Support` a *grantable* scope, or only a
    readable one?** It is a discovery root, so Large & Old lists what is in it
    and `dispose_selected_with_sink` will act on a selection from that list.
    That directory is application-private data by definition. The browser case
    was a live hole and is closed — a path inside a browser's own root is
    refused unless a `Regenerable` row covers it, because this screen shows
    sizes and dates and cannot obtain consent for "you will be signed out
    everywhere". But the boundary is "browsers this app can name", and two
    things sit outside it: `privacy::UNSUPPORTED` names Opera, Orion and Tor
    without recording a root, since the module will not guess an unmeasured
    layout; and a password manager's vault or a messaging database has no row
    and no root to match on at all. **Closing those by listing more paths is the
    exclusion-list posture `privacy.rs` argues against**, so the real question
    is the scope: keep the directory readable for Space Lens and Large & Old's
    *picture*, and stop it being grantable. That narrows a shipped feature, so
    it needs the human's word.
    **Answered (2026-09-06): keep it readable, stop it being grantable, and let
    the user override with the risk stated and attested to.** The design that
    follows, to be built and reviewed on its own:
    `~/Library/Application Support` stays in `discovery_roots`, so nothing about
    the picture changes. `dispose_selected_with_sink` refuses a path under it
    unless the request carries an explicit attestation — a second consent axis,
    exactly the shape `Acknowledged` already has for Privacy, and **refused by
    default via `#[serde(default)]`** so a frontend that loses the field gets a
    refusal rather than the wider behaviour. The GUI asks for it in words that
    make the risk concrete (a password manager's vault, a messaging database,
    the only copy of an app's data), per run and never remembered; the CLI takes
    the same attestation as a flag whose name says what it is. What this must
    *not* become is a preference set once and forgotten — "attested" has to mean
    attested for this action, or it is the old behaviour with a checkbox in
    front of it.
  - [ ] **The scope line under-states the scope.** `LargeOldView` prints
    "Documents · Downloads · Desktop · Movies · Music · Pictures", but
    `discovery_roots` also includes `~/Library/Application Support` — which the
    walk really does search. Found while writing the user docs. A one-string fix,
    but it changes a screenshot, so it goes through the visual gate.
- [ ] **M3 — Space Lens** — *engine, command layer and visualization done; the
  follow-ups below are open.*
  - [x] **Engine + command layer.** `core/src/spacelens.rs` measures the
    discovery roots with one thread per root and returns a tree DTO. It is the
    first module that cannot authorize anything even in principle — no
    `SafePath`, and no companion command that takes a node back — so a treemap
    rectangle is a picture of the disk, never a proposal. Design decisions worth
    keeping: **the caps are display decisions, not accounting ones** (sizes are
    always computed to the bottom of the tree; past the depth cap a node keeps
    its bytes and sets `collapsed`, and past the width cap the remainder becomes
    one rollup node — so `bytes == sum(children)` holds at every level that has
    children); **hard links are counted once** via a `(dev, ino)` set shared
    across the root threads (the total is deterministic, the attribution is
    not); **symlinks contribute nothing and are never descended**, which is also
    what makes a link loop impossible; and `skipped_unreadable` /
    `truncated` / `skipped_too_deep` drive `partial`, while
    `deduped_hardlinks` deliberately does not — deduplicating makes the figure
    more accurate, not less complete.
  - [x] **The visualization.** A three-ring sunburst with a breadcrumb, a
    largest-children list beside it, and the `partial` caveat. The module's own
    claim — that it cannot act — is carried by a **"Read-only view"** mark in
    the toolbar and a footer naming Large & Old as where consent happens; the
    UX suite asserts the *absence* of any disposal control, which nothing else
    in the app has to. The sunburst is `aria-hidden` and the list carries the
    same navigation as real buttons, so no wedge is the only way to anything.
    Three things surfaced on the way, all fixed at the root: **the UX oracle
    was scoring a different git worktree** (a stale `vite preview` on 4173 plus
    `reuseExistingServer: true` — now `false`, so an occupied port fails loudly
    instead of silently substituting a build); **screenshots recorded whatever
    hover state the last click left behind** (`capture()` now parks the
    pointer, the same class of bug as the mid-animation frames); and
    **`opacity-45` as a de-emphasis took row text from 12.09:1 to 4.06:1**,
    caught by axe — emphasis is now drawn by lifting the match, never by fading
    the rest.
  - [ ] **Re-measure on drill-in.** The tree is whatever one walk produced, so
    drilling past `DEFAULT_MAX_DEPTH` stops at a `collapsed` node instead of
    measuring deeper. Needs a `space_lens_at(path)` command, which means
    accepting a path from the frontend — read-only, but still a new surface to
    confine to the discovery scope.
  - [x] **Bound the materialized node count.** `24` children over `4` levels
    admitted a six-figure node count on a `node_modules`-heavy disk, all
    serialized to the webview in one payload — real homes are nowhere near it,
    which is exactly why it would not have been noticed until it was.
    `DEFAULT_MAX_NODES` (20k) is a third **display** cap alongside depth and
    width: sizes are still computed to the bottom of the tree, so the totals do
    not change and `partial` stays false — the directories that were not
    expanded are simply `collapsed`. The budget is read once per directory
    rather than per push, because a half-materialized listing would break
    `bytes == sum(children)`: `cap_children` folds the *unlisted* remainder into
    a rollup, so children dropped after that decision would take their bytes
    with them. That costs a bounded overshoot of `max_depth * (max_children+1)`
    per thread, which the test pins. Fixed the adjacent lie while here: a
    directory that was empty *and* past a cap used to report `collapsed`, so the
    UI offered "there is more inside" over nothing.
  - [ ] **Reveal in Finder.** The design artboard offers it; there is no such
    command, so the UI deliberately does not promise one. Adding it means
    `open -R` with a frontend-supplied path.

  Two things this surfaced that are **not** M3's to fix:
  - **Space Lens reports allocated bytes (`st_blocks × 512`); Large & Old
    reports apparent bytes (`st_size`).** Each is right for its own question,
    but the same file can legitimately show two different sizes in two modules.
    M7's single combined total has to pick one.
  - **`/Applications` is in `allowlist::discovery_roots`, but `resolve_roots`
    drops it** — it is on `PROTECTED_ABS_ROOTS`, and `resolve_roots` filters
    protected roots. So neither Space Lens nor Large & Old ever looks there.
    That is the right outcome for both (you do not reclaim space by disposing of
    an app from a size list), and the entry is not dead — it exists for M4's
    Uninstaller, which enumerates bundles directly rather than through
    `resolve_roots`. Worth writing down because the two are easy to conflate:
    the roots list says what may be *read*, and `resolve_roots` is a stricter
    filter that two specific walkers apply on top of it.
- [x] **M4 — Uninstaller (leftovers-only)** — *shipped: id-keyed discovery,
  containers and the human-name tier, directory-aware disposal, and the
  Applications screen (merged at the taste gate). The under-match note below
  stays open as a known limit, and the design's open questions are still the
  human's to answer.*
  **Removing the `.app` bundle itself stays out of scope** — `/Applications` is
  on `PROTECTED_ABS_ROOTS` and carving it out is a denylist amendment needing
  explicit sign-off. *Riskiest task in the plan.*
  - [x] **Leftover discovery, id-keyed.** `core/src/uninstall.rs` searches nine
    locations for a bundle id and reports what it finds, read-only. The whole
    module exists to get one predicate right, because the failure mode is
    offering a **still-installed** app's data and nothing downstream objects —
    the denylist has no opinion about who owns `~/Library/Caches/com.acme.Notes`.
    The rule, in three load-bearing parts: **segments, never bytes**
    (`com.acme.Note` must not claim `com.acme.Notes`); **byte-exact case**, with
    the near-miss counted rather than folded — everywhere else in this codebase
    folding can only *protect* more paths and is right, here it can only *claim*
    more and is wrong in both directions at once; and **the longest installed
    owner wins**, so `com.acme.Suite.Reader` is withheld from `com.acme.Suite`
    while Reader is installed. Nested helper ids go into the owner index and are
    used **only to withhold, never to claim**, which handles a crash reporter
    embedded in six vendors' apps with no special case. Also: an unreadable
    `/Applications` **refuses the scan** rather than reporting an orphan, and
    `inventory_roots` deliberately does **not** go through `resolve_roots` —
    that function drops denylisted roots and documents it as "nothing to
    report", which is right for a size walk and catastrophic for an authority
    check, since a shrunken inventory makes installed apps look uninstalled.
    Sizes count every *name*, the opposite of Space Lens, because a disposal
    unlinks names.
  - [x] **Containers, group containers and the human-name tier.** Three
    surfaces that are not id-keyed directories, each with its own posture.
    **A container is decomposed, never offered whole**: `~/Library/Containers/
    <id>` is the app's redirected home, and `Data/Documents` is where a
    sandboxed app puts the user's only copy of a file (Finder does not show
    it). Rows come from `CONTAINER_STATE_PARTS`, an **inclusion** list under
    `Data` — an exclusion list fails open the next time Apple adds a directory
    — and `Data/Documents` plus `Data/Library/Application Support` are shown
    as `Kind::UserData` and never offered. Every part is checked for being its
    own canonical spelling, because 82 of 822 real containers carry symlinks
    under `Data` that point back into the real home, and one whose `Library`
    is such a link would resolve `Data/Library/Caches` to `~/Library/Caches` —
    a location root. **A group container is shown and never claimed** — the
    entitlement that settles ownership is in the bundle that is gone — so the
    one *prefix* strip in the module (`group.` or a ten-character team id)
    exists only to show a withheld row. **A human name is a weaker key, gated
    three times** in `Application Support` only: byte-equal to a
    caller-supplied `DisplayName`, no installed app answering to that name (by
    `CFBundleName`, `CFBundleDisplayName` or `.app` stem), and an immediate
    child keyed on the target's id — via the same `claim`, so a child owned by
    an installed helper does not corroborate. Never bulk-grantable. Measured
    honestly: the corroboration gate admits **4 of 89** human-named
    directories on the reference machine. It is safe and nearly useless, on
    purpose, until a human loosens it (see the open questions). The cookie jar
    inside a container is deliberately not a state part: it is M5's surface
    and arrives with M5's consequence label.
    **For the disposal half:** confining a selection to the resolved location
    roots is no longer enough — `<container>/Data/Documents` is *inside* a
    location root. Disposal must intersect the selection with the `offerable`
    rows of a fresh scan, the way Large & Old re-walks before it acts.
  - [x] **Directory-aware disposal** — the executor half, the discovery
    honesty flags and the command layer.
    - [x] **The executor learns a second action shape.** `PlannedDirAction`
      carries a `SafeDir` — the tree walked in full by `guard_dir` — and
      deliberately **no `Disposal`**: a recursive irreversible removal is not
      expressible in the type, which is a stronger guarantee than a branch
      that declines to take it. `Plan::count` sees the recursive figures (one
      directory is `entries + 1` items, never 1), so a real cache directory
      crosses `MASS_DELETE_COUNT` and asks for confirmation — the intended
      reading of item 5. Authorization is **by grant only**: there is no
      allowlist route for a tree, because the allowlist was never a statement
      about trees. `Consent.granted_dirs` shares one cap with `granted`, so a
      selection cannot double its bound by splitting itself. At execute time
      the tree is **re-walked** (a `.git` that appeared, a component swapped
      for a symlink, a tree past `DirLimits` — all refused), must resolve to
      the planned path, and must **not have grown** since it was confirmed —
      in entries *or* in bytes, each half pinned on its own. The audit record
      carries the recursive `entries` count as data. **Any directory action
      requires the mass-delete confirmation**, before the numbers are
      consulted: the reviewer noticed `DirLimits::max_bytes` equals
      `MASS_DELETE_BYTES` and `guard_dir` refuses on `>`, so no single tree
      could ever trip the byte threshold, and one under `MASS_DELETE_COUNT`
      entries would slip under both — item 5 says *recursive* removals need
      confirmation, not large ones. Found and fixed on the way: since #27
      every *preview* refusal was logged with `"phase":"executed"` — nothing
      pinned the phase; a dry run now never writes that word, and a test says
      so. **For the command layer:** `guard_dir` canonicalizes a symlinked
      root, and `authorize_dir` has no allowlist backstop to notice the
      redirection, so a grant built from a symlink names and trashes the
      *target*. Discovery already drops symlinked rows, so nothing reaches
      this today — but the UI must display `SafeDir::as_path()`, never the
      string the user clicked, and the preview magnitude must come from
      `Plan::count()`/`total_bytes()`, never from `ExecReport`'s action
      counts.
    - [x] **Discovery honesty flags.** A row `guard_dir` is *certain* to
      refuse — a protected path inside the tree (a vendored `.git`), or a
      tree outside `DirLimits` — is now `offerable: false` with the reason in
      `Candidate::undisposable`, enforced end to end rather than left as a UI
      hint: `measure` used to treat a protected entry as "floor and skip",
      which left the row offerable. The bounds are `UninstallConfig
      .dir_limits`, injectable only so a fixture can reach them and pinned
      equal to `DirLimits::default()`, because if they diverged the flag
      would lie in the dangerous direction. Depth over-refuses by at most one
      level (a *file* at depth 33 would have been allowed; a directory would
      not), in the safe direction, and says so. Withholding for this reason
      does not make a report partial. `CFPREFSD_CAVEAT` rides on any report
      with a preferences row — including a container's own preferences part —
      so the UI can say it before the user acts; nothing is quit to prevent
      it. And a names-only `license_suspected` (`*.lic`, `*.license`,
      `*.activation`, `license*.plist`, `Receipts/` among the immediate
      children) keeps a row out of any bulk gesture without withholding it —
      nothing is opened to decide.
    - [x] **The command layer.** `gui-core::uninstall_leftovers_in` (the
      read-only DTO; offerable totals computed from the *emitted* rows so the
      header and the list cannot disagree) and `dispose_leftovers_with_sink`,
      whose ceiling is not a set of roots but **the `offerable` rows of a scan
      run inside the call**: a path is accepted only if it is byte-equal
      (`OsStr`, not `Path` — `Path` equality is component-wise, so `/x/./y`
      would pass) to one of them. That single intersection is what refuses a
      container root, a `Data/Documents` row, a group container, a withheld
      sibling, a tree `guard_dir` would refuse, a child of an offered row, and
      an app installed since the sheet was shown (no rows, so nothing is
      offerable). A scan that cannot complete — an unreadable application
      root — refuses the whole request; "I could not tell whether it is still
      installed" must never become a disposal. Then `guard` per file and
      `guard_dir` per directory, each required to be its own canonical
      spelling; any rejection refuses wholesale; drift is measured against
      what the sheet showed (rows and their sizes), not `SafeDir` figures.
      The frontend can name an id and a display name and **nothing else** —
      a command that could set the home or the inventory roots would let it
      make an installed app look uninstalled. `bulk_grantable == false` rows
      are individually disposable, on purpose and pinned. Two thin Tauri
      commands; `CleanSummary.entries_freed` says how many names a directory
      action stood for.
  - [x] **The module UI.** *(Merged at the human taste gate.)* Built as
    `UninstallerView` ("Applications" in the sidebar, under Clean). **Identity comes from a bundle the app saw:**
    the picker lists installed applications (a new read-only
    `installed_apps` command, top-level bundles only), and choosing one records
    its identifier *before* the user removes it — the interim answer to open
    question 1. An app already gone can be named by identifier, beside a
    caveat about `com.apple.` components. Picking an installed app yields the
    "Still installed" state with instructions, never rows. In the list, a row
    the backend withholds — the user's documents inside a container, a shared
    group container, a still-installed sibling's data, a tree disposal is
    certain to refuse — is rendered as **information, not a control**: a lock
    where the checkbox would be, the reason where the path would be, a tag
    saying which. Nothing is pre-selected and there is no select-all. The
    sheet says a folder is a recursive removal that needs the extra
    confirmation, however small; the `cfprefsd` caveat rides on the report and
    again on the sheet when a preferences row is selected; the done state
    says how many files the folders stood for.
  - [ ] **Team-id-prefixed containers are an under-match.** Two of 822
    containers on the reference machine are named `<TEAMID>.<id>`; the
    id-keyed matcher does not claim them (a prefix strip in `Containers` would
    be a claim, unlike the one in Group Containers). Under-reporting is the
    safe direction; recorded so it is not mistaken for a bug.

  Open questions the design surfaced, for the human:
  - **Where does the bundle id come from once the app is gone?** Either the UI
    records identity *before* the user removes the app, or an "orphan sweep"
    enumerates leftovers first and infers candidates. This decides whether
    `uninstall.rs` needs an `orphans()` entry point. **A measurement that
    constrains the answer:** on the reference machine 747 of 822 containers
    are `com.apple.*`, and 626 have no owner in the inventory at all — Apple
    system components living under `/System/Library`, outside
    `inventory_roots`. An orphan sweep over today's inventory would offer
    every one of them. So a sweep needs the inventory to cover system
    components first (and excluding `com.apple.` instead would also hide
    Xcode, GarageBand and Keynote, all legitimately removable). It also means
    the per-id path must not take a free-typed id from the UI: identity should
    come from a bundle the UI saw, so a user cannot name a system daemon by
    accident.
  - **Is a subprocess ever acceptable here?** Reading
    `com.apple.security.application-groups` via `codesign` is the only
    mechanical route to Group Containers, and there is no subprocess anywhere in
    this workspace today. Answered "no" for now, which makes group containers
    permanently unclaimable.
  - **Should disposal require the app be quit?** `cfprefsd` can resurrect
    `~/Library/Preferences/<id>.plist` after removal, so "removed" would be true
    at the moment of the action and false a second later.
  - **Should the name tier's corroboration be loosened?** As shipped it needs
    an id-keyed child *inside* the directory, which admits 4 of 89 on the
    reference machine. The alternative is report-level corroboration — the
    same scan found an exact-id row somewhere else — which would admit most of
    the 89 but rests on the caller's word that this name and this id are the
    same app. Safe-and-useless was chosen over useful-and-trusting; the choice
    is the human's.
  - **Should `Data/Library/Application Support` inside a container ever be
    offerable?** It is where a sandboxed notes app keeps the notes, and also
    the largest thing an uninstalled sandboxed app leaves behind. Shown and
    never offered today; "offerable, never bulk" is the plausible loosening.
- [x] **M5 — Privacy** — *shipped: discovery (#42), disposal (#43) and the
  screen (#44, merged at the taste gate).*
  **Cookies sign the user out everywhere** — separately opt-in, never
  pre-selected, never in Smart Scan defaults, labelled with that consequence.
  - [x] **What browsers remember, read-only.** `core/src/privacy.rs` searches
    twelve browsers — Safari, Firefox, and ten of the Chromium family as one
    table row each — for five classes per profile: cookies, history, session,
    website storage, in-profile caches. The whole module exists because the
    trust kernel protects **nothing** here: a Firefox profile keeps
    `cookies.sqlite` and `key4.db` — the key that decrypts every saved
    password — in the same flat directory, and Chromium keeps `Cookies` and
    `Login Data` as byte-adjacent siblings. Both pass `guard` cleanly. So the
    rule is an **inclusion list consulted by lookup, never by listing**: a
    constant name is joined onto a corroborated root and asked whether it
    exists, so a file this module does not already know by name is never seen
    rather than merely rejected. The lists are pinned, and the same test
    asserts eleven named precious files are absent from them.
    **No parsed string is ever joined onto a path.** The design called for
    reading Chromium's `Local State` and Firefox's `profiles.ini` and then
    validating the parsed names in six layers; instead profiles come from
    `read_dir` and are corroborated (a `Preferences` file; `prefs.js` or
    `times.json`), so the injection surface does not exist rather than being
    defended against — `uninstall.rs` states the absence of exactly that as
    one of its own invariants, and this keeps it. Cost: a Firefox profile
    outside the Firefox root is not found. Under-reporting is the safe
    direction.
    Sidecars are one row, ordered **journal, shm, wal, database last**,
    because `execute` continues past a failed action and a lone hot `-journal`
    beside a newly created empty database is the one corrupting outcome.
    Nothing under `~/Library/Caches` is emitted — already `user-caches`, and
    two routes to the same bytes is a double count in M7's combined total — so
    those are *named* in `covered_elsewhere`, which has no size field on
    purpose.
    Measured on a real machine, and three measurements changed the design: no
    `Default` profile exists (it is `Profile 1`..`3` among ~25 non-profile
    directories); Safari is hard-blocked without Full Disk Access, so
    `NotInstalled` and `NeedsFullDiskAccess` are kept apart; and Firefox's
    `.parentlock` is present whether or not Firefox is running, so keying
    liveness on it would withhold every row forever while looking like it
    worked — the real markers are `lock` and Chromium's `SingletonLock`, and
    both fail *positive*, withholding a row we could have offered.
    The safety review returned BLOCK with twelve findings, two of them found
    by deleting a line and watching every test still pass. **A tree whose
    measurement was cut short was offered as `0 B`** — the shared budget is
    spent on the withheld site-storage trees before the cache rows are reached,
    and `offer` judged rows only by `undisposable`, never by `size_is_floor`,
    which also defeated the size threshold that would have withheld them
    (an under-summed tree cannot exceed a limit). Fixed in `treewalk`, so M4 is
    hardened by the same change. **The canonical-spelling re-check had no
    test**, and without it a symlink named `Network` inside a profile emitted a
    row that is lexically inside its own profile root while naming a file
    somewhere else. And **Safari was exercised by nothing**, which hid a real
    bug: the access probe opened only `Library/Safari`, so when that was absent
    *or denied* — its resting state — the other three roots were never looked
    at. Also found: `~/Library/Cookies` is not Safari's jar but the one every
    non-sandboxed app shares, so it is now shown and never offered.
  - [x] **The disposal half.** `gui_core::dispose_privacy_with_sink`. Trash
    only, per-path grants only, ceiling = the `offerable` rows of a scan run
    inside the call (M4's rule), and confinement to each row's own
    `profile_root` — strictly stronger than M4's location roots, because one
    profile's row must never authorize a path in the next.
    **A row is a set of files and the frontend cannot take it apart:** the DTO
    carries the row's `path` and a `member_count`, never the member paths, so
    a sidecar cannot be named; the members are expanded here from the fresh
    scan, database last.
    **A second consent axis.** `confirm_mass_delete` answers "is this a lot?"
    and has nothing to say about "this signs you out of every site you use".
    `Acknowledged` carries one boolean per consequence, `Default` grants none,
    `#[serde(default)]` means a frontend that loses its checkbox state refuses
    rather than proceeds, each axis is separate, and an unacknowledged row
    refuses the *whole* request. Website storage is structurally
    unacknowledgeable as well as unofferable.
    One executor change, and it had to happen: `GRANT_DIR_NOTE` hardcoded
    "(uninstaller leftover)", which became a falsehood the moment a second
    module planned a directory action — a browser cache would have been logged
    as an uninstaller leftover. The note now carries the action's own
    category, so the log says which module, and which acknowledgement,
    authorized each line.
  - [x] **The module UI.** *(Merged at the human taste gate.)* Built as
    `PrivacyView` ("Privacy" in the
    sidebar, under **Protect** — the headings are about what a module is
    *for*, and this one is not about space). **The size is not the point**, so
    the stacked track is by consequence rather than location, every row wears
    the consequence as a tag and a glyph, the sidebar badge is a count rather
    than bytes, and the confirmation sheet asks for a **separate
    acknowledgement of each consequence in the selection** with the primary
    action disabled until each is ticked — the interface of a gate
    `dispose_privacy` already enforces, so a sheet that did not ask would
    produce a refusal the user could not act on. Withheld rows *recede* rather
    than being raised the way Applications does it, because here they are the
    majority. A reason shared by several rows is hoisted to the group header
    and said once. Denied and absent are drawn differently: Safari gets a card
    with a route to System Settings, and browsers with nothing to report get
    one line at the foot of the page.

  Decisions taken conservatively, each one the human's to loosen:
  - **A live browser withholds cookies/history/session**, and only caveats
    caches. A running browser rewrites the database on quit, so "history
    removed" would be visibly false a minute later.
  - **Website storage is shown, never offered** — `Local Storage`,
    `IndexedDB`, `storage/default` are where a local-first web app keeps the
    user's only copy. Same posture as a container's `Documents` in M4. It is
    also the largest row on the reference machine, so the cost is visible.
  - **Safari's container cookie jar is offered** — reversing the conservative
    call made when this module landed. M4's "no module offers a path inside
    another app's container" answers a question of *ownership*: a container may
    belong to an app that is still installed, and the entitlement that would
    settle who owns it was in the bundle that is gone. Neither half applies to
    Safari, which is always installed and whose browsing data the user has
    explicitly asked to clear. Withholding it bought no safety and meant the
    Safari half of the module could act on nothing at all on a current Mac.
    What sits under `WebKit` is still withheld — for being website storage,
    which is the true reason, rather than for being in a container.
  - **Firefox history is not offered at all** — not caution: `places.sqlite`
    holds the history *and* the bookmarks in one file, and separating them
    means editing rows inside a database, which is a destructive capability
    this tool does not have.
  - **No subprocess and no `libc`**, so liveness is marker *presence*, never
    proof of a running process. Verifying the pid would put the first
    `unsafe` FFI into `swept-core`; M4 answered "no subprocess" for
    `codesign` and this holds the line.
- [x] **M6 — Startup grows a verb** *(was "Maintenance")* — *shipped: the
  read-only report (#45, #46), the move primitive (#47), the command layer
  (#48) and the screen (#50, merged at the taste gate).* Reversible
  login-item management. **The milestone changed shape, and the human
  confirmed that shape by merging it.** A Maintenance *screen* would have been
  four disabled buttons beside one working list: the manageable surface is 5
  items on the reference machine, and the honest answer to most of the
  maintenance checklist is "this needs a privileged helper we do not install".
  Putting the "say so" on its own screen is the same mistake wearing a label.
  So the existing **Startup** screen learns to act instead, and the
  out-of-scope half becomes one short card at the foot of it — naming the
  one-line Terminal command for each, because telling someone the line is more
  useful than a button, and retiring "repair disk permissions", which has not
  existed since OS X El Capitan.
  - [x] **What runs at login, read-only.** `loginitems::scan` reports the
    user's LaunchAgents as rows, the moved-aside store, `/Library/Launch*` as
    controlless inventory, and the modern `SMAppService` store's *existence* —
    never its contents, because it is opaque, versioned and Apple-owned and a
    misparse would fabricate rows the user cannot cross-check.
    Three honesty fixes to shipped behaviour. **"Disabled" is not this app's
    word to use:** a plist's `Disabled` key is only the initial value for a job
    launchd's override database has not seen, that database is root-owned and
    unreadable here, and the two can disagree — so the field is
    `plist_says_disabled` and nothing reports a job as disabled. **`RunAtLoad`
    is not the whole story:** `KeepAlive` starts a job at load without it and
    `StartInterval` does not start one at login at all, so the count was wrong
    in both directions. **A file that is there and unexplained reads as a file
    the scan missed**, so a non-plist or an unparseable plist is now shown with
    its reason rather than skipped. `Broken` — an absolute program that is not
    there — is a class of its own and the *safest* thing to move aside, gated
    on `NotFound` only (never `PermissionDenied`) and on absolute paths only,
    because calling a working item broken is the wrong direction to be wrong
    in.
  - [x] **The mutating half.** `executor::stash` / `executor::restore`: a
    **hard link, an inode check, then the removal of the original name** —
    never `rename`, which replaces a destination silently, and never
    copy-then-remove. `hard_link` *is* the destination check, failing with
    `EEXIST` and creating nothing, so there is deliberately no
    `if dest.exists()` above it. Sibling types (`StashPlan`, `StashConsent`,
    `StashSink`) with no conversion to the disposal ones, so "a grant to set a
    plist aside cannot dispose of it" is a property of the types.
    Three review rounds, and the first two blocked. **The rollback removed a
    name it had never verified:** comparing the two paths to each other *after*
    linking conflates a swapped source with a swapped destination, and in the
    second case the file now at that name belongs to someone else and may have
    only that one name. The identity is taken before the link now, so each side
    is judged on its own. **The documented symlink refusal refused nothing:**
    both layers run after `guard`, which canonicalizes, so a plist that was
    already a link arrived as its target — a decoy would have set aside an item
    the user never ticked — and the test that claimed otherwise passed
    vacuously because the denylist refused its keychain target first. The plan
    now carries the listed spelling, sealed behind a constructor so a caller
    cannot back-fill it from the guarded path and make the check a tautology.
    **And a test named for the chokepoint never called the function it named.**
    One gap left open on purpose: the denylist half of that guard call is
    unreachable by construction, since `SafePath` cannot hold a protected path
    — stated in a comment rather than performed by a test built on a
    `#[cfg(test)]` bypass of the type system.
  - [x] **The command layer.** `startup_report_in`, `move_aside_with_sink`,
    `put_back_with_sink` and three thin Tauri commands. The ceiling is M4's
    rule with one addition: **each verb has its own set of rows and they do
    not overlap**, so asking to put back something still in `LaunchAgents` is
    a refusal rather than a no-op — it means the frontend and the disk
    disagree about where an item is. The review found a live falsehood rather
    than a test gap: `skipped_unrepresentable` was copied from Privacy, where
    the path is a `PathBuf` and the conversion can fail, but here the source
    is already a `String` — so the counter was always zero and `partial` could
    never learn anything, while the rows genuinely dropped for an unnameable
    filename were counted nowhere. A login item could vanish from a report
    that called itself complete. **No CLI subcommands, deliberately:** they
    would need `swept` to depend on the GUI's command layer, or a second
    copy of the ceiling. The store being pinned to one folder is the better
    recovery path, because it survives this app being removed.
  - [x] **The screen.** *Visual → taste gate; opened as a PR with screenshots,
    never auto-merged.* `StartupView` grows the verb, and its shape is one
    measurement: the app can act on **5 of 31** launchd jobs on the reference
    machine, so the disclosure that most login items live in a store macOS
    keeps to itself sits *above* the count, and what it can never change is a
    collapsed table with no controls near it — a row with a dead control reads
    as a refusal, and these outnumber the actionable ones five to one. The
    sheet asks once rather than per-consequence, because the action is
    reversible and ceremony on a safe action teaches click-through; what it
    carries instead is the timing. Three critic passes: the refused sheet
    asserted the success outcome; the ratio was never a *quantity* and the
    fixture had never rendered the design at its own design point (3 system
    jobs where a real machine has 26); and then the chart added to fix that
    shipped with **no colour key**, invisible to every gate. Final scores 4, 4,
    5, 5, 5, 5, 5, 5, 5, 4.

  The store is `~/Library/LaunchAgents/Moved aside by Swept/` — inside
  the folder the user already opens, because launchd does not recurse so the
  job is genuinely not loaded, and because uninstalling this app then strands
  nothing: putting an item back is dragging a file up one level. It is also
  what lets restore need **no recorded state at all** — the destination is the
  store's own parent — so no manifest ever names a path.
- [ ] **M7 — Smart Scan** — one button, one combined result, one total.
  *Screen shipped; one design decision open (the last sub-item).*
  **The milestone is narrower than "over M2–M6" as originally written, and that
  is a decision for the human to confirm** — the same class of change as M6's
  own reshape. Three sources are dispatchable and the rest are findings.
  - [x] **The default set is the registry's answer** (#54).
    `Category::smart_scan_default` lives beside `id` and `subpath`, because
    `privacy::Row::smart_scan_eligible` is already derived beside the rows it
    describes and a second answer kept in the aggregator would drift
    invisibly — both would still compile. `false` for the Trash, and not out of
    caution: it is the recovery mechanism for everything else this app does, so
    a gesture that empties it by default destroys its own undo in the same
    click. Also `false` for a category the registry does not know, which is the
    safe direction.
  - [x] **The read-only aggregator.** `gui-core/src/smartscan.rs` runs the
    sources and adds them up. It mints no `SafePath`, holds no `Consent` and
    calls nothing that mutates — Smart Scan is not a second disposal path, and
    what keeps that true is having no disposal code at all.
    **Two figures, and no bare byte count at the top level:** every one lives
    inside a `Total { bytes, from, incomplete }`, so a frontend cannot render
    the number without holding its completeness — the idiom `CoveredDto` and
    `StartupSummary` already set by omitting a size field on purpose.
    Incompleteness is attributed *per source* in that module's own words,
    because one boolean saying "some figure somewhere is short" is not
    something a notice on screen can be written from.
    **The invariant, and it is one test:** compute the headline, dispatch it
    immediately against a `DirSink`, assert the bytes freed are the bytes
    promised. It cannot pass while the total counts anything the verbs would
    refuse, and it needs no dry-run knob to say so.
    **No overlap-folding machinery, because there is no overlap** — and the
    earlier design assumed there was. `default_roots` and `discovery_roots` are
    disjoint, so cleanup and Large & Old cannot double-count; every path inside
    a browser root is refused by `dispose_selected_with_sink` since #53, so a
    Large & Old row there is not something the total may count either, and the
    aggregator filters on that same predicate rather than a copy of it; and
    Privacy's caches under `~/Library/Caches` are already reported without a
    size because `user-caches` covers them. Pinned, because "the scopes are
    disjoint" is a property of two lists someone may widen.
    Found while writing its own tests: letting the *cleaner's*
    `min_size_bytes` drive Large & Old's threshold would let the frontend widen
    what `dispose_paths` accepts through a control that appears to be about
    something else. Two knobs that share a name are still two knobs, so
    `SmartScanConfig` keeps them apart and pins the Large & Old floor to
    `DEFAULT_MIN_SIZE`.
  - [x] **The dispatch half.** Sequential, fail-fast, ledgered. It cannot be
    atomic — `trash::delete` has no rollback — and pre-flighting every module in
    dry-run was considered and rejected, because each verb's drift check
    compares against a scan run *inside* the call, so a green pre-flight says
    nothing about the real run at double the cost. One order,
    `cleanup → privacy → large-old`, chosen so the loosest drift tolerance
    (cleanup's ±10 %/64 MiB cache-churn allowance) runs first and the module
    most likely to refuse runs last, where its refusal strands nothing. **The
    exact claim is "no step begins after a step refused"**, not "the run is
    atomic": `executor::execute` already continues past a failed action inside
    step 1. Three outcomes per step, and the third is the point — `Executed`,
    `Refused`, `NotAttempted` — because *we did not try* must not serialize
    like *we tried and there was nothing*. Plus a backend-stamped
    `scanned_at_ms` freshness check, honestly a staleness guard against our own
    UI rather than authentication, and additive: deleting it should leave every
    existing safety test green.
    Two review rounds, fourteen findings, and the pattern in them is one
    sentence: **the set the dispatcher will act on must be the set the report
    offered**, and three predicates define that set. The first version enforced
    one. Each of the other two was a working data-loss path, reproduced by the
    reviewer against a fixture rather than argued: privacy rows the report never
    offered were disposable, because the verb's ceiling is wider than
    `smart_scan_eligible` and the acknowledgement axis was being threaded
    through (the field is gone now, so the two sets coincide by construction);
    and a 64-byte file was disposable by a gesture whose report listed no large
    files, because the size floor bounded the *offer* and nothing bounded the
    disposal. A category that merely exists was accepted where
    `smart_scan_default` is the real question — which would have emptied the
    Trash, destroying the undo for every other module in the same click and
    reporting bytes that were never freed.
  - [ ] **Connect the report to the request, and close the class.** Every
    corroboration in the dispatcher is either a magnitude the frontend supplies
    both sides of, or a static predicate someone has to remember to write.
    Nothing ties the report that was *shown* to the request that *acts* — which
    is why the same class of finding arrived three times in different clothes.
    Retaining the issued report server-side, keyed by its `scanned_at_ms`, and
    requiring a request to reference one would make the offer set a fact rather
    than a reconstruction. **It also means backend session state — lifetime,
    multiple windows, memory — so it is a design decision, not a fix.**
    **Decided (2026-09-06), with the smallest shape that closes the class:** a
    bounded in-process map of issued reports keyed by `scanned_at_ms`, capped at
    a handful of entries and evicted by the `MAX_REPORT_AGE_MS` budget that
    already exists. `dispatch_smart_scan` looks the report up and verifies that
    every category, privacy path and large-old path the request names was in
    what was actually issued. Nothing on disk, nothing shared between processes,
    and no new lifetime to reason about beyond the freshness window the
    dispatcher already enforces. **The happy path is unchanged and no
    functionality is lost:** a request whose report has been evicted gets the
    staleness refusal it would already have got, in the same words. The cap
    bounds the memory, and it is small because more than a few live reports
    means several windows mid-review, which is not a case worth holding
    megabytes for.
    Worth stating plainly: this makes the predicates the dispatcher checks today
    redundant rather than obsolete, and they should stay. They are cheap, and a
    lookup that silently missed would otherwise remove every check at once —
    which is the failure mode a single point of truth introduces.
  - [x] **The screen.** `SmartScanView.tsx`, and it is the module the app now
    opens on. Four decisions in it are worth keeping written down, because each
    one is a place the screen could have quietly widened what the engine
    offers.
    **Large & old files are offered here and never chosen for you** — their own
    collapsed section between the manifest and the findings, reading "Nothing
    chosen" until someone opens it, and a zero count until they tick something.
    *(Shipped finding-only first. The human asked for the drill-in and was
    right: leaving it out meant a third of the dispatcher was unreachable from
    the UI, and anyone who wanted those files in the sweep had to do the sweep
    twice.)*
    The two rejected alternatives are the design. Putting these rows in the
    manifest above would make a per-file decision inside a list whose every
    other row arrives pre-ticked — the context most likely to produce a careless
    tick, on the one source where the file is somebody's own document rather
    than a cache. Leaving them out entirely was the first attempt, and it cost
    more than it bought. A separate section, collapsed and empty by default,
    keeps "nothing here is ever chosen for you" literally true while still
    letting one gesture cover it. Each row carries what the decision needs —
    folder, name, size, and how long it has sat there — because that is what the
    Large & Old screen shows and it is the same decision.
    **The offer set had to become a fact in the report before any of that was
    safe.** `large_old.items` is the module's own answer and includes rows
    inside a browser's data, which the dispatcher refuses outright — so a screen
    ticking from that list could assemble a request that fails *as a whole*, for
    a row it had just shown. `SmartScanReportDto` carries `large_old_offerable`
    now, built by the identical predicate that decides the `found` contribution.
    Two tests pin it: the browser row is in the walk and not in the offer set,
    and the entire offer set handed to the dispatcher frees exactly the bytes it
    promised. Verified to bite — dropping the filter turns three tests red.
    **Only `smart_scan_default` categories are tickable.** The Trash is on the
    report, in "also found", with no checkbox and a line saying why.
    **The preview shell starts at the error state.** Every other module scans on
    first visit, so a browser reaches the truth immediately; this one waits for
    a button, and an idle "Ready to scan" hero outside the app would have looked
    like a working app right up until the button did nothing — the sample-data
    lie arriving one click later.
    **The "also found" figure is `found − selected − offerable`.** It was
    `found − selected` while Large & Old was a finding; now that those bytes
    have a section of their own, leaving them in would count them twice.
    **The sidebar badge tracks the live selection here**, unlike every other
    module, whose badge is its scan total. Those screens can only ever tick a
    subset, so the badge is never smaller than the ring. This one can add an
    18 GiB video to a 6.5 GiB sweep, and a badge showing the default would sit
    two inches from a ring showing four times as much.
    The offer-set property is asserted from the side that can violate it:
    `ux/backend-failure.spec.ts` renders a report whose walk holds a Firefox
    history file the offer set does not, opens the chooser, and pins that there
    is no control for that row at all — then records the dispatch request and
    checks that the Trash is absent from `categories`, `large_old_paths` is
    exactly the row that was offered, `expected` names three magnitudes with
    none inherited, and the request has the seven keys the backend accepts and
    no eighth. The backend refuses each of these independently; the point of the
    gate is that a UI change cannot start relying on that. Verified to bite by
    pointing the screen at `large_old.items` instead.
    Found by rendering rather than by reading: two Chrome profiles produced two
    rows reading *Google Chrome — GPU cache* word for word, distinguishable only
    by their sizes. The subtitle carries the profile now. And the ledger said
    "3 items" for a step that moved 412 files, because `executed` counts a
    verb's *actions* and a privacy row is one action over a folder.
  - [x] **The `ux-critic` round, and the four findings that were about the
    ring rather than about Smart Scan.** Scored ITERATE with nine must-fixes;
    all nine are resolved or recorded below, and four of them were pre-existing
    defects in shared components that only became visible once a second screen
    used them.
    **The scanning ring rendered `0 B` at 52px** for the whole of a scan — the
    reading of an empty disk, set at hero size, and reaching assistive tech as
    `aria-label="0 B so far"`. `ScanRing` now suppresses the figure entirely
    while sweeping with nothing counted. The Clean screen had the same bug
    whenever progress had not yet arrived.
    **A 36.4 MiB arc drew 1.00px wide** beside 6.4 GiB — measured by polar
    trace, against a 3px hard spec. The floor was 2 units *and* was applied
    after the inter-segment gap was subtracted, so the gap could push a segment
    under its own minimum. Both fixed.
    **The ledger's four outcome marks were text glyphs** measuring 6×6, 5×6,
    8×2 and 2×3 next to 36px icon tiles, and `✗` has no monospace form on this
    system so it fell back to an italic serif shape. Four drawn icons now.
    **Every row carried its own border**, which the rubric names as the generic
    dashboard tell; Space Lens, Privacy and Large & Old were already using
    `Group`. Smart Scan **and Cleanup** now do too — converting only the new
    screen would have shipped two treatments of the same category rows.
    Also: the confirmation sheet named sources without naming contents (a person
    could confirm 6.4 GiB without seeing that 4.1 of it was Xcode); the browser
    arc took Large & Old's pink, teaching a key the rest of the app
    contradicts — it takes the cache hue now, which is what Privacy already
    gives a cache row; and refusals reached the screen doubled
    (`refused: refused:`) because two honest layers each add the prefix.
  - [x] **The `deletion-safety-reviewer` round on the drill-in: PASS, seven
    findings, and the first one was a scope question rather than a bug.**
    **`~/Library/Application Support` is no longer offerable from Smart Scan.**
    `browser_root_for` only knows the browsers `privacy::BROWSERS` names, so
    everything *else* an app owns under that directory was offerable — probed
    against a fixture, both a `SomeVault/vault.db` and an Opera login database
    were listed and moved to the Trash, while Chrome's were correctly withheld.
    The reviewer called it the human's decision, and the human had already made
    it that morning: keep the directory readable, stop it being grantable, allow
    an override that states the risk and is attested to. The secure half of that
    lands here, narrowly — **this screen** stops offering the directory, while
    the Large & Old screen is untouched, because that is where a person arrives
    on purpose, chooses one file and reads its path. The override belongs with
    the attestation axis under M2 and is a change of its own. Verified to bite.
    **The confirmation sheet grew past the viewport and took its buttons with
    it.** Measured at 1200x800: at ~32 ticked files the title clipped off the
    top, at ~36 both Cancel and Move to Trash sat below the fold — on a modal
    with no Escape handler and no backdrop dismissal, so it could not be closed
    either. It failed *safe* (nobody confirms a button they cannot reach), which
    is exactly why it would have shipped unnoticed. The sheet scrolls now and a
    line names at most eight files before it starts counting; a test ticks forty
    and **clicks** the button rather than merely seeing it.
    **A load-bearing comment asserted the opposite of the code.** The module
    header still said "this screen never sends `large_old_paths`" 190 lines
    above the code that now does. `README`, `CHANGELOG` and this file had all
    been updated; the comment had not. In a codebase whose safety argument is
    carried in prose this dense, that is what makes the next reviewer skip the
    check.
    Plus three smaller ones, all fixed: the "also found" docstring still
    described the arithmetic it had before the section split; a scan whose every
    large-file match sat inside a browser said nothing about them at all; the
    collapsed header showed a capped list's floor as a total; and the tray effect
    blanked, on mount, a menu-bar label another screen had set — which mattered
    because this is the module the app opens on.
  - [ ] **Sheets have no Escape handler and no backdrop dismissal**, app-wide:
    `ConfirmSheet` here, `ConfirmModal` on Cleanup, and the Privacy and Large &
    Old sheets. Found while measuring the overflow above, and not caused by it.
    A modal that can only be left by a button is fine until the button is
    unreachable, which is the state that made this worth writing down.
  - [ ] **The gate cannot see below the fold.** `capture()` passes
    `fullPage: true`, but the shell puts content in an `overflow-y-auto` flex
    child — so "full page" is the viewport, and **every baseline in this repo
    stops at 800px**. Found by `ux-critic` on this screen, where it meant the
    whole "also found" band had no baseline at all. One scrolled capture was
    added for that band; the general fix — capturing the scroll container, or
    asserting on `scrollHeight` — is still open and affects every screen.
  - [ ] **No Stop during a scan**, which artboard 04 has. Deliberately not
    faked: the sidebar stays live so the window is not locked, and a button
    that returned the UI to idle while four full-disk walks kept running would
    be a claim this app does not get to make. Real cancellation is a backend
    change — `scanner::scan` has no cancellation token — so it is a task, not a
    polish item.
  - [ ] **Button heights are 30 / 36 / 40 against a 28 / 34 spec**, app-wide
    rather than here: `design/rubric.md` defines two sizes and no view uses
    them. Wants `.btn` / `.btn-lg` component classes and one pass over every
    view, which is why it is not folded into a module PR.
  - [ ] **Smaller `ux-critic` items, all recorded rather than done:** the two
    amber notices in the floor state say overlapping things in the same colour
    and shape and should merge; the size column should split value and unit
    into two tracks so `544.0 KiB` cannot look as long as `14.3 MiB`; at narrow
    widths the primary action sits above the manifest it describes; the Done
    state discards the ring instead of draining it, contradicting `ScanRing`'s
    own docstring about continuity; `homebrew-downloads` owns `--cat-browser`
    while Privacy uses that hue for *history*; the sheet overlay's
    `pl-[256px]` is a magic number that happens to equal the sidebar plus its
    padding; and artboard 05's "N protected items skipped" panel has no
    equivalent here because `SmartScanReportDto` carries no `skipped_protected`.
  - [ ] **A completed run leaves the other modules' figures stale.** Smart Scan
    clears its own badge and the menu-bar label afterwards, because the numbers
    no longer describe the disk — but a Cleanup screen visited earlier is kept
    mounted and still shows its pre-run report. Acting on it is *safe*: every
    verb re-scans inside the call and the drift check refuses a selection that
    no longer matches. It is still a stale number on screen, and the fix is a
    shell-level invalidation signal rather than anything in this screen, which
    is why it is not folded in here. **Pre-existing and wider than Smart Scan**
    — disposing from Large & Old already leaves the Cleanup badge behind — but
    a gesture that spans three modules is what makes it likely rather than
    theoretical.
  - **Not sources, and each for its own reason.** The **Uninstaller** takes a
    bundle id; including it means building the orphan sweep the M4 entry left
    as an open question, and it inverts the predicate that module exists to get
    right — from *prove this app is gone* to *enumerate everything and withhold
    what looks owned* — in the one module where over-reporting is catastrophic.
    **Startup** is a finding: `StartupSummary` has no bytes field, and a field
    that cannot exist cannot be summed into a total later. **Space Lens**
    contributes no bytes, and the first reason is overlap rather than units — it
    measures the same scope Large & Old does and does not measure the cleaner
    roots the default comes from at all, so it would double-count two sources
    and miss the one that matters.

## v0.6 — Distribution

The repository is public and the app is downloadable. What is left is the part
that makes a download pleasant rather than merely possible: it is unsigned and
un-notarized, so macOS treats it as suspect, and there is no auto-update.

- [x] **D1 — Universal binary** — CI ran a bare `cargo tauri build` with no
  `--target`, so it inherited the Apple Silicon runner: **every binary this
  project has published simply did not run on an Intel Mac.** Both jobs now name
  both targets. `package` builds `--target universal-apple-darwin`; the CLI is
  built twice and `lipo`'d, because there is no universal target for a plain
  `cargo build`.
  The predicted gotcha was real — `--target` moves the bundle two directories
  deeper, to `target/universal-apple-darwin/release/bundle/…`, so the upload
  glob, the release glob and the checksum step all had to move with it.
  `if-no-files-found: error` is what turns getting that wrong into a red job
  rather than a release with no `.dmg` on it.
  **Verified by building it, not by reading the config:** `lipo -info` reports
  both architectures in the shipped binary and in the `.app`'s, and
  `scripts/verify.sh --bundle` now builds the same universal target CI does —
  a local gate that bundles for this machine's architecture is not the local
  equivalent of a job that bundles for both. It skips, loudly, if either target
  is missing rather than quietly building one.
  Cost, stated: the bundle job roughly doubles, since it is two compiles.
- [ ] **The minimum macOS version is a default, not a decision.** Measured on the
  universal bundle: the Intel slice carries `LC_VERSION_MIN_MACOSX 10.13`, the
  Apple Silicon slice `minos 11.0` (which is the floor by construction — that
  hardware did not exist earlier), and `Info.plist` declares `10.13`. Those are
  coherent with each other, so nothing is *lying*; but nothing has been run on
  anything older than current macOS either, and `minimumSystemVersion` is unset
  in `tauri.conf.json`. Setting it would be a one-line change that **narrows**
  who can install — the opposite of what D1 just did — so it wants a report from
  a real old machine first rather than a guess. The README says which floor is
  declared and that it is untested, which is the honest state until then.
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
- [x] **D5 — Release hygiene** — *the three concrete items are done; the
  `package`-on-pull-requests question is settled below and is no longer a
  billing question.*
  **The version is single-sourced** (#60): `[workspace.package]` is the one
  place it is written, the three files outside the workspace are checked against
  it by `scripts/verify.sh`, and a mismatch fails the gate. *(The `[0.2.0]` link
  reference landed with the docs refresh, #33.)*
  **The release notes are the CHANGELOG section for the tag.**
  `scripts/release-notes.sh <tag>` extracts it and CI hands it to the release as
  `body_path`, so the release page and `CHANGELOG.md` cannot say different
  things about the same version. It **fails** on a missing section rather than
  producing empty notes, and `verify.sh` runs it against the current version —
  which moves "you forgot to write the notes" from after the tag was pushed to
  before it was.
  **Every asset ships a `.sha256` beside it**, written with a bare filename
  inside so `shasum -c swept.sha256` works in the folder you downloaded to.
  **A tag produces a *draft* release, published by hand.** The assets arrive
  from two jobs and `package` is much the slower, so publishing on first contact
  puts a page up saying "here is v0.4.0, download it" with the `.dmg` still
  compiling — on a project whose whole pitch is the download. Both jobs pass
  `draft: true`, because whichever reaches the release first is the one that
  creates it. `gh release edit <tag> --draft=false` is the last step, after the
  published checksums have been checked against the published files.
  **`package` stays off pull requests, and the reason has changed.** It used to
  be unaffordable — see the CI budget note below, now historical. It is free on
  a public repository, so the cost is *time*: after D1 that job compiles two
  architectures, which would add roughly a quarter of an hour to every pull
  request for a bundler that changes a few times a year.
  What that costs is worth stating rather than glossing: **`cargo tauri build`
  is the only thing that exercises release codegen and the bundler**, and the
  local Tauri gate is a *debug* `cargo build`, so nothing else runs it. A
  packaging regression surfaces when a release is cut rather than at the next
  merge. `scripts/verify.sh --bundle` is the local equivalent — it now builds
  the same universal target — and `workflow_dispatch` runs the real jobs on a
  branch. **Run one of them before tagging.**
  The version of this worth doing later is a *path-filtered* trigger: package on
  pull requests that touch `src-tauri/`, `tauri.conf.json` or the workflow, and
  nowhere else. Job-level path filters need an action (`dorny/paths-filter`),
  which is a dependency this repo does not have yet.

### CI budget — historical, kept for the reasoning

**D6 ended this: Actions minutes are free on public repositories, so nothing
below is a live constraint any more.** It is kept because the shape it produced
is still the right shape for other reasons — a superseded run is waste at any
price, prose cannot break a build, and a hung runner is still hung — and because
the next person to widen a trigger should know what was measured rather than
assumed. Where a rule was purely economic, D5 above says what replaced it.

The Actions allowance was exhausted. The cause is structural: this is a
macOS-targeted tool, GitHub bills macOS runners at **10x** on a private
repository, and every job here has to be macOS. Four jobs at ~3-4 minutes is
~140 billed minutes — about fourteen runs against a 2,000-minute month.

Neither macOS job can move to Linux without giving up what it tests. `check` is
the safety kernel's oracle and the kernel exists to reason about macOS path
semantics (case-insensitive volumes, `/Users` and `/var/folders` being symlinks,
what `realpath` does and does not normalise); a green Linux run would be testing
different rules. `gui` compares `*-darwin.png` visual baselines, and Chromium
renders text differently on Linux, so every snapshot would fail — the gate would
be measuring the runner.

What changed instead: prose and design assets are `paths-ignore`d (the docs PR
burned a full four-job macOS run for a README), superseded PR runs are
cancelled, and Playwright browsers and the Tauri CLI are cached. Roughly a third
off a code PR and all of it off a docs PR.

It was not enough — the allowance ran out again mid-session. **Measured, per
merged pull request:**

| run | jobs | runner min |
|---|---|---|
| pull request | check 0.7 + gui 3.3 | 4.0 |
| push to main | check 0.6 + gui 2.8 + release-build 0.6 + package 1.6 | 5.6 |

~9.6 macOS runner-minutes, which is ~96 billed and ~120 once per-job rounding is
counted: **about sixteen merges a month.** Two thirds of it is the push-to-main
run, and `check`/`gui` there re-validate the tree the pull-request run just
validated.

So `release-build` and `package` moved to a `v*` tag or a manual
`workflow_dispatch` — ~2.2 runner-minutes per merge for a `.dmg` nobody
downloads between releases. `scripts/verify.sh --bundle` is where that coverage
went; see D5 for what it costs.

- [ ] **The duplicate run is the remaining lever, and it is not obviously
  worth pulling.** Dropping `check` and `gui` from push-to-main saves ~4
  runner-minutes per merge — the largest single saving left — but it assumes the
  branch was up to date, so it wants "require branches to be up to date" in
  branch protection or a nightly catch-all. **Do not do this before D6 is
  decided**: if the repo goes public it buys nothing and costs a real signal.
  Levers after that, ordered by how much they cost in signal: cache more
  aggressively; run `gui` only when GUI-adjacent paths change; run the UX oracle
  on a schedule rather than per-PR. Making the repo public removes the
  constraint entirely (Actions are free for public repositories) — that is
  **D6's call to make, not CI's**, and it must not be made for billing reasons
  alone.
- [x] **D6 — Public flip.** Done on the human's word, after resolving what a
  public repository would have exposed. What was checked and found clean: no
  secrets anywhere in history, no personal paths or machine inventory in tracked
  files, `LICENSE` present and the README's MIT claim honest.
  What had to be fixed first, in order of severity:
  **The `v0.2.0` release's binaries predated every safety fix of the previous
  session** — the build that would trash a cookie jar with no acknowledgement,
  and that printed "Nothing to clean" over an unreadable Trash. Flipping public
  would have made it the top download button on a data-destroying tool. The
  assets are removed from both releases and the notes say exactly why; the tags
  and source are untouched, so the history stands and the builds can be
  recreated.
  **The README was two shipped modules out of date** — no Uninstaller at all,
  Privacy mentioned once in passing, and Startup still described as "read-only"
  after #50 gave it a verb. It also now says plainly what is *not* there:
  Smart Scan has an engine and no screen, there is no auto-update, no universal
  binary and no signed build.
  **`SECURITY.md`**, which matters more here than on most projects: someone who
  finds a way to make this remove the wrong files needs a private channel, not a
  public issue. It names the classes worth reporting — disposal outside the
  allowlist, a denylist bypass, acting without the consent given, a preview and
  an action that disagree, and a figure that is not true — and says which of
  those this project treats as the same family of defect. Plus
  `CONTRIBUTING.md`, and the sixteen merged branches D6 asked to tidy.
  Actions minutes stop being a constraint at this point: standard runners are
  free for public repositories, which retires the CI-budget note above and makes
  D5's "package on pull requests" affordable again.
