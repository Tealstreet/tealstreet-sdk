#![cfg(not(target_arch = "wasm32"))]
#[path = "support/native_account.rs"]
mod account;
mod support;
use account::{options, run, started};
use serde_json::{Value, json};

#[test]
fn mints_the_venues_honorable_client_id() {
    run(async {
        let h = started().await;
        let id = h.ok("mintClientOrderId", json!([options()])).await;
        let id = id
            .as_str()
            .expect("a loaded Binance market supports minting");
        assert!(id.starts_with("x-s3vFFRQp"));
        assert!(id.len() <= "x-s3vFFRQp".len() + 21);
        assert!(
            id.strip_prefix("x-s3vFFRQp")
                .unwrap()
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric())
        );
        h.ok("dispose", json!([])).await;
    });
}
#[test]
fn places_the_exact_client_id_and_returns_the_ts_reference() {
    run(async {
        let h = started().await;
        let mut order = options();
        order["clientOrderId"] = json!("caller-native-77");
        assert_eq!(
            h.ok("placeOrderWithClientOrderId", json!([order])).await,
            json!({"orderIds":["9001"],"clientOrderId":"caller-native-77"})
        );
        {
            let seen = h.seen.borrow();
            let request = seen
                .iter()
                .find(|request| request.method == "POST")
                .unwrap();
            assert!(
                request.query.contains("newClientOrderId=caller-native-77"),
                "{request:?}"
            );
        }
        let error = h
            .call("placeOrderWithClientOrderId", json!([options()]))
            .await
            .unwrap_err();
        assert!(
            error["message"]
                .as_str()
                .unwrap()
                .contains("requires a clientOrderId")
        );
        assert_eq!(
            h.seen
                .borrow()
                .iter()
                .filter(|request| request.method == "POST")
                .count(),
            1
        );
        h.ok("dispose", json!([])).await;
    });
}
#[test]
fn existing_placement_generates_the_ts_nanoid_format() {
    run(async {
        let h = started().await;
        h.ok("placeOrder", json!([options()])).await;
        {
            let seen = h.seen.borrow();
            let request = seen
                .iter()
                .find(|request| request.method == "POST")
                .unwrap();
            let id = support::query_value(&request.query, "newClientOrderId").unwrap();
            assert!(id.starts_with("x-s3vFFRQp"));
            assert!(
                id.len() <= "x-s3vFFRQp".len() + 21,
                "TS prefixes a <=21-character filtered nanoid, got {id}"
            );
            assert!(
                id.strip_prefix("x-s3vFFRQp")
                    .unwrap()
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric())
            );
        }
        h.ok("dispose", json!([])).await;
    });
}

macro_rules! unsupported_option_method {
    ($test:ident, $method:literal, $args:expr, $message:literal) => {
        #[test]
        fn $test() {
            run(async {
                let h = started().await;
                let error = h.call($method, $args).await.unwrap_err();
                assert_eq!(error["message"], $message);
                h.ok("dispose", json!([])).await;
            });
        }
    };
}
unsupported_option_method!(
    option_chains_reach_the_venue,
    "fetchOptionChain",
    json!([{"underlying":"BTC"}]),
    "Option chains are not supported on binance"
);
unsupported_option_method!(
    option_expiries_reach_the_venue,
    "fetchOptionExpiries",
    json!(["BTC"]),
    "Option expiries are not supported on binance"
);
unsupported_option_method!(
    account_greeks_reach_the_venue,
    "fetchAccountGreeks",
    json!([]),
    "Account greeks are not supported on binance"
);

macro_rules! nuke_orders_method {
    ($test:ident, $method:literal) => {
        #[test]
        fn $test() {
            run(async {
                let h = started().await;
                h.ok("fetchAndSetOrders", json!([null])).await;
                assert_eq!(h.ok($method, json!([])).await, Value::Null);
                {
                    let requests = h.seen.borrow();
                    for endpoint in ["/fapi/v1/allOpenOrders", "/fapi/v1/algoOpenOrders"] {
                        assert_eq!(
                            requests
                                .iter()
                                .filter(|r| r.method == "DELETE" && r.path.ends_with(endpoint))
                                .count(),
                            1
                        );
                    }
                }
                h.ok("dispose", json!([])).await;
            });
        }
    };
}
nuke_orders_method!(nuke_cancels_native_orders, "nuke");
nuke_orders_method!(nuke_orders_cancels_native_orders, "nukeOrders");
#[test]
fn nuke_positions_does_not_cancel_orders() {
    run(async {
        let h = started().await;
        h.ok("fetchAndSetOrders", json!([null])).await;
        assert_eq!(h.ok("nukePositions", json!([])).await, Value::Null);
        assert!(h.seen.borrow().iter().all(|r| r.method == "GET"));
        h.ok("dispose", json!([])).await;
    });
}
