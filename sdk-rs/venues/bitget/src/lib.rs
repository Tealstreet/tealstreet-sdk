//! Bitget perpetual futures: a deterministic, sans-I/O venue engine.
mod account;
mod book;
mod history;
mod paths;
mod sign;
mod trading;
mod wire;

use book::BookState;
use safe_cex_engine::book::Book;
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::*;
use safe_cex_engine::socket::{Config, Supervisor, Tick};
use safe_cex_engine::{
    Command, CommandId, Cx, LogLevel, Method, Response, SocketId, TimerId, Venue, num,
};
use serde::{Serialize, de::IgnoredAny};
use std::collections::HashMap;
use wire::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Product {
    #[default]
    Usdt,
    Usdc,
    Inverse,
}

#[derive(Default)]
pub struct Options {
    pub read_credentials: Option<safe_cex_engine::ReadCredentials>,
    pub testnet: bool,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
    pub passphrase: Option<String>,
    pub product: Product,
}

pub enum Pending {
    History(Box<history::Request>),
    Trade(Box<trading::Request>),
    Account {
        generation: u64,
        request: account::Request,
    },
    Markets {
        rows: Vec<Market>,
        invalid: bool,
    },
    Tickers {
        rows: Vec<Ticker>,
        invalid: bool,
    },
    Candles {
        command: CommandId,
        rows: Vec<Candle>,
        invalid: bool,
    },
}

// The private connection (index 3) is supervised by `account.supervisor`.
struct Connection {
    socket: Option<SocketId>,
    open: bool,
    ping_at: f64,
    supervisor: Supervisor,
}

impl Default for Connection {
    fn default() -> Self {
        Self {
            socket: None,
            open: false,
            ping_at: 0.0,
            supervisor: Supervisor::new(Config {
                connect_timeout_ms: 20_000.0,
                idle_timeout_ms: 20_000.0,
                ping_interval_ms: Some(5_000.0),
                ..Config::default()
            }),
        }
    }
}

pub struct BitgetFutures {
    options: Options,
    account: account::Account,
    trading: trading::Trading,
    history: history::History,
    sockets: [Connection; 4],
    ticker_index: Vec<Option<u32>>,
    books: HashMap<String, BookState>,
    candles: HashMap<(String, String), usize>,
}

impl BitgetFutures {
    pub fn new(options: Options) -> Self {
        Self {
            options,
            account: account::Account::new(),
            trading: trading::Trading::default(),
            history: history::History::default(),
            sockets: Default::default(),
            ticker_index: Vec::new(),
            books: HashMap::new(),
            candles: HashMap::new(),
        }
    }

    fn product(&self) -> &'static str {
        match (self.options.product, self.options.testnet) {
            (Product::Usdt, false) => "usdt-futures",
            (Product::Usdt, true) => "susdt-futures",
            (Product::Usdc, false) => "usdc-futures",
            (Product::Usdc, true) => "susdc-futures",
            (Product::Inverse, false) => "coin-futures",
            (Product::Inverse, true) => "scoin-futures",
        }
    }

    fn coin(&self) -> &'static str {
        match (self.options.product, self.options.testnet) {
            (Product::Usdt, false) => "USDT",
            (Product::Usdt, true) => "SUSDT",
            (Product::Usdc, false) => "USDC",
            (Product::Usdc, true) => "SUSDC",
            (Product::Inverse, _) => "BTC",
        }
    }

    fn inverse(&self) -> bool {
        self.options.product == Product::Inverse
    }

    fn quote(&self) -> &'static str {
        if self.inverse() { "USD" } else { self.coin() }
    }

    pub(crate) fn margin_coin<'a>(
        &self,
        cx: &'a Cx<'_, Pending>,
        symbol: &str,
    ) -> Result<&'a str, String> {
        if self.inverse() {
            cx.store
                .market(symbol)
                .map(|m| m.base.as_str())
                .ok_or_else(|| format!("unknown Bitget market {symbol}"))
        } else {
            Ok(self.coin())
        }
    }

    fn get(&self, cx: &mut Cx<'_, Pending>, path: String, pending: Pending) {
        cx.request_streamed(
            Method::Get,
            format!("{}{path}", paths::REST),
            vec![("X-CHANNEL-API-CODE", "Tealstreet".into())],
            ArrayAt::Envelope(&[&["data"]]),
            pending,
        );
    }

    fn connect(&mut self, cx: &mut Cx<'_, Pending>, index: usize) {
        if index == 3 && self.account.supervisor.is_waiting() {
            return;
        }
        let url = if index == 3 {
            if self.account.uta {
                paths::UTA_PRIVATE
            } else {
                paths::PRIVATE
            }
        } else {
            paths::PUBLIC
        };
        let recover =
            index == 3 && std::mem::take(&mut self.account.needs_recovery) && !self.account.uta;
        let connection = &mut self.sockets[index];
        if let Some(socket) = connection.socket.take() {
            cx.close(socket);
        }
        connection.socket = Some(cx.connect_feed(
            url,
            if index == 3 {
                safe_cex_engine::SocketChannel::Private
            } else {
                safe_cex_engine::SocketChannel::Public
            },
            ["primary", "secondary-1", "secondary-2", "private"][index],
            recover,
        ));
        connection.open = false;
        if index == 3 {
            self.account.authenticated = false;
            self.account.supervisor.connecting(cx);
        } else {
            connection.supervisor.connecting(cx);
        }
    }

    fn channel(&self, socket: SocketId) -> Option<usize> {
        self.sockets.iter().position(|c| c.socket == Some(socket))
    }

    fn subscribe(&self, cx: &mut Cx<'_, Pending>, op: &str, topics: &[(String, String)]) {
        if self.sockets[0].open
            && let Some(socket) = self.sockets[0].socket
        {
            self.send_topics(cx, socket, op, topics);
        }
    }

    fn send_topics(
        &self,
        cx: &mut Cx<'_, Pending>,
        socket: SocketId,
        op: &str,
        topics: &[(String, String)],
    ) {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Topic<'a> {
            inst_type: &'a str,
            channel: &'a str,
            inst_id: &'a str,
        }
        #[derive(Serialize)]
        struct Subscription<'a> {
            op: &'a str,
            args: Vec<Topic<'a>>,
        }
        let product = self.product().to_uppercase();
        for chunk in topics.chunks(100) {
            let args = chunk
                .iter()
                .map(|(channel, symbol)| Topic {
                    inst_type: &product,
                    channel,
                    inst_id: symbol,
                })
                .collect();
            cx.send(
                socket,
                serde_json::to_string(&Subscription { op, args }).expect("subscription"),
            );
        }
    }

    fn disconnected(&mut self, cx: &mut Cx<'_, Pending>, index: usize) {
        let connection = &mut self.sockets[index];
        connection.open = false;
        connection.socket = None;
        if index == 3 {
            self.account.authenticated = false;
            self.account.supervisor.retry(cx);
            return;
        }
        connection.supervisor.retry(cx);
        if index == 0 {
            for state in self.books.values_mut() {
                state.reset();
            }
        }
    }

    fn public_message(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        let message = match serde_json::from_str::<PublicMessage<'_>>(text) {
            Ok(message) => message,
            Err(_) => {
                if let Ok(header) = serde_json::from_str::<PublicHeader<'_>>(text)
                    && header.arg.channel == "books"
                {
                    let symbol = header.arg.inst_id.as_ref();
                    if let Some(state) = self.books.get_mut(symbol) {
                        state.reset();
                        let topics = [("books".into(), symbol.to_owned())];
                        self.subscribe(cx, "unsubscribe", &topics);
                        self.subscribe(cx, "subscribe", &topics);
                    }
                }
                return;
            }
        };
        if message.event == "error" {
            cx.log(LogLevel::Error, format!("Bitget WS: {}", message.msg));
            return;
        }
        match message.data {
            PublicData::Tickers(rows) => {
                for row in rows {
                    let symbol = if row.inst_id.is_empty() {
                        &row.symbol
                    } else {
                        &row.inst_id
                    };
                    let Some(market) = cx.store.market_index(symbol) else {
                        continue;
                    };
                    let Some(index) = self.ticker_index.get(market as usize).copied().flatten()
                    else {
                        continue;
                    };
                    cx.store
                        .update_ticker_at(index, |ticker| update_ticker(ticker, &row));
                }
            }
            PublicData::Books(rows) => {
                let symbol = message.arg.inst_id.as_ref();
                let Some(state) = self.books.get_mut(symbol) else {
                    return;
                };
                for row in rows {
                    match state.apply(&message.action, &row) {
                        Ok(true) => cx.book_changed(symbol),
                        Ok(false) => {}
                        Err(error) => {
                            state.reset();
                            cx.log(LogLevel::Warning, error);
                            let topics = [("books".into(), symbol.to_owned())];
                            self.subscribe(cx, "unsubscribe", &topics);
                            self.subscribe(cx, "subscribe", &topics);
                            break;
                        }
                    }
                }
            }
            PublicData::Candles(rows) => {
                let symbol = message.arg.inst_id.as_ref();
                let native = message.arg.channel.strip_prefix("candle").unwrap_or("");
                if message.action == "update"
                    && let Some((_, value)) = self
                        .candles
                        .keys()
                        .find(|(s, i)| s == symbol && interval(i) == Some(native))
                {
                    for row in rows {
                        if row.iter().all(|n| n.0.is_some()) {
                            cx.candle(symbol, value, candle(&row));
                        } else {
                            cx.log(LogLevel::Warning, "invalid Bitget candle");
                        }
                    }
                }
            }
            PublicData::None => {}
        }
    }
}

fn ticker_index(store: &safe_cex_engine::Store) -> Vec<Option<u32>> {
    let mut indexes = vec![None; store.markets().len()];
    // Store's symbol lookup selects the last duplicate ticker row.
    for (index, ticker) in store.tickers().iter().enumerate() {
        if let Some(market) = store.market_index(&ticker.symbol) {
            indexes[market as usize] = Some(index as u32);
        }
    }
    indexes
}

fn update_ticker(t: &mut Ticker, row: &TickerRow<'_>) {
    for (target, value) in [
        (&mut t.bid, row.value(TickerField::BidPr)),
        (&mut t.ask, row.value(TickerField::AskPr)),
        (&mut t.last, row.value(TickerField::LastPr)),
        (&mut t.mark, row.value(TickerField::MarkPrice)),
        (&mut t.index, row.value(TickerField::IndexPrice)),
        (&mut t.funding_rate, row.value(TickerField::FundingRate)),
        (&mut t.funding_time, row.value(TickerField::NextFundingTime)),
        (&mut t.volume, row.value(TickerField::BaseVolume)),
        (&mut t.quote_volume, row.value(TickerField::QuoteVolume)),
    ] {
        if let Some(value) = value {
            *target = value;
        }
    }
    if let Some(change) = row
        .value(TickerField::Change24h)
        .or(row.value(TickerField::ChangeUtc24h))
    {
        t.percentage = change * 100.0;
    }
}

fn market(row: &Contract<'_>) -> Market {
    Market {
        id: row.symbol.to_string(),
        symbol: row.symbol.to_string(),
        pretty_symbol: None,
        base: row.base_coin.to_string(),
        quote: row.quote_coin.to_string(),
        active: matches!(
            row.symbol_status.as_ref(),
            "normal" | "listed" | "limit_open"
        ),
        precision: Precision {
            amount: row.size_multiplier.get(),
            price: if row.price_end_step.get().fract() == 0.0
                && row.price_end_step.get() <= 9_007_199_254_740_991.0
            {
                row.price_end_step.get() / 10f64.powi(row.price_place.get() as i32)
            } else {
                format!("{}e-{}", row.price_end_step.get(), row.price_place.get())
                    .parse::<f64>()
                    .unwrap_or(0.0)
            },
        },
        limits: Limits {
            amount: MinMax {
                min: row.min_trade_num.get(),
                max: [
                    row.max_order_qty,
                    row.max_market_order_qty,
                    row.max_trade_num,
                ]
                .iter()
                .find_map(|v| v.0.filter(|v| *v > 0.0))
                .unwrap_or(f64::INFINITY),
            },
            leverage: MinMax {
                min: row.min_lever.0.filter(|n| *n != 0.0).unwrap_or(1.0),
                max: row.max_lever.0.filter(|n| *n != 0.0).unwrap_or(20.0),
            },
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

fn interval(value: &str) -> Option<&'static str> {
    Some(match value {
        "1m" => "1m",
        "3m" => "3m",
        "5m" => "5m",
        "15m" => "15m",
        "30m" => "30m",
        "1h" => "1H",
        "2h" => "2H",
        "4h" => "4H",
        "6h" => "6Hutc",
        "12h" => "12Hutc",
        "1d" => "1Dutc",
        "1w" => "1Wutc",
        _ => return None,
    })
}

fn candle(row: &CandleRow) -> Candle {
    Candle {
        timestamp: row[0].get() / 1000.0,
        open: row[1].get(),
        high: row[2].get(),
        low: row[3].get(),
        close: row[4].get(),
        volume: row[6].get(),
    }
}

impl Venue for BitgetFutures {
    fn guard_pending_placements(&self) -> bool {
        true
    }
    fn guard_order_mutations(&self) -> bool {
        true
    }

    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(read) = &self.options.read_credentials
            && let Err(error) = read
                .validate()
                .and_then(|_| read.validate_passphrase(self.options.passphrase.as_deref()))
        {
            cx.log(LogLevel::Error, error);
            return;
        }
        self.get(
            cx,
            format!("{}?productType={}", paths::CONTRACTS, self.product()),
            Pending::Markets {
                rows: vec![],
                invalid: false,
            },
        );
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        let Some(index) = self.channel(socket) else {
            return;
        };
        let connection = &mut self.sockets[index];
        connection.open = true;
        connection.ping_at = cx.now;
        if index == 3 {
            self.account.supervisor.opened(cx);
            self.private_open(cx, socket);
            cx.send(socket, "ping");
            return;
        }
        connection.supervisor.opened(cx);
        let mut topics: Vec<_> = cx
            .store
            .markets()
            .iter()
            .enumerate()
            .filter(|(i, _)| i % 3 == index)
            .map(|(_, m)| ("ticker".to_owned(), m.symbol.clone()))
            .collect();
        if index == 0 {
            for (symbol, state) in &mut self.books {
                state.reset();
                topics.push(("books".to_owned(), symbol.clone()));
            }
            topics.extend(
                self.candles
                    .keys()
                    .filter_map(|(s, i)| Some((format!("candle{}", interval(i)?), s.clone()))),
            );
        }
        self.send_topics(cx, socket, "subscribe", &topics);
        cx.send(socket, "ping");
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        let Some(index) = self.channel(socket) else {
            return;
        };
        if index == 3 {
            self.account.supervisor.received(cx.now);
        } else {
            self.sockets[index].supervisor.received(cx.now);
        }
        if text == "pong" {
            cx.store
                .set_latency((cx.now - self.sockets[index].ping_at) / 2.0);
            return;
        }
        if index == 3 {
            self.private_message(cx, text);
        } else {
            self.public_message(cx, text);
        }
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _: u16) {
        if let Some(index) = self.channel(socket) {
            self.disconnected(cx, index);
        }
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match pending {
            Pending::History(request) => self.query_item(cx, request, item),
            Pending::Trade(_) => {}
            Pending::Account {
                generation,
                request,
            } => {
                if *generation == self.account.generation {
                    self.account_item(cx, request, item);
                }
            }
            Pending::Markets { rows, invalid } => match serde_json::from_str::<Contract>(item) {
                Ok(row) if row.quote_coin == self.quote() => {
                    if row.symbol.is_empty()
                        || row.base_coin.is_empty()
                        || row.price_place.0.is_none()
                        || row.price_place.get().fract() != 0.0
                        || !(0.0..=18.0).contains(&row.price_place.get())
                        || row.price_end_step.get() <= 0.0
                        || row.size_multiplier.get() <= 0.0
                    {
                        *invalid = true;
                    } else {
                        let mut mapped = market(&row);
                        if self.inverse() {
                            if row.min_lever.get() == 0.0 {
                                mapped.limits.leverage.min = 1.0;
                            }
                            mapped.is_inverse = Some(true);
                            mapped.limits.amount.max = row
                                .max_trade_num
                                .0
                                .filter(|n| *n > 0.0)
                                .unwrap_or(f64::INFINITY);
                            if row.max_lever.get() == 0.0 {
                                mapped.limits.leverage.max = 20.0;
                            }
                        }
                        if !mapped.precision.price.is_finite() || mapped.precision.price <= 0.0 {
                            *invalid = true;
                        } else {
                            rows.push(mapped);
                        }
                    }
                }
                Ok(_) => {}
                Err(_) => *invalid = true,
            },
            Pending::Tickers { rows, invalid } => match serde_json::from_str::<TickerRow>(item) {
                Ok(row) => {
                    if let Some(m) = cx.store.market_by_id(&row.symbol) {
                        let mut ticker = Ticker {
                            id: m.id.clone(),
                            symbol: m.symbol.clone(),
                            mark: row.value(TickerField::LastPr).unwrap_or(0.0),
                            ..Ticker::default()
                        };
                        update_ticker(&mut ticker, &row);
                        ticker.mark = row.value(TickerField::LastPr).unwrap_or(0.0);
                        // The REST seed uses the UTC change; the live stream uses change24h.
                        ticker.percentage =
                            row.value(TickerField::ChangeUtc24h).unwrap_or(0.0) * 100.0;
                        ticker.funding_time = 0.0;
                        rows.push(ticker);
                    }
                }
                Err(_) => *invalid = true,
            },
            Pending::Candles { rows, invalid, .. } => match serde_json::from_str::<CandleRow>(item)
            {
                Ok(row) if row.iter().all(|n| n.0.is_some()) => rows.push(candle(&row)),
                Ok(_) => *invalid = true,
                Err(_) => *invalid = true,
            },
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        if let Pending::History(request) = pending {
            self.query_response(cx, *request, response);
            return;
        }
        if let Pending::Trade(request) = pending {
            self.trade_response(cx, *request, Ok(response));
            return;
        }
        if let Pending::Account {
            generation,
            request,
        } = pending
        {
            if generation == self.account.generation {
                self.account_response(cx, request, response);
                self.resume_trading(cx);
            }
            return;
        }
        let valid = serde_json::from_str::<Envelope<IgnoredAny>>(response.body)
            .map_err(|e| e.to_string())
            .and_then(Envelope::checked);
        if let Err(error) = valid {
            self.on_request_failed(cx, pending, &error);
            return;
        }
        if response.status != 200 {
            self.on_request_failed(
                cx,
                pending,
                &format!("Bitget HTTP {}: {}", response.status, response.body),
            );
            return;
        }
        match pending {
            Pending::Markets {
                rows,
                invalid: false,
            } => {
                cx.store.set_markets(rows);
                self.ticker_index = ticker_index(cx.store);
                self.connect(cx, 0);
                self.get(
                    cx,
                    format!("{}?productType={}", paths::TICKERS, self.product()),
                    Pending::Tickers {
                        rows: vec![],
                        invalid: false,
                    },
                );
            }
            Pending::Tickers {
                rows,
                invalid: false,
            } => {
                cx.store.set_tickers(rows);
                self.ticker_index = ticker_index(cx.store);
                self.connect(cx, 1);
                self.connect(cx, 2);
                self.start_private(cx);
            }
            Pending::Candles {
                command,
                rows,
                invalid: false,
            } => cx.done(command, Ok(serde_json::to_string(&rows).expect("candles"))),
            pending => self.on_request_failed(cx, pending, "invalid Bitget REST record"),
        }
    }
    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        response: Response<'_>,
    ) {
        if serde_json::from_str::<Envelope<IgnoredAny>>(response.body)
            .ok()
            .is_some_and(|e| e.code.0.is_some_and(|code| code != 0.0))
        {
            self.on_response(cx, pending, response);
        } else {
            self.on_request_failed(
                cx,
                pending,
                "Bitget successful response has no expected list",
            );
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::History(request) => self.query_failed(cx, *request, error),
            Pending::Trade(request) => self.trade_response(cx, *request, Err(error)),
            Pending::Candles { command, .. } => cx.done(command, Err(error.to_owned())),
            Pending::Account {
                generation,
                request,
            } => {
                if generation == self.account.generation {
                    self.account_transport_failed(cx, request, error);
                    self.resume_trading(cx);
                }
            }
            _ => cx.log(LogLevel::Error, error),
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.account_timer(cx, timer);
        self.trade_timer(cx, timer);
        self.history_timer(cx, timer);
        for index in 0..self.sockets.len() {
            let tick = if index == 3 {
                self.account.supervisor.on_timer(cx, timer)
            } else {
                self.sockets[index].supervisor.on_timer(cx, timer)
            };
            match tick {
                Tick::Unhandled => continue,
                Tick::Reconnect => self.connect(cx, index),
                Tick::Ping => {
                    if let Some(socket) = self.sockets[index].socket {
                        cx.send(socket, "ping");
                        self.sockets[index].ping_at = cx.now;
                    }
                }
                Tick::TimedOut(_) => {
                    if let Some(socket) = self.sockets[index].socket {
                        cx.close(socket);
                    }
                    self.disconnected(cx, index);
                }
                Tick::Handled => (),
            }
            return;
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::SubscribeOrderBook { symbol } => {
                let state = self.books.entry(symbol.clone()).or_default();
                state.subscribers += 1;
                if state.subscribers == 1 {
                    self.subscribe(cx, "subscribe", &[("books".to_owned(), symbol)]);
                }
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(state) = self.books.get_mut(&symbol) {
                    state.subscribers -= 1;
                    if state.subscribers == 0 {
                        self.books.remove(&symbol);
                        self.subscribe(cx, "unsubscribe", &[("books".to_owned(), symbol)]);
                    }
                }
            }
            Command::SubscribeCandles {
                symbol,
                interval: value,
            } => {
                let Some(native) = interval(&value) else {
                    return;
                };
                let count = self.candles.entry((symbol.clone(), value)).or_default();
                *count += 1;
                if *count == 1 {
                    self.subscribe(cx, "subscribe", &[(format!("candle{native}"), symbol)]);
                }
            }
            Command::UnsubscribeCandles {
                symbol,
                interval: value,
            } => {
                let key = (symbol, value);
                if let Some(count) = self.candles.get_mut(&key) {
                    *count -= 1;
                    if *count == 0 {
                        self.candles.remove(&key);
                        if let Some(native) = interval(&key.1) {
                            self.subscribe(
                                cx,
                                "unsubscribe",
                                &[(format!("candle{native}"), key.0)],
                            );
                        }
                    }
                }
            }
            Command::FetchCandles {
                command,
                symbol,
                interval: value,
                limit,
                from_ms,
                to_ms,
            } => {
                let Some(native) = interval(&value) else {
                    cx.done(command, Err("unsupported Bitget interval".to_owned()));
                    return;
                };
                let limit = limit.unwrap_or(500).clamp(1, 1000);
                let mut end = to_ms.unwrap_or(cx.now);
                if let Some(start) = from_ms {
                    let seconds = match value.as_str() {
                        "1w" => 604800.0,
                        "1d" => 86400.0,
                        _ => {
                            value[..value.len() - 1].parse::<f64>().unwrap_or(1.0)
                                * if value.ends_with('h') { 3600.0 } else { 60.0 }
                        }
                    };
                    end = end.min(
                        start + (seconds * 1000.0 * f64::from(limit)).min(90.0 * 86_400_000.0),
                    );
                }
                let mut path = format!(
                    "{}?symbol={symbol}&granularity={native}&endTime={}&productType={}&limit={limit}",
                    paths::CANDLES,
                    num::format(end),
                    self.product()
                );
                if let Some(start) = from_ms.filter(|v| *v < end) {
                    path.push_str(&format!("&startTime={}", num::format(start)));
                }
                self.get(
                    cx,
                    path,
                    Pending::Candles {
                        command,
                        rows: vec![],
                        invalid: false,
                    },
                );
            }
            Command::Query { command, query } => self.start_query(cx, command, query),
            trade => self.trade_command(cx, trade),
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books.get(symbol).filter(|b| b.synced).map(|b| &b.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(self.books.iter().map(|(s, b)| (s.as_str(), &b.book)))
    }
}

#[cfg(test)]
mod secondary_tests;
