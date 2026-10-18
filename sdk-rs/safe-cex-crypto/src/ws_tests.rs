use crate::*;
use serde_json::{Value, json};

fn fixtures() -> Value {
    let fixture: Value =
        serde_json::from_str(include_str!("../tests/fixtures/node-crypto.json")).unwrap();
    // Lengths are a supplementary drift check; semantic coverage is asserted below.
    assert_eq!(fixture["ws"].as_array().unwrap().len(), 7);
    assert_eq!(fixture["primitives"].as_array().unwrap().len(), 12);
    assert_eq!(fixture["kraken"].as_array().unwrap().len(), 12);
    fixture
}

struct EnumVariants<'a>(&'a mut Option<&'static [&'static str]>);

impl<'de> serde::Deserializer<'de> for EnumVariants<'_> {
    type Error = serde::de::value::Error;

    fn deserialize_any<V: serde::de::Visitor<'de>>(self, _: V) -> Result<V::Value, Self::Error> {
        Err(serde::de::Error::custom("expected enum deserialization"))
    }

    fn deserialize_enum<V: serde::de::Visitor<'de>>(
        self,
        _: &'static str,
        variants: &'static [&'static str],
        _: V,
    ) -> Result<V::Value, Self::Error> {
        *self.0 = Some(variants);
        Err(serde::de::Error::custom("captured enum variants"))
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf
        option unit unit_struct newtype_struct seq tuple tuple_struct map struct
        identifier ignored_any
    }
}

#[test]
fn every_output_format_requires_a_golden_case() {
    // Capture the remote decoder's complete generated literal list. The public
    // frontdoor deliberately admits strings before any enum visitor runs.
    let mut variants = None;
    assert!(WsOutputFormatWire::deserialize(EnumVariants(&mut variants)).is_err());
    let expected: std::collections::BTreeSet<_> = variants
        .expect("enum must expose its derived variant list")
        .iter()
        .copied()
        .collect();
    let fixture = fixtures();
    let formats: Vec<_> = fixture["ws"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| case["format"].as_str().unwrap())
        .collect();
    let covered: std::collections::BTreeSet<_> = formats.iter().copied().collect();
    assert_eq!(
        covered.len(),
        formats.len(),
        "one golden case per format, no duplicates"
    );
    assert_eq!(
        covered, expected,
        "every enum format needs an independent golden case"
    );
}

#[test]
fn local_compositions_match_existing_typescript_goldens() {
    // Read the independent, pre-existing composition oracle, not generate.cjs.
    // This intentionally accepts only its literal syntax and fails on drift.
    let source =
        include_str!("../../../packages/safe-cex/src/__tests__/base.ws-auth-signing.test.ts");
    let secret = source
        .split("const SECRET = '")
        .nth(1)
        .unwrap()
        .split('\'')
        .next()
        .unwrap();
    let fixture = fixtures();
    let mut goldens = std::collections::BTreeSet::new();
    for block in source.split("const result = await sign(").skip(1) {
        let Some((_, expectation)) = block.split_once("expect(result.wsAuthSignature).toBe(")
        else {
            continue; // The override/error cases have no signature golden.
        };
        let args = block.split_once(");").unwrap().0;
        let literals: Vec<_> = args.split('\'').collect();
        assert_eq!(
            literals.len(),
            5,
            "golden must use literal format and payload"
        );
        let format = literals[1];
        let payload = literals[3];
        let signature_literal = expectation.trim_start();
        assert!(
            signature_literal.starts_with('\''),
            "golden must be a literal signature"
        );
        let signature = signature_literal[1..].split('\'').next().unwrap();
        assert!(goldens.insert(format), "duplicate TS composition golden");
        let case = fixture["ws"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["format"] == format)
            .expect("TS golden needs a Rust fixture");
        assert_eq!(case["secret"], secret, "{format} secret");
        assert_eq!(case["payload"], payload, "{format} payload");
        assert_eq!(case["signature"], signature, "{format} composition golden");
    }
    assert!(!goldens.is_empty(), "must parse TS composition goldens");
    let mut variants = None;
    let _ = WsOutputFormatWire::deserialize(EnumVariants(&mut variants));
    let local: std::collections::BTreeSet<_> = variants
        .unwrap()
        .iter()
        .copied()
        .filter(|format| !matches!(*format, "kraken-challenge" | "coinbase-jwt"))
        .collect();
    assert_eq!(
        goldens, local,
        "every non-override format needs a TS composition golden"
    );
}

#[test]
fn corpus_preserves_decoder_edges_and_cryptographic_format_coverage() {
    let fixture = fixtures();
    let secrets: Vec<_> = fixture["kraken"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| {
            assert!(case["signature"].as_str().is_some_and(|s| !s.is_empty()));
            assert!(case["payload"].as_str().is_some_and(|s| !s.is_empty()));
            case["secret"].as_str().unwrap()
        })
        .collect();
    assert!(
        secrets.iter().any(|secret| !secret.is_ascii()),
        "Kraken needs a non-ASCII secret"
    );
    assert!(
        secrets.iter().any(|secret| secret
            .find('=')
            .is_some_and(|i| i > 0 && i + 1 < secret.len())),
        "Kraken needs a mid-string equals"
    );
    assert!(
        secrets.iter().any(|secret| {
            let decoded = secret.split('=').next().unwrap();
            decoded.contains(['+', '/']) && decoded.contains(['-', '_'])
        }),
        "Kraken needs a dual-alphabet secret"
    );
    assert!(
        secrets.iter().any(|secret| {
            let prefix: Vec<_> = secret
                .encode_utf16()
                .map(|unit| unit as u8)
                .take_while(|byte| *byte != b'=')
                .collect();
            let valid = |byte: &u8| byte.is_ascii_alphanumeric() || b"+/-_".contains(byte);
            prefix.iter().enumerate().any(|(index, byte)| {
                !valid(byte)
                    && prefix[index + 1..]
                        .iter()
                        .filter(|byte| valid(byte))
                        .count()
                        >= 2
            })
        }),
        "Kraken needs invalid characters followed by decodable data before padding"
    );
    for case in fixture["ws"].as_array().unwrap() {
        if !matches!(
            case["format"].as_str().unwrap(),
            "kraken-challenge" | "coinbase-jwt"
        ) {
            assert!(
                case["signature"].as_str().is_some_and(|s| !s.is_empty()),
                "non-override format needs a signature-bearing case: {}",
                case["format"]
            );
            assert!(
                case.get("error").is_none(),
                "local signing cannot be discharged by an error"
            );
        }
    }
}

fn request(id: &str, format: WsOutputFormat, payload: &str) -> WsAuthRequest {
    serde_json::from_value(json!({
        "id": id,
        "wsAuth": { "toSign": payload, "outputFormat": format }
    }))
    .unwrap()
}

#[test]
fn all_ws_formats_match_node_signatures_and_override_refusals() {
    for case in fixtures()["ws"].as_array().unwrap() {
        let format = serde_json::from_value(case["format"].clone()).unwrap();
        let request = request("req-1", format, case["payload"].as_str().unwrap());
        let result = sign_ws_auth(&request, case["secret"].as_str().unwrap());
        assert!(
            case.get("signature").is_some() ^ case.get("error").is_some(),
            "one expected outcome per golden case"
        );
        let expected = if let Some(error) = case.get("error") {
            json!({ "id": "req-1", "error": error })
        } else {
            json!({ "id": "req-1", "wsAuthSignature": case["signature"] })
        };
        assert_eq!(serde_json::to_value(result).unwrap(), expected);
    }
}

#[test]
fn primitive_base64_and_hex_vectors() {
    for case in fixtures()["primitives"].as_array().unwrap() {
        let key = case["key"].as_str().unwrap().as_bytes();
        let payload = case["payload"].as_str().unwrap().as_bytes();
        let encoding = serde_json::from_value(case["encoding"].clone()).unwrap();
        let actual = match case["algorithm"].as_str().unwrap() {
            "sha256" => hmac_sha256(key, payload, encoding),
            "sha512" => hmac_sha512(key, payload, encoding),
            other => panic!("unknown fixture algorithm {other}"),
        };
        assert_eq!(actual, case["signature"].as_str().unwrap());
    }
}

#[test]
fn kraken_challenge_and_permissive_node_secret_decoding() {
    for case in fixtures()["kraken"].as_array().unwrap() {
        for format in [
            WsOutputFormat::KrakenChallenge,
            WsOutputFormat::Hex,
            WsOutputFormat::CoinbaseJwt,
        ] {
            let request = request("kraken-1", format, case["payload"].as_str().unwrap());
            let result = sign_kraken_ws_auth(&request, case["secret"].as_str().unwrap());
            assert_eq!(result.id, Some(json!("kraken-1")));
            assert_eq!(result.error, None);
            assert_eq!(
                result.ws_auth_signature.as_deref(),
                case["signature"].as_str()
            );
        }
    }
}

#[test]
fn override_only_formats_fail_closed_in_base_signer() {
    for (format, venue) in [
        (WsOutputFormat::KrakenChallenge, "KrakenExchange"),
        (WsOutputFormat::CoinbaseJwt, "CoinbaseExchange"),
    ] {
        let request = request("req", format, "payload");
        let result = sign_ws_auth(&request, "secret");
        assert_eq!(result.id, Some(json!("req")));
        assert_eq!(result.ws_auth_signature, None);
        assert!(result.error.unwrap().contains(venue));
    }
}

#[test]
fn missing_payload_precedes_missing_secret() {
    for (signer, missing) in [
        (
            sign_ws_auth as fn(&WsAuthRequest, &str) -> WsAuthResponse,
            "Missing wsAuth payload for WS auth signing",
        ),
        (sign_kraken_ws_auth, "Missing challenge to sign"),
    ] {
        for input in [
            json!({"id": "req"}),
            json!({"id": "req", "wsAuth": null}),
            json!({"id": "req", "wsAuth": {"toSign": "", "outputFormat": "hex"}}),
            json!({"id": "req", "wsAuth": {"outputFormat": "hex"}}),
        ] {
            let request: WsAuthRequest = serde_json::from_value(input).unwrap();
            for secret in ["", "secret"] {
                let result = signer(&request, secret);
                assert_eq!(result.id, Some(json!("req")));
                assert_eq!(result.ws_auth_signature, None);
                assert_eq!(result.error.as_deref(), Some(missing));
            }
        }
        let request = request("req", WsOutputFormat::Hex, "payload");
        let result = signer(&request, "");
        assert_eq!(result.ws_auth_signature, None);
        assert_eq!(
            result.error.as_deref(),
            Some("No secret available for WS auth signing")
        );
    }
}

#[test]
fn unknown_format_returns_correlated_error_and_settles_pending_request() {
    let wire = json!({"id": "pending-unknown-42", "type": "ws-auth", "exchangeName": "binance", "wsAuth": {"toSign": "payload", "outputFormat": "future-format"}});
    let request: WsAuthRequest = serde_json::from_value(wire).unwrap();
    let result = sign_ws_auth(&request, "secret");
    let mut pending = BTreeMap::from([(serde_json::to_string(&request.id).unwrap(), "waiting")]);
    assert_eq!(
        pending.remove(&serde_json::to_string(&result.id).unwrap()),
        Some("waiting")
    );
    assert!(pending.is_empty());
    assert_eq!(
        serde_json::to_value(&result).unwrap(),
        json!({"id": "pending-unknown-42", "error": "Unsupported WS auth outputFormat: future-format"})
    );
    let result = sign_ws_auth(&request, "");
    assert_eq!(result.id, Some(json!("pending-unknown-42")));
    assert_eq!(
        result.error.as_deref(),
        Some("No secret available for WS auth signing")
    );
}

#[test]
fn missing_format_returns_correlated_error_while_kraken_ignores_format() {
    for payload in [
        json!({"toSign": "challenge"}),
        json!({"toSign": "challenge", "outputFormat": null}),
        json!({"toSign": "challenge", "outputFormat": ""}),
    ] {
        let request: WsAuthRequest =
            serde_json::from_value(json!({"id": "missing-format-42", "wsAuth": payload})).unwrap();
        for secret in ["", "secret"] {
            let result = sign_ws_auth(&request, secret);
            assert_eq!(
                serde_json::to_value(result).unwrap(),
                json!({"id": "missing-format-42", "error": "Missing wsAuth payload for WS auth signing"})
            );
        }
        let result = sign_kraken_ws_auth(&request, "c2VjcmV0");
        assert_eq!(result.id, Some(json!("missing-format-42")));
        assert!(result.error.is_none());
        assert!(result.ws_auth_signature.is_some());
    }
    let request: WsAuthRequest = serde_json::from_value(json!({"id": "unknown-kraken", "wsAuth": {"toSign": "challenge", "outputFormat": "future-format"}})).unwrap();
    let result = sign_kraken_ws_auth(&request, "c2VjcmV0");
    assert_eq!(result.id, Some(json!("unknown-kraken")));
    assert!(result.error.is_none());
    assert!(result.ws_auth_signature.is_some());
}

#[test]
fn wrong_json_types_preserve_raw_id_and_kraken_ignores_format() {
    fn kind(value: Option<&Value>) -> &'static str {
        match value {
            None => "missing",
            Some(Value::Null) => "null",
            Some(Value::Bool(_)) => "bool",
            Some(Value::Number(_)) => "number",
            Some(Value::String(_)) => "string",
            Some(Value::Array(_)) => "array",
            Some(Value::Object(_)) => "object",
        }
    }
    let fixture = fixtures();
    let cases = fixture["wire"].as_array().unwrap();
    let required = std::collections::BTreeSet::from([
        "missing", "null", "bool", "number", "string", "array", "object",
    ]);
    for boundary in [0.000001, 1e20] {
        assert!(
            cases
                .iter()
                .any(|case| case["request"]["wsAuth"]["outputFormat"].as_f64() == Some(boundary)),
            "wire corpus must retain ECMAScript number formatting boundary {boundary}"
        );
    }
    for field in ["id", "outputFormat"] {
        let covered: std::collections::BTreeSet<_> = cases
            .iter()
            .map(|case| {
                kind(if field == "id" {
                    case["request"].get(field)
                } else {
                    case["request"]["wsAuth"].get(field)
                })
            })
            .collect();
        assert_eq!(
            covered, required,
            "raw wire corpus must preserve every JSON kind for {field}"
        );
    }
    for case in cases {
        let request: WsAuthRequest = serde_json::from_value(case["request"].clone()).unwrap();
        let response = sign_ws_auth(&request, "secret");
        // JSON keys distinguish missing (TS undefined) from explicitly null ids.
        let expected_id = case["request"].get("id").cloned();
        let mut pending =
            BTreeMap::from([(serde_json::to_string(&expected_id).unwrap(), "waiting")]);
        assert_eq!(
            pending.remove(&serde_json::to_string(&response.id).unwrap()),
            Some("waiting")
        );
        let response_wire = serde_json::to_value(&response).unwrap();
        assert_eq!(response_wire, case["response"]);
        let roundtrip: WsAuthResponse = serde_json::from_value(response_wire).unwrap();
        assert_eq!(roundtrip.id, expected_id);
        assert_eq!(
            serde_json::to_value(sign_kraken_ws_auth(&request, "c2VjcmV0")).unwrap(),
            case["kraken"]
        );
    }
}

#[test]
fn malformed_payload_fields_do_not_destroy_correlation() {
    for ws_auth in [
        json!(123),
        json!(true),
        json!([]),
        json!({"toSign": 123, "outputFormat": "hex"}),
    ] {
        let request: WsAuthRequest =
            serde_json::from_value(json!({"id": "raw-payload", "wsAuth": ws_auth})).unwrap();
        assert_eq!(
            serde_json::to_value(sign_ws_auth(&request, "secret")).unwrap(),
            json!({"id": "raw-payload", "error": "Missing wsAuth payload for WS auth signing"})
        );
    }
}

#[test]
fn text_parser_skips_unrepresentable_ignored_metadata_and_keeps_signatures() {
    let fixture = fixtures();
    let cases = fixture["text"]["signable"].as_array().unwrap();
    assert!(
        cases
            .iter()
            .any(|case| case["wire"].as_str().unwrap().contains("1e400"))
    );
    assert!(
        cases
            .iter()
            .any(|case| case["wire"].as_str().unwrap().contains(&"[".repeat(200)))
    );
    for case in cases {
        let request: WsAuthRequest = serde_json::from_str(case["wire"].as_str().unwrap())
            .expect("ignored metadata must not prevent a response");
        assert_eq!(
            serde_json::to_value(sign_ws_auth(&request, "test-secret-value")).unwrap(),
            case["response"]
        );
        assert_eq!(
            case["response"]["wsAuthSignature"], fixture["ws"][0]["signature"],
            "text parsing retains the existing composition golden"
        );
        assert_eq!(
            serde_json::to_value(sign_kraken_ws_auth(&request, "c2VjcmV0")).unwrap(),
            case["kraken"]
        );
    }
}

#[test]
fn text_parser_retained_overflow_signs_or_errors_but_depth_produces_no_response() {
    let fixture = fixtures();
    let cases = fixture["text"]["retained"].as_array().unwrap();
    let mut coverage = std::collections::BTreeSet::new();
    for case in cases {
        let reason = case["limit"].as_str().unwrap();
        let token = match reason {
            "overflow" => "1e400".to_owned(),
            "depth" => "[".repeat(200),
            other => panic!("unexpected retained-field limit {other}"),
        };
        let wire = case["wire"].as_str().unwrap();
        let field = ["id", "outputFormat"]
            .into_iter()
            .find(|field| wire.contains(&format!("\"{field}\":{token}")))
            .expect("limit token must occur in a retained field in the actual wire");
        assert!(
            coverage.insert((field, reason)),
            "duplicate retained-field limit case"
        );
    }
    assert_eq!(
        coverage,
        std::collections::BTreeSet::from([
            ("id", "overflow"),
            ("id", "depth"),
            ("outputFormat", "overflow"),
            ("outputFormat", "depth"),
        ]),
        "retained-field corpus must cover both fields and both parser limits"
    );
    for case in cases {
        let parsed = serde_json::from_str::<WsAuthRequest>(case["wire"].as_str().unwrap());
        if case["limit"] == "depth" {
            assert!(
                parsed
                    .unwrap_err()
                    .to_string()
                    .contains("recursion limit exceeded")
            );
            continue;
        }
        let request = parsed.expect("workspace arbitrary_precision closes numeric parser refusal");
        for (response, golden) in [
            (
                sign_ws_auth(&request, "test-secret-value"),
                "tsResponseJson",
            ),
            (sign_kraken_ws_auth(&request, "c2VjcmV0"), "tsKrakenJson"),
        ] {
            let mut actual = serde_json::to_value(response).unwrap();
            let mut expected: Value = serde_json::from_str(case[golden].as_str().unwrap()).unwrap();
            if case["wire"].as_str().unwrap().starts_with(r#"{"id":1e400"#) {
                // Rust retains the raw number; JS JSON.stringify maps its Infinity id to null.
                assert_eq!(actual["id"].to_string(), "1e+400");
                assert_eq!(expected["id"], Value::Null);
                if golden == "tsResponseJson" {
                    assert_eq!(expected["wsAuthSignature"], fixture["ws"][0]["signature"]);
                }
                actual.as_object_mut().unwrap().remove("id");
                expected.as_object_mut().unwrap().remove("id");
            }
            assert_eq!(actual, expected);
        }
    }
    for wire in [r#"{"id":"broken","wsAuth": "#, "null", "123", "[]"] {
        assert!(
            serde_json::from_str::<WsAuthRequest>(wire).is_err(),
            "no response for {wire}"
        );
    }
}

#[test]
fn text_parser_invalid_payload_overflow_returns_correlated_errors() {
    let fixture = fixtures();
    let cases = fixture["text"]["invalidPayload"].as_array().unwrap();
    let mut coverage = std::collections::BTreeSet::new();
    for case in cases {
        let limit = case["limit"].as_str().unwrap();
        let token = match limit {
            "overflow" => "1e400".to_owned(),
            "depth" => "[".repeat(200),
            other => panic!("unknown limit {other}"),
        };
        let wire = case["wire"].as_str().unwrap();
        let field = ["toSign", "wsAuth"]
            .into_iter()
            .find(|field| wire.contains(&format!("\"{field}\":{token}")))
            .unwrap();
        assert!(coverage.insert((field, limit)));
        let parsed = serde_json::from_str::<WsAuthRequest>(wire);
        if field == "toSign" && limit == "depth" {
            assert!(
                parsed
                    .unwrap_err()
                    .to_string()
                    .contains("recursion limit exceeded")
            );
        } else {
            let request = parsed.expect("numeric overflow or drained arrays must not lose id");
            assert_eq!(
                serde_json::to_value(sign_ws_auth(&request, "secret")).unwrap(),
                case["response"]
            );
            assert_eq!(
                serde_json::to_value(sign_kraken_ws_auth(&request, "c2VjcmV0")).unwrap(),
                case["kraken"]
            );
        }
    }
    assert_eq!(
        coverage,
        std::collections::BTreeSet::from([
            ("toSign", "overflow"),
            ("toSign", "depth"),
            ("wsAuth", "overflow"),
            ("wsAuth", "depth"),
        ])
    );
}

#[test]
fn primitive_corpus_preserves_input_properties() {
    let fixture = fixtures();
    for (algorithm, block_size) in [("sha256", 64), ("sha512", 128)] {
        for encoding in ["hex", "base64"] {
            let cases: Vec<_> = fixture["primitives"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|case| case["algorithm"] == algorithm && case["encoding"] == encoding)
                .collect();
            assert!(
                cases
                    .iter()
                    .any(|case| case["key"] == "" && case["payload"] == ""),
                "{algorithm}/{encoding} needs empty inputs"
            );
            assert!(
                cases
                    .iter()
                    .any(|case| !case["key"].as_str().unwrap().is_ascii()
                        && !case["payload"].as_str().unwrap().is_ascii()),
                "{algorithm}/{encoding} needs Unicode key and payload"
            );
            assert!(
                cases
                    .iter()
                    .any(|case| case["payload"].as_str().unwrap().contains('\0')),
                "{algorithm}/{encoding} needs an embedded NUL payload"
            );
            assert!(
                cases
                    .iter()
                    .any(|case| case["key"].as_str().unwrap().len() > block_size),
                "{algorithm}/{encoding} needs a key longer than its HMAC block"
            );
        }
    }
}

#[test]
fn arbitrary_precision_overflow_returns_correlated_errors() {
    assert!(
        serde_json::from_str::<Value>("1e400").is_ok(),
        "requires workspace arbitrary_precision"
    );
    let fixture = fixtures();
    let cases = fixture["nonfinite"].as_array().unwrap();
    for shape in ["1e", "-1e", "[1e", "[-1e", "[[1e"] {
        assert!(
            cases.iter().any(|case| case["wire"]
                .as_str()
                .unwrap()
                .contains(&format!("\"outputFormat\":{shape}"))),
            "overflow corpus needs signed scalar/array and nested-array shapes: {shape}"
        );
    }
    for case in cases {
        let parsed = serde_json::from_str::<WsAuthRequest>(case["wire"].as_str().unwrap());
        let request = parsed.expect("workspace arbitrary_precision retains raw overflow tokens");
        assert_eq!(
            serde_json::to_value(sign_ws_auth(&request, "secret")).unwrap(),
            case["response"]
        );
        assert_eq!(
            serde_json::to_value(sign_kraken_ws_auth(&request, "c2VjcmV0")).unwrap(),
            case["kraken"]
        );
    }
}
