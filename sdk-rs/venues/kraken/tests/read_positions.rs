//! `readPositions`: success only after a read issued after the command commits.
#![allow(dead_code)]
mod support;
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Notice, Outcome, SocketId, TimerId,
    VenueAction, VenueError,
};
use safe_cex_venue_kraken::Kraken;
use support::*;

const FLAT: &str = r#"{"result":"success","openPositions":[]}"#;
const ROW: &str = r#"{"result":"success","openPositions":[{"symbol":"PF_XBTUSD","side":"long","size":2,"price":100,"unrealizedPnl":40,"maxFixedLeverage":10}]}"#;
const POSITIONS: &str = "openpositions";

struct H {
    e: Engine<Kraken>,
    socket: SocketId,
    http: Vec<HttpRequest>,
    timers: Vec<(TimerId, f64)>,
    done: Vec<(u32, Result<String, VenueError>)>,
}

fn started() -> H {
    let (e, socket) = ready();
    let mut h = H {
        e,
        socket,
        http: Vec::new(),
        timers: Vec::new(),
        done: Vec::new(),
    };
    h.pump();
    h.http.clear();
    h
}

impl H {
    fn pump(&mut self) {
        for action in safe_cex_engine::test_support::prepared_actions(&mut self.e, NOW) {
            match action {
                Action::Http(request) => self.http.push(request),
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
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
            NOW,
            Event::Command(Command::Venue {
                command,
                action: VenueAction::ReadPositions,
            }),
        );
        self.pump();
    }
    fn has(&self) -> bool {
        self.http.iter().any(|r| r.url.contains(POSITIONS))
    }
    fn take(&mut self) -> HttpRequest {
        let index = self
            .http
            .iter()
            .position(|r| r.url.contains(POSITIONS))
            .expect("a positions read");
        self.http.remove(index)
    }
    fn answer(&mut self, status: u16, body: &str) {
        let request = self.take();
        self.reply(&request, status, body);
    }
    fn reply(&mut self, request: &HttpRequest, status: u16, body: &str) {
        reply(&mut self.e, request, status, body);
        self.pump();
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
    assert_eq!(h.open(), 0.0);
    h.read(40);
    assert!(h.has(), "a read is issued");
    assert!(h.result(40).is_none());
    h.answer(200, ROW);
    assert!(h.succeeded(40), "{:?}", h.done);
    assert_eq!(h.open(), 2.0, "the store row is updated");
}

#[test]
fn a_read_in_flight_is_followed_by_a_new_one() {
    let mut h = started();
    h.read(39);
    let stale = h.take();
    h.read(41);
    assert!(!h.has(), "no second read yet");
    h.reply(&stale, 200, ROW);
    assert!(h.succeeded(39), "its own read answers it");
    assert!(h.result(41).is_none(), "the older read cannot");
    assert_eq!(h.open(), 2.0);
    assert!(h.has(), "a new read follows");
    h.answer(200, FLAT);
    assert!(h.succeeded(41), "{:?}", h.done);
    assert_eq!(h.open(), 0.0);
}

#[test]
fn a_failed_read_fails_the_command_with_the_venues_error() {
    let mut h = started();
    h.read(42);
    h.answer(401, r#"{"result":"error","error":"authenticationError"}"#);
    let error = h.failure(42);
    assert!(error.message.contains("authenticationError"), "{error:?}");
    assert_eq!(error.kind, Some(ErrorKind::AuthInvalid));
    assert_eq!(error.outcome, Some(Outcome::NotSent));

    let mut h = started();
    h.read(43);
    h.answer(500, r#"{"result":"error","error":"apiLimitExceeded"}"#);
    assert!(h.failure(43).message.contains("apiLimitExceeded"));
    assert_eq!(h.failure(43).outcome, Some(Outcome::NotSent));
}

#[test]
fn a_lost_read_keeps_the_engines_typed_cause() {
    let mut h = started();
    h.read(44);
    let request = h.take();
    h.e.handle(
        NOW,
        Event::HttpFailed {
            request: request.id,
            error: "connection lost",
        },
    );
    h.pump();
    assert_eq!(h.failure(44).kind, Some(ErrorKind::Network));
    assert_eq!(h.failure(44).outcome, Some(Outcome::NotSent));
}

#[test]
fn a_malformed_reply_or_an_unmappable_row_fails_the_command() {
    let mut h = started();
    h.read(45);
    h.answer(200, r#"{"result":"success","openPositions":{}}"#);
    assert_eq!(h.failure(45).outcome, Some(Outcome::NotSent));

    let mut h = started();
    h.read(46);
    h.answer(
        200,
        r#"{"result":"success","openPositions":[{"symbol":"PF_XBTUSD","side":"sideways"}]}"#,
    );
    assert!(
        h.failure(46).message.contains("could not be mapped"),
        "{:?}",
        h.done
    );
    assert_eq!(h.failure(46).outcome, Some(Outcome::NotSent));
}

#[test]
fn a_read_a_newer_stream_frame_held_is_not_a_commit() {
    let mut h = started();
    h.read(47);
    message(
        &mut h.e,
        h.socket,
        NOW,
        r#"{"feed":"open_positions","positions":[{"instrument":"PF_XBTUSD","balance":2,"entry_price":100,"mark_price":120,"pnl":40,"effective_leverage":10,"liquidation_threshold":50}]}"#,
    );
    h.pump();
    let frame_size = h.open();
    assert!(frame_size > 0.0);
    h.answer(200, FLAT);
    assert!(h.result(47).is_none(), "the frame held that read");
    assert_eq!(h.open(), frame_size, "the frame stands");
    let due: Vec<_> = std::mem::take(&mut h.timers)
        .into_iter()
        .filter(|(_, at)| *at <= NOW + 1_000.0)
        .collect();
    for (timer, at) in due {
        h.e.handle(at, Event::TimerFired { timer });
        h.pump();
    }
    assert!(h.has(), "a new read follows");
    h.answer(200, FLAT);
    assert!(h.succeeded(47), "{:?}", h.done);
    assert_eq!(h.open(), 0.0);
}

#[test]
fn a_public_engine_refuses_without_sending_and_spot_is_unsupported() {
    let (mut e, _) = ready_with(safe_cex_venue_kraken::Options {
        public_data_only: true,
        ..options()
    });
    let _ = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    e.handle(
        NOW,
        Event::Command(Command::Venue {
            command: 60,
            action: VenueAction::ReadPositions,
        }),
    );
    let out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 60, error }) if error.kind != Some(ErrorKind::Unsupported) && error.outcome == Some(Outcome::NotSent))), "{out:?}");
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
}
