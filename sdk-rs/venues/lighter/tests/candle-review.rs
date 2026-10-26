//! Adversarial source ordering: a recovered stream overtakes its fallback read.
mod support;
use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{Action, Command, Engine, Event};
use safe_cex_venue_lighter::Lighter;
use support::Options;
use support::{META, NOW, reply_at};

#[test]
fn stale_fallback_candle_cannot_overwrite_same_bar_from_recovered_stream() {
    let mut e = support::engine(Options {
        public_data_only: true,
        ..Options::default()
    });
    e.start(NOW);
    let markets = safe_cex_engine::test_support::prepared_actions(&mut e, NOW)
        .into_iter()
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
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    e.drain_actions().for_each(drop);
    e.take_frame(NOW);

    e.handle(NOW + 120_000.0, Event::TimerFired { timer: poll });
    let fallback = prepared_actions(&mut e, NOW + 120_000.0)
        .into_iter()
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

#[test]
fn prior_subscription_candle_response_or_failure_cannot_touch_new_fallback() {
    for failed in [false, true] {
        let mut e = support::engine(Options {
            public_data_only: true,
            ..Options::default()
        });
        e.start(NOW);
        let markets = safe_cex_engine::test_support::prepared_actions(&mut e, NOW)
            .into_iter()
            .find_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .unwrap();
        reply_at(&mut e, NOW, &markets, 200, META);
        let poll = e
            .drain_actions()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if at_ms == NOW + 2_000.0 => Some(timer),
                _ => None,
            })
            .unwrap();
        let subscribe = |e: &mut Engine<Lighter>, now| {
            e.handle(
                now,
                Event::Command(Command::SubscribeCandles {
                    symbol: "ETH".into(),
                    interval: safe_cex_engine::model::Interval::M1,
                }),
            )
        };
        subscribe(&mut e, NOW);
        e.drain_actions().for_each(drop);
        e.handle(NOW + 120_000.0, Event::TimerFired { timer: poll });
        let actions = prepared_actions(&mut e, NOW + 120_000.0);
        let old = actions
            .iter()
            .find_map(|a| match a {
                Action::Http(r) if r.url.contains("/candles?") => Some(r.clone()),
                _ => None,
            })
            .unwrap();
        let next_poll = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == NOW + 122_000.0 => Some(*timer),
                _ => None,
            })
            .unwrap();
        e.handle(
            NOW + 120_001.0,
            Event::Command(Command::UnsubscribeCandles {
                symbol: "ETH".into(),
                interval: safe_cex_engine::model::Interval::M1,
            }),
        );
        subscribe(&mut e, NOW + 120_002.0);
        e.drain_actions().for_each(drop);
        e.take_frame(NOW + 120_002.0);
        e.handle(NOW + 240_003.0, Event::TimerFired { timer: next_poll });
        let actions = prepared_actions(&mut e, NOW + 240_003.0);
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Http(r) if r.url.contains("/candles?")))
        );
        let next_poll = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == NOW + 242_003.0 => Some(*timer),
                _ => None,
            })
            .unwrap();
        e.take_frame(NOW + 240_003.0);
        if failed {
            e.handle(
                NOW + 240_004.0,
                Event::HttpFailed {
                    request: old.id,
                    error: "prior subscription failed",
                },
            );
        } else {
            let body = format!(
                r#"{{"code":200,"c":[{{"t":{},"o":"90","h":"91","l":"89","c":"90","v":"1"}}]}}"#,
                NOW as u64 + 240_000
            );
            reply_at(&mut e, NOW + 240_004.0, &old, 200, &body);
            assert!(
                e.take_frame(NOW + 240_004.0).is_none(),
                "prior subscription emitted a candle"
            );
        }
        e.handle(NOW + 250_000.0, Event::TimerFired { timer: next_poll });
        assert!(
            !prepared_actions(&mut e, NOW + 250_000.0)
                .into_iter()
                .any(|a| matches!(a, Action::Http(r) if r.url.contains("/candles?"))),
            "prior response released the current fallback request"
        );
    }
}
