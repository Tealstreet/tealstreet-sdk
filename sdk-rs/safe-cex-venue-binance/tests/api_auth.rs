use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_venue_binance::{api_factory::*, private_event_value::*, request_weight::Failure};
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
    panic!("authored executor stopped poll20 without settlement")
}
fn fixtures() -> Value {
    serde_json::from_str(include_str!("fixtures/api-auth.json")).unwrap()
}
fn decoded(v: Value) -> EventValue {
    EventValue::from_json(v)
}
fn failure() -> Failure {
    let mut e = ExchangeError::new("api fixture");
    e.name = "RangeError".into();
    e.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
        "api-code"
    )));
    e = e.with_retained_response((json!({"status":503})).into());
    Rc::new(e)
}
fn wire(v: &EventValue) -> Value {
    match v {
        EventValue::Undefined => json!({"$undefined":true}),
        EventValue::Null => Value::Null,
        EventValue::Bool(v) => json!(v),
        EventValue::Number(v) => json!(v),
        EventValue::String(v) => json!(String::from_utf16(v).unwrap()),
        EventValue::Array(v) => array_values(v).iter().map(wire).collect(),
        EventValue::Object(_) => v
            .spread()
            .iter()
            .map(|(k, v)| (k.clone(), wire(v)))
            .collect(),
        _ => panic!("fixture projection stopped before enum/inherited values"),
    }
}
fn text(v: &EventValue, want: &str, label: &str) {
    let EventValue::String(got) = v else {
        panic!("{label}: expected native String, {v:?}")
    };
    assert_eq!(
        got.as_ref(),
        want.encode_utf16().collect::<Vec<_>>(),
        "{label}"
    )
}
fn num(v: &EventValue, want: f64, label: &str) {
    let EventValue::Number(got) = v else {
        panic!("{label}: expected native Number, {v:?}")
    };
    assert_eq!(*got, want, "{label}")
}
fn boolean(v: &EventValue, want: bool, label: &str) {
    let EventValue::Bool(got) = v else {
        panic!("{label}: expected native Bool, {v:?}")
    };
    assert_eq!(*got, want, "{label}")
}
fn original(got: &Failure, want: &Failure, label: &str) {
    assert!(Rc::ptr_eq(got, want), "{label}: original Error Rc");
    assert_eq!(got.name, "RangeError", "{label}");
    assert_eq!(got.message, "api fixture", "{label}");
    assert_eq!(
        observed_error_code(&got.code),
        Some(json!("api-code")),
        "{label}"
    );
    assert_eq!(
        got.response_json_view(),
        Some(json!({"status":503})),
        "{label}"
    )
}
fn error_wire(e: &Failure, want: &Failure) -> Value {
    json!({"original":Rc::ptr_eq(e,want),"name":e.name,"message":e.message,"code":observed_error_code(&e.code).clone().unwrap_or(json!({"$undefined":true})),"response":e.response_json_view().clone().unwrap_or(json!({"$undefined":true}))})
}
#[derive(Default)]
struct Trace(Vec<Value>);
type Calls = Rc<RefCell<Trace>>;
struct Factory {
    case: String,
    error: Failure,
    client: EventValue,
    options: EventValue,
    exchange: EventValue,
    config: Option<ApiConfig>,
    mode: Option<ClientMode>,
    retry: Option<(u32, RetryRead)>,
    request: Option<RequestInterceptor>,
    response: Option<(ResponseInterceptor, Option<ResponseInterceptor>)>,
    retry_signature: Option<bool>,
    trace: Vec<Value>,
    installed: Option<(EventValue, EventValue, EventValue, ResponseTransform)>,
}
impl FactoryBindings for Factory {
    fn create(
        &mut self,
        mode: ClientMode,
        config: ApiConfig,
        opts: EventValue,
        exchange: EventValue,
    ) -> Result<EventValue, Failure> {
        self.trace.push(json!([
            "create",
            if mode == ClientMode::Proxy {
                "proxy"
            } else {
                "noProxy"
            }
        ]));
        assert!(opts.same(&self.options), "native factory original options");
        assert!(
            exchange.same(&self.exchange),
            "native factory original exchange"
        );
        if self.case == "create-fail" {
            return Err(self.error.clone());
        }
        self.mode = Some(mode);
        self.config = Some(config);
        Ok(self.client.clone())
    }
    fn retry(
        &mut self,
        c: EventValue,
        n: u32,
        r: RetryRead,
        signature: bool,
    ) -> Result<(), Failure> {
        self.trace.push(json!(["retry"]));
        assert!(c.same(&self.client), "native retry original client");
        self.retry = Some((n, r));
        self.retry_signature = Some(signature);
        if self.case == "retry-fail" {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
    fn request_use(&mut self, c: EventValue, h: RequestInterceptor) -> Result<(), Failure> {
        self.trace.push(json!(["request_use"]));
        assert!(c.same(&self.client), "native request original client");
        if self.case == "request-fail" {
            return Err(self.error.clone());
        }
        self.request = Some(h);
        Ok(())
    }
    fn install(
        &mut self,
        c: EventValue,
        o: EventValue,
        name: EventValue,
        t: ResponseTransform,
    ) -> Result<(), Failure> {
        self.trace.push(json!([
            "install",
            wire(&name),
            if t == ResponseTransform::BigInt {
                "bigInt"
            } else {
                "default"
            }
        ]));
        self.installed = Some((c, o, name, t));
        if self.case == "install-fail" {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
    fn response_use(
        &mut self,
        c: EventValue,
        s: ResponseInterceptor,
        f: Option<ResponseInterceptor>,
    ) -> Result<(), Failure> {
        self.trace.push(json!(["response_use"]));
        assert!(c.same(&self.client), "native response original client");
        self.response = Some((s, f));
        Ok(())
    }
}
struct Request {
    case: String,
    error: Failure,
    calls: Calls,
    input: EventValue,
    options: EventValue,
    sign_refs: Option<bool>,
    sign_receipt: Option<(EventValue, EventValue)>,
}
impl RequestBindings for Request {
    fn headers_from(&mut self, h: EventValue) -> Result<EventValue, Failure> {
        self.calls.borrow_mut().0.push(json!(["headers"]));
        if self.case == "headers-fail" {
            Err(self.error.clone())
        } else {
            Ok(EventValue::record_object(h.spread()))
        }
    }
    fn mark_skip(&mut self, c: EventValue) -> Result<EventValue, Failure> {
        self.calls.borrow_mut().0.push(json!(["skip"]));
        if self.case == "skip-fail" {
            return Err(self.error.clone());
        }
        c.write("safeCexSkipRemoteSigning", EventValue::Bool(true));
        Ok(c)
    }
    fn sign(
        &mut self,
        c: EventValue,
        o: EventValue,
        name: EventValue,
        window: EventValue,
    ) -> Result<ApiFuture, Failure> {
        self.calls
            .borrow_mut()
            .0
            .push(json!(["sign", wire(&name), wire(&window)]));
        self.sign_refs = Some(c.same(&self.input) && o.same(&self.options));
        self.sign_receipt = Some((name.clone(), window.clone()));
        if self.case == "sign-sync-fail" {
            return Err(self.error.clone());
        }
        let calls = self.calls.clone();
        let fail = self.case == "sign-reject";
        let e = self.error.clone();
        Ok(Box::pin(async move {
            calls.borrow_mut().0.push(json!(["resolve"]));
            if fail {
                Err(e)
            } else {
                let out = EventValue::record_object(c.spread());
                out.write("signed", EventValue::Bool(true));
                Ok(out)
            }
        }))
    }
}
struct Response {
    messages: Vec<EventValue>,
}
impl ResponseBindings for Response {
    fn emit(&mut self, m: EventValue) -> Result<(), Failure> {
        self.messages.push(m);
        Ok(())
    }
    // No fixture is a -1022 rejection, so the actual resend declines every response.
    fn resend(&mut self, _: EventValue) -> Result<EventValue, Failure> {
        Ok(EventValue::Undefined)
    }
}
fn options(name: &str) -> EventValue {
    let o = decoded(json!({"key":"key-A","secret":"secret-A"}));
    match name {
        "testnet" => o.write("testnet", EventValue::Bool(true)),
        "recv-zero" => o.write("extra", record([("recvWindow", number(0.0))])),
        "recv-null" => o.write("extra", record([("recvWindow", EventValue::Null)])),
        "recv-string" => o.write("extra", record([("recvWindow", string("2500"))])),
        "tunnel" | "install-fail" => o.write("tealstreetTunnel", EventValue::Bool(true)),
        _ => {}
    }
    o
}
fn is_public(family: &str) -> bool {
    family.ends_with("public")
}
fn stem(family: &str) -> &'static str {
    match family {
        "futures" | "futures_public" => "binance",
        "spot" => "binancespot",
        _ => "binancepm",
    }
}
fn factory_run(
    family: &str,
    name: &str,
    o: EventValue,
    e: EventValue,
    b: &mut Factory,
) -> Result<EventValue, Failure> {
    match family {
        "futures" => create_futures_api(o, e, b),
        "futures_public" => create_futures_public_api(o, e, b),
        "pm" => create_pm_api(
            o,
            e,
            if name == "pm-null" {
                EventValue::Null
            } else {
                EventValue::Undefined
            },
            b,
        ),
        "papi" => create_papi(o, e, b),
        "fapi" => create_fapi(o, e, b),
        "pm_public" => create_pm_public_api(o, e, b),
        "spot" => create_spot_api(o, e, b),
        _ => panic!(),
    }
}
fn oracle(family: &str) -> Value {
    let mut c = Command::new("node");
    c.current_dir(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap(),
    );
    c.args(["sdk-rs/safe-cex-venue-binance/scripts/api-auth.mjs", family]);
    let o = c.output().unwrap();
    assert!(
        o.status.success(),
        "source oracle {family}: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    serde_json::from_slice(&o.stdout).unwrap()
}
fn numeric_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| numeric_equal(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, a)| b.get(k).is_some_and(|b| numeric_equal(a, b)))
        }
        _ => a == b,
    }
}
fn factories(family: &str) {
    let fixtures = fixtures();
    let rows = fixtures["factories"].as_array().unwrap();
    assert_eq!(rows.len(), 12, "authored factory input floor");
    assert_eq!(fixtures["requests"].as_array().unwrap().len(), 10);
    assert_eq!(fixtures["responses"].as_array().unwrap().len(), 9);
    let mut actual = vec![];
    for (i, f) in rows.iter().enumerate() {
        let name = f["name"].as_str().unwrap();
        let label = format!("native {family} factory case{i} {name}");
        let o = options(name);
        let e = record([("name", string(stem(family)))]);
        let error = failure();
        let client = record([("client", number(9.0))]);
        let mut b = Factory {
            case: name.into(),
            error: error.clone(),
            client: client.clone(),
            options: o.clone(),
            exchange: e.clone(),
            config: None,
            mode: None,
            retry: None,
            retry_signature: None,
            request: None,
            response: None,
            trace: vec![],
            installed: None,
        };
        let result = factory_run(family, name, o.clone(), e.clone(), &mut b);
        let failed = matches!(name, "create-fail" | "retry-fail" | "request-fail")
            || (name == "install-fail" && family != "spot");
        assert_eq!(result.is_err(), failed, "{label}: native outcome");
        if let Err(got) = &result {
            original(got, &error, &label)
        } else {
            assert!(
                result.as_ref().unwrap().same(&client),
                "{label}: native returned client"
            );
        }
        let response_expected = !is_public(family) && !failed;
        if let Some(signature) = b.retry_signature {
            assert_eq!(
                signature,
                !is_public(family),
                "{label}: native signature-rejection retry arm"
            );
        }
        assert_eq!(
            b.response.is_some(),
            response_expected,
            "{label}: native response registration"
        );
        let mut config_wire = Value::Null;
        if let Some(c) = &b.config {
            let url = if name == "testnet" {
                if family == "spot" {
                    "https://testnet.binance.vision"
                } else {
                    "https://demo-fapi.binance.com"
                }
            } else if family == "spot" {
                "https://api.binance.com"
            } else if family == "papi" || (family == "pm" && name != "pm-null") {
                "https://papi.binance.com"
            } else {
                "https://fapi.binance.com"
            };
            text(&c.base_url, url, &format!("{label}: native base URL"));
            if name == "recv-string" {
                text(&c.timeout, "2500", &format!("{label}: native timeout"))
            } else {
                num(
                    &c.timeout,
                    if name == "recv-zero" { 0.0 } else { 20000.0 },
                    &format!("{label}: native timeout"),
                )
            }
            text(
                &c.headers.read("Content-Type").unwrap(),
                "application/json, charset=utf-8",
                &label,
            );
            let key = c.headers.read("X-MBX-APIKEY").unwrap();
            if is_public(family) {
                assert!(
                    matches!(key, EventValue::Undefined),
                    "{label}: native no public API key"
                )
            } else {
                text(&key, "key-A", &label)
            }
            assert_eq!(
                b.mode,
                Some(if is_public(family) {
                    ClientMode::NoProxy
                } else {
                    ClientMode::Proxy
                }),
                "{label}: native client mode"
            );
            assert_eq!(
                c.transform,
                if family.starts_with("futures") {
                    ResponseTransform::BigInt
                } else {
                    ResponseTransform::Default
                },
                "{label}: native response transform"
            );
            assert_eq!(
                c.serializer,
                QuerySerializer::Repeat,
                "{label}: native query codec"
            );
            let query = c
                .serializer
                .serialize(&json!({"symbol":["BTCUSDT","ETHUSDT"],"nested":{"v":"a b"},"n":null}));
            assert_eq!(
                query, "symbol=BTCUSDT&symbol=ETHUSDT&nested%5Bv%5D=a%20b&n=",
                "{label}: native query text"
            );
            config_wire = json!({"baseURL":wire(&c.base_url),"timeout":wire(&c.timeout),"headers":wire(&c.headers),"serializer":"repeat","query":query,"transform":if c.transform==ResponseTransform::BigInt{"bigInt"}else{"default"}});
        }
        if let Some((n, policy)) = &b.retry {
            assert_eq!(*n, 3, "{label}: native retries");
            assert_eq!(
                *policy,
                if family == "spot" {
                    RetryRead::Reads
                } else {
                    RetryRead::ReadsAndPath(if stem(family) == "binance" {
                        "/fapi/v1/listenKey"
                    } else {
                        "/papi/v1/listenKey"
                    })
                },
                "{label}: native retry read policy"
            );
        }
        if let Some((c, opts, n, t)) = &b.installed {
            assert!(
                c.same(&client) && opts.same(&o),
                "{label}: native transport referents"
            );
            text(n, stem(family), &label);
            assert_eq!(
                *t,
                if family.starts_with("futures") {
                    ResponseTransform::BigInt
                } else {
                    ResponseTransform::Default
                },
                "{label}: native transport transform"
            );
        }
        let expected_trace_len = if name == "create-fail" {
            1
        } else if name == "retry-fail" {
            2
        } else if name == "request-fail" {
            3
        } else {
            3 + usize::from(family != "spot" && matches!(name, "tunnel" | "install-fail"))
                + usize::from(response_expected)
        };
        assert_eq!(
            b.trace.len(),
            expected_trace_len,
            "{label}: native factory operation count"
        );
        let mut requests = vec![];
        let mut responses = vec![];
        if !failed {
            if name == "live-options" {
                o.write("testnet", EventValue::Bool(true));
                o.write("key", string("key-B"));
                o.write("extra", record([("recvWindow", number(9876.0))]));
                e.write("name", string("renamed"));
            }
            let handler = b.request.as_ref().unwrap();
            for (j, r) in fixtures["requests"].as_array().unwrap().iter().enumerate() {
                let rn = r["name"].as_str().unwrap();
                let label = format!("native {family} factory case{i} {name} request{j} {rn}");
                let nested = record([("value", number(5.0))]);
                let headers = record([("private", string("secret"))]);
                let input = record([
                    ("headers", headers.clone()),
                    ("timeout", number(500.0)),
                    ("nested", nested.clone()),
                ]);
                let ep = if family == "spot" {
                    "/api/v3"
                } else {
                    "/fapi/v1"
                };
                let public_path = format!("{ep}/exchangeInfo");
                match rn {
                    "listen" => input.write(
                        "url",
                        string(if stem(family) == "binancepm" {
                            "/papi/v1/listenKey".into()
                        } else {
                            format!("{ep}/listenKey")
                        }),
                    ),
                    "public" | "headers-fail" | "skip-fail" => {
                        input.write("url", string(&public_path))
                    }
                    "missing-url" => {}
                    "null-url" => input.write("url", EventValue::Null),
                    "array-url" => input.write("url", decoded(json!([public_path]))),
                    _ => input.write("url", string("/private")),
                }
                let listen = rn == "listen"
                    && !is_public(family)
                    && family != "spot"
                    && !matches!(name, "testnet" | "live-options");
                let public = is_public(family)
                    || matches!(rn, "public" | "headers-fail" | "skip-fail" | "array-url");
                let signed = !listen && !public;
                let req_failed = matches!(rn, "headers-fail" | "skip-fail")
                    || (signed && matches!(rn, "sign-sync-fail" | "sign-reject"));
                let calls = Rc::new(RefCell::new(Trace::default()));
                let mut request = Request {
                    case: rn.into(),
                    error: error.clone(),
                    calls: calls.clone(),
                    input: input.clone(),
                    options: o.clone(),
                    sign_refs: None,
                    sign_receipt: None,
                };
                let returned = handler(input.clone(), &mut request);
                let value = match returned {
                    Ok(RequestReturn::Value(v)) => {
                        assert!(!signed, "{label}: native synchronous return");
                        Ok(v)
                    }
                    Ok(RequestReturn::Future(v)) => {
                        assert!(signed, "{label}: native launched sign future");
                        settle(v)
                    }
                    Err(e) => Err(e),
                };
                assert_eq!(
                    value.is_err(),
                    req_failed,
                    "{label}: native request outcome"
                );
                if let Err(got) = &value {
                    original(got, &error, &label)
                } else {
                    let v = value.as_ref().unwrap();
                    assert_eq!(v.same(&input), listen, "{label}: native input referent");
                    assert!(
                        v.read("nested").unwrap().same(&nested),
                        "{label}: native nested referent"
                    );
                    assert_eq!(
                        v.read("headers").unwrap().same(&headers),
                        !public,
                        "{label}: native headers referent"
                    );
                    num(
                        &v.read("timeout").unwrap(),
                        if public { 20000.0 } else { 500.0 },
                        &format!("{label}: native request timeout"),
                    );
                    if signed {
                        boolean(&v.read("signed").unwrap(), true, &label)
                    } else {
                        boolean(&v.read("safeCexSkipRemoteSigning").unwrap(), true, &label)
                    }
                    if public {
                        assert!(
                            matches!(
                                v.read("headers").unwrap().read("private").unwrap(),
                                EventValue::Undefined
                            ),
                            "{label}: native private header removed"
                        );
                        text(
                            &v.read("headers").unwrap().read("Content-Type").unwrap(),
                            "application/json, charset=utf-8",
                            &label,
                        )
                    }
                }
                if signed {
                    assert_eq!(
                        request.sign_refs,
                        Some(true),
                        "{label}: native signer original inputs"
                    );
                    let (sign_name, window) = request.sign_receipt.as_ref().unwrap();
                    text(
                        sign_name,
                        if name == "live-options" {
                            "renamed"
                        } else {
                            stem(family)
                        },
                        &label,
                    );
                    if name == "recv-string" {
                        text(window, "2500", &label)
                    } else {
                        num(
                            window,
                            if name == "live-options" {
                                9876.0
                            } else if name == "recv-zero" {
                                0.0
                            } else {
                                20000.0
                            },
                            &label,
                        )
                    }
                }
                let expected_calls = if signed && rn == "sign-sync-fail" {
                    1
                } else if signed {
                    2
                } else if listen || rn == "headers-fail" {
                    1
                } else {
                    2
                };
                assert_eq!(
                    calls.borrow().0.len(),
                    expected_calls,
                    "{label}: native helper call count"
                );
                requests.push(json!({"name":rn,"calls":calls.borrow().0,"value":value.as_ref().map(wire).unwrap_or(json!({"$undefined":true})),"refs":value.as_ref().ok().map(|v|json!({"input":v.same(&input),"headers":v.read("headers").unwrap().same(&headers),"nested":v.read("nested").unwrap().same(&nested)})),"signRefs":request.sign_refs,"failure":value.as_ref().err().map(|e|error_wire(e,&error))}));
            }
            if let Some((success, None)) = &b.response {
                // PM/spot: the last interceptor only resends, so every response passes.
                for r in fixtures["responses"].as_array().unwrap() {
                    let rn = r["name"].as_str().unwrap();
                    let response = record([("data", decoded(r["data"].clone()))]);
                    let mut effects = Response { messages: vec![] };
                    let ResponseReturn::Value(v) = success(response.clone(), &mut effects).unwrap()
                    else {
                        panic!("native resend-only interceptor rejected {rn}");
                    };
                    assert!(v.same(&response), "native resend-only referent {rn}");
                    assert!(effects.messages.is_empty(), "native resend-only emits {rn}");
                    responses.push(json!({"name":rn,"which":"success","emitted":[],"value":wire(&v),"refs":true,"error":Value::Null}));
                }
            }
            if let Some((success, Some(failure))) = &b.response {
                for (j, r) in fixtures["responses"].as_array().unwrap().iter().enumerate() {
                    let rn = r["name"].as_str().unwrap();
                    let data = decoded(r["data"].clone());
                    let response = record([("data", data.clone())]);
                    let original_error = record([("response", response.clone())]);
                    for (which, handler, input) in [
                        ("success", success, response.clone()),
                        ("failure", failure, original_error.clone()),
                    ] {
                        let label = format!("native futures response case{j} {rn} {which}");
                        let mut effects = Response { messages: vec![] };
                        let value = handler(input, &mut effects).unwrap();
                        let negative = matches!(
                            rn,
                            "silent" | "string-silent" | "negative" | "fallback" | "message-object"
                        );
                        let rejected = which == "failure" || negative;
                        let emits = if which == "success" {
                            negative && rn != "silent"
                        } else {
                            matches!(
                                rn,
                                "string-silent" | "negative" | "nonnegative" | "message-object"
                            )
                        };
                        assert_eq!(
                            effects.messages.len(),
                            usize::from(emits),
                            "{label}: native emitted message count"
                        );
                        if emits {
                            let expected = if rn == "fallback" {
                                string("Binance Error: -1000")
                            } else {
                                data.read("msg").unwrap()
                            };
                            assert!(
                                effects.messages[0].same(&expected),
                                "{label}: native original emit message"
                            );
                        }
                        let (project_value, project_error, refs) = match value {
                            ResponseReturn::Value(v) => {
                                assert!(!rejected, "{label}: native resolved variant");
                                assert!(v.same(&response), "{label}: native response referent");
                                (wire(&v), Value::Null, json!(true))
                            }
                            ResponseReturn::Rejected(ApiRejection::Original(v)) => {
                                assert_eq!(which, "failure", "{label}");
                                assert!(
                                    v.same(&original_error),
                                    "{label}: native original rejection referent"
                                );
                                (
                                    json!({"$undefined":true}),
                                    json!({"original":true,"name":null,"message":null,"code":{"$undefined":true},"responseRef":true}),
                                    Value::Null,
                                )
                            }
                            ResponseReturn::Rejected(ApiRejection::New {
                                name,
                                message,
                                code,
                                response: got,
                                emitted,
                            }) => {
                                assert!(
                                    emitted,
                                    "{label}: native generated rejection carries the emit flag"
                                );
                                assert!(
                                    rejected && which == "success",
                                    "{label}: native generated rejection variant"
                                );
                                assert_eq!(name, "Error", "{label}");
                                let expected_msg = match rn {
                                    "silent" | "string-silent" => "unknown order",
                                    "negative" => "close position",
                                    "fallback" => "Binance Error: -1000",
                                    "message-object" => "[object Object]",
                                    _ => panic!(),
                                };
                                assert_eq!(message, expected_msg, "{label}: native Error message");
                                let input_code = data.read("code").unwrap();
                                assert!(
                                    code.same(&input_code),
                                    "{label}: native original error code"
                                );
                                if rn == "string-silent" {
                                    text(&code, "-2011", &label)
                                } else {
                                    num(
                                        &code,
                                        if rn == "silent" {
                                            -2011.0
                                        } else if rn == "negative" {
                                            -4130.0
                                        } else {
                                            -1000.0
                                        },
                                        &label,
                                    )
                                }
                                assert!(
                                    got.same(&response),
                                    "{label}: native original error response"
                                );
                                (
                                    json!({"$undefined":true}),
                                    json!({"original":false,"name":name,"message":message,"code":wire(&code),"responseRef":true}),
                                    Value::Null,
                                )
                            }
                        };
                        responses.push(json!({"name":rn,"which":which,"emitted":effects.messages.iter().map(wire).collect::<Vec<_>>(),"value":project_value,"refs":refs,"error":project_error}));
                    }
                }
            }
        }
        let retry = b.retry.as_ref().map(|(n, r)| {
            let path = if stem(family) == "binance" {
                "/fapi/v1/listenKey"
            } else {
                "/papi/v1/listenKey"
            };
            let query = format!("{path}?x=1");
            let probes = [
                (Some("GET"), Some("/private")),
                (Some("head"), Some("/private")),
                (Some("post"), Some(query.as_str())),
                (Some("put"), Some(path)),
                (Some("post"), Some("/private")),
                (None, Some(path)),
                (Some("delete"), Some("/private")),
            ]
            .map(|(m, u)| r.matches_read(m, u));
            assert_eq!(
                probes,
                [
                    true,
                    true,
                    family != "spot",
                    family != "spot",
                    false,
                    family != "spot",
                    false
                ],
                "{label}: native retry policy probes"
            );
            json!({"retries":n,"probes":probes})
        });
        actual.push(json!({"name":name,"trace":b.trace,"config":config_wire,"retry":retry,"refs":!failed,"requests":requests,"responses":responses,"failure":result.as_ref().err().map(|e|error_wire(e,&error))}));
    }
    // All decoded-native case assertions above complete before any source comparison.
    let source = oracle(family);
    let source = source.as_array().unwrap();
    assert_eq!(source.len(), 12, "source authored factory floor");
    for (i, (a, b)) in actual.iter().zip(source).enumerate() {
        assert!(
            numeric_equal(a, b),
            "source {family} factory case{i}: native {a} source {b}"
        );
    }
}
#[test]
#[ignore = "source oracle"]
fn futures_public() {
    factories("futures_public")
}
#[test]
#[ignore = "source oracle"]
fn futures() {
    factories("futures")
}
#[test]
#[ignore = "source oracle"]
fn pm() {
    factories("pm")
}
#[test]
#[ignore = "source oracle"]
fn papi() {
    factories("papi")
}
#[test]
#[ignore = "source oracle"]
fn fapi() {
    factories("fapi")
}
#[test]
#[ignore = "source oracle"]
fn pm_public() {
    factories("pm_public")
}
#[test]
#[ignore = "source oracle"]
fn spot() {
    factories("spot")
}
struct Auth {
    case: String,
    error: Failure,
    options: EventValue,
    result: EventValue,
    trace: Calls,
    request: Rc<RefCell<Option<(EventValue, EventValue)>>>,
    hmac: Option<(EventValue, String)>,
    options_reads: usize,
}
impl SpotAuthBindings for Auth {
    fn options(&mut self) -> Result<EventValue, Failure> {
        self.options_reads += 1;
        self.trace.borrow_mut().0.push(json!(["options"]));
        Ok(self.options.clone())
    }
    fn now(&mut self) -> Result<f64, Failure> {
        self.trace.borrow_mut().0.push(json!(["clock"]));
        if self.case == "clock-fail" {
            return Err(self.error.clone());
        }
        if self.case == "replace-options" {
            self.options = decoded(json!({"key":"key-B","secret":"secret-B"}))
        }
        Ok(1700000000123.0)
    }
    fn capture_remote_sign(&mut self, signer: EventValue) -> Result<RemoteSignCall, Failure> {
        self.trace.borrow_mut().0.push(json!(["capture_remote"]));
        let receipt = self.request.clone();
        let trace = self.trace.clone();
        let result = self.result.clone();
        let case = self.case.clone();
        let error = self.error.clone();
        Ok(Rc::new(move |request| {
            trace.borrow_mut().0.push(json!(["remote", wire(&request)]));
            *receipt.borrow_mut() = Some((signer.clone(), request));
            if case == "remote-sync-fail" {
                return Err(error.clone());
            }
            let trace = trace.clone();
            let result = result.clone();
            let error = error.clone();
            let fail = case == "remote-reject";
            Ok(Box::pin(async move {
                trace.borrow_mut().0.push(json!(["resolve"]));
                if fail { Err(error) } else { Ok(result) }
            }))
        }))
    }
    fn hmac_sha256_hex(
        &mut self,
        secret: EventValue,
        payload: String,
    ) -> Result<EventValue, Failure> {
        self.trace
            .borrow_mut()
            .0
            .push(json!(["hmac", "sha256", wire(&secret)]));
        self.hmac = Some((secret.clone(), payload.clone()));
        if self.case == "hmac-fail" {
            Err(self.error.clone())
        } else {
            utf8_hmac_sha256_hex(secret, payload)
        }
    }
}
#[test]
#[ignore = "source oracle"]
fn auth() {
    let fixture = fixtures();
    let cases = fixture["auth"].as_array().unwrap();
    assert_eq!(cases.len(), 13, "authored Spot auth floor");
    let mut actual = vec![];
    for (i, f) in cases.iter().enumerate() {
        let name = f["name"].as_str().unwrap();
        let label = format!("native Spot auth case{i} {name}");
        let options = decoded(json!({"key":"key-A","secret":"secret-A"}));
        let signer = record([("accountName", string("account-A"))]);
        let remote = name.starts_with("remote");
        if remote {
            options.write("remoteSigner", signer.clone())
        }
        let result = match name {
            "remote-error" => {
                decoded(json!({"error":"signer denied","wsAuthSignature":"remote-signature"}))
            }
            "remote-missing" => record([]),
            "remote-empty" => decoded(json!({"error":"","wsAuthSignature":""})),
            "remote-null" => EventValue::Null,
            "remote-number" => record([("wsAuthSignature", number(7.0))]),
            "remote-object" => record([("wsAuthSignature", record([("value", number(5.0))]))]),
            _ => record([("wsAuthSignature", string("remote-signature"))]),
        };
        let original_signature = if result.nullish() {
            EventValue::Undefined
        } else {
            result.read("wsAuthSignature").unwrap()
        };
        let error = failure();
        let trace = Rc::new(RefCell::new(Trace::default()));
        let mut b = Auth {
            case: name.into(),
            error: error.clone(),
            options,
            result,
            trace: trace.clone(),
            request: Rc::new(RefCell::new(None)),
            hmac: None,
            options_reads: 0,
        };
        let out = settle(build_spot_user_data_params(&mut b));
        let original_failure = matches!(
            name,
            "remote-reject" | "remote-sync-fail" | "clock-fail" | "hmac-fail"
        );
        let generated_failure = matches!(
            name,
            "remote-error" | "remote-missing" | "remote-empty" | "remote-null"
        );
        assert_eq!(
            out.is_err(),
            original_failure || generated_failure,
            "{label}: native outcome"
        );
        if let Err(got) = &out {
            if original_failure {
                original(got, &error, &label)
            } else {
                assert!(!Rc::ptr_eq(got, &error), "{label}: native generated Error");
                assert_eq!(
                    got.name,
                    if name == "remote-null" {
                        "TypeError"
                    } else {
                        "Error"
                    },
                    "{label}: native generated name"
                );
                assert_eq!(
                    got.message,
                    match name {
                        "remote-error" => "signer denied",
                        "remote-null" => "Cannot read properties of null (reading 'error')",
                        _ => "Remote signer returned no Binance Spot WebSocket signature",
                    },
                    "{label}: native generated message"
                );
                assert!(
                    got.code.is_none() && got.response_json_view().is_none(),
                    "{label}: native generated absent code/response"
                );
            }
        } else {
            let v = out.as_ref().unwrap();
            text(
                &v.read("apiKey").unwrap(),
                "key-A",
                &format!("{label}: native captured API key"),
            );
            num(
                &v.read("timestamp").unwrap(),
                1700000000123.0,
                &format!("{label}: native timestamp"),
            );
            num(
                &v.read("recvWindow").unwrap(),
                5000.0,
                &format!("{label}: native recvWindow"),
            );
            let sig = v.read("signature").unwrap();
            if remote {
                assert!(
                    sig.same(&original_signature),
                    "{label}: native original signature"
                );
                match name {
                    "remote-number" => num(&sig, 7.0, &label),
                    "remote-object" => num(&sig.read("value").unwrap(), 5.0, &label),
                    _ => text(&sig, "remote-signature", &label),
                }
            } else {
                text(
                    &sig,
                    if name == "replace-options" {
                        "f4ef9b6a46ede01c5efcd8197bc0ff7eab312eaf84ab75a0abcb3436432b546e"
                    } else {
                        "ad1c738443fc05ec6be7aaf0dfcf3751e7efc3d41e74eada2c157f9718b23f93"
                    },
                    &format!("{label}: native HMAC hex"),
                );
            }
        }
        let expected_reads = if name == "clock-fail" {
            1
        } else if remote {
            4
        } else {
            3
        };
        assert_eq!(
            b.options_reads, expected_reads,
            "{label}: native current options read count"
        );
        assert_eq!(
            b.request.borrow().is_some(),
            remote,
            "{label}: native remote call count"
        );
        assert_eq!(
            b.hmac.is_some(),
            !remote && name != "clock-fail",
            "{label}: native HMAC call count"
        );
        if let Some((got, request)) = &*b.request.borrow() {
            assert!(got.same(&signer), "{label}: native remote signer referent");
            text(&request.read("id").unwrap(), "tealstreet-user-data", &label);
            text(&request.read("type").unwrap(), "ws-auth", &label);
            text(
                &request.read("exchangeName").unwrap(),
                "binancespot",
                &label,
            );
            text(&request.read("accountName").unwrap(), "account-A", &label);
            let auth = request.read("wsAuth").unwrap();
            text(
                &auth.read("toSign").unwrap(),
                "apiKey=key-A&recvWindow=5000&timestamp=1700000000123",
                &format!("{label}: native toSign"),
            );
            text(&auth.read("outputFormat").unwrap(), "hex", &label);
            let extra = auth.read("extraData").unwrap();
            text(&extra.read("apiKey").unwrap(), "key-A", &label);
            num(&extra.read("recvWindow").unwrap(), 5000.0, &label);
            num(&extra.read("timestamp").unwrap(), 1700000000123.0, &label);
        }
        if let Some((secret, payload)) = &b.hmac {
            text(
                secret,
                if name == "replace-options" {
                    "secret-B"
                } else {
                    "secret-A"
                },
                &format!("{label}: native current secret"),
            );
            assert_eq!(
                payload, "apiKey=key-A&recvWindow=5000&timestamp=1700000000123",
                "{label}: native HMAC payload"
            );
        }
        let expected_effects = if name == "clock-fail" {
            2
        } else if remote {
            if name == "remote-sync-fail" { 7 } else { 8 }
        } else {
            5
        };
        assert_eq!(
            trace.borrow().0.len(),
            expected_effects,
            "{label}: native effect count"
        );
        actual.push(json!({"name":name,"trace":trace.borrow().0,"value":out.as_ref().map(wire).unwrap_or(json!({"$undefined":true})),"signatureRef":out.as_ref().ok().map(|v|v.read("signature").unwrap().same(&original_signature)),"failure":out.as_ref().err().map(|e|error_wire(e,&error))}));
    }
    let source = oracle("auth");
    let source = source.as_array().unwrap();
    assert_eq!(source.len(), 13, "source authored Spot auth floor");
    for (i, (a, b)) in actual.iter().zip(source).enumerate() {
        assert!(
            numeric_equal(a, b),
            "source Spot auth case{i}: native {a} source {b}"
        );
    }
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
