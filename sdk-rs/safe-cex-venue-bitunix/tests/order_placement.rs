use safe_cex_venue_bitunix::order_placement::*;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::{Future, poll_fn},
    pin::pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn bits(n: f64) -> Value {
    json!({"bits":format!("{:016x}",if n.is_nan(){0x7ff8_0000_0000_0000}else{n.to_bits()})})
}
fn undefined() -> Value {
    json!({"$undefined":true})
}
fn string(units: Vec<u16>) -> Value {
    json!({"$units":units})
}
fn units(v: &Value) -> Option<Vec<u16>> {
    if let Some(s) = v.as_str() {
        Some(s.encode_utf16().collect())
    } else {
        v.get("$units")
            .map(|v| serde_json::from_value(v.clone()).unwrap())
    }
}
fn text(s: &str) -> Value {
    json!({"units":s.encode_utf16().collect::<Vec<_>>()})
}
fn number(v: &Value) -> Option<f64> {
    v.as_f64().or_else(|| {
        v.get("$bits")
            .and_then(Value::as_str)
            .map(|s| f64::from_bits(u64::from_str_radix(s, 16).unwrap()))
    })
}
fn obs(v: &Value) -> Value {
    if v.get("$undefined") == Some(&json!(true)) {
        return json!({"undefined":true});
    }
    if let Some(units) = units(v) {
        return json!({"units":units});
    }
    if let Some(n) = number(v) {
        return bits(n);
    }
    match v {
        Value::Array(a) => json!(a.iter().map(obs).collect::<Vec<_>>()),
        Value::Object(o) => {
            let keys = safe_cex_num::js_own_keys(o.keys().map(String::as_str));
            json!({"entries":keys.iter().map(|k|json!([k,obs(&o[*k])])).collect::<Vec<_>>()})
        }
        _ => v.clone(),
    }
}
fn truth(v: &Value) -> bool {
    if v.get("$undefined") == Some(&json!(true)) || v.is_null() || v == false {
        return false;
    }
    if let Some(s) = units(v) {
        return !s.is_empty();
    }
    if let Some(n) = number(v) {
        return n != 0. && !n.is_nan();
    }
    true
}
fn source_string(v: &Value) -> Vec<u16> {
    if let Some(s) = units(v) {
        return s;
    }
    if v.get("$undefined") == Some(&json!(true)) {
        return "undefined".encode_utf16().collect();
    }
    if let Some(n) = number(v) {
        return safe_cex_num::js_number_string(n).encode_utf16().collect();
    }
    safe_cex_num::js_string(Some(v))
        .unwrap()
        .encode_utf16()
        .collect()
}
type Error = Rc<Value>;
fn error_message(message: Vec<u16>) -> Error {
    Rc::new(json!({"name":"Error","message":string(message)}))
}
fn failed(name: &str) -> Error {
    error_message(format!("{name} failed").encode_utf16().collect())
}
fn error_obs(e: &Error) -> Value {
    json!({"error":[text(e["name"].as_str().unwrap()),obs(&e["message"])]})
}
fn ready<T>(f: impl Future<Output = T>) -> T {
    let mut f = pin!(f);
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..3 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
    }
    panic!("PLACE pending beyond witness floor")
}
fn contains(text: &[u16], needle: &str) -> bool {
    let needle = needle.encode_utf16().collect::<Vec<_>>();
    text.windows(needle.len()).any(|v| v == needle)
}
struct PlaceHost {
    row: Value,
    events: Vec<Value>,
    options: Rc<RefCell<Value>>,
    payloads: Rc<Value>,
    positions: Rc<Value>,
    result: Rc<Value>,
    attempts: usize,
    formats: usize,
    first_error: Option<Error>,
}
impl PlaceHost {
    fn note(&mut self, name: &str, args: Vec<Value>) -> Result<(), Error> {
        let mut event = vec![json!(name)];
        event.extend(args);
        let event = json!(event);
        let i = self.events.len();
        assert_eq!(
            event, self.row["events"][i],
            "PLACE native effect {name}:{i}"
        );
        self.events.push(event);
        if self.row["fixture"]["fail"] == name {
            Err(failed(name))
        } else {
            Ok(())
        }
    }
    fn first_failure(&mut self) -> Error {
        let message = self.row["fixture"]["firstError"].as_str().unwrap_or("");
        let mut value = (*error_message(message.encode_utf16().collect())).clone();
        if let Some(msg) = self.row["fixture"]["responseMessage"].as_str() {
            value["response"] = json!({"data":{"msg":msg}});
        }
        let value = Rc::new(value);
        self.first_error = Some(value.clone());
        value
    }
}
impl PlaceOrderHost for PlaceHost {
    type Options = Rc<RefCell<Value>>;
    type Payloads = Rc<Value>;
    type Positions = Rc<Value>;
    type Result = Rc<Value>;
    type Message = Rc<Value>;
    type Error = Error;
    fn resolve_caller(&mut self, opts: &Self::Options) -> Result<(), Error> {
        assert!(
            Rc::ptr_eq(opts, &self.options),
            "PLACE native option identity"
        );
        self.note("resolve", vec![])
    }
    async fn hydrate(&mut self, opts: &Self::Options) -> Result<(), Error> {
        assert!(
            Rc::ptr_eq(opts, &self.options),
            "PLACE native hydration identity"
        );
        self.note("hydrate", vec![])?;
        if self.row["fixture"]["firstAt"] == "hydrate" {
            return Err(self.first_failure());
        }
        Ok(())
    }
    fn format(&mut self, opts: &Self::Options) -> Result<Self::Payloads, Error> {
        assert!(
            Rc::ptr_eq(opts, &self.options),
            "PLACE native formatting identity"
        );
        self.note("format", vec![])?;
        let first = self.formats == 0;
        self.formats += 1;
        if first && self.row["fixture"]["firstAt"] == "format" {
            return Err(self.first_failure());
        }
        Ok(self.payloads.clone())
    }
    async fn batch(&mut self, payloads: Self::Payloads) -> Result<Self::Result, Error> {
        assert!(
            Rc::ptr_eq(&payloads, &self.payloads),
            "PLACE native payload identity"
        );
        self.note("batch", vec![])?;
        let first = self.attempts == 0;
        self.attempts += 1;
        if first
            && self.row["fixture"].get("firstError").is_some()
            && self.row["fixture"].get("firstAt").is_none()
        {
            return Err(self.first_failure());
        }
        if self.attempts > 1 {
            if let Some(message) = self.row["fixture"]["secondError"].as_str() {
                return Err(error_message(message.encode_utf16().collect()));
            }
        }
        Ok(self.result.clone())
    }
    fn error_message(&mut self, e: &Error) -> Result<Self::Message, Error> {
        assert!(
            Rc::ptr_eq(e, self.first_error.as_ref().unwrap()),
            "PLACE native original error identity"
        );
        let message = if truth(&e["message"]) {
            e["message"].clone()
        } else {
            let msg = &e["response"]["data"]["msg"];
            if truth(msg) { msg.clone() } else { json!("") }
        };
        Ok(Rc::new(message))
    }
    fn includes(&mut self, message: &Self::Message, needle: &'static str) -> Result<bool, Error> {
        Ok(contains(&units(message).unwrap(), needle))
    }
    fn lowercase(&mut self, message: &Self::Message) -> Result<Self::Message, Error> {
        let text = units(message)
            .unwrap()
            .into_iter()
            .map(|c| if (65..=90).contains(&c) { c + 32 } else { c })
            .collect();
        Ok(Rc::new(string(text)))
    }
    fn message_string(&mut self, message: &Self::Message) -> Result<Vec<u16>, Error> {
        Ok(source_string(message))
    }
    fn warn(&mut self, message: Vec<u16>) -> Result<(), Error> {
        self.note("warn", vec![json!({"units":message})])
    }
    fn symbol(&mut self, opts: &Self::Options) -> Result<Vec<u16>, Error> {
        assert!(
            Rc::ptr_eq(opts, &self.options),
            "PLACE native symbol option identity"
        );
        Ok(units(&opts.borrow()["symbol"]).unwrap())
    }
    fn delete_position_id(&mut self, key: Vec<u16>) -> Result<(), Error> {
        self.note("delete", vec![json!({"units":key})])
    }
    async fn fetch_positions(&mut self) -> Result<Self::Positions, Error> {
        self.note("positions", vec![])?;
        Ok(self.positions.clone())
    }
    fn update_positions(&mut self, positions: Self::Positions) -> Result<(), Error> {
        assert!(
            Rc::ptr_eq(&positions, &self.positions),
            "PLACE native position identity"
        );
        self.note("updatePositions", vec![])?;
        if let Some(symbol) = self.row["fixture"]["mutateSymbol"].as_str() {
            self.options.borrow_mut()["symbol"] = json!(symbol);
        }
        Ok(())
    }
}
#[test]
fn actual_typescript_place_order() {
    let rows = oracle("placeOrder", 16);
    for row in rows {
        let symbol = row["fixture"]
            .get("symbolUnits")
            .map_or_else(|| json!("BTCUSDT"), |u| json!({"$units":u}));
        let mut h = PlaceHost {
            row: row.clone(),
            events: vec![],
            options: Rc::new(RefCell::new(json!({"symbol":symbol}))),
            payloads: Rc::new(json!({"tag":"payloads"})),
            positions: Rc::new(json!({"tag":"positions"})),
            result: Rc::new(json!({"tag":"result"})),
            attempts: 0,
            formats: 0,
            first_error: None,
        };
        let options = h.options.clone();
        let result = ready(place_order(&mut h, options));
        let observed = match result {
            Ok(v) => {
                assert!(Rc::ptr_eq(&v, &h.result), "PLACE native result identity");
                json!({"ok":true})
            }
            Err(e) => {
                if let Some(first) = &h.first_error {
                    if row["fixture"]["firstError"] == "nothing to reduce"
                        || row["fixture"]["firstError"] == "other"
                        || row["fixture"]["firstError"] == "MiSSing positionId"
                    {
                        assert!(Rc::ptr_eq(&e, first), "PLACE native rethrow identity");
                    }
                }
                json!({"error":error_obs(&e)})
            }
        };
        assert_eq!(observed, row["result"], "PLACE native result");
        assert_eq!(
            h.events.len(),
            row["events"].as_array().unwrap().len(),
            "PLACE native effect count"
        );
    }
}
#[derive(Clone)]
struct Opaque {
    value: Rc<Value>,
    direct: bool,
}
struct Iter {
    value: Opaque,
    index: usize,
}
struct BatchHost {
    row: Value,
    events: Vec<Value>,
    input: Vec<Opaque>,
    calls: usize,
    serial: usize,
    pending: usize,
}
impl BatchHost {
    fn wrap(value: Value, direct: bool) -> Opaque {
        Opaque {
            value: Rc::new(value),
            direct,
        }
    }
    fn note(&mut self, name: &str, args: Vec<Value>) -> Result<(), Error> {
        let mut event = vec![json!(name)];
        event.extend(args);
        let event = json!(event);
        let i = self.events.len();
        assert_eq!(
            event, self.row["events"][i],
            "BATCH native effect {name}:{i}"
        );
        self.events.push(event);
        if self.row["fixture"]["fail"] == name {
            Err(failed(name))
        } else {
            Ok(())
        }
    }
    fn body(&self, index: usize, endpoint: &str) -> Value {
        self.row["fixture"]["bodies"].get(index).cloned().unwrap_or_else(||json!({"code":0,"data":if endpoint.ends_with("batch_order"){json!({"successList":[{"orderId":format!("b{index}")}]})}else{json!({"orderId":format!("s{index}")})}}))
    }
}
impl BatchOrderHost for BatchHost {
    type Value = Opaque;
    type Iterator = Iter;
    type Error = Error;
    fn input_length(&mut self) -> Result<usize, Error> {
        Ok(self.input.len())
    }
    fn input_at(&mut self, index: usize) -> Result<Option<Opaque>, Error> {
        Ok(self.input.get(index).cloned())
    }
    fn field(&mut self, v: &Opaque, path: &[&str]) -> Result<Opaque, Error> {
        let mut value = &*v.value;
        for key in path {
            if (value.is_null() || value.get("$undefined") == Some(&json!(true))) && v.direct {
                return Err(Rc::new(
                    json!({"name":"TypeError","message":format!("Cannot read properties of null (reading '{key}')")}),
                ));
            }
            match value.get(*key) {
                Some(next) => value = next,
                None => return Ok(Self::wrap(undefined(), false)),
            }
        }
        Ok(Self::wrap(value.clone(), false))
    }
    fn truthy(&self, v: &Opaque) -> bool {
        truth(&v.value)
    }
    fn zero(&self, v: &Opaque) -> bool {
        number(&v.value).is_some_and(|n| n == 0.)
    }
    fn limit(&self, v: &Opaque) -> bool {
        units(&v.value).is_some_and(|u| u == "LIMIT".encode_utf16().collect::<Vec<_>>())
    }
    fn has_own(&mut self, v: &Opaque, key: &'static str) -> Result<bool, Error> {
        Ok(v.value.as_object().is_some_and(|o| o.contains_key(key)))
    }
    fn identical(&self, a: &Opaque, b: &Opaque) -> bool {
        Rc::ptr_eq(&a.value, &b.value)
    }
    fn symbol_key(&mut self, v: &Opaque) -> Result<Option<Vec<u16>>, Error> {
        Ok(Some(v.value.get("symbol").map_or_else(
            || "undefined".encode_utf16().collect(),
            source_string,
        )))
    }
    fn omit(&mut self, v: &Opaque, key: &'static str) -> Result<Opaque, Error> {
        let mut value = (*v.value).clone();
        value.as_object_mut().unwrap().shift_remove(key);
        Ok(Self::wrap(value, true))
    }
    fn batch_payload(&mut self, symbol: Vec<u16>, rows: Vec<Opaque>) -> Result<Opaque, Error> {
        Ok(Self::wrap(
            json!({"symbol":string(symbol),"orderList":rows.into_iter().map(|r|(*r.value).clone()).collect::<Vec<_>>()}),
            true,
        ))
    }
    async fn post(&mut self, endpoint: &'static str, payload: Opaque) -> Result<Opaque, Error> {
        self.note("post", vec![text(endpoint), obs(&payload.value)])?;
        if let Some(message) = self.row["fixture"]["postError"]["message"].as_str() {
            return Err(error_message(message.encode_utf16().collect()));
        }
        let index = self.calls;
        self.calls += 1;
        Ok(Self::wrap(self.body(index, endpoint), false))
    }
    async fn sleep(&mut self, delay: f64) -> Result<(), Error> {
        self.note("sleep", vec![bits(delay)])?;
        if self.row["fixture"]["pendingSleep"] == true {
            let mut yielded = false;
            poll_fn(|cx| {
                if yielded {
                    Poll::Ready(())
                } else {
                    yielded = true;
                    self.pending += 1;
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            })
            .await;
        }
        Ok(())
    }
    fn uuid(&mut self) -> Result<Opaque, Error> {
        self.note("uuid", vec![])?;
        let value = json!(format!("u{}", self.serial));
        self.serial += 1;
        Ok(Self::wrap(value, false))
    }
    fn string(&mut self, text: Vec<u16>) -> Result<Opaque, Error> {
        Ok(Self::wrap(string(text), false))
    }
    fn to_string(&mut self, v: &Opaque) -> Result<Vec<u16>, Error> {
        Ok(source_string(&v.value))
    }
    fn error(&mut self, message: Opaque) -> Result<Error, Error> {
        self.note("error", vec![obs(&message.value)])?;
        Ok(error_message(source_string(&message.value)))
    }
    fn report(&mut self, e: &Error) -> Result<(), Error> {
        self.note("emit", vec![obs(&e["message"])])
    }
    fn emit(&mut self, message: Opaque) -> Result<(), Error> {
        self.note("emit", vec![obs(&message.value)])
    }
    fn is_array(&self, v: &Opaque) -> bool {
        v.value.is_array()
    }
    fn array_length(&mut self, v: &Opaque) -> Result<usize, Error> {
        Ok(v.value.as_array().unwrap().len())
    }
    fn array_at(&mut self, v: &Opaque, index: usize) -> Result<Option<Opaque>, Error> {
        Ok(v.value.get(index).map(|v| Self::wrap(v.clone(), true)))
    }
    fn array(&mut self, values: Vec<Opaque>) -> Result<Opaque, Error> {
        Ok(Self::wrap(
            json!(
                values
                    .iter()
                    .map(|v| (*v.value).clone())
                    .collect::<Vec<_>>()
            ),
            false,
        ))
    }
    fn iterator(&mut self, value: Opaque) -> Result<Iter, Error> {
        Ok(Iter { value, index: 0 })
    }
    fn next(&mut self, iter: &mut Iter) -> Result<Option<Opaque>, Error> {
        let v = iter.value.value.get(iter.index).cloned();
        iter.index += 1;
        Ok(v.map(|v| Self::wrap(v, true)))
    }
    fn nullish(&self, v: &Opaque) -> bool {
        v.value.is_null() || v.value.get("$undefined") == Some(&json!(true))
    }
}
#[test]
fn actual_typescript_batch_order() {
    let rows = oracle("placeOrderBatch", 30);
    for row in rows {
        let payloads=row["fixture"].get("payloads").cloned().unwrap_or_else(||json!([{"symbol":"BTCUSDT","orderType":"LIMIT","clientId":"c0","qty":"1","price":"10"},{"symbol":"BTCUSDT","orderType":"LIMIT","clientId":"c1","qty":"1","price":"10"}]));
        let mut input = payloads
            .as_array()
            .unwrap()
            .iter()
            .map(|v| BatchHost::wrap(v.clone(), true))
            .collect::<Vec<_>>();
        if row["fixture"]["duplicateInput"] == true {
            input[1] = input[0].clone();
        }
        let mut h = BatchHost {
            row: row.clone(),
            events: vec![],
            input,
            calls: 0,
            serial: 0,
            pending: 0,
        };
        let result = ready(place_order_batch(&mut h));
        let result = match result {
            Ok(values) => json!({"ok":values.iter().map(|v|obs(&v.value)).collect::<Vec<_>>()}),
            Err(e) => json!({"error":error_obs(&e)}),
        };
        assert_eq!(
            result, row["result"],
            "BATCH native result: {}",
            row["fixture"]
        );
        assert_eq!(
            h.events.len(),
            row["events"].as_array().unwrap().len(),
            "BATCH native effect count"
        );
        if row["fixture"]["pendingSleep"] == true {
            assert_eq!(h.pending, 2, "BATCH native suspension floor");
        }
    }
}
fn oracle(name: &str, floor: usize) -> Vec<Value> {
    let output = Command::new("node")
        .arg(format!(
            "{}/tests/order-operations-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .arg(name)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "ORDER actual source {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = corpus["rows"].as_array().unwrap().clone();
    assert_eq!(rows.len(), floor, "ORDER independent family floor {name}");
    rows
}
