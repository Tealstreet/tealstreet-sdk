//! Original RED: account reads classify auth failures by the envelope code,
//! never by a substring of the raw body.
#[allow(dead_code)]
mod support;
use safe_cex_engine::{Action, Event, TimerId};
use support::*;

/// Fires the account poll and returns the next poll timer and its delay.
fn fire(h: &mut Harness, timer: TimerId, now: f64) -> (TimerId, f64) {
    h.actions.clear();
    h.e.handle(now, Event::TimerFired { timer });
    h.collect();
    h.actions
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms - now >= 7_500.0 => {
                Some((*timer, *at_ms - now))
            }
            _ => None,
        })
        .expect("next tick")
}

/// The poll delay after one failed orders page answered with `body`.
fn cadence_after(body: &str) -> f64 {
    let mut h = Harness::new();
    h.seed();
    let tick = h.tick;
    let (next, delay) = fire(&mut h, tick, 10_000.0);
    assert_eq!(delay, 10_000.0);
    h.reply("orders-pending", body);
    fire(&mut h, next, 20_000.0).1
}

#[test]
#[ignore = "original RED: account auth failures match 152401/152406 anywhere in the body; enabled by the S3-S6 cutover"]
fn non_auth_code_mentioning_an_auth_code_keeps_the_healthy_cadence() {
    let delay =
        cadence_after(r#"{"code":"102002","msg":"Order 1524010 does not exist","data":[]}"#);
    assert_eq!(delay, 10_000.0, "a non-auth refusal backed off as auth");
}

/// Passing control: the same refusal without the digits keeps the cadence.
#[test]
fn non_auth_code_keeps_the_healthy_cadence() {
    let delay = cadence_after(r#"{"code":"102002","msg":"Order does not exist","data":[]}"#);
    assert_eq!(delay, 10_000.0);
}
