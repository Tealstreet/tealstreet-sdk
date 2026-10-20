//! Account scopes reconcile independently; a failed read never becomes empty state.
use crate::{Hyperliquid, Pending, wire::*};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::*;
use safe_cex_engine::store::MarketSetting;
use safe_cex_engine::{Cx, Fill, LogLevel, Method, Response, TimerId, json};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};

pub enum Request {
    Role,
    Mode,
    State {
        dex: String,
        revision: u64,
        settings_revision: u64,
    },
    Orders {
        dex: String,
        revision: u64,
        orders: Vec<Order>,
        seen: HashSet<String>,
        valid: bool,
    },
    Spot {
        revision: u64,
    },
    Asset {
        symbol: String,
        revision: u64,
    },
    Builder,
    Fees,
}
enum ReadContinuation {
    Account(Request),
    History(Box<crate::history::Request>),
}
impl ReadContinuation {
    fn pending(self) -> Pending {
        match self {
            Self::Account(r) => Pending::Account(r),
            Self::History(r) => Pending::History(r),
        }
    }
}
struct QueuedRead {
    body: String,
    request: ReadContinuation,
    streamed: bool,
    weight: u32,
}
struct SettingBarrier {
    at: f64,
    revision: u64,
    confirmed: bool,
}

struct PendingSpotCollateral {
    balances: Vec<(String, f64, f64)>,
    available: Option<f64>,
    ws: bool,
}

#[derive(Default)]
pub struct Account {
    pub started: bool,
    mode: Mode,
    mode_resolved: bool,
    mode_timer: Option<TimerId>,
    role_timer: Option<TimerId>,
    sync_timer: Option<TimerId>,
    last_order_refresh: f64,
    read_timer: Option<TimerId>,
    read_window: f64,
    read_weight: u32,
    reads: VecDeque<QueuedRead>,
    pub refresh_timer: Option<TimerId>,
    spot_at: f64,
    spot_forced_at: Option<f64>,
    spot_inflight: bool,
    spot_revision: u64,
    spot_authoritative_ws: bool,
    core_value: f64,
    core_loaded: bool,
    core_used: f64,
    spot: Option<(f64, f64)>,
    pending_spot: Option<PendingSpotCollateral>,
    authoritative: Option<(f64, f64, f64)>,
    state_revision: HashMap<String, u64>,
    settings_revision: u64,
    settings_barriers: HashMap<String, SettingBarrier>,
    asset_retry: HashSet<String>,
    asset_retry_timer: Option<TimerId>,
    state_time: HashMap<String, f64>,
    position_time: HashMap<String, f64>,
    snapshot_time: HashMap<String, f64>,
    last_fill: HashMap<String, (f64, u64)>,
    state_dirty: HashSet<String>,
    state_refresh_timer: Option<TimerId>,
    pub orders_revision: HashMap<String, u64>,
    inflight: HashSet<String>,
    pub suppressed: HashMap<String, f64>,
    seen_fills: HashSet<u64>,
    fill_queue: VecDeque<u64>,
    spot_filled: HashMap<String, (String, f64)>,
    spot_fill_orders: VecDeque<String>,
    spot_fill_tids: HashSet<u64>,
    spot_fill_queue: VecDeque<u64>,
    notify_fills: HashSet<u64>,
    notify_queue: VecDeque<u64>,
    realized: HashMap<(String, PositionSide), f64>,
    pub records: HashMap<String, Placement>,
    pub preplacements: VecDeque<Placement>,
    pub pending: HashMap<String, Placement>,
    pub builder_approved: Option<bool>,
    valuation_warned: bool,
}
#[derive(Default, PartialEq, Eq)]
enum Mode {
    #[default]
    Standard,
    Unified,
    Portfolio,
}
#[derive(Clone)]
pub struct Placement {
    pub order: Order,
    pub at: f64,
}
impl Hyperliquid {
    pub(crate) fn apply_spot_ack_fills(&self, order: &mut Order) {
        if let Some((symbol, filled)) = self.account.spot_filled.get(&order.id)
            && *symbol == order.symbol
        {
            order.filled = order.amount.min(*filled);
            order.remaining = (order.amount - order.filled).max(0.0);
        }
    }
    fn record_spot_fill(&mut self, fill: &FillRow<'_>, symbol: &str) {
        if !self.account.spot_fill_tids.insert(fill.tid) {
            return;
        }
        self.account.spot_fill_queue.push_back(fill.tid);
        if self.account.spot_fill_queue.len() > 20_000
            && let Some(tid) = self.account.spot_fill_queue.pop_front()
        {
            self.account.spot_fill_tids.remove(&tid);
        }
        let id = fill.oid.to_string();
        if !self.account.spot_filled.contains_key(&id) {
            self.account.spot_fill_orders.push_back(id.clone());
        }
        let entry = self
            .account
            .spot_filled
            .entry(id)
            .or_insert_with(|| (symbol.to_owned(), 0.0));
        if entry.0 == symbol {
            entry.1 += fill.sz;
        }
        if self.account.spot_fill_orders.len() > 1024
            && let Some(id) = self.account.spot_fill_orders.pop_front()
        {
            self.account.spot_filled.remove(&id);
        }
    }
    pub(crate) fn user(&self) -> Option<&str> {
        self.options
            .vault
            .as_deref()
            .or(self.options.application_id.as_deref())
    }
    fn account_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        kind: &str,
        dex: Option<&str>,
        request: Request,
        streamed: bool,
    ) {
        #[derive(Serialize)]
        struct Read<'a> {
            r#type: &'a str,
            user: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            dex: Option<&'a str>,
        }
        let body = serde_json::to_string(&Read {
            r#type: kind,
            user: if kind == "userFees" {
                self.options.key.as_deref().unwrap_or_default()
            } else {
                self.user().unwrap_or_default()
            },
            dex,
        })
        .unwrap();
        self.queue_read(cx, body, request, streamed);
    }
    fn read_body(&mut self, cx: &mut Cx<'_, Pending>, body: String, request: Request) {
        self.queue_read(cx, body, request, false);
    }
    fn queue_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        body: String,
        request: Request,
        streamed: bool,
    ) {
        let weight = match &request {
            Request::Role => 60,
            Request::State { .. } | Request::Spot { .. } => 2,
            _ => 20,
        };
        self.account.reads.push_back(QueuedRead {
            body,
            request: ReadContinuation::Account(request),
            streamed,
            weight,
        });
        self.pump_reads(cx);
    }
    pub(crate) fn history_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        body: String,
        request: crate::history::Request,
    ) {
        let streamed = request.streamed();
        let weight = if streamed { 120 } else { 2 };
        self.account.reads.push_back(QueuedRead {
            body,
            request: ReadContinuation::History(Box::new(request)),
            streamed,
            weight,
        });
        self.pump_reads(cx);
    }
    pub(crate) fn history_weight(&mut self, cx: &mut Cx<'_, Pending>, rows: u32, reserved: bool) {
        if cx.now - self.account.read_window >= 60_000.0 {
            self.account.read_window = cx.now;
            self.account.read_weight = 0;
        }
        if reserved {
            self.account.read_weight = self.account.read_weight.saturating_sub(100);
        }
        self.account.read_weight = self.account.read_weight.saturating_add(rows.div_ceil(20));
        self.pump_reads(cx);
    }
    fn pump_reads(&mut self, cx: &mut Cx<'_, Pending>) {
        if cx.now - self.account.read_window >= 60_000.0 {
            self.account.read_window = cx.now;
            self.account.read_weight = 0;
        }
        while let Some(read) = self.account.reads.front() {
            // Leave headroom for chart reads and trading in the shared IP budget.
            if self.account.read_weight + read.weight > 1000 {
                if self.account.read_timer.is_none() {
                    self.account.read_timer = Some(
                        cx.set_timer((60_000.0 - (cx.now - self.account.read_window)).max(1.0)),
                    );
                }
                return;
            }
            let read = self.account.reads.pop_front().unwrap();
            self.account.read_weight += read.weight;
            if read.streamed {
                self.info(cx, read.body, ArrayAt::Root, read.request.pending());
            } else {
                cx.request(
                    Method::Post,
                    format!("{}/info", self.rest()),
                    vec![("Content-Type", "application/json".into())],
                    Some(read.body),
                    read.request.pending(),
                );
            }
        }
    }
    pub(crate) fn account_start(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.started {
            return;
        }
        self.account.started = true;
        if !self.options.public_data_only
            && let Some(secret) = self.options.secret.as_deref().filter(|s| !s.is_empty())
        {
            let expected = self.options.key.as_deref().or(self.user());
            if let Some(expected) = expected
                && safe_cex_crypto::ethereum::private_key_address(secret)
                    .map_or(true, |actual| !actual.eq_ignore_ascii_case(expected))
            {
                cx.log(
                    LogLevel::Warning,
                    "Hyperliquid invalid credentials: private key does not match API wallet",
                );
            }
        }
        // Partial discovery may start real account reads, not an empty snapshot.
        if self.spot || !self.core_discovery_incomplete || self.options.public_data_only {
            cx.store.set_orders(Vec::new());
            cx.store.set_positions(Vec::new());
        }
        if self.options.public_data_only {
            cx.store.set_balance(Balance::default());
        }
        cx.store.set_hedged(false);
        let settings = cx
            .store
            .markets()
            .iter()
            .enumerate()
            .map(|(i, _)| {
                (
                    i as u32,
                    MarketSetting {
                        leverage: if self.spot { 1.0 } else { 20.0 },
                        buy_leverage: None,
                        sell_leverage: None,
                        margin_type: Some(MarginType::Cross),
                    },
                )
            })
            .collect::<Vec<_>>();
        cx.store.set_settings(settings);
        if self.spot {
            self.account.mode_resolved = true;
            self.account.builder_approved = Some(false);
            if !self.options.public_data_only && self.user().is_some() {
                self.refresh_spot(cx, true);
                self.account.sync_timer = Some(cx.set_timer(10_000.0));
            }
            return;
        }
        self.account_read(cx, "userFees", None, Request::Fees, false);
        if self.options.public_data_only {
            return;
        }
        if self.user().is_none() {
            if let Some(key) = self.options.key.as_deref() {
                #[derive(Serialize)]
                struct Read<'a> {
                    r#type: &'static str,
                    user: &'a str,
                }
                self.read_body(
                    cx,
                    serde_json::to_string(&Read {
                        r#type: "userRole",
                        user: key,
                    })
                    .unwrap(),
                    Request::Role,
                );
                self.account.role_timer = Some(cx.set_timer(6000.0));
            }
            return;
        }
        self.private_bootstrap(cx);
    }
    fn private_bootstrap(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account_read(cx, "userAbstraction", None, Request::Mode, false);
        self.account.mode_timer = Some(cx.set_timer(6000.0));
        self.check_builder(cx);
        self.refresh_account(cx);
        if !self.dex_names.is_empty() {
            self.account.sync_timer = Some(cx.set_timer(120_000.0));
        }
    }
    pub(crate) fn check_builder(&mut self, cx: &mut Cx<'_, Pending>) {
        let Some(user) = self.options.application_id.as_deref() else {
            self.account.builder_approved = Some(false);
            return;
        };
        #[derive(Serialize)]
        struct Read<'a> {
            r#type: &'static str,
            user: &'a str,
            builder: &'a str,
        }
        let b = self.builder();
        self.read_body(
            cx,
            serde_json::to_string(&Read {
                r#type: "maxBuilderFee",
                user,
                builder: &b.b,
            })
            .unwrap(),
            Request::Builder,
        );
    }
    pub(crate) fn builder(&self) -> crate::Builder {
        let mut builder = self
            .options
            .builder
            .clone()
            .unwrap_or_else(|| crate::Builder {
                b: if self.options.testnet {
                    "0x76676949EA8f83A3a461AdfefD9a8C35d8e41fba"
                } else {
                    "0xea629e1e8534B82f704A818643F2DAF38cB7Fd4d"
                }
                .into(),
                f: 10,
            });
        builder.b.make_ascii_lowercase();
        builder
    }
    pub(crate) fn refresh_account(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.spot {
            self.refresh_spot(cx, true);
            return;
        }
        let mut dexes = vec![String::new()];
        dexes.extend(self.dex_names.iter().cloned());
        for dex in dexes {
            self.refresh_state(cx, &dex);
            self.refresh_orders_scope(cx, &dex);
        }
    }
    fn refresh_state(&mut self, cx: &mut Cx<'_, Pending>, dex: &str) {
        if !self.account.inflight.insert(format!("state:{dex}")) {
            return;
        }
        let revision = *self.account.state_revision.get(dex).unwrap_or(&0);
        self.account_read(
            cx,
            "clearinghouseState",
            (!dex.is_empty()).then_some(dex),
            Request::State {
                dex: dex.into(),
                revision,
                settings_revision: self.account.settings_revision,
            },
            false,
        );
    }
    pub(crate) fn refresh_orders(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.spot {
            self.refresh_spot(cx, true);
            return;
        }
        let mut dexes = vec![String::new()];
        dexes.extend(self.dex_names.iter().cloned());
        for dex in dexes {
            self.refresh_orders_scope(cx, &dex);
        }
    }
    fn refresh_orders_scope(&mut self, cx: &mut Cx<'_, Pending>, dex: &str) {
        if self.user().is_none() || !self.account.inflight.insert(format!("orders:{dex}")) {
            return;
        }
        let revision = *self.account.orders_revision.get(dex).unwrap_or(&0);
        self.account_read(
            cx,
            "frontendOpenOrders",
            (!dex.is_empty()).then_some(dex),
            Request::Orders {
                dex: dex.into(),
                revision,
                orders: Vec::new(),
                seen: HashSet::new(),
                valid: true,
            },
            true,
        );
    }
    pub(crate) fn schedule_orders(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.spot {
            self.refresh_spot(cx, true);
            return;
        }
        if self.account.refresh_timer.is_none() && self.user().is_some() {
            self.account.refresh_timer = Some(
                cx.set_timer(500.0f64.max(1000.0 - (cx.now - self.account.last_order_refresh))),
            );
        }
    }
    fn refresh_spot(&mut self, cx: &mut Cx<'_, Pending>, force: bool) {
        if self.account.spot_inflight
            || (!force && cx.now - self.account.spot_at < 10_000.0)
            || (self.spot
                && force
                && self
                    .account
                    .spot_forced_at
                    .is_some_and(|at| cx.now - at < 1500.0))
        {
            return;
        }
        if force {
            self.account.spot_forced_at = Some(cx.now);
        }
        self.account.spot_inflight = true;
        self.account.spot_at = cx.now;
        self.account_read(
            cx,
            "spotClearinghouseState",
            None,
            Request::Spot {
                revision: self.account.spot_revision,
            },
            false,
        );
    }
    fn mode_ready(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account.mode_resolved = true;
        if let Some(t) = self.account.mode_timer.take() {
            cx.clear_timer(t);
        }
        self.private_subscribe(cx);
        if self.account.mode != Mode::Standard {
            self.refresh_spot(cx, true);
        }
        self.live_balance(cx);
    }
    pub(crate) fn private_subscribe(&self, cx: &mut Cx<'_, Pending>) {
        if self.options.public_data_only || !self.account.mode_resolved || !self.connection.open {
            return;
        }
        let (Some(user), Some(socket)) = (self.user(), self.connection.socket) else {
            return;
        };
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Sub<'a> {
            r#type: &'a str,
            user: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            dex: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            coin: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            is_portfolio_margin: Option<bool>,
        }
        #[derive(Serialize)]
        struct Send<'a> {
            method: &'static str,
            subscription: Sub<'a>,
        }
        let markets = cx.store.markets().to_vec();
        let send = |cx: &mut Cx<'_, Pending>, kind, dex, coin, portfolio| {
            cx.send(
                socket,
                serde_json::to_string(&Send {
                    method: "subscribe",
                    subscription: Sub {
                        r#type: kind,
                        user,
                        dex,
                        coin,
                        is_portfolio_margin: portfolio,
                    },
                })
                .unwrap(),
            )
        };
        if self.spot {
            for kind in [
                "notification",
                "spotState",
                "userFills",
                "userNonFundingLedgerUpdates",
            ] {
                send(cx, kind, None, None, None);
            }
            return;
        }
        for kind in [
            "notification",
            "orderUpdates",
            "userEvents",
            "userFills",
            "userFundings",
            "userNonFundingLedgerUpdates",
            "clearinghouseState",
        ] {
            send(cx, kind, None, None, None);
        }
        if self.account.mode != Mode::Standard {
            send(
                cx,
                "spotState",
                None,
                None,
                (self.account.mode == Mode::Portfolio).then_some(true),
            );
        }
        for dex in &self.dex_names {
            for kind in [
                "clearinghouseState",
                "userFills",
                "orderUpdates",
                "userEvents",
            ] {
                send(cx, kind, Some(dex), None, None);
            }
        }
        for m in &markets {
            if m.active && !m.id.contains(':') {
                send(cx, "activeAssetData", None, Some(&m.id), None);
            }
        }
    }
    pub(crate) fn account_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        if !(200..300).contains(&response.status) {
            self.account_failed(
                cx,
                request,
                &format!("Hyperliquid HTTP {}", response.status),
            );
            return;
        }
        let raw = response.body;
        match request {
            Request::Role => {
                if self.account.role_timer.is_none() {
                    return;
                }
                if let Some(t) = self.account.role_timer.take() {
                    cx.clear_timer(t);
                }
                if let Ok(role) = serde_json::from_str::<Role>(raw) {
                    match role.role.as_ref() {
                        "user" => self.options.application_id = self.options.key.clone(),
                        "agent" => {
                            self.options.application_id = role.data.user.map(|v| v.into_owned())
                        }
                        "subAccount" => {
                            self.options.application_id = role.data.master.map(|v| v.into_owned())
                        }
                        "vault" => self.options.vault = self.options.key.clone(),
                        _ => {}
                    }
                }
                if self.user().is_some() {
                    self.private_bootstrap(cx);
                } else {
                    cx.log(
                        LogLevel::Error,
                        "Hyperliquid account identity could not be resolved; re-add this account",
                    );
                }
            }
            Request::Mode => {
                if self.account.mode_resolved {
                    return;
                }
                self.account.mode = match serde_json::from_str::<&str>(raw).unwrap_or("") {
                    "unifiedAccount" => Mode::Unified,
                    "portfolioMargin" => Mode::Portfolio,
                    _ => Mode::Standard,
                };
                self.mode_ready(cx);
            }
            Request::Builder => {
                self.account.builder_approved = serde_json::from_str::<Num>(raw)
                    .ok()
                    .map(|n| n.0 >= f64::from(self.builder().f));
            }
            Request::Fees => {
                #[derive(serde::Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Fees {
                    #[serde(deserialize_with = "number")]
                    user_cross_rate: f64,
                    #[serde(deserialize_with = "number")]
                    user_add_rate: f64,
                }
                if let Ok(f) = serde_json::from_str::<Fees>(raw) {
                    cx.store.update_markets(|m| {
                        m.taker_fee = Some(f.user_cross_rate);
                        m.maker_fee = Some(f.user_add_rate);
                    });
                }
            }
            Request::State {
                dex,
                revision,
                settings_revision,
            } => {
                self.account.inflight.remove(&format!("state:{dex}"));
                if let Ok(state) = serde_json::from_str::<Clearinghouse>(raw) {
                    let changed =
                        self.account.state_revision.get(&dex).copied().unwrap_or(0) != revision;
                    if !changed
                        || (state.server_time.or(state.time).is_some()
                            && self.account.state_time.contains_key(&dex))
                    {
                        self.apply_state(cx, &dex, &state, true, Some(settings_revision));
                    }
                } else {
                    cx.log(
                        LogLevel::Warning,
                        "Malformed Hyperliquid account state; preserving last snapshot",
                    );
                }
            }
            Request::Orders {
                dex,
                revision,
                orders,
                seen,
                valid,
            } => {
                self.account.inflight.remove(&format!("orders:{dex}"));
                if !valid {
                    return;
                }
                if self.account.orders_revision.get(&dex).copied().unwrap_or(0) != revision {
                    self.schedule_orders(cx);
                    return;
                }
                let mut merged = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| {
                        self.dex_of(cx, &o.symbol) != dex
                            || matches!(o.status, OrderStatus::Creating | OrderStatus::Updating)
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                for order in orders {
                    // A cancel can complete after this streamed row was parsed.
                    if self
                        .account
                        .suppressed
                        .get(&order.id)
                        .is_some_and(|at| cx.now - *at < 30_000.0)
                    {
                        continue;
                    }
                    self.account.suppressed.remove(&order.id);
                    if let Some(old) = merged.iter_mut().find(|o| o.id == order.id) {
                        *old = order;
                    } else {
                        merged.push(order);
                    }
                }
                cx.store.set_orders(merged);
                self.account
                    .suppressed
                    .retain(|_, at| cx.now - *at < 120_000.0);
                let _ = seen;
            }
            Request::Spot { revision } => {
                self.account.spot_inflight = false;
                if revision != self.account.spot_revision {
                    return;
                }
                if let Ok(state) = serde_json::from_str::<SpotState>(raw) {
                    self.apply_spot(cx, &state, false);
                }
            }
            Request::Asset { symbol, revision } => {
                if !self.settings_uncertain(&symbol)
                    || !self
                        .account
                        .settings_barriers
                        .get(&symbol)
                        .is_some_and(|b| b.revision == revision)
                {
                    return;
                }
                if let Ok(row) = serde_json::from_str::<ActiveAsset>(raw)
                    && self.symbol(&row.coin) == Some(symbol.as_str())
                {
                    self.apply_asset_setting(cx, &symbol, &row);
                } else {
                    self.account_failed(
                        cx,
                        Request::Asset { symbol, revision },
                        "Malformed Hyperliquid active asset settings",
                    );
                }
            }
        }
    }
    pub(crate) fn account_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        if let Request::Orders {
            orders,
            seen,
            valid,
            ..
        } = request
        {
            match serde_json::from_str::<OrderRow>(item) {
                Ok(row) => {
                    let id = row.oid.to_string();
                    if !seen.insert(id.clone()) {
                        return;
                    }
                    if self
                        .account
                        .suppressed
                        .get(&id)
                        .is_some_and(|at| cx.now - *at < 30_000.0)
                    {
                        return;
                    }
                    if let Some(order) = self.map_order(cx, &row) {
                        orders.push(order);
                    } else {
                        *valid = false;
                        cx.log(LogLevel::Warning,
                            "Hyperliquid owned order has unavailable market metadata; preserving previous orders");
                    }
                }
                Err(_) => *valid = false,
            }
        }
    }
    pub(crate) fn account_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        match request {
            Request::Mode => {
                if !self.account.mode_resolved {
                    self.mode_ready(cx);
                }
            }
            Request::Role => {
                if let Some(t) = self.account.role_timer.take() {
                    cx.clear_timer(t);
                }
            }
            Request::State { dex, .. } => {
                self.account.inflight.remove(&format!("state:{dex}"));
            }
            Request::Orders { dex, .. } => {
                self.account.inflight.remove(&format!("orders:{dex}"));
            }
            Request::Spot { .. } => self.account.spot_inflight = false,
            Request::Asset { symbol, .. } => {
                if self.settings_uncertain(&symbol) {
                    self.account.asset_retry.insert(symbol);
                    if self.account.asset_retry_timer.is_none() {
                        self.account.asset_retry_timer = Some(cx.set_timer(5000.0));
                    }
                }
            }
            Request::Builder => self.account.builder_approved = None,
            Request::Fees => {}
        }
        cx.log(LogLevel::Warning, error);
    }
    pub(crate) fn account_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.account.asset_retry_timer == Some(timer) {
            self.account.asset_retry_timer = None;
            for symbol in std::mem::take(&mut self.account.asset_retry) {
                if self.settings_uncertain(&symbol) {
                    self.refresh_asset_setting(cx, &symbol);
                }
            }
        }
        if self.account.state_refresh_timer == Some(timer) {
            self.account.state_refresh_timer = None;
            let dirty = std::mem::take(&mut self.account.state_dirty);
            for dex in dirty {
                if self.account.inflight.contains(&format!("state:{dex}")) {
                    self.account.state_dirty.insert(dex);
                } else {
                    self.refresh_state(cx, &dex);
                }
            }
            if !self.account.state_dirty.is_empty() {
                self.account.state_refresh_timer = Some(cx.set_timer(350.0));
            }
        }
        if self.account.read_timer == Some(timer) {
            self.account.read_timer = None;
            self.pump_reads(cx);
        }
        if self.account.mode_timer == Some(timer) {
            self.account.mode_timer = None;
            self.mode_ready(cx);
        }
        if self.account.role_timer == Some(timer) {
            self.account.role_timer = None;
            cx.log(
                LogLevel::Warning,
                "Hyperliquid identity resolution timed out",
            );
        }
        if self.account.refresh_timer == Some(timer) {
            self.account.refresh_timer = None;
            self.account.last_order_refresh = cx.now;
            self.refresh_orders(cx);
        }
        if self.account.sync_timer == Some(timer) && self.spot {
            self.refresh_spot(cx, false);
            self.account.sync_timer = Some(cx.set_timer(10_000.0));
            return;
        }
        if self.account.sync_timer == Some(timer) {
            for dex in self.dex_names.clone() {
                self.refresh_state(cx, &dex);
            }
            self.account.sync_timer = Some(cx.set_timer(120_000.0));
        }
    }
    fn dex_of<'a>(&self, cx: &'a Cx<'_, Pending>, symbol: &str) -> &'a str {
        cx.store
            .market(symbol)
            .and_then(|m| m.id.split_once(':'))
            .map_or("", |(d, _)| d)
    }
    pub(crate) fn acknowledge_order_write(&mut self, cx: &Cx<'_, Pending>, symbol: &str) {
        let dex = self.dex_of(cx, symbol).to_owned();
        *self.account.orders_revision.entry(dex).or_default() += 1;
    }
    pub(crate) fn acknowledge_settings(&mut self, cx: &Cx<'_, Pending>, symbol: &str) {
        self.account.settings_revision += 1;
        self.account.settings_barriers.insert(
            symbol.into(),
            SettingBarrier {
                at: cx.now,
                revision: self.account.settings_revision,
                confirmed: false,
            },
        );
    }
    pub(crate) fn refresh_uncertain_settings(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        self.acknowledge_settings(cx, symbol);
        let dex = self.dex_of(cx, symbol).to_owned();
        self.account.state_dirty.insert(dex);
        if self.account.state_refresh_timer.is_none() {
            self.account.state_refresh_timer = Some(cx.set_timer(350.0));
        }
        self.refresh_asset_setting(cx, symbol);
    }
    fn refresh_asset_setting(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        #[derive(Serialize)]
        struct Read<'a> {
            r#type: &'static str,
            user: &'a str,
            coin: &'a str,
        }
        if let Some(coin) = self.coin(cx, symbol) {
            let body = serde_json::to_string(&Read {
                r#type: "activeAssetData",
                user: self.user().unwrap_or_default(),
                coin,
            })
            .unwrap();
            self.read_body(
                cx,
                body,
                Request::Asset {
                    symbol: symbol.into(),
                    revision: self
                        .account
                        .settings_barriers
                        .get(symbol)
                        .map_or(0, |b| b.revision),
                },
            );
        }
    }
    fn apply_asset_setting(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: &str,
        row: &ActiveAsset<'_>,
    ) {
        let margin = if row.leverage.kind == "cross" {
            MarginType::Cross
        } else {
            MarginType::Isolated
        };
        cx.store.update_setting(symbol, |s| {
            s.leverage = row.leverage.value;
            s.margin_type = Some(margin);
        });
        cx.store.update_positions(|p| {
            if p.symbol == symbol {
                p.leverage = row.leverage.value;
                p.margin_type = Some(margin);
            }
        });
        if let Some(barrier) = self.account.settings_barriers.get_mut(symbol) {
            barrier.confirmed = true;
        }
        self.settings_reconciled(symbol);
    }
    fn apply_state(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        dex: &str,
        state: &Clearinghouse<'_>,
        authoritative: bool,
        settings_revision: Option<u64>,
    ) {
        // Context discovery may be retrying. Unknown nonzero native ownership
        // cannot become a successful empty/partial authoritative account scope.
        if state
            .asset_positions
            .iter()
            .any(|row| row.position.szi != 0.0 && self.symbol(&row.position.coin).is_none())
        {
            cx.log(LogLevel::Error,
                "Hyperliquid owned position has unavailable market metadata; preserving previous account snapshot");
            return;
        }
        let source_time = state.server_time.or(state.time);
        if (authoritative || !dex.is_empty())
            && let Some(time) = source_time
            && self
                .account
                .position_time
                .iter()
                .any(|(symbol, last)| self.dex_of(cx, symbol) == dex && *last > time)
        {
            return;
        }
        if let Some(time) = source_time {
            if self
                .account
                .state_time
                .get(dex)
                .is_some_and(|last| time < *last)
            {
                return;
            }
            self.account.state_time.insert(dex.into(), time);
        }
        *self.account.state_revision.entry(dex.into()).or_default() += 1;
        if authoritative && let Some(time) = source_time {
            for market in cx.store.markets() {
                if market.id.split_once(':').map_or("", |(d, _)| d) == dex {
                    self.account
                        .position_time
                        .insert(market.symbol.clone(), time);
                    self.account
                        .snapshot_time
                        .insert(market.symbol.clone(), time);
                }
            }
        }
        let mut positions = if authoritative || !dex.is_empty() {
            cx.store
                .positions()
                .iter()
                .filter(|p| self.dex_of(cx, &p.symbol) != dex)
                .cloned()
                .collect::<Vec<_>>()
        } else {
            cx.store.positions().to_vec()
        };
        for row in &state.asset_positions {
            let p = &row.position;
            let Some(symbol) = self.symbol(&p.coin).map(str::to_owned) else {
                continue;
            };
            if let Some(time) = source_time {
                if self
                    .account
                    .position_time
                    .get(&symbol)
                    .is_some_and(|last| *last > time)
                {
                    continue;
                }
                self.account.position_time.insert(symbol.clone(), time);
                self.account.snapshot_time.insert(symbol.clone(), time);
            }
            let mut margin = if p.leverage.kind == "cross" {
                MarginType::Cross
            } else {
                MarginType::Isolated
            };
            // Position size can be fresh while its settings predate a local write.
            let settings_fresh =
                self.account
                    .settings_barriers
                    .get(&symbol)
                    .is_none_or(|barrier| {
                        settings_revision.is_none_or(|read| read >= barrier.revision)
                            && source_time.map_or_else(
                                || settings_revision.is_some(),
                                |time| time >= barrier.at,
                            )
                    });
            let mut leverage = p.leverage.value;
            if settings_fresh {
                cx.store.update_setting(&symbol, |s| {
                    s.leverage = leverage;
                    s.margin_type = Some(margin);
                });
                if let Some(barrier) = self.account.settings_barriers.get_mut(&symbol) {
                    barrier.confirmed = true;
                }
                self.settings_reconciled(&symbol);
            } else if let Some(setting) = cx.store.setting(&symbol) {
                leverage = setting.leverage;
                margin = setting.margin_type.unwrap_or(margin);
            }
            positions.retain(|old| old.symbol != symbol);
            if p.szi == 0.0 {
                continue;
            }
            let side = if p.szi > 0.0 {
                PositionSide::Long
            } else {
                PositionSide::Short
            };
            positions.push(Position {
                id: symbol.clone(),
                symbol: symbol.clone(),
                side,
                entry_price: p.entry_px.unwrap_or(0.0),
                notional: p.position_value,
                leverage,
                margin_type: Some(margin),
                position_mode: Some(PositionMode::Oneway),
                unrealized_pnl: p.unrealized_pnl,
                realized_pnl: self.account.realized.get(&(symbol, side)).copied(),
                contracts: p.szi.abs(),
                liquidation_price: p.liquidation_px.unwrap_or(0.0),
                margin_size: p.margin_used,
                last_update: state.time,
            });
        }
        cx.store.set_positions(positions);
        if dex.is_empty()
            && let Some(summary) = state
                .margin_summary
                .as_ref()
                .or(state.cross_margin_summary.as_ref())
        {
            self.account.core_loaded = true;
            self.account.core_value = summary.account_value
                - cx.store
                    .positions()
                    .iter()
                    .map(|p| p.unrealized_pnl)
                    .sum::<f64>();
            self.account.core_used = summary.total_margin_used;
        }
        self.live_balance(cx);
    }
    pub(crate) fn account_prices(&mut self, cx: &mut Cx<'_, Pending>) {
        if cx.store.loaded().balance || self.account.pending_spot.is_some() {
            self.live_balance(cx);
        }
    }
    fn collateral_values<'a>(
        &self,
        cx: &Cx<'_, Pending>,
        balances: impl Iterator<Item = (&'a str, f64, f64)>,
    ) -> Option<(f64, f64, bool)> {
        let mut equity = 0.0;
        let mut hold = 0.0;
        let mut unvalued = false;
        for (coin, total, held) in balances {
            let price = if total == 0.0 && held == 0.0
                || ["USDC", "USDT", "USD", "USDH"]
                    .iter()
                    .any(|stable| coin.eq_ignore_ascii_case(stable))
            {
                1.0
            } else if let Some(symbol) = self.symbol(coin) {
                let ticker = cx.store.ticker(symbol)?;
                [ticker.mark, ticker.last]
                    .into_iter()
                    .find(|price| price.is_finite() && *price > 0.0)?
            } else if self.core_discovery_incomplete {
                return None;
            } else {
                // Perp mode has no spot meta: tokens like UBTC have no valuation market.
                unvalued = true;
                continue;
            };
            equity += total * price;
            hold += held * price;
            if !equity.is_finite() || !hold.is_finite() {
                return None;
            }
        }
        Some((equity, hold, unvalued))
    }
    fn warn_unvalued(&mut self, cx: &mut Cx<'_, Pending>, unvalued: bool) {
        if unvalued && !self.account.valuation_warned {
            self.account.valuation_warned = true;
            cx.log(
                LogLevel::Warning,
                "Hyperliquid collateral token has no valuation market; excluded from USD balance",
            );
        }
    }
    fn current_upnl(&self, cx: &Cx<'_, Pending>) -> f64 {
        cx.store
            .positions()
            .iter()
            .map(|p| {
                cx.store
                    .ticker(&p.symbol)
                    .filter(|t| t.last > 0.0 && p.entry_price > 0.0)
                    .map_or(p.unrealized_pnl, |t| {
                        (t.last - p.entry_price)
                            * p.contracts
                            * if p.side == PositionSide::Long {
                                1.0
                            } else {
                                -1.0
                            }
                    })
            })
            .sum()
    }
    fn commit_collateral(
        &mut self,
        cx: &Cx<'_, Pending>,
        equity: f64,
        hold: f64,
        available: Option<f64>,
        ws: bool,
    ) {
        self.account.spot = Some((equity, hold));
        if let Some(free) = available {
            self.account.authoritative =
                Some((equity - self.current_upnl(cx), equity - free, free));
            if ws {
                self.account.spot_authoritative_ws = true;
            }
        }
    }
    pub(crate) fn live_balance(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.spot
            || self.user().is_none()
            || self.options.public_data_only
            || !self.account.mode_resolved
            || !self.account.core_loaded
        {
            return;
        }
        if let Some(pending) = self.account.pending_spot.as_ref() {
            let Some((equity, hold, unvalued)) = self.collateral_values(
                cx,
                pending
                    .balances
                    .iter()
                    .map(|(coin, total, hold)| (coin.as_str(), *total, *hold)),
            ) else {
                return;
            };
            let available = pending.available;
            let ws = pending.ws;
            self.account.pending_spot = None;
            self.warn_unvalued(cx, unvalued);
            self.commit_collateral(cx, equity, hold, available, ws);
        }
        if self.account.mode != Mode::Standard && self.account.spot.is_none() {
            return;
        }
        let upnl = self.current_upnl(cx);
        let balance = if self.account.mode != Mode::Standard
            && let Some((equity, hold)) = self.account.spot
        {
            let (total, used, free) = self.account.authoritative.unwrap_or((
                equity,
                self.account.core_used + hold,
                equity + upnl - self.account.core_used - hold,
            ));
            Balance {
                total,
                used,
                free,
                upnl,
            }
        } else {
            let total = self.account.core_value;
            Balance {
                total,
                used: self.account.core_used,
                free: total + upnl - self.account.core_used,
                upnl,
            }
        };
        cx.store.set_balance(balance);
    }
    fn spot_price(&self, cx: &Cx<'_, Pending>, coin: &str, path: &mut Vec<String>) -> f64 {
        if ["USDC", "USDT", "USD", "USDH"]
            .iter()
            .any(|c| coin.eq_ignore_ascii_case(c))
        {
            return 1.0;
        }
        if path.len() >= 3
            || path
                .iter()
                .any(|visited| visited.eq_ignore_ascii_case(coin))
        {
            return 0.0;
        }
        path.push(coin.to_owned());
        let price =
            (|| {
                for usdc in [true, false] {
                    for market in cx.store.markets().iter().filter(|m| {
                        m.base.eq_ignore_ascii_case(coin) && (m.quote == "USDC") == usdc
                    }) {
                        let Some(ticker) = cx.store.ticker(&market.symbol) else {
                            continue;
                        };
                        let mark = if ticker.mark > 0.0 {
                            ticker.mark
                        } else {
                            ticker.last
                        };
                        if mark.is_finite() && mark > 0.0 {
                            let quote = self.spot_price(cx, &market.quote, path);
                            let notional = mark * quote;
                            if notional.is_finite() && notional > 0.0 {
                                return notional;
                            }
                        }
                    }
                }
                0.0
            })();
        path.pop();
        price
    }
    fn apply_spot(&mut self, cx: &mut Cx<'_, Pending>, state: &SpotState<'_>, ws: bool) {
        if self.spot {
            let mut balance = Balance::default();
            let mut currencies = Vec::new();
            let mut coins = Vec::new();
            let mut valuation_path = Vec::with_capacity(3);
            for b in &state.balances {
                let coin = b
                    .token
                    .and_then(|token| self.spot_tokens.get(&token))
                    .and_then(|symbol| cx.store.market(symbol))
                    .map_or(b.coin.as_ref(), |m| m.base.as_str());
                let price = self.spot_price(cx, coin, &mut valuation_path);
                if self.core_discovery_incomplete
                    && (b.total != 0.0 || b.hold != 0.0)
                    && (!price.is_finite() || price <= 0.0)
                {
                    cx.log(LogLevel::Error,
                        "Hyperliquid owned spot balance has unavailable valuation; preserving previous account snapshot");
                    return;
                }
                let free = (b.total - b.hold).max(0.0);
                balance.total += b.total * price;
                balance.used += b.hold * price;
                balance.free += free * price;
                currencies.push(CurrencyBalance {
                    currency: b.coin.to_string(),
                    balance: Balance {
                        total: b.total,
                        used: b.hold,
                        free,
                        upnl: 0.0,
                    },
                });
                coins.push(CoinBalance {
                    base: b.coin.to_string(),
                    amount: b.total,
                    notional: b.total * price,
                });
            }
            cx.store.set_balance(balance);
            cx.store.set_currency_balances(currencies);
            cx.store.set_coin_balances(coins);
            self.account.spot_revision += 1;
            return;
        }
        let available = state
            .token_to_available_after_maintenance
            .iter()
            .find(|(token, _)| *token == 0)
            .map(|(_, n)| n.0);
        if self.account.spot_authoritative_ws && (!ws || available.is_none()) {
            return;
        }
        self.account.spot_revision += 1;
        let values = self
            .account
            .core_loaded
            .then(|| {
                self.collateral_values(
                    cx,
                    state
                        .balances
                        .iter()
                        .map(|balance| (balance.coin.as_ref(), balance.total, balance.hold)),
                )
            })
            .flatten();
        let Some((equity, hold, unvalued)) = values else {
            if self.account.pending_spot.is_none() {
                cx.log(
                    LogLevel::Warning,
                    "Hyperliquid collateral valuation unavailable; retaining previous USD balance",
                );
            }
            self.account.pending_spot = Some(PendingSpotCollateral {
                balances: state
                    .balances
                    .iter()
                    .map(|balance| (balance.coin.to_string(), balance.total, balance.hold))
                    .collect(),
                available,
                ws,
            });
            // An unpriced live snapshot still supersedes an older REST read.
            if ws && available.is_some() {
                self.account.spot_authoritative_ws = true;
            }
            return;
        };
        self.account.pending_spot = None;
        self.warn_unvalued(cx, unvalued);
        self.commit_collateral(cx, equity, hold, available, ws);
        self.live_balance(cx);
    }
    pub(crate) fn map_order(&self, cx: &Cx<'_, Pending>, row: &OrderRow<'_>) -> Option<Order> {
        let symbol = self.symbol(&row.coin)?.to_owned();
        let (kind, price) = infer(row);
        Some(Order {
            id: row.oid.to_string(),
            parent_id: None,
            status: OrderStatus::Open,
            symbol,
            kind,
            side: side(&row.side),
            price,
            amount: row.orig_sz,
            filled: (row.orig_sz - row.sz).max(0.0),
            remaining: row.sz,
            reduce_only: row.reduce_only,
            close: None,
            client_order_id: row
                .cloid
                .as_deref()
                .filter(|c| valid_cloid(c))
                .map(str::to_owned),
            time_in_force: tif(&row.tif),
            last_update: cx.now,
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: None,
        })
    }
    pub(crate) fn account_frame(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        if self.user().is_none() {
            return;
        }
        let Ok(frame) = serde_json::from_str::<Envelope>(text) else {
            return;
        };
        let raw = frame.data.get();
        match frame.channel.as_ref() {
            "clearinghouseState" if !self.spot => {
                if let Ok(env) = serde_json::from_str::<ClearingEnvelope>(raw)
                    && let Some(mut state) = env.clearinghouse_state
                {
                    state.time = env.server_time.or(env.time).or(state.time);
                    self.apply_state(cx, &env.dex, &state, false, None);
                } else if let Ok(state) = serde_json::from_str::<Clearinghouse>(raw) {
                    self.apply_state(cx, "", &state, false, None);
                }
            }
            "spotState" | "spotClearinghouseState" => {
                if let Ok(env) = serde_json::from_str::<SpotEnvelope>(raw) {
                    self.apply_spot(cx, &env.spot_state, true);
                } else if let Ok(state) = serde_json::from_str::<SpotState>(raw) {
                    self.apply_spot(cx, &state, true);
                }
            }
            "activeAssetData" if !self.spot => {
                if let Ok(row) = serde_json::from_str::<ActiveAsset>(raw)
                    && let Some(symbol) = self.symbol(&row.coin).map(str::to_owned)
                {
                    let margin = if row.leverage.kind == "cross" {
                        MarginType::Cross
                    } else {
                        MarginType::Isolated
                    };
                    // Until confirmed, untimed asset frames may only echo an acknowledged write.
                    if self.settings_uncertain(&symbol)
                        || cx.store.positions().iter().any(|p| p.symbol == symbol)
                        || (self
                            .account
                            .settings_barriers
                            .get(&symbol)
                            .is_some_and(|b| !b.confirmed)
                            && cx.store.setting(&symbol).is_none_or(|s| {
                                s.leverage != row.leverage.value || s.margin_type != Some(margin)
                            }))
                    {
                        return;
                    }
                    self.apply_asset_setting(cx, &symbol, &row);
                }
            }
            "orderUpdates" if !self.spot => {
                let _ = json::each::<OrderUpdate>(raw, |u| self.order_event(cx, u));
            }
            "userFills" => {
                if let Ok(fills) = serde_json::from_str::<Fills>(raw) {
                    self.fills(cx, fills);
                }
            }
            "user" | "userEvents" if !self.spot => {
                if raw.starts_with('[') {
                    if let Ok(rows) = serde_json::from_str::<Vec<FillRow>>(raw) {
                        self.fills(
                            cx,
                            Fills {
                                is_snapshot: false,
                                fills: rows,
                            },
                        );
                    } else if let Ok(rows) = serde_json::from_str::<Vec<OrderUpdate>>(raw) {
                        for row in rows {
                            self.order_event(cx, row);
                        }
                    }
                } else if let Ok(events) = serde_json::from_str::<UserEvents>(raw) {
                    self.fills(
                        cx,
                        Fills {
                            is_snapshot: events.is_snapshot,
                            fills: events.fills,
                        },
                    );
                    for order in events.orders {
                        self.order_event(cx, order);
                    }
                }
            }
            "userNonFundingLedgerUpdates" if self.spot || self.account.mode != Mode::Standard => {
                self.refresh_spot(cx, false)
            }
            _ => {}
        }
    }
    fn order_event(&mut self, cx: &mut Cx<'_, Pending>, update: OrderUpdate<'_>) {
        let row = update.order;
        let id = row.oid.to_string();
        let Some(symbol) = self.symbol(&row.coin).map(str::to_owned) else {
            return;
        };
        let dex = self.dex_of(cx, &symbol).to_owned();
        *self.account.orders_revision.entry(dex).or_default() += 1;
        if terminal(&update.status) {
            if update.status == "filled" {
                self.amend_source_filled(&id);
            }
            self.account.suppressed.insert(id.clone(), cx.now);
            cx.store.remove_orders(&[&id]);
            if let Some(c) = row.cloid {
                self.account.records.remove(c.as_ref());
            }
            return;
        }
        if !["open", "triggered"].contains(&update.status.as_ref())
            || self
                .account
                .suppressed
                .get(&id)
                .is_some_and(|at| cx.now - *at < 120_000.0)
        {
            return;
        }
        let Some(mut order) = self.map_order(cx, &row) else {
            return;
        };
        self.account
            .records
            .retain(|_, r| cx.now - r.at < 1_800_000.0);
        let record = row
            .cloid
            .as_deref()
            .and_then(|c| self.account.records.get(c))
            .cloned();
        if let Some(record) = record {
            order.kind = record.order.kind;
            order.price = record.order.price;
            order.reduce_only = record.order.reduce_only;
            order.time_in_force = record.order.time_in_force;
            self.account
                .preplacements
                .retain(|p| p.order.client_order_id != order.client_order_id);
        } else if let Some(pending) = self
            .account
            .pending
            .remove(&id)
            .filter(|p| cx.now - p.at < 60_000.0)
        {
            order.kind = pending.order.kind;
            order.price = pending.order.price;
        } else if row.order_type.is_empty() && row.trigger_px.is_none() {
            if let Some(old) = cx.store.order(&id) {
                order.kind = old.kind;
                order.price = old.price;
                order.time_in_force = order.time_in_force.or(old.time_in_force);
            } else if row.reduce_only && !row.is_trigger && !row.coin.contains(':') {
                self.schedule_orders(cx);
                return;
            } else if let Some(record) = self.match_preplacement(
                &symbol,
                order.side,
                row.limit_px,
                row.orig_sz,
                cx.now,
                !row.coin.contains(':'),
            ) {
                order.kind = record.order.kind;
                order.price = record.order.price;
                order.time_in_force = record.order.time_in_force;
            } else if row.coin.contains(':') {
                order.status = OrderStatus::Creating;
                self.schedule_orders(cx);
            }
        }
        order.time_in_force = order
            .time_in_force
            .or_else(|| cx.store.order(&id).and_then(|o| o.time_in_force));
        if order.kind == OrderType::Limit && order.time_in_force.is_none() {
            self.schedule_orders(cx);
        }
        cx.store.upsert_order(order);
    }
    fn match_preplacement(
        &mut self,
        symbol: &str,
        side: OrderSide,
        price: f64,
        amount: f64,
        now: f64,
        strict: bool,
    ) -> Option<Placement> {
        self.account.preplacements.retain(|r| now - r.at < 30_000.0);
        let eligible = |p: &Placement| p.order.symbol == symbol && p.order.side == side;
        if strict {
            let mut matches = self
                .account
                .preplacements
                .iter()
                .enumerate()
                .filter(|(_, p)| {
                    eligible(p)
                        && matches!(p.order.kind, OrderType::TakeProfit | OrderType::StopLoss)
                        && p.order.amount == amount
                });
            let index = matches.next().map(|(i, _)| i)?;
            if matches.next().is_some() {
                return None;
            }
            return self.account.preplacements.remove(index);
        }
        let exact = self
            .account
            .preplacements
            .iter()
            .position(|p| eligible(p) && p.order.price == price);
        let mut by_size = self
            .account
            .preplacements
            .iter()
            .enumerate()
            .filter(|(_, p)| eligible(p) && p.order.amount == amount);
        let first = by_size.next().map(|(i, _)| i);
        let unique = if by_size.next().is_none() {
            first
        } else {
            None
        };
        let index = exact
            .or(unique)
            .or_else(|| self.account.preplacements.iter().position(eligible))?;
        self.account.preplacements.remove(index)
    }
    fn fills(&mut self, cx: &mut Cx<'_, Pending>, mut fills: Fills<'_>) {
        fills
            .fills
            .sort_by(|a, b| a.time.total_cmp(&b.time).then(a.tid.cmp(&b.tid)));
        for fill in fills.fills {
            let Some(symbol) = self.symbol(&fill.coin).map(str::to_owned) else {
                continue;
            };
            let ps = if fill.dir.contains("Long") {
                Some(PositionSide::Long)
            } else if fill.dir.contains("Short") {
                Some(PositionSide::Short)
            } else {
                None
            };
            if self.account.seen_fills.insert(fill.tid) {
                self.account.fill_queue.push_back(fill.tid);
                if self.account.fill_queue.len() > 20_000
                    && let Some(id) = self.account.fill_queue.pop_front()
                {
                    self.account.seen_fills.remove(&id);
                }
                if let Some(ps) = ps {
                    let key = (symbol.clone(), ps);
                    if fill.dir.contains("Open") && fill.start_position == Some(0.0) {
                        self.account.realized.insert(key.clone(), 0.0);
                    }
                    if fill.dir.contains("Close") {
                        *self.account.realized.entry(key.clone()).or_default() +=
                            fill.closed_pnl - fill.fee;
                        let realized = self.account.realized[&key];
                        cx.store.update_positions(|p| {
                            if p.symbol == symbol && p.side == ps {
                                p.realized_pnl = Some(realized);
                            }
                        });
                    }
                }
            }
            if fills.is_snapshot || (self.spot && self.account.spot_fill_tids.contains(&fill.tid)) {
                continue;
            }
            if self.account.notify_fills.insert(fill.tid) {
                self.account.notify_queue.push_back(fill.tid);
                if self.account.notify_queue.len() > 500
                    && let Some(id) = self.account.notify_queue.pop_front()
                {
                    self.account.notify_fills.remove(&id);
                }
                cx.fill(Fill {
                    order_id: fill.oid.to_string(),
                    symbol: symbol.clone(),
                    side: side(&fill.side),
                    price: fill.px,
                    amount: fill.sz,
                });
                if self.spot {
                    self.record_spot_fill(&fill, &symbol);
                    let id = fill.oid.to_string();
                    self.amend_source_filled(&id);
                    if let Some(order) = cx.store.order(&id) {
                        let remaining = (order.remaining - fill.sz).max(0.0);
                        if remaining == 0.0 {
                            cx.store.remove_orders(&[&id]);
                        } else {
                            cx.store.update_order(&id, |o| {
                                o.remaining = remaining;
                                o.filled = (o.amount - remaining).max(0.0);
                            });
                        }
                    }
                    self.refresh_spot(cx, true);
                } else {
                    self.optimistic_fill(cx, &symbol, &fill);
                }
            }
        }
        self.live_balance(cx);
    }
    fn optimistic_fill(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str, fill: &FillRow<'_>) {
        if fill.coin.contains(':') {
            let dex = fill.coin.split_once(':').unwrap().0;
            self.account
                .position_time
                .entry(symbol.into())
                .and_modify(|time| *time = time.max(fill.time))
                .or_insert(fill.time);
            *self.account.state_revision.entry(dex.into()).or_default() += 1;
            self.account.state_dirty.insert(dex.into());
            if self.account.state_refresh_timer.is_none() {
                self.account.state_refresh_timer = Some(cx.set_timer(350.0));
            }
            return;
        }
        let Some(start) = fill.start_position else {
            return;
        };
        if self
            .account
            .snapshot_time
            .get(symbol)
            .is_some_and(|t| *t >= fill.time)
        {
            return;
        }
        if self
            .account
            .last_fill
            .get(symbol)
            .is_some_and(|(time, tid)| {
                *time > fill.time || (*time == fill.time && *tid >= fill.tid)
            })
        {
            return;
        }
        self.account
            .last_fill
            .insert(symbol.into(), (fill.time, fill.tid));
        let end = start + fill.sz * if fill.side == "B" { 1.0 } else { -1.0 };
        let old = cx
            .store
            .positions()
            .iter()
            .find(|p| p.symbol == symbol)
            .cloned();
        let mut positions = cx
            .store
            .positions()
            .iter()
            .filter(|p| p.symbol != symbol)
            .cloned()
            .collect::<Vec<_>>();
        if end.abs() > 1e-10 {
            let entry = if let Some(old) = &old {
                if start * end <= 0.0 {
                    fill.px
                } else if end.abs() > start.abs() {
                    (old.entry_price * start.abs() + fill.px * fill.sz) / end.abs()
                } else {
                    old.entry_price
                }
            } else {
                fill.px
            };
            let setting = cx.store.setting(symbol).unwrap_or(MarketSetting {
                leverage: 20.0,
                buy_leverage: None,
                sell_leverage: None,
                margin_type: Some(MarginType::Cross),
            });
            let ps = if end > 0.0 {
                PositionSide::Long
            } else {
                PositionSide::Short
            };
            positions.push(Position {
                id: symbol.into(),
                symbol: symbol.into(),
                side: ps,
                entry_price: entry,
                contracts: end.abs(),
                notional: fill.px * end.abs(),
                leverage: setting.leverage,
                margin_type: setting.margin_type,
                position_mode: Some(PositionMode::Oneway),
                unrealized_pnl: (fill.px - entry) * end,
                realized_pnl: self.account.realized.get(&(symbol.into(), ps)).copied(),
                liquidation_price: old.as_ref().map_or(0.0, |p| p.liquidation_price),
                margin_size: old.and_then(|p| p.margin_size),
                last_update: Some(fill.time),
            });
        }
        cx.store.set_positions(positions);
        self.account.position_time.insert(symbol.into(), fill.time);
        *self
            .account
            .state_revision
            .entry(String::new())
            .or_default() += 1;
        if end.abs() <= 1e-10 {
            self.schedule_orders(cx);
        }
    }
}
pub(crate) fn side(side: &str) -> OrderSide {
    if side == "B" {
        OrderSide::Buy
    } else {
        OrderSide::Sell
    }
}
pub(crate) fn tif(tif: &str) -> Option<TimeInForce> {
    match tif {
        "Alo" => Some(TimeInForce::PostOnly),
        "Gtc" => Some(TimeInForce::GoodTillCancel),
        _ => None,
    }
}
pub(crate) fn valid_cloid(id: &str) -> bool {
    id.len() == 34
        && id.starts_with("0x")
        && id[2..]
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn infer(row: &OrderRow<'_>) -> (OrderType, f64) {
    if row.order_type == "Limit" {
        return (OrderType::Limit, row.limit_px);
    }
    if row.order_type == "Take Profit Market" || (row.trigger_px.is_some() && row.tpsl == "tp") {
        return (OrderType::TakeProfit, row.trigger_px.unwrap_or(0.0));
    }
    if row.order_type == "Stop Market" || row.trigger_px.is_some() {
        return (OrderType::StopLoss, row.trigger_px.unwrap_or(0.0));
    }
    if row.limit_px != 0.0 {
        (OrderType::Limit, row.limit_px)
    } else {
        (OrderType::Market, 0.0)
    }
}
pub(crate) fn terminal(status: &str) -> bool {
    status == "filled"
        || status == "canceled"
        || status == "rejected"
        || status.ends_with("Canceled")
        || status.ends_with("Rejected")
}
