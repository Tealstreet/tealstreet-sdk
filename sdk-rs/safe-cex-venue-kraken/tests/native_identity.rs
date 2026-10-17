#[allow(dead_code)]
#[path = "support/binding_harness.rs"]
mod support;
use safe_cex_types::{ExchangeName, PlaceOrderOpts};
use serde_json::{Value, json};
use support::{Harness, harness, options};
const SYMBOL: &str = "PF_XBTUSD";

fn instruments() -> Value {
    json!({"result": "success", "instruments": [{
        "symbol": SYMBOL, "tradeable": true, "base": "BTC", "quote": "USD",
        "contractValueTradePrecision": 4, "tickSize": 0.5, "contractSize": 1,
        "marginLevels": [{"initialMargin": 0.02}], "contractType": "flexible_futures",
        "status": "open"
    }]})
}

fn tickers() -> Value {
    json!({"result": "success", "tickers": [{
        "symbol": SYMBOL, "bid": 30000, "ask": 30001, "last": 30000.5,
        "markPrice": 30000.25, "indexPrice": 30000.1, "vol24h": 12, "openInterest": 7,
        "fundingRate": 0.0001, "change24h": 1.5
    }]})
}

fn futures(script: impl FnOnce(&support::Http)) -> Harness {
    harness(ExchangeName::Kraken, options(), |http| {
        http.ok("GET", "/v3/instruments", instruments());
        http.ok("GET", "/v3/tickers", tickers());
        script(http);
    })
}

/// Markets and tickers loaded into the Store, as start() leaves them.
fn loaded(h: &Harness) {
    let markets = h.drive(h.venue().fetch_markets()).expect("fetchMarkets");
    let tickers = h.drive(h.venue().fetch_tickers()).expect("fetchTickers");
    safe_cex_store::Store::update(
        h.store.as_ref(),
        safe_cex_store::StoreUpdate {
            markets: Some(markets),
            tickers: Some(tickers),
            ..Default::default()
        },
        false,
    )
    .expect("store update");
}

fn order(opts: Value) -> PlaceOrderOpts {
    serde_json::from_value(opts).expect("place order opts")
}

fn data(request: &support::Request) -> String {
    request.data.as_str().unwrap_or_default().to_owned()
}

#[test]
fn generated_client_id_strips_both_uuid_separators() {
    let h = futures(|http| {
        http.ok(
            "POST",
            "/v3/sendorder",
            json!({"result": "success", "sendStatus": {"status": "placed", "order_id": "new-1"}}),
        )
    });
    *h.runtime.id.borrow_mut() = Some("fixture-_nonce".into());
    loaded(&h);
    let ids = h
        .drive(Box::pin(safe_cex_exchange::exchange::Exchange::base(h.exchange.as_ref()).place_order(
            h.venue(),
            order(json!({"symbol": SYMBOL, "type": "limit", "side": "buy", "amount": 1, "price": 30000})),
        )))
        .expect("placeOrder");
    assert_eq!(ids, vec!["new-1"]);
    let sent = &h.http.calls("/v3/sendorder")[0];
    let body = data(sent);
    assert!(
        body.contains("cliOrdId=ts_fixturenonce"),
        "source utils/uuid client ID: {body}"
    );
    assert!(body.contains("orderType=lmt"), "{body}");
    assert!(body.contains("limitPrice=30000"), "{body}");
    assert!(body.contains("symbol=PF_XBTUSD"), "{body}");
    assert_eq!(
        sent.headers["Content-Type"],
        "application/x-www-form-urlencoded"
    );
}
