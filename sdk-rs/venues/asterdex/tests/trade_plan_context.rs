//! Managed custody of Aster's trade plans (RUST_ASTERDEX_REQUEST_CONTEXT.md,
//! remaining funnel slice 3). Each independent case is its own test.
mod support;

use safe_cex_engine::model::*;
use safe_cex_engine::request_context::{Claim, HttpAction, Prepared};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Method, Notice, Outcome, Response,
};
use safe_cex_venue_asterdex::AsterDex;
use serde_json::json;
use support::Options;

const NOW: f64 = 1_700_000_000_000.0;
const COMMAND: u32 = 1;

thread_local! {
    /// Each clock answer moves the venue clock 9 s forward, so a resend that
    /// waited for the resync signs a different stamp than the first send.
    static CLOCK: std::cell::Cell<u64> = const { std::cell::Cell::new(1_700_000_000_000) };
}

#[derive(Default)]
struct Output {
    /// Every released URL in release order, the clock's included.
    released: Vec<String>,
    requests: Vec<HttpRequest>,
    actions: Vec<Action>,
    legacy: usize,
    claimed: usize,
}

// Native and managed sources through the real Prepare/Bind/Claim handoff; the
// clock is answered in place so writes can release.
fn drain(engine: &mut Engine<AsterDex>, now: f64) -> Output {
    let mut output = Output::default();
    let mut queue: std::collections::VecDeque<_> = engine.drain_actions().collect();
    while let Some(action) = queue.pop_front() {
        let (request, legacy) = match action {
            Action::Http(request) => (request, true),
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => {
                    engine.http_prepared(
                        now,
                        preparation,
                        Prepared {
                            id: request.id.0,
                            expires_at: None,
                        },
                    );
                    queue.extend(engine.drain_actions());
                    continue;
                }
                HttpAction::Bind {
                    permit, request, ..
                } => {
                    let claim = engine.claim_http(now, permit);
                    assert!(matches!(claim, Ok(Claim::Released(_))), "{:?}", claim.err());
                    (request, false)
                }
                HttpAction::BindGroup { .. } => panic!("Aster plans declare no Mutation barriers"),
                HttpAction::Cancel { .. } => continue,
            },
            action => {
                output.actions.push(action);
                continue;
            }
        };
        output.released.push(request.url.clone());
        if request.url.ends_with("/time") {
            let body = format!(
                r#"{{"serverTime":{}}}"#,
                CLOCK.with(|clock| clock.replace(clock.get() + 9_000))
            );
            engine.handle(
                now,
                Event::HttpResponse {
                    request: request.id,
                    response: Response {
                        status: 200,
                        headers: "",
                        body: &body,
                    },
                },
            );
            queue.extend(engine.drain_actions());
            continue;
        }
        if legacy {
            output.legacy += 1;
        } else {
            output.claimed += 1;
        }
        output.requests.push(request);
        queue.extend(engine.drain_actions());
    }
    output
}

fn answer(engine: &mut Engine<AsterDex>, request: &HttpRequest, status: u16, body: &str) -> Output {
    engine.handle(
        NOW,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
    drain(engine, NOW)
}

fn lost(engine: &mut Engine<AsterDex>, request: &HttpRequest) -> Output {
    engine.handle(
        NOW,
        Event::HttpFailed {
            request: request.id,
            error: "connection reset",
        },
    );
    drain(engine, NOW)
}

/// Every write released through a plan leg; none on the native path.
fn managed(output: &Output, count: usize) {
    assert_eq!(
        (output.legacy, output.claimed),
        (0, count),
        "(legacy, Claim)"
    );
}

fn write<'a>(output: &'a Output, method: Method, path: &str) -> &'a HttpRequest {
    output
        .requests
        .iter()
        .find(|r| r.method == method && r.url.split('?').next().unwrap().ends_with(path))
        .unwrap_or_else(|| panic!("missing {method:?} {path}: {:?}", output.requests))
}

fn query<'a>(url: &'a str, name: &str) -> Option<&'a str> {
    url.split(['?', '&'])
        .find_map(|pair| pair.strip_prefix(name)?.strip_prefix('='))
}

fn stamp(url: &str) -> Option<&str> {
    query(url, "timestamp")
}

fn signature(url: &str) -> Option<&str> {
    query(url, "signature")
}

fn header<'a>(request: &'a HttpRequest, name: &str) -> Option<&'a str> {
    request
        .headers
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.as_str())
}

fn terminal(output: &Output) -> Option<Result<String, safe_cex_engine::VenueError>> {
    output.actions.iter().find_map(|action| match action {
        Action::Notice(Notice::CommandDone { command, result }) if *command == COMMAND => {
            Some(Ok(result.clone()))
        }
        Action::Notice(Notice::CommandFailed { command, error }) if *command == COMMAND => {
            Some(Err(error.clone()))
        }
        _ => None,
    })
}

fn market() -> Market {
    Market {
        id: "BTCUSDT".into(),
        symbol: "BTCUSDT".into(),
        pretty_symbol: None,
        base: "BTC".into(),
        quote: "USDT".into(),
        active: true,
        precision: Precision {
            price: 0.1,
            amount: 0.001,
        },
        limits: Limits {
            amount: MinMax {
                min: 0.001,
                max: 2.0,
            },
            leverage: MinMax {
                min: 1.0,
                max: 125.0,
            },
            price: None,
        },
        contract_size: None,
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: Some(true),
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }
}

fn open(id: &str) -> Order {
    Order {
        id: id.into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "BTCUSDT".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        price: 9300.0,
        amount: 1.0,
        filled: 0.0,
        remaining: 1.0,
        reduce_only: false,
        close: None,
        client_order_id: None,
        time_in_force: None,
        last_update: 0.0,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    }
}

fn engine() -> Engine<AsterDex> {
    let mut engine = support::engine(Options {
        key: Some("synthetic-key".into()),
        secret: Some("synthetic-secret".into()),
        ..Options::default()
    });
    engine.store_mut().set_markets(vec![market()]);
    engine.store_mut().set_orders(vec![open("777")]);
    engine
}

fn command(engine: &mut Engine<AsterDex>, command: Command) -> Output {
    engine.handle(NOW, Event::Command(command));
    drain(engine, NOW)
}

fn place() -> Command {
    Command::PlaceOrders {
        command: COMMAND,
        orders: vec![
            serde_json::from_value(
                json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1.0,"price":9300.0}),
            )
            .unwrap(),
        ],
    }
}

fn cancel() -> Command {
    Command::CancelOrders {
        command: COMMAND,
        ids: vec!["777".into()],
    }
}

fn leverage() -> Command {
    Command::SetLeverage {
        command: COMMAND,
        symbol: "BTCUSDT".into(),
        leverage: 10.0,
        side: None,
    }
}

const SIGNATURE: &str = r#"{"code":-1022,"msg":"Signature for this request is not valid."}"#;
const TIMESTAMP: &str =
    r#"{"code":-1021,"msg":"Timestamp for this request was outside of the recvWindow."}"#;

#[test]
fn a_placement_is_one_managed_plan_leg_with_its_native_result() {
    let mut e = engine();
    let sent = command(&mut e, place());
    managed(&sent, 1);
    let order = write(&sent, Method::Post, "/order").clone();
    assert!(order.url.contains("signature="));
    let done = answer(&mut e, &order, 200, r#"{"orderId":12345}"#);
    let result = terminal(&done).expect("one terminal");
    assert_eq!(result.unwrap(), r#"["12345"]"#);
}

#[test]
fn a_cancel_is_a_managed_plan_leg_and_an_unknown_order_stays_idempotent() {
    let mut e = engine();
    let sent = command(&mut e, cancel());
    managed(&sent, 1);
    let delete = write(&sent, Method::Delete, "/order").clone();
    let done = answer(
        &mut e,
        &delete,
        400,
        r#"{"code":-2011,"msg":"Unknown order sent."}"#,
    );
    assert!(terminal(&done).expect("one terminal").is_ok());
}

#[test]
fn a_setting_write_is_a_managed_plan_leg() {
    let mut e = engine();
    let sent = command(&mut e, leverage());
    managed(&sent, 1);
    let setting = write(&sent, Method::Post, "/leverage").clone();
    let done = answer(
        &mut e,
        &setting,
        200,
        r#"{"leverage":10,"symbol":"BTCUSDT"}"#,
    );
    assert!(terminal(&done).expect("one terminal").is_ok());
}

/// V1 -1022 earns exactly one re-signed resend of the same leg, per method.
#[test]
fn a_v1_signature_rejection_resends_each_write_method_once_through_the_plan() {
    for (command_of, method, path) in [
        (place as fn() -> Command, Method::Post, "/order"),
        (cancel as fn() -> Command, Method::Delete, "/order"),
        (leverage as fn() -> Command, Method::Post, "/leverage"),
    ] {
        let mut e = engine();
        let sent = command(&mut e, command_of());
        let first = write(&sent, method, path).clone();
        let resent = answer(&mut e, &first, 400, SIGNATURE);
        managed(&resent, 1);
        let second = write(&resent, method, path).clone();
        assert_ne!(second.id, first.id);
        assert!(terminal(&resent).is_none());
        let failed = answer(&mut e, &second, 400, SIGNATURE);
        assert!(failed.requests.is_empty(), "only one resend");
        let error = terminal(&failed).expect("one terminal").unwrap_err();
        assert_eq!(error.outcome, Some(Outcome::Rejected), "{error:?}");
    }
}

/// -1021 resyncs the clock once and resends the same leg, per method.
#[test]
fn a_timestamp_rejection_resyncs_then_resends_each_write_method_once() {
    for (command_of, method, path) in [
        (place as fn() -> Command, Method::Post, "/order"),
        (cancel as fn() -> Command, Method::Delete, "/order"),
        (leverage as fn() -> Command, Method::Post, "/leverage"),
    ] {
        let mut e = engine();
        let sent = command(&mut e, command_of());
        let first = write(&sent, method, path).clone();
        let resent = answer(&mut e, &first, 400, TIMESTAMP);
        managed(&resent, 1);
        let second = write(&resent, method, path).clone();
        assert_ne!(second.id, first.id);
        // The resync runs first; the resend signs its fresh stamp after it.
        let clock = resent
            .released
            .iter()
            .position(|url| url.ends_with("/time"));
        let resend = resent.released.iter().position(|url| *url == second.url);
        assert!(clock.is_some() && clock < resend, "{:?}", resent.released);
        assert_ne!(stamp(&second.url), stamp(&first.url));
        assert_ne!(signature(&second.url), signature(&first.url));
        let failed = answer(&mut e, &second, 400, TIMESTAMP);
        assert!(failed.requests.is_empty(), "only one resend");
        let error = terminal(&failed).expect("one terminal").unwrap_err();
        assert_eq!(error.outcome, Some(Outcome::Rejected), "{error:?}");
    }
}

#[test]
fn a_released_gateway_failure_is_unknown_and_never_resent() {
    let mut e = engine();
    let sent = command(&mut e, place());
    let order = write(&sent, Method::Post, "/order").clone();
    let failed = answer(&mut e, &order, 503, "upstream timeout");
    // Never resent; the only request is the fresh order read that reconciles it.
    assert!(
        failed
            .requests
            .iter()
            .all(|r| r.url.contains("/openOrders")),
        "{:?}",
        failed.requests.iter().map(|r| &r.url).collect::<Vec<_>>()
    );
    assert_eq!(failed.requests.len(), 1);
    let error = terminal(&failed).expect("one terminal").unwrap_err();
    assert_eq!(error.outcome, Some(Outcome::Unknown), "{error:?}");
}

#[test]
fn a_transport_loss_after_release_is_unknown_and_never_resent() {
    let mut e = engine();
    let sent = command(&mut e, place());
    let order = write(&sent, Method::Post, "/order").clone();
    let failed = lost(&mut e, &order);
    // Never resent; the only request is the fresh order read that reconciles it.
    assert!(
        failed
            .requests
            .iter()
            .all(|r| r.url.contains("/openOrders")),
        "{:?}",
        failed.requests.iter().map(|r| &r.url).collect::<Vec<_>>()
    );
    assert_eq!(failed.requests.len(), 1);
    let error = terminal(&failed).expect("one terminal").unwrap_err();
    assert_eq!(error.outcome, Some(Outcome::Unknown), "{error:?}");
}

#[test]
fn a_cancel_batch_continues_after_a_failure_through_one_plan() {
    let mut e = engine();
    e.store_mut().set_orders(vec![open("777"), open("778")]);
    let sent = command(
        &mut e,
        Command::CancelOrders {
            command: COMMAND,
            ids: vec!["777".into(), "778".into()],
        },
    );
    managed(&sent, 1);
    let first = write(&sent, Method::Delete, "/order").clone();
    let next = answer(&mut e, &first, 400, r#"{"code":-4000,"msg":"busy"}"#);
    managed(&next, 1);
    let second = write(&next, Method::Delete, "/order").clone();
    assert!(second.url.contains("778"));
    let done = answer(&mut e, &second, 200, r#"{"status":"CANCELED"}"#);
    assert!(terminal(&done).is_some());
}

#[test]
fn a_v3_stp_mode_read_is_a_managed_command_read() {
    let mut e = support::engine(Options::v3_wallet());
    e.store_mut().set_markets(vec![market()]);
    let sent = command(&mut e, Command::FetchStpMode { command: COMMAND });
    managed(&sent, 1);
    let read = write(&sent, Method::Get, "/stpMode").clone();
    let done = answer(&mut e, &read, 200, r#"{"stpMode":"EXPIRE_TAKER"}"#);
    assert!(terminal(&done).expect("one terminal").is_ok());
}

#[test]
fn an_error_kind_survives_to_the_write_terminal() {
    let mut e = engine();
    let sent = command(&mut e, leverage());
    let setting = write(&sent, Method::Post, "/leverage").clone();
    let failed = answer(
        &mut e,
        &setting,
        400,
        r#"{"code":-4028,"msg":"Leverage 10 is not valid"}"#,
    );
    let error = terminal(&failed).expect("one terminal").unwrap_err();
    assert_eq!(error.outcome, Some(Outcome::Rejected), "{error:?}");
    assert_ne!(error.kind, Some(ErrorKind::Network));
}

/// With a read agent configured, every plan write carries the primary key and
/// a signed read carries the agent's.
#[test]
fn plan_writes_sign_with_the_primary_while_reads_use_the_read_agent() {
    for (command_of, method, path) in [
        (place as fn() -> Command, Method::Post, "/order"),
        (cancel as fn() -> Command, Method::Delete, "/order"),
        (leverage as fn() -> Command, Method::Post, "/leverage"),
    ] {
        let mut e = support::engine(Options::v1_with_read_agent());
        e.store_mut().set_markets(vec![market()]);
        e.store_mut().set_orders(vec![open("777")]);
        let sent = command(&mut e, command_of());
        managed(&sent, 1);
        let request = write(&sent, method, path);
        assert_eq!(
            header(request, "X-MBX-APIKEY"),
            Some("primary-key"),
            "{path}"
        );
    }
    let mut e = support::engine(Options::v1_with_read_agent());
    let sent = command(&mut e, Command::FetchAccount { command: COMMAND });
    let read = write(&sent, Method::Get, "/account");
    assert_eq!(header(read, "X-MBX-APIKEY"), Some("read-key"));
}

/// V3: the STP read signs as the read agent's signer; a plan write as the
/// primary's.
#[test]
fn a_v3_stp_read_signs_as_the_read_agent_and_a_write_as_the_primary() {
    let primary = "01".repeat(32);
    let agent = "02".repeat(32);
    let primary_address = safe_cex_crypto::ethereum::private_key_address(&primary).unwrap();
    let agent_address = safe_cex_crypto::ethereum::private_key_address(&agent).unwrap();
    let options = || Options {
        api_version: Some(safe_cex_venue_asterdex::sign::Version::V3),
        application_id: Some(primary_address.clone()),
        key: Some(primary_address.clone()),
        secret: Some(primary.clone()),
        read_key: Some(agent_address.clone()),
        read_secret: Some(agent.clone()),
        ..Options::default()
    };
    let mut e = support::engine(options());
    let sent = command(&mut e, Command::FetchStpMode { command: COMMAND });
    let read = write(&sent, Method::Get, "/stpMode");
    assert_eq!(query(&read.url, "signer"), Some(agent_address.as_str()));
    let mut e = support::engine(options());
    e.store_mut().set_markets(vec![market()]);
    let sent = command(&mut e, leverage());
    let setting = write(&sent, Method::Post, "/leverage");
    assert_eq!(
        query(&setting.url, "signer"),
        Some(primary_address.as_str())
    );
}

/// An Unknown placement fences its symbol: a retry is refused NotSent, a read
/// that started before the seal cannot release it, and the fresh read the
/// venue starts for it does.
#[test]
fn an_unknown_placement_fences_its_symbol_until_a_fresh_order_read() {
    let mut e = engine();
    // A proven placement leaves an order read in flight from before the seal.
    let sent = command(&mut e, place());
    let order = write(&sent, Method::Post, "/order").clone();
    let placed = answer(&mut e, &order, 200, r#"{"orderId":1}"#);
    let stale = placed
        .requests
        .iter()
        .find(|r| r.url.contains("/openOrders"))
        .expect("a read after the placement")
        .clone();
    // A gateway failure seals the next placement Unknown.
    let sent = command(&mut e, place());
    let lost = write(&sent, Method::Post, "/order").clone();
    let failed = answer(&mut e, &lost, 503, "upstream timeout");
    let error = terminal(&failed).expect("one terminal").unwrap_err();
    assert_eq!(error.outcome, Some(Outcome::Unknown), "{error:?}");
    let retry = |e: &mut Engine<AsterDex>| command(e, place());
    let refused = retry(&mut e);
    assert!(
        refused.requests.iter().all(|r| !r.url.contains("/order?")),
        "no second placement"
    );
    let error = terminal(&refused).expect("one terminal").unwrap_err();
    assert_eq!(error.outcome, Some(Outcome::NotSent), "{error:?}");
    assert!(error.legacy_text().contains("uncertain"), "{error:?}");
    // The read started before the seal completes: still fenced.
    let after_stale = answer(&mut e, &stale, 200, "[]");
    let still = retry(&mut e);
    assert_eq!(
        terminal(&still).expect("one terminal").unwrap_err().outcome,
        Some(Outcome::NotSent)
    );
    // The venue's fresh read starts at once and releases the fence.
    let fresh = failed
        .requests
        .iter()
        .chain(&refused.requests)
        .chain(&after_stale.requests)
        .chain(&still.requests)
        .find(|r| r.url.contains("/openOrders") && r.id != stale.id)
        .expect("a fresh order read without waiting for a timer")
        .clone();
    answer(&mut e, &fresh, 200, "[]");
    let admitted = retry(&mut e);
    write(&admitted, Method::Post, "/order");
}
