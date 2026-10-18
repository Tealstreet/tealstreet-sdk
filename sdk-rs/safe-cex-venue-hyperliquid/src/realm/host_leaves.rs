//! Leaf JS host traits: operators, bindings, iteration and callback captures.
//! Every capture becomes a native function whose body is the venue's mapped
//! `run_*` algorithm; no algorithm lives here.
use super::{
    Realm,
    host_core::js,
    value::{IterSource, PropKey, Value},
};
use crate::{
    account_position_leaves::{self, AccountPositionHost, OppositePredicate},
    balance_state::{self, BalanceHost, BalanceOperator, SpotMarketPredicate},
    cancellation::{self, CancelCallback, CancellationHost},
    candle_listeners::{self, CandleCallback, CandleListenerHost},
    core_account_snapshots::{self, CoreSnapshotHost, SnapshotPredicate},
    core_book_listener::{self, CoreBookCallback, CoreBookHost},
    core_exchange_leaves::{self, CoreExchangeLeafHost, ExchangePredicate},
    core_position_order_state::{self, PositionCallback, PositionOrderHost},
    core_public_message::CorePublicMessageHost,
    core_rest_reads::{self, RestCallback, RestReadHost},
    hip3_position_refresh::{self, PositionRefreshCallback, PositionRefreshHost},
    lifecycle_delegates::{self, LifecycleCallback, LifecycleCallbackKind, LifecycleHost},
    limit_orders::LimitOrderHost,
    market_bootstrap::{self, MarketBootstrapCallback, MarketBootstrapHost},
    market_data_selectors::MarketDataHost,
    order_reads::{self, OrderReadCallback, OrderReadHost},
    order_request_leaves::{OrderRequestHost, RequestComparison},
    order_retry::{self, OrderRetryCallback, OrderRetryHost},
    order_update::{self, OrderUpdateHost, UpdateCallback},
    placement_dispatch::{self, PlacementHost, SerialOrderCallback},
    private_message_dispatch::{self, DispatchOperator, PrivateMessageHost},
    private_position_helpers::PrivatePositionHost,
    spot_json::JsString,
    spot_market_bootstrap::SpotMarketBootstrapHost,
    startup_lifecycle::{self, StartupCallback, StartupLifecycleHost},
    ws_emission::{self, DiagnosticScope, WsEmissionHost},
    ws_transport_leaves::{WsTransportHost, WsVenue},
};
use std::future::Future;

fn first(args: &[Value]) -> Value {
    args.first().cloned().unwrap_or(Value::Undefined)
}
fn pair(v: Vec<Value>) -> [Value; 2] {
    [v[0].clone(), v[1].clone()]
}
fn bool_of(r: &mut Realm, op: &str, a: &Value, b: &Value) -> Result<bool, Value> {
    Ok(Realm::binary(r, op, a, b)?.truthy())
}
fn table(r: &Realm, rows: &[(&str, &str)]) -> Value {
    r.object_from(rows.iter().map(|(k, v)| (*k, Value::str(v))).collect())
}
fn strings(r: &Realm, items: &[&str]) -> Value {
    r.new_array(items.iter().map(|s| Value::str(s)).collect())
}

impl AccountPositionHost for Realm {
    fn null(&self) -> Value {
        Value::Null
    }
    fn boolean(&self, value: bool) -> Value {
        Value::Bool(value)
    }
    fn number(&self, value: f64) -> Value {
        Value::Number(value)
    }
    fn is_nullish(&self, value: &Value) -> bool {
        value.is_nullish()
    }
    fn strict_equal(&self, left: &Value, right: &Value) -> bool {
        left.strict_equal(right)
    }
    fn greater_equal(&mut self, left: &Value, right: &Value) -> Result<bool, Value> {
        bool_of(self, ">=", left, right)
    }
    fn opposite_predicate(&mut self, capture: OppositePredicate<Value>) -> Result<Value, Value> {
        Ok(self.native("", 1, move |r, _, args| {
            account_position_leaves::run_opposite_predicate(r, &capture, &first(args))
                .map(Value::Bool)
        }))
    }
}

impl BalanceHost for Realm {
    fn null(&self) -> Value {
        Value::Null
    }
    fn number(&self, v: f64) -> Value {
        Value::Number(v)
    }
    fn boolean(&self, v: bool) -> Value {
        Value::Bool(v)
    }
    fn is_nullish(&self, v: &Value) -> bool {
        v.is_nullish()
    }
    fn is_undefined(&self, v: &Value) -> bool {
        matches!(v, Value::Undefined)
    }
    fn strict_equal(&self, a: &Value, b: &Value) -> bool {
        a.strict_equal(b)
    }
    fn index(&mut self, o: &Value, key: &Value) -> Result<Value, Value> {
        Realm::index(self, o, key)
    }
    fn set_index(&mut self, o: &Value, key: &Value, v: &Value) -> Result<(), Value> {
        Realm::set_index(self, o, key, v.clone())
    }
    fn define_index(&mut self, o: &Value, key: &Value, v: &Value) -> Result<(), Value> {
        let key = self.to_property_key(key)?;
        self.define(o, key, v.clone())
    }
    fn array(&mut self) -> Result<Value, Value> {
        Ok(self.new_array(vec![]))
    }
    fn array_define(&mut self, o: &Value, i: usize, v: &Value) -> Result<(), Value> {
        self.define(o, PropKey::str(&i.to_string()), v.clone())
    }
    fn iterator(&mut self, v: &Value) -> Result<Value, Value> {
        self.get_iterator(v)
    }
    fn next(&mut self, i: &Value) -> Result<Option<Value>, Value> {
        self.iterator_step(i)
    }
    fn close_after_throw(&mut self, i: &Value) {
        self.iterator_close_after_throw(i)
    }
    fn close_after_return(&mut self, i: &Value) -> Result<(), Value> {
        self.iterator_close(i)
    }
    fn pair(&mut self, v: &Value) -> Result<[Value; 2], Value> {
        Ok(pair(self.array_slots(v, 2)?))
    }
    fn template_string(&mut self, v: &Value) -> Result<JsString, Value> {
        Ok(js(self.template(v)?))
    }
    fn operator(&mut self, op: BalanceOperator, a: &Value, b: &Value) -> Result<Value, Value> {
        let op = match op {
            BalanceOperator::Add => "+",
            BalanceOperator::Subtract => "-",
            BalanceOperator::Multiply => "*",
            BalanceOperator::Greater => ">",
        };
        Realm::binary(self, op, a, b)
    }
    fn spread_into(&mut self, o: &Value, source: &Value) -> Result<(), Value> {
        self.copy_data_properties(o, source)
    }
    fn spot_market_predicate(
        &mut self,
        capture: SpotMarketPredicate<Value>,
    ) -> Result<Value, Value> {
        Ok(self.native("", 1, move |r, _, args| {
            balance_state::run_spot_market_predicate(r, &capture, &first(args)).map(Value::Bool)
        }))
    }
}

impl CancellationHost for Realm {
    fn unary_plus(&mut self, v: &Value) -> Result<Value, Value> {
        self.unary("+", v)
    }
    fn instance_of(&mut self, v: &Value, class: &Value) -> Result<bool, Value> {
        Realm::instance_of(self, v, class)
    }
    fn iterator(&mut self, v: &Value) -> Result<Value, Value> {
        self.get_iterator(v)
    }
    fn next(&mut self, v: &Value) -> Result<Option<Value>, Value> {
        self.iterator_step(v)
    }
    fn close_after_throw(&mut self, v: &Value) {
        self.iterator_close_after_throw(v)
    }
    fn cancel_callback(&mut self, c: CancelCallback<Value>) -> Result<Value, Value> {
        Ok(self.native("", 1, move |r, _, args| {
            cancellation::run_cancel_callback(r, &c, args)
        }))
    }
}

impl CandleListenerHost for Realm {
    fn number(&self, value: f64) -> Value {
        Value::Number(value)
    }
    fn boolean(&self, value: bool) -> Value {
        Value::Bool(value)
    }
    fn nullish(&self, value: &Value) -> bool {
        value.is_nullish()
    }
    fn strict_equal(&self, left: &Value, right: &Value) -> bool {
        left.strict_equal(right)
    }
    fn binary(&mut self, operator: &str, left: &Value, right: &Value) -> Result<Value, Value> {
        Realm::binary(self, operator, left, right)
    }
    fn decrement(&mut self, value: &Value) -> Result<Value, Value> {
        Ok(Value::Number(self.to_number(value)? - 1.0))
    }
    fn template_string(&mut self, value: &Value) -> Result<JsString, Value> {
        Ok(js(self.template(value)?))
    }
    fn index(&mut self, object: &Value, key: &Value) -> Result<Value, Value> {
        Realm::index(self, object, key)
    }
    fn assign_index(&mut self, object: &Value, key: &Value, value: &Value) -> Result<(), Value> {
        Realm::set_index(self, object, key, value.clone())
    }
    fn array(&mut self, values: &[Value]) -> Result<Value, Value> {
        Ok(self.new_array(values.to_vec()))
    }
    fn spread_append(&mut self, iterable: &Value, last: &Value) -> Result<Value, Value> {
        Realm::spread_append(self, iterable, last)
    }
    fn spread_array(&mut self, iterable: &Value) -> Result<Value, Value> {
        self.spread_to_array(iterable)
    }
    fn candle_callback(&mut self, capture: CandleCallback<Value>) -> Result<Value, Value> {
        Ok(self.native("", 1, move |r, _, args| {
            candle_listeners::run_candle_callback(r, &capture, args)
        }))
    }
}

impl CoreBookHost for Realm {
    fn imported(&mut self, name: &str) -> Result<Value, Value> {
        self.import(name)
    }
    fn spread_object(&mut self, source: &Value) -> Result<Value, Value> {
        Realm::spread_object(self, source)
    }
    fn delete_index(&mut self, object: &Value, key: &Value) -> Result<(), Value> {
        let key = self.to_property_key(key)?;
        Realm::delete(self, object, &key).map(|_| ())
    }
    fn construct(&mut self, constructor: &Value, args: &[Value]) -> Result<Value, Value> {
        Realm::construct(self, constructor, args, None)
    }
    fn throw_value(&self, value: Value) -> Value {
        value
    }
    fn book_callback(&mut self, capture: CoreBookCallback<Value>) -> Result<Value, Value> {
        Ok(self.native("", 1, move |r, _, args| {
            core_book_listener::run_core_book_callback(r, &capture, args)
        }))
    }
}

impl CoreExchangeLeafHost for Realm {
    fn typeof_value(&self, value: &Value) -> &'static str {
        value.type_of()
    }
    fn finite_positive_predicate(&mut self) -> Result<Value, Value> {
        Ok(self.singleton("finitePositivePredicate", |r| {
            r.native("", 1, |r, _, args| {
                core_exchange_leaves::run_exchange_predicate(
                    r,
                    &ExchangePredicate::FinitePositive,
                    &first(args),
                )
            })
        }))
    }
    fn exchange_predicate(&mut self, capture: ExchangePredicate<Value>) -> Result<Value, Value> {
        Ok(self.native("", 1, move |r, _, args| {
            core_exchange_leaves::run_exchange_predicate(r, &capture, &first(args))
        }))
    }
}

impl CoreSnapshotHost for Realm {
    fn null_value(&self) -> Value {
        Value::Null
    }
    fn add_values(&mut self, a: &Value, b: &Value) -> Result<Value, Value> {
        self.add(a, b)
    }
    fn return_await_value(&mut self, value: Value) -> impl Future<Output = Result<Value, Value>> {
        Realm::await_value(self, value)
    }
    fn await_value(&mut self, value: Value) -> impl Future<Output = Result<Value, Value>> {
        Realm::await_value(self, value)
    }
    fn error_value(&self, error: &Value) -> Value {
        error.clone()
    }
    fn construct_error(&mut self, constructor: &Value, args: &[Value]) -> Result<Value, Value> {
        Realm::construct(self, constructor, args, None)
    }
    fn instance_of(&mut self, value: &Value, constructor: &Value) -> Result<bool, Value> {
        Realm::instance_of(self, value, constructor)
    }
    fn snapshot_predicate(&mut self, capture: SnapshotPredicate<Value>) -> Result<Value, Value> {
        Ok(self.native("", 1, move |r, _, args| {
            core_account_snapshots::run_snapshot_predicate(r, &capture, &first(args))
        }))
    }
    fn array_define(&mut self, array: &Value, index: usize, value: &Value) -> Result<(), Value> {
        self.define(array, PropKey::str(&index.to_string()), value.clone())
    }
    fn spread_into(&mut self, object: &Value, source: &Value) -> Result<(), Value> {
        self.copy_data_properties(object, source)
    }
    fn iterator(&mut self, value: &Value) -> Result<Value, Value> {
        self.get_iterator(value)
    }
    fn next(&mut self, iterator: &Value) -> Result<Option<Value>, Value> {
        self.iterator_step(iterator)
    }
    fn close_after_throw(&mut self, iterator: &Value) {
        self.iterator_close_after_throw(iterator)
    }
    fn close_after_return(&mut self, iterator: &Value) -> Result<(), Value> {
        self.iterator_close(iterator)
    }
    fn pair(&mut self, value: &Value) -> Result<[Value; 2], Value> {
        Ok(pair(self.array_slots(value, 2)?))
    }
}

impl CorePublicMessageHost for Realm {
    fn event_data(&mut self, event: &Value) -> Result<Value, Value> {
        self.data_binding(event)
    }
    fn is_nullish(&self, value: &Value) -> bool {
        value.is_nullish()
    }
    fn is_array(&mut self, value: &Value) -> Result<bool, Value> {
        Ok(value.is_array())
    }
    fn equal(&self, left: &Value, right: &Value) -> bool {
        left.strict_equal(right)
    }
    fn number(&self, value: f64) -> Value {
        Value::Number(value)
    }
    fn less(&mut self, left: &Value, right: &Value) -> Result<bool, Value> {
        bool_of(self, "<", left, right)
    }
    fn subtract(&mut self, left: &Value, right: &Value) -> Result<Value, Value> {
        Realm::binary(self, "-", left, right)
    }
    fn divide(&mut self, left: &Value, right: &Value) -> Result<Value, Value> {
        Realm::binary(self, "/", left, right)
    }
    fn index(&mut self, value: &Value, key: &Value) -> Result<Value, Value> {
        Realm::index(self, value, key)
    }
    fn array(&mut self) -> Result<Value, Value> {
        Ok(self.new_array(vec![]))
    }
    fn template_string(&mut self, value: &Value) -> Result<JsString, Value> {
        Ok(js(self.template(value)?))
    }
    fn json_parse(&mut self, data: &Value) -> Result<Value, Value> {
        let parse = self.import("jsonParse")?;
        Realm::call(self, &parse, &Value::Undefined, &[data.clone()])
    }
}

impl LifecycleHost for Realm {
    fn null(&self) -> Value {
        Value::Null
    }
    fn boolean(&self, value: bool) -> Value {
        Value::Bool(value)
    }
    fn number(&self, value: f64) -> Value {
        Value::Number(value)
    }
    fn is_nullish(&self, value: &Value) -> bool {
        value.is_nullish()
    }
    /// The home object is the lexical class's prototype.
    fn super_get(&mut self, receiver: &Value, name: &str) -> Result<Value, Value> {
        let class = self
            .lexical
            .clone()
            .ok_or_else(|| self.error("'super' keyword unexpected here"))?;
        let home = self.get_str(&class, "prototype")?;
        Realm::super_get(self, &home, receiver, name)
    }
    fn lifecycle_callback(&mut self, capture: LifecycleCallback<Value>) -> Result<Value, Value> {
        Ok(self.native("", 1, move |r, _, args| {
            match capture.kind {
                LifecycleCallbackKind::OrderRefreshTimeout => {
                    lifecycle_delegates::run_order_refresh_timeout(r, &capture.exchange)?
                }
                LifecycleCallbackKind::OrderRefreshError => {
                    lifecycle_delegates::run_order_refresh_error(
                        r,
                        &capture.exchange,
                        &first(args),
                    )?
                }
            }
            Ok(Value::Undefined)
        }))
    }
    fn subtract(&mut self, left: &Value, right: &Value) -> Result<Value, Value> {
        Realm::binary(self, "-", left, right)
    }
    fn greater_than(&mut self, left: &Value, right: &Value) -> Result<bool, Value> {
        bool_of(self, ">", left, right)
    }
    fn template_string(&mut self, value: &Value) -> Result<JsString, Value> {
        Ok(js(self.template(value)?))
    }
}

impl LimitOrderHost for Realm {
    fn unary_plus(&mut self, v: &Value) -> Result<Value, Value> {
        self.unary("+", v)
    }
    fn first_binding(&mut self, v: &Value) -> Result<Value, Value> {
        Ok(self.array_slots(v, 1)?.remove(0))
    }
    fn iterator(&mut self, v: &Value) -> Result<Value, Value> {
        self.get_iterator(v)
    }
    fn next(&mut self, v: &Value) -> Result<Option<Value>, Value> {
        self.iterator_step(v)
    }
    fn close_after_throw(&mut self, v: &Value) {
        self.iterator_close_after_throw(v)
    }
}

impl MarketBootstrapHost for Realm {
    fn bootstrap_callback(
        &mut self,
        callback: MarketBootstrapCallback<Value>,
    ) -> Result<Value, Value> {
        Ok(self.native("", 1, move |r, _, args| {
            market_bootstrap::run_market_bootstrap_callback(r, &callback, args)
        }))
    }
    fn array_binding(&mut self, value: &Value, count: usize) -> Result<Vec<Value>, Value> {
        self.array_slots(value, count)
    }
    fn spread_arguments(&mut self, args: &mut Vec<Value>, value: &Value) -> Result<(), Value> {
        self.spread_args(args, value)
    }
    fn iterator(&mut self, value: &Value) -> Result<Value, Value> {
        self.get_iterator(value)
    }
    fn next(&mut self, iterator: &Value) -> Result<Option<Value>, Value> {
        self.iterator_step(iterator)
    }
    fn close_after_throw(&mut self, iterator: &Value) {
        self.iterator_close_after_throw(iterator)
    }
    fn close_after_break(&mut self, iterator: &Value) -> Result<(), Value> {
        self.iterator_close(iterator)
    }
}

impl MarketDataHost for Realm {
    fn null(&self) -> Value {
        Value::Null
    }
    fn number(&self, value: f64) -> Value {
        Value::Number(value)
    }
    fn is_nullish(&self, value: &Value) -> bool {
        value.is_nullish()
    }
    fn is_string(&self, value: &Value) -> bool {
        matches!(value, Value::String(_))
    }
    fn strict_equal(&self, left: &Value, right: &Value) -> bool {
        left.strict_equal(right)
    }
    fn index(&mut self, object: &Value, key: &Value) -> Result<Value, Value> {
        Realm::index(self, object, key)
    }
    fn iterator(&mut self, value: &Value) -> Result<Value, Value> {
        self.get_iterator(value)
    }
    fn next(&mut self, iterator: &Value) -> Result<Option<Value>, Value> {
        self.iterator_step(iterator)
    }
    fn close_after_throw(&mut self, iterator: &Value) {
        self.iterator_close_after_throw(iterator)
    }
    fn construct(&mut self, function: &Value, args: &[Value]) -> Result<Value, Value> {
        Realm::construct(self, function, args, None)
    }
    fn first_binding(&mut self, value: &Value) -> Result<Value, Value> {
        Ok(self.array_slots(value, 1)?.remove(0))
    }
    fn unit_regex(&mut self) -> Result<Value, Value> {
        self.regexp("/^U[A-Z0-9]/")
    }
    fn template_string(&mut self, value: &Value) -> Result<JsString, Value> {
        Ok(js(self.template(value)?))
    }
    fn subtract(&mut self, left: &Value, right: &Value) -> Result<Value, Value> {
        Realm::binary(self, "-", left, right)
    }
}

impl OrderReadHost for Realm {
    fn order_read_callback(&mut self, c: OrderReadCallback<Value>) -> Result<Value, Value> {
        Ok(self.native("", 1, move |r, _, args| {
            order_reads::run_order_read_callback(r, &c, args)
        }))
    }
    fn iterator(&mut self, v: &Value) -> Result<Value, Value> {
        self.get_iterator(v)
    }
    fn next(&mut self, v: &Value) -> Result<Option<Value>, Value> {
        self.iterator_step(v)
    }
    fn close_after_throw(&mut self, v: &Value) {
        self.iterator_close_after_throw(v)
    }
    fn first_binding(&mut self, v: &Value) -> Result<Value, Value> {
        Ok(self.array_slots(v, 1)?.remove(0))
    }
    fn typeof_value(&self, v: &Value) -> &'static str {
        v.type_of()
    }
    fn argument_spread(&mut self, out: &mut Vec<Value>, v: &Value) -> Result<(), Value> {
        self.spread_args(out, v)
    }
}

impl OrderRequestHost for Realm {
    fn await_value(&mut self, value: Value) -> impl Future<Output = Result<Value, Value>> {
        Realm::await_value(self, value)
    }
    fn construct_error(&mut self, constructor: &Value, args: &[Value]) -> Result<Value, Value> {
        Realm::construct(self, constructor, args, None)
    }
    fn instance_of(&mut self, value: &Value, constructor: &Value) -> Result<bool, Value> {
        Realm::instance_of(self, value, constructor)
    }
    fn type_is(&self, value: &Value, kind: &str) -> bool {
        value.type_of() == kind
    }
    fn unary_number(&mut self, value: &Value) -> Result<Value, Value> {
        self.unary("+", value)
    }
    fn compare(
        &mut self,
        op: RequestComparison,
        left: &Value,
        right: &Value,
    ) -> Result<Value, Value> {
        let op = match op {
            RequestComparison::Less => "<",
            RequestComparison::GreaterEqual => ">=",
        };
        Realm::binary(self, op, left, right)
    }
}

impl OrderRetryHost for Realm {
    fn order_retry_callback(&mut self, c: OrderRetryCallback<Value>) -> Result<Value, Value> {
        Ok(self.native("", 1, move |r, _, args| {
            order_retry::run_order_retry_callback(r, &c, args)
        }))
    }
    fn iterator(&mut self, v: &Value) -> Result<Value, Value> {
        self.get_iterator(v)
    }
    fn next(&mut self, v: &Value) -> Result<Option<Value>, Value> {
        self.iterator_step(v)
    }
    fn close_after_throw(&mut self, v: &Value) {
        self.iterator_close_after_throw(v)
    }
    fn retry_operator(&mut self, op: &str, a: &Value, b: &Value) -> Result<Value, Value> {
        Realm::binary(self, op, a, b)
    }
}

impl OrderUpdateHost for Realm {
    fn update_parameter_binding(&mut self, value: &Value) -> Result<(Value, Value), Value> {
        let v = self.object_pattern(value, &["order", "update"])?;
        Ok((v[0].clone(), v[1].clone()))
    }
    fn has_property(&mut self, o: &Value, k: &str) -> Result<bool, Value> {
        Realm::has_property(self, o, &PropKey::str(k))
    }
    fn first_rest_binding(&mut self, v: &Value) -> Result<(Value, Value), Value> {
        self.head_rest(v)
    }
    fn update_callback(&mut self, c: UpdateCallback<Value>) -> Result<Value, Value> {
        Ok(self.native("", 1, move |r, _, args| {
            order_update::run_update_callback(r, &c, args)
        }))
    }
}

impl PlacementHost for Realm {
    fn unary_plus(&mut self, v: &Value) -> Result<Value, Value> {
        self.unary("+", v)
    }
    fn iterator(&mut self, v: &Value) -> Result<Value, Value> {
        self.get_iterator(v)
    }
    fn next(&mut self, v: &Value) -> Result<Option<Value>, Value> {
        self.iterator_step(v)
    }
    fn close_after_throw(&mut self, v: &Value) {
        self.iterator_close_after_throw(v)
    }
    fn serial_order_callback(&mut self, c: SerialOrderCallback<Value>) -> Result<Value, Value> {
        Ok(self.native("", 1, move |r, _, args| {
            placement_dispatch::run_serial_order_callback(r, &c, args)
        }))
    }
}

impl PositionOrderHost for Realm {
    fn position_operator(
        &mut self,
        operator: &str,
        left: &Value,
        right: &Value,
    ) -> Result<Value, Value> {
        Realm::binary(self, operator, left, right)
    }
    fn position_unary(&mut self, operator: &str, value: &Value) -> Result<Value, Value> {
        self.unary(operator, value)
    }
    fn position_callback(&mut self, callback: PositionCallback<Value>) -> Result<Value, Value> {
        Ok(self.native("", 1, move |r, _, args| {
            core_position_order_state::run_position_callback(r, &callback, args)
        }))
    }
    fn set_position_index(
        &mut self,
        object: &Value,
        key: &Value,
        value: &Value,
    ) -> Result<(), Value> {
        Realm::set_index(self, object, key, value.clone())
    }
    /// A spread element `...value` inside an array literal under construction.
    fn append_position_spread(&mut self, array: &Value, value: &Value) -> Result<(), Value> {
        for item in self.collect_iterable(value)? {
            self.append(array, &item)?;
        }
        Ok(())
    }
}

impl PositionRefreshHost for Realm {
    fn position_refresh_callback(
        &mut self,
        capture: PositionRefreshCallback<Value>,
    ) -> Result<Value, Value> {
        Ok(self.native("", 1, move |r, _, args| {
            hip3_position_refresh::run_position_refresh_callback(r, &capture, args)
        }))
    }
    fn iterator(&mut self, value: &Value) -> Result<Value, Value> {
        self.get_iterator(value)
    }
    fn next(&mut self, iterator: &Value) -> Result<Option<Value>, Value> {
        self.iterator_step(iterator)
    }
    fn close_after_throw(&mut self, iterator: &Value) {
        self.iterator_close_after_throw(iterator)
    }
}

impl PrivateMessageHost for Realm {
    fn typeof_value<'a>(&self, value: &'a Value) -> &'a str {
        value.type_of()
    }
    fn has_property(&mut self, name: &str, object: &Value) -> Result<bool, Value> {
        Realm::has_property(self, object, &PropKey::str(name))
    }
    fn valid_fill_predicate(&mut self) -> Result<Value, Value> {
        Ok(self.native("", 1, |r, _, args| {
            private_message_dispatch::run_valid_fill_predicate(r, &first(args))
        }))
    }
    fn for_in(&mut self, object: &Value) -> Result<Value, Value> {
        let keys = self.for_in_keys(object);
        Ok(self.iterator_object(IterSource::Keys { keys, index: 0 }))
    }
    fn for_in_next(&mut self, iterator: &Value) -> Result<Option<Value>, Value> {
        self.iterator_step(iterator)
    }
    fn construct(&mut self, function: &Value, args: &[Value]) -> Result<Value, Value> {
        Realm::construct(self, function, args, None)
    }
    fn regex(&mut self, pattern: &str) -> Result<Value, Value> {
        self.regexp(pattern)
    }
    fn number(&self, value: f64) -> Value {
        Value::Number(value)
    }
    fn boolean(&self, value: bool) -> Value {
        Value::Bool(value)
    }
    fn is_nullish(&self, value: &Value) -> bool {
        value.is_nullish()
    }
    fn is_null(&self, value: &Value) -> bool {
        matches!(value, Value::Null)
    }
    fn strict_equal(&self, a: &Value, b: &Value) -> bool {
        a.strict_equal(b)
    }
    fn index(&mut self, object: &Value, key: &Value) -> Result<Value, Value> {
        Realm::index(self, object, key)
    }
    fn array(&mut self) -> Result<Value, Value> {
        Ok(self.new_array(vec![]))
    }
    fn template_string(&mut self, value: &Value) -> Result<JsString, Value> {
        Ok(js(self.template(value)?))
    }
    fn operator(&mut self, op: DispatchOperator, a: &Value, b: &Value) -> Result<Value, Value> {
        let op = match op {
            DispatchOperator::Less => "<",
            DispatchOperator::LessEqual => "<=",
            DispatchOperator::Greater => ">",
            DispatchOperator::Subtract => "-",
            DispatchOperator::Divide => "/",
        };
        Realm::binary(self, op, a, b)
    }
}

impl PrivatePositionHost for Realm {
    fn number(&mut self, value: &Value) -> Result<f64, Value> {
        self.to_number(value)
    }
    fn number_value(&self, value: f64) -> Value {
        Value::Number(value)
    }
    fn lowercase(&mut self, value: &JsString) -> Result<JsString, Value> {
        Ok(JsString(
            String::from_utf16_lossy(&value.0)
                .to_lowercase()
                .encode_utf16()
                .collect(),
        ))
    }
    fn index(&mut self, object: &Value, key: &Value) -> Result<Value, Value> {
        Realm::index(self, object, key)
    }
    fn add(&mut self, left: &Value, right: &Value) -> Result<Value, Value> {
        Realm::add(self, left, right)
    }
    fn max_tracked_fill_ids(&mut self) -> Result<Value, Value> {
        let class = self.global_binding("HyperliquidPrivateWebSocket")?;
        self.get_str(&class, "MAX_TRACKED_REALIZED_PNL_FILL_IDS")
    }
    fn greater_than(&mut self, value: &Value, limit: &Value) -> Result<bool, Value> {
        bool_of(self, ">", value, limit)
    }
}

impl RestReadHost for Realm {
    fn rest_callback(&mut self, callback: RestCallback<Value>) -> Result<Value, Value> {
        Ok(self.native("", 1, move |r, _, args| {
            if callback.is_async() {
                let observer =
                    core_rest_reads::start_rest_callback(r, callback.clone(), args.to_vec());
                Ok(r.spawn_value(Box::pin(observer)))
            } else {
                core_rest_reads::run_rest_callback(r, &callback, args)
            }
        }))
    }
    fn rest_exception(&self, value: Value) -> Value {
        value
    }
    fn rest_array_binding(&mut self, value: &Value, mask: &[bool]) -> Result<Vec<Value>, Value> {
        self.array_pattern(value, mask)
    }
    fn rest_argument_spread(&mut self, args: &mut Vec<Value>, value: &Value) -> Result<(), Value> {
        self.spread_args(args, value)
    }
}

impl SpotMarketBootstrapHost for Realm {
    fn property_key(&mut self, value: &Value) -> Result<Value, Value> {
        Ok(self.to_property_key(value)?.to_value())
    }
    fn set_key(&mut self, object: &Value, key: &Value, value: &Value) -> Result<(), Value> {
        Realm::set_index(self, object, key, value.clone())
    }
    fn unary_plus(&mut self, value: &Value) -> Result<Value, Value> {
        self.unary("+", value)
    }
}

impl StartupLifecycleHost for Realm {
    fn startup_callback(&mut self, callback: StartupCallback<Value>) -> Result<Value, Value> {
        Ok(self.native("", 1, move |r, _, args| {
            startup_lifecycle::run_startup_callback(r, &callback, args)
        }))
    }
}

impl WsEmissionHost for Realm {
    fn boolean(&self, value: bool) -> Value {
        Value::Bool(value)
    }
    fn system_types(&mut self, venue: WsVenue) -> Result<Value, Value> {
        Ok(match venue {
            WsVenue::Perp => self.singleton("SYSTEM_SUBSCRIPTION_TYPES", |r| {
                strings(r, crate::constants::SYSTEM_SUBSCRIPTION_TYPES)
            }),
            WsVenue::Spot => self.singleton("SPOT_SYSTEM_SUBSCRIPTION_TYPES", |r| {
                strings(r, crate::spot_constants::SYSTEM_SUBSCRIPTION_TYPES)
            }),
        })
    }
    fn iterator(&mut self, value: &Value) -> Result<Value, Value> {
        self.get_iterator(value)
    }
    fn next(&mut self, iterator: &Value) -> Result<Option<Value>, Value> {
        self.iterator_step(iterator)
    }
    fn close_after_throw(&mut self, iterator: &Value) {
        self.iterator_close_after_throw(iterator)
    }
    fn triple(&mut self, value: &Value) -> Result<[Value; 3], Value> {
        let s = self.array_slots(value, 3)?;
        Ok([s[0].clone(), s[1].clone(), s[2].clone()])
    }
    fn string_units<'a>(&self, value: &'a Value) -> Option<&'a [u16]> {
        value.as_units()
    }
    fn spread_into(&mut self, target: &Value, source: &Value) -> Result<(), Value> {
        self.copy_data_properties(target, source)
    }
    fn formatter(&mut self, scope: DiagnosticScope) -> Result<Value, Value> {
        Ok(self.native("", 1, move |r, _, args| {
            ws_emission::run_diagnostic_formatter(r, scope, &first(args))
        }))
    }
}

impl WsTransportHost for Realm {
    fn urls_binding(&mut self, venue: WsVenue) -> Result<Value, Value> {
        Ok(match venue {
            WsVenue::Perp => self.singleton("WSS_URLS", |r| table(r, crate::types::WSS_URLS)),
            WsVenue::Spot => {
                self.singleton("SPOT_WSS_URLS", |r| table(r, crate::spot_types::WSS_URLS))
            }
        })
    }
    fn boolean(&self, value: bool) -> Value {
        Value::Bool(value)
    }
}
