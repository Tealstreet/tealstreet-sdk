use safe_cex_crypto::ethereum;
use safe_cex_venue_hyperliquid::sign;
use serde::Deserialize;

#[derive(Deserialize)]
struct Input {
    name: String,
    scalar: String,
    valid: bool,
}

#[test]
fn native_scalar_spelling_and_whitespace_are_pinned_before_setup_cutover() {
    let rows: Vec<Input> = serde_json::from_str(include_str!(
        "../../../../packages/safe-cex/src/exchanges/rust/hyperliquid-signer-setup.golden.json"
    ))
    .unwrap();
    let action = serde_json::json!({"type":"scheduleCancel"});
    let mut expected = None;
    for row in rows {
        let address = ethereum::private_key_address(&row.scalar);
        let signature = sign::sign(&action, None, 1234, true, &row.scalar);
        assert_eq!(address.is_ok(), row.valid, "address: {}", row.name);
        assert_eq!(signature.is_ok(), row.valid, "signature: {}", row.name);
        if row.valid {
            let value = (
                address.unwrap(),
                serde_json::to_value(signature.unwrap()).unwrap(),
            );
            if let Some(expected) = &expected {
                assert_eq!(&value, expected, "{}", row.name);
            } else {
                expected = Some(value);
            }
        }
    }
}
