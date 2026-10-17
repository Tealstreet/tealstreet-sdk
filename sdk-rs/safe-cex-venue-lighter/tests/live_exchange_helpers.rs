use safe_cex_venue_lighter::exchange_helpers::*;
use safe_cex_venue_lighter::types::LighterQuoteCurrency;
use serde_json::Value;
use std::process::Command;

fn decode(value: &Value) -> Value {
    if value.get("$number").and_then(Value::as_str) == Some("-0") {
        return serde_json::json!(-0.0);
    }
    match value {
        Value::Object(values) => {
            Value::Object(values.iter().map(|(k, v)| (k.clone(), decode(v))).collect())
        }
        Value::Array(values) => Value::Array(values.iter().map(decode).collect()),
        _ => value.clone(),
    }
}
fn number(value: f64) -> Value {
    if !value.is_finite() || value == 0.0 && value.is_sign_negative() {
        serde_json::json!({"$number": if value==0.0 {"-0".to_owned()} else {safe_cex_num::js_number_string(value)}})
    } else {
        serde_json::from_str(&safe_cex_num::js_number_string(value)).unwrap()
    }
}
fn tagged_number(value: &Value) -> f64 {
    match value.get("$number").and_then(Value::as_str) {
        Some("NaN") => f64::NAN,
        Some("Infinity") => f64::INFINITY,
        Some("-Infinity") => f64::NEG_INFINITY,
        Some("-0") => -0.0,
        None => value.as_f64().unwrap(),
        tag => panic!("unknown oracle tag {tag:?}"),
    }
}
fn placement(value: &Value) -> safe_cex_types::PlaceOrderOpts {
    let mut finite = value.clone();
    for field in ["price", "amount", "stopLoss", "takeProfit"] {
        if finite
            .get(field)
            .is_some_and(|v| v.get("$number").is_some())
        {
            finite[field] = serde_json::json!(0);
        }
    }
    let mut order: safe_cex_types::PlaceOrderOpts = serde_json::from_value(finite).unwrap();
    order.price = value.get("price").map(tagged_number);
    order.amount = tagged_number(&value["amount"]);
    order.stop_loss = value.get("stopLoss").map(tagged_number);
    order.take_profit = value.get("takeProfit").map(tagged_number);
    order
}
fn outcome(result: Result<Value, safe_cex_num::JsCoercionError>) -> Value {
    match result {
        Ok(value) => serde_json::json!({"ok":value}),
        Err(error) => serde_json::json!({"error":{"name":error.name,"message":error.message}}),
    }
}
#[test]
#[ignore = "requires Node and the maintained TypeScript compiler"]
fn lighter_exchange_helpers_match_live_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/exchange-helpers-oracle.cjs"
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
        ("referencePrices", 2910),
        ("referenceStringBoundaries", 3),
        ("inferredStopLoss", 1672),
        ("fallbackKeys", 72),
        ("configuredMargins", 1631),
        ("marginMessages", 4893),
        ("stopSlippageCases", 533),
        ("rateCases", 2112),
        ("reduceSides", 2),
        ("reduceMessages", 8),
        ("positionChecks", 768),
        ("minimumReasons", 6912),
        ("accountIndices", 54),
        ("addressCandidates", 137),
        ("cases", 102),
        ("fractions", 35),
        ("markets", 560),
        ("leverages", 420),
        ("wasmIssues", 19),
        ("orderErrors", 2979),
        ("priceCandidates", 364),
        ("batches", 175),
        ("leverageTiers", 417),
        ("tickDelays", 12),
        ("marginModes", 58),
        ("marginToTypes", 4),
        ("typesToMargin", 3),
    ] {
        assert_eq!(
            oracle[name].as_array().unwrap().len(),
            count,
            "{name} scalar/opaque corpus inventory"
        );
    }

    for case in oracle["referencePrices"].as_array().unwrap() {
        let ticker = case.get("ticker").map(|p| {
            let mut clean = p.clone();
            for key in ["bid", "ask", "mark", "last", "index"] {
                clean[key] = serde_json::json!(0);
            }
            let mut t: safe_cex_types::Ticker = serde_json::from_value(clean).unwrap();
            t.bid = tagged_number(&p["bid"]);
            t.ask = tagged_number(&p["ask"]);
            t.mark = tagged_number(&p["mark"]);
            t.last = tagged_number(&p["last"]);
            t.index = tagged_number(&p["index"]);
            t
        });
        let actual = match resolve_market_order_reference_price(
            case["symbol"].as_str().unwrap(),
            serde_json::from_value(case["side"].clone()).unwrap(),
            ticker.as_ref(),
        ) {
            Ok(price) => serde_json::json!({"ok":number(price)}),
            Err(error) => serde_json::json!({"error":{"name":error.name,"message":error.message}}),
        };
        assert_eq!(
            actual, case["expected"],
            "resolveMarketOrderReferencePrice {case}"
        );
    }
    // CHOSEN sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary.
    let boundary_cases = oracle["referenceStringBoundaries"].as_array().unwrap();
    assert_eq!(boundary_cases.len(), 3);
    for case in boundary_cases {
        assert!(
            serde_json::from_str::<String>(case["inputJson"].as_str().unwrap()).is_err(),
            "resolveMarketOrderReferencePrice representation refusal {case}"
        );
        assert_eq!(case["errorName"], "Error");
        let units: Vec<u16> = serde_json::from_value(case["messageUnits"].clone()).unwrap();
        assert!(
            String::from_utf16(&units).is_err(),
            "TS preserves symbol units in error"
        );
    }
    for case in oracle["inferredStopLoss"].as_array().unwrap() {
        let mut order_json = case["order"].clone();
        order_json["price"] = serde_json::json!(0);
        let mut order: safe_cex_types::Order = serde_json::from_value(order_json).unwrap();
        order.price = (tagged_number(&case["order"]["price"])).into();
        let ticker = case.get("ticker").map(|p| {
            let mut clean = p.clone();
            for key in ["mark", "last", "index"] {
                clean[key] = serde_json::json!(0);
            }
            let mut t: safe_cex_types::Ticker = serde_json::from_value(clean).unwrap();
            t.mark = tagged_number(&p["mark"]);
            t.last = tagged_number(&p["last"]);
            t.index = tagged_number(&p["index"]);
            t
        });
        assert_eq!(
            serde_json::json!(infer_stop_loss_type_from_reduce_only_limit(
                &order,
                ticker.as_ref()
            )),
            case["expected"],
            "inferStopLossTypeFromReduceOnlyLimit {case}"
        );
    }
    for case in oracle["fallbackKeys"].as_array().unwrap() {
        let interval = serde_json::from_value(case["interval"].clone()).unwrap();
        assert_eq!(
            serde_json::json!(get_ohlcv_fallback_key(
                case["symbol"].as_str().unwrap(),
                interval
            )),
            case["expected"],
            "getOhlcvFallbackKey {case}"
        );
    }
    for case in oracle["configuredMargins"].as_array().unwrap() {
        let market: Option<safe_cex_types::Market> = case
            .get("market")
            .map(|v| serde_json::from_value(decode(v)).unwrap());
        let actual = outcome(
            get_market_configured_margin_mode(market.as_ref())
                .map(|v| v.map(|m| number(f64::from(m))).unwrap_or(Value::Null)),
        );
        assert_eq!(
            actual, case["expected"],
            "getMarketConfiguredMarginMode {case}"
        );
    }
    for case in oracle["marginMessages"].as_array().unwrap() {
        let market: Option<safe_cex_types::Market> = case
            .get("market")
            .map(|v| serde_json::from_value(decode(v)).unwrap());
        let actual = outcome(
            build_invalid_margin_mode_order_message(
                case["symbol"].as_str().unwrap(),
                market.as_ref(),
            )
            .map(|v| serde_json::json!(v)),
        );
        assert_eq!(
            actual, case["expected"],
            "buildInvalidMarginModeOrderMessage {case}"
        );
    }
    for case in oracle["stopSlippageCases"].as_array().unwrap() {
        let fallback = case.get("fallback").map(decode);
        let explicit = case.get("explicit").map(tagged_number);
        let actual = outcome(resolve_stop_slippage_pct(explicit, fallback.as_ref()).map(number));
        assert_eq!(actual, case["expected"], "resolveStopSlippagePct {case}");
    }
    for case in oracle["rateCases"].as_array().unwrap() {
        let error = case.get("error").map(decode);
        let actual = outcome(
            is_rate_limit_order_error(error.as_ref(), case["message"].as_str().unwrap())
                .map(|v| serde_json::json!(v)),
        );
        assert_eq!(actual, case["expected"], "isRateLimitOrderError {case}");
    }
    for case in oracle["reduceSides"].as_array().unwrap() {
        let side = serde_json::from_value(case["side"].clone()).unwrap();
        assert_eq!(
            serde_json::json!(get_reduce_only_closing_position_side(side)),
            case["expected"],
            "getReduceOnlyClosingPositionSide {case}"
        );
    }
    for case in oracle["reduceMessages"].as_array().unwrap() {
        let side = serde_json::from_value(case["side"].clone()).unwrap();
        assert_eq!(
            serde_json::json!(build_no_position_reduce_only_message(
                case["symbol"].as_str().unwrap(),
                side
            )),
            case["expected"],
            "buildNoPositionReduceOnlyMessage {case}"
        );
    }
    for case in oracle["positionChecks"].as_array().unwrap() {
        let side = serde_json::from_value(case["side"].clone()).unwrap();
        let positions: Vec<safe_cex_types::Position> = case["positions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                let mut clean = p.clone();
                clean["contracts"] = serde_json::json!(0);
                let mut position: safe_cex_types::Position = serde_json::from_value(clean).unwrap();
                position.contracts = tagged_number(&p["contracts"]);
                position
            })
            .collect();
        assert_eq!(
            serde_json::json!(has_open_reduce_only_target_position(
                case["symbol"].as_str().unwrap(),
                side,
                &positions
            )),
            case["expected"],
            "hasOpenReduceOnlyTargetPosition {case}"
        );
    }
    for case in oracle["minimumReasons"].as_array().unwrap() {
        assert_eq!(
            serde_json::json!(get_order_minimum_violation_reasons(
                tagged_number(&case["amount"]),
                tagged_number(&case["executionPrice"]),
                tagged_number(&case["minBaseAmount"]),
                tagged_number(&case["minQuoteAmount"])
            )),
            case["expected"],
            "getOrderMinimumViolationReasons {case}"
        );
    }
    for case in oracle["accountIndices"].as_array().unwrap() {
        use safe_cex_venue_lighter::builder_approval::LighterApprovalValue;
        let big = case["input"]
            .get("$bigint")
            .map(|v| v.as_str().unwrap().parse::<num_bigint::BigInt>().unwrap());
        let decoded = case.get("input").map(decode);
        let input = if let Some(ref big) = big {
            LighterApprovalValue::BigInt(big)
        } else if case["input"].is_number() || case["input"].get("$number").is_some() {
            LighterApprovalValue::Number(tagged_number(&case["input"]))
        } else {
            LighterApprovalValue::Json(decoded.as_ref())
        };
        assert_eq!(
            serde_json::json!(parse_account_index(input).map(|v| v.to_string())),
            case["expected"],
            "parseAccountIndex {case}"
        );
    }
    for case in oracle["addressCandidates"].as_array().unwrap() {
        assert_eq!(
            serde_json::json!(extract_l1_address_candidate(
                case.get("input").and_then(Value::as_str)
            )),
            case["expected"],
            "extractL1AddressCandidate {case}"
        );
    }

    for case in oracle["cases"].as_array().unwrap() {
        let default_quote: Option<LighterQuoteCurrency> = case
            .get("defaultQuote")
            .map(|v| serde_json::from_value(v.clone()).unwrap());
        let (base, quote) =
            parse_lighter_market_symbol(case["symbol"].as_str().unwrap(), default_quote);
        assert_eq!(
            serde_json::json!({"base":base,"quote":quote}),
            case["expected"],
            "parseLighterMarketSymbol {case}"
        );
    }
    for case in oracle["fractions"].as_array().unwrap() {
        let value = case.get("value").map(decode);
        assert_eq!(
            outcome(
                parse_lighter_margin_fraction_ratio(value.as_ref())
                    .map(|v| v.map(number).unwrap_or(Value::Null))
            ),
            case["ratio"],
            "parseLighterMarginFractionRatio {case}"
        );
        assert_eq!(
            outcome(
                normalize_lighter_stop_slippage_pct(value.as_ref())
                    .map(|v| v.map(number).unwrap_or(Value::Null))
            ),
            case["slippage"],
            "normalizeLighterStopSlippagePct {case}"
        );
    }
    for case in oracle["markets"].as_array().unwrap() {
        assert_eq!(
            outcome(is_lighter_perp_market(&decode(&case["market"])).map(Value::Bool)),
            case["expected"],
            "isLighterPerpMarket {case}"
        );
    }
    for case in oracle["leverages"].as_array().unwrap() {
        assert_eq!(
            outcome(get_lighter_market_leverage_max(&decode(&case["market"])).map(number)),
            case["expected"],
            "getLighterMarketLeverageMax {case}"
        );
    }
    for case in oracle["wasmIssues"].as_array().unwrap() {
        assert_eq!(
            is_wasm_runtime_issue_message(case["message"].as_str().unwrap()),
            case["expected"].as_bool().unwrap(),
            "LighterExchange.isWasmRuntimeIssueMessage {case}"
        );
    }
    for case in oracle["orderErrors"].as_array().unwrap() {
        let error = case.get("error");
        let message = case["message"].as_str().unwrap();
        assert_eq!(
            outcome(is_invalid_margin_mode_order_error(error, message).map(Value::Bool)),
            case["margin"],
            "LighterExchange.isInvalidMarginModeOrderError {case}"
        );
        assert_eq!(
            outcome(is_partner_attribution_tier_order_error(error, message).map(Value::Bool)),
            case["tier"],
            "LighterExchange.isPartnerAttributionTierOrderError {case}"
        );
        assert_eq!(
            outcome(
                is_partner_attribution_approval_missing_order_error(error, message)
                    .map(Value::Bool)
            ),
            case["approval"],
            "LighterExchange.isPartnerAttributionApprovalMissingOrderError {case}"
        );
    }
    for case in oracle["priceCandidates"].as_array().unwrap() {
        let values: Vec<_> = case["values"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                if v.get("$undefined").is_some() {
                    None
                } else {
                    Some(tagged_number(v))
                }
            })
            .collect();
        assert_eq!(
            get_positive_price_candidate(&values)
                .map(number)
                .unwrap_or(Value::Null),
            case["expected"],
            "LighterExchange.getPositivePriceCandidate {case}"
        );
    }
    for case in oracle["batches"].as_array().unwrap() {
        let orders: Vec<_> = case["orders"]
            .as_array()
            .unwrap()
            .iter()
            .map(placement)
            .collect();
        assert_eq!(
            should_use_batched_create_orders(&orders),
            case["expected"].as_bool().unwrap(),
            "LighterExchange.shouldUseBatchedCreateOrders {case}"
        );
    }
    for case in oracle["leverageTiers"].as_array().unwrap() {
        assert_eq!(
            number(snap_to_leverage_tier(tagged_number(&case["requested"]))),
            case["expected"],
            "LighterExchange.snapToLeverageTier {case}"
        );
    }
    for case in oracle["tickDelays"].as_array().unwrap() {
        assert_eq!(
            number(get_initial_tick_delay(tagged_number(&case["interval"]))),
            case["expected"],
            "LighterExchange.getInitialTickDelay {case}"
        );
    }
    for case in oracle["marginModes"].as_array().unwrap() {
        let value = case.get("value").map(decode);
        assert_eq!(
            outcome(
                parse_margin_mode(value.as_ref())
                    .map(|v| v.map(|v| number(f64::from(v))).unwrap_or(Value::Null))
            ),
            case["expected"],
            "LighterExchange.parseMarginMode {case}"
        );
    }
    for case in oracle["marginToTypes"].as_array().unwrap() {
        let mode = case.get("value").map(|v| {
            safe_cex_venue_lighter::signer_types::LighterSignerMarginMode::try_from(tagged_number(
                v,
            ))
            .unwrap()
        });
        assert_eq!(
            serde_json::to_value(margin_mode_to_margin_type(mode)).unwrap(),
            case["expected"],
            "LighterExchange.marginModeToMarginType {case}"
        );
    }
    for case in oracle["typesToMargin"].as_array().unwrap() {
        let kind = case
            .get("value")
            .map(|v| serde_json::from_value(v.clone()).unwrap());
        assert_eq!(
            number(f64::from(margin_type_to_margin_mode(kind))),
            case["expected"],
            "LighterExchange.marginTypeToMarginMode {case}"
        );
    }
}

#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn account_helpers_reject_source_mutations() {
    for (mutation, name) in [
        ("account-direct", "parseAccountIndex"),
        ("reference-side", "resolveMarketOrderReferencePrice"),
        ("stop-loss-tie", "inferStopLossTypeFromReduceOnlyLimit"),
        ("ohlcv-key", "getOhlcvFallbackKey"),
        ("margin-precedence", "getMarketConfiguredMarginMode"),
        ("margin-message", "buildInvalidMarginModeOrderMessage"),
        ("slippage-or", "resolveStopSlippagePct"),
        ("rate-status-float", "isRateLimitOrderError"),
        ("minimum-inclusive", "getOrderMinimumViolationReasons"),
        ("position-zero", "hasOpenReduceOnlyTargetPosition"),
        ("closing-side", "getReduceOnlyClosingPositionSide"),
        ("reduce-message", "buildNoPositionReduceOnlyMessage"),
        ("address-case", "extractL1AddressCandidate"),
    ] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "lighter_exchange_helpers_match_live_typescript",
                "--ignored",
                "--nocapture",
            ])
            .env("LIGHTER_EXCHANGE_ORACLE_MUTATION", mutation)
            .output()
            .unwrap();
        let diagnostic = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(101), "{diagnostic}");
        assert!(
            diagnostic.contains(name) && diagnostic.contains("assertion `left == right` failed"),
            "{diagnostic}"
        );
    }
}
