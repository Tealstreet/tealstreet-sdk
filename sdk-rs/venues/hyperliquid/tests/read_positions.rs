//! `readPositions`: success only after a read issued after the command commits.
#![allow(dead_code)]
mod support;
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Notice, Outcome, SocketId, TimerId,
    VenueAction, VenueError,
};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options};
use support::*;
const FLAT_ANSWER: &str = r#"{"skipped":[],"open":[]}"#;

const FLAT: &str = r#"{"assetPositions":[],"marginSummary":{"accountValue":"1000","totalMarginUsed":"0"},"time":2000}"#;
const LATER_FLAT: &str = r#"{"assetPositions":[],"marginSummary":{"accountValue":"1000","totalMarginUsed":"0"},"time":9000}"#;
const LATER_ROW: &str = r#"{"assetPositions":[{"position":{"coin":"BTC","szi":"3","entryPx":"100","positionValue":"300","unrealizedPnl":"0","liquidationPx":"50","marginUsed":"30","leverage":{"type":"cross","value":10}}}],"marginSummary":{"accountValue":"1000","totalMarginUsed":"30"},"time":3000}"#;
const STATE_READ: &str = "clearinghouseState";

struct H {
    e: Engine<Hyperliquid>,
    socket: SocketId,
    http: Vec<HttpRequest>,
    timers: Vec<(TimerId, f64)>,
    done: Vec<(u32, Result<String, VenueError>)>,
}

impl H {
    fn new((e, socket): (Engine<Hyperliquid>, SocketId)) -> Self {
        let mut h = Self {
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
    fn pump(&mut self) {
        for action in prepared_now(&mut self.e) {
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
            1000.0,
            Event::Command(Command::Venue {
                command,
                action: VenueAction::ReadPositions,
            }),
        );
        self.pump();
    }
    fn states(&self) -> usize {
        self.http.iter().filter(|r| kind(r) == STATE_READ).count()
    }
    fn take(&mut self, dex: bool) -> HttpRequest {
        let index = self
            .http
            .iter()
            .position(|r| {
                kind(r) == STATE_READ && r.body.as_deref().unwrap().contains("\"dex\"") == dex
            })
            .expect("a state read");
        self.http.remove(index)
    }
    fn answer(&mut self, body: &str) {
        let request = self.take(false);
        self.reply(&request, 200, body);
    }
    fn reply(&mut self, request: &HttpRequest, status: u16, body: &str) {
        reply(&mut self.e, request, status, body);
        self.pump();
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
    /// Fire every timer due within 500 ms of its arming, the retry among them.
    fn fire_retry(&mut self) {
        let due: Vec<_> = self
            .timers
            .iter()
            .filter(|(_, at)| *at <= 3000.0)
            .copied()
            .collect();
        self.timers.retain(|(_, at)| *at > 3000.0);
        for (timer, at) in due {
            self.e.handle(at, Event::TimerFired { timer });
            self.pump();
        }
    }
}

#[test]
fn a_read_issued_after_the_command_commits_then_it_succeeds() {
    let mut h = H::new(ready("disabled"));
    assert_eq!(h.open(), 2.0);
    h.read(40);
    assert_eq!(h.states(), 1, "a read is issued");
    assert!(h.result(40).is_none());
    h.answer(FLAT);
    assert!(h.succeeded(40), "{:?}", h.done);
    assert_eq!(h.open(), 0.0, "the store is flat");
}

#[test]
fn every_dex_must_commit_its_own_read() {
    let mut h = H::new(ready_with_dex("disabled", true));
    h.read(40);
    assert_eq!(h.states(), 2);
    h.answer(FLAT);
    assert!(h.result(40).is_none(), "the other dex has not answered");
    let other = h.take(true);
    h.reply(&other, 200, r#"{"assetPositions":[],"time":2000}"#);
    assert!(h.succeeded(40), "{:?}", h.done);
}

#[test]
fn a_read_in_flight_is_followed_by_a_new_one() {
    let mut h = H::new(ready("disabled"));
    h.read(39);
    let stale = h.take(false);
    h.read(41);
    assert_eq!(h.states(), 0, "no second read yet");
    h.reply(&stale, 200, STATE);
    assert!(h.succeeded(39), "its own read answers it");
    assert!(h.result(41).is_none(), "the older read cannot");
    assert_eq!(h.open(), 2.0);
    assert_eq!(h.states(), 1, "a new read follows");
    h.answer(FLAT);
    assert!(h.succeeded(41), "{:?}", h.done);
    assert_eq!(h.open(), 0.0);
}

#[test]
fn a_failed_read_fails_the_command_with_the_venues_error() {
    let mut h = H::new(ready("disabled"));
    h.read(42);
    let request = h.take(false);
    h.reply(&request, 429, "");
    let error = h.failure(42);
    assert!(error.message.contains("429"), "{error:?}");
    assert_eq!(error.kind, Some(ErrorKind::RateLimited));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert_eq!(h.open(), 2.0, "the store stands");
}

#[test]
fn a_lost_read_keeps_the_engines_typed_cause() {
    let mut h = H::new(ready("disabled"));
    h.read(43);
    let request = h.take(false);
    h.e.handle(
        1000.0,
        Event::HttpFailed {
            request: request.id,
            error: "connection lost",
        },
    );
    h.pump();
    let error = h.failure(43);
    assert_eq!(error.kind, Some(ErrorKind::Network), "{error:?}");
    assert_eq!(error.outcome, Some(Outcome::NotSent));
}

#[test]
fn a_malformed_reply_or_an_unmappable_row_fails_the_command() {
    let mut h = H::new(ready("disabled"));
    h.read(44);
    h.answer("not json");
    assert_eq!(h.failure(44).outcome, Some(Outcome::NotSent));

    let mut h = H::new(ready("disabled"));
    h.read(45);
    h.answer(r#"{"assetPositions":[{"position":{"coin":"BTC","szi":"nope"}}],"time":2000}"#);
    assert!(h.failure(45).message.contains("could not be mapped"));
    assert_eq!(h.failure(45).outcome, Some(Outcome::NotSent));
    assert_eq!(h.open(), 2.0, "the rejected row's stored copy stands");
}

#[test]
fn a_read_a_newer_stream_frame_held_is_not_a_commit() {
    let mut h = H::new(ready("disabled"));
    h.read(46);
    message(
        &mut h.e,
        h.socket,
        1000.0,
        &format!(
            r#"{{"channel":"clearinghouseState","data":{{"user":"x","dex":"","clearinghouseState":{LATER_ROW}}}}}"#
        ),
    );
    h.pump();
    assert_eq!(h.open(), 3.0);
    h.answer(FLAT);
    assert!(h.result(46).is_none(), "the frame's time beats this read");
    assert_eq!(h.open(), 3.0, "the frame stands");
    h.fire_retry();
    assert_eq!(h.states(), 1, "a new read follows");
    h.answer(LATER_FLAT);
    assert!(h.succeeded(46), "{:?}", h.done);
    assert_eq!(h.open(), 0.0);
}

#[test]
fn spot_and_a_public_engine_send_nothing() {
    let mut spot = private_engine(
        Options {
            key: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
            ..Options::default()
        },
        TEST_SECRET.to_owned(),
        true,
    );
    spot.handle(
        0.0,
        Event::Command(Command::Venue {
            command: 60,
            action: VenueAction::ReadPositions,
        }),
    );
    let out = prepared_now(&mut spot);
    assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 60, error }) if error.kind == Some(ErrorKind::Unsupported) && error.outcome == Some(Outcome::NotSent))), "{out:?}");

    let mut public =
        safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options::default()));
    public.handle(
        0.0,
        Event::Command(Command::Venue {
            command: 61,
            action: VenueAction::ReadPositions,
        }),
    );
    let out = prepared_now(&mut public);
    assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 61, error }) if error.kind != Some(ErrorKind::Unsupported) && error.outcome == Some(Outcome::NotSent))), "{out:?}");
}

const HIDDEN_META: &str = r#"[{"universe":[{"name":"xyz:FOO","szDecimals":2,"maxLeverage":10},{"name":"xyz:OLD","szDecimals":2,"maxLeverage":10}]},[{"midPx":"50","markPx":"50","oraclePx":"50"},{"midPx":null,"markPx":"50","oraclePx":"50"}]]"#;

fn hidden_row(size: &str) -> String {
    format!(
        r#"{{"assetPositions":[{{"position":{{"coin":"xyz:OLD","szi":"{size}","entryPx":"50","positionValue":"100","unrealizedPnl":"0","marginUsed":"10","leverage":{{"type":"cross","value":10}}}}}}],"time":2000}}"#
    )
}

#[test]
fn an_owned_row_on_a_hidden_coin_is_named_and_a_flat_one_is_not() {
    let mut h = H::new(ready_with_hip3_meta(HIDDEN_META));
    h.read(70);
    h.answer(FLAT);
    let other = h.take(true);
    h.reply(&other, 200, &hidden_row("2"));
    assert_eq!(
        h.result(70)
            .map(|r| r.as_deref().map_err(|e| e.message.as_str())),
        Some(Ok(r#"{"skipped":["xyz:OLD"],"open":[]}"#))
    );

    let mut h = H::new(ready_with_hip3_meta(HIDDEN_META));
    h.read(71);
    h.answer(FLAT);
    let other = h.take(true);
    h.reply(&other, 200, &hidden_row("0"));
    assert!(h.succeeded(71), "{:?}", h.done);
}
