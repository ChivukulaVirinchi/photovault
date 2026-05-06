import { call } from "./index";
import type {
  DriveDto,
  JobIdDto,
  LibraryHandleDto,
  LibraryOpenResult,
} from "./types";

export const library = {
  listDrives: () => call<DriveDto[]>("library_list_drives"),
  current: () => call<LibraryHandleDto | null>("library_current"),
  open: (drivePath: string) =>
    call<LibraryOpenResult>("library_open", { drive_path: drivePath }),
  close: () => call<null>("library_close"),
  startScan: (scanHidden = false) =>
    call<JobIdDto>("library_start_scan", { scan_hidden_folders: scanHidden }),
  cancelScan: (jobId: string) =>
    call<null>("library_cancel_scan", { job_id: jobId }),
  resolvePath: (photoId: number) =>
    call<{ absolute_path: string }>("library_resolve_path", {
      photo_id: photoId,
    }),
};
