// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { mount, unmount, tick } from "svelte";
import SmartSetupDialog from "./SmartSetupDialog.svelte";

const mocks = vi.hoisted(() => ({
  assetHealth: vi.fn(), assetsInventory: vi.fn(), installAssets: vi.fn(), error: vi.fn(),
}));
vi.mock("../api/all", () => ({ systemEx: mocks }));
vi.mock("../stores/toast.svelte", () => ({ toasts: { error: mocks.error } }));
vi.mock("../stores/jobs.svelte", () => ({ jobs: {
  isRunning: () => false, byKind: () => null, register: vi.fn(), dismiss: vi.fn(),
} }));
let component: ReturnType<typeof mount> | null = null;
const settle = async () => { await new Promise((resolve) => setTimeout(resolve, 0)); await tick(); };
beforeEach(() => {
  vi.clearAllMocks();
  mocks.assetHealth.mockResolvedValue({ missing_face_models: true });
  mocks.assetsInventory.mockResolvedValue({ assets: [] });
  mocks.installAssets.mockResolvedValue({ job_id: "setup" });
  HTMLDialogElement.prototype.showModal = function () { this.open = true; };
  HTMLDialogElement.prototype.close = function () { this.open = false; };
});
afterEach(async () => {
  if (component) await unmount(component);
  component = null;
  document.body.replaceChildren();
});
it("asks without downloading, dismisses for the session, and asks on next launch", async () => {
  component = mount(SmartSetupDialog, { target: document.body });
  await settle();
  expect(document.querySelector("dialog")?.open).toBe(true);
  expect(mocks.installAssets).not.toHaveBeenCalled();
  document.querySelector<HTMLButtonElement>("button.ghost")!.click();
  await settle();
  expect(document.querySelector("dialog")?.open).toBe(false);
  await unmount(component);
  component = mount(SmartSetupDialog, { target: document.body });
  await settle();
  expect(document.querySelector("dialog")?.open).toBe(true);
  document.querySelector<HTMLButtonElement>("button.primary")!.click();
  await settle();
  expect(mocks.installAssets).toHaveBeenCalledOnce();
});
it("stays silent when all assets are ready", async () => {
  mocks.assetHealth.mockResolvedValue({ missing_face_models: false, missing_onnx_runtime: false, missing_geonames_db: false });
  mocks.assetsInventory.mockResolvedValue({ assets: ["visual", "text", "tokenizer", "preprocess", "config"].map((name) => ({ id: `vision.semantic.${name}`, active: true })) });
  component = mount(SmartSetupDialog, { target: document.body });
  await settle();
  expect(document.querySelector("dialog")?.open).toBe(false);
});
it("reports a failed check as a toast instead of blocking startup", async () => {
  mocks.assetHealth.mockRejectedValue(new Error("unavailable"));
  component = mount(SmartSetupDialog, { target: document.body });
  await settle();
  expect(document.querySelector("dialog")?.open).toBe(false);
  expect(mocks.error).toHaveBeenCalledOnce();
});
