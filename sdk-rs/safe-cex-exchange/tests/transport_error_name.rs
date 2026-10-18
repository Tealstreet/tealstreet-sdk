use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_transport::{
    RestFailure, RestRequest, RestResponse, RestResponseBody, TransportError,
    signature_rejection::{
        IsSignatureRejection, SignatureRetry, is_binance_style_signature_rejection,
        signature_rejected_response,
    },
};
use serde_json::json;
use std::rc::Rc;

#[test]
fn signature_predicate_type_error_survives_the_exchange_boundary() {
    let request = RestRequest {
        method: "GET".into(),
        url: "/orders".into(),
        timeout_ms: Some(27),
        ..Default::default()
    };
    let mut rejected = RestFailure::pre_transmit("HTTP rejection".into(), request);
    rejected.response = Some(Box::new(RestResponse {
        status: 401,
        reason: None,
        headers: Default::default(),
        body: RestResponseBody::Untransformed(
            safe_cex_types::retained_json::RetainedJsonInput::from_value(
                json!({"code":{"toString":0}}),
            )
            .unwrap(),
        ),
    }));
    let predicate: IsSignatureRejection = Rc::new(is_binance_style_signature_rejection);
    let failure =
        signature_rejected_response(&rejected, &SignatureRetry::default(), &predicate).unwrap_err();
    assert!(
        matches!(failure.transport_error.as_deref(),Some(TransportError::JavaScript(error)) if error.name=="TypeError")
    );
    let error = ExchangeError::from(failure);
    assert_eq!(
        error.name, "TypeError",
        "EXCHANGE-TYPEERROR name survives actual signature predicate rejection"
    );
    assert_eq!(error.message, "Cannot convert object to primitive value");
    assert_eq!(
        error.js_error_string(),
        "TypeError: Cannot convert object to primitive value"
    );
    let retained = error
        .origin
        .as_ref()
        .unwrap()
        .error()
        .downcast_ref::<RestFailure>()
        .unwrap();
    assert_eq!(retained.request.url, "/orders");
    assert_eq!(retained.request.timeout_ms, Some(27));
}

#[test]
fn direct_coercion_projection_keeps_kind_message_and_native_cause() {
    for (name, message) in [
        ("TypeError", "coercion failed"),
        ("RangeError", "range failed"),
    ] {
        let original = safe_cex_num::JsCoercionError { name, message };
        let error = ExchangeError::from(original.clone());
        assert_eq!(error.name, name);
        assert_eq!(error.message, message);
        assert_eq!(
            error
                .origin
                .as_ref()
                .unwrap()
                .error()
                .downcast_ref::<safe_cex_num::JsCoercionError>(),
            Some(&original)
        );
    }
}
#[test]
fn actual_source_signature_failures_keep_type_error_without_resending() {
    let output = std::process::Command::new("node")
        .arg(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/signature-error-name-source.cjs"),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual signature source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
