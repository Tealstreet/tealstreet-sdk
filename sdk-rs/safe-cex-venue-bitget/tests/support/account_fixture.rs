use super::harness::*;
use safe_cex_store::{Store, StoreUpdate};
use serde_json::{Value, json};

pub const MARKETS: &str = "/api/v2/mix/market/contracts";
pub const TICKERS: &str = "/api/v2/mix/market/tickers";

pub fn market_row() -> Value {
    json!({
        "symbol": "BTCUSDT", "baseCoin": "BTC", "quoteCoin": "USDT", "symbolStatus": "normal",
        "pricePlace": "1", "priceEndStep": "1", "sizeMultiplier": "0.001", "minTradeNum": "0.001",
        "maxOrderQty": "100", "minLever": "1", "maxLever": "125"
    })
}

pub fn ticker_row() -> Value {
    json!({
        "symbol": "BTCUSDT", "lastPr": "50000", "bidPr": "49999", "askPr": "50001", "indexPrice": "50000.5",
        "changeUtc24h": "0.01", "fundingRate": "0.0001", "baseVolume": "10", "quoteVolume": "500000"
    })
}

pub fn position_row() -> Value {
    json!({
        "symbol": "BTCUSDT", "holdSide": "long", "total": "0.01", "openPriceAvg": "50000", "markPrice": "50100",
        "marginMode": "crossed", "posMode": "one_way_mode", "leverage": "10", "unrealizedPL": "1",
        "liquidationPrice": "40000", "achievedProfits": "0", "totalFee": "0", "deductedFee": "0", "marginSize": "50"
    })
}

pub fn order_row() -> Value {
    json!({
        "orderId": "o1", "clientOid": "c1", "symbol": "BTCUSDT", "side": "buy", "orderType": "limit",
        "price": "49000", "size": "0.01", "baseVolume": "0", "status": "live", "tradeSide": "open",
        "posSide": "long", "reduceOnly": "NO", "force": "gtc", "cTime": "1700000000000", "posMode": "one_way_mode"
    })
}

/// A Classic (non-UTA) account: the v3 settings probe answers 40084.
pub fn classic(http: &Http) {
    http.ok(
        "GET",
        "/api/v2/spot/account/info",
        json!({"userId": "123", "inviterId": "9", "parentId": null}),
    );
    http.ok("GET", MARKETS, json!([market_row()]));
    http.ok("GET", TICKERS, json!([ticker_row()]));
    http.ok(
        "GET",
        "/api/v2/mix/account/accounts",
        json!([{"marginCoin": "USDT", "available": "100", "accountEquity": "120", "unrealizedPL": "2", "locked": "5"}]),
    );
    http.ok(
        "GET",
        "/api/v2/mix/position/all-position",
        json!([position_row()]),
    );
    http.ok(
        "GET",
        "/api/v2/mix/order/orders-pending",
        json!({"entrustedList": [order_row()], "endId": "o1"}),
    );
    http.ok(
        "GET",
        "/api/v2/mix/order/orders-pending",
        json!({"entrustedList": [], "endId": null}),
    );
    http.ok(
        "GET",
        "/api/v2/mix/order/orders-plan-pending",
        json!({"entrustedList": [], "endId": null}),
    );
    http.ok(
        "POST",
        "/api/v2/mix/account/set-position-mode",
        json!({"posMode": "hedge_mode"}),
    );
    http.ok("POST", "/api/v2/mix/account/set-margin-mode", json!({}));
    http.ok("POST", "/api/v2/mix/account/set-leverage", json!({}));
    http.ok(
        "POST",
        "/api/v2/mix/order/batch-place-order",
        json!({"successList": [{"orderId": "p1", "clientOid": "cp1"}], "failureList": []}),
    );
    http.ok(
        "POST",
        "/api/v2/mix/order/modify-order",
        json!({"orderId": "o1", "clientOid": "c1"}),
    );
    http.ok(
        "POST",
        "/api/v2/mix/order/batch-cancel-orders",
        json!({"successList": [{"orderId": "o1", "clientOid": "c1"}], "failureList": []}),
    );
    http.route(
        "GET",
        "/api/v3/account/settings",
        400.0,
        json!({"code": "40084", "msg": "Classic Account mode", "requestTime": 1, "data": null}),
    );
}

pub fn loaded(h: &Harness) {
    let markets = h.drive(h.venue().fetch_markets()).expect("markets");
    Store::update(
        &*h.store,
        StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .expect("store markets");
}
