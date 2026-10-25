//! Managed custody of Aster's public reads and clock (RUST_ASTERDEX_REQUEST_CONTEXT.md,
//! remaining funnel slice 1). Each independent case is its own test.
mod support;

use safe_cex_engine::request_context::{Claim, HttpAction, Prepared};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Notice, RequestRoute, Response,
};
use safe_cex_venue_asterdex::AsterDex;
use support::Options;

const START: f64 = 1_700_000_000_000.0;
const COMMAND: u32 = 42;
const TIME: &str = r#"{"serverTime":1700000000000}"#;
const MARKETS: &str = r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","pricePrecision":2,"quantityPrecision":3,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.10"},{"filterType":"LOT_SIZE","stepSize":"0.001","minQty":"0.001","maxQty":"1000"}]}]}"#;
const DEPTH: &str = r#"{"lastUpdateId":1,"bids":[["100.0","1.5"]],"asks":[["101.0","2.0"]]}"#;
const KLINES: &str =
    r#"[[1700000000000,"1","2","0.5","1.5","10",1700000059999,"15",3,"5","7.5","0"]]"#;

#[derive(Default)]
struct Output {
    requests: Vec<HttpRequest>,
    actions: Vec<Action>,
    legacy: usize,
    prepared: usize,
    claimed: usize,
}

// One drain for native and managed sources; it follows the real
// Prepare/Bind/Claim handoff and never substitutes a request or terminal.
fn drain(engine: &mut Engine<AsterDex>, now: f64) -> Output {
    let mut output = Output::default();
    let mut queue: std::collections::VecDeque<_> = engine.drain_actions().collect();
    while let Some(action) = queue.pop_front() {
        match action {
            Action::Http(request) => {
                output.legacy += 1;
                output.requests.push(request);
            }
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => {
                    output.prepared += 1;
                    engine.http_prepared(
                        now,
                        preparation,
                        Prepared {
                            id: request.id.0,
                            expires_at: None,
                        },
                    );
                }
                HttpAction::Bind {
                    permit, request, ..
                } => {
                    assert!(matches!(
                        engine.claim_http(now, permit),
                        Ok(Claim::Released(_))
                    ));
                    output.claimed += 1;
                    output.requests.push(request);
                }
                HttpAction::BindGroup { .. } => panic!("Aster reads declare no Mutation barriers"),
                HttpAction::Cancel { .. } => {}
            },
            action => output.actions.push(action),
        }
        queue.extend(engine.drain_actions());
    }
    output
}

fn managed(output: &Output, count: usize) {
    assert_eq!(
        (output.legacy, output.prepared, output.claimed),
        (0, count, count),
        "(legacy, Prepare, Claim)"
    );
}

fn answer(
    engine: &mut Engine<AsterDex>,
    now: f64,
    request: &HttpRequest,
    status: u16,
    body: &str,
) -> Output {
    engine.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
    drain(engine, now)
}

fn transport_loss(engine: &mut Engine<AsterDex>, now: f64, request: &HttpRequest) -> Output {
    engine.handle(
        now,
        Event::HttpFailed {
            request: request.id,
            error: "network lost",
        },
    );
    drain(engine, now)
}

fn find<'a>(output: &'a Output, suffix: &str) -> &'a HttpRequest {
    output
        .requests
        .iter()
        .find(|request| request.url.split('?').next().unwrap().ends_with(suffix))
        .unwrap_or_else(|| panic!("missing {suffix}"))
}

fn terminals(output: &Output, command: u32) -> Vec<Result<String, safe_cex_engine::VenueError>> {
    output
        .actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandDone {
                command: id,
                result,
            }) if *id == command => Some(Ok(result.clone())),
            Action::Notice(Notice::CommandFailed { command: id, error }) if *id == command => {
                Some(Err(error.clone()))
            }
            _ => None,
        })
        .collect()
}

fn timers(output: &Output) -> Vec<(safe_cex_engine::TimerId, f64)> {
    output
        .actions
        .iter()
        .filter_map(|action| match action {
            Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
            _ => None,
        })
        .collect()
}

fn public() -> (Engine<AsterDex>, Output) {
    let mut engine = support::engine(Options {
        public_data_only: true,
        ..Options::default()
    });
    engine.start(START);
    let output = drain(&mut engine, START);
    (engine, output)
}

/// Started and clock-synced; startup discovery is in flight.
fn synced() -> (Engine<AsterDex>, Output) {
    let (mut engine, started) = public();
    let clock = find(&started, "/time").clone();
    let output = answer(&mut engine, START, &clock, 200, TIME);
    (engine, output)
}

fn candles() -> Command {
    Command::FetchCandles {
        command: COMMAND,
        symbol: "BTCUSDT".into(),
        interval: safe_cex_engine::model::Interval::M1,
        limit: Some(1),
        from_ms: None,
        to_ms: None,
    }
}

fn depth() -> Command {
    Command::FetchOrderBook {
        command: COMMAND,
        symbol: "BTCUSDT".into(),
        limit: Some(5),
    }
}

/// A streamed response: start, one chunk, end.
fn stream(engine: &mut Engine<AsterDex>, now: f64, request: &HttpRequest, body: &str) -> Output {
    engine.handle(
        now,
        Event::HttpStart {
            request: request.id,
            status: 200,
            headers: "",
        },
    );
    engine.handle(
        now,
        Event::HttpChunk {
            request: request.id,
            data: body.as_bytes(),
        },
    );
    engine.handle(
        now,
        Event::HttpEnd {
            request: request.id,
        },
    );
    drain(engine, now)
}

/// Startup finished: clock, discovery and seeds answered.
fn ready() -> Engine<AsterDex> {
    let (mut engine, mut output) = synced();
    for _ in 0..8 {
        let Some(request) = output.requests.first().cloned() else {
            break;
        };
        let path = request.url.split('?').next().unwrap().to_owned();
        output.requests.remove(0);
        let body = if path.ends_with("/exchangeInfo") {
            MARKETS
        } else {
            "[]"
        };
        let next = if path.ends_with("/exchangeInfo") || path.ends_with("/ticker/24hr") {
            stream(&mut engine, START, &request, body)
        } else {
            answer(&mut engine, START, &request, 200, body)
        };
        output.requests.extend(next.requests);
    }
    engine
}

fn command(engine: &mut Engine<AsterDex>, now: f64, command: Command) -> Output {
    engine.handle(now, Event::Command(command));
    drain(engine, now)
}

#[test]
fn the_startup_clock_is_a_managed_unsigned_background_read() {
    let (_, started) = public();
    managed(&started, 1);
    let clock = find(&started, "/time");
    assert!(clock.headers.is_empty(), "the clock is unsigned");
}

#[test]
fn clock_fallback_attempts_are_managed_and_the_last_two_use_public_cors() {
    let (mut engine, started) = public();
    let mut clock = find(&started, "/time").clone();
    assert_ne!(clock.route, Some(RequestRoute::PublicCors));
    let mut routes = Vec::new();
    for _ in 0..3 {
        let next = transport_loss(&mut engine, START, &clock);
        managed(&next, 1);
        clock = find(&next, "/time").clone();
        routes.push(clock.route);
    }
    assert_eq!(
        routes,
        [
            Some(RequestRoute::Direct),
            Some(RequestRoute::PublicCors),
            Some(RequestRoute::PublicCors)
        ]
    );
}

#[test]
fn a_silent_clock_attempt_expires_at_five_seconds_and_the_next_attempt_follows() {
    let (mut engine, started) = public();
    let first = find(&started, "/time").clone();
    let (timer, at) = timers(&started)
        .into_iter()
        .find(|(_, at)| *at == START + 5_000.0)
        .expect("the clock attempt's own 5 s deadline");
    engine.handle(at, Event::TimerFired { timer });
    let next = drain(&mut engine, at);
    managed(&next, 1);
    let second = find(&next, "/time");
    assert_ne!(second.id, first.id);
}

#[test]
fn startup_discovery_after_the_clock_is_managed_and_unsigned() {
    let (_, synced) = synced();
    let discovery = find(&synced, "/exchangeInfo");
    assert!(discovery.headers.is_empty());
    assert_eq!(synced.legacy, 0, "no legacy discovery read");
    assert!(synced.claimed >= 1);
}

#[test]
fn ticker_brackets_and_open_interest_seeds_are_managed() {
    let (mut engine, synced) = synced();
    let discovery = find(&synced, "/exchangeInfo").clone();
    let seeded = answer(&mut engine, START, &discovery, 200, MARKETS);
    assert_eq!(seeded.legacy, 0);
    let mut output = seeded;
    if let Some(brackets) = output
        .requests
        .iter()
        .find(|request| request.url.contains("/leverageBracket"))
        .cloned()
    {
        output = answer(&mut engine, START, &brackets, 200, "[]");
        assert_eq!(output.legacy, 0);
    }
    let daily = find(&output, "/ticker/24hr").clone();
    let after = answer(&mut engine, START, &daily, 200, "[]");
    assert_eq!(after.legacy, 0, "open interest follows managed");
}

#[test]
fn a_discovery_transport_failure_retries_without_a_venue_retry_timer() {
    let (mut engine, synced) = synced();
    let discovery = find(&synced, "/exchangeInfo").clone();
    let failed = transport_loss(&mut engine, START, &discovery);
    assert_eq!(failed.legacy, 0);
    // Startup's own discovery backoff remains; no second legacy retry path.
    assert!(
        !timers(&failed).iter().any(|(_, at)| *at == START + 1_000.0),
        "the venue's fixed 1 s read retry is gone"
    );
}

#[test]
fn fetch_candles_is_a_managed_command_read_with_one_terminal() {
    let mut engine = ready();
    let sent = command(&mut engine, START, candles());
    managed(&sent, 1);
    let klines = find(&sent, "/klines").clone();
    assert!(klines.headers.is_empty());
    let done = stream(&mut engine, START, &klines, KLINES);
    let results = terminals(&done, COMMAND);
    assert_eq!(results.len(), 1);
    assert!(results[0].is_ok(), "{:?}", results[0]);
    let late = stream(&mut engine, START, &klines, KLINES);
    assert!(
        terminals(&late, COMMAND).is_empty(),
        "a late duplicate completes nothing"
    );
}

#[test]
fn fetch_order_book_is_a_managed_command_read_with_one_terminal() {
    let mut engine = ready();
    let sent = command(&mut engine, START, depth());
    managed(&sent, 1);
    let book = find(&sent, "/depth").clone();
    let done = answer(&mut engine, START, &book, 200, DEPTH);
    let results = terminals(&done, COMMAND);
    assert_eq!(results.len(), 1);
    assert!(results[0].is_ok(), "{:?}", results[0]);
}

#[test]
fn candle_transport_loss_retries_as_managed_attempts_then_fails_typed() {
    let mut engine = ready();
    let sent = command(&mut engine, START, candles());
    let mut request = find(&sent, "/klines").clone();
    let mut now = START;
    for _ in 0..3 {
        let next = transport_loss(&mut engine, now, &request);
        assert_eq!(next.legacy, 0);
        assert!(terminals(&next, COMMAND).is_empty());
        let (timer, at) = *timers(&next).last().expect("shared backoff timer");
        now = at;
        engine.handle(now, Event::TimerFired { timer });
        let resent = drain(&mut engine, now);
        managed(&resent, 1);
        request = find(&resent, "/klines").clone();
    }
    let failed = transport_loss(&mut engine, now, &request);
    let results = terminals(&failed, COMMAND);
    assert_eq!(results.len(), 1);
    let error = results[0].as_ref().unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::Network), "{error:?}");
}

#[test]
fn a_candle_read_unanswered_at_its_deadline_fails_as_a_timeout() {
    let mut engine = ready();
    let sent = command(&mut engine, START, candles());
    let klines = find(&sent, "/klines").clone();
    let deadline = START + 20_000.0;
    let late = stream(&mut engine, deadline, &klines, KLINES);
    let results = terminals(&late, COMMAND);
    assert_eq!(results.len(), 1);
    let error = results[0].as_ref().unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::Timeout), "{error:?}");
}

#[test]
fn a_negative_public_envelope_fails_the_candle_command_once() {
    let mut engine = ready();
    let sent = command(&mut engine, START, candles());
    let klines = find(&sent, "/klines").clone();
    let failed = answer(
        &mut engine,
        START,
        &klines,
        200,
        r#"{"code":-1121,"msg":"Invalid symbol."}"#,
    );
    let results = terminals(&failed, COMMAND);
    assert_eq!(results.len(), 1);
    assert!(results[0].is_err());
    assert_eq!(failed.legacy, 0);
}
