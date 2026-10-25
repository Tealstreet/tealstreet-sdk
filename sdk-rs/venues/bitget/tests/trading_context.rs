//! Caller adoption slice 3 (`docs/plans/RUST_BITGET_TRADING_CONTEXT.md`): original
//! REDs for shared Plan authority over the trading write job, plus controls
//! pinning the native behaviour the conversion must preserve.
mod support;
use safe_cex_engine::model::OrderStatus;
use safe_cex_engine::request_context::RoutingIntent;
use safe_cex_engine::request_context::{Claim, Error as ContextError, HttpAction, Prepared};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Outcome, RequestId, TimerId, VenueError,
};
use safe_cex_venue_bitget::BitgetFutures;
use std::collections::{HashMap, HashSet};
use support::*;

const DEADLINE_MS: f64 = 20_000.0;
const PLACE: &str = "/batch-place-order";

fn limit(symbol: &str) -> safe_cex_engine::model::PlaceOrderOpts {
    serde_json::from_value(serde_json::json!({
        "symbol": symbol, "type": "limit", "side": "buy", "amount": 0.002, "price": 90
    }))
    .unwrap()
}

fn place(command: u32, symbol: &str) -> Command {
    Command::PlaceOrders {
        command,
        orders: vec![limit(symbol)],
    }
}

/// Every terminal published for `command`, in order.
fn terminals(h: &Harness, command: u32) -> Vec<Result<String, VenueError>> {
    let mut failures = h.failures.iter();
    h.done
        .iter()
        .filter_map(|(c, result)| {
            let terminal = match result {
                Ok(data) => Ok(data.clone()),
                Err(_) => Err(failures.next().expect("failure per Err").clone()),
            };
            (*c == command).then_some(terminal)
        })
        .collect()
}

fn failure(h: &Harness, command: u32) -> VenueError {
    let terminals = terminals(h, command);
    assert_eq!(terminals.len(), 1, "exactly one terminal: {terminals:?}");
    terminals[0].clone().expect_err("a failure terminal")
}

fn count(h: &Harness, path: &str) -> usize {
    h.requests.iter().filter(|r| r.url.contains(path)).count()
}

fn armed(h: &Harness) -> HashSet<TimerId> {
    h.timers.iter().map(|(timer, _)| *timer).collect()
}

/// Fires, in time order, only timers armed after `before` (including timers
/// those fire), up to and including `until`.
fn fire_new(h: &mut Harness, before: &HashSet<TimerId>, until: f64) {
    for _ in 0..10_000 {
        let next = h
            .timers
            .iter()
            .copied()
            .filter(|(timer, at)| !before.contains(timer) && *at <= until)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        let Some((timer, at)) = next else {
            h.now = h.now.max(until);
            return;
        };
        h.timers.retain(|(t, _)| *t != timer);
        h.now = h.now.max(at);
        h.event(Event::TimerFired { timer });
    }
    panic!("timer chain did not settle by {until}");
}

/// `support::collect` with each managed preparation's routing intent recorded.
fn released_with_intents(
    engine: &mut Engine<BitgetFutures>,
    now: f64,
) -> (Vec<Action>, HashMap<RequestId, RoutingIntent>) {
    engine.prepare_dispatch(now);
    let mut queue: std::collections::VecDeque<_> = engine.drain_actions().collect();
    let mut intents = HashMap::new();
    let mut actions = Vec::new();
    while let Some(action) = queue.pop_front() {
        let Action::ManagedHttp(managed) = action else {
            actions.push(action);
            continue;
        };
        match *managed {
            HttpAction::Prepare {
                preparation,
                intent,
                request,
            } => {
                if engine.http_preparation_pending(preparation) {
                    intents.insert(request.id, intent);
                    engine.http_prepared(
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
                if let Ok(Claim::Released(_)) = engine.claim_http(now, permit) {
                    actions.push(Action::Http(request));
                }
            }
            HttpAction::Cancel { .. } => {}
            HttpAction::BindGroup { .. } => {
                panic!("Bitget trading declares no Mutation barriers")
            }
        }
        for action in engine.drain_actions().collect::<Vec<_>>().into_iter().rev() {
            queue.push_front(action);
        }
    }
    (actions, intents)
}

/// Delivers `event` and records which released requests were managed.
fn managed(h: &mut Harness, event: Event<'_>) -> HashMap<RequestId, RoutingIntent> {
    h.engine.handle(h.now, event);
    let (actions, intents) = released_with_intents(&mut h.engine, h.now);
    h.absorb(actions);
    intents
}

fn latest(h: &Harness, path: &str) -> HttpRequest {
    h.requests
        .iter()
        .rev()
        .find(|r| r.url.contains(path))
        .unwrap_or_else(|| panic!("no {path}: {:?}", h.requests))
        .clone()
}

fn reply(
    h: &mut Harness,
    request: RequestId,
    status: u16,
    body: &str,
) -> HashMap<RequestId, RoutingIntent> {
    managed(
        h,
        Event::HttpResponse {
            request,
            response: safe_cex_engine::Response {
                status,
                headers: "",
                body,
            },
        },
    )
}

const SIGNATURE: &str = r#"{"code":"40009","msg":"sign signature error","data":null}"#;
const UTA_REQUIRED: &str = r#"{"code":"40085","msg":"Classic API unsupported","data":null}"#;
const UTA_PLACE: &str = "/api/v3/trade/place-order";

/// Answers the startup order reads so a reconcile is not gated behind them.
fn settle_order_reads(h: &mut Harness) {
    if h.requests.iter().any(|r| r.url.contains("/orders-pending")) {
        h.orders(r#"{"code":"00000","data":{"entrustedList":[]}}"#);
    }
}

// ---- Original REDs ----

#[test]
fn classic_place_is_released_through_managed_preparation() {
    let mut h = Harness::classic();
    h.requests.clear();
    let intents = managed(&mut h, Event::Command(place(1, "BTCUSDT")));
    let request = latest(&h, PLACE);
    assert!(request.url.ends_with("/api/v2/mix/order/batch-place-order"));
    assert_eq!(
        intents.get(&request.id),
        Some(&RoutingIntent::Primary),
        "the place write must be a managed Primary preparation, not a legacy signed request"
    );
}

#[test]
fn cancel_beyond_shared_capacity_is_refused_before_canceling_status() {
    let mut h = Harness::classic();
    for command in 1_000..1_300 {
        h.command(Command::FetchCandles {
            command,
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
            limit: Some(2),
            from_ms: Some(1000.0),
            to_ms: Some(1_000_000.0),
        });
    }
    assert!(
        terminals(&h, 1_299).first().is_some_and(|t| t
            .as_ref()
            .is_err_and(|e| e.message == ContextError::Capacity.venue_error().message)),
        "the shared registry is saturated"
    );
    h.requests.retain(|r| r.url.contains("/candles"));
    h.command(Command::CancelOrders {
        command: 1,
        ids: vec!["1".into()],
    });
    assert_eq!(count(&h, "cancel"), 0, "refused before any cancel write");
    assert_eq!(
        h.engine.store().order("1").map(|o| o.status),
        Some(OrderStatus::Open),
        "no optimistic Canceling before admission"
    );
    let error = failure(&h, 1);
    assert_eq!(error.message, ContextError::Capacity.venue_error().message);
    assert_eq!(error.outcome, Some(Outcome::NotSent));
}

#[test]
fn a_signature_rejection_is_resent_once_through_managed_custody() {
    let mut h = Harness::classic();
    h.requests.clear();
    managed(&mut h, Event::Command(place(1, "BTCUSDT")));
    let first = latest(&h, PLACE);
    let intents = reply(&mut h, first.id, 200, SIGNATURE);
    assert_eq!(count(&h, PLACE), 2, "native resends once");
    let resent = latest(&h, PLACE);
    assert_eq!(
        intents.get(&resent.id),
        Some(&RoutingIntent::Primary),
        "the resend must be a fresh managed preparation from the call's DATA"
    );
}

#[test]
fn an_unanswered_write_ends_at_the_shared_deadline_as_unknown() {
    let mut h = Harness::classic();
    h.requests.clear();
    let before = armed(&h);
    let start = h.now;
    h.command(place(1, "BTCUSDT"));
    let request = h.take(PLACE);
    fire_new(&mut h, &before, start + DEADLINE_MS);
    let error = failure(&h, 1);
    assert_eq!(error.kind, Some(ErrorKind::Timeout));
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    h.reply(
        request.id,
        200,
        r#"{"code":"00000","data":{"successList":[{"orderId":"9"}],"failureList":[]}}"#,
    );
    assert_eq!(terminals(&h, 1).len(), 1, "a late reply adds no terminal");
}

// ---- Native controls ----

#[test]
fn missing_credentials_win_over_invalid_input() {
    let mut h = Harness::public();
    h.command(place(1, "NOPEUSDT"));
    assert!(h.requests.is_empty());
    let error = failure(&h, 1);
    assert_eq!(error.message, "Bitget credentials are required");
    assert_eq!(error.outcome, Some(Outcome::NotSent));
}

#[test]
fn invalid_input_refuses_immediately_while_the_mode_is_undetected() {
    let mut h = Harness::private_started();
    assert!(
        h.requests
            .iter()
            .any(|r| r.url.contains("/account/settings")),
        "account mode detection is still outstanding"
    );
    let timers = armed(&h);
    h.command(place(1, "NOPEUSDT"));
    let error = failure(&h, 1);
    assert_eq!(error.message, "Market NOPEUSDT not found");
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert_eq!(
        armed(&h),
        timers,
        "no detection wait for an invalid command"
    );
}

#[test]
fn a_second_signature_rejection_is_rejected() {
    let mut h = Harness::classic();
    h.requests.clear();
    h.command(place(1, "BTCUSDT"));
    let first = h.take(PLACE);
    h.reply(first.id, 200, SIGNATURE);
    let second = h.take(PLACE);
    h.reply(second.id, 200, SIGNATURE);
    assert_eq!(count(&h, PLACE), 0, "never a second resend");
    assert_eq!(failure(&h, 1).outcome, Some(Outcome::Rejected));
}

// Ruled 2026-10-25 (TS-visible, RUST_CUTOVER_CALLERS.md): a 408/5xx proves
// nothing before its native code is read, so 40009 and 40085 there end Unknown
// and reconcile instead of resending or replanning.
#[test]
fn a_server_error_carrying_40009_is_unknown_and_reconciles_without_resend() {
    let mut h = Harness::classic();
    settle_order_reads(&mut h);
    h.requests.clear();
    h.command(place(1, "BTCUSDT"));
    let first = h.take(PLACE);
    h.reply(first.id, 500, SIGNATURE);
    assert_eq!(count(&h, PLACE), 0, "no resend after an unproven status");
    assert_eq!(failure(&h, 1).outcome, Some(Outcome::Unknown));
    // The engine seals an unproven write itself; the venue reconciles on its next tick.
    let tick = h.now + 2_000.0;
    fire_new(&mut h, &HashSet::new(), tick);
    assert!(
        count(&h, "/orders-pending") >= 1,
        "the Unknown write reconciles"
    );
}

#[test]
fn a_server_error_carrying_40085_is_unknown_and_reconciles_without_replanning() {
    let mut h = Harness::classic();
    settle_order_reads(&mut h);
    h.requests.clear();
    h.command(place(1, "BTCUSDT"));
    let first = h.take(PLACE);
    h.reply(first.id, 500, UTA_REQUIRED);
    assert_eq!(
        count(&h, PLACE) + count(&h, UTA_PLACE),
        0,
        "no replanned write"
    );
    assert_eq!(failure(&h, 1).outcome, Some(Outcome::Unknown));
    let tick = h.now + 2_000.0;
    fire_new(&mut h, &HashSet::new(), tick);
    assert!(
        count(&h, "/orders-pending") + count(&h, "/unfilled-orders") >= 1,
        "the Unknown write reconciles"
    );
}

#[test]
fn a_proven_40085_with_no_ids_promotes_and_replans_as_uta() {
    let mut h = Harness::classic();
    h.requests.clear();
    h.command(place(1, "BTCUSDT"));
    let first = h.take(PLACE);
    h.reply(first.id, 200, UTA_REQUIRED);
    assert!(
        terminals(&h, 1).is_empty(),
        "the job waits for re-detection"
    );
    for _ in 0..4 {
        if count(&h, UTA_PLACE) > 0 {
            break;
        }
        h.respond(
            "/account/settings",
            200,
            r#"{"code":"00000","data":{"holdMode":"one_way_mode"}}"#,
        );
    }
    assert_eq!(count(&h, UTA_PLACE), 1, "replanned once as UTA");
    assert_eq!(count(&h, PLACE), 0, "never resends the Classic write");
    assert!(terminals(&h, 1).is_empty());
}

#[test]
fn a_mixed_batch_keeps_accepted_ids_and_fails() {
    let mut h = Harness::classic();
    h.requests.clear();
    h.command(Command::PlaceOrders {
        command: 1,
        orders: vec![limit("BTCUSDT"), limit("BTCUSDT")],
    });
    let request = h.take(PLACE);
    h.reply(
        request.id,
        200,
        r#"{"code":"00000","data":{"successList":[{"orderId":"a"}],"failureList":[{"errorMsg":"balance"}]}}"#,
    );
    let error = failure(&h, 1);
    assert!(
        error.message.contains(r#"["a"]"#),
        "accepted IDs survive the failure: {}",
        error.message
    );
    // Only the failureList row is refused: the accepted order took effect.
    assert_eq!(error.outcome, None);
    assert_eq!(
        error.partial.as_ref().map(|p| p.order_ids.clone()),
        Some(vec!["a".to_owned()])
    );
}

#[test]
fn ambiguous_codes_are_unknown_and_never_resent() {
    for code in ["40010", "40725", "45001"] {
        let mut h = Harness::classic();
        h.requests.clear();
        h.command(place(1, "BTCUSDT"));
        let request = h.take(PLACE);
        h.reply(
            request.id,
            200,
            &format!(r#"{{"code":"{code}","msg":"busy","data":null}}"#),
        );
        assert_eq!(count(&h, PLACE), 0, "{code}: no resend");
        assert_eq!(failure(&h, 1).outcome, Some(Outcome::Unknown), "{code}");
    }
}
