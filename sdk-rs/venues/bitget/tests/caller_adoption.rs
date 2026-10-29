//! Caller adoption slice 1 (`docs/plans/RUST_BITGET_CALLER_ADOPTION.md`): original
//! REDs for shared Context authority over candles, identity and account reads,
//! plus controls pinning the native behaviour the conversion must preserve.
mod support;
use safe_cex_engine::request_context::RoutingIntent;
use safe_cex_engine::request_context::{Claim, Error as ContextError, HttpAction, Prepared};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Method, Outcome, RequestId,
    RequestRoute, TimerId, VenueError,
};
use safe_cex_venue_bitget::BitgetFutures;
use std::collections::{HashMap, HashSet};
use support::*;

const DEADLINE_MS: f64 = 20_000.0;
const CANDLES: &str = r#"{"code":"00000","data":[["1000","1","3","1","2","10","20"]]}"#;
const IDENTITY: &str = r#"{"code":"00000","data":{"userId":"7","inviterId":""}}"#;
const EMPTY_ORDERS: &str = r#"{"code":"00000","data":{"entrustedList":[],"endId":""}}"#;

/// Every terminal published for `command`, in order.
fn terminals(h: &Harness, command: u32) -> Vec<Result<String, VenueError>> {
    let mut failures = h.failures.iter();
    h.done
        .iter()
        .filter_map(|(c, result)| {
            let terminal = match result {
                Ok(data) => Ok(data.clone()),
                Err(_) => Err(failures.next().expect("failure per Err").clone()),
            };
            (*c == command).then_some(terminal)
        })
        .collect()
}

fn count(h: &Harness, path: &str) -> usize {
    h.requests.iter().filter(|r| r.url.contains(path)).count()
}

fn header<'a>(request: &'a HttpRequest, name: &str) -> Option<&'a str> {
    request
        .headers
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.as_str())
}

/// Fires, in time order, only timers armed after `before` (including timers
/// those fire), up to and including `until`.
fn fire_new(h: &mut Harness, before: &HashSet<TimerId>, until: f64) {
    for _ in 0..10_000 {
        let next = h
            .timers
            .iter()
            .copied()
            .filter(|(timer, at)| !before.contains(timer) && *at <= until)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        let Some((timer, at)) = next else {
            h.now = h.now.max(until);
            return;
        };
        h.timers.retain(|(t, _)| *t != timer);
        h.now = h.now.max(at);
        h.event(Event::TimerFired { timer });
    }
    panic!("timer chain did not settle by {until}");
}

fn armed(h: &Harness) -> HashSet<TimerId> {
    h.timers.iter().map(|(timer, _)| *timer).collect()
}

/// A native answer for every ordinary account read (empty where it is a list).
fn account_answer(url: &str) -> Option<&'static str> {
    if url.contains("/all-position") {
        Some(POSITION)
    } else if url.contains("/account/accounts") {
        Some(ACCOUNT)
    } else if url.contains("/orders-pending") || url.contains("planType=profit_loss") {
        Some(EMPTY_ORDERS)
    } else if url.contains("planType=track_plan") {
        Some(r#"{"code":"00000","data":[]}"#)
    } else {
        None
    }
}

/// Fires every armed timer up to `until`, answering every account read except
/// those `hold` selects; returns the held requests with their issue times.
fn run(
    h: &mut Harness,
    until: f64,
    hold: impl Fn(&HttpRequest) -> bool,
) -> Vec<(HttpRequest, f64)> {
    let mut held = Vec::new();
    for _ in 0..100_000 {
        while let Some(i) = h
            .requests
            .iter()
            .position(|r| account_answer(&r.url).is_some())
        {
            let request = h.requests.remove(i);
            if hold(&request) {
                held.push((request, h.now));
                continue;
            }
            h.reply(request.id, 200, account_answer(&request.url).unwrap());
        }
        let next = h
            .timers
            .iter()
            .copied()
            .filter(|(_, at)| *at <= until)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        let Some((timer, at)) = next else {
            h.now = h.now.max(until);
            return held;
        };
        h.timers.retain(|(t, _)| *t != timer);
        h.now = h.now.max(at);
        h.event(Event::TimerFired { timer });
    }
    panic!("timer chain did not settle by {until}");
}

fn fetch_candles(h: &mut Harness, command: u32, interval: safe_cex_engine::model::Interval) {
    h.command(Command::FetchCandles {
        command,
        symbol: "BTCUSDT".into(),
        interval,
        limit: Some(2),
        from_ms: Some(1000.0),
        to_ms: Some(1_000_000.0),
    });
}

fn read_options() -> FixtureOptions {
    FixtureOptions {
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        passphrase: Some("primary-pass".into()),
        read_credentials: Some(safe_cex_engine::ReadCredentials {
            key: "read-key".into(),
            secret: "read-secret".into(),
            passphrase: Some("read-pass".into()),
        }),
        ..Default::default()
    }
}

/// A ready Classic account whose GETs may use a read-only key.
fn classic_with_read_key() -> Harness {
    let mut h = Harness::started(read_options());
    h.load_public();
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"40084","msg":"Classic Account mode","data":null}"#,
    );
    h.respond("/all-position", 200, POSITION);
    h.respond("/account/accounts", 200, ACCOUNT);
    h.orders(OPEN_ORDER);
    h.login();
    h
}

// ---------------------------------------------------------------------------
// Original REDs: each fails on today's source by assertion.
// ---------------------------------------------------------------------------

#[test]
fn unanswered_candle_read_fails_at_the_shared_deadline() {
    let mut h = Harness::public();
    let before = armed(&h);
    let start = h.now;
    fetch_candles(&mut h, 7, safe_cex_engine::model::Interval::M1);
    let request = h.take("/candles");
    fire_new(&mut h, &before, start + DEADLINE_MS);
    let failed = terminals(&h, 7);
    assert_eq!(
        failed.len(),
        1,
        "an unanswered candle read needs exactly one terminal by the 20 s shared deadline"
    );
    let error = failed[0].as_ref().expect_err("the deadline is a failure");
    assert_eq!(error.kind, Some(ErrorKind::Timeout));
    h.reply(request.id, 200, CANDLES);
    assert_eq!(terminals(&h, 7).len(), 1, "a late reply adds no terminal");
}

#[test]
fn candle_reads_beyond_shared_capacity_are_refused_before_io() {
    let mut h = Harness::public();
    for command in 1_000..1_256 {
        fetch_candles(&mut h, command, safe_cex_engine::model::Interval::M1);
    }
    assert_eq!(
        count(&h, "/candles"),
        256,
        "256 reads are admitted and sent"
    );
    fetch_candles(&mut h, 1_256, safe_cex_engine::model::Interval::M1);
    assert_eq!(
        count(&h, "/candles"),
        256,
        "the candle read beyond shared capacity must be refused before any HTTP"
    );
    let failed = terminals(&h, 1_256);
    assert_eq!(failed.len(), 1, "one typed capacity failure");
    let error = failed[0].as_ref().expect_err("capacity is a failure");
    let expected = ContextError::Capacity.venue_error();
    assert_eq!(error.message, expected.message);
    assert_eq!(error.kind, expected.kind);
    let first = h.take("/candles");
    h.reply(first.id, 200, CANDLES);
    fetch_candles(&mut h, 1_257, safe_cex_engine::model::Interval::M1);
    assert_eq!(
        count(&h, "/candles"),
        256,
        "an answered read frees one slot"
    );
}

#[test]
fn unanswered_identity_read_fails_at_the_shared_deadline() {
    let mut h = Harness::classic();
    let before = armed(&h);
    let start = h.now;
    h.command(Command::FetchAccount { command: 42 });
    let request = h.take("/api/v2/spot/account/info");
    fire_new(&mut h, &before, start + DEADLINE_MS);
    let failed = terminals(&h, 42);
    assert_eq!(
        failed.len(),
        1,
        "an unanswered identity read needs exactly one terminal by the 20 s shared deadline"
    );
    let error = failed[0].as_ref().expect_err("the deadline is a failure");
    assert_eq!(error.kind, Some(ErrorKind::Timeout));
    let diagnostics: Vec<_> = h.command_errors.iter().filter(|(c, _)| *c == 42).collect();
    assert_eq!(diagnostics.len(), 1, "the correlated diagnostic is kept");
    assert_eq!(diagnostics[0].1, error.message);
    h.reply(request.id, 200, IDENTITY);
    assert_eq!(terminals(&h, 42).len(), 1, "a late reply adds no terminal");
}

#[test]
fn identity_reads_beyond_shared_capacity_are_refused_before_io() {
    let mut h = Harness::classic();
    // Background account reads share the registry: settle the login
    // reconciliation so none still holds a slot.
    while let Some(i) = h
        .requests
        .iter()
        .position(|r| account_answer(&r.url).is_some())
    {
        let request = h.requests.remove(i);
        h.reply(request.id, 200, account_answer(&request.url).unwrap());
    }
    assert!(h.requests.is_empty(), "{:?}", h.requests);
    for command in 1_000..1_256 {
        h.command(Command::FetchAccount { command });
    }
    assert_eq!(count(&h, "/spot/account/info"), 256);
    h.command(Command::FetchAccount { command: 1_256 });
    assert_eq!(
        count(&h, "/spot/account/info"),
        256,
        "the identity read beyond shared capacity must be refused before any HTTP"
    );
    let failed = terminals(&h, 1_256);
    assert_eq!(failed.len(), 1, "one typed capacity failure");
    let error = failed[0].as_ref().expect_err("capacity is a failure");
    let expected = ContextError::Capacity.venue_error();
    assert_eq!(error.message, expected.message);
    assert_eq!(error.kind, expected.kind);
}

#[test]
fn unanswered_balance_read_is_retired_by_the_shared_deadline_and_polled_again() {
    let mut h = Harness::classic();
    let balance = |r: &HttpRequest| r.url.contains("/account/accounts");
    let start = h.now;
    let held = run(&mut h, start + 10_000.0, balance);
    assert_eq!(held.len(), 1, "the poll issues one balance read");
    let issued = held[0].1;
    let again = run(&mut h, issued + DEADLINE_MS + 10_000.0, balance);
    assert!(
        !again.is_empty(),
        "an unanswered balance read must be retired by the 20 s shared deadline so a later \
         poll reads the balance again"
    );
    assert!(
        again[0].1 >= issued + DEADLINE_MS,
        "not before the deadline"
    );
}

#[test]
fn unanswered_settings_probe_ends_at_the_shared_deadline_into_classic_detection() {
    let mut h = Harness::private_started();
    let probe = h.take("/account/settings");
    let issued = h.now;
    let positions = run(&mut h, issued + DEADLINE_MS, |r| {
        r.url.contains("/all-position")
    });
    assert!(
        !positions.is_empty(),
        "an unanswered settings probe must end at the 20 s shared deadline and take the \
         existing transport-failure route (Classic detection)"
    );
    // A late UTA answer to the retired probe cannot switch the account route.
    h.reply(
        probe.id,
        200,
        r#"{"code":"00000","data":{"holdMode":"one_way_mode"}}"#,
    );
    assert!(
        !h.sockets
            .iter()
            .any(|(_, url)| url.contains("/v3/ws/private"))
    );
    assert_eq!(count(&h, "/current-position"), 0);
}

// ---------------------------------------------------------------------------
// Controls: pass on today's source and must pass after the conversion.
// ---------------------------------------------------------------------------

#[test]
fn candle_read_is_exact_public_unsigned_egress() {
    let mut h = Harness::public();
    fetch_candles(&mut h, 1, safe_cex_engine::model::Interval::M1);
    let request = h.take("/candles");
    assert_eq!(request.method, Method::Get);
    assert_eq!(
        request.url,
        "https://api.bitget.com/api/v2/mix/market/candles?symbol=BTCUSDT&granularity=1m\
         &endTime=121000&productType=usdt-futures&limit=2&startTime=1000"
    );
    assert_eq!(
        request.headers,
        vec![("X-CHANNEL-API-CODE", "Tealstreet".to_owned())],
        "broker attribution only; never ACCESS-* on a public read"
    );
    assert_eq!(request.body, None);
    assert_eq!(request.route, None);
    h.reply(request.id, 200, CANDLES);
    let done = terminals(&h, 1);
    assert_eq!(done.len(), 1);
    let rows = done[0].as_ref().unwrap();
    assert!(
        rows.contains("\"timestamp\":1.0") && rows.contains("\"volume\":20.0"),
        "{rows}"
    );
}

/// Command, native reply (`None` is a transport failure) and the expected text.
type Case = (u32, Option<(u16, &'static str)>, &'static str);

#[test]
fn candle_failures_keep_text_kind_and_outcome() {
    let mut h = Harness::public();
    fetch_candles(&mut h, 1, safe_cex_engine::model::Interval::H8);
    assert_eq!(
        count(&h, "/candles"),
        0,
        "unsupported interval sends nothing"
    );
    let cases: [Case; 5] = [
        (
            2,
            Some((500, r#"{"code":"00000","data":[]}"#)),
            r#"Bitget HTTP 500: {"code":"00000","data":[]}"#,
        ),
        (
            3,
            Some((200, r#"{"code":"40001","msg":"limited","data":null}"#)),
            "Bitget 40001: limited",
        ),
        (
            4,
            Some((
                200,
                r#"{"code":"00000","data":[["1000","bad","3","1","2","10","20"]]}"#,
            )),
            "invalid Bitget REST record",
        ),
        (
            5,
            Some((200, r#"{"code":"00000","data":{}}"#)),
            "Bitget successful response has no expected list",
        ),
        (6, None, "connection reset"),
    ];
    for (command, reply, _) in cases {
        fetch_candles(&mut h, command, safe_cex_engine::model::Interval::M1);
        let request = h.take("/candles");
        match reply {
            Some((status, body)) => h.reply(request.id, status, body),
            None => h.event(Event::HttpFailed {
                request: request.id,
                error: "connection reset",
            }),
        }
    }
    let mut expected = vec![(1, "Bitget does not support 8h candles".to_owned())];
    expected.extend(cases.iter().map(|(c, _, text)| (*c, (*text).to_owned())));
    for (command, text) in expected {
        let failed = terminals(&h, command);
        assert_eq!(failed.len(), 1, "command {command}: one terminal");
        let error = failed[0].as_ref().expect_err(&text);
        assert_eq!(error.message, text, "command {command}");
        assert_eq!(
            error.kind, None,
            "command {command}: untyped native failure"
        );
        assert_eq!(error.outcome, None, "command {command}");
        assert!(
            !h.command_errors.iter().any(|(c, _)| *c == command),
            "command {command}: hard failure without a soft diagnostic"
        );
    }
}

#[test]
fn identity_read_keeps_read_key_route_and_failure_shape() {
    let mut h = classic_with_read_key();
    h.command(Command::FetchAccount { command: 42 });
    let request = h.take("/api/v2/spot/account/info");
    assert_eq!(
        request.url,
        "https://api.bitget.com/api/v2/spot/account/info"
    );
    assert_eq!(request.method, Method::Get);
    assert_eq!(request.body, None);
    assert_eq!(request.route, None);
    assert_eq!(header(&request, "ACCESS-KEY"), Some("read-key"));
    assert_eq!(header(&request, "ACCESS-PASSPHRASE"), Some("read-pass"));
    assert_eq!(header(&request, "X-CHANNEL-API-CODE"), Some("Tealstreet"));
    h.reply(request.id, 200, IDENTITY);
    assert_eq!(
        terminals(&h, 42),
        vec![Ok(
            r#"{"userId":"7","subId":null,"affiliateId":null}"#.to_owned()
        )]
    );

    h.command(Command::FetchAccount { command: 43 });
    let request = h.take("/api/v2/spot/account/info");
    h.reply(request.id, 500, "down");
    let failed = terminals(&h, 43);
    assert_eq!(failed.len(), 1);
    let error = failed[0].as_ref().unwrap_err();
    assert_eq!(error.message, "Bitget account identity HTTP 500");
    assert_eq!(error.kind, None);
    let diagnostics: Vec<_> = h.command_errors.iter().filter(|(c, _)| *c == 43).collect();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].1, error.message,
        "diagnostic precedes the same failure"
    );

    h.command(Command::FetchAccount { command: 44 });
    let request = h.take("/api/v2/spot/account/info");
    h.event(Event::HttpFailed {
        request: request.id,
        error: "connection reset",
    });
    let failed = terminals(&h, 44);
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].as_ref().unwrap_err().message, "connection reset");

    // Without a read key, the identity read signs with the primary key.
    let mut h = Harness::classic();
    h.command(Command::FetchAccount { command: 45 });
    let request = h.take("/api/v2/spot/account/info");
    assert_eq!(header(&request, "ACCESS-KEY"), Some("key"));

    // Public-only: refused before any HTTP, with the correlated diagnostic.
    let mut h = Harness::public();
    h.command(Command::FetchAccount { command: 46 });
    assert_eq!(count(&h, "/spot/account/info"), 0);
    let failed = terminals(&h, 46);
    assert_eq!(failed.len(), 1);
    let error = failed[0].as_ref().unwrap_err();
    assert_eq!(
        error.message,
        "Bitget account identity requires credentials"
    );
    assert!(
        h.command_errors
            .iter()
            .any(|(c, m)| *c == 46 && *m == error.message)
    );
}

#[test]
fn classic_account_reads_keep_exact_urls_and_the_read_key() {
    let mut h = Harness::started(read_options());
    h.load_public();
    let probe = h.take("/account/settings");
    assert_eq!(probe.url, "https://api.bitget.com/api/v3/account/settings");
    assert_eq!(header(&probe, "ACCESS-KEY"), Some("read-key"));
    h.reply(
        probe.id,
        200,
        r#"{"code":"40084","msg":"Classic Account mode","data":null}"#,
    );
    for url in [
        "https://api.bitget.com/api/v2/mix/position/all-position?productType=usdt-futures&marginCoin=USDT",
        "https://api.bitget.com/api/v2/mix/account/accounts?productType=usdt-futures",
        "https://api.bitget.com/api/v2/mix/order/orders-pending?productType=usdt-futures&status=live&limit=100",
    ] {
        let request = h
            .requests
            .iter()
            .find(|r| r.url == url)
            .unwrap_or_else(|| panic!("no {url}: {:?}", h.requests));
        assert_eq!(request.method, Method::Get);
        assert_eq!(request.body, None);
        assert_eq!(request.route, None);
        assert_eq!(header(request, "ACCESS-KEY"), Some("read-key"), "{url}");
    }
    h.respond("/all-position", 200, POSITION);
    h.respond("/account/accounts", 200, ACCOUNT);
    h.respond("/orders-pending", 200, OPEN_ORDER);
    let partial = h.take("status=partially_filled");
    assert_eq!(
        partial.url,
        "https://api.bitget.com/api/v2/mix/order/orders-pending?productType=usdt-futures&status=partially_filled&limit=100"
    );
    assert_eq!(header(&partial, "ACCESS-KEY"), Some("read-key"));
    h.reply(partial.id, 200, EMPTY_ORDERS);
    let plan = h.take("planType=profit_loss");
    assert_eq!(
        plan.url,
        "https://api.bitget.com/api/v2/mix/order/orders-plan-pending?productType=usdt-futures&planType=profit_loss&limit=100"
    );
    assert_eq!(header(&plan, "ACCESS-KEY"), Some("read-key"));
    h.reply(
        plan.id,
        200,
        r#"{"code":"00000","data":{"entrustedList":[]}}"#,
    );
    h.respond("planType=track_plan", 200, r#"{"code":"00000","data":[]}"#);
    let loaded = h.engine.store().loaded();
    assert!(loaded.balance && loaded.positions && loaded.orders);
}

#[test]
fn uta_account_reads_keep_their_routes() {
    let mut h = Harness::private_started();
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"00000","data":{"holdMode":"one_way_mode"}}"#,
    );
    for url in [
        "https://api.bitget.com/api/v3/position/current-position?category=USDT-FUTURES",
        "https://api.bitget.com/api/v3/account/assets",
        "https://api.bitget.com/api/v3/trade/unfilled-orders?category=USDT-FUTURES&limit=100",
    ] {
        let request = h
            .requests
            .iter()
            .find(|r| r.url == url)
            .unwrap_or_else(|| panic!("no {url}: {:?}", h.requests));
        assert_eq!(request.method, Method::Get);
        assert_eq!(header(request, "ACCESS-KEY"), Some("key"), "{url}");
    }
    assert!(!h.requests.iter().any(|r| r.url.contains("/api/v2/mix/")));
    assert!(
        h.sockets
            .iter()
            .any(|(_, url)| url.ends_with("/v3/ws/private"))
    );
}

/// `support::collect` with each managed preparation's routing intent recorded.
fn released_with_intents(
    engine: &mut Engine<BitgetFutures>,
    now: f64,
) -> (Vec<Action>, HashMap<RequestId, RoutingIntent>) {
    engine.prepare_dispatch(now);
    let mut queue: std::collections::VecDeque<_> = engine.drain_actions().collect();
    let mut intents = HashMap::new();
    let mut actions = Vec::new();
    while let Some(action) = queue.pop_front() {
        let Action::ManagedHttp(managed) = action else {
            actions.push(action);
            continue;
        };
        match *managed {
            HttpAction::Prepare {
                preparation,
                intent,
                request,
            } => {
                if engine.http_preparation_pending(preparation) {
                    intents.insert(request.id, intent);
                    engine.http_prepared(
                        now,
                        preparation,
                        Prepared {
                            id: request.id.0,
                            expires_at: None,
                        },
                    );
                }
            }
            HttpAction::Bind {
                permit, request, ..
            } => {
                if let Ok(Claim::Released(_)) = engine.claim_http(now, permit) {
                    actions.push(Action::Http(request));
                }
            }
            HttpAction::BindGroup { .. } => {
                panic!("Bitget fixture declares no Mutation barriers")
            }
            HttpAction::Cancel { .. } => {}
        }
        for action in engine.drain_actions().collect::<Vec<_>>().into_iter().rev() {
            queue.push_front(action);
        }
    }
    (actions, intents)
}

fn primary_route(request: &HttpRequest, intents: &HashMap<RequestId, RoutingIntent>) -> bool {
    request.route == Some(RequestRoute::Primary)
        || intents.get(&request.id) == Some(&RoutingIntent::Primary)
}

#[test]
fn degraded_balance_recovery_keeps_the_primary_key_and_route() {
    let mut h = classic_with_read_key();
    h.command(Command::PlaceOrders {
        command: 1,
        orders: vec![
            serde_json::from_str(
                r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.002,"price":90}"#,
            )
            .unwrap(),
        ],
    });
    let write = h.take("/batch-place-order");
    assert_eq!(
        header(&write, "ACCESS-KEY"),
        Some("primary-key"),
        "writes stay primary"
    );
    let before = armed(&h);
    h.reply(
        write.id,
        200,
        r#"{"code":"00000","data":{"successList":[{"orderId":"9"}],"failureList":[]}}"#,
    );
    while let Some(i) = h
        .requests
        .iter()
        .position(|r| account_answer(&r.url).is_some())
    {
        let request = h.requests.remove(i);
        h.reply(request.id, 200, account_answer(&request.url).unwrap());
    }
    assert!(h.requests.is_empty(), "{:?}", h.requests);
    // No authenticated private payload arrives within five seconds of the write.
    let (timer, at) = h
        .timers
        .iter()
        .copied()
        .find(|(t, at)| !before.contains(t) && (*at - h.now - 5_000.0).abs() < 0.01)
        .expect("write check");
    h.timers.retain(|(t, _)| *t != timer);
    h.now = at;
    h.engine.handle(at, Event::TimerFired { timer });
    let (actions, intents) = released_with_intents(&mut h.engine, at);
    h.absorb(actions);
    let balance = h.take("/account/accounts");
    assert_eq!(header(&balance, "ACCESS-KEY"), Some("primary-key"));
    assert!(
        primary_route(&balance, &intents),
        "degraded balance uses primary egress"
    );
    let positions = h.take("/all-position");
    assert_eq!(header(&positions, "ACCESS-KEY"), Some("read-key"));
    assert!(
        !primary_route(&positions, &intents),
        "other reads keep the read route"
    );
}

#[test]
fn trade_entry_refusals_keep_text_and_not_sent() {
    let refused = |h: &Harness, command: u32, text: &str| {
        let failed = terminals(h, command);
        assert_eq!(failed.len(), 1, "{text}: exactly one terminal");
        let error = failed[0].as_ref().expect_err(text);
        assert_eq!(error.message, text);
        assert_eq!(error.kind, None, "{text}");
        assert_eq!(error.outcome, Some(Outcome::NotSent), "{text}");
    };
    let order = |json: &str| serde_json::from_str(json).unwrap();

    let mut h = Harness::public();
    h.command(Command::PlaceOrders {
        command: 20,
        orders: vec![order(
            r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.002,"price":90}"#,
        )],
    });
    assert!(h.requests.is_empty(), "refused before any HTTP");
    refused(&h, 20, "Bitget credentials are required");

    let mut h = Harness::classic();
    h.requests.clear();
    h.command(Command::PlaceOrders {
        command: 21,
        orders: vec![order(r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.002,"price":90,"bybitSlippageToleranceTicks":2}"#)],
    });
    h.command(Command::PlaceOrders {
        command: 22,
        orders: vec![order(
            r#"{"symbol":"NOPEUSDT","type":"limit","side":"buy","amount":1,"price":90}"#,
        )],
    });
    assert!(h.requests.is_empty(), "refused before any HTTP");
    refused(&h, 21, "Bybit order controls are not supported by Bitget");
    refused(&h, 22, "Market NOPEUSDT not found");
    // No residual lock or queued job: a same-symbol placement still writes.
    h.command(Command::PlaceOrders {
        command: 23,
        orders: vec![order(
            r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.002,"price":90}"#,
        )],
    });
    assert_eq!(count(&h, "/batch-place-order"), 1);
    assert!(terminals(&h, 23).is_empty());
}

#[test]
fn late_replies_after_a_terminal_are_inert() {
    let mut h = Harness::classic();
    fetch_candles(&mut h, 1, safe_cex_engine::model::Interval::M1);
    let candles = h.take("/candles");
    h.reply(candles.id, 200, CANDLES);
    h.command(Command::FetchAccount { command: 2 });
    let identity = h.take("/spot/account/info");
    h.reply(identity.id, 200, IDENTITY);
    let (done, failures, errors) = (h.done.len(), h.failures.len(), h.command_errors.len());
    h.reply(candles.id, 500, "late");
    h.reply(identity.id, 500, "late");
    h.event(Event::HttpFailed {
        request: candles.id,
        error: "late",
    });
    assert_eq!(
        (h.done.len(), h.failures.len(), h.command_errors.len()),
        (done, failures, errors)
    );
    assert_eq!(terminals(&h, 1).len(), 1);
    assert_eq!(terminals(&h, 2).len(), 1);
}
