use std::collections::VecDeque;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use crate::domain::models::ClipboardEntry;
use crate::services::encryption_queue::EncryptionQueue;

pub struct SettingsState {
    pub deduplicate: AtomicBool,
    pub persistent: AtomicBool,
    pub theme: Mutex<String>,
    pub capture_files: AtomicBool,
    pub capture_rich_text: AtomicBool,
    pub silent_start: AtomicBool,
    pub delete_after_paste: AtomicBool,
    pub privacy_protection: AtomicBool,
    pub privacy_protection_kinds: Mutex<Vec<String>>,
    pub privacy_protection_custom_rules: Mutex<Vec<String>>,
    pub sequential_mode: AtomicBool,
    pub sequential_paste_hotkey: Mutex<String>,
    pub rich_paste_hotkey: Mutex<String>,
    pub search_hotkey: Mutex<String>,
    pub sound_enabled: AtomicBool,
    pub hide_tray_icon: AtomicBool,
    pub edge_docking: AtomicBool,
    pub follow_mouse: AtomicBool,
    pub arrow_key_selection: AtomicBool,
    pub main_hotkey: Mutex<String>,
    pub monitors: Mutex<Vec<tauri::Monitor>>,
}

#[derive(Default)]
pub struct PasteQueueState {
    pub items: VecDeque<i64>,
    pub last_action_was_paste: bool,
    pub last_pasted_content: Option<String>,
}

#[derive(Default)]
pub struct PasteQueue(pub Mutex<PasteQueueState>);

pub struct SessionHistory(pub Mutex<VecDeque<ClipboardEntry>>);

pub struct AppDataDir(pub Mutex<std::path::PathBuf>);

pub struct EncryptionQueueState(pub EncryptionQueue);
