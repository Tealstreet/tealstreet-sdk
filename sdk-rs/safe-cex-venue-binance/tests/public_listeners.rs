//! Native assertions precede every source comparison; no native output roundtrip.
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::{Candle, OrderBook};
use safe_cex_venue_binance::{
    orderbook::*, private_event_value::*, protocol_types::BinanceOrderBookApplyResult as Applied,
    public_listeners::*, request_weight::Failure,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn type_error(message: impl Into<String>) -> Failure {
    let mut e = ExchangeError::new(message);
    e.name = "TypeError".into();
    Rc::new(e)
}
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/public-listeners.json")).unwrap()
}
fn event(v: Value) -> EventValue {
    EventValue::from_json(v)
}
fn arr(v: EventValue) -> EventValue {
    EventValue::array(vec![v])
}
fn wire(v: &EventValue) -> Value {
    match v {
        EventValue::Undefined => json!({"$undefined":true}),
        EventValue::Null => Value::Null,
        EventValue::Number(n) => {
            if n.is_nan() {
                json!({"$nan":true})
            } else {
                json!(n)
            }
        }
        EventValue::Bool(v) => json!(v),
        EventValue::String(s) => json!(String::from_utf16(s).unwrap()),
        EventValue::Array(a) => array_values(a).iter().map(wire).collect(),
        EventValue::Object(_) => v
            .spread()
            .iter()
            .map(|(k, v)| (k.clone(), wire(v)))
            .collect(),
        _ => panic!("prepared fixture excludes inherited callables"),
    }
}
fn error() -> Failure {
    let mut e = ExchangeError::new("listener fixture");
    e.name = "RangeError".into();
    e.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
        -1000
    )));
    e = e.with_retained_response((json!({"status":503,"data":{"msg":"listener detail"}})).into());
    Rc::new(e)
}
struct Timer {
    action: Action,
    delay: f64,
    id: f64,
    cancelled: bool,
}
struct Host {
    state: ListenerState,
    case: String,
    primary: Failure,
    sent: RefCell<Vec<EventValue>>,
    timers: RefCell<Vec<Timer>>,
    cleared: RefCell<Vec<EventValue>>,
    cleaned: RefCell<Vec<String>>,
    jobs: RefCell<Vec<Job>>,
    failures: RefCell<Vec<Failure>>,
    gets: RefCell<Vec<(Venue, String, f64)>>,
    ops: RefCell<Vec<&'static str>>,
    applies: RefCell<Vec<(EventValue, bool)>>,
    buffered: RefCell<Vec<EventValue>>,
    books: RefCell<Vec<Book>>,
    book_values: RefCell<Vec<OrderBook>>,
    candles: RefCell<Vec<Candle>>,
    nano: Cell<usize>,
    fixture: Value,
    load_data: RefCell<Vec<EventValue>>,
}
impl Host {
    fn new(case: &str) -> Rc<Self> {
        let h = Rc::new(Self {
            state: ListenerState::default(),
            case: case.into(),
            primary: error(),
            sent: Default::default(),
            timers: Default::default(),
            cleared: Default::default(),
            cleaned: Default::default(),
            jobs: Default::default(),
            failures: Default::default(),
            gets: Default::default(),
            ops: Default::default(),
            applies: Default::default(),
            buffered: Default::default(),
            books: Default::default(),
            book_values: Default::default(),
            candles: Default::default(),
            nano: Cell::new(0),
            fixture: fixture(),
            load_data: Default::default(),
        });
        h.state.connected.set(!matches!(
            case,
            "delayed"
                | "unsubscribe-wait"
                | "dispose-wait"
                | "timer-error"
                | "zero-ticket"
                | "delayed-unsubscribe"
        ));
        h.state.disposed.set(case == "disposed");
        h
    }
    fn observe(&self, result: Result<(), Failure>) {
        if let Err(e) = result {
            self.failures.borrow_mut().push(e);
        }
    }
    fn fire(&self) {
        let actions: Vec<_> = self
            .timers
            .borrow_mut()
            .iter_mut()
            .filter(|t| !t.cancelled)
            .map(|t| {
                t.cancelled = true;
                t.action.clone()
            })
            .collect();
        for action in actions {
            self.observe(action());
        }
    }
    fn settle(&self) {
        let jobs = std::mem::take(&mut *self.jobs.borrow_mut());
        for mut job in jobs {
            let mut cx = Context::from_waker(Waker::noop());
            let Poll::Ready(r) = job.as_mut().poll(&mut cx) else {
                panic!("fixture job ready")
            };
            self.observe(r);
        }
    }
    fn parser(&self, key: &str) -> Option<Parser> {
        self.state
            .handlers
            .borrow()
            .get(key)
            .and_then(|a| a.first())
            .cloned()
    }
    fn dispatch(&self, key: &str, data: EventValue) {
        let callbacks = self
            .state
            .handlers
            .borrow()
            .get(key)
            .cloned()
            .unwrap_or_default();
        for p in callbacks {
            self.observe(p(arr(data.clone())));
        }
    }
    fn json(&self) -> Value {
        json!({"sent":self.sent.borrow().iter().map(wire).collect::<Vec<_>>(),"timers":self.timers.borrow().iter().map(|t|json!({"delay":t.delay,"id":t.id})).collect::<Vec<_>>(),"cleared":self.cleared.borrow().iter().map(wire).collect::<Vec<_>>(),"cleanup":*self.cleaned.borrow(),"handlers":self.state.handlers.borrow().iter().map(|(k,v)|(k.clone(),json!(v.len()))).collect::<serde_json::Map<_,_>>(),"topics":self.state.topics.borrow().iter().map(|(k,v)|(k.clone(),json!(v))).collect::<serde_json::Map<_,_>>(),"nextId":self.state.next_subscription_id.get(),"candles":*self.candles.borrow(),"books":*self.book_values.borrow(),"gets":self.gets.borrow().iter().map(|(v,s,l)|json!({"client":if *v==Venue::Spot{"xhr"}else{"publicXhr"},"endpoint":if *v==Venue::Spot{"/api/v3/depth"}else{"/fapi/v1/depth"},"symbol":s,"limit":l})).collect::<Vec<_>>(),"applies":self.applies.borrow().iter().map(|(d,p)|json!({"u":wire(&d.read("u").unwrap()),"previous":p})).collect::<Vec<_>>(),"ops":*self.ops.borrow(),"failures":self.failures.borrow().iter().map(|e|json!({"original":Rc::ptr_eq(e,&self.primary),"name":e.name,"message":e.message,"code":observed_error_code(&e.code),"response":e.response_json_view()})).collect::<Vec<_>>(),"inner":self.state.inner.borrow().iter().map(|(k,v)|(k.clone(),json!({"loaded":v.snapshot_loaded.get(),"updates":v.updates.borrow().iter().map(wire).collect::<Vec<_>>()}))).collect::<serde_json::Map<_,_>>()})
    }
}
impl ListenerHost for Host {
    fn state(&self) -> &ListenerState {
        &self.state
    }
    fn send(&self, p: EventValue) -> Result<(), Failure> {
        self.ops.borrow_mut().push("send");
        self.sent.borrow_mut().push(p);
        if self.case == "send-error" {
            return Err(self.primary.clone());
        }
        Ok(())
    }
    fn set_timeout(&self, a: Action, delay: f64) -> Result<EventValue, Failure> {
        self.ops.borrow_mut().push("timer");
        if self.case == "timer-error" {
            return Err(self.primary.clone());
        }
        let id = if self.case == "zero-ticket" {
            0.0
        } else {
            self.timers.borrow().len() as f64 + 1.0
        };
        self.timers.borrow_mut().push(Timer {
            action: a,
            delay,
            id,
            cancelled: false,
        });
        Ok(number(id))
    }
    fn clear_timeout(&self, t: EventValue) -> Result<(), Failure> {
        self.ops.borrow_mut().push("clear");
        self.cleared.borrow_mut().push(t.clone());
        for timer in self.timers.borrow_mut().iter_mut() {
            if t.same(&number(timer.id)) {
                timer.cancelled = true;
            }
        }
        Ok(())
    }
    fn cleanup(&self, key: &str) -> Result<(), Failure> {
        self.ops.borrow_mut().push("cleanup");
        self.cleaned.borrow_mut().push(key.into());
        if self.case == "cleanup-error" {
            Err(self.primary.clone())
        } else {
            Ok(())
        }
    }
    fn nanoid(&self) -> Result<String, Failure> {
        self.ops.borrow_mut().push("nanoid");
        let n = self.nano.get() + 1;
        self.nano.set(n);
        Ok(format!("inner-{n}"))
    }
    fn get_snapshot(&self, v: Venue, s: &str, l: f64) -> Result<Snapshot, Failure> {
        self.ops.borrow_mut().push("get");
        let n = self.gets.borrow().len() + 1;
        self.gets.borrow_mut().push((v, s.into(), l));
        if self.case == "get-sync" && n == 1 {
            return Err(self.primary.clone());
        }
        let err = (self.case == "get-reject" && n == 1).then(|| self.primary.clone());
        let mut data = self.fixture["snapshot"].clone();
        if n > 1 {
            data["lastUpdateId"] = json!(if self.case == "pu-mismatch" { 102 } else { 105 });
        }
        let data = event(data);
        Ok(Box::pin(async move { err.map_or(Ok(data), Err) }))
    }
    fn spawn(&self, job: Job) {
        self.jobs.borrow_mut().push(job);
    }
    fn reset_sync(&self, s: SyncState) -> Result<(), Failure> {
        self.ops.borrow_mut().push("reset");
        reset_binance_order_book_sync_state(&mut s.borrow_mut());
        if self.case.starts_with("reset-removes")
            && self
                .ops
                .borrow()
                .iter()
                .filter(|op| **op == "reset")
                .count()
                == 2
        {
            self.state
                .handlers
                .borrow_mut()
                .remove("depthUpdate.BTCUSDT");
        }
        Ok(())
    }
    fn load(&self, b: Book, data: EventValue, s: SyncState) -> Result<(), Failure> {
        self.ops.borrow_mut().push("load");
        self.load_data.borrow_mut().push(data.clone());
        if self.case == "load-reject" && self.gets.borrow().len() == 1 {
            return Err(self.primary.clone());
        }
        let rows = |key: &str| {
            let EventValue::Array(a) = data.get(key).unwrap() else {
                panic!("authored levels")
            };
            array_values(&a)
                .iter()
                .map(|r| {
                    vec![
                        r.read("0").unwrap().text().unwrap(),
                        r.read("1").unwrap().text().unwrap(),
                    ]
                })
                .collect::<Vec<_>>()
        };
        load_binance_order_book_snapshot(
            &mut b.borrow_mut(),
            &rows("bids"),
            &rows("asks"),
            data.read("lastUpdateId")?.number()?,
            &mut s.borrow_mut(),
        );
        Ok(())
    }
    fn buffered(
        &self,
        updates: Vec<EventValue>,
        id: EventValue,
    ) -> Result<Vec<EventValue>, Failure> {
        self.ops.borrow_mut().push("buffer");
        let mut out = vec![];
        for u in updates {
            if u.read("u")?.number()? > id.number()? {
                self.buffered.borrow_mut().push(u.clone());
                out.push(u);
            }
        }
        Ok(out)
    }
    fn apply(
        &self,
        b: Book,
        d: EventValue,
        s: SyncState,
        previous: bool,
    ) -> Result<Applied, Failure> {
        self.ops.borrow_mut().push("apply");
        self.applies.borrow_mut().push((d.clone(), previous));
        if self.case == "apply-reject" && self.gets.borrow().len() == 1 {
            return Err(self.primary.clone());
        }
        let map = wire(&d).as_object().unwrap().clone();
        apply_binance_depth_update(&mut b.borrow_mut(), &map, &mut s.borrow_mut(), previous)
            .map_err(|e| type_error(e.message))
    }
    fn total(&self, b: Book) -> Result<(), Failure> {
        self.ops.borrow_mut().push("total");
        if self.case == "total-reject" && self.gets.borrow().len() == 1 {
            return Err(self.primary.clone());
        }
        safe_cex_store::orderbook::calc_order_book_total(&mut b.borrow_mut());
        Ok(())
    }
}
fn check_failures(h: &Host, n: usize, label: &str) {
    assert_eq!(
        h.failures.borrow().len(),
        n,
        "{label}: native failure count"
    );
    for e in h.failures.borrow().iter() {
        assert!(
            Rc::ptr_eq(e, &h.primary),
            "{label}: native original Error identity"
        );
        assert_eq!(e.name, "RangeError", "{label}: native error name");
        assert_eq!(
            e.message, "listener fixture",
            "{label}: native error message"
        );
        assert_eq!(
            observed_error_code(&e.code),
            Some(json!(-1000)),
            "{label}: native error code"
        );
        assert_eq!(
            e.response_json_view(),
            Some(json!({"status":503,"data":{"msg":"listener detail"}})),
            "{label}: native error response"
        );
    }
}
fn check_payloads(h: &Host, id_dynamic: bool, book: bool, label: &str) {
    for (i, p) in h.sent.borrow().iter().enumerate() {
        let method = p.read("method").unwrap();
        assert!(
            matches!(method,EventValue::String(v)if v.as_ref()=="SUBSCRIBE".encode_utf16().collect::<Vec<_>>()||v.as_ref()=="UNSUBSCRIBE".encode_utf16().collect::<Vec<_>>()),
            "{label}: native method"
        );
        let EventValue::Number(id) = p.read("id").unwrap() else {
            panic!("{label}: native id type")
        };
        assert_eq!(
            id,
            if book {
                3.0
            } else if id_dynamic {
                i as f64 + 2.0
            } else {
                2.0
            },
            "{label}: native subscription ID"
        );
        let EventValue::Array(a) = p.read("params").unwrap() else {
            panic!("native params")
        };
        assert_eq!(a.borrow().len(), 1, "{label}: native params count");
        assert!(
            a.borrow()[0].as_ref().unwrap().same(&string(if book {
                "btcusdt@depth"
            } else {
                "btcusdt@kline_1m"
            })),
            "{label}: native captured topic"
        );
    }
}
fn native_methods(h: &Host, expected: &[&str], label: &str) {
    assert_eq!(
        h.sent.borrow().len(),
        expected.len(),
        "{label}: native method count"
    );
    for (p, m) in h.sent.borrow().iter().zip(expected) {
        assert!(
            p.read("method").unwrap().same(&string(*m)),
            "{label}: native method {m}, got {:?}",
            p.read("method").unwrap()
        );
    }
}
fn candle_case(v: Venue, name: &str) -> Value {
    let h = Host::new(name);
    if name == "initial-nan" {
        h.state
            .topics
            .borrow_mut()
            .insert("btcusdt@kline_1m".into(), f64::NAN);
    }
    let opts = event(json!({"symbol":"BTCUSDT","interval":"1m"}));
    let cbh = h.clone();
    let callback: Rc<dyn Fn(Candle) -> Result<(), Failure>> = Rc::new(move |c| {
        cbh.ops.borrow_mut().push("candle");
        cbh.candles.borrow_mut().push(c);
        if cbh.case == "callback-error" {
            Err(cbh.primary.clone())
        } else {
            Ok(())
        }
    });
    let listen = match v {
        Venue::Futures => listen_futures_market,
        Venue::PM => listen_pm_market,
        Venue::Spot => listen_spot_candles,
    };
    let result = listen(h.clone(), opts.clone(), callback.clone());
    let mut unsub = None;
    match result {
        Ok(u) => unsub = Some(u),
        Err(e) => h.failures.borrow_mut().push(e),
    }
    let mut second = None;
    if name == "shared" {
        second = Some(listen(h.clone(), opts.clone(), callback).unwrap());
    }
    if matches!(name, "unsubscribe-wait" | "zero-ticket") {
        if let Some(u) = &unsub {
            h.observe(u());
        }
    }
    if name == "dispose-wait" {
        h.state.disposed.set(true);
    }
    if matches!(
        name,
        "delayed" | "unsubscribe-wait" | "dispose-wait" | "zero-ticket"
    ) {
        h.state.connected.set(true);
        h.fire();
    }
    let mut data = h.fixture["candle"].clone();
    if name == "live-opts" {
        opts.write("symbol", string("ETHUSDT"));
        opts.write("interval", string("5m"));
        data["k"]["s"] = json!("ETHUSDT");
        data["k"]["i"] = json!("5m");
    }
    if name == "wrong-events" {
        data["k"]["s"] = json!("ETHUSDT");
        h.dispatch("kline.1m.BTCUSDT", event(data.clone()));
        data["k"]["s"] = json!("BTCUSDT");
        data["k"]["i"] = json!("5m");
    }
    if !matches!(name, "send-error" | "timer-error") {
        h.dispatch("kline.1m.BTCUSDT", event(data));
    }
    if !matches!(name, "unsubscribe-wait" | "zero-ticket") {
        if let Some(u) = &unsub {
            h.observe(u());
        }
        if let Some(u) = second {
            h.observe(u());
        } else if matches!(name, "normal" | "disposed") {
            if let Some(u) = unsub {
                h.observe(u());
            }
        }
    }
    let f = v == Venue::Futures;
    let label = format!("native candles {v:?} {name}");
    let candles = match (f, name) {
        (_, "send-error" | "timer-error" | "wrong-events") => 0,
        (true, "unsubscribe-wait" | "zero-ticket" | "disposed" | "dispose-wait") => 0,
        (_, "shared") => 2,
        _ => 1,
    };
    assert_eq!(
        h.candles.borrow().len(),
        candles,
        "{label}: native candle count"
    );
    for c in h.candles.borrow().iter() {
        assert_eq!(c.timestamp, 1234.567, "{label}: native timestamp");
        assert_eq!(c.open, 12.3, "{label}: native open");
        assert_eq!(c.high, 12.4, "{label}: native high");
        assert_eq!(c.low, 11.9, "{label}: native low");
        assert_eq!(c.close, 12.35, "{label}: native close");
        assert_eq!(c.volume, 3.6, "{label}: native volume");
        assert!(c.last_side.is_none(), "{label}: native absent last side");
    }
    let methods = match (f, name) {
        (_, "timer-error") | (true, "unsubscribe-wait" | "zero-ticket") => vec![],
        (true, "disposed" | "dispose-wait") => vec!["UNSUBSCRIBE"],
        (_, "send-error" | "cleanup-error" | "unsubscribe-wait" | "zero-ticket") => {
            vec!["SUBSCRIBE"]
        }
        (_, "shared") => vec!["SUBSCRIBE", "SUBSCRIBE", "UNSUBSCRIBE"],
        (false, "normal" | "disposed") => vec!["SUBSCRIBE", "UNSUBSCRIBE", "UNSUBSCRIBE"],
        _ => vec!["SUBSCRIBE", "UNSUBSCRIBE"],
    };
    native_methods(&h, &methods, &label);
    check_payloads(&h, f, false, &label);
    let expected_id = if !f {
        2.0
    } else {
        match name {
            "shared" => 5.0,
            "unsubscribe-wait" | "zero-ticket" | "timer-error" => 2.0,
            "disposed" | "dispose-wait" | "send-error" => 3.0,
            _ => 4.0,
        }
    };
    assert_eq!(
        h.state.next_subscription_id.get(),
        expected_id,
        "{label}: native next subscription ID"
    );

    let err = usize::from(matches!(
        name,
        "callback-error" | "send-error" | "timer-error" | "cleanup-error"
    ));
    check_failures(&h, err, &label);
    let remains =
        matches!(name, "send-error") || !f && matches!(name, "unsubscribe-wait" | "zero-ticket");
    assert_eq!(
        h.state
            .handlers
            .borrow()
            .get("kline.1m.BTCUSDT")
            .map_or(0, Vec::len),
        usize::from(remains),
        "{label}: native remaining parser count"
    );
    assert_eq!(
        h.state.topics.borrow().get("btcusdt@kline_1m").copied(),
        remains.then_some(1.0),
        "{label}: native remaining topic count"
    );
    let clears = usize::from(f && matches!(name, "delayed" | "unsubscribe-wait" | "dispose-wait"));
    assert_eq!(
        h.cleared.borrow().len(),
        clears,
        "{label}: native clear count"
    );
    let timers = usize::from(matches!(
        name,
        "delayed" | "unsubscribe-wait" | "dispose-wait" | "zero-ticket"
    ));
    assert_eq!(
        h.timers.borrow().len(),
        timers,
        "{label}: native timer count"
    );
    for t in h.timers.borrow().iter() {
        assert_eq!(t.delay, 100.0, "{label}: native timer delay");
    }
    h.json()
}
fn book_case(v: Venue, name: &str) -> Value {
    let h = Host::new(name);
    let old = if name == "inner-collision" {
        let inner = Rc::new(InnerState::default());
        inner.snapshot_loaded.set(true);
        h.state
            .inner
            .borrow_mut()
            .insert("inner-1".into(), inner.clone());
        Some(inner)
    } else {
        None
    };
    let cbh = h.clone();
    let callback: Rc<dyn Fn(Book) -> Result<(), Failure>> = Rc::new(move |b| {
        cbh.ops.borrow_mut().push("book");
        cbh.book_values.borrow_mut().push(b.borrow().clone());
        cbh.books.borrow_mut().push(b);
        if cbh.case == "callback-reject" && cbh.gets.borrow().len() == 1 {
            Err(cbh.primary.clone())
        } else {
            Ok(())
        }
    });
    let listen = match v {
        Venue::Futures => listen_futures_book,
        Venue::PM => listen_pm_book,
        Venue::Spot => listen_spot_book,
    };
    let unsubscribe = listen(h.clone(), "BTCUSDT".into(), callback).unwrap();
    let saved = h.parser("depthUpdate.BTCUSDT");
    if name == "delayed-unsubscribe" {
        h.observe(unsubscribe());
        h.state.connected.set(true);
        h.fire();
    } else {
        let parser = saved.as_ref().unwrap();
        let mut wrong = h.fixture["first"].clone();
        wrong["s"] = json!("ETHUSDT");
        h.observe(parser(arr(event(wrong))));
        assert!(
            h.gets.borrow().is_empty(),
            "native books {v:?} {name}: wrong symbol no fetch"
        );
        let mut first = h.fixture["first"].clone();
        if name == "gap-buffer" {
            first = h.fixture["gap"].clone();
        }
        if name == "ignored-buffer" {
            first["U"] = json!(99);
            first["u"] = json!(99);
        }
        let first = event(first);
        h.observe(parser(arr(first.clone())));
        if name == "normal" {
            let second = event(h.fixture["second"].clone());
            h.observe(parser(arr(second.clone())));
            let inner = h.state.inner.borrow().get("inner-1").unwrap().clone();
            assert_eq!(
                inner.updates.borrow().len(),
                2,
                "native books {v:?} {name}: native pending buffer count"
            );
            assert!(
                inner.updates.borrow()[0].same(&first),
                "native original buffered event identity"
            );
            assert!(
                inner.updates.borrow()[1].same(&second),
                "native second buffered event identity"
            );
        }
        if name == "disposed-settle" {
            h.state.disposed.set(true);
        }
        if name == "unsubscribe-settle" {
            h.observe(unsubscribe());
        }
        if name == "get-sync" {
            h.observe(unsubscribe());
            h.observe(parser(arr(first.clone())));
            assert_eq!(
                h.gets.borrow().len(),
                2,
                "native books {v:?} get-sync: native same-turn fetch count"
            );
        }
        h.settle();
        if name == "gap-buffer" {
            h.fire();
            h.settle();
        }
        if matches!(name, "gap-live" | "pu-mismatch") {
            let mut next = if name == "gap-live" {
                h.fixture["gap"].clone()
            } else {
                h.fixture["second"].clone()
            };
            if name == "pu-mismatch" {
                next["pu"] = json!(9);
            }
            h.observe(parser(arr(event(next))));
            h.settle();
        }
        if name == "normal" {
            h.observe(parser(arr(event(h.fixture["second"].clone()))));
            h.observe(parser(arr(event(h.fixture["next"].clone()))));
        }
        if matches!(
            name,
            "get-reject" | "load-reject" | "apply-reject" | "total-reject" | "callback-reject"
        ) {
            h.observe(unsubscribe());
            h.observe(parser(arr(first)));
            h.settle();
        }
        if name == "reset-removes-disconnected" {
            h.state.connected.set(false);
        }
        if name != "unsubscribe-settle"
            && name != "get-sync"
            && !matches!(
                name,
                "get-reject"
                    | "get-sync"
                    | "load-reject"
                    | "apply-reject"
                    | "total-reject"
                    | "callback-reject"
            )
        {
            h.observe(unsubscribe());
        }
        if name == "repeated-unsubscribe" {
            h.observe(unsubscribe());
        }
    }
    let label = format!("native books {v:?} {name}");
    let retry = matches!(
        name,
        "gap-buffer"
            | "gap-live"
            | "get-reject"
            | "get-sync"
            | "load-reject"
            | "apply-reject"
            | "total-reject"
            | "callback-reject"
    ) || name == "pu-mismatch" && v != Venue::Spot;
    let gets = if name == "delayed-unsubscribe" {
        0
    } else if retry {
        2
    } else {
        1
    };
    assert_eq!(h.gets.borrow().len(), gets, "{label}: native fetch count");
    for (client, symbol, limit) in h.gets.borrow().iter() {
        assert_eq!(*client, v, "{label}: native client");
        assert_eq!(symbol, "BTCUSDT", "{label}: native symbol");
        assert_eq!(*limit, 1000.0, "{label}: native limit");
    }
    let callbacks = if matches!(name, "disposed-settle" | "delayed-unsubscribe") {
        0
    } else if matches!(
        name,
        "normal" | "gap-live" | "pu-mismatch" | "callback-reject"
    ) {
        2
    } else {
        1
    };
    assert_eq!(
        h.books.borrow().len(),
        callbacks,
        "{label}: native callback count"
    );
    for b in h.books.borrow().iter().skip(1) {
        assert!(
            Rc::ptr_eq(b, &h.books.borrow()[0]),
            "{label}: native callback book identity"
        );
    }
    for (i, book) in h.book_values.borrow().iter().enumerate() {
        assert_eq!(book.asks.len(), 1, "{label}: native ask count");
        assert_eq!(book.asks[0].price, 12.4, "{label}: native ask price");
        let two = name == "normal" || name == "pu-mismatch" && v == Venue::Spot && i == 1;
        assert_eq!(
            book.bids.len(),
            if two { 2 } else { 1 },
            "{label}: native bid count"
        );
        assert_eq!(book.bids[0].price, 12.3, "{label}: native bid price");
        let base = matches!(
            name,
            "gap-buffer"
                | "ignored-buffer"
                | "unsubscribe-settle"
                | "get-reject"
                | "get-sync"
                | "load-reject"
                | "apply-reject"
                | "total-reject"
        ) || i == 1
            && (matches!(name, "gap-live" | "callback-reject")
                || name == "pu-mismatch" && v != Venue::Spot);
        let amount = if base || name == "normal" && i == 1 {
            3.6
        } else {
            4.2
        };
        assert_eq!(book.bids[0].amount, amount, "{label}: native bid amount");
        assert_eq!(book.bids[0].total, amount, "{label}: native bid total");
        if two {
            assert_eq!(book.bids[1].price, 12.2, "{label}: native second price");
            assert_eq!(book.bids[1].amount, 3.6, "{label}: native second amount");
            assert_eq!(
                book.bids[1].total,
                amount + 3.6,
                "{label}: native second total"
            );
        }
        let ask = if two { 2.5 } else { 4.2 };
        assert_eq!(book.asks[0].amount, ask, "{label}: native ask amount");
        assert_eq!(book.asks[0].total, ask, "{label}: native ask total");
    }
    for (d, previous) in h.applies.borrow().iter() {
        assert_eq!(
            *previous,
            v != Venue::Spot,
            "{label}: native previous-final flag"
        );
        assert!(
            matches!(d.read("u").unwrap(), EventValue::Number(_)),
            "{label}: native update ID kind"
        );
    }
    if name == "reset-removes-handler" {
        let errors = h.failures.borrow();
        assert_eq!(errors.len(), 1, "{label}: native generated error count");
        let e = &errors[0];
        assert!(
            !Rc::ptr_eq(e, &h.primary),
            "{label}: native generated error identity"
        );
        assert_eq!(e.name, "TypeError", "{label}: native generated error name");
        assert_eq!(
            e.message, "Cannot read properties of undefined (reading 'length')",
            "{label}: native late handler error"
        );
        assert!(
            e.code.is_none() && e.response_json_view().is_none(),
            "{label}: native generated absent metadata"
        );
    } else {
        check_failures(
            &h,
            usize::from(matches!(
                name,
                "get-reject"
                    | "get-sync"
                    | "load-reject"
                    | "apply-reject"
                    | "total-reject"
                    | "callback-reject"
            )),
            &label,
        );
    }
    let inner = h.state.inner.borrow().get("inner-1").unwrap().clone();
    if let Some(old) = old {
        assert!(
            Rc::ptr_eq(&old, &inner),
            "{label}: native collision inner identity"
        );
    }
    assert_eq!(
        h.state.inner.borrow().len(),
        1,
        "{label}: native retained inner count"
    );
    assert_eq!(
        h.state
            .handlers
            .borrow()
            .get("depthUpdate.BTCUSDT")
            .map_or(0, Vec::len),
        0,
        "{label}: native remaining parser count"
    );
    assert!(
        h.state.topics.borrow().is_empty(),
        "{label}: native topics removed"
    );
    assert_eq!(
        h.sent.borrow().len(),
        if name == "delayed-unsubscribe" {
            0
        } else if name.starts_with("reset-removes") {
            1
        } else if name == "repeated-unsubscribe" {
            3
        } else {
            2
        },
        "{label}: native send count"
    );
    check_payloads(&h, false, true, &label);
    let methods = if name == "delayed-unsubscribe" {
        vec![]
    } else if name.starts_with("reset-removes") {
        vec!["SUBSCRIBE"]
    } else if name == "repeated-unsubscribe" {
        vec!["SUBSCRIBE", "UNSUBSCRIBE", "UNSUBSCRIBE"]
    } else {
        vec!["SUBSCRIBE", "UNSUBSCRIBE"]
    };
    native_methods(&h, &methods, &label);
    assert_eq!(
        inner.snapshot_loaded.get(),
        matches!(
            name,
            "get-reject"
                | "get-sync"
                | "load-reject"
                | "apply-reject"
                | "total-reject"
                | "callback-reject"
                | "unsubscribe-settle"
        ),
        "{label}: native final snapshot flag"
    );
    assert!(
        inner.updates.borrow().is_empty(),
        "{label}: native final updates count0"
    );
    if name.starts_with("reset-removes") {
        assert!(
            !h.state
                .handlers
                .borrow()
                .contains_key("depthUpdate.BTCUSDT"),
            "{label}: native handler removal"
        );
    }
    if !inner.snapshot_loaded.get() {
        for b in h.books.borrow().iter() {
            assert!(
                b.borrow().bids.is_empty() && b.borrow().asks.is_empty(),
                "{label}: native shared callback book reset"
            );
        }
    }

    assert_eq!(
        h.cleared.borrow().len(),
        usize::from(name == "delayed-unsubscribe"),
        "{label}: native clear count"
    );
    if name == "gap-buffer" {
        assert_eq!(
            h.timers.borrow().len(),
            1,
            "{label}: native retry timer count"
        );
        assert_eq!(
            h.timers.borrow()[0].delay,
            0.0,
            "{label}: native retry delay"
        );
    }
    h.json()
}
fn source(target: &str) -> Value {
    let o = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-listeners.mjs"
        ))
        .arg(target)
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "oracle stderr {}",
        String::from_utf8_lossy(&o.stderr)
    );
    serde_json::from_slice(&o.stdout).unwrap()
}
fn numbers(v: Value) -> Value {
    match v {
        Value::Number(n) => json!(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.into_iter().map(numbers).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, numbers(v))).collect()),
        v => v,
    }
}
fn run_candles(v: Venue, target: &str) {
    let f = fixture();
    let cases = f["candles"].as_array().unwrap();
    assert_eq!(cases.len(), 14, "authored candle case floor");
    let native: Vec<_> = cases
        .iter()
        .map(|c| candle_case(v, c.as_str().unwrap()))
        .collect();
    let wire = source(target);
    assert_eq!(wire.as_array().unwrap().len(), 14, "source candle floor");
    for (i, (a, b)) in native.into_iter().zip(wire.as_array().unwrap()).enumerate() {
        let a = numbers(a);
        let b = numbers(b.clone());
        if a != b {
            eprintln!("source {target} case{i}: native {a} source {b}");
        }
        assert_eq!(a, b, "source {target} case{i}");
    }
}
fn run_books(v: Venue, target: &str) {
    let f = fixture();
    let cases = f["books"].as_array().unwrap();
    assert_eq!(cases.len(), 18, "authored book case floor");
    let native: Vec<_> = cases
        .iter()
        .map(|c| book_case(v, c.as_str().unwrap()))
        .collect();
    let wire = source(target);
    assert_eq!(wire.as_array().unwrap().len(), 18, "source book floor");
    for (i, (a, b)) in native.into_iter().zip(wire.as_array().unwrap()).enumerate() {
        let a = numbers(a);
        let b = numbers(b.clone());
        if a != b {
            eprintln!("source {target} case{i}: native {a} source {b}");
        }
        assert_eq!(a, b, "source {target} case{i}");
    }
}
#[test]
#[ignore = "mandatory live oracle"]
fn candles_futures() {
    run_candles(Venue::Futures, "candles_futures")
}
#[test]
#[ignore = "mandatory live oracle"]
fn candles_pm() {
    run_candles(Venue::PM, "candles_pm")
}
#[test]
#[ignore = "mandatory live oracle"]
fn candles_spot() {
    run_candles(Venue::Spot, "candles_spot")
}
#[test]
#[ignore = "mandatory live oracle"]
fn books_futures() {
    run_books(Venue::Futures, "books_futures")
}
#[test]
#[ignore = "mandatory live oracle"]
fn books_pm() {
    run_books(Venue::PM, "books_pm")
}
#[test]
#[ignore = "mandatory live oracle"]
fn books_spot() {
    run_books(Venue::Spot, "books_spot")
}
fn run_constructors(pm: bool, target: &str) {
    let mut native = vec![];
    for name in ["normal", "super-error", "market-error"] {
        let parent = Rc::new(RefCell::new(12.3_f64));
        let base = Rc::new(3.6);
        let market = Rc::new(4.2);
        let trace = Rc::new(RefCell::new(vec![]));
        let p = parent.clone();
        let b = base.clone();
        let t = trace.clone();
        let e = error();
        let err = e.clone();
        let sup = move |arg| {
            assert!(
                Rc::ptr_eq(&p, &arg),
                "native constructor super parent identity"
            );
            assert_eq!(*arg.borrow(), 12.3, "native constructor parent value");
            t.borrow_mut().push("super");
            if name == "super-error" {
                Err(err)
            } else {
                Ok(b)
            }
        };
        let p = parent.clone();
        let m = market.clone();
        let t = trace.clone();
        let err = e.clone();
        let make = move |arg| {
            assert!(
                Rc::ptr_eq(&p, &arg),
                "native constructor sibling parent identity"
            );
            t.borrow_mut().push("market");
            if name == "market-error" {
                Err(err)
            } else {
                Ok(m)
            }
        };
        let r = if pm {
            construct_pm_public(parent, sup, make)
        } else {
            construct_futures_public(parent, sup, make)
        };
        let success = r.is_ok();
        match r {
            Ok(r) => {
                assert!(
                    Rc::ptr_eq(&r.base, &base),
                    "native constructor base identity"
                );
                assert!(
                    Rc::ptr_eq(&r.market_websocket, &market),
                    "native constructor sibling identity"
                );
                assert_eq!(*r.market_websocket, 4.2, "native constructor market value");
            }
            Err(err) => assert!(
                Rc::ptr_eq(&err, &e),
                "native constructor original error identity"
            ),
        };
        assert_eq!(
            *trace.borrow(),
            if name == "super-error" {
                vec!["super"]
            } else {
                vec!["super", "market"]
            },
            "native constructor call order"
        );
        native.push(json!({"trace":*trace.borrow(),"success":success,"original":!success,"marketValue":if success{Some(4.2)}else{None}}));
    }
    assert_eq!(json!(native), source(target), "source constructor result");
}
#[test]
#[ignore = "mandatory live oracle"]
fn constructors_futures() {
    run_constructors(false, "constructors_futures")
}
#[test]
#[ignore = "mandatory live oracle"]
fn constructors_pm() {
    run_constructors(true, "constructors_pm")
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
