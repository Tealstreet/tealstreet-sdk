//! Cursor-owned history reads. Every streamed page remains private until its
//! envelope succeeds, and an unwalkable boundary is an explicit incomplete read.
use crate::sign::form;
use crate::wire::{number, optional_number};
use crate::{Kraken, Pending};
use safe_cex_engine::history::{Completion, CursorHistory};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::{OrderSide, OrderStatus, OrderType};
use safe_cex_engine::paging::{CursorPage, CursorStep, CursorWalk, Stall};
use safe_cex_engine::time;
use safe_cex_engine::{Command, Cx, Method, Query, Response, VenueError};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, collections::HashMap};

const FILLS_PAGE: usize = 100;
const LEDGER_PAGE: usize = 500;
const ORDERS_PAGE: usize = 1000;
const MAX_PAGES: u32 = 50;
const WEEK: f64 = 7.0 * 86_400_000.0;

#[derive(Clone, Copy, PartialEq)]
enum Read {
    Fills,
    Income,
    Orders,
    Trades,
}
pub struct Request {
    history: Box<CursorHistory<Row>>,
    read: Read,
    symbol: Option<String>,
    order: Option<String>,
    kind: Option<String>,
    from: f64,
    to: f64,
    limit: usize,
    count: usize,
    fresh: usize,
    oldest: f64,
    newest: f64,
    previous_time: Option<f64>,
    ascending: bool,
    descending: bool,
    failed: Option<String>,
}
impl Request {
    fn output_len(&self) -> usize {
        self.history.accepted_len() + self.history.staged_len()
    }
    fn reset_page(&mut self) {
        self.count = 0;
        self.fresh = 0;
        self.oldest = f64::INFINITY;
        self.newest = f64::NEG_INFINITY;
        self.previous_time = None;
        self.ascending = true;
        self.descending = true;
        self.failed = None;
    }
    fn observe(&mut self, time: f64, id: &str) -> Result<bool, String> {
        if !time.is_finite() || time < 0.0 || id.is_empty() {
            return Err("Kraken history row has an invalid timestamp or identity".into());
        }
        self.count += 1;
        self.oldest = self.oldest.min(time);
        self.newest = self.newest.max(time);
        if self.previous_time.is_some_and(|previous| previous > time) {
            self.ascending = false;
        }
        if self.previous_time.is_some_and(|previous| previous < time) {
            self.descending = false;
        }
        self.previous_time = Some(time);
        let fresh = self.history.fresh(id.into());
        self.fresh += usize::from(fresh);
        Ok(fresh)
    }
    fn within(&self, time: f64, symbol: &str) -> bool {
        time >= self.from
            && time <= self.to
            && self
                .symbol
                .as_ref()
                .is_none_or(|expected| expected.eq_ignore_ascii_case(symbol))
    }
}
struct Row {
    time: f64,
    data: Data,
}
#[derive(Serialize)]
#[serde(untagged)]
enum Data {
    Execution(Execution),
    Income(Income),
    Order(HistoricalOrder),
    Trade(Trade),
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Execution {
    id: String,
    timestamp: f64,
    symbol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_id: Option<String>,
    price: f64,
    qty: f64,
    side: OrderSide,
    maker: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    realized_pnl: Option<f64>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Income {
    symbol: String,
    income_type: String,
    income: f64,
    asset: String,
    timestamp: f64,
    tran_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    trade_id: Option<String>,
    category: &'static str,
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
    price: f64,
    amount: f64,
    filled: f64,
    reduce_only: bool,
    created_time: f64,
    updated_time: f64,
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
    side: OrderSide,
}
#[derive(Default, Deserialize)]
#[serde(transparent)]
struct Atom<'a>(#[serde(borrow, deserialize_with = "atom")] Cow<'a, str>);
fn atom<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Cow<'de, str>, D::Error> {
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = Cow<'de, str>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("string or exact integer")
        }
        fn visit_borrowed_str<E: serde::de::Error>(self, s: &'de str) -> Result<Self::Value, E> {
            Ok(Cow::Borrowed(s))
        }
        fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<Self::Value, E> {
            Ok(Cow::Owned(s.into()))
        }
        fn visit_u64<E: serde::de::Error>(self, n: u64) -> Result<Self::Value, E> {
            Ok(Cow::Owned(n.to_string()))
        }
        fn visit_i64<E: serde::de::Error>(self, n: i64) -> Result<Self::Value, E> {
            Ok(Cow::Owned(n.to_string()))
        }
    }
    d.deserialize_any(Visitor)
}
#[derive(Deserialize)]
struct FillWire<'a> {
    #[serde(borrow)]
    fill_id: Atom<'a>,
    #[serde(borrow)]
    symbol: Cow<'a, str>,
    #[serde(borrow)]
    side: Cow<'a, str>,
    #[serde(default, borrow)]
    order_id: Option<Atom<'a>>,
    #[serde(deserialize_with = "number")]
    size: f64,
    #[serde(deserialize_with = "number")]
    price: f64,
    #[serde(borrow, rename = "fillTime")]
    fill_time: Cow<'a, str>,
    #[serde(default, borrow, rename = "fillType")]
    fill_type: Cow<'a, str>,
    #[serde(default, deserialize_with = "optional_number")]
    realized_pnl: Option<f64>,
}
#[derive(Deserialize)]
struct LedgerWire<'a> {
    #[serde(borrow)]
    date: Cow<'a, str>,
    #[serde(default, borrow)]
    booking_uid: Atom<'a>,
    #[serde(default, borrow)]
    id: Atom<'a>,
    #[serde(default, borrow)]
    contract: Cow<'a, str>,
    #[serde(default, borrow)]
    info: Cow<'a, str>,
    #[serde(borrow)]
    asset: Cow<'a, str>,
    #[serde(default, borrow)]
    execution: Option<Atom<'a>>,
    #[serde(default, deserialize_with = "optional_number")]
    realized_pnl: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    realized_funding: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    fee: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    liquidation_fee: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    old_balance: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    new_balance: Option<f64>,
}
#[derive(Deserialize)]
struct TradeWire<'a> {
    #[serde(default, borrow)]
    trade_id: Atom<'a>,
    #[serde(default, borrow)]
    uid: Cow<'a, str>,
    #[serde(borrow)]
    time: Cow<'a, str>,
    #[serde(borrow)]
    side: Cow<'a, str>,
    #[serde(deserialize_with = "number")]
    size: f64,
    #[serde(deserialize_with = "number")]
    price: f64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrderWire<'a> {
    #[serde(borrow)]
    uid: Cow<'a, str>,
    #[serde(borrow)]
    tradeable: Cow<'a, str>,
    #[serde(borrow)]
    direction: Cow<'a, str>,
    #[serde(deserialize_with = "number")]
    quantity: f64,
    #[serde(deserialize_with = "number")]
    filled: f64,
    #[serde(deserialize_with = "number")]
    timestamp: f64,
    #[serde(deserialize_with = "number")]
    limit_price: f64,
    #[serde(borrow)]
    order_type: Cow<'a, str>,
    reduce_only: bool,
    #[serde(deserialize_with = "number")]
    last_update_timestamp: f64,
}
#[derive(Deserialize)]
struct OrderEventWire<'a> {
    #[serde(borrow)]
    uid: Cow<'a, str>,
    #[serde(deserialize_with = "number")]
    timestamp: f64,
    #[serde(borrow)]
    event: OrderEvent<'a>,
}
#[derive(Deserialize)]
enum OrderEvent<'a> {
    #[serde(rename = "OrderPlaced")]
    Placed {
        #[serde(borrow)]
        order: OrderWire<'a>,
    },
    #[serde(rename = "OrderUpdated")]
    Updated {
        #[serde(borrow, rename = "newOrder")]
        new_order: OrderWire<'a>,
    },
    #[serde(rename = "OrderRejected")]
    Rejected {
        #[serde(borrow)]
        order: OrderWire<'a>,
    },
    #[serde(rename = "OrderCancelled")]
    Cancelled {
        #[serde(borrow)]
        order: OrderWire<'a>,
    },
    #[serde(rename = "OrderEditRejected")]
    EditRejected {
        #[serde(borrow, rename = "oldOrder")]
        _old_order: OrderWire<'a>,
    },
    #[serde(rename = "OrderNotFound")]
    NotFound(serde::de::IgnoredAny),
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope<'a> {
    #[serde(default, borrow)]
    result: Cow<'a, str>,
    #[serde(default, borrow)]
    error: Cow<'a, str>,
    #[serde(default, borrow)]
    status: Cow<'a, str>,
    #[serde(default, borrow)]
    reason: Cow<'a, str>,
    #[serde(default)]
    fills: Option<serde::de::IgnoredAny>,
    #[serde(default)]
    logs: Option<serde::de::IgnoredAny>,
    #[serde(default)]
    elements: Option<serde::de::IgnoredAny>,
    #[serde(default)]
    history: Option<serde::de::IgnoredAny>,
    #[serde(default)]
    len: Option<usize>,
    #[serde(default, borrow)]
    continuation_token: Option<Cow<'a, str>>,
}
fn side(value: &str) -> Result<OrderSide, String> {
    match value.to_ascii_lowercase().as_str() {
        "buy" => Ok(OrderSide::Buy),
        "sell" => Ok(OrderSide::Sell),
        _ => Err("Kraken history row has an unknown side".into()),
    }
}
fn kind(value: &str, price: f64) -> Result<OrderType, String> {
    Ok(match value.to_ascii_lowercase().as_str() {
        "limit" | "ioc" | "post" | "fillorkill" | "hedgeimmediateorcancel" => OrderType::Limit,
        // Historical orders describe an activated Stop's execution order, not
        // the trigger (which lives on a separate history endpoint).
        "stop" if price > 0.0 => OrderType::Limit,
        "stop" | "market" | "liquidation" | "assignment" | "unwind" | "block"
        | "coveredliquidation" | "hedgeassignment" | "rfq" | "partialliquidation" => {
            OrderType::Market
        }
        _ => return Err("Kraken history row has an unknown order type".into()),
    })
}
impl Kraken {
    pub(crate) fn history_command(&mut self, cx: &mut Cx<'_, Pending>, command: &Command) -> bool {
        let Command::Query { command, query } = command else {
            return false;
        };
        let (read, symbol, order, kind, from, to, limit) = match query {
            Query::OrderByClientId { .. } => {
                crate::public_context::local(
                    cx,
                    safe_cex_engine::request_context::Operation::Read,
                    Err("Kraken does not support caller client order IDs or their lookup".into()),
                );
                return true;
            }
            Query::Fills {
                symbol,
                order_id,
                from_ms,
                to_ms,
                limit,
            } => (
                Read::Fills,
                symbol.clone(),
                order_id.clone(),
                None,
                *from_ms,
                *to_ms,
                *limit,
            ),
            Query::Income {
                symbol,
                kind,
                from_ms,
                to_ms,
                limit,
            } => (
                Read::Income,
                symbol.clone(),
                None,
                kind.clone(),
                *from_ms,
                *to_ms,
                *limit,
            ),
            Query::OrderHistory {
                symbol,
                from_ms,
                to_ms,
                limit,
            } => (
                Read::Orders,
                symbol.clone(),
                None,
                None,
                *from_ms,
                *to_ms,
                *limit,
            ),
            Query::PublicTrades { symbol, limit } => (
                Read::Trades,
                Some(symbol.clone()),
                None,
                None,
                None,
                None,
                *limit,
            ),
        };
        let to = to.unwrap_or(cx.now);
        let from = from.unwrap_or((to - WEEK).max(0.0));
        if !from.is_finite()
            || !to.is_finite()
            || from < 0.0
            || to < from
            || to > 253_402_300_799_999.0
        {
            crate::public_context::local(
                cx,
                safe_cex_engine::request_context::Operation::Read,
                Err(
                    "Kraken history range must contain valid milliseconds in ascending order"
                        .into(),
                ),
            );
            return true;
        }
        let request = Request {
            history: Box::new(CursorHistory::new(
                Completion::new(*command),
                CursorWalk::new(MAX_PAGES)
                    .stall_on_empty_page()
                    .stall_on_no_new_rows(),
                stall_message,
                |rows| {
                    serde_json::to_string(&rows.iter().map(|row| &row.data).collect::<Vec<_>>())
                        .map_err(|error| VenueError::new(error.to_string()))
                },
            )),
            read,
            symbol,
            order,
            kind,
            from,
            to,
            limit: limit.filter(|n| *n > 0).map_or(
                if read == Read::Trades {
                    100
                } else {
                    usize::MAX
                },
                |n| n as usize,
            ),
            count: 0,
            fresh: 0,
            oldest: f64::INFINITY,
            newest: f64::NEG_INFINITY,
            previous_time: None,
            ascending: true,
            descending: true,
            failed: None,
        };
        self.history_request(cx, request);
        true
    }
    fn history_request(&mut self, cx: &mut Cx<'_, Pending>, mut request: Request) {
        request.reset_page();
        let mut params = Vec::new();
        let (path, at) = match request.read {
            Read::Fills => {
                if let Some(cursor) = request.history.cursor() {
                    params.push(("lastFillTime", cursor.to_owned()));
                }
                ("/derivatives/api/v3/fills", ArrayAt::Field("fills"))
            }
            Read::Income => {
                params.push((
                    "since",
                    request
                        .history
                        .cursor()
                        .map(str::to_owned)
                        .unwrap_or_else(|| format!("{:.0}", request.from.floor())),
                ));
                params.push(("count", LEDGER_PAGE.to_string()));
                ("/api/history/v3/account-log", ArrayAt::Field("logs"))
            }
            Read::Orders => {
                params.extend([
                    ("since", format!("{:.0}", request.from.floor())),
                    ("before", format!("{:.0}", request.to.ceil() + 1.0)),
                    ("count", ORDERS_PAGE.to_string()),
                    ("sort", "desc".into()),
                    ("closed", "true".into()),
                    ("opened", "false".into()),
                ]);
                if let Some(symbol) = &request.symbol {
                    params.push(("tradeable", symbol.to_ascii_lowercase()));
                }
                if let Some(cursor) = request.history.cursor() {
                    params.push(("continuation_token", cursor.to_owned()));
                }
                ("/api/history/v3/orders", ArrayAt::Field("elements"))
            }
            Read::Trades => {
                params.push((
                    "symbol",
                    request.symbol.as_ref().unwrap().to_ascii_lowercase(),
                ));
                if let Some(cursor) = request.history.cursor() {
                    params.push(("lastTime", cursor.to_owned()));
                }
                ("/derivatives/api/v3/history", ArrayAt::Field("history"))
            }
        };
        let command = request.history.command();
        let form = form(&params);
        if request.read == Read::Trades {
            cx.request_streamed(
                Method::Get,
                format!("{}{path}?{form}", self.host()),
                Vec::new(),
                at,
                Pending::History(Box::new(request)),
            );
        } else if let Err(error) = self.signed_streamed(
            cx,
            Method::Get,
            path,
            form,
            at,
            Pending::History(Box::new(request)),
        ) {
            cx.fail(command, error);
        }
    }
    pub(crate) fn history_item(
        &mut self,
        _cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        text: &str,
    ) {
        if request.failed.is_some() {
            return;
        }
        let result = match request.read {
            Read::Fills => self.history_fill(request, text),
            Read::Income => self.history_ledger(request, text),
            Read::Orders => self.history_order(request, text),
            Read::Trades => self.history_trade(request, text),
        };
        if let Err(error) = result {
            request.failed = Some(error);
        }
    }
    fn history_fill(&self, request: &mut Request, text: &str) -> Result<(), String> {
        let row: FillWire<'_> = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let time =
            timestamp(&row.fill_time).ok_or("Kraken execution has an invalid ISO timestamp")?;
        let native_side = side(&row.side)?;
        if row.symbol.is_empty() || row.price <= 0.0 || row.size <= 0.0 {
            return Err("Kraken execution has invalid economics".into());
        }
        if !request.observe(time, &row.fill_id.0)? {
            return Ok(());
        }
        let symbol = row.symbol.to_ascii_uppercase();
        if !request.within(time, &symbol)
            || request
                .order
                .as_ref()
                .is_some_and(|id| row.order_id.as_ref().is_none_or(|wire| wire.0 != *id))
        {
            return Ok(());
        }
        request.history.stage(Row {
            time,
            data: Data::Execution(Execution {
                id: row.fill_id.0.into_owned(),
                timestamp: (time / 1000.0).floor(),
                symbol,
                order_id: row
                    .order_id
                    .map(|v| v.0.into_owned())
                    .filter(|s| !s.is_empty()),
                price: row.price,
                qty: row.size,
                side: native_side,
                maker: row.fill_type.eq_ignore_ascii_case("maker"),
                realized_pnl: row.realized_pnl,
            }),
        });
        Ok(())
    }
    fn history_ledger(&self, request: &mut Request, text: &str) -> Result<(), String> {
        let row: LedgerWire<'_> = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let time = timestamp(&row.date).ok_or("Kraken ledger has an invalid ISO timestamp")?;
        let id = if row.booking_uid.0.is_empty() {
            &row.id.0
        } else {
            &row.booking_uid.0
        };
        // Count native cash and position rows before filtering. Execution IDs
        // repeat across those legs and cannot be the paging identity.
        let identity = format!("{id}|{}|{}", row.asset, row.contract);
        if id.is_empty() || !request.observe(time, &identity)? {
            return if id.is_empty() {
                Err("Kraken ledger has no booking identity".into())
            } else {
                Ok(())
            };
        }
        if !row.asset.is_empty() && row.asset.eq_ignore_ascii_case(&row.contract) {
            return Ok(());
        }
        let symbol = row.contract.to_ascii_uppercase();
        if !request.within(time, &symbol) {
            return Ok(());
        }
        let info = if row.info.is_empty() {
            "unknown"
        } else {
            &row.info
        };
        let mut values = Vec::new();
        for (value, suffix, category, kind_suffix, sign) in [
            (row.realized_pnl, "pnl", "realized_pnl", "", 1.0),
            (row.realized_funding, "funding", "funding", "_funding", 1.0),
            (row.fee, "fee", "commission", "_fee", -1.0),
            (
                row.liquidation_fee,
                "liquidation",
                "liquidation",
                "_liquidation_fee",
                -1.0,
            ),
        ] {
            if let Some(value) = value.filter(|n| *n != 0.0) {
                values.push((
                    value * sign,
                    suffix,
                    category,
                    format!("{info}{kind_suffix}"),
                ));
            }
        }
        if values.is_empty() {
            // A balance delta is evidence only when both balances were reported.
            if let (Some(old), Some(new)) = (row.old_balance, row.new_balance) {
                let delta = new - old;
                if !delta.is_finite() {
                    return Err("Kraken ledger balance delta overflow".into());
                }
                if delta != 0.0 {
                    let lower = info.to_ascii_lowercase();
                    let category = if ["transfer", "conversion", "deposit", "withdraw"]
                        .iter()
                        .any(|s| lower.contains(s))
                    {
                        "transfer"
                    } else {
                        "other"
                    };
                    values.push((delta, category, category, info.into()));
                }
            }
        }
        for (income, suffix, category, income_type) in values {
            if request
                .kind
                .as_ref()
                .is_some_and(|expected| expected != &income_type)
            {
                continue;
            }
            request.history.stage(Row {
                time,
                data: Data::Income(Income {
                    symbol: symbol.clone(),
                    income_type,
                    income,
                    asset: row.asset.to_ascii_uppercase(),
                    timestamp: time,
                    tran_id: format!("{id}:{suffix}"),
                    trade_id: row
                        .execution
                        .as_ref()
                        .map(|v| v.0.to_string())
                        .filter(|s| !s.is_empty()),
                    category,
                }),
            });
        }
        Ok(())
    }
    fn history_trade(&self, request: &mut Request, text: &str) -> Result<(), String> {
        let row: TradeWire<'_> = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let time = timestamp(&row.time).ok_or("Kraken trade has an invalid ISO timestamp")?;
        let id = if row.uid.is_empty() {
            row.trade_id.0.into_owned()
        } else {
            row.uid.into_owned()
        };
        let native_side = side(&row.side)?;
        if row.price <= 0.0 || row.size <= 0.0 || !(row.price * row.size).is_finite() {
            return Err("Kraken trade has invalid economics".into());
        }
        if request.observe(time, &id)? {
            request.history.stage(Row {
                time,
                data: Data::Trade(Trade {
                    id,
                    symbol: request.symbol.clone().unwrap(),
                    price: row.price,
                    qty: row.size,
                    quote_qty: row.price * row.size,
                    timestamp: time,
                    side: native_side,
                }),
            });
        }
        Ok(())
    }
    fn history_order(&self, request: &mut Request, text: &str) -> Result<(), String> {
        let event: OrderEventWire<'_> = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if !request.observe(event.timestamp, &event.uid)? {
            return Ok(());
        }
        let (order, canceled) = match event.event {
            OrderEvent::NotFound(_) => return Ok(()),
            OrderEvent::Placed { order } => (order, false),
            OrderEvent::Updated { new_order } => (new_order, false),
            OrderEvent::Rejected { order } | OrderEvent::Cancelled { order } => (order, true),
            // A refused edit cannot change an order's accepted state.
            OrderEvent::EditRejected { .. } => return Ok(()),
        };
        let symbol = order.tradeable.to_ascii_uppercase();
        if order.uid.is_empty()
            || symbol.is_empty()
            || order.quantity < 0.0
            || (order.quantity == 0.0 && !canceled)
            || order.filled < 0.0
            || order.filled > order.quantity
            || order.timestamp < 0.0
            || order.last_update_timestamp < order.timestamp
        {
            return Err("Kraken order history has invalid identity, time or economics".into());
        }
        if !request.within(event.timestamp, &symbol) {
            return Ok(());
        }
        let status = if canceled {
            OrderStatus::Canceled
        } else if order.filled == order.quantity {
            OrderStatus::Closed
        } else {
            return Ok(());
        };
        let native_side = side(&order.direction)?;
        let order_kind = kind(&order.order_type, order.limit_price)?;
        let mapped = HistoricalOrder {
            id: order.uid.into_owned(),
            symbol,
            side: native_side,
            kind: order_kind,
            status,
            price: order.limit_price,
            amount: order.quantity,
            filled: order.filled,
            reduce_only: order.reduce_only,
            created_time: order.timestamp,
            updated_time: event.timestamp.max(order.last_update_timestamp),
        };
        request.history.stage(Row {
            time: mapped.updated_time,
            data: Data::Order(mapped),
        });
        Ok(())
    }
    pub(crate) fn history_http(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        response: Response<'_>,
    ) {
        let envelope = match serde_json::from_str::<Envelope<'_>>(response.body) {
            Ok(envelope)
                if (200..300).contains(&response.status)
                    && envelope.error.is_empty()
                    && (envelope.result.is_empty() || envelope.result == "success")
                    && (envelope.status.is_empty() || envelope.status == "success") =>
            {
                envelope
            }
            Ok(envelope) => {
                self.history_failed(
                    cx,
                    request,
                    &format!(
                        "HTTP {} {} {} {} {}",
                        response.status,
                        envelope.result,
                        envelope.status,
                        envelope.error,
                        envelope.reason
                    ),
                );
                return;
            }
            Err(error) => {
                self.history_failed(cx, request, &format!("invalid envelope: {error}"));
                return;
            }
        };
        let array_present = match request.read {
            Read::Fills => envelope.fills.is_some(),
            Read::Income => envelope.logs.is_some(),
            Read::Orders => envelope.elements.is_some(),
            Read::Trades => envelope.history.is_some(),
        };
        if !array_present {
            self.history_failed(cx, request, "required result array is absent");
            return;
        }
        if let Some(error) = request.failed.take() {
            self.history_failed(cx, request, &error);
            return;
        }
        let mut gap = None;
        let next = match request.read {
            Read::Orders => {
                if envelope.len.is_some_and(|len| len != request.count) {
                    self.history_failed(cx, request, "order page length mismatch");
                    return;
                }
                let body_cursor = envelope
                    .continuation_token
                    .as_deref()
                    .filter(|s| !s.is_empty());
                let header_cursor = response
                    .header("Next-Continuation-Token")
                    .filter(|s| !s.is_empty());
                if body_cursor.is_some() && header_cursor.is_some() && body_cursor != header_cursor
                {
                    request.history.reject(
                        cx,
                        VenueError::new("Kraken history failed: conflicting order page cursors"),
                    );
                    return;
                }
                match body_cursor.or(header_cursor) {
                    Some(cursor) => Some(cursor.to_owned()),
                    None if request.count >= ORDERS_PAGE => {
                        request.history.reject(cx, VenueError::new(
                            "Kraken history failed: INCOMPLETE: saturated order page has no continuation token"));
                        return;
                    }
                    None => None,
                }
            }
            Read::Fills => {
                if request.count < FILLS_PAGE || request.oldest < request.from {
                    None
                } else {
                    Some(iso(request.oldest.floor() + 1.0))
                }
            }
            Read::Trades => {
                if request.count < FILLS_PAGE || request.output_len() >= request.limit {
                    None
                } else {
                    Some(iso(request.oldest.floor() + 1.0))
                }
            }
            Read::Income => {
                if request.count < LEDGER_PAGE
                    || (request.descending && request.oldest < request.from)
                {
                    None
                } else if !request.ascending || request.oldest == request.newest {
                    // Only `since` reaches the venue, so a full page that cannot advance leaves
                    // rows unread. Like TS, return what was read and say it is truncated.
                    if request.output_len() < request.limit {
                        gap = Some(format!(
                            "Kraken income history is TRUNCATED: a full {LEDGER_PAGE}-row ledger page cannot be walked further with the venue's since-only cursor; some rows in the window are missing and any total is a lower bound."
                        ));
                    }
                    None
                } else if request.newest > request.to {
                    None
                } else {
                    Some(format!("{:.0}", request.newest.floor()))
                }
            }
        };
        match request.history.advance_with_coverage(
            CursorPage {
                next,
                rows: request.count,
                fresh: request.fresh,
                want_more: true,
            },
            gap,
        ) {
            CursorStep::More(_) => self.history_request(cx, request),
            CursorStep::Done => self.history_finish(cx, request, Ok(())),
            CursorStep::Stalled(reason) => request
                .history
                .reject(cx, VenueError::new(stall_message(reason))),
        }
    }
    fn history_finish(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        result: Result<(), VenueError>,
    ) {
        let limit = request.limit;
        request.history.finish_with(cx, result, move |rows| {
            let mut orders: HashMap<String, HistoricalOrder> = HashMap::new();
            let mut output = Vec::new();
            for row in rows {
                match row.data {
                    Data::Order(order) => {
                        if orders
                            .get(&order.id)
                            .is_none_or(|previous| previous.updated_time < order.updated_time)
                        {
                            orders.insert(order.id.clone(), order);
                        }
                    }
                    data => output.push(Row {
                        time: row.time,
                        data,
                    }),
                }
            }
            for (_, order) in orders {
                output.push(Row {
                    time: order.updated_time,
                    data: Data::Order(order),
                });
            }
            output.sort_by(|a, b| b.time.total_cmp(&a.time));
            output.truncate(limit);
            let rows: Vec<_> = output.into_iter().map(|row| row.data).collect();
            serde_json::to_string(&rows).map_err(|error| VenueError::new(error.to_string()))
        });
    }
    pub(crate) fn history_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        self.history_finish(
            cx,
            request,
            Err(VenueError::new(format!("Kraken history failed: {error}"))),
        );
    }
}

fn stall_message(reason: Stall) -> String {
    match reason {
        Stall::PageBudget => "Kraken history is INCOMPLETE: page budget exhausted".into(),
        _ => "Kraken history failed: INCOMPLETE: history cursor made no progress or repeated a full boundary".into(),
    }
}

/// Strict ISO-8601/RFC3339 timestamps, retaining native milliseconds for range
/// tests and page cursors before any Execution output is rounded to seconds.
pub(crate) fn timestamp(text: &str) -> Option<f64> {
    let bytes = text.as_bytes();
    if bytes.len() < 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !matches!(bytes[10], b'T' | b't')
        || bytes[13] != b':'
        || bytes[16] != b':'
    {
        return None;
    }
    let n = |from: usize, to: usize| -> Option<i64> {
        let s = bytes.get(from..to)?;
        if !s.iter().all(u8::is_ascii_digit) {
            return None;
        }
        std::str::from_utf8(s).ok()?.parse().ok()
    };
    let year = n(0, 4)?;
    let month = n(5, 7)?;
    let day = n(8, 10)?;
    let hour = n(11, 13)?;
    let minute = n(14, 16)?;
    let second = n(17, 19)?;
    let days = time::days_in_month(year, month)?;
    if day < 1 || day > days || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let mut end = 19;
    let mut fraction = 0.0;
    if bytes.get(end) == Some(&b'.') {
        end += 1;
        let start = end;
        let mut scale = 100.0;
        while let Some(byte) = bytes.get(end).filter(|b| b.is_ascii_digit()) {
            fraction += f64::from(*byte - b'0') * scale;
            scale /= 10.0;
            end += 1;
        }
        if start == end {
            return None;
        }
    }
    let offset = match bytes.get(end) {
        Some(b'Z' | b'z') if end + 1 == bytes.len() => 0,
        Some(sign @ (b'+' | b'-'))
            if end + 6 == bytes.len() && bytes.get(end + 3) == Some(&b':') =>
        {
            let h = n(end + 1, end + 3)?;
            let m = n(end + 4, end + 6)?;
            if h > 23 || m > 59 {
                return None;
            }
            (h * 60 + m) * 60 * if *sign == b'+' { 1 } else { -1 }
        }
        _ => return None,
    };
    let epoch_days = time::days_from_civil(year, month, day);
    let ms = (epoch_days * 86400 + hour * 3600 + minute * 60 + second - offset) as f64 * 1000.0
        + fraction;
    (ms >= 0.0).then_some(ms)
}
fn iso(ms: f64) -> String {
    time::iso_millis(ms as i64)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn iso_roundtrip_and_strict_calendar() {
        for date in [
            "1970-01-01T00:00:00.000Z",
            "2024-02-29T13:12:59.999Z",
            "2026-10-20T00:00:00.001Z",
        ] {
            assert_eq!(iso(timestamp(date).unwrap()), date);
        }
        assert_eq!(
            timestamp("2026-10-20T01:30:00+01:30"),
            timestamp("2026-10-20T00:00:00Z")
        );
        assert_eq!(
            timestamp("2026-10-19T22:30:00-01:30"),
            timestamp("2026-10-20T00:00:00Z")
        );
        assert!(timestamp("2025-02-29T00:00:00Z").is_none());
        assert!(timestamp("2026-10-20T24:00:00Z").is_none());
        assert!(timestamp("2026-10-20T00:00:00.Z").is_none());
        assert!(timestamp("2026-10-20T00:00:00Ztail").is_none());
    }
}
