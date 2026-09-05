import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { library } from "./library";
import { photos } from "./photos";
import { settings } from "./all";
import contracts from "../../../../tests/fixtures/ipc.json";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(null) }));
beforeEach(() => vi.mocked(invoke).mockClear());

it("sends the display-rendition envelope accepted by Rust", async () => {
  await library.resolvePath(42, true);
  expect(invoke).toHaveBeenCalledWith("library_resolve_path", contracts.library_resolve_path);
});

it("preserves explicit null settings in the IPC envelope", async () => {
  await settings.update(contracts.settings_update.args);
  expect(invoke).toHaveBeenCalledWith("settings_update", contracts.settings_update);
});

it("sends the originating session and fingerprint with video writeback", async () => {
  await photos.saveVideoProbe(contracts.photos_save_video_probe.args);
  expect(invoke).toHaveBeenCalledWith("photos_save_video_probe", contracts.photos_save_video_probe);
});
