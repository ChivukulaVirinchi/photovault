/** Wait before reusing a fading slot. Reduced motion has no animations.
 * Cancel animations that never finish so the hidden slot can be reused safely.
 */
export async function finishSlotTransition(node: Element | null, timeoutMs = 1000): Promise<void> {
  const animations = node?.getAnimations?.() ?? [];
  if (!animations.length) return;
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    await Promise.race([
      Promise.all(animations.map((animation) => animation.finished.catch(() => undefined))),
      new Promise<void>((resolve) => {
        timer = setTimeout(() => {
          animations.forEach((animation) => animation.cancel());
          resolve();
        }, timeoutMs);
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}
