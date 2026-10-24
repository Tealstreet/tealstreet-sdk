//! Coinbase Advanced Trade: public INTX discovery and native account products.
mod account;
mod account_wire;
mod history;
mod local_context;
mod public;
mod rest;
pub mod sign;
mod trading;
mod wire;

use safe_cex_engine::book::Book;
use safe_cex_engine::{Command, Cx, Response, SocketId, TimerId, Venue};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Options {
    pub key: String,
    pub secret: String,
    pub public_data_only: bool,
    pub spot_only: bool,
    pub testnet: bool,
    /// A saved read-only key signs REST reads; writes and the user stream keep the primary key.
    pub read_credentials: Option<safe_cex_engine::ReadCredentials>,
}
safe_cex_crypto::redacted_debug!(Options);
impl safe_cex_crypto::secrets::EraseSecrets for Options {
    fn erase_secrets(&mut self) {
        safe_cex_crypto::secrets::erase(&mut self.key);
        safe_cex_crypto::secrets::erase(&mut self.secret);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductKind {
    Intx,
    Cfm,
    Spot,
}
pub enum Pending {
    Public(public::Request),
    /// The flag records whether the read-only key signed the request.
    Account(account::Request, bool),
    Trading(trading::Request),
    History(history::Request),
}
pub struct Coinbase {
    pub(crate) options: safe_cex_crypto::secrets::Sensitive<Options>,
    pub(crate) public: public::Public,
    pub(crate) credentials: Option<sign::Credentials>,
    pub(crate) read_credentials: Option<sign::Credentials>,
    /// The host routes GETs through the read key's egress unless told otherwise.
    pub(crate) read_supplied: bool,
    rest: rest::Rest,
    pub(crate) account: account::Account,
    pub(crate) trading: trading::Trading,
    pub(crate) history: history::History,
}
impl Coinbase {
    pub fn new(options: Options) -> Result<Self, String> {
        let mut options = safe_cex_crypto::secrets::Sensitive::new(options);
        let raw_key = Zeroizing::new(std::mem::take(&mut options.key));
        let secret = Zeroizing::new(std::mem::take(&mut options.secret));
        let credentials = if options.public_data_only || raw_key.is_empty() {
            None
        } else {
            Some(sign::Credentials::new(&raw_key, &secret)?)
        };
        let read = options.read_credentials.take();
        let read_credentials = match (&credentials, read) {
            (Some(_), Some(read)) => {
                read.validate()?;
                let key = Zeroizing::new(read.key);
                let secret = Zeroizing::new(read.secret);
                Some(sign::Credentials::new(&key, &secret)?)
            }
            _ => None,
        };
        options.key = credentials
            .as_ref()
            .map_or_else(String::new, |c| c.key().to_owned());
        Ok(Self {
            options,
            public: public::Public::new(),
            credentials,
            read_supplied: read_credentials.is_some(),
            read_credentials,
            rest: rest::Rest::default(),
            account: account::Account::new(),
            trading: trading::Trading::default(),
            history: history::History::default(),
        })
    }
}
impl Coinbase {
    /// The private portfolio discovered at startup is the account's identity.
    fn account_identity(&self) -> Result<String, String> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Identity<'a> {
            user_id: &'a str,
        }
        match self.account.portfolio_id.as_deref() {
            Some(id) if !self.options.public_data_only => {
                serde_json::to_string(&Identity { user_id: id }).map_err(|error| error.to_string())
            }
            _ => Err("Coinbase private portfolio ID is unavailable".into()),
        }
    }
}

impl Venue for Coinbase {
    fn guard_order_mutations(&self) -> bool {
        true
    }

    type Pending = Pending;
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            fill_history_time: safe_cex_engine::FillHistoryTime::SecondsOrMilliseconds,
            finite_command_numbers: true,
            finite_history_bounds: true,
            reject_order_history_filters: true,
            quantity_units: safe_cex_engine::QuantityUnits::NativePlacement,
            income_history: false,
            account: safe_cex_engine::AccountLookup::AfterLoad,
            leverage_query: safe_cex_engine::LeverageQuery::OnRead,
            ..safe_cex_engine::Capabilities::BASE
        }
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.credentials.is_some() {
            self.private_start(cx);
        } else {
            self.public_start(cx);
        }
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if !self.public_open(cx, socket) {
            self.private_on_open(cx, socket);
        }
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if !self.public_message(cx, socket, text) {
            self.private_on_message(cx, socket, text);
        }
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _code: u16) {
        if !self.public_close(cx, socket) {
            self.private_on_close(cx, socket);
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        match pending {
            Pending::Public(p) => self.public_response(cx, p, response),
            Pending::Account(p, read) => self.private_on_response(cx, p, read, response),
            Pending::Trading(p) => self.trading_response(cx, p, response),
            Pending::History(p) => self.history_response(cx, p, response),
        }
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match pending {
            Pending::Public(p) => self.public_item(cx, p, item),
            Pending::Account(p, _) => self.private_on_item(cx, p, item),
            Pending::Trading(_) => {}
            Pending::History(p) => self.history_item(cx, p, item),
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::Public(p) => self.public_failed(cx, p, error),
            Pending::Account(p, _) => self.private_failed(cx, p, error),
            Pending::Trading(p) => self.trading_failed(cx, p, error),
            Pending::History(p) => self.history_failed(cx, p, error),
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if !self.rest_timer(cx, timer) && !self.public_timer(cx, timer) {
            self.private_on_timer(cx, timer);
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if let Command::FetchAccount { .. } = command {
            local_context::complete(
                cx,
                safe_cex_engine::request_context::Operation::Read,
                self.account_identity()
                    .map_err(safe_cex_engine::VenueError::new),
            );
            return;
        }
        if let Some(Command::Query { command, query }) = self
            .public_command(cx, command)
            .and_then(|command| self.trading_command(cx, command))
        {
            self.history_start(cx, command, query);
        }
    }

    fn book(&self, symbol: &str) -> Option<&Book> {
        self.public
            .books
            .get(symbol)
            .filter(|b| b.live)
            .map(|b| &b.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(self.public.books.iter().map(|(s, b)| (s.as_str(), &b.book)))
    }
}
