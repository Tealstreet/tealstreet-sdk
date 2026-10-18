//! BinancePMExchange and BinanceSpotExchange on the production native host, against
//! local HTTP + WebSocket servers. Constructed through the production Rust registry.
//! #133 centralizes construction in venue_harness; each proof supplies its registry name.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use hmac::{Hmac, Mac};
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use std::rc::Rc;
use support::*;

fn hmac_hex(payload: &str) -> String {
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(SECRET.as_bytes()).unwrap();
    mac.update(payload.as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn assert_signed(seen: &Seen) {
    let signature = query_value(&seen.query, "signature").expect("signed query");
    let signed = seen.query.split("&signature=").next().unwrap();
    assert_eq!(
        signature,
        hmac_hex(signed),
        "server-verified HMAC for {}",
        seen.path
    );
    assert_eq!(
        seen.headers.get("x-mbx-apikey").map(String::as_str),
        Some("native-key")
    );
}
fn table(routes: Vec<(&'static str, Value)>) -> Handler {
    Rc::new(move |request| {
        let key = format!("{} {}", request.method, request.path);
        routes
            .iter()
            .find(|(route, _)| *route == key)
            .map(|(_, body)| body.clone())
    })
}
fn limit_buy() -> safe_cex_types::PlaceOrderOpts {
    serde_json::from_value(
        json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":10}),
    )
    .unwrap()
}

#[test]
fn binance_pm_runs_on_the_native_host_over_local_http_and_websocket() {
    run_local(async {
        let position = json!({"symbol":"BTCUSDT","positionSide":"BOTH","positionAmt":"0","notional":"0","entryPrice":"0","markPrice":"10","unRealizedProfit":"0","liquidationPrice":"0","leverage":"5","isolatedMargin":"0","marginType":"cross","updateTime":1});
        let pm = table(vec![
            (
                "GET /papi/v1/um/leverageBracket",
                json!([{"symbol":"BTCUSDT","brackets":[{"initialLeverage":125}]}]),
            ),
            ("POST /papi/v1/listenKey", json!({"listenKey":"lk-pm"})),
            ("PUT /papi/v1/listenKey", json!({})),
            (
                "GET /papi/v1/um/positionSide/dual",
                json!({"dualSidePosition":false}),
            ),
            ("GET /papi/v1/um/positionRisk", json!([position])),
            (
                "GET /papi/v1/balance",
                json!([{"accountAlias":"alias-pm","asset":"USDT","totalWalletBalance":"100","umUnrealizedPNL":"0"}]),
            ),
            (
                "GET /papi/v1/account",
                json!({"accountEquity":"100","totalAvailableBalance":"60","actualEquity":"100"}),
            ),
            ("GET /papi/v1/um/openOrders", json!([])),
            ("GET /papi/v1/um/conditional/openOrders", json!([])),
            (
                "POST /papi/v1/um/order",
                json!({"orderId":9002,"clientOrderId":"native-9002"}),
            ),
        ]);
        let Harness {
            exchange,
            store,
            seen,
            peers,
            ..
        } = venue_harness(
            ExchangeName::Binancepm,
            pm,
            json!({"key":"native-key","secret":SECRET}),
            None,
            safe_cex_exchange::native::native_runtime(),
        )
        .await;

        exchange
            .start()
            .await
            .expect("PM start over HTTP and WebSocket");
        eventually("PM private stream dialed with the listen key", || {
            peers.borrow().iter().any(|p| p.path.contains("lk-pm"))
        })
        .await;
        assert!(store.loaded().unwrap().markets && store.loaded().unwrap().tickers);
        let ids = exchange
            .base()
            .place_order(exchange.as_ref(), limit_buy())
            .await
            .expect("PM placeOrder over HTTP");
        let placed = seen
            .borrow()
            .iter()
            .rev()
            .find(|s| s.path == "/papi/v1/um/order")
            .cloned()
            .expect("PM order reached PAPI");
        assert_eq!(placed.method, "POST");
        assert_signed(&placed);
        // PM's canonical order id is the client id it sent (safe-cex CLAUDE.md).
        assert_eq!(
            ids,
            vec![query_value(&placed.query, "newClientOrderId").unwrap()],
        );
        assert!(
            seen.borrow()
                .iter()
                .any(|s| s.path == "/papi/v1/um/positionRisk"),
            "PM private snapshot came from PAPI"
        );
        exchange.dispose().expect("dispose");
    });
}

#[test]
fn binance_spot_runs_on_the_native_host_over_local_http_and_websocket() {
    run_local(async {
        let spot = table(vec![
            (
                "GET /api/v3/exchangeInfo",
                json!({"symbols":[{"symbol":"BTCUSDT","baseAsset":"BTC","quoteAsset":"USDT","status":"TRADING","baseAssetPrecision":8,"quoteAssetPrecision":8,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.01"},{"filterType":"LOT_SIZE","stepSize":"0.001","minQty":"0.001","maxQty":"100"}]}]}),
            ),
            (
                "GET /api/v3/ticker/24hr",
                json!([{"symbol":"BTCUSDT","bidPrice":"10.1","askPrice":"10.9","lastPrice":"10.5","priceChangePercent":"1.5","volume":"100","quoteVolume":"1050"}]),
            ),
            (
                "GET /api/v3/account",
                json!({"balances":[{"asset":"USDT","free":"60","locked":"40"}]}),
            ),
            ("GET /api/v3/openOrders", json!([])),
            (
                "POST /api/v3/order",
                json!({"orderId":9003,"clientOrderId":"native-9003"}),
            ),
        ]);
        let Harness {
            exchange,
            store,
            seen,
            received,
            ..
        } = venue_harness(
            ExchangeName::Binancespot,
            spot,
            json!({"key":"native-key","secret":SECRET}),
            None,
            safe_cex_exchange::native::native_runtime(),
        )
        .await;

        exchange
            .start()
            .await
            .expect("Spot start over HTTP and WebSocket");
        assert!(store.loaded().unwrap().markets && store.loaded().unwrap().tickers);
        eventually(
            "Spot private socket sent its signed user-data subscription",
            || received.borrow().iter().any(|f| f.contains("signature")),
        )
        .await;
        let frame: Value = received
            .borrow()
            .iter()
            .find(|f| f.contains("signature"))
            .map(|f| serde_json::from_str(f).unwrap())
            .unwrap();
        let params = &frame["params"];
        let payload = format!(
            "apiKey={}&recvWindow={}&timestamp={}",
            params["apiKey"].as_str().unwrap(),
            params["recvWindow"],
            params["timestamp"]
        );
        assert_eq!(
            params["signature"].as_str(),
            Some(hmac_hex(&payload).as_str()),
            "ws user-data signature verified over its payload: {frame}"
        );

        let ids = exchange
            .base()
            .place_order(exchange.as_ref(), limit_buy())
            .await
            .expect("Spot placeOrder over HTTP");
        assert_eq!(ids.len(), 1);
        let placed = seen
            .borrow()
            .iter()
            .rev()
            .find(|s| s.path == "/api/v3/order")
            .cloned()
            .expect("Spot order reached /api/v3");
        assert_eq!(placed.method, "POST");
        assert_signed(&placed);
        exchange.dispose().expect("dispose");
    });
}
