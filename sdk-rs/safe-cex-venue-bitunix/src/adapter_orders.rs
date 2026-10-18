//! Order reads, placement, batching, cancellation and amendment hosts over the store.
use crate::{
    adapter::{
        BitunixVenue, Fault, coercion, from_input, from_value, reason, request_payload, to_input,
        units, utf8,
    },
    tpsl_helpers::TpslLeg,
};
use safe_cex_exchange::runtime::{ExchangeError, ExchangeFuture};
use safe_cex_num::{JsJsonValue, js_number_string};
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use safe_cex_types::{
    Order, OrderStatus, OrderStopTriggerType, OrderType, PlaceOrderOpts, Position,
    PreparedOrderOpts, UpdateOrderOpts,
};
use serde_json::{Value, json};
use std::{cell::RefCell, future::Future, rc::Rc};

fn leg(name: &str) -> TpslLeg {
    if name == "tp" {
        TpslLeg::Tp
    } else {
        TpslLeg::Sl
    }
}
fn order_of(value: &Input) -> Result<Order, ExchangeError> {
    let mut order = to_input(&from_value::<Order>(
        json!({"id":"","status":"open","symbol":"","type":"conditional","side":"buy","price":0,"amount":0,"filled":0,"remaining":0,"reduceOnly":false,"lastUpdate":0}),
    )?)?;
    if let Some(fields) = value.as_object() {
        let target = order.as_object_mut().expect("native order object");
        for (key, value) in fields {
            if !value.is_nullish() {
                if let Some((_, field)) = target.iter_mut().find(|(name, _)| name == key) {
                    *field = value.clone();
                } else {
                    target.push((key.clone(), value.clone()));
                }
            }
        }
    }
    if order
        .get("type")
        .cloned()
        .and_then(|value| from_input::<OrderType>(value).ok())
        .is_none()
    {
        *order.get_mut("type").expect("order type default") =
            Input::Text("conditional".encode_utf16().collect());
    }
    from_input(order)
}
fn js_text(value: &JsJsonValue) -> String {
    match value {
        JsJsonValue::Undefined => "undefined".into(),
        JsJsonValue::Null => "null".into(),
        JsJsonValue::Bool(b) => b.to_string(),
        JsJsonValue::Number(n) => js_number_string(*n),
        JsJsonValue::String(s) => s.clone(),
        JsJsonValue::BigInt(b) => b.to_string(),
        JsJsonValue::Array(items) => items
            .iter()
            .map(|i| match i {
                JsJsonValue::Undefined | JsJsonValue::Null => String::new(),
                other => js_text(other),
            })
            .collect::<Vec<_>>()
            .join(","),
        JsJsonValue::Object(_) => "[object Object]".into(),
    }
}
fn js_truth(value: &JsJsonValue) -> bool {
    match value {
        JsJsonValue::Undefined | JsJsonValue::Null => false,
        JsJsonValue::Bool(b) => *b,
        JsJsonValue::Number(n) => *n != 0.0 && !n.is_nan(),
        JsJsonValue::String(s) => !s.is_empty(),
        JsJsonValue::BigInt(b) => b.to_string() != "0",
        _ => true,
    }
}

pub(crate) struct OrderFrame {
    venue: Rc<BitunixVenue>,
}
impl BitunixVenue {
    pub(crate) fn order_frame(self: &Rc<Self>) -> OrderFrame {
        OrderFrame {
            venue: self.clone(),
        }
    }
    fn store_orders(&self) -> Result<Vec<Order>, ExchangeError> {
        Ok(self.base.store.orders()?)
    }
}
impl crate::order_models::OrderModelHost for OrderFrame {
    type Error = Fault;
    fn caller_id(&mut self, options: &Input) -> Result<Option<String>, Fault> {
        let opts: PreparedOrderOpts = from_input(options.clone())?;
        Ok(self.venue.base.resolve_caller_client_order_id(&opts)?)
    }
    fn market(&mut self, symbol: &Input) -> Result<Option<Input>, Fault> {
        let symbol = safe_cex_num::js_retained_string(Some(symbol))?;
        let market = self
            .venue
            .base
            .store
            .markets_by_symbol()?
            .get(&symbol)
            .cloned();
        Ok(market.map(|m| to_input(&m)).transpose()?)
    }
    fn positions(&mut self) -> Result<Vec<Input>, Fault> {
        let positions = self.venue.base.store.positions()?.to_vec();
        Ok(positions.iter().map(to_input).collect::<Result<_, _>>()?)
    }
    fn orders(&mut self) -> Result<Vec<Input>, Fault> {
        Ok(self
            .venue
            .store_orders()?
            .iter()
            .map(to_input)
            .collect::<Result<_, _>>()?)
    }
    fn cached_position(&mut self, key: &str) -> Result<Option<Input>, Fault> {
        let state = self.venue.state.borrow();
        Ok(state
            .position_id_cache
            .get(key)
            .cloned()
            .map(|value| Input::Text(value.encode_utf16().collect())))
    }
    fn uuid(&mut self) -> Result<String, Fault> {
        Ok(self.venue.runtime.unique_id().replace(['-', '_'], ""))
    }
    fn assert_single(&mut self, caller: Option<&str>, count: usize) -> Result<(), Fault> {
        Ok(self
            .venue
            .base
            .assert_single_native_client_order_id(caller, count)?)
    }
    fn cache_tif(&mut self, client_id: &str, tif: &Input) -> Result<(), Fault> {
        let tif = safe_cex_num::js_retained_string(Some(tif))?;
        self.venue
            .state
            .borrow_mut()
            .order_tif_cache
            .insert(client_id.into(), tif);
        Ok(())
    }
    fn cache_reduce_only(&mut self, client_id: &str, reduce_only: &Input) -> Result<(), Fault> {
        self.venue
            .state
            .borrow_mut()
            .order_reduce_only_cache
            .insert(client_id.into(), reduce_only.is_truthy());
        Ok(())
    }
    fn emit(&mut self, message: Input) -> Result<(), Fault> {
        Ok(self
            .venue
            .emit_error(&safe_cex_num::js_retained_string(Some(&message))?)?)
    }
    fn error(&mut self, message: Input) -> Result<Fault, Fault> {
        Ok(Fault(ExchangeError::new(safe_cex_num::js_retained_string(
            Some(&message),
        )?)))
    }
    fn reason(&mut self, error: &Fault, fallback: Option<&str>) -> Result<Input, Fault> {
        let message = reason(&error.0);
        Ok(Input::Text(
            match fallback {
                Some(f) if message.is_empty() => f.into(),
                _ => message,
            }
            .encode_utf16()
            .collect(),
        ))
    }
    fn warning(&mut self, prefix: &'static str, error: &Fault) -> Result<(), Fault> {
        Ok(self
            .venue
            .log(&format!("{prefix}{}", reason(&error.0)), "warning")?)
    }
    fn get(
        &mut self,
        endpoint: &'static str,
        config: Input,
    ) -> impl Future<Output = Result<Input, Fault>> {
        let client = self.venue.xhr();
        async move {
            Ok(Input::Object(vec![(
                "data".encode_utf16().collect(),
                client.get(endpoint, request_payload(config)?).await?.into(),
            )]))
        }
    }
    fn post(
        &mut self,
        endpoint: &'static str,
        payload: Input,
    ) -> impl Future<Output = Result<Input, Fault>> {
        let client = self.venue.unlimited();
        async move {
            Ok(Input::Object(vec![(
                "data".encode_utf16().collect(),
                client
                    .post(endpoint, request_payload(payload)?)
                    .await?
                    .into(),
            )]))
        }
    }
    fn post_limited(
        &mut self,
        endpoint: &'static str,
        payload: Input,
    ) -> impl Future<Output = Result<Input, Fault>> {
        let client = self.venue.xhr();
        async move {
            Ok(Input::Object(vec![(
                "data".encode_utf16().collect(),
                client
                    .post(endpoint, request_payload(payload)?)
                    .await?
                    .into(),
            )]))
        }
    }
    fn update_status(&mut self, ids: &[String]) -> Result<(), Fault> {
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        self.venue
            .base
            .store
            .update_orders_status(&refs, OrderStatus::Canceled)
            .map_err(|error| Fault(error.into()))
    }
    fn place_replacement(
        &mut self,
        options: Input,
    ) -> impl Future<Output = Result<Vec<String>, Fault>> {
        let venue = self.venue.clone();
        async move {
            let options: PreparedOrderOpts = from_input(options)?;
            Ok(venue.place_order(options).await?)
        }
    }
    fn cached_tif(&mut self, client_id: Option<&Input>) -> Result<Option<Input>, Fault> {
        let Some(client) = client_id.filter(|c| !c.is_null()) else {
            return Ok(None);
        };
        let client = safe_cex_num::js_retained_string(Some(client))?;
        let state = self.venue.state.borrow();
        Ok(state
            .order_tif_cache
            .get(&client)
            .cloned()
            .map(|value| Input::Text(value.encode_utf16().collect())))
    }
    fn reduce_only(
        &mut self,
        raw: Option<&Input>,
        client_id: Option<&Input>,
    ) -> Result<bool, Fault> {
        let state = self.venue.state.borrow();
        Ok(crate::tpsl_helpers::get_bitunix_reduce_only(
            raw,
            client_id,
            false,
            &state.order_reduce_only_cache,
        )?)
    }
    fn key(&mut self, row: &Input, key: &'static str) -> Result<Option<Input>, Fault> {
        for name in [
            key.to_owned(),
            safe_cex_num::lodash_snake_case(key),
            safe_cex_num::lodash_camel_case(key),
        ] {
            if let Some(value) = row.get(&name) {
                return Ok(Some(value.clone()));
            }
            if safe_cex_num::js_table_lookup::<&str, ()>(&[], &name).is_present() {
                return Ok(None);
            }
        }
        Ok(None)
    }

    fn raw_id(&mut self, order: &Input) -> Result<String, Fault> {
        Ok(crate::order_links::get_tpsl_raw_order_id(&order_of(
            order,
        )?)?)
    }
    fn leg_id(
        &mut self,
        raw_id: &str,
        position_id: Option<&Input>,
        leg_name: &'static str,
        disambiguate: bool,
    ) -> Result<String, Fault> {
        Ok(crate::tpsl_helpers::get_tpsl_leg_order_id(
            Some(&Input::Text(raw_id.encode_utf16().collect())),
            position_id,
            leg(leg_name),
            disambiguate,
        )?)
    }
    fn clock(&mut self) -> Result<f64, Fault> {
        Ok(self.venue.runtime.now_ms())
    }
    fn fake_full_enabled(&mut self) -> Result<bool, Fault> {
        Ok(self.venue.state.borrow().enable_fake_full_tpsl)
    }
    fn attached_leg(&mut self, order: &Input) -> Result<Option<&'static str>, Fault> {
        let orders = self.venue.store_orders()?;
        Ok(crate::order_links::get_attached_virtual_tpsl_leg(
            &order_of(order)?,
            &orders,
        ))
    }
    fn standalone(&mut self, order: &Input) -> Result<bool, Fault> {
        let orders = self.venue.store_orders()?;
        Ok(crate::order_links::is_standalone_tpsl_order(
            &order_of(order)?,
            &orders,
        ))
    }
    fn position_order(&mut self, order: &Input) -> Result<bool, Fault> {
        Ok(crate::order_links::is_position_tpsl_order(&order_of(
            order,
        )?)?)
    }
    fn snapshot(
        &mut self,
        order: &Input,
        leg_name: &'static str,
    ) -> Result<Option<crate::order_state::TpslLegSnapshot>, Fault> {
        let orders = self.venue.store_orders()?;
        Ok(crate::order_state::get_tpsl_leg_snapshot(
            &order_of(order)?,
            leg(leg_name),
            &orders,
        )?)
    }
    fn trigger(&mut self, trigger: Option<&Input>, raw: Option<&Input>) -> Result<String, Fault> {
        let trigger = trigger
            .filter(|t| !t.is_null())
            .and_then(|t| from_input::<OrderStopTriggerType>(t.clone()).ok());
        Ok(crate::tpsl_helpers::get_bitunix_trigger_type(trigger, raw)?.into())
    }
    fn update_standalone(
        &mut self,
        order: &Input,
        update: &Input,
    ) -> impl Future<Output = Result<Vec<String>, Fault>> {
        let venue = self.venue.clone();
        let (order, update) = (order.clone(), update.clone());
        async move {
            crate::order_models::update_standalone_tpsl_order(
                &mut venue.order_frame(),
                &order,
                &update,
            )
            .await
        }
    }
}

struct PlaceFrame {
    venue: Rc<BitunixVenue>,
}
impl crate::order_placement::PlaceOrderHost for PlaceFrame {
    type Options = PreparedOrderOpts;
    type Payloads = Vec<JsJsonValue>;
    type Positions = Vec<Position>;
    type Result = Vec<String>;
    type Message = String;
    type Error = ExchangeError;
    fn resolve_caller(&mut self, options: &PreparedOrderOpts) -> Result<(), ExchangeError> {
        self.venue
            .base
            .resolve_caller_client_order_id(options)
            .map(drop)
    }
    fn hydrate(
        &mut self,
        options: &PreparedOrderOpts,
    ) -> impl Future<Output = Result<(), ExchangeError>> {
        self.venue.hydrate(options.clone())
    }
    fn format(&mut self, options: &PreparedOrderOpts) -> Result<Vec<JsJsonValue>, ExchangeError> {
        crate::order_models::format_create_order(&mut self.venue.order_frame(), &to_input(options)?)
            .map_err(|e| e.0)?
            .into_iter()
            .map(request_payload)
            .collect()
    }
    fn batch(
        &mut self,
        payloads: Vec<JsJsonValue>,
    ) -> impl Future<Output = Result<Vec<String>, ExchangeError>> {
        self.venue.place_order_batch(payloads)
    }
    fn error_message(&mut self, error: &ExchangeError) -> Result<String, ExchangeError> {
        // err?.message || err?.response?.data?.msg || ''
        Ok(if error.message.is_empty() {
            reason(error)
        } else {
            error.message.clone()
        })
    }
    fn includes(&mut self, message: &String, needle: &'static str) -> Result<bool, ExchangeError> {
        Ok(message.contains(needle))
    }
    fn lowercase(&mut self, message: &String) -> Result<String, ExchangeError> {
        Ok(message.to_lowercase())
    }
    fn message_string(&mut self, message: &String) -> Result<Vec<u16>, ExchangeError> {
        Ok(units(message))
    }
    fn warn(&mut self, message: Vec<u16>) -> Result<(), ExchangeError> {
        self.venue.runtime.console("warn", &utf8(&message)?);
        Ok(())
    }
    fn symbol(&mut self, options: &PreparedOrderOpts) -> Result<Vec<u16>, ExchangeError> {
        Ok(units(&options.symbol))
    }
    fn delete_position_id(&mut self, key: Vec<u16>) -> Result<(), ExchangeError> {
        let key = utf8(&key)?;
        self.venue.state.borrow_mut().position_id_cache.remove(&key);
        Ok(())
    }
    fn fetch_positions(&mut self) -> impl Future<Output = Result<Vec<Position>, ExchangeError>> {
        self.venue.fetch_positions()
    }
    fn update_positions(&mut self, positions: Vec<Position>) -> Result<(), ExchangeError> {
        self.venue.position_update(positions)
    }
}

type V = Rc<JsJsonValue>;
struct BatchFrame {
    venue: Rc<BitunixVenue>,
    payloads: Vec<V>,
}
fn get(value: &JsJsonValue, key: &str) -> JsJsonValue {
    match value {
        JsJsonValue::Object(fields) => fields
            .iter()
            .find(|(k, _)| k == key)
            .map_or(JsJsonValue::Undefined, |(_, v)| v.clone()),
        _ => JsJsonValue::Undefined,
    }
}
impl crate::order_placement::BatchOrderHost for BatchFrame {
    type Value = V;
    type Iterator = (Vec<V>, usize);
    type Error = ExchangeError;
    fn input_length(&mut self) -> Result<usize, ExchangeError> {
        Ok(self.payloads.len())
    }
    fn input_at(&mut self, index: usize) -> Result<Option<V>, ExchangeError> {
        Ok(self.payloads.get(index).cloned())
    }
    fn field(&mut self, value: &V, path: &[&str]) -> Result<V, ExchangeError> {
        let mut current = value.as_ref().clone();
        for key in path {
            current = get(&current, key);
        }
        Ok(Rc::new(current))
    }
    fn truthy(&self, value: &V) -> bool {
        js_truth(value)
    }
    fn zero(&self, value: &V) -> bool {
        matches!(value.as_ref(), JsJsonValue::Number(n) if *n == 0.0)
    }
    fn limit(&self, value: &V) -> bool {
        matches!(value.as_ref(), JsJsonValue::String(s) if s == "LIMIT")
    }
    fn has_own(&mut self, value: &V, key: &'static str) -> Result<bool, ExchangeError> {
        Ok(matches!(value.as_ref(), JsJsonValue::Object(f) if f.iter().any(|(k, _)| k == key)))
    }
    fn identical(&self, left: &V, right: &V) -> bool {
        Rc::ptr_eq(left, right)
    }
    fn symbol_key(&mut self, value: &V) -> Result<Option<Vec<u16>>, ExchangeError> {
        Ok(Some(units(&js_text(&get(value, "symbol")))))
    }
    fn omit(&mut self, value: &V, key: &'static str) -> Result<V, ExchangeError> {
        Ok(Rc::new(match value.as_ref() {
            JsJsonValue::Object(fields) => {
                JsJsonValue::Object(fields.iter().filter(|(k, _)| k != key).cloned().collect())
            }
            other => other.clone(),
        }))
    }
    fn batch_payload(&mut self, symbol: Vec<u16>, rows: Vec<V>) -> Result<V, ExchangeError> {
        Ok(Rc::new(JsJsonValue::Object(vec![
            ("symbol".into(), JsJsonValue::String(utf8(&symbol)?)),
            (
                "orderList".into(),
                JsJsonValue::Array(rows.iter().map(|r| r.as_ref().clone()).collect()),
            ),
        ])))
    }
    fn post(
        &mut self,
        endpoint: &'static str,
        payload: V,
    ) -> impl Future<Output = Result<V, ExchangeError>> {
        let f = self
            .venue
            .unlimited()
            .post(endpoint, payload.as_ref().clone());
        async move { Ok(Rc::new(JsJsonValue::from(f.await?))) }
    }
    fn sleep(&mut self, delay: f64) -> impl Future<Output = Result<(), ExchangeError>> {
        self.venue.runtime.sleep(delay as u64)
    }
    fn uuid(&mut self) -> Result<V, ExchangeError> {
        Ok(Rc::new(JsJsonValue::String(
            self.venue.runtime.unique_id().replace(['-', '_'], ""),
        )))
    }
    fn string(&mut self, text: Vec<u16>) -> Result<V, ExchangeError> {
        Ok(Rc::new(JsJsonValue::String(utf8(&text)?)))
    }
    fn to_string(&mut self, value: &V) -> Result<Vec<u16>, ExchangeError> {
        Ok(units(&js_text(value)))
    }
    fn error(&mut self, message: V) -> Result<ExchangeError, ExchangeError> {
        Ok(ExchangeError::new(js_text(&message)))
    }
    fn report(&mut self, error: &ExchangeError) -> Result<(), ExchangeError> {
        self.venue.emit_error(&reason(error))
    }
    fn emit(&mut self, message: V) -> Result<(), ExchangeError> {
        self.venue.emit_error(&js_text(&message))
    }
    fn is_array(&self, value: &V) -> bool {
        matches!(value.as_ref(), JsJsonValue::Array(_))
    }
    fn array_length(&mut self, value: &V) -> Result<usize, ExchangeError> {
        match value.as_ref() {
            JsJsonValue::Array(items) => Ok(items.len()),
            _ => Err(ExchangeError::new("Expected array")),
        }
    }
    fn array_at(&mut self, value: &V, index: usize) -> Result<Option<V>, ExchangeError> {
        match value.as_ref() {
            JsJsonValue::Array(items) => Ok(items.get(index).cloned().map(Rc::new)),
            _ => Err(ExchangeError::new("Expected array")),
        }
    }
    fn array(&mut self, values: Vec<V>) -> Result<V, ExchangeError> {
        Ok(Rc::new(JsJsonValue::Array(
            values.iter().map(|v| v.as_ref().clone()).collect(),
        )))
    }
    fn iterator(&mut self, value: V) -> Result<(Vec<V>, usize), ExchangeError> {
        match value.as_ref() {
            JsJsonValue::Array(items) => Ok((items.iter().cloned().map(Rc::new).collect(), 0)),
            _ => Err(ExchangeError::new("Expected iterable array")),
        }
    }
    fn next(&mut self, iterator: &mut (Vec<V>, usize)) -> Result<Option<V>, ExchangeError> {
        let item = iterator.0.get(iterator.1).cloned();
        iterator.1 += 1;
        Ok(item)
    }
    fn nullish(&self, value: &V) -> bool {
        matches!(value.as_ref(), JsJsonValue::Undefined | JsJsonValue::Null)
    }
}

struct CancelFrame {
    venue: Rc<BitunixVenue>,
}
fn kind_name(kind: OrderType) -> &'static str {
    match kind {
        OrderType::Market => "market",
        OrderType::Limit => "limit",
        OrderType::Conditional => "conditional",
        OrderType::StopLoss => "stop_market",
        OrderType::TakeProfit => "take_profit_market",
        OrderType::TrailingStopLoss => "trailing_stop_market",
    }
}
impl crate::order_cancellation::CancellationHost for CancelFrame {
    type Input = Vec<Order>;
    type Order = Order;
    type Body = JsJsonValue;
    type Options = PreparedOrderOpts;
    type Error = ExchangeError;
    fn input(&mut self, input: &Vec<Order>) -> Result<Vec<Order>, ExchangeError> {
        Ok(input.clone())
    }
    fn orders(&mut self) -> Result<Vec<Order>, ExchangeError> {
        self.venue.store_orders()
    }
    fn attached_leg(&mut self, order: &Order) -> Result<Option<&'static str>, ExchangeError> {
        let orders = self.venue.store_orders()?;
        Ok(crate::order_links::get_attached_virtual_tpsl_leg(
            order, &orders,
        ))
    }
    fn id(&mut self, order: &Order) -> Result<Vec<u16>, ExchangeError> {
        Ok(units(&order.id))
    }
    fn parent_id(&mut self, id: Vec<u16>) -> Result<Vec<u16>, ExchangeError> {
        Ok(units(crate::order_ids::get_virtual_tpsl_parent_order_id(
            &utf8(&id)?,
        )))
    }
    fn kind(&mut self, order: &Order) -> Result<Option<&'static str>, ExchangeError> {
        Ok(order.r#type.into_value().map(kind_name))
    }
    fn symbol(&mut self, order: &Order) -> Result<Vec<u16>, ExchangeError> {
        Ok(units(&order.symbol))
    }
    fn parent_body(&mut self, parent: &Order, id: &[u16]) -> Result<JsJsonValue, ExchangeError> {
        Ok(JsJsonValue::from(json!({
            "symbol": parent.symbol,
            "orderList": [{ "orderId": utf8(id)? }],
        })))
    }
    fn standalone_body(&mut self, order: &Order) -> Result<JsJsonValue, ExchangeError> {
        let raw = crate::order_links::get_tpsl_raw_order_id(order).map_err(coercion)?;
        Ok(JsJsonValue::from(
            json!({ "symbol": order.symbol, "orderId": raw }),
        ))
    }
    fn group_body(
        &mut self,
        symbol: &[u16],
        orders: &[Order],
    ) -> Result<JsJsonValue, ExchangeError> {
        Ok(JsJsonValue::from(json!({
            "symbol": utf8(symbol)?,
            "orderList": orders.iter().map(|o| json!({ "orderId": o.id })).collect::<Vec<_>>(),
        })))
    }
    fn post(
        &mut self,
        endpoint: &'static str,
        body: JsJsonValue,
    ) -> impl Future<Output = Result<(), ExchangeError>> {
        let f = self.venue.xhr().post(endpoint, body);
        async move { f.await.map(drop) }
    }
    fn update_status(
        &mut self,
        ids: Vec<Vec<u16>>,
        status: &'static str,
    ) -> Result<(), ExchangeError> {
        let ids = ids
            .iter()
            .map(|id| utf8(id))
            .collect::<Result<Vec<_>, _>>()?;
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let status: OrderStatus = from_value(Value::String(status.into()))?;
        self.venue
            .base
            .store
            .update_orders_status(&refs, status)
            .map_err(Into::into)
    }
    fn recreate_options(
        &mut self,
        parent: &Order,
        tp: Option<&Order>,
        sl: Option<&Order>,
    ) -> Result<PreparedOrderOpts, ExchangeError> {
        Ok(PreparedOrderOpts {
            symbol: parent.symbol.clone(),
            r#type: parent.r#type.into_property(),
            side: parent.side.into_property(),
            price: parent.price.into_value(),
            amount: parent.amount,
            reduce_only: parent.reduce_only.into_property(),
            take_profit: tp.and_then(|order| order.price.into_value()),
            stop_loss: sl.and_then(|order| order.price.into_value()),
            tp_trigger_type: tp.and_then(|order| order.tp_trigger_type.into_value()),
            sl_trigger_type: sl.and_then(|order| order.sl_trigger_type.into_value()),
            ..Default::default()
        })
    }
    fn place(
        &mut self,
        options: PreparedOrderOpts,
    ) -> impl Future<Output = Result<(), ExchangeError>> {
        let f = self.venue.place_order(options);
        async move { f.await.map(drop) }
    }
    fn report(&mut self, error: &ExchangeError) -> Result<(), ExchangeError> {
        self.venue.emit_error(&reason(error))
    }
}

impl BitunixVenue {
    pub(crate) fn hydrate(
        self: &Rc<Self>,
        opts: impl Into<PreparedOrderOpts>,
    ) -> ExchangeFuture<'static, ()> {
        let opts = opts.into();
        let me = self.clone();
        Box::pin(async move {
            let positions = me.base.store.positions()?.to_vec();
            let cache: std::collections::HashMap<String, Value> = me
                .state
                .borrow()
                .position_id_cache
                .iter()
                .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                .collect();
            let (fetch_p, update_p, fetch_o, update_o) =
                (me.clone(), me.clone(), me.clone(), me.clone());
            crate::order_state::hydrate_position_id_before_formatting(
                &opts.symbol,
                opts.r#type,
                opts.side,
                opts.amount,
                opts.reduce_only.into_value(),
                &positions,
                &cache,
                move || fetch_p.fetch_positions(),
                move |p| update_p.position_update(p),
                move || fetch_o.fetch_orders(),
                move |o| update_o.order_update(o),
            )
            .await
        })
    }
    pub(crate) fn place_order_batch(
        self: &Rc<Self>,
        payloads: Vec<JsJsonValue>,
    ) -> ExchangeFuture<'static, Vec<String>> {
        let mut frame = BatchFrame {
            venue: self.clone(),
            payloads: payloads.into_iter().map(Rc::new).collect(),
        };
        Box::pin(async move {
            let ids = crate::order_placement::place_order_batch(&mut frame).await?;
            Ok(ids.iter().map(|id| js_text(id)).collect())
        })
    }
    /// placeOrder: the caller clientOrderId guard is the body's first statement.
    pub(crate) fn place_order(
        self: &Rc<Self>,
        opts: impl Into<PreparedOrderOpts>,
    ) -> ExchangeFuture<'static, Vec<String>> {
        let opts = opts.into();
        let mut frame = PlaceFrame {
            venue: self.clone(),
        };
        Box::pin(async move { crate::order_placement::place_order(&mut frame, opts).await })
    }
    pub(crate) fn place_orders(
        self: &Rc<Self>,
        orders: Vec<PlaceOrderOpts>,
    ) -> ExchangeFuture<'static, Vec<String>> {
        let me = self.clone();
        Box::pin(async move {
            let orders = RefCell::new(orders);
            crate::rest_writes::place_orders(
                || me.base.assert_caller_client_order_ids(&orders.borrow()),
                || Ok(orders.borrow().len()),
                |i| Ok(orders.borrow().get(i).cloned()),
                |o: Option<PlaceOrderOpts>| {
                    let f = o.map(|o| me.hydrate(o));
                    async move {
                        match f {
                            Some(f) => f.await,
                            None => Ok(()),
                        }
                    }
                },
                |o| {
                    crate::order_models::format_create_order(&mut me.order_frame(), &to_input(&o)?)
                        .map_err(|e| e.0)?
                        .into_iter()
                        .map(request_payload)
                        .collect()
                },
                |payloads| me.place_order_batch(payloads),
            )
            .await
        })
    }
    pub(crate) fn cancel_orders(
        self: &Rc<Self>,
        orders: Vec<Order>,
    ) -> ExchangeFuture<'static, ()> {
        let mut frame = CancelFrame {
            venue: self.clone(),
        };
        Box::pin(async move { crate::order_cancellation::cancel_orders(&mut frame, orders).await })
    }
    pub(crate) fn cancel_symbol_orders(
        self: &Rc<Self>,
        symbol: String,
    ) -> ExchangeFuture<'static, ()> {
        let me = self.clone();
        Box::pin(async move {
            let orders = me.store_orders()?;
            crate::rest_writes::cancel_symbol_orders(
                &symbol,
                || Ok(orders.len()),
                |i| Ok(orders.get(i).cloned()),
                |o: &Order, s: &String| Ok(&o.symbol == s),
                |selected| me.cancel_orders(selected),
            )
            .await
        })
    }
    pub(crate) fn update_order(
        self: &Rc<Self>,
        options: UpdateOrderOpts,
    ) -> ExchangeFuture<'static, Vec<String>> {
        let me = self.clone();
        Box::pin(async move {
            let order = to_input(&options.order)?;
            let update = to_input(&options.update)?;
            crate::order_models::update_order(&mut me.order_frame(), &order, &update)
                .await
                .map_err(|e| e.0)
        })
    }
}
