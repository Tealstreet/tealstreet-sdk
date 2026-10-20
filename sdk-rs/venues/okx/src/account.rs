//! Private streams and atomic REST reconciliation, guarded against newer pushes.
use crate::wire::*;
use crate::{Okx, Pending, Topic, scaled, usd};
use safe_cex_engine::model::*;
use safe_cex_engine::{Cx, Fill, LogLevel, Method, Response, TimerId, json, num};
use std::collections::{HashMap, HashSet};

pub const PRIVATE: usize = 1;
const ORDER_KINDS: [&str; 5] = ["", "oco", "conditional", "trigger", "move_order_stop"];

// Match the SDK's decimal arithmetic without reducing native coin precision.
fn decimal_result(a: f64, b: f64, result: f64) -> f64 {
    let factor = 10f64.powi(num::decimals(a).max(num::decimals(b)) as i32);
    let scaled = result * factor;
    let floor = scaled.floor();
    // Adding 0.5 first can change an already-integral f64 above 2^52.
    let rounded = if scaled - floor < 0.5 {
        floor
    } else {
        floor + 1.0
    };
    rounded / factor
}

fn add(a: f64, b: f64) -> f64 {
    decimal_result(a, b, a + b)
}

fn subtract(a: f64, b: f64) -> f64 {
    decimal_result(a, b, a - b)
}

fn reserve_leverage(value: &str) -> f64 {
    // Unproven leverage must reserve the full notional; infinity would erase it.
    value
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .unwrap_or(1.0)
        .max(1.0)
}

pub enum Request {
    Config,
    Balance {
        generation: u64,
        version: u64,
        equity: Vec<(String, f64)>,
        risk_rows: usize,
        invalid: bool,
    },
    Positions {
        generation: u64,
        version: u64,
        product: usize,
        rows: Vec<(Position, RiskValues)>,
        invalid: bool,
    },
    Orders {
        generation: u64,
        version: u64,
        kind: usize,
        product: usize,
        rows: Vec<Order>,
        margins: HashMap<String, (f64, String)>,
        page_count: usize,
        cursor: String,
        cursors: HashSet<String>,
        invalid: bool,
    },
}

#[derive(Clone, Default)]
pub struct RiskValues {
    imr: f64,
    upl: f64,
    currency: String,
    native_imr: f64,
    native_upl: f64,
}
struct Join {
    generation: u64,
    positions: Option<Vec<(Position, RiskValues)>>,
    equity: Option<Vec<(String, f64)>>,
    position_version: u64,
    balance_version: u64,
}

#[derive(Default)]
pub struct AccountState {
    pub ready: bool,
    configured: bool,
    account_level: u8,
    pub revision: u64,
    position_versions: HashMap<String, u64>,
    order_versions: HashMap<String, (u64, f64, bool)>,
    balance_version: u64,
    join: Option<Join>,
    generation: u64,
    orders_in_flight: bool,
    orders_generation: u64,
    authenticated_once: [bool; 3],
    risk: HashMap<String, RiskValues>,
    order_leverage: HashMap<String, (f64, String)>,
    equity: Vec<(String, f64)>,
    total_equity: Option<f64>,
    equity_loaded: bool,
    fills: HashMap<String, f64>,
    tick: Option<TimerId>,
    retry: Option<TimerId>,
}

impl Okx {
    pub(crate) fn account_after_settings(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account.join = None;
        self.account.generation += 1;
        self.account.revision += 1;
        self.reconcile(cx, false);
    }

    pub(crate) fn account_start(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.credentials.is_none() {
            return;
        }
        self.get(
            cx,
            "/api/v5/account/config",
            true,
            Pending::Account(Request::Config),
        );
    }

    pub(crate) fn get(
        &self,
        cx: &mut Cx<'_, Pending>,
        path: &str,
        private: bool,
        pending: Pending,
    ) {
        cx.request(
            Method::Get,
            format!("{}{path}", self.rest()),
            self.headers(cx.now, Method::Get, path, "", private),
            None,
            pending,
        );
    }

    pub(crate) fn account_refresh(&mut self, cx: &mut Cx<'_, Pending>) {
        self.reconcile(cx, true);
    }

    fn reconcile(&mut self, cx: &mut Cx<'_, Pending>, include_orders: bool) {
        if self.account.join.is_none() {
            let version = self.account.revision;
            self.account.generation += 1;
            let generation = self.account.generation;
            self.account.join = Some(Join {
                generation,
                positions: None,
                equity: None,
                position_version: version,
                balance_version: version,
            });
            self.list(
                cx,
                "/api/v5/account/account-position-risk?instType=SWAP",
                true,
                Pending::Account(Request::Balance {
                    generation,
                    version,
                    equity: Vec::new(),
                    risk_rows: 0,
                    invalid: false,
                }),
            );
            self.list(
                cx,
                "/api/v5/account/positions?instType=SWAP",
                true,
                Pending::Account(Request::Positions {
                    generation,
                    version,
                    product: 0,
                    rows: Vec::new(),
                    invalid: false,
                }),
            );
        }
        if include_orders && !self.account.orders_in_flight {
            self.account.orders_in_flight = true;
            self.account.orders_generation += 1;
            let generation = self.account.orders_generation;
            self.orders_page(
                cx,
                Request::Orders {
                    generation,
                    version: self.account.revision,
                    kind: 0,
                    product: 0,
                    rows: Vec::new(),
                    margins: HashMap::new(),
                    page_count: 0,
                    cursor: String::new(),
                    cursors: HashSet::new(),
                    invalid: false,
                },
            );
        }
    }

    fn orders_page(&self, cx: &mut Cx<'_, Pending>, request: Request) {
        let Request::Orders {
            kind,
            product,
            cursor,
            ..
        } = &request
        else {
            return;
        };
        let instrument_type = self.instrument_types()[*product].as_str();
        let mut path = if *kind == 0 {
            format!("/api/v5/trade/orders-pending?instType={instrument_type}")
        } else {
            format!(
                "/api/v5/trade/orders-algo-pending?instType={instrument_type}&ordType={}",
                ORDER_KINDS[*kind]
            )
        };
        if !cursor.is_empty() {
            path.push_str(&format!("&after={cursor}"));
        }
        self.list(cx, &path, true, Pending::Account(request));
    }

    pub(crate) fn login(&self, cx: &mut Cx<'_, Pending>, channel: usize) {
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Login<'a> {
            api_key: &'a str,
            passphrase: &'a str,
            timestamp: String,
            sign: String,
        }
        #[derive(serde::Serialize)]
        struct Payload<'a> {
            op: &'static str,
            args: [Login<'a>; 1],
        }
        if let (Some(c), Some(socket)) = (&self.credentials, self.connections[channel].socket) {
            let timestamp = num::format((cx.now / 1000.0).floor());
            let sign = c.signature(&timestamp, "GET", "/users/self/verify", "");
            let payload = Payload {
                op: "login",
                args: [Login {
                    api_key: &c.key,
                    passphrase: &c.passphrase,
                    timestamp,
                    sign,
                }],
            };
            cx.send(socket, serde_json::to_string(&payload).expect("login"));
        }
    }

    pub(crate) fn subscribe_private(&self, cx: &mut Cx<'_, Pending>, channel: usize) {
        let mut args = Vec::new();
        if channel == PRIVATE {
            args.push(Topic {
                channel: "account",
                inst_id: None,
                inst_type: None,
            });
        }
        for kind in self.instrument_types() {
            let channels: &[&str] = if channel == PRIVATE {
                &["positions", "orders"]
            } else {
                &["orders-algo", "algo-advance"]
            };
            for name in channels {
                args.push(Topic {
                    channel: name,
                    inst_id: None,
                    inst_type: Some(kind.as_str()),
                });
            }
        }
        self.topics(cx, channel, "subscribe", &args);
    }

    fn account_request_current(&self, request: &Request) -> bool {
        match request {
            Request::Balance { generation, .. } | Request::Positions { generation, .. } => self
                .account
                .join
                .as_ref()
                .is_some_and(|join| join.generation == *generation),
            Request::Orders { generation, .. } => self.account.orders_generation == *generation,
            Request::Config => true,
        }
    }

    fn account_scope(&self, product: usize, inst_id: &str, supplied_type: &str) -> bool {
        let expected = self.instrument_types()[product].as_str();
        (supplied_type.is_empty() || supplied_type == expected)
            && self
                .products
                .get(inst_id)
                .is_none_or(|instrument| instrument.kind.as_str() == expected)
    }

    fn order_margin_currency(&self, row: &OrderRow<'_>) -> String {
        if row.ccy.is_empty() {
            self.products
                .get(row.inst_id)
                .map_or("", |instrument| instrument.settle_ccy.as_str())
                .into()
        } else {
            row.ccy.into()
        }
    }

    pub(crate) fn account_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        if !self.account_request_current(request) {
            return;
        }
        match request {
            Request::Config => {}
            Request::Balance {
                equity,
                risk_rows,
                invalid,
                ..
            } => match serde_json::from_str::<Risk<'_>>(item) {
                Ok(r) => {
                    *risk_rows += 1;
                    if *risk_rows != 1 {
                        *invalid = true;
                    }
                    for e in r.bal_data {
                        if let Ok(eq) = e.eq.parse::<f64>()
                            && eq.is_finite()
                            && !e.ccy.is_empty()
                        {
                            equity.push((e.ccy.into(), eq));
                        } else {
                            *invalid = true;
                        }
                    }
                }
                Err(_) => *invalid = true,
            },
            Request::Positions {
                product,
                rows,
                invalid,
                ..
            } => match serde_json::from_str::<PositionRow<'_>>(item) {
                Ok(p) => {
                    if !self.account_scope(*product, p.inst_id, p.inst_type) {
                        *invalid = true;
                        return;
                    }
                    if let Some(row) = self.position(cx, &p) {
                        rows.push(row);
                    } else if p.inst_id.is_empty() || cx.store.market_by_id(p.inst_id).is_some() {
                        *invalid = true;
                    }
                }
                Err(_) => *invalid = true,
            },
            Request::Orders {
                product,
                rows,
                margins,
                page_count,
                cursor,
                invalid,
                ..
            } => match serde_json::from_str::<OrderRow<'_>>(item) {
                Ok(o) => {
                    *page_count += 1;
                    *cursor = if o.algo_id.is_empty() {
                        o.ord_id.into()
                    } else {
                        o.algo_id.into()
                    };
                    if !self.account_scope(*product, o.inst_id, o.inst_type)
                        || o.inst_id.is_empty()
                        || (cx.store.market_by_id(o.inst_id).is_some() && !valid_order(&o))
                    {
                        *invalid = true;
                        return;
                    }
                    for order in self.orders(cx, &o) {
                        margins.insert(
                            order.id.clone(),
                            (reserve_leverage(o.lever), self.order_margin_currency(&o)),
                        );
                        rows.push(order);
                    }
                }
                Err(_) => *invalid = true,
            },
        }
    }

    pub(crate) fn account_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        if !self.account_request_current(&request) {
            return;
        }
        let data = match envelope(response.body) {
            Ok(data) => data,
            Err(e) => {
                self.account_failed(cx, request, &e);
                return;
            }
        };
        match request {
            Request::Config => {
                let configs = serde_json::from_str::<Vec<Config<'_>>>(data);
                let Ok(configs) = configs else {
                    self.account_failed(cx, Request::Config, "malformed OKX config");
                    return;
                };
                let Some(config) = configs
                    .first()
                    .filter(|c| matches!(c.pos_mode, "net_mode" | "long_short_mode"))
                else {
                    self.account_failed(cx, Request::Config, "missing OKX position mode");
                    return;
                };
                cx.store.set_hedged(config.pos_mode == "long_short_mode");
                self.account.account_level = config.acct_lv.parse().unwrap_or(2);
                self.account.configured = true;
                self.connect(cx, PRIVATE);
                self.account_refresh(cx);
                if self.account.tick.is_none() {
                    self.account.tick = Some(cx.set_timer(2_000.0));
                }
            }
            Request::Balance {
                generation,
                version,
                equity,
                risk_rows: 1,
                invalid: false,
            } => {
                if let Some(join) = &mut self.account.join
                    && join.generation == generation
                {
                    join.equity = Some(equity);
                    join.balance_version = version;
                }
                self.finish_join(cx);
            }
            Request::Positions {
                generation,
                version,
                product,
                rows,
                invalid: false,
            } => {
                let product = product + 1;
                if let Some(kind) = self.instrument_types().get(product) {
                    self.list(
                        cx,
                        &format!("/api/v5/account/positions?instType={}", kind.as_str()),
                        true,
                        Pending::Account(Request::Positions {
                            generation,
                            version,
                            product,
                            rows,
                            invalid: false,
                        }),
                    );
                    return;
                }
                if let Some(join) = &mut self.account.join
                    && join.generation == generation
                {
                    join.positions = Some(rows);
                    join.position_version = version;
                }
                self.finish_join(cx);
            }
            Request::Orders {
                generation,
                version,
                mut kind,
                mut product,
                rows,
                margins,
                page_count,
                cursor,
                mut cursors,
                invalid: false,
            } => {
                if page_count >= 100 {
                    if cursor.is_empty() || cursors.len() >= 200 || !cursors.insert(cursor.clone())
                    {
                        self.account.orders_in_flight = false;
                        self.account_failed(
                            cx,
                            Request::Config,
                            "OKX order paging did not advance",
                        );
                        return;
                    }
                    self.orders_page(
                        cx,
                        Request::Orders {
                            generation,
                            version,
                            kind,
                            product,
                            rows,
                            margins,
                            page_count: 0,
                            cursor,
                            cursors,
                            invalid: false,
                        },
                    );
                    return;
                }
                kind += 1;
                if kind == ORDER_KINDS.len() {
                    product += 1;
                    kind = 0;
                }
                if product < self.instrument_types().len() {
                    self.orders_page(
                        cx,
                        Request::Orders {
                            generation,
                            version,
                            kind,
                            product,
                            rows,
                            margins,
                            page_count: 0,
                            cursor: String::new(),
                            cursors: HashSet::new(),
                            invalid: false,
                        },
                    );
                    return;
                }
                self.account.orders_in_flight = false;
                let mut rows: Vec<_> = rows
                    .into_iter()
                    .filter(|o| {
                        !self
                            .account
                            .order_versions
                            .get(&o.id)
                            .is_some_and(|(v, _, terminal)| *v > version || *terminal)
                    })
                    .collect();
                rows.extend(
                    cx.store
                        .orders()
                        .iter()
                        .filter(|o| {
                            self.account
                                .order_versions
                                .get(&o.id)
                                .is_some_and(|(v, _, _)| *v > version)
                        })
                        .cloned(),
                );
                for (id, leverage) in margins {
                    if !self
                        .account
                        .order_versions
                        .get(&id)
                        .is_some_and(|(v, _, _)| *v > version)
                    {
                        self.account.order_leverage.insert(id, leverage);
                    }
                }
                cx.store.set_orders(rows);
                self.account.ready = cx.store.loaded().positions && cx.store.loaded().balance;
                let ids: HashSet<_> = cx.store.orders().iter().map(|o| o.id.as_str()).collect();
                self.account
                    .order_leverage
                    .retain(|id, _| ids.contains(id.as_str()));
                self.account.order_versions.retain(|id, (v, _, terminal)| {
                    *v > version || *terminal || ids.contains(id.as_str())
                });
                self.recalculate(cx);
            }
            other => self.account_failed(cx, other, "malformed OKX account list"),
        }
    }

    fn finish_join(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self
            .account
            .join
            .as_ref()
            .is_some_and(|j| j.positions.is_some() && j.equity.is_some())
        {
            return;
        }
        let join = self.account.join.take().expect("join ready");
        let mut positions = Vec::new();
        let rows = join.positions.expect("positions ready");
        let changed: HashSet<_> = self
            .account
            .position_versions
            .iter()
            .filter(|(_, v)| **v > join.position_version)
            .map(|(id, _)| id.clone())
            .collect();
        self.account.risk.retain(|id, _| changed.contains(id));
        for (position, risk) in rows {
            if changed.contains(&position.id) {
                continue;
            }
            Self::setting(cx, &position);
            self.account.risk.insert(position.id.clone(), risk);
            if position.contracts > 0.0 {
                positions.push(position);
            }
        }
        positions.extend(
            cx.store
                .positions()
                .iter()
                .filter(|p| changed.contains(&p.id))
                .cloned(),
        );
        cx.store.set_positions(positions);
        if self.account.balance_version <= join.balance_version {
            self.account.equity = join.equity.expect("equity ready");
            self.account.total_equity = None;
        }
        self.account.equity_loaded = true;
        self.recalculate(cx);
        self.account.ready = cx.store.loaded().orders;
    }

    pub(crate) fn account_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        if !self.account_request_current(&request) {
            return;
        }
        match request {
            Request::Balance { generation, .. } | Request::Positions { generation, .. } => {
                if self
                    .account
                    .join
                    .as_ref()
                    .is_some_and(|j| j.generation == generation)
                {
                    self.account.join = None;
                }
            }
            Request::Orders { .. } => self.account.orders_in_flight = false,
            Request::Config => {}
        }
        cx.log(LogLevel::Error, error);
        if self.account.retry.is_none() {
            self.account.retry = Some(cx.set_timer(5_000.0));
        }
    }

    pub(crate) fn account_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if self.account.tick == Some(timer) {
            self.account.tick = Some(cx.set_timer(2_000.0));
            self.recalculate(cx);
            self.account.fills.retain(|_, at| cx.now - *at < 120_000.0);
            self.account
                .order_versions
                .retain(|_, (_, at, terminal)| !*terminal || cx.now - *at < 600_000.0);
            return true;
        }
        if self.account.retry == Some(timer) {
            self.account.retry = None;
            if self.account.configured {
                self.account_refresh(cx);
            } else {
                self.account_start(cx);
            }
            return true;
        }
        false
    }

    fn setting(cx: &mut Cx<'_, Pending>, position: &Position) {
        cx.store.update_setting(&position.symbol, |s| {
            s.leverage = position.leverage;
            s.margin_type = position.margin_type;
        });
    }

    fn recalculate(&self, cx: &mut Cx<'_, Pending>) {
        if !self.account.equity_loaded {
            return;
        }
        let collateral = usd(self.account.total_equity.unwrap_or_else(|| {
            self.account
                .equity
                .iter()
                .map(|(ccy, eq)| {
                    let candidates = [
                        format!("{ccy}-USDT-SWAP"),
                        format!("{ccy}-USD-SWAP"),
                        format!("{ccy}USDT"),
                        format!("{ccy}USD"),
                    ];
                    let mark = cx
                        .store
                        .tickers()
                        .iter()
                        .find(|t| candidates.contains(&t.id) || candidates.contains(&t.symbol))
                        .map_or(1.0, |t| t.mark);
                    eq * mark
                })
                .sum()
        }));
        let used = usd(self.account.risk.values().map(|p| p.imr).sum());
        let upnl = usd(self.account.risk.values().map(|p| p.upl).sum());
        let orders = usd(cx
            .store
            .orders()
            .iter()
            .filter(|o| {
                o.status == OrderStatus::Open && o.kind == OrderType::Limit && !o.reduce_only
            })
            .map(|o| {
                o.remaining * o.price
                    / self
                        .account
                        .order_leverage
                        .get(&o.id)
                        .map(|(leverage, _)| *leverage)
                        .unwrap_or(1.0)
            })
            .sum::<f64>()
            * 1.1);
        cx.store.set_balance(Balance {
            used,
            free: usd(collateral - used - orders),
            total: usd(collateral - upnl),
            upnl,
        });
        let currencies = if self.account.account_level == 2 {
            self.account
                .equity
                .iter()
                .map(|(currency, equity)| {
                    let risks = || {
                        cx.store
                            .positions()
                            .iter()
                            .filter_map(|p| self.account.risk.get(&p.id))
                            .filter(|p| &p.currency == currency)
                    };
                    let native_used = risks().map(|p| p.native_imr).fold(0.0, add);
                    let native_upnl = risks().map(|p| p.native_upl).fold(0.0, add);
                    let native_used = cx
                        .store
                        .orders()
                        .iter()
                        .filter(|o| {
                            o.status == OrderStatus::Open
                                && o.kind == OrderType::Limit
                                && !o.reduce_only
                        })
                        .filter_map(|o| {
                            let m = cx.store.market(&o.symbol)?;
                            let (leverage, margin_currency) =
                                self.account.order_leverage.get(&o.id)?;
                            if margin_currency != currency {
                                return None;
                            }
                            let mut used = o.remaining * o.price / leverage * 1.1;
                            if m.is_inverse == Some(true) {
                                let px = self.reference_price(cx, m, o.price);
                                used = if px > 0.0 { used / px } else { 0.0 };
                            }
                            Some(used)
                        })
                        .fold(native_used, add);
                    CurrencyBalance {
                        currency: currency.clone(),
                        balance: Balance {
                            used: native_used,
                            free: subtract(*equity, native_used),
                            total: subtract(*equity, native_upnl),
                            upnl: native_upnl,
                        },
                    }
                })
                .collect()
        } else {
            Vec::new()
        };
        cx.store.set_currency_balances(currencies);
    }

    fn execution(&mut self, cx: &mut Cx<'_, Pending>, row: &OrderRow<'_>) {
        if !matches!(row.state, "filled" | "partially_filled") {
            return;
        }
        let (Ok(size), Ok(price)) = (row.fill_sz.parse::<f64>(), row.fill_px.parse::<f64>()) else {
            return;
        };
        if !size.is_finite() || size <= 0.0 || !price.is_finite() || price <= 0.0 {
            return;
        }
        let Some(m) = cx.store.market_by_id(row.inst_id) else {
            return;
        };
        let amount = self.contracts_to_base(cx, m, size, price);
        if !amount.is_finite() || amount <= 0.0 {
            return;
        }
        let key = if !row.trade_id.is_empty() && row.trade_id != "0" {
            format!("{}:{}:{}", row.inst_id, row.ord_id, row.trade_id)
        } else {
            format!(
                "{}:{}:{}:{}:{}",
                row.inst_id,
                row.ord_id,
                if row.fill_time.is_empty() {
                    row.u_time
                } else {
                    row.fill_time
                },
                row.fill_px,
                row.fill_sz
            )
        };
        if let std::collections::hash_map::Entry::Vacant(entry) = self.account.fills.entry(key) {
            entry.insert(cx.now);
            cx.fill(Fill {
                order_id: row.ord_id.into(),
                symbol: m.symbol.clone(),
                side: side(row.side),
                price,
                amount,
            });
        }
    }

    pub(crate) fn private_message(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        channel: usize,
        message: Message<'_>,
    ) {
        if message.event == "login" {
            if message.code == "0" {
                let reopened = self.account.authenticated_once[channel];
                self.account.authenticated_once[channel] = true;
                self.connections[channel].authenticated = true;
                self.connections[channel].auth_deadline = None;
                self.subscribe_private(cx, channel);
                self.subscribe(cx, channel);
                if self.account.configured {
                    if reopened {
                        self.account.join = None;
                        self.account.generation += 1;
                        self.account.orders_generation += 1;
                        self.account.orders_in_flight = false;
                        self.account.revision += 1;
                    }
                    self.account_refresh(cx);
                }
            } else {
                if let Some(socket) = self.connections[channel].socket {
                    cx.close(socket);
                    safe_cex_engine::Venue::on_close(self, cx, socket, 1008);
                }
                cx.log(
                    LogLevel::Error,
                    format!("OKX login {}: {}", message.code, message.msg),
                );
            }
            return;
        }
        if !self.connections[channel].authenticated {
            return;
        }
        let Some(data) = message.data else {
            return;
        };
        match message.arg.channel {
            "orders" | "orders-algo" | "algo-advance" => {
                let _ = json::each::<OrderRow<'_>>(data.get(), |row| {
                    if !valid_order(&row) {
                        return;
                    }
                    // Finality protects order state, not execution delivery order.
                    self.execution(cx, &row);
                    let terminal = matches!(
                        row.state,
                        "filled" | "canceled" | "mmp_canceled" | "order_failed"
                    );
                    let triggered = matches!(row.state, "effective" | "partially_effective");
                    let native_id = if row.algo_id.is_empty() {
                        row.ord_id
                    } else {
                        row.algo_id
                    };
                    if !terminal
                        && !triggered
                        && self
                            .account
                            .order_versions
                            .get(native_id)
                            .is_some_and(|(_, _, final_state)| *final_state)
                    {
                        return;
                    }
                    self.account.revision += 1;
                    self.account.order_versions.insert(
                        native_id.into(),
                        (self.account.revision, cx.now, terminal || triggered),
                    );
                    for order in self.orders(cx, &row) {
                        if terminal || triggered {
                            self.account.order_leverage.remove(&order.id);
                        }
                        self.account.order_versions.insert(
                            order.id.clone(),
                            (self.account.revision, cx.now, terminal || triggered),
                        );
                        if terminal {
                            cx.store.set_order_status(
                                &order.id,
                                if row.state == "filled" {
                                    OrderStatus::Closed
                                } else {
                                    OrderStatus::Canceled
                                },
                            );
                        } else if triggered {
                            cx.store.remove_orders(&[&order.id]);
                        } else if matches!(row.state, "live" | "partially_filled" | "pause") {
                            self.account.order_leverage.insert(
                                order.id.clone(),
                                (
                                    reserve_leverage(row.lever),
                                    self.order_margin_currency(&row),
                                ),
                            );
                            cx.store.upsert_order(order);
                        }
                    }
                });
            }
            "positions" => {
                let mut valid = false;
                let _ = json::each::<PositionRow<'_>>(data.get(), |row| {
                    if let Some((p, risk)) = self.position(cx, &row) {
                        valid = true;
                        self.account.revision += 1;
                        self.account
                            .position_versions
                            .insert(p.id.clone(), self.account.revision);
                        Self::setting(cx, &p);
                        if p.contracts == 0.0 {
                            self.account.risk.remove(&p.id);
                        } else {
                            self.account.risk.insert(p.id.clone(), risk);
                        }
                        cx.store.upsert_position(p);
                    }
                });
                if !valid {
                    return;
                }
                self.recalculate(cx);
            }
            "account" => {
                let mut equity = 0.0;
                let mut count = 0;
                let mut details: Vec<(String, f64)> = Vec::new();
                let mut invalid = false;
                let decoded = json::each::<AccountPush<'_>>(data.get(), |row| {
                    let Ok(total) = row.total_eq.parse::<f64>() else {
                        invalid = true;
                        return;
                    };
                    if !total.is_finite() {
                        invalid = true;
                        return;
                    }
                    equity += total;
                    count += 1;
                    for detail in row.details {
                        let Ok(value) = detail.eq.parse::<f64>() else {
                            invalid = true;
                            continue;
                        };
                        if !value.is_finite()
                            || detail.ccy.is_empty()
                            || details.iter().any(|(ccy, _)| ccy == detail.ccy)
                        {
                            invalid = true;
                            continue;
                        }
                        details.push((detail.ccy.into(), value));
                    }
                });
                if decoded.is_err() || invalid || count == 0 || !equity.is_finite() {
                    return;
                }
                for (ccy, equity) in details {
                    if let Some((_, current)) = self
                        .account
                        .equity
                        .iter_mut()
                        .find(|(currency, _)| currency == &ccy)
                    {
                        *current = equity;
                    } else {
                        self.account.equity.push((ccy, equity));
                    }
                }
                self.account.revision += 1;
                self.account.balance_version = self.account.revision;
                self.account.total_equity = Some(equity);
                self.account.equity_loaded = true;
                self.recalculate(cx);
            }
            _ => {}
        }
    }

    fn position(
        &self,
        cx: &Cx<'_, Pending>,
        p: &PositionRow<'_>,
    ) -> Option<(Position, RiskValues)> {
        let m = cx.store.market_by_id(p.inst_id)?;
        let pos = p.pos.parse::<f64>().ok().filter(|v| v.is_finite())?;
        let finite = |value: &str| value.parse::<f64>().ok().filter(|value| value.is_finite());
        if (!p.imr.is_empty() && finite(p.imr).is_none_or(|value| value < 0.0))
            || (!p.upl.is_empty() && finite(p.upl).is_none())
        {
            return None;
        }
        let (initial_margin, unrealized) = if pos == 0.0 {
            (0.0, 0.0)
        } else {
            // Isolated positions omit IMR and report their locked margin instead.
            // An open row with unavailable risk cannot replace proven account data.
            let initial = if p.imr.is_empty() && p.mgn_mode == "isolated" {
                finite(p.margin).filter(|value| *value >= 0.0)?
            } else {
                finite(p.imr)?
            };
            (initial, finite(p.upl)?)
        };
        let side = if p.pos_side == "short" || (p.pos_side == "net" && pos < 0.0) {
            PositionSide::Short
        } else {
            PositionSide::Long
        };
        let id = if cx.store.hedged() {
            format!(
                "{}:{}",
                m.symbol,
                if side == PositionSide::Long {
                    "long"
                } else {
                    "short"
                }
            )
        } else {
            m.symbol.clone()
        };
        let mark = cx.store.ticker(&m.symbol).map_or(0.0, |t| t.mark);
        let reference = if mark > 0.0 {
            mark
        } else if num::parse(p.mark_px) > 0.0 {
            num::parse(p.mark_px)
        } else {
            num::parse(p.avg_px)
        };
        let contracts = self.contracts_to_base(cx, m, pos.abs(), reference);
        if pos != 0.0 && contracts <= 0.0 {
            return None;
        }
        let margin_currency = if p.ccy.is_empty() {
            self.products
                .get(&m.id)
                .map_or("", |i| i.settle_ccy.as_str())
        } else {
            p.ccy
        };
        let margin_factor =
            if m.is_inverse == Some(true) && margin_currency.eq_ignore_ascii_case(&m.base) {
                reference
            } else {
                1.0
            };
        let native_imr = initial_margin;
        let native_upl = unrealized;
        let initial_margin = if pos == 0.0 {
            0.0
        } else {
            initial_margin * margin_factor
        };
        let unrealized = if pos == 0.0 {
            0.0
        } else {
            unrealized * margin_factor
        };
        if !initial_margin.is_finite() || !unrealized.is_finite() {
            return None;
        }
        let leverage = if p.lever.is_empty() {
            cx.store.setting(&m.symbol).map_or(0.0, |s| s.leverage)
        } else {
            num::parse(p.lever)
        };
        Some((
            Position {
                id,
                symbol: m.symbol.clone(),
                side,
                entry_price: if pos == 0.0 {
                    0.0
                } else {
                    num::parse(p.avg_px)
                },
                notional: if m.is_inverse == Some(true) {
                    scaled(pos.abs(), m.contract_size.unwrap_or(1.0))
                } else {
                    pos.abs() * mark * m.contract_size.unwrap_or(1.0)
                },
                leverage,
                margin_type: Some(if p.mgn_mode == "cross" {
                    MarginType::Cross
                } else {
                    MarginType::Isolated
                }),
                position_mode: Some(if cx.store.hedged() {
                    PositionMode::Hedged
                } else {
                    PositionMode::Oneway
                }),
                unrealized_pnl: if pos == 0.0 {
                    0.0
                } else {
                    num::adjust(unrealized, m.precision.price)
                },
                realized_pnl: None,
                contracts,
                liquidation_price: num::parse(p.liq_px),
                margin_size: None,
                last_update: None,
            },
            RiskValues {
                imr: initial_margin,
                upl: unrealized,
                currency: margin_currency.into(),
                native_imr,
                native_upl,
            },
        ))
    }

    fn orders(&self, cx: &Cx<'_, Pending>, o: &OrderRow<'_>) -> Vec<Order> {
        let Some(m) = cx.store.market_by_id(o.inst_id) else {
            return Vec::new();
        };
        let native = num::parse(o.sz).abs();
        let filled = num::parse(o.acc_fill_sz);
        let reference = if matches!(
            o.ord_type,
            "trigger" | "oco" | "conditional" | "move_order_stop"
        ) {
            [
                o.last,
                o.px,
                o.ord_px,
                o.order_px,
                o.trigger_px,
                o.sl_trigger_px,
                o.tp_trigger_px,
            ]
        } else {
            [o.px, o.avg_px, o.fill_px, o.last, "", "", ""]
        }
        .into_iter()
        .map(num::parse)
        .find(|p| *p > 0.0)
        .unwrap_or_else(|| {
            cx.store
                .ticker(&m.symbol)
                .map_or(0.0, |t| if t.mark > 0.0 { t.mark } else { t.last })
        });
        let mut legs = Vec::new();
        match o.ord_type {
            "trigger" => {
                let trigger = trigger(o.trigger_px_type);
                let reference = cx.store.ticker(&m.symbol).map_or(0.0, |t| match trigger {
                    StopTriggerType::MarkPrice => {
                        if self.marks.contains(&m.id) {
                            t.mark
                        } else {
                            0.0
                        }
                    }
                    StopTriggerType::IndexPrice => {
                        if self.indexes.contains(&m.id) {
                            t.index
                        } else {
                            0.0
                        }
                    }
                    StopTriggerType::LastPrice => t.last,
                });
                let price = num::parse(o.trigger_px);
                let kind = if o.reduce_only && reference > 0.0 && price > 0.0 {
                    if (o.side == "sell" && price > reference)
                        || (o.side == "buy" && price < reference)
                    {
                        OrderType::TakeProfit
                    } else {
                        OrderType::StopLoss
                    }
                } else {
                    OrderType::Conditional
                };
                let suffix = match kind {
                    OrderType::StopLoss => "_sl",
                    OrderType::TakeProfit => "_tp",
                    _ => "",
                };
                legs.push((kind, suffix, price, Some(trigger)));
            }
            "oco" | "conditional" => {
                if num::parse(o.sl_trigger_px) > 0.0 {
                    legs.push((
                        OrderType::StopLoss,
                        "_sl",
                        num::parse(o.sl_trigger_px),
                        Some(trigger(o.sl_trigger_px_type)),
                    ));
                }
                if num::parse(o.tp_trigger_px) > 0.0 {
                    legs.push((
                        OrderType::TakeProfit,
                        "_tp",
                        num::parse(o.tp_trigger_px),
                        Some(trigger(o.tp_trigger_px_type)),
                    ));
                }
            }
            "move_order_stop" => {
                let spread = num::parse(o.callback_spread);
                let position = cx.store.positions().iter().find(|p| {
                    p.symbol == m.symbol && (o.side == "sell") == (p.side == PositionSide::Long)
                });
                let movement = position.map_or(num::parse(o.move_trigger_px), |p| {
                    -(p.entry_price - spread).abs()
                });
                let price = num::adjust(
                    num::parse(o.last) + if o.side == "buy" { -movement } else { movement },
                    m.precision.price,
                );
                legs.push((
                    OrderType::TrailingStopLoss,
                    "_tsl",
                    if price == 0.0 { spread } else { price },
                    None,
                ));
            }
            _ => legs.push((
                if matches!(o.ord_type, "market" | "optimal_limit_ioc") {
                    OrderType::Market
                } else {
                    OrderType::Limit
                },
                "",
                num::parse(o.px),
                None,
            )),
        }
        let algo = !matches!(
            o.ord_type,
            "market" | "limit" | "post_only" | "fok" | "ioc" | "optimal_limit_ioc"
        );
        legs.into_iter()
            .map(|(kind, suffix, price, trigger_type)| {
                let trailing = kind == OrderType::TrailingStopLoss;
                let remaining_native = if trailing {
                    0.0
                } else {
                    native - if algo { 0.0 } else { filled }
                };
                Order {
                    id: format!("{}{suffix}", if algo { o.algo_id } else { o.ord_id }),
                    parent_id: None,
                    status: OrderStatus::Open,
                    symbol: m.symbol.clone(),
                    kind,
                    side: side(o.side),
                    price,
                    amount: if trailing { 0.0 } else { native },
                    filled: if algo {
                        0.0
                    } else {
                        self.contracts_to_base(cx, m, filled, reference)
                    },
                    remaining: self.contracts_to_base(cx, m, remaining_native, reference),
                    reduce_only: trailing || o.reduce_only,
                    close: None,
                    client_order_id: if algo {
                        (!o.algo_cl_ord_id.is_empty()).then(|| o.algo_cl_ord_id.into())
                    } else {
                        (!o.cl_ord_id.is_empty()).then(|| o.cl_ord_id.into())
                    },
                    time_in_force: Some(if o.ord_type == "post_only" {
                        TimeInForce::PostOnly
                    } else {
                        TimeInForce::GoodTillCancel
                    }),
                    last_update: cx.now,
                    trigger_type,
                    take_profit: None,
                    stop_loss: None,
                    extra: Some(OrderExtra {
                        remaining_native: Some(remaining_native),
                        filled_in_placement_units: (!algo).then_some(filled),
                        conditional_order_price: if o.ord_type == "trigger" {
                            let p = num::parse(if o.ord_px.is_empty() {
                                o.order_px
                            } else {
                                o.ord_px
                            });
                            (p > 0.0).then_some(p)
                        } else if matches!(o.ord_type, "oco" | "conditional") {
                            let p = num::parse(if kind == OrderType::StopLoss {
                                o.sl_ord_px
                            } else {
                                o.tp_ord_px
                            });
                            (p != 0.0).then_some(p)
                        } else {
                            None
                        },
                        ..OrderExtra::default()
                    }),
                }
            })
            .collect()
    }
}

pub(crate) fn side(side: &str) -> OrderSide {
    if side == "sell" {
        OrderSide::Sell
    } else {
        OrderSide::Buy
    }
}
pub(crate) fn trigger(value: &str) -> StopTriggerType {
    match value {
        "mark" => StopTriggerType::MarkPrice,
        "index" => StopTriggerType::IndexPrice,
        _ => StopTriggerType::LastPrice,
    }
}

fn valid_order(o: &OrderRow<'_>) -> bool {
    let algo = matches!(
        o.ord_type,
        "trigger" | "conditional" | "oco" | "move_order_stop"
    );
    if !matches!(o.side, "buy" | "sell") || (if algo { o.algo_id } else { o.ord_id }).is_empty() {
        return false;
    }
    if algo && o.sz.is_empty() {
        return true;
    }
    o.sz.parse::<f64>().is_ok_and(|n| n.is_finite() && n >= 0.0)
        && (algo
            || o.acc_fill_sz
                .parse::<f64>()
                .is_ok_and(|n| n.is_finite() && n >= 0.0))
}
