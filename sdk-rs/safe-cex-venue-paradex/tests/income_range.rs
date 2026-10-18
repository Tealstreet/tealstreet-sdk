use safe_cex_num::{
    js_number,
    numeric_ingress::{js_deserialize_passthrough, js_from_str},
};
use safe_cex_types::{IncomeCategory, IncomeHistoryOptions, IncomeRecord};
use safe_cex_venue_paradex::history_reads::fetch_income_history_range;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};

struct Pending<T> {
    settled: Rc<Cell<bool>>,
    value: Option<T>,
}
impl<T: Unpin> Future for Pending<T> {
    type Output = T;
    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<T> {
        if self.settled.get() {
            Poll::Ready(self.value.take().unwrap())
        } else {
            Poll::Pending
        }
    }
}
fn bits(v: f64) -> String {
    format!(
        "{:016x}",
        if v.is_nan() {
            0x7ff8000000000000
        } else {
            v.to_bits()
        }
    )
}
fn string(s: &str) -> Value {
    json!({"type":"string","value":s})
}
fn optional(s: Option<&str>) -> Value {
    s.map(string).unwrap_or(json!({"type":"undefined"}))
}
fn native(r: &IncomeRecord) -> Value {
    let category = r.category.as_ref().map(|c| match c {
        IncomeCategory::RealizedPnl => "realized_pnl",
        IncomeCategory::Funding => "funding",
        IncomeCategory::Commission => "commission",
        IncomeCategory::Rebate => "rebate",
        IncomeCategory::Transfer => "transfer",
        IncomeCategory::Liquidation => "liquidation",
        IncomeCategory::Other => "other",
    });
    json!({"symbol":string(&r.symbol),"incomeType":string(&r.income_type),"income":{"type":"number","bits":bits(r.income)},"asset":string(&r.asset),"timestamp":{"type":"number","bits":bits(r.timestamp)},"tranId":string(&r.tran_id),"tradeId":optional(r.trade_id.as_deref()),"category":optional(category)})
}
// The callback returns already-native IncomeRecords, not a wire decoder. Build
// that fixture through retained opaque ingress and shared numeric conversion;
// do not claim IncomeRecord has a JsNumberOutput registration it does not have.
fn fixture_records(raw: &str) -> Vec<IncomeRecord> {
    let value: Value =
        js_deserialize_passthrough(&mut serde_json::Deserializer::from_str(raw)).unwrap();
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let text = |key| r[key].as_str().unwrap().to_owned();
            assert!(
                r["income"].is_number() && r["timestamp"].is_number(),
                "income_range_fixture_numeric_shape"
            );
            IncomeRecord {
                symbol: text("symbol"),
                income_type: text("incomeType"),
                income: js_number(Some(&r["income"])).unwrap(),
                asset: text("asset"),
                timestamp: js_number(Some(&r["timestamp"])).unwrap(),
                tran_id: text("tranId"),
                trade_id: r.get("tradeId").map(|v| v.as_str().unwrap().to_owned()),
                category: r
                    .get("category")
                    .map(|v| serde_json::from_value(v.clone()).unwrap()),
            }
        })
        .collect()
}
#[test]
fn actual_typescript_income_range() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/income_range_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "income_range_source_execution {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => other,
    };
    assert_eq!(corpus["header"]["arch"], arch, "capture_architecture");
    assert!(
        corpus["header"]["nodeMajor"].as_u64().unwrap() > 0,
        "capture_node_major"
    );
    println!("capture_header {}", corpus["header"]);
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 54, "income_range_source_census");
    let source_total: usize = cases
        .iter()
        .filter_map(|c| c["outcome"]["records"].as_array().map(Vec::len))
        .sum();
    assert_eq!(source_total, 55, "income_range_source_total");
    let mut compared = 0;
    let mut native_total = 0;
    let mut cx = Context::from_waker(Waker::noop());
    for c in cases {
        let records = fixture_records(c["rawRows"].as_str().unwrap());
        let allocation = records.as_ptr();
        let record_pointers: Vec<_> = records
            .iter()
            .map(|r| (r.symbol.as_ptr(), r.tran_id.as_ptr()))
            .collect();
        let trace = RefCell::new(Vec::new());
        let settled = Rc::new(Cell::new(false));
        let error = Rc::new("original");
        let outcome = if c["mode"] == "async-error" {
            Err(error.clone())
        } else {
            Ok(records)
        };
        let start: f64 = js_from_str(c["startRaw"].as_str().unwrap()).unwrap();
        let end: f64 = js_from_str(c["endRaw"].as_str().unwrap()).unwrap();
        let mut future = Box::pin(fetch_income_history_range(
            start,
            end,
            c["kind"].as_str(),
            |opts: IncomeHistoryOptions| {
                assert!(
                    opts.symbol.is_none() && opts.limit.is_none(),
                    "income_range_unset_options"
                );
                trace.borrow_mut().push(json!({"start":bits(opts.start_time.expect("range_start_present")),"end":bits(opts.end_time.expect("range_end_present")),"kind":optional(opts.income_type.as_deref())}));
                if c["mode"] == "sync-error" {
                    Err(error.clone())
                } else {
                    Ok(Pending {
                        settled: settled.clone(),
                        value: Some(outcome),
                    })
                }
            },
        ));
        let first = future.as_mut().poll(&mut cx);
        assert_eq!(trace.borrow().len(), 1, "income_range_request_cardinality");
        assert_eq!(c["before"], 1, "income_range_source_request_cardinality");
        assert_eq!(
            trace.borrow()[0],
            json!({"start":c["trace"][0]["start"],"end":c["trace"][0]["end"],"kind":c["trace"][0]["kind"]}),
            "income_range_native_request {}",
            c["label"]
        );
        assert_eq!(
            c["trace"].as_array().unwrap().len(),
            1,
            "income_range_source_trace_total"
        );
        assert_eq!(
            c["trace"][0]["keys"],
            json!(["startTime", "endTime", "incomeType"]),
            "income_range_source_owned_undefined"
        );
        let result = match first {
            Poll::Ready(r) => {
                assert_eq!(c["mode"], "sync-error", "income_range_await_boundary");
                r
            }
            Poll::Pending => {
                settled.set(true);
                let Poll::Ready(r) = future.as_mut().poll(&mut cx) else {
                    panic!("income_range_settlement")
                };
                r
            }
        };
        match result {
            Ok(rows) => {
                let expected = c["outcome"]["records"].as_array().expect("source_success");
                assert_eq!(
                    expected.len() as u64,
                    c["expectedCount"].as_u64().unwrap(),
                    "income_range_source_per_case {}",
                    c["label"]
                );
                assert_eq!(
                    rows.len(),
                    expected.len(),
                    "income_range_native_cardinality {}",
                    c["label"]
                );
                let no_filter = c["kind"].as_str().is_none_or(str::is_empty);
                assert_eq!(
                    c["outcome"]["sameArray"], no_filter,
                    "income_range_source_array_identity"
                );
                if no_filter {
                    assert_eq!(
                        rows.as_ptr(),
                        allocation,
                        "income_range_unfiltered_allocation"
                    );
                }
                let mut record_compared = 0;
                for (i, row) in rows.iter().enumerate() {
                    assert_eq!(
                        native(row),
                        expected[i],
                        "income_range_native_record {} index {i}",
                        c["label"]
                    );
                    let original_index = c["outcome"]["positions"][i].as_u64().unwrap() as usize;
                    assert_eq!(
                        (row.symbol.as_ptr(), row.tran_id.as_ptr()),
                        record_pointers[original_index],
                        "income_range_moved_strings"
                    );
                    record_compared += 1;
                }
                assert_eq!(
                    record_compared,
                    c["expectedCount"].as_u64().unwrap() as usize,
                    "income_range_completed_records"
                );
                native_total += record_compared;
            }
            Err(e) => {
                assert_eq!(c["outcome"]["ok"], false, "income_range_expected_error");
                assert_eq!(
                    c["outcome"]["sameError"], true,
                    "income_range_source_error_identity"
                );
                assert!(Rc::ptr_eq(&e, &error), "income_range_native_error_identity");
            }
        }
        assert_eq!(trace.borrow().len(), 1, "income_range_no_pagination");
        compared += 1;
    }
    assert_eq!(native_total, 55, "income_range_native_total");
    assert_eq!(compared, 54, "income_range_completed_cases");
    assert_eq!(
        corpus["surrogate"]["length"], 1,
        "income_range_source_surrogate_match"
    );
    assert_eq!(corpus["surrogate"]["codeUnit"], 0xd800);
    typed_utf16::assert_projection(
        corpus["surrogate"]["raw"].as_str().unwrap(),
        &(js_from_str::<String>(corpus["surrogate"]["raw"].as_str().unwrap()))
            .expect("typed surrogate record is retained"),
    );
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;
