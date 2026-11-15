use crate::services::file_transfer::models::ServerActivityState;
use base64::{engine::general_purpose, Engine as _};
use crate::infrastructure::net_interfaces::list_afinet_netifas;
use std::time::SystemTime;
use tauri::{AppHandle, Manager};

pub fn update_activity(app_handle: &AppHandle) {
    if let Ok(mut guard) = app_handle
        .state::<ServerActivityState>()
        .last_activity
        .lock()
    {
        *guard = Some(SystemTime::now());
    }
}

pub fn get_app_logo_base64(_app: &AppHandle) -> String {
    // Embed the icon directly to ensure it works in Dev and Prod without path issues
    const ICON_BYTES: &[u8] = include_bytes!("../../../icons/128x128.png");
    format!(
        "data:image/png;base64,{}",
        general_purpose::STANDARD.encode(ICON_BYTES)
    )
}

pub fn score_interface(name: &str, ip: &str) -> i32 {
    let mut score = 0;
    if name.contains("wi-fi") || name.contains("wlan") {
        score += 10;
    }
    if name.contains("ethernet") {
        score += 5;
    }
    if ip.starts_with("192.168.") {
        score += 3;
    }
    if ip.starts_with("10.") {
        score += 2;
    }
    score
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct NetworkInterfaceInfo {
    pub name: String,
    pub ip: String,
    pub is_virtual: bool,
    pub label: String,
}

fn get_interface_label(name: &str, ip: &str, is_virtual: bool) -> String {
    let name_lower = name.to_lowercase();
    let type_desc = if is_virtual {
        "虚拟网络"
    } else if name_lower.contains("wi-fi") || name_lower.contains("wlan") || name_lower == "en0" {
        "Wi-Fi"
    } else if name_lower.contains("eth") || name_lower.contains("ethernet") || name_lower.starts_with("en") {
        "有线网络"
    } else {
        "同一网络"
    };

    format!("{} · {} ({})", ip, name, type_desc)
}

#[tauri::command]
pub fn get_available_network_interfaces() -> Vec<NetworkInterfaceInfo> {
    if let Some(ifas) = list_afinet_netifas() {
        let mut list = Vec::new();
        for (name, ip) in ifas {
            let ip_str = ip.to_string();
            let name_lower = name.to_lowercase();
            if ip.is_loopback() || !ip.is_ipv4() {
                continue;
            }

            let is_virtual = name_lower.contains("vnet")
                || name_lower.contains("vbox")
                || name_lower.contains("virtual")
                || name_lower.contains("vmnet")
                || name_lower.contains("tailscale")
                || name_lower.contains("zerotier")
                || name_lower.contains("pseudo")
                || name_lower.contains("clash")
                || name_lower.contains("wsl")
                || name_lower.contains("vethernet")
                || name_lower.contains("docker")
                || name_lower.contains("hyper-v")
                || name_lower.contains("radmin")
                || name_lower.contains("utun")
                || ip_str.starts_with("198.18.");

            let label = get_interface_label(&name, &ip_str, is_virtual);
            list.push((name_lower, ip_str, is_virtual, name, label));
        }

        // Sort: physical/LAN interfaces first, then by priority score
        list.sort_by(|(name_a, ip_a, is_virt_a, _, _), (name_b, ip_b, is_virt_b, _, _)| {
            if *is_virt_a != *is_virt_b {
                return is_virt_a.cmp(is_virt_b);
            }
            let score_a = score_interface(name_a, ip_a);
            let score_b = score_interface(name_b, ip_b);
            score_b.cmp(&score_a)
        });

        return list
            .into_iter()
            .map(|(_, ip, is_virtual, name, label)| NetworkInterfaceInfo {
                name,
                ip,
                is_virtual,
                label,
            })
            .collect();
    }
    vec![]
}

#[tauri::command]
pub fn get_available_ips() -> Vec<String> {
    let interfaces = get_available_network_interfaces();
    let physical: Vec<String> = interfaces
        .iter()
        .filter(|iface| !iface.is_virtual)
        .map(|iface| iface.ip.clone())
        .collect();
    if !physical.is_empty() {
        physical
    } else {
        interfaces.into_iter().map(|iface| iface.ip).collect()
    }
}

pub async fn bind_listener(start_port: u16) -> (tokio::net::TcpListener, u16) {
    let mut port = start_port;
    loop {
        let addr = format!("0.0.0.0:{}", port);
        match tokio::net::TcpListener::bind(&addr).await {
            Ok(listener) => return (listener, port),
            Err(_) => {
                if port >= 65535 {
                    if let Ok(listener) = tokio::net::TcpListener::bind("0.0.0.0:0").await {
                        let p = listener.local_addr().map(|a| a.port()).unwrap_or(0);
                        return (listener, p);
                    }
                    break;
                }
                port += 1;
            }
        }
    }
    (tokio::net::TcpListener::bind("0.0.0.0:0").await.unwrap(), 0)
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct ExpandedTransferItem {
    pub path: String,
    pub name: String,
    pub folder_name: Option<String>,
}

#[tauri::command]
pub fn expand_transfer_paths(paths: Vec<String>) -> Result<Vec<ExpandedTransferItem>, String> {
    let mut items = Vec::new();
    const MAX_FILES: usize = 2000;

    for path_str in paths {
        let path = std::path::PathBuf::from(&path_str);
        if !path.exists() {
            continue;
        }
        if path.is_file() {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "file".to_string());
            items.push(ExpandedTransferItem {
                path: path_str,
                name,
                folder_name: None,
            });
        } else if path.is_dir() {
            let folder_name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "文件夹".to_string());
            collect_files_from_dir(&path, &folder_name, &mut items, MAX_FILES);
        }
        if items.len() >= MAX_FILES {
            break;
        }
    }

    Ok(items)
}

fn collect_files_from_dir(
    dir: &std::path::Path,
    root_folder: &str,
    items: &mut Vec<ExpandedTransferItem>,
    max_files: usize,
) {
    if items.len() >= max_files {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        if items.len() >= max_files {
            break;
        }
        let p = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();
        // Skip hidden files/directories and OS metadata
        if file_name.starts_with('.')
            || file_name == "Thumbs.db"
            || file_name == "desktop.ini"
            || file_name == "$RECYCLE.BIN"
        {
            continue;
        }
        if p.is_dir() {
            collect_files_from_dir(&p, root_folder, items, max_files);
        } else if p.is_file() {
            items.push(ExpandedTransferItem {
                path: p.to_string_lossy().to_string(),
                name: file_name,
                folder_name: Some(root_folder.to_string()),
            });
        }
    }
}

/// Reserve `<prefix><timestamp>_<name>` in `dir`, adding " (2)", " (3)"… when
/// the name is taken. The empty file is created with `create_new`, so two
/// uploads finishing in the same second can never pick (and overwrite) the
/// same file; callers then write into it or rename over it.
pub fn reserve_unique_path(
    dir: &std::path::Path,
    prefix: &str,
    name: &str,
) -> std::io::Result<std::path::PathBuf> {
    let stamp = chrono::Utc::now().format("%Y%m%d%H%M%S");
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (name, String::new()),
    };
    for n in 1..1000 {
        let candidate = if n == 1 {
            format!("{prefix}{stamp}_{name}")
        } else {
            format!("{prefix}{stamp}_{stem} ({n}){ext}")
        };
        let path = dir.join(candidate);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(_) => return Ok(path),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "no free file name",
    ))
}
