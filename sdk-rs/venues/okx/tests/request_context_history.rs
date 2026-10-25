//! Original controls: caller-wait propagation and managed history are absent.
#[allow(dead_code)]
mod support;
use safe_cex_engine::request_context::HttpAction;
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Notice, Query, Response, TimerId,
};
use safe_cex_venue_okx::{Okx, Options};

const NOW: f64 = 1_710_000_000_000.;
const OWNER: u32 = 778;

fn ready() -> Engine<Okx> {
    let mut engine = support::trading_engine();
    engine.drain_actions().for_each(drop);
    engine.take_frame(NOW);
    engine
}
fn orders() -> Query {
    Query::OrderHistory {
        symbol: Some("BTCUSDT".into()),
        from_ms: None,
        to_ms: None,
        limit: None,
    }
}
fn lookup() -> Query {
    Query::OrderByClientId {
        symbol: "BTCUSDT".into(),
        client_order_id: "caller1".into(),
    }
}
fn public_trades() -> Query {
    Query::PublicTrades {
        symbol: "BTCUSDT".into(),
        limit: Some(2),
    }
}

// Valid receipts use the reviewed #1945 API. The nonfinite-clock control keeps
// exercising plain handle's unstamped entry and normal admission refusal.
fn deliver(engine: &mut Engine<Okx>, at: f64, query: Query, timeout_ms: Option<u32>) {
    let command = Command::Query {
        command: OWNER,
        query,
    };
    if at.is_finite() {
        engine
            .handle_command(at, command, timeout_ms)
            .expect("valid caller receipt");
    } else {
        engine.handle(at, Event::Command(command));
    }
}
fn actions(engine: &mut Engine<Okx>, at: f64) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(engine, at)
}
fn request(output: Vec<Action>) -> HttpRequest {
    let mut requests = output.into_iter().filter_map(|a| match a {
        Action::Http(r) if r.url.contains("/trade/") || r.url.contains("/market/trades") => Some(r),
        _ => None,
    });
    let request = requests.next().expect("native history request");
    assert!(requests.next().is_none());
    request
}
fn reply(engine: &mut Engine<Okx>, request: &HttpRequest, at: f64, body: &str) {
    engine.handle(
        at,
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
fn page(number: usize) -> String {
    let rows = (0..100).map(|index| {
        let id = 100_000 - number * 100 - index;
        format!(r#"{{"instId":"BTC-USDT-SWAP","ordId":"{id}","side":"sell","ordType":"post_only","state":"mmp_canceled","sz":"3","accFillSz":"1.5","px":"41000","avgPx":"","reduceOnly":"true","posSide":"net","cTime":"1710000000123","uTime":"1710000000200"}}"#)
    }).collect::<Vec<_>>().join(",");
    support::envelope(&rows)
}
fn paced(output: &[Action], at: f64) -> TimerId {
    output
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == at => Some(*timer),
            _ => None,
        })
        .expect("exact native100ms continuation")
}
fn done(output: &[Action]) -> Option<&str> {
    output.iter().find_map(|a| match a {
        Action::Notice(Notice::CommandDone {
            command: OWNER,
            result,
        }) => Some(result.as_str()),
        _ => None,
    })
}
fn incomplete_markers(output: &[Action]) -> Vec<usize> {
    output
        .iter()
        .enumerate()
        .filter(|(_, a)| matches!(a, Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE))
        .map(|(index, _)| index)
        .collect()
}
fn failed(output: &[Action]) -> Vec<&safe_cex_engine::VenueError> {
    output
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed {
                command: OWNER,
                error,
            }) => Some(error),
            _ => None,
        })
        .collect()
}
/// A failure with no accepted rows rejects: an empty result would read as "no history".
fn strict_failure(output: &[Action]) {
    assert!(
        !output
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_))),
        "a failed root cannot issue another page"
    );
    assert!(
        done(output).is_none(),
        "no empty success for a failed first page"
    );
    assert_eq!(failed(output).len(), 1);
    assert!(incomplete_markers(output).is_empty());
}
fn order_ids(result: &str) -> Vec<String> {
    let rows: serde_json::Value = serde_json::from_str(result).expect("history JSON");
    rows.as_array()
        .expect("history rows")
        .iter()
        .map(|row| row["id"].as_str().expect("order id").to_owned())
        .collect()
}
fn managed(output: &[Action], endpoint: &str) {
    assert!(
        !output.iter().any(|a| matches!(a, Action::Http(_))),
        "no legacy physical dispatch before managed preparation"
    );
    assert_eq!(output.iter().filter(|a| matches!(a, Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Prepare { request, .. } if request.url.contains(endpoint)))).count(), 1);
}

#[test]
fn host_invalid_clock_returns_typed_error_without_dispatch() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Okx::new(Options::default()));
    let error = engine
        .handle_command(
            f64::NAN,
            Command::Query {
                command: OWNER,
                query: orders(),
            },
            None,
        )
        .expect_err("invalid host receipt must not dispatch");
    assert_eq!(error.kind(), ErrorKind::InvalidInput);
    assert!(engine.drain_actions().next().is_none());
}

#[test]
fn missing_receipt_at_finite_clock_cannot_substitute_a_default_root() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Okx::new(Options::default()));
    engine.handle(
        f64::MAX,
        Event::Command(Command::Query {
            command: OWNER,
            query: orders(),
        }),
    );
    let output: Vec<_> = engine.drain_actions().collect();
    assert_eq!(output.len(), 1, "receipt refusal has no effects");
    assert!(
        matches!(
            &output[0],
            Action::Notice(Notice::CommandFailed { command: OWNER, error })
                if error.kind == Some(ErrorKind::InvalidInput) && error.outcome.is_none()
        ),
        "missing receipt must fail closed through typed Read entry refusal"
    );
}

#[test]
fn original_local_empty_read_requires_genuine_admission_at_invalid_time() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Okx::new(Options::default()));
    deliver(&mut engine, f64::NAN, orders(), None);
    let output: Vec<_> = engine.drain_actions().collect();
    assert!(output.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: OWNER, error }) if error.kind == Some(ErrorKind::InvalidInput) && error.outcome.is_none())), "native[] cannot bypass invalid entry admission");
    assert!(done(&output).is_none());
}
#[test]
fn original_private_history_requires_managed_page_preparation() {
    let mut engine = ready();
    deliver(&mut engine, NOW, orders(), None);
    managed(
        &engine.drain_actions().collect::<Vec<_>>(),
        "/trade/orders-history",
    );
}
#[test]
fn original_public_trades_require_managed_page_preparation() {
    let mut engine = ready();
    deliver(&mut engine, NOW, public_trades(), None);
    managed(
        &engine.drain_actions().collect::<Vec<_>>(),
        "/market/trades",
    );
}
#[test]
fn original_primary_lookup_requires_managed_page_preparation() {
    let mut engine = ready();
    deliver(&mut engine, NOW, lookup(), None);
    managed(&engine.drain_actions().collect::<Vec<_>>(), "/trade/order?");
}
#[test]
fn revised_short_caller_root_stops_a_page_waiting_at_dequeue() {
    let mut engine = ready();
    deliver(&mut engine, NOW, orders(), Some(250));
    let first = request(actions(&mut engine, NOW));
    reply(&mut engine, &first, NOW + 200., &page(0));
    let output = actions(&mut engine, NOW + 200.);
    let timer = paced(&output, NOW + 300.);
    engine.handle(NOW + 300., Event::TimerFired { timer });
    root_expired(&actions(&mut engine, NOW + 300.));
    assert!(engine.take_frame(NOW + 300.).is_none());
}
#[test]
fn revised_default_caller_root_does_not_renew_across_valid_page_children() {
    let mut engine = ready();
    deliver(&mut engine, NOW, orders(), None);
    let mut current = request(actions(&mut engine, NOW));
    for number in 0..8 {
        let at = NOW + (number as f64 + 1.) * 15_000.;
        reply(&mut engine, &current, at, &page(number));
        let output = actions(&mut engine, at);
        if number == 7 {
            root_expired(&output);
            assert!(engine.take_frame(at).is_none());
            return;
        }
        let timer = paced(&output, at + 100.);
        engine.handle(at + 100., Event::TimerFired { timer });
        current = request(actions(&mut engine, at + 100.));
    }
    unreachable!()
}
#[test]
fn original_short_root_expiry_never_turns_lookup_into_definitive_absence() {
    let mut engine = ready();
    deliver(&mut engine, NOW, lookup(), Some(1000));
    let request = request(actions(&mut engine, NOW));
    reply(
        &mut engine,
        &request,
        NOW + 1000.,
        r#"{"code":"51603","msg":"not found","data":[]}"#,
    );
    let output = actions(&mut engine, NOW + 1000.);
    assert!(
        done(&output).is_none(),
        "expired lookup cannot publish native absence"
    );
    assert!(output.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandFailed { command: OWNER, .. })
    )));
}

#[test]
fn finite_local_precedence_and_native_lookup_sentinels_remain_compatible() {
    let mut engine = ready();
    for (query, expected) in [
        (
            Query::OrderHistory {
                symbol: Some("UNKNOWN".into()),
                from_ms: Some(f64::NAN),
                to_ms: None,
                limit: None,
            },
            "invalid OKX history time bounds",
        ),
        (
            Query::OrderByClientId {
                symbol: "UNKNOWN".into(),
                client_order_id: "bad id".into(),
            },
            "Invalid OKX client order ID",
        ),
    ] {
        deliver(&mut engine, NOW, query, None);
        let output = actions(&mut engine, NOW);
        assert!(output.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: OWNER, error }) if error.message == expected)));
        assert!(
            !output
                .iter()
                .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
        );
    }
    deliver(
        &mut engine,
        NOW,
        Query::OrderByClientId {
            symbol: "UNKNOWN".into(),
            client_order_id: "caller1".into(),
        },
        None,
    );
    assert_eq!(
        done(&actions(&mut engine, NOW)),
        Some(r#"{"found":false,"definitive":false}"#)
    );
}
#[test]
fn failed_later_query_page_keeps_accepted_rows_with_one_incomplete_marker() {
    let first_of_failing_page = page(1)
        .split("},{")
        .next()
        .and_then(|head| head.split_once("[{"))
        .map(|(_, row)| format!("{{{row}}}"))
        .expect("first row of page 1");
    for body in [
        r#"{"code":"51000","msg":"bad request","data":[]}"#.to_owned(),
        support::envelope("42"),
        support::envelope(&format!("{first_of_failing_page},42")),
    ] {
        let mut engine = ready();
        deliver(&mut engine, NOW, orders(), None);
        let first = request(actions(&mut engine, NOW));
        reply(&mut engine, &first, NOW + 10., &page(0));
        let output = actions(&mut engine, NOW + 10.);
        let timer = paced(&output, NOW + 110.);
        engine.handle(NOW + 110., Event::TimerFired { timer });
        let second = request(actions(&mut engine, NOW + 110.));
        reply(&mut engine, &second, NOW + 120., &body);
        let output = actions(&mut engine, NOW + 120.);
        assert!(
            !output
                .iter()
                .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
        );
        assert!(
            failed(&output).is_empty(),
            "accepted rows are a partial, not a failure"
        );
        let result = done(&output).expect("accepted rows are published");
        let ids = order_ids(result);
        assert!(!ids.is_empty());
        assert!(
            ids.iter().all(|id| id.parse::<usize>().unwrap() > 99_900),
            "only the accepted page: the failing page's parsed rows are dropped"
        );
        let markers = incomplete_markers(&output);
        assert_eq!(markers.len(), 1, "one command-owned incomplete marker");
        let done_at = output
            .iter()
            .position(|a| {
                matches!(
                    a,
                    Action::Notice(Notice::CommandDone { command: OWNER, .. })
                )
            })
            .unwrap();
        assert!(
            markers[0] < done_at,
            "the marker precedes the result it qualifies"
        );
    }
}
#[test]
fn failed_first_query_page_is_a_typed_rejection_not_an_empty_result() {
    for body in [
        r#"{"code":"51000","msg":"bad request","data":[]}"#.to_owned(),
        support::envelope("42"),
    ] {
        let mut engine = ready();
        deliver(&mut engine, NOW, orders(), None);
        let first = request(actions(&mut engine, NOW));
        reply(&mut engine, &first, NOW + 10., &body);
        let output = actions(&mut engine, NOW + 10.);
        strict_failure(&output);
        assert!(failed(&output)[0].message.starts_with("OKX history: "));
    }
}
#[test]
fn native_lookup_transport_failure_is_strict_and_has_no_definitive_absence() {
    let mut engine = ready();
    deliver(&mut engine, NOW, lookup(), None);
    let request = request(actions(&mut engine, NOW));
    assert_eq!(
        request
            .route
            .unwrap_or(safe_cex_engine::RequestRoute::Primary),
        safe_cex_engine::RequestRoute::Primary
    );
    engine.handle(
        NOW + 1.,
        Event::HttpFailed {
            request: request.id,
            error: "fixture transport loss",
        },
    );
    let output = actions(&mut engine, NOW + 1.);
    assert!(done(&output).is_none());
    assert!(output.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: OWNER, error }) if error.message.contains("fixture transport loss"))));
}
#[test]
fn native_page_budget_starts_at_dequeue_not_when_it_enters_the_wait() {
    let mut engine = ready();
    deliver(&mut engine, NOW, orders(), None);
    let first = request(actions(&mut engine, NOW));
    reply(&mut engine, &first, NOW + 19_000., &page(0));
    let output = actions(&mut engine, NOW + 19_000.);
    let timer = paced(&output, NOW + 19_100.);
    engine.handle(NOW + 19_100., Event::TimerFired { timer });
    let second = request(actions(&mut engine, NOW + 19_100.));
    reply(&mut engine, &second, NOW + 39_050., &support::envelope(""));
    let output = actions(&mut engine, NOW + 39_050.);
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(done(&output).expect("child remains live after enqueue+20s")).unwrap();
    assert_eq!(rows.len(), 100);
}
#[test]
fn longer_caller_wait_must_not_be_replaced_by_a_fixed_default_root() {
    let mut engine = ready();
    deliver(&mut engine, NOW, orders(), Some(180_000));
    let mut current = request(actions(&mut engine, NOW));
    for number in 0..13 {
        let at = NOW + (number as f64 + 1.) * 10_000.;
        let body = if number == 12 {
            support::envelope("")
        } else {
            page(number)
        };
        reply(&mut engine, &current, at, &body);
        let output = actions(&mut engine, at);
        if number == 12 {
            let rows: Vec<serde_json::Value> = serde_json::from_str(
                done(&output).expect("configured180s root remains live past default120s"),
            )
            .unwrap();
            assert_eq!(rows.len(), 1200);
            return;
        }
        let timer = paced(&output, at + 100.);
        engine.handle(at + 100., Event::TimerFired { timer });
        current = request(actions(&mut engine, at + 100.));
    }
}

fn root_expired(output: &[Action]) {
    assert!(
        done(output).is_none(),
        "root expiry never softens into empty success"
    );
    let failures: Vec<_> = output
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed {
                command: OWNER,
                error,
            }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].kind, Some(ErrorKind::Timeout));
    assert!(failures[0].outcome.is_none());
    assert!(!output.iter().any(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE)));
    assert!(!output.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::ManagedHttp(_) | Action::Notice(Notice::CommandError { .. })
    )));
}
#[test]
fn revised_parked_root_timer_expires_without_pacing_or_prefix_publication() {
    let mut engine = ready();
    deliver(&mut engine, NOW, orders(), Some(250));
    let output = actions(&mut engine, NOW);
    let timer = output
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 250. => Some(*timer),
            _ => None,
        })
        .expect("engine caller-root timer independent of native pacing");
    let first = request(output);
    reply(&mut engine, &first, NOW + 200., &page(0));
    let waiting = actions(&mut engine, NOW + 200.);
    paced(&waiting, NOW + 300.);
    engine.handle(NOW + 250., Event::TimerFired { timer });
    root_expired(&actions(&mut engine, NOW + 250.));
}

#[test]
fn managed_primary_lookup_declares_primary_egress_before_signing() {
    let mut engine = ready();
    deliver(&mut engine, NOW, lookup(), None);
    let output: Vec<_> = engine.drain_actions().collect();
    assert_eq!(output.iter().filter(|a| matches!(a, Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Prepare { intent: safe_cex_engine::request_context::RoutingIntent::Primary, request, .. } if request.url.contains("/trade/order?")))).count(), 1);
}

#[test]
fn child_timeout_distinguishes_unreleased_and_released_query_work() {
    for released in [false, true] {
        let mut engine = ready();
        deliver(&mut engine, NOW, orders(), Some(60_000));
        let output = if released {
            actions(&mut engine, NOW)
        } else {
            engine.drain_actions().collect()
        };
        let timer = output
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == NOW + 20_000. => Some(*timer),
                _ => None,
            })
            .expect("native20s child timer distinct from caller root");
        engine.handle(NOW + 20_000., Event::TimerFired { timer });
        let output = actions(&mut engine, NOW + 20_000.);
        if released {
            strict_failure(&output);
            assert_eq!(failed(&output)[0].kind, Some(ErrorKind::Timeout));
        } else {
            assert!(done(&output).is_none());
            let failures: Vec<_> = output
                .iter()
                .filter_map(|a| match a {
                    Action::Notice(Notice::CommandFailed {
                        command: OWNER,
                        error,
                    }) => Some(error),
                    _ => None,
                })
                .collect();
            assert_eq!(failures.len(), 1);
            assert_eq!(failures[0].kind, Some(ErrorKind::Timeout));
            assert_eq!(failures[0].outcome, Some(safe_cex_engine::Outcome::NotSent));
            assert!(
                !output
                    .iter()
                    .any(|a| matches!(a, Action::Notice(Notice::CommandError { .. })))
            );
        }
    }
}

#[test]
fn preparation_refusal_keeps_typed_metadata_for_query_lookup_and_public_read() {
    for query in [orders(), lookup(), public_trades()] {
        let mut engine = ready();
        let public = matches!(query, Query::PublicTrades { .. });
        deliver(&mut engine, NOW, query, None);
        let preparation = engine
            .drain_actions()
            .find_map(|a| match a {
                Action::ManagedHttp(action) => match *action {
                    HttpAction::Prepare { preparation, .. } => Some(preparation),
                    _ => None,
                },
                _ => None,
            })
            .expect("genuine managed preparation");
        engine.http_preparation_failed(
            NOW + 1.,
            preparation,
            safe_cex_engine::request_context::Error::State,
            safe_cex_engine::VenueError::new("provider route refused")
                .kind(ErrorKind::AuthInvalid)
                .venue_code("native-code")
                .venue_message("native detail"),
        );
        let output: Vec<_> = engine.drain_actions().collect();
        let failures: Vec<_> = output
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandFailed {
                    command: OWNER,
                    error,
                }) => Some(error),
                _ => None,
            })
            .collect();
        assert_eq!(failures.len(), 1);
        let error = failures[0];
        assert_eq!(error.kind, Some(ErrorKind::AuthInvalid));
        assert_eq!(error.message, "provider route refused");
        assert_eq!(error.venue_code.as_deref(), Some("native-code"));
        assert_eq!(error.venue_message.as_deref(), Some("native detail"));
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::NotSent));
        assert!(done(&output).is_none());
        assert!(!output.iter().any(|a| match a {
            Action::Notice(Notice::CommandError { .. }) | Action::Http(_) => true,
            Action::ManagedHttp(action) => matches!(
                action.as_ref(),
                HttpAction::Prepare { .. } | HttpAction::Bind { .. }
            ),
            _ => false,
        }));
        if public {
            assert!(
                !output
                    .iter()
                    .any(|a| matches!(a, Action::Notice(Notice::AccountAuth { .. })))
            );
        }
    }
}

#[test]
fn dequeued_child_capacity_refusal_discards_rows_without_completing_peers() {
    let mut engine = ready();
    deliver(&mut engine, NOW, orders(), Some(60_000));
    let first = request(actions(&mut engine, NOW));
    reply(&mut engine, &first, NOW + 10., &page(0));
    let waiting = actions(&mut engine, NOW + 10.);
    let timer = paced(&waiting, NOW + 110.);
    let mut peers = 0;
    let mut preparations = Vec::new();
    let mut full = false;
    for command in 1000..1600 {
        engine.handle(
            NOW + 20.,
            Event::Command(Command::FetchCandles {
                command,
                symbol: "BTCUSDT".into(),
                interval: safe_cex_engine::model::Interval::M1,
                limit: Some(1),
                from_ms: None,
                to_ms: None,
            }),
        );
        let output: Vec<_> = engine.drain_actions().collect();
        assert!(!output.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandFailed { command: OWNER, .. })
        )));
        if output.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: id, error }) if *id == command && error.kind == Some(ErrorKind::Internal))) {
            full = true;
            break;
        }
        assert!(output.iter().any(|a| matches!(a, Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Prepare { .. }))));
        preparations.extend(output.iter().filter_map(|a| match a {
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare { preparation, .. } => Some(*preparation),
                _ => None,
            },
            _ => None,
        }));
        peers += 1;
    }
    assert!(full && peers > 0, "genuine peer roots fill shared registry");
    assert_eq!(preparations.len(), peers);
    assert!(
        preparations
            .iter()
            .all(|preparation| engine.http_preparation_pending(*preparation))
    );
    engine.handle(NOW + 110., Event::TimerFired { timer });
    let output: Vec<_> = engine.drain_actions().collect();
    let failures: Vec<_> = output
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command, error }) => Some((*command, error)),
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].0, OWNER);
    assert_eq!(failures[0].1.kind, Some(ErrorKind::Internal));
    assert_eq!(failures[0].1.message, "Engine request capacity reached");
    assert!(failures[0].1.outcome.is_none());
    assert!(done(&output).is_none());
    assert!(!output.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandError { .. }) | Action::Http(_)
    )));
    assert!(
        preparations
            .iter()
            .all(|preparation| engine.http_preparation_pending(*preparation))
    );
    assert!(!output.iter().any(|a| matches!(a, Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Prepare { .. } | HttpAction::Bind { .. }))));
}

#[test]
fn retired_native_pacing_timer_cannot_send_for_reused_command_id() {
    let mut engine = ready();
    deliver(&mut engine, NOW, orders(), Some(250));
    let first = request(actions(&mut engine, NOW));
    reply(&mut engine, &first, NOW + 200., &page(0));
    let waiting = actions(&mut engine, NOW + 200.);
    let old_timer = paced(&waiting, NOW + 300.);
    engine.prepare_dispatch(NOW + 250.);
    root_expired(&actions(&mut engine, NOW + 250.));
    deliver(&mut engine, NOW + 260., orders(), Some(60_000));
    let current = request(actions(&mut engine, NOW + 260.));
    assert_ne!(first.id, current.id);
    engine.handle(NOW + 300., Event::TimerFired { timer: old_timer });
    let output = actions(&mut engine, NOW + 300.);
    assert!(!output.iter().any(|a| matches!(
        a,
        Action::Http(_)
            | Action::ManagedHttp(_)
            | Action::Notice(
                Notice::CommandDone { command: OWNER, .. }
                    | Notice::CommandFailed { command: OWNER, .. }
            )
    )));
    reply(&mut engine, &first, NOW + 301., &support::envelope(""));
    assert!(actions(&mut engine, NOW + 301.).is_empty());
    reply(&mut engine, &current, NOW + 302., &support::envelope(""));
    let output = actions(&mut engine, NOW + 302.);
    assert_eq!(done(&output), Some("[]"));
}
