//! BingX registry construction and signed lifecycle over real native local HTTP/WS.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_types::{ExchangeName, PlaceOrderOpts};
use serde_json::{Value, json};
use std::rc::Rc;
use support::*;

fn routes(request: &Seen) -> Option<Value> {
    let data = match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/openApi/swap/v2/quote/contracts") => {
            json!([{"symbol":"BTC-USDT","asset":"BTC","currency":"USDT","status":1,"quantityPrecision":3,"pricePrecision":2,"tradeMinLimit":0.01,"maxLongLeverage":50}])
        }
        ("GET", "/openApi/swap/v2/quote/ticker") => {
            json!([{"symbol":"BTC-USDT","lastPrice":"100","priceChangePercent":"1","volume":"10","quoteVolume":"1000"}])
        }
        ("GET", "/openApi/swap/v2/quote/premiumIndex") => {
            json!([{"symbol":"BTC-USDT","markPrice":"101","indexPrice":"100","lastFundingRate":"0.01","nextFundingTime":1700000100000_f64}])
        }
        ("GET", "/openApi/swap/v3/user/balance") => {
            json!([{"asset":"USDT","userId":"native-bingx","balance":"10","availableMargin":"8","usedMargin":"2","unrealizedProfit":"1"}])
        }
        ("GET", "/openApi/swap/v1/positionSide/dual") => json!({"dualSidePosition":"false"}),
        ("GET", "/openApi/swap/v2/user/positions") => json!([]),
        ("GET", "/openApi/swap/v2/trade/openOrders") => json!([]),
        ("POST", "/openApi/user/auth/userDataStream") => {
            return Some(json!({"listenKey":"bingx-native-key"}));
        }
        ("POST", "/openApi/swap/v2/trade/order") => {
            json!({"order":{"orderId":9007199254740993_u64,"symbol":"BTC-USDT","side":"BUY","positionSide":"BOTH","type":"LIMIT","status":"NEW","origQty":"1","price":"100","stopPrice":"0","executedQty":"0","timeInForce":"GTC","reduceOnly":false}})
        }
        ("DELETE", "/openApi/swap/v2/trade/order") => json!({}),
        _ => return None,
    };
    Some(json!({"code":0,"data":data}))
}
#[test]
fn bingx_native_registry_bootstraps_signs_places_and_cancels() {
    run_local(async {
        let h = venue_harness(
            ExchangeName::Bingx,
            Rc::new(routes),
            json!({"key":"native-key","secret":SECRET,"tealstreetTunnel":false}),
            None,
            safe_cex_exchange::native::native_runtime(),
        )
        .await;
        h.exchange.start().await.expect("BingX native start");
        let loaded = h.store.loaded().clone();
        assert!(
            loaded.markets && loaded.tickers && loaded.balance && loaded.orders && loaded.positions
        );
        eventually("BingX listen-key socket", || {
            h.peers
                .borrow()
                .iter()
                .any(|p| p.path.contains("bingx-native-key"))
        })
        .await;
        assert_eq!(
            h.exchange.get_account().await.unwrap().user_id,
            "native-bingx"
        );
        let opts:PlaceOrderOpts=serde_json::from_value(json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":100,"clientOrderId":"tsnativecaller"})).unwrap();
        let ids = h
            .exchange
            .base()
            .place_order(h.exchange.as_ref(), opts)
            .await
            .unwrap();
        assert_eq!(ids, ["9007199254740993"]);
        let orders = h.store.orders().to_vec();
        assert_eq!(orders.len(), 1);
        h.exchange.cancel_orders(orders).await.unwrap();
        let seen = h.seen.borrow();
        let placed = seen
            .iter()
            .find(|r| r.method == "POST" && r.path == "/openApi/swap/v2/trade/order")
            .unwrap();
        let mut body: Value = serde_json::from_str(&placed.body).unwrap();
        let signature = body
            .as_object_mut()
            .unwrap()
            .remove("signature")
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned();
        let query = body
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| {
                format!(
                    "{k}={}",
                    v.as_str().map_or_else(|| v.to_string(), str::to_owned)
                )
            })
            .collect::<Vec<_>>()
            .join("&");
        use hmac::{Hmac, Mac};
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(SECRET.as_bytes()).unwrap();
        mac.update(query.as_bytes());
        let expected = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        assert_eq!(signature, expected, "native HMAC matches the wire payload");
        assert_eq!(body["clientOrderId"], "tsnativecaller");
        assert_eq!(
            placed.headers.get("x-source-key").map(String::as_str),
            Some("Tealstreet")
        );
        assert!(seen.iter().any(|r| r.method == "DELETE"
            && r.path == "/openApi/swap/v2/trade/order"
            && r.query.contains("9007199254740993")));
        drop(seen);
        h.exchange.dispose().unwrap();
    });
}
