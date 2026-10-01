import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  status: vi.fn(), setup: vi.fn(), faces: vi.fn(), places: vi.fn(),
  listeners: new Map<string, (event: { payload: Record<string, unknown> }) => unknown>(),
  library: { driveRoot: "/photos", session: 1, isOpen: true },
  jobs: { jobs: new Map(), install: vi.fn(), register: vi.fn(), isRunning: vi.fn() },
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name, listener) => {
    mocks.listeners.set(name, listener);
    return () => mocks.listeners.delete(name);
  }),
}));
vi.mock("../api/all", () => ({
  systemEx: { assetSetupStatus: mocks.status, installAssets: mocks.setup },
  people: { startProcessing: mocks.faces }, geocoding: { backfill: mocks.places },
}));
vi.mock("./library.svelte", () => ({ libraryStore: mocks.library }));
vi.mock("./jobs.svelte", () => ({ jobs: mocks.jobs }));

beforeEach(() => {
  vi.resetModules();
  vi.clearAllMocks();
  const storage = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => storage.set(key, value),
  });
  mocks.listeners.clear();
  mocks.jobs.jobs.clear();
  mocks.library.session = 1;
  mocks.library.driveRoot = "/photos";
  mocks.library.isOpen = true;
  mocks.status.mockResolvedValue({ faces: false, visual: false, places: false });
  mocks.setup.mockResolvedValue({ job_id: "setup" });
  mocks.jobs.isRunning.mockReturnValue(false);
  mocks.faces.mockResolvedValue({ job_id: "faces" });
  mocks.places.mockResolvedValue({ job_id: "places" });
});

async function store() {
  const { assetSetup } = await import("./assetSetup.svelte");
  await assetSetup.install();
  return assetSetup;
}
async function complete() {
  await mocks.listeners.get("assets:complete")?.({ payload: { job_id: "setup" } });
}

describe("smart feature setup", () => {
  it("does not download on startup, and remembers Later across launches", async () => {
    const setup = await store();
    expect(mocks.setup).not.toHaveBeenCalled();
    setup.dismissIntro();
    vi.resetModules();
    expect((await import("./assetSetup.svelte")).assetSetup.introSeen).toBe(true);
  });

  it("starts only the requested feature after the download completes", async () => {
    const setup = await store();
    await setup.enable("places");
    expect(mocks.places).not.toHaveBeenCalled();
    await complete();
    expect(mocks.setup).toHaveBeenCalledWith("places");
    expect(mocks.places).toHaveBeenCalledWith(false, 1);
    expect(mocks.faces).not.toHaveBeenCalled();
  });

  it("waits for library scanning, then resumes without another click", async () => {
    const setup = await store();
    await setup.enable("faces");
    mocks.jobs.isRunning.mockImplementation((kind) => kind === "scan");
    await complete();
    expect(mocks.faces).not.toHaveBeenCalled();
    mocks.jobs.isRunning.mockReturnValue(false);
    setup.resume();
    expect(mocks.faces).toHaveBeenCalledWith(1);
    setup.resume();
    expect(mocks.faces).toHaveBeenCalledTimes(1);
  });

  it("never starts work in a different library session", async () => {
    const setup = await store();
    await setup.enable("faces");
    mocks.library.session = 2;
    await complete();
    expect(mocks.faces).not.toHaveBeenCalled();
  });

  it("retains a download error after job indicators disappear and allows retry", async () => {
    const setup = await store();
    await setup.enable("faces");
    mocks.listeners.get("assets:progress")?.({ payload: { job_id: "setup", stage: "error", message: "Offline" } });
    expect(setup.errors.faces).toBe("Offline");
    setup.resume();
    expect(mocks.faces).not.toHaveBeenCalled();
    await setup.enable("faces");
    expect(setup.errors.faces).toBeUndefined();
    await complete();
    expect(mocks.faces).toHaveBeenCalledTimes(1);
  });

  it("handles a cached setup completing before its command returns", async () => {
    const setup = await store();
    mocks.setup.mockImplementation(async () => {
      mocks.jobs.jobs.set("setup", { status: "complete" });
      await complete();
      return { job_id: "setup" };
    });
    await setup.enable("faces");
    expect(mocks.faces).toHaveBeenCalledTimes(1);
  });

  it("still starts places if face processing cannot start", async () => {
    const setup = await store();
    mocks.faces.mockRejectedValue("Face model unavailable");
    await setup.enable("all");
    await complete();
    expect(mocks.places).toHaveBeenCalledTimes(1);
    expect(setup.errors.faces).toBe("Face model unavailable");
  });
});
