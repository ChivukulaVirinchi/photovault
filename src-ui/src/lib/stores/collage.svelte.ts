import { commandErrorMessage } from "../api";
import { collageApi } from "../api/all";
import { MAX_PHOTOS, MIN_PHOTOS, type ShapeId } from "../collage";
import { toasts } from "./toast.svelte";

export interface CollageOpenOptions {
  /** Arrange and frame the photos automatically (the first is the best). */
  auto?: boolean;
  /** A layout already chosen for these photos, in slot order. */
  layoutId?: string;
  shape?: ShapeId;
}

/// The open collage, if any. Started from the selection bar on any grid,
/// from an album or memory, or from a suggestion on the Memories page, so
/// it lives here rather than in a route.
class CollageStore {
  open = $state(false);
  ids = $state<number[]>([]);
  auto = $state(false);
  preset = $state<{ layoutId: string; shape: ShapeId } | null>(null);
  picking = $state(false);

  canMake(count: number) {
    return count >= MIN_PHOTOS && count <= MAX_PHOTOS;
  }

  show(ids: number[], options: CollageOpenOptions = {}) {
    if (!this.canMake(ids.length)) return;
    this.auto = options.auto ?? false;
    this.preset = options.layoutId && options.shape ? { layoutId: options.layoutId, shape: options.shape } : null;
    this.ids = ids;
    this.open = true;
  }

  /** Pick the best photos of an album or a set and open them arranged. */
  async showBest(source: { albumId?: number; photoIds?: number[] }) {
    if (this.picking) return;
    this.picking = true;
    try {
      const ids = await collageApi.pick(source);
      if (ids.length < MIN_PHOTOS) {
        toasts.info("There aren't enough photos here for a collage");
        return;
      }
      this.show(ids, { auto: true });
    } catch (error) {
      toasts.error(`Couldn't make a collage: ${commandErrorMessage(error)}`);
    } finally {
      this.picking = false;
    }
  }
}

export const collage = new CollageStore();
