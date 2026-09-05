# UI prettification loop — paste into a fresh Claude Code session

The GUI currently looks correct but generic. This loop iterates on *visual
distinctiveness and delight* using the UX oracle (render → screenshot → vision
critique vs the rubric + competitor references → iterate), then pauses for the
human taste gate. Paste the block below into a new session.

Tip: drop competitor/exemplar screenshots into `design/references/` first
(CleanMyMac, DaisyDisk, Raycast, Linear). With references present the critique is
far sharper; without them it falls back to the rubric + macOS conventions.

---

Swept UI PRETTIFICATION LOOP — one screen per iteration, then re-arm.

Working dir: repo root. Read CLAUDE.md and design/rubric.md first.

1. `git fetch origin -q --prune`; `git checkout -B main origin/main -q`. Read the "UI prettification" track in ROADMAP.md; pick the first unchecked screen (Clean view, Confirm modal, Startup view, Done/empty states, app-wide theming). If none, go to step 7.
2. `git checkout -b feat/prettify-<screen> origin/main`. Confirm the branch before committing.
3. RAISE THE BAR beyond default Tailwind. Concrete levers (use design tokens in `crates/gui/src/styles.css`; add tokens as needed): a deliberate type scale; intentional spacing rhythm; depth (subtle shadows, layering, a hairline border system); a refined accent + state palette (hover/active/selected); iconography for categories; a size visualization with more character than a flat bar (e.g., proportional/stacked); polished empty/loading/done states; restrained motion (150–200ms ease) for selection and the confirm modal. Keep it macOS-native in feel and never gaudy (project brief: pleasant, not as extravagant as CleanMyMac). Deletion stays Trash-only and consent-gated — visual only.
4. EXERCISE THE ORACLE: `cd crates/gui && npm run build && npm run ux:update` (re-render + baselines). Then CRITIQUE the PNGs in `crates/gui/ux/screenshots/`: invoke the `ux-critic` subagent (or, if it's not in the registry, use the Read tool to VIEW each screenshot yourself and score against `design/rubric.md` + `design/references/*`). Iterate edit → rebuild → re-screenshot → re-critique until the critic reports no MUST-FIX and every rubric dimension — including Distinctiveness & delight — is >= 4. Keep axe + visual-regression green (`npm run ux`).
5. `cargo fmt --all` (no Rust change expected); frontend `npm run build`. No deletion-safety review needed for visual-only diffs (say so); deletion logic must be untouched.
6. Check the screen off in ROADMAP.md. Commit (noreply email). Push and VERIFY. Open a PR with the before/after screenshots + the ux-critic scorecard + reference comparison. Then write `needs input:` for the human taste sign-off and STOP — do NOT auto-merge a visual change, do NOT reschedule. (After the human approves & merges, paste this prompt again for the next screen.)
7. FINISH (all screens approved): refresh README screenshots, post a summary, do NOT reschedule.

Constraints: visual-only — never change deletion/safety logic; keep the no-real-deletes hook, no force-push/reset. State results in your own text. Pause (`needs input:`) on every visual PR for the human's taste call.

Begin iteration now.
