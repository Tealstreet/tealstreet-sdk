use safe_cex_types::{MarginType, PositionMode as Mode, PositionSide as Side};
use safe_cex_venue_woo::legacy_metadata::*;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::VecDeque,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};
type Error = Rc<String>;
fn u(s: &str) -> JsString {
    s.encode_utf16().collect()
}
fn err(s: &str) -> Error {
    Rc::new(s.to_owned())
}
fn ready<T: 'static>(r: Result<T, Error>) -> Task<T, Error> {
    Box::pin(std::future::ready(r))
}
fn run<T>(mut f: Task<T, Error>) -> Result<T, Error> {
    let w = Waker::noop().clone();
    let mut cx = Context::from_waker(&w);
    for _ in 0..16 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
    }
    panic!("METADATA ready fixture must resolve within 16 polls");
}
struct Fixture {
    state: Rc<State<Error>>,
    trace: Vec<String>,
    clock: f64,
    clocks: VecDeque<Result<f64, Error>>,
    value: Rc<Value>,
    get_error: Option<(Error, bool)>,
    pending: Option<Task<Rc<Value>, Error>>,
    post_error: Option<(Error, bool)>,
    account: Rc<Value>,
    modes: VecDeque<Mode>,
    confirm_error: Option<Error>,
    emitted: Vec<(Error, bool)>,
    requests: Vec<(String, Fields)>,
    drivers: Vec<Shared<Rows, Error>>,
    clock_append: Option<(JsString, Cached)>,
}
impl Fixture {
    fn new() -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {
            state: Rc::new(State::default()),
            trace: vec![],
            clock: 40000.0,
            clocks: VecDeque::new(),
            value: Rc::new(json!({"data":{"details":[{"leverage":"12.75"}]}})),
            get_error: None,
            pending: None,
            post_error: None,
            account: Rc::new(json!({"positionMode":"ONE_WAY"})),
            modes: VecDeque::new(),
            confirm_error: None,
            emitted: vec![],
            requests: vec![],
            drivers: vec![],
            clock_append: None,
        }))
    }
}
impl Host for Fixture {
    type Error = Error;
    type Value = Rc<Value>;
    type Positions = Vec<String>;
    fn state(&self) -> Rc<State<Error>> {
        self.state.clone()
    }
    fn get(&mut self, endpoint: &str, params: Fields) -> Result<Task<Rc<Value>, Error>, Error> {
        self.trace.push("get".into());
        self.requests.push((endpoint.into(), params));
        if let Some((e, sync)) = &self.get_error {
            return if *sync {
                Err(e.clone())
            } else {
                Ok(ready(Err(e.clone())))
            };
        }
        if let Some(task) = self.pending.take() {
            return Ok(task);
        }
        Ok(ready(Ok(self.value.clone())))
    }
    fn post(&mut self, endpoint: &str, body: Fields) -> Result<Task<(), Error>, Error> {
        self.trace.push("post".into());
        self.requests.push((endpoint.into(), body));
        if let Some((e, sync)) = &self.post_error {
            return if *sync {
                Err(e.clone())
            } else {
                Ok(ready(Err(e.clone())))
            };
        }
        Ok(ready(Ok(())))
    }
    fn object(&mut self, v: &Rc<Value>) -> bool {
        v.is_object() || v.is_array()
    }
    fn truthy(&mut self, v: &Rc<Value>) -> bool {
        match v.as_ref() {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::String(s) => !s.is_empty(),
            Value::Number(n) => n.as_f64().unwrap() != 0.0,
            _ => true,
        }
    }
    fn field(&mut self, v: &Rc<Value>, key: &str) -> Result<Rc<Value>, Error> {
        if v.is_null() {
            return Err(err("null field"));
        }
        Ok(Rc::new(v.get(key).cloned().unwrap_or(Value::Null)))
    }
    fn index(&mut self, v: &Rc<Value>, index: usize) -> Result<Rc<Value>, Error> {
        Ok(Rc::new(v[index].clone()))
    }
    fn strict_true(&mut self, v: &Rc<Value>) -> bool {
        v.as_ref() == &Value::Bool(true)
    }
    fn equals(&mut self, v: &Rc<Value>, s: &str) -> bool {
        v.as_str() == Some(s)
    }
    fn text(&mut self, v: &Rc<Value>) -> Result<JsString, Error> {
        Ok(u(&match v.as_ref() {
            Value::String(s) => s.clone(),
            v => v.to_string(),
        }))
    }
    fn response_message(&mut self, v: &Rc<Value>, fallback: &str) -> Result<JsString, Error> {
        Ok(u(&safe_cex_venue_woo::response::get_woo_response_message(
            v.as_object().unwrap(),
            fallback,
        )))
    }
    fn error(&mut self, m: JsString) -> Error {
        err(&String::from_utf16_lossy(&m))
    }
    fn now(&mut self) -> Result<f64, Error> {
        self.trace.push("now".into());
        if let Some((k, c)) = self.clock_append.take() {
            self.state.cache.borrow_mut().set(k, c);
        }
        self.clocks.pop_front().unwrap_or(Ok(self.clock))
    }
    fn reverse_symbol(&mut self, s: &[u16]) -> Result<JsString, Error> {
        self.trace.push("reverse".into());
        let mut a = u("PERP_");
        a.extend_from_slice(s);
        Ok(a)
    }
    fn rate_limit_error(&mut self, e: &Error) -> Result<bool, Error> {
        Ok(e.contains("rate limit"))
    }
    fn already_set_error(&mut self, e: &Error) -> Result<bool, Error> {
        Ok(e.to_lowercase().contains("position mode remains unchanged"))
    }
    fn emit_error(&mut self, e: &Error, full: bool, _: &str) -> Result<(), Error> {
        self.trace
            .push(format!("emit:{}", if full { 2 } else { 1 }));
        self.emitted.push((e.clone(), full));
        Ok(())
    }
    fn sleep(&mut self, delay: u64) -> Result<Task<(), Error>, Error> {
        self.trace.push(format!("sleep:{delay}"));
        Ok(ready(Ok(())))
    }
    fn fetch_positions(&mut self) -> Result<Task<Vec<String>, Error>, Error> {
        self.trace.push(format!(
            "positions:{}:{}",
            self.state.cache.borrow().len(),
            self.state.rate_limited_until.borrow().len()
        ));
        Ok(ready(Ok(vec!["position".into()])))
    }
    fn update_positions(&mut self, p: Vec<String>) -> Result<(), Error> {
        assert_eq!(p, vec!["position"]);
        self.trace.push("update".into());
        Ok(())
    }
    fn account_info(&mut self) -> Result<Task<Rc<Value>, Error>, Error> {
        self.trace.push("account".into());
        Ok(ready(Ok(self.account.clone())))
    }
    fn position_mode(&mut self) -> Result<Task<Mode, Error>, Error> {
        assert_eq!(
            self.state.mode.get(),
            None,
            "METADATA readback invalidates mode before request"
        );
        self.trace.push("mode:null".into());
        Ok(ready(Ok(self.modes.pop_front().unwrap())))
    }
    fn spawn_metadata(&mut self, request: Shared<Rows, Error>) {
        self.drivers.push(request);
    }
    fn confirm_mode(&mut self, m: Mode) -> Result<Task<(), Error>, Error> {
        self.trace.push(format!(
            "confirm:{}",
            if m == Mode::Hedged {
                "hedged"
            } else {
                "oneway"
            }
        ));
        Ok(ready(self.confirm_error.clone().map_or(Ok(()), Err)))
    }
}
fn seeded(mode: Mode, leverage: f64) -> Rows {
    Rc::new(RefCell::new(vec![Rc::new(Metadata {
        symbol: u("BTC"),
        side: Side::Long,
        leverage,
        position_mode: mode,
        margin_type: MarginType::Cross,
    })]))
}
fn cache(h: &Rc<RefCell<Fixture>>, key: &str, rows: Rows, time: f64) {
    h.borrow().state.cache.borrow_mut().set(
        u(key),
        Cached {
            metadata: rows,
            updated_at: time,
        },
    );
}
fn bits(n: f64) -> String {
    format!(
        "{:016x}",
        if n.is_nan() {
            0x7ff8000000000000
        } else {
            n.to_bits()
        }
    )
}
fn projection(r: &Rows) -> Value {
    json!(r.borrow().iter().map(|m|json!({"symbol":m.symbol,"side":if m.side==Side::Long{"long"}else{"short"},"leverage":bits(m.leverage),"mode":if m.position_mode==Mode::OneWay{"oneway"}else{"hedged"},"margin":"cross"})).collect::<Vec<_>>())
}
fn assert_rows(r: &Rows, symbol: &[u16], mode: Mode, leverage: f64, count: usize) {
    let rows = r.borrow();
    assert_eq!(rows.len(), count, "METADATA native row count");
    for (i, m) in rows.iter().enumerate() {
        assert_eq!(m.symbol, symbol, "METADATA native UTF16 symbol");
        assert_eq!(
            m.side,
            if i == 0 { Side::Long } else { Side::Short },
            "METADATA native side at {i}"
        );
        assert_eq!(m.position_mode, mode, "METADATA native mode");
        assert_eq!(m.margin_type, MarginType::Cross, "METADATA native margin");
        assert_eq!(bits(m.leverage), bits(leverage), "METADATA native leverage");
    }
}
fn corpus() -> Vec<Value> {
    let out = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/legacy_metadata_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "source oracle: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v.len(), 42, "METADATA corpus SIZE");
    let groups = [
        ("account", 8),
        ("mode", 3),
        ("worker", 5),
        ("build", 6),
        ("cache", 8),
        ("update", 2),
        ("confirm", 4),
        ("write", 6),
    ];
    let mut n = 0;
    let account = accounts();
    let worker = [json!(false), json!(true), json!(""), json!({}), Value::Null];
    for (family, count) in groups {
        for i in 0..count {
            assert_eq!(v[n]["family"], family, "METADATA corpus CONTENT family {n}");
            assert_eq!(v[n]["i"], i, "METADATA corpus CONTENT iteration {n}");
            let input = match family {
                "account" => account[i].clone(),
                "worker" => worker[i].clone(),
                "build" => {
                    json!({"leverage":build_inputs()[i],"mode":if i%2==1{"hedged"}else{"oneway"},"symbol":if i==5{vec![66,0xd800]}else{u("BTC")}})
                }
                "mode" => {
                    json!({"cached":if i==2{Some("oneway")}else{None},"account":if i==1{"HEDGE_MODE"}else{"ONE_WAY"},"worker":true})
                }
                "cache" => {
                    json!({"symbol":"BTC","mode":"hedged","clock":bits(if i==7{-1.0}else{40000.0}),"cached":i!=0&&i!=6,"updatedAt":if i==0||i==6{None}else{Some(bits(if i==7{-40000.0}else if i==1{10001.0}else{10000.0}))},"backoff":if i==2{Some(bits(50000.0))}else if i==7{Some(bits(f64::NAN))}else{None},"error":if i==3||i==4{Some("rate limit")}else if i==6{Some("network")}else{None},"sync":i==4,"commitClockError":i==5})
                }
                "update" => {
                    json!({"symbol":"BTC","leverage":bits(-0.0),"otherKey":if i==1{"BTC:hedged"}else{"ETH:oneway"}})
                }
                "confirm" => json!({"expected":"hedged","mismatches":i}),
                "write" => {
                    json!({"method":if i<2{"setLeverage"}else{"changePositionMode"},"leverage":bits(7.25),"hedged":true,"postError":if i==1||i==3{Some("post failed")}else if i==4{Some("Position mode remains unchanged")}else{None},"confirmError":if i==5{Some("confirm failed")}else{None}})
                }
                _ => json!(i),
            };
            assert_eq!(v[n]["input"], input, "METADATA corpus CONTENT input {n}");
            n += 1;
        }
    }
    v
}
fn accounts() -> [Value; 8] {
    [
        Value::Null,
        json!(3),
        json!({"success":false,"message":" denied "}),
        json!({"success":true}),
        json!({"success":true,"data":3}),
        json!({"success":true,"data":{"positionMode":"bad"}}),
        json!({"success":true,"data":{"positionMode":"ONE_WAY"}}),
        json!({"success":true,"data":{"positionMode":"HEDGE_MODE"}}),
    ]
}
fn build_inputs() -> [&'static str; 6] {
    [
        "12.75",
        "  -0.8",
        "0x20",
        "garbage",
        "1e3",
        "9007199254740993",
    ]
}
#[test]
fn account_validation_and_mode_timing() {
    let source = corpus();
    let messages = [
        "WOO X account info response has an invalid shape",
        "WOO X account info response has an invalid shape",
        " denied ",
        "WOO X account info response is missing data",
        "WOO X account info response is missing data",
        "WOO X account info response has an invalid position mode",
    ];
    for (i, v) in accounts().into_iter().enumerate() {
        let h = Fixture::new();
        h.borrow_mut().value = Rc::new(v);
        let result = run(fetch_account_info(h.clone()));
        assert_eq!(
            h.borrow().requests[0].0,
            "/v3/accountinfo",
            "METADATA native account endpoint"
        );
        if i < 6 {
            let e = result.unwrap_err();
            assert_eq!(e.as_str(), messages[i], "METADATA native account error {i}");
            assert_eq!(source[i]["error"], e.as_str());
        } else {
            let data = result.unwrap();
            assert_eq!(
                data["positionMode"],
                if i == 6 { "ONE_WAY" } else { "HEDGE_MODE" },
                "METADATA native account mode {i}"
            );
            assert_eq!(source[i]["ok"], *data);
        }
    }
    for i in 0..3 {
        let h = Fixture::new();
        if i == 1 {
            h.borrow_mut().account = Rc::new(json!({"positionMode":"HEDGE_MODE"}));
        }
        if i == 2 {
            h.borrow().state.mode.set(Some(Mode::OneWay));
        }
        let p = get_position_mode(h.clone());
        let before = h.borrow().trace.clone();
        after_method_called_in_worker(
            &mut *h.borrow_mut(),
            "changePositionMode",
            Some(Rc::new(json!(true))),
        );
        let m = run(p).unwrap();
        assert_eq!(
            m,
            if i == 1 { Mode::Hedged } else { Mode::OneWay },
            "METADATA native cached mode snapshot {i}"
        );
        assert_eq!(
            source[8 + i]["ok"],
            if m == Mode::Hedged {
                "hedged"
            } else {
                "oneway"
            }
        );
        assert_eq!(source[8 + i]["before"], json!(before));
    }
}
#[test]
fn worker_key_and_metadata_native_values() {
    let source = corpus();
    let args = [json!(false), json!(true), json!(""), json!({}), Value::Null];
    for (i, v) in args.into_iter().enumerate() {
        let h = Fixture::new();
        h.borrow().state.mode.set(Some(Mode::Hedged));
        after_method_called_in_worker(
            &mut *h.borrow_mut(),
            if i == 4 {
                "other"
            } else {
                "changePositionMode"
            },
            Some(Rc::new(v)),
        );
        let mode = h.borrow().state.mode.get().unwrap();
        assert_eq!(
            mode,
            if i == 1 || i == 3 || i == 4 {
                Mode::Hedged
            } else {
                Mode::OneWay
            },
            "METADATA native worker truthiness {i}"
        );
        assert_eq!(
            source[11 + i]["ok"],
            if mode == Mode::Hedged {
                "hedged"
            } else {
                "oneway"
            }
        );
    }
    let expected = [12.0, -0.0, 32.0, f64::NAN, 1.0, 9007199254740992.0];
    for (i, text) in build_inputs().into_iter().enumerate() {
        let h = Fixture::new();
        let symbol = if i == 5 { vec![66, 0xd800] } else { u("BTC") };
        let mode = if i % 2 == 1 {
            Mode::Hedged
        } else {
            Mode::OneWay
        };
        let response = Rc::new(json!({"data":{"details":[{"leverage":text}]}}));
        let r = build_position_metadata_from_leverage(
            &mut *h.borrow_mut(),
            symbol.clone(),
            mode,
            response,
        )
        .unwrap();
        assert_rows(
            &r,
            &symbol,
            mode,
            expected[i],
            if i % 2 == 1 { 2 } else { 1 },
        );
        let key = create_position_metadata_cache_key(&symbol, mode);
        let mut want = symbol;
        want.extend(u(if mode == Mode::OneWay {
            ":oneway"
        } else {
            ":hedged"
        }));
        assert_eq!(key, want, "METADATA native key");
        assert_eq!(source[16 + i]["key"], json!(key));
        assert_eq!(source[16 + i]["ok"], projection(&r));
    }
}
#[test]
fn cache_ttl_rate_fallback_and_sync_throw() {
    let source = corpus();
    for i in 0..8 {
        let h = Fixture::new();
        let old = seeded(Mode::Hedged, 3.0);
        if i != 0 && i != 6 {
            cache(
                &h,
                "BTC:hedged",
                old.clone(),
                if i == 1 { 10001.0 } else { 10000.0 },
            );
        }
        if i == 2 {
            h.borrow()
                .state
                .rate_limited_until
                .borrow_mut()
                .set(u("BTC:hedged"), 50000.0);
        }
        if i == 7 {
            h.borrow_mut().clock = -1.0;
            cache(&h, "BTC:hedged", old.clone(), -40000.0);
            h.borrow()
                .state
                .rate_limited_until
                .borrow_mut()
                .set(u("BTC:hedged"), f64::NAN);
        }
        if i == 3 || i == 4 {
            h.borrow_mut().get_error = Some((err("rate limit"), i == 4));
        }
        if i == 6 {
            h.borrow_mut().get_error = Some((err("network"), false));
        }
        if i == 5 {
            h.borrow_mut().clocks =
                VecDeque::from([Ok(40000.0), Err(err("rate limit")), Ok(41000.0)]);
        }
        let p = fetch_position_metadata_for_symbol(h.clone(), u("BTC"), Mode::Hedged);
        let before = h.borrow().trace.clone();
        let result = run(p);
        let native = if i == 6 {
            assert_eq!(
                result.unwrap_err().as_str(),
                "network",
                "METADATA native fetch rejection"
            );
            json!({"error":"network"})
        } else {
            let r = result.unwrap();
            let same = matches!(i, 1..=5) || i == 7;
            assert_eq!(
                Rc::ptr_eq(&r, &old),
                same,
                "METADATA native cache alias {i}"
            );
            assert_rows(
                &r,
                &u("BTC"),
                Mode::Hedged,
                if same { 3.0 } else { 12.0 },
                if same { 1 } else { 2 },
            );
            json!({"ok":{"rows":projection(&r),"same":same}})
        };
        let h = h.borrow();
        assert_eq!(
            h.state.in_flight.borrow().len(),
            usize::from(i == 4),
            "METADATA native synchronous finally/set ordering {i}"
        );
        if !h.requests.is_empty() {
            assert_eq!(h.requests[0].0, "/v1/client/futures_leverage");
            let fields = &h.requests[0].1;
            assert_eq!(fields.len(), 3);
            for (index, (name, text)) in [
                ("symbol", "PERP_BTC"),
                ("position_mode", "ONE_WAY"),
                ("margin_mode", "CROSS"),
            ]
            .into_iter()
            .enumerate()
            {
                assert_eq!(fields[index].0, name);
                let Field::Text(v) = &fields[index].1 else {
                    panic!("native request text")
                };
                assert_eq!(*v, u(text), "METADATA native request field {index}");
            }
        }
        let until = h.state.rate_limited_until.borrow().get(&u("BTC:hedged"));
        let expected_until = if i == 2 || i == 3 || i == 4 {
            Some(50000.0)
        } else if i == 5 {
            Some(51000.0)
        } else {
            None
        };
        assert_eq!(
            until.filter(|v| v.is_finite()),
            expected_until,
            "METADATA native backoff deadline {i}"
        );
        let field = if i == 6 { "error" } else { "ok" };
        assert_eq!(source[22 + i][field], native[field]);
        assert_eq!(source[22 + i]["inFlight"], h.state.in_flight.borrow().len());
        assert_eq!(source[22 + i]["trace"], json!(h.trace));
        assert_eq!(source[22 + i]["before"], json!(before));
        let until = h.state.rate_limited_until.borrow().get(&u("BTC:hedged"));
        assert_eq!(
            source[22 + i]["until"].as_f64(),
            until.filter(|v| v.is_finite())
        );
    }
}
#[test]
fn cache_updates_replace_objects_and_live_map_iteration() {
    let source = corpus();
    for i in 0..2 {
        let h = Fixture::new();
        let old = seeded(Mode::OneWay, 3.0);
        let other = seeded(Mode::OneWay, 3.0);
        cache(&h, "BTC:oneway", old.clone(), 1.0);
        let other_key = if i == 1 { "BTC:hedged" } else { "ETH:oneway" };
        cache(&h, other_key, other.clone(), 1.0);
        h.borrow()
            .state
            .rate_limited_until
            .borrow_mut()
            .set(u("BTC:oneway"), 99999.0);
        update_cached_position_metadata_leverage(&mut *h.borrow_mut(), &u("BTC"), -0.0).unwrap();
        let h = h.borrow();
        let r = h.state.cache.borrow().get(&u("BTC:oneway")).unwrap();
        assert_rows(&r.metadata, &u("BTC"), Mode::OneWay, -0.0, 1);
        assert_rows(&old, &u("BTC"), Mode::OneWay, 3.0, 1);
        assert!(!Rc::ptr_eq(&r.metadata, &old), "METADATA native new array");
        assert!(
            !Rc::ptr_eq(&r.metadata.borrow()[0], &old.borrow()[0]),
            "METADATA native new object"
        );
        let other_same = Rc::ptr_eq(
            &h.state.cache.borrow().get(&u(other_key)).unwrap().metadata,
            &other,
        );
        assert_eq!(other_same, i == 0);
        assert!(h.state.rate_limited_until.borrow().is_empty());
        assert_eq!(source[30 + i]["ok"], projection(&r.metadata));
        assert_eq!(source[30 + i]["old"], projection(&old));
        assert_eq!(source[30 + i]["same"], false);
        assert_eq!(source[30 + i]["otherSame"], other_same);
        assert_eq!(source[30 + i]["trace"], json!(h.trace));
    }
    let h = Fixture::new();
    cache(&h, "BTC:oneway", seeded(Mode::OneWay, 1.0), 1.0);
    h.borrow_mut().clock_append = Some((
        u("BTC:hedged"),
        Cached {
            metadata: seeded(Mode::Hedged, 2.0),
            updated_at: 1.0,
        },
    ));
    update_cached_position_metadata_leverage(&mut *h.borrow_mut(), &u("BTC"), 7.0).unwrap();
    let c = h
        .borrow()
        .state
        .cache
        .borrow()
        .get(&u("BTC:hedged"))
        .unwrap();
    assert_rows(&c.metadata, &u("BTC"), Mode::Hedged, 7.0, 1);
}
#[test]
fn confirm_retries_clear_before_refresh_and_exhaustion() {
    let source = corpus();
    for i in 0..4 {
        let h = Fixture::new();
        cache(&h, "k", seeded(Mode::OneWay, 1.0), 0.0);
        h.borrow()
            .state
            .rate_limited_until
            .borrow_mut()
            .set(u("k"), 1.0);
        h.borrow_mut().modes = (0..3)
            .map(|n| if n >= i { Mode::Hedged } else { Mode::OneWay })
            .collect();
        let result = run(confirm_position_mode_and_refresh_positions(
            h.clone(),
            Mode::Hedged,
        ));
        let error = if i == 3 {
            let e = result.unwrap_err();
            assert_eq!(
                e.as_str(),
                "WOO position mode readback stayed oneway; expected hedged",
                "METADATA native exhausted readback"
            );
            Some(e.to_string())
        } else {
            assert!(result.is_ok(), "METADATA native confirmed mode attempt {i}");
            result.unwrap();
            None
        };
        let h = h.borrow();
        assert_eq!(
            h.trace.iter().filter(|s| s.starts_with("mode:")).count(),
            (i + 1).min(3),
            "METADATA native read attempts"
        );
        assert_eq!(
            h.trace.iter().filter(|s| s.starts_with("sleep:")).count(),
            i.min(2),
            "METADATA native retry delay count"
        );
        assert_eq!(h.state.cache.borrow().len(), usize::from(i == 3));
        assert_eq!(source[32 + i]["error"], json!(error));
        assert_eq!(source[32 + i]["trace"], json!(h.trace));
    }
}
#[test]
fn leverage_and_mode_writes_native_fields_errors_and_refresh() {
    let source = corpus();
    for i in 0..6 {
        let h = Fixture::new();
        cache(&h, "BTC:oneway", seeded(Mode::OneWay, 3.0), 0.0);
        let failure = err(if i == 4 {
            "Position mode remains unchanged"
        } else {
            "post failed"
        });
        if i == 1 || i == 3 || i == 4 {
            h.borrow_mut().post_error = Some((failure.clone(), false));
        }
        if i == 5 {
            h.borrow_mut().confirm_error = Some(err("confirm failed"));
        }
        let result = run(if i < 2 {
            set_leverage(h.clone(), u("BTC"), 7.25)
        } else {
            change_position_mode(h.clone(), true)
        });
        let error = match result {
            Ok(()) => {
                assert!(i == 0 || i == 2 || i == 4, "METADATA native write accepted");
                None
            }
            Err(e) => {
                assert!(i == 1 || i == 3 || i == 5, "METADATA native write rejected");
                if i != 5 {
                    assert!(
                        Rc::ptr_eq(&e, &failure),
                        "METADATA native original error identity"
                    );
                }
                Some(e.to_string())
            }
        };
        let h = h.borrow();
        let c = h.state.cache.borrow().get(&u("BTC:oneway")).unwrap();
        assert_rows(
            &c.metadata,
            &u("BTC"),
            Mode::OneWay,
            if i == 0 { 7.25 } else { 3.0 },
            1,
        );
        let request = &h.requests[0];
        if i < 2 {
            assert_eq!(request.0, "/v1/client/futures_leverage");
            assert_eq!(request.1.len(), 4);
            let expected = [
                ("symbol", "PERP_BTC"),
                ("margin_mode", "CROSS"),
                ("position_side", "BOTH"),
            ];
            for (n, (key, text)) in expected.into_iter().enumerate() {
                assert_eq!(request.1[n].0, key);
                let Field::Text(v) = &request.1[n].1 else {
                    panic!("native text")
                };
                assert_eq!(*v, u(text), "METADATA native leverage body {n}");
            }
            let Field::Number(v) = request.1[3].1 else {
                panic!("native leverage number")
            };
            assert_eq!(
                v.to_bits(),
                7.25f64.to_bits(),
                "METADATA native POST leverage"
            );
        } else {
            assert_eq!(request.0, "/v1/client/position_mode");
            assert_eq!(request.1[0].0, "position_mode");
            let Field::Text(v) = &request.1[0].1 else {
                panic!("native mode text")
            };
            assert_eq!(*v, u("HEDGE_MODE"));
        }
        assert_eq!(source[36 + i]["error"], json!(error));
        assert_eq!(source[36 + i]["trace"], json!(h.trace));
        assert_eq!(
            source[36 + i]["leverage"],
            bits(c.metadata.borrow()[0].leverage)
        );
    }
}
struct Signal {
    ready: Rc<RefCell<bool>>,
    waker: Rc<RefCell<Option<Waker>>>,
}
impl Future for Signal {
    type Output = Result<Rc<Value>, Error>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if *self.ready.borrow() {
            Poll::Ready(Ok(Rc::new(json!({"data":{"details":[{"leverage":"9"}]}}))))
        } else {
            *self.waker.borrow_mut() = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}
struct Counter(AtomicUsize);
impl Wake for Counter {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
#[test]
fn abandoned_reader_wakes_remaining_shared_reader() {
    let ready_flag = Rc::new(RefCell::new(false));
    let saved = Rc::new(RefCell::new(None));
    let h = Fixture::new();
    h.borrow_mut().pending = Some(Box::pin(Signal {
        ready: ready_flag.clone(),
        waker: saved.clone(),
    }));
    let mut canceled = fetch_position_metadata_for_symbol(h.clone(), u("BTC"), Mode::Hedged);
    let mut survivor = fetch_position_metadata_for_symbol(h.clone(), u("BTC"), Mode::Hedged);
    let count = Arc::new(Counter(AtomicUsize::new(0)));
    let w = Waker::from(count.clone());
    let mut cx = Context::from_waker(&w);
    assert!(survivor.as_mut().poll(&mut cx).is_pending());
    let w2 = Waker::noop().clone();
    let mut cx2 = Context::from_waker(&w2);
    assert!(canceled.as_mut().poll(&mut cx2).is_pending());
    drop(canceled);
    *ready_flag.borrow_mut() = true;
    saved.borrow_mut().take().unwrap().wake();
    assert_eq!(
        count.0.load(Ordering::SeqCst),
        1,
        "METADATA native surviving shared reader wake"
    );
    let Poll::Ready(Ok(rows)) = survivor.as_mut().poll(&mut cx) else {
        panic!("METADATA shared reader completion");
    };
    assert_rows(&rows, &u("BTC"), Mode::Hedged, 9.0, 2);
    assert_eq!(
        h.borrow().requests.len(),
        1,
        "METADATA native in-flight deduplication"
    );
    assert!(h.borrow().state.in_flight.borrow().is_empty());
    let h = Fixture::new();
    let ignored = fetch_position_metadata_for_symbol(h.clone(), u("BTC"), Mode::OneWay);
    drop(ignored);
    let driver = h.borrow_mut().drivers.pop().unwrap();
    run(Box::pin(driver)).unwrap();
    assert_eq!(
        h.borrow().state.cache.borrow().len(),
        1,
        "METADATA spawned driver commits abandoned caller"
    );
}
