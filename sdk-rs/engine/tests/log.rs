use safe_cex_engine::book::Book;
use safe_cex_engine::log::{MAX_LOG_BYTES, REDACTED, code};
use safe_cex_engine::{
    Action, Command, Cx, Engine, ErrorKind, Event, Health, Level, LogEvent, LogLevel, Method,
    Notice, Redacted, RequestId, Response, SocketChannel, SocketId, SocketSpec, TimerId, Value,
    Venue, VenueError,
};

/// Turns each socket message into one notice so tests drive `Cx` directly.
#[derive(Default)]
struct Logger;

impl Venue for Logger {
    type Pending = ();
    fn start(&mut self, _cx: &mut Cx<'_, ()>) {}
    fn on_open(&mut self, _cx: &mut Cx<'_, ()>, _socket: SocketId) {}
    fn on_message(&mut self, cx: &mut Cx<'_, ()>, _socket: SocketId, text: &str) {
        match text.split_once(':') {
            Some(("request", _)) => {
                cx.request(
                    Method::Get,
                    "https://example.test".into(),
                    Vec::new(),
                    None,
                    (),
                );
            }
            Some(("connect", role)) => {
                let channel = match role {
                    "private" => SocketChannel::Private,
                    "business" => SocketChannel::Business,
                    _ => SocketChannel::Public,
                };
                cx.connect(SocketSpec::new("wss://example.test").channel(channel));
            }
            Some(("close", socket)) => cx.close(SocketId(socket.parse().unwrap())),
            Some(("deactivate", _)) => cx.deactivate(),
            Some(("retry", "a")) => cx.retried_failure("site.a", "read failed"),
            Some(("retry", _)) => cx.retried_failure("site.b", "read failed"),
            Some(("rejectlogged", _)) => {
                cx.notice(Level::Error, code::AUTH_REJECTED, "key rejected");
                cx.deactivate();
            }
            Some(("rejectboth", _)) => {
                cx.startup_failed(VenueError::new("refused").kind(ErrorKind::AuthInvalid));
                cx.deactivate();
            }
            Some(("fail", kind)) => cx.fail(
                1,
                VenueError::new("refused").kind(if kind == "auth" {
                    ErrorKind::AuthInvalid
                } else {
                    ErrorKind::Rejected
                }),
            ),
            Some(("log", message)) => cx.log(LogLevel::Info, message),
            Some(("warn", message)) => cx.log(LogLevel::Warning, message),
            Some((level, message)) => {
                let level = Level::parse(level).unwrap();
                cx.event(
                    level,
                    code::SNAPSHOT_DISCARDED,
                    message,
                    vec![
                        ("body", Value::from(message)),
                        ("secret", Redacted("api-secret").into()),
                        ("count", 3u32.into()),
                    ],
                );
            }
            None => panic!("unknown fixture message {text}"),
        }
    }
    fn on_close(&mut self, _cx: &mut Cx<'_, ()>, _socket: SocketId, _code: u16) {}
    fn on_response(&mut self, _cx: &mut Cx<'_, ()>, _pending: (), _response: Response<'_>) {}
    fn on_request_failed(&mut self, _cx: &mut Cx<'_, ()>, _pending: (), _error: &str) {}
    fn on_timer(&mut self, _cx: &mut Cx<'_, ()>, _timer: TimerId) {}
    fn on_command(&mut self, _cx: &mut Cx<'_, ()>, _command: Command) {}
    fn book(&self, _symbol: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn engine() -> Engine<Logger> {
    let mut engine = Engine::new(Logger);
    engine.start(0.);
    engine
}

fn send(engine: &mut Engine<Logger>, text: &str) -> Vec<Notice> {
    engine.handle(
        0.,
        Event::WsMessage {
            socket: SocketId(1),
            text,
        },
    );
    engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::Notice(notice) => Some(notice),
            _ => None,
        })
        .collect()
}

fn request(engine: &mut Engine<Logger>) -> RequestId {
    engine.handle(
        0.,
        Event::WsMessage {
            socket: SocketId(1),
            text: "request:",
        },
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    match actions.as_slice() {
        [Action::Http(http)] => http.id,
        other => panic!("expected one request, got {other:?}"),
    }
}

#[test]
fn events_below_the_host_minimum_are_dropped_but_still_counted() {
    let mut engine = engine();
    assert!(send(&mut engine, "debug:hidden").is_empty());
    assert_eq!(engine.health().snapshots_discarded, 1);

    engine.set_log_level(Level::Debug);
    let notices = send(&mut engine, "debug:shown");
    assert!(matches!(
        notices.as_slice(),
        [Notice::Event(LogEvent { level: Level::Debug, code: "snapshot.discarded", message, .. })]
            if message == "shown"
    ));
    assert_eq!(engine.health().snapshots_discarded, 2);

    engine.set_log_level(Level::Warn);
    assert!(send(&mut engine, "info:hidden").is_empty());
    assert!(send(&mut engine, "log:shim info is filtered too").is_empty());
    assert!(matches!(
        send(&mut engine, "warn:shim warning passes").as_slice(),
        [Notice::Log {
            level: LogLevel::Warning,
            ..
        }]
    ));
}

#[test]
fn messages_and_string_fields_are_capped_on_a_char_boundary_and_redacted_fields_never_leak() {
    let mut engine = engine();
    let long = format!("error:{}", "é".repeat(MAX_LOG_BYTES));
    let notices = send(&mut engine, &long);
    let [Notice::Event(event)] = notices.as_slice() else {
        panic!("expected one event, got {notices:?}");
    };
    assert!(event.message.len() <= MAX_LOG_BYTES);
    assert!(event.message.ends_with('…'));
    let Value::Str(body) = &event.fields[0].1 else {
        panic!("body field")
    };
    assert!(body.len() <= MAX_LOG_BYTES);
    assert_eq!(event.fields[1], ("secret", Value::Redacted));
    assert_eq!(event.fields[2], ("count", Value::Num(3.)));
    assert_eq!(format!("{:?}", Redacted("api-secret")), REDACTED);
    assert!(!format!("{notices:?}").contains("api-secret"));

    let shim = format!("log:{}", "x".repeat(4 * MAX_LOG_BYTES));
    let notices = send(&mut engine, &shim);
    let [Notice::Log { message, .. }] = notices.as_slice() else {
        panic!("expected one shim log")
    };
    assert!(message.len() <= MAX_LOG_BYTES);
}

#[test]
fn socket_and_http_failures_are_counted_without_quoting_transport_errors() {
    let mut engine = engine();
    engine.set_log_level(Level::Debug);
    engine.handle(
        0.,
        Event::WsOpened {
            socket: SocketId(1),
        },
    );
    engine.handle(
        0.,
        Event::WsClosed {
            socket: SocketId(1),
            code: 1006,
        },
    );
    // Counted only: transport-level notices would cost every venue's wasm budget.
    assert_eq!(engine.drain_actions().count(), 0);

    let failed = request(&mut engine);
    engine.handle(
        0.,
        Event::HttpFailed {
            request: failed,
            error: "GET https://example.test/?signature=abc failed",
        },
    );
    let notices: Vec<_> = engine.drain_actions().collect();
    assert!(!format!("{notices:?}").contains("signature"));

    for status in [429, 503, 400] {
        let id = request(&mut engine);
        engine.handle(
            0.,
            Event::HttpResponse {
                request: id,
                response: Response {
                    status,
                    headers: "",
                    body: "{}",
                },
            },
        );
        engine.drain_actions();
    }
    // A response for an unknown request is not a failure of this engine.
    engine.handle(
        0.,
        Event::HttpResponse {
            request: RequestId(9_999),
            response: Response {
                status: 500,
                headers: "",
                body: "",
            },
        },
    );
    assert_eq!(
        engine.health(),
        &Health {
            ws_opened: 1,
            ws_closed: 1,
            http_failures: 2,
            rate_limited: 1,
            ..Health::default()
        }
    );
}

fn connected(engine: &mut Engine<Logger>, role: &str) -> SocketId {
    engine.handle(
        0.,
        Event::WsMessage {
            socket: SocketId(1),
            text: &format!("connect:{role}"),
        },
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    match actions.as_slice() {
        [Action::WsConnect { socket, .. }] => *socket,
        other => panic!("expected one connect, got {other:?}"),
    }
}

#[test]
fn reconnects_count_a_role_reopened_after_a_close_never_a_first_or_parallel_connect() {
    let mut engine = engine();
    let private = connected(&mut engine, "private");
    connected(&mut engine, "public");
    connected(&mut engine, "public");
    assert_eq!(engine.health().reconnects, 0, "first and parallel connects");

    engine.handle(
        0.,
        Event::WsClosed {
            socket: private,
            code: 1006,
        },
    );
    connected(&mut engine, "public");
    assert_eq!(
        engine.health().reconnects,
        0,
        "another role's connect is not this reconnect"
    );
    let replacement = connected(&mut engine, "private");
    assert_eq!(engine.health().reconnects, 1);

    // An engine-initiated close is deliberate: it counts as a close, and the
    // venue's next connect on that role (a new feed, say) is not a reconnect.
    send(&mut engine, &format!("close:{}", replacement.0));
    assert_eq!(engine.health().ws_closed, 2);
    connected(&mut engine, "private");
    assert_eq!(engine.health().reconnects, 1);
}

#[test]
fn only_private_socket_frames_advance_the_last_private_message() {
    let mut engine = engine();
    let public = connected(&mut engine, "public");
    let private = connected(&mut engine, "private");
    engine.handle(
        5.,
        Event::WsMessage {
            socket: public,
            text: "log:quiet",
        },
    );
    assert_eq!(engine.health().last_private_message_ms, None);
    engine.handle(
        7.,
        Event::WsMessage {
            socket: private,
            text: "log:quiet",
        },
    );
    assert_eq!(engine.health().last_private_message_ms, Some(7.));
    engine.handle(
        9.,
        Event::WsClosed {
            socket: private,
            code: 1000,
        },
    );
    engine.handle(
        11.,
        Event::WsMessage {
            socket: private,
            text: "log:late",
        },
    );
    assert_eq!(
        engine.health().last_private_message_ms,
        Some(7.),
        "a closed socket is no longer private"
    );
}

#[test]
fn deactivation_and_auth_kind_failures_count_as_auth_failures() {
    let mut engine = engine();
    send(&mut engine, "fail:rejected");
    assert_eq!(engine.health().auth_failures, 0);
    send(&mut engine, "fail:auth");
    send(&mut engine, "deactivate:");
    assert_eq!(engine.health().auth_failures, 2);
    // One rejection reported both ways in one event counts once.
    send(&mut engine, "rejectboth:");
    assert_eq!(engine.health().auth_failures, 3);
}

#[test]
fn an_auth_rejected_event_and_a_deactivation_in_one_event_count_once() {
    let mut engine = engine();
    send(&mut engine, "rejectlogged:");
    assert_eq!(engine.health().auth_failures, 1);
    send(&mut engine, "rejectlogged:");
    assert_eq!(engine.health().auth_failures, 2);
}

#[test]
fn a_retried_read_is_an_error_once_per_quiet_spell_and_per_site() {
    let mut engine = engine();
    let level_at = |engine: &mut Engine<Logger>, now: f64, site: &str| {
        engine.handle(
            now,
            Event::WsMessage {
                socket: SocketId(1),
                text: &format!("retry:{site}"),
            },
        );
        let levels: Vec<_> = engine
            .drain_actions()
            .filter_map(|action| match action {
                Action::Notice(Notice::Event(LogEvent {
                    level,
                    code: "request.failed",
                    ..
                })) => Some(level),
                _ => None,
            })
            .collect();
        assert_eq!(levels.len(), 1, "one request.failed event");
        levels[0]
    };
    assert_eq!(level_at(&mut engine, 0., "a"), Level::Error);
    assert_eq!(level_at(&mut engine, 1_000., "a"), Level::Warn);
    assert_eq!(
        level_at(&mut engine, 1_000., "b"),
        Level::Error,
        "sites are independent"
    );
    // Still failing every poll: the quiet spell is measured from the latest failure.
    assert_eq!(level_at(&mut engine, 290_000., "a"), Level::Warn);
    assert_eq!(level_at(&mut engine, 590_001., "a"), Level::Error);
}
