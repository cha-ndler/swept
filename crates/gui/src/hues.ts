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
 * `--cat-caches`, which is what the Privacy screen already gives a `cache` row
 * (`CLASS_HUES` in `PrivacyView.tsx`) — and every row Smart Scan offers from
 * that source is a cache. So the two screens say the same thing about the same
 * bytes.
 *
 * It was `--cat-large` first, chosen to avoid two same-coloured arcs in one
 * ring. That was the wrong trade: pink is Large & Old's identity throughout its
 * own screen — its dots, its bars, its sheet — so the ring taught a key that is
 * contradicted everywhere else in the app. Sharing the cache hue with
 * `user-caches` is not a collision but a statement, and a true one: on this
 * screen a coloured dot means "this row is an arc above", and two rows that are
 * both caches earning the same colour says exactly what it looks like.
 */
export const PRIVACY_HUE = "rgb(var(--cat-caches))";

/**
 * The hue Smart Scan gives the large-files arc, and Large & Old's own colour
 * everywhere else — its dots, its bars, its confirmation sheet. It is free for
 * this ring precisely because the browser arc stopped borrowing it.
 */
export const LARGE_HUE = "rgb(var(--cat-large))";
