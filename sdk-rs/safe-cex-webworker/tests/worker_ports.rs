use indexmap::IndexMap;
use safe_cex_exchange::delivery_timers::WorkerPortTimers;
use safe_cex_webworker::worker_ports::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Profile {
    posts: Vec<(String, String)>,
    timers: Vec<(String, u64, f64)>,
}
#[derive(Debug, PartialEq, Deserialize)]
struct TruthRow {
    label: String,
    delivered: bool,
    pending: bool,
    last: f64,
}
#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Routing {
    broadcast: Profile,
    leading_failure: Profile,
    trailing_failure: Profile,
    reentrant: Profile,
}
fn decoded<T: serde::de::DeserializeOwned>(source: &Value) -> T {
    serde_json::from_value(source.clone()).unwrap()
}
#[derive(Debug)]
enum Kind {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    Text(Vec<u16>),
    Object,
    Array,
}
#[derive(Debug)]
struct Message {
    label: &'static str,
    kind: Kind,
}
impl WorkerMessage for Message {
    fn is_truthy(&self) -> bool {
        match &self.kind {
            Kind::Undefined | Kind::Null => false,
            Kind::Bool(b) => *b,
            Kind::Number(n) => *n != 0.0 && !n.is_nan(),
            Kind::Text(s) => !s.is_empty(),
            Kind::Object | Kind::Array => true,
        }
    }
}
fn msg(label: &'static str) -> Rc<Message> {
    Rc::new(Message {
        label,
        kind: Kind::Object,
    })
}
type Hook = Rc<dyn Fn(&Rc<Message>)>;
struct Port {
    name: &'static str,
    posts: Rc<RefCell<Vec<(&'static str, Rc<Message>)>>>,
    fail: Cell<bool>,
    hook: RefCell<Option<Hook>>,
}
impl WorkerPort<Message> for Port {
    type Error = ();
    fn post_message(&self, m: &Rc<Message>) -> Result<(), ()> {
        self.posts.borrow_mut().push((self.name, m.clone()));
        let hook = self.hook.borrow().clone();
        if let Some(hook) = hook {
            hook(m);
        }
        if self.fail.get() { Err(()) } else { Ok(()) }
    }
}
#[derive(Default)]
struct Timers {
    now: Cell<f64>,
    next: Cell<u64>,
    pending: RefCell<IndexMap<u64, Rc<dyn Fn()>>>,
    effects: RefCell<Vec<(String, u64, f64)>>,
}
impl WorkerPortTimers for Timers {
    type Timer = u64;
    fn now_ms(&self) -> f64 {
        self.now.get()
    }
    fn set_timeout(&self, callback: Rc<dyn Fn()>, delay: f64) -> u64 {
        let id = self.next.get();
        self.next.set(id + 1);
        self.pending.borrow_mut().insert(id, callback);
        self.effects.borrow_mut().push(("set".into(), id, delay));
        id
    }
    fn clear_timeout(&self, id: &u64) {
        self.pending.borrow_mut().shift_remove(id);
        self.effects.borrow_mut().push(("clear".into(), *id, 0.0));
    }
    fn timer_is_truthy(&self, id: &u64) -> bool {
        *id != 0
    }
}
impl Timers {
    fn fire(&self, id: u64) {
        let callback = self
            .pending
            .borrow_mut()
            .shift_remove(&id)
            .expect("pending timer");
        callback();
    }
}
type Registry = WorkerPorts<Message, Port, Timers>;
struct Setup {
    r: Rc<Registry>,
    h: Rc<Timers>,
    posts: Rc<RefCell<Vec<(&'static str, Rc<Message>)>>>,
}
impl Setup {
    fn new() -> Self {
        let h = Rc::new(Timers::default());
        h.next.set(1);
        Self {
            r: WorkerPorts::new(h.clone()),
            h,
            posts: Rc::new(RefCell::new(vec![])),
        }
    }
    fn port(&self, name: &'static str) -> Rc<Port> {
        Rc::new(Port {
            name,
            posts: self.posts.clone(),
            fail: Cell::new(false),
            hook: RefCell::new(None),
        })
    }
    fn group(&self, key: &str, ports: Vec<Rc<Port>>) -> PortList<Port> {
        let list = Rc::new(RefCell::new(ports));
        self.r.ports.borrow_mut().insert(key.into(), list.clone());
        list
    }
    fn hidden(&self, port: &Rc<Port>) -> SharedPortState<Message, u64> {
        let state = self.r.ensure_state(port);
        {
            let mut state = state.borrow_mut();
            state.visible = false;
            state.throttle_enabled = true;
        }
        state
    }
    fn profile(&self) -> Profile {
        Profile {
            posts: self
                .posts
                .borrow()
                .iter()
                .map(|(p, m)| (p.to_string(), m.label.to_string()))
                .collect(),
            timers: self.h.effects.borrow().clone(),
        }
    }
    fn assert_post(&self, index: usize, p: &Rc<Port>, m: &Rc<Message>) {
        let posts = self.posts.borrow();
        assert_eq!(posts[index].0, p.name, "PORT native destination {index}");
        assert!(
            Rc::ptr_eq(&posts[index].1, m),
            "PORT native message identity {index}"
        );
    }
}
fn oracle() -> Value {
    let out = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/worker_ports_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn time(s: &SharedPortState<Message, u64>, expected: f64, label: &str) {
    assert_eq!(
        s.borrow().last_delivery_time.to_bits(),
        expected.to_bits(),
        "PORT native timestamp {label}"
    );
}
#[test]
fn config_registration_preserves_duplicates_and_live_callback_identity() {
    let handlers = RefCell::new(vec![]);
    let calls = Rc::new(RefCell::new(vec![]));
    let log = calls.clone();
    let a: WorkerConfigHandler<Cell<i32>> = Rc::new(move |cfg| log.borrow_mut().push(cfg.get()));
    let log = calls.clone();
    let b: WorkerConfigHandler<Cell<i32>> = Rc::new(move |cfg| log.borrow_mut().push(-cfg.get()));
    register_worker_config_handler(&handlers, a.clone());
    register_worker_config_handler(&handlers, a.clone());
    register_worker_config_handler(&handlers, b.clone());
    assert_eq!(
        handlers.borrow().len(),
        3,
        "PORT native duplicate config cardinality"
    );
    for (i, h) in [a.clone(), a.clone(), b.clone()].iter().enumerate() {
        assert!(
            Rc::ptr_eq(&handlers.borrow()[i], h),
            "PORT native callback identity {i}"
        );
    }
    let cfg = Cell::new(17);
    for h in handlers.borrow().iter() {
        h(&cfg);
    }
    cfg.set(42);
    for h in handlers.borrow().iter() {
        h(&cfg);
    }
    assert_eq!(
        *calls.borrow(),
        [17, 17, -17, 42, 42, -42],
        "PORT native config calls"
    );
    let source = oracle();
    assert_eq!(
        source["config"],
        json!({"ids":["a","a","b"],"calls":[17,17,-17,42,42,-42]})
    );
}
#[test]
fn leading_trailing_visibility_and_current_state_lookup() {
    let s = Setup::new();
    let p = s.port("p");
    let a = msg("a");
    s.r.deliver_to_port_with_throttle(p.clone(), a.clone());
    let state = s.r.state(&p).unwrap();
    {
        let st = state.borrow();
        assert!(st.visible);
        assert!(!st.throttle_enabled);
        assert!(st.pending_update.is_none());
        assert!(st.throttle_timer.is_none());
    }
    time(&state, 0.0, "default");
    s.assert_post(0, &p, &a);
    s.hidden(&p);
    s.h.now.set(3000.0);
    let b = msg("b");
    let held = state.clone();
    *p.hook.borrow_mut() = Some(Rc::new(move |_| time(&held, 3000.0, "leading before post")));
    s.r.deliver_to_port_with_throttle(p.clone(), b.clone());
    *p.hook.borrow_mut() = None;
    time(&state, 3000.0, "leading");
    s.assert_post(1, &p, &b);
    s.h.now.set(3100.0);
    let c = msg("c");
    s.r.deliver_to_port_with_throttle(p.clone(), c);
    assert_eq!(state.borrow().throttle_timer, Some(1));
    assert_eq!(
        *s.h.effects.borrow(),
        [("set".into(), 1, 2400.0)],
        "PORT native first tail delay"
    );
    s.h.now.set(3200.0);
    let d = msg("d");
    s.r.deliver_to_port_with_throttle(p.clone(), d.clone());
    assert!(
        Rc::ptr_eq(state.borrow().pending_update.as_ref().unwrap(), &d),
        "PORT native latest pending identity"
    );
    assert_eq!(s.posts.borrow().len(), 2, "PORT native no early tail");
    assert_eq!(s.h.pending.borrow().len(), 1, "PORT native one tail timer");
    let held = state.clone();
    let h = s.h.clone();
    *p.hook.borrow_mut() = Some(Rc::new(move |_| {
        time(&held, 3000.0, "tail before post");
        h.now.set(5501.0);
    }));
    s.h.now.set(5500.0);
    s.h.fire(1);
    *p.hook.borrow_mut() = None;
    s.assert_post(2, &p, &d);
    time(&state, 5501.0, "tail after post");
    assert!(state.borrow().pending_update.is_none());
    assert!(state.borrow().throttle_timer.is_none());
    s.h.now.set(5600.0);
    s.r.deliver_to_port_with_throttle(p.clone(), msg("e"));
    assert_eq!(state.borrow().throttle_timer, Some(2));
    state.borrow_mut().visible = true;
    s.h.now.set(5700.0);
    let f = msg("f");
    s.r.deliver_to_port_with_throttle(p.clone(), f.clone());
    s.assert_post(3, &p, &f);
    time(&state, 5501.0, "visible keeps hidden timestamp");
    assert!(state.borrow().pending_update.is_none());
    assert!(state.borrow().throttle_timer.is_none());
    assert!(!s.h.pending.borrow().contains_key(&2));
    {
        let mut st = state.borrow_mut();
        st.visible = false;
        st.throttle_enabled = false;
    }
    let g = msg("g");
    s.r.deliver_to_port_with_throttle(p.clone(), g.clone());
    s.assert_post(4, &p, &g);
    time(&state, 5501.0, "disabled");
    state.borrow_mut().throttle_enabled = true;
    s.h.now.set(6000.0);
    s.r.deliver_to_port_with_throttle(p.clone(), msg("h"));
    let late = s.h.pending.borrow()[&3].clone();
    s.r.cleanup_port_state(&p);
    assert!(s.r.state(&p).is_none(), "PORT native cleanup deletes state");
    assert_eq!(
        state.borrow().throttle_timer,
        Some(3),
        "PORT native cleanup retains old timer property"
    );
    let replacement = s.hidden(&p);
    assert!(!Rc::ptr_eq(&replacement, &state));
    let j = msg("j");
    replacement.borrow_mut().pending_update = Some(j.clone());
    s.h.now.set(8000.0);
    late();
    s.assert_post(5, &p, &j);
    time(&replacement, 8000.0, "timer rereads replacement");
    time(&state, 5501.0, "old state untouched");
    assert!(replacement.borrow().pending_update.is_none());
    assert!(replacement.borrow().throttle_timer.is_none());
    assert_eq!(
        s.posts.borrow().len(),
        6,
        "PORT native timeline delivery cardinality"
    );
    assert_eq!(
        *s.h.effects.borrow(),
        [
            ("set".into(), 1, 2400.0),
            ("set".into(), 2, 2401.0),
            ("clear".into(), 2, 0.0),
            ("set".into(), 3, 2001.0),
            ("clear".into(), 3, 0.0)
        ],
        "PORT native timeline timer effects"
    );
    let native = s.profile();
    assert_eq!(native, decoded::<Profile>(&oracle()["timeline"]));
}
#[test]
fn falsey_payloads_zero_timer_and_backward_wall_time() {
    let kinds = [
        ("undefined", Kind::Undefined, false),
        ("null", Kind::Null, false),
        ("false", Kind::Bool(false), false),
        ("zero", Kind::Number(0.0), false),
        ("negzero", Kind::Number(-0.0), false),
        ("nan", Kind::Number(f64::NAN), false),
        ("empty", Kind::Text(vec![]), false),
        ("true", Kind::Bool(true), true),
        ("inf", Kind::Number(f64::INFINITY), true),
        ("lone", Kind::Text(vec![0xd800]), true),
        ("object", Kind::Object, true),
        ("array", Kind::Array, true),
    ];
    let mut rows = vec![];
    for (label, kind, truthy) in kinds {
        let s = Setup::new();
        let p = s.port("p");
        let state = s.hidden(&p);
        let m = Rc::new(Message { label, kind });
        s.h.now.set(100.0);
        s.r.deliver_to_port_with_throttle(p.clone(), m.clone());
        assert_eq!(state.borrow().throttle_timer, Some(1));
        assert_eq!(
            s.h.effects.borrow()[0].2,
            2400.0,
            "PORT native first tail delay"
        );
        s.h.now.set(2500.0);
        s.h.fire(1);
        assert_eq!(
            s.posts.borrow().len(),
            usize::from(truthy),
            "PORT native truthiness delivery {label}"
        );
        time(&state, if truthy { 2500.0 } else { 0.0 }, label);
        assert!(state.borrow().throttle_timer.is_none());
        if truthy {
            s.assert_post(0, &p, &m);
            assert!(state.borrow().pending_update.is_none());
        } else {
            assert!(
                Rc::ptr_eq(state.borrow().pending_update.as_ref().unwrap(), &m),
                "PORT native falsey retained {label}"
            );
        }
        rows.push(TruthRow {
            label: label.into(),
            delivered: truthy,
            pending: !truthy,
            last: state.borrow().last_delivery_time,
        });
    }
    assert_eq!(rows.len(), 12, "PORT native fixed truthiness cases");
    let s = Setup::new();
    let p = s.port("p");
    let state = s.hidden(&p);
    s.h.next.set(0);
    s.h.now.set(-100.0);
    s.r.deliver_to_port_with_throttle(p.clone(), msg("a"));
    assert_eq!(state.borrow().throttle_timer, Some(0));
    assert_eq!(
        s.h.effects.borrow()[0].2,
        2600.0,
        "PORT native backward wall delay"
    );
    s.r.deliver_to_port_with_throttle(p.clone(), msg("b"));
    assert_eq!(state.borrow().throttle_timer, Some(1));
    assert_eq!(
        s.h.pending.borrow().len(),
        2,
        "PORT native falsey timer schedules again"
    );
    s.h.now.set(2600.0);
    s.h.fire(0);
    time(&state, 2600.0, "old zero timer clears current timer field");
    assert!(state.borrow().throttle_timer.is_none());
    assert!(s.h.pending.borrow().contains_key(&1));
    s.h.fire(1);
    assert_eq!(
        s.posts.borrow().len(),
        1,
        "PORT native orphan second timer skips empty pending"
    );
    let zero = Setup::new();
    let p = zero.port("p");
    let st = zero.hidden(&p);
    zero.h.next.set(0);
    zero.r.deliver_to_port_with_throttle(p.clone(), msg("z"));
    zero.r.cleanup_port_state(&p);
    assert!(zero.r.state(&p).is_none());
    assert_eq!(st.borrow().throttle_timer, Some(0));
    assert_eq!(
        zero.h.effects.borrow().len(),
        1,
        "PORT native cleanup does not cancel falsey timer"
    );
    let source = oracle();
    assert_eq!(rows, decoded::<Vec<TruthRow>>(&source["truthiness"]));
    assert_eq!(s.profile(), decoded::<Profile>(&source["zeroTimer"]));
    assert_eq!(zero.profile(), decoded::<Profile>(&source["zeroCleanup"]));
}
#[test]
fn broadcast_live_arrays_sub_ids_failure_cleanup_and_reentrant_state_aliases() {
    let s = Setup::new();
    let a = s.port("a");
    let b = s.port("b");
    let c = s.port("c");
    let held = s.group("group", vec![a.clone(), a.clone(), b.clone()]);
    s.r.ports_by_sub_id
        .borrow_mut()
        .insert("cross".into(), c.clone());
    let m = msg("m");
    s.r.broadcast_with_throttle("missing", m.clone(), Some("cross"));
    assert!(
        s.posts.borrow().is_empty(),
        "PORT native missing group precedes sub selection"
    );
    s.r.broadcast_with_throttle("group", m.clone(), Some("cross"));
    s.assert_post(0, &c, &m);
    s.r.broadcast_with_throttle("group", m.clone(), Some("unknown"));
    for (i, p) in [a.clone(), a.clone(), b.clone()].iter().enumerate() {
        s.assert_post(i + 1, p, &m);
    }
    s.r.broadcast_with_throttle("group", m.clone(), Some(""));
    assert_eq!(
        s.posts.borrow().len(),
        7,
        "PORT native falsey sub broadcasts duplicates"
    );
    let list = held.clone();
    let c_append = c.clone();
    let once = Rc::new(Cell::new(false));
    *a.hook.borrow_mut() = Some(Rc::new(move |_| {
        if !once.replace(true) {
            list.borrow_mut().push(c_append.clone());
        }
    }));
    s.r.broadcast_with_throttle("group", m.clone(), None);
    *a.hook.borrow_mut() = None;
    s.assert_post(10, &c, &m);
    assert_eq!(held.borrow().len(), 4, "PORT native live append consumed");
    a.fail.set(true);
    s.r.ports_by_sub_id
        .borrow_mut()
        .insert("gone".into(), a.clone());
    let only = s.group("only", vec![a.clone()]);
    s.group("empty", vec![]);
    s.r.broadcast_with_throttle("group", m.clone(), None);
    for (i, p) in [a.clone(), a.clone(), b.clone(), c.clone()]
        .iter()
        .enumerate()
    {
        s.assert_post(i + 11, p, &m);
    }
    assert_eq!(
        s.posts.borrow().len(),
        15,
        "PORT native old-array iteration after cleanup"
    );
    assert_eq!(held.borrow().len(), 4);
    assert_eq!(only.borrow().len(), 1);
    assert!(
        !s.r.ports.borrow().contains_key("only"),
        "PORT native cleanup deletes empty group"
    );
    assert!(!s.r.ports.borrow().contains_key("empty"));
    assert!(!s.r.ports_by_sub_id.borrow().contains_key("gone"));
    assert!(s.r.ports_by_sub_id.borrow().contains_key("cross"));
    assert!(s.r.state(&a).is_none());
    let replacement = s.r.ports.borrow()["group"].clone();
    assert!(!Rc::ptr_eq(&held, &replacement));
    assert_eq!(replacement.borrow().len(), 2);
    assert!(Rc::ptr_eq(&replacement.borrow()[0], &b));
    assert!(Rc::ptr_eq(&replacement.borrow()[1], &c));
    // Distinct same-name objects survive identity filtering; leading errors retain pre-post time.
    let failed = Setup::new();
    let p = failed.port("same");
    let twin = failed.port("same");
    failed.group("g", vec![p.clone(), twin.clone(), p.clone()]);
    let old = failed.hidden(&p);
    p.fail.set(true);
    failed.h.now.set(3000.0);
    failed
        .r
        .deliver_to_port_with_throttle(p.clone(), msg("lead-fail"));
    time(&old, 3000.0, "leading failure");
    assert!(failed.r.state(&p).is_none());
    assert!(Rc::ptr_eq(&failed.r.ports.borrow()["g"].borrow()[0], &twin));
    let trailing = Setup::new();
    let p = trailing.port("tail");
    trailing.group("g", vec![p.clone()]);
    let old = trailing.hidden(&p);
    trailing
        .r
        .deliver_to_port_with_throttle(p.clone(), msg("tail-fail"));
    p.fail.set(true);
    trailing.h.now.set(2500.0);
    trailing.h.fire(1);
    time(&old, 0.0, "trailing failure");
    assert_eq!(old.borrow().throttle_timer, Some(1));
    assert!(trailing.r.state(&p).is_none());
    assert!(trailing.r.ports.borrow().is_empty());
    assert_eq!(trailing.h.effects.borrow().last().unwrap().0, "clear");
    // Successful post deletes and replaces state: subsequent writes stay on the held old state.
    let reentrant = Setup::new();
    let p = reentrant.port("reentrant");
    let old = reentrant.hidden(&p);
    reentrant
        .r
        .deliver_to_port_with_throttle(p.clone(), msg("x"));
    let reg = reentrant.r.clone();
    let rp = p.clone();
    let holder = Rc::new(RefCell::new(None));
    let slot = holder.clone();
    *p.hook.borrow_mut() = Some(Rc::new(move |_| {
        reg.cleanup_port_state(&rp);
        let new = reg.ensure_state(&rp);
        new.borrow_mut().last_delivery_time = 777.0;
        new.borrow_mut().pending_update = Some(msg("replacement"));
        *slot.borrow_mut() = Some(new);
    }));
    reentrant.h.now.set(2500.0);
    reentrant.h.fire(1);
    *p.hook.borrow_mut() = None;
    let new = holder.borrow().clone().unwrap();
    time(&old, 2500.0, "reentrant old post state");
    time(&new, 777.0, "reentrant replacement untouched");
    assert!(old.borrow().pending_update.is_none());
    assert_eq!(
        new.borrow().pending_update.as_ref().unwrap().label,
        "replacement"
    );
    let own = Setup::new();
    let op = own.port("own");
    own.group("g", vec![op.clone()]);
    let om = msg("own");
    own.r
        .broadcast_with_throttle("g", om.clone(), Some("constructor"));
    assert_eq!(
        own.posts.borrow().len(),
        1,
        "PORT native own-map inherited key fallback"
    );
    own.assert_post(0, &op, &om);
    let native = Routing {
        broadcast: s.profile(),
        leading_failure: failed.profile(),
        trailing_failure: trailing.profile(),
        reentrant: reentrant.profile(),
    };
    let source = oracle();
    assert_eq!(native, decoded::<Routing>(&source["routing"]));
    // Explicit chosen boundary: no inherited Object.prototype sub-id lookup in own-key maps.
    assert_eq!(
        source["prototypeFailure"], true,
        "PORT source inherited sub-id selects non-port"
    );
}
