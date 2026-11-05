//! Process-wide logging and counters. Logs carry account names and venues only:
//! never a URI, token, init payload or signature.
use serde_json::json;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// One random process incarnation, shared by health and router acknowledgements.
pub fn boot_id() -> &'static str {
    static BOOT_ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    BOOT_ID.get_or_init(new_boot_id)
}
fn new_boot_id() -> String {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).expect("OS entropy for server boot identity");
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let value = u128::from_be_bytes(bytes);
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        value >> 96,
        (value >> 80) & 0xffff,
        (value >> 64) & 0xffff,
        (value >> 48) & 0xffff,
        value & 0xffff_ffff_ffff
    )
}

pub struct Log;

impl Log {
    fn line(level: &str, message: &str) {
        let line =
            json!({"level": level, "service": "exchange-server", "msg": message}).to_string();
        if level == "info" {
            println!("{line}");
        } else {
            eprintln!("{line}");
        }
    }
    pub fn info(message: &str) {
        Self::line("info", message);
    }
    pub fn warn(message: &str) {
        Self::line("warn", message);
    }
}

#[derive(Default)]
pub struct Metrics {
    pub connections: AtomicUsize,
    inits: AtomicU64,
    disposes: AtomicU64,
}

impl Metrics {
    pub fn note_init(&self) {
        self.inits.fetch_add(1, Ordering::Relaxed);
    }
    pub fn note_dispose(&self) {
        self.disposes.fetch_add(1, Ordering::Relaxed);
    }
    pub fn snapshot(&self) -> (usize, u64, u64) {
        (
            self.connections.load(Ordering::Relaxed),
            self.inits.load(Ordering::Relaxed),
            self.disposes.load(Ordering::Relaxed),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn boot_identity_is_one_process_uuid_v4_and_new_process_ids_are_random() {
        let id = boot_id();
        assert_eq!(id, boot_id());
        assert_eq!(id.len(), 36);
        assert_eq!(&id[14..15], "4");
        assert!(matches!(&id[19..20], "8" | "9" | "a" | "b"));
        for (i, b) in id.bytes().enumerate() {
            if [8, 13, 18, 23].contains(&i) {
                assert_eq!(b, b'-');
            } else {
                assert!(b.is_ascii_hexdigit());
            }
        }
        assert_ne!(id, new_boot_id());
    }
}
