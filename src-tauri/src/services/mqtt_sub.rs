use rumqttc::{MqttOptions, AsyncClient, QoS, Event, Incoming, Transport};
use tauri::{AppHandle, Manager, Emitter};
use std::time::Duration;
use tokio::time::sleep;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use crate::database::{DbState};
use crate::infrastructure::repository::settings_repo::SettingsRepository;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use crate::global_state::{LAST_APP_SET_HASH, LAST_APP_SET_TIMESTAMP};
use crate::{info, error};

#[derive(Clone, Debug)]
struct MqttConfig {
    host: String,
    port: u16,
    username: Option<String>,
    password: Option<String>,
    topic: String,
    client_id: String,
    protocol: String,
    ssl: bool,
    ws_path: String,
    #[allow(dead_code)]
    tls_insecure: bool,
}

// Global MQTT client state
static MQTT_RUNNING: AtomicBool = AtomicBool::new(false);
static MQTT_CONNECTED: AtomicBool = AtomicBool::new(false);
static MQTT_RECONNECT_ATTEMPTS: AtomicU32 = AtomicU32::new(0);
static MQTT_TASK_ACTIVE: AtomicBool = AtomicBool::new(false);

struct MqttTaskGuard;

impl Drop for MqttTaskGuard {
    fn drop(&mut self) {
        MQTT_TASK_ACTIVE.store(false, Ordering::Relaxed);
    }
}

pub fn get_mqtt_status() -> bool {
    MQTT_CONNECTED.load(Ordering::Relaxed)
}

pub fn get_mqtt_running() -> bool {
    MQTT_RUNNING.load(Ordering::Relaxed)
}

// Force restart MQTT client by setting running flag to false
pub fn restart_mqtt_client(app: AppHandle) {
    info!(">>> [MQTT] Restart requested.");
    MQTT_RUNNING.store(false, Ordering::Relaxed);
    MQTT_CONNECTED.store(false, Ordering::Relaxed);
    MQTT_RECONNECT_ATTEMPTS.store(0, Ordering::Relaxed);

    // 如果之前达到最大重连次数导致任务退出，重新启动任务
    if !MQTT_TASK_ACTIVE.load(Ordering::Relaxed) {
        start_mqtt_client(app);
    }
}

// Function to read config from DB
fn get_mqtt_config(app: &AppHandle) -> Option<MqttConfig> {
    let db_state = match app.try_state::<DbState>() {
        Some(s) => s,
        None => return None,
    };
    
    let enabled_str = db_state.settings_repo.get("mqtt_enabled").ok().flatten().unwrap_or("true".to_string());
    if enabled_str != "true" {
        return None;
    }

    let host = db_state.settings_repo.get("mqtt_server").ok().flatten()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("DEFAULT_MQTT_SERVER").ok().filter(|s| !s.is_empty()));

    if host.is_none() || host.as_ref().unwrap().is_empty() { 
        return None; 
    }
    let host = host.unwrap();
    
    let port_str = db_state.settings_repo.get("mqtt_port").ok().flatten();
    let port = port_str.and_then(|p| p.parse::<u16>().ok()).unwrap_or(443);
    
    let password = db_state.settings_repo.get("mqtt_password").ok().flatten()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("DEFAULT_MQTT_PASSWORD").ok().filter(|s| !s.is_empty()));

    // Topic logic (using shortened ID)
    let anon_id = db_state
        .settings_repo
        .get("app.anon_id")
        .ok()
        .flatten()
        .and_then(|v| crate::app::system::normalize_anon_id(&v))
        .unwrap_or_else(|| {
            let machine_id = crate::app::system::get_machine_id();
            let new_id = crate::app::system::build_anon_id(&machine_id);
            let _ = db_state.settings_repo.set("app.anon_id", &new_id);
            new_id
        });
    let short_id = &anon_id;
    let default_topic = format!("tiez/tiez_{}", short_id);
    
    let db_topic = db_state.settings_repo.get("mqtt_topic").ok().flatten().filter(|s| !s.is_empty());
    let topic = db_topic.unwrap_or_else(|| {
        let _ = db_state.settings_repo.set("mqtt_topic", &default_topic);
        default_topic.clone()
    });

    let custom_client_id = db_state.settings_repo.get("mqtt_client_id").ok().flatten().filter(|s| !s.is_empty());
    let default_client_id = format!("tiez_pc_{}", short_id);
    let client_id = custom_client_id.unwrap_or(default_client_id);

    let username = db_state.settings_repo.get("mqtt_username").ok().flatten()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("DEFAULT_MQTT_USERNAME").ok().filter(|s| !s.is_empty()));

    let protocol = db_state.settings_repo.get("mqtt_protocol").ok().flatten()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            if port == 8883 || port == 443 {
                "mqtts://".to_string()
            } else {
                "mqtt://".to_string()
            }
        });

    // Prefer the explicit protocol selection. `mqtt_ssl` is a legacy flag and
    // should not force TLS when the user selected mqtt:// or ws://.
    let legacy_ssl = db_state.settings_repo.get("mqtt_ssl").ok().flatten()
        .map(|s| s == "true");
    let ssl = if protocol.starts_with("mqtt://") || protocol.starts_with("ws://") {
        false
    } else if protocol.starts_with("mqtts://") || protocol.starts_with("wss://") {
        true
    } else {
        legacy_ssl.unwrap_or(port == 8883 || port == 443)
    };

    let ws_path = db_state.settings_repo.get("mqtt_ws_path").ok().flatten()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/mqtt".to_string());

    let tls_insecure = db_state.settings_repo.get("mqtt_tls_insecure").ok().flatten()
        .map(|s| s == "true")
        .unwrap_or(false);

    Some(MqttConfig {
        host,
        port,
        username,
        password,
        topic,
        client_id,
        protocol,
        ssl,
        ws_path,
        tls_insecure,
    })
}

pub fn start_mqtt_client(app: AppHandle) {
    if MQTT_TASK_ACTIVE.swap(true, Ordering::Relaxed) {
        return;
    }

    tauri::async_runtime::spawn(async move {
        let _guard = MqttTaskGuard;
        const MAX_RECONNECT_ATTEMPTS: u32 = 3;
        
        loop {
            let config = get_mqtt_config(&app);
            
            if let Some(cfg) = config {
                if !MQTT_RUNNING.load(Ordering::Relaxed) {
                    info!(">>> [MQTT] Enabling MQTT client");
                    MQTT_RUNNING.store(true, Ordering::Relaxed);
                    let _ = app.emit("mqtt-status", "connecting");
                }

                // Extract host from config (strip scheme, path, and optional port)
                let host_clean = cfg.host.trim_start_matches("wss://")
                    .trim_start_matches("ws://")
                    .trim_start_matches("mqtt://")
                    .trim_start_matches("mqtts://")
                    .split('/')
                    .next()
                    .unwrap_or(&cfg.host)
                    .to_string();
                let host_clean = if host_clean.starts_with('[') {
                    if let Some(idx) = host_clean.rfind("]:") {
                        let (h, p) = host_clean.split_at(idx + 1);
                        if p.trim_start_matches(':').chars().all(|c| c.is_ascii_digit()) {
                            h.to_string()
                        } else {
                            host_clean
                        }
                    } else {
                        host_clean
                    }
                } else if let Some((h, p)) = host_clean.rsplit_once(':') {
                    if p.chars().all(|c| c.is_ascii_digit()) { h.to_string() } else { host_clean }
                } else {
                    host_clean
                };
                
                let use_wss = cfg.protocol.starts_with("wss://");
                let use_tls = cfg.protocol.starts_with("mqtts://") || (cfg.ssl && !use_wss);
                let use_ws = cfg.protocol.starts_with("ws://");
                let ws_path = if cfg.ws_path.starts_with('/') {
                    cfg.ws_path.clone()
                } else {
                    format!("/{}", cfg.ws_path)
                };
                
                info!(">>> [MQTT] Connecting to '{}:{}' (Protocol: {}, ID: {})", 
                    host_clean, cfg.port, cfg.protocol, cfg.client_id);

                // Build mqtt options. For websockets we must store a full URL in broker_addr.
                let mut mqttoptions = if use_wss {
                    let url = format!("wss://{}:{}{}", host_clean, cfg.port, ws_path);
                    MqttOptions::new(cfg.client_id.clone(), url, cfg.port)
                } else if use_ws {
                    let url = format!("ws://{}:{}{}", host_clean, cfg.port, ws_path);
                    MqttOptions::new(cfg.client_id.clone(), url, cfg.port)
                } else {
                    MqttOptions::new(cfg.client_id.clone(), host_clean.clone(), cfg.port)
                };

                // Apply transport settings explicitly for security
                if use_wss || use_tls {
                    // Manually build TlsConfiguration to avoid panic on invalid system certs
                    // We only use webpki-roots which are safe and don't contain enterprise/AV certs that cause panic
                    let mut root_store = rustls::RootCertStore::empty();
                    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
                    
                    let mut config = rustls::ClientConfig::builder()
                        .with_root_certificates(root_store)
                        .with_no_client_auth();
                        
                    // If insecure flag is set, disable verification (dangerous but requested)
                    if cfg.tls_insecure {
                        #[derive(Debug)]
                        struct NoVerifier;
                        impl rustls::client::danger::ServerCertVerifier for NoVerifier {
                            fn verify_server_cert(
                                &self,
                                _end_entity: &rustls::pki_types::CertificateDer<'_>,
                                _intermediates: &[rustls::pki_types::CertificateDer<'_>],
                                _server_name: &rustls::pki_types::ServerName<'_>,
                                _ocsp_response: &[u8],
                                _now: rustls::pki_types::UnixTime,
                            ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
                                Ok(rustls::client::danger::ServerCertVerified::assertion())
                            }

                            fn verify_tls12_signature(
                                &self,
                                _message: &[u8],
                                _cert: &rustls::pki_types::CertificateDer<'_>,
                                _dss: &rustls::DigitallySignedStruct,
                            ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
                                Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
                            }

                            fn verify_tls13_signature(
                                &self,
                                _message: &[u8],
                                _cert: &rustls::pki_types::CertificateDer<'_>,
                                _dss: &rustls::DigitallySignedStruct,
                            ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
                                Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
                            }
                            
                            fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
                                vec![
                                    rustls::SignatureScheme::RSA_PKCS1_SHA1,
                                    rustls::SignatureScheme::ECDSA_SHA1_Legacy,
                                    rustls::SignatureScheme::RSA_PKCS1_SHA256,
                                    rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
                                    rustls::SignatureScheme::RSA_PKCS1_SHA384,
                                    rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
                                    rustls::SignatureScheme::RSA_PKCS1_SHA512,
                                    rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
                                    rustls::SignatureScheme::RSA_PSS_SHA256,
                                    rustls::SignatureScheme::RSA_PSS_SHA384,
                                    rustls::SignatureScheme::RSA_PSS_SHA512,
                                    rustls::SignatureScheme::ED25519,
                                    rustls::SignatureScheme::ED448,
                                ]
                            }
                        }
                        config.dangerous().set_certificate_verifier(std::sync::Arc::new(NoVerifier));
                    }

                    if use_wss {
                        // Use Rustls transport with Arc<ClientConfig>
                        mqttoptions.set_transport(Transport::wss_with_config(config.into()));
                    } else {
                        // Use Rustls transport with Arc<ClientConfig>
                        mqttoptions.set_transport(Transport::tls_with_config(config.into()));
                    }
                } else if use_ws {
                    mqttoptions.set_transport(Transport::ws());
                }

                mqttoptions.set_keep_alive(Duration::from_secs(30));

                if let Some(u) = cfg.username.clone() {
                    let p = cfg.password.clone().unwrap_or_default();
                    mqttoptions.set_credentials(u, p);
                }

                // Initialize client
                let (client, mut eventloop) = AsyncClient::new(mqttoptions, 10);
                
                let mut connected = false;
                let connect_result = tokio::time::timeout(Duration::from_secs(30), async {
                    loop {
                        match eventloop.poll().await {
                            Ok(Event::Incoming(Incoming::ConnAck(_))) => {
                                info!(">>> [MQTT] Connection Established.");
                                MQTT_CONNECTED.store(true, Ordering::Relaxed);
                                MQTT_RECONNECT_ATTEMPTS.store(0, Ordering::Relaxed);
                                let _ = app.emit("mqtt-status", "connected");
                                return Ok::<(), rumqttc::ConnectionError>(());
                            }
                            Ok(_) => continue,
                            Err(e) => return Err(e),
                        }
                    }
                }).await;

                match connect_result {
                    Ok(Ok(())) => connected = true,
                    Ok(Err(e)) => {
                        error!(">>> [MQTT] Connection error: {}", e);
                    }
                    Err(_) => {
                        error!(">>> [MQTT] Connection timeout.");
                    }
                }
                
                if !connected {
                    let current_attempts = MQTT_RECONNECT_ATTEMPTS.fetch_add(1, Ordering::Relaxed) + 1;
                    MQTT_RUNNING.store(false, Ordering::Relaxed);
                    MQTT_CONNECTED.store(false, Ordering::Relaxed);
                    let _ = app.emit("mqtt-status", "disconnected");
                    
                    if current_attempts >= MAX_RECONNECT_ATTEMPTS {
                        error!(">>> [MQTT] Max attempts reached ({}/{}). Task pausing.", current_attempts, MAX_RECONNECT_ATTEMPTS);
                        break; 
                    }
                    
                    let wait_secs = 5 * u64::pow(2, (current_attempts as u32).saturating_sub(1));
                    info!(">>> [MQTT] Retrying in {}s...", wait_secs);
                    sleep(Duration::from_secs(wait_secs)).await;
                    continue;
                }
                
                let sub_topic = format!("{}/#", cfg.topic);
                if let Err(e) = client.subscribe(sub_topic.clone(), QoS::AtLeastOnce).await {
                    error!(">>> [MQTT] Subscribe failed: {:?}", e);
                    sleep(Duration::from_secs(2)).await;
                    continue;
                }
                info!(">>> [MQTT] Subscribed to {}", sub_topic);

                loop {
                    match tokio::time::timeout(Duration::from_secs(5), eventloop.poll()).await {
                        Ok(event_result) => match event_result {
                            Ok(Event::Incoming(notification)) => {
                                match notification {
                                    Incoming::Publish(publish) => {
                                        if let Ok(payload_str) = std::str::from_utf8(&publish.payload) {
                                            let payload_trimmed = payload_str.trim();
                                            let final_content = if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(payload_trimmed) {
                                                let content_fields = ["msg", "content", "body", "text", "message"];
                                                let mut found_content = None;
                                                for field in content_fields {
                                                    if let Some(val) = json_val.get(field) {
                                                        if let Some(s) = val.as_str() { found_content = Some(s.to_string()); break; }
                                                        else if val.is_number() || val.is_boolean() { found_content = Some(val.to_string()); break; }
                                                    }
                                                }
                                                found_content.unwrap_or_else(|| payload_trimmed.to_string())
                                            } else {
                                                payload_trimmed.to_string()
                                            };

                                            let payload_owned = final_content.clone();
                                            let app_handle_for_clipboard = app.clone();
                                            
                                            std::thread::spawn(move || {
                                                let normalized = payload_owned.trim().replace("\r\n", "\n");
                                                let mut hasher = DefaultHasher::new();
                                                normalized.hash(&mut hasher);
                                                let content_hash = hasher.finish();
                                                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
                                                LAST_APP_SET_HASH.store(content_hash, Ordering::SeqCst);
                                                LAST_APP_SET_TIMESTAMP.store(now, Ordering::SeqCst);
                                                match arboard::Clipboard::new() {
                                                    Ok(mut clipboard) => {
                                                        let mut attempts = 0;
                                                        while attempts < 3 {
                                                            if let Err(_) = clipboard.set_text(payload_owned.clone()) {
                                                                 std::thread::sleep(std::time::Duration::from_millis(100));
                                                                attempts += 1;
                                                            } else { break; }
                                                        }
                                                    },
                                                    Err(_) => {}
                                                }
                                                crate::services::clipboard::process_new_entry(&app_handle_for_clipboard, crate::services::clipboard::ClipboardData::Text(payload_owned), Some("mqtt".to_string()), None);
                                            });

                                            let _ = app.emit("mqtt-message", &final_content);
                                        }
                                    }
                                    Incoming::ConnAck(_) => {
                                        let _ = app.emit("mqtt-status", "connected");
                                    }
                                    _ => {}
                                }
                            }
                            Ok(Event::Outgoing(_)) => {}
                            Err(e) => {
                                error!(">>> [MQTT] Event loop error: {:?}", e);
                                MQTT_RUNNING.store(false, Ordering::Relaxed);
                                MQTT_CONNECTED.store(false, Ordering::Relaxed);
                                let _ = app.emit("mqtt-status", "disconnected");
                                break; 
                            }
                        },
                        Err(_) => {
                            // Timeout, check if still enabled
                            if get_mqtt_config(&app).is_none() {
                                info!(">>> [MQTT] Disabled. Stopping task.");
                                MQTT_RUNNING.store(false, Ordering::Relaxed);
                                MQTT_CONNECTED.store(false, Ordering::Relaxed);
                                let _ = app.emit("mqtt-status", "disconnected");
                                return; 
                            }
                        }
                    }
                }
            } else {
                if MQTT_RUNNING.load(Ordering::Relaxed) {
                     info!(">>> [MQTT] Configuration invalid or disabled.");
                     MQTT_RUNNING.store(false, Ordering::Relaxed);
                     let _ = app.emit("mqtt-status", "disconnected");
                }
            }
            
            let current_attempts = MQTT_RECONNECT_ATTEMPTS.load(Ordering::Relaxed);
            if current_attempts >= MAX_RECONNECT_ATTEMPTS {
                 break;
            }
            sleep(Duration::from_secs(5)).await;
        }
    });
}
