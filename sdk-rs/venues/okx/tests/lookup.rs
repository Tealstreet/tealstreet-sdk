//! Query-order covers closed orders; only native 51603 is authoritative absence.
#![allow(dead_code)]
mod support;
use crate::support::FixtureOptions as Options;
use crate::support::{MARKET, TICKER, actions, envelope, prepared};
use safe_cex_engine::{Action, Command, Engine, Event, Method, Notice, Query, Response};
use safe_cex_venue_okx::Okx;
use serde::Deserialize;
const ROW: &str = r#"{"instId":"BTC-USDT-SWAP","ordId":"9007199254740993","clOrdId":"caller1","ordType":"post_only","state":"partially_filled","side":"sell","sz":"3","accFillSz":"1","px":"41000","avgPx":"40999","uTime":"1710000000123","reduceOnly":"true","posSide":"net"}"#;
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
    time_in_force: String,
    reduce_only: bool,
    last_update: f64,
    extra: Extra,
    info: Info,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Extra {
    remaining_native: f64,
    filled_in_placement_units: f64,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Info {
    cl_ord_id: String,
    pos_side: String,
}
fn ready(eea: bool, testnet: bool, private: bool) -> Engine<Okx> {
    let mut e = crate::support::engine(Options {
        key: private.then(|| "lookup-key".into()),
        read_credentials: Some(safe_cex_engine::ReadCredentials {
            key: "secondary-key".into(),
            secret: "secondary-secret".into(),
            passphrase: Some("secondary-pass".into()),
        }),
        secret: private.then(|| "lookup-secret".into()),
        passphrase: private.then(|| "lookup-pass".into()),
        eea,
        testnet,
        public_data_only: !private,
    });
    let mut now = 1_710_000_000_000.0;
    e.start(now);
    for _ in 0..30 {
        let out = prepared(&mut e, now);
        now = 1_710_000_000_001.0;
        let mut requests = 0;
        for a in out {
            if let Action::Http(r) = a {
                requests += 1;
                let body = if r.url.contains("instruments") {
                    envelope(if r.url.contains("instType=FUTURES") {
                        ""
                    } else {
                        MARKET
                    })
                } else if r.url.contains("market/tickers") {
                    envelope(if r.url.contains("instType=FUTURES") {
                        ""
                    } else {
                        TICKER
                    })
                } else if r.url.contains("account/config") {
                    envelope(r#"{"posMode":"net_mode","acctLv":"2"}"#)
                } else if r.url.contains("account-position-risk") {
                    envelope(r#"{"balData":[{"ccy":"USDT","eq":"5000"}]}"#)
                } else {
                    envelope("")
                };
                e.handle(
                    1_710_000_000_001.0,
                    Event::HttpResponse {
                        request: r.id,
                        response: Response {
                            status: 200,
                            headers: "",
                            body: &body,
                        },
                    },
                );
            }
        }
        if requests == 0 {
            return e;
        }
    }
    panic!("startup did not settle");
}
fn query(e: &mut Engine<Okx>, symbol: &str, id: &str) -> Vec<Action> {
    e.handle(
        1_710_000_000_100.0,
        Event::Command(Command::Query {
            command: 991,
            query: Query::OrderByClientId {
                symbol: symbol.into(),
                client_order_id: id.into(),
            },
        }),
    );
    actions(e)
}
fn complete(
    e: &mut Engine<Okx>,
    r: &safe_cex_engine::HttpRequest,
    status: u16,
    body: &str,
) -> Result<Lookup, String> {
    e.handle(
        1_710_000_000_101.0,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
    done(actions(e))
}
fn done(out: Vec<Action>) -> Result<Lookup, String> {
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::Http(_) | Action::WsSend { .. })),
        "query never writes/retries"
    );
    out.into_iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone {
                command: 991,
                result,
            }) => Some(Ok(serde_json::from_str(&result).unwrap())),
            Action::Notice(Notice::CommandFailed {
                command: 991,
                error,
            }) => Some(Err(error.legacy_text())),
            _ => None,
        })
        .expect("completed query")
}
fn request(out: Vec<Action>) -> safe_cex_engine::HttpRequest {
    assert_eq!(out.len(), 1);
    let Action::Http(r) = out.into_iter().next().unwrap() else {
        panic!("HTTP expected")
    };
    r
}
#[test]
fn lookup_signs_each_alias_and_preserves_read_only_order_provenance() {
    for (eea, demo, host) in [
        (false, false, "www.okx.com"),
        (false, true, "openapi.okx.com"),
        (true, false, "eea.okx.com"),
        (true, true, "eea.okx.com"),
    ] {
        let mut e = ready(eea, demo, true);
        let before = e.snapshot_frame(42.0).to_vec();
        let r = request(query(&mut e, "BTCUSDT", "caller1"));
        assert_eq!(r.method, Method::Get);
        assert_eq!(r.route, Some(safe_cex_engine::RequestRoute::Primary));
        assert!(r.body.is_none());
        assert_eq!(
            r.url,
            format!("https://{host}/api/v5/trade/order?instId=BTC-USDT-SWAP&clOrdId=caller1")
        );
        assert!(
            r.headers
                .iter()
                .any(|(k, v)| *k == "OK-ACCESS-KEY" && v == "lookup-key")
        );
        let ts = &r
            .headers
            .iter()
            .find(|(k, _)| *k == "OK-ACCESS-TIMESTAMP")
            .unwrap()
            .1;
        let signature = safe_cex_crypto::hmac_sha256(
            b"lookup-secret",
            format!("{ts}GET/api/v5/trade/order?instId=BTC-USDT-SWAP&clOrdId=caller1").as_bytes(),
            safe_cex_crypto::Encoding::Base64,
        );
        assert!(
            r.headers
                .iter()
                .any(|(k, v)| *k == "OK-ACCESS-SIGN" && v == &signature)
        );
        assert_eq!(
            r.headers
                .iter()
                .any(|(k, v)| *k == "x-simulated-trading" && v == "1"),
            demo
        );
        let value = complete(&mut e, &r, 200, &envelope(ROW)).unwrap();
        assert!(value.found);
        assert_eq!(value.definitive, None);
        let o = value.order.unwrap();
        assert_eq!(o.id, "9007199254740993");
        assert_eq!(o.symbol, "BTCUSDT");
        assert_eq!(o.client_order_id, "caller1");
        assert_eq!(o.status, "open");
        assert_eq!(o.amount, 3.0);
        assert_eq!(o.filled, 0.01);
        assert_eq!(o.remaining, 0.02);
        assert_eq!(o.extra.remaining_native, 2.0);
        assert_eq!(o.extra.filled_in_placement_units, 1.0);
        assert!(o.reduce_only);
        assert_eq!(o.time_in_force, "PostOnly");
        assert_eq!(o.last_update, 1_710_000_000_123.0);
        assert_eq!(o.info.cl_ord_id, "caller1");
        assert_eq!(o.info.pos_side, "net");
        assert_eq!(before, e.snapshot_frame(42.0));
    }
}
#[test]
fn lookup_handles_closed_and_canceled_orders_without_reopening_store() {
    for (native, expected) in [
        ("live", "open"),
        ("partially_filled", "open"),
        ("filled", "closed"),
        ("canceled", "canceled"),
        ("mmp_canceled", "canceled"),
    ] {
        let mut e = ready(false, false, true);
        let before = e.snapshot_frame(42.0).to_vec();
        let r = request(query(&mut e, "BTCUSDT", "caller1"));
        let value = complete(
            &mut e,
            &r,
            200,
            &envelope(&ROW.replace("partially_filled", native)),
        )
        .unwrap();
        assert_eq!(value.order.unwrap().status, expected);
        assert_eq!(before, e.snapshot_frame(42.0));
    }
}
#[test]
fn only_exact_native_51603_is_definitive_absence() {
    for (status, body, absent) in [
        (200, r#"{"code":"51603","msg":"missing","data":[]}"#, true),
        (400, r#"{"code":"51603","msg":"missing","data":[]}"#, false),
        (500, r#"{"code":"51603","msg":"missing","data":[]}"#, false),
        (401, r#"{"code":"51603","msg":"missing","data":[]}"#, false),
        (403, r#"{"code":"51603","msg":"missing","data":[]}"#, false),
        (408, r#"{"code":"51603","msg":"missing","data":[]}"#, false),
        (429, r#"{"code":"51603","msg":"missing","data":[]}"#, false),
        (
            401,
            r#"{"code":"50113","msg":"bad signature","data":[]}"#,
            false,
        ),
        (
            429,
            r#"{"code":"50011","msg":"rate limit","data":[]}"#,
            false,
        ),
        (404, "missing", false),
    ] {
        let mut e = ready(false, false, true);
        let r = request(query(&mut e, "BTCUSDT", "caller1"));
        let value = complete(&mut e, &r, status, body);
        if absent {
            let value = value.unwrap();
            assert!(!value.found);
            assert_eq!(value.definitive, Some(true));
            assert!(value.order.is_none());
        } else {
            assert!(value.is_err(), "{body}");
        }
    }
    let mut e = ready(false, false, true);
    let r = request(query(&mut e, "BTCUSDT", "caller1"));
    let value = complete(&mut e, &r, 200, &envelope("")).unwrap();
    assert!(!value.found);
    assert_eq!(value.definitive, Some(false));
}
#[test]
fn malformed_wrong_symbol_or_wrong_client_identity_never_becomes_a_miss() {
    for body in [
        "{}".into(),
        envelope(&ROW.replace("caller1", "wrong")),
        envelope(&ROW.replace("BTC-USDT-SWAP", "ETH-USDT-SWAP")),
        envelope(&ROW.replace("\"sz\":\"3\"", "\"sz\":\"NaN\"")),
        envelope(&ROW.replace("\"accFillSz\":\"1\"", "\"accFillSz\":\"4\"")),
        envelope(&ROW.replace("partially_filled", "unknown")),
        envelope(&format!("{ROW},{ROW}")),
    ] {
        let mut e = ready(false, false, true);
        let r = request(query(&mut e, "BTCUSDT", "caller1"));
        assert!(complete(&mut e, &r, 200, &body).is_err(), "{body}");
    }
}
#[test]
fn unknown_symbol_invalid_id_and_public_data_never_issue_lookup() {
    let mut e = ready(false, false, true);
    let value = done(query(&mut e, "UNLISTED", "caller1")).unwrap();
    assert!(!value.found);
    assert_eq!(value.definitive, Some(false));
    for id in ["", "bad&id", "💣", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"] {
        assert!(done(query(&mut e, "BTCUSDT", id)).is_err());
    }
    let mut e = ready(false, false, false);
    assert!(done(query(&mut e, "BTCUSDT", "caller1")).is_err());
}
#[test]
fn transport_failure_is_never_a_definitive_miss() {
    let mut e = ready(false, false, true);
    let before = e.snapshot_frame(42.0).to_vec();
    let r = request(query(&mut e, "BTCUSDT", "caller1"));
    e.handle(
        1_710_000_000_101.0,
        Event::HttpFailed {
            request: r.id,
            error: "OKX 51603: spoofed transport text",
        },
    );
    assert!(done(actions(&mut e)).is_err());
    assert_eq!(before, e.snapshot_frame(42.0));
}

#[test]
fn cached_order_lookup_stays_read_only_without_private_credentials() {
    // This local-only shortcut is safe when no secondary account can have
    // populated the cache.
    let markets = ready(false, false, false).store().markets().to_vec();
    let mut e = crate::support::engine(Options::default());
    e.store_mut().set_markets(markets);
    e.store_mut()
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
    let before = e.snapshot_frame(42.0).to_vec();
    let out = query(&mut e, "BTCUSDT", "cached1");
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
    let result = out
        .into_iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone { result, .. }) => Some(result),
            _ => None,
        })
        .expect("cached completion");
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
    let result: Cached = serde_json::from_str(&result).unwrap();
    assert!(result.found);
    assert_eq!(result.order.id, "cached");
    assert_eq!(result.order.status, "closed");
    assert_eq!(before, e.snapshot_frame(42.0));
}

#[test]
fn secondary_account_cache_never_answers_primary_lookup() {
    for outcome in ["found", "absent", "auth", "network"] {
        let mut e = ready(false, false, true);
        e.store_mut()
            .set_orders(vec![safe_cex_engine::model::Order {
                id: "secondary-order".into(),
                parent_id: None,
                status: safe_cex_engine::model::OrderStatus::Open,
                symbol: "BTCUSDT".into(),
                kind: safe_cex_engine::model::OrderType::Limit,
                side: safe_cex_engine::model::OrderSide::Buy,
                price: 100.0,
                amount: 9.0,
                filled: 0.0,
                remaining: 0.09,
                reduce_only: false,
                close: None,
                client_order_id: Some("caller1".into()),
                time_in_force: None,
                last_update: 123.0,
                trigger_type: None,
                take_profit: None,
                stop_loss: None,
                extra: None,
            }]);
        let before = e.snapshot_frame(42.0).to_vec();
        let r = request(query(&mut e, "BTCUSDT", "caller1"));
        assert_eq!(r.route, Some(safe_cex_engine::RequestRoute::Primary));
        assert!(
            r.headers
                .iter()
                .any(|(k, v)| *k == "OK-ACCESS-KEY" && v == "lookup-key")
        );
        let ts = &r
            .headers
            .iter()
            .find(|(k, _)| *k == "OK-ACCESS-TIMESTAMP")
            .unwrap()
            .1;
        let signature = safe_cex_crypto::hmac_sha256(
            b"lookup-secret",
            format!("{ts}GET/api/v5/trade/order?instId=BTC-USDT-SWAP&clOrdId=caller1").as_bytes(),
            safe_cex_crypto::Encoding::Base64,
        );
        assert!(
            r.headers
                .iter()
                .any(|(k, v)| *k == "OK-ACCESS-SIGN" && v == &signature)
        );
        let lookup = match outcome {
            "found" => complete(&mut e, &r, 200, &envelope(ROW)),
            "absent" => complete(
                &mut e,
                &r,
                200,
                r#"{"code":"51603","msg":"missing","data":[]}"#,
            ),
            "auth" => complete(
                &mut e,
                &r,
                401,
                r#"{"code":"50113","msg":"bad signature","data":[]}"#,
            ),
            _ => {
                e.handle(
                    1_710_000_000_101.0,
                    Event::HttpFailed {
                        request: r.id,
                        error: "network unavailable",
                    },
                );
                done(actions(&mut e))
            }
        };
        match outcome {
            "found" => {
                let order = lookup.unwrap().order.unwrap();
                assert_eq!(order.id, "9007199254740993");
                assert_eq!(order.amount, 3.0);
            }
            "absent" => {
                let lookup = lookup.unwrap();
                assert!(!lookup.found);
                assert_eq!(lookup.definitive, Some(true));
                assert!(lookup.order.is_none());
            }
            _ => assert!(lookup.is_err()),
        }
        assert_eq!(before, e.snapshot_frame(42.0));
    }
}
