//! `readPositions`: success only after a read issued after the command commits.
#![allow(dead_code)]
mod support;
use safe_cex_engine::{
    Action, Command, ErrorKind, Event, HttpRequest, Notice, Outcome, Response, SocketChannel,
    SocketId, TimerId, VenueAction, VenueError,
};
use safe_cex_venue_apexomni::Options;
use support::*;
const FLAT_ANSWER: &str = r#"{"skipped":[],"open":[]}"#;

const ACCOUNT: &str = r#"{"code":0,"data":{"id":"611779989544632629","l2key":"key","positions":[{"symbol":"BTC-USDT","side":"LONG","entryPrice":"100","size":"2","customImr":"0.1","unrealisedPnl":"4","curRealisedPnl":"3","liqPrice":"50"}],"contractAccount":{"makerFeeRate":"0.0002","takerFeeRate":"0.0005"}}}"#;
const FLAT: &str = r#"{"code":0,"data":{"id":"611779989544632629","positions":[],"contractAccount":{"makerFeeRate":"0.0002","takerFeeRate":"0.0005"}}}"#;
const EMPTY_ORDERS: &str = r#"{"code":0,"data":[]}"#;
const POSITIONS: &str = "/v3/account";

struct H {
    e: TestEngine,
    socket: SocketId,
    now: f64,
    http: Vec<HttpRequest>,
    timers: Vec<(TimerId, f64)>,
    done: Vec<(u32, Result<String, VenueError>)>,
}

fn started() -> H {
    let mut e = private_engine(Options::default(), &Material::default());
    e.start(0.0);
    let r = requests(&mut e, 0.0).remove(0);
    response(&mut e, &r, 10.0, MARKETS);
    let mut h = H {
        e,
        socket: SocketId(0),
        now: 10.0,
        http: Vec::new(),
        timers: Vec::new(),
        done: Vec::new(),
    };
    h.pump();
    h.e.handle(20.0, Event::WsOpened { socket: h.socket });
    h.now = 30.0;
    h.pump();
    h.answer(POSITIONS, ACCOUNT);
    h.answer("/v3/open-orders", EMPTY_ORDERS);
    h.http.clear();
    h.now = 5_000.0;
    h
}

impl H {
    fn pump(&mut self) {
        for action in self.e.prepared(self.now) {
            match action {
                Action::Http(request) => self.http.push(request),
                Action::WsConnect {
                    socket,
                    channel: Some(SocketChannel::Private),
                    ..
                } => self.socket = socket,
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
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
            self.now,
            Event::Command(Command::Venue {
                command,
                action: VenueAction::ReadPositions,
            }),
        );
        self.pump();
    }
    fn has(&self) -> bool {
        self.http.iter().any(|r| r.url.ends_with(POSITIONS))
    }
    fn take(&mut self, part: &str) -> HttpRequest {
        let index = self
            .http
            .iter()
            .position(|r| r.url.ends_with(part))
            .unwrap_or_else(|| panic!("a request for {part}"));
        self.http.remove(index)
    }
    fn reply(&mut self, request: &HttpRequest, status: u16, body: &str) {
        self.e.handle(
            self.now,
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
    fn answer(&mut self, part: &str, body: &str) {
        let request = self.take(part);
        self.reply(&request, 200, body);
    }
    fn result(&self, id: u32) -> Option<&Result<String, VenueError>> {
        self.done.iter().find(|(c, _)| *c == id).map(|(_, r)| r)
    }
    fn succeeded(&self, id: u32) -> bool {
        self.result(id).is_some_and(|r| {
            r.as_deref()
                .is_ok_and(|b| b.starts_with(r#"{"skipped":[],"open":["#))
        })
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
    h.answer(POSITIONS, FLAT);
    assert!(h.succeeded(40), "{:?}", h.done);
    assert_eq!(h.open(), 0.0, "the store is flat");
}

#[test]
fn a_read_in_flight_is_followed_by_a_new_one() {
    let mut h = started();
    h.read(39);
    let stale = h.take(POSITIONS);
    h.read(41);
    assert!(!h.has(), "no second read yet");
    h.reply(&stale, 200, ACCOUNT);
    assert!(h.succeeded(39), "its own read answers it");
    assert!(h.result(41).is_none(), "the older read cannot");
    assert_eq!(h.open(), 2.0);
    assert!(h.has(), "a new read follows");
    h.answer(POSITIONS, FLAT);
    assert!(h.succeeded(41), "{:?}", h.done);
    assert_eq!(h.open(), 0.0);
}

#[test]
fn a_failed_read_fails_the_command_with_the_venues_error() {
    let mut h = started();
    h.read(42);
    h.answer(POSITIONS, r#"{"code":10001,"msg":"Invalid request"}"#);
    let error = h.failure(42);
    assert!(error.message.contains("Invalid request"), "{error:?}");
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert_eq!(h.open(), 2.0, "the store stands");
}

#[test]
fn a_lost_read_is_resent_under_its_native_budget() {
    let mut h = started();
    h.read(43);
    let request = h.take(POSITIONS);
    h.e.handle(
        h.now,
        Event::HttpFailed {
            request: request.id,
            error: "connection lost",
        },
    );
    h.pump();
    // The native budget retries a network loss, the same read number resent.
    assert!(h.result(43).is_none());
    assert!(h.has(), "resent");
    h.answer(POSITIONS, FLAT);
    assert!(h.succeeded(43), "{:?}", h.done);
}

#[test]
fn a_malformed_reply_or_an_unmappable_row_fails_the_command() {
    let mut h = started();
    h.read(44);
    h.answer(POSITIONS, r#"{"code":0,"data":{"positions":[]}}"#);
    assert_eq!(h.failure(44).outcome, Some(Outcome::NotSent));

    let mut h = started();
    h.read(45);
    h.answer(
        POSITIONS,
        r#"{"code":0,"data":{"id":"611779989544632629","positions":[{"symbol":"BTC-USDT","side":"SIDEWAYS","size":"2"}]}}"#,
    );
    assert!(
        h.failure(45).message.contains("could not be mapped"),
        "{:?}",
        h.done
    );
    assert_eq!(h.failure(45).outcome, Some(Outcome::NotSent));
    assert_eq!(h.open(), 2.0, "the rejected row's stored copy stands");
}

#[test]
fn a_read_a_newer_stream_frame_held_is_not_a_commit() {
    let mut h = started();
    h.read(46);
    let frame = r#"{"topic":"ws_zk_accounts_v3","type":"delta","contents":{"positions":[{"symbol":"BTC-USDT","side":"LONG","entryPrice":"100","size":"3","customImr":"0.1"}]}}"#;
    ws(&mut h.e, h.socket, h.now, frame);
    h.pump();
    assert_eq!(h.open(), 3.0);
    h.answer(POSITIONS, FLAT);
    assert!(h.result(46).is_none(), "the frame held that read");
    assert_eq!(h.open(), 3.0, "the frame stands");
    let heal = h
        .timers
        .iter()
        .rfind(|(_, at)| *at <= h.now + 1000.0)
        .copied()
        .expect("a heal is due");
    h.now = heal.1;
    h.e.handle(h.now, Event::TimerFired { timer: heal.0 });
    h.pump();
    assert!(h.has(), "a new read follows");
    h.answer(POSITIONS, FLAT);
    assert!(h.succeeded(46), "{:?}", h.done);
    assert_eq!(h.open(), 0.0);
}

#[test]
fn a_public_engine_refuses_without_sending() {
    let (mut e, _) = started_public();
    e.handle(
        30.0,
        Event::Command(Command::Venue {
            command: 60,
            action: VenueAction::ReadPositions,
        }),
    );
    let out: Vec<_> = e.prepared(30.0).collect();
    assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 60, error }) if error.kind != Some(ErrorKind::Unsupported) && error.outcome == Some(Outcome::NotSent))), "{out:?}");
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
}

fn started_public() -> (TestEngine, SocketId) {
    support::started()
}

#[test]
fn an_owned_row_on_an_unknown_market_fails_and_a_flat_one_does_not() {
    let row = |size: &str| {
        format!(
            r#"{{"code":0,"data":{{"id":"611779989544632629","positions":[{{"symbol":"DOGE-USDT","side":"LONG","entryPrice":"1","size":"{size}","customImr":"0.1"}}]}}}}"#
        )
    };
    let mut h = started();
    h.read(70);
    h.answer(POSITIONS, &row("2"));
    assert!(h.failure(70).message.contains("could not be mapped"));
    let mut h = started();
    h.read(71);
    h.answer(POSITIONS, &row("0"));
    assert!(h.succeeded(71), "{:?}", h.done);
}
