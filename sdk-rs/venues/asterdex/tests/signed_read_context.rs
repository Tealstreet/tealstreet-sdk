//! Managed custody of Aster's signed reads (RUST_ASTERDEX_REQUEST_CONTEXT.md,
//! remaining funnel slice 2). Each independent case is its own test.
mod support;

use safe_cex_engine::request_context::{Claim, HttpAction, Prepared};
use safe_cex_engine::{Action, Command, Engine, ErrorKind, Event, HttpRequest, Notice, Response};
use safe_cex_venue_asterdex::AsterDex;
use support::Options;

const START: f64 = 1_700_000_000_000.0;
const COMMAND: u32 = 42;
const TIME: &str = r#"{"serverTime":1700000000000}"#;
const MARKETS: &str = r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","pricePrecision":2,"quantityPrecision":3,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.10"},{"filterType":"LOT_SIZE","stepSize":"0.001","minQty":"0.001","maxQty":"1000"}]}]}"#;

#[derive(Default)]
struct Output {
    requests: Vec<HttpRequest>,
    actions: Vec<Action>,
    legacy: usize,
    legacy_urls: Vec<String>,
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
                output.legacy_urls.push(request.url.clone());
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

fn header<'a>(request: &'a HttpRequest, name: &str) -> Option<&'a str> {
    request
        .headers
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.as_str())
}

fn signed(request: &HttpRequest) {
    assert!(request.url.contains("signature="), "{}", request.url);
    assert_eq!(header(request, "X-MBX-APIKEY"), Some("documented-key"));
}

/// A V1 account, clock-synced, with markets loaded: account startup reads are in flight.
fn private() -> (Engine<AsterDex>, Output) {
    let mut engine = support::engine(Options {
        key: Some("documented-key".into()),
        secret: Some("synthetic-secret".into()),
        ..Options::default()
    });
    engine.start(START);
    let started = drain(&mut engine, START);
    let clock = find(&started, "/time").clone();
    let synced = answer(&mut engine, START, &clock, 200, TIME);
    let discovery = find(&synced, "/exchangeInfo").clone();
    let output = stream(&mut engine, START, &discovery, MARKETS);
    (engine, output)
}

fn account() -> Command {
    Command::FetchAccount { command: COMMAND }
}

const ALIAS: &str = r#"{"accountAlias":"alias-1"}"#;

fn command(engine: &mut Engine<AsterDex>, now: f64, command: Command) -> Output {
    engine.handle(now, Event::Command(command));
    drain(engine, now)
}

#[test]
fn startup_account_snapshots_are_managed_signed_background_reads() {
    let (_, output) = private();
    for path in [
        "/account",
        "/positionRisk",
        "/openOrders",
        "/positionSide/dual",
        "/leverageBracket",
    ] {
        signed(find(&output, path));
    }
    assert!(
        output
            .legacy_urls
            .iter()
            .all(|url| url.contains("/listenKey")),
        "only the listenKey session write stays native: {:?}",
        output.legacy_urls
    );
}

#[test]
fn fetch_account_is_a_managed_signed_command_read_with_one_terminal() {
    let (mut engine, _) = private();
    let sent = command(&mut engine, START, account());
    managed(&sent, 1);
    let read = find(&sent, "/account").clone();
    signed(&read);
    let done = answer(&mut engine, START, &read, 200, ALIAS);
    let results = terminals(&done, COMMAND);
    assert_eq!(results.len(), 1);
    assert!(results[0].as_ref().unwrap().contains("alias-1"));
}

#[test]
fn a_signed_read_transport_loss_resigns_each_managed_attempt_then_fails_typed() {
    let (mut engine, _) = private();
    let sent = command(&mut engine, START, account());
    let mut read = find(&sent, "/account").clone();
    let mut now = START;
    for _ in 0..3 {
        let lost = transport_loss(&mut engine, now, &read);
        assert!(terminals(&lost, COMMAND).is_empty());
        let (timer, at) = *timers(&lost)
            .iter()
            .filter(|(_, at)| *at > now)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .expect("native retry delay");
        now = at;
        engine.handle(now, Event::TimerFired { timer });
        let resent = drain(&mut engine, now);
        managed(&resent, 1);
        let next = find(&resent, "/account").clone();
        assert_ne!(next.url, read.url, "a fresh signature per attempt");
        read = next;
    }
    let failed = transport_loss(&mut engine, now, &read);
    let results = terminals(&failed, COMMAND);
    assert_eq!(results.len(), 1);
    let error = results[0].as_ref().unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::Network), "{error:?}");
}

#[test]
fn a_signed_read_unanswered_at_its_bound_fails_as_a_timeout() {
    let (mut engine, _) = private();
    let sent = command(&mut engine, START, account());
    let read = find(&sent, "/account").clone();
    let late = answer(&mut engine, START + 20_000.0, &read, 200, ALIAS);
    let results = terminals(&late, COMMAND);
    assert_eq!(results.len(), 1);
    let error = results[0].as_ref().unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::Timeout), "{error:?}");
}

#[test]
fn a_clock_gated_signed_read_prepares_after_the_clock_and_keeps_its_own_bound() {
    let (mut engine, _) = private();
    // The 300 s clock interval has lapsed, so the read waits for a fresh sync.
    let asked = START + 300_000.0;
    let sent = command(&mut engine, asked, account());
    assert!(
        sent.requests.iter().all(|r| !r.url.contains("/account")),
        "nothing signs before the clock"
    );
    let clock = find(&sent, "/time").clone();
    let synced_at = asked + 4_000.0;
    let released = answer(
        &mut engine,
        synced_at,
        &clock,
        200,
        r#"{"serverTime":1700000304000}"#,
    );
    managed(&released, 1);
    let read = find(&released, "/account").clone();
    signed(&read);
    // Past 20 s from the command, inside 20 s from the release.
    let done = answer(&mut engine, asked + 22_000.0, &read, 200, ALIAS);
    let results = terminals(&done, COMMAND);
    assert_eq!(results.len(), 1);
    assert!(results[0].is_ok(), "{:?}", results[0]);
}

#[test]
fn a_v1_signature_rejection_resigns_once_under_the_same_root() {
    let (mut engine, _) = private();
    let sent = command(&mut engine, START, account());
    let read = find(&sent, "/account").clone();
    let rejected = answer(
        &mut engine,
        START,
        &read,
        400,
        r#"{"code":-1022,"msg":"Signature for this request is not valid."}"#,
    );
    managed(&rejected, 1);
    let resent = find(&rejected, "/account").clone();
    // A new physical attempt, re-signed at release (same clock, same bytes).
    assert_ne!(resent.id, read.id);
    signed(&resent);
    let done = answer(&mut engine, START, &resent, 200, ALIAS);
    let results = terminals(&done, COMMAND);
    assert_eq!(results.len(), 1);
    assert!(results[0].is_ok(), "{:?}", results[0]);
}

#[test]
fn fill_history_is_a_managed_signed_leg_under_the_caller_root() {
    let (mut engine, _) = private();
    let sent = command(
        &mut engine,
        START,
        Command::Query {
            command: COMMAND,
            query: safe_cex_engine::Query::Fills {
                symbol: Some("BTCUSDT".into()),
                order_id: None,
                from_ms: Some(START - 60_000.0),
                to_ms: Some(START),
                limit: Some(10),
            },
        },
    );
    managed(&sent, 1);
    let page = find(&sent, "/userTrades").clone();
    signed(&page);
    let done = stream(&mut engine, START, &page, "[]");
    let results = terminals(&done, COMMAND);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].as_ref().unwrap(), "[]");
}

#[test]
fn keyless_public_trades_are_a_managed_unsigned_read() {
    let mut engine = support::engine(Options {
        public_data_only: true,
        ..Options::default()
    });
    engine.start(START);
    let started = drain(&mut engine, START);
    let clock = find(&started, "/time").clone();
    answer(&mut engine, START, &clock, 200, TIME);
    let sent = command(
        &mut engine,
        START,
        Command::Query {
            command: COMMAND,
            query: safe_cex_engine::Query::PublicTrades {
                symbol: "BTCUSDT".into(),
                limit: Some(2),
            },
        },
    );
    managed(&sent, 1);
    let trades = find(&sent, "/trades").clone();
    assert!(trades.headers.is_empty());
    let done = stream(&mut engine, START, &trades, "[]");
    assert_eq!(terminals(&done, COMMAND).len(), 1);
}

const TIMESTAMP: &str =
    r#"{"code":-1021,"msg":"Timestamp for this request was outside of the recvWindow."}"#;

/// After a -1021, the resync's /time and then exactly one re-signed resend.
fn resynced(engine: &mut Engine<AsterDex>, rejected: &Output) -> Output {
    let clock = find(rejected, "/time").clone();
    answer(
        engine,
        START,
        &clock,
        200,
        r#"{"serverTime":1700000009000}"#,
    )
}

#[test]
fn a_command_read_timestamp_rejection_resyncs_then_resends_once_re_signed() {
    let (mut engine, _) = private();
    let sent = command(&mut engine, START, account());
    let read = find(&sent, "/account").clone();
    let rejected = answer(&mut engine, START, &read, 400, TIMESTAMP);
    assert!(
        rejected
            .requests
            .iter()
            .all(|r| !r.url.contains("/account")),
        "nothing signs before the resync"
    );
    let released = resynced(&mut engine, &rejected);
    managed(&released, 1);
    let resent = find(&released, "/account").clone();
    assert!(
        resent.url.contains("timestamp=1700000009000"),
        "{}",
        resent.url
    );
    let refused = answer(&mut engine, START, &resent, 400, TIMESTAMP);
    assert!(
        refused.requests.iter().all(|r| !r.url.contains("/account")),
        "exactly one resend"
    );
    let results = terminals(&refused, COMMAND);
    assert_eq!(results.len(), 1);
    // A read keeps the native venue text and carries no money outcome.
    let error = results[0].as_ref().unwrap_err();
    assert!(error.message.contains("Timestamp"), "{error:?}");
    assert_eq!((error.kind, error.outcome), (None, None));
}

#[test]
fn a_background_snapshot_timestamp_rejection_resyncs_then_releases_its_resend() {
    let (mut engine, startup) = private();
    let snapshot = find(&startup, "/positionRisk").clone();
    let rejected = stream(&mut engine, START, &snapshot, TIMESTAMP);
    let released = resynced(&mut engine, &rejected);
    managed(&released, 1);
    let resent = find(&released, "/positionRisk").clone();
    assert!(
        resent.url.contains("timestamp=1700000009000"),
        "{}",
        resent.url
    );
    let again = stream(&mut engine, START, &resent, TIMESTAMP);
    assert!(
        again
            .requests
            .iter()
            .all(|r| !r.url.contains("/positionRisk")),
        "exactly one resend"
    );
}
