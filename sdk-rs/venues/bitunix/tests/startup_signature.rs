//! One "Sign signature error" on a startup read is retried freshly signed; legacy re-signs once.
mod support;
use safe_cex_engine::{Action, Event, Notice, Response};
use support::*;

/// Starts with the first `refusals` position reads answered 10007; returns whether startup failed.
fn start(refusals: usize) -> (bool, bool) {
    let mut e = engine(Options {
        key: Some("k".into()),
        secret: Some("s".into()),
        ..Options::default()
    });
    let mut now = -1000.0;
    e.start(now);
    let mut refused = 0;
    let mut failed = false;
    let mut timers = Vec::new();
    for _ in 0..200 {
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, now);
        if actions.is_empty() {
            let Some(i) = (0..timers.len()).min_by(|a, b| {
                let (_, x): &(safe_cex_engine::TimerId, f64) = &timers[*a];
                x.total_cmp(&timers[*b].1)
            }) else {
                break;
            };
            let (timer, at) = timers.remove(i);
            if at > 20_000.0 {
                break;
            }
            now = f64::max(now, at);
            e.handle(now, Event::TimerFired { timer });
            continue;
        }
        for a in actions {
            match a {
                Action::WsConnect { socket, .. } => e.handle(now, Event::WsOpened { socket }),
                Action::SetTimer { timer, at_ms } => timers.push((timer, at_ms)),
                Action::Http(r) => {
                    let body = if r.url.ends_with("/trading_pairs") {
                        PAIRS
                    } else if r.url.ends_with("/tickers") {
                        TICKERS
                    } else if r.url.contains("get_pending_positions") && refused < refusals {
                        refused += 1;
                        r#"{"code":10007,"msg":"Sign signature error","data":null}"#
                    } else if r.url.contains("get_pending_positions") {
                        POSITIONS
                    } else if r.url.contains("/tpsl/") {
                        TPSL
                    } else if r.url.contains("get_pending_orders") {
                        ORDERS
                    } else {
                        ACCOUNT
                    };
                    e.handle(
                        now,
                        Event::HttpResponse {
                            request: r.id,
                            response: Response {
                                status: 200,
                                headers: "",
                                body,
                            },
                        },
                    );
                }
                Action::Notice(Notice::StartupFailed { .. }) => failed = true,
                _ => {}
            }
        }
    }
    (failed, e.store().loaded().positions)
}

#[test]
fn one_signature_refusal_on_a_startup_read_is_retried() {
    assert_eq!(start(1), (false, true));
}

#[test]
fn a_repeated_signature_refusal_still_ends_startup() {
    assert!(start(5).0);
}
