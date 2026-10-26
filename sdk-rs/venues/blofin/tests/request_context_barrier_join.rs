//! Original controls for the hedged leverage barrier join
//! (`docs/plans/RUST_BLOFIN_WRITE_FUNNEL.md`, W2): both legs prepare and sign
//! before either is released, then answer independently into one terminal.
#![allow(dead_code)]
mod support;
use safe_cex_engine::request_context::{Claim, GroupClaim, HttpAction, Prepared};
use safe_cex_engine::{Action, Command, Event, HttpRequest, Notice, Outcome, VenueError};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use support::*;

const OK: &str = r#"{"code":"0","data":{"leverage":"30"}}"#;
const REFUSED: &str = r#"{"code":"110006","msg":"Leverage cannot be adjusted"}"#;
/// Signing entropy is unlimited until a test arms a draw budget.
const UNLIMITED: usize = usize::MAX;

/// Releases pending managed work at `h.now`, including one barrier group.
fn release(h: &mut Harness) {
    let now = h.now;
    h.e.prepare_dispatch(now);
    let mut queue: std::collections::VecDeque<_> = h.e.drain_actions().collect();
    let mut actions = Vec::new();
    while let Some(action) = queue.pop_front() {
        let Action::ManagedHttp(managed) = action else {
            actions.push(action);
            continue;
        };
        match *managed {
            HttpAction::Prepare {
                preparation,
                request,
                ..
            } => {
                if h.e.http_preparation_pending(preparation) {
                    h.e.http_prepared(
                        now,
                        preparation,
                        Prepared {
                            id: request.id.0,
                            expires_at: None,
                        },
                    );
                }
            }
            HttpAction::Bind {
                permit, request, ..
            } => {
                if let Ok(Claim::Released(_)) = h.e.claim_http(now, permit) {
                    actions.push(Action::Http(request));
                }
            }
            HttpAction::BindGroup { permit, members } => {
                // GroupPacket wipes on drop; take each packet out first.
                let packets: Vec<HttpRequest> = members
                    .into_iter()
                    .filter_map(|mut member| member.request.take())
                    .collect();
                if let Ok(GroupClaim::Released(_)) = h.e.claim_http_group(now, permit) {
                    actions.extend(packets.into_iter().map(Action::Http));
                }
            }
            HttpAction::Cancel { .. } => {}
        }
        for action in h.e.drain_actions().collect::<Vec<_>>().into_iter().rev() {
            queue.push_front(action);
        }
    }
    h.collect_actions(actions);
}

fn event(h: &mut Harness, event: Event<'_>) {
    let now = h.now.max(1.0);
    h.now = now;
    h.e.handle(now, event);
    release(h);
}

fn answer(h: &mut Harness, request: &HttpRequest, status: u16, body: &str) {
    event(
        h,
        Event::HttpResponse {
            request: request.id,
            response: safe_cex_engine::Response {
                status,
                headers: "",
                body,
            },
        },
    );
}

fn reply(h: &mut Harness, path: &str, body: &str) {
    let request = h.take(path);
    answer(h, &request, 200, body);
}

fn legs(h: &Harness) -> (Option<HttpRequest>, Option<HttpRequest>) {
    let leg = |side: &str| {
        h.pending
            .iter()
            .find(|r| r.url.ends_with("set-leverage") && r.body.as_deref().unwrap().contains(side))
            .cloned()
    };
    (leg("\"long\""), leg("\"short\""))
}

fn terminals(h: &Harness, id: u32) -> Vec<Result<String, VenueError>> {
    let mut failures = h.failures.iter();
    h.actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandDone { command, result }) => {
                Some((*command, Ok(result.clone())))
            }
            Action::Notice(Notice::CommandFailed { command, .. }) => {
                Some((*command, Err(failures.next().expect("failure").clone())))
            }
            _ => None,
        })
        .filter(|(command, _)| *command == id)
        .map(|(_, terminal)| terminal)
        .collect()
}

fn failure(h: &Harness, id: u32) -> VenueError {
    let terminals = terminals(h, id);
    assert_eq!(terminals.len(), 1, "exactly one terminal: {terminals:?}");
    terminals[0].clone().expect_err("a failure terminal")
}

fn leverage(h: &Harness) -> f64 {
    h.e.store().setting("BTCUSDT").unwrap().leverage
}

fn count(h: &Harness, path: &str) -> usize {
    h.pending.iter().filter(|r| r.url.contains(path)).count()
}

/// A flat cross/net account whose signing entropy follows `draws`
/// (UNLIMITED until armed).
fn flat(draws: Arc<AtomicUsize>) -> Harness {
    flat_with(Box::new(move |bytes| {
        let left = draws.load(Ordering::SeqCst);
        if left == 0 {
            return Err("signing entropy refused".into());
        }
        if left != UNLIMITED {
            draws.store(left - 1, Ordering::SeqCst);
        }
        bytes.fill(7);
        Ok(())
    }))
}

fn flat_with(fill: safe_cex_engine::entropy::Fill) -> Harness {
    let mut h = Harness::with_signing_entropy(fill);
    h.modes();
    h.reply("account/positions", r#"{"code":"0","data":[]}"#);
    h.reply(
        "balances",
        r#"{"code":"0","data":[{"available":"1000","frozen":"0"}]}"#,
    );
    for path in [
        "orders-pending",
        "orders-tpsl-pending",
        "orders-algo-pending",
    ] {
        h.reply(path, r#"{"code":"0","data":[]}"#);
    }
    h
}

/// `flat`, switched to isolated margin and hedge mode.
fn hedged(draws: Arc<AtomicUsize>) -> Harness {
    hedge(flat(draws))
}

fn hedge(mut h: Harness) -> Harness {
    event(
        &mut h,
        Event::Command(Command::SetMarginType {
            command: 80,
            symbol: "BTCUSDT".into(),
            isolated: true,
        }),
    );
    reply(
        &mut h,
        "set-margin-mode",
        r#"{"code":"0","data":{"marginMode":"isolated"}}"#,
    );
    reply(&mut h, "account/positions", r#"{"code":"0","data":[]}"#);
    if count(&h, "balances") == 1 {
        reply(
            &mut h,
            "balances",
            r#"{"code":"0","data":[{"available":"1000","frozen":"0"}]}"#,
        );
    }
    event(
        &mut h,
        Event::Command(Command::SetPositionMode {
            command: 81,
            hedged: true,
            symbol: None,
        }),
    );
    reply(
        &mut h,
        "set-position-mode",
        r#"{"code":"0","data":{"positionMode":"long_short_mode"}}"#,
    );
    h
}

fn set_leverage(h: &mut Harness) {
    event(
        h,
        Event::Command(Command::SetLeverage {
            command: 7,
            symbol: "BTCUSDT".into(),
            leverage: 30.0,
            side: None,
        }),
    );
}

#[test]
fn both_legs_are_released_before_either_answers() {
    let mut h = hedged(Arc::new(AtomicUsize::new(UNLIMITED)));
    set_leverage(&mut h);
    let (long, short) = legs(&h);
    assert!(long.is_some() && short.is_some(), "{:?}", h.pending);
}

#[test]
fn a_short_signing_refusal_after_long_prepared_sends_nothing() {
    let draws = Arc::new(AtomicUsize::new(UNLIMITED));
    let mut h = hedged(draws.clone());
    // Long's signature draws; short's is refused.
    draws.store(1, Ordering::SeqCst);
    set_leverage(&mut h);
    assert_eq!(count(&h, "set-leverage"), 0, "zero HTTP from the step");
    if let (Some(long), _) = legs(&h) {
        answer(&mut h, &long, 200, OK);
    }
    assert_eq!(failure(&h, 7).outcome, Some(Outcome::NotSent));
    assert_ne!(leverage(&h), 30.0);
}

#[test]
fn both_accepted_commit_once() {
    let mut h = hedged(Arc::new(AtomicUsize::new(UNLIMITED)));
    set_leverage(&mut h);
    let (long, short) = legs(&h);
    let (long, short) = (long.expect("long"), short.expect("short"));
    answer(&mut h, &short, 200, OK);
    assert_ne!(leverage(&h), 30.0, "no commit before both settle");
    assert!(terminals(&h, 7).is_empty());
    answer(&mut h, &long, 200, OK);
    assert_eq!(terminals(&h, 7).len(), 1);
    assert!(terminals(&h, 7)[0].is_ok());
    assert_eq!(leverage(&h), 30.0);
}

/// hcmq3f P2: under `independent_calls` the join waits for both legs in either
/// order; the first answer leaves no terminal, no refresh and a current sibling.
fn one_503_and_one_accepted(first_503: bool) {
    let mut h = hedged(Arc::new(AtomicUsize::new(UNLIMITED)));
    set_leverage(&mut h);
    let (long, short) = legs(&h);
    let (long, short) = (long.expect("long"), short.expect("short"));
    let positions = count(&h, "account/positions");
    let (first, second) = if first_503 {
        ((&long, 503, "Service Unavailable"), (&short, 200, OK))
    } else {
        ((&short, 200, OK), (&long, 503, "Service Unavailable"))
    };
    answer(&mut h, first.0, first.1, first.2);
    assert!(
        terminals(&h, 7).is_empty(),
        "no terminal before both settle"
    );
    assert_eq!(
        count(&h, "account/positions"),
        positions,
        "no early refresh"
    );
    answer(&mut h, second.0, second.1, second.2);
    assert_eq!(failure(&h, 7).outcome, Some(Outcome::Unknown));
    assert_ne!(leverage(&h), 30.0);
    assert_eq!(
        count(&h, "account/positions"),
        positions + 1,
        "one join refresh"
    );
}

#[test]
fn a_503_then_an_accepted_waits_for_both_and_is_one_unknown() {
    one_503_and_one_accepted(true);
}

#[test]
fn an_accepted_then_a_503_waits_for_both_and_is_one_unknown() {
    one_503_and_one_accepted(false);
}

#[test]
fn one_refusal_beside_one_accepted_keeps_its_effect_and_refreshes() {
    let mut h = hedged(Arc::new(AtomicUsize::new(UNLIMITED)));
    set_leverage(&mut h);
    let (long, short) = legs(&h);
    let (long, short) = (long.expect("long"), short.expect("short"));
    let positions = count(&h, "account/positions");
    answer(&mut h, &long, 200, OK);
    answer(&mut h, &short, 200, REFUSED);
    assert_eq!(failure(&h, 7).outcome, None);
    assert_ne!(leverage(&h), 30.0);
    assert_eq!(
        count(&h, "account/positions"),
        positions + 1,
        "native join refresh"
    );
}

#[test]
fn both_refused_is_rejected() {
    let mut h = hedged(Arc::new(AtomicUsize::new(UNLIMITED)));
    set_leverage(&mut h);
    let (long, short) = legs(&h);
    let (long, short) = (long.expect("long"), short.expect("short"));
    answer(&mut h, &long, 200, REFUSED);
    answer(&mut h, &short, 200, REFUSED);
    assert_eq!(failure(&h, 7).outcome, Some(Outcome::Rejected));
    assert_ne!(leverage(&h), 30.0);
}

#[test]
fn a_join_queued_behind_a_placement_admits_both_legs_at_its_wake() {
    let mut h = hedged(Arc::new(AtomicUsize::new(UNLIMITED)));
    let orders = serde_json::from_value(serde_json::json!([
        {"symbol": "BTCUSDT", "type": "limit", "side": "buy", "amount": 1, "price": 60000}
    ]))
    .unwrap();
    event(
        &mut h,
        Event::Command(Command::PlaceOrders { command: 6, orders }),
    );
    set_leverage(&mut h);
    assert_eq!(count(&h, "set-leverage"), 0, "parked behind the placement");
    reply(
        &mut h,
        "batch-orders",
        r#"{"code":"0","data":[{"orderId":"1","code":"0","msg":"success"}]}"#,
    );
    let (long, short) = legs(&h);
    assert!(long.is_some() && short.is_some(), "{:?}", h.pending);
}

const REFUSED_SHORT: &str = r#"{"code":"110019","msg":"Margin mode cannot be adjusted"}"#;

fn refusal_beside_accepted(short_first: bool) {
    let mut h = hedged(Arc::new(AtomicUsize::new(UNLIMITED)));
    set_leverage(&mut h);
    let (long, short) = legs(&h);
    let (long, short) = (long.expect("long"), short.expect("short"));
    if short_first {
        answer(&mut h, &short, 200, REFUSED);
        answer(&mut h, &long, 200, OK);
    } else {
        answer(&mut h, &long, 200, OK);
        answer(&mut h, &short, 200, REFUSED);
    }
    let error = failure(&h, 7);
    assert_eq!(error.outcome, None, "the journal's projection");
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::Rejected));
    assert!(
        error.message.contains("Leverage cannot be adjusted"),
        "{}",
        error.message
    );
    assert_ne!(leverage(&h), 30.0);
}

#[test]
fn a_short_refusal_before_long_accepted_reports_the_refusal() {
    refusal_beside_accepted(true);
}

#[test]
fn a_short_refusal_after_long_accepted_reports_the_refusal() {
    refusal_beside_accepted(false);
}

/// Ruling (a): the terminal text is the earliest logical failing leg's (long),
/// in either callback order; kind, outcome and evidence are the journal's.
#[test]
fn both_refused_report_the_long_legs_refusal_in_either_order() {
    for long_first in [true, false] {
        let mut h = hedged(Arc::new(AtomicUsize::new(UNLIMITED)));
        set_leverage(&mut h);
        let (long, short) = legs(&h);
        let (long, short) = (long.expect("long"), short.expect("short"));
        let positions = count(&h, "account/positions");
        if long_first {
            answer(&mut h, &long, 200, REFUSED);
            answer(&mut h, &short, 200, REFUSED_SHORT);
        } else {
            answer(&mut h, &short, 200, REFUSED_SHORT);
            answer(&mut h, &long, 200, REFUSED);
        }
        let error = failure(&h, 7);
        assert_eq!(
            error.message, "BloFin setting rejected: Leverage cannot be adjusted",
            "{long_first}"
        );
        assert_eq!(error.outcome, Some(Outcome::Rejected), "{long_first}");
        assert_eq!(
            error.kind,
            Some(safe_cex_engine::ErrorKind::Rejected),
            "{long_first}"
        );
        assert_eq!(error.venue_code.as_deref(), Some("110006"), "{long_first}");
        assert_ne!(leverage(&h), 30.0);
        assert_eq!(
            count(&h, "account/positions"),
            positions + 1,
            "{long_first}"
        );
    }
}

#[test]
fn a_short_transport_loss_after_long_accepted_is_settled_unknown() {
    let mut h = hedged(Arc::new(AtomicUsize::new(UNLIMITED)));
    set_leverage(&mut h);
    let (long, short) = legs(&h);
    let (long, short) = (long.expect("long"), short.expect("short"));
    let positions = count(&h, "account/positions");
    answer(&mut h, &long, 200, OK);
    event(
        &mut h,
        Event::HttpFailed {
            request: short.id,
            error: "connection reset",
        },
    );
    assert_eq!(failure(&h, 7).outcome, Some(Outcome::Unknown));
    assert_ne!(leverage(&h), 30.0);
    assert_eq!(
        count(&h, "account/positions"),
        positions + 1,
        "native join refresh"
    );
}

#[test]
fn unanswered_legs_end_at_the_call_bound_with_one_timeout() {
    let mut h = hedged(Arc::new(AtomicUsize::new(UNLIMITED)));
    set_leverage(&mut h);
    assert!(matches!(legs(&h), (Some(_), Some(_))));
    let deadline = h.now + 20_000.0;
    let timers: Vec<_> = h
        .actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms <= deadline && *timer != h.tick => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .collect();
    for (timer, at) in timers {
        h.now = h.now.max(at);
        event(&mut h, Event::TimerFired { timer });
    }
    let error = failure(&h, 7);
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::Timeout));
    assert_ne!(leverage(&h), 30.0);
}

#[test]
fn a_leverage_that_becomes_hedged_while_queued_takes_the_barrier() {
    let mut h = flat(Arc::new(AtomicUsize::new(UNLIMITED)));
    for command in [
        Command::SetMarginType {
            command: 5,
            symbol: "BTCUSDT".into(),
            isolated: true,
        },
        Command::SetPositionMode {
            command: 6,
            hedged: true,
            symbol: None,
        },
    ] {
        event(&mut h, Event::Command(command));
    }
    // Planned at entry this is one net leg; at its wake it is the hedged join.
    set_leverage(&mut h);
    reply(
        &mut h,
        "set-margin-mode",
        r#"{"code":"0","data":{"marginMode":"isolated"}}"#,
    );
    reply(
        &mut h,
        "set-position-mode",
        r#"{"code":"0","data":{"positionMode":"long_short_mode"}}"#,
    );
    let (long, short) = legs(&h);
    assert!(long.is_some() && short.is_some(), "{:?}", h.pending);
}

#[test]
fn a_duplicate_answer_for_a_settled_leg_changes_nothing() {
    let mut h = hedged(Arc::new(AtomicUsize::new(UNLIMITED)));
    set_leverage(&mut h);
    let (long, short) = legs(&h);
    let (long, short) = (long.expect("long"), short.expect("short"));
    answer(&mut h, &long, 503, "Service Unavailable");
    answer(&mut h, &long, 200, OK);
    event(
        &mut h,
        Event::HttpFailed {
            request: long.id,
            error: "late duplicate",
        },
    );
    assert!(
        terminals(&h, 7).is_empty(),
        "long settled once; short still current"
    );
    answer(&mut h, &short, 200, OK);
    assert_eq!(failure(&h, 7).outcome, Some(Outcome::Unknown));
    answer(&mut h, &short, 200, OK);
    assert_eq!(terminals(&h, 7).len(), 1);
    assert_ne!(leverage(&h), 30.0);
}

#[test]
fn root_expiry_retires_the_join_once_and_the_fifo_proceeds() {
    let mut h = hedged(Arc::new(AtomicUsize::new(UNLIMITED)));
    set_leverage(&mut h);
    event(
        &mut h,
        Event::Command(Command::SetMarginType {
            command: 8,
            symbol: "BTCUSDT".into(),
            isolated: false,
        }),
    );
    let deadline = h.now + 20_000.0;
    let timers: Vec<_> = h
        .actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms <= deadline && *timer != h.tick => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .collect();
    for (timer, at) in timers {
        h.now = h.now.max(at);
        event(&mut h, Event::TimerFired { timer });
    }
    assert_eq!(terminals(&h, 7).len(), 1);
    assert_eq!(
        count(&h, "set-margin-mode"),
        1,
        "the queued command proceeds"
    );
}

// ---- Held-preparation controls (hcmq3f, 4yjlw0 4) ----

/// Draws after arming fill with 1, 2, ... so a leg's nonce names its draw:
/// `URL_ALPHABET[1]` is 's' and `[2]` is 'e'.
fn counted() -> (Harness, Arc<AtomicUsize>) {
    let next = Arc::new(AtomicUsize::new(0));
    let draws = next.clone();
    let h = hedge(flat_with(Box::new(move |bytes| {
        let value = draws.load(Ordering::SeqCst);
        if value == 0 {
            bytes.fill(7);
        } else {
            bytes.fill(value as u8);
            draws.store(value + 1, Ordering::SeqCst);
        }
        Ok(())
    })));
    (h, next)
}

/// Every action the engine emits now, without completing any preparation.
fn pump(h: &mut Harness) -> Vec<Action> {
    let now = h.now;
    h.e.prepare_dispatch(now);
    let mut all = Vec::new();
    loop {
        let batch: Vec<_> = h.e.drain_actions().collect();
        if batch.is_empty() {
            return all;
        }
        all.extend(batch);
    }
}

fn is_leverage(request: &HttpRequest) -> bool {
    request.url.ends_with("set-leverage")
}

fn prepares(
    actions: &[Action],
) -> Vec<(safe_cex_engine::request_context::Preparation, HttpRequest)> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::ManagedHttp(m) => match m.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } if is_leverage(request) => Some((*preparation, request.clone())),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

fn escaped(actions: &[Action]) -> bool {
    actions.iter().any(|a| match a {
        Action::Http(request) => is_leverage(request),
        Action::ManagedHttp(m) => {
            matches!(m.as_ref(), HttpAction::Bind { request, .. } if is_leverage(request))
        }
        _ => false,
    })
}

fn complete(h: &mut Harness, preparation: safe_cex_engine::request_context::Preparation, id: u64) {
    let now = h.now;
    h.e.http_prepared(
        now,
        preparation,
        Prepared {
            id,
            expires_at: None,
        },
    );
}

#[test]
fn both_preparations_are_held_and_released_as_one_group_in_declaration_order() {
    let (mut h, next) = counted();
    next.store(1, Ordering::SeqCst);
    h.now = h.now.max(1.0);
    let now = h.now;
    h.e.handle(
        now,
        Event::Command(Command::SetLeverage {
            command: 7,
            symbol: "BTCUSDT".into(),
            leverage: 30.0,
            side: None,
        }),
    );
    let actions = pump(&mut h);
    let held = prepares(&actions);
    assert_eq!(held.len(), 2, "two genuine Prepares");
    assert!(!escaped(&actions), "no early HTTP or individual Bind");
    let side = |r: &HttpRequest, s: &str| r.body.as_deref().unwrap().contains(s);
    let long = held
        .iter()
        .find(|(_, r)| side(r, "\"long\""))
        .unwrap()
        .clone();
    let short = held
        .iter()
        .find(|(_, r)| side(r, "\"short\""))
        .unwrap()
        .clone();
    // Routes complete in reverse; nothing is released until both are ready.
    complete(&mut h, short.0, short.1.id.0);
    let actions = pump(&mut h);
    assert!(!escaped(&actions));
    assert!(!actions.iter().any(|a| matches!(a, Action::ManagedHttp(m) if matches!(m.as_ref(), HttpAction::BindGroup { .. }))));
    complete(&mut h, long.0, long.1.id.0);
    let actions = pump(&mut h);
    assert!(!escaped(&actions), "still no individual release");
    let mut groups: Vec<_> = actions
        .into_iter()
        .filter_map(|a| match a {
            Action::ManagedHttp(m) => match *m {
                HttpAction::BindGroup { permit, members } => Some((permit, members)),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(groups.len(), 1, "exactly one BindGroup");
    let (permit, members) = groups.remove(0);
    let packets: Vec<HttpRequest> = members
        .into_iter()
        .filter_map(|mut member| member.request.take())
        .collect();
    assert_eq!(packets.len(), 2);
    assert!(side(&packets[0], "\"long\"") && side(&packets[1], "\"short\""));
    // Signing entropy was drawn in declaration order despite reverse routes.
    let nonce = |r: &HttpRequest| {
        r.headers
            .iter()
            .find(|(name, _)| *name == "ACCESS-NONCE")
            .map(|(_, value)| value.clone())
            .unwrap()
    };
    assert_eq!(nonce(&packets[0]), "s".repeat(21), "long signs first");
    assert_eq!(nonce(&packets[1]), "e".repeat(21));
    let now = h.now;
    assert!(matches!(
        h.e.claim_http_group(now, permit),
        Ok(GroupClaim::Released(_))
    ));
}

#[test]
fn a_deadline_before_short_is_prepared_sends_nothing_and_refreshes_nothing() {
    let (mut h, _) = counted();
    h.now = h.now.max(1.0);
    let now = h.now;
    h.e.handle(
        now,
        Event::Command(Command::SetLeverage {
            command: 7,
            symbol: "BTCUSDT".into(),
            leverage: 30.0,
            side: None,
        }),
    );
    let actions = pump(&mut h);
    let held = prepares(&actions);
    let long = held
        .iter()
        .find(|(_, r)| r.body.as_deref().unwrap().contains("\"long\""))
        .unwrap()
        .clone();
    complete(&mut h, long.0, long.1.id.0);
    let mut actions = actions;
    actions.extend(pump(&mut h));
    assert!(!escaped(&actions));
    let positions = count(&h, "account/positions");
    // Fire every armed timer through the call bound; short is never prepared.
    let deadline = h.now + 20_001.0;
    let timers: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms <= deadline => Some((*timer, *at_ms)),
            _ => None,
        })
        .collect();
    for (timer, at) in timers {
        h.now = h.now.max(at);
        let now = h.now;
        h.e.handle(now, Event::TimerFired { timer });
        let more = pump(&mut h);
        assert!(!escaped(&more), "zero HTTP from the step");
        let notices: Vec<_> = more
            .into_iter()
            .filter(|a| !matches!(a, Action::ManagedHttp(_)))
            .collect();
        h.collect_actions(notices);
    }
    let error = failure(&h, 7);
    assert!(matches!(error.outcome, Some(Outcome::NotSent)), "{error:?}");
    assert_eq!(count(&h, "account/positions"), positions, "no join refresh");
    assert_ne!(leverage(&h), 30.0);
}
