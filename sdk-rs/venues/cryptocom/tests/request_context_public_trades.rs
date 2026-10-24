//! Original real-venue controls for the unsigned one-page public history read.
use safe_cex_engine::request_context::{HttpAction, Preparation, RoutingIntent};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Method, Notice, Query, Response,
    TimerId, VenueError,
};
use safe_cex_venue_cryptocom::{Cryptocom, Options};

type TestEngine = Engine<Cryptocom>;
const NOW: f64 = 1_710_000_000_000.;
const ROW: &str =
    r#"{"d":"123","i":"BTC/USD+PERP","p":"100","q":"2","s":"SELL","t":1610000000000}"#;
const EMPTY: &str = r#"{"code":0,"result":{"data":[]}}"#;

fn engine(testnet: bool) -> TestEngine {
    safe_cex_engine::test_support::seeded_engine(Cryptocom::new(Options {
        testnet,
        ..Options::default()
    }))
}
fn query(owner: u32, limit: Option<u32>) -> Command {
    Command::Query {
        command: owner,
        query: Query::PublicTrades {
            symbol: "BTC/USD+PERP".into(),
            limit,
        },
    }
}
fn start(e: &mut TestEngine, now: f64, owner: u32, wait: Option<u32>, limit: Option<u32>) {
    e.handle_command(now, query(owner, limit), wait)
        .expect("finite host receipt");
}
fn prepared(e: &mut TestEngine, now: f64) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(e, now)
}
fn packet(actions: &[Action]) -> HttpRequest {
    let packets: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) if r.url.contains("/public/get-trades?") => Some(r.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(packets.len(), 1, "exactly one unsigned trade request");
    packets.into_iter().next().unwrap()
}
fn terminals(actions: &[Action], owner: u32) -> Vec<Result<String, VenueError>> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone { command, result }) if *command == owner => {
                Some(Ok(result.clone()))
            }
            Action::Notice(Notice::CommandFailed { command, error }) if *command == owner => {
                Some(Err(error.clone()))
            }
            _ => None,
        })
        .collect()
}
fn typed(actions: &[Action], owner: u32, kind: ErrorKind) {
    let result = terminals(actions, owner);
    assert_eq!(result.len(), 1, "one typed terminal for the actual caller");
    let error = result[0]
        .as_ref()
        .expect_err("must refuse, not return rows");
    assert_eq!(error.kind, Some(kind));
    assert_eq!(error.outcome, None);
}
fn no_send(actions: &[Action]) {
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::Sign { .. }))
    );
    assert!(!actions.iter().any(|a| matches!(a,
        Action::ManagedHttp(a) if !matches!(a.as_ref(), HttpAction::Cancel { .. })
    )));
}
fn timer(actions: &[Action], at: f64) -> TimerId {
    actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == at => Some(*timer),
            _ => None,
        })
        .expect("native timer at the exact expected instant")
}
fn reply(e: &mut TestEngine, request: &HttpRequest, now: f64, status: u16, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
}
fn body(rows: &str) -> String {
    format!(r#"{{"code":0,"result":{{"data":[{rows}]}}}}"#)
}
fn parked(e: &mut TestEngine, owner: u32) -> Preparation {
    start(e, NOW, owner, Some(50), Some(1));
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::Sign { .. })),
        "public trades must await the real route preparation"
    );
    let slots: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    intent,
                } => {
                    assert_eq!(*intent, RoutingIntent::Public);
                    assert_eq!(request.timeout_ms, Some(50));
                    Some(*preparation)
                }
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(slots.len(), 1);
    slots[0]
}

#[test]
fn public_trades_parked_preparation_is_inside_caller_wait() {
    let mut e = engine(false);
    let p = parked(&mut e, 1);
    assert!(e.http_preparation_pending(p));
    assert!(e.take_frame(NOW).is_none());
}

#[test]
fn public_trades_root_expiry_wins_response_at_equality() {
    let mut e = engine(false);
    start(&mut e, NOW, 2, Some(50), Some(1));
    let r = packet(&prepared(&mut e, NOW));
    reply(&mut e, &r, NOW + 50., 200, &body(ROW));
    let actions: Vec<_> = e.drain_actions().collect();
    typed(&actions, 2, ErrorKind::Timeout);
    assert!(!format!("{actions:?}").contains("INCOMPLETE"));
    no_send(&actions);
}

#[test]
fn public_trades_timer_first_expiry_denies_late_data() {
    let mut e = engine(false);
    start(&mut e, NOW, 3, Some(50), Some(1));
    let actions = prepared(&mut e, NOW);
    let r = packet(&actions);
    let root = timer(&actions, NOW + 50.);
    e.handle(NOW + 50., Event::TimerFired { timer: root });
    let actions: Vec<_> = e.drain_actions().collect();
    typed(&actions, 3, ErrorKind::Timeout);
    reply(&mut e, &r, NOW + 51., 200, &body(ROW));
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(terminals(&actions, 3).is_empty());
    no_send(&actions);
}

#[test]
fn public_trades_live_stream_prefix_cannot_finish_at_expiry() {
    let mut e = engine(false);
    start(&mut e, NOW, 4, Some(50), Some(1));
    let r = packet(&prepared(&mut e, NOW));
    e.handle(
        NOW + 1.,
        Event::HttpStart {
            request: r.id,
            status: 200,
            headers: "",
        },
    );
    let prefix = format!(r#"{{"code":0,"result":{{"data":[{ROW},"#);
    e.handle(
        NOW + 1.,
        Event::HttpChunk {
            request: r.id,
            data: prefix.as_bytes(),
        },
    );
    assert!(terminals(&e.drain_actions().collect::<Vec<_>>(), 4).is_empty());
    e.handle(
        NOW + 50.,
        Event::HttpChunk {
            request: r.id,
            data: format!("{ROW}]}}}}").as_bytes(),
        },
    );
    e.handle(NOW + 50., Event::HttpEnd { request: r.id });
    let actions: Vec<_> = e.drain_actions().collect();
    typed(&actions, 4, ErrorKind::Timeout);
    assert!(!format!("{actions:?}").contains("INCOMPLETE"));
}

#[test]
fn public_trades_expired_queued_owner_cannot_send_but_peer_survives() {
    let mut e = engine(false);
    start(&mut e, NOW, 5, Some(5000), Some(1));
    let peer = packet(&prepared(&mut e, NOW));
    start(&mut e, NOW + 1., 6, Some(5), Some(1));
    let queued: Vec<_> = e.drain_actions().collect();
    no_send(&queued);
    let due = timer(&queued, NOW + 10.);
    e.handle(NOW + 10., Event::TimerFired { timer: due });
    let actions = prepared(&mut e, NOW + 10.);
    no_send(&actions);
    typed(&actions, 6, ErrorKind::Timeout);
    reply(&mut e, &peer, NOW + 11., 200, &body(ROW));
    assert!(terminals(&e.drain_actions().collect::<Vec<_>>(), 5)[0].is_ok());
}

#[test]
fn public_trades_retry_wait_never_renews_the_receipt_root() {
    let mut e = engine(false);
    start(&mut e, NOW, 7, Some(50), Some(1));
    let r = packet(&prepared(&mut e, NOW));
    e.handle(
        NOW + 1.,
        Event::HttpFailed {
            request: r.id,
            error: "fixture reset",
        },
    );
    let waiting: Vec<_> = e.drain_actions().collect();
    let due = timer(&waiting, NOW + 1001.);
    e.handle(NOW + 1001., Event::TimerFired { timer: due });
    let actions = prepared(&mut e, NOW + 1001.);
    no_send(&actions);
    typed(&actions, 7, ErrorKind::Timeout);
}

#[test]
fn public_trades_duplicate_live_id_cannot_queue_another_read() {
    let mut e = engine(false);
    start(&mut e, NOW, 8, Some(5000), Some(1));
    let first = packet(&prepared(&mut e, NOW));
    start(&mut e, NOW + 20., 8, Some(5000), Some(1));
    let duplicate = prepared(&mut e, NOW + 20.);
    no_send(&duplicate);
    reply(&mut e, &first, NOW + 21., 200, &body(ROW));
    assert_eq!(
        terminals(&e.drain_actions().collect::<Vec<_>>(), 8).len(),
        1
    );
}

#[test]
fn public_trades_invalid_clock_refuses_before_io() {
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut e = engine(false);
        e.handle(now, Event::Command(query(9, Some(1))));
        let actions: Vec<_> = e.drain_actions().collect();
        no_send(&actions);
        typed(&actions, 9, ErrorKind::InvalidInput);
    }
}

#[test]
fn public_trades_capacity_is_shared_with_real_managed_peers() {
    let mut e = engine(false);
    for owner in 100..356 {
        e.handle(
            NOW,
            Event::Command(Command::FetchOrderBook {
                command: owner,
                symbol: "BTCUSD-PERP".into(),
                limit: None,
            }),
        );
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(actions.iter().any(|a| matches!(a, Action::ManagedHttp(_))));
    }
    start(&mut e, NOW, 10, Some(5000), Some(1));
    let actions: Vec<_> = e.drain_actions().collect();
    no_send(&actions);
    typed(&actions, 10, ErrorKind::Internal);
}

#[test]
fn native_public_trades_wire_network_defaults_and_limit_clamp() {
    for testnet in [false, true] {
        for (limit, count) in [(None, 25), (Some(1), 1), (Some(1000), 150)] {
            let mut e = engine(testnet);
            start(&mut e, NOW, 20, None, limit);
            let actions = prepared(&mut e, NOW);
            let r = packet(&actions);
            let host = if testnet {
                "https://uat-api.3ona.co/exchange/v1"
            } else {
                "https://api.crypto.com/exchange/v1"
            };
            assert_eq!(
                r.url,
                format!("{host}/public/get-trades?instrument_name=BTC%2FUSD%2BPERP&count={count}")
            );
            assert_eq!(r.method, Method::Get);
            assert!(r.headers.is_empty() && r.body.is_none());
            assert!(!actions.iter().any(|a| matches!(
                a,
                Action::Sign { .. } | Action::Notice(Notice::AccountAuth { .. })
            )));
            reply(&mut e, &r, NOW + 1., 200, EMPTY);
            assert_eq!(
                terminals(&e.drain_actions().collect::<Vec<_>>(), 20),
                vec![Ok("[]".into())]
            );
        }
    }
}

#[test]
fn native_public_trades_duplicate_rows_values_and_zero_limit() {
    let mut e = engine(false);
    start(&mut e, NOW, 21, None, Some(2));
    let r = packet(&prepared(&mut e, NOW));
    reply(
        &mut e,
        &r,
        NOW + 1.,
        200,
        &body(&format!("{ROW},{ROW},{ROW}")),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    let results = terminals(&actions, 21);
    let rows: Vec<serde_json::Value> = serde_json::from_str(results[0].as_ref().unwrap()).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0], rows[1]);
    assert_eq!(rows[0]["id"], "123");
    assert_eq!(rows[0]["quoteQty"], 200.);
    assert_eq!(rows[0]["timestamp"], 1_610_000_000_000_f64);
    assert_eq!(rows[0]["side"], "sell");
    start(&mut e, NOW + 2., 22, None, Some(0));
    let actions: Vec<_> = e.drain_actions().collect();
    assert_eq!(terminals(&actions, 22), vec![Ok("[]".into())]);
    no_send(&actions);
}

#[test]
fn native_public_trades_ten_ms_eligibility_is_shared_across_callers() {
    let mut e = engine(false);
    start(&mut e, NOW, 23, None, Some(1));
    let first = packet(&prepared(&mut e, NOW));
    start(&mut e, NOW + 1., 24, None, Some(1));
    let waiting: Vec<_> = e.drain_actions().collect();
    no_send(&waiting);
    let due = timer(&waiting, NOW + 10.);
    e.handle(NOW + 10., Event::TimerFired { timer: due });
    let second = packet(&prepared(&mut e, NOW + 10.));
    assert_ne!(first.id, second.id);
    for r in [&first, &second] {
        reply(&mut e, r, NOW + 11., 200, EMPTY);
    }
    let actions: Vec<_> = e.drain_actions().collect();
    assert_eq!(terminals(&actions, 23), vec![Ok("[]".into())]);
    assert_eq!(terminals(&actions, 24), vec![Ok("[]".into())]);
}

#[test]
fn native_public_trades_transport_four_attempts_and_page_reset() {
    let mut e = engine(false);
    start(&mut e, NOW, 25, None, Some(1));
    let mut r = packet(&prepared(&mut e, NOW));
    for attempt in 0..4 {
        let now = NOW + f64::from(attempt) * 1001.;
        e.handle(
            now,
            Event::HttpStart {
                request: r.id,
                status: 200,
                headers: "",
            },
        );
        let prefix = format!(r#"{{"code":0,"result":{{"data":[{ROW},"#);
        e.handle(
            now,
            Event::HttpChunk {
                request: r.id,
                data: prefix.as_bytes(),
            },
        );
        e.handle(
            now + 1.,
            Event::HttpFailed {
                request: r.id,
                error: "fixture reset",
            },
        );
        let actions: Vec<_> = e.drain_actions().collect();
        if attempt < 3 {
            assert!(terminals(&actions, 25).is_empty());
            let due = timer(&actions, now + 1001.);
            e.handle(now + 1001., Event::TimerFired { timer: due });
            r = packet(&prepared(&mut e, now + 1001.));
        } else {
            let results = terminals(&actions, 25);
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].as_ref().unwrap_err().message, "fixture reset");
            assert!(!format!("{actions:?}").contains("INCOMPLETE"));
            no_send(&actions);
        }
    }
    assert_eq!(e.health().auth_failures, 0);
}

#[test]
fn native_public_trades_retry_discards_partial_rows_before_success() {
    let mut e = engine(false);
    start(&mut e, NOW, 26, None, Some(2));
    let first = packet(&prepared(&mut e, NOW));
    e.handle(
        NOW + 1.,
        Event::HttpStart {
            request: first.id,
            status: 200,
            headers: "",
        },
    );
    let prefix = format!(r#"{{"code":0,"result":{{"data":[{ROW},"#);
    e.handle(
        NOW + 1.,
        Event::HttpChunk {
            request: first.id,
            data: prefix.as_bytes(),
        },
    );
    e.handle(
        NOW + 2.,
        Event::HttpFailed {
            request: first.id,
            error: "fixture reset",
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    let due = timer(&actions, NOW + 1002.);
    e.handle(NOW + 1002., Event::TimerFired { timer: due });
    let retry = packet(&prepared(&mut e, NOW + 1002.));
    reply(&mut e, &retry, NOW + 1003., 200, &body(ROW));
    let actions: Vec<_> = e.drain_actions().collect();
    let results = terminals(&actions, 26);
    let rows: Vec<serde_json::Value> = serde_json::from_str(results[0].as_ref().unwrap()).unwrap();
    assert_eq!(rows.len(), 1, "failed page prefix is never accepted");
}

#[test]
fn native_public_trades_40101_has_one_unsigned_retry_and_no_auth_evidence() {
    let mut e = engine(false);
    start(&mut e, NOW, 27, None, Some(1));
    let first = packet(&prepared(&mut e, NOW));
    let refused = r#"{"code":40101,"message":"fixture retry","result":{"data":[]}}"#;
    reply(&mut e, &first, NOW + 1., 200, refused);
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(terminals(&actions, 27).is_empty());
    let due = timer(&actions, NOW + 1001.);
    e.handle(NOW + 1001., Event::TimerFired { timer: due });
    let retry = packet(&prepared(&mut e, NOW + 1001.));
    assert!(retry.headers.is_empty() && retry.body.is_none());
    reply(&mut e, &retry, NOW + 1002., 200, refused);
    let actions: Vec<_> = e.drain_actions().collect();
    assert_eq!(terminals(&actions, 27).len(), 1);
    assert!(terminals(&actions, 27)[0].is_err());
    no_send(&actions);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::AccountAuth { .. })))
    );
    assert_eq!(e.health().auth_failures, 0);
}

#[test]
fn native_public_trades_malformed_or_other_refusal_has_no_retry_or_prefix() {
    for response in [
        body(&format!("{ROW},{{}}")),
        r#"{"code":40001,"message":"fixture refusal","result":{"data":[]}}"#.into(),
    ] {
        let mut e = engine(false);
        start(&mut e, NOW, 28, None, Some(2));
        let r = packet(&prepared(&mut e, NOW));
        reply(&mut e, &r, NOW + 1., 200, &response);
        let actions: Vec<_> = e.drain_actions().collect();
        let results = terminals(&actions, 28);
        assert_eq!(results.len(), 1);
        assert!(results[0].is_err());
        assert!(!format!("{actions:?}").contains("INCOMPLETE"));
        assert!(!actions.iter().any(|a| matches!(a, Action::SetTimer { .. })));
        no_send(&actions);
    }
}

#[test]
fn native_public_trades_missing_stream_array_uses_transport_retry() {
    let mut e = engine(false);
    start(&mut e, NOW, 29, None, Some(1));
    let first = packet(&prepared(&mut e, NOW));
    reply(
        &mut e,
        &first,
        NOW + 1.,
        200,
        r#"{"code":40001,"message":"fixture no array"}"#,
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(terminals(&actions, 29).is_empty());
    let due = timer(&actions, NOW + 1001.);
    e.handle(NOW + 1001., Event::TimerFired { timer: due });
    let retry = packet(&prepared(&mut e, NOW + 1001.));
    reply(&mut e, &retry, NOW + 1002., 200, EMPTY);
    assert_eq!(
        terminals(&e.drain_actions().collect::<Vec<_>>(), 29),
        vec![Ok("[]".into())]
    );
}
