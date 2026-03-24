use tauri::State;
use crate::app_state::{SessionHistory, AppDataDir};
use crate::database::DbState;
use crate::infrastructure::repository::clipboard_repo::ClipboardRepository;
use crate::infrastructure::repository::tag_repo::TagRepository;
use crate::domain::models::ClipboardEntry;
use crate::error::{AppResult, AppError};
use crate::services::clipboard::truncate_html_for_preview;

const UI_CONTENT_LIMIT: usize = 2000;
const UI_PREVIEW_LIMIT: usize = 500;
const TAG_CONTENT_LIMIT: usize = 50000;
const MAX_HISTORY_LIMIT: i32 = 150;
const MIN_HISTORY_LIMIT: i32 = 1;

fn is_text_like_content_type(content_type: &str) -> bool {
    matches!(content_type, "text" | "code" | "url" | "rich_text")
}

fn truncate_with_suffix(input: &str, limit: usize, suffix: &str) -> String {
    let mut chars = input.chars();
    if chars.by_ref().nth(limit).is_some() {
        let head: String = input.chars().take(limit).collect();
        format!("{head}{suffix}")
    } else {
        input.to_string()
    }
}

fn normalize_entries_for_ui(entries: &mut [ClipboardEntry]) {
    for item in entries {
        if is_text_like_content_type(&item.content_type) {
            item.content = truncate_with_suffix(&item.content, UI_CONTENT_LIMIT, "... [Truncated for speed]");
            item.preview = truncate_with_suffix(&item.content, UI_PREVIEW_LIMIT.saturating_sub(3), "...");
        }

        if let Some(ref html) = item.html_content {
            if html.chars().count() > 5000 {
                item.html_content = truncate_html_for_preview(html);
            }
        }
    }
}

fn normalize_limit(limit: i32) -> i32 {
    limit.clamp(MIN_HISTORY_LIMIT, MAX_HISTORY_LIMIT)
}

#[tauri::command]
pub fn get_clipboard_history(
    state: State<'_, DbState>,
    session: State<'_, SessionHistory>,
    limit: i32,
    offset: i32,
    content_type: Option<String>,
) -> AppResult<Vec<ClipboardEntry>> {
    let limit = normalize_limit(limit);
    // 1. Get history from repository
    let mut history = state
        .repo
        .get_history(limit, offset, content_type.as_deref())?;
    
    // 2. Add session history items (non-persisted) ONLY on the first page
    if offset == 0 {
        let session_items = session.inner().0.lock().unwrap();
        for item in session_items.iter().rev() {
            if let Some(ct) = content_type.as_deref() {
                if item.content_type != ct {
                    continue;
                }
            }
            // Avoid duplicates: if item is already in DB, it will have id > 0
            if !history.iter().any(|h| h.id == item.id && item.id != 0) {
                history.push(item.clone());
            }
        }
    }
    
    // 3. Apply stable sorting: Pinned -> Pinned Order -> Timestamp -> ID
    // This MUST match the repository's logic to maintain pagination stability
    history.sort_by(|a, b| {
        b.is_pinned.cmp(&a.is_pinned)
            .then_with(|| b.pinned_order.cmp(&a.pinned_order))
            .then_with(|| b.timestamp.cmp(&a.timestamp))
            .then_with(|| b.id.cmp(&a.id))
    });
    
    // 4. Truncate to limit
    if history.len() > limit as usize {
        history.truncate(limit as usize);
    }
    
    normalize_entries_for_ui(&mut history);
    
    Ok(history)
}

#[tauri::command]
pub fn search_clipboard_history(
    state: State<'_, DbState>,
    session: State<'_, SessionHistory>,
    search_term: String,
    limit: i32,
) -> AppResult<Vec<ClipboardEntry>> {
    let limit = normalize_limit(limit);
    let mut history = state.repo.search(&search_term, limit)?;

    let term = search_term.to_lowercase();
    let session_items = session.inner().0.lock().unwrap();
    for item in session_items.iter().rev() {
        let matches = item.content.to_lowercase().contains(&term) || 
                      item.source_app.to_lowercase().contains(&term) ||
                      item.tags.iter().any(|t| t.to_lowercase().contains(&term));
        
        if matches {
            if !history.iter().any(|h| h.id == item.id && item.id != 0) {
                history.push(item.clone());
            }
        }
    }

    history.sort_by(|a, b| b.timestamp.cmp(&a.timestamp).then_with(|| b.id.cmp(&a.id)));
    if history.len() > limit as usize {
        history.truncate(limit as usize);
    }

    normalize_entries_for_ui(&mut history);

    Ok(history)
}

#[tauri::command]
pub fn delete_clipboard_entry(
    state: State<'_, DbState>,
    session: State<'_, SessionHistory>,
    app_data: State<'_, AppDataDir>,
    id: i64,
) -> AppResult<()> {
    {
        let mut session_items = session.inner().0.lock().unwrap();
        session_items.retain(|item| item.id != id);
    }
    
    if id > 0 {
        let data_dir = app_data.0.lock().unwrap();
        state.repo.delete(id, Some(&data_dir))?;
    }
    Ok(())
}

#[tauri::command]
pub fn clear_clipboard_history(
    state: State<'_, DbState>,
    session: State<'_, SessionHistory>,
    app_data: State<'_, AppDataDir>
) -> AppResult<()> {
    {
        let mut session_items = session.inner().0.lock().unwrap();
        session_items.retain(|item| item.is_pinned || !item.tags.is_empty());
    }
    let data_dir = app_data.0.lock().unwrap();
    state.repo.clear(Some(&data_dir)).map_err(AppError::from)
}

#[tauri::command]
pub fn get_tag_items(state: State<'_, DbState>, tag: String) -> AppResult<Vec<ClipboardEntry>> {
    let mut history = state.tag_repo.get_entries_by_tag(&tag).map_err(AppError::from)?;
    
    for item in &mut history {
        if is_text_like_content_type(&item.content_type) {
            item.content = truncate_with_suffix(&item.content, TAG_CONTENT_LIMIT, "... [Content Truncated]");
        }
    }
    
    Ok(history)
}

#[tauri::command]
pub fn get_all_tags_info(state: State<'_, DbState>) -> AppResult<std::collections::HashMap<String, i32>> {
    state.tag_repo.get_all_with_counts().map_err(AppError::from)
}

#[tauri::command]
pub fn rename_tag_globally(state: State<'_, DbState>, session: State<'_, SessionHistory>, old_name: String, new_name: String) -> AppResult<()> {
    {
        let mut session_items = session.inner().0.lock().unwrap();
        for item in session_items.iter_mut() {
            for tag in item.tags.iter_mut() {
                if *tag == old_name {
                    *tag = new_name.clone();
                }
            }
            item.tags.sort();
            item.tags.dedup();
        }
    }
    
    state.tag_repo.rename(&old_name, &new_name).map_err(AppError::from)
}

#[tauri::command]
pub fn delete_tag_from_all(
    state: State<'_, DbState>,
    session: State<'_, SessionHistory>,
    app_data: State<'_, AppDataDir>,
    tag_name: String,
) -> AppResult<()> {
    {
        let mut session_items = session.inner().0.lock().unwrap();
        session_items.retain(|item| !item.tags.contains(&tag_name));
    }
    
    let data_dir = app_data.0.lock().unwrap();
    state.tag_repo.delete_globally(&tag_name, Some(&data_dir)).map_err(AppError::from)
}

#[tauri::command]
pub fn create_new_tag(state: State<'_, DbState>, tag_name: String) -> AppResult<()> {
    state.tag_repo.create(&tag_name).map_err(AppError::from)
}

#[tauri::command]
pub fn get_clipboard_content(
    state: State<'_, DbState>,
    session: State<'_, SessionHistory>,
    id: i64,
) -> AppResult<String> {
    {
        let session_items = session.inner().0.lock().unwrap();
        if let Some(item) = session_items.iter().find(|i| i.id == id) {
            return Ok(item.content.clone());
        }
    }

    state.repo.get_entry_content(id).map_err(AppError::from)?
        .ok_or_else(|| AppError::Validation("Entry not found".to_string()))
}

#[tauri::command]
pub fn update_pinned_order(
    state: State<'_, DbState>,
    orders: Vec<(i64, i64)>,
) -> AppResult<()> {
    state.repo.update_pinned_order(orders).map_err(AppError::from)
}

#[tauri::command]
pub fn get_db_count(state: State<'_, DbState>) -> AppResult<i64> {
    state.repo.get_count().map_err(AppError::from)
}
