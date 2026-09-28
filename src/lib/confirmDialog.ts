/**
 * Native confirm via the Tauri dialog plugin (desktop), falling back to the
 * browser dialog when running outside the webview (vite preview / tests).
 * The one copy of this: row delete and the production commit (`TableView.tsx`),
 * widget delete (`MetricsBoard.tsx`) and `canLeaveSession` all use it. Every
 * caller wants the "warning" kind; message, title and (optionally) the button
 * labels differ.
 */
export async function confirmDialog(
  message: string,
  title: string,
  labels?: { ok: string; cancel: string },
): Promise<boolean> {
  try {
    const { confirm } = await import("@tauri-apps/plugin-dialog");
    return await confirm(message, {
      title,
      kind: "warning",
      ...(labels ? { okLabel: labels.ok, cancelLabel: labels.cancel } : {}),
    });
  } catch {
    return typeof window !== "undefined" ? window.confirm(message) : false;
  }
}
