/// Undo for a decision about a face ("yes", "no", "not a face", a move).
///
/// The backend keeps what each decision changed under a token; this offers
/// the Undo toast and, once the face is back as it was, lets the screen put
/// it back too.

import { commandErrorMessage } from "./api";
import { people } from "./api/all";
import { toasts } from "./stores/toast.svelte";

export function offerFaceUndo(token: string, message: string, restored?: () => void) {
  toasts.undoable(message, async () => {
    try {
      await people.faceUndo(token);
      restored?.();
    } catch (e) {
      toasts.error(commandErrorMessage(e));
    }
  });
}
