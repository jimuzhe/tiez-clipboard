use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::SystemTime;
use tauri::AppHandle;
use tokio::sync::broadcast;

pub struct AppState {
    pub app_handle: AppHandle,
    pub ws_tx: broadcast::Sender<String>,
}

#[derive(Default)]
pub struct ServerActivityState {
    pub last_activity: Mutex<Option<SystemTime>>,
}

pub struct WsBroadcaster(pub Mutex<Option<broadcast::Sender<String>>>);

#[derive(Deserialize)]
pub struct ReceiveText {
    pub content: String,
    pub sender_id: String,
    pub sender_name: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: u64,
    pub direction: String, // "in" = Mobile->PC, "out" = PC->Mobile
    pub msg_type: String,  // "text", "file", "image"
    pub content: String,
    pub timestamp: i64,
    pub sender_id: String,   // Device unique ID
    pub sender_name: String, // Device display name (e.g., "iPhone X", "PC")
    pub file_path: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct StatusPayload {
    pub enabled: bool,
    pub port: u16,
    pub ip: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub last_seen: i64,
}

pub struct OnlineDevices(pub Mutex<HashMap<String, DeviceInfo>>);

pub struct ChatState(pub Mutex<Vec<Message>>);

impl Default for ChatState {
    fn default() -> Self {
        Self(Mutex::new(Vec::new()))
    }
}

#[derive(Deserialize, Debug)]
pub struct ChunkMetadata {
    pub upload_id: String,
    pub chunk_index: usize,
    pub total_chunks: usize,
    pub file_name: String,
    pub sender_id: String,
    pub sender_name: String,
    pub total_size: u64,
    pub content_type: Option<String>,
}

pub struct UploadSessions(pub Mutex<HashMap<String, std::path::PathBuf>>);

impl Default for UploadSessions {
    fn default() -> Self {
        Self(Mutex::new(HashMap::new()))
    }
}

pub struct SharedFileState(pub Mutex<HashMap<String, String>>);

pub struct ServerInfo {
    pub port: std::sync::atomic::AtomicU16,
    pub ip: Mutex<String>,
}
