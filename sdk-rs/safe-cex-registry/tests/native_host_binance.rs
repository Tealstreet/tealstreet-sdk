//! BinanceExchange constructed by the registry on the production native host, against
//! local HTTP + WebSocket servers. Only venue hostnames are rewritten.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use hmac::{Hmac, Mac};
use serde_json::json;
use std::rc::Rc;
use support::*;

#[test]
fn binance_futures_runs_on_the_native_host_over_local_http_and_websocket() {
    run_local(async {
        let Harness {
            exchange,
            store,
            seen,
            peers,
            ..
        } = binance_harness(Rc::new(|_| None)).await;

        let markets = exchange
            .fetch_markets()
            .await
            .expect("fetchMarkets over HTTP");
        assert!(
            markets.iter().any(|m| m.symbol == "BTCUSDT"),
            "exchangeInfo parsed from the wire"
        );
        let validated = exchange
            .validate_account()
            .await
            .expect("validateAccount over HTTP");
        assert_eq!(validated.balance.as_ref().map(|b| b.total), Some(100.0));
        assert_eq!(
            exchange.get_account().await.unwrap().user_id,
            "alias-native"
        );

        exchange
            .start()
            .await
            .expect("start over HTTP and WebSocket");
        eventually("private stream dialed with the listen key", || {
            peers
                .borrow()
                .iter()
                .any(|p| p.path.starts_with("/private/ws") && p.path.contains("lk-native"))
        })
        .await;
        assert!(
            store.loaded().markets && store.loaded().tickers,
            "start loaded public state"
        );
        assert!(
            seen.borrow()
                .iter()
                .any(|s| s.method == "POST" && s.path == "/fapi/v1/listenKey")
        );

        let order: safe_cex_types::PlaceOrderOpts = serde_json::from_value(
            json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":10}),
        )
        .unwrap();
        let ids = exchange
            .base()
            .place_order(exchange.as_ref(), order)
            .await
            .expect("placeOrder over HTTP");
        assert_eq!(ids, vec!["9001"]);
        let placed = seen
            .borrow()
            .iter()
            .rev()
            .find(|s| s.path == "/fapi/v1/order")
            .cloned()
            .expect("order reached the server");
        assert_eq!(placed.method, "POST");
        assert_eq!(
            placed.headers.get("x-mbx-apikey").map(String::as_str),
            Some("native-key")
        );
        let signature = query_value(&placed.query, "signature").expect("signed query");
        let signed = placed.query.split("&signature=").next().unwrap();
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(SECRET.as_bytes()).unwrap();
        mac.update(signed.as_bytes());
        let expected: String = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(
            signature, expected,
            "server-verified HMAC over the exact bytes sent"
        );
        assert_eq!(query_value(&placed.query, "symbol"), Some("BTCUSDT"));

        let event = json!({"e":"ORDER_TRADE_UPDATE","E":1,"o":{"s":"BTCUSDT","c":"c1","S":"BUY","ot":"LIMIT","q":"1","p":"10","sp":"0","X":"NEW","i":555,"z":"0","R":false,"ap":"0","l":"0"}});
        let private = peers
            .borrow()
            .iter()
            .find(|p| p.path.starts_with("/private/ws") && p.path.contains("lk-native"))
            .unwrap()
            .push
            .clone();
        private.send(event.to_string()).unwrap();
        eventually("private ORDER_TRADE_UPDATE reached the store", || {
            store.orders().iter().any(|o| o.id == "555")
        })
        .await;

        let streamed = store
            .orders()
            .iter()
            .find(|o| o.id == "555")
            .cloned()
            .unwrap();
        exchange
            .cancel_orders(vec![streamed])
            .await
            .expect("cancelOrders over HTTP");
        let cancel = seen
            .borrow()
            .iter()
            .rev()
            .find(|s| s.method == "DELETE" && s.path == "/fapi/v1/order")
            .cloned()
            .expect("cancel reached the server");
        assert_eq!(
            query_value(&cancel.query, "symbol"),
            Some("BTCUSDT"),
            "DELETE carries its params"
        );
        assert_eq!(query_value(&cancel.query, "orderId"), Some("555"));
        exchange.dispose().expect("dispose");
    });
}

// ts: signatureRejectedResponse on a 400 and resendOnSignatureRejection on a 200,
// each re-signing once with a fresh timestamp.
#[test]
fn binance_futures_resends_a_signature_rejection_once_on_the_native_host() {
    for status in [Some(400), None] {
        run_local(async move {
            let calls = Rc::new(std::cell::Cell::new(0));
            let counter = calls.clone();
            let Harness { exchange, seen, .. } = binance_harness(Rc::new(move |request| {
                if request.method != "POST" || request.path != "/fapi/v1/order" {
                    return None;
                }
                counter.set(counter.get() + 1);
                let rejection =
                    json!({"code":-1022,"msg":"Signature for this request is not valid."});
                (counter.get() == 1).then(|| {
                    // Native Date.now has integer milliseconds. Make the second
                    // attempt's signing input distinct even on a fast loopback
                    // response, so the assertion measures re-signing.
                    std::thread::sleep(std::time::Duration::from_millis(2));
                    match status {
                        Some(status) => json!({"__status": status, "body": rejection}),
                        None => rejection,
                    }
                })
            }))
            .await;
            exchange
                .start()
                .await
                .expect("start over HTTP and WebSocket");
            let order: safe_cex_types::PlaceOrderOpts = serde_json::from_value(
                json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":10}),
            )
            .unwrap();
            let ids = exchange
                .base()
                .place_order(exchange.as_ref(), order)
                .await
                .expect("the resend succeeds");
            assert_eq!(ids, vec!["9001"], "{status:?}");
            assert_eq!(calls.get(), 2, "one resend after a {status:?} rejection");
            let stamps: Vec<_> = seen
                .borrow()
                .iter()
                .filter(|s| s.path == "/fapi/v1/order")
                .map(|s| query_value(&s.query, "signature").unwrap().to_owned())
                .collect();
            assert_eq!(stamps.len(), 2);
            assert_ne!(stamps[0], stamps[1], "the resend is signed afresh");
            exchange.dispose().expect("dispose");
        });
    }
}

fn signature_type_error(status: Option<u16>) {
    run_local(async move {
        let Harness { exchange, seen, .. } = binance_harness(Rc::new(move |request| {
            if request.method != "POST" || request.path != "/fapi/v1/order" {
                return None;
            }
            let data = json!({"code":{"toString":0}});
            Some(match status {
                Some(status) => json!({"__status":status,"body":data}),
                None => data,
            })
        }))
        .await;
        exchange.start().await.unwrap();
        let order: safe_cex_types::PlaceOrderOpts = serde_json::from_value(
            json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":10}),
        )
        .unwrap();
        let error = exchange
            .base()
            .place_order(exchange.as_ref(), order)
            .await
            .unwrap_err();
        assert_eq!(
            error.name, "TypeError",
            "BINANCE-SIGNATURE-TYPEERROR actual native order rejection retains coercion kind for {status:?}"
        );
        assert_eq!(error.message, "Cannot convert object to primitive value");
        assert_eq!(
            seen.borrow()
                .iter()
                .filter(|request| request.path == "/fapi/v1/order")
                .count(),
            1,
            "BINANCE-SIGNATURE-TYPEERROR coercion failure must not resend"
        );
        exchange.dispose().unwrap();
    });
}
#[test]
fn resolved_signature_type_error_reaches_the_native_binance_exchange() {
    signature_type_error(None);
}
#[test]
fn rejected_signature_type_error_reaches_the_native_binance_exchange() {
    signature_type_error(Some(400));
}
