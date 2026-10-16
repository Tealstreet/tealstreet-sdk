use safe_cex_venue_polymarket::rest_forwarding::*;
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

type Data = Rc<RefCell<Payload>>;
type Failure = Rc<RefCell<NativeFailure>>;
#[derive(Debug)]
struct NativeFailure {
    code: &'static str,
    message: &'static str,
    marker: u32,
}
#[derive(Debug)]
enum Payload {
    Object {
        tag: &'static str,
        nested: Vec<f64>,
        from_reply: u32,
        from_input: u32,
    },
    Null,
    Bool(bool),
    Number(f64),
    String(Vec<u16>),
    Array(Vec<Vec<u16>>),
    Undefined,
}
fn units(value: &str) -> Vec<u16> {
    value.encode_utf16().collect()
}
fn payload(index: usize) -> Data {
    Rc::new(RefCell::new(match index {
        0 => Payload::Object {
            tag: "reply",
            nested: vec![17.0, 23.0],
            from_reply: 0,
            from_input: 0,
        },
        1 => Payload::Null,
        2 => Payload::Bool(false),
        3 => Payload::Number(0.0),
        4 => Payload::String(units("raw reply")),
        5 => Payload::Array(vec![units("reply-A"), units("reply-B")]),
        6 => Payload::Undefined,
        7 => Payload::Number(f64::NAN),
        8 => Payload::Number(-0.0),
        _ => panic!("outside fixed nine-case floor"),
    }))
}
fn oracle() -> &'static Value {
    static CAPTURE: OnceLock<Value> = OnceLock::new();
    CAPTURE.get_or_init(|| {
        let result = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/rest_forwarding_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let value: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(
            value["cases"].as_array().unwrap().len(),
            180,
            "independent forwarding fixture floor"
        );
        assert_eq!(value["cases"][179]["name"], "deleteApiKey");
        assert_eq!(value["cases"][179]["index"], 8);
        assert_eq!(
            value["cases"][179]["fails"], true,
            "last forwarding fixture"
        );
        value
    })
}
fn decoded_units(value: &Value) -> Vec<u16> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|n| u16::try_from(n.as_u64().unwrap()).unwrap())
        .collect()
}

// The host owns the already-started operation. Polling this future observes its
// completion; abandoning the borrowed observer does not own/drop the operation.
struct Operation {
    completed: Rc<Cell<bool>>,
    dropped: Rc<Cell<u32>>,
    data: Option<Data>,
    error: Option<Failure>,
}
impl Future for Operation {
    type Output = Result<MethodResponse<Data>, Failure>;
    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.completed.get() {
            return Poll::Pending;
        }
        Poll::Ready(match self.error.take() {
            Some(error) => Err(error),
            None => Ok(MethodResponse {
                data: self.data.take().unwrap(),
            }),
        })
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        self.dropped.set(self.dropped.get() + 1);
    }
}
fn poll<F: Future + ?Sized>(future: Pin<&mut F>) -> Poll<F::Output> {
    future.poll(&mut Context::from_waker(Waker::noop()))
}
fn done<F: Future + ?Sized>(future: Pin<&mut F>) -> F::Output {
    match poll(future) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("completed host operation pending"),
    }
}
fn invoke<'a>(
    name: &str,
    operation: &'a mut Operation,
    calls: Rc<RefCell<Vec<MethodRequest>>>,
    arg: &'a [u16],
    ids: Rc<RefCell<Vec<Vec<u16>>>>,
) -> Pin<Box<dyn Future<Output = Result<Option<Data>, Failure>> + 'a>> {
    let send = move |request| {
        calls.borrow_mut().push(request);
        Pin::new(operation)
    };
    macro_rules! read { ($method:ident $(,$arg:expr)*) => {{ let observing = $method(send $(,$arg)*); Box::pin(async move { observing.await.map(Some) }) }}; }
    macro_rules! delete { ($method:ident $(,$arg:expr)*) => {{ let observing = $method(send $(,$arg)*); Box::pin(async move { observing.await.map(|()| None) }) }}; }
    match name {
        "getSimplifiedMarkets" => read!(get_simplified_markets),
        "getSpread" => read!(get_spread, arg),
        "getGammaMarket" => read!(get_gamma_market, arg),
        "getGammaMarketBySlug" => read!(get_gamma_market_by_slug, arg),
        "searchGammaMarkets" => read!(search_gamma_markets, arg),
        "cancelOrder" => delete!(cancel_order, arg),
        "cancelOrders" => delete!(cancel_orders, ids),
        "cancelMarketOrders" => delete!(cancel_market_orders, arg),
        "cancelAllOrders" => delete!(cancel_all_orders),
        "deleteApiKey" => delete!(delete_api_key),
        _ => panic!("unknown source method"),
    }
}
fn native_payload(data: &Data, index: usize, expected: &Value) {
    match (&*data.borrow(), index) {
        (
            Payload::Object {
                tag,
                nested,
                from_reply,
                from_input,
            },
            0,
        ) => {
            assert_eq!(*tag, "reply", "native forwarding object tag");
            assert_eq!(nested.len(), 2, "independent native nested count");
            assert_eq!(nested[0].to_bits(), 17.0_f64.to_bits());
            assert_eq!(nested[1].to_bits(), 23.0_f64.to_bits());
            assert_eq!((*from_reply, *from_input), (0, 0));
        }
        (Payload::Null, 1) | (Payload::Undefined, 6) => {}
        (Payload::Bool(value), 2) => assert!(!value, "native false response"),
        (Payload::Number(number), 3 | 7 | 8) => {
            if index == 7 {
                assert!(number.is_nan(), "native forwarded NaN");
            } else {
                assert_eq!(
                    number.to_bits(),
                    u64::from_str_radix(expected["numberBits"].as_str().unwrap(), 16).unwrap(),
                    "native forwarded number bits"
                );
            }
        }
        (Payload::String(value), 4) => {
            assert_eq!(value, &units("raw reply"), "native forwarded string")
        }
        (Payload::Array(values), 5) => {
            assert_eq!(values.len(), 2, "independent response array count");
            assert_eq!(values[0], units("reply-A"));
            assert_eq!(values[1], units("reply-B"));
        }
        _ => panic!("native response shape changed"),
    }
}
fn check(name: &str) {
    let cases: Vec<_> = oracle()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["name"] == name)
        .collect();
    assert_eq!(
        cases.len(),
        18,
        "independent per-method nine inputs times two outcomes"
    );
    let reads = name.starts_with("get") || name == "searchGammaMarkets";
    for (iteration, case) in cases.iter().enumerate() {
        let index = iteration / 2;
        let fails = iteration % 2 == 1;
        assert_eq!(case["index"], index);
        assert_eq!(case["fails"], fails);
        let input = decoded_units(&case["arg"]);
        let ids = Rc::new(RefCell::new(
            case["ids"]
                .as_array()
                .unwrap()
                .iter()
                .map(decoded_units)
                .collect::<Vec<_>>(),
        ));
        let original = payload(index);
        let failure = Rc::new(RefCell::new(NativeFailure {
            code: "E_NATIVE",
            message: "original failure",
            marker: 71,
        }));
        let completed = Rc::new(Cell::new(false));
        let dropped = Rc::new(Cell::new(0));
        let mut operation = Operation {
            completed: completed.clone(),
            dropped: dropped.clone(),
            data: Some(original.clone()),
            error: fails.then(|| failure.clone()),
        };
        let calls = Rc::new(RefCell::new(Vec::new()));
        let mut observing = invoke(name, &mut operation, calls.clone(), &input, ids.clone());
        assert_eq!(
            calls.borrow().len(),
            1,
            "native eager single request:{name}"
        );
        assert!(
            poll(observing.as_mut()).is_pending(),
            "native awaits operation:{name}"
        );
        assert_eq!(
            dropped.get(),
            0,
            "native operation remains host-owned:{name}"
        );
        {
            let calls = calls.borrow();
            let request = &calls[0];
            let source = &case["request"];
            assert_eq!(
                request.url,
                decoded_units(&source["url"]),
                "native request URL:{name}"
            );
            assert_eq!(
                request.method,
                if reads { "get" } else { "delete" },
                "native request verb:{name}"
            );
            assert_eq!(
                request.client,
                if name.contains("Gamma") {
                    "gammaXhr"
                } else {
                    "xhr"
                },
                "native receiver field:{name}"
            );
            assert_eq!(
                source["method"], request.method,
                "source request verb:{name}"
            );
            assert_eq!(
                source["client"], request.client,
                "source receiver field:{name}"
            );
            let has_params = ["getSpread", "searchGammaMarkets"].contains(&name);
            assert_eq!(
                request.params.as_ref().map_or(0, Vec::len),
                usize::from(has_params),
                "independent native parameter count:{name}"
            );
            assert_eq!(
                source["params"].as_array().map_or(0, Vec::len),
                usize::from(has_params),
                "independent source parameter count:{name}"
            );
            if has_params {
                let params = request.params.as_ref().unwrap();
                assert_eq!(
                    params[0].0,
                    if name == "getSpread" {
                        "token_id"
                    } else {
                        "search"
                    },
                    "native parameter key:{name}"
                );
                assert_eq!(
                    source["params"][0][0], params[0].0,
                    "source parameter key:{name}"
                );
                assert_eq!(params[0].1, input, "native parameter units:{name}");
                assert_eq!(params[0].1, decoded_units(&source["params"][0][1]));
            } else {
                assert!(request.params.is_none(), "native absent params:{name}");
                assert!(source["params"].is_null(), "source absent params:{name}");
            }
            match (&request.data, name) {
                (MethodData::Fields(fields), "cancelOrder" | "cancelMarketOrders") => {
                    assert_eq!(
                        fields.len(),
                        1,
                        "independent native body field count:{name}"
                    );
                    assert_eq!(
                        fields[0].0,
                        if name == "cancelOrder" {
                            "orderID"
                        } else {
                            "market"
                        },
                        "native body key:{name}"
                    );
                    assert_eq!(source["data"]["kind"], "fields", "source body shape:{name}");
                    assert_eq!(
                        source["data"]["values"].as_array().unwrap().len(),
                        1,
                        "independent source body field count:{name}"
                    );
                    assert_eq!(
                        source["data"]["values"][0][0], fields[0].0,
                        "source body key:{name}"
                    );
                    assert_eq!(fields[0].1, input, "native body units:{name}");
                    assert_eq!(fields[0].1, decoded_units(&source["data"]["values"][0][1]));
                }
                (MethodData::Strings(values), "cancelOrders") => {
                    assert!(Rc::ptr_eq(values, &ids), "native cancel original array");
                    assert_eq!(
                        values.borrow().len(),
                        if index == 1 { 0 } else { 3 },
                        "independent native cancellation count"
                    );
                    assert_eq!(
                        source["data"]["kind"], "strings",
                        "source cancel body shape"
                    );
                    assert_eq!(
                        source["data"]["values"].as_array().unwrap().len(),
                        if index == 1 { 0 } else { 3 },
                        "independent source cancellation count"
                    );
                    for (i, value) in values.borrow().iter().enumerate() {
                        assert_eq!(*value, decoded_units(&source["data"]["values"][i]));
                    }
                    values.borrow_mut().push(units("from-host"));
                    assert_eq!(
                        ids.borrow().last().unwrap(),
                        &units("from-host"),
                        "native host to input array alias"
                    );
                    ids.borrow_mut().push(units("from-input"));
                    assert_eq!(
                        values.borrow().last().unwrap(),
                        &units("from-input"),
                        "native input to host array alias"
                    );
                }
                (MethodData::Absent, _) => assert_eq!(
                    source["data"]["kind"], "absent",
                    "native absent body:{name}"
                ),
                _ => panic!("native body shape:{name}"),
            }
        }
        completed.set(true);
        let result = done(observing.as_mut());
        if fails {
            let error = result.expect_err("native request failure propagated");
            assert!(
                Rc::ptr_eq(&error, &failure),
                "native original failure identity:{name}"
            );
            assert_eq!(error.borrow().code, "E_NATIVE");
            assert_eq!(error.borrow().message, "original failure");
            assert_eq!(error.borrow().marker, 71);
        } else if reads {
            let data = result.unwrap().expect("native read response");
            assert!(
                Rc::ptr_eq(&data, &original),
                "native original response identity:{name}"
            );
            native_payload(&data, index, case);
            if index == 0 {
                if let Payload::Object { from_reply, .. } = &mut *data.borrow_mut() {
                    *from_reply = 17;
                }
                assert!(
                    matches!(*original.borrow(), Payload::Object { from_reply: 17, .. }),
                    "native response to input alias"
                );
                if let Payload::Object { from_input, .. } = &mut *original.borrow_mut() {
                    *from_input = 23;
                }
                assert!(
                    matches!(*data.borrow(), Payload::Object { from_input: 23, .. }),
                    "native input to response alias"
                );
            }
        } else {
            assert!(result.unwrap().is_none(), "native delete void:{name}");
        }
        drop(observing);
        assert_eq!(
            dropped.get(),
            0,
            "native observer never owns operation:{name}"
        );
    }
    assert_eq!(cases[17]["index"], 8);
    assert_eq!(cases[17]["fails"], true, "last per-method fixture");
}
macro_rules! test_method {
    ($test:ident, $source:literal) => {
        #[test]
        fn $test() {
            check($source);
        }
    };
}
test_method!(simplified_markets, "getSimplifiedMarkets");
test_method!(spread, "getSpread");
test_method!(gamma_market, "getGammaMarket");
test_method!(gamma_slug, "getGammaMarketBySlug");
test_method!(gamma_search, "searchGammaMarkets");
test_method!(single_cancel, "cancelOrder");
test_method!(batch_cancel, "cancelOrders");
test_method!(market_cancel, "cancelMarketOrders");
test_method!(all_cancel, "cancelAllOrders");
test_method!(delete_key, "deleteApiKey");

#[test]
fn dropped_observation_keeps_host_operation_alive() {
    assert_eq!(
        oracle()["detachedCompleted"],
        1,
        "source ignored promise completes"
    );
    for polled in [false, true] {
        let completed = Rc::new(Cell::new(false));
        let dropped = Rc::new(Cell::new(0));
        let data = payload(0);
        let mut operation = Operation {
            completed: completed.clone(),
            dropped: dropped.clone(),
            data: Some(data.clone()),
            error: None,
        };
        let calls = Rc::new(RefCell::new(Vec::new()));
        let mut observation = invoke(
            "getSimplifiedMarkets",
            &mut operation,
            calls.clone(),
            &[],
            Rc::new(RefCell::new(Vec::new())),
        );
        assert_eq!(calls.borrow().len(), 1, "native detached eager request");
        if polled {
            assert!(poll(observation.as_mut()).is_pending());
        }
        drop(observation);
        assert_eq!(
            dropped.get(),
            0,
            "native dropping observation preserves host operation"
        );
        completed.set(true);
        let finished = done(Pin::new(&mut operation)).unwrap();
        assert!(
            Rc::ptr_eq(&finished.data, &data),
            "native detached operation still completes"
        );
    }
}
