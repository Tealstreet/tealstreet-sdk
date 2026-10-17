//! Production Coinbase reads coerce malformed REST fields like the TS instead of panicking.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod binding_harness;
use binding_harness::{Harness, Http, harness, options};
use safe_cex_types::*;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

const PORTFOLIOS: &str = "/api/v3/brokerage/portfolios";
const CFM_BALANCE: &str = "/api/v3/brokerage/cfm/balance_summary";

fn portfolio(http: &Http, kind: &str, spot: Value) {
    http.ok("GET", PORTFOLIOS, json!({"portfolios":[{"name":"Synthetic","uuid":"portfolio-synthetic","type":kind,"deleted":false}]}));
    http.ok(
        "GET",
        "/api/v3/brokerage/portfolios/portfolio-synthetic",
        json!({"breakdown":{"spot_positions":spot}}),
    );
}
fn errors(h: &Harness) -> Rc<RefCell<Vec<Value>>> {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let sink = seen.clone();
    h.venue().base().on(
        "error",
        Rc::new(move |_, value| {
            let value = safe_cex_exchange::js_runtime::value::to_json(value).unwrap();
            sink.borrow_mut().push(value.clone());
            Ok(())
        }),
    );
    seen
}
fn requested(h: &Harness) -> Vec<String> {
    h.http
        .requests
        .borrow()
        .iter()
        .map(|r| r.url.clone())
        .collect()
}

#[test]
fn spot_balance_coerces_null_and_absent_fields() {
    let h = harness(ExchangeName::Coinbase, options(), |http| {
        portfolio(
            http,
            "DEFAULT",
            json!([
                {"asset":"USDC","available_to_trade_crypto":13.25,"total_balance_crypto":null},
                {"available_to_trade_crypto":1.5,"total_balance_crypto":2.0}
            ]),
        );
        http.route(
            "GET",
            CFM_BALANCE,
            403.0,
            json!({"message":"No CFM permissions"}),
        );
    });
    let balance = h.drive(h.exchange.fetch_balance()).unwrap();
    let currencies = balance.currencies.as_ref().expect("spot currencies");
    let usdc = currencies["USDC"].as_balance().unwrap();
    assert_eq!(
        (usdc.free, usdc.used),
        (13.25, -13.25),
        "null total_balance_crypto must coerce to 0"
    );
    let unnamed = currencies["undefined"]
        .as_balance()
        .expect("absent asset keys as String(undefined)");
    assert_eq!((unnamed.free, unnamed.used), (1.5, 0.5));
}

#[test]
fn cfm_position_without_product_id_is_skipped_with_source_message() {
    let h = harness(ExchangeName::Coinbase, options(), |http| {
        portfolio(http, "DEFAULT", json!([]));
        http.ok("GET", CFM_BALANCE, json!({"balance_summary":{}}));
        http.ok("GET", "/api/v3/brokerage/cfm/positions", json!({"positions":[{"side":"LONG","number_of_contracts":"1","current_price":"1","avg_entry_price":"1","unrealized_pnl":"0"}]}));
    });
    let seen = errors(&h);
    let positions = h.drive(h.venue().fetch_positions(None)).unwrap();
    assert!(positions.is_empty(), "{positions:?} {:?}", requested(&h));
    assert!(
        seen.borrow().iter().any(|e| e
            .to_string()
            .contains("missing or invalid contract size for undefined")),
        "CFM skip must name String(undefined): {:?}",
        seen.borrow()
    );
}

#[test]
fn intx_position_without_entry_vwap_keeps_cached_positions() {
    let h = harness(ExchangeName::Coinbase, options(), |http| {
        portfolio(http, "INTX", json!([]));
        http.ok("GET", "/api/v3/brokerage/intx/positions/portfolio-synthetic", json!({"positions":[{"symbol":"BTC-PERP-INTX","position_side":"POSITION_SIDE_LONG","mark_price":{"value":"100"},"net_size":"1","liquidation_price":{"value":"50"}}]}));
    });
    let seen = errors(&h);
    let positions = h.drive(h.venue().fetch_positions(None)).unwrap();
    assert!(
        positions.is_empty(),
        "missing entry_vwap must throw and return cached positions, not NaN: {positions:?} {:?}",
        requested(&h)
    );
    assert!(
        seen.borrow().iter().any(|e| e
            .to_string()
            .contains("Cannot read properties of undefined (reading 'value')")),
        "INTX TypeError must be emitted: {:?}",
        seen.borrow()
    );
}

fn instrument() -> Value {
    let quote = json!({"best_bid_price":"1","best_ask_price":"2","trade_price":"1.5","mark_price":"1.5","index_price":"1.5","predicted_funding":"0"});
    json!({"trading_state":"TRADING","type":"PERP","qty_24hr":"1","notional_24hr":"1","open_interest":"1","quote":quote})
}

#[test]
fn intx_tickers_coerce_absent_symbol() {
    let instrument = instrument();
    let h = harness(ExchangeName::Coinbase, options(), |http| {
        portfolio(http, "INTX", json!([]));
        http.ok(
            "GET",
            "https://api.international.coinbase.com/api/v1/instruments/",
            json!([instrument]),
        );
    });
    let tickers = h.drive(h.venue().fetch_tickers()).unwrap();
    assert_eq!(
        tickers
            .iter()
            .map(|t| t.symbol.as_str())
            .collect::<Vec<_>>(),
        ["undefined-INTX"],
        "{:?}",
        requested(&h)
    );
}

#[test]
fn intx_tickers_throw_on_absent_quote() {
    let mut quoteless = instrument();
    quoteless.as_object_mut().unwrap().remove("quote");
    quoteless["symbol"] = json!("BTC-PERP");
    let h = harness(ExchangeName::Coinbase, options(), |http| {
        portfolio(http, "INTX", json!([]));
        http.ok(
            "GET",
            "https://api.international.coinbase.com/api/v1/instruments/",
            json!([quoteless]),
        );
    });
    let seen = errors(&h);
    let tickers = h.drive(h.venue().fetch_tickers()).unwrap();
    assert!(
        tickers.is_empty(),
        "absent quote must throw and return cached tickers: {tickers:?}"
    );
    assert!(
        seen.borrow()
            .iter()
            .any(|e| e.to_string().contains("reading 'best_bid_price'")),
        "{:?}",
        seen.borrow()
    );
}
