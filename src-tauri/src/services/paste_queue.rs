use crate::app_state::{PasteQueue, SessionHistory, AppDataDir};
use crate::database::DbState;
use crate::infrastructure::repository::clipboard_repo::ClipboardRepository;
use crate::infrastructure::repository::settings_repo::SettingsRepository;
use arboard::Clipboard;
use tauri::{Emitter, Manager, State};
use crate::error::AppResult;

#[allow(dead_code)]
const WM_PASTE: u32 = 0x0302;

#[tauri::command]
pub fn get_paste_queue(state: State<'_, PasteQueue>) -> Vec<i64> {
    state
        .inner()
        .0
        .lock()
        .unwrap()
        .items
        .iter()
        .copied()
        .collect()
}

#[tauri::command]
pub fn set_paste_queue(
    app_handle: tauri::AppHandle,
    state: State<'_, PasteQueue>,
    item_ids: Vec<i64>,
) -> AppResult<()> {
    if item_ids.is_empty() {
        state.inner().0.lock().unwrap().items.clear();
        return Ok(());
    }

    let mut queue = state.inner().0.lock().unwrap();
    queue.items.clear();
    for id in item_ids {
        queue.items.push_back(id);
    }
    queue.last_action_was_paste = false;
    queue.last_pasted_content = None;
    drop(queue);

    // Automatically prepare the first item
    prepare_next_paste_item(&app_handle);

    Ok(())
}

fn prepare_next_paste_item(app_handle: &tauri::AppHandle) {
    let state = app_handle.state::<PasteQueue>();
    let db_state = app_handle.state::<DbState>();
    let session = app_handle.state::<SessionHistory>();

    let next_id = {
        let queue = state.inner().0.lock().unwrap();
        queue.items.front().copied()
    };

    if let Some(id) = next_id {
        // Find content
        let content_opt = if id < 0 {
            let s = session.inner().0.lock().unwrap();
            s.iter().find(|i| i.id == id).map(|i| i.content.clone())
        } else {
            db_state.repo.get_entry_content(id).unwrap_or(None)
        };

        if let Some(_) = content_opt {
            // Logic to prepare next item
        }
    } else {
        let _ = app_handle.emit("queue-finished", ());
    }
}

#[tauri::command]
pub fn paste_next_step(app_handle: tauri::AppHandle) {
    let state = app_handle.state::<PasteQueue>();
    let db_state = app_handle.state::<DbState>();
    let session = app_handle.state::<SessionHistory>();
    let _settings = app_handle.state::<crate::app_state::SettingsState>();

    // 1. Pop item from queue (Scope the lock)
    let id_opt = {
        let mut queue = state.inner().0.lock().unwrap();
        queue.last_action_was_paste = true;
        queue.items.pop_front()
    };

    if let Some(id) = id_opt {
        // 2. Get Content (DB Lock acquired here, safe because Queue lock is released)
        let content_opt = if id < 0 {
            let s = session.inner().0.lock().unwrap();
            s.iter()
                .find(|i| i.id == id)
                .map(|i| (i.content.clone(), i.content_type.clone()))
        } else {
            db_state.repo.get_entry_content_full(id).unwrap_or(None)
        };

        if let Some((content, c_type)) = content_opt {
            // CRITICAL: Update last_pasted_content BEFORE modifying clipboard to prevent race condition
            // where the monitor sees the change before we've marked it as an echo.
            {
                let mut queue = state.inner().0.lock().unwrap();
                queue.last_pasted_content = Some(content.clone());
            }

            if c_type == "text" || c_type == "code" || c_type == "url" || c_type == "rich_text" {
                if let Ok(mut clipboard) = Clipboard::new() {
                    let _ = clipboard.set_text(content.clone());
                }
            }

            // Get paste method from settings
            let paste_method = {
                let db_state = app_handle.state::<DbState>(); 
                db_state.settings_repo.get("app.paste_method").ok().flatten().unwrap_or_else(|| "ctrl_v".to_string())
            };

            // Send paste keystroke using centralized logic
            // We measure Alt state BEFORE sending keys to know if we should restore it
            let alt_was_down = {
                #[cfg(target_os = "windows")]
                unsafe {
                    (windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(0x12) as i16) < 0
                }
                #[cfg(not(target_os = "windows"))]
                false
            };

            crate::services::clipboard_ops::send_paste_keystroke(
                &paste_method,
                Some(&content),
                Some(&c_type)
            );

            // Settle time
            std::thread::sleep(std::time::Duration::from_millis(20));

            // Restore Alt state if it was physically held down
            // This is crucial for sequential paste flow (holding Alt while tapping V)
            if alt_was_down {
                #[cfg(target_os = "windows")]
                unsafe {
                    use windows::Win32::UI::Input::KeyboardAndMouse::{
                        VK_MENU, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
                    };
                    let alt_restore = INPUT {
                        r#type: INPUT_KEYBOARD,
                        Anonymous: INPUT_0 {
                            ki: KEYBDINPUT {
                                wVk: VK_MENU,
                                dwFlags: windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(0),
                                ..Default::default()
                            },
                        },
                    };
                    SendInput(&[alt_restore], std::mem::size_of::<INPUT>() as i32);
                }
            }

            // Perform deletion if delete_after_paste is enabled
            let delete_after_paste = {
                let settings_state = app_handle.state::<crate::app_state::SettingsState>();
                settings_state.delete_after_paste.load(std::sync::atomic::Ordering::Relaxed)
            };

            if delete_after_paste {
                // Remove from session history first
                {
                    let mut s = session.inner().0.lock().unwrap();
                    if let Some(pos) = s.iter().position(|i| i.id == id) {
                        s.remove(pos);
                    }
                }

                if id > 0 {
                    // Persistent item: Delete from DB (and cleanup file)
                    let app_data = app_handle.state::<AppDataDir>();
                    let data_dir = app_data.0.lock().unwrap();
                    if db_state.repo.delete(id, Some(&data_dir)).is_ok() {
                        let _ = app_handle.emit("clipboard-removed", id);
                    }
                } else {
                    // Session item only
                    let _ = app_handle.emit("clipboard-removed", id);
                }
            } else {
                // If not deleting, increment use count
                if id > 0 {
                    let _ = db_state.repo.increment_use_count(id);
                }
            }

            // Emit event to update UI queue state
            let _ = app_handle.emit("queue-item-pasted", id);
        }
    } else {
        let _ = app_handle.emit("queue-finished", ());
    }
}
