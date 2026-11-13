import { invoke } from "@tauri-apps/api/core";
import type { CommandError } from "./types";
import { devMode } from "../stores/devMode.svelte";

/// What failed, in words: the engine's own message is for developers.
const PLAIN_FAILURE: Record<string, string> = {
  database: "Smriti couldn't read or save that in the library.",
  io: "Smriti couldn't reach that file or folder. Is the drive still connected?",
  network: "Smriti couldn't reach the internet.",
  internal: "Something went wrong.",
};

/// Generic typed wrapper around Tauri's `invoke`.
///
/// All Smriti commands take a single named-struct argument on the wire.
/// Tauri delivers it as `{ args: <payload> }` to the Rust handler. Errors
/// are pre-shaped `CommandError` discriminated unions (see types.ts).
export async function call<TReturn>(
  name: string,
  args?: unknown,
): Promise<TReturn> {
  try {
    return (await invoke(name, args ? { args } : undefined)) as TReturn;
  } catch (e) {
    throw e as CommandError;
  }
}

export function commandErrorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (!error || typeof error !== "object") return String(error);

  const e = error as Partial<CommandError>;
  switch (e.kind) {
    case "not_found":
      return `That ${(e.entity ?? "item").replace(/_/g, " ")} is no longer here.`;
    case "validation":
      return e.reason ?? "Invalid input";
    case "library_closed":
      return "Open a library first.";
    case "drive_not_mounted":
      return `Drive not mounted: ${e.path ?? ""}`.trim();
    case "ml_unavailable":
    case "conflict":
      return e.reason ?? "Operation unavailable";
    case "cancelled":
      return "Operation cancelled.";
    case "schema_too_new":
      return "This library was made with a newer version of Smriti. Update Smriti from Settings, then open it again.";
    case "database":
    case "io":
    case "network":
    case "internal": {
      const plain = PLAIN_FAILURE[e.kind];
      return devMode.enabled && e.message ? `${plain} (${e.message})` : plain;
    }
    default:
      if (typeof (error as { message?: unknown }).message === "string") {
        return (error as { message: string }).message;
      }
      if (typeof (error as { reason?: unknown }).reason === "string") {
        return (error as { reason: string }).reason;
      }
      try {
        return JSON.stringify(error);
      } catch {
        return String(error);
      }
  }
}
