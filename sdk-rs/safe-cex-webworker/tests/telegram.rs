#[path = "../../safe-cex-exchange/tests/support/runtime.rs"]
mod runtime;
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange},
    runtime::*,
};
use safe_cex_store::{StoreUpdate, store::DefaultStore};
use safe_cex_types::{ExchangeName, PlaceOrderOpts, Position};
use safe_cex_webworker::telegram::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::Future,
    pin::{Pin, pin},
    rc::Rc,
    task::{Context, Poll, Waker},
};

const POST_URL: &str = "https://api.telegram.org/bottoken:é/sendMessage";
const GET_URL: &str = "https://api.telegram.org/bottoken:é/getUpdates";
const POSITION_TEXT: &str = "BTC long UPNL: -12.5\nETH short UPNL: 0\nNAN long UPNL: NaN\nINF short UPNL: Infinity\nSMALL short UPNL: 1e-7\nBIG long UPNL: 1e+21";
const USAGE: &str = "Usage: /cli SYMBOL command [args]\nExample: /cli BTCUSDT buy 0.01";
#[derive(Clone, Debug, PartialEq)]
enum Effect {
    Get(String),
    Post(String, f64, String),
    Timer(u64),
    Exchange(String),
    GetterError,
    Exec(String, String, String),
    Log(String, String, String),
}
fn view(effects: &[Effect]) -> Value {
    json!(
        effects
            .iter()
            .map(|e| match e {
                Effect::Get(u) => json!(["get", u]),
                Effect::Post(u, id, t) => json!(["post", u, format!("{:016x}", id.to_bits()), t]),
                Effect::Timer(t) => json!(["timer", t]),
                Effect::Exchange(e) => json!(["exchange", e]),
                Effect::GetterError => json!(["getter-error"]),
                Effect::Exec(e, s, c) => json!(["exec", e, s, c]),
                Effect::Log(e, event, m) => json!(["log", e, event, m]),
            })
            .collect::<Vec<_>>()
    )
}
struct Clock {
    inner: runtime::Runtime,
    effects: Rc<RefCell<Vec<Effect>>>,
}
impl ExchangeRuntime for Clock {
    fn monotonic_now_ms(&self) -> f64 {
        self.inner.monotonic_now_ms()
    }
    fn now_ms(&self) -> f64 {
        self.inner.now_ms()
    }
    fn sleep(&self, ms: u64) -> ExchangeFuture<'static, ()> {
        self.effects.borrow_mut().push(Effect::Timer(ms));
        self.inner.sleep(ms)
    }
    fn spawn_local(&self, f: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        self.inner.spawn_local(f)
    }
    fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {}
    fn unique_id(&self) -> String {
        "id".into()
    }
    fn random(&self) -> f64 {
        self.inner.random()
    }
    fn is_worker(&self) -> bool {
        true
    }
    fn report_raw_exception(&self, _: &safe_cex_exchange::js_runtime::value::Js, _: &serde_json::Value) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, e: &ExchangeError, c: &Value) {
        self.inner.report_exception(e, c)
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("no worker bridge")
    }
}
struct Venue {
    base: BaseExchange,
    caps: ExchangeCapabilities,
}
impl Exchange for Venue {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.caps
    }
}
enum Frame {
    Ready(Vec<TelegramMessage>),
    Fail,
    Pending,
}
struct Host {
    clock: Rc<Clock>,
    effects: Rc<RefCell<Vec<Effect>>>,
    frames: RefCell<VecDeque<Frame>>,
    pending: Rc<RefCell<Option<Vec<TelegramMessage>>>>,
    mode: &'static str,
}
impl TelegramHost for Host {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.clock.clone()
    }
    fn get_updates(
        &self,
        url: String,
    ) -> Result<ExchangeFuture<'static, Vec<TelegramMessage>>, ExchangeError> {
        self.effects.borrow_mut().push(Effect::Get(url));
        Ok(
            match self
                .frames
                .borrow_mut()
                .pop_front()
                .unwrap_or(Frame::Ready(vec![]))
            {
                Frame::Ready(v) => Box::pin(async move { Ok(v) }),
                Frame::Fail => Box::pin(async { Err(ExchangeError::new("get fail")) }),
                Frame::Pending => {
                    let cell = self.pending.clone();
                    Box::pin(std::future::poll_fn(move |_| {
                        match cell.borrow_mut().take() {
                            Some(v) => Poll::Ready(Ok(v)),
                            None => Poll::Pending,
                        }
                    }))
                }
            },
        )
    }
    fn post_message(
        &self,
        url: String,
        id: f64,
        text: String,
    ) -> Result<ExchangeFuture<'static, ()>, ExchangeError> {
        self.effects.borrow_mut().push(Effect::Post(url, id, text));
        Ok(Box::pin(async { Ok(()) }))
    }
    fn command_executor(&self) -> Option<TelegramExecutor> {
        if self.mode == "none" {
            return None;
        }
        let mode = self.mode;
        let effects = self.effects.clone();
        Some(Rc::new(move |exchange, symbol, command| {
            effects.borrow_mut().push(Effect::Exec(
                label(&exchange.base().name).to_owned(),
                symbol,
                command,
            ));
            if mode == "sync" {
                return Err(ExchangeError::new("sync"));
            }
            Ok(Box::pin(async move {
                if mode == "async" {
                    Err(ExchangeError::new("async"))
                } else {
                    Ok(())
                }
            }))
        }))
    }
}
type Worker = TelegramWorker<String, Result<Rc<dyn Exchange>, ExchangeError>>;
struct Setup {
    host: Rc<Host>,
    worker: Rc<Worker>,
    current: Rc<RefCell<Rc<dyn Exchange>>>,
    second: Rc<dyn Exchange>,
    calls: Rc<Cell<u64>>,
}
fn label(name: &ExchangeName) -> &'static str {
    match name {
        ExchangeName::Bybit => "bybit",
        ExchangeName::Binance => "binance",
        _ => panic!("fixture venue"),
    }
}
fn venue(
    name: ExchangeName,
    clock: Rc<Clock>,
    effects: Rc<RefCell<Vec<Effect>>>,
) -> Rc<dyn Exchange> {
    let store = Rc::new(DefaultStore::new());
    let positions: [(&str, &str, f64); 6] = [
        ("BTC", "long", -12.5),
        ("ETH", "short", -0.0),
        ("NAN", "long", f64::NAN),
        ("INF", "short", f64::INFINITY),
        ("SMALL", "short", 1e-7),
        ("BIG", "long", 1e21),
    ];
    let positions=positions.into_iter().map(|(symbol,side,pnl)|{let mut p:Position=serde_json::from_value(json!({"id":symbol,"symbol":symbol,"side":side,"entryPrice":0,"notional":0,"leverage":1,"unrealizedPnl":0,"contracts":0,"liquidationPrice":0})).unwrap();p.unrealized_pnl=pnl;p}).collect();
    safe_cex_store::Store::update(
        store.as_ref(),
        StoreUpdate {
            positions: Some(positions),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let label = label(&name).to_owned();
    let base = BaseExchange::new(
        name,
        serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
        store,
        clock,
        Default::default(),
    );
    base.on(
        "exec-log",
        Rc::new(move |event, data| {
            effects.borrow_mut().push(Effect::Log(
                label.clone(),
                event.into(),
                safe_cex_exchange::js_runtime::value::to_json(data).unwrap()["message"]
                    .as_str()
                    .unwrap()
                    .into(),
            ));
            Ok(())
        }),
    );
    Rc::new(Venue {
        base,
        caps: Default::default(),
    })
}
fn setup(mode: &'static str) -> Setup {
    let effects = Rc::new(RefCell::new(vec![]));
    let clock = Rc::new(Clock {
        inner: Default::default(),
        effects: effects.clone(),
    });
    let first = venue(ExchangeName::Bybit, clock.clone(), effects.clone());
    let second = venue(ExchangeName::Binance, clock.clone(), effects.clone());
    let current = Rc::new(RefCell::new(first));
    let calls = Rc::new(Cell::new(0));
    let cursor = current.clone();
    let count = calls.clone();
    let log = effects.clone();
    let getter = Rc::new(move |_: &Worker| {
        let c = count.get() + 1;
        count.set(c);
        if mode == "getter" && c == 1 {
            log.borrow_mut().push(Effect::GetterError);
            return Err(ExchangeError::new("getter"));
        }
        let e = cursor.borrow().clone();
        log.borrow_mut()
            .push(Effect::Exchange(label(&e.base().name).to_owned()));
        Ok(e)
    });
    let worker = TelegramWorker::new(getter, "token:é".to_owned());
    let host = Rc::new(Host {
        clock,
        effects,
        frames: RefCell::new(VecDeque::new()),
        pending: Rc::new(RefCell::new(None)),
        mode,
    });
    Setup {
        host,
        worker,
        current,
        second,
        calls,
    }
}
fn drain(s: &Setup, now: u64) {
    for _ in 0..4 {
        s.host.clock.inner.advance(now);
    }
}
fn run<T>(future: impl Future<Output = T>) -> T {
    let mut f = pin!(future);
    match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("expected a ready fixture"),
    }
}
fn message(text: &str, id: f64) -> TelegramMessage {
    let m: TelegramMessage =
        serde_json::from_value(json!({"update_id":42,"message":{"chat":{"id":-7.5},"text":text}}))
            .unwrap();
    assert_eq!(
        m.message.chat.id.to_bits(),
        (-7.5_f64).to_bits(),
        "TELEGRAM native decoded chat id"
    );
    TelegramMessage { update_id: id, ..m }
}
fn source() -> Value {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/telegram_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn counter(s: &Setup, n: f64) {
    assert_eq!(
        s.worker.tick_counter.get().to_bits(),
        n.to_bits(),
        "TELEGRAM native tick counter"
    );
}
fn expected_post(text: &str) -> Effect {
    Effect::Post(POST_URL.into(), -7.5, text.into())
}
fn compare(s: &Setup, row: &Value, label: &str) {
    assert_eq!(row["label"], label);
    assert_eq!(
        view(&s.host.effects.borrow()),
        row["effects"],
        "TELEGRAM source/native effects {label}"
    );
    assert_eq!(
        format!("{:016x}", s.worker.tick_counter.get().to_bits()),
        row["counter"]
    );
}

#[test]
fn opaque_constructor_callback_and_live_getter_match_source() {
    let source = source();
    let rows = source["constructors"].as_array().unwrap();
    assert_eq!(rows.len(), 4, "TELEGRAM independent constructor count");
    let tokens: [&[u16]; 4] = [
        &[98, 111, 116, 58, 97],
        &[0xe9, 0xd83e, 0xde99],
        &[],
        &[0xd800],
    ];
    for (index, token) in tokens.into_iter().enumerate() {
        let first = Rc::new(Cell::new(17_u32));
        let second = Rc::new(Cell::new(42_u32));
        let current = Rc::new(RefCell::new(first.clone()));
        let cursor = current.clone();
        let calls = Rc::new(Cell::new(0));
        let count = calls.clone();
        let receivers = Rc::new(RefCell::new(Vec::new()));
        let receiver_log = receivers.clone();
        let reads = Rc::new(RefCell::new(Vec::new()));
        let read_log = reads.clone();
        let getter: Rc<dyn Fn(&TelegramWorker<Vec<u16>, Rc<Cell<u32>>>) -> Rc<Cell<u32>>> =
            Rc::new(move |receiver| {
                receiver_log.borrow_mut().push(receiver as *const _);
                read_log.borrow_mut().push(receiver.token.len());
                count.set(count.get() + 1);
                cursor.borrow().clone()
            });
        let worker = TelegramWorker::new(getter.clone(), token.to_vec());
        assert_eq!(worker.token, token, "TELEGRAM native opaque token");
        assert_eq!(
            worker.tick_counter.get().to_bits(),
            0.0_f64.to_bits(),
            "TELEGRAM native initial counter"
        );
        assert_eq!(
            worker.seen_count(),
            0,
            "TELEGRAM native initial seen cardinality"
        );
        assert_eq!(
            calls.get(),
            0,
            "TELEGRAM native constructor does not invoke callback"
        );
        assert!(
            !worker.has_seen(0.0) && !worker.has_seen(f64::NAN),
            "TELEGRAM native initial seen set"
        );
        assert!(
            Rc::ptr_eq(&worker.get_exchange, &getter),
            "TELEGRAM native callback identity"
        );
        let a = worker.exchange();
        assert!(Rc::ptr_eq(&a, &first), "TELEGRAM native first exchange");
        assert_eq!(a.get(), 17);
        assert_eq!(calls.get(), 1, "TELEGRAM native getter first call count");
        *current.borrow_mut() = second.clone();
        let b = worker.exchange();
        assert!(Rc::ptr_eq(&b, &second), "TELEGRAM native live exchange");
        assert_eq!(b.get(), 42);
        assert_eq!(calls.get(), 2, "TELEGRAM native getter total call count");
        assert!(
            receivers
                .borrow()
                .iter()
                .all(|&r| std::ptr::eq(r, worker.as_ref())),
            "TELEGRAM native callback receiver identity"
        );
        assert_eq!(
            *reads.borrow(),
            vec![token.len(), token.len()],
            "TELEGRAM native receiver token reads"
        );
        assert_eq!(
            rows[index],
            json!({"token":token,"counter":"0000000000000000","seen":0,"callbackSame":true,"firstSame":true,"secondSame":true,"receiverSame":true,"reads":[token.len(),token.len()],"calls":2})
        );
    }
}

#[test]
fn message_dispatch_preserves_commands_duplicates_and_error_prefixes() {
    let source = source();
    let rows = source["direct"].as_array().unwrap();
    assert_eq!(rows.len(), 18, "TELEGRAM independent message case count");
    let cases = [
        ("positions", "ok", "/positions"),
        ("positions-space", "ok", "/positions "),
        ("unknown", "ok", "x"),
        ("cli", "ok", "/cli BTC buy 0.01"),
        ("usage-symbol", "ok", "/cli BTC"),
        ("usage-empty", "ok", "/cli "),
        ("unicode-space", "ok", "/cli \tETH\u{a0}sell\u{feff}2  "),
        ("prefix-tab", "ok", "/cli\tBTC buy 1"),
        ("nel", "ok", "/cli \u{85}BTC buy"),
        ("line-space", "ok", "/cli BTC\tbuy\n1"),
        ("trailing", "ok", "/cli BTC buy "),
        ("usage-other", "ok", "/cli /positions"),
        ("sync", "sync", "/cli BTC buy 1"),
        ("async-swap", "async", "/cli BTC buy 1"),
        ("none", "none", "/cli BTC buy 1"),
        ("getter", "getter", "/cli BTC buy 1"),
        ("duplicate", "ok", "/positions"),
        ("repeat", "ok", "/positions"),
    ];
    for (index, (label, mode, text)) in cases.into_iter().enumerate() {
        let s = setup(mode);
        let mut messages = vec![message(text, 42.0)];
        if label == "duplicate" {
            messages.push(message(text, 42.0));
        }
        s.worker
            .process_messages(s.host.clone(), &messages)
            .unwrap();
        let prefix = s.host.effects.borrow().clone();
        if label == "sync" {
            assert_eq!(
                prefix,
                vec![
                    Effect::Exchange("bybit".into()),
                    Effect::Exec("bybit".into(), "BTC".into(), "buy 1".into()),
                    Effect::Exchange("bybit".into()),
                    Effect::Log(
                        "bybit".into(),
                        "exec-log".into(),
                        "Command failed: sync".into()
                    )
                ],
                "TELEGRAM native synchronous catch prefix"
            );
        }
        if label == "getter" {
            assert_eq!(
                prefix,
                vec![
                    Effect::GetterError,
                    Effect::Exchange("bybit".into()),
                    Effect::Log(
                        "bybit".into(),
                        "exec-log".into(),
                        "Command failed: getter".into()
                    )
                ],
                "TELEGRAM native getter catch prefix"
            );
        }
        if label == "async-swap" {
            *s.current.borrow_mut() = s.second.clone();
        }
        if label == "repeat" {
            s.worker
                .process_messages(s.host.clone(), &messages)
                .unwrap();
        }
        drain(&s, 0);
        let ex = |name: &str| Effect::Exchange(name.into());
        let exec = |symbol: &str, command: &str| {
            Effect::Exec("bybit".into(), symbol.into(), command.into())
        };
        let expected = match label {
            "positions" => vec![ex("bybit"), expected_post(POSITION_TEXT)],
            "duplicate" | "repeat" => vec![
                ex("bybit"),
                expected_post(POSITION_TEXT),
                ex("bybit"),
                expected_post(POSITION_TEXT),
            ],
            "usage-symbol" | "usage-empty" | "usage-other" => vec![expected_post(USAGE)],
            "cli" => vec![ex("bybit"), exec("BTC", "buy 0.01")],
            "unicode-space" => vec![ex("bybit"), exec("ETH", "sell 2 ")],
            "nel" => vec![ex("bybit"), exec("\u{85}BTC", "buy")],
            "line-space" => vec![ex("bybit"), exec("BTC", "buy 1")],
            "trailing" => vec![ex("bybit"), exec("BTC", "buy ")],
            "sync" => vec![
                ex("bybit"),
                exec("BTC", "buy 1"),
                ex("bybit"),
                Effect::Log(
                    "bybit".into(),
                    "exec-log".into(),
                    "Command failed: sync".into(),
                ),
            ],
            "async-swap" => vec![
                ex("bybit"),
                exec("BTC", "buy 1"),
                ex("binance"),
                Effect::Log(
                    "binance".into(),
                    "exec-log".into(),
                    "Command failed: async".into(),
                ),
            ],
            "getter" => vec![
                Effect::GetterError,
                ex("bybit"),
                Effect::Log(
                    "bybit".into(),
                    "exec-log".into(),
                    "Command failed: getter".into(),
                ),
            ],
            _ => vec![],
        };
        assert_eq!(
            *s.host.effects.borrow(),
            expected,
            "TELEGRAM native message effects {label}"
        );
        counter(&s, 0.0);
        assert!(
            !s.worker.has_seen(42.0),
            "TELEGRAM native direct process does not cache"
        );
        assert!(s.host.clock.inner.exceptions.borrow().is_empty());
        compare(&s, &rows[index], label);
        assert_eq!(
            view(&prefix),
            rows[index]["prefix"],
            "TELEGRAM source/native pre-poll prefix"
        );
        assert_eq!(rows[index]["seen"], false);
        assert_eq!(rows[index]["getCalls"], s.calls.get());
    }
    assert_eq!(source["loneSymbol"], 0xd800);
    assert!(
        serde_json::from_str::<TelegramMessage>(
            r#"{"update_id":1,"message":{"chat":{"id":1},"text":"/cli \ud800 buy 1"}}"#
        )
        .is_err(),
        "TELEGRAM declared runtime UTF8 refusal"
    );
}

#[test]
fn nine_warm_ticks_cache_ignored_ids_and_preserve_request_settlement() {
    let source = source();
    let rows = source["trace"].as_array().unwrap();
    assert_eq!(rows.len(), 14, "TELEGRAM independent tick trace count");
    let s = setup("ok");
    let mut expected = Vec::new();
    let queries = [1.0, 42.0, 43.0, 44.0, 45.0, 100.0, f64::NAN, 0.0];
    let check = |index: usize, label: &str, expected: &[Effect], n: f64, seen: [bool; 8]| {
        assert_eq!(
            *s.host.effects.borrow(),
            expected,
            "TELEGRAM native tick effects {label}"
        );
        counter(&s, n);
        let actual = queries.map(|id| s.worker.has_seen(id));
        assert_eq!(actual, seen, "TELEGRAM native cache {label}");
        compare(&s, &rows[index], label);
        assert_eq!(rows[index]["seen"], json!(seen));
    };
    for i in 1_u32..=9 {
        let mut batch = vec![message(
            "/positions",
            if i == 1 { 1.0 } else { 20.0 + f64::from(i) },
        )];
        if i == 1 {
            batch.extend([message("ignored", f64::NAN), message("ignored", -0.0)]);
        }
        s.host.frames.borrow_mut().push_back(Frame::Ready(batch));
        run(s.worker.tick(s.host.clone())).unwrap();
        drain(&s, 0);
        assert!(
            s.worker.has_seen(f64::from_bits(0x7ff0_0000_0000_0001)),
            "TELEGRAM native signaling NaN seen"
        );
        assert!(
            s.worker.has_seen(f64::from_bits(0xfff8_0000_0000_0042)),
            "TELEGRAM native negative payload NaN seen"
        );
        expected.extend([Effect::Get(GET_URL.into()), Effect::Timer(1000)]);
        check(
            (i - 1) as usize,
            &format!("warm-{i}"),
            &expected,
            f64::from(i),
            [true, false, false, false, false, false, true, true],
        );
    }
    s.host.frames.borrow_mut().push_back(Frame::Ready(vec![
        message("/positions", 1.0),
        message("/positions", 42.0),
        message("/positions", 42.0),
        message("unknown", 43.0),
        message("/cli BTC buy 1", 44.0),
    ]));
    run(s.worker.tick(s.host.clone())).unwrap();
    drain(&s, 0);
    expected.extend([
        Effect::Get(GET_URL.into()),
        Effect::Exchange("bybit".into()),
        expected_post(POSITION_TEXT),
        Effect::Exchange("bybit".into()),
        expected_post(POSITION_TEXT),
        Effect::Exchange("bybit".into()),
        Effect::Exec("bybit".into(), "BTC".into(), "buy 1".into()),
        Effect::Timer(1000),
    ]);
    check(
        9,
        "dispatch-ten",
        &expected,
        10.0,
        [true, true, true, true, false, false, true, true],
    );
    s.host.frames.borrow_mut().push_back(Frame::Ready(vec![
        message("/positions", 42.0),
        message("/positions", 45.0),
        message("/positions", f64::NAN),
        message("/positions", 0.0),
    ]));
    run(s.worker.tick(s.host.clone())).unwrap();
    drain(&s, 0);
    expected.extend([
        Effect::Get(GET_URL.into()),
        Effect::Exchange("bybit".into()),
        expected_post(POSITION_TEXT),
        Effect::Timer(1000),
    ]);
    check(
        10,
        "dispatch-eleven",
        &expected,
        11.0,
        [true, true, true, true, true, false, true, true],
    );
    s.host.frames.borrow_mut().push_back(Frame::Fail);
    let error = run(s.worker.tick(s.host.clone())).unwrap_err();
    assert_eq!(
        error.message, "get fail",
        "TELEGRAM native failed GET error"
    );
    expected.push(Effect::Get(GET_URL.into()));
    check(
        11,
        "get-fail",
        &expected,
        11.0,
        [true, true, true, true, true, false, true, true],
    );
    s.host.frames.borrow_mut().push_back(Frame::Pending);
    let mut future = s.worker.tick(s.host.clone());
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending(),
        "TELEGRAM native pending request"
    );
    expected.push(Effect::Get(GET_URL.into()));
    check(
        12,
        "get-pending",
        &expected,
        11.0,
        [true, true, true, true, true, false, true, true],
    );
    *s.host.pending.borrow_mut() = Some(vec![message("/positions", 100.0)]);
    run(future).unwrap();
    drain(&s, 0);
    expected.extend([
        Effect::Exchange("bybit".into()),
        expected_post(POSITION_TEXT),
        Effect::Timer(1000),
    ]);
    check(
        13,
        "get-settled",
        &expected,
        12.0,
        [true, true, true, true, true, true, true, true],
    );
}

#[test]
fn start_issues_get_before_poll_and_timer_restarts_polling() {
    let source = source();
    let rows = source["startRows"].as_array().unwrap();
    assert_eq!(rows.len(), 3, "TELEGRAM independent start trace count");
    let s = setup("ok");
    s.host.frames.borrow_mut().extend([
        Frame::Ready(vec![message("warm", 1.0)]),
        Frame::Ready(vec![message("warm", 2.0)]),
    ]);
    s.worker.start(s.host.clone());
    assert_eq!(
        *s.host.effects.borrow(),
        vec![Effect::Get(GET_URL.into())],
        "TELEGRAM native start GET prefix"
    );
    counter(&s, 0.0);
    assert!(!s.worker.has_seen(1.0));
    compare(&s, &rows[0], "start-prefix");
    drain(&s, 0);
    assert_eq!(
        *s.host.effects.borrow(),
        vec![Effect::Get(GET_URL.into()), Effect::Timer(1000)],
        "TELEGRAM native start settlement"
    );
    counter(&s, 1.0);
    assert!(s.worker.has_seen(1.0) && !s.worker.has_seen(2.0));
    compare(&s, &rows[1], "start-settled");
    drain(&s, 1000);
    assert_eq!(
        *s.host.effects.borrow(),
        vec![
            Effect::Get(GET_URL.into()),
            Effect::Timer(1000),
            Effect::Get(GET_URL.into()),
            Effect::Timer(1000)
        ],
        "TELEGRAM native fired timer"
    );
    counter(&s, 2.0);
    assert!(s.worker.has_seen(1.0) && s.worker.has_seen(2.0));
    compare(&s, &rows[2], "timer-fired");
}

struct DefaultExecutorHost(Rc<Host>);
impl TelegramHost for DefaultExecutorHost {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.0.runtime()
    }
    fn get_updates(
        &self,
        url: String,
    ) -> Result<ExchangeFuture<'static, Vec<TelegramMessage>>, ExchangeError> {
        self.0.get_updates(url)
    }
    fn post_message(
        &self,
        url: String,
        chat_id: f64,
        text: String,
    ) -> Result<ExchangeFuture<'static, ()>, ExchangeError> {
        self.0.post_message(url, chat_id, text)
    }
}
#[test]
fn default_host_reads_registered_executor_and_omits_request_id() {
    use safe_cex_webworker::command::{register_command_executor, reset_command_executor};
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            reset_command_executor();
        }
    }
    reset_command_executor();
    let _reset = Reset;
    let s = setup("ok");
    let host = Rc::new(DefaultExecutorHost(s.host.clone()));
    s.worker
        .process_messages(host.clone(), &[message("/cli BTC buy 1", 42.0)])
        .unwrap();
    assert!(
        s.host.effects.borrow().is_empty(),
        "TELEGRAM native absent default executor no-op"
    );
    let effects = s.host.effects.clone();
    register_command_executor(Rc::new(move |exchange, symbol, command, req_id| {
        assert!(
            req_id.is_none(),
            "TELEGRAM native default executor request ID omitted"
        );
        effects.borrow_mut().push(Effect::Exec(
            label(&exchange.base().name).into(),
            symbol,
            command,
        ));
        Box::pin(async { Err(ExchangeError::new("registered")) })
    }));
    s.worker
        .process_messages(host, &[message("/cli BTC buy 1", 42.0)])
        .unwrap();
    assert_eq!(
        *s.host.effects.borrow(),
        vec![
            Effect::Exchange("bybit".into()),
            Effect::Exec("bybit".into(), "BTC".into(), "buy 1".into())
        ],
        "TELEGRAM native default executor synchronous invocation"
    );
    drain(&s, 0);
    assert_eq!(
        *s.host.effects.borrow(),
        vec![
            Effect::Exchange("bybit".into()),
            Effect::Exec("bybit".into(), "BTC".into(), "buy 1".into()),
            Effect::Exchange("bybit".into()),
            Effect::Log(
                "bybit".into(),
                "exec-log".into(),
                "Command failed: registered".into()
            )
        ],
        "TELEGRAM native default executor rejection catch"
    );
}
