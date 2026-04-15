use crate::app_state::{AppDataDir, PasteQueue, PasteQueueState, SessionHistory};
use crate::database::DbState;
use crate::error::AppResult;
use crate::infrastructure::repository::clipboard_repo::ClipboardRepository;
use crate::infrastructure::repository::settings_repo::SettingsRepository;
use tauri::{Emitter, Manager, State};

#[allow(dead_code)]
const WM_PASTE: u32 = 0x0302;

fn reset_paste_tracking(queue: &mut PasteQueueState) {
    queue.last_action_was_paste = false;
    queue.last_pasted_content = None;
}

fn clear_paste_tracking(state: &State<'_, PasteQueue>) {
    let mut queue = state.inner().0.lock().unwrap();
    reset_paste_tracking(&mut queue);
}

fn pop_front_if_matches(state: &State<'_, PasteQueue>, id: i64) -> bool {
    let mut queue = state.inner().0.lock().unwrap();
    if queue.items.front().copied() == Some(id) {
        queue.items.pop_front();
    }
    queue.items.is_empty()
}

fn emit_queue_finished(app_handle: &tauri::AppHandle, state: &State<'_, PasteQueue>) {
    clear_paste_tracking(state);
    let _ = app_handle.emit("queue-finished", ());
}

fn emit_queue_finished_if_empty(app_handle: &tauri::AppHandle, state: &State<'_, PasteQueue>) {
    let should_emit = {
        let mut queue = state.inner().0.lock().unwrap();
        if queue.items.is_empty() {
            reset_paste_tracking(&mut queue);
            true
        } else {
            false
        }
    };

    if should_emit {
        let _ = app_handle.emit("queue-finished", ());
    }
}

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
        {
            let mut queue = state.inner().0.lock().unwrap();
            queue.items.clear();
            reset_paste_tracking(&mut queue);
        }
        let _ = app_handle.emit("queue-finished", ());
        return Ok(());
    }

    let mut queue = state.inner().0.lock().unwrap();
    queue.items.clear();
    for id in item_ids {
        queue.items.push_back(id);
    }
    reset_paste_tracking(&mut queue);
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
        emit_queue_finished(app_handle, &state);
    }
}

#[tauri::command]
pub fn paste_next_step(app_handle: tauri::AppHandle) {
    let state = app_handle.state::<PasteQueue>();
    let db_state = app_handle.state::<DbState>();
    let session = app_handle.state::<SessionHistory>();
    let _settings = app_handle.state::<crate::app_state::SettingsState>();

    // Peek first so a failed paste keystroke does not lose the queued item.
    let id_opt = {
        let queue = state.inner().0.lock().unwrap();
        queue.items.front().copied()
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
            // CRITICAL: Update last_pasted_content BEFORE modifying clipboard to prevent race condition
            // where the monitor sees the change before we've marked it as an echo.
            {
                let mut queue = state.inner().0.lock().unwrap();
                queue.last_action_was_paste = true;
                queue.last_pasted_content = Some(content.clone());
            }

            let paste_with_format = c_type == "rich_text" && html_content.as_deref().is_some();

            if crate::services::clipboard_ops::write_content_to_system_clipboard(
                &content,
                &c_type,
                html_content.as_deref(),
                paste_with_format,
            )
            .is_err()
            {
                clear_paste_tracking(&state);
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

            if crate::services::clipboard_ops::send_paste_keystroke(
                &paste_method,
                Some(&content),
                Some(&c_type),
            )
            .is_err()
            {
                clear_paste_tracking(&state);
                return;
            }

            let queue_became_empty = pop_front_if_matches(&state, id);

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
            let delete_after_paste = {
                let settings_state = app_handle.state::<crate::app_state::SettingsState>();
                settings_state
                    .delete_after_paste
                    .load(std::sync::atomic::Ordering::Relaxed)
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
            if queue_became_empty {
                emit_queue_finished(&app_handle, &state);
            }
        } else {
            let queue_became_empty = pop_front_if_matches(&state, id);
            clear_paste_tracking(&state);
            if queue_became_empty {
                emit_queue_finished_if_empty(&app_handle, &state);
            }
        }
    } else {
        emit_queue_finished(&app_handle, &state);
    }
}
