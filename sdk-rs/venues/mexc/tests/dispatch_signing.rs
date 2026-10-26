//! Original eager-signing REDs. Inputs stay native until the trusted setup cutover.
mod support;

use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::signing::{SignFuture, SignRequest, Signer};
use safe_cex_engine::{
    Action, Command, ErrorKind, Event, HttpRequest, Notice, Outcome, Query, ReadCredentials,
    Response, SocketId,
};
use safe_cex_venue_mexc::{MexcFutures, Options};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use support::{MARKETS, TICKERS, TestEngine};

struct Parked;
impl Signer for Parked {
    fn sign(&self, _: SignRequest) -> SignFuture {
        Box::pin(std::future::pending())
    }
}

fn options() -> Options {
    Options {
        key: Some("replay-key".into()),
        secret: Some("replay-secret".into()),
        ..Default::default()
    }
}

fn engine(options: Options) -> TestEngine {
    let mut engine = safe_cex_engine::test_support::seeded_engine(MexcFutures::new(options));
    engine.set_signer(Some(Arc::new(Parked))).unwrap();
    engine
}

fn reader(secondary: bool) -> TestEngine {
    engine(Options {
        read_credentials: secondary.then(|| ReadCredentials {
            key: "read-key".into(),
            secret: "read-secret".into(),
            passphrase: None,
        }),
        ..options()
    })
}

fn identity(engine: &mut TestEngine) -> Vec<Action> {
    engine.handle(
        1_700_000_000_000.875,
        Event::Command(Command::FetchAccount { command: 91 }),
    );
    engine.drain_actions().collect()
}

fn assert_parked(actions: &[Action], transport: impl Fn(&Action) -> bool) {
    assert!(
        !actions.iter().any(transport),
        "credential-bearing transport escaped while the installed provider was parked"
    );
    assert!(
        actions.iter().any(|a| matches!(a, Action::Sign { .. })),
        "the actual venue must request provider work"
    );
}

fn first_http(actions: &[Action], suffix: &str) -> HttpRequest {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with(suffix) => Some(r.clone()),
            _ => None,
        })
        .expect("native request working control")
}

fn respond(engine: &mut TestEngine, request: &HttpRequest, now: f64, body: &str) {
    engine.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}

fn private_started() -> (TestEngine, SocketId, Vec<HttpRequest>) {
    private_started_with(options())
}

fn private_started_with(options: Options) -> (TestEngine, SocketId, Vec<HttpRequest>) {
    private_started_configured(options, |_| {})
}

fn private_started_configured(
    options: Options,
    configure: impl FnOnce(&mut TestEngine),
) -> (TestEngine, SocketId, Vec<HttpRequest>) {
    let mut engine = engine(options);
    configure(&mut engine);
    engine.start(0.0);
    let actions = support::prepared(&mut engine, 0.);
    let markets = first_http(&actions, "/api/v1/contract/detail");
    respond(&mut engine, &markets, 100.0, MARKETS);
    let mut actions = support::prepared(&mut engine, 100.);
    let ticker = first_http(&actions, "/api/v1/contract/ticker");
    respond(&mut engine, &ticker, 100.0, TICKERS);
    actions.extend(engine.drain_actions());
    let socket = actions
        .iter()
        .find_map(|a| match a {
            Action::WsConnect {
                socket,
                channel: Some(safe_cex_engine::SocketChannel::Private),
                ..
            } => Some(*socket),
            _ => None,
        })
        .expect("real private startup connect");
    let mode = first_http(&actions, "/api/v1/private/position/position_mode");
    respond(
        &mut engine,
        &mode,
        100.0,
        r#"{"success":true,"code":0,"data":2}"#,
    );
    let reads = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect();
    engine.handle(100.0, Event::WsOpened { socket });
    (engine, socket, reads)
}

fn ready() -> TestEngine {
    let (mut engine, _, reads) = private_started();
    for request in reads {
        support::response(
            &mut engine,
            &request,
            r#"{"success":true,"code":0,"data":[]}"#,
        );
    }
    engine.drain_actions().for_each(drop);
    engine
}

fn place(engine: &mut TestEngine, command: u32, now: f64) {
    let order: PlaceOrderOpts = serde_json::from_str(
        r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":60,"price":59000}"#,
    )
    .unwrap();
    engine.handle(
        now,
        Event::Command(Command::PlaceOrders {
            command,
            orders: vec![order],
        }),
    );
}

#[test]
fn primary_identity_waits_for_provider_instead_of_eager_http() {
    assert_parked(&identity(&mut reader(false)), |a| {
        matches!(a, Action::Http(_))
    });
}

#[test]
fn secondary_identity_waits_for_provider_instead_of_eager_http() {
    assert_parked(&identity(&mut reader(true)), |a| {
        matches!(a, Action::Http(_))
    });
}

#[test]
fn startup_private_mode_waits_for_provider() {
    let mut engine = engine(options());
    engine.start(0.0);
    let actions = support::prepared(&mut engine, 0.);
    let markets = first_http(&actions, "/api/v1/contract/detail");
    respond(&mut engine, &markets, 100.0, MARKETS);
    let actions: Vec<_> = engine.drain_actions().collect();
    assert_parked(
        &actions,
        |a| matches!(a, Action::Http(r) if r.url.contains("/private/")),
    );
}

#[test]
fn opening_private_login_uses_provider_work() {
    let (mut engine, socket, _) = private_started();
    let actions: Vec<_> = engine.drain_actions().collect();
    assert_parked(
        &actions,
        |a| matches!(a, Action::WsSend { socket: s, text } if *s == socket && text.contains("\"login\"")),
    );
}

#[test]
fn mutation_uses_provider_instead_of_an_eager_signed_write() {
    let mut engine = ready();
    place(&mut engine, 92, 200.0);
    let actions: Vec<_> = engine.drain_actions().collect();
    assert_parked(
        &actions,
        |a| matches!(a, Action::Http(r) if r.url.ends_with("/order/submit")),
    );
}

#[test]
fn delayed_first_handoff_does_not_spend_the_450ms_release_gap_while_queued() {
    let mut engine = ready();
    place(&mut engine, 93, 200.0);
    place(&mut engine, 94, 200.0);
    // The host cannot release the queued write until 800; the ACK is fast.
    let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, 800.0);
    let first = first_http(&actions, "/api/v1/private/order/submit");
    respond(
        &mut engine,
        &first,
        810.0,
        r#"{"success":true,"code":0,"data":"first"}"#,
    );
    let after_ack = safe_cex_engine::test_support::prepared_actions(&mut engine, 810.0);
    assert!(
        !after_ack
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/order/submit"))),
        "queue-time next_submit allowed a second handoff only 10ms after the first"
    );
    let (timer, at) = after_ack
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms >= 1250.0 => Some((*timer, *at_ms)),
            _ => None,
        })
        .expect("next submit must wait for the actual release gap");
    engine.handle(at, Event::TimerFired { timer });
    let second = safe_cex_engine::test_support::prepared_actions(&mut engine, at);
    assert!(
        second
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/order/submit")))
    );
}

#[test]
fn native_primary_wire_is_integer_stamped_and_header_ordered() {
    let actions = identity(&mut reader(false));
    let request = first_http(&actions, "/api/v1/private/account/assets");
    let names: Vec<_> = request.headers.iter().map(|(name, _)| *name).collect();
    assert_eq!(
        names,
        [
            "ApiKey",
            "Request-Time",
            "Signature",
            "Recv-Window",
            "Content-Type"
        ]
    );
    assert_eq!(request.headers[0].1, "replay-key");
    assert_eq!(request.headers[1].1, "1700000000000");
    assert_eq!(
        request.headers[2].1,
        safe_cex_crypto::hmac_sha256(
            b"replay-secret",
            b"replay-key1700000000000",
            safe_cex_crypto::Encoding::Hex,
        )
    );
    assert_eq!(request.headers[3].1, "20000");
    assert_eq!(request.headers[4].1, "application/json");
    assert!(request.body.is_none());
}

#[test]
fn native_secondary_get_preserves_bytes_and_tighter_receive_window() {
    let mut engine = engine(Options {
        recv_window: Some(500),
        read_credentials: Some(ReadCredentials {
            key: " read-key ".into(),
            secret: " read-secret ".into(),
            passphrase: None,
        }),
        ..options()
    });
    let request = first_http(&identity(&mut engine), "/api/v1/private/account/assets");
    assert_eq!(request.headers[0].1, " read-key ");
    assert_eq!(
        request.headers[2].1,
        safe_cex_crypto::hmac_sha256(
            b" read-secret ",
            b" read-key 1700000000000",
            safe_cex_crypto::Encoding::Hex,
        )
    );
    assert_eq!(request.headers[3].1, "500");
}

#[test]
fn native_private_open_pins_ping_before_exact_login() {
    let (mut engine, socket, _) = private_started();
    let frames: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::WsSend { socket: s, text } if s == socket => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0], r#"{"method":"ping"}"#);
    let signature = safe_cex_crypto::hmac_sha256(
        b"replay-secret",
        b"replay-key100",
        safe_cex_crypto::Encoding::Hex,
    );
    assert_eq!(
        frames[1],
        format!(
            r#"{{"method":"login","param":{{"apiKey":"replay-key","reqTime":"100","signature":"{signature}"}}}}"#
        )
    );
}

#[test]
fn native_immediate_submits_keep_the_450ms_gap() {
    let mut engine = ready();
    place(&mut engine, 95, 200.0);
    place(&mut engine, 96, 200.0);
    let actions: Vec<_> = engine.drain_actions().collect();
    let first = first_http(&actions, "/api/v1/private/order/submit");
    respond(
        &mut engine,
        &first,
        250.0,
        r#"{"success":true,"code":0,"data":"first"}"#,
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/order/submit")))
    );
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 650.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    engine.handle(650.0, Event::TimerFired { timer });
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/order/submit")))
    );
}

#[test]
fn public_discovery_needs_no_provider() {
    let mut engine = engine(Options {
        public_data_only: true,
        ..Default::default()
    });
    engine.start(0.0);
    let actions = support::prepared(&mut engine, 0.);
    assert!(!actions.iter().any(|a| matches!(a, Action::Sign { .. })));
    let markets = first_http(&actions, "/api/v1/contract/detail");
    assert!(markets.headers.is_empty());
    respond(&mut engine, &markets, 100.0, MARKETS);
    let actions = support::prepared(&mut engine, 100.);
    let ticker = first_http(&actions, "/api/v1/contract/ticker");
    respond(&mut engine, &ticker, 100.0, TICKERS);
    assert!(engine.store().market("BTCUSDT").is_some());
}

fn header<'a>(request: &'a HttpRequest, name: &str) -> &'a str {
    &request
        .headers
        .iter()
        .find(|(key, _)| *key == name)
        .unwrap()
        .1
}

fn secondary_options() -> Options {
    Options {
        read_credentials: Some(ReadCredentials {
            key: "read-key".into(),
            secret: "read-secret".into(),
            passphrase: None,
        }),
        ..options()
    }
}

#[test]
fn native_private_history_get_is_primary_even_with_secondary_configured() {
    let mut engine = engine(secondary_options());
    engine.handle(
        1000.0,
        Event::Command(Command::Query {
            command: 101,
            query: Query::Fills {
                symbol: None,
                order_id: Some("123".into()),
                from_ms: None,
                to_ms: None,
                limit: Some(10),
            },
        }),
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    let request = first_http(&actions, "/api/v1/private/order/deal_details/123?");
    assert_eq!(header(&request, "ApiKey"), "replay-key");
    assert_eq!(header(&request, "Request-Time"), "1000");
    assert_eq!(
        header(&request, "Signature"),
        safe_cex_crypto::hmac_sha256(
            b"replay-secret",
            b"replay-key1000",
            safe_cex_crypto::Encoding::Hex,
        )
    );
}

#[test]
fn native_public_trade_history_remains_unsigned_with_secondary_configured() {
    let (mut engine, _, reads) = private_started_with(secondary_options());
    for request in reads {
        support::response(
            &mut engine,
            &request,
            r#"{"success":true,"code":0,"data":[]}"#,
        );
    }
    engine.drain_actions().for_each(drop);
    engine.handle(
        200.0,
        Event::Command(Command::Query {
            command: 102,
            query: Query::PublicTrades {
                symbol: "BTCUSDT".into(),
                limit: Some(7),
            },
        }),
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    let request = first_http(&actions, "/api/v1/contract/deals/BTC_USDT?limit=7");
    assert!(request.headers.is_empty());
    assert!(request.body.is_none());
    assert!(!actions.iter().any(|a| matches!(a, Action::Sign { .. })));
}

#[test]
fn native_amend_poll_re_signs_secondary_on_each_of_five_retries() {
    let (mut engine, socket, reads) = private_started_with(secondary_options());
    for request in reads {
        support::response(
            &mut engine,
            &request,
            r#"{"success":true,"code":0,"data":[]}"#,
        );
    }
    engine.drain_actions().for_each(drop);
    support::ws(
        &mut engine,
        socket,
        r#"{"channel":"push.personal.order","data":{"orderId":"123","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":10,"price":59000,"state":2}}"#,
    );
    engine.drain_actions().for_each(drop);
    engine.handle(
        300.0,
        Event::Command(Command::AmendOrder {
            command: 103,
            id: "123".into(),
            price: Some(58000.0),
            amount: None,
        }),
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    let cancel = first_http(&actions, "/api/v1/private/order/cancel");
    assert_eq!(header(&cancel, "ApiKey"), "replay-key");
    respond(
        &mut engine,
        &cancel,
        400.0,
        r#"{"success":true,"code":0,"data":[{"orderId":"123","errorCode":0}]}"#,
    );
    let mut actions: Vec<_> = engine.drain_actions().collect();
    for attempt in 0..=5 {
        let now = 400.0 + 200.0 * f64::from(attempt);
        let read = first_http(&actions, "/api/v1/private/order/get/123");
        assert_eq!(header(&read, "ApiKey"), "read-key");
        assert_eq!(header(&read, "Request-Time"), format!("{now:.0}"));
        assert_eq!(
            header(&read, "Signature"),
            safe_cex_crypto::hmac_sha256(
                b"read-secret",
                format!("read-key{now:.0}").as_bytes(),
                safe_cex_crypto::Encoding::Hex,
            )
        );
        respond(
            &mut engine,
            &read,
            now,
            r#"{"success":true,"code":0,"data":{"orderId":"123","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":10,"price":59000,"state":2}}"#,
        );
        actions = engine.drain_actions().collect();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        if attempt < 5 {
            let timer = actions
                .iter()
                .find_map(|a| match a {
                    Action::SetTimer { timer, at_ms } if *at_ms == now + 200.0 => Some(*timer),
                    _ => None,
                })
                .expect("native amend retry is 200 ms after its response");
            engine.handle(now + 200.0, Event::TimerFired { timer });
            actions = engine.drain_actions().collect();
        } else {
            assert_eq!(
                actions
                    .iter()
                    .filter(|a| matches!(
                        a,
                        Action::Notice(Notice::CommandFailed { command: 103, .. })
                    ))
                    .count(),
                1
            );
            assert!(
                !actions
                    .iter()
                    .any(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == now + 200.0))
            );
        }
    }
}

#[test]
fn native_batch_gap_is_inherited_by_a_following_singleton() {
    let mut engine = ready();
    let order: PlaceOrderOpts = serde_json::from_str(
        r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":60,"price":59000}"#,
    )
    .unwrap();
    engine.handle(
        200.0,
        Event::Command(Command::PlaceOrders {
            command: 104,
            orders: vec![order.clone(), order],
        }),
    );
    place(&mut engine, 105, 200.0);
    let actions: Vec<_> = engine.drain_actions().collect();
    let batch = first_http(&actions, "/api/v1/private/order/submit_batch");
    respond(
        &mut engine,
        &batch,
        250.0,
        r#"{"success":true,"code":0,"data":[{"orderId":"first","errorCode":0},{"orderId":"second","errorCode":0}]}"#,
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 1700.0 => Some(*timer),
            _ => None,
        })
        .expect("following singleton inherits the preceding job's 1500ms interval");
    engine.handle(1700.0, Event::TimerFired { timer });
    let actions: Vec<_> = engine.drain_actions().collect();
    let request = first_http(&actions, "/api/v1/private/order/submit");
    assert_eq!(header(&request, "Request-Time"), "1700");
}

#[test]
fn native_cancel_and_leverage_are_not_delayed_by_the_submit_gap() {
    for leverage in [false, true] {
        let mut engine = ready();
        place(&mut engine, 106, 200.0);
        let actions: Vec<_> = engine.drain_actions().collect();
        let first = first_http(&actions, "/api/v1/private/order/submit");
        respond(
            &mut engine,
            &first,
            250.0,
            r#"{"success":true,"code":0,"data":"placed"}"#,
        );
        engine.drain_actions().for_each(drop);
        let command = if leverage {
            Command::SetLeverage {
                command: 107,
                symbol: "BTCUSDT".into(),
                leverage: 10.0,
                side: None,
            }
        } else {
            Command::CancelOrders {
                command: 107,
                ids: vec!["placed".into()],
            }
        };
        engine.handle(300.0, Event::Command(command));
        let actions: Vec<_> = engine.drain_actions().collect();
        let path = if leverage {
            "/api/v1/private/position/change_leverage"
        } else {
            "/api/v1/private/order/cancel"
        };
        let request = first_http(&actions, path);
        assert_eq!(header(&request, "Request-Time"), "300");
        assert_eq!(header(&request, "ApiKey"), "replay-key");
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == 650.0))
        );
    }
}

#[test]
fn genuine_missing_key_mode_commands_terminalize_once_without_waiting() {
    let mut engine = engine(Options::default());
    for command in [108, 109] {
        engine.handle(
            300.0,
            Event::Command(Command::SetPositionMode {
                command,
                hedged: false,
                symbol: None,
            }),
        );
        let actions: Vec<_> = engine.drain_actions().collect();
        let errors: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandFailed {
                    command: actual,
                    error,
                }) if *actual == command => Some(error),
                _ => None,
            })
            .collect();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].kind, Some(ErrorKind::AuthInvalid));
        assert_eq!(errors[0].outcome, Some(Outcome::NotSent));
        assert!(!actions.iter().any(|a| matches!(
            a,
            Action::Http(_)
                | Action::ManagedHttp(_)
                | Action::Sign { .. }
                | Action::SetTimer { .. }
        )));
    }
}

#[test]
fn local_notsent_refresh_failure_does_not_spend_the_next_submit_slot() {
    let calls = Arc::new(AtomicUsize::new(0));
    let fail_on = Arc::new(AtomicUsize::new(usize::MAX));
    let observed_calls = calls.clone();
    let configured_failure = fail_on.clone();
    let (mut engine, _, reads) = private_started_configured(options(), |engine| {
        engine
            .set_entropy(Box::new(move |bytes| {
                let call = observed_calls.fetch_add(1, Ordering::SeqCst) + 1;
                if call == configured_failure.load(Ordering::SeqCst) {
                    return Err("test identity entropy unavailable".into());
                }
                for (index, byte) in bytes.iter_mut().enumerate() {
                    *byte = call.wrapping_add(index) as u8;
                }
                Ok(())
            }))
            .unwrap();
    });
    for request in reads {
        support::response(
            &mut engine,
            &request,
            r#"{"success":true,"code":0,"data":[]}"#,
        );
    }
    engine.drain_actions().for_each(drop);
    // Native admission succeeds; only final placement refresh loses entropy.
    let target = calls.load(Ordering::SeqCst) + 2;
    fail_on.store(target, Ordering::SeqCst);
    place(&mut engine, 110, 200.0);
    let failed: Vec<_> = engine.drain_actions().collect();
    assert_eq!(calls.load(Ordering::SeqCst), target);
    let terminals: Vec<_> = failed
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed {
                command: 110,
                error,
            }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(terminals.len(), 1);
    assert_eq!(terminals[0].outcome, Some(Outcome::NotSent));
    assert!(
        terminals[0]
            .message
            .contains("test identity entropy unavailable")
    );
    assert!(!failed.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::ManagedHttp(_) | Action::Sign { .. }
    )));
    place(&mut engine, 111, 201.0);
    let next: Vec<_> = engine.drain_actions().collect();
    assert!(
        next.iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/order/submit"))),
        "a local NotSent failure must not consume the submit release slot"
    );
    assert!(
        !next
            .iter()
            .any(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == 650.0))
    );
}
