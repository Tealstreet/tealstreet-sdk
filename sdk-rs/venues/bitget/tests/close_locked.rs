//! A Classic reduce-only close against a position partly held by resting close
//! orders. Observed live (v2 batch-place-order, one-way): Bitget shrank a market
//! close to the unlocked size and refused later ones with nothing unlocked. The
//! shrink fixture pins that observation, not a documented protocol rule.
mod support;
use safe_cex_engine::{Command, Outcome};
use serde_json::{Value, json};
use support::*;

const HELD: &str = r#"{"code":"00000","data":[{"symbol":"BTCUSDT","holdSide":"long","total":"0.749","available":"0.197","locked":"0.552","openPriceAvg":"90","markPrice":"100","leverage":"20","marginMode":"crossed","posMode":"one_way_mode","uTime":"100"}]}"#;
const REFUSAL: &str = "Not enough position is available";

fn held() -> Harness {
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
    h
}

fn close(h: &mut Harness, command: u32, amount: f64) -> (Value, safe_cex_engine::RequestId) {
    h.command(Command::PlaceOrders {
        command,
        orders: vec![
            serde_json::from_value(json!({"symbol":"BTCUSDT","type":"market","side":"sell","amount":amount,"reduceOnly":true}))
                .unwrap(),
        ],
    });
    let request = h.take("/batch-place-order");
    let body: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    (body, request.id)
}

fn result(h: &Harness, command: u32) -> &Result<String, String> {
    &h.done.iter().find(|(id, _)| *id == command).unwrap().1
}

fn drained(h: &mut Harness) {
    let after = HELD
        .replace(
            r#""total":"0.749","available":"0.197""#,
            r#""total":"0.552","available":"0""#,
        )
        .replace(r#""uTime":"100""#, r#""uTime":"200""#);
    let rows: Value = serde_json::from_str(&after).unwrap();
    h.private_message(
        &json!({"arg":{"channel":"positions"},"action":"snapshot","data":rows["data"]}).to_string(),
    );
}

#[test]
fn bitget_shrinks_a_market_close_to_the_unlocked_size_and_the_store_follows() {
    let mut h = held();
    let (body, id) = close(&mut h, 1, 0.487);
    // Sent as asked: no pre-send cap from the cached split.
    assert_eq!(body["orderList"][0]["size"], "0.487");
    assert_eq!(body["orderList"][0]["reduceOnly"], "YES");
    h.reply(
        id,
        200,
        r#"{"code":"00000","data":{"successList":[{"orderId":"shrunk","clientOid":"c"}],"failureList":[]}}"#,
    );
    assert!(result(&h, 1).is_ok(), "{:?}", h.done);
    h.private_message(
        &json!({"arg":{"channel":"orders"},"data":[{
            "orderId":"shrunk", "symbol":"BTCUSDT", "size":"0.197", "accBaseVolume":"0.197",
            "priceAvg":"75502", "side":"sell", "reduceOnly":"yes", "posMode":"one_way_mode",
            "orderType":"market", "status":"filled", "cTime":"150", "uTime":"150"
        }]})
        .to_string(),
    );
    drained(&mut h);
    let position = h
        .engine
        .store()
        .positions()
        .iter()
        .find(|p| p.symbol == "BTCUSDT")
        .unwrap()
        .clone();
    assert_eq!(position.contracts, 0.552);
    if let Some(order) = h.engine.store().order("shrunk") {
        assert_eq!(order.amount, 0.197, "the booked size, not the sent one");
    }
}

#[test]
fn a_refused_close_names_the_held_size_in_the_failure_list() {
    let mut h = held();
    drained(&mut h);
    let (_, id) = close(&mut h, 2, 0.138);
    h.reply(
        id,
        200,
        &json!({"code":"00000","data":{"successList":[],"failureList":[
            {"clientOid":"c","errorMsg":REFUSAL}
        ]}})
        .to_string(),
    );
    let error = h.failures.last().unwrap();
    assert!(
        error.message.contains(
            "only 0 of the 0.552 BTCUSDT long is free to close; 0.552 is held by open close orders, cancel them first (Not enough position is available)"
        ),
        "{}",
        error.message
    );
    assert_ne!(error.outcome, Some(Outcome::Unknown));
    assert!(result(&h, 2).is_err());
    assert!(
        !h.requests
            .iter()
            .any(|r| r.url.contains("/batch-place-order")),
        "a refusal is never resent"
    );
}

#[test]
fn a_coded_refusal_names_the_held_size() {
    let mut h = held();
    drained(&mut h);
    let (_, id) = close(&mut h, 3, 0.138);
    h.reply(
        id,
        200,
        &json!({"code":"40757","msg":REFUSAL,"data":null}).to_string(),
    );
    let error = h.failures.last().unwrap();
    assert_eq!(
        error.message,
        "Bitget 40757: only 0 of the 0.552 BTCUSDT long is free to close; 0.552 is held by open close orders, cancel them first (Not enough position is available) (HTTP 200)"
    );
    assert_eq!(error.outcome, Some(Outcome::Rejected));
    assert!(
        !h.requests
            .iter()
            .any(|r| r.url.contains("/batch-place-order"))
    );
}

#[test]
fn a_split_the_store_no_longer_matches_is_not_quoted() {
    let mut h = held();
    // A newer row without the split: its size is current, the split is not.
    h.private_message(r#"{"arg":{"channel":"positions"},"action":"snapshot","data":[{"symbol":"BTCUSDT","holdSide":"long","total":"0.552","openPriceAvg":"90","posMode":"one_way_mode","marginMode":"crossed","uTime":"300"}]}"#);
    let (_, id) = close(&mut h, 4, 0.138);
    h.reply(
        id,
        200,
        &json!({"code":"00000","data":{"successList":[],"failureList":[
            {"clientOid":"c","errorMsg":REFUSAL}
        ]}})
        .to_string(),
    );
    let message = &h.failures.last().unwrap().message;
    assert!(
        message.contains(
            "the BTCUSDT long is held by open close orders, cancel them first (Not enough position is available)"
        ),
        "{message}"
    );
    assert!(!message.contains("0.197"), "{message}");
}

#[test]
fn other_refusals_keep_the_venue_text_byte_for_byte() {
    let mut h = held();
    let (_, id) = close(&mut h, 5, 0.138);
    h.reply(
        id,
        200,
        r#"{"code":"00000","data":{"successList":[],"failureList":[{"clientOid":"c","errorMsg":"Insufficient balance"}]}}"#,
    );
    assert_eq!(
        h.failures.last().unwrap().message,
        "Bitget batch failure after ids []: Insufficient balance"
    );
    let mut h = held();
    let (_, id) = close(&mut h, 6, 0.138);
    h.reply(
        id,
        200,
        r#"{"code":"40762","msg":"order size too big","data":null}"#,
    );
    assert_eq!(
        h.failures.last().unwrap().message,
        "Bitget 40762: order size too big (HTTP 200)"
    );
}

// Review probes (#3203).
#[test]
fn a_late_rest_row_never_replaces_a_newer_stream_split() {
    let mut h = held();
    while h.requests.iter().any(|r| r.url.contains("/all-position")) {
        h.respond("/all-position", 200, HELD);
    }
    h.command(Command::Venue {
        command: 90,
        action: safe_cex_engine::VenueAction::ReadPositions,
    });
    let old = h.take("/all-position");
    h.now += 1.0;
    let fresh = HELD
        .replace(r#""available":"0.197""#, r#""available":"0""#)
        .replace(r#""locked":"0.552""#, r#""locked":"0.749""#)
        .replace(r#""uTime":"100""#, r#""uTime":"200""#);
    let data: Value = serde_json::from_str(&fresh).unwrap();
    h.private_message(
        &json!({"arg":{"channel":"positions"},"action":"update","data":data["data"]}).to_string(),
    );
    h.reply(old.id, 200, HELD);
    assert_eq!(h.engine.store().positions()[0].last_update, Some(200.0));
    let (_, id) = close(&mut h, 91, 0.138);
    h.reply(
        id,
        200,
        &json!({"code":"40757","msg":REFUSAL,"data":null}).to_string(),
    );
    let message = &h.failures.last().unwrap().message;
    assert!(
        message.contains("only 0 of the 0.749 BTCUSDT long"),
        "{message}"
    );
}

fn hedged() -> Harness {
    let mut h = Harness::private_started();
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"40084","msg":"Classic Account mode","data":null}"#,
    );
    let rows = json!({"code":"00000","data":[
        {"symbol":"BTCUSDT","holdSide":"long","total":"0.749","available":"0.197","locked":"0.552","openPriceAvg":"90","markPrice":"100","leverage":"20","marginMode":"crossed","posMode":"hedge_mode","uTime":"100"},
        {"symbol":"BTCUSDT","holdSide":"short","total":"0.5","available":"0","locked":"0.5","openPriceAvg":"90","markPrice":"100","leverage":"20","marginMode":"crossed","posMode":"hedge_mode","uTime":"100"}
    ]});
    h.respond("/all-position", 200, &rows.to_string());
    h.respond("/account/accounts", 200, ACCOUNT);
    h.orders(OPEN_ORDER);
    h.login();
    assert!(h.engine.store().hedged());
    h
}

fn both_closes(h: &mut Harness, command: u32) -> safe_cex_engine::RequestId {
    h.command(Command::PlaceOrders {
        command,
        orders: vec![
            serde_json::from_value(json!({"symbol":"BTCUSDT","type":"market","side":"sell","amount":0.1,"reduceOnly":true,"clientOrderId":"long-close"})).unwrap(),
            serde_json::from_value(json!({"symbol":"BTCUSDT","type":"market","side":"buy","amount":0.1,"reduceOnly":true,"clientOrderId":"short-close"})).unwrap(),
        ],
    });
    h.take("/batch-place-order").id
}

#[test]
fn each_failure_names_its_own_close_in_a_mixed_hedge_batch() {
    let mut h = hedged();
    let id = both_closes(&mut h, 92);
    h.reply(id, 200, &json!({"code":"00000","data":{"successList":[{"orderId":"long-ok","clientOid":"long-close"}],"failureList":[{"clientOid":"short-close","errorCode":"40757","errorMsg":REFUSAL}]}}).to_string());
    let message = &h.failures.last().unwrap().message;
    assert!(
        message.contains("only 0 of the 0.5 BTCUSDT short"),
        "{message}"
    );
    assert!(!message.contains("BTCUSDT long"), "{message}");
}

#[test]
fn an_unattributable_refusal_names_no_side_or_size() {
    // No clientOid on the row, and a whole-batch refusal: either close may be the one.
    for reply in [
        json!({"code":"00000","data":{"successList":[],"failureList":[{"errorMsg":REFUSAL}]}}),
        json!({"code":"00000","data":{"successList":[],"failureList":[{"clientOid":"unknown","errorMsg":REFUSAL}]}}),
        json!({"code":"40757","msg":REFUSAL,"data":null}),
    ] {
        let mut h = hedged();
        let id = both_closes(&mut h, 93);
        h.reply(id, 200, &reply.to_string());
        let message = &h.failures.last().unwrap().message;
        assert!(
            message.contains("part of the BTCUSDT position is held by open close orders, cancel them first (Not enough position is available)"),
            "{reply}: {message}"
        );
        assert!(
            !message.contains("long") && !message.contains("short") && !message.contains("0.5"),
            "{message}"
        );
    }
}
