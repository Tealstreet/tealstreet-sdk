//! Journal-only correlation for the remote signing boundary.
use crate::state::Log;
use safe_cex_remote_signer::{Failure, Outbox, SignerLink};
use serde_json::{Value, json};
use std::sync::Arc;

pub(crate) fn link(socket: u64, outbox: Outbox) -> Arc<SignerLink> {
    let link = SignerLink::new(socket, outbox);
    link.observe_timeouts(Arc::new(|event| {
        Log::warn(
            &json!({
                "event": "sign.timeout",
                "request_id_hash": request_id_hash(&event.request_id),
                "socket_id": event.socket,
                "sent_socket_id": event.sent_socket,
                "request_age_ms": event.request_age_ms,
                "attachment_age_ms": event.attachment_age_ms,
                "last_heard_age_ms": event.last_heard_age_ms,
                "cause": match event.failure {
                    Failure::Suspended => "suspended",
                    Failure::Duplicate => "duplicate",
                    _ => "unanswered",
                },
            })
            .to_string(),
        );
    }));
    link
}

fn request_id_hash(id: &str) -> String {
    safe_cex_crypto::hex(&safe_cex_crypto::sha256(id.as_bytes()))
}

pub(crate) struct ReplyMisses {
    socket: u64,
    started: std::time::Instant,
    last_log: Option<tokio::time::Instant>,
    count: u64,
    coalesced: u64,
}

impl ReplyMisses {
    pub(crate) fn new(socket: u64) -> Self {
        Self {
            socket,
            started: std::time::Instant::now(),
            last_log: None,
            count: 0,
            coalesced: 0,
        }
    }

    #[cfg(test)]
    pub(crate) fn count(&self) -> u64 {
        self.count
    }

    pub(crate) fn unmatched(&mut self, response: &Value) {
        self.count = self.count.saturating_add(1);
        let now = tokio::time::Instant::now();
        if self
            .last_log
            .is_some_and(|at| now.duration_since(at) < std::time::Duration::from_secs(1))
        {
            self.coalesced = self.coalesced.saturating_add(1);
            return;
        }
        self.log(
            response
                .get("id")
                .and_then(Value::as_str)
                .map(request_id_hash),
            false,
        );
        self.last_log = Some(now);
        self.coalesced = 0;
    }

    fn log(&self, id: Option<String>, summary: bool) {
        Log::warn(
            &json!({
                "event": "sign.response_unmatched",
                "request_id_hash": id,
                "socket_id": self.socket,
                "socket_age_ms": self.started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
                "unmatched_count": self.count,
                "coalesced_count": self.coalesced,
                "summary": summary,
            })
            .to_string(),
        );
    }
}

impl Drop for ReplyMisses {
    fn drop(&mut self) {
        if self.coalesced > 0 {
            self.log(None, true);
        }
    }
}
