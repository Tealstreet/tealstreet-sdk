//! Coinbase Advanced Trade: public INTX discovery and native account products.
mod public;
mod rest;
mod sign;
mod wire;

use safe_cex_engine::book::Book;
use safe_cex_engine::{Command, Cx, Response, SocketId, TimerId, Venue};
use serde::Deserialize;
use zeroize::Zeroize;

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Options {
    pub key: String,
    pub secret: String,
    pub public_data_only: bool,
    pub spot_only: bool,
    pub testnet: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductKind {
    Intx,
    Cfm,
    Spot,
}
pub enum Pending {
    Public(public::Request),
}
pub struct Coinbase {
    pub(crate) options: Options,
    pub(crate) public: public::Public,
    pub(crate) credentials: Option<sign::Credentials>,
    rest: rest::Rest,
}
impl Coinbase {
    pub fn new(options: Options) -> Result<Self, String> {
        let mut options = options;
        let credentials = if options.public_data_only || options.key.is_empty() {
            None
        } else {
            Some(sign::Credentials::new(&options.key, &options.secret)?)
        };
        options.secret.zeroize();
        options.key = credentials
            .as_ref()
            .map_or_else(String::new, |c| c.key().to_owned());
        Ok(Self {
            options,
            public: public::Public::default(),
            credentials,
            rest: rest::Rest::default(),
        })
    }
}
impl Venue for Coinbase {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.public_start(cx);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        self.public_open(cx, socket);
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        self.public_message(cx, socket, text);
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _code: u16) {
        self.public_close(cx, socket);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        let Pending::Public(pending) = pending;
        self.public_response(cx, pending, response);
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        let Pending::Public(pending) = pending;
        self.public_item(cx, pending, item);
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        let Pending::Public(pending) = pending;
        self.public_failed(cx, pending, error);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if !self.rest_timer(cx, timer) {
            self.public_timer(cx, timer);
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if let Some(
            Command::PlaceOrders { command, .. }
            | Command::CancelOrders { command, .. }
            | Command::AmendOrder { command, .. }
            | Command::CancelSymbolOrders { command, .. }
            | Command::SetLeverage { command, .. }
            | Command::SetMarginType { command, .. }
            | Command::SetPositionMode { command, .. }
            | Command::FetchLeverage { command, .. }
            | Command::Query { command, .. },
        ) = self.public_command(cx, command)
        {
            cx.done(command, Err("Coinbase account is not ready".into()));
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
        Box::new(
            self.public
                .books
                .iter()
                .filter(|(_, b)| b.live)
                .map(|(s, b)| (s.as_str(), &b.book)),
        )
    }
}
