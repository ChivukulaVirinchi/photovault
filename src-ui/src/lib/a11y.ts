/** Accessible name for a photo tile, e.g. "Photo, 12 May 2024". */
export function photoLabel(p: { date_taken?: string | null; media_type?: string | null; taken_at?: string | null }): string {
  const kind = p.media_type === "video" ? "Video" : "Photo";
  const raw = p.date_taken ?? p.taken_at;
  const date = raw ? new Date(raw) : null;
  if (!date || Number.isNaN(date.getTime())) return kind;
  return `${kind}, ${date.toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" })}`;
}
