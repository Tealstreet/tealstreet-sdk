use super::*;
use safe_cex_crypto::lighter::{public_key, schnorr_verify};
use serde_json::Value;

fn hex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
fn vectors() -> Value {
    serde_json::from_str(include_str!("../fixtures/modify-signing.json")).unwrap()
}
fn input(case: &Value) -> TxInput {
    let info = &case["txInfo"];
    let order: Modify = serde_json::from_value(serde_json::json!({
        "MarketIndex":info["MarketIndex"], "Index":info["Index"],
        "BaseAmount":info["BaseAmount"], "Price":info["Price"], "TriggerPrice":info["TriggerPrice"]
    }))
    .unwrap();
    tx_input(
        &Account::new(case["chainId"].as_u64().unwrap() as u32, 42, 3).unwrap(),
        &Transaction::Modify(order),
        info["Nonce"].as_u64().unwrap(),
        info["ExpiredAt"].as_u64().unwrap() - TX_TTL,
    )
    .unwrap()
}
#[test]
fn tx17_matches_official_sdk_hash_signature_statement_and_wire() {
    let corpus = vectors();
    let public = public_key(&hex(corpus["secret"].as_str().unwrap())).unwrap();
    assert_eq!(corpus["cases"].as_array().unwrap().len(), 9);
    for case in corpus["cases"].as_array().unwrap() {
        let local = input(case);
        assert_eq!(local.tx_type, 17);
        assert_eq!(
            safe_cex_crypto::hex(&poseidon2_hash_fp5(&local.data).unwrap()),
            case["txHash"]
        );
        let sig = hex(case["signatureHex"].as_str().unwrap());
        assert!(schnorr_verify(&local.preimage, &public, &sig).unwrap());
        let text = statement::txs(&local.account, std::slice::from_ref(&local));
        let derived = statement::derive(
            &text,
            &statement::Owner {
                chain_id: local.account.chain_id,
                account_index: Some(42),
                api_key_index: 3,
            },
        )
        .unwrap();
        assert_eq!(derived.preimages, vec![local.preimage.clone()]);
        let wire: Value = serde_json::from_str(&local.finish(&sig).unwrap().tx_info).unwrap();
        let mut official = case["txInfo"].clone();
        assert!(
            official
                .as_object_mut()
                .unwrap()
                .remove("L2TxAttributes")
                .unwrap()
                .is_null()
        );
        assert_eq!(wire, official);
    }
}
#[test]
fn changing_each_signed_tx17_field_invalidates_the_official_signature() {
    let corpus = vectors();
    let case = &corpus["cases"][0];
    let original = input(case);
    let public = public_key(&hex(corpus["secret"].as_str().unwrap())).unwrap();
    let sig = hex(case["signatureHex"].as_str().unwrap());
    assert!(schnorr_verify(&original.preimage, &public, &sig).unwrap());
    for word in 0..11 {
        let mut changed = original.preimage.clone();
        changed[word * 8] ^= 1;
        assert!(
            !schnorr_verify(&changed, &public, &sig).unwrap(),
            "signed word {word}"
        );
    }
}
#[test]
fn remote_modify_statement_rejects_smuggled_fields_types_and_out_of_range_values() {
    let original = input(&vectors()["cases"][0]);
    let text = statement::txs(&original.account, std::slice::from_ref(&original));
    let owner = statement::Owner {
        chain_id: 300,
        account_index: Some(42),
        api_key_index: 3,
    };
    assert!(statement::derive(&text, &owner).is_ok());
    for (from, to) in [
        ("\"MarketIndex\":1", "\"MarketIndex\":255"),
        ("\"Index\":42", "\"Index\":0"),
        ("\"BaseAmount\":2000", "\"BaseAmount\":0"),
        ("\"Price\":7900", "\"Price\":0"),
        ("\"TriggerPrice\":8500", "\"TriggerPrice\":0"),
        ("\"txType\":17", "\"txType\":15"),
        ("\"modify\":{", "\"modify\":{\"Type\":2,"),
        ("\"modify\":{", "\"marketIndex\":1,\"modify\":{"),
    ] {
        let changed = text.replacen(from, to, 1);
        assert_ne!(changed, text);
        assert!(statement::derive(&changed, &owner).is_err(), "{to}");
    }
}
