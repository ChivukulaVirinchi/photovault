export const SEARCH_WINDOW_SIZE = 1000;
export const SEARCH_MAX_RESULTS = 20_000;

export function nextSearchPage(loaded: number, pageSize = 200): { offset: number; limit: number } | null {
  const offset = Math.max(0, Math.floor(loaded));
  if (offset >= SEARCH_MAX_RESULTS) return null;
  const limit = Math.min(Math.max(1, Math.floor(pageSize)), SEARCH_MAX_RESULTS - offset);
  return { offset, limit };
}

/** A bounded, navigable window, never a permanent prefix truncation. */
export function searchWindow(total: number, requestedEnd: number) {
  const end = Math.max(0, Math.min(total, requestedEnd));
  const start = end === 0 ? 0 : Math.floor((end - 1) / SEARCH_WINDOW_SIZE) * SEARCH_WINDOW_SIZE;
  return { start, end };
}
