use safe_cex_exchange::proxy_token::BroadcastMessage;
use safe_cex_num::numeric_ingress::{js_from_str, js_from_value, js_from_value_ref};
use safe_cex_types::retained_json::RetainedJsonInput;
use serde_json::Value;

#[test]
fn proxy_broadcast_numeric_ingress_preserves_native_bits() {
    const LEXEMES: [&str; 11] = [
        "0",
        "-0",
        "-0.0",
        "1.25",
        "1e3",
        "1e400",
        "-1e400",
        "-1e-4000",
        "9007199254740993",
        "1.0000000000000002",
        "5e-324",
    ];
    let source = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/proxy_numeric_oracle.cjs"
        ))
        .args(LEXEMES)
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "actual initBroadcastChannel source: {}",
        String::from_utf8_lossy(&source.stderr)
    );
    let bits: Vec<String> = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(bits.len(), 11, "independent source expiry spelling floor");
    let mut assertions = 0;
    for (i, lexeme) in LEXEMES.into_iter().enumerate() {
        // The numeric enum arm is nested below an ordinary sequence decoder.
        let raw = format!(
            r#"[{{"type":"proxyTokenRequest"}},{{"type":"proxyTokenUpdate","token":"fixture","expiresAt":{lexeme}}}]"#
        );
        let value: Value = serde_json::from_str(&raw).unwrap();
        let routes = [
            (
                "bare-str",
                serde_json::from_str::<Vec<BroadcastMessage>>(&raw),
            ),
            ("bare-slice", serde_json::from_slice(raw.as_bytes())),
            ("bare-owned", serde_json::from_value(value.clone())),
            ("bare-borrowed", serde::Deserialize::deserialize(&value)),
            ("shared-str", js_from_str(&raw)),
            ("shared-owned", js_from_value(value.clone())),
            ("shared-borrowed", js_from_value_ref(&value)),
        ];
        for (route, decoded) in routes {
            let decoded = decoded.unwrap_or_else(|e| panic!("{lexeme}/{route}: {e}"));
            assert_eq!(decoded.len(), 2, "independent nested enum cardinality");
            assert!(
                matches!(decoded[0], BroadcastMessage::ProxyTokenRequest),
                "unit enum arm survives"
            );
            let BroadcastMessage::ProxyTokenUpdate { token, expires_at } = &decoded[1] else {
                panic!("numeric enum arm")
            };
            assert_eq!(token, "fixture", "native token value");
            assert_eq!(
                expires_at.to_bits(),
                bits[i].parse::<u64>().unwrap(),
                "PROXY-NATIVE-BITS {lexeme}/{route}"
            );
            assertions += 1;
        }
    }
    assert_eq!(
        assertions, 77,
        "11 spellings x 7 named routes; stop 5e-324/shared-borrowed"
    );
    for invalid in [
        "[]",
        "null",
        r#"{"type":"unknown"}"#,
        r#"{"type":"proxyTokenUpdate","token":"fixture"}"#,
    ] {
        assert!(
            serde_json::from_str::<BroadcastMessage>(invalid).is_err(),
            "tagged record shape stays strict: {invalid}"
        );
    }
    let native = serde_json::json!({"type":"proxyTokenUpdate","token":"fixture","expiresAt":-0.0});
    let BroadcastMessage::ProxyTokenUpdate { expires_at, .. } =
        serde_json::from_value(native).unwrap()
    else {
        panic!("native update")
    };
    assert_eq!(
        expires_at.to_bits(),
        (-0.0f64).to_bits(),
        "native negative-zero positive control"
    );
}

#[test]
fn proxy_broadcast_prices_shared_depth_budget() {
    #[derive(serde::Deserialize)]
    #[serde(tag = "type")]
    enum LegacyWire {
        #[serde(rename = "proxyTokenUpdate")]
        Update {
            token: String,
            #[serde(rename = "expiresAt")]
            expires_at: f64,
        },
    }
    fn value(depth: usize) -> Value {
        let mut ignored = Value::Null;
        for _ in 0..depth {
            ignored = Value::Array(vec![ignored]);
        }
        serde_json::json!({"type":"proxyTokenUpdate","token":"fixture","expiresAt":1,"ignored":ignored})
    }
    let shallow = value(8);
    let deep = value(160);
    let LegacyWire::Update { token, expires_at } = serde_json::from_value(deep.clone()).unwrap();
    assert_eq!(token, "fixture");
    assert_eq!(
        expires_at.to_bits(),
        1.0f64.to_bits(),
        "legacy tagged companion accepts ignored owned metadata"
    );
    for (name, input) in [("shallow", shallow), ("deep", deep)] {
        let bare = serde_json::from_value::<BroadcastMessage>(input.clone());
        let borrowed: Result<BroadcastMessage, serde_json::Error> =
            serde::Deserialize::deserialize(&input);
        let shared = js_from_value::<BroadcastMessage>(input.clone());
        let shared_borrowed = js_from_value_ref::<BroadcastMessage>(&input);
        for result in [bare, borrowed, shared, shared_borrowed] {
            assert_eq!(
                result.is_ok(),
                name == "shallow",
                "PROXY-DEPTH-CHOICE {name}: bounded adapter cost, not invalid source object"
            );
        }
    }
}

#[test]
fn proxy_token_typed_strings_replace_surrogates_and_raw_units_remain_observable() {
    let source = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/proxy_numeric_oracle.cjs"
        ))
        .arg("--utf16")
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "actual receiver token units: {}",
        String::from_utf8_lossy(&source.stderr)
    );
    let reference: Value = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(
        reference["rows"].as_array().unwrap().len(),
        2,
        "independent token UTF-16 floor"
    );
    for (i, expected) in [vec![0xd800u64], vec![0x78, 0xdc00]]
        .into_iter()
        .enumerate()
    {
        let raw = reference["rows"][i]["escaped"].as_str().unwrap();
        let retained: RetainedJsonInput = serde_json::from_str(raw).unwrap();
        let units: Vec<u16> = expected.iter().map(|unit| *unit as u16).collect();
        assert_eq!(
            retained.get("token"),
            Some(&RetainedJsonInput::Text(units.clone())),
            "raw token retains the actual source units before typed projection"
        );
        for (route, decoded) in [
            ("bare-str", serde_json::from_str::<BroadcastMessage>(raw)),
            ("bare-slice", serde_json::from_slice(raw.as_bytes())),
            ("shared-str", js_from_str(raw)),
        ] {
            let BroadcastMessage::ProxyTokenUpdate { token, expires_at } =
                decoded.unwrap_or_else(|error| panic!("{route}: {error}"))
            else {
                panic!("typed token update")
            };
            assert_eq!(token, String::from_utf16_lossy(&units), "{route}");
            assert!(token.contains('\u{fffd}'), "{route}: typed replacement");
            assert_eq!(expires_at.to_bits(), 1.0f64.to_bits(), "{route}");
        }
        assert_eq!(
            retained.get("token"),
            Some(&RetainedJsonInput::Text(units)),
            "typed projection does not replace retained raw units"
        );
        assert_eq!(reference["rows"][i]["units"], serde_json::json!(expected));
    }
    let raw = r#"{"type":"proxyTokenUpdate","token":"😀","expiresAt":1}"#;
    let decoded: BroadcastMessage = serde_json::from_str(raw).unwrap();
    let BroadcastMessage::ProxyTokenUpdate { token, expires_at } = decoded else {
        panic!("ordinary update")
    };
    assert_eq!(token, "😀", "native scalar token stays admitted");
    assert_eq!(expires_at.to_bits(), 1.0f64.to_bits());
    assert_eq!(reference["scalar"], token);
}
