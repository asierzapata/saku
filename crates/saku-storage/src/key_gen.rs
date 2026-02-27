use sha2::{Digest, Sha256};

/// Generate a task storage key suffix from device_id and creation timestamp.
///
/// Produces a deterministic 8-character base-36 string derived from
/// SHA-256(device_id || creation_ms). Uses the first 5 bytes (40 bits)
/// of the hash, giving ~1 trillion possible values.
pub fn generate_task_key(device_id: &str, creation_ms: i64) -> String {
    let mut hasher = Sha256::new();
    hasher.update(device_id.as_bytes());
    hasher.update(creation_ms.to_le_bytes());
    let hash = hasher.finalize();

    // Take first 5 bytes → 40 bits of entropy
    let num = u64::from_le_bytes([hash[0], hash[1], hash[2], hash[3], hash[4], 0, 0, 0]);
    base36_encode(num)
}

/// Encode a u64 as a base-36 string (digits 0-9, letters a-z).
pub fn base36_encode(mut num: u64) -> String {
    if num == 0 {
        return "0".into();
    }

    const CHARS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut result = Vec::new();

    while num > 0 {
        let remainder = (num % 36) as usize;
        result.push(CHARS[remainder]);
        num /= 36;
    }

    result.reverse();
    String::from_utf8(result).expect("base36 chars are valid UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base36_encode_zero() {
        assert_eq!(base36_encode(0), "0");
    }

    #[test]
    fn base36_encode_small_values() {
        assert_eq!(base36_encode(1), "1");
        assert_eq!(base36_encode(10), "a");
        assert_eq!(base36_encode(35), "z");
        assert_eq!(base36_encode(36), "10");
        assert_eq!(base36_encode(100), "2s");
    }

    #[test]
    fn base36_encode_large_value() {
        // 36^4 = 1_679_616
        assert_eq!(base36_encode(1_679_616), "10000");
    }

    #[test]
    fn generate_task_key_is_deterministic() {
        let key1 = generate_task_key("device-abc", 1700000000000);
        let key2 = generate_task_key("device-abc", 1700000000000);
        assert_eq!(key1, key2);
    }

    #[test]
    fn generate_task_key_different_devices_produce_different_keys() {
        let key1 = generate_task_key("device-a", 1700000000000);
        let key2 = generate_task_key("device-b", 1700000000000);
        assert_ne!(key1, key2);
    }

    #[test]
    fn generate_task_key_different_timestamps_produce_different_keys() {
        let key1 = generate_task_key("device-a", 1700000000000);
        let key2 = generate_task_key("device-a", 1700000000001);
        assert_ne!(key1, key2);
    }

    #[test]
    fn generate_task_key_is_not_empty() {
        let key = generate_task_key("dev", 12345);
        assert!(!key.is_empty());
        // Should be alphanumeric only
        assert!(key.chars().all(|c| c.is_ascii_alphanumeric()));
    }
}
