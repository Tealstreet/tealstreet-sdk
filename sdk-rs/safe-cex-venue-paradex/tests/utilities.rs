use safe_cex_num::{JsCoercionError, numeric_ingress::js_deserialize_passthrough};
use safe_cex_types::{
    HistoricalOrder, IncomeCategory, IncomeRecord, OrderSide, OrderStatus, OrderType,
};
use safe_cex_venue_paradex::{income, order_history};
use serde_json::{Value, json};
use std::process::Command;

fn number(n: f64) -> Value {
    json!({"type":"number","bits":format!("{:016x}", if n.is_nan() {0x7ff8000000000000} else {n.to_bits()})})
}
fn string(s: &str) -> Value {
    json!({"type":"string","value":s})
}
fn optional(n: Option<f64>) -> Value {
    n.map(number).unwrap_or(json!({"type":"undefined"}))
}
fn capture<T>(result: Result<T, JsCoercionError>, observe: impl FnOnce(T) -> Value) -> Value {
    match result {
        Ok(value) => json!({"ok":true,"value":observe(value)}),
        Err(e) => json!({"ok":false,"name":e.name,"message":e.message}),
    }
}
fn decode(raw: &str) -> Value {
    js_deserialize_passthrough(&mut serde_json::Deserializer::from_str(raw)).unwrap()
}
fn history(value: Option<HistoricalOrder>) -> Value {
    let Some(o) = value else {
        return json!({"type":"null"});
    };
    assert!(
        o.position_side.is_none()
            && o.realized_pnl.is_none()
            && o.fees.is_none()
            && o.fee_asset.is_none()
            && o.rebate.is_none()
            && o.rebate_asset.is_none(),
        "history_absent_fields"
    );
    let side = match o.side {
        OrderSide::Buy => "buy",
        OrderSide::Sell => "sell",
    };
    let status = match o.status {
        OrderStatus::Open => "open",
        OrderStatus::Closed => "closed",
        OrderStatus::Canceled => "canceled",
        OrderStatus::Creating => "creating",
        OrderStatus::Updating => "updating",
        OrderStatus::Canceling => "canceling",
    };
    let kind = match o.r#type {
        OrderType::Limit => "limit",
        OrderType::Market => "market",
        OrderType::StopLoss => "stop_market",
        OrderType::TakeProfit => "take_profit_market",
        OrderType::Conditional => "conditional",
        OrderType::TrailingStopLoss => "trailing_stop_market",
    };
    json!({"id":string(&o.id),"symbol":string(&o.symbol),"side":string(side),"status":string(status),"type":string(kind),"price":optional(o.price),"averagePrice":optional(o.average_price),"amount":number(o.amount),"filled":number(o.filled),"reduceOnly":{"type":"boolean","value":o.reduce_only.expect("history_reduce_only_present")},"createdTime":number(o.created_time),"updatedTime":optional(o.updated_time)})
}
fn records(records: Vec<IncomeRecord>) -> Value {
    Value::Array(records.into_iter().map(|o| {
        let category = match o.category.expect("income_category_present") {
            IncomeCategory::RealizedPnl=>"realized_pnl", IncomeCategory::Funding=>"funding", IncomeCategory::Commission=>"commission", IncomeCategory::Rebate=>"rebate", IncomeCategory::Transfer=>"transfer", IncomeCategory::Liquidation=>"liquidation", IncomeCategory::Other=>"other",
        };
        json!({"symbol":string(&o.symbol),"incomeType":string(&o.income_type),"income":number(o.income),"asset":string(&o.asset),"timestamp":number(o.timestamp),"tranId":string(&o.tran_id),"tradeId":string(o.trade_id.as_deref().expect("income_trade_id_present")),"category":string(category)})
    }).collect())
}
#[test]
fn actual_typescript_utility_corpus() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/utilities_oracle.cjs"
        ))
        .output()
        .expect("start one oracle per corpus");
    assert!(
        output.status.success(),
        "oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    for (key, expected) in [("scalars", 40), ("histories", 486), ("incomes", 230)] {
        assert_eq!(
            corpus[key].as_array().unwrap().len(),
            expected,
            "utility_corpus_census {key}"
        );
    }
    for key in ["histories", "incomes"] {
        assert!(
            corpus[key]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["label"] == "distinct-fields-supplement"),
            "utility_distinct_supplement {key}"
        );
    }
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => other,
    };
    assert_eq!(corpus["header"]["arch"], arch, "capture_cpu_architecture");
    assert!(
        corpus["header"]["nodeMajor"].as_u64().unwrap() > 0,
        "capture_node_major"
    );
    println!("capture_header {}", corpus["header"]);
    let mut compared = [0; 3];
    for c in corpus["scalars"].as_array().unwrap() {
        let v = c["raw"].as_str().map(decode);
        assert_eq!(
            capture(income::to_number(v.as_ref()), number),
            c["incomeNumber"],
            "income_to_number_native {}",
            c["label"]
        );
        assert_eq!(
            capture(order_history::to_number(v.as_ref()), number),
            c["historyNumber"],
            "history_to_number_native {}",
            c["label"]
        );
        assert_eq!(
            capture(order_history::to_optional_number(v.as_ref()), optional),
            c["optionalNumber"],
            "history_optional_native {}",
            c["label"]
        );
        compared[0] += 3;
    }
    for c in corpus["histories"].as_array().unwrap() {
        let v = decode(c["raw"].as_str().unwrap());
        assert_eq!(
            capture(
                order_history::map_paradex_historical_order(v.as_object().unwrap()),
                history
            ),
            c["result"],
            "history_native_fields {}",
            c["label"]
        );
        compared[1] += 1;
    }
    let counts: Vec<Option<usize>> =
        serde_json::from_str(include_str!("income_cardinality.json")).unwrap();
    assert_eq!(counts.len(), 230, "income_count_supplement_census");
    let source_total: usize = corpus["incomes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["result"]["value"].as_array().map(Vec::len))
        .sum();
    assert_eq!(source_total, 432, "income_source_total");
    let mut native_total = 0;
    for (i, c) in corpus["incomes"].as_array().unwrap().iter().enumerate() {
        let source_len = c["result"]["value"].as_array().map(Vec::len);
        assert_eq!(source_len, counts[i], "income_source_cardinality case {i}");
        let v = decode(c["raw"].as_str().unwrap());
        let mut calls = Vec::new();
        let result = income::map_paradex_fill_to_income_records(v.as_object().unwrap(), |market| {
            calls.push(market.to_owned());
            match c["mode"].as_str().unwrap() {
                "throw" => Err(JsCoercionError {
                    name: "TypeError",
                    message: "resolver failure",
                }),
                "missing" => Ok(None),
                "empty" => Ok(Some(String::new())),
                "resolved" => Ok(Some(format!("resolved:{market}"))),
                _ => unreachable!(),
            }
        });
        assert_eq!(
            result.as_ref().ok().map(Vec::len),
            source_len,
            "income_native_cardinality case {i}"
        );
        native_total += result.as_ref().map_or(0, Vec::len);
        assert_eq!(
            calls.len(),
            c["calls"].as_array().unwrap().len(),
            "income_native_trace_cardinality case {i}"
        );
        assert_eq!(
            capture(result, records),
            c["result"],
            "income_native_fields {}",
            c["label"]
        );
        assert_eq!(
            json!(calls),
            c["calls"],
            "income_resolver_trace {}",
            c["label"]
        );
        compared[2] += 1;
    }
    assert_eq!(native_total, 432, "income_native_total");
    assert_eq!(compared, [120, 486, 230], "utility_completed_comparisons");
    let lone = &corpus["surrogate"];
    assert_eq!(lone["codeUnit"], 0xd800);
    assert_eq!(lone["historyIdCodeUnit"], 0xd800);
    assert_eq!(lone["incomeIdCodeUnit"], 0xd800);
    assert!(
        js_deserialize_passthrough(&mut serde_json::Deserializer::from_str(
            lone["raw"].as_str().unwrap()
        ))
        .is_err(),
        "chosen_utf8_surrogate_refusal"
    );
    println!(
        "utility corpus: {} scalar, {} historical, {} income",
        corpus["scalars"].as_array().unwrap().len(),
        corpus["histories"].as_array().unwrap().len(),
        corpus["incomes"].as_array().unwrap().len()
    );
}
