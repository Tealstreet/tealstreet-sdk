use safe_cex_crypto::starknet::hash_typed_data;
use serde_json::Value;
#[test]
fn stark_domain_version_is_not_revision() {
    let corpus: Value = serde_json::from_str(include_str!(
        "fixtures/starknet/domain-field-identities.json"
    ))
    .unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 4, "DOMAIN-IDENTITY source case total");
    let mut native = Vec::new();
    for index in 0..4 {
        let case = cases
            .get(index)
            .expect("DOMAIN-IDENTITY expected/native index coverage");
        let actual = hash_typed_data(
            case["raw"].as_str().unwrap(),
            &starknet_crypto::Felt::from_hex(case["address"].as_str().unwrap()).unwrap(),
        )
        .unwrap_or_else(|error| panic!("Stark declared domain version identity: {error}"));
        native.push(format!("{actual:#x}"));
    }
    assert_eq!(
        native.len(),
        4,
        "DOMAIN-IDENTITY independent native case total"
    );
    assert_eq!(
        native.len(),
        cases.len(),
        "DOMAIN-IDENTITY independent source/native length"
    );
    let mut comparisons = 0;
    for index in 0..4 {
        let case = &cases[index];
        assert_eq!(
            native[index],
            case["hash"].as_str().unwrap(),
            "Stark declared domain version identity {}",
            case["version"]
        );
        comparisons += 1;
    }
    assert_eq!(comparisons, 4, "DOMAIN-IDENTITY native comparison total");
}
