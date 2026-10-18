//! Reconciled quantity amendment and cancel/replace bodies.
use crate::{
    data_reads::{BingxDataReads, ReadTransport},
    placement::{BingxPlacement, create_client_order_id},
    private_events::map_order,
    read_history::BingxReadHistory,
};
use safe_cex_exchange::{error_reporting::ExchangeErrorInput, runtime::ExchangeError};

use safe_cex_num::{adjust, js_retained_number, js_retained_string, js_string, js_truthy};
use safe_cex_transport::{RestBody, RestRequest};
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use safe_cex_types::{
    Order, OrderSide, OrderStatus, OrderTimeInForce, OrderType, PreparedOrderOpts, UpdateOrderOpts,
    UpdateOrderOptsUpdate,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::rc::Rc;

pub struct BingxAmendment {
    pub reads: Rc<BingxDataReads>,
    pub history: Rc<BingxReadHistory>,
    pub placement: Rc<BingxPlacement>,
    pub transport: Rc<dyn ReadTransport>,
}
fn set_field(fields: &mut Vec<(Vec<u16>, Input)>, key: Vec<u16>, value: Input) {
    if let Some((_, old)) = fields.iter_mut().find(|(name, _)| name == &key) {
        *old = value;
    } else {
        fields.push((key, value));
    }
}
fn alias<'a>(info: Option<&'a Input>, keys: &[&str]) -> Option<&'a Input> {
    keys.iter().enumerate().find_map(|(index, key)| {
        info.and_then(|value| value.get(key))
            .filter(|value| index + 1 == keys.len() || !value.is_nullish())
    })
}
fn text(value: Option<&Input>) -> Result<String, ExchangeError> {
    js_retained_string(value).map_err(|e| {
        let mut error = ExchangeError::new(e.message);
        error.name = e.name.into();
        error
    })
}
fn number(value: Option<&Input>) -> Result<f64, ExchangeError> {
    js_retained_number(value).map_err(|e| {
        let mut error = ExchangeError::new(e.message);
        error.name = e.name.into();
        error
    })
}
fn upper(info: Option<&Input>, keys: &[&str]) -> Result<String, ExchangeError> {
    Ok(text(
        alias(info, keys)
            .filter(|v| !v.is_nullish())
            .or(Some(&Input::Text(vec![]))),
    )?
    .to_uppercase())
}
fn boolean(v: Option<&Input>) -> Option<bool> {
    match v {
        Some(Input::Bool(b)) => Some(*b),
        Some(v) if v.text_equals("true") => Some(true),
        Some(v) if v.text_equals("false") => Some(false),
        _ => None,
    }
}
fn raw_side(v: &str) -> Option<OrderSide> {
    match v {
        "BUY" => Some(OrderSide::Buy),
        "SELL" => Some(OrderSide::Sell),
        _ => None,
    }
}
fn raw_tif(v: &str) -> Option<OrderTimeInForce> {
    crate::constants::TIME_IN_FORCE
        .iter()
        .find(|(k, _)| *k == v)
        .map(|(_, v)| *v)
}
fn open_status(v: &str) -> bool {
    matches!(v, "NEW" | "OPEN" | "PENDING" | "WORKING")
}
fn protection(v: Option<&Input>) -> Result<bool, ExchangeError> {
    let Some(v) = v.filter(|v| matches!(v, Input::Object(_) | Input::Array(_))) else {
        return Ok(false);
    };
    for k in ["stopPrice", "quantity", "price"] {
        if number(v.get(k))? > 0.0 {
            return Ok(true);
        }
    }
    Ok(false)
}
fn opts(
    order: &Order,
    price: f64,
    kind: safe_cex_types::ModelField<OrderType>,
) -> PreparedOrderOpts {
    PreparedOrderOpts {
        symbol: order.symbol.clone(),
        r#type: kind.into_property(),
        side: order.side.into_property(),
        amount: order.amount,
        price: Some(price),
        reduce_only: order.reduce_only.into_property(),
        close: order.close.into_value(),
        time_in_force: None,
        client_order_id: None,
        stop_loss: None,
        take_profit: None,
        tp_trigger_type: if kind == OrderType::TakeProfit {
            order.tp_trigger_type.or(order.sl_trigger_type).into_value()
        } else {
            None
        },
        sl_trigger_type: if kind == OrderType::StopLoss {
            order.sl_trigger_type.or(order.tp_trigger_type).into_value()
        } else {
            None
        },
        trigger_price: None,
        trigger_type: None,
        max_slippage_pct: None,
        implied_volatility: None,
    }
}
impl BingxAmendment {
    fn report(&self, message: String) -> Result<(), ExchangeError> {
        self.reads
            .base
            .emit_and_log_error(ExchangeErrorInput::Message(message), None, None)
    }
    fn reject<T>(&self, message: impl Into<String>) -> Result<T, ExchangeError> {
        let message = message.into();
        self.report(message.clone())?;
        Err(ExchangeError::new(message))
    }
    fn market(&self, order: &Order) -> Result<safe_cex_types::Market, ExchangeError> {
        self.reads
            .base
            .store
            .markets_by_symbol()?
            .get(&order.symbol)
            .cloned()
            .ok_or_else(|| ExchangeError::new(format!("Market {} not found", order.symbol)))
    }
    async fn post(&self, url: &str, body: Value) -> Result<Input, ExchangeError> {
        let result = self
            .transport
            .send(RestRequest {
                method: "POST".into(),
                url: url.into(),
                body: Some(RestBody::Untransformed(
                    safe_cex_types::retained_json::RetainedJsonInput::from_value(body)
                        .map_err(|error| ExchangeError::new(error.to_string()))?,
                )),
                ..Default::default()
            })
            .await;
        let mut yielded = false;
        std::future::poll_fn(|cx| {
            if yielded {
                std::task::Poll::Ready(())
            } else {
                yielded = true;
                cx.waker().wake_by_ref();
                std::task::Poll::Pending
            }
        })
        .await;
        result?
            .decode_with(Input::deserialize)
            .map_err(|e| ExchangeError::new(e.to_string()))
    }
    fn caught<T>(&self, result: Result<T, ExchangeError>) -> Result<T, ExchangeError> {
        if let Err(error) = &result {
            let wire = error
                .response
                .as_ref()
                .and_then(|r| r.get("data"))
                .and_then(|d| d.get("msg"));
            let message = if js_truthy(wire) {
                js_string(wire).map_err(|e| ExchangeError::new(e.message))?
            } else if !error.message.is_empty() {
                error.message.clone()
            } else {
                error.js_error_string()
            };
            self.report(message)?;
        }
        result
    }
    pub async fn update_order(
        &self,
        request: &UpdateOrderOpts,
    ) -> Result<Vec<Option<Input>>, ExchangeError> {
        let order = &request.order;
        let (amount, price) = match &request.update {
            UpdateOrderOptsUpdate::Variant0(u) => (Some(u.amount), u.price),
            UpdateOrderOptsUpdate::Variant1(u) => (u.amount, Some(u.price)),
        };
        if matches!(
            order.r#type,
            safe_cex_types::ModelField::Value(OrderType::StopLoss | OrderType::TakeProfit)
        ) {
            if amount.is_some() {
                return self.reject("BingX trigger order movement supports price updates only");
            }
            return self
                .replace_trigger_order_price(order, price.unwrap_or(f64::NAN))
                .await;
        }
        if order.status != OrderStatus::Open || order.r#type != OrderType::Limit {
            return self.reject(
                "BingX native amendment supports only exactly reconciled open LIMIT orders",
            );
        }
        if price.is_some() && amount.is_some() {
            return self.reject(
                "BingX limit price replacement cannot be combined with native quantity amendment",
            );
        }
        if let Some(price) = price {
            return self.cancel_replace_limit_order_price(order, price).await;
        }
        let Some(amount) = amount else {
            return self.reject("BingX native amendment requires an amount update");
        };
        if !matches!(
            order.time_in_force,
            safe_cex_types::ModelField::Value(
                OrderTimeInForce::GoodTillCancel | OrderTimeInForce::PostOnly
            )
        ) {
            return self.reject(
                "BingX native amendment requires a reconciled GTC or PostOnly time in force",
            );
        }
        if order.close == Some(true) {
            return self.reject("BingX native amendment does not support close-position orders");
        }
        let market = match self.market(order) {
            Ok(m) => m,
            Err(e) => return self.reject(e.message),
        };
        let info = order.info.as_ref();
        let id = alias(info, &["orderId", "i"]);
        let symbol = alias(info, &["symbol", "s"]);
        let kind = upper(info, &["type", "o"])?;
        let side = upper(info, &["side", "S"])?;
        let position = upper(info, &["positionSide", "ps"])?;
        let status = upper(info, &["status", "X"])?;
        let quantity = number(alias(info, &["origQty", "quantity", "q"]))?;
        let filled = number(alias(info, &["executedQty", "z"]))?;
        let reduce = boolean(alias(info, &["reduceOnly", "ro", "R"]));
        if id.is_none()
            || text(id)? != order.id
            || symbol
                .and_then(|v| v.as_text())
                .and_then(|v| String::from_utf16(v).ok())
                .is_none_or(|s| s.replace('-', "") != order.symbol)
            || kind != "LIMIT"
            || raw_side(&side) != order.side.into_value()
            || !matches!(position.as_str(), "BOTH" | "LONG" | "SHORT")
        {
            return self.reject(format!(
                "BingX cannot safely amend {}: immutable order identity is not reconciled",
                order.id
            ));
        }
        if reduce.is_some_and(|b| order.reduce_only != b) {
            return self.reject(format!(
                "BingX cannot safely amend {}: reduce-only state is not reconciled",
                order.id
            ));
        }
        let parsed = raw_tif(&text(
            alias(info, &["timeInForce", "f"])
                .filter(|v| !v.is_nullish())
                .or(Some(&Input::Text(vec![]))),
        )?);
        let tif = if boolean(alias(info, &["postOnly"])) == Some(true) {
            Some(OrderTimeInForce::PostOnly)
        } else {
            parsed
        };
        if parsed.is_none() || tif.is_none() || tif != order.time_in_force.into_value() {
            return self.reject(format!(
                "BingX cannot safely amend {}: time in force is not reconciled",
                order.id
            ));
        }
        let old = adjust(order.amount, market.precision.amount);
        if !open_status(&status)
            || order.filled != 0.0
            || filled != 0.0
            || adjust(order.remaining, market.precision.amount) != old
            || adjust(quantity, market.precision.amount) != old
        {
            return self.reject("BingX partially filled amendment is disabled until quantity is documented as total or remaining size");
        }
        if order.take_profit.is_some()
            || order.stop_loss.is_some()
            || protection(info.and_then(|v| v.get("takeProfit")))?
            || protection(info.and_then(|v| v.get("stopLoss")))?
        {
            return self.reject(
                "BingX native amendment cannot prove attached TP/SL quantity preservation",
            );
        }
        let qty = adjust(amount, market.precision.amount);
        if !qty.is_finite() || qty <= 0.0 {
            return self.reject("BingX amended order quantity must be greater than zero");
        }
        if qty < market.limits.amount.min
            || qty > safe_cex_num::numeric_value_to_number(&market.limits.amount.max)
        {
            return self
                .reject("BingX amended order quantity is outside the reconciled market limits");
        }
        if qty == old {
            return self.reject(
                "BingX amended order quantity must change after market-precision normalization",
            );
        }
        let result = async {
            let data = self
                .post(
                    crate::constants::ENDPOINTS[12].1,
                    json!({"orderId":order.id,"quantity":qty,"symbol":market.id}),
                )
                .await?;
            if data.get("code").and_then(Input::as_f64) != Some(0.0) {
                return Err(ExchangeError::new(
                    if data.get("msg").is_some_and(Input::is_truthy) {
                        text(data.get("msg"))?
                    } else {
                        format!(
                            "BingX amend order failed with code {}",
                            text(data.get("code"))?
                        )
                    },
                ));
            }
            let d = data.get("data");
            if text(
                alias(d, &["orderId"])
                    .filter(|v| !v.is_nullish())
                    .or(Some(&Input::Text(vec![]))),
            )? != order.id
                || text(
                    alias(d, &["symbol"])
                        .filter(|v| !v.is_nullish())
                        .or(Some(&Input::Text(vec![]))),
                )?
                .replace('-', "")
                    != order.symbol
                || adjust(
                    number(d.and_then(|v| v.get("quantity")))?,
                    market.precision.amount,
                ) != qty
            {
                return Err(ExchangeError::new(format!(
                    "BingX amend response did not confirm order {}",
                    order.id
                )));
            }
            Ok(vec![Some(Input::from(json!(order.id)))])
        }
        .await;
        self.caught(result)
    }
    pub async fn replace_trigger_order_price(
        &self,
        order: &Order,
        next_price: f64,
    ) -> Result<Vec<Option<Input>>, ExchangeError> {
        if order.status != OrderStatus::Open
            || !matches!(
                order.r#type,
                safe_cex_types::ModelField::Value(OrderType::StopLoss | OrderType::TakeProfit)
            )
        {
            return self
                .reject("BingX trigger replacement supports only reconciled open STOP/TP orders");
        }
        let market = match self.market(order) {
            Ok(m) => m,
            Err(e) => return self.reject(e.message),
        };
        let info = order.info.as_ref();
        let symbol = alias(info, &["symbol", "s"]);
        let kind = upper(info, &["type", "o"])?;
        let side = upper(info, &["side", "S"])?;
        let status = upper(info, &["status", "X"])?;
        let filled = number(
            alias(info, &["executedQty", "z"])
                .filter(|v| !v.is_nullish())
                .or(Some(&Input::Float(0.0))),
        )?;
        let mapped = crate::constants::ORDER_TYPE
            .iter()
            .find(|(k, _)| *k == kind)
            .map(|(_, v)| *v)
            .filter(|v| matches!(v, OrderType::StopLoss | OrderType::TakeProfit));
        let replacement = mapped
            .map(safe_cex_types::ModelField::Value)
            .unwrap_or(order.r#type);
        if symbol
            .and_then(|v| v.as_text())
            .and_then(|v| String::from_utf16(v).ok())
            .is_some_and(|s| s.replace('-', "") != order.symbol)
        {
            return self.reject(format!(
                "BingX cannot safely replace {}: trigger order identity is not reconciled",
                order.id
            ));
        }
        if (!kind.is_empty() && mapped.is_none())
            || (!side.is_empty() && raw_side(&side) != order.side.into_value())
            || (!status.is_empty() && !open_status(&status))
            || order.filled != 0.0
            || filled != 0.0
        {
            return self
                .reject("BingX trigger replacement is disabled for partially filled orders");
        }
        let amount = adjust(order.amount, market.precision.amount);
        if order.close != Some(true) && (!amount.is_finite() || amount <= 0.0) {
            return self.reject("BingX replacement trigger quantity must be greater than zero");
        }
        let price = adjust(next_price, market.precision.price);
        if !price.is_finite() || price <= 0.0 {
            return self.reject("BingX replacement trigger price must be greater than zero");
        }
        if price == adjust(order.price.to_number(), market.precision.price) {
            return self.reject(
                "BingX replacement trigger price must change after market-precision normalization",
            );
        }
        if let Some(ticker) = self
            .reads
            .base
            .store
            .tickers_by_symbol()?
            .get(&order.symbol)
            && ticker.mark.is_finite()
            && ticker.mark > 0.0
        {
            let immediate = if replacement == OrderType::StopLoss {
                if order.side == OrderSide::Sell {
                    price >= ticker.mark
                } else {
                    price <= ticker.mark
                }
            } else if order.side == OrderSide::Sell {
                price <= ticker.mark
            } else {
                price >= ticker.mark
            };
            if immediate {
                return self.reject("BingX replacement trigger price would execute immediately against current mark price");
            }
        }
        self.reads
            .cancel_orders(std::slice::from_ref(order))
            .await?;
        let mut request = opts(order, price, replacement);
        request.amount = amount;
        self.placement.place_order(&request).await
    }
    pub async fn cancel_replace_limit_order_price(
        &self,
        order: &Order,
        next_price: f64,
    ) -> Result<Vec<Option<Input>>, ExchangeError> {
        if order.status != OrderStatus::Open || order.r#type != OrderType::Limit {
            return self
                .reject("BingX cancel-replace supports only exactly reconciled open LIMIT orders");
        }
        if order.close == Some(true) {
            return self.reject("BingX cancel-replace does not support close-position orders");
        }
        let market = match self.market(order) {
            Ok(m) => m,
            Err(e) => return self.reject(e.message),
        };
        let fetched = if order.time_in_force.is_none() {
            self.history
                .fetch_orders()
                .await?
                .into_iter()
                .find(|o| o.id == order.id && o.time_in_force.is_some())
        } else {
            None
        };
        let reconciled = fetched.as_ref().unwrap_or(order);
        let info = reconciled.info.as_ref();
        let id = alias(info, &["orderId", "orderID", "i"]);
        let symbol = alias(info, &["symbol", "s"]);
        let kind = upper(info, &["type", "o"])?;
        let side = upper(info, &["side", "S"])?;
        let position = upper(info, &["positionSide", "ps"])?;
        let status = upper(info, &["status", "X"])?;
        let qty = number(alias(info, &["origQty", "quantity", "q"]))?;
        let executed = number(alias(info, &["executedQty", "z"]))?;
        let client = alias(info, &["clientOrderId", "clientOrderID", "c"]);
        let reduce = boolean(alias(info, &["reduceOnly", "ro", "R"]));
        let valid_position = matches!(position.as_str(), "BOTH" | "LONG" | "SHORT");
        let resolved = if valid_position {
            position.clone()
        } else {
            self.placement
                .get_order_position_side(
                    &opts(
                        reconciled,
                        reconciled.price.to_number(),
                        OrderType::Limit.into(),
                    ),
                    None,
                )
                .await?
        };
        if (id.is_some() && text(id)? != reconciled.id)
            || symbol
                .and_then(|v| v.as_text())
                .and_then(|v| String::from_utf16(v).ok())
                .is_some_and(|s| s.replace('-', "") != reconciled.symbol)
            || (!kind.is_empty() && kind != "LIMIT")
            || (!side.is_empty() && raw_side(&side) != reconciled.side.into_value())
            || (!position.is_empty() && !valid_position)
        {
            return self.reject(format!(
                "BingX cannot safely replace {}: immutable order identity is not reconciled",
                order.id
            ));
        }
        if reduce.is_some_and(|b| reconciled.reduce_only != b) {
            return self.reject(format!(
                "BingX cannot safely replace {}: reduce-only state is not reconciled",
                order.id
            ));
        }
        let post = boolean(alias(info, &["postOnly"]));
        let parsed = raw_tif(&text(
            alias(info, &["timeInForce", "f"])
                .filter(|v| !v.is_nullish())
                .or(Some(&Input::Text(vec![]))),
        )?);
        let tif = if post == Some(true) {
            Some(OrderTimeInForce::PostOnly)
        } else {
            parsed.or(reconciled.time_in_force.into_value())
        };
        if tif.is_none()
            || (post == Some(false) && parsed.is_none() && reconciled.time_in_force.is_none())
            || tif != reconciled.time_in_force.into_value()
        {
            return self.reject(format!(
                "BingX cannot safely replace {}: time in force is not reconciled",
                order.id
            ));
        }
        let amount = adjust(reconciled.amount, market.precision.amount);
        let raw_qty = if qty.is_finite() {
            qty
        } else {
            reconciled.amount
        };
        let raw_executed = if executed.is_finite() {
            executed
        } else {
            reconciled.filled
        };
        if (!status.is_empty() && !open_status(&status))
            || reconciled.filled != 0.0
            || raw_executed != 0.0
            || adjust(reconciled.remaining, market.precision.amount) != amount
            || adjust(raw_qty, market.precision.amount) != amount
        {
            return self
                .reject("BingX cancel-replace is disabled for partially filled LIMIT orders");
        }
        if reconciled.take_profit.is_some()
            || reconciled.stop_loss.is_some()
            || protection(info.and_then(|v| v.get("takeProfit")))?
            || protection(info.and_then(|v| v.get("stopLoss")))?
        {
            return self.reject("BingX cancel-replace cannot prove attached TP/SL preservation");
        }
        let price = adjust(next_price, market.precision.price);
        if !price.is_finite() || price <= 0.0 {
            return self.reject("BingX replacement order price must be greater than zero");
        }
        if price == adjust(reconciled.price.to_number(), market.precision.price) {
            return self.reject(
                "BingX replacement order price must change after market-precision normalization",
            );
        }
        let tif = reconciled
            .time_in_force
            .ok_or_else(|| ExchangeError::new("missing reconciled time in force"))?;
        let tif = crate::constants::TIME_IN_FORCE
            .iter()
            .find(|(_, v)| *v == tif)
            .map(|(k, _)| *k)
            .ok_or_else(|| ExchangeError::new("unknown time in force"))?;
        let Some(client) = client
            .and_then(|v| v.as_text())
            .and_then(|v| String::from_utf16(v).ok())
            .filter(|s| !s.is_empty())
        else {
            return self.reject(format!(
                "BingX cannot safely replace {}: client order id is not reconciled",
                order.id
            ));
        };
        if !amount.is_finite() {
            return self.reject("BingX typed replacement quantity refuses a non-finite number");
        }
        let mut payload = json!({"symbol":market.id,"cancelClientOrderId":client,"side":if reconciled.side==OrderSide::Buy{"BUY"}else{"SELL"},"positionSide":resolved,"type":"LIMIT","quantity":amount,"price":price,"cancelReplaceMode":"STOP_ON_FAILURE","timeInForce":tif,"clientOrderId":create_client_order_id(&self.placement.host.uuid()?)});
        if matches!(status.as_str(), "PENDING" | "NEW") {
            payload["cancelRestrictions"] = json!(if status == "PENDING" {
                "ONLY_PENDING"
            } else {
                "ONLY_NEW"
            });
        }
        if resolved == "BOTH" && reconciled.reduce_only == true {
            payload["reduceOnly"] = json!(true)
        }
        let result = async {
            let data = self
                .post(crate::constants::ENDPOINTS[13].1, payload.clone())
                .await?;
            if data.get("code").and_then(Input::as_f64) != Some(0.0) {
                return Err(ExchangeError::new(
                    if data.get("msg").is_some_and(Input::is_truthy) {
                        text(data.get("msg"))?
                    } else {
                        format!(
                            "BingX cancel-replace failed with code {}",
                            text(data.get("code"))?
                        )
                    },
                ));
            }
            let d = data.get("data").filter(|v| v.is_truthy());
            let cancel = d.and_then(|v| v.get("cancelResult"));
            let replace = alias(d, &["newOrderResult", "replaceResult"]);
            let success = |v: Option<&Input>| -> Result<bool, ExchangeError> {
                Ok(v == Some(&Input::Bool(true))
                    || matches!(text(v)?.to_uppercase().as_str(), "SUCCESS" | "TRUE"))
            };
            if !success(cancel)? || !success(replace)? {
                return Err(ExchangeError::new(format!(
                    "BingX cancel-replace failed: cancel={}, new={}",
                    text(
                        cancel
                            .filter(|v| v.is_truthy())
                            .or(Some(&Input::Text("unknown".encode_utf16().collect())))
                    )?,
                    text(
                        replace
                            .filter(|v| v.is_truthy())
                            .or(Some(&Input::Text("unknown".encode_utf16().collect())))
                    )?
                )));
            }
            let response = d
                .and_then(|v| v.get("newOrderResponse"))
                .filter(|v| v.is_truthy());
            let id = alias(d, &["newOrderId"])
                .filter(|v| !v.is_nullish())
                .or_else(|| alias(response, &["orderId", "orderID"]).filter(|v| !v.is_nullish()))
                .or_else(|| alias(d, &["orderId", "orderID"]));
            let Some(id) = id else {
                return Err(ExchangeError::new(
                    "BingX cancel-replace response did not include a replacement order id",
                ));
            };
            let id = text(Some(id))?;
            let mut fields: Vec<(Vec<u16>, Input)> = vec![];
            let retained_payload = Input::from(payload.clone());
            for value in [response, info, Some(&retained_payload)] {
                if let Some(Input::Object(source)) = value {
                    for (key, value) in source {
                        set_field(&mut fields, key.clone(), value.clone());
                    }
                }
            }
            for (key, value) in [
                ("orderId", Input::from(json!(id))),
                ("orderID", Input::from(json!(id))),
                ("status", Input::from(json!("PENDING"))),
                (
                    "origQty",
                    Input::from(json!(safe_cex_num::js_number_string(amount))),
                ),
                ("executedQty", Input::from(json!("0"))),
            ] {
                set_field(&mut fields, key.encode_utf16().collect(), value);
            }
            let raw = Input::Object(fields);
            let replacement = map_order(
                &raw,
                &self.reads.base.store.orders()?,
                safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms(),
            )?;
            self.reads
                .base
                .store
                .update_order_status(&reconciled.id, OrderStatus::Canceled)
                .map_err(ExchangeError::from)?;
            if let Some(replacement) = replacement {
                self.reads
                    .base
                    .store
                    .add_or_update_order(replacement)
                    .map_err(ExchangeError::from)?;
            }
            Ok(vec![Some(Input::from(json!(id)))])
        }
        .await;
        self.caught(result)
    }
}
