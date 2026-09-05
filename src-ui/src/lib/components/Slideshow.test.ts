// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { mount, unmount, tick } from "svelte";
import Slideshow from "./Slideshow.svelte";
import { slideshow } from "../stores/slideshow.svelte";
import { decodeOffscreen } from "../decodeOffscreen";

vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (path: string) => path, invoke: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: vi.fn() }));
vi.mock("../stores/library.svelte", () => ({ libraryStore: { driveRoot: "/test-library" } }));
vi.mock("../thumbnail", () => ({ thumbUrl: () => null }));
vi.mock("../decodeOffscreen", () => ({ decodeOffscreen: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../api/photos", () => ({
  photos: { get: vi.fn(async (id: number) => ({
    id, file_name: `photo-${id}.jpg`, date_taken: "2017-11-12T12:00:00Z",
    location: { city: "Pondicherry", country: "India" }, media_type: "photo", thumbnail_path: null,
  })) },
}));
vi.mock("../api/library", () => ({
  library: { resolvePath: vi.fn(async (id: number) => ({ absolute_path: `/photo-${id}.jpg` })) },
}));

let component: ReturnType<typeof mount> | null = null;
const settle = async () => { await tick(); await vi.advanceTimersByTimeAsync(0); await tick(); };
beforeEach(() => {
  vi.useFakeTimers();
  slideshow.close();
  vi.mocked(decodeOffscreen).mockReset().mockResolvedValue(undefined);
});
afterEach(async () => {
  slideshow.close();
  if (component) await unmount(component);
  component = null;
  document.body.replaceChildren();
  vi.useRealTimers();
});

it("reuses image buffers and quiet controls without a new feed or captions", async () => {
  component = mount(Slideshow, { target: document.body });
  slideshow.start({ kind: "surprise", label: "Trip", ids: [1, 2, 3] });
  await settle();
  expect(document.querySelector('[role="dialog"]')?.getAttribute("aria-label")).toBe("Surprise me slideshow");
  expect(document.querySelector(".slide-image.visible")?.getAttribute("src")).toBe("/photo-1.jpg");
  expect(document.querySelector(".memory-context")?.textContent).toContain("Remember this?");
  expect(document.querySelector(".memory-context")?.textContent).toContain("Pondicherry");
  expect(document.querySelector(".title")?.textContent).not.toContain("photo-1.jpg");
  expect(document.querySelector(".progress")).toBeNull();
  expect(document.querySelector('[aria-label="Toggle loop"]')).toBeNull();
  await vi.advanceTimersByTimeAsync(12000);
  await settle();
  expect(slideshow.currentId()).toBe(2);
  expect(document.querySelector(".memory-intro")).toBeNull();
  expect(document.querySelectorAll(".slide-image")).toHaveLength(2);
  window.dispatchEvent(new KeyboardEvent("keydown", { key: " ", bubbles: true }));
  await vi.advanceTimersByTimeAsync(24000);
  expect(slideshow.currentId()).toBe(2);
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  await settle();
  expect(document.querySelector('[role="dialog"]')).toBeNull();
});

it("starts the dwell timer only after the photograph is ready", async () => {
  let ready!: () => void;
  vi.mocked(decodeOffscreen).mockImplementationOnce(() => new Promise<void>((resolve) => { ready = resolve; }));
  component = mount(Slideshow, { target: document.body });
  slideshow.start({ kind: "surprise", label: "Trip", ids: [1, 2, 3] });
  await settle();
  await vi.advanceTimersByTimeAsync(24000);
  expect(slideshow.currentId()).toBe(1);
  ready();
  await settle();
  await vi.advanceTimersByTimeAsync(11999);
  expect(slideshow.currentId()).toBe(1);
  await vi.advanceTimersByTimeAsync(1);
  await settle();
  expect(slideshow.currentId()).toBe(2);
});
