//! Calls the production GRVT constructor and trait against scripted transports.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod harness;
use harness::*;
use safe_cex_types::*;
use serde_json::json;

#[test]
fn constructor_and_account_use_trading_gateway() {
    let h = harness(ExchangeName::Grvt, options(), |http| {
        http.route(
            "POST",
            "/lite/v1/account_summary",
            200.0,
            json!({"r":{"e":"101"}}),
        )
    });
    let account = h
        .drive(h.venue().get_account())
        .expect("production account binding");
    assert_eq!(account.user_id, "account-42");
    let requests = h.http.calls("/lite/v1/account_summary");
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].data, json!({"sub_account_id":"42"}));
    assert_eq!(requests[0].base_url, json!("https://trades.grvt.io"));
}

use safe_cex_store::{Store, StoreUpdate};
use serde_json::Value;
const SYMBOL: &str = "BTC_USDT_PERP";
fn instrument() -> Value {
    json!({"i":"BTC_USDT_Perp","k":"PERPETUAL","b":"BTC","q":"USDT","ms":"0.001","ts":"0.1","mp":"25","ih":"0x030501","bd":3,"qd":6})
}
fn source_order() -> Value {
    json!({"oi":"0xabcdef","sa":"42","im":false,"ti":"GOOD_TILL_TIME","po":false,"ro":false,"l":[{"i":"BTC_USDT_Perp","s":"0.125","lp":"50000","ib":true}],"m":{"co":"71","ct":"1700000000000000000"},"s1":{"s":"OPEN","ts":["0.025"],"bs":["0.1"]}})
}
fn script(http: &Http) {
    for (path, body) in [
        ("/auth/api_key/login", json!({})),
        ("/lite/v1/account_summary", json!({"r":{}})),
        ("/lite/v1/all_instruments", json!({"r":[instrument()]})),
        (
            "/lite/v1/funding_account_summary",
            json!({"r":{"ma":"account-42","te":"123","tc":"100"}}),
        ),
        (
            "/lite/v1/aggregated_account_summary",
            json!({"r":{"te":"123","ts2":"100","tu":"3","sb":[{"c":"USDT","b":"123"}]}}),
        ),
        (
            "/full/v1/get_authorized_builders",
            json!({"results":[{"builder_account_id":"0xee8f3559f298fffa79b8253437969d15b5dccff3","max_futures_fee_rate":"300"}]}),
        ),
        (
            "/lite/v1/get_all_initial_leverage",
            json!({"r":[{"i":"BTC_USDT_Perp","l":"7","ml":"1","ml1":"50"}]}),
        ),
        ("/lite/v1/set_initial_leverage", json!({"s":true})),
        (
            "/lite/v1/positions",
            json!({"r":[{"i":"BTC_USDT_Perp","s":"-0.5","ep":"49000","n":"24500","up":"-100","l":"7","el":"60000"}]}),
        ),
        ("/lite/v1/open_orders", json!({"r":[source_order()]})),
        (
            "/lite/v1/create_order",
            json!({"r":{"oi":"0xfeed","bf":"0.03"}}),
        ),
        ("/lite/v1/cancel_order", json!({"r":{}})),
        ("/lite/v1/cancel_all_orders", json!({"r":true})),
        (
            "/lite/v1/kline",
            json!({"r":[{"ot":"1700000000000000000","o":"1","h":"3","l":"0.5","c":"2","vb":"10"}]}),
        ),
        (
            "/lite/v1/book",
            json!({"r":{"bids":[{"price":"10","size":"2"}],"asks":[{"price":"11","size":"3"}]}}),
        ),
    ] {
        http.route("POST", path, 200.0, body);
    }
}
fn setup() -> Harness {
    harness(ExchangeName::Grvt, options(), script)
}
fn loaded(h: &Harness) {
    let markets = h
        .drive(h.venue().fetch_markets())
        .expect("fetch market fixture");
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .unwrap();
}
fn order_opts(id: &str) -> PlaceOrderOpts {
    serde_json::from_value(json!({"symbol":SYMBOL,"type":"limit","side":"buy","amount":0.125,"price":50000,"timeInForce":"GoodTillCancel","clientOrderId":id})).unwrap()
}
#[test]
fn placement_enforces_instrument_hash_length_before_builder_lookup_and_signing() {
    let compiler = std::env::var("SAFE_CEX_TYPESCRIPT_PATH").unwrap_or_else(|_| {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../node_modules/typescript").into()
    });
    let source = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/signing_metadata.cjs"
        ))
        .env("SAFE_CEX_TYPESCRIPT_PATH", compiler)
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let source: Value = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(source["rows"].as_array().unwrap().len(), 3204);
    for digits in [65, 64] {
        let hash = format!("0x{}", "f".repeat(digits));
        let source_rows: Vec<_> = source["rows"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| {
                let market: Value = serde_json::from_str(row["raw"].as_str().unwrap()).unwrap();
                market["info"]["ih"] == hash
                    && market["info"]["bd"].as_f64() == Some(3.0)
                    && market["info"]["qd"].as_f64() == Some(3.0)
            })
            .collect();
        assert_eq!(source_rows.len(), 1, "source boundary census");
        let source = source_rows[0];
        let h = setup();
        let mut market = instrument();
        market["ih"] = json!(hash);
        market["qd"] = json!(3);
        h.http
            .routes
            .borrow_mut()
            .retain(|(_, path, _)| path != "/lite/v1/all_instruments");
        h.http.route(
            "POST",
            "/lite/v1/all_instruments",
            200.0,
            json!({"r":[market]}),
        );
        loaded(&h);
        let result = h.drive(Box::pin(
            h.venue().base().place_order(h.venue(), order_opts("71")),
        ));
        if digits == 65 {
            let expected = "Market BTC_USDT_Perp does not have valid GRVT signing metadata (ih/bd/qd); refresh markets before placing an order";
            assert_eq!(source["error"]["name"], "Error");
            assert_eq!(
                source["error"]["message"]
                    .as_str()
                    .unwrap()
                    .replace("BTC mixed 😀", "BTC_USDT_Perp"),
                expected
            );
            assert_eq!(
                result.unwrap_err().message,
                expected,
                "placement hash-length boundary"
            );
            assert!(
                h.http.calls("/full/v1/get_authorized_builders").is_empty(),
                "refuse before builder authorization"
            );
            assert!(
                h.http.calls("/lite/v1/create_order").is_empty(),
                "refuse before order submission"
            );
        } else {
            assert_eq!(source["hash"], hash);
            assert_eq!(result.unwrap(), vec!["71"]);
            assert_eq!(h.http.calls("/lite/v1/create_order").len(), 1);
        }
    }
}
#[test]
fn fetch_markets_uses_public_gateway_and_keeps_signing_metadata() {
    let h = setup();
    let markets = h.drive(h.venue().fetch_markets()).unwrap();
    assert_eq!(markets.len(), 1);
    let market = &markets[0];
    assert_eq!(market.id, "BTC_USDT_Perp");
    assert_eq!(market.symbol, SYMBOL);
    assert_eq!(market.precision.amount, 0.001);
    assert!(
        market
            .info
            .as_ref()
            .unwrap()
            .get("ih")
            .unwrap()
            .text_equals("0x030501")
    );
    assert_eq!(
        h.http.calls("/lite/v1/all_instruments")[0].base_url,
        "https://market-data.grvt.io"
    );
}
#[test]
fn refresh_markets_replaces_metadata_when_instrument_ids_are_unchanged() {
    let h = setup();
    let mut changed = instrument();
    changed["ts"] = json!("0.25");
    changed["mp"] = json!("77");
    changed["ih"] = json!("0x030502");
    changed["bd"] = json!(4);
    changed["qd"] = json!(5);
    h.http.route(
        "POST",
        "/lite/v1/all_instruments",
        200.0,
        json!({"r":[changed]}),
    );
    loaded(&h);

    let changes = h.drive(h.venue().refresh_markets()).unwrap();
    assert!(changes.added.is_empty());
    assert!(changes.removed.is_empty());
    let markets = h.store.markets().unwrap();
    assert_eq!(markets.len(), 1);
    let market = &markets[0];
    assert_eq!(market.id, "BTC_USDT_Perp");
    assert_eq!(
        market.precision.price, 0.25,
        "refresh must replace metadata"
    );
    assert_eq!(
        safe_cex_num::numeric_value_to_number(&market.limits.amount.max),
        77.0
    );
    let info = market.info.as_ref().unwrap();
    assert!(info.get("ih").unwrap().text_equals("0x030502"));
    assert_eq!(
        info.get("bd").unwrap().as_f64().unwrap().to_bits(),
        4.0_f64.to_bits()
    );
    assert_eq!(
        info.get("qd").unwrap().as_f64().unwrap().to_bits(),
        5.0_f64.to_bits()
    );
    assert_eq!(h.http.calls("/lite/v1/all_instruments").len(), 2);
}
#[test]
fn refresh_markets_returns_added_and_removed_instruments_through_trait() {
    let h = setup();
    let mut added = instrument();
    added["i"] = json!("ETH_USDT_Perp");
    added["b"] = json!("ETH");
    added["ih"] = json!("0x030503");
    h.http.route(
        "POST",
        "/lite/v1/all_instruments",
        200.0,
        json!({"r":[added]}),
    );
    loaded(&h);

    let changes = h.drive(h.venue().refresh_markets()).unwrap();
    assert_eq!(changes.added.len(), 1);
    assert_eq!(changes.added[0].id, "ETH_USDT_Perp");
    assert!(
        changes.added[0]
            .info
            .as_ref()
            .unwrap()
            .get("ih")
            .unwrap()
            .text_equals("0x030503")
    );
    assert_eq!(changes.removed.len(), 1);
    assert_eq!(changes.removed[0].id, "BTC_USDT_Perp");
    assert!(
        changes.removed[0]
            .info
            .as_ref()
            .unwrap()
            .get("ih")
            .unwrap()
            .text_equals("0x030501")
    );
    assert_eq!(
        h.store.markets().unwrap().as_slice(),
        changes.added.as_slice()
    );
}
#[test]
fn fetch_tickers_uses_loaded_markets_without_http() {
    let h = setup();
    loaded(&h);
    let before = h.http.count();
    let rows = h.drive(h.venue().fetch_tickers()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "BTC_USDT_Perp");
    assert_eq!(rows[0].last, 0.0);
    assert_eq!(h.http.count(), before);
}
#[test]
fn validate_account_seeds_sdk_cookie_and_sends_null_funding_body() {
    let h = setup();
    let result = h.drive(h.venue().validate_account()).unwrap();
    assert_eq!(serde_json::to_value(result.error).unwrap(), json!(""));
    let calls = h.http.calls("/lite/v1/funding_account_summary");
    assert_eq!(calls.len(), 1);
    assert!(calls[0].data.is_null());
    assert_eq!(calls[0].headers["Cookie"], "gravity=fixture-session");
    assert_eq!(calls[0].headers["X-Grvt-Account-Id"], "account-42");
    assert_eq!(
        h.http.calls("/auth/api_key/login").len(),
        1,
        "authenticate seeds cookie; SDK must not log in again"
    );
}
#[test]
fn fetch_positions_maps_private_rows_and_preserves_source_filter_mapping() {
    let h = setup();
    loaded(&h);
    let before = h.store.positions().unwrap().to_vec();
    let rows = h
        .drive(h.venue().fetch_positions(Some(vec![SYMBOL.into()])))
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].symbol, SYMBOL);
    assert_eq!(rows[0].side, PositionSide::Short);
    assert_eq!(rows[0].contracts, 0.5);
    assert_eq!(rows[0].entry_price, 49000.0);
    assert_eq!(
        h.store.positions().unwrap().len(),
        before.len(),
        "fetch is pure"
    );
    let calls = h.http.calls("/lite/v1/positions");
    assert_eq!(calls[0].data["sa"], "42");
    assert_eq!(
        calls[0].data["undefined"],
        json!(["BTC_USDT_Perp"]),
        "ts-bug: SDK unknown instrument_ids key maps to undefined"
    );
}
#[test]
fn fetch_orders_maps_nonce_and_singleton_source_filter() {
    let h = setup();
    loaded(&h);
    let rows = h
        .drive(h.venue().fetch_orders(Some(vec![SYMBOL.into()])))
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "71");
    assert_eq!(rows[0].symbol, SYMBOL);
    assert_eq!(rows[0].amount, 0.125);
    assert_eq!(rows[0].filled, 0.025);
    assert!(h.store.orders().unwrap().is_empty(), "fetch is pure");
    assert_eq!(
        h.http.calls("/lite/v1/open_orders")[0].data,
        json!({"sa":"42","undefined":"BTC_USDT_Perp"})
    );
}
#[test]
fn place_order_signs_lite_sdk_payload_and_keeps_caller_id() {
    let h = setup();
    loaded(&h);
    let ids = h
        .drive(Box::pin(
            h.venue().base().place_order(h.venue(), order_opts("71")),
        ))
        .unwrap();
    assert_eq!(ids, vec!["71"]);
    let calls = h.http.calls("/lite/v1/create_order");
    assert_eq!(calls.len(), 1);
    let order = &calls[0].data["o"];
    assert_eq!(order["m"]["co"], "71");
    assert_eq!(order["l"][0]["i"], "BTC_USDT_Perp");
    assert_eq!(order["l"][0]["s"], "0.125");
    assert_eq!(order["s"]["ci"], "325");
    assert_eq!(order["s"]["r"].as_str().unwrap().len(), 66);
    assert_eq!(order["bf"], "0.03");
    assert_eq!(calls[0].headers["Cookie"], "gravity=fixture-session");
}
#[test]
fn place_orders_runs_source_sequential_batch() {
    let h = setup();
    loaded(&h);
    let ids = h
        .drive(Box::pin(h.venue().base().place_orders(
            h.venue(),
            vec![order_opts("71"), order_opts("72")],
        )))
        .unwrap();
    assert_eq!(ids, vec!["71", "72"]);
    let calls = h.http.calls("/lite/v1/create_order");
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].data["o"]["m"]["co"], "71");
    assert_eq!(calls[1].data["o"]["m"]["co"], "72");
}
#[test]
fn cancel_orders_reconciles_source_nonce_to_real_order_id() {
    let h = setup();
    loaded(&h);
    let orders = h.drive(h.venue().fetch_orders(None)).unwrap();
    h.drive(h.venue().cancel_orders(orders)).unwrap();
    let calls = h.http.calls("/lite/v1/cancel_order");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].data, json!({"sa":"42","oi":"0xabcdef"}));
}
#[test]
fn set_leverage_sends_sdk_request_and_updates_cache() {
    let h = setup();
    loaded(&h);
    h.drive(h.venue().set_leverage(SYMBOL.into(), 7.0, None))
        .unwrap();
    let calls = h.http.calls("/lite/v1/set_initial_leverage");
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].data,
        json!({"sa":"42","i":"BTC_USDT_Perp","l":"7"})
    );
    assert_eq!(
        h.store.fetched_leverage().unwrap()[SYMBOL].buy_leverage,
        7.0
    );
}
#[test]
fn update_order_validates_then_cancels_and_replaces() {
    let h = setup();
    loaded(&h);
    let orders = h.drive(h.venue().fetch_orders(None)).unwrap();
    let opts = serde_json::from_value(json!({"order":orders[0],"update":{"price":50100}})).unwrap();
    let ids = h.drive(h.venue().update_order(opts)).unwrap();
    assert_eq!(ids, vec!["500000000"]);
    assert_eq!(h.http.calls("/lite/v1/cancel_order").len(), 1);
    assert_eq!(
        h.http.calls("/lite/v1/create_order")[0].data["o"]["l"][0]["lp"],
        "50100"
    );
}

#[test]
fn money_amend_preflight_failure_uses_the_source_error_reporter() {
    let h = setup();
    let mut order: Order = serde_json::from_value(json!({
        "id":"71", "symbol":SYMBOL, "type":"limit", "side":"buy", "status":"open",
        "price":50000, "amount":0.125, "filled":0, "remaining":0.125,
        "reduceOnly":false, "lastUpdate":1
    }))
    .unwrap();
    order.time_in_force = ModelField::Undefined;
    let calls = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let saved = calls.clone();
    h.venue().base().on_args(
        "error",
        std::rc::Rc::new(move |_, args| {
            saved.borrow_mut().push(args.len());
            Ok(())
        }),
    );
    let update = serde_json::from_value(json!({"order":order,"update":{"price":50100}})).unwrap();
    let failure = h.drive(h.venue().update_order(update)).unwrap_err();
    assert_eq!(
        failure.message,
        "Cannot safely update GRVT order 71: missing time-in-force"
    );
    assert!(h.http.calls("/lite/v1/cancel_order").is_empty());
    assert!(h.http.calls("/lite/v1/create_order").is_empty());
    assert_eq!(
        *calls.borrow(),
        vec![2],
        "GRVT money-path catches must run the source reporter with metadata"
    );
    assert!(
        h.runtime
            .logs
            .borrow()
            .iter()
            .any(|line| { line.starts_with("error: exchange error Update order failed: ") })
    );
    assert!(h.runtime.logs.borrow().iter().any(|line| line == "trace: "));
}

#[test]
fn money_cancel_failures_use_the_source_error_reporter() {
    for symbol in [None, Some(SYMBOL)] {
        let h = setup();
        let path = "/lite/v1/cancel_all_orders";
        h.http.routes.borrow_mut().retain(|(_, p, _)| p != path);
        h.http.route(
            "POST",
            path,
            400.0,
            json!({"code":17,"message":"cancel denied"}),
        );
        let calls = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let saved = calls.clone();
        h.venue().base().on_args(
            "error",
            std::rc::Rc::new(move |_, args| {
                saved.borrow_mut().push(args.len());
                Ok(())
            }),
        );
        let result = match symbol {
            None => h.drive(h.venue().cancel_all_orders()),
            Some(symbol) => h.drive(h.venue().cancel_symbol_orders(symbol.into())),
        };
        assert!(result.is_err());
        assert_eq!(
            h.http.calls(path).len(),
            1,
            "one real SDK cancellation write"
        );
        assert!(!calls.borrow().is_empty());
        assert!(
            calls.borrow().iter().all(|count| *count == 2),
            "GRVT money cancellation reporter must retain metadata: {:?}",
            calls.borrow()
        );
        let prefix = symbol.map_or_else(
            || "Cancel all failed: ".to_owned(),
            |symbol| format!("Cancel {symbol} orders failed: "),
        );
        assert!(
            h.runtime
                .logs
                .borrow()
                .iter()
                .any(|line| { line.starts_with(&format!("error: exchange error {prefix}")) })
        );
    }
}
#[test]
fn start_public_only_loads_markets_tickers_and_opens_public_socket() {
    let mut options = options();
    options.public_data_only = Some(true);
    let h = harness(ExchangeName::Grvt, options, script);
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    assert!(
        h.store.loaded().unwrap().markets
            && h.store.loaded().unwrap().tickers
            && h.store.loaded().unwrap().positions
            && h.store.loaded().unwrap().orders
            && h.store.loaded().unwrap().balance
    );
    assert_eq!(h.store.markets().unwrap().len(), 1);
    assert_eq!(h.runtime.sockets.borrow().len(), 1);
    let socket = h.runtime.sockets.borrow()[0].1.clone();
    assert!(
        socket
            .sent
            .borrow()
            .iter()
            .any(|v| v["params"]["stream"] == "v1.ticker.d")
    );
    assert!(h.http.calls("/auth/api_key/login").is_empty());
}
#[test]
fn max_leverage_preserves_inherited_refusal() {
    let h = setup();
    assert_eq!(
        h.drive(h.venue().fetch_symbol_max_leverage(SYMBOL.into()))
            .unwrap_err()
            .message,
        "Not implemented"
    );
    assert_eq!(h.http.count(), 0);
}
#[test]
fn position_mode_preserves_inherited_refusal() {
    let h = setup();
    assert_eq!(
        h.drive(h.venue().change_position_mode(true, Some(SYMBOL.into())))
            .unwrap_err()
            .message,
        "Not implemented"
    );
    assert_eq!(h.http.count(), 0);
}
#[test]
fn margin_type_preserves_inherited_refusal() {
    let h = setup();
    assert_eq!(
        h.drive(h.venue().change_margin_type(true, Some(SYMBOL.into())))
            .unwrap_err()
            .message,
        "Not implemented"
    );
    assert_eq!(h.http.count(), 0);
}
#[test]
fn schedule_cancel_preserves_inherited_refusal() {
    let h = setup();
    assert_eq!(
        h.drive(h.venue().schedule_cancel(Some(Some(1.0))))
            .unwrap_err()
            .message,
        "Not implemented"
    );
    assert_eq!(h.http.count(), 0);
}
#[test]
fn public_trades_preserves_inherited_refusal() {
    let h = setup();
    assert_eq!(
        h.drive(h.venue().fetch_public_trades(SYMBOL.into(), Some(10.0)))
            .unwrap_err()
            .message,
        "Not implemented"
    );
    assert_eq!(h.http.count(), 0);
}
#[test]
fn invalid_caller_id_refuses_before_any_request() {
    let h = setup();
    for id in ["0", "00", "01", "-1", "9223372036854775808", "1.0"] {
        let error = h
            .drive(Box::pin(
                h.venue().base().place_order(h.venue(), order_opts(id)),
            ))
            .unwrap_err();
        assert!(error.message.contains("GRVT"), "{id}: {}", error.message);
    }
    assert_eq!(h.http.count(), 0);
}

#[test]
fn start_private_loads_pure_snapshots_and_tick_balance() {
    let h = setup();
    h.runtime.private_proxy.set(true);
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    assert_eq!(h.store.balance().unwrap().total, 123.0);
    assert_eq!(h.store.balance().unwrap().free, 100.0);
    assert_eq!(h.store.positions().unwrap()[0].contracts, 0.5);
    assert_eq!(h.store.orders().unwrap()[0].id, "71");
    assert_eq!(h.store.markets().unwrap()[0].limits.leverage.max, 50.0);
    assert_eq!(h.runtime.sockets.borrow().len(), 2);
    assert!(
        h.runtime.reports.borrow().is_empty(),
        "{:?}",
        h.runtime.reports.borrow()
    );
}
#[test]
fn private_socket_sends_auth_before_streams_and_recovers_once_then_fails_closed() {
    let h = setup();
    h.runtime.private_proxy.set(true);
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    let first = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url.ends_with("/private"))
        .unwrap()
        .1
        .clone();
    assert_eq!(first.sent.borrow().len(), 1);
    assert_eq!(first.sent.borrow()[0]["type"], "auth");
    assert_eq!(
        first.sent.borrow()[0]["session_cookie"],
        "rm=true; gravity=fixture-session"
    );
    first.events.borrow_mut().push_back(message(
        json!({"type":"error","message":"denied"}).to_string(),
    ));
    h.runtime.pump();
    h.runtime.advance(2000.0);
    let private: Vec<_> = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .filter(|(url, _)| url.ends_with("/private"))
        .map(|(_, s)| s.clone())
        .collect();
    assert_eq!(private.len(), 2, "one session refresh reconnect");
    assert!(first.closed.get());
    let second = &private[1];
    assert_eq!(second.sent.borrow().len(), 1);
    assert_eq!(second.sent.borrow()[0]["type"], "auth");
    second.events.borrow_mut().push_back(message(
        json!({"type":"error","message":"still denied"}).to_string(),
    ));
    h.runtime.pump();
    h.runtime.advance(10000.0);
    assert!(second.closed.get(), "second denial fails closed");
    assert_eq!(
        h.runtime
            .sockets
            .borrow()
            .iter()
            .filter(|(url, _)| url.ends_with("/private"))
            .count(),
        2,
        "no extra retry after terminal auth failure"
    );
    assert_eq!(
        h.http.calls("/auth/api_key/login").len(),
        2,
        "initial login plus one refresh"
    );
    for socket in private {
        assert!(
            socket.sent.borrow().iter().all(|v| v["type"] == "auth"),
            "auth failure sends no subscriptions or literal auth-failed frame"
        );
    }
}
#[test]
fn public_book_listener_maps_frame_and_unsubscribes() {
    use std::{cell::RefCell, rc::Rc};
    let mut opts = options();
    opts.public_data_only = Some(true);
    let h = harness(ExchangeName::Grvt, opts, script);
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    let books = Rc::new(RefCell::new(vec![]));
    let received = books.clone();
    let cleanup = h
        .venue()
        .listen_order_book(
            SYMBOL.into(),
            Rc::new(move |b| received.borrow_mut().push(b)),
        )
        .unwrap();
    let socket = h.runtime.sockets.borrow()[0].1.clone();
    socket.events.borrow_mut().push_back(message(json!({"s":"v1.book.s","f":{"i":"BTC_USDT_Perp","b":[{"p":"10","s":"2"},{"p":"9","s":"3"}],"a":[{"p":"11","s":"4"}]}}).to_string()));
    h.runtime.pump();
    assert_eq!(books.borrow().len(), 1);
    assert_eq!(books.borrow()[0].bids[1].total, 5.0);
    assert_eq!(books.borrow()[0].asks[0].price, 11.0);
    cleanup().unwrap();
    assert!(
        socket
            .sent
            .borrow()
            .iter()
            .any(|v| v["method"] == "unsubscribe")
    );
    h.venue().dispose().unwrap();
    assert!(h.venue().base().is_disposed());
    assert!(socket.closed.get());
}

fn message(value: String) -> safe_cex_exchange::websocket::WsEvent {
    safe_cex_exchange::websocket::WsEvent::Message(safe_cex_transport::socket::SocketFrame::Text(
        value,
    ))
}

#[test]
fn placement_reports_negative_half_builder_caps_with_source_rounding() {
    for (fee, cap, bits) in [
        ("-0.00015", "-1", (-1.0f64).to_bits()),
        ("-0.00005", "0", (-0.0f64).to_bits()),
        ("-0.00025", "-2", (-2.0f64).to_bits()),
    ] {
        let h = setup();
        h.http
            .routes
            .borrow_mut()
            .retain(|(_, path, _)| path != "/full/v1/get_authorized_builders");
        h.http.route("POST", "/full/v1/get_authorized_builders", 200.0, json!({"results":[{"builder_account_id":"0xee8f3559f298fffa79b8253437969d15b5dccff3","max_futures_fee_rate":fee}]}));
        loaded(&h);
        let error = h
            .drive(Box::pin(
                h.venue().base().place_order(h.venue(), order_opts("71")),
            ))
            .unwrap_err();
        assert_eq!(
            error.message,
            format!(
                "GRVT builder authorization cap ({cap}) is below required (300). Please re-authorize."
            ),
            "real placement exposes source-rounded cap for {fee}"
        );
        assert_eq!(h.http.calls("/full/v1/get_authorized_builders").len(), 1);
        assert!(h.http.calls("/lite/v1/create_order").is_empty());
        assert_eq!(
            safe_cex_venue_grvt::builder::grvt_builder_fee_rate_to_raw(fee)
                .unwrap()
                .to_bits(),
            bits,
            "accepted-contract bits retain negative zero"
        );
    }
}

fn private_started_for_frames() -> Harness {
    let h = setup();
    h.runtime.private_proxy.set(true);
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    h
}
fn private_frame(h: &Harness, frame: Value) {
    let socket = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url.ends_with("/private"))
        .unwrap()
        .1
        .clone();
    socket
        .events
        .borrow_mut()
        .push_back(message(frame.to_string()));
    h.runtime.pump();
}
#[test]
fn real_private_order_frame_populates_store() {
    let h = private_started_for_frames();
    let mut order = source_order();
    order["m"]["co"] = json!("72");
    private_frame(&h, json!({"s":"v1.order","f":order}));
    let orders = h.store.orders().unwrap();
    let actual = orders.iter().find(|o| o.id == "72").unwrap();
    assert_eq!(actual.symbol, SYMBOL);
    assert_eq!(actual.amount, 0.125);
    assert_eq!(actual.filled, 0.025);
    assert_eq!(actual.remaining, 0.1);
    drop(orders);
    h.venue().dispose().unwrap();
}
#[test]
fn real_private_fill_frame_emits_native_fill() {
    use std::{cell::RefCell, rc::Rc};
    let h = private_started_for_frames();
    let fills = Rc::new(RefCell::new(vec![]));
    let seen = fills.clone();
    h.venue().base().on(
        "fill",
        Rc::new(move |_, fill| {
            seen.borrow_mut().push(fill.clone());
            Ok(())
        }),
    );
    private_frame(
        &h,
        json!({"s":"v1.fill","f":{"i":"BTC_USDT_Perp","ib":true,"s":"0.125","p":"50001","ti":"native-fill-42","co":"71","oi":"0xabcdef"}}),
    );
    assert_eq!(fills.borrow().len(), 1);
    assert!(fills.borrow()[0].get("id").unwrap().strict_equals(
        &safe_cex_exchange::js_runtime::value::Js::str("native-fill-42")
    ));
    assert!(
        fills.borrow()[0]
            .get("orderId")
            .unwrap()
            .strict_equals(&safe_cex_exchange::js_runtime::value::Js::str("71"))
    );
    assert!(
        fills.borrow()[0]
            .get("symbol")
            .unwrap()
            .strict_equals(&safe_cex_exchange::js_runtime::value::Js::str(SYMBOL))
    );
    assert_eq!(
        fills.borrow()[0].get("amount").unwrap().as_f64(),
        Some(0.125)
    );
    h.venue().dispose().unwrap();
}
#[test]
fn real_private_position_frame_updates_positions_and_balance_upnl() {
    let h = private_started_for_frames();
    let row = json!({"i":"BTC_USDT_Perp","s":"0.25","ep":"50000","n":"12500","up":"12.5","l":"7","el":"40000"});
    h.http
        .routes
        .borrow_mut()
        .retain(|(_, path, _)| path != "/lite/v1/positions");
    h.http.route(
        "POST",
        "/lite/v1/positions",
        200.0,
        json!({"r":[row.clone()]}),
    );
    private_frame(&h, json!({"s":"v1.position","f":row}));
    let positions = h.store.positions().unwrap();
    let actual = positions.iter().find(|p| p.contracts == 0.25).unwrap();
    assert_eq!(actual.symbol, SYMBOL);
    assert_eq!(actual.entry_price, 50000.0);
    assert_eq!(actual.unrealized_pnl, 12.5);
    assert_eq!(h.store.balance().unwrap().upnl, 12.5);
    drop(positions);
    h.venue().dispose().unwrap();
}

#[test]
fn private_receive_watchdog_obeys_grvt_idle_thresholds() {
    let h = setup();
    h.runtime.private_proxy.set(true);
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    let private = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url.ends_with("/private"))
        .unwrap()
        .1
        .clone();
    private
        .events
        .borrow_mut()
        .push_back(message(json!({"type":"auth_success"}).to_string()));
    h.runtime.pump();
    let pings = || {
        private
            .sent
            .borrow()
            .iter()
            .filter(|value| **value == json!("ping"))
            .count()
    };
    let initial = pings();
    h.runtime.advance(8000.0);
    assert_eq!(
        pings(),
        initial,
        "GRVT private watchdog waits beyond base ping threshold"
    );
    h.runtime.advance(23001.0);
    assert_eq!(
        pings(),
        initial + 1,
        "GRVT private watchdog pings after 30 seconds"
    );
    assert!(
        h.runtime
            .logs
            .borrow()
            .iter()
            .any(|log| log.contains("[GRVT][PWS]") && log.contains("sending ping"))
    );
    h.runtime.advance(60000.0);
    assert!(
        h.runtime
            .logs
            .borrow()
            .iter()
            .any(|log| log.starts_with("warning:")
                && log.contains("[GRVT][PWS]")
                && log.contains("reconnecting")),
        "GRVT private watchdog reconnects with source warning after 90 seconds"
    );
    assert!(private.closed.get());
}

fn mixed_market_rows() -> Vec<Value> {
    let first = instrument();
    let mut bad = instrument();
    bad["i"] = json!("BROKEN_USDT_Perp");
    bad["b"] = json!({"unexpected":"asset"});
    let mut last = instrument();
    last["i"] = json!("ETH_USDT_Perp");
    last["b"] = json!("ETH");
    vec![first, bad, last]
}

#[test]
fn market_fetch_refuses_only_bad_asset_rows_and_reports_field_values() {
    for (field, value) in [
        ("b", Some(json!({"unexpected":"asset"}))),
        ("q", Some(json!(false))),
        ("b", Some(Value::Null)),
        ("q", Some(json!(17))),
        ("q", None),
    ] {
        let h = setup();
        let mut rows = mixed_market_rows();
        rows[1]["b"] = json!("BROKEN");
        if let Some(value) = &value {
            rows[1][field] = value.clone();
        } else {
            rows[1].as_object_mut().unwrap().remove(field);
        }
        h.http
            .routes
            .borrow_mut()
            .retain(|(_, path, _)| path != "/lite/v1/all_instruments");
        h.http
            .route("POST", "/lite/v1/all_instruments", 200., json!({"r":rows}));
        let markets = h
            .drive(h.venue().fetch_markets())
            .expect("one bad asset field must not reject valid markets");
        assert_eq!(
            markets.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            ["BTC_USDT_Perp", "ETH_USDT_Perp"]
        );
        assert_eq!(markets[0].base, "BTC");
        assert_eq!(markets[1].base, "ETH");
        let logs = h.runtime.logs.borrow();
        let report = logs
            .iter()
            .find(|line| line.contains("Refused GRVT market row"))
            .expect("row refusal must be visible");
        assert!(report.contains("BROKEN_USDT_Perp"), "{report}");
        assert!(
            report.contains(if field == "b" { "base=" } else { "quote=" }),
            "{report}"
        );
        let reported_value = value
            .as_ref()
            .map_or_else(|| "undefined".into(), Value::to_string);
        assert!(report.contains(&reported_value), "{report}");
        assert_eq!(
            logs.iter()
                .filter(|line| line.contains("Refused GRVT market row"))
                .count(),
            1
        );
    }
}

#[test]
fn market_refresh_commits_good_rows_around_a_refused_asset_row() {
    let h = setup();
    loaded(&h);
    h.http
        .routes
        .borrow_mut()
        .retain(|(_, path, _)| path != "/lite/v1/all_instruments");
    h.http.route(
        "POST",
        "/lite/v1/all_instruments",
        200.,
        json!({"r":mixed_market_rows()}),
    );
    let changes = h
        .drive(h.venue().refresh_markets())
        .expect("refresh keeps valid market metadata despite one refused row");
    assert_eq!(changes.added.len(), 1);
    assert_eq!(changes.added[0].id, "ETH_USDT_Perp");
    assert!(changes.removed.is_empty());
    let markets = h.store.markets().unwrap();
    assert_eq!(
        markets.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        ["BTC_USDT_Perp", "ETH_USDT_Perp"]
    );
    assert!(
        h.runtime
            .logs
            .borrow()
            .iter()
            .any(|line| line.contains("Refused GRVT market row"))
    );
}

fn candle_source(file: &str) -> Value {
    let output = std::process::Command::new("node")
        .arg(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests")
                .join(file),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn assert_candle_fields(candle: &Candle, expected: &Value, label: &str) {
    assert_eq!(expected.as_object().unwrap().len(), 6);
    for (field, actual) in [
        ("timestamp", candle.timestamp),
        ("open", candle.open),
        ("high", candle.high),
        ("low", candle.low),
        ("close", candle.close),
        ("volume", candle.volume),
    ] {
        if expected[field]["nan"] == true {
            assert!(actual.is_nan(), "{label}: native {field} NaN");
        } else {
            assert_eq!(
                format!("{:016x}", actual.to_bits()),
                expected[field]["bits"].as_str().unwrap(),
                "{label}: native {field}"
            );
        }
    }
}

#[test]
fn rest_candles_keep_source_aliases_time_units_and_distinct_fields() {
    let source = candle_source("data-reads-source.cjs");
    let rows = source["candleRows"].as_array().unwrap();
    assert_eq!(rows.len(), 17);
    for (name, count) in [
        ("full", 2),
        ("lite", 1),
        ("numeric", 1),
        ("defaults", 1),
        ("nan", 1),
    ] {
        let case = rows.iter().find(|row| row["name"] == name).unwrap();
        let h = setup();
        loaded(&h);
        h.http
            .routes
            .borrow_mut()
            .retain(|(_, path, _)| path != "/lite/v1/kline");
        h.http.route(
            "POST",
            "/lite/v1/kline",
            200.0,
            serde_json::from_str(case["raw"].as_str().unwrap()).unwrap(),
        );
        let candles = h
            .drive(
                h.venue().fetch_ohlcv(
                    serde_json::from_value(json!({
                        "symbol":SYMBOL,"interval":"5m","from":11,"to":23,"limit":17
                    }))
                    .unwrap(),
                ),
            )
            .unwrap();
        assert_eq!(candles.len(), count, "REST candle count {name}");
        assert_eq!(case["result"].as_array().unwrap().len(), count);
        for (candle, expected) in candles.iter().zip(case["result"].as_array().unwrap()) {
            assert_candle_fields(candle, expected, name);
        }
        let calls = h.http.calls("/lite/v1/kline");
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].data,
            json!({"i":"BTC_USDT_Perp","i1":"CI_5_M","t":"TRADE","st":"11000000","et":"23000000","l":17.0})
        );
    }
}

#[test]
fn socket_candles_keep_source_aliases_time_units_and_cleanup() {
    use std::{cell::RefCell, rc::Rc};
    let source = candle_source("public-feeds-source.cjs");
    let rows = source["candleListenerRows"].as_array().unwrap();
    assert_eq!(rows.len(), 12);
    for name in ["lite", "full", "numeric-time", "zero-fallback", "invalid"] {
        let case = rows.iter().find(|row| row["name"] == name).unwrap();
        let mut opts = options();
        opts.public_data_only = Some(true);
        let h = harness(ExchangeName::Grvt, opts, script);
        h.drive(h.venue().start()).unwrap();
        h.runtime.pump();
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        let cleanup = h
            .venue()
            .listen_ohlcv(
                serde_json::from_value(json!({"symbol":SYMBOL,"interval":"1m"})).unwrap(),
                Rc::new(move |c| sink.borrow_mut().push(c)),
            )
            .unwrap();
        let mut feed = case["input"].clone();
        let field = if feed.get("i").is_some() {
            "i"
        } else {
            "instrument"
        };
        feed[field] = json!("BTC_USDT_Perp");
        let frame = json!({"s":"v1.candle","s1":"BTC_USDT_Perp@CI_1_M-TRADE","f":feed}).to_string();
        let socket = h.runtime.sockets.borrow()[0].1.clone();
        socket.events.borrow_mut().push_back(message(frame.clone()));
        h.runtime.pump();
        assert_eq!(seen.borrow().len(), 1, "socket candle callback {name}");
        assert_eq!(case["candles"].as_array().unwrap().len(), 1);
        assert_candle_fields(&seen.borrow()[0], &case["candles"][0], name);
        let core = h
            .venue()
            .base()
            .get_socket(safe_cex_exchange::socket_lifecycle::SocketSlot::Public)
            .unwrap();
        let key = "kline.CI_1_M.BTC_USDT_Perp";
        assert_eq!(
            core.message_handlers_stats.borrow()[key].had_handlers,
            Some(true),
            "actual {name} candle registry owns its statistics"
        );
        assert!(matches!(
            core.message_handlers_stats.borrow()[key].last_callbacks_count,
            safe_cex_num::PrototypeScalar::Number(1.0)
        ));
        cleanup().unwrap();
        assert!(
            !core.message_handlers_stats.borrow().contains_key(key),
            "native cleanup removes statistics"
        );
        socket.events.borrow_mut().push_back(message(frame));
        h.runtime.pump();
        assert_eq!(
            seen.borrow().len(),
            1,
            "cleanup suppresses next candle {name}"
        );
        h.venue().dispose().unwrap();
    }
}

#[path = "../../parity/test-support/ticker_coercion.rs"]
mod ticker_coercion;

#[test]
fn production_ticker_coercion_matches_constructed_typescript() {
    use safe_cex_exchange::socket_lifecycle::SocketSlot;
    use safe_cex_store::{Store, StoreUpdate};
    use std::{cell::Cell, rc::Rc};
    let source = ticker_coercion::source("grvt");
    assert_eq!(source["cases"].as_array().unwrap().len(), 1000);
    for name in [ExchangeName::Grvt] {
        let mut opts = options();
        opts.public_data_only = Some(true);
        let h = harness(name, opts, script);
        h.drive(h.venue().start()).unwrap();
        for _ in 0..8 {
            h.runtime.pump();
        }
        let seed = ticker_coercion::seed("grvt");
        let markets = seed
            .iter()
            .map(|ticker| {
                serde_json::from_value(serde_json::json!({
            "id":ticker.id,"symbol":ticker.symbol,"base":"fixture","quote":"USDT","active":true,
            "precision":{"amount":0.01,"price":0.25},
            "limits":{"amount":{"min":0.01,"max":1000},"leverage":{"min":1,"max":20}}
        })).unwrap()
            })
            .collect();
        Store::update(
            h.store.as_ref(),
            StoreUpdate {
                markets: Some(markets),
                tickers: Some(seed.clone()),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        assert!(
            h.venue()
                .base()
                .get_socket(SocketSlot::Public)
                .unwrap()
                .is_connected()
        );
        let socket = h.runtime.sockets.borrow()[0].1.clone();
        let candle_callbacks = Rc::new(Cell::new(0));
        let cleanups: Vec<_> = seed
            .iter()
            .map(|ticker| {
                let received = candle_callbacks.clone();
                h.venue()
                    .listen_ohlcv(
                        serde_json::from_value(json!({"symbol":ticker.symbol,"interval":"1m"}))
                            .unwrap(),
                        Rc::new(move |_| received.set(received.get() + 1)),
                    )
                    .unwrap()
            })
            .collect();
        for case in source["cases"].as_array().unwrap() {
            let mut case_seed = seed.clone();
            for ticker in &mut case_seed {
                ticker.last = case["priorLast"].as_f64().unwrap();
            }
            candle_callbacks.set(0);
            Store::update(
                h.store.as_ref(),
                StoreUpdate {
                    tickers: Some(case_seed),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
            for frame in case["frames"].as_array().unwrap() {
                socket
                    .events
                    .borrow_mut()
                    .push_back(message(frame.as_str().unwrap().into()));
                h.runtime.pump();
            }
            assert!(
                h.runtime.reports.borrow().is_empty(),
                "ticker reports: {:?}",
                h.runtime.reports.borrow()
            );
            ticker_coercion::assert_native(&h.store.tickers().unwrap(), case);
            let expected_callbacks = if case["channel"].as_str().unwrap().starts_with("candle") {
                3
            } else {
                0
            };
            assert_eq!(
                candle_callbacks.get(),
                expected_callbacks,
                "native ticker candle listener count"
            );
            assert_eq!(
                case["candleCallbacks"].as_u64().unwrap(),
                expected_callbacks
            );
        }
        assert!(
            h.runtime.reports.borrow().is_empty(),
            "{:?}",
            h.runtime.reports.borrow()
        );
        for cleanup in cleanups {
            cleanup().unwrap();
        }
        h.venue().dispose().unwrap();
    }
}

#[test]
fn market_info_preserves_source_own_keys_and_native_metadata() {
    use safe_cex_exchange::js_runtime::value::{Js, from_json};
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    for metadata in [
        None,
        Some((
            Js::utf16(&[0xd800, 65, 0xdc00]),
            Js::Number(f64::INFINITY),
            Js::Number(-0.0),
        )),
        Some((Js::Undefined, Js::Number(f64::NAN), Js::Undefined)),
    ] {
        let h = setup();
        let raw = from_json(&instrument());
        for key in ["ih", "bd", "qd"] {
            raw.delete(&key.encode_utf16().collect::<Vec<_>>()).unwrap();
        }
        if let Some((ih, bd, qd)) = metadata.clone() {
            raw.define("ih", ih).unwrap();
            raw.define("bd", bd).unwrap();
            raw.define("qd", qd).unwrap();
        }
        h.http
            .routes
            .borrow_mut()
            .retain(|(_, path, _)| path != "/lite/v1/all_instruments");
        h.http.route_native(
            "POST",
            "/lite/v1/all_instruments",
            200.,
            Js::object(vec![("r", Js::array(vec![raw]))]),
        );
        loaded(&h);
        let markets = h.store.markets().unwrap();
        assert_eq!(markets.len(), 1, "real market fetch admits opaque metadata");
        let info = markets[0].info.as_ref().unwrap();
        assert_eq!(
            info.as_object().unwrap().len(),
            3,
            "source creates three own keys"
        );
        for key in ["ih", "bd", "qd"] {
            assert!(info.get(key).is_some(), "own {key}");
        }
        match metadata {
            None => {
                for key in ["ih", "bd", "qd"] {
                    assert!(matches!(info.get(key), Some(Input::Undefined)));
                }
            }
            Some((Js::Undefined, _, _)) => {
                assert!(matches!(info.get("ih"), Some(Input::Undefined)));
                assert!(info.get("bd").unwrap().as_f64().unwrap().is_nan());
                assert!(matches!(info.get("qd"), Some(Input::Undefined)));
            }
            Some(_) => {
                assert_eq!(
                    info.get("ih").unwrap().as_text().unwrap(),
                    &[0xd800, 65, 0xdc00]
                );
                assert_eq!(
                    info.get("bd").unwrap().as_f64().unwrap().to_bits(),
                    f64::INFINITY.to_bits()
                );
                assert_eq!(
                    info.get("qd").unwrap().as_f64().unwrap().to_bits(),
                    (-0.0f64).to_bits()
                );
            }
        }
    }
}
