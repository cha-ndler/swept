import { formatBytes } from "./format";
import type { CategorySummary } from "./types";
import { Checkbox } from "./Controls";
import { hue } from "./hues";

/**
 * One cleaner category, as a row in a grouped list.
 *
 * Shared because two screens show the same categories, and a second copy of
 * this markup would be a second answer to "what does a category look like" —
 * the same argument that put `hue()` in `hues.ts` rather than beside the first
 * screen that needed it. It was two copies for exactly one commit.
 *
 * The row is a `<label>` with a real checkbox inside, so the whole row is a hit
 * target without reimplementing any of what a checkbox does for keyboard and
 * assistive technology.
 */
export function CategoryRow({
  cat,
  checked,
  onToggle,
  /** "Select" on Cleanup, "Include" on Smart Scan — the screens ask different
   *  questions about the same row, and the accessible name should say which. */
  verb = "Select",
}: {
  cat: CategorySummary;
  checked: boolean;
  onToggle: () => void;
  verb?: string;
}) {
  return (
    <label
      role="listitem"
      className={`flex cursor-pointer items-center gap-3 border-t border-separator px-4 py-3 transition-colors duration-fast ease-mac first:border-t-0 ${
        checked ? "bg-accentTint" : "hover:bg-surface2"
      }`}
    >
      <Checkbox
        checked={checked}
        onChange={onToggle}
        label={`${verb} ${cat.name}`}
      />
      {/* Ties this row to its arc in the ring. */}
      <span
        className="h-2 w-2 flex-none rounded-full"
        style={{ background: hue(cat.category) }}
        aria-hidden="true"
      />
      <div className="min-w-0 flex-1">
        <span className="truncate text-body font-medium">{cat.name}</span>
        <p className="text-subtle mt-0.5 truncate text-caption">
          {cat.description}
        </p>
      </div>
      <div className="shrink-0 text-right">
        <span className="block font-mono text-body font-semibold tabular-nums">
          {formatBytes(cat.bytes)}
        </span>
        <span className="text-subtle mt-0.5 block font-mono text-caption tabular-nums">
          {cat.count.toLocaleString()} item{cat.count === 1 ? "" : "s"}
        </span>
      </div>
    </label>
  );
}
