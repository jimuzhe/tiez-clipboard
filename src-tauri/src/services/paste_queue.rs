use crate::app_state::{AppDataDir, PasteQueue, SessionHistory};
use crate::database::DbState;
use crate::error::AppResult;
use crate::infrastructure::repository::clipboard_repo::ClipboardRepository;
use crate::infrastructure::repository::settings_repo::SettingsRepository;
use tauri::{Emitter, Manager, State};

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
        let mut queue = state.inner().0.lock().unwrap();
        queue.items.clear();
        queue.last_action_was_paste = false;
        queue.last_pasted_content = None;
        queue.last_pasted_fingerprint = None;
        queue.last_paste_timestamp_ms = 0;
        return Ok(());
    }

    let mut queue = state.inner().0.lock().unwrap();
    queue.items.clear();
    for id in item_ids {
        queue.items.push_back(id);
    }
    queue.last_action_was_paste = false;
    queue.last_pasted_content = None;
    queue.last_pasted_fingerprint = None;
    queue.last_paste_timestamp_ms = 0;
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
pub async fn paste_next_step(app_handle: tauri::AppHandle) {
    let state = app_handle.state::<PasteQueue>();
    let db_state = app_handle.state::<DbState>();
    let session = app_handle.state::<SessionHistory>();

    // 1. Pop item from queue (Scope the lock)
    let id_opt = {
        let mut queue = state.inner().0.lock().unwrap();
        queue.items.pop_front()
    };

    if let Some(id) = id_opt {
        // 2. Get Content (DB Lock acquired here, safe because Queue lock is released)
        let content_opt = if id < 0 {
            let s = session.inner().0.lock().unwrap();
            s.iter().find(|i| i.id == id).map(|i| {
                (
                    i.content.clone(),
                    i.content_type.clone(),
                    i.html_content.clone(),
                )
            })
        } else {
            db_state
                .repo
                .get_entry_content_with_html(id)
                .unwrap_or(None)
        };

        if let Some((content, c_type, html_content)) = content_opt {
            crate::services::clipboard_ops::remember_recent_paste(
                &app_handle,
                &content,
                &c_type,
                html_content.as_deref(),
            );

            if let Err(err) = crate::services::clipboard_ops::prepare_clipboard_payload(
                &content,
                &c_type,
                html_content.as_deref(),
                c_type == "rich_text" && html_content.as_deref().is_some(),
            )
            .await
            {
                eprintln!(
                    "[ERROR] Failed to prepare clipboard payload for sequential paste: {err}"
                );
                let _ = app_handle.emit("queue-item-pasted", id);
                return;
            }

            // Get paste method from settings
            let paste_method = {
                let db_state = app_handle.state::<DbState>();
                db_state
                    .settings_repo
                    .get("app.paste_method")
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "shift_insert".to_string())
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
                Some(&c_type),
            );

            // Settle time
            std::thread::sleep(std::time::Duration::from_millis(20));

            // Restore Alt state if it was physically held down
            // This is crucial for sequential paste flow (holding Alt while tapping V)
            if alt_was_down {
                #[cfg(target_os = "windows")]
                unsafe {
                    use windows::Win32::UI::Input::KeyboardAndMouse::{
                        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, VK_MENU,
                    };
                    let alt_restore = INPUT {
                        r#type: INPUT_KEYBOARD,
                        Anonymous: INPUT_0 {
                            ki: KEYBDINPUT {
                                wVk: VK_MENU,
                                dwFlags:
                                    windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(
                                        0,
                                    ),
                                ..Default::default()
                            },
                        },
                    };
                    SendInput(&[alt_restore], std::mem::size_of::<INPUT>() as i32);
                    println!("[DEBUG] Restored Alt key state for continuous sequential paste");
                }
            }

            // Perform deletion if delete_after_paste is enabled
            let mut actual_delete = {
                let settings_state = app_handle.state::<crate::app_state::SettingsState>();
                settings_state
                    .delete_after_paste
                    .load(std::sync::atomic::Ordering::Relaxed)
            };

            if actual_delete && id > 0 {
                if let Ok(Some(entry)) = db_state.repo.get_entry_by_id(id) {
                    if entry.is_pinned || !entry.tags.is_empty() {
                        actual_delete = false;
                    }
                }
            }

            if actual_delete {
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
