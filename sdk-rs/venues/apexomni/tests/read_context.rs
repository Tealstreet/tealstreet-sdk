//! Original native positives precede the managed read custody assertions
//! (RUST_APEXOMNI_READ_CONTEXT.md). Each independent case is its own test.
mod support;

use safe_cex_engine::request_context::{Claim, HttpAction, Prepared, RoutingIntent};
use safe_cex_engine::{Action, Command, ErrorKind, Event, HttpRequest, Notice, Query, Response};
use safe_cex_venue_apexomni::Options;
use support::*;

const COMMAND: u32 = 42;
const ACCOUNT: &str = r#"{"code":0,"data":{"id":"611779989544632629","positions":[],"contractAccount":{"makerFeeRate":"0.0002","takerFeeRate":"0.0005"}}}"#;
const ORDERS: &str = r#"{"code":0,"data":[]}"#;
const FILLS: &str = r#"{"code":0,"data":{"fills":[]}}"#;
const SIGNATURE: &str = r#"{"code":20016,"msg":"signature invalid"}"#;

#[derive(Default)]
struct Output {
    requests: Vec<HttpRequest>,
    actions: Vec<Action>,
    legacy: usize,
    intents: Vec<RoutingIntent>,
    prepared: usize,
    claimed: usize,
}

// The same drain runs native and managed sources. It never substitutes a
// request or terminal, and follows the real Prepare/Bind/Claim handoff.
fn drain(engine: &mut TestEngine, now: f64) -> Output {
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
                    intent,
                    request,
                } => {
                    assert!(engine.http_preparation_pending(preparation));
                    output.intents.push(intent);
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
                    preparation,
                    permit,
                    request,
                } => {
                    assert_eq!(preparation.attempt().request, request.id);
                    assert!(matches!(
                        engine.claim_http(now, permit),
                        Ok(Claim::Released(_))
                    ));
                    output.claimed += 1;
                    output.requests.push(request);
                }
                HttpAction::Cancel { .. } => {}
            },
            action => output.actions.push(action),
        }
        queue.extend(engine.drain_actions());
    }
    output
}

fn reply(engine: &mut TestEngine, now: f64, request: &HttpRequest, body: &str) -> Output {
    engine.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
    drain(engine, now)
}

fn transport_loss(engine: &mut TestEngine, now: f64, request: &HttpRequest) -> Output {
    engine.handle(
        now,
        Event::HttpFailed {
            request: request.id,
            error: "network lost",
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

fn signed(request: &HttpRequest, path: &str) {
    assert!(request.url.contains(path), "{}", request.url);
    assert_eq!(header(request, "APEX-API-KEY"), Some("key"));
    assert_eq!(header(request, "APEX-PASSPHRASE"), Some("pass"));
    assert!(header(request, "APEX-SIGNATURE").is_some());
    assert!(header(request, "APEX-TIMESTAMP").is_some());
}

fn managed(output: &Output, count: usize) {
    assert_eq!(
        (output.legacy, output.prepared, output.claimed),
        (0, count, count),
        "(legacy, Prepare, Claim)"
    );
}

fn material(passphrase: Option<&'static str>) -> Material<'static> {
    Material {
        key: "key",
        signing: "c2VjcmV0",
        passphrase,
        seed: Some("11".repeat(65)),
    }
}

fn engine(passphrase: Option<&'static str>) -> TestEngine {
    private_engine(Options::default(), &material(passphrase))
}

/// Started and seeded with markets; returns the account snapshot requests.
fn seeded() -> (TestEngine, Output) {
    let mut engine = engine(Some("pass"));
    engine.start(0.);
    let symbols = drain(&mut engine, 0.).requests.remove(0);
    let snapshot = reply(&mut engine, 10., &symbols, MARKETS);
    (engine, snapshot)
}

fn terminal(output: &Output, command: u32) -> Result<String, safe_cex_engine::VenueError> {
    output
        .actions
        .iter()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandDone { command: c, result }) if *c == command => {
                Some(Ok(result.clone()))
            }
            Action::Notice(Notice::CommandFailed { command: c, error }) if *c == command => {
                Some(Err(error.clone()))
            }
            _ => None,
        })
        .expect("command terminal")
}

fn command(engine: &mut TestEngine, now: f64, command: Command, wait: Option<u32>) -> Output {
    engine.handle_command(now, command, wait).unwrap();
    drain(engine, now)
}

fn fills() -> Command {
    Command::Query {
        command: COMMAND,
        query: Query::Fills {
            symbol: Some("BTCUSDT".into()),
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    }
}

#[test]
fn startup_symbols_with_a_passphrase_is_a_managed_signed_background_read() {
    let mut engine = engine(Some("pass"));
    engine.start(0.);
    let output = drain(&mut engine, 0.);
    assert_eq!(output.requests.len(), 1);
    signed(&output.requests[0], "/v3/symbols");
    managed(&output, 1);
    assert_eq!(output.intents, [RoutingIntent::Primary]);
}

#[test]
fn startup_symbols_without_a_passphrase_is_a_managed_unsigned_read() {
    let mut engine = engine(None);
    engine.start(0.);
    let output = drain(&mut engine, 0.);
    assert_eq!(output.requests.len(), 1);
    let request = &output.requests[0];
    assert!(request.url.ends_with("/v3/symbols"));
    assert!(
        request
            .headers
            .iter()
            .all(|(key, _)| !key.starts_with("APEX-"))
    );
    managed(&output, 1);
    // No Credential or Selection residue: the context routes as Public.
    assert_eq!(output.intents, [RoutingIntent::Public]);
}

#[test]
fn account_snapshot_positions_and_orders_are_managed_signed_background_reads() {
    let (_, snapshot) = seeded();
    let mut paths: Vec<_> = snapshot.requests.iter().map(|r| r.url.clone()).collect();
    paths.sort();
    assert!(paths[0].ends_with("/v3/account"), "{paths:?}");
    assert!(paths[1].ends_with("/v3/open-orders"), "{paths:?}");
    signed(&snapshot.requests[0], "/v3/");
    signed(&snapshot.requests[1], "/v3/");
    managed(&snapshot, 2);
}

#[test]
fn committed_snapshot_balance_is_a_managed_signed_background_read() {
    let (mut engine, snapshot) = seeded();
    let mut balance = Output::default();
    for request in &snapshot.requests {
        let body = if request.url.ends_with("/v3/account") {
            ACCOUNT
        } else {
            ORDERS
        };
        let output = reply(&mut engine, 30., request, body);
        balance.requests.extend(output.requests);
        balance.legacy += output.legacy;
        balance.prepared += output.prepared;
        balance.claimed += output.claimed;
    }
    assert_eq!(balance.requests.len(), 1);
    signed(&balance.requests[0], "/v3/account-balance");
    managed(&balance, 1);
}

#[test]
fn fetch_account_identity_is_a_managed_signed_command_read() {
    let (mut engine, _) = seeded();
    let output = command(
        &mut engine,
        20.,
        Command::FetchAccount { command: COMMAND },
        None,
    );
    assert_eq!(output.requests.len(), 1);
    signed(&output.requests[0], "/v3/account");
    managed(&output, 1);
}

#[test]
fn fill_history_is_a_managed_signed_read_under_the_caller_root() {
    let (mut engine, _) = seeded();
    let output = command(&mut engine, 20., fills(), None);
    assert_eq!(output.requests.len(), 1);
    signed(&output.requests[0], "/v3/fills?");
    managed(&output, 1);
}

#[test]
fn fill_history_data_at_the_original_receipt_deadline_is_a_strict_timeout() {
    let (mut engine, _) = seeded();
    let output = command(&mut engine, 20., fills(), Some(1_000));
    let request = output.requests[0].clone();
    let done = reply(&mut engine, 1_020., &request, FILLS);
    let kinds: Vec<_> = done
        .actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandDone { command, .. }) if *command == COMMAND => {
                Some(None)
            }
            Action::Notice(Notice::CommandFailed { command, error }) if *command == COMMAND => {
                Some(Some(error.kind))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        kinds,
        [Some(Some(ErrorKind::Timeout))],
        "one strict Timeout"
    );
}

// GREEN control: the native one-time re-sign after a signature refusal keeps
// the same path and refs; a second refusal fails without a third request.
#[test]
fn signed_read_signature_refusal_re_signs_once_then_fails() {
    let (mut engine, _) = seeded();
    let first = command(
        &mut engine,
        20.,
        Command::FetchAccount { command: COMMAND },
        None,
    );
    let resent = reply(&mut engine, 21., &first.requests[0], SIGNATURE);
    assert_eq!(resent.requests.len(), 1);
    signed(&resent.requests[0], "/v3/account");
    let failed = reply(&mut engine, 22., &resent.requests[0], SIGNATURE);
    assert!(failed.requests.is_empty());
    assert!(terminal(&failed, COMMAND).is_err());
}

// GREEN control: a signed GET survives three transport losses on the same
// path, and the fourth fails without another request.
#[test]
fn signed_read_transport_loss_retries_three_times_then_fails() {
    let (mut engine, _) = seeded();
    let mut output = command(
        &mut engine,
        20.,
        Command::FetchAccount { command: COMMAND },
        None,
    );
    for attempt in 0..3 {
        let request = output.requests.remove(0);
        output = transport_loss(&mut engine, 21. + f64::from(attempt), &request);
        assert_eq!(output.requests.len(), 1, "retry {attempt}");
        signed(&output.requests[0], "/v3/account");
    }
    let request = output.requests.remove(0);
    let failed = transport_loss(&mut engine, 30., &request);
    assert!(failed.requests.is_empty());
    assert!(terminal(&failed, COMMAND).is_err());
}

// GREEN control: signed symbols keep the one native re-sign, then defer to
// the startup backoff; a transport loss never retries inline.
#[test]
fn signed_symbols_re_sign_once_and_never_transport_retry() {
    let mut refusal = engine(Some("pass"));
    refusal.start(0.);
    let first = drain(&mut refusal, 0.).requests.remove(0);
    let resent = reply(&mut refusal, 1., &first, SIGNATURE);
    assert_eq!(resent.requests.len(), 1);
    signed(&resent.requests[0], "/v3/symbols");
    let refused = reply(&mut refusal, 2., &resent.requests[0], SIGNATURE);
    assert!(refused.requests.is_empty());
    assert!(
        refused
            .actions
            .iter()
            .any(|a| matches!(a, Action::SetTimer { .. }))
    );

    let mut loss = engine(Some("pass"));
    loss.start(0.);
    let first = drain(&mut loss, 0.).requests.remove(0);
    let lost = transport_loss(&mut loss, 1., &first);
    assert!(lost.requests.is_empty());
    assert!(
        lost.actions
            .iter()
            .any(|a| matches!(a, Action::SetTimer { .. }))
    );
}

// Readiness controls for the managed source (RUST_APEXOMNI_READ_CONTEXT.md).

/// Every Prepare is refused by the host route; nothing reaches the network.
fn refused(engine: &mut TestEngine, now: f64) -> Output {
    let mut output = Output::default();
    let mut queue: std::collections::VecDeque<_> = engine.drain_actions().collect();
    while let Some(action) = queue.pop_front() {
        match action {
            Action::Http(request) => {
                output.legacy += 1;
                output.requests.push(request);
            }
            Action::ManagedHttp(action) => {
                if let HttpAction::Prepare { preparation, .. } = *action {
                    output.prepared += 1;
                    engine.http_preparation_failed(
                        now,
                        preparation,
                        safe_cex_engine::request_context::Error::Invalid,
                        safe_cex_engine::VenueError::new("route refused"),
                    );
                }
            }
            action => output.actions.push(action),
        }
        queue.extend(engine.drain_actions());
    }
    output
}

#[test]
fn signature_resend_and_transport_retries_are_managed_attempts() {
    let (mut engine, _) = seeded();
    let first = command(
        &mut engine,
        20.,
        Command::FetchAccount { command: COMMAND },
        None,
    );
    let resent = reply(&mut engine, 21., &first.requests[0], SIGNATURE);
    managed(&resent, 1);
    let mut output = transport_loss(&mut engine, 22., &resent.requests[0]);
    for attempt in 0..3 {
        managed(&output, 1);
        let request = output.requests.remove(0);
        output = transport_loss(&mut engine, 23. + f64::from(attempt), &request);
    }
    assert!(output.requests.is_empty());
    assert!(terminal(&output, COMMAND).is_err());
}

#[test]
fn fill_aliases_and_the_history_fallback_are_managed_legs_of_one_root() {
    let (mut engine, _) = seeded();
    let mut output = command(&mut engine, 20., fills(), None);
    let mut paths = vec![];
    for page in 0..4 {
        managed(&output, 1);
        signed(&output.requests[0], "/v3/");
        paths.push(output.requests[0].url.clone());
        let request = output.requests.remove(0);
        output = reply(&mut engine, 21. + f64::from(page), &request, ORDERS);
    }
    assert!(paths[0].contains("/v3/fills?") && paths[0].contains("symbol=BTC-USDT"));
    assert!(paths[1].contains("/v3/fills?") && paths[1].contains("symbol=BTCUSDT"));
    assert!(paths[2].contains("/v3/history-orders?") && paths[2].contains("symbol=BTC-USDT"));
    assert!(paths[3].contains("/v3/history-orders?") && paths[3].contains("symbol=BTCUSDT"));
    assert!(output.requests.is_empty());
    assert_eq!(terminal(&output, COMMAND).unwrap(), "[]");
}

#[test]
fn a_refused_symbols_preparation_reschedules_startup_without_sending() {
    let mut engine = engine(Some("pass"));
    engine.start(0.);
    let output = refused(&mut engine, 0.);
    assert_eq!((output.legacy, output.prepared), (0, 1));
    assert!(output.requests.is_empty());
    assert!(
        output.actions.iter().any(
            |a| matches!(a, Action::SetTimer { at_ms, .. } if (750. ..=1_000.).contains(at_ms))
        ),
        "startup backoff"
    );
}

#[test]
fn a_refused_snapshot_preparation_is_not_retried_on_the_network() {
    let mut engine = engine(Some("pass"));
    engine.start(0.);
    let symbols = drain(&mut engine, 0.).requests.remove(0);
    engine.handle(
        10.,
        Event::HttpResponse {
            request: symbols.id,
            response: Response {
                status: 200,
                headers: "",
                body: MARKETS,
            },
        },
    );
    let output = refused(&mut engine, 10.);
    assert_eq!((output.legacy, output.prepared), (0, 2));
    assert!(output.requests.is_empty());
    assert!(!engine.store().loaded().positions);
}

#[test]
fn refused_command_preparations_fail_their_own_command_once() {
    // A refused fills page walks the native aliases and fallback, never a resend.
    for (query, pages) in [
        (Command::FetchAccount { command: COMMAND }, 1),
        (fills(), 4),
    ] {
        let (mut engine, _) = seeded();
        engine.handle_command(20., query, None).unwrap();
        let output = refused(&mut engine, 20.);
        assert_eq!((output.legacy, output.prepared), (0, pages));
        assert!(output.requests.is_empty());
        let terminals = output
            .actions
            .iter()
            .filter(|a| {
                matches!(a, Action::Notice(Notice::CommandFailed { command, .. } | Notice::CommandDone { command, .. }) if *command == COMMAND)
            })
            .count();
        assert_eq!(terminals, 1);
        assert!(terminal(&output, COMMAND).is_err());
    }
}

#[test]
fn late_data_after_a_finished_background_read_changes_nothing() {
    let (mut engine, snapshot) = seeded();
    let positions = snapshot
        .requests
        .iter()
        .find(|r| r.url.ends_with("/v3/account"))
        .unwrap()
        .clone();
    reply(&mut engine, 30., &positions, ACCOUNT);
    let late = reply(&mut engine, 31., &positions, ACCOUNT);
    assert!(late.requests.is_empty());
    assert_eq!((late.legacy, late.prepared, late.claimed), (0, 0, 0));
}

#[test]
fn a_reused_command_id_with_a_live_peer_sends_nothing_more() {
    let (mut engine, _) = seeded();
    let first = command(
        &mut engine,
        20.,
        Command::FetchAccount { command: COMMAND },
        None,
    );
    managed(&first, 1);
    let _ = engine.handle_command(21., Command::FetchAccount { command: COMMAND }, None);
    let second = drain(&mut engine, 21.);
    assert!(second.requests.is_empty());
    let done = reply(&mut engine, 22., &first.requests[0], ACCOUNT);
    assert!(terminal(&done, COMMAND).is_ok());
}

#[test]
fn a_background_read_answered_at_its_exact_deadline_is_not_a_success() {
    let (mut engine, snapshot) = seeded();
    // Both snapshot reads were issued at 10 with the 20 s native timeout.
    for request in &snapshot.requests {
        let body = if request.url.ends_with("/v3/account") {
            ACCOUNT
        } else {
            ORDERS
        };
        let output = reply(&mut engine, 20_010., request, body);
        assert!(output.requests.is_empty(), "no balance read follows");
    }
    assert!(!engine.store().loaded().positions);
    assert!(!engine.store().loaded().orders);
}

#[test]
fn a_position_heal_refresh_is_a_managed_signed_background_read() {
    let mut engine = engine(Some("pass"));
    engine.start(0.);
    let symbols = drain(&mut engine, 0.).requests.remove(0);
    let snapshot = reply(&mut engine, 10., &symbols, MARKETS);
    let socket = snapshot
        .actions
        .iter()
        .find_map(|a| match a {
            Action::WsConnect {
                socket,
                channel: Some(safe_cex_engine::SocketChannel::Private),
                ..
            } => Some(*socket),
            _ => None,
        })
        .expect("private socket");
    engine.handle(20., Event::WsOpened { socket });
    drain(&mut engine, 20.);
    let mut balance = vec![];
    for request in &snapshot.requests {
        let body = if request.url.ends_with("/v3/account") {
            ACCOUNT
        } else {
            ORDERS
        };
        balance.extend(reply(&mut engine, 30., request, body).requests);
    }
    reply(
        &mut engine,
        31.,
        &balance[0],
        r#"{"code":0,"data":{"totalEquityValue":"1000","initialMargin":"0","maintenanceMargin":"0"}}"#,
    );
    engine.handle(
        100.,
        Event::WsMessage {
            socket,
            text: r#"{"topic":"ws_zk_accounts_v3","type":"delta","contents":{"positions":[]}}"#,
        },
    );
    let timer = drain(&mut engine, 100.)
        .actions
        .into_iter()
        .find_map(|a| match a {
            Action::SetTimer {
                timer,
                at_ms: 1_100.,
            } => Some(timer),
            _ => None,
        })
        .expect("heal timer");
    engine.handle(1_100., Event::TimerFired { timer });
    let heal = drain(&mut engine, 1_100.);
    assert_eq!(heal.requests.len(), 1);
    signed(&heal.requests[0], "/v3/account");
    managed(&heal, 1);
}

#[test]
fn fetch_account_refusals_keep_the_legacy_text_and_send_nothing() {
    let public_only = private_engine(
        Options {
            public_data_only: true,
            ..Options::default()
        },
        &material(Some("pass")),
    );
    let keyless = safe_cex_engine::test_support::seeded_engine(
        safe_cex_venue_apexomni::ApexOmni::new(Options::default()),
    );
    for mut engine in [public_only, keyless] {
        let output = command(
            &mut engine,
            20.,
            Command::FetchAccount { command: COMMAND },
            None,
        );
        assert!(output.requests.is_empty());
        assert_eq!((output.legacy, output.prepared), (0, 0));
        assert_eq!(
            terminal(&output, COMMAND).unwrap_err().message,
            "ApeX account identity requires API credentials"
        );
    }
}

#[test]
fn public_data_only_with_a_passphrase_still_signs_symbols() {
    let mut engine = private_engine(
        Options {
            public_data_only: true,
            ..Options::default()
        },
        &material(Some("pass")),
    );
    engine.start(0.);
    let output = drain(&mut engine, 0.);
    assert_eq!(output.requests.len(), 1);
    signed(&output.requests[0], "/v3/symbols");
    managed(&output, 1);
    assert_eq!(output.intents, [RoutingIntent::Primary]);
}
