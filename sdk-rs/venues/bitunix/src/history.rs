//! Rare history reads return JSON; list bodies stream into typed continuations.
use crate::account::order_side;
use crate::sign::Params;
use crate::wire::{Num, Str, data, time_ms};
use crate::{BitunixFutures, Pending};
use safe_cex_engine::Level;
use safe_cex_engine::history::{ReadCompletion, ReadCursorHistory};
use safe_cex_engine::log::code;
use safe_cex_engine::model::{OrderSide, OrderStatus, OrderType};
use safe_cex_engine::paging::{CursorPage, CursorStep, CursorWalk, Stall};
use safe_cex_engine::request_context::{CommandEntryError, Operation, Policy, RetryPolicy};
use safe_cex_engine::selected_read::{
    CommandReadError, LocalRefusal, ReadLeg, ReadLegPolicy, ReadRequest, ReadRole, ReadRoot,
    SelectedRead,
};
use safe_cex_engine::{Cx, ErrorKind, Method, Outcome, Query, Response, VenueError};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
const TRADES: &str = "/api/v1/futures/trade/get_history_trades";
const ORDERS: &str = "/api/v1/futures/trade/get_history_orders";
const POSITIONS: &str = "/api/v1/futures/position/get_history_positions";
fn local_read_policy() -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Fills,
    Orders,
    Income,
    /// Raw closed-position rows for the host's TS mappers (`FetchClosedPositions`).
    Positions,
}
pub struct Request {
    owner: ReadCursorHistory<MappedRow>,
    /// The command's own caller Read root; each page is one leg of it.
    root: ReadRoot,
    leg: Option<ReadLeg>,
    kind: Kind,
    symbol: Option<String>,
    order_id: Option<String>,
    from: Option<f64>,
    to: Option<f64>,
    limit: usize,
    count: usize,
    added: usize,
    oldest: f64,
    reached_start: bool,
    invalid: bool,
}
// The token models an absent first endTime, never a query parameter.
const UNBOUNDED: &str = "unbounded";
#[derive(Serialize)]
#[serde(untagged)]
enum MappedRow {
    Fill(Execution),
    Order(HistoricalOrder),
    Income(Income),
    Raw(Box<RawValue>),
}
impl MappedRow {
    fn timestamp(&self) -> f64 {
        match self {
            Self::Fill(row) => row.timestamp,
            Self::Order(row) => row.created_time,
            Self::Income(row) => row.timestamp,
            Self::Raw(_) => 0.0,
        }
    }
}
fn encode(rows: &[MappedRow]) -> Result<String, VenueError> {
    serde_json::to_string(rows).map_err(|error| VenueError::new(error.to_string()))
}
fn stall_message(reason: Stall) -> String {
    if reason == Stall::PageBudget {
        "Bitunix history reached the page cap; older rows were not fetched".into()
    } else {
        "Bitunix history paging did not advance; older rows may not have been fetched".into()
    }
}
fn positions_stall_message(reason: Stall) -> String {
    if reason == Stall::PageBudget {
        format!(
            "Bitunix closed positions stopped at the {POSITION_PAGES}-page cap; older positions were not fetched"
        )
    } else {
        "Bitunix closed positions paging did not advance; older positions may not have been fetched"
            .into()
    }
}
/// TS `fetchBitunixHistoryPages` page cap.
const POSITION_PAGES: u32 = 100;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PositionIdentity<'a> {
    #[serde(default, borrow)]
    mtime: Option<&'a RawValue>,
    #[serde(default, borrow)]
    ctime: Option<&'a RawValue>,
    #[serde(default, borrow)]
    position_id: Option<&'a RawValue>,
}
/// JS `String(value)` for a scalar row field; `None` for JSON null.
fn js_string(value: &RawValue) -> Option<String> {
    let text = value.get();
    if text == "null" {
        return None;
    }
    if let Ok(string) = serde_json::from_str::<String>(text) {
        return Some(string);
    }
    // A JSON number prints as JS does: in full inside [1e-6, 1e21), else in
    // exponent form, whose leading digit is all `parseInt` then reads.
    if let Ok(number) = text.parse::<f64>()
        && number.is_finite()
    {
        let magnitude = number.abs();
        return Some(if number == 0.0 {
            "0".to_owned()
        } else if (1e-6..1e21).contains(&magnitude) {
            format!("{number}")
        } else {
            format!("{number:e}")
        });
    }
    Some(text.to_owned())
}
/// JS `parseInt(text, 10)`; `None` is NaN.
fn js_parse_int(text: &str) -> Option<f64> {
    let text = text.trim_start();
    let (sign, rest) = match text.as_bytes().first() {
        Some(b'-') => (-1.0, &text[1..]),
        Some(b'+') => (1.0, &text[1..]),
        _ => (1.0, text),
    };
    let digits = &rest[..rest.bytes().take_while(u8::is_ascii_digit).count()];
    digits.parse::<f64>().ok().map(|value| sign * value)
}
/// TS keys: `parseInt(String(r.mtime ?? r.ctime ?? '0'), 10)` and `String(r.positionId ?? '')`.
fn position_identity(item: &str) -> (Option<f64>, String) {
    let Ok(row) = serde_json::from_str::<PositionIdentity>(item) else {
        return (Some(0.0), String::new());
    };
    let time = row
        .mtime
        .and_then(js_string)
        .or_else(|| row.ctime.and_then(js_string))
        .map_or(Some(0.0), |text| js_parse_int(&text));
    let id = row.position_id.and_then(js_string).unwrap_or_default();
    (time, id)
}
impl Request {
    /// The exact live page of this command's root.
    pub(crate) fn delivered(&self, cx: &Cx<'_, Pending>) -> bool {
        self.leg.as_ref().is_some_and(|leg| {
            self.root.validate_delivery(cx, leg).is_ok()
                && self.owner.validate_delivery(cx, leg.context()).is_ok()
        })
    }
    pub(crate) fn release(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(leg) = self.leg.take() {
            let _ = cx.finish_http_context(leg.context());
        }
    }
    fn order_cursor(&self) -> Option<f64> {
        self.owner
            .cursor()
            .filter(|cursor| *cursor != UNBOUNDED)
            .map(|cursor| cursor.parse().expect("native finite time cursor"))
    }
    fn skip_page(&self) -> u32 {
        self.owner
            .cursor()
            .map_or(0, |cursor| cursor.parse().expect("native page index"))
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Execution {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    timestamp: f64,
    symbol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_id: Option<String>,
    price: f64,
    qty: f64,
    side: OrderSide,
    commission: f64,
    commission_asset: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HistoricalOrder {
    id: String,
    symbol: String,
    side: OrderSide,
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
struct Income {
    symbol: String,
    income_type: &'static str,
    income: f64,
    asset: String,
    timestamp: f64,
    tran_id: String,
    trade_id: String,
    category: &'static str,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Row<'a> {
    #[serde(borrow, default, deserialize_with = "crate::wire::text")]
    trade_id: Str<'a>,
    #[serde(
        borrow,
        default,
        deserialize_with = "crate::wire::text",
        alias = "ordId"
    )]
    order_id: Str<'a>,
    #[serde(
        borrow,
        default,
        deserialize_with = "crate::wire::text",
        alias = "instId",
        alias = "contract",
        alias = "instrument"
    )]
    symbol: Str<'a>,
    #[serde(borrow, default, deserialize_with = "crate::wire::text")]
    side: Str<'a>,
    #[serde(borrow, default, deserialize_with = "crate::wire::text")]
    order_type: Str<'a>,
    #[serde(borrow, default, deserialize_with = "crate::wire::text")]
    status: Str<'a>,
    #[serde(borrow, default, deserialize_with = "crate::wire::text")]
    ctime: Str<'a>,
    #[serde(borrow, default, deserialize_with = "crate::wire::text")]
    mtime: Str<'a>,
    #[serde(borrow, default, deserialize_with = "crate::wire::text")]
    time: Str<'a>,
    #[serde(borrow, default, deserialize_with = "crate::wire::text")]
    ts: Str<'a>,
    #[serde(borrow, default, deserialize_with = "crate::wire::text")]
    trade_time: Str<'a>,
    #[serde(borrow, default, deserialize_with = "crate::wire::text")]
    create_time: Str<'a>,
    #[serde(borrow, default, deserialize_with = "crate::wire::text")]
    updated_time: Str<'a>,
    #[serde(default)]
    price: Option<Num>,
    #[serde(default)]
    avg_price: Option<Num>,
    #[serde(default)]
    fill_price: Option<Num>,
    #[serde(default)]
    qty: Option<Num>,
    #[serde(default)]
    trade_qty: Option<Num>,
    #[serde(default)]
    fill_size: Option<Num>,
    #[serde(default)]
    fee: Option<Num>,
    #[serde(default)]
    exec_fee: Option<Num>,
    #[serde(default, alias = "realizedPNL")]
    realized_pnl: Num,
    #[serde(borrow, default, deserialize_with = "crate::wire::text")]
    margin_coin: Str<'a>,
    #[serde(borrow, default, deserialize_with = "crate::wire::text")]
    fee_currency: Str<'a>,
    #[serde(borrow, default, deserialize_with = "crate::wire::text")]
    fee_coin: Str<'a>,
    #[serde(borrow, default)]
    reduce_only: Option<crate::wire::Supported<'a>>,
}
fn millis(v: f64) -> f64 {
    if v >= 1e18 {
        (v / 1e6).floor()
    } else if v >= 1e15 {
        (v / 1000.0).floor()
    } else if v >= 1e12 {
        v.floor()
    } else {
        v.floor() * 1000.0
    }
}
fn optional(v: f64) -> Option<f64> {
    if v != 0.0 && v.is_finite() {
        Some(v)
    } else {
        None
    }
}
impl BitunixFutures {
    pub(crate) fn query(&mut self, cx: &mut Cx<'_, Pending>, command: u32, query: Query) {
        if matches!(query, Query::OrderByClientId { .. }) {
            let Ok(context) = cx.admit_http_command(local_read_policy()) else {
                return;
            };
            let _ = cx.fail_context(
                &context,
                VenueError::new("Bitunix client order lookup is not supported by the Rust engine")
                    .kind(ErrorKind::Unsupported)
                    .outcome(Outcome::NotSent),
            );
            return;
        }
        if matches!(query, Query::PublicTrades { .. }) {
            let Ok(context) = cx.admit_http_command(local_read_policy()) else {
                return;
            };
            let _ = cx.fail_context(
                &context,
                VenueError::new("Not implemented")
                    .kind(ErrorKind::Unsupported)
                    .outcome(Outcome::NotSent),
            );
            return;
        }
        if !self.slots.has_primary() {
            let Ok(context) = cx.admit_http_command(local_read_policy()) else {
                return;
            };
            let _ = cx.done_context(&context, "[]".into());
            return;
        }
        let (kind, symbol, order_id, from, to, limit) = match query {
            Query::Fills {
                symbol,
                order_id,
                from_ms,
                to_ms,
                limit,
            } => (
                Kind::Fills,
                symbol,
                order_id,
                from_ms.map(millis),
                to_ms.map(millis),
                limit.unwrap_or(100).clamp(1, 100) as usize,
            ),
            Query::OrderHistory {
                symbol,
                from_ms,
                to_ms,
                limit,
            } => (
                Kind::Orders,
                symbol,
                None,
                from_ms,
                to_ms,
                limit.filter(|l| *l > 0).map_or(usize::MAX, |l| l as usize),
            ),
            Query::Income {
                symbol,
                kind: _,
                from_ms,
                to_ms,
                limit: _,
            } => (Kind::Income, symbol, None, from_ms, to_ms, usize::MAX),
            Query::PublicTrades { .. } | Query::OrderByClientId { .. } => unreachable!(),
        };
        self.start_history(cx, command, kind, symbol, order_id, from, to, limit);
    }
    /// `FetchClosedPositions`: TS `fetchClosedPositionRows` over the reported
    /// history-positions endpoint, rows passed through raw for the host mappers.
    pub(crate) fn closed_positions(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: u32,
        symbol: Option<String>,
        from: Option<f64>,
        to: Option<f64>,
    ) {
        if !self.slots.has_primary() {
            let Ok(context) = cx.admit_http_command(local_read_policy()) else {
                return;
            };
            let _ = cx.done_context(&context, "[]".into());
            return;
        }
        self.start_history(
            cx,
            command,
            Kind::Positions,
            symbol,
            None,
            from,
            to,
            usize::MAX,
        );
    }
    #[allow(clippy::too_many_arguments)]
    fn start_history(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: u32,
        kind: Kind,
        symbol: Option<String>,
        order_id: Option<String>,
        from: Option<f64>,
        to: Option<f64>,
        limit: usize,
    ) {
        if kind == Kind::Fills && symbol.as_ref().is_none_or(|s| cx.store.market(s).is_none()) {
            let Ok(context) = cx.admit_http_command(local_read_policy()) else {
                return;
            };
            let _ = cx.done_context(&context, "[]".into());
            return;
        }
        if [from, to].into_iter().flatten().any(|v| !v.is_finite()) {
            let Ok(context) = cx.admit_http_command(local_read_policy()) else {
                return;
            };
            let _ = cx.fail_context(
                &context,
                VenueError::new("Bitunix history time must be finite")
                    .kind(ErrorKind::InvalidInput)
                    .outcome(Outcome::NotSent),
            );
            return;
        }
        let walk = match kind {
            Kind::Orders => CursorWalk::starting_at(
                50,
                to.map_or_else(|| UNBOUNDED.into(), |time| time.to_string()),
            )
            .stall_on_no_new_rows(),
            Kind::Income => CursorWalk::starting_at(100, "0".into()).stall_on_no_new_rows(),
            Kind::Fills => CursorWalk::new(1).stall_on_no_new_rows(),
            // TS ends the walk, silently, on a page that adds nothing.
            Kind::Positions => {
                CursorWalk::starting_at(POSITION_PAGES, "0".into()).end_on_no_new_rows()
            }
        };
        let selected = SelectedRead::select(&self.slots, ReadRole::Read { scope: None });
        let root = match SelectedRead::command_root(
            cx,
            selected,
            LocalRefusal {
                diagnostic: None,
                error: VenueError::new("Bitunix credentials required").kind(ErrorKind::AuthInvalid),
            },
        ) {
            Ok(root) => root,
            Err(CommandReadError::Entry(CommandEntryError::Revoked)) => {
                let _ = cx.reject_expired_command(Operation::Read);
                return;
            }
            Err(_) => return,
        };
        let completion = match ReadCompletion::from_context(cx, command, root.context().clone()) {
            Ok(completion) => completion,
            Err(reason) => {
                let _ = cx.fail_context(root.context(), reason.read_venue_error());
                return;
            }
        };
        let read = Request {
            owner: ReadCursorHistory::new(
                completion,
                walk,
                if kind == Kind::Positions {
                    positions_stall_message
                } else {
                    stall_message
                },
                encode,
            ),
            root,
            leg: None,
            kind,
            symbol,
            order_id,
            from,
            to,
            limit,
            count: 0,
            added: 0,
            oldest: f64::INFINITY,
            reached_start: false,
            invalid: false,
        };
        self.history_page(cx, read);
    }
    fn history_page(&mut self, cx: &mut Cx<'_, Pending>, mut read: Request) {
        let mut params = Params::default()
            .opt_str("symbol", read.symbol.as_deref())
            .opt_num("startTime", read.from)
            .opt_num(
                "endTime",
                if read.kind == Kind::Orders {
                    read.order_cursor()
                } else {
                    read.to
                },
            );
        let limit = if read.kind == Kind::Fills {
            read.limit
        } else {
            100
        };
        params = params.num("limit", limit as f64);
        if read.kind == Kind::Fills {
            params = params.opt_str("orderId", read.order_id.as_deref());
        }
        if matches!(read.kind, Kind::Income | Kind::Positions) && read.skip_page() > 0 {
            params = params.num("skip", f64::from(read.skip_page() * 100));
        }
        read.count = 0;
        read.added = 0;
        read.oldest = f64::INFINITY;
        read.reached_start = false;
        read.invalid = false;
        let leg = match read.root.leg(
            cx,
            ReadLegPolicy {
                timeout_ms: 20_000,
                pacing: None,
            },
        ) {
            Ok(leg) => leg,
            // An expired or retired root already has the engine's terminal.
            Err(safe_cex_engine::request_context::Error::Revoked) => return,
            Err(reason) => {
                let _ = read.owner.reject(cx, reason.read_venue_error());
                return;
            }
        };
        let path = match read.kind {
            Kind::Orders => ORDERS,
            Kind::Positions => POSITIONS,
            Kind::Fills | Kind::Income => TRADES,
        };
        let query = params.query();
        let url = format!("{}{path}?{query}", crate::REST);
        let signing = params.signing();
        let testnet = self.options.testnet;
        let request_url = url.clone();
        if let Err((Pending::History(mut read), reason)) = leg.queue(
            cx,
            ReadRequest {
                method: Method::Get,
                url,
                body: None,
                stream: Some(crate::recovery::LIST),
            },
            |identity| {
                Box::new(crate::sign::dispatch::Http::new(
                    identity.clone(),
                    Method::Get,
                    request_url,
                    signing,
                    None,
                    testnet,
                ))
            },
            |leg| {
                read.leg = Some(leg);
                Pending::History(read)
            },
        ) {
            read.release(cx);
            let _ = read.owner.reject(cx, reason.read_venue_error());
        }
    }
    pub(crate) fn history_item(&self, cx: &Cx<'_, Pending>, read: &mut Request, item: &str) {
        // Rows admit only under this command's exact live page.
        let Some(leg) = read.leg.as_ref() else {
            return;
        };
        if read.root.validate_delivery(cx, leg).is_err() {
            return;
        }
        let Ok(mut page) = read.owner.admit(cx, leg.context()) else {
            return;
        };
        read.count += 1;
        if read.kind == Kind::Positions {
            let Ok(raw) = RawValue::from_string(item.to_owned()) else {
                read.invalid = true;
                return;
            };
            let (time, id) = position_identity(item);
            // TS `startTime && ts && ts < startTime`: zero and NaN never bound.
            let time = time.filter(|time| *time != 0.0);
            if let (Some(from), Some(time)) = (read.from.filter(|f| *f != 0.0), time)
                && time < from
            {
                read.reached_start = true;
                return;
            }
            if let (Some(to), Some(time)) = (read.to.filter(|t| *t != 0.0), time)
                && time > to
            {
                return;
            }
            if !id.is_empty() && !page.fresh(id) {
                return;
            }
            read.added += 1;
            page.stage(MappedRow::Raw(raw));
            return;
        }
        let row: Row<'_> = match serde_json::from_str(item) {
            Ok(r) => r,
            Err(_) => {
                read.invalid = true;
                return;
            }
        };
        let created = time_ms(&row.ctime);
        let timestamp = if read.kind == Kind::Fills {
            let time = [
                &row.ctime,
                &row.mtime,
                &row.time,
                &row.ts,
                &row.trade_time,
                &row.create_time,
                &row.updated_time,
            ]
            .into_iter()
            .find(|s| !s.is_empty())
            .map_or(0.0, |s| time_ms(s));
            millis(time)
        } else {
            created
        };
        if created > 0.0 {
            read.oldest = read.oldest.min(created);
        }
        let id = if read.kind == Kind::Orders {
            row.order_id.as_ref()
        } else {
            row.trade_id.as_ref()
        };
        // Raw ids, including rows later rejected by a mapper, drive paging progress.
        if read.kind != Kind::Fills && !id.is_empty() && !page.fresh(id.to_owned()) {
            return;
        }
        if read.kind == Kind::Orders && !id.is_empty() {
            read.added += 1;
        }
        if read
            .from
            .is_some_and(|from| timestamp > 0.0 && timestamp < from)
        {
            read.reached_start = true;
            return;
        }
        if read.to.is_some_and(|to| timestamp > to) {
            return;
        }
        if read.kind == Kind::Income {
            read.added += 1;
        }
        match read.kind {
            Kind::Fills => {
                let symbol = if row.symbol.is_empty() {
                    read.symbol.as_deref().unwrap_or("")
                } else {
                    &row.symbol
                };
                if symbol
                    .bytes()
                    .filter(u8::is_ascii_alphanumeric)
                    .map(|b| b.to_ascii_uppercase())
                    .ne(read
                        .symbol
                        .as_deref()
                        .unwrap_or("")
                        .bytes()
                        .filter(u8::is_ascii_alphanumeric)
                        .map(|b| b.to_ascii_uppercase()))
                    || !matches!(row.side.to_ascii_uppercase().as_str(), "BUY" | "SELL")
                {
                    return;
                }
                let price = row
                    .price
                    .or(row.avg_price)
                    .or(row.fill_price)
                    .unwrap_or_default()
                    .0;
                let qty = row
                    .qty
                    .or(row.trade_qty)
                    .or(row.fill_size)
                    .unwrap_or_default()
                    .0;
                if price <= 0.0
                    || qty <= 0.0
                    || timestamp <= 0.0
                    || read
                        .order_id
                        .as_ref()
                        .is_some_and(|id| !row.order_id.is_empty() && id != row.order_id.as_ref())
                {
                    return;
                }
                let side = order_side(&row.side);
                let key = if row.trade_id.is_empty() {
                    format!("{}:{timestamp}:{price}:{qty}:{side:?}", row.order_id)
                } else {
                    row.trade_id.to_string()
                };
                if !page.fresh(key) {
                    return;
                }
                let asset = if !row.fee_currency.is_empty() {
                    row.fee_currency.as_ref()
                } else if !row.fee_coin.is_empty() {
                    &row.fee_coin
                } else if !row.margin_coin.is_empty() {
                    &row.margin_coin
                } else {
                    cx.store.market(symbol).map_or("USDT", |m| m.quote.as_str())
                };
                page.stage(MappedRow::Fill(Execution {
                    id: if row.trade_id.is_empty() {
                        None
                    } else {
                        Some(row.trade_id.to_string())
                    },
                    timestamp: (timestamp / 1000.0).floor(),
                    symbol: read.symbol.clone().unwrap_or_default(),
                    order_id: if row.order_id.is_empty() {
                        None
                    } else {
                        Some(row.order_id.to_string())
                    },
                    price,
                    qty,
                    side,
                    commission: row.fee.or(row.exec_fee).unwrap_or_default().0.abs(),
                    commission_asset: asset.to_owned(),
                }));
            }
            Kind::Orders => {
                if row.order_id.is_empty()
                    || row.symbol.is_empty()
                    || created <= 0.0
                    || !matches!(row.side.to_ascii_uppercase().as_str(), "BUY" | "SELL")
                {
                    return;
                }
                let status = match row.status.to_ascii_uppercase().as_str() {
                    "INIT" | "NEW" | "PART_FILLED" => OrderStatus::Open,
                    "CANCELED" | "CANCELLED" | "EXPIRED" | "REJECTED" => OrderStatus::Canceled,
                    _ => OrderStatus::Closed,
                };
                page.stage(MappedRow::Order(HistoricalOrder {
                    id: row.order_id.to_string(),
                    symbol: row.symbol.to_string(),
                    side: order_side(&row.side),
                    kind: if row.order_type.eq_ignore_ascii_case("MARKET") {
                        OrderType::Market
                    } else {
                        OrderType::Limit
                    },
                    status,
                    price: optional(row.price.unwrap_or_default().0),
                    average_price: optional(row.avg_price.unwrap_or_default().0),
                    amount: row.qty.unwrap_or_default().0,
                    filled: row.trade_qty.unwrap_or_default().0,
                    reduce_only: row
                        .reduce_only
                        .as_ref()
                        .and_then(crate::wire::Supported::boolean)
                        .unwrap_or(false),
                    created_time: created,
                    updated_time: optional(time_ms(&row.mtime)),
                }));
            }
            Kind::Income => {
                if created <= 0.0 || row.symbol.is_empty() {
                    return;
                }
                let id = if row.trade_id.is_empty() {
                    format!("{created}:{}", row.symbol)
                } else {
                    row.trade_id.to_string()
                };
                let asset = if row.margin_coin.is_empty() {
                    "USDT"
                } else {
                    &row.margin_coin
                };
                for (income, kind, category, suffix) in [
                    (row.realized_pnl.0, "REALIZED_PNL", "realized_pnl", "pnl"),
                    (
                        -row.fee.unwrap_or_default().0,
                        if row.fee.unwrap_or_default().0 > 0.0 {
                            "COMMISSION"
                        } else {
                            "REBATE"
                        },
                        if row.fee.unwrap_or_default().0 > 0.0 {
                            "commission"
                        } else {
                            "rebate"
                        },
                        "fee",
                    ),
                ] {
                    if income != 0.0 {
                        page.stage(MappedRow::Income(Income {
                            symbol: row.symbol.to_string(),
                            income_type: kind,
                            income,
                            asset: asset.to_string(),
                            timestamp: created,
                            tran_id: format!("{id}:{suffix}"),
                            trade_id: id.clone(),
                            category,
                        }));
                    }
                }
            }
            // Staged raw above, before the typed row parse.
            Kind::Positions => {}
        }
    }
    pub(crate) fn history_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut read: Request,
        response: Response<'_>,
    ) {
        if !read.delivered(cx) {
            // A stale or mismatched delivery still closes its own leg.
            read.release(cx);
            return;
        }
        if read.kind == Kind::Positions
            && (!(200..300).contains(&response.status) || data(response.body).is_err())
        {
            self.positions_failed(cx, read, &response);
            return;
        }
        if !(200..300).contains(&response.status) || data(response.body).is_err() || read.invalid {
            self.history_failed(cx, read, "Bitunix history read failed");
            return;
        }
        let more = read.kind != Kind::Fills && read.count >= 100 && !read.reached_start;
        // Endpoint facts only. A native invalid/nonadvancing time bound repeats
        // the registered cursor; the shared pager owns the resulting stall.
        let next = more.then(|| match read.kind {
            Kind::Orders => {
                if !read.oldest.is_finite()
                    || read
                        .order_cursor()
                        .is_some_and(|cursor| read.oldest >= cursor)
                    || read.from.is_some_and(|from| read.oldest <= from)
                {
                    read.owner.cursor().expect("seeded order cursor").to_owned()
                } else {
                    read.oldest.to_string()
                }
            }
            Kind::Income | Kind::Positions => (read.skip_page() + 1).to_string(),
            Kind::Fills => unreachable!("single-page fills never continue"),
        });
        let step = {
            let leg = read.leg.as_ref().expect("delivered page");
            let Ok(mut page) = read.owner.admit(cx, leg.context()) else {
                read.release(cx);
                return;
            };
            page.advance(CursorPage {
                next,
                rows: read.count,
                fresh: read.added,
                want_more: more,
            })
        };
        read.release(cx);
        match step {
            CursorStep::More(_) => self.history_page(cx, read),
            CursorStep::Done | CursorStep::Stalled(_) => self.history_done(cx, read, Ok(())),
        }
    }
    /// A body without the expected list: closed positions keep the venue's
    /// classified verdict; the other history kinds keep their native text.
    pub(crate) fn history_stream_error(
        &self,
        cx: &mut Cx<'_, Pending>,
        read: Request,
        response: &Response<'_>,
    ) {
        if read.kind == Kind::Positions {
            self.positions_failed(cx, read, response);
        } else {
            let error = format!(
                "response did not contain the expected list: {}",
                response.body
            );
            self.history_failed(cx, read, &error);
        }
    }
    fn positions_failed(&self, cx: &mut Cx<'_, Pending>, read: Request, response: &Response<'_>) {
        let mut error = crate::codes::read_failure(response, "Bitunix history read failed").error;
        error.message = format!("Bitunix closed positions: {}", error.message);
        cx.notice(Level::Warn, code::REQUEST_FAILED, &error.message);
        self.history_done(cx, read, Err(error));
    }
    pub(crate) fn history_failed(&self, cx: &mut Cx<'_, Pending>, read: Request, error: &str) {
        cx.notice(Level::Warn, code::REQUEST_FAILED, error);
        // The shared callback carries both transport and stream-decode errors;
        // retain Internal fallback rather than misclassifying all as Network.
        self.history_done(cx, read, Err(VenueError::new(error)));
    }
    pub(crate) fn history_sign_failed(
        &self,
        cx: &mut Cx<'_, Pending>,
        mut read: Request,
        error: &str,
    ) {
        read.release(cx);
        cx.notice(Level::Warn, code::REQUEST_FAILED, error);
        let kind = if !self.slots.has_primary() {
            ErrorKind::AuthInvalid
        } else {
            ErrorKind::Internal
        };
        let _ = read.owner.reject(
            cx,
            VenueError::new(error).kind(kind).outcome(Outcome::NotSent),
        );
    }
    fn history_done(
        &self,
        cx: &mut Cx<'_, Pending>,
        mut read: Request,
        result: Result<(), VenueError>,
    ) {
        read.release(cx);
        let Request {
            owner, kind, limit, ..
        } = read;
        let _ = owner.finish_with(cx, result, move |mut rows| {
            // TS returns closed-position rows in page order; the host sorts mapped entries.
            if kind != Kind::Positions {
                rows.sort_by(|a, b| b.timestamp().total_cmp(&a.timestamp()));
            }
            if !matches!(kind, Kind::Income | Kind::Positions) {
                rows.truncate(limit);
            }
            encode(&rows)
        });
    }
}
