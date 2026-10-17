import { call } from "./index";

export interface ShareResult {
  method: "native" | "clipboard" | "email";
}

export const system = {
  openInExplorer: (photoId: number) =>
    call<void>("system_open_in_explorer", { photo_id: photoId }),
  sharePhoto: (photoId: number) =>
    call<ShareResult>("system_share_photo", { photo_id: photoId }),
  openPath: (path: string) => call<void>("system_open_path", { path }),
};
