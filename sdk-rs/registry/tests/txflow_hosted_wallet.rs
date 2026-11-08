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

// Independent @msgpack/msgpack + ethers oracle; only the unvaulted order is a
// published venue golden. Every case cites its exact official .md field schema.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ParityCase {
    action: serde_json::Value,
    canonical_action: CanonicalAction,
    nonce: u64,
    vault_address: Option<String>,
    msgpack: String,
    preimage: String,
    connection_id: String,
    signing_hash: String,
    signature: String,
}
#[derive(serde::Deserialize)]
struct ParityFixture {
    cases: Vec<ParityCase>,
}
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(untagged)]
enum CanonicalAction {
    Order {
        grouping: String,
        orders: Vec<ParityOrder>,
    },
    Cancel {
        cancels: Vec<ParityCancel>,
    },
    Modify {
        order: ParityModify,
    },
    Leverage {
        asset: u32,
        leverage: i32,
    },
    Margin {
        asset: u32,
        #[serde(rename = "isCross")]
        is_cross: bool,
    },
    Mode {
        #[serde(rename = "isOneway")]
        is_oneway: bool,
    },
}
#[derive(serde::Deserialize, serde::Serialize)]
struct ParityOrder {
    a: u32,
    b: bool,
    p: String,
    s: String,
    r: bool,
    t: ParityLimit,
}
#[derive(serde::Deserialize, serde::Serialize)]
struct ParityLimit {
    limit: ParityTif,
}
#[derive(serde::Deserialize, serde::Serialize)]
struct ParityTif {
    tif: String,
}
#[derive(serde::Deserialize, serde::Serialize)]
struct ParityCancel {
    a: u32,
    o: u64,
}
#[derive(serde::Deserialize, serde::Serialize)]
struct ParityModify {
    o: u64,
    p: String,
    s: String,
}

fn unhex(value: &str) -> Vec<u8> {
    value
        .trim_start_matches("0x")
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

#[test]
fn every_agent_rest_action_matches_independent_canonical_and_signature_vectors() {
    let fixture: ParityFixture = serde_json::from_str(include_str!(
        "../../../packages/safe-cex/src/__tests__/fixtures/txflow/agent-action-parity.json"
    ))
    .unwrap();
    assert_eq!(fixture.cases.len(), 12);
    assert_eq!(
        safe_cex_crypto::hex(&sign::domain_separator()),
        "839dd579adc05b1545a37213db5dd3dc6928532ddf73dfa1b53f647421060a8f"
    );
    let (provider, identities) = device();
    for case in fixture.cases {
        let mut own = account();
        own.vault = case.vault_address.clone();
        let vault = case.vault_address.as_deref();
        let action = serde_json::value::to_raw_value(&case.action).unwrap();
        let statement = sign::statement::encode(&action, case.nonce, vault, OWNER).unwrap();
        let connection = sign::statement::connection_id(action.get(), vault, case.nonce).unwrap();
        let mut preimage = unhex(&case.msgpack);
        preimage.extend_from_slice(&case.nonce.to_be_bytes());
        preimage.push(u8::from(vault.is_some()));
        if let Some(vault) = vault {
            preimage.extend_from_slice(&unhex(vault));
        }
        assert_eq!(
            format!("0x{}", safe_cex_crypto::hex(&preimage)),
            case.preimage
        );
        assert_eq!(safe_cex_crypto::ethereum::keccak256(&preimage), connection);
        assert_eq!(
            format!("0x{}", safe_cex_crypto::hex(&connection)),
            case.connection_id
        );
        let digest = sign::statement::derive(&statement, OWNER, vault).unwrap();
        assert_eq!(
            format!("0x{}", safe_cex_crypto::hex(&digest)),
            case.signing_hash
        );
        let local = sign::sign(
            &case.canonical_action,
            vault,
            case.nonce,
            &format!("0x{}", "ab".repeat(32)),
        )
        .unwrap();
        assert_eq!(
            format!(
                "{}{}{:02x}",
                local.r,
                local.s.trim_start_matches("0x"),
                local.v
            ),
            case.signature
        );
        assert_eq!(
            hosted_wallet::sign(
                "txflow",
                &identities,
                &own,
                &provider,
                &statement,
                "txflow-agent",
                0.
            )
            .unwrap(),
            case.signature
        );
        // Mutation after signing changes the exact committed bytes.
        assert_ne!(
            sign::statement::connection_id(action.get(), vault, case.nonce + 1).unwrap(),
            connection
        );
    }
}
