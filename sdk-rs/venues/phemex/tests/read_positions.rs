//! `readPositions`: success only after a read issued after the command commits.
#![allow(dead_code)]
mod support;
use safe_cex_engine::{
    Action, Command, ErrorKind, Event, HttpRequest, Notice, Outcome, Response, SocketId, TimerId,
    VenueAction, VenueError,
};
use safe_cex_venue_phemex::PhemexPerpetuals;
use support::{Options, PreparedEngine};

const NOW: f64 = 1_700_000_000_000.;
const PRODUCTS: &str = r#"{"code":0,"data":{"perpProductsV2":[{"symbol":"BTCUSDT","type":"PerpetualV2","settleCurrency":"USDT","baseCurrency":"BTC","quoteCurrency":"USDT","qtyStepSize":"0.001","tickSize":"0.1","maxOrderQtyRq":"1000","maxLeverage":"100"},{"symbol":"ETHUSDC","type":"PerpetualV2","settleCurrency":"USDC","baseCurrency":"ETH","quoteCurrency":"USDC","qtyStepSize":"0.001","tickSize":"0.1","maxOrderQtyRq":"1000","maxLeverage":"100"}]}}"#;
const DAILY: &str = r#"{"result":[{"symbol":"BTCUSDT","bidRp":"69999","askRp":"70001","lastRp":"70000","markRp":"70000","indexRp":"70000","openRp":"69000","openInterestRv":"100","fundingRateRr":"0","volumeRq":"10"},{"symbol":"ETHUSDC","bidRp":"1999","askRp":"2001","lastRp":"2000","markRp":"2000","indexRp":"2000","openRp":"1900","openInterestRv":"100","fundingRateRr":"0","volumeRq":"10"}]}"#;
const BTC: &str = r#"{"symbol":"BTCUSDT","currency":"USDT","posMode":"OneWay","posSide":"Merged","side":"Buy","size":"0.01","leverageRr":"-20","avgEntryPriceRp":"69000","liquidationPriceRp":"50000","transactTimeNs":1700000000000000000}"#;
const USDT: &str = "currency=USDT";
const USDC: &str = "currency=USDC";

fn envelope(currency: &str, rows: &str) -> String {
    format!(
        r#"{{"code":0,"data":{{"account":{{"currency":"{currency}","accountBalanceRv":"1000","totalUsedBalanceRv":"69"}},"positions":[{rows}]}}}}"#
    )
}

struct H {
    e: PreparedEngine<PhemexPerpetuals>,
    socket: SocketId,
    http: Vec<HttpRequest>,
    timers: Vec<(TimerId, f64)>,
    done: Vec<(u32, Result<String, VenueError>)>,
    now: f64,
}

fn started() -> H {
    let mut h = H {
        e: support::prepared_engine(Options {
            key: Some("key".into()),
            secret: Some("secret".into()),
            ..Options::default()
        }),
        socket: SocketId(0),
        http: Vec::new(),
        timers: Vec::new(),
        done: Vec::new(),
        now: NOW,
    };
    h.e.start(NOW);
    h.pump();
    h.answer("products", 200, PRODUCTS);
    h.answer("24hr/all", 200, DAILY);
    h.answer(USDT, 200, &envelope("USDT", BTC));
    h.answer(USDC, 200, &envelope("USDC", ""));
    h.http.clear();
    h
}

impl H {
    fn pump(&mut self) {
        let actions: Vec<_> = self.e.drain_actions().collect();
        for action in actions {
            match action {
                Action::Http(request) => self.http.push(request),
                Action::WsConnect {
                    socket,
                    channel: Some(safe_cex_engine::SocketChannel::Private),
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
    fn has(&self, part: &str) -> bool {
        self.http.iter().any(|r| r.url.contains(part))
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
fn every_currency_commits_a_new_read_then_it_succeeds() {
    let mut h = started();
    assert_eq!(h.open(), 0.01);
    h.read(40);
    assert!(h.has(USDT) && h.has(USDC), "a read per currency is issued");
    assert!(h.result(40).is_none());
    h.answer(USDT, 200, &envelope("USDT", ""));
    assert!(h.result(40).is_none(), "USDC has not answered");
    assert_eq!(h.open(), 0.0, "the store is flat");
    h.answer(USDC, 200, &envelope("USDC", ""));
    assert!(h.succeeded(40), "{:?}", h.done);
}

#[test]
fn a_read_in_flight_is_followed_by_a_new_one() {
    let mut h = started();
    h.read(39);
    let usdt = h.take(USDT);
    let usdc = h.take(USDC);
    h.read(41);
    assert!(!h.has(USDT), "no second read yet");
    h.reply(&usdt, 200, &envelope("USDT", BTC));
    h.reply(&usdc, 200, &envelope("USDC", ""));
    assert!(h.succeeded(39), "its own read answers it");
    assert!(h.result(41).is_none(), "the older read cannot");
    assert_eq!(h.open(), 0.01);
    assert!(h.has(USDT), "a new read follows");
    h.answer(USDT, 200, &envelope("USDT", ""));
    h.answer(USDC, 200, &envelope("USDC", ""));
    assert!(h.succeeded(41), "{:?}", h.done);
    assert_eq!(h.open(), 0.0);
}

#[test]
fn a_failed_read_fails_the_command_with_the_venues_error() {
    let mut h = started();
    h.read(42);
    h.answer(USDT, 401, r#"{"code":401,"msg":"bad api key"}"#);
    let error = h.failure(42);
    assert!(error.message.contains("bad api key"), "{error:?}");
    assert_eq!(error.kind, Some(ErrorKind::AuthInvalid));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert_eq!(h.open(), 0.01, "the store stands");
}

#[test]
fn a_venue_code_a_lost_read_and_a_rate_limit_each_fail_it() {
    let mut h = started();
    h.read(43);
    h.answer(USDC, 200, r#"{"code":11003,"msg":"OM_ORDER_REJECTED"}"#);
    assert!(h.failure(43).message.contains("OM_ORDER_REJECTED"));
    assert_eq!(h.failure(43).outcome, Some(Outcome::NotSent));

    let mut h = started();
    h.read(44);
    let request = h.take(USDT);
    h.e.handle(
        h.now,
        Event::HttpFailed {
            request: request.id,
            error: "connection lost",
        },
    );
    h.pump();
    assert_eq!(h.failure(44).kind, Some(ErrorKind::Network));

    let mut h = started();
    h.read(45);
    h.answer(USDT, 429, "{}");
    assert_eq!(h.failure(45).kind, Some(ErrorKind::RateLimited));
    h.answer(USDC, 200, &envelope("USDC", ""));
    h.read(46);
    assert_eq!(h.failure(46).kind, Some(ErrorKind::RateLimited), "paused");
    assert!(!h.has(USDT), "nothing is sent");
}

#[test]
fn a_lost_cycle_fails_its_waiter_at_the_deadline() {
    let mut h = started();
    h.read(47);
    let deadline = h
        .timers
        .iter()
        .find(|(_, at)| *at == h.now + 20_000.)
        .copied()
        .expect("the cycle's deadline");
    h.e.handle(deadline.1, Event::TimerFired { timer: deadline.0 });
    h.now = deadline.1;
    h.pump();
    assert_eq!(h.failure(47).kind, Some(ErrorKind::Timeout));
    assert_eq!(h.failure(47).outcome, Some(Outcome::NotSent));
}

#[test]
fn a_malformed_reply_or_an_unmappable_row_fails_the_command() {
    let mut h = started();
    h.read(48);
    h.answer(USDT, 200, "not json");
    assert_eq!(h.failure(48).outcome, Some(Outcome::NotSent));

    let mut h = started();
    h.read(49);
    let bad = r#"{"symbol":"BTCUSDT","currency":"USDT","posMode":"Sideways","posSide":"Up","side":"Buy","size":"0.01"}"#;
    h.answer(USDT, 200, &envelope("USDT", bad));
    assert!(
        h.failure(49).message.contains("could not be mapped"),
        "{:?}",
        h.done
    );
    assert_eq!(h.failure(49).outcome, Some(Outcome::NotSent));
}

#[test]
fn a_public_engine_refuses_without_sending() {
    let mut e = support::prepared_engine(Options::default());
    e.handle(
        NOW,
        Event::Command(Command::Venue {
            command: 60,
            action: VenueAction::ReadPositions,
        }),
    );
    let out: Vec<_> = e.drain_actions().collect();
    assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 60, error }) if error.kind != Some(ErrorKind::Unsupported) && error.outcome == Some(Outcome::NotSent))), "{out:?}");
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
}
