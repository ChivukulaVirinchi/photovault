/** Keep automatic paging finite even when a backend returns no progress. */
export async function drainSearchPages(
  current: () => boolean,
  next: () => Promise<boolean>,
  yieldTask: () => Promise<void> = () => new Promise(resolve => setTimeout(resolve, 0)),
): Promise<void> {
  while (current()) {
    if (!await next()) return;
    await yieldTask();
  }
}
