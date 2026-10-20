use super::{Connection, KrakenSpot, Pending, wire::*};
use crate::wire::{number, optional_number};
use safe_cex_engine::url;
use safe_cex_engine::{
    Cx, Fill, LogLevel, Method, Response, SocketChannel, SocketId, TimerId, model::*,
};
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet, VecDeque},
};

#[derive(Clone)]
pub enum Kind {
    Balance {
        revision: u64,
        command: Option<safe_cex_engine::CommandId>,
    },
    Orders {
        revision: u64,
    },
    Token {
        socket: SocketId,
    },
    Trade(super::trading::Step),
}
#[derive(Clone)]
pub struct Request {
    pub kind: Kind,
    pub path: &'static str,
    pub params: Vec<(&'static str, String)>,
    pub retries: u8,
    pub signature_retries: u8,
}
#[derive(Default)]
pub struct Account {
    pub connection: Connection,
    pub revision: u64,
    native_times: HashMap<String, f64>,
    pub balance_revision: u64,
    balance_flight: HashSet<u64>,
    orders_flight: bool,
    pub provenance: HashMap<String, String>,
    pub terminal: HashSet<String>,
    terminal_fifo: VecDeque<String>,
    fills: HashSet<String>,
    fill_fifo: VecDeque<String>,
    retries: Vec<(TimerId, Request)>,
    poll: Option<TimerId>,
    orders_throttle: Option<TimerId>,
    started: bool,
}
#[derive(Deserialize)]
struct RestOrder<'a> {
    #[serde(borrow)]
    descr: Description<'a>,
    #[serde(deserialize_with = "number")]
    vol: f64,
    #[serde(deserialize_with = "number")]
    vol_exec: f64,
    #[serde(default, deserialize_with = "number")]
    opentm: f64,
    #[serde(default, borrow)]
    oflags: Cow<'a, str>,
}
#[derive(Deserialize)]
struct Description<'a> {
    #[serde(borrow)]
    pair: Cow<'a, str>,
    #[serde(borrow, rename = "type")]
    side: Cow<'a, str>,
    #[serde(borrow)]
    ordertype: Cow<'a, str>,
    #[serde(default, deserialize_with = "number")]
    price: f64,
}
#[derive(Deserialize)]
struct Open<'a> {
    #[serde(borrow)]
    open: &'a serde_json::value::RawValue,
}
#[derive(Deserialize)]
struct Token<'a> {
    #[serde(borrow)]
    token: Cow<'a, str>,
}
#[derive(Deserialize)]
struct Amount(#[serde(deserialize_with = "number")] f64);
#[derive(Deserialize)]
struct Triggers<'a> {
    #[serde(default, deserialize_with = "optional_number")]
    actual_price: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    price: Option<f64>,
    #[serde(default, borrow)]
    price_type: Cow<'a, str>,
}
impl Triggers<'_> {
    fn effective(&self) -> Option<f64> {
        self.actual_price.or_else(|| {
            (self.price_type == "static")
                .then_some(self.price)
                .flatten()
        })
    }
}
#[derive(Deserialize)]
struct Execution<'a> {
    #[serde(default, borrow)]
    order_id: Cow<'a, str>,
    #[serde(default, borrow)]
    symbol: Cow<'a, str>,
    #[serde(default, borrow)]
    side: Cow<'a, str>,
    #[serde(default, borrow)]
    order_type: Cow<'a, str>,
    #[serde(default, borrow)]
    exec_type: Cow<'a, str>,
    #[serde(default, borrow)]
    exec_id: Cow<'a, str>,
    #[serde(default, borrow)]
    timestamp: Cow<'a, str>,
    #[serde(default, borrow)]
    time_in_force: Cow<'a, str>,
    #[serde(default)]
    post_only: bool,
    #[serde(default, deserialize_with = "optional_number")]
    order_qty: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    cum_qty: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    last_qty: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    avg_price: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    last_price: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    limit_price: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    stop_price: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    trigger_price: Option<f64>,
    #[serde(default, borrow)]
    triggers: Option<Triggers<'a>>,
}
pub(super) fn order_kind(s: &str) -> Option<OrderType> {
    Some(match s {
        "limit" | "iceberg" | "settle-position" => OrderType::Limit,
        "market" => OrderType::Market,
        "stop-loss" | "stop-loss-limit" => OrderType::StopLoss,
        "take-profit" | "take-profit-limit" => OrderType::TakeProfit,
        "trailing-stop" | "trailing-stop-limit" => OrderType::TrailingStopLoss,
        _ => return None,
    })
}
#[allow(clippy::too_many_arguments)]
fn order(
    id: String,
    symbol: String,
    kind: OrderType,
    side: OrderSide,
    price: f64,
    amount: f64,
    filled: f64,
    tif: TimeInForce,
    time: f64,
) -> Order {
    Order {
        id,
        parent_id: None,
        status: OrderStatus::Open,
        symbol,
        kind,
        side,
        price,
        amount,
        filled,
        remaining: (amount - filled).max(0.),
        reduce_only: false,
        close: None,
        client_order_id: None,
        time_in_force: Some(tif),
        last_update: time,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    }
}
pub(super) fn form(params: &[(&str, String)]) -> String {
    params
        .iter()
        .map(|(k, v)| format!("{}={}", url::encode(k), url::encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}
impl KrakenSpot {
    pub(super) fn send_private(&mut self, cx: &mut Cx<'_, Pending>, request: Request) {
        let signed = (|| {
            if self.options.public_data_only {
                return Err("Kraken Spot credentials required".into());
            }
            let key = self
                .options
                .key
                .as_ref()
                .filter(|k| !k.is_empty())
                .ok_or("Kraken Spot credentials required")?;
            let signer = self.signer.as_mut().ok_or("Kraken Spot invalid secret")?;
            let nonce = signer.nonce(cx.now)?;
            let mut params = request.params.clone();
            params.push(("nonce", nonce.clone()));
            let body = form(&params);
            let signature = signer.spot_signature(&body, &nonce, request.path)?;
            Ok::<_, String>((key.clone(), body, signature))
        })();
        match signed {
            Ok((key, body, signature)) => {
                let id = cx.request(
                    Method::Post,
                    format!("https://api.kraken.com{}", request.path),
                    vec![
                        ("API-Key", key),
                        ("API-Sign", signature),
                        ("Content-Type", "application/x-www-form-urlencoded".into()),
                        ("Accept", "application/json".into()),
                    ],
                    Some(body),
                    Pending::Private(request),
                );
                cx.request_timeout(id, 20_000);
            }
            Err(e) => {
                if !self.options.public_data_only && self.signer.is_none() {
                    self.reject_auth(cx, &e);
                }
                self.private_failed(cx, request, &e);
            }
        }
    }
    pub(super) fn reject_auth(&self, cx: &mut Cx<'_, Pending>, error: &str) {
        cx.log(LogLevel::Error, error);
        cx.deactivate();
    }
    pub(super) fn request_private(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        path: &'static str,
        params: Vec<(&'static str, String)>,
        kind: Kind,
    ) {
        self.send_private(
            cx,
            Request {
                kind,
                path,
                params,
                retries: 0,
                signature_retries: 0,
            },
        );
    }
    pub(super) fn private_start(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.started {
            return;
        }
        self.account.started = true;
        if self.options.public_data_only {
            cx.store.set_balance(Balance::default());
            cx.store.set_orders(vec![]);
            cx.store.set_positions(vec![]);
            return;
        }
        self.spot_settings(cx);
        self.account.connection.socket =
            Some(cx.connect_channel("wss://ws-auth.kraken.com/v2", SocketChannel::Private));
        self.account.connection.received = cx.now;
        self.account.connection.watchdog = Some(cx.set_timer(15_000.));
        self.balance_read(cx, None);
        self.orders_read(cx);
        self.account.poll = Some(cx.set_timer(10_000.));
    }
    fn spot_settings(&self, cx: &mut Cx<'_, Pending>) {
        let settings: Vec<_> = cx
            .store
            .markets()
            .iter()
            .enumerate()
            .filter(|(_, m)| m.active)
            .map(|(i, _)| {
                (
                    i as u32,
                    safe_cex_engine::store::MarketSetting {
                        leverage: 1.,
                        margin_type: Some(MarginType::Isolated),
                        buy_leverage: None,
                        sell_leverage: None,
                    },
                )
            })
            .collect();
        cx.store.set_positions(vec![]);
        cx.store.set_settings(settings);
        cx.store.set_hedged(false);
    }
    pub(super) fn balance_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: Option<safe_cex_engine::CommandId>,
    ) {
        if !self.account.balance_flight.is_empty() && command.is_none() {
            return;
        }
        self.account.balance_revision += 1;
        self.account
            .balance_flight
            .insert(self.account.balance_revision);
        self.request_private(
            cx,
            "/0/private/Balance",
            vec![],
            Kind::Balance {
                revision: self.account.balance_revision,
                command,
            },
        );
    }
    fn orders_read(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.orders_flight {
            return;
        }
        self.account.orders_flight = true;
        self.request_private(
            cx,
            "/0/private/OpenOrders",
            vec![],
            Kind::Orders {
                revision: self.account.revision,
            },
        );
    }
    pub(super) fn private_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) -> bool {
        if self.account.connection.socket != Some(socket) {
            return false;
        }
        self.account.connection.open = true;
        self.account.connection.received = cx.now;
        self.request_private(
            cx,
            "/0/private/GetWebSocketsToken",
            vec![],
            Kind::Token { socket },
        );
        true
    }
    pub(super) fn private_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) -> bool {
        if self.account.connection.socket != Some(socket) {
            return false;
        }
        self.account.connection.socket = None;
        self.account.connection.open = false;
        self.account.revision += 1;
        self.account.balance_revision += 1;
        if let Some(t) = self.account.connection.watchdog.take() {
            cx.clear_timer(t)
        }
        self.account.connection.attempts = self.account.connection.attempts.saturating_add(1);
        self.account.connection.reconnect = Some(cx.set_timer(
            (1000. * 2f64.powi(self.account.connection.attempts.min(5) as i32 - 1)).min(30_000.),
        ));
        true
    }
    fn balance_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        text: &str,
        revision: u64,
    ) -> Result<(), String> {
        let mut currencies = vec![];
        let mut total = 0.;
        map_each::<Amount>(text, |raw, amount| {
            let amount = amount.0;
            if amount <= 0. || raw.contains('.') {
                return;
            }
            let asset = asset(&raw);
            let rate = if matches!(asset, "USD" | "USDT" | "USDC") {
                1.
            } else {
                cx.store
                    .tickers()
                    .iter()
                    .find(|t| {
                        cx.store.market(&t.symbol).is_some_and(|m| {
                            m.base == asset && matches!(m.quote.as_str(), "USD" | "USDT")
                        })
                    })
                    .map_or(0., |t| t.last.max(0.))
            };
            total += amount * rate;
            currencies.push(CurrencyBalance {
                currency: asset.into(),
                balance: Balance {
                    total: amount,
                    free: amount,
                    used: 0.,
                    upnl: 0.,
                },
            });
        })
        .map_err(|_| "Kraken Spot invalid balance")?;
        if !total.is_finite() {
            return Err("Kraken Spot invalid balance total".into());
        }
        if revision == self.account.balance_revision {
            cx.store.set_balance(Balance {
                total,
                free: total,
                used: 0.,
                upnl: 0.,
            });
            cx.store.set_currency_balances(currencies);
        }
        Ok(())
    }
    fn orders_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        text: &str,
        revision: u64,
    ) -> Result<(), String> {
        let e: Open<'_> =
            serde_json::from_str(text).map_err(|_| "Kraken Spot missing open orders")?;
        let mut rows = vec![];
        let mut provenance = HashMap::new();
        let mut invalid = false;
        map_each::<RestOrder<'_>>(e.open.get(), |id, o| {
            let Some(kind) = order_kind(&o.descr.ordertype) else {
                invalid = true;
                return;
            };
            if id.is_empty() || o.vol <= 0. || o.vol_exec < 0. || o.vol_exec > o.vol {
                invalid = true;
                return;
            }
            let symbol = self
                .identities
                .get(&*o.descr.pair)
                .cloned()
                .unwrap_or_else(|| o.descr.pair.to_string());
            let side = match &*o.descr.side {
                "buy" => OrderSide::Buy,
                "sell" => OrderSide::Sell,
                _ => {
                    invalid = true;
                    return;
                }
            };
            provenance.insert(id.to_string(), o.descr.ordertype.to_string());
            rows.push(order(
                id.to_string(),
                symbol,
                kind,
                side,
                o.descr.price,
                o.vol,
                o.vol_exec,
                if o.oflags.split(',').any(|f| f == "post") {
                    TimeInForce::PostOnly
                } else {
                    TimeInForce::GoodTillCancel
                },
                o.opentm * 1000.,
            ));
        })
        .map_err(|_| "Kraken Spot invalid orders")?;
        if invalid {
            return Err("Kraken Spot invalid orders".into());
        }
        if revision == self.account.revision {
            self.account.provenance = provenance;
            rows.retain(|o| !self.account.terminal.contains(&o.id));
            cx.store.set_orders(rows);
        }
        Ok(())
    }
    pub(super) fn private_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        let text = match result(response) {
            Ok(t) => t,
            Err(e) => {
                if e.contains("EAPI:Invalid signature") && request.signature_retries < 1 {
                    let mut r = request;
                    r.signature_retries += 1;
                    self.send_private(cx, r);
                    return;
                }
                if e.contains("EAPI:Invalid key")
                    || e.contains("EAPI:Invalid signature")
                    || e.contains("EGeneral:Permission denied")
                    || matches!(response.status, 401 | 403)
                {
                    self.reject_auth(cx, &e);
                }
                self.private_failed(cx, request, &e);
                return;
            }
        };
        let r = match &request.kind {
            Kind::Balance { revision, command } => {
                self.account.balance_flight.remove(revision);
                let r = self.balance_response(cx, text, *revision);
                if *revision != self.account.balance_revision
                    && self.account.balance_flight.is_empty()
                {
                    self.balance_read(cx, None);
                }
                if let Some(c) = command {
                    cx.done(*c, r.as_ref().map(|_| "null".into()).map_err(Clone::clone));
                }
                r
            }
            Kind::Orders { revision } => {
                self.account.orders_flight = false;
                self.orders_response(cx, text, *revision)
            }
            Kind::Token { socket } => {
                if self.account.connection.socket != Some(*socket) || !self.account.connection.open
                {
                    return;
                }
                let token = serde_json::from_str::<Token<'_>>(text)
                    .map_err(|_| "Kraken Spot invalid WS token".to_string());
                match token {
                    Ok(t) if !t.token.is_empty() => {
                        #[derive(Serialize)]
                        struct Params<'a> {
                            channel: &'a str,
                            token: &'a str,
                            #[serde(skip_serializing_if = "Option::is_none")]
                            snap_orders: Option<bool>,
                            #[serde(skip_serializing_if = "Option::is_none")]
                            snap_trades: Option<bool>,
                            #[serde(skip_serializing_if = "Option::is_none")]
                            order_status: Option<bool>,
                            #[serde(skip_serializing_if = "Option::is_none")]
                            snapshot: Option<bool>,
                        }
                        #[derive(Serialize)]
                        struct Sub<'a> {
                            method: &'static str,
                            params: Params<'a>,
                        }
                        for channel in ["executions", "balances"] {
                            let execution = channel == "executions";
                            cx.send(
                                *socket,
                                serde_json::to_string(&Sub {
                                    method: "subscribe",
                                    params: Params {
                                        channel,
                                        token: &t.token,
                                        snap_orders: execution.then_some(true),
                                        snap_trades: execution.then_some(false),
                                        order_status: execution.then_some(true),
                                        snapshot: (!execution).then_some(true),
                                    },
                                })
                                .unwrap(),
                            );
                        }
                        self.account.connection.attempts = 0;
                        Ok(())
                    }
                    _ => Err("Kraken Spot missing WS token".into()),
                }
            }
            Kind::Trade(step) => {
                self.trade_response(cx, step.clone(), text);
                Ok(())
            }
        };
        if let Err(e) = r {
            cx.log(LogLevel::Warning, e);
        }
    }
    pub(super) fn private_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        match request.kind {
            Kind::Balance { command, revision } => {
                self.account.balance_flight.remove(&revision);
                if let Some(c) = command {
                    cx.done(c, Err(error.into()))
                }
            }
            Kind::Orders { .. } => self.account.orders_flight = false,
            Kind::Token { socket } => {
                if self.account.connection.socket == Some(socket) {
                    cx.close(socket);
                    self.private_close(cx, socket);
                }
            }
            Kind::Trade(_) => self.trade_failed(cx, error),
        }
        cx.log(LogLevel::Warning, error);
    }
    pub(super) fn private_network_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        error: &str,
    ) {
        if !matches!(request.kind, Kind::Trade(_)) && request.retries < 3 {
            request.retries += 1;
            let timer = cx.set_timer(0.);
            self.account.retries.push((timer, request));
        } else {
            self.private_failed(cx, request, error)
        }
    }
    pub(super) fn private_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if let Some(i) = self.account.retries.iter().position(|(t, _)| *t == timer) {
            let (_, r) = self.account.retries.remove(i);
            self.send_private(cx, r);
            return true;
        }
        if self.account.poll == Some(timer) {
            self.account.poll = Some(cx.set_timer(10_000.));
            self.balance_read(cx, None);
            self.orders_read(cx);
            return true;
        }
        if self.account.orders_throttle == Some(timer) {
            self.account.orders_throttle = None;
            self.orders_read(cx);
            return true;
        }
        if self.account.connection.reconnect == Some(timer) {
            self.account.connection.reconnect = None;
            self.account.connection.socket =
                Some(cx.connect_channel("wss://ws-auth.kraken.com/v2", SocketChannel::Private));
            self.account.connection.received = cx.now;
            self.account.connection.watchdog = Some(cx.set_timer(15_000.));
            return true;
        }
        if self.account.connection.watchdog == Some(timer) {
            self.account.connection.watchdog = None;
            if cx.now - self.account.connection.received >= 45_000. {
                if let Some(s) = self.account.connection.socket {
                    cx.close(s);
                    self.private_close(cx, s);
                }
            } else {
                if let Some(s) = self
                    .account
                    .connection
                    .socket
                    .filter(|_| self.account.connection.open)
                {
                    cx.send(s, "{\"method\":\"ping\"}");
                }
                self.account.connection.watchdog = Some(cx.set_timer(15_000.));
            }
            return true;
        }
        false
    }
    pub(super) fn terminal(&mut self, id: &str) {
        if self.account.terminal.insert(id.into()) {
            self.account.terminal_fifo.push_back(id.into());
            if self.account.terminal_fifo.len() > 4096
                && let Some(old) = self.account.terminal_fifo.pop_front()
            {
                self.account.terminal.remove(&old);
                self.account.native_times.remove(&old);
            }
        }
    }
    pub(super) fn private_message(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        socket: SocketId,
        text: &str,
    ) -> bool {
        if self.account.connection.socket != Some(socket) {
            return false;
        }
        self.account.connection.received = cx.now;
        let Ok(msg) = serde_json::from_str::<Ws<'_>>(text) else {
            return true;
        };
        if msg.success == Some(false) {
            cx.log(LogLevel::Warning, "Kraken Spot private subscription failed");
            cx.close(socket);
            self.private_close(cx, socket);
            return true;
        }
        let Some(data) = msg.data else { return true };
        if msg.channel == "balances" {
            self.account.balance_revision += 1;
            self.balance_read(cx, None);
            return true;
        }
        if msg.channel != "executions" {
            return true;
        }
        let mut rows: Vec<Order> = vec![];
        let mut row_indices = HashMap::<String, usize>::new();
        let mut provenance = HashMap::new();
        let mut fills = vec![];
        let snapshot = msg.kind == "snapshot";
        let mut terminal = vec![];
        let mut times = HashMap::new();
        let mut invalid = false;
        let parsed = safe_cex_engine::json::each::<Execution<'_>>(data.get(), |e| {
            if e.order_id.is_empty() {
                invalid = true;
                return;
            }
            let time = crate::history::timestamp(&e.timestamp).unwrap_or(cx.now);
            if times
                .get(&*e.order_id)
                .or_else(|| self.account.native_times.get(&*e.order_id))
                .is_some_and(|t| time < *t)
            {
                return;
            }
            let existing = row_indices
                .get(&*e.order_id)
                .and_then(|i| rows.get(*i))
                .or_else(|| cx.store.order(&e.order_id));
            if e.cum_qty
                .is_some_and(|filled| existing.is_some_and(|o| filled < o.filled))
            {
                return;
            }
            times.insert(e.order_id.to_string(), time);

            if matches!(&*e.exec_type, "canceled" | "expired") {
                terminal.push(e.order_id.to_string());
                return;
            }
            if self.account.terminal.contains(&*e.order_id) && !snapshot {
                return;
            }
            let symbol = if e.symbol.is_empty() {
                existing.map(|o| o.symbol.clone()).unwrap_or_default()
            } else {
                self.ws_canonical
                    .get(&*e.symbol)
                    .cloned()
                    .unwrap_or_else(|| e.symbol.to_string())
            };
            if cx.store.market(&symbol).is_none() {
                times.remove(&*e.order_id);
                return;
            }
            let kind = order_kind(&e.order_type).or_else(|| existing.map(|o| o.kind));
            let Some(kind) = kind else {
                invalid = true;
                return;
            };
            let Some(amount) = e.order_qty.or_else(|| existing.map(|o| o.amount)) else {
                invalid = true;
                return;
            };
            let filled = e
                .cum_qty
                .or_else(|| existing.map(|o| o.filled))
                .unwrap_or(0.);
            if amount <= 0. || filled < 0. || filled > amount {
                invalid = true;
                return;
            }
            let side = match &*e.side {
                "buy" => OrderSide::Buy,
                "sell" => OrderSide::Sell,
                _ => {
                    let Some(side) = existing.map(|o| o.side) else {
                        invalid = true;
                        return;
                    };
                    side
                }
            };
            let trigger = e.triggers.as_ref().and_then(Triggers::effective);
            if trigger.is_some_and(|p| p <= 0.) {
                invalid = true;
                return;
            }
            let price = if matches!(
                kind,
                OrderType::StopLoss | OrderType::TakeProfit | OrderType::TrailingStopLoss
            ) {
                trigger
                    .or(e.stop_price)
                    .or(e.trigger_price)
                    .or(e.limit_price)
            } else {
                e.limit_price
            }
            .or_else(|| existing.map(|o| o.price))
            .unwrap_or(0.);
            let mut o = order(
                e.order_id.to_string(),
                symbol.clone(),
                kind,
                side,
                price,
                amount,
                filled,
                if e.post_only {
                    TimeInForce::PostOnly
                } else if e.time_in_force == "IOC" {
                    TimeInForce::ImmediateOrCancel
                } else {
                    existing
                        .and_then(|o| o.time_in_force)
                        .unwrap_or(TimeInForce::GoodTillCancel)
                },
                crate::history::timestamp(&e.timestamp).unwrap_or(cx.now),
            );
            if e.exec_type == "filled" || o.remaining == 0. {
                o.status = OrderStatus::Closed;
                terminal.push(o.id.clone());
            }
            if !e.order_type.is_empty() {
                provenance.insert(o.id.clone(), e.order_type.to_string());
            }
            let progress = (filled - existing.map_or(0., |o| o.filled)).max(0.);
            // Only trade rows describe executions; filled rows are lifecycle updates.
            if !snapshot && e.exec_type == "trade" {
                let qty = e.last_qty.unwrap_or(progress);
                let price = e.last_price.or(e.avg_price).unwrap_or(o.price);
                let key = if !e.exec_id.is_empty() {
                    e.exec_id.to_string()
                } else {
                    format!("{}:{filled}:{}", e.order_id, e.timestamp)
                };
                if qty > 0. && price > 0. {
                    fills.push((
                        key,
                        Fill {
                            order_id: o.id.clone(),
                            symbol,
                            side,
                            price,
                            amount: qty,
                        },
                    ));
                }
            }
            if let Some(index) = row_indices.get(&o.id).copied() {
                rows[index] = o;
            } else {
                row_indices.insert(o.id.clone(), rows.len());
                rows.push(o);
            }
        });
        if parsed.is_err() || invalid {
            cx.log(LogLevel::Warning, "Kraken Spot invalid execution frame");
            return true;
        }
        self.account.revision += 1;
        self.account.native_times.extend(times);
        if snapshot {
            rows.retain(|o| {
                o.status == OrderStatus::Open && !self.account.terminal.contains(&o.id)
            });
            cx.store.set_orders(rows);
            self.account.provenance = provenance;
            self.account
                .native_times
                .retain(|id, _| self.account.terminal.contains(id) || cx.store.order(id).is_some());
        } else {
            for o in rows {
                cx.store.upsert_order(o);
            }
            self.account.provenance.extend(provenance);
        }
        for id in terminal {
            self.terminal(&id);
            cx.store.set_order_status(
                &id,
                if cx.store.order(&id).is_some_and(|o| o.remaining == 0.) {
                    OrderStatus::Closed
                } else {
                    OrderStatus::Canceled
                },
            );
        }
        for (key, fill) in fills {
            if self.account.fills.insert(key.clone()) {
                self.account.fill_fifo.push_back(key);
                cx.fill(fill);
                if self.account.fill_fifo.len() > 4096
                    && let Some(old) = self.account.fill_fifo.pop_front()
                {
                    self.account.fills.remove(&old);
                }
            }
        }
        if !snapshot {
            if let Some(t) = self.account.orders_throttle.take() {
                cx.clear_timer(t)
            }
            self.account.orders_throttle = Some(cx.set_timer(1_000.));
        }
        true
    }
}
