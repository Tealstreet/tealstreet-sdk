#![allow(dead_code, unused_imports)]
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
    serde_json::from_str(include_str!("../fixtures/public-listeners.json")).unwrap()
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
        EventValue::Array(a) => array_values(a)
            .expect("fixture array reads")
            .iter()
            .map(wire)
            .collect(),
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
    e = e.with_retained_response(json!({"status":503,"data":{"msg":"listener detail"}}).into());
    Rc::new(e)
}
struct Timer {
    action: Action,
    delay: f64,
    id: f64,
    cancelled: bool,
}
pub struct Host {
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
    pub fn new(case: &str) -> Rc<Self> {
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
                .expect("fixture array reads")
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
