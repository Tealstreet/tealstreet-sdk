//! Venue-reported closed positions: raw positions-history rows, paged backwards by close time.
#[allow(dead_code)]
#[path = "../support/mod.rs"]
mod support;

use safe_cex_engine::{Action, Engine, Event, Notice};
use safe_cex_venue_okx::Okx;
use support::*;

const NOW: f64 = 1_710_000_000_000.0;

fn fetch(e: &mut Engine<Okx>, symbol: Option<&str>, from: Option<f64>, to: Option<f64>) {
    e.handle(
        NOW,
        Event::Command(safe_cex_engine::command::decode_command(&serde_json::json!({"op":"fetchClosedPnl", "command":77, "symbol":symbol, "from":from, "to":to, "account":"fixture-account"}).to_string()).unwrap()),
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
fn reported_closed_pnl_is_shaped_before_the_native_terminal() {
    let mut e = trading_engine();
    fetch(&mut e, Some("BTCUSDT"), Some(1.), Some(NOW));
    let first = request(&mut e);
    reply(&mut e, &first, &page(&[row(2, 1709500000000)]), NOW);
    let result: serde_json::Value =
        serde_json::from_str(&done(&actions(&mut e)).unwrap().unwrap()).unwrap();
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../engine/tests/fixtures/history-closed-output.json"
    ))
    .unwrap();
    assert_eq!(result, expected["okx"]);
}

#[test]
fn reported_output_uses_the_catalogue_at_completion_like_the_sdk_consumer() {
    let mut e = trading_engine();
    fetch(&mut e, Some("BTCUSDT"), Some(1.), Some(NOW));
    let first = request(&mut e);
    let mut markets = e.store().markets().to_vec();
    markets[0].contract_size = Some(0.02);
    e.store_mut().set_markets(markets);
    reply(&mut e, &first, &page(&[row(2, 1709500000000)]), NOW);
    let result: serde_json::Value =
        serde_json::from_str(&done(&actions(&mut e)).unwrap().unwrap()).unwrap();
    assert_eq!(result[0]["contracts"], 0.04);
    assert_eq!(result[0]["notional"], 1640.);
}
