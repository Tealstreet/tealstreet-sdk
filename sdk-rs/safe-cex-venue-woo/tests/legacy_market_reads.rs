use safe_cex_types::PositionMode;
use safe_cex_venue_woo::{
    legacy_market_reads::*,
    legacy_metadata::{JsString, Task},
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    rc::Rc,
    task::{Context, Poll, Waker},
};
#[derive(Clone, Debug)]
enum Js {
    U,
    Null,
    B(bool),
    N(f64),
    S(JsString),
    A(Vec<Option<V>>),
    O(Vec<(String, V)>),
}
type V = Rc<RefCell<Js>>;
fn val(j: Js) -> V {
    Rc::new(RefCell::new(j))
}
fn u(s: &str) -> JsString {
    s.encode_utf16().collect()
}
fn strv(s: &str) -> V {
    val(Js::S(u(s)))
}
fn num(n: f64) -> V {
    val(Js::N(n))
}
fn obj(fields: Vec<(&str, V)>) -> V {
    val(Js::O(
        fields.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    ))
}
fn arr(a: Vec<V>) -> V {
    val(Js::A(a.into_iter().map(Some).collect()))
}
fn field(v: &V, k: &str) -> V {
    match &*v.borrow() {
        Js::O(o) => o
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| val(Js::U)),
        _ => val(Js::U),
    }
}
fn set(v: &V, k: &str, new: V) {
    let mut v = v.borrow_mut();
    let Js::O(o) = &mut *v else {
        panic!("native object write")
    };
    if let Some((_, v)) = o.iter_mut().find(|(n, _)| n == k) {
        *v = new;
    } else {
        o.push((k.into(), new));
    }
}
fn truthy(v: &V) -> bool {
    match &*v.borrow() {
        Js::U | Js::Null => false,
        Js::B(b) => *b,
        Js::N(n) => *n != 0.0 && !n.is_nan(),
        Js::S(s) => !s.is_empty(),
        _ => true,
    }
}
fn txt(v: &V) -> String {
    match &*v.borrow() {
        Js::U => "undefined".into(),
        Js::Null => "null".into(),
        Js::B(b) => b.to_string(),
        Js::N(n) => safe_cex_num::js_number_string(*n),
        Js::S(s) => String::from_utf16(s).unwrap(),
        Js::O(_) => "[object Object]".into(),
        Js::A(a) => a
            .iter()
            .map(|v| {
                v.as_ref()
                    .filter(|v| !matches!(*v.borrow(), Js::U | Js::Null))
                    .map(txt)
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join(","),
    }
}
fn number(v: &V) -> f64 {
    match &*v.borrow() {
        Js::N(n) => *n,
        Js::Null => 0.0,
        Js::U => f64::NAN,
        Js::B(b) => f64::from(*b),
        _ => safe_cex_num::js_number(Some(&Value::String(txt(v)))).unwrap(),
    }
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
fn enc(v: &V) -> Value {
    match &*v.borrow() {
        Js::U => json!({"u":true}),
        Js::Null => Value::Null,
        Js::B(b) => json!(b),
        Js::N(n) => json!({"n":bits(*n)}),
        Js::S(s) => json!({"s":s}),
        Js::A(a) => Value::Array(
            a.iter()
                .map(|v| v.as_ref().map(enc).unwrap_or(json!({"hole":true})))
                .collect(),
        ),
        Js::O(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), enc(v))).collect()),
    }
}
fn descriptor(v: &V, key: &str) -> Value {
    let mut e = enc(v);
    if let Value::Object(o) = &mut e {
        if o.contains_key(key) {
            o.insert(key.into(), json!({"getter":true}));
        }
    } else if let Value::Array(a) = &mut e {
        if let Some(Value::Object(o)) = a.first_mut() {
            o.insert(key.into(), json!({"getter":true}));
        }
    }
    e
}
fn error(s: &str) -> V {
    obj(vec![("message", strv(s))])
}
fn ready(r: Result<V, V>) -> Task<V, V> {
    Box::pin(std::future::ready(r))
}
fn run(mut f: Task<V, V>) -> Result<V, V> {
    let w = Waker::noop().clone();
    let mut c = Context::from_waker(&w);
    match f.as_mut().poll(&mut c) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("READ ready fixture pending"),
    }
}
fn account() -> V {
    obj(vec![
        ("accountMode", strv("FUTURES")),
        ("positionMode", strv("ONE_WAY")),
        ("applicationId", num(42.0)),
        ("referrerID", num(0.0)),
        ("freeCollateral", num(1.235)),
        ("totalAccountValue", num(10.555)),
        ("totalCollateral", num(8.335)),
    ])
}
fn market_rows() -> V {
    arr(vec![
        obj(vec![("symbol", strv("SPOT_BTC_USDT"))]),
        obj(vec![
            ("symbol", strv("PERP_BTC_USDT")),
            ("base_imr", num(0.0)),
            ("baseTick", strv("0.125")),
            ("quote_tick", num(0.25)),
            ("base_min", num(0.01)),
            ("base_max", num(99.0)),
        ]),
        obj(vec![
            ("symbol", strv("PERP_ETH_USDT")),
            ("base_imr", strv("0.025")),
            ("base_tick", num(0.5)),
            ("quote_tick", num(0.75)),
            ("base_min", num(1.0)),
            ("base_max", num(100.0)),
        ]),
    ])
}
fn ticker(symbol: &str) -> V {
    obj(vec![
        ("symbol", strv(symbol)),
        ("24h_open", num(8.0)),
        ("24h_close", num(10.0)),
        ("index_price", strv("9.25")),
        ("mark_price", num(9.75)),
        ("last_funding_rate", num(-0.0)),
        ("next_funding_time", num(1234.0)),
        ("24h_volume", num(4.0)),
        ("24h_amount", num(40.0)),
        ("open_interest", num(5.0)),
    ])
}
struct Fixture {
    account: V,
    balance: V,
    markets: V,
    tickers: V,
    positions: V,
    rows: V,
    response: V,
    mode: Option<PositionMode>,
    secondary: String,
    trace: Vec<String>,
    logs: Vec<V>,
    requests: Vec<(&'static str, String)>,
    account_error: Option<(V, bool)>,
    balance_error: Option<(V, bool)>,
    http_error: Option<(V, bool)>,
    positions_error: Option<V>,
    raw_error: Option<V>,
    free_getter: bool,
    free_reads: usize,
    live_getter: bool,
    grew: bool,
    parse_identity: bool,
}
impl Fixture {
    fn new() -> Rc<RefCell<Self>> {
        let rows = market_rows();
        Rc::new(RefCell::new(Self {
            account: account(),
            balance: obj(vec![
                ("free", num(7.0)),
                ("total", num(50.0)),
                ("used", num(8.0)),
                ("upnl", num(9.25)),
            ]),
            markets: arr(vec![obj(vec![("old", val(Js::B(true)))])]),
            tickers: arr(vec![obj(vec![("old", val(Js::B(true)))])]),
            positions: arr(vec![
                obj(vec![("unrealizedPnl", num(1.235))]),
                obj(vec![("unrealizedPnl", num(2.345))]),
            ]),
            rows: rows.clone(),
            response: obj(vec![("data", obj(vec![("rows", rows)]))]),
            mode: None,
            secondary: "".into(),
            trace: vec![],
            logs: vec![],
            requests: vec![],
            account_error: None,
            balance_error: None,
            http_error: None,
            positions_error: None,
            raw_error: None,
            free_getter: false,
            free_reads: 0,
            live_getter: false,
            grew: false,
            parse_identity: false,
        }))
    }
    fn record(
        &self,
        family: &str,
        i: usize,
        input: Value,
        result: &Result<V, V>,
        before: Vec<String>,
        alias: bool,
        info_alias: bool,
    ) -> Value {
        let mut row = json!({"family":family,"i":i,"input":input,"before":before,"trace":self.trace,"logs":self.logs.iter().map(enc).collect::<Vec<_>>(),"requests":self.requests,"alias":alias,"infoAlias":info_alias,"mode":self.mode.map(|m|if m==PositionMode::OneWay{"oneway"}else{"hedged"}),"freeReads":self.free_reads});
        match result {
            Ok(v) => {
                row["ok"] = enc(v);
            }
            Err(e) => {
                row["ok"] = json!({"u":true});
                row["error"] = json!(txt(&field(e, "message")));
            }
        }
        row
    }
}
impl Host for Fixture {
    type Value = V;
    type Error = V;
    type Prepared = Call;
    fn prepare(&mut self, c: Call) -> Result<Call, V> {
        Ok(c)
    }
    fn invoke(&mut self, c: Call) -> Result<Task<V, V>, V> {
        let (label, value, fail) = match c {
            Call::AccountInfo => ("account", self.account.clone(), self.account_error.clone()),
            Call::Balance => ("balance", self.balance.clone(), self.balance_error.clone()),
            Call::Positions => (
                "positions",
                self.positions.clone(),
                self.positions_error.clone().map(|e| (e, false)),
            ),
            Call::TickersRaw => (
                "raw",
                self.rows.clone(),
                self.raw_error.clone().map(|e| (e, false)),
            ),
            _ => panic!("unexpected invocation"),
        };
        self.trace.push(label.into());
        match fail {
            Some((e, true)) => Err(e),
            Some((e, false)) => Ok(ready(Err(e))),
            None => Ok(ready(Ok(value))),
        }
    }
    fn invoke_parse(&mut self, c: Call, rows: V) -> Result<V, V> {
        assert_eq!(c, Call::ParseTickers);
        self.trace.push("parse".into());
        if self.parse_identity {
            Ok(rows)
        } else {
            parse_tickers(self, rows)
        }
    }
    fn get(&mut self, t: Transport, path: &str) -> Result<Task<V, V>, V> {
        let label = match t {
            Transport::Main => "main",
            Transport::Primary => "primary",
            Transport::Secondary => "secondary",
        };
        self.trace.push(format!("get:{label}"));
        self.requests.push((label, path.into()));
        match self.http_error.clone() {
            Some((e, true)) => Err(e),
            Some((e, false)) => Ok(ready(Err(e))),
            None => Ok(ready(Ok(self.response.clone()))),
        }
    }
    fn field(&mut self, v: &V, k: &str) -> Result<V, V> {
        if matches!(*v.borrow(), Js::U | Js::Null) {
            return Err(error(&format!(
                "Cannot read properties of {} (reading '{k}')",
                if matches!(*v.borrow(), Js::U) {
                    "undefined"
                } else {
                    "null"
                }
            )));
        }
        if k == "freeCollateral" && self.free_getter && Rc::ptr_eq(v, &self.account) {
            let n = [1.235, 2.345][self.free_reads];
            self.free_reads += 1;
            return Ok(num(n));
        }
        if k == "symbol" && self.live_getter && !self.grew {
            self.grew = true;
            let mut rows = self.rows.borrow_mut();
            let Js::A(a) = &mut *rows else {
                panic!("live rows")
            };
            a.push(Some(ticker("PERP_ETH_USDT")));
        }
        Ok(field(v, k))
    }
    fn v(&mut self, v: &V, k: &str) -> Result<V, V> {
        let direct = field(v, k);
        let has = matches!(&*v.borrow(),Js::O(o) if o.iter().any(|(key,_)|key==k));
        if has {
            return Ok(direct);
        }
        let mut camel = String::new();
        let mut upper = false;
        for ch in k.chars() {
            if ch == '_' {
                upper = true;
            } else {
                camel.push(if upper { ch.to_ascii_uppercase() } else { ch });
                upper = false;
            }
        }
        Ok(field(v, &camel))
    }
    fn set_field(&mut self, v: &V, k: &str, new: V) -> Result<(), V> {
        set(v, k, new);
        Ok(())
    }
    fn truthy(&self, v: &V) -> bool {
        truthy(v)
    }
    fn nullish(&self, v: &V) -> bool {
        matches!(*v.borrow(), Js::U | Js::Null)
    }
    fn undefined(&self) -> V {
        val(Js::U)
    }
    fn null(&self) -> V {
        val(Js::Null)
    }
    fn number(&self, n: f64) -> V {
        num(n)
    }
    fn boolean(&self, b: bool) -> V {
        val(Js::B(b))
    }
    fn string(&self, s: JsString) -> V {
        val(Js::S(s))
    }
    fn object(&self, o: Vec<(&'static str, V)>) -> V {
        obj(o)
    }
    fn array(&self, a: Vec<Option<V>>) -> V {
        val(Js::A(a))
    }
    fn to_number(&mut self, v: &V) -> Result<f64, V> {
        Ok(number(v))
    }
    fn text(&mut self, v: &V) -> Result<JsString, V> {
        Ok(u(&txt(v)))
    }
    fn equals(&self, v: &V, s: &str) -> bool {
        matches!(&*v.borrow(),Js::S(t)if*t==u(s))
    }
    fn starts_with(&mut self, v: &V, p: &str) -> Result<bool, V> {
        let Js::S(s) = &*v.borrow() else {
            return Err(error("r.symbol.startsWith is not a function"));
        };
        Ok(s.starts_with(&u(p)))
    }
    fn split(&mut self, v: &V, sep: &str) -> Result<Vec<V>, V> {
        Ok(txt(v).split(sep).map(strv).collect())
    }
    fn is_array(&self, v: &V) -> bool {
        matches!(*v.borrow(), Js::A(_))
    }
    fn length(&mut self, v: &V, method: &str) -> Result<usize, V> {
        match &*v.borrow() {
            Js::A(a) => Ok(a.len()),
            Js::U | Js::Null => Err(error(&format!(
                "Cannot read properties of {} (reading '{method}')",
                if matches!(*v.borrow(), Js::U) {
                    "undefined"
                } else {
                    "null"
                }
            ))),
            _ => Err(error(&format!("rows.{method} is not a function"))),
        }
    }
    fn has_index(&mut self, v: &V, i: usize) -> Result<bool, V> {
        let Js::A(a) = &*v.borrow() else {
            panic!("array")
        };
        Ok(a.get(i).is_some_and(Option::is_some))
    }
    fn index(&mut self, v: &V, i: usize) -> Result<V, V> {
        let Js::A(a) = &*v.borrow() else {
            panic!("array")
        };
        Ok(a.get(i)
            .and_then(Clone::clone)
            .unwrap_or_else(|| val(Js::U)))
    }
    fn normalize_symbol(&mut self, v: V) -> Result<V, V> {
        Ok(strv(&safe_cex_venue_woo::utils::normalize_symbol(&txt(&v))))
    }
    fn market(&mut self, v: &V) -> Result<V, V> {
        Ok(match txt(v).as_str() {
            "BTCUSDT" => obj(vec![
                ("id", strv("BTC/USDT:USDT")),
                ("symbol", strv("BTCUSDT")),
            ]),
            "ETHUSDT" => obj(vec![
                ("id", strv("ETH/USDT:USDT")),
                ("symbol", strv("ETHUSDT")),
            ]),
            _ => val(Js::U),
        })
    }
    fn stored(&mut self, k: &str) -> Result<V, V> {
        Ok(match k {
            "balance" => self.balance.clone(),
            "markets" => self.markets.clone(),
            "tickers" => self.tickers.clone(),
            _ => panic!("stored key"),
        })
    }
    fn set_position_mode(&mut self, m: PositionMode) -> Result<(), V> {
        self.mode = Some(m);
        Ok(())
    }
    fn secondary_key(&mut self) -> Result<V, V> {
        Ok(strv(&self.secondary))
    }
    fn sum_by(&mut self, v: &V, k: &str) -> Result<V, V> {
        let Js::A(a) = &*v.borrow() else {
            panic!("sum array")
        };
        let mut result = val(Js::U);
        for v in a.iter().flatten() {
            let value = field(v, k);
            if matches!(*value.borrow(), Js::U) {
                continue;
            }
            result = if matches!(*result.borrow(), Js::U) {
                value
            } else if matches!(*result.borrow(), Js::S(_)) || matches!(*value.borrow(), Js::S(_)) {
                strv(&(txt(&result) + &txt(&value)))
            } else {
                num(number(&result) + number(&value))
            };
        }
        Ok(result)
    }
    fn error(&mut self, c: Call, message: JsString) -> V {
        assert!(matches!(c, Call::ObjectError | Call::TypeError));
        error(&String::from_utf16(&message).unwrap())
    }
    fn error_field(&mut self, e: &V, path: &[&str]) -> Result<V, V> {
        let mut v = e.clone();
        for p in path {
            v = field(&v, p);
        }
        Ok(v)
    }
    fn emit(&mut self, c: Call, message: V) -> Result<(), V> {
        assert_eq!(c, Call::EmitError);
        self.trace.push("emit".into());
        self.logs.push(message);
        Ok(())
    }
}
fn rows(v: &V) -> Vec<V> {
    let Js::A(a) = &*v.borrow() else {
        panic!("READ native array")
    };
    a.iter().map(|v| v.clone().unwrap()).collect()
}
fn n(v: &V, key: &str, expected: f64) {
    let value = field(v, key);
    let Js::N(actual) = *value.borrow() else {
        panic!("READ native numeric field {key}")
    };
    assert_eq!(bits(actual), bits(expected), "READ native number {key}");
}
fn s(v: &V, key: &str, expected: &str) {
    let value = field(v, key);
    let Js::S(actual) = &*value.borrow() else {
        panic!("READ native text {key}")
    };
    assert_eq!(*actual, u(expected), "READ native text {key}");
}
fn source() -> Vec<Value> {
    let o = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/legacy_market_reads_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "READ oracle {}",
        String::from_utf8_lossy(&o.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(rows.len(), 34, "READ corpus SIZE");
    let mut at = 0;
    for (family, count) in [
        ("account", 3),
        ("validate", 6),
        ("balance", 6),
        ("markets", 4),
        ("raw", 3),
        ("parse", 6),
        ("tickers", 2),
        ("combined", 4),
    ] {
        for i in 0..count {
            assert_eq!(
                rows[at]["family"], family,
                "READ corpus CONTENT family {at}"
            );
            assert_eq!(rows[at]["i"], i, "READ corpus CONTENT iteration {at}");
            at += 1;
        }
    }
    rows
}
fn check(
    source: &Value,
    h: &Fixture,
    family: &str,
    i: usize,
    input: Value,
    result: &Result<V, V>,
    before: Vec<String>,
    alias: bool,
    info: bool,
) {
    assert_eq!(
        source["input"], input,
        "READ corpus CONTENT input {family} {i}"
    );
    assert_eq!(
        *source,
        h.record(family, i, input, result, before, alias, info)
    );
}
#[test]
fn account_native_fields_and_rejection() {
    let source = source();
    for i in 0..3 {
        let h = Fixture::new();
        if i == 1 {
            set(
                &h.borrow().account,
                "referrerID",
                obj(vec![("code", num(7.0))]),
            );
        }
        let failure = error("account denied");
        if i == 2 {
            h.borrow_mut().account_error = Some((failure.clone(), false));
        }
        let input = enc(&h.borrow().account);
        let p = get_account(h.clone());
        let before = h.borrow().trace.clone();
        let result = run(p);
        if i == 2 {
            assert!(
                Rc::ptr_eq(result.as_ref().unwrap_err(), &failure),
                "READ native original account error"
            );
        } else {
            let r = result.as_ref().unwrap();
            n(r, "userId", 42.0);
            let a = field(r, "affiliateId");
            if i == 0 {
                assert!(
                    matches!(*a.borrow(), Js::Null),
                    "READ native falsy affiliate"
                );
            } else {
                assert!(
                    Rc::ptr_eq(&a, &field(&h.borrow().account, "referrerID")),
                    "READ native affiliate object alias"
                );
            }
        }
        check(
            &source[i],
            &h.borrow(),
            "account",
            i,
            input,
            &result,
            before,
            false,
            false,
        );
    }
}
#[test]
fn validation_native_routes_best_effort_and_eager_failures() {
    let source = source();
    for i in 0..6 {
        let h = Fixture::new();
        if i == 1 {
            h.borrow_mut().secondary = "secondary".into();
        }
        let e = error("top");
        set(
            &e,
            "response",
            obj(vec![(
                "data",
                obj(vec![
                    ("message", strv("nested")),
                    ("msg", strv("alternate")),
                ]),
            )]),
        );
        if i == 2 || i == 5 {
            h.borrow_mut().http_error = Some((e, i == 5));
        }
        if i == 3 || i == 4 {
            h.borrow_mut().balance_error = Some((
                error(if i == 4 {
                    "balance call failed"
                } else {
                    "balance rejected"
                }),
                i == 4,
            ));
        }
        let input = json!({"secondary":h.borrow().secondary,"outcome":match i{2=>"primary-reject",5=>"primary-throw",3=>"balance-reject",4=>"balance-throw",_=>"ok"}});
        let p = validate_account(h.clone());
        let before = h.borrow().trace.clone();
        let result = run(p);
        let r = result.as_ref().unwrap();
        s(
            r,
            "error",
            match i {
                2 | 5 => "nested",
                4 => "balance call failed",
                _ => "",
            },
        );
        assert_eq!(
            h.borrow().requests.len(),
            if i == 1 { 2 } else { 1 },
            "READ native validation request count"
        );
        for (_, p) in &h.borrow().requests {
            assert_eq!(p, "/v3/accountinfo", "READ native validation endpoint");
        }
        if i == 3 {
            assert!(
                matches!(*field(r, "balance").borrow(), Js::U),
                "READ native rejected balance becomes undefined"
            );
        }
        if i == 5 {
            assert_eq!(
                before,
                vec!["get:primary", "emit"],
                "READ native synchronous validation catch"
            );
        }
        let alias = Rc::ptr_eq(&field(r, "balance"), &h.borrow().balance);
        assert_eq!(
            alias,
            i == 0 || i == 1,
            "READ native validated balance alias"
        );
        check(
            &source[3 + i],
            &h.borrow(),
            "validate",
            i,
            input,
            &result,
            before,
            alias,
            false,
        );
    }
}
#[test]
fn balance_native_rounding_modes_aliases_and_getter_reads() {
    let source = source();
    for i in 0..6 {
        let h = Fixture::new();
        if i == 1 {
            let a = h.borrow().account.clone();
            set(&a, "positionMode", strv("HEDGE_MODE"));
            set(&a, "freeCollateral", num(-0.004));
            set(&a, "totalAccountValue", num(-0.0));
            set(&a, "totalCollateral", num(-0.009));
        }
        if i == 2 {
            set(&h.borrow().account, "accountMode", strv("SPOT"));
        }
        if i == 3 || i == 4 {
            h.borrow_mut().account_error = Some((
                error(if i == 4 { "sync info" } else { "info rejected" }),
                i == 4,
            ));
        }
        if i == 5 {
            h.borrow_mut().free_getter = true;
        }
        let input = json!({"case":i,"data":if i==5{descriptor(&h.borrow().account,"freeCollateral")}else{enc(&h.borrow().account)},"freeGetter":i==5});
        let p = fetch_balance(h.clone());
        let before = h.borrow().trace.clone();
        let result = run(p);
        let r = result.as_ref().unwrap();
        let alias = Rc::ptr_eq(r, &h.borrow().balance);
        assert_eq!(
            alias,
            (2..=4).contains(&i),
            "READ native balance fallback alias {i}"
        );
        if !alias {
            n(r, "free", if i == 1 { -0.0 } else { 1.24 });
            n(r, "total", if i == 1 { -0.0 } else { 10.56 });
            n(
                r,
                "used",
                if i == 1 {
                    -0.0
                } else if i == 5 {
                    5.99
                } else {
                    7.1
                },
            );
            n(r, "upnl", 9.25);
            assert_eq!(
                h.borrow().mode,
                Some(if i == 1 {
                    PositionMode::Hedged
                } else {
                    PositionMode::OneWay
                }),
                "READ native balance mode"
            );
        }
        if i == 5 {
            assert_eq!(
                h.borrow().free_reads,
                2,
                "READ native free collateral repeated getter"
            );
        }
        if i == 4 {
            assert_eq!(
                before,
                vec!["account", "emit"],
                "READ native synchronous balance catch"
            );
        }
        check(
            &source[9 + i],
            &h.borrow(),
            "balance",
            i,
            input,
            &result,
            before,
            alias,
            false,
        );
    }
}
#[test]
fn markets_native_copy_fields_leverage_sparse_filter_and_fallback() {
    let source = source();
    for i in 0..4 {
        let h = Fixture::new();
        let data = h.borrow().rows.clone();
        if i == 1 {
            let Js::A(a) = &mut *data.borrow_mut() else {
                panic!()
            };
            a[0] = None;
        }
        let selected = if i == 2 { val(Js::Null) } else { data.clone() };
        h.borrow_mut().response = obj(vec![("data", obj(vec![("rows", selected.clone())]))]);
        if i == 3 {
            h.borrow_mut().http_error = Some((error("market throw"), true));
        }
        let input = json!({"rows":enc(&selected),"sync":i==3});
        let p = fetch_markets(h.clone());
        let before = h.borrow().trace.clone();
        let result = run(p);
        let r = result.as_ref().unwrap();
        let alias = Rc::ptr_eq(r, &h.borrow().markets);
        assert_eq!(alias, i >= 2, "READ native market fallback alias");
        let mut info = false;
        if !alias {
            let a = rows(r);
            assert_eq!(a.len(), 2, "READ native market count");
            s(&a[0], "id", "BTC/USDT:USDT");
            s(&a[0], "symbol", "BTCUSDT");
            assert!(
                matches!(*field(&a[0], "active").borrow(), Js::B(true)),
                "READ native active bool"
            );
            let precision = field(&a[0], "precision");
            s(&precision, "amount", "0.125");
            n(&precision, "price", 0.25);
            let lev = field(&field(&a[0], "limits"), "leverage");
            n(&lev, "min", 1.0);
            n(&lev, "max", 100.0);
            n(&field(&field(&a[1], "limits"), "leverage"), "max", 40.0);
            let original = h.borrow_mut().index(&data, 1).unwrap();
            info = Rc::ptr_eq(&field(&a[0], "info"), &original);
            assert!(info, "READ native market info alias");
        }
        check(
            &source[15 + i],
            &h.borrow(),
            "markets",
            i,
            input,
            &result,
            before,
            alias,
            info,
        );
    }
}
#[test]
fn ticker_raw_native_identity_and_failures() {
    let source = source();
    for i in 0..3 {
        let h = Fixture::new();
        let data = arr(vec![ticker("PERP_BTC_USDT")]);
        h.borrow_mut().response = obj(vec![("data", obj(vec![("rows", data.clone())]))]);
        if i > 0 {
            h.borrow_mut().http_error = Some((
                error(if i == 2 { "raw throw" } else { "raw reject" }),
                i == 2,
            ));
        }
        let input = enc(&data);
        let p = fetch_tickers_raw(h.clone());
        let before = h.borrow().trace.clone();
        let result = run(p);
        let r = result.as_ref().unwrap();
        let alias = Rc::ptr_eq(r, &data);
        assert_eq!(alias, i == 0, "READ native raw rows alias");
        if i > 0 {
            assert!(
                matches!(*r.borrow(), Js::Null),
                "READ native raw failure null"
            );
        }
        check(
            &source[19 + i],
            &h.borrow(),
            "raw",
            i,
            input,
            &result,
            before,
            alias,
            false,
        );
    }
}
#[test]
fn ticker_wrapper_native_identity_and_rejection() {
    let source = source();
    for i in 0..2 {
        let h = Fixture::new();
        h.borrow_mut().rows = arr(vec![ticker("PERP_BTC_USDT")]);
        h.borrow_mut().parse_identity = true;
        let e = error("raw call rejected");
        if i == 1 {
            h.borrow_mut().raw_error = Some(e.clone());
        }
        let input = enc(&h.borrow().rows);
        let p = fetch_tickers(h.clone());
        let before = h.borrow().trace.clone();
        let result = run(p);
        let alias = result
            .as_ref()
            .is_ok_and(|r| Rc::ptr_eq(r, &h.borrow().rows));
        if i == 0 {
            assert!(alias, "READ native ticker parse return alias");
            assert_eq!(
                h.borrow().trace,
                vec!["raw", "parse"],
                "READ native ticker wrapper sequence"
            );
        } else {
            assert!(
                Rc::ptr_eq(result.as_ref().unwrap_err(), &e),
                "READ native raw wrapper error identity"
            );
        }
        check(
            &source[28 + i],
            &h.borrow(),
            "tickers",
            i,
            input,
            &result,
            before,
            alias,
            false,
        );
    }
}
#[test]
fn parsed_ticker_native_values_fallback_and_live_length() {
    let source = source();
    for i in 0..6 {
        let h = Fixture::new();
        let data = match i {
            1 => val(Js::Null),
            2 => obj(vec![("bad", val(Js::B(true)))]),
            _ => arr(vec![ticker("PERP_BTC_USDT"), ticker("PERP_UNKNOWN_USDT")]),
        };
        if i == 3 {
            let Js::A(a) = &mut *data.borrow_mut() else {
                panic!()
            };
            a[1] = None;
        }
        if i == 4 {
            let row = h.borrow_mut().index(&data, 0).unwrap();
            set(&row, "24h_open", num(0.0));
        }
        if i == 5 {
            h.borrow_mut().live_getter = true;
        }
        h.borrow_mut().rows = data.clone();
        let input = json!({"case":i,"data":if i==5{descriptor(&data,"symbol")}else{enc(&data)},"liveGetter":i==5});
        let result = parse_tickers(&mut *h.borrow_mut(), data);
        let alias = Rc::ptr_eq(result.as_ref().unwrap(), &h.borrow().tickers);
        assert_eq!(
            alias,
            (1..=3).contains(&i),
            "READ native parse fallback alias {i}"
        );
        if !alias {
            let a = rows(result.as_ref().unwrap());
            assert_eq!(
                a.len(),
                if i == 5 { 2 } else { 1 },
                "READ native ticker count"
            );
            let r = &a[0];
            s(r, "id", "BTC/USDT:USDT");
            s(r, "symbol", "BTCUSDT");
            for key in ["bid", "ask", "last"] {
                n(r, key, 10.0);
            }
            s(r, "index", "9.25");
            n(r, "mark", 9.75);
            n(r, "percentage", if i == 4 { f64::INFINITY } else { 25.0 });
            n(r, "fundingRate", -0.0);
            n(r, "fundingTime", 1234.0);
            n(r, "volume", 4.0);
            n(r, "quoteVolume", 40.0);
            n(r, "openInterest", 5.0);
            assert!(
                Rc::ptr_eq(&field(r, "bid"), &field(r, "last")),
                "READ native copied close alias"
            );
        }
        check(
            &source[22 + i],
            &h.borrow(),
            "parse",
            i,
            input,
            &result,
            vec![],
            alias,
            false,
        );
    }
}
#[test]
fn positions_balance_native_mutation_alias_and_sequential_reads() {
    let source = source();
    for i in 0..4 {
        let h = Fixture::new();
        if i == 1 {
            h.borrow_mut().positions = arr(vec![]);
        }
        if i == 2 {
            h.borrow_mut().positions = arr(vec![
                obj(vec![("unrealizedPnl", strv("1"))]),
                obj(vec![("unrealizedPnl", strv("2"))]),
            ]);
        }
        let e = error("positions rejected");
        if i == 3 {
            h.borrow_mut().positions_error = Some(e.clone());
        }
        let input = json!({"positions":enc(&h.borrow().positions),"balance":enc(&h.borrow().balance),"reject":i==3});
        let p = fetch_positions_and_balance(h.clone());
        let before = h.borrow().trace.clone();
        assert_eq!(
            before,
            vec!["balance"],
            "READ native positions waits for balance"
        );
        let result = run(p);
        assert_eq!(
            h.borrow().trace,
            vec!["balance", "positions"],
            "READ native sequential combined reads"
        );
        let alias = result
            .as_ref()
            .is_ok_and(|r| Rc::ptr_eq(&field(r, "balance"), &h.borrow().balance));
        if i == 3 {
            assert!(Rc::ptr_eq(result.as_ref().unwrap_err(), &e));
            n(&h.borrow().balance, "total", 50.0);
            n(&h.borrow().balance, "upnl", 9.25);
        } else {
            assert!(alias, "READ native combined balance alias");
            let b = field(result.as_ref().unwrap(), "balance");
            n(
                &b,
                "upnl",
                match i {
                    1 => 0.0,
                    2 => 12.0,
                    _ => 3.58,
                },
            );
            n(
                &b,
                "total",
                match i {
                    1 => 50.0,
                    2 => 38.0,
                    _ => 46.42,
                },
            );
            assert!(
                Rc::ptr_eq(
                    &field(result.as_ref().unwrap(), "positions"),
                    &h.borrow().positions
                ),
                "READ native combined positions alias"
            );
        }
        check(
            &source[30 + i],
            &h.borrow(),
            "combined",
            i,
            input,
            &result,
            before,
            alias,
            false,
        );
    }
}
