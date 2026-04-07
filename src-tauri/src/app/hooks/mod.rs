use tauri::AppHandle;

#[tauri::command]
pub fn set_recording_mode(_app_handle: AppHandle, enabled: bool) {
    // Frontend passes enabled=true while recording hotkeys.
    // During hotkey recording we should pause clipboard monitoring.
    crate::global_state::CLIPBOARD_MONITOR_PAUSED
        .store(enabled, std::sync::atomic::Ordering::Relaxed);
}
