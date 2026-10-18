use safe_cex_num::{JsCoercionError, JsJsonValue as J};
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use safe_cex_venue_bitunix::{
    composition::*, event_models::*, order_cancellation::*, orderbook_subscription::*,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::HashMap,
    future::Future,
    pin::{Pin, pin},
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn undefined() -> Value {
    json!({"undefined":true})
}
fn bits(v: f64) -> Value {
    json!({"bits":format!("{:016x}",if v.is_nan(){0x7ff8000000000000u64}else{v.to_bits()})})
}
fn units(v: &[u16]) -> Value {
    json!({"units":v})
}
fn text(v: &str) -> Value {
    units(&v.encode_utf16().collect::<Vec<_>>())
}
fn obs(v: &J) -> Value {
    match v {
        J::Undefined => undefined(),
        J::Null => Value::Null,
        J::Bool(v) => json!(v),
        J::Number(v) => bits(*v),
        J::String(v) => text(v),
        J::Array(v) => json!(v.iter().map(obs).collect::<Vec<_>>()),
        J::Object(v) => Value::Object(v.iter().map(|(k, v)| (k.clone(), obs(v))).collect()),
        J::BigInt(_) => panic!("Unexpected BigInt"),
    }
}
fn retained_obs(v: &Input) -> Value {
    match v {
        Input::Undefined => undefined(),
        Input::Null => Value::Null,
        Input::Bool(v) => json!(v),
        Input::Float(v) => bits(*v),
        Input::Lexeme(v) => bits(v.parse().unwrap()),
        Input::Text(v) => units(v),
        Input::Array(values) => json!(values.iter().map(retained_obs).collect::<Vec<_>>()),
        Input::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| (String::from_utf16(key).unwrap(), retained_obs(value)))
                .collect(),
        ),
    }
}
fn vo(v: &Value) -> Value {
    obs(&J::from(v.clone()))
}
fn jo(fields: Vec<(&str, J)>) -> J {
    J::Object(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn ready<T>(f: impl Future<Output = T>) -> T {
    let mut f = pin!(f);
    let mut cx = Context::from_waker(Waker::noop());
    match f.as_mut().poll(&mut cx) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("Unexpected pending"),
    }
}
fn oracle(name: &str) -> Vec<Value> {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/event-composition-oracle.cjs"
        ))
        .arg(name)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
#[derive(Debug, Clone)]
struct Error {
    message: String,
    token: Rc<()>,
}
impl From<JsCoercionError> for Error {
    fn from(e: JsCoercionError) -> Self {
        Self {
            message: e.message.into(),
            token: Rc::new(()),
        }
    }
}
fn error() -> Error {
    Error {
        message: "transport failed".into(),
        token: Rc::new(()),
    }
}
struct Host {
    row: Value,
    events: Rc<RefCell<Vec<Value>>>,
    pending: Option<Pin<Box<dyn Future<Output = Result<(), Error>>>>>,
    orders: Vec<Input>,
    positions: Vec<Input>,
    count: f64,
    handled: bool,
    last: f64,
    failure: Error,
}
impl Host {
    fn new(row: &Value) -> Self {
        Self {
            row: row.clone(),
            events: Rc::new(RefCell::new(Vec::new())),
            pending: None,
            orders: row
                .get("orders")
                .and_then(Value::as_array)
                .map(|values| values.iter().cloned().map(Into::into).collect())
                .unwrap_or_default(),
            positions: row
                .get("positions")
                .and_then(Value::as_array)
                .map(|values| values.iter().cloned().map(Into::into).collect())
                .unwrap_or_default(),
            count: 0.0,
            handled: row["fixture"]["handled"] == true,
            last: 0.0,
            failure: error(),
        }
    }
    fn note(&mut self, name: &str, args: Vec<Value>) {
        let mut event = vec![json!(name)];
        event.extend(args);
        let event = json!(event);
        assert_eq!(
            event,
            self.row["events"][self.events.borrow().len()],
            "EVENT native effect {name}; fixture {}",
            self.row["fixture"]
        );
        self.events.borrow_mut().push(event);
    }
    fn end(&self) {
        assert_eq!(
            json!(*self.events.borrow()),
            self.row["events"],
            "EVENT native complete effects; fixture {}",
            self.row["fixture"]
        );
    }
}
impl FillHistoryHost for Host {
    type Error = Error;
    fn public_only(&mut self) -> Result<bool, Error> {
        Ok(self.row["fixture"]["public"] == true)
    }
    fn market(&mut self, _: &Input) -> Result<Option<Input>, Error> {
        Ok(self
            .row
            .get("market")
            .filter(|v| !v.is_null())
            .cloned()
            .map(Into::into))
    }
    async fn get(&mut self, endpoint: &'static str, config: Input) -> Result<Input, Error> {
        self.note("get", vec![text(endpoint), retained_obs(&config)]);
        if self.row["fixture"]["error"] == true {
            return Err(self.failure.clone());
        }
        Ok(self.row["payload"].clone().into())
    }
    fn local_to_utc(&mut self, value: f64) -> f64 {
        value
    }
    fn warning(&mut self, prefix: &'static str, error: &Error) -> Result<(), Error> {
        self.note(
            "warning",
            vec![text(&format!("{prefix}{}", error.message)), text("warning")],
        );
        Ok(())
    }
}
#[test]
fn actual_typescript_fill_history() {
    let rows = oracle("fills");
    assert_eq!(rows.len(), 24);
    let mut compared = 0;
    let mut refusals = 0;
    for row in rows {
        if row["fixture"]["refusal"] == true {
            assert!(
                serde_json::from_str::<Value>(row["rawJson"].as_str().unwrap()).is_err(),
                "HISTORY native carrier refusal"
            );
            assert_eq!(row["acceptedBySource"], true);
            refusals += 1;
            continue;
        }
        let mut host = Host::new(&row);
        let output = ready(fetch_historical_fills(
            &mut host,
            &row["options"].clone().into(),
        ))
        .unwrap();
        assert_eq!(
            retained_obs(&Input::Array(output)),
            row["result"],
            "HISTORY native result; fixture {}",
            row["fixture"]
        );
        host.end();
        compared += 1;
    }
    assert_eq!((compared, refusals), (23, 1));
}
fn key(row: &Input, name: &str) -> Option<Input> {
    row.get(name)
        .or_else(|| row.get(&safe_cex_num::lodash_snake_case(name)))
        .cloned()
}
impl PrivateMessageHost for Host {
    type Raw = String;
    type Error = Error;
    fn disposed(&mut self) -> Result<bool, Error> {
        Ok(self.row["fixture"]["disposed"] == true)
    }
    fn track(&mut self) -> Result<(), Error> {
        self.note("track", vec![]);
        Ok(())
    }
    fn count(&mut self) -> Result<f64, Error> {
        Ok(self.count)
    }
    fn set_count(&mut self, value: f64) -> Result<(), Error> {
        self.count = value;
        Ok(())
    }
    fn diagnostic(&mut self, raw: &String) -> Result<(), Error> {
        self.note("diagnostic", vec![text(raw)]);
        Ok(())
    }
    fn parse(&mut self, raw: String) -> Result<Input, Error> {
        Ok(Input::parse_json(&raw.encode_utf16().collect::<Vec<_>>()).unwrap_or(Input::Null))
    }
    fn balance_notice(&mut self, json: &Input) -> Result<(), Error> {
        self.note("balanceNotice", vec![retained_obs(json)]);
        Ok(())
    }
    fn refresh(&mut self, kind: &'static str) -> Result<(), Error> {
        self.note("refresh", vec![text(kind)]);
        Ok(())
    }
    fn orders(&mut self) -> Result<Vec<Input>, Error> {
        Ok(self.orders.clone())
    }
    fn positions(&mut self) -> Result<Vec<Input>, Error> {
        Ok(self.positions.clone())
    }
    fn add_orders(&mut self, orders: Vec<Input>) -> Result<(), Error> {
        self.note("add", vec![retained_obs(&Input::Array(orders))]);
        Ok(())
    }
    fn key(&mut self, row: &Input, name: &'static str) -> Result<Option<Input>, Error> {
        Ok(key(row, name))
    }
    fn reduce_only(
        &mut self,
        raw: Option<&Input>,
        client: Option<&Input>,
        fallback: Option<&Input>,
    ) -> Result<bool, Error> {
        let cache = HashMap::from([("c".into(), true)]);
        let result = safe_cex_venue_bitunix::tpsl_helpers::get_bitunix_reduce_only(
            raw,
            client,
            fallback.and_then(Input::as_bool).unwrap_or(false),
            &cache,
        )?;
        Ok(result)
    }
    fn cache_tif(&mut self, client: &Input, tif: Input) -> Result<(), Error> {
        self.note("cacheTif", vec![retained_obs(client), retained_obs(&tif)]);
        Ok(())
    }
    fn cache_position(&mut self, key: String, id: Option<&Input>) -> Result<(), Error> {
        self.note(
            "cachePosition",
            vec![text(&key), id.map_or_else(undefined, retained_obs)],
        );
        Ok(())
    }
    fn update_position_info(&mut self, position: &Input, info: Input) -> Result<(), Error> {
        let found = self
            .positions
            .iter_mut()
            .find(|p| **p == *position)
            .unwrap();
        if let Some(value) = found.get_mut("info") {
            *value = info;
        } else {
            found
                .as_object_mut()
                .unwrap()
                .push(("info".encode_utf16().collect(), info));
        }
        Ok(())
    }
    fn fake_full(&mut self) -> Result<bool, Error> {
        Ok(self.row["fixture"]["fake"] == true)
    }
    fn leg_id(
        &mut self,
        raw: Option<&Input>,
        position: Option<&Input>,
        leg: &'static str,
        disambiguate: bool,
    ) -> Result<String, Error> {
        Ok(safe_cex_venue_bitunix::tpsl_helpers::get_tpsl_leg_order_id(
            raw,
            position,
            if leg == "tp" {
                safe_cex_venue_bitunix::tpsl_helpers::TpslLeg::Tp
            } else {
                safe_cex_venue_bitunix::tpsl_helpers::TpslLeg::Sl
            },
            disambiguate,
        )?)
    }
    fn clock(&mut self) -> Result<f64, Error> {
        Ok(self.row["fixture"]["clock"]
            .as_f64()
            .unwrap_or(1700000000000.0))
    }
    fn local_to_utc(&mut self, value: f64) -> f64 {
        value
    }
    fn handled_pong(&mut self) -> Result<bool, Error> {
        Ok(self.handled)
    }
    fn last_pong(&mut self) -> Result<f64, Error> {
        Ok(self.last)
    }
    fn set_last_pong(&mut self, value: f64) -> Result<(), Error> {
        self.last = value;
        Ok(())
    }
    fn set_handled_pong(&mut self, value: bool) -> Result<(), Error> {
        self.handled = value;
        Ok(())
    }
    fn pong(&mut self) -> Result<(), Error> {
        self.note("pong", vec![]);
        Ok(())
    }
    fn login_diagnostic(&mut self, message: String) -> Result<(), Error> {
        self.note("diagnostic", vec![text(&message)]);
        Ok(())
    }
    fn login_error(&mut self, message: &'static str) -> Result<(), Error> {
        self.note("loginError", vec![text(message)]);
        Ok(())
    }
    fn spawn_login_retry(&mut self) -> Result<(), Error> {
        let mut task = RetryTask {
            row: self.row.clone(),
            events: self.events.clone(),
        };
        let mut future: Pin<Box<dyn Future<Output = Result<(), Error>>>> =
            Box::pin(async move { retry_login(&mut task).await });
        let mut cx = Context::from_waker(Waker::noop());
        assert!(
            future.as_mut().poll(&mut cx).is_pending(),
            "PRIVATE native detached retry suspended"
        );
        self.pending = Some(future);
        Ok(())
    }
}
struct RetryTask {
    row: Value,
    events: Rc<RefCell<Vec<Value>>>,
}
impl RetryTask {
    fn note(&mut self, name: &str, args: Vec<Value>) {
        let mut event = vec![json!(name)];
        event.extend(args);
        let event = json!(event);
        assert_eq!(
            event,
            self.row["events"][self.events.borrow().len()],
            "EVENT native effect {name}"
        );
        self.events.borrow_mut().push(event);
    }
}
impl LoginRetryHost for RetryTask {
    type Error = Error;
    async fn sync(&mut self) -> Result<(), Error> {
        self.note("sync", vec![]);
        let mut pending = true;
        std::future::poll_fn(|_| {
            if pending {
                pending = false;
                Poll::Pending
            } else {
                Poll::Ready(())
            }
        })
        .await;
        Ok(())
    }
    async fn sleep(&mut self, ms: f64) -> Result<(), Error> {
        self.note("sleep", vec![bits(ms)]);
        Ok(())
    }
    fn launch_login(&mut self) -> Result<(), Error> {
        self.note("login", vec![]);
        Ok(())
    }
}

#[test]
fn actual_typescript_private_message() {
    let rows = oracle("private");
    assert_eq!(rows.len(), 29);
    let mut refusals = 0;
    for row in rows {
        if row["fixture"]["refusal"] == true {
            assert!(
                serde_json::from_str::<Value>(row["rawJson"].as_str().unwrap()).is_err(),
                "PRIVATE native carrier refusal"
            );
            assert_eq!(row["acceptedBySource"], true);
            refusals += 1;
            continue;
        }
        let mut host = Host::new(&row);
        on_private_message(&mut host, row["raw"].as_str().unwrap().into()).unwrap();
        if let Some(task) = host.pending.take() {
            ready(task).unwrap();
        }
        host.end();
        let state = Input::Object(vec![
            ("count".encode_utf16().collect(), Input::Float(host.count)),
            (
                "handled".encode_utf16().collect(),
                Input::Bool(host.handled),
            ),
            ("last".encode_utf16().collect(), Input::Float(host.last)),
            (
                "positions".encode_utf16().collect(),
                Input::Array(host.positions),
            ),
        ]);
        assert_eq!(
            retained_obs(&state),
            row["state"],
            "PRIVATE native state; fixture {}",
            row["fixture"]
        );
    }
    assert_eq!(refusals, 1);
}
fn id(v: &Value) -> Vec<u16> {
    if let Some(units) = v.get("$units").and_then(Value::as_array) {
        return units.iter().map(|v| v.as_u64().unwrap() as u16).collect();
    }
    v.as_str().unwrap().encode_utf16().collect()
}
fn field_obs(v: &Value) -> Value {
    if v.get("$units").is_some() {
        units(&id(v))
    } else {
        vo(v)
    }
}
fn ustring(v: &[u16]) -> String {
    String::from_utf16(v).unwrap()
}
struct CancelHost {
    host: Host,
    orders: Vec<Rc<RefCell<Value>>>,
    input: Rc<RefCell<Vec<Rc<RefCell<Value>>>>>,
}
impl CancellationHost for CancelHost {
    type Input = Rc<RefCell<Vec<Rc<RefCell<Value>>>>>;
    type Order = Rc<RefCell<Value>>;
    type Body = Value;
    type Options = Value;
    type Error = Error;
    fn input(&mut self, input: &Self::Input) -> Result<Vec<Self::Order>, Error> {
        Ok(input.borrow().clone())
    }
    fn orders(&mut self) -> Result<Vec<Self::Order>, Error> {
        Ok(self.orders.clone())
    }
    fn attached_leg(&mut self, order: &Self::Order) -> Result<Option<&'static str>, Error> {
        let units = id(&order.borrow()["id"]);
        let tp: Vec<_> = "__tp".encode_utf16().collect();
        let sl: Vec<_> = "__sl".encode_utf16().collect();
        let leg = if units.ends_with(&tp) {
            Some("tp")
        } else if units.ends_with(&sl) {
            Some("sl")
        } else {
            None
        };
        let Some(leg) = leg else { return Ok(None) };
        let parent = &units[..units.len() - 4];
        let found = self.orders.iter().find(|o| id(&o.borrow()["id"]) == parent);
        Ok(found
            .filter(|o| {
                !matches!(
                    o.borrow()["type"].as_str(),
                    Some("take_profit_market" | "stop_market")
                )
            })
            .map(|_| leg))
    }
    fn id(&mut self, order: &Self::Order) -> Result<Vec<u16>, Error> {
        Ok(id(&order.borrow()["id"]))
    }
    fn parent_id(&mut self, mut id: Vec<u16>) -> Result<Vec<u16>, Error> {
        id.truncate(id.len() - 4);
        Ok(id)
    }
    fn kind(&mut self, order: &Self::Order) -> Result<Option<&'static str>, Error> {
        Ok(match order.borrow()["type"].as_str() {
            Some("limit") => Some("limit"),
            Some("market") => Some("market"),
            Some("take_profit_market") => Some("take_profit_market"),
            Some("stop_market") => Some("stop_market"),
            _ => None,
        })
    }
    fn symbol(&mut self, order: &Self::Order) -> Result<Vec<u16>, Error> {
        Ok(id(&order.borrow()["symbol"]))
    }
    fn parent_body(&mut self, parent: &Self::Order, id: &[u16]) -> Result<Value, Error> {
        Ok(json!({"symbol":parent.borrow()["symbol"],"orderList":[{"orderId":ustring(id)}]}))
    }
    fn standalone_body(&mut self, order: &Self::Order) -> Result<Value, Error> {
        let order = order.borrow();
        let raw = order
            .get("info")
            .and_then(|i| {
                i.get("id")
                    .filter(|v| !v.is_null())
                    .or_else(|| i.get("orderId"))
            })
            .filter(|v| !matches!(v,Value::String(s)if s.is_empty()));
        let raw = raw.map_or_else(
            || order["id"].clone(),
            |v| json!(safe_cex_num::js_string(Some(v)).unwrap()),
        );
        Ok(json!({"symbol":order["symbol"],"orderId":raw}))
    }
    fn group_body(&mut self, symbol: &[u16], orders: &[Self::Order]) -> Result<Value, Error> {
        let symbol =
            String::from_utf16(symbol).map_or_else(|_| json!({"$units":symbol}), |v| json!(v));
        Ok(
            json!({"symbol":symbol,"orderList":orders.iter().map(|o|json!({"orderId":o.borrow()["id"]})).collect::<Vec<_>>()}),
        )
    }
    async fn post(&mut self, endpoint: &'static str, body: Value) -> Result<(), Error> {
        fn special(v: &Value) -> Value {
            if v.get("$units").is_some() {
                return field_obs(v);
            }
            match v {
                Value::Object(m) => {
                    Value::Object(m.iter().map(|(k, v)| (k.clone(), special(v))).collect())
                }
                Value::Array(a) => json!(a.iter().map(special).collect::<Vec<_>>()),
                _ => vo(v),
            }
        }
        self.host.note("post", vec![text(endpoint), special(&body)]);
        if self.host.row["fixture"]["error"] == "post" {
            return Err(self.host.failure.clone());
        }
        if self.host.row["fixture"]["mutate"] == true {
            let mut pending = true;
            std::future::poll_fn(|_| {
                if pending {
                    pending = false;
                    Poll::Pending
                } else {
                    Poll::Ready(())
                }
            })
            .await;
            self.orders
                .iter()
                .find(|o| o.borrow()["id"] == "R")
                .unwrap()
                .borrow_mut()["price"] = json!(103);
            self.input
                .borrow_mut()
                .push(Rc::new(RefCell::new(json!({"id":"added"}))));
        }
        Ok(())
    }
    fn update_status(&mut self, ids: Vec<Vec<u16>>, status: &'static str) -> Result<(), Error> {
        self.host.note(
            "status",
            vec![
                json!(
                    ids.iter()
                        .map(|v| json!({"id":units(v)}))
                        .collect::<Vec<_>>()
                ),
                text(status),
            ],
        );
        if self.host.row["fixture"]["removeLeg"] == true {
            self.orders.retain(|o| o.borrow()["id"] != "R__sl");
        }
        Ok(())
    }
    fn recreate_options(
        &mut self,
        parent: &Self::Order,
        tp: Option<&Self::Order>,
        sl: Option<&Self::Order>,
    ) -> Result<Value, Error> {
        let p = parent.borrow();
        let mut map = serde_json::Map::new();
        for k in ["symbol", "type", "side", "price", "amount"] {
            map.insert(k.into(), p[k].clone());
        }
        map.insert(
            "reduceOnly".into(),
            p.get("reduceOnly")
                .filter(|v| safe_cex_num::js_truthy(Some(v)))
                .cloned()
                .unwrap_or(json!(false)),
        );
        if let Some(tp) = tp {
            let tp = tp.borrow();
            map.insert("takeProfit".into(), tp["price"].clone());
            map.insert("tpTriggerType".into(), tp["tpTriggerType"].clone());
        }
        if let Some(sl) = sl {
            let sl = sl.borrow();
            map.insert("stopLoss".into(), sl["price"].clone());
            map.insert("slTriggerType".into(), sl["slTriggerType"].clone());
        }
        Ok(Value::Object(map))
    }
    async fn place(&mut self, options: Value) -> Result<(), Error> {
        self.host.note("place", vec![vo(&options)]);
        if self.host.row["fixture"]["error"] == "place" {
            return Err(self.host.failure.clone());
        }
        Ok(())
    }
    fn report(&mut self, error: &Error) -> Result<(), Error> {
        assert!(
            Rc::ptr_eq(&error.token, &self.host.failure.token),
            "CANCEL native error identity"
        );
        self.host.note("emit", vec![text(&error.message)]);
        Ok(())
    }
}
#[test]
fn actual_typescript_cancel_orders() {
    let rows = oracle("cancel");
    assert_eq!(rows.len(), 14);
    for row in rows {
        let orders: Vec<_> = row["orders"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| Rc::new(RefCell::new(v.clone())))
            .collect();
        let input = Rc::new(RefCell::new(
            row["input"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| {
                    orders
                        .iter()
                        .find(|o| o.borrow()["id"] == v["id"])
                        .cloned()
                        .unwrap_or_else(|| Rc::new(RefCell::new(v.clone())))
                })
                .collect::<Vec<_>>(),
        ));
        let mut host = CancelHost {
            host: Host::new(&row),
            orders,
            input: input.clone(),
        };
        let (output, suspended) = {
            let mut future = pin!(cancel_orders(&mut host, input));
            let mut cx = Context::from_waker(Waker::noop());
            let first = future.as_mut().poll(&mut cx);
            let suspended = first.is_pending();
            let output = match first {
                Poll::Ready(output) => output,
                Poll::Pending => match future.as_mut().poll(&mut cx) {
                    Poll::Ready(output) => output,
                    Poll::Pending => panic!("Unexpected second cancellation suspension"),
                },
            };
            (output, suspended)
        };
        assert_eq!(
            suspended,
            row["fixture"]["mutate"] == true,
            "CANCEL native actual suspension"
        );
        let result = match output {
            Ok(()) => json!({"ok":true}),
            Err(e) => {
                assert!(
                    Rc::ptr_eq(&e.token, &host.host.failure.token),
                    "CANCEL native rethrow identity"
                );
                json!({"error":text(&e.message)})
            }
        };
        assert_eq!(result, row["result"], "CANCEL native result");
        host.host.end();
    }
}
struct BookHost {
    host: Host,
    connected: bool,
    handlers: HashMap<String, Vec<usize>>,
    topics: HashMap<String, f64>,
    timer: usize,
    book: Option<Rc<RefCell<OrderBook>>>,
}
fn book_obs(book: &OrderBook) -> Value {
    fn side(v: &[BookLevel]) -> Value {
        json!(
            v.iter()
                .map(
                    |v| json!({"price":bits(v.price),"amount":bits(v.amount),"total":bits(v.total)})
                )
                .collect::<Vec<_>>()
        )
    }
    json!({"bids":side(&book.bids),"asks":side(&book.asks)})
}
impl OrderBookHost<()> for BookHost {
    type Parser = usize;
    type Timer = usize;
    fn precision(&mut self, symbol: &str) -> Result<Option<Value>, String> {
        Ok(
            if self.host.row["fixture"]["market"].is_null()
                && self.host.row["fixture"].get("market").is_some()
                || symbol != "BTCUSDT"
            {
                None
            } else {
                Some(
                    self.host.row["fixture"]
                        .get("precision")
                        .cloned()
                        .unwrap_or(json!(0.1)),
                )
            },
        )
    }
    fn allocate_parser(&mut self) -> usize {
        1
    }
    fn connected(&mut self) -> Result<bool, String> {
        Ok(self.connected)
    }
    fn handlers(&mut self, key: &str) -> Result<Vec<usize>, String> {
        Ok(self.handlers.get(key).cloned().unwrap_or_default())
    }
    fn set_handlers(&mut self, key: &str, handlers: Vec<usize>) -> Result<(), String> {
        self.handlers.insert(key.into(), handlers);
        Ok(())
    }
    fn topic_count(&mut self, key: &str) -> Result<f64, String> {
        Ok(self.topics.get(key).copied().unwrap_or(f64::NAN))
    }
    fn set_topic_count(&mut self, key: &str, count: f64) -> Result<(), String> {
        self.topics.insert(key.into(), count);
        Ok(())
    }
    fn send(&mut self, frame: &J) -> Result<(), String> {
        self.host.note("send", vec![obs(frame)]);
        Ok(())
    }
    fn wait(&mut self, delay: f64) -> Result<usize, String> {
        self.host.note("wait", vec![bits(delay)]);
        self.timer += 1;
        Ok(self.timer)
    }
    fn timer_truthy(&mut self, timer: &usize) -> bool {
        *timer != 0
    }
    fn clear(&mut self, timer: usize) -> Result<(), String> {
        self.host.note("clear", vec![bits(timer as f64)]);
        Ok(())
    }
    fn cleanup_stats(&mut self, key: &str) -> Result<(), String> {
        self.host.note("cleanup", vec![text(key)]);
        Ok(())
    }
    fn call(&mut self, _: &(), book: Rc<RefCell<OrderBook>>) -> Result<(), String> {
        if let Some(previous) = &self.book {
            assert!(Rc::ptr_eq(previous, &book), "BOOK native callback identity")
        }
        self.host.note("book", vec![book_obs(&book.borrow())]);
        self.book = Some(book);
        Ok(())
    }
}
#[test]
fn actual_typescript_book_listener() {
    let rows = oracle("book");
    assert_eq!(rows.len(), 12);
    let mut refusals = 0;
    for row in rows {
        if row["fixture"]["refusal"] == true {
            assert!(
                serde_json::from_str::<Value>(row["rawJson"].as_str().unwrap()).is_err(),
                "BOOK native carrier refusal"
            );
            refusals += 1;
            continue;
        }
        let mut host = BookHost {
            host: Host::new(&row),
            connected: row["fixture"]["connected"] != false,
            handlers: HashMap::new(),
            topics: HashMap::new(),
            timer: 0,
            book: None,
        };
        let mut subscription = listen_order_book(
            &mut host,
            row["fixture"]["symbol"].as_str().unwrap_or("BTCUSDT"),
            (),
        )
        .unwrap();
        if row["fixture"]["resume"] == true {
            host.connected = true;
            subscription.subscribe(&mut host).unwrap();
            if row["fixture"]["resumeTwice"] == true {
                subscription.subscribe(&mut host).unwrap();
            }
        }
        if host.handlers.values().any(|v| !v.is_empty()) {
            let frames=row["fixture"].get("frames").cloned().unwrap_or(json!([{"data":{"bids":[["100","2"],["99","3"]],"asks":[["101","1"],["102","4"]]}}]));
            for frame in frames.as_array().unwrap() {
                subscription.parse(&mut host, frame).unwrap();
            }
            if row["fixture"]["extraParser"] == true {
                host.handlers
                    .get_mut(&subscription.handler)
                    .unwrap()
                    .push(2);
            }
        }
        subscription.cancel(&mut host).unwrap();
        host.host.end();
        if let Some(book) = &host.book {
            assert!(
                book.borrow().bids.is_empty() && book.borrow().asks.is_empty(),
                "BOOK native shared cleanup"
            );
            assert!(Rc::ptr_eq(book, &subscription.book));
        }
        let topics: Value = Value::Object(
            host.topics
                .iter()
                .map(|(k, v)| (k.clone(), bits(*v)))
                .collect(),
        );
        assert_eq!(topics, row["topics"], "BOOK native topic count");
        assert_eq!(
            json!(host.handlers[&subscription.handler].len()),
            row["remaining"],
            "BOOK native surviving handlers"
        );
    }
    assert_eq!(refusals, 1);
}
struct Factory {
    host: Host,
    parent: Rc<()>,
    options: Rc<()>,
    store: Rc<()>,
}
impl ExchangeConstructorHost<&str, Rc<()>, Rc<()>> for Factory {
    type Parent = Rc<()>;
    type Client = &'static str;
    type Socket = &'static str;
    type Error = String;
    fn base(&mut self, name: &str, opts: &Rc<()>, store: Rc<()>) -> Result<Rc<()>, String> {
        assert!(Rc::ptr_eq(opts, &self.options));
        assert!(Rc::ptr_eq(&store, &self.store));
        self.host.note("base", vec![text(name)]);
        Ok(self.parent.clone())
    }
    fn secondary_client(
        &mut self,
        parent: &Rc<()>,
        opts: &Rc<()>,
        rps: f64,
    ) -> Result<&'static str, String> {
        assert!(Rc::ptr_eq(parent, &self.parent));
        assert!(Rc::ptr_eq(opts, &self.options));
        self.host.note("rateLimit", vec![bits(rps)]);
        self.host.note("secondaryClient", vec![]);
        Ok("limited")
    }
    fn client(&mut self, parent: &Rc<()>, opts: &Rc<()>) -> Result<&'static str, String> {
        assert!(Rc::ptr_eq(parent, &self.parent));
        assert!(Rc::ptr_eq(opts, &self.options));
        self.host.note("client", vec![]);
        Ok("unlimited")
    }
    fn socket(
        &mut self,
        parent: &Rc<()>,
        kind: &'static str,
        secondary: Option<bool>,
    ) -> Result<&'static str, String> {
        assert!(Rc::ptr_eq(parent, &self.parent));
        self.host.note(
            "socket",
            vec![text(kind), secondary.map_or_else(undefined, |v| json!(v))],
        );
        Ok(kind)
    }
}
fn js(v: impl Into<String>) -> J {
    J::String(v.into())
}
fn socket_state(
    count: f64,
    route: J,
    last: f64,
    attempts: f64,
    max: f64,
) -> Vec<(&'static str, J)> {
    vec![
        ("messageCount", J::Number(count)),
        ("useFallbackUrl", route),
        ("lastMessageCount", J::Number(last)),
        ("connectionAttempts", J::Number(attempts)),
        ("MAX_ATTEMPTS", J::Number(max)),
        ("pingLoopTimeoutId", J::Undefined),
    ]
}
#[test]
fn actual_typescript_construction() {
    let rows = oracle("construction");
    assert_eq!(rows.len(), 7);
    for row in rows {
        let parent = Rc::new(());
        let state;
        let mut symbol_observation = None;
        let kind = row["fixture"]["kind"].as_str().unwrap();
        if kind == "exchange" {
            let mut factory = Factory {
                host: Host::new(&row),
                parent: parent.clone(),
                options: Rc::new(()),
                store: Rc::new(()),
            };
            let opts = factory.options.clone();
            let store = factory.store.clone();
            let venue = new_exchange(&mut factory, "bitunix", &opts, store).unwrap();
            assert!(
                Rc::ptr_eq(&venue.parent, &parent),
                "COMPOSITION native parent identity"
            );
            assert_eq!(
                text(venue.client_order_id_rule.label),
                row["rule"]["label"],
                "COMPOSITION native client rule label"
            );
            assert_eq!(
                text(venue.client_order_id_rule.requirement),
                row["rule"]["requirement"],
                "COMPOSITION native client rule requirement"
            );
            for case in row["rule"]["cases"].as_array().unwrap() {
                let units: Vec<_> = case["units"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap() as u16)
                    .collect();
                assert_eq!(
                    json!(venue.client_order_id_rule.is_valid(&units)),
                    case["valid"],
                    "COMPOSITION native client rule validation"
                );
            }
            assert!(
                venue.public_shards.is_empty()
                    && venue.symbol_settings.is_empty()
                    && venue.position_id_cache.is_empty()
                    && venue.order_tif_cache.is_empty()
                    && venue.order_reduce_only_cache.is_empty()
            );
            state = jo(vec![
                ("xhr", js(venue.xhr)),
                ("unlimitedXHR", js(venue.unlimited_xhr)),
                (
                    "publicWebsocket",
                    jo(vec![("kind", js(venue.public_socket))]),
                ),
                (
                    "publicWebsocketSecondary",
                    jo(vec![("kind", js(venue.secondary_socket))]),
                ),
                (
                    "privateWebsocket",
                    jo(vec![("kind", js(venue.private_socket))]),
                ),
                (
                    "futuresWebsocket",
                    jo(vec![("kind", js(venue.futures_socket))]),
                ),
                ("publicWebsocketShards", J::Array(Vec::new())),
                (
                    "grafanaEndpointsEvery",
                    J::Array(venue.grafana_every.iter().map(|s| js(*s)).collect()),
                ),
                (
                    "grafanaEndpointsPeriodic",
                    J::Array(venue.grafana_periodic.iter().map(|s| js(*s)).collect()),
                ),
                ("enableFakeFullTpSl", J::Bool(venue.enable_fake_full_tpsl)),
                ("accountPositionMode", js(venue.account_position_mode)),
                (
                    "lastTickAccountPositionMode",
                    js(venue.last_tick_position_mode),
                ),
                ("symbolSettings", J::Array(Vec::new())),
                ("positionIdCache", J::Array(Vec::new())),
                ("orderTifCache", J::Array(Vec::new())),
                ("orderReduceOnlyCache", J::Array(Vec::new())),
            ]);
            factory.host.end();
        } else {
            let host = RefCell::new(Host::new(&row));
            let base = |p: &Rc<()>| {
                assert!(Rc::ptr_eq(p, &parent), "COMPOSITION native parent identity");
                host.borrow_mut().note("base", vec![text(kind)]);
                Ok::<(), String>(())
            };
            let route = |_: &Rc<()>| {
                Ok::<_, String>(
                    row["fixture"]
                        .get("route")
                        .map_or(J::Undefined, |v| J::from(v.clone())),
                )
            };
            match kind {
                "public" => {
                    let symbols = row["fixture"]
                        .get("symbols")
                        .map(|v| v.as_array().unwrap().iter().map(id).collect());
                    let socket: PublicSocket<_, _, usize, usize, bool, Vec<u16>> =
                        new_public_socket(
                            parent.clone(),
                            row["fixture"].get("secondary").and_then(Value::as_bool),
                            || false,
                            symbols,
                            base,
                            route,
                        )
                        .unwrap();
                    assert!(
                        socket.message_handlers.is_empty()
                            && socket.topics.is_empty()
                            && socket.subscription_queue.is_empty()
                    );
                    let mut fields = socket_state(
                        socket.message_count,
                        socket.route,
                        socket.last_message_count,
                        socket.connection_attempts,
                        socket.max_attempts,
                    );
                    fields.extend([
                        ("messageHandlers", jo(Vec::new())),
                        ("topics", jo(Vec::new())),
                        ("subscriptionTimeoutId", J::Undefined),
                        ("isSecondary", J::Bool(socket.is_secondary)),
                        ("marketSymbols", {
                            symbol_observation =
                                Some(socket.market_symbols.as_ref().map_or_else(undefined, |v| {
                                    json!(v.iter().map(|v| units(v)).collect::<Vec<_>>())
                                }));
                            J::Undefined
                        }),
                        ("subscriptionQueue", J::Array(Vec::new())),
                    ]);
                    state = jo(fields);
                }
                "private" => {
                    let socket: PrivateSocket<_, _, usize> =
                        new_private_socket(parent.clone(), base, route).unwrap();
                    let mut fields = socket_state(
                        socket.message_count,
                        socket.route,
                        socket.last_message_count,
                        socket.connection_attempts,
                        socket.max_attempts,
                    );
                    fields.extend([
                        ("handledPongEvent", J::Bool(socket.handled_pong)),
                        ("lastPongEventTimestamp", J::Number(socket.last_pong)),
                        ("balanceThrottleTimeout", J::Undefined),
                        ("ordersThrottleTimeout", J::Undefined),
                        ("positionsThrottleTimeout", J::Undefined),
                    ]);
                    state = jo(fields);
                }
                "futures" => {
                    let socket: FuturesSocket<_, _, usize, usize> =
                        new_futures_socket(parent.clone(), base, route).unwrap();
                    assert!(socket.topics.is_empty() && socket.message_handlers.is_empty());
                    let mut fields = socket_state(
                        socket.message_count,
                        socket.route,
                        socket.last_message_count,
                        socket.connection_attempts,
                        socket.max_attempts,
                    );
                    fields.extend([
                        ("topics", jo(Vec::new())),
                        ("messageHandlers", jo(Vec::new())),
                    ]);
                    state = jo(fields);
                }
                _ => panic!("Unknown constructor"),
            }
            host.borrow().end();
        }
        let mut observed = obs(&state);
        if let Some(symbols) = symbol_observation {
            observed["marketSymbols"] = symbols;
        }
        assert_eq!(
            observed, row["state"],
            "COMPOSITION native state; kind {kind}"
        );
    }
}
