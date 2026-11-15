//! Observe local interfaces and scheduling gaps without an external probe service.
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime};
use tauri::AppHandle;
use tokio::sync::Notify;

const POLL_INTERVAL: Duration = Duration::from_secs(5);
const RESUME_GAP: Duration = Duration::from_secs(20);
const RECOVERY_COOLDOWN: Duration = Duration::from_secs(10);
static TASK_ACTIVE: AtomicBool = AtomicBool::new(false);
static WAKE_REQUESTED: AtomicBool = AtomicBool::new(false);

fn wake_signal() -> &'static Notify {
    static SIGNAL: OnceLock<Notify> = OnceLock::new();
    SIGNAL.get_or_init(Notify::new)
}

pub fn request_wake_recovery() {
    WAKE_REQUESTED.store(true, Ordering::Relaxed);
    wake_signal().notify_one();
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NetworkSnapshot {
    interfaces: Vec<(String, IpAddr)>,
    default_address: Option<IpAddr>,
}

fn network_snapshot() -> Option<NetworkSnapshot> {
    let mut interfaces = crate::infrastructure::net_interfaces::list_afinet_netifas()?;
    interfaces.retain(|(_, ip)| !ip.is_loopback() && !ip.is_unspecified());
    interfaces.sort();
    interfaces.dedup();
    Some(NetworkSnapshot {
        interfaces,
        default_address: crate::infrastructure::net_interfaces::local_ip(),
    })
}

#[derive(Default)]
struct RecoveryDetector {
    network: Option<NetworkSnapshot>,
    last_poll: Option<SystemTime>,
    last_recovery: Option<Instant>,
    pending: bool,
}

impl RecoveryDetector {
    fn observe(
        &mut self,
        network: Option<NetworkSnapshot>,
        now: Instant,
        wall: SystemTime,
        wake: bool,
    ) -> bool {
        if let Some(network) = network {
            // Only the route out matters. Extra interfaces coming and going
            // (IPv6 temporary addresses, VPN adapters, virtual bridges) used
            // to restart connections that were perfectly healthy.
            if self
                .network
                .as_ref()
                .is_some_and(|old| old.default_address != network.default_address)
            {
                self.pending = true;
            }
            self.network = Some(network);
        }
        if self
            .last_poll
            .is_some_and(|last| wall.duration_since(last).is_ok_and(|gap| gap > RESUME_GAP))
        {
            // Also catches suspend/resume on Windows, where Instant may pause during sleep.
            self.pending = true;
        }
        self.last_poll = Some(wall);
        self.pending |= wake;
        if self.pending
            && self
                .last_recovery
                .is_none_or(|last| now.duration_since(last) >= RECOVERY_COOLDOWN)
        {
            self.pending = false;
            self.last_recovery = Some(now);
            return true;
        }
        false
    }
}

pub fn start_network_recovery(app: AppHandle) {
    if TASK_ACTIVE.swap(true, Ordering::Relaxed) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let mut detector = RecoveryDetector::default();
        loop {
            let wake = WAKE_REQUESTED.swap(false, Ordering::Relaxed);
            if detector.observe(network_snapshot(), Instant::now(), SystemTime::now(), wake) {
                crate::info!(">>> [SYNC] Recovering connections after network change or wake");
                crate::services::mqtt_sub::recover_mqtt_connection(app.clone());
                // Each WebDAV cycle creates a fresh HTTP client. Queue a pull instead of
                // cancelling a possibly in-flight upload or starting a second sync task.
                crate::services::cloud_sync::request_cloud_sync(app.clone());
            }
            tokio::select! {
                _ = wake_signal().notified() => {},
                _ = tokio::time::sleep(POLL_INTERVAL) => {},
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn network(ip: &str) -> Option<NetworkSnapshot> {
        let ip: IpAddr = ip.parse().unwrap();
        Some(NetworkSnapshot {
            interfaces: vec![("wifi".into(), ip)],
            default_address: Some(ip),
        })
    }

    #[test]
    fn stable_network_and_failed_probes_do_not_restart_connections() {
        let mut detector = RecoveryDetector::default();
        let now = Instant::now();
        let wall = SystemTime::now();
        assert!(!detector.observe(network("192.168.1.2"), now, wall, false));
        assert!(!detector.observe(
            network("192.168.1.2"),
            now + POLL_INTERVAL,
            wall + POLL_INTERVAL,
            false
        ));
        assert!(!detector.observe(
            None,
            now + POLL_INTERVAL * 2,
            wall + POLL_INTERVAL * 2,
            false
        ));
    }

    #[test]
    fn network_changes_are_coalesced_without_losing_a_pending_recovery() {
        let mut detector = RecoveryDetector::default();
        let now = Instant::now();
        let wall = SystemTime::now();
        detector.observe(network("192.168.1.2"), now, wall, false);
        assert!(detector.observe(
            network("192.168.2.2"),
            now + POLL_INTERVAL,
            wall + POLL_INTERVAL,
            false
        ));
        assert!(!detector.observe(
            network("192.168.3.2"),
            now + POLL_INTERVAL * 2,
            wall + POLL_INTERVAL * 2,
            true
        ));
        assert!(detector.observe(
            network("192.168.3.2"),
            now + POLL_INTERVAL * 3,
            wall + POLL_INTERVAL * 3,
            false
        ));
        assert!(!detector.observe(
            network("192.168.3.2"),
            now + POLL_INTERVAL * 4,
            wall + POLL_INTERVAL * 4,
            false
        ));
    }

    #[test]
    fn wake_and_sleep_gaps_recover_even_with_unchanged_addresses() {
        let mut detector = RecoveryDetector::default();
        let now = Instant::now();
        let wall = SystemTime::now();
        detector.observe(network("192.168.1.2"), now, wall, false);
        assert!(detector.observe(
            network("192.168.1.2"),
            now + POLL_INTERVAL,
            wall + Duration::from_secs(300),
            false
        ));
        assert!(detector.observe(
            network("192.168.1.2"),
            now + Duration::from_secs(30),
            wall + Duration::from_secs(305),
            true
        ));
    }

    #[test]
    fn side_interfaces_changing_do_not_restart_connections() {
        let mut detector = RecoveryDetector::default();
        let now = Instant::now();
        let wall = SystemTime::now();
        detector.observe(network("192.168.1.2"), now, wall, false);
        let ip: IpAddr = "192.168.1.2".parse().unwrap();
        assert!(!detector.observe(
            Some(NetworkSnapshot {
                interfaces: vec![("wifi".into(), ip), ("utun3".into(), "10.8.0.2".parse().unwrap())],
                default_address: Some(ip),
            }),
            now + POLL_INTERVAL,
            wall + POLL_INTERVAL,
            false
        ));
    }

    #[test]
    fn going_offline_then_back_online_is_detected() {
        let mut detector = RecoveryDetector::default();
        let now = Instant::now();
        let wall = SystemTime::now();
        detector.observe(network("192.168.1.2"), now, wall, false);
        assert!(detector.observe(
            Some(NetworkSnapshot {
                interfaces: vec![],
                default_address: None
            }),
            now + POLL_INTERVAL,
            wall + POLL_INTERVAL,
            false
        ));
        assert!(detector.observe(
            network("192.168.1.2"),
            now + Duration::from_secs(15),
            wall + Duration::from_secs(15),
            false
        ));
    }
}
