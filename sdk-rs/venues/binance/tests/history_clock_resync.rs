//! A timestamp refusal may replay a read page once, within its original root.
mod support;
use safe_cex_engine::{Command, ErrorKind, Event, HttpRequest, Method, Query};
use safe_cex_venue_binance::Variant;
use support::{Harness, Options};

const SKEW: &str =
    r#"{"code":-1021,"msg":"Timestamp for this request is outside of the recvWindow."}"#;

fn harness(variant: Variant, testnet: bool, portfolio_margin: bool) -> Harness {
    let mut options = Options {
        key: Some("primary-key".into()),
        secret: Some("AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=".into()),
        read_credentials: Some(safe_cex_engine::ReadCredentials {
            key: "read-key".into(),
            secret: "AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI=".into(),
            passphrase: None,
        }),
        ..Options::default()
    };
    options.variant = variant;
    options.testnet = testnet;
    options.portfolio_margin = portfolio_margin;
    let mut h = Harness::started(options);
    h.requests.clear();
    h
}
fn fills(h: &mut Harness, command: u32, wait: u32) -> HttpRequest {
    h.engine
        .handle_command(
            h.now,
            Command::Query {
                command,
                query: Query::Fills {
                    symbol: Some("BTCUSDT".into()),
                    order_id: None,
                    from_ms: Some(0.0),
                    to_ms: Some(10.0),
                    limit: Some(100),
                },
            },
            Some(wait),
        )
        .unwrap();
    h.collect();
    h.take(if h.paths().iter().any(|p| p.contains("myTrades")) {
        "/myTrades"
    } else {
        "/userTrades"
    })
}
fn fire_until(h: &mut Harness, until: f64) {
    loop {
        let Some((timer, at)) = h
            .timers
            .iter()
            .copied()
            .filter(|(_, at)| *at <= until)
            .min_by(|a, b| a.1.total_cmp(&b.1))
        else {
            h.now = until;
            return;
        };
        h.timers.retain(|(t, _)| *t != timer);
        h.now = h.now.max(at);
        h.event(Event::TimerFired { timer });
    }
}
fn assert_read_signature(request: &HttpRequest, stamp: u64) {
    assert_eq!(request.method, Method::Get);
    assert!(
        request
            .headers
            .iter()
            .any(|(k, v)| *k == "X-MBX-APIKEY" && v == "read-key")
    );
    let (base, signature) = request.url.rsplit_once("&signature=").unwrap();
    let query = base.split_once('?').unwrap().1;
    assert!(query.split('&').any(|v| v == format!("timestamp={stamp}")));
    assert_eq!(
        signature,
        safe_cex_crypto::hmac_sha256(
            b"AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI=",
            query.as_bytes(),
            safe_cex_crypto::Encoding::Hex
        )
    );
}

#[test]
fn a_skewed_read_waits_for_public_time_then_replays_once_with_the_same_key() {
    let mut h = harness(Variant::UsdM, false, false);
    let first = fills(&mut h, 901, 120_000);
    assert_read_signature(&first, 1000);
    h.now = 1100.;
    h.reply(first.id, 400, "", SKEW);
    assert!(h.done.is_empty());
    let clock = h.take("/fapi/v1/time");
    assert_eq!(clock.url, "https://fapi.binance.com/fapi/v1/time");
    assert!(clock.headers.is_empty());
    assert_eq!(clock.body, None);
    assert!(!h.has_request("userTrades"));
    h.now = 1200.;
    h.reply(clock.id, 200, "", r#"{"serverTime":50000}"#);
    let second = h.take("/userTrades");
    assert_ne!(first.id, second.id);
    assert_read_signature(&second, 50000);
    h.reply(second.id, 400, "", SKEW);
    assert_eq!(h.done.len(), 1);
    assert!(!h.has_request("/time"));
    assert!(!h.has_request("/userTrades"));
    assert_eq!(h.failures[0].1.venue_code.as_deref(), Some("-1021"));
}

#[test]
fn a_replayed_page_can_complete_and_later_reads_use_the_calibrated_clock() {
    let mut h = harness(Variant::UsdM, false, false);
    let first = fills(&mut h, 901, 120_000);
    h.reply(first.id, 400, "", SKEW);
    let clock = h.take("/time");
    h.reply(clock.id, 200, "", r#"{"serverTime":50000}"#);
    let second = h.take("/userTrades");
    h.reply(second.id, 200, "", "[]");
    assert_eq!(h.done, vec![(901, Ok("[]".into()))]);
    h.now += 10.;
    let next = fills(&mut h, 902, 120_000);
    assert_read_signature(&next, 50010);
    let mut peer = harness(Variant::UsdM, false, false);
    let untouched = fills(&mut peer, 903, 120_000);
    assert_read_signature(&untouched, 1000);
}

#[test]
fn time_recovery_never_renews_the_original_caller_deadline() {
    let mut h = harness(Variant::UsdM, false, false);
    let first = fills(&mut h, 901, 200);
    h.now = 1100.;
    h.reply(first.id, 400, "", SKEW);
    let clock = h.take("/time");
    fire_until(&mut h, 1200.);
    assert_eq!(h.failures.len(), 1);
    assert_eq!(h.failures[0].1.kind, Some(ErrorKind::Timeout));
    h.reply(clock.id, 200, "", r#"{"serverTime":50000}"#);
    assert!(!h.has_request("/userTrades"));
    assert_eq!(h.done.len(), 1);
}

#[test]
fn malformed_time_fails_the_read_without_a_resend_or_clock_change() {
    for body in [
        "{}",
        r#"{"serverTime":0}"#,
        r#"{"serverTime":-1}"#,
        r#"{"serverTime":1e300}"#,
    ] {
        let mut h = harness(Variant::UsdM, false, false);
        let first = fills(&mut h, 901, 120_000);
        h.reply(first.id, 400, "", SKEW);
        let clock = h.take("/time");
        h.reply(clock.id, 200, "", body);
        assert_eq!(h.failures.len(), 1, "{body}");
        assert!(!h.has_request("/userTrades"));
        let next = fills(&mut h, 902, 120_000);
        assert_read_signature(&next, 1000);
    }
}

#[test]
fn clock_url_matches_the_market_and_testnet_including_portfolio_margin() {
    for (variant, testnet, pm, expected) in [
        (
            Variant::UsdM,
            true,
            false,
            "https://demo-fapi.binance.com/fapi/v1/time",
        ),
        (
            Variant::CoinM,
            false,
            false,
            "https://dapi.binance.com/dapi/v1/time",
        ),
        (
            Variant::CoinM,
            true,
            false,
            "https://demo-dapi.binance.com/dapi/v1/time",
        ),
        (
            Variant::UsdM,
            false,
            true,
            "https://fapi.binance.com/fapi/v1/time",
        ),
    ] {
        let mut h = harness(variant, testnet, pm);
        let first = fills(&mut h, 901, 120_000);
        h.reply(first.id, 400, "", SKEW);
        let clock = h.take("/time");
        assert_eq!(clock.url, expected);
    }
}

#[test]
fn other_error_codes_do_not_start_clock_recovery() {
    let mut h = harness(Variant::UsdM, false, false);
    let first = fills(&mut h, 901, 120_000);
    h.reply(
        first.id,
        400,
        "",
        r#"{"code":-1022,"msg":"Signature is invalid."}"#,
    );
    assert_eq!(h.failures.len(), 1);
    assert!(!h.has_request("/time"));
}

#[test]
fn a_failed_or_timed_out_clock_probe_ends_once_without_replaying_the_page() {
    for failure in ["network", "http", "timeout"] {
        let mut h = harness(Variant::UsdM, false, false);
        let first = fills(&mut h, 901, 120_000);
        h.reply(first.id, 400, "", SKEW);
        let clock = h.take("/time");
        match failure {
            "network" => h.event(Event::HttpFailed {
                request: clock.id,
                error: "reset",
            }),
            "http" => h.reply(clock.id, 503, "", "unavailable"),
            _ => fire_until(&mut h, 6000.),
        }
        assert_eq!(h.done.len(), 1, "{failure}");
        assert!(!h.has_request("/userTrades"));
        if failure == "timeout" {
            assert_eq!(h.failures[0].1.kind, Some(ErrorKind::Timeout));
        }
        h.reply(clock.id, 200, "", r#"{"serverTime":50000}"#);
        assert_eq!(h.done.len(), 1);
        assert!(!h.has_request("/userTrades"));
    }
}

#[test]
fn calibration_can_correct_a_clock_that_is_ahead() {
    let mut h = harness(Variant::UsdM, false, false);
    h.now = 100_000.;
    let first = fills(&mut h, 901, 120_000);
    h.reply(first.id, 400, "", SKEW);
    let clock = h.take("/time");
    h.reply(clock.id, 200, "", r#"{"serverTime":50000}"#);
    assert_read_signature(&h.take("/userTrades"), 50000);
}

#[test]
fn order_and_income_reads_share_the_recovery_without_changing_the_query() {
    for query in [
        Query::OrderHistory {
            symbol: Some("BTCUSDT".into()),
            from_ms: Some(2.),
            to_ms: Some(10.),
            limit: Some(100),
        },
        Query::Income {
            symbol: Some("BTCUSDT".into()),
            kind: Some("FUNDING_FEE".into()),
            from_ms: Some(2.),
            to_ms: Some(10.),
            limit: Some(100),
        },
    ] {
        let mut h = harness(Variant::UsdM, false, false);
        h.event(Event::Command(Command::Query {
            command: 901,
            query,
        }));
        let first = h.requests.pop().unwrap();
        assert_eq!(first.method, Method::Get);
        h.reply(first.id, 400, "", SKEW);
        let clock = h.take("/time");
        h.reply(clock.id, 200, "", r#"{"serverTime":50000}"#);
        let second = h.requests.pop().unwrap();
        let normalized = |r: &HttpRequest| {
            r.url
                .split('&')
                .filter(|p| !p.starts_with("timestamp=") && !p.starts_with("signature="))
                .collect::<Vec<_>>()
                .join("&")
        };
        assert_eq!(normalized(&first), normalized(&second));
        assert_read_signature(&second, 50000);
        h.reply(second.id, 200, "", "[]");
        assert_eq!(h.done, vec![(901, Ok("[]".into()))]);
    }
}

#[test]
fn a_rejected_order_placement_never_enters_read_clock_recovery() {
    use safe_cex_engine::model::{OrderSide, OrderType, PlaceOrderOpts};
    let mut h = Harness::started(Options {
        key: Some("primary-key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    h.load_public();
    h.requests.clear();
    h.event(Event::Command(Command::PlaceOrders {
        command: 901,
        orders: vec![PlaceOrderOpts {
            symbol: "BTCUSDT".into(),
            kind: OrderType::Limit,
            side: OrderSide::Buy,
            amount: 0.001,
            price: Some(1000.),
            trigger_price: None,
            stop_loss: None,
            take_profit: None,
            reduce_only: None,
            time_in_force: None,
            trigger_type: None,
            tp_trigger_type: None,
            sl_trigger_type: None,
            client_order_id: None,
            max_slippage_pct: None,
            bybit_bbo: None,
            bybit_slippage_tolerance_ticks: None,
            implied_volatility: None,
            close: None,
            stp_mode: None,
        }],
    }));
    let order = h.take("/fapi/v1/order");
    assert_eq!(order.method, Method::Post);
    h.reply(order.id, 400, "", SKEW);
    assert_eq!(h.failures.iter().filter(|(c, _)| *c == 901).count(), 1);
    assert!(!h.has_request("/time"));
    assert!(h.requests.iter().all(|r| r.method != Method::Post));
}

#[test]
fn a_read_only_order_lookup_replays_on_the_original_primary_identity() {
    let mut h = Harness::started(Options {
        key: Some("primary-key".into()),
        secret: Some("secret".into()),
        read_credentials: Some(safe_cex_engine::ReadCredentials {
            key: "read-key".into(),
            secret: "read-secret".into(),
            passphrase: None,
        }),
        ..Options::default()
    });
    h.load_public();
    h.requests.clear();
    h.event(Event::Command(Command::Query {
        command: 901,
        query: Query::OrderByClientId {
            symbol: "BTCUSDT".into(),
            client_order_id: "caller".into(),
        },
    }));
    let first = h.take("/fapi/v1/order");
    assert_eq!(first.method, Method::Get);
    h.reply(first.id, 400, "", SKEW);
    let clock = h.take("/time");
    h.reply(clock.id, 200, "", r#"{"serverTime":50000}"#);
    let second = h.take("/fapi/v1/order");
    assert_eq!(second.method, Method::Get);
    assert!(
        second
            .headers
            .iter()
            .any(|(k, v)| *k == "X-MBX-APIKEY" && v == "primary-key")
    );
    let (base, signature) = second.url.rsplit_once("&signature=").unwrap();
    assert_eq!(
        signature,
        safe_cex_crypto::hmac_sha256(
            b"secret",
            base.split_once('?').unwrap().1.as_bytes(),
            safe_cex_crypto::Encoding::Hex
        )
    );
    h.reply(
        second.id,
        400,
        "",
        r#"{"code":-2013,"msg":"Order does not exist."}"#,
    );
    let result: serde_json::Value = serde_json::from_str(h.done[0].1.as_ref().unwrap()).unwrap();
    assert_eq!(result["found"], false);
    assert_eq!(result["definitive"], true);
    assert_eq!(h.done.len(), 1);
}

#[test]
fn a_refused_time_preparation_keeps_its_typed_cause_and_does_not_resend() {
    use safe_cex_engine::{
        Action,
        request_context::{Error, HttpAction},
    };
    let mut h = harness(Variant::UsdM, false, false);
    let first = fills(&mut h, 901, 120_000);
    h.engine.handle(
        h.now,
        Event::HttpResponse {
            request: first.id,
            response: safe_cex_engine::Response {
                status: 400,
                headers: "",
                body: SKEW,
            },
        },
    );
    let actions: Vec<_> = h.engine.drain_actions().collect();
    let preparation = actions
        .into_iter()
        .find_map(|a| match a {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } if request.url.ends_with("/time") => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .expect("clock preparation");
    h.engine.http_preparation_failed(
        h.now,
        preparation,
        Error::Invalid,
        Error::Invalid.read_venue_error(),
    );
    h.collect();
    assert_eq!(
        h.failures,
        vec![(
            901,
            Error::Invalid
                .read_venue_error()
                .outcome(safe_cex_engine::Outcome::NotSent)
        )]
    );
    assert!(!h.has_request("/time"));
    assert!(!h.has_request("/userTrades"));
}
