use safe_cex_exchange::runtime::{ExchangeError, LogSeverity};
use safe_cex_types::{
    ExecutionPositionSide, IncomeCategory, OrderSide, OrderStatus, OrderType, PositionSide,
};
use safe_cex_venue_binance::{private_event_value::*, request_weight::Failure, rest_history::*};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    pin::pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};

fn settle<F: Future>(f: F) -> F::Output {
    let w = Waker::noop().clone();
    let mut cx = Context::from_waker(&w);
    let mut f = pin!(f);
    for _ in 0..20 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
    }
    panic!("executor floor stopped poll20")
}
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/rest-history.json")).unwrap()
}
fn decoded(v: Value) -> EventValue {
    EventValue::from_json(v)
}
fn wire(v: &EventValue) -> Value {
    match v {
        EventValue::Undefined => json!({"$undefined":true}),
        EventValue::Null => Value::Null,
        EventValue::Bool(v) => json!(v),
        EventValue::Number(v) => {
            if v.is_nan() {
                json!({"$nan":true})
            } else if *v == f64::INFINITY {
                json!({"$infinity":true})
            } else if *v == f64::NEG_INFINITY {
                json!({"$negativeInfinity":true})
            } else if *v == 0.0 && v.is_sign_negative() {
                json!({"$negativeZero":true})
            } else {
                json!(v)
            }
        }
        EventValue::String(v) => json!(String::from_utf16(v).unwrap()),
        EventValue::Array(v) => array_values(v).expect("fixture array reads").iter().map(wire).collect(),
        EventValue::Object(_) => v
            .spread()
            .iter()
            .map(|(k, v)| (k.clone(), wire(v)))
            .collect(),
        _ => panic!(
            "authored REST-history fixture excludes non-data outputs; native referents are asserted in history_native"
        ),
    }
}
fn error(name: &str) -> Failure {
    let mut e = ExchangeError::new(if name == "empty-error" {
        ""
    } else {
        "history fixture"
    });
    e.name = "RangeError".into();
    e.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
        "history-code"
    )));
    e.response = Some(json!({"status":503,"data":{"msg":"venue detail"}}));
    Rc::new(e)
}
fn error_wire(e: &Failure, want: &Failure) -> Value {
    json!({"original":Rc::ptr_eq(e,want),"name":e.name,"message":e.message,"code":observed_error_code(&e.code).clone().unwrap_or(json!({"$undefined":true})),"response":e.response.clone().unwrap_or(json!({"$undefined":true}))})
}
fn original(e: &Failure, want: &Failure, l: &str) {
    assert!(Rc::ptr_eq(e, want), "{l}: original Error Rc");
    assert_eq!(e.name, "RangeError", "{l}");
    assert_eq!(e.message, want.message, "{l}");
    assert_eq!(
        observed_error_code(&e.code),
        Some(json!("history-code")),
        "{l}"
    );
    assert_eq!(
        e.response,
        Some(json!({"status":503,"data":{"msg":"venue detail"}})),
        "{l}"
    )
}
fn num(v: &EventValue, want: f64, l: &str) {
    let EventValue::Number(n) = v else {
        panic!("{l}: expected native Number, {v:?}")
    };
    if want.is_nan() {
        assert!(n.is_nan(), "{l}: expected native NaN, got{n}")
    } else {
        assert_eq!(*n, want, "{l}")
    }
}
fn text(v: &EventValue, want: &str, l: &str) {
    let EventValue::String(s) = v else {
        panic!("{l}: expected native String, {v:?}")
    };
    assert_eq!(s.as_ref(), want.encode_utf16().collect::<Vec<_>>(), "{l}")
}
fn boolean(v: &EventValue, want: bool, l: &str) {
    let EventValue::Bool(s) = v else {
        panic!("{l}: expected native Bool, {v:?}")
    };
    assert_eq!(*s, want, "{l}")
}
fn own(v: &EventValue, k: &str) -> bool {
    let EventValue::Object(o) = v else {
        panic!("native object")
    };
    o.props
        .borrow()
        .contains_key(safe_cex_exchange::js_runtime::value::utf16(k).as_ref())
}
fn rows(v: &EventValue, n: usize, l: &str) -> Vec<EventValue> {
    let EventValue::Array(a) = v else {
        panic!("{l}: native array")
    };
    assert_eq!(a.borrow().len(), n, "{l}: native row count");
    array_values(a).expect("fixture array reads")
}
fn native_enum(v: &EventValue, want: PrivateEnum, l: &str) {
    let EventValue::String(units) = v else {
        panic!("{l}: native categorical string, got{v:?}")
    };
    assert_eq!(
        units.as_ref(),
        want.wire()
            .as_str()
            .unwrap()
            .encode_utf16()
            .collect::<Vec<_>>(),
        "{l}"
    );
}
fn native_order(raw: EventValue) -> Result<EventValue, Failure> {
    safe_cex_venue_binance::order_history::map_binance_historical_order(
        &raw,
        &safe_cex_num::PrototypeRealm::current(),
    )
    .map(|value| value.unwrap_or(EventValue::Null))
}
#[derive(Clone)]
enum Notice {
    Log(String, LogSeverity),
    Emit(EventValue),
}
impl Notice {
    fn wire(&self) -> Value {
        match self {
            Self::Log(message, severity) => {
                json!({"method":"log","message":message,"severity":severity.as_str()})
            }
            Self::Emit(message) => json!({"method":"emit","message":wire(message)}),
        }
    }
}
struct GetReceipt {
    client: HistoryClient,
    endpoint: &'static str,
    params: EventValue,
}
struct State {
    name: String,
    error: Failure,
    trace: Vec<Value>,
    notices: Vec<Notice>,
    gets: Vec<GetReceipt>,
    categories: usize,
    maps: usize,
    payload: EventValue,
    range_records: EventValue,
    range_options: Option<EventValue>,
    page_args: Option<EventValue>,
    page_input: Option<EventValue>,
    method: Option<IncomePageMethod>,
    page_receiver: Option<usize>,
    iso_calls: usize,
}
type Shared = Rc<RefCell<State>>;
struct Bind(Shared);
impl HistoryBindings for Bind {
    fn public_only(&mut self) -> Result<bool, Failure> {
        let mut s = self.0.borrow_mut();
        s.trace.push(json!(["public_only"]));
        Ok(s.name == "public-only")
    }
    fn capture_get(&mut self, c: HistoryClient) -> Result<HistoryGet, Failure> {
        self.0.borrow_mut().trace.push(json!([
            "capture_get",
            if c == HistoryClient::Private {
                "private"
            } else {
                "public"
            }
        ]));
        let state = self.0.clone();
        Ok(Rc::new(move |ep, params| {
            let mut s = state.borrow_mut();
            s.trace.push(json!([
                "get",
                if c == HistoryClient::Private {
                    "private"
                } else {
                    "public"
                },
                ep,
                wire(&params)
            ]));
            s.gets.push(GetReceipt {
                client: c,
                endpoint: ep,
                params,
            });
            if s.name == "get-sync-fail" {
                return Err(s.error.clone());
            }
            drop(s);
            let state = state.clone();
            Ok(Box::pin(async move {
                let mut s = state.borrow_mut();
                s.trace.push(json!(["resolve"]));
                if matches!(s.name.as_str(), "get-reject" | "log-fail" | "emit-fail") {
                    Err(s.error.clone())
                } else {
                    Ok(s.payload.clone())
                }
            }))
        }))
    }
    fn capture_log(&mut self) -> Result<HistoryLog, Failure> {
        let mut s = self.0.borrow_mut();
        s.trace.push(json!(["capture_log"]));
        if s.name == "capture-log-fail" {
            return Err(s.error.clone());
        }
        drop(s);
        let state = self.0.clone();
        Ok(Rc::new(move |m, l| {
            let mut s = state.borrow_mut();
            s.trace.push(json!(["log"]));
            s.notices.push(Notice::Log(m, l));
            if s.name == "log-fail" {
                Err(s.error.clone())
            } else {
                Ok(())
            }
        }))
    }
    fn capture_emit(&mut self) -> Result<HistoryEmit, Failure> {
        self.0.borrow_mut().trace.push(json!(["capture_emit"]));
        let state = self.0.clone();
        Ok(Rc::new(move |m| {
            let mut s = state.borrow_mut();
            s.trace.push(json!(["emit"]));
            s.notices.push(Notice::Emit(m));
            if s.name == "emit-fail" {
                Err(s.error.clone())
            } else {
                Ok(())
            }
        }))
    }
    fn split(&mut self, from: EventValue, to: EventValue, max: f64) -> Result<EventValue, Failure> {
        self.0
            .borrow_mut()
            .trace
            .push(json!(["split", wire(&from), wire(&to), max]));
        if let (EventValue::Number(from), EventValue::Number(to)) = (from, to) {
            Ok(decoded(Value::Array(
                safe_cex_candle_cache::calendar::split_time_range(from, to, max),
            )))
        } else {
            Ok(decoded(json!([])))
        }
    }
    fn normalize_side(&mut self, raw: EventValue) -> Result<EventValue, Failure> {
        self.0
            .borrow_mut()
            .trace
            .push(json!(["normalize", wire(&raw)]));
        normalize_side(raw)
    }
    fn category(&mut self, raw: EventValue) -> Result<EventValue, Failure> {
        let mut s = self.0.borrow_mut();
        s.trace.push(json!(["category", wire(&raw)]));
        s.categories += 1;
        if s.name == "category-fail" {
            return Err(s.error.clone());
        }
        if s.name == "category-appends" && s.categories == 1 {
            let EventValue::Array(a) = &s.payload else {
                panic!()
            };
            let row = decoded(fixture()["income"][0].clone());
            row.write("tranId", string("appended"));
            EventValue::Array(a.clone()).push(row).unwrap();
        }
        drop(s);
        income_category(raw)
    }
    fn map_order(&mut self, raw: EventValue) -> Result<EventValue, Failure> {
        let mut s = self.0.borrow_mut();
        s.trace.push(json!(["map_order"]));
        s.maps += 1;
        if s.name == "map-fail" {
            return Err(s.error.clone());
        }
        drop(s);
        native_order(raw)
    }
    fn page_resolver(&mut self, method: IncomePageMethod) -> PageResolver {
        self.0.borrow_mut().method = Some(method);
        let state = self.0.clone();
        Rc::new(move || {
            let mut s = state.borrow_mut();
            let receiver = if s.name == "current-page" { 2 } else { 1 };
            s.trace.push(json!(["page_resolve", receiver]));
            s.page_receiver = Some(receiver);
            if s.name == "resolve-mutates" {
                s.page_input
                    .as_ref()
                    .unwrap()
                    .write("startTime", number(2.0));
            }
            drop(s);
            let state = state.clone();
            Ok(Rc::new(move |opts| {
                let mut s = state.borrow_mut();
                s.trace.push(json!(["page_call", receiver, wire(&opts)]));
                s.page_args = Some(opts);
                drop(s);
                let state = state.clone();
                Ok(Box::pin(async move {
                    let mut s = state.borrow_mut();
                    s.trace.push(json!(["page_settle"]));
                    if s.name == "page-reject" && method == IncomePageMethod::Rethrowing {
                        Err(s.error.clone())
                    } else {
                        Ok(decoded(json!([])))
                    }
                }))
            }))
        })
    }
    fn paginate(&mut self, fetch: PageCall, opts: EventValue) -> Result<HistoryFuture, Failure> {
        let mut s = self.0.borrow_mut();
        s.trace.push(json!(["paginate", wire(&opts)]));
        s.range_options = Some(opts);
        drop(s);
        let state = self.0.clone();
        Ok(Box::pin(async move {
            // The borrow is confined to this block: an explicit drop() leaves
            // the Ref in the coroutine at the await below, which is what
            // await_holding_refcell_ref is actually complaining about.
            let p = {
                let mut s = state.borrow_mut();
                s.trace.push(json!(["pager_settle"]));
                if s.name == "pager-reject" {
                    return Err(s.error.clone());
                }
                let p = if s.name == "null-page-arg" {
                    EventValue::Null
                } else {
                    record([
                        ("startTime", number(1.0)),
                        ("endTime", number(999.0)),
                        ("limit", number(7.0)),
                    ])
                };
                s.page_input = Some(p.clone());
                p
            };
            fetch(p)?.await?;
            let s = state.borrow();
            if s.name == "null-result" {
                Ok(EventValue::Null)
            } else {
                Ok(record([
                    ("records", s.range_records.clone()),
                    (
                        "truncated",
                        EventValue::Bool(!matches!(
                            s.name.as_str(),
                            "normal" | "current-page" | "resolve-mutates"
                        )),
                    ),
                    ("exhaustedChunks", number(2.0)),
                ]))
            }
        }))
    }
    fn iso(&mut self, time: EventValue) -> Result<String, Failure> {
        let mut s = self.0.borrow_mut();
        s.trace.push(json!(["iso", wire(&time)]));
        s.iso_calls += 1;
        if s.name == format!("iso{}-fail", s.iso_calls) {
            return Err(s.error.clone());
        }
        num(
            &time,
            if s.iso_calls == 1 { 0.0 } else { 1000.0 },
            "native date adapter input",
        );
        Ok(if s.iso_calls == 1 {
            "1970-01-01T00:00:00.000Z"
        } else {
            "1970-01-01T00:00:01.000Z"
        }
        .into())
    }
}
fn options(family: &str, name: &str) -> EventValue {
    let mut o = if matches!(family, "page" | "income") {
        decoded(
            json!({"symbol":"BTCUSDT","incomeType":"FUNDING_FEE","startTime":0,"endTime":1000,"limit":3.6}),
        )
    } else {
        decoded(json!({"symbol":"BTCUSDT","from":0,"to":1000,"orderId":"order-A","limit":3.6}))
    };
    if name == "missing-symbol" {
        let EventValue::Object(x) = &o else { panic!() };
        x.props
            .borrow_mut()
            .shift_remove(safe_cex_exchange::js_runtime::value::utf16("symbol").as_ref());
    }
    match name {
        "null-opts" => o = EventValue::Null,
        "undefined-opts" => o = EventValue::Undefined,
        "limit-null" => o.write("limit", EventValue::Null),
        "limit-zero" => o.write("limit", number(0.0)),
        "limit-string" => o.write("limit", string("2.5")),
        "zero-fields" => {
            o = decoded(json!({"symbol":"","incomeType":"","startTime":0,"endTime":0,"limit":0}))
        }
        "split-range" => {
            o.write("from", number(1.0));
            o.write("to", number(604800003.0));
        }
        "string-range" => {
            o.write("from", string("1"));
            o.write("to", number(1000.0));
        }
        _ => {}
    }
    o
}
fn state(family: &str, name: &str) -> Shared {
    let f = fixture();
    let mut payload = if matches!(family, "page" | "income") {
        decoded(f["income"].clone())
    } else if family == "orders" {
        decoded(f["orders"].clone())
    } else {
        let row = decoded(f["entry"].clone());
        if family == "public_trades" {
            row.write("isBuyerMaker", EventValue::Bool(true))
        }
        EventValue::array(vec![row])
    };
    match name {
        "non-array" => payload = decoded(json!({"code":-1000,"msg":"non array"})),
        "null-row" => payload = decoded(json!([null])),
        "null-id" | "noncall-id" => {
            let EventValue::Array(a) = &payload else {
                panic!()
            };
            a.borrow()[0].as_ref().unwrap().write(
                "id",
                if name == "null-id" {
                    EventValue::Null
                } else {
                    decoded(json!({"toString":0}))
                },
            );
        }
        _ => {}
    }
    Rc::new(RefCell::new(State {
        name: name.into(),
        error: error(name),
        trace: vec![],
        notices: vec![],
        gets: vec![],
        categories: 0,
        maps: 0,
        payload,
        range_records: decoded(json!([{"sentinel":12.3}])),
        range_options: None,
        page_args: None,
        page_input: None,
        method: None,
        page_receiver: None,
        iso_calls: 0,
    }))
}
fn oracle(family: &str) -> Value {
    let result = Command::new("node")
        .current_dir(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap(),
        )
        .args([
            "sdk-rs/safe-cex-venue-binance/scripts/rest-history.mjs",
            family,
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "source oracle{family}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}
fn equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equal(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len() && a.iter().all(|(k, a)| b.get(k).is_some_and(|b| equal(a, b)))
        }
        _ => a == b,
    }
}
fn compare(family: &str, actual: Vec<Value>, floor: usize) {
    let source = oracle(family);
    let source = source.as_array().unwrap();
    assert_eq!(source.len(), floor, "source authored floor");
    assert_eq!(actual.len(), floor, "native authored floor");
    for (i, (a, b)) in actual.iter().zip(source).enumerate() {
        assert!(
            equal(a, b),
            "source {family} case{i}: native {a} source {b}"
        )
    }
}
fn reader(family: &str) {
    let f = fixture();
    let key = if family == "page" { "income" } else { family };
    let floor = match family {
        "fills" | "page" | "income" => 12,
        "orders" | "public_trades" => 10,
        "range" => 12,
        _ => panic!(),
    };
    let cases = f["cases"][key].as_array().unwrap();
    assert_eq!(cases.len(), floor, "authored input floor");
    let mut actual = vec![];
    for (i, c) in cases.iter().enumerate() {
        let name = c["name"].as_str().unwrap();
        let label = format!("native {family} case{i} {name}");
        let s = state(family, name);
        let error = s.borrow().error.clone();
        let mut b = Bind(s.clone());
        let opts = options(family, name);
        let result = settle(async {
            match family {
                "fills" => historical_fills(opts, &mut b).await,
                "orders" => historical_orders(opts, &mut b).await,
                "page" => income_page(opts, &mut b).await,
                "income" => income_history(opts, &mut b).await,
                "range" => {
                    income_range(number(0.0), number(1000.0), EventValue::Undefined, &mut b).await
                }
                "public_trades" => {
                    public_trades(string("BTCUSDT"), opts.read("limit")?, &mut b).await
                }
                _ => panic!(),
            }
        });
        let state = s.borrow();
        let (get_count, row_count, notice_count, map_count, cat_count, failed, original_error) =
            match family {
                "fills" => match name {
                    "public-only" | "missing-symbol" | "string-range" => {
                        (0, 0, 0, 0, 0, false, false)
                    }
                    "null-opts" => (0, 0, 0, 0, 0, true, false),
                    "split-range" => (2, 2, 0, 0, 0, false, false),
                    "get-reject" | "get-sync-fail" | "null-row" => (1, 0, 1, 0, 0, false, false),
                    "log-fail" => (1, 0, 1, 0, 0, true, true),
                    "non-array" => (1, 0, 0, 0, 0, false, false),
                    _ => (1, 1, 0, 0, 0, false, false),
                },
                "orders" => match name {
                    "public-only" | "missing-symbol" | "undefined-opts" => {
                        (0, 0, 0, 0, 0, false, false)
                    }
                    "get-reject" => (1, 0, 1, 0, 0, false, false),
                    "non-array" => (1, 0, 0, 0, 0, false, false),
                    "null-row" | "map-fail" => (1, 0, 1, 1, 0, false, false),
                    "emit-fail" => (1, 0, 1, 0, 0, true, true),
                    _ => (1, 2, 0, 3, 0, false, false),
                },
                "page" | "income" => {
                    let page = family == "page";
                    match name {
                        "public-only" => (0, 0, 0, 0, 0, false, false),
                        "get-reject" | "get-sync-fail" | "log-fail" => (
                            1,
                            0,
                            usize::from(!page),
                            0,
                            0,
                            page || name == "log-fail",
                            true,
                        ),
                        "null-row" => (1, 0, usize::from(!page), 0, 0, page, false),
                        "category-fail" => (1, 0, usize::from(!page), 0, 1, page, true),
                        "non-array" => (1, 0, 0, 0, 0, false, false),
                        _ => (1, 2, 0, 0, 2, false, false),
                    }
                }
                "public_trades" => match name {
                    "get-reject" | "get-sync-fail" => (1, 0, 0, 0, 0, true, true),
                    "null-row" | "null-id" | "noncall-id" => (1, 0, 0, 0, 0, true, false),
                    "non-array" => (1, 0, 0, 0, 0, false, false),
                    _ => (1, 1, 0, 0, 0, false, false),
                },
                "range" => {
                    let bad = matches!(
                        name,
                        "pager-reject"
                            | "page-reject"
                            | "null-page-arg"
                            | "null-result"
                            | "capture-log-fail"
                            | "iso1-fail"
                            | "iso2-fail"
                            | "log-fail"
                    );
                    (
                        0,
                        usize::from(!bad),
                        usize::from(matches!(name, "truncated" | "log-fail")),
                        0,
                        0,
                        bad,
                        !matches!(name, "null-page-arg" | "null-result"),
                    )
                }
                _ => panic!(),
            };
        assert_eq!(state.gets.len(), get_count, "{label}: native GET count");
        assert_eq!(state.maps, map_count, "{label}: native map calls");
        assert_eq!(
            state.categories, cat_count,
            "{label}: native category calls"
        );
        assert_eq!(
            state.notices.len(),
            notice_count,
            "{label}: native notice count"
        );
        assert_eq!(result.is_err(), failed, "{label}: native outcome");
        if let Err(e) = &result {
            if original_error {
                original(e, &error, &label)
            } else {
                assert!(!Rc::ptr_eq(e, &error), "{label}: generated Error");
                assert_eq!(e.name, "TypeError", "{label}");
                assert!(
                    e.code.is_none() && e.response.is_none(),
                    "{label}: generated absent metadata"
                );
                let message = match (family, name) {
                    ("fills", "null-opts") => {
                        "Cannot destructure property 'symbol' of 'opts' as it is null."
                    }
                    ("page", "null-row") => "Cannot read properties of null (reading 'symbol')",
                    ("range", "null-page-arg") => {
                        "Cannot destructure property 'startTime' of 'object null' as it is null."
                    }
                    ("range", "null-result") => {
                        "Cannot destructure property 'records' of '(intermediate value)' as it is null."
                    }
                    ("public_trades", "null-row") => {
                        "Cannot read properties of null (reading 'id')"
                    }
                    ("public_trades", "null-id") => {
                        "Cannot read properties of null (reading 'toString')"
                    }
                    ("public_trades", "noncall-id") => "trade.id.toString is not a function",
                    _ => panic!("unauthored generated error"),
                };
                assert_eq!(e.message, message, "{label}: native generated message");
            }
        } else {
            let values = rows(result.as_ref().unwrap(), row_count, &label);
            for (j, row) in values.iter().enumerate() {
                let l = format!("{label} row{j}");
                match family {
                    "fills" => {
                        text(&row.read("id").unwrap(), "9007199254740993", &l);
                        text(&row.read("orderId").unwrap(), "123", &l);
                        num(
                            &row.read("timestamp").unwrap(),
                            1700000000.0,
                            &format!("{l}: native timestamp"),
                        );
                        num(&row.read("price").unwrap(), 12.3, &l);
                        num(&row.read("qty").unwrap(), 3.6, &l);
                        num(&row.read("commission").unwrap(), 0.07, &l);
                        num(&row.read("realizedPnl").unwrap(), -0.25, &l);
                        native_enum(
                            &row.read("side").unwrap(),
                            PrivateEnum::Side(OrderSide::Sell),
                            &l,
                        );
                        native_enum(
                            &row.read("positionSide").unwrap(),
                            PrivateEnum::Execution(&ExecutionPositionSide::Side(
                                PositionSide::Long,
                            )),
                            &l,
                        );
                        boolean(&row.read("maker").unwrap(), true, &l);
                        assert!(
                            own(row, "id") && own(row, "maker"),
                            "{l}: native retained fields"
                        );
                    }
                    "orders" => {
                        text(
                            &row.read("id").unwrap(),
                            if j == 0 { "200" } else { "100" },
                            &format!("{l}: native ID"),
                        );
                        num(
                            &row.read("createdTime").unwrap(),
                            if j == 0 { 2000.0 } else { 1000.0 },
                            &l,
                        );
                        num(
                            &row.read("price").unwrap(),
                            if j == 0 { 12.4 } else { 12.3 },
                            &l,
                        );
                        num(
                            &row.read("amount").unwrap(),
                            if j == 0 { 4.2 } else { 3.6 },
                            &l,
                        );
                        num(
                            &row.read("filled").unwrap(),
                            if j == 0 { 0.3 } else { 0.2 },
                            &l,
                        );
                        native_enum(
                            &row.read("side").unwrap(),
                            PrivateEnum::Side(if j == 0 {
                                OrderSide::Sell
                            } else {
                                OrderSide::Buy
                            }),
                            &l,
                        );
                        native_enum(
                            &row.read("type").unwrap(),
                            PrivateEnum::Type(if j == 0 {
                                OrderType::StopLoss
                            } else {
                                OrderType::Limit
                            }),
                            &l,
                        );
                        native_enum(
                            &row.read("status").unwrap(),
                            PrivateEnum::Status(if j == 0 {
                                OrderStatus::Open
                            } else {
                                OrderStatus::Closed
                            }),
                            &l,
                        );
                        assert!(
                            matches!(row.read("updatedTime").unwrap(), EventValue::Undefined)
                                && own(row, "updatedTime"),
                            "{l}: native own undefined"
                        );
                    }
                    "page" | "income" => {
                        num(
                            &row.read("income").unwrap(),
                            if j == 0 { 1.25 } else { -0.25 },
                            &format!("{l}: native income"),
                        );
                        if j == 0 {
                            num(&row.read("timestamp").unwrap(), 1000.0, &l);
                            text(&row.read("tranId").unwrap(), "txn-A", &l);
                        } else {
                            text(&row.read("timestamp").unwrap(), "2000", &l);
                            num(&row.read("tranId").unwrap(), 2.0, &l);
                            assert!(
                                matches!(row.read("tradeId").unwrap(), EventValue::Null),
                                "{l}: native explicit null tradeId"
                            );
                        }
                        native_enum(
                            &row.read("category").unwrap(),
                            PrivateEnum::Income(if j == 0 {
                                &IncomeCategory::Funding
                            } else {
                                &IncomeCategory::RealizedPnl
                            }),
                            &l,
                        );
                    }
                    "public_trades" => {
                        text(&row.read("id").unwrap(), "9007199254740993", &l);
                        num(&row.read("price").unwrap(), 12.3, &l);
                        num(&row.read("qty").unwrap(), 3.6, &l);
                        num(&row.read("quoteQty").unwrap(), 44.28, &l);
                        num(
                            &row.read("timestamp").unwrap(),
                            1700000000123.0,
                            &format!("{l}: native timestamp"),
                        );
                        native_enum(
                            &row.read("side").unwrap(),
                            PrivateEnum::Side(OrderSide::Sell),
                            &format!("{l}: native side"),
                        );
                    }
                    "range" => {
                        assert!(
                            result.as_ref().unwrap().same(&state.range_records),
                            "{l}: native original pager records"
                        );
                        num(&row.read("sentinel").unwrap(), 12.3, &l);
                    }
                    _ => panic!(),
                }
            }
        }
        for receipt in &state.gets {
            let l = format!("{label}: native request");
            assert_eq!(
                receipt.client,
                if family == "public_trades" {
                    HistoryClient::Public
                } else {
                    HistoryClient::Private
                },
                "{l}"
            );
            assert_eq!(
                receipt.endpoint,
                match family {
                    "fills" => "/fapi/v1/userTrades",
                    "orders" => "/fapi/v1/allOrders",
                    "page" | "income" => "/fapi/v1/income",
                    "public_trades" => "/fapi/v1/trades",
                    _ => panic!(),
                },
                "{l}"
            );
            let limit = if name == "limit-null" && family == "fills" {
                0.0
            } else if name == "limit-string" {
                2.5
            } else if matches!(
                name,
                "undefined-opts" | "zero-fields" | "limit-zero" | "limit-null"
            ) {
                1000.0
            } else {
                3.6
            };
            num(
                &receipt.params.read("limit").unwrap(),
                limit,
                &format!("{l}: native limit"),
            );
            if matches!(family, "page" | "income") {
                assert!(
                    !own(&receipt.params, "startTime"),
                    "{l}: native truthy start-time filter"
                );
                assert_eq!(
                    own(&receipt.params, "endTime"),
                    !matches!(name, "undefined-opts" | "zero-fields"),
                    "{l}"
                );
            }
            if family == "fills" && name == "split-range" {
                let index = state
                    .gets
                    .iter()
                    .position(|r| std::ptr::eq(r, receipt))
                    .unwrap();
                num(
                    &receipt.params.read("startTime").unwrap(),
                    if index == 0 { 1.0 } else { 604800002.0 },
                    &l,
                );
                num(
                    &receipt.params.read("endTime").unwrap(),
                    if index == 0 { 604800001.0 } else { 604800003.0 },
                    &l,
                );
            }
        }
        for notice in &state.notices {
            match notice {
                Notice::Log(message, severity) => {
                    assert_eq!(
                        *severity,
                        if family == "range" {
                            LogSeverity::Warning
                        } else {
                            LogSeverity::Error
                        },
                        "{label}: native severity"
                    );
                    let expected = if family == "range" {
                        "Income history for 1970-01-01T00:00:00.000Z..1970-01-01T00:00:01.000Z is INCOMPLETE — 2 chunk(s) could not be walked to completion. Any total derived from it is a lower bound.".into()
                    } else {
                        format!(
                            "Error fetching {}: {}",
                            if family == "fills" {
                                "executions"
                            } else {
                                "income history"
                            },
                            if name == "null-row" {
                                if family == "fills" {
                                    "Cannot read properties of null (reading 'id')"
                                } else {
                                    "Cannot read properties of null (reading 'symbol')"
                                }
                            } else {
                                "history fixture"
                            }
                        )
                    };
                    assert_eq!(*message, expected, "{label}: native notice text");
                }
                Notice::Emit(message) => text(
                    message,
                    if name == "null-row" {
                        "Cannot read properties of null (reading 'orderId')"
                    } else {
                        "venue detail"
                    },
                    &format!("{label}: native error emission"),
                ),
            }
        }
        if name == "category-appends" {
            let EventValue::Array(a) = &state.payload else {
                panic!()
            };
            assert_eq!(a.borrow().len(), 3, "{label}: native appended input floor");
        }
        if family == "range" {
            assert_eq!(
                state.method,
                Some(IncomePageMethod::Rethrowing),
                "{label}: native page method"
            );
            let options = state.range_options.as_ref().unwrap();
            for (k, v) in [
                ("startTime", 0.0),
                ("endTime", 1000.0),
                ("chunkMs", 604800000.0),
                ("pageLimit", 1000.0),
                ("chunkDelayMs", 2000.0),
            ] {
                num(
                    &options.get(k).unwrap(),
                    v,
                    &format!("{label}: native pager {k}"),
                );
            }
            let page = !matches!(name, "pager-reject" | "null-page-arg");
            assert_eq!(
                state.page_args.is_some(),
                page,
                "{label}: native page call count"
            );
            assert_eq!(
                state.page_receiver,
                if page {
                    Some(if name == "current-page" { 2 } else { 1 })
                } else {
                    None
                },
                "{label}: native current page receiver"
            );
            if let Some(args) = &state.page_args {
                num(
                    &args.read("startTime").unwrap(),
                    1.0,
                    &format!("{label}: native captured page start"),
                );
                num(&args.read("endTime").unwrap(), 999.0, &label);
                num(&args.read("limit").unwrap(), 7.0, &label);
                assert!(
                    matches!(args.read("incomeType").unwrap(), EventValue::Undefined)
                        && own(args, "incomeType"),
                    "{label}: native explicit undefined income type"
                );
            }
            assert_eq!(
                state.iso_calls,
                if name == "iso1-fail" {
                    1
                } else if matches!(name, "truncated" | "iso2-fail" | "log-fail") {
                    2
                } else {
                    0
                },
                "{label}: native ISO count"
            );
        }
        actual.push(json!({"name":name,"trace":state.trace,"notices":state.notices.iter().map(Notice::wire).collect::<Vec<_>>(),"gets":state.gets.len(),"categories":state.categories,"maps":state.maps,"value":result.as_ref().map(wire).unwrap_or(json!({"$undefined":true})),"rangeOptions":state.range_options.as_ref().map(wire).unwrap_or(json!({"$undefined":true})),"pageArgs":state.page_args.as_ref().map(wire).unwrap_or(json!({"$undefined":true})),"rangeRecordsRef":if family=="range"{result.as_ref().ok().map(|v|v.same(&state.range_records))}else{None},"failure":result.as_ref().err().map(|e|error_wire(e,&error))}));
    }
    compare(family, actual, floor);
}
#[test]
#[ignore = "source oracle"]
fn fills() {
    reader("fills")
}
#[test]
#[ignore = "source oracle"]
fn orders() {
    reader("orders")
}
#[test]
#[ignore = "source oracle"]
fn income_page_target() {
    reader("page")
}
#[test]
#[ignore = "source oracle"]
fn income() {
    reader("income")
}
#[test]
#[ignore = "source oracle"]
fn range() {
    reader("range")
}
#[test]
#[ignore = "source oracle"]
fn public_trades_target() {
    reader("public_trades")
}
#[test]
#[ignore = "source oracle"]
fn mapper() {
    let f = fixture();
    let cases = f["cases"]["mapper"].as_array().unwrap();
    assert_eq!(cases.len(), 8);
    let mut actual = vec![];
    for (i, c) in cases.iter().enumerate() {
        let name = c["name"].as_str().unwrap();
        let label = format!("native mapper case{i} {name}");
        let entry = decoded(f["entry"].clone());
        match name {
            "numeric-prefix" => entry.write("price", string("12.3tail")),
            "nullable-ids" => {
                entry.write("id", EventValue::Null);
                entry.write("orderId", EventValue::Null);
                entry.write("quoteQty", EventValue::Null);
            }
            "missing" => {
                let EventValue::Object(o) = &entry else {
                    panic!()
                };
                for key in [
                    "id",
                    "orderId",
                    "quoteQty",
                    "realizedPnl",
                    "commission",
                    "time",
                    "positionSide",
                    "side",
                ] {
                    o.props
                        .borrow_mut()
                        .shift_remove(safe_cex_exchange::js_runtime::value::utf16(key).as_ref());
                }
            }
            "unknown-labels" => {
                entry.write("positionSide", string("MANUAL"));
                entry.write("side", string("CUSTOM"));
            }
            _ => {}
        }
        let err = error(name);
        let mut trace = vec![];
        let result = map_user_trade(
            if name == "null-entry" {
                EventValue::Null
            } else {
                entry.clone()
            },
            |value| {
                trace.push(json!(["lower", value]));
                if name == "locale-fail" {
                    return Err(err.clone());
                }
                if name == "locale-mutates" {
                    entry.write("commission", string("0.2"));
                    entry.write("time", number(2000.0));
                }
                assert!(value.is_ascii(), "locale adapter floor is ASCII labels");
                Ok(value.to_ascii_lowercase())
            },
        );
        assert_eq!(
            result.is_err(),
            matches!(name, "locale-fail" | "null-entry"),
            "{label}: native outcome"
        );
        assert_eq!(
            trace.len(),
            if name == "null-entry" {
                0
            } else if name == "locale-fail" {
                1
            } else {
                2
            },
            "{label}: native locale calls"
        );
        if let Err(e) = &result {
            if name == "locale-fail" {
                original(e, &err, &label)
            } else {
                assert_eq!(e.name, "TypeError");
                assert_eq!(e.message, "Cannot read properties of null (reading 'id')");
                assert!(e.code.is_none() && e.response.is_none());
            }
        } else {
            let v = result.as_ref().unwrap();
            let EventValue::Object(o) = v else { panic!() };
            assert_eq!(
                o.props.borrow().len(),
                14,
                "{label}: native own field count"
            );
            num(
                &v.read("price").unwrap(),
                if name == "numeric-prefix" {
                    f64::NAN
                } else {
                    12.3
                },
                &format!("{label}: native price"),
            );
            num(&v.read("qty").unwrap(), 3.6, &label);
            for key in ["id", "orderId", "quoteQty"] {
                if matches!(name, "nullable-ids" | "missing") {
                    assert!(
                        own(v, key) && matches!(v.get(key).unwrap(), EventValue::Undefined),
                        "{label}: native own undefined {key}"
                    )
                } else if key == "quoteQty" {
                    num(&v.get(key).unwrap(), 44.28, &label)
                } else {
                    text(
                        &v.get(key).unwrap(),
                        if key == "id" {
                            "9007199254740993"
                        } else {
                            "123"
                        },
                        &label,
                    )
                }
            }
            num(
                &v.read("realizedPnl").unwrap(),
                if name == "missing" { f64::NAN } else { -0.25 },
                &label,
            );
            num(
                &v.read("commission").unwrap(),
                if name == "missing" {
                    f64::NAN
                } else if name == "locale-mutates" {
                    0.2
                } else {
                    0.07
                },
                &label,
            );
            num(
                &v.read("time").unwrap(),
                if name == "missing" {
                    f64::NAN
                } else if name == "locale-mutates" {
                    2000.0
                } else {
                    1700000000123.0
                },
                &label,
            );
            boolean(&v.read("buyer").unwrap(), false, &label);
            boolean(&v.read("maker").unwrap(), true, &label);
            if name == "unknown-labels" {
                text(&v.read("positionSide").unwrap(), "manual", &label);
                text(&v.read("side").unwrap(), "custom", &label)
            } else {
                native_enum(
                    &v.read("positionSide").unwrap(),
                    PrivateEnum::Execution(if name == "missing" {
                        &ExecutionPositionSide::Both(safe_cex_types::BothPositionSide::Both)
                    } else {
                        &ExecutionPositionSide::Side(PositionSide::Long)
                    }),
                    &label,
                );
                if name == "missing" {
                    text(&v.read("side").unwrap(), "", &label)
                } else {
                    native_enum(
                        &v.read("side").unwrap(),
                        PrivateEnum::Side(OrderSide::Sell),
                        &label,
                    )
                }
            }
        }
        actual.push(json!({"name":name,"trace":trace,"value":result.as_ref().map(wire).unwrap_or(json!({"$undefined":true})),"failure":result.as_ref().err().map(|e|error_wire(e,&err))}));
    }
    compare("mapper", actual, 8);
}
struct SeedState {
    name: String,
    error: Failure,
    trace: Vec<Value>,
    notices: Vec<Notice>,
    candles: EventValue,
    callbacks: Vec<EventValue>,
    callback_refs: Vec<bool>,
    cleanups: usize,
    params: Option<EventValue>,
    public_opts: Option<EventValue>,
    callback_identity: bool,
    owner_reads: usize,
}
type SeedShared = Rc<RefCell<SeedState>>;
struct SeedBind {
    state: SeedShared,
    opts: EventValue,
    callback: CandleCallback,
    jobs: Vec<SeedFuture>,
}
fn unsubscribe(s: SeedShared) -> CandleCleanup {
    Rc::new(move || {
        let mut s = s.borrow_mut();
        s.trace.push(json!(["unsubscribe"]));
        s.cleanups += 1;
        if s.name == "unsubscribe-fail" {
            Err(s.error.clone())
        } else {
            Ok(())
        }
    })
}
impl CandleBindings for SeedBind {
    fn external(&mut self) -> Result<bool, Failure> {
        let mut s = self.state.borrow_mut();
        s.trace.push(json!(["external"]));
        Ok(s.name == "external")
    }
    fn capture_external(&mut self) -> Result<CandleListen, Failure> {
        self.state.borrow_mut().trace.push(json!(["external"]));
        let state = self.state.clone();
        let expected = self.callback.clone();
        let opts = self.opts.clone();
        Ok(Rc::new(move |o, c| {
            let mut s = state.borrow_mut();
            s.trace.push(json!(["external_listen"]));
            assert!(o.same(&opts), "native external original options");
            s.public_opts = Some(o);
            s.callback_identity = Rc::ptr_eq(&c, &expected);
            drop(s);
            Ok(unsubscribe(state.clone()))
        }))
    }
    fn capture_public(&mut self) -> Result<CandleListen, Failure> {
        self.state
            .borrow_mut()
            .trace
            .push(json!(["capture_public"]));
        let state = self.state.clone();
        let expected = self.callback.clone();
        let opts = self.opts.clone();
        Ok(Rc::new(move |o, c| {
            let mut s = state.borrow_mut();
            s.trace.push(json!(["public_listen"]));
            assert!(o.same(&opts), "native public original options");
            s.public_opts = Some(o);
            s.callback_identity = Rc::ptr_eq(&c, &expected);
            drop(s);
            Ok(unsubscribe(state.clone()))
        }))
    }
    fn capture_fetch(&mut self) -> Result<CandleFetch, Failure> {
        self.state.borrow_mut().trace.push(json!(["capture_fetch"]));
        let state = self.state.clone();
        Ok(Rc::new(move |o| {
            let mut s = state.borrow_mut();
            s.trace.push(json!(["seed_fetch", wire(&o)]));
            s.params = Some(o);
            if s.name == "fetch-sync-fail" {
                return Err(s.error.clone());
            }
            drop(s);
            let state = state.clone();
            Ok(Box::pin(async move {
                let mut s = state.borrow_mut();
                s.trace.push(json!(["seed_settle"]));
                if matches!(
                    s.name.as_str(),
                    "fetch-reject"
                        | "empty-error"
                        | "capture-log-fail"
                        | "log-fail"
                        | "cleanup-reject"
                ) {
                    Err(s.error.clone())
                } else {
                    Ok(s.candles.clone())
                }
            }))
        }))
    }
    fn seed_owner(&mut self) -> SeedOwner {
        let state = self.state.clone();
        let owner = Rc::new(move || {
            let mut s = state.borrow_mut();
            s.trace.push(json!(["owner_disposed"]));
            s.owner_reads += 1;
            Ok(s.name == "owner-disposed")
        });
        let state = self.state.clone();
        let log = Rc::new(move || {
            let mut s = state.borrow_mut();
            s.trace.push(json!(["capture_log"]));
            if s.name == "capture-log-fail" {
                return Err(s.error.clone());
            }
            drop(s);
            let state = state.clone();
            Ok(Rc::new(move |m, l| {
                let mut s = state.borrow_mut();
                s.trace.push(json!(["log"]));
                s.notices.push(Notice::Log(m, l));
                if s.name == "log-fail" {
                    Err(s.error.clone())
                } else {
                    Ok(())
                }
            }) as HistoryLog)
        });
        SeedOwner {
            disposed: owner,
            capture_log: log,
        }
    }
    fn launch(&mut self, job: SeedFuture) -> Result<(), Failure> {
        self.jobs.push(job);
        Ok(())
    }
}
#[test]
#[ignore = "source oracle"]
fn seed() {
    let f = fixture();
    let cases = f["cases"]["seed"].as_array().unwrap();
    assert_eq!(cases.len(), 15, "authored seed input floor");
    let mut actual = vec![];
    for (i, c) in cases.iter().enumerate() {
        let name = c["name"].as_str().unwrap();
        let label = format!("native seed case{i} {name}");
        let opts =
            decoded(json!({"symbol":"BTCUSDT","interval":"1m","limit":5,"nested":{"value":3.6}}));
        let nested = opts.read("nested").unwrap();
        let candles = if name == "empty" {
            decoded(json!([]))
        } else if name == "null-candles" {
            EventValue::Null
        } else {
            decoded(f["candles"].clone())
        };
        let err = error(name);
        let s = Rc::new(RefCell::new(SeedState {
            name: name.into(),
            error: err.clone(),
            trace: vec![],
            notices: vec![],
            candles,
            callbacks: vec![],
            callback_refs: vec![],
            cleanups: 0,
            params: None,
            public_opts: None,
            callback_identity: false,
            owner_reads: 0,
        }));
        let state = s.clone();
        let callback: CandleCallback = Rc::new(move |v| {
            let mut s = state.borrow_mut();
            s.trace.push(json!(["callback"]));
            let last = if let EventValue::Array(a) = &s.candles {
                a.borrow().last().cloned().flatten()
            } else {
                None
            };
            s.callback_refs
                .push(last.as_ref().is_some_and(|last| v.same(last)));
            s.callbacks.push(v);
            if s.name == "callback-fail" {
                Err(s.error.clone())
            } else {
                Ok(())
            }
        });
        let mut b = SeedBind {
            state: s.clone(),
            opts: opts.clone(),
            callback: callback.clone(),
            jobs: vec![],
        };
        let cleanup = listen_candle(opts.clone(), callback, &mut b);
        let mut primary = cleanup.as_ref().err().cloned();
        if let Ok(c) = &cleanup {
            if matches!(
                name,
                "cleanup-first" | "unsubscribe-fail" | "repeat-cleanup" | "cleanup-reject"
            ) {
                if let Err(e) = c() {
                    primary = Some(e)
                } else if name == "repeat-cleanup" {
                    if let Err(e) = c() {
                        primary = Some(e)
                    }
                }
            }
        }
        let jobs = b.jobs.len();
        let mut job_errors = vec![];
        for task in b.jobs {
            if let Err(e) = settle(task) {
                job_errors.push(e)
            }
        }
        let state = s.borrow();
        assert_eq!(
            jobs,
            usize::from(!matches!(name, "external" | "fetch-sync-fail")),
            "{label}: native launched job count"
        );
        assert_eq!(
            cleanup.is_ok(),
            name != "fetch-sync-fail",
            "{label}: native cleanup returned"
        );
        assert_eq!(
            primary.is_some(),
            matches!(name, "fetch-sync-fail" | "unsubscribe-fail"),
            "{label}: native primary failure"
        );
        if let Some(e) = &primary {
            original(e, &err, &label)
        }
        assert_eq!(
            job_errors.len(),
            usize::from(matches!(name, "capture-log-fail" | "log-fail")),
            "{label}: native final rejected job count"
        );
        for e in &job_errors {
            original(e, &err, &label)
        }
        let callback_count = usize::from(matches!(name, "normal" | "callback-fail"));
        assert_eq!(
            state.callbacks.len(),
            callback_count,
            "{label}: native callback count"
        );
        assert_eq!(
            state.callback_refs.len(),
            callback_count,
            "{label}: native callback receipt count"
        );
        for (v, original_ref) in state.callbacks.iter().zip(&state.callback_refs) {
            assert!(*original_ref, "{label}: native original latest candle");
            for (k, n) in [
                ("timestamp", 2000.0),
                ("open", 12.2),
                ("high", 13.4),
                ("low", 11.3),
                ("close", 12.3),
                ("volume", 4.2),
            ] {
                num(
                    &v.get(k).unwrap(),
                    n,
                    &format!("{label}: native candle {k}"),
                );
            }
        }
        assert_eq!(
            state.cleanups,
            if name == "repeat-cleanup" {
                2
            } else {
                usize::from(matches!(
                    name,
                    "cleanup-first" | "unsubscribe-fail" | "cleanup-reject"
                ))
            },
            "{label}: native repeated cleanup count"
        );
        assert!(
            state.callback_identity,
            "{label}: native original callback argument"
        );
        assert!(
            state.public_opts.as_ref().unwrap().same(&opts),
            "{label}: native original options argument"
        );
        num(
            &opts.read("limit").unwrap(),
            5.0,
            &format!("{label}: native original options limit"),
        );
        if let Some(params) = &state.params {
            assert!(!params.same(&opts), "{label}: native shallow seed options");
            assert!(
                params.read("nested").unwrap().same(&nested),
                "{label}: native nested options referent"
            );
            num(
                &params.read("limit").unwrap(),
                1.0,
                &format!("{label}: native seed limit"),
            );
            text(&params.read("symbol").unwrap(), "BTCUSDT", &label);
            text(&params.read("interval").unwrap(), "1m", &label);
        }
        assert_eq!(
            state.params.is_some(),
            name != "external",
            "{label}: native seed fetch call count"
        );
        assert_eq!(
            state.owner_reads,
            usize::from(matches!(
                name,
                "normal" | "empty" | "owner-disposed" | "callback-fail" | "null-candles"
            )),
            "{label}: native disposal read count"
        );
        let notice_count = usize::from(matches!(
            name,
            "fetch-reject"
                | "callback-fail"
                | "null-candles"
                | "empty-error"
                | "log-fail"
                | "cleanup-reject"
        ));
        assert_eq!(
            state.notices.len(),
            notice_count,
            "{label}: native debug notice count"
        );
        for n in &state.notices {
            let Notice::Log(message, severity) = n else {
                panic!()
            };
            assert_eq!(*severity, LogSeverity::Debug, "{label}: native severity");
            assert_eq!(
                message,
                &format!(
                    "Unable to seed Binance live candle: {}",
                    if name == "empty-error" {
                        "RangeError"
                    } else if name == "null-candles" {
                        "Cannot read properties of null (reading 'length')"
                    } else {
                        "history fixture"
                    }
                ),
                "{label}: native debug message"
            );
        }
        actual.push(json!({"name":name,"trace":state.trace,"notices":state.notices.iter().map(Notice::wire).collect::<Vec<_>>(),"callbackRefs":state.callback_refs,"callbacks":state.callbacks.iter().map(wire).collect::<Vec<_>>(),"cleanups":state.cleanups,"params":state.params.as_ref().map(wire).unwrap_or(json!({"$undefined":true})),"paramsRefs":state.params.as_ref().map(|p|json!({"root":p.same(&opts),"nested":p.read("nested").unwrap().same(&nested)})),"cleanup":cleanup.is_ok(),"jobs":jobs,"jobFailures":job_errors.iter().map(|e|error_wire(e,&err)).collect::<Vec<_>>(),"failure":primary.as_ref().map(|e|error_wire(e,&err))}));
    }
    compare("seed", actual, 15);
}

fn observed_error_code(
    code: &Option<safe_cex_exchange::js_runtime::value::Js>,
) -> Option<serde_json::Value> {
    code.as_ref().map(|value| {
        let finite =
            safe_cex_exchange::js_runtime::value::to_json(value).expect("finite JSON code fixture");
        let text = safe_cex_num::js_json_stringify(&finite.into())
            .expect("source JSON code fixture")
            .expect("defined code fixture");
        serde_json::from_str(&text).expect("JSON code observation")
    })
}
