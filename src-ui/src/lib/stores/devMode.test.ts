// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from "vitest";

import { devMode } from "./devMode.svelte";

beforeEach(() => {
  devMode.set(false);
});

describe("devMode store", () => {
  it("starts disabled by default", () => {
    expect(devMode.enabled).toBe(false);
  });

  it("set(true) flips the flag and persists to localStorage", () => {
    devMode.set(true);
    expect(devMode.enabled).toBe(true);
    expect(localStorage.getItem("smriti:devMode")).toBe("1");
  });

  it("set(false) clears localStorage", () => {
    devMode.set(true);
    devMode.set(false);
    expect(localStorage.getItem("smriti:devMode")).toBeNull();
  });

  it("toggle flips the current state", () => {
    devMode.toggle();
    expect(devMode.enabled).toBe(true);
    devMode.toggle();
    expect(devMode.enabled).toBe(false);
  });
});
