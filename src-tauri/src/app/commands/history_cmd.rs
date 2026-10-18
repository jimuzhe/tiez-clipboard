use crate::app_state::{AppDataDir, SessionHistory};
use crate::database::DbState;
use crate::domain::models::ClipboardEntry;
use crate::error::{AppError, AppResult};
use crate::infrastructure::repository::clipboard_repo::{ClipboardRepository, ClipboardSortMode};
use crate::infrastructure::repository::tag_repo::TagRepository;
use crate::services::clipboard::{entry_matches_search, truncate_html_for_preview};
use std::collections::HashSet;
use tauri::{AppHandle, Emitter, State};

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ClipboardImportProgressEvent {
    operation_id: String,
    source: String,
    scanned: u64,
    imported: u64,
    duplicates: u64,
    unsupported: u64,
    failed: u64,
}

#[tauri::command]
pub fn discover_clipboard_import_sources() -> Vec<crate::services::importers::DiscoveredImportSource>
{
    crate::services::importers::discover_clipboard_import_sources()
}

#[tauri::command]
pub async fn import_clipboard_data(
    app_handle: AppHandle,
    state: State<'_, DbState>,
    app_data: State<'_, AppDataDir>,
    path: String,
    operation_id: String,
) -> AppResult<crate::services::importers::ImportReport> {
    let source_path = std::path::PathBuf::from(path);
    let destination = state.conn.clone();
    let data_dir = app_data
        .0
        .lock()
        .map_err(|error| AppError::Internal(error.to_string()))?
        .clone();

    let progress_app_handle = app_handle.clone();
    let report = tauri::async_runtime::spawn_blocking(move || {
        crate::services::importers::import_clipboard_data_with_progress(
            destination,
            &data_dir,
            &source_path,
            |progress| {
                let _ = progress_app_handle.emit(
                    "clipboard-import-progress",
                    ClipboardImportProgressEvent {
                        operation_id: operation_id.clone(),
                        source: progress.source.clone(),
                        scanned: progress.scanned,
                        imported: progress.imported,
                        duplicates: progress.duplicates,
                        unsupported: progress.unsupported,
                        failed: progress.failed,
                    },
                );
            },
        )
        .map_err(AppError::Validation)
    })
    .await
    .map_err(|error| AppError::Internal(error.to_string()))??;

    let _ = app_handle.emit("clipboard-changed", ());
    if report.imported > 0 {
        crate::services::cloud_sync::request_cloud_sync(app_handle);
    }
    Ok(report)
}

fn validate_and_repair_external_media(item: &mut ClipboardEntry, data_dir: &std::path::Path) {
    if item.content_type == "image"
        || item.content_type == "video"
        || item.content_type == "file"
        || item.is_external
    {
        if item.content.starts_with("data:") {
            item.file_preview_exists = true;
            return;
        }

        let raw_path = item.content.trim();
        if raw_path.is_empty() {
            item.file_preview_exists = false;
            return;
        }

        // Multi-file entries count as present while any file is: one moved
        // file used to show the whole entry as deleted although the rest still
        // paste. Only the first files are checked, so a huge list or a network
        // share doesn't make every page load stat thousands of paths.
        if item.content_type == "file" && raw_path.contains('\n') {
            const MAX_CHECKED_FILES: usize = 50;
            let mut paths = raw_path.lines().map(str::trim).filter(|line| !line.is_empty());
            let any_exist = paths
                .by_ref()
                .take(MAX_CHECKED_FILES)
                .any(|p| std::path::Path::new(p).exists())
                || paths.next().is_some();
            item.file_preview_exists = any_exist;
            return;
        }

        let path = std::path::Path::new(raw_path);
        if path.exists() {
            item.file_preview_exists = true;
            return;
        }

        // Auto-repair if attachment path was relocated or relative
        if let Some(file_name) = path.file_name().and_then(|f| f.to_str()) {
            let candidate = data_dir.join("attachments").join(file_name);
            if candidate.exists() {
                item.content = candidate.to_string_lossy().to_string();
                item.file_preview_exists = true;
                return;
            }
        }

        item.file_preview_exists = false;
    }
}

// async: off the main thread. Checking that files exist can be slow (many
// files, a disconnected network share) and froze the UI on every list load.
#[tauri::command(async)]
pub fn get_clipboard_history(
    state: State<'_, DbState>,
    session: State<'_, SessionHistory>,
    app_data: State<'_, AppDataDir>,
    limit: i32,
    offset: i32,
    content_type: Option<String>,
    sort_mode: Option<String>,
) -> AppResult<Vec<ClipboardEntry>> {
    let sort_mode = ClipboardSortMode::from_setting(sort_mode.as_deref());
    history_page(&state, &session, &app_data, limit, offset, content_type, sort_mode)
}

/// One page of history (database and session entries merged) in `sort_mode`.
pub fn history_page(
    state: &DbState,
    session: &SessionHistory,
    app_data: &AppDataDir,
    limit: i32,
    offset: i32,
    content_type: Option<String>,
    sort_mode: ClipboardSortMode,
) -> AppResult<Vec<ClipboardEntry>> {
    let data_dir = app_data.0.lock().map(|d| d.clone()).unwrap_or_default();

    let limit = limit.clamp(0, 1000);
    let offset = offset.max(0);
    if limit == 0 {
        return Ok(Vec::new());
    }
    // With session entries present, merge the sorted prefix before slicing.
    let session_items = session.0.lock().unwrap();
    let session_count = session_items.len() as i32;
    let db_offset = if session_count == 0 { offset } else { 0 };
    let db_limit = limit.saturating_add(offset - db_offset);
    let db_items = state.repo.get_history_sorted(
        db_limit, db_offset, content_type.as_deref(), sort_mode,
    )?;
    let mut history = {
        merged_history_page(
            session_items.iter().filter(|item| {
                content_type.as_deref().map_or(true, |ct| item.content_type == ct)
            }),
            &db_items,
            (offset - db_offset) as usize,
            limit as usize,
            sort_mode,
        )
    };
    drop(session_items);

    // 5. Truncate content for UI performance and validate media
    for item in &mut history {
        validate_and_repair_external_media(item, &data_dir);

        if (item.content_type == "text"
            || item.content_type == "code"
            || item.content_type == "url"
            || item.content_type == "rich_text")
            && item.content.chars().count() > 2000
        {
            item.content = format!(
                "{}... [Truncated for speed]",
                item.content.chars().take(2000).collect::<String>()
            );
        }

        if let Some(ref html) = item.html_content {
            if html.chars().count() > 5000 {
                item.html_content = truncate_html_for_preview(html);
            }
        }

        if item.content_type == "text"
            || item.content_type == "code"
            || item.content_type == "url"
            || item.content_type == "rich_text"
        {
            if item.content.chars().count() > 500 {
                item.preview = format!("{}...", item.content.chars().take(497).collect::<String>());
            } else {
                item.preview = item.content.clone();
            }
        }
    }

    Ok(history)
}

fn sorted_page<T: Clone>(items: &mut Vec<&T>, offset: usize, limit: usize,
    compare: impl Fn(&T, &T) -> std::cmp::Ordering) -> Vec<T> {
    items.sort_by(|a, b| compare(a, b));
    items.iter().skip(offset).take(limit).map(|item| (*item).clone()).collect()
}

fn merged_history_page<'a>(
    session_items: impl Iterator<Item = &'a ClipboardEntry>,
    db_items: &'a [ClipboardEntry],
    offset: usize,
    limit: usize,
    sort_mode: ClipboardSortMode,
) -> Vec<ClipboardEntry> {
    let mut seen = HashSet::new();
    // Prefer database fields over any legacy in-memory copy. Deduplicate BEFORE
    // slicing so duplicate IDs cannot consume page slots or shift later pages.
    let mut candidates = db_items.iter().chain(session_items)
        .filter(|item| item.id == 0 || seen.insert(item.id))
        .collect();
    sorted_page(&mut candidates, offset, limit, |a, b| sort_mode.compare(a, b))
}

#[cfg(test)]
mod merged_page_tests {
    use super::{merged_history_page, sorted_page, ClipboardEntry, ClipboardSortMode};

    fn entry(id: i64, sort_at: i64, tags: &[&str]) -> ClipboardEntry {
        serde_json::from_value(serde_json::json!({
            "id": id, "content_type": "text", "content": "same content",
            "source_app": "Test", "timestamp": sort_at, "sort_at": sort_at,
            "preview": "same content", "is_pinned": false, "tags": tags,
        })).unwrap()
    }

    #[test]
    fn promoted_entry_appears_once_and_uses_database_tags() {
        let session = [entry(12, 30, &["old"]), entry(-1, 20, &[])];
        let database = [entry(12, 30, &["new"]), entry(13, 10, &[])];
        let first = merged_history_page(session.iter(), &database, 0, 2, ClipboardSortMode::Manual);
        assert_eq!(first.iter().map(|item| item.id).collect::<Vec<_>>(), vec![12, -1]);
        assert_eq!(first[0].tags, vec!["new"]);
        let next = merged_history_page(session.iter(), &database, 2, 2, ClipboardSortMode::Manual);
        assert_eq!(next.iter().map(|item| item.id).collect::<Vec<_>>(), vec![13]);
    }

    #[test]
    fn equal_content_with_different_ids_is_not_removed() {
        let session = [entry(-1, 20, &[])];
        let database = [entry(12, 30, &["tag"]), entry(13, 10, &[])];
        assert_eq!(merged_history_page(session.iter(), &database, 0, 10, ClipboardSortMode::Manual).len(), 3);
    }

    #[test]
    fn recent_picks_the_newest_copy_even_below_pins() {
        let mut pin = entry(5, 10, &[]);
        pin.is_pinned = true;
        pin.pinned_order = 1;
        let database = [pin.clone(), entry(6, 20, &[])];
        let session = [entry(-1, 15, &[])];
        let top = |db: &[ClipboardEntry]| {
            merged_history_page(session.iter(), db, 0, 1, ClipboardSortMode::Recent)[0].id
        };
        assert_eq!(top(&database), 6);
        // A pin copied again is the newest copy, so it is the one pasted.
        pin.sort_at = 30;
        assert_eq!(top(&[pin, entry(6, 20, &[])]), 5);
    }

    #[test]
    fn mixed_sources_are_sorted_before_pagination() {
        let session = [8, 3, 1];
        let database = [10, 9, 7, 6, 5, 4, 2];
        let mut candidates = session.iter().chain(database.iter()).collect();
        assert_eq!(sorted_page(&mut candidates, 0, 4, |a, b| b.cmp(a)), vec![10, 9, 8, 7]);
        assert_eq!(sorted_page(&mut candidates, 4, 4, |a, b| b.cmp(a)), vec![6, 5, 4, 3]);
        assert_eq!(sorted_page(&mut candidates, 8, 4, |a, b| b.cmp(a)), vec![2, 1]);
        assert!(sorted_page(&mut candidates, 10, 4, |a, b| b.cmp(a)).is_empty());
    }
}

#[tauri::command]
pub fn search_clipboard_history(
    state: State<'_, DbState>,
    session: State<'_, SessionHistory>,
    app_data: State<'_, AppDataDir>,
    search_term: String,
    limit: i32,
    tag_only: Option<bool>,
    note_only: Option<bool>,
) -> AppResult<Vec<ClipboardEntry>> {
    let is_tag_only = tag_only.unwrap_or(false);
    let is_note_only = note_only.unwrap_or(false);
    let data_dir = app_data.0.lock().map(|d| d.clone()).unwrap_or_default();
    let mut history = state
        .repo
        .search(&search_term, limit, is_tag_only, is_note_only)?;

    let session_items = session.inner().0.lock().unwrap();
    for item in session_items.iter().rev() {
        if entry_matches_search(item, &search_term, is_tag_only, is_note_only) {
            if !history.iter().any(|h| h.id == item.id && item.id != 0) {
                history.push(item.clone());
            }
        }
    }

    history.sort_by(|a, b| b.sort_at.cmp(&a.sort_at).then_with(|| b.id.cmp(&a.id)));
    if history.len() > limit as usize {
        history.truncate(limit as usize);
    }

    for item in &mut history {
        validate_and_repair_external_media(item, &data_dir);
        if (item.content_type == "text"
            || item.content_type == "code"
            || item.content_type == "url"
            || item.content_type == "rich_text")
            && item.content.chars().count() > 2000
        {
            item.content = format!(
                "{}... [Truncated for speed]",
                item.content.chars().take(2000).collect::<String>()
            );
        }

        if let Some(ref html) = item.html_content {
            if html.chars().count() > 5000 {
                item.html_content = truncate_html_for_preview(html);
            }
        }

        if item.content_type == "text"
            || item.content_type == "code"
            || item.content_type == "url"
            || item.content_type == "rich_text"
        {
            if item.content.chars().count() > 500 {
                item.preview = format!("{}...", item.content.chars().take(497).collect::<String>());
            } else {
                item.preview = item.content.clone();
            }
        }
    }

    Ok(history)
}

#[tauri::command]
pub fn delete_clipboard_entry(
    app_handle: AppHandle,
    state: State<'_, DbState>,
    session: State<'_, SessionHistory>,
    app_data: State<'_, AppDataDir>,
    id: i64,
) -> AppResult<()> {
    {
        let mut session_items = session.inner().0.lock().unwrap();
        session_items.retain(|item| item.id != id);
    }
    // Session-only images go as soon as their entry does (privacy).
    crate::services::clipboard::sweep_session_files(&app_handle);

    if id > 0 {
        let data_dir = app_data.0.lock().unwrap();
        state.repo.delete(id, Some(&data_dir))?;
    }
    let _ = app_handle.emit("clipboard-changed", ());
    crate::services::cloud_sync::request_cloud_sync(app_handle);
    Ok(())
}

#[tauri::command]
pub fn clear_clipboard_history(
    app_handle: AppHandle,
    state: State<'_, DbState>,
    session: State<'_, SessionHistory>,
    app_data: State<'_, AppDataDir>,
) -> AppResult<()> {
    {
        let mut session_items = session.inner().0.lock().unwrap();
        session_items.retain(|item| item.is_pinned || !item.tags.is_empty());
    }
    // Session-only images go as soon as their entry does (privacy).
    crate::services::clipboard::sweep_session_files(&app_handle);
    let data_dir = app_data.0.lock().unwrap();
    state.repo.clear(Some(&data_dir)).map_err(AppError::from)?;
    let _ = app_handle.emit("clipboard-changed", ());
    crate::services::cloud_sync::request_cloud_sync(app_handle);
    Ok(())
}

#[tauri::command]
pub fn get_tag_items(state: State<'_, DbState>, tag: String) -> AppResult<Vec<ClipboardEntry>> {
    let mut history = state
        .tag_repo
        .get_entries_by_tag(&tag)
        .map_err(AppError::from)?;

    for item in &mut history {
        if (item.content_type == "text"
            || item.content_type == "code"
            || item.content_type == "url"
            || item.content_type == "rich_text")
            && item.content.chars().count() > 50000
        {
            item.content = format!(
                "{}... [Content Truncated]",
                item.content.chars().take(50000).collect::<String>()
            );
        }
    }

    Ok(history)
}

#[tauri::command]
pub fn get_all_tags_info(
    state: State<'_, DbState>,
) -> AppResult<std::collections::HashMap<String, i32>> {
    state.tag_repo.get_all_with_counts().map_err(AppError::from)
}

#[tauri::command]
pub fn rename_tag_globally(
    state: State<'_, DbState>,
    session: State<'_, SessionHistory>,
    old_name: String,
    new_name: String,
) -> AppResult<()> {
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

    state
        .tag_repo
        .rename(&old_name, &new_name)
        .map_err(AppError::from)
}

#[tauri::command]
pub fn delete_tag_from_all(
    app_handle: tauri::AppHandle,
    state: State<'_, DbState>,
    session: State<'_, SessionHistory>,
    app_data: State<'_, AppDataDir>,
    tag_name: String,
) -> AppResult<()> {
    {
        let mut session_items = session.inner().0.lock().unwrap();
        session_items.retain(|item| !item.tags.contains(&tag_name));
    }
    // Session-only images go as soon as their entry does (privacy).
    crate::services::clipboard::sweep_session_files(&app_handle);

    let data_dir = app_data.0.lock().unwrap();
    state
        .tag_repo
        .delete_globally(&tag_name, Some(&data_dir))
        .map_err(AppError::from)
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

    state
        .repo
        .get_entry_content(id)
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::Validation("Entry not found".to_string()))
}

#[tauri::command]
pub fn update_pinned_order(
    app_handle: AppHandle,
    state: State<'_, DbState>,
    orders: Vec<(i64, i64)>,
) -> AppResult<()> {
    state
        .repo
        .update_pinned_order(orders)
        .map_err(AppError::from)?;
    let _ = app_handle.emit("clipboard-changed", ());
    crate::services::cloud_sync::request_cloud_sync(app_handle);
    Ok(())
}

#[tauri::command]
pub fn get_db_count(state: State<'_, DbState>) -> AppResult<i64> {
    state.repo.get_count().map_err(AppError::from)
}
