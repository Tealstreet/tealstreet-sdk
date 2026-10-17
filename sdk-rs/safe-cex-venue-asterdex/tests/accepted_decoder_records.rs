//! Direct native witnesses for the remaining workflow decoder boundaries.
use safe_cex_venue_asterdex::{
    builder_approval::{ApproveParams, CheckParams},
    order_execution::ExecutionOrder,
};

#[test]
fn check_params_rejects_complete_positional_array() {
    let raw = r#"["api-key","https://host",{"builderAddress":"builder","feeRateMaker":"0.0001","feeRateTaker":"0.0002"},"builder","0.0002","signer","signer-key",true,"user"]"#;
    assert!(
        serde_json::from_str::<CheckParams>(raw).is_err(),
        "CheckParams must reject a complete valid Serde-order positional array"
    );
}

#[test]
fn approve_params_rejects_complete_positional_array() {
    let raw = r#"["https://host","builder","builder-name","0.0002",true,"user"]"#;
    assert!(
        serde_json::from_str::<ApproveParams>(raw).is_err(),
        "ApproveParams must reject a complete valid Serde-order positional array"
    );
}

#[test]
fn execution_order_direct_decimal_and_negative_zero() {
    for (literal, bits) in [
        ("1", 1.0_f64.to_bits()),
        ("1.25", 1.25_f64.to_bits()),
        ("-0", (-0.0_f64).to_bits()),
    ] {
        let raw = format!(
            r#"{{"symbol":"BTCUSDT","side":"buy","type":"limit","amount":{literal},"price":-0,"stpMode":"EXPIRE_BOTH","postOnly":true}}"#
        );
        let order = serde_json::from_str::<ExecutionOrder>(&raw)
            .expect("ExecutionOrder accepts valid direct decimal/negative-zero payloads");
        assert_eq!(order.opts.order.symbol, "BTCUSDT");
        assert_eq!(order.opts.order.amount.to_bits(), bits);
        assert_eq!(
            order.opts.order.price.map(f64::to_bits),
            Some((-0.0_f64).to_bits())
        );
        assert_eq!(order.post_only, Some(true));
        assert_eq!(
            order.opts.stp_mode,
            Some(safe_cex_venue_asterdex::types::AsterDEXStpMode::V2)
        );
    }
}
