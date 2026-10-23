//! Authoritative account reads and the raw CLOB user channel.
//!
//! Holdings come exclusively from Data API snapshots. Order notifications carry
//! cumulative matched size; only uniquely identified trade executions emit fills.
use crate::wire::Number;
use crate::{Pending, Polymarket, sign};
use safe_cex_engine::Level;
use safe_cex_engine::coverage::{RowReports, SnapshotBuilder};
use safe_cex_engine::fence::StaleGuard;
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::*;
use safe_cex_engine::url::encode;
use safe_cex_engine::{Cx, Fill, Method, RequestId, Response, Store, TimerId};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::collections::{HashMap, HashSet};

const REST: &str = "https://clob.polymarket.com";
const DATA: &str = "https://data-api.polymarket.com";
const INITIAL: &str = "MA==";
const END: &str = "LTE=";
const PAGE: usize = 500;
const MAX_PAGES: usize = 10000;
// The Data API documents a maximum offset of 10,000; a saturated last
// page is incomplete, never permission to replace holdings with a prefix.
const MAX_POSITION_OFFSET: usize = 10000;
const STALE_SNAPSHOT_LIMIT: u8 = 2;

pub enum Request {
    Time,
    Derive,
    Balance,
    Orders(OrderSnapshot),
    Positions(PositionSnapshot),
}
pub struct OrderSnapshot {
    revision: u64,
    cursor: String,
    cursors: HashSet<String>,
    rows: SnapshotBuilder<String, Order>,
    coverage: HashMap<String, Vec<String>>,
    ids: HashSet<String>,
    invalid: bool,
}
pub struct PositionSnapshot {
    revision: u64,
    offset: usize,
    count: usize,
    rows: SnapshotBuilder<String, Position>,
    holdings: Vec<PredictionHolding>,
    ids: HashSet<String>,
    invalid: bool,
}
#[derive(Default)]
pub(crate) struct Account {
    timer: Option<TimerId>,
    auth_retry: Option<TimerId>,
    auth_attempts: u8,
    busy: usize,
    revision: u64,
    position_revision: u64,
    positions_guard: StaleGuard,
    dirty: bool,
    versions: HashMap<String, u64>,
    terminal: HashSet<String>,
    fills: HashMap<String, f64>,
    executions: HashMap<String, HashMap<String, f64>>,
    coverage: HashMap<String, Vec<(f64, HashSet<String>)>>,
    unlisted: HashSet<String>,
    /// Assets (empty for an unidentified row) whose skipped holding was already logged.
    skip_logged: RowReports<String>,
}

#[derive(Deserialize)]
pub(crate) struct OrderRow<'a> {
    pub(crate) id: &'a str,
    #[serde(default)]
    asset_id: &'a str,
    #[serde(default)]
    side: &'a str,
    #[serde(default)]
    price: Option<Number>,
    #[serde(default)]
    original_size: Option<Number>,
    #[serde(default)]
    size_matched: Option<Number>,
    #[serde(default)]
    associate_trades: Option<Vec<String>>,
    #[serde(default)]
    status: &'a str,
    #[serde(default, rename = "type")]
    event: &'a str,
    #[serde(default)]
    order_type: &'a str,
    #[serde(default)]
    created_at: Option<Number>,
    #[serde(default)]
    timestamp: Option<Number>,
}
fn side(value: &str) -> Option<OrderSide> {
    match value {
        "BUY" => Some(OrderSide::Buy),
        "SELL" => Some(OrderSide::Sell),
        _ => None,
    }
}
fn stamp(value: f64) -> f64 {
    if value < 1e12 { value * 1000.0 } else { value }
}
impl OrderRow<'_> {
    pub(crate) fn has_fill_size(&self) -> bool {
        self.size_matched.is_some()
    }
    pub(crate) fn order(&self, store: &Store, now: f64) -> Option<Order> {
        if self.id.is_empty() {
            return None;
        }
        let old = store.order(self.id);
        let symbol = store
            .market_by_id(self.asset_id)
            .map(|m| m.symbol.as_str())
            .or_else(|| old.map(|o| o.symbol.as_str()))
            .or_else(|| (!self.asset_id.is_empty()).then_some(self.asset_id))?;
        let side = side(self.side).or_else(|| old.map(|o| o.side))?;
        let price = self.price.map(|n| n.0).or_else(|| old.map(|o| o.price))?;
        let amount = self
            .original_size
            .map(|n| n.0)
            .or_else(|| old.map(|o| o.amount))?;
        let filled = self
            .size_matched
            .map(|n| n.0)
            .or_else(|| old.map(|o| o.filled))
            .unwrap_or(0.0);
        if price <= 0.0 || price >= 1.0 || amount <= 0.0 || filled < 0.0 || filled > amount {
            return None;
        }
        let status = match (self.event, self.status) {
            ("CANCELLATION", _) | (_, "CANCELED" | "CANCELLED") => OrderStatus::Canceled,
            (_, "MATCHED") if filled >= amount => OrderStatus::Closed,
            (_, "LIVE" | "MATCHED" | "DELAYED" | "UNMATCHED" | "") => OrderStatus::Open,
            _ => return None,
        };
        let time_in_force = match self.order_type {
            "" => old
                .and_then(|o| o.time_in_force)
                .or(Some(TimeInForce::GoodTillCancel)),
            "GTC" | "GTD" => Some(TimeInForce::GoodTillCancel),
            "FOK" => Some(TimeInForce::FillOrKill),
            "FAK" => Some(TimeInForce::ImmediateOrCancel),
            _ => None,
        };
        Some(Order {
            id: self.id.into(),
            parent_id: None,
            status,
            symbol: symbol.into(),
            kind: OrderType::Limit,
            side,
            price,
            amount,
            filled,
            remaining: (amount - filled).max(0.0),
            reduce_only: false,
            close: None,
            client_order_id: None,
            time_in_force,
            last_update: self
                .timestamp
                .or(self.created_at)
                .map_or(now, |n| stamp(n.0)),
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: None,
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Holding<'a> {
    asset: &'a str,
    #[serde(default)]
    slug: &'a str,
    size: Number,
    avg_price: Number,
    #[serde(default)]
    cur_price: LenientPrice,
    #[serde(default)]
    cash_pnl: Option<Number>,
    #[serde(default)]
    realized_pnl: Option<Number>,
    #[serde(default)]
    redeemable: bool,
    #[serde(default)]
    mergeable: bool,
}
/// A quote that may be missing or undecodable; either way the holding still counts.
#[derive(Default)]
struct LenientPrice(Option<f64>);
impl<'de> Deserialize<'de> for LenientPrice {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Input {
            Price(Number),
            Other(serde::de::IgnoredAny),
        }
        Ok(Self(match Input::deserialize(d)? {
            Input::Price(n) => Some(n.0),
            Input::Other(_) => None,
        }))
    }
}
#[derive(Deserialize)]
struct Trade<'a> {
    id: &'a str,
    #[serde(default)]
    taker_order_id: &'a str,
    #[serde(default)]
    asset_id: &'a str,
    #[serde(default)]
    side: &'a str,
    #[serde(default)]
    size: Number,
    #[serde(default)]
    price: Number,
    status: &'a str,
    #[serde(default)]
    owner: &'a str,
    #[serde(default)]
    trade_owner: &'a str,
    #[serde(default)]
    trader_side: &'a str,
    #[serde(default, borrow)]
    maker_orders: Vec<Maker<'a>>,
}
#[derive(Deserialize)]
struct Maker<'a> {
    order_id: &'a str,
    #[serde(default)]
    owner: &'a str,
    #[serde(default)]
    maker_address: &'a str,
    #[serde(default)]
    asset_id: &'a str,
    #[serde(default)]
    side: &'a str,
    matched_amount: Number,
    price: Number,
}

impl Polymarket {
    /// Signing deliberately excludes URL query parameters, as the official
    /// client's L2 builder does. The body is exactly the transmitted bytes.
    pub(crate) fn signed_request(
        &self,
        cx: &mut Cx<'_, Pending>,
        method: Method,
        path: &str,
        body: Option<String>,
        pending: Pending,
    ) -> Result<RequestId, String> {
        let credentials = self
            .credentials
            .as_ref()
            .ok_or("Polymarket credentials unavailable")?;
        let address = self
            .address
            .as_deref()
            .ok_or("Polymarket signer unavailable")?;
        let seconds = (cx.now / 1000.0).floor() as u64;
        let headers = sign::l2_headers(
            credentials,
            address,
            seconds,
            method,
            path.split('?').next().unwrap_or(path),
            body.as_deref().unwrap_or(""),
        )?;
        Ok(cx.request(method, format!("{REST}{path}"), headers, body, pending))
    }
    pub(crate) fn private_subscription(&self) -> Option<String> {
        #[derive(Serialize)]
        struct Auth<'a> {
            #[serde(rename = "apiKey")]
            api_key: &'a str,
            secret: &'a str,
            passphrase: &'a str,
        }
        #[derive(Serialize)]
        struct Subscription<'a> {
            #[serde(rename = "type")]
            kind: &'static str,
            auth: Auth<'a>,
        }
        let c = self.credentials.as_ref()?;
        serde_json::to_string(&Subscription {
            kind: "user",
            auth: Auth {
                api_key: &c.api_key,
                secret: &c.secret,
                passphrase: &c.passphrase,
            },
        })
        .ok()
    }
    pub(crate) fn start_private(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.options.public_data_only {
            return;
        }
        if self.credentials.is_none()
            && let Some(options) = self.options.polymarket.as_ref()
            && let (Some(api_key), Some(secret), Some(passphrase)) = (
                options.api_key.as_ref(),
                options.api_secret.as_ref(),
                options.api_passphrase.as_ref(),
            )
            && !api_key.is_empty()
            && !secret.is_empty()
            && !passphrase.is_empty()
        {
            self.credentials = Some(sign::Credentials {
                api_key: api_key.clone(),
                secret: secret.clone(),
                passphrase: passphrase.clone(),
            });
        }
        if self.credentials.is_none()
            && self.options.polymarket.as_ref().is_some_and(|p| {
                p.api_key.is_some() || p.api_secret.is_some() || p.api_passphrase.is_some()
            })
        {
            cx.notice(
                Level::Error,
                code::CONFIG_INVALID,
                "Polymarket incomplete provided API credentials",
            );
            return;
        }
        // A wallet is required even when L2 credentials are supplied: requests
        // authenticate the signer while positions query the distinct funder.
        if self.address.is_none() {
            if self.credentials.is_some()
                || self.options.key.is_some()
                || self.options.polymarket.as_ref().is_some_and(|p| {
                    p.api_key.is_some() || p.api_secret.is_some() || p.api_passphrase.is_some()
                })
            {
                cx.notice(
                    Level::Error,
                    code::CONFIG_INVALID,
                    "Polymarket private API requires a valid signer wallet",
                );
            }
            return;
        }
        let signer = self
            .options
            .polymarket
            .as_ref()
            .map_or(0, |p| p.signer_type);
        let funder = self
            .options
            .polymarket
            .as_ref()
            .and_then(|p| p.funder_address.as_deref());
        if signer > 3
            || signer != 0 && funder.is_none()
            || funder.is_some_and(|a| safe_cex_crypto::ethereum::get_address(a).is_err())
        {
            cx.notice(
                Level::Error,
                code::CONFIG_INVALID,
                "Polymarket invalid signature type/funder",
            );
            return;
        }
        if self.credentials.is_some() {
            if self.signed_headers_valid().is_err() {
                cx.notice(
                    Level::Error,
                    code::CONFIG_INVALID,
                    "Polymarket invalid provided API credentials",
                );
                return;
            }
            self.connect(cx, 1);
            self.account_refresh(cx);
        } else if self.options.key.is_some() {
            cx.request(
                Method::Get,
                format!("{REST}/time"),
                vec![],
                None,
                Pending::Account(Request::Time),
            );
        }
    }
    fn signed_headers_valid(&self) -> Result<(), String> {
        sign::l2_headers(
            self.credentials.as_ref().ok_or("missing credentials")?,
            self.address.as_deref().ok_or("missing signer")?,
            0,
            Method::Get,
            "/data/orders",
            "",
        )
        .map(|_| ())
    }
    fn account_issue(&mut self, cx: &mut Cx<'_, Pending>, request: Request) {
        match request {
            Request::Positions(snapshot) => {
                let user = self
                    .options
                    .polymarket
                    .as_ref()
                    .and_then(|p| p.funder_address.as_deref())
                    .or(self.address.as_deref())
                    .unwrap_or("");
                let url = format!(
                    "{DATA}/positions?user={}&limit={PAGE}&offset={}&sizeThreshold=0",
                    encode(user),
                    snapshot.offset
                );
                cx.request_streamed(
                    Method::Get,
                    url,
                    vec![],
                    ArrayAt::Root,
                    Pending::Account(Request::Positions(snapshot)),
                );
            }
            Request::Orders(snapshot) => {
                let path = format!("/data/orders?next_cursor={}", encode(&snapshot.cursor));
                match self.signed_request(
                    cx,
                    Method::Get,
                    &path,
                    None,
                    Pending::Account(Request::Orders(snapshot)),
                ) {
                    Ok(id) => cx.stream_response(id, ArrayAt::Envelope(&[&[], &["data"]])),
                    Err(_) => self.account_finished(cx),
                }
            }
            Request::Balance => {
                let signer = self
                    .options
                    .polymarket
                    .as_ref()
                    .map_or(0, |p| p.signer_type);
                let path =
                    format!("/balance-allowance?asset_type=COLLATERAL&signature_type={signer}");
                if self
                    .signed_request(
                        cx,
                        Method::Get,
                        &path,
                        None,
                        Pending::Account(Request::Balance),
                    )
                    .is_err()
                {
                    self.account_finished(cx);
                }
            }
            Request::Time | Request::Derive => {}
        }
    }
    pub(crate) fn account_refresh(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.credentials.is_none() || self.account.busy != 0 {
            self.account.dirty = true;
            return;
        }
        if let Some(timer) = self.account.timer.take() {
            cx.clear_timer(timer);
        }
        self.account.busy = 3;
        self.account.dirty = false;
        self.account_issue(cx, Request::Balance);
        self.account_issue(
            cx,
            Request::Orders(OrderSnapshot {
                revision: self.account.revision,
                cursor: INITIAL.into(),
                cursors: HashSet::from([INITIAL.into()]),
                rows: SnapshotBuilder::default(),
                coverage: HashMap::new(),
                ids: HashSet::new(),
                invalid: false,
            }),
        );
        self.account_issue(
            cx,
            Request::Positions(PositionSnapshot {
                revision: self.account.position_revision,
                offset: 0,
                count: 0,
                rows: SnapshotBuilder::default(),
                holdings: vec![],
                ids: HashSet::new(),
                invalid: false,
            }),
        );
    }
    /// A user-stream reconnect loses its previous event interval. Reconcile
    /// through the same authoritative snapshots as normal private startup.
    pub(crate) fn account_reconcile(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account_refresh(cx);
    }
    fn account_finished(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account.busy = self.account.busy.saturating_sub(1);
        if self.account.busy == 0 && self.account.timer.is_none() {
            self.account.timer = Some(if self.account.dirty {
                // An event/write arrived during the snapshot: this is urgent
                // repair, independent of the periodic polling policy.
                cx.set_timer(250.0)
            } else {
                cx.set_poll_timer(10000.0)
            });
        }
    }
    pub(crate) fn account_failed(&mut self, cx: &mut Cx<'_, Pending>, request: Request) {
        cx.retried_failure(
            "polymarket.account:528",
            "Polymarket private snapshot failed; preserving state",
        );
        match request {
            Request::Time | Request::Derive => {
                if self.account.auth_attempts < 3 && self.account.auth_retry.is_none() {
                    self.account.auth_attempts += 1;
                    self.account.auth_retry =
                        Some(cx.set_timer(f64::from(self.account.auth_attempts) * 1000.0));
                }
            }
            _ => self.account_finished(cx),
        }
    }
    pub(crate) fn account_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if self.account.auth_retry == Some(timer) {
            self.account.auth_retry = None;
            if self.credentials.is_none() {
                // A derivation retry signs a newly fetched server time.
                cx.request(
                    Method::Get,
                    format!("{REST}/time"),
                    vec![],
                    None,
                    Pending::Account(Request::Time),
                );
            }
            return true;
        }
        if self.account.timer != Some(timer) {
            return false;
        }
        self.account.timer = None;
        self.account_refresh(cx);
        true
    }
    pub(crate) fn account_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        match request {
            Request::Orders(snapshot) => {
                let parsed = serde_json::from_str::<OrderRow>(item).ok();
                if let Some((row, order)) =
                    parsed.and_then(|row| row.order(cx.store, cx.now).map(|order| (row, order)))
                {
                    if !snapshot.ids.insert(order.id.clone()) {
                        snapshot.invalid = true;
                    }
                    if let Some(trades) = row.associate_trades {
                        snapshot.coverage.insert(order.id.clone(), trades);
                    }
                    snapshot.rows.push(order);
                } else {
                    #[derive(Deserialize)]
                    struct Identity<'a> {
                        #[serde(borrow)]
                        id: std::borrow::Cow<'a, str>,
                    }
                    match serde_json::from_str::<Identity>(item) {
                        Ok(Identity { id }) if !id.is_empty() => {
                            if !snapshot.ids.insert(id.to_string()) {
                                snapshot.invalid = true;
                            }
                            cx.notice(
                                Level::Warn,
                                code::ROW_SKIPPED,
                                format!("Polymarket skipped unmappable order {id}"),
                            );
                            snapshot.rows.reject(Some(id.into_owned()));
                        }
                        _ => {
                            cx.notice(
                                Level::Warn,
                                code::ROW_SKIPPED,
                                "Polymarket skipped unmappable order without an id",
                            );
                            snapshot.rows.reject(None);
                        }
                    }
                }
            }
            Request::Positions(snapshot) => {
                snapshot.count += 1;
                let Ok(row) = serde_json::from_str::<Holding>(item) else {
                    // A malformed row with a known asset keeps its stored copy; an
                    // unidentifiable one cannot prove what it would delete.
                    #[derive(Deserialize)]
                    struct Asset<'a> {
                        #[serde(borrow)]
                        asset: std::borrow::Cow<'a, str>,
                    }
                    match serde_json::from_str::<Asset>(item) {
                        Ok(Asset { asset }) if !asset.is_empty() => {
                            if !snapshot.ids.insert(asset.to_string()) {
                                snapshot.invalid = true;
                            }
                            reject_holding(cx, &mut self.account, snapshot, Some(&asset));
                        }
                        _ => reject_holding(cx, &mut self.account, snapshot, None),
                    }
                    return;
                };
                if row.asset.is_empty() {
                    reject_holding(cx, &mut self.account, snapshot, None);
                    return;
                }
                if !snapshot.ids.insert(row.asset.into()) {
                    snapshot.invalid = true;
                    return;
                }
                if row.size.0 < 0.0 || !(0.0..=1.0).contains(&row.avg_price.0) {
                    reject_holding(cx, &mut self.account, snapshot, Some(row.asset));
                    return;
                }
                if row.size.0 == 0.0 {
                    self.account.skip_logged.forget(row.asset);
                    return;
                }
                let Some(market) = cx.store.market_by_id(row.asset) else {
                    let outside_selection = !row.slug.is_empty()
                        && self
                            .options
                            .polymarket
                            .as_ref()
                            .and_then(|options| options.market_slugs.as_ref())
                            .is_some_and(|slugs| !slugs.iter().any(|slug| slug == row.slug));
                    if outside_selection {
                        return;
                    }
                    // Only a holding that may sit on a skipped catalogue row is ambiguous;
                    // otherwise its market is unlisted (e.g. closed), as in TS.
                    if self.catalogue_may_hold(row.slug) {
                        snapshot.invalid = true;
                        cx.notice(Level::Warn, code::SNAPSHOT_DISCARDED,
                            "Polymarket owned holding has unavailable market metadata; preserving previous positions");
                    } else if self.account.unlisted.insert(row.asset.into()) {
                        cx.notice(
                            Level::Warn,
                            code::ROW_SKIPPED,
                            format!(
                                "Polymarket holding {} has no loaded market; not shown",
                                row.asset
                            ),
                        );
                    }
                    return;
                };
                let valid = |p: &f64| (0.0..=1.0).contains(p);
                let price = row.cur_price.0.filter(valid).or_else(|| {
                    // BBO/book presence does not prove a default last=0 was observed.
                    cx.store
                        .ticker(&market.symbol)
                        .map(|ticker| ticker.last)
                        .filter(|price| {
                            valid(price)
                                && (*price > 0.0 || self.trade_times.contains_key(&market.symbol))
                        })
                });
                let Some(price) = price else {
                    // A seed still loading may price it; a settled one never will.
                    if !self.discovery_settled() {
                        snapshot.invalid = true;
                        cx.notice(Level::Warn, code::VALUATION_UNAVAILABLE,
                            "Polymarket owned holding price unavailable; preserving previous positions",
                        );
                        return;
                    }
                    let symbol = market.symbol.clone();
                    let level = if cx.store.position(&symbol).is_some() {
                        Level::Warn
                    } else {
                        Level::Error
                    };
                    log_skip(
                        cx,
                        &mut self.account,
                        row.asset,
                        level,
                        format!(
                            "Polymarket skipped owned holding {}: price unavailable",
                            row.asset
                        ),
                    );
                    snapshot.rows.reject(Some(symbol));
                    return;
                };
                let index = cx
                    .store
                    .market_index_by_id(row.asset)
                    .expect("known market");
                self.account.skip_logged.forget(row.asset);
                snapshot.holdings.push(PredictionHolding {
                    market: index,
                    redeemable: row.redeemable,
                    mergeable: row.mergeable,
                });
                snapshot.rows.push(Position {
                    id: market.symbol.clone(),
                    symbol: market.symbol.clone(),
                    side: PositionSide::Long,
                    entry_price: row.avg_price.0,
                    notional: row.size.0 * price,
                    leverage: 1.0,
                    margin_type: None,
                    position_mode: Some(PositionMode::Oneway),
                    unrealized_pnl: row
                        .cash_pnl
                        .map_or((price - row.avg_price.0) * row.size.0, |n| n.0),
                    realized_pnl: row.realized_pnl.map(|n| n.0),
                    contracts: row.size.0,
                    liquidation_price: 0.0,
                    margin_size: None,
                    last_update: None,
                });
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
        #[derive(Deserialize)]
        struct Error<'a> {
            #[serde(default, borrow)]
            error: Option<&'a RawValue>,
        }
        if !(200..300).contains(&response.status)
            || serde_json::from_str::<Error>(response.body)
                .ok()
                .is_some_and(|r| {
                    r.error
                        .is_some_and(|e| !matches!(e.get(), "null" | "false" | "\"\""))
                })
        {
            self.account_failed(cx, request);
            return;
        }
        match request {
            Request::Time => {
                let Ok(seconds) = serde_json::from_str::<Number>(response.body) else {
                    self.account_failed(cx, Request::Time);
                    return;
                };
                if seconds.0 < 0.0 || seconds.0.fract() != 0.0 {
                    self.account_failed(cx, Request::Time);
                    return;
                }
                let Some(key) = self.options.key.as_deref() else {
                    return;
                };
                let Some(address) = self.address.as_deref() else {
                    return;
                };
                let Ok(signature) = sign::l1_signature(key, seconds.0 as u64, 0) else {
                    return;
                };
                cx.request(
                    Method::Get,
                    format!("{REST}/auth/derive-api-key"),
                    vec![
                        ("POLY_ADDRESS", address.into()),
                        ("POLY_SIGNATURE", signature),
                        ("POLY_TIMESTAMP", seconds.0.to_string()),
                        ("POLY_NONCE", "0".into()),
                    ],
                    None,
                    Pending::Account(Request::Derive),
                );
                return;
            }
            Request::Derive => {
                if let Ok(credentials) = serde_json::from_str::<sign::Credentials>(response.body)
                    && !credentials.api_key.is_empty()
                    && !credentials.secret.is_empty()
                    && !credentials.passphrase.is_empty()
                {
                    self.credentials = Some(credentials);
                    if self.signed_headers_valid().is_err() {
                        self.credentials = None;
                        self.account_failed(cx, Request::Derive);
                        return;
                    }
                    self.connect(cx, 1);
                    self.account_refresh(cx);
                } else {
                    self.account_failed(cx, Request::Derive);
                }
                return;
            }
            Request::Balance => {
                #[derive(Deserialize)]
                struct Row {
                    balance: Number,
                }
                if let Ok(row) = serde_json::from_str::<Row>(response.body)
                    && row.balance.0 >= 0.0
                {
                    let total = row.balance.0 / 1e6;
                    cx.store.set_balance(Balance {
                        total,
                        free: total,
                        used: 0.0,
                        upnl: cx.store.positions().iter().map(|p| p.unrealized_pnl).sum(),
                    });
                }
            }
            Request::Orders(mut snapshot) => {
                #[derive(Deserialize)]
                struct Envelope<'a> {
                    data: Vec<serde::de::IgnoredAny>,
                    next_cursor: &'a str,
                }
                let cursor = if response.body.trim_start().starts_with('[') {
                    Some(END)
                } else {
                    serde_json::from_str::<Envelope>(response.body)
                        .ok()
                        .filter(|e| e.data.is_empty())
                        .map(|e| e.next_cursor)
                };
                let Some(cursor) = cursor else {
                    self.account_finished(cx);
                    return;
                };
                if snapshot.invalid {
                    self.account_finished(cx);
                    return;
                }
                if cursor != END {
                    if cursor.is_empty()
                        || snapshot.cursors.len() >= MAX_PAGES
                        || !snapshot.cursors.insert(cursor.into())
                    {
                        self.account_finished(cx);
                        return;
                    }
                    snapshot.cursor = cursor.into();
                    self.account_issue(cx, Request::Orders(snapshot));
                    return;
                }
                let mut rows: Vec<Order> = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| {
                        self.account.versions.get(&o.id).copied().unwrap_or(0) > snapshot.revision
                    })
                    .cloned()
                    .collect();
                let accepted: HashSet<String> =
                    snapshot.rows.rows().iter().map(|o| o.id.clone()).collect();
                let committed = snapshot
                    .rows
                    .commit(cx.store.orders().iter().cloned(), |o| o.id.clone());
                for mut row in committed {
                    // A rejected row's stored copy stays as it was; it proves no fill.
                    if !accepted.contains(&row.id) {
                        if !rows.iter().any(|kept| kept.id == row.id) {
                            rows.push(row);
                        }
                        continue;
                    }
                    if self.account.terminal.contains(&row.id)
                        || self.account.versions.get(&row.id).copied().unwrap_or(0)
                            > snapshot.revision
                        || row.status != OrderStatus::Open
                    {
                        continue;
                    }
                    self.account_observe_fills(
                        &row.id,
                        row.filled,
                        snapshot.coverage.remove(&row.id),
                    );
                    if let Some(old) = cx.store.order(&row.id)
                        && old.last_update > row.last_update
                    {
                        row = old.clone();
                    }
                    self.account_normalize(&mut row);
                    if row.remaining > 0.0 {
                        rows.push(row);
                    } else {
                        self.account_terminal(cx, &row.id);
                    }
                }
                let retained: HashSet<_> = rows.iter().map(|row| row.id.as_str()).collect();
                let absent: Vec<_> = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|order| !retained.contains(order.id.as_str()))
                    .map(|order| order.id.clone())
                    .collect();
                for id in absent {
                    self.account_terminal(cx, &id);
                }
                cx.store.set_orders(rows);
            }
            Request::Positions(mut snapshot) => {
                let empty = serde_json::from_str::<Vec<serde::de::IgnoredAny>>(response.body)
                    .is_ok_and(|rows| rows.is_empty());
                if !empty || snapshot.invalid {
                    self.account_finished(cx);
                    return;
                }
                if snapshot.count == PAGE {
                    if snapshot.offset >= MAX_POSITION_OFFSET {
                        self.account_finished(cx);
                        return;
                    }
                    snapshot.offset += PAGE;
                    snapshot.count = 0;
                    self.account_issue(cx, Request::Positions(snapshot));
                    return;
                }
                if snapshot.count > PAGE {
                    self.account_finished(cx);
                    return;
                }
                let current = snapshot.revision == self.account.position_revision;
                if self
                    .account
                    .positions_guard
                    .admit(current, STALE_SNAPSHOT_LIMIT)
                {
                    // The stream never carries holdings, so only a re-read corrects
                    // an admitted stale snapshot.
                    if !current {
                        self.account.dirty = true;
                    }
                    let kept: Vec<PredictionHolding> = cx
                        .store
                        .prediction_holdings()
                        .into_iter()
                        .flatten()
                        .filter(|known| {
                            !snapshot.holdings.iter().any(|h| h.market == known.market)
                                && cx
                                    .store
                                    .markets()
                                    .get(known.market as usize)
                                    .is_some_and(|m| !snapshot.rows.allows_absence(&m.symbol))
                        })
                        .copied()
                        .collect();
                    snapshot.holdings.extend(kept);
                    if snapshot.rows.rejected() == 0 {
                        self.account.skip_logged.clear();
                    }
                    let rows = snapshot
                        .rows
                        .commit(cx.store.positions().iter().cloned(), |p| p.id.clone());
                    cx.store.set_positions(rows);
                    // Committed rows include retained copies; a rejected row with none is
                    // the only unknown PnL, and it is absent from the sum.
                    if cx.store.loaded().balance {
                        let mut balance = cx.store.balance().clone();
                        balance.upnl = cx.store.positions().iter().map(|p| p.unrealized_pnl).sum();
                        cx.store.set_balance(balance);
                    }
                    cx.store.set_prediction_holdings(snapshot.holdings);
                    cx.store.set_hedged(false);
                } else {
                    self.account.dirty = true;
                }
            }
        }
        self.account_finished(cx);
    }
    fn account_normalize(&mut self, order: &mut Order) {
        let filled = self.account.fills.entry(order.id.clone()).or_default();
        *filled = filled.max(order.filled);
        order.filled = self.account_filled(&order.id);
        order.remaining = (order.amount - order.filled).max(0.0);
    }
    pub(crate) fn account_placed(&mut self, cx: &mut Cx<'_, Pending>, mut order: Order) {
        self.account_normalize(&mut order);
        if self.account.terminal.contains(&order.id) {
            return;
        }
        if order.remaining == 0.0 || order.status != OrderStatus::Open {
            self.account_terminal(cx, &order.id);
            return;
        }
        self.account.revision += 1;
        self.account
            .versions
            .insert(order.id.clone(), self.account.revision);
        cx.store.upsert_order(order);
    }
    pub(crate) fn account_terminal(&mut self, cx: &mut Cx<'_, Pending>, id: &str) {
        self.account.revision += 1;
        self.account
            .versions
            .insert(id.into(), self.account.revision);
        self.account.terminal.insert(id.into());
        cx.store.remove_orders(&[id]);
    }
    pub(crate) fn account_filled(&self, id: &str) -> f64 {
        let executions = self.account.executions.get(id);
        let sum = |covered: Option<&HashSet<String>>| {
            executions
                .into_iter()
                .flat_map(|rows| rows.iter())
                .filter(|(trade, _)| covered.is_none_or(|set| !set.contains(*trade)))
                .fold(0.0, |total, (_, amount)| {
                    safe_cex_engine::num::adjust(total + amount, 0.000001)
                })
        };
        let inferred = self
            .account
            .coverage
            .get(id)
            .into_iter()
            .flatten()
            .map(|(filled, covered)| {
                safe_cex_engine::num::adjust(filled + sum(Some(covered)), 0.000001)
            })
            .fold(0.0, f64::max);
        self.account
            .fills
            .get(id)
            .copied()
            .unwrap_or(0.0)
            .max(sum(None))
            .max(inferred)
    }
    fn account_observe_fills(&mut self, id: &str, filled: f64, trades: Option<Vec<String>>) {
        let known = self.account_filled(id).max(filled);
        self.account.fills.insert(id.into(), known);
        if let Some(trades) = trades {
            let pair = (filled, trades.into_iter().collect());
            let observations = self.account.coverage.entry(id.into()).or_default();
            // Delayed executions must still strengthen earlier cumulative observations.
            if !observations.contains(&pair) {
                observations.push(pair);
            }
        }
    }

    pub(crate) fn account_fill_proof(&self, id: &str, row: &OrderRow<'_>) -> bool {
        self.account.executions.get(id).is_none_or(|executions| {
            row.associate_trades
                .as_ref()
                .is_some_and(|trades| executions.keys().all(|id| trades.contains(id)))
        })
    }
    pub(crate) fn private_message(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        #[derive(Deserialize)]
        struct Event<'a> {
            #[serde(default)]
            event_type: &'a str,
            #[serde(default, rename = "type")]
            kind: &'a str,
            #[serde(default)]
            authenticated: bool,
        }
        let Ok(event) = serde_json::from_str::<Event>(text) else {
            return;
        };
        let kind = if event.event_type.is_empty() {
            event.kind
        } else {
            event.event_type
        };
        if kind == "authenticated" || event.authenticated {
            self.account_refresh(cx);
            return;
        }
        if kind == "order" {
            let Ok(row) = serde_json::from_str::<OrderRow>(text) else {
                return;
            };
            // A cancellation can arrive before this connection has ever seen
            // the order. Fence its ID even when the frame omits order details.
            if !row.id.is_empty()
                && (row.event == "CANCELLATION" || matches!(row.status, "CANCELED" | "CANCELLED"))
            {
                if cx.store.order(row.id).is_none_or(|old| {
                    row.timestamp
                        .is_none_or(|time| old.last_update <= stamp(time.0))
                }) {
                    let original = row
                        .original_size
                        .map(|n| n.0)
                        .or_else(|| cx.store.order(row.id).map(|order| order.amount));
                    if let Some(matched) = row
                        .size_matched
                        .filter(|n| n.0 >= 0.0 && original.is_none_or(|amount| n.0 <= amount))
                    {
                        self.account_observe_fills(row.id, matched.0, row.associate_trades);
                    }
                    self.account_terminal(cx, row.id);
                }
                return;
            }
            let Some(order) = row.order(cx.store, cx.now) else {
                self.account.dirty = true;
                return;
            };
            if self.account.terminal.contains(&order.id) {
                self.account_observe_fills(&order.id, order.filled, row.associate_trades);
                return;
            }
            if cx
                .store
                .order(&order.id)
                .is_some_and(|old| old.last_update > order.last_update)
            {
                return;
            }
            self.account_observe_fills(&order.id, order.filled, row.associate_trades);
            self.account_placed(cx, order);
        } else if kind == "trade" {
            let Ok(trade) = serde_json::from_str::<Trade>(text) else {
                return;
            };
            if trade.id.is_empty() {
                return;
            }
            self.account.position_revision += 1;
            self.account.dirty = true;
            let state = trade
                .status
                .strip_prefix("TRADE_STATUS_")
                .unwrap_or(trade.status);
            if matches!(
                state,
                "MATCHED" | "MATCHED_NOT_BROADCASTED" | "MINED" | "CONFIRMED"
            ) {
                let Some(credentials) = self.credentials.as_ref() else {
                    return;
                };
                let api = credentials.api_key.clone();
                let funder = self
                    .options
                    .polymarket
                    .as_ref()
                    .and_then(|p| p.funder_address.as_deref())
                    .or(self.address.as_deref())
                    .unwrap_or("")
                    .to_owned();
                let owns_maker = trade.maker_orders.iter().any(|maker| {
                    maker.owner == api
                        || !funder.is_empty() && maker.maker_address.eq_ignore_ascii_case(&funder)
                        || cx.store.order(maker.order_id).is_some()
                });
                let taker = trade.trader_side == "TAKER"
                    || trade.trader_side != "MAKER"
                        && (cx.store.order(trade.taker_order_id).is_some()
                            || !owns_maker && (trade.owner == api || trade.trade_owner == api));
                if taker && let Some(side) = side(trade.side) {
                    self.account_fill(
                        cx,
                        trade.id,
                        trade.taker_order_id,
                        trade.asset_id,
                        side,
                        (trade.price.0, trade.size.0),
                    );
                }
                for maker in trade.maker_orders {
                    if maker.owner != api
                        && (funder.is_empty() || !maker.maker_address.eq_ignore_ascii_case(&funder))
                        && cx.store.order(maker.order_id).is_none()
                    {
                        continue;
                    }
                    let asset = if maker.asset_id.is_empty() {
                        trade.asset_id
                    } else {
                        maker.asset_id
                    };
                    let maker_side = side(maker.side)
                        .or_else(|| cx.store.order(maker.order_id).map(|o| o.side))
                        .or_else(|| {
                            (asset == trade.asset_id)
                                .then(|| side(trade.side).map(OrderSide::opposite))
                                .flatten()
                        });
                    if let Some(side) = maker_side {
                        self.account_fill(
                            cx,
                            trade.id,
                            maker.order_id,
                            asset,
                            side,
                            (maker.price.0, maker.matched_amount.0),
                        );
                    }
                }
            }
            if self.account.busy == 0 {
                self.account_refresh(cx);
            }
        }
    }
    fn account_fill(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        trade: &str,
        order: &str,
        asset: &str,
        side: OrderSide,
        execution: (f64, f64),
    ) {
        let (price, amount) = execution;
        if order.is_empty() || amount <= 0.0 || !(0.0..=1.0).contains(&price) {
            return;
        }
        let Some(market) = cx.store.market_by_id(asset) else {
            return;
        };
        let executions = self.account.executions.entry(order.into()).or_default();
        if !executions.contains_key(trade) {
            executions.insert(trade.into(), amount);
            cx.fill(Fill {
                id: None,
                order_id: order.into(),
                symbol: market.symbol.clone(),
                side,
                price,
                amount,
            });
            if let Some(live) = cx.store.order(order).cloned() {
                // A trade can precede its cumulative order notification. Bump
                // the version too, so an in-flight snapshot cannot erase it.
                self.account_placed(cx, live);
            }
        }
    }
}

/// Rejects a holding row; a known market keys its stored copy, while an unknown
/// one has no stored copy to keep. A row with nothing stored to keep is an error.
fn reject_holding(
    cx: &mut Cx<'_, Pending>,
    account: &mut Account,
    snapshot: &mut PositionSnapshot,
    asset: Option<&str>,
) {
    let level = |stored: bool| {
        if stored { Level::Warn } else { Level::Error }
    };
    let Some(asset) = asset else {
        let level = level(!cx.store.positions().is_empty());
        log_skip(
            cx,
            account,
            "",
            level,
            "Polymarket skipped malformed holding without an asset",
        );
        snapshot.rows.reject(None);
        return;
    };
    let symbol = cx.store.market_by_id(asset).map(|m| m.symbol.clone());
    let stored = symbol
        .as_ref()
        .is_some_and(|symbol| cx.store.position(symbol).is_some());
    log_skip(
        cx,
        account,
        asset,
        level(stored),
        format!("Polymarket skipped malformed holding {asset}"),
    );
    snapshot
        .rows
        .reject(Some(symbol.unwrap_or_else(|| asset.to_owned())));
}
/// Logs a skipped holding once per asset until it maps again; polls would otherwise repeat it.
fn log_skip(
    cx: &mut Cx<'_, Pending>,
    account: &mut Account,
    asset: &str,
    level: Level,
    message: impl Into<String>,
) {
    account
        .skip_logged
        .report(cx, asset.to_owned(), level, message);
}

#[cfg(test)]
mod execution_bounds {
    use super::*;
    use crate::{Options, PolymarketOptions};
    use safe_cex_engine::{Engine, Event, SocketId};

    fn engine() -> Engine<Polymarket> {
        let mut venue = Polymarket::new(Options {
            key: Some("0000000000000000000000000000000000000000000000000000000000000001".into()),
            polymarket: Some(PolymarketOptions::default()),
            ..Default::default()
        });
        venue.credentials = Some(sign::Credentials {
            api_key: "ours".into(),
            secret: "c2VjcmV0".into(),
            passphrase: "pass".into(),
        });
        venue.connections[1].socket = Some(SocketId(77));
        venue.connections[1].open = true;
        let mut engine = safe_cex_engine::test_support::seeded_engine(venue);
        engine.store_mut().set_markets(vec![Market {
            id: "1".into(),
            symbol: "event-Yes".into(),
            pretty_symbol: None,
            base: "event".into(),
            quote: "pUSD".into(),
            active: true,
            precision: Precision {
                amount: 2.0,
                price: 2.0,
            },
            limits: Limits {
                amount: MinMax { min: 5.0, max: 1e9 },
                leverage: MinMax { min: 1.0, max: 1.0 },
                price: None,
            },
            contract_size: Some(1.0),
            is_expiring: None,
            is_inverse: None,
            is_spot: Some(true),
            is_perpetual: None,
            is_read_only: None,
            taker_fee: None,
            maker_fee: None,
        }]);
        engine
    }
    fn send(engine: &mut Engine<Polymarket>, text: &str) {
        engine.handle(
            1700000000000.0,
            Event::WsMessage {
                socket: SocketId(77),
                text,
            },
        );
    }
    fn trade(engine: &mut Engine<Polymarket>, id: &str, amount: &str, status: &str) {
        send(
            engine,
            &format!(
                r#"{{"event_type":"trade","id":"{id}","taker_order_id":"o1","asset_id":"1","side":"BUY","size":"{amount}","price":"0.5","status":"{status}","trader_side":"TAKER"}}"#
            ),
        );
    }
    fn cumulative(engine: &mut Engine<Polymarket>, filled: &str) {
        send(
            engine,
            &format!(
                r#"{{"event_type":"order","id":"o1","asset_id":"1","side":"BUY","price":"0.5","original_size":"10","size_matched":"{filled}","status":"LIVE","timestamp":"1700000000000"}}"#
            ),
        );
    }

    #[test]
    fn unique_trades_advance_a_bound_without_cumulative_order_notifications() {
        let mut engine = engine();
        for status in ["MATCHED", "MINED", "CONFIRMED"] {
            trade(&mut engine, "t1", "3", status);
        }
        assert_eq!(engine.venue().account_filled("o1"), 3.0);
        trade(&mut engine, "t2", "2", "MATCHED");
        assert_eq!(engine.venue().account_filled("o1"), 5.0);
    }
    #[test]
    fn cumulative_and_execution_observations_overlap_without_double_counting() {
        let mut engine = engine();
        cumulative(&mut engine, "3");
        trade(&mut engine, "t1", "3", "MATCHED");
        assert_eq!(engine.venue().account_filled("o1"), 3.0);
        cumulative(&mut engine, "5");
        trade(&mut engine, "t2", "2", "MATCHED");
        assert_eq!(engine.venue().account_filled("o1"), 5.0);
        cumulative(&mut engine, "4");
        assert_eq!(engine.venue().account_filled("o1"), 5.0);
    }
    #[test]
    fn terminal_removal_retains_the_bound_and_accepts_later_trade_evidence() {
        let mut engine = engine();
        cumulative(&mut engine, "2");
        send(
            &mut engine,
            r#"{"event_type":"order","type":"CANCELLATION","id":"o1","size_matched":"3","timestamp":"1700000001000"}"#,
        );
        assert!(engine.store().orders().is_empty());
        assert_eq!(engine.venue().account_filled("o1"), 3.0);
        trade(&mut engine, "t1", "4", "MATCHED");
        assert_eq!(engine.venue().account_filled("o1"), 4.0);
        assert!(engine.store().orders().is_empty());
    }
    #[test]
    fn execution_addition_preserves_six_decimal_token_units() {
        let mut engine = engine();
        trade(&mut engine, "t1", "0.1", "MATCHED");
        trade(&mut engine, "t2", "0.2", "MATCHED");
        assert_eq!(engine.venue().account_filled("o1"), 0.3);
    }
    #[test]
    fn paired_cumulative_coverage_distinguishes_new_executions() {
        let mut engine = engine();
        send(
            &mut engine,
            r#"{"event_type":"order","id":"o1","asset_id":"1","side":"BUY","price":"0.5","original_size":"10","size_matched":"2","associate_trades":["prior"],"status":"LIVE","timestamp":"1700000000000"}"#,
        );
        trade(&mut engine, "new", "1", "MATCHED");
        assert_eq!(engine.venue().account_filled("o1"), 3.0);
        assert_eq!(engine.store().order("o1").unwrap().filled, 3.0);
        send(
            &mut engine,
            r#"{"event_type":"order","id":"o1","asset_id":"1","side":"BUY","price":"0.5","original_size":"10","size_matched":"3","associate_trades":["prior","new"],"status":"LIVE","timestamp":"1700000001000"}"#,
        );
        trade(&mut engine, "new", "1", "CONFIRMED");
        assert_eq!(engine.venue().account_filled("o1"), 3.0);
    }
    #[test]
    fn coverage_keeps_each_amount_with_its_own_trade_ids_and_retains_proven_bounds() {
        let mut engine = engine();
        send(
            &mut engine,
            r#"{"event_type":"order","id":"o1","asset_id":"1","side":"BUY","price":"0.5","original_size":"10","size_matched":"2","associate_trades":["prior"],"status":"LIVE","timestamp":"1700000000000"}"#,
        );
        trade(&mut engine, "new", "1", "MATCHED");
        send(
            &mut engine,
            r#"{"event_type":"order","id":"o1","asset_id":"1","side":"BUY","price":"0.5","original_size":"10","size_matched":"2","associate_trades":["prior","new"],"status":"LIVE","timestamp":"1700000001000"}"#,
        );
        assert_eq!(engine.venue().account_filled("o1"), 3.0);
        send(
            &mut engine,
            r#"{"event_type":"order","id":"o1","asset_id":"1","side":"BUY","price":"0.5","original_size":"10","size_matched":"4","associate_trades":["prior","new","later"],"status":"LIVE","timestamp":"1700000002000"}"#,
        );
        trade(&mut engine, "latest", "1", "MATCHED");
        assert_eq!(engine.venue().account_filled("o1"), 5.0);
    }
    #[test]
    fn delayed_execution_still_strengthens_an_earlier_coverage_pair() {
        let mut engine = engine();
        send(
            &mut engine,
            r#"{"event_type":"order","id":"o1","asset_id":"1","side":"BUY","price":"0.5","original_size":"10","size_matched":"2","associate_trades":["prior"],"status":"LIVE","timestamp":"1700000000000"}"#,
        );
        send(
            &mut engine,
            r#"{"event_type":"order","id":"o1","asset_id":"1","side":"BUY","price":"0.5","original_size":"10","size_matched":"2","associate_trades":["prior","new"],"status":"LIVE","timestamp":"1700000001000"}"#,
        );
        trade(&mut engine, "new", "1", "MATCHED");
        assert_eq!(engine.venue().account_filled("o1"), 3.0);
    }
}
