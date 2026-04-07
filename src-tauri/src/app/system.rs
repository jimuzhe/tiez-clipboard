pub const LEGACY_PLACEHOLDER_MACHINE_ID: &str = "MAC-DEVICE-000000";
pub const LEGACY_PLACEHOLDER_ANON_ID: &str = "MAC-DEVICE-000000-0000-0000-0000-000000000000";
pub const LEGACY_ZERO_SUFFIX: &str = "-0000-0000-0000-000000000000";

fn hash_to_short_id(seed: &str) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(seed.as_bytes());
    let hex = format!("{:x}", hasher.finalize());
    hex.chars().take(8).collect()
}

fn fallback_machine_id() -> String {
    let mut seed = String::new();

    for key in ["COMPUTERNAME", "HOSTNAME", "USER", "USERNAME"] {
        if let Ok(value) = std::env::var(key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                seed.push_str(trimmed);
                seed.push('|');
            }
        }
    }

    if seed.is_empty() {
        seed.push_str("tiez-device");
    }

    hash_to_short_id(&seed)
}

pub fn get_machine_id() -> String {
    match machine_uid::get() {
        Ok(machine_uid) if !machine_uid.trim().is_empty() => hash_to_short_id(machine_uid.trim()),
        Ok(_) => fallback_machine_id(),
        Err(e) => {
            eprintln!("[WARN] Failed to get machine UID: {}. Using fallback.", e);
            fallback_machine_id()
        }
    }
}

pub fn build_anon_id(machine_id: &str) -> String {
    machine_id.trim().to_string()
}

pub fn is_legacy_placeholder_anon_id(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed == LEGACY_PLACEHOLDER_MACHINE_ID || trimmed == LEGACY_PLACEHOLDER_ANON_ID
}

pub fn normalize_anon_id(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || is_legacy_placeholder_anon_id(trimmed) {
        return None;
    }

    if let Some(prefix) = trimmed.strip_suffix(LEGACY_ZERO_SUFFIX) {
        let normalized = prefix.trim();
        if normalized.is_empty() || is_legacy_placeholder_anon_id(normalized) {
            return None;
        }
        return Some(normalized.to_string());
    }

    Some(trimmed.to_string())
}

pub fn same_anon_id(left: &str, right: &str) -> bool {
    match (normalize_anon_id(left), normalize_anon_id(right)) {
        (Some(l), Some(r)) => l == r,
        _ => left.trim() == right.trim(),
    }
}
