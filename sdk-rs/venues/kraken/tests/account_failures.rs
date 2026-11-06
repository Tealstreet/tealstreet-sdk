mod support;

use safe_cex_engine::{Action, Engine, Event, Level, Notice, log::Value};
use safe_cex_venue_kraken::Kraken;
use support::*;

fn notices(actions: &[Action]) -> Vec<&safe_cex_engine::LogEvent> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::Event(event)) => Some(event),
            _ => None,
        })
        .collect()
}

fn read(
    e: &mut Engine<Kraken>,
    socket: safe_cex_engine::SocketId,
    path: &str,
) -> safe_cex_engine::HttpRequest {
    message(e, socket, NOW, r#"{"feed":"fills","fills":[]}"#);
    let timer = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if at_ms == NOW + 1000.0 => Some(timer),
            _ => None,
        })
        .expect("native reconcile debounce");
    e.handle(NOW + 1000.0, Event::TimerFired { timer });
    requests(e)
        .into_iter()
        .find(|r| r.url.ends_with(path))
        .expect("account read")
}

#[test]
fn reconciliation_refusals_keep_http_status_and_native_code_on_direct_and_streamed_paths() {
    for path in [
        "openorders",
        "openpositions",
        "leveragepreferences",
        "accounts",
    ] {
        for streamed in [false, true] {
            for (status, code, kind, event_code) in [
                (200, "authenticationError", "authInvalid", "auth.rejected"),
                (401, "authenticationError", "authInvalid", "auth.rejected"),
                (403, "unauthorized", "authPermission", "auth.rejected"),
                (429, "apiLimitExceeded", "rateLimited", "request.failed"),
                (408, "authenticationError", "timeout", "request.failed"),
                (503, "Unavailable", "venueUnavailable", "request.failed"),
                (
                    503,
                    "authenticationError",
                    "venueUnavailable",
                    "request.failed",
                ),
                (200, "invalidArgument", "rejected", "request.failed"),
            ] {
                let (mut e, socket) = ready();
                let request = read(&mut e, socket, path);
                let body =
                    format!(r#"{{"result":"error","error":"{code}","private":"not diagnostic"}}"#);
                if streamed {
                    e.handle(
                        NOW + 1001.0,
                        Event::HttpStart {
                            request: request.id,
                            status,
                            headers: "",
                        },
                    );
                    e.handle(
                        NOW + 1001.0,
                        Event::HttpChunk {
                            request: request.id,
                            data: body.as_bytes(),
                        },
                    );
                    e.handle(
                        NOW + 1001.0,
                        Event::HttpEnd {
                            request: request.id,
                        },
                    );
                } else {
                    reply_at(&mut e, NOW + 1001.0, &request, status, &body);
                }
                let actions: Vec<_> = e.drain_actions().collect();
                let events = notices(&actions);
                let event = events
                    .iter()
                    .find(|event| event.code == event_code)
                    .expect("classified refusal notice");
                assert_eq!(event.level, Level::Error);
                assert!(
                    event.fields.contains(&("kind", Value::Str(kind.into()))),
                    "{path} streamed={streamed}: {event:?}"
                );
                assert!(
                    event
                        .fields
                        .contains(&("venueCode", Value::Str(code.into()))),
                    "{event:?}"
                );
                assert!(
                    event.message.contains(&format!("HTTP {status}")),
                    "{event:?}"
                );
                assert!(event.message.contains(code), "{event:?}");
                assert!(!event.message.contains("transport"));
                assert!(!event.message.contains("not diagnostic"));
                assert_eq!(e.store().balance().total, 1000.0);
                assert!(actions.iter().any(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms >= NOW + 2501.0 && *at_ms <= NOW + 3001.0)), "heal remains jittered");
            }
        }
    }
}

#[test]
fn a_real_transport_failure_stays_network_and_repeats_are_warnings() {
    let (mut e, socket) = ready();
    message(&mut e, socket, NOW, r#"{"feed":"fills","fills":[]}"#);
    let timer = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if at_ms == NOW + 1000.0 => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(NOW + 1000.0, Event::TimerFired { timer });
    let reads = requests(&mut e);
    assert_eq!(reads.len(), 4);
    for request in &reads {
        e.handle(
            NOW + 1001.0,
            Event::HttpFailed {
                request: request.id,
                error: "HTTP transport request failed",
            },
        );
    }
    let actions: Vec<_> = e.drain_actions().collect();
    let events: Vec<_> = notices(&actions)
        .into_iter()
        .filter(|event| event.code == "request.failed")
        .collect();
    assert_eq!(events.len(), 4);
    assert_eq!(
        events.iter().map(|event| event.level).collect::<Vec<_>>(),
        [Level::Error, Level::Warn, Level::Warn, Level::Warn]
    );
    for event in events {
        assert!(
            event
                .fields
                .contains(&("kind", Value::Str("network".into()))),
            "{event:?}"
        );
        assert!(event.message.contains("transport"));
    }
    assert!(
        notices(&actions)
            .iter()
            .all(|event| event.code != "auth.rejected")
    );
}

#[test]
fn private_socket_refusal_carries_krakens_error_or_message() {
    for (detail, kind, code) in [
        (
            r#""error":"authenticationError""#,
            "authInvalid",
            "auth.rejected",
        ),
        (
            r#""message":"permission denied""#,
            "authPermission",
            "auth.rejected",
        ),
        (
            r#""message":"Invalid feed""#,
            "rejected",
            "subscription.rejected",
        ),
    ] {
        let (mut e, socket) = ready();
        message(
            &mut e,
            socket,
            NOW,
            &format!(r#"{{"event":"subscribed_failed","feed":"fills",{detail}}}"#),
        );
        let actions: Vec<_> = e.drain_actions().collect();
        let events = notices(&actions);
        let event = events
            .iter()
            .find(|event| event.code == code)
            .expect("private refusal");
        assert!(
            event.fields.contains(&("kind", Value::Str(kind.into()))),
            "{event:?}"
        );
        let detail = detail.split(':').nth(1).unwrap().trim_matches('"');
        assert!(event.message.contains(detail), "{event:?}");
        assert!(
            event
                .fields
                .contains(&("venueMessage", Value::Str(detail.into()))),
            "{event:?}"
        );
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::WsClose { socket: closed } if *closed == socket))
        );
    }
}
