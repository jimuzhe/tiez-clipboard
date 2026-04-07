pub mod encryption;
pub mod macos_api;
pub mod repository;

pub mod macos_ext {
    pub struct WindowExt;
    impl WindowExt {
        pub fn show_error_box(_title: &str, _msg: &str) {
            eprintln!("ERROR: {}: {}", _title, _msg);
        }
        pub fn release_modifier_keys() {}
        pub fn get_foreground_window() -> isize {
            0
        }
        pub fn get_window_rect(_hwnd: isize) -> Option<Rect> {
            None
        }
        pub fn force_focus_window(_hwnd: isize) {}
        pub fn show_window_no_activate(_hwnd: isize) {}
        pub fn show_window_no_activate_normal(_hwnd: isize) {}
    }

    pub struct Rect {
        pub left: i32,
        pub top: i32,
        pub right: i32,
        pub bottom: i32,
    }
}
