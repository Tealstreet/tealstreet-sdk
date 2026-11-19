//! AGT-34: a price-only edit of a partial reduce-only stop must never end with
//! neither the old stop nor its replacement. Live Bybit answers the orderId
//! proof read with its single row AND a non-empty `nextPageCursor`.
use super::*;

/// The page token Bybit returns even when the orderId filter matched one row.
const CURSOR: &str = "page_args%3Dpartial%26symbol_%3DBTCUSDT%26";

/// The incident shape: a 0.001 Sell stop-market protecting a 0.003 long.
fn incident_stop(h: &mut Harness, id: &str) -> safe_cex_engine::model::Order {
    h.message("/v5/private", r#"{"topic":"position","data":[{"category":"linear","symbol":"BTCUSDT","side":"Buy","size":"0.003","avgPrice":"100","markPrice":"100","leverage":"10","unrealisedPnl":"0","positionIdx":0}]}"#);
    h.message("/v5/private", &format!(r#"{{"topic":"order","data":[{{"category":"linear","symbol":"BTCUSDT","orderId":"{id}","side":"Sell","orderType":"Market","orderStatus":"Untriggered","stopOrderType":"StopLoss","tpslMode":"Partial","qty":"0.001","cumExecQty":"0","cumExecValue":"0","avgPrice":"0","price":"0","leavesQty":"0.001","triggerPrice":"90","triggerBy":"MarkPrice","reduceOnly":true,"closeOnTrigger":true,"positionIdx":0}}]}}"#));
    set_reference_prices(h, 100.0, 100.0);
    h.engine.store().order(id).unwrap().clone()
}

fn edit(h: &mut Harness, command: u32, id: &str) {
    h.command(Command::AmendOrder {
        command,
        id: id.into(),
        price: Some(95.0),
        amount: None,
    });
    h.respond(
        "/v5/order/cancel-batch",
        200,
        &format!(
            r#"{{"retCode":0,"result":{{"list":[{{"orderId":"{id}"}}]}},"retExtInfo":{{"list":[{{"code":0,"msg":"OK"}}]}}}}"#
        ),
    );
}

fn page(rows: Vec<serde_json::Value>, cursor: &str) -> String {
    serde_json::json!({"retCode":0,"result":{"category":"linear","list":rows,"nextPageCursor":cursor}})
        .to_string()
}

/// Answers the census reads (with Bybit's cursor) until the replacement is sent.
fn census(h: &mut Harness) {
    for _ in 0..4 {
        if h.has_request("/v5/position/trading-stop") {
            return;
        }
        if h.has_request("&openOnly=0&limit=50") {
            h.respond("&openOnly=0&limit=50", 200, &page(vec![], CURSOR));
        } else if h.has_request("/v5/order/history") {
            h.respond("/v5/order/history", 200, &page(vec![], CURSOR));
        } else {
            h.advance(200.0);
        }
    }
}

fn stop_writes(h: &Harness) -> Vec<serde_json::Value> {
    h.requests
        .iter()
        .filter(|request| request.url.contains("/v5/position/trading-stop"))
        .map(|request| serde_json::from_str(body(request)).unwrap())
        .collect()
}

fn failure(h: &Harness, command: u32) -> &safe_cex_engine::VenueError {
    h.notices
        .iter()
        .find_map(|notice| match notice {
            safe_cex_engine::Notice::CommandFailed { command: c, error } if *c == command => {
                Some(error)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("command {command} did not fail: {:?}", h.result(command)))
}

#[test]
fn incident_cursor_on_the_single_cancelled_row_places_the_replacement() {
    let profile = PROFILES[0];
    let mut h = ready(profile);
    let original = incident_stop(&mut h, "partial");
    edit(&mut h, 91, "partial");
    let row = native_partial_target(&h, profile, &original);
    h.respond("&orderId=partial", 200, &page(vec![row], CURSOR));
    census(&mut h);
    let writes = stop_writes(&h);
    assert_eq!(
        writes.len(),
        1,
        "the replacement must be sent: {:?}",
        h.result(91)
    );
    assert_eq!(writes[0]["stopLoss"], "95");
    assert_eq!(writes[0]["slSize"], "0.001");
    assert_eq!(writes[0]["tpslMode"], "Partial");
    h.respond("/v5/position/trading-stop", 200, OK);
    assert!(
        h.result(91).is_some_and(Result::is_ok),
        "{:?}",
        h.result(91)
    );
    assert!(!h.unknown.contains(&91));
}

#[test]
fn repeated_incident_edits_take_the_same_protected_path_every_time() {
    let profile = PROFILES[0];
    let mut h = ready(profile);
    for (attempt, id) in ["stop-1", "stop-2", "stop-3"].into_iter().enumerate() {
        let command = 91 + attempt as u32;
        let original = incident_stop(&mut h, id);
        edit(&mut h, command, id);
        let row = native_partial_target(&h, profile, &original);
        h.respond(&format!("&orderId={id}"), 200, &page(vec![row], CURSOR));
        census(&mut h);
        let request = h.take("/v5/position/trading-stop");
        let write: serde_json::Value = serde_json::from_str(body(&request)).unwrap();
        assert_eq!(write["stopLoss"], "95", "attempt {attempt}");
        h.reply(request.id, 200, OK);
        assert!(
            h.result(command).is_some_and(Result::is_ok),
            "attempt {attempt}: {:?}",
            h.result(command)
        );
        assert!(!h.unknown.contains(&command), "attempt {attempt}");
        while h.has_request("/v5/order/realtime") {
            h.respond("/v5/order/realtime", 200, EMPTY);
        }
        while h.has_request("/v5/position/list") {
            h.respond("/v5/position/list", 200, EMPTY);
        }
    }
}

#[test]
fn incident_refused_replacement_puts_the_old_stop_back_despite_census_cursors() {
    let profile = PROFILES[0];
    let mut h = ready(profile);
    let original = incident_stop(&mut h, "partial");
    edit(&mut h, 91, "partial");
    let row = native_partial_target(&h, profile, &original);
    h.respond("&orderId=partial", 200, &page(vec![row], CURSOR));
    census(&mut h);
    h.respond(
        "/v5/position/trading-stop",
        200,
        r#"{"retCode":10001,"retMsg":"replacement refused","result":{}}"#,
    );
    let writes = stop_writes(&h);
    assert_eq!(writes.len(), 2, "the old stop must be put back");
    assert_eq!(writes[1]["stopLoss"], "90");
    assert_eq!(writes[1]["slSize"], "0.001");
    h.respond("/v5/position/trading-stop", 200, OK);
    assert_eq!(
        failure(&h, 91).protection,
        Some(safe_cex_engine::error::Protection::Restored)
    );
}

/// Every content-level proof failure after the landed cancel still places the edit.
#[test]
fn any_proof_failure_after_a_landed_cancel_still_places_the_replacement() {
    let profile = PROFILES[0];
    for shape in [
        "two-rows",
        "http-503",
        "mismatched-trigger",
        "wrong-category",
    ] {
        let mut h = ready(profile);
        let original = incident_stop(&mut h, "partial");
        edit(&mut h, 91, "partial");
        let mut row = native_partial_target(&h, profile, &original);
        match shape {
            "two-rows" => h.respond("&orderId=partial", 200, &page(vec![row.clone(), row], "")),
            "http-503" => h.respond("&orderId=partial", 503, ""),
            "mismatched-trigger" => {
                row["triggerBy"] = "LastPrice".into();
                h.respond("&orderId=partial", 200, &page(vec![row], ""));
            }
            _ => h.respond(
                "&orderId=partial",
                200,
                &serde_json::json!({"retCode":0,"result":{"category":"inverse","list":[row],"nextPageCursor":""}}).to_string(),
            ),
        }
        h.advance(200.0);
        let writes = stop_writes(&h);
        assert_eq!(writes.len(), 1, "{shape}: {:?}", h.result(91));
        assert_eq!(writes[0]["stopLoss"], "95", "{shape}");
        h.respond("/v5/position/trading-stop", 200, OK);
        assert!(h.result(91).is_some_and(Result::is_ok), "{shape}");
        assert!(
            h.command_errors
                .iter()
                .any(|(command, message)| *command == 91
                    && message.contains("Open Orders")
                    && message.contains("[proof: axis=target")),
            "{shape}: the unconfirmed cancel is named: {:?}",
            h.command_errors
        );
    }
}

#[test]
fn a_refused_replacement_after_a_failed_proof_puts_the_old_stop_back() {
    let profile = PROFILES[0];
    let mut h = ready(profile);
    let original = incident_stop(&mut h, "partial");
    edit(&mut h, 91, "partial");
    let mut row = native_partial_target(&h, profile, &original);
    row["triggerBy"] = "LastPrice".into();
    h.respond("&orderId=partial", 200, &page(vec![row], ""));
    h.advance(200.0);
    h.respond(
        "/v5/position/trading-stop",
        200,
        r#"{"retCode":10001,"retMsg":"replacement refused","result":{}}"#,
    );
    let writes = stop_writes(&h);
    assert_eq!(writes.len(), 2, "the old stop must be put back");
    assert_eq!(writes[1]["stopLoss"], "90");
    h.respond("/v5/position/trading-stop", 200, OK);
    assert_eq!(
        failure(&h, 91).protection,
        Some(safe_cex_engine::error::Protection::Restored)
    );
}

#[test]
fn a_lost_proof_read_is_read_again_and_then_places_the_replacement() {
    let profile = PROFILES[0];
    let mut h = ready(profile);
    let original = incident_stop(&mut h, "partial");
    edit(&mut h, 91, "partial");
    let request = h.take("&orderId=partial");
    h.event(Event::HttpFailed {
        request: request.id,
        error: "lost native cancellation proof",
    });
    assert!(h.result(91).is_none(), "{:?}", h.result(91));
    h.advance(200.0);
    let row = native_partial_target(&h, profile, &original);
    h.respond("&orderId=partial", 200, &page(vec![row], CURSOR));
    census(&mut h);
    h.respond("/v5/position/trading-stop", 200, OK);
    assert!(
        h.result(91).is_some_and(Result::is_ok),
        "{:?}",
        h.result(91)
    );
}
