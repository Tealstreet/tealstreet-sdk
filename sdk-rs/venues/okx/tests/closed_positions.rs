//! Venue-reported closed positions: raw positions-history rows, paged backwards by close time.
#![allow(dead_code)]
mod support;

use crate::support::FixtureOptions as Options;
use crate::support::*;
use safe_cex_engine::{Action, Command, Engine, Event, Notice, VenueAction};
use safe_cex_venue_okx::Okx;

const NOW: f64 = 1_710_000_000_000.0;

fn fetch(e: &mut Engine<Okx>, symbol: Option<&str>, from: Option<f64>, to: Option<f64>) {
    e.handle(
        NOW,
        Event::Command(Command::Venue {
            command: 77,
            action: VenueAction::FetchClosedPositions {
                symbol: symbol.map(Into::into),
                from_ms: from,
                to_ms: to,
            },
        }),
    );
}

fn request(e: &mut Engine<Okx>) -> Action {
    request_at(e, NOW)
}

fn request_at(e: &mut Engine<Okx>, now: f64) -> Action {
    prepared(e, now)
        .into_iter()
        .find(|a| matches!(a, Action::Http(_)))
        .expect("a positions-history request")
}

fn url(action: &Action) -> String {
    let Action::Http(request) = action else {
        panic!("expected HTTP")
    };
    request.url.clone()
}

fn done(actions: &[Action]) -> Option<Result<String, String>> {
    actions.iter().find_map(|a| match a {
        Action::Notice(Notice::CommandDone {
            command: 77,
            result,
        }) => Some(Ok(result.clone())),
        Action::Notice(Notice::CommandFailed { command: 77, error }) => {
            Some(Err(error.legacy_text()))
        }
        _ => None,
    })
}

fn row(i: usize, time: u64) -> String {
    format!(
        r#"{{"instId":"BTC-USDT-SWAP","instType":"SWAP","posId":"1","direction":"long","closeTotalPos":"{i}","openAvgPx":"40000","closeAvgPx":"41000","realizedPnl":"1.5","fee":"-0.1","fundingFee":"0","cTime":"1709000000000","uTime":"{time}","type":"2","lever":"10"}}"#
    )
}

fn page(rows: &[String]) -> String {
    format!(r#"{{"code":"0","msg":"","data":[{}]}}"#, rows.join(","))
}

#[test]
fn rows_page_backwards_by_the_oldest_close_time_and_pass_through_verbatim() {
    let mut e = trading_engine();
    fetch(
        &mut e,
        Some("BTCUSDT"),
        Some(1_700_000_000_000.0),
        Some(1_710_000_000_000.0),
    );
    let first = request(&mut e);
    assert!(
        url(&first).ends_with("/api/v5/account/positions-history?instType=SWAP&instId=BTC-USDT-SWAP&before=1700000000000&after=1710000000000&limit=100"),
        "{}",
        url(&first)
    );
    let rows: Vec<String> = (0..100)
        .map(|i| row(i, 1_709_500_000_000 + i as u64))
        .collect();
    reply(&mut e, &first, &page(&rows), NOW);
    let timer = actions(&mut e)
        .into_iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .expect("TS waits 100ms between pages");
    e.handle(NOW + 100.0, Event::TimerFired { timer });
    let second = request_at(&mut e, NOW + 100.0);
    assert!(
        url(&second).contains("&after=1709500000001&limit=100"),
        "{}",
        url(&second)
    );
    let last = [row(100, 1_709_400_000_000)];
    reply(&mut e, &second, &page(&last), NOW + 100.0);
    let result = done(&actions(&mut e)).unwrap().unwrap();
    let got: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
    let expected: Vec<serde_json::Value> = rows
        .iter()
        .chain(&last)
        .map(|row| serde_json::from_str(row).unwrap())
        .collect();
    assert_eq!(got, expected);
}

#[test]
fn account_wide_reads_omit_the_instrument_and_unknown_symbols_are_empty() {
    let mut e = trading_engine();
    fetch(&mut e, None, None, None);
    let first = request(&mut e);
    assert!(
        url(&first).ends_with("/api/v5/account/positions-history?instType=SWAP&limit=100"),
        "{}",
        url(&first)
    );
    reply(&mut e, &first, &page(&[]), NOW);
    assert_eq!(done(&actions(&mut e)), Some(Ok("[]".into())));

    fetch(&mut e, Some("NOPEUSDT"), None, None);
    assert_eq!(done(&actions(&mut e)), Some(Ok("[]".into())));
}

#[test]
fn a_page_without_usable_close_times_stops_and_a_rejection_fails() {
    let mut e = trading_engine();
    fetch(&mut e, None, None, None);
    let first = request(&mut e);
    let rows: Vec<String> = (0..100).map(|i| row(i, 0)).collect();
    reply(&mut e, &first, &page(&rows), NOW);
    let result = done(&actions(&mut e)).unwrap().unwrap();
    assert_eq!(
        serde_json::from_str::<Vec<serde_json::Value>>(&result)
            .unwrap()
            .len(),
        100
    );

    fetch(&mut e, None, None, None);
    let first = request(&mut e);
    reply(
        &mut e,
        &first,
        r#"{"code":"50011","msg":"Too Many Requests","data":[]}"#,
        NOW,
    );
    let result = done(&actions(&mut e)).unwrap();
    assert!(result.unwrap_err().starts_with("OKX closed positions:"));
}

#[test]
fn the_page_cap_warns_and_keeps_rows() {
    let mut e = trading_engine();
    e.set_log_level(safe_cex_engine::Level::Error);
    fetch(&mut e, None, None, None);
    let mut time = 1_709_900_000_000u64;
    let mut out = Vec::new();
    for index in 0..100 {
        let at = NOW + f64::from(index) * 100.0;
        let a = request_at(&mut e, at);
        let rows: Vec<String> = (0..100).map(|i| row(i, time - i as u64)).collect();
        time -= 1_000;
        reply(&mut e, &a, &page(&rows), at);
        out = actions(&mut e);
        if let Some(timer) = out.iter().find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(*timer),
            _ => None,
        }) {
            e.handle(at + 100.0, Event::TimerFired { timer });
        }
    }
    let rows = done(&out).unwrap().unwrap();
    assert_eq!(
        serde_json::from_str::<Vec<serde_json::Value>>(&rows)
            .unwrap()
            .len(),
        10_000
    );
    assert!(out.iter().any(|a| matches!(a,
        Action::Notice(Notice::Event(event)) if event.level == safe_cex_engine::Level::Warn && event.message.contains("100-page cap"))));
    let (index, event) = out
        .iter()
        .enumerate()
        .find_map(|(i, action)| match action {
            Action::Notice(Notice::Event(event))
                if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE =>
            {
                Some((i, event))
            }
            _ => None,
        })
        .expect("owned semantic closed-position warning");
    assert_eq!(
        event.fields,
        vec![("command", safe_cex_engine::Value::Num(77.0))]
    );
    let done = out
        .iter()
        .position(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 77, .. })))
        .unwrap();
    assert!(index < done);
}

#[test]
fn keyless_sessions_read_nothing() {
    let (mut e, _, _) = public(Options::default());
    fetch(&mut e, None, None, None);
    let out = actions(&mut e);
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
    assert_eq!(done(&out), Some(Ok("[]".into())));
}

#[test]
fn first_page_failures_are_typed_and_never_return_empty_complete_rows() {
    for body in [
        r#"{"code":"50011","msg":"rate limited","data":[]}"#,
        r#"{"code":"0","msg":"OK"}"#,
    ] {
        let mut e = trading_engine();
        fetch(&mut e, None, None, None);
        let first = request(&mut e);
        reply(&mut e, &first, body, NOW);
        let out = actions(&mut e);
        assert!(
            matches!(done(&out), Some(Err(_))),
            "first failed page must reject"
        );
        assert!(
            out.iter().any(|a| matches!(a,
                Action::Notice(Notice::CommandFailed { command: 77, error }) if error.kind.is_some()
            )),
            "first-page rejection must be typed"
        );
        assert!(!out.iter().any(|a| matches!(a,
            Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE
        )));
    }
}

#[test]
fn failed_later_page_returns_only_validated_rows_with_owned_completeness() {
    let mut e = trading_engine();
    e.set_log_level(safe_cex_engine::Level::Error);
    fetch(&mut e, None, None, None);
    let first = request(&mut e);
    let rows: Vec<_> = (0..100)
        .map(|i| row(i, 1_709_500_000_000 + i as u64))
        .collect();
    reply(&mut e, &first, &page(&rows), NOW);
    let timer = actions(&mut e)
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(NOW + 100.0, Event::TimerFired { timer });
    let second = request_at(&mut e, NOW + 100.0);
    let failed = format!(
        r#"{{"code":"50011","msg":"rate limited","data":[{}]}}"#,
        row(999, 1_709_400_000_000)
    );
    reply(&mut e, &second, &failed, NOW + 100.0);
    let out = actions(&mut e);
    assert!(
        matches!(done(&out), Some(Ok(_))),
        "later failure must retain accepted rows"
    );
    let rows: Vec<serde_json::Value> = serde_json::from_str(&done(&out).unwrap().unwrap()).unwrap();
    assert_eq!(rows.len(), 100);
    assert!(rows.iter().all(|row| row["closeTotalPos"] != "999"));
    let notices: Vec<_> = out.iter().filter(|a| matches!(a,
        Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE
    ) || matches!(a, Action::Notice(Notice::CommandDone { command: 77, .. }
        | Notice::CommandFailed { command: 77, .. }))).collect();
    assert_eq!(notices.len(), 2);
    assert!(matches!(notices[0], Action::Notice(Notice::Event(event))
        if event.fields == vec![("command", safe_cex_engine::Value::Num(77.0))]));
    assert!(matches!(
        notices[1],
        Action::Notice(Notice::CommandDone { command: 77, .. })
    ));
}

#[test]
fn rows_not_older_than_the_cursor_stop_at_once_as_incomplete() {
    let mut e = trading_engine();
    let to = 1_710_000_000_000.0;
    fetch(&mut e, None, None, Some(to));
    let first = request(&mut e);
    assert!(url(&first).contains("after=1710000000000"));
    let rows: Vec<_> = (0..100).map(|i| row(i, 1_710_000_000_001)).collect();
    reply(&mut e, &first, &page(&rows), NOW);
    let out = actions(&mut e);
    assert!(matches!(done(&out), Some(Ok(_))));
    assert!(!out.iter().any(|a| matches!(a, Action::SetTimer { .. })));
    assert!(
        out.iter().any(|a| matches!(a,
            Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE
        )),
        "a page the cursor did not bound proves nothing older was walked"
    );
}

#[test]
fn a_short_page_not_older_than_the_cursor_is_incomplete_too() {
    let mut e = trading_engine();
    fetch(&mut e, None, None, Some(1_710_000_000_000.0));
    let first = request(&mut e);
    let rows: Vec<_> = (0..3).map(|i| row(i, 1_710_000_000_000)).collect();
    reply(&mut e, &first, &page(&rows), NOW);
    let out = actions(&mut e);
    assert!(matches!(done(&out), Some(Ok(_))));
    assert!(!out.iter().any(|a| matches!(a, Action::SetTimer { .. })));
    assert!(
        out.iter().any(|a| matches!(a,
            Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE
        )),
        "rows outside the exclusive bound are never a complete result"
    );
}

fn next_request(e: &mut Engine<Okx>, at: f64) -> Action {
    let timer = actions(e)
        .into_iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .expect("a continuing page");
    e.handle(at + 100.0, Event::TimerFired { timer });
    request_at(e, at + 100.0)
}

fn result_rows(out: &[Action]) -> Vec<serde_json::Value> {
    serde_json::from_str(&done(out).expect("a terminal").expect("rows")).unwrap()
}

#[test]
fn rows_sharing_the_oldest_millisecond_across_a_page_break_are_all_kept_once() {
    let mut e = trading_engine();
    fetch(&mut e, None, None, None);
    let oldest = 1_709_500_000_000u64;
    let first_rows: Vec<String> = (0..98)
        .map(|i| row(i, oldest + 1_000 - i as u64))
        .chain([row(98, oldest), row(99, oldest)])
        .collect();
    let first = request(&mut e);
    reply(&mut e, &first, &page(&first_rows), NOW);
    let second = next_request(&mut e, NOW);
    assert!(
        url(&second).contains(&format!("&after={}&", oldest + 1)),
        "the oldest millisecond is fetched again: {}",
        url(&second)
    );
    // The venue re-sends the boundary rows, then the one that did not fit page 1.
    let second_rows = [
        row(98, oldest),
        row(99, oldest),
        row(100, oldest),
        row(101, oldest - 5),
    ];
    reply(&mut e, &second, &page(&second_rows), NOW + 100.0);
    let out = actions(&mut e);
    let got = result_rows(&out);
    let expected: Vec<serde_json::Value> = first_rows
        .iter()
        .chain(&second_rows[2..].to_vec())
        .map(|row| serde_json::from_str(row).unwrap())
        .collect();
    assert_eq!(
        got, expected,
        "every row once, none dropped at the shared millisecond"
    );
    assert!(!out.iter().any(|a| matches!(a,
        Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE
    )));
}

#[test]
fn byte_identical_rows_at_the_boundary_keep_their_multiplicity() {
    let mut e = trading_engine();
    fetch(&mut e, None, None, None);
    let oldest = 1_709_500_000_000u64;
    let twin = row(7, oldest);
    let first_rows: Vec<String> = (0..99)
        .map(|i| row(i + 10, oldest + 1_000 - i as u64))
        .chain([twin.clone()])
        .collect();
    let first = request(&mut e);
    reply(&mut e, &first, &page(&first_rows), NOW);
    let second = next_request(&mut e, NOW);
    // Two identical rows at the boundary: one was already accepted, one is new.
    let second_rows = [twin.clone(), twin.clone()];
    reply(&mut e, &second, &page(&second_rows), NOW + 100.0);
    let got = result_rows(&actions(&mut e));
    assert_eq!(got.len(), 101);
    let twins = got
        .iter()
        .filter(|row| **row == serde_json::from_str::<serde_json::Value>(&twin).unwrap())
        .count();
    assert_eq!(twins, 2);
}

#[test]
fn a_full_page_inside_one_millisecond_stops_as_incomplete_without_duplicates() {
    let mut e = trading_engine();
    fetch(&mut e, None, None, None);
    let at = 1_709_500_000_000u64;
    let rows: Vec<String> = (0..100).map(|i| row(i, at)).collect();
    let first = request(&mut e);
    reply(&mut e, &first, &page(&rows), NOW);
    let second = next_request(&mut e, NOW);
    assert!(url(&second).contains(&format!("&after={}&", at + 1)));
    reply(&mut e, &second, &page(&rows), NOW + 100.0);
    let out = actions(&mut e);
    assert!(!out.iter().any(|a| matches!(a, Action::SetTimer { .. })));
    assert_eq!(
        result_rows(&out).len(),
        100,
        "re-sent rows are not duplicated"
    );
    assert!(
        out.iter().any(|a| matches!(a,
            Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE
                && event.fields == vec![("command", safe_cex_engine::Value::Num(77.0))]
        )),
        "rows of that millisecond past one page cannot be walked"
    );
}

#[test]
fn a_nonadjacent_close_time_cursor_cycle_stops_in_the_shared_pager() {
    let mut e = trading_engine();
    fetch(&mut e, None, None, None);
    let mut terminal = Vec::new();
    for (index, time) in [1000, 500, 1000].into_iter().enumerate() {
        let at = NOW + index as f64 * 100.0;
        let current = request_at(&mut e, at);
        let rows: Vec<_> = (0..100).map(|i| row(i, time)).collect();
        reply(&mut e, &current, &page(&rows), at);
        terminal = actions(&mut e);
        if let Some(timer) = terminal.iter().find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(*timer),
            _ => None,
        }) {
            e.handle(at + 100.0, Event::TimerFired { timer });
        }
    }
    assert!(
        matches!(done(&terminal), Some(Ok(_))),
        "cycle must stop instead of scheduling another read"
    );
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(&done(&terminal).unwrap().unwrap()).unwrap();
    assert_eq!(rows.len(), 300);
    assert!(terminal.iter().any(|a| matches!(a,
        Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE
            && event.fields == vec![("command", safe_cex_engine::Value::Num(77.0))]
    )));
}
