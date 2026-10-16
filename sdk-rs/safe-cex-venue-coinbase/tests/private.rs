use safe_cex_venue_coinbase::starbase_private::*;
use serde_json::{Value, json};
use std::{process::Command, sync::OnceLock};
#[path = "support/capture_metadata.rs"]
mod capture_metadata;
#[path = "support/compare.rs"]
mod comparison;
#[path = "support/private_projection.rs"]
mod private_projection;
#[path = "support/schema.rs"]
mod schema;
#[path = "support/special_numbers.rs"]
mod special_numbers;
use private_projection::{project_order, project_trade};
fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/private.cjs"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        capture_metadata::assert_capture_metadata(&value, 1);
        value
    })
}
#[test]
fn private_protocol_union_domains_and_inventory() {
    let source = include_str!("../src/starbase_private.rs");
    for (name, expected) in oracle()["schemas"].as_object().unwrap() {
        assert_eq!(schema::wire_schema(source, name), *expected, "{name}");
    }
    let parsed = syn::parse_file(source).unwrap();
    let never = parsed
        .items
        .iter()
        .find_map(|item| match item {
            syn::Item::Enum(e) if e.ident == "CoinbaseStarbaseNever" => Some(e),
            _ => None,
        })
        .unwrap();
    assert!(never.variants.is_empty(), "source never admits no values");
    let names: std::collections::BTreeSet<_> = parsed
        .items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Enum(e)
                if !e.variants.is_empty()
                    && e.variants
                        .iter()
                        .all(|variant| matches!(variant.fields, syn::Fields::Unit)) =>
            {
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
#[test]
fn private_protocol_union_serialization() {
    let actual = json!({
        "CoinbaseStarbaseMarketPrice": [CoinbaseStarbaseMarketPrice::MarketPrice],
        "CoinbaseStarbaseTradeLiquidity": [CoinbaseStarbaseTradeLiquidity::Maker,CoinbaseStarbaseTradeLiquidity::Taker],
        "CoinbaseStarbaseAccountCurrency": [CoinbaseStarbaseAccountCurrency::Usdc],
        "CoinbaseStarbasePositionDirection": [CoinbaseStarbasePositionDirection::Buy,CoinbaseStarbasePositionDirection::Sell,CoinbaseStarbasePositionDirection::Zero],
        "CoinbaseStarbaseMarginModel": [CoinbaseStarbaseMarginModel::CrossPm,CoinbaseStarbaseMarginModel::CrossSm,CoinbaseStarbaseMarginModel::SegregatedPm,CoinbaseStarbaseMarginModel::SegregatedSm],
        "CoinbaseStarbaseOrderDirection": [CoinbaseStarbaseOrderDirection::Buy,CoinbaseStarbaseOrderDirection::Sell],
        "CoinbaseStarbaseOrderState": [CoinbaseStarbaseOrderState::Open,CoinbaseStarbaseOrderState::Filled,CoinbaseStarbaseOrderState::Rejected,CoinbaseStarbaseOrderState::Cancelled,CoinbaseStarbaseOrderState::Untriggered,CoinbaseStarbaseOrderState::Triggered],
        "CoinbaseStarbaseOrderType": [CoinbaseStarbaseOrderType::Market,CoinbaseStarbaseOrderType::Limit,CoinbaseStarbaseOrderType::StopMarket,CoinbaseStarbaseOrderType::StopLimit,CoinbaseStarbaseOrderType::TakeMarket,CoinbaseStarbaseOrderType::TakeLimit,CoinbaseStarbaseOrderType::TrailingStop],
        "CoinbaseStarbaseTimeInForce": [CoinbaseStarbaseTimeInForce::GoodTilCancelled,CoinbaseStarbaseTimeInForce::GoodTilDay,CoinbaseStarbaseTimeInForce::FillOrKill,CoinbaseStarbaseTimeInForce::ImmediateOrCancel],
        "CoinbaseStarbaseTradeState": [CoinbaseStarbaseTradeState::Open,CoinbaseStarbaseTradeState::Filled,CoinbaseStarbaseTradeState::Rejected,CoinbaseStarbaseTradeState::Cancelled,CoinbaseStarbaseTradeState::Untriggered,CoinbaseStarbaseTradeState::Archive],
        "CoinbaseStarbaseTradeType": [CoinbaseStarbaseTradeType::Limit,CoinbaseStarbaseTradeType::Market,CoinbaseStarbaseTradeType::Liquidation],
        "CoinbaseStarbaseTriggerType": [CoinbaseStarbaseTriggerType::IndexPrice,CoinbaseStarbaseTriggerType::MarkPrice,CoinbaseStarbaseTriggerType::LastPrice],
        "CoinbaseStarbaseFeeCurrency": [CoinbaseStarbaseFeeCurrency::Btc,CoinbaseStarbaseFeeCurrency::Eth,CoinbaseStarbaseFeeCurrency::Usdc,CoinbaseStarbaseFeeCurrency::Usdt,CoinbaseStarbaseFeeCurrency::Eurr],
        "CoinbaseStarbaseParityBlocker": [CoinbaseStarbaseParityBlocker::AccountPnlUnitUnpublished,CoinbaseStarbaseParityBlocker::FilledAmountUnitConflict,CoinbaseStarbaseParityBlocker::GatewayAmountMissing,CoinbaseStarbaseParityBlocker::GatewayFilledAmountMissing,CoinbaseStarbaseParityBlocker::GoodTilDayUnsupported,CoinbaseStarbaseParityBlocker::PositionMarginPnlUnitsUnpublished,CoinbaseStarbaseParityBlocker::ReduceOnlyMissing,CoinbaseStarbaseParityBlocker::StopLimitShapeUnsupported,CoinbaseStarbaseParityBlocker::TakeLimitShapeUnsupported,CoinbaseStarbaseParityBlocker::TradeAmountUnitConflict,CoinbaseStarbaseParityBlocker::TriggerMissing],
    });
    assert_eq!(
        actual.as_object().unwrap().len(),
        oracle()["schemas"].as_object().unwrap().len()
    );
    for (name, values) in actual.as_object().unwrap() {
        assert_eq!(*values, oracle()["schemas"][name]["values"], "{name}");
    }
}

#[test]
fn private_record_declared_shapes() {
    assert_eq!(
        schema::wire_schema(
            include_str!("../src/starbase_private.rs"),
            "CoinbaseStarbaseOrderPrice"
        ),
        oracle()["recordSchemas"]["CoinbaseStarbasePrivateOrder"]["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == "price")
            .unwrap()["schema"]
    );
    let parsed = syn::parse_file(include_str!("../src/starbase_private.rs")).unwrap();
    let payloads: Vec<_> = parsed
        .items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Enum(e)
                if e.variants
                    .iter()
                    .any(|v| !matches!(v.fields, syn::Fields::Unit)) =>
            {
                Some(e.ident.to_string())
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        payloads,
        ["CoinbaseStarbaseOrderPrice", "CoinbaseStarbaseParityResult"]
    );

    for (name, expected) in oracle()["recordSchemas"].as_object().unwrap() {
        assert_eq!(
            schema::wire_schema(include_str!("../src/starbase_private.rs"), name),
            *expected,
            "{name}"
        );
    }
}
#[test]
fn private_record_complete_serialization() {
    for case in oracle()["recordCases"].as_array().unwrap() {
        let mask = case["mask"].as_u64().unwrap();
        let actual = match case["name"].as_str().unwrap() {
            "CoinbaseStarbaseAccountSummary" => {
                serde_json::to_value(CoinbaseStarbaseAccountSummary {
                    currency: CoinbaseStarbaseAccountCurrency::Usdc,
                    balance_usdc: 12.25,
                    equity_usdc: 12.25,
                    available_funds_usdc: 12.25,
                    available_withdrawal_funds_usdc: 12.25,
                    initial_margin_usdc: 12.25,
                    maintenance_margin_usdc: 12.25,
                    margin_balance_usdc: (mask & (1 << 0) != 0).then_some(12.25),
                    total_equity_usd: (mask & (1 << 1) != 0).then_some(12.25),
                    total_initial_margin_usd: (mask & (1 << 2) != 0).then_some(12.25),
                    total_maintenance_margin_usd: (mask & (1 << 3) != 0).then_some(12.25),
                    total_margin_balance_usd: (mask & (1 << 4) != 0).then_some(12.25),
                    cross_collateral_enabled: (mask & (1 << 5) != 0).then_some(false),
                    portfolio_margining_enabled: (mask & (1 << 6) != 0).then_some(false),
                    margin_model: (mask & (1 << 7) != 0)
                        .then_some(CoinbaseStarbaseMarginModel::CrossPm),
                    gateway_futures_pnl: 12.25,
                    gateway_futures_session_realized_pnl: 12.25,
                    gateway_futures_session_unrealized_pnl: 12.25,
                    gateway_session_realized_pnl: 12.25,
                    gateway_session_unrealized_pnl: 12.25,
                    gateway_total_pnl: 12.25,
                })
                .unwrap()
            }
            "CoinbaseStarbasePrivatePosition" => {
                serde_json::to_value(CoinbaseStarbasePrivatePosition {
                    instrument_name: "value\u{feff}".into(),
                    legacy_saved_symbol: "value\u{feff}".into(),
                    direction: CoinbaseStarbasePositionDirection::Buy,
                    base_amount: 12.25,
                    gateway_size_usd: 12.25,
                    entry_price_usd: 12.25,
                    mark_price_usd: 12.25,
                    index_price_usd: 12.25,
                    current_notional_usd: 12.25,
                    leverage: (mask & (1 << 0) != 0).then_some(12.25),
                    liquidation_price_usd: 12.25,
                    gateway_initial_margin: 12.25,
                    gateway_maintenance_margin: 12.25,
                    gateway_open_orders_margin: 12.25,
                    gateway_floating_pnl: 12.25,
                    gateway_realized_pnl: 12.25,
                    gateway_total_pnl: 12.25,
                })
                .unwrap()
            }
            "CoinbaseStarbaseOrderHistoryOptions" => {
                serde_json::to_value(CoinbaseStarbaseOrderHistoryOptions {
                    count: (mask & (1 << 0) != 0).then_some(12.25),
                    offset: (mask & (1 << 1) != 0).then_some(12.25),
                    include_old: (mask & (1 << 2) != 0).then_some(false),
                    include_unfilled: (mask & (1 << 3) != 0).then_some(false),
                    historical: (mask & (1 << 4) != 0).then_some(false),
                })
                .unwrap()
            }
            "CoinbaseStarbaseOrderHistoryPageParams" => {
                serde_json::to_value(CoinbaseStarbaseOrderHistoryPageParams {
                    count: 12.25,
                    offset: (mask & (1 << 0) != 0).then_some(12.25),
                    include_old: (mask & (1 << 1) != 0).then_some(false),
                    include_unfilled: (mask & (1 << 2) != 0).then_some(false),
                    historical: (mask & (1 << 3) != 0).then_some(false),
                })
                .unwrap()
            }
            "CoinbaseStarbasePrivateOrder" => {
                serde_json::to_value(order_witness(mask, case["choice"].as_u64().unwrap())).unwrap()
            }
            "CoinbaseStarbasePrivateTrade" => serde_json::to_value(trade_witness(mask)).unwrap(),
            "CoinbaseStarbaseTradePage" => serde_json::to_value(trade_page_witness(mask)).unwrap(),
            name => panic!("unclassified record {name}"),
        };
        assert_eq!(actual, case["value"], "{case}");
    }
    let params = serde_json::Map::from_iter([("value".into(), json!(false))]);
    let request = CoinbaseStarbasePrivateRequest {
        method: "private/test",
        params,
    };
    assert_eq!(
        serde_json::to_value(request).unwrap(),
        oracle()["requestSample"]
    );
}

fn order_witness(mask: u64, choice: u64) -> CoinbaseStarbasePrivateOrder {
    CoinbaseStarbasePrivateOrder {
        id: "value\u{feff}".into(),
        instrument_name: "value\u{feff}".into(),
        legacy_saved_symbol: "value\u{feff}".into(),
        state: CoinbaseStarbaseOrderState::Open,
        r#type: CoinbaseStarbaseOrderType::Market,
        time_in_force: CoinbaseStarbaseTimeInForce::GoodTilCancelled,
        direction: CoinbaseStarbaseOrderDirection::Buy,
        price: if choice == 0 {
            CoinbaseStarbaseOrderPrice::Number(12.25)
        } else {
            CoinbaseStarbaseOrderPrice::Market(CoinbaseStarbaseMarketPrice::MarketPrice)
        },
        average_price: (mask & (1 << 0) != 0).then_some(12.25),
        label: "value\u{feff}".into(),
        post_only: false,
        reduce_only: (mask & (1 << 1) != 0).then_some(false),
        creation_timestamp_ms: 12.25,
        last_update_timestamp_ms: 12.25,
        gateway_amount: (mask & (1 << 2) != 0).then_some(12.25),
        gateway_filled_amount: (mask & (1 << 3) != 0).then_some(12.25),
        gateway_contracts: (mask & (1 << 4) != 0).then_some(12.25),
        trigger: (mask & (1 << 5) != 0).then_some(CoinbaseStarbaseTriggerType::IndexPrice),
        trigger_price: (mask & (1 << 6) != 0).then_some(12.25),
        trigger_order_id: (mask & (1 << 7) != 0).then_some("value\u{feff}".into()),
    }
}

fn trade_witness(mask: u64) -> CoinbaseStarbasePrivateTrade {
    CoinbaseStarbasePrivateTrade {
        id: "value\u{feff}".into(),
        sequence: 12.25,
        order_id: "value\u{feff}".into(),
        instrument_name: "value\u{feff}".into(),
        legacy_saved_symbol: "value\u{feff}".into(),
        timestamp_ms: 12.25,
        direction: CoinbaseStarbaseOrderDirection::Buy,
        r#type: CoinbaseStarbaseTradeType::Limit,
        state: CoinbaseStarbaseTradeState::Open,
        price_usd: 12.25,
        mark_price_usd: 12.25,
        index_price_usd: 12.25,
        gateway_amount: 12.25,
        gateway_contracts: (mask & (1 << 0) != 0).then_some(12.25),
        gateway_profit_loss: (mask & (1 << 1) != 0).then_some(12.25),
        fee: 12.25,
        fee_currency: CoinbaseStarbaseFeeCurrency::Btc,
        liquidity: (mask & (1 << 2) != 0).then_some(CoinbaseStarbaseTradeLiquidity::Maker),
        label: (mask & (1 << 3) != 0).then_some("value\u{feff}".into()),
    }
}

fn trade_page_witness(mask: u64) -> CoinbaseStarbaseTradePage {
    CoinbaseStarbaseTradePage {
        has_more: false,
        trades: vec![trade_witness(mask)],
    }
}

#[test]
fn private_parity_result_tag_and_branch_fields() {
    for case in oracle()["parityCases"].as_array().unwrap() {
        let result: CoinbaseStarbaseParityResult<Value, Value> =
            match case["index"].as_u64().unwrap() {
                0 => CoinbaseStarbaseParityResult::Ready {
                    value: json!({"value":false}),
                },
                1 => CoinbaseStarbaseParityResult::Blocked {
                    gateway: json!({"value":false}),
                    blockers: vec![CoinbaseStarbaseParityBlocker::AccountPnlUnitUnpublished],
                },
                other => panic!("unclassified parity branch {other}"),
            };
        assert_eq!(serde_json::to_value(result).unwrap(), case["value"]);
    }
}

fn capture<T: serde::Serialize>(
    result: Result<T, safe_cex_venue_coinbase::starbase::CoinbaseStarbaseProtocolError>,
) -> Value {
    match result {
        Ok(value) => json!({"ok":value}),
        Err(error) => json!({"error":error}),
    }
}
fn capture_optional<T: serde::Serialize>(
    result: Result<Option<T>, safe_cex_venue_coinbase::starbase::CoinbaseStarbaseProtocolError>,
) -> Value {
    match result {
        Ok(Some(value)) => json!({"ok":value}),
        Ok(None) => json!({"undefined":true}),
        Err(error) => json!({"error":error}),
    }
}
fn capture_number(
    result: Result<f64, safe_cex_venue_coinbase::starbase::CoinbaseStarbaseProtocolError>,
) -> Value {
    match result {
        Ok(value) => {
            let mut result = json!({"ok":value});
            special_numbers::project_number(&mut result, "ok", value);
            result
        }
        Err(error) => json!({"error":error}),
    }
}
#[test]
fn private_guard_results_identity_and_signed_zero() {
    for case in oracle()["guardCases"].as_array().unwrap() {
        let input: Option<Value> = case
            .get("inputJson")
            .map(|value| serde_json::from_str(value.as_str().unwrap()).unwrap());
        let value = input.as_ref();
        let context = case["context"].as_str().unwrap();
        let actual = match case["name"].as_str().unwrap() {
            "isRecord" => json!({"ok":is_record(value)}),
            "protocolError" => capture(protocol_error::<()>(context)),
            "requireRecord" => {
                let result = require_record(value, context);
                if let Ok(record) = result {
                    assert!(std::ptr::eq(record, value.unwrap().as_object().unwrap()));
                }
                capture(result)
            }
            "requireArray" => {
                let result = require_array(value, context);
                if let Ok(array) = result {
                    assert!(std::ptr::eq(array, value.unwrap().as_array().unwrap()));
                }
                capture(result)
            }
            "requireString" => capture(require_string(
                value,
                context,
                case["allowEmpty"].as_bool().unwrap_or(false),
            )),
            "optionalString" => capture_optional(optional_string(value, context)),
            "requireFinite" => capture_number(require_finite(value, context)),
            "requireNonNegative" => capture_number(require_non_negative(value, context)),
            "requirePositive" => capture_number(require_positive(value, context)),
            "requireTimestamp" => capture_number(require_timestamp(value, context)),
            "requirePositiveInteger" => capture_number(require_positive_integer(value, context)),
            "optionalBoolean" => capture_optional(optional_boolean(value, context)),
            "validateCount" => capture_number(validate_count(value, context)),
            "parseEnum" => {
                let allowed: Vec<_> = case["allowed"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap())
                    .collect();
                capture(parse_enum(value, &allowed, context))
            }
            name => panic!("unclassified guard {name}"),
        };
        comparison::compare(&actual, &case["result"], &case.to_string());
    }
}

#[test]
fn private_rest_builders_defaults_omission_and_validation_order() {
    for case in oracle()["builderCases"].as_array().unwrap() {
        let decode = |key: &str| {
            case.get(key)
                .map(|value| serde_json::from_str::<Value>(value.as_str().unwrap()).unwrap())
        };
        let options = decode("optionsJson");
        let options = options.as_ref().map(|value| value.as_object().unwrap());
        let count = decode("countJson");
        let start = decode("startJson");
        let end = decode("endJson");
        let order_id = decode("orderIdJson");
        let name = case["name"].as_str().unwrap();
        let mut actual = match name {
            "buildOrderHistoryPageParams" => {
                capture(build_order_history_page_params(options.unwrap()))
            }
            "buildCoinbaseStarbaseAccountSummary" => {
                json!({"ok":build_coinbase_starbase_account_summary()})
            }
            "buildCoinbaseStarbasePositions" => json!({"ok":build_coinbase_starbase_positions()}),
            "buildCoinbaseStarbaseOpenOrders" => {
                json!({"ok":build_coinbase_starbase_open_orders()})
            }
            "buildCoinbaseStarbaseOrderState" => {
                capture(build_coinbase_starbase_order_state(order_id.as_ref()))
            }
            "buildCoinbaseStarbaseOrderHistoryByCurrency" => {
                capture(build_coinbase_starbase_order_history_by_currency(options))
            }
            "buildCoinbaseStarbaseOrderHistoryByInstrument" => {
                capture(build_coinbase_starbase_order_history_by_instrument(
                    case["symbol"].as_str().unwrap(),
                    options,
                ))
            }
            "buildCoinbaseStarbaseUserTrades" => {
                capture(build_coinbase_starbase_user_trades(count.as_ref()))
            }
            "buildCoinbaseStarbaseUserTradesByTime" => {
                capture(build_coinbase_starbase_user_trades_by_time(
                    start.as_ref(),
                    end.as_ref(),
                    count.as_ref(),
                ))
            }
            other => panic!("unclassified builder {other}"),
        };
        if let Some(ok) = actual.get_mut("ok") {
            if name == "buildOrderHistoryPageParams" {
                let params = build_order_history_page_params(options.unwrap()).unwrap();
                special_numbers::project_number(ok, "count", params.count);
                if let Some(offset) = params.offset {
                    special_numbers::project_number(ok, "offset", offset);
                }
            } else if let Some(params) = ok.get_mut("params") {
                // Check actual returned numeric values, not the expected TS projection.
                for field in ["count", "offset", "start_timestamp", "end_timestamp"] {
                    if let Some(value) = params.get(field).and_then(Value::as_f64) {
                        special_numbers::project_number(params, field, value);
                    }
                }
            }
        }
        comparison::compare(&actual, &case["result"], &case.to_string());
    }
}

#[test]
fn private_read_only_state_replaces_only_after_complete_validation() {
    let mut state = CoinbaseStarbaseReadOnlyState::default();
    comparison::compare(
        &serde_json::to_value(&state).unwrap(),
        &oracle()["initialState"],
        "initial read-only state",
    );
    for case in oracle()["stateCases"].as_array().unwrap() {
        let input = case.get("input").filter(|value| !value.is_null());
        let actual = match case["op"].as_str().unwrap() {
            "account" => capture(state.apply_account_summary(input)),
            "positions" => capture(state.apply_positions(input)),
            "openOrders" => capture(state.apply_open_orders(input)),
            "orderState" => capture(state.apply_order_state("order-1", input)),
            "orderHistory" => capture(state.apply_order_history(input)),
            "orderHistoryByInstrument" => capture(
                state.apply_order_history_by_instrument(case["symbol"].as_str().unwrap(), input),
            ),
            "tradePage" => capture(state.apply_trade_page(input)),
            operation => panic!("unclassified read-only state operation {operation}"),
        };
        comparison::compare(&actual, &case["result"], &case.to_string());
        comparison::compare(
            &serde_json::to_value(&state).unwrap(),
            &case["state"],
            &format!("state after {}", case["op"]),
        );
    }
}

#[test]
fn private_account_decoder_and_balance_blocker() {
    for case in oracle()["accountCases"].as_array().unwrap() {
        let input = case
            .get("inputJson")
            .map(|raw| serde_json::from_str::<Value>(raw.as_str().unwrap()).unwrap());
        let result = decode_coinbase_starbase_account_summary(input.as_ref());
        let actual = match result {
            Err(error) => capture::<Value>(Err(error)),
            Ok(account) => {
                let mut projected = serde_json::to_value(&account).unwrap();
                for (field, number) in [
                    ("balanceUsdc", Some(account.balance_usdc)),
                    ("equityUsdc", Some(account.equity_usdc)),
                    ("availableFundsUsdc", Some(account.available_funds_usdc)),
                    (
                        "availableWithdrawalFundsUsdc",
                        Some(account.available_withdrawal_funds_usdc),
                    ),
                    ("initialMarginUsdc", Some(account.initial_margin_usdc)),
                    (
                        "maintenanceMarginUsdc",
                        Some(account.maintenance_margin_usdc),
                    ),
                    ("marginBalanceUsdc", account.margin_balance_usdc),
                    ("totalEquityUsd", account.total_equity_usd),
                    ("totalInitialMarginUsd", account.total_initial_margin_usd),
                    (
                        "totalMaintenanceMarginUsd",
                        account.total_maintenance_margin_usd,
                    ),
                    ("totalMarginBalanceUsd", account.total_margin_balance_usd),
                    ("gatewayFuturesPnl", Some(account.gateway_futures_pnl)),
                    (
                        "gatewayFuturesSessionRealizedPnl",
                        Some(account.gateway_futures_session_realized_pnl),
                    ),
                    (
                        "gatewayFuturesSessionUnrealizedPnl",
                        Some(account.gateway_futures_session_unrealized_pnl),
                    ),
                    (
                        "gatewaySessionRealizedPnl",
                        Some(account.gateway_session_realized_pnl),
                    ),
                    (
                        "gatewaySessionUnrealizedPnl",
                        Some(account.gateway_session_unrealized_pnl),
                    ),
                    ("gatewayTotalPnl", Some(account.gateway_total_pnl)),
                ] {
                    if let Some(number) = number {
                        special_numbers::project_number(&mut projected, field, number);
                    }
                }
                let assessment = assess_coinbase_starbase_balance_parity(&account);
                let CoinbaseStarbaseParityResult::Blocked { gateway, .. } = &assessment else {
                    panic!("balance unexpectedly ready");
                };
                assert!(std::ptr::eq(*gateway, &account));
                let mut assessed = serde_json::to_value(&assessment).unwrap();
                assert_eq!(assessed["gateway"], serde_json::to_value(&account).unwrap());
                assessed["gateway"] = projected.clone();
                comparison::compare(&assessed, &case["assessment"], &case.to_string());
                json!({"ok":projected})
            }
        };
        comparison::compare(&actual, &case["result"], &case.to_string());
    }
}

#[test]
fn private_balance_assessment_live_return_contract() {
    for (function_name, oracle_key) in [
        (
            "assess_coinbase_starbase_balance_parity",
            "accountAssessmentReturn",
        ),
        (
            "assess_coinbase_starbase_position_parity",
            "positionAssessmentReturn",
        ),
        ("normalize_coinbase_starbase_trade", "tradeNormalizerReturn"),
    ] {
        let parsed = syn::parse_file(include_str!("../src/starbase_private.rs")).unwrap();
        let function = parsed
            .items
            .iter()
            .find_map(|item| match item {
                syn::Item::Fn(f) if f.sig.ident == function_name => Some(f),
                _ => None,
            })
            .unwrap();
        let syn::ReturnType::Type(_, output) = &function.sig.output else {
            panic!("missing return type")
        };
        let syn::Type::Path(output) = output.as_ref() else {
            panic!("return must be a generic parity result")
        };
        let segment = output.path.segments.last().unwrap();
        let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
            panic!("missing result arguments")
        };
        let types: Vec<_> = arguments
            .args
            .iter()
            .enumerate()
            .map(|(index, argument)| {
                let syn::GenericArgument::Type(mut ty) = argument.clone() else {
                    panic!("not a type")
                };
                if index == 1 {
                    let syn::Type::Reference(reference) = ty else {
                        panic!("gateway must retain borrowed identity")
                    };
                    ty = *reference.elem;
                }
                let syn::Type::Path(path) = ty else {
                    panic!("unexpected argument type")
                };
                if path.path.segments.len() > 1 {
                    assert_eq!(path.path.segments.len(), 2);
                    assert_eq!(path.path.segments.first().unwrap().ident, "safe_cex_types");
                }
                let name = path.path.segments.last().unwrap().ident.to_string();
                if name == "CoinbaseStarbaseNever" {
                    "never".to_owned()
                } else {
                    name
                }
            })
            .collect();
        assert_eq!(
            json!({"name":segment.ident.to_string(),"arguments":types}),
            oracle()[oracle_key]
        );
    }
}

fn project_position(position: &CoinbaseStarbasePrivatePosition) -> Value {
    let mut value = serde_json::to_value(position).unwrap();
    for (field, number) in [
        ("baseAmount", Some(position.base_amount)),
        ("gatewaySizeUsd", Some(position.gateway_size_usd)),
        ("entryPriceUsd", Some(position.entry_price_usd)),
        ("markPriceUsd", Some(position.mark_price_usd)),
        ("indexPriceUsd", Some(position.index_price_usd)),
        ("currentNotionalUsd", Some(position.current_notional_usd)),
        ("leverage", position.leverage),
        ("liquidationPriceUsd", Some(position.liquidation_price_usd)),
        (
            "gatewayInitialMargin",
            Some(position.gateway_initial_margin),
        ),
        (
            "gatewayMaintenanceMargin",
            Some(position.gateway_maintenance_margin),
        ),
        (
            "gatewayOpenOrdersMargin",
            Some(position.gateway_open_orders_margin),
        ),
        ("gatewayFloatingPnl", Some(position.gateway_floating_pnl)),
        ("gatewayRealizedPnl", Some(position.gateway_realized_pnl)),
        ("gatewayTotalPnl", Some(position.gateway_total_pnl)),
    ] {
        if let Some(number) = number {
            special_numbers::project_number(&mut value, field, number);
        }
    }
    value
}

#[test]
fn private_instrument_and_unique_helpers() {
    for case in oracle()["instrumentCases"].as_array().unwrap() {
        let input = case
            .get("inputJson")
            .map(|raw| serde_json::from_str::<Value>(raw.as_str().unwrap()).unwrap());
        comparison::compare(
            &capture(parse_starbase_instrument(
                input.as_ref(),
                case["context"].as_str().unwrap(),
            )),
            &case["result"],
            &case.to_string(),
        );
    }
    for case in oracle()["uniqueCases"].as_array().unwrap() {
        let ids = case["ids"].as_array().unwrap();
        let mut calls = Vec::new();
        let result = assert_unique(
            ids,
            |value| {
                assert!(
                    std::ptr::eq(value, &ids[calls.len()]),
                    "callback retains original element"
                );
                let id = if value.is_object() {
                    &value["id"]
                } else {
                    value
                };
                calls.push(id.clone());
                if calls.len() as i64 - 1 == case["failAt"].as_i64().unwrap() {
                    return Err(
                        safe_cex_venue_coinbase::starbase::CoinbaseStarbaseProtocolError::new(
                            "callback failure",
                        ),
                    );
                }
                Ok(id.as_str().unwrap().to_owned())
            },
            "unique context",
        );
        let actual = match result {
            Ok(()) => json!({"undefined":true}),
            Err(error) => capture::<Value>(Err(error)),
        };
        assert_eq!(json!(calls), case["calls"]);
        comparison::compare(&actual, &case["result"], &case.to_string());
    }
}

#[test]
fn private_position_decoder_and_blocker() {
    for case in oracle()["positionCases"].as_array().unwrap() {
        let input = case
            .get("inputJson")
            .map(|raw| serde_json::from_str::<Value>(raw.as_str().unwrap()).unwrap());
        let actual = match decode_coinbase_starbase_position(input.as_ref()) {
            Err(error) => capture::<Value>(Err(error)),
            Ok(position) => {
                let projected = project_position(&position);
                let assessment = assess_coinbase_starbase_position_parity(&position);
                let CoinbaseStarbaseParityResult::Blocked { gateway, .. } = &assessment else {
                    panic!("unexpected ready position")
                };
                assert!(std::ptr::eq(*gateway, &position));
                let mut assessed = serde_json::to_value(&assessment).unwrap();
                assert_eq!(
                    assessed["gateway"],
                    serde_json::to_value(&position).unwrap()
                );
                assessed["gateway"] = projected.clone();
                comparison::compare(&assessed, &case["assessment"], &case.to_string());
                json!({"ok":projected})
            }
        };
        comparison::compare(&actual, &case["result"], &case.to_string());
    }
}

#[test]
fn private_position_list_decodes_before_duplicate_check() {
    for case in oracle()["positionsCases"].as_array().unwrap() {
        let input = case
            .get("inputJson")
            .map(|raw| serde_json::from_str::<Value>(raw.as_str().unwrap()).unwrap());
        let actual = match decode_coinbase_starbase_positions(input.as_ref()) {
            Err(error) => capture::<Value>(Err(error)),
            Ok(positions) => {
                json!({"ok":positions.iter().map(project_position).collect::<Vec<_>>()})
            }
        };
        comparison::compare(&actual, &case["result"], &case.to_string());
    }
}

#[test]
fn private_order_price_and_decoder() {
    let supplement = &oracle()["sharedOrderIdentitySupplement"];
    assert_eq!(
        supplement["label"],
        "shared-fixture-field-identity-supplement"
    );
    let order = decode_coinbase_starbase_order(Some(&supplement["input"]))
        .expect("shared fixture identity supplement order");
    assert!(order.post_only, "post_only field identity");
    assert_eq!(order.reduce_only, Some(false), "reduce_only field identity");
    assert_eq!(order.gateway_amount, Some(0.14), "amount field identity");
    assert_eq!(
        order.gateway_filled_amount,
        Some(0.03),
        "filled_amount field identity"
    );
    assert_eq!(
        order.creation_timestamp_ms, 1_800_000_000_123.0,
        "creation_timestamp field identity"
    );
    assert_eq!(
        order.last_update_timestamp_ms, 1_800_000_000_456.0,
        "last_update_timestamp field identity"
    );
    for case in oracle()["priceCases"].as_array().unwrap() {
        let input = case
            .get("inputJson")
            .map(|raw| serde_json::from_str::<Value>(raw.as_str().unwrap()).unwrap());
        let actual = match parse_order_price(input.as_ref()) {
            Err(error) => capture::<Value>(Err(error)),
            Ok(CoinbaseStarbaseOrderPrice::Number(number)) => capture_number(Ok(number)),
            Ok(market) => json!({"ok":market}),
        };
        comparison::compare(&actual, &case["result"], &case.to_string());
    }
    for case in oracle()["orderCases"].as_array().unwrap() {
        let input = case
            .get("inputJson")
            .map(|raw| serde_json::from_str::<Value>(raw.as_str().unwrap()).unwrap());
        let actual = match decode_coinbase_starbase_order(input.as_ref()) {
            Err(error) => capture::<Value>(Err(error)),
            Ok(order) => json!({"ok":project_order(&order)}),
        };
        comparison::compare(&actual, &case["result"], &case.to_string());
    }
}

#[test]
fn shared_private_fixture_field_identity_supplements() {
    let account = &oracle()["sharedAccountIdentitySupplement"];
    assert_eq!(account["label"], "shared-fixture-field-identity-supplement");
    let account = decode_coinbase_starbase_account_summary(Some(&account["input"]))
        .expect("account identity supplement");
    assert_eq!(account.equity_usdc, 52.5, "account equity field identity");
    assert_eq!(
        account.margin_balance_usdc,
        Some(53.5),
        "account margin field identity"
    );
    assert_eq!(
        account.gateway_futures_pnl, 1.25,
        "futures PnL field identity"
    );
    assert_eq!(account.gateway_total_pnl, 2.5, "total PnL field identity");
    assert_eq!(account.gateway_futures_session_realized_pnl, 0.3);
    assert_eq!(account.gateway_session_realized_pnl, 0.4);
    assert_eq!(account.gateway_futures_session_unrealized_pnl, 0.6);
    assert_eq!(account.gateway_session_unrealized_pnl, 0.7);
    assert_eq!(account.cross_collateral_enabled, Some(true));
    assert_eq!(account.portfolio_margining_enabled, Some(false));

    let position = &oracle()["sharedPositionIdentitySupplement"];
    assert_eq!(
        position["label"],
        "shared-fixture-field-identity-supplement"
    );
    let position = decode_coinbase_starbase_position(Some(&position["input"]))
        .expect("position identity supplement");
    assert_eq!(position.leverage, Some(11.0), "leverage field identity");
    assert_eq!(
        position.gateway_realized_pnl, 12.0,
        "realized PnL field identity"
    );

    let trade = &oracle()["sharedTradeIdentitySupplement"];
    assert_eq!(trade["label"], "shared-fixture-field-identity-supplement");
    let trade =
        decode_coinbase_starbase_trade(Some(&trade["input"])).expect("trade identity supplement");
    assert_eq!(trade.price_usd, 50_400.0, "trade price field identity");
    assert_eq!(
        trade.mark_price_usd, 50_500.0,
        "trade mark price field identity"
    );
}
#[test]
fn private_order_lists_identity_validation_order() {
    for case in oracle()["orderListCases"].as_array().unwrap() {
        let input = case
            .get("inputJson")
            .map(|raw| serde_json::from_str::<Value>(raw.as_str().unwrap()).unwrap());
        let result = match case["name"].as_str().unwrap() {
            "decodeCoinbaseStarbaseOpenOrders" => {
                decode_coinbase_starbase_open_orders(input.as_ref())
            }
            "decodeCoinbaseStarbaseOrderHistory" => {
                decode_coinbase_starbase_order_history(input.as_ref())
            }
            "decodeCoinbaseStarbaseOrderHistoryByInstrument" => {
                decode_coinbase_starbase_order_history_by_instrument(
                    input.as_ref(),
                    case["symbol"].as_str().unwrap(),
                )
            }
            other => panic!("unclassified list {other}"),
        };
        let actual = match result {
            Err(error) => capture::<Value>(Err(error)),
            Ok(orders) => json!({"ok":orders.iter().map(project_order).collect::<Vec<_>>()}),
        };
        comparison::compare(&actual, &case["result"], &case.to_string());
    }
    for case in oracle()["orderStateCases"].as_array().unwrap() {
        let decode = |key| {
            case.get(key)
                .map(|raw| serde_json::from_str::<Value>(raw.as_str().unwrap()).unwrap())
        };
        let input = decode("inputJson");
        let expected = decode("expectedJson");
        let actual = match decode_coinbase_starbase_order_state(input.as_ref(), expected.as_ref()) {
            Err(error) => capture::<Value>(Err(error)),
            Ok(order) => json!({"ok":project_order(&order)}),
        };
        comparison::compare(&actual, &case["result"], &case.to_string());
    }
}

#[test]
fn private_trade_decoder_and_blocked_normalization() {
    for case in oracle()["tradeCases"].as_array().unwrap() {
        let input = case
            .get("inputJson")
            .map(|raw| serde_json::from_str::<Value>(raw.as_str().unwrap()).unwrap());
        let actual = match decode_coinbase_starbase_trade(input.as_ref()) {
            Err(error) => capture::<Value>(Err(error)),
            Ok(trade) => {
                let projected = project_trade(&trade);
                let assessment = normalize_coinbase_starbase_trade(&trade);
                let CoinbaseStarbaseParityResult::Blocked { gateway, .. } = &assessment else {
                    panic!("unexpected ready trade")
                };
                assert!(std::ptr::eq(*gateway, &trade));
                let mut assessed = serde_json::to_value(&assessment).unwrap();
                assert_eq!(assessed["gateway"], serde_json::to_value(&trade).unwrap());
                assessed["gateway"] = projected.clone();
                comparison::compare(&assessed, &case["assessment"], &case.to_string());
                json!({"ok":projected})
            }
        };
        comparison::compare(&actual, &case["result"], &case.to_string());
    }
}
#[test]
fn private_trade_page_validation_and_duplicates() {
    for case in oracle()["tradePageCases"].as_array().unwrap() {
        let input = case
            .get("inputJson")
            .map(|raw| serde_json::from_str::<Value>(raw.as_str().unwrap()).unwrap());
        let actual = match decode_coinbase_starbase_trade_page(input.as_ref()) {
            Err(error) => capture::<Value>(Err(error)),
            Ok(page) => {
                let mut serialized = serde_json::to_value(&page).unwrap();
                assert_eq!(
                    serialized["trades"],
                    serde_json::to_value(&page.trades).unwrap()
                );
                serialized["trades"] =
                    json!(page.trades.iter().map(project_trade).collect::<Vec<_>>());
                json!({"ok":serialized})
            }
        };
        comparison::compare(&actual, &case["result"], &case.to_string());
    }
}

#[test]
fn private_order_mapping_domains_and_results() {
    for case in oracle()["mappingCases"].as_array().unwrap() {
        let value = case["value"].as_str().unwrap();
        // Select the concrete Rust domain by serialized spelling; this avoids
        // maintaining a second set of string literals beside the source domains.
        macro_rules! select {
            ($($variant:expr),+ $(,)?) => { [$($variant),+].into_iter().find(|variant| serde_json::to_value(variant).unwrap() == json!(value)).unwrap() };
        }
        let actual = match case["name"].as_str().unwrap() {
            "mapOrderStatus" => {
                json!({"ok":map_order_status(select!(CoinbaseStarbaseOrderState::Open,CoinbaseStarbaseOrderState::Filled,CoinbaseStarbaseOrderState::Rejected,CoinbaseStarbaseOrderState::Cancelled,CoinbaseStarbaseOrderState::Untriggered,CoinbaseStarbaseOrderState::Triggered))})
            }
            "mapOrderSide" => {
                json!({"ok":map_order_side(select!(CoinbaseStarbaseOrderDirection::Buy,CoinbaseStarbaseOrderDirection::Sell))})
            }
            "mapOrderType" => capture_optional(Ok(map_order_type(select!(
                CoinbaseStarbaseOrderType::Market,
                CoinbaseStarbaseOrderType::Limit,
                CoinbaseStarbaseOrderType::StopMarket,
                CoinbaseStarbaseOrderType::StopLimit,
                CoinbaseStarbaseOrderType::TakeMarket,
                CoinbaseStarbaseOrderType::TakeLimit,
                CoinbaseStarbaseOrderType::TrailingStop
            )))),
            "mapTimeInForce" => capture_optional(Ok(map_time_in_force(
                select!(
                    CoinbaseStarbaseTimeInForce::GoodTilCancelled,
                    CoinbaseStarbaseTimeInForce::GoodTilDay,
                    CoinbaseStarbaseTimeInForce::FillOrKill,
                    CoinbaseStarbaseTimeInForce::ImmediateOrCancel
                ),
                case["postOnly"].as_bool().unwrap(),
            ))),
            "mapTrigger" => {
                json!({"ok":map_trigger(select!(CoinbaseStarbaseTriggerType::IndexPrice,CoinbaseStarbaseTriggerType::MarkPrice,CoinbaseStarbaseTriggerType::LastPrice))})
            }
            other => panic!("unclassified mapper {other}"),
        };
        comparison::compare(&actual, &case["result"], &case.to_string());
    }
}

#[test]
fn private_numeric_raw_ingress_signed_zero_underflow_and_overflow() {
    for case in oracle()["rawNumericIngress"].as_array().unwrap() {
        let raw = case["raw"].as_str().unwrap();
        let bits = u64::from_str_radix(case["bits"].as_str().unwrap(), 16).unwrap();
        let actual = match case["kind"].as_str().unwrap() {
            "finite" => {
                match serde_json::from_str::<f64>(raw) {
                    Ok(parsed) => {
                        assert_eq!(parsed.to_bits(), bits, "raw f64 decoder bits for {raw}")
                    }
                    Err(_) => assert!(
                        !case["finite"].as_bool().unwrap(),
                        "only binary64 overflow may fail raw f64 decoding: {raw}"
                    ),
                }
                let input: Value = serde_json::from_str(raw).unwrap();
                assert!(input.is_number(), "raw JSON number type for {raw}");
                match input.as_f64() {
                    Some(parsed) => assert_eq!(parsed.to_bits(), bits, "raw Value bits for {raw}"),
                    None => assert!(
                        !case["finite"].as_bool().unwrap(),
                        "only binary64 overflow may lack a Value projection: {raw}"
                    ),
                }
                match serde_json::from_value::<f64>(input.clone()) {
                    Ok(parsed) => assert_eq!(
                        parsed.to_bits(),
                        bits,
                        "owned Value f64 decoder bits for {raw}"
                    ),
                    Err(_) => assert!(
                        !case["finite"].as_bool().unwrap(),
                        "only binary64 overflow may fail owned Value decoding: {raw}"
                    ),
                }
                capture_number(require_finite(Some(&input), "raw ingress"))
            }
            "order-price" => {
                let input: Value =
                    serde_json::from_str(case["inputJson"].as_str().unwrap()).unwrap();
                let price = &input["price"];
                match price.as_f64() {
                    Some(parsed) => {
                        assert_eq!(parsed.to_bits(), bits, "decoded Value input bits for {raw}")
                    }
                    None => assert!(
                        !case["finite"].as_bool().unwrap(),
                        "only overflow may lack f64 projection: {raw}"
                    ),
                }
                match decode_coinbase_starbase_order(Some(&input)) {
                    Ok(order) => json!({"ok":project_order(&order)}),
                    Err(error) => json!({"error":error}),
                }
            }
            "untagged-price" => {
                let input: Value = serde_json::from_str(raw).unwrap();
                match parse_order_price(Some(&input)) {
                    Ok(CoinbaseStarbaseOrderPrice::Number(value)) => capture_number(Ok(value)),
                    Ok(value) => json!({"ok":value}),
                    Err(error) => json!({"error":error}),
                }
            }
            other => panic!("unclassified numeric ingress {other}"),
        };
        comparison::compare(&actual, &case["result"], &format!("{} {raw}", case["kind"]));
    }
}
