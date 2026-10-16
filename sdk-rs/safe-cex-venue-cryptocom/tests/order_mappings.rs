use safe_cex_types::{OrderStatus, OrderTimeInForce, OrderType};
use safe_cex_venue_cryptocom::{
    order_mappings::*,
    wire::{CryptocomOrderType, CryptocomRefPriceType, CryptocomTimeInForce},
};
use serde_json::{Value, json};
use std::process::Command;

#[derive(Debug, PartialEq)]
enum Native {
    Status(OrderStatus),
    OrderType(OrderType),
    TimeInForce(OrderTimeInForce),
    VenueOrderType(CryptocomOrderType),
    VenueTimeInForce(CryptocomTimeInForce),
    RefPrice(CryptocomRefPriceType),
}

// Decode the source's finite return vocabulary into native enum variants directly,
// independently of the venue enum's Serialize/Deserialize implementations.
fn source_native(method: &str, value: &str) -> Native {
    match method {
        "mapOrderStatus" => Native::Status(match value {
            "open" => OrderStatus::Open,
            "closed" => OrderStatus::Closed,
            "canceled" => OrderStatus::Canceled,
            _ => panic!("mapping_source_status:{value}"),
        }),
        "mapOrderType" => Native::VenueOrderType(match value {
            "MARKET" => CryptocomOrderType::Market,
            "LIMIT" => CryptocomOrderType::Limit,
            "STOP_LOSS" => CryptocomOrderType::StopLoss,
            "TAKE_PROFIT" => CryptocomOrderType::TakeProfit,
            _ => panic!("mapping_source_order_type:{value}"),
        }),
        "mapCryptocomOrderType" => Native::OrderType(match value {
            "market" => OrderType::Market,
            "limit" => OrderType::Limit,
            "stop_market" => OrderType::StopLoss,
            "take_profit_market" => OrderType::TakeProfit,
            _ => panic!("mapping_source_cryptocom_order_type:{value}"),
        }),
        "mapTimeInForce" => Native::VenueTimeInForce(match value {
            "GOOD_TILL_CANCEL" => CryptocomTimeInForce::GoodTillCancel,
            "IMMEDIATE_OR_CANCEL" => CryptocomTimeInForce::ImmediateOrCancel,
            "FILL_OR_KILL" => CryptocomTimeInForce::FillOrKill,
            "POST_ONLY" => CryptocomTimeInForce::PostOnly,
            _ => panic!("mapping_source_time_in_force:{value}"),
        }),
        "mapStopTriggerType" => Native::RefPrice(match value {
            "MARK_PRICE" => CryptocomRefPriceType::Mark,
            "INDEX_PRICE" => CryptocomRefPriceType::Index,
            "LAST_PRICE" => CryptocomRefPriceType::Last,
            _ => panic!("mapping_source_stop_trigger:{value}"),
        }),
        "mapCryptocomTimeInForce" => Native::TimeInForce(match value {
            "GoodTillCancel" => OrderTimeInForce::GoodTillCancel,
            "ImmediateOrCancel" => OrderTimeInForce::ImmediateOrCancel,
            "FillOrKill" => OrderTimeInForce::FillOrKill,
            "PostOnly" => OrderTimeInForce::PostOnly,
            _ => panic!("mapping_source_cryptocom_time_in_force:{value}"),
        }),
        _ => panic!("mapping_source_method:{method}"),
    }
}

#[test]
fn live_order_mappings() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/order_mappings_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "mapping_source:{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        corpus["enumInventory"],
        json!({
            "OrderType": [["Market","market"],["Limit","limit"],["Conditional","conditional"],
                ["StopLoss","stop_market"],["TakeProfit","take_profit_market"],["TrailingStopLoss","trailing_stop_market"]],
            "OrderTimeInForce": [["GoodTillCancel","GoodTillCancel"],["ImmediateOrCancel","ImmediateOrCancel"],
                ["FillOrKill","FillOrKill"],["PostOnly","PostOnly"]],
            "OrderStopTriggerType": [["MarkPrice","mark_price"],["LastPrice","last_price"],["IndexPrice","index_price"]],
        }),
        "mapping_enum_inventory"
    );
    let groups: [(&str, &[&str]); 6] = [
        (
            "mapOrderStatus",
            &[
                "ACTIVE", "PENDING", "FILLED", "CANCELED", "EXPIRED", "REJECTED", "", "UNKNOWN",
                "active",
            ],
        ),
        (
            "mapOrderType",
            &[
                "market",
                "limit",
                "conditional",
                "stop_market",
                "take_profit_market",
                "trailing_stop_market",
                "",
                "UNKNOWN",
                "MARKET",
            ],
        ),
        (
            "mapCryptocomOrderType",
            &[
                "MARKET",
                "LIMIT",
                "STOP_LOSS",
                "STOP_LIMIT",
                "TAKE_PROFIT",
                "TAKE_PROFIT_LIMIT",
                "",
                "UNKNOWN",
                "market",
            ],
        ),
        (
            "mapTimeInForce",
            &[
                "GoodTillCancel",
                "ImmediateOrCancel",
                "FillOrKill",
                "PostOnly",
                "",
                "UNKNOWN",
                "POST_ONLY",
            ],
        ),
        (
            "mapStopTriggerType",
            &[
                "mark_price",
                "last_price",
                "index_price",
                "",
                "UNKNOWN",
                "INDEX_PRICE",
            ],
        ),
        (
            "mapCryptocomTimeInForce",
            &[
                "GOOD_TILL_CANCEL",
                "IMMEDIATE_OR_CANCEL",
                "FILL_OR_KILL",
                "POST_ONLY",
                "",
                "UNKNOWN",
                "PostOnly",
            ],
        ),
    ];
    let non_string_labels = [
        "undefined",
        "null",
        "false",
        "true",
        "zero",
        "nan",
        "infinity",
        "object",
        "array",
        "boxed",
        "symbol",
        "bigint",
        "function",
        "throwing",
    ];
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6, "mapping_group_count");
    let expected_lengths = [28, 28, 28, 26, 25, 26];
    let mut comparisons = 0;
    for (group_index, (method, strings)) in groups.into_iter().enumerate() {
        assert_eq!(
            cases[group_index]["method"], method,
            "mapping_method_admission"
        );
        let rows = cases[group_index]["rows"].as_array().unwrap();
        assert_eq!(
            rows.len(),
            expected_lengths[group_index],
            "mapping_source_count:{method}"
        );
        let mut inputs: Vec<(&str, Option<Vec<u16>>)> = strings
            .iter()
            .map(|s| ("string", Some(s.encode_utf16().collect())))
            .collect();
        inputs.extend([
            ("string", Some(vec![0xd800])),
            ("string", Some(vec![0xdc00])),
            ("string", Some("🙂".encode_utf16().collect())),
            (
                "string",
                Some(strings[0].encode_utf16().chain([0xd800]).collect()),
            ),
            (
                "string",
                Some(
                    [0xdc00]
                        .into_iter()
                        .chain(strings[0].encode_utf16())
                        .collect(),
                ),
            ),
        ]);
        inputs.extend(non_string_labels.into_iter().map(|label| (label, None)));
        assert_eq!(
            inputs.len(),
            expected_lengths[group_index],
            "mapping_inventory_count:{method}"
        );
        for (index, (label, units)) in inputs.into_iter().enumerate() {
            let row = &rows[index];
            assert_eq!(
                row["label"], label,
                "mapping_label_admission:{method}:{index}"
            );
            assert_eq!(
                row["units"],
                json!(units),
                "mapping_units_admission:{method}:{index}"
            );
            let units = units.as_deref();
            let native = match method {
                "mapOrderStatus" => Native::Status(map_order_status(units)),
                "mapOrderType" => Native::VenueOrderType(map_order_type(units)),
                "mapCryptocomOrderType" => Native::OrderType(map_cryptocom_order_type(units)),
                "mapTimeInForce" => Native::VenueTimeInForce(map_time_in_force(units)),
                "mapStopTriggerType" => Native::RefPrice(map_stop_trigger_type(units)),
                "mapCryptocomTimeInForce" => {
                    Native::TimeInForce(map_cryptocom_time_in_force(units))
                }
                _ => unreachable!(),
            };
            assert_eq!(
                native,
                source_native(method, row["result"].as_str().unwrap()),
                "mapping_native:{method}:{index}"
            );
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, 161, "mapping_executed_comparisons");
    assert_eq!(corpus["coercions"], 0, "mapping_no_argument_coercion");
}
