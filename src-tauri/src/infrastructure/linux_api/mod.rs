pub mod win_clipboard;
pub mod window_tracker;
pub mod desktop_integration;
pub mod apps;

use std::env;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayServer {
    X11,
    Wayland,
    Unknown,
}

pub fn detect_display_server() -> DisplayServer {
    if env::var("WAYLAND_DISPLAY").is_ok() {
        DisplayServer::Wayland
    } else if env::var("DISPLAY").is_ok() {
        DisplayServer::X11
    } else {
        DisplayServer::Unknown
    }
}
