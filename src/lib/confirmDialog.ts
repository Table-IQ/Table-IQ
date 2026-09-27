/**
 * Native confirm via the Tauri dialog plugin (desktop), falling back to the
 * browser dialog when running outside the webview (vite preview / tests).
 * Shared by `TableView.tsx` (row delete) and `MetricsBoard.tsx` (widget
 * delete) — both call sites want the same "warning" dialog kind, only the
 * message/title differ.
 */
export async function confirmDialog(message: string, title: string): Promise<boolean> {
  try {
    const { confirm } = await import("@tauri-apps/plugin-dialog");
    return await confirm(message, { title, kind: "warning" });
  } catch {
    return typeof window !== "undefined" ? window.confirm(message) : false;
  }
}
