use safe_cex_venue_asterdex::{tables::*, types::*};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{any::Any, sync::OnceLock};
fn oracle() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        let output = std::process::Command::new("node")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/oracle.cjs"))
            .output()
            .expect("Node oracle must execute");
        assert!(
            output.status.success(),
            "TypeScript oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let data: Value = serde_json::from_slice(&output.stdout).expect("oracle JSON");
        assert!(data["captureArchitecture"].is_string());
        data
    })
}
fn captured_bits(value: f64) -> u64 {
    if value.is_nan() {
        0x7ff8_0000_0000_0000
    } else {
        value.to_bits()
    }
}
fn assert_numeric_ingress_record_field<T>(name: &str, field: &str, get: impl Fn(&T) -> f64)
where
    T: DeserializeOwned + serde::Serialize + safe_cex_num::numeric_ingress::JsNumberOutput,
{
    use safe_cex_num::numeric_ingress::{js_from_str, js_from_value, js_from_value_ref};
    let baseline = oracle()["numericRecords"][name]["sample"].clone();
    for literal in ["-0", "-0.0", "1", "1.25", "1e400", "-1e400"] {
        let expected =
            u64::from_str_radix(oracle()["numericBits"][literal].as_str().unwrap(), 16).unwrap();
        let mut source = baseline.clone();
        source[field] = Value::String("__numeric_literal__".into());
        let text = serde_json::to_string(&source)
            .unwrap()
            .replace("\"__numeric_literal__\"", literal);
        let parsed: Value = serde_json::from_str(&text).unwrap();
        for (path, decoded) in [
            ("raw", js_from_str::<T>(&text)),
            ("owned-value", js_from_value::<T>(parsed.clone())),
            ("borrowed-value", js_from_value_ref::<T>(&parsed)),
        ] {
            let decoded =
                decoded.unwrap_or_else(|error| panic!("{name}.{field} {path} {literal}: {error}"));
            assert_eq!(
                captured_bits(get(&decoded)),
                expected,
                "{name}.{field} {path} {literal} bits"
            );
        }
    }
}
fn flatten(value: &Value, prefix: &str, target: &mut serde_json::Map<String, Value>) {
    match value {
        Value::String(_) => {
            target.insert(prefix.to_owned(), value.clone());
        }
        Value::Object(m) => {
            for (k, v) in m {
                flatten(
                    v,
                    &if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{prefix}.{k}")
                    },
                    target,
                )
            }
        }
        Value::Array(a) => {
            for (i, v) in a.iter().enumerate() {
                flatten(v, &format!("{prefix}.{i}"), target)
            }
        }
        _ => panic!("unexpected table value"),
    }
}
fn semantic_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| semantic_eq(x, y))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, v)| b.get(k).is_some_and(|x| semantic_eq(v, x)))
        }
        _ => a == b,
    }
}
fn nested_array_count(value: &Value, path: &str) -> usize {
    path.split('.')
        .try_fold(value, |current, key| current.get(key))
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
}
fn assert_array_cardinality(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        (Value::Array(actual), Value::Array(expected)) => {
            assert_eq!(actual.len(), expected.len(), "{path} native array length");
            for index in 0..expected.len() {
                assert_array_cardinality(
                    &actual[index],
                    &expected[index],
                    &format!("{path}[{index}]"),
                );
            }
        }
        (Value::Array(_), _) | (_, Value::Array(_)) => {
            panic!("{path} native/source array shape differs");
        }
        (Value::Object(actual), Value::Object(expected)) => {
            for (key, expected_value) in expected {
                if let Some(actual_value) = actual.get(key) {
                    assert_array_cardinality(
                        actual_value,
                        expected_value,
                        &format!("{path}.{key}"),
                    );
                }
            }
        }
        _ => {}
    }
}
macro_rules! identity_fields {
    ($value:expr; $($field:ident),* $(,)?) => {
        vec![$((stringify!($field), serde_json::to_value(&$value.$field).unwrap()),)*]
    };
}
fn native_identity_fields(name: &str, value: &dyn Any) -> Vec<(&'static str, Value)> {
    match name {
        "AsterDEXV3AuthContext" => {
            let v = value.downcast_ref::<AsterDEXV3AuthContext>().unwrap();
            identity_fields!(v; user_address, signer_address, signer_private_key)
        }
        "AsterDEXMarketResponseFiltersItem" => {
            let v = value
                .downcast_ref::<AsterDEXMarketResponseFiltersItem>()
                .unwrap();
            identity_fields!(v; extra, filter_type, max_price, min_price, tick_size, max_qty, min_qty, step_size, limit, notional, multiplier_up, multiplier_down, multiplier_decimal)
        }
        "AsterDEXMarketResponse" => {
            let v = value.downcast_ref::<AsterDEXMarketResponse>().unwrap();
            identity_fields!(v; extra, symbol, base_asset, quote_asset, contract_type, delivery_date, onboard_date, status, maint_margin_percent, required_margin_percent, price_precision, quantity_precision, base_asset_precision, quote_precision, underlying_type, underlying_sub_type, settle_plan, trigger_protect, filters, order_types, time_in_force, liquidation_fee, market_take_bound)
        }
        "AsterDEXAccountResponseAssetsItem" => {
            let v = value
                .downcast_ref::<AsterDEXAccountResponseAssetsItem>()
                .unwrap();
            identity_fields!(v; asset, wallet_balance, unrealized_profit, margin_balance, maint_margin, initial_margin, position_initial_margin, open_order_initial_margin, max_withdraw_amount, cross_wallet_balance, cross_un_pnl, available_balance, margin_available, update_time)
        }
        "AsterDEXAccountResponsePositionsItem" => {
            let v = value
                .downcast_ref::<AsterDEXAccountResponsePositionsItem>()
                .unwrap();
            identity_fields!(v; symbol, initial_margin, maint_margin, unrealized_profit, position_initial_margin, open_order_initial_margin, leverage, isolated, entry_price, max_notional, position_side, position_amt, notional, isolated_wallet, update_time, bid_notional, ask_notional)
        }
        "AsterDEXAccountResponse" => {
            let v = value.downcast_ref::<AsterDEXAccountResponse>().unwrap();
            identity_fields!(v; account_alias, fee_tier, can_trade, can_deposit, can_withdraw, update_time, total_initial_margin, total_maint_margin, total_wallet_balance, total_unrealized_profit, total_margin_balance, total_position_initial_margin, total_open_order_initial_margin, total_cross_wallet_balance, total_cross_un_pnl, available_balance, max_withdraw_amount, assets, positions)
        }
        "AsterDEXPositionModeResponse" => {
            let v = value
                .downcast_ref::<AsterDEXPositionModeResponse>()
                .unwrap();
            identity_fields!(v; dual_side_position)
        }
        "AsterDEXPositionResponse" => {
            let v = value.downcast_ref::<AsterDEXPositionResponse>().unwrap();
            identity_fields!(v; symbol, position_amt, entry_price, break_even_price, mark_price, un_realized_profit, liquidation_price, leverage, max_notional_value, margin_type, isolated_margin, is_auto_add_margin, position_side, notional, isolated_wallet, update_time, extra)
        }
        "AsterDEXOrderResponse" => {
            let v = value.downcast_ref::<AsterDEXOrderResponse>().unwrap();
            identity_fields!(v; symbol, order_id, client_order_id, price, orig_qty, executed_qty, cummulative_quote_qty, status, time_in_force, r#type, side, stop_price, iceberg_qty, time, update_time, is_working, working_time, orig_quote_order_qty, position_side, close_position, reduce_only, activate_price, price_rate, price_protect, extra)
        }
        "AsterDEXTickerResponse" => {
            let v = value.downcast_ref::<AsterDEXTickerResponse>().unwrap();
            identity_fields!(v; symbol, price_change, price_change_percent, weighted_avg_price, last_price, last_qty, open_price, high_price, low_price, volume, quote_volume, open_time, close_time, first_id, last_id, count, extra)
        }
        "AsterDEXOpenInterestResponse" => {
            let v = value
                .downcast_ref::<AsterDEXOpenInterestResponse>()
                .unwrap();
            identity_fields!(v; symbol, open_interest, time)
        }
        "AsterDEXDepthResponse" => {
            let v = value.downcast_ref::<AsterDEXDepthResponse>().unwrap();
            identity_fields!(v; last_update_id, capital_e, capital_t, bids, asks)
        }
        "AsterDEXTradeResponse" => {
            let v = value.downcast_ref::<AsterDEXTradeResponse>().unwrap();
            identity_fields!(v; id, price, qty, quote_qty, time, is_buyer_maker)
        }
        "AsterDEXUserTradeResponse" => {
            let v = value.downcast_ref::<AsterDEXUserTradeResponse>().unwrap();
            identity_fields!(v; symbol, id, order_id, price, qty, quote_qty, realized_pnl, side, position_side, buyer, maker, commission, commission_asset, time)
        }
        "AsterDEXWebSocketMessage" => {
            let v = value.downcast_ref::<AsterDEXWebSocketMessage>().unwrap();
            identity_fields!(v; e, capital_e, s, extra)
        }
        "AsterDEXStreamTicker" => {
            let v = value.downcast_ref::<AsterDEXStreamTicker>().unwrap();
            identity_fields!(v; e, capital_e, s, p, capital_p, w, c, capital_q, o, h, l, v, q, capital_o, capital_c, capital_f, capital_l, n)
        }
        "AsterDEXStreamDepth" => {
            let v = value.downcast_ref::<AsterDEXStreamDepth>().unwrap();
            identity_fields!(v; e, capital_e, capital_t, s, capital_u, u, pu, b, a)
        }
        "AsterDEXStreamTrade" => {
            let v = value.downcast_ref::<AsterDEXStreamTrade>().unwrap();
            identity_fields!(v; e, capital_e, s, a, p, q, f, l, capital_t, m)
        }
        "AsterDEXStreamMarkPrice" => {
            let v = value.downcast_ref::<AsterDEXStreamMarkPrice>().unwrap();
            identity_fields!(v; e, capital_e, s, p, i, capital_p, r, capital_t)
        }
        "AsterDEXAccountUpdateABItem" => {
            let v = value.downcast_ref::<AsterDEXAccountUpdateABItem>().unwrap();
            identity_fields!(v; a, wb, cw, bc)
        }
        "AsterDEXAccountUpdateAPItem" => {
            let v = value.downcast_ref::<AsterDEXAccountUpdateAPItem>().unwrap();
            identity_fields!(v; s, pa, ep, cr, up, mt, iw, ps)
        }
        "AsterDEXAccountUpdateA" => {
            let v = value.downcast_ref::<AsterDEXAccountUpdateA>().unwrap();
            identity_fields!(v; m, capital_b, capital_p)
        }
        "AsterDEXAccountUpdate" => {
            let v = value.downcast_ref::<AsterDEXAccountUpdate>().unwrap();
            identity_fields!(v; e, capital_e, capital_t, a)
        }
        "AsterDEXOrderUpdateO" => {
            let v = value.downcast_ref::<AsterDEXOrderUpdateO>().unwrap();
            identity_fields!(v; s, c, capital_s, o, f, q, p, ap, sp, x, capital_x, i, l, z, capital_l, capital_n, n, capital_t, t, b, a, m, capital_r, wt, ot, ps, cp, capital_ap, cr, p_p, si, ss, rp)
        }
        "AsterDEXOrderUpdate" => {
            let v = value.downcast_ref::<AsterDEXOrderUpdate>().unwrap();
            identity_fields!(v; e, capital_e, capital_t, o)
        }
        "AsterDEXListenKeyResponse" => {
            let v = value.downcast_ref::<AsterDEXListenKeyResponse>().unwrap();
            identity_fields!(v; listen_key)
        }
        "AsterDEXPlaceOrderOpts" => {
            let v = value.downcast_ref::<AsterDEXPlaceOrderOpts>().unwrap();
            let mut fields = vec![("stp_mode", serde_json::to_value(&v.stp_mode).unwrap())];
            fields.extend(identity_fields!(v.order; symbol, r#type, side, amount, price, trigger_price, stop_loss, take_profit, reduce_only, time_in_force, trigger_type, tp_trigger_type, sl_trigger_type, client_order_id, max_slippage_pct, close, implied_volatility));
            fields
        }
        _ => Vec::new(),
    }
}
fn rust_field_name(wire_name: &str) -> String {
    if wire_name
        .chars()
        .all(|character| character.is_ascii_uppercase())
    {
        return format!("capital_{}", wire_name.to_ascii_lowercase());
    }
    let chars: Vec<char> = wire_name.chars().collect();
    let mut rust_name = String::new();
    for (index, character) in chars.iter().enumerate() {
        if character.is_ascii_uppercase()
            && index > 0
            && (chars[index - 1].is_ascii_lowercase()
                || chars[index - 1].is_ascii_digit()
                || (chars[index - 1].is_ascii_uppercase()
                    && chars
                        .get(index + 1)
                        .is_some_and(|next| next.is_ascii_lowercase())))
        {
            rust_name.push('_');
        }
        rust_name.push(character.to_ascii_lowercase());
    }
    rust_name
}
fn assert_enum_identity<T: std::fmt::Debug>(name: &str, input: &Value, value: &T) {
    let Some(input) = input.as_str() else {
        if name == "AsterDEXPositionModeResponseDualSidePosition" {
            assert_eq!(
                format!("{value:?}"),
                format!("Boolean({})", input.as_bool().unwrap()),
                "{name} decoded boolean variant identity"
            );
            return;
        }
        panic!("{name} has no decoded enum identity mapping for {input}");
    };
    let expected = match (name, input) {
        ("AsterDEXAPIVersion", "v1") => "V1",
        ("AsterDEXAPIVersion", "v3") => "V3",
        ("AsterDEXFapiVersion", "v1") => "V1",
        ("AsterDEXFapiVersion", "v3") => "V3",
        ("AsterDEXFapiVersion", "v4") => "V4",
        ("AsterDEXStpMode", "EXPIRE_TAKER") => "V0",
        ("AsterDEXStpMode", "EXPIRE_MAKER") => "V1",
        ("AsterDEXStpMode", "EXPIRE_BOTH") => "V2",
        ("AsterDEXPositionModeResponseDualSidePositionString", "true") => "V0",
        ("AsterDEXPositionModeResponseDualSidePositionString", "false") => "V1",
        ("AsterDEXPositionModeResponseDualSidePosition", "true") => "String(V0)",
        ("AsterDEXPositionModeResponseDualSidePosition", "false") => "String(V1)",
        ("AsterDEXStreamTickerE", "ticker")
        | ("AsterDEXStreamDepthE", "depthUpdate")
        | ("AsterDEXStreamTradeE", "aggTrade")
        | ("AsterDEXStreamMarkPriceE", "markPriceUpdate")
        | ("AsterDEXAccountUpdateE", "ACCOUNT_UPDATE")
        | ("AsterDEXOrderUpdateE", "ORDER_TRADE_UPDATE") => "V0",
        _ => panic!("{name} has no decoded enum identity mapping for {input}"),
    };
    assert_eq!(
        format!("{value:?}"),
        expected,
        "{name} decoded variant identity"
    );
}
fn assert_native_record_identity<T: Any + std::fmt::Debug>(name: &str, input: &Value, value: &T) {
    if name == "AsterDEXKlineResponse" {
        let value = (value as &dyn Any)
            .downcast_ref::<AsterDEXKlineResponse>()
            .unwrap();
        let actual = [
            json!(value.0),
            json!(value.1),
            json!(value.2),
            json!(value.3),
            json!(value.4),
            json!(value.5),
            json!(value.6),
            json!(value.7),
            json!(value.8),
            json!(value.9),
            json!(value.10),
            json!(value.11),
        ];
        let expected = input.as_array().expect("kline input is a tuple");
        assert_eq!(
            expected.len(),
            12,
            "AsterDEXKlineResponse source tuple count"
        );
        assert_eq!(
            actual.len(),
            expected.len(),
            "AsterDEXKlineResponse native tuple count"
        );
        let mut compared = 0;
        for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
            assert!(
                semantic_eq(actual, expected),
                "AsterDEXKlineResponse[{index}] native tuple identity"
            );
            compared += 1;
        }
        assert_eq!(
            compared,
            expected.len(),
            "AsterDEXKlineResponse compared tuple count"
        );
        return;
    }
    let fields = native_identity_fields(name, value);
    if fields.is_empty() {
        assert_enum_identity(name, input, value);
        return;
    }
    assert_eq!(
        fields.len(),
        source_record_field_count(name) + native_extra_field_count(name),
        "{name} native identity field inventory count"
    );
    let declared_fields = oracle()["recordFields"][name].as_array().unwrap();
    assert_eq!(
        declared_fields.len(),
        source_record_field_count(name),
        "{name} pinned source field count"
    );
    let native_declared_fields: Vec<_> = declared_fields
        .iter()
        .filter_map(|wire_name| {
            let rust_name = rust_field_name(wire_name.as_str().unwrap());
            fields
                .iter()
                .find(|(field, _)| field.trim_start_matches("r#") == rust_name)
        })
        .collect();
    assert_eq!(
        native_declared_fields.len(),
        declared_fields.len(),
        "{name} native declared field count"
    );
    let expected_fields: Vec<_> = declared_fields
        .iter()
        .filter(|wire_name| input.get(wire_name.as_str().unwrap()).is_some())
        .collect();
    let mut compared_fields = 0;
    for wire_name in declared_fields {
        let wire_name = wire_name.as_str().unwrap();
        let Some(expected) = input.get(wire_name) else {
            continue;
        };
        let rust_name = rust_field_name(wire_name);
        let actual = fields
            .iter()
            .find(|(field, _)| field.trim_start_matches("r#") == rust_name)
            .unwrap_or_else(|| panic!("{name}.{wire_name} has no native field {rust_name}"));
        assert_array_cardinality(&actual.1, expected, &format!("{name}.{wire_name}"));
        assert!(
            semantic_eq(&actual.1, expected),
            "{name}.{wire_name} native field identity"
        );
        compared_fields += 1;
    }
    assert_eq!(
        compared_fields,
        expected_fields.len(),
        "{name} source/native field comparison count"
    );
}
fn source_record_counts(name: &str) -> (usize, usize) {
    match name {
        "AsterDEXAPIVersion" => (21, 2),
        "AsterDEXFapiVersion" => (28, 3),
        "AsterDEXStpMode" => (28, 3),
        "AsterDEXV3AuthContext" => (36, 11),
        "AsterDEXMarketResponse" => (348, 92),
        "AsterDEXAccountResponse" => (467, 98),
        "AsterDEXPositionModeResponse" => (29, 4),
        "AsterDEXPositionResponse" => (153, 33),
        "AsterDEXOrderResponse" => (217, 53),
        "AsterDEXTickerResponse" => (153, 33),
        "AsterDEXOpenInterestResponse" => (36, 8),
        "AsterDEXDepthResponse" => (105, 20),
        "AsterDEXTradeResponse" => (60, 11),
        "AsterDEXUserTradeResponse" => (131, 34),
        "AsterDEXKlineResponse" => (105, 24),
        "AsterDEXWebSocketMessage" => (42, 14),
        "AsterDEXStreamTicker" => (176, 35),
        "AsterDEXStreamDepth" => (148, 27),
        "AsterDEXStreamTrade" => (103, 18),
        "AsterDEXStreamMarkPrice" => (87, 15),
        "AsterDEXAccountUpdate" => (204, 38),
        "AsterDEXOrderUpdate" => (340, 68),
        "AsterDEXListenKeyResponse" => (17, 3),
        "AsterDEXPlaceOrderOpts" => (325, 52),
        "AsterDEXMarketResponseFiltersItem" => (117, 36),
        "AsterDEXAccountResponseAssetsItem" => (133, 28),
        "AsterDEXAccountResponsePositionsItem" => (160, 34),
        "AsterDEXAccountUpdateABItem" => (45, 9),
        "AsterDEXAccountUpdateAPItem" => (81, 17),
        "AsterDEXAccountUpdateA" => (162, 33),
        "AsterDEXOrderUpdateO" => (298, 63),
        "AsterDEXPositionModeResponseDualSidePositionString" => (21, 2),
        "AsterDEXPositionModeResponseDualSidePosition" => (21, 4),
        "AsterDEXStreamTickerE" => (14, 1),
        "AsterDEXStreamDepthE" => (15, 1),
        "AsterDEXStreamTradeE" => (15, 1),
        "AsterDEXStreamMarkPriceE" => (15, 1),
        "AsterDEXAccountUpdateE" => (14, 1),
        "AsterDEXOrderUpdateE" => (14, 1),
        _ => panic!("{name} has no pinned source case count"),
    }
}
fn source_record_field_count(name: &str) -> usize {
    match name {
        "AsterDEXAPIVersion"
        | "AsterDEXFapiVersion"
        | "AsterDEXStpMode"
        | "AsterDEXKlineResponse"
        | "AsterDEXPositionModeResponseDualSidePositionString"
        | "AsterDEXPositionModeResponseDualSidePosition"
        | "AsterDEXStreamTickerE"
        | "AsterDEXStreamDepthE"
        | "AsterDEXStreamTradeE"
        | "AsterDEXStreamMarkPriceE"
        | "AsterDEXAccountUpdateE"
        | "AsterDEXOrderUpdateE" => 0,
        "AsterDEXV3AuthContext" => 3,
        "AsterDEXMarketResponse" => 22,
        "AsterDEXAccountResponse" => 19,
        "AsterDEXPositionModeResponse" => 1,
        "AsterDEXPositionResponse" => 16,
        "AsterDEXOrderResponse" => 24,
        "AsterDEXTickerResponse" => 16,
        "AsterDEXOpenInterestResponse" => 3,
        "AsterDEXDepthResponse" => 5,
        "AsterDEXTradeResponse" => 6,
        "AsterDEXUserTradeResponse" => 14,
        "AsterDEXWebSocketMessage" => 3,
        "AsterDEXStreamTicker" => 18,
        "AsterDEXStreamDepth" => 9,
        "AsterDEXStreamTrade" => 10,
        "AsterDEXStreamMarkPrice" => 8,
        "AsterDEXAccountUpdate" | "AsterDEXOrderUpdate" => 4,
        "AsterDEXListenKeyResponse" => 1,
        "AsterDEXPlaceOrderOpts" => 18,
        "AsterDEXMarketResponseFiltersItem" => 12,
        "AsterDEXAccountResponseAssetsItem" => 14,
        "AsterDEXAccountResponsePositionsItem" => 17,
        "AsterDEXAccountUpdateABItem" => 4,
        "AsterDEXAccountUpdateAPItem" => 8,
        "AsterDEXAccountUpdateA" => 3,
        "AsterDEXOrderUpdateO" => 33,
        _ => panic!("{name} has no pinned source field count"),
    }
}
fn native_extra_field_count(name: &str) -> usize {
    match name {
        "AsterDEXMarketResponse"
        | "AsterDEXPositionResponse"
        | "AsterDEXOrderResponse"
        | "AsterDEXTickerResponse"
        | "AsterDEXWebSocketMessage"
        | "AsterDEXMarketResponseFiltersItem" => 1,
        _ => 0,
    }
}
fn collection_field_totals(name: &str) -> &'static [(&'static str, usize)] {
    match name {
        "AsterDEXMarketResponse" => &[
            ("underlyingSubType", 90),
            ("filters", 91),
            ("orderTypes", 91),
            ("timeInForce", 91),
        ],
        "AsterDEXAccountResponse" => &[("assets", 97), ("positions", 97)],
        "AsterDEXDepthResponse" => &[("bids", 19), ("asks", 19)],
        "AsterDEXStreamDepth" => &[("b", 26), ("a", 26)],
        "AsterDEXAccountUpdate" => &[("a.B", 37), ("a.P", 37)],
        "AsterDEXAccountUpdateA" => &[("B", 32), ("P", 32)],
        _ => &[],
    }
}
fn check_record<T: DeserializeOwned + serde::Serialize + Any + std::fmt::Debug>(name: &str) {
    let cases = oracle()["records"][name].as_array().expect("named type");
    let source_case_count = cases.len();
    let source_accepted_count = cases.iter().filter(|case| case["accept"] == true).count();
    let (pinned_source_count, pinned_accepted_count) = source_record_counts(name);
    assert_eq!(
        source_case_count, pinned_source_count,
        "{name} pinned source case count"
    );
    assert_eq!(
        source_accepted_count, pinned_accepted_count,
        "{name} pinned source accepted count"
    );
    assert!(source_case_count > 0, "{name} source case count");
    let mut compared_cases = 0;
    let mut compared_accepted_cases = 0;
    let mut source_collection_elements = vec![0; collection_field_totals(name).len()];
    let mut native_collection_elements = vec![0; collection_field_totals(name).len()];
    for case in cases {
        let decoded = <T as Deserialize>::deserialize(case["input"].clone());
        assert_eq!(
            decoded.is_ok(),
            case["accept"].as_bool().unwrap(),
            "{name} input {}",
            case["input"]
        );
        if let Ok(value) = decoded {
            compared_accepted_cases += 1;
            assert_native_record_identity(name, &case["input"], &value);
            let fields = native_identity_fields(name, &value);
            for (index, (path, _)) in collection_field_totals(name).iter().enumerate() {
                source_collection_elements[index] += nested_array_count(&case["input"], path);
                let (root, nested) = path.split_once('.').unwrap_or((path, ""));
                let native_root = fields
                    .iter()
                    .find(|(field, _)| field.trim_start_matches("r#") == rust_field_name(root))
                    .map(|(_, value)| value)
                    .unwrap_or_else(|| panic!("{name}.{path} missing native collection"));
                native_collection_elements[index] += if nested.is_empty() {
                    native_root.as_array().map_or(0, Vec::len)
                } else {
                    nested_array_count(native_root, nested)
                };
            }
            assert!(
                semantic_eq(&serde_json::to_value(value).unwrap(), &case["input"]),
                "{name} round trip"
            );
        }
        compared_cases += 1;
    }
    assert_eq!(
        compared_cases, source_case_count,
        "{name} source/native case count"
    );
    assert_eq!(
        compared_cases, pinned_source_count,
        "{name} fixed native comparison count"
    );
    assert_eq!(
        compared_accepted_cases, source_accepted_count,
        "{name} source/native accepted case count"
    );
    assert_eq!(
        compared_accepted_cases, pinned_accepted_count,
        "{name} fixed native accepted comparison count"
    );
    for (index, (path, expected_count)) in collection_field_totals(name).iter().enumerate() {
        assert_eq!(
            source_collection_elements[index], *expected_count,
            "{name}.{path} pinned source collection element count"
        );
        assert_eq!(
            native_collection_elements[index], *expected_count,
            "{name}.{path} fixed native collection element count"
        );
    }
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn table_asterdex_endpoints() {
    let mut expected = serde_json::Map::new();
    flatten(&oracle()["tables"]["ASTERDEX_ENDPOINTS"], "", &mut expected);
    let actual: serde_json::Map<String, Value> = ASTERDEX_ENDPOINTS
        .iter()
        .map(|(k, v)| (k.to_string(), json!(v)))
        .collect();
    assert_eq!(actual, expected, "ASTERDEX_ENDPOINTS");
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn table_asterdex_stp_modes() {
    assert_eq!(
        json!(ASTERDEX_STP_MODES),
        oracle()["tables"]["ASTERDEX_STP_MODES"],
        "ASTERDEX_STP_MODES"
    );
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn table_asterdex_main_wallet_required_hint() {
    assert_eq!(
        json!(ASTERDEX_MAIN_WALLET_REQUIRED_HINT),
        oracle()["tables"]["ASTERDEX_MAIN_WALLET_REQUIRED_HINT"],
        "ASTERDEX_MAIN_WALLET_REQUIRED_HINT"
    );
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn table_asterdex_main_wallet_required_message() {
    assert_eq!(
        json!(ASTERDEX_MAIN_WALLET_REQUIRED_MESSAGE),
        oracle()["tables"]["ASTERDEX_MAIN_WALLET_REQUIRED_MESSAGE"],
        "ASTERDEX_MAIN_WALLET_REQUIRED_MESSAGE"
    );
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn table_asterdex_rate_limit_endpoints() {
    let mut expected = serde_json::Map::new();
    flatten(
        &oracle()["tables"]["ASTERDEX_RATE_LIMIT_ENDPOINTS"],
        "",
        &mut expected,
    );
    let actual: serde_json::Map<String, Value> = ASTERDEX_RATE_LIMIT_ENDPOINTS
        .iter()
        .map(|(k, v)| (k.to_string(), json!(v)))
        .collect();
    assert_eq!(actual, expected, "ASTERDEX_RATE_LIMIT_ENDPOINTS");
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn table_asterdexordertypemap() {
    let mut expected = serde_json::Map::new();
    flatten(
        &oracle()["tables"]["asterdexOrderTypeMap"],
        "",
        &mut expected,
    );
    let actual: serde_json::Map<String, Value> = ASTERDEX_ORDER_TYPE_MAP
        .iter()
        .map(|(k, v)| (k.to_string(), json!(v)))
        .collect();
    assert_eq!(actual, expected, "asterdexOrderTypeMap");
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn table_asterdexordersidemap() {
    let mut expected = serde_json::Map::new();
    flatten(
        &oracle()["tables"]["asterdexOrderSideMap"],
        "",
        &mut expected,
    );
    let actual: serde_json::Map<String, Value> = ASTERDEX_ORDER_SIDE_MAP
        .iter()
        .map(|(k, v)| (k.to_string(), json!(v)))
        .collect();
    assert_eq!(actual, expected, "asterdexOrderSideMap");
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn table_asterdexpositionsidemap() {
    let mut expected = serde_json::Map::new();
    flatten(
        &oracle()["tables"]["asterdexPositionSideMap"],
        "",
        &mut expected,
    );
    let actual: serde_json::Map<String, Value> = ASTERDEX_POSITION_SIDE_MAP
        .iter()
        .map(|(k, v)| (k.to_string(), json!(v)))
        .collect();
    assert_eq!(actual, expected, "asterdexPositionSideMap");
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn table_asterdextimeinforcemap() {
    let mut expected = serde_json::Map::new();
    flatten(
        &oracle()["tables"]["asterdexTimeInForceMap"],
        "",
        &mut expected,
    );
    let actual: serde_json::Map<String, Value> = ASTERDEX_TIME_IN_FORCE_MAP
        .iter()
        .map(|(k, v)| (k.to_string(), json!(v)))
        .collect();
    assert_eq!(actual, expected, "asterdexTimeInForceMap");
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn table_asterdexorderstatusmap() {
    let mut expected = serde_json::Map::new();
    flatten(
        &oracle()["tables"]["asterdexOrderStatusMap"],
        "",
        &mut expected,
    );
    let actual: serde_json::Map<String, Value> = ASTERDEX_ORDER_STATUS_MAP
        .iter()
        .map(|(k, v)| (k.to_string(), json!(v)))
        .collect();
    assert_eq!(actual, expected, "asterdexOrderStatusMap");
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn contract_asterdexapiversion() {
    check_record::<AsterDEXAPIVersion>("AsterDEXAPIVersion");
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn contract_asterdexfapiversion() {
    check_record::<AsterDEXFapiVersion>("AsterDEXFapiVersion");
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn contract_asterdexstpmode() {
    check_record::<AsterDEXStpMode>("AsterDEXStpMode");
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn contract_asterdexv3authcontext() {
    check_record::<AsterDEXV3AuthContext>("AsterDEXV3AuthContext");
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn contract_asterdexmarketresponse() {
    check_record::<AsterDEXMarketResponse>("AsterDEXMarketResponse");
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn contract_asterdexaccountresponse() {
    check_record::<AsterDEXAccountResponse>("AsterDEXAccountResponse");
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn contract_asterdexpositionmoderesponse() {
    check_record::<AsterDEXPositionModeResponse>("AsterDEXPositionModeResponse");
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn contract_asterdexpositionresponse() {
    check_record::<AsterDEXPositionResponse>("AsterDEXPositionResponse");
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn contract_asterdexorderresponse() {
    check_record::<AsterDEXOrderResponse>("AsterDEXOrderResponse");
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn contract_asterdextickerresponse() {
    check_record::<AsterDEXTickerResponse>("AsterDEXTickerResponse");
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn contract_asterdexopeninterestresponse() {
    check_record::<AsterDEXOpenInterestResponse>("AsterDEXOpenInterestResponse");
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn contract_asterdexdepthresponse() {
    check_record::<AsterDEXDepthResponse>("AsterDEXDepthResponse");
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn contract_asterdextraderesponse() {
    check_record::<AsterDEXTradeResponse>("AsterDEXTradeResponse");
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn contract_asterdexusertraderesponse() {
    check_record::<AsterDEXUserTradeResponse>("AsterDEXUserTradeResponse");
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn contract_asterdexklineresponse() {
    check_record::<AsterDEXKlineResponse>("AsterDEXKlineResponse");
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn contract_asterdexwebsocketmessage() {
    check_record::<AsterDEXWebSocketMessage>("AsterDEXWebSocketMessage");
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn contract_asterdexstreamticker() {
    check_record::<AsterDEXStreamTicker>("AsterDEXStreamTicker");
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn contract_asterdexstreamdepth() {
    check_record::<AsterDEXStreamDepth>("AsterDEXStreamDepth");
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn contract_asterdexstreamtrade() {
    check_record::<AsterDEXStreamTrade>("AsterDEXStreamTrade");
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn contract_asterdexstreammarkprice() {
    check_record::<AsterDEXStreamMarkPrice>("AsterDEXStreamMarkPrice");
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn contract_asterdexaccountupdate() {
    check_record::<AsterDEXAccountUpdate>("AsterDEXAccountUpdate");
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn contract_asterdexorderupdate() {
    check_record::<AsterDEXOrderUpdate>("AsterDEXOrderUpdate");
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn contract_asterdexlistenkeyresponse() {
    check_record::<AsterDEXListenKeyResponse>("AsterDEXListenKeyResponse");
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn nested_type_record_identity() {
    check_record::<AsterDEXMarketResponseFiltersItem>("AsterDEXMarketResponseFiltersItem");
    check_record::<AsterDEXAccountResponseAssetsItem>("AsterDEXAccountResponseAssetsItem");
    check_record::<AsterDEXAccountResponsePositionsItem>("AsterDEXAccountResponsePositionsItem");
    check_record::<AsterDEXPositionModeResponseDualSidePositionString>(
        "AsterDEXPositionModeResponseDualSidePositionString",
    );
    check_record::<AsterDEXPositionModeResponseDualSidePosition>(
        "AsterDEXPositionModeResponseDualSidePosition",
    );
    check_record::<AsterDEXStreamTickerE>("AsterDEXStreamTickerE");
    check_record::<AsterDEXStreamDepthE>("AsterDEXStreamDepthE");
    check_record::<AsterDEXStreamTradeE>("AsterDEXStreamTradeE");
    check_record::<AsterDEXStreamMarkPriceE>("AsterDEXStreamMarkPriceE");
    check_record::<AsterDEXAccountUpdateE>("AsterDEXAccountUpdateE");
    check_record::<AsterDEXAccountUpdateABItem>("AsterDEXAccountUpdateABItem");
    check_record::<AsterDEXAccountUpdateAPItem>("AsterDEXAccountUpdateAPItem");
    check_record::<AsterDEXAccountUpdateA>("AsterDEXAccountUpdateA");
    check_record::<AsterDEXOrderUpdateE>("AsterDEXOrderUpdateE");
    check_record::<AsterDEXOrderUpdateO>("AsterDEXOrderUpdateO");
}

#[derive(Debug)]
struct UnionInventory(Vec<String>);
impl std::fmt::Display for UnionInventory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
impl std::error::Error for UnionInventory {}
impl serde::de::Error for UnionInventory {
    fn custom<T: std::fmt::Display>(message: T) -> Self {
        Self(vec![message.to_string()])
    }
    fn unknown_variant(_: &str, expected: &'static [&'static str]) -> Self {
        Self(expected.iter().map(|s| (*s).to_owned()).collect())
    }
}
fn union_inventory<T: DeserializeOwned>(name: &str) {
    let error = T::deserialize(serde::de::value::StringDeserializer::<UnionInventory>::new(
        "__invalid__".to_owned(),
    ))
    .err()
    .expect("unknown variant");
    let mut expected: Vec<String> = oracle()["records"][name]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["accept"] == true)
        .filter_map(|c| c["input"].as_str().map(str::to_owned))
        .collect();
    expected.sort();
    expected.dedup();
    let mut actual = error.0;
    actual.sort();
    assert_eq!(actual, expected, "{name} actual serde variants and aliases");
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn union_metadata() {
    union_inventory::<AsterDEXAPIVersion>("AsterDEXAPIVersion");
    union_inventory::<AsterDEXFapiVersion>("AsterDEXFapiVersion");
    union_inventory::<AsterDEXStpMode>("AsterDEXStpMode");
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn source_inventory_has_no_unchecked_entries() {
    let mut actual: Vec<_> = oracle()["tables"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    actual.sort_unstable();
    assert_eq!(
        actual,
        vec![
            "ASTERDEX_ENDPOINTS",
            "ASTERDEX_MAIN_WALLET_REQUIRED_HINT",
            "ASTERDEX_MAIN_WALLET_REQUIRED_MESSAGE",
            "ASTERDEX_RATE_LIMIT_ENDPOINTS",
            "ASTERDEX_STP_MODES",
            "asterdexOrderSideMap",
            "asterdexOrderStatusMap",
            "asterdexOrderTypeMap",
            "asterdexPositionSideMap",
            "asterdexTimeInForceMap"
        ],
        "tables oracle coverage"
    );
    let mut actual: Vec<_> = oracle()["records"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    actual.sort_unstable();
    assert_eq!(
        actual,
        vec![
            "AsterDEXAPIVersion",
            "AsterDEXAccountResponse",
            "AsterDEXAccountResponseAssetsItem",
            "AsterDEXAccountResponsePositionsItem",
            "AsterDEXAccountUpdate",
            "AsterDEXAccountUpdateA",
            "AsterDEXAccountUpdateABItem",
            "AsterDEXAccountUpdateAPItem",
            "AsterDEXAccountUpdateE",
            "AsterDEXDepthResponse",
            "AsterDEXFapiVersion",
            "AsterDEXKlineResponse",
            "AsterDEXListenKeyResponse",
            "AsterDEXMarketResponse",
            "AsterDEXMarketResponseFiltersItem",
            "AsterDEXOpenInterestResponse",
            "AsterDEXOrderResponse",
            "AsterDEXOrderUpdate",
            "AsterDEXOrderUpdateE",
            "AsterDEXOrderUpdateO",
            "AsterDEXPlaceOrderOpts",
            "AsterDEXPositionModeResponse",
            "AsterDEXPositionModeResponseDualSidePosition",
            "AsterDEXPositionModeResponseDualSidePositionString",
            "AsterDEXPositionResponse",
            "AsterDEXStpMode",
            "AsterDEXStreamDepth",
            "AsterDEXStreamDepthE",
            "AsterDEXStreamMarkPrice",
            "AsterDEXStreamMarkPriceE",
            "AsterDEXStreamTicker",
            "AsterDEXStreamTickerE",
            "AsterDEXStreamTrade",
            "AsterDEXStreamTradeE",
            "AsterDEXTickerResponse",
            "AsterDEXTradeResponse",
            "AsterDEXUserTradeResponse",
            "AsterDEXV3AuthContext",
            "AsterDEXWebSocketMessage"
        ],
        "records oracle coverage"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn contract_asterdexchange_place_order_opts() {
    check_record::<AsterDEXPlaceOrderOpts>("AsterDEXPlaceOrderOpts");
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn live_oracle_rejects_in_memory_ts_endpoint_mutation() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "table_asterdex_endpoints",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_ORACLE_CONTROL", "endpoint")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.status.code(),
        Some(101),
        "live TS endpoint mutation must fail comparison: {diagnostics}"
    );
    assert!(
        diagnostics.contains("table_asterdex_endpoints")
            && diagnostics.contains("https://asterdex-control.invalid"),
        "control must reach endpoint comparison, not fail loading compiler: {diagnostics}"
    );
}

// Numeric ingress is checked before serialization: equal numbers can have different bits.
fn audit_number<
    T: DeserializeOwned + serde::Serialize + safe_cex_num::numeric_ingress::JsNumberOutput,
>(
    name: &str,
    field: &str,
    get: impl Fn(&T) -> f64,
) -> (String, String) {
    let baseline = &oracle()["numericRecords"][name]["sample"];
    let _: T = serde_json::from_str(&serde_json::to_string(baseline).unwrap())
        .expect("integer baseline must decode before a field is varied");
    for literal in ["-0", "-0.0", "1", "1.25", "1e400", "-1e400"] {
        let expected =
            u64::from_str_radix(oracle()["numericBits"][literal].as_str().unwrap(), 16).unwrap();
        let mut source = baseline.clone();
        let slot = if source.is_array() {
            &mut source[field.parse::<usize>().unwrap()]
        } else {
            &mut source[field]
        };
        *slot = Value::String("__numeric_literal__".into());
        let text = serde_json::to_string(&source)
            .unwrap()
            .replace("\"__numeric_literal__\"", literal);
        let raw: Value = serde_json::from_str(&text).unwrap();
        let value = if raw.is_array() {
            &raw[field.parse::<usize>().unwrap()]
        } else {
            &raw[field]
        };
        let overflow = literal.contains('e');
        if overflow {
            assert_eq!(value.to_string(), literal.replace("e", "e+"));
        } else {
            assert_eq!(
                value.as_f64().unwrap().to_bits(),
                expected,
                "{name}.{field} ingress witness"
            );
        }
        use safe_cex_num::numeric_ingress::{js_from_str, js_from_value, js_from_value_ref};
        for (path, decoded) in [
            ("raw-str", serde_json::from_str::<T>(&text)),
            ("raw-slice", serde_json::from_slice::<T>(text.as_bytes())),
            ("owned", serde_json::from_value::<T>(raw.clone())),
            ("borrowed", <T as Deserialize>::deserialize(&raw)),
            ("shared-str", js_from_str::<T>(&text)),
            ("shared-owned", js_from_value::<T>(raw.clone())),
            ("shared-borrowed", js_from_value_ref::<T>(&raw)),
        ] {
            let decoded =
                decoded.unwrap_or_else(|error| panic!("{name}.{field} {path} {literal}: {error}"));
            assert_eq!(
                get(&decoded).to_bits(),
                expected,
                "{name}.{field} {path} {literal} native bits"
            );
        }
        if literal.starts_with("-0") {
            let mut native = raw;
            let slot = if native.is_array() {
                &mut native[field.parse::<usize>().unwrap()]
            } else {
                &mut native[field]
            };
            *slot = json!(-0.0_f64);
            let decoded: T = serde_json::from_value(native).unwrap();
            assert_eq!(
                get(&decoded).to_bits(),
                expected,
                "{name}.{field} native negative zero"
            );
        }
    }
    (name.into(), field.into())
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn numeric_ingress_native_bits() {
    let mut covered = std::collections::BTreeSet::new();
    covered.insert(audit_number::<AsterDEXPlaceOrderOpts>(
        "AsterDEXPlaceOrderOpts",
        "amount",
        |v| v.order.amount,
    ));
    covered.insert(audit_number::<AsterDEXPlaceOrderOpts>(
        "AsterDEXPlaceOrderOpts",
        "price",
        |v| v.order.price.unwrap(),
    ));
    covered.insert(audit_number::<AsterDEXPlaceOrderOpts>(
        "AsterDEXPlaceOrderOpts",
        "triggerPrice",
        |v| v.order.trigger_price.unwrap(),
    ));
    covered.insert(audit_number::<AsterDEXPlaceOrderOpts>(
        "AsterDEXPlaceOrderOpts",
        "stopLoss",
        |v| v.order.stop_loss.unwrap(),
    ));
    covered.insert(audit_number::<AsterDEXPlaceOrderOpts>(
        "AsterDEXPlaceOrderOpts",
        "takeProfit",
        |v| v.order.take_profit.unwrap(),
    ));
    covered.insert(audit_number::<AsterDEXPlaceOrderOpts>(
        "AsterDEXPlaceOrderOpts",
        "maxSlippagePct",
        |v| v.order.max_slippage_pct.unwrap(),
    ));
    covered.insert(audit_number::<AsterDEXPlaceOrderOpts>(
        "AsterDEXPlaceOrderOpts",
        "impliedVolatility",
        |v| v.order.implied_volatility.unwrap(),
    ));
    covered.insert(audit_number::<AsterDEXMarketResponse>(
        "AsterDEXMarketResponse",
        "deliveryDate",
        |v| v.delivery_date.unwrap(),
    ));
    covered.insert(audit_number::<AsterDEXMarketResponse>(
        "AsterDEXMarketResponse",
        "onboardDate",
        |v| v.onboard_date.unwrap(),
    ));
    covered.insert(audit_number::<AsterDEXMarketResponse>(
        "AsterDEXMarketResponse",
        "pricePrecision",
        |v| v.price_precision,
    ));
    covered.insert(audit_number::<AsterDEXMarketResponse>(
        "AsterDEXMarketResponse",
        "quantityPrecision",
        |v| v.quantity_precision,
    ));
    covered.insert(audit_number::<AsterDEXMarketResponse>(
        "AsterDEXMarketResponse",
        "baseAssetPrecision",
        |v| v.base_asset_precision,
    ));
    covered.insert(audit_number::<AsterDEXMarketResponse>(
        "AsterDEXMarketResponse",
        "quotePrecision",
        |v| v.quote_precision,
    ));
    covered.insert(audit_number::<AsterDEXMarketResponse>(
        "AsterDEXMarketResponse",
        "settlePlan",
        |v| v.settle_plan.unwrap(),
    ));
    covered.insert(audit_number::<AsterDEXMarketResponseFiltersItem>(
        "AsterDEXMarketResponseFiltersItem",
        "limit",
        |v| v.limit.unwrap(),
    ));
    covered.insert(audit_number::<AsterDEXAccountResponse>(
        "AsterDEXAccountResponse",
        "feeTier",
        |v| v.fee_tier,
    ));
    covered.insert(audit_number::<AsterDEXAccountResponse>(
        "AsterDEXAccountResponse",
        "updateTime",
        |v| v.update_time,
    ));
    covered.insert(audit_number::<AsterDEXAccountResponseAssetsItem>(
        "AsterDEXAccountResponseAssetsItem",
        "updateTime",
        |v| v.update_time,
    ));
    covered.insert(audit_number::<AsterDEXAccountResponsePositionsItem>(
        "AsterDEXAccountResponsePositionsItem",
        "updateTime",
        |v| v.update_time,
    ));
    covered.insert(audit_number::<AsterDEXPositionResponse>(
        "AsterDEXPositionResponse",
        "updateTime",
        |v| v.update_time,
    ));
    covered.insert(audit_number::<AsterDEXOrderResponse>(
        "AsterDEXOrderResponse",
        "orderId",
        |v| v.order_id,
    ));
    covered.insert(audit_number::<AsterDEXOrderResponse>(
        "AsterDEXOrderResponse",
        "time",
        |v| v.time,
    ));
    covered.insert(audit_number::<AsterDEXOrderResponse>(
        "AsterDEXOrderResponse",
        "updateTime",
        |v| v.update_time,
    ));
    covered.insert(audit_number::<AsterDEXOrderResponse>(
        "AsterDEXOrderResponse",
        "workingTime",
        |v| v.working_time,
    ));
    covered.insert(audit_number::<AsterDEXTickerResponse>(
        "AsterDEXTickerResponse",
        "openTime",
        |v| v.open_time,
    ));
    covered.insert(audit_number::<AsterDEXTickerResponse>(
        "AsterDEXTickerResponse",
        "closeTime",
        |v| v.close_time,
    ));
    covered.insert(audit_number::<AsterDEXTickerResponse>(
        "AsterDEXTickerResponse",
        "firstId",
        |v| v.first_id,
    ));
    covered.insert(audit_number::<AsterDEXTickerResponse>(
        "AsterDEXTickerResponse",
        "lastId",
        |v| v.last_id,
    ));
    covered.insert(audit_number::<AsterDEXTickerResponse>(
        "AsterDEXTickerResponse",
        "count",
        |v| v.count,
    ));
    covered.insert(audit_number::<AsterDEXOpenInterestResponse>(
        "AsterDEXOpenInterestResponse",
        "time",
        |v| v.time.unwrap(),
    ));
    covered.insert(audit_number::<AsterDEXDepthResponse>(
        "AsterDEXDepthResponse",
        "lastUpdateId",
        |v| v.last_update_id,
    ));
    covered.insert(audit_number::<AsterDEXDepthResponse>(
        "AsterDEXDepthResponse",
        "E",
        |v| v.capital_e,
    ));
    covered.insert(audit_number::<AsterDEXDepthResponse>(
        "AsterDEXDepthResponse",
        "T",
        |v| v.capital_t,
    ));
    covered.insert(audit_number::<AsterDEXTradeResponse>(
        "AsterDEXTradeResponse",
        "id",
        |v| v.id,
    ));
    covered.insert(audit_number::<AsterDEXTradeResponse>(
        "AsterDEXTradeResponse",
        "time",
        |v| v.time,
    ));
    covered.insert(audit_number::<AsterDEXUserTradeResponse>(
        "AsterDEXUserTradeResponse",
        "id",
        |v| v.id,
    ));
    covered.insert(audit_number::<AsterDEXUserTradeResponse>(
        "AsterDEXUserTradeResponse",
        "orderId",
        |v| v.order_id,
    ));
    covered.insert(audit_number::<AsterDEXUserTradeResponse>(
        "AsterDEXUserTradeResponse",
        "time",
        |v| v.time,
    ));
    covered.insert(audit_number::<AsterDEXKlineResponse>(
        "AsterDEXKlineResponse",
        "0",
        |v| v.0,
    ));
    covered.insert(audit_number::<AsterDEXKlineResponse>(
        "AsterDEXKlineResponse",
        "6",
        |v| v.6,
    ));
    covered.insert(audit_number::<AsterDEXKlineResponse>(
        "AsterDEXKlineResponse",
        "8",
        |v| v.8,
    ));
    covered.insert(audit_number::<AsterDEXWebSocketMessage>(
        "AsterDEXWebSocketMessage",
        "E",
        |v| v.capital_e,
    ));
    covered.insert(audit_number::<AsterDEXStreamTicker>(
        "AsterDEXStreamTicker",
        "E",
        |v| v.capital_e,
    ));
    covered.insert(audit_number::<AsterDEXStreamTicker>(
        "AsterDEXStreamTicker",
        "O",
        |v| v.capital_o,
    ));
    covered.insert(audit_number::<AsterDEXStreamTicker>(
        "AsterDEXStreamTicker",
        "C",
        |v| v.capital_c,
    ));
    covered.insert(audit_number::<AsterDEXStreamTicker>(
        "AsterDEXStreamTicker",
        "F",
        |v| v.capital_f,
    ));
    covered.insert(audit_number::<AsterDEXStreamTicker>(
        "AsterDEXStreamTicker",
        "L",
        |v| v.capital_l,
    ));
    covered.insert(audit_number::<AsterDEXStreamTicker>(
        "AsterDEXStreamTicker",
        "n",
        |v| v.n,
    ));
    covered.insert(audit_number::<AsterDEXStreamDepth>(
        "AsterDEXStreamDepth",
        "E",
        |v| v.capital_e,
    ));
    covered.insert(audit_number::<AsterDEXStreamDepth>(
        "AsterDEXStreamDepth",
        "T",
        |v| v.capital_t,
    ));
    covered.insert(audit_number::<AsterDEXStreamDepth>(
        "AsterDEXStreamDepth",
        "U",
        |v| v.capital_u,
    ));
    covered.insert(audit_number::<AsterDEXStreamDepth>(
        "AsterDEXStreamDepth",
        "u",
        |v| v.u,
    ));
    covered.insert(audit_number::<AsterDEXStreamDepth>(
        "AsterDEXStreamDepth",
        "pu",
        |v| v.pu,
    ));
    covered.insert(audit_number::<AsterDEXStreamTrade>(
        "AsterDEXStreamTrade",
        "E",
        |v| v.capital_e,
    ));
    covered.insert(audit_number::<AsterDEXStreamTrade>(
        "AsterDEXStreamTrade",
        "a",
        |v| v.a,
    ));
    covered.insert(audit_number::<AsterDEXStreamTrade>(
        "AsterDEXStreamTrade",
        "f",
        |v| v.f,
    ));
    covered.insert(audit_number::<AsterDEXStreamTrade>(
        "AsterDEXStreamTrade",
        "l",
        |v| v.l,
    ));
    covered.insert(audit_number::<AsterDEXStreamTrade>(
        "AsterDEXStreamTrade",
        "T",
        |v| v.capital_t,
    ));
    covered.insert(audit_number::<AsterDEXStreamMarkPrice>(
        "AsterDEXStreamMarkPrice",
        "E",
        |v| v.capital_e,
    ));
    covered.insert(audit_number::<AsterDEXStreamMarkPrice>(
        "AsterDEXStreamMarkPrice",
        "T",
        |v| v.capital_t,
    ));
    covered.insert(audit_number::<AsterDEXAccountUpdate>(
        "AsterDEXAccountUpdate",
        "E",
        |v| v.capital_e,
    ));
    covered.insert(audit_number::<AsterDEXAccountUpdate>(
        "AsterDEXAccountUpdate",
        "T",
        |v| v.capital_t,
    ));
    covered.insert(audit_number::<AsterDEXOrderUpdate>(
        "AsterDEXOrderUpdate",
        "E",
        |v| v.capital_e,
    ));
    covered.insert(audit_number::<AsterDEXOrderUpdate>(
        "AsterDEXOrderUpdate",
        "T",
        |v| v.capital_t,
    ));
    covered.insert(audit_number::<AsterDEXOrderUpdateO>(
        "AsterDEXOrderUpdateO",
        "i",
        |v| v.i,
    ));
    covered.insert(audit_number::<AsterDEXOrderUpdateO>(
        "AsterDEXOrderUpdateO",
        "T",
        |v| v.capital_t,
    ));
    covered.insert(audit_number::<AsterDEXOrderUpdateO>(
        "AsterDEXOrderUpdateO",
        "t",
        |v| v.t,
    ));
    covered.insert(audit_number::<AsterDEXOrderUpdateO>(
        "AsterDEXOrderUpdateO",
        "si",
        |v| v.si,
    ));
    covered.insert(audit_number::<AsterDEXOrderUpdateO>(
        "AsterDEXOrderUpdateO",
        "ss",
        |v| v.ss,
    ));
    let expected: std::collections::BTreeSet<_> = oracle()["numericRecords"]
        .as_object()
        .unwrap()
        .iter()
        .flat_map(|(name, record)| {
            record["fields"]
                .as_array()
                .unwrap()
                .iter()
                .map(move |field| {
                    (
                        name.clone(),
                        field
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| field.to_string()),
                    )
                })
        })
        .collect();
    assert_eq!(covered, expected, "numeric ingress field inventory");
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn place_order_numeric_ingress_acceptance() {
    use safe_cex_num::numeric_ingress::{js_from_str, js_from_value, js_from_value_ref};
    let baseline = oracle()["numericRecords"]["AsterDEXPlaceOrderOpts"]["sample"].clone();
    for field in [
        "amount",
        "price",
        "triggerPrice",
        "stopLoss",
        "takeProfit",
        "maxSlippagePct",
        "impliedVolatility",
    ] {
        for literal in ["-0", "-0.0", "1", "1.25", "1e400", "-1e400"] {
            let expected =
                u64::from_str_radix(oracle()["numericBits"][literal].as_str().unwrap(), 16)
                    .unwrap();
            let mut source = baseline.clone();
            source[field] = Value::String("__numeric_literal__".into());
            let text = serde_json::to_string(&source)
                .unwrap()
                .replace("\"__numeric_literal__\"", literal);
            let parsed: Value = serde_json::from_str(&text).unwrap();
            for (path, decoded) in [
                ("raw", js_from_str::<AsterDEXPlaceOrderOpts>(&text)),
                (
                    "owned-value",
                    js_from_value::<AsterDEXPlaceOrderOpts>(parsed.clone()),
                ),
                (
                    "borrowed-value",
                    js_from_value_ref::<AsterDEXPlaceOrderOpts>(&parsed),
                ),
            ] {
                let decoded = decoded.unwrap_or_else(|error| {
                    panic!("AsterDEXPlaceOrderOpts.{field} {path} {literal}: {error}")
                });
                let actual = match field {
                    "amount" => decoded.order.amount,
                    "price" => decoded.order.price.unwrap(),
                    "triggerPrice" => decoded.order.trigger_price.unwrap(),
                    "stopLoss" => decoded.order.stop_loss.unwrap(),
                    "takeProfit" => decoded.order.take_profit.unwrap(),
                    "maxSlippagePct" => decoded.order.max_slippage_pct.unwrap(),
                    "impliedVolatility" => decoded.order.implied_volatility.unwrap(),
                    _ => unreachable!(),
                };
                assert_eq!(
                    actual.to_bits(),
                    expected,
                    "AsterDEXPlaceOrderOpts.{field} {path} {literal} bits"
                );
            }
        }
    }
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn account_asset_numeric_ingress_acceptance() {
    use safe_cex_num::numeric_ingress::{js_from_str, js_from_value, js_from_value_ref};
    let baseline =
        oracle()["numericRecords"]["AsterDEXAccountResponseAssetsItem"]["sample"].clone();
    for literal in ["-0", "-0.0", "1", "1.25", "1e400", "-1e400"] {
        let expected =
            u64::from_str_radix(oracle()["numericBits"][literal].as_str().unwrap(), 16).unwrap();
        let mut source = baseline.clone();
        source["updateTime"] = Value::String("__numeric_literal__".into());
        let text = serde_json::to_string(&source)
            .unwrap()
            .replace("\"__numeric_literal__\"", literal);
        let parsed: Value = serde_json::from_str(&text).unwrap();
        for (path, decoded) in [
            (
                "raw",
                js_from_str::<AsterDEXAccountResponseAssetsItem>(&text),
            ),
            (
                "owned-value",
                js_from_value::<AsterDEXAccountResponseAssetsItem>(parsed.clone()),
            ),
            (
                "borrowed-value",
                js_from_value_ref::<AsterDEXAccountResponseAssetsItem>(&parsed),
            ),
        ] {
            let decoded = decoded.unwrap_or_else(|error| {
                panic!("AsterDEXAccountResponseAssetsItem.updateTime {path} {literal}: {error}")
            });
            assert_eq!(
                decoded.update_time.to_bits(),
                expected,
                "AsterDEXAccountResponseAssetsItem.updateTime {path} {literal} bits"
            );
        }
    }
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn account_asset_numeric_ingress_rejects_oracle_negative_zero_mutation() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "account_asset_numeric_ingress_acceptance",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_NUMERIC_CONTROL", "erase-zero")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("AsterDEXAccountResponseAssetsItem.updateTime raw -0 bits"),
        "oracle mutation must reach the named signed-zero bits assertion: {diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn account_response_numeric_ingress_acceptance() {
    use safe_cex_num::numeric_ingress::{js_from_str, js_from_value, js_from_value_ref};
    let baseline = oracle()["numericRecords"]["AsterDEXAccountResponse"]["sample"].clone();
    for field in [
        "feeTier",
        "updateTime",
        "assets.0.updateTime",
        "positions.0.updateTime",
    ] {
        for literal in ["-0", "-0.0", "1", "1.25", "1e400", "-1e400"] {
            let expected =
                u64::from_str_radix(oracle()["numericBits"][literal].as_str().unwrap(), 16)
                    .unwrap();
            let mut source = baseline.clone();
            match field {
                "feeTier" | "updateTime" => {
                    source[field] = Value::String("__numeric_literal__".into());
                }
                "assets.0.updateTime" => {
                    source["assets"][0]["updateTime"] = Value::String("__numeric_literal__".into());
                }
                "positions.0.updateTime" => {
                    source["positions"][0]["updateTime"] =
                        Value::String("__numeric_literal__".into());
                }
                _ => unreachable!(),
            }
            let text = serde_json::to_string(&source)
                .unwrap()
                .replace("\"__numeric_literal__\"", literal);
            let parsed: Value = serde_json::from_str(&text).unwrap();
            for (path, decoded) in [
                ("raw", js_from_str::<AsterDEXAccountResponse>(&text)),
                (
                    "owned-value",
                    js_from_value::<AsterDEXAccountResponse>(parsed.clone()),
                ),
                (
                    "borrowed-value",
                    js_from_value_ref::<AsterDEXAccountResponse>(&parsed),
                ),
            ] {
                let decoded = decoded.unwrap_or_else(|error| {
                    panic!("AsterDEXAccountResponse.{field} {path} {literal}: {error}")
                });
                let actual = match field {
                    "feeTier" => decoded.fee_tier,
                    "updateTime" => decoded.update_time,
                    "assets.0.updateTime" => decoded.assets[0].update_time,
                    "positions.0.updateTime" => decoded.positions[0].update_time,
                    _ => unreachable!(),
                };
                assert_eq!(
                    actual.to_bits(),
                    expected,
                    "AsterDEXAccountResponse.{field} {path} {literal} bits"
                );
            }
        }
    }
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn account_response_numeric_ingress_rejects_oracle_negative_zero_mutation() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "account_response_numeric_ingress_acceptance",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_NUMERIC_CONTROL", "erase-zero")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("AsterDEXAccountResponse.feeTier raw -0 bits"),
        "oracle mutation must reach the named signed-zero bits assertion: {diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn trade_response_numeric_ingress_acceptance() {
    assert_numeric_ingress_record_field::<AsterDEXOpenInterestResponse>(
        "AsterDEXOpenInterestResponse",
        "time",
        |v| v.time.unwrap(),
    );
    assert_numeric_ingress_record_field::<AsterDEXTradeResponse>(
        "AsterDEXTradeResponse",
        "id",
        |v| v.id,
    );
    assert_numeric_ingress_record_field::<AsterDEXTradeResponse>(
        "AsterDEXTradeResponse",
        "time",
        |v| v.time,
    );
    assert_numeric_ingress_record_field::<AsterDEXUserTradeResponse>(
        "AsterDEXUserTradeResponse",
        "id",
        |v| v.id,
    );
    assert_numeric_ingress_record_field::<AsterDEXUserTradeResponse>(
        "AsterDEXUserTradeResponse",
        "orderId",
        |v| v.order_id,
    );
    assert_numeric_ingress_record_field::<AsterDEXUserTradeResponse>(
        "AsterDEXUserTradeResponse",
        "time",
        |v| v.time,
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn trade_response_numeric_ingress_rejects_oracle_negative_zero_mutation() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "trade_response_numeric_ingress_acceptance",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_NUMERIC_CONTROL", "erase-zero")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("AsterDEXOpenInterestResponse.time raw -0 bits"),
        "oracle mutation must reach the named signed-zero bits assertion: {diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn place_order_numeric_ingress_rejects_oracle_negative_zero_mutation() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "place_order_numeric_ingress_acceptance",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_NUMERIC_CONTROL", "erase-zero")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("AsterDEXPlaceOrderOpts.amount raw -0 bits"),
        "oracle mutation must reach the named negative-zero bits assertion: {diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn numeric_oracle_rejects_erased_zero_sign() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "numeric_ingress_native_bits",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_NUMERIC_CONTROL", "erase-zero")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.status.code(),
        Some(101),
        "numeric sign control: {diagnostics}"
    );
    assert!(
        diagnostics.contains("AsterDEXPlaceOrderOpts.amount ingress witness"),
        "numeric sign control must reach native-bit assertion: {diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn numeric_oracle_rejects_erased_finite_payload() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "place_order_numeric_ingress_acceptance",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_NUMERIC_CONTROL", "erase-finite")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("AsterDEXPlaceOrderOpts.amount raw 1.25 bits"),
        "finite-payload control must reach the named field assertion: {diagnostics}"
    );
}

fn assert_opaque_numeric_field<T>(name: &str, extra: impl Fn(&T) -> &serde_json::Map<String, Value>)
where
    T: DeserializeOwned + serde::Serialize + safe_cex_num::numeric_ingress::JsNumberOutput,
{
    use safe_cex_num::numeric_ingress::{
        js_from_str, js_from_value, js_from_value_ref, js_value_as_f64,
    };
    for literal in ["-0", "-0.0", "1", "1.25", "1e400", "-1e400"] {
        let expected =
            u64::from_str_radix(oracle()["numericBits"][literal].as_str().unwrap(), 16).unwrap();
        let mut input = oracle()["numericRecords"][name]["sample"].clone();
        input["opaqueNumericExtension"] = json!("__numeric_literal__");
        let text = input
            .to_string()
            .replace("\"__numeric_literal__\"", literal);
        let raw: Value = serde_json::from_str(&text).unwrap();
        for (path, result) in [
            ("raw-str", serde_json::from_str::<T>(&text)),
            ("raw-slice", serde_json::from_slice::<T>(text.as_bytes())),
            ("owned", serde_json::from_value::<T>(raw.clone())),
            ("borrowed", T::deserialize(&raw)),
            ("shared-str", js_from_str::<T>(&text)),
            ("shared-owned", js_from_value::<T>(raw.clone())),
            ("shared-borrowed", js_from_value_ref::<T>(&raw)),
        ] {
            let decoded =
                result.unwrap_or_else(|error| panic!("{name} opaque {path} {literal}: {error}"));
            let value = extra(&decoded)
                .get("opaqueNumericExtension")
                .expect("native opaque own field");
            assert_eq!(
                js_value_as_f64(value)
                    .expect("native opaque numeric value")
                    .to_bits(),
                expected,
                "{name} opaque {path} {literal} native bits"
            );
        }
    }
}
#[test]
#[ignore = "requires live isolated TypeScript numeric oracle"]
fn opaque_extensions_retain_native_numeric_values() {
    assert_opaque_numeric_field::<AsterDEXMarketResponseFiltersItem>(
        "AsterDEXMarketResponseFiltersItem",
        |v| &v.extra,
    );
    assert_opaque_numeric_field::<AsterDEXMarketResponse>("AsterDEXMarketResponse", |v| &v.extra);
    assert_opaque_numeric_field::<AsterDEXPositionResponse>("AsterDEXPositionResponse", |v| {
        &v.extra
    });
    assert_opaque_numeric_field::<AsterDEXOrderResponse>("AsterDEXOrderResponse", |v| &v.extra);
    assert_opaque_numeric_field::<AsterDEXTickerResponse>("AsterDEXTickerResponse", |v| &v.extra);
    assert_opaque_numeric_field::<AsterDEXWebSocketMessage>("AsterDEXWebSocketMessage", |v| {
        &v.extra
    });
}

fn assert_record_depth_cost<
    T: DeserializeOwned
        + serde::Serialize
        + safe_cex_num::numeric_ingress::JsNumberOutput
        + std::fmt::Debug,
>(
    name: &str,
    known: impl Fn(&T) -> f64,
) {
    let source = &oracle()["deepRecords"][name];
    assert_eq!(source["depth"], 140, "independent source depth floor");
    assert_eq!(source["leaf"], 1, "source depth native leaf");
    assert_eq!(
        source["accepted"], true,
        "source accepts deep extension for {name}"
    );
    let mut deep = json!(1);
    for _ in 0..140 {
        deep = Value::Array(vec![deep]);
    }
    let mut raw = oracle()["numericRecords"][name]["sample"].clone();
    raw["depthPayload"] = deep;
    let text = source["text"].as_str().unwrap();
    for (path, result) in [
        ("raw-str", serde_json::from_str::<T>(text)),
        ("raw-slice", serde_json::from_slice::<T>(text.as_bytes())),
        ("owned", serde_json::from_value::<T>(raw.clone())),
        ("borrowed", T::deserialize(&raw)),
        (
            "shared-str",
            safe_cex_num::numeric_ingress::js_from_str::<T>(text),
        ),
        (
            "shared-owned",
            safe_cex_num::numeric_ingress::js_from_value::<T>(raw.clone()),
        ),
        (
            "shared-borrowed",
            safe_cex_num::numeric_ingress::js_from_value_ref::<T>(&raw),
        ),
    ] {
        if let Ok(decoded) = &result {
            assert_eq!(
                known(decoded).to_bits(),
                1_f64.to_bits(),
                "legacy accepted native number"
            );
        }
        let error = result.expect_err(&format!("native depth guard must refuse {name}/{path}"));
        assert!(
            error.to_string().contains("recursion limit"),
            "native {name}/{path} depth refusal: {error}"
        );
    }
}
#[test]
#[ignore = "requires live isolated TypeScript depth oracle"]
fn object_record_depth_cost_is_explicit() {
    assert_record_depth_cost::<AsterDEXPlaceOrderOpts>("AsterDEXPlaceOrderOpts", |v| {
        v.order.amount
    });
    assert_record_depth_cost::<AsterDEXMarketResponse>("AsterDEXMarketResponse", |v| {
        v.delivery_date.unwrap()
    });
    assert_record_depth_cost::<AsterDEXAccountResponse>("AsterDEXAccountResponse", |v| v.fee_tier);
    assert_record_depth_cost::<AsterDEXPositionResponse>("AsterDEXPositionResponse", |v| {
        v.update_time
    });
    assert_record_depth_cost::<AsterDEXOrderResponse>("AsterDEXOrderResponse", |v| v.order_id);
    assert_record_depth_cost::<AsterDEXTickerResponse>("AsterDEXTickerResponse", |v| v.open_time);
    assert_record_depth_cost::<AsterDEXOpenInterestResponse>("AsterDEXOpenInterestResponse", |v| {
        v.time.unwrap()
    });
    assert_record_depth_cost::<AsterDEXDepthResponse>("AsterDEXDepthResponse", |v| {
        v.last_update_id
    });
    assert_record_depth_cost::<AsterDEXTradeResponse>("AsterDEXTradeResponse", |v| v.id);
    assert_record_depth_cost::<AsterDEXUserTradeResponse>("AsterDEXUserTradeResponse", |v| v.id);
    assert_record_depth_cost::<AsterDEXWebSocketMessage>("AsterDEXWebSocketMessage", |v| {
        let mut leaf = &v.extra["depthPayload"];
        let mut depth = 0;
        while let Some(array) = leaf.as_array() {
            assert_eq!(array.len(), 1);
            depth += 1;
            leaf = &array[0];
        }
        assert_eq!(depth, 140, "legacy native websocket payload depth");
        assert_eq!(
            leaf.as_i64(),
            Some(1),
            "legacy native websocket payload leaf"
        );
        v.capital_e
    });
    assert_record_depth_cost::<AsterDEXStreamTicker>("AsterDEXStreamTicker", |v| v.capital_e);
    assert_record_depth_cost::<AsterDEXStreamDepth>("AsterDEXStreamDepth", |v| v.capital_e);
    assert_record_depth_cost::<AsterDEXStreamTrade>("AsterDEXStreamTrade", |v| v.capital_e);
    assert_record_depth_cost::<AsterDEXStreamMarkPrice>("AsterDEXStreamMarkPrice", |v| v.capital_e);
    assert_record_depth_cost::<AsterDEXAccountUpdate>("AsterDEXAccountUpdate", |v| v.capital_e);
    assert_record_depth_cost::<AsterDEXOrderUpdate>("AsterDEXOrderUpdate", |v| v.capital_e);
}
