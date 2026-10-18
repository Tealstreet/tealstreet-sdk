#[path = "support/runtime.rs"]
mod runtime;
use safe_cex_exchange::{
    income_paging::*,
    js_runtime::{
        globals::stringify_json,
        value::{Js, Thrown, error_object},
    },
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_types::{IncomeCategory, IncomeRecord};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    future::Future,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};

fn ready<T>(runtime: &runtime::Runtime, future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    for _ in 0..128 {
        if let Poll::Ready(value) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return value;
        }
        runtime.advance(0);
    }
    panic!("income paging unexpectedly pending");
}

fn number(value: &Value) -> f64 {
    f64::from_bits(
        u64::from_str_radix(
            value["numberBits"].as_str().expect("captured numeric bits"),
            16,
        )
        .unwrap(),
    )
}
fn bits(value: f64) -> u64 {
    if value.is_nan() {
        0x7ff8000000000000
    } else {
        value.to_bits()
    }
}
fn category(value: &Value) -> Option<IncomeCategory> {
    match value.as_str() {
        None => None,
        Some("realized_pnl") => Some(IncomeCategory::RealizedPnl),
        Some("funding") => Some(IncomeCategory::Funding),
        Some("commission") => Some(IncomeCategory::Commission),
        Some("rebate") => Some(IncomeCategory::Rebate),
        Some("transfer") => Some(IncomeCategory::Transfer),
        Some("liquidation") => Some(IncomeCategory::Liquidation),
        Some("other") => Some(IncomeCategory::Other),
        other => panic!("unexpected source category {other:?}"),
    }
}
fn record(value: &Value) -> IncomeRecord {
    IncomeRecord {
        symbol: value["symbol"].as_str().unwrap().into(),
        income_type: value["incomeType"].as_str().unwrap().into(),
        income: number(&value["income"]),
        asset: value["asset"].as_str().unwrap().into(),
        timestamp: number(&value["timestamp"]),
        tran_id: value["tranId"].as_str().unwrap().into(),
        trade_id: value["tradeId"].as_str().map(str::to_owned),
        category: category(&value["category"]),
    }
}
fn assert_record(actual: &IncomeRecord, expected: &Value, name: &str) {
    assert_eq!(
        actual.symbol,
        expected["symbol"].as_str().unwrap(),
        "PAGER native symbol {name}"
    );
    assert_eq!(
        actual.income_type,
        expected["incomeType"].as_str().unwrap(),
        "PAGER native incomeType {name}"
    );
    assert_eq!(
        actual.asset,
        expected["asset"].as_str().unwrap(),
        "PAGER native asset {name}"
    );
    assert_eq!(
        actual.tran_id,
        expected["tranId"].as_str().unwrap(),
        "PAGER native tranId {name}"
    );
    assert_eq!(
        actual.trade_id.as_deref(),
        expected["tradeId"].as_str(),
        "PAGER native tradeId {name}"
    );
    assert_eq!(
        actual.category,
        category(&expected["category"]),
        "PAGER native category {name}"
    );
    assert_eq!(
        bits(actual.timestamp),
        bits(number(&expected["timestamp"])),
        "PAGER native timestamp bits {name}"
    );
    assert_eq!(
        bits(actual.income),
        bits(number(&expected["income"])),
        "PAGER native income bits {name}"
    );
}

#[test]
#[ignore = "requires live Node and real Vitest; core CI includes ignored tests"]
fn live_shared_income_pager() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/income-paging-capture.cjs"
        ))
        .output()
        .expect("PAGER requires Node and real Vitest");
    assert!(
        output.status.success(),
        "PAGER actual-source assertions failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let line = stdout
        .lines()
        .find_map(|line| line.strip_prefix("INCOME_PAGING_CAPTURE="))
        .expect("PAGER capture must execute");
    let capture: Value = serde_json::from_str(line).unwrap();
    assert_eq!(capture["provenance"]["feederBytes"], 17508);
    assert_eq!(
        capture["provenance"]["feederSha256"],
        "676b632262daa2632a202dc89dd91d332cef8285160fea9328fce9f35833d248"
    );
    assert_eq!(
        capture["cases"].as_array().unwrap().len(),
        33,
        "PAGER corpus admission count"
    );
    let source_counts: Vec<usize> = capture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| case["result"]["records"].as_array().unwrap().len())
        .collect();
    assert_eq!(
        source_counts,
        [
            250, 250, 200, 400, 3, 3, 10, 0, 0, 2, 0, 30, 4, 0, 10, 19, 2, 1, 2, 0, 0, 1, 1, 1, 0,
            0, 0, 0, 0, 0, 0, 0, 4
        ],
        "PAGER source per-case output counts"
    );
    assert_eq!(
        source_counts.iter().sum::<usize>(),
        1193,
        "PAGER source total output count"
    );
    assert_eq!(
        capture["helpers"].as_array().unwrap().len(),
        130,
        "PAGER helper admission count"
    );
    let request_counts: Vec<usize> = capture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| case["pages"].as_array().unwrap().len())
        .collect();
    assert_eq!(
        request_counts,
        [
            4, 4, 7, 9, 3, 7, 3, 1, 1, 3, 0, 5, 3, 2, 2, 3, 3, 3, 3, 0, 0, 1, 3, 4, 0, 0, 0, 0, 0,
            1, 1, 1, 3
        ],
        "PAGER source per-case request counts"
    );
    assert_eq!(
        request_counts.iter().sum::<usize>(),
        80,
        "PAGER source total request count"
    );
    let mut native_total = 0;
    let mut native_comparisons = 0;
    let mut native_requests = 0;
    for case_index in 0..33 {
        let case = &capture["cases"][case_index];
        let name = case["name"].as_str().unwrap();
        let opts = &case["options"];
        let options = IncomeRangeOptions {
            start_time: number(&opts["startTime"]),
            end_time: number(&opts["endTime"]),
            chunk_ms: number(&opts["chunkMs"]),
            page_limit: number(&opts["pageLimit"]),
            chunk_delay_ms: opts.get("chunkDelayMs").map(number),
            max_pages_per_chunk: opts.get("maxPagesPerChunk").map(number),
        };
        let pages = case["pages"].as_array().unwrap().clone();
        let expected_pages = pages.len();
        let runtime = Rc::new(runtime::Runtime::default());
        let index = Rc::new(Cell::new(0));
        let trace = Rc::new(RefCell::new(Vec::new()));
        let index_fetch = index.clone();
        let fetch_trace = trace.clone();
        let name_owned = name.to_owned();
        let fetch = move |params: IncomePageParams| -> ExchangeFuture<'_, Vec<IncomeRecord>> {
            fetch_trace
                .borrow_mut()
                .push(serde_json::json!({"fetch":index_fetch.get()}));
            let page = pages
                .get(index_fetch.get())
                .expect("PAGER issued unexpected extra request");
            index_fetch.set(index_fetch.get() + 1);
            let expected = &page["params"];
            assert_eq!(
                bits(params.start_time),
                bits(number(&expected["startTime"])),
                "PAGER request startTime {name_owned}"
            );
            assert_eq!(
                bits(params.end_time),
                bits(number(&expected["endTime"])),
                "PAGER request endTime {name_owned}"
            );
            assert_eq!(
                bits(params.limit),
                bits(number(&expected["limit"])),
                "PAGER request limit {name_owned}"
            );
            let result = if page["error"] == true {
                Err(ExchangeError::new("source fetch failure"))
            } else {
                Ok(page["records"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(record)
                    .collect())
            };
            Box::pin(async move { result })
        };
        let delay_trace = trace.clone();
        let delay = move |ms: f64| -> ExchangeFuture<'_, ()> {
            delay_trace
                .borrow_mut()
                .push(serde_json::json!({"delay":{"numberBits":format!("{:016x}",bits(ms))}}));
            assert_eq!(
                bits(ms),
                bits(options.chunk_delay_ms.unwrap()),
                "PAGER native delay"
            );
            Box::pin(async { Ok(()) })
        };
        let result = ready(
            &runtime,
            paginate_income_range(runtime.clone(), Box::new(fetch), options, Box::new(delay)),
        )
        .unwrap();
        assert_eq!(
            index.get(),
            expected_pages,
            "PAGER consumed all source requests {name}"
        );
        assert_eq!(
            *trace.borrow(),
            *case["events"].as_array().unwrap(),
            "PAGER successful delay/request trace {name}"
        );
        native_total += result.records.len();
        native_requests += index.get();
        let expected = &case["result"];
        assert_eq!(
            result.truncated,
            expected["truncated"].as_bool().unwrap(),
            "PAGER incomplete flag {name}"
        );
        assert_eq!(
            result.requests.to_bits(),
            number(&expected["requests"]).to_bits(),
            "PAGER native requests {name}"
        );
        assert_eq!(
            result.exhausted_chunks.to_bits(),
            number(&expected["exhaustedChunks"]).to_bits(),
            "PAGER native exhaustedChunks {name}"
        );
        let expected_records = expected["records"].as_array().unwrap();
        assert_eq!(
            result.records.len(),
            expected_records.len(),
            "PAGER native record count {name}"
        );
        for (actual, expected) in result.records.iter().zip(expected_records) {
            assert_record(actual, expected, name);
            native_comparisons += 1;
        }
    }
    assert_eq!(native_total, 1193, "PAGER native total output count");
    assert_eq!(native_comparisons, 1193, "PAGER native record comparisons");
    assert_eq!(native_requests, 80, "PAGER native request comparisons");
    for index in 0..130 {
        let helper = &capture["helpers"][index];
        if helper["kind"] == "identify" {
            assert_eq!(
                identify(&record(&helper["row"])),
                helper["expected"].as_str().unwrap(),
                "PAGER identity helper {index}"
            );
        } else {
            let page: Vec<_> = helper["page"]
                .as_array()
                .unwrap()
                .iter()
                .map(record)
                .collect();
            let expected = match helper["expected"].as_str().unwrap() {
                "ascending" => PageDirection::Ascending,
                "descending" => PageDirection::Descending,
                "unknown" => PageDirection::Unknown,
                _ => unreachable!(),
            };
            assert_eq!(
                page_direction(&page, 0.0, 4.0),
                expected,
                "PAGER global direction {index}"
            );
        }
    }
    assert_eq!(capture["boundaries"]["alias"]["same"], true);
    assert_eq!(capture["boundaries"]["alias"]["consumerAsset"], "later");
    assert_eq!(capture["boundaries"]["nullIdentity"], "|||");
    let surrogate = capture["boundaries"]["surrogate"]["input"]
        .as_str()
        .unwrap();
    assert!(
        serde_json::from_str::<String>(surrogate).is_err(),
        "PAGER UTF8 representation refusal"
    );
    assert_eq!(
        capture["boundaries"]["surrogate"]["output"],
        "\"\\ud800|funding|type-B|17.25\""
    );
}

fn options() -> IncomeRangeOptions {
    IncomeRangeOptions {
        start_time: -0.0,
        end_time: 6.0,
        chunk_ms: 2.0,
        page_limit: 7.0,
        chunk_delay_ms: Some(-3.5),
        max_pages_per_chunk: Some(11.0),
    }
}
#[test]
fn income_pager_delay_failure_and_runtime_ownership() {
    let runtime = Rc::new(runtime::Runtime::default());
    let events = Rc::new(RefCell::new(vec![]));
    let fetch_events = events.clone();
    let fetch = move |params: IncomePageParams| -> ExchangeFuture<'_, Vec<IncomeRecord>> {
        assert!(
            fetch_events.borrow().is_empty(),
            "PAGER no fetch after rejected chunk delay"
        );
        fetch_events
            .borrow_mut()
            .push(format!("fetch:{}", params.start_time));
        Box::pin(async { Ok(vec![]) })
    };
    let source = timing_source_profiles();
    assert_eq!(source["delay"]["same"], true);
    let code = Js::object(vec![
        ("zero", Js::Number(-0.0)),
        ("nan", Js::Number(f64::from_bits(0x7ff8_0000_0000_0042))),
        ("text", Js::utf16(&[0xd800])),
    ]);
    let original = error_object("RangeError", "delay original");
    original.set("code", code.clone()).unwrap();
    let cause = Thrown::new(original.clone()).to_exchange_error();
    let expected = cause.clone();
    let mut cause = Some(cause);
    let delay_events = events.clone();
    let delay = move |ms: f64| -> ExchangeFuture<'_, ()> {
        assert_eq!(
            ms.to_bits(),
            (-3.5_f64).to_bits(),
            "PAGER delay is native f64"
        );
        delay_events.borrow_mut().push("delay".into());
        let cause = cause.take().expect("delay invoked once");
        Box::pin(async move { Err(cause) })
    };
    let error = ready(
        &runtime,
        paginate_income_range(runtime.clone(), Box::new(fetch), options(), Box::new(delay)),
    )
    .unwrap_err();
    assert_eq!(
        *events.borrow(),
        vec!["fetch:-0", "delay"],
        "PAGER delay failure precedes next request"
    );
    assert_eq!(error, expected, "PAGER complete delay metadata");
    assert_eq!(error.name, source["delay"]["name"]);
    assert_eq!(error.message, source["delay"]["message"]);
    let recovered = Thrown::from(error);
    assert!(
        recovered.value.strict_equals(&original),
        "PAGER original delay cause identity"
    );
    assert!(recovered.value.get("code").unwrap().strict_equals(&code));
    assert_eq!(
        code.get("zero").unwrap().as_f64().unwrap().to_bits(),
        (-0.0_f64).to_bits()
    );
    assert_eq!(
        code.get("nan").unwrap().as_f64().unwrap().to_bits(),
        0x7ff8_0000_0000_0042
    );
    assert_eq!(
        code.get("text").unwrap().as_str().unwrap().as_ref(),
        &[0xd800]
    );
    assert_eq!(
        String::from_utf16(&stringify_json(&recovered.value).unwrap().unwrap()).unwrap(),
        source["delay"]["json"]
    );
    struct Pending(Rc<Cell<bool>>);
    impl Drop for Pending {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    impl Future for Pending {
        type Output = Result<Vec<IncomeRecord>, ExchangeError>;
        fn poll(self: std::pin::Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
            Poll::Pending
        }
    }
    let dropped = Rc::new(Cell::new(false));
    let pending_dropped = dropped.clone();
    let fetch = move |_: IncomePageParams| -> ExchangeFuture<'_, Vec<IncomeRecord>> {
        Box::pin(Pending(pending_dropped.clone()))
    };
    let delay = |_: f64| -> ExchangeFuture<'_, ()> { panic!("PAGER pending fetch cannot delay") };
    let mut future = Box::pin(paginate_income_range(
        runtime.clone(),
        Box::new(fetch),
        options(),
        Box::new(delay),
    ));
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    drop(future);
    assert!(
        !dropped.get(),
        "PAGER waiter drop retains runtime-owned fetch"
    );
    runtime.tasks.borrow_mut().clear();
    assert!(
        dropped.get(),
        "PAGER runtime shutdown releases active fetch"
    );
}

#[test]
fn pager_starts_first_fetch_before_first_poll() {
    for (budget, start, expected) in [
        (1.0, 1.0, 1),
        (f64::NAN, 1.0, 1),
        (0.0, 1.0, 0),
        (-1.0, 1.0, 0),
        (1.0, 5.0, 0),
        (1.0, f64::NAN, 0),
    ] {
        let runtime = Rc::new(runtime::Runtime::default());
        let calls = Rc::new(Cell::new(0));
        let fetch_calls = calls.clone();
        let fetch = move |params: IncomePageParams| -> ExchangeFuture<'_, Vec<IncomeRecord>> {
            fetch_calls.set(fetch_calls.get() + 1);
            assert_eq!(params.start_time, 1.0, "PAGER native eager start");
            assert_eq!(params.end_time, 5.0, "PAGER native eager end");
            assert_eq!(params.limit, 7.0, "PAGER native eager limit");
            Box::pin(std::future::pending())
        };
        let delay = |_| -> ExchangeFuture<'_, ()> { panic!("no eager delay") };
        let future = paginate_income_range(
            runtime.clone(),
            Box::new(fetch),
            IncomeRangeOptions {
                start_time: start,
                end_time: 5.0,
                chunk_ms: 8.0,
                page_limit: 7.0,
                chunk_delay_ms: None,
                max_pages_per_chunk: Some(budget),
            },
            Box::new(delay),
        );
        assert_eq!(
            calls.get(),
            expected,
            "PAGER native invocation fetches before first poll budget={budget} start={start}"
        );
        drop(future);
        assert_eq!(
            calls.get(),
            expected,
            "PAGER native drop never repeats eager fetch"
        );
    }
}

fn timing_source_profiles() -> Value {
    let output = Command::new("node")
        .arg("-e")
        .arg(r#"
const fs = require('node:fs');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const source = fs.readFileSync(process.argv[1], 'utf8');
const exports = {};
const delayCause = new RangeError('delay original');
delayCause.code = {zero:-0,nan:NaN,text:'\ud800'};
vm.runInNewContext(ts.transpileModule(source, {compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText, {exports,setTimeout(){throw delayCause;}});
(async () => {
  const trace = [];
  let release;
  const held = new Promise(resolve => { release = resolve; });
  let calls = 0;
  exports.paginateIncomeRange(async () => {
    const call = ++calls;
    trace.push(`fetch:${call}`);
    if (call === 1) await held;
    trace.push(`done:${call}`);
    return [];
  }, {startTime:0,endTime:2,chunkMs:1,pageLimit:1});
  const before = trace.slice();
  release();
  for (let i=0;i<16;i++) await Promise.resolve();
  let rejected;
  try { await exports.paginateIncomeRange(async () => [], {startTime:0,endTime:2,chunkMs:1,pageLimit:1,chunkDelayMs:1}); }
  catch (error) { rejected = error; }
  if (rejected !== delayCause) throw new Error('source replaced delay cause');
  process.stdout.write(JSON.stringify({before,after:trace,delay:{same:rejected===delayCause,name:rejected.name,message:rejected.message,json:JSON.stringify(rejected)}}));
})().catch(error => { console.error(error); process.exitCode=1; });
"#)
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/../../packages/safe-cex/src/exchanges/income-paging.ts"))
        .output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
fn pager_continues_after_discarding_an_unpolled_waiter() {
    let source = timing_source_profiles();
    assert_eq!(source["before"], serde_json::json!(["fetch:1"]));
    assert_eq!(
        source["after"],
        serde_json::json!(["fetch:1", "done:1", "fetch:2", "done:2"])
    );
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&executor, async {
        let trace = Rc::new(RefCell::new(Vec::new()));
        let (release, held) = tokio::sync::oneshot::channel::<()>();
        let held = Rc::new(RefCell::new(Some(held)));
        let fetch = {
            let trace = trace.clone();
            let calls = Cell::new(0);
            move |_: IncomePageParams| -> ExchangeFuture<'static, Vec<IncomeRecord>> {
                calls.set(calls.get() + 1);
                let call = calls.get();
                trace.borrow_mut().push(format!("fetch:{call}"));
                let trace = trace.clone();
                let held = held.borrow_mut().take();
                Box::pin(async move {
                    if let Some(held) = held {
                        held.await.expect("source releases first page");
                    }
                    trace.borrow_mut().push(format!("done:{call}"));
                    Ok(vec![])
                })
            }
        };
        let delay = |_| -> ExchangeFuture<'static, ()> {
            panic!("no chunk delay");
        };
        let waiter = paginate_income_range(
            safe_cex_exchange::native::native_runtime(),
            Box::new(fetch),
            IncomeRangeOptions {
                start_time: 0.0,
                end_time: 2.0,
                chunk_ms: 1.0,
                page_limit: 1.0,
                chunk_delay_ms: None,
                max_pages_per_chunk: None,
            },
            Box::new(delay),
        );
        assert_eq!(serde_json::json!(*trace.borrow()), source["before"]);
        drop(waiter);
        release
            .send(())
            .expect("runtime retains first page receiver after waiter drop");
        for _ in 0..16 {
            tokio::task::yield_now().await;
        }
        assert_eq!(
            serde_json::json!(*trace.borrow()),
            source["after"],
            "PAGER_DETACHED runtime owns remaining pages"
        );
        assert_eq!(
            Rc::strong_count(&trace),
            1,
            "PAGER settled walk releases callback captures"
        );
    });
}
