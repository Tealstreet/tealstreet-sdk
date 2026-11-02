//! `readPositions`: success only after a read issued after the command commits.
#![allow(dead_code)]
mod support;
use safe_cex_engine::{
    Action, Command, ErrorKind, Event, HttpRequest, Notice, Outcome, Response, VenueAction,
    VenueError,
};
use support::*;

const FLAT: &str = r#"{"code":0,"data":{"positionList":[]}}"#;
const PATH: &str = "get_pending_positions";

struct H {
    e: TestEngine,
    http: Vec<HttpRequest>,
    done: Vec<(u32, Result<String, VenueError>)>,
}

fn started() -> H {
    let (e, _) = startup();
    let mut h = H {
        e,
        http: Vec::new(),
        done: Vec::new(),
    };
    h.pump();
    h.http.clear();
    h
}

impl H {
    fn pump(&mut self) {
        for action in safe_cex_engine::test_support::prepared_actions(&mut self.e, 100.0) {
            match action {
                Action::Http(request) => self.http.push(request),
                Action::Notice(Notice::CommandDone { command, result }) => {
                    self.done.push((command, Ok(result)));
                }
                Action::Notice(Notice::CommandFailed { command, error }) => {
                    self.done.push((command, Err(error)));
                }
                _ => {}
            }
        }
    }
    fn read(&mut self, command: u32) {
        self.e.handle(
            100.0,
            Event::Command(Command::Venue {
                command,
                action: VenueAction::ReadPositions,
            }),
        );
        self.pump();
    }
    fn has(&self) -> bool {
        self.http.iter().any(|r| r.url.contains(PATH))
    }
    fn take(&mut self) -> HttpRequest {
        let index = self
            .http
            .iter()
            .position(|r| r.url.contains(PATH))
            .expect("a positions read");
        self.http.remove(index)
    }
    fn reply(&mut self, request: &HttpRequest, status: u16, body: &str) {
        self.e.handle(
            100.0,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
        self.pump();
    }
    fn answer(&mut self, status: u16, body: &str) {
        let request = self.take();
        self.reply(&request, status, body);
    }
    fn result(&self, id: u32) -> Option<&Result<String, VenueError>> {
        self.done.iter().find(|(c, _)| *c == id).map(|(_, r)| r)
    }
    fn succeeded(&self, id: u32) -> bool {
        self.result(id).is_some_and(|r| r.as_deref() == Ok("[]"))
    }
    fn failure(&self, id: u32) -> &VenueError {
        match self.result(id) {
            Some(Err(error)) => error,
            other => panic!("expected a failure for {id}: {other:?}"),
        }
    }
    fn open(&self) -> f64 {
        self.e.store().positions().iter().map(|p| p.contracts).sum()
    }
}

#[test]
fn a_read_issued_after_the_command_commits_then_it_succeeds() {
    let mut h = started();
    assert_eq!(h.open(), 0.01);
    h.read(40);
    assert!(h.has(), "a read is issued");
    assert!(h.result(40).is_none());
    h.answer(200, FLAT);
    assert!(h.succeeded(40), "{:?}", h.done);
    assert_eq!(h.open(), 0.0, "the store is flat");
}

#[test]
fn a_read_in_flight_is_followed_by_a_new_one() {
    let mut h = started();
    h.read(39);
    let stale = h.take();
    h.read(41);
    assert!(!h.has(), "no second read yet");
    h.reply(&stale, 200, POSITIONS);
    assert!(h.succeeded(39), "its own read answers it");
    assert!(h.result(41).is_none(), "the older read cannot");
    assert_eq!(h.open(), 0.01);
    assert!(h.has(), "a new read follows at once");
    h.answer(200, FLAT);
    assert!(h.succeeded(41), "{:?}", h.done);
    assert_eq!(h.open(), 0.0);
}

#[test]
fn a_failed_read_fails_the_command_with_the_venues_error() {
    let mut h = started();
    h.read(42);
    h.answer(200, r#"{"code":10003,"msg":"Invalid api key"}"#);
    let error = h.failure(42);
    assert!(error.message.contains("Invalid api key"), "{error:?}");
    assert_eq!(error.kind, Some(ErrorKind::AuthInvalid));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert_eq!(h.open(), 0.01, "the store stands");
}

#[test]
fn a_lost_read_keeps_the_engines_typed_cause() {
    let mut h = started();
    h.read(43);
    let request = h.take();
    h.e.handle(
        100.0,
        Event::HttpFailed {
            request: request.id,
            error: "connection lost",
        },
    );
    h.pump();
    assert_eq!(h.failure(43).kind, Some(ErrorKind::Network));
    assert_eq!(h.failure(43).outcome, Some(Outcome::NotSent));
}

#[test]
fn a_malformed_reply_or_an_unmappable_row_fails_the_command() {
    let mut h = started();
    h.read(44);
    h.answer(200, "not json");
    assert_eq!(h.failure(44).outcome, Some(Outcome::NotSent));

    let mut h = started();
    h.read(45);
    h.answer(
        200,
        r#"{"code":0,"data":{"positionList":[{"qty":"1","side":"SELL"}]}}"#,
    );
    assert!(
        h.failure(45).message.contains("could not be mapped"),
        "{:?}",
        h.done
    );
    assert_eq!(h.failure(45).outcome, Some(Outcome::NotSent));
    assert_eq!(h.open(), 0.01, "the rejected row's stored copy stands");
}

#[test]
fn an_unreadable_quantity_is_not_flat() {
    for (command, row) in [
        (46, r#"{"symbol":"BTCUSDT","qty":"nope","side":"SELL"}"#),
        (47, r#"{"symbol":"BTCUSDT","side":"SELL"}"#),
    ] {
        let mut h = started();
        h.read(command);
        let body = format!(r#"{{"code":0,"data":{{"positionList":[{row}]}}}}"#);
        h.answer(200, &body);
        assert_eq!(h.failure(command).outcome, Some(Outcome::NotSent));
        assert_eq!(h.open(), 0.01, "the stored copy stands");
    }
}

#[test]
fn a_public_engine_refuses_without_sending() {
    let mut e = engine(Options::default());
    e.handle(
        0.0,
        Event::Command(Command::Venue {
            command: 60,
            action: VenueAction::ReadPositions,
        }),
    );
    let out = safe_cex_engine::test_support::prepared_actions(&mut e, 0.0);
    assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 60, error }) if error.kind != Some(ErrorKind::Unsupported) && error.outcome == Some(Outcome::NotSent))), "{out:?}");
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
}

#[test]
fn a_row_on_an_unknown_market_commits_to_the_store_rather_than_being_dropped() {
    let mut h = started();
    h.read(70);
    h.answer(200, &POSITIONS.replace("BTCUSDT", "DOGEUSDT"));
    assert!(h.succeeded(70), "{:?}", h.done);
    assert_eq!(h.open(), 0.01, "the row is shown, not skipped");
}
