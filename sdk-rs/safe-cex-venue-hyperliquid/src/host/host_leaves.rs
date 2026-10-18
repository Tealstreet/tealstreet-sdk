//! Leaf JS host traits: operators, bindings, iteration and callback captures.
//! Every capture becomes a native function whose body is the venue's mapped
//! `run_*` algorithm; no algorithm lives here.
use safe_cex_exchange::js_runtime::{value::{Js, Thrown}};
use super::{
    Host,
    host_core::js,
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

fn first(args: &[Js]) -> Js {
    args.first().cloned().unwrap_or(Js::Undefined)
}
fn pair(v: Vec<Js>) -> [Js; 2] {
    [v[0].clone(), v[1].clone()]
}
fn bool_of(r: &mut Host, op: &str, a: &Js, b: &Js) -> Result<bool, Thrown> {
    Ok(Host::binary(r, op, a, b)?.truthy())
}
fn table(r: &Host, rows: &[(&str, &str)]) -> Js {
    r.object_from(rows.iter().map(|(k, v)| (*k, Js::str(v))).collect())
}
fn strings(r: &Host, items: &[&str]) -> Js {
    r.new_array(items.iter().map(|s| Js::str(s)).collect())
}

impl AccountPositionHost for Host {
    fn null(&self) -> Js {
        Js::Null
    }
    fn boolean(&self, value: bool) -> Js {
        Js::Bool(value)
    }
    fn number(&self, value: f64) -> Js {
        Js::Number(value)
    }
    fn is_nullish(&self, value: &Js) -> bool {
        value.is_nullish()
    }
    fn strict_equal(&self, left: &Js, right: &Js) -> bool {
        left.strict_equals(right)
    }
    fn greater_equal(&mut self, left: &Js, right: &Js) -> Result<bool, Thrown> {
        bool_of(self, ">=", left, right)
    }
    fn opposite_predicate(&mut self, capture: OppositePredicate<Js>) -> Result<Js, Thrown> {
        Ok(self.native("", 1, move |r, _, args| {
            account_position_leaves::run_opposite_predicate(r, &capture, &first(args))
                .map(Js::Bool)
        }))
    }
}

impl BalanceHost for Host {
    fn null(&self) -> Js {
        Js::Null
    }
    fn number(&self, v: f64) -> Js {
        Js::Number(v)
    }
    fn boolean(&self, v: bool) -> Js {
        Js::Bool(v)
    }
    fn is_nullish(&self, v: &Js) -> bool {
        v.is_nullish()
    }
    fn is_undefined(&self, v: &Js) -> bool {
        matches!(v, Js::Undefined)
    }
    fn strict_equal(&self, a: &Js, b: &Js) -> bool {
        a.strict_equals(b)
    }
    fn index(&mut self, o: &Js, key: &Js) -> Result<Js, Thrown> {
        Host::index(self, o, key)
    }
    fn set_index(&mut self, o: &Js, key: &Js, v: &Js) -> Result<(), Thrown> {
        Host::set_index(self, o, key, v.clone())
    }
    fn define_index(&mut self, o: &Js, key: &Js, v: &Js) -> Result<(), Thrown> {
        let key = self.to_property_key(key)?;
        self.define(o, key, v.clone())
    }
    fn array(&mut self) -> Result<Js, Thrown> {
        Ok(self.new_array(vec![]))
    }
    fn array_define(&mut self, o: &Js, i: usize, v: &Js) -> Result<(), Thrown> {
        self.define(o, Js::str(&i.to_string()), v.clone())
    }
    fn iterator(&mut self, v: &Js) -> Result<Js, Thrown> {
        self.get_iterator(v)
    }
    fn next(&mut self, i: &Js) -> Result<Option<Js>, Thrown> {
        self.iterator_step(i)
    }
    fn close_after_throw(&mut self, i: &Js) {
        self.iterator_close_after_throw(i)
    }
    fn close_after_return(&mut self, i: &Js) -> Result<(), Thrown> {
        self.iterator_close(i)
    }
    fn pair(&mut self, v: &Js) -> Result<[Js; 2], Thrown> {
        Ok(pair(self.array_slots(v, 2)?))
    }
    fn template_string(&mut self, v: &Js) -> Result<JsString, Thrown> {
        Ok(js(self.template(v)?))
    }
    fn operator(&mut self, op: BalanceOperator, a: &Js, b: &Js) -> Result<Js, Thrown> {
        let op = match op {
            BalanceOperator::Add => "+",
            BalanceOperator::Subtract => "-",
            BalanceOperator::Multiply => "*",
            BalanceOperator::Greater => ">",
        };
        Host::binary(self, op, a, b)
    }
    fn spread_into(&mut self, o: &Js, source: &Js) -> Result<(), Thrown> {
        self.copy_data_properties(o, source)
    }
    fn spot_market_predicate(
        &mut self,
        capture: SpotMarketPredicate<Js>,
    ) -> Result<Js, Thrown> {
        Ok(self.native("", 1, move |r, _, args| {
            balance_state::run_spot_market_predicate(r, &capture, &first(args)).map(Js::Bool)
        }))
    }
}

impl CancellationHost for Host {
    fn unary_plus(&mut self, v: &Js) -> Result<Js, Thrown> {
        self.unary("+", v)
    }
    fn instance_of(&mut self, v: &Js, class: &Js) -> Result<bool, Thrown> {
        Host::instance_of(self, v, class)
    }
    fn iterator(&mut self, v: &Js) -> Result<Js, Thrown> {
        self.get_iterator(v)
    }
    fn next(&mut self, v: &Js) -> Result<Option<Js>, Thrown> {
        self.iterator_step(v)
    }
    fn close_after_throw(&mut self, v: &Js) {
        self.iterator_close_after_throw(v)
    }
    fn cancel_callback(&mut self, c: CancelCallback<Js>) -> Result<Js, Thrown> {
        Ok(self.native("", 1, move |r, _, args| {
            cancellation::run_cancel_callback(r, &c, args)
        }))
    }
}

impl CandleListenerHost for Host {
    fn number(&self, value: f64) -> Js {
        Js::Number(value)
    }
    fn boolean(&self, value: bool) -> Js {
        Js::Bool(value)
    }
    fn nullish(&self, value: &Js) -> bool {
        value.is_nullish()
    }
    fn strict_equal(&self, left: &Js, right: &Js) -> bool {
        left.strict_equals(right)
    }
    fn binary(&mut self, operator: &str, left: &Js, right: &Js) -> Result<Js, Thrown> {
        Host::binary(self, operator, left, right)
    }
    fn decrement(&mut self, value: &Js) -> Result<Js, Thrown> {
        Ok(Js::Number(self.to_number(value)? - 1.0))
    }
    fn template_string(&mut self, value: &Js) -> Result<JsString, Thrown> {
        Ok(js(self.template(value)?))
    }
    fn index(&mut self, object: &Js, key: &Js) -> Result<Js, Thrown> {
        Host::index(self, object, key)
    }
    fn assign_index(&mut self, object: &Js, key: &Js, value: &Js) -> Result<(), Thrown> {
        Host::set_index(self, object, key, value.clone())
    }
    fn array(&mut self, values: &[Js]) -> Result<Js, Thrown> {
        Ok(self.new_array(values.to_vec()))
    }
    fn spread_append(&mut self, iterable: &Js, last: &Js) -> Result<Js, Thrown> {
        Host::spread_append(self, iterable, last)
    }
    fn spread_array(&mut self, iterable: &Js) -> Result<Js, Thrown> {
        self.spread_to_array(iterable)
    }
    fn candle_callback(&mut self, capture: CandleCallback<Js>) -> Result<Js, Thrown> {
        Ok(self.native("", 1, move |r, _, args| {
            candle_listeners::run_candle_callback(r, &capture, args)
        }))
    }
}

impl CoreBookHost for Host {
    fn imported(&mut self, name: &str) -> Result<Js, Thrown> {
        self.import(name)
    }
    fn spread_object(&mut self, source: &Js) -> Result<Js, Thrown> {
        Host::spread_object(self, source)
    }
    fn delete_index(&mut self, object: &Js, key: &Js) -> Result<(), Thrown> {
        let key = self.to_property_key(key)?;
        Host::delete(self, object, &key).map(|_| ())
    }
    fn construct(&mut self, constructor: &Js, args: &[Js]) -> Result<Js, Thrown> {
        Host::construct(self, constructor, args, None)
    }
    fn throw_value(&self, value: Js) -> Thrown {
        Thrown::new(value)
    }
    fn book_callback(&mut self, capture: CoreBookCallback<Js>) -> Result<Js, Thrown> {
        Ok(self.native("", 1, move |r, _, args| {
            core_book_listener::run_core_book_callback(r, &capture, args)
        }))
    }
}

impl CoreExchangeLeafHost for Host {
    fn typeof_value(&self, value: &Js) -> &'static str {
        value.type_of()
    }
    fn finite_positive_predicate(&mut self) -> Result<Js, Thrown> {
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
    fn exchange_predicate(&mut self, capture: ExchangePredicate<Js>) -> Result<Js, Thrown> {
        Ok(self.native("", 1, move |r, _, args| {
            core_exchange_leaves::run_exchange_predicate(r, &capture, &first(args))
        }))
    }
}

impl CoreSnapshotHost for Host {
    fn null_value(&self) -> Js {
        Js::Null
    }
    fn add_values(&mut self, a: &Js, b: &Js) -> Result<Js, Thrown> {
        self.add(a, b)
    }
    fn return_await_value(&mut self, value: Js) -> impl Future<Output = Result<Js, Thrown>> {
        Host::await_value(self, value)
    }
    fn await_value(&mut self, value: Js) -> impl Future<Output = Result<Js, Thrown>> {
        Host::await_value(self, value)
    }
    fn error_value(&self, error: &Thrown) -> Js {
        error.value.clone()
    }
    fn construct_error(&mut self, constructor: &Js, args: &[Js]) -> Result<Thrown, Thrown> {
        Host::construct(self, constructor, args, None).map(Thrown::new)
    }
    fn instance_of(&mut self, value: &Thrown, constructor: &Js) -> Result<bool, Thrown> {
        Host::instance_of(self, &value.value, constructor)
    }
    fn snapshot_predicate(&mut self, capture: SnapshotPredicate<Js>) -> Result<Js, Thrown> {
        Ok(self.native("", 1, move |r, _, args| {
            core_account_snapshots::run_snapshot_predicate(r, &capture, &first(args))
        }))
    }
    fn array_define(&mut self, array: &Js, index: usize, value: &Js) -> Result<(), Thrown> {
        self.define(array, Js::str(&index.to_string()), value.clone())
    }
    fn spread_into(&mut self, object: &Js, source: &Js) -> Result<(), Thrown> {
        self.copy_data_properties(object, source)
    }
    fn iterator(&mut self, value: &Js) -> Result<Js, Thrown> {
        self.get_iterator(value)
    }
    fn next(&mut self, iterator: &Js) -> Result<Option<Js>, Thrown> {
        self.iterator_step(iterator)
    }
    fn close_after_throw(&mut self, iterator: &Js) {
        self.iterator_close_after_throw(iterator)
    }
    fn close_after_return(&mut self, iterator: &Js) -> Result<(), Thrown> {
        self.iterator_close(iterator)
    }
    fn pair(&mut self, value: &Js) -> Result<[Js; 2], Thrown> {
        Ok(pair(self.array_slots(value, 2)?))
    }
}

impl CorePublicMessageHost for Host {
    fn event_data(&mut self, event: &Js) -> Result<Js, Thrown> {
        self.data_binding(event)
    }
    fn is_nullish(&self, value: &Js) -> bool {
        value.is_nullish()
    }
    fn is_array(&mut self, value: &Js) -> Result<bool, Thrown> {
        Ok(value.is_array())
    }
    fn equal(&self, left: &Js, right: &Js) -> bool {
        left.strict_equals(right)
    }
    fn number(&self, value: f64) -> Js {
        Js::Number(value)
    }
    fn less(&mut self, left: &Js, right: &Js) -> Result<bool, Thrown> {
        bool_of(self, "<", left, right)
    }
    fn subtract(&mut self, left: &Js, right: &Js) -> Result<Js, Thrown> {
        Host::binary(self, "-", left, right)
    }
    fn divide(&mut self, left: &Js, right: &Js) -> Result<Js, Thrown> {
        Host::binary(self, "/", left, right)
    }
    fn index(&mut self, value: &Js, key: &Js) -> Result<Js, Thrown> {
        Host::index(self, value, key)
    }
    fn array(&mut self) -> Result<Js, Thrown> {
        Ok(self.new_array(vec![]))
    }
    fn template_string(&mut self, value: &Js) -> Result<JsString, Thrown> {
        Ok(js(self.template(value)?))
    }
    fn json_parse(&mut self, data: &Js) -> Result<Js, Thrown> {
        let parse = self.import("jsonParse")?;
        Host::call(self, &parse, &Js::Undefined, &[data.clone()])
    }
}

impl LifecycleHost for Host {
    fn null(&self) -> Js {
        Js::Null
    }
    fn boolean(&self, value: bool) -> Js {
        Js::Bool(value)
    }
    fn number(&self, value: f64) -> Js {
        Js::Number(value)
    }
    fn is_nullish(&self, value: &Js) -> bool {
        value.is_nullish()
    }
    /// The home object is the lexical class's prototype.
    fn super_get(&mut self, receiver: &Js, name: &str) -> Result<Js, Thrown> {
        let class = self
            .lexical
            .as_ref()
            .and_then(std::rc::Weak::upgrade)
            .map(Js::Function)
            .ok_or_else(|| self.error("'super' keyword unexpected here"))?;
        let home = self.get_str(&class, "prototype")?;
        Host::super_get(self, &home, receiver, name)
    }
    fn lifecycle_callback(&mut self, capture: LifecycleCallback<Js>) -> Result<Js, Thrown> {
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
            Ok(Js::Undefined)
        }))
    }
    fn subtract(&mut self, left: &Js, right: &Js) -> Result<Js, Thrown> {
        Host::binary(self, "-", left, right)
    }
    fn greater_than(&mut self, left: &Js, right: &Js) -> Result<bool, Thrown> {
        bool_of(self, ">", left, right)
    }
    fn template_string(&mut self, value: &Js) -> Result<JsString, Thrown> {
        Ok(js(self.template(value)?))
    }
}

impl LimitOrderHost for Host {
    fn unary_plus(&mut self, v: &Js) -> Result<Js, Thrown> {
        self.unary("+", v)
    }
    fn first_binding(&mut self, v: &Js) -> Result<Js, Thrown> {
        Ok(self.array_slots(v, 1)?.remove(0))
    }
    fn iterator(&mut self, v: &Js) -> Result<Js, Thrown> {
        self.get_iterator(v)
    }
    fn next(&mut self, v: &Js) -> Result<Option<Js>, Thrown> {
        self.iterator_step(v)
    }
    fn close_after_throw(&mut self, v: &Js) {
        self.iterator_close_after_throw(v)
    }
}

impl MarketBootstrapHost for Host {
    fn bootstrap_callback(
        &mut self,
        callback: MarketBootstrapCallback<Js>,
    ) -> Result<Js, Thrown> {
        Ok(self.native("", 1, move |r, _, args| {
            market_bootstrap::run_market_bootstrap_callback(r, &callback, args)
        }))
    }
    fn array_binding(&mut self, value: &Js, count: usize) -> Result<Vec<Js>, Thrown> {
        self.array_slots(value, count)
    }
    fn spread_arguments(&mut self, args: &mut Vec<Js>, value: &Js) -> Result<(), Thrown> {
        self.spread_args(args, value)
    }
    fn iterator(&mut self, value: &Js) -> Result<Js, Thrown> {
        self.get_iterator(value)
    }
    fn next(&mut self, iterator: &Js) -> Result<Option<Js>, Thrown> {
        self.iterator_step(iterator)
    }
    fn close_after_throw(&mut self, iterator: &Js) {
        self.iterator_close_after_throw(iterator)
    }
    fn close_after_break(&mut self, iterator: &Js) -> Result<(), Thrown> {
        self.iterator_close(iterator)
    }
}

impl MarketDataHost for Host {
    fn null(&self) -> Js {
        Js::Null
    }
    fn number(&self, value: f64) -> Js {
        Js::Number(value)
    }
    fn is_nullish(&self, value: &Js) -> bool {
        value.is_nullish()
    }
    fn is_string(&self, value: &Js) -> bool {
        matches!(value, Js::String(_))
    }
    fn strict_equal(&self, left: &Js, right: &Js) -> bool {
        left.strict_equals(right)
    }
    fn index(&mut self, object: &Js, key: &Js) -> Result<Js, Thrown> {
        Host::index(self, object, key)
    }
    fn iterator(&mut self, value: &Js) -> Result<Js, Thrown> {
        self.get_iterator(value)
    }
    fn next(&mut self, iterator: &Js) -> Result<Option<Js>, Thrown> {
        self.iterator_step(iterator)
    }
    fn close_after_throw(&mut self, iterator: &Js) {
        self.iterator_close_after_throw(iterator)
    }
    fn construct(&mut self, function: &Js, args: &[Js]) -> Result<Js, Thrown> {
        Host::construct(self, function, args, None)
    }
    fn first_binding(&mut self, value: &Js) -> Result<Js, Thrown> {
        Ok(self.array_slots(value, 1)?.remove(0))
    }
    fn unit_regex(&mut self) -> Result<Js, Thrown> {
        self.regexp("/^U[A-Z0-9]/")
    }
    fn template_string(&mut self, value: &Js) -> Result<JsString, Thrown> {
        Ok(js(self.template(value)?))
    }
    fn subtract(&mut self, left: &Js, right: &Js) -> Result<Js, Thrown> {
        Host::binary(self, "-", left, right)
    }
}

impl OrderReadHost for Host {
    fn order_read_callback(&mut self, c: OrderReadCallback<Js>) -> Result<Js, Thrown> {
        Ok(self.native("", 1, move |r, _, args| {
            order_reads::run_order_read_callback(r, &c, args)
        }))
    }
    fn iterator(&mut self, v: &Js) -> Result<Js, Thrown> {
        self.get_iterator(v)
    }
    fn next(&mut self, v: &Js) -> Result<Option<Js>, Thrown> {
        self.iterator_step(v)
    }
    fn close_after_throw(&mut self, v: &Js) {
        self.iterator_close_after_throw(v)
    }
    fn first_binding(&mut self, v: &Js) -> Result<Js, Thrown> {
        Ok(self.array_slots(v, 1)?.remove(0))
    }
    fn typeof_value(&self, v: &Js) -> &'static str {
        v.type_of()
    }
    fn argument_spread(&mut self, out: &mut Vec<Js>, v: &Js) -> Result<(), Thrown> {
        self.spread_args(out, v)
    }
}

impl OrderRequestHost for Host {
    fn await_value(&mut self, value: Js) -> impl Future<Output = Result<Js, Thrown>> {
        Host::await_value(self, value)
    }
    fn construct_error(&mut self, constructor: &Js, args: &[Js]) -> Result<Thrown, Thrown> {
        Host::construct(self, constructor, args, None).map(Thrown::new)
    }
    fn instance_of(&mut self, value: &Thrown, constructor: &Js) -> Result<bool, Thrown> {
        Host::instance_of(self, &value.value, constructor)
    }
    fn type_is(&self, value: &Js, kind: &str) -> bool {
        value.type_of() == kind
    }
    fn unary_number(&mut self, value: &Js) -> Result<Js, Thrown> {
        self.unary("+", value)
    }
    fn compare(
        &mut self,
        op: RequestComparison,
        left: &Js,
        right: &Js,
    ) -> Result<Js, Thrown> {
        let op = match op {
            RequestComparison::Less => "<",
            RequestComparison::GreaterEqual => ">=",
        };
        Host::binary(self, op, left, right)
    }
}

impl OrderRetryHost for Host {
    fn order_retry_callback(&mut self, c: OrderRetryCallback<Js>) -> Result<Js, Thrown> {
        Ok(self.native("", 1, move |r, _, args| {
            order_retry::run_order_retry_callback(r, &c, args)
        }))
    }
    fn iterator(&mut self, v: &Js) -> Result<Js, Thrown> {
        self.get_iterator(v)
    }
    fn next(&mut self, v: &Js) -> Result<Option<Js>, Thrown> {
        self.iterator_step(v)
    }
    fn close_after_throw(&mut self, v: &Js) {
        self.iterator_close_after_throw(v)
    }
    fn retry_operator(&mut self, op: &str, a: &Js, b: &Js) -> Result<Js, Thrown> {
        Host::binary(self, op, a, b)
    }
}

impl OrderUpdateHost for Host {
    fn update_parameter_binding(&mut self, value: &Js) -> Result<(Js, Js), Thrown> {
        let v = self.object_pattern(value, &["order", "update"])?;
        Ok((v[0].clone(), v[1].clone()))
    }
    fn has_property(&mut self, o: &Js, k: &str) -> Result<bool, Thrown> {
        Host::has_property(self, o, &Js::str(k))
    }
    fn first_rest_binding(&mut self, v: &Js) -> Result<(Js, Js), Thrown> {
        self.head_rest(v)
    }
    fn update_callback(&mut self, c: UpdateCallback<Js>) -> Result<Js, Thrown> {
        Ok(self.native("", 1, move |r, _, args| {
            order_update::run_update_callback(r, &c, args)
        }))
    }
}

impl PlacementHost for Host {
    fn unary_plus(&mut self, v: &Js) -> Result<Js, Thrown> {
        self.unary("+", v)
    }
    fn iterator(&mut self, v: &Js) -> Result<Js, Thrown> {
        self.get_iterator(v)
    }
    fn next(&mut self, v: &Js) -> Result<Option<Js>, Thrown> {
        self.iterator_step(v)
    }
    fn close_after_throw(&mut self, v: &Js) {
        self.iterator_close_after_throw(v)
    }
    fn serial_order_callback(&mut self, c: SerialOrderCallback<Js>) -> Result<Js, Thrown> {
        Ok(self.native("", 1, move |r, _, args| {
            placement_dispatch::run_serial_order_callback(r, &c, args)
        }))
    }
}

impl PositionOrderHost for Host {
    fn position_operator(
        &mut self,
        operator: &str,
        left: &Js,
        right: &Js,
    ) -> Result<Js, Thrown> {
        Host::binary(self, operator, left, right)
    }
    fn position_unary(&mut self, operator: &str, value: &Js) -> Result<Js, Thrown> {
        self.unary(operator, value)
    }
    fn position_callback(&mut self, callback: PositionCallback<Js>) -> Result<Js, Thrown> {
        Ok(self.native("", 1, move |r, _, args| {
            core_position_order_state::run_position_callback(r, &callback, args)
        }))
    }
    fn set_position_index(
        &mut self,
        object: &Js,
        key: &Js,
        value: &Js,
    ) -> Result<(), Thrown> {
        Host::set_index(self, object, key, value.clone())
    }
    /// A spread element `...value` inside an array literal under construction.
    fn append_position_spread(&mut self, array: &Js, value: &Js) -> Result<(), Thrown> {
        for item in self.collect_iterable(value)? {
            self.append(array, &item)?;
        }
        Ok(())
    }
}

impl PositionRefreshHost for Host {
    fn position_refresh_callback(
        &mut self,
        capture: PositionRefreshCallback<Js>,
    ) -> Result<Js, Thrown> {
        Ok(self.native("", 1, move |r, _, args| {
            hip3_position_refresh::run_position_refresh_callback(r, &capture, args)
        }))
    }
    fn iterator(&mut self, value: &Js) -> Result<Js, Thrown> {
        self.get_iterator(value)
    }
    fn next(&mut self, iterator: &Js) -> Result<Option<Js>, Thrown> {
        self.iterator_step(iterator)
    }
    fn close_after_throw(&mut self, iterator: &Js) {
        self.iterator_close_after_throw(iterator)
    }
}

impl PrivateMessageHost for Host {
    fn typeof_value<'a>(&self, value: &'a Js) -> &'a str {
        value.type_of()
    }
    fn has_property(&mut self, name: &str, object: &Js) -> Result<bool, Thrown> {
        Host::has_property(self, object, &Js::str(name))
    }
    fn valid_fill_predicate(&mut self) -> Result<Js, Thrown> {
        Ok(self.native("", 1, |r, _, args| {
            private_message_dispatch::run_valid_fill_predicate(r, &first(args))
        }))
    }
    fn for_in(&mut self, object: &Js) -> Result<Js, Thrown> {
        Ok(self.for_in_iterator(object))
    }
    fn for_in_next(&mut self, iterator: &Js) -> Result<Option<Js>, Thrown> {
        self.iterator_step(iterator)
    }
    fn construct(&mut self, function: &Js, args: &[Js]) -> Result<Js, Thrown> {
        Host::construct(self, function, args, None)
    }
    fn regex(&mut self, pattern: &str) -> Result<Js, Thrown> {
        self.regexp(pattern)
    }
    fn number(&self, value: f64) -> Js {
        Js::Number(value)
    }
    fn boolean(&self, value: bool) -> Js {
        Js::Bool(value)
    }
    fn is_nullish(&self, value: &Js) -> bool {
        value.is_nullish()
    }
    fn is_null(&self, value: &Js) -> bool {
        matches!(value, Js::Null)
    }
    fn strict_equal(&self, a: &Js, b: &Js) -> bool {
        a.strict_equals(b)
    }
    fn index(&mut self, object: &Js, key: &Js) -> Result<Js, Thrown> {
        Host::index(self, object, key)
    }
    fn array(&mut self) -> Result<Js, Thrown> {
        Ok(self.new_array(vec![]))
    }
    fn template_string(&mut self, value: &Js) -> Result<JsString, Thrown> {
        Ok(js(self.template(value)?))
    }
    fn operator(&mut self, op: DispatchOperator, a: &Js, b: &Js) -> Result<Js, Thrown> {
        let op = match op {
            DispatchOperator::Less => "<",
            DispatchOperator::LessEqual => "<=",
            DispatchOperator::Greater => ">",
            DispatchOperator::Subtract => "-",
            DispatchOperator::Divide => "/",
        };
        Host::binary(self, op, a, b)
    }
}

impl PrivatePositionHost for Host {
    fn number(&mut self, value: &Js) -> Result<f64, Thrown> {
        self.to_number(value)
    }
    fn number_value(&self, value: f64) -> Js {
        Js::Number(value)
    }
    fn lowercase(&mut self, value: &JsString) -> Result<JsString, Thrown> {
        Ok(JsString(
            String::from_utf16_lossy(&value.0)
                .to_lowercase()
                .encode_utf16()
                .collect(),
        ))
    }
    fn index(&mut self, object: &Js, key: &Js) -> Result<Js, Thrown> {
        Host::index(self, object, key)
    }
    fn add(&mut self, left: &Js, right: &Js) -> Result<Js, Thrown> {
        Host::add(self, left, right)
    }
    fn max_tracked_fill_ids(&mut self) -> Result<Js, Thrown> {
        let class = self.global_binding("HyperliquidPrivateWebSocket")?;
        self.get_str(&class, "MAX_TRACKED_REALIZED_PNL_FILL_IDS")
    }
    fn greater_than(&mut self, value: &Js, limit: &Js) -> Result<bool, Thrown> {
        bool_of(self, ">", value, limit)
    }
}

impl RestReadHost for Host {
    fn rest_callback(&mut self, callback: RestCallback<Js>) -> Result<Js, Thrown> {
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
    fn rest_exception(&self, value: Js) -> Thrown {
        Thrown::new(value)
    }
    fn rest_array_binding(&mut self, value: &Js, mask: &[bool]) -> Result<Vec<Js>, Thrown> {
        self.array_pattern(value, mask)
    }
    fn rest_argument_spread(&mut self, args: &mut Vec<Js>, value: &Js) -> Result<(), Thrown> {
        self.spread_args(args, value)
    }
}

impl SpotMarketBootstrapHost for Host {
    fn property_key(&mut self, value: &Js) -> Result<Js, Thrown> {
        Ok(self.to_property_key(value)?.clone())
    }
    fn set_key(&mut self, object: &Js, key: &Js, value: &Js) -> Result<(), Thrown> {
        Host::set_index(self, object, key, value.clone())
    }
    fn unary_plus(&mut self, value: &Js) -> Result<Js, Thrown> {
        self.unary("+", value)
    }
}

impl StartupLifecycleHost for Host {
    fn startup_callback(&mut self, callback: StartupCallback<Js>) -> Result<Js, Thrown> {
        Ok(self.native("", 1, move |r, _, args| {
            startup_lifecycle::run_startup_callback(r, &callback, args)
        }))
    }
}

impl WsEmissionHost for Host {
    fn boolean(&self, value: bool) -> Js {
        Js::Bool(value)
    }
    fn system_types(&mut self, venue: WsVenue) -> Result<Js, Thrown> {
        Ok(match venue {
            WsVenue::Perp => self.singleton("SYSTEM_SUBSCRIPTION_TYPES", |r| {
                strings(r, crate::constants::SYSTEM_SUBSCRIPTION_TYPES)
            }),
            WsVenue::Spot => self.singleton("SPOT_SYSTEM_SUBSCRIPTION_TYPES", |r| {
                strings(r, crate::spot_constants::SYSTEM_SUBSCRIPTION_TYPES)
            }),
        })
    }
    fn iterator(&mut self, value: &Js) -> Result<Js, Thrown> {
        self.get_iterator(value)
    }
    fn next(&mut self, iterator: &Js) -> Result<Option<Js>, Thrown> {
        self.iterator_step(iterator)
    }
    fn close_after_throw(&mut self, iterator: &Js) {
        self.iterator_close_after_throw(iterator)
    }
    fn triple(&mut self, value: &Js) -> Result<[Js; 3], Thrown> {
        let s = self.array_slots(value, 3)?;
        Ok([s[0].clone(), s[1].clone(), s[2].clone()])
    }
    fn string_units<'a>(&self, value: &'a Js) -> Option<&'a [u16]> {
        value.as_str().map(|units| units.as_ref())
    }
    fn spread_into(&mut self, target: &Js, source: &Js) -> Result<(), Thrown> {
        self.copy_data_properties(target, source)
    }
    fn formatter(&mut self, scope: DiagnosticScope) -> Result<Js, Thrown> {
        Ok(self.native("", 1, move |r, _, args| {
            ws_emission::run_diagnostic_formatter(r, scope, &first(args))
        }))
    }
}

impl WsTransportHost for Host {
    fn urls_binding(&mut self, venue: WsVenue) -> Result<Js, Thrown> {
        Ok(match venue {
            WsVenue::Perp => self.singleton("WSS_URLS", |r| table(r, crate::types::WSS_URLS)),
            WsVenue::Spot => {
                self.singleton("SPOT_WSS_URLS", |r| table(r, crate::spot_types::WSS_URLS))
            }
        })
    }
    fn boolean(&self, value: bool) -> Js {
        Js::Bool(value)
    }
}
