import { beforeEach, describe, expect, it, vi } from "vitest";

// Stub @tauri-apps/api/core::convertFileSrc — in real usage it
// returns a `tauri://localhost/path` style URL via the asset
// protocol; in tests we just echo the input prefixed so we can
// assert what got passed.
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => `mock://${p}`,
}));

let thumbUrl: typeof import("./thumbnail").thumbUrl;

beforeEach(async () => {
  vi.resetModules();
  ({ thumbUrl } = await import("./thumbnail"));
});

describe("thumbUrl", () => {
  it("returns null when path is null", () => {
    expect(thumbUrl("/drive", null)).toBeNull();
  });

  it("returns null when path is relative but driveRoot is missing", () => {
    expect(thumbUrl(null, "relative/thumb.jpg")).toBeNull();
  });

  it("joins relative path against drive root with a slash separator", () => {
    expect(thumbUrl("/drive", "thumb.jpg")).toBe("mock:///drive/thumb.jpg");
  });

  it("does not double-up the slash when drive root already ends in one", () => {
    expect(thumbUrl("/drive/", "thumb.jpg")).toBe("mock:///drive/thumb.jpg");
  });

  it("accepts Windows-style drive root ending in backslash", () => {
    expect(thumbUrl("C:\\drive\\", "thumb.jpg")).toBe(
      "mock://C:\\drive\\thumb.jpg",
    );
  });

  it("rejects absolute Windows drive-letter paths", () => {
    expect(thumbUrl("/anywhere", "D:\\photos\\img.jpg")).toBeNull();
  });

  it("rejects forward-slash absolute paths", () => {
    expect(thumbUrl("/anywhere", "/abs/path/img.jpg")).toBeNull();
  });

  it("rejects UNC paths", () => {
    expect(thumbUrl("/anywhere", "\\\\server\\share\\img.jpg")).toBeNull();
  });

  it("rejects relative traversal and malformed relative paths", () => {
    expect(thumbUrl("/drive", "../secret.jpg")).toBeNull();
    expect(thumbUrl("/drive", ".photovault/../secret.jpg")).toBeNull();
    expect(thumbUrl("/drive", ".photovault//thumb.jpg")).toBeNull();
    expect(thumbUrl("/drive", ".photovault\\thumb.jpg")).toBeNull();
  });
});
