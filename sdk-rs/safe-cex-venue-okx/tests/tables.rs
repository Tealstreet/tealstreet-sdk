use safe_cex_venue_okx::types::*;
use serde::Serialize;
use serde_json::{Map, Value, json};

fn object<K: Serialize, V: Serialize>(entries: impl IntoIterator<Item = (K, V)>) -> Value {
    Value::Object(
        entries
            .into_iter()
            .map(|(key, value)| {
                (
                    serde_json::to_value(key)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(value).unwrap(),
                )
            })
            .collect::<Map<_, _>>(),
    )
}

#[test]
#[ignore = "requires Node and the isolated TypeScript compiler; mandatory in CI"]
fn maintained_typescript_protocol_tables() {
    let oracle = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/tables-oracle.mjs"
        ))
        .output()
        .expect("Node is required for the live TypeScript oracle");
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let expected: Value = serde_json::from_slice(&oracle.stdout).unwrap();
    let actual = json!({
        "RECV_WINDOW": RECV_WINDOW, "BROKER_ID": BROKER_ID,
        "OkxRegion": [OkxRegion::Okx, OkxRegion::Okxeu],
        "OkxInstrumentType": [OkxInstrumentType::Swap, OkxInstrumentType::Futures],
        "BASE_URLS": object(BASE_URLS.map(|(region, urls)| (region, json!({"livenet": urls[0], "testnet": urls[1]})))),
        "BASE_WS_URLS": object(BASE_WS_URLS.map(|(region, urls)| (region, json!({
            "public": {"livenet":urls[0][0], "testnet":urls[0][1]},
            "private": {"livenet":urls[1][0], "testnet":urls[1][1]},
            "business": {"livenet":urls[2][0], "testnet":urls[2][1]}
        })))),
        "ENDPOINTS": object(ENDPOINTS), "GRAFANA_ENDPOINTS_EVERY": GRAFANA_ENDPOINTS_EVERY,
        "GRAFANA_ENDPOINTS_PERIODIC": GRAFANA_ENDPOINTS_PERIODIC, "INTERVAL": object(INTERVAL),
        "ORDER_STATUS": object(ORDER_STATUS), "ORDER_TYPE": object(ORDER_TYPE),
        "REVERSE_ORDER_TYPE": object(REVERSE_ORDER_TYPE), "ORDER_SIDE": object(ORDER_SIDE),
        "POSITION_SIDE": object(POSITION_SIDE)
    });
    assert_eq!(actual, expected);
    assert_accepted_set::<OkxRegion>(&expected["OkxRegion"]);
    assert_accepted_set::<OkxInstrumentType>(&expected["OkxInstrumentType"]);
}

// Compare deserialization, including JSON shapes that default serde unit enums
// accept but a TypeScript string-literal union rejects.
fn assert_accepted_set<T: serde::de::DeserializeOwned>(expected: &Value) {
    let accepted = expected.as_array().unwrap();
    for literal in accepted {
        let name = literal.as_str().unwrap();
        assert!(serde_json::from_value::<T>(literal.clone()).is_ok());
        for rejected in [
            json!({name: null}),
            json!({name: []}),
            json!([name]),
            json!(format!(" {name}")),
            json!(format!("{name}\u{feff}")),
            json!(format!("{name}x")),
            json!(name.to_ascii_lowercase()),
            json!(name.to_ascii_uppercase()),
        ] {
            if !accepted.contains(&rejected) {
                assert!(
                    serde_json::from_value::<T>(rejected.clone()).is_err(),
                    "closed union accepted {rejected}"
                );
            }
        }
    }
    for rejected in [
        Value::Null,
        json!(0),
        json!(true),
        json!({}),
        json!(""),
        json!("constructor"),
        json!("__proto__"),
    ] {
        assert!(
            serde_json::from_value::<T>(rejected.clone()).is_err(),
            "closed union accepted {rejected}"
        );
    }
}

#[test]
fn public_qualified_union_decoders_reject_tagged_objects() {
    use serde::Deserialize;
    let mut decoder = serde_json::Deserializer::from_str(r#"{"okx":null}"#);
    assert!(OkxRegion::deserialize(&mut decoder).is_err());
    let mut decoder = serde_json::Deserializer::from_str(r#"{"SWAP":null}"#);
    assert!(OkxInstrumentType::deserialize(&mut decoder).is_err());
}
