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
