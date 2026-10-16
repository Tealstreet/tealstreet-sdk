use safe_cex_venue_backpack::{
    orderbook_state::*,
    private_control as private,
    public_control::{JsString, Message, Subscriptions},
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
type Error = Rc<()>;
fn oracle() -> &'static Value {
    static V: OnceLock<Value> = OnceLock::new();
    V.get_or_init(|| {
        let out = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/orderbook_state_oracle.cjs"
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
fn u(s: &str) -> JsString {
    s.encode_utf16().collect()
}
fn bits(x: f64) -> String {
    if x.is_nan() {
        "7ff8000000000000".into()
    } else {
        format!("{:016x}", x.to_bits())
    }
}
fn strings(v: &Value) -> Vec<JsString> {
    serde_json::from_value(v.clone()).unwrap()
}
fn params(v: &Value) -> Option<Vec<Option<JsString>>> {
    serde_json::from_value(v.clone()).unwrap()
}
fn error(result: Result<(), Error>, failure: &Error, expected: bool, row: &Value, label: &str) {
    assert_eq!(result.is_err(), expected, "{label} native error");
    assert_eq!(
        row["sameError"], expected,
        "{label} independent source error"
    );
    if let Err(e) = result {
        assert!(Rc::ptr_eq(&e, failure), "{label} native error identity");
    }
}
fn fresh_native<U, H>(s: &OrderBookState<U, H>, label: &str) {
    assert!(s.updates.is_empty(), "{label} native empty updates");
    assert!(!s.is_snapshot_loaded, "{label} native snapshot flag");
    assert!(!s.is_terminal, "{label} native terminal flag");
    assert_eq!(s.last_update_id.to_bits(), 0, "{label} native last ID");
    assert!(!s.is_snapshot_fetch_in_flight, "{label} native fetch flag");
    assert!(!s.is_recovery_scheduled, "{label} native schedule flag");
    assert_eq!(
        s.recovery_attempts.to_bits(),
        0,
        "{label} native recovery count"
    );
    assert!(!s.terminal_error_emitted, "{label} native error flag");
    assert!(
        s.terminal_handlers.borrow().is_empty(),
        "{label} native empty handlers"
    );
}
fn native_state(s: &OrderBookState<i32, String>, v: &Value, label: &str) {
    assert_eq!(
        s.updates,
        serde_json::from_value::<Vec<i32>>(v["updates"].clone()).unwrap(),
        "{label} native updates"
    );
    assert_eq!(
        s.is_snapshot_loaded,
        v["loaded"].as_bool().unwrap(),
        "{label} native loaded"
    );
    assert_eq!(
        s.is_terminal,
        v["terminal"].as_bool().unwrap(),
        "{label} native terminal"
    );
    assert_eq!(bits(s.last_update_id), v["last"], "{label} native last");
    assert_eq!(
        s.is_snapshot_fetch_in_flight,
        v["inFlight"].as_bool().unwrap()
    );
    assert_eq!(s.is_recovery_scheduled, v["scheduled"].as_bool().unwrap());
    assert_eq!(bits(s.recovery_attempts), v["attempts"]);
    assert_eq!(s.terminal_error_emitted, v["emitted"].as_bool().unwrap());
    let handlers: Vec<_> = s
        .terminal_handlers
        .borrow()
        .iter()
        .map(|h| h.as_str().to_owned())
        .collect();
    assert_eq!(json!(handlers), v["handlers"], "{label} native handlers");
}
#[test]
fn fresh_orderbook_state() {
    let rows = oracle()["created"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "STATE independent size");
    let mut a = create_order_book_state::<i32, String>();
    let b = create_order_book_state::<i32, String>();
    fresh_native(&a, "STATE 0");
    fresh_native(&b, "STATE 1");
    native_state(&a, &rows[0], "STATE 0");
    native_state(&b, &rows[1], "STATE 1");
    assert!(
        !Rc::ptr_eq(&a.terminal_handlers, &b.terminal_handlers),
        "STATE native fresh Set identity"
    );
    a.updates.push(9);
    a.terminal_handlers
        .borrow_mut()
        .insert(Rc::new("changed".into()));
    fresh_native(&b, "STATE other");
    native_state(&b, &oracle()["fresh"]["other"], "STATE other");
    assert_eq!(oracle()["fresh"]["updates"], true);
    assert_eq!(oracle()["fresh"]["handlers"], true);
}
fn sequence(bridge: bool) {
    let rows = oracle()["sequence"].as_array().unwrap();
    assert_eq!(rows.len(), 891, "SEQUENCE independent size");
    let states = [
        -0.0,
        0.0,
        1.0,
        5.0,
        -1.0,
        2.5,
        9007199254740991.0,
        9007199254740992.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    let ids = [
        None,
        Some(f64::NEG_INFINITY),
        Some(-0.0),
        Some(1.0),
        Some(2.0),
        Some(6.0),
        Some(9007199254740992.0),
        Some(f64::INFINITY),
        Some(f64::NAN),
    ];
    for (i, row) in rows.iter().enumerate() {
        let last = states[i / 81];
        let first = ids[(i % 81) / 9];
        let end = ids[i % 9];
        assert_eq!(row["state"], bits(last), "SEQUENCE independent state {i}");
        assert_eq!(
            row["first"],
            json!(first.map(bits)),
            "SEQUENCE independent first {i}"
        );
        assert_eq!(
            row["last"],
            json!(end.map(bits)),
            "SEQUENCE independent last {i}"
        );
        let mut state = create_order_book_state::<(), ()>();
        state.last_update_id = last;
        let update = SequenceUpdate { first, last: end };
        if bridge {
            assert_eq!(
                is_initial_order_book_bridge(&state, update),
                row["bridge"].as_bool().unwrap(),
                "BRIDGE native value {i}"
            );
        } else {
            assert_eq!(
                is_continuous_order_book_update(&state, update),
                row["continuous"].as_bool().unwrap(),
                "CONTINUOUS native value {i}"
            );
        }
    }
}
#[test]
fn initial_sequence_bridge() {
    sequence(true);
}
#[test]
fn continuous_sequence_update() {
    sequence(false);
}
fn samples() -> Vec<[JsString; 2]> {
    vec![
        [u("1"), u("2")],
        [u("-0"), u("Infinity")],
        [u("2.5e1x"), u("3x")],
        [u("bad"), u("-Infinity")],
        [vec![49, 0xd800], vec![0xd800, 50]],
        [u("\u{feff}4.5"), u("5")],
        [u("1e309"), u("-1e309")],
        [u(".25"), u("-0")],
    ]
}
#[test]
fn apply_live_orderbook_levels() {
    let rows = oracle()["apply"].as_array().unwrap();
    assert_eq!(rows.len(), 16, "APPLY independent size");
    for (i, row) in rows.iter().enumerate() {
        let input = samples()[i / 2].clone();
        let mode = (i / 2) % 4;
        let fail = i % 2 == 1;
        assert_eq!(strings(&row["input"]), input);
        assert_eq!(row["mode"], mode);
        assert_eq!(row["fail"], fail);
        let bids = Rc::new(RefCell::new(vec![input, [u("9"), u("10")]]));
        let data = Rc::new(RefCell::new(DepthUpdate {
            bids: Some(bids.clone()),
            asks: Some(Rc::new(RefCell::new(vec![[u("11"), u("12")]]))),
        }));
        let book = Rc::new(());
        let failure = Rc::new(());
        let mut calls = vec![];
        let result = apply_order_book_update(&book, &data, |actual, side, level| {
            let index = calls.len();
            let expected = &row["calls"][index];
            assert!(
                Rc::ptr_eq(actual, &book),
                "APPLY native book identity {i}/{index}"
            );
            assert_eq!(
                side,
                if expected["side"] == "bids" {
                    BookSide::Bids
                } else {
                    BookSide::Asks
                },
                "APPLY native side {i}/{index}"
            );
            assert_eq!(
                bits(level.price),
                expected["price"],
                "APPLY native price {i}/{index}"
            );
            assert_eq!(
                bits(level.amount),
                expected["amount"],
                "APPLY native amount {i}/{index}"
            );
            assert_eq!(level.total.to_bits(), 0, "APPLY native total {i}/{index}");
            assert_eq!(expected["total"], "0000000000000000");
            assert_eq!(expected["sameBook"], true);
            calls.push(side);
            if index == 0 {
                match mode {
                    1 => bids.borrow_mut().push([u("13"), u("14")]),
                    2 => bids.borrow_mut().clear(),
                    3 => {
                        data.borrow_mut().asks =
                            Some(Rc::new(RefCell::new(vec![[u("15"), u("16")]])))
                    }
                    _ => {}
                }
                if fail {
                    return Err(failure.clone());
                }
            }
            Ok(())
        });
        let count = if fail { 1 } else { [3, 4, 2, 3][mode] };
        assert_eq!(calls.len(), count, "APPLY native count {i}");
        assert_eq!(
            row["calls"].as_array().unwrap().len(),
            count,
            "APPLY independent source count {i}"
        );
        error(result, &failure, fail, row, &format!("APPLY {i}"));
    }
}

struct ResetHost {
    fail: &'static str,
    mutate: bool,
    failure: Error,
    calls: Vec<String>,
    states: BTreeMap<String, StateRef<i32, String>>,
    originals: BTreeMap<String, StateRef<i32, String>>,
    handlers: BTreeMap<String, TerminalHandlers<String>>,
    created: Vec<StateRef<i32, String>>,
    fetchers: BTreeMap<String, Rc<String>>,
}
impl ResetOrderBooks<i32, String, Rc<String>, Error> for ResetHost {
    fn state_keys(&mut self) -> Result<Vec<JsString>, Error> {
        Ok(["1", "2", "b"].map(u).to_vec())
    }
    fn create_state(&mut self) -> Result<StateRef<i32, String>, Error> {
        self.calls.push("create".into());
        if self.fail == "create" {
            return Err(self.failure.clone());
        }
        let s = Rc::new(RefCell::new(create_order_book_state()));
        fresh_native(&s.borrow(), "RESET create");
        self.created.push(s.clone());
        Ok(s)
    }
    fn terminal_handlers(&mut self, key: &[u16]) -> Result<TerminalHandlers<String>, Error> {
        let k = String::from_utf16(key).unwrap();
        self.calls.push(format!("handlers:{k}"));
        if self.fail == "handlers" && k == "2" {
            return Err(self.failure.clone());
        }
        Ok(self.handlers[&k].clone())
    }
    fn replace_state(&mut self, key: &[u16], state: StateRef<i32, String>) -> Result<(), Error> {
        let k = String::from_utf16(key).unwrap();
        self.calls.push(format!("set:{k}"));
        assert!(
            Rc::ptr_eq(&state.terminal_handlers(), &self.handlers[&k]),
            "RESET native retained handler Set"
        );
        assert!(
            self.created.iter().any(|s| Rc::ptr_eq(s, &state)),
            "RESET native created object identity"
        );
        if self.fail == "replace" && k == "2" {
            return Err(self.failure.clone());
        }
        self.states.insert(k, state);
        Ok(())
    }
    fn fetcher_keys(&mut self) -> Result<Vec<JsString>, Error> {
        Ok(["1", "2", "b"].map(u).to_vec())
    }
    fn snapshot_fetcher(&mut self, key: &[u16]) -> Result<Option<Rc<String>>, Error> {
        let k = String::from_utf16(key).unwrap();
        if let Some(f) = self.fetchers.get(&k) {
            self.calls.push(format!("lookup:{k}"));
            if self.fail == "lookup" && k == "2" {
                return Err(self.failure.clone());
            }
            Ok(Some(f.clone()))
        } else {
            Ok(None)
        }
    }
    fn fetch_snapshot(&mut self, f: Rc<String>) -> Result<(), Error> {
        assert!(
            Rc::ptr_eq(&f, &self.fetchers[f.as_str()]),
            "RESET native fetcher identity"
        );
        self.calls.push(format!("fetch:{f}"));
        if self.mutate && *f == "1" {
            self.fetchers.remove("2");
        }
        if self.fail == "fetch" && *f == "1" {
            return Err(self.failure.clone());
        }
        Ok(())
    }
}
trait StateHandlers {
    fn terminal_handlers(&self) -> TerminalHandlers<String>;
}
impl StateHandlers for StateRef<i32, String> {
    fn terminal_handlers(&self) -> TerminalHandlers<String> {
        self.borrow().terminal_handlers.clone()
    }
}
fn reset_calls(fail: &str, mutate: bool) -> Vec<String> {
    let mut trace = vec![];
    for k in ["1", "2", "b"] {
        trace.push("create".into());
        if fail == "create" {
            return trace;
        }
        trace.push(format!("handlers:{k}"));
        if fail == "handlers" && k == "2" {
            return trace;
        }
        trace.push(format!("set:{k}"));
        if fail == "replace" && k == "2" {
            return trace;
        }
    }
    for k in ["1", "2", "b"] {
        if mutate && k == "2" {
            continue;
        }
        trace.push(format!("lookup:{k}"));
        if fail == "lookup" && k == "2" {
            return trace;
        }
        trace.push(format!("fetch:{k}"));
        if fail == "fetch" && k == "1" {
            return trace;
        }
    }
    trace
}
#[test]
fn reset_preserves_state_and_fetcher_identity() {
    let rows = oracle()["reset"].as_array().unwrap();
    assert_eq!(rows.len(), 12, "RESET independent size");
    for (i, row) in rows.iter().enumerate() {
        let fail = ["none", "create", "handlers", "replace", "lookup", "fetch"][i / 2];
        let mutate = i % 2 == 1;
        assert_eq!(row["fail"], fail);
        assert_eq!(row["mutate"], mutate);
        let mut host = ResetHost {
            fail,
            mutate,
            failure: Rc::new(()),
            calls: vec![],
            states: BTreeMap::new(),
            originals: BTreeMap::new(),
            handlers: BTreeMap::new(),
            created: vec![],
            fetchers: BTreeMap::new(),
        };
        for k in ["b", "2", "1"] {
            let mut s = create_order_book_state();
            s.updates = vec![4];
            s.is_snapshot_loaded = true;
            s.is_terminal = true;
            s.last_update_id = 88.0;
            s.is_snapshot_fetch_in_flight = true;
            s.is_recovery_scheduled = true;
            s.recovery_attempts = 2.0;
            s.terminal_error_emitted = true;
            s.terminal_handlers.borrow_mut().insert(Rc::new(k.into()));
            let s = Rc::new(RefCell::new(s));
            host.handlers.insert(k.into(), s.terminal_handlers());
            host.originals.insert(k.into(), s.clone());
            host.states.insert(k.into(), s);
            host.fetchers.insert(k.into(), Rc::new(k.into()));
        }
        let result = reset_and_refetch_order_books(&mut host);
        let expected = reset_calls(fail, mutate);
        assert_eq!(host.calls, expected, "RESET native operation order {i}");
        assert_eq!(row["calls"], json!(expected));
        assert_eq!(
            row["final"].as_array().unwrap().len(),
            3,
            "RESET independent states"
        );
        for (j, k) in ["1", "2", "b"].into_iter().enumerate() {
            let v = &row["final"][j];
            assert_eq!(v["key"], k);
            let actual = &host.states[k];
            let fresh = match fail {
                "create" => false,
                "handlers" | "replace" => k == "1",
                _ => true,
            };
            assert_eq!(
                !Rc::ptr_eq(actual, &host.originals[k]),
                fresh,
                "RESET native replacement {i}/{k}"
            );
            assert_eq!(v["fresh"], fresh);
            assert_eq!(v["sameCreated"], fresh);
            assert!(
                Rc::ptr_eq(&actual.terminal_handlers(), &host.handlers[k]),
                "RESET native final Set identity {i}/{k}"
            );
            assert_eq!(v["sameHandlers"], true);
            native_state(&actual.borrow(), &v["value"], &format!("RESET {i}/{k}"));
        }
        let expected_error = fail != "none" && !(fail == "lookup" && mutate);
        error(
            result,
            &host.failure,
            expected_error,
            row,
            &format!("RESET {i}"),
        );
    }
}

struct DisposeHost {
    fail: &'static str,
    failure: Error,
    missing: Error,
    calls: Vec<String>,
    subscriptions: Vec<String>,
    states: Rc<Vec<i32>>,
    fetchers: Rc<Vec<i32>>,
    throttles: Rc<RefCell<Vec<Rc<String>>>>,
    socket: Option<Rc<()>>,
    second_socket: Rc<()>,
    reads: usize,
}
impl DisposePublicSocket<Rc<String>, Rc<()>, Error> for DisposeHost {
    fn clear_subscriptions(&mut self) -> Result<(), Error> {
        self.calls.push("clearSub".into());
        if self.fail == "clearSub" {
            return Err(self.failure.clone());
        }
        self.subscriptions.clear();
        Ok(())
    }
    fn clear_order_book_state(&mut self) -> Result<(), Error> {
        self.calls.push("clearState".into());
        if self.fail == "clearState" {
            return Err(self.failure.clone());
        }
        self.states = Rc::new(vec![]);
        Ok(())
    }
    fn clear_snapshot_fetchers(&mut self) -> Result<(), Error> {
        self.calls.push("clearFetch".into());
        if self.fail == "clearFetch" {
            return Err(self.failure.clone());
        }
        self.fetchers = Rc::new(vec![]);
        Ok(())
    }
    fn throttle_values(&mut self) -> Result<Vec<Rc<String>>, Error> {
        Ok(self.throttles.borrow().clone())
    }
    fn cancel_throttle(&mut self, t: Rc<String>) -> Result<(), Error> {
        self.calls.push(format!("cancel:{t}"));
        if *t == "1" {
            self.throttles.borrow_mut().retain(|x| x.as_str() != "2");
            self.throttles.borrow_mut().push(Rc::new("new".into()));
        }
        if self.fail == format!("cancel:{t}") {
            return Err(self.failure.clone());
        }
        Ok(())
    }
    fn clear_depth_throttles(&mut self) -> Result<(), Error> {
        self.calls.push("clearThrottle".into());
        if self.fail == "clearThrottle" {
            return Err(self.failure.clone());
        }
        self.throttles = Rc::new(RefCell::new(vec![]));
        Ok(())
    }
    fn socket(&mut self) -> Result<Option<Rc<()>>, Error> {
        self.calls.push("socket".into());
        self.reads += 1;
        if self.fail == "secondRead" && self.reads == 2 {
            return Err(self.failure.clone());
        }
        if self.fail == "missingSocket" && self.reads >= 2 {
            return Ok(None);
        }
        if self.fail == "alternateSocket" && self.reads >= 2 {
            return Ok(Some(self.second_socket.clone()));
        }
        Ok(self.socket.clone())
    }
    fn close_socket(&mut self, s: Option<Rc<()>>) -> Result<(), Error> {
        let Some(s) = s else {
            return Err(self.missing.clone());
        };
        let expected = if self.fail == "alternateSocket" {
            &self.second_socket
        } else {
            self.socket.as_ref().unwrap()
        };
        assert!(
            Rc::ptr_eq(&s, expected),
            "DISPOSE native second socket identity"
        );
        self.calls.push(
            if self.fail == "alternateSocket" {
                "close:second"
            } else {
                "close"
            }
            .into(),
        );
        if self.fail == "close" {
            return Err(self.failure.clone());
        }
        Ok(())
    }
}
#[test]
fn dispose_partial_state_and_snapshot() {
    let rows = oracle()["disposal"].as_array().unwrap();
    assert_eq!(rows.len(), 12, "DISPOSE independent size");
    for (i, row) in rows.iter().enumerate() {
        let fail = [
            "none",
            "clearSub",
            "clearState",
            "clearFetch",
            "cancel:1",
            "cancel:2",
            "clearThrottle",
            "close",
            "noSocket",
            "alternateSocket",
            "secondRead",
            "missingSocket",
        ][i];
        assert_eq!(row["fail"], fail);
        let mut host = DisposeHost {
            fail,
            failure: Rc::new(()),
            missing: Rc::new(()),
            calls: vec![],
            subscriptions: vec!["x".into()],
            states: Rc::new(vec![1]),
            fetchers: Rc::new(vec![2]),
            throttles: Rc::new(RefCell::new(
                ["1", "2", "b"].map(|k| Rc::new(k.into())).to_vec(),
            )),
            socket: if fail == "noSocket" {
                None
            } else {
                Some(Rc::new(()))
            },
            second_socket: Rc::new(()),
            reads: 0,
        };
        let old_state = host.states.clone();
        let old_fetch = host.fetchers.clone();
        let old_throttles = host.throttles.clone();
        let old_socket = host.socket.clone();
        let result = dispose(&mut host);
        let prefix = [
            "clearSub",
            "clearState",
            "clearFetch",
            "cancel:1",
            "cancel:2",
            "cancel:b",
            "clearThrottle",
        ];
        let mut expected = vec![];
        for name in prefix {
            expected.push(name);
            if fail == name {
                break;
            }
        }
        if expected.len() == 7 && fail != "clearThrottle" {
            expected.push("socket");
            if fail != "noSocket" {
                expected.push("socket");
                if !["secondRead", "missingSocket"].contains(&fail) {
                    expected.push(if fail == "alternateSocket" {
                        "close:second"
                    } else {
                        "close"
                    });
                }
            }
        }
        let length = expected.len();
        assert_eq!(host.calls, expected, "DISPOSE native operation order {i}");
        assert_eq!(row["calls"], json!(expected));
        if fail == "missingSocket" {
            let e = result.expect_err("missing socket must reject close access");
            assert!(
                Rc::ptr_eq(&e, &host.missing),
                "DISPOSE native missing-property error"
            );
            assert_eq!(row["sameError"], false);
            assert_eq!(row["missingError"], true);
        } else {
            error(
                result,
                &host.failure,
                !["none", "noSocket", "alternateSocket"].contains(&fail),
                row,
                &format!("DISPOSE {i}"),
            );
            assert_eq!(row["missingError"], false);
        }
        assert_eq!(
            host.subscriptions.is_empty(),
            fail != "clearSub",
            "DISPOSE native subscriptions {i}"
        );
        assert_eq!(row["subscriptions"], json!(host.subscriptions));
        assert_eq!(row["stateReplaced"], !Rc::ptr_eq(&old_state, &host.states));
        assert_eq!(
            row["fetchReplaced"],
            !Rc::ptr_eq(&old_fetch, &host.fetchers)
        );
        assert_eq!(
            row["throttlesReplaced"],
            !Rc::ptr_eq(&old_throttles, &host.throttles)
        );
        assert_eq!(*old_state, vec![1], "DISPOSE native old state alias {i}");
        assert_eq!(*old_fetch, vec![2]);
        assert_eq!(row["oldStateKeys"], json!(["a"]));
        assert_eq!(row["oldFetchKeys"], json!(["a"]));
        let keys: Vec<_> = host
            .throttles
            .borrow()
            .iter()
            .map(|x| x.as_str().to_string())
            .collect();
        let source_order = if Rc::ptr_eq(&old_throttles, &host.throttles) && length >= 4 {
            vec!["1", "b", "new"]
        } else if length < 4 {
            vec!["1", "2", "b"]
        } else {
            vec![]
        };
        assert_eq!(keys, source_order, "DISPOSE native throttle contents {i}");
        assert_eq!(row["throttleKeys"], json!(source_order));
        if let Some(original) = &old_socket {
            assert!(
                Rc::ptr_eq(host.socket.as_ref().unwrap(), original),
                "DISPOSE native no socket assignment {i}"
            );
        }
        let observed = host.socket().unwrap();
        let same = match (&observed, &old_socket) {
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            _ => false,
        };
        assert_eq!(
            same,
            ![8, 9, 11].contains(&i),
            "DISPOSE native observed getter {i}"
        );
        assert_eq!(row["sameWs"], same);
    }
}

fn subscriptions(values: &[JsString]) -> Rc<RefCell<Subscriptions>> {
    let s = Rc::new(RefCell::new(Subscriptions::default()));
    for v in values {
        s.borrow_mut().insert(v.clone());
    }
    s
}
struct Hold {
    open: Rc<Cell<bool>>,
    rejected: bool,
    failure: Error,
}
impl Future for Hold {
    type Output = Result<(), Error>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.open.get() {
            Poll::Pending
        } else {
            Poll::Ready(if self.rejected {
                Err(self.failure.clone())
            } else {
                Ok(())
            })
        }
    }
}
fn poll<F: Future>(future: Pin<&mut F>) -> Poll<F::Output> {
    future.poll(&mut Context::from_waker(Waker::noop()))
}
#[test]
fn private_resubscribe_eager_and_pending() {
    let rows = oracle()["resubscribe"].as_array().unwrap();
    assert_eq!(rows.len(), 8, "PRIVATE-RESUB independent size");
    for (i, row) in rows.iter().enumerate() {
        let nonempty = i / 4 == 1;
        let kind = i % 4;
        assert_eq!(row["nonempty"], nonempty);
        assert_eq!(row["kind"], kind);
        let state = subscriptions(&if nonempty {
            vec![u("account.orderUpdate")]
        } else {
            vec![]
        });
        let failure = Rc::new(());
        let calls = RefCell::new(vec![]);
        let open = Rc::new(Cell::new(kind != 3));
        let future = private::resubscribe(&state, || {
            calls.borrow_mut().push("auth");
            if kind == 1 {
                Err(failure.clone())
            } else {
                Ok(Hold {
                    open: open.clone(),
                    rejected: kind == 2,
                    failure: failure.clone(),
                })
            }
        });
        let expected = if nonempty { vec!["auth"] } else { vec![] };
        assert_eq!(
            *calls.borrow(),
            expected,
            "PRIVATE-RESUB native eager calls {i}"
        );
        assert_eq!(row["eager"], json!(expected));
        let subscribed = state.borrow().values().to_vec();
        for v in subscribed {
            state.borrow_mut().remove(&v);
        }
        let mut future = std::pin::pin!(future);
        let first = poll(future.as_mut());
        let pending = nonempty && kind == 3;
        assert_eq!(
            first.is_pending(),
            pending,
            "PRIVATE-RESUB native pending {i}"
        );
        assert_eq!(row["pending"], pending);
        let result = if pending {
            open.set(true);
            match poll(future.as_mut()) {
                Poll::Ready(r) => r,
                Poll::Pending => panic!("released auth must finish"),
            }
        } else {
            match first {
                Poll::Ready(r) => r,
                Poll::Pending => unreachable!(),
            }
        };
        assert_eq!(*calls.borrow(), expected);
        assert_eq!(row["calls"], json!(expected));
        error(
            result,
            &failure,
            nonempty && [1, 2].contains(&kind),
            row,
            &format!("PRIVATE-RESUB {i}"),
        );
    }
}
#[derive(Clone)]
enum Handle {
    Number(f64),
    Object,
}
fn handle_label(h: Option<&Rc<Handle>>) -> String {
    match h.map(|v| v.as_ref()) {
        None => "absent".into(),
        Some(Handle::Object) => "object".into(),
        Some(Handle::Number(x)) => bits(*x),
    }
}
#[test]
fn private_order_timer_truthiness_and_identity() {
    let rows = oracle()["timers"].as_array().unwrap();
    assert_eq!(rows.len(), 18, "TIMER independent size");
    let handles = [
        None,
        Some(Handle::Number(0.0)),
        Some(Handle::Number(-0.0)),
        Some(Handle::Number(f64::NAN)),
        Some(Handle::Number(f64::INFINITY)),
        Some(Handle::Number(f64::NEG_INFINITY)),
        Some(Handle::Number(1.0)),
        Some(Handle::Number(-1.0)),
        Some(Handle::Object),
    ];
    for (i, row) in rows.iter().enumerate() {
        let input = i / 2;
        let fail = i % 2 == 1;
        assert_eq!(row["input"], input);
        assert_eq!(row["fail"], fail);
        let original = handles[input].clone().map(Rc::new);
        let timeout = RefCell::new(original.clone());
        let callback = Rc::new(());
        let new = Rc::new(Handle::Number(55.0));
        let failure = Rc::new(());
        let calls = Cell::new(0);
        let result = private::throttled_fetch_orders(
            &timeout,
            |h| {
                Ok(match h.map(|v| v.as_ref()) {
                    None => false,
                    Some(Handle::Object) => true,
                    Some(Handle::Number(x)) => *x != 0.0 && !x.is_nan(),
                })
            },
            &callback,
            |c, delay| {
                calls.set(calls.get() + 1);
                assert!(
                    Rc::ptr_eq(c, &callback),
                    "TIMER native callback identity {i}"
                );
                assert_eq!(
                    delay.to_bits(),
                    10000.0_f64.to_bits(),
                    "TIMER native delay {i}"
                );
                assert_eq!(row["calls"][0]["delay"], bits(delay));
                assert_eq!(row["calls"][0]["sameCallback"], true);
                assert_eq!(
                    handle_label(timeout.borrow().as_ref()),
                    handle_label(original.as_ref()),
                    "TIMER native assignment after scheduling {i}"
                );
                assert_eq!(row["calls"][0]["before"], handle_label(original.as_ref()));
                if fail {
                    Err(failure.clone())
                } else {
                    Ok(new.clone())
                }
            },
        );
        let scheduled = input < 4;
        let count = usize::from(scheduled);
        assert_eq!(calls.get(), count, "TIMER native schedule count {i}");
        assert_eq!(row["calls"].as_array().unwrap().len(), count);
        let changed = scheduled && !fail;
        assert_eq!(row["after"], handle_label(timeout.borrow().as_ref()));
        assert_eq!(row["sameHandle"], !changed);
        if changed {
            assert!(
                Rc::ptr_eq(timeout.borrow().as_ref().unwrap(), &new),
                "TIMER native returned handle {i}"
            );
        } else {
            match (&*timeout.borrow(), &original) {
                (None, None) => {}
                (Some(a), Some(b)) => {
                    assert!(Rc::ptr_eq(a, b), "TIMER native preserved handle {i}")
                }
                _ => panic!("timer handle presence changed"),
            }
        }
        error(
            result,
            &failure,
            scheduled && fail,
            row,
            &format!("TIMER {i}"),
        );
    }
}
fn native_message(
    m: &Message,
    p: &Value,
    method: &str,
    expected: Option<Vec<Option<JsString>>>,
    label: &str,
) {
    assert_eq!(m.method, method, "{label} native method");
    assert_eq!(m.params, expected, "{label} native params");
    assert_eq!(p["method"], method);
    assert_eq!(params(&p["params"]), m.params);
}
fn wire_checks(wires: Vec<(JsString, JsString)>) {
    for (i, (a, b)) in wires.into_iter().enumerate() {
        assert_eq!(a, b, "wire {i} after all native assertions");
    }
}
#[test]
fn private_ping_guards_and_time() {
    let rows = oracle()["ping"].as_array().unwrap();
    assert_eq!(rows.len(), 12, "PRIVATE-PING independent size");
    let mut cases = vec![];
    for now in [0.0, -0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 42.0] {
        cases.push((false, true, "none", now));
    }
    cases.extend([
        (true, true, "connected", 7.0),
        (false, false, "none", 7.0),
        (false, true, "connected", 7.0),
        (false, true, "now", 7.0),
        (false, true, "send", 7.0),
        (true, false, "none", 7.0),
    ]);
    let mut wires = vec![];
    for (i, (disposed, connected, fail, now)) in cases.into_iter().enumerate() {
        let row = &rows[i];
        assert_eq!(row["disposed"], disposed);
        assert_eq!(row["connected"], connected);
        assert_eq!(row["fail"], fail);
        assert_eq!(row["now"], bits(now));
        let at = Cell::new(99.0);
        let failure = Rc::new(());
        let calls = RefCell::new(vec![]);
        let label = format!("PRIVATE-PING {i}");
        let result = private::ping(
            disposed,
            || {
                calls.borrow_mut().push("connected");
                if fail == "connected" {
                    Err(failure.clone())
                } else {
                    Ok(connected)
                }
            },
            &at,
            || {
                calls.borrow_mut().push("now");
                if fail == "now" {
                    Err(failure.clone())
                } else {
                    Ok(now)
                }
            },
            |m| {
                calls.borrow_mut().push("send");
                assert_eq!(
                    bits(at.get()),
                    bits(now),
                    "{label} native timestamp before send"
                );
                native_message(&m, &row["packets"][0], "PING", None, &label);
                wires.push((
                    m.to_json(),
                    serde_json::from_value(row["wires"][0].clone()).unwrap(),
                ));
                if fail == "send" {
                    Err(failure.clone())
                } else {
                    Ok(())
                }
            },
        );
        let mut expected = vec![];
        if !disposed {
            expected.push("connected");
            if connected && fail != "connected" {
                expected.push("now");
                if fail != "now" {
                    expected.push("send");
                }
            }
        }
        assert_eq!(*calls.borrow(), expected, "{label} native call order");
        assert_eq!(row["calls"], json!(expected));
        let count = usize::from(expected.contains(&"send"));
        assert_eq!(row["packets"].as_array().unwrap().len(), count);
        assert_eq!(row["wires"].as_array().unwrap().len(), count);
        let expected_at = if count == 1 { now } else { 99.0 };
        assert_eq!(
            bits(at.get()),
            bits(expected_at),
            "{label} native timestamp"
        );
        assert_eq!(row["pingAt"], bits(expected_at));
        let rejected =
            !disposed && (fail == "connected" || (connected && ["now", "send"].contains(&fail)));
        error(result, &failure, rejected, row, &label);
    }
    wire_checks(wires);
}
#[test]
fn private_unsubscribe_live_streams() {
    let rows = oracle()["unsubscribe"].as_array().unwrap();
    assert_eq!(rows.len(), 14, "PRIVATE-UNSUB independent size");
    let cases = [
        vec![],
        vec![Some(u("a"))],
        vec![Some(u("a")), Some(u("b")), Some(u("a"))],
        vec![Some(vec![0xd800]), None, Some(u("c"))],
        vec![Some(u("a"))],
        vec![Some(u("a"))],
        vec![Some(u("a"))],
    ];
    let removed = [
        vec![],
        vec![u("a")],
        vec![u("a"), u("b")],
        vec![vec![0xd800], u("c")],
        vec![u("a"), u("b")],
        vec![],
        vec![u("c")],
    ];
    let mut wires = vec![];
    for (i, row) in rows.iter().enumerate() {
        let mode = i / 2;
        let fail = i % 2 == 1;
        let input = &cases[mode];
        assert_eq!(row["mode"], mode);
        assert_eq!(row["fail"], fail);
        assert_eq!(params(&row["input"]), Some(input.clone()));
        let initial = vec![u("a"), u("b"), u("c"), vec![0xd800]];
        let state = subscriptions(&initial);
        let streams = Rc::new(RefCell::new(input.clone()));
        let failure = Rc::new(());
        let calls = Cell::new(0);
        let label = format!("PRIVATE-UNSUB {i}");
        let result = private::unsubscribe(&state, &streams, |m| {
            calls.set(calls.get() + 1);
            assert_eq!(
                state.borrow().values(),
                initial,
                "{label} native send before removal"
            );
            assert_eq!(strings(&row["before"][0]), initial);
            native_message(
                &m,
                &row["packets"][0],
                "UNSUBSCRIBE",
                Some(input.clone()),
                &label,
            );
            wires.push((
                m.to_json(),
                serde_json::from_value(row["wires"][0].clone()).unwrap(),
            ));
            match mode {
                4 => streams.borrow_mut().push(Some(u("b"))),
                5 => streams.borrow_mut().clear(),
                6 => streams.borrow_mut()[0] = Some(u("c")),
                _ => {}
            }
            if fail { Err(failure.clone()) } else { Ok(()) }
        });
        let count = usize::from(!input.is_empty());
        assert_eq!(calls.get(), count, "{label} native send count");
        for field in ["packets", "wires", "before"] {
            assert_eq!(row[field].as_array().unwrap().len(), count);
        }
        let after: Vec<_> = initial
            .into_iter()
            .filter(|v| fail || !removed[mode].contains(v))
            .collect();
        assert_eq!(state.borrow().values(), after, "{label} native final state");
        assert_eq!(strings(&row["after"]), after);
        error(result, &failure, fail && count == 1, row, &label);
    }
    wire_checks(wires);
}
