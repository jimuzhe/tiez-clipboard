pub mod handlers;
pub mod models;
pub mod sealed;
pub mod utils;
pub mod web_ui;

use axum::{
    extract::DefaultBodyLimit,
    middleware,
    routing::{get, options, post},
    Router,
};
use base64::Engine;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use crate::app_state::{SessionHistory, SettingsState};
use crate::database::ClipboardEntry;
use crate::database::DbState;
use crate::infrastructure::repository::clipboard_repo::ClipboardRepository;
use crate::infrastructure::repository::settings_repo::SettingsRepository;
pub use models::*;
pub use utils::*;

pub static SERVER_HANDLE: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);
/// Aborting the server task only stops accepting; axum's per-connection tasks
/// (open WebSockets) live on until told to close through this.
static SERVER_SHUTDOWN: Mutex<Option<tokio::sync::watch::Sender<bool>>> = Mutex::new(None);
/// Bumped on every start, so an idle watch never stops a later server.
static SERVER_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// "Close automatically": how long the server may sit with no device
/// connected and no request before it stops.
const AUTO_CLOSE_IDLE: std::time::Duration = std::time::Duration::from_secs(5 * 60);
/// Set while toggle_file_server is starting a server, so a concurrent start
/// (UI toggle racing an auto-start) can't spawn a second, unstoppable one.
static SERVER_STARTING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

struct StartingGuard;

impl Drop for StartingGuard {
    fn drop(&mut self) {
        SERVER_STARTING.store(false, Ordering::SeqCst);
    }
}
const SHARED_FILE_TOKEN_LIFETIME: Duration = Duration::from_secs(8 * 60 * 60);
const MAX_SHARED_FILE_TOKENS: usize = 1_024;
const MAX_CHAT_MESSAGES: usize = 5_000;
const MAX_FILES_PER_BATCH: usize = 2_000;
static NEXT_CHAT_MESSAGE_ID: AtomicU64 = AtomicU64::new(1);

pub(super) fn register_shared_file(app_handle: &AppHandle, file_path: String) -> Option<String> {
    if !std::path::Path::new(&file_path).is_file() {
        return None;
    }

    let shared_state = app_handle.state::<SharedFileState>();
    let mut files = shared_state.0.lock().ok()?;
    files.retain(|_, entry| {
        let keep = !entry.is_expired();
        if !keep && is_generated_batch_archive(&entry.path) {
            let _ = std::fs::remove_file(&entry.path);
        }
        keep
    });
    // Reuse the file's live token: /poll re-registers every file message, and
    // minting fresh ones evicted the links the phone was already showing.
    if let Some((token, entry)) = files.iter_mut().find(|(_, entry)| entry.path == file_path) {
        *entry = SharedFileEntry::new(file_path, SHARED_FILE_TOKEN_LIFETIME);
        return Some(token.clone());
    }
    if files.len() >= MAX_SHARED_FILE_TOKENS {
        if let Some(oldest_key) = files
            .iter()
            .min_by_key(|(_, entry)| entry.expires_at)
            .map(|(key, _)| key.clone())
        {
            if let Some(entry) = files.remove(&oldest_key) {
                if is_generated_batch_archive(&entry.path) {
                    let _ = std::fs::remove_file(entry.path);
                }
            }
        }
    }

    let token = uuid::Uuid::new_v4().to_string();
    files.insert(
        token.clone(),
        SharedFileEntry::new(file_path, SHARED_FILE_TOKEN_LIFETIME),
    );
    Some(token)
}

fn clear_shared_files(app_handle: &AppHandle) {
    if let Some(shared_state) = app_handle.try_state::<SharedFileState>() {
        if let Ok(mut files) = shared_state.0.lock() {
            for entry in files.values() {
                if is_generated_batch_archive(&entry.path) {
                    let _ = std::fs::remove_file(&entry.path);
                }
            }
            files.clear();
        }
    }
}

fn is_generated_batch_archive(path: &str) -> bool {
    std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.starts_with(".tiez-batch-") && name.ends_with(".zip"))
        .unwrap_or(false)
}

fn clear_upload_sessions(app_handle: &AppHandle) {
    if let Some(upload_sessions) = app_handle.try_state::<UploadSessions>() {
        if let Ok(mut sessions) = upload_sessions.0.lock() {
            for path in sessions.values() {
                let _ = std::fs::remove_file(path);
            }
            sessions.clear();
        }
    }
}


/// Stops the server once "close automatically" is on and it has had no
/// connected device and no request for `AUTO_CLOSE_IDLE`. The setting was
/// saved before but nothing acted on it.
fn spawn_idle_watch(app_handle: AppHandle) {
    let generation = SERVER_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            tick.tick().await;
            let running = SERVER_HANDLE.lock().map(|h| h.is_some()).unwrap_or(false);
            if !running || SERVER_GENERATION.load(Ordering::SeqCst) != generation {
                break;
            }
            let settings = app_handle.state::<crate::app_state::SettingsState>();
            if !settings.file_server_auto_close.load(Ordering::Relaxed) {
                continue;
            }
            let connected = app_handle
                .state::<OnlineDevices>()
                .0
                .lock()
                .map(|devices| !devices.is_empty())
                .unwrap_or(true);
            let idle = app_handle
                .state::<ServerActivityState>()
                .last_activity
                .lock()
                .ok()
                .and_then(|last| *last)
                .and_then(|last| last.elapsed().ok())
                .unwrap_or_default();
            if !connected && idle >= AUTO_CLOSE_IDLE {
                let _ = toggle_file_server(app_handle.clone(), false, None).await;
                break;
            }
        }
    });
}

/// The pairing key as it goes into the QR code; empty while the server is off.
fn lan_key_text(server_info: &ServerInfo) -> String {
    server_info
        .lan_key
        .lock()
        .unwrap()
        .as_ref()
        .map(sealed::encode_key)
        .unwrap_or_default()
}

#[tauri::command]
pub fn get_local_ip_addr(app_handle: AppHandle) -> String {
    let server_info = app_handle.state::<ServerInfo>();
    if let Ok(guard) = server_info.ip.lock() {
        if !guard.is_empty() && *guard != "0.0.0.0" {
            return guard.clone();
        }
    }

    let ips = get_available_ips();
    if !ips.is_empty() {
        return ips[0].clone();
    }

    crate::infrastructure::net_interfaces::local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or("127.0.0.1".to_string())
}

/// Phones connected now; the window otherwise learns only about changes.
#[tauri::command]
pub fn get_online_devices(app_handle: AppHandle) -> Vec<DeviceInfo> {
    app_handle
        .state::<OnlineDevices>()
        .0
        .lock()
        .map(|devices| devices.values().cloned().collect())
        .unwrap_or_default()
}

#[tauri::command]
pub fn set_display_ip(app_handle: AppHandle, ip: String) {
    let server_info = app_handle.state::<ServerInfo>();
    {
        let mut guard = server_info.ip.lock().unwrap();
        *guard = ip.clone();
    }
    let port = server_info.port.load(Ordering::Relaxed);
    let _ = app_handle.emit(
        "file-server-status-changed",
        StatusPayload {
            enabled: port != 0,
            port,
            ip,
            access_token: server_info.access_token.lock().unwrap().clone(),
            lan_key: lan_key_text(&server_info),
        },
    );
}

#[tauri::command]
pub fn get_active_file_transfer_path(app_handle: AppHandle) -> String {
    let db_state = app_handle.state::<DbState>();
    let mut dir = app_handle
        .path()
        .download_dir()
        .unwrap_or_else(|_| std::env::temp_dir());
    if let Ok(Some(custom)) = db_state.settings_repo.get("file_transfer_path") {
        if !custom.trim().is_empty() {
            dir = std::path::PathBuf::from(custom);
        }
    }
    dir.to_string_lossy().to_string()
}

#[tauri::command]
pub fn send_chat_message(app_handle: AppHandle, msg_type: String, content: String) {
    append_message(&app_handle, "out", &msg_type, &content, "pc", "电脑", None);
}

#[tauri::command]
pub fn get_chat_history(app_handle: AppHandle) -> Vec<Message> {
    if let Ok(msgs) = app_handle.state::<ChatState>().0.lock() {
        return msgs.clone();
    }
    vec![]
}

#[tauri::command]
pub fn clear_chat_history(app_handle: AppHandle) -> Result<(), String> {
    if let Ok(mut msgs) = app_handle.state::<ChatState>().0.lock() {
        msgs.clear();
    }
    if let Some(ws_state) = app_handle.try_state::<WsBroadcaster>() {
        if let Ok(guard) = ws_state.0.lock() {
            if let Some(tx) = guard.as_ref() {
                let clear_event = serde_json::json!({
                    "type": "clear_history"
                });
                let _ = tx.send(clear_event.to_string());
            }
        }
    }
    let _ = app_handle.emit("chat-history-cleared", ());
    Ok(())
}

#[tauri::command]
pub fn open_file_transfer_folder(app_handle: AppHandle) -> Result<(), String> {
    let folder_path = get_active_file_transfer_path(app_handle);
    let path = std::path::Path::new(&folder_path);
    if !path.exists() {
        let _ = std::fs::create_dir_all(path);
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&folder_path)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&folder_path)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(&folder_path)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }
    Ok(())
}

#[tauri::command]
pub fn send_file_to_client(app_handle: AppHandle, file_path: String) -> Result<(), String> {
    if !std::path::Path::new(&file_path).is_file() {
        return Err("File does not exist or is not a regular file".to_string());
    }

    update_activity(&app_handle);

    let server_info = app_handle.state::<ServerInfo>();
    let port = server_info.port.load(Ordering::Relaxed);
    let ip = server_info.ip.lock().unwrap().clone();

    if port == 0 || ip.is_empty() || ip == "0.0.0.0" {
        return Err("Server not running or IP not detected".to_string());
    }

    let path_lower = file_path.to_lowercase();
    let is_image = path_lower.ends_with(".png")
        || path_lower.ends_with(".jpg")
        || path_lower.ends_with(".jpeg")
        || path_lower.ends_with(".gif")
        || path_lower.ends_with(".webp")
        || path_lower.ends_with(".bmp")
        || path_lower.ends_with(".svg")
        || path_lower.ends_with(".ico");

    let is_video = path_lower.ends_with(".mp4")
        || path_lower.ends_with(".mkv")
        || path_lower.ends_with(".avi")
        || path_lower.ends_with(".mov")
        || path_lower.ends_with(".wmv")
        || path_lower.ends_with(".flv")
        || path_lower.ends_with(".webm");

    let msg_type = if is_image {
        "image"
    } else if is_video {
        "video"
    } else {
        "file"
    };

    append_message(
        &app_handle,
        "out",
        msg_type,
        &file_path,
        "pc",
        "电脑",
        Some(&file_path),
    );

    Ok(())
}

#[tauri::command]
pub fn send_files_to_client(
    app_handle: AppHandle,
    file_paths: Vec<String>,
    batch_name: Option<String>,
) -> Result<String, String> {
    if file_paths.len() < 2 {
        return Err("A file package requires at least two files".to_string());
    }
    if file_paths.len() > MAX_FILES_PER_BATCH {
        return Err("Too many files in one package".to_string());
    }

    let mut files = Vec::with_capacity(file_paths.len());
    let mut total_size = 0_u64;
    for file_path in file_paths {
        let path = std::path::PathBuf::from(&file_path);
        let metadata = std::fs::metadata(&path)
            .map_err(|_| format!("File does not exist: {}", path.to_string_lossy()))?;
        if !metadata.is_file() {
            return Err(format!("Not a regular file: {}", path.to_string_lossy()));
        }
        total_size = total_size.saturating_add(metadata.len());
        files.push((file_path, metadata.len()));
    }

    update_activity(&app_handle);
    let server_info = app_handle.state::<ServerInfo>();
    let port = server_info.port.load(Ordering::Relaxed);
    let ip = server_info.ip.lock().unwrap().clone();
    if port == 0 || ip.is_empty() || ip == "0.0.0.0" {
        return Err("Server not running or IP not detected".to_string());
    }

    let batch_id = uuid::Uuid::new_v4().to_string();
    let total = files.len();
    let name = batch_name
        .unwrap_or_else(|| format!("{} 个文件", total))
        .chars()
        .filter(|c| !c.is_control())
        .take(120)
        .collect::<String>();

    for (index, (file_path, file_size)) in files.into_iter().enumerate() {
        let msg_type = transfer_message_type(&file_path);
        let batch = FileBatchMetadata {
            id: batch_id.clone(),
            name: name.clone(),
            index,
            total,
            total_size,
        };
        append_message_with_batch(
            &app_handle,
            "out",
            msg_type,
            &file_path,
            "pc",
            "电脑",
            Some(&file_path),
            Some(&batch),
            Some(file_size),
        );
    }

    Ok(batch_id)
}

fn transfer_message_type(file_path: &str) -> &'static str {
    let path_lower = file_path.to_lowercase();
    if [
        ".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".svg", ".ico",
    ]
    .iter()
    .any(|ext| path_lower.ends_with(ext))
    {
        "image"
    } else if [".mp4", ".mkv", ".avi", ".mov", ".wmv", ".flv", ".webm"]
        .iter()
        .any(|ext| path_lower.ends_with(ext))
    {
        "video"
    } else {
        "file"
    }
}

#[tauri::command]
pub fn save_temp_image(app_handle: AppHandle, base64_data: String) -> Result<String, String> {
    use std::io::Write;
    let b64 = if let Some(idx) = base64_data.find(',') {
        &base64_data[idx + 1..]
    } else {
        &base64_data
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| format!("Base64 Error: {}", e))?;
    let save_dir = app_handle
        .path()
        .download_dir()
        .unwrap_or_else(|_| std::env::temp_dir());
    if !save_dir.exists() {
        let _ = std::fs::create_dir_all(&save_dir);
    }
    let filename = format!("paste_{}.png", chrono::Utc::now().format("%Y%m%d%H%M%S%f"));
    let path = save_dir.join(&filename);
    let mut file = std::fs::File::create(&path).map_err(|e| format!("File Error: {}", e))?;
    file.write_all(&bytes)
        .map_err(|e| format!("Write Error: {}", e))?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn get_download_url(app_handle: AppHandle, file_path: String) -> Result<String, String> {
    if !std::path::Path::new(&file_path).is_file() {
        return Err("File does not exist or is not a regular file".to_string());
    }

    let port = {
        let server_info = app_handle.state::<ServerInfo>();
        let p = server_info.port.load(Ordering::Relaxed);
        if p == 0 {
            match toggle_file_server(app_handle.clone(), true, None).await {
                Ok(p_str) => match p_str.parse::<u16>() {
                    Ok(port) => port,
                    // Another caller is starting (or just started) it.
                    Err(_) => wait_for_server_port(&app_handle).await,
                },
                Err(_) => 0,
            }
        } else {
            p
        }
    };
    if port == 0 {
        return Err("Failed to start server".to_string());
    }
    let token = register_shared_file(&app_handle, file_path.clone())
        .ok_or_else(|| "Failed to register shared file".to_string())?;
    let filename = std::path::Path::new(&file_path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    Ok(format!(
        "http://127.0.0.1:{}/download/{}?name={}",
        port,
        token,
        urlencoding::encode(&filename)
    ))
}

/// Leftovers that nothing cleans up otherwise: `.tmp_*` parts of uploads that
/// never finished (aborted, app quit or crashed) in the transfer folder, and
/// files phones shared into the temp folder. Only files old enough that no
/// live upload or recent chat message can still need them.
fn sweep_stale_transfer_files(app_handle: &AppHandle) {
    const PARTIAL_AFTER: Duration = Duration::from_secs(24 * 60 * 60);
    const SHARED_AFTER: Duration = Duration::from_secs(7 * 24 * 60 * 60);
    let save_dir = std::path::PathBuf::from(get_active_file_transfer_path(app_handle.clone()));
    let shared_dir = std::env::temp_dir().join("tiez_shared_uploads");
    for (dir, partial_only, max_age) in [(save_dir, true, PARTIAL_AFTER), (shared_dir, false, SHARED_AFTER)] {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if partial_only && !entry.file_name().to_string_lossy().starts_with(".tmp_") {
                continue;
            }
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            let stale = meta
                .modified()
                .ok()
                .and_then(|modified| modified.elapsed().ok())
                .is_some_and(|age| age > max_age);
            if meta.is_file() && stale {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

async fn wait_for_server_port(app_handle: &AppHandle) -> u16 {
    for _ in 0..30 {
        let port = app_handle.state::<ServerInfo>().port.load(Ordering::SeqCst);
        if port != 0 {
            return port;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    0
}

#[tauri::command]
pub async fn toggle_file_server(
    app_handle: AppHandle,
    enabled: bool,
    port: Option<u16>,
) -> Result<String, String> {
    if enabled {
        if SERVER_STARTING.swap(true, Ordering::SeqCst) {
            return Ok("Server already starting".to_string());
        }
        let _starting = StartingGuard;
        {
            let handle = SERVER_HANDLE.lock().unwrap();
            if handle.is_some() {
                return Ok("Server already running".to_string());
            }
        }
        let target_port = port.unwrap_or(28462);
        let (listener, actual_port) = bind_listener(target_port).await;
        if actual_port == 0 {
            return Err("Failed to bind any port".to_string());
        }
        let actual_ip = get_local_ip_addr(app_handle.clone());
        let server_info = app_handle.state::<ServerInfo>();
        let access_token = uuid::Uuid::new_v4().to_string();
        let Some(lan_key) = sealed::generate_key() else {
            return Err("Failed to create the transfer key".to_string());
        };
        *server_info.lan_key.lock().unwrap() = Some(lan_key);
        server_info.port.store(actual_port, Ordering::SeqCst);
        {
            let mut ip_guard = server_info.ip.lock().unwrap();
            *ip_guard = actual_ip.clone();
        }
        {
            let mut token_guard = server_info.access_token.lock().unwrap();
            *token_guard = access_token.clone();
        }
        let app_handle_clone = app_handle.clone();
        let h = tokio::spawn(async move {
            run_server(listener, app_handle_clone).await;
        });
        let sweep_app = app_handle.clone();
        tokio::task::spawn_blocking(move || sweep_stale_transfer_files(&sweep_app));
        // Idle time counts from the start, not from some earlier session.
        utils::update_activity(&app_handle);
        spawn_idle_watch(app_handle.clone());
        {
            let mut handle = SERVER_HANDLE.lock().unwrap();
            *handle = Some(h);
        }
        let _ = app_handle.emit(
            "file-server-status-changed",
            StatusPayload {
                enabled: true,
                port: actual_port,
                ip: if actual_ip.is_empty() || actual_ip.contains("0.0.0.0") {
                    "127.0.0.1".to_string()
                } else {
                    actual_ip
                },
                access_token,
                lan_key: sealed::encode_key(&lan_key),
            },
        );
        let db_state = app_handle.state::<DbState>();
        let _ = db_state.settings_repo.set("file_server_enabled", "true");
        let _ = db_state
            .settings_repo
            .set("file_server_port", &actual_port.to_string());
        Ok(format!("{}", actual_port))
    } else {
        let mut handle = SERVER_HANDLE.lock().unwrap();
        if let Some(h) = handle.take() {
            h.abort();
            if let Some(shutdown) = SERVER_SHUTDOWN.lock().unwrap().take() {
                let _ = shutdown.send(true);
            }
            if let Some(ws_state) = app_handle.try_state::<WsBroadcaster>() {
                if let Ok(mut guard) = ws_state.0.lock() {
                    *guard = None;
                }
            }
            let db_state = app_handle.state::<DbState>();
            let _ = db_state.settings_repo.set("file_server_enabled", "false");
            let server_info = app_handle.state::<ServerInfo>();
            server_info.port.store(0, Ordering::SeqCst);
            {
                let mut ip_guard = server_info.ip.lock().unwrap();
                *ip_guard = String::new();
            }
            {
                let mut token_guard = server_info.access_token.lock().unwrap();
                *token_guard = String::new();
            }
            *server_info.lan_key.lock().unwrap() = None;
            clear_shared_files(&app_handle);
            clear_upload_sessions(&app_handle);
            // Connections may be dropped with the server before they clean up.
            if let Some(devices) = app_handle.try_state::<OnlineDevices>() {
                devices.0.lock().unwrap().clear();
            }
            let _ = app_handle.emit("online-devices-updated", Vec::<DeviceInfo>::new());
            let _ = app_handle.emit(
                "file-server-status-changed",
                StatusPayload {
                    enabled: false,
                    port: 0,
                    ip: String::new(),
                    access_token: String::new(),
                    lan_key: String::new(),
                },
            );
            Ok("Server stopped".to_string())
        } else {
            Ok("Server not running".to_string())
        }
    }
}

pub async fn run_server(listener: tokio::net::TcpListener, app_handle: AppHandle) {
    let (ws_tx, _) = broadcast::channel::<String>(100);
    {
        let ws_state = app_handle.state::<WsBroadcaster>();
        if let Ok(mut guard) = ws_state.0.lock() {
            *guard = Some(ws_tx.clone());
        };
    }
    let (shutdown_tx, shutdown) = tokio::sync::watch::channel(false);
    *SERVER_SHUTDOWN.lock().unwrap() = Some(shutdown_tx);
    let lan_key = app_handle
        .state::<ServerInfo>()
        .lan_key
        .lock()
        .unwrap()
        .or_else(sealed::generate_key)
        .unwrap_or([0u8; sealed::KEY_LEN]);
    let state = Arc::new(AppState {
        app_handle: app_handle.clone(),
        ws_tx: ws_tx.clone(),
        shutdown,
        lan_key,
    });
    let protected_routes = Router::new()
        .route("/", get(handlers::index))
        .route("/ws", get(handlers::ws_handler))
        .route("/poll", get(handlers::poll_messages))
        .route("/messages", get(handlers::poll_messages))
        .route("/upload", post(handlers::upload))
        .route("/upload_chunk", post(handlers::upload_chunk))
        .route("/upload-chunk", post(handlers::upload_chunk))
        .route("/upload-chunk-base64", post(handlers::upload_chunk_base64))
        .route("/share_chunk", post(handlers::share_chunk))
        .route("/share-chunk", post(handlers::share_chunk))
        .route("/share_chunk", options(handlers::share_chunk_options))
        .route("/share-chunk", options(handlers::share_chunk_options))
        .route("/send_text", post(handlers::handle_text))
        .route("/send-text", post(handlers::handle_text))
        .route(
            "/download-batch/{batch_id}",
            post(handlers::prepare_batch_download),
        )
        // Encrypted counterparts (sealed.rs). Downloads here also need the
        // session: they are fetched by script, not by native <img>/<video>.
        .route("/s/poll", get(handlers::sealed_poll))
        .route("/s/send-text", post(handlers::sealed_text))
        .route("/s/upload-chunk", post(handlers::sealed_upload_chunk))
        .route("/s/download/{token}", get(handlers::sealed_download))
        .route(
            "/s/download-batch/{batch_id}",
            post(handlers::sealed_batch_download),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            handlers::require_session,
        ))
        // Outermost, so preflights and auth failures carry it too.
        .layer(middleware::from_fn(handlers::app_origin_cors));

    // A download token is an unguessable, path-scoped capability with its own expiry.
    // Keeping this route outside the session middleware lets native <img>/<video>
    // elements load previews without exposing the broader transfer-session secret.
    let download_route = Router::new()
        .route(
            "/download/{token}",
            get(handlers::handle_file_download_proxy),
        )
        .layer(middleware::from_fn(
            handlers::apply_download_security_headers,
        ));
    let app = Router::new()
        .merge(download_route)
        .merge(protected_routes)
        .with_state(state.clone())
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024));

    if let Err(e) = axum::serve(listener, app).await {
        eprintln!("Server error: {}", e);
    }

    let server_info = app_handle.state::<ServerInfo>();
    server_info.port.store(0, Ordering::SeqCst);
    {
        let mut ip_guard = server_info.ip.lock().unwrap();
        *ip_guard = String::new();
    }
    {
        let mut token_guard = server_info.access_token.lock().unwrap();
        *token_guard = String::new();
    }
    *server_info.lan_key.lock().unwrap() = None;
    clear_shared_files(&app_handle);
    clear_upload_sessions(&app_handle);
    let _ = app_handle.emit(
        "file-server-status-changed",
        StatusPayload {
            enabled: false,
            port: 0,
            ip: String::new(),
            access_token: String::new(),
            lan_key: String::new(),
        },
    );
}

pub fn append_message(
    app: &AppHandle,
    direction: &str,
    msg_type: &str,
    content: &str,
    sender_id: &str,
    sender_name: &str,
    file_path: Option<&str>,
) {
    append_message_with_batch(
        app,
        direction,
        msg_type,
        content,
        sender_id,
        sender_name,
        file_path,
        None,
        file_path.and_then(|path| std::fs::metadata(path).ok().map(|metadata| metadata.len())),
    );
}

pub fn append_message_with_batch(
    app: &AppHandle,
    direction: &str,
    msg_type: &str,
    content: &str,
    sender_id: &str,
    sender_name: &str,
    file_path: Option<&str>,
    batch: Option<&FileBatchMetadata>,
    file_size: Option<u64>,
) {
    let chat_state = app.state::<ChatState>();
    let mut msgs = match chat_state.0.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    let id = NEXT_CHAT_MESSAGE_ID.fetch_add(1, Ordering::Relaxed);
    let mut final_content = content.to_string();
    if (msg_type == "image" || msg_type == "video" || msg_type == "file")
        && !final_content.starts_with("data:")
        && !final_content.starts_with("/download/")
    {
        if let Some(path) = file_path {
            if let Some(token) = register_shared_file(app, path.to_string()) {
                let filename = std::path::Path::new(path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                final_content = format!(
                    "/download/{}?name={}",
                    token,
                    urlencoding::encode(&filename)
                );
            }
        }
    }
    let msg = Message {
        id,
        direction: direction.to_string(),
        msg_type: msg_type.to_string(),
        content: final_content,
        timestamp: chrono::Utc::now().timestamp_millis(),
        sender_id: sender_id.to_string(),
        sender_name: sender_name.to_string(),
        file_path: file_path.map(|s| s.to_string()),
        batch_id: batch.map(|value| value.id.clone()),
        batch_name: batch.map(|value| value.name.clone()),
        batch_index: batch.map(|value| value.index),
        batch_total: batch.map(|value| value.total),
        batch_size: batch.map(|value| value.total_size),
        file_size,
    };
    msgs.push(msg.clone());
    if msgs.len() > MAX_CHAT_MESSAGES {
        let excess = msgs.len() - MAX_CHAT_MESSAGES;
        msgs.drain(..excess);
    }
    drop(msgs);
    if let Some(ws_state) = app.try_state::<WsBroadcaster>() {
        if let Ok(guard) = ws_state.0.lock() {
            if let Some(tx) = guard.as_ref() {
                let mut public_msg = msg.clone();
                public_msg.file_path = None;
                let _ = tx.send(serde_json::to_string(&public_msg).unwrap_or_default());
            }
        }
    }
    let _ = app.emit("new-chat-message", msg);
}

pub async fn register_received_file(
    app_handle: &AppHandle,
    final_path: std::path::PathBuf,
    file_name: String,
    content_type: String,
    sender_id: String,
    sender_name: String,
    batch: Option<FileBatchMetadata>,
    file_size: Option<u64>,
) {
    let settings = app_handle.state::<SettingsState>();
    let session_hist = app_handle.state::<SessionHistory>();
    let is_image = content_type.starts_with("image/");
    let file_name_lower = file_name.to_lowercase();
    let is_video = content_type.starts_with("video/")
        || [".mp4", ".mov", ".mkv", ".avi", ".wmv", ".flv", ".webm"]
            .iter()
            .any(|ext| file_name_lower.ends_with(ext));
    let saved_path = final_path.to_string_lossy().to_string();
    let mut preview = format!("[File] {}", file_name);
    let mut type_enum = "file";
    if is_image {
        preview = "[Image]".to_string();
        type_enum = "image";
    } else if is_video {
        preview = "[Video]".to_string();
        type_enum = "video";
    }
    append_message_with_batch(
        app_handle,
        "in",
        type_enum,
        &saved_path,
        &sender_id,
        &sender_name,
        Some(&saved_path),
        batch.as_ref(),
        file_size,
    );
    if settings.auto_copy_file.load(Ordering::Relaxed) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let entry = ClipboardEntry {
            id: 0,
            content_type: type_enum.to_string(),
            content: saved_path.clone(),
            html_content: None,
            source_app: "File Transfer".to_string(),
            source_app_path: None,
            timestamp,
            created_at: timestamp,
            last_used_at: 0,
            sort_at: timestamp,
            preview,
            is_pinned: false,
            tags: Vec::new(),
            note: String::new(),
            use_count: 0,
            is_external: false,
            pinned_order: 0,
            file_preview_exists: true,
        };
        if settings.persistent.load(Ordering::Relaxed) {
            let db_state = app_handle.state::<DbState>();
            if let Ok(id) = db_state.repo.save(&entry, None) {
                if id != 0 {
                    let _ = app_handle.emit("clipboard-changed", id);
                }
            }
        } else {
            let id = -(SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_micros() as i64
                / 1000);
            let mut entry_mem = entry;
            entry_mem.id = id;
            if let Ok(mut session) = session_hist.0.lock() {
                session.push_back(entry_mem);
                if session.len() > 500 {
                    if let Some(removed) = session.pop_front() {
                        let _ = app_handle.emit("clipboard-removed", removed.id);
                    }
                }
            }
            let _ = app_handle.emit("clipboard-changed", id);
        }
    }
    if settings.auto_copy_file.load(Ordering::Relaxed) {
        #[cfg(target_os = "windows")]
        unsafe {
            let _ = crate::infrastructure::windows_api::win_clipboard::set_clipboard_files(vec![
                saved_path.clone(),
            ]);
        }

        #[cfg(target_os = "macos")]
        {
            let _ = crate::infrastructure::macos_api::clipboard::set_clipboard_files(vec![
                saved_path.clone(),
            ]);
        }

        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        {
            // Keep the existing path-text fallback where no native file pasteboard adapter exists.
            if let Ok(mut cb) = arboard::Clipboard::new() {
                let _ = cb.set_text(saved_path.clone());
            }
        }
    }
    let db_state = app_handle.state::<DbState>();
    if let Ok(Some(val)) = db_state.settings_repo.get("file_transfer_auto_open") {
        if val == "true" {
            let parent = final_path.parent().unwrap_or(std::path::Path::new("."));
            #[cfg(target_os = "windows")]
            let _ = std::process::Command::new("explorer").arg(parent).spawn();
            #[cfg(target_os = "macos")]
            let _ = std::process::Command::new("open").arg(parent).spawn();
            #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
            let _ = std::process::Command::new("xdg-open").arg(parent).spawn();
        }
    }
}

#[tauri::command]
pub fn get_file_server_status(app_handle: AppHandle) -> StatusPayload {
    let server_info = app_handle.state::<ServerInfo>();
    let port = server_info.port.load(Ordering::Relaxed);
    let ip = server_info.ip.lock().unwrap().clone();
    let access_token = server_info.access_token.lock().unwrap().clone();
    StatusPayload {
        enabled: port != 0,
        port,
        ip,
        access_token,
        lan_key: lan_key_text(&server_info),
    }
}
