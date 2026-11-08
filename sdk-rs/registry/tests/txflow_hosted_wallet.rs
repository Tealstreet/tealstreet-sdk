#![cfg(feature = "txflow")]
use safe_cex_venue_txflow::sign;
use safe_cex_venues::{
    EngineOptions,
    hosted_wallet::{self, Account},
};
use serde_json::json;
const OWNER: &str = "0x1111111111111111111111111111111111111111";
fn device() -> (
    safe_cex_engine::signing::Provider,
    safe_cex_engine::signing::Identities,
) {
    let mut options: EngineOptions = serde_json::from_value(json!({"testnet":true,"applicationId":OWNER,"key":OWNER,"secret":format!("0x{}", "ab".repeat(32))})).unwrap();
    let output = safe_cex_venues::prepare_local_signer_for("txflow", &mut options).unwrap();
    assert!(options.secret.is_none());
    output
}
fn account() -> Account {
    Account {
        key: Some(OWNER.into()),
        application_id: Some(OWNER.into()),
        testnet: true,
        ..Default::default()
    }
}
#[test]
fn txflow_hosted_and_local_signing_match_the_published_vector() {
    let (provider, ids) = device();
    let action = r#"{"type":"order","grouping":"na","orders":[{"a":1,"b":true,"p":"50000.0","s":"0.01","r":false,"t":{"limit":{"tif":"Gtc"}}}]}"#;
    let raw = serde_json::value::RawValue::from_string(action.into()).unwrap();
    let statement = sign::statement::encode(&raw, 1700000000000, None, OWNER).unwrap();
    let signature = hosted_wallet::sign(
        "txflow",
        &ids,
        &account(),
        &provider,
        &statement,
        "txflow-agent",
        0.,
    )
    .unwrap();
    assert_eq!(
        signature,
        "0x4064848dd881985c52dacdc00f7046f1010f7f370e34620d36fcf3b2d1ecb3e025c45ad67b9506ac653fc3658f47aa8b6002bfa86a31ae619b334d71aa8b4f6f1b"
    );
    assert_eq!(
        hosted_wallet::sign(
            "txflow",
            &ids,
            &account(),
            &provider,
            OWNER,
            "txflow-address",
            0.
        )
        .unwrap(),
        "0xe239cdc5fbe977a8a141b72194d3cf8c41bc5bc6"
    );
}
#[test]
fn hosted_device_rejects_mainnet_foreign_owner_vault_and_owner_actions() {
    let (provider, ids) = device();
    let own = account();
    let statement = json!({"kind":"txflow/1","user":OWNER,"testnet":true,"nonce":1700000000000u64,"action":{"type":"cancel","cancels":[{"a":1,"o":1}]}});
    let attempt = |value: &serde_json::Value, a: &Account, format: &str| {
        hosted_wallet::sign("txflow", &ids, a, &provider, &value.to_string(), format, 0.)
    };
    assert!(attempt(&statement, &own, "txflow-agent").is_ok());
    let mut mainnet = own.clone();
    mainnet.testnet = false;
    assert!(attempt(&statement, &mainnet, "txflow-agent").is_err());
    for action in ["withdraw3", "tokenSend", "vaultTransfer", "approveAgent"] {
        let mut value = statement.clone();
        value["action"] = json!({"type":action});
        assert!(attempt(&value, &own, "txflow-agent").is_err());
    }
    let mut value = statement.clone();
    value["user"] = json!("0x2222222222222222222222222222222222222222");
    assert!(attempt(&value, &own, "txflow-agent").is_err());
    let mut value = statement.clone();
    value["vaultAddress"] = json!(OWNER);
    assert!(attempt(&value, &own, "txflow-agent").is_err());
    assert!(attempt(&statement, &own, "hyperliquid-l1").is_err());
}
