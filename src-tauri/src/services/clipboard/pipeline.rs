use tauri::{AppHandle, Manager, Emitter};
use crate::domain::models::ClipboardEntry;
use crate::app_state::{SettingsState, SessionHistory, AppDataDir, PasteQueue};
use crate::database::DbState;
use crate::database::is_text_type;
use crate::services::clipboard::utils::*;
use std::sync::atomic::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};
use crate::infrastructure::windows_api::window_tracker::{
    get_clipboard_source_app_info,
    ActiveAppInfo,
};
use base64::Engine;

#[derive(Debug, Clone)]
pub enum ClipboardData {
    Text(String),
    RichText { text: String, html: String },
    Image { data_url: String },
    Files(Vec<String>),
}

pub struct PipelineContext {
    pub data: ClipboardData,
    pub app_handle: AppHandle,
    pub source_app: String,
    pub source_app_path: Option<String>,
    pub timestamp: i64,
    pub entry: Option<ClipboardEntry>,
    pub should_stop: bool,
    pub pending_removals: Vec<i64>,
    pub reuse_session_id: Option<i64>,
}

impl PipelineContext {
    pub fn new(app_handle: AppHandle, data: ClipboardData, source_snapshot: Option<ActiveAppInfo>) -> Self {
        let active_app = source_snapshot.unwrap_or_else(get_clipboard_source_app_info);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        Self {
            data,
            app_handle,
            source_app: active_app.app_name,
            source_app_path: active_app.process_path,
            timestamp,
            entry: None,
            should_stop: false,
            pending_removals: Vec::new(),
            reuse_session_id: None,
        }
    }
}

pub trait PipelineStage {
    fn process(&self, context: &mut PipelineContext);
}

pub struct ClipboardPipeline {
    stages: Vec<Box<dyn PipelineStage + Send + Sync>>,
}

impl ClipboardPipeline {
    pub fn new() -> Self {
        Self {
            stages: vec![
                Box::new(DiscoveryStage),
                Box::new(TransformationStage),
                Box::new(ValidationStage),
                Box::new(PersistenceStage),
                Box::new(DistributionStage),
            ]
        }
    }

    pub fn execute(&self, context: &mut PipelineContext) {
        for stage in &self.stages {
            stage.process(context);
            if context.should_stop { break; }
        }
    }
}

// Stage 1: Discovery
pub struct DiscoveryStage;
impl PipelineStage for DiscoveryStage {
    fn process(&self, ctx: &mut PipelineContext) {
        let (content_type, content, html_content) = match &ctx.data {
            ClipboardData::Text(t) => (detect_content_type(t), t.clone(), None),
            ClipboardData::RichText { text, html } => ("rich_text".to_string(), text.clone(), Some(html.clone())),
            ClipboardData::Image { data_url } => ("image".to_string(), data_url.clone(), None),
            ClipboardData::Files(f) => {
                let content = f.join("\n");
                if f.len() == 1 {
                    let path = &f[0];
                    let lower = path.to_lowercase();
                    if lower.ends_with(".gif") {
                         ("image".to_string(), path.clone(), None)
                    } else if lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg") || lower.ends_with(".bmp") || lower.ends_with(".webp") {
                        if let Ok(img_data) = std::fs::read(path) {
                            if let Ok(img) = image::load_from_memory(&img_data) {
                                let mut bytes: Vec<u8> = Vec::new();
                                let mut cursor = std::io::Cursor::new(&mut bytes);
                                if img.write_to(&mut cursor, image::ImageFormat::Png).is_ok() {
                                    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
                                    ("image".to_string(), format!("data:image/png;base64,{}", b64), None)
                                } else { ("file".to_string(), content, None) }
                            } else { ("file".to_string(), content, None) }
                        } else { ("file".to_string(), content, None) }
                    } else if lower.ends_with(".mp4") || lower.ends_with(".mkv") || lower.ends_with(".avi") || lower.ends_with(".mov") || lower.ends_with(".wmv") || lower.ends_with(".flv") || lower.ends_with(".webm") {
                        ("video".to_string(), path.clone(), None)
                    } else { ("file".to_string(), content, None) }
                } else {
                    ("file".to_string(), content, None)
                }
            }
        };

        let preview = if content_type == "image" {
            "[Image Content]".to_string()
        } else if content.chars().count() > 500 {
            let preview_text: String = content.chars().take(497).collect();
            format!("{}...", preview_text.replace('\n', " "))
        } else {
            content.replace('\n', " ")
        };

        let is_external = (content_type == "file" || content_type == "video" || content_type == "image")
            && !content.starts_with("data:");

        ctx.entry = Some(ClipboardEntry {
            id: 0,
            content_type,
            content,
            html_content,
            source_app: ctx.source_app.clone(),
            source_app_path: ctx.source_app_path.clone(),
            timestamp: ctx.timestamp,
            preview,
            is_pinned: false,
            tags: Vec::new(),
            use_count: 0,
            is_external,
            pinned_order: 0,
            file_preview_exists: true,
        });
    }
}

// Stage 2: Transformation
pub struct TransformationStage;
impl PipelineStage for TransformationStage {
    fn process(&self, ctx: &mut PipelineContext) {
        let entry = ctx.entry.as_mut().unwrap();
        let settings = ctx.app_handle.state::<SettingsState>();

        // Normalization (already partially done but let's be thorough)
        entry.content = entry.content.trim().replace("\r\n", "\n");

        // Sensitive Info
        let protect_kinds = settings.privacy_protection_kinds.lock().unwrap().clone();
        let custom_rules = settings.privacy_protection_custom_rules.lock().unwrap().clone();
        if settings.privacy_protection.load(Ordering::Relaxed) && is_text_type(&entry.content_type) {
            if contains_sensitive_info(&entry.content, &protect_kinds, &custom_rules) {
                entry.tags.push("sensitive".to_string());
            }
        }
        
        // Rich Text Image Processing
        if let Some(html) = &entry.html_content {
            let app_data_dir = ctx.app_handle.state::<AppDataDir>();
            let data_dir = app_data_dir.0.lock().unwrap().clone();
            
            entry.html_content = if settings.persistent.load(Ordering::Relaxed) {
                let html_with_local_assets = process_local_images_in_html(html, &data_dir);
                Some(externalize_rich_image_fallback(&html_with_local_assets, &data_dir))
            } else {
                Some(embed_local_images(html))
            };
        }
    }
}

// Stage 3: Validation (Deduplication & Sequential Echo)
pub struct ValidationStage;
impl PipelineStage for ValidationStage {
    fn process(&self, ctx: &mut PipelineContext) {
        let settings = ctx.app_handle.state::<SettingsState>();

        // Sequential Echo Check
        if settings.sequential_mode.load(Ordering::Relaxed) {
            let entry = ctx.entry.as_ref().unwrap();
            let queue_state = ctx.app_handle.state::<PasteQueue>();
            let queue = queue_state.0.lock().unwrap();
            if queue.last_action_was_paste && queue.last_pasted_content.as_deref() == Some(&entry.content) {
                println!("Ignoring echo paste from queue");
                ctx.should_stop = true;
                return;
            }
        }

        // Deduplication
        if settings.deduplicate.load(Ordering::Relaxed) {
            let persistent_enabled = settings.persistent.load(Ordering::Relaxed);
            let db_state = ctx.app_handle.state::<DbState>();
            let conn = db_state.conn.lock().unwrap();
            
            let mut existing_id = None;
            let (content, content_type, html_content) = {
                let e = ctx.entry.as_ref().unwrap();
                (e.content.clone(), e.content_type.clone(), e.html_content.clone())
            };

            // Try precise match and normalized match
            let normalized_content = content.trim().replace("\r\n", "\n");
            let normalized_html = |html: &str| html.trim().replace("\r\n", "\n");
            let htmls_equivalent = |a: Option<&str>, b: Option<&str>| -> bool {
                match (a, b) {
                    (None, None) => true,
                    (Some(left), Some(right)) => normalized_html(left) == normalized_html(right),
                    _ => false,
                }
            };
            let rich_text_html_matches = |id: i64| -> bool {
                if let Ok(Some((_content, c_type, h_content))) =
                    db_state.repo.get_entry_content_with_html_with_conn(&conn, id)
                {
                    if c_type != "rich_text" {
                        return false;
                    }
                    return htmls_equivalent(html_content.as_deref(), h_content.as_deref());
                }
                false
            };
            
            // For rich text, we want to deduplicate against all text types
            let types_to_check = if content_type == "rich_text" {
                vec!["rich_text", "text", "code", "url"]
            } else {
                vec![content_type.as_str()]
            };

            for t in types_to_check {
                if let Ok(Some(id)) = db_state.repo.find_by_content_with_conn(&conn, &content, Some(t)) {
                    if content_type == "rich_text" && t == "rich_text" && !rich_text_html_matches(id) {
                        continue;
                    }
                    existing_id = Some(id);
                    break;
                }
                if let Ok(Some(id)) = db_state.repo.find_by_content_with_conn(&conn, &normalized_content, Some(t)) {
                    if content_type == "rich_text" && t == "rich_text" && !rich_text_html_matches(id) {
                        continue;
                    }
                    existing_id = Some(id);
                    break;
                }
            }

            if persistent_enabled {
                if let Some(id) = existing_id {
                    // Instead of deleting, we set the entry ID so PersistenceStage performs an UPDATE
                    // This ensures the item is "moved to top" without risking data loss
                    let entry_mut = ctx.entry.as_mut().unwrap();
                    entry_mut.id = id;
                }
            }
            
            // Session history cleanup
            let session_history = ctx.app_handle.state::<SessionHistory>();
            let mut removed_ids = Vec::new();
            let mut reuse_session_id: Option<i64> = None;
            {
                let session = session_history.0.lock().unwrap();
                let entry = ctx.entry.as_ref().expect("entry exists");
                let normalized_content = entry.content.trim().replace("\r\n", "\n");
                for item in session.iter() {
                    let item_normalized = item.content.trim().replace("\r\n", "\n");
                    let html_match = if entry.content_type == "rich_text" && item.content_type == "rich_text" {
                        htmls_equivalent(item.html_content.as_deref(), entry.html_content.as_deref())
                    } else {
                        true
                    };
                    let match_found = (item.content == entry.content || item_normalized == normalized_content) && html_match;
                    if match_found {
                        removed_ids.push(item.id);
                        if !persistent_enabled {
                            reuse_session_id = Some(item.id);
                        }
                    }
                }
            }
            if !persistent_enabled {
                if let Some(reuse_id) = reuse_session_id {
                    ctx.reuse_session_id = Some(reuse_id);
                    if let Some(entry_mut) = ctx.entry.as_mut() {
                        entry_mut.id = reuse_id;
                    }
                    removed_ids.retain(|id| *id != reuse_id);
                }
            }
            ctx.pending_removals.extend(removed_ids);
        }
    }
}

// Stage 4: Persistence
pub struct PersistenceStage;
impl PipelineStage for PersistenceStage {
    fn process(&self, ctx: &mut PipelineContext) {
        let entry = ctx.entry.as_mut().unwrap();
        let settings = ctx.app_handle.state::<SettingsState>();
        let db_state = ctx.app_handle.state::<DbState>();

        if settings.persistent.load(Ordering::Relaxed) {
            let app_data_dir = ctx.app_handle.state::<AppDataDir>();
            let data_dir = app_data_dir.0.lock().unwrap().clone();
            let conn = db_state.conn.lock().unwrap();
            
            if let Ok(id) = db_state.repo.save_with_conn(&conn, entry, Some(&data_dir)) {
                entry.id = id;
                if let Ok(deleted_ids) = db_state.repo.enforce_limit_with_conn(&conn, Some(&data_dir)) {
                    for rid in deleted_ids { let _ = ctx.app_handle.emit("clipboard-removed", rid); }
                }
            }
        } else {
            // Session-only items
            if let Some(reuse_id) = ctx.reuse_session_id {
                let session_history = ctx.app_handle.state::<SessionHistory>();
                let mut updated_entry: Option<ClipboardEntry> = None;
                {
                    let mut session = session_history.0.lock().unwrap();
                    if let Some(existing) = session.iter_mut().find(|i| i.id == reuse_id) {
                        let preserved_tags = existing.tags.clone();
                        let preserved_pinned = existing.is_pinned;
                        let preserved_pinned_order = existing.pinned_order;
                        let preserved_use_count = existing.use_count;

                        existing.content_type = entry.content_type.clone();
                        existing.content = entry.content.clone();
                        existing.html_content = entry.html_content.clone();
                        existing.source_app = entry.source_app.clone();
                        existing.source_app_path = entry.source_app_path.clone();
                        existing.timestamp = entry.timestamp;
                        existing.preview = entry.preview.clone();
                        existing.is_external = entry.is_external;
                        existing.file_preview_exists = entry.file_preview_exists;
                        existing.is_pinned = preserved_pinned;
                        existing.pinned_order = preserved_pinned_order;
                        existing.tags = if entry.tags.is_empty() { preserved_tags } else { entry.tags.clone() };
                        existing.use_count = preserved_use_count + 1;

                        updated_entry = Some(existing.clone());
                    }
                }

                if let Some(updated) = updated_entry {
                    *entry = updated;
                    return;
                }
            }

            // Use a unique negative ID for new session-only items
            let id = -(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() as i64 / 1000);
            entry.id = id;
            let session_history = ctx.app_handle.state::<SessionHistory>();
            let mut session = session_history.0.lock().unwrap();
            session.push_back(entry.clone());
            if session.len() > 500 {
                if let Some(removed) = session.pop_front() {
                    let _ = ctx.app_handle.emit("clipboard-removed", removed.id);
                }
            }
        }
    }
}

// Stage 5: Distribution
pub struct DistributionStage;
impl PipelineStage for DistributionStage {
    fn process(&self, ctx: &mut PipelineContext) {
        let entry = ctx.entry.as_ref().unwrap();
        let settings = ctx.app_handle.state::<SettingsState>();

        if entry.id == 0 && settings.persistent.load(Ordering::Relaxed) {
            return; // Failed to save
        }

        if !ctx.pending_removals.is_empty() {
            let mut pending = std::mem::take(&mut ctx.pending_removals);
            pending.retain(|id| *id != entry.id);
            if !pending.is_empty() {
                let unique: std::collections::HashSet<i64> = pending.into_iter().collect();
                {
                    let session_history = ctx.app_handle.state::<SessionHistory>();
                    let mut session = session_history.0.lock().unwrap();
                    session.retain(|item| !unique.contains(&item.id));
                }
                for rid in unique {
                    let _ = ctx.app_handle.emit("clipboard-removed", rid);
                }
            }
        }

        // Sequential Queue updates
        if settings.sequential_mode.load(Ordering::Relaxed) {
            let queue_state = ctx.app_handle.state::<PasteQueue>();
            let mut queue = queue_state.0.lock().unwrap();
            if queue.last_action_was_paste {
                queue.items.clear();
                queue.last_action_was_paste = false;
                queue.last_pasted_content = None;
            }
            queue.items.push_back(entry.id);
        }

        // Sound
        if settings.sound_enabled.load(Ordering::Relaxed) {
            let _ = ctx.app_handle.emit("play-sound", "copy");
        }

        // Notify
        let _ = ctx.app_handle.emit("clipboard-updated", truncate_entry_for_ui(entry.clone()));
    }
}
