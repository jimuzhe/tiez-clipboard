// Content handler module for opening various content types
#[cfg(target_os = "windows")]
use crate::infrastructure::windows_api::apps::launch_uwp_with_file;
use crate::database::DbState;
use crate::infrastructure::repository::settings_repo::SettingsRepository;
use crate::infrastructure::repository::clipboard_repo::ClipboardRepository;
use crate::error::AppError;
use base64::{engine::general_purpose, Engine as _};
use std::io::Read;
use std::collections::HashMap;
use std::process::Command;
use std::sync::{mpsc, OnceLock};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use tauri::{Emitter, Manager, State};

#[tauri::command]
pub async fn open_content(
    app_handle: tauri::AppHandle,
    state: State<'_, DbState>,
    id: i64,
    mut content: String,
    content_type: String,
) -> Result<(), AppError> {
    // 0. Resolve full content if ID is provided and content is placeholder/truncated
    if id != 0 {
        if id > 0 {
            // Fetch from Database
            if let Ok(Some(full_content)) = state.repo.get_entry_content(id) {
                content = full_content;
            }
        } else {
            // Fetch from Session
            let session = app_handle.state::<crate::app_state::SessionHistory>();
            let session_items = session.0.lock().unwrap();
            if let Some(item) = session_items.iter().find(|i| i.id == id) {
                content = item.content.clone();
            }
        }
    }

    // Increment use count if ID is provided
    if id > 0 {
        let _ = state.repo.increment_use_count(id);
    }

    let app_path = get_app_path_for_content_type(&state, &content_type)?;
    let mut temp_path = std::env::temp_dir();
    let filename = format!("TieZ_Clip_{}", chrono::Utc::now().format("%Y%m%d-%H%M%S"));
    let mut use_direct_path = false;

    // Handle links/URLs
    if content_type == "link" || content_type == "url" {
        return handle_url_content(&app_path, &content).await;
    }

    // Check if content points to existing file(s)
    if is_file_type(&content_type) && !content.starts_with("data:image") {
        if let Some(path) = get_existing_file_path(&content) {
            temp_path = path;
            use_direct_path = true;
        }
    }

    // Create temp file if needed
    if !use_direct_path {
        temp_path = create_temp_file(&content, &content_type, &filename, temp_path)?;
    }

    let path_str = temp_path.to_str().unwrap().to_string();
    let file_path_clone = temp_path.clone();

    // Launch the file with appropriate application
    launch_file_with_app(&app_path, &temp_path, &path_str, &content_type, use_direct_path).await?;

    // Start background watcher ONLY if we created a temp file
    if !use_direct_path {
        start_file_watcher(app_handle, file_path_clone, content_type, id);
    }

    Ok(())
}

fn get_app_path_for_content_type(
    state: &State<'_, DbState>,
    content_type: &str,
) -> Result<Option<String>, AppError> {
    let setting_key = format!("app.{}", content_type);
    let mut val = state.settings_repo.get(&setting_key)
        .map_err(AppError::from)?
        .filter(|s| !s.trim().is_empty());

    // Fallback: If 'code' app is not set, use 'text' app
    if val.is_none() && content_type == "code" {
        val = state.settings_repo.get("app.text")
            .map_err(AppError::from)?
            .filter(|s| !s.trim().is_empty());
    }
    Ok(val)
}


async fn handle_url_content(app_path: &Option<String>, content: &str) -> Result<(), AppError> {
    if let Some(app) = app_path {
        if std::path::Path::new(app).exists() {
            Command::new(app)
                .arg(content)
                .spawn()
                .map_err(|e| AppError::Internal(format!("启动程序失败: {}", e)))?;
            return Ok(());
        } else {
            // Check for macOS-style paths on Windows to avoid invalid Start-Process calls
            #[cfg(target_os = "windows")]
            if app.starts_with("/Applications/") || app.contains(".app") {
                return launch_default_handler(content).await;
            }

            println!("Attempting to launch URL handler: {}", app);
            let ps_script = format!(
                "Start-Process -FilePath 'shell:AppsFolder\\{}' -ArgumentList '{}'",
                app,
                content.replace("'", "''")
            );

            #[cfg(target_os = "windows")]
            {
                let mut cmd = Command::new("powershell");
                cmd.args(["-NoProfile", "-Command", &ps_script])
                    .creation_flags(0x08000000);
                
                match cmd.spawn() {
                    Ok(_) => return Ok(()),
                    Err(e) => {
                        println!("Failed to launch via powershell: {}, falling back to default", e);
                        return launch_default_handler(content).await;
                    }
                }
            }
            
            #[cfg(not(target_os = "windows"))]
            {
                return launch_default_handler(content).await;
            }
        }
    } else {
        return launch_default_handler(content).await;
    }
}

async fn launch_default_handler(content: &str) -> Result<(), AppError> {
    #[cfg(target_os = "windows")]
    {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", "start", "", content])
            .creation_flags(0x08000000);
        cmd.spawn().map_err(|e| AppError::Internal(format!("启动默认浏览器失败: {}", e)))?;
    }
    
    #[cfg(not(target_os = "windows"))]
    {
        Command::new("open").arg(content).spawn().map_err(|e| AppError::Internal(format!("启动默认浏览器失败: {}", e)))?;
    }
    Ok(())
}

fn is_file_type(content_type: &str) -> bool {
    matches!(content_type, "file" | "video" | "image")
}

fn get_existing_file_path(content: &str) -> Option<std::path::PathBuf> {
    let first_line = content.lines().next().unwrap_or(content).trim();
    let path = std::path::Path::new(first_line);
    if path.exists() {
        Some(path.to_path_buf())
    } else {
        None
    }
}

fn create_temp_file(
    content: &str,
    content_type: &str,
    filename: &str,
    mut temp_path: std::path::PathBuf,
) -> Result<std::path::PathBuf, AppError> {
    match content_type {
        "image" => {
            let is_gif = content.contains("image/gif");
            let extension = if is_gif { "gif" } else { "png" };
            temp_path.push(format!("{}.{}", filename, extension));
            
            let b64_data = if content.starts_with("data:image") {
                content.split(',').nth(1).unwrap_or(content)
            } else {
                content
            };
            let bytes = general_purpose::STANDARD
                .decode(b64_data)
                .map_err(|e| e.to_string())?;

            if is_gif {
                // For GIFs, write raw bytes directly to preserve animation
                std::fs::write(&temp_path, &bytes).map_err(AppError::from)?;
            } else {
                // For other images, use image crate to ensure standard peak format
                let img = image::load_from_memory(&bytes).map_err(AppError::from)?;
                img.save(&temp_path).map_err(AppError::from)?;
            }
        }
        _ => {
            temp_path.push(format!("{}.txt", filename));
            let mut file = std::fs::File::create(&temp_path).map_err(AppError::from)?;
            use std::io::Write;
            file.write_all(content.as_bytes())
                .map_err(|e| AppError::IO(e.to_string()))?;
        }
    }
    Ok(temp_path)
}

async fn launch_file_with_app(
    app_path: &Option<String>,
    temp_path: &std::path::Path,
    path_str: &str,
    content_type: &str,
    use_direct_path: bool,
) -> Result<(), AppError> {
    if let Some(app) = app_path {
        if std::path::Path::new(app).exists() {
            Command::new(app)
                .arg(temp_path)
                .spawn()
                .map_err(|e| format!("启动程序失败: {}", e))?;
        } else {
            // macOS style apps on Windows should fall back to default
            #[cfg(target_os = "windows")]
            if app.starts_with("/Applications/") || app.contains(".app") {
                return launch_with_default_app(path_str, content_type, use_direct_path);
            }

            let safe_path = path_str.replace("'", "''");
            // Try UWP launch on Windows
            #[cfg(target_os = "windows")]
            {
                println!("Attempting to launch UWP app: {} for file: {}", app, path_str);
                if let Err(e) = launch_uwp_with_file(app, path_str) {
                    println!("WinRT launch failed: {}, falling back to old method", e);
                    let ps_script = format!(
                        "Start-Process -FilePath 'shell:AppsFolder\\{}' -ArgumentList '{}'",
                        app, safe_path
                    );
                    let mut cmd = Command::new("powershell");
                    cmd.args(["-NoProfile", "-Command", &ps_script])
                        .creation_flags(0x08000000);
                    match cmd.spawn() {
                        Ok(_) => return Ok(()),
                        Err(err) => {
                            println!("Fallback launch failed: {}, using system default", err);
                            return launch_with_default_app(path_str, content_type, use_direct_path);
                        }
                    }
                }
            }
            // Non-Windows fallback
            #[cfg(not(target_os = "windows"))]
            {
                Command::new("open").arg(safe_path).spawn().map_err(|e| AppError::Internal(format!("启动 UWP 程序失败 (Fallback): {}", e)))?;
            }
        }
    } else {
        launch_with_default_app(path_str, content_type, use_direct_path)?;
    }
    Ok(())
}

fn launch_with_default_app(
    path_str: &str,
    _content_type: &str,
    _use_direct_path: bool,
) -> Result<(), AppError> {

    #[cfg(target_os = "windows")]
    {
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOW;
        use windows::core::{HSTRING, PCWSTR};

        let operation = HSTRING::from("open");
        let file = HSTRING::from(path_str);

        let ret = unsafe {
            ShellExecuteW(
                None,
                PCWSTR::from_raw(operation.as_ptr()),
                PCWSTR::from_raw(file.as_ptr()),
                None,
                None,
                SW_SHOW,
            )
        };

        if (ret.0 as isize) <= 32 {
            return Err(AppError::Internal(format!(
                "Failed to open file (ShellExecute error code: {})",
                ret.0 as isize
            )));
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        Command::new("open").arg(path_str).spawn().map_err(|e| AppError::Internal(format!("Failed to open file: {}", e)))?;
    }

    Ok(())
}

struct WatchEntry {
    app_handle: tauri::AppHandle,
    file_path: std::path::PathBuf,
    content_type: String,
    id: i64,
    last_mtime: Option<std::time::SystemTime>,
    start_time: std::time::Instant,
}

enum FileWatcherCommand {
    Watch {
        app_handle: tauri::AppHandle,
        file_path: std::path::PathBuf,
        content_type: String,
        id: i64,
    },
}

fn watcher_sender() -> &'static mpsc::Sender<FileWatcherCommand> {
    static WATCHER_SENDER: OnceLock<mpsc::Sender<FileWatcherCommand>> = OnceLock::new();
    WATCHER_SENDER.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<FileWatcherCommand>();
        let _ = std::thread::Builder::new()
            .name("content-file-watcher".to_string())
            .spawn(move || run_file_watcher_loop(rx));
        tx
    })
}

fn run_file_watcher_loop(rx: mpsc::Receiver<FileWatcherCommand>) {
    let mut watchers: HashMap<String, WatchEntry> = HashMap::new();
    loop {
        match rx.recv_timeout(std::time::Duration::from_secs(2)) {
            Ok(FileWatcherCommand::Watch { app_handle, file_path, content_type, id }) => {
                let key = file_path.to_string_lossy().to_string();
                let last_mtime = std::fs::metadata(&file_path)
                    .ok()
                    .and_then(|m| m.modified().ok());
                watchers.insert(
                    key,
                    WatchEntry {
                        app_handle,
                        file_path,
                        content_type,
                        id,
                        last_mtime,
                        start_time: std::time::Instant::now(),
                    },
                );
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        if watchers.is_empty() {
            continue;
        }

        let mut remove_keys = Vec::new();
        for (key, entry) in watchers.iter_mut() {
            if entry.start_time.elapsed().as_secs() >= 3600 {
                remove_keys.push(key.clone());
                continue;
            }
            if !entry.file_path.exists() {
                remove_keys.push(key.clone());
                continue;
            }
            let current_mtime = std::fs::metadata(&entry.file_path)
                .ok()
                .and_then(|m| m.modified().ok());
            if current_mtime != entry.last_mtime {
                entry.last_mtime = current_mtime;
                if let Some((new_content, preview)) =
                    read_changed_file(&entry.file_path, &entry.content_type)
                {
                    update_database_with_changes(&entry.app_handle, entry.id, &new_content, &preview);
                }
            }
        }
        for key in remove_keys {
            watchers.remove(&key);
        }
    }
}

fn start_file_watcher(
    app_handle: tauri::AppHandle,
    file_path: std::path::PathBuf,
    content_type: String,
    id: i64,
) {
    let _ = watcher_sender().send(FileWatcherCommand::Watch {
        app_handle,
        file_path,
        content_type,
        id,
    });
}

fn read_changed_file(
    file_path: &std::path::Path,
    content_type: &str,
) -> Option<(String, String)> {
    let mut new_content = String::new();
    let success = if content_type == "image" {
        read_image_file(file_path, &mut new_content)
    } else {
        read_text_file(file_path, &mut new_content)
    };

    if success {
        let preview = generate_preview(&new_content);
        Some((new_content, preview))
    } else {
        None
    }
}

fn read_image_file(file_path: &std::path::Path, new_content: &mut String) -> bool {
    if let Ok(mut f) = std::fs::File::open(file_path) {
        let mut buffer = Vec::new();
        if f.read_to_end(&mut buffer).is_ok() {
            // Preserve animated GIFs
            let is_gif_header = buffer.len() >= 6
                && (buffer.starts_with(b"GIF87a") || buffer.starts_with(b"GIF89a"));
            let is_gif_ext = file_path
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.eq_ignore_ascii_case("gif"))
                .unwrap_or(false);

            if is_gif_header || is_gif_ext {
                let b64 = general_purpose::STANDARD.encode(&buffer);
                *new_content = format!("data:image/gif;base64,{}", b64);
                return true;
            }

            if let Ok(img) = image::load_from_memory(&buffer) {
                use std::io::Cursor;
                let mut bytes: Vec<u8> = Vec::new();
                if img
                    .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
                    .is_ok()
                {
                    let b64 = general_purpose::STANDARD.encode(&bytes);
                    *new_content = format!("data:image/png;base64,{}", b64);
                    return true;
                }
            }
        }
    }
    false
}

fn read_text_file(file_path: &std::path::Path, new_content: &mut String) -> bool {
    if let Ok(mut f) = std::fs::File::open(file_path) {
        f.read_to_string(new_content).is_ok()
    } else {
        false
    }
}

fn generate_preview(content: &str) -> String {
    if content.starts_with("data:image") {
        "[New Image]".to_string()
    } else if content.chars().count() > 500 {
        let preview_text: String = content.chars().take(497).collect();
        format!("{}...", preview_text.replace('\n', " "))
    } else {
        content.replace('\n', " ")
    }
}

fn update_database_with_changes(
    app_handle: &tauri::AppHandle,
    id: i64,
    new_content: &str,
    preview: &str,
) {
    use crate::app_state::SessionHistory;

    if id < 0 {
        if let Some(session) = app_handle.try_state::<SessionHistory>() {
            let mut history = session.0.lock().unwrap();
            if let Some(item) = history.iter_mut().find(|i| i.id == id) {
                item.content = new_content.to_string();
                item.preview = preview.to_string();
                item.html_content = None;
                if item.content_type == "rich_text" {
                    item.content_type = "text".to_string();
                }
                let _ = app_handle.emit("clipboard-updated", item.clone());
                println!("Session item updated and clipboard-updated event emitted for id: {}", id);
                return;
            }
        }
        let _ = app_handle.emit("clipboard-changed", id);
        return;
    }

    let state = app_handle.state::<DbState>();

    if state.repo.update_entry_content(id, new_content, preview).is_ok() {
        if let Some(session) = app_handle.try_state::<SessionHistory>() {
            let mut history = session.0.lock().unwrap();
            if let Some(item) = history.iter_mut().find(|i| i.id == id) {
                item.content = new_content.to_string();
                item.preview = preview.to_string();
                item.html_content = None;
                if item.content_type == "rich_text" {
                    item.content_type = "text".to_string();
                }
            }
        }

        if let Ok(Some(updated_entry)) = state.repo.get_entry_by_id(id) {
            let _ = app_handle.emit("clipboard-updated", updated_entry);
            println!("Database updated and clipboard-updated event emitted for id: {}", id);
        } else {
            let _ = app_handle.emit("clipboard-changed", id);
            println!("Database updated for id: {}", id);
        }
    }
}
