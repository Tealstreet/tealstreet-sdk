include!("support/trading_values.rs");
use safe_cex_venue_kraken::{
    api::{self, Api, ApiHost, Form, FuturesModule},
    trading::{self as trade, Client, ErrorHost, Method, TradingHost},
};
fn copy_raw(v: &Raw) -> Rc<Raw> {
    Rc::new(match v {
        Raw::Undefined => Raw::Undefined,
        Raw::Null => Raw::Null,
        Raw::Bool(b) => Raw::Bool(*b),
        Raw::Number(n) => Raw::Number(*n),
        Raw::Text(s) => Raw::Text(s.clone()),
        Raw::Array(a) => Raw::Array(a.clone()),
        Raw::Object(o) => Raw::Object(RefCell::new(o.borrow().clone())),
    })
}
impl ErrorHost for Host {
    fn is_string(&self, v: &Raw) -> bool {
        matches!(v, Raw::Text(_))
    }
    fn lower_method(&self, v: &Raw, expression: &str) -> Result<Rc<Raw>, E> {
        match v {
            Raw::Text(s) => Ok(self.text(&self.lower(s))),
            _ => Err(self.error_text(format!("{expression}.toLowerCase is not a function"))),
        }
    }
    fn upper_method(&self, v: &Raw, expression: &str) -> Result<Rc<Raw>, E> {
        match v {
            Raw::Text(s) => Ok(self.text(&self.upper(s))),
            _ => Err(self.error_text(format!("{expression}.toUpperCase is not a function"))),
        }
    }
    fn error_value(&self, e: &E) -> Result<Rc<Raw>, E> {
        let msg = self.string(&e.message)?;
        Ok(self.text(
            &units(e.name)
                .into_iter()
                .chain(if msg.is_empty() {
                    vec![]
                } else {
                    [units(": "), msg].concat()
                })
                .collect::<Vec<_>>(),
        ))
    }
    fn error_response(&self, e: &E) -> Result<Rc<Raw>, E> {
        Ok(e.response.clone().unwrap_or(Rc::new(Raw::Undefined)))
    }
    fn error_details(&self, e: &E) -> Result<Rc<Raw>, E> {
        Ok(e.response
            .as_ref()
            .map(|r| get(r, "data"))
            .unwrap_or(Rc::new(Raw::Undefined)))
    }
    fn new_error(&self, message: Rc<Raw>, response: Option<Rc<Raw>>, cause: Option<&E>) -> E {
        Rc::new(Error {
            message: self.text(&self.string(&message).unwrap()),
            response,
            cause: cause.cloned(),
            original: false,
            name: "Error",
        })
    }
    fn emit_and_log(&self, m: Rc<Raw>, context: Option<&str>) -> Result<(), E> {
        self.event(vec![
            ("kind", text("emit")),
            ("message", m),
            (
                "context",
                context.map(text).unwrap_or(Rc::new(Raw::Undefined)),
            ),
        ]);
        if flag(&self.cfg, "emitFail") {
            Err(self.original.clone())
        } else {
            Ok(())
        }
    }
}
impl Host {
    fn after_inputs(&self) {
        self.after();
        if let Raw::Object(values) = &*get(&self.cfg, "afterOrder") {
            let order = get(&get(&self.cfg, "input"), "order");
            for (k, v) in values.borrow().iter() {
                self.assign(&order, k, v.clone()).unwrap();
            }
        }
        let changes = get(&self.cfg, "afterInputs");
        if let Raw::Object(o) = &*changes {
            let input = get(&self.cfg, "input");
            for (k, v) in o.borrow().iter() {
                self.assign(&input, k, v.clone()).unwrap();
            }
        }
    }
    fn parent_future(
        &self,
        kind: &str,
        arg: Rc<Raw>,
    ) -> Result<impl Future<Output = Result<Rc<Raw>, E>>, E> {
        self.event(vec![("kind", text(kind)), ("argument", arg)]);
        let fail = flag(&self.cfg, &format!("{kind}Fail"));
        let result = get(&self.cfg, &format!("{kind}Result"));
        let first = self.calls.get() == 0;
        self.calls.set(self.calls.get() + 1);
        Ok(async move {
            if first {
                yield_once().await;
                self.after_inputs();
            }
            if fail {
                Err(self.original.clone())
            } else {
                Ok(if self.undefined(&result) {
                    self.array(vec![])
                } else {
                    result
                })
            }
        })
    }
}
impl TradingHost for Host {
    fn resolve_caller_id(&self, o: &Raw) -> Result<(), E> {
        self.event(vec![
            ("kind", text("resolve")),
            (
                "identity",
                self.boolean(std::ptr::eq(o, &*get(&self.cfg, "input"))),
            ),
        ]);
        if flag(&self.cfg, "resolveFail") {
            Err(self.original.clone())
        } else {
            Ok(())
        }
    }
    fn adjust(&self, v: &Raw, p: &Raw) -> Result<f64, E> {
        let a = self.numeric(v)?;
        self.event(vec![
            ("kind", text("adjust")),
            ("amount", copy_raw(v)),
            ("precision", copy_raw(p)),
        ]);
        if flag(&self.cfg, "adjustFail") {
            return Err(self.original.clone());
        }
        let step = match p {
            Raw::Text(t) => safe_cex_num::PrecisionInput::String(String::from_utf16(t).unwrap()),
            _ => safe_cex_num::PrecisionInput::Number(self.numeric(p)?),
        };
        Ok(safe_cex_num::adjust(a, step))
    }
    fn uuid(&self) -> Result<Vec<u16>, E> {
        self.event(vec![("kind", text("uuid"))]);
        Ok(units("uuid-A"))
    }
    fn spot_client_id(&self) -> Result<Rc<Raw>, E> {
        self.event(vec![("kind", text("spotId"))]);
        let id = get(&self.cfg, "clientId");
        Ok(if self.undefined(&id) {
            text("0123456789abcdef0123456789abcdef")
        } else {
            id
        })
    }
    fn iterable(&self, v: &Raw, expression: &str) -> Result<Vec<Rc<Raw>>, E> {
        match v {
            Raw::Array(a) => Ok(a.clone()),
            Raw::Text(s) => Ok(s.iter().map(|u| self.text(&[*u])).collect()),
            _ => Err(self.error_text(if expression == "txids" {
                "txids is not iterable".into()
            } else {
                "Spread syntax requires ...iterable[Symbol.iterator] to be a function".into()
            })),
        }
    }
    fn send(
        &self,
        spot: bool,
        client: Client,
        method: Method,
        endpoint: &str,
        body: Rc<Raw>,
    ) -> Result<impl Future<Output = Result<Rc<Raw>, E>>, E> {
        self.event(vec![
            ("kind", text("send")),
            ("spot", self.boolean(spot)),
            (
                "client",
                text(if client == Client::Unlimited {
                    "unlimited"
                } else {
                    "xhr"
                }),
            ),
            (
                "method",
                text(if method == Method::Post {
                    "post"
                } else {
                    "put"
                }),
            ),
            ("endpoint", text(endpoint)),
            ("body", body),
        ]);
        let first = self.calls.get() == 0;
        self.calls.set(self.calls.get() + 1);
        if flag(&self.cfg, "syncFail") {
            return Err(self.original.clone());
        }
        Ok(async move {
            if first {
                yield_once().await;
                self.after_inputs();
            }
            if flag(&self.cfg, "fail") {
                Err(self.original.clone())
            } else {
                Ok(get(&self.cfg, "data"))
            }
        })
    }
    fn derive(&self, options: Vec<Rc<Raw>>) -> Result<Rc<Raw>, E> {
        self.event(vec![
            ("kind", text("derive")),
            (
                "identity",
                self.boolean(Rc::ptr_eq(&options[0], &get(&self.cfg, "input"))),
            ),
        ]);
        if flag(&self.cfg, "deriveFail") {
            Err(self.original.clone())
        } else {
            Ok(text("derived"))
        }
    }
    fn place_orders(&self, o: Rc<Raw>) -> Result<impl Future<Output = Result<Rc<Raw>, E>>, E> {
        self.parent_future("placeOrders", o)
    }
    fn cancel_orders(&self, o: Vec<Rc<Raw>>) -> Result<impl Future<Output = Result<(), E>>, E> {
        let f = self.parent_future("cancelOrders", self.array(o))?;
        Ok(async move { f.await.map(|_| ()) })
    }
    fn place_order(&self, o: Rc<Raw>) -> Result<impl Future<Output = Result<Rc<Raw>, E>>, E> {
        self.parent_future("placeOrder", o)
    }
    fn finite_number(&self, v: &Raw) -> bool {
        matches!(v,Raw::Number(n) if n.is_finite())
    }
    fn set_leverage(&self, s: Rc<Raw>, v: Option<Rc<Raw>>) -> Result<(), E> {
        let key = self.string(&s)?;
        let mut rows = self.leverage.borrow_mut();
        let position = rows.iter().position(|(k, _)| *k == key);
        match (v, position) {
            (Some(v), Some(i)) => rows[i].1 = v,
            (Some(v), None) => rows.push((key, v)),
            (None, Some(i)) => {
                rows.remove(i);
            }
            (None, None) => {}
        }
        Ok(())
    }
    fn info(&self, m: Rc<Raw>) -> Result<(), E> {
        self.event(vec![
            ("kind", text("log")),
            ("message", m),
            ("severity", text("info")),
        ]);
        if flag(&self.cfg, "logFail") {
            Err(self.original.clone())
        } else {
            Ok(())
        }
    }
    fn fetch_positions(&self) -> Result<impl Future<Output = Result<Rc<Raw>, E>>, E> {
        self.parent_future("fetchPositions", Rc::new(Raw::Undefined))
    }
    fn add_positions(&self, p: Rc<Raw>) -> Result<(), E> {
        self.event(vec![("kind", text("addPositions")), ("positions", p)]);
        if flag(&self.cfg, "addFail") {
            Err(self.original.clone())
        } else {
            Ok(())
        }
    }
}
impl ApiHost for Host {
    fn create_client(
        &self,
        config: Rc<Raw>,
        options: Rc<Raw>,
        exchange: Rc<Raw>,
    ) -> Result<Rc<Raw>, E> {
        self.event(vec![
            ("kind", text("create")),
            ("config", config),
            (
                "optionsIdentity",
                self.boolean(Rc::ptr_eq(&options, &get(&self.cfg, "options"))),
            ),
            (
                "exchangeIdentity",
                self.boolean(Rc::ptr_eq(&exchange, &get(&self.cfg, "exchange"))),
            ),
        ]);
        if flag(&self.cfg, "createFail") {
            Err(self.original.clone())
        } else {
            self.object(vec![(units("id"), text("client"))])
        }
    }
    fn retry_condition(&self, posted: Option<Vec<&'static str>>) -> Result<Rc<Raw>, E> {
        self.event(vec![
            ("kind", text("network")),
            (
                "posted",
                posted
                    .map(|p| self.array(p.iter().map(|s| text(s)).collect()))
                    .unwrap_or(Rc::new(Raw::Undefined)),
            ),
        ]);
        Ok(text("condition"))
    }
    fn retry(&self, _client: Rc<Raw>, config: Rc<Raw>) -> Result<(), E> {
        self.event(vec![("kind", text("retry")), ("config", config)]);
        if flag(&self.cfg, "retryFail") {
            Err(self.original.clone())
        } else {
            Ok(())
        }
    }
    fn install_transport(
        &self,
        _client: Rc<Raw>,
        _options: Rc<Raw>,
        name: Rc<Raw>,
    ) -> Result<(), E> {
        self.event(vec![("kind", text("transport")), ("name", name)]);
        Ok(())
    }
    fn request_use(&self, client: Rc<Raw>, api: Rc<Api<Self>>) -> Result<(), E> {
        assert!(
            Rc::ptr_eq(&client, &api.client),
            "registered original native client"
        );
        self.event(vec![("kind", text("requestUse"))]);
        Ok(())
    }
    fn response_use(&self, client: Rc<Raw>, api: Rc<Api<Self>>) -> Result<(), E> {
        assert!(
            Rc::ptr_eq(&client, &api.client),
            "registered original native client"
        );
        self.event(vec![("kind", text("responseUse"))]);
        Ok(())
    }
    fn strip_slash(&self, v: &Raw) -> Result<Vec<u16>, E> {
        match v {
            Raw::Text(s) => Ok(if s.first() == Some(&47) {
                s[1..].into()
            } else {
                s.clone()
            }),
            _ => Err(self.error_text("(config.url || \"\").replace is not a function".into())),
        }
    }
    fn qs(&self, v: Rc<Raw>, form: Form) -> Result<Rc<Raw>, E> {
        self.event(vec![
            ("kind", text("qs")),
            ("value", v),
            (
                "form",
                text(if form == Form::Sorted1738 {
                    "sorted1738"
                } else {
                    "repeat"
                }),
            ),
        ]);
        if flag(&self.cfg, "qsFail") {
            Err(self.original.clone())
        } else {
            Ok(text("qs-result"))
        }
    }
    fn json_component(&self, v: Rc<Raw>) -> Result<Rc<Raw>, E> {
        self.event(vec![("kind", text("json")), ("value", v)]);
        self.event(vec![("kind", text("encode")), ("value", text("JSON"))]);
        Ok(text("ENCODED"))
    }
    fn sha256(&self, v: Rc<Raw>) -> Result<Rc<Raw>, E> {
        self.event(vec![("kind", text("sha256")), ("value", v)]);
        if flag(&self.cfg, "hashFail") {
            Err(self.original.clone())
        } else {
            Ok(text("HASH"))
        }
    }
    fn decode_secret(&self, v: Rc<Raw>) -> Result<Rc<Raw>, E> {
        self.event(vec![("kind", text("secret")), ("value", v)]);
        Ok(text("SECRET"))
    }
    fn path_hash(&self, path: Rc<Raw>, hash: Rc<Raw>) -> Result<Rc<Raw>, E> {
        self.event(vec![
            ("kind", text("pathHash")),
            ("path", path),
            ("hash", hash),
        ]);
        Ok(text("PATH_HASH"))
    }
    fn hmac512(&self, secret: Rc<Raw>, payload: Rc<Raw>) -> Result<Rc<Raw>, E> {
        self.event(vec![
            ("kind", text("hmac512")),
            ("secret", secret),
            ("payload", payload),
        ]);
        Ok(text("SIGNATURE"))
    }
    fn join(&self, v: &Raw, separator: &str) -> Result<Rc<Raw>, E> {
        match v {
            Raw::Array(a) => {
                let mut out = vec![];
                for (i, v) in a.iter().enumerate() {
                    if i > 0 {
                        out.extend(units(separator));
                    }
                    if !self.nullish(v) {
                        out.extend(self.string(v)?);
                    }
                }
                Ok(self.text(&out))
            }
            _ => Err(self.error_text("data.error.join is not a function".into())),
        }
    }
    fn mark_skip(&self, _config: Rc<Raw>) -> Result<(), E> {
        self.event(vec![("kind", text("skip"))]);
        Ok(())
    }
}
async fn yield_once() {
    let mut yielded = false;
    std::future::poll_fn(move |_| {
        if yielded {
            Poll::Ready(())
        } else {
            yielded = true;
            Poll::Pending
        }
    })
    .await;
}
fn complete<F: Future>(f: F, h: &Host, prefix: usize) -> F::Output {
    let mut f = std::pin::pin!(f);
    let mut cx = Context::from_waker(Waker::noop());
    let first = f.as_mut().poll(&mut cx);
    assert_eq!(
        h.events.borrow().len(),
        prefix,
        "source prefix before await"
    );
    match first {
        Poll::Ready(v) => v,
        Poll::Pending => match f.as_mut().poll(&mut cx) {
            Poll::Ready(v) => v,
            Poll::Pending => panic!("released first await"),
        },
    }
}
fn oracle() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        let output = Command::new("node")
            .env("TZ", "UTC")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/trading-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        {
            let v: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(v["header"]["hashes"].as_object().unwrap().len(), 5);
            assert_eq!(v["header"]["tz"], "UTC");
            v
        }
    })
}
fn native(actual: &Raw, expected: &Value, label: &str) {
    match actual {
        Raw::Undefined => assert_eq!(expected["undefined"], true, "{label}: native undefined"),
        Raw::Null => assert!(expected.is_null(), "{label}: native null"),
        Raw::Bool(b) => assert_eq!(expected.as_bool(), Some(*b), "{label}: native boolean"),
        Raw::Text(s) => assert_eq!(
            expected.as_str().map(units),
            Some(s.clone()),
            "{label}: native UTF16"
        ),
        Raw::Number(n) => {
            let bits = expected["number"]
                .as_str()
                .unwrap_or_else(|| panic!("{label}: numeric {expected}"));
            let expected = f64::from_bits(u64::from_str_radix(bits, 16).unwrap());
            if expected.is_nan() {
                assert!(n.is_nan(), "{label}: native NaN");
            } else {
                assert_eq!(n.to_bits(), expected.to_bits(), "{label}: native bits");
            }
        }
        Raw::Array(a) => {
            let e = expected
                .as_array()
                .unwrap_or_else(|| panic!("{label}: expected array {expected}"));
            assert_eq!(a.len(), e.len(), "{label}: native length");
            for (i, (a, e)) in a.iter().zip(e).enumerate() {
                native(a, e, &format!("{label}[{i}]"));
            }
        }
        Raw::Object(o) => {
            let o = o.borrow();
            let e = expected
                .as_object()
                .unwrap_or_else(|| panic!("{label}: expected object {expected}"));
            assert_eq!(
                o.iter()
                    .map(|(k, _)| String::from_utf16(k).unwrap())
                    .collect::<Vec<_>>(),
                e.keys().cloned().collect::<Vec<_>>(),
                "{label}: native own-key order"
            );
            for (k, v) in o.iter() {
                let k = String::from_utf16(k).unwrap();
                native(v, &e[&k], &format!("{label}.{k}"));
            }
        }
    }
}
fn assert_result(
    h: &Host,
    result: Result<Rc<Raw>, E>,
    case: &Value,
    label: &str,
    response: Option<Rc<Raw>>,
) {
    match result {
        Ok(v) => {
            assert!(case["error"].is_null(), "{label}: source threw");
            native(&v, &case["result"], label);
            if case["identity"] == true {
                assert!(
                    Rc::ptr_eq(&v, &response.unwrap()),
                    "{label}: original native object identity"
                );
            }
        }
        Err(e) => {
            let err = &case["error"];
            assert!(err.is_object(), "{label}: unexpected {e:?}");
            native(
                &e.message,
                &err["message"],
                &format!("{label}: error message"),
            );
            assert_eq!(
                Rc::ptr_eq(&e, &h.original),
                err["identity"].as_bool().unwrap(),
                "{label}: original error identity"
            );
            assert_eq!(e.name, err["name"].as_str().unwrap(), "{label}: error name");
            if let Some(response) = response {
                assert_eq!(
                    e.response
                        .as_ref()
                        .is_some_and(|r| Rc::ptr_eq(r, &response)),
                    err["responseIdentity"].as_bool().unwrap(),
                    "{label}: original response identity"
                );
                assert_eq!(
                    e.cause.as_ref().is_some_and(|c| Rc::ptr_eq(c, &h.original)),
                    err["causeIdentity"].as_bool().unwrap(),
                    "{label}: original cause identity"
                );
            }
        }
    }
}
fn trading_family(family: &str, floor: usize) {
    let cases = oracle()["trading"][family].as_array().unwrap();
    assert_eq!(cases.len(), floor, "independent finite floor");
    for (i, case) in cases.iter().enumerate() {
        let h = Host::new(&case["cfg"]);
        let input = get(&h.cfg, "input");
        let prefix = case["prefix"].as_u64().unwrap() as usize;
        let result = match family {
            "futuresPlace" => complete(trade::futures_place(&h, input), &h, prefix),
            "spotPlace" => complete(trade::spot_place(&h, input), &h, prefix),
            "futuresUpdate" => complete(trade::futures_update(&h, &input), &h, prefix),
            "spotUpdate" => complete(trade::spot_update(&h, &input), &h, prefix),
            "futuresCancel" => complete(trade::futures_cancel(&h, &input), &h, prefix)
                .map(|()| Rc::new(Raw::Undefined)),
            "leverage" => complete(
                trade::futures_set_leverage(&h, get(&input, "symbol"), &get(&input, "leverage")),
                &h,
                prefix,
            )
            .map(|()| Rc::new(Raw::Undefined)),
            "margin" => complete(
                trade::futures_margin(&h, &get(&input, "isolated"), get(&input, "symbol")),
                &h,
                prefix,
            )
            .map(|()| Rc::new(Raw::Undefined)),
            _ => unreachable!(),
        };
        let label = format!("{family}/{i}");
        assert_result(&h, result, case, &label, None);
        native(
            &Raw::Array(h.events.borrow().clone()),
            &case["events"],
            &format!("{label}/events"),
        );
        native(
            &Raw::Object(RefCell::new(h.margin.borrow().clone())),
            &case["margin"],
            &format!("{label}/margin"),
        );
        native(
            &Raw::Object(RefCell::new(h.leverage.borrow().clone())),
            &case["leverage"],
            &format!("{label}/leverage"),
        );
    }
}
fn api_family(spot: bool, floor: usize) {
    let family = if spot { "spotApi" } else { "futuresApi" };
    let cases = oracle()["api"][family].as_array().unwrap();
    assert_eq!(cases.len(), floor, "independent API floor");
    for (i, case) in cases.iter().enumerate() {
        let h = Rc::new(Host::new(&case["cfg"]));
        let options = get(&h.cfg, "options");
        let exchange = get(&h.cfg, "exchange");
        let count = h.numeric(&get(&h.cfg, "clients")).unwrap() as usize;
        let module = Rc::new(FuturesModule::with_counter(
            if h.undefined(&get(&h.cfg, "counter")) {
                0.0
            } else {
                h.numeric(&get(&h.cfg, "counter")).unwrap()
            },
        ));
        let mut clients = vec![];
        let label = format!("{family}/{i}");
        for _ in 0..count {
            let api = if spot {
                api::spot_create_api(h.clone(), options.clone(), exchange.clone())
            } else {
                api::futures_create_api(
                    h.clone(),
                    options.clone(),
                    exchange.clone(),
                    &get(&h.cfg, "baseKind"),
                    module.clone(),
                )
            };
            match api {
                Ok(a) => clients.push(a),
                Err(e) => {
                    assert_result(&h, Err(e), case, &label, None);
                    break;
                }
            }
        }
        for (j, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            if step["options"].is_object() {
                let values = decode(&step["options"]);
                if let Raw::Object(values) = &*values {
                    for (k, v) in values.borrow().iter() {
                        h.assign(&options, k, v.clone()).unwrap();
                    }
                }
            }
            let cfg = decode(&step["input"]);
            let selected = step["client"].as_u64().unwrap() as usize;
            let api = &clients[selected];
            let (result, original) = match step["kind"].as_str().unwrap() {
                "request" => (api.request(cfg.clone()), Some(cfg.clone())),
                "success" => (
                    complete(api.success(cfg.clone()), &h, h.events.borrow().len()),
                    Some(cfg.clone()),
                ),
                "rejected" => {
                    let response = h.original.response.clone().unwrap_or(cfg.clone());
                    (
                        complete(
                            api.rejected(h.original.clone()),
                            &h,
                            h.events.borrow().len(),
                        ),
                        Some(response),
                    )
                }
                _ => unreachable!(),
            };
            assert_result(&h, result, step, &format!("{label}/{j}"), original);
            native(
                &cfg,
                &step["after"],
                &format!("{label}/{j}: native input mutations"),
            );
            if !spot {
                assert_eq!(
                    api.counter().to_bits(),
                    f64::from_bits(
                        u64::from_str_radix(step["counter"]["number"].as_str().unwrap(), 16)
                            .unwrap()
                    )
                    .to_bits(),
                    "{label}: native counter"
                );
            };
        }
        native(
            &Raw::Array(h.events.borrow().clone()),
            &case["events"],
            &format!("{label}/events"),
        );
    }
}
#[test]
fn futures_place_native() {
    trading_family("futuresPlace", 37);
}
#[test]
fn spot_place_native() {
    trading_family("spotPlace", 29);
}
#[test]
fn futures_update_native() {
    trading_family("futuresUpdate", 21);
}
#[test]
fn spot_update_native() {
    trading_family("spotUpdate", 24);
}
#[test]
fn futures_cancel_native() {
    trading_family("futuresCancel", 12);
}
#[test]
fn leverage_native() {
    trading_family("leverage", 18);
}
#[test]
fn margin_native() {
    trading_family("margin", 20);
}
#[test]
fn futures_api_native() {
    api_family(false, 33);
}
#[test]
fn spot_api_native() {
    api_family(true, 34);
}
