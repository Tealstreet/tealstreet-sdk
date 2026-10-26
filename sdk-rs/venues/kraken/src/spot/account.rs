use super::{Connection, KrakenSpot, Pending, wire::*};
use crate::wire::{number, optional_number};
use safe_cex_engine::Level;
use safe_cex_engine::coverage::{RowReports, SnapshotBuilder};
use safe_cex_engine::execution::{Arithmetic, CumulativePrice, Cursor, Report};
use safe_cex_engine::fence::StaleGuard;
use safe_cex_engine::log::code;
use safe_cex_engine::url;
use safe_cex_engine::{Cx, Fill, Method, Response, SocketId, TimerId, model::*};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet, VecDeque},
};

const STALE_SNAPSHOT_LIMIT: u8 = 2;

#[derive(Clone)]
pub enum Kind {
    Balance {
        revision: u64,
    },
    Orders {
        revision: u64,
    },
    Token {
        epoch: safe_cex_engine::socket_lifecycle::Epoch,
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
    balance_guard: StaleGuard,
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
    wallet_valuation_pending: bool,
    wallet_valuation_warnings: u8,
    /// `coin:`/`order:` + key of rows already logged as skipped.
    skip_logged: RowReports<String>,
}
impl Account {
    /// Logs a skipped row once per `key` until it maps again; polls would otherwise repeat it.
    fn log_skip(&mut self, cx: &mut Cx<'_, Pending>, key: String, level: Level, message: String) {
        self.skip_logged.report(cx, key, level, message);
    }
    /// Re-arms the skip logs of `prefix`: all after a clean read, else those `ids` mapped.
    fn rearm_skips<'a>(&mut self, prefix: &str, clean: bool, ids: impl Iterator<Item = &'a str>) {
        self.skip_logged.rearm_namespace(prefix, clean, ids);
    }
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
/// What an execution row that failed to decode still says about its order.
#[derive(Default, Deserialize)]
struct ExecutionIdentity<'a> {
    #[serde(default, borrow)]
    order_id: Cow<'a, str>,
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
    fn current_token(&self, cx: &Cx<'_, Pending>, kind: &Kind) -> bool {
        match kind {
            Kind::Token { epoch } => {
                cx.socket_epoch(epoch.feed()) == Some(*epoch)
                    && self.account.connection.socket == Some(epoch.socket())
                    && self.account.connection.open
            }
            _ => true,
        }
    }
    pub(super) fn send_private(&mut self, cx: &mut Cx<'_, Pending>, request: Request) {
        if !self.current_token(cx, &request.kind) {
            return;
        }
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
                self.private_failed(cx, request, super::trading::not_sent(e));
            }
        }
    }
    pub(super) fn reject_auth(&self, cx: &mut Cx<'_, Pending>, error: &str) {
        cx.notice(Level::Error, code::AUTH_REJECTED, error);
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
        self.private_connect(cx);
        self.balance_read(cx);
        self.orders_read(cx);
        self.account.poll = Some(cx.set_poll_timer(10_000.));
    }
    pub(super) fn private_connect(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Err(error) = self.managed_connect(cx, true) {
            self.socket_error(cx, true, error);
        }
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
    pub(super) fn balance_read(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.account.balance_flight.is_empty() {
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
        if let Some(epoch) = self
            .account
            .connection
            .feed
            .and_then(|feed| cx.socket_epoch(feed))
        {
            self.request_private(
                cx,
                "/0/private/GetWebSocketsToken",
                vec![],
                Kind::Token { epoch },
            );
        }
        true
    }
    pub(super) fn private_close(&mut self, _cx: &mut Cx<'_, Pending>, socket: SocketId) -> bool {
        if self.account.connection.socket != Some(socket) {
            return false;
        }
        self.account.connection.socket = None;
        self.account.connection.open = false;
        self.account.revision += 1;
        self.account.balance_revision += 1;
        true
    }
    /// A catalogue re-read can list the route a funded currency was valued without.
    pub(super) fn revalue_after_catalogue(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account.wallet_valuation_pending |= cx.store.currency_balances().is_some();
        self.revalue_pending_balances(cx);
    }
    // Private balances can arrive before the first public quote seed. Revalue
    // those retained native holdings once, without changing later poll cadence.
    pub(super) fn revalue_pending_balances(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.account.wallet_valuation_pending {
            return;
        }
        let Some(currencies) = cx.store.currency_balances() else {
            return;
        };
        let mut total = 0.0;
        let mut missing = false;
        let mut estimated = false;
        let mut zeroed = false;
        let mut unsupported = false;
        let mut wanted = Vec::new();
        // A route a catalogue re-read just added waits for its ticker re-seed.
        let seeded = cx.store.loaded().tickers && !self.ticker_reseed;
        for holding in currencies {
            let rate = if matches!(holding.currency.as_str(), "USD" | "USDT" | "USDC") {
                Some(1.0)
            } else {
                let mut supported = false;
                let mut midpoint = None;
                let rate = cx
                    .store
                    .markets()
                    .iter()
                    .filter_map(|market| {
                        if market.base != holding.currency
                            || !matches!(market.quote.as_str(), "USD" | "USDT" | "USDC")
                        {
                            return None;
                        }
                        supported = true;
                        let ticker = cx.store.ticker(&market.symbol)?;
                        if midpoint.is_none() && ticker.bid > 0.0 && ticker.ask > 0.0 {
                            midpoint = Some((ticker.bid + ticker.ask) / 2.0)
                                .filter(|mid: &f64| mid.is_finite());
                        }
                        Some(ticker.last).filter(|last| last.is_finite() && *last > 0.0)
                    })
                    .next();
                if rate.is_some() {
                    rate
                } else if self.rejected_routes.contains(&holding.currency) {
                    // The malformed USD pair awaits its re-read; zero is not proof.
                    missing = true;
                    None
                } else if supported {
                    wanted.push(holding.currency.clone());
                    if !seeded {
                        missing = true;
                        None
                    } else {
                        // After the seed, BBO or zero stands in until a last arrives.
                        estimated = true;
                        zeroed |= midpoint.is_none();
                        midpoint
                    }
                } else {
                    unsupported = true;
                    None
                }
            };
            if let Some(rate) = rate {
                total += holding.balance.total * rate;
            }
        }
        for base in &wanted {
            cx.store.want_ticker_for_base(base);
        }
        // Each warning repeats only after its condition has cleared.
        if !unsupported {
            self.account.wallet_valuation_warnings &= !2;
        }
        if unsupported && self.account.wallet_valuation_warnings & 2 == 0 {
            self.account.wallet_valuation_warnings |= 2;
            cx.notice(Level::Warn, code::VALUATION_UNAVAILABLE,
                "Kraken Spot wallet retains unpriced native currency without a supported USD market");
        }
        if missing || !total.is_finite() {
            if self.account.wallet_valuation_warnings & 1 == 0 {
                self.account.wallet_valuation_warnings |= 1;
                cx.notice(Level::Warn, code::VALUATION_UNAVAILABLE,
                    "Kraken Spot USD wallet valuation awaits a valid quote; retaining native quantities and prior USD balance");
            }
            return;
        }
        self.account.wallet_valuation_warnings &= !1;
        if !zeroed {
            self.account.wallet_valuation_warnings &= !4;
        } else if self.account.wallet_valuation_warnings & 4 == 0 {
            self.account.wallet_valuation_warnings |= 4;
            cx.notice(Level::Warn, code::VALUATION_UNAVAILABLE,
                "Kraken Spot USD wallet values a currency without a quote at zero until one arrives");
        }
        // An estimated route keeps revaluing on live quotes; a complete valuation
        // waits for the next private poll.
        self.account.wallet_valuation_pending = estimated;
        cx.store.set_balance(Balance {
            total,
            free: total,
            used: 0.0,
            upnl: 0.0,
        });
    }
    fn balance_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        text: &str,
        revision: u64,
    ) -> Result<(), String> {
        let mut currencies = SnapshotBuilder::default();
        let mut skipped = vec![];
        map_each::<&RawValue>(text, |raw, value| {
            if raw.contains('.') {
                return;
            }
            // An unreadable amount keeps the stored balance of its asset.
            let Ok(Amount(amount)) = serde_json::from_str::<Amount>(value.get()) else {
                skipped.push(asset(&raw).to_owned());
                return;
            };
            if amount <= 0. {
                return;
            }
            currencies.push(CurrencyBalance {
                currency: asset(&raw).into(),
                balance: Balance {
                    total: amount,
                    free: amount,
                    used: 0.0,
                    upnl: 0.0,
                },
            });
        })
        .map_err(|_| "Kraken Spot invalid balance")?;
        for currency in skipped {
            let message = format!("Kraken Spot skipped malformed {currency} balance");
            self.account
                .log_skip(cx, format!("coin:{currency}"), Level::Warn, message);
            currencies.reject(Some(currency));
        }
        let current = revision == self.account.balance_revision;
        // An admitted stale read is re-read by the caller's stale-revision path.
        if self
            .account
            .balance_guard
            .admit_issued(current, revision, STALE_SNAPSHOT_LIMIT)
        {
            self.account.rearm_skips(
                "coin:",
                currencies.rejected() == 0,
                currencies
                    .rows()
                    .iter()
                    .map(|c| c.currency.as_str())
                    .filter(|c| currencies.allows_absence(&c.to_string())),
            );
            let stored = cx.store.currency_balances().unwrap_or_default().to_vec();
            let currencies = currencies.commit(stored, |c| c.currency.clone());
            cx.store.set_currency_balances(currencies);
            self.account.wallet_valuation_pending = true;
            self.revalue_pending_balances(cx);
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
        let mut rows = SnapshotBuilder::default();
        let mut provenance = HashMap::new();
        // The object key names every row, so a bad row keeps only its stored copy.
        let mut skipped = vec![];
        map_each::<&RawValue>(e.open.get(), |id, raw| {
            let mapped = serde_json::from_str::<RestOrder<'_>>(raw.get())
                .ok()
                .filter(|o| !id.is_empty() && o.vol > 0. && o.vol_exec >= 0. && o.vol_exec <= o.vol)
                .and_then(|o| {
                    let kind = order_kind(&o.descr.ordertype)?;
                    let side = match &*o.descr.side {
                        "buy" => OrderSide::Buy,
                        "sell" => OrderSide::Sell,
                        _ => return None,
                    };
                    Some((o, kind, side))
                });
            let Some((o, kind, side)) = mapped else {
                skipped.push(id.into_owned());
                return;
            };
            let symbol = self
                .identities
                .get(&*o.descr.pair)
                .cloned()
                .unwrap_or_else(|| o.descr.pair.to_string());
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
        for id in skipped {
            if id.is_empty() {
                let message = "Kraken Spot skipped order row without an id".to_owned();
                self.account
                    .log_skip(cx, "order:".into(), Level::Warn, message);
                rows.reject(None);
            } else {
                let message = format!("Kraken Spot skipped unmappable order {id}");
                self.account
                    .log_skip(cx, format!("order:{id}"), Level::Warn, message);
                rows.reject(Some(id));
            }
        }
        if revision == self.account.revision {
            self.account.rearm_skips(
                "order:",
                rows.rejected() == 0,
                rows.rows().iter().map(|o| o.id.as_str()),
            );
            let mut rows = rows.commit(cx.store.orders().iter().cloned(), |o| o.id.clone());
            for o in &rows {
                if let Some(kind) = self.account.provenance.get(&o.id)
                    && !provenance.contains_key(&o.id)
                {
                    provenance.insert(o.id.clone(), kind.clone());
                }
            }
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
        if !self.current_token(cx, &request.kind) {
            return;
        }
        let text = match result(response) {
            Ok(t) => t,
            Err(e) => {
                if e.contains("EAPI:Invalid signature") && request.signature_retries < 1 {
                    let mut r = request;
                    r.signature_retries += 1;
                    self.send_private(cx, r);
                    return;
                }
                // A key without trade permission still reads; only fail that write.
                let permission_on_write = e.contains("EGeneral:Permission denied")
                    && matches!(request.kind, Kind::Trade(_));
                if !permission_on_write
                    && (e.contains("EAPI:Invalid key")
                        || e.contains("EAPI:Invalid signature")
                        || e.contains("EGeneral:Permission denied")
                        || matches!(response.status, 401 | 403))
                {
                    self.reject_auth(cx, &e);
                }
                let error = super::trading::write_failure(response.status, response.body, e);
                self.private_failed(cx, request, error);
                return;
            }
        };
        let r = match &request.kind {
            Kind::Balance { revision } => {
                self.account.balance_flight.remove(revision);
                let r = self.balance_response(cx, text, *revision);
                if *revision != self.account.balance_revision
                    && self.account.balance_flight.is_empty()
                {
                    self.balance_read(cx);
                }
                r
            }
            Kind::Orders { revision } => {
                self.account.orders_flight = false;
                self.orders_response(cx, text, *revision)
            }
            Kind::Token { epoch } => {
                if cx.socket_epoch(epoch.feed()) != Some(*epoch)
                    || self.account.connection.socket != Some(epoch.socket())
                    || !self.account.connection.open
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
                            if let Err(error) = cx.send_after_socket_interests(
                                epoch.feed(),
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
                            ) {
                                self.socket_error(cx, true, error);
                                break;
                            }
                        }
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
            cx.notice(Level::Warn, code::REQUEST_FAILED, e);
        }
    }
    pub(super) fn private_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: safe_cex_engine::VenueError,
    ) {
        if !self.current_token(cx, &request.kind) {
            return;
        }
        let Kind::Trade(_) = request.kind else {
            let error = error.message.as_str();
            match request.kind {
                Kind::Balance { revision } => {
                    self.account.balance_flight.remove(&revision);
                }
                Kind::Orders { .. } => self.account.orders_flight = false,
                Kind::Token { epoch } => {
                    cx.retire_socket(
                        epoch.feed(),
                        safe_cex_engine::socket_lifecycle::RetireReason::Closed,
                    );
                }
                Kind::Trade(_) => unreachable!("trade failures return below"),
            }
            cx.notice(Level::Warn, code::REQUEST_FAILED, error);
            return;
        };
        cx.notice(Level::Warn, code::ORDER_REJECTED, &error.message);
        self.trade_failed(cx, error);
    }
    pub(super) fn private_network_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        error: &str,
    ) {
        if !self.current_token(cx, &request.kind) {
            return;
        }
        if !matches!(request.kind, Kind::Trade(_)) && request.retries < 3 {
            request.retries += 1;
            let timer = cx.set_timer(0.);
            self.account.retries.push((timer, request));
        } else {
            let error = safe_cex_engine::VenueError::new(error)
                .kind(safe_cex_engine::ErrorKind::Network)
                .outcome(safe_cex_engine::Outcome::Unknown);
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
            self.account.poll = Some(cx.set_poll_timer(10_000.));
            self.balance_read(cx);
            self.orders_read(cx);
            return true;
        }
        if self.account.orders_throttle == Some(timer) {
            self.account.orders_throttle = None;
            self.orders_read(cx);
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
        if !self.account.connection.open {
            return true;
        }
        let Ok(msg) = serde_json::from_str::<Ws<'_>>(text) else {
            return true;
        };
        if msg.success == Some(false) {
            cx.notice(
                Level::Warn,
                code::SUBSCRIPTION_REJECTED,
                "Kraken Spot private subscription failed",
            );
            if let Some(feed) = self.account.connection.feed {
                cx.retire_socket(
                    feed,
                    safe_cex_engine::socket_lifecycle::RetireReason::Closed,
                );
            }
            return true;
        }
        let Some(data) = msg.data else { return true };
        if msg.channel == "balances" {
            self.account.balance_revision += 1;
            self.balance_read(cx);
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
        // Order ids of unmappable rows; empty when the row names no order.
        let mut rejected: Vec<String> = vec![];
        let parsed = safe_cex_engine::json::each::<&RawValue>(data.get(), |raw| {
            let Ok(e) = serde_json::from_str::<Execution<'_>>(raw.get()) else {
                let row = serde_json::from_str::<ExecutionIdentity<'_>>(raw.get());
                rejected.push(row.unwrap_or_default().order_id.into_owned());
                return;
            };
            if e.order_id.is_empty() {
                rejected.push(String::new());
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
            if matches!(&*e.exec_type, "canceled" | "expired") {
                times.insert(e.order_id.to_string(), time);
                terminal.push(e.order_id.to_string());
                return;
            }
            if self.account.terminal.contains(&*e.order_id) && !snapshot {
                times.insert(e.order_id.to_string(), time);
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
                return;
            }
            let kind = order_kind(&e.order_type).or_else(|| existing.map(|o| o.kind));
            let Some(kind) = kind else {
                rejected.push(e.order_id.to_string());
                return;
            };
            let Some(amount) = e.order_qty.or_else(|| existing.map(|o| o.amount)) else {
                rejected.push(e.order_id.to_string());
                return;
            };
            let filled = e
                .cum_qty
                .or_else(|| existing.map(|o| o.filled))
                .unwrap_or(0.);
            if amount <= 0. || filled < 0. || filled > amount {
                rejected.push(e.order_id.to_string());
                return;
            }
            let side = match &*e.side {
                "buy" => OrderSide::Buy,
                "sell" => OrderSide::Sell,
                _ => {
                    let Some(side) = existing.map(|o| o.side) else {
                        rejected.push(e.order_id.to_string());
                        return;
                    };
                    side
                }
            };
            let trigger = e.triggers.as_ref().and_then(Triggers::effective);
            if trigger.is_some_and(|p| p <= 0.) {
                rejected.push(e.order_id.to_string());
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
            // A failed delivery must not consume the display baseline or row fences.
            let delivery = if !snapshot && e.exec_type == "trade" {
                let price = e.last_price.or(e.avg_price).unwrap_or(o.price);
                let report = match e.last_qty {
                    Some(amount) => Report::Last {
                        amount,
                        cumulative: e.cum_qty,
                        price,
                    },
                    None => Report::Cumulative {
                        quantity: filled,
                        price: CumulativePrice::Direct(price),
                    },
                };
                match Cursor::seed(existing.map_or(0., |o| o.filled), None)
                    .and_then(|cursor| cursor.prepare(report, Arithmetic::Binary))
                {
                    Ok(delivery) => delivery,
                    Err(_) => {
                        rejected.push(e.order_id.to_string());
                        return;
                    }
                }
            } else {
                None
            };
            times.insert(e.order_id.to_string(), time);
            if e.exec_type == "filled" || o.remaining == 0. {
                o.status = OrderStatus::Closed;
                terminal.push(o.id.clone());
            }
            if !e.order_type.is_empty() {
                provenance.insert(o.id.clone(), e.order_type.to_string());
            }
            // Only trade rows describe executions; filled rows are lifecycle updates.
            if let Some(delivery) = delivery {
                let key = if !e.exec_id.is_empty() {
                    e.exec_id.to_string()
                } else {
                    format!("{}:{filled}:{}", e.order_id, e.timestamp)
                };
                fills.push((
                    key,
                    Fill {
                        id: None,
                        order_id: o.id.clone(),
                        symbol,
                        side,
                        price: delivery.price(),
                        amount: delivery.amount(),
                    },
                ));
            }
            if let Some(index) = row_indices.get(&o.id).copied() {
                rows[index] = o;
            } else {
                row_indices.insert(o.id.clone(), rows.len());
                rows.push(o);
            }
        });
        if parsed.is_err() {
            cx.notice(
                Level::Warn,
                code::STREAM_MALFORMED,
                "Kraken Spot invalid execution frame",
            );
            return true;
        }
        // A rejected row is skipped alone: it emits no fill and changes no order.
        let mut orders = SnapshotBuilder::default();
        for id in rejected {
            self.execution_rejected(cx, &mut orders, id);
        }
        self.account.rearm_skips(
            "order:",
            snapshot && orders.rejected() == 0,
            rows.iter()
                .map(|o| o.id.as_str())
                .filter(|id| orders.allows_absence(&id.to_string())),
        );
        self.account.revision += 1;
        self.account.native_times.extend(times);
        if snapshot {
            for o in rows {
                orders.push(o);
            }
            // Rejected ids keep their stored copies.
            let mut rows = orders.commit(cx.store.orders().iter().cloned(), |o| o.id.clone());
            rows.retain(|o| {
                o.status == OrderStatus::Open && !self.account.terminal.contains(&o.id)
            });
            for o in &rows {
                if let Some(kind) = self.account.provenance.get(&o.id)
                    && !provenance.contains_key(&o.id)
                {
                    provenance.insert(o.id.clone(), kind.clone());
                }
            }
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
    /// Logs a skipped execution row once per order and records its id in `orders`.
    fn execution_rejected(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        orders: &mut SnapshotBuilder<String, Order>,
        id: String,
    ) {
        let stored = if id.is_empty() {
            !cx.store.orders().is_empty()
        } else {
            cx.store.order(&id).is_some()
        };
        let level = if stored { Level::Warn } else { Level::Error };
        let message = if id.is_empty() {
            "Kraken Spot skipped execution row without an order id".to_owned()
        } else {
            format!("Kraken Spot skipped unmappable execution for order {id}")
        };
        self.account
            .log_skip(cx, format!("order:{id}"), level, message);
        orders.reject((!id.is_empty()).then_some(id));
    }
}
