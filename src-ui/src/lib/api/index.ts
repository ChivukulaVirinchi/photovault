import { invoke } from "@tauri-apps/api/core";
import type { CommandError } from "./types";

/// Generic typed wrapper around Tauri's `invoke`.
///
/// All photovault commands take a single named-struct argument on the wire.
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
