use safe_cex_venue_mexc::types::*;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
struct SwappedEnvelopeMessages {
    #[serde(rename = "msg")]
    message: Option<String>,
    #[serde(rename = "message")]
    msg: Option<String>,
}

fn tables() -> Value {
    json!({
        "RECV_WINDOW":RECV_WINDOW,"BASE_URL":*BASE_URL,"BASE_WS_URL":*BASE_WS_URL,
        "MEXC_WS_HEARTBEAT_INTERVAL_MS":MEXC_WS_HEARTBEAT_INTERVAL_MS,"ENDPOINTS":*ENDPOINTS,
        "PUBLIC_ENDPOINTS":*PUBLIC_ENDPOINTS,"GRAFANA_ENDPOINTS_EVERY":*GRAFANA_ENDPOINTS_EVERY,
        "GRAFANA_ENDPOINTS_PERIODIC":*GRAFANA_ENDPOINTS_PERIODIC,"INTERVAL":*INTERVAL,
        "ORDER_SIDE":*ORDER_SIDE,"REVERSE_ORDER_SIDE":*REVERSE_ORDER_SIDE,"ORDER_TYPE":*ORDER_TYPE,
        "ORDER_TIME_IN_FORCE":*ORDER_TIME_IN_FORCE,"REVERSE_ORDER_TYPE":*REVERSE_ORDER_TYPE,
        "REVERSE_TIME_IN_FORCE":*REVERSE_TIME_IN_FORCE,"ORDER_STATUS":*ORDER_STATUS,
        "POSITION_SIDE":*POSITION_SIDE,"POSITION_MODE":*POSITION_MODE,"REVERSE_POSITION_MODE":*REVERSE_POSITION_MODE,
        "MARGIN_TYPE":*MARGIN_TYPE,"REVERSE_MARGIN_TYPE":*REVERSE_MARGIN_TYPE
    })
}
#[test]
fn mexc_tables_live_typescript_every_value() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/tables_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "live oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        expected["tables"].as_object().unwrap().len(),
        21,
        "table census"
    );
    let actual = tables();
    assert_eq!(actual.as_object().unwrap().len(), 21);
    for (name, value) in expected["tables"].as_object().unwrap() {
        assert_eq!(&actual[name], value, "table {name}");
    }
    let channels = expected["channels"].as_array().unwrap();
    assert_eq!(channels.len(), 10);
    for case in channels {
        assert_eq!(
            is_mexc_control_channel(case["input"].as_str().unwrap()),
            case["output"].as_bool().unwrap(),
            "channel {}",
            case["input"]
        );
    }
    let envelope = expected["envelope"].as_array().unwrap();
    assert_eq!(envelope.len(), 44);
    for case in envelope {
        let decoded = <MexcEnvelope as Deserialize>::deserialize(case["input"].clone());
        assert_eq!(
            decoded.is_ok(),
            case["accepted"].as_bool().unwrap(),
            "MexcEnvelope accepted set: {}",
            case["input"]
        );
        if let Ok(mut decoded) = decoded {
            if std::env::var_os("MEXC_ENVELOPE_RUST_FIELD_SWAP").is_some() {
                let swapped: SwappedEnvelopeMessages =
                    serde_json::from_value(case["input"].clone()).unwrap();
                decoded.message = swapped.message;
                decoded.msg = swapped.msg;
            }
            let input = case["input"].as_object().unwrap();
            if let Some(value) = input.get("success") {
                assert_eq!(
                    decoded.success,
                    value.as_bool(),
                    "MexcEnvelope.success decoded field identity"
                );
            }
            if let Some(value) = input.get("code") {
                match (decoded.code.as_ref().unwrap(), value) {
                    (MexcCode::Number(actual), expected) => assert_eq!(
                        actual.to_bits(),
                        expected.as_f64().unwrap().to_bits(),
                        "MexcEnvelope.code decoded field identity"
                    ),
                    (MexcCode::String(actual), expected) => assert_eq!(
                        Some(actual.as_str()),
                        expected.as_str(),
                        "MexcEnvelope.code decoded field identity"
                    ),
                }
            }
            if let Some(value) = input.get("message") {
                assert_eq!(
                    decoded.message.as_deref(),
                    value.as_str(),
                    "MexcEnvelope.message decoded field identity"
                );
            }
            if let Some(value) = input.get("msg") {
                assert_eq!(
                    decoded.msg.as_deref(),
                    value.as_str(),
                    "MexcEnvelope.msg decoded field identity"
                );
            }
            if let Some(value) = input.get("data") {
                assert_eq!(
                    decoded.data.as_ref(),
                    Some(value),
                    "MexcEnvelope.data decoded field identity"
                );
            }
            let actual = serde_json::to_value(decoded).unwrap();
            for (key, value) in case["input"].as_object().unwrap() {
                if value.is_number() {
                    assert_eq!(actual[key].as_f64(), value.as_f64(), "MexcEnvelope.{key}");
                } else {
                    assert_eq!(&actual[key], value, "MexcEnvelope.{key}");
                }
            }
            assert_eq!(
                actual.as_object().unwrap().len(),
                case["input"].as_object().unwrap().len()
            );
        }
    }
    println!("21 complete protocol tables, 10 channels, 44 TypeScript-checked envelope shapes");
}

#[test]
fn mexc_envelope_field_swap_control_fails_by_name() {
    let name = "mexc_tables_live_typescript_every_value";
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("MEXC_ENVELOPE_RUST_FIELD_SWAP", "1")
        .output()
        .unwrap();
    let status = output.status.code();
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(status, Some(101), "{output}");
    assert!(output.contains(&format!("{name} ... FAILED")), "{output}");
    assert!(
        output.contains("MexcEnvelope.message decoded field identity"),
        "{output}"
    );
    assert!(
        output.contains("assertion `left == right` failed"),
        "{output}"
    );
}
#[test]
fn mexc_tables_typescript_negative_control_is_observed() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "mexc_tables_live_typescript_every_value",
            "--nocapture",
        ])
        .env("MEXC_TS_CONTROL", "recv-window")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.contains("mexc_tables_live_typescript_every_value ... FAILED"),
        "{output}"
    );
    assert!(output.contains("table RECV_WINDOW"), "{output}");
}

#[test]
fn mexc_tables_isolated_source_has_no_root_dependency() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/isolated-tables.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "isolated oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("missing import/compiler controls fail loudly")
    );
}

#[test]
fn mexc_inherited_table_lookup_acceptance_witness() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/inherited_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        result["census"]
            .as_object()
            .unwrap()
            .values()
            .map(|v| v.as_array().unwrap().len())
            .sum::<usize>(),
        27
    );
    let witnesses = result["witnesses"].as_array().unwrap();
    assert_eq!(witnesses.len(), 52);
    for witness in witnesses {
        let expression = witness["expression"].as_str().unwrap();
        let table_name = expression.split('[').next().unwrap();
        let all = tables();
        let entries: Vec<_> = all[table_name]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let hit = safe_cex_num::js_table_lookup(&entries, witness["key"].as_str().unwrap());
        assert_eq!(hit.is_truthy(), witness["truthy"].as_bool().unwrap());
        assert_eq!(
            hit.is_nullish(),
            witness["nullishFallback"].as_bool().unwrap()
        );
        let kind = match hit {
            safe_cex_num::TableHit::Inherited(safe_cex_num::ProtoMember::Prototype) => "prototype",
            safe_cex_num::TableHit::Inherited(_) => "function",
            safe_cex_num::TableHit::Missing => "missing",
            safe_cex_num::TableHit::Mapped(_) => panic!("control key unexpectedly mapped"),
        };
        assert_eq!(kind, witness["kind"]);
        let inherited = witness["key"] != "unknown";
        assert_eq!(witness["truthy"], inherited, "{witness}");
        assert_eq!(witness["nullishFallback"], !inherited, "{witness}");
    }
    // Shared TableHit now preserves these reads; affected consumers remain pending. This pins
    // TypeScript's acceptance and pre-serialization type, not Rust completion.
}
