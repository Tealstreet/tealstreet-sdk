//! Account GET evidence and scheduling; no trading request enters this state.
use crate::{Pending, account::Axis, error};
use safe_cex_engine::log::code;
use safe_cex_engine::startup::Backoff;
use safe_cex_engine::{AuthOutcome, Cx, Level, MergedScopeAuth, Response, TimerId, VenueError};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
};

pub(crate) const LIST: safe_cex_engine::json::ArrayAt =
    safe_cex_engine::json::ArrayAt::AnyFieldEnvelope(&[
        "data",
        "tradeList",
        "userTrades",
        "positionList",
        "positions",
        "orderList",
        "list",
        "rows",
    ]);
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) enum Key {
    Axis(Axis),
    Settings(String),
}
pub(crate) struct State {
    retry: Backoff<Key>,
    pause_until: f64,
    deferred: HashMap<Key, f64>,
    deferred_timer: Option<TimerId>,
    settings_serial: HashMap<String, u64>,
    settings_sequence: u64,
    pub(crate) orders_auth: MergedScopeAuth,
    settings_rejected: RefCell<HashSet<String>>,
    settings_overflow: std::cell::Cell<bool>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            retry: Backoff::default(),
            pause_until: f64::NEG_INFINITY,
            deferred: HashMap::new(),
            deferred_timer: None,
            settings_serial: HashMap::new(),
            settings_sequence: 0,
            orders_auth: MergedScopeAuth::default(),
            settings_rejected: RefCell::new(HashSet::new()),
            settings_overflow: std::cell::Cell::new(false),
        }
    }
}
impl State {
    fn arm(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(timer) = self.deferred_timer.take() {
            cx.clear_timer(timer);
        }
        if let Some(due) = self
            .deferred
            .values()
            .map(|due| due.max(self.pause_until))
            .min_by(f64::total_cmp)
        {
            self.deferred_timer = Some(cx.set_timer((due - cx.now).max(0.0)));
        }
    }
    pub(crate) fn defer(&mut self, cx: &mut Cx<'_, Pending>, key: Key, due: f64) {
        let due = due.max(self.pause_until);
        self.deferred
            .entry(key)
            .and_modify(|old| *old = old.min(due))
            .or_insert(due);
        self.arm(cx);
    }
    pub(crate) fn blocked(&mut self, cx: &mut Cx<'_, Pending>, key: &Key) -> bool {
        if self.retry.waiting(key) {
            return true;
        }
        let due = self
            .deferred
            .get(key)
            .copied()
            .unwrap_or(f64::NEG_INFINITY)
            .max(self.pause_until);
        if cx.now < due {
            self.defer(cx, key.clone(), due);
            return true;
        }
        if self.deferred.remove(key).is_some() {
            self.arm(cx);
        }
        false
    }
    pub(crate) fn failed(&mut self, cx: &mut Cx<'_, Pending>, key: Key, pause: f64) {
        if pause > 0.0 {
            self.pause_until = self.pause_until.max(cx.now + pause);
            self.arm(cx);
        }
        self.retry
            .failed_after(cx, key, (self.pause_until - cx.now).max(0.0));
    }
    pub(crate) fn succeeded(&mut self, cx: &mut Cx<'_, Pending>, key: &Key) {
        self.retry.succeeded(cx, key);
        if self.deferred.remove(key).is_some() {
            self.arm(cx);
        }
    }
    pub(crate) fn timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> Option<Vec<Key>> {
        if self.deferred_timer == Some(timer) {
            self.deferred_timer = None;
            let ready: Vec<_> = self
                .deferred
                .iter()
                .filter(|(_, due)| cx.now >= due.max(self.pause_until))
                .map(|(key, _)| key.clone())
                .collect();
            for key in &ready {
                self.deferred.remove(key);
            }
            self.arm(cx);
            return Some(ready);
        }
        self.retry.on_timer(timer).map(|key| vec![key])
    }
    pub(crate) fn next_settings(&mut self, symbol: &str) -> u64 {
        self.settings_sequence += 1;
        self.settings_serial
            .insert(symbol.into(), self.settings_sequence);
        self.settings_sequence
    }
    pub(crate) fn retain_settings(&mut self, cx: &mut Cx<'_, Pending>) {
        let removed: Vec<_> = self
            .settings_serial
            .keys()
            .filter(|symbol| cx.store.market(symbol).is_none())
            .cloned()
            .collect();
        for symbol in removed {
            self.settings_serial.remove(&symbol);
            let key = Key::Settings(symbol);
            self.retry.succeeded(cx, &key);
            self.deferred.remove(&key);
        }
        self.arm(cx);
    }
    /// The last settings serial issued, for any symbol.
    pub(crate) fn settings_sequence(&self) -> u64 {
        self.settings_sequence
    }
    pub(crate) fn settings_current(&self, symbol: &str, serial: u64) -> bool {
        self.settings_serial.get(symbol) == Some(&serial)
    }
    pub(crate) fn settings_auth(
        &self,
        symbol: &str,
        outcome: Option<AuthOutcome>,
    ) -> Option<AuthOutcome> {
        let mut refused = self.settings_rejected.borrow_mut();
        match outcome? {
            AuthOutcome::Rejected => {
                if refused.len() < 4096 || refused.contains(symbol) {
                    refused.insert(symbol.into());
                } else {
                    self.settings_overflow.set(true);
                }
                Some(AuthOutcome::Rejected)
            }
            AuthOutcome::Accepted => {
                refused.remove(symbol);
                (refused.is_empty() && !self.settings_overflow.get())
                    .then_some(AuthOutcome::Accepted)
            }
        }
    }
}
#[derive(serde::Deserialize)]
struct Evidence {
    code: error::Code,
}
pub(crate) fn native(response: &Response<'_>) -> Option<i64> {
    serde_json::from_str::<Evidence>(response.body)
        .ok()
        .map(|e| e.code.0)
}
pub(crate) fn refused(response: &Response<'_>) -> bool {
    response.status == 200 && matches!(native(response), Some(403 | 10003 | 10004))
}
pub(crate) fn success(response: &Response<'_>) -> bool {
    (200..300).contains(&response.status) && native(response) == Some(0)
}
pub(crate) fn pause(response: &Response<'_>) -> f64 {
    if !matches!(response.status, 418 | 429) && !matches!(native(response), Some(10005 | 10006)) {
        return 0.0;
    }
    response
        .header("retry-after")
        .and_then(|s| s.trim().parse::<f64>().ok())
        .filter(|n| !n.is_nan() && *n > 0.0)
        .map_or(60_000.0, |n| {
            (n * 1000.0).clamp(60_000.0, safe_cex_engine::startup::MAX_RATE_LIMIT_PAUSE_MS)
        })
}
pub(crate) fn failed(response: &Response<'_>) -> VenueError {
    let native = native(response);
    // The shared table decides, as for writes: HTTP status classifies a
    // non-2xx answer, and a native code (10007 included) classifies a 2xx.
    let kind = crate::codes::read_failure(response, "")
        .error
        .kind
        .unwrap_or(safe_cex_engine::ErrorKind::Internal);
    let message = if let Some(native) = native.filter(|code| response.status == 200 && *code != 0) {
        format!("Bitunix account read failed: code {native}")
    } else if !(200..300).contains(&response.status) {
        format!("Bitunix account read HTTP {}", response.status)
    } else {
        "Malformed Bitunix account read response".into()
    };
    let mut error = VenueError::new(message).kind(kind);
    if let Some(native) = native {
        error = error.venue_code(native.to_string());
    }
    let pause = pause(response);
    if pause > 0.0 {
        error = error.retry_after_ms(pause);
    }
    error
}
pub(crate) fn notice(cx: &mut Cx<'_, Pending>, response: Option<&Response<'_>>) {
    let error = response.map_or_else(
        || VenueError::new("Bitunix account read transport/signing failure"),
        failed,
    );
    let level = if response.is_some_and(refused) {
        Level::Error
    } else {
        Level::Warn
    };
    // AccountAuth is the sole signer/scope evidence. The legacy auth.rejected
    // diagnostic is interpreted by SDK consumers as primary/account evidence.
    let name = code::REQUEST_FAILED;
    let mut fields = vec![];
    if let Some(kind) = error.kind {
        fields.push(("kind", kind.as_str().into()));
    }
    if let Some(native) = error.venue_code {
        fields.push(("venueCode", native.into()));
    }
    if let Some(retry) = error.retry_after_ms {
        fields.push(("retryAfterMs", retry.into()));
    }
    cx.event(level, name, error.message, fields);
}
