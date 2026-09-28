// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { pausePooledImages, pooledImage } from "./assetImage";

afterEach(() => vi.useRealTimers());

describe("pooledImage ownership", () => {
  it("does not restart an active same-URL request when priority changes", () => {
    const node = document.createElement("img");
    const setter = vi.spyOn(node, "src", "set");
    const action = pooledImage(node, { src: "https://example.test/a" });
    action.update({ src: "https://example.test/a", priority: 10 });
    expect(setter).toHaveBeenCalledTimes(1);
    action.destroy();
  });
  it("reclaims five cancelled requests and admits the next image", () => {
    const actions = Array.from({ length: 5 }, (_, i) => pooledImage(document.createElement("img"), { src: `https://example.test/${i}` }));
    const next = document.createElement("img");
    const action = pooledImage(next, { src: "https://example.test/next" });
    expect(next.getAttribute("src")).toBeNull();
    actions.forEach((a) => a.destroy());
    expect(next.getAttribute("src")).toBe("https://example.test/next");
    action.destroy();
  });
  it("holds new loads while interaction has priority", () => {
    const resume = pausePooledImages();
    const node = document.createElement("img");
    const action = pooledImage(node, { src: "https://example.test/deferred" });
    expect(node.getAttribute("src")).toBeNull();
    resume();
    expect(node.getAttribute("src")).toBe("https://example.test/deferred");
    action.destroy();
  });
});
