//! ApeX Omni perpetuals, driven entirely by host events.
mod account;
mod history;
mod history_wire;
mod private_wire;
mod public;
mod public_wire;
mod read;
pub mod sign;
mod trading;

use safe_cex_engine::book::Book;
use safe_cex_engine::credential_selection::{SlotPolicy, Slots};
use safe_cex_engine::signing::Identity;
use read::Split;
use safe_cex_engine::{Command, Cx, Response, SocketId, TimerId, Venue};
use std::collections::HashMap;

#[derive(Clone, Default)]
pub struct Options {
    pub testnet: bool,
    pub public_data_only: bool,
    /// The provider identity for every signed request: opaque refs only. The
    /// venue holds no key material; the zkLink seed is its `zklinkSeed` role.
    pub identity: Option<safe_cex_engine::signing::Identity>,
    pub tick_interval_ms: Option<f64>,
}
safe_cex_crypto::redacted_debug!(Options);

pub enum Pending {
    Candles(public::Candles),
    Public(read::Read, public::Request),
    Account(read::Read, account::Request),
    Trade(trading::Request),
    History(read::Read, history::Request),
}

pub struct ApexOmni {
    options: Options,
    /// Primary-only: ApeX has no separate read key.
    slots: Slots<Identity>,
    rest: &'static str,
    contracts: HashMap<String, public::ContractConfig>,
    public: public::Public,
    account: account::Account,
    trading: trading::Trading,
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum Code {
    Integer(i64),
    Text(String),
    Number(f64),
    Invalid(serde::de::IgnoredAny),
}
impl Code {
    fn number(&self) -> Option<i64> {
        match self {
            Self::Integer(value) => Some(*value),
            Self::Text(value) => Some(value.parse().unwrap_or(i64::MIN)),
            Self::Number(value) => Some(if *value == 0.0 { 0 } else { i64::MIN }),
            Self::Invalid(_) => Some(i64::MIN),
        }
    }
}
#[derive(serde::Deserialize)]
struct Envelope<'a> {
    code: Option<Code>,
    #[serde(borrow, default)]
    msg: std::borrow::Cow<'a, str>,
}

enum ApiResponse {
    Success,
    SignatureRefusal(safe_cex_engine::VenueError),
    Failed(safe_cex_engine::VenueError),
}

fn api_response(response: Response<'_>) -> ApiResponse {
    let envelope = serde_json::from_str::<Envelope<'_>>(response.body).ok();
    if envelope.is_none() && response.body.trim_start().starts_with('{') {
        return ApiResponse::Failed(trading::unknown(
            "ApeX Omni response contains malformed error metadata",
        ));
    }
    let code = envelope
        .as_ref()
        .and_then(|e| e.code.as_ref())
        .and_then(Code::number);
    if (200..300).contains(&response.status) && code.is_none_or(|code| code == 0) {
        return ApiResponse::Success;
    }
    let signature = envelope.as_ref().is_some_and(|e| {
        code.is_some_and(|code| code != 0)
            && (matches!(code, Some(20016 | 401)) || e.msg.to_ascii_lowercase().contains("sign"))
    });
    let error = trading::failure(
        code,
        response.status,
        format!(
            "ApeX Omni request failed [{}]: {}",
            code.unwrap_or(i64::from(response.status)),
            envelope
                .as_ref()
                .map(|e| e.msg.as_ref())
                .unwrap_or("invalid response")
        ),
    );
    if signature {
        ApiResponse::SignatureRefusal(error)
    } else {
        ApiResponse::Failed(error)
    }
}

impl ApexOmni {
    pub fn new(options: Options) -> Self {
        Self {
            rest: if options.testnet {
                "https://testnet.omni.apex.exchange/api"
            } else {
                "https://omni.apex.exchange/api"
            },
            slots: Slots::new(options.identity.clone(), None, SlotPolicy::default()),
            options,
            contracts: HashMap::new(),
            public: public::Public::default(),
            account: account::Account::new(),
            trading: trading::Trading::default(),
        }
    }

    fn response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        match read::split(pending) {
            Split::Read(read, native) => self.read_response(cx, read, native, response),
            Split::Candles(pending) => self.candle_response(cx, pending, response),
            Split::Trade(pending) => self.trading_response(cx, pending, response),
        }
    }

    fn transport_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match read::split(pending) {
            Split::Read(read, native) => self.read_transport_failed(cx, read, native, error),
            Split::Candles(pending) => self.candle_transport_failed(cx, pending, error.into()),
            Split::Trade(pending) => {
                let error = trading::unknown(error).kind(safe_cex_engine::ErrorKind::Network);
                self.trading_failed(cx, pending, error)
            }
        }
    }

    fn failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        error: safe_cex_engine::VenueError,
    ) {
        match read::split(pending) {
            Split::Read(read, native) => self.read_failed(cx, read, native, error),
            Split::Candles(pending) => Self::candle_failed(cx, pending, error),
            Split::Trade(pending) => self.trading_failed(cx, pending, error),
        }
    }
}

impl Venue for ApexOmni {
    fn capture_http_reconciliation(
        &self,
        pending: &Pending,
    ) -> Result<
        safe_cex_engine::request_context::ReconciliationData,
        safe_cex_engine::request_context::Error,
    > {
        match pending {
            Pending::Trade(delivery) => trading::capture_reconciliation(delivery),
            _ => Err(safe_cex_engine::request_context::Error::Invalid),
        }
    }
    fn unproven_http_evidence(
        &self,
        pending: &Pending,
        response: Response<'_>,
        evidence: &mut safe_cex_engine::request_context::UnprovenEvidence,
    ) {
        if let Pending::Trade(request) = pending {
            trading::capture_unproven(request, response, evidence);
        }
    }
    fn on_http_owner_retired_preflight(
        &mut self,
        owner: &safe_cex_engine::request_context::HttpOwnerRetirement,
    ) -> Option<safe_cex_engine::request_context::Preflight> {
        self.trading_retired(owner)
    }
    fn on_http_preflight_ready(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        preflight: &safe_cex_engine::request_context::Preflight,
    ) {
        self.trading_ready(cx, preflight);
    }
    fn on_http_plan_ready(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        plan: &safe_cex_engine::request_context::Plan,
    ) {
        self.trading_plan_ready(cx, plan);
    }
    fn on_sign_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        error: safe_cex_engine::VenueError,
    ) {
        self.failed(cx, pending, error);
    }
    fn guard_order_mutations(&self) -> bool {
        true
    }

    type Pending = Pending;
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            fill_history_time: safe_cex_engine::FillHistoryTime::SecondsOrMilliseconds,
            account: safe_cex_engine::AccountLookup::Query,
            order_history: false,
            income_history: false,
            ..safe_cex_engine::Capabilities::BASE
        }
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.start_public(cx);
    }
    fn on_public_feed(&mut self, cx: &mut Cx<'_, Pending>) {
        self.revalue_public_positions(cx);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if !self.public_open(cx, socket) {
            self.account_open(cx, socket);
        }
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if !self.public_message(cx, socket, text) {
            self.account_message(cx, socket, text);
        }
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _code: u16) {
        if !self.public_close(cx, socket) {
            self.account_close(cx, socket);
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.response(cx, pending, response);
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        self.read_item(cx, pending, item);
    }
    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        response: Response<'_>,
    ) {
        let rejected = serde_json::from_str::<Envelope<'_>>(response.body)
            .ok()
            .and_then(|e| e.code)
            .and_then(|c| c.number())
            .is_some_and(|c| c != 0);
        if rejected || !(200..300).contains(&response.status) {
            self.response(cx, pending, response);
        } else {
            self.failed(
                cx,
                pending,
                trading::unknown("ApeX Omni response did not contain the expected list"),
            );
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.transport_failed(cx, pending, error);
    }
    fn on_http_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        error: safe_cex_engine::VenueError,
    ) {
        match pending {
            Pending::Candles(pending) => self.candle_transport_failed(cx, pending, error),
            Pending::Trade(request) => self.trading_failed(cx, request, error),
            pending => self.transport_failed(cx, pending, &error.message),
        }
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        _reason: safe_cex_engine::request_context::Error,
        error: safe_cex_engine::VenueError,
    ) {
        match pending {
            Pending::Candles(pending) => Self::candle_failed(cx, pending, error),
            Pending::Trade(request) => self.trading_failed(cx, request, error),
            pending => self.on_sign_error(cx, pending, error),
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if !self.public_timer(cx, timer) && !self.account_timer(cx, timer) {
            self.history_timer(cx, timer);
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if let Err(command) = self.public_command(cx, command)
            && let Err(command) = self.account_command(cx, command)
            && let Err(command) = self.trading_command(cx, command)
        {
            let _ = self.history_command(cx, command);
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.public.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.public.books()
    }
}
