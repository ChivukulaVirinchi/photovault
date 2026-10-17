/// Shared checks for keyboard shortcuts.

const NON_TEXT_INPUTS = new Set(["checkbox", "radio", "button", "submit", "reset", "color", "file"]);

/** True when the key press belongs to a text field, list box or slider
 *  rather than to a page shortcut. */
export function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target instanceof HTMLInputElement) return !NON_TEXT_INPUTS.has(target.type);
  return target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement || target.isContentEditable;
}

/** True while a dialog is open; pages behind it must ignore their shortcuts. */
export function modalOpen(): boolean {
  return document.querySelector('dialog[open], [role="dialog"][aria-modal="true"]') != null;
}

/** A page-level shortcut should be ignored for this key press. */
export function ignoreShortcut(e: KeyboardEvent): boolean {
  return e.defaultPrevented || isTypingTarget(e.target) || modalOpen();
}
