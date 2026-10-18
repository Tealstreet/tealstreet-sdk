//! Production HyroTrader facade, with the same build-time origins as the source.
#[path = "support/binding_harness.rs"]
mod support;
use safe_cex_types::ExchangeName;
use serde_json::json;

fn configured() -> bool {
    safe_cex_venue_bybit::types::HYROTRADER_SIMULATOR_URL["livenet"]
        .as_str()
        .is_some_and(|value| !value.is_empty())
}

fn configured_child(name: &str) -> bool {
    if !configured() {
        // Cargo tracks option_env! inputs. The parent process retains its empty
        // source environment; this child exercises an independently built venue.
        assert!(std::env::var_os("BYBIT_HYRO_CONFIGURED_CHILD").is_none());
        use std::{
            fs,
            process::{Command, Stdio},
            sync::atomic::{AtomicU64, Ordering},
            time::{Duration, Instant, SystemTime, UNIX_EPOCH},
        };
        static NEXT_CAPTURE: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "bybit-hyro-configured-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT_CAPTURE.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&directory).expect("private configured-witness directory");
        let log_path = directory.join("cargo.log");
        let log = fs::File::create(&log_path).unwrap();
        let mut child = Command::new(std::env::var_os("CARGO").unwrap_or("cargo".into()))
            .args([
                "test",
                "--locked",
                "--jobs",
                "1",
                "--manifest-path",
                concat!(env!("CARGO_MANIFEST_DIR"), "/../Cargo.toml"),
                "-p",
                "safe-cex-venue-bybit",
                "--test",
                "hyro_binding",
                "--",
                "--exact",
                name,
                "--nocapture",
                "--test-threads=1",
            ])
            .env("BYBIT_HYRO_CONFIGURED_CHILD", "1")
            .env(
                "NEXT_PUBLIC_HYROTRADER_SIMULATOR_REST_URL",
                "https://hyro.example",
            )
            .env(
                "NEXT_PUBLIC_HYROTRADER_SIMULATOR_PUBLIC_WS_URL",
                "wss://hyro.example/v5/public/linear",
            )
            .env(
                "NEXT_PUBLIC_HYROTRADER_SIMULATOR_PRIVATE_WS_URL",
                "wss://hyro.example/v5/private",
            )
            .stdout(Stdio::from(log.try_clone().unwrap()))
            .stderr(Stdio::from(log))
            .spawn()
            .expect("configured native HyroTrader witness");
        let deadline = Instant::now() + Duration::from_secs(600);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!(
                    "configured native witness timed out; log {}",
                    log_path.display()
                );
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let output = fs::read_to_string(&log_path).unwrap();
        assert!(
            status.success(),
            "configured native witness failed ({}):\n{}",
            log_path.display(),
            output
        );
        assert!(output.contains("1 passed; 0 failed"));
        return true;
    }
    false
}

#[test]
fn hyro_constructor_matches_source_configuration_boundary() {
    let result = support::try_harness_as(ExchangeName::Hyrotrader, support::options(), |_| {});
    if configured() {
        let h = result.unwrap_or_else(|error| panic!("configured source constructor: {error}"));
        assert_eq!(h.venue().class_name(), "HyrotraderExchange");
        assert!(h.venue().base().is_demo);
        assert!(!h.venue().base().is_spot);
        assert!(h.wire.requests.borrow().is_empty());
        assert!(h.runtime.sockets.borrow().is_empty());
    } else {
        let error = match result {
            Ok(_) => panic!("unconfigured source HyroTrader constructor must refuse"),
            Err(error) => error,
        };
        assert_eq!(
            error.message,
            "HyroTrader simulator REST URL is not configured"
        );
    }
}

#[test]
fn hyro_configured_facade_preserves_metadata_and_mode_refusals() {
    if configured_child("hyro_configured_facade_preserves_metadata_and_mode_refusals") {
        return;
    }
    let h = support::harness_as(ExchangeName::Hyrotrader, support::options(), |_| {});
    assert_eq!(h.venue().class_name(), "HyrotraderExchange");
    assert!(h.venue().base().is_demo);
    assert_eq!(
        h.drive(h.venue().get_account()).unwrap_err().message,
        "HyroTrader does not provide account identity metadata"
    );
    h.drive(h.venue().change_position_mode(false, None))
        .unwrap();
    assert_eq!(
        h.drive(h.venue().change_position_mode(true, None))
            .unwrap_err()
            .message,
        "HyroTrader supports one-way position mode only"
    );
    h.drive(h.venue().change_margin_type(false, None)).unwrap();
    assert_eq!(
        h.drive(h.venue().change_margin_type(true, None))
            .unwrap_err()
            .message,
        "HyroTrader supports cross margin only"
    );
    assert!(
        h.wire.requests.borrow().is_empty(),
        "source metadata/mode guards do not send HTTP"
    );
    h.venue().dispose().unwrap();
}

#[test]
fn hyro_private_reads_are_validated_usdt_only_and_pure() {
    if configured_child("hyro_private_reads_are_validated_usdt_only_and_pure") {
        return;
    }
    let h =
        support::harness_as(ExchangeName::Hyrotrader, support::options(), |w| {
            w.ok(
                "GET",
                "/v5/order/realtime",
                json!({"list":[{
            "orderId":"hyro-order","orderLinkId":"sim-link","symbol":"BTCUSDT","side":"Buy",
            "orderType":"Limit","price":"25000","qty":"0.01","leavesQty":"0.01",
            "cumExecQty":"0","orderStatus":"New","timeInForce":"GTC","reduceOnly":false,
            "closeOnTrigger":false,"stopOrderType":"","triggerPrice":"","stopLoss":"",
            "takeProfit":"","tpslMode":"","createType":"CreateByUser","positionIdx":0,
            "triggerBy":"","slTriggerBy":"MarkPrice"
        }],"nextPageCursor":""}),
            );
            w.ok(
                "GET",
                "/v5/position/list",
                json!({"list":[{
            "symbol":"BTCUSDT","side":"Buy","size":"0.5","avgPrice":"25000","markPrice":"25010",
            "positionValue":"12500","leverage":"10","unrealisedPnl":"5","curRealisedPnl":"-1",
            "liqPrice":"20000","positionIdx":0,"positionIM":"1250","updatedTime":"1700000000000",
            "seq":7,"positionStatus":"Normal"
        }],"nextPageCursor":""}),
            );
            w.ok("GET", "/v5/account/wallet-balance", json!({"list":[{
            "accountType":"UNIFIED","totalEquity":"100","totalWalletBalance":"95",
            "totalMarginBalance":"100","totalAvailableBalance":"90","totalInitialMargin":"10",
            "totalMaintenanceMargin":"1","totalPerpUPL":"5","coin":[]
        }]}));
        });
    let orders = h
        .drive(h.venue().fetch_orders(None))
        .expect("validated Hyro orders");
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].id, "hyro-order");
    assert_eq!(orders[0].remaining, 0.01);
    let positions = h
        .drive(h.venue().fetch_positions(None))
        .expect("validated Hyro positions");
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].leverage, 10.);
    assert_eq!(positions[0].contracts, 0.5);
    let validation = h
        .drive(h.venue().validate_account())
        .expect("Hyro validation result");
    assert_eq!(
        validation.error,
        safe_cex_types::AccountValidationError::from("")
    );
    assert!(validation.balance.is_some());
    for endpoint in ["/v5/order/realtime", "/v5/position/list"] {
        let requests = h.wire.calls(endpoint);
        assert_eq!(
            requests.len(),
            1,
            "Hyro source reads only USDT: {requests:?}"
        );
        assert_eq!(requests[0].params["category"], "linear");
        assert_eq!(requests[0].params["settleCoin"], "USDT");
        assert_eq!(requests[0].params["limit"], "50");
    }
    assert!(h.store.orders().is_empty());
    assert!(h.store.positions().is_empty());
    assert_eq!(h.store.balance().total, 0.);
    h.venue().dispose().unwrap();
}

#[test]
fn hyro_malformed_private_rows_and_cursor_fail_without_publication() {
    if configured_child("hyro_malformed_private_rows_and_cursor_fail_without_publication") {
        return;
    }
    for (row, expected) in [
        (
            json!({"symbol":"BTCUSDT","side":"Buy","size":"1","positionIdx":0}),
            "HyroTrader position response is missing valid leverage",
        ),
        (
            json!({"symbol":"BTCUSDT","side":"Buy","size":"-1","positionIdx":0,"leverage":"10"}),
            "HyroTrader /v5/position/list returned a malformed row",
        ),
        (
            json!({"symbol":"BTCUSDC","side":"Buy","size":"1","positionIdx":0,"leverage":"10"}),
            "HyroTrader /v5/position/list returned a malformed row",
        ),
        (
            json!({"symbol":"BTCUSDT","side":"Buy","size":"1","positionIdx":0,"leverage":"NaN"}),
            "HyroTrader position response is missing valid leverage",
        ),
    ] {
        let h = support::harness_as(ExchangeName::Hyrotrader, support::options(), |w| {
            w.ok(
                "GET",
                "/v5/position/list",
                json!({"list":[row],"nextPageCursor":""}),
            );
        });
        assert_eq!(
            h.drive(h.venue().fetch_positions(None))
                .unwrap_err()
                .message,
            expected
        );
        assert_eq!(h.wire.calls("/v5/position/list").len(), 1);
        assert!(h.store.positions().is_empty());
        h.venue().dispose().unwrap();
    }
    let h = support::harness_as(ExchangeName::Hyrotrader, support::options(), |w| {
        w.ok(
            "GET",
            "/v5/order/realtime",
            json!({"list":[],"nextPageCursor":"stuck"}),
        );
    });
    assert_eq!(
        h.drive(h.venue().fetch_orders(None)).unwrap_err().message,
        "HyroTrader private snapshot cursor did not advance"
    );
    let calls = h.wire.calls("/v5/order/realtime");
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].params["cursor"], "");
    assert_eq!(calls[1].params["cursor"], "stuck");
    assert!(h.store.orders().is_empty());
    h.venue().dispose().unwrap();
    for (account, expected) in [
        (
            json!({"accountType":"CONTRACT","totalEquity":"100","totalWalletBalance":"95","coin":[]}),
            "HyroTrader wallet response is missing the UNIFIED account",
        ),
        (
            json!({"accountType":"UNIFIED","totalEquity":"100","totalWalletBalance":true,"coin":[]}),
            "HyroTrader wallet response contains invalid monetary fields",
        ),
    ] {
        let h = support::harness_as(ExchangeName::Hyrotrader, support::options(), |w| {
            w.ok(
                "GET",
                "/v5/account/wallet-balance",
                json!({"list":[account]}),
            );
        });
        let result = h.drive(h.venue().validate_account()).unwrap();
        assert_eq!(
            result.error,
            safe_cex_types::AccountValidationError::from(expected)
        );
        assert!(result.balance.is_none());
        assert_eq!(h.wire.requests.borrow().len(), 1);
        assert_eq!(h.store.balance().total, 0.);
        h.venue().dispose().unwrap();
    }
}

fn instrument(symbol: &str, quote: &str, status: &str, tick: &str) -> serde_json::Value {
    json!({"symbol":symbol,"contractType":"LinearPerpetual","status":status,
        "baseCoin":"BTC","quoteCoin":quote,"settleCoin":quote,"deliveryTime":"0",
        "priceFilter":{"tickSize":tick,"minPrice":"0.10","maxPrice":"199999.80"},
        "lotSizeFilter":{"qtyStep":"0.001","minOrderQty":"0.001","maxOrderQty":"100"},
        "leverageFilter":{"minLeverage":"1","maxLeverage":"100","leverageStep":"0.01"}})
}

#[test]
fn hyro_public_start_uses_simulator_usdt_markets_and_ignores_optional_categories() {
    if configured_child(
        "hyro_public_start_uses_simulator_usdt_markets_and_ignores_optional_categories",
    ) {
        return;
    }
    let mut options = support::options();
    options.public_data_only = Some(true);
    options.enable_option_markets = Some(true);
    options.enable_inverse_markets = Some(true);
    let h = support::harness_as(ExchangeName::Hyrotrader, options, |w| {
        w.ok(
            "GET",
            "/v5/market/instruments-info",
            json!({"list":[
            instrument("BTCUSDT","USDT","Trading","0.1"),
            instrument("BTCUSDC","USDC","Trading","0.1")],"nextPageCursor":""}),
        );
        w.route_when(
            "GET",
            "/v5/market/instruments-info",
            "status",
            "PreLaunch",
            json!({"retCode":0,"retMsg":"OK","result":{"list":[
                instrument("BTCUSDT","USDT","PreLaunch","0.5"),
                instrument("NEWUSDT","USDT","PreLaunch","0.2")],"nextPageCursor":""}}),
        );
        w.ok("GET", "/v5/market/tickers", json!({"category":"linear","list":[
            {"symbol":"BTCUSDT","lastPrice":"25000","bid1Price":"24999.9","ask1Price":"25000.1",
             "markPrice":"25000.5","indexPrice":"25001","price24hPcnt":"0.012","openInterest":"1000",
             "fundingRate":"0.0001","nextFundingTime":"1700003600000","volume24h":"10","turnover24h":"250000"},
            {"symbol":"NEWUSDT","lastPrice":"1","bid1Price":"1","ask1Price":"1","markPrice":"1",
             "indexPrice":"1","price24hPcnt":"0","openInterest":"0","fundingRate":"0",
             "nextFundingTime":"1700003600000","volume24h":"0","turnover24h":"0"}]}));
    });
    use safe_cex_exchange::socket_lifecycle::SocketSlot;
    assert!(
        h.venue()
            .base()
            .get_socket(SocketSlot::PublicSecondary)
            .is_none(),
        "the HyroTrader subclass disables inverse sockets even when the account flag is true"
    );
    h.drive(h.venue().start())
        .expect("source public-only HyroTrader startup");
    h.runtime.pump();
    let markets = h.store.markets();
    assert_eq!(
        markets
            .iter()
            .map(|m| m.symbol.as_str())
            .collect::<Vec<_>>(),
        ["BTCUSDT", "NEWUSDT"]
    );
    assert_eq!(
        markets[0].precision.price, 0.5,
        "source Map keeps the last duplicate value"
    );
    assert_eq!(h.store.tickers().len(), 2);
    let sockets = h.runtime.sockets.borrow();
    assert_eq!(sockets.len(), 1);
    assert!(
        sockets[0]
            .0
            .starts_with("wss://hyro.example/v5/public/linear?requestId="),
        "{}",
        sockets[0].0
    );
    assert!(
        h.wire
            .requests
            .borrow()
            .iter()
            .all(|r| r.params["category"] == "linear")
    );
    drop(sockets);
    drop(markets);
    h.venue().dispose().unwrap();
    assert!(
        h.runtime
            .sockets
            .borrow()
            .iter()
            .all(|(_, socket)| socket.closed.get())
    );
}

fn private_position(leverage: &str) -> serde_json::Value {
    json!({"symbol":"BTCUSDT","side":"Buy","size":"0.5","avgPrice":"25000","markPrice":"25010",
        "positionValue":"12500","leverage":leverage,"unrealisedPnl":"5","curRealisedPnl":"-1",
        "liqPrice":"20000","positionIdx":0,"positionIM":"1250","updatedTime":"1700000000000",
        "seq":7,"positionStatus":"Normal"})
}

fn private_fixture(w: &support::Wire) {
    w.ok(
        "GET",
        "/v5/market/instruments-info",
        json!({"list":[instrument("BTCUSDT","USDT","Trading","0.1")],"nextPageCursor":""}),
    );
    w.route_when(
        "GET",
        "/v5/market/instruments-info",
        "status",
        "PreLaunch",
        json!({"retCode":0,"retMsg":"OK","result":{"list":[],"nextPageCursor":""}}),
    );
    w.ok("GET", "/v5/market/tickers", json!({"category":"linear","list":[
        {"symbol":"BTCUSDT","lastPrice":"25000","bid1Price":"24999.9","ask1Price":"25000.1",
         "markPrice":"25000.5","indexPrice":"25001","price24hPcnt":"0.012","openInterest":"1000",
         "fundingRate":"0.0001","nextFundingTime":"1700003600000","volume24h":"10","turnover24h":"250000"}]}));
    w.ok(
        "GET",
        "/v5/order/realtime",
        json!({"list":[],"nextPageCursor":""}),
    );
    w.ok(
        "GET",
        "/v5/position/list",
        json!({"list":[private_position("10")],"nextPageCursor":""}),
    );
    w.ok(
        "GET",
        "/v5/account/wallet-balance",
        json!({"list":[{
        "accountType":"UNIFIED","totalEquity":"100","totalWalletBalance":"95",
        "totalMarginBalance":"100","totalAvailableBalance":"90","totalInitialMargin":"10",
        "totalMaintenanceMargin":"1","totalPerpUPL":"5","coin":[]}]}),
    );
}

fn frames(socket: &support::Socket, op: &str) -> Vec<serde_json::Value> {
    socket
        .sent
        .borrow()
        .iter()
        .map(|s| serde_json::from_str::<serde_json::Value>(s).unwrap())
        .filter(|value| value["op"] == op)
        .collect()
}

#[test]
fn hyro_private_session_awaits_both_acks_and_gates_position_delivery() {
    fn message(data: String) -> safe_cex_exchange::websocket::WsEvent {
        safe_cex_exchange::websocket::WsEvent::Message(
            safe_cex_transport::socket::SocketFrame::Text(data),
        )
    }
    if configured_child("hyro_private_session_awaits_both_acks_and_gates_position_delivery") {
        return;
    }
    use safe_cex_exchange::socket_lifecycle::SocketSlot;
    let h = support::harness_as(
        ExchangeName::Hyrotrader,
        support::options(),
        private_fixture,
    );
    h.drive(h.venue().start())
        .expect("HyroTrader private startup");
    h.advance(50.);
    let sockets = h.runtime.sockets.borrow();
    let (url, private) = sockets
        .iter()
        .find(|(url, _)| url.contains("/private"))
        .expect("private dial");
    assert!(
        url.starts_with("wss://hyro.example/v5/private?requestId="),
        "{url}"
    );
    let private = private.clone();
    drop(sockets);
    let core = h.venue().base().get_socket(SocketSlot::Private).unwrap();
    assert!(
        !core.is_recovery_complete(),
        "auth has not been acknowledged"
    );
    let auth = frames(&private, "auth");
    assert_eq!(auth.len(), 1);
    let auth_id = auth[0]["req_id"].as_str().expect("source auth request ID");
    assert_eq!(auth_id.len(), 21);
    assert!(frames(&private, "subscribe").is_empty());
    private.events.borrow_mut().push_back(message(
        json!({"topic":"position","data":[private_position("12")]}).to_string(),
    ));
    private.events.borrow_mut().push_back(message(
        json!({"op":"subscribe","req_id":auth_id,"success":true}).to_string(),
    ));
    h.runtime.pump();
    assert_eq!(
        h.store.positions()[0].leverage,
        10.,
        "pre-ready frames do not publish"
    );
    assert!(
        frames(&private, "subscribe").is_empty(),
        "the wrong acknowledgement operation is ignored"
    );
    private.events.borrow_mut().push_back(message(
        json!({"op":"auth","req_id":auth_id,"success":true}).to_string(),
    ));
    h.runtime.pump();
    let subscribe = frames(&private, "subscribe");
    assert_eq!(
        subscribe.len(),
        1,
        "reports={:?}; logs={:?}",
        h.runtime.reports.borrow(),
        h.runtime.logs.borrow()
    );
    assert_eq!(subscribe[0]["args"], json!(["order", "position"]));
    assert!(
        !core.is_recovery_complete(),
        "subscription has not been acknowledged"
    );
    private.events.borrow_mut().push_back(message(
        json!({"op":"subscribe","req_id":subscribe[0]["req_id"],"success":true}).to_string(),
    ));
    h.runtime.pump();
    assert!(
        core.is_recovery_complete(),
        "acknowledged subscription and stable REST reconciliation complete the session"
    );
    assert_eq!(h.store.balance().total, 95.);
    assert_eq!(h.store.balance().upnl, 5.);
    private.events.borrow_mut().push_back(message(
        json!({"topic":"position","data":[private_position("12")]}).to_string(),
    ));
    h.runtime.pump();
    assert_eq!(h.store.positions()[0].leverage, 12.);
    h.venue().dispose().unwrap();
    assert!(private.closed.get());
}

fn start_private() -> (
    support::Harness,
    std::rc::Rc<support::Socket>,
    std::rc::Rc<safe_cex_exchange::websocket::BaseWebSocket>,
) {
    let h = support::harness_as(
        ExchangeName::Hyrotrader,
        support::options(),
        private_fixture,
    );
    h.runtime.random_sequence.set(Some(0));
    h.drive(h.venue().start()).expect("Hyro startup");
    h.advance(50.0);
    let private = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url.contains("/private"))
        .unwrap()
        .1
        .clone();
    let core = h
        .venue()
        .base()
        .get_socket(safe_cex_exchange::socket_lifecycle::SocketSlot::Private)
        .unwrap();
    (h, private, core)
}
fn feed_private(h: &support::Harness, socket: &support::Socket, value: serde_json::Value) {
    socket
        .events
        .borrow_mut()
        .push_back(safe_cex_exchange::websocket::WsEvent::Message(
            safe_cex_transport::socket::SocketFrame::Text(value.to_string()),
        ));
    h.runtime.pump();
}
fn ack_private(h: &support::Harness, socket: &support::Socket, op: &str) {
    let sent = frames(socket, op);
    let last = sent.last().unwrap();
    feed_private(
        h,
        socket,
        json!({"op":op,"req_id":last["req_id"],"success":true}),
    );
}

#[test]
fn hyro_rejected_and_timed_out_auth_close_the_session_without_subscription() {
    if configured_child("hyro_rejected_and_timed_out_auth_close_the_session_without_subscription") {
        return;
    }
    for rejected in [true, false] {
        let (h, private, core) = start_private();
        let errors = std::rc::Rc::new(std::cell::RefCell::new(vec![]));
        let observed = errors.clone();
        h.venue().base().on(
            "error",
            std::rc::Rc::new(move |_, value| {
                observed.borrow_mut().push(value.clone());
                Ok(())
            }),
        );
        if rejected {
            let auth = frames(&private, "auth");
            feed_private(
                &h,
                &private,
                json!({"op":"auth","req_id":auth[0]["req_id"],"success":false,"ret_msg":"invalid fixture key"}),
            );
        } else {
            h.advance(10_050.0);
        }
        assert!(private.closed.get());
        assert!(!core.is_recovery_complete());
        assert!(frames(&private, "subscribe").is_empty());
        let expected = if rejected {
            "HyroTrader private auth rejected: invalid fixture key"
        } else {
            "HyroTrader private auth acknowledgement timed out"
        };
        assert!(
            errors.borrow().iter().any(|value| {
                value
                    .as_str()
                    .is_some_and(|text| String::from_utf16_lossy(text).contains(expected))
            }),
            "{:?}",
            errors.borrow()
        );
        assert_eq!(h.store.positions()[0].leverage, 10.0);
        h.venue().dispose().unwrap();
        h.advance(20_000.0);
        assert!(frames(&private, "subscribe").is_empty());
    }
}

#[test]
fn hyro_reconnect_ignores_old_acks_and_retired_socket_frames() {
    if configured_child("hyro_reconnect_ignores_old_acks_and_retired_socket_frames") {
        return;
    }
    let (h, old, core) = start_private();
    let old_auth = frames(&old, "auth")[0]["req_id"].clone();
    old.closed.set(true);
    old.events
        .borrow_mut()
        .push_back(safe_cex_exchange::websocket::WsEvent::Close {
            code: Some(1006),
            reason: "fixture outage".into(),
        });
    h.runtime.pump();
    h.advance(5_000.0);
    let private = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .rfind(|(url, _)| url.contains("/private"))
        .unwrap()
        .1
        .clone();
    assert!(!std::rc::Rc::ptr_eq(&old, &private));
    assert!(!core.is_recovery_complete());
    let new_auth = frames(&private, "auth");
    assert_eq!(new_auth.len(), 1);
    assert_ne!(new_auth[0]["req_id"], old_auth);
    feed_private(
        &h,
        &private,
        json!({"op":"auth","req_id":old_auth,"success":true}),
    );
    assert!(frames(&private, "subscribe").is_empty());
    ack_private(&h, &private, "auth");
    assert_eq!(frames(&private, "subscribe").len(), 1);
    feed_private(
        &h,
        &old,
        json!({"topic":"position","data":[private_position("99")]}),
    );
    assert_eq!(h.store.positions()[0].leverage, 10.0);
    ack_private(&h, &private, "subscribe");
    assert!(core.is_recovery_complete());
    h.venue().dispose().unwrap();
    h.advance(20_000.0);
    assert!(private.closed.get());
    assert!(
        h.runtime.reports.borrow().is_empty(),
        "{:?}",
        h.runtime.reports.borrow()
    );
}

#[test]
fn hyro_private_reconciliation_retries_changed_revision_and_publishes_atomically() {
    if configured_child(
        "hyro_private_reconciliation_retries_changed_revision_and_publishes_atomically",
    ) {
        return;
    }
    let (h, private, core) = start_private();
    let gate = std::rc::Rc::new(support::release::Release::default());
    h.wire
        .response_gates
        .borrow_mut()
        .push(("/v5/position/list".into(), gate.clone()));
    {
        let mut routes = h.wire.routes.borrow_mut();
        for (_, path, queue) in routes.iter_mut() {
            if path == "/v5/position/list" || path == "/v5/account/wallet-balance" {
                queue.clear();
            }
        }
    }
    for leverage in ["20", "30"] {
        h.wire.ok(
            "GET",
            "/v5/position/list",
            json!({"list":[private_position(leverage)],"nextPageCursor":""}),
        );
    }
    h.wire.ok("GET", "/v5/account/wallet-balance", json!({"list":[{"accountType":"UNIFIED","totalEquity":"200","totalWalletBalance":"195","totalMarginBalance":"200","totalAvailableBalance":"190","totalInitialMargin":"10","totalMaintenanceMargin":"1","totalPerpUPL":"5","coin":[]}]}));
    let snapshots = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
    let observed = snapshots.clone();
    let _unsubscribe = h.store.subscribe(std::sync::Arc::new(move |state, _| {
        observed.lock().unwrap().push((
            state.positions.first().map(|position| position.leverage),
            state.balance.total,
            state.balance.upnl,
        ));
        Ok(())
    }));
    let before = h.wire.calls("/v5/position/list").len();
    ack_private(&h, &private, "auth");
    ack_private(&h, &private, "subscribe");
    assert!(!core.is_recovery_complete(), "REST is still pending");
    assert_eq!(h.wire.calls("/v5/position/list").len(), before + 1);
    feed_private(
        &h,
        &private,
        json!({"topic":"position","data":[private_position("13")]}),
    );
    assert_eq!(h.store.positions()[0].leverage, 10.0);
    gate.set(true);
    h.runtime.pump();
    assert!(core.is_recovery_complete());
    assert_eq!(h.wire.calls("/v5/position/list").len(), before + 2);
    assert_eq!(h.store.positions()[0].leverage, 30.0);
    assert_eq!(h.store.balance().total, 195.0);
    let snapshots = snapshots.lock().unwrap();
    assert!(snapshots.contains(&(Some(30.0), 195.0, 5.0)));
    assert!(
        snapshots
            .iter()
            .all(|row| matches!(row, (Some(10.0), 95.0, 5.0) | (Some(30.0), 195.0, 5.0))),
        "{:?}",
        *snapshots
    );
    drop(snapshots);
    h.venue().dispose().unwrap();
}

fn place_hyro(
    h: &support::Harness,
    input: serde_json::Value,
) -> Result<Vec<String>, safe_cex_exchange::runtime::ExchangeError> {
    use safe_cex_exchange::exchange::Exchange;
    let opts: safe_cex_venue_bybit::order_params::BybitPlaceOrderOpts =
        serde_json::from_value(input).unwrap();
    h.drive(Box::pin(
        h.exchange.base().place_order(h.exchange.as_ref(), opts),
    ))
}
fn hyro_limit() -> serde_json::Value {
    json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.01,"price":25000.0})
}

#[test]
fn hyro_write_guards_precede_network_and_preserve_caller_id_precedence() {
    if configured_child("hyro_write_guards_precede_network_and_preserve_caller_id_precedence") {
        return;
    }
    let (h, private, _) = start_private();
    h.wire.ok(
        "POST",
        "/v5/order/create",
        json!({"orderId":"should-not-send"}),
    );
    let mut caller = hyro_limit();
    caller["clientOrderId"] = json!("forbidden");
    let error = place_hyro(&h, caller).expect_err("caller IDs must reject first");
    assert!(error.message.contains("clientOrderId"), "{}", error.message);
    let error =
        place_hyro(&h, hyro_limit()).expect_err("unacknowledged private session must block writes");
    assert!(
        error.message.contains("account is reconnecting"),
        "{}",
        error.message
    );
    assert!(h.wire.calls("/v5/order/create").is_empty());
    ack_private(&h, &private, "auth");
    ack_private(&h, &private, "subscribe");
    let mut slippage = hyro_limit();
    slippage["maxSlippagePct"] = json!(1.0);
    let error = place_hyro(&h, slippage).expect_err("source simulator slippage guard");
    assert!(
        error
            .message
            .contains("does not support client-defined slippage"),
        "{}",
        error.message
    );
    let mut bbo = hyro_limit();
    bbo["bybitBbo"] = json!({"sideType":"Queue","level":1});
    let error = place_hyro(&h, bbo).expect_err("source simulator BBO guard");
    assert!(
        error.message.contains("does not support Bybit BBO"),
        "{}",
        error.message
    );
    assert!(h.wire.calls("/v5/order/create").is_empty());
    h.venue().dispose().unwrap();
}

#[test]
fn hyro_order_submission_mints_source_identity_and_records_acknowledgement() {
    if configured_child("hyro_order_submission_mints_source_identity_and_records_acknowledgement") {
        return;
    }
    let (h, private, _) = start_private();
    ack_private(&h, &private, "auth");
    ack_private(&h, &private, "subscribe");
    h.wire
        .ok("POST", "/v5/order/create", json!({"orderId":"hyro-new-1"}));
    let ids = place_hyro(&h, hyro_limit()).expect("source recovery submission");
    assert_eq!(ids, ["hyro-new-1"]);
    let posts = h.wire.calls("/v5/order/create");
    assert_eq!(posts.len(), 1);
    let link = posts[0].body["orderLinkId"]
        .as_str()
        .expect("source requiresOrderLinkId uses canonical nanoid");
    assert_eq!(link.len(), 21);
    assert_eq!(posts[0].body["positionIdx"], 0);
    assert_eq!(h.store.orders()[0].id, "hyro-new-1");
    h.venue().dispose().unwrap();
}

#[test]
fn hyro_submission_disables_rejected_signature_replay() {
    if configured_child("hyro_submission_disables_rejected_signature_replay") {
        return;
    }
    let (h, private, _) = start_private();
    h.wire.reject_http_errors.set(true);
    ack_private(&h, &private, "auth");
    ack_private(&h, &private, "subscribe");
    h.wire.route(
        "POST",
        "/v5/order/create",
        400,
        json!({"retCode":10004,"retMsg":"error sign","result":{}}),
    );
    h.wire.ok(
        "POST",
        "/v5/order/create",
        json!({"orderId":"unsafe-replay"}),
    );
    let error = place_hyro(&h, hyro_limit())
        .expect_err("explicit axios-retry retries:0 keeps a definitive rejection");
    assert_eq!(error.message, "Request failed with status code 400");
    assert_eq!(h.wire.calls("/v5/order/create").len(), 1);
    assert!(h.store.orders().is_empty());
    h.venue().dispose().unwrap();
}

#[test]
fn hyro_uncertain_submission_recovers_terminal_identity_without_a_second_post() {
    if configured_child(
        "hyro_uncertain_submission_recovers_terminal_identity_without_a_second_post",
    ) {
        return;
    }
    use safe_cex_exchange::exchange::Exchange;
    use std::{
        collections::VecDeque,
        future::Future,
        rc::Rc,
        task::{Context, Poll, Waker},
    };
    let (h, private, _) = start_private();
    ack_private(&h, &private, "auth");
    ack_private(&h, &private, "subscribe");
    h.wire.route(
        "POST",
        "/v5/order/create",
        500,
        json!({"retCode":10016,"retMsg":"unconfirmed"}),
    );
    let gate = Rc::new(support::release::Release::default());
    h.wire
        .response_gates
        .borrow_mut()
        .push(("/v5/order/create".into(), gate.clone()));
    let opts: safe_cex_venue_bybit::order_params::BybitPlaceOrderOpts =
        serde_json::from_value(hyro_limit()).unwrap();
    let mut pending = Box::pin(h.exchange.base().place_order(h.exchange.as_ref(), opts));
    assert!(matches!(
        pending
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop())),
        Poll::Pending
    ));
    h.runtime.pump();
    let posts = h.wire.calls("/v5/order/create");
    assert_eq!(posts.len(), 1);
    let link = posts[0].body["orderLinkId"].clone();
    let body = json!({"retCode":0,"retMsg":"OK","result":{"list":[{"orderId":"recovered-filled","orderLinkId":link,"orderStatus":"Filled"}],"nextPageCursor":""}});
    let mut routes = h.wire.routes.borrow_mut();
    let (_, _, replies) = routes
        .iter_mut()
        .find(|(method, path, _)| method == "GET" && path == "/v5/order/realtime")
        .unwrap();
    *replies = VecDeque::from([(200, body)]);
    drop(routes);
    gate.set(true);
    let ids = h
        .drive(pending)
        .expect("source reconciles a terminal identity after uncertain acceptance");
    assert_eq!(ids, ["recovered-filled"]);
    assert_eq!(h.wire.calls("/v5/order/create").len(), 1);
    assert!(
        h.store
            .orders()
            .iter()
            .all(|order| order.id != "recovered-filled")
    );
    h.venue().dispose().unwrap();
}

#[test]
fn hyro_history_fallback_and_seconds_refusal_do_not_send_requests() {
    if configured_child("hyro_history_fallback_and_seconds_refusal_do_not_send_requests") {
        return;
    }
    let h = support::harness_as(ExchangeName::Hyrotrader, support::options(), |_| {});
    assert!(
        h.drive(h.venue().fetch_income_history(None))
            .unwrap()
            .is_empty()
    );
    assert!(
        h.drive(h.venue().fetch_income_history_range(0., 1., None))
            .unwrap()
            .is_empty()
    );
    let options = serde_json::from_value(json!({"symbol":"BTCUSDT","interval":"1s"})).unwrap();
    let error = h.drive(h.venue().fetch_ohlcv(options)).unwrap_err();
    assert_eq!(
        error.message,
        "HyroTrader seconds candles require a supported trade stream"
    );
    assert!(h.wire.requests.borrow().is_empty());
    h.venue().dispose().unwrap();
    let weak = std::rc::Rc::downgrade(&h.exchange);
    drop(h);
    assert!(
        weak.upgrade().is_none(),
        "receiver callbacks must not retain the native venue"
    );
}

#[test]
fn hyro_hosted_private_auth_preserves_bybit_scope_and_refuses_missing_signatures() {
    if configured_child(
        "hyro_hosted_private_auth_preserves_bybit_scope_and_refuses_missing_signatures",
    ) {
        return;
    }
    use safe_cex_exchange::runtime::{Cleanup, RemoteSigner};
    use safe_cex_transport::signing::{
        HttpSigningResponse, RemoteSignerRequest, RemoteSignerResponse, SignedConfig,
    };
    use std::{cell::RefCell, rc::Rc};
    struct Signer {
        calls: RefCell<Vec<serde_json::Value>>,
        failure: &'static str,
    }
    impl safe_cex_transport::signing::RemoteSigner for Signer {
        fn sign(
            &self,
            request: RemoteSignerRequest,
        ) -> safe_cex_transport::TransportFuture<'_, RemoteSignerResponse> {
            self.calls
                .borrow_mut()
                .push(serde_json::to_value(&request).unwrap());
            let response = match request {
                RemoteSignerRequest::Http(request) => {
                    RemoteSignerResponse::Http(HttpSigningResponse {
                        id: request.id,
                        config: Some(SignedConfig::default()),
                        error: None,
                    })
                }
                RemoteSignerRequest::WsAuth { request, .. } => {
                    RemoteSignerResponse::WsAuth(safe_cex_types::ws_auth::WsAuthResponse {
                        id: request.id,
                        ws_auth_signature: (self.failure.is_empty())
                            .then(|| "hosted-hyro-signature".into()),
                        error: (self.failure == "denied").then(|| "denied".into()),
                    })
                }
            };
            Box::pin(async move { Ok(response) })
        }
    }
    impl RemoteSigner for Signer {
        fn is_available(&self) -> bool {
            true
        }
        fn on_availability_change(&self, _: Rc<dyn Fn(bool)>) -> Cleanup {
            Box::new(|| {})
        }
    }
    for failure in ["", "denied", "missing"] {
        let signer = Rc::new(Signer {
            calls: RefCell::new(vec![]),
            failure,
        });
        let capability: Rc<dyn RemoteSigner> = signer.clone();
        let mut options = support::options();
        options.secret.clear();
        let h = support::harness_with_signer(
            ExchangeName::Hyrotrader,
            options,
            (capability, Some("hyro-account".into())),
            private_fixture,
        );
        h.drive(h.venue().start()).unwrap();
        h.advance(50.);
        let private = h
            .runtime
            .sockets
            .borrow()
            .iter()
            .find(|(url, _)| url.contains("/private"))
            .unwrap()
            .1
            .clone();
        let calls = signer.calls.borrow();
        let call = calls
            .iter()
            .find(|call| call["type"] == "ws-auth")
            .expect("actual source session invokes hosted WS signer");
        assert_eq!(call["exchangeName"], "bybit");
        assert_eq!(call["accountName"], "hyro-account");
        assert_eq!(call["wsAuth"]["outputFormat"], "hex");
        let expires = call["wsAuth"]["extraData"]["expires"].as_f64().unwrap() as u64;
        assert_eq!(call["wsAuth"]["toSign"], format!("GET/realtime{expires}"));
        let auth = frames(&private, "auth");
        if failure.is_empty() {
            assert_eq!(auth.len(), 1);
            assert_eq!(
                auth[0]["args"],
                json!(["fixture-key", expires, "hosted-hyro-signature"])
            );
            assert!(frames(&private, "subscribe").is_empty());
        } else {
            assert!(auth.is_empty());
            let message = if failure == "denied" {
                "Remote WS signing unavailable: denied"
            } else {
                "Remote WS signing unavailable: missing signature"
            };
            assert!(
                h.runtime
                    .logs
                    .borrow()
                    .iter()
                    .any(|line| line.contains(message)),
                "{:?}",
                h.runtime.logs.borrow()
            );
            assert!(private.closed.get());
        }
        drop(calls);
        h.venue().dispose().unwrap();
    }
}

#[test]
fn hyro_concurrent_identical_batches_share_one_submission_sequence() {
    if configured_child("hyro_concurrent_identical_batches_share_one_submission_sequence") {
        return;
    }
    use safe_cex_exchange::exchange::Exchange;
    use std::{
        future::Future,
        rc::Rc,
        task::{Context, Poll, Waker},
    };
    let (h, private, _) = start_private();
    ack_private(&h, &private, "auth");
    ack_private(&h, &private, "subscribe");
    h.wire.ok(
        "POST",
        "/v5/order/create",
        json!({"orderId":"hyro-batch-first"}),
    );
    h.wire.ok(
        "POST",
        "/v5/order/create",
        json!({"orderId":"hyro-batch-second"}),
    );
    let gate = Rc::new(support::release::Release::default());
    h.wire
        .response_gates
        .borrow_mut()
        .push(("/v5/order/create".into(), gate.clone()));
    let mut second = hyro_limit();
    second["price"] = json!(25001.0);
    let orders: Vec<safe_cex_venue_bybit::order_params::BybitPlaceOrderOpts> = vec![
        serde_json::from_value(hyro_limit()).unwrap(),
        serde_json::from_value(second).unwrap(),
    ];
    let mut first = Box::pin(
        h.exchange
            .base()
            .place_orders(h.exchange.as_ref(), orders.clone()),
    );
    let mut duplicate = Box::pin(h.exchange.base().place_orders(h.exchange.as_ref(), orders));
    let mut cx = Context::from_waker(Waker::noop());
    assert!(matches!(first.as_mut().poll(&mut cx), Poll::Pending));
    h.runtime.pump();
    assert!(matches!(duplicate.as_mut().poll(&mut cx), Poll::Pending));
    h.runtime.pump();
    assert_eq!(
        h.wire.calls("/v5/order/create").len(),
        1,
        "the same pending batch does not start a replacement"
    );
    gate.set(true);
    let ids = h.drive(first).unwrap();
    assert_eq!(ids, ["hyro-batch-first", "hyro-batch-second"]);
    assert_eq!(h.drive(duplicate).unwrap(), ids);
    assert_eq!(h.wire.calls("/v5/order/create").len(), 2);
    assert_eq!(h.store.orders().len(), 2);
    h.venue().dispose().unwrap();
}

#[test]
fn hyro_public_candles_use_authoritative_snapshots_without_trade_accumulation() {
    if configured_child(
        "hyro_public_candles_use_authoritative_snapshots_without_trade_accumulation",
    ) {
        return;
    }
    use safe_cex_exchange::socket_lifecycle::SocketSlot;
    use safe_cex_transport::socket::SocketFrame;
    use std::{cell::RefCell, rc::Rc};
    let (h, private, _) = start_private();
    ack_private(&h, &private, "auth");
    ack_private(&h, &private, "subscribe");
    let socket = h.venue().base().get_socket(SocketSlot::Public).unwrap();
    let wire = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url.contains("/public/linear"))
        .unwrap()
        .1
        .clone();
    let seen = Rc::new(RefCell::new(vec![]));
    let sink = seen.clone();
    let cleanup = h
        .venue()
        .listen_ohlcv(
            serde_json::from_value(json!({"symbol":"BTCUSDT","interval":"1m"})).unwrap(),
            Rc::new(move |candle| sink.borrow_mut().push(candle)),
        )
        .unwrap();
    assert!(
        !wire
            .sent
            .borrow()
            .iter()
            .any(|frame| frame.contains("publicTrade.BTCUSDT")),
        "Hyro snapshot streams never subscribe to trade-derived candles"
    );
    for volume in ["7", "8"] {
        socket.on_message(&SocketFrame::Text(json!({"topic":"kline.1.BTCUSDT","data":[{"start":1700000040000u64,"open":"100","high":"105","low":"99","close":"103","volume":volume}]}).to_string())).unwrap();
        socket.on_message(&SocketFrame::Text(json!({"topic":"publicTrade.BTCUSDT","data":[{"T":1700000041000u64,"p":"200","v":"1000","S":"Buy"}]}).to_string())).unwrap();
    }
    assert_eq!(seen.borrow().len(), 2);
    assert_eq!(seen.borrow()[0].volume, 7.);
    assert_eq!(seen.borrow()[1].volume, 8.);
    assert_eq!(seen.borrow()[1].close, 103.);
    cleanup().unwrap();
    assert!(
        !wire
            .sent
            .borrow()
            .iter()
            .any(|frame| frame.contains("publicTrade.BTCUSDT"))
    );
    let error = match h.venue().listen_ohlcv(
        serde_json::from_value(json!({"symbol":"BTCUSDT","interval":"1s"})).unwrap(),
        Rc::new(|_| panic!("seconds candle cannot emit")),
    ) {
        Ok(_) => panic!("source refuses seconds streams"),
        Err(error) => error,
    };
    assert_eq!(
        error.message,
        "HyroTrader seconds candles require a supported trade stream"
    );
    h.venue().dispose().unwrap();
}

#[test]
fn hyro_inherits_dynamic_history_except_explicit_empty_income() {
    if configured_child("hyro_inherits_dynamic_history_except_explicit_empty_income") {
        return;
    }
    use safe_cex_exchange::capabilities::{BaseCapabilityMethod, is_base_capability_fallback};
    let h = support::harness_as(ExchangeName::Hyrotrader, support::options(), |wire| {
        for path in [
            "/v5/position/closed-pnl",
            "/v5/order/history",
            "/v5/execution/list",
        ] {
            wire.ok("GET", path, json!({"list":[]}));
        }
    });
    let venue = h.venue();
    let caps = venue.capabilities();
    let options = json!({"symbol":"BTCUSDT","from":1,"to":2});
    assert!(
        h.drive(caps.fetch_closed_pnl(venue, serde_json::from_value(options.clone()).unwrap()))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        h.wire.calls("/v5/position/closed-pnl").len(),
        1,
        "inherited dynamic closed PnL must reach the simulator"
    );
    assert!(
        h.drive(caps.fetch_historical_orders(
            venue,
            Some(serde_json::from_value(options.clone()).unwrap())
        ))
        .unwrap()
        .is_empty()
    );
    assert_eq!(h.wire.calls("/v5/order/history").len(), 1);
    assert!(
        h.drive(caps.fetch_historical_fills(venue, serde_json::from_value(options).unwrap()))
            .unwrap()
            .is_empty()
    );
    assert_eq!(h.wire.calls("/v5/execution/list").len(), 1);
    assert!(!is_base_capability_fallback(
        &caps.fetch_closed_pnl,
        BaseCapabilityMethod::FetchClosedPnl
    ));
    assert!(!is_base_capability_fallback(
        &caps.fetch_historical_orders,
        BaseCapabilityMethod::FetchHistoricalOrders
    ));
    assert!(!is_base_capability_fallback(
        &caps.fetch_historical_fills,
        BaseCapabilityMethod::FetchHistoricalFills
    ));
    assert!(is_base_capability_fallback(
        &caps.fetch_income_history,
        BaseCapabilityMethod::FetchIncomeHistory
    ));
    assert!(is_base_capability_fallback(
        &caps.fetch_income_history_range,
        BaseCapabilityMethod::FetchIncomeHistoryRange
    ));
    assert!(
        h.drive(caps.fetch_income_history(venue, None))
            .unwrap()
            .is_empty()
    );
    assert!(
        h.drive(caps.fetch_income_history_range(venue, 1., 2., None))
            .unwrap()
            .is_empty()
    );
    assert!(h.wire.calls("/v5/account/transaction-log").is_empty());
    venue.dispose().unwrap();
}

#[test]
fn hyro_inherits_journal_closed_rows_and_order_join() {
    if configured_child("hyro_inherits_journal_closed_rows_and_order_join") {
        return;
    }
    use safe_cex_exchange::journal::HistoricalPositionOptions;
    let h = support::harness_as(ExchangeName::Hyrotrader, support::options(), |wire| {
        wire.ok("GET", "/v5/position/closed-pnl", json!({"list":[]}));
        wire.ok("GET", "/v5/order/history", json!({"list":[]}));
    });
    let rows = h
        .drive(
            h.venue()
                .fetch_historical_positions(HistoricalPositionOptions {
                    account: "hyro-account".into(),
                    from: Some(0.),
                    to: Some(10000.),
                }),
        )
        .unwrap();
    assert!(rows.is_empty());
    assert_eq!(h.wire.calls("/v5/position/closed-pnl").len(), 1);
    assert_eq!(h.wire.calls("/v5/order/history").len(), 1);
    assert!(h.wire.calls("/v5/account/transaction-log").is_empty());
    h.venue().dispose().unwrap();
}

#[test]
fn hyro_private_order_frames_publish_fills_and_ignore_wallet_execution_topics() {
    if configured_child(
        "hyro_private_order_frames_publish_fills_and_ignore_wallet_execution_topics",
    ) {
        return;
    }
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_transport::socket::SocketFrame;
    use std::{cell::RefCell, rc::Rc};
    let (h, private, _) = start_private();
    ack_private(&h, &private, "auth");
    ack_private(&h, &private, "subscribe");
    let fills = Rc::new(RefCell::new(vec![]));
    let observed = fills.clone();
    h.venue().base().on(
        "fill",
        Rc::new(move |_, value| {
            observed.borrow_mut().push(value.clone());
            Ok(())
        }),
    );
    let feed = |value: serde_json::Value| {
        private
            .events
            .borrow_mut()
            .push_back(WsEvent::Message(SocketFrame::Text(value.to_string())));
        h.runtime.pump();
    };
    let mut order = json!({"orderId":"hyro-frame","orderLinkId":"native-link","symbol":"BTCUSDT","side":"Buy",
        "orderType":"Limit","price":"25000","qty":"0.01","leavesQty":"0.01","cumExecQty":"0",
        "orderStatus":"New","timeInForce":"GTC","reduceOnly":false,"closeOnTrigger":false,
        "stopOrderType":"","triggerPrice":"","stopLoss":"","takeProfit":"","tpslMode":"",
        "createType":"CreateByUser","positionIdx":0,"triggerBy":"","slTriggerBy":"MarkPrice","rejectReason":"EC_NoError"});
    feed(json!({"topic":"order","data":[order.clone()]}));
    assert_eq!(h.store.orders().len(), 1);
    assert_eq!(h.store.orders()[0].id, "hyro-frame");
    order["orderStatus"] = json!("PartiallyFilled");
    order["cumExecQty"] = json!("0.004");
    order["cumExecValue"] = json!("100");
    order["avgPrice"] = json!("25000");
    feed(json!({"topic":"order","data":[order.clone()]}));
    feed(json!({"topic":"order","data":[order.clone()]}));
    assert_eq!(
        fills
            .borrow()
            .iter()
            .map(safe_cex_exchange::js_runtime::value::to_json)
            .collect::<Result<Vec<_>, _>>()
            .expect("finite Hyro fill fixture"),
        vec![
            json!({"id":"hyro-frame","side":"buy","symbol":"BTCUSDT","price":25000.0,"amount":0.004})
        ]
    );
    order["orderStatus"] = json!("Filled");
    order["cumExecQty"] = json!("0.01");
    order["cumExecValue"] = json!("250");
    feed(json!({"topic":"order","data":[order]}));
    assert_eq!(fills.borrow().len(), 2);
    assert_eq!(
        h.store.orders()[0].status,
        safe_cex_types::OrderStatus::Closed
    );
    feed(json!({"topic":"position","data":[private_position("12")]}));
    assert_eq!(h.store.positions()[0].leverage, 12.);
    let balance = serde_json::to_value(&*h.store.balance()).unwrap();
    let requests = h.wire.requests.borrow().len();
    feed(json!({"topic":"wallet","data":[{"totalEquity":"999999"}]}));
    feed(
        json!({"topic":"execution","data":[{"orderId":"hyro-frame","execQty":"999","execPrice":"1"}]}),
    );
    assert_eq!(serde_json::to_value(&*h.store.balance()).unwrap(), balance);
    assert_eq!(fills.borrow().len(), 2);
    assert_eq!(h.wire.requests.borrow().len(), requests);
    h.venue().dispose().unwrap();
}

#[test]
fn hyro_option_reads_reject_with_inherited_source_guard() {
    if configured_child("hyro_option_reads_reject_with_inherited_source_guard") {
        return;
    }
    let mut options = support::options();
    options.enable_option_markets = Some(true);
    let h = support::harness_as(ExchangeName::Hyrotrader, options, |_| {});
    let source = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/hyro_option_refusal_source.cjs"
        ))
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let expected: serde_json::Value = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(expected["calls"], 0);
    let mut failures = vec![];
    for dynamic in [false, true] {
        let v = h.venue();
        let query = serde_json::from_value(json!({"underlying":"BTC"})).unwrap();
        let results = [
            h.drive(if dynamic {
                v.capabilities().fetch_option_chain(v, query)
            } else {
                v.fetch_option_chain(query)
            })
            .map(|_| ()),
            h.drive(if dynamic {
                v.capabilities().fetch_option_expiries(v, "BTC".into())
            } else {
                v.fetch_option_expiries("BTC".into())
            })
            .map(|_| ()),
            h.drive(if dynamic {
                v.capabilities().fetch_account_greeks(v)
            } else {
                v.fetch_account_greeks()
            })
            .map(|_| ()),
        ];
        for (result, row) in results
            .into_iter()
            .zip(expected["rows"].as_array().unwrap())
        {
            let error =
                result.expect_err("Hyro source disables options even when inherited methods exist");
            if error.name != row["name"].as_str().unwrap()
                || error.message != row["message"].as_str().unwrap()
            {
                failures.push(format!(
                    "dynamic={dynamic} {}: {}: {}; expected {row}",
                    row["method"], error.name, error.message
                ));
            }
        }
    }
    assert!(
        h.wire.requests.borrow().is_empty(),
        "source guard must reject before HTTP"
    );
    h.venue().dispose().unwrap();
    assert!(
        failures.is_empty(),
        "Hyro inherited option refusals: {}",
        failures.join("\n")
    );
}

#[test]
fn hyro_private_positions_update_balance_upnl_without_wallet_or_fill_events() {
    if configured_child("hyro_private_positions_update_balance_upnl_without_wallet_or_fill_events")
    {
        return;
    }
    use std::{cell::Cell, rc::Rc};
    let (h, private, _) = start_private();
    ack_private(&h, &private, "auth");
    ack_private(&h, &private, "subscribe");
    let before = h.store.balance().clone();
    assert_eq!(before.upnl, 5.0);
    let fills = Rc::new(Cell::new(0));
    let observed = fills.clone();
    h.venue().base().on(
        "fill",
        Rc::new(move |_, _| {
            observed.set(observed.get() + 1);
            Ok(())
        }),
    );
    let mut row = private_position("12");
    row["unrealisedPnl"] = json!("17.25");
    row["size"] = json!("0.75");
    let requests = h.wire.requests.borrow().len();
    feed_private(&h, &private, json!({"topic":"position","data":[row]}));
    assert_eq!(h.store.positions()[0].contracts, 0.75);
    assert_eq!(h.store.positions()[0].leverage, 12.0);
    assert_eq!(h.store.positions()[0].unrealized_pnl, 17.25);
    let mut expected = before;
    expected.upnl = 17.25;
    assert_eq!(
        *h.store.balance(),
        expected,
        "acknowledged Hyro position frame updates only the unified balance PnL"
    );
    feed_private(
        &h,
        &private,
        json!({"topic":"wallet","data":[{
        "accountType":"UNIFIED","totalEquity":"999999","totalPerpUPL":"999"}]}),
    );
    assert_eq!(
        *h.store.balance(),
        expected,
        "source does not subscribe to wallet updates"
    );
    assert_eq!(fills.get(), 0, "position and wallet frames emit no fills");
    assert_eq!(
        h.wire.requests.borrow().len(),
        requests,
        "private frame values did not come from REST"
    );
    assert!(h.runtime.reports.borrow().is_empty());
    h.venue().dispose().unwrap();
}
