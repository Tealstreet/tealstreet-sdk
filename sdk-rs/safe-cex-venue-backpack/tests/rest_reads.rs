type Array<V> = std::rc::Rc<std::cell::RefCell<Vec<Option<V>>>>;
use safe_cex_types::{MarginType, OrderSide, OrderType, PositionMode, PositionSide};
use safe_cex_venue_backpack::{public_control::JsString, rest_reads::*};
use serde_json::{Value as Json, json};
use std::{
    cell::RefCell,
    process::Command,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
#[derive(Clone, Debug)]
enum V {
    U,
    Null,
    B(bool),
    N(f64),
    S(JsString),
    O(Rc<RefCell<Vec<(String, V)>>>),
    A(Array<V>),
}
fn u(s: &str) -> JsString {
    s.encode_utf16().collect()
}
fn odd() -> JsString {
    let mut s = u("λ");
    s.push(0xd800);
    s.extend(u("_X_Y"));
    s
}
fn value(v: Json) -> V {
    match v {
        Json::Null => V::Null,
        Json::Bool(b) => V::B(b),
        Json::Number(n) => V::N(n.as_f64().unwrap()),
        Json::String(s) => V::S(if s == "ODD" {
            odd()
        } else if s == "ODDTRADE" {
            vec![955, 0xd800]
        } else {
            u(&s)
        }),
        Json::Array(a) => V::A(Rc::new(RefCell::new(
            a.into_iter().map(|v| Some(value(v))).collect(),
        ))),
        Json::Object(o) => V::O(Rc::new(RefCell::new(
            o.into_iter().map(|(k, v)| (k, value(v))).collect(),
        ))),
    }
}
fn fld(v: &V, k: &str) -> V {
    match v {
        V::O(o) => o
            .borrow()
            .iter()
            .rev()
            .find(|(key, _)| key == k)
            .map_or(V::U, |(_, v)| v.clone()),
        _ => V::U,
    }
}
fn text(v: &V) -> JsString {
    match v {
        V::U => u("undefined"),
        V::Null => u("null"),
        V::B(b) => u(if *b { "true" } else { "false" }),
        V::N(n) => u(&safe_cex_num::js_number_string(*n)),
        V::S(s) => s.clone(),
        V::O(_) => u("[object Object]"),
        V::A(_) => panic!("unused array coercion"),
    }
}
fn truth(v: &V) -> bool {
    match v {
        V::U | V::Null => false,
        V::B(b) => *b,
        V::N(n) => *n != 0.0 && !n.is_nan(),
        V::S(s) => !s.is_empty(),
        _ => true,
    }
}
fn same(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::U, V::U) | (V::Null, V::Null) => true,
        (V::B(a), V::B(b)) => a == b,
        (V::N(a), V::N(b)) => a == b,
        (V::S(a), V::S(b)) => a == b,
        (V::O(a), V::O(b)) => Rc::ptr_eq(a, b),
        (V::A(a), V::A(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
fn bits(n: f64) -> String {
    format!(
        "{:016x}",
        if n.is_nan() {
            f64::NAN.to_bits()
        } else {
            n.to_bits()
        }
    )
}
fn num(n: f64, v: &Json, name: &str) {
    assert_eq!(bits(n), v["n"].as_str().unwrap(), "{name} native bits");
}
fn str_assert(s: &[u16], v: &Json, name: &str) {
    assert_eq!(
        s,
        serde_json::from_value::<Vec<u16>>(v["s"].clone()).unwrap(),
        "{name} native UTF16"
    );
}
fn native(v: &V, e: &Json, name: &str) {
    match v {
        V::U => assert_eq!(e, &json!({"u":true}), "{name} native undefined"),
        V::Null => assert!(e.is_null(), "{name} native null"),
        V::B(b) => assert_eq!(Some(*b), e.as_bool(), "{name} native bool"),
        V::N(n) => num(*n, e, name),
        V::S(s) => str_assert(s, e, name),
        V::A(a) => {
            let a = a.borrow();
            let e = e.as_array().unwrap();
            assert_eq!(a.len(), e.len(), "{name} native array count");
            for (v, e) in a.iter().zip(e) {
                match v {
                    Some(v) => native(v, e, name),
                    None => assert_eq!(e, &json!({"hole":true})),
                }
            }
        }
        V::O(o) => {
            let o = o.borrow();
            let e = e.as_object().unwrap();
            assert_eq!(o.len(), e.len(), "{name} native own property count");
            for (k, v) in &*o {
                native(v, e.get(k).unwrap(), &format!("{name}.{k}"));
            }
        }
    }
}
fn oracle_all() -> &'static Json {
    static O: OnceLock<Json> = OnceLock::new();
    O.get_or_init(|| {
        let out = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/rest_reads_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    })
}
fn oracle() -> &'static Vec<Json> {
    oracle_all()["rows"].as_array().unwrap()
}
fn run<T>(mut task: Task<T, Rc<str>>) -> Result<T, Rc<str>> {
    for _ in 0..12 {
        if let Poll::Ready(v) = task.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            return v;
        }
    }
    panic!("REST bounded driver completed by iteration 11");
}
#[derive(Clone)]
struct Fixtures {
    markets: V,
    tickers: V,
    marks: V,
    klines: V,
    depth: V,
    orders: V,
    positions: V,
    store: V,
    fills: V,
}
fn fixtures(i: usize) -> Fixtures {
    let markets = value(
        json!([{"symbol":"BTC_USDC","marketType":"PERP","imfFunction":{"base":"0.08"},"quantityIncrement":"0.125","priceIncrement":"0.75","minQuantity":"2","maxQuantity":"99","minNotional":"7","maxNotional":"987","filters":{"price":{"minPrice":"1","maxPrice":"800"}}},{"symbol":"ODD","visible":null,"marketType":"SPOT","filters":{"quantity":{"stepSize":"0.007","minQuantity":"0.3","maxQuantity":"17"},"price":{"tickSize":"0.2"}}},{"symbol":"HIDDEN_X","visible":false},{"symbol":"invalid"}]),
    );
    let tickers = value(
        json!([{"symbol":"BTC_USDC","lastPrice":"12.5","priceChangePercent":"0.125","volume":"8.25","quoteVolume":"105"},{"symbol":"ODD","lastPrice":"-0","priceChangePercent":"bad","volume":"Infinity","quoteVolume":"4tail"}]),
    );
    let marks = value(
        json!([{"symbol":"BTC_USDC","markPrice":"111","indexPrice":"222","fundingRate":"0.003","nextFundingTimestamp":123},{"symbol":"BTC_USDC","markPrice":"13.75","indexPrice":"14.5","fundingRate":"-0.002","nextFundingTimestamp":1700000000999.0}]),
    );
    let klines = value(
        json!([{"start":"2025-12-15 13:28:00","open":"11","high":"22","low":"3","close":"17","volume":"99"},{"start":"invalid","open":"-0","high":"bad","low":"Infinity","close":"4tail","volume":"0"}]),
    );
    let depth =
        value(json!({"bids":[["11","2"],["-0","bad"]],"asks":[["22","3"],["Infinity","4tail"]]}));
    let orders = value(
        json!([{"id":"old1","symbol":"BTC_USDC","side":"Bid","orderType":"Limit","triggerPrice":"15","triggerQuantity":"2","quantity":"9","price":"14","triggerBy":null},{"id":"old2","symbol":"BTC_USDC","side":"Ask","orderType":"Market","triggerPrice":"4","quantity":"3"},{"id":"keep","symbol":"ODD","triggerPrice":null}]),
    );
    let positions = value(
        json!([{"symbol":"BTC_USDC","netQuantity":"-2","entryPrice":"12","netExposureNotional":"0","pnlUnrealized":"3.75","estLiquidationPrice":"bad"},{"symbol":"NAN_X","netQuantity":"bad","entryPrice":"7","netExposureNotional":"88","pnlUnrealized":"-0","estLiquidationPrice":"9"},{"symbol":"BTC_USDC","netQuantity":"0"},{"symbol":"BTC_USDC","netQuantity":"-3","entryPrice":"77"}]),
    );
    let store = value(
        json!([{"id":"BTC_USDC","symbol":"BTC_USDC","active":true,"isSpot":false},{"id":"ODD","symbol":"ODD","active":true,"isSpot":false},{"id":"SPOT","symbol":"SPOT","active":true,"isSpot":true},{"id":"OFF","symbol":"OFF","active":false,"isSpot":false},{"id":"ODD","symbol":"ODD","active":true,"isSpot":false}]),
    );
    let valid = json!({"symbol":"BTC_USDC","orderId":"o1","timestamp":"1700000001","price":"7.5","quantity":"2.5","side":"Bid","fee":"-0.125","feeSymbol":"USDC","tradeId":123});
    let mut fills = vec![valid.clone()];
    let mut row = valid.clone();
    row["tradeId"] = json!("ODDTRADE");
    row["side"] = json!("Ask");
    row["timestamp"] = json!("2025-12-15T13:28:00Z");
    row["fee"] = json!("bad");
    row["feeSymbol"] = json!("");
    fills.push(row);
    for (k, v) in [
        ("symbol", "OTHER"),
        ("orderId", "other"),
        ("timestamp", "bad"),
        ("timestamp", "0"),
        ("timestamp", "1700000001123"),
        ("timestamp", "-1"),
        ("price", "0"),
        ("quantity", "Infinity"),
        ("side", "Other"),
    ] {
        let mut row = valid.clone();
        row[k] = json!(v);
        fills.push(row);
    }
    let fills = value(Json::Array(fills));
    let first = |a: &V| match a {
        V::A(a) => a.borrow()[0].clone().unwrap(),
        _ => panic!(),
    };
    if i == 1 {
        set(&first(&markets), "quantityIncrement", V::S(u("")));
        set(&fld(&first(&markets), "imfFunction"), "base", V::S(u("0")));
        for a in [&klines, &tickers, &fills, &orders, &positions] {
            if let V::A(a) = a {
                a.borrow_mut().clear();
            }
        }
        if let V::A(a) = fld(&depth, "bids") {
            a.borrow_mut().clear();
        }
    }
    if i == 2 {
        set(
            &fld(&first(&markets), "imfFunction"),
            "base",
            V::S(u("2.2204460492503126e-16")),
        );
        set(&first(&markets), "priceIncrement", V::S(u("-0")));
        set(&first(&orders), "triggerBy", V::S(u("LastPrice")));
    }
    if i == 3 {
        set(&first(&markets), "marketType", V::S(u("SPOT")));
        set(
            &fld(&first(&markets), "imfFunction"),
            "base",
            V::S(u("1e-320")),
        );
        set(&first(&orders), "triggerQuantity", V::S(u("")));
    }
    if i == 11 {
        array(&marks).borrow_mut()[0] = None;
    }
    Fixtures {
        markets,
        tickers,
        marks,
        klines,
        depth,
        orders,
        positions,
        store,
        fills,
    }
}
fn set(v: &V, k: &str, n: V) {
    if let V::O(o) = v {
        let mut o = o.borrow_mut();
        if let Some((_, v)) = o.iter_mut().find(|(key, _)| key == k) {
            *v = n;
        } else {
            o.push((k.into(), n));
        }
    } else {
        panic!()
    }
}
fn array(v: &V) -> Array<V> {
    if let V::A(a) = v { a.clone() } else { panic!() }
}
struct H {
    family: &'static str,
    i: usize,
    f: Fixtures,
    calls: Vec<String>,
    requests: Vec<(&'static str, Request<V>)>,
    logs: Vec<(JsString, Severity)>,
    original: Rc<str>,
    logger: Rc<str>,
    repair_host: Option<std::rc::Weak<RefCell<H>>>,
    witness: Option<&'static str>,
    reads: usize,
    created: Vec<V>,
    mapped: Vec<V>,
}
impl H {
    fn new(family: &'static str, i: usize) -> Rc<RefCell<Self>> {
        let h = Rc::new(RefCell::new(Self {
            family,
            i,
            f: fixtures(i),
            calls: vec![],
            requests: vec![],
            logs: vec![],
            original: Rc::from(if i == 8 {
                "endpoint not found"
            } else {
                "failedλ"
            }),
            logger: Rc::from("loggerFailed"),
            repair_host: None,
            witness: None,
            reads: 0,
            created: vec![],
            mapped: vec![],
        }));
        h.borrow_mut().repair_host = Some(Rc::downgrade(&h));
        h
    }
    fn invoke(
        &mut self,
        method: &'static str,
        req: Request<V>,
    ) -> Result<Reply<V, Rc<str>>, Rc<str>> {
        self.calls.push(format!("{method}:{}", req.endpoint));
        self.requests.push((method, req.clone()));
        if self.i == 4 {
            return Err(self.original.clone());
        }
        if self.i == 5
            || (self.i == 8 && ["/api/v1/markPrices", "/api/v1/position"].contains(&req.endpoint))
        {
            return Ok(Box::pin(std::future::ready(Err(self.original.clone()))));
        }
        let v = if method == "delete" {
            value(json!({}))
        } else if method == "post" {
            let o = V::O(Rc::new(RefCell::new(vec![])));
            for (k, v) in req.data {
                set(&o, k, param(v));
            }
            let p = String::from_utf16_lossy(&text(&fld(&o, "triggerPrice")));
            set(&o, "id", V::S(u(&format!("new-{p}"))));
            self.created.push(o.clone());
            o
        } else {
            match req.endpoint {
                "/api/v1/markets" => self.f.markets.clone(),
                "/api/v1/tickers" => self.f.tickers.clone(),
                "/api/v1/markPrices" => self.f.marks.clone(),
                "/api/v1/klines" => self.f.klines.clone(),
                "/api/v1/depth" => self.f.depth.clone(),
                "/wapi/v1/history/fills" => self.f.fills.clone(),
                "/api/v1/orders" => self.f.orders.clone(),
                "/api/v1/position" => self.f.positions.clone(),
                _ => panic!(),
            }
        };
        let v = if self.i == 11 && method == "get" && ["ohlcv", "fills"].contains(&self.family) {
            value(json!({"bad":true}))
        } else {
            v
        };
        Ok(Box::pin(std::future::ready(Ok(v))))
    }
}
fn param(v: Param<V>) -> V {
    match v {
        Param::Value(v) => v,
        Param::Number(n) => V::N(n),
        Param::Text(s) => V::S(s),
        Param::Bool(b) => V::B(b),
        Param::Undefined => V::U,
    }
}
impl Host for H {
    type Array = Array<V>;
    fn array_length(&mut self, a: &Array<V>) -> Result<usize, Rc<str>> {
        Ok(a.borrow().len())
    }
    fn array_element(&mut self, a: &Array<V>, index: usize) -> Result<Option<V>, Rc<str>> {
        Ok(a.borrow().get(index).cloned().flatten())
    }
    fn array_set(&mut self, a: &Array<V>, index: usize, value: V) -> Result<(), Rc<str>> {
        let mut a = a.borrow_mut();
        // JS indexed assignment can regrow an array cleared by a live getter.
        if index >= a.len() {
            a.resize_with(index + 1, || None);
        }
        a[index] = Some(value);
        Ok(())
    }
    fn array_from(&mut self, values: Vec<Option<V>>) -> Array<V> {
        Rc::new(RefCell::new(values))
    }
    type Value = V;
    type Error = Rc<str>;
    type LogTarget = ();
    type Order = V;
    type TransportTarget = ();
    type TriggerTarget = ();
    fn transport_target(&mut self, _: &str) -> Result<(), Rc<str>> {
        Ok(())
    }
    fn trigger_target(&mut self) -> Result<(), Rc<str>> {
        Ok(())
    }
    fn field(&mut self, v: &V, k: &str) -> Result<V, Rc<str>> {
        if self.witness == Some("imf") && k == "imfFunction" {
            self.reads += 1;
            return Ok(value(json!({"base":if self.reads==1{"0.01"}else{"0.04"}})));
        }
        if self.witness == Some("quote") && k == "symbol" {
            self.reads += 1;
            return Ok(V::S(u(if self.reads == 1 { "BTC_USDC" } else { "BTC" })));
        }
        if self.witness == Some("shrink")
            && k == "id"
            && matches!(fld(v,"id"),V::S(s) if s==u("old1"))
        {
            self.reads += 1;
            if self.reads == 4 {
                array(&self.f.orders).borrow_mut().clear();
            }
            return Ok(V::S(u("old1")));
        }
        if matches!(v, V::U | V::Null) {
            Err(Rc::from(format!(
                "TypeError: Cannot read properties of undefined (reading '{k}')"
            )))
        } else {
            Ok(fld(v, k))
        }
    }
    fn array(&mut self, v: &V) -> Result<Array<V>, Rc<str>> {
        if let V::A(a) = v {
            Ok(a.clone())
        } else {
            Err(Rc::from("not array"))
        }
    }
    fn is_array(&mut self, v: &V) -> bool {
        matches!(v, V::A(_))
    }
    fn truthy(&mut self, v: &V) -> bool {
        truth(v)
    }
    fn nullish(&mut self, v: &V) -> bool {
        matches!(v, V::U | V::Null)
    }
    fn strict_equal(&mut self, a: &V, b: &V) -> bool {
        same(a, b)
    }
    fn is_text(&mut self, v: &V, s: &str) -> bool {
        matches!(v,V::S(t)if t==&u(s))
    }
    fn is_false(&mut self, v: &V) -> bool {
        matches!(v, V::B(false))
    }
    fn string(&mut self, v: &V) -> Result<JsString, Rc<str>> {
        Ok(text(v))
    }
    fn parse_float(&mut self, v: &V) -> Result<f64, Rc<str>> {
        Ok(safe_cex_num::parse_float(&String::from_utf16_lossy(&text(
            v,
        ))))
    }
    fn number(&mut self, v: &V) -> Result<f64, Rc<str>> {
        Ok(String::from_utf16_lossy(&text(v))
            .parse()
            .unwrap_or(f64::NAN))
    }
    fn undefined(&mut self) -> V {
        V::U
    }
    fn zero(&mut self) -> V {
        V::N(0.0)
    }
    fn is_undefined(&mut self, v: &V) -> bool {
        matches!(v, V::U)
    }
    fn is_null(&mut self, v: &V) -> bool {
        matches!(v, V::Null)
    }
    fn pair(&mut self, v: &V) -> Result<(V, V), Rc<str>> {
        let a = self.array(v)?;
        let a = a.borrow();
        Ok((
            a.first().cloned().flatten().unwrap_or(V::U),
            a.get(1).cloned().flatten().unwrap_or(V::U),
        ))
    }
    fn spread(&mut self, values: &[V]) -> Result<V, Rc<str>> {
        let out = V::O(Rc::new(RefCell::new(vec![])));
        for v in values {
            if let V::O(o) = v {
                for (k, v) in &*o.borrow() {
                    set(&out, k, v.clone());
                }
            }
        }
        Ok(out)
    }
    fn get(&mut self, _: (), r: Request<V>) -> Result<Reply<V, Rc<str>>, Rc<str>> {
        self.invoke("get", r)
    }
    fn delete(&mut self, _: (), r: Request<V>) -> Result<Reply<V, Rc<str>>, Rc<str>> {
        self.invoke("delete", r)
    }
    fn post(&mut self, _: (), r: Request<V>) -> Result<Reply<V, Rc<str>>, Rc<str>> {
        self.invoke("post", r)
    }
    fn log_target(&mut self) -> Result<(), Rc<str>> {
        Ok(())
    }
    fn log(&mut self, _: (), s: JsString, v: Severity) -> Result<(), Rc<str>> {
        self.calls.push("log".into());
        if self.i == 6 || self.i == 7 {
            return Err(self.logger.clone());
        }
        self.logs.push((s, v));
        Ok(())
    }
    fn error_text(&mut self, e: &Rc<str>) -> Result<JsString, Rc<str>> {
        Ok(u(e))
    }
    fn error_message(&mut self, e: &Rc<str>) -> Result<Option<JsString>, Rc<str>> {
        Ok(Some(u(e)))
    }
    fn stringify(&mut self, v: &V) -> Result<JsString, Rc<str>> {
        if matches!(v, V::O(_)) {
            Ok(u("{\"bad\":true}"))
        } else {
            panic!()
        }
    }
    fn error(&mut self, s: &str) -> Rc<str> {
        Rc::from(s)
    }
    fn date_parse(&mut self, s: JsString) -> Result<f64, Rc<str>> {
        Ok(match String::from_utf16_lossy(&s).as_str() {
            "2025-12-15 13:28:00Z" | "2025-12-15T13:28:00Z" => 1765805280000.0,
            "0" => 946684800000.0,
            "-1" => 978307200000.0,
            _ => f64::NAN,
        })
    }
    fn clock(&mut self) -> Result<f64, Rc<str>> {
        self.calls.push("clock".into());
        Ok(1700000000123.0)
    }
    fn public_only(&mut self) -> Result<bool, Rc<str>> {
        Ok(self.i == 9)
    }
    fn market(&mut self, s: &[u16]) -> Result<Option<V>, Rc<str>> {
        Ok(if s == u("BTC_USDC") {
            Some(array(&self.f.store).borrow()[0].clone().unwrap())
        } else {
            None
        })
    }
    fn markets(&mut self) -> Result<Array<V>, Rc<str>> {
        Ok(array(&self.f.store))
    }
    fn position_mode(&mut self) -> Result<Task<PositionMode, Rc<str>>, Rc<str>> {
        self.calls.push("mode".into());
        Ok(Box::pin(std::future::ready(Ok(PositionMode::Hedged))))
    }
    fn leverage(&mut self) -> Result<f64, Rc<str>> {
        Ok(8.5)
    }
    fn position_id(
        &mut self,
        s: &[u16],
        side: PositionSide,
        _: PositionMode,
    ) -> Result<JsString, Rc<str>> {
        let side = if side == PositionSide::Long {
            "long"
        } else {
            "short"
        };
        self.calls.push(format!("id:{side}"));
        let mut id = s.to_vec();
        id.extend(u(&format!("|{side}|hedged")));
        Ok(id)
    }
    fn repair(&mut self, a: Array<V>) -> Result<Task<(), Rc<str>>, Rc<str>> {
        self.calls.push("repair".into());
        let h = self.repair_host.as_ref().unwrap().upgrade().unwrap();
        Ok(Box::pin(async move {
            fix_trigger_orders_without_trigger_by(h, a).await
        }))
    }
    fn map_order(&mut self, v: V) -> Result<V, Rc<str>> {
        self.calls.push("map".into());
        self.mapped.push(v.clone());
        Ok(v)
    }
    fn trigger_type(&mut self, _: (), _: V, s: OrderSide, p: f64) -> Result<OrderType, Rc<str>> {
        self.calls.push(format!(
            "trigger:{}",
            if s == OrderSide::Buy { "buy" } else { "sell" }
        ));
        Ok(if p > 10.0 {
            OrderType::TakeProfit
        } else {
            OrderType::StopLoss
        })
    }
    fn trigger_by(&mut self, tp: bool) -> Result<JsString, Rc<str>> {
        self.calls
            .push(format!("by:{}", if tp { "tp" } else { "sl" }));
        Ok(u(if tp { "MarkPrice" } else { "LastPrice" }))
    }
}
fn opts(i: usize) -> OHLCVOptions {
    OHLCVOptions {
        symbol: u(if i == 10 {
            "MISSING"
        } else if i == 9 {
            ""
        } else {
            "BTC_USDC"
        }),
        interval: u("1m"),
        from: Some([-0.0, 1700000001.25, f64::NAN, -1234.0, f64::INFINITY][i % 5]),
        to: [
            Some(1700000001123.75),
            Some(f64::NEG_INFINITY),
            Some(0.0),
            None,
            Some(12.0),
        ][i % 5],
    }
}
fn fills_opts(i: usize) -> ExecutionsOptions {
    let o = opts(i);
    ExecutionsOptions {
        symbol: Some(o.symbol),
        order_id: i.is_multiple_of(2).then(|| u("o1")),
        from: o.from,
        to: o.to,
        limit: [None, Some(-2.0), Some(2000.0), Some(f64::NAN), Some(0.0)][i % 5],
    }
}
fn pin(family: &str) -> Vec<&'static Json> {
    let corpus = oracle();
    assert_eq!(corpus.len(), 96, "REST independent corpus size");
    let families = [
        "markets",
        "tickers",
        "ohlcv",
        "depth",
        "fills",
        "orders",
        "repair",
        "positions",
    ];
    let modes = [
        "none",
        "empty",
        "special",
        "fallback",
        "throw",
        "reject",
        "logger",
        "log",
        "marksReject",
        "public",
        "missing",
        "notArray",
    ];
    for (f, row) in corpus.as_chunks::<12>().0.iter().enumerate() {
        for (i, row) in row.iter().enumerate() {
            assert_eq!(row["family"], families[f], "REST independent corpus family");
            assert_eq!(row["i"], i, "REST independent corpus iteration");
            assert_eq!(row["mode"], modes[i], "REST independent corpus mode");
            let o = opts(i);
            str_assert(
                &o.symbol,
                &row["opts"]["symbol"],
                "REST independent input symbol",
            );
            str_assert(
                &o.interval,
                &row["opts"]["interval"],
                "REST independent input interval",
            );
            num(
                o.from.unwrap(),
                &row["opts"]["from"],
                "REST independent input from",
            );
            if let Some(n) = o.to {
                num(n, &row["opts"]["to"], "REST independent input to")
            } else {
                assert_eq!(row["opts"]["to"], json!({"u":true}));
            }
            let o = fills_opts(i);
            if let Some(n) = o.limit {
                num(n, &row["opts"]["limit"], "REST independent input limit")
            } else {
                assert_eq!(row["opts"]["limit"], json!({"u":true}));
            }
            if let Some(s) = o.order_id {
                str_assert(
                    &s,
                    &row["opts"]["orderId"],
                    "REST independent input orderId",
                )
            } else {
                assert_eq!(row["opts"]["orderId"], json!({"u":true}));
            }
        }
    }
    corpus.iter().filter(|v| v["family"] == family).collect()
}
fn input(h: &Rc<RefCell<H>>, r: &Json) {
    let h = h.borrow();
    for (v, k) in [
        (&h.f.markets, "markets"),
        (&h.f.tickers, "tickers"),
        (&h.f.marks, "marks"),
        (&h.f.klines, "klines"),
        (&h.f.depth, "depth"),
        (&h.f.orders, "orders"),
        (&h.f.positions, "positions"),
        (&h.f.store, "storeMarkets"),
        (&h.f.fills, "fills"),
    ] {
        native(
            v,
            &r["inputs"][k],
            &format!("REST independent fixture input {k}"),
        );
    }
}
fn eager(h: &Rc<RefCell<H>>, r: &Json) {
    assert_eq!(
        json!(h.borrow().calls),
        r["before"],
        "REST native eager invocation sequence"
    );
}
fn outcome<T>(result: Result<T, Rc<str>>, h: &Rc<RefCell<H>>, r: &Json) -> Option<T> {
    match result {
        Ok(v) => {
            assert_eq!(r["outcome"], "ok", "REST native outcome");
            Some(v)
        }
        Err(e) => {
            assert_eq!(r["outcome"], "error", "REST native outcome");
            let h = h.borrow();
            let kind = if Rc::ptr_eq(&e, &h.original) {
                "original"
            } else if Rc::ptr_eq(&e, &h.logger) {
                "logger"
            } else {
                "generated"
            };
            assert_eq!(r["sameError"], kind, "REST native thrown identity");
            None
        }
    }
}
fn tail(h: &Rc<RefCell<H>>, r: &Json) {
    let h = h.borrow();
    let created = V::A(Rc::new(RefCell::new(
        h.created.iter().cloned().map(Some).collect(),
    )));
    native(&created, &r["created"], "REST native created replies");
    if h.family == "repair" {
        for v in &h.created {
            assert!(
                array(&h.f.orders)
                    .borrow()
                    .iter()
                    .flatten()
                    .any(|o| same(v, o)),
                "REPAIR native created order identity"
            );
        }
        assert!(
            r["createdAlias"]
                .as_array()
                .unwrap()
                .iter()
                .all(|v| v == true),
            "REPAIR source created order identity"
        );
    }
    native(
        &h.f.orders,
        &r["orders"],
        "REST native repaired input array",
    );
    assert_eq!(
        json!(h.calls),
        r["calls"],
        "REST native completed invocation sequence"
    );
    assert_eq!(
        h.logs.len(),
        r["logs"].as_array().unwrap().len(),
        "REST native log count"
    );
    for ((s, v), e) in h.logs.iter().zip(r["logs"].as_array().unwrap()) {
        str_assert(s, &e[0], "REST native log text");
        assert_eq!(
            e[1],
            match v {
                Severity::Info => "info",
                Severity::Warning => "warning",
                Severity::Error => "error",
            },
            "REST native log severity"
        );
    }
}
fn requests(h: &Rc<RefCell<H>>, r: &Json) {
    let h = h.borrow();
    let expected = r["requests"].as_array().unwrap();
    assert_eq!(h.requests.len(), expected.len(), "REST wire request count");
    for ((method, req), e) in h.requests.iter().zip(expected) {
        assert_eq!(*method, e["method"].as_str().unwrap());
        assert_eq!(req.endpoint, e["url"].as_str().unwrap());
        let check = |fields: &Vec<(&str, Param<V>)>, e: &Json| {
            assert_eq!(
                fields.len(),
                e.as_object().unwrap().len(),
                "REST wire field count"
            );
            for (k, v) in fields {
                native(&param(v.clone()), &e[*k], "REST wire field");
            }
        };
        if *method == "get" {
            if req.params.is_empty() {
                assert_eq!(e["params"], json!({"u":true}));
            } else {
                check(&req.params, &e["params"]);
            }
        } else {
            check(&req.data, &e["data"]);
        }
    }
}
fn dense<'a, T>(v: &'a [Option<T>], e: &'a Json) -> Vec<(&'a T, &'a Json)> {
    let e = e.as_array().unwrap();
    assert_eq!(v.len(), e.len(), "REST native mapped count");
    v.iter()
        .zip(e)
        .map(|(v, e)| (v.as_ref().unwrap(), e))
        .collect()
}
#[test]
fn markets() {
    let rows = pin("markets");
    let mut hosts = vec![];
    for (i, r) in rows.iter().enumerate() {
        let h = H::new("markets", i);
        input(&h, r);
        let t = fetch_markets(h.clone());
        eager(&h, r);
        if let Some(v) = outcome(run(t), &h, r) {
            let e = r["value"].as_array().unwrap();
            assert_eq!(v.len(), e.len(), "MARKETS native count");
            for (m, e) in v.iter().zip(e) {
                for (s, k) in [
                    (&m.id, "id"),
                    (&m.symbol, "symbol"),
                    (&m.base, "base"),
                    (m.quote.as_ref().unwrap(), "quote"),
                ] {
                    str_assert(s, &e[k], "MARKETS native text");
                }
                assert_eq!(
                    m.kind,
                    if e["type"]["s"] == json!([115, 119, 97, 112]) {
                        "swap"
                    } else {
                        "spot"
                    }
                );
                for (b, k) in [
                    (m.active, "active"),
                    (m.spot, "spot"),
                    (m.swap, "swap"),
                    (m.future, "future"),
                    (m.contract, "contract"),
                    (m.linear, "linear"),
                    (m.inverse, "inverse"),
                    (m.is_spot, "isSpot"),
                ] {
                    assert_eq!(Some(b), e[k].as_bool(), "MARKETS native flags");
                }
                assert!(
                    !e.as_object().unwrap().contains_key("option"),
                    "MARKETS source omits option metadata"
                );
                num(m.contract_size, &e["contractSize"], "MARKETS contractSize");
                for (n, k) in m.precision.iter().zip(["amount", "price", "base", "quote"]) {
                    num(*n, &e["precision"][k], &format!("MARKETS precision {k}"));
                }
                for (pair, k) in m.limits.iter().zip(["amount", "price", "cost", "leverage"]) {
                    num(pair[0], &e["limits"][k]["min"], "MARKETS limit min");
                    num(pair[1], &e["limits"][k]["max"], "MARKETS limit max");
                }
                native(&m.info, &e["info"], "MARKETS native info");
                assert!(
                    array(&h.borrow().f.markets)
                        .borrow()
                        .iter()
                        .flatten()
                        .any(|v| same(v, &m.info)),
                    "MARKETS original info identity"
                );
            }
            assert!(r["alias"].as_array().unwrap().iter().all(|v| v == true));
        }
        tail(&h, r);
        hosts.push(h);
    }
    for (h, r) in hosts.iter().zip(rows) {
        requests(h, r);
    }
}
#[test]
fn tickers() {
    let rows = pin("tickers");
    let mut hosts = vec![];
    for (i, r) in rows.iter().enumerate() {
        let h = H::new("tickers", i);
        input(&h, r);
        let t = fetch_tickers(h.clone());
        eager(&h, r);
        if let Some(v) = outcome(run(t), &h, r) {
            for (t, e) in dense(&v, &r["value"]) {
                str_assert(&t.id, &e["id"], "TICKERS id");
                str_assert(&t.symbol, &e["symbol"], "TICKERS symbol");
                for (n, k) in [
                    (t.bid, "bid"),
                    (t.ask, "ask"),
                    (t.last, "last"),
                    (t.mark, "mark"),
                    (t.index, "index"),
                    (t.percentage, "percentage"),
                    (t.funding_rate, "fundingRate"),
                    (t.volume, "volume"),
                    (t.quote_volume, "quoteVolume"),
                    (t.open_interest, "openInterest"),
                ] {
                    num(n, &e[k], &format!("TICKERS {k}"));
                }
                native(&t.funding_time, &e["fundingTime"], "TICKERS fundingTime");
                native(&t.info, &e["info"], "TICKERS info");
                assert!(
                    !array(&h.borrow().f.tickers)
                        .borrow()
                        .iter()
                        .flatten()
                        .any(|v| same(v, &t.info)),
                    "TICKERS fresh info identity"
                );
                assert!(
                    !array(&h.borrow().f.marks)
                        .borrow()
                        .iter()
                        .flatten()
                        .any(|v| same(v, &t.info)),
                    "TICKERS fresh mark info identity"
                );
            }
            assert!(r["alias"].as_array().unwrap().iter().all(|v| v == true));
        }
        tail(&h, r);
        hosts.push(h);
    }
    for (h, r) in hosts.iter().zip(rows) {
        requests(h, r);
    }
}
#[test]
fn ohlcv() {
    let rows = pin("ohlcv");
    let mut hosts = vec![];
    for (i, r) in rows.iter().enumerate() {
        let h = H::new("ohlcv", i);
        input(&h, r);
        let t = fetch_ohlcv(h.clone(), opts(i));
        eager(&h, r);
        if let Some(v) = outcome(run(t), &h, r) {
            for (c, e) in dense(&v, &r["value"]) {
                for (n, k) in [
                    (c.timestamp, "timestamp"),
                    (c.open, "open"),
                    (c.high, "high"),
                    (c.low, "low"),
                    (c.close, "close"),
                    (c.volume, "volume"),
                ] {
                    num(n, &e[k], &format!("OHLCV {k}"));
                }
            }
        }
        tail(&h, r);
        hosts.push(h);
    }
    for (h, r) in hosts.iter().zip(rows) {
        requests(h, r);
    }
}
#[test]
fn depth() {
    let rows = pin("depth");
    let mut hosts = vec![];
    for (i, r) in rows.iter().enumerate() {
        let h = H::new("depth", i);
        input(&h, r);
        let t = fetch_order_book(h.clone(), opts(i).symbol);
        eager(&h, r);
        if let Some(v) = outcome(run(t), &h, r) {
            for (side, k) in [(&v.bids, "bids"), (&v.asks, "asks")] {
                for (c, e) in dense(side, &r["value"][k]) {
                    str_assert(&c.id, &e["id"], "DEPTH id");
                    num(c.price, &e["price"], &format!("DEPTH {k} price"));
                    num(c.amount, &e["amount"], "DEPTH amount");
                    num(c.total, &e["total"], "DEPTH total");
                }
            }
        }
        tail(&h, r);
        hosts.push(h);
    }
    for (h, r) in hosts.iter().zip(rows) {
        requests(h, r);
    }
}
#[test]
fn fills() {
    let rows = pin("fills");
    let mut hosts = vec![];
    for (i, r) in rows.iter().enumerate() {
        let h = H::new("fills", i);
        input(&h, r);
        let t = fetch_historical_fills(h.clone(), fills_opts(i));
        eager(&h, r);
        if let Some(v) = outcome(run(t), &h, r) {
            let e = r["value"].as_array().unwrap();
            assert_eq!(v.len(), e.len(), "FILLS native count");
            for (c, e) in v.iter().zip(e) {
                for (s, k) in [
                    (&c.id, "id"),
                    (&c.symbol, "symbol"),
                    (&c.order_id, "orderId"),
                ] {
                    str_assert(s, &e[k], "FILLS native text");
                }
                for (n, k) in [
                    (c.timestamp, "timestamp"),
                    (c.price, "price"),
                    (c.qty, "qty"),
                ] {
                    num(n, &e[k], &format!("FILLS {k}"));
                }
                assert_eq!(
                    c.side,
                    if e["side"]["s"] == json!([98, 117, 121]) {
                        OrderSide::Buy
                    } else {
                        OrderSide::Sell
                    },
                    "FILLS native side"
                );
                match c.commission {
                    Some(n) => num(n, &e["commission"], "FILLS commission"),
                    None => assert_eq!(e["commission"], json!({"u":true})),
                }
                match &c.commission_asset {
                    Some(s) => str_assert(s, &e["commissionAsset"], "FILLS commissionAsset"),
                    None => assert_eq!(e["commissionAsset"], json!({"u":true})),
                }
            }
        }
        tail(&h, r);
        hosts.push(h);
    }
    for (h, r) in hosts.iter().zip(rows) {
        requests(h, r);
    }
}
#[test]
fn orders() {
    let rows = pin("orders");
    let mut hosts = vec![];
    for (i, r) in rows.iter().enumerate() {
        let h = H::new("orders", i);
        input(&h, r);
        let symbols = match i % 3 {
            0 => Some(vec![u("BTC_USDC")]),
            1 => Some(vec![]),
            _ => None,
        };
        let t = fetch_orders(h.clone(), symbols);
        eager(&h, r);
        if let Some(v) = outcome(run(t), &h, r) {
            assert!(
                r["mappedAlias"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|v| v == true)
            );
            for (v, e) in dense(&v, &r["value"]) {
                native(v, e, "ORDERS native mapped record");
                assert!(
                    h.borrow().mapped.iter().any(|mapped| same(v, mapped)),
                    "ORDERS native mapper result identity"
                );
            }
        }
        tail(&h, r);
        hosts.push(h);
    }
    for (h, r) in hosts.iter().zip(rows) {
        requests(h, r);
    }
}
#[test]
fn repair() {
    let rows = pin("repair");
    let mut hosts = vec![];
    for (i, r) in rows.iter().enumerate() {
        let h = H::new("repair", i);
        input(&h, r);
        let a = array(&h.borrow().f.orders);
        let t = fix_trigger_orders_without_trigger_by(h.clone(), a);
        eager(&h, r);
        outcome(run(t), &h, r);
        tail(&h, r);
        hosts.push(h);
    }
    for (h, r) in hosts.iter().zip(rows) {
        requests(h, r);
    }
}
#[test]
fn positions() {
    let rows = pin("positions");
    let mut hosts = vec![];
    for (i, r) in rows.iter().enumerate() {
        let h = H::new("positions", i);
        input(&h, r);
        let t = fetch_positions(h.clone());
        eager(&h, r);
        if let Some(v) = outcome(run(t), &h, r) {
            let e = r["value"].as_array().unwrap();
            assert_eq!(v.len(), e.len(), "POSITIONS native count");
            for (p, e) in v.iter().zip(e) {
                str_assert(&p.id, &e["id"], "POSITIONS id");
                str_assert(&p.symbol, &e["symbol"], "POSITIONS symbol");
                assert_eq!(
                    p.side,
                    if e["side"]["s"] == json!([108, 111, 110, 103]) {
                        PositionSide::Long
                    } else {
                        PositionSide::Short
                    },
                    "POSITIONS native side"
                );
                assert_eq!(p.position_mode, PositionMode::Hedged);
                assert_eq!(p.margin_type, MarginType::Cross);
                str_assert(&u("hedged"), &e["positionMode"], "POSITIONS mode");
                str_assert(&u("cross"), &e["marginType"], "POSITIONS margin");
                for (n, k) in [
                    (p.entry_price, "entryPrice"),
                    (p.notional, "notional"),
                    (p.leverage, "leverage"),
                    (p.unrealized_pnl, "unrealizedPnl"),
                    (p.contracts, "contracts"),
                    (p.liquidation_price, "liquidationPrice"),
                    (p.last_update, "lastUpdate"),
                ] {
                    num(n, &e[k], &format!("POSITIONS {k}"));
                }
            }
        }
        tail(&h, r);
        hosts.push(h);
    }
    for (h, r) in hosts.iter().zip(rows) {
        requests(h, r);
    }
}

#[test]
fn live_getter_witnesses() {
    let rows = oracle_all()["witnesses"].as_array().unwrap();
    assert_eq!(rows.len(), 4, "REST independent live witness count");
    for (i, kind) in ["imf", "quote", "shrink", "syncNotFound"]
        .into_iter()
        .enumerate()
    {
        let r = &rows[i];
        assert_eq!(r["kind"], kind, "REST independent live witness order");
        let h = H::new(
            if kind == "syncNotFound" {
                "positions"
            } else if kind == "shrink" {
                "repair"
            } else {
                "markets"
            },
            if kind == "syncNotFound" { 4 } else { 0 },
        );
        h.borrow_mut().witness = Some(kind);
        if kind == "syncNotFound" {
            h.borrow_mut().original = Rc::from("endpoint not found");
            let task = fetch_positions(h.clone());
            eager(&h, r);
            let v = run(task).unwrap();
            num(
                v.len() as f64,
                &r["value"],
                "POSITIONS synchronous not-found native count",
            );
        } else if kind == "shrink" {
            array(&h.borrow().f.orders).borrow_mut().truncate(1);
            let a = array(&h.borrow().f.orders);
            let task = fix_trigger_orders_without_trigger_by(h.clone(), a.clone());
            eager(&h, r);
            run(task).unwrap();
            num(
                a.borrow().len() as f64,
                &r["value"]["length"],
                "REPAIR shrunken array native length",
            );
            let record = a.borrow()[0].clone().unwrap();
            native(
                &fld(&record, "id"),
                &r["value"]["id"],
                "REPAIR shrunken array native id",
            );
        } else {
            array(&h.borrow().f.markets).borrow_mut().truncate(1);
            let task = fetch_markets(h.clone());
            eager(&h, r);
            let v = run(task).unwrap();
            assert_eq!(v.len(), 1, "MARKETS live witness native count");
            num(
                v[0].limits[3][1],
                &r["value"]["leverage"],
                "MARKETS live IMF native leverage",
            );
            match &v[0].quote {
                Some(s) => str_assert(s, &r["value"]["quote"], "MARKETS live quote"),
                None => assert_eq!(
                    r["value"]["quote"],
                    json!({"u":true}),
                    "MARKETS native absent quote"
                ),
            };
            str_assert(&v[0].base, &r["value"]["base"], "MARKETS live base");
            assert!(
                same(
                    &v[0].info,
                    array(&h.borrow().f.markets).borrow()[0].as_ref().unwrap()
                ),
                "MARKETS live info identity"
            );
            assert_eq!(r["value"]["sameInfo"], true);
        }
        assert_eq!(
            h.borrow().reads,
            r["reads"].as_u64().unwrap() as usize,
            "REST native live getter read count"
        );
        assert_eq!(
            json!(h.borrow().calls),
            r["calls"],
            "REST native live getter invocation sequence"
        );
    }
}
