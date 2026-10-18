#![allow(dead_code)]
use super::fixture::*;
use safe_cex_venue_woo::{
    legacy_fill_recovery as recovery,
    legacy_metadata::{JsString, Task},
    legacy_socket_control as control,
    public_streams::StreamOps,
};
use std::rc::Rc;
#[derive(Clone)]
pub(crate) struct Frame {
    pub(crate) receiver: V,
    pub(crate) ops: Rc<StreamOps<V, E>>,
}
impl Frame {
    fn get(&self, v: &V, k: &str) -> Result<V, E> {
        (self.ops.value.get)(v, k)
    }
    fn method(&self, r: V, k: &str, a: Vec<V>) -> Result<V, E> {
        let f = self.get(&r, k)?;
        (self.ops.value.invoke)(f, r, a)
    }
    fn own(&self, k: &str, a: Vec<V>) -> Result<V, E> {
        self.method(self.receiver.clone(), k, a)
    }
    fn property(&self, k: &str) -> Result<V, E> {
        self.get(&self.receiver, k)
    }
    fn text(v: &V) -> JsString {
        text(v).encode_utf16().collect()
    }
}
impl control::Host for Frame {
    type Value = V;
    type Error = E;
    type Socket = V;
    type Call = (V, V);
    type Signer = V;
    type Iterator = (V, usize);
    type Callback = V;
    fn prepare(&mut self, c: control::Callee) -> Result<Self::Call, E> {
        let k = match c {
            control::Callee::CreateSocket => "createWebsocket",
            control::Callee::Send(false) => "send",
            control::Callee::Send(true) => "sendWithLog",
            control::Callee::EmitError => "emitAndLogError",
            control::Callee::Execution(false) => "handleExecutionReport",
            control::Callee::Execution(true) => "handleAlgoExecutionReport",
        };
        Ok((self.property(k)?, self.receiver.clone()))
    }
    fn disposed(&mut self) -> Result<bool, E> {
        Ok(truthy(&self.property("isDisposed")?))
    }
    fn option(&mut self, k: &str) -> Result<V, E> {
        self.get(&self.get(&self.property("parent")?, "options")?, k)
    }
    fn truthy(&mut self, v: &V) -> bool {
        truthy(v)
    }
    fn nullish(&mut self, v: &V) -> bool {
        matches!(v.as_ref(), Js::U | Js::Null)
    }
    fn is_string(&mut self, v: &V) -> bool {
        matches!(v.as_ref(), Js::S(_))
    }
    fn equals(&mut self, v: &V, s: &str) -> bool {
        matches!(v.as_ref(),Js::S(v)if v==s)
    }
    fn text(&mut self, v: &V) -> Result<JsString, E> {
        Ok(Self::text(v))
    }
    fn field(&mut self, v: &V, k: &str) -> Result<V, E> {
        self.get(v, k)
    }
    fn string(&self, v: JsString) -> V {
        s(&String::from_utf16(&v).unwrap())
    }
    fn number(&self, v: f64) -> V {
        n(v)
    }
    fn object(&self, f: Vec<(&'static str, V)>) -> V {
        obj(f)
    }
    fn stringify(&mut self, v: &V) -> Result<JsString, E> {
        let json = (self.ops.value.global)("JSON")?;
        let result = self.method(json, "stringify", vec![v.clone()])?;
        Ok(Self::text(&result))
    }
    fn parse(&mut self, data: V) -> Result<V, E> {
        let helper = (self.ops.import)("jsonParse")?;
        (self.ops.value.invoke)(helper, u(), vec![data])
    }
    fn create_socket(&mut self, (f, r): Self::Call, url: JsString) -> Result<V, E> {
        (self.ops.value.invoke)(f, r, vec![self.string(url)])
    }
    fn set_socket(&mut self, v: V) {
        set(&self.receiver, "ws", v)
    }
    fn socket(&mut self) -> Result<V, E> {
        self.property("ws")
    }
    fn add_listener(&mut self, ws: V, event: &str, listener: control::Listener) -> Result<(), E> {
        let f = self.get(&ws, "addEventListener")?;
        let callback = self.property(match listener {
            control::Listener::Open => "onOpen",
            control::Listener::Message => "onMessage",
            control::Listener::Close => "onClose",
        })?;
        (self.ops.value.invoke)(f, ws, vec![s(event), callback])?;
        Ok(())
    }
    fn send(&mut self, (f, r): Self::Call, t: JsString, _: bool) -> Result<(), E> {
        (self.ops.value.invoke)(f, r, vec![self.string(t)])?;
        Ok(())
    }
    fn ping(&mut self, _: control::Channel) -> Result<(), E> {
        self.own("ping", vec![])?;
        Ok(())
    }
    fn pong(&mut self) -> Result<(), E> {
        self.own("handlePongEvent", vec![])?;
        Ok(())
    }
    fn reset_orderbook(&mut self) -> Result<(), E> {
        self.own("resetOrderbook", vec![])?;
        Ok(())
    }
    fn subscribe(&mut self, _: control::Channel) -> Result<(), E> {
        self.own("subscribe", vec![])?;
        Ok(())
    }
    fn reconnecting_enabled(&mut self) -> bool {
        false
    }
    fn emit_reconnected(&mut self) -> Result<(), E> {
        self.own("emitWsReconnected", vec![])?;
        Ok(())
    }
    fn topics(&mut self) -> Result<V, E> {
        self.property("topics")
    }
    fn object_values(&mut self, v: V) -> Result<Vec<V>, E> {
        let Js::O(fields) = v.as_ref() else {
            panic!("object values")
        };
        Ok(fields.borrow().iter().map(|(_, v)| v.clone()).collect())
    }
    fn flattenable_values(&mut self, v: &V) -> Result<Option<Vec<V>>, E> {
        Ok(if let Js::A(a) = v.as_ref() {
            Some(a.borrow().clone())
        } else {
            None
        })
    }
    fn track_last_received(&mut self) -> Result<(), E> {
        self.own("trackLastReceived", vec![])?;
        Ok(())
    }
    fn diagnostic_receive(&mut self, data: &V) -> Result<(), E> {
        self.own("emitWsDiagnosticReceive", vec![data.clone()])?;
        Ok(())
    }
    fn handlers(&mut self, k: &[u16]) -> Result<V, E> {
        self.get(
            &self.property("messageHandlers")?,
            &String::from_utf16(k).unwrap(),
        )
    }
    fn stats(&mut self, k: JsString) -> Result<(), E> {
        self.own("recordMessageHandlerStats", vec![self.string(k)])?;
        Ok(())
    }
    fn iterator(&mut self, v: V) -> Result<Self::Iterator, E> {
        assert!(matches!(v.as_ref(), Js::A(_)));
        Ok((v, 0))
    }
    fn next(&mut self, (v, i): &mut Self::Iterator) -> Result<Option<V>, E> {
        let Js::A(values) = v.as_ref() else {
            panic!("iterator")
        };
        let out = values.borrow().get(*i).cloned();
        *i += 1;
        Ok(out)
    }
    fn callback(&mut self, callback: V, json: &V) -> Result<(), E> {
        (self.ops.value.invoke)(callback, u(), vec![json.clone()])?;
        Ok(())
    }
    fn execution_report(&mut self, (f, r): Self::Call, data: V, _: bool) -> Result<(), E> {
        (self.ops.value.invoke)(f, r, vec![data])?;
        Ok(())
    }
    fn emit_error(&mut self, (f, r): Self::Call, error: V) -> Result<(), E> {
        (self.ops.value.invoke)(f, r, vec![error])?;
        Ok(())
    }
    fn timestamp(&mut self) -> Result<f64, E> {
        Ok(73.)
    }
    fn nanoid(&mut self) -> Result<JsString, E> {
        Ok("id".encode_utf16().collect())
    }
    fn prepare_signer(&mut self, signer: V) -> Result<V, E> {
        Ok(signer)
    }
    fn sign(&mut self, _: V, _: V) -> Result<Task<V, E>, E> {
        panic!("remote signer outside fixture floor")
    }
    fn hmac_sha256_hex(&mut self, secret: V, t: JsString) -> Result<JsString, E> {
        Ok(safe_cex_crypto::hmac_sha256(
            text(&secret).as_bytes(),
            String::from_utf16(&t).unwrap().as_bytes(),
            safe_cex_crypto::Encoding::Hex,
        )
        .encode_utf16()
        .collect())
    }
    fn private_snapshot(&mut self) -> Result<(), E> {
        let parent = self.property("parent")?;
        self.method(parent, "fetchAndSetPrivateSnapshot", vec![])?;
        Ok(())
    }
}
impl recovery::Host for Frame {
    type Value = V;
    type Error = E;
    type Prepared = (V, V);
    type MapIter = ();
    type Iterator = (V, usize);
    type MapTarget = ();
    fn prepare(&mut self, b: recovery::Binding) -> Result<Self::Prepared, E> {
        let (r, k) = match b {
            recovery::Binding::MapLimit => (self.property("parent")?, "mapLimitOrder"),
            recovery::Binding::MapAlgo => (self.property("parent")?, "mapAlgoOrder"),
            recovery::Binding::ParseTimestamp => (self.receiver.clone(), "parseTimestampToSeconds"),
            _ => panic!("binding outside report floor: {b:?}"),
        };
        Ok((self.get(&r, k)?, r))
    }
    fn invoke(&mut self, (f, r): Self::Prepared, args: Vec<V>) -> Result<V, E> {
        (self.ops.value.invoke)(f, r, args)
    }
    fn call(&mut self, _: Self::Prepared, _: Vec<V>) -> Result<Task<V, E>, E> {
        panic!("promise outside report floor")
    }
    fn root(&mut self, k: &str) -> Result<V, E> {
        self.property(k)
    }
    fn set_root(&mut self, k: &str, v: V) -> Result<(), E> {
        set(&self.receiver, k, v);
        Ok(())
    }
    fn field(&mut self, v: &V, k: &str) -> Result<V, E> {
        self.get(v, k)
    }
    fn key_field(&mut self, v: &V, k: Vec<u16>) -> Result<V, E> {
        self.get(v, &String::from_utf16(&k).unwrap())
    }
    fn v(&mut self, v: &V, k: &str) -> Result<V, E> {
        self.get(v, k)
    }
    fn length(&mut self, v: &V) -> Result<usize, E> {
        let Js::A(a) = v.as_ref() else {
            panic!("array length")
        };
        Ok(a.borrow().len())
    }
    fn index(&mut self, v: &V, i: usize) -> Result<V, E> {
        self.get(v, &i.to_string())
    }
    fn has_index(&mut self, v: &V, i: usize) -> Result<bool, E> {
        let Js::A(a) = v.as_ref() else {
            panic!("array index")
        };
        Ok(i < a.borrow().len())
    }
    fn is_array(&self, v: &V) -> bool {
        matches!(v.as_ref(), Js::A(_))
    }
    fn truthy(&self, v: &V) -> bool {
        truthy(v)
    }
    fn nullish(&self, v: &V) -> bool {
        matches!(v.as_ref(), Js::U | Js::Null)
    }
    fn is_undefined(&self, v: &V) -> bool {
        matches!(v.as_ref(), Js::U)
    }
    fn equals(&self, v: &V, t: &str) -> bool {
        matches!(v.as_ref(),Js::S(s)if s==t)
    }
    fn same(&self, a: &V, b: &V) -> bool {
        Rc::ptr_eq(a, b)
    }
    fn number(&self, v: f64) -> V {
        n(v)
    }
    fn boolean(&self, v: bool) -> V {
        b(v)
    }
    fn string(&self, v: Vec<u16>) -> V {
        s(&String::from_utf16(&v).unwrap())
    }
    fn undefined(&self) -> V {
        u()
    }
    fn object(&self, v: Vec<(&'static str, V)>) -> V {
        obj(v)
    }
    fn array(&self, v: Vec<V>) -> V {
        arr(v)
    }
    fn text(&mut self, v: &V) -> Result<Vec<u16>, E> {
        Ok(Self::text(v))
    }
    fn number_coerce(&mut self, v: &V) -> Result<f64, E> {
        Ok(number(v))
    }
    fn operator_number(&mut self, v: &V) -> Result<f64, E> {
        Ok(number(v))
    }
    fn add(&mut self, a: V, b: V) -> Result<V, E> {
        Ok(n(number(&a) + number(&b)))
    }
    fn sort_by_id(&mut self, _: Vec<V>) -> Result<Vec<V>, E> {
        panic!("sort outside report floor")
    }
    fn prepare_map_set(&mut self, _: recovery::Map) -> Result<(), E> {
        panic!("map outside report floor")
    }
    fn map_set_prepared(&mut self, _: (), _: V, _: V) -> Result<(), E> {
        panic!("map outside report floor")
    }
    fn map_get(&mut self, _: recovery::Map, _: &V) -> Result<V, E> {
        panic!("map outside report floor")
    }
    fn map_has(&mut self, _: recovery::Map, _: &V) -> Result<bool, E> {
        panic!("map outside report floor")
    }
    fn map_set(&mut self, _: recovery::Map, _: V, _: V) -> Result<(), E> {
        panic!("map outside report floor")
    }
    fn map_delete(&mut self, _: recovery::Map, _: &V) -> Result<(), E> {
        panic!("map outside report floor")
    }
    fn map_clear(&mut self, _: recovery::Map) -> Result<(), E> {
        panic!("map outside report floor")
    }
    fn map_iterator(&mut self, _: recovery::Map) -> Result<(), E> {
        panic!("map outside report floor")
    }
    fn map_next(&mut self, _: &mut ()) -> Result<Option<V>, E> {
        panic!("map outside report floor")
    }
    fn iterator(&mut self, v: V) -> Result<Self::Iterator, E> {
        Ok((v, 0))
    }
    fn next(&mut self, (v, i): &mut Self::Iterator) -> Result<Option<V>, E> {
        let Js::A(a) = v.as_ref() else {
            panic!("array iterator")
        };
        let result = a.borrow().get(*i).cloned();
        *i += 1;
        Ok(result)
    }
    fn close_iterator(&mut self, _: &mut Self::Iterator) -> Result<(), E> {
        Ok(())
    }
    fn timeout(
        &mut self,
        _: Self::Prepared,
        _: f64,
        _: Rc<dyn Fn() -> Result<Task<V, E>, E>>,
    ) -> Result<V, E> {
        panic!("timer outside report floor")
    }
    fn error_field(&mut self, _: &E, _: &[&str]) -> Result<V, E> {
        panic!("error outside report floor")
    }
    fn emit_error(&mut self, _: Self::Prepared, _: V) -> Result<(), E> {
        panic!("emit outside report floor")
    }
    fn order_side(&mut self, _: V) -> Result<V, E> {
        panic!("side outside report floor")
    }
}
