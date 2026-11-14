use safe_cex_venue_txflow::sign::{agent_digest, statement};
use serde_json::json;

const USER: &str = "0x1111111111111111111111111111111111111111";

#[test]
fn device_rebuilds_the_published_digest_and_refuses_other_authority() {
    let order = json!({"type":"order", "grouping":"na", "orders":[{
        "a":1,"b":true,"p":"50000.0","s":"0.01","r":false,"t":{"limit":{"tif":"Gtc"}}
    }]});
    let descriptor = json!({"kind":"txflow/1", "user":USER,"action":order,
        "nonce":1700000000000u64,"testnet":true})
    .to_string();
    assert_eq!(
        statement::derive(&descriptor, USER, None).unwrap(),
        agent_digest(hex_literal(
            "6546d1c3acf430a28a0b2552c66e985f6543646660874734732ae9e9daf4d806"
        ))
    );
    assert_ne!(
        statement::derive(&descriptor, USER, None).unwrap(),
        statement::derive(&descriptor.replace("50000.0", "50000"), USER, None).unwrap(),
        "price strings are hashed exactly as written"
    );
    assert!(
        statement::derive(
            &descriptor,
            "0x2222222222222222222222222222222222222222",
            None
        )
        .is_err()
    );
    assert!(
        statement::derive(
            &descriptor.replace("\"testnet\":true", "\"testnet\":false"),
            USER,
            None
        )
        .is_err()
    );
    for action in ["withdraw3", "tokenSend", "vaultTransfer", "approveAgent"] {
        let bad = json!({"kind":"txflow/1","user":USER,"action":{"type":action},"nonce":7,"testnet":true});
        assert!(statement::derive(&bad.to_string(), USER, None).is_err());
    }
}

fn hex_literal(s: &str) -> [u8; 32] {
    std::array::from_fn(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap())
}
