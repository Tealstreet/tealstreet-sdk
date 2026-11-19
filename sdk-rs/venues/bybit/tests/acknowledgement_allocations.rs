#![cfg(not(target_arch = "wasm32"))]

// Compile the real matcher in a separate native test executable so its allocator
// instrumentation does not enter the venue library's credential-scope proof.
include!("../src/trading/acknowledgement.rs");

#[path = "support/acknowledgement_alloc_tests.rs"]
mod allocation_tests;

#[test]
fn included_codec_keeps_contradictory_refusals_unproven() {
    let empty = serde_json::from_str::<&RawValue>("{}").unwrap();
    let accepted = serde_json::from_str::<&RawValue>(r#"{"orderId":"accepted"}"#).unwrap();
    assert_eq!(position_setting(Some(empty), Some(empty)), Ok(()));
    assert_eq!(
        position_setting(Some(accepted), Some(empty)),
        Err(Malformed)
    );
    assert_eq!(
        clean_amend_refusal(Some(accepted), None, "accepted"),
        Ok(())
    );
    assert_eq!(
        clean_amend_refusal(Some(accepted), None, "different"),
        Err(Malformed)
    );
    assert_eq!(clean_rate_limit_refusal(Some(empty), None), Ok(()));
    assert_eq!(
        clean_rate_limit_refusal(Some(accepted), None),
        Err(Malformed)
    );
    assert_eq!(margin_reasons(Some(empty)), None);
    let reasons = serde_json::from_str::<&RawValue>(
        r#"{"reasons":[{"reasonCode":"1","reasonMsg":"insufficient margin"}]}"#,
    )
    .unwrap();
    assert_eq!(
        margin_reasons(Some(reasons)).as_deref(),
        Some("insufficient margin")
    );
}
