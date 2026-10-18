#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use std::rc::Rc;

fn collect_ids(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                if ["orderId", "ordId", "orderID", "algoId"].contains(&key.as_str()) {
                    out.push(
                        value
                            .as_str()
                            .map_or_else(|| value.to_string(), str::to_owned),
                    );
                } else {
                    collect_ids(value, out);
                }
            }
        }
        Value::Array(rows) => rows.iter().for_each(|row| collect_ids(row, out)),
        _ => {}
    }
}
async fn check(name: ExchangeName, market_id: &str, all: bool) {
    let selected = name.clone();
    let handler = Rc::new(move |request: &support::Seen| {
        if request.path.contains("account/settings") {
            return Some(json!({"code":"40084","msg":"classic account"}));
        }
        Some(match selected {
            ExchangeName::Okx => json!({"code":"0","data":[{"ordId":"202","sCode":"0","sMsg":""}]}),
            ExchangeName::Blofin => {
                json!({"code":"0","data":[{"orderId":"202","code":"0","msg":""}]})
            }
            ExchangeName::Bitget => {
                json!({"code":"00000","data":{"successList":[{"orderId":"202"}],"failureList":[]}})
            }
            ExchangeName::Bitunix => {
                json!({"code":0,"data":{"successList":[{"orderId":"202"}],"failureList":[]}})
            }
            ExchangeName::Phemex => json!({"code":0,"data":{}}),
            _ => json!({"orderId":202,"status":"CANCELED"}),
        })
    });
    let h = support::venue_harness(
        name.clone(), handler,
        json!({"key":"native-key","secret":support::SECRET,"passphrase":"pass","tealstreetTunnel":false}),
        None, safe_cex_exchange::native::native_runtime(),
    ).await;
    let mut orders = ["closed", "canceled", "open"].into_iter().zip(["101","102","202"])
        .map(|(status,id)| json!({"id":id,"status":status,"symbol":"BTCUSDT","type":"limit","side":"buy","price":100,"amount":1,"filled":0,"remaining":1,"reduceOnly":false,"lastUpdate":0,"info":{"posSide":"Merged"}}))
        .collect::<Vec<_>>();
    if all {
        let mut terminal_only_symbol = orders[0].clone();
        terminal_only_symbol["id"] = json!("303");
        terminal_only_symbol["symbol"] = json!("ETHUSDT");
        orders.push(terminal_only_symbol);
    }
    Store::update(h.store.as_ref(), StoreUpdate {
        markets: Some(serde_json::from_value(json!([{"id":market_id,"symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":0.001,"price":0.1},"limits":{"amount":{"min":0.001,"max":100},"leverage":{"min":1,"max":100}},"contractSize":1,"info":{"settleCurrency":"USDT"}}])).unwrap()),
        orders: Some(serde_json::from_value(Value::Array(orders)).unwrap()),
        ..Default::default()
    }, false).unwrap();
    if all {
        h.exchange
            .base()
            .cancel_all_orders(h.exchange.as_ref())
            .await
    } else if name == ExchangeName::Binance {
        h.exchange
            .base()
            .cancel_symbol_orders(h.exchange.as_ref(), "BTCUSDT")
            .await
    } else {
        h.exchange.cancel_symbol_orders("BTCUSDT".into()).await
    }
    .expect("native cancellation");
    let requests = h.seen.borrow();
    let mut ids = Vec::new();
    for request in requests.iter() {
        if !request.body.is_empty() {
            let value =
                serde_json::from_str::<Value>(&request.body).expect("native cancellation JSON");
            collect_ids(&value, &mut ids);
        }
        for key in ["orderId", "ordId", "orderID", "algoId", "orderIdList"] {
            if let Some(value) = support::query_value(&request.query, key) {
                ids.push(value.to_owned());
            }
        }
    }
    if all {
        assert_eq!(
            requests.len(),
            2,
            "only BTC's live symbol reaches the concrete venue-wide cancellation"
        );
        for request in requests.iter() {
            assert_eq!(
                support::query_value(&request.query, "symbol"),
                Some("BTCUSDT")
            );
        }
    } else {
        assert_eq!(
            ids,
            ["202"],
            "{name:?}: only the live order reaches signed native HTTP; requests={requests:?}"
        );
    }
    drop(requests);
    h.exchange.dispose().unwrap();
}
#[test]
fn okx_symbol_cancel_excludes_terminal_rows() {
    support::run_local(check(ExchangeName::Okx, "BTC-USDT-SWAP", false));
}
#[test]
fn bitget_symbol_cancel_excludes_terminal_rows() {
    support::run_local(check(ExchangeName::Bitget, "BTCUSDT", false));
}
#[test]
fn bitunix_symbol_cancel_excludes_terminal_rows() {
    support::run_local(check(ExchangeName::Bitunix, "BTCUSDT", false));
}
#[test]
fn blofin_symbol_cancel_excludes_terminal_rows() {
    support::run_local(check(ExchangeName::Blofin, "BTC-USDT", false));
}
#[test]
fn phemex_symbol_cancel_excludes_terminal_rows() {
    support::run_local(check(ExchangeName::Phemex, "BTCUSDT", false));
}
#[test]
fn base_symbol_cancel_excludes_terminal_rows() {
    support::run_local(check(ExchangeName::Binance, "BTCUSDT", false));
}

#[test]
fn base_all_cancel_excludes_terminal_rows() {
    support::run_local(check(ExchangeName::Binance, "BTCUSDT", true));
}
#[test]
fn binance_close_conflict_excludes_terminal_rows() {
    support::run_local(async {
        let h = support::binance_harness(Rc::new(|request| {
            request
                .path
                .ends_with("/algoOrder")
                .then(|| json!({"algoId":404}))
        }))
        .await;
        let markets = h.exchange.fetch_markets().await.unwrap();
        let orders = ["closed", "canceled", "open"].into_iter().zip(["101", "102", "202"])
            .map(|(status,id)| json!({"id":id,"status":status,"symbol":"BTCUSDT","type":"stop_market","side":"sell","price":49000,"amount":0,"filled":0,"remaining":0,"close":true,"reduceOnly":true,"lastUpdate":0,"info":{}})).collect::<Vec<_>>();
        Store::update(h.store.as_ref(), StoreUpdate {
            markets: Some(markets), orders: Some(serde_json::from_value(Value::Array(orders)).unwrap()),
            positions: Some(serde_json::from_value(json!([{"id":"BTCUSDT:long","symbol":"BTCUSDT","side":"long","contracts":1,"entryPrice":50000,"notional":50000,"leverage":10,"unrealizedPnl":0,"realizedPnl":0,"liquidationPrice":0,"positionMode":"oneway","marginType":"cross"}])).unwrap()),
            ..Default::default()
        }, false).unwrap();
        let ids = h.exchange.base().place_order(h.exchange.as_ref(), serde_json::from_value::<safe_cex_types::PlaceOrderOpts>(json!({"symbol":"BTCUSDT","type":"stop_market","side":"sell","price":49000,"amount":0,"close":true,"reduceOnly":true})).unwrap()).await.unwrap();
        assert_eq!(ids, ["404"]);
        let requests = h.seen.borrow();
        let canceled = requests
            .iter()
            .filter(|r| r.method == "DELETE")
            .map(|r| support::query_value(&r.query, "algoId").unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            canceled,
            ["202"],
            "closed/canceled closePosition orders produce no signed cancellation"
        );
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.method == "POST" && r.path.ends_with("/algoOrder"))
                .count(),
            1
        );
        drop(requests);
        h.exchange.dispose().unwrap();
    });
}
#[test]
fn blofin_empty_snapshot_warns_only_for_live_store_orders() {
    support::run_local(async {
        let h = support::venue_harness(
            ExchangeName::Blofin,
            Rc::new(|_| Some(json!({"code":"0","data":[]}))),
            json!({"key":"native-key","secret":support::SECRET,"passphrase":"pass"}),
            None,
            safe_cex_exchange::native::native_runtime(),
        )
        .await;
        let messages = Rc::new(std::cell::RefCell::new(Vec::new()));
        let saved = messages.clone();
        h.exchange.base().on(
            "log",
            Rc::new(move |_, value| {
                saved
                    .borrow_mut()
                    .push(String::from_utf16(value.as_str().unwrap()).unwrap());
                Ok(())
            }),
        );
        for (status, warnings) in [("closed", 0), ("canceled", 0), ("open", 1)] {
            let orders = serde_json::from_value(json!([{"id":"101","status":status,"symbol":"BTCUSDT","type":"limit","side":"buy","price":100,"amount":1,"filled":0,"remaining":1,"reduceOnly":false,"lastUpdate":0,"info":{}}])).unwrap();
            Store::update(
                h.store.as_ref(),
                StoreUpdate {
                    orders: Some(orders),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
            let before = h.seen.borrow().len();
            assert!(h.exchange.fetch_orders(None).await.unwrap().is_empty());
            assert_eq!(
                h.seen.borrow().len() - before,
                3,
                "all three actual signed snapshot families ran"
            );
            assert_eq!(
                messages
                    .borrow()
                    .iter()
                    .filter(|m| m.contains("Warning: fetchOrders returned empty"))
                    .count(),
                warnings,
                "{status} warning policy"
            );
        }
        h.exchange.dispose().unwrap();
    });
}
