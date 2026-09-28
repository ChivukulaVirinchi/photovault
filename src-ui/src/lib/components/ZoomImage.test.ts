// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { mount, tick, unmount } from "svelte";
import ZoomImage from "./ZoomImage.svelte";

let component: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
    x: 0, y: 0, left: 0, top: 0, right: 1000, bottom: 600,
    width: 1000, height: 600, toJSON: () => ({}),
  });
  vi.stubGlobal("ResizeObserver", class {
    constructor(private callback: ResizeObserverCallback) {}
    observe() { this.callback([], this as unknown as ResizeObserver); }
    unobserve() {}
    disconnect() {}
  });
});

afterEach(async () => {
  if (component) await unmount(component);
  component = null;
  document.body.replaceChildren();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it("opens a large photograph fitted inside the viewer", async () => {
  component = mount(ZoomImage, {
    target: document.body,
    props: {
      src: "/photo.jpg",
      intrinsicWidth: 4000,
      intrinsicHeight: 3000,
    },
  });
  await tick();

  expect(document.querySelector("img")?.style.transform).toContain("scale(0.2)");
});

