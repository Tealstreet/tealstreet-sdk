use safe_cex_num::{JsCoercionError, js_truthy};
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use safe_cex_types::{Order, OrderStopTriggerType};
use safe_cex_venue_bitunix::{
    order_links,
    order_models::*,
    order_state,
    tpsl_helpers::{self, TpslLeg},
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    future::Future,
    pin::pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
#[derive(Clone)]
struct Error {
    name: String,
    message: String,
    response: Option<Value>,
    token: Rc<()>,
}
impl From<JsCoercionError> for Error {
    fn from(e: JsCoercionError) -> Self {
        Self {
            name: e.name.into(),
            message: e.message.into(),
            response: None,
            token: Rc::new(()),
        }
    }
}
fn failure(message: impl Into<String>) -> Error {
    Error {
        name: "Error".into(),
        message: message.into(),
        response: None,
        token: Rc::new(()),
    }
}
fn bits(v: f64) -> Value {
    json!({"bits":format!("{:016x}",if v.is_nan(){0x7ff8_0000_0000_0000}else{v.to_bits()})})
}
fn text(v: &str) -> Value {
    json!({"units":v.encode_utf16().collect::<Vec<_>>()})
}
fn undefined() -> Value {
    json!({"undefined":true})
}
fn retained_text(value: impl AsRef<str>) -> Input {
    Input::Text(value.as_ref().encode_utf16().collect())
}
fn obs(v: &Input) -> Value {
    match v {
        Input::Undefined => undefined(),
        Input::Null => Value::Null,
        Input::Bool(v) => json!(v),
        Input::Float(v) => bits(*v),
        Input::Lexeme(v) => bits(v.parse().unwrap()),
        Input::Text(v) => json!({"units":v}),
        Input::Array(v) => json!(v.iter().map(obs).collect::<Vec<_>>()),
        Input::Object(v) => {
            json!({"entries":v.iter().map(|(k,v)|json!([String::from_utf16(k).unwrap(),obs(v)])).collect::<Vec<_>>()})
        }
    }
}
fn value_obs(v: &Input) -> Value {
    obs(v)
}
fn error_obs(e: &Error) -> Value {
    json!({"error":[text(&e.name),text(&e.message)]})
}
fn ready<T>(f: impl Future<Output = T>) -> T {
    let mut f = pin!(f);
    let mut cx = Context::from_waker(Waker::noop());
    match f.as_mut().poll(&mut cx) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("MODEL unexpected suspended projection"),
    }
}
fn key_value(row: &Input, key: &str) -> Option<Input> {
    row.get(key)
        .or_else(|| row.get(&safe_cex_num::lodash_snake_case(key)))
        .or_else(|| row.get(&safe_cex_num::lodash_camel_case(key)))
        .cloned()
}
fn canonical(v: &Input) -> Order {
    use safe_cex_exchange::js_runtime::value::{from_json, from_retained_input, to_typed};
    let value = from_json(
        &json!({"id":"","status":"open","symbol":"BTCUSDT","type":"conditional","side":"buy","price":0,"amount":0,"filled":0,"remaining":0,"reduceOnly":false,"lastUpdate":0}),
    );
    for (key, field) in from_retained_input(v.clone()).own_entries().unwrap() {
        value.set_key(&key, field).unwrap();
    }
    if ![
        "market",
        "limit",
        "conditional",
        "stop_market",
        "take_profit_market",
        "trailing_stop_market",
    ]
    .iter()
    .any(|kind| v.get("type").is_some_and(|v| v.text_equals(kind)))
    {
        value
            .define(
                "type",
                safe_cex_exchange::js_runtime::value::Js::str("conditional"),
            )
            .unwrap();
    }
    to_typed(&value).unwrap()
}
struct Host {
    row: Value,
    events: Vec<Value>,
    options: Value,
    order: Value,
    positions: Vec<Value>,
    orders: Vec<Value>,
    data: Value,
    serial: usize,
}
// Test recorder: Error is large and these two helpers only ever propagate it,
// so the lint is about this harness's return shape, not production error flow.
impl Host {
    fn note(&mut self, name: &str, args: Vec<Value>) -> Result<(), Error> {
        let mut event = vec![json!(name)];
        event.extend(args);
        let event = json!(event);
        let index = self.events.len();
        assert_eq!(
            event, self.row["events"][index],
            "MODEL native effect {}:{name}:{index}",
            self.row["name"]
        );
        self.events.push(event);
        if self.row["fixture"]["fail"] == name {
            Err(failure(format!("{name} failed")))
        } else {
            Ok(())
        }
    }
    fn spec_error(&self, key: &str) -> Option<Error> {
        self.row["fixture"].get(key).map(|spec| Error {
            name: "Error".into(),
            message: spec["message"].as_str().unwrap_or("post failed").into(),
            response: spec.get("response").cloned(),
            token: Rc::new(()),
        })
    }
    fn source_orders(&mut self) -> Result<Vec<Order>, Error> {
        self.orders()
            .map(|orders| orders.iter().map(canonical).collect())
    }
    fn reason_value(&self, e: &Error, fallback: Option<&str>) -> Input {
        if let Some(message) = e
            .response
            .as_ref()
            .and_then(|r| r.get("data"))
            .and_then(|r| r.get("msg"))
            .filter(|v| js_truthy(Some(v)))
        {
            Input::from(message.clone())
        } else if !e.message.is_empty() {
            retained_text(e.message.clone())
        } else {
            fallback.map_or(retained_text(e.message.clone()), retained_text)
        }
    }
}
impl OrderModelHost for Host {
    type Error = Error;
    fn caller_id(&mut self, opts: &Input) -> Result<Option<String>, Error> {
        assert_eq!(
            opts,
            &Input::from(self.options.clone()),
            "MODEL native option contents"
        );
        self.note("caller", vec![])?;
        Ok(self.row["fixture"]["caller"].as_str().map(str::to_owned))
    }
    fn market(&mut self, symbol: &Input) -> Result<Option<Input>, Error> {
        self.note("market", vec![value_obs(symbol)])?;
        Ok(
            if self.row["fixture"]["market"].is_null()
                && self.row["fixture"].get("market").is_some()
            {
                None
            } else {
                Some(self.row["fixture"].get("market").cloned().unwrap_or_else(||json!({"symbol":"BTCUSDT","precision":{"price":0.5,"amount":0.1},"limits":{"amount":{"max":2}}})).into())
            },
        )
    }
    fn positions(&mut self) -> Result<Vec<Input>, Error> {
        self.note("positions", vec![])?;
        Ok(self.positions.iter().cloned().map(Into::into).collect())
    }
    fn orders(&mut self) -> Result<Vec<Input>, Error> {
        self.note("orders", vec![])?;
        Ok(self.orders.iter().cloned().map(Into::into).collect())
    }
    fn cached_position(&mut self, key: &str) -> Result<Option<Input>, Error> {
        self.note("cachePosition", vec![text(key)])?;
        Ok(self.row["fixture"]["cache"]
            .get(key)
            .cloned()
            .map(Into::into))
    }
    fn uuid(&mut self) -> Result<String, Error> {
        self.note("uuid", vec![])?;
        let id = format!("u{}", self.serial);
        self.serial += 1;
        Ok(id)
    }
    fn assert_single(&mut self, caller: Option<&str>, count: usize) -> Result<(), Error> {
        self.note(
            "assertSingle",
            vec![caller.map_or_else(undefined, text), bits(count as f64)],
        )?;
        if caller.is_some() && count > 1 {
            Err(failure("one caller ID cannot split"))
        } else {
            Ok(())
        }
    }
    fn cache_tif(&mut self, id: &str, tif: &Input) -> Result<(), Error> {
        self.note("cacheTif", vec![text(id), value_obs(tif)])
    }
    fn cache_reduce_only(&mut self, id: &str, reduce: &Input) -> Result<(), Error> {
        self.note("cacheReduce", vec![text(id), value_obs(reduce)])
    }
    fn emit(&mut self, message: Input) -> Result<(), Error> {
        self.note("emit", vec![obs(&message)])
    }
    fn error(&mut self, message: Input) -> Result<Error, Error> {
        self.note("error", vec![obs(&message)])?;
        let message = match message {
            Input::Text(s) => String::from_utf16(&s).unwrap(),
            Input::Float(n) => safe_cex_num::js_number_string(n),
            Input::Bool(b) => b.to_string(),
            Input::Null => "null".into(),
            Input::Undefined => String::new(),
            _ => panic!("unexpected factory input"),
        };
        Ok(failure(message))
    }
    fn reason(&mut self, error: &Error, fallback: Option<&str>) -> Result<Input, Error> {
        assert_eq!(
            Rc::strong_count(&error.token),
            1,
            "MODEL native error factory ownership"
        );
        Ok(self.reason_value(error, fallback))
    }
    fn warning(&mut self, prefix: &'static str, e: &Error) -> Result<(), Error> {
        let reason = self.reason_value(e, None);
        let reason = match reason {
            Input::Text(s) => String::from_utf16(&s).unwrap(),
            _ => panic!("unexpected warning reason"),
        };
        self.note(
            "warning",
            vec![text(&format!("{prefix}{reason}")), text("warning")],
        )
    }
    async fn get(&mut self, endpoint: &'static str, config: Input) -> Result<Input, Error> {
        self.note("get", vec![text(endpoint), obs(&config)])?;
        if let Some(error) = self.spec_error("getError") {
            return Err(error);
        }
        Ok(json!({"data":{"data":self.data}}).into())
    }
    async fn post(&mut self, endpoint: &'static str, payload: Input) -> Result<Input, Error> {
        self.note("post", vec![text(endpoint), obs(&payload)])?;
        if let Some(error) = self.spec_error("postError") {
            return Err(error);
        }
        Ok(self.row["fixture"]
            .get("reply")
            .cloned()
            .unwrap_or_else(|| json!({"data":{"code":0,"data":{"orderId":"ACK"}}}))
            .into())
    }
    async fn post_limited(
        &mut self,
        endpoint: &'static str,
        payload: Input,
    ) -> Result<Input, Error> {
        self.note("postLimited", vec![text(endpoint), obs(&payload)])?;
        Ok(json!({"data":{"code":0}}).into())
    }
    fn update_status(&mut self, ids: &[String]) -> Result<(), Error> {
        self.note("updateStatus", vec![json!(ids)])?;
        for order in &mut self.orders {
            if order
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| ids.iter().any(|candidate| candidate == id))
            {
                order["status"] = json!("canceled");
            }
        }
        Ok(())
    }
    async fn place_replacement(&mut self, options: Input) -> Result<Vec<String>, Error> {
        self.note("placeReplacement", vec![value_obs(&options)])?;
        Ok(vec!["NEW".into()])
    }
    fn cached_tif(&mut self, client: Option<&Input>) -> Result<Option<Input>, Error> {
        self.note("cachedTif", vec![client.map_or_else(undefined, value_obs)])?;
        Ok(client
            .and_then(Input::as_text)
            .and_then(|id| String::from_utf16(id).ok())
            .and_then(|id| self.row["fixture"]["tifCache"].get(&id))
            .cloned()
            .map(Into::into))
    }
    fn reduce_only(&mut self, raw: Option<&Input>, client: Option<&Input>) -> Result<bool, Error> {
        let cache = self.row["fixture"]["reduceCache"]
            .as_object()
            .map(|map| {
                map.iter()
                    .map(|(k, v)| (k.clone(), v.as_bool().unwrap()))
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        Ok(tpsl_helpers::get_bitunix_reduce_only(
            raw, client, false, &cache,
        )?)
    }
    fn key(&mut self, row: &Input, key: &'static str) -> Result<Option<Input>, Error> {
        self.note("key", vec![text(key)])?;
        Ok(key_value(row, key))
    }
    fn raw_id(&mut self, order: &Input) -> Result<String, Error> {
        Ok(order_links::get_tpsl_raw_order_id(&canonical(order))?)
    }
    fn leg_id(
        &mut self,
        raw: &str,
        position: Option<&Input>,
        leg: &'static str,
        disambiguate: bool,
    ) -> Result<String, Error> {
        Ok(tpsl_helpers::get_tpsl_leg_order_id(
            Some(&retained_text(raw)),
            position,
            if leg == "tp" {
                TpslLeg::Tp
            } else {
                TpslLeg::Sl
            },
            disambiguate,
        )?)
    }
    fn clock(&mut self) -> Result<f64, Error> {
        self.note("clock", vec![])?;
        Ok(1700000000000.)
    }
    fn fake_full_enabled(&mut self) -> Result<bool, Error> {
        self.note("fake", vec![])?;
        Ok(self.row["fixture"]["fake"] == true)
    }
    fn attached_leg(&mut self, order: &Input) -> Result<Option<&'static str>, Error> {
        if safe_cex_venue_bitunix::order_ids::get_virtual_tpsl_leg(
            safe_cex_num::js_retained_string(order.get("id"))
                .unwrap()
                .as_str(),
        )
        .is_none()
        {
            return Ok(None);
        }
        let orders = self.source_orders()?;
        Ok(order_links::get_attached_virtual_tpsl_leg(
            &canonical(order),
            &orders,
        ))
    }
    fn standalone(&mut self, order: &Input) -> Result<bool, Error> {
        if !order.get("type").is_some_and(|value| {
            value.text_equals("stop_market") || value.text_equals("take_profit_market")
        }) {
            return Ok(false);
        }
        Ok(self.attached_leg(order)?.is_none())
    }
    fn position_order(&mut self, order: &Input) -> Result<bool, Error> {
        Ok(order_links::is_position_tpsl_order(&canonical(order))?)
    }
    fn snapshot(
        &mut self,
        order: &Input,
        leg: &'static str,
    ) -> Result<Option<order_state::TpslLegSnapshot>, Error> {
        let leg_type = if leg == "tp" {
            "take_profit_market"
        } else {
            "stop_market"
        };
        let actual = if order
            .get("type")
            .is_some_and(|value| value.text_equals(leg_type))
            && self.standalone(order)?
        {
            vec![]
        } else {
            self.source_orders()?
        };
        // Existing snapshot helpers may read store orders while classifying virtual children.
        Ok(order_state::get_tpsl_leg_snapshot(
            &canonical(order),
            if leg == "tp" {
                TpslLeg::Tp
            } else {
                TpslLeg::Sl
            },
            &actual,
        )?)
    }
    fn trigger(&mut self, trigger: Option<&Input>, raw: Option<&Input>) -> Result<String, Error> {
        let trigger = trigger.and_then(|v| {
            safe_cex_exchange::js_runtime::value::to_typed::<OrderStopTriggerType>(
                &safe_cex_exchange::js_runtime::value::from_retained_input(v.clone()),
            )
            .ok()
        });
        Ok(tpsl_helpers::get_bitunix_trigger_type(trigger, raw)?.into())
    }
    async fn update_standalone(
        &mut self,
        order: &Input,
        update: &Input,
    ) -> Result<Vec<String>, Error> {
        update_standalone_tpsl_order(self, order, update).await
    }
}
fn run(name: &str, floor: usize) {
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
        "MODEL actual source {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = corpus["rows"].as_array().unwrap();
    assert_eq!(rows.len(), floor, "MODEL independent source floor {name}");
    let mut native = 0;
    let mut refusals = 0;
    for row in rows {
        if row["fixture"]["refusal"] == true {
            let raw = if name.starts_with("fetch") {
                row["dataJson"].as_str().unwrap()
            } else if name.starts_with("update") {
                row["orderJson"].as_str().unwrap()
            } else {
                row["optionsJson"].as_str().unwrap()
            };
            assert!(
                serde_json::from_str::<Value>(raw).is_err(),
                "MODEL measured UTF-8 refusal {name}"
            );
            assert!(
                row["result"].get("ok").is_some(),
                "MODEL observed source success before refusal {name}"
            );
            refusals += 1;
            continue;
        }
        native += 1;
        let mut host = Host {
            row: row.clone(),
            events: vec![],
            options: serde_json::from_str(row["optionsJson"].as_str().unwrap()).unwrap(),
            order: serde_json::from_str(row["orderJson"].as_str().unwrap()).unwrap(),
            positions: serde_json::from_str(row["positionsJson"].as_str().unwrap()).unwrap(),
            orders: serde_json::from_str(row["ordersJson"].as_str().unwrap()).unwrap(),
            data: serde_json::from_str(row["dataJson"].as_str().unwrap()).unwrap(),
            serial: 0,
        };
        let options = host.options.clone().into();
        let order = host.order.clone().into();
        let update = row["fixture"]
            .get("update")
            .cloned()
            .unwrap_or_else(|| json!({"price":110.25}))
            .into();
        let result = match name {
            "formatCreateOrder" => format_create_order(&mut host, &options),
            "formatTPSLOrder" => format_tpsl_order(&mut host, &options),
            "fetchRegularOrders" => ready(fetch_regular_orders(&mut host)),
            "fetchTpslOrders" => ready(fetch_tpsl_orders(&mut host)),
            "updateOrder" => ready(update_order(&mut host, &order, &update))
                .map(|ids| ids.into_iter().map(retained_text).collect()),
            "updateStandaloneTpslOrder" => {
                ready(update_standalone_tpsl_order(&mut host, &order, &update))
                    .map(|ids| ids.into_iter().map(retained_text).collect())
            }
            _ => panic!("unknown model family"),
        };
        let result = match result {
            Ok(values) => json!({"ok":values.iter().map(obs).collect::<Vec<_>>()}),
            Err(e) => json!({"error":error_obs(&e)}),
        };
        assert_eq!(
            result, row["result"],
            "MODEL native result {name}: {}",
            row["fixture"]
        );
        assert_eq!(
            host.events.len(),
            row["events"].as_array().unwrap().len(),
            "MODEL native effect count {name}"
        );
    }
    assert_eq!(refusals, 1, "MODEL independent refusal floor {name}");
    assert_eq!(native, floor - 1, "MODEL independent native floor {name}");
}
#[test]
fn actual_typescript_create_order() {
    run("formatCreateOrder", 22);
}
#[test]
fn actual_typescript_tpsl_format() {
    run("formatTPSLOrder", 19);
}
#[test]
fn actual_typescript_regular_reads() {
    run("fetchRegularOrders", 17);
}
#[test]
fn actual_typescript_tpsl_reads() {
    run("fetchTpslOrders", 15);
}
#[test]
fn actual_typescript_standalone_amend() {
    run("updateStandaloneTpslOrder", 17);
}
#[test]
fn actual_typescript_regular_amend() {
    run("updateOrder", 26);
}
