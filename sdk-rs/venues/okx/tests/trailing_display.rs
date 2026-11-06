#![allow(dead_code)]
//! A trailing stop's row price is where Tealstreet draws and drags its line (NX
//! `OrderLineManager` sets the line at `order.price`), so it must be the trigger level.
mod support;

use safe_cex_engine::{Action, Engine};
use safe_cex_venue_okx::Okx;
use support::*;

/// A sell trailing stop 50 behind last; OKX publishes its live trigger as moveTriggerPx.
/// `last` on an algo row is the placement-time fill price, not the current price.
const TRAILING: &str = r#"{"instId":"BTC-USDT-SWAP","algoId":"91","ordType":"move_order_stop","state":"live","side":"sell","sz":"4","tdMode":"cross","posSide":"net","reduceOnly":"true","algoClOrdId":"OldTrail91","callbackSpread":"50","callbackRatio":"","activePx":"","moveTriggerPx":"39500","last":"38000"}"#;

/// Startup with `row` as the only trailing stop and `position` (or none), positions first.
fn loaded(row: &str, position: Option<&str>) -> Engine<Okx> {
    let mut e = engine(FixtureOptions {
        key: Some("test-key".into()),
        secret: Some("test-secret".into()),
        passphrase: Some("test-passphrase".into()),
        ..Default::default()
    });
    e.start(0.);
    let a = take_http(&mut prepared(&mut e, 0.));
    reply(&mut e, &a, &envelope(MARKET), 1.);
    let mut out = prepared(&mut e, 1.);
    let ticker = take_http(&mut out);
    reply(&mut e, &ticker, &envelope(TICKER), 2.);
    out.extend(prepared(&mut e, 2.));
    let config = out
        .iter()
        .find(|a| is_get(a, "/api/v5/account/config"))
        .unwrap();
    reply(
        &mut e,
        config,
        &envelope(r#"{"posMode":"net_mode","acctLv":"2"}"#),
        3.,
    );
    let mut pending = prepared(&mut e, 3.);
    while !pending.is_empty() {
        // Positions land before the algo rows that are mapped against them.
        pending.sort_by_key(|a| matches!(a, Action::Http(r) if r.url.contains("ordType=")));
        let a = pending.remove(0);
        let Action::Http(r) = &a else { continue };
        let rows = if r.url.contains("ordType=move_order_stop") {
            row
        } else if r.url.contains("/account/positions") {
            position.unwrap_or("")
        } else if r.url.contains("account-position-risk") {
            r#"{"balData":[{"ccy":"USDT","eq":"5000"}]}"#
        } else {
            ""
        };
        reply(&mut e, &a, &envelope(rows), 4.);
        pending.extend(prepared(&mut e, 4.));
    }
    e
}

fn row(side: &str, trigger: &str) -> String {
    TRAILING
        .replace("\"side\":\"sell\"", &format!("\"side\":\"{side}\""))
        .replace(
            "\"moveTriggerPx\":\"39500\"",
            &format!("\"moveTriggerPx\":\"{trigger}\""),
        )
}

const LONG: &str = r#"{"instId":"BTC-USDT-SWAP","pos":"5","posSide":"net","avgPx":"39000","lever":"10","upl":"50","liqPx":"30000","mgnMode":"cross","imr":"200","mmr":"20"}"#;
const SHORT: &str = r#"{"instId":"BTC-USDT-SWAP","pos":"-5","posSide":"net","avgPx":"41000","lever":"10","upl":"50","liqPx":"50000","mgnMode":"cross","imr":"200","mmr":"20"}"#;

#[test]
fn a_trailing_row_shows_its_trigger_level_with_or_without_a_position() {
    // (side, moveTriggerPx, position, shown price); last is 40000.
    let (mut shown_prices, mut expected) = (Vec::new(), Vec::new());
    for (side, trigger, position, shown) in [
        ("sell", "39500", None, 39500.),
        ("sell", "39500", Some(LONG), 39500.),
        ("buy", "40500", None, 40500.),
        ("buy", "40500", Some(SHORT), 40500.),
        // Not yet published (activation pending): the stop sits a callback behind last.
        ("sell", "", None, 39950.),
        ("sell", "", Some(LONG), 39950.),
        ("buy", "", None, 40050.),
        ("buy", "", Some(SHORT), 40050.),
    ] {
        let e = loaded(&row(side, trigger), position);
        let price = e.store().order("91_tsl").expect("trailing row").price;
        shown_prices.push((side, trigger, position.is_some(), price));
        expected.push((side, trigger, position.is_some(), shown));
    }
    assert_eq!(shown_prices, expected);
}

#[test]
fn a_ratio_trailing_row_without_a_trigger_sits_its_ratio_behind_last() {
    let ratio = TRAILING
        .replace(
            "\"callbackSpread\":\"50\",\"callbackRatio\":\"\"",
            "\"callbackSpread\":\"\",\"callbackRatio\":\"0.01\"",
        )
        .replace("\"moveTriggerPx\":\"39500\"", "\"moveTriggerPx\":\"\"");
    let e = loaded(&ratio, None);
    assert_eq!(e.store().order("91_tsl").unwrap().price, 39600.);
}
