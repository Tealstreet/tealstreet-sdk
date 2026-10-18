//! Futures history/public-trade readers and same-owner live candle seed bodies.
//! Current configured HTTP/helper/pager/Date/Store/socket operations are the
//! boundary. Dense decoded graphs preserve f64/undefined/native enums/referents.
//! Caller retains launched async jobs through settlement. Concrete client/class/
//! shared-helper/date-library wiring, ignored job rejection/drop/unwind policy,
//! executable graph getters/iterators/prototypes/symbols and mutable Error data
//! are unclaimed. Map labels use a caller-supplied locale lowercase operation.
use crate::{
    private_event_value::{
        EventOps, EventValue, PrivateEnum, array_values, fallback, nullish, number, record, string,
        table,
    },
    request_weight::{Failure, type_error},
};
use safe_cex_types::{
    BothPositionSide, ExecutionPositionSide, IncomeCategory, OrderSide, PositionSide,
};
use std::{cell::Cell, future::Future, pin::Pin, rc::Rc};
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
    EventValue::array(vec![])
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
        let value = { a.borrow().get(i).cloned().flatten() };
        if let Some(v) = value {
            out.push(f(v)?);
        }
    }
    Ok(EventValue::array(out))
}
fn omit(v: EventValue) -> EventValue {
    EventValue::record_object(
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
        v.read(key)
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
        .retained_response_input()
        .clone()
        .map(safe_cex_exchange::js_runtime::value::from_retained_input)
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
        "both" => EventValue::enum_value(PrivateEnum::Execution(&ExecutionPositionSide::Both(
            BothPositionSide::Both,
        ))),
        "long" => EventValue::enum_value(PrivateEnum::Execution(&ExecutionPositionSide::Side(
            PositionSide::Long,
        ))),
        "short" => EventValue::enum_value(PrivateEnum::Execution(&ExecutionPositionSide::Side(
            PositionSide::Short,
        ))),
        _ => string(value),
    }
}
fn side(value: String) -> EventValue {
    match value.as_str() {
        "buy" => EventValue::enum_value(PrivateEnum::Side(OrderSide::Buy)),
        "sell" => EventValue::enum_value(PrivateEnum::Side(OrderSide::Sell)),
        _ => string(value),
    }
}
// ts: mapBinanceUserTrade
pub fn map_user_trade(
    entry: EventValue,
    mut lowercase: impl FnMut(String) -> Result<String, Failure>,
) -> Result<EventValue, Failure> {
    let out = record([]);
    out.write("id", entry.read("id")?.optional_id("entry.id")?);
    out.write("symbol", entry.read("symbol")?);
    out.write(
        "orderId",
        entry.read("orderId")?.optional_id("entry.orderId")?,
    );
    out.write("price", number(entry.read("price")?.number()?));
    out.write("qty", number(entry.read("qty")?.number()?));
    let q = entry.read("quoteQty")?;
    out.write(
        "quoteQty",
        if q.nullish() {
            EventValue::Undefined
        } else {
            number(q.number()?)
        },
    );
    out.write("realizedPnl", number(entry.read("realizedPnl")?.number()?));
    out.write(
        "positionSide",
        execution(lowercase(
            nullish(entry.read("positionSide")?, string("BOTH")).text()?,
        )?),
    );
    out.write(
        "side",
        side(lowercase(nullish(entry.read("side")?, string("")).text()?)?),
    );
    out.write("buyer", entry.read("buyer")?);
    out.write("maker", entry.read("maker")?);
    out.write("commission", number(entry.read("commission")?.number()?));
    out.write("commissionAsset", entry.read("commissionAsset")?);
    out.write("time", number(entry.read("time")?.number()?));
    Ok(out)
}
fn execution_row(entry: EventValue, b: &mut impl HistoryBindings) -> Result<EventValue, Failure> {
    let out = record([]);
    let id = entry.read("id")?;
    out.write(
        "id",
        if id.nullish() {
            EventValue::Undefined
        } else {
            string(id.text()?)
        },
    );
    out.write(
        "timestamp",
        number((entry.read("time")?.number()? / 1000.0).floor()),
    );
    out.write("symbol", entry.read("symbol")?);
    out.write(
        "orderId",
        entry.read("orderId")?.optional_id("entry.orderId")?,
    );
    out.write("price", number(entry.read("price")?.float()?));
    out.write("qty", number(entry.read("qty")?.float()?));
    out.write(
        "side",
        fallback(
            table(
                crate::binance_types::ORDER_SIDE,
                entry.read("side")?,
                PrivateEnum::Side,
            )?,
            EventValue::enum_value(PrivateEnum::Side(OrderSide::Buy)),
        ),
    );
    out.write(
        "commission",
        number(fallback(entry.read("commission")?, string("0")).float()?),
    );
    out.write("commissionAsset", entry.read("commissionAsset")?);
    let p = entry.read("positionSide")?;
    out.write(
        "positionSide",
        if p.truthy() {
            b.normalize_side(entry.read("positionSide")?)?
        } else {
            EventValue::Undefined
        },
    );
    let pnl = entry.read("realizedPnl")?.float()?;
    out.write(
        "realizedPnl",
        if pnl.is_finite() {
            number(entry.read("realizedPnl")?.float()?)
        } else {
            EventValue::Undefined
        },
    );
    let maker = entry.read("maker")?;
    out.write(
        "maker",
        if matches!(maker, EventValue::Bool(_)) {
            entry.read("maker")?
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
    let symbol = opts.read("symbol")?;
    let order = opts.read("orderId")?;
    let from = opts.read("from")?;
    let to = opts.read("to")?;
    let raw = opts.read("limit")?;
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
            params.write("orderId", order)
        }
        let windows = if from.truthy() && to.truthy() {
            b.split(from, to, crate::utils::USER_TRADES_MAX_WINDOW_MS)?
        } else {
            EventValue::array(vec![record([("startTime", from), ("endTime", to)])])
        };
        let EventValue::Array(windows) = windows else {
            return Err(type_error("windows is not iterable"));
        };
        let mut rows = vec![];
        let mut i = 0;
        loop {
            let window = windows.borrow().get(i).cloned().flatten();
            let Some(window) = window else { break };
            i += 1;
            let get = b.capture_get(HistoryClient::Private)?;
            let request = EventValue::record_object(params.spread());
            request.write("startTime", window.read("startTime")?);
            request.write("endTime", window.read("endTime")?);
            let data = get(endpoint("USER_TRADES"), omit(request))?.await?;
            if let EventValue::Array(a) = data {
                rows.extend(array_values(&a))
            }
        }
        map(EventValue::array(rows), |entry| execution_row(entry, b))
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
            ("symbol", opts.read("symbol")?),
            ("startTime", opts.read("from")?),
            ("endTime", opts.read("to")?),
            (
                "limit",
                number(min(
                    fallback(opts.read("limit")?, number(1000.0)).number()?,
                    1000.0,
                )),
            ),
        ]));
        let rows = get(endpoint("ALL_ORDERS"), params)?.await?;
        let rows = map(array(rows), |r| b.map_order(r))?;
        let EventValue::Array(a) = rows else {
            unreachable!()
        };
        let mut out = array_values(&a)
            .into_iter()
            .filter(|r| !matches!(r, EventValue::Null))
            .collect::<Vec<_>>();
        // Stable sort, with NaN comparator treated as zero by Array.sort.
        for i in 1..out.len() {
            let mut j = i;
            while j > 0 {
                let delta = out[j].read("createdTime")?.number()?
                    - out[j - 1].read("createdTime")?.number()?;
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
        Ok(EventValue::array(out))
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
            params.write(key, opts.get(key)?);
        }
    }
    let get = b.capture_get(HistoryClient::Private)?;
    let data = get(endpoint("INCOME"), params)?.await?;
    map(array(data), |r| {
        let out = record([]);
        out.write("symbol", r.read("symbol")?);
        out.write("incomeType", r.read("incomeType")?);
        out.write("income", number(r.read("income")?.float()?));
        out.write("asset", r.read("asset")?);
        out.write("timestamp", r.read("time")?);
        out.write("tranId", r.read("tranId")?);
        out.write("tradeId", r.read("tradeId")?);
        out.write("category", b.category(r.read("incomeType")?)?);
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
        let from = p.read("startTime")?;
        let to = p.read("endTime")?;
        let limit = p.read("limit")?;
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
    let records = result.read("records")?;
    let truncated = result.read("truncated")?;
    let exhausted = result.read("exhaustedChunks")?;
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
        let id = r.read("id")?;
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
        if matches!(id, EventValue::Object(_)) {
            if id.has_own("toString") {
                return Err(type_error("trade.id.toString is not a function"));
            }
        }
        let id = id.optional_id("trade.id")?;
        Ok(record([
            ("id", id),
            ("symbol", symbol.clone()),
            ("price", number(r.read("price")?.float()?)),
            ("qty", number(r.read("qty")?.float()?)),
            ("quoteQty", number(r.read("quoteQty")?.float()?)),
            ("timestamp", r.read("time")?),
            (
                "side",
                EventValue::enum_value(PrivateEnum::Side(if r.read("isBuyerMaker")?.truthy() {
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
    Ok(EventValue::enum_value(PrivateEnum::Income(
        match category {
            IncomeCategory::RealizedPnl => &IncomeCategory::RealizedPnl,
            IncomeCategory::Funding => &IncomeCategory::Funding,
            IncomeCategory::Commission => &IncomeCategory::Commission,
            IncomeCategory::Rebate => &IncomeCategory::Rebate,
            IncomeCategory::Transfer => &IncomeCategory::Transfer,
            IncomeCategory::Liquidation => &IncomeCategory::Liquidation,
            IncomeCategory::Other => &IncomeCategory::Other,
        },
    )))
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
    let params = EventValue::record_object(opts.spread());
    params.write("limit", number(1.0));
    let future = fetch(params)?;
    let owner = b.seed_owner();
    let cancelled = disposed.clone();
    let task: SeedFuture = Box::pin(async move {
        let result: Result<(), Failure> = async {
            let candles = future.await?;
            if cancelled.get() || (owner.disposed)()? {
                return Ok(());
            }
            let index = candles.read("length")?.number()? - 1.0;
            let latest = if let EventValue::String(s) = &candles {
                if index.is_finite() && index >= 0.0 {
                    EventValue::String(s.clone())
                        .text()?
                        .chars()
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
