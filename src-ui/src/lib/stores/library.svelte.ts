import { library } from "../api/library";
import type { DriveDto, LibraryHandleDto } from "../api/types";

class LibraryStore {
  isOpen = $state(false);
  driveRoot = $state<string | null>(null);
  photoCount = $state(0);
  drives = $state<DriveDto[]>([]);
  loading = $state(false);
  error = $state<string | null>(null);

  async refresh() {
    this.loading = true;
    this.error = null;
    try {
      const cur: LibraryHandleDto | null = await library.current();
      this.isOpen = cur !== null;
      this.driveRoot = cur?.drive_root ?? null;
      this.photoCount = cur?.photo_count ?? 0;
      this.drives = await library.listDrives();
    } catch (e) {
      this.error = JSON.stringify(e);
    } finally {
      this.loading = false;
    }
  }

  async open(drivePath: string) {
    this.loading = true;
    this.error = null;
    try {
      const r = await library.open(drivePath);
      this.isOpen = true;
      this.driveRoot = r.drive_root;
      this.photoCount = r.photo_count;
    } catch (e) {
      this.error = JSON.stringify(e);
      throw e;
    } finally {
      this.loading = false;
    }
  }

  async close() {
    await library.close();
    this.isOpen = false;
    this.driveRoot = null;
    this.photoCount = 0;
  }
}

export const libraryStore = new LibraryStore();
