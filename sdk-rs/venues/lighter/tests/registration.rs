use safe_cex_engine::{Method, RequestId};
use safe_cex_venue_lighter::registration::{Network, decode, request};

fn row(account: u64, index: u8, public: &str) -> serde_json::Value {
    serde_json::json!({"account_index":account,"api_key_index":index,"public_key":public,"nonce":0,"transaction_time":0})
}
fn response(rows: Vec<serde_json::Value>) -> Vec<u8> {
    serde_json::json!({"code":200,"api_keys":rows})
        .to_string()
        .into_bytes()
}

#[test]
fn registered_key_is_exact_canonical_and_unambiguous() {
    let key = safe_cex_crypto::lighter::public_key(&[3; 40]).unwrap();
    let hex = safe_cex_crypto::hex(&key);
    for public in [hex.clone(), format!("0x{hex}")] {
        assert_eq!(
            decode(&response(vec![row(12, 4, &public)]), 12, 4).unwrap(),
            key
        );
    }
    for rows in [
        vec![],
        vec![row(13, 4, &hex)],
        vec![row(12, 5, &hex)],
        vec![row(12, 4, &hex), row(12, 4, &hex)],
        vec![row(12, 4, "00")],
        vec![row(12, 4, &"00".repeat(40))],
        vec![row(12, 4, &"ff".repeat(40))],
    ] {
        assert!(decode(&response(rows), 12, 4).is_err());
    }
    let body = response(vec![row(12, 4, &hex)]);
    assert!(decode(&body, 12, 255).is_err());
    assert!(decode(&body, 12, 4).is_ok());
    assert!(
        decode(
            &String::from_utf8(body)
                .unwrap()
                .replace("\"code\":200", "\"code\":400")
                .into_bytes(),
            12,
            4
        )
        .is_err()
    );
    assert!(decode(&vec![b' '; 16_385], 12, 4).is_err());
    assert!(decode(br#"{"code":200,"code":400,"api_keys":[]}"#, 12, 4).is_err());
}

#[test]
fn registration_requests_keep_the_existing_network_origins_and_do_not_sign() {
    for (network, origin) in [
        (Network::Mainnet, "https://mainnet.zklighter.elliot.ai"),
        (Network::Testnet, "https://testnet-api.lighter.xyz"),
        (Network::Robinhood, "https://api.rh.lighter.xyz"),
        (
            Network::RobinhoodTestnet,
            "https://api.rh-testnet.lighter.xyz",
        ),
    ] {
        let request = request(network, 12, 4).unwrap();
        assert_eq!(request.id, RequestId(1));
        assert_eq!(request.method, Method::Get);
        assert_eq!(
            request.url,
            format!("{origin}/api/v1/apikeys?account_index=12&api_key_index=4")
        );
        assert!(
            request.body.is_none()
                && request.headers.is_empty()
                && request.signer_headers.is_empty()
        );
    }
    assert!(request(Network::Mainnet, 12, 255).is_err());
    assert!(request(Network::Mainnet, u64::MAX, 4).is_err());
    assert!(request(Network::Mainnet, (1 << 48) - 1, 4).is_err());
}
