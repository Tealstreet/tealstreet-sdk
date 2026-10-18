//! Spot registry construction, local native HTTP/WS, and real action signing.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_types::{ExchangeName, PlaceOrderOpts};
use serde_json::{Value, json};
use std::rc::Rc;
use support::*;
const KEY: &str = "0x0123456789012345678901234567890123456789012345678901234567890123";
const ADDRESS: &str = "0x14791697260e4c9a71f18484c9f997b308e59325";
fn routes(request: &Seen) -> Option<Value> {
    let body: Value = serde_json::from_str(&request.body).ok()?;
    match (request.method.as_str(), request.path.as_str()) {
        ("POST", "/info") => match body["type"].as_str()? {
            "spotMetaAndAssetCtxs" => Some(json!([
                {"tokens":[{"name":"USDC","szDecimals":6,"weiDecimals":8,"index":0},{"name":"PURR","szDecimals":2,"weiDecimals":8,"index":1}],"universe":[{"name":"@0","tokens":[1,0],"index":0,"isCanonical":true}]},
                [{"coin":"@0","midPx":"1.2","markPx":"1.25","prevDayPx":"1.0","dayNtlVlm":"120","dayBaseVlm":"100"}]])),
            "spotClearinghouseState" => Some(
                json!({"balances":[{"coin":"USDC","token":0,"hold":"2","total":"100","entryNtl":"0"}],"tokenToAvailableAfterMaintenance":[]}),
            ),
            _ => None,
        },
        ("POST", "/exchange") => {
            let expected = serde_json::to_value(
                safe_cex_venue_hyperliquid::signing::sign_l1_action(
                    KEY,
                    &body["action"],
                    None,
                    &body["nonce"],
                    true,
                )
                .unwrap(),
            )
            .unwrap();
            for key in ["r", "s"] {
                assert_eq!(
                    body["signature"][key], expected[key],
                    "native server verifies source Spot action/nonce signature"
                );
            }
            assert_eq!(body["signature"]["v"].as_f64(), expected["v"].as_f64());
            Some(match body["action"]["type"].as_str()? {
                "order" => {
                    json!({"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":777}}]}}})
                }
                "cancel" => {
                    json!({"status":"ok","response":{"type":"cancel","data":{"statuses":["success"]}}})
                }
                _ => return None,
            })
        }
        _ => None,
    }
}
#[test]
fn hyperliquidspot_registry_bootstraps_places_and_cancels_over_native_http() {
    run_local(async {
        tokio::time::timeout(std::time::Duration::from_secs(15),async {
            let h=venue_harness(ExchangeName::Hyperliquidspot,Rc::new(routes),json!({"key":ADDRESS,"secret":KEY,"applicationId":ADDRESS}),None,safe_cex_exchange::native::native_runtime()).await;
            assert_eq!(h.exchange.name(),&ExchangeName::Hyperliquidspot);
            h.exchange.start().await.expect("Spot production start");
            assert!(h.store.loaded().unwrap().markets && h.store.loaded().unwrap().tickers);
            assert_eq!(h.store.markets().unwrap()[0].symbol,"PURR/USDC");
            assert!(h.store.tickers().unwrap()[0].index.is_nan());
            eventually("both native Spot sockets connect",||h.peers.borrow().len()==2).await;
            let opts:PlaceOrderOpts=serde_json::from_value(json!({"symbol":"PURR/USDC","type":"limit","side":"buy","amount":1,"price":1.2,"clientOrderId":"0x11111111111111111111111111111111"})).unwrap();
            let ids=h.exchange.base().place_order(h.exchange.as_ref(),opts).await.unwrap();
            assert_eq!(ids,["777"]);
            let order=serde_json::from_value(json!({"id":ids[0],"symbol":"PURR/USDC","status":"open","type":"limit","side":"buy","price":1.2,"amount":1,"filled":0,"remaining":1,"reduceOnly":false,"lastUpdate":1700000000000u64})).unwrap();
            h.exchange.cancel_orders(vec![order]).await.unwrap();
            let seen=h.seen.borrow();
            let orders:Vec<Value>=seen.iter().filter(|r|r.path=="/exchange").map(|r|serde_json::from_str(&r.body).unwrap()).collect();
            assert_eq!(orders.len(),2);
            assert_eq!(orders[0]["action"]["orders"][0]["a"],10000);
            assert_eq!(orders[0]["action"]["orders"][0]["c"],"0x11111111111111111111111111111111");
            assert_eq!(orders[1]["action"]["cancels"][0],json!({"a":10000,"o":777}));
            drop(seen);
            h.exchange.dispose().unwrap();
        }).await.expect("native Spot lifecycle completes locally");
    });
}
