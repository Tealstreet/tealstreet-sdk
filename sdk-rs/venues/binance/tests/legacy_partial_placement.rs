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
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: binance | batch-chunking/audit trading.rs:1468
// TS: packages/safe-cex/src/__tests__/reference/binance/binancespot.exchange.ts:467-470,576-594.
// The first order lands before the below-minimum second order is sent and refused.
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
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: binance | separate-TP-SL-children/audit trading.rs:2452
// TS: packages/safe-cex/src/__tests__/reference/binance/binance.exchange.ts:1783-1816,1851-1905.
// Both regular lots are dispatched before the single whole-quantity algo child.
fn attached_algo_keeps_legacy_whole_quantity_after_split_parent() {
    let mut h = ready(Variant::UsdM);
    h.event(Event::Command(Command::PlaceOrders {
        command: 77,
        orders: serde_json::from_str(r#"[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":10,"price":100,"stopLoss":90}]"#).unwrap(),
    }));
    let parent = h.take("/fapi/v1/batchOrders");
    let parent_sizes = support::quantities(&parent);
    assert_eq!(parent_sizes, [5.0, 5.0]);
    assert_eq!(parent_sizes.iter().sum::<f64>(), 10.0);
    h.reply(parent.id, 200, "", r#"[{"orderId":1},{"orderId":2}]"#);
    let child = h.take("/fapi/v1/algoOrder");
    assert!(child.url.contains("quantity=10&"));
    let child_sizes = support::quantities(&child);
    assert_eq!(child_sizes, [10.0]);
    assert_eq!(child_sizes.iter().sum::<f64>(), 10.0);
    h.reply(child.id, 200, "", r#"{"algoId":3}"#);
    assert!(h.result(77).unwrap().is_ok());
    assert!(
        h.requests
            .iter()
            .all(|request| request.method == Method::Get)
    );
}

#[test]
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: binance | max-qty-splitting/audit trading.rs:2831
// TS: packages/safe-cex/src/__tests__/reference/binance/binancespot.exchange.ts:535-547,576-594.
// A 10-unit intent with max=5 emits two 5-unit writes; a later refusal keeps the prefix.
fn spot_split_lots_keep_the_legacy_partial_write_sequence() {
    let mut h = ready(Variant::Spot);
    h.event(Event::Command(Command::PlaceOrders {
        command: 77,
        orders: serde_json::from_str(
            r#"[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":10,"price":100}]"#,
        )
        .unwrap(),
    }));
    let first = h.take("/api/v3/order");
    assert_eq!(first.method, Method::Post);
    assert!(first.url.contains("quantity=5&"));
    assert!(h.requests.is_empty());
    h.reply(first.id, 200, "", r#"{"orderId":1}"#);
    let last = h.take("/api/v3/order");
    assert_eq!(last.method, Method::Post);
    assert!(last.url.contains("quantity=5&"));
    let sizes: Vec<_> = support::quantities(&first)
        .into_iter()
        .chain(support::quantities(&last))
        .collect();
    assert_eq!(sizes, [5.0, 5.0]);
    assert_eq!(sizes.iter().sum::<f64>(), 10.0);
    assert!(h.requests.is_empty());
    h.reply(
        last.id,
        400,
        "",
        r#"{"code":-1013,"msg":"Filter failure: MIN_NOTIONAL"}"#,
    );
    assert!(
        h.requests
            .iter()
            .all(|request| request.method == Method::Get)
    );
    let failure = &h.failures.iter().find(|(id, _)| *id == 77).unwrap().1;
    assert_eq!(failure.partial.as_ref().unwrap().order_ids.len(), 1);
    let rows = failure.elements.as_ref().unwrap();
    assert!(matches!(
        &rows[0].result,
        safe_cex_engine::write_plan::ElementResult::Placed { .. }
    ));
    assert!(matches!(
        &rows[1].result,
        safe_cex_engine::write_plan::ElementResult::NotSent { error: Some(_) }
    ));
}
