use safe_cex_venue_backpack::{public_control::JsString, socket_events::*};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::HashMap, process::Command, rc::Rc, sync::OnceLock};
fn u(s: &str) -> JsString {
    s.encode_utf16().collect()
}
fn units(v: &Value) -> JsString {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_u64().unwrap() as u16)
        .collect()
}
fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/socket_events_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "source oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
#[derive(Debug)]
struct Failure {
    tag: &'static str,
    text: JsString,
}
type Error = Rc<Failure>;
struct Sink {
    mode: &'static str,
    calls: Vec<String>,
    messages: Vec<JsString>,
    emitter: Rc<()>,
    primary: Error,
    lookup: Error,
    format: Error,
    emit: Error,
    starts: Error,
}
impl Sink {
    fn new(mode: &'static str) -> Self {
        let error = |tag, text| Rc::new(Failure { tag, text });
        Self {
            mode,
            calls: vec![],
            messages: vec![],
            emitter: Rc::new(()),
            primary: error("primary", [u("primaryλ"), vec![0xd800]].concat()),
            lookup: error("lookup", u("emitter-failure")),
            format: error("format", u("format-failure")),
            emit: error("emit", u("emit-failure")),
            starts: error(
                "starts",
                u("TypeError: stream.startsWith is not a function"),
            ),
        }
    }
    fn call(&mut self, text: impl Into<String>) {
        self.calls.push(text.into());
    }
    fn verify(&self, row: &Value, result: Result<(), Error>, expected: &str) {
        match result {
            Ok(()) => assert_eq!(expected, "ok", "EVENT native outcome"),
            Err(error) => {
                let wanted = match expected {
                    "primary" => &self.primary,
                    "lookup" => &self.lookup,
                    "format" => &self.format,
                    "emit" => &self.emit,
                    _ => panic!("unexpected native failure"),
                };
                assert!(
                    Rc::ptr_eq(&error, wanted),
                    "EVENT native original error identity"
                );
            }
        }
        assert_eq!(row["result"], expected, "EVENT independent outcome");
        assert_eq!(
            json!(self.calls),
            row["calls"],
            "EVENT native operation order"
        );
        assert_eq!(
            self.messages,
            row["messages"]
                .as_array()
                .unwrap()
                .iter()
                .map(units)
                .collect::<Vec<_>>(),
            "EVENT native emitted UTF-16 messages"
        );
    }
}
trait Context {
    fn sink(&mut self) -> &mut Sink;
}
struct Ops<C>(Rc<RefCell<C>>);
impl<C: Context> ErrorSink for Ops<C> {
    type Error = Error;
    type Emitter = Rc<()>;
    fn error_emitter(&mut self) -> Result<Rc<()>, Error> {
        let mut context = self.0.borrow_mut();
        let s = context.sink();
        s.call("emitter");
        if s.mode == "emitLookup" {
            Err(s.lookup.clone())
        } else {
            Ok(s.emitter.clone())
        }
    }
    fn error_text(&mut self, error: &Error) -> Result<JsString, Error> {
        let mut context = self.0.borrow_mut();
        let s = context.sink();
        if error.tag != "starts" {
            s.call("format");
        }
        if ["format", "dispatchFormat"].contains(&s.mode) {
            Err(s.format.clone())
        } else {
            Ok(error.text.clone())
        }
    }
    fn emit_error(&mut self, emitter: Rc<()>, message: JsString) -> Result<(), Error> {
        let mut context = self.0.borrow_mut();
        let s = context.sink();
        assert!(
            Rc::ptr_eq(&emitter, &s.emitter),
            "EVENT native captured emitter identity"
        );
        s.call("emit");
        s.messages.push(message);
        if ["emit", "dispatchEmit"].contains(&s.mode) {
            Err(s.emit.clone())
        } else {
            Ok(())
        }
    }
}
#[derive(Clone)]
struct Val(Rc<Node>);
struct Node {
    value: Option<Value>,
    properties: RefCell<HashMap<String, Val>>,
}
impl Val {
    fn new(value: Option<Value>) -> Self {
        Self(Rc::new(Node {
            value,
            properties: RefCell::new(HashMap::new()),
        }))
    }
    fn get(&self, key: &str) -> Self {
        if let Some(value) = self.0.properties.borrow().get(key) {
            return value.clone();
        }
        let value = Self::new(self.0.value.as_ref().and_then(|v| v.get(key)).cloned());
        self.0
            .properties
            .borrow_mut()
            .insert(key.into(), value.clone());
        value
    }
}
impl MessageValue for Val {
    fn truthy(&self) -> bool {
        match &self.0.value {
            None | Some(Value::Null) => false,
            Some(Value::Bool(b)) => *b,
            Some(Value::Number(n)) => n.as_f64().unwrap() != 0.0,
            Some(Value::String(s)) => !s.is_empty(),
            _ => true,
        }
    }
    fn is_null(&self) -> bool {
        matches!(self.0.value, Some(Value::Null))
    }
    fn equals_number(&self, n: f64) -> bool {
        self.0.value.as_ref().and_then(Value::as_f64) == Some(n)
    }
    fn equals_string(&self, s: &str) -> bool {
        self.0.value.as_ref().and_then(Value::as_str) == Some(s)
    }
}
const WIRES: [&str; 24] = [
    "null",
    "false",
    "0",
    "\"\"",
    "[]",
    "{}",
    r#"{"result":null,"id":1,"e":"trade"}"#,
    r#"{"result":null,"id":0,"e":"trade"}"#,
    "{bad",
    r#"{"e":"depth","tag":9}"#,
    r#"{"data":{"e":"ticker","tag":10}}"#,
    r#"{"e":"trade","tag":11}"#,
    r#"{"e":"bookTicker","tag":12}"#,
    r#"{"stream":"routeλ🚀","data":{"e":"kline","tag":13}}"#,
    r#"{"data":0,"e":"trade","tag":14}"#,
    r#"{"data":{"e":""}}"#,
    r#"{"e":"unknown"}"#,
    r#"{"error":{"code":4002}}"#,
    r#"{"error":{"code":"4002"}}"#,
    r#"{"error":{"code":7,"msg":"bad"}}"#,
    r#"{"stream":"account.orderUpdate","data":{"tag":20}}"#,
    r#"{"stream":"account.orderUpdate.BTC","data":{"tag":21}}"#,
    r#"{"stream":"account.positionUpdate","data":{"tag":22}}"#,
    r#"{"stream":17,"data":{"tag":23}}"#,
];
fn route_specs() -> Vec<(usize, &'static str, bool)> {
    let mut specs: Vec<_> = (0..24).map(|i| (i, "none", false)).collect();
    specs.extend([
        (9, "track", false),
        (9, "parse", false),
        (9, "dispatch", false),
        (19, "stringify", false),
        (19, "emitLookup", false),
        (19, "emit", false),
        (9, "dispatchFormat", false),
        (9, "dispatchEmit", false),
        (20, "captureDispatch", false),
        (22, "captureDispatch", false),
        (9, "none", true),
        (19, "emit", true),
    ]);
    specs
}
struct Dispatcher {
    kind: EventKind,
    identity: Rc<()>,
}
struct Routing {
    sink: Sink,
    disposed: bool,
    parsed: Option<Val>,
    dispatch: Vec<(EventKind, Val, Option<Val>)>,
    pongs: Vec<f64>,
    dispatcher: Rc<()>,
    live_dispatcher: Rc<()>,
    payload: Option<Val>,
    data_reads: usize,
}
impl Context for Routing {
    fn sink(&mut self) -> &mut Sink {
        &mut self.sink
    }
}
fn kind_name(kind: EventKind) -> &'static str {
    match kind {
        EventKind::Depth => "depth",
        EventKind::Ticker => "ticker",
        EventKind::Trade => "trade",
        EventKind::BookTicker => "bookTicker",
        EventKind::Kline => "kline",
        EventKind::Order => "order",
        EventKind::Position => "position",
    }
}
impl MessageHost<JsString> for Ops<Routing> {
    type Value = Val;
    type Dispatcher = Dispatcher;
    fn dispatch_method(&mut self, kind: EventKind) -> Result<Dispatcher, Error> {
        let mut c = self.0.borrow_mut();
        if c.sink.mode == "captureDispatch" {
            c.sink.call(format!("method:{}", kind_name(kind)));
            c.payload = Some(Val::new(Some(json!({"tag":99}))));
        }
        Ok(Dispatcher {
            kind,
            identity: c.live_dispatcher.clone(),
        })
    }
    fn is_disposed(&mut self) -> Result<bool, Error> {
        let mut c = self.0.borrow_mut();
        c.sink.call("disposed");
        Ok(c.disposed)
    }
    fn track_last_received(&mut self) -> Result<(), Error> {
        let mut c = self.0.borrow_mut();
        c.sink.call("track");
        if c.sink.mode == "track" {
            Err(c.sink.primary.clone())
        } else {
            Ok(())
        }
    }
    fn parse(&mut self, data: &JsString) -> Result<Val, Error> {
        let mut c = self.0.borrow_mut();
        c.sink.call("parse");
        if c.sink.mode == "parse" {
            return Err(c.sink.primary.clone());
        }
        let value = Val::new(Some(
            serde_json::from_str(&String::from_utf16(data).unwrap()).unwrap_or(Value::Null),
        ));
        c.parsed = Some(value.clone());
        Ok(value)
    }
    fn property(&mut self, value: &Val, name: &str) -> Result<Val, Error> {
        let mut c = self.0.borrow_mut();
        if c.sink.mode == "captureDispatch" && name == "data" {
            c.sink.call("data");
            c.data_reads += 1;
            if c.data_reads == 2 {
                c.live_dispatcher = Rc::new(());
            }
            return Ok(c.payload.clone().unwrap_or_else(|| value.get(name)));
        }
        Ok(value.get(name))
    }
    fn stringify(&mut self, value: &Val) -> Result<JsString, Error> {
        let mut c = self.0.borrow_mut();
        if c.sink.mode == "stringify" {
            c.sink.call("stringify");
            return Err(c.sink.primary.clone());
        }
        Ok(u(
            &serde_json::to_string(value.0.value.as_ref().unwrap()).unwrap()
        ))
    }
    fn starts_with(&mut self, value: &Val, prefix: &str) -> Result<bool, Error> {
        match value.0.value.as_ref().and_then(Value::as_str) {
            Some(s) => Ok(s.starts_with(prefix)),
            None => Err(self.0.borrow().sink.starts.clone()),
        }
    }
    fn pong(&mut self, interval: f64) -> Result<(), Error> {
        let mut c = self.0.borrow_mut();
        assert_eq!(
            interval.to_bits(),
            10_000.0_f64.to_bits(),
            "ROUTING native pong interval"
        );
        c.sink.call("pong");
        c.pongs.push(interval);
        Ok(())
    }
    fn dispatch(
        &mut self,
        method: Dispatcher,
        data: Val,
        stream: Option<Val>,
    ) -> Result<(), Error> {
        let mut c = self.0.borrow_mut();
        assert!(
            Rc::ptr_eq(&method.identity, &c.dispatcher),
            "ROUTING native captured handler identity"
        );
        let kind = method.kind;
        let root = c.parsed.as_ref().unwrap();
        let candidate = root.get("data");
        let expected = if let Some(payload) = &c.payload {
            payload.clone()
        } else if candidate.truthy() {
            candidate
        } else {
            root.clone()
        };
        assert!(
            Rc::ptr_eq(&data.0, &expected.0),
            "ROUTING native event identity"
        );
        if kind == EventKind::Kline {
            assert!(
                Rc::ptr_eq(&stream.as_ref().unwrap().0, &root.get("stream").0),
                "ROUTING native stream identity"
            );
        }
        c.sink.call(format!("dispatch:{}", kind_name(kind)));
        c.dispatch.push((kind, data, stream));
        if ["dispatch", "dispatchFormat", "dispatchEmit"].contains(&c.sink.mode) {
            Err(c.sink.primary.clone())
        } else {
            Ok(())
        }
    }
}
#[test]
fn socket_routing() {
    let rows = oracle()["routing"].as_array().unwrap();
    assert_eq!(rows.len(), 72, "ROUTING independent corpus size");
    for private in [false, true] {
        for (i, (input, mode, disposed)) in route_specs().into_iter().enumerate() {
            let row = &rows[usize::from(private) * 36 + i];
            assert_eq!(row["private"], private);
            assert_eq!(row["input"], input, "EVENT independent case input");
            assert_eq!(row["mode"], mode, "EVENT independent case mode");
            assert_eq!(row["disposed"], disposed);
            assert_eq!(
                units(&row["wire"]),
                u(WIRES[input]),
                "ROUTING independent content {private}/{i}"
            );
            let dispatcher = Rc::new(());
            let c = Rc::new(RefCell::new(Routing {
                sink: Sink::new(mode),
                disposed,
                parsed: None,
                dispatch: vec![],
                pongs: vec![],
                dispatcher: dispatcher.clone(),
                live_dispatcher: dispatcher,
                payload: None,
                data_reads: 0,
            }));
            let result = if private {
                on_private_message(&u(WIRES[input]), &mut Ops(c.clone()))
            } else {
                on_public_message(&u(WIRES[input]), &mut Ops(c.clone()))
            };
            let c = c.borrow();
            let active = !disposed && !matches!(mode, "track" | "parse");
            let expected_kind = if !active {
                None
            } else if private {
                match input {
                    20 | 21 => Some(EventKind::Order),
                    22 => Some(EventKind::Position),
                    _ => None,
                }
            } else {
                match input {
                    7 | 11 | 14 => Some(EventKind::Trade),
                    9 => Some(EventKind::Depth),
                    10 => Some(EventKind::Ticker),
                    12 => Some(EventKind::BookTicker),
                    13 => Some(EventKind::Kline),
                    _ => None,
                }
            };
            assert_eq!(
                c.dispatch.len(),
                usize::from(expected_kind.is_some()),
                "ROUTING native dispatch count {private}/{i}"
            );
            assert_eq!(
                row["dispatch"].as_array().unwrap().len(),
                usize::from(expected_kind.is_some())
            );
            if let Some(kind) = expected_kind {
                let (actual, data, stream) = &c.dispatch[0];
                assert_eq!(
                    *actual, kind,
                    "ROUTING native dispatched kind {private}/{i}"
                );
                let d = &row["dispatch"][0];
                assert_eq!(d["kind"], kind_name(kind));
                assert_eq!(
                    data.0.value.as_ref().unwrap(),
                    &d["data"],
                    "ROUTING native decoded data fields"
                );
                assert_eq!(d["sameData"], true);
                if kind == EventKind::Kline {
                    let text = stream
                        .as_ref()
                        .unwrap()
                        .0
                        .value
                        .as_ref()
                        .and_then(Value::as_str)
                        .unwrap();
                    assert_eq!(
                        u(text),
                        units(&d["stream"]),
                        "ROUTING native decoded stream"
                    );
                } else {
                    assert!(stream.is_none());
                }
            }
            let pong = active && input == 17;
            assert_eq!(c.pongs.len(), usize::from(pong));
            assert_eq!(row["pongs"].as_array().unwrap().len(), usize::from(pong));
            if pong {
                assert_eq!(
                    u64::from_str_radix(row["pongs"][0].as_str().unwrap(), 16).unwrap(),
                    10_000.0_f64.to_bits()
                );
            }
            let expected = if disposed {
                "ok"
            } else if mode == "track" {
                "primary"
            } else if mode == "emitLookup" {
                "lookup"
            } else if mode == "emit" || (!private && mode == "dispatchEmit") {
                "emit"
            } else if !private && mode == "dispatchFormat" {
                "format"
            } else {
                "ok"
            };
            c.sink.verify(row, result, expected);
        }
    }
}

fn samples() -> [[JsString; 4]; 6] {
    [
        [u("1x"), u("0.25"), u("3.5"), u("7.75")],
        [u("bad"), u("NaN"), u("oops"), u("")],
        [u("-0"), u("Infinity"), u("0"), u("-0")],
        [
            vec![49, 0xd800],
            vec![0xd800, 50],
            u("\u{feff}3.5"),
            u("4x"),
        ],
        [u("1e309"), u("-1e309"), u("2.5e1junk"), u("0x10")],
        [u(".25"), u("-0"), u("1e-400"), u("-1e-400")],
    ]
}
fn numeric_specs(method: &str) -> Vec<(usize, &'static str)> {
    let mut v = vec![];
    for i in 0..6 {
        for mode in ["none", "missing", "update"] {
            v.push((i, mode));
        }
    }
    for mode in ["lookup", "field", "emitLookup", "format", "emit"] {
        v.push((0, mode));
    }
    if method == "depth" {
        for i in 0..6 {
            for mode in ["emptyB", "emptyA", "emptyBoth"] {
                v.push((i, mode));
            }
        }
    }
    v
}
struct Numeric {
    sink: Sink,
    values: [JsString; 4],
    data: Rc<()>,
    ticker: Rc<()>,
    patches: Vec<TickerPatch>,
}
impl Context for Numeric {
    fn sink(&mut self) -> &mut Sink {
        &mut self.sink
    }
}
impl TickerHost<Rc<()>> for Ops<Numeric> {
    type Ticker = Rc<()>;
    fn ticker(&mut self, data: &Rc<()>) -> Result<Option<Rc<()>>, Error> {
        let mut c = self.0.borrow_mut();
        assert!(Rc::ptr_eq(data, &c.data), "NUMERIC native event identity");
        c.sink.call("symbol");
        c.sink.call("lookup");
        if c.sink.mode == "lookup" {
            Err(c.sink.primary.clone())
        } else {
            Ok((c.sink.mode != "missing").then(|| c.ticker.clone()))
        }
    }
    fn field_text(&mut self, _data: &Rc<()>, field: &str) -> Result<JsString, Error> {
        let mut c = self.0.borrow_mut();
        c.sink.call(format!("field:{field}"));
        if c.sink.mode == "field" {
            return Err(c.sink.primary.clone());
        }
        Ok(c.values[match field {
            "c" | "p" | "b" => 0,
            "P" | "a" => 1,
            "v" => 2,
            "q" => 3,
            _ => panic!("unknown field"),
        }]
        .clone())
    }
    fn levels_len(&mut self, _data: &Rc<()>, side: Side) -> Result<usize, Error> {
        let mut c = self.0.borrow_mut();
        c.sink.call(if side == Side::Bids { "b" } else { "a" });
        if c.sink.mode == "field" {
            return Err(c.sink.primary.clone());
        }
        Ok(usize::from(
            c.sink.mode != "emptyBoth"
                && !(side == Side::Bids && c.sink.mode == "emptyB")
                && !(side == Side::Asks && c.sink.mode == "emptyA"),
        ))
    }
    fn first_price(&mut self, _data: &Rc<()>, side: Side) -> Result<JsString, Error> {
        let mut c = self.0.borrow_mut();
        let name = if side == Side::Bids { "b" } else { "a" };
        c.sink.call(name);
        c.sink.call(format!("price:{name}"));
        Ok(c.values[usize::from(side == Side::Asks)].clone())
    }
    fn update_ticker(&mut self, ticker: Rc<()>, update: TickerPatch) -> Result<(), Error> {
        let mut c = self.0.borrow_mut();
        assert!(
            Rc::ptr_eq(&ticker, &c.ticker),
            "NUMERIC native ticker identity"
        );
        c.sink.call("update");
        c.patches.push(update);
        if ["update", "emitLookup", "format", "emit"].contains(&c.sink.mode) {
            Err(c.sink.primary.clone())
        } else {
            Ok(())
        }
    }
}
fn expected_patch(method: &str, input: usize, mode: &str) -> usize {
    if ["missing", "lookup", "field", "emptyBoth"].contains(&mode) {
        return 0;
    }
    let first = input != 1;
    let second = !matches!(input, 1 | 3);
    usize::from(match method {
        "depth" => (mode != "emptyB" && first) || (mode != "emptyA" && second),
        "bookTicker" => first || second,
        _ => first,
    })
}
#[test]
fn ticker_updates() {
    let rows = oracle()["numeric"].as_array().unwrap();
    assert_eq!(rows.len(), 110, "NUMERIC independent corpus size");
    let mut index = 0;
    for method in ["depth", "ticker", "trade", "bookTicker"] {
        for (input, mode) in numeric_specs(method) {
            let row = &rows[index];
            assert_eq!(row["method"], method);
            assert_eq!(row["input"], input, "EVENT independent case input");
            assert_eq!(row["mode"], mode, "EVENT independent case mode");
            let values = samples()[input].clone();
            assert_eq!(
                row["inputValues"],
                json!(values),
                "NUMERIC independent content {index}"
            );
            let symbol = if input == 3 {
                vec![88, 0xd800]
            } else {
                u("BTC")
            };
            assert_eq!(units(&row["symbol"]), symbol);
            let data = Rc::new(());
            let c = Rc::new(RefCell::new(Numeric {
                sink: Sink::new(mode),
                values,
                data: data.clone(),
                ticker: Rc::new(()),
                patches: vec![],
            }));
            let result = match method {
                "depth" => update_ticker_from_depth(&data, &mut Ops(c.clone())),
                "trade" => handle_trade_update(&data, &mut Ops(c.clone())),
                "bookTicker" => handle_book_ticker_update(&data, &mut Ops(c.clone())),
                _ => {
                    let mut factories = 0;
                    let mut captured = None;
                    let callback = create_ticker_handler(
                        Rc::new(RefCell::new(Ops(c.clone()))),
                        |callback, wait, leading, trailing| {
                            factories += 1;
                            assert_eq!(
                                wait.to_bits(),
                                1000.0_f64.to_bits(),
                                "THROTTLE native ticker delay"
                            );
                            assert!(leading && trailing, "THROTTLE native ticker edges");
                            captured = Some(callback.clone());
                            Ok(callback)
                        },
                    )
                    .unwrap();
                    assert_eq!(factories, 1, "THROTTLE independent factory count");
                    assert!(
                        Rc::ptr_eq(&callback, &captured.unwrap()),
                        "THROTTLE native returned handler identity"
                    );
                    assert_eq!(row["factory"].as_array().unwrap().len(), 1);
                    assert_eq!(row["factory"][0]["wait"], "408f400000000000");
                    assert_eq!(row["factory"][0]["leading"], true);
                    assert_eq!(row["factory"][0]["trailing"], true);
                    callback(&data)
                }
            };
            let c = c.borrow();
            let count = expected_patch(method, input, mode);
            assert_eq!(
                c.patches.len(),
                count,
                "NUMERIC native update count {index}"
            );
            assert_eq!(
                row["patches"].as_array().unwrap().len(),
                count,
                "NUMERIC independent source update count {index}"
            );
            for (i, patch) in c.patches.iter().enumerate() {
                let p = &row["patches"][i];
                assert_eq!(p["sameTicker"], true);
                let fields = p["fields"].as_object().unwrap();
                let native = [
                    ("last", patch.last),
                    ("percentage", patch.percentage),
                    ("volume", patch.volume),
                    ("quoteVolume", patch.quote_volume),
                    ("bid", patch.bid),
                    ("ask", patch.ask),
                ];
                for (name, value) in native {
                    let expected = fields
                        .get(name)
                        .map(|v| u64::from_str_radix(v.as_str().unwrap(), 16).unwrap());
                    assert_eq!(
                        value.map(f64::to_bits),
                        expected,
                        "NUMERIC native decoded {name} {index}"
                    );
                }
            }
            let expected = match mode {
                "emitLookup" => "lookup",
                "format" => "format",
                "emit" => "emit",
                _ => "ok",
            };
            c.sink.verify(row, result, expected);
            index += 1;
        }
    }
    assert_eq!(index, 110, "NUMERIC independent iteration floor");
}

const DELIVERY_MODES: [&str; 15] = [
    "none",
    "absent",
    "empty",
    "stats",
    "firstThrow",
    "secondThrow",
    "remove",
    "append",
    "replace",
    "mutateSymbol",
    "emitLookup",
    "format",
    "emit",
    "lookup",
    "throttle",
];
struct Delivery {
    sink: Sink,
    symbol: JsString,
    interval: JsString,
    data: Rc<()>,
    handlers: Rc<RefCell<Vec<Option<Rc<usize>>>>>,
    called: Vec<usize>,
    topics: Vec<JsString>,
    throttles: Vec<JsString>,
    throttle: Rc<()>,
    factory: Rc<()>,
}
impl Context for Delivery {
    fn sink(&mut self) -> &mut Sink {
        &mut self.sink
    }
}
impl HandlerHost<Rc<()>> for Ops<Delivery> {
    type List = Rc<RefCell<Vec<Option<Rc<usize>>>>>;
    type Handler = Rc<usize>;
    type Throttle = Rc<()>;
    type ThrottleFactory = Rc<()>;
    fn throttle_factory(&mut self) -> Result<Rc<()>, Error> {
        let mut c = self.0.borrow_mut();
        if c.sink.mode == "captureThrottle" {
            c.sink.call("factory");
            c.symbol = [u("factory"), vec![0xd800]].concat();
        }
        Ok(c.factory.clone())
    }
    fn symbol(&mut self, _data: &Rc<()>) -> Result<JsString, Error> {
        let mut c = self.0.borrow_mut();
        c.sink.call("symbol");
        Ok(c.symbol.clone())
    }
    fn interval(&mut self, _data: &Rc<()>) -> Result<JsString, Error> {
        let mut c = self.0.borrow_mut();
        c.sink.call("interval");
        Ok(if c.sink.mode == "missingInterval" {
            u("undefined")
        } else {
            c.interval.clone()
        })
    }
    fn handlers(&mut self, topic: &[u16]) -> Result<Option<Self::List>, Error> {
        let mut c = self.0.borrow_mut();
        c.sink.call("lookup");
        c.topics.push(topic.to_vec());
        if c.sink.mode == "lookup" {
            Err(c.sink.primary.clone())
        } else {
            Ok((c.sink.mode != "absent").then(|| c.handlers.clone()))
        }
    }
    fn length(&mut self, list: &Self::List) -> Result<usize, Error> {
        Ok(list.borrow().len())
    }
    fn handler(&mut self, list: &Self::List, index: usize) -> Result<Option<Rc<usize>>, Error> {
        Ok(list.borrow().get(index).cloned().flatten())
    }
    fn record_stats(&mut self, topic: &[u16]) -> Result<(), Error> {
        let mut c = self.0.borrow_mut();
        assert_eq!(topic, c.topics[0], "DELIVERY native stats topic");
        c.sink.call("stats");
        if c.sink.mode == "stats" {
            Err(c.sink.primary.clone())
        } else {
            Ok(())
        }
    }
    fn invoke_handler(&mut self, handler: Rc<usize>, data: &Rc<()>) -> Result<(), Error> {
        let mut c = self.0.borrow_mut();
        assert!(Rc::ptr_eq(data, &c.data), "DELIVERY native event identity");
        let index = *handler;
        assert!(
            Rc::ptr_eq(&handler, c.handlers.borrow()[index].as_ref().unwrap()),
            "DELIVERY native handler identity"
        );
        c.sink.call(format!("handler:{index}"));
        c.called.push(index);
        if index == 0 {
            match c.sink.mode {
                "remove" => c.handlers.borrow_mut()[1] = None,
                "append" => c.handlers.borrow_mut().push(Some(Rc::new(2))),
                "mutateSymbol" => c.symbol = [u("after"), vec![0xdc00]].concat(),
                _ => {}
            }
        }
        if (index == 0 && ["firstThrow", "emitLookup", "format", "emit"].contains(&c.sink.mode))
            || (index == 1 && c.sink.mode == "secondThrow")
        {
            Err(c.sink.primary.clone())
        } else {
            Ok(())
        }
    }
    fn is_null(&mut self, _data: &Rc<()>, field: &str) -> Result<bool, Error> {
        let mut c = self.0.borrow_mut();
        c.sink.call(field);
        Ok((field == "o" && c.sink.mode == "nullOpen")
            || (field == "c" && c.sink.mode == "nullClose"))
    }
    fn depth_throttle(&mut self, factory: Rc<()>, symbol: &[u16]) -> Result<Rc<()>, Error> {
        let mut c = self.0.borrow_mut();
        assert!(
            Rc::ptr_eq(&factory, &c.factory),
            "DELIVERY native captured factory identity"
        );
        c.sink.call("throttle");
        c.throttles.push(symbol.to_vec());
        Ok(c.throttle.clone())
    }
    fn invoke_throttle(&mut self, throttle: Rc<()>, data: &Rc<()>) -> Result<(), Error> {
        let mut c = self.0.borrow_mut();
        assert!(
            Rc::ptr_eq(&throttle, &c.throttle),
            "DELIVERY native throttle identity"
        );
        assert!(
            Rc::ptr_eq(data, &c.data),
            "DELIVERY native throttle data identity"
        );
        c.sink.call("invoke");
        if c.sink.mode == "throttle" {
            Err(c.sink.primary.clone())
        } else {
            Ok(())
        }
    }
}
fn delivery(kind: &'static str) {
    let rows = oracle()["delivery"].as_array().unwrap();
    assert_eq!(rows.len(), 36, "DELIVERY independent corpus size");
    let modes: Vec<_> = DELIVERY_MODES
        .into_iter()
        .chain(if kind == "kline" {
            vec![
                "nullOpen",
                "nullClose",
                "emptyStream",
                "loneStream",
                "missingInterval",
            ]
        } else {
            vec!["captureThrottle"]
        })
        .collect();
    for (i, mode) in modes.into_iter().enumerate() {
        let row = &rows[if kind == "depth" { i } else { 16 + i }];
        assert_eq!(row["kind"], kind);
        assert_eq!(row["mode"], mode, "EVENT independent case mode");
        let symbol = vec![0x3bb, 0xd800];
        let interval = u("1m");
        assert_eq!(units(&row["symbol"]), symbol);
        assert_eq!(units(&row["interval"]), interval);
        let stream = if kind == "depth" || mode == "missingInterval" {
            None
        } else if mode == "emptyStream" {
            Some(vec![])
        } else if mode == "loneStream" {
            Some([u("custom."), vec![0xdc00]].concat())
        } else {
            Some(u("custom.stream"))
        };
        assert_eq!(
            row["stream"],
            json!(stream),
            "DELIVERY independent stream content {i}"
        );
        let data = Rc::new(());
        let c = Rc::new(RefCell::new(Delivery {
            sink: Sink::new(mode),
            symbol: symbol.clone(),
            interval: interval.clone(),
            data: data.clone(),
            handlers: Rc::new(RefCell::new(if mode == "empty" {
                vec![]
            } else {
                vec![Some(Rc::new(0)), Some(Rc::new(1))]
            })),
            called: vec![],
            topics: vec![],
            throttles: vec![],
            throttle: Rc::new(()),
            factory: Rc::new(()),
        }));
        let result = if kind == "depth" {
            handle_depth_update(&data, &mut Ops(c.clone()))
        } else {
            handle_kline_update(&data, stream.as_deref(), &mut Ops(c.clone()))
        };
        let c = c.borrow();
        let expected_calls = if [
            "absent",
            "empty",
            "stats",
            "lookup",
            "nullOpen",
            "nullClose",
        ]
        .contains(&mode)
        {
            vec![]
        } else if mode == "remove"
            || ["emitLookup", "format", "emit"].contains(&mode)
            || (kind == "kline" && mode == "firstThrow")
        {
            vec![0]
        } else {
            vec![0, 1]
        };
        assert_eq!(
            c.called, expected_calls,
            "DELIVERY native callback sequence {kind}/{i}"
        );
        assert_eq!(row["called"], json!(expected_calls));
        let topic = if kind == "depth" {
            [u("depth."), symbol.clone()].concat()
        } else if let Some(stream) = stream.filter(|s| !s.is_empty()) {
            stream
        } else {
            [
                u("kline."),
                if mode == "missingInterval" {
                    u("undefined")
                } else {
                    interval
                },
                u("."),
                symbol.clone(),
            ]
            .concat()
        };
        assert_eq!(
            c.topics,
            vec![topic.clone()],
            "DELIVERY native lookup topic {kind}/{i}"
        );
        assert_eq!(row["topics"], json!([topic]));
        let failed = match mode {
            "emitLookup" => "lookup",
            "format" => "format",
            "emit" => "emit",
            "stats" | "lookup" | "throttle" if kind == "depth" => "primary",
            _ => "ok",
        };
        let throttle = kind == "depth" && failed == "ok";
        let expected_throttles = if throttle {
            vec![if mode == "captureThrottle" {
                [u("factory"), vec![0xd800]].concat()
            } else if mode == "mutateSymbol" {
                [u("after"), vec![0xdc00]].concat()
            } else {
                symbol
            }]
        } else if kind == "depth" && mode == "throttle" {
            vec![symbol]
        } else {
            vec![]
        };
        assert_eq!(
            c.throttles, expected_throttles,
            "DELIVERY native throttle symbol {kind}/{i}"
        );
        assert_eq!(row["throttles"], json!(expected_throttles));
        c.sink.verify(row, result, failed);
    }
}
#[test]
fn depth_delivery() {
    delivery("depth");
}
#[test]
fn kline_delivery() {
    delivery("kline");
}
