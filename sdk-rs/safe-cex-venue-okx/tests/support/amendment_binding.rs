#[allow(dead_code)]
#[path = "binding_harness.rs"]
mod harness;
use harness::*;
use safe_cex_exchange::js_runtime::value::{Js, from_json};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::ExchangeName;
use serde_json::json;

#[test]
fn amendments_use_inherited_update_fields() {
    check_amendment_properties(false);
}

#[test]
fn amendments_reject_primitive_updates_before_http() {
    check_amendment_properties(true);
}

fn check_amendment_properties(primitive: bool) {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/amendment-properties-source.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(expected.len(), 8);
    let mut checked = 0;
    for case in expected {
        if (case["shape"] == "primitive") != primitive {
            continue;
        }
        checked += 1;
        let h = harness(ExchangeName::Okx, options(), |http| {
            for path in ["/api/v5/trade/amend-order", "/api/v5/trade/amend-algos"] {
                http.ok("POST", path, json!([{"sCode":"0"}]));
            }
        });
        Store::update(
            &*h.store,
            serde_json::from_value::<StoreUpdate>(json!({"markets":[{
                "id":"BTC-USDT-SWAP","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,
                "precision":{"price":0.25,"amount":0.003},
                "limits":{"amount":{"min":0.003,"max":100},"leverage":{"min":1,"max":50}}
            }]}))
            .unwrap(),
            false,
        )
        .unwrap();
        let update = if case["shape"] == "primitive" {
            Js::Number(7.0)
        } else {
            let update = Js::object(vec![]);
            let prototype = if case["shape"] == "absent" {
                Js::Null
            } else {
                Js::object(vec![
                    ("price", Js::Number(104.37)),
                    ("amount", Js::Number(7.9)),
                ])
            };
            if let Js::Object(data) = &update {
                *data.proto.borrow_mut() = Some(prototype);
            }
            if case["shape"] == "shadowed" {
                update.define("price", Js::Number(102.625)).unwrap();
                update.define("amount", Js::Number(4.1)).unwrap();
            }
            update
        };
        let order = from_json(
            &json!({"id":"order-1","symbol":"BTCUSDT","type":case["family"],
            "amount":3,"price":100,"triggerType":"mark_price"}),
        );
        let result: Result<Vec<String>, _> = h.drive(h.exchange.invoke(
            "updateOrder",
            vec![Js::object(vec![("order", order), ("update", update)])],
        ));
        if let Some(error) = case.get("error") {
            let caught = result.expect_err("primitive update must reject before HTTP");
            assert_eq!(Some(caught.name.as_str()), error["name"].as_str());
            assert_eq!(Some(caught.message.as_str()), error["message"].as_str());
        } else {
            assert_eq!(serde_json::to_value(result.unwrap()).unwrap(), case["ids"]);
        }
        let requests = h.http.requests.borrow();
        let wanted = case["requests"].as_array().unwrap();
        assert_eq!(requests.len(), wanted.len(), "{case}");
        for (index, wanted_request) in wanted.iter().enumerate() {
            assert_eq!(requests[index].url, wanted_request["url"].as_str().unwrap());
            assert_eq!(
                requests[index].data, wanted_request["data"],
                "amendment payload follows source inherited property lookup: {case}"
            );
        }
    }
    assert_eq!(checked, if primitive { 2 } else { 6 });
}
