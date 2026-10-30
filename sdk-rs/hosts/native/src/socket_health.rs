//! Per-channel socket state, as the WASM host's `socketHealth()` keeps it, so a
//! native server can report `store.socketHealth` like the browser engine does.
use safe_cex_engine::{SocketChannel, SocketId};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Rust venues reconnect after up to a minute of silence (`RUST_SOCKET_STALE_MS`).
pub const SOCKET_STALE_MS: u64 = 60_000;

/// One channel's counters (`EngineSocketStats`); times are wall-clock milliseconds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SocketStats {
    pub open: u32,
    pub initialized: u32,
    pub last_received: u64,
    pub last_close_code: Option<u16>,
    pub last_close_at: u64,
    pub closes: u32,
    pub close_streak: u32,
}

impl SocketStats {
    fn initialized(&self) -> bool {
        self.initialized > 0
    }

    /// As `RustSocketHandle.isHealthy`.
    fn healthy(&self, now: u64) -> bool {
        self.open > 0
            && self.last_received > 0
            && now.saturating_sub(self.last_received) < SOCKET_STALE_MS
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SocketHealth {
    pub public: SocketStats,
    pub business: SocketStats,
    pub private: SocketStats,
}

/// `store.socketHealth` as `summarizeSocketHealth` derives it: public and business
/// sockets are public, the private channel is private.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SocketSummary {
    pub public_healthy: bool,
    pub private_healthy: bool,
    pub public_initialized: bool,
    pub private_initialized: bool,
    pub public_last_message: Option<u64>,
    pub private_last_message: Option<u64>,
    pub public: SocketDiagnostics,
    pub private: SocketDiagnostics,
}

/// `summarizeSocketDiagnostics`: the most recent close and the summed counters.
/// Engine venues own their backoff, so a reconnect is never paused.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SocketDiagnostics {
    pub close_code: Option<u16>,
    pub close_count_total: u32,
    pub close_streak: u32,
}

impl SocketDiagnostics {
    fn of(channels: &[&SocketStats]) -> Self {
        let latest = channels
            .iter()
            .filter(|stats| stats.last_close_code.is_some())
            .max_by_key(|stats| stats.last_close_at);
        Self {
            close_code: latest.and_then(|stats| stats.last_close_code),
            close_count_total: channels.iter().map(|stats| stats.closes).sum(),
            close_streak: channels.iter().map(|stats| stats.close_streak).sum(),
        }
    }
}

impl SocketHealth {
    pub fn summary(&self, now: u64) -> SocketSummary {
        let public: Vec<&SocketStats> = [&self.public, &self.business]
            .into_iter()
            .filter(|stats| stats.initialized())
            .collect();
        let private = self.private.initialized().then_some(&self.private);
        let last = |stats: &[&SocketStats]| {
            stats
                .iter()
                .map(|stats| stats.last_received)
                .max()
                .filter(|last| *last > 0)
        };
        SocketSummary {
            public_healthy: public.is_empty() || public.iter().any(|stats| stats.healthy(now)),
            private_healthy: private.is_none_or(|stats| stats.healthy(now)),
            public_initialized: !public.is_empty(),
            private_initialized: private.is_some(),
            public_last_message: last(&public),
            private_last_message: last(private.as_slice()),
            public: SocketDiagnostics::of(&[&self.public, &self.business]),
            private: SocketDiagnostics::of(&[&self.private]),
        }
    }
}

struct Record {
    channel: SocketChannel,
    open: bool,
    initialized: bool,
}

/// Host-side tracker; the shared cell is what account handles read.
#[derive(Default)]
pub(crate) struct SocketTracker {
    records: HashMap<SocketId, Record>,
    shared: Arc<Mutex<SocketHealth>>,
}

fn wall_ms() -> u64 {
    crate::now_ms() as u64
}

impl SocketTracker {
    pub(crate) fn cell(&self) -> Arc<Mutex<SocketHealth>> {
        self.shared.clone()
    }

    fn with(&self, channel: &SocketChannel, update: impl FnOnce(&mut SocketStats)) {
        let mut health = self
            .shared
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        update(match channel {
            SocketChannel::Public => &mut health.public,
            SocketChannel::Business => &mut health.business,
            SocketChannel::Private => &mut health.private,
        });
    }

    pub(crate) fn connecting(&mut self, socket: SocketId, channel: Option<SocketChannel>) {
        self.closed(socket, None);
        self.records.insert(
            socket,
            Record {
                channel: channel.unwrap_or(SocketChannel::Public),
                open: false,
                initialized: false,
            },
        );
    }

    pub(crate) fn opened(&mut self, socket: SocketId) {
        let Some(record) = self.records.get_mut(&socket).filter(|record| !record.open) else {
            return;
        };
        record.open = true;
        let channel = record.channel.clone();
        self.with(&channel, |stats| stats.open += 1);
    }

    /// One write per frame; the first frame after open marks the socket initialized.
    pub(crate) fn received(&mut self, socket: SocketId) {
        let Some(record) = self.records.get_mut(&socket) else {
            return;
        };
        let first = record.open && !record.initialized;
        record.initialized |= first;
        let channel = record.channel.clone();
        let now = wall_ms();
        self.with(&channel, |stats| {
            stats.last_received = now;
            if first {
                stats.initialized += 1;
                stats.close_streak = 0;
            }
        });
    }

    /// `code` is `None` for a socket the host closed without a venue close code.
    pub(crate) fn closed(&mut self, socket: SocketId, code: Option<u16>) {
        let Some(record) = self.records.remove(&socket) else {
            return;
        };
        let now = wall_ms();
        self.with(&record.channel, |stats| {
            stats.open -= u32::from(record.open);
            stats.initialized -= u32::from(record.initialized);
            if let Some(code) = code {
                stats.last_close_code = Some(code);
                stats.last_close_at = now;
                stats.closes += 1;
                stats.close_streak += 1;
            }
        });
    }

    pub(crate) fn clear(&mut self) {
        let sockets: Vec<SocketId> = self.records.keys().copied().collect();
        for socket in sockets {
            self.closed(socket, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_private_socket_initializes_on_its_first_frame_after_open() {
        let mut tracker = SocketTracker::default();
        let cell = tracker.cell();
        let summary = || cell.lock().unwrap().summary(wall_ms());
        assert!(!summary().private_initialized && summary().private_healthy);
        tracker.connecting(SocketId(1), Some(SocketChannel::Private));
        tracker.received(SocketId(1));
        assert!(
            !summary().private_initialized,
            "a frame before open does not count"
        );
        tracker.opened(SocketId(1));
        tracker.received(SocketId(1));
        let after = summary();
        assert!(after.private_initialized && after.private_healthy);
        assert!(after.private_last_message.is_some());
        assert!(!after.public_initialized && after.public_healthy);
        tracker.closed(SocketId(1), Some(1006));
        let closed = cell.lock().unwrap().private;
        assert_eq!((closed.open, closed.initialized, closed.closes), (0, 0, 1));
        assert_eq!(closed.last_close_code, Some(1006));
        assert!(!summary().private_initialized);
    }

    #[test]
    fn a_silent_initialized_socket_turns_unhealthy() {
        let health = SocketHealth {
            private: SocketStats {
                open: 1,
                initialized: 1,
                last_received: 1_000,
                ..SocketStats::default()
            },
            business: SocketStats {
                open: 1,
                initialized: 1,
                last_received: 1_000,
                ..SocketStats::default()
            },
            ..SocketHealth::default()
        };
        let fresh = health.summary(1_000 + SOCKET_STALE_MS - 1);
        assert!(fresh.private_healthy && fresh.public_healthy && fresh.public_initialized);
        let stale = health.summary(1_000 + SOCKET_STALE_MS);
        assert!(stale.private_initialized && !stale.private_healthy && !stale.public_healthy);
    }

    #[test]
    fn diagnostics_take_the_latest_close_and_sum_the_counters() {
        let closed = |code, at, closes, streak| SocketStats {
            last_close_code: Some(code),
            last_close_at: at,
            closes,
            close_streak: streak,
            ..SocketStats::default()
        };
        let health = SocketHealth {
            public: closed(1006, 10, 2, 1),
            business: closed(1000, 20, 3, 0),
            private: SocketStats::default(),
        };
        let summary = health.summary(30);
        assert_eq!(
            summary.public,
            SocketDiagnostics {
                close_code: Some(1000),
                close_count_total: 5,
                close_streak: 1,
            }
        );
        assert_eq!(summary.private, SocketDiagnostics::default());
    }

    #[test]
    fn host_closes_and_clears_drop_live_counts_without_a_close_code() {
        let mut tracker = SocketTracker::default();
        let cell = tracker.cell();
        tracker.connecting(SocketId(1), None);
        tracker.opened(SocketId(1));
        tracker.received(SocketId(1));
        tracker.clear();
        let public = cell.lock().unwrap().public;
        assert_eq!((public.open, public.initialized, public.closes), (0, 0, 0));
    }
}
