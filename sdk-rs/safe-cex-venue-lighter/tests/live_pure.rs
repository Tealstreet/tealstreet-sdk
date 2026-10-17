use safe_cex_exchange::js_runtime::value::{from_json, from_retained_input, to_json};
use safe_cex_num::PrecisionInput;
use safe_cex_transport::*;
use safe_cex_types::{ExchangeName, ExchangeOptions, Timeframe};
use safe_cex_venue_lighter::builder_approval::{
    self, DEFAULT_LIGHTER_APPROVAL_FEE_CAPS, DEFAULT_LIGHTER_APPROVAL_FEE_VALUE,
    LighterBuilderApprovalFeeCaps,
};
use safe_cex_venue_lighter::utils::*;
use safe_cex_venue_lighter::ws_policy::*;
use safe_cex_venue_lighter::ws_public_parsers::*;
use safe_cex_venue_lighter::{deployment::*, types::*, ws_public_ohlcv::*};
use serde::Deserialize;
use serde_json::Value;
use std::{process::Command, sync::Arc};

fn public_close_identity(value: &LighterPublicCloseClassification) -> Value {
    serde_json::json!({"isRateLimited":value.is_rate_limited,"isPolicyRestricted":value.is_policy_restricted})
}
fn private_close_identity(value: &LighterPrivateCloseClassification) -> Value {
    serde_json::json!({"isHandshakeFailure":value.is_handshake_failure,"isRateLimited":value.is_rate_limited,"isPolicyRestricted":value.is_policy_restricted,"isAuthIssue":value.is_auth_issue,"isReadonlyFallbackSignal":value.is_readonly_fallback_signal,"shouldBackoff":value.should_backoff})
}

fn number(value: &Value) -> f64 {
    match value.get("$number").and_then(Value::as_str) {
        Some("NaN") => f64::NAN,
        Some("Infinity") => f64::INFINITY,
        Some("-Infinity") => f64::NEG_INFINITY,
        Some("-0") => -0.0,
        Some(other) => panic!("unknown numeric tag {other}"),
        None => value.as_f64().unwrap(),
    }
}
fn decode(value: &Value) -> Value {
    if value.get("$number").is_some() {
        return serde_json::json!(number(value));
    }
    match value {
        Value::Array(values) => Value::Array(values.iter().map(decode).collect()),
        Value::Object(values) => {
            Value::Object(values.iter().map(|(k, v)| (k.clone(), decode(v))).collect())
        }
        _ => value.clone(),
    }
}

struct Proxy(String);
impl TransportProvider for Proxy {
    fn name(&self) -> &str {
        "test"
    }
    fn public_cors_proxy(&self) -> Option<String> {
        Some(self.0.clone())
    }
    fn rest(&self, c: RestTransportContext<'_>) -> Result<RestTransport, TransportError> {
        DirectTransport.rest(c)
    }
    fn rest_adapter(&self, c: RestAdapterContext<'_>) -> Option<Arc<dyn RestAdapter>> {
        DirectTransport.rest_adapter(c)
    }
    fn proxy_prefix(&self, c: RestTransportContext<'_>) -> String {
        DirectTransport.proxy_prefix(c)
    }
    fn private_proxy(&self, e: &ExchangeName) -> Option<Arc<dyn PrivateProxy>> {
        DirectTransport.private_proxy(e)
    }
    fn require_proxy(&self, o: ExchangeOptions) -> ExchangeOptions {
        DirectTransport.require_proxy(o)
    }
    fn server_proxy_url(&self, e: &ExchangeName) -> Option<String> {
        DirectTransport.server_proxy_url(e)
    }
    fn ws_routing(&self, e: &ExchangeName, o: &RoutingOptions) -> ResolvedRouting {
        DirectTransport.ws_routing(e, o)
    }
    fn ws_connection(&self, u: &str) -> Option<WsConnection> {
        DirectTransport.ws_connection(u)
    }
}

#[test]
#[ignore = "requires Node and the maintained TypeScript compiler"]
fn lighter_pure_helpers_match_live_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/pure-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    for (name, count) in [
        ("publicRestPayloads", 2298),
        ("publicRestBoundaries", 3),
        ("stringBoundaries", 99),
        ("exponentialBackoffs", 2744),
        ("publicBackoffs", 5488),
        ("privateBackoffs", 21952),
        ("fundingRates", 2527),
        ("approvalMessageSchema", 10),
        ("approvalMessageCases", 177),
        ("scaledInts", 1150),
        ("approvalBigints", 458),
        ("approvalHex", 642),
        ("bigintParses", 427),
        ("approvalNumbers", 32),
        ("clientIds", 240),
        ("bigintStrings", 5),
        ("helperUrlSchema", 13),
        ("helperUrls", 480),
        ("approvalIntegratorDefaults", 8),
        ("capCases", 4096),
        ("approvalUrls", 176),
        ("policyPatterns", 75),
        ("publicCloses", 750),
        ("privateCloses", 3000),
        ("precision", 34),
        ("prefixes", 9),
        ("clamps", 15),
        ("deployments", 36),
        ("intervals", 12),
        ("topics", 64),
        ("recoveries", 648),
        ("promotions", 8),
        ("dispatch", 96),
        ("numbers", 31),
        ("books", 43),
        ("channels", 10),
    ] {
        assert_eq!(
            oracle[name].as_array().unwrap().len(),
            count,
            "PURE-CARDINALITY {name} source cases"
        );
    }

    let same_bits = |actual: f64, expected: f64, name: &str, case: &Value| {
        if expected.is_nan() {
            assert!(actual.is_nan(), "{name} {case}");
        } else {
            assert_eq!(actual.to_bits(), expected.to_bits(), "{name} {case}");
        }
    };
    for case in oracle["exponentialBackoffs"].as_array().unwrap() {
        let opts = case.get("opts").map(|o| LighterBackoffOptions {
            multiplier: o.get("multiplier").map(number),
            max_exponent: o.get("maxExponent").map(number),
            max_delay_ms: o.get("maxDelayMs").map(number),
        });
        same_bits(
            calculate_exponential_backoff_delay(
                number(&case["baseDelayMs"]),
                number(&case["failureCount"]),
                opts,
            ),
            number(&case["expected"]),
            "calculateExponentialBackoffDelay",
            case,
        );
    }
    for case in oracle["publicBackoffs"].as_array().unwrap() {
        same_bits(
            calculate_lighter_public_reconnect_delay(
                number(&case["baseDelayMs"]),
                number(&case["failureCount"]),
                case["isRateLimited"].as_bool().unwrap(),
                number(&case["rateLimitReconnectDelayMs"]),
            ),
            number(&case["expected"]),
            "calculateLighterPublicReconnectDelay",
            case,
        );
    }
    for case in oracle["privateBackoffs"].as_array().unwrap() {
        same_bits(
            calculate_lighter_private_reconnect_delay(
                number(&case["baseDelayMs"]),
                number(&case["failedHandshakes"]),
                case["shouldBackoff"].as_bool().unwrap(),
                case["isRateLimited"].as_bool().unwrap(),
                case["isWsRateLimited"].as_bool().unwrap(),
                number(&case["rateLimitReconnectDelayMs"]),
            ),
            number(&case["expected"]),
            "calculateLighterPrivateReconnectDelay",
            case,
        );
    }
    for (name, fields) in [
        (
            "calculateExponentialBackoffDelay",
            vec![
                ("multiplier", "number", true),
                ("maxExponent", "number", true),
                ("maxDelayMs", "number", true),
            ],
        ),
        (
            "calculateLighterPublicReconnectDelay",
            vec![
                ("baseDelayMs", "number", false),
                ("failureCount", "number", false),
                ("isRateLimited", "boolean", false),
                ("rateLimitReconnectDelayMs", "number", false),
            ],
        ),
        (
            "calculateLighterPrivateReconnectDelay",
            vec![
                ("baseDelayMs", "number", false),
                ("failedHandshakes", "number", false),
                ("shouldBackoff", "boolean", false),
                ("isRateLimited", "boolean", false),
                ("isWsRateLimited", "boolean", false),
                ("rateLimitReconnectDelayMs", "number", false),
            ],
        ),
    ] {
        let fields: Vec<Value> = fields.into_iter().map(|(name,typ,optional)|serde_json::json!({"name":name,"type":typ,"optional":optional})).collect();
        assert_eq!(
            serde_json::json!(fields),
            oracle["backoffSchemas"][name],
            "{name} parameter schema"
        );
    }

    for case in oracle["fundingRates"].as_array().unwrap() {
        let actual = safe_cex_venue_lighter::ws_public::normalize_lighter_funding_rate(number(
            &case["input"],
        ));
        let expected = number(&case["expected"]);
        if expected.is_nan() {
            assert!(actual.is_nan(), "normalizeLighterFundingRate {case}");
        } else {
            assert_eq!(
                actual.to_bits(),
                expected.to_bits(),
                "normalizeLighterFundingRate {case}"
            );
        }
    }

    for case in oracle["scaledInts"].as_array().unwrap() {
        let actual = match to_lighter_scaled_int(number(&case["value"]), number(&case["decimals"]))
        {
            Ok(value) => serde_json::json!({"ok":value.to_string()}),
            Err(error) => {
                serde_json::json!({"error":{"name":error.kind.name(),"message":error.message,"messageUtf16":error.message_utf16}})
            }
        };
        let expected = if case.get("error").is_some() {
            serde_json::json!({"error":case["error"]})
        } else {
            serde_json::json!({"ok":case["ok"]})
        };
        assert_eq!(actual, expected, "toLighterScaledInt {case}");
    }

    for case in oracle["approvalBigints"].as_array().unwrap() {
        let big = case["input"]
            .get("$bigint")
            .map(|v| v.as_str().unwrap().parse::<num_bigint::BigInt>().unwrap());
        let decoded = case.get("input").map(decode);
        let input = if let Some(ref big) = big {
            builder_approval::LighterApprovalValue::BigInt(big)
        } else if case["input"].is_number() || case["input"].get("$number").is_some() {
            builder_approval::LighterApprovalValue::Number(number(&case["input"]))
        } else {
            builder_approval::LighterApprovalValue::Json(decoded.as_ref())
        };
        assert_eq!(
            serde_json::json!(builder_approval::to_big_int(input).map(|v| v.to_string())),
            case["expected"],
            "toBigInt {case}"
        );
    }
    for case in oracle["approvalHex"].as_array().unwrap() {
        let big = case["input"]
            .get("$bigint")
            .map(|v| v.as_str().unwrap().parse::<num_bigint::BigInt>().unwrap());
        let input = match big.as_ref() {
            Some(big) => builder_approval::LighterApprovalInteger::BigInt(big),
            None => builder_approval::LighterApprovalInteger::Number(number(&case["input"])),
        };
        let actual = match builder_approval::to_hex_10(input, case["field"].as_str().unwrap()) {
            Ok(value) => serde_json::json!({"ok":value}),
            Err(error) => {
                serde_json::json!({"error":{"name":error.kind.name(),"message":error.message,"messageUtf16":error.message_utf16}})
            }
        };
        let expected = if case.get("error").is_some() {
            serde_json::json!({"error":case["error"]})
        } else {
            serde_json::json!({"ok":case["ok"]})
        };
        assert_eq!(actual, expected, "toHex10 {case}");
    }

    let message_schema: Vec<Value> = [
        ("nonce", "bigint"),
        ("accountIndex", "bigint"),
        ("apiKeyIndex", "number"),
        ("integratorAccountIndex", "bigint"),
        ("maxPerpsTakerFee", "number"),
        ("maxPerpsMakerFee", "number"),
        ("maxSpotTakerFee", "number"),
        ("maxSpotMakerFee", "number"),
        ("approvalExpiryMs", "bigint"),
        ("chainId", "number"),
    ]
    .into_iter()
    .map(|(name, typ)| serde_json::json!({"name":name,"type":typ,"optional":false}))
    .collect();
    assert_eq!(
        serde_json::json!(message_schema),
        oracle["approvalMessageSchema"],
        "buildLighterApproveIntegratorL1Message parameter schema"
    );
    for case in oracle["approvalMessageCases"].as_array().unwrap() {
        let p = &case["params"];
        let big = |key: &str| {
            p[key]["$bigint"]
                .as_str()
                .unwrap()
                .parse::<num_bigint::BigInt>()
                .unwrap()
        };
        let nonce = big("nonce");
        let account = big("accountIndex");
        let integrator = big("integratorAccountIndex");
        let expiry = big("approvalExpiryMs");
        let params = builder_approval::LighterApproveIntegratorL1MessageParams {
            nonce: &nonce,
            account_index: &account,
            api_key_index: number(&p["apiKeyIndex"]),
            integrator_account_index: &integrator,
            max_perps_taker_fee: number(&p["maxPerpsTakerFee"]),
            max_perps_maker_fee: number(&p["maxPerpsMakerFee"]),
            max_spot_taker_fee: number(&p["maxSpotTakerFee"]),
            max_spot_maker_fee: number(&p["maxSpotMakerFee"]),
            approval_expiry_ms: &expiry,
            chain_id: number(&p["chainId"]),
        };
        let actual = match builder_approval::build_lighter_approve_integrator_l1_message(&params) {
            Ok(value) => serde_json::json!({"ok":value}),
            Err(error) => {
                serde_json::json!({"error":{"name":error.kind.name(),"message":error.message,"messageUtf16":error.message_utf16}})
            }
        };
        let expected = if case.get("error").is_some() {
            serde_json::json!({"error":case["error"]})
        } else {
            serde_json::json!({"ok":case["ok"]})
        };
        assert_eq!(
            actual, expected,
            "buildLighterApproveIntegratorL1Message {case}"
        );
    }

    assert_eq!(
        oracle["publicRestPayloads"].as_array().unwrap().len(),
        2298,
        "public collection identity inventory"
    );
    let rest_cases = oracle["publicRestPayloads"].as_array().unwrap();
    let mut source_totals = [0_usize; 4];
    for case in rest_cases {
        if let Some(book) = case["book"].as_object() {
            source_totals[0] += 1;
            source_totals[1] += book["bids"].as_array().unwrap().len();
            source_totals[2] += book["asks"].as_array().unwrap().len();
        }
        source_totals[3] += case["tickers"].as_array().unwrap().len();
    }
    assert_eq!(
        source_totals,
        [424, 425, 251, 3286],
        "REST-CARDINALITY source books/bids/asks/tickers totals"
    );
    let mut native_totals = [0_usize; 4];
    for case in rest_cases {
        let input = case.get("input").map(from_json);
        let book = safe_cex_venue_lighter::ws_public::parse_rest_order_book_levels(input.as_ref())
            .unwrap();
        if !book.is_undefined() {
            let bids = book.get("bids").unwrap();
            let asks = book.get("asks").unwrap();
            let bids = bids.array_values().unwrap();
            let asks = asks.array_values().unwrap();
            native_totals[0] += 1;
            native_totals[1] += bids.len();
            native_totals[2] += asks.len();
            assert_eq!(
                bids.len(),
                case["book"]["bids"].as_array().unwrap().len(),
                "REST-CARDINALITY native bid count"
            );
            assert_eq!(
                asks.len(),
                case["book"]["asks"].as_array().unwrap().len(),
                "REST-CARDINALITY native ask count"
            );
            assert_eq!(
                bids.iter()
                    .map(|value| to_json(value).unwrap())
                    .collect::<Vec<_>>(),
                case["book"]["bids"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|value| to_json(&from_json(value)).unwrap())
                    .collect::<Vec<_>>(),
                "parseRestOrderBookLevels.bids native identity {case}"
            );
            assert_eq!(
                asks.iter()
                    .map(|value| to_json(value).unwrap())
                    .collect::<Vec<_>>(),
                case["book"]["asks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|value| to_json(&from_json(value)).unwrap())
                    .collect::<Vec<_>>(),
                "parseRestOrderBookLevels.asks native identity {case}"
            );
        } else {
            assert!(
                case["book"].is_null(),
                "parseRestOrderBookLevels absent {case}"
            );
        }
        let tickers = safe_cex_venue_lighter::ws_public::parse_rest_ticker_items(input.as_ref())
            .unwrap()
            .array_values()
            .unwrap();
        native_totals[3] += tickers.len();
        assert_eq!(
            tickers.len(),
            case["tickers"].as_array().unwrap().len(),
            "REST-CARDINALITY native ticker count"
        );
        assert_eq!(
            tickers
                .iter()
                .map(|value| to_json(value).unwrap())
                .collect::<Vec<_>>(),
            case["tickers"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| to_json(&from_json(value)).unwrap())
                .collect::<Vec<_>>(),
            "parseRestTickerItems native item identity {case}"
        );
    }
    assert_eq!(
        native_totals,
        [424, 425, 251, 3286],
        "REST-CARDINALITY native comparisons books/bids/asks/tickers"
    );
    let rest_boundaries = oracle["publicRestBoundaries"].as_array().unwrap();
    assert_eq!(rest_boundaries.len(), 3);
    for case in rest_boundaries {
        let native = |field: &str| {
            from_retained_input(
                safe_cex_types::retained_json::RetainedJsonInput::parse_json(
                    &case[field]
                        .as_str()
                        .unwrap()
                        .encode_utf16()
                        .collect::<Vec<_>>(),
                )
                .unwrap(),
            )
        };
        let input = native("inputJson");
        let book =
            safe_cex_venue_lighter::ws_public::parse_rest_order_book_levels(Some(&input)).unwrap();
        let expected_book = native("bookJson");
        for side in ["bids", "asks"] {
            let actual = book.get(side).unwrap().array_values().unwrap();
            let expected = expected_book.get(side).unwrap().array_values().unwrap();
            assert_eq!(actual.len(), expected.len());
            for (actual, expected) in actual.iter().zip(&expected) {
                assert_eq!(
                    actual.to_js_string().unwrap(),
                    expected.to_js_string().unwrap()
                );
            }
        }
        let tickers = safe_cex_venue_lighter::ws_public::parse_rest_ticker_items(Some(&input))
            .unwrap()
            .array_values()
            .unwrap();
        let expected = native("tickersJson").array_values().unwrap();
        assert_eq!(tickers.len(), expected.len());
        for (actual, expected) in tickers.iter().zip(&expected) {
            assert_eq!(
                actual.get("symbol").unwrap().to_js_string().unwrap(),
                expected.get("symbol").unwrap().to_js_string().unwrap()
            );
        }
    }
    for case in oracle["bigintParses"].as_array().unwrap() {
        let actual = match string_to_big_int(case["input"].as_str().unwrap()) {
            Ok(value) => serde_json::json!({"ok":value.to_string()}),
            Err(error) => {
                // See sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary.
                // The canonical error is UTF-16: valid scalar input can be truncated
                // between an astral pair by V8's diagnostic length limit.
                assert_eq!(
                    error.message,
                    String::from_utf16_lossy(&error.message_utf16)
                );
                serde_json::json!({"error":{"name":error.kind.name(),"messageUtf16":error.message_utf16}})
            }
        };
        let expected = if case.get("error").is_some() {
            serde_json::json!({"error":case["error"]})
        } else {
            serde_json::json!({"ok":case["ok"]})
        };
        assert_eq!(actual, expected, "stringToBigInt {case}");
    }

    let truncated_astral_input = format!("{}😀", "x".repeat(999));
    let truncated_astral_case = oracle["bigintParses"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["input"].as_str() == Some(truncated_astral_input.as_str()))
        .expect("stringToBigInt truncated-astral witness must remain in live corpus");
    let units: Vec<u16> =
        serde_json::from_value(truncated_astral_case["error"]["messageUtf16"].clone()).unwrap();
    assert!(
        String::from_utf16(&units).is_err(),
        "stringToBigInt TS diagnostic retains an unpaired surrogate"
    );
    assert!(
        units.contains(&0xd83d),
        "stringToBigInt truncated high surrogate"
    );

    for case in oracle["clientIds"].as_array().unwrap() {
        let big = case["input"]
            .get("$bigint")
            .map(|v| num_bigint::BigInt::parse_bytes(v.as_str().unwrap().as_bytes(), 10).unwrap());
        let input = match big.as_ref() {
            Some(value) => LighterClientIdAccountIndex::BigInt(value),
            None => LighterClientIdAccountIndex::String(case["input"]["string"].as_str().unwrap()),
        };
        assert_eq!(
            generate_lighter_client_id(input, number(&case["now"]), number(&case["random"])),
            case["expected"].as_str().unwrap(),
            "generateLighterClientId {case}"
        );
    }
    for case in oracle["bigintStrings"].as_array().unwrap() {
        let value = num_bigint::BigInt::parse_bytes(case["input"].as_str().unwrap().as_bytes(), 10)
            .unwrap();
        assert_eq!(
            big_int_to_string(&value),
            case["expected"].as_str().unwrap(),
            "bigIntToString {case}"
        );
    }
    let helper_fields = [
        ("accountIndex", "bigint", false),
        ("apiKeyIndex", "number", false),
        ("approvalExpiryMs", "number", false),
        ("chainId", "number", false),
        ("expectedWalletAddress", "string", true),
        ("integratorAccountIndex", "bigint", false),
        ("maxPerpsMakerFee", "number", false),
        ("maxPerpsTakerFee", "number", false),
        ("maxSpotMakerFee", "number", false),
        ("maxSpotTakerFee", "number", false),
        ("nonce", "bigint", false),
        ("origin", "string", false),
        ("testnet", "boolean", false),
    ];
    assert_eq!(
        serde_json::json!(helper_fields.map(
            |(name, typ, optional)| serde_json::json!({"name":name,"type":typ,"optional":optional})
        )),
        oracle["helperUrlSchema"],
        "createLighterL1SignatureHelperUrl parameter schema"
    );
    for case in oracle["helperUrls"].as_array().unwrap() {
        let p = &case["params"];
        let bigint = |key: &str| {
            num_bigint::BigInt::parse_bytes(p[key].as_str().unwrap().as_bytes(), 10).unwrap()
        };
        let account = bigint("accountIndex");
        let integrator = bigint("integratorAccountIndex");
        let nonce = bigint("nonce");
        let params = builder_approval::LighterL1SignatureHelperUrlParams {
            account_index: &account,
            api_key_index: number(&p["apiKeyIndex"]),
            approval_expiry_ms: number(&p["approvalExpiryMs"]),
            chain_id: number(&p["chainId"]),
            expected_wallet_address: p.get("expectedWalletAddress").and_then(Value::as_str),
            integrator_account_index: &integrator,
            max_perps_maker_fee: number(&p["maxPerpsMakerFee"]),
            max_perps_taker_fee: number(&p["maxPerpsTakerFee"]),
            max_spot_maker_fee: number(&p["maxSpotMakerFee"]),
            max_spot_taker_fee: number(&p["maxSpotTakerFee"]),
            nonce: &nonce,
            origin: p["origin"].as_str().unwrap(),
            testnet: p["testnet"].as_bool().unwrap(),
        };
        assert_eq!(
            builder_approval::create_lighter_l1_signature_helper_url(&params),
            case["expected"].as_str().unwrap(),
            "createLighterL1SignatureHelperUrl {case}"
        );
    }
    for case in oracle["approvalIntegratorDefaults"].as_array().unwrap() {
        let name = case
            .get("exchangeName")
            .map(|v| serde_json::from_value(v.clone()).unwrap());
        let actual = builder_approval::get_default_integrator_account_index(
            name.as_ref(),
            case["testnet"].as_bool().unwrap(),
        );
        match actual {
            Ok(value) => assert_eq!(
                value.to_string(),
                case["ok"].as_str().unwrap(),
                "approval getDefaultIntegratorAccountIndex {case}"
            ),
            Err(message) => assert_eq!(
                serde_json::json!({"name":"Error","message":message}),
                case["error"],
                "approval getDefaultIntegratorAccountIndex {case}"
            ),
        }
    }

    for case in oracle["precision"].as_array().unwrap() {
        let input = case["input"]
            .as_str()
            .map(|s| PrecisionInput::String(s.into()))
            .unwrap_or_else(|| PrecisionInput::Number(number(&case["input"])));
        assert_eq!(
            get_precision(input),
            number(&case["expected"]),
            "getPrecision {case}"
        );
    }
    // A lone surrogate in valid JSON is a shared representation refusal,
    // not malformed JSON and not permission to replace its code unit.
    // sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary
    assert_eq!(
        oracle["stringBoundaries"].as_array().unwrap().len(),
        99,
        "shared string boundary witness inventory"
    );
    for case in oracle["stringBoundaries"].as_array().unwrap() {
        let units: Vec<u16> = serde_json::from_value(case["inputUnits"].clone()).unwrap();
        if case["symbol"] == "normalizeNonNegativeNumber" {
            let value = safe_cex_exchange::js_runtime::value::Js::utf16(&units);
            assert_eq!(
                serde_json::json!({"okJson":safe_cex_num::js_number_string(normalize_non_negative_number(&value).unwrap())}),
                serde_json::json!({"okJson":case["okJson"]}),
                "native UTF16 normalization {case}",
            );
            continue;
        }
        let input = serde_json::from_str::<String>(case["inputJson"].as_str().unwrap());
        if String::from_utf16(&units).is_err() {
            assert!(
                input.is_err(),
                "{} shared string ingress must refuse lone surrogate: {case}",
                case["symbol"]
            );
            assert!(
                case.get("okJson").is_some() || case.get("error").is_some(),
                "live TypeScript outcome retained: {case}"
            );
            continue;
        }
        let input = input.expect("scalar Unicode ingress must remain supported");
        let actual = match case["symbol"].as_str().unwrap() {
            "dropPrefix" => {
                serde_json::json!({"okJson":serde_json::to_string(drop_prefix(&input)).unwrap()})
            }
            "addPrefix" => {
                serde_json::json!({"okJson":serde_json::to_string(&add_prefix(&input)).unwrap()})
            }
            "stringToBigInt" => match string_to_big_int(&input) {
                Ok(value) => {
                    serde_json::json!({"okJson":serde_json::to_string(&value.to_string()).unwrap()})
                }
                Err(error) => {
                    serde_json::json!({"error":{"name":error.kind.name(),"messageUtf16":error.message_utf16}})
                }
            },
            "parseLighterApiKeyIndex" => {
                let value = Value::String(input);
                let result = parse_lighter_api_key_index(Some(&value)).unwrap();
                serde_json::json!({"okJson":result.map(safe_cex_num::js_number_string)})
            }
            "getPrecision" => {
                serde_json::json!({"okJson":safe_cex_num::js_number_string(get_precision(PrecisionInput::String(input)))})
            }
            "toLighterApiKeyIndexString" => {
                serde_json::json!({"okJson":serde_json::to_string(&to_lighter_api_key_index_string(Some(&Value::String(input)), None).unwrap()).unwrap()})
            }
            "parseLighterPositiveIntegerString" => {
                serde_json::json!({"okJson":parse_lighter_positive_integer_string(Some(&Value::String(input))).unwrap().map(|s|serde_json::to_string(&s).unwrap())})
            }
            "isLighterL1Address" => {
                serde_json::json!({"okJson":is_lighter_l1_address(Some(&Value::String(input))).unwrap().to_string()})
            }
            other => panic!("unrecognized boundary {other}"),
        };
        let expected = if case.get("error").is_some() {
            serde_json::json!({"error":case["error"]})
        } else {
            serde_json::json!({"okJson":case["okJson"]})
        };
        assert_eq!(actual, expected, "shared string boundary {case}");
    }
    for case in oracle["prefixes"].as_array().unwrap() {
        let input = case["input"].as_str().unwrap();
        assert_eq!(
            drop_prefix(input),
            case["drop"].as_str().unwrap(),
            "dropPrefix {case}"
        );
        assert_eq!(
            add_prefix(input),
            case["add"].as_str().unwrap(),
            "addPrefix {case}"
        );
    }
    for case in oracle["clamps"].as_array().unwrap() {
        let actual = clamp_decimals(number(&case["input"]));
        assert_eq!(
            actual.to_bits(),
            number(&case["expected"]).to_bits(),
            "clampDecimals {case}"
        );
    }
    for case in oracle["policyPatterns"].as_array().unwrap() {
        let reason = case["reason"].as_str().unwrap();
        assert_eq!(
            LIGHTER_POLICY_REASON_RE(reason),
            case["policy"].as_bool().unwrap(),
            "LIGHTER_POLICY_REASON_RE {case}"
        );
        assert_eq!(
            LIGHTER_AUTH_REASON_RE(reason),
            case["auth"].as_bool().unwrap(),
            "LIGHTER_AUTH_REASON_RE {case}"
        );
    }
    for case in oracle["publicCloses"].as_array().unwrap() {
        let code = (!case["closeCode"].is_null()).then(|| number(&case["closeCode"]));
        let result = classify_lighter_public_close(code, case["closeReason"].as_str().unwrap());
        assert_eq!(
            public_close_identity(&result),
            case["expected"],
            "classifyLighterPublicClose native identity {case}"
        );
        assert_eq!(
            serde_json::to_value(&result).unwrap(),
            case["expected"],
            "classifyLighterPublicClose {case}"
        );
    }
    for case in oracle["privateCloses"].as_array().unwrap() {
        let code = (!case["closeCode"].is_null()).then(|| number(&case["closeCode"]));
        let result = classify_lighter_private_close(
            code,
            case["closeReason"].as_str().unwrap(),
            case["failedBeforeOpen"].as_bool().unwrap(),
            case["earlyPostOpenNoStream"].as_bool().unwrap(),
        );
        assert_eq!(
            private_close_identity(&result),
            case["expected"],
            "classifyLighterPrivateClose native identity {case}"
        );
        assert_eq!(
            serde_json::to_value(&result).unwrap(),
            case["expected"],
            "classifyLighterPrivateClose {case}"
        );
    }
    for (name, fields) in [
        (
            "LighterPublicCloseClassification",
            vec!["isRateLimited", "isPolicyRestricted"],
        ),
        (
            "LighterPrivateCloseClassification",
            vec![
                "isHandshakeFailure",
                "isRateLimited",
                "isPolicyRestricted",
                "isAuthIssue",
                "isReadonlyFallbackSignal",
                "shouldBackoff",
            ],
        ),
    ] {
        let schema: Vec<_> = fields
            .iter()
            .map(|name| serde_json::json!({"name":name,"type":"boolean","optional":false}))
            .collect();
        assert_eq!(
            oracle["policySchemas"][name],
            serde_json::json!(schema),
            "{name} schema"
        );
        for bits in 0..(1_u64 << fields.len()) {
            let input = Value::Object(
                fields
                    .iter()
                    .enumerate()
                    .map(|(index, field)| {
                        ((*field).to_owned(), Value::Bool(bits & (1 << index) != 0))
                    })
                    .collect(),
            );
            let native = if name == "LighterPublicCloseClassification" {
                public_close_identity(&serde_json::from_value(input.clone()).unwrap())
            } else {
                private_close_identity(&serde_json::from_value(input.clone()).unwrap())
            };
            for field in &fields {
                assert_eq!(
                    native[*field], input[*field],
                    "{name}.{field} native decode identity mask {bits}"
                );
            }
        }
        let value: Value = serde_json::json!(
            fields
                .iter()
                .map(|name| ((*name).to_owned(), Value::Bool(false)))
                .collect::<serde_json::Map<_, _>>()
        );
        for field in &fields {
            for null in [false, true] {
                let mut invalid = value.clone();
                if null {
                    invalid[*field] = Value::Null;
                } else {
                    invalid.as_object_mut().unwrap().remove(*field);
                }
                let accepted = if name == "LighterPublicCloseClassification" {
                    serde_json::from_value::<LighterPublicCloseClassification>(invalid).is_ok()
                } else {
                    serde_json::from_value::<LighterPrivateCloseClassification>(invalid).is_ok()
                };
                assert!(!accepted, "{name}.{field} required non-null boolean");
            }
        }
    }
    assert_eq!(
        oracle["approvalTables"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec![
            "DEFAULT_LIGHTER_APPROVAL_FEE_CAPS",
            "DEFAULT_LIGHTER_APPROVAL_FEE_VALUE"
        ],
        "approval exported table inventory"
    );
    assert_eq!(
        serde_json::to_value(DEFAULT_LIGHTER_APPROVAL_FEE_CAPS)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        oracle["approvalTables"]["DEFAULT_LIGHTER_APPROVAL_FEE_CAPS"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        "DEFAULT_LIGHTER_APPROVAL_FEE_CAPS keys"
    );
    for (field, value) in serde_json::to_value(DEFAULT_LIGHTER_APPROVAL_FEE_CAPS)
        .unwrap()
        .as_object()
        .unwrap()
    {
        assert_eq!(
            value.as_f64(),
            oracle["approvalTables"]["DEFAULT_LIGHTER_APPROVAL_FEE_CAPS"][field].as_f64(),
            "DEFAULT_LIGHTER_APPROVAL_FEE_CAPS.{field}"
        );
    }
    assert_eq!(
        DEFAULT_LIGHTER_APPROVAL_FEE_VALUE,
        number(&oracle["approvalTables"]["DEFAULT_LIGHTER_APPROVAL_FEE_VALUE"]),
        "DEFAULT_LIGHTER_APPROVAL_FEE_VALUE"
    );
    for case in oracle["capCases"].as_array().unwrap() {
        let caps = &case["input"];
        let caps = LighterBuilderApprovalFeeCaps {
            max_perps_maker_fee: number(&caps["maxPerpsMakerFee"]),
            max_perps_taker_fee: number(&caps["maxPerpsTakerFee"]),
            max_spot_maker_fee: number(&caps["maxSpotMakerFee"]),
            max_spot_taker_fee: number(&caps["maxSpotTakerFee"]),
        };
        assert_eq!(
            builder_approval::approval_fee_caps_have_non_zero_fees(&caps),
            case["expected"].as_bool().unwrap(),
            "approvalFeeCapsHaveNonZeroFees {case}"
        );
    }
    for (name, fields) in [
        (
            "LighterCancelTarget",
            vec![("orderId", "string"), ("marketId", "number")],
        ),
        (
            "LighterOrderMinimums",
            vec![("minBaseAmount", "number"), ("minQuoteAmount", "number")],
        ),
        (
            "LighterBuilderApprovalFeeCaps",
            vec![
                ("maxPerpsMakerFee", "number"),
                ("maxPerpsTakerFee", "number"),
                ("maxSpotMakerFee", "number"),
                ("maxSpotTakerFee", "number"),
            ],
        ),
    ] {
        let schema: Vec<_> = fields
            .iter()
            .map(|(name, kind)| serde_json::json!({"name":name,"type":kind,"optional":false}))
            .collect();
        assert_eq!(
            oracle["scalarSchemas"][name],
            serde_json::json!(schema),
            "{name} schema"
        );
        let roundtrip = |value: Value| -> Result<Value, serde_json::Error> {
            match name {
                "LighterCancelTarget" => {
                    serde_json::to_value(serde_json::from_value::<LighterCancelTarget>(value)?)
                }
                "LighterOrderMinimums" => {
                    serde_json::to_value(serde_json::from_value::<LighterOrderMinimums>(value)?)
                }
                "LighterBuilderApprovalFeeCaps" => serde_json::to_value(serde_json::from_value::<
                    LighterBuilderApprovalFeeCaps,
                >(value)?),
                _ => unreachable!(),
            }
        };
        let identity = Value::Object(
            fields
                .iter()
                .enumerate()
                .map(|(index, (field, kind))| {
                    (
                        (*field).to_owned(),
                        if *kind == "number" {
                            serde_json::json!(index as f64 + 0.25)
                        } else {
                            serde_json::json!(format!("{field}-identity"))
                        },
                    )
                })
                .collect(),
        );
        let native = match name {
            "LighterCancelTarget" => {
                let decoded: LighterCancelTarget =
                    serde_json::from_value(identity.clone()).unwrap();
                serde_json::json!({"orderId":decoded.order_id,"marketId":decoded.market_id})
            }
            "LighterOrderMinimums" => {
                let decoded: LighterOrderMinimums =
                    serde_json::from_value(identity.clone()).unwrap();
                serde_json::json!({"minBaseAmount":decoded.min_base_amount,"minQuoteAmount":decoded.min_quote_amount})
            }
            "LighterBuilderApprovalFeeCaps" => {
                let decoded: LighterBuilderApprovalFeeCaps =
                    serde_json::from_value(identity.clone()).unwrap();
                serde_json::json!({"maxPerpsMakerFee":decoded.max_perps_maker_fee,"maxPerpsTakerFee":decoded.max_perps_taker_fee,"maxSpotMakerFee":decoded.max_spot_maker_fee,"maxSpotTakerFee":decoded.max_spot_taker_fee})
            }
            _ => unreachable!(),
        };
        for (field, _) in &fields {
            assert_eq!(
                native[*field], identity[*field],
                "{name}.{field} native field identity"
            );
        }
        for n in [0.0_f64, -0.0, 1.25, -0.25, 16777217.0, 1e100] {
            let valid = Value::Object(
                fields
                    .iter()
                    .map(|(name, kind)| {
                        (
                            (*name).to_owned(),
                            if *kind == "number" {
                                serde_json::json!(n)
                            } else {
                                serde_json::json!("001😀")
                            },
                        )
                    })
                    .collect(),
            );
            let restored = roundtrip(valid.clone()).unwrap();
            assert_eq!(
                restored.as_object().unwrap().len(),
                fields.len(),
                "{name} field inventory"
            );
            for (field, kind) in &fields {
                if *kind == "number" {
                    assert_eq!(
                        restored[*field].as_f64().unwrap().to_bits(),
                        n.to_bits(),
                        "{name}.{field} binary64 domain"
                    );
                } else {
                    assert_eq!(restored[*field], valid[*field], "{name}.{field}");
                }
                for null in [false, true] {
                    let mut invalid = valid.clone();
                    if null {
                        invalid[*field] = Value::Null;
                    } else {
                        invalid.as_object_mut().unwrap().remove(*field);
                    }
                    assert!(
                        roundtrip(invalid).is_err(),
                        "{name}.{field} required non-null"
                    );
                }
            }
        }
    }
    let approval_registry = TransportRegistry::default();
    for case in oracle["approvalUrls"].as_array().unwrap() {
        approval_registry
            .set_default_transport(Arc::new(Proxy(case["proxy"].as_str().unwrap().into())));
        let name: Option<ExchangeName> = case
            .get("exchangeName")
            .map(|v| serde_json::from_value(v.clone()).unwrap());
        assert_eq!(
            builder_approval::get_base_url(
                name.as_ref(),
                case.get("overrideBaseUrl").and_then(Value::as_str),
                case["testnet"].as_bool().unwrap(),
                &approval_registry
            ),
            case["expected"].as_str().unwrap(),
            "approval getBaseUrl {case}"
        );
    }
    assert!(
        LighterPublicCloseClassification::deserialize(serde_json::json!([false, false])).is_err(),
        "LighterPublicCloseClassification rejects positional array"
    );
    assert!(
        LighterPrivateCloseClassification::deserialize(serde_json::json!([
            false, false, false, false, false, false
        ]))
        .is_err(),
        "LighterPrivateCloseClassification rejects positional array"
    );
    assert!(
        LighterCancelTarget::deserialize(serde_json::json!(["x", 1])).is_err(),
        "LighterCancelTarget rejects positional array"
    );
    assert!(
        LighterOrderMinimums::deserialize(serde_json::json!([1, 2])).is_err(),
        "LighterOrderMinimums rejects positional array"
    );
    assert!(
        LighterBuilderApprovalFeeCaps::deserialize(serde_json::json!([0, 250, 0, 250])).is_err(),
        "LighterBuilderApprovalFeeCaps rejects positional array"
    );
    let registry = TransportRegistry::default();
    for case in oracle["deployments"].as_array().unwrap() {
        let name: Option<ExchangeName> = case["params"]
            .get("exchangeName")
            .map(|n| serde_json::from_value(n.clone()).unwrap());
        let testnet = case["params"].get("testnet").map(|v| v.as_bool().unwrap());
        registry.set_default_transport(Arc::new(Proxy(case["proxy"].as_str().unwrap().into())));
        let network: LighterDeploymentNetwork =
            serde_json::from_value(case["network"].clone()).unwrap();
        assert_eq!(
            is_lighter_robinhood_exchange(name.as_ref()),
            case["robinhood"].as_bool().unwrap(),
            "isLighterRobinhoodExchange {case}"
        );
        assert_eq!(
            resolve_lighter_deployment_network(name.as_ref(), testnet),
            network,
            "resolveLighterDeploymentNetwork {case}"
        );
        assert_eq!(
            resolve_lighter_deployment_profile(name.as_ref(), testnet),
            serde_json::from_value::<LighterDeploymentProfile>(case["profile"].clone()).unwrap(),
            "resolveLighterDeploymentProfile {case}"
        );
        assert_eq!(
            get_base_url(network, &registry),
            case["baseUrl"].as_str().unwrap(),
            "getBaseUrl {case}"
        );
        assert_eq!(
            get_lighter_api_base_url(name.as_ref(), testnet, &registry),
            case["apiUrl"].as_str().unwrap(),
            "getLighterApiBaseUrl {case}"
        );
        assert_eq!(
            get_lighter_ws_url(name.as_ref(), testnet),
            case["wsUrl"].as_str().unwrap(),
            "getLighterWsUrl {case}"
        );
    }
    for case in oracle["intervals"].as_array().unwrap() {
        let interval: Timeframe = serde_json::from_value(case["interval"].clone()).unwrap();
        match resolve_lighter_interval(&interval) {
            Ok(actual) => assert_eq!(
                actual,
                case["ok"].as_str().unwrap(),
                "resolveLighterInterval {case}"
            ),
            Err(actual) => assert_eq!(
                actual,
                case["error"].as_str().unwrap(),
                "resolveLighterInterval {case}"
            ),
        }
    }
    for case in oracle["topics"].as_array().unwrap() {
        let resolution = case["resolution"].as_str().unwrap();
        let (candle, trade) = if let Some(id) = case["marketId"].as_str() {
            (
                build_lighter_candle_topic(id, resolution),
                build_lighter_trades_topic(id),
            )
        } else {
            let id = number(&case["marketId"]);
            (
                build_lighter_candle_topic(id, resolution),
                build_lighter_trades_topic(id),
            )
        };
        assert_eq!(
            candle,
            case["candle"].as_str().unwrap(),
            "buildLighterCandleTopic {case}"
        );
        assert_eq!(
            trade,
            case["trade"].as_str().unwrap(),
            "buildLighterTradesTopic {case}"
        );
    }
    for case in oracle["recoveries"].as_array().unwrap() {
        assert_eq!(
            should_attempt_lighter_ohlcv_recovery(
                case["hasTopicHandlers"].as_bool().unwrap(),
                number(&case["nowMs"]),
                number(&case["lastRecoveryReconnectAtMs"]),
                number(&case["recoveryCooldownMs"])
            ),
            case["expected"].as_bool().unwrap(),
            "shouldAttemptLighterOhlcvRecovery {case}"
        );
    }
    for case in oracle["promotions"].as_array().unwrap() {
        assert_eq!(
            should_promote_lighter_ohlcv_recovery_to_proxy(
                case["isWsProxyRouteEnabled"].as_bool().unwrap(),
                case["useProxyRoute"].as_bool().unwrap(),
                case["connectedViaProxyRoute"].as_bool().unwrap()
            ),
            case["expected"].as_bool().unwrap(),
            "shouldPromoteLighterOhlcvRecoveryToProxy {case}"
        );
    }
    for case in oracle["dispatch"].as_array().unwrap() {
        assert_eq!(
            should_dispatch_lighter_public_message(
                case["topic"].as_str().unwrap(),
                case["data"].as_str().unwrap()
            ),
            case["expected"].as_bool().unwrap(),
            "shouldDispatchLighterPublicMessage {case}"
        );
    }
    for case in oracle["numbers"].as_array().unwrap() {
        let actual = parse_lighter_ws_number(decode(&case["values"]).as_array().unwrap());
        assert_eq!(
            actual,
            decode(&case["expected"]["value"]).as_f64(),
            "parseLighterWsNumber {case}"
        );
        assert_eq!(
            actual.is_some_and(|v| v == 0.0 && v.is_sign_negative()),
            case["expected"]["negativeZero"].as_bool().unwrap(),
            "parseLighterWsNumber signed zero {case}"
        );
    }
    for case in oracle["books"].as_array().unwrap() {
        let actual = parse_lighter_ws_book_side_price(&decode(&case["value"]));
        assert_eq!(
            actual,
            decode(&case["expected"]["value"]).as_f64(),
            "parseLighterWsBookSidePrice {case}"
        );
        assert_eq!(
            actual.is_some_and(|v| v == 0.0 && v.is_sign_negative()),
            case["expected"]["negativeZero"].as_bool().unwrap(),
            "parseLighterWsBookSidePrice signed zero {case}"
        );
    }
    for case in oracle["channels"].as_array().unwrap() {
        assert_eq!(
            extract_lighter_ticker_market_id_from_channel(case["channel"].as_str().unwrap()),
            case["expected"].as_str(),
            "extractLighterTickerMarketIdFromChannel {case}"
        );
    }
    for case in oracle["approvalNumbers"].as_array().unwrap() {
        let input = case.get("input").map(decode);
        match builder_approval::to_number(input.as_ref()) {
            Err(error) => assert_eq!(
                serde_json::json!({"name":error.name,"message":error.message}),
                case["error"],
                "toNumber {case}"
            ),
            Ok(actual) => {
                assert!(
                    case.get("error").is_none(),
                    "toNumber expected error {case}"
                );
                let expected = case.get("ok").filter(|v| !v.is_null()).map(number);
                assert_eq!(
                    actual.map(f64::to_bits),
                    expected.map(f64::to_bits),
                    "toNumber {case}"
                );
            }
        }
    }
}

#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn bigint_oracle_rejects_live_source_numeric_coercion_mutation() {
    for (mutation, symbol) in [
        ("bigint-number", "stringToBigInt"),
        // Source cardinality fails before native identity for this precedence mutant.
        (
            "rest-book-precedence",
            "REST-CARDINALITY source books/bids/asks/tickers totals",
        ),
        (
            "rest-book-deep",
            "REST-CARDINALITY source books/bids/asks/tickers totals",
        ),
        (
            "rest-ticker-deep",
            "REST-CARDINALITY source books/bids/asks/tickers totals",
        ),
        ("rest-ticker-order", "parseRestTickerItems"),
        ("backoff-min", "calculateExponentialBackoffDelay"),
        ("public-rate-max", "calculateLighterPublicReconnectDelay"),
        ("private-rate-max", "calculateLighterPrivateReconnectDelay"),
        ("funding-floor", "normalizeLighterFundingRate"),
        (
            "approval-message-label",
            "buildLighterApproveIntegratorL1Message",
        ),
        ("scale-round-trunc", "toLighterScaledInt"),
        ("approval-bigint-direct", "toBigInt"),
        ("approval-hex-width", "toHex10"),
    ] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "lighter_pure_helpers_match_live_typescript",
                "--ignored",
                "--nocapture",
            ])
            .env("LIGHTER_ORACLE_MUTATION", mutation)
            .output()
            .unwrap();
        let diagnostic = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(101), "{diagnostic}");
        assert!(diagnostic.contains(symbol), "{diagnostic}");
        assert!(
            diagnostic.contains("assertion `left == right` failed"),
            "{diagnostic}"
        );
    }
}

#[test]
#[ignore = "requires Node"]
fn every_live_oracle_fails_loudly_without_its_compiler() {
    let scripts = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts");
    for name in [
        "approval-types-oracle.cjs",
        "builder-fee-oracle.cjs",
        "capture-signing.cjs",
        "exchange-helpers-oracle.cjs",
        "oracle.cjs",
        "pure-oracle.cjs",
        "signer-helpers-oracle.cjs",
        "signer-types-oracle.cjs",
        "tier-oracle.cjs",
        "utils-coercion-oracle.cjs",
    ] {
        let output = Command::new("node")
            .arg(scripts.join(name))
            .env(
                "SAFE_CEX_TYPESCRIPT_PATH",
                scripts.join("deliberately-missing-typescript"),
            )
            .output()
            .unwrap();
        assert!(!output.status.success(), "{name} silently skipped compiler");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("MODULE_NOT_FOUND"),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
#[ignore = "requires Node"]
fn pow_capture_canonicalizes_only_nan() {
    let output = Command::new("node")
        .args(["-e", r#"
const fs=require('node:fs'),vm=require('node:vm');
let line=fs.readFileSync(process.argv[1],'utf8').split('\n').find(l=>l.startsWith('const hex ='));
if(process.env.LIGHTER_POW_CAPTURE_MUTATION==='zero')line=line.replace('writeDoubleBE(x)','writeDoubleBE(Object.is(x,-0)?0:x)');
if(process.env.LIGHTER_POW_CAPTURE_MUTATION==='finite')line=line.replace('writeDoubleBE(x)','writeDoubleBE(Number.isFinite(x)&&x!==0?0:x)');
const hex=vm.runInNewContext(line+'\nhex',{Buffer});
const payload=Buffer.from('fff8123456789abc','hex').readDoubleBE();
process.stdout.write(JSON.stringify([['NaN',hex(NaN)],['negative NaN',hex(-NaN)],['payload NaN',hex(payload)],['signed zero',hex(-0)],['positive zero',hex(0)],['finite',hex(1.25)],['infinity',hex(Infinity)]]));
"#])
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/probe-powf.cjs"))
        .output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Value = serde_json::from_slice(&output.stdout).unwrap();
    for (i, (name, bits)) in [
        ("NaN", "7ff8000000000000"),
        ("negative NaN", "7ff8000000000000"),
        ("payload NaN", "7ff8000000000000"),
        ("signed zero", "8000000000000000"),
        ("positive zero", "0000000000000000"),
        ("finite", "3ff4000000000000"),
        ("infinity", "7ff0000000000000"),
    ]
    .iter()
    .enumerate()
    {
        assert_eq!(rows[i][0], *name, "pow capture case inventory");
        assert_eq!(rows[i][1], *bits, "pow capture {name} exact");
    }
    assert_eq!(rows.as_array().unwrap().len(), 7, "pow capture inventory");
}
#[test]
#[ignore = "requires Node"]
fn pow_capture_zero_and_finite_controls_fail_by_name() {
    for (mutation, witness) in [
        ("zero", "pow capture signed zero exact"),
        ("finite", "pow capture finite exact"),
    ] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "pow_capture_canonicalizes_only_nan",
                "--ignored",
                "--nocapture",
            ])
            .env("LIGHTER_POW_CAPTURE_MUTATION", mutation)
            .output()
            .unwrap();
        let log = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(101), "{log}");
        assert!(log.contains(witness), "{log}");
    }
}

#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn source_cardinality_controls_fail_by_name() {
    for mutation in ["empty", "subset"] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "lighter_pure_helpers_match_live_typescript",
                "--ignored",
                "--nocapture",
            ])
            .env("LIGHTER_PURE_CARDINALITY_MUTATION", mutation)
            .output()
            .unwrap();
        let log = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(101), "{log}");
        assert!(
            log.contains("PURE-CARDINALITY publicRestPayloads source cases"),
            "{log}"
        );
    }
}
