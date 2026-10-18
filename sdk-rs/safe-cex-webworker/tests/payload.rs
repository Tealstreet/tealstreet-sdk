use safe_cex_webworker::payload::*;
use serde_json::{Value, json};
#[test]
fn payload_presence_and_shared_store_delta_roundtrip() {
    for wire in [
        json!({"op":"store"}),
        json!({"op":"store","data":null}),
        json!({"op":"exec","data":{"reqId":"","command":"x"}}),
    ] {
        let payload: StoreWorkerPayload = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(
            payload.op,
            if wire["op"] == "exec" {
                StoreWorkerOp::Exec
            } else {
                StoreWorkerOp::Store
            },
            "PAYLOAD-IDENTITY native envelope op"
        );
        assert_eq!(
            payload.data.as_ref(),
            wire.get("data"),
            "PAYLOAD-IDENTITY native envelope presence"
        );
        assert_eq!(serde_json::to_value(payload).unwrap(), wire);
    }
    let wire = json!({"tickerDeltas":[[3.0,5.0,1.25,2.5]],"updateIds":{"orders":2.0}});
    let payload: StoreWorkerStorePayload = serde_json::from_value(wire.clone()).unwrap();
    let delta = &payload.ticker_deltas.as_ref().unwrap()[0];
    assert_eq!(delta.index, 3.0, "PAYLOAD-IDENTITY packed index");
    assert_eq!(delta.mask, 5.0, "PAYLOAD-IDENTITY packed mask");
    assert_eq!(delta.values, [1.25, 2.5], "PAYLOAD-IDENTITY packed values");
    assert_eq!(
        payload.update_ids.as_ref().unwrap().orders,
        Some(2.0),
        "PAYLOAD-IDENTITY native order update id"
    );
    assert_eq!(serde_json::to_value(payload).unwrap(), wire);
}
#[test]
fn operation_sets_equal_live_typescript_unions() {
    let raw_source = include_str!("../../../packages/safe-cex/src/webworker/StoreWorkerPayload.ts");
    let source = raw_source
        .lines()
        .map(|line| line.split("//").next().unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    let mut combined = Vec::new();
    for name in ["StoreWorkerIncomingOp", "StoreWorkerOutgoingOp"] {
        let block = source
            .split(&format!("export type {name} ="))
            .nth(1)
            .unwrap()
            .split(';')
            .next()
            .unwrap();
        for line in block.lines() {
            if let Some(value) = line
                .trim()
                .strip_prefix("| '")
                .and_then(|x| x.split("'").next())
            {
                let wire = json!(value);
                let result: Value = if name == "StoreWorkerIncomingOp" {
                    serde_json::to_value(
                        serde_json::from_value::<StoreWorkerIncomingOp>(wire.clone()).unwrap(),
                    )
                    .unwrap()
                } else {
                    serde_json::to_value(
                        serde_json::from_value::<StoreWorkerOutgoingOp>(wire.clone()).unwrap(),
                    )
                    .unwrap()
                };
                assert_eq!(result, wire);
                combined.push(value.to_string());
                assert_eq!(
                    serde_json::to_value(
                        serde_json::from_value::<StoreWorkerOp>(wire.clone()).unwrap()
                    )
                    .unwrap(),
                    wire
                );
            }
        }
    }
    assert_eq!(combined.len(), 65);
    assert!(serde_json::from_value::<StoreWorkerOp>(json!("unknown")).is_err());
}

#[test]
fn public_deserialize_cannot_bypass_string_literal_boundary() {
    use serde::Deserialize;
    assert!(StoreWorkerIncomingOp::deserialize(json!({"init":null})).is_err());
    assert!(StoreWorkerOutgoingOp::deserialize(json!({"store":null})).is_err());
    assert!(StoreWorkerOp::deserialize(json!({"init":null})).is_err());
}
