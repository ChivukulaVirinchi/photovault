import { settingsStore } from "./stores/settings.svelte";

/** A count with its noun: "1 photo", "2 photos", "1,204 people". */
export function countOf(n: number, one: string, many = `${one}s`): string {
  return `${n.toLocaleString()} ${n === 1 ? one : many}`;
}

/** Human file size: "820 KB", "1.4 GB". */
export function formatBytes(bytes: number | null | undefined): string {
  if (!bytes) return "-";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value >= 10 || unit === 0 ? value.toFixed(0) : value.toFixed(1)} ${units[unit]}`;
}

/** A short numeric date in the format chosen in Settings. */
export function shortDate(value: string | Date): string {
  const d = new Date(value);
  if (Number.isNaN(d.getTime())) return "";
  const y = d.getFullYear();
  const m = d.getMonth() + 1;
  const day = d.getDate();
  const pad = (n: number) => String(n).padStart(2, "0");
  switch (settingsStore.data?.date_format) {
    case "iso": return `${y}-${pad(m)}-${pad(day)}`;
    case "us": return `${m}/${day}/${y}`;
    case "eu": return `${day}/${m}/${y}`;
    default: return d.toLocaleDateString();
  }
}

/** "16/5/2026", or "16/5/2026 – 2/6/2026" when the days differ. */
export function dateRange(start: string, end: string): string {
  const a = shortDate(start);
  const b = shortDate(end);
  return a === b ? a : `${a} – ${b}`;
}
