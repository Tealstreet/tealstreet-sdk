//! FetchCandles under the shared command Context (OKX caller slice 2).
//! REDs pin the shared deadline, capacity and public managed route; controls
//! pin native URLs, paging, soft-empty failures, refusals and public egress.
#![allow(dead_code)]
mod support;
use crate::support::FixtureOptions as Options;
use crate::support::*;
use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{HttpAction, RoutingIntent};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, Notice, Outcome, Response, TimerId, VenueError,
};
use safe_cex_venue_okx::Okx;

const NOW: f64 = 1_800_000_000_000.0;
const DEADLINE_MS: f64 = 20_000.0;
const ROW_NEW: &str = r#"["1799999940000","3","5","2","4","0","0","30","0"]"#;
const ROW_OLD: &str = r#"["1799999880000","1","3","0.5","2","0","0","20","0"]"#;

fn fetch(
    e: &mut Engine<Okx>,
    command: u32,
    symbol: &str,
    interval: Interval,
    limit: Option<u32>,
    (from_ms, to_ms): (Option<f64>, Option<f64>),
    now: f64,
) -> Vec<Action> {
    e.handle(
        now,
        Event::Command(Command::FetchCandles {
            command,
            symbol: symbol.into(),
            interval,
            limit,
            from_ms,
            to_ms,
        }),
    );
    prepared(e, now)
}

fn fetch_ten(e: &mut Engine<Okx>, command: u32, now: f64) -> Vec<Action> {
    fetch(
        e,
        command,
        "BTCUSDT",
        Interval::M1,
        Some(10),
        (None, None),
        now,
    )
}

fn http(out: &[Action]) -> Vec<&safe_cex_engine::HttpRequest> {
    out.iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect()
}

fn sends_io(out: &[Action]) -> bool {
    out.iter()
        .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
}

fn candle_read(out: &[Action]) -> Action {
    out.iter()
        .find(|a| matches!(a, Action::Http(r) if r.url.contains("/api/v5/market/")))
        .cloned()
        .expect("one candle read")
}

fn terminals(out: &[Action], command: u32) -> Vec<Result<String, VenueError>> {
    out.iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone { command: c, result }) if *c == command => {
                Some(Ok(result.clone()))
            }
            Action::Notice(Notice::CommandFailed { command: c, error }) if *c == command => {
                Some(Err(error.clone()))
            }
            _ => None,
        })
        .collect()
}

fn diagnostics(out: &[Action], command: u32) -> Vec<String> {
    out.iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandError {
                command: c,
                message,
            }) if *c == command => Some(message.clone()),
            _ => None,
        })
        .collect()
}

/// Native OKX candle failure after send: one correlated diagnostic, then `[]`.
fn assert_soft_empty(out: &[Action], command: u32, message: &str) {
    assert_eq!(
        terminals(out, command),
        vec![Ok("[]".to_owned())],
        "exactly one empty-result terminal for {message:?}"
    );
    assert_eq!(diagnostics(out, command), vec![message.to_owned()]);
    let error = out
        .iter()
        .position(|a| matches!(a, Action::Notice(Notice::CommandError { command: c, .. }) if *c == command))
        .unwrap();
    let done = out
        .iter()
        .position(|a| matches!(a, Action::Notice(Notice::CommandDone { command: c, .. }) if *c == command))
        .unwrap();
    assert!(error < done, "the diagnostic precedes its terminal");
}

fn respond(e: &mut Engine<Okx>, action: &Action, status: u16, body: &str, now: f64) -> Vec<Action> {
    let Action::Http(r) = action else {
        panic!("expected request")
    };
    e.handle(
        now,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
    prepared(e, now)
}

fn answer(e: &mut Engine<Okx>, action: &Action, body: &str, now: f64) -> Vec<Action> {
    respond(e, action, 200, body, now)
}

/// Fires, in time order, every timer armed by `out` or by the timers it
/// fires, up to and including `until`; returns all actions they produce.
fn fire_armed(e: &mut Engine<Okx>, out: &[Action], until: f64) -> Vec<Action> {
    fn track(timers: &mut Vec<(TimerId, f64)>, out: &[Action]) {
        for action in out {
            match action {
                Action::SetTimer { timer, at_ms } => timers.push((*timer, *at_ms)),
                Action::ClearTimer { timer } => timers.retain(|(t, _)| t != timer),
                _ => {}
            }
        }
    }
    let mut timers = Vec::new();
    track(&mut timers, out);
    let mut seen = Vec::new();
    for _ in 0..10_000 {
        timers.sort_by(|a, b| a.1.total_cmp(&b.1));
        let Some(&(timer, at)) = timers.first() else {
            return seen;
        };
        if at > until {
            return seen;
        }
        timers.remove(0);
        e.handle(at, Event::TimerFired { timer });
        let out = prepared(e, at);
        track(&mut timers, &out);
        seen.extend(out);
    }
    panic!("timer chain did not settle by {until}");
}

/// Answers every startup GET (public or private) until the venue goes quiet.
fn boot(options: Options) -> Engine<Okx> {
    let mut e = engine(options);
    e.start(0.0);
    let mut now = 0.0;
    let mut pending: Vec<Action> = prepared(&mut e, now)
        .into_iter()
        .filter(|a| matches!(a, Action::Http(_)))
        .collect();
    while !pending.is_empty() {
        let a = take_http(&mut pending);
        let Action::Http(r) = &a else { unreachable!() };
        let body = if r.url.contains("instruments?instType=SWAP") {
            envelope(MARKET)
        } else if r.url.contains("tickers?instType=SWAP") {
            envelope(TICKER)
        } else if r.url.contains("/api/v5/account/config") {
            envelope(r#"{"posMode":"net_mode","acctLv":"2"}"#)
        } else if r.url.contains("account-position-risk") {
            envelope(r#"{"balData":[{"ccy":"USDT","eq":"5000"}]}"#)
        } else {
            envelope("")
        };
        now += 1.0;
        reply(&mut e, &a, &body, now);
        pending.extend(
            prepared(&mut e, now)
                .into_iter()
                .filter(|a| matches!(a, Action::Http(_))),
        );
    }
    e
}

fn ready() -> Engine<Okx> {
    boot(Options::default())
}

fn header<'a>(request: &'a safe_cex_engine::HttpRequest, name: &str) -> Option<&'a str> {
    request
        .headers
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.as_str())
}

// ---------------------------------------------------------------- REDs

#[test]
fn unanswered_candle_read_ends_at_the_shared_deadline() {
    let mut e = ready();
    let out = fetch_ten(&mut e, 7, NOW);
    let request = candle_read(&out);
    let mut seen = out.clone();
    seen.extend(fire_armed(&mut e, &out, NOW + DEADLINE_MS));
    assert_eq!(
        terminals(&seen, 7).len(),
        1,
        "an unanswered candle read needs exactly one terminal by the 20 s shared deadline"
    );
    assert_soft_empty(&seen, 7, "HTTP request timed out");
    let late = answer(
        &mut e,
        &request,
        &envelope(ROW_NEW),
        NOW + DEADLINE_MS + 1.0,
    );
    assert!(terminals(&late, 7).is_empty(), "no second terminal");
    assert!(
        !sends_io(&late),
        "a late reply cannot start the fallback leg"
    );
}

#[test]
fn history_fallback_leg_keeps_the_original_deadline() {
    let mut e = ready();
    let out = fetch_ten(&mut e, 8, NOW);
    let first = candle_read(&out);
    let half = NOW + DEADLINE_MS / 2.0;
    let out = answer(&mut e, &first, &envelope(""), half);
    let fallback = candle_read(&out);
    let Action::Http(leg) = &fallback else {
        unreachable!()
    };
    assert!(leg.url.contains("/api/v5/market/history-candles?"));
    assert_eq!(
        leg.timeout_ms,
        Some((DEADLINE_MS / 2.0) as u32),
        "the fallback leg is bounded by the command's original deadline, not a fresh one"
    );
    let seen = fire_armed(&mut e, &out, NOW + DEADLINE_MS);
    assert_soft_empty(&seen, 8, "HTTP request timed out");
    let late = answer(
        &mut e,
        &fallback,
        &envelope(ROW_NEW),
        NOW + DEADLINE_MS + 1.0,
    );
    assert!(terminals(&late, 8).is_empty(), "no second terminal");
}

#[test]
fn candle_reads_beyond_shared_capacity_are_refused_before_io() {
    use safe_cex_engine::request_context::Error;
    let mut e = ready();
    let mut reads = Vec::new();
    for command in 1_000..1_256 {
        let out = fetch_ten(&mut e, command, NOW);
        assert_eq!(http(&out).len(), 1, "read {command} is admitted and sent");
        reads.push(candle_read(&out));
    }
    let out = fetch_ten(&mut e, 1_256, NOW);
    assert!(
        !sends_io(&out),
        "the candle read beyond shared capacity must be refused before any HTTP"
    );
    let failed = terminals(&out, 1_256);
    assert_eq!(failed.len(), 1, "one typed capacity failure");
    let error = failed[0].as_ref().expect_err("capacity is a failure");
    let expected = Error::Capacity.venue_error();
    assert_eq!(error.message, expected.message);
    assert_eq!(error.kind, expected.kind);
    let out = answer(&mut e, &reads[0], &envelope(ROW_NEW), NOW + 1.0);
    assert_eq!(terminals(&out, 1_000).len(), 1);
    let out = fetch_ten(&mut e, 1_257, NOW + 2.0);
    assert_eq!(
        http(&out).len(),
        1,
        "an answered read frees capacity for the next"
    );
}

#[test]
fn candle_read_is_prepared_as_a_bounded_public_route() {
    let mut e = boot(Options {
        key: Some("test-key".into()),
        secret: Some("test-secret".into()),
        passphrase: Some("test-passphrase".into()),
        ..Options::default()
    });
    e.handle(
        NOW,
        Event::Command(Command::FetchCandles {
            command: 9,
            symbol: "BTCUSDT".into(),
            interval: Interval::M1,
            limit: Some(10),
            from_ms: None,
            to_ms: None,
        }),
    );
    let raw: Vec<_> = e.drain_actions().collect();
    assert!(
        !raw.iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("/api/v5/market/"))),
        "a candle read must await managed route preparation instead of raw send"
    );
    let prepared: Vec<_> = raw
        .iter()
        .filter_map(|a| match a {
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare {
                    intent, request, ..
                } => Some((*intent, request.clone())),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(prepared.len(), 1, "one managed candle preparation");
    let (intent, request) = &prepared[0];
    assert_eq!(*intent, RoutingIntent::Public, "candles are public egress");
    assert_eq!(request.timeout_ms, Some(DEADLINE_MS as u32));
    assert!(
        request.url.ends_with(&format!(
            "/api/v5/market/candles?instId=BTC-USDT-SWAP&bar=1m&limit=10&before={}&after={}",
            NOW - 600_000.0,
            NOW
        )),
        "{}",
        request.url
    );
    assert!(
        request
            .headers
            .iter()
            .all(|(k, _)| !k.starts_with("OK-ACCESS"))
    );
}

// ------------------------------------------------------------ controls

#[test]
fn candle_urls_limits_and_history_fallback_are_exact() {
    let mut e = ready();
    let out = fetch_ten(&mut e, 1, NOW);
    let reads = http(&out);
    assert_eq!(reads.len(), 1);
    assert_eq!(
        reads[0].url,
        format!(
            "https://www.okx.com/api/v5/market/candles?instId=BTC-USDT-SWAP&bar=1m&limit=10&before={}&after={}",
            NOW - 600_000.0,
            NOW
        )
    );
    let first = candle_read(&out);
    let out = answer(&mut e, &first, &envelope(""), NOW + 1.0);
    let reads = http(&out);
    assert_eq!(reads.len(), 1, "an empty recent page falls back once");
    assert_eq!(
        reads[0].url,
        format!(
            "https://www.okx.com/api/v5/market/history-candles?instId=BTC-USDT-SWAP&bar=1m&limit=10&before={}&after={}",
            NOW - 600_000.0,
            NOW
        )
    );
    assert!(terminals(&out, 1).is_empty());
    let fallback = candle_read(&out);
    let out = answer(
        &mut e,
        &fallback,
        &format!(r#"{{"code":"0","msg":"","data":[{ROW_NEW},{ROW_OLD}]}}"#),
        NOW + 2.0,
    );
    assert!(!sends_io(&out));
    assert!(diagnostics(&out, 1).is_empty());
    let done = terminals(&out, 1);
    assert_eq!(done.len(), 1);
    let rows: serde_json::Value = serde_json::from_str(done[0].as_ref().unwrap()).unwrap();
    assert_eq!(
        rows,
        serde_json::json!([
            {"timestamp":1799999880.0,"open":1.0,"high":3.0,"low":0.5,"close":2.0,"volume":20.0},
            {"timestamp":1799999940.0,"open":3.0,"high":5.0,"low":2.0,"close":4.0,"volume":30.0}
        ]),
        "rows are sorted ascending"
    );

    // A non-empty recent page completes without a fallback.
    let out = fetch_ten(&mut e, 2, NOW);
    let out = answer(&mut e, &candle_read(&out), &envelope(ROW_NEW), NOW + 1.0);
    assert!(!sends_io(&out));
    assert_eq!(terminals(&out, 2).len(), 1);

    // Limits clamp to 300 recent and 100 historical; no limit uses from_ms.
    let out = fetch(
        &mut e,
        3,
        "BTCUSDT",
        Interval::M1,
        Some(1_000),
        (None, None),
        NOW,
    );
    assert!(http(&out)[0].url.contains("&limit=300&"));
    let out = answer(&mut e, &candle_read(&out), &envelope(""), NOW + 1.0);
    assert!(
        http(&out)[0].url.contains("/history-candles?")
            && http(&out)[0].url.contains("&limit=100&")
    );
    let out = fetch(
        &mut e,
        4,
        "BTCUSDT",
        Interval::H1,
        None,
        (Some(NOW - 7_200_000.0), Some(NOW - 3_600_000.0)),
        NOW,
    );
    assert!(http(&out)[0].url.ends_with(&format!(
        "bar=1H&limit=300&before={}&after={}",
        NOW - 7_200_000.0,
        NOW - 3_600_000.0
    )));

    // An end older than 1440 bars still reads recent first, then forces history.
    let old = NOW - 1_441.0 * 60_000.0;
    let out = fetch(
        &mut e,
        5,
        "BTCUSDT",
        Interval::M1,
        Some(10),
        (None, Some(old)),
        NOW,
    );
    assert!(http(&out)[0].url.contains("/market/candles?"));
    let out = answer(&mut e, &candle_read(&out), &envelope(ROW_NEW), NOW + 1.0);
    assert!(http(&out)[0].url.contains("/history-candles?"));
    assert!(
        terminals(&out, 5).is_empty(),
        "forced history discards the recent page"
    );
    let out = answer(&mut e, &candle_read(&out), &envelope(ROW_OLD), NOW + 2.0);
    let done = terminals(&out, 5);
    assert_eq!(done.len(), 1);
    assert!(done[0].as_ref().unwrap().contains("\"volume\":20.0"));
    assert!(!done[0].as_ref().unwrap().contains("\"volume\":30.0"));
}

#[test]
fn candle_entry_refusals_keep_text_kind_and_outcome_without_http() {
    let mut e = ready();
    // Unknown market is a native empty success, not an error.
    let out = fetch(
        &mut e,
        21,
        "NOPE",
        Interval::M1,
        Some(10),
        (None, None),
        NOW,
    );
    assert!(!sends_io(&out));
    assert_eq!(terminals(&out, 21), vec![Ok("[]".to_owned())]);
    assert!(diagnostics(&out, 21).is_empty());

    for (command, interval, from, to, message) in [
        (
            22,
            Interval::M1,
            Some(f64::NAN),
            None,
            "invalid candle time bounds".to_owned(),
        ),
        (
            23,
            Interval::M1,
            None,
            Some(f64::INFINITY),
            "invalid candle time bounds".to_owned(),
        ),
        (
            24,
            Interval::M1,
            Some(NOW - 60_000.0),
            Some(NOW - 120_000.0),
            "invalid candle time bounds".to_owned(),
        ),
        (
            25,
            Interval::H8,
            None,
            None,
            Interval::H8.unsupported("OKX"),
        ),
    ] {
        let out = fetch(&mut e, command, "BTCUSDT", interval, None, (from, to), NOW);
        assert!(!sends_io(&out), "{message}");
        assert!(diagnostics(&out, command).is_empty(), "{message}");
        let failed = terminals(&out, command);
        assert_eq!(failed.len(), 1, "{message}");
        let error = failed[0].as_ref().expect_err("a refusal is a failure");
        assert_eq!(error.message, message);
        assert_eq!(error.kind, None, "{message}");
        assert_eq!(error.outcome, None, "{message}");
        assert_eq!(error.venue_code, None, "{message}");
    }
}

#[test]
fn failed_candle_reads_keep_the_native_soft_empty_result() {
    for (status, body, message) in [
        (
            200,
            r#"{"code":"51001","msg":"Instrument ID does not exist","data":[]}"#,
            "OKX 51001: Instrument ID does not exist",
        ),
        (500, "busy", "OKX HTTP 500: busy"),
        (
            200,
            r#"{"code":"0","msg":"","data":[["bad"]]}"#,
            "malformed OKX list item",
        ),
    ] {
        let mut e = ready();
        let out = fetch_ten(&mut e, 30, NOW);
        let out = respond(&mut e, &candle_read(&out), status, body, NOW + 1.0);
        assert!(!sends_io(&out), "{body}");
        assert_soft_empty(&out, 30, message);

        // The history fallback leg fails the same way.
        let mut e = ready();
        let out = fetch_ten(&mut e, 31, NOW);
        let out = answer(&mut e, &candle_read(&out), &envelope(""), NOW + 1.0);
        let out = respond(&mut e, &candle_read(&out), status, body, NOW + 2.0);
        assert!(!sends_io(&out), "{body}");
        assert_soft_empty(&out, 31, message);
    }

    let mut e = ready();
    let out = fetch_ten(&mut e, 32, NOW);
    let Action::Http(r) = candle_read(&out) else {
        unreachable!()
    };
    e.handle(
        NOW + 1.0,
        Event::HttpFailed {
            request: r.id,
            error: "wire failed",
        },
    );
    let out = prepared(&mut e, NOW + 1.0);
    assert!(!sends_io(&out));
    assert_soft_empty(&out, 32, "wire failed");
}

#[test]
fn candle_reads_stay_unsigned_public_egress_on_every_alias() {
    let credentials = Options {
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        passphrase: Some("primary-passphrase".into()),
        read_credentials: Some(safe_cex_engine::ReadCredentials {
            key: "read-key".into(),
            secret: "read-secret".into(),
            passphrase: Some("read-passphrase".into()),
        }),
        ..Options::default()
    };
    for (options, host, demo) in [
        (Options::default(), "https://www.okx.com/", false),
        (credentials.clone(), "https://www.okx.com/", false),
        (
            Options {
                testnet: true,
                ..credentials.clone()
            },
            "https://openapi.okx.com/",
            true,
        ),
        (
            Options {
                eea: true,
                ..credentials.clone()
            },
            "https://eea.okx.com/",
            false,
        ),
        (
            Options {
                eea: true,
                testnet: true,
                ..Options::default()
            },
            "https://eea.okx.com/",
            true,
        ),
    ] {
        let mut e = boot(options);
        let out = fetch_ten(&mut e, 40, NOW);
        let reads = http(&out);
        assert_eq!(reads.len(), 1, "{host}");
        let read = reads[0];
        assert!(read.url.starts_with(host), "{}", read.url);
        assert!(read.body.is_none());
        assert!(
            read.headers
                .iter()
                .all(|(k, _)| !k.starts_with("OK-ACCESS")),
            "candles never carry a credential: {:?}",
            read.headers
        );
        assert_eq!(header(read, "x-simulated-trading"), demo.then_some("1"));
        assert!(header(read, "Content-Type").is_none());
    }
}

#[test]
fn late_reply_after_a_terminal_is_inert() {
    let mut e = ready();
    let out = fetch_ten(&mut e, 50, NOW);
    let first = candle_read(&out);
    let out = answer(&mut e, &first, &envelope(ROW_NEW), NOW + 1.0);
    assert_eq!(terminals(&out, 50).len(), 1);
    let before = e.store().markets().len();
    for late in [
        answer(&mut e, &first, &envelope(ROW_OLD), NOW + 2.0),
        respond(&mut e, &first, 500, "busy", NOW + 3.0),
    ] {
        assert!(terminals(&late, 50).is_empty());
        assert!(diagnostics(&late, 50).is_empty());
        assert!(!sends_io(&late));
    }
    assert_eq!(e.store().markets().len(), before);

    // A stale recent leg replayed after its fallback was issued cannot complete it.
    let out = fetch_ten(&mut e, 51, NOW);
    let first = candle_read(&out);
    let out = answer(&mut e, &first, &envelope(""), NOW + 1.0);
    let fallback = candle_read(&out);
    let stale = answer(&mut e, &first, &envelope(ROW_NEW), NOW + 2.0);
    assert!(terminals(&stale, 51).is_empty());
    assert!(!sends_io(&stale));
    let out = answer(&mut e, &fallback, &envelope(ROW_OLD), NOW + 3.0);
    assert_eq!(terminals(&out, 51).len(), 1);
}

#[test]
fn a_refused_history_fallback_keeps_the_soft_empty_result() {
    let mut e = ready();
    let reads: Vec<_> = (1_000..1_256)
        .map(|command| candle_read(&fetch_ten(&mut e, command, NOW)))
        .collect();
    // Every Context slot is held, so the fallback leg cannot be admitted.
    let out = answer(&mut e, &reads[0], &envelope(""), NOW + 1.0);
    assert!(!sends_io(&out), "no fallback is sent past capacity");
    assert_eq!(terminals(&out, 1_000).len(), 1);
    assert_eq!(terminals(&out, 1_000)[0], Ok("[]".to_owned()));
    assert_eq!(diagnostics(&out, 1_000).len(), 1);
}

#[test]
fn a_deadline_before_route_preparation_is_a_hard_not_sent_failure() {
    let mut e = ready();
    e.handle(
        NOW,
        Event::Command(Command::FetchCandles {
            command: 60,
            symbol: "BTCUSDT".into(),
            interval: Interval::M1,
            limit: Some(10),
            from_ms: None,
            to_ms: None,
        }),
    );
    e.prepare_dispatch(NOW);
    let raw: Vec<_> = e.drain_actions().collect();
    assert!(
        http(&raw).is_empty(),
        "no physical HTTP before route preparation"
    );
    let seen = fire_armed(&mut e, &raw, NOW + DEADLINE_MS);
    let terminal = terminals(&seen, 60);
    assert_eq!(terminal.len(), 1);
    let error = terminal[0].clone().unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::Timeout));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert!(
        diagnostics(&seen, 60).is_empty(),
        "pre-send failure is not soft"
    );
}

#[test]
fn a_deadline_mid_stream_is_soft_once_and_late_chunks_are_inert() {
    let mut e = ready();
    let out = fetch_ten(&mut e, 61, NOW);
    let Action::Http(r) = candle_read(&out) else {
        unreachable!()
    };
    e.handle(
        NOW + 1.0,
        Event::HttpStart {
            request: r.id,
            status: 200,
            headers: "",
        },
    );
    let body = format!(r#"{{"code":"0","msg":"","data":[{ROW_NEW},"#);
    e.handle(
        NOW + 2.0,
        Event::HttpChunk {
            request: r.id,
            data: body.as_bytes(),
        },
    );
    let mut all = out.clone();
    all.extend(prepared(&mut e, NOW + 2.0));
    let seen = fire_armed(&mut e, &all, NOW + DEADLINE_MS);
    assert_eq!(terminals(&seen, 61), vec![Ok("[]".to_owned())]);
    assert_eq!(diagnostics(&seen, 61).len(), 1);
    let tail = format!("{ROW_OLD}]}}");
    e.handle(
        NOW + DEADLINE_MS + 1.0,
        Event::HttpChunk {
            request: r.id,
            data: tail.as_bytes(),
        },
    );
    e.handle(NOW + DEADLINE_MS + 1.0, Event::HttpEnd { request: r.id });
    let late = prepared(&mut e, NOW + DEADLINE_MS + 1.0);
    assert!(terminals(&late, 61).is_empty(), "no second terminal");
}
