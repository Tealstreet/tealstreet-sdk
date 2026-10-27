//! Provider-signed Paradex hashes and signatures against the independent
//! starknet.js / ethers oracle, for both key forms on both networks.
mod support;
use safe_cex_engine::signing::{Algorithm, Operation, PayloadPart, SignRequest};
use safe_cex_venue_paradex::Options;
use safe_cex_venue_paradex::sign::dispatch::Preimage;
use safe_cex_venue_paradex::sign::{Credentials, SystemConfig, quantum};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    environment: String,
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

/// Trusted setup's provider and the account facts it seals, for `key`.
fn installed(
    key: &str,
    case: &Case,
) -> (
    safe_cex_engine::signing::Provider,
    Result<Credentials, String>,
) {
    let testnet = case.environment == "testnet";
    let (options, provider, identities) = support::keyed(
        Options {
            testnet,
            ..Options::default()
        },
        key,
    );
    let identity = identities.primary.unwrap();
    let public = identity
        .verified_public_key(Algorithm::StarkDigest, provider.incarnation())
        .unwrap();
    let credentials = Credentials::new(
        identity.signing_ref().unwrap().clone(),
        public,
        &case.address,
        &case.subkey,
        options.ethereum_key.then_some(testnet),
        &config(case),
    );
    (provider, credentials)
}

/// The provider's signature over `hash`, in the venue's wire text.
fn signed(provider: &safe_cex_engine::signing::Provider, hash: starknet_crypto::Felt) -> String {
    let request = SignRequest {
        receipt: Default::default(),
        context: Default::default(),
        operations: vec![Operation::Sign {
            algorithm: Algorithm::StarkDigest,
            key: safe_cex_engine::signing::KeyRef("primary.signing".into()),
            payload: vec![PayloadPart::Bytes {
                bytes: hash.to_bytes_be().to_vec(),
            }],
        }],
    };
    let value = provider
        .sign_ready(&request)
        .unwrap()
        .unwrap()
        .values
        .remove(0);
    Credentials::signature_text(
        value[..32].try_into().unwrap(),
        value[32..].try_into().unwrap(),
    )
}

#[test]
fn auth_orders_and_amendments_match_independent_starknet_js_for_both_environments() {
    for case in corpus() {
        let (provider, credentials) = installed(&case.key, &case);
        let credentials = credentials.unwrap();
        assert_eq!(credentials.address(), case.address);
        assert_eq!(credentials.subkey(), case.subkey);
        let timestamp = case.now / 1000;
        let hash = credentials.auth_hash(timestamp, timestamp + 300).unwrap();
        assert_eq!(format!("{hash:#x}"), case.auth.hash);
        assert_eq!(signed(&provider, hash), case.auth.signature);
        for order in &case.orders {
            let hash = credentials
                .order_hash(
                    case.now,
                    &Preimage {
                        market: &order.market,
                        side: &order.side,
                        kind: &order.kind,
                        size: &order.size,
                        price: order.price.as_deref(),
                        id: order.id.as_deref(),
                    },
                )
                .unwrap();
            assert_eq!(
                format!("{hash:#x}"),
                order.hash,
                "{} {}",
                case.chain,
                order.name
            );
            assert_eq!(
                signed(&provider, hash),
                order.signature,
                "{} {}",
                case.chain,
                order.name
            );
        }
    }
}

#[test]
fn ethereum_derivation_matches_ethers_and_vendored_sdk_grinding() {
    for case in corpus() {
        let (provider, credentials) = installed(&case.ethereum.key, &case);
        let credentials = credentials.unwrap();
        assert_eq!(credentials.address(), case.ethereum.address);
        let timestamp = case.now / 1000;
        let hash = credentials.auth_hash(timestamp, timestamp + 300).unwrap();
        assert_eq!(signed(&provider, hash), case.ethereum.auth.signature);
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
    for key in [
        format!("0x{}", "0".repeat(64)),
        format!("0x{}", "f".repeat(64)),
        format!("0x{}", "q".repeat(64)),
    ] {
        let error = safe_cex_crypto::starknet::parse_stark_key(&key)
            .err()
            .unwrap();
        assert!(!error.contains(&key));
    }
    let error =
        safe_cex_crypto::starknet::derive_from_ethereum("private-test-key-never-print", "1")
            .err()
            .unwrap();
    assert!(!error.contains("private-test-key-never-print"));
    let (_, credentials) = installed(&case.key, &case);
    let credentials = credentials.unwrap();
    for address in [
        "0x0",
        "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        "0x800000000000011000000000000000000000000000000000000000000000001",
        " 0x123",
        "-1",
    ] {
        let (_, refused) = installed(
            &case.key,
            &Case {
                address: address.into(),
                ..corpus().remove(0)
            },
        );
        assert!(refused.is_err(), "{address}");
    }
    let bad_side = Preimage {
        market: "BTC-USD-PERP",
        side: "BAD",
        kind: "LIMIT",
        size: "1",
        price: Some("2"),
        id: None,
    };
    assert!(credentials.order_hash(case.now, &bad_side).is_err());
}
