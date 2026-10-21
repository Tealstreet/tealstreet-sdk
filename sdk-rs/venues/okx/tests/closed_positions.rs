//! Venue-reported closed positions: raw positions-history rows, paged backwards by close time.
#![allow(dead_code)]
mod support;

use safe_cex_engine::{Action, Command, Engine, Event, Notice, VenueAction};
use safe_cex_venue_okx::{Okx, Options};
use support::*;

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
    actions(e)
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
    let second = request(&mut e);
    assert!(
        url(&second).contains("&after=1709499999999&limit=100"),
        "{}",
        url(&second)
    );
    let last = [row(100, 1_709_400_000_000)];
    reply(&mut e, &second, &page(&last), NOW);
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
    for _ in 0..100 {
        let a = request(&mut e);
        let rows: Vec<String> = (0..100).map(|i| row(i, time - i as u64)).collect();
        time -= 1_000;
        reply(&mut e, &a, &page(&rows), NOW);
        out = actions(&mut e);
        if let Some(timer) = out.iter().find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(*timer),
            _ => None,
        }) {
            e.handle(NOW, Event::TimerFired { timer });
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
