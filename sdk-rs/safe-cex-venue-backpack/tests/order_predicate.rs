use safe_cex_types::{OrderSide, OrderStopTriggerType, OrderType, PlaceOrderOpts};
use safe_cex_venue_backpack::order_utils::is_ordinary_limit_order;
use serde_json::Value;
use std::process::Command;

fn order_type(value: &str) -> OrderType {
    match value {
        "market" => OrderType::Market,
        "limit" => OrderType::Limit,
        "conditional" => OrderType::Conditional,
        "stop_market" => OrderType::StopLoss,
        "take_profit_market" => OrderType::TakeProfit,
        "trailing_stop_market" => OrderType::TrailingStopLoss,
        _ => panic!("unexpected source order type: {value}"),
    }
}
fn numeric(case: &Value, field: &str) -> Option<f64> {
    case["numeric"][field]
        .as_str()
        .map(|s| f64::from_bits(u64::from_str_radix(s, 16).unwrap()))
}
#[test]
fn actual_typescript_ordinary_limit_predicate() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/order_predicate_oracle.cjs"
        ))
        .output()
        .expect("one actual TypeScript predicate corpus");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    for key in ["node", "arch", "platform"] {
        assert!(!corpus["runtime"][key].as_str().unwrap().is_empty());
    }
    eprintln!("predicate runtime: {}", corpus["runtime"]);
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 576, "ORDINARY-CARDINALITY source cases");
    for name in [
        "market",
        "limit",
        "conditional",
        "stop_market",
        "take_profit_market",
        "trailing_stop_market",
    ] {
        assert_eq!(
            cases.iter().filter(|c| c["type"] == name).count(),
            96,
            "ORDINARY-CARDINALITY order type {name}"
        );
    }
    for mask in 0..32 {
        assert_eq!(
            cases.iter().filter(|c| c["mask"] == mask).count(),
            18,
            "ORDINARY-CARDINALITY field mask {mask}"
        );
    }
    let mut compared = [0; 2];
    for case in cases {
        let mask = case["mask"].as_u64().unwrap();
        let opts = PlaceOrderOpts {
            symbol: "BTC_USDC_PERP".into(),
            r#type: order_type(case["type"].as_str().unwrap()),
            side: OrderSide::Buy,
            amount: 1.0,
            price: None,
            trigger_price: numeric(case, "triggerPrice"),
            take_profit: numeric(case, "takeProfit"),
            stop_loss: numeric(case, "stopLoss"),
            tp_trigger_type: (mask & 8 != 0).then_some(OrderStopTriggerType::MarkPrice),
            sl_trigger_type: (mask & 16 != 0).then_some(OrderStopTriggerType::LastPrice),
            close: case["close"].as_bool(),
            reduce_only: None,
            time_in_force: None,
            trigger_type: None,
            client_order_id: None,
            max_slippage_pct: None,
            implied_volatility: None,
        };
        let source = case["actual"].as_bool().expect("native source bool");
        assert_eq!(
            is_ordinary_limit_order(&opts),
            source,
            "ordinary native predicate type {} mask {mask} close {:?}",
            case["type"],
            opts.close
        );
        compared[usize::from(!source)] += 1;
    }
    assert_eq!(
        compared,
        [2, 574],
        "ORDINARY-CARDINALITY completed comparisons"
    );
    let boundary = corpus["invalid"].as_array().unwrap();
    assert_eq!(boundary.len(), 5, "ORDINARY-CARDINALITY null boundaries");
    let mut refusals = 0;
    for case in boundary {
        assert_eq!(
            case["actual"], false,
            "source null is present {}",
            case["field"]
        );
        assert!(
            safe_cex_num::numeric_ingress::js_from_str::<PlaceOrderOpts>(
                case["raw"].as_str().unwrap()
            )
            .is_err(),
            "shared typed opts refuse explicit null {}",
            case["field"]
        );
        refusals += 1;
    }
    assert_eq!(refusals, 5, "ORDINARY-CARDINALITY completed boundaries");
    let strings = corpus["strings"].as_array().unwrap();
    assert_eq!(strings.len(), 2, "ORDINARY-CARDINALITY string boundaries");
    let positive = safe_cex_num::numeric_ingress::js_from_str::<PlaceOrderOpts>(
        strings[0]["raw"].as_str().unwrap(),
    )
    .unwrap();
    assert_eq!(positive.symbol, "λ🚀", "ordinary typed Unicode symbol");
    assert!(
        strings[0]["actual"].as_bool().unwrap() && is_ordinary_limit_order(&positive),
        "ordinary Unicode source/native positive"
    );
    assert!(
        strings[1]["actual"].as_bool().unwrap(),
        "actual source ignores surrogate symbol"
    );
    typed_utf16::assert_projection(
        strings[1]["raw"].as_str().unwrap(),
        &(safe_cex_num::numeric_ingress::js_from_str::<PlaceOrderOpts>(
            strings[1]["raw"].as_str().unwrap(),
        ))
        .expect("typed surrogate record is retained"),
    );
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;
