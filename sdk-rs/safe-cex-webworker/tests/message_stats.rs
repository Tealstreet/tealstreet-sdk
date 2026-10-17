#[path = "../../safe-cex-exchange/tests/support/runtime.rs"]
mod runtime;
#[path = "support/ws_runtime.rs"]
mod ws_runtime;
use runtime::Runtime;
use safe_cex_exchange::message_stats::*;
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange},
    socket_lifecycle::SocketSlot,
    websocket::{BaseWebSocket, MessageHandlerStat, WsProtocol},
    ws_parent::WsExchangeParent,
};
use safe_cex_store::store::{DEFAULT_STORE, DefaultStore};
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use std::rc::Rc;
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
struct Protocol;
impl WsProtocol for Protocol {}
fn venue() -> (Rc<dyn Exchange>, Rc<Runtime>) {
    let runtime = Rc::new(Runtime::default());
    runtime.discard_spawn.set(true);
    runtime.advance(1);
    let exchange = Rc::new(Venue {
        base: BaseExchange::new(
            ExchangeName::Bybit,
            serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
            Rc::new(DefaultStore::new()),
            runtime.clone(),
            Default::default(),
        ),
        capabilities: Default::default(),
    });
    (exchange, runtime)
}
fn socket(exchange: &Rc<dyn Exchange>, runtime: Rc<Runtime>, v: &Value) -> Rc<BaseWebSocket> {
    let socket = BaseWebSocket::new(
        0,
        WsExchangeParent::new(exchange),
        runtime,
        Rc::new(Protocol),
    );
    if v["isInitialized"] == true {
        socket.track_last_received().unwrap();
    }
    for (key, row) in v["messageHandlersStats"].as_object().unwrap() {
        socket.message_handlers_stats.borrow_mut().insert(
            key.clone(),
            MessageHandlerStat {
                last_message_timestamp: row["lastMessageTimestamp"].as_f64().unwrap(),
                last_callbacks_count: row
                    .get("lastCallbacksCount")
                    .map(|v| v.as_u64().unwrap() as usize)
                    .into(),
                had_handlers: row.get("hadHandlers").and_then(Value::as_bool),
                had_handlers_undefined: false,
            },
        );
    }
    socket
}
fn normalize(v: Value) -> Value {
    match v {
        Value::Number(n) => json!(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.into_iter().map(normalize).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, normalize(v))).collect()),
        v => v,
    }
}
#[test]
fn message_stats_match_live_socket_precedence_and_pin_shared_representation_gaps() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/message_stats_oracle.mjs"
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
    assert_eq!(rows.len(), 160);
    for (i, row) in rows.iter().enumerate() {
        let (exchange, runtime) = venue();
        let mut store = DEFAULT_STORE.clone();
        store.message_handler_stats = row["input"].get("existing").cloned();
        for (n, (name, slot)) in [
            ("publicWebsocket", SocketSlot::Public),
            ("businessWebsocket", SocketSlot::Business),
            ("futuresWebsocket", SocketSlot::Futures),
            ("privateWebsocket", SocketSlot::Private),
            ("publicInverseWebsocket", SocketSlot::PublicInverse),
            ("publicWebsocketSecondary", SocketSlot::PublicSecondary),
        ]
        .into_iter()
        .enumerate()
        {
            let socket = row["input"]["sockets"]
                .get(name)
                .map(|v| socket(&exchange, runtime.clone(), v));
            assert_eq!(
                websocket_has_stats(socket.as_deref()),
                row["has"][n].as_bool().unwrap(),
                "has stats {i}/{n}"
            );
            exchange.base().set_socket(slot, socket);
        }
        MessageHandlerStatsPostprocessor
            .process(&mut store, exchange.as_ref())
            .unwrap();
        assert_eq!(
            normalize(store.message_handler_stats.unwrap()),
            normalize(row["expected"].clone()),
            "stats scenario{i}"
        );
    }
    for row in data["optionalShapes"].as_array().unwrap() {
        let (exchange, runtime) = venue();
        let socket = socket(
            &exchange,
            runtime,
            &json!({"messageHandlersStats":{"topic":row["topic"]}}),
        );
        exchange.base().set_socket(SocketSlot::Public, Some(socket));
        let mut store = DEFAULT_STORE.clone();
        MessageHandlerStatsPostprocessor
            .process(&mut store, exchange.as_ref())
            .unwrap();
        assert_eq!(
            normalize(store.message_handler_stats.unwrap()),
            normalize(row["expected"].clone()),
            "OPTIONAL-STATS: {row}"
        );
    }
    let (exchange, runtime) = venue();
    let socket = socket(
        &exchange,
        runtime,
        &json!({"isInitialized":false,"messageHandlersStats":{"topic":{"lastMessageTimestamp":1}}}),
    );
    exchange
        .base()
        .set_socket(SocketSlot::Public, Some(socket.clone()));
    let mut store = DEFAULT_STORE.clone();
    MessageHandlerStatsPostprocessor
        .process(&mut store, exchange.as_ref())
        .unwrap();
    assert_eq!(
        data["witness"]["partial"]["public"]["topic"],
        json!({"lastMessageTimestamp":1})
    );
    assert_eq!(
        store.message_handler_stats.as_ref().unwrap()["public"]["topic"],
        json!({"lastMessageTimestamp":1.0}),
        "core optional statistics retain absent fields"
    );
    socket
        .message_handlers_stats
        .borrow_mut()
        .get_mut("topic")
        .unwrap()
        .last_message_timestamp = 99.0;
    assert_eq!(data["witness"]["aliasedTimestamp"], 99);
    assert_eq!(
        store.message_handler_stats.as_ref().unwrap()["public"]["topic"]["lastMessageTimestamp"],
        1.0,
        "CHOSEN JSON snapshot omits source alias"
    );
    socket
        .message_handlers_stats
        .borrow_mut()
        .get_mut("topic")
        .unwrap()
        .last_message_timestamp = f64::INFINITY;
    MessageHandlerStatsPostprocessor
        .process(&mut store, exchange.as_ref())
        .unwrap();
    assert_eq!(data["witness"]["infinite"], true);
    assert!(
        store.message_handler_stats.as_ref().unwrap()["public"]["topic"]["lastMessageTimestamp"]
            .is_null(),
        "CHOSEN JSON boundary collapses nonfinite stats"
    );
    socket.message_handlers_stats.borrow_mut().clear();
    assert_eq!(data["witness"]["inheritedHas"], true);
    assert!(
        !websocket_has_stats(Some(&socket)),
        "CHOSEN HashMap cannot hold inherited topics"
    );
    store.message_handler_stats = Some(json!([]));
    socket
        .message_handlers_stats
        .borrow_mut()
        .insert("topic".into(), MessageHandlerStat::default());
    assert_eq!(data["witness"]["arrayNamedGroup"], true);
    assert!(
        MessageHandlerStatsPostprocessor
            .process(&mut store, exchange.as_ref())
            .is_err(),
        "CHOSEN JSON arrays cannot hold named stats properties"
    );
    exchange.base().set_socket(SocketSlot::Public, None);
    socket.message_handlers_stats.borrow_mut().clear();
    socket.message_handlers_stats.borrow_mut().insert(
        "secondary".into(),
        MessageHandlerStat {
            last_message_timestamp: 4.0,
            last_callbacks_count: safe_cex_num::PrototypeScalar::from(Some(4)),
            had_handlers: Some(true),
            had_handlers_undefined: false,
        },
    );
    exchange
        .base()
        .set_socket(SocketSlot::PublicSecondary, Some(socket));
    for row in data["primaryShapes"].as_array().unwrap() {
        store.message_handler_stats = Some(json!({"public":row["primary"]}));
        let result = MessageHandlerStatsPostprocessor.process(&mut store, exchange.as_ref());
        if safe_cex_num::js_truthy(Some(&row["primary"])) {
            assert!(
                result.is_err(),
                "CHOSEN non-record primary rejection: {row}"
            );
            let mut expected = json!({"secondary":{"lastMessageTimestamp":4,"lastCallbacksCount":4,"hadHandlers":true}});
            if row["primary"].is_string() {
                expected["0"] = json!("x");
                expected["1"] = json!("y");
            }
            assert_eq!(
                row["expected"]["public"], expected,
                "live scalar spread: {row}"
            );
        } else {
            result.unwrap();
            assert_eq!(
                store.message_handler_stats.as_ref().unwrap(),
                &row["expected"],
                "falsey primary bypass: {row}"
            );
        }
    }
}
