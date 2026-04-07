use crate::app_state::{AppDataDir, PasteQueue, SessionHistory};
use crate::database::DbState;
use crate::error::AppResult;
use crate::infrastructure::repository::clipboard_repo::ClipboardRepository;
use tauri::{Emitter, Manager, State};

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
    // Run on async runtime so we can await focus-restoration delays.
    tauri::async_runtime::spawn(async move {
        paste_next_step_inner(app_handle).await;
    });
}

async fn paste_next_step_inner(app_handle: tauri::AppHandle) {
    let state = app_handle.state::<PasteQueue>();
    let db_state = app_handle.state::<DbState>();
    let session = app_handle.state::<SessionHistory>();

    // Check if TieZ window is currently visible.
    // If the window is hidden (user pressed hotkey while TieZ was in the background),
    // focus is already in the target app - we must NOT do any focus restoration,
    // as that would steal focus FROM the target app to a wrong app.
    let window_was_visible = app_handle
        .get_webview_window("main")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);

    // 1. Pop item from queue (Scope the lock)
    let id_opt = {
        let mut queue = state.inner().0.lock().unwrap();
        queue.last_action_was_paste = true;
        queue.items.pop_front()
    };

    if let Some(id) = id_opt {
        // 2. Get Content (DB Lock acquired here, safe because Queue lock is released)
        let entry_opt = if id < 0 {
            let s = session.inner().0.lock().unwrap();
            s.iter().find(|i| i.id == id).cloned()
        } else {
            db_state.repo.get_entry_by_id(id).unwrap_or(None)
        };

        if let Some(entry) = entry_opt {
            let content = entry.content;
            let c_type = entry.content_type;
            let html_content = entry.html_content;
            let is_pinned = entry.is_pinned;
            let has_tags = !entry.tags.is_empty();
            // CRITICAL: Update last_pasted_content BEFORE modifying clipboard to prevent race condition
            // where the monitor sees the change before we've marked it as an echo.
            {
                let mut queue = state.inner().0.lock().unwrap();
                queue.last_pasted_content = Some(content.clone());
            }

            // 3. Write content to system clipboard using the same payload logic as normal paste.
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

            // 4. Focus management before paste keystroke.
            //    - If TieZ window WAS open (user had it open and used it), we need to
            //      restore focus to the previous app, because clicking in TieZ stole it.
            //    - If TieZ window was HIDDEN (pure background hotkey press), focus is
            //      already in the target app. Do NOT restore focus as that would move
            //      focus to a stale/wrong app and break the paste.
            if window_was_visible {
                let mut reactivated = false;
                let prev_pid = crate::global_state::LAST_ACTIVE_APP_PID
                    .load(std::sync::atomic::Ordering::Relaxed);
                if prev_pid != 0 {
                    reactivated = crate::infrastructure::macos_api::apps::activate_app_by_pid(
                        prev_pid as i32,
                    );
                }
                if !reactivated {
                    let prev_app = crate::global_state::get_last_active_app_name();
                    if !prev_app.is_empty() {
                        crate::infrastructure::macos_api::apps::activate_app_by_name(&prev_app);
                    }
                }
                // Wait for focus transfer to complete before sending keystroke
                // Reduced from 150ms to 60ms for native activation.
                tokio::time::sleep(std::time::Duration::from_millis(60)).await;
            } else {
                // When triggered via global hotkey with the window hidden,
                // the event is now fired on ShortcutState::Released.
                // However, the OS shortcut system might still have slight jitter.
                // A tiny 20ms settle helps ensure the physical key state is completely cleared.
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }

            // 6. Send paste keystroke
            crate::services::clipboard_ops::send_paste_keystroke(Some(&content), Some(&c_type));

            // Settle time
            std::thread::sleep(std::time::Duration::from_millis(20));

            // 7. Perform deletion if delete_after_paste is enabled
            let delete_after_paste = {
                let settings_state = app_handle.state::<crate::app_state::SettingsState>();
                settings_state
                    .delete_after_paste
                    .load(std::sync::atomic::Ordering::Relaxed)
            };

            if delete_after_paste && !is_pinned && !has_tags {
                {
                    let mut s = session.inner().0.lock().unwrap();
                    if let Some(pos) = s.iter().position(|i| i.id == id) {
                        s.remove(pos);
                    }
                }

                if id > 0 {
                    let app_data = app_handle.state::<AppDataDir>();
                    let data_dir = app_data.0.lock().unwrap();
                    if db_state.repo.delete(id, Some(&data_dir)).is_ok() {
                        let _ = app_handle.emit("clipboard-removed", id);
                    }
                } else {
                    let _ = app_handle.emit("clipboard-removed", id);
                }
            } else {
                if id > 0 {
                    let _ = db_state.repo.increment_use_count(id);
                }
            }

            let _ = app_handle.emit("queue-item-pasted", id);
        }
    } else {
        let _ = app_handle.emit("queue-finished", ());
    }
}
