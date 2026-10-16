#[allow(dead_code)]
#[path = "support/cache_host.rs"]
mod cache;
use cache::{N, eq, object, s};
use safe_cex_venue_hyperliquid::{
    json_boundaries::JsPayloadHost,
    public_constructor::*,
    spot_json::JsString,
    ws_ping_lifecycle::{PingCallback, WsPingHost, public_on_open},
    ws_transport_leaves::spot_public_on_open,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::HashMap,
    panic::{AssertUnwindSafe, catch_unwind},
    process::Command,
    rc::Rc,
};
const CORE_FIELDS: [&str; 25] = [
    "topics",
    "messageHandlers",
    "activeSubscriptions",
    "hlPingTimeoutId",
    "connectAndSubscribe",
    "onOpen",
    "startPingLoop",
    "schedulePing",
    "stopPingLoop",
    "subscribePublicTopics",
    "sendSubscription",
    "sendUnsubscription",
    "subscribeUserTopics",
    "ping",
    "onMessage",
    "sendWithDiagnostics",
    "emitReceiveDiagnostics",
    "_midsUpdateBuf",
    "handleAllMids",
    "_fastMarkUpdateBuf",
    "_fastMidUpdateBuf",
    "_fastMarkAndMidUpdateBuf",
    "handleFastAssetCtxs",
    "listenOHLCV",
    "listenOrderBook",
];
const CORE_ARROWS: [(&str, PublicSocketArrow, u8); 17] = [
    (
        "connectAndSubscribe",
        PublicSocketArrow::ConnectAndSubscribe,
        0,
    ),
    ("onOpen", PublicSocketArrow::OnOpen, 0),
    ("startPingLoop", PublicSocketArrow::StartPingLoop, 0),
    ("schedulePing", PublicSocketArrow::SchedulePing, 0),
    ("stopPingLoop", PublicSocketArrow::StopPingLoop, 0),
    (
        "subscribePublicTopics",
        PublicSocketArrow::SubscribePublicTopics,
        0,
    ),
    ("sendSubscription", PublicSocketArrow::SendSubscription, 1),
    (
        "sendUnsubscription",
        PublicSocketArrow::SendUnsubscription,
        1,
    ),
    (
        "subscribeUserTopics",
        PublicSocketArrow::SubscribeUserTopics,
        0,
    ),
    ("ping", PublicSocketArrow::Ping, 0),
    ("onMessage", PublicSocketArrow::OnMessage, 1),
    (
        "sendWithDiagnostics",
        PublicSocketArrow::SendWithDiagnostics,
        1,
    ),
    (
        "emitReceiveDiagnostics",
        PublicSocketArrow::EmitReceiveDiagnostics,
        2,
    ),
    ("handleAllMids", PublicSocketArrow::HandleAllMids, 1),
    (
        "handleFastAssetCtxs",
        PublicSocketArrow::HandleFastAssetCtxs,
        1,
    ),
    ("listenOHLCV", PublicSocketArrow::ListenOHLCV, 2),
    ("listenOrderBook", PublicSocketArrow::ListenOrderBook, 2),
];
const SPOT_FIELDS: [&str; 12] = [
    "topics",
    "messageHandlers",
    "connectAndSubscribe",
    "onOpen",
    "subscribePublicTopics",
    "subscribeUserTopics",
    "ping",
    "onMessage",
    "_midsUpdateBuf",
    "handleAllMids",
    "listenOHLCV",
    "listenOrderBook",
];
const SPOT_ARROWS: [(&str, PublicSocketArrow, u8); 9] = [
    (
        "connectAndSubscribe",
        PublicSocketArrow::ConnectAndSubscribe,
        0,
    ),
    ("onOpen", PublicSocketArrow::OnOpen, 0),
    (
        "subscribePublicTopics",
        PublicSocketArrow::SubscribePublicTopics,
        0,
    ),
    (
        "subscribeUserTopics",
        PublicSocketArrow::SubscribeUserTopics,
        0,
    ),
    ("ping", PublicSocketArrow::Ping, 0),
    ("onMessage", PublicSocketArrow::OnMessage, 1),
    ("handleAllMids", PublicSocketArrow::HandleAllMids, 1),
    ("listenOHLCV", PublicSocketArrow::ListenOHLCV, 2),
    ("listenOrderBook", PublicSocketArrow::ListenOrderBook, 2),
];
const SCENES: [&str; 11] = [
    "normal",
    "parent-undefined",
    "parent-null",
    "parent-utf16",
    "replacement",
    "subclass",
    "foreign-target",
    "base-error",
    "double",
    "detached",
    "base-marker",
];
fn fields(kind: PublicSocketKind) -> &'static [&'static str] {
    match kind {
        PublicSocketKind::Core => &CORE_FIELDS,
        PublicSocketKind::Spot => &SPOT_FIELDS,
    }
}
fn arrows(kind: PublicSocketKind) -> &'static [(&'static str, PublicSocketArrow, u8)] {
    match kind {
        PublicSocketKind::Core => &CORE_ARROWS,
        PublicSocketKind::Spot => &SPOT_ARROWS,
    }
}
fn get(v: &N, k: &str) -> N {
    JsPayloadHost::get(&mut cache::Host::default(), v, k).unwrap()
}
fn put(v: &N, k: &str, value: &N) {
    cache::Host::default().put(v, k, value, true).unwrap()
}
fn keys(v: &N) -> Vec<String> {
    let N::O(o) = v else {
        panic!("PUBLIC-CTOR native object")
    };
    o.borrow()
        .fields
        .iter()
        .filter(|p| p.2)
        .map(|p| p.0.clone())
        .collect()
}
fn same(a: &N, b: &N) -> bool {
    eq(a, b) || matches!((a,b),(N::Builtin(a),N::Builtin(b))if a==b)
}
struct Host {
    kind: PublicSocketKind,
    scene: String,
    fail: Option<String>,
    class: N,
    parent: N,
    target: N,
    proto: N,
    replacement: N,
    marker: N,
    receiver: Option<N>,
    sets: Vec<N>,
    callbacks: HashMap<String, (PublicSocketCapture<N>, String, u8)>,
    effects: Vec<Value>,
    global_reads: Vec<String>,
    next: usize,
}
impl Host {
    fn new(kind: PublicSocketKind, scene: &str, fail: Option<&str>) -> Self {
        let class = N::Class(Rc::new(()));
        let target = if ["subclass", "foreign-target"].contains(&scene) {
            N::Class(Rc::new(()))
        } else {
            class.clone()
        };
        let parent = match scene {
            "parent-undefined" => N::U,
            "parent-null" => N::Null,
            "parent-utf16" => N::S(JsString(vec![0xd800, 80])),
            _ => object(vec![("token", s("original-parent"))]),
        };
        Self {
            kind,
            scene: scene.into(),
            fail: fail.map(str::to_owned),
            class,
            parent,
            target,
            proto: object(vec![]),
            replacement: object(vec![]),
            marker: object(vec![("original-error", N::B(true))]),
            receiver: None,
            sets: vec![],
            callbacks: HashMap::new(),
            effects: vec![],
            global_reads: vec![],
            next: 0,
        }
    }
    fn construct_socket(&mut self) -> Result<N, N> {
        let class = self.class.clone();
        let parent = self.parent.clone();
        let target = self.target.clone();
        let explicit = ["subclass", "foreign-target"].contains(&self.scene.as_str());
        match self.kind {
            PublicSocketKind::Core => {
                new_core_public_socket(self, &class, &parent, explicit.then_some(&target))
            }
            PublicSocketKind::Spot => {
                new_spot_public_socket(self, &class, &parent, explicit.then_some(&target))
            }
        }
    }
    fn record(&self, arrow: &N) -> &(PublicSocketCapture<N>, String, u8) {
        let N::Builtin(id) = arrow else {
            panic!("PUBLIC-CTOR native arrow")
        };
        self.callbacks.get(id).expect("PUBLIC-CTOR callable")
    }
}
impl WsPingHost for Host {
    type Value = N;
    type Error = N;
    fn undefined(&self) -> N {
        N::U
    }
    fn literal(&self, v: &JsString) -> N {
        N::S(v.clone())
    }
    fn truthy(&self, v: &N) -> bool {
        !matches!(v, N::U | N::Null | N::B(false))
            && !matches!(v,N::Num(n)if *n==0.||n.is_nan())
            && !matches!(v,N::S(s)if s.0.is_empty())
    }
    fn global(&mut self, k: &str) -> Result<N, N> {
        self.global_reads.push(k.into());
        if k == "Set" {
            self.effects.push(json!(["get-Set"]));
            if self.scene == "set-read-error" {
                return Err(self.marker.clone());
            }
        }
        Ok(if k == "LogSeverity" {
            object(vec![("Debug", s("debug"))])
        } else {
            N::Builtin(k.into())
        })
    }
    fn get(&mut self, v: &N, k: &str) -> Result<N, N> {
        if matches!(v, N::Set(_)) && k == "clear" {
            Ok(N::Builtin("clear".into()))
        } else {
            Ok(get(v, k))
        }
    }
    fn set(&mut self, v: &N, k: &str, value: &N) -> Result<(), N> {
        put(v, k, value);
        Ok(())
    }
    fn object(&mut self) -> Result<N, N> {
        Ok(object(vec![]))
    }
    fn define_data(&mut self, v: &N, k: &str, value: &N) -> Result<(), N> {
        if self.receiver.as_ref().is_some_and(|r| eq(r, v)) {
            self.effects.push(json!(["define", k]));
            if self.fail.as_deref() == Some(k) {
                return Err(self.marker.clone());
            }
        }
        put(v, k, value);
        Ok(())
    }
    fn callback(&mut self, _: PingCallback<N>) -> Result<N, N> {
        panic!("PUBLIC-CTOR unused timer")
    }
    fn call(&mut self, f: &N, r: &N, args: &[N]) -> Result<N, N> {
        let N::Builtin(id) = f else {
            panic!("PUBLIC-CTOR callee")
        };
        if let Some((capture, _, _)) = self.callbacks.get(id).cloned() {
            assert_eq!(
                capture.body,
                PublicSocketArrow::OnOpen,
                "PUBLIC-CTOR selected native body"
            );
            match capture.kind {
                PublicSocketKind::Core => public_on_open(self, &capture.receiver)?,
                PublicSocketKind::Spot => spot_public_on_open(self, &capture.receiver)?,
            };
            return Ok(N::U);
        }
        if id == "log" {
            assert!(eq(r, &self.parent), "PUBLIC-CTOR original log receiver");
            assert_eq!(args.len(), 2);
            assert!(same(
                &args[0],
                &s("Public WebSocket connection established")
            ));
            assert!(same(&args[1], &s("debug")));
        } else if id == "clear" {
            assert!(
                self.sets.iter().any(|set| eq(set, r)),
                "PUBLIC-CTOR original active Set"
            );
            assert!(args.is_empty());
            let N::Set(set) = r else { unreachable!() };
            set.borrow_mut().clear();
        } else {
            assert!(
                [
                    "trackLastReceived",
                    "resetOrderbook",
                    "subscribePublicTopics",
                    "startPingLoop",
                    "ping"
                ]
                .contains(&id.as_str())
            );
            assert!(args.is_empty(), "PUBLIC-CTOR native opening arity");
            assert!(
                self.receiver.as_ref().is_some_and(|native| eq(native, r)),
                "PUBLIC-CTOR detached captured receiver"
            );
        }
        self.effects.push(json!(["call", id]));
        Ok(N::U)
    }
}
impl PublicConstructorHost for Host {
    fn number(&self, v: f64) -> N {
        N::Num(v)
    }
    fn construct(&mut self, c: &N, args: &[N]) -> Result<N, N> {
        assert!(matches!(c,N::Builtin(k)if k=="Set"));
        assert!(args.is_empty(), "PUBLIC-CTOR native empty Set args");
        self.effects.push(json!(["Set"]));
        if self.scene == "set-error" {
            return Err(self.marker.clone());
        }
        let set = N::Set(Rc::new(RefCell::new(vec![])));
        self.sets.push(set.clone());
        Ok(set)
    }
    fn public_super(&mut self, class: &N, parent: &N, target: &N) -> Result<N, N> {
        assert!(eq(class, &self.class), "PUBLIC-CTOR defining class");
        assert!(eq(parent, &self.parent), "PUBLIC-CTOR original parent");
        assert!(eq(target, &self.target), "PUBLIC-CTOR original newTarget");
        self.effects.push(json!([
            "super",
            if eq(target, class) { "class" } else { "custom" }
        ]));
        if self.scene == "base-error" {
            return Err(self.marker.clone());
        }
        let v = if self.scene == "replacement" {
            self.replacement.clone()
        } else {
            object(vec![])
        };
        put(&v, "parent", parent);
        put(
            &v,
            "baseMark",
            &s(if self.scene == "base-marker" {
                "alternate"
            } else {
                "base"
            }),
        );
        let N::O(o) = &v else { unreachable!() };
        o.borrow_mut().prototype = Some(self.proto.clone());
        self.receiver = Some(v.clone());
        Ok(v)
    }
    fn public_arrow(
        &mut self,
        capture: PublicSocketCapture<N>,
        name: &str,
        length: u8,
    ) -> Result<N, N> {
        let id = format!("arrow{}", self.next);
        self.next += 1;
        self.callbacks
            .insert(id.clone(), (capture, name.into(), length));
        Ok(N::Builtin(id))
    }
}
fn assert_fields(h: &Host, result: &Result<N, N>) {
    let failed = h.scene.ends_with("-error") || h.fail.is_some();
    assert_eq!(result.is_err(), failed, "PUBLIC-CTOR independent rejection");
    if let Err(error) = result {
        assert!(eq(error, &h.marker), "PUBLIC-CTOR original error identity")
    };
    if h.scene == "base-error" {
        assert!(
            h.receiver.is_none(),
            "PUBLIC-CTOR no receiver after super error"
        );
        assert!(h.callbacks.is_empty());
        return;
    }
    let receiver = h.receiver.as_ref().unwrap();
    if let Ok(value) = result {
        assert!(eq(value, receiver), "PUBLIC-CTOR actual base receiver");
        if h.scene == "replacement" {
            assert!(eq(value, &h.replacement), "PUBLIC-CTOR replacement alias")
        }
    }
    let expected = fields(h.kind);
    let stopped = if let Some(fail) = &h.fail {
        expected.iter().position(|k| k == fail).unwrap()
    } else if h.scene == "set-error" || h.scene == "set-read-error" {
        2
    } else {
        match h.kind {
            PublicSocketKind::Core => 25,
            PublicSocketKind::Spot => 12,
        }
    };
    let initialized = stopped + usize::from(h.fail.is_some());
    let expected_callbacks = expected[..initialized]
        .iter()
        .filter(|name| arrows(h.kind).iter().any(|(arrow, _, _)| arrow == *name))
        .count();
    assert_eq!(
        h.callbacks.len(),
        expected_callbacks,
        "PUBLIC-CTOR independent partial arrow allocation floor"
    );
    let mut own = vec!["parent".to_string(), "baseMark".into()];
    own.extend(expected[..stopped].iter().map(|s| s.to_string()));
    assert_eq!(keys(receiver), own, "PUBLIC-CTOR exact field prefix/order");
    assert!(
        eq(&get(receiver, "parent"), &h.parent),
        "PUBLIC-CTOR parent identity"
    );
    let N::O(o) = receiver else { unreachable!() };
    assert!(
        eq(o.borrow().prototype.as_ref().unwrap(), &h.proto),
        "PUBLIC-CTOR supplied base prototype binding"
    );
    for name in ["topics", "messageHandlers"] {
        if expected[..stopped].contains(&name) {
            assert!(
                keys(&get(receiver, name)).is_empty(),
                "PUBLIC-CTOR empty native object"
            );
        }
    }
    if stopped >= 2 {
        assert!(
            !eq(&get(receiver, "topics"), &get(receiver, "messageHandlers")),
            "PUBLIC-CTOR separate dictionary objects"
        )
    }
    if expected[..stopped].contains(&"activeSubscriptions") {
        assert_eq!(h.sets.len(), 1);
        assert!(
            eq(&get(receiver, "activeSubscriptions"), &h.sets[0]),
            "PUBLIC-CTOR original Set identity"
        );
        assert!(
            matches!(&h.sets[0],N::Set(set)if set.borrow().is_empty()),
            "PUBLIC-CTOR empty actual Set"
        );
    }
    if expected[..stopped].contains(&"hlPingTimeoutId") {
        assert!(
            matches!(get(receiver, "hlPingTimeoutId"), N::U),
            "PUBLIC-CTOR own undefined field"
        )
    }
    for (name, numeric) in [
        ("_midsUpdateBuf", &["last", "midPrice"][..]),
        ("_fastMarkUpdateBuf", &["mark"][..]),
        ("_fastMidUpdateBuf", &["last", "midPrice"][..]),
        (
            "_fastMarkAndMidUpdateBuf",
            &["mark", "last", "midPrice"][..],
        ),
    ] {
        if expected[..stopped].contains(&name) {
            let value = get(receiver, name);
            assert_eq!(keys(&value), numeric, "PUBLIC-CTOR native buffer key order");
            for field in numeric {
                assert!(
                    matches!(get(&value,field),N::Num(n)if n.to_bits()==0f64.to_bits()),
                    "PUBLIC-CTOR native positive zero buffer"
                )
            }
        }
    }
    let objects = [
        "topics",
        "messageHandlers",
        "activeSubscriptions",
        "_midsUpdateBuf",
        "_fastMarkUpdateBuf",
        "_fastMidUpdateBuf",
        "_fastMarkAndMidUpdateBuf",
    ];
    for (i, left) in objects.iter().enumerate() {
        if !expected[..stopped].contains(left) {
            continue;
        }
        for right in &objects[i + 1..] {
            if expected[..stopped].contains(right) {
                assert!(
                    !eq(&get(receiver, left), &get(receiver, right)),
                    "PUBLIC-CTOR distinct same-instance fields {left}/{right}"
                );
            }
        }
    }
    for (name, body, length) in arrows(h.kind) {
        if expected[..stopped].contains(name) {
            let (capture, actual_name, actual_length) = h.record(&get(receiver, name));
            assert_eq!(
                capture.body, *body,
                "PUBLIC-CTOR mapped arrow selector {name}"
            );
            assert_eq!(capture.kind, h.kind, "PUBLIC-CTOR arrow variant");
            assert_eq!(actual_name, name, "PUBLIC-CTOR arrow name");
            assert_eq!(actual_length, length, "PUBLIC-CTOR arrow length");
            assert!(
                eq(&capture.receiver, receiver),
                "PUBLIC-CTOR lexical arrow receiver"
            );
            assert!(eq(&capture.class, &h.class), "PUBLIC-CTOR lexical class");
        }
    }
    if !failed {
        assert_eq!(
            h.callbacks.len(),
            match h.kind {
                PublicSocketKind::Core => 17,
                PublicSocketKind::Spot => 9,
            },
            "PUBLIC-CTOR independent callback floor"
        );
        assert_eq!(
            h.sets.len(),
            usize::from(h.kind == PublicSocketKind::Core),
            "PUBLIC-CTOR independent Set floor"
        );
        assert_eq!(
            h.global_reads,
            if h.kind == PublicSocketKind::Core {
                vec!["Set"]
            } else {
                vec![]
            },
            "PUBLIC-CTOR global constructor order"
        )
    }
}
fn run(kind: PublicSocketKind, scene: &str, fail: Option<&str>) -> Value {
    let mut h = Host::new(kind, scene, fail);
    let result = h.construct_socket();
    assert_fields(&h, &result);
    let first = h.receiver.clone();
    if scene == "double" {
        let first = first.as_ref().unwrap();
        h.callbacks.clear();
        h.sets.clear();
        h.global_reads.clear();
        let second = h.construct_socket();
        assert_fields(&h, &second);
        let second = second.unwrap();
        assert!(!eq(first, &second), "PUBLIC-CTOR independent instances");
        for name in fields(kind) {
            if *name != "hlPingTimeoutId" {
                assert!(
                    !same(&get(first, name), &get(&second, name)),
                    "PUBLIC-CTOR fresh per-instance field {name}"
                )
            }
        }
    }
    if scene == "detached" {
        let receiver = first.as_ref().unwrap();
        put(receiver, "isDisposed", &N::B(false));
        let methods = if kind == PublicSocketKind::Core {
            vec![
                "trackLastReceived",
                "resetOrderbook",
                "subscribePublicTopics",
                "startPingLoop",
            ]
        } else {
            vec![
                "trackLastReceived",
                "resetOrderbook",
                "subscribePublicTopics",
                "ping",
            ]
        };
        for name in methods {
            put(receiver, name, &N::Builtin(name.into()))
        }
        if kind == PublicSocketKind::Core {
            put(&h.parent, "log", &N::Builtin("log".into()));
            let N::Set(set) = get(receiver, "activeSubscriptions") else {
                panic!("PUBLIC-CTOR active Set")
            };
            set.borrow_mut().push(s("stale"));
        }
        let arrow = get(receiver, "onOpen");
        let before = h.effects.len();
        let out = h
            .call(&arrow, &object(vec![("isDisposed", N::B(true))]), &[])
            .unwrap();
        assert!(matches!(out, N::U), "PUBLIC-CTOR opening void");
        let calls = if kind == PublicSocketKind::Core {
            vec![
                "log",
                "trackLastReceived",
                "resetOrderbook",
                "clear",
                "subscribePublicTopics",
                "startPingLoop",
            ]
        } else {
            vec![
                "trackLastReceived",
                "resetOrderbook",
                "subscribePublicTopics",
                "ping",
            ]
        };
        assert_eq!(
            &h.effects[before..],
            &calls.iter().map(|k| json!(["call", k])).collect::<Vec<_>>(),
            "PUBLIC-CTOR detached original onOpen body"
        );
        if kind == PublicSocketKind::Core {
            assert!(
                matches!(get(receiver,"activeSubscriptions"),N::Set(set)if set.borrow().is_empty()),
                "PUBLIC-CTOR actual native clearing"
            )
        }
    }
    json!({"kind":if kind==PublicSocketKind::Core{"core"}else{"spot"},"scene":scene,"fail":fail,"rejected":result.is_err(),"effects":h.effects})
}
#[test]
fn public_constructors_native_before_source() {
    let mut output = vec![];
    let mut failures = vec![];
    for kind in [PublicSocketKind::Core, PublicSocketKind::Spot] {
        for scene in SCENES.into_iter().chain(if kind == PublicSocketKind::Core {
            vec!["set-error", "set-read-error"]
        } else {
            vec![]
        }) {
            match catch_unwind(AssertUnwindSafe(|| run(kind, scene, None))) {
                Ok(v) => output.push(v),
                Err(_) => failures.push(format!("{kind:?}/{scene}")),
            }
        }
        for field in fields(kind) {
            match catch_unwind(AssertUnwindSafe(|| run(kind, "define-error", Some(field)))) {
                Ok(v) => output.push(v),
                Err(_) => failures.push(format!("{kind:?}/define/{field}")),
            }
        }
    }
    assert!(
        failures.is_empty(),
        "PUBLIC-CTOR native failures {failures:?}"
    );
    assert_eq!(
        output.len(),
        61,
        "PUBLIC-CTOR independently completed floor"
    );
    let source = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/public-constructor-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "PUBLIC-CTOR source {}",
        String::from_utf8_lossy(&source.stderr)
    );
    let value: Value = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(
        value["last_index"], 60,
        "PUBLIC-CTOR final source iteration"
    );
    assert_eq!(
        value["cases"],
        json!(output),
        "PUBLIC-CTOR wire after all native assertions"
    );
}
