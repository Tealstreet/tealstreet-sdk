#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod support;
use safe_cex_exchange::exchange::Exchange;
use safe_cex_exchange::js_runtime::value::to_typed;
use safe_cex_store::Store;
use safe_cex_types::*;
use serde_json::{Value, json};
use support::{Harness, harness, options};

include!("support/binding_streams.rs");
include!("support/binding_failures.rs");
include!("support/binding_factory.rs");
include!("support/private_lifecycle.rs");
include!("support/start_continuation.rs");
include!("support/placement_parity.rs");

fn raw_order() -> Value {
    json!({"id":"order-1","symbol":"BTC-USDC","type":"LIMIT","side":"BUY","status":"OPEN","timeInForce":"GOOD_TIL_CANCEL","price":"100.12","size":"1.234","cumSuccessFillSize":"0","reduceOnly":false,"triggerPrice":"0","openSlParams":{"triggerPrice":"0"},"openTpParams":{"triggerPrice":"0"}})
}
fn contract() -> Value {
    json!({"symbol":"BTC-USDC","crossSymbolName":"BTCUSDC","baseTokenId":"BTC","settleAssetId":"USDC","l2PairId":"11","enableTrade":true,"stepSize":"0.001","tickSize":"0.01","minOrderSize":"0.001","maxOrderSize":"1000","displayMinLeverage":"1","displayMaxLeverage":"50","initialMarginRate":"0.1"})
}
fn fixture() -> Harness {
    fixture_with_tick_interval(None)
}
fn fixture_with_tick_interval(tick_interval: Option<f64>) -> Harness {
    let mut opts = options();
    opts.extra = tick_interval
        .map(|interval| serde_json::from_value(json!({"tickInterval": interval})).unwrap());
    opts.omni_key_seed = Some(format!("0x{}1b", "01".repeat(64)));
    opts.passphrase = Some("fixture-passphrase".into());
    let h = harness(ExchangeName::Apexomni, opts, |http| {
        http.ok(
            "GET",
            "/v3/symbols",
            json!({"contractConfig":{"perpetualContract":[contract()]}}),
        );
        http.ok("GET", "/v3/account", json!({"id":"16909060","l2key":"fixture-l2","positions":[],"contractAccount":{"makerFeeRate":"0.0012","takerFeeRate":"0.0013"}}));
        http.ok(
            "GET",
            "/v3/account-balance",
            json!({"totalEquityValue":"100","maintenanceMargin":"2","initialMargin":"10"}),
        );
        http.ok("GET", "/v3/user", json!({"id":"16909060"}));
        http.ok("GET", "/v3/open-orders", json!([raw_order()]));
        http.ok("POST", "/v3/order", json!({"id":"9007199254740993"}));
        http.ok("POST", "/v3/delete-order", json!({}));
        http.ok("POST", "/v3/set-initial-margin-rate", json!({}));
        http.ok("POST", "/v3/delete-open-orders", json!({}));
    });
    Store::update(h.store.as_ref(), serde_json::from_value(json!({"markets":[{"id":"BTC-USDC","symbol":"BTCUSDC","base":"BTC","quote":"USDC","active":true,"precision":{"amount":0.001,"price":0.01},"limits":{"amount":{"min":0.001,"max":1000},"leverage":{"min":1,"max":50}},"info":contract()}]})).unwrap(), false).unwrap();
    h.store.update_zk_link_config(Some(json!({"id":"16909060","l2Key":"fixture-l2","contractAccount":{"makerFeeRate":"0.0012","takerFeeRate":"0.0013"},"contractConfig":{"perpetualContract":[contract()]}}))).unwrap();
    h
}
fn placed() -> PlaceOrderOpts {
    serde_json::from_value(json!({"symbol":"BTCUSDC","type":"limit","side":"buy","amount":1.2345,"price":100.127,"clientOrderId":"tscaller-1"})).unwrap()
}
fn done(h: &Harness) {
    h.venue().dispose().unwrap();
    h.runtime.pump();
}

#[test]
fn bound_start() {
    let h = fixture();
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    assert!(h.store.loaded().markets && h.store.loaded().tickers && h.store.loaded().balance);
    assert_eq!(h.runtime.sockets.borrow().len(), 2);
    done(&h);
}
#[test]
fn bound_get_account() {
    let h = fixture();
    assert_eq!(
        h.drive(h.venue().get_account()).unwrap().user_id,
        "16909060"
    );
    assert!(
        h.http.calls("/v3/account")[0].serializer_callable,
        "source query serializer survives typed interceptor"
    );
    done(&h);
}
#[test]
fn bound_validate_account() {
    let h = fixture();
    assert_eq!(
        serde_json::to_value(h.drive(h.venue().validate_account()).unwrap()).unwrap()["error"],
        ""
    );
    assert_eq!(h.http.calls("/v3/user").len(), 1);
    done(&h);
}
#[test]
fn bound_fetch_positions() {
    let h = fixture();
    let positions = h.drive(h.venue().fetch_positions(None)).unwrap();
    assert_eq!(positions.len(), 2);
    assert_eq!(positions[0].leverage, 10.);
    assert!(h.store.positions().is_empty());
    done(&h);
}
#[test]
fn bound_fetch_orders() {
    let h = fixture();
    let orders = h.drive(h.venue().fetch_orders(None)).unwrap();
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].id, "order-1");
    assert!(h.store.orders().is_empty());
    done(&h);
}
#[test]
fn bound_fetch_markets() {
    let h = fixture();
    let markets = h.drive(h.venue().fetch_markets()).unwrap();
    assert_eq!(markets.len(), 1);
    assert_eq!(markets[0].id, "BTC-USDC");
    assert_eq!(markets[0].precision.amount, 0.001);
    assert_eq!(markets[0].precision.price, 0.01);
    done(&h);
}
#[test]
fn bound_place_order() {
    let h = fixture();
    let source_clock = safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms();
    let ids = h
        .drive(h.exchange.base().place_order(h.exchange.as_ref(), placed()))
        .unwrap();
    assert_eq!(ids, ["9007199254740993"]);
    let calls = h.http.calls("/v3/order");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].data["size"], "1.235", "wire size uses amount step");
    assert_eq!(
        calls[0].data["price"], "100.13",
        "wire price uses price step"
    );
    assert_eq!(calls[0].data["clientOrderId"], "tscaller-1");
    assert_eq!(
        calls[0].data["signature"].as_str().unwrap().len(),
        128,
        "real native contract signature"
    );
    assert_eq!(calls[0].headers["APEX-API-KEY"], "fixture-key");
    let source = std::process::Command::new("node")
        .env("SAFE_CEX_VIRTUAL_TIME", source_clock.to_string())
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/binding-signature.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "actual source placement: {}",
        String::from_utf8_lossy(&source.stderr)
    );
    let source: Value = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(
        calls[0].data_raw, source["body"],
        "complete source/native signed placement wire body"
    );
    done(&h);
}
#[test]
fn bound_set_leverage() {
    let h = fixture();
    h.drive(h.venue().set_leverage("BTCUSDC".into(), 5., None))
        .unwrap();
    let calls = h.http.calls("/v3/set-initial-margin-rate");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].data["initialMarginRate"], "0.20000");
    done(&h);
}
#[test]
fn bound_update_order() {
    let h = fixture();
    let order = h.drive(h.venue().fetch_orders(None)).unwrap().remove(0);
    let opts =
        serde_json::from_value(json!({"order":order,"update":{"price":102.129,"amount":0.3456}}))
            .unwrap();
    assert_eq!(
        h.drive(h.venue().update_order(opts)).unwrap(),
        ["9007199254740993"]
    );
    assert_eq!(h.http.calls("/v3/delete-order").len(), 1);
    assert_eq!(h.http.calls("/v3/order")[0].data["price"], "102.13");
    done(&h);
}
#[test]
fn bound_cancel_orders() {
    let h = fixture();
    let orders = h.drive(h.venue().fetch_orders(None)).unwrap();
    h.drive(h.venue().cancel_orders(orders)).unwrap();
    assert_eq!(h.http.calls("/v3/delete-order")[0].data["id"], "order-1");
    done(&h);
}
// Independent named E2Es for each inherited rejection; controllers can mutate
// each trait override separately and cannot satisfy these through zero tests.
macro_rules! inherited_test {($name:ident, $method:ident $(, $arg:expr)*) => {#[test] fn $name() {let h=fixture(); assert_eq!(h.drive(h.venue().$method($($arg),*)).unwrap_err().message,"Not implemented"); assert!(h.http.requests.borrow().is_empty()); done(&h);}};}
inherited_test!(bound_fetch_tickers, fetch_tickers);
inherited_test!(
    bound_fetch_symbol_max_leverage,
    fetch_symbol_max_leverage,
    "BTCUSDC".into()
);
inherited_test!(bound_change_position_mode, change_position_mode, true, None);
inherited_test!(bound_change_margin_type, change_margin_type, true, None);
inherited_test!(bound_schedule_cancel, schedule_cancel, None);
inherited_test!(
    bound_fetch_public_trades,
    fetch_public_trades,
    "BTCUSDC".into(),
    None
);

#[test]
fn bound_amendment_omits_undefined_side() {
    let h = fixture();
    let order = h.drive(h.venue().fetch_orders(None)).unwrap().remove(0);
    let mut row = serde_json::to_value(order).unwrap();
    row.as_object_mut().unwrap().remove("side");
    let opts = serde_json::from_value(json!({"order":row,"update":{"price":102.129}})).unwrap();
    assert_eq!(
        h.drive(h.venue().update_order(opts)).unwrap(),
        ["9007199254740993"]
    );
    let calls = h.http.calls("/v3/order");
    assert_eq!(calls.len(), 1);
    assert!(!calls[0].data.as_object().unwrap().contains_key("side"));
    done(&h);
}

fn market_field_refusal_case(field: &str, wire_field: &str) {
    use std::{cell::RefCell, rc::Rc};
    let values = if field == "active" {
        vec![
            Some(json!({"asset":"wrong"})),
            Some(json!("truthy")),
            Some(json!(17)),
            Some(json!([])),
        ]
    } else {
        vec![
            Some(json!({"asset":"wrong"})),
            Some(json!(false)),
            Some(Value::Null),
            Some(json!(17)),
            None,
        ]
    };
    let mut count = 0;
    for value in values {
        for refresh in [false, true] {
            let h = fixture();
            h.http
                .routes
                .borrow_mut()
                .retain(|(method, path, _)| method != "GET" || path != "/v3/symbols");
            let mut bad = contract();
            bad["symbol"] = json!("BAD-USDC");
            bad["crossSymbolName"] = json!("BADUSDC");
            if let Some(value) = &value {
                bad[wire_field] = value.clone();
            } else {
                bad.as_object_mut().unwrap().remove(wire_field);
            }
            let mut eth = contract();
            eth["symbol"] = json!("ETH-USDC");
            eth["crossSymbolName"] = json!("ETHUSDC");
            eth["baseTokenId"] = json!("ETH");
            h.http.ok(
                "GET",
                "/v3/symbols",
                json!({"contractConfig":{"perpetualContract":[contract(),bad,eth]}}),
            );
            let logs = Rc::new(RefCell::new(Vec::new()));
            let capture = logs.clone();
            h.exchange.base().on_args(
                "log",
                Rc::new(move |_, args| {
                    capture.borrow_mut().push(args.to_vec());
                    Ok(())
                }),
            );
            let markets = if refresh {
                h.drive(h.venue().refresh_markets()).unwrap();
                h.store.markets().to_vec()
            } else {
                h.drive(h.venue().fetch_markets()).unwrap()
            };
            assert_eq!(
                markets
                    .iter()
                    .map(|m| m.symbol.as_str())
                    .collect::<Vec<_>>(),
                ["BTCUSDC", "ETHUSDC"],
                "bad{field} refresh={refresh}"
            );
            let logs = logs.borrow();
            let refusals = logs
                .iter()
                .filter(|args| {
                    to_typed::<String>(&args[0])
                        .unwrap()
                        .contains("Refused ApexOmni market row")
                })
                .collect::<Vec<_>>();
            assert_eq!(refusals.len(), 1, "visible per-row refusal");
            assert_eq!(to_typed::<String>(&refusals[0][1]).unwrap(), "warning");
            let message = to_typed::<String>(&refusals[0][0]).unwrap();
            assert!(
                message.contains("row 1") && message.contains(&format!("{field}=")),
                "{message}"
            );
            let expected = match value.as_ref() {
                None => "Undefined".into(),
                Some(Value::Null) => "Null".into(),
                Some(value) if value.is_object() => "wrong".into(),
                Some(value) if value.is_array() => "[]".into(),
                Some(value) if value.is_string() => value.as_str().unwrap().to_owned(),
                Some(value) => value.to_string(),
            };
            assert!(message.contains(&expected), "{message}");
            drop(logs);
            done(&h);
            count += 1;
        }
    }
    assert_eq!(count, if field == "active" { 8 } else { 10 });
}
#[test]
fn market_id_row_refusals_keep_siblings() {
    market_field_refusal_case("id", "symbol");
}
#[test]
fn market_symbol_row_refusals_keep_siblings() {
    market_field_refusal_case("symbol", "crossSymbolName");
}
#[test]
fn market_base_row_refusals_keep_siblings() {
    market_field_refusal_case("base", "baseTokenId");
}
#[test]
fn market_quote_row_refusals_keep_siblings() {
    market_field_refusal_case("quote", "settleAssetId");
}
#[test]
fn market_active_row_refusals_keep_siblings() {
    market_field_refusal_case("active", "enableTrade");
}

#[path = "../../parity/test-support/ticker_coercion.rs"]
mod ticker_coercion;

#[test]
fn production_ticker_coercion_matches_constructed_typescript() {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_store::StoreUpdate;
    use safe_cex_transport::socket::SocketFrame;
    let source = ticker_coercion::source("apexomni");
    assert_eq!(source["cases"].as_array().unwrap().len(), 64);
    let h = fixture();
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    let socket = public_socket(&h);
    for case in source["cases"].as_array().unwrap() {
        Store::update(
            h.store.as_ref(),
            StoreUpdate {
                tickers: Some(ticker_coercion::seed("apexomni")),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        for frame in case["frames"].as_array().unwrap() {
            socket
                .events
                .borrow_mut()
                .push_back(WsEvent::Message(SocketFrame::Text(
                    frame.as_str().unwrap().into(),
                )));
            h.runtime.pump();
        }
        ticker_coercion::assert_native(&h.store.tickers(), case);
    }
    assert!(
        h.runtime.reports.borrow().is_empty(),
        "{:?}",
        h.runtime.reports.borrow()
    );
    done(&h);
}
