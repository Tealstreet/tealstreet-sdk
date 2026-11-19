//! AGT-34: an edit of a partial reduce-only TP/SL (price or size) must never end
//! with neither the old order nor its replacement. Live Bybit answers the orderId
//! proof read with its single row AND a non-empty `nextPageCursor`.
use super::*;

/// The page token Bybit returns even when the orderId filter matched one row.
const CURSOR: &str = "page_args%3Dpartial%26symbol_%3DBTCUSDT%26";

/// The incident shape: a 0.001 Sell stop-market protecting a 0.003 long.
fn incident_stop(h: &mut Harness, id: &str) -> safe_cex_engine::model::Order {
    incident_order(h, id, "StopLoss", "90")
}

fn incident_order(
    h: &mut Harness,
    id: &str,
    kind: &str,
    trigger: &str,
) -> safe_cex_engine::model::Order {
    h.message("/v5/private", r#"{"topic":"position","data":[{"category":"linear","symbol":"BTCUSDT","side":"Buy","size":"0.003","avgPrice":"100","markPrice":"100","leverage":"10","unrealisedPnl":"0","positionIdx":0}]}"#);
    h.message("/v5/private", &format!(r#"{{"topic":"order","data":[{{"category":"linear","symbol":"BTCUSDT","orderId":"{id}","side":"Sell","orderType":"Market","orderStatus":"Untriggered","stopOrderType":"{kind}","tpslMode":"Partial","qty":"0.001","cumExecQty":"0","cumExecValue":"0","avgPrice":"0","price":"0","leavesQty":"0.001","triggerPrice":"{trigger}","triggerBy":"MarkPrice","reduceOnly":true,"closeOnTrigger":true,"positionIdx":0}}]}}"#));
    set_reference_prices(h, 100.0, 100.0);
    h.engine.store().order(id).unwrap().clone()
}

fn edit(h: &mut Harness, command: u32, id: &str) {
    edit_to(h, command, id, Some(95.0), None);
}

fn edit_to(h: &mut Harness, command: u32, id: &str, price: Option<f64>, amount: Option<f64>) {
    h.command(Command::AmendOrder {
        command,
        id: id.into(),
        price,
        amount,
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
        if h.has_request("&orderId=") {
            // The final orderId read before placing on missing evidence: still missing.
            h.respond("&orderId=", 200, &page(vec![], ""));
        } else if h.has_request("&openOnly=0&limit=50") {
            h.respond("&openOnly=0&limit=50", 200, &page(vec![], CURSOR));
        } else if h.has_request("/v5/order/history") {
            h.respond("/v5/order/history", 200, &page(vec![], CURSOR));
        } else {
            h.advance(200.0);
        }
    }
}

/// The trading-stop writes still awaiting an answer.
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
    h.advance(200.0);
    let writes = stop_writes(&h);
    assert_eq!(
        writes.len(),
        1,
        "the old stop must be put back: {:?}",
        h.result(91)
    );
    assert_eq!(writes[0]["stopLoss"], "90");
    assert_eq!(writes[0]["slSize"], "0.001");
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
        census(&mut h);
        let writes = stop_writes(&h);
        assert_eq!(writes.len(), 1, "{shape}: {:?}", h.result(91));
        assert_eq!(writes[0]["stopLoss"], "95", "{shape}");
        h.respond("/v5/position/trading-stop", 200, OK);
        // The command's own result (what a client shows) says a second stop may exist.
        let result = h.result(91).unwrap().as_ref().unwrap_err();
        assert!(
            result.contains("a second stop may exist") && result.contains("Open Orders"),
            "{shape}: {result}"
        );
        assert!(
            h.command_errors
                .iter()
                .any(|(command, message)| *command == 91
                    && message.contains("Open Orders")
                    && message.contains("[proof: axis=")),
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
    census(&mut h);
    h.respond(
        "/v5/position/trading-stop",
        200,
        r#"{"retCode":10001,"retMsg":"replacement refused","result":{}}"#,
    );
    h.advance(200.0);
    let writes = stop_writes(&h);
    assert_eq!(
        writes.len(),
        1,
        "the old stop must be put back: {:?}",
        h.result(91)
    );
    assert_eq!(writes[0]["stopLoss"], "90");
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
    h.advance(300.0);
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

/// The live run hit every partial TP/SL edit: TP as well as SL, size as well as price.
#[test]
fn every_partial_tp_sl_edit_shape_survives_the_cursor() {
    let profile = PROFILES[0];
    for (kind, trigger, price, amount, level, size, expected_level, expected_size) in [
        (
            "StopLoss",
            "90",
            Some(95.0),
            None,
            "stopLoss",
            "slSize",
            "95",
            "0.001",
        ),
        (
            "TakeProfit",
            "110",
            Some(111.0),
            None,
            "takeProfit",
            "tpSize",
            "111",
            "0.001",
        ),
        (
            "StopLoss",
            "90",
            None,
            Some(0.002),
            "stopLoss",
            "slSize",
            "90",
            "0.002",
        ),
        (
            "TakeProfit",
            "110",
            None,
            Some(0.002),
            "takeProfit",
            "tpSize",
            "110",
            "0.002",
        ),
    ] {
        let shape = format!("{kind}/{price:?}/{amount:?}");
        let mut h = ready(profile);
        let original = incident_order(&mut h, "partial", kind, trigger);
        edit_to(&mut h, 91, "partial", price, amount);
        let row = native_partial_target(&h, profile, &original);
        h.respond("&orderId=partial", 200, &page(vec![row], CURSOR));
        census(&mut h);
        let writes = stop_writes(&h);
        assert_eq!(writes.len(), 1, "{shape}: {:?}", h.result(91));
        assert_eq!(writes[0][level], expected_level, "{shape}");
        assert_eq!(writes[0][size], expected_size, "{shape}");
        h.respond("/v5/position/trading-stop", 200, OK);
        assert!(
            h.result(91).is_some_and(Result::is_ok),
            "{shape}: {:?}",
            h.result(91)
        );
    }
}

/// A replacement the guard would refuse at release cannot continue the plan, so the
/// old stop goes back instead of the replacement being vetoed into no stop at all.
#[test]
fn a_replacement_the_price_has_crossed_sends_the_old_stop_back_instead() {
    let profile = PROFILES[0];
    let mut h = ready(profile);
    let original = incident_stop(&mut h, "partial");
    edit(&mut h, 91, "partial");
    let row = native_partial_target(&h, profile, &original);
    h.respond("&orderId=partial", 200, &page(vec![row], CURSOR));
    // Mark falls to 94: a Sell stop at 95 would fire at once, the old one at 90 would not.
    set_reference_prices(&mut h, 94.0, 94.0);
    census(&mut h);
    let writes = stop_writes(&h);
    assert_eq!(writes.len(), 1, "{:?}", h.result(91));
    assert_eq!(
        writes[0]["stopLoss"], "90",
        "the old stop, not the replacement"
    );
    assert_eq!(writes[0]["slSize"], "0.001");
    h.respond("/v5/position/trading-stop", 200, OK);
    let error = failure(&h, 91);
    assert_eq!(
        error.protection,
        Some(safe_cex_engine::error::Protection::Restored),
        "{error:?}"
    );
    assert!(error.message.contains("would trigger"), "{error:?}");
}

fn lose(h: &mut Harness, path: &str) {
    let request = h.take(path);
    h.event(Event::HttpFailed {
        request: request.id,
        error: "lost native cancellation proof",
    });
}

/// F1 / probe (B): the cancel was certain (retCode 0); only the reads were lost. A
/// failed read may only be followed by another read (engine guard
/// `failed_read_cannot_continue_as_mutation`), so the proof is read again with backoff
/// and the edit goes out once one answers, instead of sealing with no write.
#[test]
fn four_lost_proof_reads_after_an_accepted_cancel_still_end_with_one_stop() {
    let profile = PROFILES[0];
    let mut h = ready(profile);
    let original = incident_stop(&mut h, "partial");
    edit(&mut h, 91, "partial");
    for lost in 0..4 {
        if lost > 0 {
            h.advance(1_500.0);
        }
        lose(&mut h, "&orderId=partial");
    }
    assert!(h.result(91).is_none(), "{:?}", h.result(91));
    h.advance(1_500.0);
    set_reference_prices(&mut h, 100.0, 100.0);
    let row = native_partial_target(&h, profile, &original);
    h.respond("&orderId=partial", 200, &page(vec![row], CURSOR));
    census(&mut h);
    let writes = stop_writes(&h);
    assert_eq!(writes.len(), 1, "{:?}", h.result(91));
    assert_eq!(writes[0]["stopLoss"], "95");
    h.respond("/v5/position/trading-stop", 200, OK);
    assert!(
        h.result(91).is_some_and(Result::is_ok),
        "{:?}",
        h.result(91)
    );
}

/// Every proof read lost for the whole backoff: nothing can be sent, and the outcome
/// says plainly that the cancelled stop is gone (never a silent Unknown).
#[test]
fn proof_reads_lost_throughout_end_explicitly_unprotected() {
    let mut h = ready(PROFILES[0]);
    incident_stop(&mut h, "partial");
    edit(&mut h, 91, "partial");
    for _ in 0..30 {
        if !h.has_request("&orderId=partial") {
            break;
        }
        lose(&mut h, "&orderId=partial");
        h.advance(1_500.0);
    }
    assert!(stop_writes(&h).is_empty());
    let error = failure(&h, 91);
    assert_eq!(
        error.protection,
        Some(safe_cex_engine::error::Protection::Unprotected),
        "{error:?}"
    );
    assert!(error.message.contains("re-place"), "{error:?}");
    assert!(error.message.contains("canceled"), "{error:?}");
}

/// Proves the target, then holds the replacement in signing preparation.
fn replacement_held_in_preparation(
    h: &mut Harness,
) -> safe_cex_engine::request_context::Preparation {
    let profile = PROFILES[0];
    let original = incident_stop(h, "partial");
    edit(h, 91, "partial");
    let row = native_partial_target(h, profile, &original);
    h.respond("&orderId=partial", 200, &page(vec![row], ""));
    h.respond("&openOnly=0&limit=50", 200, &page(vec![], ""));
    let history = h.take("/v5/order/history");
    h.engine.handle(
        h.now,
        Event::HttpResponse {
            request: history.id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body: &page(vec![], ""),
            },
        },
    );
    take_partial_preparation(h, "/v5/position/trading-stop")
}

fn release(h: &mut Harness, preparation: safe_cex_engine::request_context::Preparation) {
    h.engine.http_prepared(
        h.now,
        preparation,
        safe_cex_engine::request_context::Prepared {
            id: preparation.attempt().request.0,
            expires_at: None,
        },
    );
    h.collect();
    h.advance(200.0);
}

/// F2 control: the price did not move while the replacement waited.
#[test]
fn a_replacement_released_at_an_unchanged_price_is_sent() {
    let mut h = ready(PROFILES[0]);
    let preparation = replacement_held_in_preparation(&mut h);
    release(&mut h, preparation);
    let writes = stop_writes(&h);
    assert_eq!(writes.len(), 1, "{:?}", h.result(91));
    assert_eq!(writes[0]["stopLoss"], "95");
}

/// F2: mark falls 100 -> 94 while the replacement (95) waits for release. The
/// release guard vetoes it; the old stop (90, still valid) must go back.
#[test]
fn a_replacement_vetoed_at_release_puts_the_old_stop_back() {
    let mut h = ready(PROFILES[0]);
    let preparation = replacement_held_in_preparation(&mut h);
    set_reference_prices(&mut h, 94.0, 94.0);
    release(&mut h, preparation);
    let writes = stop_writes(&h);
    assert_eq!(
        writes.len(),
        1,
        "the old stop must go back: {:?}",
        h.result(91)
    );
    assert_eq!(writes[0]["stopLoss"], "90");
    assert_eq!(writes[0]["slSize"], "0.001");
    h.respond("/v5/position/trading-stop", 200, OK);
    let error = failure(&h, 91);
    assert_eq!(
        error.protection,
        Some(safe_cex_engine::error::Protection::Restored),
        "{error:?}"
    );
}

/// A stale reference fails both guards: nothing is asked of the venue and the
/// outcome says plainly that the position has no stop.
#[test]
fn a_stale_reference_ends_explicitly_unprotected_without_asking_the_venue() {
    let mut h = ready(PROFILES[0]);
    let preparation = replacement_held_in_preparation(&mut h);
    h.advance(10_000.0);
    release(&mut h, preparation);
    assert!(stop_writes(&h).is_empty(), "{:?}", h.result(91));
    let error = failure(&h, 91);
    assert_eq!(
        error.protection,
        Some(safe_cex_engine::error::Protection::Unprotected),
        "{error:?}"
    );
    assert!(error.message.contains("re-place"), "{error:?}");
}

fn assert_not_doubled(h: &mut Harness, said: &str) {
    census(h);
    h.advance(200.0);
    assert!(stop_writes(h).is_empty(), "{said}: a second stop was sent");
    let error = failure(h, 91);
    assert_ne!(
        error.outcome,
        Some(safe_cex_engine::Outcome::Unknown),
        "{error:?}"
    );
    assert!(error.message.contains(said), "{error:?}");
    assert!(
        error.message.contains("no replacement was placed"),
        "{error:?}"
    );
}

/// B2: Bybit shows the old stop still live after the accepted cancel. Placing
/// would leave two reduce-only stops; the edit ends definitely instead.
#[test]
fn a_still_live_old_stop_is_never_doubled() {
    let profile = PROFILES[0];
    for read in ["first", "census", "final"] {
        let mut h = ready(profile);
        let original = incident_stop(&mut h, "partial");
        edit(&mut h, 91, "partial");
        let row = native_partial_target(&h, profile, &original);
        let mut live = row.clone();
        live["orderStatus"] = "Untriggered".into();
        match read {
            // A live row whose fields no longer match is not retried as a lag.
            "first" => {
                live["qty"] = "0.002".into();
                h.respond("&orderId=partial", 200, &page(vec![live], CURSOR));
            }
            "census" => {
                h.respond("&orderId=partial", 200, &page(vec![row], CURSOR));
                live["orderStatus"] = "New".into();
                h.respond("&openOnly=0&limit=50", 200, &page(vec![live], CURSOR));
            }
            _ => {
                h.respond("&orderId=partial", 503, "");
                h.advance(200.0);
                h.respond("&orderId=partial", 200, &page(vec![live], ""));
            }
        }
        assert_not_doubled(&mut h, "still shows");
    }
}

/// B2: Bybit shows the old stop already executed: no full-size replacement goes
/// over a position that has already shrunk.
#[test]
fn an_executed_old_stop_is_never_replaced() {
    let profile = PROFILES[0];
    for status in ["Triggered", "Filled"] {
        let mut h = ready(profile);
        let original = incident_stop(&mut h, "partial");
        edit(&mut h, 91, "partial");
        let mut row = native_partial_target(&h, profile, &original);
        row["orderStatus"] = status.into();
        if status == "Filled" {
            row["cumExecQty"] = "0.001".into();
        }
        h.respond("&orderId=partial", 200, &page(vec![row], CURSOR));
        assert_not_doubled(&mut h, "already triggered or filled");
    }
}

/// B2 control: unreadable evidence gets one final orderId read; a cancelled row
/// there proves the cancel and the replacement goes out without a duplicate warning.
#[test]
fn unreadable_evidence_is_settled_by_a_final_order_read() {
    let profile = PROFILES[0];
    let mut h = ready(profile);
    let original = incident_stop(&mut h, "partial");
    edit(&mut h, 91, "partial");
    let row = native_partial_target(&h, profile, &original);
    h.respond("&orderId=partial", 503, "");
    h.advance(200.0);
    h.respond("&orderId=partial", 200, &page(vec![row], CURSOR));
    census(&mut h);
    let writes = stop_writes(&h);
    assert_eq!(writes.len(), 1, "{:?}", h.result(91));
    assert_eq!(writes[0]["stopLoss"], "95");
    h.respond("/v5/position/trading-stop", 200, OK);
    assert!(
        h.result(91).is_some_and(Result::is_ok),
        "{:?}",
        h.result(91)
    );
    assert!(
        !h.command_errors.iter().any(|(command, _)| *command == 91),
        "{:?}",
        h.command_errors
    );
}

/// A replacement whose signing fails before release sent nothing: the old stop goes back.
#[test]
fn a_replacement_that_fails_signing_puts_the_old_stop_back() {
    let mut h = ready(PROFILES[0]);
    let preparation = replacement_held_in_preparation(&mut h);
    h.engine.http_preparation_failed(
        h.now,
        preparation,
        safe_cex_engine::request_context::Error::Revoked,
        safe_cex_engine::VenueError::new("Signing refused"),
    );
    h.collect();
    h.advance(200.0);
    let writes = stop_writes(&h);
    assert_eq!(
        writes.len(),
        1,
        "the old stop must go back: {:?}",
        h.result(91)
    );
    assert_eq!(writes[0]["stopLoss"], "90");
    h.respond("/v5/position/trading-stop", 200, OK);
    assert_eq!(
        failure(&h, 91).protection,
        Some(safe_cex_engine::error::Protection::Restored)
    );
}

/// The hosted action result keeps only the first 512 chars of a failure message
/// (`order_actions::settle`). After an unconfirmed cancel, a refused replacement with a
/// long venue retMsg, then a successful put-back, the second-stop warning must survive it.
#[test]
fn the_second_stop_warning_survives_a_long_venue_refusal() {
    let profile = PROFILES[0];
    let mut h = ready(profile);
    incident_stop(&mut h, "partial");
    edit(&mut h, 91, "partial");
    h.respond("&orderId=partial", 503, "");
    census(&mut h);
    let long = "x".repeat(1_000);
    h.respond(
        "/v5/position/trading-stop",
        200,
        &serde_json::json!({"retCode":10001,"retMsg":long,"result":{}}).to_string(),
    );
    h.advance(200.0);
    h.respond("/v5/position/trading-stop", 200, OK);
    let message = &failure(&h, 91).message;
    assert!(
        message.contains(&long),
        "the refusal is kept in full: {}",
        message.len()
    );
    let shown: String = message.chars().take(512).collect();
    assert!(
        shown.contains("a second stop may exist") && shown.contains("Open Orders"),
        "{shown}"
    );
    assert!(
        h.result(91)
            .unwrap()
            .as_ref()
            .unwrap_err()
            .chars()
            .take(512)
            .collect::<String>()
            .contains("a second stop may exist")
    );
}
