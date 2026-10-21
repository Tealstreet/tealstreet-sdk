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
