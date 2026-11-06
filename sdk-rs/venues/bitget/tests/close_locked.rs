//! A Classic reduce-only close against a position partly held by resting close
//! orders: Bitget shrinks a market close to the unlocked size, and refuses one
//! when nothing is unlocked (live report, v2 batch-place-order, one-way).
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
