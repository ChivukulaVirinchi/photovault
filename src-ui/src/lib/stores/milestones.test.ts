// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";

import { insights, type InsightsData } from "../api/all";
import { crossedMilestones, milestones } from "./milestones.svelte";

vi.mock("../api/all", () => ({ insights: { compute: vi.fn() } }));

function insightsData(overrides: Partial<InsightsData> = {}): InsightsData {
  return {
    total_photos: 0,
    date_range_start: null,
    date_range_end: null,
    people_count: 0,
    album_count: 0,
    country_count: 0,
    city_count: 0,
    photos_with_gps: 0,
    hero_photo_id: null,
    hero_thumbnail_path: null,
    heatmap: {},
    heatmap_year: 2026,
    months_by_year: {},
    monthly_counts: [],
    top_people: [],
    top_locations: [],
    top_countries: [],
    top_cameras: [],
    available_years: [],
    ...overrides,
  };
}

/// Point the mocked command at a library with this many photos.
function libraryWith(overrides: Partial<InsightsData>) {
  vi.mocked(insights.compute).mockResolvedValue(insightsData(overrides));
}

describe("milestone selection", () => {
  it("reports nothing for a library below the first threshold", () => {
    expect(crossedMilestones(insightsData({ total_photos: 999 }), [])).toEqual([]);
  });

  it("reports a threshold the moment it is reached", () => {
    const crossed = crossedMilestones(insightsData({ total_photos: 1_000 }), []);
    expect(crossed.map((m) => m.id)).toEqual(["photos-1k"]);
  });

  it("orders by significance so a fast index shows the biggest one", () => {
    /// A first scan that lands on 12,000 photos crosses both 1k and 10k; the
    /// user should be congratulated on 10,000, not on 1,000.
    const crossed = crossedMilestones(insightsData({ total_photos: 12_000 }), []);
    expect(crossed.map((m) => m.id)).toEqual(["photos-10k", "photos-1k"]);
  });

  it("does not re-fire a milestone that is already recorded", () => {
    const crossed = crossedMilestones(insightsData({ total_photos: 12_000 }), [
      "photos-10k",
      "photos-1k",
    ]);
    expect(crossed).toEqual([]);
  });

  it("treats a ten-year span as a milestone only when both ends are known", () => {
    const spanning = insightsData({
      date_range_start: "2013-04-02",
      date_range_end: "2026-01-19",
    });
    expect(crossedMilestones(spanning, []).map((m) => m.id)).toContain("span-10");

    const undated = insightsData({ date_range_start: null, date_range_end: null });
    expect(crossedMilestones(undated, []).map((m) => m.id)).not.toContain("span-10");
  });

  it("counts people and places independently of photo volume", () => {
    const people = insightsData({ total_photos: 400, people_count: 1 });
    expect(crossedMilestones(people, []).map((m) => m.id)).toEqual(["people-1"]);

    const places = insightsData({ total_photos: 400, city_count: 25 });
    expect(crossedMilestones(places, []).map((m) => m.id)).toEqual(["cities-25"]);
  });
});

describe("milestone evaluation", () => {
  beforeEach(() => {
    localStorage.clear();
    milestones.current = null;
    vi.mocked(insights.compute).mockReset();
  });

  it("stays quiet for a library that already passed the thresholds", async () => {
    /// The library predates the feature. Nothing should be celebrated on the
    /// first check — nor on the one after it, which is the bug this guards.
    libraryWith({ total_photos: 80_000 });
    await milestones.evaluate("/drive/existing");
    expect(milestones.current).toBeNull();

    await milestones.evaluate("/drive/existing");
    expect(milestones.current).toBeNull();
  });

  it("celebrates a threshold crossed after the baseline", async () => {
    libraryWith({ total_photos: 4_000 });
    await milestones.evaluate("/drive/growing");
    expect(milestones.current).toBeNull();

    libraryWith({ total_photos: 12_000 });
    await milestones.evaluate("/drive/growing");
    expect(milestones.current?.id).toBe("photos-10k");
  });

  it("marks every crossed threshold as seen so they do not queue up", async () => {
    libraryWith({ total_photos: 1 });
    await milestones.evaluate("/drive/queued");

    libraryWith({ total_photos: 12_000 });
    await milestones.evaluate("/drive/queued");
    expect(milestones.current?.id).toBe("photos-10k");

    /// 1k was crossed in the same jump and must not follow 10k on screen.
    milestones.dismiss();
    await milestones.evaluate("/drive/queued");
    expect(milestones.current).toBeNull();
  });

  it("keeps libraries independent of one another", async () => {
    libraryWith({ total_photos: 90_000 });
    await milestones.evaluate("/drive/first");
    libraryWith({ total_photos: 400 });
    await milestones.evaluate("/drive/second");

    /// The second drive is below every threshold, so nothing fires from it…
    expect(milestones.current).toBeNull();

    /// …even though the first drive had recorded a high baseline.
    libraryWith({ total_photos: 120_000 });
    await milestones.evaluate("/drive/first");
    expect(milestones.current?.id).toBe("photos-100k");
  });
});
