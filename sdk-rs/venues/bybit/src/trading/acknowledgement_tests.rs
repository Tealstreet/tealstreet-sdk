use super::*;

fn refused(body: &str) -> Option<(i64, String)> {
    match answer(body) {
        Ok(Answer::Refused { code, message, .. }) => Some((code, message)),
        _ => None,
    }
}

fn success(body: &str) -> (Option<&RawValue>, Option<&RawValue>) {
    match answer(body).unwrap() {
        Answer::Success { result, ext } => (result, ext),
        refused => panic!("expected success, got {refused:?}"),
    }
}

#[test]
fn the_envelope_needs_an_exact_integer_code_and_an_object_or_absent_result() {
    assert_eq!(
        refused(r#"{"retCode":10001,"retMsg":"params error"}"#),
        Some((10001, "params error".into()))
    );
    assert_eq!(
        refused(r#"{"retCode":"10004","retMsg":"error sign"}"#),
        Some((10004, "error sign".into()))
    );
    assert!(success(r#"{"retCode":0,"retMsg":"OK"}"#).0.is_none());
    for body in [
        r#"[0,"OK",{}]"#,
        r#"{"retCode":0,"result":[]}"#,
        r#"{"retCode":0,"result":null}"#,
        r#"{"retCode":0.0,"result":{}}"#,
        r#"{"retCode":10001.5,"retMsg":"x"}"#,
        r#"{"retMsg":"OK","result":{}}"#,
        r#"{"retCode":0,"result":{},"retExtInfo":[]}"#,
        "<html>",
        "",
    ] {
        assert!(matches!(answer(body), Err(Malformed)), "{body}");
    }
}

#[test]
fn a_single_identity_needs_its_bounded_id_and_consistent_echoes() {
    let (result, _) = success(r#"{"retCode":0,"result":{"orderId":"n1","orderLinkId":"c1"}}"#);
    assert_eq!(created(result, Some("c1")), Ok("n1".into()));
    assert_eq!(created(result, Some("other")), Err(Malformed));
    // A single create names one order; an unrequested echo is harmless.
    assert_eq!(created(result, None), Ok("n1".into()));
    let (bare, _) = success(r#"{"retCode":0,"result":{"orderId":"n1"}}"#);
    assert_eq!(created(bare, Some("c1")), Ok("n1".into()));
    assert_eq!(targeted(result, "n1"), Ok(()));
    assert_eq!(targeted(result, "n2"), Err(Malformed));
    let (empty, _) = success(r#"{"retCode":0,"result":{"orderId":"","orderLinkId":"c1"}}"#);
    assert_eq!(created(empty, Some("c1")), Err(Malformed));
    let (none, _) = success(r#"{"retCode":0,"result":{}}"#);
    assert_eq!(created(none, None), Err(Malformed));
    assert_eq!(created(None, None), Err(Malformed));
    let long = format!(
        r#"{{"retCode":0,"result":{{"orderId":"{}"}}}}"#,
        "n".repeat(1025)
    );
    let (long, _) = success(&long);
    assert_eq!(created(long, None), Err(Malformed));
}

#[test]
fn created_rows_match_by_count_echo_and_unique_ids() {
    let body = r#"{"retCode":0,"result":{"list":[
        {"orderId":"n1","orderLinkId":"c1"},{"orderId":"","orderLinkId":""}]},
        "retExtInfo":{"list":[{"code":0,"msg":"OK"},{"code":110007,"msg":"no balance"}]}}"#;
    let (result, ext) = success(body);
    assert_eq!(
        created_rows(result, ext, &[Some("c1"), Some("c2")]),
        Ok(vec![
            Row::Accepted("n1".into()),
            Row::Refused {
                code: 110007,
                message: "no balance".into()
            }
        ])
    );
    // Swapped echoes prove nothing about which request was accepted.
    assert_eq!(
        created_rows(result, ext, &[Some("c2"), Some("c1")]),
        Err(Malformed)
    );
    // Wrong counts, missing statuses and positional rows are malformed.
    assert_eq!(created_rows(result, ext, &[Some("c1")]), Err(Malformed));
    assert_eq!(
        created_rows(result, None, &[Some("c1"), Some("c2")]),
        Err(Malformed)
    );
    let (twice, twice_ext) = success(
        r#"{"retCode":0,"result":{"list":[{"orderId":"n1"},{"orderId":"n1"}]},"retExtInfo":{"list":[{"code":0},{"code":0}]}}"#,
    );
    assert_eq!(
        created_rows(twice, twice_ext, &[None, None]),
        Err(Malformed)
    );
    // A refused row that names an order may have placed it.
    let (named, named_ext) = success(
        r#"{"retCode":0,"result":{"list":[{"orderId":"n1"}]},"retExtInfo":{"list":[{"code":110007,"msg":"no balance"}]}}"#,
    );
    assert_eq!(created_rows(named, named_ext, &[None]), Err(Malformed));
    for body in [
        r#"{"retCode":0,"result":{"list":[["n1","c1"]]},"retExtInfo":{"list":[{"code":0}]}}"#,
        r#"{"retCode":0,"result":{"list":[{"orderId":"n1","orderLinkId":"c1"}]},"retExtInfo":{"list":[[0,"OK"]]}}"#,
        r#"{"retCode":0,"result":{"list":[{"orderId":"n1","orderLinkId":"c1"}]},"retExtInfo":{"list":[{"msg":"OK"}]}}"#,
        r#"{"retCode":0,"result":{"list":null},"retExtInfo":{"list":[{"code":0}]}}"#,
        r#"{"retCode":0,"result":{},"retExtInfo":{"list":[{"code":0}]}}"#,
    ] {
        let (result, ext) = success(body);
        assert_eq!(
            created_rows(result, ext, &[Some("c1")]),
            Err(Malformed),
            "{body}"
        );
    }
}

#[test]
fn cancelled_rows_need_one_status_per_order_and_the_requested_target() {
    let body = r#"{"retCode":0,"result":{"list":[{"orderId":"a"},{"orderId":"b"}]},
        "retExtInfo":{"list":[{"code":0,"msg":"OK"},{"code":110001,"msg":"order not exists"}]}}"#;
    let (result, ext) = success(body);
    assert_eq!(
        cancelled_rows(result, ext, &["a", "b"]),
        Ok(vec![
            Row::Accepted("a".into()),
            Row::Refused {
                code: 110001,
                message: "order not exists".into()
            }
        ])
    );
    // Native ID evidence, rather than original request position, binds each per-row status.
    assert_eq!(
        cancelled_rows(result, ext, &["b", "a"]),
        Ok(vec![
            Row::Refused {
                code: 110001,
                message: "order not exists".into()
            },
            Row::Accepted("a".into()),
        ])
    );
    // A short status list never marks the unmatched order cancelled (#4).
    let (short, short_ext) = success(
        r#"{"retCode":0,"result":{"list":[{"orderId":"a"},{"orderId":"b"}]},"retExtInfo":{"list":[{"code":0}]}}"#,
    );
    assert_eq!(
        cancelled_rows(short, short_ext, &["a", "b"]),
        Err(Malformed)
    );
    let (none, _) = success(r#"{"retCode":0,"result":{"list":[{"orderId":"a"}]}}"#);
    assert_eq!(cancelled_rows(none, None, &["a"]), Err(Malformed));
}

#[test]
fn one_partial_stop_cancel_needs_exactly_one_status_and_no_other_target() {
    for (body, expected) in [
        (
            r#"{"retCode":0,"result":{"orderId":"p"},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#,
            Ok(Row::Accepted("p".into())),
        ),
        (
            r#"{"retCode":0,"retExtInfo":{"list":[{"code":110001,"msg":"gone"}]}}"#,
            Ok(Row::Refused {
                code: 110001,
                message: "gone".into(),
            }),
        ),
        (
            r#"{"retCode":0,"result":{"orderId":"other"},"retExtInfo":{"list":[{"code":0}]}}"#,
            Err(Malformed),
        ),
        (r#"{"retCode":0}"#, Err(Malformed)),
        (r#"{"retCode":0,"retExtInfo":{"list":[]}}"#, Err(Malformed)),
        (
            r#"{"retCode":0,"retExtInfo":{"list":[{}]}}"#,
            Err(Malformed),
        ),
        (
            r#"{"retCode":0,"retExtInfo":{"list":[{"code":0},{"code":0}]}}"#,
            Err(Malformed),
        ),
    ] {
        let (result, ext) = success(body);
        assert_eq!(cancelled_one(result, ext, "p"), expected, "{body}");
    }
}

#[test]
fn cancel_all_needs_object_rows_and_a_successful_flag() {
    let (result, _) = success(
        r#"{"retCode":0,"result":{"list":[{"orderId":"b"},{"orderId":"a"},{"orderId":"a"}],"success":"1"}}"#,
    );
    assert_eq!(cancelled_all(result), Ok(vec!["a".into(), "b".into()]));
    for body in [
        r#"{"retCode":0,"result":{"list":null,"success":"1"}}"#,
        r#"{"retCode":0,"result":{"list":[],"success":"0"}}"#,
        r#"{"retCode":0,"result":{"list":[["a"]]}}"#,
        r#"{"retCode":0,"result":{"list":[{"orderId":""}]}}"#,
        r#"{"retCode":0}"#,
    ] {
        let (result, _) = success(body);
        assert_eq!(cancelled_all(result), Err(Malformed), "{body}");
    }
}

#[test]
fn a_whole_refusal_carrying_rows_of_any_shape_contradicts_itself() {
    for body in [
        r#"{"retCode":10004,"result":[{"orderId":"x"}]}"#,
        r#"{"retCode":10004,"result":{"list":[{"orderId":"x"}]}}"#,
        r#"{"retCode":10004,"retExtInfo":{"list":[{"code":0}]}}"#,
        r#"{"retCode":10004,"retExtInfo":[{"code":0}]}"#,
    ] {
        let Ok(Answer::Refused { result, ext, .. }) = answer(body) else {
            panic!("{body}");
        };
        assert!(carries_rows(result, ext), "{body}");
    }
    let Ok(Answer::Refused { result, ext, .. }) =
        answer(r#"{"retCode":10004,"result":{},"retExtInfo":{}}"#)
    else {
        panic!()
    };
    assert!(!carries_rows(result, ext));
}

#[test]
fn evidence_keeps_only_wholly_well_formed_bounded_rows() {
    let (rows, _) = success(r#"{"retCode":0,"result":{"list":[{"orderId":"a"},{"orderId":"b"}]}}"#);
    assert_eq!(evidence_ids(rows), ["a", "b"]);
    let (single, _) = success(r#"{"retCode":0,"result":{"orderId":"a"}}"#);
    assert_eq!(evidence_ids(single), ["a"]);
    for body in [
        r#"{"retCode":0,"result":{"list":[{"orderId":"a"},123]}}"#,
        r#"{"retCode":0,"result":{"list":[["a"]]}}"#,
        r#"{"retCode":0,"result":{"list":[{"orderId":"a"},{}]}}"#,
    ] {
        let (result, _) = success(body);
        assert!(evidence_ids(result).is_empty(), "{body}");
    }
}

#[test]
fn margin_success_needs_no_reason_rows_of_any_shape() {
    for body in [
        r#"{"retCode":0}"#,
        r#"{"retCode":0,"result":{}}"#,
        r#"{"retCode":0,"result":{"reasons":[]}}"#,
    ] {
        assert_eq!(margin_applied(success(body).0), Ok(true), "{body}");
    }
    for body in [
        r#"{"retCode":0,"result":{"reasons":[{"reasonCode":"3400000","reasonMsg":""}]}}"#,
        r#"{"retCode":0,"result":{"reasons":[{}]}}"#,
    ] {
        assert_eq!(margin_applied(success(body).0), Ok(false), "{body}");
    }
    for body in [
        r#"{"retCode":0,"result":{"reasons":[[]]}}"#,
        r#"{"retCode":0,"result":{"reasons":null}}"#,
        r#"{"retCode":0,"result":{"reasons":{"reasonMsg":"x"}}}"#,
        r#"{"retCode":0,"result":{"reasons":[{"reasonCode":1}]}}"#,
    ] {
        assert_eq!(margin_applied(success(body).0), Err(Malformed), "{body}");
    }
}

#[test]
fn position_modes_need_object_rows_with_integer_indexes() {
    let (one_way, _) = success(
        r#"{"retCode":0,"result":{"list":[{"symbol":"BTCUSDT","positionIdx":0},{"symbol":"ETHUSDT","positionIdx":1}]}}"#,
    );
    assert_eq!(position_mode(one_way, "BTCUSDT"), Ok(false));
    assert_eq!(position_mode(one_way, "ETHUSDT"), Ok(true));
    assert!(position_mode(one_way, "SOLUSDT").is_err());
    let (conflict, _) = success(
        r#"{"retCode":0,"result":{"list":[{"symbol":"BTCUSDT","positionIdx":0},{"symbol":"BTCUSDT","positionIdx":1}]}}"#,
    );
    assert!(position_mode(conflict, "BTCUSDT").is_err());
    for body in [
        r#"{"retCode":0,"result":{"list":[["BTCUSDT",null,null,null,0]]}}"#,
        r#"{"retCode":0,"result":{"list":[{"symbol":"BTCUSDT","positionIdx":"0"}]}}"#,
        r#"{"retCode":0,"result":{"list":[{"symbol":"BTCUSDT","positionIdx":3}]}}"#,
        r#"{"retCode":0,"result":{}}"#,
        r#"{"retCode":0}"#,
    ] {
        let (result, _) = success(body);
        assert!(position_mode(result, "BTCUSDT").is_err(), "{body}");
    }
}

/// Decoder parity pins: what the serde-derived codec guaranteed implicitly.
#[test]
fn field_decoding_keeps_its_strictness() {
    // A duplicated known field is malformed; unknown fields are ignored.
    assert_eq!(
        answer(r#"{"retCode":0,"retCode":1}"#).err(),
        Some(Malformed)
    );
    assert!(answer(r#"{"retCode":0,"extra":[1,{"x":null}]}"#).is_ok());
    // A text field that is not a string (or null) is malformed; null is absent.
    assert_eq!(answer(r#"{"retCode":5,"retMsg":7}"#).err(), Some(Malformed));
    assert_eq!(
        refused(r#"{"retCode":5,"retMsg":null}"#),
        Some((5, String::new()))
    );
    let raw = |body: &'static str| Some(serde_json::from_str::<&RawValue>(body).unwrap());
    assert_eq!(created(raw(r#"{"orderId":7}"#), None), Err(Malformed));
    assert_eq!(created(raw(r#"{"orderId":null}"#), None), Err(Malformed));
    assert_eq!(
        created(raw(r#"{"orderId":"a","orderId":"b"}"#), None),
        Err(Malformed)
    );
    assert_eq!(
        created(raw(r#"{"orderId":"ab"}"#), None),
        Ok("ab".to_owned())
    );
    // Position rows need a string symbol and an exact integer index.
    let mode = |body: &'static str| position_mode(raw(body), "BTCUSDT");
    assert!(mode(r#"{"list":[{"symbol":"BTCUSDT","positionIdx":1.0}]}"#).is_err());
    assert!(mode(r#"{"list":[{"symbol":"BTCUSDT","positionIdx":"1"}]}"#).is_err());
    assert!(mode(r#"{"list":[{"symbol":7,"positionIdx":1}]}"#).is_err());
    assert_eq!(
        mode(r#"{"list":[{"symbol":"BTCUSDT","positionIdx":2}]}"#),
        Ok(true)
    );
    // A margin reason with a non-string code is malformed.
    assert_eq!(
        margin_applied(raw(r#"{"reasons":[{"reasonCode":1}]}"#)),
        Err(Malformed)
    );
}

#[test]
fn cancelled_rows_match_complete_echoes_by_native_id_instead_of_request_position() {
    let (result, ext) = success(
        r#"{"retCode":0,"result":{"list":[{"orderId":"b"},{"orderId":"a"}]},"retExtInfo":{"list":[{"code":0},{"code":0}]}}"#,
    );
    assert_eq!(
        cancelled_rows(result, ext, &["a", "b"]),
        Ok(vec![Row::Accepted("a".into()), Row::Accepted("b".into())])
    );
}

#[test]
fn cancelled_rows_keep_reordered_statuses_attached_to_their_echoed_order() {
    let (result, ext) = success(
        r#"{"retCode":0,"result":{"list":[{"orderId":"b"},{"orderId":"a"}]},"retExtInfo":{"list":[{"code":110020,"msg":"permission denied"},{"code":0}]}}"#,
    );
    assert_eq!(
        cancelled_rows(result, ext, &["a", "b"]),
        Ok(vec![
            Row::Accepted("a".into()),
            Row::Refused {
                code: 110020,
                message: "permission denied".into()
            }
        ])
    );
}

#[test]
fn one_cancel_with_an_empty_echo_still_has_one_unambiguous_success_status() {
    for result in [
        r#"{"list":[{}]}"#,
        r#"{"list":[{"orderId":""}]}"#,
        r#"{"list":[]}"#,
    ] {
        let body =
            format!(r#"{{"retCode":0,"result":{result},"retExtInfo":{{"list":[{{"code":0}}]}}}}"#);
        let (result, ext) = success(&body);
        assert_eq!(
            cancelled_rows(result, ext, &["a"]),
            Ok(vec![Row::Accepted("a".into())])
        );
    }
}

#[test]
fn cancelled_rows_refuse_foreign_duplicate_and_ambiguous_echoes() {
    for results in [
        r#"[{"orderId":"a"},{"orderId":"a"}]"#,
        r#"[{"orderId":"a"},{"orderId":"foreign"}]"#,
        r#"[{},{}]"#,
    ] {
        let body = format!(
            r#"{{"retCode":0,"result":{{"list":{results}}},"retExtInfo":{{"list":[{{"code":0}},{{"code":0}}]}}}}"#
        );
        let (result, ext) = success(&body);
        assert_eq!(cancelled_rows(result, ext, &["a", "b"]), Err(Malformed));
    }
}

#[test]
fn cancel_shape_diagnostic_never_contains_response_values() {
    let body = r#"{"retCode":0,"retMsg":"private-ret-message","result":{"list":[{"orderId":"private-native-needle","orderLinkId":"private-client-needle"}]},"retExtInfo":{"list":[{"code":10005,"msg":"private-status-needle"}]}}"#;
    let (result, ext) = success(body);
    let diagnostic = cancel_diagnostic(result, ext, &["private-native-needle", "other"]);
    assert_eq!(
        diagnostic,
        "targets=2; result.list=array/1; retExtInfo.list=array/1; orderId fields/nonempty/matched=1/1/1; orderLinkId fields/nonempty=1/1; statuses success/failed/invalid=0/1/0; failure codes=10005; duplicated targets=0"
    );
    for value in [
        "private-ret-message",
        "private-native-needle",
        "private-client-needle",
        "private-status-needle",
    ] {
        assert!(!diagnostic.contains(value));
    }
}

#[test]
fn cancelled_rows_match_a_sole_unnamed_reordered_status_by_elimination() {
    for (code, expected) in [
        (0, Row::Accepted("a".into())),
        (
            110001,
            Row::Refused {
                code: 110001,
                message: String::new(),
            },
        ),
    ] {
        let body = format!(
            r#"{{"retCode":0,"result":{{"list":[{{}},{{"orderId":"b"}}]}},"retExtInfo":{{"list":[{{"code":{code}}},{{"code":0}}]}}}}"#
        );
        let (result, ext) = success(&body);
        assert_eq!(
            cancelled_rows(result, ext, &["b", "a"]),
            Ok(vec![Row::Accepted("b".into()), expected])
        );
    }
}

fn over_count(results: &str, statuses: &str, targets: &[&str]) -> Result<Vec<Row>, Malformed> {
    let body = format!(
        r#"{{"retCode":0,"result":{{"list":{results}}},"retExtInfo":{{"list":{statuses}}}}}"#
    );
    let (result, ext) = success(&body);
    cancelled_rows(result, ext, targets)
}

#[test]
fn an_over_count_whose_duplicate_echo_is_order_not_found_confirms_every_target() {
    assert_eq!(
        over_count(
            r#"[{"orderId":"a"},{"orderId":"b"},{"orderId":"c"},{"orderId":"b"},{"orderId":"d"}]"#,
            r#"[{"code":0},{"code":0},{"code":0},{"code":110001,"msg":"order not exists"},{"code":0}]"#,
            &["a", "b", "c", "d"],
        ),
        Ok(Vec::from(
            ["a", "b", "c", "d"].map(|id| Row::Accepted(id.into()))
        ))
    );
}

#[test]
fn an_over_count_keeps_a_target_with_only_a_failed_row_refused_beside_its_twin() {
    assert_eq!(
        over_count(
            r#"[{"orderId":"a"},{"orderId":"b"},{"orderId":"a"}]"#,
            r#"[{"code":0},{"code":110001,"msg":"order not exists"},{"code":0}]"#,
            &["a", "b"],
        ),
        Ok(vec![
            Row::Accepted("a".into()),
            Row::Refused {
                code: 110001,
                message: "order not exists".into()
            }
        ])
    );
}

#[test]
fn an_over_count_stays_malformed_unless_complete_named_and_order_not_found() {
    let targets = &["a", "b"];
    for (results, statuses) in [
        // A foreign fifth ID.
        (
            r#"[{"orderId":"a"},{"orderId":"b"},{"orderId":"foreign"}]"#,
            r#"[{"code":0},{"code":0},{"code":110001}]"#,
        ),
        // An unnamed extra row.
        (
            r#"[{"orderId":"a"},{"orderId":"b"},{}]"#,
            r#"[{"code":0},{"code":0},{"code":110001}]"#,
        ),
        // Unequal list lengths.
        (
            r#"[{"orderId":"a"},{"orderId":"b"},{"orderId":"b"}]"#,
            r#"[{"code":0},{"code":0}]"#,
        ),
        (
            r#"[{"orderId":"a"},{"orderId":"b"}]"#,
            r#"[{"code":0},{"code":0},{"code":110001}]"#,
        ),
        // A target missing from the echoes.
        (
            r#"[{"orderId":"a"},{"orderId":"a"},{"orderId":"a"}]"#,
            r#"[{"code":0},{"code":110001},{"code":110001}]"#,
        ),
        // A twin whose failure is not order-not-found.
        (
            r#"[{"orderId":"a"},{"orderId":"b"},{"orderId":"b"}]"#,
            r#"[{"code":0},{"code":0},{"code":110020,"msg":"permission denied"}]"#,
        ),
        // A twin with no success row.
        (
            r#"[{"orderId":"a"},{"orderId":"b"},{"orderId":"b"}]"#,
            r#"[{"code":0},{"code":110001},{"code":110001}]"#,
        ),
    ] {
        assert_eq!(
            over_count(results, statuses, targets),
            Err(Malformed),
            "{results} {statuses}"
        );
    }
}

#[test]
fn cancel_diagnostic_names_failure_codes_and_duplicated_targets() {
    let body = r#"{"retCode":0,"result":{"list":[{"orderId":"a"},{"orderId":"b"},{"orderId":"b"}]},
        "retExtInfo":{"list":[{"code":0},{"code":110020,"msg":"permission denied"},{"code":110001}]}}"#;
    let (result, ext) = success(body);
    assert_eq!(
        cancel_diagnostic(result, ext, &["a", "b"]),
        "targets=2; result.list=array/3; retExtInfo.list=array/3; orderId fields/nonempty/matched=3/3/3; orderLinkId fields/nonempty=0/0; statuses success/failed/invalid=1/2/0; failure codes=110020,110001; duplicated targets=1"
    );
}

#[test]
fn an_over_count_accepts_a_twin_whose_rows_are_all_successes() {
    // ddi2il ruling: repeated success rows agree; only a twin mixing in a failure needs 110001.
    assert_eq!(
        over_count(
            r#"[{"orderId":"b"},{"orderId":"a"},{"orderId":"b"}]"#,
            r#"[{"code":0},{"code":0},{"code":0}]"#,
            &["a", "b"],
        ),
        Ok(vec![Row::Accepted("a".into()), Row::Accepted("b".into())])
    );
}
