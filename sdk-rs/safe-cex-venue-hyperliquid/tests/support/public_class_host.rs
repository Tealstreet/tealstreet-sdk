#![allow(dead_code)]
use super::{
    cache::{N, arr, object, s},
    private_host,
};
use safe_cex_venue_hyperliquid::{
    candle_listeners::{CandleCallback, CandleListenerHost},
    core_book_listener::{CoreBookCallback, CoreBookHost},
    core_public_message::CorePublicMessageHost,
    cross_cache::CrossCacheHost,
    json_boundaries::JsPayloadHost,
    lifecycle_delegates::{LifecycleCallback, LifecycleHost},
    private_constructor::PrivateConstructorHost,
    private_message_dispatch::{DispatchOperator, PrivateMessageHost},
    public_class::*,
    public_constructor::*,
    spot_json::JsString,
    ws_emission::{DiagnosticScope, WsEmissionHost},
    ws_ping_lifecycle::{self, PingCallback, WsPingHost},
    ws_transport_leaves::{WsTransportHost, WsVenue},
};
use std::collections::HashMap;
#[derive(Clone)]
pub enum Function {
    Arrow(PublicSocketCapture<N>),
    Dispose(N),
    Ping(PingCallback<N>),
}
#[derive(Default)]
pub struct Host {
    pub inner: private_host::Host,
    pub functions: HashMap<String, Function>,
    pub metadata: Vec<(N, String, u8)>,
    pub sent: Vec<N>,
    pub send_fail: bool,
    pub before_send_sizes: Vec<usize>,
    pub current_socket: Option<N>,
    pub close_count: usize,
    pub topic_subscriptions: Vec<N>,
    pub lifecycle_calls: Vec<(&'static str, N)>,
}
pub fn same(a: &N, b: &N) -> bool {
    private_host::same(a, b)
}
impl Host {
    pub fn allocate(&mut self, c: Function, name: &str, len: u8) -> N {
        let id = format!("public/{}", self.functions.len());
        let v = N::Builtin(id.clone());
        self.functions.insert(id, c);
        self.metadata.push((v.clone(), name.into(), len));
        v
    }
    pub fn put(&mut self, o: &N, k: &str, v: &N) {
        self.inner.put(o, k, v, true)
    }
    pub fn get(&mut self, o: &N, k: &str) -> Result<N, N> {
        if let N::Builtin(id) = o {
            if ["JSON", "Object", "flatten"].contains(&id.as_str()) {
                return Ok(N::Builtin(format!("{id}.{k}")));
            }
        }
        if matches!(o, N::S(_)) && k == "split" {
            return Ok(N::Builtin("split".into()));
        }
        self.inner.get(o, k)
    }
    pub fn invoke(&mut self, f: &N, r: &N, a: &[N]) -> Result<N, N> {
        if let N::Builtin(id) = f {
            if let Some(c) = self.functions.get(id).cloned() {
                return match c {
                    Function::Arrow(c) => {
                        if c.body == PublicSocketArrow::SendSubscription {
                            self.topic_subscriptions
                                .push(a.first().cloned().unwrap_or(N::U));
                        }
                        if c.body == PublicSocketArrow::StopPingLoop {
                            self.lifecycle_calls
                                .push(("stopPingLoop", c.receiver.clone()));
                        }
                        call_core_public_arrow(self, &c, a)
                    }
                    Function::Dispose(c) => call_core_public_dispose(self, &c, r),
                    Function::Ping(c) => {
                        ws_ping_lifecycle::run_ping_timeout(self, &c)?;
                        Ok(N::U)
                    }
                };
            }
            match id.as_str() {
                "JSON.stringify" => {
                    return Ok(JsPayloadHost::json_stringify(&mut self.inner.cache, &a[0])
                        .unwrap()
                        .map_or(N::U, N::S));
                }
                "Object.keys" => {
                    let N::O(o) = &a[0] else {
                        panic!("native own keys")
                    };
                    return Ok(arr(o
                        .borrow()
                        .fields
                        .iter()
                        .filter(|p| p.2)
                        .map(|p| s(&p.0))
                        .collect()));
                }
                "flatten" => return Ok(a[0].clone()),
                "split" => {
                    let (N::S(t), N::S(sep)) = (r, &a[0]) else {
                        panic!("native split")
                    };
                    assert_eq!(sep.0, vec![46]);
                    return Ok(arr(t
                        .0
                        .split(|u| *u == 46)
                        .map(|u| N::S(JsString(u.to_vec())))
                        .collect()));
                }
                "send" => {
                    if let Some(socket) = self.current_socket.clone() {
                        let active = self.get(&socket, "activeSubscriptions")?;
                        let N::Set(set) = active else {
                            panic!("native active set")
                        };
                        self.before_send_sizes.push(set.borrow().len())
                    };
                    if self.send_fail {
                        return Err(self.inner.marker.clone());
                    };
                    self.sent.push(a[0].clone());
                    return Ok(N::U);
                }
                "shouldEmitHyperliquidWsSendDiagnostic" => return Ok(N::B(false)),
                "super.dispose" => {
                    assert!(a.is_empty(), "PUBLIC native super dispose arity");
                    self.lifecycle_calls.push(("super.dispose", r.clone()));
                    self.inner.events.push("public-super-dispose".into());
                    return Ok(N::U);
                }
                _ => {}
            }
        }
        self.inner.invoke(f, r, a)
    }
}
impl WsPingHost for Host {
    type Value = N;
    type Error = N;
    fn undefined(&self) -> N {
        N::U
    }
    fn literal(&self, v: &JsString) -> N {
        N::S(v.clone())
    }
    fn truthy(&self, v: &N) -> bool {
        WsPingHost::truthy(&self.inner, v)
    }
    fn global(&mut self, k: &str) -> Result<N, N> {
        if [
            "JSON",
            "Object",
            "flatten",
            "shouldEmitHyperliquidWsSendDiagnostic",
        ]
        .contains(&k)
        {
            self.inner.events.push(format!("global:{k}"));
            Ok(N::Builtin(k.into()))
        } else {
            WsPingHost::global(&mut self.inner, k)
        }
    }
    fn get(&mut self, o: &N, k: &str) -> Result<N, N> {
        self.get(o, k)
    }
    fn set(&mut self, o: &N, k: &str, v: &N) -> Result<(), N> {
        WsPingHost::set(&mut self.inner, o, k, v)
    }
    fn call(&mut self, f: &N, r: &N, a: &[N]) -> Result<N, N> {
        self.invoke(f, r, a)
    }
    fn object(&mut self) -> Result<N, N> {
        WsPingHost::object(&mut self.inner)
    }
    fn define_data(&mut self, o: &N, k: &str, v: &N) -> Result<(), N> {
        WsPingHost::define_data(&mut self.inner, o, k, v)
    }
    fn callback(&mut self, c: PingCallback<N>) -> Result<N, N> {
        Ok(self.allocate(Function::Ping(c), "", 0))
    }
}
impl PublicConstructorHost for Host {
    fn number(&self, n: f64) -> N {
        N::Num(n)
    }
    fn construct(&mut self, c: &N, a: &[N]) -> Result<N, N> {
        PrivateConstructorHost::construct(&mut self.inner, c, a)
    }
    fn public_super(&mut self, c: &N, p: &N, t: &N) -> Result<N, N> {
        PrivateConstructorHost::private_super(&mut self.inner, c, p, t)
    }
    fn public_arrow(&mut self, c: PublicSocketCapture<N>, name: &str, len: u8) -> Result<N, N> {
        Ok(self.allocate(Function::Arrow(c), name, len))
    }
}
impl CorePublicClassHost for Host {
    fn allocate_public_class(&mut self, b: &N) -> Result<(N, N), N> {
        let p = object(vec![]);
        let c = object(vec![
            ("prototype", p.clone()),
            ("name", s("HyperliquidPublicWebSocket")),
            ("length", N::Num(1.)),
        ]);
        self.inner.put(&p, "constructor", &c, false);
        let bp = self.get(b, "prototype")?;
        let N::O(co) = &c else { unreachable!() };
        co.borrow_mut().prototype = Some(b.clone());
        let N::O(po) = &p else { unreachable!() };
        po.borrow_mut().prototype = Some(bp);
        self.inner.classes.push((c.clone(), p.clone()));
        Ok((c, p))
    }
    fn public_dispose_method(&mut self, c: &N) -> Result<N, N> {
        Ok(self.allocate(Function::Dispose(c.clone()), "dispose", 0))
    }
    fn define_public_method(&mut self, p: &N, k: &str, v: &N) -> Result<(), N> {
        self.inner.put(p, k, v, false);
        self.inner
            .descriptors
            .push((p.clone(), k.into(), false, true, true));
        Ok(())
    }
    fn public_super_get(&mut self, c: &N, r: &N, k: &str) -> Result<N, N> {
        let p = self.get(c, "prototype")?;
        let N::O(p) = p else { panic!("prototype") };
        let bp = p.borrow().prototype.clone().unwrap();
        self.inner
            .events
            .push(format!("super:{k}:{}", self.inner.label(r)));
        self.get(&bp, k)
    }
}
impl WsTransportHost for Host {
    fn urls_binding(&mut self, venue: WsVenue) -> Result<N, N> {
        WsTransportHost::urls_binding(&mut self.inner, venue)
    }
    fn boolean(&self, value: bool) -> N {
        N::B(value)
    }
}
impl WsEmissionHost for Host {
    fn boolean(&self, value: bool) -> N {
        N::B(value)
    }
    fn system_types(&mut self, venue: WsVenue) -> Result<N, N> {
        WsEmissionHost::system_types(&mut self.inner, venue)
    }
    fn iterator(&mut self, value: &N) -> Result<N, N> {
        Ok(CrossCacheHost::entries(&mut self.inner.cache, value).unwrap())
    }
    fn next(&mut self, iterator: &N) -> Result<Option<N>, N> {
        Ok(CrossCacheHost::iterator_step(&mut self.inner.cache, iterator).unwrap())
    }
    fn close_after_throw(&mut self, _iterator: &N) {
        {
            self.close_count += 1;
        }
    }
    fn triple(&mut self, value: &N) -> Result<[N; 3], N> {
        {
            let N::A(a) = value else {
                panic!("native triple")
            };
            let a = a.borrow();
            Ok(std::array::from_fn(|i| a.get(i).cloned().unwrap_or(N::U)))
        }
    }
    fn string_units<'a>(&self, value: &'a N) -> Option<&'a [u16]> {
        if let N::S(v) = value {
            Some(&v.0)
        } else {
            None
        }
    }
    fn spread_into(&mut self, target: &N, source: &N) -> Result<(), N> {
        WsEmissionHost::spread_into(&mut self.inner, target, source)
    }
    fn formatter(&mut self, scope: DiagnosticScope) -> Result<N, N> {
        WsEmissionHost::formatter(&mut self.inner, scope)
    }
}
impl PrivateMessageHost for Host {
    fn typeof_value<'a>(&self, value: &'a N) -> &'a str {
        PrivateMessageHost::typeof_value(&self.inner, value)
    }
    fn has_property(&mut self, name: &str, object: &N) -> Result<bool, N> {
        PrivateMessageHost::has_property(&mut self.inner, name, object)
    }
    fn valid_fill_predicate(&mut self) -> Result<N, N> {
        PrivateMessageHost::valid_fill_predicate(&mut self.inner)
    }
    fn for_in(&mut self, object: &N) -> Result<N, N> {
        PrivateMessageHost::for_in(&mut self.inner, object)
    }
    fn for_in_next(&mut self, iterator: &N) -> Result<Option<N>, N> {
        PrivateMessageHost::for_in_next(&mut self.inner, iterator)
    }
    fn construct(&mut self, function: &N, args: &[N]) -> Result<N, N> {
        PrivateMessageHost::construct(&mut self.inner, function, args)
    }
    fn regex(&mut self, pattern: &str) -> Result<N, N> {
        PrivateMessageHost::regex(&mut self.inner, pattern)
    }
    fn number(&self, value: f64) -> N {
        PrivateMessageHost::number(&self.inner, value)
    }
    fn boolean(&self, value: bool) -> N {
        PrivateMessageHost::boolean(&self.inner, value)
    }
    fn is_nullish(&self, value: &N) -> bool {
        PrivateMessageHost::is_nullish(&self.inner, value)
    }
    fn is_null(&self, value: &N) -> bool {
        PrivateMessageHost::is_null(&self.inner, value)
    }
    fn strict_equal(&self, a: &N, b: &N) -> bool {
        PrivateMessageHost::strict_equal(&self.inner, a, b)
    }
    fn index(&mut self, object: &N, key: &N) -> Result<N, N> {
        PrivateMessageHost::index(&mut self.inner, object, key)
    }
    fn array(&mut self) -> Result<N, N> {
        PrivateMessageHost::array(&mut self.inner)
    }
    fn template_string(&mut self, value: &N) -> Result<JsString, N> {
        PrivateMessageHost::template_string(&mut self.inner, value)
    }
    fn operator(&mut self, op: DispatchOperator, a: &N, b: &N) -> Result<N, N> {
        PrivateMessageHost::operator(&mut self.inner, op, a, b)
    }
}
impl CorePublicMessageHost for Host {
    fn event_data(&mut self, event: &N) -> Result<N, N> {
        let _ = event;
        panic!("outside public class witness: CorePublicMessageHost::event_data")
    }
    fn is_nullish(&self, value: &N) -> bool {
        let _ = value;
        panic!("outside public class witness: CorePublicMessageHost::is_nullish")
    }
    fn is_array(&mut self, value: &N) -> Result<bool, N> {
        let _ = value;
        panic!("outside public class witness: CorePublicMessageHost::is_array")
    }
    fn equal(&self, left: &N, right: &N) -> bool {
        let _ = left;
        let _ = right;
        panic!("outside public class witness: CorePublicMessageHost::equal")
    }
    fn number(&self, value: f64) -> N {
        let _ = value;
        panic!("outside public class witness: CorePublicMessageHost::number")
    }
    fn less(&mut self, left: &N, right: &N) -> Result<bool, N> {
        let _ = left;
        let _ = right;
        panic!("outside public class witness: CorePublicMessageHost::less")
    }
    fn subtract(&mut self, left: &N, right: &N) -> Result<N, N> {
        let _ = left;
        let _ = right;
        panic!("outside public class witness: CorePublicMessageHost::subtract")
    }
    fn divide(&mut self, left: &N, right: &N) -> Result<N, N> {
        let _ = left;
        let _ = right;
        panic!("outside public class witness: CorePublicMessageHost::divide")
    }
    fn index(&mut self, value: &N, key: &N) -> Result<N, N> {
        let _ = value;
        let _ = key;
        panic!("outside public class witness: CorePublicMessageHost::index")
    }
    fn array(&mut self) -> Result<N, N> {
        panic!("outside public class witness: CorePublicMessageHost::array")
    }
    fn template_string(&mut self, value: &N) -> Result<JsString, N> {
        let _ = value;
        panic!("outside public class witness: CorePublicMessageHost::template_string")
    }
    fn json_parse(&mut self, data: &N) -> Result<N, N> {
        let _ = data;
        panic!("outside public class witness: CorePublicMessageHost::json_parse")
    }
}
impl CandleListenerHost for Host {
    fn number(&self, value: f64) -> N {
        let _ = value;
        panic!("outside public class witness: CandleListenerHost::number")
    }
    fn boolean(&self, value: bool) -> N {
        let _ = value;
        panic!("outside public class witness: CandleListenerHost::boolean")
    }
    fn nullish(&self, value: &N) -> bool {
        let _ = value;
        panic!("outside public class witness: CandleListenerHost::nullish")
    }
    fn strict_equal(&self, left: &N, right: &N) -> bool {
        let _ = left;
        let _ = right;
        panic!("outside public class witness: CandleListenerHost::strict_equal")
    }
    fn binary(&mut self, operator: &str, left: &N, right: &N) -> Result<N, N> {
        let _ = operator;
        let _ = left;
        let _ = right;
        panic!("outside public class witness: CandleListenerHost::binary")
    }
    fn decrement(&mut self, value: &N) -> Result<N, N> {
        let _ = value;
        panic!("outside public class witness: CandleListenerHost::decrement")
    }
    fn template_string(&mut self, value: &N) -> Result<JsString, N> {
        let _ = value;
        panic!("outside public class witness: CandleListenerHost::template_string")
    }
    fn index(&mut self, object: &N, key: &N) -> Result<N, N> {
        let _ = object;
        let _ = key;
        panic!("outside public class witness: CandleListenerHost::index")
    }
    fn assign_index(&mut self, object: &N, key: &N, value: &N) -> Result<(), N> {
        let _ = object;
        let _ = key;
        let _ = value;
        panic!("outside public class witness: CandleListenerHost::assign_index")
    }
    fn array(&mut self, values: &[N]) -> Result<N, N> {
        let _ = values;
        panic!("outside public class witness: CandleListenerHost::array")
    }
    fn spread_append(&mut self, iterable: &N, last: &N) -> Result<N, N> {
        let _ = iterable;
        let _ = last;
        panic!("outside public class witness: CandleListenerHost::spread_append")
    }
    fn spread_array(&mut self, iterable: &N) -> Result<N, N> {
        let _ = iterable;
        panic!("outside public class witness: CandleListenerHost::spread_array")
    }
    fn candle_callback(&mut self, capture: CandleCallback<N>) -> Result<N, N> {
        let _ = capture;
        panic!("outside public class witness: CandleListenerHost::candle_callback")
    }
}
impl CoreBookHost for Host {
    fn imported(&mut self, name: &str) -> Result<N, N> {
        let _ = name;
        panic!("outside public class witness: CoreBookHost::imported")
    }
    fn spread_object(&mut self, source: &N) -> Result<N, N> {
        let _ = source;
        panic!("outside public class witness: CoreBookHost::spread_object")
    }
    fn delete_index(&mut self, object: &N, key: &N) -> Result<(), N> {
        let _ = object;
        let _ = key;
        panic!("outside public class witness: CoreBookHost::delete_index")
    }
    fn construct(&mut self, constructor: &N, args: &[N]) -> Result<N, N> {
        let _ = constructor;
        let _ = args;
        panic!("outside public class witness: CoreBookHost::construct")
    }
    fn throw_value(&self, value: N) -> N {
        let _ = value;
        panic!("outside public class witness: CoreBookHost::throw_value")
    }
    fn book_callback(&mut self, capture: CoreBookCallback<N>) -> Result<N, N> {
        let _ = capture;
        panic!("outside public class witness: CoreBookHost::book_callback")
    }
}
impl LifecycleHost for Host {
    fn null(&self) -> N {
        LifecycleHost::null(&self.inner)
    }
    fn boolean(&self, value: bool) -> N {
        LifecycleHost::boolean(&self.inner, value)
    }
    fn number(&self, value: f64) -> N {
        LifecycleHost::number(&self.inner, value)
    }
    fn is_nullish(&self, value: &N) -> bool {
        LifecycleHost::is_nullish(&self.inner, value)
    }
    fn super_get(&mut self, receiver: &N, name: &str) -> Result<N, N> {
        LifecycleHost::super_get(&mut self.inner, receiver, name)
    }
    fn lifecycle_callback(&mut self, capture: LifecycleCallback<N>) -> Result<N, N> {
        LifecycleHost::lifecycle_callback(&mut self.inner, capture)
    }
    fn subtract(&mut self, left: &N, right: &N) -> Result<N, N> {
        LifecycleHost::subtract(&mut self.inner, left, right)
    }
    fn greater_than(&mut self, left: &N, right: &N) -> Result<bool, N> {
        LifecycleHost::greater_than(&mut self.inner, left, right)
    }
    fn template_string(&mut self, value: &N) -> Result<JsString, N> {
        LifecycleHost::template_string(&mut self.inner, value)
    }
}
