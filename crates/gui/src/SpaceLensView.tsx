import { useEffect, useMemo, useState } from "react";
import { call, describeError } from "./backend";
import { ChevronIcon, Group, InfoIcon, LensIcon, ShieldIcon } from "./Shell";
import { formatBytes, formatBytesParts } from "./format";
import type { SpaceLensReport, SpaceNode } from "./types";

/**
 * Space Lens.
 *
 * Every other module in the app ends in a button that changes the disk. This
 * one has none, and that is the design, not an omission: there is no command
 * behind this screen that accepts a node back. A ring segment is a picture of
 * where the space went, and the only thing you can do with it is look closer.
 *
 * Two consequences run through the whole view:
 *
 * 1. **It says what it is.** A "Read-only view" mark sits in the toolbar, and
 *    the footer names the module you would use to actually act on something.
 *    A screen full of large, colourful, clickable areas otherwise reads as a
 *    control surface — and being clear about that is cheaper than a user
 *    discovering it by trying.
 * 2. **The picture cannot outrun the numbers.** The backend guarantees
 *    `bytes === sum(children)` at every level that has children, so a ring is
 *    always a faithful division of the circle inside it. Where it stops
 *    dividing — the depth cap, the width rollup — the node says `collapsed`,
 *    and the view says so too rather than drawing an empty directory.
 *
 * The sunburst is `aria-hidden`. The list beside it is not decoration or a
 * fallback: it carries the same navigation with real buttons, real focus and
 * real labels, so nothing here is reachable only by clicking a wedge.
 */

/** Rings drawn outside the hub. Deeper levels exist in the data; three is what
 *  a 360px circle can divide without the outer band becoming slivers. */
const RINGS = 3;

/** Ring geometry, in the SVG's own units. */
const HUB = 86;
const BANDS = [
  [HUB + 4, 124],
  [127, 156],
  [159, 184],
];
const VIEW = 380;
const CENTER = VIEW / 2;

/**
 * One hue per top-level child, inherited by everything beneath it, so a family
 * of folders reads as one region of the circle rather than a confetti of
 * unrelated wedges. These are the same category hues the rest of the app uses,
 * ordered so that adjacent families are the most distinguishable pairs.
 */
const FAMILY_NAMES = ["caches", "large", "build", "logs", "browser", "trashes"];

/** Rollup nodes ("N more items") are grey on purpose: they are not a place. */
const ROLLUP = "148 148 158";

/** Segments narrower than this are below a pixel at these radii. */
const MIN_ARC = 0.004;

type Arc = {
  node: SpaceNode;
  /** 1-based ring index. */
  depth: number;
  a0: number;
  a1: number;
  family: string;
  /** Index path from the node currently at the centre. */
  trail: number[];
};

export default function SpaceLensView({
  onTotal,
}: {
  onTotal?: (bytes: number | null) => void;
}) {
  const [report, setReport] = useState<SpaceLensReport | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  /** Index path from the synthetic root to whatever is at the centre. */
  const [trail, setTrail] = useState<number[]>([]);
  const [hovered, setHovered] = useState<string | null>(null);
  const [reloadKey, setReloadKey] = useState(0);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError("");
    // A fresh measurement describes a different tree, so the position we had
    // drilled to no longer means anything. Going back to the top is the honest
    // reset; keeping the indices would silently land on a different folder.
    setTrail([]);

    call<SpaceLensReport>("space_lens")
      .then((r) => {
        if (cancelled) return;
        setReport(r);
        onTotal?.(r.total_bytes);
      })
      .catch((e) => {
        if (cancelled) return;
        setError(describeError(e));
        setReport(null);
        onTotal?.(null);
      })
      .finally(() => !cancelled && setLoading(false));

    return () => {
      cancelled = true;
    };
  }, [reloadKey, onTotal]);

  // A synthetic node so the whole view has one shape to walk, including at the
  // top level where the "children" are the discovery roots.
  const root: SpaceNode | null = useMemo(
    () =>
      report && {
        name: "Your files",
        path: null,
        bytes: report.total_bytes,
        files: report.total_files,
        is_dir: true,
        collapsed: false,
        children: report.roots,
      },
    [report],
  );

  // Navigating leaves the pointer sitting over whatever was clicked, so without
  // this the row that was just opened stays "hovered" — and the highlight then
  // lands on an unrelated child of the folder we moved into.
  useEffect(() => setHovered(null), [trail]);

  const chain = useMemo(() => (root ? follow(root, trail) : []), [root, trail]);
  const current = chain.length > 0 ? chain[chain.length - 1] : null;
  const arcs = useMemo(() => (current ? layout(current) : []), [current]);

  if (error) {
    return (
      <Frame>
        <div
          className="flex items-start gap-3 rounded-card border border-danger/30 bg-danger/[.07] px-4 py-3"
          role="alert"
        >
          <span className="mt-0.5 flex-none text-danger">
            <InfoIcon size={16} />
          </span>
          <div>
            <p className="text-body font-medium">
              Couldn&rsquo;t measure your folders
            </p>
            <p className="text-muted mt-1 text-caption">
              {error} Nothing was scanned, and nothing was changed.
            </p>
          </div>
        </div>
      </Frame>
    );
  }

  if (loading || !report || !current) {
    return (
      <Frame>
        <Skeleton />
      </Frame>
    );
  }

  if (report.total_bytes === 0) {
    return (
      <Frame>
        <Group>
          <div className="flex flex-col items-center px-6 py-10 text-center">
            <span className="grid h-11 w-11 place-items-center rounded-card bg-accentTint text-accentText">
              <LensIcon size={20} />
            </span>
            <h2 className="mt-4 text-title font-semibold">Nothing to measure</h2>
            <p className="text-muted mx-auto mt-1.5 max-w-sm text-body">
              Your documents, downloads and media folders are empty — or this
              app could not read them.
            </p>
            <button
              onClick={() => setReloadKey((k) => k + 1)}
              className="mt-5 rounded-control border border-border bg-surface2 px-4 py-2 text-body font-medium text-text transition-colors duration-fast ease-mac"
            >
              Measure again
            </button>
          </div>
        </Group>
      </Frame>
    );
  }

  const children = current.children;

  return (
    <div className="flex h-full flex-col">
      <header className="flex h-[52px] flex-none items-center gap-2 border-b border-separator px-5">
        <button
          onClick={() => setTrail((t) => t.slice(0, -1))}
          disabled={trail.length === 0}
          aria-label="Go up one folder"
          className="grid h-[26px] w-[26px] flex-none place-items-center rounded-control text-muted transition-colors duration-fast ease-mac hover:bg-surface2 hover:text-text disabled:pointer-events-none disabled:text-subtle/40"
        >
          <ChevronIcon size={15} dir="left" />
        </button>

        <nav aria-label="Location" className="flex min-w-0 items-center gap-1">
          {chain.map((node, i) => (
            <span key={i} className="flex min-w-0 items-center gap-1">
              {i > 0 && (
                <span className="text-subtle flex-none" aria-hidden="true">
                  <ChevronIcon size={12} />
                </span>
              )}
              {i === chain.length - 1 ? (
                <span
                  aria-current="page"
                  className="truncate text-title font-semibold"
                >
                  {node.name}
                </span>
              ) : (
                <button
                  onClick={() => setTrail((t) => t.slice(0, i))}
                  className="truncate rounded-control px-1 text-title font-medium text-muted transition-colors duration-fast ease-mac hover:text-text"
                >
                  {node.name}
                </button>
              )}
            </span>
          ))}
        </nav>

        <div className="h-full flex-1" data-tauri-drag-region />

        <ReadOnlyMark />
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-6 pb-6 pt-5">
        <div className="flex flex-wrap items-start gap-8">
          <Sunburst
            arcs={arcs}
            current={current}
            hovered={hovered}
            onHover={setHovered}
            onOpen={(t) => setTrail((prev) => [...prev, ...t])}
          />

          <div className="min-w-[280px] flex-1">
            <h2 className="text-subtle mb-2 text-micro font-semibold uppercase">
              {trail.length === 0 ? "Largest locations" : "Largest inside"}
            </h2>

            {children.length === 0 ? (
              <Group>
                <p className="text-muted px-4 py-6 text-center text-body">
                  {current.collapsed
                    ? "There is more inside, but the measurement stops here."
                    : "This folder is empty."}
                </p>
              </Group>
            ) : (
              <Group>
                {children.map((child, i) => (
                  <ChildRow
                    key={`${i}-${child.name}`}
                    node={child}
                    parentBytes={current.bytes}
                    family={familyFor(child, i)}
                    first={i === 0}
                    active={hovered === String(i)}
                    onHover={(on) => setHovered(on ? String(i) : null)}
                    onOpen={() => setTrail((t) => [...t, i])}
                  />
                ))}
              </Group>
            )}

            <CoverageNotice report={report} />

            <div className="mt-3 flex gap-2.5 rounded-card border border-separator bg-surface px-3.5 py-3">
              <span className="text-subtle mt-px flex-none">
                <InfoIcon size={15} />
              </span>
              <p className="text-muted text-caption leading-relaxed">
                Space Lens only measures — nothing here can be selected or
                removed. To act on something you find, open{" "}
                <strong className="font-semibold text-text">Large &amp; Old</strong>
                , which asks for your confirmation file by file. Sizes are what
                each file <em>occupies</em> on disk, so they can differ slightly
                from the figures elsewhere in the app.
              </p>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

/** Toolbar + padded body, shared by the states that have no tree to draw. */
function Frame({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex h-full flex-col">
      <header className="flex h-[52px] flex-none items-center gap-2.5 border-b border-separator px-5">
        <h1 className="text-title font-semibold">Space Lens</h1>
        <div className="h-full flex-1" data-tauri-drag-region />
        {/* The same mark the loaded view carries. A claim that appears once the
            data arrives and is missing while it loads is a claim the user has
            to notice twice to believe. */}
        <ReadOnlyMark />
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto px-6 pb-6 pt-5">
        {children}
      </div>
    </div>
  );
}

/**
 * The claim this whole module rests on, kept on screen.
 *
 * Every other view in the app ends in a button that changes the disk. A user
 * has no way to know this one does not, and a screen full of large, colourful,
 * clickable areas reads as a control surface by default — saying so costs a
 * corner of the toolbar and is cheaper than someone finding out by trying.
 */
function ReadOnlyMark() {
  return (
    <span className="flex flex-none items-center gap-1.5 rounded-control border border-separator bg-surface px-2.5 py-1">
      <span className="text-success flex-none">
        <ShieldIcon size={13} />
      </span>
      <span className="text-muted text-caption font-medium">Read-only view</span>
    </span>
  );
}

// --- the circle ------------------------------------------------------------

function Sunburst({
  arcs,
  current,
  hovered,
  onHover,
  onOpen,
}: {
  arcs: Arc[];
  current: SpaceNode;
  hovered: string | null;
  onHover: (key: string | null) => void;
  onOpen: (trail: number[]) => void;
}) {
  const { value, unit } = formatBytesParts(current.bytes);

  return (
    <div className="relative h-[340px] w-[340px] max-w-full flex-none">
      {/*
        `aria-hidden`, deliberately. The list beside this carries the same
        navigation as real buttons with real labels, so hiding the wedges costs
        a screen-reader user nothing and spares them 60 unlabelled paths. The
        clicks here are a shortcut on top of that list, never the only way in.
      */}
      <svg
        viewBox={`0 0 ${VIEW} ${VIEW}`}
        className="h-full w-full"
        aria-hidden="true"
        focusable="false"
      >
        {arcs.map((arc) => {
          const key = String(arc.trail[0]);
          const [r0, r1] = BANDS[arc.depth - 1];
          const openable = arc.node.children.length > 0;
          return (
            <path
              key={arc.trail.join(".")}
              d={arcPath(r0, r1, arc.a0, arc.a1)}
              fill={`rgb(${arc.family})`}
              // Deeper rings are lighter, which is what makes the circle read
              // outward. Done with opacity over the dark canvas rather than a
              // second colour, so a family stays one hue at every depth.
              fillOpacity={1 - (arc.depth - 1) * 0.26}
              stroke="rgb(var(--bg-window))"
              strokeWidth={2}
              // Enough to push the other families back, not so much that the
              // chart reads as disabled. 0.3 was the first try and made two of
              // four wedges look switched off.
              opacity={hovered !== null && hovered !== key ? 0.55 : 1}
              className={`transition-opacity duration-fast ease-mac ${
                openable ? "cursor-pointer" : ""
              }`}
              onMouseEnter={() => onHover(key)}
              onMouseLeave={() => onHover(null)}
              onClick={() => openable && onOpen(arc.trail)}
            />
          );
        })}

        <circle
          cx={CENTER}
          cy={CENTER}
          r={HUB}
          fill="rgb(var(--surface-1))"
          stroke="var(--separator)"
        />
      </svg>

      {/* The hub's text lives outside the SVG so it uses the same type scale as
          everything else, rather than a second set of font sizes nobody
          maintains. Centred by the layout rather than by an offset computed
          from the viewBox — the circle is already centred in this box, so the
          two stay together at any size. */}
      <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center">
        <p className="font-mono text-display font-semibold tabular-nums">
          {value}
          <span className="text-muted ml-1 text-heroUnit font-medium">
            {unit}
          </span>
        </p>
        <p className="text-subtle mt-0.5 max-w-[150px] truncate text-caption">
          {current.name}
        </p>
      </div>
    </div>
  );
}

function ChildRow({
  node,
  parentBytes,
  family,
  first,
  active,
  onHover,
  onOpen,
}: {
  node: SpaceNode;
  parentBytes: number;
  family: string;
  first: boolean;
  /** True when this row is the one the pointer is linked to — via the row
   *  itself or via its wedge in the circle. */
  active: boolean;
  onHover: (on: boolean) => void;
  onOpen: () => void;
}) {
  const openable = node.children.length > 0;
  const share = parentBytes > 0 ? (node.bytes / parentBytes) * 100 : 0;

  const inner = (
    <>
      <span
        className="mt-[7px] h-[9px] w-[9px] flex-none rounded-full"
        style={{ background: `rgb(${family})` }}
        aria-hidden="true"
      />
      <span className="min-w-0 flex-1">
        <span className="block truncate text-body font-medium text-text">
          {node.name}
        </span>
        <span className="text-muted block truncate text-caption">
          {describe(node)}
        </span>
      </span>
      <span className="flex-none text-right">
        <span className="block font-mono text-body font-semibold tabular-nums text-text">
          {formatBytes(node.bytes)}
        </span>
        <span className="text-subtle block text-micro normal-case tracking-normal tabular-nums">
          {share >= 0.1 ? `${share.toFixed(0)}%` : "<1%"}
        </span>
      </span>
      {openable && (
        <span className="text-subtle flex-none" aria-hidden="true">
          <ChevronIcon size={14} />
        </span>
      )}
    </>
  );

  // The link between a wedge and its row is drawn by *lifting* the one that
  // matches, never by fading the others. Dimming was the first attempt and the
  // a11y gate rejected it on the spot: `opacity-45` over this surface takes the
  // row's own label from 12.09:1 to 4.06:1 and its subtitle to 2.71:1 — a
  // hover effect that quietly pushes half the screen below WCAG AA. The circle
  // still dims, because it is `aria-hidden` decoration carrying no text.
  const shell = `flex w-full items-start gap-3 px-4 py-2.5 text-left transition-colors duration-fast ease-mac ${
    first ? "" : "border-t border-separator"
  } ${active ? "bg-surface2" : ""}`;

  // A row you cannot open is not a button. Rendering it as one and then doing
  // nothing on click is the same small dishonesty as a disabled control with no
  // disabled styling.
  if (!openable) {
    return (
      <div
        className={shell}
        onMouseEnter={() => onHover(true)}
        onMouseLeave={() => onHover(false)}
      >
        {inner}
      </div>
    );
  }

  return (
    <button
      onClick={onOpen}
      onMouseEnter={() => onHover(true)}
      onMouseLeave={() => onHover(false)}
      className={`${shell} hover:bg-surface2`}
    >
      {inner}
    </button>
  );
}

/** The one-line subtitle: what is inside, or why there is nothing to open. */
function describe(node: SpaceNode): string {
  if (node.path === null && !node.is_dir && node.children.length === 0) {
    return "Smaller items, not listed separately";
  }
  if (!node.is_dir) return "File";
  if (node.children.length > 0) {
    const names = node.children.slice(0, 3).map((c) => c.name);
    const rest = node.children.length - names.length;
    return rest > 0 ? `${names.join(", ")} and ${rest} more` : names.join(", ");
  }
  return node.collapsed ? "More inside, not measured this deep" : "Empty";
}

/**
 * Say when the picture describes less than the disk does.
 *
 * Deliberately does *not* fire for de-duplicated hard links: counting one
 * file's blocks once makes the figure more accurate, not less complete, and a
 * caveat that cries wolf is worse than none.
 */
function CoverageNotice({ report }: { report: SpaceLensReport }) {
  if (!report.partial) return null;

  const reasons: string[] = [];
  if (report.truncated) {
    reasons.push(
      `the walk stopped after ${report.examined.toLocaleString()} items, so everything here is an under-count`,
    );
  }
  if (report.skipped_unreadable > 0) {
    reasons.push(
      `${report.skipped_unreadable.toLocaleString()} folder${
        report.skipped_unreadable === 1 ? " was" : "s were"
      } unreadable — granting Full Disk Access would include them`,
    );
  }
  if (report.skipped_too_deep > 0) {
    reasons.push(
      `${report.skipped_too_deep.toLocaleString()} folder${
        report.skipped_too_deep === 1 ? " is" : "s are"
      } nested too deeply to measure`,
    );
  }
  if (reasons.length === 0) return null;

  return (
    <div className="mt-3 flex items-start gap-3 rounded-card border border-cat-trashes/30 bg-cat-trashes/[.07] px-4 py-3">
      <span className="mt-0.5 flex-none text-cat-trashes">
        <InfoIcon size={16} />
      </span>
      <div className="min-w-0">
        <p className="text-body font-medium">This is a floor, not a total</p>
        <ul className="text-muted mt-1 flex list-disc flex-col gap-1 pl-4 text-caption">
          {reasons.map((r) => (
            <li key={r}>{r}</li>
          ))}
        </ul>
      </div>
    </div>
  );
}

function Skeleton() {
  return (
    <div className="flex flex-wrap items-start gap-8" aria-hidden="true">
      <div className="h-[340px] w-[340px] flex-none animate-pulse rounded-full bg-surface2" />
      <div className="min-w-[280px] flex-1">
        <div className="h-[11px] w-32 animate-pulse rounded bg-surface2" />
        <div className="mt-3 flex flex-col gap-px overflow-hidden rounded-card">
          {[0, 1, 2, 3, 4].map((i) => (
            <div key={i} className="h-[54px] animate-pulse bg-surface2" />
          ))}
        </div>
      </div>
    </div>
  );
}

// --- geometry --------------------------------------------------------------

/** The nodes from the synthetic root down to `trail`, inclusive of both. */
function follow(root: SpaceNode, trail: number[]): SpaceNode[] {
  const chain = [root];
  let node = root;
  for (const i of trail) {
    const next = node.children[i];
    // A trail that no longer resolves stops where it stopped. Cannot happen
    // today (the trail is reset with every report), and returning a shorter
    // chain rather than throwing keeps it that way if it ever can.
    if (!next) break;
    chain.push(next);
    node = next;
  }
  return chain;
}

function familyFor(node: SpaceNode, index: number): string {
  if (node.path === null && !node.is_dir) return ROLLUP;
  return `var(--cat-${familyName(index)})`;
}

function familyName(index: number): string {
  return FAMILY_NAMES[index % FAMILY_NAMES.length];
}

/**
 * Lay the current node's descendants out as concentric rings.
 *
 * Each ring divides its parent's own wedge, so the picture inherits the
 * backend's guarantee that a node's bytes are exactly its children's — no ring
 * can claim more of the circle than the one inside it gave it.
 */
export function layout(current: SpaceNode): Arc[] {
  const arcs: Arc[] = [];

  function ring(
    parent: SpaceNode,
    from: number,
    to: number,
    depth: number,
    family: string | null,
    trail: number[],
  ) {
    if (depth > RINGS || parent.bytes <= 0) return;
    let a = from;
    parent.children.forEach((child, i) => {
      const span = (child.bytes / parent.bytes) * (to - from);
      // Advance regardless of whether this one is drawn: skipping the cursor
      // as well would slide every later sibling out of position, which is the
      // kind of error that looks like a rounding artifact and is not.
      const a0 = a;
      a += span;
      if (span < MIN_ARC) return;
      const own = family ?? `var(--cat-${familyName(i)})`;
      const mine =
        child.path === null && !child.is_dir && depth === 1 ? ROLLUP : own;
      const next = [...trail, i];
      arcs.push({ node: child, depth, a0, a1: a0 + span, family: mine, trail: next });
      ring(child, a0, a0 + span, depth + 1, mine, next);
    });
  }

  // A single child would span the whole circle, where an SVG arc from a point
  // back to itself draws nothing at all. Stopping a thousandth of a radian
  // short costs a hairline seam at twelve o'clock and keeps the ring visible.
  const FULL = Math.PI * 2 - 0.001;
  ring(current, -Math.PI / 2, -Math.PI / 2 + FULL, 1, null, []);
  return arcs;
}

function arcPath(r0: number, r1: number, a0: number, a1: number): string {
  const large = a1 - a0 > Math.PI ? 1 : 0;
  const at = (r: number, a: number) =>
    `${(CENTER + r * Math.cos(a)).toFixed(2)} ${(CENTER + r * Math.sin(a)).toFixed(2)}`;
  return [
    `M${at(r1, a0)}`,
    `A${r1} ${r1} 0 ${large} 1 ${at(r1, a1)}`,
    `L${at(r0, a1)}`,
    `A${r0} ${r0} 0 ${large} 0 ${at(r0, a0)}`,
    "Z",
  ].join("");
}
