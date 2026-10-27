//! Production binding with genuine installed-provider signing and captured IDs.
#![allow(dead_code)]
mod support;
use safe_cex_engine::{Action, Event, SocketId, Venue};
use safe_cex_venue_blofin::Blofin;
use support::Harness;

fn sends(actions: &[Action], socket: SocketId) -> Vec<&str> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::WsSend { socket: own, text } if *own == socket => Some(text.as_str()),
            _ => None,
        })
        .collect()
}
fn public(h: &Harness) -> SocketId {
    h.actions
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.ends_with("/public") => Some(*socket),
            _ => None,
        })
        .expect("actual public connection")
}
#[test]
fn blofin_declares_managed_sockets_for_the_production_binding() {
    assert!(<Blofin as Venue>::managed_sockets());
}
#[test]
fn duplicate_public_open_has_no_subscriptions_or_supervision_effects() {
    let mut h = Harness::new();
    let socket = public(&h);
    h.actions.clear();
    h.handle(1., Event::WsOpened { socket });
    h.collect();
    assert!(
        sends(&h.actions, socket)
            .iter()
            .any(|s| s.contains("tickers"))
    );
    assert!(sends(&h.actions, socket).contains(&"ping"));
    h.actions.clear();
    h.handle(2., Event::WsOpened { socket });
    h.collect();
    assert!(
        h.actions.is_empty(),
        "duplicate Opened must own no work: {:?}",
        h.actions
    );
}
#[test]
fn duplicate_private_open_cannot_resign_or_repeat_ping() {
    let mut h = Harness::new();
    let socket = h.private;
    h.actions.clear();
    h.handle(1., Event::WsOpened { socket });
    h.collect();
    assert_eq!(
        sends(&h.actions, socket)
            .iter()
            .filter(|s| s.contains("\"op\":\"login\""))
            .count(),
        1
    );
    assert!(sends(&h.actions, socket).contains(&"ping"));
    h.actions.clear();
    h.handle(2., Event::WsOpened { socket });
    h.collect();
    assert!(
        h.actions.is_empty(),
        "duplicate Opened cannot sign or reschedule: {:?}",
        h.actions
    );
}
#[test]
fn private_subscriptions_need_the_real_ack_and_duplicate_ack_is_inert() {
    let mut h = Harness::new();
    let socket = h.private;
    h.actions.clear();
    h.handle(1., Event::WsOpened { socket });
    h.collect();
    assert!(
        sends(&h.actions, socket)
            .iter()
            .all(|s| !s.contains("subscribe"))
    );
    h.actions.clear();
    h.handle(
        2.,
        Event::WsMessage {
            socket,
            text: "pong",
        },
    );
    h.collect();
    assert!(sends(&h.actions, socket).is_empty());
    h.handle(
        3.,
        Event::WsMessage {
            socket,
            text: r#"{"event":"login","code":"0"}"#,
        },
    );
    h.collect();
    assert_eq!(
        sends(&h.actions, socket)
            .iter()
            .filter(|s| s.contains("subscribe"))
            .count(),
        4
    );
    h.actions.clear();
    h.handle(
        4.,
        Event::WsMessage {
            socket,
            text: r#"{"event":"login","code":"0"}"#,
        },
    );
    h.collect();
    assert!(h.actions.is_empty());
}

fn event(h: &mut Harness, now: f64, value: Event<'_>) -> Vec<Action> {
    h.actions.clear();
    h.handle(now, value);
    h.collect();
    std::mem::take(&mut h.actions)
}
fn topics(actions: &[Action], socket: SocketId) -> Vec<(String, String, String)> {
    sends(actions, socket)
        .into_iter()
        .filter_map(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .flat_map(|v| {
            let op = v["op"].as_str().unwrap_or_default().to_owned();
            v["args"]
                .as_array()
                .into_iter()
                .flatten()
                .map(move |a| {
                    (
                        op.clone(),
                        a["channel"].as_str().unwrap_or_default().to_owned(),
                        a["instId"].as_str().unwrap_or_default().to_owned(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}
#[test]
fn books5_is_shared_until_the_last_book_or_candle_reference_leaves() {
    use safe_cex_engine::{Command, model::Interval};
    let mut h = Harness::new();
    let socket = public(&h);
    event(&mut h, 0., Event::WsOpened { socket });
    let add = event(
        &mut h,
        1.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    assert_eq!(
        topics(&add, socket)
            .iter()
            .filter(|(_, c, _)| c == "books5")
            .count(),
        1
    );
    assert!(
        event(
            &mut h,
            2.,
            Event::Command(Command::SubscribeOrderBook {
                symbol: "BTCUSDT".into()
            })
        )
        .is_empty()
    );
    let candle = event(
        &mut h,
        3.,
        Event::Command(Command::SubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: Interval::M1,
        }),
    );
    assert_eq!(
        topics(&candle, socket),
        vec![("subscribe".into(), "candle1m".into(), "BTC-USDT".into())]
    );
    assert!(
        event(
            &mut h,
            4.,
            Event::Command(Command::UnsubscribeOrderBook {
                symbol: "BTCUSDT".into()
            })
        )
        .is_empty()
    );
    let remove = event(
        &mut h,
        5.,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    assert_eq!(
        topics(&remove, socket),
        vec![("unsubscribe".into(), "books".into(), "BTC-USDT".into())]
    );
    let last = event(
        &mut h,
        6.,
        Event::Command(Command::UnsubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: Interval::M1,
        }),
    );
    let last = topics(&last, socket);
    assert_eq!(last.len(), 2);
    assert!(last.contains(&("unsubscribe".into(), "books5".into(), "BTC-USDT".into())));
    assert!(last.contains(&("unsubscribe".into(), "candle1m".into(), "BTC-USDT".into())));
}
#[test]
fn public_reconnect_does_not_retire_private_or_accept_old_public_ingress() {
    let mut h = Harness::new();
    let old = public(&h);
    let private = h.private;
    event(&mut h, 0., Event::WsOpened { socket: old });
    event(&mut h, 0., Event::WsOpened { socket: private });
    event(
        &mut h,
        0.,
        Event::WsMessage {
            socket: private,
            text: r#"{"event":"login","code":"0"}"#,
        },
    );
    let retired = event(
        &mut h,
        1.,
        Event::WsClosed {
            socket: old,
            code: 1006,
        },
    );
    assert!(
        retired
            .iter()
            .all(|a| !matches!(a, Action::WsClose { socket } if *socket == private))
    );
    let retry: Vec<_> = retired
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if (751. ..=1001.).contains(at_ms) => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .collect();
    assert_eq!(retry.len(), 1);
    assert_eq!(
        sends(
            &event(
                &mut h,
                2.,
                Event::WsMessage {
                    socket: private,
                    text: "ping"
                }
            ),
            private
        ),
        vec!["pong"]
    );
    let before = h.e.snapshot_frame(100.).to_vec();
    for ev in [
        Event::WsOpened { socket: old },
        Event::WsClosed {
            socket: old,
            code: 1006,
        },
        Event::WsMessage {
            socket: old,
            text: "malformed",
        },
    ] {
        assert!(event(&mut h, 3., ev).is_empty());
        assert_eq!(h.e.snapshot_frame(100.), before);
    }
    let connected = event(&mut h, retry[0].1, Event::TimerFired { timer: retry[0].0 });
    let current = connected
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.ends_with("/public") => Some(*socket),
            _ => None,
        })
        .unwrap();
    assert_ne!(current, old);
    let opened = event(&mut h, retry[0].1, Event::WsOpened { socket: current });
    assert_eq!(topics(&opened, current).len(), 2);
    assert!(sends(&opened, current).contains(&"ping"));
    assert!(event(&mut h, retry[0].1, Event::TimerFired { timer: retry[0].0 }).is_empty());
}
#[test]
fn nonfinite_public_and_private_ingress_leave_full_store_and_actions_unchanged() {
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut h = Harness::new();
        h.seed();
        let public = public(&h);
        let private = h.private;
        event(&mut h, 0., Event::WsOpened { socket: public });
        event(&mut h, 0., Event::WsOpened { socket: private });
        event(
            &mut h,
            0.,
            Event::WsMessage {
                socket: private,
                text: r#"{"event":"login","code":"0"}"#,
            },
        );
        let before = h.e.snapshot_frame(100.).to_vec();
        for socket in [public, private] {
            for ev in [
                Event::WsOpened { socket },
                Event::WsClosed { socket, code: 1006 },
                Event::WsMessage {
                    socket,
                    text: "invalid",
                },
            ] {
                h.e.handle(invalid, ev);
                let actions: Vec<_> = h.e.drain_actions().collect();
                assert!(actions.is_empty(), "invalid time owns no work: {actions:?}");
                assert_eq!(h.e.snapshot_frame(100.), before);
            }
            assert_eq!(
                sends(
                    &event(
                        &mut h,
                        1.,
                        Event::WsMessage {
                            socket,
                            text: "ping"
                        }
                    ),
                    socket
                ),
                vec!["pong"]
            );
        }
    }
}
