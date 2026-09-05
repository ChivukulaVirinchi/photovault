import { afterEach, expect, it, vi } from "vitest";
import { memoryContext, recentMemories, rememberPhoto } from "./surpriseHistory";

afterEach(() => vi.unstubAllGlobals());

it("bounds local history and keeps libraries separate", () => {
  const values = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => values.set(key, value),
  });
  for (let i = 1; i <= 250; i++) rememberPhoto("a", i);
  rememberPhoto("b", 1);
  expect(recentMemories("a")).toHaveLength(200);
  expect(recentMemories("a")[0]).toBe(51);
  expect(recentMemories("b")).toEqual([1]);
});

it("tolerates missing, corrupt or unavailable storage", () => {
  vi.stubGlobal("localStorage", { getItem: () => "not json", setItem: () => { throw Error(); } });
  expect(recentMemories("a")).toEqual([]);
  expect(() => rememberPhoto("a", 1)).not.toThrow();
});

it("only describes known date and location metadata", () => {
  expect(memoryContext({ date_taken: null, location: null })).toBe("");
  expect(memoryContext({ date_taken: "invalid", location: { city: null, country: "India" } })).toBe("India");
  expect(memoryContext({ date_taken: "2017-11-12T12:00:00Z", location: { city: "Pondicherry", country: "India" } }))
    .toMatch(/2017 · Pondicherry$/);
});
