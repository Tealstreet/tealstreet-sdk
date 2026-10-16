use safe_cex_venue_coinbase::starbase::{
    CoinbaseStarbaseJsonRpcErrorObject, CoinbaseStarbaseJsonRpcErrorResponse,
    CoinbaseStarbaseJsonRpcVersion,
};
use serde_json::{Value, json};
use std::process::Command;

fn source_schema() -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/rpc-error-response-type.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[path = "support/schema.rs"]
mod schema;

#[test]
fn json_rpc_error_response_matches_source_shape_and_serialization() {
    let expected = source_schema();
    assert_eq!(
        schema::wire_schema(
            include_str!("../src/starbase.rs"),
            "CoinbaseStarbaseJsonRpcErrorResponse"
        ),
        expected,
        "source-versus-Rust protocol record schema"
    );

    let absent_id = CoinbaseStarbaseJsonRpcErrorResponse {
        jsonrpc: CoinbaseStarbaseJsonRpcVersion::V2,
        id: None,
        error: CoinbaseStarbaseJsonRpcErrorObject {
            code: 401.0,
            message: "not authorized".into(),
            data: Some(Value::Null),
        },
    };
    assert_eq!(absent_id.jsonrpc, CoinbaseStarbaseJsonRpcVersion::V2);
    assert_eq!(absent_id.id, None, "native optional ID field");
    assert_eq!(absent_id.error.code, 401.0, "native error code field");
    assert_eq!(
        absent_id.error.message, "not authorized",
        "native error message field"
    );
    assert_eq!(
        absent_id.error.data,
        Some(Value::Null),
        "native explicit-null data field"
    );
    assert_eq!(
        serde_json::to_value(&absent_id).unwrap(),
        json!({"jsonrpc":"2.0","error":{"code":401.0,"message":"not authorized","data":null}}),
        "absent ID is omitted while explicit-null error data is retained"
    );

    let present_id = CoinbaseStarbaseJsonRpcErrorResponse {
        jsonrpc: CoinbaseStarbaseJsonRpcVersion::V2,
        id: Some(17.0),
        error: CoinbaseStarbaseJsonRpcErrorObject {
            code: 7.0,
            message: "denied".into(),
            data: None,
        },
    };
    assert_eq!(present_id.id, Some(17.0), "native present ID field");
    assert_eq!(
        serde_json::to_value(&present_id).unwrap(),
        json!({"jsonrpc":"2.0","id":17.0,"error":{"code":7.0,"message":"denied"}}),
        "present ID and absent nested data serialization"
    );
}
