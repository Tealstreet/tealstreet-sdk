#![allow(dead_code)]
use super::cache::{self, N, arr, eq, object, s};
use safe_cex_venue_hyperliquid::private_class::*;
use safe_cex_venue_hyperliquid::{
    core_account_snapshots::{CoreSnapshotHost, SnapshotPredicate},
    core_position_order_state::{PositionCallback, PositionOrderHost},
    cross_cache::CrossCacheHost,
    json_boundaries::JsPayloadHost,
    lifecycle_delegates::{LifecycleCallback, LifecycleHost},
    market_data_selectors::MarketDataHost,
    private_constructor::{CorePrivateCapture, PrivateConstructorHost},
    private_message_dispatch::{DispatchOperator, PrivateMessageHost},
    private_position_helpers::PrivatePositionHost,
    spot_json::JsString,
    ws_emission::{DiagnosticScope, WsEmissionHost},
    ws_ping_lifecycle::{self, PingCallback, WsPingHost},
    ws_transport_leaves::{WsTransportHost, WsVenue},
};
use std::future::Future;

use std::{cell::RefCell, collections::HashMap, rc::Rc};
#[derive(Clone)]
pub enum Function {
    Method(CorePrivateMethodCapture<N>),
    Arrow(CorePrivateCapture<N>),
    Ping(PingCallback<N>),
}
pub struct Host {
    pub cache: cache::Host,
    pub classes: Vec<(N, N)>,
    pub functions: HashMap<String, Function>,
    pub globals: HashMap<String, N>,
    pub labels: Vec<(N, String)>,
    pub events: Vec<String>,
    pub marker: N,
    pub a: N,
    pub b: N,
    pub number: N,
    pub string: N,
    pub probe: bool,
    pub scene: String,
    pub count: usize,
    pub finite_count: usize,
    pub timers: Vec<(N, N)>,
    pub supers: Vec<(N, N, N)>,
    pub descriptors: Vec<(N, String, bool, bool, bool)>,
    pub reentry: Option<(N, N, Vec<N>)>,
    pub reentry_target: N,
    pub reentry_results: Vec<N>,
    pub fail: Option<String>,
}
impl Default for Host {
    fn default() -> Self {
        let mut globals = HashMap::new();
        for name in [
            "Set",
            "Map",
            "Number",
            "String",
            "PositionMode",
            "PositionSide",
            "setTimeout",
            "clearTimeout",
        ] {
            globals.insert(name.into(), N::Builtin(name.into()));
        }
        Self {
            cache: cache::Host::default(),
            classes: vec![],
            functions: HashMap::new(),
            globals,
            labels: vec![],
            events: vec![],
            marker: object(vec![]),
            a: object(vec![]),
            b: object(vec![]),
            number: N::Builtin("Number".into()),
            string: N::Builtin("String".into()),
            probe: false,
            scene: String::new(),
            count: 0,
            finite_count: 0,
            timers: vec![],
            supers: vec![],
            descriptors: vec![],
            reentry: None,
            reentry_target: N::U,
            reentry_results: vec![],
            fail: None,
        }
    }
}
impl Host {
    pub fn allocate_test_arrow(&mut self, c: CorePrivateCapture<N>, name: &str) -> N {
        self.allocate(Function::Arrow(c), name, 0)
    }
    pub fn label(&self, v: &N) -> String {
        self.labels
            .iter()
            .find(|(o, _)| same(o, v))
            .map(|(_, s)| s.clone())
            .unwrap_or_else(|| "other".into())
    }
    fn event(&mut self, s: String) -> Result<(), N> {
        self.events.push(s.clone());
        if self.probe || self.fail.as_deref() == Some(&s) {
            Err(self.marker.clone())
        } else {
            Ok(())
        }
    }
    pub fn put(&mut self, v: &N, k: &str, x: &N, e: bool) {
        self.cache.put(v, k, x, e).unwrap()
    }
    pub fn get(&mut self, v: &N, k: &str) -> Result<N, N> {
        self.event(format!("get:{}:{k}", self.label(v)))?;
        if k == "PING_INTERVAL_MS" && same(v, &self.reentry_target) {
            if let Some((function, receiver, args)) = self.reentry.take() {
                let result = self.invoke(&function, &receiver, &args)?;
                self.reentry_results.push(result);
            }
        }
        if self.scene == "summary-callee" && k == "marginSummary" {
            self.count += 1;
            if self.count == 2 {
                self.number = N::Builtin("Number2".into())
            }
        }
        if self.scene == "mode-callee" && k == "type" {
            self.string = N::Builtin("String2".into())
        }
        if self.scene == "timestamp-callee" && k == "serverTime" {
            self.number = N::Builtin("Number2".into())
        }
        if let N::Builtin(b) = v {
            if b == "PositionMode" {
                return Ok(if k == "Hedged" {
                    self.a.clone()
                } else {
                    self.b.clone()
                });
            }
            if b == "PositionSide" {
                return Ok(s(if k == "Long" { "long" } else { "short" }));
            }
            if k == "isFinite" {
                self.finite_count += 1;
                return Ok(N::Builtin(format!("{b}.isFinite")));
            }
        }
        if let N::S(_) = v {
            if ["includes", "toLowerCase"].contains(&k) {
                return Ok(N::Builtin(k.into()));
            }
        }
        JsPayloadHost::get(&mut self.cache, v, k).map_err(|e| panic!("unsupported get {e}"))
    }
    pub fn invoke(&mut self, f: &N, r: &N, args: &[N]) -> Result<N, N> {
        if let N::Builtin(id) = f {
            if let Some(function) = self.functions.get(id).cloned() {
                return match function {
                    Function::Method(c) => call_core_private_method(self, &c, r, args),
                    Function::Arrow(c) => call_core_private_arrow(self, &c, args),
                    Function::Ping(c) => {
                        ws_ping_lifecycle::run_ping_timeout(self, &c)?;
                        Ok(N::U)
                    }
                };
            }
            self.events.push(format!("call:{id}"));
            match id.as_str() {
                "Number" | "Number2" => {
                    assert!(matches!(r, N::U), "CLASS bare Number receiver");
                    assert_eq!(args.len(), 1, "CLASS independent Number arity");
                    if matches!(
                        self.scene.as_str(),
                        "summary-callee" | "number-identity" | "timestamp-callee"
                    ) {
                        let expected: f64 = if self.scene == "summary-callee" && id == "Number2" {
                            3.0
                        } else {
                            9.0
                        };
                        let N::Num(n) = &args[0] else {
                            panic!("CLASS original Number argument native variant")
                        };
                        assert_eq!(
                            n.to_bits(),
                            expected.to_bits(),
                            "CLASS original Number argument native bits"
                        );
                    }
                    return Ok(
                        if self.scene == "summary-callee" || self.scene == "timestamp-callee" {
                            if id == "Number" {
                                self.a.clone()
                            } else {
                                self.b.clone()
                            }
                        } else if self.scene == "number-identity" {
                            self.a.clone()
                        } else {
                            N::Num(cache::num(&args[0]))
                        },
                    );
                }
                "Number.isFinite" | "Number2.isFinite" => {
                    assert_eq!(args.len(), 1, "CLASS independent isFinite arity");
                    if matches!(
                        self.scene.as_str(),
                        "summary-callee" | "number-identity" | "timestamp-callee"
                    ) {
                        let expected = if self.scene == "timestamp-callee"
                            || (self.scene == "summary-callee" && self.finite_count == 2)
                        {
                            &self.b
                        } else {
                            &self.a
                        };
                        assert!(
                            same(&args[0], expected),
                            "CLASS isFinite original result argument"
                        );
                    }
                    assert!(
                        same(r, &N::Builtin(id.trim_end_matches(".isFinite").into())),
                        "CLASS original finite receiver"
                    );
                    return Ok(N::B(if self.scene == "finite-false" {
                        false
                    } else if matches!(
                        self.scene.as_str(),
                        "summary-callee" | "number-identity" | "timestamp-callee"
                    ) {
                        true
                    } else {
                        cache::num(&args[0]).is_finite()
                    }));
                }
                "String" | "String2" => {
                    assert_eq!(args.len(), 1, "CLASS independent String arity");
                    if self.scene == "mode-callee" {
                        assert!(
                            same(&args[0], &s("hedged")),
                            "CLASS original String argument"
                        );
                    }
                    assert!(matches!(r, N::U));
                    if self.scene == "mode-callee" {
                        return Ok(s(if id == "String" { "hedged" } else { "oneway" }));
                    }
                    return Ok(N::S(
                        JsPayloadHost::to_string(&mut self.cache, &args[0]).unwrap(),
                    ));
                }
                "toLowerCase" => {
                    let N::S(t) = r else { panic!("string lower") };
                    return Ok(s(&String::from_utf16(&t.0).unwrap().to_lowercase()));
                }
                "includes" => {
                    let (N::S(t), N::S(n)) = (r, &args[0]) else {
                        panic!("string includes")
                    };
                    return Ok(N::B(t.0.windows(n.0.len()).any(|v| v == n.0)));
                }
                "setTimeout" => {
                    assert!(matches!(r, N::U));
                    self.timers.push((args[0].clone(), args[1].clone()));
                    return Ok(N::Num(self.timers.len() as f64));
                }
                "shift" => {
                    let N::A(values) = r else {
                        panic!("native shift array")
                    };
                    let mut values = values.borrow_mut();
                    return Ok(if values.is_empty() {
                        N::U
                    } else {
                        values.remove(0)
                    });
                }
                "trackLastReceived" | "subscribed" | "start" => return Ok(self.a.clone()),
                _ => {}
            }
        }
        CrossCacheHost::call(&mut self.cache, f, r, args)
            .map_err(|e| panic!("unsupported call {e}: {f:?}"))
    }
    pub fn allocate(&mut self, function: Function, name: &str, length: u8) -> N {
        let id = format!("callable/{}", self.functions.len());
        let f = N::Builtin(id.clone());
        self.functions.insert(id, function);
        self.descriptors
            .push((f.clone(), format!("{name}/{length}"), false, false, true));
        f
    }
}
pub fn same(a: &N, b: &N) -> bool {
    eq(a, b) || matches!((a,b),(N::Builtin(a),N::Builtin(b))if a==b)
}
impl WsPingHost for Host {
    type Value = N;
    type Error = N;
    fn undefined(&self) -> N {
        N::U
    }
    fn literal(&self, text: &JsString) -> N {
        let _ = &text;
        N::S(text.clone())
    }
    fn truthy(&self, value: &N) -> bool {
        let _ = &value;
        CrossCacheHost::truthy(&self.cache, value)
    }
    fn global(&mut self, name: &str) -> Result<N, N> {
        let _ = &name;
        self.event(format!("global:{name}"))?;
        Ok(if name == "Number" {
            self.number.clone()
        } else if name == "String" {
            self.string.clone()
        } else {
            self.globals
                .get(name)
                .cloned()
                .unwrap_or_else(|| panic!("unbound global {name}"))
        })
    }
    fn get(&mut self, object: &N, name: &str) -> Result<N, N> {
        let _ = &object;
        let _ = &name;
        self.get(object, name)
    }
    fn set(&mut self, object: &N, name: &str, value: &N) -> Result<(), N> {
        let _ = &object;
        let _ = &name;
        let _ = &value;
        self.put(object, name, value, true);
        Ok(())
    }
    fn call(&mut self, function: &N, receiver: &N, args: &[N]) -> Result<N, N> {
        let _ = &function;
        let _ = &receiver;
        let _ = &args;
        self.invoke(function, receiver, args)
    }
    fn object(&mut self) -> Result<N, N> {
        Ok(object(vec![]))
    }
    fn define_data(&mut self, object: &N, name: &str, value: &N) -> Result<(), N> {
        let _ = &object;
        let _ = &name;
        let _ = &value;
        self.event(format!("define:{}:{name}", self.label(object)))?;
        self.put(object, name, value, true);
        self.descriptors
            .push((object.clone(), name.into(), true, true, true));
        Ok(())
    }
    fn callback(&mut self, capture: PingCallback<N>) -> Result<N, N> {
        let _ = &capture;
        Ok(self.allocate(Function::Ping(capture), "", 0))
    }
}
impl PrivateConstructorHost for Host {
    fn number(&self, value: f64) -> N {
        let _ = &value;
        N::Num(value)
    }
    fn array(&mut self) -> Result<N, N> {
        Ok(arr(vec![]))
    }
    fn construct(&mut self, constructor: &N, args: &[N]) -> Result<N, N> {
        let _ = &constructor;
        let _ = &args;
        if let N::Builtin(name) = constructor {
            return match name.as_str() {
                "Map" => Ok(N::Map(Rc::new(RefCell::new(vec![])))),
                "Set" => {
                    let values = if args.is_empty() {
                        vec![]
                    } else {
                        let N::A(a) = &args[0] else {
                            panic!("set iterable")
                        };
                        a.borrow().clone()
                    };
                    Ok(N::Set(Rc::new(RefCell::new(values))))
                }
                _ => panic!("construct {name}"),
            };
        }
        panic!("construct native class through construct_core_private_class")
    }
    fn private_super(&mut self, class: &N, parent: &N, new_target: &N) -> Result<N, N> {
        let _ = &class;
        let _ = &parent;
        let _ = &new_target;
        self.supers
            .push((class.clone(), parent.clone(), new_target.clone()));
        let proto = self
            .classes
            .iter()
            .find(|(c, _)| same(c, new_target))
            .map(|(_, p)| p.clone())
            .unwrap_or_else(|| {
                JsPayloadHost::get(&mut self.cache, new_target, "prototype").unwrap()
            });
        let value = object(vec![("parent", parent.clone())]);
        let N::O(o) = &value else { unreachable!() };
        o.borrow_mut().prototype = Some(proto);
        Ok(value)
    }
    fn private_arrow(
        &mut self,
        capture: CorePrivateCapture<N>,
        name: &str,
        length: u8,
    ) -> Result<N, N> {
        let _ = &capture;
        let _ = &name;
        let _ = &length;
        Ok(self.allocate(Function::Arrow(capture), name, length))
    }
}
impl WsTransportHost for Host {
    fn urls_binding(&mut self, venue: WsVenue) -> Result<N, N> {
        let _ = &venue;
        panic!("unenumerated binding WsTransportHost::urls_binding")
    }
    fn boolean(&self, value: bool) -> N {
        let _ = &value;
        panic!("unenumerated binding WsTransportHost::boolean")
    }
}
impl WsEmissionHost for Host {
    fn boolean(&self, value: bool) -> N {
        let _ = &value;
        panic!("unenumerated binding WsEmissionHost::boolean")
    }
    fn system_types(&mut self, venue: WsVenue) -> Result<N, N> {
        let _ = &venue;
        panic!("unenumerated binding WsEmissionHost::system_types")
    }
    fn iterator(&mut self, value: &N) -> Result<N, N> {
        let _ = &value;
        panic!("unenumerated binding WsEmissionHost::iterator")
    }
    fn next(&mut self, iterator: &N) -> Result<Option<N>, N> {
        let _ = &iterator;
        panic!("unenumerated binding WsEmissionHost::next")
    }
    fn close_after_throw(&mut self, iterator: &N) {
        let _ = &iterator;
        panic!("unenumerated binding WsEmissionHost::close_after_throw")
    }
    fn triple(&mut self, value: &N) -> Result<[N; 3], N> {
        let _ = &value;
        panic!("unenumerated binding WsEmissionHost::triple")
    }
    fn string_units<'a>(&self, value: &'a N) -> Option<&'a [u16]> {
        let _ = &value;
        panic!("unenumerated binding WsEmissionHost::string_units")
    }
    fn spread_into(&mut self, target: &N, source: &N) -> Result<(), N> {
        let _ = &target;
        let _ = &source;
        panic!("unenumerated binding WsEmissionHost::spread_into")
    }
    fn formatter(&mut self, scope: DiagnosticScope) -> Result<N, N> {
        let _ = &scope;
        panic!("unenumerated binding WsEmissionHost::formatter")
    }
}
impl PrivateMessageHost for Host {
    fn typeof_value<'a>(&self, value: &'a N) -> &'a str {
        let _ = &value;
        match value {
            N::S(_) => "string",
            N::Num(_) => "number",
            N::B(_) => "boolean",
            N::U => "undefined",
            _ => "object",
        }
    }
    fn has_property(&mut self, name: &str, object: &N) -> Result<bool, N> {
        let _ = &name;
        let _ = &object;
        panic!("unenumerated binding PrivateMessageHost::has_property")
    }
    fn valid_fill_predicate(&mut self) -> Result<N, N> {
        panic!("unenumerated binding PrivateMessageHost::valid_fill_predicate")
    }
    fn for_in(&mut self, object: &N) -> Result<N, N> {
        let _ = &object;
        panic!("unenumerated binding PrivateMessageHost::for_in")
    }
    fn for_in_next(&mut self, iterator: &N) -> Result<Option<N>, N> {
        let _ = &iterator;
        panic!("unenumerated binding PrivateMessageHost::for_in_next")
    }
    fn construct(&mut self, function: &N, args: &[N]) -> Result<N, N> {
        let _ = &function;
        let _ = &args;
        panic!("unenumerated binding PrivateMessageHost::construct")
    }
    fn regex(&mut self, pattern: &str) -> Result<N, N> {
        let _ = &pattern;
        panic!("unenumerated binding PrivateMessageHost::regex")
    }
    fn number(&self, value: f64) -> N {
        let _ = &value;
        N::Num(value)
    }
    fn boolean(&self, value: bool) -> N {
        let _ = &value;
        N::B(value)
    }
    fn is_nullish(&self, value: &N) -> bool {
        let _ = &value;
        matches!(value, N::U | N::Null)
    }
    fn is_null(&self, value: &N) -> bool {
        let _ = &value;
        matches!(value, N::Null)
    }
    fn strict_equal(&self, a: &N, b: &N) -> bool {
        let _ = &a;
        let _ = &b;
        same(a, b)
    }
    fn index(&mut self, object: &N, key: &N) -> Result<N, N> {
        let _ = &object;
        let _ = &key;
        panic!("unenumerated binding PrivateMessageHost::index")
    }
    fn array(&mut self) -> Result<N, N> {
        panic!("unenumerated binding PrivateMessageHost::array")
    }
    fn template_string(&mut self, value: &N) -> Result<JsString, N> {
        let _ = &value;
        if self.probe {
            self.event(format!("get:{}:@@toPrimitive", self.label(value)))?;
        }
        if let N::S(units) = value {
            return Ok(units.clone());
        }
        JsPayloadHost::to_string(&mut self.cache, value).map_err(|e| panic!("template {e}"))
    }
    fn operator(&mut self, op: DispatchOperator, a: &N, b: &N) -> Result<N, N> {
        let _ = &op;
        let _ = &a;
        let _ = &b;
        let a = if same(a, &self.a) || same(a, &self.b) {
            1.0
        } else {
            cache::num(a)
        };
        let b = cache::num(b);
        Ok(match op {
            DispatchOperator::LessEqual => N::B(a <= b),
            DispatchOperator::Less => N::B(a < b),
            DispatchOperator::Greater => N::B(a > b),
            DispatchOperator::Subtract => N::Num(a - b),
            DispatchOperator::Divide => N::Num(a / b),
        })
    }
}
impl MarketDataHost for Host {
    fn null(&self) -> N {
        N::Null
    }
    fn number(&self, value: f64) -> N {
        let _ = &value;
        N::Num(value)
    }
    fn is_nullish(&self, value: &N) -> bool {
        let _ = &value;
        matches!(value, N::U | N::Null)
    }
    fn is_string(&self, value: &N) -> bool {
        let _ = &value;
        panic!("unenumerated binding MarketDataHost::is_string")
    }
    fn strict_equal(&self, left: &N, right: &N) -> bool {
        let _ = &left;
        let _ = &right;
        same(left, right)
    }
    fn index(&mut self, object: &N, key: &N) -> Result<N, N> {
        let _ = &object;
        let _ = &key;
        if !matches!(key, N::S(_)) && self.probe {
            self.event(format!("get:{}:@@toPrimitive", self.label(key)))?;
        }
        let N::S(key) = key else {
            panic!("native index key")
        };
        self.get(object, &String::from_utf16(&key.0).unwrap())
    }
    fn iterator(&mut self, value: &N) -> Result<N, N> {
        let _ = &value;
        panic!("unenumerated binding MarketDataHost::iterator")
    }
    fn next(&mut self, iterator: &N) -> Result<Option<N>, N> {
        let _ = &iterator;
        panic!("unenumerated binding MarketDataHost::next")
    }
    fn close_after_throw(&mut self, iterator: &N) {
        let _ = &iterator;
        panic!("unenumerated binding MarketDataHost::close_after_throw")
    }
    fn construct(&mut self, function: &N, args: &[N]) -> Result<N, N> {
        let _ = &function;
        let _ = &args;
        panic!("unenumerated binding MarketDataHost::construct")
    }
    fn first_binding(&mut self, value: &N) -> Result<N, N> {
        let _ = &value;
        panic!("unenumerated binding MarketDataHost::first_binding")
    }
    fn unit_regex(&mut self) -> Result<N, N> {
        panic!("unenumerated binding MarketDataHost::unit_regex")
    }
    fn template_string(&mut self, value: &N) -> Result<JsString, N> {
        let _ = &value;
        panic!("unenumerated binding MarketDataHost::template_string")
    }
    fn subtract(&mut self, left: &N, right: &N) -> Result<N, N> {
        let _ = &left;
        let _ = &right;
        panic!("unenumerated binding MarketDataHost::subtract")
    }
}
impl JsPayloadHost for Host {
    type Value = N;
    type Error = N;
    fn string_units<'a>(&self, value: &'a N) -> Option<&'a [u16]> {
        let _ = &value;
        JsPayloadHost::string_units(&self.cache, value)
    }
    fn is_object(&self, value: &N) -> bool {
        let _ = &value;
        JsPayloadHost::is_object(&self.cache, value)
    }
    fn is_array(&mut self, value: &N) -> Result<bool, N> {
        let _ = &value;
        JsPayloadHost::is_array(&mut self.cache, value).map_err(|e| panic!("cache primitive {e}"))
    }
    fn is_nullish(&self, value: &N) -> bool {
        let _ = &value;
        JsPayloadHost::is_nullish(&self.cache, value)
    }
    fn undefined(&self) -> N {
        N::U
    }
    fn parse_json(&mut self, text: &[u16]) -> Result<N, N> {
        let _ = &text;
        JsPayloadHost::parse_json(&mut self.cache, text).map_err(|e| panic!("cache primitive {e}"))
    }
    fn get(&mut self, value: &N, property: &str) -> Result<N, N> {
        let _ = &value;
        let _ = &property;
        self.get(value, property)
    }
    fn json_stringify(&mut self, value: &N) -> Result<Option<JsString>, N> {
        let _ = &value;
        JsPayloadHost::json_stringify(&mut self.cache, value)
            .map_err(|e| panic!("cache primitive {e}"))
    }
    fn to_string(&mut self, value: &N) -> Result<JsString, N> {
        let _ = &value;
        JsPayloadHost::to_string(&mut self.cache, value).map_err(|e| panic!("cache primitive {e}"))
    }
}
impl CrossCacheHost for Host {
    fn null(&self) -> N {
        N::Null
    }
    fn is_undefined(&self, value: &N) -> bool {
        let _ = &value;
        panic!("unenumerated binding CrossCacheHost::is_undefined")
    }
    fn truthy(&self, value: &N) -> bool {
        let _ = &value;
        CrossCacheHost::truthy(&self.cache, value)
    }
    fn session_storage(&mut self) -> Result<N, N> {
        panic!("unenumerated binding CrossCacheHost::session_storage")
    }
    fn literal(&self, text: &JsString) -> N {
        let _ = &text;
        panic!("unenumerated binding CrossCacheHost::literal")
    }
    fn object(&mut self) -> Result<N, N> {
        panic!("unenumerated binding CrossCacheHost::object")
    }
    fn set(&mut self, object: &N, key: &N, value: &N) -> Result<(), N> {
        let _ = &object;
        let _ = &key;
        let _ = &value;
        panic!("unenumerated binding CrossCacheHost::set")
    }
    fn define_data(&mut self, object: &N, key: &JsString, value: &N) -> Result<(), N> {
        let _ = &object;
        let _ = &key;
        let _ = &value;
        panic!("unenumerated binding CrossCacheHost::define_data")
    }
    fn call(&mut self, function: &N, receiver: &N, args: &[N]) -> Result<N, N> {
        let _ = &function;
        let _ = &receiver;
        let _ = &args;
        self.invoke(function, receiver, args)
    }
    fn parse_json_value(&mut self, value: &N) -> Result<N, N> {
        let _ = &value;
        panic!("unenumerated binding CrossCacheHost::parse_json_value")
    }
    fn property_key(&mut self, value: &N) -> Result<N, N> {
        let _ = &value;
        panic!("unenumerated binding CrossCacheHost::property_key")
    }
    fn entries(&mut self, map: &N) -> Result<N, N> {
        let _ = &map;
        panic!("unenumerated binding CrossCacheHost::entries")
    }
    fn iterator_step(&mut self, iterator: &N) -> Result<Option<N>, N> {
        let _ = &iterator;
        panic!("unenumerated binding CrossCacheHost::iterator_step")
    }
    fn entry_pair(&mut self, entry: &N) -> Result<[N; 2], N> {
        let _ = &entry;
        panic!("unenumerated binding CrossCacheHost::entry_pair")
    }
    fn close_after_throw(&mut self, iterator: &N) {
        let _ = &iterator;
        panic!("unenumerated binding CrossCacheHost::close_after_throw")
    }
    fn spread(&mut self, value: &N) -> Result<N, N> {
        let _ = &value;
        panic!("unenumerated binding CrossCacheHost::spread")
    }
    fn greater_than_500(&mut self, value: &N) -> Result<bool, N> {
        let _ = &value;
        panic!("unenumerated binding CrossCacheHost::greater_than_500")
    }
}
impl PrivatePositionHost for Host {
    fn number(&mut self, value: &N) -> Result<f64, N> {
        let _ = &value;
        Ok(cache::num(value))
    }
    fn number_value(&self, value: f64) -> N {
        let _ = &value;
        N::Num(value)
    }
    fn lowercase(&mut self, value: &JsString) -> Result<JsString, N> {
        let _ = &value;
        panic!("unenumerated binding PrivatePositionHost::lowercase")
    }
    fn index(&mut self, object: &N, key: &N) -> Result<N, N> {
        let _ = &object;
        let _ = &key;
        panic!("unenumerated binding PrivatePositionHost::index")
    }
    fn add(&mut self, left: &N, right: &N) -> Result<N, N> {
        let _ = &left;
        let _ = &right;
        panic!("unenumerated binding PrivatePositionHost::add")
    }
    fn max_tracked_fill_ids(&mut self) -> Result<N, N> {
        panic!("unenumerated binding PrivatePositionHost::max_tracked_fill_ids")
    }
    fn greater_than(&mut self, value: &N, limit: &N) -> Result<bool, N> {
        let _ = &value;
        let _ = &limit;
        Ok(cache::num(value) > cache::num(limit))
    }
}
impl CoreSnapshotHost for Host {
    fn null_value(&self) -> N {
        panic!("unenumerated binding CoreSnapshotHost::null_value")
    }
    fn add_values(&mut self, a: &N, b: &N) -> Result<N, N> {
        let _ = &a;
        let _ = &b;
        panic!("unenumerated binding CoreSnapshotHost::add_values")
    }
    fn return_await_value(&mut self, value: N) -> impl Future<Output = Result<N, N>> {
        let _ = &value;
        std::future::ready(Err(self.marker.clone()))
    }
    fn await_value(&mut self, value: N) -> impl Future<Output = Result<N, N>> {
        let _ = &value;
        std::future::ready(Err(self.marker.clone()))
    }
    fn error_value(&self, error: &N) -> N {
        let _ = &error;
        panic!("unenumerated binding CoreSnapshotHost::error_value")
    }
    fn construct_error(&mut self, constructor: &N, args: &[N]) -> Result<N, N> {
        let _ = &constructor;
        let _ = &args;
        panic!("unenumerated binding CoreSnapshotHost::construct_error")
    }
    fn instance_of(&mut self, value: &N, constructor: &N) -> Result<bool, N> {
        let _ = &value;
        let _ = &constructor;
        panic!("unenumerated binding CoreSnapshotHost::instance_of")
    }
    fn snapshot_predicate(&mut self, capture: SnapshotPredicate<N>) -> Result<N, N> {
        let _ = &capture;
        panic!("unenumerated binding CoreSnapshotHost::snapshot_predicate")
    }
    fn array_define(&mut self, array: &N, index: usize, value: &N) -> Result<(), N> {
        let _ = &array;
        let _ = &index;
        let _ = &value;
        let N::A(a) = array else {
            panic!("array define")
        };
        assert_eq!(a.borrow().len(), index);
        a.borrow_mut().push(value.clone());
        Ok(())
    }
    fn spread_into(&mut self, object: &N, source: &N) -> Result<(), N> {
        let _ = &object;
        let _ = &source;
        panic!("unenumerated binding CoreSnapshotHost::spread_into")
    }
    fn iterator(&mut self, value: &N) -> Result<N, N> {
        let _ = &value;
        panic!("unenumerated binding CoreSnapshotHost::iterator")
    }
    fn next(&mut self, iterator: &N) -> Result<Option<N>, N> {
        let _ = &iterator;
        panic!("unenumerated binding CoreSnapshotHost::next")
    }
    fn close_after_throw(&mut self, iterator: &N) {
        let _ = &iterator;
        panic!("unenumerated binding CoreSnapshotHost::close_after_throw")
    }
    fn close_after_return(&mut self, iterator: &N) -> Result<(), N> {
        let _ = &iterator;
        panic!("unenumerated binding CoreSnapshotHost::close_after_return")
    }
    fn pair(&mut self, value: &N) -> Result<[N; 2], N> {
        let _ = &value;
        panic!("unenumerated binding CoreSnapshotHost::pair")
    }
}
impl PositionOrderHost for Host {
    fn position_operator(&mut self, operator: &str, left: &N, right: &N) -> Result<N, N> {
        let _ = &operator;
        let _ = &left;
        let _ = &right;
        panic!("unenumerated binding PositionOrderHost::position_operator")
    }
    fn position_unary(&mut self, operator: &str, value: &N) -> Result<N, N> {
        let _ = &operator;
        let _ = &value;
        panic!("unenumerated binding PositionOrderHost::position_unary")
    }
    fn position_callback(&mut self, callback: PositionCallback<N>) -> Result<N, N> {
        let _ = &callback;
        panic!("unenumerated binding PositionOrderHost::position_callback")
    }
    fn set_position_index(&mut self, object: &N, key: &N, value: &N) -> Result<(), N> {
        let _ = &object;
        let _ = &key;
        let _ = &value;
        panic!("unenumerated binding PositionOrderHost::set_position_index")
    }
    fn append_position_spread(&mut self, array: &N, value: &N) -> Result<(), N> {
        let _ = &array;
        let _ = &value;
        panic!("unenumerated binding PositionOrderHost::append_position_spread")
    }
}
impl LifecycleHost for Host {
    fn null(&self) -> N {
        panic!("unenumerated binding LifecycleHost::null")
    }
    fn boolean(&self, value: bool) -> N {
        let _ = &value;
        panic!("unenumerated binding LifecycleHost::boolean")
    }
    fn number(&self, value: f64) -> N {
        let _ = &value;
        panic!("unenumerated binding LifecycleHost::number")
    }
    fn is_nullish(&self, value: &N) -> bool {
        let _ = &value;
        panic!("unenumerated binding LifecycleHost::is_nullish")
    }
    fn super_get(&mut self, receiver: &N, name: &str) -> Result<N, N> {
        let _ = &receiver;
        let _ = &name;
        panic!("unenumerated binding LifecycleHost::super_get")
    }
    fn lifecycle_callback(&mut self, capture: LifecycleCallback<N>) -> Result<N, N> {
        let _ = &capture;
        panic!("unenumerated binding LifecycleHost::lifecycle_callback")
    }
    fn subtract(&mut self, left: &N, right: &N) -> Result<N, N> {
        let _ = &left;
        let _ = &right;
        panic!("unenumerated binding LifecycleHost::subtract")
    }
    fn greater_than(&mut self, left: &N, right: &N) -> Result<bool, N> {
        let _ = &left;
        let _ = &right;
        panic!("unenumerated binding LifecycleHost::greater_than")
    }
    fn template_string(&mut self, value: &N) -> Result<JsString, N> {
        let _ = &value;
        panic!("unenumerated binding LifecycleHost::template_string")
    }
}
impl CorePrivateClassHost for Host {
    fn allocate_private_class(&mut self, base: &N) -> Result<(N, N), N> {
        let proto = object(vec![]);
        let c = object(vec![
            ("length", N::Num(1.0)),
            ("name", s("HyperliquidPrivateWebSocket")),
            ("prototype", proto.clone()),
        ]);
        for key in ["length", "name", "prototype"] {
            let value = JsPayloadHost::get(&mut self.cache, &c, key).unwrap();
            self.put(&c, key, &value, false);
        }
        self.put(&proto, "constructor", &c, false);
        let N::O(o) = &c else { unreachable!() };
        o.borrow_mut().prototype = Some(base.clone());
        let base_proto = JsPayloadHost::get(&mut self.cache, base, "prototype").unwrap();
        let N::O(o) = &proto else { unreachable!() };
        o.borrow_mut().prototype = Some(base_proto);
        self.classes.push((c.clone(), proto.clone()));
        self.labels
            .push((c.clone(), format!("class{}", self.classes.len())));
        self.labels
            .push((proto.clone(), format!("prototype{}", self.classes.len())));
        Ok((c, proto))
    }
    fn private_method(
        &mut self,
        capture: CorePrivateMethodCapture<N>,
        name: &str,
        length: u8,
    ) -> Result<N, N> {
        Ok(self.allocate(Function::Method(capture), name, length))
    }
    fn define_private_method(&mut self, prototype: &N, name: &str, value: &N) -> Result<(), N> {
        self.event(format!("method:{}:{name}", self.label(prototype)))?;
        self.put(prototype, name, value, false);
        self.descriptors
            .push((prototype.clone(), name.into(), false, true, true));
        Ok(())
    }
}
