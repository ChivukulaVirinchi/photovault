// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { pooledImage } from "./assetImage";
afterEach(() => vi.useRealTimers());

describe("image presentation", () => {
  it("hides the native image until load, and hides again on source replacement", () => {
    const img = document.createElement("img");
    const action = pooledImage(img, { src: "https://example.test/a" });
    expect(img.dataset.imageState).toBe("loading");
    expect(img.style.opacity).toBe("0");
    img.dispatchEvent(new Event("load"));
    expect(img.dataset.imageState).toBe("ready");
    action.update({ src: "https://example.test/b" });
    expect(img.style.opacity).toBe("0");
    img.dispatchEvent(new Event("error"));
    expect(img.dataset.imageState).toBe("error");
    expect(img.getAttribute("src")).toBeNull();
    action.destroy();
  });
  it("times out and removes the source before releasing capacity", () => {
    vi.useFakeTimers();
    const img = document.createElement("img");
    const action = pooledImage(img, { src: "https://example.test/slow" });
    vi.advanceTimersByTime(12_000);
    expect(img.dataset.imageState).toBe("error");
    expect(img.getAttribute("src")).toBeNull();
    action.destroy();
  });
  it("does not let stale decode reveal the next photo", async () => {
    const img = document.createElement("img");
    let done!: () => void;
    img.decode = () => new Promise<void>(resolve => { done = resolve; });
    const action = pooledImage(img, { src: "https://example.test/a" });
    img.dispatchEvent(new Event("load"));
    action.update({ src: "https://example.test/b" });
    done();
    await Promise.resolve();
    expect(img.dataset.imageState).toBe("loading");
    action.destroy();
  });
  it("handles an already-complete cached image without waiting for a new event", () => {
    const img = document.createElement("img");
    Object.defineProperty(img, "complete", { value: true });
    Object.defineProperty(img, "naturalWidth", { value: 100 });
    const action = pooledImage(img, { src: "https://example.test/cached" });
    expect(img.dataset.imageState).toBe("ready");
    action.destroy();
  });
});
