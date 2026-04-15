use crate::global_state::TASKBAR_CREATED_MSG;
use std::sync::atomic::Ordering;
use tauri::Manager;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
#[cfg(target_os = "windows")]
use windows::Win32::UI::Shell::DefSubclassProc;

/// 获取硬件机器码（基于硬件唯一标识）
/// 返回格式: 8字符的十六进制字符串 (例如: "ef785433")
pub fn get_machine_id() -> String {
    use sha2::{Digest, Sha256};

    match machine_uid::get() {
        Ok(machine_uid) => {
            let mut hasher = Sha256::new();
            hasher.update(machine_uid.as_bytes());
            let result = hasher.finalize();
            let hex = format!("{:x}", result);
            hex.chars().take(8).collect()
        }
        Err(e) => {
            eprintln!("[WARN] Failed to get machine UID: {}. Using fallback.", e);
            let mut hasher = Sha256::new();
            if let Ok(computer_name) = std::env::var("COMPUTERNAME") {
                hasher.update(computer_name.as_bytes());
            }
            if let Ok(username) = std::env::var("USERNAME") {
                hasher.update(username.as_bytes());
            }
            let result = hasher.finalize();
            let hex = format!("{:x}", result);
            hex.chars().take(8).collect()
        }
    }
}

pub fn build_anon_id(machine_id: &str) -> String {
    machine_id.to_string()
}

pub fn is_legacy_placeholder_anon_id(id: &str) -> bool {
    id.contains("-0000-0000-0000-000000000000")
}

pub fn normalize_anon_id(id: &str) -> Option<String> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(short) = trimmed.split('-').next() {
        if (short.len() == 8 || short.len() == 9) && short.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(short.to_string());
        }
    }
    None
}

pub fn is_same_device_id(id1: &str, id2: &str) -> bool {
    let n1 = normalize_anon_id(id1);
    let n2 = normalize_anon_id(id2);
    n1.is_some() && n1 == n2
}

/// Window subclass procedure to handle taskbar recreation (explorer restart)
#[cfg(target_os = "windows")]
pub unsafe extern "system" fn tray_subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    let taskbar_msg = TASKBAR_CREATED_MSG.load(Ordering::Relaxed);
    if msg != 0 && msg == taskbar_msg {
        if let Some(app_handle) = crate::GLOBAL_APP_HANDLE.get() {
            let handle = app_handle.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(1500));
                if let Some(settings) = handle.try_state::<crate::app_state::SettingsState>() {
                    if settings.hide_tray_icon.load(Ordering::Relaxed) {
                        if let Some(tray) = handle.tray_by_id("main_tray") {
                            let _ = tray.set_visible(false);
                            println!(">>> [TRAY] Explorer restart detected, re-hiding tray icon per user setting.");
                        }
                    }
                }
            });
        }
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}
