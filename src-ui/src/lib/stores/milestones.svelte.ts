import { insights, type InsightsData } from "../api/all";

/**
 * Milestone moments — the point at which a growing library is worth
 * celebrating, and worth showing someone.
 *
 * Two rules keep this from becoming spam:
 *
 * - **Nothing fires retroactively.** The first time a library is seen, its
 *   current state is recorded silently. Otherwise somebody who has had an
 *   80,000-photo library for two years would be congratulated on "reaching
 *   50,000" the day they update.
 * - **One moment at a time.** If a fast index jumps straight past several
 *   thresholds, every crossed milestone is marked as seen and only the most
 *   significant one is shown.
 */

export interface Milestone {
  id: string;
  /** Counts up on screen; the hero of the moment. */
  value: number;
  /** Mono label under the number. */
  label: string;
  /** The warm line underneath. */
  line: string;
  achieved: (data: InsightsData) => boolean;
}

/// Ordered most-significant first: the list is scanned in order and the first
/// crossed milestone is the one that gets shown.
const MILESTONES: Milestone[] = [
  {
    id: "photos-250k",
    value: 250_000,
    label: "photographs",
    line: "That is the size of a serious archive. And it is all on your drive.",
    achieved: (d) => d.total_photos >= 250_000,
  },
  {
    id: "photos-100k",
    value: 100_000,
    label: "photographs",
    line: "Very few people have ever seen their own library at this size.",
    achieved: (d) => d.total_photos >= 100_000,
  },
  {
    id: "photos-50k",
    value: 50_000,
    label: "photographs",
    line: "Half a lifetime of pictures, finally in one place.",
    achieved: (d) => d.total_photos >= 50_000,
  },
  {
    id: "photos-10k",
    value: 10_000,
    label: "photographs",
    line: "Most people never see their library this whole.",
    achieved: (d) => d.total_photos >= 10_000,
  },
  {
    id: "photos-1k",
    value: 1_000,
    label: "photographs",
    line: "A real library now. Smriti has read every one of them.",
    achieved: (d) => d.total_photos >= 1_000,
  },
  {
    id: "span-10",
    value: 10,
    label: "years",
    line: "Your photographs now span a decade — long enough to forget, and worth finding again.",
    achieved: (d) => {
      const from = Number(d.date_range_start?.slice(0, 4));
      const to = Number(d.date_range_end?.slice(0, 4));
      return Number.isFinite(from) && Number.isFinite(to) && to - from >= 10;
    },
  },
  {
    id: "people-10",
    value: 10,
    label: "people",
    line: "Ten people now have a thread running through your library.",
    achieved: (d) => d.people_count >= 10,
  },
  {
    id: "cities-25",
    value: 25,
    label: "places",
    line: "Your library has been to twenty-five places.",
    achieved: (d) => d.city_count >= 25,
  },
  {
    id: "people-1",
    value: 1,
    label: "person found",
    line: "Smriti found someone it recognises across your library.",
    achieved: (d) => d.people_count >= 1,
  },
];

const STORAGE_KEY = "smriti.milestones.v1";

/**
 * Which milestones this library has newly crossed, most significant first.
 *
 * Pure, so the ordering rules can be tested without a running app: callers
 * show `[0]` and mark everything returned as seen.
 */
export function crossedMilestones(data: InsightsData, done: readonly string[]): Milestone[] {
  return MILESTONES.filter(
    (milestone) => milestone.achieved(data) && !done.includes(milestone.id),
  );
}

type LibraryMilestones = { done: string[] };
type MilestoneFile = Record<string, LibraryMilestones>;

function readFile(): MilestoneFile {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return {};
    const parsed: unknown = JSON.parse(raw);
    return parsed && typeof parsed === "object" ? (parsed as MilestoneFile) : {};
  } catch {
    return {};
  }
}

function writeFile(file: MilestoneFile) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(file));
  } catch {
    /// A full or blocked localStorage only costs us a repeated celebration.
  }
}

class MilestoneStore {
  /** The milestone currently on screen, if any. */
  current = $state<Milestone | null>(null);
  /** Latest insights snapshot, so "Make a card" needs no second round trip. */
  snapshot = $state<InsightsData | null>(null);
  private evaluating = false;

  async evaluate(driveRoot: string) {
    if (this.evaluating || this.current) return;
    this.evaluating = true;
    try {
      const data = await insights.compute(null);
      this.snapshot = data;
      const file = readFile();
      const known = file[driveRoot];

      /// First sighting of this library: record everything it already
      /// satisfies and stay quiet. Recording an empty list here would make the
      /// *next* check fire every threshold the library passed long ago.
      if (!known) {
        file[driveRoot] = { done: crossedMilestones(data, []).map((m) => m.id) };
        writeFile(file);
        return;
      }

      const crossed = crossedMilestones(data, known.done);
      if (crossed.length === 0) return;

      /// Everything crossed is marked seen; only the headline one is shown.
      known.done.push(...crossed.map((milestone) => milestone.id));
      file[driveRoot] = known;
      writeFile(file);
      this.current = crossed[0];
    } catch {
      /// Never let a celebration break the app; try again on the next trigger.
    } finally {
      this.evaluating = false;
    }
  }

  dismiss() {
    this.current = null;
  }
}

export const milestones = new MilestoneStore();
