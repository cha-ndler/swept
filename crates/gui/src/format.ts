/** Human-readable binary size (matches the CLI's formatting). */
export function formatBytes(bytes: number): string {
  const { value, unit } = formatBytesParts(bytes);
  return `${value} ${unit}`;
}

/**
 * The same figure split into its parts, so the hero can set the number and its
 * unit at different sizes without re-parsing a formatted string.
 */
export function formatBytesParts(bytes: number): {
  value: string;
  unit: string;
} {
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let size = bytes;
  let unit = 0;
  while (size >= 1024 && unit < units.length - 1) {
    size /= 1024;
    unit += 1;
  }
  return unit === 0
    ? { value: String(bytes), unit: "B" }
    : { value: size.toFixed(1), unit: units[unit] };
}

/** `/Users/someone/Downloads/x` → `~/Downloads/x`. Display only. */
export function tilde(path: string): string {
  return path.replace(/^\/Users\/[^/]+\//, "~/");
}

/** The folded path split into the part that locates it and the part that names it. */
export function split(path: string): { dir: string; name: string } {
  const p = tilde(path);
  const slash = p.lastIndexOf("/");
  return slash > 0
    ? { dir: p.slice(0, slash), name: p.slice(slash + 1) }
    : { dir: "", name: p };
}

/** "3y ago" / "8mo ago". An em dash when the mtime could not be read. */
export function formatWhen(ms: number | null): string {
  if (ms === null) return "\u2014";
  const days = Math.floor((Date.now() - ms) / 86_400_000);
  if (days < 1) return "today";
  if (days < 30) return `${days}d ago`;
  const months = Math.floor(days / 30);
  if (months < 12) return `${months}mo ago`;
  return `${Math.floor(days / 365)}y ago`;
}
