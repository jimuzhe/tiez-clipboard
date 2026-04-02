use crate::infrastructure::linux_api::{detect_display_server, DisplayServer};

#[derive(Debug, Clone, Default)]
pub struct ActiveAppInfo {
    pub app_name: String,
    pub process_path: Option<String>,
}

static ACTIVE_APP: std::sync::OnceLock<std::sync::RwLock<ActiveAppInfo>> = std::sync::OnceLock::new();

fn get_active_app_store() -> &'static std::sync::RwLock<ActiveAppInfo> {
    ACTIVE_APP.get_or_init(|| std::sync::RwLock::new(ActiveAppInfo::default()))
}

pub fn start_window_tracking(_app_handle: tauri::AppHandle) {
    let display_server = detect_display_server();

    if display_server != DisplayServer::X11 {
        return;
    }

    std::thread::spawn(move || {
        #[cfg(target_os = "linux")]
        {
            if let Ok((conn, _)) = x11rb::connect(None) {
                loop {
                    if let Some(app_name) = get_active_window_app_name(&conn) {
                        if let Ok(mut store) = get_active_app_store().write() {
                            store.app_name = app_name;
                            store.process_path = None;
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
            }
        }
    });
}

#[cfg(target_os = "linux")]
fn get_active_window_app_name(conn: &x11rb::rust_connection::RustConnection) -> Option<String> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{ConnectionExt, AtomEnum};

    let screen = conn.setup().roots.first()?;
    let root = screen.root;

    let active_window_atom = conn.intern_atom(false, b"_NET_ACTIVE_WINDOW").ok()?.reply().ok()?.atom;

    let reply = conn.get_property(false, root, active_window_atom, AtomEnum::WINDOW, 0, 1).ok()?.reply().ok()?;

    // Parse the window ID from the value (u32 in native byte order)
    if reply.value.len() < 4 {
        return None;
    }

    let active_window = u32::from_ne_bytes([reply.value[0], reply.value[1], reply.value[2], reply.value[3]]);
    if active_window == 0 {
        return None;
    }

    let wm_class_atom = conn.intern_atom(false, b"WM_CLASS").ok()?.reply().ok()?.atom;

    let class_reply = conn.get_property(false, active_window, wm_class_atom, AtomEnum::STRING, 0, 1024).ok()?.reply().ok()?;

    let class_data = class_reply.value;
    if class_data.is_empty() {
        return None;
    }

    let class_string = String::from_utf8_lossy(&class_data);
    let parts: Vec<&str> = class_string.split('\0').filter(|s| !s.is_empty()).collect();

    parts.last().map(|s| s.to_string())
}

#[cfg(not(target_os = "linux"))]
fn get_active_window_app_name(_conn: &()) -> Option<String> {
    None
}

pub fn get_active_app_info() -> ActiveAppInfo {
    let display_server = detect_display_server();

    if display_server != DisplayServer::X11 {
        return ActiveAppInfo {
            app_name: "Unknown".into(),
            process_path: None,
        };
    }

    if let Ok(store) = get_active_app_store().read() {
        store.clone()
    } else {
        ActiveAppInfo::default()
    }
}

pub fn get_clipboard_source_app_info() -> ActiveAppInfo {
    get_active_app_info()
}
