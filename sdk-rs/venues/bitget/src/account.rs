use crate::wire::*;
use crate::{BitgetFutures, Pending, paths, sign};
use safe_cex_engine::Level;
use safe_cex_engine::coverage::{RowAdmission, row_skipped};
use safe_cex_engine::credential_selection::{
    CredentialRole, CredentialUnavailable, PrimaryReason, SlotPolicy, Slots,
};
use safe_cex_engine::execution::{Arithmetic, CumulativePrice, Cursor, Report};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::*;
use safe_cex_engine::request_context::{Credential, Scope};
use safe_cex_engine::signing::Identity;
use safe_cex_engine::socket::{Config, Supervisor};
use safe_cex_engine::{Cx, ErrorKind, Fill, Method, Response, TimerId, VenueError};
use serde::{Deserialize, Serialize, de::IgnoredAny};
use std::collections::{HashMap, HashSet};

/// Private REST pause after a 418/429 that carries no usable `Retry-After`.
/// While the private stream is live, REST re-reads the account only this often.
const SAFETY_RECONCILE_MS: f64 = 60_000.0;
const REST_PAUSE_MS: f64 = 30_000.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OrderSource {
    Normal,
    Protection,
    Trailing,
}

pub enum Request {
    Probe,
    /// The settings re-read a frame-fed page asks of its data source: UTA
    /// settings, or a Classic symbol's account config. Never a write.
    Settings(Option<String>),
    Positions {
        rows: Vec<Position>,
        skipped: RowAdmission<PositionKey>,
        coins: std::collections::VecDeque<String>,
        started: f64,
    },
    Balance {
        row: Option<Amounts>,
        coins: Vec<InverseAsset>,
        version: u64,
        invalid: bool,
    },
    UtaBalance {
        version: u64,
    },
    Orders(Box<OrderSnapshot>),
}

pub struct OrderSnapshot {
    version: u64,
    source: OrderSource,
    cursor: Option<String>,
    cursors: HashSet<String>,
    pages: u32,
    page_count: usize,
    rows: Vec<Order>,
    skipped: RowAdmission<String>,
    plans: Vec<(String, String)>,
    trailing: Vec<(String, (String, f64))>,
    started: f64,
    stream_revision: u64,
    virtual_only: bool,
}

/// Symbol and side of a position row; a `None` side covers both legs.
type PositionKey = (String, Option<PositionSide>);

#[derive(Clone)]
pub struct InverseAsset {
    coin: CoinBalance,
    free: f64,
    upnl: f64,
    used: f64,
    total: f64,
    priced: bool,
}

#[derive(Clone, Copy)]
pub struct Amounts {
    equity: f64,
    free: f64,
    upnl: f64,
}

struct WsPosition {
    at: Option<f64>,
    update: Option<f64>,
    /// Observation only: never replaces or renews valid financial timestamps.
    rejected_at: Option<f64>,
}

#[derive(Default)]
pub struct Account {
    pub(crate) supervisor: Supervisor,
    pub generation: u64,
    pub orders_version: u64,
    pub config_at: Option<f64>,
    pub uta: bool,
    pub detected: bool,
    pub authenticated: bool,
    pub needs_recovery: bool,
    /// Egress refusals (40018/40024) in the current private-login streak.
    route_refusals: u8,
    /// The last refusal's native code and text, reported once on exhaustion.
    route_refused: Option<(String, String)>,
    /// The next private connect asks the host to leave the refused route.
    pub reject_route: bool,
    /// The venue answered this private connection's login: its close says
    /// nothing about the route.
    pub login_answered: bool,
    /// Private connections in a row that opened but closed before any login
    /// answer; two mean the route is unusable (TS #8093).
    unanswered_closes: u8,
    /// Every route was refused: the private socket stops, as TS #8093.
    pub route_exhausted: bool,
    poll: Option<TimerId>,
    /// Backs off the settings probe that gates private startup; transient failures never end it.
    probe: safe_cex_engine::startup::Backoff<()>,
    signature_rejections: u8,
    pub startup_rejected: bool,
    tick: u32,
    positions_in_flight: bool,
    balance_in_flight: bool,
    orders_in_flight: bool,
    orders_dirty: bool,
    positions_timer: Option<TimerId>,
    balance_timer: Option<TimerId>,
    orders_timer: Option<TimerId>,
    balance_version: u64,
    inverse_assets: Vec<InverseAsset>,
    inverse_unpriced_warned: bool,
    full_positions_at: Option<f64>,
    /// After partial traffic, a prior clean snapshot only fences older reads.
    full_positions_fence_only: bool,
    full_position_ids: HashSet<String>,
    ws_positions: HashMap<String, WsPosition>,
    terminal: HashMap<String, f64>,
    opened: HashMap<String, f64>,
    pub plans: HashMap<String, String>,
    pub trailing: HashMap<String, (String, f64)>,
    ws_order_clock: u64,
    ws_order_revisions: HashMap<String, u64>,
    executed: HashSet<String>,
    executed_order: std::collections::VecDeque<String>,
    filled: HashMap<String, (Cursor, f64)>,
    rest_paused_until: f64,
    last_payload: f64,
    write_check: Option<(TimerId, f64)>,
    degraded: bool,
    /// When positions, balance and orders were last read over REST while live;
    /// `None` after each (re)login, so the first live poll covers the gap.
    last_account_read_at: Option<f64>,
}

impl Account {
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
}

impl BitgetFutures {
    pub(crate) fn private_enabled(&self) -> bool {
        !self.options.public_data_only
            && self
                .options
                .credentials
                .as_ref()
                .is_some_and(|id| id.validate().is_ok())
    }

    /// The private stream is logged in and has not been found silent or unreachable.
    pub(crate) fn stream_live(&self) -> bool {
        self.account.authenticated && !self.account.degraded
    }

    pub(crate) fn primary_balance_read(&self, path: &str) -> bool {
        self.account.degraded
            && matches!(
                path.split('?').next().unwrap_or(path),
                paths::BALANCE | paths::UTA_ASSETS
            )
    }

    /// Both keys, reachable only through a declared role.
    fn credential_slots(&self) -> Slots<&Identity> {
        Slots::new(
            self.options.credentials.as_ref(),
            self.options.read_credentials.as_ref(),
            SlotPolicy::default(),
        )
    }

    /// The read key serves every GET except a degraded balance read, which
    /// keeps the primary key (and primary egress); the primary key is the fallback.
    fn read_role(&self, path: &str) -> CredentialRole {
        if self.primary_balance_read(path) {
            CredentialRole::PrimaryRead {
                reason: PrimaryReason::DegradedRead,
                scope: None,
            }
        } else {
            CredentialRole::Read { scope: None }
        }
    }

    pub(crate) fn signing_identity(&self, method: Method, path: &str) -> Option<&Identity> {
        let role = if method == Method::Get {
            self.read_role(path)
        } else {
            CredentialRole::Mutation
        };
        self.credential_slots()
            .select(role)
            .ok()
            .map(|(_, identity)| *identity)
    }

    /// The managed read credential and the identity that signs it, from one selection.
    pub(crate) fn read_selection(&self, path: &str) -> (Option<Credential>, Option<Identity>) {
        match self.credential_slots().select(self.read_role(path)) {
            Ok((selection, identity)) => (
                Some(selection.credential(identity.credential.clone())),
                Some((*identity).clone()),
            ),
            Err(CredentialUnavailable::Missing) => (None, None),
        }
    }

    /// One physical account read under its own Background Read Context. An
    /// issuance or queue refusal is a pre-send failure, as a provider refusal is.
    fn account_request(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        path: String,
        at: Option<ArrayAt>,
        request: Request,
    ) {
        // Frame-fed: account state arrives as frames; this engine reads nothing.
        if cx.frame_fed() {
            return;
        }
        let (credential, identity) = self.read_selection(&path);
        let policy = crate::read_policy(credential);
        let context = match cx.http_context(Scope::Background, policy) {
            Ok(context) => context,
            Err(reason) => {
                return self.account_sign_failed(cx, request, &reason.venue_error().message);
            }
        };
        let recipe = sign::Http::new(identity, path.clone());
        let pending = Pending::Account {
            generation: self.account.generation,
            context: context.clone(),
            request,
        };
        match cx.request_signed_managed(
            context,
            None,
            Method::Get,
            format!("{}{path}", paths::REST),
            None,
            Box::new(recipe),
            pending,
        ) {
            Ok(id) => {
                if let Some(at) = at {
                    cx.stream_response(id, at);
                }
            }
            // Synchronous, so no trade resume here: the caller's callback continues.
            Err((pending, reason)) => {
                if let Pending::Account {
                    context, request, ..
                } = pending
                {
                    let _ = cx.finish_http_context(&context);
                    self.account_sign_failed(cx, request, &reason.venue_error().message);
                }
            }
        }
    }

    pub(crate) fn start_private(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.private_enabled() {
            cx.store.set_balance(Balance::default());
            cx.store.set_positions(vec![]);
            cx.store.set_orders(vec![]);
            return;
        }
        if self
            .options
            .credentials
            .as_ref()
            .and_then(|id| id.passphrase.as_ref())
            .is_none_or(|key| key.0.is_empty())
        {
            let error =
                VenueError::new("Bitget requires a passphrase").kind(ErrorKind::InvalidInput);
            self.reject_startup(cx, error, false);
            return;
        }
        self.account_request(cx, paths::SETTINGS.to_owned(), None, Request::Probe);
    }

    fn private_detected(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account.detected = true;
        self.account.signature_rejections = 0;
        self.account.probe.succeeded(cx, &());
        self.connect(cx, 3);
        self.refresh_positions(cx);
        self.refresh_balance(cx);
        self.refresh_orders(cx, false);
        if self.account.poll.is_none() {
            self.account.poll = Some(cx.set_poll_timer(2_000.0));
        }
    }

    pub(crate) fn private_open(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        socket: safe_cex_engine::SocketId,
    ) {
        self.account.authenticated = false;
        if let Some(identity) = self.options.credentials.clone() {
            cx.send_signed(
                socket,
                String::new(),
                Box::new(sign::WsLogin::new(
                    identity,
                    if self.account.uta {
                        paths::UTA_PRIVATE
                    } else {
                        paths::PRIVATE
                    }
                    .into(),
                )),
            );
        }
        self.refresh_positions(cx);
        self.refresh_balance(cx);
        self.refresh_orders(cx, false);
    }

    fn private_subscribe(&self, cx: &mut Cx<'_, Pending>) {
        let Some(socket) = self.sockets[3].socket else {
            return;
        };
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Topic<'a> {
            inst_type: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            channel: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            topic: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            inst_id: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            coin: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            symbol: Option<&'a str>,
        }
        #[derive(Serialize)]
        struct Subscribe<'a> {
            op: &'static str,
            args: Vec<Topic<'a>>,
        }
        let product = self.product().to_uppercase();
        let args = if self.account.uta {
            ["account", "position", "order", "fill", "strategy-order"]
                .into_iter()
                .map(|topic| Topic {
                    inst_type: "UTA",
                    symbol: (topic == "strategy-order").then_some("default"),
                    channel: None,
                    topic: Some(topic),
                    inst_id: None,
                    coin: None,
                })
                .collect()
        } else {
            ["account", "positions", "orders", "orders-algo"]
                .into_iter()
                .map(|channel| Topic {
                    inst_type: &product,
                    symbol: None,
                    channel: Some(channel),
                    topic: None,
                    inst_id: (channel != "account").then_some("default"),
                    coin: (channel == "account").then_some("default"),
                })
                .collect()
        };
        cx.send(
            socket,
            serde_json::to_string(&Subscribe {
                op: "subscribe",
                args,
            })
            .expect("private subscription"),
        );
    }

    pub(crate) fn refresh_positions(&mut self, cx: &mut Cx<'_, Pending>) {
        if cx.frame_fed()
            || !self.account.detected
            || self.account.positions_in_flight
            || cx.now < self.account.rest_paused_until
        {
            return;
        }
        self.account.positions_in_flight = true;
        let mut coins = std::collections::VecDeque::new();
        if self.inverse() {
            for market in cx.store.markets() {
                if !coins.contains(&market.base) {
                    coins.push_back(market.base.clone());
                }
            }
        }
        let coin = coins.pop_front().unwrap_or_else(|| self.coin().to_owned());
        let path = if self.account.uta {
            format!(
                "{}?category={}",
                paths::UTA_POSITIONS,
                self.product().to_uppercase()
            )
        } else {
            format!(
                "{}?productType={}&marginCoin={}",
                paths::POSITIONS,
                self.product(),
                sign::encode(&coin)
            )
        };
        let at = if self.account.uta {
            ArrayAt::Envelope(&[&["data", "list"]])
        } else {
            ArrayAt::Envelope(&[&["data"]])
        };
        self.account_request(
            cx,
            path,
            Some(at),
            Request::Positions {
                rows: vec![],
                skipped: RowAdmission::default(),
                coins,
                started: cx.now,
            },
        );
    }

    pub(crate) fn refresh_balance(&mut self, cx: &mut Cx<'_, Pending>) {
        if cx.frame_fed()
            || !self.account.detected
            || self.account.balance_in_flight
            || cx.now < self.account.rest_paused_until
        {
            return;
        }
        self.account.balance_in_flight = true;
        let version = self.account.balance_version;
        if self.account.uta {
            self.account_request(
                cx,
                paths::UTA_ASSETS.to_owned(),
                None,
                Request::UtaBalance { version },
            );
        } else {
            self.account_request(
                cx,
                format!("{}?productType={}", paths::BALANCE, self.product()),
                Some(ArrayAt::Envelope(&[&["data"]])),
                Request::Balance {
                    row: None,
                    coins: vec![],
                    version,
                    invalid: false,
                },
            );
        }
    }

    pub(crate) fn refresh_orders(&mut self, cx: &mut Cx<'_, Pending>, virtual_only: bool) {
        if cx.frame_fed()
            || !self.account.detected
            || self.account.orders_in_flight
            || cx.now < self.account.rest_paused_until
        {
            return;
        }
        self.account.orders_in_flight = true;
        self.account.orders_dirty = false;
        self.order_page(
            cx,
            Box::new(OrderSnapshot {
                version: self.account.orders_version,
                source: OrderSource::Normal,
                cursor: None,
                cursors: HashSet::new(),
                pages: 0,
                page_count: 0,
                rows: vec![],
                skipped: RowAdmission::default(),
                plans: vec![],
                trailing: vec![],
                started: cx.now,
                stream_revision: self.order_revision_clock(),
                virtual_only,
            }),
        );
    }

    fn order_page(&mut self, cx: &mut Cx<'_, Pending>, mut snapshot: Box<OrderSnapshot>) {
        snapshot.page_count = 0;
        let source = snapshot.source;
        let mut path = if self.account.uta {
            if source == OrderSource::Normal {
                format!(
                    "{}?category={}&limit=100",
                    paths::UTA_ORDERS,
                    self.product().to_uppercase()
                )
            } else {
                format!(
                    "{}?category={}&type={}",
                    paths::UTA_STRATEGIES,
                    self.product().to_uppercase(),
                    if source == OrderSource::Trailing {
                        "trailing_stop"
                    } else {
                        "tpsl"
                    }
                )
            }
        } else if source == OrderSource::Normal {
            format!("{}?productType={}&limit=200", paths::ORDERS, self.product())
        } else {
            format!(
                "{}?productType={}&planType={}&limit=200",
                paths::PLANS,
                self.product(),
                if source == OrderSource::Protection {
                    "profit_loss"
                } else {
                    "track_plan"
                }
            )
        };
        if let Some(cursor) = &snapshot.cursor {
            path.push_str(&format!(
                "&{}={}",
                if self.account.uta {
                    "cursor"
                } else {
                    "idLessThan"
                },
                sign::encode(cursor)
            ));
        }
        let at = ArrayAt::Envelope(&[&["data", "entrustedList"], &["data", "list"], &["data"]]);
        self.account_request(cx, path, Some(at), Request::Orders(snapshot));
    }

    pub(crate) fn account_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        match request {
            Request::Positions { rows, skipped, .. } => {
                match serde_json::from_str::<PositionRow>(item)
                    .map_err(|_| "malformed record".to_owned())
                    .and_then(|row| self.map_position(cx, &row))
                {
                    Ok(p) => rows.push(p),
                    Err(error) => {
                        let key = self.position_key(item);
                        let message = format!(
                            "skipped Bitget position record {}: {error}",
                            key.as_ref().map_or("without symbol", |(s, _)| s.as_str())
                        );
                        skipped.skip(cx, key, Level::Warn, message);
                    }
                }
            }
            Request::Balance {
                row,
                coins,
                invalid,
                ..
            } => match serde_json::from_str::<AccountRow>(item) {
                Ok(value) if self.inverse() => match self.inverse_asset(cx, &value) {
                    Some(asset) if !coins.iter().any(|a| a.coin.base == asset.coin.base) => {
                        coins.push(asset)
                    }
                    Some(_) => *invalid = true,
                    None => *invalid = true,
                },
                Ok(value) if value.margin_coin == self.coin() || value.coin == self.coin() => {
                    *row = classic_amounts(&value);
                    *invalid |= row.is_none();
                }
                Ok(_) => {}
                Err(_) => *invalid = true,
            },
            Request::Orders(snapshot) => {
                snapshot.page_count += 1;
                match serde_json::from_str::<OrderRow>(item)
                    .map_err(|_| "malformed record".to_owned())
                    .and_then(|mut row| {
                        if self.account.uta
                            && snapshot.source != OrderSource::Normal
                            && raw_status(&row).is_empty()
                        {
                            row.status = std::borrow::Cow::Borrowed("live");
                        }
                        if self.account.uta
                            && snapshot.source == OrderSource::Trailing
                            && row.strategy_type.is_empty()
                        {
                            row.strategy_type = std::borrow::Cow::Borrowed("trailing_stop");
                        }
                        let orders = self.map_order(cx, &row)?;
                        if let Some(trailing) = trailing_state(&row) {
                            for order in &orders {
                                snapshot.trailing.push((order.id.clone(), trailing.clone()));
                            }
                        }
                        if self.account.uta && snapshot.source != OrderSource::Normal {
                            for order in &orders {
                                snapshot
                                    .plans
                                    .push((order.id.clone(), "uta_tpsl".to_owned()));
                            }
                        }
                        if let Some(plan) = plan_type(&row.plan_type) {
                            for order in &orders {
                                snapshot.plans.push((order.id.clone(), plan.to_owned()));
                            }
                        }
                        Ok(orders)
                    }) {
                    Ok(orders) => snapshot.rows.extend(orders),
                    Err(error) => {
                        let id = order_row_id(item);
                        let message = format!(
                            "skipped Bitget order record {}: {error}",
                            id.as_deref().unwrap_or("without id")
                        );
                        snapshot.skipped.skip(cx, id, Level::Warn, message);
                    }
                }
            }
            _ => {}
        }
    }

    pub(crate) fn account_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        let envelope = serde_json::from_str::<Envelope<IgnoredAny>>(response.body);
        let pause = safe_cex_engine::startup::rate_limit_pause_ms(&response, REST_PAUSE_MS);
        if pause > 0.0 {
            self.account.rest_paused_until = self.account.rest_paused_until.max(cx.now + pause);
        }
        if let Request::Probe = request {
            self.probe_response(cx, response, envelope.ok(), pause);
            return;
        }
        if envelope
            .as_ref()
            .is_ok_and(|header| header.code.0 == Some(40085.0))
            && self.promote_uta(cx)
        {
            return;
        }
        let valid = envelope
            .map_err(|e| e.to_string())
            .and_then(|header| header.checked().map(|_| ()));
        if let Err(error) = valid {
            self.account_refused(cx, request, read_error(&response, error));
            return;
        }
        if let Request::Settings(symbol) = &request {
            self.settings_response(cx, symbol.as_deref(), response.body);
            return;
        }
        if response.status != 200 {
            let error = read_error(&response, format!("Bitget HTTP {}", response.status));
            self.account_refused(cx, request, error);
            return;
        }
        match request {
            Request::Positions {
                rows,
                mut skipped,
                mut coins,
                started,
            } => {
                if let Some(coin) = coins.pop_front() {
                    self.account_request(
                        cx,
                        format!(
                            "{}?productType={}&marginCoin={}",
                            paths::POSITIONS,
                            self.product(),
                            sign::encode(&coin)
                        ),
                        Some(ArrayAt::Envelope(&[&["data"]])),
                        Request::Positions {
                            rows,
                            skipped,
                            coins,
                            started,
                        },
                    );
                    return;
                }
                self.account.positions_in_flight = false;
                let mut rows = self.merge_positions(cx, rows, started);
                self.coherent_position_modes(cx, &mut rows, &mut skipped, true);
                for p in &rows {
                    if !self
                        .account
                        .ws_positions
                        .get(&p.id)
                        .is_some_and(|mark| mark.rejected_at.is_some_and(|at| at >= started))
                    {
                        self.position_setting(cx, p);
                    }
                }
                for current in cx.store.positions() {
                    if !rows.iter().any(|p| p.id == current.id)
                        && [Some(current.side), None]
                            .into_iter()
                            .any(|side| !skipped.allows_absence(&(current.symbol.clone(), side)))
                    {
                        rows.push(current.clone());
                    }
                }
                cx.store
                    .set_positions(rows.into_iter().filter(|p| p.contracts > 0.0).collect());
            }
            Request::Balance {
                row,
                coins,
                version,
                invalid: false,
            } => {
                self.account.balance_in_flight = false;
                if self.account.balance_version == version {
                    if self.inverse() && !coins.is_empty() {
                        self.account.inverse_assets = coins;
                        self.apply_inverse_balance(cx);
                    } else if let Some(row) = row {
                        self.apply_balance(cx, row);
                    } else {
                        cx.store.set_balance(cx.store.balance().clone());
                    }
                }
            }
            Request::UtaBalance { version } => {
                self.account.balance_in_flight = false;
                match serde_json::from_str::<Envelope<UtaBalance>>(response.body)
                    .map_err(|e| e.to_string())
                    .and_then(Envelope::checked)
                {
                    Ok(row) => {
                        if self.account.balance_version == version
                            && let Some(amounts) = uta_amounts(&row, self.coin())
                        {
                            self.apply_balance(cx, amounts);
                        }
                    }
                    Err(error) => cx
                        .retried_failure_error("bitget.account:744", &read_error(&response, error)),
                }
            }
            Request::Orders(mut snapshot) => {
                if snapshot.version != self.account.orders_version {
                    self.account.orders_in_flight = false;
                    self.refresh_orders(cx, false);
                    return;
                }
                #[derive(Deserialize)]
                #[serde(untagged)]
                enum Meta {
                    Object(PageMeta),
                    Array(Vec<IgnoredAny>),
                }
                let meta = serde_json::from_str::<Envelope<Meta>>(response.body)
                    .ok()
                    .and_then(|e| e.data);
                let cursor = match meta {
                    Some(Meta::Object(meta)) => meta.end_id.or(meta.cursor),
                    Some(Meta::Array(rows)) => {
                        debug_assert!(rows.is_empty());
                        None
                    }
                    None => None,
                }
                .filter(|cursor| !cursor.is_empty());
                if snapshot.page_count > 0
                    && let Some(cursor) = cursor
                {
                    if !snapshot.cursors.insert(cursor.clone()) {
                        self.account_failed(
                            cx,
                            Request::Orders(snapshot),
                            "Bitget open-order cursor repeated; incomplete snapshot",
                        );
                        return;
                    }
                    snapshot.pages += 1;
                    if snapshot.pages >= 20 {
                        self.account_failed(
                            cx,
                            Request::Orders(snapshot),
                            "Bitget open-order page bound reached",
                        );
                        return;
                    }
                    snapshot.cursor = Some(cursor);
                    self.order_page(cx, snapshot);
                    return;
                }
                let next = match snapshot.source {
                    OrderSource::Normal if !snapshot.virtual_only => Some(OrderSource::Protection),
                    OrderSource::Protection => Some(OrderSource::Trailing),
                    _ => None,
                };
                if let Some(next) = next {
                    snapshot.source = next;
                    snapshot.pages = 0;
                    snapshot.cursor = None;
                    snapshot.cursors.clear();
                    self.order_page(cx, snapshot);
                } else {
                    self.account.orders_in_flight = false;
                    for (id, state) in snapshot.trailing {
                        if self.order_metadata_current(
                            cx,
                            &id,
                            snapshot.started,
                            snapshot.stream_revision,
                        ) {
                            self.account.trailing.insert(id, state);
                        }
                    }
                    for (id, plan) in snapshot.plans {
                        if self.order_metadata_current(
                            cx,
                            &id,
                            snapshot.started,
                            snapshot.stream_revision,
                        ) {
                            self.account.plans.insert(id, plan);
                        }
                    }
                    // Only a clean census can prove an uncertain placement absent.
                    let clean = snapshot.skipped.rejected() == 0;
                    self.commit_orders(
                        cx,
                        snapshot.rows,
                        &snapshot.skipped,
                        snapshot.virtual_only,
                        snapshot.started,
                        snapshot.stream_revision,
                    );
                    if !snapshot.virtual_only && clean {
                        cx.orders_reconciled(None);
                    }
                    if self.account.orders_dirty {
                        self.refresh_orders(cx, false);
                    }
                }
            }
            Request::Probe => {}
            request => self.account_failed(cx, request, "invalid Bitget account record"),
        }
    }

    fn clear_in_flight(&mut self, request: &Request) {
        match request {
            Request::Positions { .. } => self.account.positions_in_flight = false,
            Request::Balance { .. } | Request::UtaBalance { .. } => {
                self.account.balance_in_flight = false
            }
            Request::Orders(_) => self.account.orders_in_flight = false,
            _ => {}
        }
    }

    pub(crate) fn account_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        self.account_refused(cx, request, VenueError::new(error));
    }

    /// A failed account read, still `request.failed`, carrying its kind fields.
    fn account_refused(&mut self, cx: &mut Cx<'_, Pending>, request: Request, error: VenueError) {
        self.clear_in_flight(&request);
        cx.retried_failure_error("bitget.account:870", &error);
    }

    pub(crate) fn account_sign_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        match request {
            // Provider refusal is not transport evidence for Classic fallback.
            Request::Probe => self.probe_failed(cx, error, 0.0),
            request => self.account_failed(cx, request, error),
        }
    }

    pub(crate) fn account_transport_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        if matches!(request, Request::Probe) {
            self.probe_transport_failed(cx, error);
        } else {
            self.account_refused(cx, request, VenueError::new(error).kind(ErrorKind::Network));
        }
    }

    fn probe_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        response: Response<'_>,
        envelope: Option<Envelope<IgnoredAny>>,
        pause_ms: f64,
    ) {
        let Some(header) = envelope else {
            if response.status >= 500 {
                return self.probe_transport_failed(cx, "Bitget settings transport failure");
            }
            let error = format!("Bitget settings HTTP {}", response.status);
            return self.probe_failed(cx, &error, pause_ms);
        };
        match header.code.0 {
            Some(40084.0) => {
                self.account.uta = false;
                self.private_detected(cx);
            }
            Some(0.0) if response.status == 200 => {
                if self.options.testnet || self.inverse() {
                    let error = VenueError::new("Bitget UTA is unsupported on inverse or testnet")
                        .kind(ErrorKind::Unsupported);
                    return self.reject_startup(cx, error, false);
                }
                match serde_json::from_str::<Envelope<UtaSettings>>(response.body)
                    .map_err(|e| e.to_string())
                    .and_then(Envelope::checked)
                {
                    Ok(settings) => {
                        self.account.uta = true;
                        self.apply_settings(cx, &settings);
                        self.private_detected(cx);
                    }
                    Err(error) => self.probe_failed(cx, &error, 0.0),
                }
            }
            code => {
                let code = code.unwrap_or(0.0);
                if !self.startup_ready(cx)
                    && let Some(kind) = key_rejection(code, &header.msg)
                {
                    let signature = code == 40009.0;
                    if signature {
                        self.account.signature_rejections += 1;
                    }
                    if !signature || self.account.signature_rejections >= 2 {
                        let read_key = self.options.read_credentials.is_some();
                        let which = if read_key {
                            "read-only API key"
                        } else {
                            "API key"
                        };
                        let error = rejection(which, code, &header.msg, kind);
                        return self.reject_startup(
                            cx,
                            error,
                            kind == ErrorKind::AuthInvalid && !read_key,
                        );
                    }
                } else {
                    self.account.signature_rejections = 0;
                }
                self.probe_failed(cx, &format!("Bitget {code}: {}", header.msg), pause_ms);
            }
        }
    }

    /// Classic is the safe route when the first probe cannot reach Bitget; a
    /// confirmed UTA account keeps probing instead of downgrading.
    fn probe_transport_failed(&mut self, cx: &mut Cx<'_, Pending>, error: &str) {
        if self.account.uta {
            self.probe_failed(cx, error, 0.0);
        } else {
            self.private_detected(cx);
        }
    }

    /// Transient probe failures retry with backoff until detection succeeds,
    /// never inside the rate-limit window a 418/429 answer set.
    fn probe_failed(&mut self, cx: &mut Cx<'_, Pending>, error: &str, pause_ms: f64) {
        cx.notice(Level::Warn, code::REQUEST_FAILED, error);
        if !self.account.startup_rejected {
            self.account.probe.failed_after(cx, (), pause_ms);
        }
    }

    fn startup_ready(&self, cx: &Cx<'_, Pending>) -> bool {
        let loaded = cx.store.loaded();
        loaded.balance && loaded.positions && loaded.orders
    }

    /// Fails startup once and retires every private retry, read or socket.
    fn reject_startup(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        error: VenueError,
        trading_key_refused: bool,
    ) {
        if std::mem::replace(&mut self.account.startup_rejected, true) {
            return;
        }
        self.account.probe.succeeded(cx, &());
        self.account.detected = false;
        self.account.generation += 1;
        for timer in [
            self.account.poll.take(),
            self.account.positions_timer.take(),
            self.account.balance_timer.take(),
            self.account.orders_timer.take(),
        ]
        .into_iter()
        .flatten()
        {
            cx.clear_timer(timer);
        }
        self.account.supervisor.cancel(cx);
        self.account.authenticated = false;
        self.sockets[3].open = false;
        if let Some(socket) = self.sockets[3].socket.take() {
            cx.close(socket);
        }
        cx.notice(
            Level::Error,
            code::for_startup_error(&error),
            &error.message,
        );
        cx.startup_failed(error);
        // As Bybit: a refused trading key also stops the host's login loop.
        if trading_key_refused {
            cx.deactivate();
        }
    }

    /// A login refused for this egress (40018 IP-bound key, 40024 restricted
    /// location): leave the route. Only a streak's first refusal skips backoff,
    /// so a host that ignores `reject_route` never redials hot.
    fn refuse_route(&mut self, cx: &mut Cx<'_, Pending>, code: f64, msg: &str) {
        let code = safe_cex_engine::num::format(code);
        cx.notice(
            Level::Warn,
            code::AUTH_REJECTED,
            format!(
                "Bitget private WS login refused this egress IP ({code}); trying the next route"
            ),
        );
        self.account.route_refused = Some((code, msg.to_owned()));
        self.account.route_refusals = self.account.route_refusals.saturating_add(1);
        self.account.reject_route = true;
        let Some(socket) = self.sockets[3].socket.take() else {
            return;
        };
        cx.close(socket);
        self.sockets[3].open = false;
        if self.account.route_refusals == 1 {
            self.account.supervisor.retry_now(cx);
        } else {
            self.account.supervisor.retry(cx);
        }
    }

    /// A private connection closed. One that opened and closed before any
    /// login answer, twice in a row, means the route drops sockets silently:
    /// the next connect asks the host for the next route.
    pub(crate) fn private_closed(&mut self, cx: &mut Cx<'_, Pending>, opened: bool) {
        let answered = std::mem::replace(&mut self.account.login_answered, false);
        if !opened || answered || self.account.authenticated || self.account.route_exhausted {
            self.account.unanswered_closes = 0;
            return;
        }
        self.account.unanswered_closes = self.account.unanswered_closes.saturating_add(1);
        if self.account.unanswered_closes < 2 {
            return;
        }
        self.account.unanswered_closes = 0;
        self.account.reject_route = true;
        cx.notice(
            Level::Warn,
            code::STREAM_UNAVAILABLE,
            "Bitget private WS route keeps closing before login; trying the next route",
        );
    }

    /// The host refused every route: report once and stop the private socket.
    /// Startup is REST-gated, so it still settles; REST polling stands in.
    pub(crate) fn private_route_exhausted(&mut self, cx: &mut Cx<'_, Pending>) {
        if std::mem::replace(&mut self.account.route_exhausted, true) {
            return;
        }
        self.account.supervisor.cancel(cx);
        self.account.authenticated = false;
        self.account.reject_route = false;
        self.account.degraded = true;
        self.sockets[3].open = false;
        self.sockets[3].socket = None;
        let detail = self
            .account
            .route_refused
            .take()
            .map_or_else(String::new, |(code, msg)| {
                [code, msg]
                    .into_iter()
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ")
            });
        cx.notice(
            Level::Error,
            code::AUTH_REJECTED,
            format!(
                "Bitget private WS login rejected: {detail} — the API key's IP whitelist allows none of Tealstreet's WebSocket egress IPs"
            ),
        );
    }

    /// `Venue::refresh_settings`: re-read the account's mode, margin and
    /// leverage as startup and pre-write metadata do; never a write.
    pub(crate) fn refresh_settings(&mut self, cx: &mut Cx<'_, Pending>, symbol: Option<&str>) {
        if !self.account.detected {
            return;
        }
        if self.account.uta {
            self.account_request(
                cx,
                paths::SETTINGS.to_owned(),
                None,
                Request::Settings(None),
            );
            return;
        }
        // Classic keeps mode, margin and leverage per symbol: the given one, or
        // every symbol with a position or order.
        let mut symbols: Vec<String> = match symbol {
            Some(symbol) => vec![symbol.to_owned()],
            None => cx
                .store
                .positions()
                .iter()
                .map(|p| p.symbol.clone())
                .chain(cx.store.orders().iter().map(|o| o.symbol.clone()))
                .collect(),
        };
        symbols.sort_unstable();
        symbols.dedup();
        for symbol in symbols {
            let Ok(coin) = self.margin_coin(cx, &symbol) else {
                continue;
            };
            let path = format!(
                "{}?symbol={symbol}&productType={}&marginCoin={coin}",
                paths::ACCOUNT,
                self.product()
            );
            self.account_request(cx, path, None, Request::Settings(Some(symbol)));
        }
    }

    fn settings_response(&mut self, cx: &mut Cx<'_, Pending>, symbol: Option<&str>, body: &str) {
        match symbol {
            None => match serde_json::from_str::<Envelope<UtaSettings>>(body)
                .map_err(|e| e.to_string())
                .and_then(Envelope::checked)
            {
                Ok(settings) => self.apply_settings(cx, &settings),
                Err(error) => self.account_failed(cx, Request::Settings(None), &error),
            },
            Some(symbol) => match serde_json::from_str::<Envelope<crate::wire::SymbolConfig>>(body)
                .map_err(|e| e.to_string())
                .and_then(Envelope::checked)
            {
                Ok(row) if !row.pos_mode.is_empty() && !row.margin_mode.is_empty() => {
                    self.apply_symbol_config(cx, symbol, &row)
                }
                _ => self.account_failed(
                    cx,
                    Request::Settings(Some(symbol.to_owned())),
                    "Bitget position mode or margin type could not be resolved",
                ),
            },
        }
    }

    pub(crate) fn promote_uta(&mut self, cx: &mut Cx<'_, Pending>) -> bool {
        // Frame-fed: the data source detects the account type; a 40085 is an error here.
        if cx.frame_fed() || self.account.uta || self.options.testnet || self.inverse() {
            return false;
        }
        self.account.uta = true;
        self.account.generation += 1;
        self.account.detected = false;
        self.account.positions_in_flight = false;
        self.account.balance_in_flight = false;
        self.account.orders_in_flight = false;
        self.account_request(cx, paths::SETTINGS.to_owned(), None, Request::Probe);
        true
    }

    pub(crate) fn apply_settings(&self, cx: &mut Cx<'_, Pending>, settings: &UtaSettings<'_>) {
        cx.store.set_hedged(is_hedged(&settings.hold_mode));
        for row in &settings.symbol_config_list {
            if !row.category.is_empty() && !row.category.eq_ignore_ascii_case(self.product()) {
                continue;
            }
            let margin = margin(&row.margin_mode);
            if !row.pos_mode.is_empty() {
                cx.store.set_hedged(is_hedged(&row.pos_mode));
            }
            let per_side = cx.store.hedged() && margin == MarginType::Isolated;
            let long = first(&[row.long_leverage, row.isolated_long_lever]);
            let short = first(&[row.short_leverage, row.isolated_short_lever]);
            let common = first(&[row.leverage, row.crossed_margin_leverage]);
            let leverage = if per_side {
                long.or(common).or(short)
            } else {
                common.or(long).or(short)
            }
            .unwrap_or(1.0);
            cx.store.update_setting(&row.symbol, |s| {
                s.leverage = leverage;
                s.margin_type = Some(margin);
                if per_side {
                    // A scalar fallback says nothing about the opposite hedge
                    // leg. Preserve a known leg if this row omits its value.
                    s.buy_leverage = long.or(s.buy_leverage).or(common);
                    s.sell_leverage = short.or(s.sell_leverage).or(common);
                    s.leverage = s.buy_leverage.unwrap_or(leverage);
                } else {
                    s.buy_leverage = Some(leverage);
                    s.sell_leverage = Some(leverage);
                }
            });
        }
    }

    fn position_setting(&self, cx: &mut Cx<'_, Pending>, p: &Position) {
        cx.store
            .set_hedged(p.position_mode == Some(PositionMode::Hedged));
        cx.store.update_setting(&p.symbol, |s| {
            s.margin_type = p.margin_type;
            if p.position_mode == Some(PositionMode::Hedged)
                && p.margin_type == Some(MarginType::Isolated)
            {
                match p.side {
                    PositionSide::Long => {
                        s.leverage = p.leverage;
                        s.buy_leverage = Some(p.leverage);
                    }
                    PositionSide::Short => {
                        s.sell_leverage = Some(p.leverage);
                        // A short row must not rewrite a known long scalar.
                        if let Some(long) = s.buy_leverage {
                            s.leverage = long;
                        } else if !s.leverage.is_finite() {
                            s.leverage = p.leverage;
                        }
                    }
                }
            } else {
                s.leverage = p.leverage;
                s.buy_leverage = Some(p.leverage);
                s.sell_leverage = Some(p.leverage);
            }
        });
    }

    fn coherent_position_modes(
        &self,
        cx: &mut Cx<'_, Pending>,
        positions: &mut Vec<Position>,
        skipped: &mut RowAdmission<PositionKey>,
        snapshot: bool,
    ) -> HashSet<String> {
        let mut conflicts = HashSet::new();
        let Some(first) = positions.first().map(|p| p.position_mode) else {
            return conflicts;
        };
        let mixed = positions.iter().any(|p| p.position_mode != first);
        if snapshot && !mixed && skipped.rejected() == 0 {
            return conflicts;
        }
        let held_conflict = cx.store.positions().iter().any(|old| {
            old.position_mode != first
                && !positions.iter().any(|p| p.id == old.id)
                && (!snapshot
                    || [Some(old.side), None]
                        .into_iter()
                        .any(|side| !skipped.allows_absence(&(old.symbol.clone(), side))))
        });
        if mixed || held_conflict {
            conflicts.extend(
                positions
                    .iter()
                    .filter(|p| {
                        (p.position_mode == Some(PositionMode::Hedged)) != cx.store.hedged()
                    })
                    .map(|p| p.symbol.clone()),
            );
            for symbol in &conflicts {
                skipped.reject(Some((symbol.clone(), None)));
            }
            positions.retain(|p| !conflicts.contains(&p.symbol));
            if !conflicts.is_empty() {
                row_skipped(
                    cx,
                    Level::Warn,
                    "skipped Bitget position stream record: conflicting account position modes; cached exposure retained",
                );
            }
        }
        conflicts
    }

    fn merge_positions(
        &self,
        cx: &Cx<'_, Pending>,
        rows: Vec<Position>,
        started: f64,
    ) -> Vec<Position> {
        let full_fresh = self.account.full_positions_at.is_some_and(|at| {
            at >= started || (!self.account.full_positions_fence_only && started - at <= 5_000.0)
        });
        let mut rows: Vec<_> = rows
            .into_iter()
            .filter_map(|mut p| {
                if self.account.config_at.is_some_and(|at| started <= at) {
                    if let Some(settings) = cx.store.setting(&p.symbol) {
                        if cx.store.hedged() && settings.margin_type == Some(MarginType::Isolated) {
                            let leverage = match p.side {
                                PositionSide::Long => settings.buy_leverage,
                                PositionSide::Short => settings.sell_leverage,
                            };
                            if let Some(leverage) = leverage.or_else(|| {
                                cx.store
                                    .positions()
                                    .iter()
                                    .find(|current| {
                                        current.symbol == p.symbol && current.side == p.side
                                    })
                                    .map(|current| current.leverage)
                            }) {
                                p.leverage = leverage;
                            }
                        } else {
                            p.leverage = settings.leverage;
                        }
                        p.margin_type = settings.margin_type;
                    }
                    p.position_mode = Some(if cx.store.hedged() {
                        PositionMode::Hedged
                    } else {
                        PositionMode::Oneway
                    });
                }
                if cx.store.positions().iter().any(|old| {
                    old.symbol == p.symbol
                        && old.position_mode != p.position_mode
                        && self
                            .account
                            .ws_positions
                            .get(&old.id)
                            .is_some_and(|mark| mark.rejected_at.is_some_and(|at| at >= started))
                }) {
                    return None;
                }
                if full_fresh && !self.account.full_position_ids.contains(&p.id) {
                    return None;
                }
                if let Some(mark) = self.account.ws_positions.get(&p.id) {
                    let rejection = mark.rejected_at.is_some_and(|at| at >= started);
                    let financial = mark.at.is_some_and(|at| {
                        started - at <= 5_000.0
                            && (at >= started
                                || p.last_update.is_none()
                                || mark
                                    .update
                                    .is_some_and(|t| p.last_update.is_some_and(|r| t > r)))
                    });
                    if rejection || financial {
                        return cx.store.position(&p.id).cloned();
                    }
                }
                Some(p)
            })
            .collect();
        for current in cx.store.positions() {
            if !rows.iter().any(|p| p.id == current.id)
                && self.account.ws_positions.get(&current.id).is_some_and(|m| {
                    m.rejected_at.is_some_and(|at| at >= started)
                        || m.at.is_some_and(|at| started - at <= 5_000.0)
                })
            {
                rows.push(current.clone());
            }
        }
        rows
    }

    fn order_metadata_current(
        &self,
        cx: &Cx<'_, Pending>,
        id: &str,
        started: f64,
        stream_revision: u64,
    ) -> bool {
        let newer = self.order_revision(id) > stream_revision
            || self
                .account
                .opened
                .get(id)
                .is_some_and(|until| *until >= started + 5_000.0);
        let terminal = self.account.terminal.contains_key(id)
            || cx.store.order(id).is_some_and(|o| {
                matches!(
                    o.status,
                    OrderStatus::Canceled | OrderStatus::Closed | OrderStatus::Canceling
                )
            });
        !newer && !terminal
    }

    fn commit_orders(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut rows: Vec<Order>,
        skipped: &RowAdmission<String>,
        virtual_only: bool,
        started: f64,
        stream_revision: u64,
    ) {
        self.account.terminal.retain(|_, until| *until > cx.now);
        self.account.opened.retain(|_, until| *until > cx.now);
        let mut seen = HashSet::new();
        rows.retain(|row| seen.insert(row.id.clone()));
        for row in &mut rows {
            if (self.order_revision(&row.id) > stream_revision
                || self
                    .account
                    .opened
                    .get(&row.id)
                    .is_some_and(|until| *until >= started + 5_000.0))
                && let Some(current) = cx.store.order(&row.id)
            {
                *row = current.clone();
            }
        }
        let live = |id: &String| {
            !self.account.terminal.contains_key(id)
                && !cx.store.order(id).is_some_and(|current| {
                    matches!(
                        current.status,
                        OrderStatus::Canceling | OrderStatus::Closed | OrderStatus::Canceled
                    )
                })
        };
        rows.retain(|row| live(&row.id));
        if virtual_only {
            rows.retain(|o| o.parent_id.is_some());
            rows.extend(
                cx.store
                    .orders()
                    .iter()
                    .filter(|o| {
                        o.parent_id.is_none()
                            || self
                                .account
                                .plans
                                .get(&o.id)
                                .is_some_and(|plan| plan == "uta_tpsl")
                    })
                    .cloned(),
            );
        } else {
            for current in cx.store.orders() {
                if current.status == OrderStatus::Open
                    && !rows.iter().any(|r| r.id == current.id)
                    && ((is_algo(current.kind)
                        && !(self.account.uta && current.kind == OrderType::TrailingStopLoss))
                        || self.account.opened.contains_key(&current.id)
                        || self.order_revision(&current.id) > stream_revision)
                {
                    rows.push(current.clone());
                }
            }
        }
        // Preset TP/SL children carry their rejected parent's identity.
        for current in cx.store.orders() {
            let protected = [Some(&current.id), current.parent_id.as_ref()]
                .into_iter()
                .flatten()
                .any(|id| !skipped.allows_absence(id));
            if protected && live(&current.id) && !rows.iter().any(|r| r.id == current.id) {
                rows.push(current.clone());
            }
        }
        cx.store.set_orders(rows);
    }

    fn inverse_asset(&self, cx: &Cx<'_, Pending>, raw: &AccountRow<'_>) -> Option<InverseAsset> {
        let coin = first_text(&[&raw.margin_coin, &raw.coin]);
        if coin.is_empty() {
            return None;
        }
        let amounts = classic_amounts(raw)?;
        let usd = raw.usdt_equity.0.filter(|v| *v > 0.0).or_else(|| {
            cx.store
                .tickers()
                .iter()
                .find(|t| t.symbol == format!("{coin}USD") || t.symbol == format!("{coin}USDT"))
                .map(|t| amounts.equity * t.mark)
                .filter(|v| *v > 0.0)
        });
        let asset = match usd {
            Some(usd) if amounts.equity != 0.0 => {
                let rate = usd / amounts.equity;
                InverseAsset {
                    coin: CoinBalance {
                        base: coin.into(),
                        amount: amounts.equity,
                        notional: usd,
                    },
                    free: amounts.free * rate,
                    used: (amounts.equity - amounts.free) * rate,
                    total: (amounts.equity - amounts.upnl) * rate,
                    upnl: amounts.upnl * rate,
                    priced: true,
                }
            }
            _ if amounts.equity == 0.0 => InverseAsset {
                coin: CoinBalance {
                    base: coin.into(),
                    amount: 0.0,
                    notional: usd.unwrap_or(0.0),
                },
                free: amounts.free,
                used: -amounts.free,
                total: -amounts.upnl,
                upnl: amounts.upnl,
                priced: true,
            },
            // Unpriced coin: keep its last real USD valuation, never value coins as dollars.
            _ => match self
                .account
                .inverse_assets
                .iter()
                .find(|prior| prior.priced && prior.coin.base == coin)
            {
                Some(prior) => InverseAsset {
                    coin: CoinBalance {
                        amount: amounts.equity,
                        ..prior.coin.clone()
                    },
                    ..prior.clone()
                },
                None => InverseAsset {
                    coin: CoinBalance {
                        base: coin.into(),
                        amount: amounts.equity,
                        notional: 0.0,
                    },
                    free: 0.0,
                    used: 0.0,
                    total: 0.0,
                    upnl: 0.0,
                    priced: false,
                },
            },
        };
        [
            asset.coin.amount,
            asset.coin.notional,
            asset.free,
            asset.used,
            asset.total,
            asset.upnl,
        ]
        .iter()
        .all(|n| n.is_finite())
        .then_some(asset)
    }

    fn apply_inverse_balance(&mut self, cx: &mut Cx<'_, Pending>) {
        cx.store.set_coin_balances(
            self.account
                .inverse_assets
                .iter()
                .map(|a| a.coin.clone())
                .collect(),
        );
        if self.account.inverse_assets.iter().any(|a| !a.priced) {
            if !std::mem::replace(&mut self.account.inverse_unpriced_warned, true) {
                cx.notice(Level::Warn, code::VALUATION_UNAVAILABLE,
                    "Bitget inverse collateral USD price unavailable; preserving previous USD balance",
                );
            }
            return;
        }
        self.account.inverse_unpriced_warned = false;
        let mut balance = Balance::default();
        for a in &self.account.inverse_assets {
            balance.total += a.total;
            balance.upnl += a.upnl;
            balance.used += a.used;
            balance.free += a.free;
        }
        cx.store.set_balance(balance);
    }

    fn apply_balance(&self, cx: &mut Cx<'_, Pending>, amounts: Amounts) {
        let open = cx.store.positions().iter().any(|p| p.contracts > 0.0);
        let upnl = if open {
            cx.store
                .positions()
                .iter()
                .map(|p| {
                    let direction = if p.side == PositionSide::Long {
                        1.0
                    } else {
                        -1.0
                    };
                    cx.store
                        .ticker(&p.symbol)
                        .filter(|t| t.last > 0.0)
                        .map_or(p.unrealized_pnl, |t| {
                            direction * (t.last - p.entry_price) * p.contracts
                        })
                })
                .sum()
        } else {
            amounts.upnl
        };
        cx.store.set_balance(Balance {
            used: clean(amounts.equity - amounts.free),
            free: amounts.free,
            total: clean(amounts.equity - upnl),
            upnl: clean(upnl),
        });
    }

    pub(crate) fn private_message(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        let Ok(header) = serde_json::from_str::<Message<'_, IgnoredAny>>(text) else {
            return;
        };
        // Bitget answers a refused login with `event: "error"` and a 300xx code.
        let refused = header.event == "error" && !self.account.authenticated;
        if refused || header.event == "login" {
            self.account.login_answered = true;
            let code = header.code.and_then(|n| n.0);
            if !refused && code == Some(0.0) {
                if self.account.authenticated {
                    return;
                }
                self.account.authenticated = true;
                self.account.last_account_read_at = None;
                self.account.unanswered_closes = 0;
                self.account.route_refusals = 0;
                self.account.route_refused = None;
                self.account.supervisor.authenticated(cx);
                self.private_subscribe(cx);
            } else {
                if !self.account.authenticated
                    && let Some(code) = code.filter(|code| [40018.0, 40024.0].contains(code))
                {
                    return self.refuse_route(cx, code, &header.msg);
                }
                if !self.account.authenticated
                    && !self.startup_ready(cx)
                    && let Some(code) = socket_key_rejection(text)
                {
                    let error =
                        rejection("API key login", code, &header.msg, ErrorKind::AuthInvalid);
                    return self.reject_startup(cx, error, true);
                }
                cx.notice(
                    Level::Error,
                    code::AUTH_REJECTED,
                    format!("Bitget private WS login failed: {}", header.msg),
                );
                if let Some(socket) = self.sockets[3].socket {
                    cx.close(socket);
                    self.disconnected(cx, 3);
                }
            }
            return;
        }
        if !header.event.is_empty() || !self.account.authenticated {
            return;
        }
        let channel = if !header.arg.topic.is_empty() {
            header.arg.topic.as_ref()
        } else {
            header.arg.channel.as_ref()
        };
        match channel {
            "positions" | "position" => {
                let Ok(message) =
                    serde_json::from_str::<Message<'_, &serde_json::value::RawValue>>(text)
                else {
                    return;
                };
                let Some(rows) = message.data else {
                    return;
                };
                let empty = rows.is_empty();
                let mut positions = Vec::with_capacity(rows.len());
                let mut skipped = RowAdmission::default();
                for item in rows {
                    let mapped = match serde_json::from_str::<PositionRow>(item.get()) {
                        Ok(row) => {
                            if self.account.uta
                                && !row.margin_coin.is_empty()
                                && row.margin_coin != self.coin()
                            {
                                continue;
                            }
                            self.map_position(cx, &row)
                        }
                        Err(_) => {
                            if self.foreign_stream_row(item.get(), true) {
                                continue;
                            }
                            Err("malformed record".to_owned())
                        }
                    };
                    match mapped {
                        Ok(position) => positions.push(position),
                        Err(error) => {
                            skipped.skip(
                                cx,
                                self.position_key(item.get()),
                                Level::Warn,
                                format!("skipped Bitget position stream record: {error}"),
                            );
                        }
                    }
                }
                if !empty && positions.is_empty() && skipped.rejected() == 0 {
                    return;
                }
                let mut rejected = skipped.clone();
                // Preserve the prior all-unmappable scope hold. With valid peers,
                // rejected identity coverage alone determines safe omission.
                if positions.is_empty() && skipped.rejected() != 0 {
                    skipped.reject(None);
                }
                let conflicts = self.coherent_position_modes(
                    cx,
                    &mut positions,
                    &mut skipped,
                    message.action == "snapshot",
                );
                for symbol in conflicts {
                    rejected.reject(Some((symbol, None)));
                }
                let useful = !positions.is_empty() || empty;
                for p in &positions {
                    self.position_setting(cx, p);
                }
                let held: Vec<_> = cx
                    .store
                    .positions()
                    .iter()
                    .filter(|current| {
                        !positions.iter().any(|p| p.id == current.id)
                            && [Some(current.side), None].into_iter().any(|side| {
                                !skipped.allows_absence(&(current.symbol.clone(), side))
                            })
                    })
                    .cloned()
                    .collect();
                // Rejections are observation barriers, never a new full financial
                // snapshot. A fresh read can heal them; unrelated peers remain eligible.
                for p in &held {
                    if [Some(p.side), None]
                        .into_iter()
                        .any(|side| !rejected.allows_absence(&(p.symbol.clone(), side)))
                    {
                        self.account
                            .ws_positions
                            .entry(p.id.clone())
                            .and_modify(|mark| mark.rejected_at = Some(cx.now))
                            .or_insert(WsPosition {
                                at: None,
                                update: None,
                                rejected_at: Some(cx.now),
                            });
                    }
                }
                if message.action == "snapshot" {
                    // Validly omitted identities still fence an older read's resurrection.
                    for old in cx.store.positions() {
                        if !positions.iter().any(|p| p.id == old.id)
                            && !held.iter().any(|p| p.id == old.id)
                        {
                            self.account.ws_positions.insert(
                                old.id.clone(),
                                WsPosition {
                                    at: Some(cx.now),
                                    update: None,
                                    rejected_at: None,
                                },
                            );
                        }
                    }
                    if skipped.rejected() == 0 {
                        self.account.full_positions_at = Some(cx.now);
                        self.account.full_positions_fence_only = false;
                        self.account.full_position_ids = positions
                            .iter()
                            .filter(|p| p.contracts > 0.0)
                            .map(|p| p.id.clone())
                            .collect();
                    } else {
                        self.account.full_positions_fence_only = true;
                    }
                }
                for p in &positions {
                    self.account.ws_positions.insert(
                        p.id.clone(),
                        WsPosition {
                            at: Some(cx.now),
                            update: p.last_update,
                            rejected_at: None,
                        },
                    );
                }
                positions.extend(held);
                if message.action == "snapshot" && useful {
                    cx.store.set_positions(
                        positions
                            .into_iter()
                            .filter(|p| p.contracts > 0.0)
                            .collect(),
                    );
                } else if useful {
                    for p in positions {
                        cx.store.upsert_position(p);
                    }
                }
                if useful {
                    self.account.last_payload = cx.now;
                    self.account.degraded = false;
                }
                if self.account.positions_timer.is_none() {
                    self.account.positions_timer = Some(cx.set_timer(500.0));
                }
                if self.account.balance_timer.is_none() {
                    self.account.balance_timer = Some(cx.set_timer(300.0));
                }
            }
            "orders" | "order" | "orders-algo" | "ordersAlgo" | "strategy-order" => {
                let Ok(message) =
                    serde_json::from_str::<Message<'_, &serde_json::value::RawValue>>(text)
                else {
                    return;
                };
                let Some(rows) = message.data else {
                    return;
                };
                let mut useful = rows.is_empty();
                for item in rows {
                    let Ok(raw) = serde_json::from_str::<OrderRow>(item.get()) else {
                        if self.foreign_stream_row(item.get(), false) {
                            continue;
                        }
                        self.reject_stream_order(cx, item.get());
                        continue;
                    };
                    if self.account.uta && !raw.category.eq_ignore_ascii_case(self.product()) {
                        continue;
                    }
                    let Ok(orders) = self.map_order(cx, &raw) else {
                        self.reject_stream_order(cx, item.get());
                        continue;
                    };
                    let Some(order) = orders.first() else {
                        continue;
                    };
                    useful = true;
                    let status = raw_status(&raw);
                    let explicit_fill = matches!(
                        status,
                        "partial_fill"
                            | "partially_filled"
                            | "partially_fill"
                            | "filled"
                            | "full_fill"
                    );
                    if explicit_fill && !self.account.uta && !is_algo(order.kind) {
                        self.emit_execution(cx, &raw, order);
                    }
                    for order in orders {
                        self.account.ws_order_clock = self.account.ws_order_clock.saturating_add(1);
                        self.account
                            .ws_order_revisions
                            .insert(order.id.clone(), self.account.ws_order_clock);
                        if let Some(state) = trailing_state(&raw) {
                            self.account.trailing.insert(order.id.clone(), state);
                        }
                        if self.account.uta && channel == "strategy-order" {
                            self.account
                                .plans
                                .insert(order.id.clone(), "uta_tpsl".to_owned());
                        }
                        if !raw.plan_type.is_empty() {
                            self.account.plans.insert(
                                order.id.clone(),
                                plan_type(&raw.plan_type)
                                    .unwrap_or(&raw.plan_type)
                                    .to_owned(),
                            );
                        }
                        match order.status {
                            OrderStatus::Closed | OrderStatus::Canceled => {
                                self.account
                                    .terminal
                                    .insert(order.id.clone(), cx.now + 10_000.0);
                                self.account.opened.remove(&order.id);
                                cx.store.set_order_status(&order.id, order.status);
                            }
                            _ if !self
                                .account
                                .terminal
                                .get(&order.id)
                                .is_some_and(|until| *until > cx.now) =>
                            {
                                self.account
                                    .opened
                                    .insert(order.id.clone(), cx.now + 5_000.0);
                                cx.store.upsert_order(order);
                            }
                            _ => {}
                        }
                    }
                    if explicit_fill {
                        if self.account.balance_timer.is_none() {
                            self.account.balance_timer = Some(cx.set_timer(300.0));
                        }
                        if self.account.positions_timer.is_none() {
                            self.account.positions_timer = Some(cx.set_timer(500.0));
                        }
                    }
                }
                if useful {
                    self.account.last_payload = cx.now;
                    self.account.degraded = false;
                }
                if self.account.orders_timer.is_none() {
                    self.account.orders_timer = Some(cx.set_timer(500.0));
                }
            }
            "fill" if self.account.uta => {
                if let Ok(message) = serde_json::from_str::<Message<'_, FillRow>>(text)
                    && let Some(rows) = message.data
                {
                    for row in rows {
                        if !row.category.eq_ignore_ascii_case(self.product())
                            || cx.store.market(&row.symbol).is_none()
                        {
                            continue;
                        }
                        let price = row.exec_price.get();
                        let amount = row.exec_qty.get();
                        let execution = row.exec_id.text();
                        let order = row.order_id.text();
                        if execution.is_empty() || order.is_empty() || price <= 0.0 || amount <= 0.0
                        {
                            continue;
                        }
                        let Some(side) = order_side(&row.side) else {
                            continue;
                        };
                        if self.remember_execution(format!("{}:{execution}", row.symbol)) {
                            cx.fill(Fill {
                                id: None,
                                order_id: order.into_owned(),
                                symbol: row.symbol.into_owned(),
                                side,
                                price,
                                amount,
                            });
                        }
                    }
                }
                self.account.last_payload = cx.now;
                self.account.degraded = false;
                if self.account.balance_timer.is_none() {
                    self.account.balance_timer = Some(cx.set_timer(300.0));
                }
                if self.account.positions_timer.is_none() {
                    self.account.positions_timer = Some(cx.set_timer(500.0));
                }
            }
            "account" => {
                if self.account.uta {
                    if let Ok(message) = serde_json::from_str::<Message<'_, UtaBalance>>(text)
                        && let Some(rows) = message.data
                    {
                        for row in rows {
                            if let Some(amounts) = uta_amounts(&row, self.coin()) {
                                self.apply_balance(cx, amounts);
                                self.account.balance_version += 1;
                            }
                        }
                    }
                } else if let Ok(message) = serde_json::from_str::<Message<'_, AccountRow>>(text)
                    && let Some(rows) = message.data
                {
                    if self.inverse() {
                        let assets: Option<Vec<_>> =
                            rows.iter().map(|row| self.inverse_asset(cx, row)).collect();
                        if let Some(assets) = assets.filter(|assets| {
                            let mut seen = HashSet::new();
                            assets.iter().all(|a| seen.insert(a.coin.base.as_str()))
                        }) {
                            if message.action == "snapshot" {
                                self.account.inverse_assets = assets;
                            } else {
                                for asset in assets {
                                    self.account
                                        .inverse_assets
                                        .retain(|a| a.coin.base != asset.coin.base);
                                    self.account.inverse_assets.push(asset);
                                }
                            }
                            self.apply_inverse_balance(cx);
                            self.account.balance_version += 1;
                        }
                    } else {
                        for row in rows {
                            if (row.margin_coin == self.coin() || row.coin == self.coin())
                                && let Some(amounts) = classic_amounts(&row)
                            {
                                self.apply_balance(cx, amounts);
                                self.account.balance_version += 1;
                            }
                        }
                    }
                }
                self.account.last_payload = cx.now;
                self.account.degraded = false;
            }
            _ => {}
        }
    }

    fn emit_execution(&mut self, cx: &mut Cx<'_, Pending>, raw: &OrderRow<'_>, order: &Order) {
        let previous = if let Some((cursor, _)) = self.account.filled.get(&order.id) {
            *cursor
        } else {
            let historical = cx.store.order(&order.id).map_or(0.0, |order| order.filled);
            let Ok(seed) = Cursor::seed(historical, None) else {
                return;
            };
            // Pin the pre-observation bootstrap even if this delivery is unpriced.
            // The display order below may advance; it is not delivery evidence.
            self.account.filled.insert(order.id.clone(), (seed, cx.now));
            seed
        };
        let cumulative = first(&[
            raw.acc_base_volume,
            raw.cum_exec_qty,
            raw.filled_qty,
            raw.acc_fill_sz,
        ]);
        let price = first(&[
            raw.fill_price,
            raw.price_avg,
            raw.avg_price,
            raw.price,
            raw.px,
        ])
        .unwrap_or(order.price);
        let report = if let Some(amount) = raw.base_volume.0 {
            Report::Last {
                amount,
                cumulative,
                price,
            }
        } else {
            Report::Cumulative {
                quantity: cumulative.unwrap_or(0.0),
                price: CumulativePrice::Direct(price),
            }
        };
        let Ok(Some(prepared)) = previous.prepare(report, Arithmetic::Binary) else {
            return;
        };
        let execution = first_text(&[&raw.trade_id.text(), &raw.exec_id.text()]).to_owned();
        let key = if execution.is_empty() {
            format!(
                "{}:{}:{}:{}",
                order.id,
                cumulative.unwrap_or(order.filled),
                raw.fill_time.or(raw.u_time.get()),
                prepared.amount()
            )
        } else {
            format!("{}:{execution}", order.symbol)
        };
        if !self.remember_execution(key) {
            return;
        }
        cx.fill(Fill {
            id: None,
            order_id: order.id.clone(),
            symbol: order.symbol.clone(),
            side: order.side,
            price: prepared.price(),
            amount: prepared.amount(),
        });
        self.account
            .filled
            .insert(order.id.clone(), (prepared.next(), cx.now));
    }

    fn remember_execution(&mut self, key: String) -> bool {
        if !self.account.executed.insert(key.clone()) {
            return false;
        }
        self.account.executed_order.push_back(key);
        while self.account.executed_order.len() > 4096 {
            if let Some(old) = self.account.executed_order.pop_front() {
                self.account.executed.remove(&old);
            }
        }
        true
    }

    pub(crate) fn order_revision_clock(&self) -> u64 {
        self.account.ws_order_clock
    }

    pub(crate) fn order_revision(&self, id: &str) -> u64 {
        self.account
            .ws_order_revisions
            .get(id)
            .copied()
            .unwrap_or(0)
    }

    pub(crate) fn mark_opened(&mut self, now: f64, id: &str) {
        self.account.ws_order_clock = self.account.ws_order_clock.saturating_add(1);
        self.account
            .ws_order_revisions
            .insert(id.to_owned(), self.account.ws_order_clock);
        self.account.opened.insert(id.to_owned(), now + 5_000.0);
    }

    pub(crate) fn order_terminal(&self, id: &str) -> bool {
        self.account.terminal.contains_key(id)
    }

    pub(crate) fn positions_in_flight(&self) -> bool {
        self.account.positions_in_flight
    }

    pub(crate) fn mark_canceled(&mut self, cx: &mut Cx<'_, Pending>, id: &str) {
        self.account
            .terminal
            .insert(id.to_owned(), cx.now + 10_000.0);
        self.account.opened.remove(id);
        cx.store.update_order(id, |order| {
            if !matches!(order.status, OrderStatus::Closed | OrderStatus::Canceled) {
                order.status = OrderStatus::Canceled;
            }
        });
    }

    /// A complete order read for an Unknown write. One already in flight may
    /// predate the write, so another follows it; an engine re-ask joins either.
    pub(crate) fn reconcile_orders(&mut self, cx: &mut Cx<'_, Pending>) {
        if !cx.reconcile_reask() {
            self.account.orders_dirty = true;
        }
        self.refresh_orders(cx, false);
    }

    pub(crate) fn note_write(&mut self, cx: &mut Cx<'_, Pending>) {
        // Frame-fed: the data source reconciles from its own private stream.
        if cx.frame_fed() {
            return;
        }
        if self.account.uta {
            self.account.orders_dirty = true;
            self.refresh_orders(cx, false);
        }
        if self.account.write_check.is_none() {
            self.account.write_check = Some((cx.set_timer(5_000.0), cx.now));
        }
    }

    pub(crate) fn account_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.account.probe.on_timer(timer).is_some() {
            self.account_request(cx, paths::SETTINGS.to_owned(), None, Request::Probe);
        }
        if self.account.write_check.is_some_and(|(t, _)| t == timer) {
            let (_, since) = self.account.write_check.take().expect("write check");
            if self.account.last_payload <= since {
                self.account.degraded = true;
                if !self.account.uta {
                    self.account.needs_recovery = true;
                }
                cx.notice(Level::Warn, code::STREAM_UNAVAILABLE, "Bitget private stream has no authenticated payload after a write; reconciling and reconnecting");
                self.refresh_positions(cx);
                self.refresh_balance(cx);
                self.refresh_orders(cx, false);
                self.connect(cx, 3);
            }
        }
        if self.account.poll == Some(timer) {
            self.account.poll = None;
            // Keep stream updates and closures until every collateral read completes.
            if !self.account.positions_in_flight {
                self.account.ws_positions.retain(|_, m| {
                    m.at.is_some_and(|at| cx.now - at < 10_000.0)
                        || m.rejected_at.is_some_and(|at| cx.now - at < 10_000.0)
                });
            }
            self.account.tick += 1;
            if self.stream_live() {
                // Positions, balance and orders stream on the private topics; REST is the
                // 60 s safety reconcile, plus positions while one is open (the margin
                // uPnL is the venue's position PnL, and topics may push only on changes).
                let reconcile = self
                    .account
                    .last_account_read_at
                    .is_none_or(|at| cx.now - at >= SAFETY_RECONCILE_MS);
                if reconcile || cx.store.positions().iter().any(|p| p.contracts > 0.0) {
                    self.refresh_positions(cx);
                }
                if reconcile {
                    self.account.last_account_read_at = Some(cx.now);
                    self.refresh_balance(cx);
                    self.refresh_orders(cx, !self.account.uta);
                }
            } else {
                if self.account.degraded {
                    self.refresh_balance(cx);
                    self.refresh_orders(cx, false);
                }
                self.refresh_positions(cx);
                if self.account.tick.is_multiple_of(2) {
                    self.refresh_balance(cx);
                }
                if self.account.tick.is_multiple_of(4) {
                    self.refresh_orders(cx, !self.account.uta);
                }
            }
            self.account.poll = Some(cx.set_poll_timer(2_000.0));
            self.account
                .filled
                .retain(|id, (_, at)| cx.now - *at < 600_000.0 || cx.store.order(id).is_some());
            self.account
                .trailing
                .retain(|id, _| cx.store.order(id).is_some());
            self.account.ws_order_revisions.retain(|id, _| {
                cx.store.order(id).is_some() || self.account.terminal.contains_key(id)
            });
            self.account
                .plans
                .retain(|id, _| cx.store.order(id).is_some());
            self.account.terminal.retain(|_, until| *until > cx.now);
            self.account.opened.retain(|_, until| *until > cx.now);
        }
        if self.account.positions_timer == Some(timer) {
            self.account.positions_timer = None;
            if self.account.positions_in_flight || cx.now < self.account.rest_paused_until {
                let delay = (self.account.rest_paused_until - cx.now).max(100.0);
                self.account.positions_timer = Some(cx.set_timer(delay));
            } else {
                self.refresh_positions(cx);
            }
        }
        if self.account.balance_timer == Some(timer) {
            self.account.balance_timer = None;
            if self.account.balance_in_flight || cx.now < self.account.rest_paused_until {
                let delay = (self.account.rest_paused_until - cx.now).max(100.0);
                self.account.balance_timer = Some(cx.set_timer(delay));
            } else {
                self.refresh_balance(cx);
            }
        }
        if self.account.orders_timer == Some(timer) {
            self.account.orders_timer = None;
            if self.account.orders_in_flight || cx.now < self.account.rest_paused_until {
                let delay = (self.account.rest_paused_until - cx.now).max(100.0);
                self.account.orders_timer = Some(cx.set_timer(delay));
            } else {
                self.refresh_orders(cx, false);
            }
        }
    }

    fn foreign_stream_row(&self, item: &str, position: bool) -> bool {
        if !self.account.uta {
            return false;
        }
        #[derive(Deserialize)]
        struct Product<'a> {
            #[serde(default, borrow)]
            category: std::borrow::Cow<'a, str>,
            #[serde(default, borrow, rename = "marginCoin")]
            margin_coin: std::borrow::Cow<'a, str>,
        }
        let Ok(row) = serde_json::from_str::<Product>(item) else {
            return false;
        };
        if position {
            !row.margin_coin.is_empty() && row.margin_coin != self.coin()
        } else {
            !row.category.is_empty() && !row.category.eq_ignore_ascii_case(self.product())
        }
    }

    fn reject_stream_order(&mut self, cx: &mut Cx<'_, Pending>, item: &str) {
        let id = order_row_id(item);
        let held: Vec<_> = cx
            .store
            .orders()
            .iter()
            .filter(|o| {
                id.as_ref().is_none_or(|id| {
                    o.id == *id
                        || o.parent_id.as_ref() == Some(id)
                        || o.id.strip_suffix("__stop_loss") == Some(id.as_str())
                        || o.id.strip_suffix("__take_profit") == Some(id.as_str())
                })
            })
            .map(|o| o.id.clone())
            .collect();
        for id in held {
            self.account.ws_order_clock = self.account.ws_order_clock.saturating_add(1);
            self.account
                .ws_order_revisions
                .insert(id, self.account.ws_order_clock);
        }
        row_skipped(cx, Level::Warn, "invalid Bitget order stream record");
        if self.account.orders_timer.is_none() {
            self.account.orders_timer = Some(cx.set_timer(500.0));
        }
    }

    fn normalized_symbol(&self, symbol: &str) -> String {
        symbol.split('_').next().unwrap_or(symbol).to_owned()
    }

    /// Identity of a position row that failed to map, if it has a symbol.
    fn position_key(&self, item: &str) -> Option<PositionKey> {
        #[derive(Deserialize)]
        struct Row<'a> {
            #[serde(default, borrow)]
            symbol: std::borrow::Cow<'a, str>,
            #[serde(default, rename = "instId", borrow)]
            inst_id: std::borrow::Cow<'a, str>,
            #[serde(default, rename = "holdSide", borrow)]
            hold_side: std::borrow::Cow<'a, str>,
            #[serde(default, rename = "posSide", borrow)]
            pos_side: std::borrow::Cow<'a, str>,
            #[serde(default, borrow)]
            side: std::borrow::Cow<'a, str>,
        }
        let row: Row<'_> = serde_json::from_str(item).ok()?;
        let symbol = self.normalized_symbol(first_text(&[&row.inst_id, &row.symbol]));
        let side = match first_text(&[&row.hold_side, &row.pos_side, &row.side]) {
            "short" | "sell" => Some(PositionSide::Short),
            "long" | "buy" => Some(PositionSide::Long),
            _ => None,
        };
        (!symbol.is_empty()).then_some((symbol, side))
    }

    fn map_position(
        &self,
        cx: &Cx<'_, Pending>,
        raw: &PositionRow<'_>,
    ) -> Result<Position, String> {
        let symbol = self.normalized_symbol(first_text(&[&raw.inst_id, &raw.symbol]));
        let native_contracts = first(&[
            raw.total,
            raw.size,
            raw.pos,
            raw.open_total_pos,
            raw.available,
        ])
        .ok_or("Bitget position has no size")?;
        if cx.store.market(&symbol).is_none() || native_contracts < 0.0 {
            return Err("invalid Bitget position symbol/size".into());
        }
        let side = if matches!(
            first_text(&[&raw.hold_side, &raw.pos_side, &raw.side]),
            "short" | "sell"
        ) {
            PositionSide::Short
        } else {
            PositionSide::Long
        };
        let raw_mode = first_text(&[&raw.pos_mode, &raw.hold_mode]);
        let mode = if is_hedged(raw_mode) || (raw_mode.is_empty() && cx.store.hedged()) {
            PositionMode::Hedged
        } else {
            PositionMode::Oneway
        };
        let id = format!("{symbol}:{side:?}:{mode:?}");
        let entry = if self.account.uta {
            first(&[
                raw.avg_price,
                raw.open_price_avg,
                raw.open_avg_price,
                raw.avg_px,
                raw.average_open_price,
            ])
        } else {
            first(&[
                raw.open_price_avg,
                raw.open_avg_price,
                raw.avg_price,
                raw.avg_px,
                raw.average_open_price,
            ])
        }
        .unwrap_or(0.0);
        let mark = first(&[raw.mark_price, raw.market_price])
            .filter(|n| *n > 0.0)
            .unwrap_or(entry);
        let realized = if self.account.uta {
            raw.cur_realised_pnl.get()
                + raw.total_funding.or(raw.total_funding_fee.get())
                + raw.open_fee_total.get()
                + raw.close_fee_total.get()
                - raw.deducted_fee.get()
        } else {
            raw.achieved_profits.get() + raw.total_fee.get() - raw.deducted_fee.get()
        };
        let contracts = if self.inverse() {
            // The row's own mark sizes first snapshots read before tickers load.
            let reference = [raw.mark_price, raw.market_price]
                .into_iter()
                .filter_map(|price| price.0)
                .find(|p| p.is_finite() && *p > 0.0)
                .or_else(|| {
                    cx.store
                        .ticker(&symbol)
                        .map(|t| if t.mark > 0.0 { t.mark } else { t.last })
                        .filter(|p| *p > 0.0)
                })
                .unwrap_or(entry);
            if reference > 0.0 {
                round8(native_contracts / reference)
            } else {
                native_contracts
            }
        } else {
            native_contracts
        };
        Ok(Position {
            id: id.clone(),
            symbol,
            side,
            entry_price: entry,
            notional: if self.inverse() {
                native_contracts
            } else {
                clean(contracts * mark)
            },
            leverage: first(&[raw.leverage, raw.lever]).unwrap_or(1.0),
            margin_type: Some(margin(&raw.margin_mode)),
            position_mode: Some(mode),
            unrealized_pnl: first(&[raw.unrealized_pl, raw.unrealised_pnl, raw.upl]).unwrap_or(0.0),
            realized_pnl: Some(clean(realized)),
            contracts,
            liquidation_price: first(&[raw.liquidation_price, raw.liq_price, raw.liq_px])
                .unwrap_or_else(|| cx.store.position(&id).map_or(0.0, |p| p.liquidation_price)),
            margin_size: first(&[raw.margin_size, raw.margin, raw.position_balance])
                .filter(|n| *n > 0.0),
            last_update: first(&[raw.u_time, raw.updated_time, raw.c_time]),
        })
    }

    pub(crate) fn map_order(
        &self,
        cx: &Cx<'_, Pending>,
        raw: &OrderRow<'_>,
    ) -> Result<Vec<Order>, String> {
        let id = [&raw.order_id, &raw.client_oid, &raw.ord_id, &raw.id]
            .into_iter()
            .map(Identifier::text)
            .find(|s| !s.is_empty())
            .ok_or("Bitget order has no id")?
            .into_owned();
        let symbol = self.normalized_symbol(first_text(&[&raw.symbol, &raw.inst_id]));
        if cx.store.market(&symbol).is_none() {
            return Err("unknown Bitget order market".into());
        }
        let status = order_status(raw_status(raw)).ok_or("unknown Bitget order status")?;
        let kind = order_type(first_text(&[
            if raw.strategy_type == "trailing_stop" {
                "trailing_stop"
            } else {
                ""
            },
            &raw.plan_type,
            &raw.order_type,
            &raw.ord_type,
        ]));
        let side_wire = first_text(&[&raw.trade_side, &raw.t_s, &raw.side]);
        let mut side = order_side(side_wire)
            .or_else(|| order_side(&raw.side))
            .unwrap_or(if raw.pos_side == "short" {
                OrderSide::Buy
            } else {
                OrderSide::Sell
            });
        let protected = first_text(&[&raw.hold_side, &raw.pos_side]);
        if (!raw.plan_type.is_empty() || (self.account.uta && kind.is_none()))
            && matches!(protected, "long" | "short" | "buy" | "sell")
        {
            side = if matches!(protected, "long" | "buy") {
                OrderSide::Sell
            } else {
                OrderSide::Buy
            };
        } else if !self.account.uta
            && is_hedged(first_text(&[&raw.pos_mode, &raw.hold_mode]))
            && (raw.reduce_only.0 || side_wire == "close")
            && matches!(side_wire, "buy" | "sell" | "close")
        {
            side = side.opposite();
        }
        let amount = first(&[raw.size, raw.sz, raw.qty, raw.amount]).unwrap_or(0.0);
        let filled = first(&[
            raw.acc_base_volume,
            raw.filled_qty,
            raw.acc_fill_sz,
            raw.cum_exec_qty,
            raw.base_volume,
        ])
        .unwrap_or(0.0);
        let tp = first(&[
            raw.preset_stop_surplus_price,
            raw.stop_surplus_trigger_price,
            raw.preset_take_profit_price,
            raw.take_profit,
        ])
        .filter(|v| *v > 0.0);
        let sl = first(&[
            raw.preset_stoploss_price,
            raw.stoploss_trigger_price,
            raw.stoploss,
        ])
        .filter(|v| *v > 0.0);
        let native_remaining = clean((amount - filled).max(0.0));
        let raw_price = first(&[raw.price, raw.px]).unwrap_or(0.0);
        let base = Order {
            id,
            parent_id: None,
            symbol,
            status,
            kind: kind.unwrap_or(OrderType::StopLoss),
            side,
            price: if raw.strategy_type == "trailing_stop" {
                raw.trailing_stop_params
                    .as_ref()
                    .and_then(|p| p.activation_price.0)
                    .ok_or("Bitget trailing order missing activation price")?
            } else if !raw.plan_type.is_empty() {
                first(&[raw.trigger_price, raw.trigger_px]).unwrap_or(0.0)
            } else {
                first(&[raw.price, raw.px]).unwrap_or(0.0)
            },
            amount,
            filled,
            remaining: if self.inverse() && raw_price > 0.0 {
                round8(native_remaining / raw_price)
            } else {
                native_remaining
            },
            reduce_only: raw.reduce_only.0
                || !raw.plan_type.is_empty()
                || (raw.strategy_type == "trailing_stop" && !unsupported_trailing_exposure(raw))
                || [&raw.trade_side, &raw.t_s, &raw.side].iter().any(|value| {
                    matches!(
                        value.as_ref(),
                        "close"
                            | "close_long"
                            | "close_short"
                            | "reduce_close_long"
                            | "reduce_close_short"
                            | "offset_close_long"
                            | "offset_close_short"
                            | "burst_close_long"
                            | "burst_close_short"
                            | "delivery_close_long"
                            | "delivery_close_short"
                    )
                }),
            close: None,
            client_order_id: Some(raw.client_oid.text().into_owned()).filter(|s| !s.is_empty()),
            time_in_force: Some(
                match first_text(&[&raw.force, &raw.time_in_force, &raw.ti_f]) {
                    "post_only" => TimeInForce::PostOnly,
                    "ioc" => TimeInForce::ImmediateOrCancel,
                    "fok" => TimeInForce::FillOrKill,
                    _ => TimeInForce::GoodTillCancel,
                },
            ),
            last_update: first(&[raw.u_time, raw.updated_time, raw.c_time, raw.created_time])
                .unwrap_or(cx.now),
            trigger_type: (!raw.plan_type.is_empty() || raw.strategy_type == "trailing_stop")
                .then_some(
                    if raw.trigger_type == "fill_price"
                        || raw
                            .trailing_stop_params
                            .as_ref()
                            .is_some_and(|p| p.activation_type == "market")
                    {
                        StopTriggerType::LastPrice
                    } else {
                        StopTriggerType::MarkPrice
                    },
                ),
            take_profit: tp,
            stop_loss: sl,
            extra: self.inverse().then_some(OrderExtra {
                remaining_native: Some(native_remaining),
                ..OrderExtra::default()
            }),
        };
        let mut orders = vec![];
        if kind.is_none() {
            for (kind, price, trigger) in [
                (OrderType::StopLoss, sl, &raw.sl_trigger_by),
                (OrderType::TakeProfit, tp, &raw.tp_trigger_by),
            ] {
                if let Some(price) = price {
                    let mut order = base.clone();
                    order.kind = kind;
                    if tp.is_some() && sl.is_some() {
                        order.id = format!(
                            "{}__{}",
                            base.id,
                            if kind == OrderType::StopLoss {
                                "stop_loss"
                            } else {
                                "take_profit"
                            }
                        );
                        order.parent_id = Some(base.id.clone());
                    }
                    order.price = price;
                    order.reduce_only = true;
                    order.trigger_type = Some(if trigger == "market" {
                        StopTriggerType::LastPrice
                    } else {
                        StopTriggerType::MarkPrice
                    });
                    orders.push(order);
                }
            }
            if orders.is_empty() {
                return Err("unknown Bitget order type".into());
            }
            return Ok(orders);
        }
        orders.push(base.clone());
        if base.kind == OrderType::Limit {
            for (kind, price, suffix) in [
                (OrderType::StopLoss, sl, "stop_loss"),
                (OrderType::TakeProfit, tp, "take_profit"),
            ] {
                if let Some(price) = price {
                    let mut child = base.clone();
                    child.id = format!("{}__{suffix}", base.id);
                    child.parent_id = Some(base.id.clone());
                    child.side = base.side.opposite();
                    child.kind = kind;
                    child.price = price;
                    orders.push(child);
                }
            }
        }
        Ok(orders)
    }
}

pub(crate) fn clean(v: f64) -> f64 {
    (v * 1e12).round() / 1e12
}
pub(crate) fn first(values: &[Number]) -> Option<f64> {
    values.iter().find_map(|v| v.0)
}
pub(crate) fn first_text<'a>(values: &[&'a str]) -> &'a str {
    values.iter().copied().find(|s| !s.is_empty()).unwrap_or("")
}
pub(crate) fn is_hedged(mode: &str) -> bool {
    matches!(mode, "hedge_mode" | "double_hold")
}
pub(crate) fn margin(mode: &str) -> MarginType {
    if matches!(mode, "crossed" | "cross") {
        MarginType::Cross
    } else {
        MarginType::Isolated
    }
}
pub(crate) fn is_algo(kind: OrderType) -> bool {
    matches!(
        kind,
        OrderType::StopLoss | OrderType::TakeProfit | OrderType::TrailingStopLoss
    )
}
pub(crate) fn plan_type(value: &str) -> Option<&'static str> {
    Some(match value {
        "psl" | "pos_loss" => "pos_loss",
        "ptp" | "pos_profit" => "pos_profit",
        "sl" | "loss_plan" => "loss_plan",
        "tp" | "profit_plan" => "profit_plan",
        "track" | "track_plan" => "track_plan",
        _ => return None,
    })
}
pub(crate) fn order_type(value: &str) -> Option<OrderType> {
    Some(match value {
        "limit" => OrderType::Limit,
        "market" => OrderType::Market,
        "pos_loss" | "loss_plan" | "psl" | "sl" => OrderType::StopLoss,
        "pos_profit" | "profit_plan" | "ptp" | "tp" => OrderType::TakeProfit,
        "track_plan" | "track" | "trailing_stop" => OrderType::TrailingStopLoss,
        _ => return None,
    })
}
pub(crate) fn order_status(value: &str) -> Option<OrderStatus> {
    Some(match value {
        "init" | "new" | "not_trigger" | "partial_fill" | "partially_fill" | "partially_filled"
        | "live" | "pending" | "submitting" => OrderStatus::Open,
        "triggered" | "full_fill" | "filled" | "success" | "executed" => OrderStatus::Closed,
        "fail_trigger" | "cancel" | "cancelled" | "canceled" | "failed" | "fail_execute" => {
            OrderStatus::Canceled
        }
        _ => return None,
    })
}
/// Native id of an order row that failed to map, chosen as `map_order` does.
fn order_row_id(item: &str) -> Option<String> {
    #[derive(Deserialize)]
    struct Row<'a> {
        #[serde(default, rename = "orderId", borrow)]
        order_id: Identifier<'a>,
        #[serde(default, rename = "clientOid", borrow)]
        client_oid: Identifier<'a>,
        #[serde(default, rename = "ordId", borrow)]
        ord_id: Identifier<'a>,
        #[serde(default, borrow)]
        id: Identifier<'a>,
    }
    let row: Row<'_> = serde_json::from_str(item).ok()?;
    [&row.order_id, &row.client_oid, &row.ord_id, &row.id]
        .into_iter()
        .map(Identifier::text)
        .find(|s| !s.is_empty())
        .map(std::borrow::Cow::into_owned)
}
fn raw_status<'a>(raw: &'a OrderRow<'_>) -> &'a str {
    first_text(&[&raw.status, &raw.state, &raw.plan_status, &raw.order_status])
}
pub(crate) fn order_side(value: &str) -> Option<OrderSide> {
    Some(match value {
        "buy"
        | "buy_single"
        | "open_long"
        | "close_short"
        | "reduce_close_short"
        | "offset_close_short"
        | "burst_close_short"
        | "delivery_close_short" => OrderSide::Buy,
        "sell"
        | "sell_single"
        | "open_short"
        | "close_long"
        | "reduce_close_long"
        | "offset_close_long"
        | "burst_close_long"
        | "delivery_close_long" => OrderSide::Sell,
        _ => return None,
    })
}
fn classic_amounts(row: &AccountRow<'_>) -> Option<Amounts> {
    Some(Amounts {
        equity: first(&[row.union_total_margin, row.account_equity, row.equity])?,
        free: first(&[row.union_available, row.available])?,
        upnl: row
            .unrealized_pl
            .or(row.crossed_unrealized_pl.get() + row.isolated_unrealized_pl.get()),
    })
}
fn uta_amounts(row: &UtaBalance<'_>, coin: &str) -> Option<Amounts> {
    let asset = row.assets.iter().find(|a| a.coin == coin);
    Some(Amounts {
        equity: row
            .account_equity
            .0
            .or(row.usdt_equity.0)
            .or_else(|| asset.and_then(|a| a.usd_value.0.or(a.equity.0)))?,
        free: row
            .eff_equity
            .0
            .or_else(|| asset.and_then(|a| a.available.0))?,
        upnl: first(&[row.unrealised_pnl, row.usdt_unrealised_pnl]).unwrap_or(0.0),
    })
}

fn round8(value: f64) -> f64 {
    (value * 100_000_000.0).round() / 100_000_000.0
}

fn unsupported_trailing_exposure(raw: &OrderRow<'_>) -> bool {
    if raw.strategy_type != "trailing_stop" {
        return false;
    }
    match (raw.side.as_ref(), raw.pos_side.as_ref()) {
        ("buy", "long" | "buy") | ("sell", "short" | "sell") => true,
        ("sell", "long" | "buy") | ("buy", "short" | "sell") => false,
        // Replacing side-only or net orders is safe only with explicit reduce-only intent.
        _ => !raw.reduce_only.0,
    }
}

fn trailing_state(raw: &OrderRow<'_>) -> Option<(String, f64)> {
    if let Some(params) = &raw.trailing_stop_params {
        let value = params.trail_variance.0.filter(|v| *v > 0.0)?;
        if unsupported_trailing_exposure(raw)
            || (!params.pre_order_type.is_empty() && params.pre_order_type != "market")
            || (!params.activation_type.is_empty()
                && !matches!(params.activation_type.as_str(), "mark" | "market"))
        {
            return Some(("unsupported".into(), value));
        }
        if !matches!(params.trail_type.as_str(), "distance" | "spread" | "ratio") {
            return None;
        }
        // REST names price distance "spread"; placement names the same mode "distance".
        Some((
            if params.trail_type == "spread" {
                "distance".into()
            } else {
                params.trail_type.clone()
            },
            value,
        ))
    } else {
        raw.callback_ratio
            .0
            .filter(|v| *v > 0.0)
            .map(|v| ("ratio".to_owned(), v))
    }
}

/// Bitget key refusals no retry can fix (REST common codes): empty or invalid
/// key/passphrase, missing key, permission or IP, wrong environment. 40009 (bad
/// signature) is final only after one re-signed resend; others stay transient.
fn key_rejection(code: f64, msg: &str) -> Option<ErrorKind> {
    match code as i64 {
        40001 | 40006 | 40009 | 40011 | 40012 | 40037 => Some(ErrorKind::AuthInvalid),
        40014 | 40018 => Some(ErrorKind::AuthPermission),
        _ if msg
            .to_ascii_lowercase()
            .contains("environment is incorrect") =>
        {
            Some(ErrorKind::AuthInvalid)
        }
        _ => None,
    }
}

/// Terminal WS login refusals require an exact integer code before f64 decoding:
/// nearby fractions may otherwise round to 30011 or 30012.
fn socket_key_rejection(text: &str) -> Option<f64> {
    #[derive(Deserialize)]
    struct Header<'a> {
        #[serde(borrow)]
        code: Option<&'a serde_json::value::RawValue>,
    }
    let header: Header<'_> = serde_json::from_str(text).ok()?;
    let raw = header.code?.get();
    let code: std::borrow::Cow<'_, str> = if raw.starts_with('"') {
        serde_json::from_str(raw).ok()?
    } else {
        std::borrow::Cow::Borrowed(raw)
    };
    match code.as_ref() {
        "30011" => Some(30011.0),
        "30012" => Some(30012.0),
        _ => None,
    }
}

fn rejection(which: &str, code: f64, msg: &str, kind: ErrorKind) -> VenueError {
    let code = safe_cex_engine::num::format(code);
    let mut error = VenueError::new(format!("Bitget rejected the {which} ({code}): {msg}"))
        .kind(kind)
        .venue_code(code);
    if !msg.is_empty() {
        error = error.venue_message(msg);
    }
    error
}

/// A failed account read, classified by the Bitget table: the envelope's integer
/// code when it has one, else the HTTP status. The text is unchanged.
pub(crate) fn read_error(response: &Response<'_>, message: String) -> VenueError {
    use safe_cex_engine::classify::{self, Native, NativeCode, Released};
    let envelope = serde_json::from_str::<Envelope<IgnoredAny>>(response.body).ok();
    let code = envelope
        .as_ref()
        .and_then(|envelope| envelope.code.0)
        .filter(|code| code.fract() == 0.0 && *code != 0.0);
    let native_message = envelope
        .as_ref()
        .map_or("", |envelope| envelope.msg.as_str());
    let native = code.map(|code| Native {
        code: NativeCode::Int(code as i64),
        message: native_message,
    });
    classify::read(
        &crate::trading::CODES,
        Released::Response(response),
        native.as_ref(),
        message,
    )
    .error
}

#[cfg(test)]
mod secondary_policy_tests {
    use super::*;
    #[test]
    fn degraded_balance_uses_primary_fast_client_but_other_gets_use_read_identity() {
        for degraded in [false, true] {
            for path in [paths::BALANCE, paths::UTA_ASSETS, paths::POSITIONS] {
                let (mut venue, provider) = crate::secondary_tests::setup();
                venue.account.degraded = degraded;
                let primary = venue.primary_balance_read(path);
                let headers = crate::secondary_tests::headers(venue, provider, Method::Get, path);
                let expected = if degraded && path != paths::POSITIONS {
                    "primary-key"
                } else {
                    "read-key"
                };
                assert!(headers.contains(&("ACCESS-KEY", expected.into())));
                assert_eq!(primary, degraded && path != paths::POSITIONS);
            }
        }
    }
}

#[cfg(test)]
mod key_refusal_tests {
    use super::*;
    use safe_cex_engine::classify::{Intent, Native, NativeCode, Verdict};

    /// A write and a read refused with the same key code carry the same kind;
    /// 40009 keeps its one re-signed resend first.
    #[test]
    fn write_and_read_key_refusals_agree() {
        for code in 40000..40100 {
            let Some(kind) = key_rejection(code as f64, "") else {
                continue;
            };
            let native = Native {
                code: NativeCode::Int(code),
                message: "native",
            };
            let verdict = crate::trading::CODES.verdict(&native, Intent::Place);
            assert!(
                matches!(verdict, Verdict::Refused(k) | Verdict::RefusedResend(k) if k == kind),
                "{code}: {verdict:?}"
            );
        }
    }
}
