use axum::{
    extract::{Multipart, State, Query, WebSocketUpgrade, ws::{WebSocket, Message as WsMessage}, Path},
    response::{Html, IntoResponse, Json},
    http::{header, HeaderMap, StatusCode},
    body::Body,
};
use futures::{StreamExt, SinkExt};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt, AsyncSeekExt, SeekFrom};
use tokio::fs::File;
use tokio_util::io::ReaderStream;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use std::sync::atomic::Ordering;
use tauri::{Emitter, Manager};

use crate::infrastructure::repository::clipboard_repo::ClipboardRepository;
use crate::infrastructure::repository::settings_repo::SettingsRepository;
use crate::database::DbState;
use crate::app_state::{SettingsState, SessionHistory};
use crate::database::ClipboardEntry;

use super::models::*;
use super::utils::*;
use super::web_ui::render_index;
use super::{append_message, register_received_file};

pub async fn index(State(state): State<Arc<AppState>>) -> Html<String> {
    let app_handle = &state.app_handle;
    let settings = app_handle.state::<SettingsState>();
    let logo_base64 = get_app_logo_base64(app_handle);
    let theme = {
        let guard = settings.theme.lock().unwrap();
        guard.clone()
    };

    Html(render_index(&theme, &logo_base64))
}

pub async fn poll_messages(
    Query(params): Query<HashMap<String, String>>,
    State(state): State<Arc<AppState>>,
) -> Json<Vec<Message>> {
    let last_id = params.get("last_id").and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
    let chat_state = state.app_handle.state::<ChatState>();
    
    let msgs_result = {
        match chat_state.0.lock() {
            Ok(msgs) => {
                msgs.iter()
                    .filter(|m| m.id > last_id)
                    .map(|m| {
                         let mut m_clone = m.clone();
                         if m.msg_type == "image" && !m.content.starts_with("data:") && !m.content.starts_with("/download/") {
                            let token = format!("temp_{}", m.id);
                            let mut filename = "image.png".to_string();
                            let path = std::path::Path::new(&m.content);
                            if let Some(name) = path.file_name() {
                                filename = name.to_string_lossy().to_string();
                            }
                            
                            let shared_files = state.app_handle.state::<SharedFileState>();
                            if let Ok(mut map) = shared_files.0.lock() {
                                 map.insert(token.clone(), m.content.clone());
                            }
                            m_clone.content = format!("/download/{}?name={}", token, urlencoding::encode(&filename));
                         }
                         m_clone
                    })
                    .collect::<Vec<Message>>()
            },
            Err(_) => vec![]
        }
    };

    Json(msgs_result)
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

#[derive(serde::Deserialize)]
#[serde(tag = "type")]
enum WsIncoming {
    #[serde(rename = "identity")]
    Identity { device_id: String, device_name: String },
}

pub async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.ws_tx.subscribe();
    let mut current_device_id: Option<String> = None;

    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if sender.send(WsMessage::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    let state_inner = state.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let WsMessage::Text(text) = msg {
                if let Ok(incoming) = serde_json::from_str::<WsIncoming>(&text) {
                    match incoming {
                        WsIncoming::Identity { device_id, device_name } => {
                            let online_devices = state_inner.app_handle.state::<OnlineDevices>();
                            {
                                let mut guard = online_devices.0.lock().unwrap();
                                guard.insert(device_id.clone(), DeviceInfo {
                                    id: device_id.clone(),
                                    name: device_name,
                                    last_seen: chrono::Utc::now().timestamp_millis(),
                                });
                                current_device_id = Some(device_id);
                                
                                let devices: Vec<DeviceInfo> = guard.values().cloned().collect();
                                let update = serde_json::json!({
                                    "type": "devices_update",
                                    "devices": devices
                                });
                                let _ = state_inner.ws_tx.send(update.to_string());
                                let _ = state_inner.app_handle.emit("online-devices-updated", devices);
                            }
                        }
                    }
                }
            }
        }
        
        if let Some(id) = current_device_id {
            let online_devices = state_inner.app_handle.state::<OnlineDevices>();
            {
                let mut guard = online_devices.0.lock().unwrap();
                guard.remove(&id);
                let devices: Vec<DeviceInfo> = guard.values().cloned().collect();
                let update = serde_json::json!({
                    "type": "devices_update",
                    "devices": devices
                });
                let _ = state_inner.ws_tx.send(update.to_string());
                let _ = state_inner.app_handle.emit("online-devices-updated", devices);
            }
        }
    });

    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    }
}

pub async fn handle_text(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ReceiveText>,
) -> axum::response::Response {
    let db_state = state.app_handle.state::<DbState>();
    update_activity(&state.app_handle);
    
    let sender_id = if payload.sender_id.is_empty() { "mobile" } else { &payload.sender_id };
    let sender_name = if payload.sender_name.is_empty() { "手机" } else { &payload.sender_name };
    
    append_message(&state.app_handle, "in", "text", &payload.content, sender_id, sender_name, None);

    let settings = state.app_handle.state::<SettingsState>();
    let session_hist = state.app_handle.state::<SessionHistory>();
    
    let mut preview = payload.content.clone();
    if preview.chars().count() > 100 {
        preview = preview.chars().take(100).collect();
        preview.push_str("...");
    }

    let mut id_result = Ok(0);

    if settings.auto_copy_file.load(Ordering::Relaxed) {
        id_result = if settings.persistent.load(Ordering::Relaxed) {
            let entry = ClipboardEntry {
                id: 0,
                content_type: "text".to_string(),
                content: payload.content.clone(),
                html_content: None,
                source_app: sender_name.to_string(),
                source_app_path: None,
                timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64,
                preview: preview.clone(),
                is_pinned: false,
                tags: Vec::new(),
                use_count: 0,
                is_external: false,
                pinned_order: 0,
                file_preview_exists: true,
            };
            db_state.repo.save(&entry, None).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))
        } else {
            let id = -(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() as i64 / 1000);
            let entry = ClipboardEntry {
                id,
                content_type: "text".to_string(),
                content: payload.content.clone(),
                html_content: None,
                source_app: "File Transfer".to_string(),
                source_app_path: None,
                timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64,
                preview: preview.clone(),
                is_pinned: false,
                tags: Vec::new(),
                use_count: 0,
                is_external: false,
                pinned_order: 0,
                file_preview_exists: true,
            };

            if let Ok(mut session) = session_hist.0.lock() {
                session.push_back(entry);
                if session.len() > 500 {
                    if let Some(removed) = session.pop_front() {
                        let _ = state.app_handle.emit("clipboard-removed", removed.id);
                    }
                }
            }
            Ok(id)
        };
    }

    if let Ok(id) = id_result {
        if id != 0 {
            let _ = state.app_handle.emit("clipboard-changed", id);
            return (StatusCode::OK, "Text received").into_response();
        }
    }
    (StatusCode::INTERNAL_SERVER_ERROR, "Failed to save text").into_response()
}

pub async fn upload(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> axum::response::Response {
    update_activity(&state.app_handle);
    let mut success = false;
    let db_state = state.app_handle.state::<DbState>();

    let mut current_sender_id = "mobile".to_string();
    let mut current_sender_name = "手机".to_string();

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        
        if name == "sender_id" {
            if let Ok(val) = field.text().await { current_sender_id = val; }
            continue;
        }
        if name == "sender_name" {
            if let Ok(val) = field.text().await { current_sender_name = val; }
            continue;
        }

        if name == "file" {
            let file_name = field.file_name().unwrap_or("unknown.txt").to_string();
            let content_type = field.content_type().unwrap_or("application/octet-stream").to_string();
            
            let mut save_dir = state.app_handle.path().download_dir().unwrap_or_else(|_| std::env::temp_dir());
            if let Ok(Some(custom)) = db_state.settings_repo.get("file_transfer_path") {
                 if !custom.trim().is_empty() { save_dir = std::path::PathBuf::from(custom); }
            }
            if !save_dir.exists() { let _ = std::fs::create_dir_all(&save_dir); }

            let target_path = save_dir.join(format!("{}_{}", chrono::Utc::now().format("%Y%m%d%H%M%S"), file_name));
            
            if let Ok(mut file) = File::create(&target_path).await {
                let mut stream = field;
                let mut write_success = true;
                while let Some(Ok(chunk)) = stream.next().await {
                    if let Err(e) = file.write_all(&chunk).await {
                        eprintln!("Error writing: {}", e);
                        write_success = false;
                        break;
                    }
                }

                if write_success {
                    register_received_file(
                        &state.app_handle,
                        target_path,
                        file_name,
                        content_type,
                        current_sender_id.clone(),
                        current_sender_name.clone()
                    ).await;
                    success = true;
                }
            }
        }
    }
    
    if success {
        (StatusCode::OK, "Upload successful").into_response()
    } else {
        (StatusCode::INTERNAL_SERVER_ERROR, "Upload failed").into_response()
    }
}

pub async fn upload_chunk(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> axum::response::Response {
    update_activity(&state.app_handle);
    let mut metadata: Option<ChunkMetadata> = None;
    let mut chunk_data: Option<Vec<u8>> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();

        if name == "metadata" {
            if let Ok(bytes) = field.bytes().await {
                if let Ok(text) = String::from_utf8(bytes.to_vec()) {
                    if let Ok(m) = serde_json::from_str(&text) {
                        metadata = Some(m);
                    }
                }
            }
        } else if name == "data" || name == "file" {
            if let Ok(bytes) = field.bytes().await {
                chunk_data = Some(bytes.to_vec());
            }
        }
    }

    let meta = match metadata {
        Some(m) => m,
        None => return (StatusCode::BAD_REQUEST, "Missing metadata").into_response(),
    };
    let data = match chunk_data {
        Some(d) => d,
        None => return (StatusCode::BAD_REQUEST, "Missing data").into_response(),
    };

    let sessions = state.app_handle.state::<UploadSessions>();
    let temp_path = {
        let mut sessions_map = sessions.0.lock().unwrap();
        sessions_map
            .entry(meta.upload_id.clone())
            .or_insert_with(|| {
                let mut path = state.app_handle.path().download_dir().unwrap_or_else(|_| std::env::temp_dir());
                if let Ok(Some(custom)) = state.app_handle.state::<DbState>().settings_repo.get("file_transfer_path") {
                    if !custom.trim().is_empty() { path = std::path::PathBuf::from(custom); }
                }
                if !path.exists() { let _ = std::fs::create_dir_all(&path); }
                path.join(format!(".tmp_{}", meta.upload_id))
            })
            .clone()
    };

    let mut options = tokio::fs::OpenOptions::new();
    options.create(true).append(true).write(true);

    if let Ok(mut file) = options.open(&temp_path).await {
        if let Err(e) = file.write_all(&data).await {
            eprintln!("Error writing chunk: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Write failed").into_response();
        }
    } else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "Open failed").into_response();
    }

    if meta.chunk_index == meta.total_chunks - 1 {
        let final_filename = format!("{}_{}", chrono::Utc::now().format("%Y%m%d%H%M%S"), meta.file_name);
        let final_path = temp_path.parent().unwrap().join(&final_filename);

        if let Err(e) = tokio::fs::rename(&temp_path, &final_path).await {
            eprintln!("Error finalizing file: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Finalize failed").into_response();
        }

        {
            let mut sessions_map = sessions.0.lock().unwrap();
            sessions_map.remove(&meta.upload_id);
        }

        let content_type = meta.content_type.unwrap_or_else(|| "application/octet-stream".to_string());
        register_received_file(
            &state.app_handle,
            final_path,
            meta.file_name,
            content_type,
            meta.sender_id,
            meta.sender_name,
        ).await;

        return (StatusCode::OK, "Upload complete").into_response();
    }

    (StatusCode::OK, "Chunk received").into_response()
}

pub async fn handle_file_download_proxy(
    Path(token): Path<String>,
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let app_handle = &state.app_handle;
    update_activity(app_handle);
    let shared_state = app_handle.state::<SharedFileState>();

    let file_path = {
        let guard = shared_state.0.lock().unwrap();
        guard.get(&token).cloned()
    };

    if let Some(path_str) = file_path {
        let path = std::path::PathBuf::from(&path_str);
        if path.exists() {
            let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            let mime = mime_guess::from_path(&path).first_or_octet_stream().to_string();
            let is_image = mime.starts_with("image/");
            let is_video = mime.starts_with("video/");
            let encoded_name = urlencoding::encode(&filename);
            let disposition = if is_image || is_video {
                format!("inline; filename=\"{}\"; filename*=UTF-8''{}", filename, encoded_name)
            } else {
                format!("attachment; filename=\"{}\"; filename*=UTF-8''{}", filename, encoded_name)
            };

            if let Ok(mut file) = File::open(&path).await {
                let metadata = match file.metadata().await {
                    Ok(m) => m,
                    Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Metadata failed").into_response(),
                };
                let total_size = metadata.len();
                let range_header = headers.get(header::RANGE).and_then(|h| h.to_str().ok());

                if let Some(range) = range_header {
                    if let Some(r) = range.strip_prefix("bytes=") {
                        let parts: Vec<&str> = r.split('-').collect();
                        if parts.len() == 2 {
                            let start = parts[0].parse::<u64>().unwrap_or(0);
                            let end = parts[1].parse::<u64>().unwrap_or(total_size - 1);

                            if start < total_size {
                                let end = if end >= total_size { total_size - 1 } else { end };
                                let content_length = end - start + 1;

                                if let Ok(_) = file.seek(SeekFrom::Start(start)).await {
                                    let stream = ReaderStream::with_capacity(file.take(content_length), 64 * 1024);
                                    let body = Body::from_stream(stream);

                                    return (
                                        StatusCode::PARTIAL_CONTENT,
                                        [
                                            (header::CONTENT_TYPE, mime),
                                            (header::CONTENT_DISPOSITION, disposition),
                                            (header::ACCEPT_RANGES, "bytes".to_string()),
                                            (header::CONTENT_RANGE, format!("bytes {}-{}/{}", start, end, total_size)),
                                            (header::CONTENT_LENGTH, content_length.to_string()),
                                        ],
                                        body,
                                    ).into_response();
                                }
                            }
                        }
                    }
                }

                let stream = ReaderStream::with_capacity(file, 64 * 1024);
                let body = Body::from_stream(stream);

                return (
                    StatusCode::OK,
                    [
                        (header::CONTENT_TYPE, mime),
                        (header::CONTENT_DISPOSITION, disposition),
                        (header::ACCEPT_RANGES, "bytes".to_string()),
                        (header::CONTENT_LENGTH, total_size.to_string()),
                    ],
                    body,
                ).into_response();
            }
        }
    }

    (StatusCode::NOT_FOUND, "File not found").into_response()
}
