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
  `sudo macclean clean --execute --permanent --yes` is reachable. The exposure
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
- [ ] **M4 — Uninstaller (leftovers-only)** — *discovery is done — id-keyed
  locations, containers and the human-name tier; disposal and the UI are open.*
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
  - [ ] **Directory-aware disposal** — *the executor half is done; the
    discovery flags and the command layer are open.*
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
  - [ ] **The module UI.** *Visual → taste gate.*
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
  release notes; publish checksums. *(The `[0.2.0]` link reference landed with
  the docs refresh, #33.)*
  **Reversed:** this item used to call for running the `package` job on pull
  requests. That is now the opposite of what the repo can afford — see the CI
  budget note below — so bundling stays validated locally for PRs and in CI only
  on main and tags.

### CI budget — a real constraint, not a preference

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
cancelled, `release-build` and `package` are main-and-tags only, and Playwright
browsers and the Tauri CLI are cached. Roughly a third off a code PR and all of
it off a docs PR.

- [ ] **Watch whether that is enough.** If it is not, the remaining levers are
  ordered by how much they cost in signal: cache more aggressively; drop the
  `gui` job to run only when GUI-adjacent paths change; run the UX oracle on a
  schedule rather than per-PR. Making the repo public removes the constraint
  entirely (Actions are free for public repositories) — that is **D6's call to
  make, not CI's**, and it must not be done for billing reasons alone.
- [ ] **D6 — Public flip** — *(separate, on the human's word.)* Tidy the stale
  merged remote branches, then make the repo public.
