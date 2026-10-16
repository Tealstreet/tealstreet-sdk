use safe_cex_venue_polymarket::{price_reads::*, rest_forwarding::MethodResponse};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    process::Command,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};

const NAMES: [&str; 8] = [
    "getMidpoint",
    "getMidpoints",
    "getPrice",
    "getLastTradePrice",
    "getPriceHistory",
    "getOrder",
    "getBalanceAllowance",
    "deriveApiKey",
];
const STRINGS: [&str; 16] = [
    "id/1",
    "",
    "🦀 λ,a?b#c%20",
    "",
    "",
    "a/b/c?x=1&y=2",
    "__proto__",
    "constructor",
    "a\0b",
    "quote\"\\end",
    "x\u{2028}y",
    "pair😀",
    "01",
    "-0",
    "",
    "final",
];
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn input(i: usize) -> Vec<u16> {
    match i {
        3 => vec![0xd800],
        4 => vec![0xdfff],
        14 => vec![0xd800, 0xdfff],
        _ => units(STRINGS[i]),
    }
}
fn price(i: usize) -> Vec<u16> {
    match i {
        0 => units("0.375tail"),
        1 => vec![],
        2 => units("\u{feff} -0"),
        3 => units("Infinityrest"),
        4 => units("-Infinity"),
        5 => units("1e999"),
        6 => units("1e-999"),
        7 => units("-1e-999"),
        8 => units("0x10"),
        9 => units("1e+oops"),
        10 => units(".25suffix"),
        11 => units("\u{85} 9"),
        12 => vec![49, 50, 0xd800, 57, 57],
        13 => vec![0xdfff, 49, 50],
        14 => units("\u{2028}+6.02e23ignore"),
        15 => units("5e-324suffix"),
        _ => unreachable!(),
    }
}
fn parsed(i: usize) -> f64 {
    [
        0.375,
        f64::NAN,
        -0.0,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        0.0,
        -0.0,
        0.0,
        1.0,
        0.25,
        f64::NAN,
        12.0,
        f64::NAN,
        6.02e23,
        f64::from_bits(1),
    ][i]
}
fn numeric(i: usize) -> f64 {
    [
        0.0,
        -0.0,
        1.5,
        -1.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        1e21,
        1e-7,
        1e-6,
        f64::MAX,
        f64::from_bits(1),
        9007199254740992.0,
        0.9999999999999999,
        f64::from_bits(1),
        -0.0,
    ][i]
}
fn oracle() -> &'static Value {
    static CAPTURE: OnceLock<Result<Value, String>> = OnceLock::new();
    CAPTURE
        .get_or_init(|| {
            let r = Command::new("node")
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/price_reads_oracle.cjs"
                ))
                .output()
                .map_err(|e| e.to_string())?;
            if !r.status.success() {
                return Err(String::from_utf8_lossy(&r.stderr).into_owned());
            }
            serde_json::from_slice(&r.stdout).map_err(|e| e.to_string())
        })
        .as_ref()
        .unwrap_or_else(|e| panic!("source oracle:{e}"))
}
fn decoded(v: &Value) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| u16::try_from(x.as_u64().unwrap()).unwrap())
        .collect()
}
fn number_assert(actual: f64, expected: f64, source: &Value, label: &str) {
    if expected.is_nan() {
        assert!(actual.is_nan(), "{label}:native NaN");
        assert_eq!(source["kind"], "nan", "{label}:source NaN")
    } else {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "{label}:native Number bits"
        );
        assert_eq!(source["kind"], "number");
        assert_eq!(
            actual.to_bits(),
            u64::from_str_radix(source["bits"].as_str().unwrap(), 16).unwrap(),
            "{label}:source Number bits"
        )
    }
}
#[derive(Debug)]
struct Row {
    marker: u32,
    timestamp: f64,
    price: Vec<u16>,
    from_reply: u32,
    from_input: u32,
}
fn row(marker: u32, i: usize) -> Row {
    Row {
        marker,
        timestamp: -0.0,
        price: input(i),
        from_reply: 0,
        from_input: 0,
    }
}
fn payload(i: usize) -> HistoryData<Row> {
    match i {
        0 => HistoryData::Array(Rc::new(RefCell::new(vec![row(73, i)]))),
        1 => HistoryData::Undefined,
        2 => HistoryData::Null,
        3 => HistoryData::Bool(false),
        4 => HistoryData::Number(0.0),
        5 => HistoryData::Number(-0.0),
        6 => HistoryData::Number(f64::NAN),
        7 => HistoryData::String(vec![]),
        8 => HistoryData::Array(Rc::new(RefCell::new(vec![]))),
        9 => HistoryData::Bool(true),
        10 => HistoryData::Number(f64::INFINITY),
        11 => HistoryData::String(vec![0xd800, 114, 97, 119]),
        12 => HistoryData::Object(Rc::new(RefCell::new(row(73, i)))),
        13 => HistoryData::Number(f64::NEG_INFINITY),
        14 => HistoryData::String(units("-0")),
        15 => HistoryData::Array(Rc::new(RefCell::new(vec![row(73, i), row(79, i)]))),
        _ => unreachable!(),
    }
}
// Clone aliases without requiring Row to be Clone.
fn alias(v: &HistoryData<Row>) -> HistoryData<Row> {
    match v {
        HistoryData::Undefined => HistoryData::Undefined,
        HistoryData::Null => HistoryData::Null,
        HistoryData::Bool(v) => HistoryData::Bool(*v),
        HistoryData::Number(v) => HistoryData::Number(*v),
        HistoryData::String(v) => HistoryData::String(v.clone()),
        HistoryData::Array(v) => HistoryData::Array(v.clone()),
        HistoryData::Object(v) => HistoryData::Object(v.clone()),
    }
}
#[derive(Debug)]
struct Failure {
    marker: u32,
    message: &'static str,
}
type Error = Rc<RefCell<Failure>>;
struct Harness {
    calls: Rc<RefCell<Vec<ReadRequest>>>,
    completed: Rc<Cell<bool>>,
    dropped: Rc<Cell<usize>>,
    failure: Error,
    fails: bool,
}
impl Harness {
    fn new(fails: bool) -> Self {
        Self {
            calls: Rc::new(RefCell::new(vec![])),
            completed: Rc::new(Cell::new(false)),
            dropped: Rc::new(Cell::new(0)),
            failure: Rc::new(RefCell::new(Failure {
                marker: 47,
                message: "original read failure",
            })),
            fails,
        }
    }
}
struct Operation<T> {
    completed: Rc<Cell<bool>>,
    dropped: Rc<Cell<usize>>,
    data: Rc<RefCell<Option<T>>>,
    error: Option<Error>,
}
impl<T> Unpin for Operation<T> {}
impl<T> Future for Operation<T> {
    type Output = Result<MethodResponse<T>, Error>;
    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.completed.get() {
            return Poll::Pending;
        }
        Poll::Ready(match self.error.take() {
            Some(e) => Err(e),
            None => Ok(MethodResponse {
                data: self.data.borrow_mut().take().unwrap(),
            }),
        })
    }
}
impl<T> Drop for Operation<T> {
    fn drop(&mut self) {
        self.dropped.set(self.dropped.get() + 1)
    }
}
fn operation<T>(h: &Harness, data: T) -> Operation<T> {
    Operation {
        completed: h.completed.clone(),
        dropped: h.dropped.clone(),
        data: Rc::new(RefCell::new(Some(data))),
        error: h.fails.then(|| h.failure.clone()),
    }
}
fn poll<F: Future + ?Sized>(future: Pin<&mut F>) -> Poll<F::Output> {
    future.poll(&mut Context::from_waker(Waker::noop()))
}
#[derive(Debug)]
enum Output {
    Number(f64),
    Entries(Vec<(Vec<u16>, f64)>),
    Value(HistoryData<Row>),
}

fn request_assert(h: &Harness, name: &str, i: usize, case: &Value) {
    let calls = h.calls.borrow();
    assert_eq!(calls.len(), 1, "native eager request:{name}");
    let r = &calls[0];
    let s = &case["request"];
    assert_eq!(r.client, "xhr", "native receiver:{name}");
    assert_eq!(s["client"], "xhr");
    let verb = if name == "getMidpoints" {
        "post"
    } else {
        "get"
    };
    assert_eq!(r.method, verb, "native verb:{name}");
    assert_eq!(s["method"], verb);
    let url = match name {
        "getMidpoint" => "/midpoint",
        "getMidpoints" => "/midpoints",
        "getPrice" => "/price",
        "getLastTradePrice" => "/last-trade-price",
        "getPriceHistory" => "/prices-history",
        "getOrder" => "/data/order/",
        "getBalanceAllowance" => "/balance-allowance",
        "deriveApiKey" => "/auth/derive-api-key",
        _ => unreachable!(),
    };
    let mut expected_url = units(url);
    if name == "getOrder" {
        expected_url.extend(input(i))
    }
    assert_eq!(r.url, expected_url, "native request URL:{name}");
    assert_eq!(r.url, decoded(&s["url"]), "source request URL:{name}");
    let param_count = match name {
        "getMidpoint" | "getLastTradePrice" => 1,
        "getPrice" | "getBalanceAllowance" => 2,
        "getPriceHistory" => 5,
        _ => 0,
    };
    assert_eq!(
        r.params.as_ref().map_or(0, Vec::len),
        param_count,
        "independent native param count:{name}"
    );
    assert_eq!(
        s["params"].as_array().map_or(0, Vec::len),
        param_count,
        "independent source param count:{name}"
    );
    let keys: &[&str] = match name {
        "getMidpoint" | "getLastTradePrice" => &["token_id"],
        "getPrice" => &["token_id", "side"],
        "getBalanceAllowance" => &["asset_type", "signature_type"],
        "getPriceHistory" => &["market", "startTs", "endTs", "interval", "fidelity"],
        _ => &[],
    };
    if param_count == 0 {
        assert!(r.params.is_none());
        assert!(s["params"].is_null())
    }
    for (j, (key, value)) in r.params.iter().flatten().enumerate() {
        assert_eq!(*key, keys[j], "native parameter key:{name}");
        assert_eq!(s["params"][j][0], keys[j]);
        let src = &s["params"][j][1];
        match value {
            ParamValue::Undefined => {
                let absent = name == "getPriceHistory"
                    && match j {
                        1 => i.is_multiple_of(3),
                        2 => i.is_multiple_of(4),
                        3 => i.is_multiple_of(5),
                        4 => i.is_multiple_of(2),
                        _ => false,
                    };
                assert!(absent, "native present undefined argument:{name}");
                assert_eq!(src["kind"], "undefined")
            }
            ParamValue::String(value) => {
                let expected = if name == "getBalanceAllowance" {
                    units("COLLATERAL")
                } else if name == "getPrice" && j == 1 {
                    units(if i.is_multiple_of(2) { "BUY" } else { "SELL" })
                } else {
                    input(i)
                };
                assert_eq!(*value, expected, "native parameter units:{name}");
                assert_eq!(src["kind"], "string");
                assert_eq!(*value, decoded(&src["units"]));
                if name == "getPriceHistory" && j == 3 {
                    assert_ne!(i % 5, 0)
                }
            }
            ParamValue::Number(value) => {
                let expected = if name == "getBalanceAllowance" {
                    numeric(i)
                } else {
                    match j {
                        1 => {
                            assert_ne!(i % 3, 0);
                            numeric(i)
                        }
                        2 => {
                            assert_ne!(i % 4, 0);
                            numeric((i + 5) % 16)
                        }
                        4 => {
                            assert_ne!(i % 2, 0);
                            numeric((i + 7) % 16)
                        }
                        _ => panic!("unexpected numeric param"),
                    }
                };
                number_assert(
                    *value,
                    expected,
                    src,
                    &format!("native numeric parameter:{name}"),
                )
            }
        }
    }
    if name == "deriveApiKey" {
        let headers = r.headers.as_ref().unwrap();
        assert_eq!(headers.len(), 4, "independent header count");
        assert_eq!(s["headers"].as_array().unwrap().len(), 4);
        for (j, key) in [
            "POLY_ADDRESS",
            "POLY_SIGNATURE",
            "POLY_TIMESTAMP",
            "POLY_NONCE",
        ]
        .iter()
        .enumerate()
        {
            assert_eq!(headers[j].0, *key, "native header key:{name}");
            assert_eq!(s["headers"][j][0], *key);
            assert_eq!(
                headers[j].1,
                decoded(&s["headers"][j][1]),
                "native header units:{name}"
            );
        }
        assert_eq!(headers[0].1, input(i));
        let mut signature = units("sig-");
        signature.extend(input(i));
        assert_eq!(headers[1].1, signature);
    } else {
        assert!(r.headers.is_none());
        assert!(s["headers"].is_null())
    }
    if name == "getMidpoints" {
        let rows = r.data.as_ref().unwrap();
        let count = if i == 1 { 0 } else { 3 };
        assert_eq!(rows.len(), count, "independent mapped row count");
        assert_eq!(s["data"].as_array().unwrap().len(), count);
        for (j, row) in rows.iter().enumerate() {
            assert_eq!(row.len(), 1, "native mapped row field count");
            assert_eq!(
                s["data"][j].as_array().unwrap().len(),
                1,
                "independent source mapped row field count"
            );
            assert_eq!(row[0].0, "token_id", "native mapped row key:getMidpoints");
            let expected = if j == 1 { units("second") } else { input(i) };
            assert_eq!(row[0].1, expected, "native mapped token units");
            assert_eq!(s["data"][j][0][0], "token_id");
            assert_eq!(row[0].1, decoded(&s["data"][j][0][1]));
        }
    } else {
        assert!(r.data.is_none());
        assert!(s["data"].is_null())
    }
}
fn row_assert(r: &Row, marker: u32, i: usize, s: &Value) {
    assert_eq!(r.marker, marker, "native row marker");
    assert_eq!(
        r.timestamp.to_bits(),
        (-0.0f64).to_bits(),
        "native row negative zero"
    );
    assert_eq!(r.price, input(i), "native row UTF16");
    assert_eq!((r.from_reply, r.from_input), (0, 0));
    assert_eq!(s["kind"], "object");
    assert_eq!(s["marker"], marker);
    number_assert(r.timestamp, -0.0, &s["timestamp"], "source row timestamp");
    assert_eq!(r.price, decoded(&s["price"]));
    assert_eq!(s["fromReply"], 0);
    assert_eq!(s["fromInput"], 0)
}
fn value_assert(
    actual: &HistoryData<Row>,
    original: &HistoryData<Row>,
    name: &str,
    i: usize,
    s: &Value,
) {
    let fallback = name == "getPriceHistory" && (1..=7).contains(&i);
    assert_eq!(s["fallback"], fallback, "source history truthiness");
    if fallback {
        let HistoryData::Array(array) = actual else {
            panic!("native history fallback:getPriceHistory")
        };
        assert_eq!(
            array.borrow().len(),
            0,
            "native fresh fallback count:getPriceHistory"
        );
        assert_eq!(s["kind"], "array");
        assert_eq!(s["rows"].as_array().unwrap().len(), 0);
        assert_eq!(s["same"], false);
        return;
    }
    assert_eq!(s["same"], true, "source ordinary value identity:{name}");
    match (actual, i) {
        (HistoryData::Undefined, 1) => assert_eq!(s["kind"], "undefined"),
        (HistoryData::Null, 2) => assert_eq!(s["kind"], "null"),
        (HistoryData::Bool(value), 3 | 9) => {
            assert_eq!(*value, i == 9, "native boolean reply");
            assert_eq!(s["kind"], "bool");
            assert_eq!(s["value"], *value)
        }
        (HistoryData::Number(value), 4 | 5 | 6 | 10 | 13) => {
            let expected = match i {
                4 => 0.0,
                5 => -0.0,
                6 => f64::NAN,
                10 => f64::INFINITY,
                13 => f64::NEG_INFINITY,
                _ => unreachable!(),
            };
            number_assert(
                *value,
                expected,
                s,
                &format!("native returned number:{name}"),
            )
        }
        (HistoryData::String(value), 7 | 11 | 14) => {
            let expected = match i {
                7 => vec![],
                11 => vec![0xd800, 114, 97, 119],
                14 => units("-0"),
                _ => unreachable!(),
            };
            assert_eq!(*value, expected, "native returned UTF16:{name}");
            assert_eq!(s["kind"], "string");
            assert_eq!(*value, decoded(&s["units"]));
        }
        (HistoryData::Array(array), 0 | 8 | 15) => {
            let HistoryData::Array(before) = original else {
                panic!("original array")
            };
            assert!(
                Rc::ptr_eq(array, before),
                "native original array identity:{name}"
            );
            let count = match i {
                0 => 1,
                8 => 0,
                15 => 2,
                _ => unreachable!(),
            };
            assert_eq!(
                array.borrow().len(),
                count,
                "independent native returned array count"
            );
            assert_eq!(s["rows"].as_array().unwrap().len(), count);
            for (j, row) in array.borrow().iter().enumerate() {
                row_assert(row, if j == 0 { 73 } else { 79 }, i, &s["rows"][j])
            }
            if count != 0 {
                array.borrow_mut()[0].from_reply = 17;
                assert_eq!(
                    before.borrow()[0].from_reply,
                    17,
                    "native reply-to-input alias"
                );
                before.borrow_mut()[0].from_input = 23;
                assert_eq!(
                    array.borrow()[0].from_input,
                    23,
                    "native input-to-reply alias"
                );
                assert_eq!(s["replyToInput"], true);
                assert_eq!(s["inputToReply"], true)
            }
        }
        (HistoryData::Object(object), 12) => {
            let HistoryData::Object(before) = original else {
                panic!("original object")
            };
            assert!(
                Rc::ptr_eq(object, before),
                "native original object identity:{name}"
            );
            row_assert(&object.borrow(), 73, i, s);
            object.borrow_mut().from_reply = 17;
            assert_eq!(
                before.borrow().from_reply,
                17,
                "native object reply-to-input alias"
            );
            before.borrow_mut().from_input = 23;
            assert_eq!(
                object.borrow().from_input,
                23,
                "native object input-to-reply alias"
            );
            assert_eq!(s["replyToInput"], true);
            assert_eq!(s["inputToReply"], true);
        }
        _ => panic!("native ordinary response shape:{name} index{i}"),
    }
}
fn check(name: &str) {
    let all = oracle()["cases"].as_array().unwrap();
    assert_eq!(
        all.len(),
        256,
        "independent eight by sixteen by two fixture floor"
    );
    assert_eq!(all[255]["name"], "deriveApiKey");
    assert_eq!(all[255]["index"], 15);
    assert_eq!(all[255]["fails"], true);
    for (j, expected) in NAMES.iter().enumerate() {
        assert_eq!(all[j * 32]["name"], *expected);
        assert_eq!(all[j * 32 + 31]["index"], 15);
        assert_eq!(all[j * 32 + 31]["fails"], true);
    }
    let cases: Vec<_> = all.iter().filter(|c| c["name"] == name).collect();
    assert_eq!(cases.len(), 32, "independent method fixture count");
    for (iteration, case) in cases.iter().enumerate() {
        let i = iteration / 2;
        let fails = iteration % 2 == 1;
        assert_eq!(case["index"], i);
        assert_eq!(case["fails"], fails);
        let h = Harness::new(fails);
        let arg = input(i);
        let mut ids = if i == 1 {
            vec![]
        } else {
            vec![arg.clone(), units("second"), arg.clone()]
        };
        let original = payload(i);
        let mut price_op = operation(
            &h,
            PriceData {
                mid: units("-55"),
                price: units("66"),
            },
        );
        let price_field = price_op.data.clone();
        let mut key = units("key-");
        key.extend(&arg);
        let keys = [
            units("2"),
            units("10"),
            units("01"),
            units("__proto__"),
            units("constructor"),
            key,
        ];
        let midpoint = MidpointObject {
            entries: keys
                .iter()
                .cloned()
                .zip([
                    price(i),
                    units("-0"),
                    units("Infinity"),
                    units("bad"),
                    units("0x99"),
                    units("1e+bad"),
                ])
                .collect(),
        };
        let mut midpoint_op = operation(&h, midpoint);
        let mut history_op = operation(
            &h,
            HistoryResponse {
                history: alias(&original),
            },
        );
        let mut value_op = operation(&h, alias(&original));
        let calls = h.calls.clone();
        macro_rules! observe{($op:ident,$method:ident,$map:expr $(,$arg:expr)*)=>{{let borrowed = &mut $op; let observing=$method(move|r|{calls.borrow_mut().push(r);Pin::new(borrowed)} $(,$arg)*);Box::pin(async move{observing.await.map($map)})}};}
        let side = units(if i % 2 == 0 { "BUY" } else { "SELL" });
        let mut signature = units("sig-");
        signature.extend(&arg);
        let mut observing: Pin<Box<dyn Future<Output = Result<Output, Error>> + '_>> = match name {
            "getMidpoint" => observe!(price_op, get_midpoint, Output::Number, &arg),
            "getMidpoints" => observe!(midpoint_op, get_midpoints, Output::Entries, &ids),
            "getPrice" => observe!(price_op, get_price, Output::Number, &arg, &side),
            "getLastTradePrice" => observe!(price_op, get_last_trade_price, Output::Number, &arg),
            "getPriceHistory" => observe!(
                history_op,
                get_price_history,
                Output::Value,
                &arg,
                (i % 3 != 0).then(|| numeric(i)),
                (i % 4 != 0).then(|| numeric((i + 5) % 16)),
                (i % 5 != 0).then_some(arg.as_slice()),
                (i % 2 != 0).then(|| numeric((i + 7) % 16))
            ),
            "getOrder" => observe!(value_op, get_order, Output::Value, &arg),
            "getBalanceAllowance" => {
                observe!(value_op, get_balance_allowance, Output::Value, numeric(i))
            }
            "deriveApiKey" => observe!(
                value_op,
                derive_api_key,
                Output::Value,
                &arg,
                numeric(i),
                &signature,
                numeric((i + 5) % 16)
            ),
            _ => unreachable!(),
        };
        request_assert(&h, name, i, case);
        assert!(
            poll(observing.as_mut()).is_pending(),
            "native awaits host:{name}"
        );
        assert_eq!(h.dropped.get(), 0, "native host retains operation:{name}");
        *price_field.borrow_mut() = Some(PriceData {
            mid: price(i),
            price: price((i + 3) % 16),
        });
        if name == "getMidpoints" && !ids.is_empty() {
            h.calls.borrow_mut()[0].data.as_mut().unwrap()[0][0].1 = units("from-host");
            assert_eq!(ids[0], arg, "native fresh mapped row:getMidpoints");
        }
        h.completed.set(true);
        let Poll::Ready(result) = poll(observing.as_mut()) else {
            panic!("native completed operation remains pending:{name}")
        };
        let source = &case["result"];
        if fails {
            let e = result.expect_err("native source error required");
            assert!(
                Rc::ptr_eq(&e, &h.failure),
                "native original failure identity:{name}"
            );
            assert_eq!(e.borrow().marker, 47);
            assert_eq!(e.borrow().message, "original read failure");
            assert_eq!(source["kind"], "error");
            assert_eq!(source["marker"], 47);
            assert_eq!(source["message"], "original read failure");
            e.borrow_mut().marker = 53;
            assert_eq!(h.failure.borrow().marker, 53, "native original error alias");
        } else {
            match result.unwrap() {
                Output::Number(v) => number_assert(
                    v,
                    parsed(if name == "getMidpoint" {
                        i
                    } else {
                        (i + 3) % 16
                    }),
                    source,
                    &format!("native price result:{name}"),
                ),
                Output::Entries(entries) => {
                    assert_eq!(entries.len(), 6, "independent native midpoint entry count");
                    assert_eq!(source["values"].as_array().unwrap().len(), 6);
                    for (j, (key, v)) in entries.iter().enumerate() {
                        assert_eq!(*key, keys[j], "native midpoint own key:getMidpoints");
                        assert_eq!(*key, decoded(&source["values"][j][0]));
                        number_assert(
                            *v,
                            match j {
                                0 => parsed(i),
                                1 => -0.0,
                                2 => f64::INFINITY,
                                3 => f64::NAN,
                                4 => 0.0,
                                5 => 1.0,
                                _ => unreachable!(),
                            },
                            &source["values"][j][1],
                            "native midpoint mapped Number:getMidpoints",
                        );
                    }
                }
                Output::Value(v) => value_assert(&v, &original, name, i, source),
            }
        }
        drop(observing);
        if name == "getMidpoints" && !ids.is_empty() {
            ids[0] = units("from-input");
            assert_eq!(
                h.calls.borrow()[0].data.as_ref().unwrap()[0][0].1,
                units("from-host"),
                "native input-to-body detached:getMidpoints"
            );
        }
        assert_eq!(
            h.dropped.get(),
            0,
            "native observer does not own operation:{name}"
        );
    }
    assert_eq!(cases[31]["index"], 15);
    assert_eq!(cases[31]["fails"], true, "last per-method fixture");
}
macro_rules! row_test {
    ($test:ident,$name:literal) => {
        #[test]
        fn $test() {
            check($name)
        }
    };
}
row_test!(midpoint, "getMidpoint");
row_test!(midpoints, "getMidpoints");
row_test!(price_value, "getPrice");
row_test!(last_trade_price, "getLastTradePrice");
row_test!(price_history, "getPriceHistory");
row_test!(order, "getOrder");
row_test!(balance_allowance, "getBalanceAllowance");
row_test!(derive_key, "deriveApiKey");
#[test]
fn abandoned_observer_keeps_host_operation() {
    let h = Harness::new(false);
    let mut op = operation(
        &h,
        PriceData {
            mid: units("1"),
            price: units("2"),
        },
    );
    let completed = h.completed.clone();
    let dropped = h.dropped.clone();
    let future = get_midpoint(
        |r| {
            h.calls.borrow_mut().push(r);
            Pin::new(&mut op)
        },
        &units("ignored"),
    );
    drop(future);
    assert_eq!(h.calls.borrow().len(), 1);
    assert_eq!(dropped.get(), 0);
    completed.set(true);
    assert!(poll(Pin::new(&mut op)).is_ready());
    assert_eq!(oracle()["detachedCompleted"], 1);
    drop(op);
    assert_eq!(dropped.get(), 1);
}

#[test]
fn history_fallback_arrays_are_fresh() {
    let h = Harness::new(false);
    h.completed.set(true);
    let mut op1 = operation(
        &h,
        HistoryResponse::<Row> {
            history: HistoryData::Bool(false),
        },
    );
    let mut op2 = operation(
        &h,
        HistoryResponse::<Row> {
            history: HistoryData::Bool(false),
        },
    );
    let first = get_price_history(|_| Pin::new(&mut op1), &[], None, None, None, None);
    let second = get_price_history(|_| Pin::new(&mut op2), &[], None, None, None, None);
    let Poll::Ready(Ok(HistoryData::Array(first))) = poll(Box::pin(first).as_mut()) else {
        panic!("native first history fallback")
    };
    let Poll::Ready(Ok(HistoryData::Array(second))) = poll(Box::pin(second).as_mut()) else {
        panic!("native second history fallback")
    };
    assert_eq!(first.borrow().len(), 0);
    assert_eq!(second.borrow().len(), 0);
    assert!(
        !Rc::ptr_eq(&first, &second),
        "native independently fresh history fallbacks"
    );
    first.borrow_mut().push(row(73, 0));
    assert_eq!(
        second.borrow().len(),
        0,
        "native fallback arrays do not alias"
    );
    assert_eq!(oracle()["freshFallbacks"], true);
}
