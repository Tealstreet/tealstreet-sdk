// Accepted/native E2E fixture: actual BitgetExchange over existing scripted I/O.
#[allow(dead_code)]
pub mod io {
    use super::venue_crate as safe_cex_venue_bitget;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/binding_harness.rs"
    ));
}

pub fn setup(alternate: bool) -> io::Harness {
    use safe_cex_store::{Store, StoreUpdate};
    use safe_cex_types::*;
    use serde_json::json;
    let (symbol, base, price, qty, id) = if alternate {
        ("ETHUSDT", "ETH", 202.5, 3.0, "native-b")
    } else {
        ("BTCUSDT", "BTC", 101.5, 2.0, "native-a")
    };
    let h = io::harness(ExchangeName::Bitget, io::options(), |http| {
        http.route(
            "GET",
            "/api/v3/account/settings",
            400.0,
            json!({"code":"40084","msg":"Classic Account mode","requestTime":1,"data":null}),
        );
        http.ok("GET","/api/v2/mix/position/all-position",json!([{"symbol":symbol,"holdSide":"long","total":qty.to_string(),"openPriceAvg":price.to_string(),"markPrice":price.to_string(),"marginMode":"crossed","posMode":"one_way_mode","leverage":"10","unrealizedPL":"1","liquidationPrice":"71.25","achievedProfits":"0","totalFee":"0","deductedFee":"0","marginSize":"50"}]));
        http.ok("GET","/api/v2/mix/order/orders-pending",json!({"entrustedList":[{"orderId":id,"symbol":symbol,"side":"buy","orderType":"limit","price":price.to_string(),"size":qty.to_string(),"baseVolume":"0.5","status":"live","force":"gtc","reduceOnly":"NO"}],"endId":null}));
        http.ok(
            "GET",
            "/api/v2/mix/order/orders-plan-pending",
            json!({"entrustedList":[],"endId":null}),
        );
        for path in [
            "/api/v2/mix/account/set-leverage",
            "/api/v2/mix/account/set-margin-mode",
            "/api/v2/mix/account/set-position-mode",
        ] {
            http.ok("POST", path, json!({}));
        }
    });
    let market:Market=serde_json::from_value(json!({"id":symbol,"symbol":symbol,"base":base,"quote":"USDT","active":true,"precision":{"amount":0.001,"price":0.1},"limits":{"amount":{"min":0.001,"max":50},"leverage":{"min":1,"max":20}}})).unwrap();
    let position:Position=serde_json::from_value(json!({"id":symbol,"symbol":symbol,"side":"long","entryPrice":price,"notional":price*qty,"leverage":10,"marginType":"cross","positionMode":"oneway","unrealizedPnl":1,"contracts":qty,"liquidationPrice":71.25})).unwrap();
    Store::update(
        &*h.store,
        StoreUpdate {
            markets: Some(vec![market]),
            positions: Some(vec![position]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    h
}

pub fn posted(h: &io::Harness, path: &str) -> serde_json::Value {
    h.http
        .calls(path)
        .last()
        .expect("actual HTTP POST")
        .data
        .clone()
}
