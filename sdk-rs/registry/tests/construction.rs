use safe_cex_engine::{Engine, ErrorKind, Venue, VenueError};
use safe_cex_venues::{EngineOptions, Visit, build};

struct Accept;

impl Visit for Accept {
    type Output = ();
    fn visit<V: Venue + 'static>(self, _engine: Engine<V>) {}
}

fn construct(venue: &str, options: &str) -> Result<(), VenueError> {
    let options: EngineOptions = serde_json::from_str(options).expect("options decode");
    build(venue, options, Accept)
}

fn rejected(venue: &str, options: &str) -> String {
    let error = construct(venue, options).expect_err("construction is refused");
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput), "{venue}");
    error.message
}

#[test]
fn unknown_venues_are_refused_by_name() {
    assert_eq!(rejected("nope", "{}"), "unsupported venue: nope");
}

#[test]
fn private_only_accounts_cannot_own_a_direct_public_feed() {
    assert_eq!(
        rejected(
            "binance",
            r#"{"publicDataOnly":true,"privateOnly":true,"publicFeed":{"mode":"direct"}}"#
        ),
        "privateOnly cannot use direct public feeds"
    );
    construct("binance", r#"{"publicDataOnly":true,"privateOnly":true}"#).unwrap();
}

#[test]
fn secondary_read_credentials_need_a_key_and_secret() {
    assert_eq!(
        rejected(
            "binance",
            r#"{"key":"k","secret":"s","readCredentials":{"key":"read","secret":""}}"#
        ),
        "Secondary read credentials require a nonempty key and secret"
    );
    // AsterDEX reads its agent from `asterdex.readKey`/`readSecret`, so the uniform blob's copy is inert.
    construct(
        "asterdex",
        r#"{"publicDataOnly":true,"readCredentials":{"key":"read","secret":""}}"#,
    )
    .unwrap();
    // The SDK omits an absent secondary secret, which must not fail to parse.
    construct(
        "asterdex",
        r#"{"publicDataOnly":true,"readCredentials":{"key":"read"}}"#,
    )
    .unwrap();
    assert_eq!(
        rejected(
            "binance",
            r#"{"key":"k","secret":"s","readCredentials":{"key":"read"}}"#
        ),
        "Secondary read credentials require a nonempty key and secret"
    );
}

#[test]
fn passphrase_venues_refuse_private_accounts_without_one() {
    for (venues, message) in [
        (&["weex"][..], "WEEX requires a passphrase"),
        (
            &["bitget", "bitgetusdc", "bitgetinverse"][..],
            "Bitget requires a passphrase",
        ),
        (
            &["okx", "okxeu", "okxdemo", "okxeudemo"][..],
            "OKX requires a passphrase",
        ),
    ] {
        for venue in venues {
            assert_eq!(
                rejected(venue, r#"{"key":"k","secret":"s"}"#),
                message,
                "{venue}"
            );
            assert_eq!(
                rejected(venue, r#"{"key":"k","secret":"s","passphrase":""}"#),
                message,
                "{venue}"
            );
            construct(venue, r#"{"key":"k","secret":"s","passphrase":"p"}"#).unwrap();
            construct(venue, r#"{"publicDataOnly":true}"#).unwrap();
        }
    }
}

#[test]
fn kraken_refuses_a_secret_it_cannot_sign_with() {
    for venue in ["kraken", "krakenspot"] {
        let message = rejected(venue, r#"{"key":"k","secret":"not base64!"}"#);
        assert!(message.contains("base64"), "{venue}: {message}");
        construct(
            venue,
            r#"{"publicDataOnly":true,"key":"k","secret":"not base64!"}"#,
        )
        .unwrap();
    }
}

#[test]
fn hyperliquid_perpetuals_accept_the_spot_toggle() {
    construct(
        "hyperliquid",
        r#"{"publicDataOnly":true,"enableSpotMarkets":true}"#,
    )
    .unwrap();
    construct(
        "hyperliquidspot",
        r#"{"publicDataOnly":true,"enableSpotMarkets":true}"#,
    )
    .unwrap();
}

#[test]
fn txflow_requires_testnet() {
    assert_eq!(
        rejected("txflow", r#"{"publicDataOnly":true}"#),
        "TxFlow mainnet is offline; testnet is required"
    );
    construct("txflow", r#"{"publicDataOnly":true,"testnet":true}"#).unwrap();
}

#[test]
fn custom_endpoints_belong_to_hyrotrader_only() {
    let endpoints = r#""endpoints":{"rest":"https://hyro.test","publicWs":"wss://hyro.test/public","privateWs":"wss://hyro.test/private"}"#;
    assert_eq!(
        rejected(
            "bybit",
            &format!(r#"{{"publicDataOnly":true,{endpoints}}}"#)
        ),
        "Custom endpoints are supported only for HyroTrader"
    );
    assert_eq!(
        rejected("hyrotrader", r#"{"publicDataOnly":true}"#),
        "HyroTrader endpoints are not configured"
    );
    construct(
        "hyrotrader",
        &format!(r#"{{"publicDataOnly":true,{endpoints}}}"#),
    )
    .unwrap();
}

#[test]
fn log_level_accepts_the_four_levels_and_refuses_anything_else() {
    for level in ["debug", "info", "warn", "error"] {
        construct(
            "binance",
            &format!(r#"{{"publicDataOnly":true,"logLevel":"{level}"}}"#),
        )
        .unwrap();
    }
    assert_eq!(
        rejected("binance", r#"{"publicDataOnly":true,"logLevel":"warning"}"#),
        "logLevel must be debug, info, warn or error"
    );
}

const WALLET: &str = "0x1111111111111111111111111111111111111111";
const PRIVATE_KEY: &str = "0x2222222222222222222222222222222222222222222222222222222222222222";

#[test]
fn breakoutprop_needs_dxtrade_credentials_even_for_market_data() {
    let message = "BreakoutProp market data requires a legacy DXtrade username and password";
    for options in [
        r#"{"publicDataOnly":true}"#,
        r#"{"key":"user"}"#,
        r#"{"key":"user","secret":""}"#,
        r#"{"secret":"password"}"#,
    ] {
        assert_eq!(rejected("breakoutprop", options), message, "{options}");
    }
    construct(
        "breakoutprop",
        r#"{"publicDataOnly":true,"key":"user","secret":"password"}"#,
    )
    .unwrap();
}

#[test]
fn polymarket_private_accounts_need_a_wallet_key_and_coherent_l2_credentials() {
    let account =
        |polymarket: &str| format!(r#"{{"key":"{PRIVATE_KEY}","polymarket":{polymarket}}}"#);
    assert_eq!(
        rejected("polymarket", r#"{"key":"not-a-key"}"#),
        "Polymarket requires a local 32-byte wallet private key"
    );
    for partial in [
        r#"{"apiKey":"k"}"#,
        r#"{"apiKey":"k","apiSecret":"s","apiPassphrase":" "}"#,
    ] {
        assert_eq!(
            rejected("polymarket", &account(partial)),
            "Polymarket requires all three L2 API credentials together"
        );
    }
    assert_eq!(
        rejected("polymarket", &account(r#"{"signerType":4}"#)),
        "Invalid Polymarket signature type"
    );
    for funder in [r#"{"signerType":1}"#, r#"{"funderAddress":"0x12"}"#] {
        assert_eq!(
            rejected("polymarket", &account(funder)),
            "Polymarket requires a valid funder address for non-EOA signing"
        );
    }
    construct(
        "polymarket",
        &account(r#"{"apiKey":"k","apiSecret":"s","apiPassphrase":"p"}"#),
    )
    .unwrap();
    construct(
        "polymarket",
        &account(&format!(r#"{{"signerType":2,"funderAddress":"{WALLET}"}}"#)),
    )
    .unwrap();
    construct(
        "polymarket",
        &format!(r#"{{"key":"{}"}}"#, &PRIVATE_KEY[2..]),
    )
    .unwrap();
    construct("polymarket", r#"{"publicDataOnly":true}"#).unwrap();
}

#[test]
fn orderly_private_accounts_need_an_application_id_and_base58_ed25519_keys() {
    // 31 zero bytes: valid base58, one byte short of an Ed25519 key.
    let key = "1111111111111111111111111111111";
    let valid = "11111111111111111111111111111111";
    for venue in [
        "orderly",
        "tealstreet-dex",
        "kodiakfi",
        "mode",
        "salsadex",
        "what",
        "woofi",
    ] {
        for application in [
            r#""#,
            r#","applicationId":"""#,
            r#","applicationId":"bad/id""#,
        ] {
            assert_eq!(
                rejected(
                    venue,
                    &format!(r#"{{"key":"{valid}","secret":"{valid}"{application}}}"#)
                ),
                "Orderly private applicationId is required",
                "{venue} {application}"
            );
        }
        for (k, s) in [(key, valid), (valid, "not-base58!")] {
            assert_eq!(
                rejected(
                    venue,
                    &format!(r#"{{"key":"{k}","secret":"{s}","applicationId":"app_1-x"}}"#)
                ),
                "Orderly requires base58-encoded 32-byte key and secret",
                "{venue}"
            );
        }
        construct(
            venue,
            &format!(r#"{{"key":"ed25519:{valid}","secret":"{valid}","applicationId":"app_1-x"}}"#),
        )
        .unwrap();
        construct(venue, r#"{"publicDataOnly":true}"#).unwrap();
    }
}

#[test]
fn asterdex_v1_with_wallet_keys_needs_the_main_wallet() {
    let message = "Aster API Wallet/vault keys require the Main Wallet Address. Fill Main Wallet Address with your login wallet — do not replace the main wallet with the API/vault wallet.";
    let options = |extra: &str| {
        format!(
            r#"{{"key":"{WALLET}","secret":"{PRIVATE_KEY}","asterdex":{{"apiVersion":"v1"{extra}}}}}"#
        )
    };
    assert_eq!(rejected("asterdex", &options("")), message);
    assert_eq!(
        rejected("asterdex", &options(r#","userAddress":" ""#)),
        message
    );
    construct(
        "asterdex",
        &options(&format!(r#","userAddress":"{WALLET}""#)),
    )
    .unwrap();
    construct(
        "asterdex",
        &format!(r#"{{"key":"{WALLET}","secret":"{PRIVATE_KEY}","applicationId":"{WALLET}","asterdex":{{"apiVersion":"v1"}}}}"#),
    )
    .unwrap();
    construct(
        "asterdex",
        &format!(
            r#"{{"key":"{WALLET}","secret":"{PRIVATE_KEY}","asterdex":{{"apiVersion":"v3"}}}}"#
        ),
    )
    .unwrap();
    construct(
        "asterdex",
        r#"{"key":"hmac-key","secret":"hmac-secret","asterdex":{"apiVersion":"v1"}}"#,
    )
    .unwrap();
}

#[test]
fn locally_signing_venues_refuse_private_accounts_without_a_key_and_secret() {
    let message = "Rust engine private accounts require a local API key and secret; remote signing is unsupported";
    for venue in [
        "binance",
        "bybit",
        "okx",
        "orderly",
        "woo",
        "hyperliquidspot",
        "kraken",
        "bitunix",
    ] {
        for options in [
            r#"{"remoteSigning":true}"#,
            r#"{"remoteSigning":true,"key":"k"}"#,
            r#"{"remoteSigning":true,"key":"","secret":"s"}"#,
        ] {
            assert_eq!(rejected(venue, options), message, "{venue} {options}");
        }
        // Incomplete credentials without a remote signer build a public account, as before
        // (Orderly's own rules still apply to its private accounts).
        if venue != "orderly" {
            construct(venue, r#"{"key":"k"}"#).unwrap();
        }
    }
    // Delegated and derived signers have their own rules.
    for venue in [
        "hyperliquid",
        "paradex",
        "lighter",
        "coinbase",
        "polymarket",
        "breakoutprop",
    ] {
        assert_ne!(
            construct(venue, r#"{"remoteSigning":true}"#)
                .err()
                .map(|error| error.message),
            Some(message.into()),
            "{venue}"
        );
    }
    // AsterDEX signs with the wallet private key, from auth or the secret.
    construct(
        "asterdex",
        r#"{"remoteSigning":true,"key":"k","asterdex":{"privateKey":"p"}}"#,
    )
    .unwrap();
    assert_eq!(
        rejected(
            "asterdex",
            r#"{"remoteSigning":true,"key":"k","secret":"  "}"#
        ),
        message
    );
}

#[test]
fn aster_api_resolution_preserves_override_and_trimmed_auth_precedence() {
    let wallet = "0x1111111111111111111111111111111111111111";
    let private = "12".repeat(32);
    let flat = format!(r#""key":"{wallet}","secret":"{private}","applicationId":"{wallet}""#);
    for (fields, version) in [
        (String::new(), "v1"),
        (flat.clone(), "v3"),
        (format!(r#"{flat},"asterdex":{{"apiVersion":"v1"}}"#), "v1"),
        (r#""asterdex":{"apiVersion":"v3"}"#.into(), "v3"),
        (
            format!(r#"{flat},"asterdex":{{"userAddress":" ","privateKey":" "}}"#),
            "v3",
        ),
        (
            format!(r#"{flat},"asterdex":{{"privateKey":"invalid"}}"#),
            "v1",
        ),
        (
            format!(
                r#""asterdex":{{"userAddress":" {wallet} ","signerAddress":" {wallet} ","privateKey":" {private} "}}"#
            ),
            "v3",
        ),
        (
            format!(r#"{flat},"asterdex":{{"signerAddress":"invalid"}}"#),
            "v1",
        ),
    ] {
        let options: EngineOptions = serde_json::from_str(&format!("{{{fields}}}")).unwrap();
        assert_eq!(
            options.construction("asterdex").asterdex_api_version,
            Some(version),
            "{fields}"
        );
        assert_eq!(
            serde_json::to_string(&options.construction("binance")).unwrap(),
            "{}"
        );
    }
    // An inferred V3 reaches the same registry path as an explicit V3.
    construct("asterdex", &format!("{{{flat}}}")).unwrap();
}

#[test]
fn sdk_public_mode_is_resolved_before_native_credential_validation() {
    // Derived public input disagrees with raw explicit private intent.
    assert_eq!(
        rejected(
            "paradex",
            r#"{"publicDataOnly":true,"publicMode":{"requested":false}}"#
        ),
        "Rust Paradex private accounts require a key, Paradex address and subkey address"
    );
    construct(
        "paradex",
        r#"{"publicDataOnly":false,"publicMode":{"requested":true}}"#,
    )
    .unwrap();
    // Legacy harness options keep their explicit native interpretation.
    construct("paradex", r#"{"publicDataOnly":true}"#).unwrap();
    // An automatic public fallback skips the private passphrase check.
    construct(
        "weex",
        r#"{"publicDataOnly":false,"key":"k","publicMode":{}}"#,
    )
    .unwrap();
    assert_eq!(
        rejected(
            "weex",
            r#"{"publicDataOnly":true,"key":"k","secret":"s","publicMode":{"requested":false}}"#
        ),
        "WEEX requires a passphrase"
    );
}

#[test]
fn raw_public_mode_distinguishes_remote_and_payload_signers() {
    let resolve = |venue, fields: &str| {
        let options: EngineOptions = serde_json::from_str(fields).unwrap();
        let result = options.construction(venue);
        (result.can_sign.unwrap(), result.public_data_only.unwrap())
    };
    assert_eq!(
        resolve(
            "asterdex",
            r#"{"key":"k","publicMode":{"remoteSigner":true}}"#
        ),
        (true, false)
    );
    assert_eq!(
        resolve(
            "asterdex",
            r#"{"key":"k","publicMode":{"payloadSigner":true}}"#
        ),
        (false, true)
    );
    assert_eq!(
        resolve(
            "lighter",
            r#"{"publicMode":{"remoteSigner":true,"apiKeyIndexPresent":true}}"#
        ),
        (false, false)
    );
    assert_eq!(
        resolve(
            "lighter",
            r#"{"publicMode":{"payloadSigner":true,"apiKeyIndexPresent":true}}"#
        ),
        (false, true)
    );
    assert_eq!(
        resolve("hyperliquid", r#"{"vault":"v","publicMode":{}}"#),
        (false, false)
    );
    assert_eq!(
        resolve("hyperliquidspot", r#"{"vault":"v","publicMode":{}}"#),
        (false, true)
    );
    assert_eq!(
        resolve("coinbase", r#"{"key":"k","publicMode":{"requested":true}}"#),
        (true, true)
    );
    assert_eq!(resolve("txflow", r#"{"publicMode":{}}"#), (false, false));
}
