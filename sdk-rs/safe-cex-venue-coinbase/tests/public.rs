use safe_cex_venue_coinbase::starbase::CoinbaseStarbaseProtocolError;
use safe_cex_venue_coinbase::starbase_public::*;
use serde_json::{Value, json};
use std::{process::Command, sync::OnceLock};
#[path = "support/compare.rs"]
mod comparison;
#[path = "support/schema.rs"]
mod schema;
fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/public.cjs"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        for (name, expected) in [
            ("subscriptionCases", 632),
            ("acknowledgementCases", 19_592),
            ("channelBuilderCases", 59_377),
            ("channelTemplates", 4),
            ("channelCases", 158),
            ("sequencedCases", 2_405),
            ("resolutionCases", 29),
            ("chartRequestCases", 1_189),
            ("marketParityCases", 104),
            ("requestCases", 24_553),
            ("deltaCases", 4_170),
            ("requestSamples", 2),
            ("restLevelCases", 912),
            ("bookCases", 635),
            ("candleCases", 1_098),
            ("chartPayloadCases", 231),
            ("chartCases", 417),
            ("instrumentCases", 1_561),
            ("instrumentListCases", 4_763),
            ("stepCases", 123),
            ("tickerCases", 1_560),
            ("tickerPayloadCases", 1_585),
            ("cases", 864),
            ("finite", 1_440),
            ("nameCases", 3_515),
            ("errors", 6),
        ] {
            assert_eq!(
                value[name].as_array().map(Vec::len),
                Some(expected),
                "Coinbase public oracle source total {name}"
            );
        }
        value
    })
}
#[test]
fn public_data_literal_declarations() {
    for (name, expected) in oracle()["schemas"].as_object().unwrap() {
        assert_eq!(
            schema::wire_schema(include_str!("../src/starbase_public.rs"), name),
            *expected,
            "{name}"
        );
    }
}
#[test]
fn public_data_literal_serialization() {
    let actual = json!({
        "CoinbaseStarbaseSubscriptionMethod": [CoinbaseStarbaseSubscriptionMethod::Subscribe,CoinbaseStarbaseSubscriptionMethod::Unsubscribe],
        "CoinbaseStarbaseInstrumentStateCurrency": [CoinbaseStarbaseInstrumentStateCurrency::Btc,CoinbaseStarbaseInstrumentStateCurrency::Eth,CoinbaseStarbaseInstrumentStateCurrency::Usdc,CoinbaseStarbaseInstrumentStateCurrency::Any],
        "CoinbaseStarbaseMarketParityStatus": [CoinbaseStarbaseMarketParityStatus::Blocked],
        "StarbaseBookAction": [StarbaseBookAction::New,StarbaseBookAction::Change,StarbaseBookAction::Delete],
        "CoinbaseStarbaseBookFrameType": [CoinbaseStarbaseBookFrameType::Snapshot,CoinbaseStarbaseBookFrameType::Change],
        "CoinbaseStarbaseOrderBookDepth": [CoinbaseStarbaseOrderBookDepth::D1,CoinbaseStarbaseOrderBookDepth::D5,CoinbaseStarbaseOrderBookDepth::D10,CoinbaseStarbaseOrderBookDepth::D20,CoinbaseStarbaseOrderBookDepth::D50,CoinbaseStarbaseOrderBookDepth::D100,CoinbaseStarbaseOrderBookDepth::D1000,CoinbaseStarbaseOrderBookDepth::D10000],
        "CoinbaseStarbaseUsdc": [CoinbaseStarbaseUsdc::Usdc],
        "CoinbaseStarbaseFuture": [CoinbaseStarbaseFuture::Future],
        "CoinbaseStarbaseLinear": [CoinbaseStarbaseLinear::Linear],
        "CoinbaseStarbasePerpetual": [CoinbaseStarbasePerpetual::Perpetual],
        "CoinbaseStarbaseBaseCurrency": [CoinbaseStarbaseBaseCurrency::Btc, CoinbaseStarbaseBaseCurrency::Eth],
        "CoinbaseStarbaseBookState": [CoinbaseStarbaseBookState::Open, CoinbaseStarbaseBookState::Settlement, CoinbaseStarbaseBookState::Delivered, CoinbaseStarbaseBookState::Inactive, CoinbaseStarbaseBookState::Locked, CoinbaseStarbaseBookState::Halted, CoinbaseStarbaseBookState::Archivized],
        "CoinbaseStarbaseBookInterval": [CoinbaseStarbaseBookInterval::V100Ms, CoinbaseStarbaseBookInterval::Agg2],
        "CoinbaseStarbaseChartResolution": [CoinbaseStarbaseChartResolution::V1, CoinbaseStarbaseChartResolution::V3, CoinbaseStarbaseChartResolution::V5, CoinbaseStarbaseChartResolution::V15, CoinbaseStarbaseChartResolution::V30, CoinbaseStarbaseChartResolution::V60, CoinbaseStarbaseChartResolution::V120, CoinbaseStarbaseChartResolution::V360, CoinbaseStarbaseChartResolution::V720, CoinbaseStarbaseChartResolution::V1D],
        "CoinbaseStarbaseMarketParityBlocker": [CoinbaseStarbaseMarketParityBlocker::AmountIncrementUnpublished, CoinbaseStarbaseMarketParityBlocker::MaximumBaseAmountUnpublished],
    });
    for (name, values) in actual.as_object().unwrap() {
        assert_eq!(*values, oracle()["schemas"][name]["values"], "{name}");
    }
    let parsed = syn::parse_file(include_str!("../src/starbase_public.rs")).unwrap();
    let names: std::collections::BTreeSet<_> = parsed
        .items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Enum(e) if e.ident != "CoinbaseStarbasePublicChannel" => {
                Some(e.ident.to_string())
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        names,
        oracle()["schemas"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect()
    );
}

fn capture<T: serde::Serialize>(result: Result<T, CoinbaseStarbaseProtocolError>) -> Value {
    match result {
        Ok(v) => json!({"ok":v}),
        Err(e) => json!({"error":e}),
    }
}
#[test]
fn public_data_guard_results_and_errors() {
    for case in oracle()["cases"].as_array().unwrap() {
        let value = case.get("input");
        let context = case["context"].as_str().unwrap();
        let actual = match case["name"].as_str().unwrap() {
            "isRecord" => json!({"ok":is_record(value)}),
            "requireRecord" => capture(require_record(value, context)),
            "requireString" => capture(require_string(value, context)),
            "requirePositive" => capture(require_positive(value, context)),
            "requirePositiveInteger" => capture(require_positive_integer(value, context)),
            "requireTimestamp" => capture(require_timestamp(value, context)),
            name => panic!("unhandled source guard {name}"),
        };
        comparison::compare(&actual, &case["result"], &case.to_string());
    }
    for case in oracle()["errors"].as_array().unwrap() {
        assert_eq!(
            capture(protocol_error::<()>(case["context"].as_str().unwrap())),
            case["result"],
            "{case}"
        );
    }
}
#[test]
fn public_data_finite_minimum_and_js_number_spelling() {
    for case in oracle()["finite"].as_array().unwrap() {
        let minimum = match case["minimum"].as_str() {
            Some("default") => None,
            Some("NaN") => Some(f64::NAN),
            Some("Infinity") => Some(f64::INFINITY),
            Some("-Infinity") => Some(f64::NEG_INFINITY),
            None => Some(case["minimum"].as_f64().unwrap()),
            other => panic!("unknown minimum {other:?}"),
        };
        comparison::compare(
            &capture(require_finite(
                case.get("input"),
                case["context"].as_str().unwrap(),
                minimum,
            )),
            &case["result"],
            &case.to_string(),
        );
    }
}

#[test]
fn public_data_names_and_literal_parsers_reject_widening() {
    for case in oracle()["nameCases"].as_array().unwrap() {
        let value = case.get("input");
        let context = case["context"].as_str().unwrap();
        let actual = match case["name"].as_str().unwrap() {
            "parseBookState" => capture(parse_book_state(value, context)),
            "parseBaseCurrency" => capture(parse_base_currency(value, context)),
            "parseStarbasePerpetualName" => capture(parse_starbase_perpetual_name(value)),
            "toCoinbaseStarbaseInstrumentName" => capture(to_coinbase_starbase_instrument_name(
                value.unwrap().as_str().unwrap(),
            )),
            "toCoinbaseLegacyIntxSymbol" => capture(to_coinbase_legacy_intx_symbol(
                value.unwrap().as_str().unwrap(),
            )),
            name => panic!("unhandled parser {name}"),
        };
        assert_eq!(actual, case["result"], "{case}");
    }
}

#[test]
fn public_ticker_declaration_and_complete_mapping() {
    assert_eq!(
        schema::wire_schema(
            include_str!("../src/starbase_public.rs"),
            "CoinbaseStarbasePublicTicker"
        ),
        oracle()["recordSchemas"]["CoinbaseStarbasePublicTicker"]
    );
    let supplement = &oracle()["tickerIdentitySupplement"];
    assert_eq!(supplement["label"], "field-identity-supplement");
    let ticker = normalize_coinbase_starbase_ticker(Some(&supplement["input"]))
        .expect("identity supplement ticker");
    assert_eq!(ticker.timestamp_ms, 1234.0, "timestamp field identity");
    assert_eq!(ticker.bid, 10.01, "bid field identity");
    assert_eq!(ticker.bid_base_amount, 0.11, "bid amount field identity");
    assert_eq!(ticker.ask, 10.02, "ask field identity");
    assert_eq!(ticker.ask_base_amount, 0.22, "ask amount field identity");
    assert_eq!(ticker.last, 10.03, "last field identity");
    assert_eq!(ticker.mark, 10.04, "mark field identity");
    assert_eq!(ticker.index, 10.05, "index field identity");
    assert_eq!(ticker.percentage, Some(0.06), "percentage field identity");
    assert_eq!(ticker.funding_rate, Some(0.0007), "funding field identity");
    assert_eq!(
        ticker.funding_rate_8h,
        Some(0.0008),
        "8h funding field identity"
    );
    assert_eq!(ticker.volume_base, 0.9, "volume field identity");
    assert_eq!(ticker.volume_usd, Some(1.01), "USD volume field identity");
    assert_eq!(
        ticker.open_interest_base, 2.02,
        "open interest field identity"
    );
    comparison::compare(
        &capture(Ok(ticker)),
        &supplement["result"],
        "ticker field identity supplement result",
    );
    for case in oracle()["tickerCases"].as_array().unwrap() {
        comparison::compare(
            &capture(normalize_coinbase_starbase_ticker(case.get("input"))),
            &case["result"],
            &format!("ticker {case}"),
        );
    }
    for case in oracle()["tickerPayloadCases"].as_array().unwrap() {
        comparison::compare(
            &capture(normalize_coinbase_starbase_ticker_payload(
                case.get("input"),
            )),
            &case["result"],
            &format!("ticker payload {case}"),
        );
    }
}

#[test]
fn public_instrument_declaration_and_complete_mapping() {
    assert_eq!(
        schema::wire_schema(
            include_str!("../src/starbase_public.rs"),
            "CoinbaseStarbasePublicInstrument"
        ),
        oracle()["recordSchemas"]["CoinbaseStarbasePublicInstrument"]
    );
    let supplement = &oracle()["instrumentIdentitySupplement"];
    assert_eq!(supplement["label"], "field-identity-supplement");
    let instrument = normalize_coinbase_starbase_instrument(Some(&supplement["input"]))
        .expect("identity supplement instrument");
    assert_eq!(
        instrument.quote,
        CoinbaseStarbaseUsdc::Usdc,
        "quote identity"
    );
    assert_eq!(
        instrument.counter,
        CoinbaseStarbaseUsdc::Usdc,
        "counter identity"
    );
    assert_eq!(
        instrument.settlement,
        CoinbaseStarbaseUsdc::Usdc,
        "settlement identity"
    );
    assert_eq!(
        instrument.kind,
        CoinbaseStarbaseFuture::Future,
        "kind identity"
    );
    assert_eq!(
        instrument.instrument_type,
        CoinbaseStarbaseLinear::Linear,
        "instrument type identity"
    );
    assert_eq!(
        instrument.minimum_base_amount, 0.0002,
        "minTradeAmount field identity"
    );
    assert_eq!(
        instrument.contract_size, 0.0003,
        "contractSize field identity"
    );
    assert_eq!(
        instrument.price_increment, 0.0004,
        "tickSize field identity"
    );
    assert_eq!(
        instrument.created_at_ms, 1234.0,
        "creationTimestamp field identity"
    );
    assert_eq!(
        instrument.expires_at_ms, 5678.0,
        "expirationTimestamp field identity"
    );
    comparison::compare(
        &capture(Ok(instrument)),
        &supplement["result"],
        "field identity supplement result",
    );
    let eth_supplement = &oracle()["ethInstrumentIdentitySupplement"];
    assert_eq!(eth_supplement["label"], "field-identity-supplement");
    let eth = normalize_coinbase_starbase_instrument(Some(&eth_supplement["input"]))
        .expect("ETH identity supplement instrument");
    assert_eq!(
        eth.minimum_base_amount, 0.0012,
        "ETH minTradeAmount identity"
    );
    assert_eq!(eth.contract_size, 0.0023, "ETH contractSize identity");
    assert_eq!(eth.price_increment, 0.0034, "ETH tickSize identity");
    assert_eq!(eth.created_at_ms, 2345.0, "ETH creationTimestamp identity");
    assert_eq!(
        eth.expires_at_ms, 6789.0,
        "ETH expirationTimestamp identity"
    );
    comparison::compare(
        &capture(Ok(eth)),
        &eth_supplement["result"],
        "ETH field identity supplement result",
    );
    let evidence_supplement = &oracle()["evidenceInstrumentIdentitySupplement"];
    assert_eq!(
        evidence_supplement["label"],
        "field-identity-supplement: live REST instruments.json"
    );
    let evidence = normalize_coinbase_starbase_instrument(Some(&evidence_supplement["input"]))
        .expect("live REST instrument identity supplement");
    assert_eq!(
        evidence.minimum_base_amount, 0.0005,
        "live minTradeAmount identity"
    );
    assert_eq!(evidence.contract_size, 0.0006, "live contractSize identity");
    assert_eq!(evidence.price_increment, 0.0007, "live tickSize identity");
    assert_eq!(
        evidence.created_at_ms, 3456.0,
        "live creationTimestamp identity"
    );
    assert_eq!(
        evidence.expires_at_ms, 7890.0,
        "live expirationTimestamp identity"
    );
    comparison::compare(
        &capture(Ok(evidence)),
        &evidence_supplement["result"],
        "live REST instrument identity result",
    );
    for case in oracle()["instrumentCases"].as_array().unwrap() {
        comparison::compare(
            &capture(normalize_coinbase_starbase_instrument(case.get("input"))),
            &case["result"],
            &format!("instrument {case}"),
        );
    }
    for case in oracle()["stepCases"].as_array().unwrap() {
        comparison::compare(
            &capture(parse_tick_size_steps(case.get("input"))),
            &case["result"],
            &format!("steps {case}"),
        );
    }
}
#[test]
fn public_instrument_discovery_validation_order() {
    for case in oracle()["instrumentListCases"].as_array().unwrap() {
        comparison::compare(
            &capture(normalize_coinbase_starbase_instruments(case.get("input"))),
            &case["result"],
            &format!("discovery {case}"),
        );
    }
}

#[test]
fn public_rest_book_levels_sort_then_accumulate() {
    for case in oracle()["restLevelCases"].as_array().unwrap() {
        comparison::compare(
            &capture(parse_rest_book_levels(
                case.get("input"),
                case["context"].as_str().unwrap(),
            )),
            &case["result"],
            &format!("levels {case}"),
        );
    }
    for case in oracle()["bookCases"].as_array().unwrap() {
        let parsed: Option<Value> = case
            .get("inputJson")
            .map(|value| serde_json::from_str(value.as_str().unwrap()).unwrap());
        let result = normalize_coinbase_starbase_order_book_snapshot(
            parsed.as_ref().or_else(|| case.get("input")),
        );
        let mut actual = capture(result.clone());
        if let Ok(book) = result {
            for (side, levels) in [("bids", book.bids), ("asks", book.asks)] {
                for (index, level) in levels.iter().enumerate() {
                    for (field, value) in [
                        ("price", level.price),
                        ("amount", level.amount),
                        ("total", level.total),
                    ] {
                        special_numbers::project_number(
                            &mut actual["ok"][side][index],
                            field,
                            value,
                        );
                    }
                }
            }
        }
        comparison::compare(&actual, &case["result"], &format!("book {case}"));
    }
}
#[test]
fn public_chart_mapping_errors_units_and_parallel_arrays() {
    let oracle = oracle();
    let candle_cases = oracle["candleCases"].as_array().unwrap();
    assert_eq!(candle_cases.len(), 1_098, "source candle case count");
    for case in candle_cases {
        comparison::compare(
            &capture(parse_candle(
                case["input"].as_object().unwrap(),
                case["context"].as_str().unwrap(),
            )),
            &case["result"],
            &format!("candle {case}"),
        );
    }
    let chart_payload_cases = oracle["chartPayloadCases"].as_array().unwrap();
    assert_eq!(
        chart_payload_cases.len(),
        231,
        "source chart payload case count"
    );
    for case in chart_payload_cases {
        comparison::compare(
            &capture(normalize_coinbase_starbase_chart_payload(case.get("input"))),
            &case["result"],
            &format!("chart payload {case}"),
        );
    }
    let chart_cases = oracle["chartCases"].as_array().unwrap();
    assert_eq!(chart_cases.len(), 417, "source chart data case count");
    let mut source_rows = 0;
    let mut source_array_cases = 0;
    let mut source_nonempty_cases = 0;
    let mut native_rows = 0;
    let mut native_array_cases = 0;
    let mut native_nonempty_cases = 0;
    for (index, case) in chart_cases.iter().enumerate() {
        let expected = &case["result"];
        let actual = capture(normalize_coinbase_starbase_chart_data(case.get("input")));
        if let Some(rows) = expected["ok"].as_array() {
            let input_count = if case["input"]["status"] == "ok" {
                case["input"]["ticks"].as_array().unwrap().len()
            } else {
                0
            };
            assert_eq!(rows.len(), input_count, "chart source rows case {index}");
            source_rows += rows.len();
            source_array_cases += 1;
            source_nonempty_cases += usize::from(!rows.is_empty());
            let actual_rows = actual["ok"].as_array().unwrap_or_else(|| {
                panic!("chart native result missing rows case {index}: {actual}")
            });
            assert_eq!(
                actual_rows.len(),
                input_count,
                "chart native rows case {index}"
            );
            native_rows += actual_rows.len();
            native_array_cases += 1;
            native_nonempty_cases += usize::from(!actual_rows.is_empty());
        }
        comparison::compare(&actual, expected, &format!("chart data {case}"));
    }
    assert_eq!(source_array_cases, 40, "source chart-array cases");
    assert_eq!(source_nonempty_cases, 37, "source nonempty chart cases");
    assert_eq!(source_rows, 111, "source chart rows total");
    assert_eq!(native_array_cases, 40, "native chart-array cases");
    assert_eq!(native_nonempty_cases, 37, "native nonempty chart cases");
    assert_eq!(native_rows, 111, "native chart rows total");
}

#[test]
fn public_request_generic_constraints_and_fields() {
    assert_eq!(
        schema::wire_schema(
            include_str!("../src/starbase_public.rs"),
            "CoinbaseStarbasePublicRequest"
        ),
        oracle()["recordSchemas"]["CoinbaseStarbasePublicRequest"]
    );
}

#[test]
fn public_request_complete_serialization() {
    let params = serde_json::Map::from_iter([
        ("currency".into(), json!("USDC")),
        ("expired".into(), json!(false)),
        ("kind".into(), json!("future")),
    ]);
    let instruments = CoinbaseStarbasePublicRequest {
        method: "public/get_instruments",
        params,
    };
    let ticker = CoinbaseStarbasePublicRequest {
        method: String::from("public/ticker"),
        params: serde_json::Map::from_iter([(
            "instrument_name".into(),
            json!("ETH_USDC-PERPETUAL"),
        )]),
    };
    comparison::compare(
        &json!([instruments, ticker]),
        &oracle()["requestSamples"],
        "generic public requests",
    );
}

#[path = "support/special_numbers.rs"]
mod special_numbers;

#[test]
fn public_delta_level_actions_and_error_precedence() {
    let cases = oracle()["deltaCases"].as_array().unwrap();
    assert_eq!(cases.len(), 4_170, "delta native independent source floor");
    let expected_number = |value: &Value| match value.get("$number").and_then(Value::as_str) {
        Some("-0") => -0.0_f64,
        None => value.as_f64().expect("finite source delta number"),
        Some(tag) => panic!("unexpected successful delta number tag {tag}"),
    };
    let mut results = Vec::with_capacity(cases.len());
    for case in cases {
        let frame_type = match case["frameType"].as_str().unwrap() {
            "snapshot" => CoinbaseStarbaseBookFrameType::Snapshot,
            "change" => CoinbaseStarbaseBookFrameType::Change,
            frame => panic!("unhandled source frame type {frame}"),
        };
        let parsed: Option<Value> = case
            .get("inputJson")
            .map(|value| serde_json::from_str(value.as_str().unwrap()).unwrap());
        let result = parse_delta_levels(
            parsed.as_ref().or_else(|| case.get("input")),
            frame_type,
            case["context"].as_str().unwrap(),
        );
        match (
            &result,
            case["result"].get("ok"),
            case["result"].get("error"),
        ) {
            (Ok(levels), Some(expected), None) => {
                let expected = expected.as_array().expect("source delta levels");
                assert_eq!(
                    levels.len(),
                    expected.len(),
                    "delta native level count: {case}"
                );
                for (level, expected) in levels.iter().zip(expected) {
                    let action = match expected["action"].as_str().unwrap() {
                        "new" => StarbaseBookAction::New,
                        "change" => StarbaseBookAction::Change,
                        "delete" => StarbaseBookAction::Delete,
                        action => panic!("unexpected source delta action {action}"),
                    };
                    assert_eq!(level.action, action, "delta native action: {case}");
                    assert_eq!(
                        level.price.to_bits(),
                        expected_number(&expected["price"]).to_bits(),
                        "delta native price bits: {case}"
                    );
                    assert_eq!(
                        level.amount.to_bits(),
                        expected_number(&expected["amount"]).to_bits(),
                        "delta native amount bits: {case}"
                    );
                }
            }
            (Err(error), None, Some(expected)) => {
                assert_eq!(
                    error.name,
                    expected["name"].as_str().unwrap(),
                    "delta native error name: {case}"
                );
                assert_eq!(
                    error.message,
                    expected["message"].as_str().unwrap(),
                    "delta native error message: {case}"
                );
            }
            _ => panic!("delta native result kind: {case}"),
        }
        results.push(result);
    }
    assert_eq!(
        schema::wire_schema(
            include_str!("../src/starbase_public.rs"),
            "ParsedStarbaseBookLevel"
        ),
        oracle()["recordSchemas"]["ParsedStarbaseBookLevel"]
    );
    for (case, result) in cases.iter().zip(results) {
        let mut actual = capture(result.clone());
        if let Ok(levels) = result {
            for (index, level) in levels.iter().enumerate() {
                special_numbers::project_number(&mut actual["ok"][index], "amount", level.amount);
                special_numbers::project_number(&mut actual["ok"][index], "price", level.price);
            }
        }
        comparison::compare(&actual, &case["result"], &format!("delta level {case}"));
    }
}

#[test]
fn public_rest_request_builders_and_depth_precedence() {
    for case in oracle()["requestCases"].as_array().unwrap() {
        let result = match case["name"].as_str().unwrap() {
            "instruments" => capture(Ok(build_coinbase_starbase_get_instruments())),
            "ticker" => capture(build_coinbase_starbase_ticker(
                case["symbol"].as_str().unwrap(),
            )),
            "book" => capture(build_coinbase_starbase_order_book(
                case["symbol"].as_str().unwrap(),
                case.get("depth"),
            )),
            name => panic!("unexpected request {name}"),
        };
        assert_eq!(result, case["result"], "{case}");
    }
}

#[test]
fn public_market_parity_retains_gateway_and_ordered_blockers() {
    assert_eq!(
        schema::wire_schema(
            include_str!("../src/starbase_public.rs"),
            "CoinbaseStarbaseMarketParityResult"
        ),
        oracle()["recordSchemas"]["CoinbaseStarbaseMarketParityResult"]
    );
    for case in oracle()["marketParityCases"].as_array().unwrap() {
        let gateway = normalize_coinbase_starbase_instrument(case.get("input")).unwrap();
        let result = assess_coinbase_starbase_market_parity(&gateway);
        assert!(std::ptr::eq(result.gateway, &gateway));
        comparison::compare(
            &serde_json::to_value(result).unwrap(),
            &case["result"],
            "market parity",
        );
    }
}

#[test]
fn public_chart_request_order_resolution_and_prototype_correction() {
    for case in oracle()["resolutionCases"].as_array().unwrap() {
        let timeframe = case["timeframe"].as_str().unwrap();
        let result = capture(resolve_chart_resolution(timeframe));
        if case["prototype"] == true {
            assert!(matches!(
                case["actualType"].as_str(),
                Some("function" | "object")
            ));
            assert_eq!(
                result,
                json!({"error":{"name":"CoinbaseStarbaseProtocolError","message":format!("Unsupported Starbase chart timeframe: {timeframe}")}})
            );
        } else {
            assert_eq!(result, case["result"], "{case}");
        }
    }
    for case in oracle()["chartRequestCases"].as_array().unwrap() {
        let start = case["startJson"]
            .as_str()
            .map(|s| serde_json::from_str::<Value>(s).unwrap());
        let end = case["endJson"]
            .as_str()
            .map(|s| serde_json::from_str::<Value>(s).unwrap());
        let timeframe = case["timeframe"].as_str().unwrap();
        let result = build_coinbase_starbase_chart_data(
            case["symbol"].as_str().unwrap(),
            timeframe,
            start.as_ref().or_else(|| case.get("start")),
            end.as_ref().or_else(|| case.get("end")),
        );
        let mut actual = capture(result.clone());
        if case["prototype"] == true && case["result"].get("ok").is_some() {
            assert!(matches!(
                case["resolutionType"].as_str(),
                Some("function" | "object")
            ));
            let mut params = json!({
                "end_timestamp": case["end"],
                "instrument_name": to_coinbase_starbase_instrument_name(case["symbol"].as_str().unwrap()).unwrap(),
            });
            if case["resolutionType"] == "object" {
                params["resolution"] = json!({});
            }
            params["start_timestamp"] = case["start"].clone();
            comparison::compare(
                &case["result"],
                &json!({"ok":{"method":"public/get_tradingview_chart_data","params":params}}),
                "prototype wire view",
            );
            assert_eq!(
                actual,
                json!({"error":{"name":"CoinbaseStarbaseProtocolError","message":format!("Unsupported Starbase chart timeframe: {timeframe}")}})
            );
            continue;
        }
        if let Ok(request) = result {
            for field in ["start_timestamp", "end_timestamp"] {
                let value = request.params[field].as_f64().unwrap();
                special_numbers::project_number(&mut actual["ok"]["params"], field, value);
            }
        }
        comparison::compare(&actual, &case["result"], &case.to_string());
    }
}

#[test]
fn public_sequenced_book_state_recovery_and_persistent_identity() {
    let scenarios = oracle()["sequencedCases"].as_array().unwrap();
    assert_eq!(scenarios.len(), 2_405, "sequenced source scenario count");
    let mut native_comparisons = 0;
    let mut source_step_lengths = std::collections::BTreeMap::new();
    for (scenario, steps) in scenarios.iter().enumerate() {
        let steps = steps.as_array().unwrap();
        *source_step_lengths.entry(steps.len()).or_insert(0usize) += 1;
        let mut instance = CoinbaseStarbaseSequencedOrderBook::new();
        let identity = instance.order_book();
        for (step, event) in steps.iter().enumerate() {
            let result = if event["reconnect"] == true {
                instance.on_reconnect();
                json!({"ok":null})
            } else if let Some(book) = event.get("externalBook") {
                *identity.lock().unwrap() = serde_json::from_value(book.clone()).unwrap();
                json!({"ok":null})
            } else {
                capture(instance.apply_payload(event.get("input")))
            };
            let context = format!("sequenced scenario {scenario} step {step} {event}");
            comparison::compare(&result, &event["result"], &context);
            native_comparisons += 1;
            assert!(std::sync::Arc::ptr_eq(&identity, &instance.order_book()));
            let book = identity.lock().unwrap();
            let mut actual = serde_json::to_value(&*book).unwrap();
            for (side, levels) in [("bids", &book.bids), ("asks", &book.asks)] {
                for (index, level) in levels.iter().enumerate() {
                    for (field, value) in [
                        ("price", level.price),
                        ("amount", level.amount),
                        ("total", level.total),
                    ] {
                        special_numbers::project_number(&mut actual[side][index], field, value);
                    }
                }
            }
            comparison::compare(&actual, &event["book"], &context);
        }
    }
    assert_eq!(
        source_step_lengths,
        [(3, 1), (4, 1), (5, 403), (6, 800), (7, 1_200)]
            .into_iter()
            .collect(),
        "sequenced per-scenario source step counts"
    );
    assert_eq!(
        native_comparisons, 15_222,
        "sequenced native comparison count"
    );
}

#[test]
fn public_channel_template_domain_and_serialization() {
    let source = include_str!("../src/starbase_public.rs");
    let parsed = syn::parse_file(source).unwrap();
    let channel = parsed
        .items
        .iter()
        .find_map(|item| match item {
            syn::Item::Enum(e) if e.ident == "CoinbaseStarbasePublicChannel" => Some(e),
            _ => None,
        })
        .unwrap();
    let templates: Vec<Value> = channel
        .variants
        .iter()
        .map(|variant| {
            let head = match variant.ident.to_string().as_str() {
                "Ticker" => "ticker.",
                "Book" => "book.",
                "Chart" => "chart.trades.",
                "InstrumentState" => "instrument.state.future.",
                other => panic!("unclassified channel {other}"),
            };
            let fields: Vec<_> = variant.fields.iter().collect();
            let spans: Vec<Value> = fields
                .iter()
                .enumerate()
                .map(|(i, field)| {
                    let syn::Type::Path(ty) = &field.ty else {
                        panic!("channel field must be a named type")
                    };
                    let name = ty.path.get_ident().unwrap().to_string();
                    let values = if name == "String" {
                        Value::Null
                    } else {
                        schema::wire_schema(source, &name)["values"].clone()
                    };
                    json!({"values":values,"tail":if i+1<fields.len(){"."}else{""}})
                })
                .collect();
            json!({"head":head,"spans":spans})
        })
        .collect();
    assert_eq!(json!(templates), oracle()["channelTemplates"]);
    for case in oracle()["channelCases"].as_array().unwrap() {
        let channel = channel_from_case(case);
        assert_eq!(
            serde_json::to_value(channel).unwrap(),
            case["text"],
            "{case}"
        );
    }
}

#[test]
fn public_channel_builders_validation_precedence_and_prototype_correction() {
    for case in oracle()["channelBuilderCases"].as_array().unwrap() {
        let symbol = case["symbol"].as_str().unwrap_or("");
        let actual = match case["kind"].as_str().unwrap() {
            "Ticker" => capture(build_coinbase_starbase_ticker_channel(
                symbol,
                case.get("interval"),
            )),
            "Book" => capture(build_coinbase_starbase_book_channel(
                symbol,
                case.get("interval"),
            )),
            "Chart" => capture(build_coinbase_starbase_chart_channel(
                symbol,
                case["timeframe"].as_str().unwrap(),
            )),
            "InstrumentState" => capture(build_coinbase_starbase_instrument_state_channel(
                case.get("currency"),
            )),
            other => panic!("unknown builder {other}"),
        };
        if case["prototype"] == true && case["result"].get("ok").is_some() {
            assert!(
                case["result"]["ok"]
                    .as_str()
                    .unwrap()
                    .starts_with("chart.trades.")
            );
            assert_eq!(
                actual,
                capture::<()>(protocol_error(format!(
                    "Unsupported Starbase chart timeframe: {}",
                    case["timeframe"].as_str().unwrap()
                ))),
                "{case}"
            );
        } else {
            assert_eq!(actual, case["result"], "{case}");
        }
    }
}

fn channel_from_case(case: &Value) -> CoinbaseStarbasePublicChannel {
    let parts = case["parts"].as_array().unwrap();
    let part = |i: usize| parts[i].as_str().unwrap();
    let interval = || match part(1) {
        "100ms" => CoinbaseStarbaseBookInterval::V100Ms,
        "agg2" => CoinbaseStarbaseBookInterval::Agg2,
        other => panic!("unknown interval {other}"),
    };
    match case["head"].as_str().unwrap() {
        "ticker." => CoinbaseStarbasePublicChannel::Ticker {
            instrument_name: part(0).into(),
            interval: interval(),
        },
        "book." => CoinbaseStarbasePublicChannel::Book {
            instrument_name: part(0).into(),
            interval: interval(),
        },
        "chart.trades." => CoinbaseStarbasePublicChannel::Chart {
            instrument_name: part(0).into(),
            resolution: match part(1) {
                "1" => CoinbaseStarbaseChartResolution::V1,
                "3" => CoinbaseStarbaseChartResolution::V3,
                "5" => CoinbaseStarbaseChartResolution::V5,
                "15" => CoinbaseStarbaseChartResolution::V15,
                "30" => CoinbaseStarbaseChartResolution::V30,
                "60" => CoinbaseStarbaseChartResolution::V60,
                "120" => CoinbaseStarbaseChartResolution::V120,
                "360" => CoinbaseStarbaseChartResolution::V360,
                "720" => CoinbaseStarbaseChartResolution::V720,
                "1D" => CoinbaseStarbaseChartResolution::V1D,
                other => panic!("unknown resolution {other}"),
            },
        },
        "instrument.state.future." => CoinbaseStarbasePublicChannel::InstrumentState {
            currency: match part(0) {
                "BTC" => CoinbaseStarbaseInstrumentStateCurrency::Btc,
                "ETH" => CoinbaseStarbaseInstrumentStateCurrency::Eth,
                "USDC" => CoinbaseStarbaseInstrumentStateCurrency::Usdc,
                "any" => CoinbaseStarbaseInstrumentStateCurrency::Any,
                other => panic!("unknown currency {other}"),
            },
        },
        other => panic!("unknown channel {other}"),
    }
}

fn reconnect_result(instance: &CoinbaseStarbasePublicSubscriptions) -> Value {
    match instance.build_reconnect_request() {
        Ok(Some(value)) => json!({"ok":value}),
        Ok(None) => json!({"undefined":true}),
        Err(error) => json!({"error":error}),
    }
}
fn indexed_channels(value: &Value) -> Vec<CoinbaseStarbasePublicChannel> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|index| channel_from_case(&oracle()["channelCases"][index.as_u64().unwrap() as usize]))
        .collect()
}
#[test]
fn public_subscriptions_transactional_state_order_and_detached_requests() {
    let scenarios = oracle()["subscriptionCases"].as_array().unwrap();
    assert_eq!(scenarios.len(), 632, "subscription source scenario count");
    let mut native_comparisons = 0;
    let mut source_step_lengths = std::collections::BTreeMap::new();
    for (scenario, steps) in scenarios.iter().enumerate() {
        let steps = steps.as_array().unwrap();
        *source_step_lengths.entry(steps.len()).or_insert(0usize) += 1;
        let mut instance = CoinbaseStarbasePublicSubscriptions::new();
        for (step, event) in steps.iter().enumerate() {
            let mut channels = event
                .get("channels")
                .map(indexed_channels)
                .unwrap_or_default();
            let mut result = match event["op"].as_str().unwrap() {
                "subscribe" => capture(instance.subscribe(&channels)),
                "unsubscribe" => capture(instance.unsubscribe(&channels)),
                "reconnect" => reconnect_result(&instance),
                other => panic!("unknown subscription operation {other}"),
            };
            let context = format!("scenario {scenario} step {step} {event}");
            comparison::compare(&result, &event["result"], &context);
            native_comparisons += 1;
            channels.clear();
            comparison::compare(&result, &event["result"], &context);
            if let Some(ok) = result.get_mut("ok") {
                ok["params"]["channels"] = json!(["mutated-output"]);
            }
            comparison::compare(&reconnect_result(&instance), &event["reconnect"], &context);
        }
    }
    assert_eq!(
        source_step_lengths,
        [(11, 158), (12, 474)].into_iter().collect(),
        "subscription per-scenario source step counts"
    );
    assert_eq!(
        native_comparisons, 7_426,
        "subscription native comparison count"
    );
}
#[test]
fn public_subscription_acknowledgements_shape_and_membership() {
    let instance = CoinbaseStarbasePublicSubscriptions::new();
    for case in oracle()["acknowledgementCases"].as_array().unwrap() {
        let requested = indexed_channels(&case["requested"]);
        let actual = match instance.validate_acknowledgement(case.get("value"), &requested) {
            Ok(()) => json!({"undefined":true}),
            Err(error) => json!({"error":error}),
        };
        comparison::compare(&actual, &case["result"], &case.to_string());
    }
}
