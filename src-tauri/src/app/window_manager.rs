use crate::app_state::SettingsState;
use crate::global_state::*;
#[cfg(target_os = "windows")]
use crate::infrastructure::windows_ext::WindowExt;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager, Theme, WebviewWindow};

/// Disable WebView2 browser accelerators for one window (or all when label is empty).
/// No-op on non-Windows platforms.
#[tauri::command]
pub fn disable_webview_browser_accelerators(
    app_handle: AppHandle,
    label: Option<String>,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use crate::infrastructure::windows_api::webview_keys::{
            disable_browser_accelerator_keys_for_app, disable_browser_accelerator_keys_for_window,
        };

        match label.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(window_label) => {
                let window = app_handle
                    .get_webview_window(window_label)
                    .ok_or_else(|| format!("window '{window_label}' not found"))?;
                disable_browser_accelerator_keys_for_window(&window);
            }
            None => disable_browser_accelerator_keys_for_app(&app_handle),
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app_handle, label);
    }

    Ok(())
}

#[cfg(feature = "devtools")]
use std::sync::atomic::AtomicBool;

#[cfg(feature = "devtools")]
static DEVTOOLS_OPENED_ONCE: AtomicBool = AtomicBool::new(false);

#[cfg(feature = "devtools")]
fn parse_env_truthy(key: &str) -> Option<bool> {
    std::env::var(key).ok().map(|v| {
        let t = v.trim().to_ascii_lowercase();
        t == "1" || t == "true" || t == "yes" || t == "on"
    })
}

#[cfg(feature = "devtools")]
pub fn should_auto_open_devtools() -> bool {
    parse_env_truthy("TIEZ_AUTO_OPEN_DEVTOOLS").unwrap_or(false)
}

#[cfg(not(feature = "devtools"))]
pub fn should_auto_open_devtools() -> bool {
    false
}

#[cfg(feature = "devtools")]
pub fn maybe_open_devtools(window: &WebviewWindow) {
    if !should_auto_open_devtools() || DEVTOOLS_OPENED_ONCE.swap(true, Ordering::Relaxed) {
        return;
    }

    window.open_devtools();

    let app_handle = window.app_handle().clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(450));
        if let Some(win) = app_handle.get_webview_window("main") {
            win.open_devtools();
        }
    });
}

#[cfg(not(feature = "devtools"))]
pub fn maybe_open_devtools(_window: &WebviewWindow) {}

/// AppKit view mutations must run together on the main thread. In particular,
/// clearing off-thread can succeed while applying returns NotMainThread, leaving
/// only the transparent webview. Keep the material attached across hide/show.
#[cfg(target_os = "macos")]
pub fn apply_macos_window_material(window: &WebviewWindow) -> tauri::Result<()> {
    let target = window.clone();
    window.run_on_main_thread(move || {
        use window_vibrancy::{
            apply_liquid_glass, apply_vibrancy, clear_liquid_glass, LiquidGlassOptions,
            NSGlassEffectViewStyle, NSVisualEffectMaterial, NSVisualEffectState,
        };

        // Read at execution time so queued requests cannot restore an old theme.
        let settings = target.app_handle().state::<SettingsState>();
        let theme = settings.theme.lock().unwrap().clone();
        let material = match theme.as_str() {
            "mica" => Some(NSVisualEffectMaterial::Sidebar),
            "acrylic" => Some(NSVisualEffectMaterial::Popover),
            _ => None,
        };

        // Both APIs mutate the same native container. Clear on the main thread
        // before changing material so repeated theme changes never stack views.
        if let Err(error) = clear_liquid_glass(&target) {
            eprintln!("[window-material] clear glass failed: {error}");
            return;
        }
        if let Err(error) = window_vibrancy::clear_vibrancy(&target) {
            eprintln!("[window-material] clear failed: {error}");
            return;
        }
        if theme == "acrylic" {
            // The library checks AppKit availability before creating the view.
            // Regular provides frosting; Clear is intentionally more transparent.
            let options = LiquidGlassOptions::new(NSGlassEffectViewStyle::Regular)
                .radius(10.0)
                .opaque(false);
            match apply_liquid_glass(&target, options) {
                Ok(()) => return,
                Err(window_vibrancy::Error::UnsupportedPlatformVersion(_)) => {}
                Err(error) => {
                    eprintln!("[window-material] glass failed, using Popover: {error}");
                }
            }
        }
        if let Some(material) = material {
            // The clipboard is a non-activating panel and can remain visible
            // while another application owns focus (especially when pinned).
            if let Err(error) = apply_vibrancy(
                &target,
                material,
                Some(NSVisualEffectState::Active),
                Some(12.0),
            ) {
                eprintln!("[window-material] apply {theme} failed: {error}");
            }
        }
    })
}

/// Windows 10-style composition acrylic (SetWindowCompositionAttribute,
/// accent 4). From 22H2 window-vibrancy maps apply_acrylic to the DWM system
/// backdrop instead, which DWM draws only while the window is focused and
/// often not at all on frameless windows; apply_blur no longer blurs there.
/// This accent keeps blurring when unfocused and honours the tint.
#[cfg(target_os = "windows")]
fn apply_composition_acrylic(
    window: &WebviewWindow,
    (r, g, b, a): (u8, u8, u8, u8),
) -> Result<(), String> {
    use std::ffi::c_void;
    use windows::core::{s, w};
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

    #[repr(C)]
    struct AccentPolicy {
        accent_state: u32,
        accent_flags: u32,
        gradient_color: u32,
        animation_id: u32,
    }
    #[repr(C)]
    struct CompositionAttributeData {
        attribute: u32,
        data: *mut c_void,
        size: usize,
    }
    type SetWindowCompositionAttribute =
        unsafe extern "system" fn(*mut c_void, *mut CompositionAttributeData) -> i32;
    const WCA_ACCENT_POLICY: u32 = 0x13;
    const ACCENT_ENABLE_ACRYLICBLURBEHIND: u32 = 4;

    let hwnd = window.hwnd().map_err(|error| error.to_string())?;
    unsafe {
        let user32 = LoadLibraryW(w!("user32.dll")).map_err(|error| error.to_string())?;
        let proc = GetProcAddress(user32, s!("SetWindowCompositionAttribute"))
            .ok_or_else(|| "SetWindowCompositionAttribute unavailable".to_string())?;
        let set_attribute = std::mem::transmute::<
            unsafe extern "system" fn() -> isize,
            SetWindowCompositionAttribute,
        >(proc);
        let mut policy = AccentPolicy {
            accent_state: ACCENT_ENABLE_ACRYLICBLURBEHIND,
            accent_flags: 0,
            // Acrylic misbehaves with a fully transparent tint.
            gradient_color: u32::from(r)
                | (u32::from(g) << 8)
                | (u32::from(b) << 16)
                | (u32::from(a.max(1)) << 24),
            animation_id: 0,
        };
        let mut data = CompositionAttributeData {
            attribute: WCA_ACCENT_POLICY,
            data: &mut policy as *mut AccentPolicy as *mut c_void,
            size: std::mem::size_of::<AccentPolicy>(),
        };
        if set_attribute(hwnd.0 as *mut c_void, &mut data) == 0 {
            return Err("SetWindowCompositionAttribute failed".to_string());
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn apply_windows_window_material(window: &WebviewWindow, theme: &str, is_dark: bool) {
    let build = windows_version::OsVersion::current().build;
    // Clear the actual Windows materials, including when leaving glass themes.
    // clear_blur resets the composition accent, which also removes
    // apply_composition_acrylic.
    if build >= 17763 {
        let _ = window_vibrancy::clear_acrylic(window);
        let _ = window_vibrancy::clear_blur(window);
    }
    if build >= 22000 {
        let _ = window_vibrancy::clear_mica(window);
    }

    let glass = matches!(theme, "acrylic" | "mica");
    // A neutral tint softens the backdrop without obscuring its colours.
    let tint = if is_dark { (28, 28, 30, 96) } else { (246, 246, 248, 80) };
    let applied: Result<(), String> = match theme {
        "mica" if build >= 22000 => {
            window_vibrancy::apply_mica(window, Some(is_dark)).map_err(|error| error.to_string())
        }
        // The clipboard mostly sits unfocused over other apps; see
        // apply_composition_acrylic for why 22H2+ needs its own path.
        "acrylic" if build >= 22523 => apply_composition_acrylic(window, tint),
        "acrylic" | "mica" if build >= 17763 => {
            window_vibrancy::apply_acrylic(window, Some(tint)).map_err(|error| error.to_string())
        }
        _ => {
            // Built-in themes below paint an opaque shell, so an opaque window
            // costs nothing visually. A transparent WebView2 background makes
            // Chromium drop ClearType for grayscale text, which reads thin on
            // Windows. Sakura's shell and installed themes may be translucent.
            let opaque_shell = matches!(
                theme,
                "macos" | "retro" | "sticky-note" | "paper" | "terminal" | "receipt" | "ink"
            );
            if glass || opaque_shell {
                let _ = window.set_background_color(Some(if is_dark {
                    tauri::window::Color(28, 28, 30, 255)
                } else {
                    tauri::window::Color(246, 246, 248, 255)
                }));
            } else {
                let _ = window.set_background_color(Some(tauri::window::Color(0, 0, 0, 0)));
            }
            return;
        }
    };
    if let Err(error) = &applied {
        eprintln!("[window-material] Windows {theme} failed: {error}");
    }
    let background = if applied.is_ok() {
        tauri::window::Color(0, 0, 0, 0)
    } else if is_dark {
        tauri::window::Color(28, 28, 30, 255)
    } else {
        tauri::window::Color(246, 246, 248, 255)
    };
    let _ = window.set_background_color(Some(background));
}

pub fn apply_window_vibrancy(_app: &AppHandle, window: &WebviewWindow) {
    #[cfg(target_os = "macos")]
    if let Err(error) = apply_macos_window_material(window) {
        eprintln!("[window-material] scheduling failed: {error}");
    }

    #[cfg(target_os = "windows")]
    {
        let settings = _app.state::<SettingsState>();
        let theme = settings.theme.lock().unwrap().clone();
        let is_dark = matches!(window.theme().unwrap_or(Theme::Dark), Theme::Dark);
        apply_windows_window_material(window, &theme, is_dark);
    }
}
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

    const EDGE_SNAP_DISTANCE: i32 = 16;
    let remap_axis = |offset: i32, source_span: i32, target_span: i32, ratio: f64| {
        if offset <= EDGE_SNAP_DISTANCE {
            0
        } else if source_span.saturating_sub(offset) <= EDGE_SNAP_DISTANCE {
            target_span
        } else {
            (ratio * target_span as f64).round() as i32
        }
    };

    let mapped_x = target_monitor.x + remap_axis(source_offset_x, source_span_x, target_span_x, ratio_x);
    let mapped_y = target_monitor.y + remap_axis(source_offset_y, source_span_y, target_span_y, ratio_y);

    (
        mapped_x.clamp(target_monitor.x, target_monitor.x + target_span_x),
        mapped_y.clamp(target_monitor.y, target_monitor.y + target_span_y),
    )
}

/// Screens, the cursor and the window measured in one unit: points on macOS,
/// pixels on Windows. On macOS tao gives each screen in its own scale's
/// pixels, the window in its current screen's and the cursor in the primary
/// screen's, so with a Retina and a 1x screen the raw numbers don't line up.
#[cfg(target_os = "macos")]
pub(crate) fn layout_scale(scale_factor: f64) -> f64 {
    scale_factor
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn layout_scale(_scale_factor: f64) -> f64 {
    1.0
}

#[cfg(target_os = "macos")]
pub(super) fn cursor_position_in_layout(cursor: (f64, f64), primary_scale: f64) -> (f64, f64) {
    (cursor.0 / primary_scale, cursor.1 / primary_scale)
}

/// The cursor in layout units (see `layout_scale`).
pub(crate) fn global_cursor_layout(window: &WebviewWindow) -> Option<(f64, f64)> {
    #[cfg(windows)]
    {
        let _ = window;
        let mut point = POINT::default();
        unsafe {
            if GetCursorPos(&mut point).is_err() {
                return None;
            }
        }
        Some((point.x as f64, point.y as f64))
    }
    #[cfg(target_os = "macos")]
    {
        let cursor = window.cursor_position().ok()?;
        let scale = window
            .primary_monitor()
            .ok()
            .flatten()
            .map_or(1.0, |monitor| monitor.scale_factor());
        Some(cursor_position_in_layout((cursor.x, cursor.y), scale))
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = window;
        None
    }
}

/// A screen in layout units: left, top, width, height.
type LayoutScreen = (f64, f64, f64, f64);

/// Where a `size` window opens for the cursor: centred below it, kept on the
/// cursor's screen (above the cursor if there is no room below).
fn follow_mouse_target(cursor: (f64, f64), size: (f64, f64), screens: &[LayoutScreen]) -> (f64, f64) {
    let (cursor_x, cursor_y) = cursor;
    let (w, h) = size;
    let mut target_x = cursor_x - w / 2.0;
    let mut target_y = cursor_y + 12.0;

    let screen = screens
        .iter()
        .find(|(x, y, width, height)| {
            cursor_x >= *x && cursor_x < x + width && cursor_y >= *y && cursor_y < y + height
        })
        .or(screens.first());

    if let Some(&(mx, my, mw, mh)) = screen {
        if target_x < mx {
            target_x = mx + 5.0;
        }
        if target_x + w > mx + mw {
            target_x = mx + mw - w - 5.0;
        }
        if target_y + h > my + mh {
            let above_y = cursor_y - h - 12.0;
            if above_y >= my {
                target_y = above_y;
            } else {
                target_y = my + mh - h - 5.0;
            }
        }
        if target_y < my {
            target_y = my + 5.0;
        }
    }

    (target_x, target_y)
}

fn position_window_follow_mouse(window: &WebviewWindow) {
    let Ok(size) = window.outer_size() else {
        return;
    };
    let Some(cursor) = global_cursor_layout(window) else {
        return;
    };
    let window_scale = layout_scale(window.scale_factor().unwrap_or(1.0));
    let screens: Vec<LayoutScreen> = window
        .available_monitors()
        .map(|monitors| {
            monitors
                .iter()
                .map(|monitor| {
                    let scale = layout_scale(monitor.scale_factor());
                    let pos = monitor.position();
                    let size = monitor.size();
                    (
                        pos.x as f64 / scale,
                        pos.y as f64 / scale,
                        size.width as f64 / scale,
                        size.height as f64 / scale,
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let (x, y) = follow_mouse_target(
        cursor,
        (size.width as f64 / window_scale, size.height as f64 / window_scale),
        &screens,
    );

    #[cfg(target_os = "macos")]
    let position = tauri::Position::Logical(tauri::LogicalPosition { x, y });
    #[cfg(not(target_os = "macos"))]
    let position = tauri::Position::Physical(tauri::PhysicalPosition {
        x: x.round() as i32,
        y: y.round() as i32,
    });
    let _ = window.set_position(position);
}

fn maybe_position_window_follow_mouse(app: &AppHandle, window: &WebviewWindow) {
    let settings = app.state::<SettingsState>();
    if settings.follow_mouse.load(Ordering::Relaxed) {
        position_window_follow_mouse(window);
    }
}

pub fn toggle_window(app: &AppHandle) {
    #[cfg(target_os = "windows")]
    toggle_window_windows(app);
    #[cfg(target_os = "macos")]
    toggle_window_macos(app);
}

/// Whether the clipboard window is currently on screen.
pub fn is_main_window_shown(app: &AppHandle) -> bool {
    #[cfg(target_os = "macos")]
    if crate::infrastructure::macos_api::window::is_clipboard_panel_available(app) {
        return crate::infrastructure::macos_api::window::is_clipboard_panel_visible();
    }
    app.get_webview_window("main")
        .map(|window| window.is_visible().unwrap_or(false))
        .unwrap_or(false)
}

/// Light mode destroyed the UI: rebuild it and show it once it has rendered.
/// Skipped if another summon already showed the window in the meantime.
fn rebuild_then_toggle(app: &AppHandle) {
    crate::app::light_mode::rebuild_then(app, |app| {
        if !is_main_window_shown(app) {
            toggle_window(app);
        }
    });
}

#[cfg(target_os = "windows")]
fn toggle_window_windows(app: &AppHandle) {
    if crate::app::light_mode::needs_rebuild(app) {
        rebuild_then_toggle(app);
        return;
    }

    if let Some(window) = app.get_webview_window("main") {
        #[cfg(windows)]
        let mut active_center: Option<(i32, i32)> = None;
        crate::app::setup::adopt_offscreen_tuck(app, &window);
        let is_visible = window.is_visible().unwrap_or(false);
        let is_hidden_by_edge = IS_HIDDEN.load(Ordering::Relaxed);

        if is_visible && !is_hidden_by_edge {
            #[cfg(target_os = "windows")]
            WindowExt::release_modifier_keys();
            let _ = window.set_focusable(false);
            let _ = window.hide();

            let _ = restore_previous_app_focus(app.clone());

            IS_HIDDEN.store(false, Ordering::Relaxed);
            NAVIGATION_ENABLED.store(false, Ordering::SeqCst);
            NAVIGATION_MODE_ACTIVE.store(false, Ordering::SeqCst);
            crate::app::light_mode::on_window_hidden();
            return;
        }

        crate::app::light_mode::on_window_shown();
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
                position_window_follow_mouse(&window);
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
        WindowExt::release_modifier_keys();
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

        let _ = app.emit("clipboard-shown", ());
    }
}

#[cfg(target_os = "macos")]
fn toggle_window_macos(app_handle: &AppHandle) {
    if crate::app::light_mode::needs_rebuild(app_handle) {
        rebuild_then_toggle(app_handle);
        return;
    }

    if let Some(window) = app_handle.get_webview_window("main") {
        let panel_available =
            crate::infrastructure::macos_api::window::is_clipboard_panel_available(app_handle);
        let is_visible = if panel_available {
            crate::infrastructure::macos_api::window::is_clipboard_panel_visible()
        } else {
            window.is_visible().unwrap_or(false)
        };
        let is_tucked = crate::app::setup::is_window_edge_tucked(&window);

        eprintln!(
            "[macos-toggle] visible={} panel={} tucked={}",
            is_visible, panel_available, is_tucked
        );

        if is_visible && !is_tucked {
            eprintln!("[macos-toggle] hide visible panel");
            let _ = app_handle.emit("force-hide-compact-preview", ());
            let pinned = WINDOW_PINNED.load(Ordering::Relaxed);
            let _ = window.set_always_on_top(pinned);
            crate::infrastructure::macos_api::window::set_window_focusable(&window, false);
            if !crate::infrastructure::macos_api::window::hide_clipboard_panel(app_handle) {
                let _ = window.hide();
            }
            let _ = restore_previous_app_focus(app_handle.clone());

            IS_HIDDEN.store(false, Ordering::Relaxed);
            CURRENT_DOCK.store(0, Ordering::Relaxed);
            NAVIGATION_ENABLED.store(false, Ordering::SeqCst);
            NAVIGATION_MODE_ACTIVE.store(false, Ordering::SeqCst);
            crate::app::light_mode::on_window_hidden();
            return;
        }

        crate::app::light_mode::on_window_shown();
        let (prev_app_name, prev_app_pid) =
            crate::infrastructure::macos_api::window::get_active_app_info();
        let self_pid = std::process::id();
        if !prev_app_name.is_empty() && !prev_app_name.eq_ignore_ascii_case("TieZ") {
            crate::global_state::set_last_active_app_name(prev_app_name);
        }
        if let Ok(pid) = prev_app_pid.parse::<u32>() {
            if pid != 0 && pid != self_pid {
                crate::global_state::LAST_ACTIVE_APP_PID.store(pid, Ordering::Relaxed);
            }
        }

        let expanded = crate::app::setup::expand_from_edge_dock(app_handle, &window);
        if !expanded {
            IS_HIDDEN.store(false, Ordering::Relaxed);
            CURRENT_DOCK.store(0, Ordering::Relaxed);
            crate::app::setup::persist_edge_dock(app_handle, 0);
            NAVIGATION_ENABLED.store(true, Ordering::SeqCst);
        }

        let pinned = WINDOW_PINNED.load(Ordering::Relaxed);
        let _ = window.set_always_on_top(true);
        crate::infrastructure::macos_api::window::set_window_focusable(&window, !pinned);
        let _ = app_handle.emit("window-pinned-changed", pinned);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        if !expanded {
            LAST_SHOW_TIMESTAMP.store(now, Ordering::Relaxed);
            maybe_position_window_follow_mouse(app_handle, &window);
        }

        eprintln!("[macos-toggle] show + present overlay");
        if !crate::infrastructure::macos_api::window::show_clipboard_panel(app_handle) {
            apply_window_vibrancy(app_handle, &window);
            let _ = window.show();
            crate::infrastructure::macos_api::window::present_overlay_above_fullscreen_async(
                &window,
            );
            let _ = app_handle.emit("clipboard-shown", ());
        }
        crate::infrastructure::macos_api::window::reinforce_overlay_above_fullscreen(&window);
        maybe_open_devtools(&window);
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
        #[cfg(target_os = "macos")]
        crate::infrastructure::macos_api::window::set_window_focusable(&window, true);

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

        // Nonactivating NSPanel: Tauri `set_focus()` often fails to make the panel
        // key (or blurs the focused input). Use AppKit makeKeyWindow instead so
        // search/note/tag fields can receive typed characters.
        #[cfg(target_os = "macos")]
        {
            if crate::infrastructure::macos_api::window::make_clipboard_panel_key(&app_handle) {
                return Ok(());
            }
        }

        let _ = window.set_focus();
    }
    Ok(())
}

/// Native dialogs require TieZ to be the active application on macOS. Keep
/// this separate from editable WebView focus: activating the whole process for
/// an input click can make the non-activating panel resign key mid-keystroke.
#[tauri::command]
pub fn activate_app_focus(app_handle: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::NSApplication;

        if let Some(mtm) = MainThreadMarker::new() {
            #[allow(deprecated)]
            NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
        } else {
            crate::infrastructure::macos_api::apps::activate_app_by_pid(std::process::id() as i32);
        }
    }

    activate_window_focus(app_handle)
}

fn hide_window(app_handle: AppHandle, restore_focus: bool) -> Result<(), String> {
    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = app_handle.emit("force-hide-compact-preview", ());
        if let Some(compact_preview) = app_handle.get_webview_window("compact-preview") {
            let _ = compact_preview.hide();
        }
        #[cfg(target_os = "windows")]
        WindowExt::release_modifier_keys();
        IS_HIDDEN.store(false, Ordering::Relaxed);
        CURRENT_DOCK.store(0, Ordering::Relaxed);
        let pinned = WINDOW_PINNED.load(Ordering::Relaxed);
        let _ = window.set_always_on_top(pinned);
        #[cfg(target_os = "macos")]
        crate::infrastructure::macos_api::window::set_window_focusable(&window, false);
        #[cfg(target_os = "macos")]
        {
            if !crate::infrastructure::macos_api::window::hide_clipboard_panel(&app_handle) {
                let _ = window.hide();
            }
        }
        #[cfg(not(target_os = "macos"))]
        let _ = window.hide();
        NAVIGATION_ENABLED.store(false, Ordering::SeqCst);
        NAVIGATION_MODE_ACTIVE.store(false, Ordering::SeqCst);
        crate::app::light_mode::on_window_hidden();
        if restore_focus {
            let _ = restore_previous_app_focus(app_handle.clone());
        }
    }
    Ok(())
}

#[tauri::command]
pub fn hide_window_cmd(app_handle: AppHandle) -> Result<(), String> {
    hide_window(app_handle, true)
}

pub fn hide_window_on_resign(app_handle: AppHandle) -> Result<(), String> {
    hide_window(app_handle, false)
}

/// The frontend has rendered with its settings applied (see light_mode).
#[tauri::command]
pub fn main_ui_ready() {
    crate::app::light_mode::mark_ui_ready();
}

/// The frontend has unmounted ahead of a light-mode release (see light_mode).
#[tauri::command]
pub fn main_ui_released() {
    crate::app::light_mode::mark_ui_released();
}

#[tauri::command]
pub fn toggle_window_cmd(app_handle: AppHandle) -> Result<(), String> {
    toggle_window(&app_handle);
    Ok(())
}

#[tauri::command]
pub fn focus_clipboard_window(app_handle: AppHandle) -> Result<(), String> {
    if crate::app::light_mode::needs_rebuild(&app_handle) {
        // Rebuilt after light mode: focus (idempotent) once the UI has rendered.
        crate::app::light_mode::rebuild_then(&app_handle, |app| {
            let _ = focus_clipboard_window(app.clone());
        });
        return Ok(());
    }
    crate::app::light_mode::on_window_shown();

    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = crate::app::setup::expand_from_edge_dock(&app_handle, &window);
        IS_HIDDEN.store(false, Ordering::Relaxed);
        CURRENT_DOCK.store(0, Ordering::Relaxed);
        crate::app::setup::persist_edge_dock(&app_handle, 0);
        #[cfg(target_os = "macos")]
        crate::infrastructure::macos_api::window::set_window_focusable(&window, true);

        #[cfg(windows)]
        {
            if let Ok(hwnd_raw) = window.hwnd() {
                unsafe {
                    let ex_style = GetWindowLongPtrW(HWND(hwnd_raw.0), GWL_EXSTYLE);
                    let next = ex_style & !(WS_EX_NOACTIVATE.0 as isize);
                    let _ = SetWindowLongPtrW(HWND(hwnd_raw.0), GWL_EXSTYLE, next);
                }
                let _ = window.show();
                let _ = window.set_focus();
                WindowExt::force_focus_window(HWND(hwnd_raw.0));
                maybe_open_devtools(&window);
                return Ok(());
            }
        }

        #[cfg(target_os = "macos")]
        {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            LAST_SHOW_TIMESTAMP.store(now, Ordering::Relaxed);

            if crate::infrastructure::macos_api::window::is_clipboard_panel_visible() {
                let _ =
                    crate::infrastructure::macos_api::window::make_clipboard_panel_key(&app_handle);
                maybe_open_devtools(&window);
                return Ok(());
            }

            if !crate::infrastructure::macos_api::window::show_clipboard_panel(&app_handle) {
                apply_window_vibrancy(&app_handle, &window);
                let _ = window.show();
                let _ = window.set_focus();
                let _ = app_handle.emit("clipboard-shown", ());
            }
            crate::infrastructure::macos_api::window::reinforce_overlay_above_fullscreen(&window);
            maybe_open_devtools(&window);
            return Ok(());
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = window.show();
            let _ = window.set_focus();
            let _ = app_handle.emit("clipboard-shown", ());
            maybe_open_devtools(&window);
            return Ok(());
        }
    } else {
        Err("Main window not found".to_string())
    }
}

#[tauri::command]
pub fn restore_previous_app_focus(_app_handle: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let last_hwnd_val = LAST_ACTIVE_HWND.load(Ordering::Relaxed);
        if last_hwnd_val == 0 {
            return Ok(());
        }
        WindowExt::force_focus_window(HWND(last_hwnd_val as _));
        std::thread::sleep(std::time::Duration::from_millis(60));
    }

    #[cfg(target_os = "macos")]
    {
        let prev_pid = crate::global_state::LAST_ACTIVE_APP_PID.load(Ordering::Relaxed);
        if prev_pid != 0 {
            crate::infrastructure::macos_api::apps::activate_app_by_pid(prev_pid as i32);
        } else {
            let prev_app = crate::global_state::get_last_active_app_name();
            if !prev_app.trim().is_empty() {
                crate::infrastructure::macos_api::apps::activate_app_by_name(&prev_app);
            }
        }
    }

    Ok(())
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

pub fn release_modifier_keys() {
    #[cfg(target_os = "windows")]
    WindowExt::release_modifier_keys();
}

pub fn release_win_keys() {
    release_modifier_keys();
}

pub fn is_main_window_focused() -> bool {
    IS_MAIN_WINDOW_FOCUSED.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::{follow_mouse_target, remap_fixed_window_position, MonitorBounds};

    // A Retina main screen (1440x900 points) with a 1x screen to its right.
    const SCREENS: [(f64, f64, f64, f64); 2] =
        [(0.0, 0.0, 1440.0, 900.0), (1440.0, 0.0, 1920.0, 1080.0)];

    #[test]
    fn follow_mouse_opens_on_the_cursors_screen() {
        assert_eq!(follow_mouse_target((2000.0, 300.0), (400.0, 600.0), &SCREENS), (1800.0, 312.0));
        // No room below or above: pinned to the bottom of that screen.
        assert_eq!(follow_mouse_target((2000.0, 500.0), (400.0, 600.0), &SCREENS), (1800.0, 475.0));
        // Near the shared edge, kept on the cursor's screen.
        assert_eq!(follow_mouse_target((1450.0, 100.0), (400.0, 600.0), &SCREENS), (1445.0, 112.0));
        assert_eq!(follow_mouse_target((1430.0, 100.0), (400.0, 600.0), &SCREENS), (1035.0, 112.0));
    }

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

        assert_eq!(mapped, (-950, 250));
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
