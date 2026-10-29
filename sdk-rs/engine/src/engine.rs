//! The sans-I/O driver. Hosts feed [`Event`]s in and execute the [`Action`]s
//! that come out; venues never touch sockets, HTTP clients or clocks directly.
#[path = "address_nonce_setup.rs"]
pub(crate) mod address_nonce_setup;
#[cfg(feature = "nonce-handoff")]
#[path = "nonce_handoff_core.rs"]
pub(crate) mod nonce_handoff_core;
#[cfg(feature = "address-nonce")]
pub use address_nonce_setup::{AddressSetupCleanup, AddressSetupProgress, SetupHostSupport};
pub use address_nonce_setup::{AddressSetupSlots, SetupRetirement};

use crate::book::{Book, BookSide};
use crate::capabilities::{AccountLookup, Capabilities};
use crate::error::{ErrorKind, VenueError};
use crate::json::{ArrayAt, ArraySplitter, ItemCheck};
use crate::log::{Health, Level, LogEvent, Value, code as log_code};
use crate::model::{
    CancelOrderRef, Candle, Interval, OrderIdentityReceipt, OrderSide, PlaceOrderOpts, PositionSide,
};
use crate::public_feed::{PublicFeedPolicy, PublicFeedUpdate};
use crate::store::Store;
use crate::wire::{FrameWriter, section};
use std::collections::HashMap;

/// Bounds on a host-scaled poll cadence, beyond a venue's own native cadence.
pub const POLL_POLICY_FLOOR_MS: f64 = 1_000.0;
pub const POLL_POLICY_CAP_MS: f64 = 120_000.0;
/// Close code the default `on_route_exhausted` reports: abnormal closure, no peer frame.
pub const ROUTE_EXHAUSTED_CLOSE: u16 = 1006;

/// Bound a configured absolute cadence; positive infinity explicitly pauses polling.
pub fn bounded_tick_interval(interval_ms: f64) -> f64 {
    if interval_ms.is_finite() {
        interval_ms.clamp(POLL_POLICY_FLOOR_MS, POLL_POLICY_CAP_MS)
    } else {
        f64::INFINITY
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SocketId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RequestId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TimerId(pub u64);
/// Explicit limits on a periodic poll, separate from its fallback cadence.
#[derive(Clone, Copy, Debug)]
pub struct PollTimerBounds {
    /// Recurring rate floor, including after configuration changes.
    pub minimum_interval_ms: f64,
    /// Absolute cooldown/backoff deadline; configuration changes never move it.
    pub not_before_ms: f64,
}

impl Default for PollTimerBounds {
    fn default() -> Self {
        Self {
            minimum_interval_ms: 0.,
            not_before_ms: f64::NEG_INFINITY,
        }
    }
}
/// Correlates a [`Command`] with its [`Notice::CommandDone`].
pub type CommandId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }
}

/// A public request can select a host-owned egress without naming a proxy URL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestRoute {
    /// A primary-account read must not use the secondary read identity/egress.
    Primary,
    /// A read signed by the secondary key uses the read identity's egress.
    Secondary,
    Direct,
    PublicCors,
}
impl RequestRoute {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Secondary => "secondary",
            Self::Direct => "direct",
            Self::PublicCors => "publicCors",
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct HttpRequest {
    pub id: RequestId,
    pub method: Method,
    pub route: Option<RequestRoute>,
    pub timeout_ms: Option<u32>,
    pub url: String,
    pub headers: Vec<(&'static str, String)>,
    /// Delegated signer headers; fixed venue authentication headers take precedence.
    pub signer_headers: Vec<(String, String)>,
    pub body: Option<String>,
}

impl std::fmt::Debug for HttpRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpRequest")
            .field("id", &self.id)
            .field("method", &self.method)
            .field("route", &self.route)
            .field("timeout_ms", &self.timeout_ms)
            .field("url", &"<redacted>")
            .field("headers", &"<redacted>")
            .field("signer_headers", &"<redacted>")
            .field("body", &"<redacted>")
            .finish()
    }
}

/// Construction-time signer slot, captured before dispatching an account read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CredentialSlot {
    Primary,
    Secondary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AccountReadScope {
    Account,
    Balance,
    Orders,
    Positions,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthOutcome {
    Accepted,
    Rejected,
}

impl AuthOutcome {
    /// Validate a successful account envelope's required list, without decoding
    /// row fields. Call after the venue's typed success/rejection and fence checks.
    /// Row mapping failures do not invalidate credential evidence; a missing or
    /// malformed envelope/list must stay inert before grouped auth state changes.
    pub fn accepted_list(response: &Response<'_>, at: ArrayAt) -> Option<Self> {
        if !(200..300).contains(&response.status) {
            return None;
        }
        let mut list = ArraySplitter::new(at);
        list.feed(response.body.as_bytes(), |_| {});
        list.finished().then_some(Self::Accepted)
    }
}

/// Auth evidence for one scope served by several endpoints (orders as regular,
/// plan and stop reads). A rejection reports at once; an acceptance reports only
/// once no endpoint of the scope still rejects, so a sibling's success cannot
/// reset the streak of a key refused on one endpoint.
#[derive(Debug, Default)]
pub struct MergedScopeAuth(std::cell::Cell<u32>);

impl MergedScopeAuth {
    /// `endpoint` is the endpoint's index within the scope (below 32).
    pub fn report(&self, endpoint: u32, outcome: Option<AuthOutcome>) -> Option<AuthOutcome> {
        let bit = 1 << endpoint;
        match outcome? {
            AuthOutcome::Rejected => {
                self.0.set(self.0.get() | bit);
                Some(AuthOutcome::Rejected)
            }
            AuthOutcome::Accepted => {
                self.0.set(self.0.get() & !bit);
                (self.0.get() == 0).then_some(AuthOutcome::Accepted)
            }
        }
    }
}

#[cfg(test)]
mod merged_scope_auth_tests {
    use super::{AuthOutcome, AuthOutcome::*, MergedScopeAuth, Response};
    use crate::json::ArrayAt;

    #[test]
    fn accepted_list_checks_envelope_shape_without_decoding_row_fields() {
        for (at, body, accepted) in [
            (ArrayAt::Root, "[]", true),
            (ArrayAt::Root, "[false,{},null]", true),
            (ArrayAt::Root, "null", false),
            (ArrayAt::Root, "[false", false),
            (ArrayAt::EnvelopeField("data"), r#"{"data":[false]}"#, true),
            (ArrayAt::EnvelopeField("data"), r#"{"data":null}"#, false),
            (ArrayAt::EnvelopeField("data"), r#"{"data":{}}"#, false),
            (ArrayAt::EnvelopeField("data"), r#"{}"#, false),
            (
                ArrayAt::EnvelopeFieldOrNested("data", "orders"),
                r#"{"data":[]}"#,
                true,
            ),
            (
                ArrayAt::EnvelopeFieldOrNested("data", "orders"),
                r#"{"data":{"orders":[false]}}"#,
                true,
            ),
            (
                ArrayAt::EnvelopeFieldOrNested("data", "orders"),
                r#"{"data":{"orders":null}}"#,
                false,
            ),
            (
                ArrayAt::EnvelopeFieldOrNested("data", "orders"),
                r#"{"data":{"orders":[]},"#,
                false,
            ),
        ] {
            let response = Response {
                status: 200,
                headers: "",
                body,
            };
            assert_eq!(
                AuthOutcome::accepted_list(&response, at),
                accepted.then_some(Accepted),
                "{at:?} {body}"
            );
        }
        let limited = Response {
            status: 429,
            headers: "",
            body: "[]",
        };
        assert_eq!(AuthOutcome::accepted_list(&limited, ArrayAt::Root), None);
    }

    #[test]
    fn missing_list_cannot_clear_a_grouped_endpoint_before_its_sibling_succeeds() {
        let auth = MergedScopeAuth::default();
        assert_eq!(auth.report(0, Some(Rejected)), Some(Rejected));
        let response = Response {
            status: 200,
            headers: "",
            body: "{}",
        };
        assert_eq!(
            auth.report(0, AuthOutcome::accepted_list(&response, ArrayAt::Root)),
            None
        );
        assert_eq!(auth.report(1, Some(Accepted)), None);
        let response = Response {
            body: "[false]",
            ..response
        };
        assert_eq!(
            auth.report(0, AuthOutcome::accepted_list(&response, ArrayAt::Root)),
            Some(Accepted)
        );
    }

    #[test]
    fn a_rejected_endpoint_holds_back_its_siblings_acceptance_until_it_recovers() {
        let auth = MergedScopeAuth::default();
        assert_eq!(auth.report(0, Some(Accepted)), Some(Accepted));
        assert_eq!(auth.report(1, Some(Rejected)), Some(Rejected));
        // Every cycle: regular accepted, plan rejected. The streak can only grow.
        for _ in 0..3 {
            assert_eq!(auth.report(0, Some(Accepted)), None);
            assert_eq!(auth.report(2, None), None);
            assert_eq!(auth.report(1, Some(Rejected)), Some(Rejected));
        }
        assert_eq!(auth.report(0, Some(Accepted)), None);
        assert_eq!(auth.report(1, Some(Accepted)), Some(Accepted));
        assert_eq!(auth.report(0, Some(Accepted)), Some(Accepted));
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Notice {
    /// Definitive evidence from a credential-bearing private account read.
    AccountAuth {
        credential: CredentialSlot,
        scope: AccountReadScope,
        outcome: AuthOutcome,
    },
    /// Definitive credential rejection; transient failures never deactivate.
    Deactivate,
    Event(LogEvent),
    /// Error diagnostic attributed to a command; completion owns its outcome.
    CommandError {
        command: CommandId,
        message: String,
    },
    /// The command succeeded; `result` is its JSON. Failures arrive as `CommandFailed`.
    CommandDone {
        command: CommandId,
        result: String,
    },
    /// Actual native write identities, emitted before their corresponding I/O.
    CommandProgress {
        command: CommandId,
        receipts: Vec<OrderIdentityReceipt>,
    },
    /// Startup cannot complete without intervention (bad configuration or an
    /// account the venue refuses); hosts fail `start` with it.
    StartupFailed {
        error: VenueError,
    },
    /// Completes a command with a structured failure; hosts report it like a
    /// `CommandDone` error, plus the detail.
    CommandFailed {
        command: CommandId,
        error: VenueError,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Fill {
    /// Native execution identity, distinct from the order identity when reported.
    pub id: Option<String>,
    pub order_id: String,
    pub symbol: String,
    pub side: OrderSide,
    pub price: f64,
    pub amount: f64,
}

/// Optional financial values for a live execution, in its venue's event
/// convention. This is separate from historical execution PnL normalization.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FillFinancials {
    pub realized_pnl: Option<f64>,
    pub fee: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SocketChannel {
    Public,
    Private,
    Business,
}

/// Stable feed identity and protocol recovery; hosts choose transport routes.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct SocketContext {
    pub feed: String,
    pub recover: bool,
    /// The venue refused the last route's egress: advance exactly one route
    /// candidate for this connect, or report `WsRouteExhausted` without dialling.
    #[serde(rename = "rejectRoute", skip_serializing_if = "std::ops::Not::not")]
    pub reject_route: bool,
}

/// Everything a host needs to open one socket; `Cx::connect` takes it or a bare URL.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SocketSpec {
    pub url: String,
    /// Logical feed role lets hosts route identical venue URLs independently.
    pub channel: Option<SocketChannel>,
    pub context: Option<SocketContext>,
    pub headers: Vec<(&'static str, String)>,
    /// Binary protocols still receive text control responses through on_message.
    pub binary: bool,
}

impl SocketSpec {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            ..Self::default()
        }
    }

    pub fn channel(mut self, channel: SocketChannel) -> Self {
        self.channel = Some(channel);
        self
    }

    pub fn feed(mut self, feed: impl Into<String>, recover: bool) -> Self {
        self.context = Some(SocketContext {
            feed: feed.into(),
            recover,
            reject_route: false,
        });
        self
    }

    /// Ask the host to leave the current route; needs a `feed` context.
    pub fn reject_route(mut self) -> Self {
        if let Some(context) = self.context.as_mut() {
            context.reject_route = true;
        }
        self
    }

    pub fn headers(mut self, headers: Vec<(&'static str, String)>) -> Self {
        self.headers = headers;
        self
    }

    pub fn binary(mut self) -> Self {
        self.binary = true;
        self
    }
}

impl From<&str> for SocketSpec {
    fn from(url: &str) -> Self {
        Self::new(url)
    }
}

impl From<String> for SocketSpec {
    fn from(url: String) -> Self {
        Self::new(url)
    }
}

impl From<&String> for SocketSpec {
    fn from(url: &String) -> Self {
        Self::new(url.as_str())
    }
}

#[derive(Clone)]
pub enum Action {
    WsConnect {
        /// Preserve application binary frames rather than decoding UTF-8.
        binary: bool,
        socket: SocketId,
        url: String,
        /// Upgrade authentication; hosts must fail closed if headers are unsupported.
        headers: Vec<(&'static str, String)>,
        channel: Option<SocketChannel>,
        context: Option<SocketContext>,
    },
    WsSend {
        socket: SocketId,
        text: String,
    },
    WsClose {
        socket: SocketId,
    },
    Http(HttpRequest),
    ManagedHttp(Box<crate::request_context::HttpAction>),
    /// Internal outbound queue wrapper; always unwrapped/validated by drain.
    SignedOutbound {
        attempt: RequestId,
        outbound: Box<Action>,
    },
    /// Atomic primitive/credential work; hosts never log payload or result bytes.
    Sign {
        attempt: RequestId,
        work: crate::signing::SignRequest,
        timeout_ms: u32,
    },
    SetTimer {
        timer: TimerId,
        at_ms: f64,
    },
    ClearTimer {
        timer: TimerId,
    },
    Notice(Notice),
    RetireAddressSetup {
        task: RequestId,
        token: std::sync::Arc<SetupRetirement>,
    },
}

impl PartialEq for Action {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::RetireAddressSetup { task: a, token: x },
                Self::RetireAddressSetup { task: b, token: y },
            ) => a == b && std::sync::Arc::ptr_eq(x, y),
            (
                Self::WsConnect {
                    binary: a,
                    socket: b,
                    url: c,
                    headers: d,
                    channel: e,
                    context: f,
                },
                Self::WsConnect {
                    binary: g,
                    socket: h,
                    url: i,
                    headers: j,
                    channel: k,
                    context: l,
                },
            ) => (a, b, c, d, e, f) == (g, h, i, j, k, l),
            (Self::WsSend { socket: a, text: b }, Self::WsSend { socket: c, text: d }) => {
                (a, b) == (c, d)
            }
            (Self::WsClose { socket: a }, Self::WsClose { socket: b }) => a == b,
            (Self::Http(a), Self::Http(b)) => a == b,
            (Self::ManagedHttp(a), Self::ManagedHttp(b)) => a == b,
            (
                Self::SignedOutbound {
                    attempt: a,
                    outbound: b,
                },
                Self::SignedOutbound {
                    attempt: c,
                    outbound: d,
                },
            ) => (a, b) == (c, d),
            (
                Self::Sign {
                    attempt: a,
                    work: b,
                    timeout_ms: c,
                },
                Self::Sign {
                    attempt: d,
                    work: e,
                    timeout_ms: f,
                },
            ) => (a, b, c) == (d, e, f),
            (Self::SetTimer { timer: a, at_ms: b }, Self::SetTimer { timer: c, at_ms: d }) => {
                (a, b) == (c, d)
            }
            (Self::ClearTimer { timer: a }, Self::ClearTimer { timer: b }) => a == b,
            (Self::Notice(a), Self::Notice(b)) => a == b,
            _ => false,
        }
    }
}

impl std::fmt::Debug for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Notice(notice) => f.debug_tuple("Notice").field(notice).finish(),
            _ => f
                .debug_struct("Action")
                .field("kind", &std::mem::discriminant(self))
                .finish_non_exhaustive(),
        }
    }
}

/// Venue-specific actions outside the shared trading surface.
#[derive(Clone, Debug, PartialEq)]
pub enum VenueAction {
    /// Hyperliquid account-wide absolute UTCms deadline; None clears it.
    ScheduleCancel { time_ms: Option<f64> },
    /// Atomically refresh public instrument discovery without restarting the account.
    RefreshMarkets,
    /// Lighter: the cached account tier as a JSON string, or `null`.
    FetchAccountTier,
    /// Lighter: switch the account tier (`standard`, `premium` or `plus`).
    SetAccountTier { tier: String },
    /// Venue-reported closed positions as the venue's own JSON rows, newest
    /// window first, for hosts that map them with the venue's TS mappers.
    FetchClosedPositions {
        symbol: Option<String>,
        from_ms: Option<f64>,
        to_ms: Option<f64>,
    },
    /// The venue's raw account balance payload as JSON, or `null` before one
    /// is read, for hosts that expose it as `balance.info`.
    FetchBalanceInfo,
}

/// The shared rejection for a venue action the venue does not implement.
const UNSUPPORTED_VENUE_ACTION: &str = "This action is not supported on this venue";
#[cfg(feature = "order-locks")]
const UNCERTAIN_PLACEMENT: &str =
    "Placement outcome is uncertain; reconcile before placing or amending";
#[cfg(feature = "order-locks")]
const UNCERTAIN_CANCEL_ALL: &str = "Placement outcome is uncertain; reconcile before cancel-all";

pub fn unsupported_venue_action<P>(cx: &mut Cx<'_, P>, command: CommandId) {
    cx.fail(command, UNSUPPORTED_VENUE_ACTION);
}

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    FetchOptionChain {
        command: CommandId,
        query: crate::model::OptionChainQuery,
    },
    FetchOptionExpiries {
        command: CommandId,
        underlying: String,
    },
    FetchAccountGreeks {
        command: CommandId,
    },
    SetTickerSymbols {
        symbols: Vec<String>,
    },
    SubscribeOrderBook {
        symbol: String,
    },
    UnsubscribeOrderBook {
        symbol: String,
    },
    SubscribeCandles {
        symbol: String,
        interval: Interval,
    },
    UnsubscribeCandles {
        symbol: String,
        interval: Interval,
    },
    PlaceOrders {
        command: CommandId,
        orders: Vec<PlaceOrderOpts>,
    },
    CancelOrders {
        command: CommandId,
        ids: Vec<String>,
    },
    /// Cancel using caller-supplied routing context before Store sees the order.
    CancelOrdersWithSymbols {
        command: CommandId,
        orders: Vec<CancelOrderRef>,
    },
    /// Change an open order's price or amount.
    AmendOrder {
        command: CommandId,
        id: String,
        price: Option<f64>,
        amount: Option<f64>,
    },
    AmendOrderWithSymbol {
        command: CommandId,
        id: String,
        symbol: String,
        price: Option<f64>,
        amount: Option<f64>,
    },
    CancelSymbolOrders {
        command: CommandId,
        symbol: String,
    },
    SetLeverage {
        command: CommandId,
        symbol: String,
        leverage: f64,
        side: Option<PositionSide>,
    },
    SetMarginType {
        command: CommandId,
        symbol: String,
        isolated: bool,
    },
    SetPositionMode {
        command: CommandId,
        hedged: bool,
        symbol: Option<String>,
    },
    FetchStpMode {
        command: CommandId,
    },
    FetchAccount {
        command: CommandId,
    },
    FetchOrderBook {
        command: CommandId,
        symbol: String,
        limit: Option<u32>,
    },
    SetStpMode {
        command: CommandId,
        mode: crate::model::StpMode,
    },
    /// A venue-specific action; the engine rejects it unless the venue implements
    /// `Venue::on_venue_action`, so venue `on_command` matches never see it.
    Venue {
        command: CommandId,
        action: VenueAction,
    },
    /// Refresh account-specific leverage rails and margin mode for a symbol.
    FetchLeverage {
        command: CommandId,
        symbol: String,
    },
    /// Refresh the venue/account's maximum leverage without assuming flat leverage.
    FetchMaxLeverage {
        command: CommandId,
        symbol: String,
    },
    /// A rare request/response read (history, income, trades); the result is
    /// JSON in [`Notice::CommandDone`] and never enters the frame.
    Query {
        command: CommandId,
        query: Query,
    },
    FetchCandles {
        command: CommandId,
        symbol: String,
        interval: Interval,
        limit: Option<u32>,
        from_ms: Option<f64>,
        to_ms: Option<f64>,
    },
}

impl Command {
    /// The logical intent a write under this command serves. Venue actions
    /// classify conservatively as placements.
    pub(crate) fn write_intent(&self) -> crate::classify::Intent {
        use crate::classify::Intent;
        match self {
            Self::PlaceOrders { .. } => Intent::Place,
            Self::CancelOrders { .. }
            | Self::CancelOrdersWithSymbols { .. }
            | Self::CancelSymbolOrders { .. } => Intent::Cancel,
            Self::AmendOrder { .. } | Self::AmendOrderWithSymbol { .. } => Intent::Amend,
            Self::SetLeverage { .. }
            | Self::SetMarginType { .. }
            | Self::SetPositionMode { .. }
            | Self::SetStpMode { .. } => Intent::Setting,
            Self::Venue { .. } => Intent::Place,
            _ => Intent::Read,
        }
    }

    pub fn completion_id(&self) -> Option<CommandId> {
        match self {
            Self::PlaceOrders { command, .. }
            | Self::AmendOrderWithSymbol { command, .. }
            | Self::AmendOrder { command, .. }
            | Self::CancelOrders { command, .. }
            | Self::CancelOrdersWithSymbols { command, .. }
            | Self::CancelSymbolOrders { command, .. }
            | Self::SetLeverage { command, .. }
            | Self::SetMarginType { command, .. }
            | Self::SetPositionMode { command, .. }
            | Self::SetStpMode { command, .. }
            | Self::FetchStpMode { command }
            | Self::FetchAccount { command }
            | Self::FetchOrderBook { command, .. }
            | Self::FetchLeverage { command, .. }
            | Self::FetchMaxLeverage { command, .. }
            | Self::Query { command, .. }
            | Self::FetchCandles { command, .. }
            | Self::FetchOptionChain { command, .. }
            | Self::FetchOptionExpiries { command, .. }
            | Self::FetchAccountGreeks { command }
            | Self::Venue { command, .. } => Some(*command),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Query {
    /// Resolve an uncertain placement using the venue-native client ID.
    OrderByClientId {
        symbol: String,
        client_order_id: String,
    },
    /// The account's own executions.
    Fills {
        symbol: Option<String>,
        order_id: Option<String>,
        from_ms: Option<f64>,
        to_ms: Option<f64>,
        limit: Option<u32>,
    },
    /// Closed and canceled orders.
    OrderHistory {
        symbol: Option<String>,
        from_ms: Option<f64>,
        to_ms: Option<f64>,
        limit: Option<u32>,
    },
    /// Realized PnL, funding and commission ledger rows.
    Income {
        symbol: Option<String>,
        kind: Option<String>,
        from_ms: Option<f64>,
        to_ms: Option<f64>,
        limit: Option<u32>,
    },
    PublicTrades {
        symbol: String,
        limit: Option<u32>,
    },
}

/// An HTTP response as the host received it.
#[derive(Clone, Copy, Debug)]
pub struct Response<'a> {
    pub status: u16,
    /// Raw `name: value` lines separated by `\n`; names in any case.
    pub headers: &'a str,
    pub body: &'a str,
}

impl<'a> Response<'a> {
    pub fn header(&self, name: &str) -> Option<&'a str> {
        self.headers.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.trim().eq_ignore_ascii_case(name).then(|| value.trim())
        })
    }
}

pub enum Event<'a> {
    /// Host polling policy: a factor on each poll's own cadence (resolved SDK
    /// tick over its metadata tick); infinity pauses optional venue polling.
    PollScaleChanged {
        scale: f64,
    },
    /// Host's absolute bound on slowed polls (its resolved tick); `None` lifts it.
    PollCeilingChanged {
        ceiling_ms: Option<f64>,
    },
    WsOpened {
        socket: SocketId,
    },
    WsBinary {
        socket: SocketId,
        data: &'a [u8],
    },
    WsMessage {
        socket: SocketId,
        text: &'a str,
    },
    WsClosed {
        socket: SocketId,
        code: u16,
    },
    /// Host-only: a `reject_route` connect had no route left, so nothing was
    /// dialled. It is that socket's only terminal; no open or close follows.
    WsRouteExhausted {
        socket: SocketId,
    },
    /// A complete response.
    HttpResponse {
        request: RequestId,
        response: Response<'a>,
    },
    /// A response delivered in parts: start, any number of chunks, end.
    HttpStart {
        request: RequestId,
        status: u16,
        headers: &'a str,
    },
    HttpChunk {
        request: RequestId,
        data: &'a [u8],
    },
    HttpEnd {
        request: RequestId,
    },
    HttpFailed {
        request: RequestId,
        error: &'a str,
    },
    Signed {
        attempt: RequestId,
        result: Result<crate::signing::SignResult, crate::signing::SignError>,
    },
    TimerFired {
        timer: TimerId,
    },
    Command(Command),
}

impl std::fmt::Debug for Event<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Events include authenticated payloads and headers.
        f.debug_struct("Event")
            .field("kind", &std::mem::discriminant(self))
            .finish_non_exhaustive()
    }
}

#[path = "request_context_entry.rs"]
mod command_entry;

#[cfg(test)]
#[path = "request_context_entry_tests.rs"]
mod command_entry_tests;
#[cfg(test)]
#[path = "command_wait_tests.rs"]
mod command_wait_tests;
#[cfg(all(test, feature = "order-locks"))]
#[path = "order_lock_capacity_tests.rs"]
mod order_lock_capacity_tests;
#[cfg(test)]
#[path = "pending_drop_tests.rs"]
mod pending_drop_tests;
#[cfg(test)]
#[path = "read_root_expiry_tests.rs"]
mod read_root_expiry_tests;
#[cfg(all(test, feature = "release-domains"))]
#[path = "release_domain_tests.rs"]
mod release_domain_tests;

/// What a venue may do while handling an event.
pub struct Cx<'a, P> {
    pub now: f64,
    pub store: &'a mut Store,
    io: &'a mut Io<P>,
}

/// A response arriving in parts.
struct Incoming {
    status: u16,
    headers: String,
    /// Splits a streamed array out of a successful body.
    splitter: Option<ArraySplitter>,
    /// Everything not streamed: error bodies and unstreamed requests. A
    /// streamed body's envelope is kept by its splitter.
    body: Vec<u8>,
}

/// How much of a streamed envelope is quoted to report a response without its array.
const STREAM_PREVIEW_BYTES: usize = 512;

struct PollTimer {
    logical: TimerId,
    default_interval_ms: f64,
    bounds: PollTimerBounds,
    physical: Option<TimerId>,
}

#[derive(Clone, Copy)]
struct ReadAuth {
    credential: CredentialSlot,
    scope: AccountReadScope,
}

struct PendingHttp<P> {
    value: P,
    auth: Option<ReadAuth>,
}

#[path = "read_root_expiry.rs"]
mod read_root_expiry;
#[cfg(feature = "release-domains")]
#[path = "release_domain.rs"]
mod release_domain;
#[cfg(feature = "repair-mutations")]
#[path = "repair_core.rs"]
mod repair_core;
#[path = "request_context_registry.rs"]
mod request_context_registry;

#[path = "http_lifecycle.rs"]
mod http_lifecycle;

#[path = "execution_cx.rs"]
mod execution_cx;

#[cfg(test)]
#[path = "execution_cx_tests.rs"]
mod execution_cx_tests;
pub use command_entry::{CommandWait, DEFAULT_COMMAND_WAIT_MS, MAX_COMMAND_WAIT_MS};
pub use execution_cx::{ExecutionHandle, Joined, Window};

/// A delivered response whose bytes the engine cannot hand to a codec. The
/// venue answered, so this is never transport loss.
fn malformed_body(message: &str) -> VenueError {
    VenueError::new(message).kind(ErrorKind::Internal)
}

fn observe_account_auth<V: Venue>(
    venue: &V,
    pending: &PendingHttp<V::Pending>,
    response: &Response<'_>,
    expected_list_present: bool,
    now: f64,
    store: &Store,
    actions: &mut Vec<Action>,
) {
    let Some(auth) = pending.auth else { return };
    let Some(outcome) = venue.account_auth_outcome(&pending.value, response, now, store) else {
        return;
    };
    if outcome == AuthOutcome::Accepted && !expected_list_present {
        return;
    }
    actions.push(Action::Notice(Notice::AccountAuth {
        credential: auth.credential,
        scope: auth.scope,
        outcome,
    }));
}

/// Host poll policy and the registered polls. Deliberately not generic over a
/// venue's pending type: every venue in a bundle shares this one copy.
#[derive(Default)]
struct Polls {
    scale: Option<f64>,
    ceiling_ms: Option<f64>,
    /// A venue registers only a few polls, kept in registration order.
    entries: Vec<PollTimer>,
}

/// The engine timer state a poll change writes to.
struct TimerSink<'a> {
    next_id: &'a mut crate::resource_ids::ResourceIds,
    timers: &'a mut crate::IdMap<TimerId, ()>,
    actions: &'a mut Vec<Action>,
}

fn next_engine_id(next_id: &mut crate::resource_ids::ResourceIds) -> u64 {
    next_id.next_or_fatal()
}

#[cfg(test)]
mod resource_identity_regressions {
    use super::*;

    #[test]
    fn physical_engine_allocation_crosses_u32_without_wrapping() {
        let mut next = crate::resource_ids::ResourceIds::starting_at(u64::from(u32::MAX));
        assert_eq!(next_engine_id(&mut next), u64::from(u32::MAX));
        assert_eq!(next_engine_id(&mut next), 1_u64 << 32);
        assert_eq!(next_engine_id(&mut next), (1_u64 << 32) + 1);
    }

    #[test]
    fn later_allocations_never_reissue_a_retired_socket_identity() {
        let retired = SocketId(1);
        let mut next = crate::resource_ids::ResourceIds::starting_at(u64::from(u32::MAX - 3));
        let later: Vec<_> = (0..32)
            .map(|_| SocketId(next_engine_id(&mut next)))
            .collect();
        assert!(
            !later.contains(&retired),
            "retired socket identity was reissued"
        );
        assert!(later.windows(2).all(|pair| pair[0].0 < pair[1].0));
    }

    #[derive(Default)]
    struct Probe {
        socket: Option<SocketId>,
        received: usize,
        exhaust: bool,
        catch_sign_failure: bool,
        caught: bool,
    }
    struct Refused;
    impl crate::SignStep for Refused {
        fn prepare(
            &mut self,
            _: &mut Action,
            _: &crate::DispatchContext<'_>,
        ) -> Result<crate::signing::SignRequest, crate::signing::SignError> {
            Err(crate::signing::SignError::Refused)
        }
        fn apply(
            &mut self,
            _: &mut Action,
            _: &crate::signing::SignResult,
        ) -> Result<(), crate::signing::SignError> {
            unreachable!()
        }
        fn freshness_ms(&self) -> f64 {
            1_000.
        }
    }
    impl Venue for Probe {
        type Pending = ();
        fn start(&mut self, cx: &mut Cx<'_, ()>) {
            if self.catch_sign_failure {
                let request = cx.request(
                    Method::Get,
                    "https://venue.test/read".into(),
                    vec![],
                    None,
                    (),
                );
                cx.sign_http(request, Box::new(Refused));
                return;
            }
            self.socket = Some(cx.connect("wss://venue.test/private".to_owned()));
            if self.exhaust {
                cx.set_timer(1.);
            }
        }
        fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
        fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
        fn on_message(&mut self, _: &mut Cx<'_, ()>, socket: SocketId, _: &str) {
            if self.socket == Some(socket) {
                self.received += 1;
            }
        }
        fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
        fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
        fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
        fn on_sign_error(&mut self, cx: &mut Cx<'_, ()>, _: (), _: VenueError) {
            use std::panic::{AssertUnwindSafe, catch_unwind};
            cx.request(
                Method::Get,
                "https://venue.test/recover".into(),
                vec![],
                None,
                (),
            );
            self.caught = catch_unwind(AssertUnwindSafe(|| cx.set_timer(1.))).is_err();
        }
        fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
        fn book(&self, _: &str) -> Option<&Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }

    #[test]
    fn retired_socket_ingress_cannot_alias_after_crossing_old_counter_boundary() {
        let mut engine = crate::test_support::seeded_engine(Probe::default());
        engine.start(0.);
        let retired = engine.venue.socket.unwrap();
        engine.drain_actions().for_each(drop);
        // Simulate the long-lived allocation count without a load test.
        engine.io.core.next_id =
            crate::resource_ids::ResourceIds::starting_at(u64::from(u32::MAX - 3));
        let mut cx = Cx {
            now: 1.,
            store: &mut engine.store,
            io: &mut engine.io,
        };
        cx.close(retired);
        for _ in 0..4 {
            cx.set_timer(1.);
        }
        engine.venue.socket = Some(cx.connect("wss://venue.test/private".to_owned()));
        assert_ne!(engine.venue.socket, Some(retired));
        engine.handle(
            2.,
            Event::WsMessage {
                socket: retired,
                text: "old",
            },
        );
        assert_eq!(engine.venue.received, 0);
        engine.handle(
            2.,
            Event::WsMessage {
                socket: engine.venue.socket.unwrap(),
                text: "new",
            },
        );
        assert_eq!(engine.venue.received, 1);
    }

    #[test]
    fn fatal_legacy_exhaustion_prevents_partial_callback_actions_from_draining() {
        use std::panic::{AssertUnwindSafe, catch_unwind};
        let mut engine = crate::test_support::seeded_engine(Probe {
            exhaust: true,
            ..Probe::default()
        });
        engine.io.core.next_id = crate::resource_ids::ResourceIds::with_ceiling(1);
        assert!(catch_unwind(AssertUnwindSafe(|| engine.start(0.))).is_err());
        assert!(engine.failed());
        assert!(
            !engine.io.core.actions.is_empty(),
            "callback queued an action before exhaustion"
        );
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                engine.drain_actions().for_each(drop);
            }))
            .is_err()
        );
        assert!(
            catch_unwind(AssertUnwindSafe(|| engine.handle(
                1.,
                Event::WsMessage {
                    socket: SocketId(1),
                    text: "stale"
                }
            )))
            .is_err()
        );
        assert_eq!(engine.venue.received, 0);
    }

    #[test]
    fn finite_poll_reserves_logical_and_physical_ids_before_registration() {
        use crate::resource_ids::{MAX_RESOURCE_ID, ResourceIds};
        use std::panic::{AssertUnwindSafe, catch_unwind};
        let mut polls = Polls::default();
        let mut ids = ResourceIds::starting_at(MAX_RESOURCE_ID);
        let mut timers = crate::IdMap::default();
        let mut actions = Vec::new();
        let mut sink = TimerSink {
            next_id: &mut ids,
            timers: &mut timers,
            actions: &mut actions,
        };
        assert!(
            catch_unwind(AssertUnwindSafe(|| polls.register(
                0.,
                1_000.,
                PollTimerBounds::default(),
                &mut sink
            )))
            .is_err()
        );
        assert!(polls.entries.is_empty());
        assert!(timers.is_empty() && actions.is_empty());
        assert!(ids.failed());
    }

    #[test]
    fn paused_poll_uses_only_its_last_available_logical_id() {
        use crate::resource_ids::{MAX_RESOURCE_ID, ResourceIds};
        let mut polls = Polls {
            scale: Some(f64::INFINITY),
            ..Polls::default()
        };
        let mut ids = ResourceIds::starting_at(MAX_RESOURCE_ID);
        let mut timers = crate::IdMap::default();
        let mut actions = Vec::new();
        let mut sink = TimerSink {
            next_id: &mut ids,
            timers: &mut timers,
            actions: &mut actions,
        };
        assert_eq!(
            polls.register(0., 1_000., PollTimerBounds::default(), &mut sink),
            TimerId(MAX_RESOURCE_ID)
        );
        assert_eq!(polls.entries.len(), 1);
        assert!(timers.is_empty() && actions.is_empty());
        assert!(!ids.failed());
        assert!(!ids.available(1));
    }

    fn assert_unsafe_batch_retires(unsafe_action: Action) {
        use std::panic::{AssertUnwindSafe, catch_unwind};
        let mut engine = crate::test_support::seeded_engine(Probe::default());
        // A valid first packet must not escape before the unsafe later action.
        engine.io.core.actions.push(Action::Http(HttpRequest {
            id: RequestId(4),
            method: Method::Get,
            route: None,
            timeout_ms: None,
            url: "https://venue.test/read".into(),
            headers: vec![],
            signer_headers: vec![],
            body: None,
        }));
        engine.io.core.actions.push(unsafe_action);
        let mut exposed = 0;
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                for _ in engine.drain_actions() {
                    exposed += 1;
                }
            }))
            .is_err()
        );
        assert_eq!(
            exposed, 0,
            "the entire batch must be checked before yielding its first action"
        );
        assert!(
            engine.failed(),
            "unsafe output must use the sticky terminal path"
        );
    }

    #[test]
    fn unsafe_timer_id_retires_whole_batch_before_any_packet_is_exposed() {
        assert_unsafe_batch_retires(Action::SetTimer {
            timer: TimerId(crate::resource_ids::MAX_RESOURCE_ID + 1),
            at_ms: 1.,
        });
    }

    #[test]
    fn unsafe_held_outbound_id_is_validated_before_dispatch_can_discard_it() {
        assert_unsafe_batch_retires(Action::SignedOutbound {
            attempt: RequestId(1),
            outbound: Box::new(Action::WsSend {
                socket: SocketId(crate::resource_ids::MAX_RESOURCE_ID + 1),
                text: "".into(),
            }),
        });
    }

    #[test]
    fn unsafe_managed_cancel_signing_id_uses_the_same_terminal_boundary() {
        assert_unsafe_batch_retires(Action::ManagedHttp(Box::new(
            crate::request_context::HttpAction::Cancel {
                preparation: crate::request_context::Preparation::from_wire("p1:1:1:0").unwrap(),
                signing: Some(RequestId(crate::resource_ids::MAX_RESOURCE_ID + 1)),
            },
        )));
    }

    #[test]
    fn callback_catching_allocation_panic_cannot_publish_its_normal_return_actions() {
        use crate::resource_ids::{MAX_RESOURCE_ID, ResourceIds};
        use std::panic::{AssertUnwindSafe, catch_unwind};
        let mut engine = crate::test_support::seeded_engine(Probe {
            catch_sign_failure: true,
            ..Probe::default()
        });
        engine.start(0.);
        // A failure callback may queue one valid final request, then catch an
        // exhausted timer allocation itself instead of propagating the panic.
        engine.io.core.next_id = ResourceIds::starting_at(MAX_RESOURCE_ID);
        let mut exposed = 0;
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                for _ in engine.drain_actions() {
                    exposed += 1;
                }
            }))
            .is_err()
        );
        assert!(
            engine.venue.caught,
            "the callback returned normally after catching exhaustion"
        );
        assert!(engine.failed());
        assert_eq!(exposed, 0);
    }
}

impl TimerSink<'_> {
    fn clear(&mut self, timer: TimerId) {
        if self.timers.remove(&timer).is_some() {
            self.actions.push(Action::ClearTimer { timer });
        }
    }
}

/// A poll's own cadence times the host scale. A slowdown never exceeds the
/// host's absolute ceiling unless the native cadence is already slower.
fn poll_interval_for(default_ms: f64, scale: Option<f64>, ceiling_ms: Option<f64>) -> f64 {
    let scale = scale.unwrap_or(1.);
    // Only an explicit infinite scale or native cadence pauses; a huge finite
    // scale overflowing the product is still finite and takes the cap below.
    if scale == f64::INFINITY || default_ms == f64::INFINITY {
        return f64::INFINITY;
    }
    let interval = (default_ms * scale).min(f64::MAX);
    let interval = match ceiling_ms {
        Some(ceiling) if interval.is_finite() && ceiling.is_finite() => {
            interval.min(default_ms.max(ceiling))
        }
        _ => interval,
    };
    // Host policy cannot flood a venue or overflow a timer; infinity pauses.
    if interval.is_finite() {
        interval.clamp(
            default_ms.min(POLL_POLICY_FLOOR_MS),
            default_ms.max(POLL_POLICY_CAP_MS),
        )
    } else {
        interval
    }
}

impl Polls {
    fn interval(&self, default_ms: f64) -> f64 {
        poll_interval_for(default_ms, self.scale, self.ceiling_ms)
    }

    fn current_interval(&self, logical: TimerId) -> Option<f64> {
        let poll = self.entries.iter().find(|poll| poll.logical == logical)?;
        Some(
            self.interval(poll.default_interval_ms)
                .max(poll.bounds.minimum_interval_ms),
        )
    }

    fn deadline(&self, now: f64, interval: f64, bounds: PollTimerBounds) -> Option<f64> {
        if !interval.is_finite()
            || interval <= 0.
            || !bounds.minimum_interval_ms.is_finite()
            || bounds.minimum_interval_ms < 0.
            || bounds.not_before_ms.is_nan()
        {
            return None;
        }
        let at = (now + interval.max(bounds.minimum_interval_ms)).max(bounds.not_before_ms);
        at.is_finite().then_some(at)
    }

    fn arm(&mut self, now: f64, index: usize, sink: &mut TimerSink<'_>) {
        let poll = &self.entries[index];
        let Some(at_ms) = self.deadline(now, self.interval(poll.default_interval_ms), poll.bounds)
        else {
            return;
        };
        let physical = TimerId(next_engine_id(sink.next_id));
        self.arm_reserved(index, physical, at_ms, sink);
    }

    fn arm_reserved(
        &mut self,
        index: usize,
        physical: TimerId,
        at_ms: f64,
        sink: &mut TimerSink<'_>,
    ) {
        self.entries[index].physical = Some(physical);
        sink.timers.insert(physical, ());
        sink.actions.push(Action::SetTimer {
            timer: physical,
            at_ms,
        });
    }

    fn register(
        &mut self,
        now: f64,
        default_interval_ms: f64,
        bounds: PollTimerBounds,
        sink: &mut TimerSink<'_>,
    ) -> TimerId {
        let deadline = self.deadline(now, self.interval(default_interval_ms), bounds);
        let ids = sink
            .next_id
            .reserve_or_fatal(if deadline.is_some() { 2 } else { 1 });
        let logical = TimerId(ids.start);
        self.entries.push(PollTimer {
            logical,
            default_interval_ms,
            bounds,
            physical: None,
        });
        if let Some(at_ms) = deadline {
            self.arm_reserved(self.entries.len() - 1, TimerId(ids.start + 1), at_ms, sink);
        }
        logical
    }

    /// Removes a registration; false when `logical` is not a poll.
    fn clear(&mut self, logical: TimerId, sink: &mut TimerSink<'_>) -> bool {
        let Some(index) = self.entries.iter().position(|poll| poll.logical == logical) else {
            return false;
        };
        if let Some(physical) = self.entries.remove(index).physical {
            sink.clear(physical);
        }
        true
    }

    /// A fired physical timer ends its one-shot registration; returns the
    /// logical ID the venue knows.
    fn fired(&mut self, physical: TimerId) -> Option<TimerId> {
        let index = self
            .entries
            .iter()
            .position(|poll| poll.physical == Some(physical))?;
        Some(self.entries.remove(index).logical)
    }

    fn set_policy(
        &mut self,
        now: f64,
        scale: Option<f64>,
        ceiling_ms: Option<f64>,
        sink: &mut TimerSink<'_>,
    ) {
        let (old_scale, old_ceiling) = (self.scale, self.ceiling_ms);
        if (old_scale, old_ceiling) == (scale, ceiling_ms) {
            return;
        }
        self.scale = scale;
        self.ceiling_ms = ceiling_ms;
        // Registration order keeps action ordering deterministic.
        for index in 0..self.entries.len() {
            let poll = &self.entries[index];
            let old_interval = poll_interval_for(poll.default_interval_ms, old_scale, old_ceiling);
            let new_interval = poll_interval_for(poll.default_interval_ms, scale, ceiling_ms);
            if old_interval.is_finite()
                && old_interval > 0.
                && new_interval.is_finite()
                && old_interval.max(poll.bounds.minimum_interval_ms)
                    == new_interval.max(poll.bounds.minimum_interval_ms)
            {
                // An equivalent cadence must not postpone an already due poll.
                continue;
            }
            if let Some(physical) = self.entries[index].physical.take() {
                sink.clear(physical);
            }
            self.arm(now, index, sink);
        }
    }
}

#[cfg(feature = "managed-sockets")]
#[path = "engine_socket.rs"]
mod managed_sockets;

/// An engine-held action queue: the undrained queue, or the transient
/// dispatch queue. Its actions may hold a Bearer or signed packet, so the
/// engine wipes what it still holds on drop, unwinding included. A drained
/// action is the host's to dispose of, as signer headers are.
#[cfg(feature = "session-bearer")]
struct Wiping<C>(C)
where
    for<'a> &'a mut C: IntoIterator<Item = &'a mut Action>;

#[cfg(feature = "session-bearer")]
type Actions = Wiping<Vec<Action>>;
/// Without `session-bearer` no queued packet carries a Session Bearer.
#[cfg(not(feature = "session-bearer"))]
type Actions = Vec<Action>;

#[cfg(feature = "session-bearer")]
impl Default for Actions {
    fn default() -> Self {
        Self(Vec::new())
    }
}

#[cfg(feature = "session-bearer")]
impl<C> std::ops::Deref for Wiping<C>
where
    for<'a> &'a mut C: IntoIterator<Item = &'a mut Action>,
{
    type Target = C;
    fn deref(&self) -> &C {
        &self.0
    }
}

#[cfg(feature = "session-bearer")]
impl<C> std::ops::DerefMut for Wiping<C>
where
    for<'a> &'a mut C: IntoIterator<Item = &'a mut Action>,
{
    fn deref_mut(&mut self) -> &mut C {
        &mut self.0
    }
}

#[cfg(feature = "session-bearer")]
impl<C> Drop for Wiping<C>
where
    for<'a> &'a mut C: IntoIterator<Item = &'a mut Action>,
{
    fn drop(&mut self) {
        for action in &mut self.0 {
            crate::signing_dispatch::wipe(action);
        }
    }
}

/// Every Io field that does not name the venue's pending type, so engine
/// code that only touches these can be shared across venues.
struct IoCore {
    #[cfg(feature = "address-nonce")]
    address_setup: address_nonce_setup::AddressNonceSetup,
    nonce_unscheduled: bool,
    #[cfg(feature = "dispatch-nonce")]
    nonce_enabled: bool,
    /// `V::managed_sockets()`, copied by `Engine::new` for registration in `Cx`.
    #[cfg(feature = "managed-sockets")]
    managed_sockets_declared: bool,
    /// `V::http_mutation_barriers()`, copied by `Engine::new` for admission in `Cx`.
    #[cfg(feature = "http-mutation-barriers")]
    mutation_barriers_declared: bool,
    request_contexts: request_context_registry::ContextRegistry,
    http_delivery: Option<request_context_registry::Delivery>,
    #[cfg(feature = "http-plans")]
    http_history_failure: Option<crate::request_context::Attempt>,
    // Typed cause of the HttpFailed being delivered: Network from the host,
    // Timeout when the engine expires a released request at its deadline.
    http_failure_kind: ErrorKind,
    // Status and headers of the managed write reply being delivered.
    http_write_reply: Option<(u16, String)>,
    #[cfg(feature = "repair-mutations")]
    repairs: repair_core::Repairs,
    #[cfg(all(feature = "serial-plans", feature = "session-bearer"))]
    http_status_exempt: Option<RequestId>,
    // Reconcile reads the engine owes the venue, one per uncertain seal: the
    // symbols (`None` = all) and the earliest time to ask.
    #[cfg(feature = "order-locks")]
    reconcile_due: Vec<(Option<Vec<String>>, f64)>,
    /// An opted-in placement fence re-asks its reconcile read until a fresh read
    /// clears it: the next re-ask and how many have run (bounded backoff).
    #[cfg(feature = "order-locks")]
    fence_reread: Option<(f64, u32)>,
    /// The `reconcile_uncertain` call in progress is a backoff re-ask.
    #[cfg(feature = "order-locks")]
    reconcile_reask: bool,
    command_entry: Option<command_entry::Entry>,
    /// Caller waits stamped at each external command receipt.
    command_waits: command_entry::Waits,
    #[cfg(feature = "http-plans")]
    http_plan_ready: Option<crate::request_context::Plan>,
    #[cfg(feature = "http-plans")]
    http_plan_expiring: Option<crate::request_context::Plan>,
    #[cfg(feature = "http-plans")]
    http_preflight_ready: Option<crate::request_context::Preflight>,
    http_owner_poison: bool,
    #[cfg(feature = "http-plans")]
    http_plan_poison: bool,
    managed_http: crate::IdMap<RequestId, http_lifecycle::Managed>,
    /// Managed requests bound to each Session generation's Bearer. At most a
    /// few live Sessions, so a scan beats another map type in every build.
    #[cfg(feature = "session-bearer")]
    bearer_bound: Vec<(u64, Vec<RequestId>)>,
    /// Bound preparations a Session retire fenced, failed after the callback.
    #[cfg(feature = "session-bearer")]
    bearer_fenced: Vec<crate::request_context::Preparation>,
    /// Unreleased Bearer calls parked on (generation, request), in park order.
    #[cfg(feature = "session-bearer")]
    bearer_parked: Vec<(u64, RequestId)>,
    /// Generations whose venue announced a bundle; drained after the callback.
    #[cfg(feature = "session-bearer")]
    bearer_woken: Vec<u64>,
    http_pacing: crate::request_context::SendPacing,
    http_responses: crate::request_context::ReconciliationResponses,
    http_write_revision: u64,
    entropy: Option<crate::entropy::Entropy>,
    #[cfg(feature = "order-locks")]
    order_locks: crate::order_locks::OrderLocks,
    #[cfg(feature = "order-locks")]
    active_commands: std::collections::HashSet<CommandId>,
    #[cfg(feature = "order-locks")]
    order_read_tokens: crate::IdMap<RequestId, u64>,
    #[cfg(feature = "order-locks")]
    order_response_token: Option<u64>,
    actions: Actions,
    placement_acks: crate::placement_acks::PlacementAcks,
    /// One engine timer for the earliest execution-group deadline.
    execution_timer: Option<(TimerId, f64)>,
    /// Tracked caller-wait Read roots: (deadline, command, generation).
    /// Preallocated to owner capacity so tracking never allocates.
    read_roots: Vec<(f64, CommandId, u64, TimerId)>,
    read_roots_expiring: bool,
    /// The venue's FIFO release domain, its accepted members in order, and
    /// the Bind each non-head member holds.
    #[cfg(feature = "release-domains")]
    release_domain: Option<crate::request_context::ReleaseDomain>,
    #[cfg(feature = "release-domains")]
    domain_queue: std::collections::VecDeque<RequestId>,
    #[cfg(feature = "release-domains")]
    domain_held: Vec<(RequestId, release_domain::Held)>,
    /// Parked WriteAfterReads Preflights bounded by their caller's receipt:
    /// (cap, owner, its timer).
    #[cfg(feature = "write-preflight-wait")]
    write_preflight_waits: Vec<(f64, crate::request_context::Preflight, TimerId)>,
    /// Action capacity right after the sweep's reservation, for tests.
    #[cfg(test)]
    read_root_sweep_capacity: Option<usize>,
    public_feed_policy: PublicFeedPolicy,
    private_feed_policy: crate::signing_policy::PrivateFeedPolicy,
    /// Attempts a Session resume selected with no delivery in this event: the
    /// resume is their issue custody until the outer Plan flush. Reserved at
    /// park, so a resume never allocates.
    #[cfg(feature = "http-plans")]
    session_resumed: Vec<RequestId>,
    signing: crate::signing_dispatch::Dispatch,
    protocol_clock: crate::protocol_clock::ProtocolClock,
    signer: Option<crate::signing::Provider>,
    #[cfg(feature = "nonce-handoff")]
    nonce_handoff_issuer: Option<std::sync::Arc<()>>,
    /// Trusted-construction refs per slot, used only for capability admission.
    identities: crate::signing::Identities,
    /// Algorithms a host-fulfilled (Action::Sign) signer declared at install.
    host_algorithms: Option<Vec<crate::signing::Algorithm>>,
    dispatch_now: f64,
    signing_admission: Vec<(Action, crate::signing::SignError)>,
    signing_latch: SigningLatch,
    streams: crate::IdMap<RequestId, (ArrayAt, ItemCheck)>,
    incoming: crate::IdMap<RequestId, Incoming>,
    timers: crate::IdMap<TimerId, ()>,
    polls: Polls,
    next_id: crate::resource_ids::ResourceIds,
    /// Retained symbol buffers; only the active prefix belongs to the next frame.
    changed_books: Vec<String>,
    changed_books_active: usize,
    candles: Vec<(String, Interval, Candle)>,
    fills: Vec<(Fill, Option<FillFinancials>)>,
    frame: Vec<u8>,
    snapshot: Vec<u8>,
    min_level: Level,
    health: Health,
    sockets: SocketRoles,
    /// One rejection often arrives as an auth-kind failure plus `deactivate`.
    auth_failure_counted: bool,
    /// Each retried read's latest Error time and digit-normalised text, by call site.
    retried_failures: HashMap<&'static str, (f64, String)>,
    #[cfg(feature = "managed-sockets")]
    socket_driver: crate::socket_driver::Driver,
    #[cfg(feature = "managed-sockets")]
    socket_message: Option<(crate::socket_lifecycle::Epoch, f64)>,
    #[cfg(feature = "managed-sockets")]
    managed_sockets: bool,
    /// `V::socket_capabilities()`, copied once by `Engine::new` for admission in
    /// `Cx`, which does not know the venue type. Never reassigned.
    #[cfg(feature = "managed-sockets")]
    socket_capabilities: crate::socket_lifecycle::SocketCapabilities,
    #[cfg(feature = "managed-sockets")]
    socket_failed: bool,
    #[cfg(feature = "managed-sockets")]
    socket_effects: std::collections::VecDeque<crate::socket_driver::Effect>,
    #[cfg(feature = "managed-sockets")]
    socket_proposal_bytes: usize,
    #[cfg(feature = "managed-sockets")]
    socket_proposal_next: u64,
    #[cfg(feature = "managed-sockets")]
    socket_after: std::collections::VecDeque<(crate::socket_lifecycle::Epoch, String)>,
    #[cfg(feature = "managed-sockets")]
    socket_continuation_bytes: usize,
}

impl IoCore {
    /// A trusted local seal, read only under the provider that sealed it.
    fn verified_public_key(
        &self,
        slot: CredentialSlot,
        algorithm: crate::signing::Algorithm,
    ) -> Result<[u8; 32], crate::signing::SignError> {
        let identity = match slot {
            CredentialSlot::Primary => self.identities.primary.as_ref(),
            CredentialSlot::Secondary => self.identities.secondary.as_ref(),
        }
        .ok_or(crate::signing::SignError::MissingKey)?;
        let installed = self
            .signer
            .as_deref()
            .and_then(|signer| signer.incarnation());
        identity.public_key_sealed_by(algorithm, installed)
    }
}

struct Io<P> {
    core: IoCore,
    /// Managed close, installed by `Engine::new` only for venues that declare
    /// managed sockets, so other venues never reference the managed retire path.
    #[cfg(feature = "managed-sockets")]
    managed_close: Option<for<'a> fn(&mut Cx<'a, P>, SocketId)>,
    pending: crate::IdMap<RequestId, PendingHttp<P>>,
    session_waits: request_context_registry::session::Waits<P>,
    #[cfg(feature = "managed-sockets")]
    socket_proposals: std::collections::VecDeque<managed_sockets::Proposal<P>>,
    #[cfg(feature = "managed-sockets")]
    socket_continuations: std::collections::VecDeque<managed_sockets::Continuation<P>>,
}

impl Default for IoCore {
    // One caller per venue engine: inline it, so a family pays no extra call or copy.
    #[inline]
    fn default() -> Self {
        Self {
            #[cfg(feature = "address-nonce")]
            address_setup: Default::default(),
            nonce_unscheduled: false,
            #[cfg(feature = "dispatch-nonce")]
            nonce_enabled: false,
            #[cfg(feature = "managed-sockets")]
            managed_sockets_declared: false,
            #[cfg(feature = "http-mutation-barriers")]
            mutation_barriers_declared: false,
            request_contexts: Default::default(),
            http_delivery: None,
            #[cfg(feature = "http-plans")]
            http_history_failure: None,
            http_failure_kind: ErrorKind::Network,
            http_write_reply: None,
            #[cfg(feature = "repair-mutations")]
            repairs: Default::default(),
            #[cfg(all(feature = "serial-plans", feature = "session-bearer"))]
            http_status_exempt: None,
            #[cfg(feature = "order-locks")]
            reconcile_due: Vec::new(),
            #[cfg(feature = "order-locks")]
            fence_reread: None,
            #[cfg(feature = "order-locks")]
            reconcile_reask: false,
            command_entry: None,
            command_waits: Default::default(),
            #[cfg(feature = "http-plans")]
            http_plan_ready: None,
            #[cfg(feature = "http-plans")]
            http_plan_expiring: None,
            #[cfg(feature = "http-plans")]
            http_preflight_ready: None,
            http_owner_poison: false,
            #[cfg(feature = "http-plans")]
            http_plan_poison: false,
            http_responses: Default::default(),
            http_write_revision: 0,
            managed_http: Default::default(),
            #[cfg(feature = "session-bearer")]
            bearer_bound: Vec::new(),
            #[cfg(feature = "session-bearer")]
            bearer_fenced: Vec::new(),
            #[cfg(feature = "session-bearer")]
            bearer_parked: Vec::new(),
            #[cfg(feature = "session-bearer")]
            bearer_woken: Vec::new(),
            http_pacing: Default::default(),
            entropy: None,
            #[cfg(feature = "order-locks")]
            order_locks: crate::order_locks::OrderLocks::default(),
            #[cfg(feature = "order-locks")]
            active_commands: Default::default(),
            #[cfg(feature = "order-locks")]
            order_read_tokens: Default::default(),
            #[cfg(feature = "order-locks")]
            order_response_token: None,
            actions: Actions::default(),
            placement_acks: Default::default(),
            execution_timer: None,
            read_roots: Vec::with_capacity(256),
            #[cfg(feature = "write-preflight-wait")]
            write_preflight_waits: Vec::new(),
            read_roots_expiring: false,
            #[cfg(feature = "release-domains")]
            release_domain: None,
            #[cfg(feature = "release-domains")]
            domain_queue: std::collections::VecDeque::new(),
            #[cfg(feature = "release-domains")]
            domain_held: Vec::new(),
            #[cfg(test)]
            read_root_sweep_capacity: None,
            public_feed_policy: PublicFeedPolicy::Direct,
            private_feed_policy: Default::default(),
            #[cfg(feature = "http-plans")]
            session_resumed: Vec::new(),
            signing: Default::default(),
            protocol_clock: Default::default(),
            signer: None,
            #[cfg(feature = "nonce-handoff")]
            nonce_handoff_issuer: None,
            identities: Default::default(),
            host_algorithms: None,
            dispatch_now: 0.,
            signing_admission: Vec::new(),
            signing_latch: SigningLatch::default(),
            streams: Default::default(),
            incoming: Default::default(),
            timers: Default::default(),
            polls: Polls::default(),
            next_id: Default::default(),
            changed_books: Vec::new(),
            changed_books_active: 0,
            candles: Vec::new(),
            fills: Vec::new(),
            frame: Vec::new(),
            snapshot: Vec::new(),
            min_level: Level::Info,
            health: Health::default(),
            sockets: SocketRoles::default(),
            auth_failure_counted: false,
            retried_failures: HashMap::new(),
            #[cfg(feature = "managed-sockets")]
            socket_driver: Default::default(),
            #[cfg(feature = "managed-sockets")]
            socket_message: None,
            #[cfg(feature = "managed-sockets")]
            managed_sockets: false,
            #[cfg(feature = "managed-sockets")]
            socket_capabilities: crate::socket_lifecycle::SocketCapabilities::FULL,
            #[cfg(feature = "managed-sockets")]
            socket_failed: false,
            #[cfg(feature = "managed-sockets")]
            socket_effects: Default::default(),
            #[cfg(feature = "managed-sockets")]
            socket_proposal_bytes: 0,
            #[cfg(feature = "managed-sockets")]
            socket_proposal_next: 0,
            #[cfg(feature = "managed-sockets")]
            socket_after: Default::default(),
            #[cfg(feature = "managed-sockets")]
            socket_continuation_bytes: 0,
        }
    }
}

/// Drops a managed call's pending without naming the venue's type. The drop
/// happens synchronously, at the same point the typed removal did.
trait PendingDiscard {
    fn discard(&mut self, request: RequestId) -> bool;
}

impl<P> PendingDiscard for crate::IdMap<RequestId, PendingHttp<P>> {
    fn discard(&mut self, request: RequestId) -> bool {
        self.remove(&request).is_some()
    }
}

/// The venue-independent view of `Io<P>`. Engine code that needs only this is
/// compiled once, instead of once per venue.
struct CoreIo<'a> {
    core: &'a mut IoCore,
    pending: &'a mut dyn PendingDiscard,
}

/// The venue-independent view of `Cx<P>`; see `CoreIo`.
struct CoreCx<'a> {
    now: f64,
    #[cfg(feature = "metadata-freshness")]
    store: &'a mut Store,
    io: CoreIo<'a>,
}

impl<P> Io<P> {
    #[inline]
    fn core_io(&mut self) -> CoreIo<'_> {
        CoreIo {
            core: &mut self.core,
            pending: &mut self.pending,
        }
    }
}

impl<P> Cx<'_, P> {
    #[inline]
    fn core(&mut self) -> CoreCx<'_> {
        CoreCx {
            now: self.now,
            #[cfg(feature = "metadata-freshness")]
            store: &mut *self.store,
            io: self.io.core_io(),
        }
    }
}

/// The shared-borrow form of `CoreIo`, so venue-independent `&self` checks
/// are compiled once.
struct CoreIoRef<'a> {
    core: &'a IoCore,
}

/// The shared-borrow form of `CoreCx`; see `CoreIoRef`.
struct CoreRef<'a> {
    #[allow(dead_code)]
    now: f64,
    io: CoreIoRef<'a>,
}

impl<P> Io<P> {
    #[inline]
    fn core_ref(&self) -> CoreIoRef<'_> {
        CoreIoRef { core: &self.core }
    }
}

impl<P> Cx<'_, P> {
    /// The public key trusted local setup sealed for `slot`, read only while
    /// the provider that sealed it is the one installed.
    pub fn verified_public_key(
        &self,
        slot: CredentialSlot,
        algorithm: crate::signing::Algorithm,
    ) -> Result<[u8; 32], crate::signing::SignError> {
        self.io.core.verified_public_key(slot, algorithm)
    }
}

impl<P> Cx<'_, P> {
    #[inline]
    fn core_ref(&self) -> CoreRef<'_> {
        CoreRef {
            now: self.now,
            io: self.io.core_ref(),
        }
    }
}

impl CoreIo<'_> {
    #[inline]
    fn core_ref(&self) -> CoreIoRef<'_> {
        CoreIoRef { core: &*self.core }
    }
}

impl CoreCx<'_> {
    #[inline]
    fn core_ref(&self) -> CoreRef<'_> {
        CoreRef {
            now: self.now,
            io: self.io.core_ref(),
        }
    }
}

impl<P> Default for Io<P> {
    fn default() -> Self {
        Self {
            core: IoCore::default(),
            #[cfg(feature = "managed-sockets")]
            managed_close: None,
            pending: Default::default(),
            session_waits: Default::default(),
            #[cfg(feature = "managed-sockets")]
            socket_proposals: Default::default(),
            #[cfg(feature = "managed-sockets")]
            socket_continuations: Default::default(),
        }
    }
}

/// The delivery that may authorize or forbid a command's completion. A
/// session delivery is no command's, so its callbacks settle like a
/// delivery-less (socket or timer) callback; it does not bind waiters.
fn completion_delivery(
    delivery: &Option<request_context_registry::Delivery>,
) -> Option<&request_context_registry::Delivery> {
    delivery.as_ref().filter(|delivery| {
        !matches!(
            delivery.context.owner(),
            crate::request_context::Owner::Session { .. }
        )
    })
}

impl<P> Io<P> {
    #[cfg(feature = "order-locks")]
    #[inline]
    // Only some feature combinations call this.
    #[cfg_attr(feature = "order-locks", allow(dead_code))]
    fn queue_reconcile(&mut self, symbols: Option<Vec<String>>, at: f64) {
        self.core_io().queue_reconcile(symbols, at)
    }

    fn managed_socket_active(&self) -> bool {
        #[cfg(feature = "managed-sockets")]
        {
            self.core.managed_sockets
        }
        #[cfg(not(feature = "managed-sockets"))]
        {
            false
        }
    }
    #[inline]
    fn next(&mut self) -> u64 {
        self.core_io().next()
    }

    #[inline]
    fn reserve_ids(&mut self, count: u64) -> Option<std::ops::Range<u64>> {
        self.core_io().reserve_ids(count)
    }

    #[inline]
    fn assert_action_safe(&mut self, action: &Action) {
        self.core_io().assert_action_safe(action)
    }

    #[inline]
    fn assert_actions_safe(&mut self) {
        self.core_io().assert_actions_safe()
    }

    /// Splits the poll registry from the timer state it writes to.
    fn polls(&mut self) -> (&mut Polls, TimerSink<'_>) {
        (
            &mut self.core.polls,
            TimerSink {
                next_id: &mut self.core.next_id,
                timers: &mut self.core.timers,
                actions: &mut self.core.actions,
            },
        )
    }
}

impl crate::engine::CoreIo<'_> {
    /// Bounded: past the cap, coalesce into one full read that waits for the
    /// latest merged deadline, so no rate or credential wait is cut short.
    #[cfg(feature = "order-locks")]
    #[inline(never)]
    fn queue_reconcile(&mut self, symbols: Option<Vec<String>>, at: f64) {
        const RECONCILE_QUEUE: usize = 64;
        // A seal at a non-finite clock is due at the next drain.
        let at = if at.is_finite() { at } else { 0. };
        if self.core.reconcile_due.len() < RECONCILE_QUEUE {
            self.core.reconcile_due.push((symbols, at));
            return;
        }
        let latest = self
            .core
            .reconcile_due
            .iter()
            .map(|(_, at)| *at)
            .fold(at, f64::max);
        self.core.reconcile_due = vec![(None, latest)];
    }

    fn next(&mut self) -> u64 {
        next_engine_id(&mut self.core.next_id)
    }

    fn reserve_ids(&mut self, count: u64) -> Option<std::ops::Range<u64>> {
        self.core.next_id.reserve(count)
    }

    fn assert_action_safe(&mut self, action: &Action) {
        assert!(
            !self.core.next_id.failed() && !self.core.signing.failed(),
            "engine resource identities exhausted"
        );
        if !crate::resource_ids::action_ids_safe(action) {
            self.core.next_id.invalidate();
        }
    }

    fn assert_actions_safe(&mut self) {
        assert!(
            !self.core.next_id.failed() && !self.core.signing.failed(),
            "engine resource identities exhausted"
        );
        if self
            .core
            .actions
            .iter()
            .any(|a| !crate::resource_ids::action_ids_safe(a))
            || self
                .core
                .signing_admission
                .iter()
                .any(|(a, _)| !crate::resource_ids::action_ids_safe(a))
        {
            self.core.next_id.invalidate();
        }
    }
}

impl<P> Cx<'_, P> {
    #[cfg(not(feature = "managed-sockets"))]
    fn managed_send_allowed(&self, _socket: SocketId) -> bool {
        true
    }
    /// Admission check for a command that releases several signed requests:
    /// every need must be supported before the first release, so a refusal
    /// is a whole-command NotSent rather than a partial effect.
    pub fn can_sign(
        &self,
        slot: CredentialSlot,
        needs: &[crate::signing::Need],
    ) -> Result<(), crate::signing::SignError> {
        use crate::signing::{Capability, KeyUse, SignError};
        let identity = match slot {
            CredentialSlot::Primary => self.io.core.identities.primary.as_ref(),
            CredentialSlot::Secondary => self.io.core.identities.secondary.as_ref(),
        }
        .ok_or(SignError::InvalidRequest)?;
        // A malformed command is InvalidRequest before an absent signing ref is
        // MissingKey, regardless of the order of its needs.
        if needs.iter().any(
            |need| matches!(need.key, KeyUse::Extra(extra) if !identity.extra.contains_key(&extra)),
        ) {
            return Err(SignError::InvalidRequest);
        }
        let keys = needs
            .iter()
            .map(|need| match need.key {
                KeyUse::Signing => identity.signing_ref(),
                KeyUse::Extra(extra) => Ok(&identity.extra[&extra]),
            })
            .collect::<Result<Vec<_>, _>>()?;
        for (need, key) in needs.iter().zip(keys) {
            let capability = match (&self.io.core.signer, &self.io.core.host_algorithms) {
                (Some(signer), _) => signer.can_sign(key, need.algorithm),
                (None, Some(declared)) if declared.contains(&need.algorithm) => {
                    Capability::Supported
                }
                (None, Some(_)) => Capability::Unsupported,
                // No Rust provider means the host fulfils Action::Sign; undeclared is unknown.
                (None, None) => Capability::Unknown,
            };
            if capability != Capability::Supported {
                return Err(SignError::Unsupported);
            }
        }
        Ok(())
    }

    /// Calibrated venue UTC for outbound timestamps, absolute expiry and
    /// time-based nonces. Timers, deadlines and emitted data retain `self.now`.
    pub fn protocol_now(&self) -> f64 {
        self.io.core.protocol_clock.at(self.now)
    }

    /// Integer calibrated Unix milliseconds for outbound protocol fields.
    /// Scheduling time and fractional calibration are unchanged.
    pub fn protocol_stamp_ms(&self) -> f64 {
        crate::time::wire_stamp_ms(self.protocol_now())
    }

    /// Feed a server-time observation captured at this callback's raw host time.
    /// Replaces the per-engine correction; invalid observations preserve it.
    pub fn observe_protocol_time(&mut self, server_ms: f64) -> bool {
        self.io.core.protocol_clock.observe(self.now, server_ms)
    }

    pub fn public_feed_policy(&self) -> PublicFeedPolicy {
        self.io.core.public_feed_policy
    }

    pub fn private_feed_policy(&self) -> crate::signing_policy::PrivateFeedPolicy {
        self.io.core.private_feed_policy
    }

    /// `SocketId(0)` stands for a connect the engine suppressed.
    fn inert_socket(&self, socket: SocketId) -> bool {
        socket.0 == 0
            && (self.io.core.public_feed_policy != PublicFeedPolicy::Direct
                || self.io.core.signing_latch.roles != 0)
    }

    pub fn connect(&mut self, spec: impl Into<SocketSpec>) -> SocketId {
        let spec = spec.into();
        if self.io.managed_socket_active()
            || spec.channel == Some(SocketChannel::Public)
                && self.io.core.public_feed_policy != PublicFeedPolicy::Direct
            || self.io.core.signing_latch.blocks(spec.channel.as_ref())
        {
            return SocketId(0);
        }
        let socket = SocketId(self.io.next());
        self.queue_socket_transport(socket, spec);
        socket
    }
    #[inline]
    fn queue_socket_transport(&mut self, socket: SocketId, spec: SocketSpec) {
        self.core().queue_socket_transport(socket, spec)
    }

    /// Create an unsigned upgrade and stage it through the shared signing queue.
    /// Public-role suppression and socket metadata are identical to `connect`.
    pub fn connect_signed(
        &mut self,
        spec: impl Into<SocketSpec>,
        step: Box<dyn crate::SignStep>,
    ) -> SocketId {
        let socket = self.connect(spec);
        if socket.0 == 0 {
            return socket;
        }
        self.sign_socket_upgrade(socket, step);
        socket
    }
    fn sign_socket_upgrade(&mut self, socket: SocketId, step: Box<dyn crate::SignStep>) {
        let index = self
            .io
            .core
            .actions
            .iter()
            .position(
                |action| matches!(action, Action::WsConnect { socket: id, .. } if *id == socket),
            )
            .expect("connect queued its upgrade");
        let outbound = self.io.core.actions.remove(index);
        self.stage_signed(index, outbound, step);
    }

    pub fn send(&mut self, socket: SocketId, text: impl Into<String>) {
        if !self.managed_send_allowed(socket) {
            return;
        }
        if self.inert_socket(socket) {
            return;
        }
        self.io.opaque_entry_activity();
        let action = Action::WsSend {
            socket,
            text: text.into(),
        };
        if self.io.core.signing.has_socket(socket) {
            if self.io.core.signing.hold(action.clone()).is_err() {
                self.io
                    .core
                    .signing_admission
                    .push((action, crate::signing::SignError::Unavailable));
            }
        } else {
            self.io.core.actions.push(action);
        }
    }

    pub fn close(&mut self, socket: SocketId) {
        #[cfg(feature = "managed-sockets")]
        if self.io.managed_socket_active()
            && let Some(close) = self.io.managed_close
        {
            close(self, socket);
            return;
        }
        self.close_socket_transport(socket);
    }

    #[cfg(feature = "managed-sockets")]
    fn managed_close(cx: &mut Cx<'_, P>, socket: SocketId) {
        cx.io.opaque_entry_activity();
        if let Some(epoch) = cx.io.core.socket_driver.state.socket_epoch(socket) {
            cx.retire_socket(epoch.feed(), crate::socket_lifecycle::RetireReason::Stopped);
        }
    }

    /// Physical close and signing cancellation only. Managed lifecycle effects
    /// use this after revoking the epoch, before native invalidation; public
    /// close can then enter retirement without recursively invoking itself.
    fn close_socket_transport(&mut self, socket: SocketId) {
        if self.inert_socket(socket) {
            return;
        }
        self.io.opaque_entry_activity();
        for timer in self.io.core.signing.close_socket(socket) {
            if timer.0 != 0 {
                self.io.core.actions.push(Action::ClearTimer { timer });
            }
        }
        // The host drops engine-initiated closes without reporting them back.
        // They are deliberate, so they never make the next connect a reconnect.
        if self.io.core.sockets.closed(socket, false) {
            self.io.core.health.count(log_code::WS_CLOSED);
        }
        self.io.core.actions.push(Action::WsClose { socket });
    }

    /// Issue a request; `pending` comes back with its response.
    pub fn request(
        &mut self,
        method: Method,
        url: String,
        headers: Vec<(&'static str, String)>,
        body: Option<String>,
        pending: P,
    ) -> RequestId {
        let id = RequestId(self.io.next());
        #[cfg(feature = "order-locks")]
        {
            if method != Method::Get {
                self.io.core.order_locks.note_write();
            }
            self.io
                .core
                .order_read_tokens
                .insert(id, self.io.core.order_locks.token());
        }
        self.io.pending.insert(
            id,
            PendingHttp {
                value: pending,
                auth: None,
            },
        );
        self.io.core.actions.push(Action::Http(HttpRequest {
            id,
            method,
            route: None,
            timeout_ms: None,
            url,
            headers,
            signer_headers: Vec::new(),
            body,
        }));
        id
    }

    /// Tag only credential-bearing private account reads, after signer selection.
    pub fn tag_account_read(
        &mut self,
        request: RequestId,
        credential: CredentialSlot,
        scope: AccountReadScope,
    ) {
        if let Some(pending) = self.io.pending.get_mut(&request) {
            pending.auth = Some(ReadAuth { credential, scope });
        }
    }

    /// Issue a request whose successful body holds a large array: each element
    /// goes to [`Venue::on_item`] as it arrives, then [`Venue::on_response`]
    /// receives the envelope with its array emptied. Error bodies arrive whole.
    pub fn request_streamed(
        &mut self,
        method: Method,
        url: String,
        headers: Vec<(&'static str, String)>,
        at: ArrayAt,
        pending: P,
    ) -> RequestId {
        self.request_streamed_body(method, url, headers, None, at, pending)
    }

    /// Attach a venue's unsigned HTTP recipe. Preparation happens at dispatch.
    pub fn sign_http(&mut self, request: RequestId, step: Box<dyn crate::SignStep>) {
        let Some(index) = self
            .io
            .core
            .actions
            .iter()
            .position(|a| matches!(a, Action::Http(http) if http.id == request))
        else {
            return;
        };
        let outbound = self.io.core.actions.remove(index);
        self.stage_signed(index, outbound, step);
    }

    /// WS authentication uses the same deadline/freshness/generation service.
    pub fn send_signed(&mut self, socket: SocketId, text: String, step: Box<dyn crate::SignStep>) {
        if !self.managed_send_allowed(socket) {
            return;
        }
        let index = self.io.core.actions.len();
        self.stage_signed(index, Action::WsSend { socket, text }, step);
    }

    #[inline]
    fn stage_signed(&mut self, index: usize, outbound: Action, step: Box<dyn crate::SignStep>) {
        self.core().stage_signed(index, outbound, step)
    }

    /// Stream a POSTed read while retaining its typed request body.
    pub fn request_streamed_body(
        &mut self,
        method: Method,
        url: String,
        headers: Vec<(&'static str, String)>,
        body: Option<String>,
        at: ArrayAt,
        pending: P,
    ) -> RequestId {
        let id = self.request(method, url, headers, body, pending);
        self.io.core.streams.insert(id, (at, ItemCheck::Splitter));
        id
    }

    /// Stream the response of an already-issued request, including signed POST reads.
    pub fn stream_response(&mut self, request: RequestId, at: ArrayAt) {
        self.io
            .core
            .streams
            .insert(request, (at, ItemCheck::Splitter));
    }

    /// Skip the splitter's per-element syntax pass for a streamed request
    /// whose venue deserializes every element with serde_json and fails the
    /// request on any error.
    pub fn venue_parses_items(&mut self, request: RequestId) {
        if let Some((_, check)) = self.io.core.streams.get_mut(&request) {
            *check = ItemCheck::Venue;
        }
    }

    /// Set host policy on an outbound action before the host drains it.
    pub fn route_request(&mut self, id: RequestId, route: RequestRoute) {
        if let Some(request) = self.io.core.signing.request_mut(id) {
            request.route = Some(route);
            return;
        }
        if let Some(request) = self
            .io
            .core
            .actions
            .iter_mut()
            .rev()
            .find_map(|action| match action {
                Action::Http(request) if request.id == id => Some(request),
                _ => None,
            })
        {
            request.route = Some(route);
        }
    }

    /// Bound the whole host operation, including route setup and response body.
    pub fn request_timeout(&mut self, id: RequestId, timeout_ms: u32) {
        if let Some((timer, at_ms)) = self.io.core.signing.timeout_request(id, timeout_ms) {
            if timer.0 != 0 {
                self.io
                    .core
                    .actions
                    .retain(|a| !matches!(a,Action::SetTimer {timer:old,..} if *old==timer));
                self.io.core.actions.push(Action::SetTimer { timer, at_ms });
            }
            return;
        }
        if let Some(request) = self
            .io
            .core
            .actions
            .iter_mut()
            .rev()
            .find_map(|action| match action {
                Action::Http(request) if request.id == id => Some(request),
                _ => None,
            })
        {
            request.timeout_ms = Some(timeout_ms.max(1));
        }
    }

    pub fn fill_entropy(&self, bytes: &mut [u8]) -> Result<(), String> {
        self.io
            .core
            .entropy
            .as_ref()
            .ok_or("Engine entropy providers are required")?
            .identity(bytes)
    }
    pub fn fill_signing_entropy(&self, bytes: &mut [u8]) -> Result<(), String> {
        self.io
            .core
            .entropy
            .as_ref()
            .ok_or("Engine entropy providers are required")?
            .signing(bytes)
    }

    /// Spreads a failed-request retry delay into `[0.75, 1.0] * delay_ms` so
    /// clients do not retry in lockstep. Apply rate-limit minimums afterwards.
    /// Non-finite and non-positive delays pass through unchanged.
    pub fn jittered(&mut self, delay_ms: f64) -> f64 {
        if !(delay_ms.is_finite() && delay_ms > 0.0) {
            return delay_ms;
        }
        let mut bytes = [0; 8];
        if self.fill_entropy(&mut bytes).is_err() {
            return delay_ms;
        }
        crate::backoff::jitter(delay_ms, u64::from_le_bytes(bytes))
    }

    pub fn set_timer(&mut self, delay_ms: f64) -> TimerId {
        let timer = TimerId(self.io.next());
        self.io.core.timers.insert(timer, ());
        self.io.core.actions.push(Action::SetTimer {
            timer,
            at_ms: self.now + delay_ms,
        });
        timer
    }

    /// Register one periodic-poll wakeup. The configured positive interval
    /// replaces this fallback cadence; Infinity pauses without losing it.
    /// Rearm from the venue callback or read completion for the next cycle.
    pub fn set_poll_timer(&mut self, default_interval_ms: f64) -> TimerId {
        self.set_poll_timer_with_bounds(default_interval_ms, PollTimerBounds::default())
    }
    /// The interval a registered poll is armed with under the current host
    /// policy (bounded, and infinite while paused).
    pub fn poll_interval(&self, timer: TimerId) -> Option<f64> {
        self.io.core.polls.current_interval(timer)
    }

    /// Configuration changes rearm from change-time, honoring both the recurring
    /// minimum and the unchanged absolute deadline. Backoff should therefore be
    /// an absolute `not_before_ms`, not the fallback cadence. Invalid bounds or
    /// nonfinite due times stay parked rather than reaching a host timer.
    pub fn set_poll_timer_with_bounds(
        &mut self,
        default_interval_ms: f64,
        bounds: PollTimerBounds,
    ) -> TimerId {
        let now = self.now;
        let (polls, mut sink) = self.io.polls();
        polls.register(now, default_interval_ms, bounds, &mut sink)
    }

    pub fn clear_timer(&mut self, timer: TimerId) {
        let (polls, mut sink) = self.io.polls();
        if !polls.clear(timer, &mut sink) {
            sink.clear(timer);
        }
    }

    pub fn book_changed(&mut self, symbol: &str) {
        let active = self.io.core.changed_books_active;
        if !self.io.core.changed_books[..active]
            .iter()
            .any(|s| s == symbol)
        {
            if let Some(slot) = self.io.core.changed_books.get_mut(active) {
                slot.clear();
                slot.push_str(symbol);
            } else {
                self.io.core.changed_books.push(symbol.to_owned());
            }
            self.io.core.changed_books_active += 1;
        }
    }

    /// Whether an event at `level` reaches the host; check before formatting
    /// an expensive Debug message.
    pub fn logs(&self, level: Level) -> bool {
        level >= self.io.core.min_level
    }

    #[inline]
    pub fn notice(&mut self, level: Level, code: &'static str, message: impl Into<String>) {
        self.core().notice(level, code, message)
    }

    /// A `request.failed` read the venue retries on its own schedule. A repeat
    /// of the same failure inside [`RETRIED_FAILURE_ERROR_EVERY_MS`] of the site's
    /// last Error is a warning; otherwise, and whenever the failure text changes,
    /// it is an Error. The app's auth badge and reload latch need an Error at
    /// least that often while a key stays rejected.
    pub fn retried_failure(&mut self, site: &'static str, message: impl Into<String>) {
        let message = message.into();
        let level = self.retried_failure_level(site, &message);
        self.notice(level, log_code::REQUEST_FAILED, message);
    }

    /// [`Self::retried_failure`] for a classified read failure: still
    /// `request.failed`, now carrying the error's fields ([`Self::notice_error`]).
    /// An auth kind stays here: account auth evidence is `AccountAuth`, and
    /// consumers read `auth.rejected` as account-level (bitunix `recovery.rs`).
    pub fn retried_failure_error(&mut self, site: &'static str, error: &VenueError) {
        let level = self.retried_failure_level(site, &error.message);
        self.notice_error(level, log_code::REQUEST_FAILED, error);
    }

    /// A notice for a classified failure under the caller's `code`. Like
    /// `startup.degraded`, the event carries `kind`, `venueCode`, `venueMessage`
    /// and `retryAfterMs` when the error has them. It never becomes
    /// `auth.rejected`, which counts an auth failure and slows account polling.
    pub fn notice_error(&mut self, level: Level, code: &'static str, error: &VenueError) {
        let mut fields = Vec::new();
        if let Some(kind) = error.kind {
            fields.push(("kind", kind.as_str().into()));
        }
        if let Some(venue_code) = &error.venue_code {
            fields.push(("venueCode", venue_code.as_str().into()));
        }
        if let Some(venue_message) = &error.venue_message {
            fields.push(("venueMessage", venue_message.as_str().into()));
        }
        if let Some(retry_after_ms) = error.retry_after_ms {
            fields.push(("retryAfterMs", retry_after_ms.into()));
        }
        self.event(level, code, error.message.clone(), fields);
    }

    fn retried_failure_level(&mut self, site: &'static str, message: &str) -> Level {
        let shape: String = message
            .chars()
            .map(|c| if c.is_ascii_digit() { '#' } else { c })
            .collect();
        let now = self.now;
        match self.io.core.retried_failures.get(site) {
            Some((at, last)) if *last == shape && now - at < RETRIED_FAILURE_ERROR_EVERY_MS => {
                Level::Warn
            }
            _ => {
                self.io.core.retried_failures.insert(site, (now, shape));
                Level::Error
            }
        }
    }

    #[inline]
    pub fn event(
        &mut self,
        level: Level,
        code: &'static str,
        message: impl Into<String>,
        fields: Vec<(&'static str, Value)>,
    ) {
        self.core().event(level, code, message, fields)
    }

    pub fn deactivate(&mut self) {
        count_auth_failure(&mut self.io.core, None);
        self.io
            .core
            .actions
            .push(Action::Notice(Notice::Deactivate));
    }

    /// Legacy numeric finalization. Managed commands use `done_context`.
    pub fn done(&mut self, command: CommandId, result: String) {
        let _ = self.finish_command(command, Ok(result), None);
    }

    #[inline]
    pub fn command_progress(&mut self, command: CommandId, receipts: Vec<OrderIdentityReceipt>) {
        self.core().command_progress(command, receipts)
    }

    #[inline]
    pub fn command_error(&mut self, command: CommandId, message: impl Into<String>) {
        self.core().command_error(command, message)
    }

    pub fn startup_failed(&mut self, error: impl Into<VenueError>) {
        let error = error.into();
        count_auth_failure(&mut self.io.core, Some(&error));
        self.io
            .core
            .actions
            .push(Action::Notice(Notice::StartupFailed { error }));
    }

    /// Legacy numeric finalization. Managed commands use `fail_context`.
    pub fn fail(&mut self, command: CommandId, error: impl Into<VenueError>) {
        let _ = self.finish_command(command, Err(error.into()), None);
    }

    #[inline]
    fn finish_command(
        &mut self,
        command: CommandId,
        result: Result<String, VenueError>,
        authority: Option<&crate::request_context::Context>,
    ) -> Result<(), crate::request_context::Error> {
        self.core().finish_command(command, result, authority)
    }

    #[inline]
    fn finish_sealed_command(
        &mut self,
        command: CommandId,
        success: Option<String>,
        seal: request_context_registry::Seal,
    ) -> Result<(), crate::request_context::Error> {
        self.core().finish_sealed_command(command, success, seal)
    }

    /// Every placement in this response is proven accepted or explicitly rejected.
    pub fn placement_resolved(&mut self, command: CommandId) {
        #[cfg(feature = "order-locks")]
        self.io.core.order_locks.resolved(command);
        #[cfg(not(feature = "order-locks"))]
        let _ = command;
    }

    /// True inside `reconcile_uncertain` when the engine re-asks with backoff and
    /// no write has turned uncertain since its last ask: join any read already
    /// running or owed rather than starting (or owing) another.
    pub fn reconcile_reask(&self) -> bool {
        #[cfg(feature = "order-locks")]
        return self.io.core.reconcile_reask;
        #[cfg(not(feature = "order-locks"))]
        false
    }

    /// The order-uncertainty epoch now. A read that spans several contexts (a
    /// paged census) records it before its first page and reports through
    /// `orders_reconciled_since`, so it clears only marks its first page saw.
    pub fn order_read_epoch(&self) -> u64 {
        #[cfg(feature = "order-locks")]
        return self.io.core.order_locks.token();
        #[cfg(not(feature = "order-locks"))]
        0
    }

    /// `orders_reconciled` for a read that began at `epoch` (`order_read_epoch`):
    /// a mark made after its first page was sent survives this answer.
    pub fn orders_reconciled_since(&mut self, symbol: Option<&str>, epoch: u64) {
        #[cfg(feature = "order-locks")]
        if let Some(token) = self.io.core.order_response_token {
            self.io
                .core
                .order_locks
                .reconciled(symbol, token.min(epoch));
        }
        #[cfg(not(feature = "order-locks"))]
        let _ = (symbol, epoch);
    }

    /// Call only after accepting a complete, fresh authoritative order read.
    /// A read started before an uncertain placement cannot clear its fence.
    pub fn orders_reconciled(&mut self, symbol: Option<&str>) {
        #[cfg(feature = "order-locks")]
        if let Some(token) = self.io.core.order_response_token {
            self.io.core.order_locks.reconciled(symbol, token);
        }
        #[cfg(not(feature = "order-locks"))]
        let _ = symbol;
    }

    /// Queue a live candle for the next frame; a newer update of the same
    /// candle replaces the queued one.
    pub fn candle(&mut self, symbol: &str, interval: Interval, candle: Candle) {
        let same = self
            .io
            .core
            .candles
            .iter_mut()
            .find(|(s, i, c)| s == symbol && *i == interval && c.timestamp == candle.timestamp);
        match same {
            Some(queued) => queued.2 = candle,
            None => self
                .io
                .core
                .candles
                .push((symbol.to_owned(), interval, candle)),
        }
    }

    /// Queue an execution for the next frame.
    pub fn fill(&mut self, fill: Fill) {
        self.io.core.fills.push((fill, None));
    }

    /// Preserve optional per-execution financials without inventing values for
    /// cumulative/order-derived fills. Missing/nonfinite values stay absent.
    pub fn fill_with_financials(&mut self, fill: Fill, mut financials: FillFinancials) {
        financials.realized_pnl = financials.realized_pnl.filter(|v| v.is_finite());
        financials.fee = financials.fee.filter(|v| v.is_finite());
        let financials =
            (financials.realized_pnl.is_some() || financials.fee.is_some()).then_some(financials);
        self.io.core.fills.push((fill, financials));
    }
}

impl crate::engine::CoreCx<'_> {
    fn queue_socket_transport(&mut self, socket: SocketId, spec: SocketSpec) {
        let SocketSpec {
            url,
            channel,
            context,
            headers,
            binary,
        } = spec;
        if let Some(channel) = &channel {
            self.io
                .core
                .sockets
                .connected(socket, channel, &mut self.io.core.health);
        }
        self.io.core.actions.push(Action::WsConnect {
            binary,
            socket,
            url,
            headers,
            channel,
            context,
        });
    }

    fn stage_signed(&mut self, index: usize, outbound: Action, step: Box<dyn crate::SignStep>) {
        if step.nonce_claim().is_some() {
            self.io
                .core
                .signing_admission
                .push((outbound, crate::signing::SignError::InvalidRequest));
            return;
        }
        self.io.opaque_entry_activity();
        self.io.assert_action_safe(&outbound);
        let attempt = self.io.core.signing.next_id();
        // Exhaustion is handled through the same asynchronous failure event.
        let timer = if self.io.core.signer.as_ref().is_some_and(|s| s.immediate()) {
            TimerId(0)
        } else {
            TimerId(self.io.next())
        };
        let staged = self
            .io
            .core
            .signing
            .stage(attempt, outbound.clone(), step, timer, self.now);
        if let Err(error) = staged {
            // Retain the continuation even when admission fails, so the engine
            // reports a typed pre-dispatch failure instead of silently dropping
            // it. Only a latched missing key keeps its own kind.
            let error = if error == crate::signing::SignError::MissingKey {
                error
            } else {
                crate::signing::SignError::Unavailable
            };
            self.io.core.signing_admission.push((outbound, error));
            return;
        }
        if timer.0 != 0 {
            self.io.core.actions.push(Action::SetTimer {
                timer,
                at_ms: staged.expect("checked admission"),
            });
        }
        self.io.core.actions.insert(
            index,
            Action::Sign {
                attempt,
                work: crate::signing::SignRequest {
                    receipt: Default::default(),
                    operations: Vec::new(),
                    context: Default::default(),
                },
                timeout_ms: 0,
            },
        );
    }

    /// [`Cx::event`] without fields.
    pub fn notice(&mut self, level: Level, code: &'static str, message: impl Into<String>) {
        self.event(level, code, message, Vec::new());
    }

    /// Counted in [`Health`] whatever the host's minimum level.
    pub fn event(
        &mut self,
        level: Level,
        code: &'static str,
        message: impl Into<String>,
        fields: Vec<(&'static str, Value)>,
    ) {
        // A rejection often also arrives as an auth-kind failure or `deactivate`.
        if code == log_code::AUTH_REJECTED {
            count_auth_failure(self.io.core, None);
        }
        push_event(
            &mut self.io.core.actions,
            &mut self.io.core.health,
            self.io.core.min_level,
            LogEvent {
                level,
                code,
                message: message.into(),
                fields,
            },
        );
    }

    pub fn command_progress(&mut self, command: CommandId, receipts: Vec<OrderIdentityReceipt>) {
        if !receipts.is_empty() {
            self.io
                .core
                .actions
                .push(Action::Notice(Notice::CommandProgress {
                    command,
                    receipts,
                }));
        }
    }

    pub fn command_error(&mut self, command: CommandId, message: impl Into<String>) {
        let message = message.into();
        if command == 0 {
            self.notice(Level::Error, log_code::COMMAND_FAILED, message);
        } else {
            self.io
                .core
                .actions
                .push(Action::Notice(Notice::CommandError { command, message }));
        }
    }

    // One non-generic seal/retire/publish path, including success conversion.
    #[inline(never)]
    fn finish_command(
        &mut self,
        command: CommandId,
        result: Result<String, VenueError>,
        authority: Option<&crate::request_context::Context>,
    ) -> Result<(), crate::request_context::Error> {
        self.finish_command_as(command, result, authority, false)
    }

    fn finish_command_as(
        &mut self,
        command: CommandId,
        result: Result<String, VenueError>,
        authority: Option<&crate::request_context::Context>,
        observer: bool,
    ) -> Result<(), crate::request_context::Error> {
        #[cfg(feature = "http-plans")]
        if self.io.core.http_preflight_ready.as_ref().is_some_and(|ready| {
            !matches!(ready.owner(), crate::request_context::Owner::Command { id, .. } if id == command)
        }) {
            return Err(crate::request_context::Error::Revoked);
        }
        if authority.is_none() && !self.io.numeric_entry_completion_allowed(command) {
            return Err(crate::request_context::Error::Revoked);
        }
        let (success, error) = match result {
            Ok(result) => (Some(result), None),
            Err(error) => (None, Some(error)),
        };
        let delivery = if observer {
            None
        } else {
            completion_delivery(&self.io.core.http_delivery)
        };
        let seal = self
            .io
            .core
            .request_contexts
            .seal_as(command, delivery, authority, error, observer);
        self.finish_sealed_command(command, success, seal)
    }

    // Validated Context, Plan and execution-observer seals share one terminal
    // cleanup/publication path without lending physical delivery authority.
    #[inline(never)]
    fn finish_sealed_command(
        &mut self,
        command: CommandId,
        success: Option<String>,
        seal: request_context_registry::Seal,
    ) -> Result<(), crate::request_context::Error> {
        let mut write_sealed = false;
        let result = match seal {
            request_context_registry::Seal::Stale => {
                return Err(crate::request_context::Error::Revoked);
            }
            request_context_registry::Seal::Legacy(Some(error)) => Err(error),
            request_context_registry::Seal::Legacy(None) => Ok(success.unwrap_or_default()),
            request_context_registry::Seal::Success => {
                write_sealed = true;
                Ok(success.unwrap_or_default())
            }
            request_context_registry::Seal::Failure(error, rejections) => {
                write_sealed = true;
                for message in rejections {
                    self.command_error(command, message);
                }
                Err(error)
            }
        };
        // A credential-free route's auth-kind refusal is no credential evidence.
        let credential_evidence = self.io.core.request_contexts.owner_credentialed(command);
        self.io.finish_command_wait(command);
        self.io.retire_http_command(command);
        if !self.io.core.request_contexts.finish(command) {
            self.io.core.http_owner_poison = true;
            panic!("HTTP owner retirement bookkeeping failed");
        }
        self.io.untrack_read_root(command);
        #[cfg(feature = "metadata-freshness")]
        self.store.metadata_write_finished(command);
        #[cfg(feature = "order-locks")]
        {
            if write_sealed
                && result
                    .as_ref()
                    .err()
                    .is_none_or(|error| error.outcome != Some(crate::Outcome::Unknown))
            {
                self.io.core.order_locks.managed_resolved(command);
            }
            let scope = self.io.core.order_locks.reconcile_scope(command);
            let absent = self.io.core.order_locks.absent_pending(command);
            let unknown = result
                .as_ref()
                .err()
                .is_some_and(|error| error.outcome == Some(crate::Outcome::Unknown));
            let unscoped = self.io.core.order_locks.settle_mark(command, unknown);
            let marked = self.io.core.order_locks.finish(command, result.is_err());
            // Every uncertain seal asks again, even for an already-uncertain
            // symbol: an earlier in-flight read may predate this write.
            // A managed write the lock never held still reconciles: a full read.
            let scope = scope
                .or(unscoped)
                .or_else(|| (write_sealed && unknown).then_some(None));
            if let Some(scope) = scope.filter(|_| unknown || absent || !marked.is_empty()) {
                // A rate limit or credential refusal must not fan out reads now.
                let delay = match result.as_ref().err().and_then(|error| error.kind) {
                    Some(ErrorKind::RateLimited) => result
                        .as_ref()
                        .err()
                        .and_then(|error| error.retry_after_ms)
                        .unwrap_or(crate::classify::DEFAULT_RATE_LIMIT_DEFERRAL_MS),
                    Some(
                        ErrorKind::AuthInvalid | ErrorKind::AuthExpired | ErrorKind::AuthPermission,
                    ) => crate::classify::DEFAULT_RATE_LIMIT_DEFERRAL_MS,
                    _ => 0.,
                };
                let delay = delay.clamp(0., crate::startup::MAX_RATE_LIMIT_PAUSE_MS);
                self.io.queue_reconcile(scope, self.now + delay);
            }
            self.io.core.active_commands.remove(&command);
        }
        #[cfg(not(feature = "order-locks"))]
        let _ = write_sealed;
        let notice = match result {
            Ok(result) => Notice::CommandDone { command, result },
            Err(error) => {
                if credential_evidence {
                    count_auth_failure(self.io.core, Some(&error));
                }
                Notice::CommandFailed { command, error }
            }
        };
        self.io.entry_completed(command);
        self.io.core.actions.push(Action::Notice(notice));
        Ok(())
    }
}

/// A final fresh signing release. No clock, I/O or credential values are exposed.
pub enum SigningTarget<'a, P> {
    Http {
        request: RequestId,
        pending: &'a mut P,
    },
    Socket {
        socket: SocketId,
    },
}

/// Exactly one final HTTP admission mode, fixed at engine construction.
/// Mutable scheduler admission cannot participate in an atomic nonce release.
pub enum HttpRelease<V: Venue> {
    Mutable(
        fn(
            &mut V,
            &mut V::Pending,
            &crate::request_context::HttpRelease,
        ) -> Result<crate::request_context::Admission, crate::request_context::Error>,
    ),
    NonceUnscheduled(
        fn(
            &V,
            &V::Pending,
            &crate::request_context::HttpRelease,
        ) -> Result<(), crate::request_context::Error>,
    ),
}

impl<V: Venue> Copy for HttpRelease<V> {}
impl<V: Venue> Clone for HttpRelease<V> {
    fn clone(&self) -> Self {
        *self
    }
}

/// One exchange implementation. All methods are synchronous and must not block.
pub trait Venue {
    fn address_nonce_domain(&self) -> Option<&'static crate::nonce::NonceDomain> {
        None
    }
    fn public_address_slots(&self) -> AddressSetupSlots {
        AddressSetupSlots::None
    }
    /// Slots whose public key a host-answered provider binds at setup, in place
    /// of an address; a venue declares at most one of the two.
    fn public_key_setup(&self) -> Option<(AddressSetupSlots, crate::signing::Algorithm)> {
        None
    }
    /// A fully parsed native refusal may exempt this delivery from unproven
    /// HTTP status handling. The default keeps the strict shared policy.
    fn status_exempt_refusal(&self, _pending: &Self::Pending, _response: Response<'_>) -> bool {
        false
    }

    /// Read-only native evidence from an unproven managed mutation envelope.
    /// Exact SDK identity namespaces must be validated by the native codec.
    fn unproven_http_evidence(
        &self,
        _pending: &Self::Pending,
        _response: Response<'_>,
        _evidence: &mut crate::request_context::UnprovenEvidence,
    ) {
    }

    /// Drop only this generation's native command planning after the engine
    /// has sealed and retired it. No Store, response or completion authority.
    fn discard_unproven_http_command(&mut self, _owner: crate::request_context::Owner) {}

    /// The engine retired this tracked Read root at its deadline. `pending`
    /// holds the continuations of its outstanding requests, already discarded
    /// from transport. Drop them and any queued continuation for `owner`; the
    /// engine seals the command itself. No Cx: no completion, send or Store.
    fn read_root_retired(
        &mut self,
        _owner: crate::request_context::Owner,
        _pending: Vec<Self::Pending>,
    ) {
    }

    /// Discard only continuation state matching this exact retired generation.
    /// This cleanup hook has no Store, response, command or I/O authority.
    #[cfg(feature = "http-plans")]
    fn on_http_owner_retired(&mut self, _owner: &crate::request_context::HttpOwnerRetirement) {}

    /// Cleanup may return one already-parked successor, without Cx authority.
    /// The engine validates it only after this exact retirement completes.
    #[cfg(feature = "http-plans")]
    fn on_http_owner_retired_preflight(
        &mut self,
        owner: &crate::request_context::HttpOwnerRetirement,
    ) -> Option<crate::request_context::Preflight> {
        self.on_http_owner_retired(owner);
        None
    }

    /// One exact retained owner is active after the outer event has unwound.
    /// Returning without a terminal, submitted read or Plan fails that owner.
    #[cfg(feature = "http-plans")]
    fn on_http_preflight_ready(
        &mut self,
        _cx: &mut Cx<'_, Self::Pending>,
        _preflight: &crate::request_context::Preflight,
    ) {
    }

    /// Typed readiness after actual HTTP scope exit. The engine marks due slots
    /// once before this callback; no delivery/publication authority is lent.
    #[cfg(feature = "http-plans")]
    fn on_http_plan_ready(
        &mut self,
        _cx: &mut Cx<'_, Self::Pending>,
        _plan: &crate::request_context::Plan,
    ) {
    }

    /// An engine-bounded condition wait expired, in failure custody. Fail the
    /// owner here; returning without a terminal seals it once as NotSent.
    #[cfg(feature = "condition-waits")]
    fn on_condition_wait_expired(
        &mut self,
        _cx: &mut Cx<'_, Self::Pending>,
        _owner: &crate::request_context::ConditionOwner,
        _error: VenueError,
    ) {
    }

    /// Bounded native correlation metadata only. No credential, outbound packet
    /// or recipe may be retained here. Managed mutations require this codec.
    fn capture_http_reconciliation(
        &self,
        _pending: &Self::Pending,
    ) -> Result<crate::request_context::ReconciliationData, crate::request_context::Error> {
        Err(crate::request_context::Error::Invalid)
    }

    /// Pure late response codec, separate from ordinary reducers. Immutable
    /// venue access and the restricted view cannot reopen a command or publish
    /// authentication/retry/scheduler state or clear an order-lock fence.
    fn reconcile_http(
        &self,
        _cx: &mut crate::request_context::Reconciliation<'_>,
        _data: &crate::request_context::ReconciliationData,
        _response: Response<'_>,
    ) {
    }

    /// Called only while preparing a Session-bound managed request: the one
    /// place the session headers (an `Authorization`, or a session id plus its
    /// cookie) are copied. No I/O, `Cx` or state change.
    fn session_bearer(&self, _generation: u64) -> crate::request_context::SessionBearer {
        crate::request_context::SessionBearer::NotReady
    }

    /// The current revision only, or `None` when not ready: the claim-time
    /// fence, and the signed path's readiness check before provider work. It
    /// must not touch, clone or allocate the session headers; one revision
    /// covers the whole header set.
    fn session_bearer_revision(&self, _generation: u64) -> Option<u64> {
        None
    }

    /// Fixed per-engine send-group floors; individual attempts may only extend
    /// the gap following their physical transport handoff.
    fn http_pacing(&self) -> &[crate::request_context::Pacing] {
        &[]
    }
    /// Flat immutable sets share the registered primitive clocks.
    fn http_pacing_sets(&self) -> &[crate::request_context::PacingSet] {
        &[]
    }

    /// Whether this venue uses managed sockets. Engine<V> reads it statically,
    /// so a venue that does not declare it compiles every managed path out.
    #[inline]
    fn managed_sockets() -> bool
    where
        Self: Sized,
    {
        false
    }

    /// Whether this venue uses HTTP mutation barrier groups. Engine<V> reads it
    /// statically, so a venue that does not declare it compiles group paths out.
    #[inline]
    fn http_mutation_barriers() -> bool
    where
        Self: Sized,
    {
        false
    }

    /// The venue's one FIFO release domain, if any. Its group must be registered
    /// in `http_pacing`; the engine stamps it onto every member Context.
    #[cfg(feature = "release-domains")]
    #[inline]
    fn http_release_domain() -> Option<crate::request_context::ReleaseDomain>
    where
        Self: Sized,
    {
        None
    }
    /// Immutable nonce capability; the engine captures its permission at construction.
    /// This selector grants no claim or release authority.
    #[inline]
    fn dispatch_nonce() -> bool
    where
        Self: Sized,
    {
        false
    }

    /// Captured once at engine construction; the two admission modes are exclusive.
    fn http_release() -> HttpRelease<Self>
    where
        Self: Sized,
    {
        HttpRelease::Mutable(|_, _, _| Ok(crate::request_context::Admission::Unscheduled))
    }

    /// Explicit pure group admission; the default refuses group support.
    /// Implementations must not mutate state, schedule work or use interior
    /// mutability here. This first contract accepts only Unscheduled.
    #[cfg(feature = "http-mutation-barriers")]
    fn prepare_group_http_release(
        &self,
        _pending: &Self::Pending,
        _release: &crate::request_context::HttpRelease,
    ) -> Result<crate::request_context::Admission, crate::request_context::Error> {
        Err(crate::request_context::Error::Invalid)
    }

    /// Typed setup/bind/denial failures are pre-send facts, distinct from a
    /// transport failure after release. Read schedulers can retire denied work
    /// neutrally rather than count another network/authentication failure.
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        _reason: crate::request_context::Error,
        error: VenueError,
    ) {
        self.on_sign_error(cx, pending, error);
    }

    /// Reconcile writes whose outcome is unknown: issue a fresh order read for
    /// these symbols (`None`: all) and report it through `orders_reconciled`.
    /// The engine asks once per uncertain seal, after any rate-limit wait, and
    /// re-asks with backoff while a fence stands (`Cx::reconcile_reask`).
    fn reconcile_uncertain(
        &mut self,
        _cx: &mut Cx<'_, Self::Pending>,
        _symbols: Option<&[String]>,
    ) {
    }

    /// A repair root ended or a snapshot mint was refused. Runs outside any
    /// command entry, like `reconcile_uncertain`; it grants no new authority.
    #[cfg(feature = "repair-mutations")]
    fn on_repair_notice(
        &mut self,
        _cx: &mut Cx<'_, Self::Pending>,
        _notice: crate::repair::RepairNotice<'_>,
    ) {
    }

    /// A Plan Slot parked on a Session ended unissued because the Session was
    /// retired or failed. This runs in that Plan's ready custody: re-park the Slot
    /// on another live Session, or fail the Plan. Nothing was sent for it.
    #[cfg(feature = "http-plans")]
    fn on_plan_session_wait_ended(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        plan: &crate::request_context::Plan,
        pending: Self::Pending,
        error: VenueError,
    ) {
        let _ = plan;
        self.on_session_wait_expired(cx, pending, error);
    }

    /// A retained Preflight read parked on a Session ended unissued because the
    /// Session was retired or failed. This runs in that Preflight's ready
    /// custody: re-park on another live Session, or finish or fail the Preflight.
    #[cfg(feature = "http-plans")]
    fn on_preflight_session_wait_ended(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        preflight: &crate::request_context::Preflight,
        pending: Self::Pending,
        error: VenueError,
    ) {
        let _ = preflight;
        self.on_session_wait_expired(cx, pending, error);
    }

    /// A session waiter reached its owner's original deadline before the venue
    /// drained it. Nothing was sent for that owner.
    fn on_session_wait_expired(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        error: VenueError,
    ) {
        self.on_http_preparation_failed(
            cx,
            pending,
            crate::request_context::Error::TimedOut,
            error,
        );
    }

    /// Defer symbol cancellation until placement native identities are known.
    fn guard_pending_placements(&self) -> bool {
        false
    }
    /// Prevent overlapping precomputed amendments and cancellations.
    fn guard_order_mutations(&self) -> bool {
        false
    }
    /// Cancel and amend route only through stored orders: hold one naming an
    /// acknowledged placement until the store has it (`placement_acks`).
    fn cancel_needs_stored_order(&self) -> bool {
        false
    }
    /// Refuse cancel-all while a placement outcome is unknown. A venue that opts
    /// out must itself cancel any late-landing order once its fresh read proves it.
    fn fence_uncertain_cancel_all(&self) -> bool {
        true
    }

    /// Refuse a placement or amend on a symbol whose earlier placement is
    /// Unknown until a fresh order read resolves it. Opt in only when
    /// `reconcile_uncertain` starts that read at once, so the fence is short-lived.
    fn fence_uncertain_placements(&self) -> bool {
        false
    }

    /// What this venue answers beyond the common commands; the engine gates
    /// option and symbol-routed commands from it.
    fn capabilities(&self) -> Capabilities {
        Capabilities::BASE
    }
    /// Per-request continuation: what to do with the response.
    type Pending;
    /// Observe typed final authentication evidence without changing retry or state.
    /// Unknown, malformed, stale and intermediate retry responses return `None`.
    fn account_auth_outcome(
        &self,
        _pending: &Self::Pending,
        _response: &Response<'_>,
        _now: f64,
        _store: &Store,
    ) -> Option<AuthOutcome> {
        None
    }
    /// Update only pending metadata using the actual stamp/expiry released.
    /// This is not evidence of transport delivery or venue acceptance.
    fn on_signing_receipt(
        &mut self,
        _target: SigningTarget<'_, Self::Pending>,
        _receipt: crate::signing::SignReceipt,
    ) {
    }
    /// Observe exact final-release clock metadata; this grants no transport right.
    fn on_signing_release(
        &mut self,
        target: SigningTarget<'_, Self::Pending>,
        release: crate::SigningRelease,
    ) {
        self.on_signing_receipt(target, release.receipt());
    }
    /// Shared dispatch signing failure, typed before transport. Composite jobs
    /// retain earlier attempt outcomes when projecting this attempted notSent.
    fn on_sign_error(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        error: VenueError,
    ) {
        self.on_sign_failed(cx, pending, &error.message);
    }
    /// Delegated signing failed before an HTTP write could reach the venue.
    fn on_sign_failed(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        error: &str,
    ) {
        self.on_request_failed(cx, pending, error);
    }

    #[cfg(feature = "managed-sockets")]
    fn socket_control(
        &self,
        _store: &Store,
        _feed: crate::socket_lifecycle::FeedId,
        _authenticated: bool,
        _text: &str,
    ) -> crate::socket_lifecycle::Control {
        crate::socket_lifecycle::Control::Other
    }
    /// Optional managed-socket behaviour this venue uses; FULL unless narrowed.
    #[cfg(feature = "managed-sockets")]
    /// An associated function, not a const, so `Venue` stays dyn compatible.
    #[inline]
    fn socket_capabilities() -> crate::socket_lifecycle::SocketCapabilities
    where
        Self: Sized,
    {
        crate::socket_lifecycle::SocketCapabilities::FULL
    }
    #[cfg(feature = "managed-sockets")]
    fn socket_startup_ready(&self, _store: &Store) -> bool {
        false
    }
    #[cfg(feature = "managed-sockets")]
    fn on_socket_event(
        &mut self,
        _cx: &mut Cx<'_, Self::Pending>,
        _event: crate::socket_lifecycle::LifecycleEvent,
    ) -> Result<(), crate::socket_lifecycle::SocketError> {
        Err(crate::socket_lifecycle::SocketError::NotPermitted)
    }
    #[cfg(feature = "managed-sockets")]
    fn on_socket_interests_released(
        &mut self,
        _cx: &mut Cx<'_, Self::Pending>,
        _epoch: crate::socket_lifecycle::Epoch,
        _pending: Self::Pending,
    ) -> Result<(), crate::socket_lifecycle::SocketError> {
        Ok(())
    }
    #[cfg(feature = "managed-sockets")]
    fn encode_socket_interests(
        &self,
        _store: &Store,
        _feed: crate::socket_lifecycle::FeedId,
        _subscribe: bool,
        _items: &[crate::socket_lifecycle::Interest],
        _emit: &mut dyn FnMut(
            crate::socket_lifecycle::EncodedBatch,
        ) -> Result<(), crate::socket_lifecycle::SocketError>,
    ) -> Result<(), crate::socket_lifecycle::SocketError> {
        Err(crate::socket_lifecycle::SocketError::NotPermitted)
    }
    fn start(&mut self, cx: &mut Cx<'_, Self::Pending>);
    fn on_open(&mut self, cx: &mut Cx<'_, Self::Pending>, socket: SocketId);
    fn on_binary(&mut self, _cx: &mut Cx<'_, Self::Pending>, _socket: SocketId, _data: &[u8]) {}
    fn on_message(&mut self, cx: &mut Cx<'_, Self::Pending>, socket: SocketId, text: &str);
    fn on_close(&mut self, cx: &mut Cx<'_, Self::Pending>, socket: SocketId, code: u16);
    /// Opts in to `on_route_exhausted`; every other venue sees today's close.
    #[inline]
    fn route_exhaustion() -> bool
    where
        Self: Sized,
    {
        false
    }
    /// Every route was refused, so the host dialled nothing. Called only when
    /// `route_exhaustion` returns true.
    fn on_route_exhausted(&mut self, _cx: &mut Cx<'_, Self::Pending>, _socket: SocketId) {}
    fn on_response(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        response: Response<'_>,
    );
    /// One element of a streamed response's array (see [`Cx::request_streamed`]).
    fn on_item(
        &mut self,
        _cx: &mut Cx<'_, Self::Pending>,
        _pending: &mut Self::Pending,
        _item: &str,
    ) {
    }
    /// A complete envelope arrived without its expected streamed list. Venues
    /// may recognize business errors; successful missing lists still fail.
    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        response: Response<'_>,
    ) {
        self.on_request_failed(
            cx,
            pending,
            &format!(
                "response did not contain the expected list: {}",
                response.body
            ),
        );
    }
    /// The streamed envelope itself was malformed. Kept distinct from transport
    /// failures so public startup reporting need not expose a raw body preview.
    fn on_stream_malformed(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        _status: u16,
        _headers: &str,
        error: &str,
    ) {
        // The venue answered but the stream broke (or is not UTF-8): a
        // malformed reply, never transport loss. Defaults reach on_request_failed.
        self.on_http_failed(cx, pending, malformed_body(error));
    }
    fn on_request_failed(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        error: &str,
    );
    /// A request that may have reached the venue failed without a usable
    /// response. `error.kind` is `Timeout` for engine deadline expiry,
    /// `Internal` for a delivered body (or managed write reply) the engine cannot
    /// decode, including a stream that breaks mid-array (the default
    /// `on_stream_malformed` forwards here), else `Network`.
    fn on_http_failed(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        error: VenueError,
    ) {
        self.on_request_failed(cx, pending, &error.message);
    }
    fn on_poll_scale(&mut self, _cx: &mut Cx<'_, Self::Pending>, _scale: f64) {}
    /// Revalue account-derived state after host-owned public prices change.
    fn on_public_feed(&mut self, _cx: &mut Cx<'_, Self::Pending>) {}
    fn on_timer(&mut self, cx: &mut Cx<'_, Self::Pending>, timer: TimerId);
    fn on_command(&mut self, cx: &mut Cx<'_, Self::Pending>, command: Command);
    /// Venue-specific actions; the engine never passes `Command::Venue` to `on_command`.
    fn on_venue_action(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        command: CommandId,
        _action: VenueAction,
    ) {
        unsupported_venue_action(cx, command);
    }
    fn book(&self, symbol: &str) -> Option<&Book>;
    /// Every subscribed book, including unsynchronized subscriptions.
    /// Snapshot frames clear subscriptions for which `book` returns `None`.
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_>;
}

/// Startup never hangs silently: sections still unloaded after this are named
/// in a visible notice, repeated on a slow cadence until they load (non-terminal).
const STARTUP_WATCH_MS: f64 = 60_000.0;
const EMPTY_CATALOGUE_WARNING: &str = "Venue returned an empty market catalogue; it was not applied and the previous catalogue is kept";
const STARTUP_REPEAT_MS: f64 = 300_000.0;

fn watch_startup<P>(cx: &mut Cx<'_, P>, account: bool) -> bool {
    let loaded = cx.store.loaded();
    let tickers = cx.io.core.public_feed_policy != PublicFeedPolicy::Disabled;
    let missing: Vec<&str> = [
        ("markets", loaded.markets, true),
        ("tickers", loaded.tickers, tickers),
        ("balance", loaded.balance, account),
        ("orders", loaded.orders, account),
        ("positions", loaded.positions, account),
    ]
    .into_iter()
    .filter_map(|(name, done, required)| (required && !done).then_some(name))
    .collect();
    if missing.is_empty() {
        return false;
    }
    // Only missing tickers can still leave a private account usable (no prices).
    let level = if missing == ["tickers"] {
        Level::Warn
    } else {
        Level::Error
    };
    cx.notice(
        level,
        log_code::STARTUP_INCOMPLETE,
        format!(
            "Startup incomplete: still waiting for {}",
            missing.join(", ")
        ),
    );
    true
}

pub struct Engine<V: Venue> {
    venue: V,
    http_release: HttpRelease<V>,
    store: Store,
    io: Io<V::Pending>,
    started: bool,
    /// Checked on handled events, so it adds no timer of its own.
    startup_watch_at: Option<f64>,
    /// Engine-owned wake-up for commands held by `placement_acks`, and its time.
    ack_timer: Option<(TimerId, f64)>,
    #[cfg(feature = "order-locks")]
    reconcile_timer: Option<(TimerId, f64)>,
    /// Set while dispatching released commands, which must not be held again.
    releasing: bool,
    /// Hosts opt in when the account can sign; public-only engines have no account sections.
    account_startup: bool,
    shared_books: HashMap<String, Book>,
    shared_tickers: HashMap<String, crate::public_feed::PublicTicker>,
    shared_ticker_revision: u64,
    #[cfg(feature = "command-tracking")]
    commands_received: bool,
    #[cfg(feature = "book-deltas")]
    book_deltas: bool,
    #[cfg(feature = "book-deltas")]
    book_baselines: HashMap<String, BookBaseline>,
    #[cfg(feature = "book-deltas")]
    book_market_rows: Vec<(String, String)>,
}

#[cfg(feature = "book-deltas")]
struct BookBaseline {
    market: u32,
    id: String,
    book: Book,
}

impl<V: Venue> Engine<V> {
    pub fn new(venue: V, entropy: Option<crate::entropy::Entropy>) -> Result<Self, VenueError> {
        let entropy = entropy.ok_or_else(|| {
            VenueError::new("Engine entropy providers are required").kind(ErrorKind::InvalidInput)
        })?;
        let mut store = Store::default();
        store.set_quantity_units(venue.capabilities().quantity_units);
        let http_pacing = crate::request_context::SendPacing::with_sets(
            venue.http_pacing(),
            venue.http_pacing_sets(),
        )
        .map_err(|_| VenueError::new("Invalid HTTP pacing policy").kind(ErrorKind::InvalidInput))?;
        let http_release = V::http_release();
        #[cfg(feature = "order-locks")]
        let order_locks =
            crate::order_locks::OrderLocks::fencing_placements(venue.fence_uncertain_placements());
        #[cfg(feature = "release-domains")]
        let release_domain = V::http_release_domain();
        #[cfg(feature = "release-domains")]
        if let Some(domain) = release_domain {
            release_domain::validate(domain, &http_pacing).map_err(|_| {
                VenueError::new("Invalid HTTP release domain").kind(ErrorKind::InvalidInput)
            })?;
        }
        #[cfg(feature = "release-domains")]
        let (domain_queue, domain_held) = release_domain::reserve(release_domain.is_some())
            .map_err(|_| VenueError::new("HTTP release domain capacity unavailable"))?;
        Ok(Self {
            venue,
            http_release,
            store,
            io: Io {
                core: IoCore {
                    entropy: Some(entropy),
                    http_pacing,
                    #[cfg(feature = "release-domains")]
                    release_domain,
                    #[cfg(feature = "release-domains")]
                    domain_queue,
                    #[cfg(feature = "release-domains")]
                    domain_held,
                    nonce_unscheduled: matches!(http_release, HttpRelease::NonceUnscheduled(_)),
                    #[cfg(feature = "dispatch-nonce")]
                    nonce_enabled: V::dispatch_nonce(),
                    #[cfg(feature = "http-mutation-barriers")]
                    mutation_barriers_declared: V::http_mutation_barriers(),
                    #[cfg(feature = "managed-sockets")]
                    managed_sockets_declared: V::managed_sockets(),
                    #[cfg(feature = "managed-sockets")]
                    socket_capabilities: V::socket_capabilities(),
                    #[cfg(feature = "order-locks")]
                    order_locks,
                    ..IoCore::default()
                },
                #[cfg(feature = "managed-sockets")]
                managed_close: V::managed_sockets().then_some(
                    Cx::<V::Pending>::managed_close
                        as for<'a> fn(&mut Cx<'a, V::Pending>, SocketId),
                ),
                ..Io::default()
            },
            started: false,
            startup_watch_at: None,
            ack_timer: None,
            #[cfg(feature = "order-locks")]
            reconcile_timer: None,
            releasing: false,
            account_startup: false,
            shared_books: HashMap::new(),
            shared_tickers: HashMap::new(),
            shared_ticker_revision: 0,
            #[cfg(feature = "command-tracking")]
            commands_received: false,
            #[cfg(feature = "book-deltas")]
            book_deltas: false,
            #[cfg(feature = "book-deltas")]
            book_baselines: HashMap::new(),
            #[cfg(feature = "book-deltas")]
            book_market_rows: Vec::new(),
        })
    }

    /// Trusted construction installs the provider handle; no key material enters the engine.
    pub fn set_signer(
        &mut self,
        signer: Option<crate::signing::Provider>,
    ) -> Result<(), VenueError> {
        if self.started {
            return Err(VenueError::new("Signer must be configured before startup")
                .kind(ErrorKind::InvalidInput));
        }
        #[cfg(feature = "address-nonce")]
        self.invalidate_address_setup()?;
        #[cfg(feature = "nonce-handoff")]
        self.invalidate_nonce_handoff();
        self.io.core.signer = signer;
        Ok(())
    }
    pub fn signer(&self) -> Option<crate::signing::Provider> {
        self.io.core.signer.clone()
    }

    /// Trusted construction registers the same refs the venue holds, so
    /// command admission can resolve a slot without trusting the venue's claim.
    pub fn set_identities(
        &mut self,
        identities: crate::signing::Identities,
    ) -> Result<(), VenueError> {
        if self.started {
            return Err(
                VenueError::new("Identities must be configured before startup")
                    .kind(ErrorKind::InvalidInput),
            );
        }
        #[cfg(feature = "address-nonce")]
        self.invalidate_address_setup()?;
        #[cfg(feature = "address-nonce")]
        let identities = {
            let mut identities = identities;
            for identity in [&mut identities.primary, &mut identities.secondary]
                .into_iter()
                .flatten()
            {
                identity.address = Default::default();
            }
            identities
        };
        #[cfg(feature = "nonce-handoff")]
        let identities = {
            self.invalidate_nonce_handoff();
            let mut identities = identities;
            for id in [&mut identities.primary, &mut identities.secondary]
                .into_iter()
                .flatten()
            {
                id.nonce_owner = None;
            }
            identities
        };
        self.io.core.identities = identities;
        Ok(())
    }
    /// The same read `Cx::verified_public_key` makes, for installation controls.
    #[cfg(any(test, feature = "test-support"))]
    pub fn verified_public_key(
        &self,
        slot: CredentialSlot,
        algorithm: crate::signing::Algorithm,
    ) -> Result<[u8; 32], crate::signing::SignError> {
        self.io.core.verified_public_key(slot, algorithm)
    }
    pub fn identities(&self) -> &crate::signing::Identities {
        &self.io.core.identities
    }

    /// A host signer fulfilling Action::Sign declares what it can sign; undeclared is unknown.
    pub fn set_host_algorithms(
        &mut self,
        algorithms: Option<Vec<crate::signing::Algorithm>>,
    ) -> Result<(), VenueError> {
        if self.started {
            return Err(VenueError::new("Signer must be configured before startup")
                .kind(ErrorKind::InvalidInput));
        }
        #[cfg(feature = "address-nonce")]
        self.invalidate_address_setup()?;
        self.io.core.host_algorithms = algorithms;
        Ok(())
    }

    /// Delegated REST-only intent does not disable account reads or writes.
    pub fn set_private_feed_policy(
        &mut self,
        policy: crate::signing_policy::PrivateFeedPolicy,
    ) -> Result<(), VenueError> {
        if self.started {
            return Err(
                VenueError::new("Private feed policy must be configured before startup")
                    .kind(ErrorKind::InvalidInput),
            );
        }
        self.io.core.private_feed_policy = policy;
        Ok(())
    }

    /// Read the same calibrated protocol view and revision used by dispatch,
    /// without starting the venue or changing its raw scheduling clock.
    pub fn signing_clock(&self, host_now: f64) -> (f64, u64) {
        (
            self.io.core.protocol_clock.at(host_now),
            self.io.core.protocol_clock.revision(),
        )
    }

    /// Host configuration is immutable once startup begins.
    pub fn set_entropy(&mut self, fill: crate::entropy::Fill) -> Result<(), VenueError> {
        if self.started {
            return Err(VenueError::new("Entropy must be configured before startup")
                .kind(ErrorKind::InvalidInput));
        }
        self.io
            .core
            .entropy
            .as_mut()
            .expect("validated construction")
            .set_identity(fill);
        Ok(())
    }
    /// Independent explicit injection; identity seeds never affect signing.
    pub fn set_signing_entropy(&mut self, fill: crate::entropy::Fill) -> Result<(), VenueError> {
        if self.started {
            return Err(
                VenueError::new("Signing entropy must be configured before startup")
                    .kind(ErrorKind::InvalidInput),
            );
        }
        self.io
            .core
            .entropy
            .as_mut()
            .expect("validated construction")
            .set_signing(fill);
        Ok(())
    }

    /// Feed ownership is fixed before startup; a disconnected provider never falls back.
    pub fn set_public_feed_policy(&mut self, policy: PublicFeedPolicy) -> Result<(), &'static str> {
        if self.started {
            return Err("public feed policy must be configured before startup");
        }
        self.io.core.public_feed_policy = policy;
        Ok(())
    }

    /// Account sections (balance, orders, positions) are expected at startup only
    /// for an account that can sign, matching the SDK's readiness rule.
    pub fn set_account_startup(&mut self, account: bool) {
        self.account_startup = account;
    }

    pub fn public_feed_policy(&self) -> PublicFeedPolicy {
        self.io.core.public_feed_policy
    }

    /// Apply an explicitly shared public feed without admitting any account fields.
    pub fn apply_public_feed(
        &mut self,
        now: f64,
        update: PublicFeedUpdate,
    ) -> Result<(), &'static str> {
        self.assert_live();
        self.expire_read_roots(now);
        if self.public_feed_policy() != PublicFeedPolicy::Shared {
            return Err("publicFeedUpdate requires shared public feed policy");
        }
        // Validate the complete update before changing state.
        update.validate()?;
        let mut accepted_tickers = false;
        for ticker in update.tickers {
            if !ticker.has_values() || self.store.market(&ticker.symbol).is_none() {
                continue;
            }
            accepted_tickers = true;
            match self.shared_tickers.entry(ticker.symbol.clone()) {
                std::collections::hash_map::Entry::Occupied(mut entry) => {
                    entry.get_mut().merge(ticker)
                }
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(ticker);
                }
            }
        }
        // Book/candle traffic must not replay quote-dependent account updates.
        // Still restore independently changed native ticker revisions.
        self.restore_shared_tickers(accepted_tickers, now);
        let mut cx = Cx {
            now,
            store: &mut self.store,
            io: &mut self.io,
        };
        for update in update.books {
            if cx.store.market(&update.symbol).is_none() {
                continue;
            }
            let book = self.shared_books.entry(update.symbol.clone()).or_default();
            book.clear();
            for (side, levels) in [(BookSide::Bids, update.bids), (BookSide::Asks, update.asks)] {
                for (price, amount) in levels {
                    book.apply(side, price, amount);
                }
            }
            cx.book_changed(&update.symbol);
        }
        for update in update.candles {
            if cx.store.market(&update.symbol).is_some() {
                cx.candle(&update.symbol, update.interval, update.candle());
            }
        }
        Ok(())
    }

    #[inline]
    fn restore_shared_tickers(&mut self, force: bool, now: f64) {
        if self.public_feed_policy() != PublicFeedPolicy::Shared
            || (!force && self.shared_ticker_revision == self.store.ticker_revision())
        {
            return;
        }
        self.apply_shared_tickers(now);
    }

    fn apply_shared_tickers(&mut self, now: f64) {
        let mut missing = Vec::new();
        for ticker in self.shared_tickers.values() {
            if self.store.ticker(&ticker.symbol).is_none()
                && let Some(market) = self.store.market(&ticker.symbol)
            {
                missing.push(crate::model::Ticker {
                    id: market.id.clone(),
                    symbol: market.symbol.clone(),
                    ..Default::default()
                });
            }
        }
        if !missing.is_empty() {
            let mut tickers = self.store.tickers().to_vec();
            tickers.extend(missing);
            self.store.set_tickers(tickers);
        }
        // Venue REST seeds may fill omitted fields, but cannot overwrite provider quotes.
        for ticker in self.shared_tickers.values() {
            if self.store.market(&ticker.symbol).is_some() {
                self.store
                    .update_ticker(&ticker.symbol, |target| ticker.apply(target));
            }
        }
        // A venue may have revalued its account using REST prices before restoration.
        let mut cx = Cx {
            now,
            store: &mut self.store,
            io: &mut self.io,
        };
        self.venue.on_public_feed(&mut cx);
        self.flush_http_owner_retirements();
        self.shared_ticker_revision = self.store.ticker_revision();
    }

    /// Events below `level` are dropped before they are built; defaults to Info.
    pub fn set_log_level(&mut self, level: Level) {
        self.io.core.min_level = level;
    }

    pub fn health(&self) -> &Health {
        &self.io.core.health
    }

    /// Native hosts surface a rejected trusted-provider update as an ordinary notice.
    pub fn report_public_feed_error(&mut self, error: &str) {
        push_event(
            &mut self.io.core.actions,
            &mut self.io.core.health,
            self.io.core.min_level,
            LogEvent {
                level: Level::Error,
                code: log_code::PUBLIC_FEED_REJECTED,
                message: error.to_owned(),
                fields: Vec::new(),
            },
        );
    }

    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.assert_retirement_safe();
        self.assert_command_entry_safe();
        if self.public_feed_policy() == PublicFeedPolicy::Shared {
            Box::new(
                self.shared_books
                    .iter()
                    .map(|(symbol, book)| (symbol.as_str(), book)),
            )
        } else {
            self.venue.books()
        }
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn store_mut(&mut self) -> &mut Store {
        &mut self.store
    }

    pub fn venue(&self) -> &V {
        self.assert_retirement_safe();
        self.assert_command_entry_safe();
        &self.venue
    }

    /// Networked hosts opt into level deltas. Browser consumers retain full
    /// books by default. Switching modes starts a fresh full-book baseline.
    #[cfg(feature = "book-deltas")]
    pub fn set_book_deltas(&mut self, enabled: bool) {
        self.assert_retirement_safe();
        self.assert_command_entry_safe();
        if self.book_deltas != enabled {
            self.book_deltas = enabled;
            self.book_baselines.clear();
            self.book_market_rows.clear();
            let symbols: Vec<_> = self.books().map(|(symbol, _)| symbol.to_owned()).collect();
            for symbol in symbols {
                self.request_book_resync(&symbol);
            }
        }
    }

    /// Force a full BOOK on the next flush, including when another client
    /// joins an existing upstream subscription.
    #[cfg(feature = "book-deltas")]
    pub fn request_book_resync(&mut self, symbol: &str) {
        self.assert_live();
        self.book_baselines.remove(symbol);
        let mut cx = Cx {
            now: 0.0,
            store: &mut self.store,
            io: &mut self.io,
        };
        cx.book_changed(symbol);
    }

    /// Hosts that allocate command IDs require a fresh command boundary.
    #[cfg(feature = "command-tracking")]
    pub fn has_received_commands(&self) -> bool {
        self.commands_received
    }

    pub fn capabilities(&self) -> Capabilities {
        self.assert_retirement_safe();
        self.assert_command_entry_safe();
        self.venue.capabilities()
    }

    /// Whether the venue start boundary has already run.
    pub fn has_started(&self) -> bool {
        self.started
    }

    pub fn start(&mut self, now: f64) {
        self.assert_live();
        self.io.core.dispatch_now = now;
        self.expire_read_roots(now);
        if std::mem::replace(&mut self.started, true) {
            return;
        }
        #[cfg(feature = "address-nonce")]
        self.seal_address_setup();
        let mut cx = Cx {
            now,
            store: &mut self.store,
            io: &mut self.io,
        };
        self.venue.start(&mut cx);
        self.process_socket_work(now);
        self.flush_http_owner_retirements();
        self.flush_http_plans(now);
        self.startup_watch_at = Some(now + STARTUP_WATCH_MS);
        self.restore_shared_tickers(false, now);
        self.flush_http_owner_retirements();
    }

    pub fn handle(&mut self, now: f64, event: Event<'_>) {
        // A poisoned engine traps before any expiry cleanup; then a due root
        // is sealed before this receipt can find it still live.
        self.assert_live();
        self.expire_read_roots(now);
        // An external command receipt gets the SDK default caller wait.
        if let Event::Command(command) = &event
            && let Some(id) = command.completion_id()
        {
            let deadline =
                command_entry::wait_deadline(now, command_entry::DEFAULT_COMMAND_WAIT_MS);
            self.io.receive_command_wait(id, deadline, now);
        }
        self.handle_received(now, event);
        self.io.core.command_waits.end_receipt();
        #[cfg(feature = "repair-mutations")]
        self.flush_repair_notices(now);
    }

    /// Dispatch one host command with the caller's own wait. A non-finite
    /// clock, an out-of-range wait or a deadline that does not advance is
    /// refused with nothing dispatched.
    pub fn handle_command(
        &mut self,
        now: f64,
        command: Command,
        wait_ms: Option<u32>,
    ) -> Result<(), crate::request_context::Error> {
        let wait_ms = wait_ms.unwrap_or(command_entry::DEFAULT_COMMAND_WAIT_MS);
        if !(1..=command_entry::MAX_COMMAND_WAIT_MS).contains(&wait_ms) {
            return Err(crate::request_context::Error::Invalid);
        }
        let deadline = command_entry::wait_deadline(now, wait_ms)
            .ok_or(crate::request_context::Error::Invalid)?;
        self.assert_live();
        self.expire_read_roots(now);
        if let Some(id) = command.completion_id() {
            self.io.receive_command_wait(id, Some(deadline), now);
        }
        self.handle_received(now, Event::Command(command));
        self.io.core.command_waits.end_receipt();
        #[cfg(feature = "repair-mutations")]
        self.flush_repair_notices(now);
        Ok(())
    }

    fn handle_received(&mut self, now: f64, event: Event<'_>) {
        self.assert_live();
        self.io.core.dispatch_now = now;
        #[cfg(feature = "address-nonce")]
        {
            self.poll_address_setup(
                now,
                &mut std::task::Context::from_waker(std::task::Waker::noop()),
            );
            if let Event::Signed { attempt, result } = &event
                && self.address_setup_signed(now, *attempt, result.clone())
            {
                return;
            }
            if let Event::TimerFired { timer } = &event
                && self.address_setup_timer(now, *timer)
            {
                return;
            }
        }
        self.expire_read_roots(now);
        if let Event::TimerFired { timer } = &event
            && self.io.read_root_timer(*timer, now)
        {
            return;
        }
        #[cfg(feature = "write-preflight-wait")]
        if let Event::TimerFired { timer } = &event
            && self.io.write_preflight_timer(*timer, now)
        {
            return;
        }
        self.process_socket_work(now);
        self.io.core.http_responses.prune(now);
        // Expired observers detach before any delivery can project to them.
        self.sweep_executions(now);
        if self.expire_plan_timer(now, &event) {
            self.flush_preflight_readiness(now);
            return;
        }
        if self.expire_http_timer(now, &event) {
            self.sweep_executions(now);
            if self.venue.cancel_needs_stored_order() {
                self.release_held(now);
            }
            self.flush_preflight_readiness(now);
            return;
        }
        if let Event::TimerFired { timer } = &event
            && self
                .io
                .core
                .execution_timer
                .is_some_and(|(own, _)| own == *timer)
        {
            self.io.core.execution_timer = None;
            self.sweep_executions(now);
            self.flush_preflight_readiness(now);
            return;
        }
        let admitted = self.admit_http_event(now, &event);
        if self.collect_write_response(now, &event, admitted) {
            self.flush_preflight_readiness(now);
            return;
        }
        if !admitted {
            if self.venue.cancel_needs_stored_order() {
                self.release_held(now);
            }
            self.flush_preflight_readiness(now);
            return;
        }
        let completed_http = match &event {
            Event::HttpResponse { request, .. }
            | Event::HttpEnd { request }
            | Event::HttpFailed { request, .. } => Some(*request),
            _ => None,
        };
        if let Event::Signed { attempt, result } = event {
            if !self.io.core.signing.is_managed(attempt) || self.signing_pending(attempt) {
                self.sign_completed(now, attempt, result);
            }
            if self.venue.cancel_needs_stored_order() {
                self.release_held(now);
            }
            #[cfg(feature = "order-locks")]
            self.reconcile_uncertain(now);
            self.flush_preflight_readiness(now);
            return;
        }
        if let Event::TimerFired { timer } = &event
            && let Some((_, pending)) = self.io.core.signing.expire(*timer)
        {
            self.sign_failed(now, pending.outbound, crate::signing::SignError::TimedOut);
            if self.venue.cancel_needs_stored_order() {
                self.release_held(now);
            }
            #[cfg(feature = "order-locks")]
            self.reconcile_uncertain(now);
            self.flush_preflight_readiness(now);
            return;
        }
        #[cfg(feature = "order-locks")]
        if let Event::TimerFired { timer } = &event
            && self.reconcile_timer.is_some_and(|(own, _)| own == *timer)
        {
            self.reconcile_timer = None;
            // The full end-of-event path: deferred commands, the reconcile
            // drain itself and shared tickers.
            self.finish_event_scope(now);
            if self.venue.cancel_needs_stored_order() {
                self.release_held(now);
            }
            self.flush_preflight_readiness(now);
            return;
        }
        if let Event::TimerFired { timer } = &event
            && self.ack_timer.is_some_and(|(own, _)| own == *timer)
        {
            self.ack_timer = None;
        } else {
            self.handle_event(now, event);
        }
        if self.venue.cancel_needs_stored_order() {
            self.release_held(now);
        }
        if let Some(request) = completed_http {
            self.settle_http_event(request);
            self.sweep_executions(now);
        }
        self.flush_preflight_readiness(now);
    }

    fn sweep_executions(&mut self, now: f64) {
        let mut cx = Cx {
            now,
            store: &mut self.store,
            io: &mut self.io,
        };
        cx.sweep_executions();
        #[cfg(feature = "http-plans")]
        cx.fail_orphaned_plans();
    }

    /// Dispatch held cancellations whose acknowledged ids are now stored, or
    /// whose wait expired, then re-arm the wake-up for the remaining ones.
    fn release_held(&mut self, now: f64) {
        // Bookkeeping reads this event's outcomes here rather than in `Cx`, so
        // venues that do not opt in compile none of it.
        let io = &mut self.io;
        io.core
            .placement_acks
            .observe(&io.core.actions, &io.core.fills, now);
        let released = self.io.core.placement_acks.release(&self.store, now);
        if !released.is_empty() {
            // A nested release (order-lock `ready` re-entering `handle`) keeps the flag.
            let was_releasing = std::mem::replace(&mut self.releasing, true);
            for (_, receipt) in &released {
                self.io.core.command_waits.batch(*receipt);
            }
            for (command, receipt) in released {
                let carried = command.completion_id().map(|id| (id, receipt));
                let outer = self.io.core.command_waits.carry(carried);
                self.handle_event(now, Event::Command(command));
                self.io.core.command_waits.carry(outer);
            }
            self.releasing = was_releasing;
        }
        let next = self.io.core.placement_acks.next_deadline(&self.store, now);
        if self.ack_timer.map(|(_, at)| at) == next {
            return;
        }
        if let Some((timer, _)) = self.ack_timer.take() {
            self.io.core.actions.push(Action::ClearTimer { timer });
        }
        if let Some(at_ms) = next {
            let timer = TimerId(self.io.next());
            self.io.core.actions.push(Action::SetTimer { timer, at_ms });
            self.ack_timer = Some((timer, at_ms));
        }
    }

    // Managed and legacy text use the same native callback boundary.
    #[cfg(feature = "managed-sockets")]
    #[inline(never)]
    fn deliver_socket_message(
        venue: &mut V,
        cx: &mut Cx<'_, V::Pending>,
        socket: SocketId,
        text: &str,
    ) {
        venue.on_message(cx, socket, text);
    }

    fn handle_event(&mut self, now: f64, event: Event<'_>) {
        self.handle_event_with_read_timeout(now, event, false);
    }

    fn handle_event_with_read_timeout(
        &mut self,
        now: f64,
        event: Event<'_>,
        physical_timeout: bool,
    ) {
        #[cfg(feature = "http-plans")]
        {
            self.io.core.http_history_failure = None;
        }
        let request = match &event {
            Event::HttpResponse { request, .. }
            | Event::HttpStart { request, .. }
            | Event::HttpChunk { request, .. }
            | Event::HttpEnd { request }
            | Event::HttpFailed { request, .. } => Some(*request),
            _ => None,
        };
        let delivery = request
            .and_then(|request| self.io.core.managed_http.get(&request))
            .map(|managed| managed.delivery());
        if let Event::HttpFailed { error, .. } = &event
            && let Some(delivery) = delivery.as_ref()
            && delivery.context.operation() == crate::request_context::Operation::Mutation
        {
            let _ = self.io.core.request_contexts.result(
                delivery,
                crate::request_context::WriteResult::Failed(
                    VenueError::new(*error).kind(self.io.core.http_failure_kind),
                ),
            );
        }
        if let Some(delivery) = delivery.as_ref() {
            self.io.core.request_contexts.enter_delivery(delivery);
        }
        let previous = std::mem::replace(&mut self.io.core.http_delivery, delivery.clone());
        #[cfg(feature = "managed-sockets")]
        let socket_message = self.io.core.socket_message.take();
        #[cfg(feature = "repair-mutations")]
        let previous_data = self.begin_repair_data(match (&event, &delivery) {
            (Event::HttpResponse { request, .. } | Event::HttpEnd { request }, Some(_)) => {
                Some(*request)
            }
            _ => None,
        });
        self.handle_event_scoped(now, event, physical_timeout);
        #[cfg(feature = "repair-mutations")]
        self.end_repair_data(previous_data);
        #[cfg(feature = "http-plans")]
        {
            self.io.core.http_history_failure = None;
        }
        self.io.core.http_delivery = previous;
        #[cfg(feature = "managed-sockets")]
        {
            self.io.core.socket_message = socket_message;
        }
        if let Some(delivery) = delivery.as_ref() {
            self.io.core.request_contexts.exit_delivery(delivery);
        }
        #[cfg(feature = "repair-mutations")]
        if let Some(delivery) = delivery.as_ref() {
            self.repair_delivered(delivery, now);
        }
        self.flush_http_plans(now);
    }

    fn duplicate_command(&mut self) {
        push_event(
            &mut self.io.core.actions,
            &mut self.io.core.health,
            self.io.core.min_level,
            LogEvent {
                level: Level::Error,
                code: log_code::COMMAND_DUPLICATE,
                message: "Command ID is already in flight".into(),
                fields: Vec::new(),
            },
        );
    }

    fn handle_event_scoped(&mut self, now: f64, mut event: Event<'_>, _physical_timeout: bool) {
        // A new event cannot borrow an in-flight managed command's generation.
        // Sibling legs originate under Cx authority, not a repeated wire command.
        if let Event::Command(command) = &event
            && let Some(id) = command.completion_id()
            && self.io.core.request_contexts.command_live(id, now)
        {
            self.duplicate_command();
            return;
        }
        if self.venue.cancel_needs_stored_order() && !self.releasing {
            if let Event::Command(Command::PlaceOrders { command, .. }) = &event {
                self.io.core.placement_acks.placing(*command);
            } else if matches!(
                &event,
                Event::Command(Command::CancelOrders { .. } | Command::AmendOrder { .. })
            ) {
                // Moved, not cloned, into the hold.
                let Event::Command(command) = event else {
                    unreachable!()
                };
                let receipt = self.io.core.command_waits.receipt_of(&command);
                match self
                    .io
                    .core
                    .placement_acks
                    .hold(&self.store, command, receipt, now)
                {
                    Ok(()) => {
                        self.release_held(now);
                        return;
                    }
                    Err(command) => event = Event::Command(command),
                }
            }
        }
        if self.public_feed_policy() != PublicFeedPolicy::Direct
            && matches!(&event, Event::WsOpened { socket } | Event::WsBinary { socket, .. }
            | Event::WsMessage { socket, .. } | Event::WsClosed { socket, .. }
            | Event::WsRouteExhausted { socket } if socket.0 == 0)
        {
            return;
        }
        #[cfg(feature = "order-locks")]
        if self.venue.guard_order_mutations()
            && let Event::Command(command) = &event
            && let Some(id) = command.completion_id()
            && !self.io.core.active_commands.insert(id)
        {
            self.duplicate_command();
            return;
        }
        self.begin_event_scope();
        #[cfg(feature = "command-tracking")]
        if matches!(&event, Event::Command(_)) {
            self.commands_received = true;
        }
        #[cfg(feature = "book-deltas")]
        match &event {
            Event::Command(Command::SubscribeOrderBook { symbol }) => {
                self.request_book_resync(symbol);
            }
            Event::Command(Command::UnsubscribeOrderBook { symbol }) => {
                self.book_baselines.remove(symbol);
            }
            _ => {}
        }
        if self.public_feed_policy() != PublicFeedPolicy::Direct {
            match &event {
                Event::Command(Command::UnsubscribeOrderBook { symbol }) => {
                    self.shared_books.remove(symbol);
                    return;
                }
                Event::Command(
                    Command::SetTickerSymbols { .. }
                    | Command::SubscribeOrderBook { .. }
                    | Command::SubscribeCandles { .. }
                    | Command::UnsubscribeCandles { .. },
                ) => return,
                _ => {}
            }
        }

        let mut cx = Cx {
            now,
            store: &mut self.store,
            io: &mut self.io,
        };
        if let Some(at) = self.startup_watch_at
            && now >= at
        {
            self.startup_watch_at =
                watch_startup(&mut cx, self.account_startup).then_some(now + STARTUP_REPEAT_MS);
        }
        #[cfg(feature = "managed-sockets")]
        if V::managed_sockets() && self.io.managed_socket_active() {
            // Managed ingress shares the ordinary event prelude and epilogue.
            // In particular health accounting is reset once per host event.
            if self.handle_managed_socket(now, &event) {
                self.finish_event_scope(now);
                return;
            }
        }
        let mut cx = Cx {
            now,
            store: &mut self.store,
            io: &mut self.io,
        };
        match event {
            Event::PollScaleChanged { scale } => {
                if scale.is_nan() || scale <= 0. {
                    return;
                }
                let (polls, mut sink) = cx.io.polls();
                let ceiling = polls.ceiling_ms;
                polls.set_policy(now, Some(scale), ceiling, &mut sink);
                self.venue.on_poll_scale(&mut cx, scale)
            }
            Event::PollCeilingChanged { ceiling_ms } => {
                let ceiling_ms = ceiling_ms.filter(|ms| ms.is_finite() && *ms > 0.);
                let (polls, mut sink) = cx.io.polls();
                let scale = polls.scale;
                polls.set_policy(now, scale, ceiling_ms, &mut sink);
            }
            Event::WsOpened { socket } => {
                cx.io.core.health.ws_opened = cx.io.core.health.ws_opened.saturating_add(1);
                self.venue.on_open(&mut cx, socket)
            }
            Event::WsBinary { socket, data } => {
                cx.io
                    .core
                    .sockets
                    .received(socket, now, &mut cx.io.core.health);
                self.venue.on_binary(&mut cx, socket, data)
            }
            Event::WsMessage { socket, text } => {
                cx.io
                    .core
                    .sockets
                    .received(socket, now, &mut cx.io.core.health);
                // Only managed venues pay for the shared non-inlined delivery boundary.
                #[cfg(feature = "managed-sockets")]
                if V::managed_sockets() {
                    Self::deliver_socket_message(&mut self.venue, &mut cx, socket, text);
                } else {
                    self.venue.on_message(&mut cx, socket, text);
                }
                #[cfg(not(feature = "managed-sockets"))]
                self.venue.on_message(&mut cx, socket, text);
            }
            // One terminal body for both: exhaustion is a close unless the venue opts in.
            Event::WsClosed { socket, .. } | Event::WsRouteExhausted { socket } => {
                let (code, exhausted) = match event {
                    Event::WsClosed { code, .. } => (code, false),
                    _ => (ROUTE_EXHAUSTED_CLOSE, true),
                };
                for timer in cx.io.core.signing.close_socket(socket) {
                    if timer.0 != 0 {
                        cx.io.core.actions.push(Action::ClearTimer { timer });
                    }
                }
                cx.io.core.sockets.closed(socket, true);
                if !exhausted {
                    cx.io.core.health.count(log_code::WS_CLOSED);
                }
                if exhausted && V::route_exhaustion() {
                    self.venue.on_route_exhausted(&mut cx, socket)
                } else {
                    self.venue.on_close(&mut cx, socket, code)
                }
            }
            Event::HttpResponse { request, response } => {
                self.http_start(now, request, response.status, response.headers);
                self.http_chunk(now, request, response.body.as_bytes());
                self.http_end(now, request);
            }
            Event::HttpStart {
                request,
                status,
                headers,
            } => self.http_start(now, request, status, headers),
            Event::HttpChunk { request, data } => self.http_chunk(now, request, data),
            Event::HttpEnd { request } => self.http_end(now, request),
            Event::Signed { .. } => unreachable!("handled before venue dispatch"),
            Event::HttpFailed { request, error } => {
                #[cfg(feature = "order-locks")]
                cx.io.core.order_read_tokens.remove(&request);
                if let Some(pending) = cx.io.core.signing.remove_request(request)
                    && pending.timer.0 != 0
                {
                    cx.io.core.actions.push(Action::ClearTimer {
                        timer: pending.timer,
                    });
                }
                cx.io.core.streams.remove(&request);
                cx.io.core.incoming.remove(&request);
                if let Some(pending) = cx.io.pending.remove(&request) {
                    cx.io.core.health.count(log_code::HTTP_FAILED);
                    let kind = cx.io.core.http_failure_kind;
                    let make_failure = || VenueError::new(error).kind(kind);
                    #[cfg(feature = "http-plans")]
                    {
                        use request_context_registry::ReleasedReadFailure;
                        let origin = if kind == ErrorKind::Network {
                            Some(ReleasedReadFailure::Network)
                        } else if _physical_timeout && kind == ErrorKind::Timeout {
                            Some(ReleasedReadFailure::PhysicalTimeout)
                        } else {
                            None
                        };
                        cx.with_read_plan_failure_origin(origin, make_failure, |cx| {
                            self.venue.on_http_failed(cx, pending.value, make_failure());
                        });
                    }
                    #[cfg(not(feature = "http-plans"))]
                    cx.with_owned_read_failure(make_failure, |cx| {
                        self.venue.on_http_failed(cx, pending.value, make_failure());
                    });
                }
            }
            Event::TimerFired { timer } => {
                if cx.io.core.timers.remove(&timer).is_some() {
                    let logical = cx.io.core.polls.fired(timer).unwrap_or(timer);
                    self.venue.on_timer(&mut cx, logical);
                }
            }
            Event::Command(Command::Venue { command, action }) => {
                cx.io.begin_command_entry(Some(command));
                cx.io.core.request_contexts.dispatch =
                    Some((command, crate::classify::Intent::Place));
                self.venue.on_venue_action(&mut cx, command, action);
                cx.io.core.request_contexts.dispatch = None;
                cx.io.end_command_entry();
            }
            Event::Command(Command::FetchAccount { command })
                if self.venue.capabilities().account == AccountLookup::Unsupported =>
            {
                cx.refuse_before_entry(command, UNSUPPORTED_VENUE_ACTION);
            }
            Event::Command(Command::AmendOrderWithSymbol { command, .. })
                if !self.venue.capabilities().amend_with_symbol =>
            {
                cx.refuse_before_entry(
                    command,
                    "symbol-aware amendment is unsupported by this venue",
                );
            }
            Event::Command(command) => {
                #[cfg(feature = "order-locks")]
                if self.venue.guard_order_mutations() {
                    let id = match &command {
                        Command::PlaceOrders { command, .. }
                        | Command::AmendOrder { command, .. }
                        | Command::AmendOrderWithSymbol { command, .. }
                        | Command::CancelOrders { command, .. }
                        | Command::CancelOrdersWithSymbols { command, .. }
                        | Command::CancelSymbolOrders { command, .. } => Some(*command),
                        _ => None,
                    };
                    if id.is_some_and(|id| cx.io.core.order_locks.contains(id)) {
                        cx.notice(
                            Level::Error,
                            log_code::COMMAND_DUPLICATE,
                            "Order command ID is already in flight",
                        );
                        return;
                    }
                    if cx
                        .io
                        .core
                        .order_locks
                        .defer(&command, cx.io.core.command_waits.receipt_of(&command))
                    {
                        return;
                    }
                    if (!matches!(command, Command::PlaceOrders { .. })
                        || self.venue.guard_pending_placements())
                        && let Err(refusal) = cx.io.core.order_locks.acquire(
                            &command,
                            cx.store,
                            self.venue.fence_uncertain_cancel_all(),
                        )
                    {
                        let error = match (refusal, &command) {
                            (crate::order_locks::Refusal::Capacity(_), _) => {
                                VenueError::new(crate::order_locks::CAPACITY_REACHED)
                                    .kind(ErrorKind::Internal)
                                    .outcome(crate::Outcome::NotSent)
                            }
                            (crate::order_locks::Refusal::Uncertain(_), _) => {
                                VenueError::new(UNCERTAIN_PLACEMENT)
                                    .outcome(crate::Outcome::NotSent)
                            }
                            (_, Command::CancelSymbolOrders { symbol, .. })
                                if self.venue.fence_uncertain_cancel_all()
                                    && cx.io.core.order_locks.uncertain(symbol) =>
                            {
                                VenueError::new(UNCERTAIN_CANCEL_ALL)
                                    .outcome(crate::Outcome::NotSent)
                            }
                            _ => "Order amendment or cancellation is already in flight".into(),
                        };
                        let (crate::order_locks::Refusal::Conflict(id)
                        | crate::order_locks::Refusal::Capacity(id)
                        | crate::order_locks::Refusal::Uncertain(id)) = refusal;
                        cx.refuse_before_entry(id, error);
                        return;
                    }
                }
                // A command no lock scope holds is still fenced (cancel-all as the
                // venue declares), and a placement or amend is marked so its own
                // Unknown outcome fences its symbols.
                #[cfg(feature = "order-locks")]
                if !(self.venue.guard_order_mutations()
                    && (!matches!(command, Command::PlaceOrders { .. })
                        || self.venue.guard_pending_placements()))
                    && let Err(id) = cx.io.core.order_locks.admit_unscoped(
                        &command,
                        cx.store,
                        self.venue.fence_uncertain_cancel_all(),
                    )
                {
                    let message = if matches!(command, Command::CancelSymbolOrders { .. }) {
                        UNCERTAIN_CANCEL_ALL
                    } else {
                        UNCERTAIN_PLACEMENT
                    };
                    cx.refuse_before_entry(
                        id,
                        VenueError::new(message).outcome(crate::Outcome::NotSent),
                    );
                    return;
                }
                if !self.venue.capabilities().options {
                    let unsupported = match &command {
                        Command::FetchOptionChain { command, .. }
                        | Command::FetchOptionExpiries { command, .. }
                        | Command::FetchAccountGreeks { command } => Some(*command),
                        Command::PlaceOrders { command, orders }
                            if orders
                                .iter()
                                .any(|order| order.implied_volatility.is_some()) =>
                        {
                            Some(*command)
                        }
                        _ => None,
                    };
                    if let Some(command) = unsupported {
                        cx.refuse_before_entry(command, "Options are not enabled on this account");
                        return;
                    }
                }
                cx.io.begin_command_entry(command.completion_id());
                // The classifier takes intent from the command, never the venue
                // (an amend's cancel leg is still an amend).
                cx.io.core.request_contexts.dispatch = command
                    .completion_id()
                    .map(|id| (id, command.write_intent()));
                self.venue.on_command(&mut cx, command);
                cx.io.core.request_contexts.dispatch = None;
                cx.io.end_command_entry();
            }
        }
        self.finish_event_scope(now);
    }

    fn begin_event_scope(&mut self) {
        #[cfg(feature = "order-locks")]
        {
            self.io.core.order_response_token = None;
        }
        self.io.core.auth_failure_counted = false;
    }

    fn count_http_status(&mut self, status: u16) {
        if status == 429 {
            self.io.core.health.count(log_code::RATE_LIMITED);
        } else if status >= 500 {
            self.io.core.health.count(log_code::HTTP_FAILED);
        }
    }

    fn finish_event_scope(&mut self, now: f64) {
        #[cfg(feature = "http-plans")]
        self.io.core.request_contexts.end_preflight_admission();
        self.io.core.request_contexts.end_plan_admission();
        self.flush_bearer_fences(now);
        self.flush_http_owner_retirements();
        self.finish_deferred_commands(now);
        self.restore_shared_tickers(false, now);
        self.flush_http_owner_retirements();
        self.flush_http_plans(now);
    }

    #[inline(never)]
    fn finish_deferred_commands(&mut self, now: f64) {
        #[cfg(not(feature = "order-locks"))]
        let _ = now;
        // Deferred commands are engine-owned work, not part of the HTTP codec
        // whose callback just released the lock. Do not lend that codec's
        // attempt authority to an unrelated internal finalization.
        #[cfg(feature = "http-plans")]
        {
            self.io.core.http_history_failure = None;
        }
        let delivery = self.io.core.http_delivery.take();
        let entry = self.io.core.command_entry.take();
        #[cfg(feature = "order-locks")]
        let order_response = self.io.core.order_response_token.take();
        #[cfg(feature = "order-locks")]
        let ready = self.io.core.order_locks.ready();
        #[cfg(feature = "order-locks")]
        for (.., receipt) in &ready {
            self.io.core.command_waits.batch(*receipt);
        }
        #[cfg(feature = "order-locks")]
        for (command, failed, receipt) in ready {
            let carried = command.completion_id().map(|id| (id, receipt));
            let outer = self.io.core.command_waits.carry(carried);
            if failed && self.venue.fence_uncertain_cancel_all() {
                let Command::CancelSymbolOrders { command, .. } = command else {
                    unreachable!()
                };
                let mut cx = Cx {
                    now,
                    store: &mut self.store,
                    io: &mut self.io,
                };
                cx.refuse_before_entry(
                    command,
                    VenueError::new(UNCERTAIN_CANCEL_ALL).outcome(crate::Outcome::NotSent),
                );
            } else {
                if let Some(id) = command.completion_id() {
                    self.io.core.active_commands.remove(&id);
                }
                // Not a new receipt: it keeps the receipt it was deferred with.
                self.handle_received(now, Event::Command(command));
            }
            self.io.core.command_waits.carry(outer);
        }
        self.process_socket_work(now);
        self.io.core.http_delivery = delivery;
        self.io.core.command_entry = entry;
        #[cfg(feature = "order-locks")]
        {
            self.io.core.order_response_token = order_response;
        }
        #[cfg(feature = "order-locks")]
        self.reconcile_uncertain(now);
        self.restore_shared_tickers(false, now);
    }

    /// Ask the venue once for each due uncertain-write reconcile read, then
    /// re-arm one wake-up for deferred ones. A fresh read the venue reports
    /// through `orders_reconciled` clears the order-lock fence as before.
    // Out of line: its callers share one copy (wasm size budget).
    #[cfg(feature = "order-locks")]
    #[inline(never)]
    fn reconcile_uncertain(&mut self, now: f64) {
        // Never inside a command entry; the hook runs with no HTTP delivery
        // or order-read authority of its own.
        if self.io.core.command_entry.is_some() {
            return;
        }
        let (mut any, mut all, mut symbols) = (false, false, Vec::<String>::new());
        self.io.core.reconcile_due.retain_mut(|(scope, at)| {
            if *at > now && now.is_finite() {
                return true;
            }
            any = true;
            match scope.take() {
                None => all = true,
                Some(scope) => {
                    for symbol in scope {
                        if !symbols.contains(&symbol) {
                            symbols.push(symbol);
                        }
                    }
                }
            }
            false
        });
        if any {
            #[cfg(feature = "http-plans")]
            {
                self.io.core.http_history_failure = None;
            }
            let delivery = self.io.core.http_delivery.take();
            let token = self.io.core.order_response_token.take();
            let mut cx = Cx {
                now,
                store: &mut self.store,
                io: &mut self.io,
            };
            self.venue
                .reconcile_uncertain(&mut cx, (!all).then_some(symbols.as_slice()));
            self.io.core.http_delivery = delivery;
            self.io.core.order_response_token = token;
        }
        // A failed or lost reconcile read must not leave the fence standing: re-ask
        // with bounded backoff (1 s doubling to 30 s) until a fresh read clears it.
        // A new seal restarts the backoff, since its own read has just been asked for.
        match self.io.core.order_locks.fence_reread() {
            None => self.io.core.fence_reread = None,
            Some(scope) => {
                let (at, attempts) = self.io.core.fence_reread.unwrap_or((f64::NEG_INFINITY, 0));
                let attempts = if any { 0 } else { attempts };
                if any || at <= now {
                    if !any {
                        let delivery = self.io.core.http_delivery.take();
                        let token = self.io.core.order_response_token.take();
                        let mut cx = Cx {
                            now,
                            store: &mut self.store,
                            io: &mut self.io,
                        };
                        cx.io.core.reconcile_reask = true;
                        self.venue.reconcile_uncertain(&mut cx, scope.as_deref());
                        self.io.core.reconcile_reask = false;
                        self.io.core.http_delivery = delivery;
                        self.io.core.order_response_token = token;
                    }
                    let delay = (1_000. * f64::from(1u32 << attempts.min(5))).min(30_000.);
                    self.io.core.fence_reread = Some((now + delay, attempts.saturating_add(1)));
                }
            }
        }
        let next = self
            .io
            .core
            .reconcile_due
            .iter()
            .map(|(_, at)| *at)
            .chain(self.io.core.fence_reread.map(|(at, _)| at))
            .min_by(f64::total_cmp);
        if self.reconcile_timer.map(|(_, at)| at) == next {
            return;
        }
        if let Some((timer, _)) = self.reconcile_timer.take() {
            self.io.core.actions.push(Action::ClearTimer { timer });
        }
        if let Some(at_ms) = next {
            let timer = TimerId(self.io.next());
            self.io.core.actions.push(Action::SetTimer { timer, at_ms });
            self.reconcile_timer = Some((timer, at_ms));
        }
    }

    fn http_start(&mut self, _now: f64, request: RequestId, status: u16, headers: &str) {
        if !self.io.pending.contains_key(&request) {
            return;
        }
        let stream = self.io.core.streams.remove(&request);
        let splitter = stream
            .filter(|_| (200..300).contains(&status))
            .map(|(at, check)| ArraySplitter::with_item_check(at, check));
        self.io.core.incoming.insert(
            request,
            Incoming {
                status,
                headers: headers.to_owned(),
                splitter,
                body: Vec::new(),
            },
        );
    }

    fn http_chunk(&mut self, now: f64, request: RequestId, data: &[u8]) {
        let managed = self.io.managed_preparation(request);
        let Some(incoming) = self.io.core.incoming.get_mut(&request) else {
            return;
        };
        let Some(splitter) = incoming.splitter.as_mut() else {
            incoming.body.extend_from_slice(data);
            return;
        };
        // The continuation leaves the map while the venue holds it.
        let Some(mut pending) = self.io.pending.remove(&request) else {
            return;
        };
        let mut splitter = std::mem::replace(splitter, ArraySplitter::new(ArrayAt::Root));
        let mut cx = Cx {
            now,
            store: &mut self.store,
            io: &mut self.io,
        };
        let venue = &mut self.venue;
        splitter.feed(data, |item| {
            if managed.is_none_or(|token| cx.io.managed_response_live(request, token, now)) {
                venue.on_item(&mut cx, &mut pending.value, item)
            }
        });
        if managed.is_none_or(|token| self.io.managed_response_live(request, token, now)) {
            self.io.pending.insert(request, pending);
            if let Some(incoming) = self.io.core.incoming.get_mut(&request) {
                incoming.splitter = Some(splitter);
            }
        }
    }

    fn http_end(&mut self, now: f64, request: RequestId) {
        #[cfg(feature = "order-locks")]
        {
            self.io.core.order_response_token = self.io.core.order_read_tokens.remove(&request);
        }
        let (Some(incoming), Some(pending)) = (
            self.io.core.incoming.remove(&request),
            self.io.pending.remove(&request),
        ) else {
            return;
        };
        self.count_http_status(incoming.status);
        let mut cx = Cx {
            now,
            store: &mut self.store,
            io: &mut self.io,
        };
        // A 2xx without the expected array (an error envelope, a truncated
        // body) must never read as an authoritative empty list.
        if let Some(splitter) = &incoming.splitter {
            let outside = splitter.outside();
            if !splitter.finished() {
                let body = splitter.envelope();
                if serde_json::from_str::<serde::de::IgnoredAny>(body).is_ok() {
                    let response = Response {
                        status: incoming.status,
                        headers: &incoming.headers,
                        body,
                    };
                    observe_account_auth(
                        &self.venue,
                        &pending,
                        &response,
                        false,
                        now,
                        cx.store,
                        &mut cx.io.core.actions,
                    );
                    cx.with_owned_read_failure(
                        || {
                            VenueError::new("Response did not contain the expected list")
                                .kind(ErrorKind::Internal)
                        },
                        |cx| self.venue.on_stream_error(cx, pending.value, response),
                    );
                    return;
                }
                let preview = &outside[..outside.len().min(STREAM_PREVIEW_BYTES)];
                let preview = String::from_utf8_lossy(preview);
                let message = format!("response did not contain the expected list: {preview}");
                cx.with_owned_read_failure(
                    || VenueError::new(&message).kind(ErrorKind::Internal),
                    |cx| {
                        self.venue.on_stream_malformed(
                            cx,
                            pending.value,
                            incoming.status,
                            &incoming.headers,
                            &message,
                        )
                    },
                );
                return;
            }
            // Every selector retains the fully validated envelope, with its array emptied.
            let body = match std::str::from_utf8(outside) {
                Ok(body) => body,
                Err(_) => {
                    let make_failure = || malformed_body("stream envelope is not UTF-8");
                    cx.with_owned_read_failure(make_failure, |cx| {
                        self.venue.on_http_failed(cx, pending.value, make_failure());
                    });
                    return;
                }
            };
            let response = Response {
                status: incoming.status,
                headers: &incoming.headers,
                body,
            };
            observe_account_auth(
                &self.venue,
                &pending,
                &response,
                true,
                now,
                cx.store,
                &mut cx.io.core.actions,
            );
            self.venue.on_response(&mut cx, pending.value, response);
            return;
        }
        match std::str::from_utf8(&incoming.body) {
            Ok(body) => {
                let response = Response {
                    status: incoming.status,
                    headers: &incoming.headers,
                    body,
                };
                observe_account_auth(
                    &self.venue,
                    &pending,
                    &response,
                    true,
                    now,
                    cx.store,
                    &mut cx.io.core.actions,
                );
                // Session writes skip the journaled collector, so expose the
                // reply metadata the write classifier reads during this callback.
                let session_write = cx.io.core.http_delivery.as_ref().is_some_and(|delivery| {
                    delivery.context.operation() == crate::request_context::Operation::Mutation
                        && matches!(
                            delivery.context.owner(),
                            crate::request_context::Owner::Session { .. }
                        )
                });
                if session_write {
                    cx.io.core.http_write_reply =
                        Some((response.status, response.headers.to_owned()));
                }
                self.venue.on_response(&mut cx, pending.value, response);
                if session_write {
                    cx.io.core.http_write_reply = None;
                }
            }
            Err(_) => {
                let make_failure = || malformed_body("response body is not UTF-8");
                cx.with_owned_read_failure(make_failure, |cx| {
                    self.venue.on_http_failed(cx, pending.value, make_failure());
                });
            }
        }
    }

    /// The local signer's missing key on primary material latches signing
    /// (`SigningLatch`). An asynchronous result fails only its own request:
    /// no host reports `MissingKey`, and none may stop the account if one did.
    fn provider_refused(
        &mut self,
        work: &crate::signing::SignRequest,
        result: &Result<crate::signing::SignResult, crate::signing::SignError>,
    ) {
        if let Err(crate::signing::SignError::MissingKey) = result {
            let startup = self.startup_watch_at.is_some();
            let io = &mut self.io;
            let primary = io.core.identities.primary_owns(work);
            SigningLatch::missing_key(
                &mut io.core.signing,
                primary,
                startup,
                &mut io.core.actions,
                &mut io.core.health,
                io.core.min_level,
            );
        }
    }

    fn sign_failed(&mut self, now: f64, outbound: Action, error: crate::signing::SignError) {
        let mut cx = Cx {
            now,
            store: &mut self.store,
            io: &mut self.io,
        };
        match outbound {
            Action::Http(http) => {
                #[cfg(feature = "order-locks")]
                cx.io.core.order_read_tokens.remove(&http.id);
                cx.io.core.streams.remove(&http.id);
                if let Some(pending) = cx.io.pending.remove(&http.id) {
                    self.venue
                        .on_sign_error(&mut cx, pending.value, error.venue_error());
                }
            }
            Action::WsSend { socket, .. } | Action::WsConnect { socket, .. } => {
                let io = &mut *cx.io;
                // Records the role; managed feeds also stop on it.
                #[cfg_attr(not(feature = "managed-sockets"), allow(unused_variables))]
                let refused =
                    io.core
                        .signing_latch
                        .socket_failed(&io.core.signing, &io.core.sockets, socket);
                #[cfg(feature = "managed-sockets")]
                if refused
                    && V::managed_sockets()
                    && cx.io.managed_socket_active()
                    && let Some(epoch) = cx.io.core.socket_driver.state.socket_epoch(socket)
                {
                    cx.stop_socket_feed(epoch.feed());
                    self.flush_http_owner_retirements();
                    return;
                }
                if V::managed_sockets() && cx.io.managed_socket_active() {
                    cx.close(socket);
                } else {
                    cx.close(socket);
                    self.venue.on_close(&mut cx, socket, 1006);
                }
            }
            _ => {}
        }
        self.flush_http_owner_retirements();
    }

    fn sign_completed(
        &mut self,
        now: f64,
        attempt: RequestId,
        result: Result<crate::signing::SignResult, crate::signing::SignError>,
    ) {
        use crate::signing_dispatch::Completion;
        #[cfg(feature = "managed-sockets")]
        if let Some((socket, connect)) = self.io.core.signing.outbound_socket(attempt)
            && !self.socket_gate(now, socket, connect)
        {
            return;
        }
        let Some(completion) = self.io.core.signing.finish(
            attempt,
            now,
            self.io.core.protocol_clock.revision(),
            result,
        ) else {
            return;
        };
        if match &completion {
            Completion::Ready(pending, _)
            | Completion::Stale(pending)
            | Completion::NonceStale(pending)
            | Completion::Failed(pending, _) => pending.managed,
        } {
            self.managed_sign_completed(now, completion);
            return;
        }
        match completion {
            Completion::Ready(pending, outbound) => {
                if pending.timer.0 != 0 {
                    self.io.core.actions.push(Action::ClearTimer {
                        timer: pending.timer,
                    });
                }
                self.io.core.signing.ready(attempt, pending);
                self.io.core.actions.push(Action::SignedOutbound {
                    attempt,
                    outbound: Box::new(outbound),
                });
            }
            Completion::Stale(mut pending) | Completion::NonceStale(mut pending) => {
                if pending.timer.0 != 0 {
                    self.io.core.actions.push(Action::ClearTimer {
                        timer: pending.timer,
                    });
                    pending.timer = TimerId(0);
                }
                // New identity invalidates every late completion from the old attempt.
                let next = self.io.core.signing.next_id();
                self.io.core.signing.retry(next, pending);
                self.io.core.actions.push(Action::Sign {
                    attempt: next,
                    work: crate::signing::SignRequest {
                        receipt: Default::default(),
                        operations: Vec::new(),
                        context: Default::default(),
                    },
                    timeout_ms: 0,
                });
            }
            Completion::Failed(pending, error) => {
                if pending.timer.0 != 0 {
                    self.io.core.actions.push(Action::ClearTimer {
                        timer: pending.timer,
                    });
                }
                self.sign_failed(now, pending.outbound, error);
            }
        }
    }

    /// Hosts call this with their canonical engine clock immediately before a
    /// delayed drain. Normal event-driven drains use the current event time.
    /// Provider work may have been drained before its owner was retired.
    /// Hosts check this before starting any delayed primitive operation.
    /// A fatal allocator failure permanently retires this engine.
    pub fn failed(&self) -> bool {
        self.io.core.http_owner_poison
            || self.plan_callback_failed()
            || self.io.core.command_entry.is_some()
            || self.io.core.next_id.failed()
            || self.io.core.signing.failed()
    }

    fn plan_callback_failed(&self) -> bool {
        #[cfg(feature = "http-plans")]
        {
            self.io.core.http_plan_poison
        }
        #[cfg(not(feature = "http-plans"))]
        {
            false
        }
    }

    /// Outer paths without `finish_event_scope` (start, delivery exit,
    /// `drain_actions`) still deliver a retire's Bearer fences.
    #[cfg(not(feature = "http-plans"))]
    fn flush_http_plans(&mut self, now: f64) {
        self.flush_bearer_fences(now);
    }

    #[cfg(not(feature = "http-plans"))]
    fn expire_plan_timer(&mut self, _now: f64, _event: &Event<'_>) -> bool {
        false
    }

    #[cfg(not(feature = "http-plans"))]
    fn flush_http_owner_retirements(&mut self) {
        self.assert_live();
    }

    fn assert_retirement_safe(&self) {
        assert!(
            !self.io.core.http_owner_poison && !self.plan_callback_failed(),
            "engine HTTP owner callback or cleanup failed"
        );
    }

    fn assert_command_entry_safe(&self) {
        assert!(
            self.io.core.command_entry.is_none(),
            "engine command callback did not return"
        );
    }

    fn assert_live(&self) {
        self.assert_retirement_safe();
        self.assert_command_entry_safe();
        assert!(!self.failed(), "engine resource identities exhausted");
    }

    /// Test-only counter of Plan scheduling cells visited by this engine.
    #[cfg(all(feature = "test-support", feature = "http-plans"))]
    #[doc(hidden)]
    pub fn plan_scan_visits(&self) -> usize {
        self.io.core.request_contexts.plan_scan_visits()
    }

    /// A Plan-ready callback, the Plan timer, delivery exit and
    /// `drain_actions` run no `finish_event_scope`: a retire there is fenced
    /// at this tail, not left queued for a later event.
    #[cfg(feature = "http-plans")]
    fn flush_http_plans(&mut self, now: f64) {
        #[cfg(all(feature = "serial-plans", feature = "session-bearer"))]
        self.flush_keyless_session_roots(now);
        if self.io.core.request_contexts.has_plans() {
            self.flush_live_http_plans(now);
        }
        // Bearer fences first, then parked Slots (which drain fences per pass).
        self.flush_bearer_fences(now);
        #[cfg(all(feature = "serial-plans", feature = "session-bearer"))]
        self.flush_keyless_session_roots(now);
        // Last, at every outer Plan flush, including after the final Plan
        // sealed: parked Slots whose Session ended or whose Plan sealed.
        self.flush_plan_session_waits(now);
        // A no-delivery resume's custody ends with its event; every outer
        // path ends here, after any callback in this flush could resume.
        self.io.core.session_resumed.clear();
    }

    #[inline(never)]
    #[cfg(feature = "http-plans")]
    fn flush_live_http_plans(&mut self, now: f64) {
        self.io.core.request_contexts.end_plan_admission();
        if !now.is_finite()
            || self.io.core.http_delivery.is_some()
            || self.io.core.http_plan_ready.is_some()
            || self.io.core.http_plan_expiring.is_some()
            || self.io.core.http_preflight_ready.is_some()
            || !self.io.core.request_contexts.plan_scope_clear()
        {
            return;
        }
        let mut advanced = false;
        while let Some(plan) = self.io.core.request_contexts.next_plan_ready(now) {
            advanced = true;
            if now >= plan.deadline() {
                self.expire_plan_calls(now, &plan);
                Cx {
                    now,
                    store: &mut self.store,
                    io: &mut self.io,
                }
                .fail_plan_owned(
                    &plan,
                    VenueError::new("HTTP plan timed out").kind(ErrorKind::Timeout),
                )
                .expect("current expired plan");
            } else {
                self.io.core.http_plan_ready = Some(plan.clone());
                self.io.core.http_plan_poison = true;
                #[cfg(feature = "order-locks")]
                let order_response = self.io.core.order_response_token.take();
                self.io.core.http_history_failure = None;
                self.venue.on_http_plan_ready(
                    &mut Cx {
                        now,
                        store: &mut self.store,
                        io: &mut self.io,
                    },
                    &plan,
                );
                #[cfg(feature = "order-locks")]
                {
                    self.io.core.order_response_token = order_response;
                }
                self.io.core.http_plan_ready = None;
                self.io.core.http_plan_poison = false;
                self.assert_live();
                request_context_registry::ContextRegistry::rearm_live_plan(&mut self.io, &plan);
            }
            self.flush_http_owner_retirements();
        }
        // A root timeout or a readiness queue refusal can release placement
        // locks. Wake their deferred commands after all readiness callbacks
        // return, without recursively invoking finish_event_scope here.
        if advanced {
            self.finish_deferred_commands(now);
        }
    }

    #[cfg(feature = "http-plans")]
    fn expire_plan_timer(&mut self, now: f64, event: &Event<'_>) -> bool {
        let Event::TimerFired { timer } = event else {
            return false;
        };
        let Some(plan) = self.io.core.request_contexts.plan_for_timer(*timer) else {
            #[cfg(feature = "condition-waits")]
            return self.expire_preflight_condition(now, *timer);
            #[cfg(not(feature = "condition-waits"))]
            return false;
        };
        #[cfg(feature = "condition-waits")]
        self.expire_plan_condition(now, &plan);
        self.flush_http_plans(now);
        request_context_registry::ContextRegistry::rearm_live_plan(&mut self.io, &plan);
        true
    }

    #[cfg(feature = "http-plans")]
    fn flush_http_owner_retirements(&mut self) {
        self.io.core.request_contexts.end_plan_admission();
        self.assert_live();
        if self.io.core.request_contexts.has_retirements() {
            self.flush_pending_http_owner_retirements();
        }
    }

    #[inline(never)]
    #[cfg(feature = "http-plans")]
    fn flush_pending_http_owner_retirements(&mut self) {
        while let Some(token) = self.io.core.request_contexts.next_retirement() {
            self.assert_live();
            if !self.io.core.request_contexts.retirement_current(&token) {
                self.io.core.http_owner_poison = true;
                panic!("invalid HTTP owner retirement token");
            }
            // A caught hook panic permanently fences the engine. No unwind
            // reset guard or fabricated allocator failure is used.
            self.io.core.http_owner_poison = true;
            let wake = self.venue.on_http_owner_retired_preflight(&token);
            assert!(
                self.io.core.request_contexts.complete_retirement(&token),
                "HTTP owner retirement changed during cleanup"
            );
            if let Some(wake) = wake {
                let _ = self.io.core.request_contexts.queue_preflight_ready(&wake);
            }
            self.io.core.http_owner_poison = false;
            self.assert_live();
        }
    }

    pub fn signing_pending(&self, attempt: RequestId) -> bool {
        !self.failed()
            && (self.io.core.signing.signature_pending(attempt) || {
                #[cfg(feature = "address-nonce")]
                {
                    self.io.core.address_setup.current_meta(attempt).is_some()
                }
                #[cfg(not(feature = "address-nonce"))]
                {
                    false
                }
            })
    }

    /// The latest event time, for fixtures that prepare after `handle`.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn dispatch_now(&self) -> f64 {
        self.io.core.dispatch_now
    }

    pub fn prepare_dispatch(&mut self, now: f64) {
        self.assert_live();
        #[cfg(feature = "address-nonce")]
        self.poll_address_setup(
            now,
            &mut std::task::Context::from_waker(std::task::Waker::noop()),
        );
        self.expire_read_roots(now);
        self.flush_http_owner_retirements();
        self.io.core.dispatch_now = now;
        self.flush_http_plans(now);
        self.flush_preflight_readiness(now);
    }

    /// Callback actions must traverse the same queue as the original event.
    /// In particular, an empty signing marker is internal preparation work.
    fn queue_dispatch_callback_actions(
        &mut self,
        queued: &mut std::collections::VecDeque<Action>,
        emitted: usize,
    ) {
        self.assert_live();
        self.flush_http_owner_retirements();
        for action in self.io.core.actions.split_off(emitted).into_iter().rev() {
            queued.push_front(action);
        }
    }

    #[inline(always)]
    fn dispatch_actions(&mut self, now: f64) {
        self.assert_live();
        if self.io.core.actions.is_empty()
            && self.io.core.signing_admission.is_empty()
            && self.io.core.signing.is_empty()
        {
            return;
        }
        self.dispatch_queued_actions(now);
    }

    #[inline(never)]
    fn dispatch_queued_actions(&mut self, now: f64) {
        use crate::signing_dispatch::Completion;
        for (outbound, error) in std::mem::take(&mut self.io.core.signing_admission) {
            self.sign_failed(now, outbound, error);
            self.assert_live();
        }
        #[cfg(feature = "session-bearer")]
        let mut queued = Wiping(std::collections::VecDeque::from(std::mem::take(
            &mut *self.io.core.actions,
        )));
        #[cfg(not(feature = "session-bearer"))]
        let mut queued =
            std::collections::VecDeque::from(std::mem::take(&mut self.io.core.actions));
        loop {
            self.assert_live();
            while let Some(action) = queued.pop_front() {
                self.io.assert_action_safe(&action);
                let emitted = self.io.core.actions.len();
                #[cfg(not(feature = "managed-sockets"))]
                let live = true;
                #[cfg(feature = "managed-sockets")]
                let live = if !(V::managed_sockets() && self.io.managed_socket_active()) {
                    true
                } else {
                    match &action {
                        Action::Sign { attempt, .. } => {
                            self.io.core.signing.outbound_socket(*attempt).is_none_or(
                                |(socket, connect)| self.socket_gate(now, socket, connect),
                            )
                        }
                        Action::SignedOutbound { outbound, .. } => {
                            self.gate_socket_action(now, outbound)
                        }
                        _ => self.gate_socket_action(now, &action),
                    }
                };
                self.queue_dispatch_callback_actions(&mut queued, emitted);
                if !live {
                    continue;
                }
                match action {
                    Action::SignedOutbound { attempt, outbound } => {
                        match self.io.core.signing.release(
                            attempt,
                            *outbound,
                            now,
                            self.io.core.protocol_clock.revision(),
                        ) {
                            Some(Completion::Ready(mut pending, mut outbound)) => {
                                let time = crate::request_context::ReleaseTime {
                                    raw_ms: now,
                                    protocol_ms: self.io.core.protocol_clock.at(now),
                                    revision: self.io.core.protocol_clock.revision(),
                                };
                                let release =
                                    match crate::SigningRelease::new(pending.receipt, time) {
                                        Ok(release) => release,
                                        Err(crate::request_context::Error::Stale) => {
                                            crate::signing_dispatch::wipe(&mut outbound);
                                            pending.timer = TimerId(0);
                                            let next = self.io.core.signing.next_id();
                                            self.io.core.signing.retry(next, pending);
                                            continue;
                                        }
                                        Err(_) => {
                                            crate::signing_dispatch::wipe(&mut outbound);
                                            let emitted = self.io.core.actions.len();
                                            self.sign_failed(
                                                now,
                                                pending.outbound,
                                                crate::signing::SignError::InvalidRequest,
                                            );
                                            self.queue_dispatch_callback_actions(
                                                &mut queued,
                                                emitted,
                                            );
                                            continue;
                                        }
                                    };
                                match &outbound {
                                    Action::Http(http) => {
                                        if let Some(value) = self.io.pending.get_mut(&http.id) {
                                            self.venue.on_signing_release(
                                                SigningTarget::Http {
                                                    request: http.id,
                                                    pending: &mut value.value,
                                                },
                                                release,
                                            );
                                        }
                                    }
                                    Action::WsConnect { socket, .. }
                                    | Action::WsSend { socket, .. } => {
                                        self.venue.on_signing_release(
                                            SigningTarget::Socket { socket: *socket },
                                            release,
                                        );
                                    }
                                    _ => {}
                                }
                                self.io.core.actions.push(outbound);
                            }
                            Some(
                                Completion::Stale(mut pending)
                                | Completion::NonceStale(mut pending),
                            ) => {
                                pending.timer = TimerId(0);
                                let next = self.io.core.signing.next_id();
                                self.io.core.signing.retry(next, pending);
                            }
                            Some(Completion::Failed(pending, error)) => {
                                let emitted = self.io.core.actions.len();
                                self.sign_failed(now, pending.outbound, error);
                                self.queue_dispatch_callback_actions(&mut queued, emitted);
                            }
                            None => {}
                        }
                    }
                    Action::Sign {
                        attempt,
                        work,
                        timeout_ms,
                    } => {
                        if !work.operations.is_empty() {
                            self.io.core.actions.push(Action::Sign {
                                attempt,
                                work,
                                timeout_ms,
                            });
                            continue;
                        }
                        if !self.io.core.signing.can_prepare(attempt) {
                            continue;
                        }
                        let work = match self.io.core.signing.prepare(
                            attempt,
                            now,
                            &crate::DispatchContext::new(
                                self.io.core.protocol_clock.at(now),
                                self.io.core.entropy.as_ref(),
                            ),
                            self.io.core.protocol_clock.revision(),
                            &self.io.core.identities,
                        ) {
                            Ok(work) => work,
                            Err(error) => {
                                let emitted = self.io.core.actions.len();
                                if let Some(primary) =
                                    self.io.core.signing.take_credential_refusal()
                                {
                                    let startup = self.startup_watch_at.is_some();
                                    let io = &mut self.io;
                                    SigningLatch::missing_key(
                                        &mut io.core.signing,
                                        primary,
                                        startup,
                                        &mut io.core.actions,
                                        &mut io.core.health,
                                        io.core.min_level,
                                    );
                                }
                                self.sign_completed(now, attempt, Err(error));
                                self.queue_dispatch_callback_actions(&mut queued, emitted);
                                continue;
                            }
                        };
                        if let Some(result) = self
                            .io
                            .core
                            .signer
                            .as_ref()
                            .and_then(|s| s.sign_ready(&work))
                        {
                            let emitted = self.io.core.actions.len();
                            self.provider_refused(&work, &result);
                            self.sign_completed(now, attempt, result);
                            // Inline local completion retains original mixed action order.
                            self.queue_dispatch_callback_actions(&mut queued, emitted);
                        } else {
                            if self.io.core.signer.as_ref().is_some_and(|s| s.immediate()) {
                                let emitted = self.io.core.actions.len();
                                self.sign_completed(
                                    now,
                                    attempt,
                                    Err(crate::signing::SignError::Unavailable),
                                );
                                self.queue_dispatch_callback_actions(&mut queued, emitted);
                                continue;
                            }
                            if !self.io.core.signing.is_managed(attempt)
                                && self
                                    .io
                                    .core
                                    .signing
                                    .timer(attempt)
                                    .is_some_and(|t| t.0 == 0)
                            {
                                let timer = TimerId(self.io.next());
                                if let Some(at_ms) = self.io.core.signing.arm(attempt, timer) {
                                    self.io.core.actions.push(Action::SetTimer { timer, at_ms });
                                }
                            }
                            let timeout_ms = self.io.core.signing.timeout_ms(attempt, now);
                            self.io.core.actions.push(Action::Sign {
                                attempt,
                                work,
                                timeout_ms,
                            });
                        }
                    }
                    action => self.io.core.actions.push(action),
                }
            }
            self.assert_live();
            if let Some(action) = self.io.core.signing.take_plain() {
                queued.push_back(action);
                continue;
            }
            if let Some(attempt) = self.io.core.signing.next_work() {
                queued.push_back(Action::Sign {
                    attempt,
                    work: crate::signing::SignRequest {
                        receipt: Default::default(),
                        operations: vec![],
                        context: Default::default(),
                    },
                    timeout_ms: 0,
                });
                continue;
            }
            break;
        }
    }

    #[cfg(not(feature = "managed-sockets"))]
    fn process_socket_work(&mut self, _now: f64) {}
    #[cfg(not(feature = "managed-sockets"))]
    fn has_socket_work(&self) -> bool {
        false
    }

    /// Actions produced since the previous drain, in order.
    pub fn drain_actions(&mut self) -> std::vec::Drain<'_, Action> {
        self.assert_live();
        // No clock here: the last observed engine time, never a wall clock.
        self.expire_read_roots(self.io.core.dispatch_now);
        self.flush_http_plans(self.io.core.dispatch_now);
        self.flush_http_owner_retirements();
        self.io.assert_actions_safe();
        self.process_socket_work(self.io.core.dispatch_now);
        self.io.assert_actions_safe();
        self.dispatch_actions(self.io.core.dispatch_now);
        self.process_socket_work(self.io.core.dispatch_now);
        #[cfg(feature = "managed-sockets")]
        if V::managed_sockets() && self.io.managed_socket_active() {
            self.fence_socket_actions(self.io.core.dispatch_now);
            self.dispatch_actions(self.io.core.dispatch_now);
            self.drain_socket_frames(self.io.core.dispatch_now);
            self.process_socket_work(self.io.core.dispatch_now);
            self.dispatch_actions(self.io.core.dispatch_now);
            self.fence_socket_actions(self.io.core.dispatch_now);
        }
        self.flush_preflight_readiness(self.io.core.dispatch_now);
        if self.store.take_rejected_empty_catalogue() {
            push_event(
                &mut self.io.core.actions,
                &mut self.io.core.health,
                self.io.core.min_level,
                LogEvent {
                    level: Level::Warn,
                    code: log_code::SNAPSHOT_DISCARDED,
                    message: EMPTY_CATALOGUE_WARNING.to_owned(),
                    fields: Vec::new(),
                },
            );
        }
        self.flush_http_owner_retirements();
        self.assert_live();
        self.io.assert_actions_safe();
        #[cfg(feature = "managed-sockets")]
        if V::socket_capabilities().handoff && !self.io.socket_continuations.is_empty() {
            self.arm_socket_continuations(self.io.core.dispatch_now);
            self.io.assert_actions_safe();
        }
        self.io.core.actions.drain(..)
    }

    pub fn has_actions(&self) -> bool {
        !self.io.core.actions.is_empty()
            || !self.io.core.signing_admission.is_empty()
            || self.has_socket_work()
            || self.io.core.signing.next_work().is_some()
            || self.store.rejected_empty_catalogue()
    }

    /// The venue's synchronized book for `symbol`, if any.
    pub fn book(&self, symbol: &str) -> Option<&Book> {
        self.assert_retirement_safe();
        self.assert_command_entry_safe();
        if self.public_feed_policy() == PublicFeedPolicy::Shared {
            self.shared_books.get(symbol)
        } else {
            self.venue.book(symbol)
        }
    }

    /// The whole current state as one frame, for a consumer that starts late
    /// or skipped frames (a new tab, a throttled hidden tab). Frames taken
    /// afterwards still apply on top: every value in them is absolute. Delta
    /// hosts must flush their existing stream before snapshotting a new client;
    /// this method deliberately leaves the stream's baseline untouched.
    pub fn snapshot_frame(&mut self, now: f64) -> &[u8] {
        self.assert_live();
        self.expire_read_roots(now);
        let buffer = std::mem::take(&mut self.io.core.snapshot);
        let mut w = FrameWriter::new(buffer, now);
        let everything = self.store.everything();
        self.store.write(&everything, &mut w);
        self.venue.capabilities().write(&mut w);
        for (symbol, book) in self.books() {
            if let Some(market) = self.store.market_index(symbol) {
                // Recovering consumers may have missed the original invalidation frame.
                if self.book(symbol).is_some() {
                    book.write(market, &mut w);
                } else {
                    Book::default().write(market, &mut w);
                }
            }
        }
        self.io.core.snapshot = w.finish();
        &self.io.core.snapshot
    }

    /// Full private sections and any unflushed market catalog, without consuming
    /// changes, queued events or public stream baselines.
    pub fn account_snapshot_frame(&mut self, now: f64) -> &[u8] {
        self.assert_live();
        self.expire_read_roots(now);
        let buffer = std::mem::take(&mut self.io.core.snapshot);
        let mut writer = FrameWriter::new(buffer, now);
        self.store
            .write(&self.store.account_snapshot(), &mut writer);
        writer.begin(section::ACCOUNT_READ);
        writer.end();
        self.io.core.snapshot = writer.finish();
        &self.io.core.snapshot
    }

    /// Write everything that changed since the previous call as one flush
    /// frame. Returns `None` when nothing changed.
    pub fn take_frame(&mut self, now: f64) -> Option<&[u8]> {
        self.assert_live();
        self.expire_read_roots(now);
        self.write_frame(now, true)
    }

    /// Like [`take_frame`](Self::take_frame) but only books, candles and
    /// fills, which consumers want at the venue's book cadence; store
    /// sections wait for the next full frame unless fills are pending, which
    /// travel with the orders they belong to.
    pub fn take_market_frame(&mut self, now: f64) -> Option<&[u8]> {
        self.assert_live();
        self.expire_read_roots(now);
        self.write_frame(now, false)
    }

    fn write_frame(&mut self, now: f64, store: bool) -> Option<&[u8]> {
        // A fill must not reach a consumer before its order: a book-cadence
        // flush carrying fills writes the store sections with them.
        let store = store || !self.io.core.fills.is_empty();
        if !store && self.store.markets_pending() {
            return None;
        }
        #[cfg(feature = "book-deltas")]
        let write_books = {
            let markets_match = !self.book_deltas
                || (self.book_market_rows.len() == self.store.markets().len()
                    && self.book_market_rows.iter().zip(self.store.markets()).all(
                        |((symbol, id), market)| symbol == &market.symbol && id == &market.id,
                    ));
            if self.book_deltas {
                self.book_baselines.retain(|symbol, _| {
                    (if self.io.core.public_feed_policy == PublicFeedPolicy::Shared {
                        self.shared_books.contains_key(symbol)
                    } else {
                        self.venue.book(symbol).is_some()
                    }) && self.store.market_index(symbol).is_some()
                });
                if store && !markets_match {
                    self.book_market_rows = self
                        .store
                        .markets()
                        .iter()
                        .map(|market| (market.symbol.clone(), market.id.clone()))
                        .collect();
                    let symbols: Vec<_> = self
                        .books()
                        .filter_map(|(symbol, _)| {
                            let market = self.store.market_index(symbol)?;
                            let id = &self.store.markets()[market as usize].id;
                            self.book_baselines
                                .get(symbol)
                                .is_none_or(|baseline| {
                                    baseline.market != market || &baseline.id != id
                                })
                                .then(|| symbol.to_owned())
                        })
                        .collect();
                    for symbol in symbols {
                        self.request_book_resync(&symbol);
                    }
                }
            }
            // A market-only flush must not publish row indexes ahead of MARKETS.
            !self.book_deltas || store || markets_match
        };
        let buffer = std::mem::take(&mut self.io.core.frame);
        let mut w = FrameWriter::new(buffer, now);
        let pending = store.then(|| self.store.take_pending());
        if let Some(pending) = &pending {
            self.store.write(pending, &mut w);
        }
        let books = self.io.core.changed_books[..self.io.core.changed_books_active].iter();
        #[cfg(feature = "book-deltas")]
        let books = books.filter(|_| write_books);
        for symbol in books {
            let Some(market) = self.store.market_index(symbol) else {
                continue;
            };
            if let Some(book) = if self.io.core.public_feed_policy == PublicFeedPolicy::Shared {
                self.shared_books.get(symbol)
            } else {
                self.venue.book(symbol)
            } {
                #[cfg(feature = "book-deltas")]
                if self.book_deltas {
                    let id = &self.store.markets()[market as usize].id;
                    match self.book_baselines.get_mut(symbol) {
                        Some(baseline)
                            if baseline.market == market
                                && &baseline.id == id
                                && baseline.book.reset_generation() == book.reset_generation() =>
                        {
                            book.write_delta(&baseline.book, market, &mut w);
                            baseline.book.clone_from(book);
                        }
                        baseline => {
                            book.write(market, &mut w);
                            if let Some(baseline) = baseline {
                                baseline.market = market;
                                baseline.id.clone_from(id);
                                baseline.book.clone_from(book);
                            } else {
                                self.book_baselines.insert(
                                    symbol.clone(),
                                    BookBaseline {
                                        market,
                                        id: id.clone(),
                                        book: book.clone(),
                                    },
                                );
                            }
                        }
                    }
                    continue;
                }
                book.write(market, &mut w);
            } else {
                // A lost sequence makes the old consumer book unusable immediately.
                Book::default().write(market, &mut w);
                #[cfg(feature = "book-deltas")]
                self.book_baselines.remove(symbol);
            }
        }
        let candles: Vec<_> = self
            .io
            .core
            .candles
            .iter()
            .filter_map(|(symbol, interval, candle)| {
                Some((self.store.market_index(symbol)?, interval.as_str(), *candle))
            })
            .collect();
        w.candles(&candles);
        let mut fills = std::mem::take(&mut self.io.core.fills);
        if !fills.is_empty() {
            w.begin_revision(section::FILLS, 2);
            w.u32(fills.len() as u32);
            let orders: Vec<u32> = fills.iter().map(|(f, _)| w.string(&f.order_id)).collect();
            w.u32s(orders);
            w.u32s(fills.iter().map(|(f, _)| {
                self.store
                    .market_index(&f.symbol)
                    .unwrap_or(crate::wire::NONE)
            }));
            w.u32s(fills.iter().map(|(f, _)| f.side as u32));
            w.pad();
            for (f, _) in &fills {
                w.f64s([f.price, f.amount]);
            }
            for (fill, _) in &fills {
                let symbol = w.string(&fill.symbol);
                w.u32(symbol);
            }
            for (fill, _) in &fills {
                let id = w.optional_string(fill.id.as_deref());
                w.u32(id);
            }
            w.pad();
            w.f64s(fills.iter().map(|(_, financials)| {
                financials.and_then(|v| v.realized_pnl).unwrap_or(f64::NAN)
            }));
            w.f64s(
                fills
                    .iter()
                    .map(|(_, financials)| financials.and_then(|v| v.fee).unwrap_or(f64::NAN)),
            );
            w.end();
        }
        let empty = w.is_empty();
        let frame = w.finish();
        #[cfg(feature = "book-deltas")]
        if write_books {
            self.io.core.changed_books_active = 0;
        }
        #[cfg(not(feature = "book-deltas"))]
        {
            self.io.core.changed_books_active = 0;
        }
        if let Some(pending) = pending {
            self.store.recycle_pending(pending);
        }
        fills.clear();
        self.io.core.fills = fills;
        drop(candles);
        self.io.core.candles.clear();
        self.io.core.frame = frame;
        (!empty).then_some(self.io.core.frame.as_slice())
    }
}

/// The longest a repeating retried-read failure goes without an Error. Below the
/// app's 120 s auth-badge expiry, so a key that stays rejected keeps its badge.
pub const RETRIED_FAILURE_ERROR_EVERY_MS: f64 = 90_000.0;

/// The latch notice, `SignError::MissingKey`'s text after a fixed prefix.
const MISSING_KEY_NOTICE: &str =
    "Signing cannot succeed with the installed credentials: Signing key is not installed";

/// Credential-level signing refusal. Non-generic, so the combined build holds
/// one copy rather than one per venue engine.
#[derive(Default)]
struct SigningLatch {
    /// Socket roles whose signing was refused; they are never reconnected.
    roles: u8,
}

impl SigningLatch {
    #[inline(never)]
    fn blocks(&self, channel: Option<&SocketChannel>) -> bool {
        channel.is_some_and(|channel| self.roles & (1 << SocketRoles::role(channel)) != 0)
    }

    /// Whether signing is latched; if so, the failed socket's role is too.
    #[inline(never)]
    fn socket_failed(
        &mut self,
        signing: &crate::signing_dispatch::Dispatch,
        sockets: &SocketRoles,
        socket: SocketId,
    ) -> bool {
        if !signing.refused() {
            return false;
        }
        if let Some(role) = sockets.role_of(socket) {
            self.roles |= 1 << role;
        }
        true
    }

    /// A key missing from primary material can never sign: fail once,
    /// visibly, and during startup fail `start` (`Identities::primary_owns`).
    #[inline(never)]
    fn missing_key(
        signing: &mut crate::signing_dispatch::Dispatch,
        primary: bool,
        startup: bool,
        actions: &mut Vec<Action>,
        health: &mut Health,
        min_level: Level,
    ) {
        if !primary || !signing.refuse() {
            return;
        }
        let error = crate::signing::SignError::MissingKey;
        push_event(
            actions,
            health,
            min_level,
            LogEvent {
                level: Level::Error,
                code: log_code::CONFIG_INVALID,
                message: MISSING_KEY_NOTICE.into(),
                fields: Vec::new(),
            },
        );
        if startup {
            actions.push(Action::Notice(Notice::StartupFailed {
                error: error.venue_error(),
            }));
        }
    }
}

/// Live sockets that declared a role, for reconnect and private-traffic counts.
#[derive(Default)]
struct SocketRoles {
    live: Vec<(SocketId, u8)>,
    /// Private sockets alone, so the per-frame check scans one or two ids.
    private: Vec<SocketId>,
    /// Closes per role whose replacement connect has not happened yet.
    awaiting_reconnect: [u32; 3],
}

impl SocketRoles {
    fn role(channel: &SocketChannel) -> u8 {
        match channel {
            SocketChannel::Public => 0,
            SocketChannel::Private => 1,
            SocketChannel::Business => 2,
        }
    }

    fn role_of(&self, socket: SocketId) -> Option<u8> {
        self.live
            .iter()
            .find_map(|(id, role)| (*id == socket).then_some(*role))
    }

    fn connected(&mut self, socket: SocketId, channel: &SocketChannel, health: &mut Health) {
        let role = Self::role(channel);
        let awaiting = &mut self.awaiting_reconnect[role as usize];
        if *awaiting > 0 {
            *awaiting -= 1;
            health.reconnects = health.reconnects.saturating_add(1);
        }
        self.live.push((socket, role));
        if role == 1 {
            self.private.push(socket);
        }
    }

    /// Whether `socket` was a live role socket. Only an unexpected close makes
    /// the role's next connect a reconnect.
    fn closed(&mut self, socket: SocketId, unexpected: bool) -> bool {
        let Some(index) = self.live.iter().position(|(id, _)| *id == socket) else {
            return false;
        };
        let (_, role) = self.live.swap_remove(index);
        if unexpected {
            self.awaiting_reconnect[role as usize] =
                self.awaiting_reconnect[role as usize].saturating_add(1);
        }
        self.private.retain(|id| *id != socket);
        true
    }

    fn received(&self, socket: SocketId, now: f64, health: &mut Health) {
        if self.private.contains(&socket) {
            health.last_private_message_ms = Some(now);
        }
    }
}

/// At most one per engine event; `error: None` is a definitive deactivation.
fn count_auth_failure(io: &mut IoCore, error: Option<&VenueError>) {
    let auth = error.is_none_or(|error| {
        matches!(
            error.kind,
            Some(ErrorKind::AuthInvalid | ErrorKind::AuthExpired | ErrorKind::AuthPermission)
        )
    });
    if auth && !io.auth_failure_counted {
        io.auth_failure_counted = true;
        io.health.count(log_code::AUTH_REJECTED);
    }
}

#[inline(never)]
fn push_event(
    actions: &mut Vec<Action>,
    health: &mut Health,
    min_level: Level,
    mut event: LogEvent,
) {
    if event.code != log_code::AUTH_REJECTED {
        health.count(event.code);
    }
    // History coverage is part of the result contract, even when Warn logs are hidden.
    if event.level < min_level && event.code != log_code::HISTORY_INCOMPLETE {
        return;
    }
    event.message = crate::log::cap(event.message);
    for (_, value) in &mut event.fields {
        if let Value::Str(text) = value {
            *text = crate::log::cap(std::mem::take(text));
        }
    }
    actions.push(Action::Notice(Notice::Event(event)));
}

#[cfg(test)]
mod streaming_tests {
    use super::*;

    fn execution() -> Fill {
        Fill {
            id: None,
            order_id: "9007199254740995".into(),
            symbol: "A".into(),
            side: OrderSide::Buy,
            price: 99.,
            amount: 1.,
        }
    }

    #[test]
    fn revision_two_fills_preserve_absent_identity_and_financial_values() {
        for financials in [
            None,
            Some(FillFinancials::default()),
            Some(FillFinancials {
                realized_pnl: Some(f64::NAN),
                fee: Some(f64::INFINITY),
            }),
        ] {
            let mut engine = frame_engine();
            engine.store.set_markets(vec![market("A")]);
            engine.take_frame(0.);
            let mut cx = Cx {
                now: 1.,
                store: &mut engine.store,
                io: &mut engine.io,
            };
            if let Some(financials) = financials {
                cx.fill_with_financials(execution(), financials);
            } else {
                cx.fill(execution());
            }
            let frame = engine.take_frame(1.).unwrap();
            let fills = crate::wire::sections(frame)
                .find(|(kind, _)| *kind == section::FILLS)
                .unwrap()
                .1;
            let mut expected = Vec::new();
            for word in [1u32, 0, 0, OrderSide::Buy as u32] {
                expected.extend(word.to_le_bytes());
            }
            for value in [99f64, 1.] {
                expected.extend(value.to_le_bytes());
            }
            expected.extend(1u32.to_le_bytes()); // Stable raw symbol reference.
            expected.extend(crate::wire::NONE.to_le_bytes()); // Missing execution identity.
            expected.extend(f64::NAN.to_le_bytes());
            expected.extend(f64::NAN.to_le_bytes());
            assert_eq!(
                fills, expected,
                "revision two represents absent financials and identity explicitly"
            );
            assert!(engine.take_frame(1.).is_none(), "fill must flush once");
        }
    }

    #[test]
    fn fill_financial_columns_preserve_missing_zero_and_negative_values_once() {
        let mut engine = frame_engine();
        engine.store.set_markets(vec![market("A")]);
        engine.take_frame(0.);
        {
            let mut cx = Cx {
                now: 1.,
                store: &mut engine.store,
                io: &mut engine.io,
            };
            cx.fill(execution());
            cx.fill_with_financials(
                execution(),
                FillFinancials {
                    realized_pnl: Some(0.),
                    fee: Some(0.),
                },
            );
            cx.fill_with_financials(
                execution(),
                FillFinancials {
                    realized_pnl: Some(-3.5),
                    fee: Some(f64::NAN),
                },
            );
        }
        assert!(
            crate::wire::sections(engine.account_snapshot_frame(1.))
                .all(|(kind, _)| kind != section::FILLS)
        );
        let frame = engine.take_market_frame(1.).unwrap();
        let fills = crate::wire::sections(frame)
            .find(|(kind, _)| *kind == section::FILLS)
            .unwrap()
            .1;
        assert_eq!(u32::from_le_bytes(fills[..4].try_into().unwrap()), 3);
        assert_eq!(fills.len(), 160);
        let rows: Vec<_> = fills[40..88]
            .as_chunks::<8>()
            .0
            .iter()
            .map(|value| f64::from_le_bytes(*value))
            .collect();
        assert_eq!(rows, [99., 1., 99., 1., 99., 1.]);
        assert_eq!(&fills[88..100], &[1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0]);
        assert_eq!(&fills[100..112], &[255; 12]);
        let floats: Vec<_> = fills[112..]
            .as_chunks::<8>()
            .0
            .iter()
            .map(|value| f64::from_le_bytes(*value))
            .collect();
        assert!(floats[0].is_nan());
        assert_eq!(&floats[1..3], &[0., -3.5]);
        assert!(floats[3].is_nan());
        assert_eq!(floats[4], 0.);
        assert!(floats[5].is_nan());
        assert!(engine.take_market_frame(1.).is_none());
        assert!(engine.take_frame(1.).is_none());
    }

    #[test]
    fn command_errors_are_attributed_before_done_but_replyless_errors_stay_visible() {
        let mut engine = frame_engine();
        let before = engine.snapshot_frame(0.).to_vec();
        {
            let mut cx = Cx {
                now: 1.,
                store: &mut engine.store,
                io: &mut engine.io,
            };
            cx.command_error(0, "background error");
            cx.command_error(7, "command error");
            cx.fail(7, "command error");
        }
        let actions = engine.drain_actions().collect::<Vec<_>>();
        assert!(matches!(actions.as_slice(), [
            Action::Notice(Notice::Event(crate::log::LogEvent { level: Level::Error, message: background, code: log_code::COMMAND_FAILED, .. })),
            Action::Notice(Notice::CommandError { command: 7, message }),
            Action::Notice(Notice::CommandFailed { command: 7, error: result }),
        ] if background == "background error" && message == "command error" && result.legacy_text() == *message));
        assert_eq!(before, engine.snapshot_frame(0.));
    }

    #[test]
    fn command_progress_skips_empty_batches_and_precedes_io_without_store_changes() {
        let mut engine = frame_engine();
        let before = engine.snapshot_frame(0.).to_vec();
        let receipts = vec![OrderIdentityReceipt {
            symbol: "BTC-USD".into(),
            kind: crate::OrderIdentityKind::ClientOrderId,
            identity: "actual-native-id".into(),
        }];
        {
            let mut cx = Cx {
                now: 1.,
                store: &mut engine.store,
                io: &mut engine.io,
            };
            cx.command_progress(7, Vec::new());
            cx.command_progress(7, receipts.clone());
            cx.request(
                Method::Post,
                "https://example.test/orders".into(),
                vec![],
                None,
                Vec::new(),
            );
        }
        let actions = engine.drain_actions().collect::<Vec<_>>();
        assert!(matches!(actions.as_slice(), [
            Action::Notice(Notice::CommandProgress { command: 7, receipts: actual }),
            Action::Http(_),
        ] if actual == &receipts));
        assert_eq!(before, engine.snapshot_frame(0.));
    }

    struct StreamVenue {
        at: ArrayAt,
        rows: Vec<String>,
        body: Option<String>,
        failure: Option<String>,
        book: Option<Book>,
        empty_book: Book,
        book_subscribers: u32,
        symbol_amend: bool,
        fence_uncertain: bool,
        /// Placements and amends hold lock scopes (both guard flags).
        scoped: bool,
        commands: Vec<Command>,
    }

    impl Venue for StreamVenue {
        fn guard_pending_placements(&self) -> bool {
            self.scoped
        }
        fn guard_order_mutations(&self) -> bool {
            self.scoped
        }
        fn fence_uncertain_cancel_all(&self) -> bool {
            self.fence_uncertain
        }
        fn fence_uncertain_placements(&self) -> bool {
            true
        }
        type Pending = Vec<String>;
        fn start(&mut self, cx: &mut Cx<'_, Self::Pending>) {
            cx.request_streamed(
                Method::Get,
                "https://example.test/list".into(),
                Vec::new(),
                self.at,
                Vec::new(),
            );
        }
        fn on_open(&mut self, _cx: &mut Cx<'_, Self::Pending>, _socket: SocketId) {}
        fn on_message(&mut self, cx: &mut Cx<'_, Self::Pending>, _socket: SocketId, text: &str) {
            #[cfg(feature = "order-locks")]
            if let Some(completion) = text.strip_prefix("complete:") {
                let (id, status) = completion.split_once(':').unwrap();
                let id = id.parse().unwrap();
                if status == "ok" {
                    cx.done(id, "[]".into());
                } else if status == "unknown" {
                    cx.fail(
                        id,
                        VenueError::new("Lost write reply").outcome(crate::Outcome::Unknown),
                    );
                } else {
                    cx.fail(id, "Lost write reply");
                }
                return;
            }
            #[cfg(feature = "order-locks")]
            if let Some(symbol) = text.strip_prefix("read-orders:") {
                cx.request(
                    Method::Get,
                    "https://example.test/orders".into(),
                    Vec::new(),
                    None,
                    vec![format!("reconcile:{symbol}")],
                );
                return;
            }
            #[cfg(feature = "order-locks")]
            if text == "write-order" {
                cx.request(
                    Method::Post,
                    "https://example.test/orders".into(),
                    Vec::new(),
                    Some("{}".into()),
                    Vec::new(),
                );
                return;
            }
            for symbol in text.split(',') {
                cx.book_changed(symbol);
            }
        }
        fn on_close(&mut self, _cx: &mut Cx<'_, Self::Pending>, _socket: SocketId, _code: u16) {}
        fn on_response(
            &mut self,
            _cx: &mut Cx<'_, Self::Pending>,
            pending: Self::Pending,
            response: Response<'_>,
        ) {
            #[cfg(feature = "order-locks")]
            if let Some(symbol) = pending.first().and_then(|s| s.strip_prefix("reconcile:")) {
                _cx.orders_reconciled(Some(symbol));
                return;
            }
            self.rows = pending;
            self.body = Some(response.body.to_owned());
        }
        fn on_item(
            &mut self,
            _cx: &mut Cx<'_, Self::Pending>,
            pending: &mut Self::Pending,
            item: &str,
        ) {
            pending.push(item.to_owned());
        }
        fn on_request_failed(
            &mut self,
            _cx: &mut Cx<'_, Self::Pending>,
            _pending: Self::Pending,
            error: &str,
        ) {
            self.failure = Some(error.to_owned());
        }
        fn on_timer(&mut self, _cx: &mut Cx<'_, Self::Pending>, _timer: TimerId) {}
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                amend_with_symbol: self.symbol_amend,
                ..Capabilities::BASE
            }
        }
        fn on_command(&mut self, _cx: &mut Cx<'_, Self::Pending>, command: Command) {
            match &command {
                Command::SubscribeOrderBook { .. } => self.book_subscribers += 1,
                Command::UnsubscribeOrderBook { .. } => {
                    self.book_subscribers = self.book_subscribers.saturating_sub(1)
                }
                _ => {}
            }
            self.commands.push(command);
        }
        fn book(&self, _symbol: &str) -> Option<&Book> {
            self.book.as_ref()
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            Box::new(
                self.book
                    .as_ref()
                    .or((self.book_subscribers > 0).then_some(&self.empty_book))
                    .into_iter()
                    .map(|book| ("A", book)),
            )
        }
    }

    #[cfg(feature = "order-locks")]
    #[test]
    fn order_mutation_guard_holds_until_completion_and_normalizes_virtual_ids() {
        let mut engine = frame_engine();
        // This test venue holds commands until its scripted completion event.
        for (command, id) in [(1, "native__stop_loss"), (2, "native"), (3, "other")] {
            engine.handle(
                0.,
                Event::Command(Command::AmendOrder {
                    command,
                    id: id.into(),
                    price: Some(5.),
                    amount: None,
                }),
            );
        }
        assert_eq!(
            engine.venue.commands.len(),
            2,
            "overlapping native mutation must not dispatch"
        );
        assert!(
            engine
                .drain_actions()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 2, .. })))
        );
        // Duplicate IDs cannot release the first operation's lock.
        engine.handle(
            0.,
            Event::Command(Command::CancelOrders {
                command: 1,
                ids: vec!["native".into()],
            }),
        );
        engine.handle(
            0.,
            Event::Command(Command::CancelOrders {
                command: 4,
                ids: vec!["native".into()],
            }),
        );
        assert_eq!(engine.venue.commands.len(), 2);
        engine.io.core.order_locks.finish(1, false);
        engine.handle(
            0.,
            Event::Command(Command::CancelOrders {
                command: 5,
                ids: vec!["native".into()],
            }),
        );
        assert_eq!(engine.venue.commands.len(), 3);
        let mut cx = Cx {
            now: 0.,
            store: &mut engine.store,
            io: &mut engine.io,
        };
        cx.fail(5, "lost response");
        engine.handle(
            0.,
            Event::Command(Command::CancelOrders {
                command: 6,
                ids: vec!["native".into()],
            }),
        );
        assert_eq!(
            engine.venue.commands.len(),
            4,
            "failed completion must also release"
        );
    }

    #[cfg(feature = "order-locks")]
    #[test]
    fn cancel_symbol_guards_every_order_in_its_scope_but_allows_other_markets() {
        let mut engine = frame_engine();
        engine.handle(
            0.,
            Event::Command(Command::CancelSymbolOrders {
                command: 1,
                symbol: "A".into(),
            }),
        );
        for (command, symbol) in [(2, "A"), (3, "B")] {
            engine.handle(
                0.,
                Event::Command(Command::CancelOrdersWithSymbols {
                    command,
                    orders: vec![crate::model::CancelOrderRef {
                        id: "x".into(),
                        symbol: symbol.into(),
                    }],
                }),
            );
        }
        assert_eq!(engine.venue.commands.len(), 2);
        engine.handle(
            0.,
            Event::Command(Command::CancelSymbolOrders {
                command: 4,
                symbol: String::new(),
            }),
        );
        assert_eq!(
            engine.venue.commands.len(),
            2,
            "global cancel-all conflicts with either symbol"
        );
    }

    #[cfg(feature = "order-locks")]
    fn held_place(command: u32, symbol: &str) -> Command {
        let order = serde_json::from_str(&format!(
            r#"{{"symbol":"{symbol}","type":"limit","side":"buy","price":100,"amount":1}}"#
        ))
        .unwrap();
        Command::PlaceOrders {
            command,
            orders: vec![order],
        }
    }

    #[cfg(feature = "order-locks")]
    fn complete_held(engine: &mut Engine<StreamVenue>, command: u32, failed: bool) {
        engine.handle(
            1.,
            Event::WsMessage {
                socket: SocketId(1),
                text: &format!("complete:{command}:{}", if failed { "error" } else { "ok" }),
            },
        );
    }

    #[cfg(feature = "order-locks")]
    #[test]
    fn an_unknown_write_fences_its_symbols_on_a_venue_without_lock_scopes() {
        let mut engine = frame_engine();
        engine.venue.scoped = false;
        engine.venue.symbol_amend = true;
        let refused = |engine: &mut Engine<StreamVenue>, id: u32| {
            engine.drain_actions().any(|a| {
                matches!(a, Action::Notice(Notice::CommandFailed { command, error })
                    if command == id
                        && error.outcome == Some(crate::Outcome::NotSent)
                        && error.legacy_text().contains("uncertain"))
            })
        };
        let unknown = |engine: &mut Engine<StreamVenue>, id: u32| {
            engine.handle(
                1.,
                Event::WsMessage {
                    socket: SocketId(1),
                    text: &format!("complete:{id}:unknown"),
                },
            );
        };
        // A placement refused later in admission (options) leaves no mark.
        let mut options = held_place(9, "A");
        if let Command::PlaceOrders { orders, .. } = &mut options {
            orders[0].implied_volatility = Some(0.5);
        }
        engine.handle(0., Event::Command(options));
        engine.drain_actions().for_each(drop);
        assert_eq!(engine.io.core.order_locks.marked(), 0);
        assert!(!engine.io.core.order_locks.uncertain("A"));
        let stale = engine.io.core.order_locks.token();
        engine.handle(0., Event::Command(held_place(1, "A")));
        unknown(&mut engine, 1);
        assert!(engine.io.core.order_locks.uncertain("A"));
        assert!(
            !engine.io.core.order_locks.uncertain("B"),
            "only the placement's own symbols"
        );
        assert!(
            engine.reconcile_timer.is_some(),
            "the Unknown write queues its reconcile at once"
        );
        engine.drain_actions().for_each(drop);
        let admitted = engine.venue.commands.len();
        // A retry on the symbol is refused NotSent and never reaches the venue.
        engine.handle(0., Event::Command(held_place(2, "A")));
        assert!(refused(&mut engine, 2));
        assert_eq!(engine.venue.commands.len(), admitted);
        // Cancel-all keeps the venue's declared disposition: refused by default,
        // admitted when the venue opts out; cancel by ID is always admitted.
        let cancel_all = |command| Command::CancelSymbolOrders {
            command,
            symbol: "A".into(),
        };
        engine.handle(0., Event::Command(cancel_all(10)));
        assert!(
            engine.drain_actions().any(|a| matches!(a,
                Action::Notice(Notice::CommandFailed { command: 10, error })
                    if error.outcome == Some(crate::Outcome::NotSent)
                        && error.legacy_text().contains("cancel-all"))),
            "a fenced symbol's cancel-all is refused before any I/O"
        );
        assert_eq!(engine.venue.commands.len(), admitted);
        engine.handle(
            0.,
            Event::Command(Command::CancelOrders {
                command: 11,
                ids: vec!["resting".into()],
            }),
        );
        assert!(!refused(&mut engine, 11));
        engine.venue.fence_uncertain = false;
        engine.handle(0., Event::Command(cancel_all(12)));
        assert!(!refused(&mut engine, 12));
        assert_eq!(
            engine.venue.commands.len(),
            admitted + 2,
            "both cancels reach the venue"
        );
        engine.venue.fence_uncertain = true;
        let admitted = engine.venue.commands.len();
        // Another symbol is admitted; a definitive failure marks nothing.
        engine.handle(0., Event::Command(held_place(3, "B")));
        complete_held(&mut engine, 3, true);
        assert!(!engine.io.core.order_locks.uncertain("B"));
        // An amend whose outcome is Unknown fences its symbol too.
        engine.handle(
            0.,
            Event::Command(Command::AmendOrderWithSymbol {
                command: 4,
                id: "resting".into(),
                symbol: "C".into(),
                price: Some(101.),
                amount: None,
            }),
        );
        unknown(&mut engine, 4);
        assert!(engine.io.core.order_locks.uncertain("C"));
        // A stale read cannot release; a fresh read can, and the retry is admitted.
        engine.io.core.order_locks.reconciled(None, stale);
        engine.drain_actions().for_each(drop);
        engine.handle(0., Event::Command(held_place(5, "A")));
        assert!(refused(&mut engine, 5), "a stale read does not release");
        let fresh = engine.io.core.order_locks.token();
        engine.io.core.order_locks.reconciled(None, fresh);
        engine.handle(0., Event::Command(held_place(6, "A")));
        assert!(!refused(&mut engine, 6));
        assert_eq!(engine.venue.commands.len(), admitted + 3);
    }

    #[cfg(feature = "order-locks")]
    #[test]
    fn cancel_all_waits_for_overlapping_places_and_duplicate_ids_do_not_unlock() {
        let mut engine = frame_engine();
        engine.handle(0., Event::Command(held_place(1, "A")));
        engine.handle(
            0.,
            Event::Command(Command::CancelSymbolOrders {
                command: 2,
                symbol: "A".into(),
            }),
        );
        assert_eq!(
            engine.venue.commands.len(),
            1,
            "cancel-all cannot miss in-flight placement"
        );
        engine.handle(
            0.,
            Event::Command(Command::CancelOrders {
                command: 1,
                ids: vec!["unrelated".into()],
            }),
        );
        engine.handle(
            0.,
            Event::Command(Command::CancelSymbolOrders {
                command: 2,
                symbol: "B".into(),
            }),
        );
        engine.handle(0., Event::Command(held_place(3, "B")));
        assert_eq!(
            engine.venue.commands.len(),
            2,
            "unrelated market may place while cancellation waits"
        );
        complete_held(&mut engine, 1, false);
        assert_eq!(
            engine.venue.commands.len(),
            3,
            "waiting cancel-all dispatches after ACK"
        );
        engine.handle(0., Event::Command(held_place(4, "A")));
        assert_eq!(
            engine.venue.commands.len(),
            3,
            "replacement placement cannot escape active cancel-all scope"
        );
        complete_held(&mut engine, 2, false);
        engine.handle(0., Event::Command(held_place(5, "A")));
        assert_eq!(engine.venue.commands.len(), 4);
    }

    #[cfg(feature = "order-locks")]
    #[test]
    fn failed_sent_place_blocks_queued_and_later_cancel_all_until_fresh_reconciliation() {
        let mut engine = frame_engine();
        engine.handle(0., Event::Command(held_place(1, "A")));
        engine.io.core.order_locks.note_write();
        let stale_read = engine.io.core.order_locks.token();
        engine.handle(
            0.,
            Event::Command(Command::CancelSymbolOrders {
                command: 2,
                symbol: "A".into(),
            }),
        );
        // A snapshot while the write is still in flight cannot prove absence.
        engine.io.core.order_locks.reconciled(Some("A"), stale_read);
        complete_held(&mut engine, 1, true);
        engine.handle(
            0.,
            Event::Command(Command::CancelSymbolOrders {
                command: 3,
                symbol: "A".into(),
            }),
        );
        assert_eq!(
            engine.venue.commands.len(),
            1,
            "neither cancellation may report false success"
        );
        let actions: Vec<_> = engine.drain_actions().collect();
        for command in [2, 3] {
            assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: id, error }) if *id == command && error.legacy_text() == "Placement outcome is uncertain; reconcile before cancel-all" && error.outcome == Some(crate::Outcome::NotSent))));
        }
        engine
            .io
            .core
            .order_locks
            .reconciled(Some("B"), engine.io.core.order_locks.token());
        assert!(engine.io.core.order_locks.uncertain("A"));
        engine.io.core.order_locks.reconciled(Some("A"), stale_read);
        assert!(
            engine.io.core.order_locks.uncertain("A"),
            "pre-write snapshot cannot clear uncertainty"
        );
        engine
            .io
            .core
            .order_locks
            .reconciled(Some("A"), engine.io.core.order_locks.token());
        engine.handle(
            0.,
            Event::Command(Command::CancelSymbolOrders {
                command: 4,
                symbol: "A".into(),
            }),
        );
        assert_eq!(engine.venue.commands.len(), 2);
        // A preflight failure without a dispatched write does not poison its market.
        engine.handle(0., Event::Command(held_place(5, "B")));
        complete_held(&mut engine, 5, true);
        assert!(!engine.io.core.order_locks.uncertain("B"));
    }

    #[cfg(feature = "order-locks")]
    #[test]
    fn opted_out_venue_receives_queued_and_later_cancel_all_after_uncertain_place() {
        let mut engine = frame_engine();
        engine.venue.fence_uncertain = false;
        engine.handle(0., Event::Command(held_place(1, "A")));
        engine.io.core.order_locks.note_write();
        engine.handle(
            0.,
            Event::Command(Command::CancelSymbolOrders {
                command: 2,
                symbol: "A".into(),
            }),
        );
        assert_eq!(
            engine.venue.commands.len(),
            1,
            "cancel-all still waits for the placement"
        );
        complete_held(&mut engine, 1, true);
        assert!(engine.io.core.order_locks.uncertain("A"));
        assert_eq!(
            engine.venue.commands.len(),
            2,
            "queued cancel-all is dispatched"
        );
        complete_held(&mut engine, 2, false);
        engine.handle(
            0.,
            Event::Command(Command::CancelSymbolOrders {
                command: 3,
                symbol: "A".into(),
            }),
        );
        assert_eq!(
            engine.venue.commands.len(),
            3,
            "later cancel-all is dispatched"
        );
        let actions: Vec<_> = engine.drain_actions().collect();
        assert!(!actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { error, .. }) if error.legacy_text().contains("uncertain"))));
    }

    #[cfg(feature = "order-locks")]
    #[test]
    fn complete_known_partial_outcome_does_not_poison_or_clear_older_uncertainty() {
        let mut engine = frame_engine();
        engine.handle(0., Event::Command(held_place(1, "A")));
        engine.io.core.order_locks.note_write();
        complete_held(&mut engine, 1, true);
        assert!(engine.io.core.order_locks.uncertain("A"));
        // A later placement on the ambiguous symbol is refused at entry, so it
        // can neither duplicate nor clear the earlier one.
        engine.handle(0., Event::Command(held_place(2, "A")));
        assert_eq!(engine.venue.commands.len(), 1);
        engine.handle(0., Event::Command(held_place(3, "B")));
        engine.io.core.order_locks.note_write();
        // Every batch row has either an exact native identity or a definite rejection.
        engine.io.core.order_locks.resolved(3);
        complete_held(&mut engine, 3, true);
        assert!(
            engine.io.core.order_locks.uncertain("A"),
            "known later error cannot clear earlier ambiguous placement"
        );
        assert!(
            !engine.io.core.order_locks.uncertain("B"),
            "definite rejection or complete partial ACK is safe"
        );
        engine.handle(
            0.,
            Event::Command(Command::CancelSymbolOrders {
                command: 4,
                symbol: "B".into(),
            }),
        );
        assert_eq!(engine.venue.commands.len(), 3);
    }

    #[cfg(feature = "order-locks")]
    #[test]
    fn an_uncertain_placement_fences_later_placements_and_amends_until_reconciled() {
        let mut engine = frame_engine();
        // Cancels stay admitted on an uncertain symbol unless a venue opts in.
        engine.venue.fence_uncertain = false;
        let message = |engine: &mut Engine<StreamVenue>, text| {
            engine.handle(
                0.,
                Event::WsMessage {
                    socket: SocketId(1),
                    text,
                },
            );
        };
        let request = |engine: &mut Engine<StreamVenue>| {
            engine
                .drain_actions()
                .find_map(|a| match a {
                    Action::Http(request) => Some(request.id),
                    _ => None,
                })
                .unwrap()
        };
        let refused = |engine: &mut Engine<StreamVenue>, id: u32| {
            engine.drain_actions().any(|a| {
                matches!(a, Action::Notice(Notice::CommandFailed { command, error })
                    if command == id
                        && error.outcome == Some(crate::Outcome::NotSent)
                        && error.legacy_text().contains("uncertain"))
            })
        };
        engine.handle(0., Event::Command(held_place(1, "A")));
        message(&mut engine, "write-order");
        request(&mut engine);
        complete_held(&mut engine, 1, true);
        assert!(engine.io.core.order_locks.uncertain("A"));
        assert!(
            !engine.io.core.reconcile_due.is_empty() || engine.drain_actions().count() > 0,
            "the Unknown placement queues its reconcile at once"
        );
        engine.drain_actions().for_each(drop);
        let admitted = engine.venue.commands.len();

        // A retry on the uncertain symbol is refused NotSent, never sent.
        engine.handle(0., Event::Command(held_place(2, "A")));
        assert!(refused(&mut engine, 2), "a placement retry is fenced");
        // An unroutable amend may target the uncertain symbol.
        let amend = Command::AmendOrder {
            command: 3,
            id: "resting".into(),
            price: Some(101.),
            amount: None,
        };
        engine.handle(0., Event::Command(amend));
        assert!(refused(&mut engine, 3), "an amend is fenced");
        assert_eq!(engine.venue.commands.len(), admitted);

        // Another symbol, and cancels on this one, are unaffected.
        engine.handle(0., Event::Command(held_place(4, "B")));
        complete_held(&mut engine, 4, false);
        engine.handle(
            0.,
            Event::Command(Command::CancelOrders {
                command: 5,
                ids: vec!["resting".into()],
            }),
        );
        complete_held(&mut engine, 5, false);
        engine.handle(
            0.,
            Event::Command(Command::CancelSymbolOrders {
                command: 6,
                symbol: "A".into(),
            }),
        );
        complete_held(&mut engine, 6, false);
        assert_eq!(engine.venue.commands.len(), admitted + 3);
        engine.drain_actions().for_each(drop);

        // The first successful fresh order read clears the fence.
        message(&mut engine, "read-orders:A");
        let fresh = request(&mut engine);
        engine.handle(
            1.,
            Event::HttpResponse {
                request: fresh,
                response: Response {
                    status: 200,
                    headers: "",
                    body: "[]",
                },
            },
        );
        assert!(!engine.io.core.order_locks.uncertain("A"));
        engine.handle(0., Event::Command(held_place(7, "A")));
        assert_eq!(engine.venue.commands.len(), admitted + 4);
    }

    #[cfg(feature = "order-locks")]
    #[test]
    fn authoritative_order_read_carries_its_dispatch_epoch_through_the_response() {
        let mut engine = frame_engine();
        let message = |engine: &mut Engine<StreamVenue>, text| {
            engine.handle(
                0.,
                Event::WsMessage {
                    socket: SocketId(1),
                    text,
                },
            );
        };
        let request = |engine: &mut Engine<StreamVenue>| {
            engine
                .drain_actions()
                .find_map(|a| match a {
                    Action::Http(request) => Some(request.id),
                    _ => None,
                })
                .unwrap()
        };
        engine.handle(0., Event::Command(held_place(1, "A")));
        message(&mut engine, "write-order");
        request(&mut engine);
        message(&mut engine, "read-orders:A");
        let stale = request(&mut engine);
        complete_held(&mut engine, 1, true);
        let respond = |engine: &mut Engine<StreamVenue>, request| {
            engine.handle(
                1.,
                Event::HttpResponse {
                    request,
                    response: Response {
                        status: 200,
                        headers: "",
                        body: "[]",
                    },
                },
            );
        };
        respond(&mut engine, stale);
        assert!(engine.io.core.order_locks.uncertain("A"));
        engine.handle(
            0.,
            Event::Command(Command::CancelSymbolOrders {
                command: 2,
                symbol: "A".into(),
            }),
        );
        assert_eq!(engine.venue.commands.len(), 1);
        message(&mut engine, "read-orders:A");
        let fresh = request(&mut engine);
        respond(&mut engine, fresh);
        assert!(!engine.io.core.order_locks.uncertain("A"));
        engine.handle(
            0.,
            Event::Command(Command::CancelSymbolOrders {
                command: 3,
                symbol: "A".into(),
            }),
        );
        assert_eq!(engine.venue.commands.len(), 2);
    }

    #[cfg(feature = "order-locks")]
    #[test]
    fn symbol_cancel_conflicts_with_uncached_alias_and_unknown_native_ids() {
        let mut engine = frame_engine();
        engine.store.set_markets(vec![market("A"), market("B")]);
        let order = crate::model::Order {
            id: "native".into(),
            parent_id: None,
            status: crate::model::OrderStatus::Open,
            symbol: "A".into(),
            kind: crate::model::OrderType::Limit,
            side: crate::model::OrderSide::Buy,
            price: 100.,
            amount: 1.,
            filled: 0.,
            remaining: 1.,
            reduce_only: false,
            close: None,
            client_order_id: None,
            time_in_force: None,
            last_update: 0.,
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: None,
        };
        engine.store.upsert_order(order);
        engine.handle(
            0.,
            Event::Command(Command::AmendOrder {
                command: 1,
                id: "native__stop_loss".into(),
                price: Some(101.),
                amount: None,
            }),
        );
        engine.handle(
            0.,
            Event::Command(Command::CancelSymbolOrders {
                command: 2,
                symbol: "A".into(),
            }),
        );
        assert_eq!(
            engine.venue.commands.len(),
            1,
            "virtual alias must inherit underlying order's market"
        );
        engine.handle(
            0.,
            Event::Command(Command::CancelSymbolOrders {
                command: 3,
                symbol: "B".into(),
            }),
        );
        assert_eq!(
            engine.venue.commands.len(),
            2,
            "known alias does not block unrelated market"
        );
        complete_held(&mut engine, 1, false);
        complete_held(&mut engine, 3, false);
        engine.handle(
            0.,
            Event::Command(Command::CancelOrders {
                command: 4,
                ids: vec!["uncached".into()],
            }),
        );
        engine.handle(
            0.,
            Event::Command(Command::CancelSymbolOrders {
                command: 5,
                symbol: "B".into(),
            }),
        );
        assert_eq!(
            engine.venue.commands.len(),
            3,
            "unresolved native identity overlaps every symbol-wide mutation"
        );
    }

    #[cfg(feature = "command-tracking")]
    #[test]
    fn command_boundary_allows_startup_but_records_commands_before_dispatch() {
        let mut engine = frame_engine();
        assert!(!engine.has_received_commands());
        engine.start(0.);
        assert!(
            !engine.has_received_commands(),
            "startup is outside the command ID domain"
        );
        engine.handle(
            1.,
            Event::WsOpened {
                socket: SocketId(999),
            },
        );
        assert!(!engine.has_received_commands());
        engine.handle(
            1.,
            Event::PollScaleChanged {
                scale: f64::INFINITY,
            },
        );
        assert!(
            !engine.has_received_commands(),
            "host configuration is outside the command ID domain"
        );
        engine.handle(
            2.,
            Event::Command(Command::SubscribeOrderBook { symbol: "A".into() }),
        );
        assert!(
            engine.has_received_commands(),
            "subscription commands also close the boundary"
        );
        engine.start(3.);
        assert!(
            engine.has_received_commands(),
            "start never resets prior command history"
        );
    }

    fn stream(at: ArrayAt, body: &[u8]) -> StreamVenue {
        let mut engine = crate::test_support::seeded_engine(StreamVenue {
            at,
            rows: Vec::new(),
            body: None,
            failure: None,
            book: None,
            empty_book: Book::default(),
            book_subscribers: 0,
            symbol_amend: false,
            fence_uncertain: true,
            scoped: true,
            commands: Vec::new(),
        });
        engine.start(0.0);
        let request = engine
            .drain_actions()
            .find_map(|action| match action {
                Action::Http(request) => Some(request.id),
                _ => None,
            })
            .unwrap();
        engine.handle(
            1.0,
            Event::HttpStart {
                request,
                status: 200,
                headers: "",
            },
        );
        for data in body.chunks(1) {
            engine.handle(1.0, Event::HttpChunk { request, data });
        }
        engine.handle(1.0, Event::HttpEnd { request });
        engine.venue
    }

    fn frame_engine() -> Engine<StreamVenue> {
        crate::test_support::seeded_engine(StreamVenue {
            at: ArrayAt::Root,
            rows: Vec::new(),
            body: None,
            failure: None,
            book: Some(Book::default()),
            empty_book: Book::default(),
            book_subscribers: 0,
            symbol_amend: false,
            fence_uncertain: true,
            scoped: true,
            commands: Vec::new(),
        })
    }

    #[test]
    fn account_snapshot_has_complete_empty_private_sections_without_public_tables() {
        let mut engine = frame_engine();
        engine.store.set_markets(vec![market("A")]);
        engine.take_frame(0.);
        engine.store.set_balance(crate::model::Balance {
            used: 1.,
            free: 2.,
            total: 3.,
            upnl: 4.,
        });
        let snapshot = engine.account_snapshot_frame(1.).to_vec();
        let sections = crate::wire::sections(&snapshot).collect::<Vec<_>>();
        assert_eq!(
            sections.iter().map(|(kind, _)| *kind).collect::<Vec<_>>(),
            [
                section::ORDERS,
                section::POSITIONS,
                section::BALANCE,
                section::SETTINGS,
                section::STATUS,
                section::ACCOUNT_READ
            ]
        );
        for kind in [section::ORDERS, section::POSITIONS, section::SETTINGS] {
            let payload = sections.iter().find(|(id, _)| *id == kind).unwrap().1;
            assert_eq!(u32::from_le_bytes(payload[..4].try_into().unwrap()), 0);
        }
        let balance = sections
            .iter()
            .find(|(id, _)| *id == section::BALANCE)
            .unwrap()
            .1;
        assert_eq!(
            balance[..32]
                .as_chunks::<8>()
                .0
                .iter()
                .map(|row| f64::from_le_bytes(*row))
                .collect::<Vec<_>>(),
            [1., 2., 3., 4.]
        );
        assert_eq!(engine.account_snapshot_frame(1.), snapshot);
        assert!(
            engine.take_frame(1.).is_some(),
            "snapshot must leave dirty private state for the stream"
        );
        assert!(engine.take_frame(1.).is_none());
        assert_eq!(
            engine.account_snapshot_frame(1.),
            snapshot,
            "clean state remains readable"
        );
    }

    #[test]
    fn account_snapshot_carries_unflushed_catalog_reordering_before_private_indexes() {
        let mut engine = frame_engine();
        engine.store.set_markets(vec![market("A"), market("B")]);
        engine.take_frame(0.);
        engine.store.set_positions(vec![crate::model::Position {
            id: "A:long".into(),
            symbol: "A".into(),
            side: crate::model::PositionSide::Long,
            entry_price: 10.,
            notional: 20.,
            leverage: 2.,
            margin_type: None,
            position_mode: None,
            unrealized_pnl: 0.,
            realized_pnl: None,
            contracts: 2.,
            liquidation_price: 0.,
            margin_size: None,
            last_update: None,
        }]);
        engine.store.set_markets(vec![market("B"), market("A")]);
        let snapshot = engine.account_snapshot_frame(1.).to_vec();
        let sections = crate::wire::sections(&snapshot).collect::<Vec<_>>();
        assert_eq!(sections[0].0, section::MARKETS);
        let positions = sections
            .iter()
            .find(|(kind, _)| *kind == section::POSITIONS)
            .unwrap()
            .1;
        assert_eq!(u32::from_le_bytes(positions[4..8].try_into().unwrap()), 1);
        assert!(sections.iter().all(|(kind, _)| !matches!(
            *kind,
            section::TICKERS
                | section::TICKER_DELTAS
                | section::BOOK
                | section::CANDLES
                | section::FILLS
        )));
        assert_eq!(engine.account_snapshot_frame(1.), snapshot);
        let normal = engine.take_frame(1.).unwrap().to_vec();
        for kind in [section::MARKETS, section::POSITIONS] {
            assert_eq!(
                crate::wire::sections(&normal)
                    .find(|(id, _)| *id == kind)
                    .unwrap()
                    .1,
                sections.iter().find(|(id, _)| *id == kind).unwrap().1,
                "account read must leave the catalog and private changes pending"
            );
        }
        assert!(engine.take_frame(1.).is_none());
        assert!(
            crate::wire::sections(engine.account_snapshot_frame(1.))
                .all(|(kind, _)| kind != section::MARKETS)
        );
    }

    #[test]
    fn account_snapshot_leaves_public_events_and_ticker_deltas_for_their_normal_flushes() {
        let mut engine = frame_engine();
        engine.store.set_markets(vec![market("A")]);
        engine.store.set_tickers(vec![crate::model::Ticker {
            symbol: "A".into(),
            ..Default::default()
        }]);
        engine.take_frame(0.);
        engine.store.update_ticker("A", |ticker| ticker.last = 99.);
        engine.store.set_balance(crate::model::Balance {
            free: 10.,
            ..Default::default()
        });
        {
            let mut cx = Cx {
                now: 1.,
                store: &mut engine.store,
                io: &mut engine.io,
            };
            cx.book_changed("A");
            cx.candle(
                "A",
                Interval::M1,
                crate::model::Candle {
                    timestamp: 1.,
                    open: 98.,
                    high: 100.,
                    low: 97.,
                    close: 99.,
                    volume: 2.,
                },
            );
            cx.fill(Fill {
                id: None,
                order_id: "9007199254740995".into(),
                symbol: "A".into(),
                side: OrderSide::Buy,
                price: 99.,
                amount: 1.,
            });
        }
        let snapshot = engine.account_snapshot_frame(1.).to_vec();
        assert!(crate::wire::sections(&snapshot).all(|(kind, _)| !matches!(
            kind,
            section::MARKETS
                | section::TICKERS
                | section::TICKER_DELTAS
                | section::BOOK
                | section::BOOK_DELTA
                | section::CANDLES
                | section::FILLS
        )));
        let market = engine.take_market_frame(1.).unwrap().to_vec();
        let kinds = crate::wire::sections(&market)
            .map(|(kind, _)| kind)
            .collect::<Vec<_>>();
        // A fill travels with the store sections, so its flush carries them too.
        for kind in [
            section::BOOK,
            section::CANDLES,
            section::FILLS,
            section::TICKER_DELTAS,
            section::BALANCE,
        ] {
            assert!(kinds.contains(&kind));
        }
        assert!(
            engine.take_market_frame(1.).is_none(),
            "events must flush exactly once"
        );
        assert!(engine.take_frame(1.).is_none());
    }

    #[cfg(feature = "book-deltas")]
    #[test]
    fn account_snapshots_do_not_advance_or_reset_the_book_delta_baseline() {
        use crate::book::BookSide::Bids;
        let mut engine = frame_engine();
        engine.set_book_deltas(true);
        engine.store.set_markets(vec![market("A")]);
        change_book(&mut engine, Bids, 100., 1.);
        engine.take_frame(0.);
        change_book(&mut engine, Bids, 100., 2.);
        engine.account_snapshot_frame(1.);
        let rows = book_sections(engine.take_market_frame(1.).unwrap());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, section::BOOK_DELTA);
        change_book(&mut engine, Bids, 100., 3.);
        engine.account_snapshot_frame(2.);
        change_book(&mut engine, Bids, 100., 2.);
        assert!(
            engine.take_market_frame(2.).is_none(),
            "baseline must remain the last flushed size"
        );
    }

    #[test]
    fn account_identity_on_a_venue_without_it_fails_at_once() {
        let mut engine = frame_engine();
        engine.handle(0.0, Event::Command(Command::FetchAccount { command: 8 }));
        let result = engine.drain_actions().find_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: 8, error }) => Some(error.message),
            _ => None,
        });
        assert_eq!(
            result.as_deref(),
            Some("This action is not supported on this venue")
        );
    }

    #[test]
    fn venue_actions_default_to_one_shared_rejection() {
        let mut engine = frame_engine();
        engine.handle(
            0.0,
            Event::Command(Command::Venue {
                command: 9,
                action: VenueAction::FetchAccountTier,
            }),
        );
        let error = engine.drain_actions().find_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: 9, error }) => Some(error.message),
            _ => None,
        });
        assert_eq!(
            error.as_deref(),
            Some("This action is not supported on this venue")
        );
    }

    #[test]
    fn market_only_frames_wait_for_pending_market_identity_without_dropping_events() {
        use crate::model::Candle;
        let mut engine = frame_engine();
        engine.store.set_markets(vec![market("A"), market("B")]);
        engine.take_frame(0.);
        engine.store.set_markets(vec![market("B"), market("A")]);
        engine.handle(
            1.,
            Event::WsMessage {
                socket: SocketId(1),
                text: "A",
            },
        );
        engine.io.core.candles.push((
            "A".into(),
            Interval::M1,
            Candle {
                timestamp: 1.,
                open: 2.,
                high: 3.,
                low: 1.,
                close: 2.5,
                volume: 4.,
            },
        ));
        assert!(engine.take_market_frame(1.).is_none());
        assert_eq!(engine.io.core.changed_books_active, 1);
        assert_eq!(engine.io.core.candles.len(), 1);
        // A pending fill would promote the flush to a full frame (MARKETS first).
        engine.io.core.fills.push((
            Fill {
                id: None,
                order_id: "filled-order".into(),
                symbol: "ORPHAN".into(),
                side: OrderSide::Buy,
                price: 7.,
                amount: 0.25,
            },
            None,
        ));
        let frame = engine.take_frame(2.).unwrap().to_vec();
        let sections: Vec<_> = crate::wire::sections(&frame).collect();
        assert!(sections.iter().any(|(kind, _)| *kind == section::MARKETS));
        assert_eq!(book_markets(&frame), [1]);
        let candles = sections
            .iter()
            .find(|(kind, _)| *kind == section::CANDLES)
            .unwrap()
            .1;
        assert_eq!(u32::from_le_bytes(candles[4..8].try_into().unwrap()), 1);
        let fills = sections
            .iter()
            .find(|(kind, _)| *kind == section::FILLS)
            .unwrap()
            .1;
        assert_eq!(
            u32::from_le_bytes(fills[8..12].try_into().unwrap()),
            crate::wire::NONE
        );
        assert_eq!(
            fills.len(),
            56,
            "one revision-two fill includes identity and financial columns"
        );
        let reference = u32::from_le_bytes(fills[32..36].try_into().unwrap()) as usize;
        let strings = sections
            .iter()
            .find(|(kind, _)| *kind == section::STRINGS)
            .unwrap()
            .1;
        let count = u32::from_le_bytes(strings[..4].try_into().unwrap()) as usize;
        let at = 4 + reference * 4;
        let start = u32::from_le_bytes(strings[at..at + 4].try_into().unwrap()) as usize;
        let end = u32::from_le_bytes(strings[at + 4..at + 8].try_into().unwrap()) as usize;
        let data = 4 + (count + 1) * 4;
        assert_eq!(&strings[data + start..data + end], b"ORPHAN");
        assert!(engine.io.core.fills.is_empty());
        assert!(engine.io.core.candles.is_empty());
        assert_eq!(engine.io.core.changed_books_active, 0);
        assert!(engine.take_market_frame(3.).is_none());
    }

    #[test]
    fn symbol_amend_defaults_to_one_unsupported_reply_without_dispatch_or_state_changes() {
        let mut engine = frame_engine();
        engine.store.set_markets(vec![market("BTC-USDT")]);
        let before = engine.snapshot_frame(1.).to_vec();
        engine.handle(
            1.,
            Event::Command(Command::AmendOrderWithSymbol {
                command: 31,
                id: "unseen".into(),
                symbol: "BTC-USDT".into(),
                price: Some(100.),
                amount: Some(2.),
            }),
        );
        assert!(engine.venue.commands.is_empty());
        let actions = engine.drain_actions().collect::<Vec<_>>();
        assert!(
            matches!(actions.as_slice(), [Action::Notice(Notice::CommandFailed { command: 31, error })] if error.legacy_text().contains("unsupported"))
        );
        assert_eq!(engine.snapshot_frame(1.), before);
        let legacy = Command::SetTickerSymbols {
            symbols: vec!["BTC-USDT".into()],
        };
        engine.handle(1., Event::Command(legacy.clone()));
        assert_eq!(engine.venue.commands, [legacy]);
        assert!(!engine.has_actions());
    }

    #[test]
    fn symbol_amend_opt_in_forwards_the_complete_original_command() {
        let mut engine = frame_engine();
        engine.venue.symbol_amend = true;
        let command = Command::AmendOrderWithSymbol {
            command: 32,
            id: "unseen".into(),
            symbol: "BTC-USDT".into(),
            price: Some(100.),
            amount: Some(2.),
        };
        engine.handle(1., Event::Command(command.clone()));
        assert_eq!(engine.venue.commands, [command]);
        assert!(!engine.has_actions());
    }

    #[test]
    fn unsupported_options_complete_without_reaching_the_venue() {
        let mut engine = frame_engine();
        let query = serde_json::from_str(r#"{"underlying":"BTC","right":"call"}"#).unwrap();
        let commands = [
            Command::FetchOptionChain { command: 1, query },
            Command::FetchOptionExpiries { command: 2, underlying: "BTC".into() },
            Command::FetchAccountGreeks { command: 3 },
            Command::PlaceOrders { command: 4, orders: vec![
                serde_json::from_str(r#"{"symbol":"BTCUSDT","type":"market","side":"buy","amount":1}"#).unwrap(),
                serde_json::from_str(r#"{"symbol":"BTC-option","type":"limit","side":"buy","amount":1,"impliedVolatility":0.5}"#).unwrap(),
            ] },
        ];
        for command in commands {
            engine.handle(0.0, Event::Command(command));
        }
        let actions: Vec<_> = engine.drain_actions().collect();
        assert_eq!(actions.len(), 4, "no order or query I/O may escape");
        assert!(engine.venue.commands.is_empty());
        for (index, action) in actions.iter().enumerate() {
            assert!(
                matches!(action, Action::Notice(Notice::CommandFailed { command, error })
                if *command == index as u32 + 1 && error.message.contains("Options are not enabled"))
            );
        }
    }

    pub(super) fn market(symbol: &str) -> crate::model::Market {
        use crate::model::{Limits, Market, MinMax, Precision};
        Market {
            id: symbol.into(),
            symbol: symbol.into(),
            pretty_symbol: None,
            base: symbol.into(),
            quote: "USD".into(),
            active: true,
            precision: Precision {
                amount: 1.,
                price: 1.,
            },
            limits: Limits {
                amount: MinMax { min: 1., max: 100. },
                leverage: MinMax { min: 1., max: 10. },
                price: None,
            },
            contract_size: None,
            is_expiring: None,
            is_inverse: None,
            is_spot: None,
            is_perpetual: None,
            is_read_only: None,
            taker_fee: None,
            maker_fee: None,
        }
    }

    fn book_markets(frame: &[u8]) -> Vec<u32> {
        crate::wire::sections(frame)
            .filter(|(kind, _)| *kind == section::BOOK)
            .map(|(_, row)| u32::from_le_bytes(row[..4].try_into().unwrap()))
            .collect()
    }

    #[cfg(feature = "book-deltas")]
    fn change_book(
        engine: &mut Engine<StreamVenue>,
        side: crate::book::BookSide,
        price: f64,
        amount: f64,
    ) {
        engine
            .venue
            .book
            .as_mut()
            .unwrap()
            .apply(side, price, amount);
        engine.handle(
            1.,
            Event::WsMessage {
                socket: SocketId(1),
                text: "A",
            },
        );
    }

    #[cfg(feature = "book-deltas")]
    fn book_sections(frame: &[u8]) -> Vec<(u16, Vec<u8>)> {
        crate::wire::sections(frame)
            .filter(|(kind, _)| matches!(*kind, section::BOOK | section::BOOK_DELTA))
            .map(|(kind, payload)| (kind, payload.to_vec()))
            .collect()
    }

    #[cfg(feature = "book-deltas")]
    #[test]
    fn deltas_coalesce_net_changes_and_snapshots_do_not_consume_the_baseline() {
        use crate::book::BookSide::{Asks, Bids};
        let mut engine = frame_engine();
        engine.set_book_deltas(true);
        engine.store.set_markets(vec![market("A")]);
        change_book(&mut engine, Bids, 100., 1.);
        change_book(&mut engine, Asks, 101., 2.);
        assert_eq!(
            book_sections(engine.take_frame(1.).unwrap())[0].0,
            section::BOOK
        );
        change_book(&mut engine, Bids, 100., 3.);
        change_book(&mut engine, Bids, 99., 4.);
        change_book(&mut engine, Asks, 101., 0.);
        assert_eq!(book_sections(engine.snapshot_frame(2.))[0].0, section::BOOK);
        let sections = book_sections(engine.take_market_frame(2.).unwrap());
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].0, section::BOOK_DELTA);
        let payload = &sections[0].1;
        assert_eq!(
            &payload[..16],
            &[0, 0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]
        );
        let rows: Vec<_> = payload[16..]
            .as_chunks::<8>()
            .0
            .iter()
            .map(|row| f64::from_le_bytes(*row))
            .collect();
        assert_eq!(rows, [100., 3., 99., 4., 101., 0.]);
        change_book(&mut engine, Bids, 100., 7.);
        engine.snapshot_frame(3.);
        change_book(&mut engine, Bids, 100., 3.);
        // Mid-interval snapshots cannot be used as a new stream boundary.
        // The global baseline remains the previously flushed value, 3.
        assert!(engine.take_market_frame(3.).is_none());
        assert!(engine.take_frame(3.).is_none());
    }

    #[cfg(feature = "book-deltas")]
    #[test]
    fn deltas_resync_on_book_reset_subscription_and_market_identity_changes() {
        use crate::book::BookSide::Bids;
        let mut engine = frame_engine();
        engine.set_book_deltas(true);
        engine.store.set_markets(vec![market("A")]);
        change_book(&mut engine, Bids, 100., 1.);
        engine.take_frame(0.).unwrap();
        for reset in 0..5 {
            match reset {
                0 => engine.venue.book.as_mut().unwrap().clear(),
                1 => engine
                    .venue
                    .book
                    .as_mut()
                    .unwrap()
                    .load_sorted(Bids, [(100., 1.)]),
                2 => engine
                    .venue
                    .book
                    .as_mut()
                    .unwrap()
                    .load_sorted_owned(Bids, vec![(100., 1.)]),
                3 => engine.venue.book = Some(Book::default()),
                _ => engine.handle(
                    1.,
                    Event::Command(Command::SubscribeOrderBook { symbol: "A".into() }),
                ),
            }
            change_book(&mut engine, Bids, 100., 1.);
            assert_eq!(
                book_sections(engine.take_frame(1.).unwrap())[0].0,
                section::BOOK
            );
        }
        let mut renamed = market("A");
        renamed.id = "new-native-id".into();
        engine.store.set_markets(vec![market("B"), renamed]);
        change_book(&mut engine, Bids, 100., 2.);
        assert!(
            engine.take_market_frame(2.).is_none(),
            "new market indexes wait for MARKETS"
        );
        assert_eq!(book_markets(engine.take_frame(2.).unwrap()), [1]);
        engine.store.set_markets(vec![market("A")]);
        assert_eq!(
            book_markets(engine.take_frame(3.).unwrap()),
            [0],
            "identity changes resync even without a dirty notification"
        );
    }

    #[cfg(feature = "book-deltas")]
    #[test]
    fn delta_baselines_are_removed_with_books_and_markets_and_modes_can_switch() {
        use crate::book::BookSide::Bids;
        let mut engine = frame_engine();
        engine.set_book_deltas(true);
        engine.store.set_markets(vec![market("A")]);
        change_book(&mut engine, Bids, 100., 1.);
        engine.take_frame(0.).unwrap();
        assert_eq!(engine.book_baselines.len(), 1);
        engine.handle(
            1.,
            Event::Command(Command::UnsubscribeOrderBook { symbol: "A".into() }),
        );
        assert!(engine.book_baselines.is_empty());
        engine.venue.book = None;
        assert!(engine.take_frame(1.).is_none());
        engine.venue.book = Some(Book::default());
        change_book(&mut engine, Bids, 100., 1.);
        assert_eq!(
            book_sections(engine.take_frame(2.).unwrap())[0].0,
            section::BOOK
        );
        engine.store.set_markets(vec![market("B")]);
        engine.take_frame(3.).unwrap();
        assert!(engine.book_baselines.is_empty());
        engine.store.set_markets(vec![market("A")]);
        assert_eq!(
            book_sections(engine.take_frame(4.).unwrap())[0].0,
            section::BOOK
        );
        engine.set_book_deltas(false);
        change_book(&mut engine, Bids, 100., 2.);
        assert_eq!(
            book_sections(engine.take_frame(5.).unwrap())[0].0,
            section::BOOK
        );
        assert!(engine.book_baselines.is_empty());
        engine.set_book_deltas(true);
        assert_eq!(
            book_sections(engine.take_frame(6.).unwrap())[0].0,
            section::BOOK
        );
        change_book(&mut engine, Bids, 100., 3.);
        assert_eq!(
            book_sections(engine.take_frame(7.).unwrap())[0].0,
            section::BOOK_DELTA
        );
    }

    #[test]
    fn invalidated_book_emits_clear_and_recovered_one_sided_book_is_preserved() {
        for deltas in [false, true] {
            let mut engine = frame_engine();
            #[cfg(feature = "book-deltas")]
            engine.set_book_deltas(deltas);
            #[cfg(not(feature = "book-deltas"))]
            let _ = deltas;
            engine.store.set_markets(vec![market("A")]);
            engine
                .venue
                .book
                .as_mut()
                .unwrap()
                .apply(crate::book::BookSide::Bids, 100., 2.);
            engine.handle(
                1.,
                Event::WsMessage {
                    socket: SocketId(1),
                    text: "A",
                },
            );
            engine.take_frame(1.).unwrap();
            engine.venue.book = None;
            engine.handle(
                2.,
                Event::WsMessage {
                    socket: SocketId(1),
                    text: "A",
                },
            );
            let frame = engine
                .take_market_frame(2.)
                .expect("consumer must clear stale levels");
            let (_, payload) = crate::wire::sections(frame)
                .find(|(kind, _)| *kind == section::BOOK)
                .unwrap();
            assert_eq!(payload, [0; 16]);
            engine.venue.book = Some(Book::default());
            engine
                .venue
                .book
                .as_mut()
                .unwrap()
                .apply(crate::book::BookSide::Asks, 101., 3.);
            engine.handle(
                3.,
                Event::WsMessage {
                    socket: SocketId(1),
                    text: "A",
                },
            );
            let frame = engine.take_market_frame(3.).unwrap();
            let (_, payload) = crate::wire::sections(frame)
                .find(|(kind, _)| *kind == section::BOOK)
                .unwrap();
            assert_eq!(u32::from_le_bytes(payload[4..8].try_into().unwrap()), 0);
            assert_eq!(u32::from_le_bytes(payload[8..12].try_into().unwrap()), 1);
            assert_eq!(
                f64::from_le_bytes(payload[16..24].try_into().unwrap()),
                101.
            );
        }
    }

    #[test]
    fn snapshot_clears_subscribed_resync_book_even_when_the_clear_frame_was_missed() {
        for deltas in [false, true] {
            let mut engine = frame_engine();
            #[cfg(feature = "book-deltas")]
            engine.set_book_deltas(deltas);
            #[cfg(not(feature = "book-deltas"))]
            let _ = deltas;
            engine.store.set_markets(vec![market("A")]);
            for _ in 0..2 {
                engine.handle(
                    0.,
                    Event::Command(Command::SubscribeOrderBook { symbol: "A".into() }),
                );
            }
            engine
                .venue
                .book
                .as_mut()
                .unwrap()
                .apply(crate::book::BookSide::Bids, 100., 2.);
            engine.handle(
                1.,
                Event::WsMessage {
                    socket: SocketId(1),
                    text: "A",
                },
            );
            engine.take_frame(1.).unwrap();
            engine.venue.book = None;
            engine
                .venue
                .empty_book
                .apply(crate::book::BookSide::Bids, 99., 7.);
            engine.handle(
                2.,
                Event::WsMessage {
                    socket: SocketId(1),
                    text: "A",
                },
            );
            // The worker flushed this frame, but a suspended consumer lost it.
            engine.take_frame(2.).unwrap();
            let frame = engine.snapshot_frame(3.);
            let (_, payload) = crate::wire::sections(frame)
                .find(|(kind, _)| *kind == section::BOOK)
                .expect("resync snapshot must overwrite stale consumer levels");
            assert_eq!(payload, [0; 16]);
            engine.venue.book = Some(Book::default());
            engine
                .venue
                .book
                .as_mut()
                .unwrap()
                .apply(crate::book::BookSide::Asks, 101., 3.);
            let (_, payload) = crate::wire::sections(engine.snapshot_frame(4.))
                .find(|(kind, _)| *kind == section::BOOK)
                .unwrap();
            assert_eq!(u32::from_le_bytes(payload[8..12].try_into().unwrap()), 1);
            engine.venue.book = None;
            engine.handle(
                5.,
                Event::Command(Command::UnsubscribeOrderBook { symbol: "A".into() }),
            );
            assert!(
                crate::wire::sections(engine.snapshot_frame(5.))
                    .any(|(kind, _)| kind == section::BOOK),
                "another subscriber is still recovering"
            );
            engine.handle(
                6.,
                Event::Command(Command::UnsubscribeOrderBook { symbol: "A".into() }),
            );
            assert!(
                !crate::wire::sections(engine.snapshot_frame(6.))
                    .any(|(kind, _)| kind == section::BOOK),
                "last unsubscribe removes the recovery snapshot entry"
            );
        }
    }

    #[test]
    fn book_flushes_preserve_first_change_order_and_resolve_current_market_identity() {
        let mut engine = frame_engine();
        engine.store.set_markets(vec![market("A"), market("B")]);
        engine.take_frame(0.).unwrap();
        engine.handle(
            1.,
            Event::WsMessage {
                socket: SocketId(1),
                text: "B,A,B",
            },
        );
        engine.snapshot_frame(1.);
        assert_eq!(book_markets(engine.take_market_frame(1.).unwrap()), [1, 0]);
        assert!(engine.take_market_frame(1.).is_none());

        engine.handle(
            2.,
            Event::WsMessage {
                socket: SocketId(1),
                text: "A,A,B",
            },
        );
        engine.store.set_markets(vec![market("B"), market("A")]);
        assert_eq!(book_markets(engine.take_frame(2.).unwrap()), [1, 0]);
        assert!(engine.take_frame(2.).is_none());

        engine.handle(
            3.,
            Event::WsMessage {
                socket: SocketId(1),
                text: "missing",
            },
        );
        assert!(engine.take_market_frame(3.).is_none());
        engine
            .store
            .set_markets(vec![market("missing"), market("A")]);
        assert!(book_markets(engine.take_frame(3.).unwrap()).is_empty());
        engine.handle(
            4.,
            Event::WsMessage {
                socket: SocketId(1),
                text: "missing,A,missing",
            },
        );
        assert_eq!(book_markets(engine.take_market_frame(4.).unwrap()), [0, 1]);
    }

    #[test]
    fn ticker_flushes_do_not_replay_deltas_or_consume_them_during_snapshots() {
        use crate::model::Ticker;
        let mut engine = frame_engine();
        let tickers = || {
            ["A", "B"]
                .into_iter()
                .map(|symbol| Ticker {
                    symbol: symbol.into(),
                    ..Ticker::default()
                })
                .collect()
        };
        engine.store.set_tickers(tickers());
        engine.take_frame(0.).unwrap();
        engine.store.update_ticker("B", |t| t.last = 1.);
        assert!(engine.take_market_frame(1.).is_none());
        engine.snapshot_frame(1.);
        let deltas = |frame: &[u8]| -> Vec<f64> {
            crate::wire::sections(frame)
                .filter(|(kind, _)| *kind == section::TICKER_DELTAS)
                .flat_map(|(_, row)| row.as_chunks::<8>().0)
                .map(|value| f64::from_le_bytes(*value))
                .collect()
        };
        assert_eq!(deltas(engine.take_frame(1.).unwrap()), [1., 4., 1.]);
        assert!(engine.take_frame(1.).is_none());
        engine.store.update_ticker("A", |t| t.bid = 2.);
        engine.store.update_ticker("A", |t| t.bid = 3.);
        assert_eq!(deltas(engine.take_frame(2.).unwrap()), [0., 1., 3.]);

        engine.store.update_ticker("A", |t| t.last = 9.);
        engine.store.set_tickers(vec![Ticker {
            symbol: "B".into(),
            ..Ticker::default()
        }]);
        engine.store.update_ticker("B", |t| t.volume = 3.);
        assert!(deltas(engine.take_frame(3.).unwrap()).is_empty());
        engine.store.update_ticker("B", |t| t.ask = 4.);
        assert_eq!(deltas(engine.take_frame(4.).unwrap()), [0., 2., 4.]);
        assert!(engine.take_frame(4.).is_none());
    }

    #[test]
    fn streamed_completion_preserves_bybit_cursor_and_bingx_alias_envelopes() {
        for (at, body) in [
            (ArrayAt::Root, "[1]"),
            (ArrayAt::Field("symbols"), r#"{"symbols":[1]}"#),
            (
                ArrayAt::Path(&["result", "list"]),
                r#"{"list":[9],"retCode":0,"result":{"list":[1],"nextPageCursor":"next"}}"#,
            ),
            (ArrayAt::EnvelopeField("data"), r#"{"data":[1],"code":0}"#),
            (
                ArrayAt::EnvelopeFieldOrNested("data", "orders"),
                r#"{"data":{"orders":[1]},"code":0}"#,
            ),
            (
                ArrayAt::EnvelopeFieldOrNestedAny("data", &["fill_orders", "fill_history_orders"]),
                r#"{"data":{"fill_orders":[1]},"code":0}"#,
            ),
            (
                ArrayAt::EnvelopeFieldOrNestedAny("data", &["fill_orders", "fill_history_orders"]),
                r#"{"data":{"fill_history_orders":[1]},"code":0}"#,
            ),
        ] {
            let venue = stream(at, body.as_bytes());
            assert_eq!(venue.failure, None, "{at:?}");
            assert_eq!(venue.rows, ["1"]);
            let expected = body.replace("[1]", "[]");
            assert_eq!(venue.body.as_deref(), Some(expected.as_str()));
        }
    }

    #[test]
    fn invalid_utf8_truncated_lists_and_oversized_metadata_discard_staged_rows() {
        let oversized = format!(r#"{{"data":[1]}}{}garbage"#, " ".repeat(1024 * 1024));
        for body in [
            b"{\"data\":[1],\"msg\":\"\xff\"}".as_slice(),
            b"{\"data\":[1,\"\xff\"]}".as_slice(),
            br#"{"data":[1,"#,
            oversized.as_bytes(),
        ] {
            let venue = stream(ArrayAt::EnvelopeField("data"), body);
            assert!(venue.failure.is_some());
            assert!(venue.rows.is_empty());
            assert!(venue.body.is_none());
        }
    }
}

#[cfg(test)]
mod request_route_tests {
    use super::*;
    #[test]
    fn route_hints_mark_only_the_requested_outbound_action() {
        let mut store = Store::default();
        let mut io = Io::<()> {
            core: IoCore {
                entropy: Some(crate::test_support::seeded_entropy(17)),
                ..IoCore::default()
            },
            ..Io::default()
        };
        let mut cx = Cx {
            now: 0.0,
            store: &mut store,
            io: &mut io,
        };
        let normal = cx.request(
            Method::Post,
            "https://venue.test/order".into(),
            vec![],
            None,
            (),
        );
        let direct = cx.request(
            Method::Get,
            "https://venue.test/time".into(),
            vec![],
            None,
            (),
        );
        let cors = cx.request(
            Method::Get,
            "https://venue.test/time".into(),
            vec![],
            None,
            (),
        );
        cx.route_request(direct, RequestRoute::Direct);
        cx.route_request(cors, RequestRoute::PublicCors);
        cx.request_timeout(direct, 5_000);
        cx.request_timeout(cors, 0);
        cx.route_request(RequestId(999), RequestRoute::Direct);
        let requests = io
            .core
            .actions
            .iter()
            .filter_map(|a| {
                if let Action::Http(r) = a {
                    Some(r)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            requests.iter().map(|r| (r.id, r.route)).collect::<Vec<_>>(),
            vec![
                (normal, None),
                (direct, Some(RequestRoute::Direct)),
                (cors, Some(RequestRoute::PublicCors))
            ]
        );
        assert!(requests.iter().all(|r| r.headers.is_empty()));
        assert_eq!(
            requests.iter().map(|r| r.timeout_ms).collect::<Vec<_>>(),
            vec![None, Some(5_000), Some(1)]
        );
    }
}

#[cfg(all(test, feature = "order-locks"))]
mod signing_tests {
    use super::*;
    #[derive(Default)]
    struct SigningVenue {
        signing_failures: Vec<(u32, String)>,
        transport_failures: Vec<(u32, String)>,
        responses: Vec<(u32, Option<u64>)>,
    }
    impl Venue for SigningVenue {
        fn guard_order_mutations(&self) -> bool {
            true
        }
        type Pending = u32;
        fn start(&mut self, _cx: &mut Cx<'_, u32>) {}
        fn on_open(&mut self, _cx: &mut Cx<'_, u32>, _socket: SocketId) {}
        fn on_message(&mut self, _cx: &mut Cx<'_, u32>, _socket: SocketId, _text: &str) {}
        fn on_close(&mut self, _cx: &mut Cx<'_, u32>, _socket: SocketId, _code: u16) {}
        fn on_response(&mut self, cx: &mut Cx<'_, u32>, pending: u32, _response: Response<'_>) {
            self.responses
                .push((pending, cx.io.core.order_response_token));
        }
        fn on_sign_failed(&mut self, _cx: &mut Cx<'_, u32>, pending: u32, error: &str) {
            self.signing_failures.push((pending, error.to_owned()));
        }
        fn on_request_failed(&mut self, _cx: &mut Cx<'_, u32>, pending: u32, error: &str) {
            self.transport_failures.push((pending, error.to_owned()));
        }
        fn on_timer(&mut self, _cx: &mut Cx<'_, u32>, _timer: TimerId) {}
        fn on_command(&mut self, _cx: &mut Cx<'_, u32>, _command: Command) {}
        fn book(&self, _symbol: &str) -> Option<&Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }

    #[test]
    fn duplicate_unsupported_symbol_amend_cannot_complete_or_release_an_existing_mutation() {
        let mut engine = crate::test_support::seeded_engine(SigningVenue::default());
        engine.handle(
            0.,
            Event::Command(Command::AmendOrder {
                command: 1,
                id: "native".into(),
                price: Some(100.),
                amount: None,
            }),
        );
        assert!(engine.io.core.order_locks.contains(1));
        engine.handle(
            1.,
            Event::Command(Command::AmendOrderWithSymbol {
                command: 1,
                id: "native".into(),
                symbol: "BTCUSDT".into(),
                price: Some(101.),
                amount: None,
            }),
        );
        assert!(engine.io.core.active_commands.contains(&1));
        assert!(engine.io.core.order_locks.contains(1));
        let actions: Vec<_> = engine.drain_actions().collect();
        assert!(matches!(
            actions.as_slice(),
            [Action::Notice(Notice::Event(crate::log::LogEvent {
                level: Level::Error,
                ..
            }))]
        ));
        // A newly added read must not complete an older write with the same ID.
        engine.handle(
            1.5,
            Event::Command(Command::FetchMaxLeverage {
                command: 1,
                symbol: "BTCUSDT".into(),
            }),
        );
        assert!(matches!(
            engine.drain_actions().collect::<Vec<_>>().as_slice(),
            [Action::Notice(Notice::Event(crate::log::LogEvent {
                level: Level::Error,
                ..
            }))]
        ));
        assert!(engine.io.core.active_commands.contains(&1));
        assert!(engine.io.core.order_locks.contains(1));
        engine.handle(
            2.,
            Event::Command(Command::CancelOrders {
                command: 2,
                ids: vec!["native".into()],
            }),
        );
        assert!(
            engine
                .drain_actions()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 2, .. })))
        );
        assert!(engine.io.core.order_locks.contains(1));
        engine.handle(
            3.,
            Event::Command(Command::AmendOrderWithSymbol {
                command: 3,
                id: "other".into(),
                symbol: "ETHUSDT".into(),
                price: Some(101.),
                amount: None,
            }),
        );
        assert!(!engine.io.core.active_commands.contains(&3));
        assert!(engine.io.core.order_locks.contains(1));
    }
}

#[cfg(test)]
mod security_debug {
    use super::*;

    #[test]
    fn transport_and_signing_debug_never_render_authentication_material() {
        let sentinel = "credential-sentinel-never-log";
        let http = HttpRequest {
            id: RequestId(17),
            method: Method::Post,
            route: None,
            timeout_ms: Some(5_000),
            url: format!("https://venue.test/private?signature={sentinel}"),
            headers: vec![("Authorization", sentinel.into())],
            signer_headers: vec![("X-Proof".into(), sentinel.into())],
            body: Some(sentinel.into()),
        };
        let debug = format!("{http:?}");
        assert!(!debug.contains(sentinel));
        assert!(!debug.contains("https://venue.test"));
        assert!(debug.contains("RequestId(17)"));
    }
}

#[cfg(test)]
mod jitter_tests {
    use super::*;

    fn draws(clocks: &[f64], delay: f64) -> Vec<f64> {
        let mut io = Io::<()> {
            core: IoCore {
                entropy: Some(crate::test_support::seeded_entropy(17)),
                ..IoCore::default()
            },
            ..Io::default()
        };
        let mut store = Store::default();
        clocks
            .iter()
            .map(|&now| {
                Cx {
                    now,
                    store: &mut store,
                    io: &mut io,
                }
                .jittered(delay)
            })
            .collect()
    }

    #[test]
    fn retry_jitter_stays_in_the_last_quarter_and_replays() {
        let clocks: Vec<f64> = (0..64).map(|i| f64::from(i) * 250.0).collect();
        let first = draws(&clocks, 4_000.0);
        assert!(first.iter().all(|d| (3_000.0..=4_000.0).contains(d)));
        assert_eq!(first, draws(&clocks, 4_000.0));
        // Same clock, successive calls: the state advances, so draws differ.
        let same_clock = draws(&[0.0; 8], 4_000.0);
        assert!(same_clock.windows(2).all(|pair| pair[0] != pair[1]));
        // Host entropy, rather than the clock, now controls reproducibility.
        assert_eq!(first[..8], draws(&[1.0; 8], 4_000.0)[..]);
    }

    #[test]
    fn failed_entropy_uses_the_nominal_delay() {
        let mut io = Io::<()> {
            core: IoCore {
                entropy: Some(crate::entropy::Entropy::new(
                    Box::new(|bytes| {
                        bytes.fill(99);
                        Err("unavailable".into())
                    }),
                    Box::new(|_| panic!("jitter must not consume signing entropy")),
                )),
                ..IoCore::default()
            },
            ..Io::default()
        };
        let mut store = Store::default();
        let mut cx = Cx {
            now: 0.,
            store: &mut store,
            io: &mut io,
        };
        assert_eq!(cx.jittered(4_000.), 4_000.);
    }

    #[test]
    fn retry_jitter_passes_through_unschedulable_delays() {
        let draws = draws(&[0.0; 4], f64::INFINITY);
        assert!(draws.iter().all(|d| *d == f64::INFINITY));
        let mut io = Io::<()> {
            core: IoCore {
                entropy: Some(crate::test_support::seeded_entropy(17)),
                ..IoCore::default()
            },
            ..Io::default()
        };
        let mut store = Store::default();
        let mut cx = Cx {
            now: 0.0,
            store: &mut store,
            io: &mut io,
        };
        assert_eq!(cx.jittered(0.0), 0.0);
        assert_eq!(cx.jittered(-5.0), -5.0);
        assert!(cx.jittered(f64::NAN).is_nan());
        let mut actual = [0; 8];
        cx.fill_entropy(&mut actual).unwrap();
        let mut expected = [0; 8];
        crate::test_support::seeded_entropy(17)
            .identity(&mut expected)
            .unwrap();
        assert_eq!(actual, expected, "pass-through draws nothing");
    }
}

#[cfg(test)]
mod startup_tests {
    use super::*;
    use crate::startup::{Backoff, number_or_zero};

    fn assert_jittered(actual: f64, nominal: f64) {
        assert!(
            (0.75 * nominal..=nominal).contains(&actual),
            "{actual} outside jittered [0.75, 1.0] x {nominal}"
        );
    }

    #[test]
    fn discovery_retry_jitters_but_never_undercuts_a_rate_limit_minimum() {
        let mut io = Io::<()> {
            core: IoCore {
                entropy: Some(crate::test_support::seeded_entropy(17)),
                ..IoCore::default()
            },
            ..Io::default()
        };
        let mut store = Store::default();
        let mut cx = Cx {
            now: 0.0,
            store: &mut store,
            io: &mut io,
        };
        let mut retry = Backoff::default();
        let keys = ["a", "b", "c", "d", "e", "f", "g", "h"];
        for key in keys {
            retry.failed(&mut cx, key);
        }
        // A 1 s Retry-After equals the nominal first delay, so jitter cannot shorten it.
        retry.failed_after(&mut cx, "limited", 1_000.0);
        let delays: Vec<_> = cx
            .io
            .core
            .actions
            .drain(..)
            .filter_map(|a| match a {
                Action::SetTimer { at_ms, .. } => Some(at_ms),
                _ => None,
            })
            .collect();
        let (jittered, limited) = delays.split_at(keys.len());
        for delay in jittered {
            assert_jittered(*delay, 1_000.0);
        }
        assert!(jittered.iter().any(|delay| *delay < 1_000.0));
        assert!(limited.iter().all(|delay| *delay == 1_000.0));
    }

    #[test]
    fn discovery_retry_waits_out_a_rate_limit_window() {
        use crate::startup::rate_limit_pause_ms;
        let mut io = Io::<()> {
            core: IoCore {
                entropy: Some(crate::test_support::seeded_entropy(17)),
                ..IoCore::default()
            },
            ..Io::default()
        };
        let mut store = Store::default();
        let mut cx = Cx {
            now: 0.0,
            store: &mut store,
            io: &mut io,
        };
        let mut retry = Backoff::default();
        retry.failed_after(&mut cx, "markets", 120_000.0);
        retry.failed_after(&mut cx, "tickers", 10.0);
        let delays: Vec<_> = cx
            .io
            .core
            .actions
            .drain(..)
            .filter_map(|a| match a {
                Action::SetTimer { at_ms, .. } => Some(at_ms),
                _ => None,
            })
            .collect();
        // A read failing during the ban waits it out too, whatever its own minimum.
        assert_eq!(delays, [120_000.0, 120_000.0]);
        // A later, shorter ban neither shortens the window nor reschedules reads already due.
        retry.failed_after(&mut cx, "premium", 90_000.0);
        let actions = std::mem::take(&mut cx.io.core.actions);
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::ClearTimer { .. }))
        );
        let delays: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { at_ms, .. } => Some(*at_ms),
                _ => None,
            })
            .collect();
        assert_eq!(delays, [120_000.0]);
        // A longer ban still pushes back every waiting read.
        retry.failed_after(&mut cx, "funding", 150_000.0);
        let actions = std::mem::take(&mut cx.io.core.actions);
        let delays: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { at_ms, .. } => Some(*at_ms),
                _ => None,
            })
            .collect();
        assert_eq!(delays, [150_000.0; 4]);
        let response = |status, headers| Response {
            status,
            headers,
            body: "",
        };
        assert_eq!(
            rate_limit_pause_ms(&response(429, "Retry-After: 30"), 5_000.0),
            30_000.0
        );
        assert_eq!(rate_limit_pause_ms(&response(418, ""), 5_000.0), 5_000.0);
        assert_eq!(
            rate_limit_pause_ms(&response(429, "retry-after: soon"), 5_000.0),
            5_000.0
        );
        assert_eq!(
            rate_limit_pause_ms(&response(503, "Retry-After: 30"), 5_000.0),
            0.0
        );
    }

    #[test]
    fn discovery_backoff_caps_deduplicates_and_resets_independent_reads() {
        let mut io = Io::<()> {
            core: IoCore {
                entropy: Some(crate::test_support::seeded_entropy(17)),
                ..IoCore::default()
            },
            ..Io::default()
        };
        let mut store = Store::default();
        let mut cx = Cx {
            now: 0.0,
            store: &mut store,
            io: &mut io,
        };
        let mut retry = Backoff::default();
        for expected in [1000., 2000., 4000., 8000., 16000., 30000., 30000.] {
            retry.failed(&mut cx, "markets");
            retry.failed(&mut cx, "markets");
            let timers: Vec<_> = cx
                .io
                .core
                .actions
                .drain(..)
                .filter_map(|a| match a {
                    Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
                    _ => None,
                })
                .collect();
            assert_eq!(timers.len(), 1);
            assert_jittered(timers[0].1, expected);
            assert_eq!(retry.on_timer(timers[0].0), Some("markets"));
            assert_eq!(retry.on_timer(timers[0].0), None);
        }
        retry.failed(&mut cx, "tickers");
        retry.succeeded(&mut cx, &"markets");
        retry.failed(&mut cx, "markets");
        let timers: Vec<_> = cx
            .io
            .core
            .actions
            .drain(..)
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
                _ => None,
            })
            .collect();
        assert_eq!(timers.len(), 2);
        for (_, at) in &timers {
            assert_jittered(*at, 1_000.0);
        }
        retry.succeeded(&mut cx, &"tickers");
        assert_eq!(retry.on_timer(timers[0].0), None);
        assert!(
            cx.io
                .core
                .actions
                .iter()
                .any(|a| matches!(a, Action::ClearTimer { timer } if *timer == timers[0].0))
        );
    }

    #[test]
    fn public_blank_coercion_rejects_nonfinite_or_garbage_numbers() {
        for blank in ["", " ", "\t"] {
            assert_eq!(number_or_zero(blank), Some(0.));
        }
        assert_eq!(number_or_zero("-2.5"), Some(-2.5));
        for bad in ["NaN", "inf", "oops", "1x"] {
            assert_eq!(number_or_zero(bad), None);
        }
    }
}

#[cfg(test)]
mod public_retry_tests {
    use super::*;
    use crate::startup::{DiscoveryRead, PublicRetries, public_response_error};

    fn count(out: &[Action], code: &str) -> usize {
        out.iter()
            .filter(|a| matches!(a, Action::Notice(Notice::Event(e)) if e.code == code))
            .count()
    }
    fn last_timer(out: &[Action]) -> (TimerId, f64) {
        out.iter()
            .rev()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                _ => None,
            })
            .unwrap()
    }
    fn cause() -> VenueError {
        VenueError::new("latest outage cause").kind(ErrorKind::Network)
    }

    #[test]
    fn public_retry_deduplicates_ban_extensions_recovers_and_keeps_repairs_running() {
        let mut io = Io::<()> {
            core: IoCore {
                entropy: Some(crate::test_support::seeded_entropy(17)),
                ..IoCore::default()
            },
            ..Io::default()
        };
        let mut store = Store::default();
        let mut cx = Cx {
            now: 0.0,
            store: &mut store,
            io: &mut io,
        };
        let mut retry = PublicRetries::default();
        let mut waiting = None;
        for attempt in 1..=6 {
            retry.failed_after(&mut cx, DiscoveryRead::Markets, 0.0, cause());
            let out = std::mem::take(&mut cx.io.core.actions);
            assert_eq!(count(&out, log_code::REQUEST_FAILED), 1);
            assert_eq!(
                count(&out, log_code::STARTUP_DEGRADED),
                usize::from(attempt == 6)
            );
            let (old, old_at) = last_timer(&out);
            // Extending an armed timer must not spend another attempt or re-report.
            retry.failed_after(&mut cx, DiscoveryRead::Markets, 120_000.0, cause());
            let out = std::mem::take(&mut cx.io.core.actions);
            assert_eq!(count(&out, log_code::REQUEST_FAILED), 0);
            assert_eq!(count(&out, log_code::STARTUP_DEGRADED), 0);
            assert!(
                out.iter()
                    .any(|a| matches!(a, Action::ClearTimer { timer } if *timer == old))
            );
            let (timer, at) = last_timer(&out);
            assert!(at >= old_at && at >= cx.now + 120_000.0);
            assert_eq!(retry.on_timer(old), None);
            if attempt < 6 {
                cx.now = at;
                assert_eq!(retry.on_timer(timer), Some(DiscoveryRead::Markets));
            } else {
                waiting = Some(timer);
            }
        }
        cx.store
            .set_markets(vec![super::streaming_tests::market("BTC")]);
        retry.ready(&mut cx);
        assert_eq!(count(&cx.io.core.actions, log_code::STARTUP_RECOVERED), 0);
        cx.store.set_tickers(Vec::new());
        retry.ready(&mut cx);
        retry.ready(&mut cx);
        assert_eq!(count(&cx.io.core.actions, log_code::STARTUP_RECOVERED), 1);
        // Becoming ready must not retire an incomplete catalogue's repair timer.
        assert_eq!(
            retry.on_timer(waiting.unwrap()),
            Some(DiscoveryRead::Markets)
        );
        cx.io.core.actions.clear();
        for _ in 0..8 {
            retry.failed_after(&mut cx, DiscoveryRead::Markets, 0.0, cause());
            let out = std::mem::take(&mut cx.io.core.actions);
            assert_eq!(count(&out, log_code::STARTUP_DEGRADED), 0);
            assert_eq!(count(&out, log_code::REQUEST_FAILED), 1);
            let (timer, at) = last_timer(&out);
            cx.now = at;
            assert_eq!(retry.on_timer(timer), Some(DiscoveryRead::Markets));
        }
    }

    #[test]
    fn classified_notices_carry_the_startup_degraded_fields_under_the_callers_code() {
        let mut io = Io::<()>::default();
        let mut store = Store::default();
        let mut cx = Cx {
            now: 0.0,
            store: &mut store,
            io: &mut io,
        };
        let event = |out: Vec<Action>| {
            out.into_iter()
                .find_map(|action| match action {
                    Action::Notice(Notice::Event(event)) => Some(event),
                    _ => None,
                })
                .unwrap()
        };
        let balance = VenueError {
            venue_code: Some("-2019".into()),
            venue_message: Some("Margin is insufficient.".into()),
            ..VenueError::new("Margin is insufficient.").kind(ErrorKind::InsufficientBalance)
        };
        cx.notice_error(Level::Warn, log_code::ORDER_REJECTED, &balance);
        let notice = event(std::mem::take(&mut cx.io.core.actions));
        assert_eq!(notice.code, log_code::ORDER_REJECTED);
        assert_eq!(notice.message, "Margin is insufficient.");
        assert_eq!(
            notice.fields,
            vec![
                ("kind", "insufficientBalance".into()),
                ("venueCode", "-2019".into()),
                ("venueMessage", "Margin is insufficient.".into()),
            ]
        );
        let limited = VenueError {
            retry_after_ms: Some(1_000.0),
            ..VenueError::new("Too many requests").kind(ErrorKind::RateLimited)
        };
        cx.notice_error(Level::Warn, log_code::ORDER_REJECTED, &limited);
        let notice = event(std::mem::take(&mut cx.io.core.actions));
        assert!(notice.fields.contains(&("retryAfterMs", 1_000.0.into())));
        // An auth kind keeps the caller's code and counts no auth failure.
        let scoped =
            VenueError::new("No permission to trade this symbol").kind(ErrorKind::AuthPermission);
        cx.notice_error(Level::Warn, log_code::ORDER_REJECTED, &scoped);
        let notice = event(std::mem::take(&mut cx.io.core.actions));
        assert_eq!(notice.code, log_code::ORDER_REJECTED);
        assert_eq!(notice.fields, vec![("kind", "authPermission".into())]);
        assert!(
            !cx.io.core.auth_failure_counted,
            "an order refusal is no auth evidence"
        );
        // A read failure stays request.failed, with retried_failure's cadence.
        let rejected = VenueError::new("API-key format invalid.").kind(ErrorKind::AuthInvalid);
        cx.retried_failure_error("site", &rejected);
        let first = event(std::mem::take(&mut cx.io.core.actions));
        assert_eq!(
            (first.code, first.level),
            (log_code::REQUEST_FAILED, Level::Error)
        );
        assert_eq!(first.fields, vec![("kind", "authInvalid".into())]);
        assert!(!cx.io.core.auth_failure_counted);
        cx.retried_failure_error("site", &rejected);
        assert_eq!(
            event(std::mem::take(&mut cx.io.core.actions)).level,
            Level::Warn
        );
        let plain = VenueError::new("Binance account: network error");
        cx.retried_failure_error("other", &plain);
        let other = event(std::mem::take(&mut cx.io.core.actions));
        assert_eq!(other.code, log_code::REQUEST_FAILED);
        assert!(
            other.fields.is_empty(),
            "an unclassified error adds no fields"
        );
    }

    #[test]
    fn public_retry_counts_reads_independently_and_reports_only_one_outage() {
        let mut io = Io::<()> {
            core: IoCore {
                entropy: Some(crate::test_support::seeded_entropy(17)),
                ..IoCore::default()
            },
            ..Io::default()
        };
        let mut store = Store::default();
        let mut cx = Cx {
            now: 0.0,
            store: &mut store,
            io: &mut io,
        };
        let mut retry = PublicRetries::default();
        for attempt in 1..=6 {
            for read in [DiscoveryRead::Markets, DiscoveryRead::Tickers] {
                let error = public_response_error(429, Some("busy"), Some("maintenance"));
                retry.failed_after(&mut cx, read, 10_000.0, error);
                let out = std::mem::take(&mut cx.io.core.actions);
                assert_eq!(
                    count(&out, log_code::STARTUP_DEGRADED),
                    usize::from(attempt == 6 && read == DiscoveryRead::Markets)
                );
                if attempt == 6 && read == DiscoveryRead::Markets {
                    let event = out
                        .iter()
                        .find_map(|a| match a {
                            Action::Notice(Notice::Event(e))
                                if e.code == log_code::STARTUP_DEGRADED =>
                            {
                                Some(e)
                            }
                            _ => None,
                        })
                        .unwrap();
                    assert!(event.fields.contains(&("kind", "rateLimited".into())));
                    assert!(event.fields.contains(&("venueCode", "busy".into())));
                    assert!(
                        event
                            .fields
                            .contains(&("venueMessage", "maintenance".into()))
                    );
                    assert!(event.fields.contains(&("retryAfterMs", 10_000.0.into())));
                }
                let (timer, at) = last_timer(&out);
                cx.now = at;
                assert_eq!(retry.on_timer(timer), Some(read));
            }
        }
        retry.succeeded(&mut cx, DiscoveryRead::Markets);
        assert_eq!(count(&cx.io.core.actions, log_code::STARTUP_RECOVERED), 0);
    }

    #[test]
    fn public_response_causes_are_typed_and_do_not_require_raw_bodies() {
        assert_eq!(
            public_response_error(429, None, None).kind,
            Some(ErrorKind::RateLimited)
        );
        let malformed = public_response_error(200, None, None);
        assert_eq!(malformed.kind, Some(ErrorKind::VenueUnavailable));
        assert_eq!(malformed.message, "Malformed public discovery response");
        for kind in [
            ErrorKind::Rejected,
            ErrorKind::InvalidInput,
            ErrorKind::InsufficientBalance,
            ErrorKind::AuthInvalid,
            ErrorKind::AuthExpired,
            ErrorKind::AuthPermission,
            ErrorKind::RateLimited,
            ErrorKind::Network,
            ErrorKind::Timeout,
            ErrorKind::VenueUnavailable,
            ErrorKind::Unsupported,
            ErrorKind::Internal,
        ] {
            assert_eq!(
                serde_json::to_string(&kind).unwrap(),
                format!("\"{}\"", kind.as_str())
            );
        }
    }
}

#[cfg(test)]
mod startup_watch_tests {
    use super::streaming_tests::market;
    use super::*;

    struct Idle;
    impl Venue for Idle {
        type Pending = ();
        fn start(&mut self, _cx: &mut Cx<'_, ()>) {}
        fn on_open(&mut self, _cx: &mut Cx<'_, ()>, _socket: SocketId) {}
        fn on_message(&mut self, _cx: &mut Cx<'_, ()>, _socket: SocketId, _text: &str) {}
        fn on_close(&mut self, _cx: &mut Cx<'_, ()>, _socket: SocketId, _code: u16) {}
        fn on_response(&mut self, _cx: &mut Cx<'_, ()>, _pending: (), _response: Response<'_>) {}
        fn on_request_failed(&mut self, _cx: &mut Cx<'_, ()>, _pending: (), _error: &str) {}
        fn on_timer(&mut self, _cx: &mut Cx<'_, ()>, _timer: TimerId) {
            panic!("the idle venue sets no timers");
        }
        fn on_command(&mut self, _cx: &mut Cx<'_, ()>, _command: Command) {}
        fn book(&self, _symbol: &str) -> Option<&Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }

    fn notices(engine: &mut Engine<Idle>, now: f64) -> Vec<(Level, String)> {
        engine.handle(now, Event::PollScaleChanged { scale: 1.0 });
        engine
            .drain_actions()
            .filter_map(|action| match action {
                Action::Notice(Notice::Event(crate::log::LogEvent { level, message, .. })) => {
                    Some((level, message))
                }
                _ => None,
            })
            .collect()
    }

    /// A venue with one 2 s native poll, so a scale is the host interval / 2000.
    struct PollVenue;
    impl crate::Venue for PollVenue {
        type Pending = ();
        fn start(&mut self, cx: &mut crate::Cx<'_, ()>) {
            cx.set_poll_timer(2_000.0);
        }
        fn on_open(&mut self, _: &mut crate::Cx<'_, ()>, _: crate::SocketId) {}
        fn on_message(&mut self, _: &mut crate::Cx<'_, ()>, _: crate::SocketId, _: &str) {}
        fn on_close(&mut self, _: &mut crate::Cx<'_, ()>, _: crate::SocketId, _: u16) {}
        fn on_response(&mut self, _: &mut crate::Cx<'_, ()>, _: (), _: crate::Response<'_>) {}
        fn on_request_failed(&mut self, _: &mut crate::Cx<'_, ()>, _: (), _: &str) {}
        fn on_timer(&mut self, _: &mut crate::Cx<'_, ()>, _: crate::TimerId) {}
        fn on_command(&mut self, _: &mut crate::Cx<'_, ()>, _: crate::Command) {}
        fn book(&self, _: &str) -> Option<&crate::book::Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &crate::book::Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }
    /// The effective poll deadline after a policy change on a fresh engine:
    /// the last one armed, counting the native arm at startup.
    fn armed_after(change: impl FnOnce(&mut crate::Engine<PollVenue>)) -> Vec<f64> {
        let mut engine = crate::test_support::seeded_engine(PollVenue);
        engine.start(0.0);
        change(&mut engine);
        let armed: Vec<_> = engine
            .drain_actions()
            .filter_map(|action| match action {
                crate::Action::SetTimer { at_ms, .. } => Some(at_ms),
                _ => None,
            })
            .collect();
        armed.last().copied().into_iter().collect()
    }

    // Port of #1351: typed native policy events obey the same bounds as host
    // commands, and invalid typed inputs retain the last policy.
    #[test]
    fn typed_native_tick_events_obey_the_same_bounds_as_host_commands() {
        for (input, expected) in [(0.00001, 1_000.0), (1e300, 120_000.0)] {
            let scale = input / 2_000.0;
            let typed = armed_after(|engine| {
                engine.handle(0.0, Event::PollScaleChanged { scale });
            });
            assert_eq!(typed, [expected]);
            #[cfg(feature = "polling-control")]
            {
                let json = format!(r#"{{"op":"setPollScale","scale":{scale}}}"#);
                let crate::command::HostInput::PollScaleChanged { scale: decoded, .. } =
                    crate::command::decode_host_input(&json).unwrap()
                else {
                    panic!("{json} must decode as a polling control")
                };
                let commanded = armed_after(|engine| {
                    engine.handle(0.0, Event::PollScaleChanged { scale: decoded });
                });
                assert_eq!(typed, commanded, "typed and host-command bounds agree");
            }
        }
        let mut engine = crate::test_support::seeded_engine(PollVenue);
        engine.start(0.0);
        engine.handle(
            0.0,
            Event::PollScaleChanged {
                scale: f64::INFINITY,
            },
        );
        assert!(
            engine
                .drain_actions()
                .any(|action| matches!(action, Action::ClearTimer { .. }))
        );
        for invalid in [0.0, -1.0, f64::NEG_INFINITY, f64::NAN] {
            engine.handle(0.0, Event::PollScaleChanged { scale: invalid });
            assert!(
                !engine
                    .drain_actions()
                    .any(|action| matches!(action, Action::SetTimer { .. })),
                "{invalid} must retain the pause"
            );
        }
    }

    #[test]
    fn an_empty_venue_catalogue_is_warned_about_and_never_applied() {
        let mut engine = crate::test_support::seeded_engine(Idle);
        engine.start(0.0);
        engine.store_mut().set_markets(vec![market("A")]);
        engine.store_mut().set_markets(Vec::new());
        assert!(engine.has_actions());
        let logs: Vec<_> = engine
            .drain_actions()
            .filter_map(|action| match action {
                Action::Notice(Notice::Event(crate::log::LogEvent { level, message, .. })) => {
                    Some((level, message))
                }
                _ => None,
            })
            .collect();
        assert_eq!(logs, [(Level::Warn, EMPTY_CATALOGUE_WARNING.to_owned())]);
        assert_eq!(engine.store().markets().len(), 1);
        assert_eq!(engine.health().snapshots_discarded, 1);
        assert!(engine.drain_actions().next().is_none());
        engine.set_log_level(Level::Error);
        engine.store_mut().set_markets(Vec::new());
        assert!(engine.drain_actions().next().is_none());
        assert_eq!(engine.health().snapshots_discarded, 2);
    }

    #[test]
    fn unloaded_startup_is_reported_until_every_section_loads() {
        let mut engine = crate::test_support::seeded_engine(Idle);
        engine.set_account_startup(true);
        engine.start(0.0);
        assert!(
            engine
                .drain_actions()
                .all(|action| !matches!(action, Action::SetTimer { .. })),
            "the watch adds no timer of its own"
        );
        engine.store_mut().set_markets(vec![market("A")]);
        assert!(notices(&mut engine, STARTUP_WATCH_MS - 1.0).is_empty());
        assert_eq!(
            notices(&mut engine, STARTUP_WATCH_MS),
            [(
                Level::Error,
                "Startup incomplete: still waiting for tickers, balance, orders, positions"
                    .to_owned()
            )]
        );
        assert!(notices(&mut engine, STARTUP_WATCH_MS + 1.0).is_empty());
        let store = engine.store_mut();
        store.set_balance(Default::default());
        store.set_orders(vec![]);
        store.set_positions(vec![]);
        let repeat = STARTUP_WATCH_MS + STARTUP_REPEAT_MS;
        assert_eq!(
            notices(&mut engine, repeat),
            [(
                Level::Warn,
                "Startup incomplete: still waiting for tickers".to_owned()
            )]
        );
        engine.store_mut().set_tickers(vec![]);
        assert!(notices(&mut engine, repeat + STARTUP_REPEAT_MS).is_empty());
        assert!(notices(&mut engine, repeat + 10.0 * STARTUP_REPEAT_MS).is_empty());
    }

    #[test]
    fn public_only_engine_is_not_told_its_account_never_loaded() {
        let mut engine = crate::test_support::seeded_engine(Idle);
        engine.start(0.0);
        engine.store_mut().set_markets(vec![market("A")]);
        engine.store_mut().set_tickers(vec![]);
        assert!(notices(&mut engine, STARTUP_WATCH_MS).is_empty());
        let mut disabled = crate::test_support::seeded_engine(Idle);
        disabled
            .set_public_feed_policy(PublicFeedPolicy::Disabled)
            .unwrap();
        disabled.set_account_startup(true);
        disabled.start(0.0);
        let store = disabled.store_mut();
        store.set_markets(vec![market("A")]);
        store.set_balance(Default::default());
        store.set_orders(vec![]);
        store.set_positions(vec![]);
        assert!(notices(&mut disabled, STARTUP_WATCH_MS).is_empty());
    }
}

#[cfg(test)]
mod placement_ack_tests {
    use super::*;
    use crate::model::{Order, OrderSide, OrderStatus, OrderType};
    use crate::placement_acks::{ACK_WAIT_MS, FILL_GRACE_MS};

    /// Routes cancellation only through stored orders, like Bybit or Binance.
    struct StoredOnly {
        needs_stored: bool,
        sent: Vec<String>,
    }

    fn order(id: &str) -> Order {
        Order {
            id: id.into(),
            parent_id: None,
            status: OrderStatus::Open,
            symbol: "BTCUSDT".into(),
            kind: OrderType::Limit,
            side: OrderSide::Buy,
            price: 1.,
            amount: 1.,
            filled: 0.,
            remaining: 1.,
            reduce_only: false,
            close: None,
            client_order_id: None,
            time_in_force: None,
            last_update: 0.,
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: None,
        }
    }

    impl Venue for StoredOnly {
        type Pending = ();
        fn cancel_needs_stored_order(&self) -> bool {
            self.needs_stored
        }
        fn start(&mut self, _cx: &mut Cx<'_, ()>) {}
        fn on_open(&mut self, _cx: &mut Cx<'_, ()>, _socket: SocketId) {}
        /// The private stream reports the order named by the message, a group
        /// child (`child@group`), or a fill of an unstored order (`fill:id`).
        fn on_message(&mut self, cx: &mut Cx<'_, ()>, _socket: SocketId, text: &str) {
            if let Some(id) = text.strip_prefix("remove:") {
                cx.store.remove_orders(&[id]);
            } else if let Some(id) = text.strip_prefix("fill:") {
                cx.fill(Fill {
                    id: None,
                    order_id: id.into(),
                    symbol: "BTCUSDT".into(),
                    side: OrderSide::Buy,
                    price: 1.,
                    amount: 1.,
                });
            } else if let Some((child, group)) = text.split_once('@') {
                let mut row = order(child);
                row.parent_id = Some(group.into());
                cx.store.upsert_order(row);
            } else {
                cx.store.upsert_order(order(text));
            }
        }
        fn on_close(&mut self, _cx: &mut Cx<'_, ()>, _socket: SocketId, _code: u16) {}
        fn on_response(&mut self, _cx: &mut Cx<'_, ()>, _pending: (), _response: Response<'_>) {}
        fn on_request_failed(&mut self, _cx: &mut Cx<'_, ()>, _pending: (), _error: &str) {}
        fn on_timer(&mut self, _cx: &mut Cx<'_, ()>, _timer: TimerId) {
            panic!("the venue sets no timers");
        }
        fn on_command(&mut self, cx: &mut Cx<'_, ()>, command: Command) {
            match command {
                Command::PlaceOrders { command, .. } => cx.done(command, r#"["7","8"]"#.into()),
                Command::CancelOrders { command, ids } => {
                    if let Some(id) = ids.iter().find(|id| cx.store.order(id).is_none()) {
                        cx.fail(command, format!("Order {id} not found"));
                    } else {
                        self.sent.extend(ids);
                        cx.done(command, "null".into());
                    }
                }
                Command::AmendOrder { command, id, .. } => {
                    if cx.store.order(&id).is_none() {
                        cx.fail(command, format!("Order {id} not found"));
                    } else {
                        self.sent.push(format!("amend {id}"));
                        cx.done(command, format!(r#"["{id}"]"#));
                    }
                }
                _ => {}
            }
        }
        fn book(&self, _symbol: &str) -> Option<&Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }

    fn engine(needs_stored: bool) -> Engine<StoredOnly> {
        let mut engine = crate::test_support::seeded_engine(StoredOnly {
            needs_stored,
            sent: Vec::new(),
        });
        engine.start(0.);
        engine
    }

    fn place(engine: &mut Engine<StoredOnly>, now: f64) {
        engine.handle(
            now,
            Event::Command(Command::PlaceOrders {
                command: 1,
                orders: Vec::new(),
            }),
        );
    }

    fn cancel(engine: &mut Engine<StoredOnly>, now: f64, command: CommandId, id: &str) {
        engine.handle(
            now,
            Event::Command(Command::CancelOrders {
                command,
                ids: vec![id.into()],
            }),
        );
    }

    fn results(engine: &mut Engine<StoredOnly>) -> Vec<(CommandId, Result<String, String>)> {
        engine
            .drain_actions()
            .filter_map(|action| match action {
                Action::Notice(Notice::CommandDone { command, result }) => {
                    Some((command, Ok(result)))
                }
                Action::Notice(Notice::CommandFailed { command, error }) => {
                    Some((command, Err(error.legacy_text())))
                }
                _ => None,
            })
            .collect()
    }

    fn own_timer(engine: &mut Engine<StoredOnly>) -> Option<(TimerId, f64)> {
        engine
            .drain_actions()
            .fold(None, |armed, action| match action {
                Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
                Action::ClearTimer { .. } => None,
                _ => armed,
            })
    }

    #[test]
    fn cancel_of_an_acknowledged_placement_waits_for_the_stream_then_reaches_the_venue() {
        let mut engine = engine(true);
        place(&mut engine, 0.);
        assert_eq!(results(&mut engine), [(1, Ok(r#"["7","8"]"#.into()))]);
        // A chaser stopped mid-placement cancels the returned id at once.
        cancel(&mut engine, 1., 2, "7");
        assert!(
            engine.venue.sent.is_empty(),
            "held until the store has the order"
        );
        assert!(results(&mut engine).is_empty());
        engine.handle(
            2.,
            Event::WsMessage {
                socket: SocketId(1),
                text: "7",
            },
        );
        assert_eq!(engine.venue.sent, ["7"]);
        assert_eq!(results(&mut engine), [(2, Ok("null".into()))]);
    }

    #[test]
    fn an_amendment_of_an_acknowledged_placement_waits_too() {
        let mut engine = engine(true);
        place(&mut engine, 0.);
        engine.drain_actions().for_each(drop);
        engine.handle(
            1.,
            Event::Command(Command::AmendOrder {
                command: 2,
                id: "7".into(),
                price: Some(2.),
                amount: None,
            }),
        );
        assert!(engine.venue.sent.is_empty());
        engine.handle(
            2.,
            Event::WsMessage {
                socket: SocketId(1),
                text: "7",
            },
        );
        assert_eq!(engine.venue.sent, ["amend 7"]);
    }

    #[test]
    fn a_held_cancel_is_dispatched_at_its_deadline_and_fails_explicitly() {
        let mut engine = engine(true);
        place(&mut engine, 0.);
        engine.drain_actions().for_each(drop);
        cancel(&mut engine, 1., 2, "7");
        let (timer, at_ms) = own_timer(&mut engine).expect("an engine wake-up is armed");
        // The wait is bounded by the acknowledgement (placed at 0), not the cancel.
        assert_eq!(at_ms, ACK_WAIT_MS);
        engine.handle(at_ms, Event::TimerFired { timer });
        assert_eq!(
            results(&mut engine),
            [(2, Err("Order 7 not found".into()))],
            "the stream never reported the order: the venue answers, never silence"
        );
        // The expired id is not held again.
        cancel(&mut engine, at_ms + 1., 3, "7");
        assert_eq!(results(&mut engine), [(3, Err("Order 7 not found".into()))]);
    }

    #[test]
    fn released_cancel_disarms_the_wake_up() {
        let mut engine = engine(true);
        place(&mut engine, 0.);
        engine.drain_actions().for_each(drop);
        cancel(&mut engine, 1., 2, "7");
        let (timer, _) = own_timer(&mut engine).unwrap();
        engine.handle(
            2.,
            Event::WsMessage {
                socket: SocketId(1),
                text: "7",
            },
        );
        assert!(engine.drain_actions().any(
            |action| matches!(action, Action::ClearTimer { timer: cleared } if cleared == timer)
        ));
    }

    #[test]
    fn unrelated_ids_and_opted_out_venues_are_never_held() {
        let mut engine = engine(true);
        place(&mut engine, 0.);
        engine.drain_actions().for_each(drop);
        cancel(&mut engine, 1., 2, "unknown");
        assert_eq!(
            results(&mut engine),
            [(2, Err("Order unknown not found".into()))]
        );

        let mut engine = engine_without_hold();
        place(&mut engine, 0.);
        engine.drain_actions().for_each(drop);
        cancel(&mut engine, 1., 2, "7");
        assert_eq!(results(&mut engine), [(2, Err("Order 7 not found".into()))]);
    }

    fn stream(engine: &mut Engine<StoredOnly>, now: f64, text: &str) {
        engine.handle(
            now,
            Event::WsMessage {
                socket: SocketId(1),
                text,
            },
        );
    }

    #[test]
    fn stored_legs_and_group_children_release_a_held_cancel() {
        // OKX stores an algo order only as its `_sl` leg.
        let mut engine = engine(true);
        place(&mut engine, 0.);
        engine.drain_actions().for_each(drop);
        cancel(&mut engine, 1., 2, "7");
        stream(&mut engine, 2., "7_sl");
        assert!(
            results(&mut engine)
                .iter()
                .any(|(command, _)| *command == 2),
            "a stored leg is the order"
        );
        // Orderly stores only a TP/SL group's children, each naming the group.
        let mut grouped = engine_with_hold();
        place(&mut grouped, 0.);
        grouped.drain_actions().for_each(drop);
        cancel(&mut grouped, 1., 2, "7");
        stream(&mut grouped, 2., "70@7");
        assert!(
            results(&mut grouped)
                .iter()
                .any(|(command, _)| *command == 2)
        );
    }

    #[test]
    fn a_fill_of_an_unstored_order_shortens_its_wait_to_the_grace_period() {
        let mut engine = engine(true);
        place(&mut engine, 0.);
        engine.drain_actions().for_each(drop);
        cancel(&mut engine, 1., 2, "7");
        engine.drain_actions().for_each(drop);
        // A venue may drop a filled order rather than store it.
        stream(&mut engine, 2., "fill:7");
        let (timer, at_ms) = own_timer(&mut engine).expect("re-armed for the grace period");
        assert_eq!(at_ms, 2. + FILL_GRACE_MS);
        engine.handle(at_ms, Event::TimerFired { timer });
        assert_eq!(results(&mut engine), [(2, Err("Order 7 not found".into()))]);
    }

    #[test]
    fn a_partial_fill_before_the_order_row_keeps_the_cancel_until_the_row_lands() {
        let mut engine = engine(true);
        place(&mut engine, 0.);
        engine.drain_actions().for_each(drop);
        cancel(&mut engine, 1., 2, "7");
        // Bybit and Hyperliquid fills arrive on a topic separate from orders.
        stream(&mut engine, 2., "fill:7");
        assert!(
            results(&mut engine).is_empty(),
            "the remainder may still be live"
        );
        stream(&mut engine, 3., "7");
        assert_eq!(results(&mut engine), [(2, Ok("null".into()))]);
    }

    #[test]
    fn a_released_cancel_is_not_held_again_behind_a_later_one() {
        let mut engine = engine(true);
        place(&mut engine, 0.);
        engine.drain_actions().for_each(drop);
        cancel(&mut engine, 1., 2, "7");
        engine.handle(
            2.,
            Event::Command(Command::CancelOrders {
                command: 3,
                ids: vec!["7".into(), "8".into()],
            }),
        );
        stream(&mut engine, 3., "7");
        assert_eq!(
            results(&mut engine),
            [(2, Ok("null".into()))],
            "the first cancel goes out once 7 is stored; the second still waits for 8"
        );
        stream(&mut engine, 4., "8");
        assert_eq!(results(&mut engine), [(3, Ok("null".into()))]);
    }

    #[test]
    fn a_later_write_to_a_held_order_keeps_its_arrival_order() {
        let mut engine = engine(true);
        place(&mut engine, 0.);
        engine.drain_actions().for_each(drop);
        engine.handle(
            1.,
            Event::Command(Command::AmendOrder {
                command: 2,
                id: "7".into(),
                price: Some(2.),
                amount: None,
            }),
        );
        assert!(results(&mut engine).is_empty());
        // The id's own wait has ended while the amend is still held: the cancel
        // must not overtake it; the amend goes out first.
        cancel(&mut engine, ACK_WAIT_MS + 0.5, 3, "7");
        let order: Vec<_> = results(&mut engine)
            .into_iter()
            .map(|(command, _)| command)
            .collect();
        assert_eq!(order, [2, 3]);
    }

    #[test]
    fn a_fill_of_one_id_keeps_the_wait_for_the_others_in_the_command() {
        let mut engine = engine(true);
        place(&mut engine, 0.);
        engine.drain_actions().for_each(drop);
        engine.handle(
            1.,
            Event::Command(Command::CancelOrders {
                command: 2,
                ids: vec!["7".into(), "8".into()],
            }),
        );
        stream(&mut engine, 2., "fill:8");
        let (timer, at_ms) = own_timer(&mut engine).unwrap();
        assert_eq!(at_ms, ACK_WAIT_MS, "7 is still awaited for its full wait");
        // 7's row lands after the fill's grace; the cancel must still wait for it.
        engine.handle(2. + FILL_GRACE_MS + 1., Event::TimerFired { timer });
        assert!(results(&mut engine).is_empty());
        stream(&mut engine, 4000., "7");
        assert_eq!(results(&mut engine), [(2, Err("Order 8 not found".into()))]);
    }

    #[test]
    fn the_wake_up_never_targets_a_command_waiting_behind_another() {
        let mut engine = engine(true);
        place(&mut engine, 0.);
        engine.drain_actions().for_each(drop);
        cancel(&mut engine, 1., 2, "7");
        engine.handle(
            1.5,
            Event::Command(Command::CancelOrders {
                command: 3,
                ids: vec!["7".into(), "8".into()],
            }),
        );
        stream(&mut engine, 2., "fill:8");
        let (timer, at_ms) = own_timer(&mut engine).unwrap();
        assert_eq!(
            at_ms, ACK_WAIT_MS,
            "cancel 3 is behind cancel 2, which waits for 7"
        );
        engine.handle(at_ms, Event::TimerFired { timer });
        let order: Vec<_> = results(&mut engine)
            .into_iter()
            .map(|(command, _)| command)
            .collect();
        assert_eq!(order, [2, 3]);
        assert!(own_timer(&mut engine).is_none(), "nothing left to wake for");
    }

    #[test]
    fn an_order_stored_then_removed_does_not_hold_a_later_cancel() {
        // Hyperliquid removes terminal orders from the store.
        let mut engine = engine(true);
        place(&mut engine, 0.);
        stream(&mut engine, 1., "7");
        stream(&mut engine, 2., "remove:7");
        engine.drain_actions().for_each(drop);
        cancel(&mut engine, 3., 2, "7");
        assert_eq!(results(&mut engine), [(2, Err("Order 7 not found".into()))]);
    }

    fn engine_with_hold() -> Engine<StoredOnly> {
        engine(true)
    }

    fn engine_without_hold() -> Engine<StoredOnly> {
        engine(false)
    }
}

#[cfg(test)]
mod fill_frame_tests {
    use super::streaming_tests::market;
    use super::*;
    use crate::model::{Order, OrderSide, OrderStatus, OrderType};

    /// The private stream updates an order (`order`) or fills it (`fill`).
    struct Stream;

    fn order() -> Order {
        Order {
            id: "7".into(),
            parent_id: None,
            status: OrderStatus::Closed,
            symbol: "A".into(),
            kind: OrderType::Limit,
            side: OrderSide::Buy,
            price: 1.,
            amount: 1.,
            filled: 1.,
            remaining: 0.,
            reduce_only: false,
            close: None,
            client_order_id: None,
            time_in_force: None,
            last_update: 0.,
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: None,
        }
    }

    impl Venue for Stream {
        type Pending = ();
        fn start(&mut self, _cx: &mut Cx<'_, ()>) {}
        fn on_open(&mut self, _cx: &mut Cx<'_, ()>, _socket: SocketId) {}
        fn on_message(&mut self, cx: &mut Cx<'_, ()>, _socket: SocketId, text: &str) {
            cx.store.upsert_order(order());
            if text == "fill" {
                cx.fill(Fill {
                    id: None,
                    order_id: "7".into(),
                    symbol: "A".into(),
                    side: OrderSide::Buy,
                    price: 1.,
                    amount: 1.,
                });
            }
        }
        fn on_close(&mut self, _cx: &mut Cx<'_, ()>, _socket: SocketId, _code: u16) {}
        fn on_response(&mut self, _cx: &mut Cx<'_, ()>, _pending: (), _response: Response<'_>) {}
        fn on_request_failed(&mut self, _cx: &mut Cx<'_, ()>, _pending: (), _error: &str) {}
        fn on_timer(&mut self, _cx: &mut Cx<'_, ()>, _timer: TimerId) {}
        fn on_command(&mut self, _cx: &mut Cx<'_, ()>, _command: Command) {}
        fn book(&self, _symbol: &str) -> Option<&Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }

    fn started() -> Engine<Stream> {
        let mut engine = crate::test_support::seeded_engine(Stream);
        engine.start(0.);
        engine.store_mut().set_markets(vec![market("A")]);
        engine.take_frame(0.);
        engine
    }

    fn kinds(frame: Option<&[u8]>) -> Vec<u16> {
        frame
            .map(|frame| crate::wire::sections(frame).map(|(kind, _)| kind).collect())
            .unwrap_or_default()
    }

    fn message(engine: &mut Engine<Stream>, text: &str) {
        engine.handle(
            1.,
            Event::WsMessage {
                socket: SocketId(1),
                text,
            },
        );
    }

    #[test]
    fn a_book_cadence_flush_carries_the_orders_its_fills_belong_to() {
        let mut engine = started();
        message(&mut engine, "fill");
        let sections = kinds(engine.take_market_frame(1.));
        assert!(sections.contains(&section::FILLS));
        assert!(
            sections.contains(&section::ORDERS),
            "a fill must never reach a consumer before its order: {sections:?}"
        );
        assert!(
            kinds(engine.take_frame(2.)).is_empty(),
            "nothing is sent twice"
        );
    }

    #[test]
    fn a_book_cadence_flush_without_fills_leaves_store_sections_to_the_full_frame() {
        let mut engine = started();
        message(&mut engine, "order");
        assert!(!kinds(engine.take_market_frame(1.)).contains(&section::ORDERS));
        assert!(kinds(engine.take_frame(2.)).contains(&section::ORDERS));
    }
}

#[cfg(test)]
#[path = "signing_tests.rs"]
mod dispatch_signing_tests;

#[cfg(all(test, feature = "order-locks", feature = "http-plans"))]
#[path = "http_plan_boundary_tests.rs"]
mod http_plan_boundary_tests;

#[cfg(feature = "http-plans")]
#[path = "http_plan_expiry.rs"]
mod http_plan_expiry;
