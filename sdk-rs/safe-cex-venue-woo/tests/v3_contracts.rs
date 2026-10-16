#[path = "support/field_identity.rs"]
mod identity;
use safe_cex_venue_woo::v3_contracts::*;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
fn roundtrip<T: DeserializeOwned + Serialize>(v: Value) -> Result<Value, serde_json::Error> {
    serde_json::from_value::<T>(v).and_then(serde_json::to_value)
}
#[test]
fn inert_v3_declarations_preserve_presence_and_literals() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/contracts.json")).unwrap();
    let supplement: Value = serde_json::from_str(&identity::capture("contracts")).unwrap();
    for case in fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .chain(supplement["cases"].as_array().unwrap())
    {
        let input = case["input"].clone();
        let result = match case["name"].as_str().unwrap() {
            "WooV3Endpoint" => roundtrip::<WooV3Endpoint>(input.clone()),
            "WooV3HttpMethod" => roundtrip::<WooV3HttpMethod>(input.clone()),
            "WooV3MigrationGate" => roundtrip::<WooV3MigrationGate>(input.clone()),
            "WooV3MigrationRoute" => roundtrip::<WooV3MigrationRoute>(input.clone()),
            "WooV3Decimal" => roundtrip::<WooV3Decimal>(input.clone()),
            "WooV3Response" => roundtrip::<WooV3Response<Vec<String>>>(input.clone()),
            "WooV3PageMeta" => roundtrip::<WooV3PageMeta>(input.clone()),
            "WooV3Rows" => roundtrip::<WooV3Rows<Vec<String>>>(input.clone()),
            "WooV3Instrument" => roundtrip::<WooV3Instrument>(input.clone()),
            "WooV3Balance" => roundtrip::<WooV3Balance>(input.clone()),
            "WooV3Position" => roundtrip::<WooV3Position>(input.clone()),
            "WooV3AccountInfo" => roundtrip::<WooV3AccountInfo>(input.clone()),
            "WooV3PlaceOrderRequest" => roundtrip::<WooV3PlaceOrderRequest>(input.clone()),
            "WooV3PlaceOrderResult" => roundtrip::<WooV3PlaceOrderResult>(input.clone()),
            "WooV3ListenKeyRequest" => roundtrip::<WooV3ListenKeyRequest>(input.clone()),
            "WooV3ListenKeyResult" => roundtrip::<WooV3ListenKeyResult>(input.clone()),
            name => panic!("unmapped V3 type {name}"),
        };
        assert_eq!(
            result.is_ok(),
            case["accepted"].as_bool().unwrap(),
            "{} {}: {result:?}",
            case["name"],
            case["label"]
        );
        if let Ok(output) = result {
            assert_eq!(output, input, "{} {}", case["name"], case["label"]);
        }
    }
}
fn object(pairs: Value) -> Value {
    Value::Object(
        pairs
            .as_array()
            .unwrap()
            .iter()
            .map(|pair| (pair[0].as_str().unwrap().to_string(), pair[1].clone()))
            .collect(),
    )
}
#[test]
fn inert_v3_tables_match_live_typescript_and_every_gate_stays_closed() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/contracts.json")).unwrap();
    let ws = Value::Object(
        WOO_V3_WS_URL
            .iter()
            .map(|(key, v)| ((*key).into(), object(json!(v))))
            .collect(),
    );
    let actual = json!({
     "WOO_V3_REST_BASE_URL":object(json!(WOO_V3_REST_BASE_URL)),
     "WOO_V3_WS_URL":ws,
     "WOO_V3_ENDPOINTS":object(json!(WOO_V3_ENDPOINTS)),
     "WOO_V3_MIGRATION_GATES":object(json!(WOO_V3_MIGRATION_GATES)),
     "WOO_V3_RUNTIME_STATUS":WOO_V3_RUNTIME_STATUS,
     "WOO_V3_WEBSOCKET_MIGRATION":object(json!(WOO_V3_WEBSOCKET_MIGRATION)),
     "WOO_V3_MIGRATION_ROUTES":*WOO_V3_MIGRATION_ROUTES,
    });
    assert_eq!(actual, fixture["constants"]);
    assert_eq!(
        json!({
            "WOO_V3_REST_BASE_URL": WOO_V3_REST_BASE_URL,
            "WOO_V3_WS_URL": WOO_V3_WS_URL,
            "WOO_V3_ENDPOINTS": WOO_V3_ENDPOINTS,
            "WOO_V3_MIGRATION_GATES": WOO_V3_MIGRATION_GATES,
            "WOO_V3_WEBSOCKET_MIGRATION": WOO_V3_WEBSOCKET_MIGRATION,
        }),
        fixture["orderedTables"]
    );
    assert_eq!(WOO_V3_MIGRATION_GATES.len(), 8);
    for (gate, enabled) in WOO_V3_MIGRATION_GATES {
        assert!(!enabled, "migration enabled: {gate:?}");
    }
}
