//! Independent vectors from https://docs.txflow-testnet.xyz/api/signing.md.
use safe_cex_crypto::{ethereum, hex};
use safe_cex_venue_txflow::sign::{action_hash, agent_digest, domain_separator, sign};
use serde::Serialize;

#[derive(Serialize)]
struct Action<'a> {
    grouping: &'a str,
    orders: [Order<'a>; 1],
}
#[derive(Serialize)]
struct Order<'a> {
    a: u32,
    b: bool,
    p: &'a str,
    s: &'a str,
    r: bool,
    t: Limit<'a>,
}
#[derive(Serialize)]
struct Limit<'a> {
    limit: Tif<'a>,
}
#[derive(Serialize)]
struct Tif<'a> {
    tif: &'a str,
}
fn action() -> Action<'static> {
    Action {
        grouping: "na",
        orders: [Order {
            a: 1,
            b: true,
            p: "50000.0",
            s: "0.01",
            r: false,
            t: Limit {
                limit: Tif { tif: "Gtc" },
            },
        }],
    }
}
fn typed_hash(raw: &serde_json::Value) -> [u8; 32] {
    serde_json::from_value::<alloy_dyn_abi::TypedData>(raw.clone())
        .unwrap()
        .eip712_signing_hash()
        .unwrap()
        .into()
}
fn vector_key() -> String {
    format!("0x{}", "ab".repeat(32))
}

#[test]
fn official_order_preimage_domain_digest_and_signature() {
    assert_eq!(
        hex(&domain_separator()),
        "839dd579adc05b1545a37213db5dd3dc6928532ddf73dfa1b53f647421060a8f"
    );
    let hash = action_hash(&action(), None, 1_700_000_000_000).unwrap();
    assert_eq!(
        hex(&hash),
        "6546d1c3acf430a28a0b2552c66e985f6543646660874734732ae9e9daf4d806"
    );
    assert_eq!(
        hex(&agent_digest(hash)),
        "eae1ae63709636fb9567accd33bc5dd2717bfac4d97e9a853f91c7fb391e1cc6"
    );
    let signature = sign(&action(), None, 1_700_000_000_000, &vector_key()).unwrap();
    assert_eq!(
        signature.r,
        "0x4064848dd881985c52dacdc00f7046f1010f7f370e34620d36fcf3b2d1ecb3e0"
    );
    assert_eq!(
        signature.s,
        "0x25c45ad67b9506ac653fc3658f47aa8b6002bfa86a31ae619b334d71aa8b4f6f"
    );
    assert_eq!(signature.v, 27);
}

#[test]
fn fixed_agent_words_match_dynamic_eip712() {
    for hash in [[0; 32], [255; 32], core::array::from_fn(|n| n as u8)] {
        let typed = serde_json::json!({"domain":{"name":"TxFlow-Testnet","version":"1","chainId":2511,"verifyingContract":"0x0000000000000000000000000000000000000000"},"types":{"Agent":[{"name":"txflowNetwork","type":"string"},{"name":"chainId","type":"uint32"},{"name":"apiVersion","type":"uint32"},{"name":"connectionId","type":"bytes32"}]},"primaryType":"Agent","message":{"txflowNetwork":"TxFlow-Testnet","chainId":2511,"apiVersion":1,"connectionId":format!("0x{}",hex(&hash))}});
        assert_eq!(
            format!("0x{}", hex(&agent_digest(hash))),
            format!("0x{}", hex(&typed_hash(&typed)))
        );
    }
}

#[test]
fn official_account_approval_uses_separate_schema() {
    // Crypto can verify the account-key vector; the trading connector never approves agents.
    let typed = serde_json::json!({"domain":{"name":"TxFlow-Testnet","version":"1","chainId":2511,"verifyingContract":"0x0000000000000000000000000000000000000000"},"types":{"ApproveAgent":[{"name":"txflowNetwork","type":"string"},{"name":"chainId","type":"uint32"},{"name":"apiVersion","type":"uint32"},{"name":"agentAddress","type":"address"},{"name":"agentName","type":"string"},{"name":"nonce","type":"uint64"}]},"primaryType":"ApproveAgent","message":{"txflowNetwork":"TxFlow-Testnet","chainId":2511,"apiVersion":1,"agentAddress":"0x0202020202020202020202020202020202020202","agentName":"my-trading-bot","nonce":2000}});
    assert_eq!(
        format!("0x{}", hex(&typed_hash(&typed))),
        "0x1df22801b48ae1402ad36b244aa245fd6e842fa5fef2c40819b1e6c661cd2212"
    );
    assert_eq!(
        ethereum::sign_digest(&typed_hash(&typed), &vector_key()).unwrap(),
        "0x4431d4afb9fcd0dcd619afd55a7044cc26cb0ee1d436a1b06d836da217842ead54a0755ee3d8c241235a1828a2be732d72e9c152b5de25fc7c169966de4210f71b"
    );
}

#[test]
fn exact_fields_nonce_and_vault_separate_messages() {
    let base = action_hash(&action(), None, 7).unwrap();
    assert_ne!(base, action_hash(&action(), None, 8).unwrap());
    let mut changed = action();
    changed.orders[0].p = "50000";
    assert_ne!(base, action_hash(&changed, None, 7).unwrap());
    assert_ne!(
        base,
        action_hash(
            &action(),
            Some("0x0000000000000000000000000000000000000000"),
            7
        )
        .unwrap()
    );
    assert!(action_hash(&action(), Some("invalid"), 7).is_err());
    #[derive(Serialize)]
    struct Tagged<'a> {
        r#type: &'a str,
        #[serde(flatten)]
        action: Action<'a>,
    }
    assert_ne!(
        base,
        action_hash(
            &Tagged {
                r#type: "order",
                action: action()
            },
            None,
            7
        )
        .unwrap()
    );
    assert!(sign(&action(), None, 7, "invalid").is_err());
    assert_eq!(
        ethereum::private_key_address(&vector_key())
            .unwrap()
            .to_lowercase(),
        "0xe239cdc5fbe977a8a141b72194d3cf8c41bc5bc6"
    );
}
