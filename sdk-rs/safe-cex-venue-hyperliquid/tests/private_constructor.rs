#[allow(dead_code)]
#[path = "support/cache_host.rs"]
mod cache;
use cache::{N, arr, eq, object, s};
use safe_cex_venue_hyperliquid::{
    json_boundaries::JsPayloadHost,
    private_constructor::*,
    ws_ping_lifecycle::{PingCallback, WsPingHost, private_on_open},
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    panic::{AssertUnwindSafe, catch_unwind},
    process::Command,
    rc::Rc,
};
const FIELDS: [&str; 27] = [
    "hlPingTimeoutId",
    "timestampState",
    "lastCoreClearinghouseStateAt",
    "lastCoreClearinghouseStateServerTime",
    "realizedPnlByPositionKey",
    "processedRealizedPnlFillIds",
    "processedRealizedPnlFillQueue",
    "connectAndSubscribe",
    "onOpen",
    "startPingLoop",
    "schedulePing",
    "stopPingLoop",
    "subscribe",
    "ping",
    "subscribeSystemTopics",
    "subscribeAllActiveAssetData",
    "onMessage",
    "sendWithDiagnostics",
    "emitReceiveDiagnostics",
    "_hip3MidsUpdateBuf",
    "handleUserEvents",
    "dispose",
    "handleHip3ClearinghouseState",
    "handleFills",
    "handleFundings",
    "handleUserNonFundingLedgerUpdates",
    "handleOrderUpdates",
];
const ARROWS: [(&str, CorePrivateArrow, u8); 19] = [
    (
        "connectAndSubscribe",
        CorePrivateArrow::ConnectAndSubscribe,
        0,
    ),
    ("onOpen", CorePrivateArrow::OnOpen, 0),
    ("startPingLoop", CorePrivateArrow::StartPingLoop, 0),
    ("schedulePing", CorePrivateArrow::SchedulePing, 0),
    ("stopPingLoop", CorePrivateArrow::StopPingLoop, 0),
    ("subscribe", CorePrivateArrow::Subscribe, 0),
    ("ping", CorePrivateArrow::Ping, 0),
    (
        "subscribeSystemTopics",
        CorePrivateArrow::SubscribeSystemTopics,
        0,
    ),
    (
        "subscribeAllActiveAssetData",
        CorePrivateArrow::SubscribeAllActiveAssetData,
        0,
    ),
    ("onMessage", CorePrivateArrow::OnMessage, 1),
    (
        "sendWithDiagnostics",
        CorePrivateArrow::SendWithDiagnostics,
        1,
    ),
    (
        "emitReceiveDiagnostics",
        CorePrivateArrow::EmitReceiveDiagnostics,
        2,
    ),
    ("handleUserEvents", CorePrivateArrow::HandleUserEvents, 1),
    ("dispose", CorePrivateArrow::Dispose, 0),
    (
        "handleHip3ClearinghouseState",
        CorePrivateArrow::HandleHip3ClearinghouseState,
        2,
    ),
    ("handleFills", CorePrivateArrow::HandleFills, 1),
    ("handleFundings", CorePrivateArrow::HandleFundings, 1),
    (
        "handleUserNonFundingLedgerUpdates",
        CorePrivateArrow::HandleUserNonFundingLedgerUpdates,
        1,
    ),
    (
        "handleOrderUpdates",
        CorePrivateArrow::HandleOrderUpdates,
        1,
    ),
];
const SCENES: [&str; 14] = [
    "normal",
    "parent-undefined",
    "parent-null",
    "parent-utf16",
    "replacement",
    "subclass",
    "foreign-target",
    "base-error",
    "map-first-error",
    "map-third-error",
    "set-error",
    "double",
    "detached",
    "base-marker",
];
fn get(v: &N, key: &str) -> N {
    JsPayloadHost::get(&mut cache::Host::default(), v, key).unwrap()
}
fn put(v: &N, key: &str, value: &N) {
    cache::Host::default().put(v, key, value, true).unwrap()
}
fn keys(v: &N) -> Vec<String> {
    let N::O(o) = v else {
        panic!("CTOR native object")
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
    scene: String,
    fail_field: Option<String>,
    class: N,
    parent: N,
    target: N,
    proto: N,
    replacement: N,
    marker: N,
    receiver: Option<N>,
    maps: Vec<N>,
    sets: Vec<N>,
    callbacks: HashMap<String, (CorePrivateCapture<N>, String, u8)>,
    effects: Vec<Value>,
    global_reads: Vec<String>,
    last_defined: Vec<String>,
    next: usize,
}
impl Host {
    fn new(scene: &str, fail_field: Option<&str>) -> Self {
        let class = N::Class(Rc::new(()));
        let target = if ["subclass", "foreign-target"].contains(&scene) {
            N::Class(Rc::new(()))
        } else {
            class.clone()
        };
        let parent = match scene {
            "parent-undefined" => N::U,
            "parent-null" => N::Null,
            "parent-utf16" => N::S(safe_cex_venue_hyperliquid::spot_json::JsString(vec![
                0xd800, 80,
            ])),
            _ => object(vec![("token", s("original-parent"))]),
        };
        Self {
            scene: scene.into(),
            fail_field: fail_field.map(str::to_string),
            class,
            parent,
            target,
            proto: object(vec![]),
            replacement: object(vec![]),
            marker: object(vec![("original-error", N::B(true))]),
            receiver: None,
            maps: vec![],
            sets: vec![],
            callbacks: HashMap::new(),
            effects: vec![],
            global_reads: vec![],
            last_defined: vec![],
            next: 0,
        }
    }
    fn construct_socket(&mut self) -> Result<N, N> {
        let class = self.class.clone();
        let parent = self.parent.clone();
        let target = self.target.clone();
        let explicit = ["subclass", "foreign-target"].contains(&self.scene.as_str());
        new_core_private_socket(self, &class, &parent, explicit.then_some(&target))
    }
    fn arrow_record(&self, arrow: &N) -> &(CorePrivateCapture<N>, String, u8) {
        let N::Builtin(id) = arrow else {
            panic!("CTOR native arrow")
        };
        self.callbacks.get(id).expect("CTOR own callable")
    }
}
impl WsPingHost for Host {
    type Value = N;
    type Error = N;
    fn undefined(&self) -> N {
        N::U
    }
    fn literal(&self, v: &safe_cex_venue_hyperliquid::spot_json::JsString) -> N {
        N::S(v.clone())
    }
    fn truthy(&self, v: &N) -> bool {
        !matches!(v, N::U | N::Null | N::B(false))
    }
    fn global(&mut self, k: &str) -> Result<N, N> {
        self.global_reads.push(k.into());
        Ok(N::Builtin(k.into()))
    }
    fn get(&mut self, v: &N, k: &str) -> Result<N, N> {
        Ok(get(v, k))
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
            if self.fail_field.as_deref() == Some(k) {
                return Err(self.marker.clone());
            }
            self.last_defined.push(k.into());
        }
        put(v, k, value);
        Ok(())
    }
    fn callback(&mut self, _: PingCallback<N>) -> Result<N, N> {
        panic!("CTOR unused timer binding")
    }
    fn call(&mut self, f: &N, r: &N, args: &[N]) -> Result<N, N> {
        let N::Builtin(id) = f else {
            panic!("CTOR callee")
        };
        if let Some((capture, _, _)) = self.callbacks.get(id).cloned() {
            assert_eq!(
                capture.body,
                CorePrivateArrow::OnOpen,
                "CTOR selected native body"
            );
            private_on_open(self, &capture.receiver)?;
            return Ok(N::U);
        }
        assert!(matches!(
            id.as_str(),
            "trackLastReceived" | "subscribe" | "startPingLoop"
        ));
        assert!(args.is_empty(), "CTOR native opening call arity");
        assert!(
            self.receiver.as_ref().is_some_and(|native| eq(native, r)),
            "CTOR native detached captured receiver"
        );
        self.effects.push(json!(["call", id]));
        Ok(N::U)
    }
}
impl PrivateConstructorHost for Host {
    fn number(&self, v: f64) -> N {
        N::Num(v)
    }
    fn array(&mut self) -> Result<N, N> {
        Ok(arr(vec![]))
    }
    fn construct(&mut self, constructor: &N, args: &[N]) -> Result<N, N> {
        assert!(args.is_empty(), "CTOR native empty collection arguments");
        let N::Builtin(k) = constructor else {
            panic!("CTOR collection constructor")
        };
        match k.as_str() {
            "Map" => {
                self.effects.push(json!(["Map"]));
                if (self.scene == "map-first-error" && self.maps.is_empty())
                    || (self.scene == "map-third-error" && self.maps.len() == 2)
                {
                    return Err(self.marker.clone());
                }
                let v = N::Map(Rc::new(std::cell::RefCell::new(vec![])));
                self.maps.push(v.clone());
                Ok(v)
            }
            "Set" => {
                self.effects.push(json!(["Set"]));
                if self.scene == "set-error" {
                    return Err(self.marker.clone());
                }
                let v = N::Set(Rc::new(std::cell::RefCell::new(vec![])));
                self.sets.push(v.clone());
                Ok(v)
            }
            _ => panic!("CTOR native constructor selector"),
        }
    }
    fn private_super(&mut self, class: &N, parent: &N, target: &N) -> Result<N, N> {
        assert!(eq(class, &self.class), "CTOR native defining class");
        assert!(eq(parent, &self.parent), "CTOR native original parent");
        assert!(eq(target, &self.target), "CTOR native original newTarget");
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
    fn private_arrow(
        &mut self,
        capture: CorePrivateCapture<N>,
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
    let failed = h.scene == "base-error" || h.scene.ends_with("-error") || h.fail_field.is_some();
    assert_eq!(result.is_err(), failed, "CTOR native independent rejection");
    if let Err(e) = result {
        assert!(eq(e, &h.marker), "CTOR native original error identity")
    }
    if h.scene == "base-error" {
        assert!(
            h.receiver.is_none(),
            "CTOR native no fields after super failure"
        );
        assert!(h.callbacks.is_empty());
        return;
    }
    let receiver = h.receiver.as_ref().unwrap();
    if let Ok(value) = result {
        assert!(eq(value, receiver), "CTOR native actual base receiver");
        if h.scene == "replacement" {
            assert!(eq(value, &h.replacement), "CTOR native replacement alias")
        }
    }
    let stopped = if let Some(field) = &h.fail_field {
        FIELDS.iter().position(|f| *f == field).unwrap()
    } else {
        match h.scene.as_str() {
            "map-first-error" => 1,
            "map-third-error" => 4,
            "set-error" => 5,
            _ => 27,
        }
    };
    let initialized = stopped + usize::from(h.fail_field.is_some());
    let expected_arrows = FIELDS[..initialized]
        .iter()
        .filter(|name| ARROWS.iter().any(|(arrow, _, _)| arrow == *name))
        .count();
    assert_eq!(
        h.callbacks.len(),
        expected_arrows,
        "CTOR native partial arrow allocation floor"
    );
    let expected_maps = if h.scene == "map-first-error" || stopped == 0 {
        0
    } else {
        2 + usize::from(stopped >= 4 && h.scene != "map-third-error")
    };
    assert_eq!(
        h.maps.len(),
        expected_maps,
        "CTOR native partial Map allocation floor"
    );
    assert_eq!(
        h.sets.len(),
        usize::from(stopped >= 5 && h.scene != "set-error"),
        "CTOR native partial Set allocation floor"
    );
    let mut expected = vec!["parent".to_string(), "baseMark".into()];
    expected.extend(FIELDS[..stopped].iter().map(|s| s.to_string()));
    assert_eq!(
        keys(receiver),
        expected,
        "CTOR native exact field prefix and order"
    );
    assert!(
        eq(&get(receiver, "parent"), &h.parent),
        "CTOR native parent retained"
    );
    let N::O(o) = receiver else { unreachable!() };
    assert!(
        eq(o.borrow().prototype.as_ref().unwrap(), &h.proto),
        "CTOR native supplied base prototype binding"
    );
    if stopped > 0 {
        assert!(
            matches!(get(receiver, "hlPingTimeoutId"), N::U),
            "CTOR native own undefined field"
        )
    }
    if stopped > 1 {
        let state = get(receiver, "timestampState");
        assert_eq!(
            keys(&state),
            ["lastTime", "lastDataHash"],
            "CTOR native timestamp field order"
        );
        assert!(
            eq(&get(&state, "lastTime"), &h.maps[0]),
            "CTOR native first Map member identity"
        );
        assert!(
            eq(&get(&state, "lastDataHash"), &h.maps[1]),
            "CTOR native second Map member identity"
        );
        assert!(
            !eq(&h.maps[0], &h.maps[1]),
            "CTOR native independent timestamp maps"
        )
    }
    for k in [
        "lastCoreClearinghouseStateAt",
        "lastCoreClearinghouseStateServerTime",
    ] {
        if keys(receiver).iter().any(|x| x == k) {
            assert!(
                matches!(get(receiver,k),N::Num(n)if n.to_bits()==0f64.to_bits()),
                "CTOR native positive zero state"
            )
        }
    }
    if stopped > 4 {
        assert!(
            eq(&get(receiver, "realizedPnlByPositionKey"), &h.maps[2]),
            "CTOR native third map"
        )
    }
    if stopped > 5 {
        assert!(
            eq(&get(receiver, "processedRealizedPnlFillIds"), &h.sets[0]),
            "CTOR native fill Set"
        )
    }
    if stopped > 6 {
        assert!(
            matches!(get(receiver,"processedRealizedPnlFillQueue"),N::A(a)if a.borrow().is_empty()),
            "CTOR native empty queue"
        )
    }
    for (name, body, length) in ARROWS {
        if keys(receiver).iter().any(|k| k == name) {
            let (capture, actual_name, actual_length) = h.arrow_record(&get(receiver, name));
            assert_eq!(
                capture.body, body,
                "CTOR native mapped arrow selector {name}"
            );
            assert_eq!(actual_name, name, "CTOR native arrow name");
            assert_eq!(*actual_length, length, "CTOR native arrow length");
            assert!(
                eq(&capture.receiver, receiver),
                "CTOR native lexical arrow receiver"
            );
            assert!(
                eq(&capture.class, &h.class),
                "CTOR native original lexical class"
            );
        }
    }
    if stopped > 19 {
        let buf = get(receiver, "_hip3MidsUpdateBuf");
        assert_eq!(
            keys(&buf),
            ["last", "midPrice"],
            "CTOR native buffer field order"
        );
        for k in ["last", "midPrice"] {
            assert!(
                matches!(get(&buf,k),N::Num(n)if n.to_bits()==0f64.to_bits()),
                "CTOR native positive zero buffer"
            )
        }
    }
    if !failed {
        assert_eq!(
            h.callbacks.len(),
            19,
            "CTOR native independent callback floor"
        );
        assert_eq!(
            h.global_reads,
            ["Map", "Map", "Map", "Set"],
            "CTOR native global constructor order"
        );
        assert_eq!(h.maps.len(), 3, "CTOR native independent Map floor");
        assert_eq!(h.sets.len(), 1, "CTOR native independent Set floor");
    }
}
fn run(scene: &str, fail: Option<&str>) -> Value {
    let mut h = Host::new(scene, fail);
    let result = h.construct_socket();
    assert_fields(&h, &result);
    let first = h.receiver.clone();
    if scene == "double" {
        let old = first.as_ref().unwrap().clone();
        let old_callbacks = h.callbacks.clone();
        h.maps.clear();
        h.sets.clear();
        h.global_reads.clear();
        h.callbacks.clear();
        let second = h.construct_socket();
        assert_fields(&h, &second);
        let second = second.unwrap();
        assert!(!eq(&old, &second), "CTOR native independent instances");
        for k in [
            "timestampState",
            "realizedPnlByPositionKey",
            "processedRealizedPnlFillIds",
            "processedRealizedPnlFillQueue",
            "_hip3MidsUpdateBuf",
        ] {
            assert!(
                !eq(&get(&old, k), &get(&second, k)),
                "CTOR native independent member {k}"
            )
        }
        for (name, _, _) in ARROWS {
            assert!(
                !same(&get(&old, name), &get(&second, name)),
                "CTOR native fresh arrow {name}"
            );
            let N::Builtin(id) = get(&old, name) else {
                unreachable!()
            };
            assert!(eq(&old_callbacks[&id].0.receiver, &old));
        }
    }
    if scene == "detached" {
        let receiver = first.as_ref().unwrap();
        put(receiver, "isDisposed", &N::B(false));
        for k in ["trackLastReceived", "subscribe", "startPingLoop"] {
            put(receiver, k, &N::Builtin(k.into()))
        }
        let arrow = get(receiver, "onOpen");
        let before = h.effects.len();
        let out = h
            .call(&arrow, &object(vec![("isDisposed", N::B(true))]), &[])
            .unwrap();
        assert!(matches!(out, N::U), "CTOR native opening void return");
        assert_eq!(
            &h.effects[before..],
            &[
                json!(["call", "trackLastReceived"]),
                json!(["call", "subscribe"]),
                json!(["call", "startPingLoop"])
            ],
            "CTOR native detached onOpen body"
        );
    }
    json!({"scene":scene,"fail":fail,"rejected":result.is_err(),"effects":h.effects})
}
#[test]
fn private_constructor_native_before_source() {
    let mut outputs = vec![];
    let mut failures = vec![];
    for scene in SCENES {
        match catch_unwind(AssertUnwindSafe(|| run(scene, None))) {
            Ok(v) => outputs.push(v),
            Err(_) => failures.push(scene.to_string()),
        }
    }
    for field in FIELDS {
        match catch_unwind(AssertUnwindSafe(|| run("define-error", Some(field)))) {
            Ok(v) => outputs.push(v),
            Err(_) => failures.push(format!("define/{field}")),
        }
    }
    assert!(failures.is_empty(), "CTOR native failures {failures:?}");
    assert_eq!(
        outputs.len(),
        41,
        "CTOR native independently completed floor"
    );
    let source = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/private-constructor-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "CTOR source gate {}",
        String::from_utf8_lossy(&source.stderr)
    );
    let value: Value = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(value["last_index"], 40);
    assert_eq!(
        value["cases"],
        json!(outputs),
        "CTOR source effects after all native assertions"
    );
}
