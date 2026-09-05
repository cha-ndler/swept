# Swept UX rubric

The `ux-critic` subagent scores GUI screenshots against these dimensions (1–5).
A view is ready for the human taste gate when there are no MUST-FIX findings and
every dimension is ≥ 4.

## The target

**A native Mac pro-tool.** The register is DaisyDisk / Raycast / Linear: depth
from hairlines, vibrancy and surface elevation rather than gradients and
illustration; numbers as the hero; one accent used with intent; motion you
notice only if you look for it.

This supersedes the earlier "pleasant, not as extravagant as CleanMyMac"
phrasing, which said what to avoid without saying what to aim for — and
"standard" is exactly what you get when the target is defined by negation.

The reference implementation is `design/canvas/index.html`, exported to
`design/references/artboard-*.png`. **Those artboards are the exemplars the
critic compares against.** They are first-party: we generate them, so they can
be shipped, versioned, and diffed, and there is no third-party copyright
question. Regenerate after editing the canvas:

```bash
node design/canvas/render.mjs
```

---

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
8. **Restraint** — polish through clarity, not flourish. No decoration that
   carries no information, no motion that isn't feedback. A gradient, a shadow
   or an animation each needs a reason; "it looks nicer" is not one.
9. **Distinctiveness & craft** — does NOT look like a default Tailwind page.
   Has a point of view: a considered type scale, an accent used with intent,
   category iconography, and a size visualization with more character than a
   flat bar. A first-time user should feel "this is a crafted Mac app," not
   "this is a form." Score 5 only if it would not be mistaken for a generic
   admin dashboard.
10. **Depth & motion** — deliberate layering (hairline borders, subtle shadows,
    surface elevation) and *restrained* motion (150ms on selection and hover,
    220ms on the confirm sheet). No jank, no gratuitous animation; absence of
    any depth or motion is itself a finding.

---

## Hard specs

These are the checkable half. A finding here is objective, so the critic should
cite the specific violation rather than a feeling. All values come from the
token block at the top of `design/canvas/index.html`, which is the single source
of truth that `crates/gui/src/styles.css` implements.

### Colour

| Role | Token | Value | Rule |
|---|---|---|---|
| Content canvas | `--bg-window` | `#17171A` | |
| Sidebar | `--sidebar` | `#232328` | lighter than content, per macOS |
| Card at rest | `--surface-1` | `#1F1F23` | |
| Hover / popover | `--surface-2` | `#26262B` | |
| Sheet / modal | `--surface-3` | `#2E2E34` | nothing sits higher |
| Primary text | `--text` | `#F2F2F7` | ≥ 12.09:1 on every surface |
| Secondary text | `--text-2` | `#B4B4BD` | ≥ 6.56:1 |
| Tertiary text | `--text-3` | `#9A9AA3` | ≥ 4.83:1 — still AA |
| Accent **fill** | `--accent-fill` | `#0A66CC` | the *only* blue that may carry a white label (5.55:1) |
| Accent **graphic** | `--accent-graphic` | `#0A84FF` | rings, bars, tints — **never text**, only 3.65:1 on white |
| Accent **text** | `--accent-text` | `#409CFF` | accent-coloured text (≥ 4.77:1) |
| Danger fill | `--danger-fill` | `#C9241B` | white label 5.60:1 |
| Danger text | `--danger-text` | `#FF6961` | ≥ 4.78:1 |

Category hues, stable across every view: caches `#0A84FF`, build `#30D158`,
logs `#BF5AF2`, trashes `#FF9F0A`, large & old `#FF6482`, browser `#64D2FF`.

**Splitting the accent into three roles is the point.** The vivid system blue is
the natural choice for a button and it fails AA the moment a white label lands
on it. Any new blue must declare which of the three roles it is.

- Body text: **≥ 4.5:1**. No large-text exemption applies — nothing in this app
  is 18px+ except the hero number, which is on the canvas at 16:1.
- Non-text graphics (bars, ring segments, dots, focus rings): **≥ 3.0:1**.
- Verify with `axe` in the Playwright run, not by eye.

### Type

| Token | Size / line / tracking / weight | Used for |
|---|---|---|
| `--fs-hero` | 52 / 52 / -.025em / 600 | the reclaimable total, and nothing else |
| `--fs-hero-unit` | 24 / 28 / 0 / 500 | the unit beside it (`GiB`), never alone |
| `--fs-display` | 28 / 32 / -.02em / 600 | screen title in a centred state |
| `--fs-title` | 17 / 22 / -.01em / 600 | toolbar title, sheet heading |
| `--fs-emph` | 15 / 20 / 0 / 500 | primary button, lead paragraph |
| `--fs-body` | 13 / 18 / 0 / 400–500 | the macOS default control size |
| `--fs-caption` | 12 / 16 / 0 / 400 | row metadata |
| `--fs-micro` | 11 / 14 / .07em / 600 uppercase | section labels |

**Every figure a user might compare — sizes, counts, percentages, dates — is set
in `--mono` with `tabular-nums`.** Digits then align in a column and a live
counter doesn't reflow as it ticks. A size in the body font is a MUST-FIX.

### Spacing, radii, elevation, motion

- **4pt grid.** 4 / 8 / 12 / 16 / 20 / 24 / 32 / 40 / 48. A value off the grid
  is a finding.
- **Radii:** 6 control, 10 card, 14 panel/sheet, 999 pill.
- **Elevation — exactly three levels.** E1 card: hairline, no shadow. E2 hover
  and popover: `0 1px 2px rgba(0,0,0,.30), 0 4px 12px rgba(0,0,0,.22)`. E3 sheet:
  `0 2px 8px rgba(0,0,0,.35), 0 16px 48px rgba(0,0,0,.50)`. A fourth invented
  shadow is a finding.
- **Motion — two durations, one curve.** 150ms for hover / selection / checkbox /
  segment; 220ms for sheet entry (opacity + scale .96→1); curve
  `cubic-bezier(.32,.72,0,1)` throughout. All of it disabled under
  `prefers-reduced-motion`, including the scan ring.

### Controls

Stock browser controls are the single loudest "this is a web page" tell, and
v0.2 shipped two of them.

- `<select>` → **segmented control** (`.seg`) for ≤ 4 options; a styled popover
  above that.
- `<input type=checkbox>` → **14px rounded square**, `--accent-fill` when on,
  with a drawn checkmark and a distinct mixed state.
- Buttons: 28px standard, 34px `.lg`. Primary is `--accent-fill`; secondary is
  `--surface-2` + hairline; tertiary is transparent until hover.
- Rows live in an **inset grouped list** — one card, hairline-separated rows —
  not one bordered card per row.

### Iconography

One icon per module and per category. 16–17px box, 1.5 stroke, round caps and
joins, `currentColor`. Inline SVG `<symbol>` sprite; no icon font, no emoji in
place of an icon. Zero iconography is a MUST-FIX on dimension 9.

### Data visualisation

- Sizes that sum to a whole are drawn as **one proportional stack or ring**, not
  as N independent bars — independent bars answer "how big relative to the
  largest", which is the question nobody asked.
- **Every segment has a 3px minimum width.** v0.2 rendered Logs at 0.08% of the
  max as an empty track, which reads as zero. Rendering a real quantity as
  nothing is a truthfulness bug, not a cosmetic one.
- No 3-D, no drop shadows on chart geometry, no more than one chart per screen.

---

## Automatic MUST-FIX

Findings that need no judgement. Any one of these blocks the taste gate:

1. A figure describing the user's disk that is not `tabular-nums`.
2. A stock `<select>` or `<input type=checkbox>` visible in the screenshot.
3. A non-zero quantity rendered as a zero-width bar segment.
4. Any axe violation at serious or critical impact.
5. A colour not traceable to a token.
6. A destructive action styled identically to a safe one.
7. A state with no exit — the Done screen with no way back was one.
8. Motion that survives `prefers-reduced-motion`.
9. Sample or placeholder figures shown where real data failed to load. This one
   is a safety defect, not a design defect: the fixture category ids are the
   real ones, so a fabricated number is directly actionable by the user.

---

## How "pleasant" is verified

Three gates, in order of how cheaply they run:

1. **Objective (CI)** — `cd crates/gui && npm run ux`. axe-core allows no
   serious/critical violations, and visual-regression snapshots catch
   unintended change.

   Note the snapshot gate is only as sensitive as its threshold. It ran with
   pixelmatch's default `threshold: 0.2` until v0.3, which against this palette
   (`#2a2a30` on `#1e1e22`) was loose enough that **replacing an entire
   component passed** — 14.5% of pixels differed and the gate saw nothing. It is
   now `threshold: 0.05` / `maxDiffPixelRatio: 0.01`. If a gate ever passes a
   change you know is large, distrust the gate first.

2. **Iterative critique (loop)** — `ux-critic` scores the PNGs in
   `ux/screenshots/` against this rubric *and* the artboards in
   `design/references/`. With exemplars present the critique is concrete
   ("the row group is per-row bordered; artboard 05 uses one grouped card");
   without them the critic falls back to the rubric alone, which is how
   "standard" output slipped through v0.2.

3. **Human taste gate** — subjective delight and native-macOS feel. Anything a
   user sees pauses here and opens as a PR with screenshots. Never auto-merged.

## Scope note

The canvas is the design *target*, not a claim about the current build. When a
screen ships, its screenshot should be comparable to the corresponding artboard;
until then the gap between them is the backlog.
