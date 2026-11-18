//! `readPositions`: success only after a read issued after the command commits.
#![allow(dead_code)]
mod support;
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Notice, Outcome, Response, SocketId,
    TimerId, VenueAction, VenueError,
};
use safe_cex_venue_bingx::Bingx;
use support::Options;
const FLAT_ANSWER: &str = r#"{"skipped":[],"open":[]}"#;

#[test]
fn a_rate_limited_positions_command_keeps_its_kind_delay_and_snapshot() {
    for (status, headers, expected) in [(200, "", 90_000.0), (429, "Retry-After: 120", 120_000.0)] {
        let mut h = started();
        h.read(77);
        let request = h.take(POSITIONS);
        let body = format!(
            r#"{{"code":100410,"msg":"请稍后重试; unblocked after {:013.0}"}}"#,
            h.now + 90_000.0
        );
        h.e.handle(
            h.now,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status,
                    headers,
                    body: &body,
                },
            },
        );
        h.pump();
        let error = h.failure(77);
        assert_eq!(error.kind, Some(ErrorKind::RateLimited));
        assert_eq!(error.retry_after_ms, Some(expected));
        assert_eq!(error.venue_code.as_deref(), Some("100410"));
        assert!(!error.message.contains("expected list"));
        assert_eq!(h.open(), 0.1);
    }
}

#[test]
fn a_later_transport_failure_cannot_inherit_an_old_rate_limit() {
    let mut h = started();
    h.read(78);
    h.answer(POSITIONS, 200, r#"{"code":100410,"msg":"请稍后重试"}"#);
    assert_eq!(h.failure(78).kind, Some(ErrorKind::RateLimited));
    h.read(79);
    let request = h.take(POSITIONS);
    h.e.handle(
        h.now,
        Event::HttpFailed {
            request: request.id,
            error: "connection lost",
        },
    );
    h.pump();
    assert_eq!(h.failure(79).kind, Some(ErrorKind::Network));
    assert_eq!(h.failure(79).retry_after_ms, None);
}

const KEY: &str = "/openApi/user/auth/userDataStream";
const MODE: &str = "/openApi/swap/v1/positionSide/dual";
const BALANCE: &str = "/openApi/swap/v3/user/balance";
const POSITIONS: &str = "/openApi/swap/v2/user/positions";
const ORDERS: &str = "/openApi/swap/v2/trade/openOrders";
const MARKETS: &str = r#"{"code":0,"data":[{"symbol":"BTC-USDT","asset":"BTC","currency":"USDT","status":1,"quantityPrecision":4,"pricePrecision":1,"tradeMinLimit":"0.0001","maxLongLeverage":125}]}"#;
const DAILY: &str = r#"{"code":0,"data":[{"symbol":"BTC-USDT","lastPrice":"60000","priceChangePercent":"1","volume":"12","quoteVolume":"720000"}]}"#;
const PREMIUM: &str = r#"{"code":0,"data":[{"symbol":"BTC-USDT","markPrice":"60001","indexPrice":"60002","lastFundingRate":"0.01","nextFundingTime":1700000000000}]}"#;
const BALANCES: &str = r#"{"code":0,"data":[{"asset":"USDT","balance":"1000","availableMargin":"950","usedMargin":"50","unrealizedProfit":"10"}]}"#;
const ROW: &str = r#"{"code":0,"data":[{"symbol":"BTC-USDT","positionAmt":"0.1","positionSide":"BOTH","avgPrice":"60000","unrealizedProfit":"10","leverage":20,"isolated":false,"liquidationPrice":50000,"initialMargin":"300"}]}"#;
const FLAT: &str = r#"{"code":0,"data":[]}"#;
const OPEN: &str = r#"{"code":0,"data":{"orders":[]}}"#;

struct H {
    e: Engine<Bingx>,
    now: f64,
    http: Vec<HttpRequest>,
    socket: Option<SocketId>,
    timers: Vec<(TimerId, f64)>,
    done: Vec<(u32, Result<String, VenueError>)>,
}

fn started() -> H {
    let mut h = H {
        e: support::engine(Options {
            key: Some("key".into()),
            secret: Some("secret".into()),
            ..Options::default()
        }),
        now: 1000.0,
        http: Vec::new(),
        socket: None,
        timers: Vec::new(),
        done: Vec::new(),
    };
    h.e.start(h.now);
    h.pump();
    h.answer("/quote/contracts", 200, MARKETS);
    h.answer("/quote/ticker", 200, DAILY);
    h.answer("/quote/premiumIndex", 200, PREMIUM);
    h.answer(KEY, 200, r#"{"listenKey":"key-one"}"#);
    h.answer(MODE, 200, r#"{"code":0,"data":{"dualSidePosition":false}}"#);
    h.answer(POSITIONS, 200, ROW);
    h.answer(BALANCE, 200, BALANCES);
    h.answer(ORDERS, 200, OPEN);
    let socket = h.socket.expect("the private socket");
    h.e.handle(h.now, Event::WsOpened { socket });
    h.pump();
    h.answer(POSITIONS, 200, ROW);
    h.answer(BALANCE, 200, BALANCES);
    h.answer(ORDERS, 200, OPEN);
    h.http.clear();
    h
}

impl H {
    fn pump(&mut self) {
        for action in safe_cex_engine::test_support::prepared_actions(&mut self.e, self.now) {
            match action {
                Action::Http(request) => self.http.push(request),
                Action::WsConnect { socket, url, .. } if url.contains("listenKey=") => {
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
        self.http.iter().any(|r| r.url.contains(POSITIONS))
    }
    fn take(&mut self, part: &str) -> HttpRequest {
        let index = self
            .http
            .iter()
            .position(|r| r.url.contains(part))
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
    fn answer(&mut self, part: &str, status: u16, body: &str) {
        let request = self.take(part);
        self.reply(&request, status, body);
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
    assert_eq!(h.open(), 0.1);
    h.read(40);
    assert!(h.has(), "a read is issued");
    assert!(h.result(40).is_none());
    h.answer(POSITIONS, 200, FLAT);
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
    h.reply(&stale, 200, ROW);
    assert!(h.succeeded(39), "its own read answers it");
    assert!(h.result(41).is_none(), "the older read cannot");
    assert_eq!(h.open(), 0.1);
    assert!(h.has(), "a new read follows");
    h.answer(POSITIONS, 200, FLAT);
    assert!(h.succeeded(41), "{:?}", h.done);
    assert_eq!(h.open(), 0.0);
}

#[test]
fn a_failed_read_fails_the_command_with_the_venues_error() {
    let mut h = started();
    h.read(42);
    h.answer(
        POSITIONS,
        200,
        r#"{"code":100413,"msg":"Incorrect apiKey"}"#,
    );
    let error = h.failure(42);
    assert!(error.message.contains("Incorrect apiKey"), "{error:?}");
    assert_eq!(error.kind, Some(ErrorKind::AuthInvalid), "{error:?}");
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert_eq!(h.open(), 0.1, "the store stands");

    let mut h = started();
    h.read(43);
    h.answer(POSITIONS, 429, r#"{"code":100410,"msg":"rate limited"}"#);
    assert_eq!(h.failure(43).kind, Some(ErrorKind::RateLimited));
}

#[test]
fn a_lost_read_keeps_the_engines_typed_cause() {
    let mut h = started();
    h.read(44);
    let request = h.take(POSITIONS);
    h.e.handle(
        h.now,
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
    h.answer(POSITIONS, 200, "not json");
    assert_eq!(h.failure(45).outcome, Some(Outcome::NotSent));

    let mut h = started();
    h.read(46);
    h.answer(
        POSITIONS,
        200,
        r#"{"code":0,"data":[{"symbol":"BTC-USDT","positionAmt":"0.1","positionSide":"BOTH","leverage":0}]}"#,
    );
    assert!(
        h.failure(46).message.contains("could not be mapped"),
        "{:?}",
        h.done
    );
    assert_eq!(h.failure(46).outcome, Some(Outcome::NotSent));
    assert_eq!(h.open(), 0.1, "the rejected row's stored copy stands");
}

#[test]
fn a_read_a_newer_stream_event_discarded_is_not_a_commit() {
    let mut h = started();
    h.read(47);
    let socket = h.socket.unwrap();
    h.e.handle(
        h.now,
        Event::WsMessage {
            socket,
            text: r#"{"e":"ACCOUNT_UPDATE","a":{"P":[{"s":"BTC-USDT","pa":"0.3","ep":"62000","up":"7","ps":"BOTH","mt":"cross"}]}}"#,
        },
    );
    h.pump();
    assert_eq!(h.open(), 0.3);
    h.answer(POSITIONS, 200, FLAT);
    assert!(h.result(47).is_none(), "the event discarded that read");
    assert_eq!(h.open(), 0.3, "the event stands");
    let repair = h
        .timers
        .iter()
        .find(|(_, at)| *at <= h.now + 750.0)
        .copied()
        .expect("a repair read is due");
    h.now = repair.1;
    h.e.handle(h.now, Event::TimerFired { timer: repair.0 });
    h.pump();
    assert!(h.has(), "a new read follows");
    h.answer(POSITIONS, 200, FLAT);
    assert!(h.succeeded(47), "{:?}", h.done);
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
    let out = safe_cex_engine::test_support::prepared_actions(&mut e, 0.0);
    assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 60, error }) if error.kind != Some(ErrorKind::Unsupported) && error.outcome == Some(Outcome::NotSent))), "{out:?}");
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
}

#[test]
fn an_owned_row_outside_the_catalogue_is_named_and_a_flat_one_is_not() {
    let unlisted = ROW.replace("BTC-USDT", "DOGE-USDT");
    let mut h = started();
    h.read(70);
    h.answer(POSITIONS, 200, &unlisted);
    assert_eq!(
        h.result(70)
            .map(|r| r.as_deref().map_err(|e| e.message.as_str())),
        Some(Ok(r#"{"skipped":["DOGEUSDT"],"open":[]}"#))
    );

    let flat = unlisted.replace("\"positionAmt\":\"0.1\"", "\"positionAmt\":\"0\"");
    let mut h = started();
    h.read(71);
    h.answer(POSITIONS, 200, &flat);
    assert!(h.succeeded(71), "{:?}", h.done);
}
