import { library } from "../api/library";
import { resetThumbnailRequests } from "../thumbnailRequest";
import { settings } from "../api/all";
import { commandErrorMessage } from "../api";
import type {
  CommandError,
  DriveDto,
  LibraryHandleDto,
  SchemaTooNewInfo,
} from "../api/types";

const MAX_REMEMBERED = 10;

class LibraryStore {
  session = $state(0);
  isOpen = $state(false);
  driveRoot = $state<string | null>(null);
  photoCount = $state(0);
  unsupportedSchema = $state<SchemaTooNewInfo | null>(null);
  drives = $state<DriveDto[]>([]);
  /// Recently-opened paths from settings.remembered_drives. Most-recent first.
  remembered = $state<string[]>([]);
  loading = $state(false);
  error = $state<string | null>(null);
  lastError = $state<CommandError | null>(null);
  private seq = 0;

  private async syncCurrent(seq: number) {
    const cur: LibraryHandleDto | null = await library.current();
    if (seq !== this.seq) return;
    this.isOpen = cur !== null && !cur.read_only;
    this.driveRoot = cur?.drive_root ?? null;
    this.photoCount = cur?.photo_count ?? 0;
    this.unsupportedSchema = cur?.schema_too_new ?? null;
  }

  async refresh() {
    const seq = ++this.seq;
    this.loading = true;
    this.error = null;
    this.lastError = null;
    try {
      await this.syncCurrent(seq);
      const drives = await library.listDrives();
      if (seq !== this.seq) return;
      this.drives = drives;
      try {
        const s = await settings.get();
        if (seq !== this.seq) return;
        this.remembered = s.remembered_drives ?? [];
      } catch {
        if (seq !== this.seq) return;
        // Settings is best-effort here; drive picker still works without.
        this.remembered = [];
      }
    } catch (e) {
      if (seq === this.seq) {
        this.error = commandErrorMessage(e);
        this.lastError = isCommandError(e) ? e : null;
      }
    } finally {
      if (seq === this.seq) this.loading = false;
    }
  }

  async open(drivePath: string) {
    resetThumbnailRequests();
    const seq = ++this.seq;
    this.loading = true;
    this.error = null;
    this.lastError = null;
    try {
      const r = await library.open(drivePath);
      if (seq !== this.seq) return;
      resetThumbnailRequests();
      this.session += 1;
      this.isOpen = !r.read_only;
      this.driveRoot = r.drive_root;
      this.photoCount = r.photo_count;
      this.unsupportedSchema = r.schema_too_new;
      // Push into remembered_drives, dedup, cap at MAX_REMEMBERED.
      // Best-effort persistence — if settings.update fails, we still opened.
      try {
        const next = [
          r.drive_root,
          ...this.remembered.filter((p) => p !== r.drive_root),
        ].slice(0, MAX_REMEMBERED);
        const updated = await settings.update({ remembered_drives: next });
        if (seq !== this.seq) return;
        this.remembered = updated.remembered_drives ?? next;
      } catch {
        // Silent — opening succeeded; persistence is a polish layer.
      }
    } catch (e) {
      if (seq === this.seq) {
        this.error = commandErrorMessage(e);
        this.lastError = isCommandError(e) ? e : null;
        try {
          await this.syncCurrent(seq);
        } catch {
          if (seq === this.seq) {
            this.isOpen = false;
            this.driveRoot = null;
            this.photoCount = 0;
            this.unsupportedSchema = null;
          }
        }
      }
      throw e;
    } finally {
      if (seq === this.seq) this.loading = false;
    }
  }

  async close() {
    resetThumbnailRequests();
    const seq = ++this.seq;
    await library.close();
    if (seq !== this.seq) return;
    this.isOpen = false;
    this.session += 1;
    this.driveRoot = null;
    this.photoCount = 0;
    this.unsupportedSchema = null;
    this.error = null;
    this.lastError = null;
  }
}

function isCommandError(error: unknown): error is CommandError {
  return Boolean(error && typeof error === "object" && "kind" in error);
}

export const libraryStore = new LibraryStore();
