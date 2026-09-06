/**
 * One hue per cleaner, stable across every view (design/rubric.md § Hard specs).
 *
 * This lives in its own module rather than beside the first screen that needed
 * it because "stable across every view" is a property of *one* table. Smart Scan
 * draws the same categories in the same ring as Cleanup, and a second copy of
 * this map would keep them in agreement only for as long as nobody edited one of
 * them — the same argument that put `Category::smart_scan_default` in the
 * registry instead of in the aggregator.
 *
 * Ids come from `swept_core::categories`.
 */
const CATEGORY_HUE: Record<string, string> = {
  "user-caches": "rgb(var(--cat-caches))",
  "xcode-derived-data": "rgb(var(--cat-build))",
  "user-logs": "rgb(var(--cat-logs))",
  trash: "rgb(var(--cat-trashes))",
  "homebrew-downloads": "rgb(var(--cat-browser))",
};

/**
 * The colour for a category id.
 *
 * An unknown id deliberately falls back to grey rather than borrowing another
 * category's colour — a wrong hue would claim a relationship that isn't there.
 */
export function hue(id: string): string {
  return CATEGORY_HUE[id] ?? "var(--text-3)";
}

/**
 * The hue Smart Scan gives the browser-data arc.
 *
 * Pink is Large & Old's colour elsewhere, and it is free *here* because Large &
 * Old contributes no arc to this ring — it is a finding on this screen, not a
 * selection. The alternative was `--cat-browser`, which `homebrew-downloads`
 * already owns, and two identically coloured arcs in one ring meaning two
 * different things is worse than one hue meaning different things on two
 * screens. The rule the dot obeys is local and absolute: on this screen, a
 * coloured dot means "this row is an arc in the ring above", and nothing else
 * carries one.
 */
export const PRIVACY_HUE = "rgb(var(--cat-large))";
