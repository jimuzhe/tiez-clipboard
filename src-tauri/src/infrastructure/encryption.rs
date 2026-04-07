pub const ENCRYPT_PREFIX: &str = "plain:";

pub fn encrypt_value(plain: &str) -> Option<String> {
    // On macOS, we could use Keychain (security-framework crate).
    // For now, returning clear text to ensure functionality while refactoring.
    Some(format!("{}{}", ENCRYPT_PREFIX, plain))
}

pub fn decrypt_value(cipher: &str) -> Option<String> {
    if let Some(payload) = cipher.strip_prefix(ENCRYPT_PREFIX) {
        return Some(payload.to_string());
    }
    // Fallback if not prefixed
    Some(cipher.to_string())
}
