/// Primary tabs stay mounted after their first visit and are hidden with
/// `inert` while another route is showing (see App.svelte). Window-level
/// key handlers registered by those tabs keep firing while hidden, so they
/// must ignore events unless their own panel is the visible one —
/// otherwise Delete on an album page would also trash the shared selection
/// from the hidden Timeline.
export function inInactivePanel(el: Element | null | undefined): boolean {
  return !el || !el.isConnected || el.closest("[inert]") !== null;
}
