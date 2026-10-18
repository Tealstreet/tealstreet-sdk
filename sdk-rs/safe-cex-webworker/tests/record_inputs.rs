use safe_cex_webworker::{error::SerializableWorkerError, payload::*, settings::*};
use serde::Deserialize;
use serde_json::json;

macro_rules! reject_sequence {
    ($test:ident, $record:ty, $input:expr) => {
        #[test]
        fn $test() {
            let input = $input;
            assert!(
                serde_json::from_value::<$record>(input.clone()).is_err(),
                "record accepted positional array: {input}"
            );
            assert!(
                <$record>::deserialize(input).is_err(),
                "qualified decoder bypassed map-only guard"
            );
        }
    };
}
reject_sequence!(
    worker_payload,
    StoreWorkerPayload,
    json!(["store", null, null])
);
reject_sequence!(
    app_settings,
    AppSettingsWorkerType,
    json!([{"tpTriggerType":"mark_price","slTriggerType":"last_price"}])
);
reject_sequence!(
    place_order_settings,
    PlaceOrderSettings,
    json!(["mark_price", "last_price"])
);
reject_sequence!(
    exchange_metadata,
    ExchangeMetadataWorkerType,
    json!([[], [], null, null, null])
);
reject_sequence!(
    serialized_error,
    SerializableWorkerError,
    json!(["message", "Error", null])
);

#[test]
fn valid_records_keep_their_object_shape_and_nested_arrays_are_rejected() {
    let settings =
        json!({"placeOrder":{"tpTriggerType":"mark_price","slTriggerType":"last_price"}});
    let parsed: AppSettingsWorkerType = serde_json::from_value(settings.clone()).unwrap();
    assert_eq!(
        parsed.place_order.tp_trigger_type,
        safe_cex_types::OrderStopTriggerType::MarkPrice,
        "RECORD-IDENTITY native TP trigger"
    );
    assert_eq!(
        parsed.place_order.sl_trigger_type,
        safe_cex_types::OrderStopTriggerType::LastPrice,
        "RECORD-IDENTITY native SL trigger"
    );
    assert_eq!(serde_json::to_value(parsed).unwrap(), settings);
    assert!(
        serde_json::from_value::<AppSettingsWorkerType>(
            json!({"placeOrder":["mark_price","last_price"]})
        )
        .is_err()
    );
    let error = json!({"message":"m","name":"Error","stack":"trace"});
    let parsed: SerializableWorkerError = serde_json::from_value(error.clone()).unwrap();
    assert_eq!(parsed.message, "m", "RECORD-IDENTITY native message");
    assert_eq!(parsed.name, "Error", "RECORD-IDENTITY native name");
    assert_eq!(
        parsed.stack.as_deref(),
        Some("trace"),
        "RECORD-IDENTITY native stack"
    );
    assert_eq!(serde_json::to_value(parsed).unwrap(), error);
}

#[test]
fn shared_records_and_nested_unions_reject_previously_accepted_wide_shapes() {
    // P4 strict decoders (3bbc52657f) repair the previously pinned accepted-set findings.
    assert!(StoreWorkerStorePayload::deserialize(json!([])).is_err());
    assert!(
        serde_json::from_value::<AppSettingsWorkerType>(
            json!({"placeOrder":{"tpTriggerType":{"mark_price":null},"slTriggerType":"last_price"}})
        )
        .is_err()
    );
}

#[test]
fn optional_nonnullable_fields_reject_null_without_erasing_payload_null() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/task_types_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let expected: std::collections::BTreeSet<_> = [
        "StoreWorkerPayload.instanceId",
        "WorkerConfig.tickIntervalMultipliers",
        "WorkerConfig.perfFixture",
        "WorkerConfig.wsDiagnosticsEnabled",
        "PerfFixtureConfig.fixture",
        "PerfFixtureConfig.intervalMs",
        "PerfFixtureConfig.loop",
        "ExchangeMetadataWorkerType.defaultTPTriggerType",
        "ExchangeMetadataWorkerType.defaultSLTriggerType",
        "ExchangeMetadataWorkerType.fetchLeverageOnMarginSymbolChange",
        "SerializableWorkerError.stack",
    ]
    .into_iter()
    .collect();
    assert_eq!(
        source["wireNonnullable"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<std::collections::BTreeSet<_>>(),
        expected,
        "source optional nonnullable inventory changed"
    );

    let mut accepted = Vec::new();
    if StoreWorkerPayload::deserialize(json!({"op":"store","instanceId":null})).is_ok() {
        accepted.push("StoreWorkerPayload.instanceId");
    }
    for key in [
        "defaultTPTriggerType",
        "defaultSLTriggerType",
        "fetchLeverageOnMarginSymbolChange",
    ] {
        if ExchangeMetadataWorkerType::deserialize(
            json!({"supportedTPSLTriggerTypes":[],"privateProxyIps":[],key:null}),
        )
        .is_ok()
        {
            accepted.push(key);
        }
    }
    if SerializableWorkerError::deserialize(json!({"message":"m","name":"Error","stack":null}))
        .is_ok()
    {
        accepted.push("SerializableWorkerError.stack");
    }
    assert!(
        accepted.is_empty(),
        "accepted nonnullable null fields: {accepted:?}"
    );
    let payload = StoreWorkerPayload::deserialize(json!({"op":"store","data":null})).unwrap();
    assert_eq!(payload.data, Some(serde_json::Value::Null));
    assert!(SerializableWorkerError::deserialize(json!({"message":"m","name":"Error"})).is_ok());
    assert!(
        ExchangeMetadataWorkerType::deserialize(
            json!({"supportedTPSLTriggerTypes":[],"privateProxyIps":[]})
        )
        .is_ok()
    );
}

#[test]
fn nullability_inventory_detects_parenthesized_and_aliased_source_mutations() {
    for flag in ["--nullable-parentheses", "--nullable-alias"] {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/task_types_oracle.mjs"
            ))
            .arg(flag)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let source: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let fields = source["wireNonnullable"].as_array().unwrap();
        assert_eq!(
            fields.len(),
            10,
            "source mutation must remove exactly one field: {flag}"
        );
        assert!(
            !fields.contains(&json!("SerializableWorkerError.stack")),
            "nullable source mutation was invisible: {flag}"
        );
    }
}

#[test]
fn nullability_inventory_rejects_shadowed_builtin_source_mutation() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/task_types_oracle.mjs"
        ))
        .arg("--shadowed-record")
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "shadowed builtin must not be classified as a nonnullable record"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("shadowed nullability builtin Record")
    );
}
