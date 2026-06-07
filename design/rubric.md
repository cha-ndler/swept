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

## How "pleasant" is verified

- Objective gates (CI): axe-core a11y (no critical/serious violations) and
  visual-regression snapshots (intentional changes only).
- Iterative critique (loop): the `ux-critic` scores screenshots vs this rubric +
  `design/references/`.
- Final taste gate (human): subjective delight / native-macOS feel.
