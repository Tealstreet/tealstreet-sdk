//! Error reports for a host's telemetry. The native host logs to stderr only;
//! a host that reports errors elsewhere (the exchange server's Sentry) installs
//! one sink. Reports carry hosts, routes and codes, never a URL query, token,
//! credential or payload, and are already rate-limited where they are raised.
use std::sync::OnceLock;

/// What failed. Each kind is one alertable condition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// A Hosted account's start failed (`hosted.start_failed`).
    AccountStartFailed,
    /// A venue, relay or HTTP-tunnel socket did not open.
    SocketOpenFailed,
    /// A venue request failed in transport, directly or through the HTTP tunnel.
    RequestFailed,
}

impl ErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AccountStartFailed => "account.start_failed",
            Self::SocketOpenFailed => "transport.socket_open_failed",
            Self::RequestFailed => "transport.request_failed",
        }
    }
}

/// One report. `label` is the account's log label (`user_tag account venue`)
/// where there is one; `detail` is raw error text and the sink must scrub it.
pub struct ErrorReport<'a> {
    pub kind: ErrorKind,
    pub label: Option<&'a str>,
    /// A host or route (`host` or `host via HTTP tunnel`), never a URL.
    pub target: Option<&'a str>,
    /// The engine's error kind and the venue's code, for a start failure.
    pub error_kind: Option<&'a str>,
    pub venue_code: Option<&'a str>,
    pub detail: &'a str,
}

static SINK: OnceLock<fn(&ErrorReport<'_>)> = OnceLock::new();

/// Installs the process's error sink; false when one is already installed.
pub fn set_error_sink(sink: fn(&ErrorReport<'_>)) -> bool {
    SINK.set(sink).is_ok()
}

pub(crate) fn report(report: &ErrorReport<'_>) {
    if let Some(sink) = SINK.get() {
        sink(report);
    }
}
