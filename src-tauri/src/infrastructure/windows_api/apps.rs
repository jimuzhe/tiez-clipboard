use crate::error::{AppError, AppResult};
use base64::Engine;
use image::{ImageBuffer, Rgba};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ, RGBQUAD,
};
use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL};
use windows::Win32::UI::Shell::{
    AssocQueryStringW, ExtractIconExW, SHGetFileInfoW, ASSOCF_VERIFY, ASSOCSTR_EXECUTABLE,
    ASSOCSTR_FRIENDLYAPPNAME, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGFI_USEFILEATTRIBUTES,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, DrawIconEx, GetSystemMetrics, DI_NORMAL, HICON, SM_CXICON, SM_CXSMICON, SM_CYICON,
    SM_CYSMICON,
};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppInfo {
    pub name: String,
    pub path: String,
}

static EXECUTABLE_ICON_CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
static FILE_ICON_CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();

#[tauri::command]
pub async fn scan_installed_apps() -> AppResult<Vec<AppInfo>> {
    let mut apps = Vec::new();
    println!("Starting app scan...");

    // 1. Add known system apps directly (Backend Fallback)
    let sys_root = std::env::var("SystemRoot").unwrap_or("C:\\Windows".to_string());

    let common_apps = vec![
        (
            "Notepad (记事本)",
            format!(r"{}\System32\notepad.exe", sys_root),
        ),
        (
            "Paint (画图)",
            format!(r"{}\System32\mspaint.exe", sys_root),
        ),
        (
            "Calculator (计算器)",
            format!(r"{}\System32\calc.exe", sys_root),
        ),
        (
            "Command Prompt (CMD)",
            format!(r"{}\System32\cmd.exe", sys_root),
        ),
        (
            "PowerShell",
            format!(
                r"{}\System32\WindowsPowerShell\v1.0\powershell.exe",
                sys_root
            ),
        ),
        ("Registry Editor", format!(r"{}\regedit.exe", sys_root)),
        (
            "Snipping Tool",
            format!(r"{}\System32\SnippingTool.exe", sys_root),
        ),
        ("Explorer", format!(r"{}\explorer.exe", sys_root)),
    ];

    for (name, path) in common_apps {
        if Path::new(&path).exists() {
            apps.push(AppInfo {
                name: name.to_string(),
                path,
            });
        }
    }

    // Check for common browsers
    let program_files = std::env::var("ProgramFiles").unwrap_or(r"C:\Program Files".to_string());
    let program_files_x86 =
        std::env::var("ProgramFiles(x86)").unwrap_or(r"C:\Program Files (x86)".to_string());

    let chrome_path = format!(r"{}\Google\Chrome\Application\chrome.exe", program_files);
    if Path::new(&chrome_path).exists() {
        apps.push(AppInfo {
            name: "Google Chrome".to_string(),
            path: chrome_path,
        });
    }

    let edge_path = format!(
        r"{}\Microsoft\Edge\Application\msedge.exe",
        program_files_x86
    );
    if Path::new(&edge_path).exists() {
        apps.push(AppInfo {
            name: "Microsoft Edge".to_string(),
            path: edge_path,
        });
    }

    // 2. Run PowerShell Scan (Best Effort)
    let ps_script = r#"
        $ErrorActionPreference = 'SilentlyContinue'
        [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
        
        $apps = Get-StartApps | Select-Object Name, AppID
        
        $results = New-Object System.Collections.Generic.List[Object]
        foreach ($app in $apps) {
            if (![string]::IsNullOrEmpty($app.AppID)) {
                $obj = @{ name = $app.Name; path = $app.AppID }
                $results.Add($obj)
            }
        }
        
        if ($results.Count -eq 0) {
            Write-Output "[]"
        } else {
            $results | ConvertTo-Json -Depth 2 -Compress
        }
    "#;

    let output_res = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            ps_script,
        ])
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .output();

    if let Ok(output) = output_res {
        if output.status.success() {
            let json_str = String::from_utf8_lossy(&output.stdout);
            if let Ok(scanned) = serde_json::from_str::<Vec<AppInfo>>(&json_str) {
                apps.extend(scanned);
            } else if let Ok(single) = serde_json::from_str::<AppInfo>(&json_str) {
                apps.push(single);
            }
        }
    }

    // 3. Deduplicate and Filter
    let invalid_keywords = [
        "uninstall",
        "卸载",
        "setup",
        "install",
        "config",
        "help",
        "readme",
        "update",
        "修复",
        "remove",
    ];

    apps.retain(|app| {
        let name_lower = app.name.to_lowercase();
        !invalid_keywords.iter().any(|&k| name_lower.contains(k))
    });

    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps.dedup_by(|a, b| a.path.eq_ignore_ascii_case(&b.path));

    Ok(apps)
}

#[tauri::command]
pub async fn get_associated_apps(extension: String) -> AppResult<Vec<AppInfo>> {
    let ext = if extension.starts_with('.') {
        extension.clone()
    } else {
        format!(".{}", extension)
    };

    let ps_script = format!(
        r#"
        $ErrorActionPreference = 'SilentlyContinue'
        [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
        
        $ext = "{}"
        $list = New-Object System.Collections.Generic.List[Object]
        $addedPaths = New-Object System.Collections.Generic.HashSet[String]

        $regPath = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\$ext\OpenWithList"
        if (Test-Path $regPath) {{
            $mru = Get-ItemProperty $regPath
            $mru.PSObject.Properties | Where-Object {{ $_.Name -match "^[a-zA-Z]$" }} | ForEach-Object {{
                $exeName = $_.Value
                if ($exeName -and $exeName.EndsWith(".exe")) {{
                    try {{
                        $cmd = Get-Command $exeName -ErrorAction SilentlyContinue
                        if ($cmd) {{
                            $fullPath = $cmd.Source
                             if (-not $addedPaths.Contains($fullPath)) {{
                                $list.Add(@{{ name = $cmd.Name; path = $fullPath }})
                                $addedPaths.Add($fullPath) | Out-Null
                             }}
                        }} else {{
                            $appPathKey = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\$exeName"
                            if (Test-Path $appPathKey) {{
                                $fullPath = (Get-ItemProperty $appPathKey).'(default)'
                                if ($fullPath -and (Test-Path $fullPath)) {{
                                    if (-not $addedPaths.Contains($fullPath)) {{
                                        $list.Add(@{{ name = $exeName; path = $fullPath }})
                                        $addedPaths.Add($fullPath) | Out-Null
                                    }}
                                }}
                            }}
                        }}
                    }} catch {{}}
                }}
            }}
        }}

        $list | ConvertTo-Json -Depth 2 -Compress
    "#,
        ext
    );

    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &ps_script,
        ])
        .creation_flags(0x08000000)
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        let json_str = String::from_utf8_lossy(&output.stdout);
        if let Ok(apps) = serde_json::from_str::<Vec<AppInfo>>(&json_str) {
            return Ok(apps);
        }
    }

    Ok(Vec::new())
}

#[tauri::command]
pub fn get_system_default_app(content_type: String) -> AppResult<String> {
    let ext = match content_type.as_str() {
        "image" => ".png",
        "video" => ".mp4",
        "code" => ".txt",
        "text" => ".txt",
        "rich_text" => ".html",
        "file" => ".txt",
        "link" | "url" => "http",
        _ => return Ok("系统默认".to_string()),
    };

    unsafe {
        let mut buffer = [0u16; 1024];
        let mut size = buffer.len() as u32;
        let ext_wide: Vec<u16> = std::ffi::OsStr::new(ext)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let ext_pcwstr = PCWSTR(ext_wide.as_ptr());

        let res = AssocQueryStringW(
            ASSOCF_VERIFY,
            ASSOCSTR_FRIENDLYAPPNAME,
            ext_pcwstr,
            PCWSTR::null(),
            Some(PWSTR(buffer.as_mut_ptr())),
            &mut size,
        );

        if res.is_ok() {
            let len = (0..size as usize)
                .position(|i| buffer[i] == 0)
                .unwrap_or(size as usize);
            let name = String::from_utf16_lossy(&buffer[0..len]);
            if !name.trim().is_empty() {
                return Ok(name);
            }
        }

        // Fallback to executable name
        let mut size = buffer.len() as u32;
        let res = AssocQueryStringW(
            ASSOCF_VERIFY,
            ASSOCSTR_EXECUTABLE,
            ext_pcwstr,
            PCWSTR::null(),
            Some(PWSTR(buffer.as_mut_ptr())),
            &mut size,
        );
        if res.is_ok() {
            let len = (0..size as usize)
                .position(|i| buffer[i] == 0)
                .unwrap_or(size as usize);
            let path_str = String::from_utf16_lossy(&buffer[0..len]);
            if let Some(name) = Path::new(&path_str).file_name() {
                return Ok(name.to_string_lossy().to_string());
            }
        }
    }

    Ok("系统默认".to_string())
}

use std::os::windows::ffi::OsStrExt;

#[tauri::command]
pub fn get_executable_icon(executable_path: String) -> AppResult<Option<String>> {
    let cache_key = normalize_icon_cache_key(&executable_path);
    if cache_key.is_empty() {
        return Ok(None);
    }

    let cache = EXECUTABLE_ICON_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(cached) = cache
        .lock()
        .map_err(|e| AppError::Internal(e.to_string()))?
        .get(&cache_key)
        .cloned()
    {
        return Ok(cached);
    }

    let icon = extract_executable_icon_data_url(&executable_path)?;
    cache
        .lock()
        .map_err(|e| AppError::Internal(e.to_string()))?
        .insert(cache_key, icon.clone());
    Ok(icon)
}

#[tauri::command]
pub fn get_file_icon(file_path: String) -> AppResult<Option<String>> {
    let cache_key = normalize_file_icon_cache_key(&file_path);
    if cache_key.is_empty() {
        return Ok(None);
    }

    let cache = FILE_ICON_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(cached) = cache
        .lock()
        .map_err(|e| AppError::Internal(e.to_string()))?
        .get(&cache_key)
        .cloned()
    {
        return Ok(cached);
    }

    let icon = extract_file_icon_data_url(&file_path)?;
    cache
        .lock()
        .map_err(|e| AppError::Internal(e.to_string()))?
        .insert(cache_key, icon.clone());
    Ok(icon)
}

fn normalize_icon_cache_key(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    std::fs::canonicalize(trimmed)
        .map(|resolved| resolved.to_string_lossy().to_string())
        .unwrap_or_else(|_| trimmed.to_string())
        .replace('/', "\\")
        .to_ascii_lowercase()
}

fn normalize_file_icon_cache_key(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let candidate = Path::new(trimmed);
    if candidate.exists() {
        return normalize_icon_cache_key(trimmed);
    }

    candidate
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(|value| format!(".{}", value.to_ascii_lowercase()))
        .unwrap_or_else(|| trimmed.replace('/', "\\").to_ascii_lowercase())
}

fn extract_executable_icon_data_url(executable_path: &str) -> AppResult<Option<String>> {
    let trimmed = executable_path.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let path = Path::new(trimmed);
    if !path.exists() || !path.is_file() {
        return Ok(None);
    }

    let icon = unsafe { extract_executable_icon_handle(trimmed)? };
    let Some(icon) = icon else {
        return Ok(None);
    };

    let png_result = unsafe { render_icon_to_png_bytes(icon) };
    let _ = unsafe { DestroyIcon(icon) };

    match png_result {
        Ok(bytes) => Ok(Some(format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ))),
        Err(err) => Err(err),
    }
}

fn extract_file_icon_data_url(file_path: &str) -> AppResult<Option<String>> {
    let icon = unsafe { extract_file_icon_handle(file_path)? };
    let Some(icon) = icon else {
        return Ok(None);
    };

    let width = unsafe { GetSystemMetrics(SM_CXICON).max(32) };
    let height = unsafe { GetSystemMetrics(SM_CYICON).max(32) };
    let png_result = unsafe { render_icon_to_png_bytes_with_size(icon, width, height) };
    let _ = unsafe { DestroyIcon(icon) };

    match png_result {
        Ok(bytes) => Ok(Some(format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ))),
        Err(err) => Err(err),
    }
}

unsafe fn extract_executable_icon_handle(executable_path: &str) -> AppResult<Option<HICON>> {
    let wide_path: Vec<u16> = OsStr::new(executable_path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut large_icon = HICON::default();
    let mut small_icon = HICON::default();

    if ExtractIconExW(
        PCWSTR(wide_path.as_ptr()),
        0,
        Some(&mut large_icon),
        Some(&mut small_icon),
        1,
    ) == 0
    {
        return Ok(None);
    }

    let selected = if !small_icon.0.is_null() {
        small_icon
    } else {
        large_icon
    };

    if selected.0.is_null() {
        if !small_icon.0.is_null() {
            let _ = DestroyIcon(small_icon);
        }
        if !large_icon.0.is_null() {
            let _ = DestroyIcon(large_icon);
        }
        return Ok(None);
    }

    if selected.0 != small_icon.0 && !small_icon.0.is_null() {
        let _ = DestroyIcon(small_icon);
    }
    if selected.0 != large_icon.0 && !large_icon.0.is_null() {
        let _ = DestroyIcon(large_icon);
    }

    Ok(Some(selected))
}

unsafe fn extract_file_icon_handle(file_path: &str) -> AppResult<Option<HICON>> {
    let trimmed = file_path.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let path = Path::new(trimmed);
    let (lookup_target, attributes, use_file_attributes) = if path.exists() {
        (
            trimmed.to_string(),
            if path.is_dir() {
                FILE_ATTRIBUTE_DIRECTORY
            } else {
                FILE_ATTRIBUTE_NORMAL
            },
            false,
        )
    } else if let Some(ext) = path
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
    {
        (format!(".{}", ext), FILE_ATTRIBUTE_NORMAL, true)
    } else {
        (trimmed.to_string(), FILE_ATTRIBUTE_NORMAL, true)
    };

    let wide_path: Vec<u16> = OsStr::new(&lookup_target)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut file_info = SHFILEINFOW::default();
    let mut flags = SHGFI_ICON | SHGFI_LARGEICON;
    if use_file_attributes {
        flags |= SHGFI_USEFILEATTRIBUTES;
    }

    if SHGetFileInfoW(
        PCWSTR(wide_path.as_ptr()),
        attributes,
        Some(&mut file_info),
        std::mem::size_of::<SHFILEINFOW>() as u32,
        flags,
    ) == 0
    {
        return Ok(None);
    }

    if file_info.hIcon.0.is_null() {
        return Ok(None);
    }

    Ok(Some(file_info.hIcon))
}

unsafe fn render_icon_to_png_bytes(icon: HICON) -> AppResult<Vec<u8>> {
    let width = GetSystemMetrics(SM_CXSMICON).max(16);
    let height = GetSystemMetrics(SM_CYSMICON).max(16);
    render_icon_to_png_bytes_with_size(icon, width, height)
}

unsafe fn render_icon_to_png_bytes_with_size(
    icon: HICON,
    width: i32,
    height: i32,
) -> AppResult<Vec<u8>> {
    let bitmap_info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        bmiColors: [RGBQUAD::default()],
    };
    let mut bits = std::ptr::null_mut();
    let mem_dc = CreateCompatibleDC(None);
    if mem_dc.0.is_null() {
        return Err(AppError::Internal(
            "Failed to create icon memory DC".to_string(),
        ));
    }

    let bitmap = match CreateDIBSection(None, &bitmap_info, DIB_RGB_COLORS, &mut bits, None, 0) {
        Ok(bitmap) => bitmap,
        Err(err) => {
            let _ = DeleteDC(mem_dc);
            return Err(AppError::Internal(format!(
                "Failed to create icon bitmap: {}",
                err
            )));
        }
    };

    let old_bitmap = SelectObject(mem_dc, HGDIOBJ(bitmap.0));
    let result = (|| -> AppResult<Vec<u8>> {
        if bits.is_null() {
            return Err(AppError::Internal(
                "Icon bitmap has no backing buffer".to_string(),
            ));
        }

        let pixel_len = (width as usize) * (height as usize) * 4;
        std::slice::from_raw_parts_mut(bits as *mut u8, pixel_len).fill(0);
        DrawIconEx(mem_dc, 0, 0, icon, width, height, 0, None, DI_NORMAL)
            .map_err(|e| AppError::Internal(format!("Failed to draw executable icon: {}", e)))?;

        let bgra = std::slice::from_raw_parts(bits as *const u8, pixel_len);
        let mut rgba = vec![0u8; pixel_len];
        for (src, dst) in bgra.chunks_exact(4).zip(rgba.chunks_exact_mut(4)) {
            dst[0] = src[2];
            dst[1] = src[1];
            dst[2] = src[0];
            dst[3] = src[3];
        }

        let image = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(width as u32, height as u32, rgba)
            .ok_or_else(|| {
            AppError::Internal("Failed to build icon image buffer".to_string())
        })?;
        let mut png_bytes = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut png_bytes);
        image
            .write_to(&mut cursor, image::ImageFormat::Png)
            .map_err(|e| AppError::Internal(format!("Failed to encode icon PNG: {}", e)))?;
        Ok(png_bytes)
    })();

    let _ = SelectObject(mem_dc, old_bitmap);
    let _ = DeleteObject(HGDIOBJ(bitmap.0));
    let _ = DeleteDC(mem_dc);
    result
}

// Moved from main.rs
pub async fn launch_uwp_with_file(app_id: &str, file_path: &str) -> AppResult<()> {
    let path = std::path::Path::new(file_path);
    if !path.exists() {
        return Err(AppError::Validation(format!(
            "File does not exist: {}",
            file_path
        )));
    }

    let family_name = app_id.split('!').next().unwrap_or(app_id);

    let ps_script = format!(
        r#"
        Add-Type -AssemblyName System.Runtime.WindowsRuntime
        $asTask = ([System.WindowsRuntimeSystemExtensions].GetMethods() | ? {{ $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1' }})[0]
        
        $fileOp = [Windows.Storage.StorageFile,Windows.Storage,ContentType=WindowsRuntime]::GetFileFromPathAsync('{}')
        $fileTask = $asTask.MakeGenericMethod([Windows.Storage.StorageFile]).Invoke($null, @($fileOp))
        $file = $fileTask.GetAwaiter().GetResult()
        
        $options = New-Object Windows.System.LauncherOptions
        $options.TargetApplicationPackageFamilyName = '{}'
        
        $launchOp = [Windows.System.Launcher,Windows.System,ContentType=WindowsRuntime]::LaunchFileAsync($file, $options)
        $launchTask = $asTask.MakeGenericMethod([Boolean]).Invoke($null, @($launchOp))
        $result = $launchTask.GetAwaiter().GetResult()
        
        if ($result) {{ exit 0 }} else {{ exit 1 }}
        "#,
        file_path.replace("'", "''"),
        family_name
    );

    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &ps_script,
        ])
        .creation_flags(0x08000000)
        .output()
        .map_err(|e| format!("Starting PowerShell failed: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("WinRT Launch failed: {}", stderr.trim()).into())
    }
}
