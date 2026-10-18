use super::*;

fn observations() -> &'static Value {
    static C: OnceLock<Value> = OnceLock::new();
    C.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/surrogate-observability.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "APEX-SURROGATE source producer: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
fn check_observations(name: &str) {
    let expected: [(&str, &[u16], &str); 10] = [
        ("high", &[0xd800], "x=%F0%90%80%80"),
        ("low", &[0xdc00], "x=%F0%90%80%80"),
        ("high-tail", &[0xd800, 0x41], "x=%F0%90%81%81"),
        ("low-prefix", &[0x41, 0xdc00], "x=A%F0%90%80%80"),
        ("pair", &[0xd83d, 0xde00], "x=%F0%9F%98%80"),
        ("scalar", &[0xe9], "x=%C3%A9"),
        ("replacement", &[0xfffd], "x=%EF%BF%BD"),
        (
            "ascii-escape",
            &[0x5c, 0x75, 0x64, 0x38, 0x30, 0x30],
            "x=%5Cud800",
        ),
        ("whitespace", &[0xfeff, 0x85], "x=%EF%BB%BF%C2%85"),
        ("empty", &[], "x="),
    ];
    let rows = observations()["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 10, "APEX-SURROGATE independent source floor");
    let mut source_outputs = Vec::new();
    let mut native_outputs = Vec::new();
    let mut native_count = 0;
    let mut typed_boundary_count = 0;
    for (row, (label, expected_input, query)) in rows.iter().zip(expected) {
        let label = format!("APEX-SURROGATE {name}/{label}");
        assert_eq!(
            row["inputType"], "string",
            "{label} materialized input type"
        );
        assert_eq!(
            units(&row["inputUnits"]),
            expected_input,
            "{label} materialized input units"
        );
        let result = &row["results"][name];
        assert_eq!(result["type"], "string", "{label} source result type");
        let expected_output: Vec<u16> = if name == "generateApeXRandomClientId" {
            "apexomni-"
                .encode_utf16()
                .chain(expected_input.iter().copied())
                .chain("-1234-322222".encode_utf16())
                .collect()
        } else if name == "serializeApexPayloadLegacy" && expected_input.is_empty() {
            Vec::new()
        } else {
            query.encode_utf16().collect()
        };
        source_outputs.push((
            label.clone(),
            units(&result["units"]),
            expected_output.clone(),
        ));
        let decoded = safe_cex_num::numeric_ingress::js_from_str::<
            std::collections::BTreeMap<String, String>,
        >(row["json"].as_str().unwrap());
        if expected_input
            .iter()
            .any(|unit| (0xd800..=0xdfff).contains(unit))
            && expected_input != [0xd83d, 0xde00]
        {
            let decoded = decoded.unwrap();
            assert_eq!(
                decoded["x"],
                String::from_utf16_lossy(expected_input),
                "{label} typed String replacement"
            );
            typed_utf16::assert_projection(row["json"].as_str().unwrap(), &decoded);
            typed_boundary_count += 1;
            continue;
        }
        let decoded = decoded.unwrap();
        let account = decoded["x"].as_str();
        assert_eq!(
            account.encode_utf16().collect::<Vec<_>>(),
            expected_input,
            "{label} decoded native input"
        );
        let actual = match name {
            "serializeApexPayload" => serialize_apex_payload(&JsJsonValue::Object(
                decoded
                    .into_iter()
                    .map(|(key, value)| (key, JsJsonValue::String(value)))
                    .collect(),
            ))
            .unwrap(),
            "serializeApexPayloadLegacy" => serialize_apex_payload_legacy(&JsJsonValue::Object(
                decoded
                    .into_iter()
                    .map(|(key, value)| (key, JsJsonValue::String(value)))
                    .collect(),
            ))
            .unwrap(),
            "generateApeXRandomClientId" => {
                generate_apex_random_client_id(account, || 1234.0, || 0.25)
            }
            _ => panic!("APEX-SURROGATE authored row"),
        };
        native_outputs.push((
            label,
            actual.encode_utf16().collect::<Vec<_>>(),
            expected_output,
        ));
        native_count += 1;
    }
    assert_eq!(
        native_count, 6,
        "APEX-SURROGATE {name} independent native floor"
    );
    assert_eq!(
        typed_boundary_count, 4,
        "APEX-SURROGATE {name} independent typed-boundary floor"
    );
    // Complete every decoded-native input/boundary assertion before query output assertions.
    for (label, actual, expected) in native_outputs {
        assert_eq!(actual, expected, "{label} native result");
    }
    for (label, actual, expected) in source_outputs {
        assert_eq!(actual, expected, "{label} source observable result");
    }
}

#[path = "../../../test-support/typed_utf16.rs"]
mod typed_utf16;
#[test]
fn primary_surrogate_observability() {
    check_observations("serializeApexPayload");
}
#[test]
fn legacy_surrogate_observability() {
    check_observations("serializeApexPayloadLegacy");
}
#[test]
fn client_id_surrogate_observability() {
    check_observations("generateApeXRandomClientId");
}
