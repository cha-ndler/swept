---
description: Iterate on the visual polish of one GUI screen via the UX oracle, then pause for human taste sign-off.
argument-hint: <screen, e.g. "clean view" | "confirm modal" | "startup">
---

Prettify the GUI screen: **$ARGUMENTS** (or, if empty, the first unchecked item
in the "UI prettification" track of `ROADMAP.md`).

This is a **visual** task — build it, critique it, but do NOT auto-merge; pause
for the human taste gate.

1. Read `design/rubric.md` (esp. dimensions 9–10: Distinctiveness & delight,
   Depth & motion) and any images in `design/references/`. Branch
   `feat/prettify-<screen>` from origin/main.
2. Raise the screen beyond default Tailwind: deliberate type scale, intentional
   accent/state palette, category iconography, a size visualization with
   character, depth/layering, restrained motion (~150–200ms). Edit design tokens
   in `crates/gui/src/styles.css`. **Visual only** — never touch deletion/safety
   logic; deletion stays Trash-only + consent-gated.
3. Exercise the oracle: `cd crates/gui && npm run build && npm run ux:update`,
   then critique the PNGs in `crates/gui/ux/screenshots/` — invoke the `ux-critic`
   subagent, or use the Read tool to view them yourself and score against the
   rubric + references. Iterate edit → rebuild → re-screenshot → re-critique
   until no MUST-FIX and dimensions 9–10 are ≥ 4. Keep `npm run ux` (axe +
   visual-regression) green.
4. Open a PR with before/after screenshots + the ux-critic scorecard, then write
   `needs input:` for the human taste sign-off and STOP (do not merge).

See `.claude/loops/prettify-loop.md` for the self-continuing multi-screen version.
