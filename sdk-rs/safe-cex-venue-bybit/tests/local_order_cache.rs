// Field tables written in the loop shape their siblings use, so adding a second
// field is one row rather than a restructure. One row today is not a reason to
// unroll the table.
#![allow(clippy::single_element_loop)]
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::{Order, OrderSide, OrderStatus, OrderTimeInForce, OrderType};
use safe_cex_venue_bybit::local_order_cache::*;
use serde::Deserialize;
use serde_json::Value;
use std::{cell::RefCell, collections::HashMap, rc::Rc};

#[derive(Debug, PartialEq, Deserialize)]
struct Event {
    kind: String,
    keys: Vec<String>,
    orders: Vec<String>,
}
fn keys(cache: &RecentOrderCache) -> Vec<String> {
    cache
        .snapshot()
        .into_iter()
        .map(|(key, _, _)| key)
        .collect()
}
fn number(actual: f64, expected: &Value, field: &str) {
    let bits = if actual.is_nan() {
        "nan".into()
    } else {
        format!("{:016x}", actual.to_bits())
    };
    assert_eq!(
        bits,
        expected["number"].as_str().unwrap(),
        "CACHE native number {field}"
    );
}
fn order(actual: &Order, expected: &Value) {
    assert_eq!(
        actual.id,
        expected["id"].as_str().unwrap(),
        "CACHE native order id"
    );
    assert_eq!(
        actual.symbol,
        expected["symbol"].as_str().unwrap(),
        "CACHE native order symbol"
    );
    assert_eq!(
        actual.r#type,
        serde_json::from_value::<OrderType>(expected["type"].clone()).unwrap(),
        "CACHE native order type"
    );
    assert_eq!(
        actual.status,
        serde_json::from_value::<OrderStatus>(expected["status"].clone()).unwrap(),
        "CACHE native order status"
    );
    assert_eq!(
        actual.side,
        serde_json::from_value::<OrderSide>(expected["side"].clone()).unwrap(),
        "CACHE native order side"
    );
    for (name, value) in [
        ("price", actual.price.to_number()),
        ("amount", actual.amount),
        ("filled", actual.filled),
        ("remaining", actual.remaining),
        ("lastUpdate", actual.last_update),
    ] {
        number(value, &expected[name], name);
    }
    assert_eq!(
        actual.reduce_only,
        expected["reduceOnly"].as_bool().unwrap(),
        "CACHE native reduce only"
    );
    assert_eq!(
        actual.time_in_force,
        Some(serde_json::from_value::<OrderTimeInForce>(expected["timeInForce"].clone()).unwrap()),
        "CACHE native time in force"
    );
}
fn mutate(order: &Rc<RefCell<Order>>, fields: &Value) {
    let mut order = order.borrow_mut();
    if let Some(value) = fields.get("id") {
        order.id = value.as_str().unwrap().into();
    }
    if let Some(value) = fields.get("type") {
        order.r#type = serde_json::from_value(value.clone()).unwrap();
    }
    for (name, target) in [("amount", &mut order.amount)] {
        if let Some(value) = fields.get(name) {
            *target = value.as_f64().unwrap();
        }
    }
    if let Some(value) = fields.get("price") {
        order.price = (value.as_f64().unwrap()).into();
    }
    if let Some(value) = fields.get("filled") {
        order.filled = value.as_f64().unwrap();
    }
    if let Some(value) = fields.get("remaining") {
        order.remaining = value.as_f64().unwrap();
    }
    if let Some(value) = fields.get("lastUpdate") {
        order.last_update = value.as_f64().unwrap();
    }
}

#[test]
#[ignore = "requires installed live TypeScript"]
fn local_order_cache_native_source_parity() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/local-order-cache-fixtures.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let fixture: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        fixture["fallback"].as_array().unwrap().len(),
        22,
        "CACHE independent fallback floor"
    );
    for case in fixture["fallback"].as_array().unwrap() {
        let value = case["raw"]
            .as_str()
            .map(|raw| serde_json::from_str::<Value>(raw).unwrap());
        assert_eq!(
            should_use_local_rest_order_lifecycle_fallback(value.as_ref()),
            case["result"].as_bool().unwrap(),
            "CACHE native fallback"
        );
    }
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 30, "CACHE independent scenario floor");
    let mut observations = 0;
    for case in cases {
        let cache = RecentOrderCache::default();
        let registry: HashMap<String, Rc<RefCell<Order>>> = case["orders"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(key, value)| {
                (
                    key.clone(),
                    Rc::new(RefCell::new(serde_json::from_value(value.clone()).unwrap())),
                )
            })
            .collect();
        let events = RefCell::new(Vec::<Event>::new());
        let reference = |value: &Rc<RefCell<Order>>| {
            registry
                .iter()
                .find(|(_, order)| Rc::ptr_eq(value, order))
                .unwrap()
                .0
                .clone()
        };
        for step in case["steps"].as_array().unwrap() {
            observations += 1;
            events.borrow_mut().clear();
            let command = &step["command"];
            let expected = &step["result"];
            let mut host_error = ExchangeError::new("store callback failed");
            host_error.name = "StoreError".into();
            host_error.code = Some(safe_cex_exchange::js_runtime::value::from_json(
                &serde_json::json!(41),
            ));
            let site_state = RefCell::new(command.get("site").cloned());
            let store_state = RefCell::new(
                command["store"]
                    .as_array()
                    .map(|values| {
                        values
                            .iter()
                            .map(|key| Rc::clone(&registry[key.as_str().unwrap()]))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default(),
            );
            let now = || {
                events.borrow_mut().push(Event {
                    kind: "clock".into(),
                    keys: keys(&cache),
                    orders: Vec::new(),
                });
                if let Some(change) = command.get("clockMutate") {
                    mutate(
                        &registry[change["key"].as_str().unwrap()],
                        &change["fields"],
                    );
                }
                if let Some(id) = command["clockSeen"].as_str() {
                    mark_order_seen(&cache, id);
                }
                if let Some(site) = command.get("clockSite") {
                    *site_state.borrow_mut() = Some(site.clone());
                }
                if let Some(values) = command["clockStore"].as_array() {
                    *store_state.borrow_mut() = values
                        .iter()
                        .map(|key| Rc::clone(&registry[key.as_str().unwrap()]))
                        .collect();
                }
                match command["time"].as_str().unwrap() {
                    "NaN" => f64::NAN,
                    "Infinity" => f64::INFINITY,
                    "-Infinity" => f64::NEG_INFINITY,
                    token => token.parse::<f64>().unwrap(),
                }
            };
            let site = || site_state.borrow().clone();
            let result = match command["op"].as_str().unwrap() {
                "record" => record_recently_placed_order(
                    &cache,
                    Rc::clone(&registry[command["key"].as_str().unwrap()]),
                    site,
                    now,
                    |orders| {
                        events.borrow_mut().push(Event {
                            kind: "add".into(),
                            keys: keys(&cache),
                            orders: orders.iter().map(reference).collect(),
                        });
                        if command["host"] == "seen" {
                            mark_order_seen(&cache, &orders[0].borrow().id);
                        }
                        if command["host"] == "fail" {
                            Err(host_error.clone())
                        } else {
                            Ok(())
                        }
                    },
                ),
                "seen" => {
                    mark_order_seen(&cache, command["id"].as_str().unwrap());
                    Ok(())
                }
                "retire" => {
                    let ids: Vec<String> = serde_json::from_value(command["ids"].clone()).unwrap();
                    retire_cancelled_orders(&cache, &ids, site, |ids| {
                        events.borrow_mut().push(Event {
                            kind: "remove".into(),
                            keys: keys(&cache),
                            orders: ids.to_vec(),
                        });
                        if command["host"] == "fail" {
                            Err(host_error.clone())
                        } else {
                            Ok(())
                        }
                    })
                }
                "mutate" => {
                    mutate(
                        &registry[command["key"].as_str().unwrap()],
                        &command["fields"],
                    );
                    Ok(())
                }
                "preserve" => {
                    let refs = |values: &Value| {
                        values
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|key| Rc::clone(&registry[key.as_str().unwrap()]))
                            .collect::<Vec<_>>()
                    };
                    let fetched = refs(&command["fetched"]);
                    let original_ptr = fetched.as_ptr();
                    let original_len = fetched.len();
                    let preserved = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        preserve_recently_placed_orders(
                            &cache,
                            fetched,
                            || {
                                if command["observeStore"] == true {
                                    events.borrow_mut().push(Event {
                                        kind: "storeRead".into(),
                                        keys: keys(&cache),
                                        orders: Vec::new(),
                                    });
                                }
                                store_state.borrow().clone()
                            },
                            now,
                        )
                    }));
                    assert!(preserved.is_ok(), "CACHE native read-only store reentry");
                    let actual = preserved.unwrap();
                    let wanted = expected["orders"].as_array().unwrap();
                    assert_eq!(actual.len(), wanted.len(), "CACHE native preserved count");
                    for (actual, wanted) in actual.iter().zip(wanted) {
                        order(&actual.borrow(), &wanted["value"]);
                        assert!(
                            Rc::ptr_eq(actual, &registry[wanted["ref"].as_str().unwrap()]),
                            "CACHE native preserved reference"
                        );
                    }
                    if expected["sameArray"] == true && original_len > 0 {
                        assert_eq!(
                            actual.as_ptr(),
                            original_ptr,
                            "CACHE native unchanged fetched allocation"
                        );
                    }
                    Ok(())
                }
                _ => panic!("unknown cache command"),
            };
            if let Some(name) = expected["error"].as_str() {
                let error = result.unwrap_err();
                assert_eq!(error.name, name, "CACHE native host error name");
                assert_eq!(
                    error.message,
                    expected["message"].as_str().unwrap(),
                    "CACHE native host error message"
                );
                assert_eq!(expected["marker"], 41);
                assert_eq!(error, host_error, "CACHE native retained host error");
            } else {
                result.unwrap();
            }
            let actual = cache.snapshot();
            let wanted = step["cache"].as_array().unwrap();
            assert_eq!(actual.len(), wanted.len(), "CACHE native cache count");
            for ((key, placed_at, value), wanted) in actual.iter().zip(wanted) {
                assert_eq!(
                    key,
                    wanted["key"].as_str().unwrap(),
                    "CACHE native ordered key"
                );
                number(*placed_at, &wanted["placedAt"], "placedAt");
                order(&value.borrow(), &wanted["order"]);
                assert!(
                    Rc::ptr_eq(value, &registry[wanted["ref"].as_str().unwrap()]),
                    "CACHE native cache reference"
                );
            }
            assert_eq!(
                *events.borrow(),
                serde_json::from_value::<Vec<Event>>(step["events"].clone()).unwrap(),
                "CACHE native host call sequence"
            );
        }
    }
    assert_eq!(observations, 98, "CACHE independent observation floor");
}
