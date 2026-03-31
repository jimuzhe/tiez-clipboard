use tauri::{AppHandle, Manager, Emitter};
use std::sync::atomic::Ordering;
use crate::app_state::SettingsState;
use crate::global_state::*;
#[cfg(target_os = "windows")]
use crate::infrastructure::windows_ext::WindowExt;

#[cfg(windows)]
use windows::Win32::Foundation::{HWND, POINT};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE};

pub fn toggle_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        #[cfg(windows)]
        let mut active_center: Option<(i32, i32)> = None;
        let is_visible = window.is_visible().unwrap_or(false);
        let is_hidden_by_edge = IS_HIDDEN.load(Ordering::Relaxed);

        if is_visible && !is_hidden_by_edge {
            #[cfg(target_os = "windows")]
            WindowExt::release_win_keys();
            let _ = window.set_focusable(false);
            let _ = window.hide();
            
            let _ = restore_last_focus(app.clone());
            
            IS_HIDDEN.store(false, Ordering::Relaxed);
            NAVIGATION_ENABLED.store(false, Ordering::SeqCst);
            NAVIGATION_MODE_ACTIVE.store(false, Ordering::SeqCst);
            return;
        }

        IS_HIDDEN.store(false, Ordering::Relaxed);
        NAVIGATION_ENABLED.store(true, Ordering::SeqCst);
        let was_docked = is_hidden_by_edge;
        let current_dock_val = CURRENT_DOCK.load(Ordering::Relaxed);
        CURRENT_DOCK.store(0, Ordering::Relaxed);

        #[cfg(windows)]
        {
            let hwnd = WindowExt::get_foreground_window();
            let current_hwnd_val = hwnd.0 as isize;
            if current_hwnd_val != 0 {
                let mut main_hwnd_val = 0isize;
                if let Ok(h) = window.hwnd() {
                    main_hwnd_val = h.0 as isize;
                }
                if current_hwnd_val != main_hwnd_val {
                    LAST_ACTIVE_HWND.store(current_hwnd_val as usize, Ordering::Relaxed);
                    if let Some(rect) = WindowExt::get_window_rect(hwnd) {
                        let cx = (rect.left + rect.right) / 2;
                        let cy = (rect.top + rect.bottom) / 2;
                        active_center = Some((cx, cy));
                    }
                }
            }
        }

        #[cfg(target_os = "linux")]
        {
            // Save the currently active X11 window before TieZ takes focus
            if let Ok(output) = std::process::Command::new("xdotool")
                .arg("getactivewindow")
                .output()
            {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if let Ok(wid) = stdout.parse::<u64>() {
                    // Don't save our own window
                    if let Ok(our_wid) = std::process::Command::new("xdotool")
                        .args(["search", "--pid", &std::process::id().to_string()])
                        .output()
                    {
                        let our_windows: Vec<u64> = String::from_utf8_lossy(&our_wid.stdout)
                            .lines()
                            .filter_map(|l| l.trim().parse().ok())
                            .collect();
                        if !our_windows.contains(&wid) {
                            crate::LAST_ACTIVE_X11_WINDOW.store(wid, Ordering::Relaxed);
                        }
                    }
                }
            }
        }

        if let Ok(size) = window.outer_size() {
            let settings = app.state::<SettingsState>();
            if settings.follow_mouse.load(Ordering::Relaxed) {
                let w = size.width as i32;
                let h = size.height as i32;
                
                #[cfg(windows)]
                {
                    let mut point = POINT::default();
                    unsafe { let _ = GetCursorPos(&mut point); }
                    let mut target_x = point.x - (w / 2);
                    let mut target_y = point.y + 12;

                    let mut target_monitor: Option<tauri::Monitor> = None;
                    if let Ok(monitors) = window.available_monitors() {
                        for m in &monitors {
                            let m_pos = m.position();
                            let m_size = m.size();
                            let mx = m_pos.x;
                            let my = m_pos.y;
                            let mw = m_size.width as i32;
                            let mh = m_size.height as i32;
                            if point.x >= mx && point.x < mx + mw && point.y >= my && point.y < my + mh {
                                target_monitor = Some(m.clone());
                                break;
                            }
                        }
                        if target_monitor.is_none() && !monitors.is_empty() {
                            target_monitor = Some(monitors[0].clone());
                        }
                    }

                    if let Some(m) = target_monitor.as_ref() {
                        let m_pos = m.position();
                        let m_size = m.size();
                        let mx = m_pos.x;
                        let my = m_pos.y;
                        let mw = m_size.width as i32;
                        let mh = m_size.height as i32;
                        if target_x < mx { target_x = mx + 5; }
                        if target_x + w > mx + mw { target_x = mx + mw - w - 5; }
                        if target_y + h > my + mh {
                            let above_y = point.y - h - 12;
                            if above_y >= my { target_y = above_y; }
                            else { target_y = my + mh - h - 5; }
                        }
                        if target_y < my { target_y = my + 5; }
                    }

                    let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x: target_x, y: target_y }));
                }
            } else if was_docked {
                let mut target_monitor = window.current_monitor().ok().flatten();

                #[cfg(windows)]
                {
                    let mut point = POINT::default();
                    unsafe { let _ = GetCursorPos(&mut point); }
                    let (ref_x, ref_y) = active_center.unwrap_or((point.x, point.y));

                    if let Ok(monitors) = window.available_monitors() {
                        for m in &monitors {
                            let m_pos = m.position();
                            let m_size = m.size();
                            let mx = m_pos.x;
                            let my = m_pos.y;
                            let mw = m_size.width as i32;
                            let mh = m_size.height as i32;
                            if ref_x >= mx && ref_x < mx + mw && ref_y >= my && ref_y < my + mh {
                                target_monitor = Some(m.clone());
                                break;
                            }
                        }
                        if target_monitor.is_none() && !monitors.is_empty() {
                            target_monitor = Some(monitors[0].clone());
                        }
                    }
                }

                if let Some(monitor) = target_monitor {
                     let m_size = monitor.size();
                     let m_pos = monitor.position();
                     let w = size.width as i32;
                     let h = size.height as i32;
                     let mx = m_pos.x;
                     let my = m_pos.y;
                     let mw = m_size.width as i32;
                     
                     match current_dock_val {
                          1 => { let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x: mx + (mw/2 - w/2), y: my + 10 })); },
                          2 => { let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x: mx + 10, y: my + 10 })); },
                          3 => { let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x: mx + mw - w - 10, y: my + 10 })); },
                          _ => {
                                  let center_x = mx + (mw / 2) - (w / 2);
                                  let center_y = my + (m_size.height as i32 / 2) - (h / 2);
                                  let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x: center_x, y: center_y }));
                          }
                     }
                }
            } else {
                let w = size.width as i32;
                let h = size.height as i32;

                #[cfg(windows)]
                {
                    let mut point = POINT::default();
                    unsafe { let _ = GetCursorPos(&mut point); }
                    let (ref_x, ref_y) = active_center.unwrap_or((point.x, point.y));

                    let mut target_monitor: Option<tauri::Monitor> = None;
                    if let Ok(monitors) = window.available_monitors() {
                        for m in &monitors {
                            let m_pos = m.position();
                            let m_size = m.size();
                            let mx = m_pos.x;
                            let my = m_pos.y;
                            let mw = m_size.width as i32;
                            let mh = m_size.height as i32;
                            if ref_x >= mx && ref_x < mx + mw && ref_y >= my && ref_y < my + mh {
                                target_monitor = Some(m.clone());
                                break;
                            }
                        }
                        if target_monitor.is_none() && !monitors.is_empty() {
                            target_monitor = Some(monitors[0].clone());
                        }
                    }

                    if let Some(target) = target_monitor {
                        let current = window.current_monitor().ok().flatten();
                        let is_same = current.as_ref().map(|c| {
                            let c_pos = c.position();
                            let c_size = c.size();
                            let t_pos = target.position();
                            let t_size = target.size();
                            c_pos.x == t_pos.x
                                && c_pos.y == t_pos.y
                                && c_size.width == t_size.width
                                && c_size.height == t_size.height
                        }).unwrap_or(false);

                        if !is_same {
                            let t_pos = target.position();
                            let t_size = target.size();
                            let center_x = t_pos.x + (t_size.width as i32 - w) / 2;
                            let center_y = t_pos.y + (t_size.height as i32 - h) / 2;
                            let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x: center_x, y: center_y }));
                        }
                    }
                }
            }
        }

        #[cfg(target_os = "windows")]
        WindowExt::release_win_keys();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
        LAST_SHOW_TIMESTAMP.store(now, Ordering::Relaxed);

        let pinned = WINDOW_PINNED.load(Ordering::Relaxed);
        let _ = window.set_always_on_top(pinned);
        let _ = window.set_focusable(false);
        let _ = app.emit("window-pinned-changed", pinned);

        #[cfg(target_os = "windows")]
        {
            if let Ok(hwnd_raw) = window.hwnd() {
                unsafe {
                    let ex_style = GetWindowLongPtrW(HWND(hwnd_raw.0), GWL_EXSTYLE);
                    let _ = SetWindowLongPtrW(HWND(hwnd_raw.0), GWL_EXSTYLE, ex_style | WS_EX_NOACTIVATE.0 as isize);
                }
                let _ = window.show();
                if pinned {
                    WindowExt::show_window_no_activate(HWND(hwnd_raw.0));
                } else {
                    WindowExt::show_window_no_activate_normal(HWND(hwnd_raw.0));
                }
            } else {
                let _ = window.show();
            }
        }

        #[cfg(not(windows))]
        {
            let _ = window.show();
        }
    }
}

#[tauri::command]
pub fn set_navigation_enabled(enabled: bool) -> Result<(), String> {
    NAVIGATION_ENABLED.store(enabled, Ordering::SeqCst);
    if !enabled {
        NAVIGATION_MODE_ACTIVE.store(false, Ordering::SeqCst);
    }
    Ok(())
}

#[tauri::command]
pub fn set_navigation_mode(active: bool) -> Result<(), String> {
    NAVIGATION_MODE_ACTIVE.store(active, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
pub fn activate_window_focus(app_handle: AppHandle) -> Result<(), String> {
    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.set_focusable(true);
        
        #[cfg(windows)]
        {
            if let Ok(hwnd_raw) = window.hwnd() {
                unsafe {
                    let ex_style = GetWindowLongPtrW(HWND(hwnd_raw.0), GWL_EXSTYLE);
                    let next = ex_style & !(WS_EX_NOACTIVATE.0 as isize);
                    let _ = SetWindowLongPtrW(HWND(hwnd_raw.0), GWL_EXSTYLE, next);
                }
                let _ = window.set_focus();
                WindowExt::force_focus_window(HWND(hwnd_raw.0));
                return Ok(());
            }
        }
        let _ = window.set_focus();
    }
    Ok(())
}

#[tauri::command]
pub fn hide_window_cmd(app_handle: AppHandle) -> Result<(), String> {
    if let Some(window) = app_handle.get_webview_window("main") {
        #[cfg(target_os = "windows")]
        WindowExt::release_win_keys();
        let _ = window.set_focusable(false);
        let _ = window.hide();
        NAVIGATION_ENABLED.store(false, Ordering::SeqCst);
        NAVIGATION_MODE_ACTIVE.store(false, Ordering::SeqCst);
        let _ = restore_last_focus(app_handle.clone());
    }
    Ok(())
}

#[tauri::command]
pub fn toggle_window_cmd(app_handle: AppHandle) -> Result<(), String> {
    toggle_window(&app_handle);
    Ok(())
}

#[tauri::command]
pub fn focus_clipboard_window(app_handle: AppHandle) -> Result<(), String> {
    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.set_focusable(true);
        let _ = window.show();
        
        #[cfg(windows)]
        {
            if let Ok(hwnd_raw) = window.hwnd() {
                unsafe {
                    let ex_style = GetWindowLongPtrW(HWND(hwnd_raw.0), GWL_EXSTYLE);
                    let next = ex_style & !(WS_EX_NOACTIVATE.0 as isize);
                    let _ = SetWindowLongPtrW(HWND(hwnd_raw.0), GWL_EXSTYLE, next);
                }
                let _ = window.set_focus();
                WindowExt::force_focus_window(HWND(hwnd_raw.0));
                return Ok(());
            }
        }
        let _ = window.set_focus();
        Ok(())
    } else {
        Err("Main window not found".to_string())
    }
}

#[tauri::command]
pub fn restore_last_focus(_app_handle: AppHandle) -> Result<(), String> {
    #[cfg(windows)]
    {
        let last_hwnd_val = LAST_ACTIVE_HWND.load(Ordering::Relaxed);
        if last_hwnd_val == 0 {
            return Ok(());
        }
        WindowExt::force_focus_window(HWND(last_hwnd_val as _));
        std::thread::sleep(std::time::Duration::from_millis(60));
    }
    Ok(())
}

/// On Linux, continuously track the active X11 window so we know where to
/// send paste keystrokes.  Runs a lightweight poll loop (500 ms) that reads
/// _NET_ACTIVE_WINDOW and caches the most recent non-TieZ window ID.
#[cfg(target_os = "linux")]
pub fn start_linux_window_tracker() {
    std::thread::spawn(move || {
        let our_pid = std::process::id().to_string();
        let mut our_windows_cache: Vec<u64> = Vec::new();
        let mut cache_time = std::time::Instant::now();

        loop {
            std::thread::sleep(std::time::Duration::from_millis(500));

            // Refresh our own window list every 5 s (PID doesn't change, but
            // transient helper windows may appear/disappear).
            if cache_time.elapsed() > std::time::Duration::from_secs(5) {
                our_windows_cache = std::process::Command::new("xdotool")
                    .args(["search", "--pid", &our_pid])
                    .output()
                    .ok()
                    .map(|o| {
                        String::from_utf8_lossy(&o.stdout)
                            .lines()
                            .filter_map(|l| l.trim().parse::<u64>().ok())
                            .collect()
                    })
                    .unwrap_or_default();
                cache_time = std::time::Instant::now();
            }

            if let Ok(output) = std::process::Command::new("xdotool")
                .args(["getactivewindow"])
                .output()
            {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if let Ok(wid) = stdout.parse::<u64>() {
                    if !our_windows_cache.contains(&wid) {
                        LAST_ACTIVE_X11_WINDOW.store(wid, Ordering::Relaxed);
                    }
                }
            }
        }
    });
}

pub fn release_win_keys() {
    #[cfg(target_os = "windows")]
    WindowExt::release_win_keys();
}

pub fn is_main_window_focused() -> bool {
    IS_MAIN_WINDOW_FOCUSED.load(Ordering::Relaxed)
}
