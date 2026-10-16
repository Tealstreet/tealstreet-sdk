use serde::Deserialize;
mod support;
use safe_cex_venue_lighter::types::*;
use serde::Serialize;
use serde_json::{Value, json};
use std::process::Command;

fn profile_identity(value: &LighterDeploymentProfile) -> Value {
    let network = match value.network {
        LighterDeploymentNetwork::Livenet => "livenet",
        LighterDeploymentNetwork::Testnet => "testnet",
        LighterDeploymentNetwork::Robinhood => "robinhood",
        LighterDeploymentNetwork::RobinhoodTestnet => "robinhood-testnet",
    };
    let quote = match value.quote_currency {
        LighterQuoteCurrency::Usdc => "USDC",
        LighterQuoteCurrency::Usdg => "USDG",
    };
    json!({"network":network,"apiBaseUrl":value.api_base_url,"wsUrl":value.ws_url,"signerChainId":value.signer_chain_id,"quoteCurrency":quote,"usesRestProxy":value.uses_rest_proxy,"supportsWsProxy":value.supports_ws_proxy})
}

fn object<K: Serialize, V: Serialize, const N: usize>(entries: [(K, V); N]) -> Value {
    let entries = serde_json::to_value(entries.as_slice()).unwrap();
    Value::Object(
        entries
            .as_array()
            .unwrap()
            .iter()
            .map(|pair| (pair[0].as_str().unwrap().to_owned(), pair[1].clone()))
            .collect(),
    )
}

fn assert_value(actual: &Value, expected: &Value, name: &str) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(a.as_f64(), b.as_f64(), "{name}"),
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(
                a.keys().collect::<Vec<_>>(),
                b.keys().collect::<Vec<_>>(),
                "{name} keys"
            );
            for (key, value) in a {
                assert_value(value, &b[key], &format!("{name}.{key}"));
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{name} length");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                assert_value(a, b, &format!("{name}[{i}]"));
            }
        }
        _ => assert_eq!(actual, expected, "{name}"),
    }
}

// Exhaustive matches ensure an added Rust variant cannot evade the live union check.
fn network_name(value: LighterDeploymentNetwork) -> &'static str {
    match value {
        LighterDeploymentNetwork::Livenet => "livenet",
        LighterDeploymentNetwork::Testnet => "testnet",
        LighterDeploymentNetwork::Robinhood => "robinhood",
        LighterDeploymentNetwork::RobinhoodTestnet => "robinhood-testnet",
    }
}
fn quote_name(value: LighterQuoteCurrency) -> &'static str {
    match value {
        LighterQuoteCurrency::Usdc => "USDC",
        LighterQuoteCurrency::Usdg => "USDG",
    }
}

#[test]
#[ignore = "requires Node and the maintained TypeScript compiler"]
fn lighter_protocol_tables_match_live_typescript() {
    let output = Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/oracle.cjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "TypeScript oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    for literal in oracle["networks"].as_array().unwrap() {
        let invalid = serde_json::json!({literal.as_str().unwrap():null});
        assert!(
            LighterDeploymentNetwork::deserialize(invalid).is_err(),
            "LighterDeploymentNetwork public qualified decoder rejects object"
        );
    }
    for literal in oracle["quoteCurrencies"].as_array().unwrap() {
        let invalid = serde_json::json!({literal.as_str().unwrap():null});
        assert!(
            LighterQuoteCurrency::deserialize(invalid).is_err(),
            "LighterQuoteCurrency public qualified decoder rejects object"
        );
    }

    support::assert_string_only::<LighterDeploymentNetwork>(
        &oracle["networks"],
        "LighterDeploymentNetwork",
    );
    support::assert_string_only::<LighterQuoteCurrency>(
        &oracle["quoteCurrencies"],
        "LighterQuoteCurrency",
    );

    assert_eq!(
        json!(support::enum_inventory::<LighterDeploymentNetwork>()),
        oracle["networks"],
        "LighterDeploymentNetwork accepted wire inventory"
    );
    assert_eq!(
        json!(support::enum_inventory::<LighterQuoteCurrency>()),
        oracle["quoteCurrencies"],
        "LighterQuoteCurrency accepted wire inventory"
    );
    let actual = json!({
        "RECV_WINDOW": RECV_WINDOW,
        "LIGHTER_DEPLOYMENT_PROFILES": object(LIGHTER_DEPLOYMENT_PROFILES),
        "ENDPOINTS": object(ENDPOINTS),
        "GRAFANA_ENDPOINTS_EVERY": GRAFANA_ENDPOINTS_EVERY,
        "GRAFANA_ENDPOINTS_PERIODIC": GRAFANA_ENDPOINTS_PERIODIC,
        "INTERVAL": object(INTERVAL),
        "ORDER_STATUS": object(ORDER_STATUS), "ORDER_TYPE": object(ORDER_TYPE),
        "ORDER_SIDE": object(ORDER_SIDE), "ORDER_TIME_IN_FORCE": object(ORDER_TIME_IN_FORCE),
        "ORDER_TRIGGER_TYPE": object(ORDER_TRIGGER_TYPE),
    });
    assert_eq!(
        actual.as_object().unwrap().keys().collect::<Vec<_>>(),
        oracle["tables"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        "exported protocol table inventory"
    );
    for (name, value) in actual.as_object().unwrap() {
        assert_value(value, &oracle["tables"][name], name);
    }
    let networks = [
        LighterDeploymentNetwork::Livenet,
        LighterDeploymentNetwork::Testnet,
        LighterDeploymentNetwork::Robinhood,
        LighterDeploymentNetwork::RobinhoodTestnet,
    ];
    assert_eq!(
        serde_json::to_value(networks).unwrap(),
        oracle["networks"],
        "LighterDeploymentNetwork"
    );
    for network in networks {
        assert_eq!(
            serde_json::to_value(network).unwrap(),
            json!(network_name(network))
        );
    }
    let quotes = [LighterQuoteCurrency::Usdc, LighterQuoteCurrency::Usdg];
    assert_eq!(
        serde_json::to_value(quotes).unwrap(),
        oracle["quoteCurrencies"],
        "LighterDeploymentProfile.quoteCurrency"
    );
    for quote in quotes {
        assert_eq!(
            serde_json::to_value(quote).unwrap(),
            json!(quote_name(quote))
        );
    }
    assert_eq!(
        oracle["fields"],
        json!([
            ["network", "LighterDeploymentNetwork"],
            ["apiBaseUrl", "string"],
            ["wsUrl", "string"],
            ["signerChainId", "number"],
            ["quoteCurrency", "'USDC' | 'USDG'"],
            ["usesRestProxy", "boolean"],
            ["supportsWsProxy", "boolean"]
        ])
    );
    for profile in oracle["tables"]["LIGHTER_DEPLOYMENT_PROFILES"]
        .as_object()
        .unwrap()
        .values()
    {
        let positional = Value::Array(profile.as_object().unwrap().values().cloned().collect());
        assert!(
            serde_json::from_value::<LighterDeploymentProfile>(positional.clone()).is_err(),
            "LighterDeploymentProfile rejects positional array"
        );
        assert!(
            LighterDeploymentProfile::deserialize(positional).is_err(),
            "LighterDeploymentProfile public decoder rejects positional array"
        );
        let parsed: LighterDeploymentProfile = serde_json::from_value(profile.clone()).unwrap();
        assert_value(
            &profile_identity(&parsed),
            profile,
            "DeploymentProfile native field identity",
        );
        for (rest, ws) in [(false, false), (false, true), (true, false), (true, true)] {
            let mut input = profile.clone();
            input["usesRestProxy"] = json!(rest);
            input["supportsWsProxy"] = json!(ws);
            let decoded: LighterDeploymentProfile = serde_json::from_value(input.clone()).unwrap();
            assert_eq!(
                decoded.uses_rest_proxy, rest,
                "DeploymentProfile.usesRestProxy native identity"
            );
            assert_eq!(
                decoded.supports_ws_proxy, ws,
                "DeploymentProfile.supportsWsProxy native identity"
            );
            assert_value(
                &profile_identity(&decoded),
                &input,
                "DeploymentProfile native field identity",
            );
        }
        assert_value(
            &serde_json::to_value(parsed).unwrap(),
            profile,
            "profile roundtrip",
        );
        for number in [-0.25, 0.0, 304.5, 16_777_217.0, 1e100] {
            let mut fractional = profile.clone();
            fractional["signerChainId"] = json!(number);
            let parsed: LighterDeploymentProfile =
                serde_json::from_value(fractional.clone()).unwrap();
            assert_value(
                &serde_json::to_value(parsed).unwrap(),
                &fractional,
                "number domain",
            );
        }
        for field in profile.as_object().unwrap().keys() {
            let mut missing = profile.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<LighterDeploymentProfile>(missing).is_err(),
                "missing {field}"
            );
            let mut null = profile.clone();
            null[field] = Value::Null;
            assert!(
                serde_json::from_value::<LighterDeploymentProfile>(null).is_err(),
                "null {field}"
            );
        }
    }
    for invalid in ["", "mainnet", "constructor", "__proto__"] {
        assert!(serde_json::from_value::<LighterDeploymentNetwork>(json!(invalid)).is_err());
        assert!(serde_json::from_value::<LighterQuoteCurrency>(json!(invalid)).is_err());
    }
}
