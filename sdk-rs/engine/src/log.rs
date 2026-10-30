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

/// Codes counted in [`Health`]. The engine counts socket closes, reconnects
/// on a declared socket role, HTTP failures, and auth failures it sees
/// (`deactivate`, and auth-kind startup or command failures); venues emit the
/// rest through `Cx::event`. Venues tag every credential rejection
/// `auth.rejected`, and `Cx::retried_failure_error` does the same for a private
/// read the venue refused with an auth kind: one engine event counts at most one
/// auth failure, however many of these paths report it.
pub mod code {
    pub const WS_CLOSED: &str = "ws.closed";
    pub const WS_RECONNECT: &str = "ws.reconnect";
    pub const AUTH_REJECTED: &str = "auth.rejected";
    pub const HTTP_FAILED: &str = "http.failed";
    /// The engine counts HTTP 429 itself; venues emit this only for socket limits.
    pub const RATE_LIMITED: &str = "rate_limited";
    /// Counted once per report: a single discarded snapshot or row.
    pub const SNAPSHOT_DISCARDED: &str = "snapshot.discarded";
    /// Counted once per report: one skipped row, or one summary of several.
    pub const ROW_SKIPPED: &str = "row.skipped";

    /// A replyless command diagnostic; attributed commands use CommandError.
    pub const COMMAND_FAILED: &str = "command.failed";
    /// A command ID was reused while the original command remains in flight.
    pub const COMMAND_DUPLICATE: &str = "command.duplicate";
    /// Startup is still missing required data after its watchdog deadline.
    pub const STARTUP_INCOMPLETE: &str = "startup.incomplete";
    /// Repeated public startup failures; retries continue without intervention.
    pub const STARTUP_DEGRADED: &str = "startup.degraded";
    /// Both required public sections loaded after startup degradation.
    pub const STARTUP_RECOVERED: &str = "startup.recovered";
    /// A trusted public-feed update was rejected by validation.
    pub const PUBLIC_FEED_REJECTED: &str = "public_feed.rejected";

    // Not counted. Hosts and apps match these codes, never message text.
    /// A socket dropped and the venue is reconnecting it.
    pub const WS_DISCONNECTED: &str = "ws.disconnected";
    /// A private or user-data stream could not be established or kept alive.
    pub const STREAM_UNAVAILABLE: &str = "stream.unavailable";
    /// A frame or response could not be decoded and was ignored.
    pub const STREAM_MALFORMED: &str = "stream.malformed";
    /// The venue refused a subscription, or one would exceed its limits.
    pub const SUBSCRIPTION_REJECTED: &str = "subscription.rejected";
    /// A REST read failed; the venue retries on its own schedule.
    pub const REQUEST_FAILED: &str = "request.failed";
    /// Reads are paused after the venue limited them.
    pub const REQUEST_PAUSED: &str = "request.paused";
    /// A price needed for valuation is missing; the previous value is kept.
    pub const VALUATION_UNAVAILABLE: &str = "valuation.unavailable";
    /// The venue rejected an order write.
    pub const ORDER_REJECTED: &str = "order.rejected";
    /// One leg of a multi-order command (bracket, close, replace) failed.
    pub const CHILD_LEG_FAILED: &str = "child_leg.failed";
    /// Existing orders were cancelled or replaced as part of a command.
    pub const ORDERS_REPLACED: &str = "orders.replaced";
    /// A history walk stopped before covering its whole range.
    pub const HISTORY_INCOMPLETE: &str = "history.incomplete";
    /// A request asked for something this venue does not support.
    pub const COMMAND_UNSUPPORTED: &str = "command.unsupported";
    /// Account options are invalid, so the venue cannot start as configured.
    pub const CONFIG_INVALID: &str = "config.invalid";
    /// A setting change was applied, but may behave unexpectedly as the user asked it.
    pub const SETTING_CAUTION: &str = "setting.caution";
    /// Startup progress, such as markets loaded.
    pub const STARTUP_PROGRESS: &str = "startup.progress";
    /// A routine account summary, logged at Debug.
    pub const BALANCE_SUMMARY: &str = "balance.summary";

    /// The code for a startup refusal, from the error's kind: only a
    /// credential rejection is `auth.rejected`.
    pub fn for_startup_error(error: &crate::error::VenueError) -> &'static str {
        use crate::error::ErrorKind;
        match error.kind {
            Some(ErrorKind::AuthInvalid | ErrorKind::AuthExpired | ErrorKind::AuthPermission) => {
                AUTH_REJECTED
            }
            Some(ErrorKind::InvalidInput | ErrorKind::Unsupported) => CONFIG_INVALID,
            _ => REQUEST_FAILED,
        }
    }
}

/// Counters since engine construction, read by hosts through `getHealth()`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Health {
    pub ws_opened: u32,
    pub ws_closed: u32,
    pub reconnects: u32,
    pub auth_failures: u32,
    pub http_failures: u32,
    pub rate_limited: u32,
    pub snapshots_discarded: u32,
    pub rows_skipped: u32,
    /// Engine clock time of the newest frame on a private-role socket. Venues
    /// that carry public feeds on that socket (Hyperliquid) advance it with them.
    pub last_private_message_ms: Option<f64>,
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
