mod support;
use safe_cex_engine::{Action, Event, Level, Notice, TimerId, log::code};
use support::*;

fn message(
    e: &mut TestEngine,
    socket: safe_cex_engine::SocketId,
    at: f64,
    text: &str,
) -> Vec<Action> {
    e.handle(at, Event::WsMessage { socket, text });
    e.drain_actions().collect()
}
fn retry(actions: &[Action], at: f64) -> TimerId {
    actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == at => Some(*timer),
            _ => None,
        })
        .expect("bounded login retry")
}
fn login_sends(actions: &[Action]) -> usize {
    actions
        .iter()
        .filter(|a| matches!(a, Action::WsSend { text, .. } if text.contains("\"login\"")))
        .count()
}
#[test]
fn malformed_login_controls_warn_without_claiming_bad_credentials_and_retry() {
    for text in [
        r#"{"op":"login"}"#,
        r#"{"op":"login","data":null}"#,
        r#"{"op":"login","data":{}}"#,
        r#"{"op":"login","data":{"result":"false","secret":"never-log-me"}}"#,
    ] {
        let (mut e, socket) = startup();
        let actions = message(&mut e, socket, 100.0, text);
        assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.code == code::STREAM_MALFORMED && event.level == Level::Warn)), "{text}");
        assert!(!actions.iter().any(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.code == code::AUTH_REJECTED || event.message.contains("never-log-me"))));
        let timer = retry(&actions, 2100.0);
        e.handle(2100.0, Event::TimerFired { timer });
        assert_eq!(login_sends(&e.drain_actions().collect::<Vec<_>>()), 1);
    }
}
#[test]
fn explicit_refusal_remains_visible_but_success_cancels_its_retry() {
    let (mut e, socket) = startup();
    let actions = message(
        &mut e,
        socket,
        100.0,
        r#"{"op":"login","data":{"result":false}}"#,
    );
    assert!(actions.iter().any(
        |a| matches!(a, Action::Notice(Notice::Event(event)) if event.code == code::AUTH_REJECTED)
    ));
    let timer = retry(&actions, 2100.0);
    message(
        &mut e,
        socket,
        200.0,
        r#"{"op":"login","data":{"result":true}}"#,
    );
    e.handle(2100.0, Event::TimerFired { timer });
    assert_eq!(login_sends(&e.drain_actions().collect::<Vec<_>>()), 0);
}
#[test]
fn repeated_login_refusals_do_not_postpone_the_existing_retry() {
    let (mut e, socket) = startup();
    let text = r#"{"op":"login","data":{"result":false}}"#;
    let first = message(&mut e, socket, 100.0, text);
    let timer = retry(&first, 2100.0);
    let second = message(&mut e, socket, 2000.0, text);
    assert!(!second.iter().any(|a| matches!(a, Action::SetTimer { .. })));
    e.handle(2100.0, Event::TimerFired { timer });
    assert_eq!(login_sends(&e.drain_actions().collect::<Vec<_>>()), 1);
}
#[test]
fn retired_login_retry_cannot_sign_on_a_replacement_socket() {
    let (mut e, socket) = startup();
    let first = message(
        &mut e,
        socket,
        100.0,
        r#"{"op":"login","data":{"result":false}}"#,
    );
    let timer = retry(&first, 2100.0);
    e.handle(200.0, Event::WsClosed { socket, code: 1006 });
    let actions: Vec<_> = e.drain_actions().collect();
    let mut replacement = actions.iter().find_map(|a| match a {
        Action::WsConnect { socket, .. } => Some(*socket),
        _ => None,
    });
    if replacement.is_none() {
        let (retry_timer, at) = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                _ => None,
            })
            .expect("socket retry");
        e.handle(at, Event::TimerFired { timer: retry_timer });
        replacement = e.drain_actions().find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        });
    }
    e.handle(
        1500.0,
        Event::WsOpened {
            socket: replacement.unwrap(),
        },
    );
    e.drain_actions().for_each(drop);
    e.handle(2100.0, Event::TimerFired { timer });
    assert_eq!(login_sends(&e.drain_actions().collect::<Vec<_>>()), 0);
}
