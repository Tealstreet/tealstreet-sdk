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

/// A stop whose activation price is not reached yet: OKX publishes no moveTriggerPx.
fn pending_row(side: &str, activation: &str) -> String {
    row(side, "").replace(
        "\"activePx\":\"\"",
        &format!("\"activePx\":\"{activation}\""),
    )
}

#[test]
fn an_activation_pending_row_sits_its_callback_behind_the_activation_price() {
    let ratio = pending_row("sell", "41000").replace(
        "\"callbackSpread\":\"50\",\"callbackRatio\":\"\"",
        "\"callbackSpread\":\"\",\"callbackRatio\":\"0.01\"",
    );
    for (row, position, shown) in [
        (pending_row("sell", "41000"), None, 40950.),
        (pending_row("sell", "41000"), Some(LONG), 40950.),
        (pending_row("buy", "39000"), None, 39050.),
        (pending_row("buy", "39000"), Some(SHORT), 39050.),
        (ratio, None, 40590.),
    ] {
        let e = loaded(&row, position);
        assert_eq!(e.store().order("91_tsl").unwrap().price, shown, "{row}");
    }
}

fn edit(e: &mut Engine<Okx>, price: Option<f64>, amount: Option<f64>) -> Vec<Action> {
    e.handle(
        5.,
        safe_cex_engine::Event::Command(safe_cex_engine::Command::AmendOrder {
            command: 20,
            id: "91_tsl".into(),
            price,
            amount,
        }),
    );
    prepared(e, 5.)
}

fn writes(out: &[Action]) -> Vec<String> {
    out.iter()
        .filter_map(|a| match a {
            Action::Http(r) if r.method != safe_cex_engine::Method::Get => Some(r.url.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn an_activation_pending_trailing_price_edit_is_refused_before_any_cancel() {
    for (side, activation, level) in [("sell", "41000", 40900.), ("buy", "39000", 39100.)] {
        let mut e = loaded(&pending_row(side, activation), None);
        let before = e.store().order("91_tsl").unwrap().clone();
        let out = edit(&mut e, Some(level), None);
        assert!(writes(&out).is_empty(), "{side}: {out:?}");
        let error = out
            .iter()
            .find_map(|a| match a {
                Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 20, error }) => {
                    Some(error)
                }
                _ => None,
            })
            .expect("refused edit");
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::NotSent));
        assert_eq!(
            error.message,
            "OKX trailing stop not active yet; re-place it to change its distance"
        );
        assert_eq!(e.store().order("91_tsl"), Some(&before));
        // A size edit has no level to convert, so it still re-places the stop.
        let mut e = loaded(&pending_row(side, activation), None);
        let out = edit(&mut e, None, Some(5.));
        assert!(
            writes(&out)
                .iter()
                .any(|url| url.ends_with("/cancel-algos")),
            "{side}: {out:?}"
        );
    }
}
