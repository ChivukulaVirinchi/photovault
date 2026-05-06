import { library } from "../api/library";
import { settings } from "../api/all";
import type { DriveDto, LibraryHandleDto } from "../api/types";

const MAX_REMEMBERED = 10;

class LibraryStore {
  isOpen = $state(false);
  driveRoot = $state<string | null>(null);
  photoCount = $state(0);
  drives = $state<DriveDto[]>([]);
  /// Recently-opened paths from settings.remembered_drives. Most-recent first.
  remembered = $state<string[]>([]);
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
      try {
        const s = await settings.get();
        this.remembered = s.remembered_drives ?? [];
      } catch {
        // Settings is best-effort here; drive picker still works without.
        this.remembered = [];
      }
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
      // Push into remembered_drives, dedup, cap at MAX_REMEMBERED.
      // Best-effort persistence — if settings.update fails, we still opened.
      try {
        const next = [
          r.drive_root,
          ...this.remembered.filter((p) => p !== r.drive_root),
        ].slice(0, MAX_REMEMBERED);
        const updated = await settings.update({ remembered_drives: next });
        this.remembered = updated.remembered_drives ?? next;
      } catch {
        // Silent — opening succeeded; persistence is a polish layer.
      }
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
