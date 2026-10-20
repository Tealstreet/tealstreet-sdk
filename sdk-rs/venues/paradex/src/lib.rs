//! Paradex perpetuals on the sans-I/O engine.
mod account;
mod history;
mod public;
mod sbe;
pub mod sign;
mod trading;
mod wire;

use safe_cex_engine::{Command, Cx, Response, SocketId, TimerId, Venue, book::Book};

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub testnet: bool,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub paradex_address: Option<String>,
    pub subkey_address: Option<String>,
}

impl Options {
    fn private_enabled(&self) -> bool {
        !self.public_data_only
            && self.key.as_deref().is_some_and(|s| !s.is_empty())
            && self
                .paradex_address
                .as_deref()
                .is_some_and(|s| !s.is_empty())
            && self
                .subkey_address
                .as_deref()
                .is_some_and(|s| !s.is_empty())
    }
}

pub enum Pending {
    Public(public::Request),
    Account(account::Request),
    Trade(trading::Request),
    History(history::Request),
}

pub struct Paradex {
    options: Options,
    public: public::Public,
    account: account::Account,
    trading: trading::Trading,
    history: history::History,
}

impl Paradex {
    pub fn new(options: Options) -> Self {
        Self {
            account: account::Account::new(options.clone()),
            trading: trading::Trading::default(),
            history: history::History,
            options,
            public: public::Public::default(),
        }
    }
    fn rest(&self) -> &'static str {
        if self.options.testnet {
            "https://api.testnet.paradex.trade/v1"
        } else {
            "https://api.prod.paradex.trade/v1"
        }
    }
    fn ws(&self) -> &'static str {
        if self.options.testnet {
            "wss://ws.api.testnet.paradex.trade/v1?cancel-on-disconnect=false&sbeSchemaId=1&sbeSchemaVersion=2"
        } else {
            "wss://ws.api.prod.paradex.trade/v1?cancel-on-disconnect=false&sbeSchemaId=1&sbeSchemaVersion=2"
        }
    }
}

impl Venue for Paradex {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.public_start(cx);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if self.account.handles_socket(socket) {
            self.account.on_open(cx, socket);
        } else {
            self.public_open(cx, socket);
        }
        self.failures(cx);
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if self.account.handles_socket(socket) {
            self.account.on_message(cx, socket, text);
        } else {
            self.public_message(cx, socket, text);
        }
        self.failures(cx);
    }
    fn on_binary(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, data: &[u8]) {
        if self.account.handles_socket(socket) {
            self.account.on_binary(cx, socket, data);
        } else {
            self.public_binary(cx, socket, data);
        }
        self.failures(cx);
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _code: u16) {
        if self.account.handles_socket(socket) {
            self.account.on_close(cx, socket);
        } else {
            self.public_close(cx, socket);
        }
        self.failures(cx);
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match pending {
            Pending::Public(request) => self.public_item(cx, request, item),
            Pending::Account(request) => self.account.on_item(cx, request, item),
            Pending::History(request) => self.history.on_item(cx, request, item),
            Pending::Trade(_) => {}
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        match pending {
            Pending::Public(request) => {
                self.public_response(cx, request, response);
                if cx.store.loaded().tickers {
                    self.account.start(cx);
                }
            }
            Pending::Account(request) => self.account.on_response(cx, request, response),
            Pending::Trade(request) => {
                self.trading
                    .on_response(cx, &mut self.account, request, response)
            }
            Pending::History(request) => {
                self.history
                    .on_response(cx, &mut self.account, request, response)
            }
        }
        self.failures(cx);
    }
    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        response: Response<'_>,
    ) {
        let error = success(response)
            .err()
            .unwrap_or_else(|| format!("Malformed Paradex list (HTTP {})", response.status));
        self.on_request_failed(cx, pending, &error);
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.dispatch_failed(cx, pending, error);
        self.failures(cx);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.account.handles_timer(timer) {
            self.account.on_timer(cx, timer);
        } else {
            self.public_timer(cx, timer);
        }
        self.failures(cx);
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if let Some(command) = self.public_command(cx, command) {
            match command {
                Command::Query { command, query } => self.history.query(
                    cx,
                    &mut self.account,
                    command,
                    query,
                    !self.options.private_enabled(),
                ),
                other => {
                    self.trading
                        .command(cx, &mut self.account, other, &self.public.price_bands);
                }
            }
        }
        self.failures(cx);
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.public.books.get(symbol).map(|s| &s.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(self.public.books.iter().map(|(s, b)| (s.as_str(), &b.book)))
    }
}

impl Paradex {
    fn dispatch_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::Public(request) => self.public_failed(cx, request, error),
            Pending::Account(request) => self.account.on_failed(cx, request, error),
            Pending::Trade(request) => {
                self.trading
                    .on_failed(cx, &mut self.account, request, error)
            }
            Pending::History(request) => self.history.on_failed(cx, request, error),
        }
    }
    fn failures(&mut self, cx: &mut Cx<'_, Pending>) {
        loop {
            let failed = self.account.drain_failed();
            if failed.is_empty() {
                break;
            }
            for (pending, error) in failed {
                self.dispatch_failed(cx, pending, &error);
            }
        }
    }
}
fn success(response: Response<'_>) -> Result<(), String> {
    #[derive(serde::Deserialize)]
    struct Envelope<'a> {
        #[serde(borrow)]
        error: Option<&'a serde_json::value::RawValue>,
    }
    if !(200..300).contains(&response.status) {
        return Err(format!("Paradex HTTP {}", response.status));
    }
    if let Ok(envelope) = serde_json::from_str::<Envelope<'_>>(response.body)
        && let Some(error) = envelope.error
        && !matches!(error.get(), "null" | "\"\"")
    {
        return Err("Paradex request rejected".into());
    }
    Ok(())
}
