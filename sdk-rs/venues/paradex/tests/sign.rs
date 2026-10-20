use safe_cex_venue_paradex::sign::{Credentials, SystemConfig, quantum};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    chain: String,
    eth_chain: String,
    key: String,
    address: String,
    subkey: String,
    now: u64,
    #[serde(rename = "shortAuth")]
    auth: Signed,
    orders: Vec<Order>,
    ethereum: Ethereum,
}

#[derive(Deserialize)]
struct Signed {
    signature: String,
    hash: String,
}

#[derive(Deserialize)]
struct Order {
    name: String,
    market: String,
    side: String,
    #[serde(rename = "type")]
    kind: String,
    size: String,
    price: Option<String>,
    id: Option<String>,
    signature: String,
    hash: String,
}

#[test]
fn delegated_hashes_match_independent_starknet_js_without_a_private_key() {
    for case in corpus() {
        let credentials = Credentials::delegated(
            "opaque-public-key",
            &case.address,
            &case.subkey,
            &config(&case),
        )
        .unwrap();
        assert!(credentials.is_delegated());
        let auth = credentials.auth(case.now).unwrap();
        assert_eq!(auth.signature, case.auth.hash);
        let request = credentials.payload_request(&auth.signature).unwrap();
        assert_eq!(request.key, "opaque-public-key");
        assert_eq!(request.exchange_name, "paradex");
        assert_eq!(request.scheme, "stark");
        assert_eq!(request.encoding, "felt-pair");
        for order in case.orders {
            let signed = if let Some(id) = &order.id {
                credentials.modify(
                    case.now,
                    &order.market,
                    &order.side,
                    &order.kind,
                    &order.size,
                    order.price.as_deref().unwrap(),
                    id,
                )
            } else {
                credentials.order(
                    case.now,
                    &order.market,
                    &order.side,
                    &order.kind,
                    &order.size,
                    order.price.as_deref(),
                )
            }
            .unwrap();
            assert_eq!(
                signed.signature, order.hash,
                "{} {}",
                case.chain, order.name
            );
        }
    }
}

#[derive(Deserialize)]
struct Ethereum {
    key: String,
    address: String,
    #[serde(rename = "shortAuth")]
    auth: Signed,
}

fn corpus() -> Vec<Case> {
    #[derive(Deserialize)]
    struct Corpus {
        cases: Vec<Case>,
    }
    serde_json::from_str::<Corpus>(include_str!("sign-vectors.json"))
        .unwrap()
        .cases
}

fn config(case: &Case) -> SystemConfig {
    SystemConfig {
        starknet_chain_id: case.chain.clone(),
        l1_chain_id: Some(case.eth_chain.clone()),
        paraclear_account_hash: Some("0x123".into()),
        paraclear_account_proxy_hash: Some("0x456".into()),
    }
}

#[test]
fn auth_orders_and_amendments_match_independent_starknet_js_for_both_environments() {
    for case in corpus() {
        let credentials =
            Credentials::new(&case.key, &case.address, &case.subkey, &config(&case)).unwrap();
        assert_eq!(credentials.address(), case.address);
        assert_eq!(credentials.subkey(), case.subkey);
        let auth = credentials.auth(case.now).unwrap();
        assert_eq!(auth.signature, case.auth.signature);
        assert_eq!(auth.timestamp, 1717171717);
        assert_eq!(auth.expiration, 1717172017);
        assert!(
            auth.headers(credentials.address())
                .iter()
                .any(|(name, value)| *name == "PARADEX-STARKNET-ACCOUNT" && value == &case.address)
        );
        for order in &case.orders {
            let result = if let Some(id) = &order.id {
                credentials.modify(
                    case.now,
                    &order.market,
                    &order.side,
                    &order.kind,
                    &order.size,
                    order.price.as_deref().unwrap(),
                    id,
                )
            } else {
                credentials.order(
                    case.now,
                    &order.market,
                    &order.side,
                    &order.kind,
                    &order.size,
                    order.price.as_deref(),
                )
            }
            .unwrap();
            assert_eq!(
                result.signature, order.signature,
                "{} {}",
                case.chain, order.name
            );
            assert_eq!(result.signature_timestamp, case.now);
        }
    }
}

#[test]
fn ethereum_derivation_matches_ethers_and_vendored_sdk_grinding() {
    for case in corpus() {
        let credentials = Credentials::new(
            &case.ethereum.key,
            &case.address,
            &case.subkey,
            &config(&case),
        )
        .unwrap();
        assert_eq!(credentials.address(), case.ethereum.address);
        assert_eq!(
            credentials.auth(case.now).unwrap().signature,
            case.ethereum.auth.signature
        );
    }
}

#[test]
fn decimal_quantum_is_exact_and_truncates_toward_zero() {
    for (input, output) in [
        ("0.00000999", "999"),
        ("1.234567899", "123456789"),
        ("1e-8", "1"),
        ("999e-20", "0"),
        (".1", "10000000"),
        ("1.", "100000000"),
        ("0", "0"),
        ("1.234e3", "123400000000"),
        ("+0001.23", "123000000"),
    ] {
        assert_eq!(quantum(input).unwrap(), output, "{input}");
    }
    for input in [
        "NaN",
        "Infinity",
        "-1",
        "-0",
        ".",
        "",
        "1e999999999999",
        "1e100",
        "0x10",
        "1.2.3",
        "1e2e3",
    ] {
        assert!(quantum(input).is_err(), "{input}");
    }
}

#[test]
fn invalid_credentials_fail_without_revealing_secret_material() {
    let case = corpus().remove(0);
    let config = config(&case);
    for key in [
        format!("0x{}", "0".repeat(64)),
        format!("0x{}", "f".repeat(64)),
        format!("0x{}", "q".repeat(64)),
        "private-test-key-never-print".into(),
    ] {
        let error = Credentials::new(&key, &case.address, &case.subkey, &config)
            .err()
            .unwrap();
        assert!(!error.contains(&key));
    }
    for address in [
        "0x0",
        "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        "0x800000000000011000000000000000000000000000000000000000000000001",
        " 0x123",
        "-1",
    ] {
        assert!(Credentials::new(&case.key, address, &case.subkey, &config).is_err());
    }
    let credentials = Credentials::new(&case.key, &case.address, &case.subkey, &config).unwrap();
    assert!(
        credentials
            .order(case.now, "BTC-USD-PERP", "BAD", "LIMIT", "1", Some("2"))
            .is_err()
    );
    assert!(
        credentials
            .modify(case.now, "BTC-USD-PERP", "BUY", "LIMIT", "1", "2", "")
            .is_err()
    );
}
