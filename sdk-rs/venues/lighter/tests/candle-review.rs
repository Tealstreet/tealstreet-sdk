//! Adversarial source ordering: a recovered stream overtakes its fallback read.
mod support;
use safe_cex_engine::{Action, Command, Engine, Event};
use safe_cex_venue_lighter::{Lighter, Options};
use support::{META, NOW, reply_at};

#[test]
fn stale_fallback_candle_cannot_overwrite_same_bar_from_recovered_stream() {
    let mut e = Engine::new(Lighter::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    e.start(NOW);
    let markets = e
        .drain_actions()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    reply_at(&mut e, NOW, &markets, 200, META);
    let actions = e.drain_actions().collect::<Vec<_>>();
    let socket = actions
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(*socket),
            _ => None,
        })
        .unwrap();
    let poll = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 2_000.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(NOW, Event::WsOpened { socket });
    e.drain_actions().for_each(drop);
    e.handle(
        NOW,
        Event::Command(Command::SubscribeCandles {
            symbol: "ETH".into(),
            interval: "1m".into(),
        }),
    );
    e.drain_actions().for_each(drop);
    e.take_frame(NOW);

    e.handle(NOW + 120_000.0, Event::TimerFired { timer: poll });
    let fallback = e
        .drain_actions()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains("/candles?") => Some(r),
            _ => None,
        })
        .expect("quiet stream starts candle fallback");
    let candle = format!(
        r#"{{"channel":"candle:1:1m","type":"update/candle","candles":[{{"t":{},"o":"100","h":"103","l":"99","c":"102","v":"5"}}]}}"#,
        NOW as u64 + 120_000
    );
    e.handle(
        NOW + 120_010.0,
        Event::WsMessage {
            socket,
            text: &candle,
        },
    );
    assert!(
        e.take_frame(NOW + 120_010.0).is_some(),
        "recovered stream emitted its current candle"
    );

    let old = format!(
        r#"{{"code":200,"c":[{{"t":{},"o":"100","h":"101","l":"99","c":"100","v":"1"}}]}}"#,
        NOW as u64 + 120_000
    );
    reply_at(&mut e, NOW + 120_020.0, &fallback, 200, &old);
    assert!(
        e.take_frame(NOW + 120_020.0).is_none(),
        "older HTTP data must not emit a rollback of close or volume"
    );
    e.handle(
        NOW + 120_030.0,
        Event::WsMessage {
            socket,
            text: &candle,
        },
    );
    assert!(
        e.take_frame(NOW + 120_030.0).is_none(),
        "the current candle remained unchanged after the stale response"
    );
}
