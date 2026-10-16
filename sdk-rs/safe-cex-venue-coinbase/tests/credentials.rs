use safe_cex_venue_coinbase::utils::{
    base64_to_bytes, get_coinbase_json_secret, normalize_coinbase_api_credentials,
    parse_coinbase_credential_json,
};
use serde_json::{Value, json};
use std::{process::Command, sync::OnceLock};
fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/credentials.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}

fn credential_parse_inputs() -> [&'static str; 9] {
    [
        "",
        "  {\"a\":1} ",
        "\u{feff}{\"a\":1}",
        "null",
        "[]",
        "{}",
        " { invalid",
        "true",
        "{\"a\":1} trailing",
    ]
}

fn credential_normalization_inputs() -> Vec<(String, String)> {
    let valid_key = "organizations/org-1/apiKeys/key-1";
    let private_pem = "-----BEGIN EC PRIVATE KEY-----\nAQID\n-----END EC PRIVATE KEY-----";
    let keys = [
        valid_key.to_owned(),
        format!(" {valid_key}\u{feff}"),
        format!("{valid_key}/extra"),
        "organizations//apiKeys/key".to_owned(),
        format!("{{\"name\":\"{valid_key}\"}}"),
        "{\"name\":7}".to_owned(),
    ];
    let secrets = [
        private_pem.to_owned(),
        "-----BEGIN PRIVATE KEY-----x-----END EC PRIVATE KEY-----".to_owned(),
        "-----BEGIN EC PRIVATE KEY----------END PRIVATE KEY-----".to_owned(),
        "AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyA=".to_owned(),
        "ed25519:AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyAhIiMkJSYnKCkqKywtLi8wMTIzNDU2Nzg5Ojs8PT4/QA==".to_owned(),
        format!("{{\"private_key\":{}}}", serde_json::to_string(private_pem).unwrap()),
        "{\"secret\":\"invalid\"}".to_owned(),
        String::new(),
        "not-json".to_owned(),
    ];
    keys.into_iter()
        .flat_map(|key| {
            secrets
                .iter()
                .map(move |secret| (key.clone(), secret.clone()))
        })
        .collect()
}
#[test]
fn credential_secret_aliases_keep_empty_strings_and_precedence() {
    for case in oracle()["secrets"].as_array().unwrap() {
        assert_eq!(
            json!(get_coinbase_json_secret(case.get("input"))),
            case["result"],
            "{case}"
        );
    }
}

#[test]
fn credential_json_parser_matches_live_typescript() {
    assert_eq!(oracle()["credentialJson"].as_array().unwrap().len(), 13);
    for case in oracle()["credentialJson"].as_array().unwrap() {
        let actual = parse_coinbase_credential_json(case["input"].as_str().unwrap())
            .unwrap_or(serde_json::Value::Null);
        assert_eq!(actual, case["result"], "parseCoinbaseCredentialJson {case}");
    }
    assert_eq!(oracle()["credentialJsonSurrogateAccepted"], true);
    assert_eq!(parse_coinbase_credential_json(r#"{"x":"\ud800"}"#), None);
}

#[test]
fn credential_normalizer_matches_live_typescript() {
    assert_eq!(
        oracle()["recoveryNormalization"].as_array().unwrap().len(),
        19
    );
    for case in oracle()["recoveryNormalization"].as_array().unwrap() {
        let actual = match normalize_coinbase_api_credentials(
            case["key"].as_str().unwrap(),
            case["secret"].as_str().unwrap(),
        ) {
            Ok(value) => json!({
                "algorithm": match value.algorithm { safe_cex_venue_coinbase::utils::CoinbaseSignatureAlgorithm::Es256 => "ES256", safe_cex_venue_coinbase::utils::CoinbaseSignatureAlgorithm::EdDsa => "EdDSA" },
                "ed25519Seed": value.ed25519_seed,
                "keyName": value.key_name,
                "keySecret": value.key_secret,
            }),
            Err(error) => json!({ "error": error }),
        };
        let expected = if let Some(error) = case.get("error") {
            json!({ "error": error })
        } else {
            case["result"].clone()
        };
        assert_eq!(actual, expected, "normalizeCoinbaseApiCredentials {case}");
    }
}

#[test]
fn credential_base64_preserves_prefix_whitespace_and_forgiving_bits() {
    for case in oracle()["base64"].as_array().unwrap() {
        assert_eq!(
            json!(base64_to_bytes(case["input"].as_str().unwrap())),
            case["result"],
            "{case}"
        );
    }
}

#[test]
fn credential_json_parse_matches_live_typescript() {
    let cases = oracle()["parse"].as_array().unwrap();
    let inputs = credential_parse_inputs();
    assert_eq!(cases.len(), inputs.len(), "credential parse source total");
    let mut null_results = 0;
    let mut object_results = 0;
    let mut array_results = 0;
    let mut boolean_results = 0;
    for (case, expected_input) in cases.iter().zip(inputs) {
        assert_eq!(
            case["input"], expected_input,
            "credential parse case identity"
        );
        match case["result"] {
            Value::Null => null_results += 1,
            Value::Object(_) => object_results += 1,
            Value::Array(_) => array_results += 1,
            Value::Bool(_) => boolean_results += 1,
            ref other => panic!("unexpected credential parse result: {other}"),
        }
        assert_eq!(
            json!(parse_coinbase_credential_json(
                case["input"].as_str().unwrap()
            )),
            case["result"],
            "{case}"
        );
    }
    assert_eq!(
        (null_results, object_results, array_results, boolean_results),
        (6, 3, 0, 0),
        "credential parse result branches"
    );
}

#[test]
fn credential_normalization_matches_live_typescript() {
    let cases = oracle()["normalization"].as_array().unwrap();
    let inputs = credential_normalization_inputs();
    assert_eq!(cases.len(), 54, "credential normalization source total");
    assert_eq!(
        inputs.len(),
        54,
        "independent credential normalization inventory"
    );
    let mut successes = 0;
    let mut errors = 0;
    let mut es256 = 0;
    let mut eddsa = 0;
    for (case, (expected_key, expected_secret)) in cases.iter().zip(inputs) {
        assert_eq!(case["key"], expected_key, "normalization key case identity");
        assert_eq!(
            case["secret"], expected_secret,
            "normalization secret case identity"
        );
        let actual = normalize_coinbase_api_credentials(
            case["key"].as_str().unwrap(),
            case["secret"].as_str().unwrap(),
        );
        let projection = match actual {
            Ok(value) => {
                let mut result = json!({
                    "algorithm": match value.algorithm {
                        safe_cex_venue_coinbase::utils::CoinbaseSignatureAlgorithm::Es256 => "ES256",
                        safe_cex_venue_coinbase::utils::CoinbaseSignatureAlgorithm::EdDsa => "EdDSA",
                    },
                    "keyName": value.key_name,
                    "keySecret": value.key_secret,
                });
                if let Some(seed) = value.ed25519_seed {
                    result["ed25519Seed"] = json!(seed);
                }
                json!({"result":result})
            }
            Err(message) => json!({"error":{"name":"Error","message":message}}),
        };
        let expected = if case.get("result").is_some() {
            successes += 1;
            match case["result"]["algorithm"].as_str().unwrap() {
                "ES256" => es256 += 1,
                "EdDSA" => eddsa += 1,
                other => panic!("unknown credential algorithm {other}"),
            }
            json!({"result":case["result"]})
        } else {
            errors += 1;
            json!({"error":case["error"]})
        };
        assert_eq!(projection, expected, "{case}");
    }
    assert_eq!(
        (successes, errors, es256, eddsa),
        (15, 39, 9, 6),
        "credential normalization result branches"
    );
}

#[test]
fn escaped_surrogate_credential_json_divergence_reaches_normalization() {
    let key = r#"{"name":"organizations/org/apiKeys/id","ignored":"\ud800"}"#;
    let secret = "-----BEGIN EC PRIVATE KEY-----\nAQID\n-----END EC PRIVATE KEY-----";
    let source = &oracle()["surrogateNormalization"];
    assert_eq!(source["key"], key, "surrogate credential witness identity");
    assert_eq!(source["secret"], secret);
    assert_eq!(
        source["algorithm"], "ES256",
        "source accepts unused-surrogate credential property"
    );
    assert_eq!(source["keyName"], "organizations/org/apiKeys/id");
    let error = normalize_coinbase_api_credentials(key, secret).unwrap_err();
    assert_eq!(
        error,
        "Coinbase API key must be the Advanced Trade CDP key name, like organizations/{org_id}/apiKeys/{key_id}.",
        "native downstream UTF-8 JSON refusal finding"
    );
}
