use super::*;

fn first_proof(kind: &str) -> (Harness, serde_json::Value) {
    let profile = PROFILES[0];
    let mut h = ready(profile);
    seed(&mut h, profile, "Sell", kind, "MarkPrice", false, true);
    let original = h.engine.store().order("partial").unwrap().clone();
    let row = native_partial_target(&h, profile, &original);
    amend(
        &mut h,
        Some(if kind == "StopLoss" { 95.0 } else { 115.0 }),
        None,
    );
    let cancel = h.take(profile.cancel_path());
    assert_eq!(
        body(&cancel),
        r#"{"category":"linear","request":[{"symbol":"BTCUSDT","orderId":"partial"}]}"#
    );
    h.reply(cancel.id, 200, r#"{"retCode":0,"result":{"list":[{"orderId":"partial"}]},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#);
    assert!(h.has_request("/v5/order/realtime?category=linear&symbol=BTCUSDT&orderId=partial"));
    assert!(!h.has_request("/v5/order/history"));
    (h, row)
}

fn answer(h: &mut Harness, rows: Vec<serde_json::Value>, cursor: &str) {
    h.respond(
        "&orderId=partial",
        200,
        &serde_json::json!({
            "retCode":0,"result":{"category":"linear","list":rows,"nextPageCursor":cursor}
        })
        .to_string(),
    );
}

fn advanced(h: &Harness) -> bool {
    h.has_request("&openOnly=0&limit=50") && !h.unknown.contains(&91)
}

fn assert_restored_after_census(h: &mut Harness) {
    h.respond(
        "/v5/order/history",
        200,
        r#"{"retCode":0,"result":{"category":"linear","list":[],"nextPageCursor":""}}"#,
    );
    h.respond(
        "/v5/position/trading-stop",
        200,
        r#"{"retCode":10001,"retMsg":"replacement refused","result":{}}"#,
    );
    assert!(
        h.has_request("/v5/position/trading-stop"),
        "standalone target must remain restorable"
    );
    h.respond("/v5/position/trading-stop", 200, OK);
    let protection = h.notices.iter().find_map(|notice| match notice {
        safe_cex_engine::Notice::CommandFailed { command: 91, error } => error.protection,
        _ => None,
    });
    assert_eq!(
        protection,
        Some(safe_cex_engine::error::Protection::Restored)
    );
}

fn complete_census_and_restore(h: &mut Harness) {
    h.respond(
        "&openOnly=0&limit=50",
        200,
        r#"{"retCode":0,"result":{"category":"linear","list":[],"nextPageCursor":""}}"#,
    );
    assert_restored_after_census(h);
}

#[test]
fn first_deactivated_stop_loss_should_prove_zero_fill_cancellation() {
    let (mut h, mut row) = first_proof("StopLoss");
    row["orderStatus"] = "Deactivated".into();
    answer(&mut h, vec![row], "");
    assert!(
        advanced(&h),
        "first Deactivated proof refused: {:?}",
        h.result(91)
    );
    complete_census_and_restore(&mut h);
}

#[test]
fn first_deactivated_take_profit_should_prove_zero_fill_cancellation() {
    let (mut h, mut row) = first_proof("TakeProfit");
    row["orderStatus"] = "Deactivated".into();
    answer(&mut h, vec![row], "");
    assert!(
        advanced(&h),
        "first Deactivated proof refused: {:?}",
        h.result(91)
    );
    complete_census_and_restore(&mut h);
}

#[test]
fn deactivated_repeated_in_census_should_not_invalidate_terminal_target() {
    let (mut h, mut row) = first_proof("StopLoss");
    answer(&mut h, vec![row.clone()], "");
    assert!(advanced(&h));
    row["orderStatus"] = "Deactivated".into();
    h.respond(
        "&openOnly=0&limit=50",
        200,
        &serde_json::json!({
            "retCode":0,"result":{"category":"linear","list":[row],"nextPageCursor":""}
        })
        .to_string(),
    );
    assert!(
        !h.unknown.contains(&91),
        "same terminal target refused in census"
    );
    assert!(h.has_request("/v5/order/history"));
    assert_restored_after_census(&mut h);
}

#[test]
fn documented_partial_stop_loss_alias_should_match_original_kind() {
    let (mut h, mut row) = first_proof("StopLoss");
    row["stopOrderType"] = "PartialStopLoss".into();
    answer(&mut h, vec![row], "");
    assert!(
        advanced(&h),
        "documented same-kind alias refused: {:?}",
        h.result(91)
    );
    complete_census_and_restore(&mut h);
}

#[test]
fn documented_partial_take_profit_alias_should_match_original_kind() {
    let (mut h, mut row) = first_proof("TakeProfit");
    row["stopOrderType"] = "PartialTakeProfit".into();
    answer(&mut h, vec![row], "");
    assert!(
        advanced(&h),
        "documented same-kind alias refused: {:?}",
        h.result(91)
    );
    complete_census_and_restore(&mut h);
}

#[test]
fn numeric_formatting_of_every_compared_number_is_already_equivalent() {
    for (field, raw) in [
        ("qty", "7.00000000"),
        ("cumExecQty", "0.00000000"),
        ("triggerPrice", "90.00000000"),
        ("price", "0.00000000"),
    ] {
        let (mut h, mut row) = first_proof("StopLoss");
        row[field] = raw.into();
        answer(&mut h, vec![row], "");
        assert!(
            advanced(&h),
            "equivalent numeric {field} refused: {:?}",
            h.result(91)
        );
    }
}

#[test]
fn first_read_field_matrix_preserves_real_identity_and_economic_mismatches() {
    for (field, replacement) in [
        ("orderId", serde_json::json!("other")),
        ("symbol", serde_json::json!("ETHUSDT")),
        ("side", serde_json::json!("Buy")),
        ("qty", serde_json::json!("8")),
        ("cumExecQty", serde_json::json!("1")),
        ("triggerPrice", serde_json::json!("91")),
        ("triggerBy", serde_json::json!("LastPrice")),
        ("orderType", serde_json::json!("Limit")),
        ("price", serde_json::json!("1")),
        ("positionIdx", serde_json::json!(1)),
        ("tpslMode", serde_json::json!("Full")),
        ("stopOrderType", serde_json::json!("TakeProfit")),
        ("stopOrderType", serde_json::json!("PartialTakeProfit")),
        ("stopOrderType", serde_json::json!("UNKNOWN")),
    ] {
        let (mut h, mut row) = first_proof("StopLoss");
        row[field] = replacement;
        answer(&mut h, vec![row], "");
        assert!(
            h.unknown.contains(&91),
            "changed {field} unexpectedly accepted"
        );
        assert!(!advanced(&h));
        assert!(!h.has_request("/v5/position/trading-stop"));
        assert!(!h.has_request("/v5/order/history"));
    }
}

#[test]
fn first_read_missing_or_empty_fields_are_not_assumed_safe() {
    for field in [
        "qty",
        "cumExecQty",
        "triggerPrice",
        "triggerBy",
        "orderType",
        "price",
        "tpslMode",
        "stopOrderType",
        "positionIdx",
        "side",
    ] {
        for missing in [false, true] {
            let (mut h, mut row) = first_proof("StopLoss");
            if missing {
                row.as_object_mut().unwrap().remove(field);
            } else {
                row[field] = "".into();
            }
            answer(&mut h, vec![row], "");
            assert!(
                h.unknown.contains(&91),
                "{field}/missing={missing} unexpectedly accepted"
            );
            assert!(!h.has_request("/v5/position/trading-stop"));
        }
    }
}

#[test]
fn first_read_two_rows_fail_closed_without_a_second_read() {
    let (mut h, row) = first_proof("StopLoss");
    answer(&mut h, vec![row.clone(), row], "");
    assert!(h.unknown.contains(&91));
    assert!(!h.has_request("&orderId=partial"));
    assert!(!h.has_request("/v5/order/history"));
}

#[test]
fn first_read_cursor_with_single_row_fails_closed() {
    let (mut h, row) = first_proof("StopLoss");
    answer(&mut h, vec![row], "another-page");
    assert!(h.unknown.contains(&91));
    assert!(!h.has_request("&orderId=partial"));
}

#[test]
fn first_read_zero_rows_without_cursor_retries_instead_of_failing_immediately() {
    let (mut h, _) = first_proof("StopLoss");
    answer(&mut h, vec![], "");
    assert!(!h.unknown.contains(&91));
    assert!(h.result(91).is_none());
    h.advance(100.0);
    assert!(h.has_request("/v5/order/realtime"));
}

#[test]
fn first_read_zero_rows_with_cursor_fails_closed() {
    let (mut h, _) = first_proof("StopLoss");
    answer(&mut h, vec![], "another-page");
    assert!(h.unknown.contains(&91));
}

#[test]
fn first_read_canonical_cancelled_control_advances_without_unknown() {
    let (mut h, row) = first_proof("StopLoss");
    answer(&mut h, vec![row], "");
    assert!(advanced(&h));
    assert_eq!(
        h.engine.store().order("partial").unwrap().status,
        OrderStatus::Canceled
    );
}

/// The command error and the `[proof-diag]` warning both name the failed check.
fn assert_proof_diag(h: &Harness, reason: &str) {
    assert!(h.unknown.contains(&91));
    let errors: Vec<_> = h
        .command_errors
        .iter()
        .filter(|(command, _)| *command == 91)
        .map(|(_, message)| message.as_str())
        .collect();
    assert_eq!(
        errors,
        vec![format!(
            "The protective cancel could not be confirmed from native orders; check Open Orders before retrying [proof: {reason}]"
        )]
    );
    let warnings: Vec<_> = h
        .notices
        .iter()
        .filter_map(|notice| match notice {
            safe_cex_engine::Notice::Event(event) if event.code == "partial.proof_failed" => {
                Some((event.level, event.message.clone()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        warnings,
        vec![(
            safe_cex_engine::Level::Warn,
            format!("[proof-diag] {reason}")
        )]
    );
}

#[test]
fn proof_diag_names_a_non_terminal_first_read_status() {
    let (mut h, mut row) = first_proof("StopLoss");
    row["orderStatus"] = "Triggered".into();
    answer(&mut h, vec![row], "");
    assert_proof_diag(&h, "axis=target target-status=Triggered");
}

#[test]
fn proof_diag_names_the_first_mismatched_field() {
    let (mut h, mut row) = first_proof("StopLoss");
    row["triggerBy"] = "LastPrice".into();
    answer(&mut h, vec![row], "");
    assert_proof_diag(&h, "axis=target target-field=triggerBy");
}

#[test]
fn failed_proof_preserves_its_read_diagnostic_without_mutation_telemetry_or_resend() {
    for kind in ["StopLoss", "TakeProfit"] {
        let (mut control, row) = first_proof(kind);
        answer(&mut control, vec![row], "");
        assert!(advanced(&control), "valid cancellation proof advances");

        let (mut h, mut row) = first_proof(kind);
        row["triggerBy"] = "LastPrice".into();
        answer(&mut h, vec![row], "");
        assert_proof_diag(&h, "axis=target target-field=triggerBy");
        h.advance(60_000.0);
        assert!(h.unknown.contains(&91));
        assert!(
            !h.requests
                .iter()
                .any(|request| request.method == safe_cex_engine::Method::Post),
            "a failed proof must never release replacement, restore or resend"
        );
        assert!(
            !h.notices.iter().any(|notice| matches!(
                notice,
                safe_cex_engine::Notice::Event(event) if event.code == "order.outcome_unknown"
            )),
            "read-only proof diagnostics must not claim a mutation response"
        );
    }
}

#[test]
fn proof_diag_names_status_and_field_together() {
    let (mut h, mut row) = first_proof("TakeProfit");
    row["orderStatus"] = "Filled".into();
    row["cumExecQty"] = "1".into();
    answer(&mut h, vec![row], "");
    assert_proof_diag(
        &h,
        "axis=target target-status=Filled target-field=cumExecQty",
    );
}

#[test]
fn proof_diag_carries_kind_enums_only() {
    let (mut h, mut row) = first_proof("StopLoss");
    row["tpslMode"] = "Full".into();
    answer(&mut h, vec![row], "");
    assert_proof_diag(
        &h,
        "axis=target target-field=kind tpslMode=Full stopOrderType=StopLoss",
    );
}

#[test]
fn proof_diag_never_echoes_a_non_enum_value() {
    for status in ["Can celled", "x".repeat(33).as_str(), "", "BTCUSDT-0.5"] {
        let (mut h, mut row) = first_proof("StopLoss");
        row["orderStatus"] = status.into();
        answer(&mut h, vec![row], "");
        assert_proof_diag(&h, "axis=target target-status=other");
    }
}

#[test]
fn proof_diag_names_page_shape_failures() {
    let (mut h, row) = first_proof("StopLoss");
    answer(&mut h, vec![row.clone(), row], "");
    assert_proof_diag(&h, "axis=target target-len=2");

    let (mut h, row) = first_proof("StopLoss");
    answer(&mut h, vec![row], "another-page");
    assert_proof_diag(&h, "axis=target target-cursor");

    let (mut h, row) = first_proof("StopLoss");
    h.respond(
        "&orderId=partial",
        200,
        &serde_json::json!({
            "retCode":0,"result":{"category":"inverse","list":[row],"nextPageCursor":""}
        })
        .to_string(),
    );
    assert_proof_diag(&h, "axis=target category");

    let (mut h, _) = first_proof("StopLoss");
    h.respond("&orderId=partial", 503, "");
    assert_proof_diag(&h, "axis=target http=503");
}

#[test]
fn proof_diag_names_a_census_row_for_the_target() {
    let (mut h, row) = first_proof("StopLoss");
    answer(&mut h, vec![row.clone()], "");
    assert!(advanced(&h));
    let mut census = row;
    census["orderStatus"] = "New".into();
    h.respond(
        "&openOnly=0&limit=50",
        200,
        &serde_json::json!({
            "retCode":0,"result":{"category":"linear","list":[census],"nextPageCursor":""}
        })
        .to_string(),
    );
    assert_proof_diag(&h, "axis=live peer-target-status=New");
}

/// A unified account's realtime read omits the cancelled stop: three empty target reads.
fn exhaust_realtime_reads(h: &mut Harness) {
    for read in 0..3 {
        if read > 0 {
            h.advance(100.0);
        }
        answer(h, vec![], "");
        assert!(
            !h.unknown.contains(&91),
            "empty realtime read {read} failed"
        );
    }
}

fn answer_history(h: &mut Harness, rows: Vec<serde_json::Value>) {
    h.respond(
        "/v5/order/history?category=linear&symbol=BTCUSDT&orderId=partial",
        200,
        &serde_json::json!({
            "retCode":0,"result":{"category":"linear","list":rows,"nextPageCursor":""}
        })
        .to_string(),
    );
}

#[test]
fn lagging_history_is_polled_until_it_shows_the_cancelled_stop() {
    let (mut h, mut row) = first_proof("StopLoss");
    exhaust_realtime_reads(&mut h);
    h.advance(250.0);
    answer_history(&mut h, vec![]);
    assert!(
        !h.unknown.contains(&91),
        "history that has not indexed the cancel yet must be read again"
    );
    h.advance(500.0);
    row["orderStatus"] = "Deactivated".into();
    answer_history(&mut h, vec![row]);
    assert!(advanced(&h), "indexed history refused: {:?}", h.result(91));
    complete_census_and_restore(&mut h);
}

#[test]
fn history_that_never_shows_the_stop_still_ends_unknown() {
    let (mut h, _) = first_proof("StopLoss");
    exhaust_realtime_reads(&mut h);
    for delay in [250.0, 500.0, 750.0, 1_000.0, 1_250.0] {
        h.advance(delay);
        answer_history(&mut h, vec![]);
        assert!(!h.unknown.contains(&91));
    }
    h.advance(1_500.0);
    answer_history(&mut h, vec![]);
    assert_proof_diag(&h, "axis=target-history target-len=0");
    assert!(!h.has_request("/v5/order/history?category=linear&symbol=BTCUSDT&orderId=partial"));
}

#[test]
fn history_polls_wait_longer_than_realtime_polls() {
    let (mut h, _) = first_proof("StopLoss");
    exhaust_realtime_reads(&mut h);
    h.advance(100.0);
    assert!(
        !h.has_request("/v5/order/history"),
        "the first history read waits 250 ms, not the realtime 100 ms"
    );
    h.advance(150.0);
    assert!(h.has_request("/v5/order/history?category=linear&symbol=BTCUSDT&orderId=partial"));
}
