use safe_cex_venue_backpack::{
    orderbook_state::{HandlerSet, StateRef, create_order_book_state},
    public_control::JsString,
    signing::{BackpackSigningParams, generate_ws_auth_signature},
    startup_recovery::*,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
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
fn strings(v: &Value) -> Vec<String> {
    serde_json::from_value(v.clone()).unwrap()
}
fn bits(n: f64) -> u64 {
    if n.is_nan() {
        f64::NAN.to_bits()
    } else {
        n.to_bits()
    }
}
fn number(n: f64, v: &Value, label: &str) {
    assert_eq!(
        bits(n),
        u64::from_str_radix(v.as_str().unwrap(), 16).unwrap(),
        "{label} native bits"
    );
}
fn oracle() -> &'static Value {
    static V: OnceLock<Value> = OnceLock::new();
    V.get_or_init(|| {
        let o = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/startup_recovery_oracle.cjs"
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
fn outcome<T>(result: Result<T, Error>, error: &Error, expected: &Value, label: &str) {
    assert_eq!(
        result.is_ok(),
        expected == "ok",
        "{label} native completion"
    );
    if let Err(e) = result {
        assert!(Rc::ptr_eq(&e, error), "{label} native error identity");
    }
}
fn severity(s: Option<Severity>) -> Option<String> {
    s.map(|s| match s {
        Severity::Warning => "warning".into(),
        Severity::Error => "error".into(),
    })
}
fn assert_messages(actual: &[(JsString, Option<Severity>)], v: &Value, label: &str) {
    let expected = v.as_array().unwrap();
    assert_eq!(actual.len(), expected.len(), "{label} native log count");
    for ((message, s), row) in actual.iter().zip(expected) {
        assert_eq!(*message, units(&row[0]), "{label} native log message");
        assert_eq!(
            severity(*s),
            row[1].as_str().map(str::to_owned),
            "{label} native severity"
        );
    }
}
struct Ctor {
    mode: String,
    error: Error,
    calls: Vec<String>,
    base: Rc<()>,
    fields: Option<ExchangeFields>,
    xhr: Option<Rc<()>>,
    public: Option<Rc<()>>,
    private: Option<Rc<()>>,
    rate: Option<f64>,
}
impl Ctor {
    fn new(mode: &str) -> Self {
        Self {
            mode: mode.into(),
            error: Rc::new("ctor"),
            calls: vec![],
            base: Rc::new(()),
            fields: None,
            xhr: None,
            public: None,
            private: None,
            rate: None,
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
    fn base(&self, base: &Rc<()>) {
        assert!(
            Rc::ptr_eq(base, &self.base),
            "CONSTRUCTOR native receiver identity"
        );
    }
}
impl ConstructorHost for Ctor {
    type Error = Error;
    type Base = Rc<()>;
    type Xhr = Rc<()>;
    type Socket = Rc<()>;
    fn super_constructor(&mut self) -> Result<Rc<()>, Error> {
        self.call("super")?;
        Ok(self.base.clone())
    }
    fn initialize_fields(&mut self, base: &Rc<()>, fields: ExchangeFields) -> Result<(), Error> {
        self.base(base);
        self.fields = Some(fields);
        Ok(())
    }
    fn create_xhr(&mut self, base: &Rc<()>, max_rps: f64) -> Result<Rc<()>, Error> {
        self.base(base);
        assert_eq!(
            self.fields.as_ref().unwrap().account_leverage_limit.get(),
            1.0
        );
        self.call("xhr")?;
        self.rate = Some(max_rps);
        self.call("rate")?;
        Ok(Rc::new(()))
    }
    fn assign_xhr(&mut self, base: &Rc<()>, xhr: Rc<()>) -> Result<(), Error> {
        self.base(base);
        self.xhr = Some(xhr);
        Ok(())
    }
    fn create_public_socket(&mut self, base: &Rc<()>) -> Result<Rc<()>, Error> {
        self.base(base);
        assert!(self.xhr.is_some(), "CONSTRUCTOR native xhr before public");
        self.call("public")?;
        Ok(Rc::new(()))
    }
    fn assign_public_socket(&mut self, base: &Rc<()>, socket: Rc<()>) -> Result<(), Error> {
        self.base(base);
        self.public = Some(socket);
        Ok(())
    }
    fn create_private_socket(&mut self, base: &Rc<()>) -> Result<Rc<()>, Error> {
        self.base(base);
        assert!(
            self.public.is_some(),
            "CONSTRUCTOR native public before private"
        );
        self.call("private")?;
        Ok(Rc::new(()))
    }
    fn assign_private_socket(&mut self, base: &Rc<()>, socket: Rc<()>) -> Result<(), Error> {
        self.base(base);
        self.private = Some(socket);
        Ok(())
    }
}
#[test]
fn constructors() {
    let cases = oracle()["constructors"].as_array().unwrap();
    assert_eq!(cases.len(), 9, "CONSTRUCTOR independent corpus size");
    for (i, c) in cases.iter().enumerate() {
        if i < 6 {
            let mode = ["none", "super", "xhr", "rate", "public", "private"][i];
            assert_eq!(c["kind"], "exchange");
            assert_eq!(c["mode"], mode, "CONSTRUCTOR independent corpus mode");
            let mut h = Ctor::new(mode);
            let result = construct_exchange(&mut h);
            if let Ok(base) = &result {
                h.base(base);
                assert!(h.public.is_some() && h.private.is_some());
                assert_eq!(c["sameSockets"], true);
            }
            outcome(result, &h.error, &c["outcome"], "CONSTRUCTOR exchange");
            assert_eq!(h.calls, strings(&c["calls"]));
            if let Some(rate) = h.rate {
                number(rate, &c["maxRPS"], "CONSTRUCTOR rate");
            }
            if !c["fields"].is_null() {
                let fields = h.fields.as_ref().unwrap();
                number(
                    fields.account_leverage_limit.get(),
                    &c["fields"]["leverage"],
                    "CONSTRUCTOR leverage",
                );
                assert_eq!(fields.grafana_every, ["/api/v1/order", "/api/v1/orders"]);
                assert_eq!(
                    fields.grafana_periodic,
                    ["/api/v1/capital", "/api/v1/position"]
                );
                assert_eq!(
                    c["fields"]["every"],
                    json!(["/api/v1/order", "/api/v1/orders"])
                );
                assert_eq!(
                    c["fields"]["periodic"],
                    json!(["/api/v1/capital", "/api/v1/position"])
                );
                assert_eq!(
                    fields.client_id_label,
                    c["fields"]["label"].as_str().unwrap()
                );
                assert_eq!(
                    fields.client_id_requirement,
                    c["fields"]["requirement"].as_str().unwrap()
                );
                let ids = [
                    u("1"),
                    u("4294967295"),
                    u("4294967296"),
                    u("01"),
                    u("0"),
                    u("-1"),
                    u("1λ"),
                    vec![49, 50, 0xd800],
                ];
                let expected = [true, true, false, false, false, false, false, false];
                for (j, id) in ids.iter().enumerate() {
                    assert_eq!(
                        fields.valid_client_id(id),
                        expected[j],
                        "CONSTRUCTOR native client rule {j}"
                    );
                    assert_eq!(c["fields"]["valid"][j], expected[j]);
                }
            }
        } else {
            let mode = ["none", "super", "throttle"][i - 6];
            assert_eq!(c["kind"], "public");
            assert_eq!(c["mode"], mode);
            let calls = RefCell::new(Vec::new());
            let parent = Rc::new(());
            let error = Rc::new("public");
            let handler = Rc::new(());
            let result = construct_public::<_, usize, usize, usize, usize, _, _>(
                || {
                    calls.borrow_mut().push("super".to_string());
                    if mode == "super" {
                        Err(error.clone())
                    } else {
                        Ok(parent.clone())
                    }
                },
                |socket, delay, leading, trailing| {
                    number(delay, &c["throttle"][0], "CONSTRUCTOR native ticker delay");
                    assert_eq!(
                        leading,
                        c["throttle"][1].as_bool().unwrap(),
                        "CONSTRUCTOR native leading"
                    );
                    assert_eq!(
                        trailing,
                        c["throttle"][2].as_bool().unwrap(),
                        "CONSTRUCTOR native trailing"
                    );
                    calls.borrow_mut().push("throttle".into());
                    assert!(Rc::ptr_eq(&socket.base, &parent));
                    assert!(socket.subscriptions.borrow().values().is_empty());
                    assert!(socket.topics.borrow().is_empty());
                    assert!(socket.order_book_state.borrow().is_empty());
                    assert!(socket.snapshot_fetchers.borrow().is_empty());
                    assert!(socket.depth_throttles.borrow().is_empty());
                    assert!(socket.ticker_handler.is_none());
                    if mode == "throttle" {
                        Err(error.clone())
                    } else {
                        Ok(handler.clone())
                    }
                },
            );
            if let Ok(socket) = &result {
                assert!(
                    Rc::ptr_eq(socket.ticker_handler.as_ref().unwrap(), &handler),
                    "CONSTRUCTOR native ticker handler"
                );
                assert!(Rc::ptr_eq(&socket.base, &parent));
                assert_eq!(c["empty"], json!([0, 0, 0, 0, 0]));
                assert_eq!(c["sameParent"], true);
            }
            outcome(result, &error, &c["outcome"], "CONSTRUCTOR public");
            assert_eq!(*calls.borrow(), strings(&c["calls"]));
            if mode != "super" {
                assert_eq!(c["throttle"], json!(["408f400000000000", true, true]));
            }
        }
    }
}
fn kind_name(k: ReadKind) -> &'static str {
    match k {
        ReadKind::Markets => "markets",
        ReadKind::Tickers => "tickers",
        ReadKind::Tick => "tick",
        ReadKind::Account => "account",
        ReadKind::Positions => "positions",
        ReadKind::Balance => "balance",
        ReadKind::Orders => "orders",
    }
}
fn kind_index(k: ReadKind) -> usize {
    match k {
        ReadKind::Markets => 0,
        ReadKind::Tickers => 1,
        ReadKind::Tick => 2,
        ReadKind::Account => 3,
        ReadKind::Positions => 4,
        ReadKind::Balance => 5,
        ReadKind::Orders => 6,
    }
}
#[derive(Clone)]
struct NativeValue {
    kind: ReadKind,
    token: Rc<()>,
    markets: Rc<RefCell<Vec<Rc<Market>>>>,
    positions: Rc<Vec<f64>>,
    upnl: Rc<Cell<f64>>,
}
struct Market {
    id: JsString,
    index: usize,
}
#[derive(Clone)]
struct Subscriber {
    kind: StreamKind,
    active: bool,
    identity: Rc<()>,
}
struct Runtime {
    mode: String,
    error: Error,
    calls: Vec<String>,
    messages: Vec<(JsString, Option<Severity>)>,
    disposed: bool,
    guard_errors: bool,
    public_only: bool,
    values: Vec<NativeValue>,
    pending: VecDeque<(ReadKind, Rc<Cell<bool>>)>,
    loaded: Vec<(JsString, bool)>,
    patches: Vec<(Option<ReadKind>, Option<NativeValue>, Vec<(JsString, bool)>)>,
    update_target: Rc<()>,
    original_update: Rc<()>,
    logger: Rc<()>,
    timer: Option<Callback<Error>>,
    loop_callback: Option<Callback<Error>>,
    delay: Option<f64>,
    loop_delay: Option<f64>,
    subscribers: Vec<Subscriber>,
    replaced: bool,
    sent: Vec<(StreamKind, JsString)>,
}
impl Runtime {
    fn new(mode: &str, public_only: bool, sample: usize) -> Self {
        let markets = Rc::new(RefCell::new(vec![
            Rc::new(Market {
                id: u("BTC"),
                index: 0,
            }),
            Rc::new(Market {
                id: vec![955, 0xd800],
                index: 1,
            }),
        ]));
        let positions = Rc::new(match sample {
            0 => vec![1.25, -3.5],
            1 => vec![],
            _ => vec![-0.0],
        });
        let values = [
            ReadKind::Markets,
            ReadKind::Tickers,
            ReadKind::Tick,
            ReadKind::Account,
            ReadKind::Positions,
            ReadKind::Balance,
            ReadKind::Orders,
        ]
        .into_iter()
        .map(|kind| NativeValue {
            kind,
            token: Rc::new(()),
            markets: markets.clone(),
            positions: positions.clone(),
            upnl: Rc::new(Cell::new(99.0)),
        })
        .collect();
        let target = Rc::new(());
        Self {
            mode: mode.into(),
            error: Rc::new("failedλ"),
            calls: vec![],
            messages: vec![],
            disposed: mode == "disposedBefore",
            guard_errors: true,
            public_only,
            values,
            pending: VecDeque::new(),
            loaded: [
                ("markets", false),
                ("tickers", false),
                ("balance", false),
                ("positions", false),
                ("orders", false),
                ("keep", true),
            ]
            .into_iter()
            .map(|(k, v)| (u(k), v))
            .collect(),
            patches: vec![],
            update_target: target.clone(),
            original_update: target,
            logger: Rc::new(()),
            timer: None,
            loop_callback: None,
            delay: None,
            loop_delay: None,
            subscribers: [
                StreamKind::Ticker,
                StreamKind::Trades,
                StreamKind::BookTicker,
            ]
            .into_iter()
            .map(|kind| Subscriber {
                kind,
                active: true,
                identity: Rc::new(()),
            })
            .collect(),
            replaced: false,
            sent: vec![],
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
    fn value(&self, kind: ReadKind) -> NativeValue {
        self.values[kind_index(kind)].clone()
    }
}
impl Logger for Runtime {
    type Error = Error;
    type LogTarget = Rc<()>;
    fn log_target(&mut self) -> Result<Rc<()>, Error> {
        self.call("logger")?;
        Ok(self.logger.clone())
    }
    fn log(&mut self, target: Rc<()>, message: JsString, s: Option<Severity>) -> Result<(), Error> {
        assert!(
            Rc::ptr_eq(&target, &self.logger),
            "RUNTIME native logger identity"
        );
        self.calls.push("log".into());
        self.messages.push((message, s));
        if self.mode == "log" {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
    fn error_text(&mut self, error: &Error) -> Result<JsString, Error> {
        assert!(Rc::ptr_eq(error, &self.error));
        self.calls.push("format".into());
        Ok(u("failedλ"))
    }
}
impl RuntimeHost for Runtime {
    type Value = NativeValue;
    type FetchFuture = Hold<NativeValue>;
    type UpdateTarget = Rc<()>;
    type Market = Rc<Market>;
    type Subscriber = Subscriber;
    fn fetch(&mut self, kind: ReadKind) -> Result<Self::FetchFuture, Error> {
        self.calls.push(format!("fetch:{}", kind_name(kind)));
        if self.mode == format!("factory:{}", kind_name(kind)) {
            return Err(self.error.clone());
        }
        let open = Rc::new(Cell::new(false));
        self.pending.push_back((kind, open.clone()));
        Ok(Hold {
            open,
            result: if self.mode == format!("reject:{}", kind_name(kind))
                || (kind == ReadKind::Account && ["logger", "log"].contains(&self.mode.as_str()))
            {
                Err(self.error.clone())
            } else {
                Ok(self.value(kind))
            },
        })
    }
    fn disposed(&mut self) -> Result<bool, Error> {
        if self.guard_errors {
            self.call("disposed")?;
        } else {
            self.calls.push("disposed".into());
        }
        Ok(self.disposed)
    }
    fn public_only(&mut self) -> Result<bool, Error> {
        self.call("publicOnly")?;
        Ok(self.public_only)
    }
    fn update_target(&mut self) -> Result<Rc<()>, Error> {
        if Rc::ptr_eq(&self.update_target, &self.original_update) {
            self.calls.push("updateTarget".into());
        }
        Ok(self.update_target.clone())
    }
    fn loaded(&mut self) -> Result<Vec<(JsString, bool)>, Error> {
        self.calls.push("loaded".into());
        if self.mode == "captureUpdate" {
            self.update_target = Rc::new(());
        }
        Ok(self.loaded.clone())
    }
    fn update(
        &mut self,
        target: Rc<()>,
        kind: Option<ReadKind>,
        value: Option<Self::Value>,
        loaded: Vec<(JsString, bool)>,
    ) -> Result<(), Error> {
        if !Rc::ptr_eq(&target, &self.original_update) {
            return Err(self.error.clone());
        }
        if let (Some(kind), Some(value)) = (kind, &value) {
            assert_eq!(value.kind, kind, "RUNTIME native update kind");
            assert!(
                Rc::ptr_eq(&value.token, &self.values[kind_index(kind)].token),
                "RUNTIME native fetched identity"
            );
        }
        self.calls.push("update".into());
        self.patches.push((kind, value, loaded.clone()));
        self.loaded = loaded;
        if self.mode == "update" {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
    fn length(&mut self, value: &Self::Value) -> Result<usize, Error> {
        Ok(match value.kind {
            ReadKind::Markets => value.markets.borrow().len(),
            ReadKind::Tickers | ReadKind::Orders => 1,
            ReadKind::Positions => value.positions.len(),
            _ => panic!("wrong native length kind"),
        })
    }
    fn connect_public(&mut self) -> Result<(), Error> {
        self.call("publicConnect")
    }
    fn connect_private(&mut self) -> Result<(), Error> {
        self.call("privateConnect")
    }
    fn subscribe_all(&mut self, markets: Self::Value) -> Result<(), Error> {
        assert!(
            Rc::ptr_eq(&markets.token, &self.values[0].token),
            "START native subscription markets identity"
        );
        self.call("subscribeAll")
    }
    fn positions_upnl(&mut self, positions: &Self::Value) -> Result<f64, Error> {
        assert_eq!(positions.kind, ReadKind::Positions);
        let mut iter = positions.positions.iter();
        let first = *iter.next().expect("nonempty source sumBy");
        Ok(iter.fold(first, |a, b| a + b))
    }
    fn set_balance_upnl(&mut self, balance: &Self::Value, upnl: f64) -> Result<(), Error> {
        assert!(Rc::ptr_eq(
            &balance.token,
            &self.value(ReadKind::Balance).token
        ));
        balance.upnl.set(upnl);
        Ok(())
    }
    fn error_message(&mut self, error: &Error) -> Result<JsString, Error> {
        assert!(Rc::ptr_eq(error, &self.error));
        self.calls.push("errorMessage".into());
        Ok(u("failedλ"))
    }
    fn schedule_loop(&mut self, callback: Callback<Error>, delay: f64) -> Result<(), Error> {
        self.calls.push("loop".into());
        self.loop_delay = Some(delay);
        self.loop_callback = Some(callback);
        if self.mode == "loop" {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
    fn repeat_tick(&mut self) -> Result<(), Error> {
        self.calls.push("repeatTick".into());
        Ok(())
    }
    fn timeout(&mut self, callback: Callback<Error>, delay: f64) -> Result<(), Error> {
        self.calls.push("timeout".into());
        self.delay = Some(delay);
        self.timer = Some(callback);
        if self.mode == "timeout" {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
    fn market_at(&mut self, markets: &Self::Value, index: usize) -> Result<Self::Market, Error> {
        Ok(markets.markets.borrow()[index].clone())
    }
    fn market_id(&mut self, market: &Self::Market) -> Result<JsString, Error> {
        if self.mode == "capture" && market.index == 0 {
            self.calls.push("id".into());
            self.replaced = true;
        }
        Ok(market.id.clone())
    }
    fn subscriber(&mut self, kind: StreamKind) -> Result<Subscriber, Error> {
        let original = self
            .subscribers
            .iter()
            .find(|s| s.kind == kind)
            .unwrap()
            .clone();
        if self.replaced {
            Ok(Subscriber {
                active: false,
                ..original
            })
        } else {
            self.calls.push(format!("subscriber:{}", stream_name(kind)));
            Ok(original)
        }
    }
    fn subscribe(&mut self, target: Subscriber, id: JsString) -> Result<(), Error> {
        let original = self
            .subscribers
            .iter()
            .find(|s| s.kind == target.kind)
            .unwrap();
        assert!(
            Rc::ptr_eq(&target.identity, &original.identity),
            "SUBSCRIBE native captured callee"
        );
        if !target.active {
            return if self.mode == "replace" && target.kind == StreamKind::Ticker {
                Err(self.error.clone())
            } else {
                Ok(())
            };
        }
        self.calls
            .push(format!("subscribe:{}", stream_name(target.kind)));
        self.sent.push((target.kind, id));
        if self.sent.len() == 1 {
            let markets = self.values[0].markets.clone();
            if self.mode == "append" {
                markets.borrow_mut().push(Rc::new(Market {
                    id: u("new"),
                    index: 2,
                }));
            }
            if self.mode == "remove" {
                markets.borrow_mut().pop();
            }
            if self.mode == "replace" {
                self.replaced = true;
            }
        }
        if self.mode == stream_name(target.kind)
            || (["logger", "log"].contains(&self.mode.as_str())
                && target.kind == StreamKind::Ticker)
        {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
}
fn stream_name(k: StreamKind) -> &'static str {
    match k {
        StreamKind::Ticker => "ticker",
        StreamKind::Trades => "trades",
        StreamKind::BookTicker => "bookTicker",
    }
}
fn assert_patches(h: &Runtime, c: &Value, label: &str) {
    let patches = c["patches"].as_array().unwrap();
    assert_eq!(
        h.patches.len(),
        patches.len(),
        "{label} native update count"
    );
    for ((kind, value, loaded), row) in h.patches.iter().zip(patches) {
        assert_eq!(
            kind.map(kind_name),
            row["kind"].as_str(),
            "{label} native patch field"
        );
        let expected = row["loaded"].as_array().unwrap();
        assert_eq!(loaded.len(), expected.len(), "{label} native loaded length");
        for ((name, flag), entry) in loaded.iter().zip(expected) {
            assert_eq!(
                *name,
                u(entry[0].as_str().unwrap()),
                "{label} native loaded key"
            );
            assert_eq!(
                *flag,
                entry[1].as_bool().unwrap(),
                "{label} native loaded flag"
            );
        }
        if let Some(value) = value {
            assert!(Rc::ptr_eq(
                &value.token,
                &h.values[kind_index(kind.unwrap())].token
            ));
            assert_eq!(row["sameValue"], true);
            if value.kind == ReadKind::Balance {
                number(value.upnl.get(), &row["upnl"], "TICK balance patch upnl");
            }
        }
    }
}
fn drive(
    mut future: Pin<Box<dyn Future<Output = Result<(), Error>>>>,
    h: &Rc<RefCell<Runtime>>,
) -> Result<(), Error> {
    for _ in 0..12 {
        match poll(future.as_mut()) {
            Poll::Ready(result) => return result,
            Poll::Pending => {
                let (kind, open) = h
                    .borrow_mut()
                    .pending
                    .pop_front()
                    .expect("native pending fetch");
                if h.borrow().mode == format!("dispose:{}", kind_name(kind)) {
                    h.borrow_mut().disposed = true;
                }
                open.set(true);
            }
        }
    }
    panic!("native finite driver exhausted")
}
#[test]
fn runtime_start() {
    let cases = oracle()["starts"].as_array().unwrap();
    assert_eq!(cases.len(), 32, "START independent corpus size");
    let modes = [
        "none",
        "disposedBefore",
        "dispose:markets",
        "dispose:tickers",
        "dispose:tick",
        "dispose:orders",
        "factory:markets",
        "reject:tickers",
        "reject:tick",
        "reject:orders",
        "update",
        "logger",
        "publicConnect",
        "privateConnect",
        "subscribeAll",
        "captureUpdate",
    ];
    for (i, c) in cases.iter().enumerate() {
        let public_only = i / 16 == 1;
        let mode = modes[i % 16];
        assert_eq!(c["publicOnly"], public_only);
        assert_eq!(c["mode"], mode, "START independent corpus mode");
        let h = Rc::new(RefCell::new(Runtime::new(mode, public_only, 0)));
        let future = Box::pin(start(h.clone()));
        assert_eq!(
            h.borrow().calls,
            strings(&c["before"]),
            "START native eager calls {i}"
        );
        let result = drive(future, &h);
        let h = h.borrow();
        outcome(result, &h.error, &c["outcome"], "START");
        assert_patches(&h, c, "START");
        assert_eq!(
            h.calls,
            strings(&c["calls"]),
            "START native operation order {i}"
        );
        assert_messages(&h.messages, &c["messages"], "START");
    }
}
#[test]
fn runtime_tick() {
    let cases = oracle()["ticks"].as_array().unwrap();
    assert_eq!(cases.len(), 57, "TICK independent corpus size");
    let modes = [
        "none",
        "publicOnlyMode",
        "disposedBefore",
        "disposed",
        "publicOnly",
        "factory:account",
        "reject:account",
        "dispose:account",
        "reject:positions",
        "dispose:positions",
        "reject:balance",
        "dispose:balance",
        "reject:orders",
        "dispose:orders",
        "update",
        "logger",
        "log",
        "loop",
        "captureUpdate",
    ];
    for (i, c) in cases.iter().enumerate() {
        let sample = i / 19;
        let mode = modes[i % 19];
        assert_eq!(c["sample"], sample);
        assert_eq!(c["mode"], mode, "TICK independent corpus mode");
        let h = Rc::new(RefCell::new(Runtime::new(
            mode,
            mode == "publicOnlyMode",
            sample,
        )));
        let future = Box::pin(tick(h.clone()));
        assert_eq!(
            h.borrow().calls,
            strings(&c["before"]),
            "TICK native eager calls {i}"
        );
        let result = drive(future, &h);
        if sample == 0 && mode == "none" {
            let callback = h.borrow_mut().loop_callback.take().unwrap();
            callback().unwrap();
        }
        let h = h.borrow();
        outcome(result, &h.error, &c["outcome"], "TICK");
        assert_patches(&h, c, "TICK");
        assert_eq!(
            h.calls,
            strings(&c["calls"]),
            "TICK native operation order {i}"
        );
        assert_messages(&h.messages, &c["messages"], "TICK");
        number(
            h.value(ReadKind::Balance).upnl.get(),
            &c["balanceUpnl"],
            "TICK native balance upnl",
        );
        match h.loop_delay {
            Some(delay) => number(delay, &c["loopDelay"], "TICK loop delay"),
            None => assert!(c["loopDelay"].is_null()),
        };
    }
}
#[test]
fn subscriptions() {
    let cases = oracle()["subscriptions"].as_array().unwrap();
    assert_eq!(cases.len(), 12, "SUBSCRIBE independent corpus size");
    let modes = [
        "none",
        "disposed",
        "ticker",
        "trades",
        "bookTicker",
        "append",
        "remove",
        "replace",
        "capture",
        "timeout",
        "logger",
        "log",
    ];
    for (i, c) in cases.iter().enumerate() {
        let mode = modes[i];
        assert_eq!(c["mode"], mode, "SUBSCRIBE independent corpus mode");
        let h = Rc::new(RefCell::new(Runtime::new(mode, false, 0)));
        h.borrow_mut().disposed = mode == "disposed";
        h.borrow_mut().guard_errors = false;
        let markets = h.borrow().value(ReadKind::Markets);
        let result = subscribe_to_all_markets(markets, h.clone());
        outcome(
            result,
            &h.borrow().error,
            &c["outcome"],
            "SUBSCRIBE schedule",
        );
        assert_eq!(
            h.borrow().calls,
            strings(&c["before"]),
            "SUBSCRIBE native eager calls"
        );
        let callback = h.borrow_mut().timer.take();
        if let Some(callback) = callback {
            let result = callback();
            outcome(
                result,
                &h.borrow().error,
                &c["timerOutcome"],
                "SUBSCRIBE timer",
            );
        } else {
            assert_eq!(c["timerOutcome"], "none");
        }
        let h = h.borrow();
        assert_eq!(
            h.sent.len(),
            c["sent"].as_array().unwrap().len(),
            "SUBSCRIBE native invocation count"
        );
        for ((kind, id), row) in h.sent.iter().zip(c["sent"].as_array().unwrap()) {
            assert_eq!(
                stream_name(*kind),
                row[0].as_str().unwrap(),
                "SUBSCRIBE native selected kind"
            );
            assert_eq!(*id, units(&row[1]), "SUBSCRIBE native market id");
        }
        assert_eq!(
            h.calls,
            strings(&c["calls"]),
            "SUBSCRIBE native operation order {i}"
        );
        assert_messages(&h.messages, &c["messages"], "SUBSCRIBE");
        if let Some(delay) = h.delay {
            number(delay, &c["delay"], "SUBSCRIBE native delay");
        }
    }
}

struct Recovery {
    mode: String,
    error: Error,
    calls: Vec<String>,
    messages: Vec<(JsString, Option<Severity>)>,
    state: Option<StateRef<usize, usize>>,
    original: StateRef<usize, usize>,
    fetcher: Option<Rc<()>>,
    original_fetcher: Rc<()>,
    callback: Option<Callback<Error>>,
    called: Vec<usize>,
    disabled: Vec<(JsString, JsString)>,
    handlers: Vec<Rc<usize>>,
    logger: Rc<()>,
    state_reads: usize,
    fetcher_reads: usize,
}
impl Recovery {
    fn new(mode: &str) -> Self {
        let state = Rc::new(RefCell::new(create_order_book_state()));
        let fetcher = Rc::new(());
        Self {
            mode: mode.into(),
            error: Rc::new("recovery"),
            calls: vec![],
            messages: vec![],
            state: Some(state.clone()),
            original: state,
            fetcher: Some(fetcher.clone()),
            original_fetcher: fetcher,
            callback: None,
            called: vec![],
            disabled: vec![],
            handlers: (0..3).map(Rc::new).collect(),
            logger: Rc::new(()),
            state_reads: 0,
            fetcher_reads: 0,
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
impl Logger for Recovery {
    type Error = Error;
    type LogTarget = Rc<()>;
    fn log_target(&mut self) -> Result<Rc<()>, Error> {
        self.call("logger")?;
        Ok(self.logger.clone())
    }
    fn log(&mut self, target: Rc<()>, message: JsString, s: Option<Severity>) -> Result<(), Error> {
        assert!(Rc::ptr_eq(&target, &self.logger));
        self.calls.push("log".into());
        self.messages.push((message, s));
        if self.mode == "log" {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
    fn error_text(&mut self, error: &Error) -> Result<JsString, Error> {
        assert!(Rc::ptr_eq(error, &self.error));
        self.calls.push("format".into());
        Ok(u("failedλ"))
    }
}
impl RecoveryHost<usize, usize> for Recovery {
    type Fetcher = Rc<()>;
    type TerminalEmitter = ();
    fn state(&mut self, id: &[u16]) -> Result<Option<StateRef<usize, usize>>, Error> {
        assert_eq!(id, u("BTC"));
        self.state_reads += 1;
        Ok(self.state.clone())
    }
    fn fetcher(&mut self, id: &[u16]) -> Result<Option<Rc<()>>, Error> {
        assert_eq!(id, u("BTC"));
        self.fetcher_reads += 1;
        Ok(self.fetcher.clone())
    }
    fn disposed(&mut self) -> Result<bool, Error> {
        self.calls.push("disposed".into());
        Ok(self.mode == "disposed")
    }
    fn terminate(
        &mut self,
        id: JsString,
        state: StateRef<usize, usize>,
        reason: JsString,
    ) -> Result<(), Error> {
        assert!(
            Rc::ptr_eq(&state, &self.original),
            "RECOVERY native terminate state identity"
        );
        self.calls.push("terminate".into());
        self.disabled.push((id, reason));
        Ok(())
    }
    fn microtask(&mut self, callback: Callback<Error>) -> Result<(), Error> {
        self.callback = Some(callback);
        Ok(())
    }
    fn fetch_snapshot(&mut self, fetcher: Rc<()>) -> Result<(), Error> {
        assert!(
            Rc::ptr_eq(&fetcher, &self.original_fetcher),
            "RECOVERY native captured fetcher identity"
        );
        self.call("fetch")
    }
    fn terminal_emitter(&mut self) -> Result<(), Error> {
        Ok(())
    }
    fn emit_terminal(&mut self, _: (), message: JsString) -> Result<(), Error> {
        self.calls.push("emit".into());
        self.messages.push((message, None));
        if self.mode == "replaceSet" {
            let mut set = HandlerSet::default();
            set.insert(self.handlers[2].clone());
            self.original.borrow_mut().terminal_handlers = Rc::new(RefCell::new(set));
        }
        if self.mode == "emit" {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
    fn terminal_handler(&mut self, handler: Rc<usize>, reason: &[u16]) -> Result<(), Error> {
        assert!(
            Rc::ptr_eq(&handler, &self.handlers[*handler]),
            "TERMINATE native callback identity"
        );
        let mut expected = u("gapλ");
        expected.push(0xd800);
        assert_eq!(reason, expected);
        self.calls.push(format!("handler:{}", *handler));
        self.called.push(*handler);
        if *handler == 0 {
            let set = self.original.borrow().terminal_handlers.clone();
            if self.mode == "append" {
                set.borrow_mut().insert(self.handlers[2].clone());
            }
            if self.mode == "remove" {
                set.borrow_mut().remove(&self.handlers[1]);
            }
            if ["firstThrow", "logger", "log"].contains(&self.mode.as_str()) {
                return Err(self.error.clone());
            }
        }
        Ok(())
    }
}
fn assert_recovery_state(h: &Recovery, v: &Value, label: &str) {
    let state = h.original.borrow();
    number(state.recovery_attempts, &v[0], &format!("{label} attempts"));
    assert_eq!(
        state.is_snapshot_loaded,
        v[1].as_bool().unwrap(),
        "{label} native snapshot loaded"
    );
    number(
        state.last_update_id,
        &v[2],
        &format!("{label} lastUpdateId"),
    );
    assert_eq!(
        state.is_recovery_scheduled,
        v[3].as_bool().unwrap(),
        "{label} native scheduled"
    );
}
#[test]
fn recovery() {
    let cases = oracle()["recoveries"].as_array().unwrap();
    assert_eq!(cases.len(), 15, "RECOVERY independent corpus size");
    let modes = [
        "none",
        "missingState",
        "missingFetcher",
        "terminal",
        "inFlight",
        "scheduled",
        "exhausted",
        "disposed",
        "replaceState",
        "replaceFetcher",
        "fetch",
        "logger",
        "log",
        "negative",
        "nan",
    ];
    for (i, c) in cases.iter().enumerate() {
        let mode = modes[i];
        assert_eq!(c["mode"], mode, "RECOVERY independent corpus mode");
        let mut h = Recovery::new(mode);
        {
            let mut s = h.original.borrow_mut();
            s.is_snapshot_loaded = true;
            s.last_update_id = 99.0;
            s.recovery_attempts = match mode {
                "exhausted" => 3.0,
                "negative" => -1.0,
                "nan" => f64::NAN,
                _ => 2.0,
            };
            s.is_terminal = mode == "terminal";
            s.is_snapshot_fetch_in_flight = mode == "inFlight";
            s.is_recovery_scheduled = mode == "scheduled";
        }
        if mode == "missingState" {
            h.state = None;
        }
        if mode == "missingFetcher" {
            h.fetcher = None;
        }
        let h = Rc::new(RefCell::new(h));
        let mut reason = u("gapλ");
        reason.push(0xd800);
        let result = recover_order_book(u("BTC"), reason, h.clone());
        outcome(result, &h.borrow().error, &c["outcome"], "RECOVERY eager");
        assert_recovery_state(&h.borrow(), &c["beforeState"], "RECOVERY eager");
        assert_eq!(h.borrow().calls, strings(&c["before"]));
        if mode == "replaceState" {
            h.borrow_mut().state = Some(Rc::new(RefCell::new(create_order_book_state())));
        }
        if mode == "replaceFetcher" {
            h.borrow_mut().fetcher = Some(Rc::new(()));
        }
        let callback = h.borrow_mut().callback.take();
        if let Some(callback) = callback {
            outcome(
                callback(),
                &h.borrow().error,
                &c["timerOutcome"],
                "RECOVERY microtask",
            );
        } else {
            assert_eq!(c["timerOutcome"], "none");
        }
        let h = h.borrow();
        assert_recovery_state(&h, &c["state"], "RECOVERY settled");
        assert_eq!(
            h.calls,
            strings(&c["calls"]),
            "RECOVERY native operation order {i}"
        );
        assert_messages(&h.messages, &c["messages"], "RECOVERY");
        let active = [
            "none",
            "disposed",
            "replaceState",
            "replaceFetcher",
            "fetch",
            "negative",
            "nan",
        ]
        .contains(&mode);
        assert_eq!(
            h.state_reads,
            if active && mode != "disposed" { 2 } else { 1 },
            "RECOVERY independent state read count"
        );
        assert_eq!(
            h.fetcher_reads, 1,
            "RECOVERY independent fetcher read count"
        );
        assert_eq!(h.disabled.len(), if mode == "exhausted" { 1 } else { 0 });
        for ((id, reason), row) in h.disabled.iter().zip(c["disabled"].as_array().unwrap()) {
            assert_eq!(*id, units(&row[0]));
            assert_eq!(*reason, units(&row[1]), "RECOVERY native exhausted reason");
        }
    }
}
#[test]
fn termination() {
    let cases = oracle()["terminations"].as_array().unwrap();
    assert_eq!(cases.len(), 10, "TERMINATE independent corpus size");
    let modes = [
        "none",
        "terminal",
        "emitted",
        "firstThrow",
        "append",
        "remove",
        "replaceSet",
        "emit",
        "logger",
        "log",
    ];
    for (i, c) in cases.iter().enumerate() {
        let mode = modes[i];
        assert_eq!(c["mode"], mode, "TERMINATE independent corpus mode");
        let mut h = Recovery::new(mode);
        let state = h.original.clone();
        {
            let mut s = state.borrow_mut();
            s.is_snapshot_loaded = true;
            s.is_terminal = mode == "terminal";
            s.terminal_error_emitted = mode == "emitted";
            s.updates = vec![1, 2];
            s.terminal_handlers
                .borrow_mut()
                .insert(h.handlers[0].clone());
            s.terminal_handlers
                .borrow_mut()
                .insert(h.handlers[1].clone());
        }
        let mut reason = u("gapλ");
        reason.push(0xd800);
        let result = terminate_order_book(&u("BTC"), &state, &reason, &mut h);
        outcome(result, &h.error, &c["outcome"], "TERMINATE");
        let s = state.borrow();
        assert_eq!(
            s.is_terminal,
            c["state"][0].as_bool().unwrap(),
            "TERMINATE native terminal flag"
        );
        assert_eq!(s.is_snapshot_loaded, c["state"][1].as_bool().unwrap());
        assert_eq!(
            s.updates.len(),
            c["state"][2].as_u64().unwrap() as usize,
            "TERMINATE native updates cleared"
        );
        assert_eq!(s.terminal_error_emitted, c["state"][3].as_bool().unwrap());
        assert_eq!(
            h.called,
            serde_json::from_value::<Vec<usize>>(c["called"].clone()).unwrap(),
            "TERMINATE native callback sequence"
        );
        assert_eq!(
            h.calls,
            strings(&c["calls"]),
            "TERMINATE native operation order {i}"
        );
        assert_messages(&h.messages, &c["messages"], "TERMINATE");
    }
}
#[test]
fn ws_auth_composition() {
    let cases = oracle()["wsAuth"].as_array().unwrap();
    assert_eq!(cases.len(), 24, "WS AUTH independent corpus size");
    let numbers = [
        -0.0,
        0.0,
        1234.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    let modes = ["none", "clock", "sign", "reject"];
    for (i, c) in cases.iter().enumerate() {
        let input = i / 4;
        let mode = modes[i % 4];
        assert_eq!(c["input"], input);
        assert_eq!(c["mode"], mode, "WS AUTH independent corpus mode");
        let now = numbers[input];
        let window = if input == 0 {
            None
        } else {
            Some(numbers[5 - input])
        };
        number(now, &c["now"], "WS AUTH independent clock input");
        if let Some(window) = window {
            number(window, &c["window"], "WS AUTH independent window input");
        } else {
            assert!(c["window"].is_null());
        }
        let calls = Rc::new(RefCell::new(Vec::<String>::new()));
        let error = Rc::new("auth");
        let open = Rc::new(Cell::new(false));
        let mut key = u("keyλ");
        key.push(0xdc00);
        let original_key = key.clone();
        assert_eq!(
            c["apiKey"],
            json!(original_key),
            "WS AUTH independent key input"
        );
        let secret = u(&format!("{}Bwc=", "BwcH".repeat(10)));
        let observed = calls.clone();
        let signed = calls.clone();
        let e = error.clone();
        let se = error.clone();
        let so = open.clone();
        let original_secret = secret.clone();
        assert_eq!(
            c["secret"],
            json!(original_secret),
            "WS AUTH independent secret input"
        );
        let future = generate_ws_auth_signature::<Rc<()>, _, _>(
            key,
            secret,
            window,
            move || {
                observed.borrow_mut().push("clock".into());
                if mode == "clock" { Err(e) } else { Ok(now) }
            },
            move |instruction, params, timestamp, window, secret| {
                signed.borrow_mut().push("sign".into());
                assert_eq!(instruction, "subscribe", "WS AUTH native instruction");
                assert_eq!(secret, original_secret, "WS AUTH native original secret");
                assert_eq!(
                    bits(timestamp),
                    bits(now),
                    "WS AUTH native timestamp argument"
                );
                assert_eq!(
                    bits(window),
                    bits(if input == 0 {
                        5000.0
                    } else {
                        numbers[5 - input]
                    }),
                    "WS AUTH native window argument"
                );
                match params {
                    BackpackSigningParams::Object(record) => {
                        assert!(record.borrow().is_empty(), "WS AUTH native empty object")
                    }
                    BackpackSigningParams::Batch(_) => panic!("WS AUTH native params kind"),
                };
                if mode == "sign" {
                    Err(se)
                } else {
                    Ok(Hold {
                        open: so,
                        result: if mode == "reject" {
                            Err(se)
                        } else {
                            Ok(u("AQcDCQ=="))
                        },
                    })
                }
            },
        );
        assert_eq!(
            *calls.borrow(),
            strings(&c["before"]),
            "WS AUTH native eager calls {i}"
        );
        let mut future = Box::pin(future);
        if mode == "none" || mode == "reject" {
            assert!(poll(future.as_mut()).is_pending());
        }
        open.set(true);
        let result = match poll(future.as_mut()) {
            Poll::Ready(result) => result,
            Poll::Pending => panic!("auth did not settle"),
        };
        if let Ok(value) = &result {
            let expected = c["value"].as_array().unwrap();
            assert_eq!(value[0], original_key, "WS AUTH native returned key");
            for (index, field) in value.iter().enumerate() {
                assert_eq!(
                    *field,
                    units(&expected[index]),
                    "WS AUTH native field {index}"
                );
            }
        }
        outcome(result, &error, &c["outcome"], "WS AUTH");
        assert_eq!(*calls.borrow(), strings(&c["calls"]));
    }
    let number_text = ["0", "0", "1234", "NaN", "Infinity", "-Infinity"];
    for (i, c) in cases.iter().enumerate() {
        let input = i / 4;
        if i % 4 == 1 {
            assert!(
                c["message"].is_null() && c["seed"].is_null(),
                "WS AUTH clock failure has no signer arguments"
            );
            continue;
        }
        let expected = format!(
            "instruction=subscribe&timestamp={}&window={}",
            number_text[input],
            if input == 0 {
                "5000"
            } else {
                number_text[5 - input]
            }
        );
        assert_eq!(
            serde_json::from_value::<Vec<u8>>(c["message"].clone()).unwrap(),
            expected.as_bytes(),
            "WS AUTH independent actual-source signing payload {i}"
        );
        assert_eq!(
            serde_json::from_value::<Vec<u8>>(c["seed"].clone()).unwrap(),
            vec![7; 32],
            "WS AUTH independent actual-source decoded seed {i}"
        );
    }
}
#[test]
fn signing_params() {
    assert_eq!(
        oracle()["paramsShape"],
        json!([
            "Record<string, unknown>",
            "readonly Record<string, unknown>[]"
        ]),
        "PARAMS independent source shape"
    );
    let native = Rc::new(());
    let record = Rc::new(RefCell::new(vec![(vec![0xd800], native.clone())]));
    let batch = Rc::new(RefCell::new(vec![record.clone()]));
    let object = BackpackSigningParams::object(record.clone());
    let array = BackpackSigningParams::batch(batch.clone());
    match object {
        BackpackSigningParams::Object(actual) => {
            assert!(
                Rc::ptr_eq(&actual, &record),
                "PARAMS native object identity"
            );
            assert_eq!(actual.borrow()[0].0, vec![0xd800]);
            assert!(Rc::ptr_eq(&actual.borrow()[0].1, &native));
        }
        _ => panic!("PARAMS native object variant"),
    };
    match array {
        BackpackSigningParams::Batch(actual) => {
            assert!(Rc::ptr_eq(&actual, &batch), "PARAMS native batch identity");
            assert!(Rc::ptr_eq(&actual.borrow()[0], &record));
        }
        _ => panic!("PARAMS native batch variant"),
    };
}
