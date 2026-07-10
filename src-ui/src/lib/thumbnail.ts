import { convertFileSrc } from "@tauri-apps/api/core";

/// Resolve a stored relative thumbnail path to a Tauri-loadable asset URL.
///
/// `path` is the relative thumbnail_path from the DB (e.g.
/// `.photovault/thumbs/ab/abc123_small.jpg`). We prepend the drive_root
/// before converting so the webview can load it via the asset protocol.
export function thumbUrl(driveRoot: string | null, path: string | null): string | null {
  if (!path) return null;
  if (!isSafeRelativeThumbPath(path)) return null;
  if (!driveRoot) return null;
  // Forward-slash join works on both platforms when paths use absolute
  // styles. driveRoot is already in the host's native form.
  const sep = driveRoot.endsWith("/") || driveRoot.endsWith("\\") ? "" : "/";
  return convertFileSrc(`${driveRoot}${sep}${path}`);
}

function isSafeRelativeThumbPath(path: string): boolean {
  if (path.startsWith("/") || path.startsWith("\\\\") || /^[a-zA-Z]:[\\/]/.test(path)) {
    return false;
  }
  if (path.includes("\\") || path.includes(":")) return false;
  return path
    .split("/")
    .every((part) => part.length > 0 && part !== "." && part !== "..");
}
