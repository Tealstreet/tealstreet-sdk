//! Venue-reported closed positions: raw positions-history rows, deduped and paged by historyId.
#![allow(dead_code)]
mod support;
use safe_cex_engine::{Action, Command, Event, Notice, VenueAction};
use support::*;

fn fetch(h: &mut Harness, symbol: Option<&str>, from: Option<f64>, to: Option<f64>) {
    h.e.handle(
        1.0,
        Event::Command(Command::Venue {
            command: 9,
            action: VenueAction::FetchClosedPositions {
                symbol: symbol.map(Into::into),
                from_ms: from,
                to_ms: to,
            },
        }),
    );
    // The managed page releases through Prepare/Bind/Claim.
    h.collect_at(1.0);
}

fn result(h: &Harness) -> Option<Result<String, String>> {
    h.actions.iter().rev().find_map(|a| match a {
        Action::Notice(Notice::CommandDone { command: 9, result }) => Some(Ok(result.clone())),
        Action::Notice(Notice::CommandFailed { command: 9, error }) => {
            Some(Err(error.legacy_text()))
        }
        _ => None,
    })
}

fn row(history: &str, position: &str) -> String {
    format!(
        r#"{{"positionId":"{position}","historyId":"{history}","instId":"BTC-USDT","marginMode":"cross","positionSide":"net","direction":"long","openAveragePrice":"60000","closeAveragePrice":"61000","closePositions":"1","realizedPnl":"10","fee":"-0.1","fundingFee":"0","createTime":"1700000000000","updateTime":"1700000001000"}}"#
    )
}

fn page(rows: &[String]) -> String {
    format!(r#"{{"code":"0","msg":"","data":[{}]}}"#, rows.join(","))
}

#[test]
fn full_pages_continue_after_the_smallest_history_id_and_rows_pass_through() {
    let mut h = Harness::new();
    h.seed();
    fetch(&mut h, Some("BTCUSDT"), Some(1_000.0), Some(2_000.0));
    let first = h.take("positions-history");
    assert!(
        first.url.ends_with(
            "/api/v1/account/positions-history?instId=BTC-USDT&begin=1000&end=2000&limit=100"
        ),
        "{}",
        first.url
    );
    // Ids of different lengths: "99" is older than "100" (TS `oldestCursorId`).
    let rows: Vec<String> = (0..100)
        .map(|i| row(&(99 + i).to_string(), "same-position"))
        .collect();
    h.respond(first, &page(&rows));
    let second = h.take("positions-history");
    assert!(
        second.url.ends_with("&limit=100&after=99"),
        "{}",
        second.url
    );
    let duplicate = row("150", "same-position");
    let fresh = row("7", "same-position");
    h.respond(second, &page(&[duplicate, fresh.clone()]));
    let got: Vec<serde_json::Value> = serde_json::from_str(&result(&h).unwrap().unwrap()).unwrap();
    assert_eq!(
        got.len(),
        101,
        "the repeated historyId is dropped, positionId is not a key"
    );
    assert_eq!(
        got[100],
        serde_json::from_str::<serde_json::Value>(&fresh).unwrap()
    );
}

#[test]
fn a_page_of_only_seen_rows_stops_and_rejections_fail() {
    let mut h = Harness::new();
    h.seed();
    fetch(&mut h, None, None, None);
    let rows: Vec<String> = (0..100)
        .map(|i| row(&(1000 + i).to_string(), "p"))
        .collect();
    h.reply("positions-history", &page(&rows));
    h.reply("positions-history", &page(&rows));
    assert!(
        h.pending
            .iter()
            .all(|r| !r.url.contains("positions-history"))
    );
    let got: Vec<serde_json::Value> = serde_json::from_str(&result(&h).unwrap().unwrap()).unwrap();
    assert_eq!(got.len(), 100);

    fetch(&mut h, None, None, None);
    h.reply(
        "positions-history",
        r#"{"code":"152002","msg":"Parameter error","data":[]}"#,
    );
    assert_eq!(
        result(&h),
        Some(Err(
            "BloFin closed positions: BloFin 152002: Parameter error".into()
        ))
    );
}

#[test]
fn unknown_symbols_read_nothing_and_the_page_cap_warns() {
    let mut h = Harness::new();
    h.seed();
    fetch(&mut h, Some("NOPEUSDT"), None, None);
    assert_eq!(result(&h), Some(Ok("[]".into())));

    h.actions.clear();
    h.e.set_log_level(safe_cex_engine::Level::Error);
    fetch(&mut h, None, None, None);
    for p in 0..100u64 {
        let rows: Vec<String> = (0..100u64)
            .map(|i| row(&(1_000_000 - p * 100 - i).to_string(), "p"))
            .collect();
        h.reply("positions-history", &page(&rows));
    }
    let got: Vec<serde_json::Value> = serde_json::from_str(&result(&h).unwrap().unwrap()).unwrap();
    assert_eq!(got.len(), 10_000);
    assert!(h.actions.iter().any(|a| matches!(a,
        Action::Notice(Notice::Event(event)) if event.level == safe_cex_engine::Level::Warn && event.message.contains("100-page cap"))));
    let (marker_index, marker) = h
        .actions
        .iter()
        .enumerate()
        .find_map(|(i, a)| match a {
            Action::Notice(Notice::Event(event))
                if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE =>
            {
                Some((i, event))
            }
            _ => None,
        })
        .expect("semantic coverage survives Error-only logging");
    assert_eq!(
        marker.fields,
        vec![("command", safe_cex_engine::Value::Num(9.0))]
    );
    let done_index = h
        .actions
        .iter()
        .position(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 9, .. })))
        .unwrap();
    assert!(marker_index < done_index, "coverage precedes retained rows");
}

#[test]
fn second_page_failure_returns_only_accepted_rows_with_owned_incomplete() {
    let mut h = Harness::new();
    h.seed();
    h.actions.clear();
    h.e.set_log_level(safe_cex_engine::Level::Error);
    fetch(&mut h, None, None, None);
    let rows: Vec<_> = (1000..1100).map(|id| row(&id.to_string(), "p")).collect();
    h.reply("positions-history", &page(&rows));
    // The body is streamed before the envelope verdict. This row must not be
    // admitted merely because earlier pages succeeded.
    h.reply(
        "positions-history",
        &format!(
            r#"{{"code":"152002","msg":"Parameter error","data":[{}]}}"#,
            row("999", "untrusted")
        ),
    );
    assert!(
        matches!(result(&h), Some(Ok(_))),
        "later page failure must retain accepted rows"
    );
    let got: Vec<serde_json::Value> = serde_json::from_str(&result(&h).unwrap().unwrap()).unwrap();
    assert_eq!(
        got.len(),
        100,
        "a failed page cannot leak into accepted rows"
    );
    assert!(got.iter().all(|row| row["positionId"] == "p"));
    let notices: Vec<_> = h
        .actions
        .iter()
        .filter(|action| match action {
            Action::Notice(Notice::Event(event))
                if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE =>
            {
                true
            }
            Action::Notice(
                Notice::CommandDone { command: 9, .. } | Notice::CommandFailed { command: 9, .. },
            ) => true,
            _ => false,
        })
        .collect();
    assert_eq!(notices.len(), 2);
    assert!(matches!(notices[0], Action::Notice(Notice::Event(event))
        if event.fields == vec![("command", safe_cex_engine::Value::Num(9.0))]
        && event.message.contains("152002")));
    assert!(matches!(
        notices[1],
        Action::Notice(Notice::CommandDone { command: 9, .. })
    ));
}

#[test]
fn first_page_failure_cannot_resolve_unvalidated_rows_as_complete_history() {
    let mut h = Harness::new();
    h.seed();
    h.actions.clear();
    fetch(&mut h, None, None, None);
    h.reply(
        "positions-history",
        &format!(
            r#"{{"code":"152002","msg":"Parameter error","data":[{}]}}"#,
            row("999", "untrusted")
        ),
    );
    assert!(
        matches!(result(&h), Some(Err(_))),
        "first failed page must reject"
    );
    assert!(
        h.failures.last().unwrap().kind.is_some(),
        "first-page rejection is typed"
    );
    assert!(!h.actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { command: 9, .. })
            | Action::Notice(Notice::Event(safe_cex_engine::LogEvent {
                code: safe_cex_engine::log::code::HISTORY_INCOMPLETE,
                ..
            }))
    )));
}

#[test]
fn missing_ids_pass_through_and_unchanged_cursor_is_an_endpoint_end() {
    let mut h = Harness::new();
    h.seed();
    h.actions.clear();
    fetch(&mut h, None, None, None);
    let mut rows: Vec<_> = (1000..1100).map(|id| row(&id.to_string(), "p")).collect();
    h.reply("positions-history", &page(&rows));
    // A full page with fresh rows but the same smallest continuation is a
    // documented legacy end, not a safety-budget failure. Unknown IDs remain.
    rows = (0..99)
        .map(|_| r#"{"positionId":"no-history-id"}"#.to_owned())
        .collect();
    rows.push(row("1000", "p"));
    h.reply("positions-history", &page(&rows));
    let got: Vec<serde_json::Value> = serde_json::from_str(&result(&h).unwrap().unwrap()).unwrap();
    assert_eq!(got.len(), 199);
    assert!(!h.actions.iter().any(|a| matches!(a,
        Action::Notice(Notice::Event(event))
        if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE
    )));
    assert!(
        !h.pending
            .iter()
            .any(|r| r.url.contains("positions-history"))
    );
}

#[test]
fn a_cursor_cycle_keeps_rows_with_owned_incomplete_instead_of_requesting_again() {
    let mut h = Harness::new();
    h.seed();
    h.actions.clear();
    h.e.set_log_level(safe_cex_engine::Level::Error);
    fetch(&mut h, None, None, None);
    for first in [1000, 500] {
        let rows: Vec<_> = (first..first + 100)
            .map(|id| row(&id.to_string(), "p"))
            .collect();
        h.reply("positions-history", &page(&rows));
    }
    let mut rows: Vec<_> = (1100..1199).map(|id| row(&id.to_string(), "p")).collect();
    rows.push(row("1000", "p"));
    h.reply("positions-history", &page(&rows));
    assert!(
        !h.pending
            .iter()
            .any(|r| r.url.contains("positions-history")),
        "revisiting a prior continuation cannot prove more coverage"
    );
    let got: Vec<serde_json::Value> = serde_json::from_str(&result(&h).unwrap().unwrap()).unwrap();
    assert_eq!(got.len(), 299);
    let relevant: Vec<_> = h
        .actions
        .iter()
        .filter(|a| {
            matches!(a,
                Action::Notice(Notice::Event(event))
                    if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE
            ) || matches!(a, Action::Notice(Notice::CommandDone { command: 9, .. }))
        })
        .collect();
    assert_eq!(relevant.len(), 2);
    assert!(matches!(relevant[0], Action::Notice(Notice::Event(event))
        if event.fields == vec![("command", safe_cex_engine::Value::Num(9.0))]
        && event.message.contains("repeated cursor")));
    assert!(matches!(
        relevant[1],
        Action::Notice(Notice::CommandDone { command: 9, .. })
    ));
}
