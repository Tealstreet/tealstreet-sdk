//! Structured engine events and health counters (docs/plans/RUST_QUALITY.md,
//! "Logging and observability"). Hosts add venue and account context.

/// Ordered so a host-set minimum compares directly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "debug" => Some(Self::Debug),
            "info" => Some(Self::Info),
            "warn" => Some(Self::Warn),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Str(String),
    Num(f64),
    Bool(bool),
    /// A field whose value must never leave the engine.
    Redacted,
}

/// Wraps a sensitive value so only its presence reaches a notice.
pub struct Redacted<T>(pub T);

impl<T> std::fmt::Debug for Redacted<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(REDACTED)
    }
}

impl<T> From<Redacted<T>> for Value {
    fn from(_: Redacted<T>) -> Self {
        Self::Redacted
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Self::Str(value)
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Self::Str(value.to_owned())
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Self {
        Self::Num(value)
    }
}

impl From<u32> for Value {
    fn from(value: u32) -> Self {
        Self::Num(value.into())
    }
}

impl From<u16> for Value {
    fn from(value: u16) -> Self {
        Self::Num(value.into())
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

/// How a [`Value::Redacted`] field is written on every wire.
pub const REDACTED: &str = "[redacted]";

/// Messages and string fields are cut to this many bytes, so a raw body can never
/// flood a notice.
pub const MAX_LOG_BYTES: usize = 512;
const ELLIPSIS: &str = "…";

pub(crate) fn cap(mut text: String) -> String {
    if text.len() > MAX_LOG_BYTES {
        let mut end = MAX_LOG_BYTES - ELLIPSIS.len();
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push_str(ELLIPSIS);
    }
    text
}

#[derive(Clone, Debug, PartialEq)]
pub struct LogEvent {
    pub level: Level,
    /// Stable dotted identifier, such as `ws.reconnect`; message text may change.
    pub code: &'static str,
    pub message: String,
    pub fields: Vec<(&'static str, Value)>,
}

/// Codes the engine counts in [`Health`]; venues emit them through `Cx::event`.
pub mod code {
    pub const WS_CLOSED: &str = "ws.closed";
    pub const WS_RECONNECT: &str = "ws.reconnect";
    pub const AUTH_REJECTED: &str = "auth.rejected";
    pub const HTTP_FAILED: &str = "http.failed";
    /// The engine counts HTTP 429 itself; venues emit this only for socket limits.
    pub const RATE_LIMITED: &str = "rate_limited";
    pub const SNAPSHOT_DISCARDED: &str = "snapshot.discarded";
    pub const ROW_SKIPPED: &str = "row.skipped";
}

/// Counters since engine construction, read by hosts through `getHealth()`.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    pub ws_opened: u32,
    pub ws_closed: u32,
    pub reconnects: u32,
    pub auth_failures: u32,
    pub http_failures: u32,
    pub rate_limited: u32,
    pub snapshots_discarded: u32,
    pub rows_skipped: u32,
}

impl Health {
    pub(crate) fn count(&mut self, code: &str) {
        let counter = match code {
            code::WS_CLOSED => &mut self.ws_closed,
            code::WS_RECONNECT => &mut self.reconnects,
            code::AUTH_REJECTED => &mut self.auth_failures,
            code::HTTP_FAILED => &mut self.http_failures,
            code::RATE_LIMITED => &mut self.rate_limited,
            code::SNAPSHOT_DISCARDED => &mut self.snapshots_discarded,
            code::ROW_SKIPPED => &mut self.rows_skipped,
            _ => return,
        };
        *counter = counter.saturating_add(1);
    }
}
