#[path = "../../parity/evidence/retained_model_values.rs"]
pub mod retained_model_values;
use safe_cex_num::{JsJsonValue, numeric_ingress::js_from_str};
use safe_cex_types::{Order, OrderSide, OrderType, Position, PositionSide};
use safe_cex_venue_bitunix::{order_state as state, tpsl_helpers::TpslLeg};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::HashMap,
    future::{Future, ready},
    pin::pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn resolve<T>(f: impl Future<Output = T>) -> T {
    match pin!(f).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(v) => v,
        _ => panic!(),
    }
}
fn number(n: f64) -> Value {
    json!({"bits":format!("{:016x}",if n.is_nan(){0x7ff8_0000_0000_0000}else{n.to_bits()})})
}
fn obs(v: JsJsonValue) -> Value {
    match v {
        JsJsonValue::Undefined => json!({"undefined":true}),
        JsJsonValue::Null => Value::Null,
        JsJsonValue::Bool(v) => json!(v),
        JsJsonValue::Number(v) => number(v),
        JsJsonValue::String(v) => json!(v),
        JsJsonValue::Array(v) => json!(v.into_iter().map(obs).collect::<Vec<_>>()),
        JsJsonValue::Object(v) => Value::Object(v.into_iter().map(|(k, v)| (k, obs(v))).collect()),
        _ => panic!(),
    }
}
fn positions(f: &Value) -> Vec<Position> {
    js_from_str(&serde_json::to_string(f.get("positions").unwrap_or(&json!([{"id":"BTCUSDT:short","symbol":"BTCUSDT","side":"short","entryPrice":0,"notional":0,"leverage":10,"unrealizedPnl":0,"contracts":2.5,"liquidationPrice":0,"info":{"positionId":"WS-ID"}}]))).unwrap()).unwrap()
}
#[test]
fn actual_typescript_order_state() {
    let out = Command::new("node")
        .arg(format!(
            "{}/tests/order-state-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "ORDER STATE actual source {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rows.len(), 67, "ORDER STATE independent case floor");
    let mut counts = [0; 3];
    for (i, row) in rows.iter().enumerate() {
        let f: Value = serde_json::from_str(row["fixture"].as_str().unwrap()).unwrap();
        let mut expected = row["result"].clone();
        if let Some(kind) = expected
            .get_mut("ok")
            .and_then(|value| value.get_mut("orderType"))
        {
            if let Some(text) = kind.as_str() {
                *kind = json!({"units":text.encode_utf16().collect::<Vec<_>>()});
            }
        }
        let actual = match row["key"].as_str().unwrap() {
            "snapshot" => {
                counts[0] += 1;
                let order: Order =
                    js_from_str(&serde_json::to_string(&f["order"]).unwrap()).unwrap();
                let orders: Vec<Order> = js_from_str(
                    &serde_json::to_string(f.get("orders").unwrap_or(&json!([]))).unwrap(),
                )
                .unwrap();
                match state::get_tpsl_leg_snapshot(
                    &order,
                    if f["leg"] == "tp" {
                        TpslLeg::Tp
                    } else {
                        TpslLeg::Sl
                    },
                    &orders,
                ) {
                    Ok(s) => {
                        json!({"ok":s.map_or(json!({"undefined":true}),|s|json!({"price":number(s.price),"qty":number(s.qty),"stopType":s.stop_type,"orderType":{"units":s.order_type.as_text().unwrap()},"orderPrice":number(s.order_price)}))})
                    }
                    Err(e) => json!({"error":e.name,"message":e.message}),
                }
            }
            "id" => {
                counts[1] += 1;
                let positions = positions(&f);
                let cache: HashMap<String, safe_cex_types::retained_json::RetainedJsonInput> =
                    serde_json::from_value(
                        f.get("cache")
                            .cloned()
                            .unwrap_or(json!({"BTCUSDT:short":"CACHE-ID"})),
                    )
                    .unwrap();
                let result =
                    state::get_position_id("BTCUSDT", PositionSide::Short, &positions, &cache);
                if let Some(v) = result {
                    let origin = positions.iter().find(|p| {
                        p.symbol == "BTCUSDT" && p.side == PositionSide::Short && p.contracts > 0.
                    });
                    let raw = origin
                        .and_then(|p| p.info.as_ref())
                        .and_then(|i| i.get("positionId"));
                    let expected = raw
                        .filter(|v| v.is_truthy())
                        .or_else(|| cache.get("BTCUSDT:short"))
                        .unwrap();
                    assert!(
                        std::ptr::eq(v, expected),
                        "ORDER STATE native position id identity"
                    );
                }
                assert_eq!(row["identity"], true, "ORDER STATE source id identity");
                result.map_or(json!({"undefined":true}), |v| {
                    obs(retained_model_values::prepared(v))
                })
            }
            "hydrate" => {
                counts[2] += 1;
                let positions = positions(&f);
                let cache: HashMap<String, Value> =
                    serde_json::from_value(f.get("cache").cloned().unwrap_or(json!({}))).unwrap();
                let events = RefCell::new(Vec::<Value>::new());
                let note = |v: Value| {
                    let j = events.borrow().len();
                    assert_eq!(
                        v, row["events"][j],
                        "ORDER STATE native hydration effect {i}:{j}"
                    );
                    events.borrow_mut().push(v);
                };
                let fetched_positions = Rc::new(json!([{"id":"fresh"}]));
                let fetched_orders = Rc::new(json!([{"id":"new"}]));
                let update = |key: &str, got: Rc<Value>, expected: &Rc<Value>| {
                    assert!(
                        Rc::ptr_eq(&got, expected),
                        "ORDER STATE native hydration payload identity"
                    );
                    let mut patch = serde_json::Map::new();
                    patch.insert(key.into(), (*got).clone());
                    note(json!(["update", Value::Object(patch)]));
                    if f["fail"] == "update" {
                        Err("update failed")
                    } else {
                        Ok(())
                    }
                };
                let result = resolve(state::hydrate_position_id_before_formatting(
                    "BTCUSDT",
                    match f["type"].as_str() {
                        Some("limit") => OrderType::Limit,
                        Some("take_profit_market") => OrderType::TakeProfit,
                        _ => OrderType::StopLoss,
                    },
                    OrderSide::Buy,
                    f["amountBits"].as_str().map_or_else(
                        || f["amount"].as_f64().unwrap_or(1.),
                        |bits| f64::from_bits(u64::from_str_radix(bits, 16).unwrap()),
                    ),
                    f["reduceOnly"].as_bool(),
                    &positions,
                    &cache,
                    || {
                        note(json!(["fetch", "positions"]));
                        ready(if f["fail"] == "positions" {
                            Err("positions failed")
                        } else {
                            Ok(fetched_positions.clone())
                        })
                    },
                    |got| update("positions", got, &fetched_positions),
                    || {
                        note(json!(["fetch", "orders"]));
                        ready(if f["fail"] == "orders" {
                            Err("orders failed")
                        } else {
                            Ok(fetched_orders.clone())
                        })
                    },
                    |got| update("orders", got, &fetched_orders),
                ));
                assert_eq!(
                    json!(*events.borrow()),
                    row["events"],
                    "ORDER STATE native hydration effect count {i}"
                );
                match result {
                    Ok(()) => json!({"ok":{"undefined":true}}),
                    Err(e) => json!({"error":"Error","message":e}),
                }
            }
            _ => panic!(),
        };
        assert_eq!(
            actual, expected,
            "ORDER STATE decoded native {} case {i}",
            row["key"]
        );
    }
    assert_eq!(counts, [42, 12, 13], "ORDER STATE independent method floor");
}
