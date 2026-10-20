//! ApeX Omni perpetuals, driven entirely by host events.
mod account;
mod history;
mod history_wire;
mod private_wire;
mod public;
mod public_wire;
pub mod sign;
mod trading;

use safe_cex_engine::book::Book;
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::{
    Command, Cx, LogLevel, Method, RequestId, Response, SocketId, TimerId, Venue,
};
use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub testnet: bool,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
    pub passphrase: Option<String>,
    pub omni_key_seed: Option<String>,
    pub tick_interval_ms: Option<f64>,
}

pub enum Pending {
    Public(public::Request),
    Account(account::Request),
    Trade(trading::Request),
    History(history::Request),
    SignedAuth {
        request: sign::Request,
        pending: Box<Pending>,
        legacy: bool,
        retries: u8,
        at: Option<ArrayAt>,
    },
}

pub struct ApexOmni {
    options: Options,
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

impl ApexOmni {
    pub fn new(options: Options) -> Self {
        Self {
            rest: if options.testnet {
                "https://testnet.omni.apex.exchange/api"
            } else {
                "https://omni.apex.exchange/api"
            },
            options,
            contracts: HashMap::new(),
            public: public::Public::default(),
            account: account::Account::new(),
            trading: trading::Trading::default(),
        }
    }

    fn signed(
        &self,
        cx: &mut Cx<'_, Pending>,
        request: sign::Request,
        pending: Pending,
        at: Option<ArrayAt>,
    ) -> Result<RequestId, String> {
        self.send_signed(cx, request, pending, at, false, 0)
    }

    fn send_signed(
        &self,
        cx: &mut Cx<'_, Pending>,
        request: sign::Request,
        pending: Pending,
        at: Option<ArrayAt>,
        legacy: bool,
        retries: u8,
    ) -> Result<RequestId, String> {
        let signed = sign::request(&self.options, cx.now, &request, legacy)?;
        let url = format!("{}{}", self.rest, signed.path);
        let stream = at;
        let method = request.method;
        let pending = Pending::SignedAuth {
            request,
            pending: Box::new(pending),
            legacy,
            retries,
            at,
        };
        Ok(if let Some(at) = stream {
            cx.request_streamed_body(method, url, signed.headers, signed.body, at, pending)
        } else {
            cx.request(method, url, signed.headers, signed.body, pending)
        })
    }

    fn response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        match pending {
            Pending::SignedAuth {
                request,
                mut pending,
                legacy,
                retries,
                at,
            } => {
                let envelope = serde_json::from_str::<Envelope<'_>>(response.body).ok();
                if envelope.is_none() && response.body.trim_start().starts_with('{') {
                    self.failed(
                        cx,
                        *pending,
                        "ApeX Omni response contains malformed error metadata",
                    );
                    return;
                }
                let code = envelope
                    .as_ref()
                    .and_then(|e| e.code.as_ref())
                    .and_then(Code::number);
                let rejected = envelope.as_ref().is_some_and(|e| {
                    code.is_some_and(|code| code != 0)
                        && (matches!(code, Some(20016 | 401))
                            || e.msg.to_ascii_lowercase().contains("sign"))
                });
                if !legacy && rejected {
                    Self::reset_request(&mut pending);
                    if let Err(error) = self.send_signed(cx, request, *pending, at, true, retries) {
                        cx.log(LogLevel::Error, &error);
                    }
                    return;
                }
                if !(200..300).contains(&response.status) || code.is_some_and(|code| code != 0) {
                    let error = format!(
                        "ApeX Omni request failed [{}]: {}",
                        code.unwrap_or(i64::from(response.status)),
                        envelope
                            .as_ref()
                            .map(|e| e.msg.as_ref())
                            .unwrap_or("invalid response")
                    );
                    self.failed(cx, *pending, &error);
                } else {
                    self.response(cx, *pending, response);
                }
            }
            Pending::Public(pending) => self.public_response(cx, pending, response),
            Pending::Account(pending) => self.account_response(cx, pending, response),
            Pending::Trade(pending) => self.trading_response(cx, pending, response),
            Pending::History(pending) => self.history_response(cx, pending, response),
        }
    }

    fn item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match pending {
            Pending::SignedAuth { pending, .. } => self.item(cx, pending, item),
            Pending::Public(pending) => self.public_item(cx, pending, item),
            Pending::Account(pending) => self.account_item(cx, pending, item),
            Pending::Trade(_) => {}
            Pending::History(pending) => self.history_item(cx, pending, item),
        }
    }

    fn reset_request(pending: &mut Pending) {
        match pending {
            Pending::SignedAuth { pending, .. } => Self::reset_request(pending),
            Pending::Public(request) => request.reset(),
            Pending::Account(request) => request.reset(),
            Pending::Trade(_) => {}
            Pending::History(pending) => pending.reset(),
        }
    }

    fn transport_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::SignedAuth {
                request,
                mut pending,
                legacy,
                retries,
                at,
            } if request.method == Method::Get && retries < 3 => {
                Self::reset_request(&mut pending);
                if let Err(error) = self.send_signed(cx, request, *pending, at, legacy, retries + 1)
                {
                    cx.log(LogLevel::Error, error);
                }
            }
            pending => self.failed(cx, pending, error),
        }
    }

    fn failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::SignedAuth { pending, .. } => self.failed(cx, *pending, error),
            Pending::Public(pending) => self.public_failed(cx, pending, error),
            Pending::Account(pending) => self.account_failed(cx, pending, error),
            Pending::Trade(pending) => self.trading_failed(cx, pending, error),
            Pending::History(pending) => self.history_failed(cx, pending, error),
        }
    }
}

impl Venue for ApexOmni {
    fn guard_order_mutations(&self) -> bool {
        true
    }

    type Pending = Pending;
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
        self.item(cx, pending, item);
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
                "ApeX Omni response did not contain the expected list",
            );
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.transport_failed(cx, pending, error);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if !self.public_timer(cx, timer)
            && !self.account_timer(cx, timer)
            && !self.trading_timer(cx, timer)
        {
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
