// Content handler module for opening various content types
use crate::database::DbState;
use crate::error::AppError;
use crate::infrastructure::repository::clipboard_repo::ClipboardRepository;
use crate::infrastructure::repository::settings_repo::SettingsRepository;
use crate::infrastructure::windows_api::apps::launch_uwp_with_file;
use base64::{engine::general_purpose, Engine as _};
use std::io::Read;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::process::Command;
use tauri::{Emitter, Manager, State};

#[tauri::command]
pub async fn open_content(
    app_handle: tauri::AppHandle,
    state: State<'_, DbState>,
    id: i64,
    mut content: String,
    content_type: String,
) -> Result<(), AppError> {
    let mut html_content: Option<String> = None;

    // 0. Resolve full content if ID is provided and content is placeholder/truncated
    if id != 0 {
        if id > 0 {
            // Fetch from Database
            if content_type == "rich_text" {
                if let Ok(Some((full_content, _, html))) =
                    state.repo.get_entry_content_with_html(id)
                {
                    content = full_content;
                    html_content = html;
                }
            } else if let Ok(Some(full_content)) = state.repo.get_entry_content(id) {
                content = full_content;
            }
        } else {
            // Fetch from Session
            let session = app_handle.state::<crate::app_state::SessionHistory>();
            let session_items = session.0.lock().unwrap();
            if let Some(item) = session_items.iter().find(|i| i.id == id) {
                content = item.content.clone();
                html_content = item.html_content.clone();
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
        temp_path = create_temp_file(
            &content,
            &content_type,
            html_content.as_deref(),
            &filename,
            temp_path,
        )?;
    }

    let path_str = temp_path.to_str().unwrap().to_string();
    let file_path_clone = temp_path.clone();

    // Launch the file with appropriate application
    launch_file_with_app(
        &app_path,
        &temp_path,
        &path_str,
        &content_type,
        use_direct_path,
    )
    .await?;

    // Start background watcher ONLY if we created a temp file
    if !use_direct_path && content_type != "rich_text" {
        start_file_watcher(app_handle, file_path_clone, content_type, id);
    }

    Ok(())
}

fn get_app_path_for_content_type(
    state: &State<'_, DbState>,
    content_type: &str,
) -> Result<Option<String>, AppError> {
    let setting_key = format!("app.{}", content_type);
    let mut val = state
        .settings_repo
        .get(&setting_key)
        .map_err(AppError::from)?
        .filter(|s| !s.trim().is_empty());

    // Fallback: If 'code' app is not set, use 'text' app
    if val.is_none() && content_type == "code" {
        val = state
            .settings_repo
            .get("app.text")
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
                        println!(
                            "Failed to launch via powershell: {}, falling back to default",
                            e
                        );
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
        cmd.spawn()
            .map_err(|e| AppError::Internal(format!("启动默认浏览器失败: {}", e)))?;
    }

    #[cfg(not(target_os = "windows"))]
    {
        Command::new("open")
            .arg(content)
            .spawn()
            .map_err(|e| AppError::Internal(format!("启动默认浏览器失败: {}", e)))?;
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
    html_content: Option<&str>,
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
        "rich_text" => {
            if let Some(html) = html_content.filter(|value| !value.trim().is_empty()) {
                temp_path.push(format!("{}.html", filename));
                let mut file = std::fs::File::create(&temp_path).map_err(AppError::from)?;
                use std::io::Write;
                file.write_all(html.as_bytes())
                    .map_err(|e| AppError::IO(e.to_string()))?;
            } else {
                temp_path.push(format!("{}.txt", filename));
                let mut file = std::fs::File::create(&temp_path).map_err(AppError::from)?;
                use std::io::Write;
                file.write_all(content.as_bytes())
                    .map_err(|e| AppError::IO(e.to_string()))?;
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
            // Check for macOS-style paths on Windows to avoid invalid WinRT/Start-Process calls
            #[cfg(target_os = "windows")]
            if app.starts_with("/Applications/") || app.contains(".app") {
                return launch_with_default_app(path_str, content_type, use_direct_path);
            }

            println!(
                "Attempting to launch UWP app: {} for file: {}",
                app, path_str
            );
            if let Err(e) = launch_uwp_with_file(app, path_str).await {
                println!("WinRT launch failed: {}, falling back to old method", e);
                let safe_path = path_str.replace("'", "''");
                let ps_script = format!(
                    "Start-Process -FilePath 'shell:AppsFolder\\{}' -ArgumentList '{}'",
                    app, safe_path
                );
                #[cfg(target_os = "windows")]
                {
                    let mut cmd = Command::new("powershell");
                    cmd.args(["-NoProfile", "-Command", &ps_script])
                        .creation_flags(0x08000000);
                    match cmd.spawn() {
                        Ok(_) => return Ok(()),
                        Err(err) => {
                            println!("Fallback launch failed: {}, using system default", err);
                            return launch_with_default_app(
                                path_str,
                                content_type,
                                use_direct_path,
                            );
                        }
                    }
                }

                #[cfg(not(target_os = "windows"))]
                Command::new("open").arg(safe_path).spawn().map_err(|e| {
                    AppError::Internal(format!("启动 UWP 程序失败 (Fallback): {}", e))
                })?;
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
        use windows::core::{HSTRING, PCWSTR};
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOW;

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
        Command::new("open")
            .arg(path_str)
            .spawn()
            .map_err(|e| AppError::Internal(format!("Failed to open file: {}", e)))?;
    }

    Ok(())
}

fn start_file_watcher(
    app_handle: tauri::AppHandle,
    file_path: std::path::PathBuf,
    content_type: String,
    id: i64,
) {
    std::thread::spawn(move || {
        let mut last_mtime = std::fs::metadata(&file_path)
            .ok()
            .and_then(|m| m.modified().ok());
        let start_time = std::time::Instant::now();

        // Monitor for 1 hour or until file deleted
        while start_time.elapsed().as_secs() < 3600 {
            std::thread::sleep(std::time::Duration::from_secs(2));

            if !file_path.exists() {
                break;
            }

            let current_mtime = std::fs::metadata(&file_path)
                .ok()
                .and_then(|m| m.modified().ok());

            if current_mtime != last_mtime {
                last_mtime = current_mtime;
                println!("File changed detected: {:?}", file_path);

                if let Some((new_content, preview)) = read_changed_file(&file_path, &content_type) {
                    update_database_with_changes(&app_handle, id, &new_content, &preview);
                }
            }
        }
    });
}

fn read_changed_file(file_path: &std::path::Path, content_type: &str) -> Option<(String, String)> {
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
                println!(
                    "Session item updated and clipboard-updated event emitted for id: {}",
                    id
                );
                return;
            }
        }
        let _ = app_handle.emit("clipboard-changed", id);
        return;
    }

    let state = app_handle.state::<DbState>();

    if state
        .repo
        .update_entry_content(id, new_content, preview)
        .is_ok()
    {
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
            println!(
                "Database updated and clipboard-updated event emitted for id: {}",
                id
            );
            crate::services::cloud_sync::request_cloud_sync(app_handle.clone());
        } else {
            let _ = app_handle.emit("clipboard-changed", id);
            println!("Database updated for id: {}", id);
            crate::services::cloud_sync::request_cloud_sync(app_handle.clone());
        }
    }
}
