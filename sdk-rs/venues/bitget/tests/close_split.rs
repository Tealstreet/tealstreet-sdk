//! The free/locked split quoted in a Classic close refusal comes only from the
//! exact position row occurrence the Store admitted; every other case falls
//! back to the text without numbers (review probes from #3203).
mod support;
use safe_cex_engine::{Command, Event};
use serde_json::{Value, json};
use support::*;

const HELD: &str = r#"{"code":"00000","data":[{"symbol":"BTCUSDT","holdSide":"long","total":"0.749","available":"0.197","locked":"0.552","openPriceAvg":"90","markPrice":"100","leverage":"20","marginMode":"crossed","posMode":"one_way_mode","uTime":"100"}]}"#;
const REFUSAL: &str = "Not enough position is available";
const STALE: &str = "only 0.197 of your 0.749";
const FRESH: &str = "only 0 of your 0.749 BTCUSDT long position is free to close; 0.749 is held by open close orders; cancel them first (Not enough position is available)";
const NO_NUMBERS: &str = "part of your BTCUSDT long position is held by open close orders; cancel them first (Not enough position is available)";

fn settled() -> Harness {
    let mut h = Harness::private_started();
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"40084","msg":"Classic Account mode","data":null}"#,
    );
    h.respond("/all-position", 200, HELD);
    h.respond("/account/accounts", 200, ACCOUNT);
    h.orders(OPEN_ORDER);
    h.login();
    while h.requests.iter().any(|r| r.url.contains("/all-position")) {
        h.respond("/all-position", 200, HELD);
    }
    h
}

/// The refusal text for a one-way long close of 0.138.
fn refusal(h: &mut Harness) -> String {
    h.command(Command::PlaceOrders {
        command: 91,
        orders: vec![
            serde_json::from_value(json!({"symbol":"BTCUSDT","type":"market","side":"sell","amount":0.138,"reduceOnly":true}))
                .unwrap(),
        ],
    });
    let request = h.take("/batch-place-order");
    h.reply(
        request.id,
        200,
        &json!({"code":"40757","msg":REFUSAL,"data":null}).to_string(),
    );
    h.failures.last().unwrap().message.clone()
}

/// A newer row: nothing free, all locked, and a new entry so the kept row shows.
fn fresh_row(with_time: bool) -> Value {
    let mut response: Value = serde_json::from_str(HELD).unwrap();
    let mut row = response["data"][0].take();
    row["available"] = json!("0");
    row["locked"] = json!("0.749");
    row["openPriceAvg"] = json!("91");
    if !with_time {
        row.as_object_mut().unwrap().remove("uTime");
    }
    row
}

fn stream(h: &mut Harness, rows: Vec<Value>) {
    h.private_message(
        &json!({"arg":{"channel":"positions"},"action":"update","data":rows}).to_string(),
    );
}

fn read_positions(h: &mut Harness) -> safe_cex_engine::RequestId {
    h.command(Command::Venue {
        command: 90,
        action: safe_cex_engine::VenueAction::ReadPositions,
    });
    h.take("/all-position").id
}

#[test]
fn an_admitted_rest_row_quotes_its_split() {
    let mut h = settled();
    let message = refusal(&mut h);
    assert!(
        message.contains("only 0.197 of your 0.749 BTCUSDT long position is free to close; 0.552 is held by open close orders; cancel them first (Not enough position is available)"),
        "{message}"
    );
}

#[test]
fn an_admitted_stream_row_replaces_the_split() {
    let mut h = settled();
    h.now += 1.0;
    stream(&mut h, vec![fresh_row(true)]);
    let message = refusal(&mut h);
    assert!(message.contains(FRESH), "{message}");
}

#[test]
fn a_row_without_a_split_quotes_no_numbers() {
    let mut h = settled();
    let mut row = fresh_row(true);
    row.as_object_mut().unwrap().remove("available");
    row.as_object_mut().unwrap().remove("locked");
    h.now += 1.0;
    stream(&mut h, vec![row]);
    let message = refusal(&mut h);
    assert!(message.contains(NO_NUMBERS), "{message}");
}

#[test]
fn a_rejected_rest_row_with_an_equal_update_time_never_writes_its_split() {
    let mut h = settled();
    let old = read_positions(&mut h);
    h.now += 1.0;
    stream(&mut h, vec![fresh_row(true)]);
    h.reply(old, 200, HELD);
    assert_eq!(h.engine.store().positions()[0].entry_price, 91.0);
    let message = refusal(&mut h);
    assert!(!message.contains(STALE), "{message}");
}

#[test]
fn a_rejected_rest_row_without_update_times_never_writes_its_split() {
    let mut h = settled();
    let old = read_positions(&mut h);
    h.now += 1.0;
    stream(&mut h, vec![fresh_row(false)]);
    h.reply(old, 200, &HELD.replace(r#","uTime":"100""#, ""));
    assert_eq!(h.engine.store().positions()[0].entry_price, 91.0);
    let message = refusal(&mut h);
    assert!(!message.contains(STALE), "{message}");
}

#[test]
fn a_pre_reconnect_rest_row_never_writes_its_split() {
    let mut h = settled();
    let old = read_positions(&mut h);
    let socket = h
        .sockets
        .iter()
        .rev()
        .find(|(_, u)| u.ends_with("/private"))
        .unwrap()
        .0;
    let timers: Vec<_> = h.timers.iter().map(|(id, _)| *id).collect();
    h.event(Event::WsClosed { socket, code: 1006 });
    let (timer, at) = h
        .timers
        .iter()
        .copied()
        .filter(|(id, _)| !timers.contains(id))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap();
    h.timers.retain(|(id, _)| *id != timer);
    h.now = at;
    h.event(Event::TimerFired { timer });
    let next = h
        .sockets
        .iter()
        .rev()
        .find(|(_, u)| u.ends_with("/private"))
        .unwrap()
        .0;
    assert_ne!(socket, next);
    h.event(Event::WsOpened { socket: next });
    h.private_message(r#"{"event":"login","code":0,"msg":""}"#);
    h.now += 1.0;
    stream(&mut h, vec![fresh_row(true)]);
    h.reply(old, 200, HELD);
    assert_eq!(h.engine.store().positions()[0].entry_price, 91.0);
    let message = refusal(&mut h);
    assert!(!message.contains(STALE), "{message}");
}

#[test]
fn duplicate_stream_rows_keep_each_rows_own_split() {
    let mut h = settled();
    let response: Value = serde_json::from_str(HELD).unwrap();
    let first = response["data"][0].clone();
    let mut last = fresh_row(true);
    last["uTime"] = json!("101");
    h.now += 1.0;
    stream(&mut h, vec![first, last]);
    let p = &h.engine.store().positions()[0];
    assert_eq!(
        p.entry_price, 91.0,
        "the last duplicate is the admitted row"
    );
    assert_eq!(p.last_update, Some(101.0));
    let message = refusal(&mut h);
    assert!(message.contains(FRESH), "{message}");
}

#[test]
fn a_mode_rejected_rest_row_never_writes_its_split() {
    let mut h = settled();
    let current = json!({"code":"00000","data":[fresh_row(true)]});
    h.command(Command::Venue {
        command: 89,
        action: safe_cex_engine::VenueAction::ReadPositions,
    });
    h.respond("/all-position", 200, &current.to_string());
    assert_eq!(h.engine.store().positions()[0].entry_price, 91.0);
    let response: Value = serde_json::from_str(HELD).unwrap();
    let oneway = response["data"][0].clone();
    let mut hedge = oneway.clone();
    hedge["posMode"] = json!("hedge_mode");
    let id = read_positions(&mut h);
    h.reply(
        id,
        200,
        &json!({"code":"00000","data":[oneway, hedge]}).to_string(),
    );
    assert_eq!(h.engine.store().positions().len(), 1);
    assert_eq!(h.engine.store().positions()[0].entry_price, 91.0);
    let message = refusal(&mut h);
    assert!(!message.contains(STALE), "{message}");
}

#[test]
fn duplicate_rest_rows_quote_no_numbers() {
    let mut h = settled();
    let response: Value = serde_json::from_str(HELD).unwrap();
    let first = response["data"][0].clone();
    let second = fresh_row(true);
    h.now += 6_000.0;
    let id = read_positions(&mut h);
    h.reply(
        id,
        200,
        &json!({"code":"00000","data":[first, second]}).to_string(),
    );
    let message = refusal(&mut h);
    assert!(!message.contains("only "), "{message}");
}

#[test]
fn an_absent_position_never_quotes_a_kept_split() {
    let mut h = settled();
    h.command(Command::PlaceOrders {
        command: 91,
        orders: vec![
            serde_json::from_value(json!({"symbol":"BTCUSDT","type":"market","side":"sell","amount":0.138,"reduceOnly":true}))
                .unwrap(),
        ],
    });
    let close = h.take("/batch-place-order");
    h.now += 6_000.0;
    let id = read_positions(&mut h);
    h.reply(id, 200, r#"{"code":"00000","data":[]}"#);
    assert!(h.engine.store().positions().is_empty());
    h.reply(
        close.id,
        200,
        &json!({"code":"40757","msg":REFUSAL,"data":null}).to_string(),
    );
    let message = &h.failures.last().unwrap().message;
    assert!(message.contains(NO_NUMBERS), "{message}");
}
