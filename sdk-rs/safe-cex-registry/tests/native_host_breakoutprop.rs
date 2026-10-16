//! DXtrade login and order traffic through the registry's production native host.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_types::{ExchangeName, PlaceOrderOpts};
use serde_json::{Value, json};
use std::rc::Rc;

#[test]
fn breakoutprop_registry_logs_in_reads_places_and_cancels_over_native_http() {
    support::run_local(async {
        let h = support::venue_harness(
            ExchangeName::Breakoutprop,
            Rc::new(|request| {
                let path = request.path.strip_prefix("/dxsca-web").unwrap_or(&request.path);
                Some(match (request.method.as_str(), path) {
                    ("POST", "/login") => json!({"sessionToken":"dx-native-token","timeout":"1:00:00"}),
                    ("GET", "/users/k") => json!({"userDetails":[{"login":"k","domain":"default","version":1,"fullName":"Trader","username":"k","accounts":[{"owner":"k","account":"default:A","version":1,"baseCurrency":"USD","registrationTime":"2026-01-01T00:00:00Z","accountStatus":"FULL_TRADING","pricingStream":"quote","positionBased":false}]}]}),
                    ("GET", "/instruments/query") => json!({"instruments":[{"type":"FOREX","symbol":"BTCUSD","version":1,"description":"Bitcoin","priceIncrement":0.1,"pipSize":0.01,"currency":"USD","lotSize":1,"multiplier":1,"firstCurrency":"BTC"}]}),
                    ("GET", "/accounts/default:A/instruments/query") => json!({"instrumentDetails":[{"account":"default:A","type":"FOREX","symbol":"BTCUSD","tradingStatus":"TRADING","minOrderSize":0.01,"maxOrderSize":{"value":100,"limitType":"UNITS"},"minOrderSizeIncrement":0.01,"marginRate":{"rateType":"FIXED","tiers":[{"endVolume":"Infinity","value":0.05}]}}]}),
                    ("GET", "/accounts/default:A/portfolio") => json!({"portfolios":[{"account":"default:A","version":1,"balances":[{"account":"default:A","version":1,"currency":"USD","value":50.25}],"positions":[],"orders":[]}]}),
                    ("POST", "/marketdata") => json!({"events":[{"symbol":"BTCUSD","type":"Quote","bid":99,"ask":101,"time":"2026-01-01T00:00:00Z"}]}),
                    ("POST", "/accounts/default:A/orders") => json!({"orderId":"native-dx-order"}),
                    ("DELETE", "/accounts/default:A/orders/native-dx-order") => json!({}),
                    _ => return None,
                })
            }),
            json!({"key":"k","secret":"s"}),
            None,
            safe_cex_exchange::native::native_runtime(),
        ).await;
        assert_eq!(
            serde_json::to_value(h.exchange.validate_account().await.unwrap()).unwrap(),
            json!({"error":""})
        );
        h.exchange.start().await.unwrap();
        assert_eq!(h.store.markets().len(), 1);
        assert_eq!(h.exchange.fetch_tickers().await.unwrap()[0].last, 99.0);
        let positions = h.exchange.fetch_positions(None).await.unwrap();
        assert_eq!(
            positions.len(),
            1,
            "source creates a flat position for each stored market"
        );
        assert_eq!(positions[0].symbol, "BTCUSD");
        assert_eq!(positions[0].contracts, 0.0);
        assert!(h.exchange.fetch_orders(None).await.unwrap().is_empty());
        let opts: PlaceOrderOpts = serde_json::from_value(
            json!({"symbol":"BTCUSD","type":"limit","side":"buy","amount":1.234,"price":99.26}),
        )
        .unwrap();
        let ids = h
            .exchange
            .base()
            .place_order(h.exchange.as_ref(), opts)
            .await
            .unwrap();
        assert_eq!(ids, ["native-dx-order"]);
        let order = serde_json::from_value(json!({"id":"native-dx-order","status":"open","symbol":"BTCUSD","type":"limit","side":"buy","price":99.3,"amount":1.23,"filled":0,"remaining":1.23,"reduceOnly":false,"lastUpdate":0})).unwrap();
        h.exchange.cancel_orders(vec![order]).await.unwrap();
        let seen = h.seen.borrow();
        let login: Value = serde_json::from_str(
            &seen
                .iter()
                .find(|r| r.path.ends_with("/login"))
                .unwrap()
                .body,
        )
        .unwrap();
        assert_eq!(
            login,
            json!({"domain":"default","username":"k","password":"s"})
        );
        let placed = seen
            .iter()
            .find(|r| r.method == "POST" && r.path.ends_with("/accounts/default:A/orders"))
            .unwrap();
        assert_eq!(
            placed
                .headers
                .get("coinbase-authorization")
                .map(String::as_str),
            Some("DXAPI dx-native-token")
        );
        let body: Value = serde_json::from_str(&placed.body).unwrap();
        assert_eq!(body["quantity"], 1.23);
        assert_eq!(body["limitPrice"], 99.3);
        assert!(
            body["orderCode"]
                .as_str()
                .unwrap()
                .starts_with("tealstreet-")
        );
        assert!(
            seen.iter()
                .any(|r| r.method == "DELETE" && r.path.ends_with("/orders/native-dx-order"))
        );
        assert!(
            h.peers.borrow().is_empty(),
            "DXtrade polling has no socket dial"
        );
        drop(seen);
        h.exchange.dispose().unwrap();
    });
}
