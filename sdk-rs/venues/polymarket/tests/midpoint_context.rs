//! Native midpoint batches retain wire/publication rules under Background Read custody.
use safe_cex_engine::request_context::{HttpAction, Prepared};
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Method, Notice, Response};
use safe_cex_venue_polymarket::{Options, Polymarket};

type Driver = Engine<Polymarket>;
const ADMITTED: f64 = 10.;
const DEADLINE: f64 = ADMITTED + 20_000.;

fn catalogue(tokens: usize) -> Driver {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Polymarket::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    engine.start(0.);
    let request = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains("gamma-api.polymarket.com/markets/keyset") => Some(r),
            _ => None,
        })
        .expect("native Gamma catalogue");
    let ids: Vec<_> = (1..=tokens).map(|n| n.to_string()).collect();
    let outcomes: Vec<_> = (1..=tokens).map(|n| format!("Outcome{n}")).collect();
    let body = serde_json::json!({"markets":[{
        "conditionId":"0x123", "slug":"election", "active":true, "closed":false,
        "clobTokenIds":serde_json::to_string(&ids).unwrap(),
        "outcomes":serde_json::to_string(&outcomes).unwrap(),
        "orderPriceMinTickSize":0.01,"orderMinSize":5
    }]})
    .to_string();
    engine.handle(
        ADMITTED,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: &body,
            },
        },
    );
    assert_eq!(engine.store().markets().len(), tokens);
    assert!(!engine.store().loaded().tickers);
    engine
}
fn actions(engine: &mut Driver, now: f64) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(engine, now)
}
fn no_command_or_signing(a: &[Action]) {
    assert!(!a.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
            | Action::Sign { .. }
    )));
}
fn midpoint(a: Vec<Action>) -> HttpRequest {
    no_command_or_signing(&a);
    let mut requests = a.into_iter().filter_map(|a| match a {
        Action::Http(r) => Some(r),
        _ => None,
    });
    let request = requests.next().expect("actual released midpoint request");
    assert!(requests.next().is_none());
    assert_eq!(request.method, Method::Post);
    assert_eq!(request.url, "https://clob.polymarket.com/midpoints");
    assert_eq!(
        request.headers,
        vec![("Content-Type", "application/json".into())]
    );
    request
}
fn answer(engine: &mut Driver, request: &HttpRequest, now: f64, body: &str) -> Vec<Action> {
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
    actions(engine, now)
}
fn body_ids(request: &HttpRequest) -> Vec<String> {
    #[derive(serde::Deserialize)]
    struct Token {
        token_id: String,
    }
    serde_json::from_str::<Vec<Token>>(request.body.as_deref().unwrap())
        .unwrap()
        .into_iter()
        .map(|t| t.token_id)
        .collect()
}

#[test]
fn midpoint_does_not_release_before_host_preparation() {
    let mut engine = catalogue(2);
    let a: Vec<_> = engine.drain_actions().collect();
    assert!(
        !a.iter().any(|a| matches!(a, Action::Http(_))),
        "midpoint escaped before Prepare/Bind/Claim"
    );
    assert!(a.iter().any(|a| matches!(a, Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Prepare { request, .. } if request.url.ends_with("/midpoints")))));
    no_command_or_signing(&a);
}
#[test]
fn midpoint_at_deadline_cannot_publish_and_preserves_failure_progress() {
    let mut engine = catalogue(102);
    let request = midpoint(actions(&mut engine, ADMITTED));
    let a = answer(&mut engine, &request, DEADLINE, r#"{"1":"0.4"}"#);
    assert!(
        engine.store().tickers().is_empty(),
        "expired midpoint published native quotes"
    );
    assert!(!engine.store().loaded().tickers);
    let retry_timer = a
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if (750.0..=1000.).contains(&(*at_ms - DEADLINE)) => {
                Some(*timer)
            }
            _ => None,
        })
        .expect("typed timeout retains the failed offset's native backoff");
    let next = midpoint(a);
    assert_eq!(
        body_ids(&next),
        vec!["101", "102"],
        "failure-owned next batch may advance"
    );
    answer(&mut engine, &next, DEADLINE + 1., "{}");
    assert!(!engine.store().loaded().tickers);
    engine.handle(DEADLINE + 1000., Event::TimerFired { timer: retry_timer });
    let retry = midpoint(actions(&mut engine, DEADLINE + 1000.));
    assert_eq!(body_ids(&retry), body_ids(&request));
    assert_ne!(retry.id, request.id);
    answer(&mut engine, &retry, DEADLINE + 1001., r#"{"1":"0.4"}"#);
    assert!(engine.store().loaded().tickers);
}
#[test]
fn unprepared_midpoint_times_out_and_late_bind_is_inert() {
    let mut engine = catalogue(2);
    let a: Vec<_> = engine.drain_actions().collect();
    let timer = a
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == DEADLINE => Some(*timer),
            _ => None,
        })
        .expect("admission deadline exists before host Prepare");
    let token = a
        .into_iter()
        .find_map(|a| match a {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .expect("managed midpoint preparation");
    engine.handle(DEADLINE, Event::TimerFired { timer });
    assert!(!engine.http_preparation_pending(token));
    engine.http_prepared(
        DEADLINE,
        token,
        Prepared {
            id: token.attempt().request.0,
            expires_at: None,
        },
    );
    let a: Vec<_> = engine.drain_actions().collect();
    assert!(!a.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(!a.iter().any(|a| matches!(a, Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Bind { .. }))));
    assert!(!engine.store().loaded().tickers);
    no_command_or_signing(&a);
}
#[test]
fn native_unsigned_wire_and_subset_quotes_are_preserved() {
    let mut engine = catalogue(2);
    let request = midpoint(actions(&mut engine, ADMITTED));
    assert_eq!(
        request.body.as_deref(),
        Some(r#"[{"token_id":"1"},{"token_id":"2"}]"#)
    );
    let a = answer(&mut engine, &request, 11., r#"{"1":"0.4","999":"0.9"}"#);
    assert!(engine.store().loaded().tickers);
    assert_eq!(engine.store().tickers().len(), 1);
    let quote = &engine.store().tickers()[0];
    assert_eq!(quote.id, "1");
    assert_eq!(
        (quote.bid, quote.ask, quote.last, quote.mark, quote.index),
        (0.4, 0.4, 0.4, 0.4, 0.4)
    );
    no_command_or_signing(&a);
}
#[test]
fn bookless_empty_success_completes_the_seed_without_fake_quotes() {
    let mut engine = catalogue(2);
    let request = midpoint(actions(&mut engine, ADMITTED));
    let a = answer(&mut engine, &request, 11., "{}");
    assert!(engine.store().loaded().tickers);
    assert!(engine.store().tickers().is_empty());
    assert!(!a.iter().any(|a| matches!(a, Action::Http(_))));
    no_command_or_signing(&a);
}
#[test]
fn native_transport_failure_waits_for_backoff_then_recovers_same_batch() {
    let mut engine = catalogue(2);
    let request = midpoint(actions(&mut engine, ADMITTED));
    engine.handle(
        11.,
        Event::HttpFailed {
            request: request.id,
            error: "native midpoint failure",
        },
    );
    let a = actions(&mut engine, 11.);
    assert!(!a.iter().any(|a| matches!(a, Action::Http(_))));
    let timer = a
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if (750.0..=1000.).contains(&(*at_ms - 11.)) => {
                Some(*timer)
            }
            _ => None,
        })
        .expect("native one-second jittered retry");
    no_command_or_signing(&a);
    engine.handle(1011., Event::TimerFired { timer });
    let retry = midpoint(actions(&mut engine, 1011.));
    assert_ne!(retry.id, request.id);
    assert_eq!(retry.body, request.body);
    answer(&mut engine, &retry, 1012., r#"{"1":"0.4","2":"0.6"}"#);
    assert!(engine.store().loaded().tickers);
    assert_eq!(engine.store().tickers().len(), 2);
}
#[test]
fn native_hundred_token_batches_ignore_cross_batch_rows() {
    let mut engine = catalogue(102);
    let first = midpoint(actions(&mut engine, ADMITTED));
    assert_eq!(
        body_ids(&first),
        (1..=100).map(|n| n.to_string()).collect::<Vec<_>>()
    );
    let next = midpoint(answer(
        &mut engine,
        &first,
        11.,
        r#"{"1":"0.4","101":"0.9"}"#,
    ));
    assert_eq!(body_ids(&next), vec!["101", "102"]);
    assert!(
        engine.store().loaded().tickers,
        "native seed publishes accepted partial batches"
    );
    assert_eq!(engine.store().tickers().len(), 1);
    assert_eq!(engine.store().tickers()[0].id, "1");
    let a = answer(&mut engine, &next, 12., r#"{"101":"0.6"}"#);
    assert!(engine.store().loaded().tickers);
    assert_eq!(engine.store().tickers().len(), 2);
    assert_eq!(
        engine
            .store()
            .tickers()
            .iter()
            .find(|t| t.id == "101")
            .unwrap()
            .last,
        0.6
    );
    no_command_or_signing(&a);
}
#[test]
fn malformed_response_uses_native_retry_and_keeps_tickers_unloaded() {
    let mut engine = catalogue(2);
    let request = midpoint(actions(&mut engine, ADMITTED));
    let a = answer(&mut engine, &request, 11., "[]");
    assert!(!engine.store().loaded().tickers);
    assert!(engine.store().tickers().is_empty());
    assert!(a.iter().any(
        |a| matches!(a, Action::SetTimer { at_ms, .. } if (750.0..=1000.).contains(&(*at_ms-11.)))
    ));
    assert!(!a.iter().any(|a| matches!(a, Action::Http(_))));
    no_command_or_signing(&a);
}

#[test]
fn native_failed_offset_backs_off_while_next_batch_advances() {
    let mut engine = catalogue(102);
    let first = midpoint(actions(&mut engine, ADMITTED));
    engine.handle(
        11.,
        Event::HttpFailed {
            request: first.id,
            error: "native midpoint failure",
        },
    );
    let a = actions(&mut engine, 11.);
    let timer = a
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if (750.0..=1000.).contains(&(*at_ms - 11.)) => {
                Some(*timer)
            }
            _ => None,
        })
        .expect("failed offset retains its native backoff");
    let next = midpoint(a);
    assert_eq!(body_ids(&next), vec!["101", "102"]);
    assert!(engine.store().tickers().is_empty());
    answer(&mut engine, &next, 12., "{}");
    assert!(!engine.store().loaded().tickers);
    engine.handle(1011., Event::TimerFired { timer });
    let retry = midpoint(actions(&mut engine, 1011.));
    assert_eq!(body_ids(&retry), body_ids(&first));
    assert_ne!(retry.id, first.id);
    answer(&mut engine, &retry, 1012., r#"{"1":"0.4"}"#);
    assert!(engine.store().loaded().tickers);
    assert_eq!(engine.store().tickers().len(), 1);
}
