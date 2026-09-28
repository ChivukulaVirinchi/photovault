import type { LibraryHealthData } from "./api/all";

const MAX_ENTRIES = 4;
const cache = new Map<string, LibraryHealthData>();

export function healthCacheKey(root: string | null, session: number, revision: number): string {
  return `${root ?? "closed"}:${session}:${revision}`;
}

export function getHealthCache(key: string): LibraryHealthData | null {
  const value = cache.get(key);
  if (!value) return null;
  cache.delete(key);
  cache.set(key, value);
  return value;
}

export function setHealthCache(key: string, value: LibraryHealthData): void {
  cache.delete(key);
  cache.set(key, value);
  while (cache.size > MAX_ENTRIES) cache.delete(cache.keys().next().value!);
}

export function clearHealthCache(): void { cache.clear(); }
