//! Recorded 2/2 acknowledgement shape; identities are synthetic, not retained live IDs.
use super::*;
use safe_cex_engine::HttpRequest;

const PEER: &str = "resting-reduce-only-protection";
const SLICE: &str = "new-twap-slice";

fn protected_position(partial_tp: bool) -> Harness {
    let mut h = ready();
    let positions: serde_json::Value = serde_json::from_str(POSITIONS).unwrap();
    let mut position = positions["result"]["list"][0].clone();
    position["size"] = "0.002".into();
    position["avgPrice"] = "63855.5".into();
    h.message(
        "/v5/private",
        &serde_json::json!({"topic":"position","data":[position]}).to_string(),
    );
    price_at(&mut h, "BTCUSDT", 63_939.9);
    let mut peer = serde_json::json!({
        "category":"linear","symbol":"BTCUSDT","orderId":PEER,"orderLinkId":"",
        "side":"Sell","orderType":"Limit","orderStatus":"New","price":"64840.7",
        "qty":"0.002","leavesQty":"0.002","cumExecQty":"0","avgPrice":"0",
        "timeInForce":"PostOnly","reduceOnly":true,"positionIdx":0,
        "createdTime":"1000","updatedTime":"1000"
    });
    if partial_tp {
        peer["stopOrderType"] = "TakeProfit".into();
        peer["tpslMode"] = "Partial".into();
        peer["triggerPrice"] = "64840.7".into();
        peer["triggerBy"] = "LastPrice".into();
        peer["orderStatus"] = "Untriggered".into();
        peer["orderType"] = "Market".into();
        peer["price"] = "0".into();
    }
    h.message(
        "/v5/private",
        &serde_json::json!({"topic":"order","data":[peer]}).to_string(),
    );
    assert_eq!(h.engine.store().positions()[0].contracts, 0.002);
    let peer = h
        .engine
        .store()
        .order(PEER)
        .expect("real private row is loaded before dispatch");
    assert!(peer.reduce_only);
    assert_eq!(peer.amount, 0.002);
    h
}

fn slice(h: &mut Harness, command: u32) -> HttpRequest {
    let mut order = limit("BTCUSDT", OrderSide::Sell, 0.001, 63_940.1);
    order.reduce_only = Some(true);
    place(h, command, vec![order]);
    let request = h.take("/v5/order/create-batch");
    let body: serde_json::Value = serde_json::from_str(body(&request)).unwrap();
    assert_eq!(
        body["request"].as_array().unwrap().len(),
        1,
        "one physical target, not two sends"
    );
    assert_eq!(body["request"][0]["qty"], "0.001");
    assert_eq!(body["request"][0]["reduceOnly"], true);
    request
}

fn answer(ids: &[&str], codes: &[i64]) -> String {
    serde_json::json!({"retCode":0,"retMsg":"OK",
        "result":{"list":ids.iter().map(|id|serde_json::json!({"orderId":id,"orderLinkId":""})).collect::<Vec<_>>()},
        "retExtInfo":{"list":codes.iter().map(|code|serde_json::json!({"code":code,"msg":"OK"})).collect::<Vec<_>>()}
    }).to_string()
}

#[test]
fn create_batch_reduce_only_peer_resize_returns_only_the_new_slice() {
    for partial_tp in [false, true] {
        for reversed in [false, true] {
            let mut h = protected_position(partial_tp);
            let request = slice(&mut h, 6100);
            let ids = if reversed {
                [PEER, SLICE]
            } else {
                [SLICE, PEER]
            };
            h.reply(request.id, 200, &answer(&ids, &[0, 0]));
            let result = h.result(6100).expect("native command settles");
            assert!(
                result.is_ok(),
                "known reduce-only peer must not stop the TWAP: {result:?}"
            );
            let ids: Vec<String> = serde_json::from_str(result.as_ref().unwrap()).unwrap();
            assert_eq!(
                ids,
                [SLICE],
                "peer is not a caller order or algo-owned effect"
            );
            assert!(!h.unknown.contains(&6100));
            assert!(!h.has_request("/v5/order/create-batch"));
            assert!(!h.has_request("/v5/order/create"));
            assert_eq!(
                h.engine.store().order(PEER).unwrap().amount,
                0.002,
                "ack does not fabricate a peer edit; authoritative private stream owns it"
            );
        }
    }
}

#[test]
fn create_batch_peer_proof_keeps_misaligned_foreign_and_contradictory_rows_unknown() {
    for (ids, codes) in [
        (vec![SLICE, PEER], vec![0]),
        (vec![SLICE, "unknown-foreign"], vec![0, 0]),
        (vec![SLICE, SLICE], vec![0, 0]),
        (vec![SLICE, PEER], vec![0, 110007]),
    ] {
        let mut h = protected_position(false);
        let request = slice(&mut h, 6101);
        h.reply(request.id, 200, &answer(&ids, &codes));
        assert!(matches!(h.result(6101), Some(Err(_))));
        assert!(h.unknown.contains(&6101));
        assert!(
            !h.has_request("/v5/order/create-batch"),
            "no resend on uncertainty"
        );
        assert!(!h.has_request("/v5/order/create"));
    }
}

#[test]
fn create_batch_single_target_without_a_peer_still_succeeds() {
    let mut h = ready();
    let request = slice(&mut h, 6102);
    h.reply(request.id, 200, &answer(&[SLICE], &[0]));
    assert!(h.result(6102).unwrap().is_ok());
    assert!(!h.has_request("/v5/order/create-batch"));
}

#[test]
fn create_batch_response_time_peer_does_not_supply_dispatch_time_proof() {
    let mut h = ready();
    let request = slice(&mut h, 6103);
    h.message(
        "/v5/private",
        &serde_json::json!({"topic":"order","data":[{
            "category":"linear","symbol":"BTCUSDT","orderId":PEER,"side":"Sell",
            "orderType":"Limit","orderStatus":"New","price":"64840.7","qty":"0.002",
            "leavesQty":"0.002","cumExecQty":"0","reduceOnly":true,"positionIdx":0
        }]})
        .to_string(),
    );
    assert!(h.engine.store().order(PEER).is_some());
    h.reply(request.id, 200, &answer(&[SLICE, PEER], &[0, 0]));
    assert!(matches!(h.result(6103), Some(Err(_))));
    assert!(h.unknown.contains(&6103));
    assert!(!h.has_request("/v5/order/create-batch"));
}

#[test]
fn create_batch_peer_scope_is_symbol_side_reduce_only_and_open() {
    for (field, value) in [
        ("symbol", serde_json::json!("ETHUSDT")),
        ("side", serde_json::json!("Buy")),
        ("reduceOnly", serde_json::json!(false)),
        ("orderStatus", serde_json::json!("Cancelled")),
    ] {
        let mut h = ready();
        let mut peer = serde_json::json!({"category":"linear","symbol":"BTCUSDT",
            "orderId":PEER,"side":"Sell","orderType":"Limit","orderStatus":"New",
            "price":"64840.7","qty":"0.002","leavesQty":"0.002","cumExecQty":"0",
            "reduceOnly":true,"positionIdx":0});
        peer[field] = value;
        h.message(
            "/v5/private",
            &serde_json::json!({"topic":"order","data":[peer]}).to_string(),
        );
        let request = slice(&mut h, 6104);
        h.reply(request.id, 200, &answer(&[SLICE, PEER], &[0, 0]));
        assert!(
            matches!(h.result(6104), Some(Err(_))),
            "{field} cannot prove a peer"
        );
        assert!(h.unknown.contains(&6104));
        assert!(!h.has_request("/v5/order/create-batch"));
    }
}

#[test]
fn create_batch_dispatch_snapshot_survives_a_terminal_peer_stream_update() {
    let mut h = protected_position(false);
    let request = slice(&mut h, 6105);
    h.message(
        "/v5/private",
        &serde_json::json!({"topic":"order","data":[{
            "category":"linear","symbol":"BTCUSDT","orderId":PEER,"side":"Sell",
            "orderType":"Limit","orderStatus":"Cancelled","price":"64840.7","qty":"0.002",
            "leavesQty":"0","cumExecQty":"0","reduceOnly":true,"positionIdx":0
        }]})
        .to_string(),
    );
    h.reply(request.id, 200, &answer(&[PEER, SLICE], &[0, 0]));
    let result = h.result(6105).unwrap().unwrap();
    assert_eq!(
        serde_json::from_str::<Vec<String>>(&result).unwrap(),
        [SLICE]
    );
    assert!(!h.has_request("/v5/order/create-batch"));
}
