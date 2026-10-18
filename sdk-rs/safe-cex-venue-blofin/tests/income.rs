#[path = "support/cardinality.rs"]
mod cardinality;
use safe_cex_num::numeric_ingress::{js_from_str, js_from_value, js_from_value_ref};
use safe_cex_types::*;
use safe_cex_venue_blofin::income::*;
use serde_json::{Value, json};
use std::process::Command;

fn bits(value: f64) -> Value {
    json!({"number": if value.is_nan() { "nan".into() } else { format!("{:016x}", value.to_bits()) }})
}
fn income(record: IncomeRecord) -> Value {
    // Native fields, not a shared-model serde roundtrip.
    json!({"symbol":record.symbol,"incomeType":record.income_type,"income":bits(record.income),
    "asset":record.asset,"timestamp":bits(record.timestamp),"tranId":record.tran_id,
    "tradeId":record.trade_id,"category":match record.category.unwrap() {
        IncomeCategory::RealizedPnl => "realized_pnl", IncomeCategory::Commission => "commission",
        IncomeCategory::Rebate => "rebate", other => panic!("unexpected category {other:?}"),
    }})
}
fn closed(record: ClosedPnlEntry) -> Value {
    let ClosedPnlEntryNotional::Variant1(notional) = record.notional else {
        panic!("native_notional_number")
    };
    let ClosedPnlEntryContracts::Variant1(contracts) = record.contracts else {
        panic!("native_contracts_number")
    };
    let ClosedPnlEntryEntryPrice::Variant1(entry) = record.entry_price else {
        panic!("native_entry_number")
    };
    let ClosedPnlEntryExitPrice::Variant1(exit) = record.exit_price else {
        panic!("native_exit_number")
    };
    let ClosedPnlEntryUnrealizedPnl::Variant1(pnl) = record.unrealized_pnl else {
        panic!("native_pnl_number")
    };
    let ClosedPnlEntryFees::Variant1(fees) = record.fees else {
        panic!("native_fees_number")
    };
    let ClosedPnlEntryLeverage::Variant0(leverage) = record.leverage else {
        panic!("native_leverage_number")
    };
    assert!(
        matches!(record.provenance, DataProvenance::Venue),
        "native_provenance"
    );
    json!({"provenance":"venue", "symbol":record.symbol, "notional":bits(notional),
        "contracts":bits(contracts), "entryPrice":bits(entry), "exitPrice":bits(exit),
        "unrealizedPnl":bits(pnl), "side":record.side, "fees":bits(fees), "time":bits(record.time),
        "closedTime":bits(record.closed_time), "account":record.account, "leverage":bits(leverage)})
}
#[test]
fn live_income_corpus() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/income_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual_typescript_income: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let pin = cardinality::Census::new("income", &corpus);
    let header = &corpus["header"];
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => other,
    };
    assert_eq!(header["architecture"], arch, "capture_architecture");
    assert!(
        header["nodeMajor"].as_u64().is_some_and(|v| v > 0),
        "capture_node_major"
    );
    eprintln!("income capture header: {header}");
    let cases = corpus["cases"].as_array().unwrap();
    for i in 0..pin.count("/cases") {
        let case = &corpus["cases"][i];
        for path in 0..3 {
            let raw = case["raw"].as_str();
            let input: Option<IncomeInput> = raw.map(|raw| match path {
                0 => js_from_str(raw).unwrap(),
                1 => js_from_value(serde_json::from_str(raw).unwrap()).unwrap(),
                _ => js_from_value_ref(&serde_json::from_str(raw).unwrap()).unwrap(),
            });
            let mut trace = Vec::new();
            let resolver = |id: &str| {
                trace.push(id.to_owned());
                Ok(case["resolved"].as_str().map(str::to_owned))
            };
            let result = match case["op"].as_str().unwrap() {
                "number" => to_number(input.as_ref())
                    .map(bits)
                    .map_err(safe_cex_exchange::runtime::ExchangeError::from),
                "fill" => map_blofin_fill_to_income_records(input.as_ref().unwrap(), resolver)
                    .map(|v| {
                        pin.native(i, "/result/ok", v.len());
                        Value::Array(v.into_iter().map(income).collect())
                    })
                    .map_err(safe_cex_exchange::runtime::ExchangeError::from),
                "closed" => {
                    map_blofin_closed_position(input.as_ref().unwrap(), resolver, "account:unique")
                        .map(|v| v.map_or(Value::Null, closed))
                        .map_err(safe_cex_exchange::runtime::ExchangeError::from)
                }
                _ => unreachable!(),
            };
            let actual = match result {
                Ok(v) => json!({"ok":v}),
                Err(e) => json!({"error":e.name,"message":e.message}),
            };
            let assertion = match case["op"].as_str().unwrap() {
                "number" => "income_to_number_native",
                "fill" => "income_fill_native",
                _ => "income_closed_native",
            };
            assert_eq!(
                actual, case["result"],
                "{assertion} case={i} path={path} raw={raw:?}"
            );
            assert_eq!(
                json!(trace),
                case["trace"],
                "income_callback_order case={i}"
            );
            pin.compared(i, path);
        }
        pin.hit("cases");
    }
    pin.finish("cases", pin.count("/cases"));
    pin.finish_comparisons(3);
    for index in 0..pin.count("/boundaries") {
        let raw = &corpus["boundaries"][index];
        let raw = raw.as_str().unwrap();
        assert!(
            js_from_str::<IncomeInput>(raw).is_err(),
            "income_ingress_boundary"
        );
        pin.hit("boundaries-raw-1");
    }
    pin.finish("boundaries-raw-1", pin.count("/boundaries"));
    eprintln!(
        "income corpus: {} cases, three ingress paths, one Node process",
        cases.len()
    );
}
