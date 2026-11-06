#![allow(dead_code)]
//! A trailing stop's price is the stop level Tealstreet shows and sends (chart drags and
//! `trail` send a level); OKX trails by callbackSpread, a price distance, or callbackRatio.
mod support;

use safe_cex_engine::model::*;
use safe_cex_engine::{Action, Command, Engine, Event, Method, Notice, Outcome};
use safe_cex_venue_okx::Okx;
use support::*;

/// A sell trailing stop 50 behind the 40000 last price, activating at 41000.
const TRAILING: &str = r#"{"instId":"BTC-USDT-SWAP","algoId":"91","ordType":"move_order_stop","state":"live","side":"sell","sz":"4","tdMode":"cross","posSide":"net","reduceOnly":"true","algoClOrdId":"OldTrail91","callbackSpread":"50","callbackRatio":"","activePx":"41000","moveTriggerPx":"39500","last":"40000"}"#;

/// The trailing row on `side`; a buy stop's trigger sits above last.
fn side_row(side: &str) -> String {
    if side == "buy" {
        TRAILING
            .replace("\"side\":\"sell\"", "\"side\":\"buy\"")
            .replace("\"moveTriggerPx\":\"39500\"", "\"moveTriggerPx\":\"40500\"")
    } else {
        TRAILING.into()
    }
}

fn native_engine(row: &str) -> Engine<Okx> {
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
    while let Some(a) = pending.pop() {
        let Action::Http(r) = &a else { continue };
        let rows = if r.url.contains("ordType=move_order_stop") {
            row
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

fn post(out: &[Action], suffix: &str) -> Action {
    out.iter()
        .find(|a| matches!(a, Action::Http(r) if r.url.ends_with(suffix)))
        .unwrap_or_else(|| {
            let urls: Vec<_> = out
                .iter()
                .filter_map(|a| match a {
                    Action::Http(r) => Some(format!("{:?} {}", r.method, r.url)),
                    _ => None,
                })
                .collect();
            panic!("missing {suffix}: {urls:?} {out:?}")
        })
        .clone()
}

fn body(a: &Action) -> serde_json::Value {
    let Action::Http(r) = a else {
        panic!("not HTTP")
    };
    serde_json::from_str(r.body.as_deref().unwrap()).unwrap()
}

fn raw(a: &Action) -> String {
    let Action::Http(r) = a else {
        panic!("not HTTP")
    };
    r.body.clone().unwrap()
}

fn writes(out: &[Action]) -> usize {
    out.iter()
        .filter(|a| matches!(a, Action::Http(r) if r.method != Method::Get))
        .count()
}

fn failure(out: &[Action], command: u32) -> &safe_cex_engine::VenueError {
    out.iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: c, error }) if *c == command => {
                Some(error)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("command {command} failure: {out:?}"))
}

/// The replacement body after the cancel of `91_tsl` landed.
fn replacement_after_edit(e: &mut Engine<Okx>, price: f64) -> serde_json::Value {
    e.handle(
        5.,
        Event::Command(Command::AmendOrder {
            command: 20,
            id: "91_tsl".into(),
            price: Some(price),
            amount: None,
        }),
    );
    let cancel = post(&prepared(e, 5.), "/cancel-algos");
    reply(e, &cancel, &envelope(r#"{"algoId":"91","sCode":"0"}"#), 6.);
    body(&post(&prepared(e, 6.), "/order-algo"))
}

#[test]
fn a_trailing_edit_sends_the_distance_from_last_to_the_new_level() {
    for (side, level, spread) in [("sell", 39940., "60"), ("buy", 40100., "100")] {
        let row = side_row(side);
        let mut e = native_engine(&row);
        let b = replacement_after_edit(&mut e, level);
        assert_eq!(b["callbackSpread"], spread, "{side} {level}");
        assert!(b.get("callbackRatio").is_none(), "{b}");
        assert_eq!(b["activePx"], "41000", "activation is unchanged");
        assert_eq!(b["sz"], "4");
    }
}

#[test]
fn a_ratio_trailing_edit_stays_a_ratio_of_last() {
    let row = TRAILING.replace(
        "\"callbackSpread\":\"50\",\"callbackRatio\":\"\"",
        "\"callbackSpread\":\"\",\"callbackRatio\":\"0.005\"",
    );
    let mut e = native_engine(&row);
    let b = replacement_after_edit(&mut e, 39600.);
    assert_eq!(b["callbackRatio"], "0.01");
    assert!(b.get("callbackSpread").is_none(), "{b}");
    assert_eq!(b["activePx"], "41000");
}

#[test]
fn a_trailing_edit_level_that_would_not_trail_is_refused_before_any_cancel() {
    for (side, level) in [
        ("sell", 40000.),
        ("sell", 40100.),
        ("buy", 39900.),
        ("buy", 40000.),
    ] {
        let row = side_row(side);
        let mut e = native_engine(&row);
        let before = e.store().order("91_tsl").unwrap().clone();
        e.handle(
            5.,
            Event::Command(Command::AmendOrder {
                command: 20,
                id: "91_tsl".into(),
                price: Some(level),
                amount: None,
            }),
        );
        let out = prepared(&mut e, 5.);
        assert_eq!(writes(&out), 0, "{side} {level}: {out:?}");
        let error = failure(&out, 20);
        assert_eq!(error.outcome, Some(Outcome::NotSent), "{side} {level}");
        assert!(
            error.message.contains("the stop is unchanged"),
            "{side} {level}: {}",
            error.message
        );
        assert_eq!(e.store().order("91_tsl"), Some(&before));
    }
}

fn place(e: &mut Engine<Okx>, now: f64, side: &str, price: f64) -> Vec<Action> {
    let order: PlaceOrderOpts = serde_json::from_str(&format!(
        r#"{{"symbol":"BTCUSDT","type":"trailing_stop_market","side":"{side}","amount":1,"price":{price},"reduceOnly":true}}"#
    ))
    .unwrap();
    e.handle(
        now,
        Event::Command(Command::PlaceOrders {
            command: 30,
            orders: vec![order],
        }),
    );
    prepared(e, now)
}

#[test]
fn a_trailing_placement_sends_the_distance_from_last_to_its_level() {
    for (side, level, spread) in [("sell", 39940., "60"), ("buy", 40100., "100")] {
        let mut e = trading_engine();
        let out = place(&mut e, 5., side, level);
        let b = body(&post(&out, "/order-algo"));
        assert_eq!(b["ordType"], "move_order_stop");
        assert_eq!(b["callbackSpread"], spread, "{side} {level}");
        // Master's empty ratio and activation fields are kept as they were.
        assert_eq!(b["callbackRatio"], "", "{b}");
        assert_eq!(b["activePx"], "", "{b}");
    }
}

#[test]
fn a_trailing_placement_that_would_not_trail_or_has_no_fresh_last_is_refused() {
    for (side, level, now) in [
        ("sell", 40000., 5.),
        ("sell", 40100., 5.),
        ("buy", 39900., 5.),
        ("sell", 39940., 10_000.),
    ] {
        let mut e = trading_engine();
        let out = place(&mut e, now, side, level);
        assert_eq!(writes(&out), 0, "{side} {level} at {now}: {out:?}");
        assert_eq!(
            failure(&out, 30).outcome,
            Some(Outcome::NotSent),
            "{side} {level} at {now}"
        );
    }
}

/// Ordinary limit edits and placements keep master's exact bytes.
#[test]
fn ordinary_limit_placement_and_amend_bodies_are_unchanged() {
    let mut e = trading_engine();
    let order: PlaceOrderOpts = serde_json::from_str(
        r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":2,"price":38000,"clientOrderId":"Pinned1"}"#,
    )
    .unwrap();
    e.handle(
        5.,
        Event::Command(Command::PlaceOrders {
            command: 40,
            orders: vec![order],
        }),
    );
    let placed = raw(&post(&prepared(&mut e, 5.), "/batch-orders"));
    assert_eq!(
        placed,
        r#"[{"instId":"BTC-USDT-SWAP","tdMode":"cross","tag":"e847386590ce4dBC","side":"buy","ordType":"limit","clOrdId":"Pinned1","sz":"2","px":"38000"}]"#
    );
    e.store_mut().upsert_order(Order {
        id: "77".into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "BTCUSDT".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        price: 38000.,
        amount: 2.,
        filled: 0.,
        remaining: 2.,
        reduce_only: false,
        close: None,
        client_order_id: Some("Pinned1".into()),
        time_in_force: Some(TimeInForce::GoodTillCancel),
        last_update: 4.,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    });
    e.handle(
        6.,
        Event::Command(Command::AmendOrder {
            command: 41,
            id: "77".into(),
            price: Some(38100.),
            amount: Some(3.),
        }),
    );
    let amended = raw(&post(&prepared(&mut e, 6.), "/amend-order"));
    assert_eq!(
        amended,
        r#"{"instId":"BTC-USDT-SWAP","ordId":"77","newPx":"38100","newSz":"3"}"#
    );
}
