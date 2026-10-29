//! Process-wide logging and counters. Logs carry account names and venues only:
//! never a URI, token, init payload or signature.
use serde_json::json;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

pub struct Log;

impl Log {
    fn line(level: &str, message: &str) {
        let line = json!({"level": level, "service": "exchange-server", "msg": message}).to_string();
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
