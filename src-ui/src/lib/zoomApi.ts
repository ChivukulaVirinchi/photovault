/// Imperative API exposed by ZoomImage.svelte to its parent
/// (PhotoDetail.svelte) so toolbar buttons + keyboard shortcuts can
/// drive zoom without reaching into the child's internal state.
export interface ZoomApi {
  zoomIn(): void;
  zoomOut(): void;
  fit(): void;
  actual(): void;
}
