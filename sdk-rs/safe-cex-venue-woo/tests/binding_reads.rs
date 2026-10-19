//! Account/market/ticker reads and the leverage/position-mode writes of the
//! production WooExchange, driven end to end over a scripted transport.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)] // Shared support: each test crate uses a subset.
mod binding_harness;
use binding_harness::{Harness, harness, options};
use safe_cex_store::Store;
use safe_cex_types::{ExchangeName, ExchangeOptions};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

const ACCOUNT: &str = "/v3/accountinfo";
const MARKETS: &str = "/v1/public/info";
const TICKERS: &str = "/v1/public/futures";
const LEVERAGE: &str = "/v1/client/futures_leverage";
const MODE: &str = "/v1/client/position_mode";
const POSITIONS: &str = "/v3/positions";

#[path = "../../parity/test-support/ticker_ingress.rs"]
#[cfg(not(target_arch = "wasm32"))]
mod ticker_ingress;

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn ticker_ingress_startup_keeps_maintained_construction_order() {
    let source = ticker_ingress::source("woo", "rest");
    let opts =
        serde_json::from_value(json!({"key": "", "secret": "", "publicDataOnly": true})).unwrap();
    let h = harness(ExchangeName::Woo, opts, |http| {
        http.route(
            "GET",
            MARKETS,
            200.0,
            json!({"success": true, "rows": [{"symbol": "PERP_BTC_USDT",
            "base_imr": 0.02, "base_tick": 0.0001, "quote_tick": 0.1,
            "base_min": 0.0001, "base_max": 100}]}),
        );
        http.route(
            "GET",
            TICKERS,
            200.0,
            json!({"success": true, "rows": [{"symbol": "PERP_BTC_USDT",
            "24h_open": 100, "24h_close": 101.2345, "index_price": 101.1,
            "mark_price": 101.2, "last_funding_rate": 0.0001,
            "next_funding_time": 1700000000000_f64, "24h_volume": 12,
            "24h_amount": 1200, "open_interest": 7}]}),
        );
    });
    h.drive(h.venue().start()).unwrap();
    assert_eq!(h.http.calls(MARKETS).len(), 1);
    assert_eq!(h.http.calls(TICKERS).len(), 1);
    assert_eq!(
        ticker_ingress::observe(h.store.clone()),
        source,
        "WOO_POST1248_TICKER_ORDER"
    );
    h.venue().dispose().unwrap();
}

fn account(mode: &str) -> Value {
    json!({
        "applicationId": "app-1",
        "referrerID": "",
        "accountMode": "FUTURES",
        "positionMode": mode,
        "freeCollateral": 10.236,
        "totalAccountValue": 20.004,
        "totalCollateral": 15.5,
    })
}

fn errors(h: &Harness) -> Rc<RefCell<Vec<String>>> {
    let seen: Rc<RefCell<Vec<String>>> = Rc::default();
    let sink = seen.clone();
    h.venue().base().on(
        "error",
        Rc::new(move |_, data| {
            sink.borrow_mut().push(data.to_display());
            Ok(())
        }),
    );
    seen
}

fn json_of<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap()
}

fn seed_markets(h: &Harness) {
    let update = serde_json::from_value(json!({"markets": [{
        "id": "BTC/USDT:USDT", "symbol": "BTCUSDT", "base": "BTC", "quote": "USDT", "active": true,
        "precision": {"amount": 0.0001, "price": 0.1},
        "limits": {"amount": {"min": 0.0001, "max": 100}, "leverage": {"min": 1, "max": 50}},
    }]}))
    .unwrap();
    Store::update(h.store.as_ref(), update, false).unwrap();
}

fn no_positions(http: &binding_harness::Http) {
    http.ok("GET", POSITIONS, json!({"positions": []}));
}

#[test]
fn get_account_maps_application_and_absent_referrer() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.ok("GET", ACCOUNT, account("ONE_WAY"));
    });
    let result = h.drive(h.venue().get_account()).unwrap();
    assert_eq!(
        json_of(&result),
        json!({"userId": "app-1", "affiliateId": null})
    );
    let calls = h.http.calls(ACCOUNT);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].method, "GET");
}

#[test]
fn get_account_rejects_an_invalid_account_envelope() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.ok(
            "GET",
            ACCOUNT,
            json!({"applicationId": "app-1", "positionMode": "SIDEWAYS"}),
        );
    });
    let error = h.drive(h.venue().get_account()).unwrap_err();
    assert_eq!(
        error.message,
        "WOO X account info response has an invalid position mode"
    );
}

#[test]
fn get_account_rejects_with_the_failed_envelope_message() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.route(
            "GET",
            ACCOUNT,
            200.0,
            json!({"success": false, "msg": "  key revoked  "}),
        );
    });
    let error = h.drive(h.venue().get_account()).unwrap_err();
    assert_eq!(error.message, "  key revoked  ");
}

#[test]
fn validate_account_checks_the_key_then_reads_the_balance() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.ok("GET", ACCOUNT, account("HEDGE_MODE"));
    });
    let result = h.drive(h.venue().validate_account()).unwrap();
    assert_eq!(
        json_of(&result),
        json!({"error": "", "balance": {"free": 10.24, "total": 20.0, "used": 5.26, "upnl": 0.0}})
    );
    assert_eq!(
        h.http.calls(ACCOUNT).len(),
        2,
        "key check, then fetchBalance"
    );
}

#[test]
fn validate_account_reads_the_secondary_key_when_configured() {
    let mut opts: Value = serde_json::to_value(options()).unwrap();
    opts["secondary_key"] = json!("second-key");
    opts["secondary_secret"] = json!("second-secret");
    let opts: ExchangeOptions = serde_json::from_value(opts).unwrap();
    let h = harness(ExchangeName::Woo, opts, |http| {
        http.ok("GET", ACCOUNT, account("ONE_WAY"));
    });
    let result = h.drive(h.venue().validate_account()).unwrap();
    assert_eq!(json_of(&result)["error"], "");
    let keys: Vec<Value> = h
        .http
        .calls(ACCOUNT)
        .iter()
        .map(|r| r.headers["x-api-key"].clone())
        .collect();
    assert_eq!(
        keys,
        vec![
            json!("fixture-key"),
            json!("second-key"),
            json!("second-key")
        ]
    );
}

#[test]
fn validate_account_reports_the_rejection_message() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.route(
            "GET",
            ACCOUNT,
            401.0,
            json!({"success": false, "message": "the api key is wrong"}),
        );
    });
    let seen = errors(&h);
    let result = h.drive(h.venue().validate_account()).unwrap();
    assert_eq!(json_of(&result), json!({"error": "the api key is wrong"}));
    assert_eq!(
        h.http.calls(ACCOUNT).len(),
        1,
        "no balance read after a failed key check"
    );
    assert!(
        seen.borrow()
            .iter()
            .any(|e| e.contains("the api key is wrong")),
        "emitAndLogError carries the response message: {:?}",
        seen.borrow()
    );
}

#[test]
fn validate_account_balance_falls_back_to_the_store_outside_futures_mode() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        let mut data = account("ONE_WAY");
        data["accountMode"] = json!("MARGIN");
        http.ok("GET", ACCOUNT, data);
    });
    let seen = errors(&h);
    let result = h.drive(h.venue().validate_account()).unwrap();
    assert_eq!(
        json_of(&result),
        json!({"error": "", "balance": {"free": 0.0, "total": 0.0, "used": 0.0, "upnl": 0.0}})
    );
    assert!(
        seen.borrow()
            .iter()
            .any(|e| e.contains("WOO X account is in MARGIN mode, only FUTURES mode is supported")),
        "{:?}",
        seen.borrow()
    );
}

#[test]
fn fetch_markets_maps_perpetual_rows_only() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.route(
            "GET",
            MARKETS,
            200.0,
            json!({"success": true, "rows": [
                {"symbol": "SPOT_BTC_USDT", "base_imr": 0.1},
                {"symbol": "PERP_BTC_USDT", "base_imr": 0.02, "base_tick": 0.0001,
                 "quote_tick": 0.1, "base_min": 0.0001, "base_max": 100},
                {"symbol": "PERP_ETH_USDT", "base_tick": 0.001, "quote_tick": 0.01,
                 "base_min": 0.001, "base_max": 1000},
            ]}),
        );
    });
    let markets = h.drive(h.venue().fetch_markets()).unwrap();
    let markets = json_of(&markets);
    assert_eq!(markets.as_array().unwrap().len(), 2);
    let btc = &markets[0];
    assert_eq!(btc["id"], "BTC/USDT:USDT");
    assert_eq!(btc["symbol"], "BTCUSDT");
    assert_eq!(btc["base"], "BTC");
    assert_eq!(btc["quote"], "USDT");
    assert_eq!(btc["active"], true);
    assert_eq!(btc["precision"], json!({"amount": 0.0001, "price": 0.1}));
    assert_eq!(
        btc["limits"],
        json!({"amount": {"min": 0.0001, "max": 100.0}, "leverage": {"min": 1.0, "max": 50.0}})
    );
    assert_eq!(
        markets[1]["limits"]["leverage"]["max"], 100.0,
        "base_imr defaults to 0.01"
    );
    assert_eq!(h.http.calls(MARKETS)[0].method, "GET");
}

#[test]
fn fetch_markets_failure_returns_the_stored_markets() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.route(
            "GET",
            MARKETS,
            500.0,
            json!({"success": false, "message": "down"}),
        );
    });
    seed_markets(&h);
    let seen = errors(&h);
    let markets = h.drive(h.venue().fetch_markets()).unwrap();
    assert_eq!(json_of(&markets)[0]["symbol"], "BTCUSDT");
    assert!(
        seen.borrow().iter().any(|e| e.contains("down")),
        "{:?}",
        seen.borrow()
    );
}

#[test]
fn fetch_tickers_parses_rows_with_a_known_market() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.route(
            "GET",
            TICKERS,
            200.0,
            json!({"success": true, "rows": [
                {"symbol": "PERP_BTC_USDT", "24h_open": 100, "24h_close": 101.2345,
                 "index_price": 101.1, "mark_price": 101.2, "last_funding_rate": 0.0001,
                 "next_funding_time": 1700000000000.0, "24h_volume": 12, "24h_amount": 1200,
                 "open_interest": 7},
                {"symbol": "PERP_DOGE_USDT", "24h_open": 1, "24h_close": 1},
            ]}),
        );
    });
    seed_markets(&h);
    let tickers = json_of(&h.drive(h.venue().fetch_tickers()).unwrap());
    assert_eq!(
        tickers.as_array().unwrap().len(),
        1,
        "unknown markets are skipped"
    );
    let t = &tickers[0];
    assert_eq!(t["id"], "BTC/USDT:USDT");
    assert_eq!(t["symbol"], "BTCUSDT");
    for key in ["bid", "ask", "last"] {
        assert_eq!(t[key], 101.2345, "{key}");
    }
    assert_eq!(t["index"], 101.1);
    assert_eq!(t["mark"], 101.2);
    assert_eq!(t["percentage"], 1.23);
    assert_eq!(t["fundingRate"], 0.0001);
    assert_eq!(t["fundingTime"], 1700000000000.0);
    assert_eq!(t["volume"], 12.0);
    assert_eq!(t["quoteVolume"], 1200.0);
    assert_eq!(t["openInterest"], 7.0);
}

#[test]
fn fetch_tickers_failure_returns_the_stored_tickers() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.route(
            "GET",
            TICKERS,
            500.0,
            json!({"success": false, "message": "no tickers"}),
        );
    });
    let seen = errors(&h);
    let tickers = h.drive(h.venue().fetch_tickers()).unwrap();
    assert!(tickers.is_empty());
    assert!(
        seen.borrow().iter().any(|e| e.contains("no tickers")),
        "{:?}",
        seen.borrow()
    );
}

#[test]
fn set_leverage_posts_cross_both_sides() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.ok("POST", LEVERAGE, json!({}));
    });
    h.drive(h.venue().set_leverage("BTCUSDT".into(), 10.0, None))
        .unwrap();
    let calls = h.http.calls(LEVERAGE);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].method, "POST");
    assert_eq!(
        calls[0].data,
        json!("leverage=10&margin_mode=CROSS&position_side=BOTH&symbol=PERP_BTC_USDT"),
        "V1 signs the body as a sorted form"
    );
}

#[test]
fn set_leverage_failure_is_reported_and_rethrown() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.route(
            "POST",
            LEVERAGE,
            400.0,
            json!({"success": false, "message": "leverage too high"}),
        );
    });
    let seen = errors(&h);
    let error = h
        .drive(h.venue().set_leverage("BTCUSDT".into(), 200.0, None))
        .unwrap_err();
    assert!(!error.message.is_empty());
    assert!(
        seen.borrow()
            .iter()
            .any(|e| e.contains("leverage too high")),
        "{:?}",
        seen.borrow()
    );
}

#[test]
fn set_leverage_refreshes_cached_metadata_without_a_new_read() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.ok("GET", ACCOUNT, account("ONE_WAY"));
        http.ok("GET", POSITIONS, json!({"positions": []}));
        http.route(
            "GET",
            LEVERAGE,
            200.0,
            json!({"success": true, "data": {"details": [{"leverage": "5"}]}}),
        );
        http.ok("POST", LEVERAGE, json!({}));
    });
    let read = |h: &Harness| {
        json_of(
            &h.drive(h.venue().fetch_positions(Some(vec!["BTCUSDT".into()])))
                .unwrap(),
        )
    };
    let first = read(&h);
    assert_eq!(first[0]["leverage"], 5.0, "{first}");
    let get = h
        .http
        .calls(LEVERAGE)
        .iter()
        .filter(|r| r.method == "GET")
        .count();
    assert_eq!(get, 1);
    assert_eq!(
        h.http.calls(LEVERAGE)[0].params,
        json!({"symbol": "PERP_BTC_USDT", "position_mode": "ONE_WAY", "margin_mode": "CROSS"})
    );
    h.drive(h.venue().set_leverage("BTCUSDT".into(), 9.0, None))
        .unwrap();
    let second = read(&h);
    assert_eq!(second[0]["leverage"], 9.0, "{second}");
    let get = h
        .http
        .calls(LEVERAGE)
        .iter()
        .filter(|r| r.method == "GET")
        .count();
    assert_eq!(get, 1, "the cached metadata answered");
}

#[test]
fn change_position_mode_posts_then_confirms_by_readback() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.ok("POST", MODE, json!({}));
        http.ok("GET", ACCOUNT, account("HEDGE_MODE"));
        no_positions(http);
    });
    h.drive(h.venue().change_position_mode(true, None)).unwrap();
    let posts = h.http.calls(MODE);
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0].data, json!("position_mode=HEDGE_MODE"));
    assert!(
        !h.http.calls(ACCOUNT).is_empty(),
        "the readback reads account info"
    );
    assert!(
        !h.http.calls(POSITIONS).is_empty(),
        "positions are refreshed after readback"
    );
}

#[test]
fn change_position_mode_already_set_is_a_no_op_after_readback() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.route(
            "POST",
            MODE,
            400.0,
            json!({"success": false, "code": -1005, "error_code": 111000,
                   "message": "Position mode remains unchanged."}),
        );
        http.ok("GET", ACCOUNT, account("ONE_WAY"));
        no_positions(http);
    });
    let seen = errors(&h);
    h.drive(h.venue().change_position_mode(false, None))
        .unwrap();
    assert_eq!(h.http.calls(MODE)[0].data, json!("position_mode=ONE_WAY"));
    assert!(
        !h.http.calls(ACCOUNT).is_empty(),
        "already-set still confirms by readback"
    );
    assert!(
        seen.borrow().is_empty(),
        "no error emitted: {:?}",
        seen.borrow()
    );
}

#[test]
fn change_position_mode_refuses_with_pending_orders() {
    let message = "Position mode cannot be altered while there are pending orders.";
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.route(
            "POST",
            MODE,
            400.0,
            json!({"success": false, "code": -1005, "error_code": 111000, "message": message}),
        );
    });
    let seen = errors(&h);
    let error = h
        .drive(h.venue().change_position_mode(true, None))
        .unwrap_err();
    assert!(!error.message.is_empty());
    assert!(
        h.http.calls(ACCOUNT).is_empty(),
        "no readback after a refusal"
    );
    assert!(
        seen.borrow().iter().any(|e| e.contains(message)),
        "{:?}",
        seen.borrow()
    );
}

#[test]
fn change_position_mode_rejects_when_readback_never_matches() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.ok("POST", MODE, json!({}));
        http.ok("GET", ACCOUNT, account("ONE_WAY"));
    });
    let error = h
        .drive(h.venue().change_position_mode(true, None))
        .unwrap_err();
    assert_eq!(
        error.message,
        "WOO position mode readback stayed oneway; expected hedged"
    );
    assert_eq!(h.http.calls(ACCOUNT).len(), 3, "three readback attempts");
}
