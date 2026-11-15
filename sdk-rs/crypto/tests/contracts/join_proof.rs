#![cfg(feature = "admission")]
use safe_cex_crypto::admission::{
    Family, Statement,
    identity::Projection,
    wire::{Request, Response},
};
use serde_json::{Value, json};
fn vectors() -> Vec<Value> {
    serde_json::from_str::<Value>(include_str!("../fixtures/join-proof.json")).unwrap()["cases"]
        .as_array()
        .unwrap()
        .clone()
}
#[test]
fn independent_identity_and_challenge_vectors() {
    for case in vectors() {
        let projection =
            Projection::from_json(case["owner"].as_str().unwrap(), &case["facts"].to_string())
                .unwrap_or_else(|error| panic!("{}: {error:?}", case["facts"]["venue"]));
        assert_eq!(projection.scope(), case["scope"].as_str().unwrap());
        assert_eq!(projection.key_id(), case["keyId"].as_str().unwrap());
        assert_eq!(
            safe_cex_crypto::hex(&projection.digest()),
            case["identityDigest"].as_str().unwrap()
        );
        let statement = Statement::from_json(&case["statement"].to_string()).unwrap();
        assert_eq!(
            safe_cex_crypto::hex(&statement.digest()),
            case["challengeDigest"].as_str().unwrap()
        );
    }
}
fn request() -> Value {
    json!({"id":"request-1","type":"join-proof","exchangeName":"backpack","accountName":"fixture-account","instanceId":"instance-1","joinProof":{"version":1,"statement":vectors()[0]["statement"]}})
}
#[test]
fn closed_wire_round_trip_and_family_widths() {
    let input = request();
    let decoded = Request::from_json(&input.to_string()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), input);
    for (family, width) in [
        (Family::Ed25519, 64),
        (Family::Ethereum, 65),
        (Family::Stark, 64),
        (Family::Schnorr, 80),
    ] {
        use base64::{Engine, engine::general_purpose::STANDARD};
        let signature = STANDARD.encode(vec![7; width]);
        let input = json!({"id":"request-1","joinProof":{"version":1,"signature":signature}});
        let decoded = Response::from_json(&input.to_string(), family).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), input);
        let wrong = json!({"id":"request-1","joinProof":{"version":1,"signature":STANDARD.encode(vec![7;width-1])}});
        assert!(Response::from_json(&wrong.to_string(), family).is_err());
    }
}
#[test]
fn absent_facts_and_foreign_credential_shapes_refuse() {
    for case in vectors() {
        for field in ["venue", "network", "credential"] {
            let mut facts = case["facts"].clone();
            facts.as_object_mut().unwrap().remove(field);
            assert!(Projection::from_json("fixture|owner-a", &facts.to_string()).is_err());
        }
        for field in case["facts"]["credential"].as_object().unwrap().keys() {
            let mut facts = case["facts"].clone();
            facts["credential"].as_object_mut().unwrap().remove(field);
            assert!(
                Projection::from_json("fixture|owner-a", &facts.to_string()).is_err(),
                "{facts}"
            );
        }
        assert!(Projection::from_json("", &case["facts"].to_string()).is_err());
        let mut facts = case["facts"].clone();
        facts["secret"] = "forbidden".into();
        assert!(Projection::from_json("fixture|owner-a", &facts.to_string()).is_err());
    }
    let mut facts = vectors()[0]["facts"].clone();
    facts["venue"] = "hyperliquid".into();
    assert!(Projection::from_json("fixture|owner-a", &facts.to_string()).is_err());
}
#[test]
fn wire_missing_mixed_duplicate_and_unsafe_fields_refuse() {
    let valid = request();
    for field in [
        "id",
        "type",
        "exchangeName",
        "accountName",
        "instanceId",
        "joinProof",
    ] {
        let mut input = valid.clone();
        input.as_object_mut().unwrap().remove(field);
        assert!(Request::from_json(&input.to_string()).is_err());
    }
    for (key, value) in [
        ("config", json!({})),
        ("wsAuth", json!({})),
        ("valid", json!(true)),
    ] {
        let mut input = valid.clone();
        input[key] = value;
        assert!(Request::from_json(&input.to_string()).is_err());
    }
    let mut input = valid.clone();
    input["joinProof"]["version"] = 2.into();
    assert!(Request::from_json(&input.to_string()).is_err());
    let mut input = valid.clone();
    input["joinProof"]["statement"]["issuedAtMs"] = 9007199254740992u64.into();
    assert!(Request::from_json(&input.to_string()).is_err());
    let raw = valid.to_string().replacen('{', "{\"id\":\"duplicate\",", 1);
    assert!(Request::from_json(&raw).is_err());
    for input in [
        r#"{"id":"x","joinProof":{"version":1,"signature":"AA=="},"wsAuthSignature":"AA=="}"#,
        r#"{"id":"x","error":"raw callback text"}"#,
        r#"{"id":"x","error":"Join proof refused","joinProof":{"version":1,"signature":"AA=="}}"#,
    ] {
        assert!(Response::from_json(input, Family::Ed25519).is_err());
    }
}
#[test]
fn identity_owner_network_and_vault_are_bound() {
    let input = vectors()[1]["facts"].clone();
    let base = Projection::from_json("fixture|owner-a", &input.to_string())
        .unwrap()
        .digest();
    assert_ne!(
        base,
        Projection::from_json("fixture|owner-b", &input.to_string())
            .unwrap()
            .digest()
    );
    let mut changed = input.clone();
    changed["network"] = "testnet".into();
    assert_ne!(
        base,
        Projection::from_json("fixture|owner-a", &changed.to_string())
            .unwrap()
            .digest()
    );
    changed = input;
    changed["credential"]["accountScope"] =
        json!({"kind":"vault","address":"0x3333333333333333333333333333333333333333"});
    assert_ne!(
        base,
        Projection::from_json("fixture|owner-a", &changed.to_string())
            .unwrap()
            .digest()
    );
}

#[test]
fn nested_duplicate_null_unknown_and_malformed_facts_refuse() {
    for case in vectors() {
        let original = case["facts"].clone();
        for field in original["credential"].as_object().unwrap().keys() {
            let mut facts = original.clone();
            facts["credential"][field] = Value::Null;
            assert!(
                Projection::from_json("owner", &facts.to_string()).is_err(),
                "{facts}"
            );
        }
        let mut facts = original.clone();
        facts["credential"]["secret"] = "forbidden".into();
        assert!(Projection::from_json("owner", &facts.to_string()).is_err());
        let raw = original.to_string().replacen(
            "\"credential\":{",
            "\"credential\":{\"kind\":\"foreign\",",
            1,
        );
        assert!(Projection::from_json("owner", &raw).is_err());
        let raw = original
            .to_string()
            .replacen('{', "{\"network\":\"mainnet\",", 1);
        assert!(Projection::from_json("owner", &raw).is_err());
    }
    let mut facts = vectors()[1]["facts"].clone();
    for scope in [
        json!({"kind":"main","address":"unexpected"}),
        json!({"kind":"vault"}),
        json!({"kind":"vault","address":null}),
    ] {
        facts["credential"]["accountScope"] = scope;
        assert!(Projection::from_json("owner", &facts.to_string()).is_err());
    }
    let mut facts = vectors()[0]["facts"].clone();
    for key in [
        "0x00".repeat(32),
        format!("0x{}", "00".repeat(32)),
        format!("0x{}", "ff".repeat(32)),
        "not-a-key".into(),
    ] {
        facts["credential"]["publicKey"] = key.into();
        assert!(Projection::from_json("owner", &facts.to_string()).is_err());
    }
    let mut facts = vectors().last().unwrap()["facts"].clone();
    facts["credential"]["account"] = format!("0x{}", "22".repeat(32)).into();
    assert!(Projection::from_json("owner", &facts.to_string()).is_err());
}

#[test]
fn wire_nested_statement_and_signature_encodings_are_closed() {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let valid = request();
    for field in ["kind", "attempt", "identityDigest", "nonce", "issuedAtMs"] {
        let mut input = valid.clone();
        input["joinProof"]["statement"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(Request::from_json(&input.to_string()).is_err());
        let mut input = valid.clone();
        input["joinProof"]["statement"][field] = Value::Null;
        assert!(Request::from_json(&input.to_string()).is_err());
    }
    for (field, value) in [
        ("nonce", json!(vec![0; 31])),
        ("issuedAtMs", json!(1.5)),
        ("attempt", json!("a/b")),
        ("valid", json!(true)),
    ] {
        let mut input = valid.clone();
        input["joinProof"]["statement"][field] = value;
        assert!(Request::from_json(&input.to_string()).is_err());
    }
    let raw = valid.to_string().replace(
        "\"statement\":{",
        "\"statement\":{\"attempt\":\"duplicate\",",
    );
    assert!(Request::from_json(&raw).is_err());
    for (field, value) in [
        ("id", json!("")),
        ("instanceId", json!(" i")),
        ("accountName", json!("a\nb")),
    ] {
        let mut input = valid.clone();
        input[field] = value;
        assert!(Request::from_json(&input.to_string()).is_err());
    }
    let mut boundary = valid.clone();
    boundary["joinProof"]["statement"]["issuedAtMs"] = 9_007_199_254_740_991u64.into();
    assert!(Request::from_json(&boundary.to_string()).is_ok());
    let signature = STANDARD.encode([7; 64]);
    for wrong in [
        signature.trim_end_matches('=').into(),
        format!(" {signature}"),
        "AA==".into(),
    ] {
        let input = json!({"id":"x","joinProof":{"version":1,"signature":wrong}});
        assert!(Response::from_json(&input.to_string(), Family::Ed25519).is_err());
    }
    let signed = json!({"id":"x","joinProof":{"version":1,"signature":signature}});
    let duplicate = signed
        .to_string()
        .replacen('{', "{\"id\":\"duplicate\",", 1);
    assert!(Response::from_json(&duplicate, Family::Ed25519).is_err());
    let mut future = signed.clone();
    future["joinProof"]["version"] = 2.into();
    assert!(Response::from_json(&future.to_string(), Family::Ed25519).is_err());
    let mut extra_nested = signed.clone();
    extra_nested["joinProof"]["valid"] = true.into();
    assert!(Response::from_json(&extra_nested.to_string(), Family::Ed25519).is_err());
    for extra in ["config", "wsAuthSignature", "error", "publicKey", "valid"] {
        let mut input = json!({"id":"x","joinProof":{"version":1,"signature":signature}});
        input[extra] = Value::Null;
        assert!(Response::from_json(&input.to_string(), Family::Ed25519).is_err());
    }
    for error in ["Join proof refused", "Join proof unavailable"] {
        let input = json!({"id":"x","error":error});
        let response = Response::from_json(&input.to_string(), Family::Ed25519).unwrap();
        assert_eq!(response.id(), "x");
        assert!(response.signature().is_none());
    }
    assert!(Request::from_json(&" ".repeat(4097)).is_err());
    assert!(Projection::from_json("owner", &" ".repeat(8193)).is_err());
}

#[test]
fn normalization_preserves_scope_and_exact_alias_separation() {
    let all = vectors();
    let project = |facts: &Value| {
        Projection::from_json("owner", &facts.to_string())
            .unwrap()
            .digest()
    };
    let input = all[0]["facts"].clone();
    let mut upper = input.clone();
    let key = upper["credential"]["publicKey"].as_str().unwrap();
    upper["credential"]["publicKey"] = format!("0x{}", key[2..].to_uppercase()).into();
    assert_eq!(project(&input), project(&upper));
    assert_eq!(project(&all[1]["facts"]), project(&all[2]["facts"]));
    for pair in all[7..12].windows(2) {
        assert_ne!(project(&pair[0]["facts"]), project(&pair[1]["facts"]));
    }
    assert_ne!(project(&all[12]["facts"]), project(&all[13]["facts"]));
    let mut index = all[12]["facts"].clone();
    let original = project(&index);
    index["credential"]["accountIndex"] = format!(
        "00{}",
        index["credential"]["accountIndex"].as_str().unwrap()
    )
    .into();
    assert_eq!(original, project(&index));
    for bad in ["-1", "1.5", "18446744073709551616"] {
        index["credential"]["accountIndex"] = bad.into();
        assert!(Projection::from_json("owner", &index.to_string()).is_err());
    }
}
