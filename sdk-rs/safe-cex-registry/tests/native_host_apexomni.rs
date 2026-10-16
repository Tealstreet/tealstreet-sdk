//! Rust registry -> real Apex binding -> native loopback HTTP/WS and signer.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_types::{ExchangeName, PlaceOrderOpts};
use serde_json::{Value, json};
use std::rc::Rc;
use support::*;

fn routes(request: &Seen) -> Option<Value> {
    let path = request.path.strip_prefix("/api").unwrap_or(&request.path);
    let data = match (request.method.as_str(), path) {
        ("GET", "/v3/symbols") => {
            json!({"contractConfig":{"perpetualContract":[{"symbol":"BTC-USDC","crossSymbolName":"BTCUSDC","baseTokenId":"BTC","settleAssetId":"USDC","l2PairId":"11","enableTrade":true,"stepSize":"0.001","tickSize":"0.01","minOrderSize":"0.001","maxOrderSize":"1000","displayMinLeverage":"1","displayMaxLeverage":"50","initialMarginRate":"0.1"}]}})
        }
        ("GET", "/v3/account") => {
            json!({"id":"16909060","positions":[],"l2key":"native-l2","contractAccount":{"makerFeeRate":"0.0012","takerFeeRate":"0.0013"}})
        }
        ("GET", "/v3/account-balance") => {
            json!({"totalEquityValue":"100","maintenanceMargin":"2","initialMargin":"10"})
        }
        ("GET", "/v3/open-orders") => json!([]),
        ("GET", "/v3/user") => json!({"id":"16909060"}),
        ("POST", "/v3/order") => json!({"id":"9007199254740993"}),
        ("POST", "/v3/delete-order") => json!({}),
        _ => return None,
    };
    Some(json!({"code":0,"data":data}))
}
fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in bytes.chunks(3) {
        let word = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        result.push(TABLE[((word >> 18) & 63) as usize] as char);
        result.push(TABLE[((word >> 12) & 63) as usize] as char);
        result.push(if chunk.len() > 1 {
            TABLE[((word >> 6) & 63) as usize] as char
        } else {
            '='
        });
        result.push(if chunk.len() > 2 {
            TABLE[(word & 63) as usize] as char
        } else {
            '='
        });
    }
    result
}
#[test]
fn apexomni_native_registry_bootstraps_signs_places_and_cancels() {
    run_local(async {
        let h=venue_harness(ExchangeName::Apexomni,Rc::new(routes),json!({"key":"native-key","secret":SECRET,"passphrase":"native-passphrase","omniKeySeed":format!("0x{}1b","01".repeat(64)),"tealstreetTunnel":false}),None,safe_cex_exchange::native::native_runtime()).await;
        h.exchange.start().await.expect("Apex native start");
        let loaded = h.store.loaded().clone();
        assert!(
            loaded.markets && loaded.tickers && loaded.balance && loaded.orders && loaded.positions
        );
        eventually("Apex native private login", || {
            h.received
                .borrow()
                .iter()
                .any(|frame| frame.contains("\"op\":\"login\""))
        })
        .await;
        assert_eq!(h.exchange.get_account().await.unwrap().user_id, "16909060");
        let opts: PlaceOrderOpts=serde_json::from_value(json!({"symbol":"BTCUSDC","type":"limit","side":"buy","amount":1.2345,"price":100.127,"clientOrderId":"tsnativecaller"})).unwrap();
        assert_eq!(
            h.exchange
                .base()
                .place_order(h.exchange.as_ref(), opts)
                .await
                .unwrap(),
            ["9007199254740993"]
        );
        let order = json!({"id":"9007199254740993","symbol":"BTC-USDC","type":"LIMIT","side":"BUY","status":"OPEN","timeInForce":"GOOD_TIL_CANCEL","price":"100.13","size":"1.235","cumSuccessFillSize":"0","reduceOnly":false,"triggerPrice":"0","updatedAt":0,"openSlParams":{"triggerPrice":"0"},"openTpParams":{"triggerPrice":"0"}});
        let peer = h
            .peers
            .borrow()
            .iter()
            .find(|p| p.path.contains("private"))
            .unwrap()
            .push
            .clone();
        let received_order = Rc::new(std::cell::RefCell::new(None));
        let observed = received_order.clone();
        let cleanup = h.store.subscribe(Box::new(move |data, _| {
            if let Some(order) = data.orders.iter().find(|o| o.id == "9007199254740993") {
                *observed.borrow_mut() = Some(order.clone());
            }
            Ok(())
        }));
        peer.send(json!({"topic":"ws_zk_accounts_v3","contents":{"orders":[order]}}).to_string())
            .unwrap();
        eventually("Apex placed order arrives through private WS", || {
            received_order.borrow().is_some()
        })
        .await;
        let order = received_order
            .borrow()
            .clone()
            .expect("private WS delivered order");
        cleanup();
        h.exchange.cancel_orders(vec![order]).await.unwrap();
        let seen = h.seen.borrow();
        let placed = seen
            .iter()
            .find(|r| r.method == "POST" && r.path == "/api/v3/order")
            .unwrap();
        assert!(placed.body.contains("clientOrderId=tsnativecaller"));
        assert!(placed.body.contains("size=1.235"));
        assert!(placed.body.contains("price=100.13"));
        let signature = placed
            .body
            .split('&')
            .find_map(|v| v.strip_prefix("signature="))
            .unwrap();
        assert_eq!(signature.len(), 128);
        let timestamp = placed.headers.get("apex-timestamp").unwrap();
        let message = format!("{timestamp}POST/api/v3/order{}", placed.body);
        use hmac::{Hmac, Mac};
        let mut mac =
            Hmac::<sha2::Sha256>::new_from_slice(base64(SECRET.as_bytes()).as_bytes()).unwrap();
        mac.update(message.as_bytes());
        assert_eq!(
            placed.headers.get("apex-signature").unwrap(),
            &base64(&mac.finalize().into_bytes()),
            "HMAC authenticates actual transmitted form bytes"
        );
        assert!(seen.iter().any(|r| r.method == "POST"
            && r.path == "/api/v3/delete-order"
            && r.body.contains("9007199254740993")));
        drop(seen);
        h.exchange.dispose().unwrap();
    });
}
