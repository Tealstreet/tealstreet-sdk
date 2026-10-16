use safe_cex_crypto::zklink::contract::{
    Contract, ContractSigner, closest_packable_amount, closest_packable_fee,
};
use serde_json::Value;
use std::process::Command;

fn corpus() -> Value {
    // Every input enters through retained raw JSON; no f64 numeric projection.
    serde_json::from_str(include_str!("fixtures/zklink/contracts.json")).unwrap()
}

fn contract(value: &Value) -> Contract {
    serde_json::from_str(&value.to_string()).unwrap()
}

#[test]
fn contract_oracle_artifacts_remain_unchanged() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/zklink/contracts.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "pinned npm ContractBuilder oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn contract_fields_and_wire_signatures_match_npm() {
    let corpus = corpus();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 24, "contract supplement case floor");
    let base = &cases[0];
    for case in &cases[..20] {
        let input = contract(&case["input"]);
        let signer = ContractSigner::from_signature(case["seed"].as_str().unwrap()).unwrap();
        let signed = signer.sign_contract(&input).unwrap();
        assert_eq!(
            signed, case["signed"],
            "contract field/signature {}",
            case["name"]
        );
        assert_eq!(
            signed.to_string(),
            case["signed"].to_string(),
            "contract wire property order {}",
            case["name"]
        );
    }
    // Each independent field mutation changes the actual SDK signature while
    // retaining the same signer. Equal fixtures cannot conceal a field swap.
    for case in &cases[1..12] {
        assert_ne!(case["signed"]["signature"], base["signed"]["signature"]);
        let a = case["input"].as_object().unwrap();
        let b = base["input"].as_object().unwrap();
        assert_eq!(
            a.iter()
                .filter(|(key, value)| b.get(*key) != Some(*value))
                .count(),
            1
        );
    }
    let bytes = contract(&base["input"]).signing_bytes().unwrap();
    assert_eq!(&bytes[..13], &[254, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 1]);
    assert_eq!(
        &bytes[13..18],
        &((12_345_678_901u64 << 5) | 8).to_be_bytes()[3..]
    );
    assert_eq!(
        &bytes[18..33],
        &987_654_321_098_765_432_109_876_543_210u128.to_be_bytes()[1..]
    );
    assert_eq!(&bytes[33..], &[12, 13, 0]);
}

#[test]
fn contract_seed_bytes_match_npm_without_recovery_normalization() {
    let corpus = corpus();
    for case in &corpus["cases"].as_array().unwrap()[20..] {
        let input = contract(&case["input"]);
        let seed = case["seed"].as_str().unwrap();
        let signer = ContractSigner::from_signature(&format!("0x{seed}")).unwrap();
        assert_eq!(
            signer.sign_contract(&input).unwrap(),
            case["signed"],
            "contract seed/signature {}",
            case["name"]
        );
    }
}

#[test]
fn contract_amount_and_fee_rounding_match_npm() {
    let corpus = corpus();
    for (field, run) in [
        (
            "amounts",
            closest_packable_amount as fn(&str) -> Result<String, &'static str>,
        ),
        ("fees", closest_packable_fee),
    ] {
        let cases = corpus[field].as_array().unwrap();
        assert!(cases.len() >= 8);
        for case in cases {
            assert_eq!(
                run(case["input"].as_str().unwrap()).unwrap(),
                case["output"].as_str().unwrap(),
                "contract {field} rounding {}",
                case["input"]
            );
        }
    }
}

#[test]
fn contract_bounds_are_explicit_errors_before_signing() {
    let corpus = corpus();
    let mut input = contract(&corpus["cases"][0]["input"]);
    for raw in [
        r#""340282366920938463463374607431768211456""#,
        r#""-1""#,
        r#""0x123""#,
        r#"" 1""#,
        r#""_1""#,
        r#""++1""#,
        r#""""#,
    ] {
        let amount: String = serde_json::from_str(raw).unwrap();
        input.size = amount.clone();
        assert!(input.signing_bytes().is_err(), "invalid amount {raw}");
        assert!(closest_packable_amount(&amount).is_err());
        assert!(closest_packable_fee(&amount).is_err());
    }
    // Source FloatConversions may panic after overflowing its u128 product;
    // these are explicit native refusals, not claims about source error text.
    assert!(closest_packable_amount("340282366920938463463374607431768211455").is_err());
    assert!(closest_packable_fee("20470000000000000000000000000000001").is_err());
    input.size = "1".into();
    input.price = "1329227995784915872903807060280344576".into();
    let signer = ContractSigner::from_signature(&"01".repeat(65)).unwrap();
    assert_eq!(
        signer.sign_contract(&input),
        Err("zkLink contract price exceeds 120 bits")
    );

    // Contract is the typed post-WASM ABI. Raw overflow tokens must fail here
    // rather than rounding through f64; the venue owns WASM modulo coercion.
    for (field, token) in [
        ("accountId", "9007199254740993"),
        ("size", "9007199254740993"),
    ] {
        let raw = corpus["cases"][0]["input"].to_string();
        let original = if field == "accountId" {
            "16909060"
        } else {
            "\"1234567890123456789\""
        };
        assert!(serde_json::from_str::<Contract>(&raw.replace(original, token)).is_err());
    }
    for seed in ["", "0x00", &"01".repeat(64), &"gg".repeat(65)] {
        assert!(ContractSigner::from_signature(seed).is_err());
    }
}

#[test]
fn contract_named_fields_reject_complete_positional_abi_arrays() {
    use serde::Deserialize;
    let raw = r#"{"accountId":16909060,"subAccountId":5,"slotId":1543,"nonce":526602,"pairId":11,"size":"1","price":"2","direction":true,"feeRates":[12,13],"hasSubsidy":false}"#;
    let text: Contract = serde_json::from_str(raw).unwrap();
    let qualified = Contract::deserialize(serde_json::from_str::<Value>(raw).unwrap()).unwrap();
    for value in [text, qualified] {
        assert_eq!(value.account_id, 16_909_060);
        assert_eq!(value.sub_account_id, 5);
        assert_eq!(value.slot_id, 1543);
        assert_eq!(value.nonce, 526_602);
        assert_eq!(value.pair_id, 11);
        assert_eq!(value.size, "1");
        assert_eq!(value.price, "2");
        assert!(value.direction);
        assert_eq!(value.fee_rates, [12, 13]);
        assert!(!value.has_subsidy);
        assert_eq!(
            value.signing_bytes().unwrap(),
            [
                254, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 1, 0, 0, 0, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 2, 12, 13, 0
            ]
        );
    }
    for raw in [
        r#"[16909060,5,1543,526602,11,"1","2",true,[12,13],false]"#,
        "[]",
        "null",
        "3",
        "true",
        "\"x\"",
    ] {
        assert!(
            serde_json::from_str::<Contract>(raw).is_err(),
            "raw contract shape {raw}"
        );
        let value: Value = serde_json::from_str(raw).unwrap();
        assert!(
            Contract::deserialize(value).is_err(),
            "qualified contract shape {raw}"
        );
    }
}
