use safe_cex_engine::{Action, Engine, Event, SocketId, TimerId, Venue};

fn scheduled(actions: &[Action], now: f64, low: f64, high: f64) -> (TimerId, f64) {
    let timers: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms - now >= low && *at_ms - now <= high => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .collect();
    assert_eq!(timers.len(), 1, "one bounded reconnect timer");
    timers[0]
}
fn connected(actions: &[Action]) -> SocketId {
    actions
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(*socket),
            _ => None,
        })
        .expect("reconnect opens a fresh socket")
}

// The old fixed-delay implementation fails the second-delay assertion. This also
// covers a handshake that never completes, late frames and duplicate host closes.
pub fn flaps<V: Venue>(mut e: Engine<V>, first: SocketId, mut now: f64) {
    e.drain_actions().for_each(drop);
    e.handle(
        now,
        Event::WsClosed {
            socket: first,
            code: 1006,
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    let (retry, at) = scheduled(&actions, now, 750., 1000.);
    e.handle(
        now,
        Event::WsClosed {
            socket: first,
            code: 1006,
        },
    );
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a, Action::SetTimer { .. }))
    );
    now = at;
    e.handle(now, Event::TimerFired { timer: retry });
    let actions: Vec<_> = e.drain_actions().collect();
    let socket = connected(&actions);
    assert_ne!(socket, first);
    let (watch, _) = scheduled(&actions, now, 5000., 5000.);
    e.handle(now + 1., Event::WsOpened { socket: first });
    e.handle(
        now + 1.,
        Event::WsMessage {
            socket: first,
            text: "PONG",
        },
    );
    e.handle(now + 1., Event::TimerFired { timer: retry });
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a, Action::WsSend { .. } | Action::WsConnect { .. }))
    );
    now += 10_000.;
    e.handle(now, Event::TimerFired { timer: watch });
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::WsClose { socket: s } if *s == socket)),
        "never-open socket must close"
    );
    let (retry2, at2) = scheduled(&actions, now, 1500., 2000.);
    now = at2;
    e.handle(now, Event::TimerFired { timer: retry2 });
    let actions: Vec<_> = e.drain_actions().collect();
    let fresh = connected(&actions);
    e.handle(now, Event::WsOpened { socket: fresh });
    let restored: Vec<_> = e.drain_actions().collect();
    assert!(
        restored.iter().any(|a| matches!(a, Action::WsSend { .. })),
        "current interests or authentication restored"
    );
    e.handle(
        now + 1.,
        Event::WsClosed {
            socket: fresh,
            code: 1006,
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    scheduled(&actions, now + 1., 3000., 4000.);
}
