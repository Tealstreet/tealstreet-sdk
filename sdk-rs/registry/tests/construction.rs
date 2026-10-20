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
            r#"{"privateOnly":true,"publicFeed":{"mode":"direct"}}"#
        ),
        "privateOnly cannot use direct public feeds"
    );
    construct("binance", r#"{"privateOnly":true}"#).unwrap();
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
fn hyperliquid_perpetuals_refuse_the_spot_toggle() {
    assert_eq!(
        rejected("hyperliquid", r#"{"enableSpotMarkets":true}"#),
        "Rust Hyperliquid currently supports perpetual markets only"
    );
    construct("hyperliquidspot", r#"{"enableSpotMarkets":true}"#).unwrap();
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
        rejected("bybit", &format!("{{{endpoints}}}")),
        "Custom endpoints are supported only for HyroTrader"
    );
    assert_eq!(
        rejected("hyrotrader", "{}"),
        "HyroTrader endpoints are not configured"
    );
    construct("hyrotrader", &format!("{{{endpoints}}}")).unwrap();
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
    let key = "5HueCGU8rMjxEXxiPuD5BDku4MkFqeZyd4dZ1jvhTVqvbTLvyTJ";
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
        for (k, s) in [(key, valid), (valid, "not-base58!"), ("", valid)] {
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
