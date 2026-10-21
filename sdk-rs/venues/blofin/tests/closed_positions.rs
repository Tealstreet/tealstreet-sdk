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
    h.collect();
}

fn result(h: &Harness) -> Option<Result<String, String>> {
    h.actions.iter().rev().find_map(|a| match a {
        Action::Notice(Notice::CommandDone { command: 9, result }) => Some(result.clone()),
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
}
