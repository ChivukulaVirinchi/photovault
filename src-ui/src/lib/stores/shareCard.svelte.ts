import type { InsightsData } from "../api/all";

/**
 * Holds the currently-open library card. Two surfaces open it — the Insights
 * page and a milestone moment — so the state lives here rather than in either
 * caller.
 */
class ShareCardStore {
  open = $state(false);
  data = $state<InsightsData | null>(null);

  /** Ignore requests while insights are still loading. */
  show(data: InsightsData | null) {
    if (!data || data.total_photos === 0) return;
    this.data = data;
    this.open = true;
  }
}

export const shareCard = new ShareCardStore();
