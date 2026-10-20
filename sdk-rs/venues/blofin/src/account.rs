//! Atomic private snapshots, protected from newer stream and command updates.
use super::*;
use safe_cex_engine::Fill;
use safe_cex_engine::model::*;
use safe_cex_engine::socket::{Config, Supervisor, Tick as SocketTick};
use serde::Deserialize;
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Clone, Default)]
pub struct Source {
    execution_baseline: (f64, Option<f64>),
    pub inst_id: String,
    pub margin_mode: String,
    pub position_side: String,
    pub order_id: String,
    pub tpsl_id: String,
    pub algo_id: String,
    pub trigger_price_type: String,
    pub order_price: f64,
    pub tp: f64,
    pub sl: f64,
    pub tp_type: String,
    pub sl_type: String,
    pub tp_order_price: f64,
    pub sl_order_price: f64,
}
// BloFin reports cumulative quantities/value, not individual execution IDs.
// Keep early observations until the first complete REST census proves a prefix.
struct Execution {
    fill: Fill,
    qty: f64,
    value: f64,
    contract: f64,
    updated: f64,
}

#[derive(Default)]
pub(crate) struct Account {
    pub(crate) supervisor: Supervisor,
    authenticated: bool,
    pub connection: Connection,
    pub margin: Option<MarginType>,
    pub mode_loaded: bool,
    pub orders_revision: u64,
    pub positions_revision: u64,
    pub balance_revision: u64,
    pub settings_revision: u64,
    orders_busy: bool,
    startup_started: bool,
    session_started: f64,
    orders_seeded: bool,
    reporting_started: f64,
    startup_executions: VecDeque<Execution>,
    account_busy: bool,
    pub sources: HashMap<String, Source>,
    fills: HashMap<String, (f64, Option<f64>)>,
    fill_order: VecDeque<String>,
    terminal: HashSet<String>,
    terminal_order: VecDeque<String>,
    tick: Option<TimerId>,
    debounce: Option<TimerId>,
    retry: Option<(TimerId, u32)>,
    auth_failures: u32,
}
impl Account {
    fn execution_progress(&mut self, id: &str, qty: f64, value: Option<f64>) {
        if !matches!(
            execution_prefix(self.fills.get(id).copied(), qty, value),
            Ok(true)
        ) {
            return;
        }
        self.store_execution_prefix(id, qty, value);
    }
    fn historical_execution_floor(&mut self, id: &str, qty: f64) {
        if qty.is_finite() && qty > self.fills.get(id).map_or(0.0, |prefix| prefix.0) {
            // Unpriceable pre-session quantity is history, never a live suffix.
            self.store_execution_prefix(id, qty, None);
        }
    }
    fn store_execution_prefix(&mut self, id: &str, qty: f64, value: Option<f64>) {
        self.fills.insert(id.to_owned(), (qty, value));
        self.fill_order.retain(|key| key != id);
        self.fill_order.push_back(id.to_owned());
        while self.fill_order.len() > 500 {
            if let Some(id) = self.fill_order.pop_front() {
                self.fills.remove(&id);
            }
        }
    }
    pub(crate) fn new() -> Self {
        Self {
            supervisor: Supervisor::new(Config {
                auth_timeout_ms: Some(10_000.0),
                idle_timeout_ms: 20_000.0,
                ping_interval_ms: Some(5_000.0),
                ..Config::default()
            }),
            ..Self::default()
        }
    }

    fn remember_terminal(&mut self, id: &str) {
        if self.terminal.insert(id.to_owned()) {
            self.terminal_order.push_back(id.to_owned());
            if self.terminal_order.len() > 500
                && let Some(expired) = self.terminal_order.pop_front()
            {
                self.terminal.remove(&expired);
            }
        }
    }
}
pub enum Request {
    Leverage {
        command: CommandId,
        symbol: String,
        inst_id: String,
        margin: MarginType,
        settings_revision: u64,
        positions_revision: u64,
    },
    Margin {
        revision: u64,
    },
    Mode {
        revision: u64,
    },
    Positions {
        positions: Vec<Position>,
        used: f64,
        upnl: f64,
        positions_revision: u64,
        balance_revision: u64,
        failed: bool,
    },
    Balance {
        positions: Vec<Position>,
        used: f64,
        upnl: f64,
        available: f64,
        frozen: f64,
        positions_revision: u64,
        balance_revision: u64,
        failed: bool,
    },
    Orders {
        family: u8,
        page: u32,
        attempt: u32,
        cursor: String,
        last_cursor: String,
        page_rows: usize,
        rows: Vec<Order>,
        sources: HashMap<String, Source>,
        revision: u64,
        failed: bool,
    },
}
impl Blofin {
    fn seed_execution_prefix(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        id: &str,
        qty: f64,
        value: Option<f64>,
    ) {
        match execution_prefix(self.account.fills.get(id).copied(), qty, value) {
            Ok(true) => self.account.execution_progress(id, qty, value),
            Ok(false) => {}
            Err(error) => cx.log(LogLevel::Error, format!("BloFin execution price unavailable for order {id}: {error}; retained proven execution baseline")),
        }
    }
    pub(crate) fn fetch_leverage(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        symbol: String,
    ) {
        let Some(market) = cx.store.market(&symbol) else {
            cx.done(command, Err(format!("Unknown market {symbol}")));
            return;
        };
        let Some(margin) = self.account.margin.filter(|_| self.account.mode_loaded) else {
            cx.done(command, Err("BloFin margin mode is not loaded".into()));
            return;
        };
        let inst_id = market.id.clone();
        let path = format!(
            "/api/v1/account/batch-leverage-info?instId={inst_id}&marginMode={}",
            if margin == MarginType::Isolated {
                "isolated"
            } else {
                "cross"
            }
        );
        self.private_read(
            cx,
            &path,
            Request::Leverage {
                command,
                symbol,
                inst_id,
                margin,
                settings_revision: self.account.settings_revision,
                positions_revision: self.account.positions_revision,
            },
            false,
        );
    }
    pub(crate) fn remember_terminal(&mut self, id: &str) {
        self.account.remember_terminal(id);
    }
    pub(crate) fn filled_native(&self, id: &str) -> f64 {
        self.account.fills.get(id).map_or(0.0, |value| value.0)
    }
    pub(crate) fn signed(&self) -> bool {
        !self.options.public_data_only
            && self.options.key.as_ref().is_some_and(|s| !s.is_empty())
            && self.options.secret.as_ref().is_some_and(|s| !s.is_empty())
    }
    pub(crate) fn headers(
        &self,
        path: &str,
        method: Method,
        now: f64,
        body: &str,
    ) -> Result<Vec<(&'static str, String)>, String> {
        if !self.signed() {
            return Err("BloFin requires API credentials".into());
        }
        let read = self
            .options
            .read_credentials
            .as_ref()
            .filter(|_| method == Method::Get);
        let pass = self
            .options
            .passphrase
            .as_deref()
            .map(|primary| {
                read.and_then(|r| r.passphrase.as_deref())
                    .unwrap_or(primary)
            })
            .filter(|s| !s.is_empty())
            .ok_or("BloFin requires a passphrase")?;
        let secret = read.map_or(self.options.secret.as_deref().unwrap(), |r| {
            r.secret.as_str()
        });
        let nonce = safe_cex_crypto::uuid::uuid()?;
        let timestamp = num::format(now.floor());
        Ok(vec![
            (
                "ACCESS-KEY",
                read.map_or_else(|| self.options.key.clone().unwrap(), |r| r.key.clone()),
            ),
            (
                "ACCESS-SIGN",
                sign::signature(secret, path, method.as_str(), &timestamp, &nonce, body),
            ),
            ("ACCESS-NONCE", nonce),
            ("ACCESS-TIMESTAMP", timestamp),
            ("ACCESS-PASSPHRASE", pass.to_owned()),
            ("Content-Type", "application/json".into()),
        ])
    }
    fn private_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        path: &str,
        request: Request,
        stream: bool,
    ) {
        match self.headers(path, Method::Get, cx.now, "") {
            Ok(headers) => {
                let pending = Pending::Account(request);
                if stream {
                    cx.request_streamed(
                        Method::Get,
                        format!("{}{path}", self.host()),
                        headers,
                        ArrayAt::EnvelopeField("data"),
                        pending,
                    );
                } else {
                    cx.request(
                        Method::Get,
                        format!("{}{path}", self.host()),
                        headers,
                        None,
                        pending,
                    );
                }
            }
            Err(e) => self.account_failed(cx, request, &e),
        }
    }
    pub(crate) fn private_start(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.signed() {
            return;
        }
        if self.options.passphrase.as_deref().is_none_or(str::is_empty) {
            cx.log(LogLevel::Error, "BloFin requires a passphrase");
            return;
        }
        self.account.session_started = cx.now;
        self.private_connect(cx);
        self.account.tick = Some(cx.set_poll_timer(10_000.0));
        self.private_read(
            cx,
            "/api/v1/account/margin-mode",
            Request::Margin {
                revision: self.account.settings_revision,
            },
            false,
        );
        self.private_read(
            cx,
            "/api/v1/account/position-mode",
            Request::Mode {
                revision: self.account.settings_revision,
            },
            false,
        );
    }
    fn private_connect(&mut self, cx: &mut Cx<'_, Pending>) {
        let url = if self.options.testnet {
            "wss://demo-trading-openapi.blofin.com/ws/private"
        } else {
            "wss://openapi.blofin.com/ws/private"
        };
        self.account.connection.socket =
            Some(cx.connect_channel(url, safe_cex_engine::SocketChannel::Private));
        self.account.authenticated = false;
        self.account.supervisor.connecting(cx);
    }
    pub(crate) fn private_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        self.account.supervisor.opened(cx);
        let nonce = match safe_cex_crypto::uuid::uuid() {
            Ok(n) => n,
            Err(e) => {
                cx.log(LogLevel::Error, e);
                cx.close(socket);
                self.private_close(cx, socket);
                return;
            }
        };
        let timestamp = cx.now.floor() as u64;
        let sign = sign::signature(
            self.options.secret.as_deref().unwrap_or(""),
            "/users/self/verify",
            "GET",
            &timestamp.to_string(),
            &nonce,
            "",
        );
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Login<'a> {
            api_key: &'a str,
            passphrase: &'a str,
            timestamp: u64,
            sign: String,
            nonce: String,
        }
        #[derive(Serialize)]
        struct Auth<'a> {
            op: &'static str,
            args: [Login<'a>; 1],
        }
        cx.send(
            socket,
            serde_json::to_string(&Auth {
                op: "login",
                args: [Login {
                    api_key: self.options.key.as_deref().unwrap_or(""),
                    passphrase: self.options.passphrase.as_deref().unwrap_or(""),
                    timestamp,
                    sign,
                    nonce,
                }],
            })
            .unwrap(),
        );
        self.account.connection.open = true;

        cx.send(socket, "ping");
    }
    pub(crate) fn refresh_account(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.account_busy || !self.signed() {
            return;
        }
        self.account.account_busy = true;
        self.private_read(
            cx,
            "/api/v1/account/positions",
            Request::Positions {
                positions: vec![],
                used: 0.0,
                upnl: 0.0,
                positions_revision: self.account.positions_revision,
                balance_revision: self.account.balance_revision,
                failed: false,
            },
            true,
        );
    }
    pub(crate) fn refresh_orders(&mut self, cx: &mut Cx<'_, Pending>, attempt: u32) {
        if self.account.orders_busy || !self.signed() {
            return;
        }
        self.account.orders_busy = true;
        self.orders_page(
            cx,
            Request::Orders {
                family: 0,
                page: 0,
                attempt,
                cursor: String::new(),
                last_cursor: String::new(),
                page_rows: 0,
                rows: vec![],
                sources: HashMap::new(),
                revision: self.account.orders_revision,
                failed: false,
            },
        );
    }
    fn orders_page(&mut self, cx: &mut Cx<'_, Pending>, request: Request) {
        let Request::Orders { family, cursor, .. } = &request else {
            return;
        };
        let endpoint = match family {
            0 => "orders-pending",
            1 => "orders-tpsl-pending",
            _ => "orders-algo-pending",
        };
        let mut path = format!("/api/v1/trade/{endpoint}?limit=100");
        if *family == 2 {
            path.push_str("&orderType=trigger");
        }
        if !cursor.is_empty() {
            path.push_str("&after=");
            path.push_str(cursor);
        }
        self.private_read(cx, &path, request, true);
    }
    pub(crate) fn map_orders(&self, cx: &Cx<'_, Pending>, o: &OrderRow<'_>) -> Vec<Order> {
        if !o.inst_type.is_empty() && !o.inst_type.eq_ignore_ascii_case("SWAP") {
            return vec![];
        }
        let Some(m) = cx.store.market_by_id(&o.inst_id) else {
            return vec![];
        };
        let side = if o.side == "sell" {
            OrderSide::Sell
        } else {
            OrderSide::Buy
        };
        let amount = clean(o.size.0 * m.contract_size.unwrap_or(1.0));
        let filled = clean(o.filled_size.0 * m.contract_size.unwrap_or(1.0));
        let status = match o.state.as_ref() {
            "filled" | "effective" => OrderStatus::Closed,
            "canceled" | "order_failed" => OrderStatus::Canceled,
            _ => OrderStatus::Open,
        };
        let base = Order {
            id: o.order_id.to_string(),
            parent_id: None,
            status,
            symbol: m.symbol.clone(),
            kind: OrderType::Limit,
            side,
            price: o.price.0,
            amount,
            filled,
            remaining: clean(amount - filled),
            reduce_only: o.reduce_only.0,
            close: None,
            client_order_id: (!o.client_order_id.is_empty()).then(|| o.client_order_id.to_string()),
            time_in_force: Some(if o.order_type == "post_only" {
                TimeInForce::PostOnly
            } else {
                TimeInForce::GoodTillCancel
            }),
            last_update: cx.now,
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: Some(OrderExtra {
                remaining_native: Some(clean(o.size.0 - o.filled_size.0)),
                filled_in_placement_units: Some(o.filled_size.0),
                ..OrderExtra::default()
            }),
        };
        if o.order_type == "trigger"
            && !o.algo_id.is_empty()
            && !o.reduce_only.0
            && o.trigger_price.0 > 0.0
        {
            return vec![Order {
                id: o.algo_id.to_string(),
                kind: OrderType::Conditional,
                price: o.trigger_price.0,
                filled: 0.0,
                remaining: amount,
                time_in_force: None,
                ..base
            }];
        }
        if !o.tpsl_id.is_empty() {
            return [
                (OrderType::TakeProfit, o.tp_trigger_price.0, "take_profit"),
                (OrderType::StopLoss, o.sl_trigger_price.0, "stop_loss"),
            ]
            .into_iter()
            .filter(|(_, p, _)| *p > 0.0)
            .map(|(kind, price, suffix)| Order {
                id: format!("{}__{suffix}", o.tpsl_id),
                kind,
                price,
                side: if o.order_id.is_empty() {
                    side
                } else {
                    side.opposite()
                },
                reduce_only: true,
                filled: 0.0,
                remaining: amount,
                time_in_force: None,
                ..base.clone()
            })
            .collect();
        }
        if o.order_id.is_empty() || o.order_category != "normal" {
            return vec![];
        }
        vec![Order {
            kind: if matches!(o.order_type.as_ref(), "market" | "optimal_limit_ioc") {
                OrderType::Market
            } else {
                OrderType::Limit
            },
            ..base
        }]
    }
    fn source(o: &OrderRow<'_>) -> Source {
        let attached = o.attach_algo_orders.first();
        let (tp, sl, tp_order_price, sl_order_price, tp_type, sl_type) =
            attached.map_or((0.0, 0.0, 0.0, 0.0, "", ""), |a| {
                (
                    a.tp_trigger_price.0,
                    a.sl_trigger_price.0,
                    a.tp_order_price.0,
                    a.sl_order_price.0,
                    a.tp_trigger_price_type.as_ref(),
                    a.sl_trigger_price_type.as_ref(),
                )
            });
        Source {
            execution_baseline: (
                o.filled_size.0,
                (o.average_price.0 > 0.0 || o.filled_size.0 == 0.0)
                    .then_some(o.filled_size.0 * o.average_price.0),
            ),
            inst_id: o.inst_id.to_string(),
            margin_mode: o.margin_mode.to_string(),
            position_side: o.position_side.to_string(),
            order_id: o.order_id.to_string(),
            tpsl_id: o.tpsl_id.to_string(),
            algo_id: o.algo_id.to_string(),
            trigger_price_type: o.trigger_price_type.to_string(),
            order_price: o.order_price.0,
            tp: if o.tp_trigger_price.0 > 0.0 {
                o.tp_trigger_price.0
            } else {
                tp
            },
            sl: if o.sl_trigger_price.0 > 0.0 {
                o.sl_trigger_price.0
            } else {
                sl
            },
            tp_type: if o.tp_trigger_price_type.is_empty() {
                tp_type.to_owned()
            } else {
                o.tp_trigger_price_type.to_string()
            },
            sl_type: if o.sl_trigger_price_type.is_empty() {
                sl_type.to_owned()
            } else {
                o.sl_trigger_price_type.to_string()
            },
            tp_order_price: if o.tp_order_price.0 != 0.0 {
                o.tp_order_price.0
            } else {
                tp_order_price
            },
            sl_order_price: if o.sl_order_price.0 != 0.0 {
                o.sl_order_price.0
            } else {
                sl_order_price
            },
        }
    }
    fn map_position(&self, cx: &Cx<'_, Pending>, p: &PositionRow<'_>) -> Option<Position> {
        if !p.inst_type.is_empty() && !p.inst_type.eq_ignore_ascii_case("SWAP") {
            return None;
        }
        let m = cx.store.market_by_id(&p.inst_id)?;
        let contracts = clean(p.positions.0 * m.contract_size.unwrap_or(1.0));
        let side = if p.position_side == "short" || (p.position_side == "net" && contracts < 0.0) {
            PositionSide::Short
        } else {
            PositionSide::Long
        };
        let mode = if p.position_side == "net" {
            PositionMode::Oneway
        } else {
            PositionMode::Hedged
        };
        let id = if mode == PositionMode::Oneway {
            m.symbol.clone()
        } else {
            format!(
                "{}:{}",
                m.symbol,
                if side == PositionSide::Long {
                    "long"
                } else {
                    "short"
                }
            )
        };
        let existing = cx.store.position(&id);
        let leverage = if p.leverage.0 > 0.0 {
            p.leverage.0
        } else {
            existing
                .map(|p| p.leverage)
                .filter(|v| v.is_finite() && *v > 0.0)
                .or_else(|| {
                    cx.store
                        .setting(&m.symbol)
                        .map(|s| {
                            if mode == PositionMode::Hedged {
                                if side == PositionSide::Long {
                                    s.buy_leverage
                                } else {
                                    s.sell_leverage
                                }
                                .unwrap_or(s.leverage)
                            } else {
                                s.leverage
                            }
                        })
                        .filter(|v| v.is_finite() && *v > 0.0)
                })
                .unwrap_or(0.0)
        };
        Some(Position {
            id,
            symbol: m.symbol.clone(),
            side,
            entry_price: if contracts != 0.0 {
                p.average_price.0
            } else {
                0.0
            },
            notional: num::adjust((contracts * p.mark_price.0).abs(), m.precision.price),
            leverage,
            margin_type: Some(if p.margin_mode == "isolated" {
                MarginType::Isolated
            } else {
                MarginType::Cross
            }),
            position_mode: Some(mode),
            unrealized_pnl: num::adjust(p.unrealized_pnl.0, m.precision.price),
            realized_pnl: None,
            contracts: contracts.abs(),
            liquidation_price: p
                .liquidation_price
                .parse::<f64>()
                .ok()
                .filter(|p| p.is_finite())
                .unwrap_or_else(|| existing.map_or(0.0, |p| p.liquidation_price)),
            margin_size: None,
            last_update: None,
        })
    }
    fn commit_positions(&mut self, cx: &mut Cx<'_, Pending>, positions: Vec<Position>) {
        for p in &positions {
            cx.store.update_setting(&p.symbol, |s| {
                if p.position_mode == Some(PositionMode::Hedged) {
                    if p.side == PositionSide::Long {
                        s.buy_leverage = Some(p.leverage);
                    } else {
                        s.sell_leverage = Some(p.leverage);
                    }
                } else {
                    s.buy_leverage = Some(p.leverage);
                    s.sell_leverage = Some(p.leverage);
                }
                s.leverage = s.buy_leverage.or(s.sell_leverage).unwrap_or(p.leverage);
                s.margin_type = p.margin_type;
            });
        }
        // Position margin overrides apply only while that symbol is open. A
        // close (including omission from a full snapshot) restores the account
        // default without losing the symbol's last confirmed leverage.
        if let Some(margin) = self.account.margin {
            let open: HashSet<_> = positions
                .iter()
                .filter(|position| position.contracts > 0.0)
                .map(|position| position.symbol.as_str())
                .collect();
            let closed: Vec<_> = cx
                .store
                .markets()
                .iter()
                .filter(|market| !open.contains(market.symbol.as_str()))
                .filter(|market| {
                    cx.store
                        .setting(&market.symbol)
                        .is_some_and(|setting| setting.margin_type != Some(margin))
                })
                .map(|market| market.symbol.clone())
                .collect();
            for symbol in closed {
                cx.store
                    .update_setting(&symbol, |setting| setting.margin_type = Some(margin));
            }
        }
        cx.store.set_positions(
            positions
                .into_iter()
                .filter(|p| p.contracts > 0.0)
                .collect(),
        );
    }
    pub(crate) fn account_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        match request {
            Request::Positions {
                positions,
                used,
                upnl,
                failed,
                ..
            } => match serde_json::from_str::<PositionRow<'_>>(item) {
                Ok(p) => {
                    if let Some(position) = self.map_position(cx, &p) {
                        *used += p.maintenance_margin.0;
                        *upnl += position.unrealized_pnl;
                        positions.push(position);
                    }
                }
                Err(_) => *failed = true,
            },
            Request::Balance {
                available,
                frozen,
                failed,
                ..
            } => match serde_json::from_str::<AssetRow>(item) {
                Ok(b) => {
                    *available += b.available.0;
                    *frozen += b.frozen.0;
                }
                Err(_) => *failed = true,
            },
            Request::Orders {
                family,
                last_cursor,
                page_rows,
                rows,
                sources,
                failed,
                ..
            } => match serde_json::from_str::<OrderRow<'_>>(item) {
                Ok(o) => {
                    *page_rows += 1;
                    *last_cursor = match family {
                        0 => o.order_id.to_string(),
                        1 => o.tpsl_id.to_string(),
                        _ => o.algo_id.to_string(),
                    };
                    if last_cursor.is_empty() {
                        *failed = true;
                    }
                    let source = Self::source(&o);
                    for order in self.map_orders(cx, &o) {
                        sources.insert(order.id.clone(), source.clone());
                        rows.push(order);
                    }
                }
                Err(_) => *failed = true,
            },
            _ => {}
        }
    }
    pub(crate) fn account_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        if let Err(e) = check(&response) {
            if response.body.contains("152401") || response.body.contains("152406") {
                self.account.auth_failures += 1;
            }
            self.account_failed(cx, request, &e);
            return;
        }
        self.account.auth_failures = 0;
        match request {
            Request::Leverage {
                command,
                symbol,
                inst_id,
                margin,
                settings_revision,
                positions_revision,
            } => {
                let result = (|| {
                    let Data { data: rows } =
                        serde_json::from_str::<Data<Vec<LeverageRow<'_>>>>(response.body)
                            .map_err(|_| "Malformed BloFin leverage response")?;
                    if rows.is_empty() || rows.len() > 2 {
                        return Err("Malformed BloFin leverage rows");
                    }
                    let expected_margin = if margin == MarginType::Isolated {
                        "isolated"
                    } else {
                        "cross"
                    };
                    let mut sides = std::collections::HashSet::new();
                    for row in &rows {
                        if row.inst_id != inst_id
                            || row.margin_mode != expected_margin
                            || !matches!(row.position_side.as_ref(), "net" | "long" | "short")
                            || !row.leverage.0.is_finite()
                            || row.leverage.0 <= 0.0
                            || !sides.insert(row.position_side.as_ref())
                        {
                            return Err("Malformed BloFin leverage row");
                        }
                    }
                    let net = rows.iter().find(|row| row.position_side == "net");
                    if net.is_some() && rows.len() != 1 {
                        return Err("Conflicting BloFin leverage sides");
                    }
                    if settings_revision != self.account.settings_revision
                        || positions_revision != self.account.positions_revision
                    {
                        return Err("BloFin leverage response was superseded");
                    }
                    let buy = rows
                        .iter()
                        .find(|row| row.position_side == "long")
                        .or(net)
                        .unwrap_or(&rows[0]);
                    let sell = rows
                        .iter()
                        .find(|row| row.position_side == "short")
                        .or(net)
                        .unwrap_or(&rows[0]);
                    #[derive(Serialize)]
                    #[serde(rename_all = "camelCase")]
                    struct FetchedLeverage {
                        buy_leverage: f64,
                        sell_leverage: f64,
                        margin_type: MarginType,
                        position_mode: PositionMode,
                    }
                    let result = FetchedLeverage {
                        buy_leverage: buy.leverage.0,
                        sell_leverage: sell.leverage.0,
                        margin_type: margin,
                        position_mode: if net.is_some() {
                            PositionMode::Oneway
                        } else {
                            PositionMode::Hedged
                        },
                    };
                    let json = serde_json::to_string(&result)
                        .map_err(|_| "Invalid BloFin leverage result")?;
                    self.account.settings_revision += 1;
                    self.account.positions_revision += 1;
                    cx.store.update_setting(&symbol, |setting| {
                        setting.leverage = result.buy_leverage;
                        setting.buy_leverage = Some(result.buy_leverage);
                        setting.sell_leverage = Some(result.sell_leverage);
                        setting.margin_type = Some(margin);
                    });
                    Ok(json)
                })();
                cx.done(command, result.map_err(str::to_owned));
            }
            Request::Margin { revision } => {
                if let Ok(Data { data: m }) =
                    serde_json::from_str::<Data<ModeRow<'_>>>(response.body)
                {
                    if !matches!(m.margin_mode.as_ref(), "cross" | "isolated") {
                        cx.log(LogLevel::Error, "Malformed BloFin margin mode");
                        return;
                    }
                    if revision == self.account.settings_revision {
                        self.account.margin = Some(if m.margin_mode == "isolated" {
                            MarginType::Isolated
                        } else {
                            MarginType::Cross
                        });
                        let symbols: Vec<_> = cx
                            .store
                            .markets()
                            .iter()
                            .map(|m| m.symbol.clone())
                            .collect();
                        for symbol in symbols {
                            cx.store
                                .update_setting(&symbol, |s| s.margin_type = self.account.margin);
                        }
                    }
                    self.bootstrap_account(cx);
                } else {
                    cx.log(LogLevel::Error, "Malformed BloFin margin mode");
                }
            }
            Request::Mode { revision } => {
                if let Ok(Data { data: m }) =
                    serde_json::from_str::<Data<ModeRow<'_>>>(response.body)
                {
                    if !matches!(m.position_mode.as_ref(), "net_mode" | "long_short_mode") {
                        cx.log(LogLevel::Error, "Malformed BloFin position mode");
                        return;
                    }
                    if revision == self.account.settings_revision {
                        cx.store.set_hedged(m.position_mode == "long_short_mode");
                        self.account.mode_loaded = true;
                    }
                    self.bootstrap_account(cx);
                } else {
                    cx.log(LogLevel::Error, "Malformed BloFin position mode");
                }
            }
            Request::Positions {
                positions,
                used,
                upnl,
                positions_revision,
                balance_revision,
                failed: false,
            } => self.private_read(
                cx,
                "/api/v1/asset/balances?accountType=futures",
                Request::Balance {
                    positions,
                    used,
                    upnl,
                    available: 0.0,
                    frozen: 0.0,
                    positions_revision,
                    balance_revision,
                    failed: false,
                },
                true,
            ),
            Request::Balance {
                positions,
                used,
                upnl,
                available,
                frozen,
                positions_revision,
                balance_revision,
                failed: false,
            } => {
                self.account.account_busy = false;
                if positions_revision == self.account.positions_revision {
                    self.commit_positions(cx, positions);
                }
                if balance_revision == self.account.balance_revision {
                    let used = round_usd(used);
                    let upnl = round_usd(upnl);
                    cx.store.set_balance(Balance {
                        used,
                        upnl,
                        free: clean(clean(round_usd(available) + round_usd(frozen)) - used),
                        total: clean(clean(round_usd(available) + round_usd(frozen)) - upnl),
                    });
                }
            }
            Request::Orders {
                family,
                page,
                attempt,
                cursor,
                last_cursor,
                page_rows,
                mut rows,
                mut sources,
                revision,
                failed: false,
            } => {
                if page_rows > 0 {
                    if page >= 49 || cursor == last_cursor {
                        self.account_failed(
                            cx,
                            Request::Orders {
                                family,
                                page,
                                attempt,
                                cursor,
                                last_cursor,
                                page_rows,
                                rows,
                                sources,
                                revision,
                                failed: true,
                            },
                            "BloFin pending order pagination did not advance",
                        );
                        return;
                    }
                    self.orders_page(
                        cx,
                        Request::Orders {
                            family,
                            page: page + 1,
                            attempt,
                            cursor: last_cursor,
                            last_cursor: String::new(),
                            page_rows: 0,
                            rows,
                            sources,
                            revision,
                            failed: false,
                        },
                    );
                } else if family < 2 {
                    self.orders_page(
                        cx,
                        Request::Orders {
                            family: family + 1,
                            page: 0,
                            attempt,
                            cursor: String::new(),
                            last_cursor: String::new(),
                            page_rows: 0,
                            rows,
                            sources,
                            revision,
                            failed: false,
                        },
                    );
                } else {
                    self.account.orders_busy = false;
                    if !self.account.orders_seeded {
                        // Even when newer WS state rejects this store snapshot, its
                        // complete cumulative rows still prove execution prefixes.
                        self.seed_startup_executions(cx, &rows, &sources);
                    }
                    if revision == self.account.orders_revision {
                        for order in cx.store.orders() {
                            if matches!(order.status, OrderStatus::Closed | OrderStatus::Canceled) {
                                self.account.remember_terminal(&order.id);
                            }
                        }
                        rows.retain(|order| {
                            !self.account.terminal.contains(&order.id)
                                && (order
                                    .extra
                                    .as_ref()
                                    .and_then(|extra| extra.filled_in_placement_units)
                                    .unwrap_or(0.0)
                                    >= self.filled_native(&order.id)
                                    || cx.store.order(&order.id).is_some())
                        });
                        let retained: HashSet<_> =
                            rows.iter().map(|order| order.id.as_str()).collect();
                        sources.retain(|id, _| retained.contains(id.as_str()));
                        for order in &mut rows {
                            if let Some(existing) = cx
                                .store
                                .order(&order.id)
                                .filter(|existing| existing.filled > order.filled)
                            {
                                *order = existing.clone();
                                if let Some(source) = self.account.sources.get(&order.id) {
                                    sources.insert(order.id.clone(), source.clone());
                                }
                            }
                        }
                        let mut extra = Vec::new();
                        for order in &rows {
                            let Some(source) = sources.get(&order.id) else {
                                continue;
                            };
                            if order.kind != OrderType::Limit
                                || !source.tpsl_id.is_empty()
                                || !source.algo_id.is_empty()
                            {
                                continue;
                            }
                            for (kind, price, suffix) in [
                                (OrderType::TakeProfit, source.tp, "take_profit"),
                                (OrderType::StopLoss, source.sl, "stop_loss"),
                            ] {
                                if price <= 0.0
                                    || rows.iter().any(|leg| {
                                        leg.kind == kind
                                            && sources.get(&leg.id).is_some_and(|s| {
                                                s.order_id == order.id && !s.tpsl_id.is_empty()
                                            })
                                    })
                                {
                                    continue;
                                }
                                let id = format!("{}__{suffix}", order.id);
                                extra.push(Order {
                                    id: id.clone(),
                                    kind,
                                    price,
                                    side: order.side.opposite(),
                                    parent_id: Some(order.id.clone()),
                                    reduce_only: true,
                                    filled: 0.0,
                                    amount: if order.remaining > 0.0 {
                                        order.remaining
                                    } else {
                                        order.amount
                                    },
                                    remaining: if order.remaining > 0.0 {
                                        order.remaining
                                    } else {
                                        order.amount
                                    },
                                    extra: Some(OrderExtra {
                                        remaining_native: order
                                            .extra
                                            .as_ref()
                                            .and_then(|e| e.remaining_native),
                                        ..OrderExtra::default()
                                    }),
                                    ..order.clone()
                                });
                            }
                        }
                        extra.retain(|order| !self.account.terminal.contains(&order.id));
                        for order in &extra {
                            if let Some(source) = order
                                .parent_id
                                .as_ref()
                                .and_then(|id| sources.get(id))
                                .cloned()
                            {
                                sources.insert(order.id.clone(), source);
                            }
                        }
                        rows.extend(extra);
                        self.account.sources = sources;
                        cx.store.set_orders(rows);
                        cx.orders_reconciled(None);
                    } else if self.account.retry.is_none() {
                        self.account.retry = Some((cx.set_timer(1_000.0), 1));
                    }
                }
            }
            other => self.account_failed(cx, other, "Malformed BloFin private response"),
        }
    }
    fn bootstrap_account(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.margin.is_some()
            && self.account.mode_loaded
            && !self.account.startup_started
        {
            self.account.startup_started = true;
            self.refresh_account(cx);
            self.refresh_orders(cx, 1);
        }
    }
    pub(crate) fn account_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        match request {
            Request::Leverage { command, .. } => {
                cx.command_error(command, error);
                cx.done(command, Err(error.to_owned()));
                return;
            }
            Request::Orders { attempt, .. } => {
                self.account.orders_busy = false;
                if attempt < 3 && self.account.auth_failures == 0 {
                    self.account.retry =
                        Some((cx.set_timer(1000.0 * f64::from(attempt)), attempt + 1));
                }
            }
            Request::Positions { .. } | Request::Balance { .. } => {
                self.account.account_busy = false
            }
            _ => {}
        }
        cx.log(LogLevel::Error, error);
    }
    pub(crate) fn private_message(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        socket: SocketId,
        text: &str,
    ) {
        self.account.supervisor.received(cx.now);
        if text == "pong" {
            return;
        }
        if text == "ping" {
            cx.send(socket, "pong");
            return;
        }
        let Ok(message) = serde_json::from_str::<Message<'_>>(text) else {
            return;
        };
        if message.event == "login" {
            if message.code != "0" {
                cx.log(
                    LogLevel::Error,
                    format!("BloFin login rejected: {}", message.code),
                );
                cx.close(socket);
                self.private_close(cx, socket);
                return;
            }
            if self.account.authenticated {
                return;
            }
            self.account.authenticated = true;
            self.account.supervisor.authenticated(cx);
            for channel in ["account", "positions", "orders", "orders-algo"] {
                cx.send(
                    socket,
                    format!("{{\"op\":\"subscribe\",\"args\":[{{\"channel\":\"{channel}\"}}]}}"),
                );
            }
            if self.account.margin.is_some() && self.account.mode_loaded {
                self.refresh_account(cx);
                self.refresh_orders(cx, 1);
            }
            return;
        }
        if !message.event.is_empty() {
            return;
        }
        match message.arg.channel.as_ref() {
            "orders" | "orders-algo" => {
                // Missing quantity is not an explicit zero proof; a present
                // quantity must retain OrderRow's rejection of malformed/null values.
                fn present_filled_size<'de, D: serde::Deserializer<'de>>(
                    d: D,
                ) -> Result<Option<Number>, D::Error> {
                    Number::deserialize(d).map(Some)
                }
                #[derive(Deserialize)]
                struct StreamOrder<'a> {
                    #[serde(flatten, borrow)]
                    row: OrderRow<'a>,
                    #[serde(rename = "updateTime", default)]
                    updated: Number,
                    #[serde(rename = "createTime", default)]
                    created: Number,
                    #[serde(
                        rename = "filledSize",
                        default,
                        deserialize_with = "present_filled_size"
                    )]
                    filled: Option<Number>,
                }
                if let Ok(Data { data: rows }) =
                    serde_json::from_str::<Data<Vec<&serde_json::value::RawValue>>>(text)
                {
                    self.account.orders_revision += 1;
                    for raw in rows {
                        let Ok(StreamOrder {
                            mut row,
                            updated,
                            created,
                            filled,
                        }) = serde_json::from_str::<StreamOrder<'_>>(raw.get())
                        else {
                            cx.log(
                                LogLevel::Error,
                                "Malformed BloFin order row; retaining known order",
                            );
                            continue;
                        };
                        if let Some(qty) = filled {
                            row.filled_size = qty;
                        }
                        let orders = self.map_orders(cx, &row);
                        if filled.is_some_and(|qty| qty.0 == 0.0)
                            && let Some(order) = orders.first()
                            && matches!(order.kind, OrderType::Limit | OrderType::Market)
                            && !self.account.fills.contains_key(&order.id)
                        {
                            self.account.execution_progress(&order.id, 0.0, Some(0.0));
                        }
                        if matches!(row.state.as_ref(), "partially_filled" | "filled")
                            && !orders.is_empty()
                        {
                            self.execution(cx, &row, &orders[0], updated.0, created.0);
                        }
                        let source = Self::source(&row);
                        for order in orders {
                            if matches!(order.status, OrderStatus::Closed | OrderStatus::Canceled) {
                                self.account.remember_terminal(&order.id);
                                self.account.sources.remove(&order.id);
                                if !cx
                                    .store
                                    .order(&order.id)
                                    .is_some_and(|existing| existing.status == OrderStatus::Closed)
                                {
                                    cx.store.set_order_status(&order.id, order.status);
                                }
                                continue;
                            }
                            if self.account.terminal.contains(&order.id) {
                                continue;
                            }
                            if row.filled_size.0 < self.filled_native(&order.id)
                                || cx.store.order(&order.id).is_some_and(|existing| {
                                    matches!(
                                        existing.status,
                                        OrderStatus::Closed | OrderStatus::Canceled
                                    ) || order.filled < existing.filled
                                })
                            {
                                continue;
                            }
                            self.account
                                .sources
                                .insert(order.id.clone(), source.clone());
                            cx.store.upsert_order(order);
                        }
                    }
                    if let Some(timer) = self.account.debounce.take() {
                        cx.clear_timer(timer);
                    }
                    self.account.debounce = Some(cx.set_timer(2_000.0));
                }
            }
            "positions" => {
                if let Ok(Data { data: rows }) =
                    serde_json::from_str::<Data<Vec<&serde_json::value::RawValue>>>(text)
                {
                    let mut positions = Vec::new();
                    let mut malformed = HashSet::new();
                    let mut used = 0.0;
                    let mut upnl = 0.0;
                    for raw in rows {
                        match serde_json::from_str::<PositionRow<'_>>(raw.get()) {
                            Ok(row) => {
                                // Account totals cover every native position, including
                                // instruments outside the tradable market catalog. Sum
                                // raw PnL before any per-market display rounding.
                                used += row.maintenance_margin.0;
                                upnl += row.unrealized_pnl.0;
                                if let Some(position) = self.map_position(cx, &row) {
                                    positions.push(position);
                                }
                            }
                            Err(_) => {
                                cx.log(LogLevel::Error, "Malformed BloFin position row; retaining known position and balance totals");
                                #[derive(serde::Deserialize)]
                                struct Identity<'a> {
                                    #[serde(borrow, rename = "instId")]
                                    inst_id: Str<'a>,
                                }
                                let Ok(identity) = serde_json::from_str::<Identity<'_>>(raw.get())
                                else {
                                    // With no identity, any missing known position might be
                                    // the malformed row. Preserve this authoritative snapshot.
                                    return;
                                };
                                let Some(market) = cx.store.market_by_id(&identity.inst_id) else {
                                    return;
                                };
                                malformed.insert(market.symbol.clone());
                            }
                        }
                    }
                    // Preserve the complete affected symbol, including both hedge legs.
                    positions.retain(|position| !malformed.contains(&position.symbol));
                    positions.extend(
                        cx.store
                            .positions()
                            .iter()
                            .filter(|position| malformed.contains(&position.symbol))
                            .cloned(),
                    );
                    self.account.positions_revision += 1;
                    self.account.balance_revision += 1;
                    self.commit_positions(cx, positions);
                    if malformed.is_empty() {
                        let mut balance = cx.store.balance().clone();
                        balance.used = round_usd(used);
                        balance.upnl = round_usd(upnl);
                        cx.store.set_balance(balance);
                    }
                }
            }
            "account" => {
                if let Ok(Data { data: row }) = serde_json::from_str::<Data<AccountRow>>(text) {
                    self.account.balance_revision += 1;
                    let mut balance = cx.store.balance().clone();
                    let total = round_usd(row.total_equity.0);
                    balance.free = clean(total - balance.used);
                    balance.total = clean(total - balance.upnl);
                    cx.store.set_balance(balance);
                }
            }
            _ => {}
        }
    }
    fn execution(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        row: &OrderRow<'_>,
        order: &Order,
        updated: f64,
        created: f64,
    ) {
        let id = if row.order_id.is_empty() {
            &row.ord_id
        } else {
            &row.order_id
        };
        let qty = row.filled_size.0;
        let value = qty * row.average_price.0;
        if qty > 0.0 && (row.average_price.0 <= 0.0 || !value.is_finite() || value <= 0.0) {
            if updated > 0.0 && updated < self.account.session_started {
                self.account.historical_execution_floor(id, qty);
            } else {
                cx.log(LogLevel::Error, format!("BloFin execution price unavailable for order {id}: invalid cumulative quote value; retained proven execution baseline"));
            }
            return;
        }
        let contract = cx
            .store
            .market(&order.symbol)
            .and_then(|m| m.contract_size)
            .unwrap_or(1.0);
        let execution = Execution {
            fill: Fill {
                order_id: id.to_string(),
                symbol: order.symbol.clone(),
                side: order.side,
                price: row.average_price.0,
                amount: clean(qty * contract),
            },
            qty,
            value,
            contract,
            updated,
        };
        let session_order = created > 0.0
            && created >= self.account.session_started
            && created <= cx.now
            && (updated == 0.0 || created <= updated);
        let known_prefix = self.account.fills.contains_key(id.as_ref());
        if !session_order
            && (!self.account.orders_seeded || !known_prefix)
            && !(updated > 0.0 && updated < self.account.session_started)
        {
            if self.account.orders_seeded {
                // A delayed terminal row absent from the startup open-order census
                // cannot distinguish old fills from its unobserved startup suffix.
                self.seed_execution_prefix(cx, id, qty, Some(execution.value));
                cx.log(
                    if updated == 0.0 || updated >= self.account.reporting_started {
                        LogLevel::Error
                    } else {
                        LogLevel::Warning
                    },
                    format!("BloFin execution prefix unavailable for existing order {id}; cannot distinguish historical fills from live executions; refreshed cumulative baseline"),
                );
            } else {
                let prefix = self
                    .account
                    .startup_executions
                    .iter()
                    .filter(|queued| queued.fill.order_id == id.as_ref())
                    .fold(
                        self.account.fills.get(id.as_ref()).copied(),
                        |prefix, queued| {
                            if matches!(
                                execution_prefix(prefix, queued.qty, Some(queued.value)),
                                Ok(true)
                            ) {
                                Some((queued.qty, Some(queued.value)))
                            } else {
                                prefix
                            }
                        },
                    );
                if let Err(error) = execution_prefix(prefix, qty, Some(execution.value)) {
                    cx.log(LogLevel::Error, format!("BloFin execution price unavailable for order {id}: {error}; retained proven execution baseline"));
                    return;
                }
                if self.account.startup_executions.len() == 500
                    && let Some(old) = self.account.startup_executions.pop_front()
                {
                    // Bound startup memory; an evicted observation becomes a quiet
                    // baseline, so later cumulative rows cannot replay its prefix.
                    let previous = self
                        .account
                        .fills
                        .get(&old.fill.order_id)
                        .map_or(0.0, |p| p.0);
                    if old.qty > previous {
                        self.seed_execution_prefix(
                            cx,
                            &old.fill.order_id,
                            old.qty,
                            Some(old.value),
                        );
                    }
                }
                self.account.startup_executions.push_back(execution);
            }
            return;
        }
        self.emit_execution(cx, execution);
    }
    fn seed_startup_executions(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        rows: &[Order],
        sources: &HashMap<String, Source>,
    ) {
        for order in rows {
            if matches!(order.kind, OrderType::Limit | OrderType::Market)
                && let Some(source) = sources.get(&order.id)
                && !self.account.fills.contains_key(&order.id)
            {
                let (qty, value) = source.execution_baseline;
                self.seed_execution_prefix(cx, &order.id, qty, value);
            }
        }
        let pending = std::mem::take(&mut self.account.startup_executions);
        let mut unknown = HashMap::<String, (f64, f64)>::new();
        for execution in &pending {
            if !self.account.fills.contains_key(&execution.fill.order_id) {
                let previous = unknown
                    .get(&execution.fill.order_id)
                    .map(|&(qty, value)| (qty, Some(value)));
                match execution_prefix(previous, execution.qty, Some(execution.value)) {
                    Ok(true) => { unknown.insert(execution.fill.order_id.clone(), (execution.qty, execution.value)); }
                    Ok(false) => {}
                    Err(error) => cx.log(LogLevel::Error, format!("BloFin execution price unavailable for order {}: {error}; retained proven execution baseline", execution.fill.order_id)),
                }
            }
        }
        // An old order already terminal before the first census has no proven
        // historical prefix. Start reporting after its last startup observation.
        for (id, (qty, value)) in unknown {
            self.seed_execution_prefix(cx, &id, qty, Some(value));
        }
        self.account.orders_seeded = true;
        self.account.reporting_started = cx.now;
        for execution in pending {
            if !self.account.fills.contains_key(&execution.fill.order_id) {
                // The bounded tracker may have evicted a queued order while
                // seeding a large census. Never turn that eviction into a zero prefix.
                if let Some(source) = sources.get(&execution.fill.order_id) {
                    let (qty, value) = source.execution_baseline;
                    self.seed_execution_prefix(cx, &execution.fill.order_id, qty, value);
                } else {
                    self.seed_execution_prefix(
                        cx,
                        &execution.fill.order_id,
                        execution.qty,
                        Some(execution.value),
                    );
                    continue;
                }
            }
            self.emit_execution(cx, execution);
        }
    }
    fn emit_execution(&mut self, cx: &mut Cx<'_, Pending>, execution: Execution) {
        let Execution {
            mut fill,
            qty,
            value,
            contract,
            updated,
        } = execution;
        let previous = self
            .account
            .fills
            .get(&fill.order_id)
            .copied()
            .unwrap_or((0.0, Some(0.0)));
        if qty <= previous.0 {
            if qty == previous.0 && previous.1.is_none() {
                self.account
                    .execution_progress(&fill.order_id, qty, Some(value));
            }
            return;
        }
        let historical = updated > 0.0 && updated < self.account.session_started;
        let delta = decimal_difference(qty, previous.0);
        // Missing historical quote value cannot prove the suffix price. This
        // observation supplies the quote baseline for subsequent live executions.
        let Some(previous_value) = previous.1 else {
            self.account
                .execution_progress(&fill.order_id, qty, Some(value));
            if !historical {
                cx.log(LogLevel::Error, format!("BloFin execution price unavailable for order {}: missing cumulative quote baseline; cannot recover execution price; refreshed execution baseline", fill.order_id));
            }
            return;
        };
        let delta_value = decimal_difference(value, previous_value);
        if !delta_value.is_finite() || delta_value <= 0.0 {
            if historical {
                self.account.historical_execution_floor(&fill.order_id, qty);
            } else {
                cx.log(LogLevel::Error, format!("BloFin execution price unavailable for order {}: nonpositive cumulative quote interval; retained proven execution baseline", fill.order_id));
            }
            return;
        }
        self.account
            .execution_progress(&fill.order_id, qty, Some(value));
        if historical {
            return;
        }
        fill.price = clean(delta_value / delta);
        fill.amount = clean(delta * contract);
        cx.fill(fill);
    }
    pub(crate) fn private_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if self.account.connection.socket != Some(socket) {
            return;
        }
        let c = &mut self.account.connection;
        c.socket = None;
        c.open = false;
        self.account.supervisor.retry(cx);
    }
    pub(crate) fn account_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        match self.account.supervisor.on_timer(cx, timer) {
            SocketTick::Unhandled => (),
            SocketTick::Reconnect => {
                self.private_connect(cx);
                return true;
            }
            SocketTick::Ping => {
                if let Some(socket) = self.account.connection.socket {
                    cx.send(socket, "ping");
                }
                return true;
            }
            SocketTick::TimedOut(_) => {
                if let Some(socket) = self.account.connection.socket {
                    cx.close(socket);
                    self.private_close(cx, socket);
                }
                return true;
            }
            SocketTick::Handled => return true,
        }
        if self.account.tick == Some(timer) {
            self.account.tick = None;
            if self.account.margin.is_none() {
                self.private_read(
                    cx,
                    "/api/v1/account/margin-mode",
                    Request::Margin {
                        revision: self.account.settings_revision,
                    },
                    false,
                );
            }
            if !self.account.mode_loaded {
                self.private_read(
                    cx,
                    "/api/v1/account/position-mode",
                    Request::Mode {
                        revision: self.account.settings_revision,
                    },
                    false,
                );
            }
            if self.account.margin.is_some() && self.account.mode_loaded {
                self.refresh_account(cx);
                self.refresh_orders(cx, 1);
            }
            let not_before_ms = if self.account.auth_failures > 0 {
                cx.now
                    + (10_000.0 * 2f64.powi(self.account.auth_failures.min(3) as i32)).min(60_000.0)
            } else {
                f64::NEG_INFINITY
            };
            self.account.tick = Some(cx.set_poll_timer_with_bounds(
                10_000.0,
                safe_cex_engine::PollTimerBounds {
                    not_before_ms,
                    ..Default::default()
                },
            ));
            return true;
        }
        if self.account.debounce == Some(timer) {
            self.account.debounce = None;
            self.refresh_orders(cx, 1);
            return true;
        }
        if self.account.retry.is_some_and(|(t, _)| t == timer) {
            let (_, attempt) = self.account.retry.take().unwrap();
            self.refresh_orders(cx, attempt);
            return true;
        }
        false
    }
}
pub(crate) fn clean(n: f64) -> f64 {
    if n == 0.0 || !n.is_finite() {
        return n;
    }
    let factor = 10f64.powi(14 - n.abs().log10().floor() as i32);
    (n * factor).round() / factor
}
fn decimal_difference(a: f64, b: f64) -> f64 {
    let digits = num::decimals(a).max(num::decimals(b)).min(20);
    let factor = 10f64.powi(digits as i32);
    ((a - b) * factor).round() / factor
}
fn execution_prefix(
    previous: Option<(f64, Option<f64>)>,
    qty: f64,
    value: Option<f64>,
) -> Result<bool, &'static str> {
    if !qty.is_finite()
        || qty < 0.0
        || value.is_some_and(|value| !value.is_finite() || value < 0.0 || qty > 0.0 && value == 0.0)
    {
        return Err("invalid cumulative quote value");
    }
    if let Some((previous_qty, previous_value)) = previous {
        if qty <= previous_qty {
            return Ok(qty == previous_qty && previous_value.is_none() && value.is_some());
        }
        if let Some(previous_value) = previous_value {
            let Some(value) = value else {
                return Err("missing cumulative quote value");
            };
            let interval = decimal_difference(value, previous_value);
            if !interval.is_finite() || interval <= 0.0 {
                return Err("nonpositive cumulative quote interval");
            }
        }
    }
    Ok(true)
}
