/// Lightweight toast / undoable-action notifications.
///
/// Three flavours:
///   - info / success / error: text-only, auto-dismiss in ~3.5s.
///   - undoable: text + Undo button; auto-dismiss in 6s; on click,
///     `onUndo` runs and the toast disappears immediately.

export type ToastKind = "info" | "success" | "error" | "undo";

export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
  /// Undo callback — only used when kind === "undo".
  onUndo?: () => void | Promise<void>;
  /// Internal: timestamp when this should auto-dismiss.
  expiresAt: number;
}

class ToastStore {
  list = $state<Toast[]>([]);
  private nextId = 1;

  show(opts: { kind?: ToastKind; message: string; durationMs?: number; onUndo?: () => void | Promise<void> }) {
    const kind = opts.kind ?? "info";
    const duration = opts.durationMs ?? (kind === "undo" ? 6000 : 3500);
    const id = this.nextId++;
    const t: Toast = {
      id,
      kind,
      message: opts.message,
      onUndo: opts.onUndo,
      expiresAt: Date.now() + duration,
    };
    this.list = [...this.list, t];
    setTimeout(() => this.dismiss(id), duration);
    return id;
  }

  /// Convenience: show an undoable action. The caller has already
  /// performed the action; the toast offers a way to roll it back.
  undoable(message: string, onUndo: () => void | Promise<void>) {
    return this.show({ kind: "undo", message, onUndo });
  }

  success(message: string) { this.show({ kind: "success", message }); }
  error(message: string)   { this.show({ kind: "error",   message }); }
  info(message: string)    { this.show({ kind: "info",    message }); }

  dismiss(id: number) {
    this.list = this.list.filter((t) => t.id !== id);
  }
}

export const toasts = new ToastStore();
