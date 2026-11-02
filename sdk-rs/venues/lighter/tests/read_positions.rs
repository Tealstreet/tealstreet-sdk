//! `readPositions`: success only after a read issued after the command commits.
#![allow(dead_code)]
mod support;
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Notice, Outcome, SocketId, VenueAction,
    VenueError,
};
use safe_cex_venue_lighter::Lighter;
use support::*;

const FLAT: &str = r#"{"code":200,"accounts":[{"index":718895,"collateral":"1000","total_asset_value":"1000","available_balance":"900","positions":[]}]}"#;
const SNAPSHOT: &str = "/account?";

struct H {
    e: Engine<Lighter>,
    socket: SocketId,
    timers: Timers,
    http: Vec<HttpRequest>,
    done: Vec<(u32, Result<String, VenueError>)>,
}

fn started() -> H {
    let (e, socket, timers) = ready_tracked(options());
    let mut h = H {
        e,
        socket,
        timers,
        http: Vec::new(),
        done: Vec::new(),
    };
    h.pump();
    h.http.clear();
    h
}

impl H {
    fn sort(&mut self, actions: Vec<Action>) {
        for action in actions {
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
    fn pump(&mut self) {
        let actions = drain(&mut self.e, &mut self.timers);
        self.sort(actions);
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
        self.http.iter().any(|r| r.url.contains(SNAPSHOT))
    }
    fn take(&mut self) -> HttpRequest {
        let index = self
            .http
            .iter()
            .position(|r| r.url.contains(SNAPSHOT))
            .expect("a snapshot read");
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
    assert_eq!(h.open(), 2.0);
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
    h.reply(&stale, 200, ACCOUNT);
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
    h.answer(429, r#"{"code":429,"message":"too many requests"}"#);
    let error = h.failure(42);
    assert!(error.message.contains("too many requests"), "{error:?}");
    assert_eq!(error.kind, Some(ErrorKind::RateLimited));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert_eq!(h.open(), 2.0, "the store stands");
}

#[test]
fn after_a_rate_limit_the_command_refuses_without_sending() {
    let mut h = started();
    h.read(42);
    h.answer(429, r#"{"code":429,"message":"too many requests"}"#);
    h.read(43);
    assert_eq!(h.failure(43).kind, Some(ErrorKind::RateLimited));
    assert!(!h.has());
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
    h.answer(200, r#"{"code":200,"accounts":[{"index":718895}]}"#);
    assert_eq!(h.failure(45).outcome, Some(Outcome::NotSent));

    let mut h = started();
    h.read(46);
    h.answer(
        200,
        r#"{"code":200,"accounts":[{"index":718895,"collateral":"1000","total_asset_value":"1000","available_balance":"900","positions":[{"market_id":77,"position":"2","sign":1,"avg_entry_price":"100","unrealized_pnl":"0","realized_pnl":"3","initial_margin_fraction":1000,"margin_mode":0}]}]}"#,
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
        r#"{"channel":"account_all:718895","positions":{"1":{"market_id":1,"position":"3","sign":1,"avg_entry_price":"100","margin_mode":0,"initial_margin_fraction":1000}}}"#,
    );
    h.pump();
    assert_eq!(h.open(), 3.0);
    h.answer(200, FLAT);
    assert!(h.result(47).is_none(), "the frame held that read");
    assert_eq!(h.open(), 3.0, "the frame stands");
    let actions = advance(&mut h.e, &mut h.timers, NOW + 500.0);
    h.sort(actions);
    assert!(h.has(), "a new read follows");
    h.answer(200, FLAT);
    assert!(h.succeeded(47), "{:?}", h.done);
    assert_eq!(h.open(), 0.0);
}

#[test]
fn before_the_account_resolves_the_command_refuses_without_sending() {
    let mut e = engine(options());
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
