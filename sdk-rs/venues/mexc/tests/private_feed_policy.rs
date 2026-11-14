mod support;

use safe_cex_engine::signing_policy::PrivateFeedPolicy;
use safe_cex_engine::{Action, Event, SocketChannel};
use support::{MARKETS, Options, TICKERS};

#[test]
fn payload_mode_keeps_private_rest_polling_without_private_socket_or_auth_timers() {
    let mut engine = support::configured(Options {
        key: Some("replay-key".into()),
        secret: Some("replay-secret".into()),
        ..Default::default()
    });
    engine
        .set_private_feed_policy(PrivateFeedPolicy::RestOnly)
        .unwrap();
    engine.start(0.);
    let markets = support::requests(&mut engine).remove(0);
    support::response(&mut engine, &markets, MARKETS);
    let mut actions = support::prepared(&mut engine, 100.);
    let tickers = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with("/contract/ticker") => Some(r.clone()),
            _ => None,
        })
        .unwrap();
    support::response(&mut engine, &tickers, TICKERS);
    actions.extend(support::prepared(&mut engine, 100.));
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::WsConnect {
            channel: Some(SocketChannel::Private),
            ..
        }
    )));
    let mode = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with("/position_mode") => Some(r.clone()),
            _ => None,
        })
        .expect("REST account mode remains signed");
    support::response(&mut engine, &mode, r#"{"success":true,"code":0,"data":1}"#);
    let actions = support::prepared(&mut engine, 100.);
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("/account/assets")))
    );
    for action in actions {
        if let Action::Http(r) = action {
            support::response(&mut engine, &r, r#"{"success":true,"code":0,"data":[]}"#);
        }
    }
    let timers: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .collect();
    for (timer, at_ms) in timers {
        engine.handle(at_ms, Event::TimerFired { timer });
    }
    assert!(
        !support::prepared(&mut engine, 2100.)
            .iter()
            .any(|a| matches!(
                a,
                Action::WsConnect {
                    channel: Some(SocketChannel::Private),
                    ..
                }
            ))
    );
}
