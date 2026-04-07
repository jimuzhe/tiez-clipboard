#[cfg(target_os = "macos")]
use objc2::rc::autoreleasepool;
#[cfg(target_os = "macos")]
use objc2_app_kit::NSWorkspace;
#[cfg(target_os = "macos")]
use objc2_foundation::NSString;
use tauri::AppHandle;

#[derive(Debug, Clone, Default)]
pub struct ActiveAppInfo {
    pub app_name: String,
    pub pid: u32,
    pub process_path: Option<String>,
}

pub fn start_window_tracking(_app_handle: AppHandle) {
    #[cfg(target_os = "macos")]
    std::thread::spawn(move || {
        use std::sync::atomic::Ordering;
        let self_pid = std::process::id();
        loop {
            std::thread::sleep(std::time::Duration::from_millis(500));
            // Only update track if TieZ is NOT actually focused right now,
            // to avoid overwriting the "previous" app with "TieZ" itself.
            if !crate::global_state::IS_MAIN_WINDOW_FOCUSED.load(Ordering::Relaxed) {
                let info = get_active_app_snapshot();
                if info.pid != 0 {
                    let name = info.app_name;
                    let pid = info.pid;
                    if pid != 0 && pid != self_pid && !name.eq_ignore_ascii_case("TieZ") {
                        crate::global_state::LAST_ACTIVE_APP_PID.store(pid, Ordering::Relaxed);
                        crate::global_state::set_last_active_app_name(name);
                    }
                }
            }
        }
    });
}

pub fn get_active_app_snapshot() -> ActiveAppInfo {
    #[cfg(target_os = "macos")]
    return autoreleasepool(|_| {
        let workspace = NSWorkspace::sharedWorkspace();
        let app = workspace.frontmostApplication();

        if let Some(app) = app {
            let name = app
                .localizedName()
                .map(|s: objc2::rc::Retained<NSString>| s.to_string())
                .unwrap_or_else(|| "Unknown".to_string());
            let pid = app.processIdentifier() as u32;
            let bundle_url = app.bundleURL();
            let process_path = if let Some(url) = bundle_url {
                url.path()
                    .map(|s: objc2::rc::Retained<NSString>| s.to_string())
            } else {
                None
            };

            ActiveAppInfo {
                app_name: name,
                pid,
                process_path,
            }
        } else {
            ActiveAppInfo {
                app_name: "macOS App".into(),
                pid: 0,
                process_path: None,
            }
        }
    });

    #[cfg(not(target_os = "macos"))]
    ActiveAppInfo {
        app_name: "Unknown App".into(),
        pid: 0,
        process_path: None,
    }
}

pub fn get_active_app_info() -> (String, String) {
    let info = get_active_app_snapshot();
    let pid = if info.pid == 0 {
        String::new()
    } else {
        info.pid.to_string()
    };
    (info.app_name, pid)
}
