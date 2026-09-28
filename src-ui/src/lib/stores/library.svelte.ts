import { library } from "../api/library";
import { resetThumbnailRequests } from "../thumbnailRequest";
import { settings } from "../api/all";
import { commandErrorMessage } from "../api";
import { toasts } from "./toast.svelte";
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
  /// Resolves once the current refresh has loaded (or failed to load)
  /// the remembered drives — the boot fast-path waits on this to know
  /// which library to reopen.
  refreshDone: Promise<void> = Promise.resolve();

  private async syncCurrent(seq: number) {
    const cur: LibraryHandleDto | null = await library.current();
    if (seq !== this.seq) return;
    this.session = cur?.library_session_id ?? 0;
    this.isOpen = cur !== null && !cur.read_only;
    this.driveRoot = cur?.drive_root ?? null;
    this.photoCount = cur?.photo_count ?? 0;
    this.unsupportedSchema = cur?.schema_too_new ?? null;
  }

  refresh(): Promise<void> {
    this.refreshDone = this.runRefresh(++this.seq);
    return this.refreshDone;
  }

  private async runRefresh(seq: number) {
    this.loading = true;
    this.error = null;
    this.lastError = null;
    try {
      await this.syncCurrent(seq);
      if (seq !== this.seq) return;
      // Optional drive discovery must not disable manual folder selection.
      this.loading = false;
      // Drives and settings are independent — fetch them together so
      // the boot fast-path can start reopening the last library sooner.
      const [drives, remembered] = await Promise.all([
        library.listDrives(),
        settings
          .get()
          .then((s) => s.remembered_drives ?? [])
          .catch(() => []), // Settings is best-effort here; drive picker still works without.
      ]);
      if (seq !== this.seq) return;
      this.drives = drives;
      this.remembered = remembered;
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
      this.session = r.library_session_id;
      this.isOpen = !r.read_only;
      this.driveRoot = r.drive_root;
      this.photoCount = r.photo_count;
      this.unsupportedSchema = r.schema_too_new;
      // The catalog records which drive root it was created for; opening
      // it from somewhere else means face/album data may not match files.
      if (r.catalog_mismatch) {
        toasts.error(
          `This index was created for ${r.catalog_mismatch}, not for ${r.drive_root}. Photo data may not match.`,
        );
      }
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
