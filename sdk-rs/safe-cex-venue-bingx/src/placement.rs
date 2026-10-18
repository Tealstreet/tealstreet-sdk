//! Placement bodies over shared Store/guard/mode helpers and configured HTTP.
use crate::{
    data_reads::{BingxDataReads, ReadTransport},
    private_events::map_order,
};
use safe_cex_exchange::{
    error_reporting::ExchangeErrorInput,
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_num::{
    adjust, js_number_string, js_string, js_truthy, math_max, math_min, math_round,
};
use safe_cex_transport::{RestBody, RestRequest};
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use safe_cex_types::{
    OrderSide, OrderStatus, OrderStopTriggerType, OrderTimeInForce, OrderType, PositionMode,
    PositionSide, PreparedOrderOpts, retained_json::RetainedJsonInput,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{rc::Rc, task::Poll};

pub fn create_client_order_id(uuid: &str) -> String {
    format!("ts{uuid}")
        .to_lowercase()
        .chars()
        .take(40)
        .collect()
}
/// Imported UUID operation; venue formatting/submission bodies remain local.
pub trait PlacementHost {
    fn uuid(&self) -> Result<String, ExchangeError>;
}
pub struct BingxPlacement {
    pub reads: Rc<BingxDataReads>,
    pub unlimited: Rc<dyn ReadTransport>,
    pub host: Rc<dyn PlacementHost>,
}
fn numeric(n: f64) -> Result<Value, ExchangeError> {
    RetainedJsonInput::Float(n)
        .into_value()
        .map_err(|_| ExchangeError::new("BingX typed payload refuses NaN"))
}
fn put_number(value: &mut Value, key: &str, n: f64) -> Result<(), ExchangeError> {
    value[key] = numeric(n)?;
    Ok(())
}
fn error_text(error: &ExchangeError) -> Result<String, ExchangeError> {
    for value in [
        error
            .response
            .as_ref()
            .and_then(|r| r.get("data"))
            .and_then(|v| v.get("msg")),
        error.response.as_ref().and_then(|r| r.get("msg")),
        error.original_error.as_ref().and_then(|r| r.get("msg")),
    ] {
        if js_truthy(value) {
            return js_string(value).map_err(|e| ExchangeError::new(e.message));
        }
    }
    if !error.message.is_empty() {
        Ok(error.message.clone())
    } else {
        Ok("{}".into())
    }
}
fn kind(kind: safe_cex_types::ModelField<OrderType>) -> Option<&'static str> {
    match kind.into_value() {
        Some(OrderType::Conditional) | None => None,
        Some(OrderType::Limit) => Some("LIMIT"),
        Some(OrderType::Market) => Some("MARKET"),
        Some(OrderType::StopLoss) => Some("STOP_MARKET"),
        Some(OrderType::TakeProfit) => Some("TAKE_PROFIT_MARKET"),
        Some(OrderType::TrailingStopLoss) => Some("TRAILING_STOP_MARKET"),
    }
}
fn side(side: safe_cex_types::ModelField<OrderSide>) -> Option<&'static str> {
    match side.into_value() {
        Some(OrderSide::Buy) => Some("BUY"),
        Some(OrderSide::Sell) => Some("SELL"),
        None => None,
    }
}
fn tif(tif: OrderTimeInForce) -> &'static str {
    match tif {
        OrderTimeInForce::GoodTillCancel => "GTC",
        OrderTimeInForce::ImmediateOrCancel => "IOC",
        OrderTimeInForce::FillOrKill => "FOK",
        OrderTimeInForce::PostOnly => "PostOnly",
    }
}
fn truthy_number(n: f64) -> bool {
    n != 0.0 && !n.is_nan()
}
impl BingxPlacement {
    pub fn new(
        reads: Rc<BingxDataReads>,
        unlimited: Rc<dyn ReadTransport>,
        host: Rc<dyn PlacementHost>,
    ) -> Rc<Self> {
        Rc::new(Self {
            reads,
            unlimited,
            host,
        })
    }
    fn report(&self, message: String, label: Option<&str>) -> Result<(), ExchangeError> {
        self.reads
            .base
            .emit_and_log_error(ExchangeErrorInput::Message(message), label, None)
    }
    async fn post(&self, url: &str, payload: Value) -> Result<Input, ExchangeError> {
        let response = self
            .unlimited
            .send(RestRequest {
                method: "POST".into(),
                url: url.into(),
                body: Some(RestBody::Untransformed(
                    safe_cex_types::retained_json::RetainedJsonInput::from_value(payload)
                        .map_err(|error| ExchangeError::new(error.to_string()))?,
                )),
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
        response?
            .decode_with(Input::deserialize)
            .map_err(|e| ExchangeError::new(e.to_string()))
            .and_then(|r| {
                if r.is_null() {
                    Err(ExchangeError::new(
                        "Cannot read properties of null (reading 'code')",
                    ))
                } else {
                    Ok(r)
                }
            })
    }
    pub async fn get_order_position_side(
        &self,
        opts: &PreparedOrderOpts,
        override_mode: Option<PositionMode>,
    ) -> Result<String, ExchangeError> {
        let mode = match override_mode {
            Some(mode) => mode,
            None => self.reads.get_or_fetch_position_mode(false).await?,
        };
        let mut value = "BOTH";
        if mode == PositionMode::Hedged {
            value = if opts.side == OrderSide::Buy {
                "LONG"
            } else {
                "SHORT"
            };
            if matches!(
                opts.r#type.into_value(),
                Some(OrderType::StopLoss | OrderType::TakeProfit | OrderType::TrailingStopLoss)
            ) || opts.reduce_only == Some(true)
            {
                value = if value == "LONG" { "SHORT" } else { "LONG" };
            }
        }
        Ok(value.into())
    }
    pub async fn format_create_trailing_stop_loss_order(
        &self,
        opts: &PreparedOrderOpts,
    ) -> Result<Vec<Value>, ExchangeError> {
        let market = self
            .reads
            .base
            .store
            .markets_by_symbol()
            .get(&opts.symbol)
            .cloned();
        let ticker = self
            .reads
            .base
            .store
            .tickers_by_symbol()
            .get(&opts.symbol)
            .cloned();
        let pside = if opts.side == OrderSide::Buy {
            PositionSide::Short
        } else {
            PositionSide::Long
        };
        let position = self
            .reads
            .base
            .store
            .positions()
            .iter()
            .find(|p| p.symbol == opts.symbol && p.side == pside)
            .cloned();
        let reject = |message: String| -> Result<Vec<Value>, ExchangeError> {
            self.report(message.clone(), None)?;
            Err(ExchangeError::new(message))
        };
        let Some(market) = market else {
            return reject(format!("Market {} not found", opts.symbol));
        };
        let Some(ticker) = ticker.filter(|t| t.last.is_finite() && t.last > 0.0) else {
            return reject(format!("Ticker {} not found", opts.symbol));
        };
        let Some(position) = position else {
            return reject(format!(
                "Position {} and side {} not found",
                opts.symbol,
                if pside == PositionSide::Long {
                    "long"
                } else {
                    "short"
                }
            ));
        };
        let Some(price) = opts.price.filter(|p| p.is_finite()) else {
            return reject(format!(
                "Trailing stop price for {} is required",
                opts.symbol
            ));
        };
        let distance = adjust(
            math_max(ticker.last, price) - math_min(ticker.last, price),
            market.precision.price,
        );
        let rate = math_round(distance / ticker.last * 10000.0) / 10000.0;
        if rate <= 0.0 || rate > 1.0 {
            return reject(
                "BingX trailing stop callback rate must be greater than 0 and at most 1".into(),
            );
        }
        let position_side = self
            .get_order_position_side(opts, (position.position_mode).into_value())
            .await?;
        let mut request = json!({"symbol":market.id,"positionSide":position_side,"side":side(opts.side),"type":"TRAILING_STOP_MARKET","quantity":js_number_string(position.contracts),"priceRate":js_number_string(rate),"priceProtect":"true"});
        if opts.side.is_none() {
            request.as_object_mut().unwrap().remove("side");
        }
        Ok(vec![request])
    }
    pub async fn format_create_order(
        &self,
        opts: &PreparedOrderOpts,
    ) -> Result<Vec<Value>, ExchangeError> {
        let client_id = self.reads.base.resolve_caller_client_order_id(opts)?;
        if opts.r#type == OrderType::TrailingStopLoss {
            return self.format_create_trailing_stop_loss_order(opts).await;
        }
        let market = self
            .reads
            .base
            .store
            .markets_by_symbol()
            .get(&opts.symbol)
            .cloned()
            .ok_or_else(|| ExchangeError::new(format!("Market {} not found", opts.symbol)))?;
        let existing_mode = self
            .reads
            .base
            .store
            .positions()
            .iter()
            .find(|p| p.symbol == opts.symbol)
            .and_then(|p| (p.position_mode).into_value());
        let mut mode = match existing_mode {
            Some(mode) => mode,
            None => self.reads.get_or_fetch_position_mode(false).await?,
        };
        if crate::market_utils::is_likely_tradfi_market(&market) && mode == PositionMode::OneWay {
            mode = self.reads.get_or_fetch_position_mode(true).await?;
            if mode == PositionMode::OneWay {
                return Err(ExchangeError::new(
                    crate::errors::build_tradfi_one_way_mode_error(&opts.symbol, None),
                ));
            }
        }
        let stop = matches!(
            opts.r#type.into_value(),
            Some(OrderType::StopLoss | OrderType::TakeProfit)
        );
        let position_side = self.get_order_position_side(opts, Some(mode)).await?;
        let amount = adjust(opts.amount, market.precision.amount);
        let full = stop && (opts.close == Some(true) || !truthy_number(amount));
        let price = opts
            .price
            .filter(|p| truthy_number(*p) && opts.r#type != OrderType::Market)
            .map(|p| adjust(p, market.precision.price));
        let mut order_type = kind(opts.r#type);
        if stop {
            let opposite = if opts.side == OrderSide::Buy {
                PositionSide::Short
            } else {
                PositionSide::Long
            };
            let found = self.reads.base.store.positions().iter().any(|p| {
                p.symbol == opts.symbol
                    && p.side == opposite
                    && p.contracts > 0.0
                    && p.position_mode == Some(mode)
            });
            if !found {
                if full {
                    let message = "Cannot place a full-position stop loss or take profit order without an open position";
                    self.report(message.into(), None)?;
                    return Err(ExchangeError::new(message));
                }
                order_type = Some("TRIGGER_MARKET");
            }
        }
        let mut request =
            json!({"symbol":market.id,"positionSide":position_side,"side":side(opts.side)});
        if opts.side.is_none() {
            request.as_object_mut().unwrap().remove("side");
        }
        if let Some(order_type) = order_type {
            request["type"] = json!(order_type);
        }
        if !full && truthy_number(amount) {
            put_number(&mut request, "quantity", amount)?;
        }
        if let Some(price) = price {
            put_number(
                &mut request,
                if stop { "stopPrice" } else { "price" },
                price,
            )?;
        }
        if opts.r#type == OrderType::Limit {
            request["timeInForce"] = json!(tif(opts
                .time_in_force
                .unwrap_or(OrderTimeInForce::GoodTillCancel)));
        }
        if full {
            request["closePosition"] = json!("true");
        }
        if !full
            && mode != PositionMode::Hedged
            && (order_type == Some("TRIGGER_MARKET") || (opts.reduce_only == Some(true) && !stop))
        {
            request["reduceOnly"] = json!(true);
        }
        if opts.r#type == OrderType::TakeProfit {
            request["workingType"] = json!(if opts.tp_trigger_type
                == Some(OrderStopTriggerType::MarkPrice)
            {
                "MARK_PRICE"
            } else {
                "CONTRACT_PRICE"
            });
        }
        if opts.r#type == OrderType::StopLoss {
            request["workingType"] = json!(if opts.sl_trigger_type
                == Some(OrderStopTriggerType::LastPrice)
            {
                "CONTRACT_PRICE"
            } else {
                "MARK_PRICE"
            });
        }
        let max = safe_cex_num::numeric_value_to_number(&market.limits.amount.max);
        let lots = if amount > max {
            (amount / max).ceil()
        } else {
            1.0
        };
        let rest = if amount > max {
            adjust(amount % max, market.precision.amount)
        } else {
            0.0
        };
        let lot_size = adjust((amount - rest) / lots, market.precision.amount);
        let ordinary = matches!(
            opts.r#type.into_value(),
            Some(OrderType::Limit | OrderType::Market)
        );
        let with_id = |mut payload: Value| -> Result<Value, ExchangeError> {
            if ordinary {
                payload["clientOrderId"] = json!(match &client_id {
                    Some(id) => id.clone(),
                    None => create_client_order_id(&self.host.uuid()?),
                });
            }
            Ok(payload)
        };
        let mut payloads = Vec::new();
        if full {
            payloads.push(request.clone());
        } else {
            for _ in 0..if !lots.is_finite() || lots < 1.0 || lots > 9007199254740991.0 {
                0
            } else {
                lots as usize
            } {
                let mut payload = request.clone();
                put_number(&mut payload, "quantity", lot_size)?;
                payloads.push(with_id(payload)?);
            }
        }
        if truthy_number(rest) {
            let mut payload = request.clone();
            put_number(&mut payload, "quantity", rest)?;
            payloads.push(with_id(payload)?);
        }
        self.reads
            .base
            .assert_single_native_client_order_id(client_id.as_deref(), payloads.len())?;
        for price in [opts.stop_loss, opts.take_profit]
            .into_iter()
            .flatten()
            .filter(|p| truthy_number(*p))
        {
            let mut payload = request.clone();
            payload
                .as_object_mut()
                .ok_or_else(|| ExchangeError::new("placement payload is not an object"))?
                .remove("price");
            payload["side"] = json!(if opts.side == OrderSide::Buy {
                "SELL"
            } else {
                "BUY"
            });
            payload["type"] = json!("TRIGGER_MARKET");
            put_number(
                &mut payload,
                "stopPrice",
                adjust(price, market.precision.price),
            )?;
            if truthy_number(amount) {
                put_number(&mut payload, "quantity", amount)?;
            } else {
                payload.as_object_mut().unwrap().remove("quantity");
            }
            payload["timeInForce"] = json!("GTC");
            if mode == PositionMode::Hedged {
                payload.as_object_mut().unwrap().remove("reduceOnly");
            } else {
                payload["reduceOnly"] = json!(true);
            }
            payloads.push(payload);
        }
        Ok(payloads)
    }
    pub async fn place_order(
        &self,
        opts: &PreparedOrderOpts,
    ) -> Result<Vec<Option<Input>>, ExchangeError> {
        let payloads = self.format_create_order(opts).await?;
        self.place_order_batch(&payloads).await
    }
    pub async fn place_orders(
        self: &Rc<Self>,
        orders: &[PreparedOrderOpts],
    ) -> Result<Vec<Option<Input>>, ExchangeError> {
        self.reads.base.assert_caller_client_order_ids(orders)?;
        self.reads.get_or_fetch_position_mode(false).await?;
        let mut futures = orders
            .iter()
            .cloned()
            .map(|opts| {
                let owner = self.clone();
                Some(
                    Box::pin(async move { owner.format_create_order(&opts).await })
                        as ExchangeFuture<'static, Vec<Value>>,
                )
            })
            .collect::<Vec<_>>();
        let mut results = (0..futures.len()).map(|_| None).collect::<Vec<_>>();
        let result = std::future::poll_fn(|cx| {
            let mut pending = false;
            let mut first_error = None;
            for i in 0..futures.len() {
                if let Some(future) = &mut futures[i] {
                    match future.as_mut().poll(cx) {
                        Poll::Pending => pending = true,
                        Poll::Ready(result) => {
                            futures[i] = None;
                            match result {
                                Ok(value) => results[i] = Some(value),
                                Err(error) => {
                                    if first_error.is_none() {
                                        first_error = Some(error)
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if let Some(error) = first_error {
                Poll::Ready(Err(error))
            } else if pending {
                Poll::Pending
            } else {
                Poll::Ready(Ok(()))
            }
        })
        .await;
        if let Err(error) = result {
            for mut future in futures.into_iter().flatten() {
                let runtime = self.reads.base.runtime.clone();
                let reporter = runtime.clone();
                runtime.spawn_local(Box::pin(async move {
                    if let Err(e) = future.as_mut().await {
                        reporter.report_exception(
                            &e,
                            &json!({"operation":"BingX formatting sibling"}).into(),
                        );
                    }
                }));
            }
            return Err(error);
        }
        let payloads = results.into_iter().flatten().flatten().collect::<Vec<_>>();
        self.place_order_batch(&payloads).await
    }
    pub async fn place_order_batch(
        &self,
        payloads: &[Value],
    ) -> Result<Vec<Option<Input>>, ExchangeError> {
        let mut ids = Vec::new();
        let mut message = String::new();
        for payload in payloads {
            let symbol = payload
                .get("symbol")
                .and_then(Value::as_str)
                .map(|s| s.replace('-', ""))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "unknown-symbol".into());
            let result = async {
                let data = self
                    .post(crate::constants::ENDPOINTS[11].1, payload.clone())
                    .await?;
                if data.get("code").and_then(Input::as_f64) != Some(0.0) {
                    message = crate::errors::normalize_order_placement_error(
                        &symbol,
                        &safe_cex_num::js_retained_string(
                            data.get("msg")
                                .filter(|v| v.is_truthy())
                                .or(Some(&Input::from(json!("")))),
                        )
                        .map_err(|e| ExchangeError::new(e.message))?,
                    );
                    self.report(message.clone(), None)?;
                } else {
                    let raw = data
                        .get("data")
                        .and_then(|d| d.get("order"))
                        .filter(|v| v.is_truthy())
                        .cloned()
                        .unwrap_or_else(|| Input::Object(vec![]));
                    let order = map_order(
                        &raw,
                        &self.reads.base.store.orders(),
                        safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms(),
                    )?;
                    if let Some(order) = order
                        && order.status == OrderStatus::Open
                        && order.r#type != OrderType::Market
                    {
                        self.reads
                            .base
                            .store
                            .add_or_update_order(order)
                            .map_err(ExchangeError::from)?;
                    }
                    ids.push(
                        raw.get("orderID")
                            .filter(|v| v.is_truthy())
                            .or_else(|| raw.get("orderId"))
                            .cloned(),
                    );
                }
                Ok::<_, ExchangeError>(())
            }
            .await;
            if let Err(error) = result {
                message =
                    crate::errors::normalize_order_placement_error(&symbol, &error_text(&error)?);
                self.report(message.clone(), Some("Failed to switch margin type"))?;
            }
        }
        if !message.is_empty() {
            Err(ExchangeError::new(message))
        } else {
            Ok(ids)
        }
    }
    pub async fn place_order_batch_old(
        &self,
        payloads: &[Value],
    ) -> Result<Vec<Option<Input>>, ExchangeError> {
        let mut ids = Vec::new();
        let mut message = String::new();
        for lot in payloads.chunks(5) {
            if lot.len() == 1 {
                let result = self
                    .post(crate::constants::ENDPOINTS[11].1, lot[0].clone())
                    .await;
                match result {
                    Ok(data) => {
                        if data.get("code").and_then(Input::as_f64) != Some(0.0) {
                            message = safe_cex_num::js_retained_string(data.get("msg"))
                                .map_err(|e| ExchangeError::new(e.message))?;
                            self.report(message.clone(), None)?;
                            ids.push(None);
                        } else {
                            let raw = data.get("data").and_then(|d| d.get("order"));
                            ids.push(
                                raw.and_then(|o| o.get("orderID"))
                                    .filter(|v| v.is_truthy())
                                    .or_else(|| raw.and_then(|o| o.get("orderId")))
                                    .cloned(),
                            );
                        }
                    }
                    Err(error) => {
                        message = error_text(&error)?;
                        self.report(message.clone(), Some("Failed to update leverage"))?;
                        ids.push(None);
                    }
                }
            } else {
                let batch =
                    safe_cex_num::js_json_stringify(&safe_cex_num::JsJsonValue::from(json!(lot)))
                        .map_err(|e| ExchangeError::new(e.message))?
                        .unwrap_or_default();
                let result = self
                    .post(
                        crate::constants::ENDPOINTS[14].1,
                        json!({"batchOrders":batch}),
                    )
                    .await;
                match result {
                    Ok(data) => {
                        if data.get("code").and_then(Input::as_f64) != Some(0.0) {
                            if let Some(msg) = data.get("msg").and_then(|v| match v {
                                Input::Text(units) => String::from_utf16(units).ok(),
                                _ => None,
                            }) {
                                if !msg.contains("Msg: lock fail") {
                                    message = msg;
                                    self.report(message.clone(), None)?;
                                }
                            } else {
                                message = match data.get("msg") {
                                    None => {
                                        "Cannot read properties of undefined (reading 'includes')"
                                    }
                                    Some(Input::Null) => {
                                        "Cannot read properties of null (reading 'includes')"
                                    }
                                    Some(_) => "data.msg.includes is not a function",
                                }
                                .into();
                                self.report(
                                    message.clone(),
                                    Some("Failed to switch position mode"),
                                )?;
                            }
                        }
                        if data.get("forEach").is_some_and(|v| !v.is_null()) {
                            message = "data?.forEach is not a function".into();
                            self.report(message.clone(), Some("Failed to switch position mode"))?;
                        }
                    }
                    Err(error) => {
                        message = error_text(&error)?;
                        self.report(message.clone(), Some("Failed to switch position mode"))?;
                    }
                }
            }
        }
        if !message.is_empty() {
            Err(ExchangeError::new(message))
        } else {
            Ok(ids)
        }
    }
}
