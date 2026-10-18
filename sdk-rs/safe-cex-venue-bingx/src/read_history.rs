//! REST histories and polling, over configured transport and imported date/loop operations.
use crate::{
    data_reads::{BingxDataReads, ReadTransport},
    private_events::map_order,
};
use safe_cex_exchange::{
    error_reporting::ExchangeErrorInput,
    runtime::{ExchangeError, ExchangeFuture, LogSeverity},
};
use safe_cex_num::numeric_ingress::{JsNumberOutput, js_deserialize_passthrough, js_value_as_f64};
use safe_cex_num::{
    JsJsonValue, js_date_parse, js_json_stringify, js_number, js_string, js_time_clip, js_truthy,
    math_min, parse_float, parse_int,
};
use safe_cex_store::StoreUpdate;
use safe_cex_transport::RestRequest;
use safe_cex_types::{
    Candle, Execution, ExecutionsOptions, OHLCVOptions, Order, OrderSide, OrderType,
    retained_json::RetainedJsonInput,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{rc::Rc, task::Poll};
pub trait HistoryHost {
    fn now_ms(&self) -> f64;
    fn subtract_date(&self, end: f64, amount: f64, unit: &str) -> Result<f64, ExchangeError>;
    fn local_to_utc(&self, ms: f64) -> f64;
    fn is_public_data_only(&self) -> bool;
    fn loop_after(
        &self,
        delay: Option<f64>,
        callback: Rc<dyn Fn() -> ExchangeFuture<'static, ()>>,
    ) -> Result<(), ExchangeError>;
}
pub struct BingxReadHistory {
    transport: Rc<dyn ReadTransport>,
    pub reads: Rc<BingxDataReads>,
    pub host: Rc<dyn HistoryHost>,
}
#[derive(Serialize)]
#[serde(transparent)]
struct Response {
    value: Value,
}
impl JsNumberOutput for Response {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{} passthrough{value});
    }
}
fn error(message: impl Into<String>) -> ExchangeError {
    let mut e = ExchangeError::new(message);
    e.name = "TypeError".into();
    e
}
fn text(v: Option<&Value>) -> Result<String, ExchangeError> {
    js_string(v).map_err(|e| error(e.message))
}
fn float(v: Option<&Value>) -> Result<f64, ExchangeError> {
    Ok(parse_float(&text(v)?))
}
fn number(v: Option<&Value>) -> Result<f64, ExchangeError> {
    js_number(v).map_err(|e| error(e.message))
}
fn truthy(n: f64) -> bool {
    n != 0.0 && !n.is_nan()
}
fn json_number(n: f64) -> Result<Value, ExchangeError> {
    RetainedJsonInput::Float(n)
        .into_value()
        .map_err(|_| error("BingX typed history query refuses NaN"))
}
fn pick<'a>(row: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter()
        .find_map(|k| row.get(*k).filter(|v| js_truthy(Some(v))))
}
fn stringify(v: &Value) -> Result<String, ExchangeError> {
    js_json_stringify(&JsJsonValue::from(v.clone()))
        .map_err(|e| error(e.message))?
        .ok_or_else(|| error("undefined history log"))
}
pub fn timeframe_name(t: safe_cex_types::AnyTimeframe) -> &'static str {
    match t {
        safe_cex_types::AnyTimeframe::Variant0(safe_cex_types::SecondsTimeframe::S1) => "1s",
        safe_cex_types::AnyTimeframe::Variant0(safe_cex_types::SecondsTimeframe::S5) => "5s",
        safe_cex_types::AnyTimeframe::Variant0(safe_cex_types::SecondsTimeframe::S15) => "15s",
        safe_cex_types::AnyTimeframe::Variant0(safe_cex_types::SecondsTimeframe::S30) => "30s",
        safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::M1) => "1m",
        safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::M3) => "3m",
        safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::M5) => "5m",
        safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::M15) => "15m",
        safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::M30) => "30m",
        safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::H1) => "1h",
        safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::H2) => "2h",
        safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::H4) => "4h",
        safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::H6) => "6h",
        safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::H12) => "12h",
        safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::D1) => "1d",
        safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::W1) => "1w",
        safe_cex_types::AnyTimeframe::Variant2(safe_cex_types::ExtendedTimeframe::H3) => "3h",
        safe_cex_types::AnyTimeframe::Variant2(safe_cex_types::ExtendedTimeframe::H5) => "5h",
        safe_cex_types::AnyTimeframe::Variant2(safe_cex_types::ExtendedTimeframe::H7) => "7h",
        safe_cex_types::AnyTimeframe::Variant2(safe_cex_types::ExtendedTimeframe::H8) => "8h",
        safe_cex_types::AnyTimeframe::Variant2(safe_cex_types::ExtendedTimeframe::H9) => "9h",
        safe_cex_types::AnyTimeframe::Variant2(safe_cex_types::ExtendedTimeframe::H10) => "10h",
        safe_cex_types::AnyTimeframe::Variant2(safe_cex_types::ExtendedTimeframe::H11) => "11h",
        safe_cex_types::AnyTimeframe::Variant2(safe_cex_types::ExtendedTimeframe::H24) => "24h",
        safe_cex_types::AnyTimeframe::Variant2(safe_cex_types::ExtendedTimeframe::D2) => "2d",
        safe_cex_types::AnyTimeframe::Variant2(safe_cex_types::ExtendedTimeframe::D3) => "3d",
        safe_cex_types::AnyTimeframe::Variant2(safe_cex_types::ExtendedTimeframe::MN1) => "1M",
    }
}

pub fn candle(row: &Value) -> Result<Candle, ExchangeError> {
    if row.is_null() {
        return Err(error("Cannot read properties of null (reading 'time')"));
    }
    Ok(Candle {
        timestamp: number(row.get("time"))? / 1000.0,
        open: float(row.get("open"))?,
        high: float(row.get("high"))?,
        low: float(row.get("low"))?,
        close: float(row.get("close"))?,
        volume: float(row.get("volume"))?,
        last_side: None,
    })
}
impl BingxReadHistory {
    pub fn new(
        reads: Rc<BingxDataReads>,
        transport: Rc<dyn ReadTransport>,
        host: Rc<dyn HistoryHost>,
    ) -> Rc<Self> {
        Rc::new(Self {
            reads,
            transport,
            host,
        })
    }
    fn report(&self, message: String) -> Result<(), ExchangeError> {
        self.reads
            .base
            .emit_and_log_error(ExchangeErrorInput::Message(message), None, None)
    }
    fn log(&self, message: String, severity: LogSeverity) -> Result<(), ExchangeError> {
        self.reads.base.log(&message, severity.as_str())
    }
    async fn request(&self, url: &str, params: Option<Value>) -> Result<Value, ExchangeError> {
        let result = self
            .transport
            .send(RestRequest {
                method: "GET".into(),
                url: url.into(),
                params,
                ..Default::default()
            })
            .await;
        let mut yielded = false;
        std::future::poll_fn(|cx| {
            if yielded {
                Poll::Ready(())
            } else {
                yielded = true;
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        })
        .await;
        result?
            .decode_with(|d| js_deserialize_passthrough(d).map(|value| Response { value }))
            .map(|r| r.value)
            .map_err(|e| error(e.to_string()))
    }
    pub async fn fetch_orders(&self) -> Result<Vec<Order>, ExchangeError> {
        let data = self.request(crate::constants::ENDPOINTS[9].1, None).await?;
        if data.is_null() {
            return Err(error("Cannot read properties of null (reading 'code')"));
        }
        if js_truthy(data.get("code")) {
            self.report(text(data.get("msg"))?)?;
            return Ok(self.reads.base.store.orders().to_vec());
        }
        let content = data.get("data");
        let rows = content
            .and_then(|v| v.get("orders"))
            .and_then(Value::as_array)
            .or_else(|| content.and_then(Value::as_array));
        let mut orders = Vec::new();
        for raw in rows.into_iter().flatten() {
            if let Some(mut order) = map_order(
                raw,
                &self.reads.base.store.orders(),
                safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms(),
            )? {
                if order.r#type == OrderType::StopLoss {
                    let mark = self
                        .reads
                        .base
                        .store
                        .tickers_by_symbol()
                        .get(&order.symbol)
                        .map(|t| t.mark);
                    if mark.is_some_and(|m| {
                        m.is_finite()
                            && m > 0.0
                            && ((order.side == OrderSide::Buy && order.price.to_number() < m)
                                || (order.side == OrderSide::Sell && order.price.to_number() > m))
                    }) {
                        order.r#type = (OrderType::TakeProfit).into();
                        order.tp_trigger_type = order.sl_trigger_type;
                    }
                }
                orders.push(order);
            }
        }
        Ok(orders)
    }
    pub async fn fetch_ohlcv(&self, opts: &OHLCVOptions) -> Result<Vec<Candle>, ExchangeError> {
        let interval = timeframe_name(opts.interval.clone());
        let (digits, unit) = interval.split_at(
            interval
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(interval.len()),
        );
        let amount = parse_float(digits);
        let end = js_time_clip(
            opts.to
                .filter(|n| truthy(*n))
                .unwrap_or_else(|| self.host.now_ms()),
        );
        let start = if !opts.limit.is_some_and(truthy) && opts.from.is_some_and(truthy) {
            js_time_clip(opts.from.unwrap())
        } else {
            self.host.subtract_date(
                end,
                amount * math_min(opts.limit.filter(|n| truthy(*n)).unwrap_or(500.0), 1500.0),
                unit,
            )?
        };
        let market = self
            .reads
            .base
            .store
            .markets_by_symbol()
            .get(&opts.symbol)
            .cloned();
        let Some(market) = market else {
            self.report(format!("Market {} not found on BingX", opts.symbol))?;
            return Ok(vec![]);
        };
        let result = async {
            let params = json!({"symbol":market.id,"interval":interval,"startTime":json_number(start)?,"endTime":json_number(end)?});
            let response = self
                .request(crate::constants::ENDPOINTS[15].1, Some(params))
                .await?;
            if response.is_null() {
                return Err(error("Cannot read properties of null (reading 'code')"));
            }
            let field = |key| response.as_object().and_then(|object| object.get(key));
            // Strict `code !== undefined && code !== 0`: "0" and null are failures.
            if let Some(code) = field("code")
                && code.as_f64() != Some(0.0)
            {
                let msg = field("msg");
                let reason = if js_truthy(msg) {
                    text(msg)?
                } else {
                    format!("code {}", text(Some(code))?)
                };
                self.log(
                    format!("BingX kline request failed: {reason}"),
                    LogSeverity::Warning,
                )?;
                return Ok(vec![]);
            }
            let Some(rows) = field("data").and_then(Value::as_array) else {
                self.log(
                    format!("BingX kline response did not include candles for {}", opts.symbol),
                    LogSeverity::Warning,
                )?;
                return Ok(vec![]);
            };
            let mut candles = rows
                .iter()
                .map(|row| {
                    if row.is_null() {
                        Err(error(
                            "Cannot destructure property 'time' of 'object null' as it is null.",
                        ))
                    } else {
                        candle(row)
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            candles.sort_by(|a, b| {
                (a.timestamp - b.timestamp)
                    .partial_cmp(&0.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            Ok::<_, ExchangeError>(candles)
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => {
                let msg = e
                    .response
                    .as_ref()
                    .and_then(|v| v.get("data"))
                    .and_then(|v| v.get("msg"));
                self.report(if js_truthy(msg) {
                    text(msg)?
                } else {
                    e.message
                })?;
                Ok(vec![])
            }
        }
    }
    pub async fn fetch_historical_fills(
        &self,
        opts: &ExecutionsOptions,
    ) -> Result<Vec<Execution>, ExchangeError> {
        if self.host.is_public_data_only() {
            return Ok(vec![]);
        }
        let Some(symbol) = opts.symbol.as_ref().filter(|s| !s.is_empty()) else {
            return Ok(vec![]);
        };
        let mut executions = Vec::new();
        let result = async {
            let market = self
                .reads
                .base
                .store
                .markets_by_symbol()
                .get(symbol)
                .cloned();
            let native = match market.filter(|m| !m.id.is_empty()) {
                Some(m) => m.id,
                None => venue_symbol(&self.reads, symbol),
            };
            let mut params = json!({"symbol":native});
            let limit = opts.limit.unwrap_or(100.0);
            if truthy(limit) {
                params["limit"] = json_number(math_min(limit, 500.0))?;
            }
            if let Some(id) = opts.order_id.as_ref().filter(|s| !s.is_empty()) {
                params["orderId"] = json!(id);
            }
            for (key, value) in [("startTime", opts.from), ("endTime", opts.to)] {
                if let Some(n) = value.filter(|n| truthy(*n)) {
                    params[key] = json_number(n)?;
                }
            }
            self.log(
                format!(
                    "fetchHistoricalFills params: {}",
                    js_json_stringify(&JsJsonValue::Object(
                        ["symbol", "limit", "orderId", "startTime", "endTime"]
                            .into_iter()
                            .filter_map(|key| params
                                .get(key)
                                .map(|v| (key.into(), JsJsonValue::from(v.clone()))))
                            .collect()
                    ))
                    .map_err(|e| error(e.message))?
                    .ok_or_else(|| error("undefined history params"))?
                ),
                LogSeverity::Debug,
            )?;
            let response = self
                .request(crate::constants::ENDPOINTS[18].1, Some(params))
                .await?;
            self.log(
                format!("fetchHistoricalFills response: {}", stringify(&response)?),
                LogSeverity::Debug,
            )?;
            if response.is_null() {
                return Err(error("Cannot read properties of null (reading 'code')"));
            }
            if response.get("code").and_then(js_value_as_f64) != Some(0.0) {
                self.log(
                    format!("Failed to fetch executions: {}", text(response.get("msg"))?),
                    LogSeverity::Warning,
                )?;
                return Ok(());
            }
            let data = response.get("data");
            let rows = data
                .and_then(|v| v.get("fill_history_orders"))
                .and_then(Value::as_array)
                .or_else(|| {
                    data.and_then(|v| v.get("fill_orders"))
                        .and_then(Value::as_array)
                })
                .or_else(|| data.and_then(Value::as_array));
            let rows = rows.map_or(&[][..], Vec::as_slice);
            self.log(
                format!("fetchHistoricalFills found {} fills", rows.len()),
                LogSeverity::Debug,
            )?;
            for fill in rows {
                if fill.is_null() {
                    return Err(error(
                        "Cannot read properties of null (reading 'filledTime')",
                    ));
                }
                let time = pick(fill, &["filledTime", "filledTm", "time"]);
                let timestamp =
                    if let Some(t) = time.and_then(Value::as_str).filter(|s| s.contains('T')) {
                        (js_date_parse(t, |ms| self.host.local_to_utc(ms)) / 1000.0).floor()
                    } else {
                        (parse_int(
                            &time.map_or_else(
                                || Ok(safe_cex_num::js_number_string(self.host.now_ms())),
                                |v| text(Some(v)),
                            )?,
                            None,
                        ) / 1000.0)
                            .floor()
                    };
                let fill_symbol = match fill.get("symbol") {
                    None | Some(Value::Null) => symbol.clone(),
                    Some(Value::String(s)) => {
                        let s = s.replace('-', "");
                        if s.is_empty() { symbol.clone() } else { s }
                    }
                    _ => return Err(error("fill.symbol?.replace is not a function")),
                };
                let id = fill
                    .get("orderId")
                    .filter(|v| js_truthy(Some(v)))
                    .or_else(|| fill.get("order_id"));
                let zero = json!("0");
                let price = float(pick(fill, &["price", "filledPrice"]).or(Some(&zero)))?;
                let qty = float(pick(fill, &["volume", "qty", "filledQty"]).or(Some(&zero)))?;
                let empty = json!("");
                let side = if text(
                    fill.get("side")
                        .filter(|v| js_truthy(Some(v)))
                        .or(Some(&empty)),
                )?
                .to_uppercase()
                    == "BUY"
                {
                    OrderSide::Buy
                } else {
                    OrderSide::Sell
                };
                let commission = float(pick(fill, &["commission", "fee"]).or(Some(&zero)))?.abs();
                let id = id
                    .map(|v| {
                        v.as_str()
                            .map(str::to_owned)
                            .ok_or_else(|| error("BingX typed Execution orderId requires a string"))
                    })
                    .transpose()?;
                let asset = pick(fill, &["commissionAsset", "currency"])
                    .map(|v| {
                        v.as_str().map(str::to_owned).ok_or_else(|| {
                            error("BingX typed Execution commissionAsset requires a string")
                        })
                    })
                    .transpose()?
                    .unwrap_or_else(|| "USDT".into());
                executions.push(Execution {
                    id: None,
                    timestamp,
                    symbol: fill_symbol,
                    order_id: id,
                    price,
                    qty,
                    side,
                    commission: Some(commission),
                    commission_asset: Some(asset),
                    position_side: None,
                    realized_pnl: None,
                    maker: None,
                    venue_reported_revenue: None,
                    venue_reported_revenue_asset: None,
                });
            }
            Ok::<_, ExchangeError>(())
        }
        .await;
        if let Err(e) = result {
            let msg = e
                .response
                .as_ref()
                .and_then(|v| v.get("data"))
                .and_then(|v| v.get("msg"));
            let message = if js_truthy(msg) {
                text(msg)?
            } else if !e.message.is_empty() {
                e.message
            } else {
                e.js_error_string()
            };
            self.log(
                format!("Failed to fetch executions: {message}"),
                LogSeverity::Warning,
            )?;
        }
        Ok(executions)
    }
    pub async fn tick(self: &Rc<Self>, count: f64) -> Result<(), ExchangeError> {
        if self.reads.base.is_disposed() {
            return Ok(());
        }
        let result = async {
            if (count + 2.0) % 4.0 == 0.0 {
                let orders = self.fetch_orders().await?;
                if self.reads.base.is_disposed() {
                    return Ok(());
                }
                let mut loaded = self.reads.base.store.loaded().clone();
                loaded.orders = true;
                self.reads
                    .base
                    .store
                    .update(
                        StoreUpdate {
                            orders: Some(orders),
                            loaded: Some(loaded),
                            ..Default::default()
                        },
                        false,
                    )
                    .map_err(ExchangeError::from)?;
            }
            if count % 4.0 == 0.0 {
                let balance = self.reads.fetch_balance().await?;
                if self.reads.base.is_disposed() {
                    return Ok(());
                }
                let mut loaded = self.reads.base.store.loaded().clone();
                loaded.balance = true;
                self.reads
                    .base
                    .store
                    .update(
                        StoreUpdate {
                            balance: Some((*balance).clone()),
                            loaded: Some(loaded),
                            ..Default::default()
                        },
                        false,
                    )
                    .map_err(ExchangeError::from)?;
            }
            let positions = self.reads.fetch_positions(None).await?;
            if self.reads.base.is_disposed() {
                return Ok(());
            }
            self.reads
                .base
                .store
                .add_or_update_positions_full(positions)
                .map_err(ExchangeError::from)?;
            Ok::<_, ExchangeError>(())
        }
        .await;
        if let Err(e) = result {
            self.report(e.message)?;
        }
        if !self.reads.base.is_disposed() {
            let weak = Rc::downgrade(self);
            self.host.loop_after(
                self.reads.base.get_tick_interval(),
                Rc::new(move || {
                    let owner = weak.upgrade();
                    Box::pin(async move {
                        owner
                            .ok_or_else(|| {
                                ExchangeError::new("BingX history polling owner released")
                            })?
                            .tick(count + 1.0)
                            .await
                    })
                }),
            )?;
        }
        Ok(())
    }
}
pub fn dispose_exchange<E>(
    base: impl FnOnce() -> Result<(), E>,
    public: impl FnOnce() -> Result<(), E>,
    private: impl FnOnce() -> Result<(), E>,
) -> Result<(), E> {
    base()?;
    public()?;
    private()
}

pub(crate) fn venue_symbol(reads: &BingxDataReads, symbol: &str) -> String {
    let by_symbol = reads
        .base
        .store
        .markets_by_symbol()
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect::<Vec<_>>();
    let by_id = reads
        .base
        .store
        .markets_by_id()
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect::<Vec<_>>();
    crate::market_utils::to_bingx_symbol(symbol, &by_symbol, &by_id)
}
