use safe_cex_types::{
    MarginType, OrderSide, OrderStatus, OrderTimeInForce, OrderType, PositionMode, PositionSide,
};
use safe_cex_venue_backpack::{
    private_events::*, private_order_utils, public_control::JsString, socket_events::ErrorSink,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    process::Command,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
type Error = Rc<&'static str>;
fn u(s: &str) -> JsString {
    s.encode_utf16().collect()
}
fn units(v: &Value) -> JsString {
    serde_json::from_value(v.clone()).unwrap()
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
fn n(actual: f64, expected: &Value, name: &str) {
    assert_eq!(
        bits(actual),
        expected.as_str().unwrap(),
        "{name} native number"
    );
}
fn oracle() -> &'static Value {
    static V: OnceLock<Value> = OnceLock::new();
    V.get_or_init(|| {
        let o = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/private_events_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        serde_json::from_slice(&o.stdout).unwrap()
    })
}
fn poll<F: Future + ?Sized>(f: Pin<&mut F>) -> Poll<F::Output> {
    f.poll(&mut Context::from_waker(Waker::noop()))
}
struct Hold<T> {
    open: Rc<Cell<bool>>,
    result: Result<T, Error>,
}
impl<T: Clone> Future for Hold<T> {
    type Output = Result<T, Error>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if self.open.get() {
            Poll::Ready(self.result.clone())
        } else {
            Poll::Pending
        }
    }
}
#[derive(Clone)]
struct Data {
    fields: BTreeMap<String, JsString>,
    reduce_only: bool,
    timestamp: f64,
}
impl Data {
    fn set(&mut self, key: &str, value: &str) {
        self.fields.insert(key.into(), u(value));
    }
    fn encoded(&self) -> Value {
        let mut map = serde_json::Map::new();
        for (k, v) in &self.fields {
            map.insert(k.clone(), json!(v));
        }
        if self.fields.contains_key("o") {
            map.insert("r".into(), json!(self.reduce_only));
        }
        map.insert("E".into(), json!(bits(self.timestamp)));
        Value::Object(map)
    }
}
fn order_data(i: usize) -> Data {
    let mut d = Data {
        fields: [
            ("s", "BTC"),
            ("S", "Bid"),
            ("P", ""),
            ("o", "Limit"),
            ("p", "12.25"),
            ("L", "14.5"),
            ("Q", "9.75"),
            ("q", "3.5"),
            ("z", "1.25"),
            ("i", "idλ"),
            ("X", "New"),
            ("f", "GTC"),
            ("e", "orderUpdate"),
            ("l", "0.5"),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), u(v)))
        .collect(),
        reduce_only: false,
        timestamp: 1234567.0,
    };
    let changes: &[(&str, &str)] = match i {
        0 => &[],
        1 => &[
            ("S", "Ask"),
            ("o", "MARKET"),
            ("P", "10.5"),
            ("e", "orderFill"),
            ("X", "Filled"),
            ("f", "IOC"),
        ],
        2 => {
            d.reduce_only = true;
            &[
                ("o", "market"),
                ("p", ""),
                ("L", ""),
                ("Q", ""),
                ("e", "orderFill"),
                ("f", "PO"),
            ]
        }
        3 => &[
            ("P", "bad"),
            ("q", "bad"),
            ("z", "bad"),
            ("X", "Expired"),
            ("f", "FOK"),
        ],
        4 => &[("q", "-0"), ("z", "-0"), ("p", ""), ("L", ""), ("l", "")],
        5 => {
            d.timestamp = -1.0;
            &[
                ("P", "0"),
                ("Q", "1e309"),
                ("z", "Infinity"),
                ("S", "other"),
            ]
        }
        6 => &[
            ("o", "other"),
            ("p", "1e309"),
            ("l", "bad"),
            ("e", "orderFill"),
        ],
        7 => &[("q", "\u{feff}2.5"), ("f", "")],
        8 => &[
            ("p", ""),
            ("L", "-0"),
            ("q", "-2.5"),
            ("z", "3.75"),
            ("X", "Cancelled"),
        ],
        9 => {
            d.reduce_only = true;
            &[("o", "Market"), ("p", "0"), ("Q", "5.5")]
        }
        10 => &[
            ("P", "-0"),
            ("Q", "-0"),
            ("z", "-1e-400"),
            ("e", "orderFill"),
            ("L", ""),
        ],
        11 => &[
            ("P", "20"),
            ("o", "market"),
            ("S", "Ask"),
            ("e", "orderFill"),
            ("l", "1e309"),
        ],
        _ => unreachable!(),
    };
    for (k, v) in changes {
        d.set(k, v);
    }
    if i == 7 {
        d.fields.insert("p".into(), vec![49, 0xd800]);
        d.fields.insert("s".into(), vec![955, 0xd800]);
        d.fields.insert("i".into(), vec![0xdc00]);
    }
    d
}
fn position_data(i: usize) -> Data {
    let quantities = [
        "2.5",
        "-2.5",
        "0",
        "-0",
        "bad",
        "Infinity",
        "-Infinity",
        "1",
        "1e-400",
        "-1e-400",
        "3.75",
        "-8.5",
    ];
    let fractions = [
        "0.1",
        "0.4",
        "-1",
        "0",
        "bad",
        "Infinity",
        "1e-309",
        "2.220446049250312e-16",
        "0.2",
        "0.08",
        "0.5",
        "1",
    ];
    let mut d = Data {
        fields: [
            ("s", "BTC"),
            ("q", quantities[i]),
            ("b", if i.is_multiple_of(3) { "12.25" } else { "" }),
            ("B", "14.5"),
            ("f", fractions[i]),
            ("n", if i.is_multiple_of(2) { "19.75" } else { "0" }),
            ("P", if i == 4 { "bad" } else { "-2.5" }),
            ("l", if i.is_multiple_of(2) { "5.25" } else { "-0" }),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), u(v)))
        .collect(),
        reduce_only: false,
        timestamp: if i == 3 { -1.0 } else { 1234567.0 },
    };
    if i == 7 {
        d.fields.insert("s".into(), vec![955, 0xd800]);
        d.fields.insert("q".into(), vec![49, 0xd800]);
    }
    d
}
#[derive(Clone)]
struct Stored {
    id: JsString,
    tag: usize,
}
struct Updates {
    mode: String,
    error: Error,
    calls: Vec<String>,
    messages: Vec<JsString>,
    market: Rc<RefCell<JsString>>,
    orders: Vec<OrderUpdate>,
    fills: Vec<FillUpdate>,
    positions: Rc<RefCell<Vec<Option<Rc<Stored>>>>>,
    trigger: Rc<()>,
    original_trigger: Rc<()>,
    replace: Rc<()>,
    original_replace: Rc<()>,
    replace_reads: usize,
    originals: Vec<Rc<Stored>>,
    replacements: Vec<Vec<usize>>,
    added: Vec<PositionUpdate>,
    input: usize,
    open: Rc<Cell<bool>>,
}
impl Updates {
    fn new(mode: &str, symbol: JsString, input: usize) -> Self {
        let originals = (0..3)
            .map(|tag| {
                Rc::new(Stored {
                    id: if mode == "remove" || mode == "replace" {
                        u(["otherA", "otherB", "otherC"][tag])
                    } else if tag == 2 {
                        u("other")
                    } else {
                        let mut id = symbol.clone();
                        id.extend(u(if tag == 0 { ":long" } else { ":short" }));
                        id
                    },
                    tag,
                })
            })
            .collect::<Vec<_>>();
        let trigger = Rc::new(());
        let replace = Rc::new(());
        Self {
            mode: mode.into(),
            trigger: trigger.clone(),
            original_trigger: trigger,
            replace: replace.clone(),
            original_replace: replace,
            replace_reads: 0,
            error: Rc::new("failedλ"),
            calls: vec![],
            messages: vec![],
            market: Rc::new(RefCell::new(symbol)),
            orders: vec![],
            fills: vec![],
            positions: Rc::new(RefCell::new(originals.iter().cloned().map(Some).collect())),
            originals,
            replacements: vec![],
            added: vec![],
            input,
            open: Rc::new(Cell::new(false)),
        }
    }
    fn call(&mut self, name: &str) -> Result<(), Error> {
        self.calls.push(name.into());
        if self.mode == name {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
}
impl ErrorSink for Updates {
    type Error = Error;
    type Emitter = ();
    fn error_emitter(&mut self) -> Result<(), Error> {
        Ok(())
    }
    fn error_text(&mut self, error: &Error) -> Result<JsString, Error> {
        assert!(
            Rc::ptr_eq(error, &self.error),
            "UPDATE native caught error identity"
        );
        self.calls.push("format".into());
        Ok(u(error))
    }
    fn emit_error(&mut self, _: (), text: JsString) -> Result<(), Error> {
        self.calls.push("error".into());
        self.messages.push(text);
        Ok(())
    }
}
type SharedData = Rc<RefCell<Data>>;
impl UpdateHost<SharedData> for Updates {
    type Market = Rc<RefCell<JsString>>;
    type FillTarget = Rc<()>;
    type TriggerTarget = Rc<()>;
    fn trigger_target(&mut self) -> Result<Self::TriggerTarget, Error> {
        if self.mode == "captureTrigger" {
            self.calls.push("triggerTarget".into());
        }
        Ok(self.trigger.clone())
    }
    fn field(&mut self, d: &SharedData, key: &str) -> Result<Option<JsString>, Error> {
        self.calls.push(format!("field:{key}"));
        if self.mode == "field" && key == "q" {
            return Err(self.error.clone());
        }
        Ok(d.borrow().fields.get(key).cloned())
    }
    fn strict_true(&mut self, d: &SharedData, key: &str) -> Result<bool, Error> {
        self.calls.push(format!("field:{key}"));
        assert_eq!(key, "r");
        Ok(d.borrow().reduce_only)
    }
    fn numeric_field(&mut self, d: &SharedData, key: &str) -> Result<f64, Error> {
        self.calls.push(format!("field:{key}"));
        assert_eq!(key, "E");
        Ok(d.borrow().timestamp)
    }
    fn market(&mut self, symbol: JsString) -> Result<Option<Self::Market>, Error> {
        self.calls.push("market".into());
        assert_eq!(
            symbol,
            *self.market.borrow(),
            "UPDATE native market argument"
        );
        Ok((self.mode != "missing").then(|| self.market.clone()))
    }
    fn market_symbol(&mut self, m: &Self::Market) -> Result<JsString, Error> {
        assert!(Rc::ptr_eq(m, &self.market), "UPDATE native market identity");
        if self.mode == "captureTrigger" {
            self.calls.push("symbol".into());
            self.trigger = Rc::new(());
        }
        Ok(m.borrow().clone())
    }
    fn trigger_type(
        &mut self,
        target: Self::TriggerTarget,
        symbol: JsString,
        side: OrderSide,
        price: Option<f64>,
    ) -> Result<OrderType, Error> {
        assert!(
            Rc::ptr_eq(&target, &self.original_trigger),
            "ORDER native captured classifier identity"
        );
        self.calls.push("trigger".into());
        Ok(private_order_utils::determine_trigger_order_type(
            &[(symbol.clone(), Some(15.0), None)],
            &symbol,
            side,
            price,
        ))
    }
    fn add_order(&mut self, order: OrderUpdate) -> Result<(), Error> {
        self.orders.push(order);
        self.call("add")
    }
    fn fill_target(&mut self) -> Result<Self::FillTarget, Error> {
        self.calls.push("fillTarget".into());
        if self.mode == "fillTarget" {
            *self.market.borrow_mut() = vec![97, 102, 116, 101, 114, 0xdc00];
        }
        Ok(Rc::new(()))
    }
    fn emit_fill(&mut self, _: Self::FillTarget, fill: FillUpdate) -> Result<(), Error> {
        self.fills.push(fill);
        self.call("fill")
    }
    fn fetch_positions_throttled(&mut self) -> Result<(), Error> {
        self.call("positions")
    }
    fn fetch_balance_throttled(&mut self) -> Result<(), Error> {
        self.call("balance")
    }
    fn fetch_orders_throttled(&mut self) -> Result<(), Error> {
        self.call("orders")
    }
}
impl PositionHost<SharedData> for Updates {
    type ModeFuture = Hold<PositionMode>;
    type StoredPosition = Rc<Stored>;
    type PositionList = Rc<RefCell<Vec<Option<Rc<Stored>>>>>;
    type ReplaceTarget = Rc<()>;
    fn replace_target(&mut self) -> Result<Self::ReplaceTarget, Error> {
        if self.mode == "captureReplace" {
            self.calls.push("replaceTarget".into());
        }
        Ok(self.replace.clone())
    }
    fn position_mode(&mut self) -> Result<Self::ModeFuture, Error> {
        self.calls.push("mode".into());
        Ok(Hold {
            open: self.open.clone(),
            result: if self.mode == "modeReject" {
                Err(self.error.clone())
            } else {
                Ok(if self.input.is_multiple_of(2) {
                    PositionMode::Hedged
                } else {
                    PositionMode::OneWay
                })
            },
        })
    }
    fn positions(&mut self) -> Result<Self::PositionList, Error> {
        Ok(self.positions.clone())
    }
    fn positions_len(&mut self, list: &Self::PositionList) -> Result<usize, Error> {
        Ok(list.borrow().len())
    }
    fn position_at(
        &mut self,
        list: &Self::PositionList,
        index: usize,
    ) -> Result<Option<Self::StoredPosition>, Error> {
        Ok(list.borrow().get(index).cloned().flatten())
    }
    fn position_id(&mut self, p: &Self::StoredPosition) -> Result<JsString, Error> {
        if p.tag == 0 && (self.mode == "remove" || self.mode == "replace") {
            self.positions.borrow_mut()[1] = if self.mode == "remove" {
                None
            } else {
                self.positions.borrow()[2].clone()
            };
        }
        if p.tag == 0 && self.mode == "captureReplace" {
            self.calls.push("id:0".into());
            self.replace_reads += 1;
            if self.input == 0 && self.replace_reads == 2 {
                self.replace = Rc::new(());
            }
        }
        Ok(p.id.clone())
    }
    fn replace_positions(
        &mut self,
        target: Self::ReplaceTarget,
        positions: Vec<Self::StoredPosition>,
    ) -> Result<(), Error> {
        assert!(
            Rc::ptr_eq(&target, &self.original_replace),
            "POSITION native captured update identity"
        );
        self.calls.push("replace".into());
        for p in &positions {
            assert!(
                Rc::ptr_eq(p, &self.originals[p.tag]),
                "POSITION native retained record identity"
            );
        }
        self.replacements
            .push(positions.iter().map(|p| p.tag).collect());
        self.positions = Rc::new(RefCell::new(positions.into_iter().map(Some).collect()));
        Ok(())
    }
    fn add_position(&mut self, position: PositionUpdate) -> Result<(), Error> {
        self.added.push(position);
        self.call("position").and_then(|()| {
            if self.mode == "add" {
                Err(self.error.clone())
            } else {
                Ok(())
            }
        })
    }
}
fn side(v: &Value) -> OrderSide {
    if units(v) == u("buy") {
        OrderSide::Buy
    } else {
        assert_eq!(units(v), u("sell"));
        OrderSide::Sell
    }
}
fn order_type(v: &Value) -> OrderType {
    match String::from_utf16(&units(v)).unwrap().as_str() {
        "limit" => OrderType::Limit,
        "market" => OrderType::Market,
        "stop_market" => OrderType::StopLoss,
        "take_profit_market" => OrderType::TakeProfit,
        _ => panic!("unexpected type"),
    }
}
fn order_status(v: &Value) -> OrderStatus {
    match String::from_utf16(&units(v)).unwrap().as_str() {
        "open" => OrderStatus::Open,
        "closed" => OrderStatus::Closed,
        "canceled" => OrderStatus::Canceled,
        _ => panic!("unexpected status"),
    }
}
fn tif(v: &Value) -> OrderTimeInForce {
    match String::from_utf16(&units(v)).unwrap().as_str() {
        "GoodTillCancel" => OrderTimeInForce::GoodTillCancel,
        "ImmediateOrCancel" => OrderTimeInForce::ImmediateOrCancel,
        "FillOrKill" => OrderTimeInForce::FillOrKill,
        "PostOnly" => OrderTimeInForce::PostOnly,
        _ => panic!("unexpected TIF"),
    }
}
fn assert_order(o: &OrderUpdate, v: &Value) {
    assert_eq!(o.id, units(&v["id"]), "ORDER native id");
    assert_eq!(o.symbol, units(&v["symbol"]), "ORDER native symbol");
    assert_eq!(o.order_type, order_type(&v["type"]), "ORDER native type");
    assert_eq!(o.side, side(&v["side"]), "ORDER native side");
    for (name, value) in [
        ("price", o.price),
        ("amount", o.amount),
        ("filled", o.filled),
        ("remaining", o.remaining),
        ("lastUpdate", o.last_update),
    ] {
        n(value, &v[name], &format!("ORDER {name}"));
    }
    assert_eq!(o.status, order_status(&v["status"]), "ORDER native status");
    assert_eq!(
        o.reduce_only,
        v["reduceOnly"].as_bool().unwrap(),
        "ORDER native reduceOnly"
    );
    assert_eq!(o.time_in_force, tif(&v["timeInForce"]), "ORDER native TIF");
}
fn assert_fill(o: &FillUpdate, v: &Value) {
    assert_eq!(o.order_id, units(&v["orderId"]), "FILL native orderId");
    assert_eq!(o.symbol, units(&v["symbol"]), "FILL native symbol");
    assert_eq!(o.side, side(&v["side"]), "FILL native side");
    for (name, value) in [
        ("price", o.price),
        ("amount", o.amount),
        ("timestamp", o.timestamp),
    ] {
        n(value, &v[name], &format!("FILL {name}"));
    }
}
fn assert_position(p: &PositionUpdate, v: &Value) {
    assert_eq!(p.id, units(&v["id"]), "POSITION native id");
    assert_eq!(p.symbol, units(&v["symbol"]), "POSITION native symbol");
    assert_eq!(
        p.side,
        if units(&v["side"]) == u("long") {
            PositionSide::Long
        } else {
            PositionSide::Short
        },
        "POSITION native side"
    );
    for (name, value) in [
        ("entryPrice", p.entry_price),
        ("notional", p.notional),
        ("leverage", p.leverage),
        ("unrealizedPnl", p.unrealized_pnl),
        ("contracts", p.contracts),
        ("liquidationPrice", p.liquidation_price),
        ("lastUpdate", p.last_update),
    ] {
        n(value, &v[name], &format!("POSITION {name}"));
    }
    assert_eq!(
        p.position_mode,
        if units(&v["positionMode"]) == u("hedged") {
            PositionMode::Hedged
        } else {
            PositionMode::OneWay
        }
    );
    assert_eq!(p.margin_type, MarginType::Cross);
    assert_eq!(units(&v["marginType"]), u("cross"));
}
#[test]
fn private_initialization() {
    let cases = oracle()["constructors"].as_array().unwrap();
    assert_eq!(cases.len(), 2, "CONSTRUCTOR independent corpus size");
    for (i, c) in cases.iter().enumerate() {
        assert_eq!(c["failBase"], json!(i == 1));
        let parent = Rc::new(());
        let error = Rc::new("super");
        let mut called = 0;
        let result = construct::<_, usize, _>(|| {
            called += 1;
            if i == 1 {
                Err(error.clone())
            } else {
                Ok(parent.clone())
            }
        });
        assert_eq!(called, 1, "CONSTRUCTOR native super count");
        if i == 1 {
            assert!(Rc::ptr_eq(&result.err().unwrap(), &error));
            continue;
        }
        let socket = result.unwrap();
        assert!(
            Rc::ptr_eq(&socket.base, &parent),
            "CONSTRUCTOR native parent identity"
        );
        assert!(socket.state.subscriptions.borrow().values().is_empty());
        assert!(socket.state.orders_timeout.borrow().is_none());
        assert!(!Rc::ptr_eq(&socket.state.positions, &socket.state.balance));
        for (state, field) in [
            (&socket.state.positions, "position"),
            (&socket.state.balance, "balance"),
        ] {
            assert_eq!(
                state.last.get().to_bits(),
                0,
                "CONSTRUCTOR native positive-zero timestamp"
            );
            assert_eq!(
                c[format!("{field}Bits")],
                json!(["0000000000000000", "407f400000000000"])
            );
            assert_eq!(
                state.throttle_ms.get(),
                500.0,
                "CONSTRUCTOR native throttle"
            );
            assert!(!state.scheduled.get());
            assert!(!state.pending.get());
            assert_eq!(c[field], json!([0, 500, false, false]));
        }
        assert_eq!(c["sameParent"], true);
        assert_eq!(c["subscriptions"], 0);
        assert_eq!(c["timeoutAbsent"], true);
    }
}
#[test]
fn private_order_updates() {
    let cases = oracle()["orders"].as_array().unwrap();
    assert_eq!(cases.len(), 120, "ORDER independent corpus size");
    let modes = [
        "none",
        "missing",
        "add",
        "fill",
        "positions",
        "balance",
        "orders",
        "field",
        "fillTarget",
        "captureTrigger",
    ];
    for (i, c) in cases.iter().enumerate() {
        let input = i / 10;
        let mode = modes[i % 10];
        assert_eq!(c["input"], input);
        assert_eq!(c["mode"], mode, "ORDER independent corpus mode");
        let data = order_data(input);
        assert_eq!(
            c["data"],
            data.encoded(),
            "ORDER independent corpus content"
        );
        let mut h = Updates::new(mode, data.fields["s"].clone(), input);
        handle_order_update(&Rc::new(RefCell::new(data)), &mut h).unwrap();
        assert_eq!(h.orders.len(), c["added"].as_array().unwrap().len());
        for (o, v) in h.orders.iter().zip(c["added"].as_array().unwrap()) {
            assert_order(o, v);
        }
        assert_eq!(h.fills.len(), c["fills"].as_array().unwrap().len());
        for (o, v) in h.fills.iter().zip(c["fills"].as_array().unwrap()) {
            assert_fill(o, v);
        }
        assert_eq!(
            h.calls,
            serde_json::from_value::<Vec<String>>(c["calls"].clone()).unwrap(),
            "ORDER native call sequence {i}"
        );
        assert_eq!(
            h.messages,
            serde_json::from_value::<Vec<JsString>>(c["messages"].clone()).unwrap(),
            "ORDER native errors {i}"
        );
    }
}
#[test]
fn private_position_updates() {
    let cases = oracle()["positions"].as_array().unwrap();
    assert_eq!(cases.len(), 96, "POSITION independent corpus size");
    let modes = [
        "none",
        "missing",
        "modeReject",
        "add",
        "during",
        "remove",
        "replace",
        "captureReplace",
    ];
    for (i, c) in cases.iter().enumerate() {
        let input = i / 8;
        let mode = modes[i % 8];
        assert_eq!(c["input"], input);
        assert_eq!(c["mode"], mode, "POSITION independent corpus mode");
        let data = Rc::new(RefCell::new(position_data(input)));
        let original_symbol = data.borrow().fields["s"].clone();
        assert_eq!(c["originalSymbol"], json!(original_symbol));
        assert_eq!(
            c["positionMode"],
            json!(u(if input % 2 == 0 { "hedged" } else { "oneway" })),
            "POSITION independent corpus mode input"
        );
        let h = Rc::new(RefCell::new(Updates::new(mode, original_symbol, input)));
        let mut future = Box::pin(handle_position_update(data.clone(), h.clone()));
        assert_eq!(
            h.borrow().calls,
            serde_json::from_value::<Vec<String>>(c["before"].clone()).unwrap(),
            "POSITION native eager calls {i}"
        );
        if mode != "missing" {
            assert!(poll(future.as_mut()).is_pending());
        }
        if mode == "during" {
            *h.borrow().market.borrow_mut() = vec![99, 104, 97, 110, 103, 101, 100, 0xdc00];
            data.borrow_mut().set("b", "99.5");
        }
        assert_eq!(
            c["data"],
            data.borrow().encoded(),
            "POSITION independent corpus content"
        );
        h.borrow().open.set(true);
        match poll(future.as_mut()) {
            Poll::Ready(result) => result.unwrap(),
            Poll::Pending => panic!("position did not settle"),
        };
        let h = h.borrow();
        assert_eq!(h.added.len(), c["added"].as_array().unwrap().len());
        for (p, v) in h.added.iter().zip(c["added"].as_array().unwrap()) {
            assert_position(p, v);
        }
        assert_eq!(
            h.replacements,
            serde_json::from_value::<Vec<Vec<usize>>>(c["updates"].clone()).unwrap(),
            "POSITION native filtered records {i}"
        );
        assert_eq!(
            h.positions
                .borrow()
                .iter()
                .map(|p| p.as_ref().map(|p| p.tag))
                .collect::<Vec<_>>(),
            serde_json::from_value::<Vec<Option<usize>>>(c["survivors"].clone()).unwrap()
        );
        assert_eq!(
            h.calls,
            serde_json::from_value::<Vec<String>>(c["calls"].clone()).unwrap(),
            "POSITION native call sequence {i}"
        );
        assert_eq!(
            h.messages,
            serde_json::from_value::<Vec<JsString>>(c["messages"].clone()).unwrap()
        );
    }
}

struct Fetches {
    mode: String,
    expected_kind: Option<FetchKind>,
    error: Error,
    calls: Vec<String>,
    now: f64,
    disposed: bool,
    callback: Option<TimeoutCallback<Error>>,
    delay: Option<f64>,
    open: Rc<Cell<bool>>,
    value: Rc<()>,
    updates: Vec<(FetchKind, Rc<()>)>,
    loaded: Vec<(JsString, bool)>,
    order_loaded: Vec<Vec<(JsString, bool)>>,
    orders_target: Rc<()>,
    original_orders_target: Rc<()>,
}
impl Fetches {
    fn new(mode: &str) -> Self {
        let target = Rc::new(());
        Self {
            orders_target: target.clone(),
            original_orders_target: target,
            mode: mode.into(),
            error: Rc::new("fetch"),
            calls: vec![],
            now: 1234.0,
            expected_kind: None,
            disposed: false,
            callback: None,
            delay: None,
            open: Rc::new(Cell::new(false)),
            value: Rc::new(()),
            updates: vec![],
            loaded: vec![
                (u("markets"), true),
                (u("orders"), false),
                (u("positions"), false),
            ],
            order_loaded: vec![],
        }
    }
    fn call(&mut self, name: &str) -> Result<(), Error> {
        self.calls.push(name.into());
        if self.mode == name {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
}
impl FetchHost for Fetches {
    type Error = Error;
    type Value = Rc<()>;
    type OrdersTarget = Rc<()>;
    fn orders_target(&mut self) -> Result<Self::OrdersTarget, Error> {
        if self.mode == "captureOrders" {
            self.calls.push("ordersTarget".into());
        }
        Ok(self.orders_target.clone())
    }
    type FetchFuture = Hold<Rc<()>>;
    fn now(&mut self) -> Result<f64, Error> {
        self.call("now")?;
        Ok(self.now)
    }
    fn disposed(&mut self) -> Result<bool, Error> {
        self.calls.push("disposed".into());
        Ok(self.disposed)
    }
    fn execute(&mut self, kind: FetchKind) -> Result<(), Error> {
        assert_eq!(
            Some(kind),
            self.expected_kind,
            "THROTTLE native selected executor"
        );
        self.call("execute")
    }
    fn timeout(&mut self, callback: TimeoutCallback<Error>, delay: f64) -> Result<(), Error> {
        self.delay = Some(delay);
        self.call("timeout")?;
        self.callback = Some(callback);
        Ok(())
    }
    fn fetch(&mut self, kind: FetchKind) -> Result<Self::FetchFuture, Error> {
        assert_eq!(
            Some(kind),
            self.expected_kind,
            "FETCH native selected factory"
        );
        self.calls.push("fetch".into());
        if self.mode == "factory" {
            return Err(self.error.clone());
        }
        Ok(Hold {
            open: self.open.clone(),
            result: if self.mode == "reject" {
                Err(self.error.clone())
            } else {
                Ok(self.value.clone())
            },
        })
    }
    fn update(&mut self, kind: FetchKind, value: Self::Value) -> Result<(), Error> {
        assert!(
            Rc::ptr_eq(&value, &self.value),
            "FETCH native result identity"
        );
        self.updates.push((kind, value));
        self.call("update")
    }
    fn loaded(&mut self) -> Result<Vec<(JsString, bool)>, Error> {
        self.calls.push("loaded".into());
        if self.mode == "captureOrders" {
            self.orders_target = Rc::new(());
        }
        Ok(self.loaded.clone())
    }
    fn update_orders(
        &mut self,
        target: Self::OrdersTarget,
        value: Self::Value,
        loaded: Vec<(JsString, bool)>,
    ) -> Result<(), Error> {
        assert!(
            Rc::ptr_eq(&target, &self.original_orders_target),
            "FETCH native captured orders update identity"
        );
        self.order_loaded.push(loaded);
        self.update(FetchKind::Orders, value)
    }
}
fn state() -> Rc<FetchState> {
    Rc::new(FetchState {
        throttle_ms: Cell::new(500.0),
        ..FetchState::default()
    })
}
fn throttle(kind: FetchKind, s: Rc<FetchState>, h: Rc<RefCell<Fetches>>) -> Result<(), Error> {
    if kind == FetchKind::Positions {
        throttled_fetch_positions(s, h)
    } else {
        throttled_fetch_balance(s, h)
    }
}
#[test]
fn private_fetch_throttles() {
    let cases = oracle()["throttles"].as_array().unwrap();
    assert_eq!(cases.len(), 24, "THROTTLE independent corpus size");
    let modes = [
        "immediate",
        "edge",
        "delay",
        "scheduled",
        "disposed",
        "timerDisposed",
        "timerEmpty",
        "now",
        "timeout",
        "execute",
        "futureLast",
        "secondEvent",
    ];
    for (i, c) in cases.iter().enumerate() {
        let kind = if i / 12 == 0 {
            FetchKind::Positions
        } else {
            FetchKind::Balance
        };
        let mode = modes[i % 12];
        assert_eq!(
            c["kind"],
            if kind == FetchKind::Positions {
                "positions"
            } else {
                "balance"
            }
        );
        assert_eq!(c["mode"], mode, "THROTTLE independent corpus mode");
        let now = match mode {
            "immediate" | "execute" => 1000.0,
            "edge" => 500.0,
            "delay" | "secondEvent" => 499.0,
            _ => 10.0,
        };
        assert_eq!(c["now"].as_f64(), Some(now));
        let last = if mode == "futureLast" { 100.0 } else { 0.0 };
        assert_eq!(c["last"].as_f64(), Some(last));
        let s = state();
        s.last.set(last);
        s.scheduled.set(mode == "scheduled");
        let h = Rc::new(RefCell::new(Fetches::new(mode)));
        h.borrow_mut().expected_kind = Some(kind);
        h.borrow_mut().now = now;
        h.borrow_mut().disposed = mode == "disposed";
        let result = throttle(kind, s.clone(), h.clone());
        assert_eq!(result.is_err(), c["caught"].as_bool().unwrap());
        if let Err(e) = result {
            assert!(Rc::ptr_eq(&e, &h.borrow().error));
        }
        assert_eq!(
            h.borrow().calls,
            serde_json::from_value::<Vec<String>>(c["before"].clone()).unwrap(),
            "THROTTLE native eager calls {i}"
        );
        assert_eq!(
            json!([s.scheduled.get(), s.pending.get()]),
            c["beforeState"]
        );
        if mode == "secondEvent" {
            h.borrow_mut().now = 501.0;
            throttle(kind, s.clone(), h.clone()).unwrap();
        }
        if mode == "timerDisposed" {
            h.borrow_mut().disposed = true;
        }
        if mode == "timerEmpty" {
            s.pending.set(false);
        }
        let callback = h.borrow_mut().callback.take();
        if let Some(callback) = callback {
            callback().unwrap();
        }
        assert_eq!(
            h.borrow().calls,
            serde_json::from_value::<Vec<String>>(c["calls"].clone()).unwrap(),
            "THROTTLE native callback calls {i}"
        );
        assert_eq!(
            json!([s.scheduled.get(), s.pending.get()]),
            c["state"],
            "THROTTLE native callback state {i}"
        );
        match h.borrow().delay {
            Some(delay) => n(delay, &c["delay"], "THROTTLE delay"),
            None => assert!(c["delay"].is_null()),
        };
    }
}
#[test]
fn private_fetch_execution() {
    let cases = oracle()["fetches"].as_array().unwrap();
    assert_eq!(cases.len(), 22, "FETCH independent corpus size");
    let modes = [
        "none",
        "disposedBefore",
        "disposedAfter",
        "reject",
        "factory",
        "update",
        "now",
    ];
    for (i, c) in cases.iter().enumerate() {
        let kind = [FetchKind::Positions, FetchKind::Balance, FetchKind::Orders][(i / 7).min(2)];
        let mode = if i == 21 {
            "captureOrders"
        } else {
            modes[i % 7]
        };
        assert_eq!(
            c["kind"],
            ["positions", "balance", "orders"][(i / 7).min(2)]
        );
        assert_eq!(c["mode"], mode, "FETCH independent corpus mode");
        let p = state();
        p.pending.set(true);
        let b = state();
        b.pending.set(true);
        let timeout = RefCell::new(Some(27));
        let h = Rc::new(RefCell::new(Fetches::new(mode)));
        h.borrow_mut().expected_kind = Some(kind);
        h.borrow_mut().disposed = mode == "disposedBefore";
        let mut future: Pin<Box<dyn Future<Output = Result<(), Error>> + '_>> = match kind {
            FetchKind::Positions => Box::pin(execute_position_fetch(&p, h.clone())),
            FetchKind::Balance => Box::pin(execute_balance_fetch(&b, h.clone())),
            FetchKind::Orders => Box::pin(fetch_throttled_orders(&timeout, h.clone())),
        };
        assert_eq!(
            h.borrow().calls,
            serde_json::from_value::<Vec<String>>(c["before"].clone()).unwrap(),
            "FETCH native eager calls {i}"
        );
        assert_eq!(
            p.last.get(),
            c["beforeState"][0].as_f64().unwrap(),
            "FETCH native eager position time {i}"
        );
        assert_eq!(
            p.pending.get(),
            c["beforeState"][1].as_bool().unwrap(),
            "FETCH native eager position pending {i}"
        );
        assert_eq!(
            b.last.get(),
            c["beforeState"][2].as_f64().unwrap(),
            "FETCH native eager balance time {i}"
        );
        assert_eq!(
            b.pending.get(),
            c["beforeState"][3].as_bool().unwrap(),
            "FETCH native eager balance pending {i}"
        );
        assert_eq!(
            timeout.borrow().is_none(),
            c["beforeState"][4].as_bool().unwrap(),
            "FETCH native eager timeout {i}"
        );
        if mode == "disposedAfter" {
            h.borrow_mut().disposed = true;
        }
        h.borrow().open.set(true);
        let result = match poll(future.as_mut()) {
            Poll::Ready(result) => result,
            Poll::Pending => panic!("fetch did not settle"),
        };
        let expected_error = (mode == "reject"
            || mode == "factory"
            || mode == "update"
            || mode == "now" && kind != FetchKind::Orders)
            && !(mode == "disposedBefore" && kind == FetchKind::Orders);
        assert_eq!(
            result.is_err(),
            expected_error,
            "FETCH chosen visible failure {i}"
        );
        if let Err(e) = result {
            assert!(
                Rc::ptr_eq(&e, &h.borrow().error),
                "FETCH native error identity"
            );
        }
        let h = h.borrow();
        assert_eq!(
            h.calls,
            serde_json::from_value::<Vec<String>>(c["calls"].clone()).unwrap(),
            "FETCH native calls {i}"
        );
        assert_eq!(h.updates.len(), c["patches"].as_array().unwrap().len());
        for ((actual, value), patch) in h.updates.iter().zip(c["patches"].as_array().unwrap()) {
            assert_eq!(*actual, kind, "FETCH native update kind");
            assert!(Rc::ptr_eq(value, &h.value));
            assert_eq!(patch["sameValue"], true);
            if kind == FetchKind::Orders {
                assert_eq!(
                    h.order_loaded[0],
                    vec![
                        (u("markets"), true),
                        (u("orders"), true),
                        (u("positions"), false)
                    ],
                    "FETCH native loaded fields"
                );
                assert_eq!(
                    patch["loaded"],
                    json!({"markets":true,"orders":true,"positions":false})
                );
                assert_eq!(
                    h.loaded,
                    vec![
                        (u("markets"), true),
                        (u("orders"), false),
                        (u("positions"), false)
                    ],
                    "FETCH native original loaded unchanged"
                );
            } else {
                assert!(patch["loaded"].is_null());
            }
        }
    }
}
