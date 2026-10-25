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
    let fill: safe_cex_engine::entropy::Fill = Box::new(move |bytes| {
        let left = draws.load(Ordering::SeqCst);
        if left == 0 {
            return Err("signing entropy refused".into());
        }
        if left != UNLIMITED {
            draws.store(left - 1, Ordering::SeqCst);
        }
        bytes.fill(7);
        Ok(())
    });
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
    let mut h = flat(draws);
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

/// dazzy3 P1: a released 5xx seals the Plan; the other leg's answer, in
/// either order, adds no second terminal and no commit.
fn one_503_and_one_accepted(first_503: bool) {
    let mut h = hedged(Arc::new(AtomicUsize::new(UNLIMITED)));
    set_leverage(&mut h);
    let (long, short) = legs(&h);
    let (long, short) = (long.expect("long"), short.expect("short"));
    let positions = count(&h, "account/positions");
    if first_503 {
        answer(&mut h, &long, 503, "Service Unavailable");
        // The unproven seal is the terminal; the sibling has not answered.
        assert_eq!(terminals(&h, 7).len(), 1, "terminal at the 503");
        answer(&mut h, &short, 200, OK);
    } else {
        answer(&mut h, &short, 200, OK);
        answer(&mut h, &long, 503, "Service Unavailable");
    }
    assert_eq!(failure(&h, 7).outcome, Some(Outcome::Unknown));
    assert_ne!(leverage(&h), 30.0);
    assert_eq!(
        count(&h, "account/positions"),
        positions + 1,
        "native join refresh"
    );
}

#[test]
fn a_503_then_a_late_accepted_is_one_unknown_without_commit() {
    one_503_and_one_accepted(true);
}

#[test]
fn an_accepted_then_a_503_is_one_unknown_without_commit() {
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

#[test]
fn both_refused_reports_the_last_refusal_as_native_does() {
    for long_first in [true, false] {
        let mut h = hedged(Arc::new(AtomicUsize::new(UNLIMITED)));
        set_leverage(&mut h);
        let (long, short) = legs(&h);
        let (long, short) = (long.expect("long"), short.expect("short"));
        let (first, second, last) = if long_first {
            (
                (&long, REFUSED),
                (&short, REFUSED_SHORT),
                "Margin mode cannot be adjusted",
            )
        } else {
            (
                (&short, REFUSED_SHORT),
                (&long, REFUSED),
                "Leverage cannot be adjusted",
            )
        };
        answer(&mut h, first.0, 200, first.1);
        answer(&mut h, second.0, 200, second.1);
        let error = failure(&h, 7);
        assert_eq!(error.outcome, Some(Outcome::Rejected));
        assert!(
            error.message.contains(last),
            "{long_first}: {}",
            error.message
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
