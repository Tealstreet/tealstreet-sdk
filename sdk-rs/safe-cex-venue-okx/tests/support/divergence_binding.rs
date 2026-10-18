//! FIX-audit probes against the constructed production receiver, not mock bodies.
use super::binding_harness::*;
use safe_cex_exchange::js_runtime::value::{Js, from_json, from_retained_input};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{ExchangeName, PlaceOrderOpts};
use serde_json::{Value, json};

const NORMAL: &str = "/api/v5/trade/batch-orders";
const ALGO: &str = "/api/v5/trade/order-algo";

fn names() -> [ExchangeName; 4] {
    [
        ExchangeName::Okx,
        ExchangeName::Okxdemo,
        ExchangeName::Okxeu,
        ExchangeName::Okxeudemo,
    ]
}

fn placement_scene(name: ExchangeName, algo: bool, rows: Value) -> Harness {
    let h = harness(name, options(), |http| {
        http.ok(
            "GET",
            "/api/v5/account/config",
            json!([{"posMode":"net_mode","acctLv":"2"}]),
        );
        http.ok("POST", if algo { ALGO } else { NORMAL }, rows);
    });
    Store::update(
        &*h.store,
        serde_json::from_value::<StoreUpdate>(json!({"markets":[{
            "id":"BTC-USDT-SWAP","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,
            "precision":{"price":0.1,"amount":0.01},
            "limits":{"amount":{"min":0.01,"max":100},"leverage":{"min":1,"max":50}},
            "info":{"ctVal":"0.01","ctValCcy":"BTC","settleCcy":"USDT"}
        }]}))
        .unwrap(),
        false,
    )
    .unwrap();
    h
}

fn public_numeric_id(algo: bool) {
    for name in names() {
        let key = if algo { "algoId" } else { "ordId" };
        let h = placement_scene(name, algo, json!([{key: 9007199254740990_i64}]));
        let errors = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = errors.clone();
        h.venue().base().on(
            "error",
            std::rc::Rc::new(move |_, data| {
                sink.borrow_mut().push(data.to_display());
                Ok(())
            }),
        );
        let opts: PlaceOrderOpts = serde_json::from_value(json!({
            "symbol":"BTCUSDT", "type":if algo {"stop_market"} else {"limit"},
            "side":"buy", "price":50000, "amount":1
        }))
        .unwrap();
        let result = h.drive(Box::pin(h.venue().base().place_order(h.venue(), opts)));
        assert_eq!(
            result.unwrap_or_else(|e| panic!("live placement ID must survive: {}", e.message)),
            ["9007199254740990"],
            "OKX live numeric placement ID"
        );
        assert_eq!(h.http.calls(if algo { ALGO } else { NORMAL }).len(), 1);
        assert!(h.runtime.reports.borrow().is_empty());
        assert!(
            errors.borrow().is_empty(),
            "successful live ID must not emit an error"
        );
        h.venue().dispose().unwrap();
    }
}

#[test]
fn real_normal_placement_keeps_numeric_live_order_id() {
    public_numeric_id(false);
}

#[test]
fn real_algo_placement_keeps_numeric_live_order_id() {
    public_numeric_id(true);
}

#[test]
fn real_batch_members_coerce_ids_and_keep_later_errors() {
    for algo in [false, true] {
        let path = if algo { ALGO } else { NORMAL };
        let member = if algo {
            "placeAlgoOrderBatch"
        } else {
            "placeOrderBatch"
        };
        let key = if algo { "algoId" } else { "ordId" };
        let h = placement_scene(
            ExchangeName::Okx,
            algo,
            json!([{key:17},{key:true},{key:["native"]},{key:{}},{key:"string-id"}]),
        );
        let payloads = vec![json!({}); if algo { 2 } else { 21 }];
        let result: Vec<String> = h
            .drive(h.exchange.invoke(member, vec![from_json(&json!(payloads))]))
            .unwrap();
        assert_eq!(
            result,
            ["17", "true", "native", "[object Object]", "string-id"].repeat(2),
            "OKX JS String output ID values and order"
        );
        assert_eq!(h.http.calls(path).len(), 2, "all serial requests complete");
        h.venue().dispose().unwrap();

        for bad in [
            json!({"sMsg":"missing ID"}),
            json!({key:0,"sMsg":"zero ID"}),
            Value::Null,
        ] {
            let h = placement_scene(ExchangeName::Okx, algo, json!([{key:17},bad]));
            let error = h
                .drive(
                    h.exchange
                        .invoke::<Vec<String>>(member, vec![from_json(&json!([{}]))]),
                )
                .unwrap_err();
            assert!(
                error.message == "missing ID"
                    || error.message == "zero ID"
                    || error.message.contains("Cannot read properties of null"),
                "later source error wins: {}",
                error.message
            );
            assert_eq!(h.http.calls(path).len(), 1);
            h.venue().dispose().unwrap();
        }
    }
}

#[test]
fn real_private_finite_members_keep_wire_values_and_bits() {
    let cases = [
        ("null", 0.0_f64),
        ("false", 0.0),
        ("\"12.5tail\"", 12.5),
        ("\"-0\"", -0.0),
        ("[\"4.25\",null]", 4.25),
        ("{}", 0.0),
        ("1e400", 0.0),
        ("-1e-400", 0.0),
    ];
    for name in names() {
        let h = harness(name, options(), |_| {});
        let socket = h.exchange.scope.this.get("privateWebsocket").unwrap();
        for (wire, expected) in cases {
            let value = from_retained_input(safe_cex_transport::how::json_parse(wire));
            let parsed = socket
                .call_method("parseFinite", vec![value])
                .unwrap()
                .to_number()
                .unwrap();
            assert_eq!(
                parsed.to_bits(),
                expected.to_bits(),
                "OKX real parseFinite wire {wire}"
            );
        }
        let values = from_retained_input(safe_cex_transport::how::json_parse(
            "[\"4.25\",[2,3],{},\"Infinity\"]",
        ));
        let sum = socket
            .call_method("sumFinite", vec![values])
            .unwrap()
            .to_number()
            .unwrap();
        assert_eq!(sum.to_bits(), 6.25_f64.to_bits(), "OKX real sumFinite wire");
        let bad = from_json(&json!({"toString":null}));
        assert!(
            socket
                .call_method("parseFinite", vec![bad.clone()])
                .is_err()
        );
        assert!(
            socket
                .call_method("sumFinite", vec![Js::array(vec![bad])])
                .is_err()
        );
        h.venue().dispose().unwrap();
    }
}

#[test]
fn real_constructor_preserves_typed_options_and_socket_receivers() {
    for name in names() {
        let mut opts = options();
        opts.testnet = Some(false);
        let h = harness(name.clone(), opts, |_| {});
        let this = &h.exchange.scope.this;
        let eu = matches!(name, ExchangeName::Okxeu | ExchangeName::Okxeudemo);
        let demo = matches!(name, ExchangeName::Okxdemo | ExchangeName::Okxeudemo);
        assert!(this.get("region").unwrap().strict_equals(&Js::str(if eu {
            "okxeu"
        } else {
            "okx"
        })));
        assert_eq!(
            this.get("options")
                .unwrap()
                .get("testnet")
                .unwrap()
                .truthy(),
            demo
        );
        assert_eq!(this.get("isDemo").unwrap().truthy(), demo);
        for field in ["publicWebsocket", "privateWebsocket", "businessWebsocket"] {
            assert!(
                this.get(field)
                    .unwrap()
                    .get("parent")
                    .unwrap()
                    .strict_equals(this),
                "original parent receiver {field}"
            );
        }
        assert!(h.http.requests.borrow().is_empty());
        h.venue().dispose().unwrap();
    }
}
