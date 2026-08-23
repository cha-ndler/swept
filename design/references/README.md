# UX references

These are the exemplars the `ux-critic` subagent scores screenshots against,
alongside `design/rubric.md`.

## First-party artboards (generated — do not hand-edit)

`artboard-*.png` are rendered from `design/canvas/index.html`, which is the
design target for v0.4 and the single source of truth for the token system.
Edit the canvas, then regenerate:

```bash
node design/canvas/render.mjs
```

| Artboard | What it fixes |
|---|---|
| `01-foundations` | The token system: measured palette, type scale, 4pt grid, three elevations, two motion durations |
| `02-shell` | Vibrancy sidebar with inset traffic lights, replacing the two-tab pill row |
| `03-smart-scan-idle` | The safety promise stated before anything runs |
| `04-smart-scan-scanning` | Live counts with no invented percentage |
| `05-smart-scan-results` | Hero total, category ring, one proportional stack, opt-in modules kept separate |
| `06-confirm-sheet` | The consent moment — Trash vs. permanent, visually weighted |
| `07-module-large-old` | Per-file consent for paths outside the disposal allowlist |
| `08-space-lens` | Read-only sunburst; no checkbox anywhere on the screen |
| `09-onboarding-fda` | Full Disk Access asked for honestly, with a real decline path |
| `10-states` | Empty, error and complete — each with an exit |

Because we generate them, they can be committed, versioned, diffed, and shipped
without a copyright question. That is the whole reason this folder exists in
this form.

## Third-party screenshots

Comparable tools (CleanMyMac, DaisyDisk, OnyX) and polish exemplars (Raycast,
Linear) are useful for critique but are **third-party copyrighted UI**. If you
want them here:

- **Add them manually.** Do not auto-download them, and do not ask the agent to.
- Name them `cleanmymac-*.png`, `daisydisk-*.png`, and so on, so they are
  distinguishable from the generated `artboard-*.png` files.
- Keep them **out of any shipped artifact** — local critique only.

The first-party artboards above are what make the critique concrete by default,
so none of this is required.
