use safe_cex_engine::book::Book;
use safe_cex_engine::log::{MAX_LOG_BYTES, REDACTED, code};
use safe_cex_engine::{
    Action, Command, Cx, Engine, Event, Health, Level, LogEvent, LogLevel, Method, Notice,
    Redacted, RequestId, Response, SocketId, TimerId, Value, Venue,
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
            Some(("log", message)) => cx.log(LogLevel::Info, message),
            Some(("warn", message)) => cx.log(LogLevel::Warning, message),
            Some((level, message)) => {
                let level = Level::parse(level).unwrap();
                cx.event(
                    level,
                    code::SNAPSHOT_DISCARDED,
                    message,
                    [
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
    let closed: Vec<_> = engine.drain_actions().collect();
    assert!(matches!(
        closed.as_slice(),
        [Action::Notice(Notice::Event(LogEvent { code: "ws.closed", fields, .. }))]
            if fields == &[("socket", Value::Num(1.)), ("code", Value::Num(1006.))]
    ));

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
