use super::support;
use serde_json::json;

pub fn script(wire: &support::Wire) {
    let mut responses = wire.responses.borrow_mut();
    responses.insert(
        "/fapi/v1/time".into(),
        json!({"serverTime":1700000000000u64}),
    );
    responses.insert("/fapi/v4/account".into(),json!({"accountAlias":"aster-account","totalWalletBalance":"100","availableBalance":"70","totalUnrealizedProfit":"5","totalInitialMargin":"12","totalOpenOrderInitialMargin":"3"}));
    responses.insert(
        "/fapi/v1/positionSide/dual".into(),
        json!({"dualSidePosition":false}),
    );
    responses.insert(
        "/fapi/v1/listenKey".into(),
        json!({"listenKey":"fixture-listen-key"}),
    );
    responses.insert("/fapi/v1/exchangeInfo".into(),json!({"symbols":[{
        "symbol":"BTCUSDT","baseAsset":"BTC","quoteAsset":"USDT","contractType":"PERPETUAL","status":"TRADING",
        "maintMarginPercent":"2.5","requiredMarginPercent":"5","pricePrecision":2,"quantityPrecision":3,"baseAssetPrecision":8,"quotePrecision":8,
        "triggerProtect":"0.05","liquidationFee":"0.01","marketTakeBound":"0.3","filters":[{"filterType":"PRICE_FILTER","minPrice":"0.01","maxPrice":"1000000","tickSize":"0.25"},{"filterType":"LOT_SIZE","minQty":"0.001","maxQty":"100","stepSize":"0.001"}],"orderTypes":["LIMIT","MARKET"],"timeInForce":["GTC","IOC"]
    }]}));
    responses.insert(
        "/fapi/v1/leverageBracket".into(),
        json!([{"symbol":"BTCUSDT","brackets":[{"initialLeverage":100}]}]),
    );
    responses.insert("/fapi/v1/ticker/24hr".into(),json!([{"symbol":"BTCUSDT","priceChange":"2","priceChangePercent":"1","weightedAvgPrice":"101","lastPrice":"102","lastQty":"0.2","openPrice":"100","highPrice":"110","lowPrice":"90","volume":"50","quoteVolume":"5000","openTime":1699990000000u64,"closeTime":1700000000000u64,"firstId":1,"lastId":10,"count":10}]));
    responses.insert(
        "/fapi/v1/openInterest".into(),
        json!({"symbol":"BTCUSDT","openInterest":"7"}),
    );
    responses.insert("/fapi/v1/positionRisk".into(),json!([{"symbol":"BTCUSDT","positionAmt":"2","entryPrice":"99","breakEvenPrice":"99.5","markPrice":"102","unRealizedProfit":"6","liquidationPrice":"50","leverage":"10","maxNotionalValue":"10000","marginType":"cross","isolatedMargin":"0","isAutoAddMargin":"false","positionSide":"BOTH","notional":"204","isolatedWallet":"0","updateTime":1700000000000u64}]));
    responses.insert("/fapi/v1/openOrders".into(),json!([{"symbol":"BTCUSDT","orderId":9001,"clientOrderId":"resting","price":"95","origQty":"0.02","executedQty":"0.005","cummulativeQuoteQty":"0.475","status":"NEW","timeInForce":"GTC","type":"LIMIT","side":"BUY","time":1699990000000u64,"updateTime":1700000000000u64,"isWorking":true,"workingTime":1699990000000u64,"origQuoteOrderQty":"1.9"}]));
    responses.insert("/fapi/v1/order".into(), json!({"orderId":1234567}));
    responses.insert("/fapi/v1/trades".into(),json!([{"id":4,"price":"101","qty":"0.03","quoteQty":"3.03","time":1700000000000u64,"isBuyerMaker":true}]));
}
