use super::*;

fn refused(body: &str) -> Option<(i64, String)> {
    match answer(body) {
        Ok(Answer::Refused { code, message }) => Some((code, message)),
        _ => None,
    }
}

fn success(body: &str) -> (&RawValue, Option<&RawValue>) {
    match answer(body).unwrap() {
        Answer::Success { result, ext } => (result, ext),
        refused => panic!("expected success, got {refused:?}"),
    }
}

#[test]
fn the_envelope_needs_an_exact_integer_code_and_an_object_result() {
    assert_eq!(
        refused(r#"{"retCode":10001,"retMsg":"params error"}"#),
        Some((10001, "params error".into()))
    );
    assert_eq!(
        refused(r#"{"retCode":"10004","retMsg":"error sign"}"#),
        Some((10004, "error sign".into()))
    );
    for body in [
        r#"[0,"OK",{}]"#,
        r#"{"retCode":0,"retMsg":"OK"}"#,
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
fn a_single_identity_needs_its_id_and_exact_echoes() {
    let (result, _) = success(r#"{"retCode":0,"result":{"orderId":"n1","orderLinkId":"c1"}}"#);
    assert_eq!(identity(result, None, Some("c1")), Ok("n1".into()));
    assert_eq!(identity(result, Some("n1"), None), Ok("n1".into()));
    assert_eq!(identity(result, None, Some("other")), Err(Malformed));
    assert_eq!(identity(result, Some("n2"), None), Err(Malformed));
    let (empty, _) = success(r#"{"retCode":0,"result":{"orderId":"","orderLinkId":"c1"}}"#);
    assert_eq!(identity(empty, None, Some("c1")), Err(Malformed));
    let (bare, _) = success(r#"{"retCode":0,"result":{}}"#);
    assert_eq!(identity(bare, None, None), Err(Malformed));
}

#[test]
fn created_rows_match_by_count_and_client_echo_not_position() {
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
fn margin_reasons_refuse_even_when_their_text_is_blank() {
    let (ok, _) = success(r#"{"retCode":0,"result":{}}"#);
    assert_eq!(margin_refusal(ok), Ok(None));
    let (empty, _) = success(r#"{"retCode":0,"result":{"reasons":[]}}"#);
    assert_eq!(margin_refusal(empty), Ok(None));
    let (blank, _) =
        success(r#"{"retCode":0,"result":{"reasons":[{"reasonCode":"3400000","reasonMsg":""}]}}"#);
    assert_eq!(margin_refusal(blank), Ok(Some("3400000".into())));
    for body in [
        r#"{"retCode":0,"result":{"reasons":[[]]}}"#,
        r#"{"retCode":0,"result":{"reasons":{"reasonMsg":"x"}}}"#,
    ] {
        let (result, _) = success(body);
        assert_eq!(margin_refusal(result), Err(Malformed), "{body}");
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
    ] {
        let (result, _) = success(body);
        assert!(position_mode(result, "BTCUSDT").is_err(), "{body}");
    }
}
