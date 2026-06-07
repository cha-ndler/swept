# mac-cleaner UX rubric

The `ux-critic` subagent scores GUI screenshots against these dimensions (1–5).
A view is ready for the human taste gate when there are no MUST-FIX findings and
every dimension is ≥ 4.

## Dimensions

1. **Visual hierarchy** — the most important thing (what will be cleaned, how
   much space is reclaimable, the primary action) is the most prominent.
2. **Spacing & rhythm** — consistent padding/gaps; nothing cramped or adrift; a
   clear grid.
3. **Contrast & legibility** — text meets WCAG AA against its background; numbers
   (sizes/counts) are easy to scan.
4. **Affordance clarity** — buttons look pressable; selection state is obvious;
   destructive vs safe actions are visually distinct (danger color reserved for
   destructive).
5. **Trust for a destructive tool** — it is always obvious that scanning is a
   preview, exactly what would be removed, and where consent/confirmation lives.
   Irreversible (permanent) actions are visually heavier than Trash.
6. **States** — empty, loading, results, confirmation, and error states all
   exist and look intentional (no blank flashes, no dead-ends).
7. **Consistency** — components, colors, and copy are consistent within and
   across views; uses the design tokens (no one-off colors).
8. **Restraint** — no gratuitous motion or decoration; polish through clarity,
   not flourish (per the project brief: pleasant, not as extravagant as
   CleanMyMac).
9. **Distinctiveness & delight** — does NOT look like a default Tailwind/bootstrap
   page. Has a point of view: a considered type scale, an accent used with
   intent, category iconography, and a size visualization with more character
   than a flat bar. A first-time user should feel "this is a crafted Mac app,"
   not "this is a form." Score 5 only if it would not be mistaken for a generic
   admin dashboard.
10. **Depth & motion** — deliberate layering (hairline borders, subtle shadows,
   surface elevation) and *restrained* motion (≈150–200ms ease on selection,
   hover, and the confirm modal). No jank, no gratuitous animation; absence of
   any depth/motion is itself a finding for a v0.3 prettification pass.

## The prettification bar (v0.2 → v0.3)

v0.2 cleared dimensions 1–8 (correct, accessible, trustworthy) but reads as
"standard." For prettification, dimensions **9 and 10 are the focus**: push each
screen until it has genuine craft and character while staying macOS-native and
restrained. "Correct" is the floor, not the goal.

## How "pleasant" is verified

- Objective gates (CI): axe-core a11y (no critical/serious violations) and
  visual-regression snapshots (intentional changes only).
- Iterative critique (loop): the `ux-critic` scores screenshots vs this rubric +
  `design/references/`. **Add competitor/exemplar screenshots to
  `design/references/`** (CleanMyMac, DaisyDisk, Raycast, Linear) to make the
  critique concrete — without them the critic falls back to the rubric alone,
  which is how "standard" output slips through.
- Final taste gate (human): subjective delight / native-macOS feel.

- Objective gates (CI): axe-core a11y (no critical/serious violations) and
  visual-regression snapshots (intentional changes only).
- Iterative critique (loop): the `ux-critic` scores screenshots vs this rubric +
  `design/references/`.
- Final taste gate (human): subjective delight / native-macOS feel.
