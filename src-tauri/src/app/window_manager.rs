use crate::app_state::SettingsState;
use crate::global_state::*;
#[cfg(target_os = "windows")]
use crate::infrastructure::windows_ext::WindowExt;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager};

#[cfg(windows)]
use windows::Win32::Foundation::{HWND, POINT};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MonitorBounds {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

fn monitor_bounds(monitor: &tauri::Monitor) -> MonitorBounds {
    let position = monitor.position();
    let size = monitor.size();

    MonitorBounds {
        x: position.x,
        y: position.y,
        width: size.width as i32,
        height: size.height as i32,
    }
}

fn same_monitor(a: &tauri::Monitor, b: &tauri::Monitor) -> bool {
    monitor_bounds(a) == monitor_bounds(b)
}

fn remap_fixed_window_position(
    window_pos: (i32, i32),
    window_size: (i32, i32),
    source_monitor: MonitorBounds,
    target_monitor: MonitorBounds,
) -> (i32, i32) {
    let (window_width, window_height) = window_size;
    let source_span_x = (source_monitor.width - window_width).max(0);
    let source_span_y = (source_monitor.height - window_height).max(0);
    let target_span_x = (target_monitor.width - window_width).max(0);
    let target_span_y = (target_monitor.height - window_height).max(0);

    let source_offset_x = (window_pos.0 - source_monitor.x).clamp(0, source_span_x);
    let source_offset_y = (window_pos.1 - source_monitor.y).clamp(0, source_span_y);

    let ratio_x = if source_span_x == 0 {
        0.0
    } else {
        source_offset_x as f64 / source_span_x as f64
    };
    let ratio_y = if source_span_y == 0 {
        0.0
    } else {
        source_offset_y as f64 / source_span_y as f64
    };

    let mapped_x = target_monitor.x + (ratio_x * target_span_x as f64).round() as i32;
    let mapped_y = target_monitor.y + (ratio_y * target_span_y as f64).round() as i32;

    (
        mapped_x.clamp(target_monitor.x, target_monitor.x + target_span_x),
        mapped_y.clamp(target_monitor.y, target_monitor.y + target_span_y),
    )
}

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

        if let Ok(size) = window.outer_size() {
            let settings = app.state::<SettingsState>();
            if settings.follow_mouse.load(Ordering::Relaxed) {
                let w = size.width as i32;
                let h = size.height as i32;

                #[cfg(windows)]
                {
                    let mut point = POINT::default();
                    unsafe {
                        let _ = GetCursorPos(&mut point);
                    }
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
                            if point.x >= mx
                                && point.x < mx + mw
                                && point.y >= my
                                && point.y < my + mh
                            {
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
                        if target_x < mx {
                            target_x = mx + 5;
                        }
                        if target_x + w > mx + mw {
                            target_x = mx + mw - w - 5;
                        }
                        if target_y + h > my + mh {
                            let above_y = point.y - h - 12;
                            if above_y >= my {
                                target_y = above_y;
                            } else {
                                target_y = my + mh - h - 5;
                            }
                        }
                        if target_y < my {
                            target_y = my + 5;
                        }
                    }

                    let _ =
                        window.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
                            x: target_x,
                            y: target_y,
                        }));
                }
            } else if was_docked {
                let mut target_monitor = window.current_monitor().ok().flatten();

                #[cfg(windows)]
                {
                    let mut point = POINT::default();
                    unsafe {
                        let _ = GetCursorPos(&mut point);
                    }
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
                        1 => {
                            let _ = window.set_position(tauri::Position::Physical(
                                tauri::PhysicalPosition {
                                    x: mx + (mw / 2 - w / 2),
                                    y: my + 10,
                                },
                            ));
                        }
                        2 => {
                            let _ = window.set_position(tauri::Position::Physical(
                                tauri::PhysicalPosition {
                                    x: mx + 10,
                                    y: my + 10,
                                },
                            ));
                        }
                        3 => {
                            let _ = window.set_position(tauri::Position::Physical(
                                tauri::PhysicalPosition {
                                    x: mx + mw - w - 10,
                                    y: my + 10,
                                },
                            ));
                        }
                        _ => {
                            let center_x = mx + (mw / 2) - (w / 2);
                            let center_y = my + (m_size.height as i32 / 2) - (h / 2);
                            let _ = window.set_position(tauri::Position::Physical(
                                tauri::PhysicalPosition {
                                    x: center_x,
                                    y: center_y,
                                },
                            ));
                        }
                    }
                }
            } else {
                let w = size.width as i32;
                let h = size.height as i32;

                #[cfg(windows)]
                {
                    let mut point = POINT::default();
                    unsafe {
                        let _ = GetCursorPos(&mut point);
                    }
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
                        let is_same = current
                            .as_ref()
                            .map(|c| same_monitor(c, &target))
                            .unwrap_or(false);

                        if !is_same {
                            let mapped_position = current
                                .as_ref()
                                .and_then(|current_monitor| {
                                    window.outer_position().ok().map(|current_pos| {
                                        remap_fixed_window_position(
                                            (current_pos.x, current_pos.y),
                                            (w, h),
                                            monitor_bounds(current_monitor),
                                            monitor_bounds(&target),
                                        )
                                    })
                                })
                                .unwrap_or_else(|| {
                                    let target_bounds = monitor_bounds(&target);
                                    (
                                        target_bounds.x + (target_bounds.width - w) / 2,
                                        target_bounds.y + (target_bounds.height - h) / 2,
                                    )
                                });

                            let _ = window.set_position(tauri::Position::Physical(
                                tauri::PhysicalPosition {
                                    x: mapped_position.0,
                                    y: mapped_position.1,
                                },
                            ));
                        }
                    }
                }
            }
        }

        #[cfg(target_os = "windows")]
        WindowExt::release_win_keys();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
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
                    let _ = SetWindowLongPtrW(
                        HWND(hwnd_raw.0),
                        GWL_EXSTYLE,
                        ex_style | WS_EX_NOACTIVATE.0 as isize,
                    );
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

pub fn release_win_keys() {
    #[cfg(target_os = "windows")]
    WindowExt::release_win_keys();
}

pub fn is_main_window_focused() -> bool {
    IS_MAIN_WINDOW_FOCUSED.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::{remap_fixed_window_position, MonitorBounds};

    #[test]
    fn keeps_bottom_right_anchor_when_switching_monitors() {
        let source = MonitorBounds {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let target = MonitorBounds {
            x: 1920,
            y: 0,
            width: 2560,
            height: 1440,
        };

        let mapped = remap_fixed_window_position((1610, 670), (300, 400), source, target);

        assert_eq!(mapped, (4180, 1040));
    }

    #[test]
    fn preserves_center_ratio_for_mid_screen_window() {
        let source = MonitorBounds {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let target = MonitorBounds {
            x: -1600,
            y: 0,
            width: 1600,
            height: 900,
        };

        let mapped = remap_fixed_window_position((810, 340), (300, 400), source, target);

        assert_eq!(mapped, (-800, 250));
    }

    #[test]
    fn clamps_positions_that_started_partly_outside_source_monitor() {
        let source = MonitorBounds {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let target = MonitorBounds {
            x: 1920,
            y: 0,
            width: 1280,
            height: 1024,
        };

        let mapped = remap_fixed_window_position((2000, 900), (500, 500), source, target);

        assert_eq!(mapped, (2700, 524));
    }
}
