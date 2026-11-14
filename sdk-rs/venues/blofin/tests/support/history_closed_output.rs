//! Venue-reported closed positions: raw positions-history rows, deduped and paged by historyId.
#[allow(dead_code)]
#[path = "../support/mod.rs"]
mod support;
use safe_cex_engine::{Action, Event, Notice};
use support::*;

fn fetch(h: &mut Harness, symbol: Option<&str>, from: Option<f64>, to: Option<f64>) {
    h.handle(
        1.0,
        Event::Command(safe_cex_engine::command::decode_command(&serde_json::json!({"op":"fetchClosedPnl", "command":9, "symbol":symbol, "from":from, "to":to, "account":"fixture-account"}).to_string()).unwrap()),
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
fn reported_closed_pnl_is_shaped_before_the_native_terminal() {
    let mut h = Harness::new();
    h.seed();
    fetch(&mut h, Some("BTCUSDT"), Some(1.), Some(2.));
    h.reply("positions-history", &page(&[row("101", "position-1")]));
    let result: serde_json::Value = serde_json::from_str(&result(&h).unwrap().unwrap()).unwrap();
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../engine/tests/fixtures/history-closed-output.json"
    ))
    .unwrap();
    assert_eq!(result, expected["blofin"]);
}
