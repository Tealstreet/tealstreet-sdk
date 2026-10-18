#[path = "../../safe-cex-exchange/tests/support/runtime.rs"]
mod runtime;
#[path = "../../safe-cex-exchange/tests/support/store.rs"]
mod test_store;
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange},
};
use safe_cex_types::{ExchangeName, Order, Position};
use safe_cex_webworker::{
    chaser::ChaserOrderInstance,
    chaser_manager::{check_and_mark_if_complete, get_current_orders, log},
    task_types::ChaserOrderOptions,
};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};
struct Venue {
    base: BaseExchange,
    capabilities: ExchangeCapabilities,
}
impl Exchange for Venue {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
}
fn numeric(v: Value) -> Value {
    match v {
        Value::Number(n) => json!(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.into_iter().map(numeric).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, numeric(v))).collect()),
        v => v,
    }
}
fn position(v: &Value) -> Position {
    let mut input = v.clone();
    input["notional"] = json!(0);
    let mut p: Position = serde_json::from_value(input).unwrap();
    p.notional = match v["notional"].as_str() {
        Some("NaN") => f64::NAN,
        Some("Infinity") => f64::INFINITY,
        Some("-0") => -0.0,
        _ => v["notional"].as_f64().unwrap(),
    };
    p
}
fn fixture(row: &Value, id: &str) -> (Rc<ChaserOrderInstance>, Rc<test_store::TestStore>) {
    let runtime = Rc::new(runtime::Runtime::default());
    runtime.advance(0);
    *runtime.id.borrow_mut() = Some(id.into());
    let store = Rc::new(test_store::TestStore::new());
    store.data.borrow_mut().orders = serde_json::from_value(row["orders"].clone()).unwrap();
    store.data.borrow_mut().positions = row["positions"]
        .as_array()
        .unwrap()
        .iter()
        .map(position)
        .collect();
    let venue = Rc::new(Venue {
        base: BaseExchange::new(
            ExchangeName::Bybitdemo,
            serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
            store.clone(),
            runtime,
            Default::default(),
        ),
        capabilities: Default::default(),
    });
    let options:ChaserOrderOptions=serde_json::from_value(json!({"symbol":"BTC","size":3,"side":row["side"],"maxAttempts":1,"delay":100,"reduceOnly":row["reduceOnly"]})).unwrap();
    let instance =
        Rc::new(ChaserOrderInstance::new(venue, Rc::new(RefCell::new(options))).unwrap());
    instance.set_current_order_ids(serde_json::from_value(row["ids"].clone()).unwrap());
    if row["stopped"] == true {
        instance.mark_complete(Some("prior"));
    }
    (instance, store)
}
#[test]
fn tracked_orders_completion_and_logs_match_live_manager_and_pin_alias_boundary() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/chaser_completion_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = data["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 6864);
    for (index, row) in rows
        .iter()
        .chain(std::iter::once(&data["reentrant"]))
        .enumerate()
    {
        let (instance, store) = fixture(row, "id");
        let events = Rc::new(RefCell::new(Vec::<Value>::new()));
        let log_events = events.clone();
        let target = store.clone();
        let reentrant = row["reentrant"] == true;
        instance.exchange.base().on(
            "log",
            Rc::new(move |event, message| {
                log_events
                    .borrow_mut()
                    .push(json!({"event":event,"message":safe_cex_exchange::js_runtime::value::to_json(message).unwrap()}));
                if reentrant {
                    target.data.borrow_mut().orders[0].status =
                        safe_cex_types::OrderStatus::Closed.into();
                }
                Ok(())
            }),
        );
        assert_eq!(
            numeric(serde_json::to_value(get_current_orders(&instance).unwrap()).unwrap()),
            numeric(row["selection"].clone()),
            "TRACKED-ORDERS row {index}"
        );
        check_and_mark_if_complete(&instance).unwrap();
        let actual = json!({"ids":instance.current_order_ids(),"stopped":instance.is_stopped(),"reason":instance.state().borrow().stop_reason,"events":events.borrow().clone()});
        if reentrant {
            assert_eq!(row["expected"]["stopped"], true);
            assert_eq!(row["expected"]["ids"], json!([]));
            assert_eq!(actual["stopped"], false);
            assert_eq!(actual["ids"], json!(["a"]));
            assert_eq!(actual["events"], row["expected"]["events"]);
            assert_eq!(
                store.data.borrow().orders[0].status,
                safe_cex_types::OrderStatus::Closed
            );
        } else {
            assert_eq!(actual, row["expected"], "ORDER-COMPLETION row {index}");
        }
    }
    let (instance, store) = fixture(&data["reentrant"], "id");
    let mut selected: Vec<Order> = get_current_orders(&instance).unwrap();
    store.data.borrow_mut().orders[0].status = safe_cex_types::OrderStatus::Closed.into();
    selected[0].price = 42.0.into();
    assert_eq!(
        data["alias"],
        json!({"selectedStatus":"closed","storePrice":42})
    );
    assert_eq!(selected[0].status, safe_cex_types::OrderStatus::Open);
    assert_eq!(store.data.borrow().orders[0].price, 100.0);
    let mut logs = Vec::new();
    for id in ["id", "", "𐐀"] {
        let (instance, _) = fixture(&data["reentrant"], id);
        let events = Rc::new(RefCell::new(Vec::new()));
        let captured = events.clone();
        let name = id.to_owned();
        instance.exchange.base().on(
            "log",
            Rc::new(move |event, message| {
                captured
                    .borrow_mut()
                    .push(json!({"id":name,"event":event,"message":safe_cex_exchange::js_runtime::value::to_json(message).unwrap()}));
                Ok(())
            }),
        );
        for message in ["", "x", " α\n", "𐐀"] {
            log(message, &instance).unwrap();
        }
        logs.extend(events.borrow().clone());
    }
    assert_eq!(
        json!(logs),
        data["logs"],
        "CHASER-LOG format and event target"
    );
    assert!(data["surrogateInput"].as_str().unwrap().contains("\\ud800"));
    assert!(serde_json::from_str::<String>(data["surrogateInput"].as_str().unwrap()).is_err());
    assert!(
        data["surrogateOutput"]
            .as_str()
            .unwrap()
            .contains("\\ud800")
    );
    let input = data["surrogateCaseInput"].as_str().unwrap();
    assert!(input.contains("\\ud800"));
    assert!(serde_json::from_str::<Value>(input).is_err());
    assert!(
        data["surrogateSelection"]
            .as_str()
            .unwrap()
            .contains("\\ud800")
    );
    let completed = data["surrogateCompletion"].as_str().unwrap();
    assert!(completed.contains("\\ud800"));
    assert!(completed.contains("\"stopped\":true"));
}
