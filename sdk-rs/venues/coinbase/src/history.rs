//! Advanced Trade history reads; their JSON results never enter account state.
use crate::{Coinbase, Pending, ProductKind, account, account_wire::Decimal, public::ws_symbol};
use safe_cex_engine::history::{Completion, CursorHistory};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::{OrderStatus, OrderType};
use safe_cex_engine::paging::{CursorPage, CursorStep, CursorWalk};
use safe_cex_engine::url::encode;
use safe_cex_engine::{CommandId, Cx, ErrorKind, Method, Outcome, Query, Response, VenueError};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet, VecDeque};
const MAX_OUTPUT: usize = 1000;
/// Pages per product family before a pending cursor records incomplete coverage.
const MAX_PAGES: u32 = 50;
const MAX_TIME: f64 = 253402300799999.0;
#[derive(Default)]
pub struct History {
    active: HashSet<CommandId>,
}
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Fills,
    Orders,
    Trades,
}
pub struct Request {
    command: CommandId,
    kind: Kind,
    symbol: Option<String>,
    native: Option<String>,
    order_id: Option<String>,
    from: Option<f64>,
    to: Option<f64>,
    limit: usize,
    page_items: usize,
    owner: Box<CursorHistory<Row>>,
    families: VecDeque<ProductKind>,
    family: ProductKind,
    family_start: usize,
    rows: Vec<Row>,
    ids: HashMap<String, String>,
    invalid: Option<String>,
}
#[derive(Serialize)]
#[serde(untagged)]
enum Row {
    Fill(Execution),
    Order(HistoricalOrder),
    Trade(Trade),
}
impl Row {
    fn timestamp(&self) -> f64 {
        match self {
            Self::Fill(r) => r.timestamp * 1000.0,
            Self::Order(r) => r.created_time,
            Self::Trade(r) => r.timestamp,
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Execution {
    id: String,
    timestamp: f64,
    symbol: String,
    order_id: String,
    price: f64,
    qty: f64,
    side: &'static str,
    commission: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    maker: Option<bool>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HistoricalOrder {
    id: String,
    symbol: String,
    side: &'static str,
    #[serde(rename = "type")]
    kind: OrderType,
    status: OrderStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    average_price: Option<f64>,
    amount: f64,
    filled: f64,
    reduce_only: bool,
    created_time: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_time: Option<f64>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Trade {
    id: String,
    symbol: String,
    price: f64,
    qty: f64,
    quote_qty: f64,
    timestamp: f64,
    side: &'static str,
}
#[derive(Deserialize)]
struct FillRow<'a> {
    #[serde(borrow)]
    trade_id: Cow<'a, str>,
    #[serde(borrow)]
    order_id: Cow<'a, str>,
    #[serde(borrow)]
    trade_time: Cow<'a, str>,
    #[serde(borrow)]
    trade_type: Cow<'a, str>,
    price: Decimal,
    size: Decimal,
    commission: Decimal,
    #[serde(borrow)]
    product_id: Cow<'a, str>,
    #[serde(borrow)]
    side: Cow<'a, str>,
    size_in_quote: bool,
    #[serde(default, borrow)]
    liquidity_indicator: Cow<'a, str>,
}
#[derive(Deserialize)]
struct OrderTime<'a> {
    #[serde(borrow)]
    created_time: Cow<'a, str>,
    #[serde(default, borrow)]
    last_fill_time: Cow<'a, str>,
    #[serde(
        default,
        deserialize_with = "safe_cex_engine::de::optional_number_or_blank"
    )]
    average_filled_price: Option<f64>,
}
#[derive(Deserialize)]
struct TradeRow<'a> {
    #[serde(borrow)]
    trade_id: Cow<'a, str>,
    #[serde(borrow)]
    product_id: Cow<'a, str>,
    price: Decimal,
    size: Decimal,
    #[serde(borrow)]
    time: Cow<'a, str>,
    #[serde(borrow)]
    side: Cow<'a, str>,
}
fn id(s: &str) -> Result<(), String> {
    if s.is_empty() || s.len() > 200 || s.chars().any(char::is_control) {
        Err("Coinbase invalid history identity".into())
    } else {
        Ok(())
    }
}
fn side(s: &str) -> Result<&'static str, String> {
    Ok(match account::order_side(s)? {
        safe_cex_engine::model::OrderSide::Buy => "buy",
        safe_cex_engine::model::OrderSide::Sell => "sell",
    })
}
fn checked_time(n: f64) -> Result<f64, String> {
    if n.is_finite() && (0.0..=MAX_TIME).contains(&n) && n.fract() == 0.0 {
        Ok(n)
    } else {
        Err("Coinbase history time must be an integer UTC millisecond in years1970..9999".into())
    }
}
fn rfc3339(ms: f64) -> Result<String, String> {
    Ok(safe_cex_engine::time::iso_millis(checked_time(ms)? as i64))
}
fn timestamp(text: &str) -> Result<f64, String> {
    if !text.is_ascii() || text.len() < 20 {
        return Err("Coinbase invalid history timestamp".into());
    }
    let (zone, offset) = if let Some(base) = text.strip_suffix('Z') {
        (base, 0i64)
    } else if text.len() >= 25 {
        let at = text.len() - 6;
        let suffix = &text[at..];
        if !matches!(suffix.as_bytes()[0], b'+' | b'-') || suffix.as_bytes()[3] != b':' {
            return Err("Coinbase invalid UTC offset".into());
        }
        if !suffix.as_bytes()[1..3].iter().all(u8::is_ascii_digit)
            || !suffix.as_bytes()[4..6].iter().all(u8::is_ascii_digit)
        {
            return Err("Coinbase invalid UTC offset".into());
        }
        let h = suffix[1..3]
            .parse::<i64>()
            .map_err(|_| "Coinbase invalid UTC offset")?;
        let m = suffix[4..6]
            .parse::<i64>()
            .map_err(|_| "Coinbase invalid UTC offset")?;
        if h > 23 || m > 59 {
            return Err("Coinbase invalid UTC offset".into());
        }
        (
            &text[..at],
            (h * 3600 + m * 60) * if suffix.starts_with('+') { 1 } else { -1 },
        )
    } else {
        return Err("Coinbase history timestamp has no timezone".into());
    };
    if zone.len() < 19
        || [0..4, 5..7, 8..10, 11..13, 14..16]
            .into_iter()
            .any(|r| !zone.as_bytes()[r].iter().all(u8::is_ascii_digit))
        || !zone.as_bytes()[17..19].iter().all(u8::is_ascii_digit)
        || (!zone[19..].is_empty()
            && !(zone.as_bytes()[19] == b'.'
                && zone.len() > 20
                && zone.as_bytes()[20..].iter().all(u8::is_ascii_digit)))
    {
        return Err("Coinbase invalid timestamp seconds".into());
    }
    let value = crate::public::timestamp(&format!("{zone}Z"))
        .ok_or("Coinbase invalid history timestamp")?
        - (offset as f64);
    if !value.is_finite() || value < 0.0 || value * 1000.0 > MAX_TIME {
        return Err("Coinbase history timestamp out of range".into());
    }
    Ok(value * 1000.0)
}
impl Coinbase {
    pub(crate) fn history_start(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        query: Query,
    ) {
        let result: Result<Request, String> = (|| {
            if self.history.active.len() >= 16 || self.history.active.contains(&command) {
                return Err("Coinbase history request limit".into());
            }
            let(kind,symbol,order_id,from,to,limit)=match query{
                Query::Fills{symbol,order_id,from_ms,to_ms,limit}=>(Kind::Fills,symbol,order_id,from_ms,to_ms,limit),
                Query::OrderHistory{symbol,from_ms,to_ms,limit}=>(Kind::Orders,symbol,None,from_ms,to_ms,limit),
                Query::PublicTrades{symbol,limit}=>(Kind::Trades,Some(symbol),None,None,None,limit),
                Query::Income{..}=>return Err("Coinbase Advanced Trade does not publish an income ledger API for this account path".into()),
                Query::OrderByClientId{..}=>return Err("Coinbase has no exact client-order-ID lookup API; a bounded history scan cannot prove placement absence".into()),
            };
            if kind != Kind::Trades && self.credentials.is_none() {
                return Err("Coinbase private history requires credentials".into());
            }
            let limit = limit.unwrap_or(100) as usize;
            if limit == 0 || limit > MAX_OUTPUT {
                return Err("Coinbase history limit must be1..1000".into());
            }
            let from = from.map(checked_time).transpose()?;
            let to = to.map(checked_time).transpose()?;
            if from.zip(to).is_some_and(|(a, b)| a > b) {
                return Err("Coinbase reversed history range".into());
            }
            if let Some(order) = &order_id {
                id(order)?;
            }
            let native = if let Some(s) = &symbol {
                let m = cx
                    .store
                    .market(s)
                    .ok_or("Coinbase history market missing")?;
                id(&m.id)?;
                Some(ws_symbol(&m.id))
            } else {
                None
            };
            let mut families = VecDeque::new();
            let family = if let Some(s) = &symbol {
                let m = cx.store.market(s).unwrap();
                if m.is_spot == Some(true) {
                    ProductKind::Spot
                } else if m.is_expiring == Some(true) {
                    ProductKind::Cfm
                } else {
                    ProductKind::Intx
                }
            } else {
                match self.account.kind.unwrap_or(ProductKind::Spot) {
                    ProductKind::Cfm => {
                        families.push_back(ProductKind::Spot);
                        ProductKind::Cfm
                    }
                    v => v,
                }
            };
            Ok(Request {
                command,
                kind,
                symbol,
                native,
                order_id,
                from,
                to,
                limit,
                page_items: 0,
                owner: Box::new(CursorHistory::new(
                    Completion::new(command),
                    CursorWalk::new(MAX_PAGES),
                    |_| "Coinbase history cursor repeated or page limit exceeded".into(),
                    |rows| serde_json::to_string(rows).map_err(|e| VenueError::new(e.to_string())),
                )),
                families,
                family,
                family_start: 0,
                rows: vec![],
                ids: HashMap::new(),
                invalid: None,
            })
        })();
        match result {
            Ok(req) => {
                self.history.active.insert(command);
                self.history_read(cx, req, None)
            }
            Err(e) => crate::local_context::complete(
                cx,
                safe_cex_engine::request_context::Operation::Read,
                Err(e.into()),
            ),
        }
    }
    fn history_read(&mut self, cx: &mut Cx<'_, Pending>, req: Request, cursor: Option<&str>) {
        let command = req.command;
        let result: Result<(String, ArrayAt), String> = (|| {
            let mut path = match req.kind {
                Kind::Fills => format!(
                    "/api/v3/brokerage/orders/historical/fills?limit={}&sort_by=TRADE_TIME",
                    req.limit.min(100)
                ),
                Kind::Orders => format!(
                    "/api/v3/brokerage/orders/historical/batch?limit={}&order_status=FILLED&order_status=CANCELLED&order_status=EXPIRED&order_status=FAILED",
                    req.limit.min(100)
                ),
                Kind::Trades => format!(
                    "/api/v3/brokerage/market/products/{}/ticker?limit={}",
                    encode(req.native.as_deref().unwrap()),
                    req.limit
                ),
            };
            if req.kind != Kind::Trades {
                if let Some(product) = &req.native {
                    path.push_str("&product_ids=");
                    path.push_str(&encode(product));
                }
                if let Some(order) = &req.order_id {
                    path.push_str("&order_ids=");
                    path.push_str(&encode(order));
                }
                let product = if req.family == ProductKind::Spot {
                    "SPOT"
                } else {
                    "FUTURE"
                };
                path.push_str(if req.kind == Kind::Fills {
                    "&product_types="
                } else {
                    "&product_type="
                });
                path.push_str(product);
                if req.kind == Kind::Orders && req.family != ProductKind::Spot {
                    path.push_str("&contract_expiry_type=");
                    path.push_str(if req.family == ProductKind::Intx {
                        "PERPETUAL"
                    } else {
                        "EXPIRING"
                    });
                }
                if let Some(from) = req.from {
                    path.push_str(if req.kind == Kind::Fills {
                        "&start_sequence_timestamp="
                    } else {
                        "&start_date="
                    });
                    let from = if req.kind == Kind::Fills {
                        (from - 1.0).max(0.0)
                    } else {
                        from
                    };
                    path.push_str(&encode(&rfc3339(from)?));
                }
                if let Some(to) = req.to {
                    path.push_str(if req.kind == Kind::Fills {
                        "&end_sequence_timestamp="
                    } else {
                        "&end_date="
                    });
                    path.push_str(&encode(&rfc3339((to + 1.0).min(MAX_TIME))?));
                }
                if let Some(cursor) = cursor {
                    path.push_str("&cursor=");
                    path.push_str(&encode(cursor));
                }
            }
            Ok((
                path,
                ArrayAt::Field(match req.kind {
                    Kind::Fills => "fills",
                    Kind::Orders => "orders",
                    Kind::Trades => "trades",
                }),
            ))
        })();
        match result {
            Ok((path, at)) => {
                if let Err(e) =
                    self.request(cx, Method::Get, path, None, Some(at), Pending::History(req))
                {
                    self.history.active.remove(&command);
                    cx.fail(
                        command,
                        VenueError::new(e)
                            .kind(ErrorKind::Internal)
                            .outcome(Outcome::NotSent),
                    );
                }
            }
            Err(e) => {
                self.history.active.remove(&command);
                req.owner.reject(
                    cx,
                    VenueError::new(e)
                        .kind(ErrorKind::Internal)
                        .outcome(Outcome::NotSent),
                );
            }
        }
    }
    pub(crate) fn history_item(&mut self, cx: &mut Cx<'_, Pending>, req: &mut Request, text: &str) {
        if req.invalid.is_some() {
            return;
        }
        let result: Result<(), String> = (|| {
            req.page_items += 1;
            if req.page_items > 1000 {
                return Err("Coinbase history page exceeds limit".into());
            }
            let row = match req.kind {
                Kind::Fills => {
                    let r: FillRow<'_> = serde_json::from_str(text).map_err(|e| e.to_string())?;
                    id(&r.trade_id)?;
                    id(&r.order_id)?;
                    id(&r.product_id)?;
                    if r.trade_type != "FILL" {
                        return Err("Coinbase unsupported fill correction type".into());
                    }
                    if req
                        .order_id
                        .as_ref()
                        .is_some_and(|id| id != r.order_id.as_ref())
                    {
                        return Err("Coinbase fill order identity mismatch".into());
                    }
                    if req
                        .native
                        .as_ref()
                        .is_some_and(|id| id != &ws_symbol(&r.product_id))
                    {
                        return Err("Coinbase fill product mismatch".into());
                    }
                    let time = timestamp(&r.trade_time)?;
                    let symbol = history_symbol(cx, &r.product_id, req.symbol.as_deref());
                    let scale = cx
                        .store
                        .market(&symbol)
                        .filter(|m| m.is_expiring == Some(true))
                        .and_then(|m| m.contract_size)
                        .unwrap_or(1.0);
                    let price = r.price.0;
                    if r.size_in_quote
                        && req.family == ProductKind::Cfm
                        && cx.store.market(&symbol).is_none()
                    {
                        return Err(
                            "Coinbase historical quote-sized CFM fill lacks contract proof".into(),
                        );
                    }
                    let qty = if r.size_in_quote {
                        r.size.0 / price / scale
                    } else {
                        r.size.0
                    };
                    if price <= 0.0 || qty <= 0.0 || !qty.is_finite() {
                        return Err("Coinbase invalid fill economics".into());
                    }
                    let maker = match r.liquidity_indicator.as_ref() {
                        "MAKER" => Some(true),
                        "TAKER" => Some(false),
                        "UNKNOWN_LIQUIDITY_INDICATOR" | "" => None,
                        _ => return Err("Coinbase unknown fill liquidity".into()),
                    };
                    Row::Fill(Execution {
                        id: r.trade_id.into_owned(),
                        timestamp: time / 1000.0,
                        symbol,
                        order_id: r.order_id.into_owned(),
                        price,
                        qty,
                        side: side(&r.side)?,
                        commission: r.commission.0,
                        maker,
                    })
                }
                Kind::Orders => {
                    let (o, _) = account::map_order(cx.store, text, 0.0)?;
                    if o.status == OrderStatus::Open {
                        return Ok(());
                    }
                    let time: OrderTime<'_> =
                        serde_json::from_str(text).map_err(|e| e.to_string())?;
                    let created = timestamp(&time.created_time)?;
                    let updated = if time.last_fill_time.is_empty() {
                        None
                    } else {
                        Some(timestamp(&time.last_fill_time)?)
                    };
                    if updated.is_some_and(|t| t < created) {
                        return Err("Coinbase historical order timestamp precedes creation".into());
                    }
                    let m = cx
                        .store
                        .market(&o.symbol)
                        .ok_or("Coinbase historical order market missing")?;
                    if req
                        .native
                        .as_ref()
                        .is_some_and(|id| id != &ws_symbol(&m.id))
                    {
                        return Err("Coinbase historical order product mismatch".into());
                    }
                    let scale = if m.is_expiring == Some(true) {
                        m.contract_size
                            .filter(|n| n.is_finite() && *n > 0.0)
                            .ok_or("Coinbase historical CFM contract size missing")?
                    } else {
                        1.0
                    };
                    Row::Order(HistoricalOrder {
                        id: o.id,
                        symbol: o.symbol,
                        side: if o.side == safe_cex_engine::model::OrderSide::Buy {
                            "buy"
                        } else {
                            "sell"
                        },
                        kind: o.kind,
                        status: o.status,
                        price: (o.price > 0.0).then_some(o.price),
                        average_price: time.average_filled_price.filter(|n| *n > 0.0),
                        amount: o.amount / scale,
                        filled: o.filled / scale,
                        reduce_only: o.reduce_only,
                        created_time: created,
                        updated_time: updated,
                    })
                }
                Kind::Trades => {
                    let r: TradeRow<'_> = serde_json::from_str(text).map_err(|e| e.to_string())?;
                    id(&r.trade_id)?;
                    if req.native.as_deref() != Some(ws_symbol(&r.product_id).as_str()) {
                        return Err("Coinbase public trade product mismatch".into());
                    }
                    let price = r.price.0;
                    let qty = r.size.0;
                    let symbol = req.symbol.clone().unwrap();
                    let scale = cx
                        .store
                        .market(&symbol)
                        .filter(|m| m.is_expiring == Some(true))
                        .and_then(|m| m.contract_size)
                        .unwrap_or(1.0);
                    let value = price * qty * scale;
                    if price <= 0.0 || qty <= 0.0 || !value.is_finite() {
                        return Err("Coinbase invalid public trade economics".into());
                    }
                    Row::Trade(Trade {
                        id: r.trade_id.into_owned(),
                        symbol,
                        price,
                        qty,
                        quote_qty: value,
                        timestamp: timestamp(&r.time)?,
                        // Advanced Trade public ticks report the maker's side.
                        side: if side(&r.side)? == "buy" {
                            "sell"
                        } else {
                            "buy"
                        },
                    })
                }
            };
            let key = match &row {
                Row::Fill(r) => format!("{}:{}", r.symbol, r.id),
                Row::Order(r) => r.id.clone(),
                Row::Trade(r) => r.id.clone(),
            };
            let fingerprint = serde_json::to_string(&row).map_err(|e| e.to_string())?;
            if let Some(previous) = req.ids.get(&key) {
                if previous != &fingerprint {
                    return Err("Coinbase conflicting duplicate history identity".into());
                }
                return Ok(());
            }
            req.ids.insert(key, fingerprint);
            let time = row.timestamp();
            if req.from.is_some_and(|from| time < from) || req.to.is_some_and(|to| time > to) {
                return Ok(());
            }
            req.rows.push(row);
            if req.owner.accepted_len() + req.rows.len() > MAX_OUTPUT * 2 {
                return Err("Coinbase history result exceeds limit".into());
            }
            Ok(())
        })();
        if let Err(e) = result {
            req.invalid = Some(e)
        }
    }
    pub(crate) fn history_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut req: Request,
        response: Response<'_>,
    ) {
        if !(200..300).contains(&response.status) {
            self.history_failed(
                cx,
                req,
                &format!("Coinbase history HTTP {}", response.status),
            );
            return;
        }
        if let Some(error) = req.invalid.take() {
            self.history_failed(cx, req, &error);
            return;
        }
        #[derive(Deserialize)]
        struct Page {
            #[serde(default)]
            cursor: String,
            #[serde(default)]
            has_next: bool,
            #[serde(default)]
            proof_token_required: bool,
        }
        let page = match serde_json::from_str::<Page>(response.body) {
            Ok(p) => p,
            Err(e) => {
                // Native continuation/proof metadata is a policy boundary;
                // a wrong type cannot be followed, even after admitted rows.
                self.history_rejected(cx, req, &e.to_string());
                return;
            }
        };
        if page.proof_token_required {
            self.history_rejected(
                cx,
                req,
                "Coinbase history requires an additional account proof token",
            );
            return;
        }
        let more = req.kind != Kind::Trades
            && if req.kind == Kind::Orders {
                page.has_next
            } else {
                req.page_items > 0 && !page.cursor.is_empty()
            };
        let want_more = req.owner.accepted_len() - req.family_start + req.rows.len() < req.limit;
        // Orders announce more pages with has_next; that flag without a usable
        // cursor cannot be followed.
        if more && want_more && (page.cursor.is_empty() || page.cursor.len() > 2048) {
            self.history_rejected(
                cx,
                req,
                "Coinbase history cursor repeated or page limit exceeded",
            );
            return;
        }
        // The entire response passed native validation. Only now may its
        // mapped rows affect shared identity membership and accepted coverage.
        let fresh = req.rows.len();
        for row in req.rows.drain(..) {
            let key = match &row {
                Row::Fill(r) => format!("{}:{}", r.symbol, r.id),
                Row::Order(r) => r.id.clone(),
                Row::Trade(r) => r.id.clone(),
            };
            if req.owner.fresh(key) {
                req.owner.stage(row);
            }
        }
        let step = req.owner.advance(CursorPage {
            next: more.then_some(page.cursor),
            rows: req.page_items,
            fresh,
            want_more,
        });
        match step {
            CursorStep::More(cursor) => {
                req.page_items = 0;
                self.history_read(cx, req, Some(&cursor));
                return;
            }
            CursorStep::Stalled(_) => {
                self.history_finish(cx, req, Ok(()));
                return;
            }
            CursorStep::Done => {}
        }
        if let Some(next) = req.families.pop_front() {
            req.family = next;
            req.page_items = 0;
            req.family_start = req.owner.accepted_len();
            if !req.owner.restart_stage() {
                self.history_rejected(cx, req, "Coinbase history stage cannot restart");
                return;
            }
            self.history_read(cx, req, None);
            return;
        }
        self.history_finish(cx, req, Ok(()));
    }
    fn history_finish(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        req: Request,
        result: Result<(), VenueError>,
    ) {
        self.history.active.remove(&req.command);
        req.owner.finish_with(cx, result, |mut rows| {
            rows.sort_by(|a, b| b.timestamp().total_cmp(&a.timestamp()));
            rows.truncate(req.limit);
            serde_json::to_string(&rows).map_err(|e| VenueError::new(e.to_string()))
        });
    }
    pub(crate) fn history_failed(&mut self, cx: &mut Cx<'_, Pending>, req: Request, error: &str) {
        self.history_finish(cx, req, Err(VenueError::new(error)));
    }
    fn history_rejected(&mut self, cx: &mut Cx<'_, Pending>, req: Request, error: &str) {
        self.history.active.remove(&req.command);
        req.owner.reject(cx, VenueError::new(error));
    }
    pub(crate) fn history_unsent(&mut self, cx: &mut Cx<'_, Pending>, req: Request, error: &str) {
        self.history.active.remove(&req.command);
        req.owner.reject(
            cx,
            VenueError::new(error)
                .kind(ErrorKind::Internal)
                .outcome(Outcome::NotSent),
        );
    }
}
fn history_symbol(cx: &Cx<'_, Pending>, product: &str, requested: Option<&str>) -> String {
    if let Some(s) = requested {
        return s.into();
    }
    if let Some(base) = product.strip_suffix("-USD") {
        let alias = format!("{base}-USDC");
        if cx.store.market(&alias).is_some() {
            return alias;
        }
    }
    if cx.store.market(product).is_some() {
        return product.into();
    }
    ws_symbol(product)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utc_filters_roundtrip_calendar_boundaries() {
        for (n, expected) in [
            (0.0, "1970-01-01T00:00:00.000Z"),
            (1709164800123.0, "2024-02-29T00:00:00.123Z"),
            (253402300799999.0, "9999-12-31T23:59:59.999Z"),
        ] {
            assert_eq!(rfc3339(n).unwrap(), expected);
            assert_eq!(timestamp(expected).unwrap(), n);
        }
    }
    #[test]
    fn malformed_timestamps_reject_instead_of_rolling_dates() {
        for s in [
            "2023-02-29T00:00:00Z",
            "2024-01-01T00:00:1e1Z",
            "2024-01-01T00:00:00",
            "2024-01-01T00:00:00+99:00",
            "2024-01-01T00:00:00.Z",
        ] {
            assert!(timestamp(s).is_err(), "{s}")
        }
    }
    #[test]
    fn timezone_offsets_preserve_instant() {
        assert_eq!(
            timestamp("2024-01-01T01:30:00+01:30").unwrap(),
            timestamp("2024-01-01T00:00:00Z").unwrap()
        );
    }
}

#[cfg(test)]
#[path = "../tests/support/history_family.rs"]
mod family_tests;
