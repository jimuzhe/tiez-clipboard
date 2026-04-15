use crate::global_state::LAST_ACTIVE_HWND;
use std::path::Path;
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::DataExchange::GetClipboardOwner;
use windows::Win32::System::ProcessStatus::GetModuleFileNameExW;
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
};
use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetForegroundWindow, GetWindowThreadProcessId, IsWindowVisible,
    EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT,
};

#[derive(Debug, Clone, Default)]
pub struct ActiveAppInfo {
    pub app_name: String,
    pub process_path: Option<String>,
}

fn unknown_app_info() -> ActiveAppInfo {
    ActiveAppInfo {
        app_name: "Unknown".to_string(),
        process_path: None,
    }
}

fn resolve_app_info_from_hwnd(hwnd: HWND) -> Option<ActiveAppInfo> {
    if hwnd.0.is_null() {
        return None;
    }

    unsafe {
        let mut process_id = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut process_id));
        if process_id == 0 {
            return None;
        }

        let process_handle = OpenProcess(
            PROCESS_QUERY_INFORMATION | PROCESS_VM_READ,
            false,
            process_id,
        );

        if let Ok(handle) = process_handle {
            let mut path_buf = [0u16; 1024];
            let path_len = GetModuleFileNameExW(Some(handle), None, &mut path_buf);
            let info = if path_len > 0 {
                let path_str = String::from_utf16_lossy(&path_buf[..path_len as usize]);
                ActiveAppInfo {
                    app_name: Path::new(&path_str)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("Unknown")
                        .to_string(),
                    process_path: Some(path_str),
                }
            } else {
                ActiveAppInfo {
                    app_name: "Unknown".to_string(),
                    process_path: None,
                }
            };
            let _ = CloseHandle(handle);
            if info.process_path.is_some() {
                Some(info)
            } else {
                None
            }
        } else {
            None
        }
    }
}

pub fn get_active_app_info() -> ActiveAppInfo {
    unsafe {
        let hwnd = GetForegroundWindow();
        if is_own_process_window(hwnd) || is_system_focus_window(hwnd) {
            return unknown_app_info();
        }
        resolve_app_info_from_hwnd(hwnd).unwrap_or_else(unknown_app_info)
    }
}

pub fn get_clipboard_source_app_info() -> ActiveAppInfo {
    unsafe {
        if let Ok(owner_hwnd) = GetClipboardOwner() {
            if !owner_hwnd.0.is_null() && !is_own_process_window(owner_hwnd) {
                if let Some(info) = resolve_app_info_from_hwnd(owner_hwnd) {
                    return info;
                }
            }
        }

        let foreground_hwnd = GetForegroundWindow();
        if !foreground_hwnd.0.is_null()
            && !is_own_process_window(foreground_hwnd)
            && !is_system_focus_window(foreground_hwnd)
        {
            if let Some(info) = resolve_app_info_from_hwnd(foreground_hwnd) {
                return info;
            }
        }

        let last_active = LAST_ACTIVE_HWND.load(Ordering::SeqCst);
        if last_active != 0 {
            let hwnd = HWND(last_active as *mut core::ffi::c_void);
            if let Some(info) = resolve_app_info_from_hwnd(hwnd) {
                return info;
            }
        }
    }

    unknown_app_info()
}

pub fn start_window_tracking(_app_handle: tauri::AppHandle) {
    std::thread::spawn(move || {
        unsafe {
            // Register hook to monitor window foreground changes
            let _hook = SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                None,
                Some(event_hook_callback),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            );

            // Keep the thread alive and processing messages
            let mut msg = windows::Win32::UI::WindowsAndMessaging::MSG::default();
            while windows::Win32::UI::WindowsAndMessaging::GetMessageW(&mut msg, None, 0, 0)
                .as_bool()
            {
                let _ = windows::Win32::UI::WindowsAndMessaging::TranslateMessage(&msg);
                windows::Win32::UI::WindowsAndMessaging::DispatchMessageW(&msg);
            }
        }
    });
}

fn is_own_process_window(hwnd: HWND) -> bool {
    if hwnd.0.is_null() {
        return false;
    }
    let mut process_id = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut process_id));
    }
    process_id != 0 && process_id == unsafe { GetCurrentProcessId() }
}

pub fn is_window_visible(hwnd: HWND) -> bool {
    if hwnd.0.is_null() {
        return false;
    }
    unsafe { IsWindowVisible(hwnd).as_bool() }
}

fn is_system_focus_window(hwnd: HWND) -> bool {
    if hwnd.0.is_null() {
        return true;
    }

    let mut class_name = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd, &mut class_name) };
    let class_str = if len > 0 {
        String::from_utf16_lossy(&class_name[..len as usize])
    } else {
        String::new()
    };

    matches!(
        class_str.as_str(),
        // Taskbar / tray / shell surfaces (these should NOT receive paste)
        "Shell_TrayWnd"
            | "Shell_SecondaryTrayWnd"
            | "TrayNotifyWnd"
            | "NotifyIconOverflowWindow"
            | "ReBarWindow32"
            | "MSTaskSwWClass"
            // Note: Progman and WorkerW (desktop windows) are intentionally NOT filtered
            // because users may need to paste when renaming files on the desktop
            | "Button"
            // System UI overlays that should NOT receive paste
            | "ImmersiveLauncher"
            | "ShellExperienceHost"
            | "TaskSwitcherWnd"
            | "MultitaskingViewFrame" // Note: Windows.UI.Core.CoreWindow, SearchUI, Cortana, XamlExplorerHostIslandWindow,
                                      // and ApplicationFrameWindow are intentionally NOT filtered because users may need
                                      // to paste in Windows search box and other UWP app input fields
    )
}

unsafe extern "system" fn event_hook_callback(
    _h_win_event_hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    _id_object: i32,
    _id_child: i32,
    _dw_event_thread: u32,
    _dwms_event_time: u32,
) {
    if event == EVENT_SYSTEM_FOREGROUND && !hwnd.0.is_null() {
        // Skip hidden windows
        if !unsafe { IsWindowVisible(hwnd).as_bool() } {
            return;
        }

        // Skip our own app windows
        if is_own_process_window(hwnd) {
            return;
        }

        // Skip system/shell windows that shouldn't receive paste
        if is_system_focus_window(hwnd) {
            return;
        }

        // Only store valid user windows (this is the "Save" part)
        LAST_ACTIVE_HWND.store(hwnd.0 as usize, Ordering::SeqCst);
        // println!("[DEBUG] Hook captured last focus HWND: {}", hwnd.0);
    }
}
