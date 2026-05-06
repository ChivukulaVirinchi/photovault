import { call } from "./index";
import type { AppVersionDto, AssetHealthDto } from "./types";

export const system = {
  assetHealth: () => call<AssetHealthDto>("system_asset_health"),
  appVersion: () => call<AppVersionDto>("system_app_version"),
  openInExplorer: (photoId: number) =>
    call<void>("system_open_in_explorer", { photo_id: photoId }),
};
