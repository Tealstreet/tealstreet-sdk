use safe_cex_engine::{Engine, ErrorKind, Venue, VenueError};
use safe_cex_venues::{EngineOptions, Visit, build};

struct Accept;

impl Visit for Accept {
    type Output = ();
    fn visit<V: Venue + 'static>(self, _engine: Engine<V>) {}
}

fn construct(venue: &str, options: &str) -> Result<(), VenueError> {
    let options: EngineOptions = serde_json::from_str(options).expect("options decode");
    build(
        venue,
        options,
        Some(safe_cex_engine::test_support::seeded_entropy(17)),
        Accept,
    )
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
        (&["weex"][..], "Invalid local signer setup"),
        (
            &["bitget", "bitgetusdc", "bitgetinverse"][..],
            "Invalid local signer setup",
        ),
        (
            &["okx", "okxeu", "okxdemo", "okxeudemo"][..],
            "Invalid local signer setup",
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
    // AsterDEX V3 signs with the auth wallet key; setup resolves V3 from its shape.
    let wallet = "0x1111111111111111111111111111111111111111";
    construct(
        "asterdex",
        &format!(
            r#"{{"remoteSigning":true,"key":"{wallet}","applicationId":"{wallet}","asterdex":{{"privateKey":"0x{}"}}}}"#,
            "12".repeat(32)
        ),
    )
    .unwrap();
    // A wallet key that is not V3-shaped resolves V1, which has no secret to sign with.
    assert_eq!(
        rejected(
            "asterdex",
            r#"{"remoteSigning":true,"key":"k","asterdex":{"privateKey":"p"}}"#
        ),
        message
    );
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
            r#"{"bridge":{"spot":false,"demo":false,"primaryCredentialsOnly":false,"payloadSigning":false,"apiKeyIndexRequired":false,"isolatedBookSubscriptions":false,"dispatchSigning":true}}"#
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
        "Invalid local signer setup"
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

#[test]
fn native_payload_refusal_preserves_classification_with_or_without_local_credentials() {
    use safe_cex_engine::Outcome;
    for venue in [
        "binance",
        "binancepm",
        "binanceinverse",
        "binancespot",
        "bybit",
        "asterdex",
        "coinbase",
        "hyperliquid",
        "lighter",
        "polymarket",
        "txflow",
    ] {
        for local in ["", "local"] {
            let error = construct(venue, &format!(r#"{{"key":"key","secret":"{local}","remoteSigning":true,"payloadSigner":true}}"#)).unwrap_err();
            assert_eq!(error.kind, Some(ErrorKind::Unsupported), "{venue}");
            assert_eq!(error.outcome, Some(Outcome::NotSent), "{venue}");
            let expected = if safe_cex_venues::remote_signing_policy(venue).enabled {
                format!(
                    "Rust {venue} payloadSigner requires a configured injected signing provider"
                )
            } else {
                format!(
                    "Rust {venue} does not support payloadSigner; remote signing is unsupported. Use the TS venue, or remove payloadSigner and configure local signing credentials."
                )
            };
            assert_eq!(error.message, expected);
        }
    }
    construct("paradex", r#"{"key":"public-subkey","paradexAddress":"0x1","subkeyAddress":"0x2","payloadSigner":true}"#).unwrap();
    construct("binance", r#"{"publicDataOnly":true,"payloadSigner":true}"#).unwrap();
    for venue in ["binance", "binancepm", "binanceinverse", "binancespot"] {
        // Explicit shared intent overrides the legacy boolean in either direction.
        construct(venue, r#"{"publicDataOnly":false,"payloadSigner":true,"publicMode":{"requested":true,"payloadSigner":true}}"#).unwrap();
        let error = construct(venue, r#"{"key":"key","secret":"unused","publicDataOnly":true,"payloadSigner":true,"publicMode":{"requested":false,"payloadSigner":true}}"#).unwrap_err();
        assert_eq!(error.kind, Some(ErrorKind::Unsupported));
        assert_eq!(error.outcome, Some(Outcome::NotSent));
    }
    // Aster and Lighter payload-only SDK accounts retain their public fallback.
    for venue in ["asterdex", "lighter"] {
        construct(venue, r#"{"key":"key","payloadSigner":true,"publicMode":{"payloadSigner":true,"apiKeyIndexPresent":true}}"#).unwrap();
    }
}

#[cfg(feature = "bybit")]
#[test]
fn hyro_raw_config_preserves_network_demo_and_missing_production_fallback() {
    use safe_cex_venues::HyroConfig;
    let config = |mode, production_rest: &str| {
        serde_json::json!({
            "mode": mode,
            "production": {
                "rest": {"livenet":production_rest,"testnet":"https://prod-test.example"},
                "publicWs": {"livenet":"wss://prod.example/public","testnet":"wss://prod-test.example/public"},
                "privateWs": {"livenet":"wss://prod.example/private","testnet":"wss://prod-test.example/private"}
            },
            "simulator": {
                "rest": {"livenet":"https://sim.example","testnet":"https://sim-test.example"},
                "publicWs": {"livenet":"wss://sim.example/public","testnet":"wss://sim-test.example/public"},
                "privateWs": {"livenet":"wss://sim.example/private","testnet":"wss://sim-test.example/private"}
            }
        })
    };
    for (mode, prod, testnet, expected) in [
        (None, "https://prod.example", false, "prod"),
        (None, "https://prod.example", true, "prod-test"),
        (Some("demo"), "https://prod.example", false, "sim"),
        (Some("demo"), "https://prod.example", true, "sim-test"),
        (None, "", false, "sim"),
        (None, "", true, "sim-test"),
    ] {
        let raw = config(mode, prod);
        let selected = serde_json::from_value::<HyroConfig>(raw.clone())
            .unwrap()
            .resolve(testnet)
            .unwrap();
        assert_eq!(selected.rest, format!("https://{expected}.example"));
        assert_eq!(
            selected.public_ws,
            format!("wss://{expected}.example/public")
        );
        assert_eq!(
            selected.private_ws,
            format!("wss://{expected}.example/private")
        );
        construct(
            "hyrotrader",
            &serde_json::json!({"publicDataOnly":true,"testnet":testnet,"hyrotrader":raw})
                .to_string(),
        )
        .unwrap();
    }
    let mut incomplete = config(None, "https://prod.example");
    incomplete["production"]["publicWs"]["livenet"] = "".into();
    assert_eq!(
        rejected("hyrotrader", &serde_json::json!({
            "publicDataOnly":true,"hyrotrader":incomplete,
            "endpoints":{"rest":"https://legacy.example","publicWs":"wss://legacy.example/public","privateWs":"wss://legacy.example/private"}
        }).to_string()),
        "HyroTrader REST/public/private URLs are not configured"
    );
    // Existing native endpoint inputs remain valid when the SDK raw config is absent.
    construct("hyrotrader", r#"{"publicDataOnly":true,"endpoints":{"rest":"https://legacy.example","publicWs":"wss://legacy.example/public","privateWs":"wss://legacy.example/private"}}"#).unwrap();
}

#[cfg(feature = "bybit")]
#[test]
fn bybit_regional_and_remote_signing_markers_suppress_opposite_host_diagnostics() {
    struct RejectFee;
    impl Visit for RejectFee {
        type Output = Vec<safe_cex_engine::Action>;
        fn visit<V: Venue + 'static>(self, mut engine: Engine<V>) -> Self::Output {
            engine.start(1_000.0);
            let request = engine
                .drain_actions()
                .find_map(|action| match action {
                    safe_cex_engine::Action::Http(request)
                        if request.url.contains("/v5/account/fee-rate") =>
                    {
                        Some(request.id)
                    }
                    _ => None,
                })
                .expect("private startup fee read");
            engine.handle(
                1_001.0,
                safe_cex_engine::Event::HttpResponse {
                    request,
                    response: safe_cex_engine::Response {
                        status: 200,
                        headers: "",
                        body: r#"{"retCode":10003,"retMsg":"API key is invalid.","result":{}}"#,
                    },
                },
            );
            // The probe is a managed Background read: it surfaces as a Prepare.
            engine.prepare_dispatch(1_001.0);
            engine.drain_actions().collect()
        }
    }
    for (site, remote, expected_probe) in [
        (None, false, true),
        (Some(""), false, true),
        (Some("BRA_BTL"), false, false),
        (None, true, false),
    ] {
        let raw = serde_json::json!({"key":"key","secret":"secret","bybitSiteId":site,"remoteSigning":remote});
        let options: EngineOptions = serde_json::from_value(raw).unwrap();
        let actions = build(
            "bybit",
            options,
            Some(safe_cex_engine::test_support::seeded_entropy(17)),
            RejectFee,
        )
        .unwrap();
        let probe = |url: &str| url == "https://api-demo.bybit.com/v5/user/query-api";
        assert_eq!(
            actions.iter().any(|action| match action {
                safe_cex_engine::Action::Http(request) => probe(&request.url),
                safe_cex_engine::Action::ManagedHttp(managed) => matches!(managed.as_ref(),
                    safe_cex_engine::request_context::HttpAction::Prepare { request, .. } if probe(&request.url)),
                _ => false,
            }),
            expected_probe
        );
    }
}

#[cfg(feature = "bitunix")]
#[test]
fn bitunix_secondary_reads_reach_the_native_signer() {
    use safe_cex_engine::{Action, Event, HttpRequest, Response};
    struct Started;
    impl Visit for Started {
        type Output = Vec<HttpRequest>;
        fn visit<V: Venue + 'static>(self, mut engine: Engine<V>) -> Self::Output {
            engine.start(1700000000000.0);
            let mut private = Vec::new();
            loop {
                let actions =
                    safe_cex_engine::test_support::prepared_actions(&mut engine, 1700000000000.0);
                if actions.is_empty() {
                    break;
                }
                for a in actions {
                    if let Action::Http(r) = a {
                        let body = if r.url.ends_with("/trading_pairs") {
                            r#"{"code":0,"data":[{"symbol":"BTCUSDT","base":"BTC","quote":"USDT","symbolStatus":"OPEN","basePrecision":3,"quotePrecision":1,"maxLimitOrderVolume":"100"}]}"#
                        } else if r.url.ends_with("/tickers") {
                            r#"{"code":0,"data":[{"symbol":"BTCUSDT","last":"96000","open":"95000","markPrice":"96001","baseVol":"1000","quoteVol":"96000000"}]}"#
                        } else {
                            private.push(r);
                            continue;
                        };
                        assert!(
                            !r.headers
                                .iter()
                                .any(|(n, _)| n.eq_ignore_ascii_case("api-key"))
                        );
                        engine.handle(
                            1700000000000.0,
                            Event::HttpResponse {
                                request: r.id,
                                response: Response {
                                    status: 200,
                                    headers: "",
                                    body,
                                },
                            },
                        );
                    }
                }
            }
            private
        }
    }
    let requests = build("bitunix", serde_json::from_str(r#"{"key":"primary-key","secret":"primary-secret","readCredentials":{"key":"read-key","secret":"read-secret"}}"#).unwrap(), Some(safe_cex_engine::test_support::seeded_entropy(17)), Started).unwrap();
    assert!(!requests.is_empty());
    for request in requests {
        assert!(
            request
                .headers
                .iter()
                .any(|(n, v)| n.eq_ignore_ascii_case("api-key") && v == "read-key"),
            "registry must forward read identity to actual native GET"
        );
    }
}

#[test]
fn opaque_signing_refs_are_rejected_until_a_family_uses_dispatch_signing() {
    for venue in ["hyperliquid", "orderly"] {
        assert_eq!(
            rejected(
                venue,
                r#"{"signing":{"primary":{"credential":"primary.credential","signing":"primary.signing"}},"remoteSigning":true,"publicMode":{"requested":false}}"#
            ),
            "Injected signing is not supported for this venue yet"
        );
    }
}

#[test]
fn migrated_opaque_refs_require_an_injected_provider_before_construction() {
    for venue in [
        "okx",
        "okxeu",
        "okxdemo",
        "okxeudemo",
        "bitget",
        "bitgetusdc",
        "bitgetinverse",
        "binance",
        "binancepm",
        "binanceinverse",
        "binancespot",
        "bybit",
        "bybitinverse",
        "bybitspot",
        "bybitdemo",
        "hyrotrader",
    ] {
        let error = rejected(
            venue,
            r#"{"signing":{"primary":{"credential":"primary.credential","signing":"primary.signing","passphrase":"primary.passphrase"}}}"#,
        );
        assert!(
            error.to_lowercase().contains("provider"),
            "{venue}: {error}"
        );
    }
}
