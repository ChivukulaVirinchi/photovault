const LIMIT = 200;
const key = (library: string) => `smriti:surprise:${library}`;

export function recentMemories(library: string): number[] {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(key(library)) ?? "[]");
    return Array.isArray(value)
      ? value.filter((id): id is number => Number.isSafeInteger(id) && id > 0).slice(-LIMIT)
      : [];
  } catch { return []; }
}

export function rememberPhoto(library: string, id: number) {
  try {
    const ids = [...recentMemories(library).filter((other) => other !== id), id].slice(-LIMIT);
    localStorage.setItem(key(library), JSON.stringify(ids));
  } catch { /* Browsing still works if local storage is unavailable. */ }
}

export function memoryContext(photo: {
  date_taken: string | null;
  location: { city: string | null; country: string | null } | null;
}): string {
  const date = photo.date_taken ? new Date(photo.date_taken) : null;
  const when = date && Number.isFinite(date.getTime())
    ? date.toLocaleDateString(undefined, { month: "long", year: "numeric" }) : "";
  const where = photo.location?.city || photo.location?.country || "";
  return [when, where].filter(Boolean).join(" · ");
}
