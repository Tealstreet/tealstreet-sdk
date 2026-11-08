//! Legacy TypeScript permits native lot refusals after earlier placements.
mod support;
use safe_cex_engine::{Command, Event, Method};
use safe_cex_venue_binance::Variant;
use support::{Harness, Options};

fn ready(variant: Variant) -> Harness {
    let mut h = Harness::started(Options {
        variant,
        key: Some("fixture-key".into()),
        secret: Some("fixture-seed".into()),
        ..Options::default()
    });
    let prefix = if variant == Variant::Spot {
        "/api/v3"
    } else {
        "/fapi/v1"
    };
    h.respond(&format!("{prefix}/exchangeInfo"), 200, r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","marginAsset":"USDT","contractType":"PERPETUAL","filters":[{"filterType":"PRICE_FILTER","tickSize":"0.1"},{"filterType":"LOT_SIZE","stepSize":"1","minQty":"3","maxQty":"5"},{"filterType":"MARKET_LOT_SIZE","stepSize":"1","minQty":"3","maxQty":"5"}]}]}"#);
    h.respond(&format!("{prefix}/ticker/24hr"), 200, support::DAILY);
    if variant != Variant::Spot {
        h.respond(
            &format!("{prefix}/ticker/bookTicker"),
            200,
            support::BOOK_TICKERS,
        );
        h.respond(&format!("{prefix}/premiumIndex"), 200, support::PREMIUM);
    }
    h.requests.clear();
    h
}

#[test]
fn spot_native_minimum_is_not_a_new_whole_plan_gate() {
    let mut h = ready(Variant::Spot);
    h.event(Event::Command(Command::PlaceOrders {
        command: 77,
        orders: serde_json::from_str(r#"[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":5,"price":100},{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":2,"price":100}]"#).unwrap(),
    }));
    let first = h.take("/api/v3/order");
    assert_eq!(first.method, Method::Post);
    assert!(first.url.contains("quantity=5&"));
    h.reply(first.id, 200, "", r#"{"orderId":1}"#);
    let last = h.take("/api/v3/order");
    assert!(last.url.contains("quantity=2&"));
    h.reply(
        last.id,
        400,
        "",
        r#"{"code":-1013,"msg":"Filter failure: LOT_SIZE"}"#,
    );
    assert!(h.result(77).unwrap().is_err());
    assert!(
        h.requests
            .iter()
            .all(|request| request.method == Method::Get)
    );
}

#[test]
fn attached_algo_keeps_legacy_whole_quantity_after_split_parent() {
    let mut h = ready(Variant::UsdM);
    h.event(Event::Command(Command::PlaceOrders {
        command: 77,
        orders: serde_json::from_str(r#"[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":10,"price":100,"stopLoss":90}]"#).unwrap(),
    }));
    let parent = h.take("/fapi/v1/batchOrders");
    h.reply(parent.id, 200, "", r#"[{"orderId":1},{"orderId":2}]"#);
    let child = h.take("/fapi/v1/algoOrder");
    assert!(child.url.contains("quantity=10&"));
    h.reply(child.id, 200, "", r#"{"algoId":3}"#);
    assert!(h.result(77).unwrap().is_ok());
    assert!(
        h.requests
            .iter()
            .all(|request| request.method == Method::Get)
    );
}
