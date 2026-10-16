mod support;
use safe_cex_types::Order;
use safe_cex_venue_mexc::{exchange_helpers::*, utils::normalize_order};
use serde_json::{Value, json};
use std::{cell::Cell, collections::BTreeMap};
fn order(value: &Value) -> Order {
    let mut order = normalize_order(
        json!({"id":"fixture","symbol":"BTC_USDT","side":1})
            .as_object()
            .unwrap(),
        None,
        &|v| v,
    )
    .unwrap()
    .unwrap();
    order.id = value["id"].as_str().unwrap().into();
    order.r#type = serde_json::from_value(value["type"].clone()).unwrap();
    order
}
#[test]
fn mexc_shields_live_typescript_state_transitions() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/shields_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let traces: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(traces.len(), 16);
    let mut count = 0;
    for trace in traces {
        let mut placed = BTreeMap::new();
        let mut canceled = BTreeMap::new();
        let id = trace["id"].as_str().unwrap();
        let mut rejected = BTreeMap::from([(
            id.split("__").next().unwrap().to_owned(),
            trace["start"].as_f64().unwrap(),
        )]);
        for step in trace["steps"].as_array().unwrap() {
            count += 1;
            let args = step["args"].as_array().unwrap();
            let now = support::decode_number(&step["time"]);
            let name = step["name"].as_str().unwrap();
            let clock_calls = Cell::new(0);
            let clock_value = Cell::new(now);
            let clock = || {
                clock_calls.set(clock_calls.get() + 1);
                let n = clock_value.get();
                clock_value.set(n + step["clockStep"].as_f64().unwrap());
                n
            };
            let mut output = json!({"$undefined":true});
            match name {
                "notePlacedStopLikeOrder" => {
                    note_placed_stop_like_order(&mut placed, args[0].as_str().unwrap(), &clock)
                }
                "markStopLikeOrderSeen" => {
                    mark_stop_like_order_seen(&mut placed, args[0].as_str().unwrap())
                }
                "isRecentlyPlacedStopLikeOrder" => {
                    output = json!(is_recently_placed_stop_like_order(
                        &mut placed,
                        &order(&args[0]),
                        &clock
                    ))
                }
                "noteRecentlyCanceledStopLikeOrders" => note_recently_canceled_stop_like_orders(
                    &mut canceled,
                    &mut placed,
                    &args[0]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(order)
                        .collect::<Vec<_>>(),
                    &clock,
                ),
                "isRecentlyCanceledStopLikeOrderId" => {
                    output = json!(is_recently_canceled_stop_like_order_id(
                        &mut canceled,
                        args[0].as_str().unwrap(),
                        &clock
                    ))
                }
                "isRecentlyCanceledStopLikeOrder" => {
                    output = json!(is_recently_canceled_stop_like_order(
                        &mut canceled,
                        &order(&args[0]),
                        &clock
                    ))
                }
                "isRecentlyRejectedRegularOrder" => {
                    output = json!(is_recently_rejected_regular_order(
                        &mut rejected,
                        args[0].as_str().unwrap(),
                        &clock
                    ))
                }
                "filterRecentlyCanceledStopLikeOrders" => {
                    output = json!(
                        filter_recently_canceled_stop_like_orders(
                            &mut canceled,
                            &args[0]
                                .as_array()
                                .unwrap()
                                .iter()
                                .map(order)
                                .collect::<Vec<_>>(),
                            &clock
                        )
                        .iter()
                        .map(|o| json!({"id":o.id,"type":o.r#type}))
                        .collect::<Vec<_>>()
                    )
                }
                _ => panic!("unhandled {name}"),
            }
            assert_eq!(
                json!(clock_calls.get()),
                step["clockCalls"],
                "clock calls {count}: {step}"
            );
            assert_eq!(output, step["output"], "shield output {count}: {step}");
            assert_eq!(
                support::encode(&BTreeMap::from([
                    ("placed", &placed),
                    ("canceled", &canceled),
                    ("rejected", &rejected)
                ])),
                support::encode(
                    &serde_json::from_value::<BTreeMap<String, BTreeMap<String, f64>>>(
                        step["state"].clone()
                    )
                    .unwrap()
                ),
                "shield state {count}: {step}"
            );
        }
    }
    assert_eq!(count, 304);
}
#[test]
fn mexc_shields_typescript_control_is_observed() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "mexc_shields_live_typescript_state_transitions",
            "--nocapture",
        ])
        .env("MEXC_TS_CONTROL", "shield-expiry")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{text}");
    assert!(
        text.contains("mexc_shields_live_typescript_state_transitions ... FAILED")
            && text.contains("isRecentlyPlacedStopLikeOrder")
            && text.contains("assertion `left == right` failed"),
        "{text}"
    );
}
