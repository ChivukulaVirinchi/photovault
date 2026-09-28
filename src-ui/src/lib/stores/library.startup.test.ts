import { beforeEach, describe, expect, it, vi } from "vitest";
const mocks = vi.hoisted(() => ({
  current: vi.fn(), listDrives: vi.fn(), open: vi.fn(),
  get: vi.fn(), update: vi.fn(),
}));
vi.mock("../api/library", () => ({ library: mocks }));
vi.mock("../api/all", () => ({ settings: mocks }));
vi.mock("../thumbnailRequest", () => ({ resetThumbnailRequests: vi.fn() }));
import { libraryStore } from "./library.svelte";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(r => { resolve = r; });
  return { promise, resolve };
}
beforeEach(() => {
  vi.clearAllMocks();
  libraryStore.isOpen = false;
  libraryStore.driveRoot = null;
  libraryStore.session = 0;
  mocks.get.mockResolvedValue({ remembered_drives: ["/old"] });
  mocks.update.mockResolvedValue({ remembered_drives: ["/chosen"] });
});

describe("startup ownership", () => {
  it("restores the backend session and does not open remembered paths", async () => {
    mocks.current.mockResolvedValue({ drive_root: "/existing", photo_count: 2, read_only: false, schema_too_new: null, library_session_id: 42 });
    mocks.listDrives.mockResolvedValue([]);
    await libraryStore.refresh();
    expect(libraryStore.session).toBe(42);
    expect(libraryStore.isOpen).toBe(true);
    expect(mocks.open).not.toHaveBeenCalled();
  });
  it("allows manual selection while drive discovery is pending; late refresh cannot override it", async () => {
    const drives = deferred<never[]>();
    mocks.current.mockResolvedValue(null);
    mocks.listDrives.mockReturnValue(drives.promise);
    const refresh = libraryStore.refresh();
    await Promise.resolve(); await Promise.resolve(); await Promise.resolve();
    expect(libraryStore.loading).toBe(false);
    mocks.open.mockResolvedValue({ drive_root: "/chosen", photo_count: 10, library_session_id: 7, read_only: false, schema_too_new: null });
    await libraryStore.open("/chosen");
    drives.resolve([]);
    await refresh;
    expect(libraryStore.driveRoot).toBe("/chosen");
    expect(libraryStore.session).toBe(7);
  });
});
