//! Private streams and atomic REST reconciliation, guarded against newer pushes.
use crate::wire::*;
use crate::{Okx, Pending, scaled, usd};
use safe_cex_engine::Level;
use safe_cex_engine::coverage::SnapshotBuilder;
use safe_cex_engine::log::code;
use safe_cex_engine::model::*;
use safe_cex_engine::{
    CommandId, Cx, ErrorKind, Fill, Method, Response, TimerId, VenueError, json, num,
};
use std::collections::{HashMap, HashSet};

pub const PRIVATE: usize = 1;
const CONFIG_POLL_MS: f64 = 30_000.0;
const ORDER_KINDS: [&str; 5] = ["", "oco", "conditional", "trigger", "move_order_stop"];
const SKIP_LOG_LIMIT: usize = 4_096;
/// Store-id suffixes one native order row may expand into.
const ORDER_SUFFIXES: [&str; 4] = ["", "_sl", "_tp", "_tsl"];

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

/// What a list row that failed to decode still says about its identity.
#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct RowIdentity {
    #[serde(deserialize_with = "lenient")]
    inst_id: String,
    #[serde(deserialize_with = "lenient")]
    inst_type: String,
    #[serde(deserialize_with = "lenient")]
    ord_id: String,
    #[serde(deserialize_with = "lenient")]
    algo_id: String,
    #[serde(deserialize_with = "lenient")]
    state: String,
}

/// A string field, or empty when it holds anything else; one bad field keeps the rest.
fn lenient<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    use serde::Deserialize;
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Field<'a> {
        Text(#[serde(borrow)] std::borrow::Cow<'a, str>),
        Other(serde::de::IgnoredAny),
    }
    Ok(match Field::deserialize(d)? {
        Field::Text(text) => text.into_owned(),
        Field::Other(_) => String::new(),
    })
}

fn terminal_state(state: &str) -> bool {
    matches!(
        state,
        "filled" | "canceled" | "mmp_canceled" | "order_failed" | "effective"
    )
}

pub enum Request {
    Config {
        mode_version: u64,
    },
    Leverage {
        command: CommandId,
        symbol: String,
        inst_id: String,
        version: u64,
    },
    Balance {
        generation: u64,
        version: u64,
        equity: SnapshotBuilder<String, (String, f64)>,
        risk_rows: usize,
        invalid: bool,
    },
    Positions {
        generation: u64,
        version: u64,
        product: usize,
        rows: SnapshotBuilder<String, (Position, RiskValues)>,
        invalid: bool,
    },
    Orders {
        generation: u64,
        version: u64,
        kind: usize,
        product: usize,
        rows: SnapshotBuilder<String, Order>,
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
    positions: Option<SnapshotBuilder<String, (Position, RiskValues)>>,
    equity: Option<SnapshotBuilder<String, (String, f64)>>,
    position_version: u64,
    balance_version: u64,
}

#[derive(Default)]
pub struct AccountState {
    pub ready: bool,
    configured: bool,
    config_in_flight: bool,
    config_read_at: f64,
    pub mode_version: u64,
    pub leverage_versions: HashMap<String, u64>,
    skipped_instruments_logged: HashSet<String>,
    unlisted_logged: HashSet<String>,
    /// `position:`/`order:`/`coin:` + identity (empty when unreadable) of skips already logged.
    skip_logged: HashSet<String>,
    account_level: u8,
    pub revision: u64,
    position_versions: HashMap<String, u64>,
    order_versions: HashMap<String, (u64, f64, bool)>,
    balance_version: u64,
    join: Option<Join>,
    generation: u64,
    orders_in_flight: bool,
    orders_generation: u64,
    risk: HashMap<String, RiskValues>,
    order_leverage: HashMap<String, (f64, String)>,
    equity: Vec<(String, f64)>,
    total_equity: Option<f64>,
    /// A currency row was rejected: retained stale equity cannot be summed with fresh rows.
    aggregate_held: bool,
    equity_loaded: bool,
    valuation_pending: bool,
    fills: HashMap<String, f64>,
    tick: Option<TimerId>,
    retry: Option<TimerId>,
    /// Backs off the config read that gates startup; transient failures never end it.
    startup: safe_cex_engine::startup::Backoff<()>,
    /// Pause the latest 418/429 config answer asked for; a transport loss has none.
    pub(crate) config_pause_ms: f64,
    /// Consecutive 50113 answers to the config read; two in a row reject the key.
    signature_rejections: u8,
    pub startup_rejected: bool,
}
impl AccountState {
    pub(crate) fn order_final(&self, id: &str) -> bool {
        self.order_versions
            .get(id)
            .is_some_and(|(_, _, final_state)| *final_state)
    }
}

impl Okx {
    pub(crate) fn account_after_uncertain_write(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account.join = None;
        self.account.orders_generation += 1;
        self.account.orders_in_flight = false;
        self.account.revision += 1;
        self.account_refresh(cx);
    }

    pub(crate) fn account_after_settings(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account.join = None;
        self.account.generation += 1;
        self.account.revision += 1;
        self.reconcile(cx, false);
    }

    pub(crate) fn account_start(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.credentials.is_none()
            || self.account.config_in_flight
            || self.account.startup_rejected
        {
            return;
        }
        self.account.config_in_flight = true;
        self.get(
            cx,
            "/api/v5/account/config",
            true,
            Pending::Account(Request::Config {
                mode_version: self.account.mode_version,
            }),
        );
    }

    pub(crate) fn fetch_leverage(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        symbol: String,
    ) {
        let Some(m) = cx.store.market(&symbol) else {
            cx.fail(command, format!("unknown market {symbol}"));
            return;
        };
        if self.credentials.is_none() {
            cx.fail(command, "OKX leverage requires credentials");
            return;
        }
        let inst_id = m.id.clone();
        let version = self
            .account
            .leverage_versions
            .get(&symbol)
            .copied()
            .unwrap_or(0);
        self.get(
            cx,
            &format!("/api/v5/account/leverage-info?instId={inst_id}&mgnMode=cross"),
            true,
            Pending::Account(Request::Leverage {
                command,
                symbol,
                inst_id,
                version,
            }),
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
                    equity: SnapshotBuilder::default(),
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
                    rows: SnapshotBuilder::default(),
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
                    rows: SnapshotBuilder::default(),
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
        if let (Some(c), Some(socket)) = (
            &self.credentials,
            self.connections[channel]
                .feed
                .and_then(|f| cx.socket_epoch(f))
                .map(|e| e.socket()),
        ) {
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

    fn account_request_current(&self, request: &Request) -> bool {
        match request {
            Request::Balance { generation, .. } | Request::Positions { generation, .. } => self
                .account
                .join
                .as_ref()
                .is_some_and(|join| join.generation == *generation),
            Request::Orders { generation, .. } => self.account.orders_generation == *generation,
            Request::Config { .. } | Request::Leverage { .. } => true,
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
            Request::Config { .. } | Request::Leverage { .. } => {}
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
                            // A currency row that cannot be read keeps its stored equity.
                            let message = if e.ccy.is_empty() {
                                "OKX skipped balance row without a currency".into()
                            } else {
                                format!("OKX skipped malformed {} balance row", e.ccy)
                            };
                            self.log_skip(cx, format!("coin:{}", e.ccy), Level::Warn, message);
                            equity.reject((!e.ccy.is_empty()).then(|| e.ccy.to_owned()));
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
                    if cx.store.market_by_id(p.inst_id).is_none()
                        && p.pos.parse::<f64>().ok() != Some(0.0)
                    {
                        if self.unlisted_exposure(cx, p.inst_id) {
                            *invalid = true;
                            cx.notice(Level::Warn, code::SNAPSHOT_DISCARDED, "OKX owned position market unavailable; preserving account snapshot");
                        } else {
                            self.unlisted_skipped(cx, p.inst_id);
                        }
                        return;
                    }
                    if let Some(row) = self.position(cx, &p) {
                        rows.push(row);
                    } else if mark_pending(cx, &p) {
                        // No price can value it yet; the account snapshot waits for one.
                        *invalid = true;
                    } else if p.inst_id.is_empty() || cx.store.market_by_id(p.inst_id).is_some() {
                        let owned = p.pos.parse::<f64>().ok() != Some(0.0);
                        self.reject_position(cx, rows, p.inst_id, owned);
                    }
                }
                Err(_) => {
                    let row = serde_json::from_str::<RowIdentity>(item).unwrap_or_default();
                    if !self.account_scope(*product, &row.inst_id, &row.inst_type) {
                        *invalid = true;
                        return;
                    }
                    let inst_id = row.inst_id.as_str();
                    if !inst_id.is_empty() && cx.store.market_by_id(inst_id).is_none() {
                        if self.unlisted_exposure(cx, inst_id) {
                            *invalid = true;
                        } else {
                            self.unlisted_skipped(cx, inst_id);
                        }
                        return;
                    }
                    self.reject_position(cx, rows, inst_id, true);
                }
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
                    if !self.account_scope(*product, o.inst_id, o.inst_type) {
                        *invalid = true;
                        return;
                    }
                    if o.inst_id.is_empty()
                        || (cx.store.market_by_id(o.inst_id).is_some() && !valid_order(&o))
                    {
                        self.reject_order(cx, rows, o.ord_id, o.algo_id);
                        return;
                    }
                    if cx.store.market_by_id(o.inst_id).is_none() && !terminal_state(o.state) {
                        self.unlisted_order(cx, o.inst_id, invalid);
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
                // Still a page row: paging counts it and resumes after its id when readable.
                Err(_) => {
                    *page_count += 1;
                    let row = serde_json::from_str::<RowIdentity>(item).unwrap_or_default();
                    let id = if row.algo_id.is_empty() {
                        &row.ord_id
                    } else {
                        &row.algo_id
                    };
                    if !id.is_empty() {
                        cursor.clone_from(id);
                    }
                    if !self.account_scope(*product, &row.inst_id, &row.inst_type) {
                        *invalid = true;
                        return;
                    }
                    if !row.inst_id.is_empty() && cx.store.market_by_id(&row.inst_id).is_none() {
                        // An unreadable state may be working, so it is held like one.
                        if !terminal_state(&row.state) {
                            self.unlisted_order(cx, &row.inst_id, invalid);
                        }
                        return;
                    }
                    self.reject_order(cx, rows, &row.ord_id, &row.algo_id);
                }
            },
        }
    }

    /// Rejects every leg id a position row on `inst_id` may carry; an empty id protects all.
    /// An owned row with no stored copy is reported as an error: it commits as flat.
    fn reject_position(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        rows: &mut SnapshotBuilder<String, (Position, RiskValues)>,
        inst_id: &str,
        owned: bool,
    ) {
        let Some(symbol) = cx.store.market_by_id(inst_id).map(|m| m.symbol.clone()) else {
            let level = if cx.store.positions().is_empty() {
                Level::Error
            } else {
                Level::Warn
            };
            let message = "OKX skipped position row without an instrument";
            self.log_skip(cx, "position:".into(), level, message);
            rows.reject(None);
            return;
        };
        let stored = cx.store.positions().iter().any(|p| p.symbol == symbol);
        let level = if owned && !stored {
            Level::Error
        } else {
            Level::Warn
        };
        let message = format!("OKX skipped unmappable {symbol} position row");
        self.log_skip(cx, format!("position:{symbol}"), level, message);
        for id in [
            symbol.clone(),
            format!("{symbol}:long"),
            format!("{symbol}:short"),
        ] {
            rows.reject(Some(id));
        }
    }

    /// Rejects every store id an order row may expand into; a row with no id protects all.
    fn reject_order(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        rows: &mut SnapshotBuilder<String, Order>,
        ord_id: &str,
        algo_id: &str,
    ) {
        let ids: Vec<&str> = [ord_id, algo_id]
            .into_iter()
            .filter(|id| !id.is_empty())
            .collect();
        let Some(first) = ids.first() else {
            let message = "OKX skipped order row without an id";
            self.log_skip(cx, "order:".into(), Level::Warn, message);
            rows.reject(None);
            return;
        };
        let message = format!("OKX skipped unmappable order {first}");
        self.log_skip(cx, format!("order:{first}"), Level::Warn, message);
        for id in ids {
            for suffix in ORDER_SUFFIXES {
                rows.reject(Some(format!("{id}{suffix}")));
            }
        }
    }

    /// Logs a skipped row once per `key` until it maps again; polls would otherwise repeat it.
    fn log_skip(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        key: String,
        level: Level,
        message: impl Into<String>,
    ) {
        let logged = &mut self.account.skip_logged;
        if logged.len() >= SKIP_LOG_LIMIT {
            logged.clear();
        }
        if logged.insert(key) {
            cx.notice(level, code::ROW_SKIPPED, message);
        }
    }

    /// Re-arms the skip logs of `prefix`: all after a clean read, else those `ids` mapped.
    fn rearm_skips<'a>(
        &mut self,
        prefix: &str,
        clean: bool,
        ids: impl IntoIterator<Item = &'a str>,
    ) {
        let logged = &mut self.account.skip_logged;
        if clean {
            logged.retain(|key| !key.starts_with(prefix));
        } else {
            for id in ids {
                logged.remove(&format!("{prefix}{id}"));
            }
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
            Request::Leverage {
                command,
                symbol,
                inst_id,
                version,
            } => {
                let result = self.leverage_result(cx, data, &symbol, &inst_id, version);
                match result {
                    Ok(data) => cx.done(command, data),
                    Err(error) => cx.fail(command, error),
                };
            }
            Request::Config { mode_version } => {
                self.account.config_in_flight = false;
                let configs = serde_json::from_str::<Vec<Config<'_>>>(data);
                let Ok(configs) = configs else {
                    self.account_failed(
                        cx,
                        Request::Config { mode_version },
                        "malformed OKX config",
                    );
                    return;
                };
                let Some(config) = configs
                    .first()
                    .filter(|c| matches!(c.pos_mode, "net_mode" | "long_short_mode"))
                else {
                    self.account_failed(
                        cx,
                        Request::Config { mode_version },
                        "missing OKX position mode",
                    );
                    return;
                };
                self.account.config_read_at = cx.now;
                self.account.startup.succeeded(cx, &());
                let hedged = config.pos_mode == "long_short_mode";
                let level = config.acct_lv.parse().ok();
                if self.account.configured {
                    // A read sent before our own mode write cannot overrule it.
                    if mode_version != self.account.mode_version {
                        return;
                    }
                    // A mode changed in the OKX app makes every posSide we send wrong.
                    let level = level.unwrap_or(self.account.account_level);
                    if hedged != cx.store.hedged() || level != self.account.account_level {
                        cx.store.set_hedged(hedged);
                        self.account.account_level = level;
                        self.account_after_settings(cx);
                    }
                    return;
                }
                let level = level.unwrap_or(2);
                cx.store.set_hedged(hedged);
                self.account.account_level = level;
                self.account.configured = true;
                self.ensure_feed(cx, PRIVATE);
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
                        cx.retried_failure("okx.account:664", "OKX order paging did not advance");
                        if self.account.retry.is_none() {
                            self.account.retry = Some(cx.set_timer(5_000.0));
                        }
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
                let clean = rows.rejected() == 0;
                let mapped: Vec<String> = rows
                    .rows()
                    .iter()
                    .filter(|o| rows.allows_absence(&o.id))
                    .map(|o| o.id.clone())
                    .collect();
                self.rearm_skips("order:", clean, mapped.iter().map(String::as_str));
                // Rejected rows keep their stored copies; versions and terminals still apply.
                let mut rows: Vec<_> = rows
                    .commit(cx.store.orders().iter().cloned(), |o| o.id.clone())
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
                // A rejected row may be an uncertain placement; it proves nothing.
                if clean {
                    cx.orders_reconciled(None);
                }
                self.trading
                    .acknowledged
                    .retain(|_, order| order.version > version);
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
        let builder = join.positions.expect("positions ready");
        let mapped: Vec<String> = builder
            .rows()
            .iter()
            .filter(|(p, _)| builder.allows_absence(&p.id))
            .map(|(p, _)| p.symbol.clone())
            .collect();
        self.rearm_skips(
            "position:",
            builder.rejected() == 0,
            mapped.iter().map(String::as_str),
        );
        // Rejected rows keep their stored copies, with the risk counted for them.
        let current: Vec<_> = cx
            .store
            .positions()
            .iter()
            .map(|p| {
                let risk = self.account.risk.get(&p.id).cloned().unwrap_or_default();
                (p.clone(), risk)
            })
            .collect();
        let rows = builder.commit(current, |(p, _)| p.id.clone());
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
            let equity = join.equity.expect("equity ready");
            let mapped: Vec<String> = equity
                .rows()
                .iter()
                .filter(|(ccy, _)| equity.allows_absence(ccy))
                .map(|(ccy, _)| ccy.clone())
                .collect();
            self.rearm_skips(
                "coin:",
                equity.rejected() == 0,
                mapped.iter().map(String::as_str),
            );
            self.account.aggregate_held = equity.rejected() > 0;
            let current = std::mem::take(&mut self.account.equity);
            self.account.equity = equity.commit(current, |(ccy, _)| ccy.clone());
            self.account.total_equity = None;
        }
        self.account.equity_loaded = true;
        self.recalculate(cx);
        self.account.ready = cx.store.loaded().orders && cx.store.loaded().balance;
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
            // Clearing the in-flight flag lets the backoff timer re-enter account_start.
            Request::Config { .. } => {
                self.account.config_in_flight = false;
                if self.account.configured {
                    self.account.config_read_at = cx.now;
                    cx.notice(Level::Warn, code::REQUEST_FAILED, error);
                } else {
                    cx.retried_failure("okx.account:841", error);
                    let pause = std::mem::take(&mut self.account.config_pause_ms);
                    self.account.startup.failed_after(cx, (), pause);
                }
                return;
            }
            Request::Leverage { command, .. } => {
                cx.fail(command, error.to_owned());
                return;
            }
        }
        cx.retried_failure("okx.account:852", error);
        if self.account.retry.is_none() {
            self.account.retry = Some(cx.set_timer(5_000.0));
        }
    }

    /// A refused key can never pass the config read, so startup fails instead
    /// of polling. Only documented key codes count; 50113 only twice in a row,
    /// i.e. after one re-signed resend. Any other answer ends the run.
    pub(crate) fn config_rejected(&mut self, cx: &mut Cx<'_, Pending>, body: &str) -> bool {
        let envelope = serde_json::from_str::<Envelope<'_>>(body).ok();
        if envelope.as_ref().is_some_and(|e| e.code == "50113") {
            self.account.signature_rejections += 1;
        } else {
            self.account.signature_rejections = 0;
        }
        if self.account.configured {
            return false;
        }
        let Some(envelope) = envelope else {
            return false;
        };
        let Some(kind) = rest_key_rejection(envelope.code) else {
            return false;
        };
        if envelope.code == "50113" && self.account.signature_rejections < 2 {
            return false;
        }
        self.account.config_in_flight = false;
        let read_key = self.options.read_credentials.is_some();
        let which = if read_key {
            "read-only API key"
        } else {
            "API key"
        };
        let error = rejection(which, envelope.code, envelope.msg, kind);
        self.reject_startup(cx, error, kind == ErrorKind::AuthInvalid && !read_key);
        true
    }

    /// A config read lost in transport is not a 50113 answer, so it ends the run.
    pub(crate) fn config_unanswered(&mut self) {
        self.account.signature_rejections = 0;
    }

    pub(crate) fn socket_account_is_ready(&self) -> bool {
        self.account.ready
    }
    pub(crate) fn socket_account_ready(&mut self, cx: &mut Cx<'_, Pending>, reopened: bool) {
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
    }

    /// Fails startup once and retires every private retry, read or socket.
    pub(crate) fn reject_startup(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        error: VenueError,
        trading_key_refused: bool,
    ) {
        if std::mem::replace(&mut self.account.startup_rejected, true) {
            return;
        }
        self.account.startup.succeeded(cx, &());
        if let Some(timer) = self.account.retry.take() {
            cx.clear_timer(timer);
        }
        for channel in [PRIVATE, crate::BUSINESS] {
            if let Some(feed) = self.connections[channel].feed {
                cx.stop_socket_feed(feed);
            }
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

    pub(crate) fn account_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if self.account.tick == Some(timer) {
            self.account.tick = Some(cx.set_timer(2_000.0));
            if self.account.configured && cx.now - self.account.config_read_at >= CONFIG_POLL_MS {
                self.account_start(cx);
            }
            self.recalculate(cx);
            self.account.fills.retain(|_, at| cx.now - *at < 120_000.0);
            self.account
                .order_versions
                .retain(|_, (_, at, terminal)| !*terminal || cx.now - *at < 600_000.0);
            return true;
        }
        if self.account.startup.on_timer(timer).is_some() {
            self.account_start(cx);
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

    fn leverage_result(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        data: &str,
        symbol: &str,
        inst_id: &str,
        version: u64,
    ) -> Result<String, String> {
        let rows = serde_json::from_str::<Vec<LeverageRow<'_>>>(data)
            .map_err(|_| "malformed OKX leverage-info".to_owned())?;
        // Cross leverage is per instrument on OKX; hedge mode repeats it per side.
        let leverage = rows
            .iter()
            .filter(|r| r.inst_id == inst_id && r.mgn_mode == "cross")
            .filter_map(|r| r.lever.parse::<f64>().ok())
            .find(|v| v.is_finite() && *v > 0.0)
            .ok_or_else(|| format!("OKX returned no cross leverage for {symbol}"))?;
        // A leverage write since this read was sent owns the setting; report it.
        let leverage = if self
            .account
            .leverage_versions
            .get(symbol)
            .copied()
            .unwrap_or(0)
            == version
        {
            cx.store.update_setting(symbol, |s| {
                s.leverage = leverage;
                s.margin_type = Some(MarginType::Cross);
            });
            leverage
        } else {
            cx.store.setting(symbol).map_or(leverage, |s| s.leverage)
        };
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Fetched {
            buy_leverage: f64,
            sell_leverage: f64,
            margin_type: MarginType,
            position_mode: PositionMode,
        }
        serde_json::to_string(&Fetched {
            buy_leverage: leverage,
            sell_leverage: leverage,
            margin_type: MarginType::Cross,
            position_mode: if cx.store.hedged() {
                PositionMode::Hedged
            } else {
                PositionMode::Oneway
            },
        })
        .map_err(|e| e.to_string())
    }

    /// Exposure on an instrument whose catalog row was malformed holds the snapshot;
    /// any other unlisted instrument was filtered or delisted and is skipped.
    fn unlisted_exposure(&mut self, cx: &mut Cx<'_, Pending>, inst_id: &str) -> bool {
        if !self.skipped_instruments.contains(inst_id) {
            return false;
        }
        if self
            .account
            .skipped_instruments_logged
            .insert(inst_id.into())
        {
            cx.notice(Level::Error, code::ROW_SKIPPED,
                format!("OKX {inst_id} has open exposure but its instrument row was malformed; it is not shown"),
            );
        }
        true
    }

    /// A working order on a catalogue-malformed market holds the snapshot; others are skipped.
    fn unlisted_order(&mut self, cx: &mut Cx<'_, Pending>, inst_id: &str, invalid: &mut bool) {
        if self.unlisted_exposure(cx, inst_id) {
            *invalid = true;
            cx.notice(
                Level::Warn,
                code::SNAPSHOT_DISCARDED,
                "OKX working order market unavailable; preserving order snapshot",
            );
        } else {
            self.unlisted_skipped(cx, inst_id);
        }
    }

    fn unlisted_skipped(&mut self, cx: &mut Cx<'_, Pending>, inst_id: &str) {
        if self.account.unlisted_logged.insert(inst_id.into()) {
            cx.notice(
                Level::Warn,
                code::ROW_SKIPPED,
                format!(
                    "OKX {inst_id} is not a listed market; its positions and orders are skipped"
                ),
            );
        }
    }

    fn setting(cx: &mut Cx<'_, Pending>, position: &Position) {
        cx.store.update_setting(&position.symbol, |s| {
            s.leverage = position.leverage;
            s.margin_type = position.margin_type;
        });
    }

    pub(crate) fn account_prices(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.valuation_pending {
            let mut positions = cx.store.positions().to_vec();
            let mut risk_updates = Vec::new();
            for position in &mut positions {
                let Some(market) = cx.store.market(&position.symbol) else {
                    continue;
                };
                let Some(risk) = self.account.risk.get(&position.id) else {
                    continue;
                };
                if market.is_inverse == Some(true)
                    && risk.currency.eq_ignore_ascii_case(&market.base)
                    && let Some(rate) = self.public_currency_rate(cx, &risk.currency)
                {
                    let contracts = position.notional / rate;
                    let imr = risk.native_imr * rate;
                    let upl = risk.native_upl * rate;
                    if !contracts.is_finite() || !imr.is_finite() || !upl.is_finite() {
                        cx.notice(
                            Level::Warn,
                            code::VALUATION_UNAVAILABLE,
                            "OKX collateral FX overflow; retaining previous account valuation",
                        );
                        return;
                    }
                    position.contracts = contracts;
                    position.unrealized_pnl = num::adjust(upl, market.precision.price);
                    risk_updates.push((position.id.clone(), imr, upl));
                }
            }
            if !risk_updates.is_empty() {
                for (id, imr, upl) in risk_updates {
                    if let Some(risk) = self.account.risk.get_mut(&id) {
                        risk.imr = imr;
                        risk.upl = upl;
                    }
                }
                cx.store.set_positions(positions);
            }
            self.recalculate(cx);
            self.account.ready = cx.store.loaded().positions
                && cx.store.loaded().orders
                && cx.store.loaded().balance;
        }
    }

    fn public_currency_rate(&self, cx: &Cx<'_, Pending>, currency: &str) -> Option<f64> {
        let stable = |coin: &str| {
            ["USD", "USDT", "USDC"]
                .iter()
                .any(|stable| coin.eq_ignore_ascii_case(stable))
        };
        if stable(currency) {
            return Some(1.0);
        }
        for preference in 0..3 {
            for market in cx.store.markets().iter().filter(|market| {
                let priority = if market.id.ends_with("-SWAP") {
                    if market.quote.eq_ignore_ascii_case("USDT") {
                        0
                    } else {
                        1
                    }
                } else {
                    2
                };
                market.base.eq_ignore_ascii_case(currency)
                    && stable(&market.quote)
                    && preference == priority
            }) {
                if let Some(rate) = cx.store.ticker(&market.symbol).and_then(|ticker| {
                    [ticker.mark, ticker.last]
                        .into_iter()
                        .find(|price| price.is_finite() && *price > 0.0)
                }) {
                    return Some(rate);
                }
            }
        }
        None
    }

    fn currency_rate(&self, cx: &Cx<'_, Pending>, currency: &str) -> Option<f64> {
        self.public_currency_rate(cx, currency).or_else(|| {
            // Position notional below uses only a validated native/public mark.
            cx.store.positions().iter().find_map(|position| {
                let market = cx.store.market(&position.symbol)?;
                if !market.base.eq_ignore_ascii_case(currency)
                    || !["USD", "USDT", "USDC"]
                        .iter()
                        .any(|coin| market.quote.eq_ignore_ascii_case(coin))
                    || position.contracts <= 0.0
                {
                    return None;
                }
                let price = position.notional / position.contracts;
                (price.is_finite() && price > 0.0).then_some(price)
            })
        })
    }

    fn publish_currencies(&self, cx: &mut Cx<'_, Pending>) {
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

    fn recalculate(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.account.equity_loaded {
            return;
        }
        self.publish_currencies(cx);
        // The USD balance keeps its previous value until every currency row reads again.
        if self.account.aggregate_held {
            return;
        }
        let collateral = self
            .account
            .total_equity
            .or_else(|| {
                self.account
                    .equity
                    .iter()
                    .try_fold(0.0, |total, (currency, equity)| {
                        let rate = if *equity == 0.0 {
                            1.0
                        } else {
                            self.currency_rate(cx, currency)?
                        };
                        let total = total + equity * rate;
                        total.is_finite().then_some(total)
                    })
            })
            .filter(|total| total.is_finite());
        let Some(collateral) = collateral else {
            if !self.account.valuation_pending {
                cx.notice(Level::Warn, code::VALUATION_UNAVAILABLE, "OKX collateral price unavailable; retaining native currencies and previous USD balance");
            }
            self.account.valuation_pending = true;
            return;
        };
        // Native position marks prove startup FX, but the first public price
        // must supersede them without waiting for another account poll.
        self.account.valuation_pending = self.account.total_equity.is_none()
            && self.account.equity.iter().any(|(currency, equity)| {
                *equity != 0.0 && self.public_currency_rate(cx, currency).is_none()
            });
        let collateral = usd(collateral);
        let used = usd(self.account.risk.values().map(|risk| risk.imr).sum());
        let upnl = usd(self.account.risk.values().map(|risk| risk.upl).sum());
        if !used.is_finite() || !upnl.is_finite() {
            cx.notice(
                Level::Warn,
                code::VALUATION_UNAVAILABLE,
                "OKX collateral risk invalid; retaining previous USD balance",
            );
            return;
        }
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
                id: None,
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
        if !self.connections[channel]
            .feed
            .is_some_and(|f| cx.socket_authenticated(f))
        {
            return;
        }
        let Some(data) = message.data else {
            return;
        };
        match message.arg.channel {
            "orders" | "orders-algo" | "algo-advance" => {
                let _ = json::each::<OrderRow<'_>>(data.get(), |row| {
                    if cx.store.market_by_id(row.inst_id).is_none() {
                        cx.notice(
                            Level::Warn,
                            code::SNAPSHOT_DISCARDED,
                            "OKX order update market unavailable; preserving previous orders",
                        );
                        return;
                    }
                    if !valid_order(&row) {
                        return;
                    }
                    // Finality protects order state, not execution delivery order.
                    self.execution(cx, &row);
                    let terminal = matches!(
                        row.state,
                        "filled" | "canceled" | "mmp_canceled" | "order_failed"
                    );
                    let triggered = row.state == "effective";
                    let native_id = if row.algo_id.is_empty() {
                        row.ord_id
                    } else {
                        row.algo_id
                    };
                    self.trading.acknowledged.remove(native_id);
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
                        } else if matches!(
                            row.state,
                            "live" | "partially_filled" | "partially_effective" | "pause"
                        ) {
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
                let _ =
                    json::each::<PositionRow<'_>>(data.get(), |row| {
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
                        } else if cx.store.market_by_id(row.inst_id).is_none()
                            && row.pos.parse::<f64>().ok() != Some(0.0)
                        {
                            cx.notice(Level::Warn, code::SNAPSHOT_DISCARDED,
                            "OKX owned position market unavailable; preserving previous positions",
                        );
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
                self.account.aggregate_held = false;
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
        let mark = finite(p.mark_px)
            .filter(|price| *price > 0.0)
            .or_else(|| {
                cx.store.ticker(&m.symbol).and_then(|ticker| {
                    [ticker.mark, ticker.last]
                        .into_iter()
                        .find(|price| price.is_finite() && *price > 0.0)
                })
            })
            .unwrap_or(0.0);
        if pos != 0.0 && mark == 0.0 {
            return None;
        }
        let reference = mark;
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
        // Same allow-list as valid_order: mmp/elp and other regular types carry an ordId.
        let algo = matches!(
            o.ord_type,
            "trigger" | "conditional" | "oco" | "move_order_stop"
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

/// An open position row on a listed market whose only missing input is a price.
fn mark_pending(cx: &Cx<'_, Pending>, p: &PositionRow<'_>) -> bool {
    let Some(m) = cx.store.market_by_id(p.inst_id) else {
        return false;
    };
    let priced = |price: f64| price.is_finite() && price > 0.0;
    p.pos
        .parse::<f64>()
        .is_ok_and(|pos| pos.is_finite() && pos != 0.0)
        && !p.mark_px.parse::<f64>().is_ok_and(priced)
        && !cx
            .store
            .ticker(&m.symbol)
            .is_some_and(|t| priced(t.mark) || priced(t.last))
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

/// OKX REST authentication codes that no retry can fix (docs-v5 "Error Code",
/// 501xx). Timestamp codes 50102/50112 and every other code stay transient.
fn rest_key_rejection(code: &str) -> Option<ErrorKind> {
    match code {
        "50100" | "50110" | "50120" => Some(ErrorKind::AuthPermission),
        "50101" | "50103" | "50104" | "50105" | "50111" | "50113" | "50114" | "50119" => {
            Some(ErrorKind::AuthInvalid)
        }
        _ => None,
    }
}

/// OKX WebSocket login codes for a missing or refused key or passphrase (60xxx).
fn socket_key_rejection(code: &str) -> Option<ErrorKind> {
    matches!(code, "60005" | "60024" | "60032").then_some(ErrorKind::AuthInvalid)
}

fn rejection(which: &str, code: &str, msg: &str, kind: ErrorKind) -> VenueError {
    let mut error = VenueError::new(format!("OKX rejected the {which} ({code}): {msg}"))
        .kind(kind)
        .venue_code(code);
    if !msg.is_empty() {
        error = error.venue_message(msg);
    }
    error
}

pub(crate) fn socket_login_error(code: &str, msg: &str) -> VenueError {
    rejection(
        "API key login",
        code,
        msg,
        socket_key_rejection(code).unwrap_or(ErrorKind::Internal),
    )
}
