//! `readPositions`: success only after a read issued after the command commits.
#![allow(dead_code)]
mod support;
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Notice, Outcome, Response, SocketId,
    TimerId, VenueAction, VenueError,
};
use safe_cex_venue_asterdex::AsterDex;
use support::Options;

const MARKETS: &str = r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","pricePrecision":2,"quantityPrecision":3,"filters":[{"filterType":"LOT_SIZE","minQty":"0.001","maxQty":"1000"}]}]}"#;
const BALANCE: &str = r#"{"totalWalletBalance":"1000","availableBalance":"800","totalInitialMargin":"100","totalOpenOrderInitialMargin":"20","totalUnrealizedProfit":"5"}"#;
const ROW: &str = r#"[{"symbol":"BTCUSDT","positionSide":"BOTH","positionAmt":"1","entryPrice":"100","unRealizedProfit":"5","liquidationPrice":"20","notional":"100","leverage":"10","marginType":"cross"}]"#;
const RISK: &str = "/positionRisk";

struct H {
    e: Engine<AsterDex>,
    now: f64,
    http: Vec<HttpRequest>,
    socket: Option<SocketId>,
    timers: Vec<(TimerId, f64)>,
    done: Vec<(u32, Result<String, VenueError>)>,
}

fn started() -> H {
    let mut h = H {
        e: support::engine(Options {
            key: Some("documented-key".into()),
            secret: Some("synthetic-secret".into()),
            ..Default::default()
        }),
        now: 1700000000000.0,
        http: Vec::new(),
        socket: None,
        timers: Vec::new(),
        done: Vec::new(),
    };
    h.e.start(h.now);
    h.pump();
    h.answer("/time", 200, r#"{"serverTime":1700000000000}"#);
    h.answer("/exchangeInfo", 200, MARKETS);
    h.answer("/leverageBracket", 200, "[]");
    h.answer(
        "/ticker/24hr",
        200,
        r#"[{"symbol":"BTCUSDT","lastPrice":"100","priceChangePercent":"0","volume":"1","quoteVolume":"100"}]"#,
    );
    h.answer("/openInterest", 200, "[]");
    h.answer("/account", 200, BALANCE);
    h.answer(RISK, 200, ROW);
    h.answer("/openOrders", 200, "[]");
    h.answer("/listenKey", 200, r#"{"listenKey":"private-contract"}"#);
    let socket = h.socket.expect("the private socket");
    h.e.handle(h.now, Event::WsOpened { socket });
    h.pump();
    h.answer("/openOrders", 200, "[]");
    h.answer(RISK, 200, ROW);
    h.answer("/positionSide/dual", 200, r#"{"dualSidePosition":false}"#);
    h.http.clear();
    h
}

impl H {
    fn pump(&mut self) {
        for action in support::drain(&mut self.e, self.now) {
            match action {
                Action::Http(request) => self.http.push(request),
                Action::WsConnect { socket, url, .. } if url.contains("/ws/") => {
                    self.socket = Some(socket);
                }
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
        self.http.iter().any(|r| r.url.contains(RISK))
    }
    fn take(&mut self, suffix: &str) -> HttpRequest {
        let index = self
            .http
            .iter()
            .position(|r| r.url.split('?').next().unwrap().ends_with(suffix))
            .unwrap_or_else(|| panic!("a request for {suffix}: {:?}", self.http));
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
    fn answer(&mut self, suffix: &str, status: u16, body: &str) {
        let request = self.take(suffix);
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
    assert_eq!(h.open(), 1.0);
    h.read(40);
    assert!(h.has(), "a read is issued");
    assert!(h.result(40).is_none());
    h.answer(RISK, 200, "[]");
    assert!(h.succeeded(40), "{:?}", h.done);
    assert_eq!(h.open(), 0.0, "the store is flat");
}

#[test]
fn a_read_in_flight_is_followed_by_a_new_one() {
    let mut h = started();
    h.read(39);
    let stale = h.take(RISK);
    h.read(41);
    assert!(!h.has(), "no second read yet");
    h.reply(&stale, 200, ROW);
    assert!(h.succeeded(39), "its own read answers it");
    assert!(h.result(41).is_none(), "the older read cannot");
    assert_eq!(h.open(), 1.0);
    assert!(h.has(), "a new read follows");
    h.answer(RISK, 200, "[]");
    assert!(h.succeeded(41), "{:?}", h.done);
    assert_eq!(h.open(), 0.0);
}

#[test]
fn a_failed_read_fails_the_command_with_the_venues_error() {
    let mut h = started();
    h.read(42);
    h.answer(RISK, 200, r#"{"code":-2015,"msg":"Invalid API-key"}"#);
    let error = h.failure(42);
    assert!(error.message.contains("Invalid API-key"), "{error:?}");
    assert_eq!(error.kind, Some(ErrorKind::AuthInvalid));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert_eq!(h.open(), 1.0, "the store stands");
}

#[test]
fn a_rejected_row_or_a_missing_list_fails_the_command() {
    let mut h = started();
    h.read(43);
    h.answer(
        RISK,
        200,
        r#"[{"symbol":"BTCUSDT","positionSide":"BOTH","positionAmt":"nope"}]"#,
    );
    assert!(
        h.failure(43).message.contains("could not be mapped"),
        "{:?}",
        h.done
    );
    assert_eq!(h.failure(43).outcome, Some(Outcome::NotSent));
    assert_eq!(h.open(), 1.0, "the rejected row's stored copy stands");

    let mut h = started();
    h.read(44);
    h.answer(RISK, 200, r#"{"unexpected":"envelope"}"#);
    assert_eq!(h.failure(44).outcome, Some(Outcome::NotSent));
}

#[test]
fn a_read_a_newer_stream_event_held_is_not_a_commit() {
    let mut h = started();
    h.read(45);
    let text = r#"{"e":"ACCOUNT_UPDATE","E":1700000001000,"a":{"B":[{"a":"USDT","wb":"1010","cw":"900"}],"P":[{"s":"BTCUSDT","pa":"3","ep":"101","up":"7","ps":"BOTH","cr":"3"}]}}"#;
    let socket = h.socket.unwrap();
    h.e.handle(h.now, Event::WsMessage { socket, text });
    h.pump();
    assert_eq!(h.open(), 3.0);
    h.answer(RISK, 200, "[]");
    assert!(h.result(45).is_none(), "the event held that read");
    assert_eq!(h.open(), 3.0, "the event stands");
    let retry = h
        .timers
        .iter()
        .copied()
        .find(|(_, at)| *at == h.now + 1_000.0);
    let (timer, at) = retry.expect("a retry is armed");
    h.now = at;
    h.e.handle(h.now, Event::TimerFired { timer });
    h.pump();
    assert!(h.has(), "a new read follows");
    h.answer(RISK, 200, "[]");
    assert!(h.succeeded(45), "{:?}", h.done);
    assert_eq!(h.open(), 0.0);
}

#[test]
fn a_public_engine_refuses_without_sending() {
    let mut e = support::engine(Options::default());
    e.handle(
        0.0,
        Event::Command(Command::Venue {
            command: 60,
            action: VenueAction::ReadPositions,
        }),
    );
    let out = support::drain(&mut e, 0.0);
    assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 60, error }) if error.kind != Some(ErrorKind::Unsupported) && error.outcome == Some(Outcome::NotSent))), "{out:?}");
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
}

#[test]
fn a_row_on_an_unknown_market_commits_to_the_store_rather_than_being_dropped() {
    let mut h = started();
    h.read(70);
    h.answer(RISK, 200, &ROW.replace("BTCUSDT", "DOGEUSDT"));
    assert!(h.succeeded(70), "{:?}", h.done);
    assert_eq!(h.open(), 1.0, "the row is shown, not skipped");
}
