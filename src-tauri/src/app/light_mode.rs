//! Idle "light mode": drop the main WebView's page to free memory while
//! clipboard listening / hotkeys / tray keep running; rebuild on next summon
//! (#154). Windows destroys the window. macOS cannot: tauri-nspanel replaces
//! the window delegate, so Tauri never sees the close and the panel (with its
//! WKWebView) stays alive. There the page is swapped for about:blank instead,
//! which frees the JS heap, DOM and decoded images, and is reloaded on summon.

use crate::app_state::SettingsState;
use crate::database::DbState;
use crate::global_state::{
    CURRENT_DOCK, IS_HIDDEN, LAST_SHOW_TIMESTAMP, NAVIGATION_ENABLED, NAVIGATION_MODE_ACTIVE,
    WINDOW_PINNED,
};
use crate::infrastructure::repository::settings_repo::SettingsRepository;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow, WebviewWindowBuilder};

/// True while the main WebView has been destroyed for light mode.
pub static WEBVIEW_LIGHT_MODE: AtomicBool = AtomicBool::new(false);

/// Wall-clock ms when the UI was last hidden. 0 = currently considered "in use".
static LAST_HIDDEN_AT_MS: AtomicU64 = AtomicU64::new(0);

/// When true, `ExitRequested` is allowed to tear down the process (tray Quit etc.).
pub static ALLOW_APP_EXIT: AtomicBool = AtomicBool::new(false);

/// The main page has been unmounted or unloaded for light mode; it must be
/// reloaded from APP_URL before the window is shown again.
static PAGE_UNLOADED: AtomicBool = AtomicBool::new(false);
static APP_URL: std::sync::Mutex<Option<tauri::Url>> = std::sync::Mutex::new(None);

/// The main UI must be rebuilt (or reloaded) before it can be shown.
pub fn needs_rebuild(app: &AppHandle) -> bool {
    app.get_webview_window("main").is_none() || PAGE_UNLOADED.load(Ordering::SeqCst)
}

/// False from rebuilding the WebView until its UI has rendered (main_ui_ready).
static UI_READY: AtomicBool = AtomicBool::new(true);
const UI_READY_TIMEOUT: Duration = Duration::from_millis(1500);

/// Set by the page once it has unmounted ahead of a light-mode release.
static UI_RELEASED: AtomicBool = AtomicBool::new(false);
const UI_RELEASE_TIMEOUT: Duration = Duration::from_millis(1000);

pub fn mark_ui_released() {
    UI_RELEASED.store(true, Ordering::SeqCst);
}

/// Called by the frontend once settings and theme are applied.
pub fn mark_ui_ready() {
    UI_READY.store(true, Ordering::SeqCst);
}

/// A rebuild is under way; further summons wait for it instead of starting
/// another (two would show the window and then toggle it shut).
static REBUILDING: AtomicBool = AtomicBool::new(false);

/// Rebuild (or reload) the main UI, then run `show` once it has rendered or
/// after a short timeout, so a summon never flashes an empty shell.
///
/// Always off the calling thread: hotkeys, the tray and sync commands call in
/// on the main thread, and on Windows creating the WebView2 there waits on the
/// event loop it is blocking, which froze the app on the first summon after
/// light mode (wry#583).
pub fn rebuild_then<F>(app: &AppHandle, show: F)
where
    F: FnOnce(&AppHandle) + Send + 'static,
{
    if REBUILDING.swap(true, Ordering::SeqCst) {
        eprintln!("[light-mode] rebuild already under way; summon dropped");
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        if let Err(err) = ensure_main_window(&app) {
            REBUILDING.store(false, Ordering::SeqCst);
            eprintln!("[light-mode] rebuild failed: {err}");
            return;
        }
        let started = Instant::now();
        while !UI_READY.load(Ordering::SeqCst) && started.elapsed() < UI_READY_TIMEOUT {
            std::thread::sleep(Duration::from_millis(15));
        }
        eprintln!(
            "[light-mode] showing rebuilt window after {} ms ({})",
            started.elapsed().as_millis(),
            if UI_READY.load(Ordering::SeqCst) { "UI ready" } else { "timed out" }
        );
        REBUILDING.store(false, Ordering::SeqCst);
        show(&app);
    });
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn on_window_shown() {
    LAST_HIDDEN_AT_MS.store(0, Ordering::SeqCst);
    WEBVIEW_LIGHT_MODE.store(false, Ordering::SeqCst);
}

pub fn on_window_hidden() {
    LAST_HIDDEN_AT_MS.store(now_ms(), Ordering::SeqCst);
}

pub fn request_app_exit(app: &AppHandle) {
    ALLOW_APP_EXIT.store(true, Ordering::SeqCst);
    app.exit(0);
}

/// Bench override `TIEZ_LIGHT_MODE_IDLE_SECS=15`: light mode on, entered after
/// 15 s instead of minutes. Debug builds only, so users can't trip it.
fn bench_idle_secs() -> Option<u64> {
    if !cfg!(debug_assertions) {
        return None;
    }
    std::env::var("TIEZ_LIGHT_MODE_IDLE_SECS").ok()?.parse().ok()
}

fn light_mode_enabled(app: &AppHandle) -> bool {
    if bench_idle_secs().is_some() {
        return true;
    }
    app.try_state::<SettingsState>()
        .map(|s| s.light_mode_enabled.load(Ordering::Relaxed))
        .unwrap_or(false)
}

fn light_mode_idle_minutes(app: &AppHandle) -> u64 {
    app.try_state::<SettingsState>()
        .map(|s| s.light_mode_idle_minutes.load(Ordering::Relaxed).max(1))
        .unwrap_or(15)
}

fn light_mode_idle_ms(app: &AppHandle) -> u64 {
    if let Some(secs) = bench_idle_secs() {
        return secs.saturating_mul(1000).max(1_000);
    }
    light_mode_idle_minutes(app).saturating_mul(60_000)
}

fn watcher_interval() -> Duration {
    if bench_idle_secs().is_some() {
        Duration::from_secs(2)
    } else {
        Duration::from_secs(30)
    }
}

fn trim_working_set() {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::System::ProcessStatus::EmptyWorkingSet;
        use windows::Win32::System::Threading::GetCurrentProcess;
        unsafe {
            let _ = EmptyWorkingSet(GetCurrentProcess());
        }
    }
}

fn apply_post_create(app: &AppHandle, window: &WebviewWindow) {
    if let Some(db) = app.try_state::<DbState>() {
        let width = db
            .settings_repo
            .get("app.window_width")
            .ok()
            .flatten()
            .and_then(|v| v.parse::<u32>().ok());
        let height = db
            .settings_repo
            .get("app.window_height")
            .ok()
            .flatten()
            .and_then(|v| v.parse::<u32>().ok());
        if let (Some(w), Some(h)) = (width, height) {
            if w >= 250 && h >= 200 {
                let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize {
                    width: w,
                    height: h,
                }));
            }
        }

        let theme = db
            .settings_repo
            .get("app.theme")
            .ok()
            .flatten()
            .unwrap_or_else(|| crate::build_config::default_theme_id().to_string());
        let mode = db
            .settings_repo
            .get("app.color_mode")
            .ok()
            .flatten();
        if let Some(settings) = app.try_state::<SettingsState>() {
            let _ = crate::app::commands::set_theme(
                window.clone(),
                settings,
                db,
                theme,
                mode,
                None,
            );
        }
    }

    let pinned = WINDOW_PINNED.load(Ordering::Relaxed);
    let _ = window.set_always_on_top(pinned);

    #[cfg(target_os = "windows")]
    {
        let _ = window.set_focusable(!pinned);
        crate::infrastructure::windows_api::webview_keys::disable_browser_accelerator_keys_for_window(
            window,
        );
        crate::infrastructure::windows_api::drag_drop::register_emoji_drag_drop(app.clone());
    }

    #[cfg(target_os = "macos")]
    {
        crate::infrastructure::macos_api::window::configure_overlay_space_behavior(window);
        if let Err(err) =
            crate::infrastructure::macos_api::window::setup_clipboard_panel(window)
        {
            eprintln!("[light-mode] setup_clipboard_panel failed: {err}");
        }
        crate::infrastructure::macos_api::window::configure_overlay_space_behavior(window);
        crate::app::window_manager::apply_macos_window_material(window).ok();
    }

    crate::app::window_manager::maybe_open_devtools(window);
}

/// Ensure the main WebView exists (recreate after light-mode destroy). Runs on
/// rebuild_then's thread, never the main one.
fn ensure_main_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window("main") {
        if PAGE_UNLOADED.load(Ordering::SeqCst) {
            reload_page(&window)?;
        }
        WEBVIEW_LIGHT_MODE.store(false, Ordering::SeqCst);
        return Ok(window);
    }

    let cfg = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == "main")
        .cloned()
        .ok_or_else(|| "main window config missing".to_string())?;

    UI_READY.store(false, Ordering::SeqCst);
    let builder = WebviewWindowBuilder::from_config(app, &cfg).map_err(|e| e.to_string())?;
    let window = builder
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?;

    // Window, WebView2 and OLE drag-and-drop setup belong to the UI thread.
    // It is free (this thread is not it), so waiting for it cannot deadlock.
    let (done, wait) = std::sync::mpsc::channel();
    let (post_app, post_window) = (app.clone(), window.clone());
    app.run_on_main_thread(move || {
        #[cfg(target_os = "windows")]
        crate::app::hooks::remember_main_hwnd(&post_window);
        apply_post_create(&post_app, &post_window);
        let _ = done.send(());
    })
    .map_err(|e| e.to_string())?;
    if wait.recv_timeout(Duration::from_secs(3)).is_err() {
        eprintln!("[light-mode] window setup on the main thread timed out");
    }
    PAGE_UNLOADED.store(false, Ordering::SeqCst);
    WEBVIEW_LIGHT_MODE.store(false, Ordering::SeqCst);
    on_window_shown();
    eprintln!("[light-mode] main WebView recreated");
    Ok(window)
}

fn window_is_idle_candidate(app: &AppHandle) -> bool {
    if WINDOW_PINNED.load(Ordering::Relaxed) {
        return false;
    }
    if CURRENT_DOCK.load(Ordering::Relaxed) != 0 || IS_HIDDEN.load(Ordering::Relaxed) {
        // Edge-docked: keep WebView for quick hover reveal.
        return false;
    }
    let Some(window) = app.get_webview_window("main") else {
        return false;
    };
    !window.is_visible().unwrap_or(true)
}

pub fn try_enter_light_mode(app: &AppHandle) {
    if !light_mode_enabled(app) {
        return;
    }
    if WEBVIEW_LIGHT_MODE.load(Ordering::SeqCst) {
        return;
    }
    if !window_is_idle_candidate(app) {
        return;
    }

    let hidden_at = LAST_HIDDEN_AT_MS.load(Ordering::SeqCst);
    if hidden_at == 0 {
        return;
    }
    let idle_ms = light_mode_idle_ms(app);
    if now_ms().saturating_sub(hidden_at) < idle_ms {
        return;
    }

    if let Some(preview) = app.get_webview_window("compact-preview") {
        let _ = preview.destroy();
    }

    let Some(main) = app.get_webview_window("main") else {
        return;
    };
    if main.is_visible().unwrap_or(false) {
        return;
    }
    let Ok(url) = main.url() else {
        return;
    };
    if url.scheme() != "about" {
        *APP_URL.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(url);
    }

    // 1. Let the page unmount React first: Tauri keeps a webview's JS event
    //    listeners across reloads and destroy, so without the effect cleanups
    //    every cycle would leave ~50 dead listeners behind. From here on a
    //    summon must reload the page (needs_rebuild).
    PAGE_UNLOADED.store(true, Ordering::SeqCst);
    UI_RELEASED.store(false, Ordering::SeqCst);
    let _ = main.emit_to("main", "light-mode-release", ());
    let started = Instant::now();
    while !UI_RELEASED.load(Ordering::SeqCst) && started.elapsed() < UI_RELEASE_TIMEOUT {
        std::thread::sleep(Duration::from_millis(15));
    }
    eprintln!(
        "[light-mode] page unmount after {} ms ({})",
        started.elapsed().as_millis(),
        if UI_RELEASED.load(Ordering::SeqCst) { "acknowledged" } else { "timed out" }
    );

    // 2. Release on the main thread, where window show/hide also runs, so a
    //    summon in between can't end up showing a page that is going away.
    let app = app.clone();
    let _ = app.clone().run_on_main_thread(move || {
        let Some(main) = app.get_webview_window("main") else {
            return;
        };
        // A summon in the meantime reloaded the page (or is showing it).
        if !PAGE_UNLOADED.load(Ordering::SeqCst) || main.is_visible().unwrap_or(false) {
            return;
        }
        WEBVIEW_LIGHT_MODE.store(true, Ordering::SeqCst);
        NAVIGATION_ENABLED.store(false, Ordering::SeqCst);
        NAVIGATION_MODE_ACTIVE.store(false, Ordering::SeqCst);
        crate::global_state::SEARCH_TOP_HIT_ACTIVE.store(false, Ordering::SeqCst);

        // macOS can't destroy (see module docs); blank the page instead.
        #[cfg(target_os = "macos")]
        let released = tauri::Url::parse("about:blank")
            .map_err(|err| err.to_string())
            .and_then(|blank| main.navigate(blank).map_err(|err| err.to_string()));
        #[cfg(not(target_os = "macos"))]
        let released = main.destroy().map_err(|err| err.to_string());

        if let Err(err) = released {
            WEBVIEW_LIGHT_MODE.store(false, Ordering::SeqCst);
            eprintln!("[light-mode] release failed: {err}");
            return;
        }
        eprintln!("[light-mode] UI released after {} ms idle", idle_ms);
        trim_working_set();
    });
}

/// Load the app page back into a window whose page light mode unloaded.
fn reload_page(window: &WebviewWindow) -> Result<(), String> {
    let url = APP_URL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
        .ok_or_else(|| "app URL unknown".to_string())?;
    UI_READY.store(false, Ordering::SeqCst);
    // Cleared first so a release still queued on the main thread backs off.
    PAGE_UNLOADED.store(false, Ordering::SeqCst);
    window.navigate(url).map_err(|err| err.to_string())?;
    eprintln!("[light-mode] main page reloaded");
    Ok(())
}

pub fn start_idle_watcher(app: AppHandle) {
    let watcher_app = app.clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(watcher_interval());
            if !light_mode_enabled(&watcher_app) {
                continue;
            }
            try_enter_light_mode(&watcher_app);
        }
    });

    // Deterministic memory bench: --bench-light-mode
    // shows briefly, hides, waits idle secs (env or 15), then destroys WebView.
    if std::env::args().any(|a| a == "--bench-light-mode") {
        let app2 = app.clone();
        std::thread::spawn(move || {
            let idle_ms = light_mode_idle_ms(&app2).max(5_000);
            std::thread::sleep(Duration::from_secs(5));
            if let Some(window) = app2.get_webview_window("main") {
                let _ = window.hide();
            }
            on_window_hidden();
            eprintln!(
                "[light-mode] bench: hidden, waiting {} ms before destroy",
                idle_ms
            );
            std::thread::sleep(Duration::from_millis(idle_ms + 2_000));
            try_enter_light_mode(&app2);
            // destroy() is dispatched to the main thread; look once it has run.
            std::thread::sleep(Duration::from_secs(2));
            eprintln!(
                "[light-mode] bench: after try_enter light_mode={} window={} page_unloaded={}",
                WEBVIEW_LIGHT_MODE.load(Ordering::SeqCst),
                app2.get_webview_window("main").is_some(),
                PAGE_UNLOADED.load(Ordering::SeqCst)
            );
        });
    }
}

/// Touch hidden timestamp if the window is already hidden (e.g. silent start).
pub fn seed_hidden_if_needed(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if !window.is_visible().unwrap_or(true) {
            on_window_hidden();
        }
    } else {
        // No window yet — treat as already light.
        WEBVIEW_LIGHT_MODE.store(true, Ordering::SeqCst);
        on_window_hidden();
    }
    let _ = LAST_SHOW_TIMESTAMP.load(Ordering::Relaxed);
}
