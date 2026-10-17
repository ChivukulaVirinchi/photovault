const FOCUSABLE =
  'a[href], button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex="-1"])';

/**
 * For custom (non-`<dialog>`) modals: moves focus into the dialog when it
 * opens, keeps Tab and Shift+Tab inside it, and puts focus back where it
 * was when it closes. The node should have `tabindex="-1"`.
 */
export function trapFocus(node: HTMLElement) {
  const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  const focusables = () =>
    [...node.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => el.getClientRects().length > 0);

  const onKey = (e: KeyboardEvent) => {
    if (e.key !== "Tab") return;
    const list = focusables();
    if (list.length === 0) { e.preventDefault(); node.focus(); return; }
    const first = list[0];
    const last = list[list.length - 1];
    const inside = node.contains(document.activeElement);
    if (e.shiftKey && (!inside || document.activeElement === first || document.activeElement === node)) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && (!inside || document.activeElement === last)) {
      e.preventDefault();
      first.focus();
    }
  };

  node.addEventListener("keydown", onKey);
  queueMicrotask(() => {
    if (node.contains(document.activeElement)) return;
    // The dialog itself, unless a field asks for focus: focusing a button
    // near the bottom would scroll a long dialog away from its title.
    (node.querySelector<HTMLElement>("[autofocus]") ?? node).focus({ preventScroll: true });
  });

  return {
    destroy() {
      node.removeEventListener("keydown", onKey);
      if (previous && document.contains(previous)) previous.focus();
    },
  };
}
