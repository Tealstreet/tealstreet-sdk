//! Native query-order identity and authoritative absence; deterministic API schemas.
use support::Options;
mod support;
use safe_cex_engine::{Command, Event, Method, Query};

use serde::Deserialize;
use support::Harness;

const ROW: &str = r#"{"orderId":9007199254740993,"symbol":"BTCUSDT","status":"PARTIALLY_FILLED","clientOrderId":"caller:/1","price":"100","avgPrice":"99","origQty":"2","executedQty":"0.5","type":"LIMIT","origType":"LIMIT","side":"BUY","positionSide":"BOTH","reduceOnly":true,"time":1710000000000,"updateTime":1710000000123,"timeInForce":"GTX"}"#;
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Lookup {
    found: bool,
    definitive: Option<bool>,
    order: Option<Order>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Order {
    id: String,
    symbol: String,
    client_order_id: String,
    status: String,
    amount: f64,
    filled: f64,
    remaining: f64,
    reduce_only: bool,
    last_update: f64,
    time_in_force: String,
    info: Info,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Info {
    order_id: u64,
    position_side: String,
}
fn ready(pm: bool) -> Harness {
    ready_with_window(pm, None)
}
fn ready_with_window(pm: bool, recv_window: Option<f64>) -> Harness {
    let mut h = Harness::started(Options {
        key: Some("lookup-key".into()),
        read_credentials: Some(safe_cex_engine::ReadCredentials {
            key: "secondary-key".into(),
            secret: "secondary-secret".into(),
            passphrase: Some("secondary-pass".into()),
        }),
        secret: Some("lookup-secret".into()),
        portfolio_margin: pm,
        recv_window,
        ..Options::default()
    });
    h.load_public();
    h.requests.clear();
    h.done.clear();
    h.sent.clear();
    h
}
fn query(h: &mut Harness, symbol: &str, id: &str) {
    h.event(Event::Command(Command::Query {
        command: 991,
        query: Query::OrderByClientId {
            symbol: symbol.into(),
            client_order_id: id.into(),
        },
    }));
}
fn result(h: &mut Harness) -> Result<Lookup, String> {
    let (_, value) = h.done.pop().expect("query completes");
    value.map(|text| serde_json::from_str(&text).unwrap())
}
fn cached_order(id: &str, client_order_id: &str) -> safe_cex_engine::model::Order {
    safe_cex_engine::model::Order {
        id: id.into(),
        parent_id: None,
        status: safe_cex_engine::model::OrderStatus::Open,
        symbol: "BTCUSDT".into(),
        kind: safe_cex_engine::model::OrderType::Limit,
        side: safe_cex_engine::model::OrderSide::Buy,
        price: 100.0,
        amount: 9.0,
        filled: 0.0,
        remaining: 9.0,
        reduce_only: false,
        close: None,
        client_order_id: Some(client_order_id.into()),
        time_in_force: None,
        last_update: 123.0,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    }
}

#[test]
fn secondary_account_cache_never_answers_primary_lookup() {
    for pm in [false, true] {
        for outcome in ["found", "absent", "auth", "network"] {
            let mut h = ready(pm);
            // A successful secondary-account snapshot can contain the same
            // account-scoped client ID as the primary placement account.
            h.engine.store_mut().set_orders(vec![cached_order(
                if pm { "caller:/1" } else { "secondary-order" },
                "caller:/1",
            )]);
            let before = h.engine.snapshot_frame(42.0).to_vec();
            query(&mut h, "BTCUSDT", "caller:/1");
            assert_eq!(h.requests.len(), 1, "{pm}/{outcome}");
            let r = h.requests.pop().unwrap();
            assert_eq!(
                r.route, None,
                "prepared fixture transport carries no route override"
            );
            assert!(
                r.headers
                    .iter()
                    .any(|(k, v)| *k == "X-MBX-APIKEY" && v == "lookup-key")
            );
            let (_, signed) = r.url.split_once('?').unwrap();
            let (payload, signature) = signed.rsplit_once("&signature=").unwrap();
            assert_eq!(
                signature,
                safe_cex_crypto::hmac_sha256(
                    b"lookup-secret",
                    payload.as_bytes(),
                    safe_cex_crypto::Encoding::Hex,
                )
            );
            match outcome {
                "found" => h.reply(r.id, 200, "", ROW),
                "absent" => h.reply(r.id, 400, "", r#"{"code":-2013,"msg":"missing"}"#),
                "auth" => h.reply(r.id, 401, "", r#"{"code":-2015,"msg":"bad key"}"#),
                _ => h.event(Event::HttpFailed {
                    request: r.id,
                    error: "network unavailable",
                }),
            }
            let lookup = result(&mut h);
            match outcome {
                "found" => {
                    let order = lookup.unwrap().order.unwrap();
                    assert_eq!(order.amount, 2.0);
                    assert_eq!(order.info.order_id, 9_007_199_254_740_993);
                }
                "absent" => {
                    let lookup = lookup.unwrap();
                    assert!(!lookup.found);
                    assert_eq!(lookup.definitive, Some(true));
                    assert!(lookup.order.is_none());
                }
                _ => assert!(lookup.is_err()),
            }
            assert!(h.requests.is_empty());
            assert!(h.sent.is_empty());
            assert_eq!(before, h.engine.snapshot_frame(42.0));
        }
    }
}

#[test]
fn numeric_client_id_lookup_keeps_pm_canonical_identity_usable_after_reconciliation() {
    for pm in [false, true] {
        let mut h = ready(pm);
        query(&mut h, "BTCUSDT", "12345");
        let r = h.requests.pop().unwrap();
        h.reply(r.id, 200, "", &ROW.replace("caller:/1", "12345"));
        let recovered = result(&mut h).unwrap().order.unwrap();
        let canonical_id = if pm { "12345" } else { "9007199254740993" };
        assert_eq!(recovered.id, canonical_id);
        assert_eq!(recovered.client_order_id, "12345");
        assert_eq!(recovered.info.order_id, 9_007_199_254_740_993);
        // The ordinary snapshot uses the canonical ID; cancellation by the
        // recovered ID must find that row and preserve the venue's identity axis.
        h.engine
            .store_mut()
            .set_orders(vec![cached_order(canonical_id, "12345")]);
        h.event(Event::Command(Command::CancelOrders {
            command: 992,
            ids: vec![recovered.id],
        }));
        assert_eq!(h.requests.len(), 1);
        let r = h.requests.pop().unwrap();
        assert_eq!(r.method, Method::Delete);
        assert!(r.url.contains(if pm {
            "origClientOrderId=12345&"
        } else {
            "orderId=9007199254740993&"
        }));
    }
}
#[test]
fn lookup_routes_signed_primary_usdm_and_pm_and_keeps_native_identity() {
    for pm in [false, true] {
        let mut h = ready(pm);
        let before = h.engine.snapshot_frame(42.0).to_vec();
        query(&mut h, "BTCUSDT", "caller:/1");
        assert_eq!(h.requests.len(), 1);
        let r = h.requests.pop().unwrap();
        assert_eq!(r.method, Method::Get);
        assert_eq!(
            r.route, None,
            "prepared fixture transport carries no route override"
        );
        assert!(r.url.starts_with(if pm { "https://papi.binance.com/papi/v1/um/order?symbol=BTCUSDT&origClientOrderId=caller%3A%2F1&" } else { "https://fapi.binance.com/fapi/v1/order?symbol=BTCUSDT&origClientOrderId=caller%3A%2F1&" }));
        let (_, signed) = r.url.split_once('?').unwrap();
        let (payload, signature) = signed.rsplit_once("&signature=").unwrap();
        assert_eq!(
            signature,
            safe_cex_crypto::hmac_sha256(
                b"lookup-secret",
                payload.as_bytes(),
                safe_cex_crypto::Encoding::Hex
            )
        );
        assert!(
            r.headers
                .iter()
                .any(|(k, v)| *k == "X-MBX-APIKEY" && v == "lookup-key")
        );
        assert!(r.body.is_none());
        h.reply(r.id, 200, "", ROW);
        let result = result(&mut h).unwrap();
        assert!(result.found);
        assert_eq!(result.definitive, None);
        let o = result.order.unwrap();
        assert_eq!(o.id, if pm { "caller:/1" } else { "9007199254740993" });
        assert_eq!(o.info.order_id, 9_007_199_254_740_993);
        assert_eq!(o.symbol, "BTCUSDT");
        assert_eq!(o.client_order_id, "caller:/1");
        assert_eq!(o.status, "open");
        assert_eq!(o.amount, 2.0);
        assert_eq!(o.filled, 0.5);
        assert_eq!(o.remaining, 1.5);
        assert!(o.reduce_only);
        assert_eq!(o.last_update, 1_710_000_000_123.0);
        assert_eq!(o.time_in_force, "PostOnly");
        assert_eq!(o.info.position_side, "BOTH");
        assert!(h.requests.is_empty());
        assert!(h.sent.is_empty());
        assert_eq!(before, h.engine.snapshot_frame(42.0));
    }
}
#[test]
fn lookup_keeps_open_filled_and_all_native_cancellation_statuses() {
    for (native, expected) in [
        ("NEW", "open"),
        ("PARTIALLY_FILLED", "open"),
        ("FILLED", "closed"),
        ("CANCELED", "canceled"),
        ("EXPIRED", "canceled"),
        ("EXPIRED_IN_MATCH", "canceled"),
        ("REJECTED", "canceled"),
    ] {
        let mut h = ready(false);
        query(&mut h, "BTCUSDT", "caller:/1");
        let r = h.requests.pop().unwrap();
        h.reply(r.id, 200, "", &ROW.replace("PARTIALLY_FILLED", native));
        assert_eq!(result(&mut h).unwrap().order.unwrap().status, expected);
    }
}
#[test]
fn only_native_order_does_not_exist_is_a_definitive_miss() {
    for (status, body, definitive) in [
        (400, r#"{"code": -2013,"msg":"Order does not exist"}"#, true),
        (200, r#"{"code":-2013,"msg":"missing"}"#, true),
        (500, r#"{"code":-2013,"msg":"missing"}"#, false),
        (401, r#"{"code":-2013,"msg":"missing"}"#, false),
        (403, r#"{"code":-2013,"msg":"missing"}"#, false),
        (408, r#"{"code":-2013,"msg":"missing"}"#, false),
        (429, r#"{"code":-2013,"msg":"missing"}"#, false),
        (400, r#"{"code":-2011,"msg":"unknown order"}"#, false),
        (401, r#"{"code":-2015,"msg":"bad credentials"}"#, false),
        (429, r#"{"code":-1003,"msg":"rate limited"}"#, false),
        (500, "upstream failed", false),
    ] {
        let mut h = ready(false);
        query(&mut h, "BTCUSDT", "caller:/1");
        let r = h.requests.pop().unwrap();
        h.reply(r.id, status, "", body);
        if definitive {
            let value = result(&mut h).unwrap();
            assert!(!value.found);
            assert_eq!(value.definitive, Some(true));
            assert!(value.order.is_none());
        } else {
            assert!(result(&mut h).is_err(), "{body}");
        }
    }
}
#[test]
fn malformed_or_wrong_identity_response_never_reports_absence() {
    for body in [
        "null".into(),
        "{}".into(),
        ROW.replace("caller:/1", "other"),
        ROW.replace("BTCUSDT", "ETHUSDT"),
        ROW.replace("\"origQty\":\"2\"", "\"origQty\":\"NaN\""),
        ROW.replace("\"executedQty\":\"0.5\"", "\"executedQty\":\"3\""),
        ROW.replace("PARTIALLY_FILLED", "NEW_UNKNOWN_STATUS"),
    ] {
        let mut h = ready(false);
        query(&mut h, "BTCUSDT", "caller:/1");
        let r = h.requests.pop().unwrap();
        h.reply(r.id, 200, "", &body);
        assert!(result(&mut h).is_err(), "{body}");
    }
}
#[test]
fn missing_market_bad_id_and_missing_credentials_do_not_send_requests() {
    let mut h = ready(false);
    query(&mut h, "NOTLISTED", "caller:/1");
    let value = result(&mut h).unwrap();
    assert!(!value.found);
    assert_eq!(value.definitive, Some(false));
    assert!(h.requests.is_empty());
    for id in ["", "evil&id", "💣", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"] {
        query(&mut h, "BTCUSDT", id);
        assert!(result(&mut h).is_err());
        assert!(h.requests.is_empty());
    }
    let mut h = Harness::new();
    h.requests.clear();
    query(&mut h, "BTCUSDT", "caller:/1");
    assert!(result(&mut h).is_err());
    assert!(h.requests.is_empty());
}
#[test]
fn network_failure_is_an_error_and_never_retries_or_mutates_state() {
    let mut h = ready(false);
    let before = h.engine.snapshot_frame(42.0).to_vec();
    query(&mut h, "BTCUSDT", "caller:/1");
    let r = h.requests.pop().unwrap();
    h.event(Event::HttpFailed {
        request: r.id,
        error: "network unavailable",
    });
    assert!(result(&mut h).is_err());
    assert!(h.requests.is_empty());
    assert_eq!(before, h.engine.snapshot_frame(42.0));
}

#[test]
fn cached_order_lookup_stays_read_only_and_does_not_require_private_io() {
    let mut h = Harness::new();
    h.requests.clear();
    h.engine
        .store_mut()
        .set_orders(vec![safe_cex_engine::model::Order {
            id: "cached".into(),
            parent_id: None,
            status: safe_cex_engine::model::OrderStatus::Closed,
            symbol: "BTCUSDT".into(),
            kind: safe_cex_engine::model::OrderType::Limit,
            side: safe_cex_engine::model::OrderSide::Buy,
            price: 100.0,
            amount: 2.0,
            filled: 2.0,
            remaining: 0.0,
            reduce_only: false,
            close: None,
            client_order_id: Some("cached1".into()),
            time_in_force: None,
            last_update: 123.0,
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: None,
        }]);
    let before = h.engine.snapshot_frame(42.0).to_vec();
    query(&mut h, "BTCUSDT", "cached1");
    let (_, result) = h.done.pop().expect("cached completion");
    #[derive(Deserialize)]
    struct Cached {
        found: bool,
        order: CachedOrder,
    }
    #[derive(Deserialize)]
    struct CachedOrder {
        id: String,
        status: String,
    }
    let result: Cached = serde_json::from_str(&result.unwrap()).unwrap();
    assert!(result.found);
    assert_eq!(result.order.id, "cached");
    assert_eq!(result.order.status, "closed");
    assert!(h.requests.is_empty());
    assert_eq!(before, h.engine.snapshot_frame(42.0));
}

#[test]
fn native_json_escaped_client_identity_is_valid_and_matches_the_caller() {
    let mut h = ready(false);
    query(&mut h, "BTCUSDT", "caller:/1");
    let r = h.requests.pop().unwrap();
    h.reply(r.id, 200, "", &ROW.replace("caller:/1", r"caller:\/1"));
    assert_eq!(
        result(&mut h).unwrap().order.unwrap().client_order_id,
        "caller:/1"
    );
}

#[test]
fn primary_lookup_signs_the_configured_recv_window_without_using_read_keys() {
    for pm in [false, true] {
        for (window, expected) in [(None, "20000"), (Some(4321.0), "4321"), (Some(0.0), "0")] {
            let mut h = ready_with_window(pm, window);
            h.engine
                .store_mut()
                .set_orders(vec![cached_order("secondary-order", "caller:/1")]);
            query(&mut h, "BTCUSDT", "caller:/1");
            let request = h.requests.pop().unwrap();
            assert_eq!(
                request.route, None,
                "prepared fixture transport carries no route override"
            );
            assert!(
                request
                    .headers
                    .iter()
                    .any(|(name, value)| *name == "X-MBX-APIKEY" && value == "lookup-key")
            );
            let (payload, signature) = request
                .url
                .split_once('?')
                .unwrap()
                .1
                .rsplit_once("&signature=")
                .unwrap();
            let expected_payload = format!(
                "symbol=BTCUSDT&origClientOrderId=caller%3A%2F1&timestamp=1000&recvWindow={expected}"
            );
            assert_eq!(payload, expected_payload);
            assert_eq!(
                signature,
                safe_cex_crypto::hmac_sha256(
                    b"lookup-secret",
                    expected_payload.as_bytes(),
                    safe_cex_crypto::Encoding::Hex
                )
            );
            h.reply(request.id, 200, "", ROW);
            assert!(result(&mut h).unwrap().found);
        }
    }
}
