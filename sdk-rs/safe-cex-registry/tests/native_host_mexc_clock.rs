//! MEXC signing must use the same server-synced clock as its TypeScript source.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use hmac::{Hmac, Mac};
use safe_cex_store::virtual_clock::VIRTUAL_CLOCK;
use safe_cex_types::{ExchangeName, PlaceOrderOpts};
use serde_json::{Value, json};
use std::rc::Rc;

struct ResetClock;
impl Drop for ResetClock {
    fn drop(&mut self) {
        VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)());
    }
}
fn signature(message: &str) -> String {
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(support::SECRET.as_bytes()).unwrap();
    mac.update(message.as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn response(request: &support::Seen) -> Option<Value> {
    let data = match request.path.as_str() {
        "/api/v1/private/account/assets" => {
            json!([{"currency":"USDT","availableBalance":81,"equity":123,"unrealized":5,"frozenBalance":0}])
        }
        "/api/v1/contract/detail" => json!([{
            "symbol":"BTC_USDT", "baseCoin":"BTC", "quoteCoin":"USDT", "settleCoin":"USDT",
            "contractSize":0.0001, "priceUnit":0.1, "volUnit":1, "minVol":1, "maxVol":1000000,
            "minLeverage":1, "maxLeverage":125, "state":0, "apiAllowed":true,
            "makerFeeRate":0.0002, "takerFeeRate":0.0006
        }]),
        "/api/v1/private/order/submit" => json!({"orderId":"9007001"}),
        "/api/v1/private/position/position_mode" => json!(1),
        "/api/v1/private/order/list/open_orders"
        | "/api/v1/private/planorder/list/orders"
        | "/api/v1/private/stoporder/list/orders" => json!({"resultList":[],"totalCount":0}),
        _ => json!([]),
    };
    Some(json!({"success":true,"code":0,"data":data}))
}
#[test]
fn native_mexc_rest_and_private_login_use_the_server_synced_clock() {
    support::run_local(async {
        VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)() + 90_000.0);
        let _reset = ResetClock;
        let before = VIRTUAL_CLOCK.get_current_time_ms();
        let h = support::venue_harness(
            ExchangeName::Mexc,
            Rc::new(response),
            json!({"key":"native-key","secret":support::SECRET}),
            None,
            safe_cex_exchange::native::native_runtime(),
        )
        .await;
        h.exchange
            .get_account()
            .await
            .expect("signed REST validation");
        h.exchange.start().await.expect("start private socket");
        support::eventually("MEXC login", || {
            h.received.borrow().iter().any(|text| {
                serde_json::from_str::<Value>(text).is_ok_and(|f| f["method"] == "login")
            })
        })
        .await;
        let opts: PlaceOrderOpts = serde_json::from_value(json!({
            "symbol":"BTCUSDT", "type":"limit", "side":"buy", "amount":3, "price":59500
        }))
        .unwrap();
        let ids = h
            .exchange
            .base()
            .place_order(h.exchange.as_ref(), opts)
            .await
            .unwrap();
        assert_eq!(ids, ["9007001"]);
        let placed_at = h
            .exchange
            .base()
            .store
            .orders()
            .unwrap()
            .iter()
            .find(|order| order.id == "9007001")
            .expect("production optimistic order")
            .last_update;
        let after = VIRTUAL_CLOCK.get_current_time_ms();
        let request = h
            .seen
            .borrow()
            .iter()
            .find(|r| r.path == "/api/v1/private/account/assets")
            .expect("private REST request")
            .clone();
        let login: Value = h
            .received
            .borrow()
            .iter()
            .filter_map(|text| serde_json::from_str::<Value>(text).ok())
            .find(|f| f["method"] == "login")
            .unwrap();
        let rest_time = request.headers["request-time"].parse::<f64>().unwrap();
        let ws_time = login["param"]["reqTime"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap();
        assert_eq!(
            request.headers["signature"],
            signature(&format!("native-key{}", request.headers["request-time"]))
        );
        assert_eq!(
            login["param"]["signature"],
            signature(&format!(
                "native-key{}",
                login["param"]["reqTime"].as_str().unwrap()
            ))
        );
        h.exchange.dispose().unwrap();
        for path in [
            "/api/v1/private/planorder/list/orders",
            "/api/v1/private/stoporder/list/orders",
        ] {
            let seen = h.seen.borrow();
            let request = seen
                .iter()
                .find(|request| request.path == path)
                .expect("stop-order read");
            let end = support::query_value(&request.query, "end_time")
                .unwrap()
                .parse::<f64>()
                .unwrap();
            assert!(
                end >= before && end <= after,
                "MEXC stop-order window uses synced time: path={path}, before={before}, after={after}, end={end}"
            );
        }
        assert!(
            placed_at >= before && placed_at <= after,
            "MEXC optimistic placement uses synced time: before={before}, after={after}, placed={placed_at}"
        );
        assert_eq!(
            (
                rest_time >= before && rest_time <= after,
                ws_time >= before && ws_time <= after
            ),
            (true, true),
            "MEXC REST and WS use synced time: before={before}, after={after}, REST={rest_time}, WS={ws_time}"
        );
    });
}
