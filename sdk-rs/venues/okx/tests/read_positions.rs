//! `readPositions`: success only after a read issued after the command commits.
#![allow(dead_code)]
mod support;
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, Notice, Outcome, SocketId, TimerId, VenueAction,
    VenueError,
};
use safe_cex_venue_okx::Okx;
use support::FixtureOptions as Options;
use support::*;

const POSITION: &str = r#"{"instId":"BTC-USDT-SWAP","pos":"5","posSide":"net","avgPx":"39000","lever":"10","upl":"50","liqPx":"30000","mgnMode":"cross","imr":"200","mmr":"20"}"#;
const RISK: &str = r#"{"balData":[{"ccy":"USDT","eq":"5000"}]}"#;
const POSITIONS: &str = "/account/positions?";
const RISK_PATH: &str = "account-position-risk";

struct H {
    e: Engine<Okx>,
    http: Vec<Action>,
    sockets: Vec<(SocketId, String)>,
    timers: Vec<(TimerId, f64)>,
    done: Vec<(u32, Result<String, VenueError>)>,
    now: f64,
}

impl H {
    fn pump(&mut self) {
        for action in prepared(&mut self.e, self.now) {
            match action {
                Action::Http(_) => self.http.push(action),
                Action::WsConnect { socket, url, .. } => self.sockets.push((socket, url)),
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
    /// Move to `at`, firing every timer due by then.
    fn advance(&mut self, at: f64) {
        self.now = at;
        self.pump();
        let due: Vec<_> = self
            .timers
            .iter()
            .filter(|(_, t)| *t <= at)
            .copied()
            .collect();
        self.timers.retain(|(_, t)| *t > at);
        for (timer, when) in due {
            self.e.handle(when, Event::TimerFired { timer });
            self.pump();
        }
    }
    fn has(&self, part: &str) -> bool {
        self.http
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains(part)))
    }
    fn take(&mut self, part: &str) -> Action {
        let index = self
            .http
            .iter()
            .position(|a| matches!(a, Action::Http(r) if r.url.contains(part)))
            .unwrap_or_else(|| panic!("a request for {part}"));
        self.http.remove(index)
    }
    fn answer(&mut self, part: &str, body: &str) {
        let action = self.take(part);
        self.reply(&action, body);
    }
    fn reply(&mut self, action: &Action, body: &str) {
        self.now += 1.0;
        reply(&mut self.e, action, body, self.now);
        self.pump();
    }
    /// Every read out, the whole cycle, with `positions` on the first page of each.
    fn read_all(&mut self, positions: &str) {
        self.answer(RISK_PATH, &envelope(RISK));
        self.answer("instType=SWAP", &envelope(positions));
        if self.has("instType=FUTURES") {
            self.answer("instType=FUTURES", &envelope(""));
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

/// Discovery and the first account cycle answered, flat, with no read left out.
fn started(options: Options) -> H {
    let mut h = H {
        e: support::engine(Options {
            key: Some("test-key".into()),
            secret: Some("test-secret".into()),
            passphrase: Some("test-passphrase".into()),
            ..options
        }),
        http: Vec::new(),
        sockets: Vec::new(),
        timers: Vec::new(),
        done: Vec::new(),
        now: 0.0,
    };
    h.e.start(0.0);
    h.pump();
    while let Some(action) = h.http.pop() {
        let Action::Http(request) = &action else {
            continue;
        };
        let url = request.url.as_str();
        let body = if url.contains("instruments?instType=SWAP") {
            envelope(MARKET)
        } else if url.contains("instruments?instType=FUTURES") {
            envelope(&MARKET.replace("BTC-USDT-SWAP", "BTC-USDT-261225").replace(
                "\"instType\":\"SWAP\"",
                "\"instType\":\"FUTURES\",\"ruleType\":\"xperp\",\"uly\":\"BTC-USDT\"",
            ))
        } else if url.contains("tickers?instType=SWAP") {
            envelope(TICKER)
        } else if url.contains("/account/config") {
            envelope(r#"{"posMode":"net_mode","acctLv":"2"}"#)
        } else if url.contains(RISK_PATH) {
            envelope(RISK)
        } else {
            envelope("")
        };
        h.reply(&action, &body);
    }
    assert!(h.e.store().loaded().positions);
    h
}

type Case = (&'static str, fn() -> Options);
const CASES: [Case; 3] = [
    ("live", Options::default),
    ("demo", || Options {
        testnet: true,
        ..Options::default()
    }),
    ("eu", || Options {
        eea: true,
        ..Options::default()
    }),
];

#[test]
fn a_read_issued_after_the_command_commits_then_it_succeeds() {
    for (name, options) in CASES {
        let mut h = started(options());
        assert_eq!(h.open(), 0.0);
        h.read(40);
        assert!(h.has(POSITIONS), "{name}: a read is issued");
        assert!(h.has(RISK_PATH), "{name}: with its risk half");
        assert!(h.result(40).is_none(), "{name}");
        h.read_all(POSITION);
        assert!(h.succeeded(40), "{name}: {:?}", h.done);
        assert_eq!(h.open(), 0.05, "{name}: the store row is updated");
    }
}

#[test]
fn a_read_in_flight_is_followed_by_a_new_one() {
    for (name, options) in CASES {
        let mut h = started(options());
        h.read(39);
        let stale_risk = h.take(RISK_PATH);
        let stale = h.take("instType=SWAP");
        h.read(41);
        assert!(!h.has(POSITIONS), "{name}: no second read yet");
        h.reply(&stale_risk, &envelope(RISK));
        h.reply(&stale, &envelope(POSITION));
        if h.has("instType=FUTURES") {
            h.answer("instType=FUTURES", &envelope(""));
        }
        assert!(h.succeeded(39), "{name}: its own read answers it");
        assert!(h.result(41).is_none(), "{name}: the older read cannot");
        assert_eq!(h.open(), 0.05);
        assert!(h.has(POSITIONS), "{name}: a new read follows");
        h.read_all("");
        assert!(h.succeeded(41), "{name}: {:?}", h.done);
        assert_eq!(h.open(), 0.0, "{name}");
    }
}

#[test]
fn a_failed_read_fails_the_command_with_the_venues_error() {
    let refused = r#"{"code":"50030","msg":"No permission to use this API","data":[]}"#;
    for (name, options) in CASES {
        for leg in ["instType=SWAP", RISK_PATH] {
            let mut h = started(options());
            h.read(42);
            h.answer(leg, refused);
            let error = h.failure(42);
            assert!(error.message.contains("No permission"), "{name}: {error:?}");
            assert_eq!(error.venue_code.as_deref(), Some("50030"), "{name}");
            assert_eq!(error.outcome, Some(Outcome::NotSent), "{name}");
        }
    }
}

#[test]
fn a_lost_read_fails_a_waiter_and_a_later_one_gets_a_new_read() {
    let mut h = started(Options::default());
    h.read(43);
    let lost = h.take(POSITIONS);
    h.read(44);
    let Action::Http(request) = &lost else {
        unreachable!()
    };
    h.e.handle(
        h.now,
        Event::HttpFailed {
            request: request.id,
            error: "connection lost",
        },
    );
    h.pump();
    assert_eq!(h.failure(43).outcome, Some(Outcome::NotSent));
    assert!(h.result(44).is_none(), "the lost read predates it");
    assert!(h.has(POSITIONS), "a new read follows");
    h.answer(RISK_PATH, &envelope(RISK));
    h.answer(RISK_PATH, &envelope(RISK));
    h.answer("instType=SWAP", &envelope(POSITION));
    assert!(h.succeeded(44), "{:?}", h.done);
}

#[test]
fn an_unmappable_row_or_a_malformed_reply_fails_the_command() {
    let mut h = started(Options::default());
    h.read(45);
    h.answer(RISK_PATH, &envelope(RISK));
    h.answer(
        "instType=SWAP",
        &envelope(r#"{"instId":"BTC-USDT-SWAP","pos":"nope"}"#),
    );
    assert!(h.failure(45).message.contains("could not be mapped"));
    assert_eq!(h.failure(45).outcome, Some(Outcome::NotSent));

    let mut h = started(Options::default());
    h.read(46);
    h.answer("instType=SWAP", r#"{"code":"0","data":{"not":"a list"}}"#);
    assert_eq!(h.failure(46).outcome, Some(Outcome::NotSent));
}

#[test]
fn a_read_a_newer_stream_frame_held_is_not_a_commit() {
    let mut h = started(Options::default());
    let private = h
        .sockets
        .iter()
        .find(|(_, url)| url.ends_with("/private"))
        .expect("the private socket")
        .0;
    h.e.handle(h.now, Event::WsOpened { socket: private });
    h.pump();
    ws(
        &mut h.e,
        private,
        r#"{"event":"login","code":"0","msg":""}"#,
    );
    h.advance(h.now);
    while h.has(POSITIONS) || h.has(RISK_PATH) {
        h.read_all("");
    }
    h.http.clear();
    h.read(47);
    let frame = format!(r#"{{"arg":{{"channel":"positions"}},"data":[{POSITION}]}}"#);
    ws(&mut h.e, private, &frame);
    h.pump();
    assert_eq!(h.open(), 0.05);
    h.read_all("");
    assert!(h.result(47).is_none(), "the frame held that read");
    assert_eq!(h.open(), 0.05, "the frame stands");
    h.advance(h.now + 500.0);
    assert!(h.has(POSITIONS), "a new read follows");
    h.read_all("");
    assert!(h.succeeded(47), "{:?}", h.done);
    assert_eq!(h.open(), 0.0);
}

#[test]
fn before_startup_loads_the_command_refuses_without_sending() {
    let mut e = support::engine(Options::default());
    e.handle(
        0.0,
        Event::Command(Command::Venue {
            command: 60,
            action: VenueAction::ReadPositions,
        }),
    );
    let refused = prepared(&mut e, 0.0).into_iter().find_map(|a| match a {
        Action::Notice(Notice::CommandFailed { command: 60, error }) => Some(error),
        _ => None,
    });
    let refused = refused.expect("a refusal");
    assert_ne!(refused.kind, Some(ErrorKind::Unsupported));
    assert_eq!(refused.outcome, Some(Outcome::NotSent));
}

#[test]
fn an_owned_row_outside_the_catalogue_is_named_and_a_flat_one_is_not() {
    for (name, options) in CASES {
        let unlisted = POSITION.replace("BTC-USDT-SWAP", "DOGE-USDT-SWAP");
        let mut h = started(options());
        h.read(70);
        h.read_all(&unlisted);
        assert_eq!(
            h.result(70)
                .map(|r| r.as_deref().map_err(|e| e.message.as_str())),
            Some(Ok(r#"["DOGE-USDT-SWAP"]"#)),
            "{name}"
        );
        assert_eq!(h.open(), 0.0, "{name}: the store is unchanged");

        let flat = unlisted.replace("\"pos\":\"5\"", "\"pos\":\"0\"");
        let mut h = started(options());
        h.read(71);
        h.read_all(&flat);
        assert!(h.succeeded(71), "{name}: {:?}", h.done);
    }
}
