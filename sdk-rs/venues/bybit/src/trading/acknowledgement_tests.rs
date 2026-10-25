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
    assert_eq!(cancelled_rows(result, ext, &["b", "a"]), Err(Malformed));
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
