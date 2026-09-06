import { formatBytesParts } from "./format";

const SIZE = 220;
const R = 92;
const STROKE = 18;
const C = 2 * Math.PI * R;
/** Gap between segments, in path units. Keeps adjacent hues from touching. */
const GAP = 3;
/**
 * The shortest arc a real quantity may be drawn as (design/rubric.md, hard
 * specs). Path units are pixels here — the `viewBox` is 1:1 with `SIZE`.
 *
 * The floor used to be 2, and it was applied *after* subtracting `GAP`, so a
 * segment the gap had already eaten could not climb back out of it. Measured on
 * the Smart Scan ring: 36.4 MiB of browser data next to 6.4 GiB of caches came
 * out **1.00px wide** at full saturation — indistinguishable from a seam
 * between two other arcs, on the one part of that selection that touches
 * browser data.
 */
const MIN_ARC = 3;

export type RingSegment = { id: string; bytes: number; color: string };

/**
 * The reclaimable total, drawn as one ring broken into its categories.
 *
 * One shape carries both the sum and the composition, which is the whole point
 * — the figure in the middle and the arcs around it are the same number, so
 * they cannot disagree. It is the same element in all three states (at rest,
 * sweeping during a scan, filled with the result), so the states read as one
 * continuous object rather than three unrelated cards.
 */
export function ScanRing({
  segments,
  total,
  caption,
  busy = false,
}: {
  segments: RingSegment[];
  total: number;
  caption: string;
  busy?: boolean;
}) {
  const { value, unit } = formatBytesParts(total);
  /** Sweeping, with nothing counted yet. There is no honest figure. */
  const blank = busy && total === 0;

  // Fractions are of the total the ring is showing, so the arcs always close.
  let acc = 0;
  const arcs = segments
    .filter((s) => s.bytes > 0 && total > 0)
    .map((s) => {
      const frac = s.bytes / total;
      const offset = -acc * C;
      acc += frac;
      return {
        id: s.id,
        color: s.color,
        // Never let a real quantity vanish: a category at a fraction of a
        // percent still gets a visible arc (design/rubric.md MUST-FIX #3).
        // The floor is applied to the *drawn* length, so the inter-segment gap
        // cannot push a segment under it.
        len: Math.max(frac * C - GAP, MIN_ARC),
        offset,
      };
    });

  return (
    <div
      className="relative"
      style={{ width: SIZE, height: SIZE }}
      role="img"
      aria-label={blank ? "Scanning" : `${value} ${unit} ${caption}`}
    >
      <svg
        width={SIZE}
        height={SIZE}
        viewBox={`0 0 ${SIZE} ${SIZE}`}
        aria-hidden="true"
      >
        <circle
          cx={SIZE / 2}
          cy={SIZE / 2}
          r={R}
          fill="none"
          strokeWidth={STROKE}
          className="stroke-white/[.055]"
        />
        {busy ? (
          <g
            className="ring-spin"
            style={{ transformOrigin: `${SIZE / 2}px ${SIZE / 2}px` }}
          >
            <circle
              cx={SIZE / 2}
              cy={SIZE / 2}
              r={R}
              fill="none"
              strokeWidth={STROKE}
              strokeLinecap="round"
              stroke="rgb(var(--accent-graphic))"
              strokeDasharray={`${C * 0.22} ${C}`}
            />
          </g>
        ) : (
          <g transform={`rotate(-90 ${SIZE / 2} ${SIZE / 2})`}>
            {arcs.map((a) => (
              <circle
                key={a.id}
                cx={SIZE / 2}
                cy={SIZE / 2}
                r={R}
                fill="none"
                strokeWidth={STROKE}
                stroke={a.color}
                strokeDasharray={`${a.len} ${C}`}
                strokeDashoffset={a.offset}
              />
            ))}
          </g>
        )}
      </svg>

      <div className="absolute inset-0 grid place-content-center text-center">
        {/* A sweeping ring with nothing counted yet has no figure, and `0 B`
            is not one — it is the reading of an empty disk, set at 52px, for
            the whole of a scan that has not finished looking. Suppressed
            rather than faked, and the `aria-label` above says the same. */}
        {!blank && (
          <p className="font-mono text-hero font-semibold tabular-nums">
            {value}
            <span className="text-muted ml-1 text-heroUnit font-medium tracking-normal">
              {unit}
            </span>
          </p>
        )}
        <p className={`text-muted text-caption ${blank ? "" : "mt-2"}`}>
          {blank ? "Looking" : caption}
        </p>
      </div>
    </div>
  );
}
