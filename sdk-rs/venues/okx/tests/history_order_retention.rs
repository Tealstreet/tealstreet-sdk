//! OKX documented ordinary-order history: recent cancellations plus archived fills.
//! https://www.okx.com/docs-v5/en/#order-book-trading-trade-get-order-history-last-7-days
//! https://www.okx.com/docs-v5/en/#order-book-trading-trade-get-order-history-last-3-months
#[allow(dead_code)]
mod support;
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, Query, Response, Value,
};
use safe_cex_venue_okx::Okx;

const NOW: f64 = 1_710_000_000_000.;
const HOUR: f64 = 3_600_000.;
const DAY: f64 = 24. * HOUR;
const OWNER: u32 = 984;

fn start(from: Option<f64>) -> (Engine<Okx>, HttpRequest) {
    let mut engine = support::trading_engine();
    engine.drain_actions().for_each(drop);
    engine.take_frame(NOW);
    engine
        .handle_command(
            NOW,
            Command::Query {
                command: OWNER,
                query: Query::OrderHistory {
                    symbol: Some("BTCUSDT".into()),
                    from_ms: from,
                    to_ms: Some(NOW),
                    limit: None,
                },
            },
            None,
        )
        .unwrap();
    let request = physical(support::prepared(&mut engine, NOW));
    (engine, request)
}
fn physical(output: Vec<Action>) -> HttpRequest {
    output
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .expect("history request")
}
fn reply(engine: &mut Engine<Okx>, request: &HttpRequest, now: f64, body: &str) -> Vec<Action> {
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
    support::prepared(engine, now)
}
fn next(engine: &mut Engine<Okx>, output: Vec<Action>, now: f64) -> HttpRequest {
    assert!(
        !output.iter().any(|a| matches!(a, Action::Http(_))),
        "source switch retains native pacing"
    );
    let (timer, at) = output
        .into_iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if at_ms > now && at_ms <= now + 100. => {
                Some((timer, at_ms))
            }
            _ => None,
        })
        .expect("100ms history continuation");
    engine.handle(at, Event::TimerFired { timer });
    physical(support::prepared(engine, at))
}
fn row(id: &str, state: &str, at: f64, filled: &str) -> String {
    format!(
        r#"{{"instId":"BTC-USDT-SWAP","ordId":"{id}","side":"buy","ordType":"limit","state":"{state}","sz":"2","accFillSz":"{filled}","px":"40000","avgPx":"40000","posSide":"net","cTime":"{at}","uTime":"{at}"}}"#
    )
}
fn rows(output: &[Action]) -> serde_json::Value {
    let result = output
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone {
                command: OWNER,
                result,
            }) => Some(result),
            _ => None,
        })
        .expect("completed history");
    serde_json::from_str(result).unwrap()
}
fn reasons(output: &[Action]) -> Vec<String> {
    output
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::Event(event))
                if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE =>
            {
                assert!(
                    event
                        .fields
                        .iter()
                        .any(|(k, v)| *k == "command" && *v == Value::Num(OWNER as f64))
                );
                event.fields.iter().find_map(|(k, v)| match (*k, v) {
                    ("reason", Value::Str(s)) => Some(s.clone()),
                    _ => None,
                })
            }
            _ => None,
        })
        .collect()
}

#[test]
fn old_range_keeps_recent_cancellations_and_merges_archived_fills_by_id() {
    let (mut engine, recent) = start(Some(NOW - 10. * DAY));
    assert!(recent.url.contains("/trade/orders-history?"));
    let canceled = row("300", "canceled", NOW - HOUR, "0");
    let filled = row("200", "filled", NOW - 3. * DAY, "2");
    let output = reply(
        &mut engine,
        &recent,
        NOW + 10.,
        &support::envelope(&format!("{canceled},{filled}")),
    );
    let archive = next(&mut engine, output, NOW + 10.);
    assert!(archive.url.contains("/trade/orders-history-archive?"));
    assert!(archive.url.contains("begin=1709136000000"));
    let older = row("100", "filled", NOW - 8. * DAY, "2");
    // Archived duplicates must neither lose the recent cancellation nor overwrite it.
    let stale = row("300", "filled", NOW - HOUR, "2");
    let output = reply(
        &mut engine,
        &archive,
        NOW + 120.,
        &support::envelope(&format!("{older},{filled},{stale}")),
    );
    let data = rows(&output);
    assert_eq!(data.as_array().unwrap().len(), 3);
    assert_eq!(data[0]["id"], "300");
    assert_eq!(data[0]["status"], "canceled");
    assert_eq!(data[2]["id"], "100");
    assert_eq!(reasons(&output), ["retentionLimit"]);
}

#[test]
fn expired_unfilled_cancellations_cannot_make_an_empty_range_complete() {
    let (mut engine, recent) = start(Some(NOW - 3. * HOUR));
    let output = reply(&mut engine, &recent, NOW + 10., &support::envelope(""));
    assert_eq!(rows(&output), serde_json::json!([]));
    assert_eq!(reasons(&output), ["retentionLimit"]);
    assert!(!output.iter().any(|a| matches!(a, Action::Http(_))));
}

#[test]
fn recent_ranges_inside_retention_keep_the_original_single_read() {
    for from in [NOW - HOUR, NOW - 2. * HOUR + 100.] {
        let (mut engine, recent) = start(Some(from));
        let output = reply(&mut engine, &recent, NOW + 10., &support::envelope(""));
        assert_eq!(rows(&output), serde_json::json!([]));
        assert!(reasons(&output).is_empty());
        assert!(!output.iter().any(|a| matches!(a, Action::Http(_))));
    }
}

#[test]
fn unbounded_history_discloses_retention_without_starting_an_unbounded_archive_walk() {
    let (mut engine, recent) = start(None);
    let output = reply(&mut engine, &recent, NOW + 10., &support::envelope(""));
    assert_eq!(rows(&output), serde_json::json!([]));
    assert_eq!(reasons(&output), ["retentionLimit"]);
}

#[test]
fn failed_archive_keeps_recent_rows_incomplete() {
    let (mut engine, recent) = start(Some(NOW - 10. * DAY));
    let output = reply(
        &mut engine,
        &recent,
        NOW + 10.,
        &support::envelope(&row("300", "canceled", NOW - HOUR, "0")),
    );
    let archive = next(&mut engine, output, NOW + 10.);
    let output = reply(
        &mut engine,
        &archive,
        NOW + 120.,
        r#"{"code":"1","msg":"fixture archive unavailable","data":[]}"#,
    );
    assert_eq!(rows(&output)[0]["id"], "300");
    assert_eq!(reasons(&output), ["laterPageFailed"]);
}

#[test]
fn archive_keeps_the_original_caller_deadline() {
    let mut engine = support::trading_engine();
    engine.drain_actions().for_each(drop);
    engine.take_frame(NOW);
    engine
        .handle_command(
            NOW,
            Command::Query {
                command: OWNER,
                query: Query::OrderHistory {
                    symbol: Some("BTCUSDT".into()),
                    from_ms: Some(NOW - 10. * DAY),
                    to_ms: Some(NOW),
                    limit: None,
                },
            },
            Some(150),
        )
        .unwrap();
    let output = support::prepared(&mut engine, NOW);
    let timer = output
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 150. => Some(*timer),
            _ => None,
        })
        .expect("original caller deadline");
    let recent = physical(output);
    let output = reply(
        &mut engine,
        &recent,
        NOW + 10.,
        &support::envelope(&row("300", "canceled", NOW - HOUR, "0")),
    );
    let archive = next(&mut engine, output, NOW + 10.);
    engine.handle(NOW + 150., Event::TimerFired { timer });
    let output = support::prepared(&mut engine, NOW + 150.);
    assert!(output.iter().any(|a| matches!(a,
        Action::Notice(Notice::CommandFailed { command: OWNER, error })
            if error.kind == Some(safe_cex_engine::ErrorKind::Timeout))));
    assert!(!output.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { .. }) | Action::Http(_)
    )));
    let late = reply(&mut engine, &archive, NOW + 160., &support::envelope(""));
    assert!(!late.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { .. }) | Action::Http(_)
    )));
}

#[test]
fn composite_account_walks_both_instrument_types_in_each_source() {
    let mut engine = support::engine(support::FixtureOptions {
        key: Some("test-key".into()),
        secret: Some("test-secret".into()),
        passphrase: Some("test-passphrase".into()),
        eea: true,
        ..Default::default()
    });
    engine.start(NOW - 1000.);
    let mut pending = support::prepared(&mut engine, NOW - 1000.);
    while let Some(action) = pending.pop() {
        let Action::Http(request) = action else {
            continue;
        };
        let body = if request.url.contains("/account/config") {
            support::envelope(r#"{"posMode":"net_mode","acctLv":"2"}"#)
        } else if request.url.contains("instruments?instType=SWAP") {
            support::envelope(support::MARKET)
        } else if request.url.contains("tickers?instType=SWAP") {
            support::envelope(support::TICKER)
        } else if request.url.contains("account-position-risk") {
            support::envelope(r#"{"balData":[{"ccy":"USDT","eq":"5000"}]}"#)
        } else {
            support::envelope("")
        };
        pending.extend(reply(&mut engine, &request, NOW - 900., &body));
    }
    assert!(engine.store().loaded().markets);
    engine.take_frame(NOW);
    engine
        .handle_command(
            NOW,
            Command::Query {
                command: OWNER,
                query: Query::OrderHistory {
                    symbol: None,
                    from_ms: Some(NOW - 10. * DAY),
                    to_ms: Some(NOW),
                    limit: None,
                },
            },
            None,
        )
        .unwrap();
    let first = physical(support::prepared(&mut engine, NOW));
    assert!(first.url.contains("orders-history?instType=SWAP"));
    let output = reply(&mut engine, &first, NOW + 10., &support::envelope(""));
    let second = physical(output);
    assert!(second.url.contains("orders-history?instType=FUTURES"));
    let output = reply(&mut engine, &second, NOW + 20., &support::envelope(""));
    let third = next(&mut engine, output, NOW + 20.);
    assert!(third.url.contains("orders-history-archive?instType=SWAP"));
    let output = reply(&mut engine, &third, NOW + 130., &support::envelope(""));
    let fourth = physical(output);
    assert!(
        fourth
            .url
            .contains("orders-history-archive?instType=FUTURES")
    );
    let output = reply(&mut engine, &fourth, NOW + 140., &support::envelope(""));
    assert_eq!(rows(&output), serde_json::json!([]));
    assert_eq!(reasons(&output), ["retentionLimit"]);
}

#[test]
fn retention_boundary_crossed_in_flight_cannot_report_complete() {
    for from in [NOW - 2. * HOUR, NOW - 2. * HOUR + 5.] {
        let (mut engine, recent) = start(Some(from));
        let output = reply(&mut engine, &recent, NOW + 10., &support::envelope(""));
        assert_eq!(rows(&output), serde_json::json!([]));
        assert_eq!(reasons(&output), ["retentionLimit"]);
    }
}
