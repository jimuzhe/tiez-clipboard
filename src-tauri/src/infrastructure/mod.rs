pub mod encryption;
pub mod repository;
#[cfg(target_os = "windows")]
pub mod windows_ext;

#[cfg(target_os = "windows")]
pub mod windows_api;

#[cfg(not(target_os = "windows"))]
pub mod windows_api {
    pub mod win_clipboard {
        pub struct ImageData {
            pub width: usize,
            pub height: usize,
            pub bytes: Vec<u8>,
        }
        static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
        pub fn get_clipboard_sequence_number() -> u32 {
            SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        }
        pub unsafe fn get_clipboard_image() -> Option<ImageData> {
            None
        }
        pub unsafe fn get_clipboard_files() -> Option<Vec<String>> {
            None
        }
        pub unsafe fn get_clipboard_raw_format(_name: &str) -> Option<Vec<u8>> {
            None
        }
        pub unsafe fn set_clipboard_files(_paths: Vec<String>) -> Result<(), String> {
            Ok(())
        }
        pub unsafe fn set_clipboard_text_and_html(_text: &str, _: &str) -> Result<(), String> {
            Ok(())
        }
        pub fn set_clipboard_image_with_formats(_data: ImageData) -> Result<(), String> {
            Ok(())
        }
    }

    pub mod window_tracker {
        pub fn start_window_tracking(_app_handle: tauri::AppHandle) {}
        #[derive(Debug, Clone, Default)]
        pub struct ActiveAppInfo {
            pub app_name: String,
            pub process_path: Option<String>,
        }
        pub fn get_active_app_info() -> ActiveAppInfo {
            ActiveAppInfo {
                app_name: "FallbackApp".into(),
                process_path: None,
            }
        }
        pub fn get_clipboard_source_app_info() -> ActiveAppInfo {
            ActiveAppInfo {
                app_name: "FallbackApp".into(),
                process_path: None,
            }
        }
    }

    pub mod apps {
        pub fn launch_uwp_with_file(
            _package: &str,
            _file: &str,
        ) -> Result<(), Box<dyn std::error::Error>> {
            Ok(())
        }
        pub fn get_system_default_app(_ext: &str) -> String {
            "".into()
        }
        pub fn get_executable_icon(_executable_path: String) -> Result<Option<String>, String> {
            Ok(None)
        }
        pub fn get_file_icon(_file_path: String) -> Result<Option<String>, String> {
            Ok(None)
        }
        pub fn scan_installed_apps() -> Vec<serde_json::Value> {
            vec![]
        }
        pub fn get_associated_apps(_ext: &str) -> Vec<serde_json::Value> {
            vec![]
        }
    }

    pub mod drag_drop {
        pub fn register_emoji_drag_drop(_app_handle: tauri::AppHandle) {}
    }
}
