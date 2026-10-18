use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::JsJsonValue as J;
use safe_cex_types::{OrderSide, OrderStopTriggerType, OrderTimeInForce, OrderType};
use safe_cex_venue_backpack::native_value::NativeValue;
use safe_cex_venue_backpack::{api_factory as api, order_actions as a, public_listeners as l};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    process::Command,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn ready<F: Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("fixture must resolve"),
    }
}
fn corpus() -> &'static Value {
    static C: OnceLock<Value> = OnceLock::new();
    C.get_or_init(|| {
        let out = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/adapter_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let rows: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(
            rows.as_array().unwrap().len(),
            26,
            "ADAPTER independent corpus SIZE"
        );
        let families = [("place", 8), ("update", 4), ("hash", 4), ("api", 4)];
        let mut at = 0;
        for (name, count) in families {
            for i in 0..count {
                assert_eq!(
                    rows[at]["family"], name,
                    "ADAPTER independent corpus CONTENT {at}"
                );
                assert_eq!(
                    rows[at]["i"], i,
                    "ADAPTER independent corpus CONTENT index {at}"
                );
                at += 1;
            }
        }
        for i in 0..3 {
            for name in ["candle", "depth"] {
                assert_eq!(
                    rows[at]["family"], name,
                    "ADAPTER independent corpus CONTENT {at}"
                );
                assert_eq!(rows[at]["i"], i);
                at += 1;
            }
        }
        rows
    })
}
fn row(f: &str, i: usize) -> &'static Value {
    corpus()
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["family"] == f && r["i"] == i)
        .unwrap()
}
fn number(n: f64, v: &Value, name: &str) {
    let bits = if n.is_nan() {
        f64::NAN.to_bits()
    } else {
        n.to_bits()
    };
    assert_eq!(
        format!("{bits:016x}"),
        v["n"].as_str().unwrap(),
        "{name} native numeric bits"
    )
}
fn string(s: &[u16], v: &Value, name: &str) {
    assert_eq!(
        s,
        serde_json::from_value::<Vec<u16>>(v["s"].clone()).unwrap(),
        "{name} native UTF16"
    )
}
fn native(v: &J, e: &Value, name: &str) {
    match v {
        J::Undefined => assert_eq!(e, &json!({"u":true}), "{name}"),
        J::Null => assert!(e.is_null()),
        J::Bool(b) => assert_eq!(e, &json!(b)),
        J::Number(n) => number(*n, e, name),
        J::String(s) => string(&u(s), e, name),
        J::Array(a) => {
            assert_eq!(
                a.len(),
                e.as_array().unwrap().len(),
                "{name} native array size"
            );
            for (v, e) in a.iter().zip(e.as_array().unwrap()) {
                native(v, e, name)
            }
        }
        J::Object(o) => {
            assert_eq!(
                o.len(),
                e.as_object().unwrap().len(),
                "{name} native own field count"
            );
            for (k, v) in o {
                native(v, &e[k], name)
            }
        }
        J::BigInt(_) => panic!("unadmitted oracle BigInt"),
    }
}
fn options(i: usize) -> a::Options {
    let mut o = a::Options {
        symbol: u("BTC_USDC"),
        r#type: safe_cex_types::ModelField::Value(OrderType::Limit),
        side: safe_cex_types::ModelField::Value(OrderSide::Buy),
        amount: 1.375,
        price: None,
        trigger_price: None,
        take_profit: None,
        stop_loss: None,
        time_in_force: None,
        reduce_only: false.into(),
        tp_trigger_type: None,
        sl_trigger_type: None,
        client_order_id: None,
        ..Default::default()
    };
    match i {
        0 => {
            o.price = Some(12.375);
            o.time_in_force = Some(OrderTimeInForce::PostOnly);
            o.client_order_id = Some(u("12"));
        }
        1 => o.r#type = OrderType::Market.into(),
        2 => {
            o.r#type = OrderType::StopLoss.into();
            o.trigger_price = Some(10.125);
        }
        3 => {
            o.r#type = OrderType::TakeProfit.into();
            o.price = Some(11.75);
        }
        4 | 6 => {
            o.price = Some(12.0);
            o.take_profit = Some(15.0);
            o.stop_loss = Some(8.0);
        }
        5 => {
            o.r#type = OrderType::Market.into();
            o.take_profit = Some(15.0);
            o.stop_loss = Some(8.0);
            o.reduce_only = true.into();
        }
        7 => o.price = Some(12.0),
        _ => panic!("independent case index"),
    };
    o
}
struct Orders {
    case: usize,
    trace: Vec<String>,
    requests: Vec<(String, a::Request)>,
    logs: Vec<(Vec<u16>, a::Severity)>,
    old: Rc<RefCell<a::Options>>,
    placed: Vec<a::Options>,
}
impl Orders {
    fn new(i: usize) -> Self {
        let mut old = options(1);
        old.r#type = (if i == 1 {
            OrderType::StopLoss
        } else {
            OrderType::Limit
        })
        .into();
        old.side = safe_cex_types::ModelField::Value(OrderSide::Sell);
        old.amount = 2.0;
        old.price = Some(9.0);
        old.reduce_only = true.into();
        old.time_in_force = Some(OrderTimeInForce::ImmediateOrCancel);
        Self {
            case: i,
            trace: vec![],
            requests: vec![],
            logs: vec![],
            old: Rc::new(RefCell::new(old)),
            placed: vec![],
        }
    }
}
impl a::Host for Orders {
    type Error = ExchangeError;
    type Market = ();
    type Existing = Rc<RefCell<a::Options>>;
    type Target = ();
    fn resolve_caller_id(&mut self, o: &a::Options) -> Result<Option<Vec<u16>>, ExchangeError> {
        self.trace.push("resolve".into());
        Ok(o.client_order_id.clone())
    }
    fn market(&mut self, _: &[u16]) -> Result<Option<()>, ExchangeError> {
        Ok(Some(()))
    }
    fn market_id(&mut self, _: &()) -> Result<Vec<u16>, ExchangeError> {
        Ok(u("BTC_USDC"))
    }
    fn precision(&mut self, _: &(), price: bool) -> Result<f64, ExchangeError> {
        Ok(if price { 0.125 } else { 0.25 })
    }
    fn trigger_by(
        &mut self,
        tp: bool,
        _: Option<OrderStopTriggerType>,
    ) -> Result<Vec<u16>, ExchangeError> {
        self.trace
            .push(format!("by:{}", if tp { "tp" } else { "sl" }));
        Ok(u(if tp { "MarkPrice" } else { "LastPrice" }))
    }
    fn post(
        &mut self,
        url: &str,
        r: a::Request,
    ) -> Result<a::Task<Vec<u16>, ExchangeError>, ExchangeError> {
        self.trace.push("post".into());
        self.requests.push((url.into(), r));
        let n = self.requests.len();
        Ok(Box::pin(std::future::ready(
            if self.case == 7 || self.case == 6 && n == 2 {
                Err(ExchangeError::new(if self.case == 7 {
                    "post"
                } else {
                    "supplement"
                }))
            } else {
                Ok(u(&format!("id{n}")))
            },
        )))
    }
    fn log_target(&mut self) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn log(&mut self, _: (), s: Vec<u16>, v: a::Severity) -> Result<(), ExchangeError> {
        self.logs.push((s, v));
        Ok(())
    }
    fn error_text(&mut self, e: &ExchangeError) -> Result<Vec<u16>, ExchangeError> {
        Ok(u(&e.js_error_string()))
    }
    fn error(&mut self, s: Vec<u16>, range: bool) -> ExchangeError {
        let mut e = ExchangeError::new(String::from_utf16_lossy(&s));
        if range {
            e.name = "RangeError".into();
        }
        e
    }
    fn find_order(&mut self, _: &[u16]) -> Result<Option<Self::Existing>, ExchangeError> {
        Ok((self.case != 2).then(|| self.old.clone()))
    }
    fn order_options(&mut self, o: &Self::Existing) -> Result<a::Options, ExchangeError> {
        Ok(o.borrow().clone())
    }
    fn order_id(&mut self, _: &Self::Existing) -> Result<Vec<u16>, ExchangeError> {
        Ok(u("old"))
    }
    fn cancel(&mut self, o: Self::Existing) -> Result<a::Task<(), ExchangeError>, ExchangeError> {
        self.trace.push("cancel".into());
        let fail = self.case == 3;
        Ok(Box::pin(async move {
            if fail {
                return Err(ExchangeError::new("cancel"));
            }
            o.borrow_mut().amount = 7.0;
            Ok(())
        }))
    }
    fn place(
        &mut self,
        o: a::Options,
    ) -> Result<a::Task<Vec<Vec<u16>>, ExchangeError>, ExchangeError> {
        self.trace.push("replace".into());
        self.placed.push(o);
        Ok(Box::pin(std::future::ready(Ok(vec![u("replacement")]))))
    }
}
fn logs(h: &Orders, r: &Value) {
    assert_eq!(h.logs.len(), r["logs"].as_array().unwrap().len());
    for ((s, severity), e) in h.logs.iter().zip(r["logs"].as_array().unwrap()) {
        string(s, &e[0], "ORDER logs");
        string(
            &u(match severity {
                a::Severity::Info => "info",
                a::Severity::Warning => "warning",
                a::Severity::Error => "error",
            }),
            &e[1],
            "ORDER severity",
        )
    }
}
#[test]
fn placement_native_source_requests() {
    for i in 0..8 {
        let r = row("place", i);
        let o = options(i);
        let type_name = serde_json::to_value(o.r#type).unwrap();
        string(
            &u(type_name.as_str().unwrap()),
            &r["input"]["type"],
            "PLACE corpus input type",
        );
        number(o.amount, &r["input"]["amount"], "PLACE corpus input amount");
        let h = Rc::new(RefCell::new(Orders::new(i)));
        let f = a::place_order(h.clone(), o);
        assert_eq!(
            h.borrow().trace,
            json_strings(&r["before"]),
            "PLACE eager prefix {i}"
        );
        let result = ready(f);
        let h = h.borrow();
        assert_eq!(
            h.requests.len(),
            [1, 1, 1, 1, 3, 1, 3, 1][i],
            "PLACE independent native requests count {i}"
        );
        for ((url, request), e) in h.requests.iter().zip(r["requests"].as_array().unwrap()) {
            string(&u(url), &e[0], "PLACE native endpoint");
            assert_eq!(
                request.len(),
                e[1].as_object().unwrap().len(),
                "PLACE native field count {i}"
            );
            for (k, v) in request {
                match v {
                    a::Field::Text(s) => string(s, &e[1][k], "PLACE native request text"),
                    a::Field::Number(n) => number(*n, &e[1][k], "PLACE native request number"),
                    a::Field::Bool(b) => {
                        assert_eq!(Some(*b), e[1][k].as_bool(), "PLACE native request bool")
                    }
                }
            }
        }
        match result {
            Ok(ids) => {
                assert_eq!(
                    ids.len(),
                    [1, 1, 1, 1, 3, 1, 2, 0][i],
                    "PLACE independent result size"
                );
                for (s, e) in ids.iter().zip(r["result"].as_array().unwrap()) {
                    string(s, e, "PLACE native result")
                }
            }
            Err(e) => assert_eq!(
                e.message,
                r["error"].as_str().unwrap(),
                "PLACE native error"
            ),
        };
        logs(&h, r);
        assert_eq!(h.trace, json_strings(&r["trace"]));
    }
}
fn json_strings(v: &Value) -> Vec<String> {
    serde_json::from_value(v.clone()).unwrap()
}
#[test]
fn replacement_native_source_options() {
    for i in 0..4 {
        let r = row("update", i);
        let h = Rc::new(RefCell::new(Orders::new(i)));
        let f = a::update_order(
            h.clone(),
            a::Update {
                id: u("old"),
                amount: Some(Some(3.25)),
                price: None,
            },
        );
        assert_eq!(h.borrow().trace, json_strings(&r["before"]));
        let result = ready(f);
        let h = h.borrow();
        assert_eq!(
            h.placed.len(),
            [1, 1, 0, 0][i],
            "REPLACE independent count {i}"
        );
        if i < 2 {
            let o = &h.placed[0];
            let e = &r["placed"][0];
            number(o.amount, &e["amount"], "REPLACE native amount");
            number(o.price.unwrap(), &e["price"], "REPLACE native price");
            string(&o.symbol, &e["symbol"], "REPLACE native symbol");
            assert_eq!(
                o.r#type,
                if i == 1 {
                    OrderType::StopLoss
                } else {
                    OrderType::Limit
                },
                "REPLACE native type"
            );
            assert_eq!(o.side, OrderSide::Sell);
            assert_eq!(o.reduce_only, true);
            assert!(o.trigger_price.is_none());
            string(
                &result.unwrap()[0],
                &r["result"][0],
                "REPLACE native result",
            );
        } else {
            assert_eq!(result.unwrap_err().message, r["error"].as_str().unwrap());
        }
        logs(&h, r);
        assert_eq!(h.trace, json_strings(&r["trace"]));
    }
}
#[test]
fn sha512_native_source_bytes() {
    for i in 0..4 {
        let bytes = [
            vec![],
            b"abc".to_vec(),
            vec![0, 255, 1, 254],
            (0..129).map(|n| n as u8).collect(),
        ][i]
            .clone();
        let r = row("hash", i);
        assert_eq!(
            bytes,
            serde_json::from_value::<Vec<u8>>(r["input"].clone()).unwrap(),
            "HASH independent input CONTENT {i}"
        );
        let hash = safe_cex_venue_backpack::signing::sha512_for_ed25519(&bytes);
        assert_eq!(hash.len(), 64, "HASH independent output SIZE {i}");
        assert_eq!(
            hash,
            serde_json::from_value::<Vec<u8>>(r["digest"].clone()).unwrap(),
            "HASH native output bytes {i}"
        );
    }
}
struct Api {
    build: Vec<Value>,
    config: Option<api::ClientConfig<ExchangeError>>,
    request: Option<api::RequestHandler<ExchangeError>>,
    success: Option<api::ResponseHandler<ExchangeError>>,
    failure: Option<api::FailureHandler<ExchangeError>>,
    calls: Vec<J>,
    events: Vec<(String, String)>,
    response: Option<Rc<api::Response>>,
    error_config: Option<Rc<api::Config>>,
}
impl api::Host for Api {
    type Error = ExchangeError;
    type Client = ();
    type RetryCondition = ();
    fn create(
        &mut self,
        c: api::ClientConfig<ExchangeError>,
        _: Rc<RefCell<api::Options>>,
    ) -> Result<(), ExchangeError> {
        self.config = Some(c);
        self.build.push(json!(["create"]));
        Ok(())
    }
    fn qs_repeat(&mut self, p: J) -> Result<String, ExchangeError> {
        let J::Object(o) = p else {
            return Err(ExchangeError::new("Record needed"));
        };
        let mut v = vec![];
        for (k, p) in o {
            let values = if let J::Array(a) = p { a } else { vec![p] };
            for p in values {
                v.push(format!(
                    "{k}={}",
                    safe_cex_venue_backpack::native_value::Value::from_prepared(p).utf8()?
                ));
            }
        }
        Ok(v.join("&"))
    }
    fn read_retry_condition(&mut self) -> Result<(), ExchangeError> {
        self.build.push(json!(["condition"]));
        Ok(())
    }
    fn retry(&mut self, _: &(), n: u32, _: ()) -> Result<(), ExchangeError> {
        self.build.push(json!(["retry", n]));
        Ok(())
    }
    fn install_transport(
        &mut self,
        _: &(),
        _: Rc<RefCell<api::Options>>,
    ) -> Result<(), ExchangeError> {
        self.build.push(json!(["transport"]));
        Ok(())
    }
    fn register_request(
        &mut self,
        _: &(),
        h: api::RequestHandler<ExchangeError>,
    ) -> Result<(), ExchangeError> {
        self.request = Some(h);
        self.build.push(json!(["request"]));
        Ok(())
    }
    fn register_response(
        &mut self,
        _: &(),
        s: api::ResponseHandler<ExchangeError>,
        f: api::FailureHandler<ExchangeError>,
    ) -> Result<(), ExchangeError> {
        self.success = Some(s);
        self.failure = Some(f);
        self.build.push(json!(["response"]));
        Ok(())
    }
    fn auth(
        &mut self,
        inst: &str,
        p: J,
        key: String,
        secret: String,
        window: f64,
    ) -> Result<api::Task<Vec<(String, J)>, ExchangeError>, ExchangeError> {
        self.calls.push(J::Array(vec![
            J::String(inst.into()),
            p,
            J::String(key),
            J::String(secret),
            J::Number(window),
        ]));
        Ok(Box::pin(std::future::ready(Ok(vec![(
            "X-Test".into(),
            J::String("signed".into()),
        )]))))
    }
    fn error_text(&mut self, e: &ExchangeError) -> Result<String, ExchangeError> {
        Ok(e.js_error_string())
    }
    fn error_message(&mut self, e: &ExchangeError) -> Result<J, ExchangeError> {
        Ok(J::String(e.message.clone()))
    }
    fn response(&mut self, _: &ExchangeError) -> Result<Option<Rc<api::Response>>, ExchangeError> {
        Ok(self.response.clone())
    }
    fn error_config(
        &mut self,
        _: &ExchangeError,
    ) -> Result<Option<Rc<api::Config>>, ExchangeError> {
        Ok(self.error_config.clone())
    }
    fn emit_and_log(&mut self, m: String) -> Result<(), ExchangeError> {
        self.events.push(("log".into(), m));
        Ok(())
    }
    fn emit_error(&mut self, e: String) -> Result<(), ExchangeError> {
        self.events.push(("error".into(), e));
        Ok(())
    }
    fn append_http_failure(
        &mut self,
        m: String,
        _: Rc<api::Response>,
    ) -> Result<String, ExchangeError> {
        Ok(m + "|http")
    }
    fn make_error(&mut self, m: String) -> ExchangeError {
        ExchangeError::new(m)
    }
    fn coercion_error(&mut self, e: safe_cex_num::JsCoercionError) -> ExchangeError {
        let mut v = ExchangeError::new(e.message);
        v.name = e.name.into();
        v
    }
}
fn config(url: &str) -> Rc<api::Config> {
    Rc::new(api::Config {
        url: Some(url.into()),
        method: Some("post".into()),
        params: Rc::new(J::Object(vec![("q".into(), J::Number(2.0))])),
        data: Rc::new(J::Object(vec![("p".into(), J::Number(3.0))])),
        headers: Some(Rc::new(RefCell::new(vec![]))),
        skip_remote_signing: false,
        extra: vec![],
    })
}
#[test]
fn api_factory_native_configuration_and_hooks() {
    for i in 0..4 {
        let r = row("api", i);
        let h = Rc::new(RefCell::new(Api {
            build: vec![],
            config: None,
            request: None,
            success: None,
            failure: None,
            calls: vec![],
            events: vec![],
            response: None,
            error_config: None,
        }));
        let mut options: api::Options = serde_json::from_value(json!({
            "key":"key", "secret":"secret", "testnet":i == 1, "tealstreetTunnel":i == 2,
        }))
        .unwrap();
        options.extra = [None, Some(0.0), Some(100000.0), Some(-3.0)][i]
            .map(|window| serde_json::from_value(json!({"recvWindow":window})).unwrap());
        let opts = Rc::new(RefCell::new(options));
        api::create_api(h.clone(), opts).unwrap();
        {
            let h = h.borrow();
            let c = h.config.as_ref().unwrap();
            number(c.timeout, &r["config"]["timeout"], "API native timeout");
            string(
                &u(c.base_url),
                &r["config"]["baseURL"],
                "API native baseURL",
            );
            for (k, v) in c.headers {
                string(&u(v), &r["config"]["headers"][k], "API native header");
            }
            assert_eq!(
                h.build,
                serde_json::from_value::<Vec<Value>>(r["build"].clone()).unwrap(),
                "API native setup order"
            );
        }
        let (req, success, failure) = {
            let h = h.borrow();
            (
                h.request.clone().unwrap(),
                h.success.clone().unwrap(),
                h.failure.clone().unwrap(),
            )
        };
        let original = config(if i == 0 {
            "/api/v1/markets"
        } else {
            "/api/v1/order"
        });
        let next = ready(req(original.clone())).unwrap();
        assert!(!Rc::ptr_eq(&original, &next), "API native shallow clone");
        assert!(
            Rc::ptr_eq(
                original.headers.as_ref().unwrap(),
                next.headers.as_ref().unwrap()
            ),
            "API native header identity"
        );
        assert_eq!(next.skip_remote_signing, i == 0, "API native skip flag");
        assert_eq!(
            h.borrow().calls.len(),
            if i == 0 { 0 } else { 1 },
            "API independent signing count"
        );
        native(
            &J::Array(h.borrow().calls.clone()),
            &r["authCalls"],
            "API native auth arguments",
        );
        native(
            &J::Object(next.headers.as_ref().unwrap().borrow().clone()),
            &r["headers"],
            "API native auth headers",
        );
        let result = match i {
            0 => {
                let response = Rc::new(api::Response {
                    data: J::Object(vec![("okay".into(), J::Number(1.0))]),
                    config: None,
                    status: None,
                });
                let result = ready(success(response.clone())).unwrap();
                assert!(
                    Rc::ptr_eq(&response, &result),
                    "API native response identity"
                );
                Ok(result)
            }
            1 => ready(success(Rc::new(api::Response {
                data: json!({"error":{"msg":"bad"}}).into(),
                config: None,
                status: None,
            }))),
            2 => {
                h.borrow_mut().response = Some(Rc::new(api::Response {
                    data: json!({"message":"bad","code":"X"}).into(),
                    status: Some(403.0),
                    config: None,
                }));
                h.borrow_mut().error_config = Some(config("u"));
                ready(failure(ExchangeError::new("original")))
            }
            _ => ready(failure(ExchangeError::new("original"))),
        };
        if i > 0 {
            assert_eq!(
                result.err().unwrap().message,
                r["error"].as_str().unwrap(),
                "API native error message {i}"
            );
        }
        let h = h.borrow();
        assert_eq!(
            h.events.len(),
            [0, 1, 2, 1][i],
            "API independent events count"
        );
        for ((kind, msg), e) in h.events.iter().zip(r["events"].as_array().unwrap()) {
            string(&u(kind), &e[0], "API native event kind");
            string(&u(msg), &e[1], "API native event message");
        }
    }
}
struct Listen {
    state: Rc<l::ListenerState<ExchangeError, ()>>,
    calls: Vec<J>,
    cleanup: Vec<String>,
    pending: Vec<l::Task<(), ExchangeError>>,
    timers: Vec<l::Action<ExchangeError>>,
    connected: bool,
}
impl Listen {
    fn new() -> Self {
        Self {
            state: Rc::new(l::ListenerState::default()),
            calls: vec![],
            cleanup: vec![],
            pending: vec![],
            timers: vec![],
            connected: true,
        }
    }
    fn call(&mut self, name: &str, args: Vec<String>) {
        let mut a = vec![J::String(name.into())];
        a.extend(args.into_iter().map(J::String));
        self.calls.push(J::Array(a));
    }
}
impl l::Host for Listen {
    type Error = ExchangeError;
    type Throttle = ();
    fn state(&self) -> Rc<l::ListenerState<ExchangeError, ()>> {
        self.state.clone()
    }
    fn market(&mut self, s: &[u16]) -> Result<Option<Vec<u16>>, ExchangeError> {
        Ok((s == u("BTC_USDC")).then(|| u("BTC_USDC")))
    }
    fn connected(&mut self) -> Result<bool, ExchangeError> {
        Ok(self.connected)
    }
    fn disposed(&mut self) -> Result<bool, ExchangeError> {
        Ok(false)
    }
    fn emit(&mut self, s: Vec<u16>) -> Result<(), ExchangeError> {
        self.call("error", vec![String::from_utf16(&s).unwrap()]);
        Ok(())
    }
    fn error_text(&mut self, e: &ExchangeError) -> Result<Vec<u16>, ExchangeError> {
        Ok(u(&e.js_error_string()))
    }
    fn timeout(&mut self, c: l::Action<ExchangeError>, d: f64) -> Result<(), ExchangeError> {
        assert_eq!(d, 100.0);
        self.timers.push(c);
        Ok(())
    }
    fn subscribe_kline(&mut self, s: Vec<u16>, i: Vec<u16>) -> Result<(), ExchangeError> {
        self.call(
            "kline",
            vec![
                String::from_utf16(&s).unwrap(),
                String::from_utf16(&i).unwrap(),
            ],
        );
        Ok(())
    }
    fn unsubscribe_kline(&mut self, s: Vec<u16>, i: Vec<u16>) -> Result<(), ExchangeError> {
        self.call(
            "unkline",
            vec![
                String::from_utf16(&s).unwrap(),
                String::from_utf16(&i).unwrap(),
            ],
        );
        Ok(())
    }
    fn subscribe_depth(&mut self, s: Vec<u16>) -> Result<(), ExchangeError> {
        self.call("depth", vec![String::from_utf16(&s).unwrap()]);
        Ok(())
    }
    fn unsubscribe(&mut self, s: Vec<u16>) -> Result<(), ExchangeError> {
        self.calls.push(J::Array(vec![
            J::String("unsub".into()),
            J::Array(vec![J::String(String::from_utf16(&s).unwrap())]),
        ]));
        Ok(())
    }
    fn cleanup_stats(&mut self, s: Vec<u16>) -> Result<(), ExchangeError> {
        self.cleanup.push(String::from_utf16(&s).unwrap());
        Ok(())
    }
    fn date(&mut self, s: Vec<u16>) -> Result<f64, ExchangeError> {
        assert_eq!(s, u("2025-01-01T00:00:00Z"), "CANDLE native date input");
        Ok(1735689600000.0)
    }
    fn snapshot(
        &mut self,
        s: Vec<u16>,
    ) -> Result<l::Task<l::Snapshot, ExchangeError>, ExchangeError> {
        self.calls.push(J::Array(vec![
            J::String("get".into()),
            J::String("/api/v1/depth".into()),
            J::Object(vec![(
                "params".into(),
                J::Object(vec![(
                    "symbol".into(),
                    J::String(String::from_utf16(&s).unwrap()),
                )]),
            )]),
        ]));
        Ok(Box::pin(std::future::ready(Ok(l::Snapshot {
            bids: vec![[u("10"), u("2")]],
            asks: vec![[u("11"), u("3")]],
            last: l::LastId::Number(4.0),
        }))))
    }
    fn spawn(&mut self, t: l::Task<(), ExchangeError>) -> Result<(), ExchangeError> {
        self.pending.push(t);
        Ok(())
    }
    fn apply(&mut self, book: l::Book, d: Rc<RefCell<l::Delta>>) -> Result<(), ExchangeError> {
        let levels = d.borrow().levels.clone();
        safe_cex_venue_backpack::orderbook_state::apply_order_book_update(
            &book,
            &levels,
            |b, side, row| {
                safe_cex_store::update_order_book::update_order_book(
                    &mut b.borrow_mut(),
                    matches!(
                        side,
                        safe_cex_venue_backpack::orderbook_state::BookSide::Bids
                    ),
                    row,
                    true,
                );
                Ok(())
            },
        )
    }
    fn totals(&mut self, b: l::Book) -> Result<(), ExchangeError> {
        safe_cex_store::orderbook::calc_order_book_total(&mut b.borrow_mut());
        Ok(())
    }
    fn recover(&mut self, s: Vec<u16>, reason: Vec<u16>) -> Result<(), ExchangeError> {
        self.call(
            "recover",
            vec![
                String::from_utf16(&s).unwrap(),
                String::from_utf16(&reason).unwrap(),
            ],
        );
        Ok(())
    }
    fn terminate(
        &mut self,
        _: Vec<u16>,
        state: l::State<ExchangeError>,
        _: Vec<u16>,
    ) -> Result<(), ExchangeError> {
        state.borrow_mut().is_terminal = true;
        Ok(())
    }
    fn cancel_throttle(&mut self, _: ()) -> Result<(), ExchangeError> {
        Ok(())
    }
}
#[test]
fn candle_listener_native_values_and_cleanup() {
    for i in 0..3 {
        let r = row("candle", i);
        assert_eq!(
            r["values"].as_array().unwrap().len(),
            [1, 0, 0][i],
            "CANDLE independent corpus CONTENT callbacks"
        );
        let h = Rc::new(RefCell::new(Listen::new()));
        let values = Rc::new(RefCell::new(vec![]));
        let out = values.clone();
        let stop = l::listen_ohlcv(
            h.clone(),
            u(if i == 2 { "missing" } else { "BTC_USDC" }),
            u("1m"),
            Rc::new(move |c| {
                out.borrow_mut().push(c);
                Ok(())
            }),
        )
        .unwrap();
        if i < 2 {
            let handler = h.borrow().state.handlers.borrow()[0].1.borrow()[0].clone();
            (handler.call)(l::Data::Kline(l::Kline {
                symbol: u("BTC_USDC"),
                time: u("2025-01-01T00:00:00"),
                open: if i == 1 {
                    l::Field::Null
                } else {
                    l::Field::Text(u("10"))
                },
                high: l::Field::Text(u("12")),
                low: l::Field::Text(u("8")),
                close: l::Field::Text(u("11")),
                volume: l::Field::Text(u("5")),
            }))
            .unwrap();
            stop().unwrap();
        }
        let values = values.borrow();
        assert_eq!(
            values.len(),
            [1, 0, 0][i],
            "CANDLE independent native callback count {i}"
        );
        for (c, e) in values.iter().zip(r["values"].as_array().unwrap()) {
            for (k, n) in [
                ("timestamp", c.timestamp),
                ("open", c.open),
                ("high", c.high),
                ("low", c.low),
                ("close", c.close),
                ("volume", c.volume),
            ] {
                number(n, &e[k], "CANDLE native decoded field");
            }
        }
        let h = h.borrow();
        native(
            &J::Array(h.calls.clone()),
            &r["calls"],
            "CANDLE native calls",
        );
        assert_eq!(h.cleanup, json_strings(&r["cleanups"]));
        if i < 2 {
            assert_eq!(
                h.state.topics.borrow()[0].1.to_bits(),
                0.0f64.to_bits(),
                "CANDLE native topic count"
            );
        }
    }
}
#[test]
fn depth_listener_native_snapshot_and_sequence() {
    for i in 0..3 {
        let r = row("depth", i);
        assert_eq!(
            r["values"].as_array().unwrap().len(),
            [2, 1, 2][i],
            "DEPTH independent corpus CONTENT callbacks"
        );
        let h = Rc::new(RefCell::new(Listen::new()));
        let books = Rc::new(RefCell::new(vec![]));
        let refs = Rc::new(RefCell::new(vec![]));
        let out = books.clone();
        let identities = refs.clone();
        let stop = l::listen_order_book(
            h.clone(),
            u("BTC_USDC"),
            Rc::new(move |b| {
                identities.borrow_mut().push(b.clone());
                out.borrow_mut().push(b.borrow().clone());
                Ok(())
            }),
        )
        .unwrap();
        let tasks = std::mem::take(&mut h.borrow_mut().pending);
        assert_eq!(tasks.len(), 1, "DEPTH independent snapshot task count");
        for f in tasks {
            ready(f).unwrap();
        }
        let parser = h.borrow().state.handlers.borrow()[0].1.borrow()[0].clone();
        let n = if i == 1 { 8.0 } else { 5.0 };
        (parser.call)(l::Data::Delta(Rc::new(RefCell::new(l::Delta {
            symbol: u("BTC_USDC"),
            sequence: safe_cex_venue_backpack::orderbook_state::SequenceUpdate {
                first: Some(n),
                last: Some(n),
            },
            levels: Rc::new(RefCell::new(
                safe_cex_venue_backpack::orderbook_state::DepthUpdate {
                    bids: Some(Rc::new(RefCell::new(vec![[u("10"), u("4")]]))),
                    asks: Some(Rc::new(RefCell::new(vec![]))),
                },
            )),
        }))))
        .unwrap();
        let b = books.borrow();
        assert_eq!(
            b.len(),
            [2, 1, 2][i],
            "DEPTH independent native callbacks {i}"
        );
        for (book, e) in b.iter().zip(r["values"].as_array().unwrap()) {
            for (levels, k) in [(&book.bids, "bids"), (&book.asks, "asks")] {
                assert_eq!(levels.len(), 1, "DEPTH independent levels floor");
                for (level, e) in levels.iter().zip(e[k].as_array().unwrap()) {
                    number(level.price, &e["price"], "DEPTH native price");
                    number(level.amount, &e["amount"], "DEPTH native amount");
                    number(level.total, &e["total"], "DEPTH native total");
                }
            }
        }
        let refs = refs.borrow();
        assert!(
            refs.iter().all(|b| Rc::ptr_eq(b, &refs[0])),
            "DEPTH native callback book identity"
        );
        let state = h.borrow().state.states.borrow()[0].1.clone();
        let state = state.borrow();
        number(
            state.last_update_id,
            &r["last"],
            "DEPTH native lastUpdateId",
        );
        assert_eq!(
            state.is_snapshot_loaded,
            r["loaded"].as_bool().unwrap(),
            "DEPTH native loaded"
        );
        assert_eq!(
            state.updates.len(),
            if i == 1 { 1 } else { 0 },
            "DEPTH independent queue count"
        );
        drop(state);
        stop().unwrap();
        assert!(
            h.borrow().state.states.borrow().is_empty(),
            "DEPTH native unsubscribe state removal"
        );
    }
}
