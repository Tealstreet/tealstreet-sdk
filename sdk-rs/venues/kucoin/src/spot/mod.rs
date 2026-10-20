//! KuCoin Spot HF: base-unit quantities and native order alteration.
mod trading;
mod wire;
use crate::wire::{Bullet, Number};
use crate::{Options, encoded_path, interval, sign, success};
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::*;
use safe_cex_engine::store::MarketSetting;
use safe_cex_engine::{
    Command, CommandId, Cx, Fill, LogLevel, Method, Query, Response, SocketChannel, SocketId,
    TimerId, Venue,
};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::collections::{BTreeMap, HashSet, VecDeque};
use wire::*;
const REST: &str = "https://api.kucoin.com";

pub enum Pending {
    Markets {
        rows: Vec<Market>,
        invalid: bool,
    },
    Tickers {
        rows: Vec<Ticker>,
        invalid: bool,
    },
    Bullet(usize),
    Balance(u64),
    Symbols {
        revision: u64,
    },
    Orders(ReadOrders),
    Write(trading::Job),
    Candles {
        command: CommandId,
        rows: Vec<Candle>,
        invalid: bool,
    },
    Fills(CommandId),
    Account(CommandId),
}
pub struct ReadOrders {
    revision: u64,
    symbols: VecDeque<String>,
    symbol: String,
    page: u32,
    rows: Vec<Order>,
    lookup: Option<(CommandId, String)>,
}
#[derive(Default)]
struct Socket {
    socket: Option<SocketId>,
    ready: bool,
    ping_interval: f64,
    ping_timeout: f64,
    ping: Option<TimerId>,
    pong: Option<TimerId>,
    reconnect: Option<TimerId>,
}
#[derive(Default)]
struct SpotBook {
    book: Book,
    subscribers: u32,
    last: f64,
}
pub struct KucoinSpot {
    options: safe_cex_crypto::secrets::Sensitive<Options>,
    credentials: Option<sign::Credentials>,
    sockets: [Socket; 2],
    books: BTreeMap<String, SpotBook>,
    candles: BTreeMap<(String, String), u32>,
    revision: u64,
    balance_revision: u64,
    orders_loading: bool,
    balance_loading: bool,
    poll: Option<TimerId>,
    heal_orders: Option<TimerId>,
    heal_balance: Option<TimerId>,
    next_id: u64,
    terminal: HashSet<String>,
    terminal_order: VecDeque<String>,
    fills: HashSet<String>,
    fill_order: VecDeque<String>,
}
impl KucoinSpot {
    pub fn new(options: Options) -> Self {
        let options = safe_cex_crypto::secrets::Sensitive::new(options);
        let credentials = (!options.public_data_only)
            .then(|| {
                options
                    .key
                    .as_ref()
                    .filter(|v| !v.is_empty())
                    .zip(options.secret.as_ref().filter(|v| !v.is_empty()))
                    .zip(options.passphrase.as_ref().filter(|v| !v.is_empty()))
                    .map(|((key, secret), passphrase)| sign::Credentials {
                        key: key.clone(),
                        secret: secret.clone(),
                        passphrase: passphrase.clone(),
                        broker: options.broker.clone(),
                    })
            })
            .flatten();
        Self {
            options,
            credentials,
            sockets: Default::default(),
            books: Default::default(),
            candles: Default::default(),
            revision: 0,
            balance_revision: 0,
            orders_loading: false,
            balance_loading: false,
            poll: None,
            heal_orders: None,
            heal_balance: None,
            next_id: 0,
            terminal: Default::default(),
            terminal_order: Default::default(),
            fills: Default::default(),
            fill_order: Default::default(),
        }
    }
    fn request(
        &self,
        cx: &mut Cx<'_, Pending>,
        method: Method,
        path: &str,
        body: Option<String>,
        private: bool,
        pending: Pending,
    ) {
        let headers = if private {
            let read_credentials = self
                .options
                .read_credentials
                .as_ref()
                .filter(|_| method == Method::Get && self.credentials.is_some())
                .map(|read| sign::Credentials {
                    key: read.key.clone(),
                    secret: read.secret.clone(),
                    passphrase: read
                        .passphrase
                        .clone()
                        .or_else(|| self.options.passphrase.clone())
                        .unwrap_or_default(),
                    broker: self.options.broker.clone(),
                });
            read_credentials
                .as_ref()
                .or(self.credentials.as_ref())
                .map_or_else(sign::public_headers, |c| {
                    sign::headers(c, method, path, body.as_deref().unwrap_or(""), cx.now)
                })
        } else {
            sign::public_headers()
        };
        cx.request(
            method,
            format!("{REST}{}", encoded_path(path)),
            headers,
            body,
            pending,
        );
    }
    fn streamed(&self, cx: &mut Cx<'_, Pending>, path: &str, at: ArrayAt, pending: Pending) {
        cx.request_streamed(
            Method::Get,
            format!("{REST}{path}"),
            sign::public_headers(),
            at,
            pending,
        );
    }
    fn bullet(&mut self, cx: &mut Cx<'_, Pending>, channel: usize) {
        self.request(
            cx,
            Method::Post,
            if channel == 1 {
                "/api/v1/bullet-private"
            } else {
                "/api/v1/bullet-public"
            },
            None,
            channel == 1,
            Pending::Bullet(channel),
        );
    }
    fn topic(&mut self, cx: &mut Cx<'_, Pending>, channel: usize, topic: &str, subscribe: bool) {
        if !self.sockets[channel].ready {
            return;
        }
        if let Some(socket) = self.sockets[channel].socket {
            self.next_id += 1;
            #[derive(Serialize)]
            #[serde(rename_all = "camelCase")]
            struct Subscription<'a> {
                id: u64,
                #[serde(rename = "type")]
                kind: &'a str,
                topic: &'a str,
                private_channel: bool,
                response: bool,
            }
            cx.send(
                socket,
                serde_json::to_string(&Subscription {
                    id: self.next_id,
                    kind: if subscribe {
                        "subscribe"
                    } else {
                        "unsubscribe"
                    },
                    topic,
                    private_channel: channel == 1,
                    response: true,
                })
                .unwrap(),
            );
        }
    }
    fn subscribe(&mut self, cx: &mut Cx<'_, Pending>, channel: usize) {
        if channel == 1 {
            for t in ["/spotMarket/tradeOrdersV2", "/account/balance"] {
                self.topic(cx, channel, t, true);
            }
        } else {
            self.topic(cx, 0, "/market/ticker:all", true);
            let topics = self
                .books
                .iter()
                .filter(|(_, b)| b.subscribers > 0)
                .map(|(s, _)| format!("/spotMarket/level2Depth50:{s}"))
                .chain(
                    self.candles
                        .keys()
                        .map(|(s, i)| format!("/market/candles:{s}_{}", interval(i).unwrap().1)),
                )
                .collect::<Vec<_>>();
            for t in topics {
                self.topic(cx, 0, &t, true);
            }
        }
    }
    fn ping(&mut self, cx: &mut Cx<'_, Pending>, channel: usize) {
        let c = &mut self.sockets[channel];
        if !c.ready {
            return;
        }
        if let Some(t) = c.ping.take() {
            cx.clear_timer(t);
        }
        if let Some(socket) = c.socket {
            cx.send(
                socket,
                format!(r#"{{"id":{},"type":"ping"}}"#, cx.now as u64),
            );
            c.pong = Some(cx.set_timer(c.ping_timeout.max(1000.0)));
        }
    }
    fn reconnect(&mut self, cx: &mut Cx<'_, Pending>, channel: usize) {
        if channel == 1 {
            self.revision += 1;
            self.balance_revision += 1;
        }
        let c = &mut self.sockets[channel];
        c.ready = false;
        for t in [c.ping.take(), c.pong.take(), c.reconnect.take()]
            .into_iter()
            .flatten()
        {
            cx.clear_timer(t);
        }
        if let Some(socket) = c.socket.take() {
            cx.close(socket);
        }
        c.reconnect = Some(cx.set_timer(1000.0));
    }
    fn fixed_settings(&self, cx: &mut Cx<'_, Pending>) {
        let settings = cx
            .store
            .markets()
            .iter()
            .enumerate()
            .map(|(i, _)| {
                (
                    i as u32,
                    MarketSetting {
                        leverage: 1.0,
                        margin_type: Some(MarginType::Isolated),
                        buy_leverage: Some(1.0),
                        sell_leverage: Some(1.0),
                    },
                )
            })
            .collect::<Vec<_>>();
        cx.store.set_settings(settings);
        cx.store.set_hedged(false);
        cx.store.set_positions(vec![]);
    }
    fn balance(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.balance_loading || self.credentials.is_none() {
            return;
        }
        self.balance_loading = true;
        self.request(
            cx,
            Method::Get,
            "/api/v1/accounts?type=trade",
            None,
            true,
            Pending::Balance(self.balance_revision),
        );
    }
    fn orders(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.orders_loading || self.credentials.is_none() {
            return;
        }
        self.orders_loading = true;
        self.request(
            cx,
            Method::Get,
            "/api/v1/hf/orders/active/symbols",
            None,
            true,
            Pending::Symbols {
                revision: self.revision,
            },
        );
    }
    fn page(&self, cx: &mut Cx<'_, Pending>, r: ReadOrders) {
        self.request(
            cx,
            Method::Get,
            &format!(
                "/api/v1/hf/orders/active/page?symbol={}&pageNum={}&pageSize=50",
                r.symbol, r.page
            ),
            None,
            true,
            Pending::Orders(r),
        );
    }
    fn heal(&mut self, cx: &mut Cx<'_, Pending>, balance: bool) {
        let timer = if balance {
            &mut self.heal_balance
        } else {
            &mut self.heal_orders
        };
        if timer.is_none() {
            *timer = Some(cx.set_timer(250.0));
        }
    }
    fn account_balance(&self, cx: &mut Cx<'_, Pending>, data: &RawValue) -> Result<(), String> {
        let rows: Vec<Account> = serde_json::from_str(data.get()).map_err(|e| e.to_string())?;
        let mut sum = Balance::default();
        let mut currencies = vec![];
        let mut coins = vec![];
        for r in rows {
            if r.currency.is_empty() || r.available.0 < 0.0 || r.holds.0 < 0.0 || r.balance.0 < 0.0
            {
                return Err("invalid KuCoin Spot trade-account balance".into());
            }
            let p = if matches!(r.currency, "USD" | "USDT" | "USDC") {
                1.0
            } else {
                ["USDT", "USDC", "USD"]
                    .into_iter()
                    .find_map(|q| cx.store.ticker(&format!("{}-{q}", r.currency)))
                    .map_or(0.0, |t| t.last)
            };
            currencies.push(CurrencyBalance {
                currency: r.currency.into(),
                balance: Balance {
                    free: r.available.0,
                    used: r.holds.0,
                    total: r.balance.0,
                    upnl: 0.0,
                },
            });
            coins.push(CoinBalance {
                base: r.currency.into(),
                amount: r.balance.0,
                notional: r.balance.0 * p,
            });
            sum.free += r.available.0 * p;
            sum.used += r.holds.0 * p;
            sum.total += r.balance.0 * p;
        }
        cx.store.set_currency_balances(currencies);
        cx.store.set_coin_balances(coins);
        cx.store.set_balance(sum);
        Ok(())
    }
    fn private_message(&mut self, cx: &mut Cx<'_, Pending>, m: Message<'_>) {
        if m.topic.starts_with("/account/balance") {
            self.balance_revision += 1;
            self.heal(cx, true);
            return;
        }
        if !m.topic.starts_with("/spotMarket/tradeOrdersV2") {
            return;
        }
        self.revision += 1;
        self.heal(cx, false);
        let Some(row) = m
            .data
            .and_then(|d| serde_json::from_str::<OrderRow>(d.get()).ok())
        else {
            return;
        };
        if row.id().is_empty() {
            return;
        }
        let previous = cx.store.order(row.id());
        if !matches!(row.side, "" | "buy" | "sell")
            || previous.is_some_and(|o| !row.symbol.is_empty() && row.symbol != o.symbol)
            || (row.event == "match" && row.side.is_empty() && previous.is_none())
        {
            return;
        }
        let Some(mut order) = row.order(cx.now, previous, true) else {
            return;
        };
        if !valid_native_quantities(&row) || order.price < 0.0 {
            return;
        }
        if order.status == OrderStatus::Open
            && !self.terminal.contains(row.id())
            && previous.is_none_or(|old| order.last_update >= old.last_update)
            && (!valid_order(&order) || previous.is_some_and(|old| old.filled > order.amount))
        {
            return;
        }
        if row.event == "match" {
            let amount = row.match_size.or(row.filled_size).map_or(0.0, |n| n.0);
            let price = row.match_price.or(row.price).map_or(0.0, |n| n.0);
            if amount <= 0.0
                || price <= 0.0
                || row.total().is_some_and(|total| exceeds(amount, total))
                || row
                    .cumulative()
                    .is_some_and(|filled| exceeds(amount, filled))
            {
                return;
            }
            let key = if let Some(id) = row.trade_id {
                format!("{}:{id}", row.id())
            } else {
                format!(
                    "{}:{}:{amount}:{price}",
                    row.id(),
                    row.ts.map_or(order.last_update, |n| n.0)
                )
            };
            if amount > 0.0 && self.fills.insert(key.clone()) {
                self.fill_order.push_back(key);
                if self.fill_order.len() > 20_000
                    && let Some(k) = self.fill_order.pop_front()
                {
                    self.fills.remove(&k);
                }
                cx.fill(Fill {
                    order_id: order.id.clone(),
                    symbol: order.symbol.clone(),
                    side: order.side,
                    price,
                    amount,
                });
            }
        }
        let old = cx.store.order(row.id());
        if old.is_some_and(|o| order.last_update < o.last_update) {
            return;
        }
        if self.terminal.contains(row.id()) && order.status == OrderStatus::Open {
            return;
        }
        if order.status == OrderStatus::Open && !valid_order(&order) {
            return;
        }
        if let Some(old) = old {
            order.filled = order.filled.max(old.filled);
            order.remaining = order.remaining.min((order.amount - order.filled).max(0.0));
        }
        if order.status == OrderStatus::Open && !valid_order(&order) {
            return;
        }
        if matches!(order.status, OrderStatus::Canceled | OrderStatus::Closed) {
            // Terminal identities fence snapshots started before a fill/cancel.
            if self.terminal.insert(order.id.clone()) {
                self.terminal_order.push_back(order.id.clone());
                if self.terminal_order.len() > 20_000
                    && let Some(id) = self.terminal_order.pop_front()
                {
                    self.terminal.remove(&id);
                }
            }
            cx.store.remove_orders(&[&order.id]);
        } else {
            cx.store.upsert_order(order);
        }
    }
    fn price(cx: &mut Cx<'_, Pending>, topic: &str, subject: &str, p: Price<'_>) {
        let symbol = if !p.symbol.is_empty() {
            p.symbol
        } else if !subject.is_empty() {
            subject
        } else {
            topic.split_once(':').map_or("", |(_, s)| s)
        };
        cx.store.update_ticker(symbol, |t| p.apply(t));
    }
    fn public_message(&mut self, cx: &mut Cx<'_, Pending>, m: Message<'_>) {
        let Some(data) = m.data else {
            return;
        };
        if m.topic.starts_with("/market/ticker") {
            if let Ok(p) = serde_json::from_str::<Price>(data.get()) {
                Self::price(cx, m.topic, m.subject, p);
            }
        } else if let Some(symbol) = m.topic.strip_prefix("/spotMarket/level2Depth50:") {
            #[derive(Deserialize)]
            struct Depth {
                bids: Vec<(Number, Number)>,
                asks: Vec<(Number, Number)>,
                #[serde(default)]
                timestamp: Option<Number>,
            }
            if let Some(b) = self.books.get_mut(symbol).filter(|b| b.subscribers > 0)
                && let Ok(d) = serde_json::from_str::<Depth>(data.get())
            {
                let time = d.timestamp.map_or(cx.now, |n| n.0);
                if time < b.last {
                    return;
                }
                b.last = time;
                b.book.clear();
                for (side, rows) in [(BookSide::Bids, d.bids), (BookSide::Asks, d.asks)] {
                    for (p, a) in rows {
                        b.book.apply(side, p.0, a.0);
                    }
                }
                cx.book_changed(symbol);
            }
        } else if let Some(topic) = m.topic.strip_prefix("/market/candles:")
            && let Some((symbol, native)) = topic.rsplit_once('_')
        {
            let Some(i) = self
                .candles
                .keys()
                .find(|(s, i)| s == symbol && interval(i).is_ok_and(|(_, v)| v == native))
                .map(|(_, i)| i.clone())
            else {
                return;
            };
            #[derive(Deserialize)]
            struct Update<'a> {
                #[serde(borrow)]
                candles: &'a RawValue,
            }
            if let Ok(u) = serde_json::from_str::<Update>(data.get())
                && let Ok(c) = candle(u.candles.get())
            {
                cx.candle(symbol, &i, c);
            }
        }
    }
}
fn exceeds(value: f64, bound: f64) -> bool {
    value > bound + (bound.abs() * 1e-12).max(1e-12)
}
fn valid_native_quantities(row: &OrderRow<'_>) -> bool {
    let (total, filled, remaining) = (row.total(), row.cumulative(), row.remain_size.map(|n| n.0));
    if [total, filled, remaining]
        .into_iter()
        .flatten()
        .any(|n| n < 0.0)
    {
        return false;
    }
    match total {
        Some(total) => {
            !filled.is_some_and(|filled| exceeds(filled, total))
                && !remaining.is_some_and(|remaining| exceeds(remaining, total))
                && !filled
                    .zip(remaining)
                    .is_some_and(|(filled, remaining)| exceeds(filled + remaining, total))
        }
        None => true,
    }
}
fn valid_order(o: &Order) -> bool {
    o.amount >= 0.0
        && o.filled >= 0.0
        && o.filled <= o.amount
        && o.remaining >= 0.0
        && o.remaining <= o.amount
        && o.price >= 0.0
        && o.filled + o.remaining <= o.amount + (o.amount * 1e-12).max(1e-12)
}
fn lookup_output(symbol: &str, id: &str, rows: &[Order]) -> String {
    #[derive(Serialize)]
    struct Lookup<'a> {
        found: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        order: Option<&'a Order>,
        #[serde(skip_serializing_if = "Option::is_none")]
        definitive: Option<bool>,
    }
    let order = rows.iter().find(|o| {
        o.symbol == symbol
            && !matches!(id, "" | "0" | "undefined")
            && (o.id == id || o.client_order_id.as_deref() == Some(id))
    });
    serde_json::to_string(&Lookup {
        found: order.is_some(),
        order,
        definitive: order.is_none().then_some(false),
    })
    .unwrap()
}
fn candle(text: &str) -> Result<Candle, String> {
    let r: Vec<Number> = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if r.len() < 6 {
        return Err("invalid KuCoin Spot candle".into());
    }
    Ok(Candle {
        timestamp: if r[0].0 > 1e10 {
            (r[0].0 / 1000.0).floor()
        } else {
            r[0].0
        },
        open: r[1].0,
        close: r[2].0,
        high: r[3].0,
        low: r[4].0,
        volume: r[5].0,
    })
}

impl Venue for KucoinSpot {
    type Pending = Pending;
    fn supports_amend_order_with_symbol(&self) -> bool {
        true
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(read) = &self.options.read_credentials
            && let Err(error) = read
                .validate()
                .and_then(|_| read.validate_passphrase(self.options.passphrase.as_deref()))
        {
            cx.log(LogLevel::Error, error);
            return;
        }
        if self.options.testnet || (!self.options.public_data_only && self.credentials.is_none()) {
            cx.log(
                LogLevel::Error,
                "KuCoin Spot requires production key, secret and passphrase",
            );
            return;
        }
        self.streamed(
            cx,
            "/api/v2/symbols",
            ArrayAt::EnvelopeField("data"),
            Pending::Markets {
                rows: vec![],
                invalid: false,
            },
        );
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if let Some(channel) = self.sockets.iter().position(|c| c.socket == Some(socket)) {
            self.sockets[channel].pong = Some(cx.set_timer(10000.0));
        }
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        let Some(channel) = self.sockets.iter().position(|c| c.socket == Some(socket)) else {
            return;
        };
        // Decode high-volume ticker envelopes and prices together, without a raw-data rescan.
        if channel == 0
            && self.sockets[channel].ready
            && let Ok(m) = serde_json::from_str::<PriceMessage>(text)
            && m.kind == "message"
            && m.topic.starts_with("/market/ticker")
        {
            Self::price(cx, m.topic, m.subject, m.data);
            return;
        }
        let Ok(m) = serde_json::from_str::<Message>(text) else {
            return;
        };
        match m.kind {
            "welcome" => {
                if self.sockets[channel].ready {
                    return;
                }
                self.sockets[channel].ready = true;
                if let Some(t) = self.sockets[channel].pong.take() {
                    cx.clear_timer(t);
                }
                self.subscribe(cx, channel);
                self.ping(cx, channel);
                if channel == 1 {
                    self.balance(cx);
                    self.orders(cx);
                }
            }
            "pong" => {
                let c = &mut self.sockets[channel];
                // A pong acknowledges a sent ping, never the welcome watchdog.
                if !c.ready || c.pong.is_none() {
                    return;
                }
                if let Some(t) = c.pong.take() {
                    cx.clear_timer(t);
                }
                if let Some(t) = c.ping.take() {
                    cx.clear_timer(t);
                }
                c.ping = Some(cx.set_timer(c.ping_interval.max(1000.0)));
            }
            "message" if self.sockets[channel].ready => {
                if channel == 1 {
                    self.private_message(cx, m);
                } else {
                    self.public_message(cx, m);
                }
            }
            _ => {}
        }
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _code: u16) {
        if let Some(c) = self.sockets.iter().position(|c| c.socket == Some(socket)) {
            self.sockets[c].socket = None;
            self.reconnect(cx, c);
        }
    }
    fn on_item(&mut self, _cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match pending {
            Pending::Markets { rows, invalid } => match serde_json::from_str::<Instrument>(item) {
                Ok(r) => rows.push(r.market()),
                Err(_) => *invalid = true,
            },
            Pending::Tickers { rows, invalid } => match serde_json::from_str::<Price>(item) {
                Ok(r) => rows.push(r.ticker()),
                Err(_) => *invalid = true,
            },
            Pending::Candles { rows, invalid, .. } => match candle(item) {
                Ok(c) => rows.push(c),
                Err(_) => *invalid = true,
            },
            _ => {}
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        let result = success(response.status, response.body)
            .and_then(|e| e.data.ok_or_else(|| "KuCoin Spot missing data".into()));
        let data = match result {
            Ok(d) => d,
            Err(e) => {
                self.on_request_failed(cx, pending, &e);
                return;
            }
        };
        match pending {
            Pending::Markets { rows, invalid } => {
                if invalid {
                    cx.log(LogLevel::Error, "invalid KuCoin Spot markets");
                    return;
                }
                cx.store.set_markets(rows);
                self.fixed_settings(cx);
                self.streamed(
                    cx,
                    "/api/v1/market/allTickers",
                    ArrayAt::Path(&["data", "ticker"]),
                    Pending::Tickers {
                        rows: vec![],
                        invalid: false,
                    },
                );
            }
            Pending::Tickers { mut rows, invalid } => {
                if invalid {
                    cx.log(LogLevel::Error, "invalid KuCoin Spot tickers");
                    return;
                }
                rows.retain(|r| cx.store.market(&r.symbol).is_some());
                cx.store.set_tickers(rows);
                self.bullet(cx, 0);
                if self.credentials.is_some() {
                    self.bullet(cx, 1);
                    self.balance(cx);
                    self.orders(cx);
                    self.poll = Some(cx.set_timer(5000.0));
                } else {
                    cx.store.set_orders(vec![]);
                    cx.store.set_balance(Balance::default());
                }
            }
            Pending::Bullet(channel) => match serde_json::from_str::<Bullet>(data.get()) {
                Ok(t)
                    if !t.token.is_empty()
                        && t.instance_servers
                            .first()
                            .is_some_and(|s| s.endpoint.starts_with("wss://")) =>
                {
                    let server = &t.instance_servers[0];
                    let c = &mut self.sockets[channel];
                    c.ping_interval = server.ping_interval;
                    c.ping_timeout = server.ping_timeout;
                    let url = format!(
                        "{}{}token={}&connectId={}",
                        server.endpoint,
                        if server.endpoint.contains('?') {
                            "&"
                        } else {
                            "?"
                        },
                        crate::encode(t.token),
                        cx.now as u64
                    );
                    c.socket = Some(cx.connect_channel(
                        url,
                        if channel == 1 {
                            SocketChannel::Private
                        } else {
                            SocketChannel::Public
                        },
                    ));
                }
                _ => {
                    cx.log(LogLevel::Error, "invalid KuCoin Spot bullet server");
                    self.reconnect(cx, channel);
                }
            },
            Pending::Balance(revision) => {
                self.balance_loading = false;
                if revision == self.balance_revision {
                    if let Err(e) = self.account_balance(cx, data) {
                        cx.log(LogLevel::Error, e);
                    }
                } else {
                    self.heal(cx, true);
                }
            }
            Pending::Symbols { revision } => {
                let symbols = match serde_json::from_str::<Symbols>(data.get()) {
                    Ok(s) => s
                        .symbols
                        .into_iter()
                        .map(str::to_owned)
                        .collect::<VecDeque<_>>(),
                    Err(e) => {
                        self.orders_loading = false;
                        cx.log(LogLevel::Error, e.to_string());
                        return;
                    }
                };
                let mut r = ReadOrders {
                    revision,
                    symbols,
                    symbol: String::new(),
                    page: 1,
                    rows: vec![],
                    lookup: None,
                };
                if let Some(symbol) = r.symbols.pop_front() {
                    r.symbol = symbol;
                    self.page(cx, r);
                } else {
                    self.orders_loading = false;
                    if revision == self.revision {
                        cx.store.set_orders(vec![]);
                    } else {
                        self.heal(cx, false);
                    }
                }
            }
            Pending::Orders(mut r) => {
                let p = match serde_json::from_str::<Page>(data.get()) {
                    Ok(p) => p,
                    Err(e) => {
                        self.on_request_failed(cx, Pending::Orders(r), &e.to_string());
                        return;
                    }
                };
                let count = p.items.len();
                for raw in p.items {
                    let row = match serde_json::from_str::<OrderRow>(raw.get()) {
                        Ok(row) => row,
                        Err(e) => {
                            self.on_request_failed(cx, Pending::Orders(r), &e.to_string());
                            return;
                        }
                    };
                    if row.id().is_empty()
                        || row.symbol.is_empty()
                        || !matches!(row.side, "buy" | "sell")
                        || !matches!(row.event, "limit" | "market")
                        || row.size.or(row.origin_size).is_none()
                    {
                        self.on_request_failed(
                            cx,
                            Pending::Orders(r),
                            "invalid KuCoin Spot active-order row",
                        );
                        return;
                    }
                    if let Some(mut o) = row.order(cx.now, None, false) {
                        if o.id.is_empty()
                            || !valid_order(&o)
                            || (o.kind == OrderType::Limit && row.price.is_none())
                            || o.symbol != r.symbol
                            || r.rows.len() >= 20_000
                            || r.rows.iter().any(|row| row.id == o.id)
                        {
                            self.on_request_failed(
                                cx,
                                Pending::Orders(r),
                                "invalid or oversized KuCoin Spot active-order snapshot",
                            );
                            return;
                        }
                        if let Some(current) = cx.store.order(&o.id) {
                            if o.last_update < current.last_update || o.amount < current.filled {
                                o = current.clone();
                            } else {
                                o.filled = o.filled.max(current.filled);
                                o.remaining = o.remaining.min((o.amount - o.filled).max(0.0));
                            }
                        }
                        if !self.terminal.contains(&o.id) && o.status == OrderStatus::Open {
                            r.rows.push(o);
                        }
                    } else {
                        self.on_request_failed(
                            cx,
                            Pending::Orders(r),
                            "missing KuCoin Spot active-order identity",
                        );
                        return;
                    }
                }
                let more = p.total_page.map_or(count >= 50, |n| r.page < n);
                if more {
                    if r.page >= 100 {
                        self.on_request_failed(
                            cx,
                            Pending::Orders(r),
                            "KuCoin Spot orders exceeded 100 pages",
                        );
                        return;
                    }
                    r.page += 1;
                    self.page(cx, r);
                } else if let Some(symbol) = r.symbols.pop_front() {
                    r.symbol = symbol;
                    r.page = 1;
                    self.page(cx, r);
                } else if let Some((command, id)) = r.lookup {
                    cx.done(command, Ok(lookup_output(&r.symbol, &id, &r.rows)));
                } else {
                    self.orders_loading = false;
                    if r.revision == self.revision {
                        cx.store.set_orders(r.rows);
                    } else {
                        self.heal(cx, false);
                    }
                }
            }
            Pending::Write(job) => self.write_response(cx, job, data),
            Pending::Candles {
                command,
                mut rows,
                invalid,
            } => {
                if invalid {
                    cx.done(command, Err("invalid KuCoin Spot candles".into()));
                } else {
                    rows.sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
                    cx.done(command, Ok(serde_json::to_string(&rows).unwrap()));
                }
            }
            Pending::Account(command) => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Summary<'a> {
                    #[serde(borrow)]
                    user_id: Option<&'a RawValue>,
                    #[serde(borrow)]
                    uid: Option<&'a RawValue>,
                    #[serde(borrow)]
                    sub_user_id: Option<&'a RawValue>,
                    #[serde(borrow)]
                    referrer_user_id: Option<&'a RawValue>,
                }
                let result = serde_json::from_str::<Summary>(data.get())
                    .map(|s| {
                        fn present(v: Option<&RawValue>) -> Option<&str> {
                            v.map(RawValue::get)
                                .filter(|s| !matches!(*s, "null" | "0" | "false" | "\"\""))
                        }
                        format!(
                            r#"{{"userId":{},"subId":{},"affiliateId":{}}}"#,
                            present(s.user_id)
                                .or_else(|| present(s.uid))
                                .unwrap_or("\"\""),
                            present(s.sub_user_id).unwrap_or("null"),
                            present(s.referrer_user_id).unwrap_or("null")
                        )
                    })
                    .map_err(|e| e.to_string());
                cx.done(command, result);
            }
            Pending::Fills(command) => {
                #[derive(Serialize)]
                #[serde(rename_all = "camelCase")]
                struct Execution<'a> {
                    timestamp: f64,
                    symbol: &'a str,
                    order_id: &'a str,
                    price: f64,
                    qty: f64,
                    side: &'a str,
                    commission: f64,
                    commission_asset: &'a str,
                }
                let out = (|| {
                    let p: Page = serde_json::from_str(data.get()).map_err(|e| e.to_string())?;
                    let mut rows = vec![];
                    for raw in p.items {
                        let r: FillRow =
                            serde_json::from_str(raw.get()).map_err(|e| e.to_string())?;
                        rows.push(Execution {
                            timestamp: r.created_at.0,
                            symbol: r.symbol,
                            order_id: r.order_id,
                            price: r.price.0,
                            qty: r.size.0,
                            side: if r.side == "sell" { "sell" } else { "buy" },
                            commission: r.fee.0,
                            commission_asset: r.fee_currency,
                        });
                    }
                    serde_json::to_string(&rows).map_err(|e| e.to_string())
                })();
                cx.done(command, out);
            }
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::Bullet(c) => self.reconnect(cx, c),
            Pending::Balance(_) => self.balance_loading = false,
            Pending::Symbols { .. } => self.orders_loading = false,
            Pending::Orders(r) => {
                if let Some((command, id)) = r.lookup {
                    let out = lookup_output(&r.symbol, &id, cx.store.orders());
                    cx.done(command, Ok(out));
                } else {
                    self.orders_loading = false;
                }
            }
            Pending::Write(j) => cx.done(j.command, Err(error.into())),
            Pending::Candles { command, .. }
            | Pending::Fills(command)
            | Pending::Account(command) => cx.done(command, Err(error.into())),
            _ => {}
        }
        cx.log(LogLevel::Error, error);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        for channel in 0..2 {
            let c = &mut self.sockets[channel];
            if c.reconnect == Some(timer) {
                c.reconnect = None;
                self.bullet(cx, channel);
                return;
            }
            if c.ping == Some(timer) {
                c.ping = None;
                self.ping(cx, channel);
                return;
            }
            if c.pong == Some(timer) {
                c.pong = None;
                self.reconnect(cx, channel);
                return;
            }
        }
        if self.heal_orders == Some(timer) {
            self.heal_orders = None;
            self.orders(cx);
        } else if self.heal_balance == Some(timer) {
            self.heal_balance = None;
            self.balance(cx);
        } else if self.poll == Some(timer) {
            self.orders(cx);
            self.balance(cx);
            self.poll = Some(cx.set_timer(5000.0));
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::SubscribeOrderBook { symbol } => {
                let b = self.books.entry(symbol.clone()).or_default();
                b.subscribers += 1;
                if b.subscribers == 1 {
                    self.topic(cx, 0, &format!("/spotMarket/level2Depth50:{symbol}"), true);
                }
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(b) = self.books.get_mut(&symbol) {
                    b.subscribers = b.subscribers.saturating_sub(1);
                    if b.subscribers == 0 {
                        self.topic(cx, 0, &format!("/spotMarket/level2Depth50:{symbol}"), false);
                        self.books.remove(&symbol);
                    }
                }
            }
            Command::SubscribeCandles {
                symbol,
                interval: i,
            } => {
                if interval(&i).is_err() {
                    cx.log(LogLevel::Error, "unsupported KuCoin Spot candle interval");
                    return;
                }
                let n = self.candles.entry((symbol.clone(), i.clone())).or_default();
                *n += 1;
                if *n == 1 {
                    self.topic(
                        cx,
                        0,
                        &format!("/market/candles:{symbol}_{}", interval(&i).unwrap().1),
                        true,
                    );
                }
            }
            Command::UnsubscribeCandles {
                symbol,
                interval: i,
            } => {
                if let Some(n) = self.candles.get_mut(&(symbol.clone(), i.clone())) {
                    *n = n.saturating_sub(1);
                    if *n == 0 {
                        self.candles.remove(&(symbol.clone(), i.clone()));
                        self.topic(
                            cx,
                            0,
                            &format!("/market/candles:{symbol}_{}", interval(&i).unwrap().1),
                            false,
                        );
                    }
                }
            }
            Command::SetTickerSymbols { .. } => {}
            Command::FetchCandles {
                command,
                symbol,
                interval: i,
                limit,
                from_ms,
                to_ms,
            } => {
                let Ok((minutes, native)) = interval(&i) else {
                    cx.done(
                        command,
                        Err("unsupported KuCoin Spot candle interval".into()),
                    );
                    return;
                };
                let end = to_ms.unwrap_or(cx.now);
                let start = match (limit, from_ms) {
                    (None, Some(from)) => from,
                    _ => {
                        end - f64::from(minutes)
                            * 60_000.0
                            * f64::from(limit.filter(|n| *n > 0).unwrap_or(500).min(1500))
                    }
                };
                let path = format!(
                    "/api/v1/market/candles?symbol={symbol}&type={native}&startAt={}&endAt={}",
                    (start / 1000.0).floor() as u64,
                    (end / 1000.0).floor() as u64
                );
                self.streamed(
                    cx,
                    &path,
                    ArrayAt::EnvelopeField("data"),
                    Pending::Candles {
                        command,
                        rows: vec![],
                        invalid: false,
                    },
                );
            }
            Command::Query { command, query } => self.query(cx, command, query),
            other => self.trade(cx, other),
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books
            .get(symbol)
            .filter(|b| b.subscribers > 0)
            .map(|b| &b.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(
            self.books
                .iter()
                .filter(|(_, b)| b.subscribers > 0)
                .map(|(s, b)| (s.as_str(), &b.book)),
        )
    }
}
impl KucoinSpot {
    fn query(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId, query: Query) {
        if self.credentials.is_none() {
            cx.done(command, Err("KuCoin Spot requires credentials".into()));
            return;
        }
        match query {
            Query::Fills {
                symbol,
                order_id,
                from_ms,
                to_ms,
                limit,
            } => {
                let Some(symbol) = symbol else {
                    cx.done(command, Ok("[]".into()));
                    return;
                };
                let mut path = format!("/api/v1/hf/fills?symbol={symbol}");
                if let Some(o) = order_id {
                    path.push_str(&format!("&orderId={o}"));
                }
                path.push_str(&format!(
                    "&limit={}",
                    limit.filter(|n| *n > 0).unwrap_or(100).min(100)
                ));
                if let Some(n) = from_ms {
                    path.push_str(&format!("&startAt={}", n as u64));
                }
                if let Some(n) = to_ms {
                    path.push_str(&format!("&endAt={}", n as u64));
                }
                self.request(cx, Method::Get, &path, None, true, Pending::Fills(command));
            }
            Query::OrderByClientId {
                symbol,
                client_order_id,
            } => {
                if let Some(o) = cx.store.orders().iter().find(|o| {
                    o.symbol == symbol
                        && !matches!(client_order_id.as_str(), "" | "0" | "undefined")
                        && (o.client_order_id.as_deref() == Some(&client_order_id)
                            || o.id == client_order_id)
                }) {
                    #[derive(Serialize)]
                    struct Found<'a> {
                        found: bool,
                        order: &'a Order,
                    }
                    cx.done(
                        command,
                        Ok(serde_json::to_string(&Found {
                            found: true,
                            order: o,
                        })
                        .unwrap()),
                    );
                } else {
                    self.page(
                        cx,
                        ReadOrders {
                            revision: self.revision,
                            symbols: VecDeque::new(),
                            symbol,
                            page: 1,
                            rows: vec![],
                            lookup: Some((command, client_order_id)),
                        },
                    );
                }
            }
            Query::PublicTrades { .. } => cx.done(
                command,
                Err("KuCoin Spot public trades are not implemented in TS".into()),
            ),
            Query::OrderHistory { .. } | Query::Income { .. } => cx.done(command, Ok("[]".into())),
        }
    }
}

#[cfg(test)]
mod security_credentials {
    use super::*;
    use safe_cex_crypto::secrets::EraseSecrets;
    #[test]
    fn spot_owns_both_credential_copies_in_scrubbing_carriers() {
        let mut venue = KucoinSpot::new(Options {
            key: Some("private-key".into()),
            secret: Some("private-secret".into()),
            passphrase: Some("private-passphrase".into()),
            broker: Some(sign::BrokerOptions {
                partner: "partner".into(),
                broker_key: "private-broker-key".into(),
                broker_name: "broker".into(),
                verify: None,
            }),
            ..Options::default()
        });
        let _: &safe_cex_crypto::secrets::Sensitive<Options> = &venue.options;
        assert!(venue.credentials.is_some());
        venue.options.erase_secrets();
        venue.credentials.as_mut().unwrap().erase_secrets();
        assert!(venue.options.secret.is_none());
        assert!(venue.options.broker.as_ref().unwrap().broker_key.is_empty());
        let credentials = venue.credentials.as_ref().unwrap();
        assert!(credentials.secret.is_empty() && credentials.passphrase.is_empty());
        assert!(credentials.broker.as_ref().unwrap().broker_key.is_empty());
    }
    #[test]
    fn incomplete_and_public_only_credentials_keep_a_scrubbing_owner() {
        for (key, public_data_only) in [
            (None, false),
            (Some(String::new()), false),
            (Some("key".into()), true),
        ] {
            let venue = KucoinSpot::new(Options {
                key,
                secret: Some("private-secret".into()),
                passphrase: Some("private-passphrase".into()),
                public_data_only,
                ..Options::default()
            });
            let _: &safe_cex_crypto::secrets::Sensitive<Options> = &venue.options;
            assert!(venue.credentials.is_none());
        }
    }
}
