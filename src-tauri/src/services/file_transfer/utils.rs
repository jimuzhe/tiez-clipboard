use crate::services::file_transfer::models::ServerActivityState;
use base64::{engine::general_purpose, Engine as _};
use local_ip_address::list_afinet_netifas;
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
    const ICON_BYTES: &[u8] = include_bytes!("../../../icons/icon.png");
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

#[tauri::command]
pub fn get_available_ips() -> Vec<String> {
    if let Ok(ifas) = list_afinet_netifas() {
        let mut candidates = Vec::new();
        for (name, ip) in ifas {
            let ip_str = ip.to_string();
            let name_lower = name.to_lowercase();
            if ip.is_loopback() || !ip.is_ipv4() {
                continue;
            }

            // 过滤掉明显的虚拟网卡
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
                || name_lower.contains("radmin");

            if is_virtual {
                continue;
            }

            if ip_str.starts_with("192.168.")
                || ip_str.starts_with("10.")
                || ip_str.starts_with("172.")
            {
                candidates.push((name_lower, ip_str));
            }
        }

        candidates.sort_by(|(name_a, ip_a), (name_b, ip_b)| {
            let score_a = score_interface(name_a, ip_a);
            let score_b = score_interface(name_b, ip_b);
            score_b.cmp(&score_a)
        });

        return candidates.into_iter().map(|(_, ip)| ip).collect();
    }
    vec![]
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
