export function formatCompactCount(value: number): string {
  if (value < 1_000) return String(value);
  const units = [
    { threshold: 1_000_000_000, suffix: "B" },
    { threshold: 1_000_000, suffix: "M" },
    { threshold: 1_000, suffix: "K" },
  ];
  const unit = units.find((candidate) => value >= candidate.threshold)!;
  const compact = value / unit.threshold;
  return `${compact >= 10 || Number.isInteger(compact) ? compact.toFixed(0) : compact.toFixed(1)}${unit.suffix}`;
}

export function formatFileSize(bytes: number): string {
  if (bytes < 1_024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1_024;
  let index = 0;
  while (value >= 1_024 && index < units.length - 1) {
    value /= 1_024;
    index += 1;
  }
  return `${value >= 10 ? value.toFixed(0) : value.toFixed(1)} ${units[index]}`;
}
