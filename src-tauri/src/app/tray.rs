//! The menu bar (macOS) / notification area (Windows) icon.
//!
//! A left click opens the clipboard, the most frequent action. The menu holds
//! the rest: open (showing the open hotkey, so the icon teaches it), pause
//! recording, settings, update check, help and quit. Its labels follow the app
//! language, so the menu is rebuilt when the language, the open hotkey or the
//! pause changes.

use std::sync::atomic::Ordering;

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::database::DbState;
use crate::global_state::USER_RECORDING_PAUSED;
use crate::infrastructure::repository::settings_repo::SettingsRepository;

pub const TRAY_ID: &str = "main_tray";
const ICON_BYTES: &[u8] = include_bytes!("../../icons/tray-icon.png");
const HELP_URL: &str = "https://tiez.name666.top/";
/// Opacity of the icon while recording is paused: a dimmed icon is how the
/// user notices, days later, why nothing new shows up.
const PAUSED_ICON_ALPHA: f32 = 0.4;

/// Events the main window listens for (App.tsx).
pub const EVENT_OPEN_SETTINGS: &str = "tray-open-settings";
pub const EVENT_CHECK_UPDATE: &str = "tray-check-update";

struct Labels {
    open: &'static str,
    pause: &'static str,
    settings: &'static str,
    check_update: &'static str,
    help: &'static str,
    quit: &'static str,
    tooltip: &'static str,
    tooltip_paused: &'static str,
}

fn labels(language: &str) -> Labels {
    match language {
        "en" => Labels {
            open: "Open TieZ",
            pause: "Pause Recording",
            settings: "Settings…",
            check_update: "Check for Updates…",
            help: "Help",
            quit: "Quit TieZ",
            tooltip: "TieZ",
            tooltip_paused: "TieZ (recording paused)",
        },
        "tw" => Labels {
            open: "打開 TieZ",
            pause: "暫停記錄",
            settings: "設定…",
            check_update: "檢查更新…",
            help: "使用說明",
            quit: "結束 TieZ",
            tooltip: "TieZ",
            tooltip_paused: "TieZ（已暫停記錄）",
        },
        _ => Labels {
            open: "打开 TieZ",
            pause: "暂停记录",
            settings: "设置…",
            check_update: "检查更新…",
            help: "使用帮助",
            quit: "退出 TieZ",
            tooltip: "TieZ",
            tooltip_paused: "TieZ（已暂停记录）",
        },
    }
}

fn current_language<R: Runtime>(app: &AppHandle<R>) -> String {
    app.try_state::<DbState>()
        .and_then(|db| db.settings_repo.get("app.language").ok().flatten())
        .unwrap_or_else(|| "zh".to_string())
}

fn current_open_hotkey<R: Runtime>(app: &AppHandle<R>) -> String {
    if let Some(settings) = app.try_state::<crate::app_state::SettingsState>() {
        let hotkey = settings
            .main_hotkey
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        if !hotkey.trim().is_empty() {
            return hotkey;
        }
    }
    crate::global_state::HOTKEY_STRING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

/// The open hotkey ("Alt+C", "Command+Shift+V") as a menu accelerator, or
/// None when the menu can't show it: not set, a bare key, or a mouse button.
fn menu_accelerator(hotkey: &str) -> Option<String> {
    let parts: Vec<&str> = hotkey
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    let (key, modifiers) = parts.split_last()?;
    if modifiers.is_empty() || key.to_ascii_uppercase().starts_with("MOUSE") {
        return None;
    }
    let mut out: Vec<String> = Vec::with_capacity(parts.len());
    for modifier in modifiers {
        let name = match modifier.to_ascii_uppercase().as_str() {
            "CTRL" | "CONTROL" => "Ctrl",
            "ALT" | "OPTION" | "MENU" => "Alt",
            "SHIFT" => "Shift",
            "COMMAND" | "CMD" | "WIN" | "SUPER" | "META" => "Super",
            _ => return None,
        };
        out.push(name.to_string());
    }
    out.push((*key).to_string());
    Some(out.join("+"))
}

/// A menu item that shows a shortcut. macOS draws it from the accelerator.
/// On Windows the accelerator also goes into a key table, so only the text
/// after a tab (which Windows right-aligns) is used: the menu shows the
/// shortcut without claiming the keys.
fn item_with_shortcut<R: Runtime>(
    app: &AppHandle<R>,
    id: &str,
    text: &str,
    shortcut: Option<&str>,
) -> tauri::Result<MenuItem<R>> {
    #[cfg(target_os = "macos")]
    {
        if let Some(accelerator) = shortcut {
            if let Ok(item) = MenuItem::with_id(app, id, text, true, Some(accelerator)) {
                return Ok(item);
            }
        }
        MenuItem::with_id(app, id, text, true, None::<&str>)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let text = match shortcut {
            Some(display) => format!("{text}\t{display}"),
            None => text.to_string(),
        };
        MenuItem::with_id(app, id, text, true, None::<&str>)
    }
}

fn build_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let l = labels(&current_language(app));
    let paused = USER_RECORDING_PAUSED.load(Ordering::Relaxed);
    let hotkey = current_open_hotkey(app);

    #[cfg(target_os = "macos")]
    let open_shortcut = menu_accelerator(&hotkey);
    #[cfg(not(target_os = "macos"))]
    let open_shortcut = menu_accelerator(&hotkey).map(|_| hotkey.trim().to_string());

    let open = item_with_shortcut(app, "open", l.open, open_shortcut.as_deref())?;
    let pause = CheckMenuItem::with_id(app, "pause", l.pause, true, paused, None::<&str>)?;
    #[cfg(target_os = "macos")]
    let (settings_shortcut, quit_shortcut) = (Some("Cmd+Comma"), Some("Cmd+Q"));
    #[cfg(not(target_os = "macos"))]
    let (settings_shortcut, quit_shortcut): (Option<&str>, Option<&str>) = (None, None);
    let settings = item_with_shortcut(app, "settings", l.settings, settings_shortcut)?;
    let check_update = MenuItem::with_id(app, "check_update", l.check_update, true, None::<&str>)?;
    let help = MenuItem::with_id(app, "help", l.help, true, None::<&str>)?;
    let quit = item_with_shortcut(app, "quit", l.quit, quit_shortcut)?;

    Menu::with_items(
        app,
        &[
            &open,
            &PredefinedMenuItem::separator(app)?,
            &pause,
            &PredefinedMenuItem::separator(app)?,
            &settings,
            &check_update,
            &help,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )
}

/// The tray icon, dimmed while recording is paused.
fn icon(paused: bool) -> tauri::Result<Image<'static>> {
    let base = Image::from_bytes(ICON_BYTES)?;
    if !paused {
        return Ok(base.to_owned());
    }
    let mut rgba = base.rgba().to_vec();
    for alpha in rgba.iter_mut().skip(3).step_by(4) {
        *alpha = (*alpha as f32 * PAUSED_ICON_ALPHA).round() as u8;
    }
    Ok(Image::new_owned(rgba, base.width(), base.height()))
}

/// Brings the menu, icon and tooltip in line with the language, the open
/// hotkey and the pause. Safe to call before the tray exists.
pub fn refresh<R: Runtime>(app: &AppHandle<R>) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let paused = USER_RECORDING_PAUSED.load(Ordering::Relaxed);
    let l = labels(&current_language(app));
    match build_menu(app) {
        Ok(menu) => {
            let _ = tray.set_menu(Some(menu));
        }
        Err(err) => eprintln!(">>> [TRAY] Failed to rebuild menu: {err}"),
    }
    if let Ok(image) = icon(paused) {
        let _ = tray.set_icon(Some(image));
        #[cfg(target_os = "macos")]
        let _ = tray.set_icon_as_template(true);
    }
    let _ = tray.set_tooltip(Some(if paused { l.tooltip_paused } else { l.tooltip }));
}

/// Opens the clipboard window, then tells it to show a page. When light mode
/// has released the window, the event waits until the page has been rebuilt.
fn open_window_then_emit(app: &AppHandle, event: &'static str) {
    if crate::app::light_mode::needs_rebuild(app) {
        crate::app::light_mode::rebuild_then(app, move |app| {
            let _ = crate::app::window_manager::focus_clipboard_window(app.clone());
            let _ = app.emit(event, ());
        });
        return;
    }
    let _ = crate::app::window_manager::focus_clipboard_window(app.clone());
    let _ = app.emit(event, ());
}

fn on_menu_event(app: &AppHandle, id: &str) {
    match id {
        "open" => {
            let _ = crate::app::window_manager::focus_clipboard_window(app.clone());
        }
        "pause" => {
            let paused = !USER_RECORDING_PAUSED.load(Ordering::Relaxed);
            USER_RECORDING_PAUSED.store(paused, Ordering::Relaxed);
            println!(">>> [TRAY] Recording {}", if paused { "paused" } else { "resumed" });
            refresh(app);
        }
        "settings" => open_window_then_emit(app, EVENT_OPEN_SETTINGS),
        "check_update" => open_window_then_emit(app, EVENT_CHECK_UPDATE),
        "help" => {
            use tauri_plugin_opener::OpenerExt;
            let _ = app.opener().open_url(HELP_URL, None::<&str>);
        }
        "quit" => crate::app::light_mode::request_app_exit(app),
        _ => {}
    }
}

pub fn setup(app: &tauri::App, hide_tray: bool) {
    let handle = app.handle().clone();
    let menu = build_menu(&handle).expect("Failed to build tray menu");
    let image = icon(false).expect("Failed to load tray icon");

    let mut tray_builder = TrayIconBuilder::with_id(TRAY_ID)
        .icon(image)
        .tooltip("TieZ")
        .show_menu_on_left_click(false)
        .menu(&menu)
        .on_menu_event(|app, event| on_menu_event(app, event.id.as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                ..
            } = event
            {
                // Same intentional-open path as Dock reopen (make-key + LAST_SHOW).
                let _ = crate::app::window_manager::focus_clipboard_window(tray.app_handle().clone());
            }
        });

    #[cfg(target_os = "macos")]
    {
        tray_builder = tray_builder.icon_as_template(true);
    }

    let tray = tray_builder.build(app).expect("Failed to build tray");
    let _ = tray.set_visible(!hide_tray);
    app.manage(tray);
    refresh(&handle);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotkeys_become_menu_accelerators() {
        assert_eq!(menu_accelerator("Alt+C").as_deref(), Some("Alt+C"));
        assert_eq!(menu_accelerator("Command+Shift+V").as_deref(), Some("Super+Shift+V"));
        assert_eq!(menu_accelerator("Ctrl+Shift+Space").as_deref(), Some("Ctrl+Shift+Space"));
        assert_eq!(menu_accelerator("Win+V").as_deref(), Some("Super+V"));
    }

    #[test]
    fn hotkeys_the_menu_cannot_show() {
        assert_eq!(menu_accelerator(""), None);
        assert_eq!(menu_accelerator("F9"), None);
        assert_eq!(menu_accelerator("Ctrl+MouseMiddle"), None);
        assert_eq!(menu_accelerator("Hyper+V"), None);
    }

    #[test]
    fn every_language_has_menu_labels() {
        for language in ["zh", "en", "tw", "unknown"] {
            let l = labels(language);
            assert!(!l.open.is_empty() && !l.pause.is_empty() && !l.quit.is_empty());
            assert!(l.tooltip_paused.starts_with("TieZ"));
        }
    }
}
