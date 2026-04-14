use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;
use tauri::Emitter;

use crate::app::window_manager::toggle_window;
use crate::global_state::{GLOBAL_APP_HANDLE, HOTKEY_STRING};

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ButtonIndex, ConnectionExt, EventMask, GrabMode, ModMask};

struct GrabGuard<'a> {
    conn: &'a x11rb::rust_connection::RustConnection,
    root: x11rb::protocol::xproto::Window,
}

impl<'a> Drop for GrabGuard<'a> {
    fn drop(&mut self) {
        let _ = self
            .conn
            .ungrab_button(ButtonIndex::from(2u8), self.root, ModMask::ANY);
        let _ = self.conn.flush();
    }
}

pub fn start_mouse_hotkey_listener() {
    thread::spawn(|| {
        let Ok((conn, screen_num)) = x11rb::connect(None) else {
            eprintln!("[mouse-hotkey] Failed to connect to X11 display");
            return;
        };

        let screen = match conn.setup().roots.get(screen_num) {
            Some(s) => s,
            None => {
                eprintln!("[mouse-hotkey] Invalid screen number");
                return;
            }
        };
        let root = screen.root;

        if let Err(e) = conn.grab_button(
            false,
            root,
            EventMask::BUTTON_PRESS,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
            x11rb::NONE,
            x11rb::NONE,
            ButtonIndex::from(2u8),
            ModMask::ANY,
        ) {
            eprintln!("[mouse-hotkey] Failed to grab middle mouse button: {:?}", e);
            return;
        }

        if let Err(e) = conn.flush() {
            eprintln!("[mouse-hotkey] Failed to flush connection: {:?}", e);
            return;
        }

        let _guard = GrabGuard { conn: &conn, root };

        loop {
            match conn.poll_for_event() {
                Ok(Some(event)) => {
                    if let x11rb::protocol::Event::ButtonPress(evt) = event {
                        if evt.detail == 2 {
                            let configured = HOTKEY_STRING.lock().unwrap().clone();
                            let matched = configured
                                .split(['\n', '\r'])
                                .map(str::trim)
                                .filter(|s| !s.is_empty())
                                .any(|item| {
                                    let lower = item.to_lowercase();
                                    lower == "mousemiddle" || lower == "mbutton"
                                });

                            if matched {
                                if let Some(handle) = GLOBAL_APP_HANDLE.get() {
                                    let _ = handle.emit("global-shortcut-triggered", "MouseMiddle");
                                    toggle_window(handle);
                                }
                            }
                        }
                    }
                }
                Ok(None) => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(e) => {
                    eprintln!("[mouse-hotkey] X11 event error: {:?}", e);
                    break;
                }
            }
        }
    });
}
