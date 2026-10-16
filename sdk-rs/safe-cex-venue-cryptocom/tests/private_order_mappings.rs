use safe_cex_types::{OrderStatus, OrderTimeInForce, OrderType};
use safe_cex_venue_cryptocom::private_order_mappings as mappings;
use serde::Deserialize;
use serde_json::{Value, json};
use std::process::Command;

#[derive(Deserialize)]
struct Case {
    label: String,
    units: Option<Vec<u16>>,
    result: String,
}
#[derive(Deserialize)]
struct Group {
    method: String,
    rows: Vec<Case>,
}
#[derive(Deserialize)]
struct Capture {
    captures: Vec<Group>,
    inventory: Value,
    touches: usize,
    coercions: usize,
}
#[derive(Debug, PartialEq)]
enum Native {
    Type(OrderType),
    Status(OrderStatus),
    Tif(OrderTimeInForce),
}

// Decode source values to native variants without the Rust enums' wire codecs.
fn source_native(method: &str, value: &str) -> Native {
    match method {
        "mapOrderType" => Native::Type(match value {
            "market" => OrderType::Market,
            "limit" => OrderType::Limit,
            "stop_market" => OrderType::StopLoss,
            "take_profit_market" => OrderType::TakeProfit,
            _ => panic!("unexpected source order type {value}"),
        }),
        "mapOrderStatus" => Native::Status(match value {
            "open" => OrderStatus::Open,
            "closed" => OrderStatus::Closed,
            "canceled" => OrderStatus::Canceled,
            _ => panic!("unexpected source order status {value}"),
        }),
        "mapTimeInForce" => Native::Tif(match value {
            "GoodTillCancel" => OrderTimeInForce::GoodTillCancel,
            "ImmediateOrCancel" => OrderTimeInForce::ImmediateOrCancel,
            "FillOrKill" => OrderTimeInForce::FillOrKill,
            "PostOnly" => OrderTimeInForce::PostOnly,
            _ => panic!("unexpected source time in force {value}"),
        }),
        _ => panic!("unexpected source method {method}"),
    }
}

#[test]
fn private_order_mappings_match_actual_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/private_order_mappings_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "PRIVATE-MAPPING source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Capture = serde_json::from_slice(&output.stdout).unwrap();
    let mut compared = 0;
    for group in &capture.captures {
        for (index, case) in group.rows.iter().enumerate() {
            let value = case.units.as_deref();
            let native = match group.method.as_str() {
                "mapOrderType" => Native::Type(mappings::map_order_type(value)),
                "mapOrderStatus" => Native::Status(mappings::map_order_status(value)),
                "mapTimeInForce" => Native::Tif(mappings::map_time_in_force(value)),
                _ => panic!("unexpected source group {}", group.method),
            };
            assert_eq!(
                native,
                source_native(&group.method, &case.result),
                "CRYPTOCOM-PRIVATE-MAPPING-NATIVE {} case{}",
                group.method,
                index
            );
            compared += 1;
        }
    }
    // All native comparisons precede the independent inventory/shape checks.
    let specs: [(&str, &[&str], usize); 3] = [
        (
            "mapOrderType",
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
            30,
        ),
        (
            "mapOrderStatus",
            &[
                "ACTIVE", "PENDING", "FILLED", "CANCELED", "EXPIRED", "REJECTED", "", "UNKNOWN",
                "active",
            ],
            30,
        ),
        (
            "mapTimeInForce",
            &[
                "GOOD_TILL_CANCEL",
                "IMMEDIATE_OR_CANCEL",
                "FILL_OR_KILL",
                "POST_ONLY",
                "",
                "UNKNOWN",
                "immediate_or_cancel",
            ],
            28,
        ),
    ];
    const NON_STRINGS: [&str; 16] = [
        "undefined",
        "null",
        "false",
        "true",
        "zero",
        "negative",
        "nan",
        "infinity",
        "object",
        "array",
        "boxed",
        "symbol",
        "bigint",
        "function",
        "coercion-trap",
        "proxy-trap",
    ];
    assert_eq!(capture.captures.len(), 3, "PRIVATE-MAPPING group count");
    assert_eq!(
        compared, 88,
        "PRIVATE-MAPPING independent native comparison total"
    );
    for (group, (method, strings, count)) in capture.captures.iter().zip(specs) {
        assert_eq!(group.method, method, "PRIVATE-MAPPING method identity");
        assert_eq!(
            group.rows.len(),
            count,
            "PRIVATE-MAPPING independent group floor"
        );
        let mut expected: Vec<Vec<u16>> =
            strings.iter().map(|s| s.encode_utf16().collect()).collect();
        expected.extend([
            vec![0xd800],
            vec![0xdfff],
            vec![0xd83d, 0xde42],
            strings[0].encode_utf16().chain([0xd800]).collect(),
            [0xdfff]
                .into_iter()
                .chain(strings[0].encode_utf16())
                .collect(),
        ]);
        for (case, units) in group.rows.iter().zip(&expected) {
            assert_eq!(
                case.label, "string",
                "PRIVATE-MAPPING input string classification"
            );
            assert_eq!(
                case.units.as_deref(),
                Some(units.as_slice()),
                "PRIVATE-MAPPING independent UTF16 input"
            );
        }
        for (case, label) in group.rows.iter().skip(expected.len()).zip(NON_STRINGS) {
            assert_eq!(case.label, label, "PRIVATE-MAPPING non-string identity");
            assert!(case.units.is_none(), "PRIVATE-MAPPING default class");
        }
    }
    assert_eq!(
        capture.inventory,
        json!({
            "OrderType": [["Market","market"],["Limit","limit"],["Conditional","conditional"],["StopLoss","stop_market"],["TakeProfit","take_profit_market"],["TrailingStopLoss","trailing_stop_market"]],
            "OrderStatus": [["Open","open"],["Closed","closed"],["Canceled","canceled"],["Creating","creating"],["Updating","updating"],["Canceling","canceling"]],
            "OrderTimeInForce": [["GoodTillCancel","GoodTillCancel"],["ImmediateOrCancel","ImmediateOrCancel"],["FillOrKill","FillOrKill"],["PostOnly","PostOnly"]]
        }),
        "PRIVATE-MAPPING actual model enum inventories"
    );
    assert_eq!(capture.touches, 0, "PRIVATE-MAPPING no receiver effects");
    assert_eq!(capture.coercions, 0, "PRIVATE-MAPPING no coercion effects");
    println!("PRIVATE-MAPPING floor: 88 native cases, last mapTimeInForce case27 proxy-trap");
}
