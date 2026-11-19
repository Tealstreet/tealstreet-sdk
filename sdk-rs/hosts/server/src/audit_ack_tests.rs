// Rule 5 (Sam, 2026-11-20): display wording only; outcome and no-retry checks stay.
use super::*;

const DIAGNOSTIC: &str = "We couldn’t confirm this change. Check your open orders before trying again.. Bybit create-batch acknowledgement does not prove its orders (targets=1; result.list=array/2; retExtInfo.list=array/2; children=[0:orderId=nonempty,orderLinkId=empty,status=valid,code=0,echo=absent;1:orderId=nonempty,orderLinkId=empty,status=valid,code=0,echo=unmatched]; omitted=0)";

fn legacy() -> String {
    VenueError::new(DIAGNOSTIC)
        .outcome(safe_cex_engine::Outcome::Unknown)
        .partial_ids(&["new-slice".into(), "resting-peer".into()])
        .legacy_text()
}

fn check(detail: Value) {
    let text = detail["message"].as_str().unwrap();
    assert!(
        text.starts_with("The outcome is unknown"),
        "decode before applying the text cap: {detail}"
    );
    assert!(text.chars().count() <= 160);
    let ack = &detail["bybitBatchAcknowledgement"];
    assert_eq!(ack["targets"], 1);
    assert_eq!(ack["result"], json!({"shape":"array","count":2}));
    assert_eq!(ack["statuses"], json!({"shape":"array","count":2}));
    assert_eq!(ack["children"].as_array().unwrap().len(), 2);
    assert_eq!(ack["children"][1]["orderId"], "nonempty");
    assert_eq!(ack["children"][1]["code"], 0);
    assert_eq!(
        ack["evidenceOrderIds"],
        json!(["new-slice", "resting-peer"])
    );
    assert!(!detail.to_string().contains("secret-canary"));
    let line = line(&Who::default(), "algo", &Map::new(), detail);
    assert_eq!(
        serde_json::from_str::<Value>(&line.to_string()).unwrap(),
        line
    );
}

#[test]
fn audit_create_ack_survives_native_legacy_json_and_the_160_character_cap() {
    let native = VenueError::new(legacy()).outcome(safe_cex_engine::Outcome::Unknown);
    check(venue_result::<()>(&Err(native)));
}

#[test]
fn audit_create_ack_survives_hosted_json_without_copying_unknown_fields() {
    check(result(&Err(
        json!({"message":legacy(), "signature":"secret-canary"}),
    )));
}

#[test]
fn audit_create_ack_survives_the_actual_algo_dispatch_wrapper() {
    check(algo_error(&legacy()));
}

#[test]
fn audit_create_ack_unknown_metadata_never_becomes_an_ack_field() {
    let detail = result(&Err(
        json!({"message":"ordinary error", "bybitBatchAcknowledgement":{"apiKey":"secret-canary"}}),
    ));
    assert!(detail.get("bybitBatchAcknowledgement").is_none());
    assert_eq!(detail["message"], "ordinary error");
    assert!(!detail.to_string().contains("secret-canary"));
}

#[test]
fn audit_create_ack_bounds_children_and_evidence_without_exposing_extra_json() {
    let children = (0..8)
        .map(|i| {
            format!("{i}:orderId=nonempty,orderLinkId=empty,status=valid,code=0,echo=unmatched")
        })
        .collect::<Vec<_>>()
        .join(";");
    let diagnostic = format!(
        "Bybit create-batch acknowledgement does not prove its orders (targets=1; result.list=array/20; retExtInfo.list=array/20; children=[{children}]; omitted=12)"
    );
    let ids = (0..20).map(|i| format!("native-{i}")).collect::<Vec<_>>();
    let legacy =
        json!({"message":diagnostic,"partialOrderIds":ids,"signature":"secret-canary"}).to_string();
    let detail = algo_error(&legacy);
    let ack = &detail["bybitBatchAcknowledgement"];
    assert_eq!(ack["children"].as_array().unwrap().len(), 8);
    assert_eq!(ack["evidenceOrderIds"].as_array().unwrap().len(), 8);
    assert_eq!(ack["omitted"], 12);
    assert!(!detail.to_string().contains("secret-canary"));
    assert!(detail.to_string().len() < 8192);
    assert!(detail["message"].as_str().unwrap().chars().count() <= 160);
}

#[test]
fn audit_create_ack_rejects_unknown_grammar_and_never_truncates_a_json_wrapper() {
    for message in [
        DIAGNOSTIC.replace("targets=1", "targets=501"),
        DIAGNOSTIC.replace("echo=absent", "echo=private-canary"),
        DIAGNOSTIC.replace("status=valid,code=0", "status=missing,code=0"),
        DIAGNOSTIC.replace("omitted=0", "omitted=1"),
    ] {
        assert!(
            algo_error(&message)
                .get("bybitBatchAcknowledgement")
                .is_none()
        );
    }
    for message in [
        "{broken".to_owned(),
        json!({"unrecognized":"private-canary"}).to_string(),
        json!({"message":"x".repeat(70_000)}).to_string(),
    ] {
        let detail = algo_error(&message);
        assert_eq!(detail["message"], "Unparseable native error detail");
        assert!(detail.get("bybitBatchAcknowledgement").is_none());
    }
}
