export type ToastKind = 'success' | 'error' | 'info';

export type Toast = {
  id: number;
  kind: ToastKind;
  message: string;
};

let nextId = 1;

export const toasts = $state<{ items: Toast[] }>({ items: [] });

export function pushToast(kind: ToastKind, message: string, ttlMs = 2600): number {
  const id = nextId++;
  toasts.items = [...toasts.items, { id, kind, message }];
  if (ttlMs > 0) setTimeout(() => dismissToast(id), ttlMs);
  return id;
}

export function dismissToast(id: number): void {
  toasts.items = toasts.items.filter((toast) => toast.id !== id);
}

export const toastSuccess = (message: string) => pushToast('success', message);
export const toastError = (message: string) => pushToast('error', message, 4000);
export const toastInfo = (message: string) => pushToast('info', message);

/** Show "Loading…" while `work` runs, then a success or error toast. */
export async function withToast<T>(
  pending: string,
  work: () => Promise<T>,
  done?: (result: T) => string
): Promise<T | null> {
  const id = pushToast('info', pending, 0);
  try {
    const result = await work();
    dismissToast(id);
    toastSuccess(done ? done(result) : 'Done!');
    return result;
  } catch (error) {
    dismissToast(id);
    toastError(error instanceof Error ? error.message : String(error));
    return null;
  }
}
