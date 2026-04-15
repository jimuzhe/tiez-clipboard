use crate::app_state::{AppDataDir, EncryptionQueueState, SessionHistory};
use crate::database::{self, has_sensitive_tag, DbState};
use crate::error::{AppError, AppResult};
use crate::infrastructure::repository::clipboard_repo::ClipboardRepository;
use crate::infrastructure::repository::tag_repo::TagRepository;
use crate::services::encryption_queue::{EncryptionAction, EncryptionJob};
use serde_json;
use tauri::{AppHandle, Emitter, Manager, State};

fn truncate_chars_with_suffix(text: &str, max_chars: usize, suffix: &str) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let cut = text
        .char_indices()
        .nth(max_chars)
        .map(|(idx, _)| idx)
        .unwrap_or(text.len());
    let mut out = String::with_capacity(cut + suffix.len());
    out.push_str(&text[..cut]);
    out.push_str(suffix);
    out
}

#[tauri::command]
pub fn toggle_clipboard_pin(
    app_handle: AppHandle,
    state: State<'_, DbState>,
    session: State<'_, SessionHistory>,
    app_data_dir: State<'_, AppDataDir>,
    id: i64,
    is_pinned: bool,
) -> AppResult<i64> {
    let mut real_id = id;
    let mut entry_to_save = None;

    {
        let mut session_items = session.inner().0.lock().unwrap();
        if let Some(item) = session_items.iter_mut().find(|i| i.id == id) {
            item.is_pinned = is_pinned;
            if id < 0 && is_pinned {
                entry_to_save = Some(item.clone());
            }
        }
    }

    let conn = state.conn.lock().unwrap();

    if let Some(entry) = entry_to_save {
        let data_dir = app_data_dir.0.lock().unwrap().clone();
        if let Ok(new_id) = state.repo.save_with_conn(&conn, &entry, Some(&data_dir)) {
            real_id = new_id;
            if let Ok(deleted_ids) = state.repo.enforce_limit_with_conn(&conn, Some(&data_dir)) {
                for deleted_id in deleted_ids {
                    let _ = app_handle.emit("clipboard-removed", deleted_id);
                }
            }
            {
                let mut session_items = session.inner().0.lock().unwrap();
                if let Some(item) = session_items.iter_mut().find(|i| i.id == id) {
                    item.id = new_id;
                }
            }
        }
    }

    if real_id > 0 {
        state
            .repo
            .toggle_pin_with_conn(&conn, real_id, is_pinned)
            .map_err(AppError::from)?;
    }
    drop(conn);
    let _ = app_handle.emit("clipboard-changed", ());
    crate::services::cloud_sync::request_cloud_sync(app_handle);
    Ok(real_id)
}

#[tauri::command]
pub fn update_tags(
    app_handle: AppHandle,
    state: State<'_, DbState>,
    session: State<'_, SessionHistory>,
    app_data_dir: State<'_, AppDataDir>,
    id: i64,
    tags: Vec<String>,
) -> AppResult<i64> {
    if id < 0 {
        let mut session_items = session.inner().0.lock().unwrap();
        if let Some(index) = session_items.iter().position(|item| item.id == id) {
            let mut item = session_items[index].clone();
            item.tags = tags.clone();

            let data_dir = app_data_dir.0.lock().unwrap().clone();
            let new_id = state.repo.save(&item, Some(&data_dir))?;

            session_items[index].id = new_id;
            session_items[index].tags = tags;
            crate::services::cloud_sync::request_cloud_sync(app_handle);
            return Ok(new_id);
        }
        return Err(AppError::Validation("Item not found".to_string()));
    }

    let old_sensitive = {
        let conn = state.conn.lock().unwrap();
        let tags_json: Option<String> = conn
            .query_row(
                "SELECT tags FROM clipboard_history WHERE id = ?",
                [id],
                |row| row.get(0),
            )
            .ok();
        let prev_tags: Vec<String> = tags_json
            .as_deref()
            .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
            .unwrap_or_default();
        has_sensitive_tag(&prev_tags)
    };

    let new_sensitive = has_sensitive_tag(&tags);
    state
        .tag_repo
        .update_entry_tags(id, tags)
        .map_err(AppError::from)?;
    if old_sensitive != new_sensitive {
        let queue = app_handle.state::<EncryptionQueueState>();
        let action = if new_sensitive {
            EncryptionAction::Encrypt
        } else {
            EncryptionAction::Decrypt
        };
        queue.0.enqueue(EncryptionJob { id, action });
    }
    crate::services::cloud_sync::request_cloud_sync(app_handle);
    Ok(id)
}

#[tauri::command]
pub async fn add_manual_item(
    app_handle: AppHandle,
    state: State<'_, DbState>,
    content: String,
    content_type: String,
    tags: Vec<String>,
) -> AppResult<i64> {
    let preview = truncate_chars_with_suffix(&content, 200, "...");

    let entry = database::ClipboardEntry {
        id: 0,
        content_type,
        content,
        html_content: None,
        source_app: "Manual".to_string(),
        source_app_path: None,
        timestamp: chrono::Utc::now().timestamp_millis(),
        preview,
        is_pinned: false,
        tags,
        use_count: 0,
        is_external: false,
        pinned_order: 0,
        file_preview_exists: true,
    };

    let app_data_dir = app_handle.state::<AppDataDir>();
    let data_dir = app_data_dir.0.lock().unwrap().clone();
    let new_id = state.repo.save(&entry, Some(&data_dir))?;
    let _ = app_handle.emit("clipboard-changed", ());
    crate::services::cloud_sync::request_cloud_sync(app_handle);
    Ok(new_id)
}

#[tauri::command]
pub async fn update_item_content(
    app_handle: AppHandle,
    state: State<'_, DbState>,
    session: State<'_, SessionHistory>,
    id: i64,
    new_content: String,
) -> AppResult<()> {
    let preview = truncate_chars_with_suffix(&new_content, 500, "...");

    {
        let mut session_items = session.inner().0.lock().unwrap();
        if let Some(item) = session_items.iter_mut().find(|i| i.id == id) {
            item.content = new_content.clone();
            item.preview = preview.clone();
        }
    }

    state
        .repo
        .update_entry_content(id, &new_content, &preview)
        .map_err(AppError::from)?;
    let _ = app_handle.emit("clipboard-changed", ());
    crate::services::cloud_sync::request_cloud_sync(app_handle);
    Ok(())
}
