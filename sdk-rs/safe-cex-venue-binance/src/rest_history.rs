//! Futures history/public-trade readers and same-owner live candle seed bodies.
//! Current configured HTTP/helper/pager/Date/Store/socket operations are the
//! boundary. Dense decoded graphs preserve f64/undefined/native enums/referents.
//! Caller retains launched async jobs through settlement. Concrete client/class/
//! shared-helper/date-library wiring, ignored job rejection/drop/unwind policy,
//! executable graph getters/iterators/prototypes/symbols and mutable Error data
//! are unclaimed. Map labels use a caller-supplied locale lowercase operation.
use crate::{
    private_event_value::*,
    request_weight::{Failure, type_error},
};
use safe_cex_types::{
    BothPositionSide, ExecutionPositionSide, IncomeCategory, OrderSide, PositionSide,
};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
};
const RANGE_WINDOW: f64 = 604800000.0;
const RANGE_LIMIT: f64 = 1000.0;
const RANGE_DELAY: f64 = 2000.0;
pub type HistoryFuture = Pin<Box<dyn Future<Output = Result<EventValue, Failure>>>>;
/// GET yields decoded response.data; response envelope/data getters are unclaimed.
pub type HistoryGet = Rc<dyn Fn(&'static str, EventValue) -> Result<HistoryFuture, Failure>>;
pub type PageCall = Rc<dyn Fn(EventValue) -> Result<HistoryFuture, Failure>>;
pub type PageResolver = Rc<dyn Fn() -> Result<PageCall, Failure>>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryClient {
    Private,
    Public,
}
pub use safe_cex_exchange::runtime::LogSeverity as HistorySeverity;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IncomePageMethod {
    Rethrowing,
    Swallowing,
}
pub type HistoryLog = Rc<dyn Fn(String, HistorySeverity) -> Result<(), Failure>>;
pub type HistoryEmit = Rc<dyn Fn(EventValue) -> Result<(), Failure>>;
pub trait HistoryBindings {
    fn public_only(&mut self) -> Result<bool, Failure>;
    fn capture_get(&mut self, client: HistoryClient) -> Result<HistoryGet, Failure>;
    fn capture_log(&mut self) -> Result<HistoryLog, Failure>;
    fn capture_emit(&mut self) -> Result<HistoryEmit, Failure>;
    fn split(
        &mut self,
        from: EventValue,
        to: EventValue,
        max_span: f64,
    ) -> Result<EventValue, Failure>;
    fn normalize_side(&mut self, raw: EventValue) -> Result<EventValue, Failure>;
    fn category(&mut self, raw: EventValue) -> Result<EventValue, Failure>;
    fn map_order(&mut self, raw: EventValue) -> Result<EventValue, Failure>;
    /// Non-observable same-owner handle; lookup current named method at invocation.
    fn page_resolver(&mut self, method: IncomePageMethod) -> PageResolver;
    fn paginate(&mut self, fetch: PageCall, options: EventValue) -> Result<HistoryFuture, Failure>;
    fn iso(&mut self, time: EventValue) -> Result<String, Failure>;
}
fn empty() -> EventValue {
    EventValue::Array(Rc::new(RefCell::new(vec![])))
}
fn array(v: EventValue) -> EventValue {
    if matches!(v, EventValue::Array(_)) {
        v
    } else {
        empty()
    }
}
fn min(v: f64, max: f64) -> f64 {
    if v.is_nan() {
        f64::NAN
    } else if v == 0.0 && v.is_sign_negative() {
        -0.0
    } else {
        v.min(max)
    }
}
fn map(
    v: EventValue,
    mut f: impl FnMut(EventValue) -> Result<EventValue, Failure>,
) -> Result<EventValue, Failure> {
    let EventValue::Array(a) = v else {
        unreachable!("array validated")
    };
    let len = a.borrow().len();
    let mut out = vec![];
    for i in 0..len {
        let value = { a.borrow().get(i).cloned() };
        if let Some(v) = value {
            out.push(f(v)?);
        }
    }
    Ok(EventValue::Array(Rc::new(RefCell::new(out))))
}
fn omit(v: EventValue) -> EventValue {
    EventValue::object(
        v.spread()
            .into_iter()
            .filter(|(_, v)| !matches!(v, EventValue::Undefined))
            .collect(),
    )
}
fn optional(v: &EventValue, key: &str) -> Result<EventValue, Failure> {
    if v.nullish() {
        Ok(EventValue::Undefined)
    } else {
        v.get(key)
    }
}
fn endpoint(key: &str) -> &'static str {
    crate::binance_types::ENDPOINTS
        .iter()
        .find(|(k, _)| *k == key)
        .expect("generated endpoint")
        .1
}
/// Immutable JSON error metadata adapter preserves value but not nested
/// response/message referent identity; executable metadata properties excluded.
fn err_message(e: &Failure) -> EventValue {
    let response = e
        .response
        .clone()
        .map(EventValue::from_json)
        .unwrap_or(EventValue::Undefined);
    let msg = optional(
        &optional(&response, "data").unwrap_or(EventValue::Undefined),
        "msg",
    )
    .unwrap_or(EventValue::Undefined);
    fallback(msg, string(&e.message))
}
fn execution(value: String) -> EventValue {
    match value.as_str() {
        "both" => EventValue::Enum(PrivateEnum::Execution(&ExecutionPositionSide::Both(
            BothPositionSide::Both,
        ))),
        "long" => EventValue::Enum(PrivateEnum::Execution(&ExecutionPositionSide::Side(
            PositionSide::Long,
        ))),
        "short" => EventValue::Enum(PrivateEnum::Execution(&ExecutionPositionSide::Side(
            PositionSide::Short,
        ))),
        _ => string(value),
    }
}
fn side(value: String) -> EventValue {
    match value.as_str() {
        "buy" => EventValue::Enum(PrivateEnum::Side(OrderSide::Buy)),
        "sell" => EventValue::Enum(PrivateEnum::Side(OrderSide::Sell)),
        _ => string(value),
    }
}
// ts: mapBinanceUserTrade
pub fn map_user_trade(
    entry: EventValue,
    mut lowercase: impl FnMut(String) -> Result<String, Failure>,
) -> Result<EventValue, Failure> {
    let out = record([]);
    out.define("id", entry.get("id")?.optional_id("entry.id")?);
    out.define("symbol", entry.get("symbol")?);
    out.define(
        "orderId",
        entry.get("orderId")?.optional_id("entry.orderId")?,
    );
    out.define("price", number(entry.get("price")?.number()?));
    out.define("qty", number(entry.get("qty")?.number()?));
    let q = entry.get("quoteQty")?;
    out.define(
        "quoteQty",
        if q.nullish() {
            EventValue::Undefined
        } else {
            number(q.number()?)
        },
    );
    out.define("realizedPnl", number(entry.get("realizedPnl")?.number()?));
    out.define(
        "positionSide",
        execution(lowercase(
            nullish(entry.get("positionSide")?, string("BOTH")).text()?,
        )?),
    );
    out.define(
        "side",
        side(lowercase(nullish(entry.get("side")?, string("")).text()?)?),
    );
    out.define("buyer", entry.get("buyer")?);
    out.define("maker", entry.get("maker")?);
    out.define("commission", number(entry.get("commission")?.number()?));
    out.define("commissionAsset", entry.get("commissionAsset")?);
    out.define("time", number(entry.get("time")?.number()?));
    Ok(out)
}
fn execution_row(entry: EventValue, b: &mut impl HistoryBindings) -> Result<EventValue, Failure> {
    let out = record([]);
    let id = entry.get("id")?;
    out.define(
        "id",
        if id.nullish() {
            EventValue::Undefined
        } else {
            string(id.text()?)
        },
    );
    out.define(
        "timestamp",
        number((entry.get("time")?.number()? / 1000.0).floor()),
    );
    out.define("symbol", entry.get("symbol")?);
    out.define(
        "orderId",
        entry.get("orderId")?.optional_id("entry.orderId")?,
    );
    out.define("price", number(entry.get("price")?.float()?));
    out.define("qty", number(entry.get("qty")?.float()?));
    out.define(
        "side",
        fallback(
            table(
                crate::binance_types::ORDER_SIDE,
                entry.get("side")?,
                PrivateEnum::Side,
            )?,
            EventValue::Enum(PrivateEnum::Side(OrderSide::Buy)),
        ),
    );
    out.define(
        "commission",
        number(fallback(entry.get("commission")?, string("0")).float()?),
    );
    out.define("commissionAsset", entry.get("commissionAsset")?);
    let p = entry.get("positionSide")?;
    out.define(
        "positionSide",
        if p.truthy() {
            b.normalize_side(entry.get("positionSide")?)?
        } else {
            EventValue::Undefined
        },
    );
    let pnl = entry.get("realizedPnl")?.float()?;
    out.define(
        "realizedPnl",
        if pnl.is_finite() {
            number(entry.get("realizedPnl")?.float()?)
        } else {
            EventValue::Undefined
        },
    );
    let maker = entry.get("maker")?;
    out.define(
        "maker",
        if matches!(maker, EventValue::Bool(_)) {
            entry.get("maker")?
        } else {
            EventValue::Undefined
        },
    );
    Ok(omit(out))
}
// ts: BinanceExchange.fetchHistoricalFills
pub async fn historical_fills(
    opts: EventValue,
    b: &mut impl HistoryBindings,
) -> Result<EventValue, Failure> {
    if b.public_only()? {
        return Ok(empty());
    }
    if opts.nullish() {
        return Err(type_error(format!(
            "Cannot destructure property 'symbol' of 'opts' as it is {}.",
            if matches!(opts, EventValue::Null) {
                "null"
            } else {
                "undefined"
            }
        )));
    }
    let symbol = opts.get("symbol")?;
    let order = opts.get("orderId")?;
    let from = opts.get("from")?;
    let to = opts.get("to")?;
    let raw = opts.get("limit")?;
    let limit = if matches!(raw, EventValue::Undefined) {
        number(1000.0)
    } else {
        raw
    };
    if !symbol.truthy() {
        return Ok(empty());
    }
    let result = async {
        let params = record([
            ("symbol", symbol),
            ("limit", number(min(limit.number()?, 1000.0))),
        ]);
        if order.truthy() {
            params.define("orderId", order)
        }
        let windows = if from.truthy() && to.truthy() {
            b.split(from, to, crate::utils::USER_TRADES_MAX_WINDOW_MS)?
        } else {
            EventValue::Array(Rc::new(RefCell::new(vec![record([
                ("startTime", from),
                ("endTime", to),
            ])])))
        };
        let EventValue::Array(windows) = windows else {
            return Err(type_error("windows is not iterable"));
        };
        let mut rows = vec![];
        let mut i = 0;
        loop {
            let window = windows.borrow().get(i).cloned();
            let Some(window) = window else { break };
            i += 1;
            let get = b.capture_get(HistoryClient::Private)?;
            let request = EventValue::object(params.spread());
            request.define("startTime", window.get("startTime")?);
            request.define("endTime", window.get("endTime")?);
            let data = get(endpoint("USER_TRADES"), omit(request))?.await?;
            if let EventValue::Array(a) = data {
                rows.extend(a.borrow().iter().cloned())
            }
        }
        map(EventValue::Array(Rc::new(RefCell::new(rows))), |entry| {
            execution_row(entry, b)
        })
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            let log = b.capture_log()?;
            log(
                format!("Error fetching executions: {}", e.message),
                HistorySeverity::Error,
            )?;
            Ok(empty())
        }
    }
}
// ts: BinanceExchange.fetchHistoricalOrders
pub async fn historical_orders(
    opts: EventValue,
    b: &mut impl HistoryBindings,
) -> Result<EventValue, Failure> {
    if b.public_only()? || !optional(&opts, "symbol")?.truthy() {
        return Ok(empty());
    }
    let result = async {
        let get = b.capture_get(HistoryClient::Private)?;
        let params = omit(record([
            ("symbol", opts.get("symbol")?),
            ("startTime", opts.get("from")?),
            ("endTime", opts.get("to")?),
            (
                "limit",
                number(min(
                    fallback(opts.get("limit")?, number(1000.0)).number()?,
                    1000.0,
                )),
            ),
        ]));
        let rows = get(endpoint("ALL_ORDERS"), params)?.await?;
        let rows = map(array(rows), |r| b.map_order(r))?;
        let EventValue::Array(a) = rows else {
            unreachable!()
        };
        let mut out = a
            .borrow()
            .iter()
            .filter(|r| !matches!(r, EventValue::Null))
            .cloned()
            .collect::<Vec<_>>();
        // Stable sort, with NaN comparator treated as zero by Array.sort.
        for i in 1..out.len() {
            let mut j = i;
            while j > 0 {
                let delta = out[j].get("createdTime")?.number()?
                    - out[j - 1].get("createdTime")?.number()?;
                // `!(delta > 0.0)` is NOT `delta <= 0.0`: a NaN delta (either
                // createdTime missing, so Number(undefined)) must take the
                // break, and the rewritten form would not. The source sorts
                // with this comparison; keep it.
                #[allow(clippy::neg_cmp_op_on_partial_ord)]
                if !(delta > 0.0) {
                    break;
                }
                out.swap(j, j - 1);
                j -= 1;
            }
        }
        Ok(EventValue::Array(Rc::new(RefCell::new(out))))
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            let emit = b.capture_emit()?;
            emit(err_message(&e))?;
            Ok(empty())
        }
    }
}
async fn income(opts: EventValue, b: &mut impl HistoryBindings) -> Result<EventValue, Failure> {
    let params = record([(
        "limit",
        number(min(
            fallback(optional(&opts, "limit")?, number(1000.0)).number()?,
            1000.0,
        )),
    )]);
    for key in ["symbol", "incomeType", "startTime", "endTime"] {
        if optional(&opts, key)?.truthy() {
            params.define(key, opts.get(key)?);
        }
    }
    let get = b.capture_get(HistoryClient::Private)?;
    let data = get(endpoint("INCOME"), params)?.await?;
    map(array(data), |r| {
        let out = record([]);
        out.define("symbol", r.get("symbol")?);
        out.define("incomeType", r.get("incomeType")?);
        out.define("income", number(r.get("income")?.float()?));
        out.define("asset", r.get("asset")?);
        out.define("timestamp", r.get("time")?);
        out.define("tranId", r.get("tranId")?);
        out.define("tradeId", r.get("tradeId")?);
        out.define("category", b.category(r.get("incomeType")?)?);
        Ok(out)
    })
}
// ts: BinanceExchange#_fetchIncomeHistoryPage
pub async fn income_page(
    opts: EventValue,
    b: &mut impl HistoryBindings,
) -> Result<EventValue, Failure> {
    if b.public_only()? {
        return Ok(empty());
    }
    income(opts, b).await
}
// ts: BinanceExchange.fetchIncomeHistory
pub async fn income_history(
    opts: EventValue,
    b: &mut impl HistoryBindings,
) -> Result<EventValue, Failure> {
    if b.public_only()? {
        return Ok(empty());
    }
    match income(opts, b).await {
        Ok(v) => Ok(v),
        Err(e) => {
            let log = b.capture_log()?;
            log(
                format!("Error fetching income history: {}", e.message),
                HistorySeverity::Error,
            )?;
            Ok(empty())
        }
    }
}
// ts: BinanceExchange.fetchIncomeHistoryRange
pub async fn income_range(
    start: EventValue,
    end: EventValue,
    income_type: EventValue,
    b: &mut impl HistoryBindings,
) -> Result<EventValue, Failure> {
    let resolve = b.page_resolver(IncomePageMethod::Rethrowing);
    let fetch: PageCall = Rc::new(move |p| {
        if p.nullish() {
            return Err(type_error(format!(
                "Cannot destructure property 'startTime' of '{}' as it is {}.",
                if matches!(p, EventValue::Null) {
                    "object null"
                } else {
                    "undefined"
                },
                if matches!(p, EventValue::Null) {
                    "null"
                } else {
                    "undefined"
                }
            )));
        }
        let from = p.get("startTime")?;
        let to = p.get("endTime")?;
        let limit = p.get("limit")?;
        let page = resolve()?;
        page(record([
            ("startTime", from),
            ("endTime", to),
            ("incomeType", income_type.clone()),
            ("limit", limit),
        ]))
    });
    let result = b
        .paginate(
            fetch,
            record([
                ("startTime", start.clone()),
                ("endTime", end.clone()),
                ("chunkMs", number(RANGE_WINDOW)),
                ("pageLimit", number(RANGE_LIMIT)),
                ("chunkDelayMs", number(RANGE_DELAY)),
            ]),
        )?
        .await?;
    if result.nullish() {
        return Err(type_error(format!(
            "Cannot destructure property 'records' of '(intermediate value)' as it is {}.",
            if matches!(result, EventValue::Null) {
                "null"
            } else {
                "undefined"
            }
        )));
    }
    let records = result.get("records")?;
    let truncated = result.get("truncated")?;
    let exhausted = result.get("exhaustedChunks")?;
    if truncated.truthy() {
        let log = b.capture_log()?;
        let from = b.iso(start)?;
        let to = b.iso(end)?;
        log(
            format!(
                "Income history for {from}..{to} is INCOMPLETE — {} chunk(s) could not be walked to completion. Any total derived from it is a lower bound.",
                exhausted.text()?
            ),
            HistorySeverity::Warning,
        )?;
    }
    Ok(records)
}
// ts: BinanceExchange.fetchPublicTrades
pub async fn public_trades(
    symbol: EventValue,
    limit: EventValue,
    b: &mut impl HistoryBindings,
) -> Result<EventValue, Failure> {
    let limit = if limit.truthy() {
        min(limit.number()?, 1000.0)
    } else {
        1000.0
    };
    let get = b.capture_get(HistoryClient::Public)?;
    let data = get(
        endpoint("TRADES"),
        record([("symbol", symbol.clone()), ("limit", number(limit))]),
    )?
    .await?;
    map(array(data), |r| {
        let id = r.get("id")?;
        if id.nullish() {
            return Err(type_error(format!(
                "Cannot read properties of {} (reading 'toString')",
                if matches!(id, EventValue::Null) {
                    "null"
                } else {
                    "undefined"
                }
            )));
        }
        if let EventValue::Object(o) = &id {
            if o.borrow().fields.contains_key("toString") {
                return Err(type_error("trade.id.toString is not a function"));
            }
        }
        let id = id.optional_id("trade.id")?;
        Ok(record([
            ("id", id),
            ("symbol", symbol.clone()),
            ("price", number(r.get("price")?.float()?)),
            ("qty", number(r.get("qty")?.float()?)),
            ("quoteQty", number(r.get("quoteQty")?.float()?)),
            ("timestamp", r.get("time")?),
            (
                "side",
                EventValue::Enum(PrivateEnum::Side(if r.get("isBuyerMaker")?.truthy() {
                    OrderSide::Sell
                } else {
                    OrderSide::Buy
                })),
            ),
        ]))
    })
}
/// Existing shared helpers at decoded-data boundaries. No native output is
/// re-encoded for proof; canonical enum variants are retained in the graph.
pub fn normalize_side(raw: EventValue) -> Result<EventValue, Failure> {
    let text = nullish(raw, string("")).text()?;
    let side = safe_cex_types::model_utils::normalize_execution_position_side(Some(
        &serde_json::Value::String(text),
    ))
    .map_err(|e| type_error(e.message))?;
    Ok(execution(side.into()))
}
pub fn income_category(raw: EventValue) -> Result<EventValue, Failure> {
    let text = nullish(raw, string("")).text()?;
    let category = safe_cex_types::model_utils::income_category_from_type(Some(
        &serde_json::Value::String(text),
    ))
    .map_err(|e| type_error(e.message))?;
    Ok(EventValue::Enum(PrivateEnum::Income(match category {
        IncomeCategory::RealizedPnl => &IncomeCategory::RealizedPnl,
        IncomeCategory::Funding => &IncomeCategory::Funding,
        IncomeCategory::Commission => &IncomeCategory::Commission,
        IncomeCategory::Rebate => &IncomeCategory::Rebate,
        IncomeCategory::Transfer => &IncomeCategory::Transfer,
        IncomeCategory::Liquidation => &IncomeCategory::Liquidation,
        IncomeCategory::Other => &IncomeCategory::Other,
    })))
}

pub type CandleCallback = Rc<dyn Fn(EventValue) -> Result<(), Failure>>;
pub type CandleCleanup = Rc<dyn Fn() -> Result<(), Failure>>;
pub type CandleListen = Rc<dyn Fn(EventValue, CandleCallback) -> Result<CandleCleanup, Failure>>;
pub type CandleFetch = Rc<dyn Fn(EventValue) -> Result<HistoryFuture, Failure>>;
pub type SeedFuture = Pin<Box<dyn Future<Output = Result<(), Failure>>>>;
#[derive(Clone)]
pub struct SeedOwner {
    pub disposed: Rc<dyn Fn() -> Result<bool, Failure>>,
    pub capture_log: Rc<dyn Fn() -> Result<HistoryLog, Failure>>,
}
pub trait CandleBindings {
    fn external(&mut self) -> Result<bool, Failure>;
    fn capture_external(&mut self) -> Result<CandleListen, Failure>;
    fn capture_public(&mut self) -> Result<CandleListen, Failure>;
    fn capture_fetch(&mut self) -> Result<CandleFetch, Failure>;
    /// Non-observable same-owner handle for the detached then/catch continuation.
    fn seed_owner(&mut self) -> SeedOwner;
    /// Attach/run caller-retained seed task; its final log rejection is handled by
    /// the owning runtime. Promise method lookup/attachment errors are unclaimed.
    fn launch(&mut self, task: SeedFuture) -> Result<(), Failure>;
}
// ts: BinanceExchange.listenOHLCV
pub fn listen_candle(
    opts: EventValue,
    callback: CandleCallback,
    b: &mut impl CandleBindings,
) -> Result<CandleCleanup, Failure> {
    if b.external()? {
        return b.capture_external()?(opts, callback);
    }
    let disposed = Rc::new(Cell::new(false));
    let unsubscribe = b.capture_public()?(opts.clone(), callback.clone())?;
    let fetch = b.capture_fetch()?;
    let params = EventValue::object(opts.spread());
    params.define("limit", number(1.0));
    let future = fetch(params)?;
    let owner = b.seed_owner();
    let cancelled = disposed.clone();
    let task: SeedFuture = Box::pin(async move {
        let result: Result<(), Failure> = async {
            let candles = future.await?;
            if cancelled.get() || (owner.disposed)()? {
                return Ok(());
            }
            let index = candles.get("length")?.number()? - 1.0;
            let latest = if let EventValue::String(s) = &candles {
                if index.is_finite() && index >= 0.0 {
                    s.chars()
                        .nth(index as usize)
                        .map(|c| string(c.to_string()))
                        .unwrap_or(EventValue::Undefined)
                } else {
                    EventValue::Undefined
                }
            } else {
                candles.get(&safe_cex_num::js_number_string(index))?
            };
            if latest.truthy() {
                callback(latest)?;
            }
            Ok(())
        }
        .await;
        if let Err(e) = result {
            let log = (owner.capture_log)()?;
            log(
                format!(
                    "Unable to seed Binance live candle: {}",
                    if e.message.is_empty() {
                        e.js_error_string()
                    } else {
                        e.message.clone()
                    }
                ),
                HistorySeverity::Debug,
            )?;
        }
        Ok(())
    });
    b.launch(task)?;
    Ok(Rc::new(move || {
        disposed.set(true);
        unsubscribe()
    }))
}
