use super::*;
use serde_json::{Value, json};

fn init(venue: &str, key: &str) -> InitPayload {
    serde_json::from_value(json!({"accountName":"desktop", "exchangeName":venue, "key":key}))
        .unwrap()
}

fn digest(init: &InitPayload) -> String {
    identity::digest("alice", init)
        .unwrap()
        .expect("hosted private identity")
}

#[test]
fn hosted_identity_covers_each_declared_remote_venue() {
    let rules: Value = serde_json::from_str(include_str!(
        "../../../../../packages/safe-cex/src/exchanges/rust/construction-rules.json"
    ))
    .unwrap();
    let wallets = [
        "hyperliquid",
        "hyperliquidspot",
        "grvt",
        "paradex",
        "lighter",
        "lighter-robinhood",
        "backpack",
    ];
    let mut checked = 0;
    for (venue, rule) in rules["venues"].as_object().unwrap() {
        if rule["remoteSigning"]["enabled"] != true || wallets.contains(&venue.as_str()) {
            continue;
        }
        let first = init(venue, "synthetic-key-A");
        let id = digest(&first);
        assert_eq!(id.len(), 64, "{venue}");
        let mut alias = first.clone();
        alias.account_name = "phone".into();
        assert_eq!(digest(&alias), id, "labels: {venue}");
        alias.key = "synthetic-key-B".into();
        assert_ne!(digest(&alias), id, "keys: {venue}");
        assert_ne!(
            identity::digest("bob", &first).unwrap().unwrap(),
            id,
            "owners: {venue}"
        );
        alias = first.clone();
        alias.testnet = Some(true);
        assert_ne!(digest(&alias), id, "networks: {venue}");
        alias = first.clone();
        alias.public_only = Some(true);
        assert!(
            identity::digest("", &alias).unwrap().is_none(),
            "public: {venue}"
        );
        checked += 1;
    }
    assert_eq!(checked, 32);
}

#[test]
fn hosted_identity_preserves_opaque_key_case_and_declared_trimming() {
    for venue in ["bitunix", "mexc", "bybit", "kraken", "coinbase", "woo"] {
        let first = init(venue, "synthetic-Key-A");
        let padded = init(venue, " synthetic-Key-A ");
        let trim = safe_cex_venues::signer_setup_policy(venue)
            .primary
            .and_then(|p| p.trim_credential)
            == Some(true);
        assert_eq!(digest(&first) == digest(&padded), trim, "{venue}");
        assert_ne!(
            digest(&first),
            digest(&init(venue, "synthetic-key-a")),
            "{venue}"
        );
    }
}

#[test]
fn hosted_identity_scopes_related_markets_and_separates_sites_and_demo() {
    for family in [
        &["bybit", "bybitinverse", "bybitspot"][..],
        &[
            "binance",
            "binancepm",
            "binanceinverse",
            "binancespot",
            "binanceoptions",
        ][..],
        &["bitget", "bitgetinverse", "bitgetusdc"][..],
        &["kucoin", "kucoinspot"][..],
        &["coinbase", "coinbasespot"][..],
        &["bingx", "bingxspot"][..],
    ] {
        let id = digest(&init(family[0], "synthetic-key"));
        for venue in family {
            assert_eq!(digest(&init(venue, "synthetic-key")), id, "{venue}");
        }
    }
    for (main, other) in [
        ("bybit", "bybitdemo"),
        ("okx", "okxeu"),
        ("okx", "okxdemo"),
        ("okxeu", "okxeudemo"),
    ] {
        assert_ne!(
            digest(&init(main, "synthetic-key")),
            digest(&init(other, "synthetic-key"))
        );
    }
}

#[test]
fn hosted_wallet_identity_requires_its_public_signing_binding() {
    for venue in [
        "hyperliquid",
        "hyperliquidspot",
        "grvt",
        "paradex",
        "lighter",
        "lighter-robinhood",
    ] {
        let first = init(venue, "synthetic-key");
        assert!(identity::digest("alice", &first).is_err(), "{venue}");
    }
}

#[test]
fn hosted_identity_settings_exclude_primary_credentials_for_every_hmac_venue() {
    for venue in [
        "bybitinverse",
        "binance",
        "okx",
        "bitget",
        "blofin",
        "kucoin",
        "phemex",
        "woo",
        "bitunix",
        "mexc",
        "kraken",
        "cryptocom",
        "bitrue",
        "apexomni",
        "coinbase",
    ] {
        let first = init(venue, "synthetic-key-A");
        let mut second = first.clone();
        second.key = "synthetic-key-B".into();
        second.passphrase = Some("synthetic-passphrase".into());
        assert_eq!(first.fingerprint(), second.fingerprint(), "{venue}");
    }
}

#[test]
fn hosted_hyperliquid_identity_uses_signer_and_main_or_vault_scope() {
    for venue in ["hyperliquid", "hyperliquidspot"] {
        let first: InitPayload = serde_json::from_value(json!({
            "accountName":"desktop", "exchangeName":venue,
            "key":"0x1111111111111111111111111111111111111111",
            "signerAddress":"0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        }))
        .unwrap();
        let original = digest(&first);
        let mut alias = serde_json::to_value(&first).unwrap();
        alias["signerAddress"] = "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into();
        assert_eq!(digest(&serde_json::from_value(alias).unwrap()), original);
        for (field, value) in [
            (
                "signerAddress",
                "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            ),
            ("key", "0x2222222222222222222222222222222222222222"),
            ("vault", "0x3333333333333333333333333333333333333333"),
        ] {
            let mut changed = serde_json::to_value(&first).unwrap();
            changed[field] = value.into();
            assert_ne!(
                digest(&serde_json::from_value(changed).unwrap()),
                original,
                "{field}"
            );
        }
    }
}

#[test]
fn hosted_grvt_identity_uses_trading_account_and_signer() {
    let first: InitPayload = serde_json::from_value(json!({
        "accountName":"desktop", "exchangeName":"grvt", "key":"synthetic-login-A",
        "grvtTradingAccountId":"00042", "grvtSignerAddress":"0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    })).unwrap();
    let original = digest(&first);
    let mut same = first.clone();
    same.grvt_trading_account_id = Some("42".into());
    same.grvt_signer_address = Some("0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into());
    same.key = "synthetic-login-B".into();
    assert_eq!(digest(&same), original);
    same.grvt_trading_account_id = Some("43".into());
    assert_ne!(digest(&same), original);
    same = first.clone();
    same.grvt_signer_address = Some("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into());
    assert_ne!(digest(&same), original);
}

#[test]
fn hosted_paradex_identity_uses_the_account_and_stark_public_key() {
    let mut scalar = [0; 32];
    scalar[31] = 7;
    let public = safe_cex_crypto::hex(&safe_cex_crypto::starknet::public_key_bytes(&scalar));
    let first: InitPayload = serde_json::from_value(json!({
        "accountName":"desktop", "exchangeName":"paradex", "key":"synthetic-public-selector",
        "paradexAddress":"0xabc", "paradexPublicKey":format!("0x{public}")
    }))
    .unwrap();
    let original = digest(&first);
    let mut same = serde_json::to_value(&first).unwrap();
    same["paradexAddress"] = format!("0x{:0>64}", "ABC").into();
    same["paradexPublicKey"] = format!("0x{}", public.to_uppercase()).into();
    assert_eq!(digest(&serde_json::from_value(same).unwrap()), original);
    let mut other = first.clone();
    other.paradex_address = Some("0xabd".into());
    assert_ne!(digest(&other), original);
    scalar[31] = 8;
    let mut other = serde_json::to_value(&first).unwrap();
    other["paradexPublicKey"] = format!(
        "0x{}",
        safe_cex_crypto::hex(&safe_cex_crypto::starknet::public_key_bytes(&scalar))
    )
    .into();
    assert_ne!(digest(&serde_json::from_value(other).unwrap()), original);
}

#[test]
fn hosted_backpack_identity_uses_validated_public_key_bytes() {
    use base64::Engine as _;
    let public = |n| {
        base64::engine::general_purpose::STANDARD
            .encode(safe_cex_crypto::ed25519::public_key(&[n; 32]).unwrap())
    };
    let first = init("backpack", &public(7));
    assert_ne!(digest(&first), digest(&init("backpack", &public(8))));
    for key in [
        "",
        "synthetic-invalid",
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
    ] {
        assert!(identity::digest("alice", &init("backpack", key)).is_err());
    }
}
