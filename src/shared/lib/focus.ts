import { invoke } from "@tauri-apps/api/core";

export async function focusClipboardWindow(): Promise<void> {
  await invoke("focus_clipboard_window");
}

export async function restoreLastFocus(): Promise<void> {
  await invoke("restore_last_focus");
}

export async function focusWindowImmediately(): Promise<void> {
  await focusClipboardWindow();
}

export async function restoreFocus(): Promise<void> {
  await restoreLastFocus();
}
