use safe_cex_venue_weex::{
    classes::*,
    lifecycle::WeexExchangeConstructionRuntime,
    order_routing,
    public_feed::WeexPublicConstructionRuntime,
    ws_control::{self, WeexWebsocketControlRuntime, WeexWebsocketControlState},
};
use serde::Deserialize;
use std::{cell::RefCell, process::Command, rc::Rc};
#[derive(Clone)]
enum V {
    Undefined,
    Bool(bool),
    Number(f64),
    Text(String),
    Object(Rc<RefCell<Object>>),
    Map(Rc<RefCell<Vec<V>>>),
    Set(Rc<RefCell<Vec<V>>>),
    Array(Rc<RefCell<Vec<V>>>),
    Arrow(BoundMember<V, String>),
}
struct Object {
    fields: Vec<(&'static str, V)>,
    control: WeexWebsocketControlState,
}
fn object(fields: Vec<(&'static str, V)>) -> V {
    V::Object(Rc::new(RefCell::new(Object {
        fields,
        control: WeexWebsocketControlState {
            is_disposed: false,
            subscription_id: 0.0,
            ping_at: 0.0,
            has_logged_unsupported: false,
        },
    })))
}
fn obj(v: &V) -> Rc<RefCell<Object>> {
    match v {
        V::Object(o) => o.clone(),
        _ => panic!("object receiver"),
    }
}
fn same(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Object(a), V::Object(b)) => Rc::ptr_eq(a, b),
        (V::Map(a), V::Map(b)) | (V::Set(a), V::Set(b)) | (V::Array(a), V::Array(b)) => {
            Rc::ptr_eq(a, b)
        }
        (V::Arrow(a), V::Arrow(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
fn get(v: &V, name: &str) -> V {
    let o = obj(v);
    let o = o.borrow();
    match name {
        "subscriptionId" => V::Number(o.control.subscription_id),
        "pingAt" => V::Number(o.control.ping_at),
        "isDisposed" => V::Bool(o.control.is_disposed),
        _ => o
            .fields
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.clone())
            .unwrap_or(V::Undefined),
    }
}
fn set(v: &V, name: &'static str, value: V) {
    let o = obj(v);
    let mut o = o.borrow_mut();
    match (name, &value) {
        ("subscriptionId", V::Number(x)) => o.control.subscription_id = *x,
        ("pingAt", V::Number(x)) => o.control.ping_at = *x,
        ("isDisposed", V::Bool(x)) => o.control.is_disposed = *x,
        _ => (),
    }
    if let Some((_, v)) = o.fields.iter_mut().find(|(n, _)| *n == name) {
        *v = value
    } else {
        o.fields.push((name, value))
    }
}
fn names(v: &V) -> Vec<String> {
    obj(v)
        .borrow()
        .fields
        .iter()
        .map(|(n, _)| n.to_string())
        .collect()
}
fn number(v: V) -> f64 {
    if let V::Number(n) = v {
        n
    } else {
        panic!("native number")
    }
}
fn boolean(v: V) -> bool {
    if let V::Bool(n) = v {
        n
    } else {
        panic!("native boolean")
    }
}
fn text(v: V) -> String {
    if let V::Text(n) = v {
        n
    } else {
        panic!("native string")
    }
}
#[derive(Debug, Deserialize, PartialEq)]
struct Send {
    method: String,
    id: f64,
}
#[derive(Deserialize)]
struct ExpectedClass {
    name: String,
    names: Vec<String>,
    arrows: Vec<String>,
    prototype: Vec<String>,
    collections: Vec<(String, String, usize)>,
}
#[derive(Deserialize)]
struct ExpectedSocket {
    id: f64,
    #[serde(rename = "pingAt")]
    ping_at: f64,
    sent: Vec<Send>,
}
#[derive(Deserialize)]
struct Expected {
    classes: Vec<ExpectedClass>,
    algo: bool,
    sockets: Vec<ExpectedSocket>,
}
fn expected() -> Expected {
    let result = Command::new("node")
        .arg(format!(
            "{}/tests/classes-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}
struct Host {
    calls: Rc<RefCell<Vec<(ClassKind, &'static str, V, Vec<V>)>>>,
    prototypes: RefCell<Vec<(ClassKind, &'static str, NativeMember<V, String>)>>,
    sends: Rc<RefCell<Vec<Send>>>,
    events: RefCell<Vec<(V, V)>>,
    body_assignments: RefCell<Vec<&'static str>>,
    bases: RefCell<usize>,
    missing: Option<(ClassKind, &'static str)>,
    wrong: bool,
    every: V,
    periodic: V,
}
impl Host {
    fn new() -> Rc<Self> {
        Rc::new(Self {
            calls: Default::default(),
            prototypes: Default::default(),
            sends: Default::default(),
            events: Default::default(),
            body_assignments: Default::default(),
            bases: Default::default(),
            missing: None,
            wrong: false,
            every: object(vec![]),
            periodic: object(vec![]),
        })
    }
}
fn socket(h: &Host, parent: V) -> Result<V, String> {
    *h.bases.borrow_mut() += 1;
    let store = get(&parent, "store");
    Ok(object(vec![
        ("parent", parent),
        ("store", store),
        ("isDisposed", V::Bool(false)),
        ("pingAt", V::Number(0.0)),
    ]))
}
impl ClassHost for Host {
    type Value = V;
    type Error = String;
    fn exchange_runtime(&self) -> WeexExchangeConstructionRuntime<Self, V, String> {
        WeexExchangeConstructionRuntime {
            base_construct: |h, name, options, store| {
                *h.bases.borrow_mut() += 1;
                Ok(object(vec![
                    ("name", name),
                    ("options", options),
                    ("store", store),
                ]))
            },
            client_rule: |_, _| Ok(object(vec![])),
            api_factory: |_| Ok(V::Text("createAPI".into())),
            new_map: |_| Ok(V::Map(Default::default())),
            define: |_, r, n, v| {
                set(r, n, v);
                Ok(())
            },
            assign: |h, r, n, v| {
                h.body_assignments.borrow_mut().push(n);
                set(r, n, v);
                Ok(())
            },
            constant: |h, n| {
                Ok(if n == "GRAFANA_ENDPOINTS_EVERY" {
                    h.every.clone()
                } else {
                    h.periodic.clone()
                })
            },
            method: |_, _, n| Ok(V::Text(n.into())),
            call: |_, f, args| {
                assert_eq!(text(f), "createXhrWithSecondaryKey");
                assert_eq!(text(args[1].clone()), "createAPI");
                Ok(object(vec![]))
            },
            construct: |_, n, parent| {
                Ok(object(vec![
                    ("kind", V::Text(n.into())),
                    ("parent", parent),
                ]))
            },
            undefined: || V::Undefined,
            text: |s| V::Text(s.into()),
        }
    }
    fn public_runtime(&self) -> WeexPublicConstructionRuntime<Self, V, String> {
        WeexPublicConstructionRuntime {
            base_construct: socket,
            define: |_, r, n, v| {
                set(r, n, v);
                Ok(())
            },
            new_set: |_| Ok(V::Set(Default::default())),
            object,
            array: |_| V::Array(Default::default()),
            number: V::Number,
            boolean: V::Bool,
            property: |_, r, n| Ok(get(r, n)),
            method: |_, _, n| Ok(V::Text(n.into())),
            call_event: |h, on, parent, event, handler| {
                assert_eq!(text(on), "on");
                assert_eq!(event, "activeTickerSymbols:changed");
                h.events.borrow_mut().push((parent, handler));
                Ok(())
            },
        }
    }
    fn super_private(&self, parent: V) -> Result<V, String> {
        socket(self, parent)
    }
    fn define(&self, r: &V, n: &'static str, v: V) -> Result<(), String> {
        set(r, n, v);
        Ok(())
    }
    fn undefined(&self) -> V {
        V::Undefined
    }
    fn boolean(&self, v: bool) -> V {
        V::Bool(v)
    }
    fn number(&self, v: f64) -> V {
        V::Number(v)
    }
    fn member(
        &self,
        kind: ClassKind,
        name: &'static str,
    ) -> Result<Option<MemberBinding<V, String>>, String> {
        if self.missing == Some((kind, name)) {
            return Ok(None);
        }
        let calls = self.calls.clone();
        let sends = self.sends.clone();
        let target = members(kind)
            .iter()
            .find(|m| m.name == name)
            .unwrap()
            .target;
        let body: NativeMember<V, String> = Rc::new(move |receiver, args| {
            calls
                .borrow_mut()
                .push((kind, name, receiver.clone(), args.clone()));
            if name == "isAlgoOrder" {
                let id = text(get(&args[0], "id")).encode_utf16().collect::<Vec<_>>();
                return Ok(ClassReturn::Value(V::Bool(order_routing::is_algo_order(
                    None, &id,
                ))));
            }
            if name == "ping" {
                let rt = WeexWebsocketControlRuntime {
                    performance_now: |_: &mut Rc<RefCell<Vec<Send>>>, _| Ok(42.0),
                    send_with_log: |h: &mut Rc<RefCell<Vec<Send>>>, state, method, _, id| {
                        assert_eq!(state.subscription_id, id + 1.0);
                        h.borrow_mut().push(Send {
                            method: method.into(),
                            id,
                        });
                        Ok(())
                    },
                    snapshot: |_, _, _| Err("unexercised snapshot fixture adapter".to_string()),
                    emit_reconnected: |_, _| {
                        Err("unexercised reconnect fixture adapter".to_string())
                    },
                    log_warning: |_, _, _| Err("unexercised warning fixture adapter".to_string()),
                };
                let receiver = obj(&receiver);
                let mut h = sends.clone();
                if kind == ClassKind::Public {
                    ws_control::public_ping(&mut h, &mut receiver.borrow_mut().control, &rt)?
                } else {
                    ws_control::private_ping(&mut h, &mut receiver.borrow_mut().control, &rt)?
                };
            }
            Ok(ClassReturn::Value(V::Undefined))
        });
        Ok(Some(MemberBinding {
            target: if self.wrong { "wrong" } else { target },
            body,
        }))
    }
    fn binding_error(&self, k: ClassKind, n: &'static str, r: &'static str) -> String {
        format!("{k:?}.{n}: {r}")
    }
    fn arrow(&self, _: &'static str, b: BoundMember<V, String>) -> Result<V, String> {
        Ok(V::Arrow(b))
    }
    fn prototype(
        &self,
        k: ClassKind,
        n: &'static str,
        b: NativeMember<V, String>,
    ) -> Result<(), String> {
        let mut prototypes = self.prototypes.borrow_mut();
        if !prototypes
            .iter()
            .any(|(kind, name, _)| (*kind, *name) == (k, n))
        {
            prototypes.push((k, n, b));
        }
        Ok(())
    }
}
fn invoke(v: &V, args: Vec<V>) -> V {
    let V::Arrow(f) = v else {
        panic!("native bound arrow")
    };
    match f(args).unwrap() {
        ClassReturn::Value(v) => v,
        ClassReturn::Future(_) => panic!("fixture sync value"),
    }
}
fn layout(
    kind: ClassKind,
    h: &Rc<Host>,
    r: &V,
    e: &ExpectedClass,
    floor: usize,
    arrow_floor: usize,
) {
    assert_eq!(names(r), e.names, "classes::native layout {:?}", kind);
    assert_eq!(
        names(r).len(),
        floor,
        "classes::independent own floor {:?}",
        kind
    );
    let actual = obj(r)
        .borrow()
        .fields
        .iter()
        .filter(|(_, v)| matches!(v, V::Arrow(_)))
        .map(|(n, _)| n.to_string())
        .collect::<Vec<_>>();
    assert_eq!(actual, e.arrows, "classes::native arrows {:?}", kind);
    assert_eq!(
        actual.len(),
        arrow_floor,
        "classes::independent arrows {:?}",
        kind
    );
    let proto = h
        .prototypes
        .borrow()
        .iter()
        .filter(|(k, _, _)| *k == kind)
        .map(|(_, n, _)| n.to_string())
        .collect::<Vec<_>>();
    assert_eq!(proto, e.prototype, "classes::native prototype {:?}", kind);
    for (field, kind, size) in &e.collections {
        let value = get(r, field);
        let (actual_kind, actual_size) = match value {
            V::Map(v) => ("Map", v.borrow().len()),
            V::Set(v) => ("Set", v.borrow().len()),
            V::Array(v) => ("Array", v.borrow().len()),
            V::Object(v) => ("Object", v.borrow().fields.len()),
            _ => panic!("native allocation kind"),
        };
        assert_eq!(
            (actual_kind, actual_size),
            (kind.as_str(), *size),
            "classes::native collection {}",
            field
        );
    }
    let store = get(r, "store");
    if kind != ClassKind::Exchange {
        assert!(same(&store, &get(&get(r, "parent"), "store")));
    }
    for name in &e.arrows {
        let arrow = get(r, name);
        let arg = object(vec![("id", V::Text("a__b".into()))]);
        invoke(&arrow, vec![arg.clone()]);
        let call = h.calls.borrow().last().unwrap().clone();
        assert_eq!(
            (call.0, call.1),
            (kind, name.as_str()),
            "classes::native dispatch {:?}",
            kind
        );
        assert!(
            same(&call.2, r),
            "classes::native retained receiver {:?}",
            kind
        );
        assert!(same(&call.3[0], &arg));
    }
    assert_eq!(
        h.calls.borrow().len(),
        arrow_floor,
        "classes::independent dispatch floor {:?}",
        kind
    );
}
#[test]
fn classes_exchange() {
    let expected = expected();
    assert_eq!(expected.classes.len(), 3);
    let h = Host::new();
    let store = object(vec![]);
    let options = object(vec![]);
    let c = WeexExchange::new(
        h.clone(),
        V::Text("WEEX".into()),
        options.clone(),
        store.clone(),
    )
    .unwrap();
    assert_eq!(expected.classes[0].name, "WeexExchange");
    assert!(same(&get(&c.receiver, "options"), &options));
    assert!(same(&get(&c.receiver, "store"), &store));
    assert_eq!(
        text(get(&c.receiver, "accountPositionMode")),
        "oneway",
        "classes::native exchange mode"
    );
    assert_eq!(
        *h.body_assignments.borrow(),
        ["xhr", "publicWebsocket", "privateWebsocket"]
    );
    for field in ["publicWebsocket", "privateWebsocket"] {
        assert!(same(&get(&get(&c.receiver, field), "parent"), &c.receiver));
    }
    layout(
        ClassKind::Exchange,
        &h,
        &c.receiver,
        &expected.classes[0],
        56,
        45,
    );
    let retained = get(&c.receiver, "isAlgoOrder");
    let receiver = c.receiver.clone();
    drop(c);
    let result = invoke(
        &retained,
        vec![object(vec![("id", V::Text("a__b".into()))])],
    );
    assert_eq!(
        boolean(result),
        expected.algo,
        "classes::native exchange body"
    );
    assert!(same(&h.calls.borrow().last().unwrap().2, &receiver));
    let second = WeexExchange::new(h.clone(), V::Text("WEEX".into()), options, store).unwrap();
    assert!(same(
        &get(&receiver, "grafanaEndpointsEvery"),
        &get(&second.receiver, "grafanaEndpointsEvery")
    ));
    assert!(!same(
        &get(&receiver, "symbolSettings"),
        &get(&second.receiver, "symbolSettings")
    ));
    assert!(!same(&retained, &get(&second.receiver, "isAlgoOrder")));
}
fn socket_case(kind: ClassKind, index: usize, floor: usize, arrows: usize) {
    let expected = expected();
    let h = Host::new();
    let parent = object(vec![("store", object(vec![]))]);
    let receiver = if kind == ClassKind::Public {
        WeexPublicWebsocket::new(h.clone(), parent.clone())
            .unwrap()
            .receiver
    } else {
        WeexPrivateWebsocket::new(h.clone(), parent.clone())
            .unwrap()
            .receiver
    };
    assert_eq!(
        number(get(&receiver, "subscriptionId")),
        1.0,
        "classes::native socket initial {:?}",
        kind
    );
    assert!(boolean(get(&receiver, "treatAnyMessageAsPong")));
    assert!(same(&get(&receiver, "parent"), &parent));
    if kind == ClassKind::Public {
        let events = h.events.borrow();
        assert_eq!(events.len(), 1);
        assert!(same(&events[0].0, &parent));
        assert!(same(
            &events[0].1,
            &get(&receiver, "handleActiveTickerSymbolsChanged")
        ));
    } else {
        for n in [
            "refreshOrdersTimeout",
            "refreshPositionsTimeout",
            "refreshBalanceTimeout",
        ] {
            assert!(
                matches!(get(&receiver, n), V::Undefined),
                "explicit timer undefined"
            );
        }
    }
    layout(kind, &h, &receiver, &expected.classes[index], floor, arrows);
    h.sends.borrow_mut().clear();
    set(&receiver, "subscriptionId", V::Number(1.0));
    let retained = get(&receiver, "ping");
    invoke(&retained, vec![]);
    invoke(&retained, vec![]);
    assert_eq!(
        number(get(&receiver, "subscriptionId")),
        expected.sockets[index - 1].id,
        "classes::native ping id {:?}",
        kind
    );
    assert_eq!(
        number(get(&receiver, "pingAt")),
        expected.sockets[index - 1].ping_at,
        "classes::native ping clock {:?}",
        kind
    );
    assert_eq!(
        *h.sends.borrow(),
        expected.sockets[index - 1].sent,
        "classes::native ping sends {:?}",
        kind
    );
    set(&receiver, "isDisposed", V::Bool(true));
    invoke(&retained, vec![]);
    assert_eq!(h.sends.borrow().len(), 2);
    if kind == ClassKind::Public {
        let (k, n, body) = h.prototypes.borrow()[0].clone();
        assert_eq!((k, n), (kind, "dispose"));
        let other = object(vec![]);
        body(other.clone(), vec![]).unwrap();
        assert!(same(&h.calls.borrow().last().unwrap().2, &other));
    }
    let second = if kind == ClassKind::Public {
        WeexPublicWebsocket::new(h.clone(), parent)
            .unwrap()
            .receiver
    } else {
        WeexPrivateWebsocket::new(h.clone(), parent)
            .unwrap()
            .receiver
    };
    assert!(!same(&retained, &get(&second, "ping")));
    if kind == ClassKind::Public {
        assert_eq!(
            h.prototypes.borrow().len(),
            1,
            "classes::native shared prototype"
        );
    }
    if kind == ClassKind::Public {
        for n in [
            "topics",
            "tickerTopics",
            "orderbooks",
            "messageHandlers",
            "subscriptionOperationTimestamps",
        ] {
            assert!(!same(&get(&receiver, n), &get(&second, n)));
        }
    }
}
#[test]
fn classes_public() {
    socket_case(ClassKind::Public, 1, 32, 20)
}
#[test]
fn classes_private() {
    socket_case(ClassKind::Private, 2, 24, 14)
}
#[test]
fn classes_binding_rejections() {
    let mut missing_cases = 0;
    for kind in [ClassKind::Exchange, ClassKind::Public, ClassKind::Private] {
        for member in members(kind) {
            missing_cases += 1;
            let mut h = Host::new();
            Rc::get_mut(&mut h).unwrap().missing = Some((kind, member.name));
            let result = match kind {
                ClassKind::Exchange => {
                    WeexExchange::new(h.clone(), V::Undefined, V::Undefined, V::Undefined)
                        .map(|_| ())
                }
                ClassKind::Public => WeexPublicWebsocket::new(h.clone(), V::Undefined).map(|_| ()),
                ClassKind::Private => {
                    WeexPrivateWebsocket::new(h.clone(), V::Undefined).map(|_| ())
                }
            };
            assert_eq!(
                result,
                Err(format!("{kind:?}.{}: missing native binding", member.name)),
                "classes::native missing binding"
            );
            assert_eq!(*h.bases.borrow(), 0);
            assert!(h.prototypes.borrow().is_empty());
        }
        let mut h = Host::new();
        Rc::get_mut(&mut h).unwrap().wrong = true;
        let result = match kind {
            ClassKind::Exchange => {
                WeexExchange::new(h.clone(), V::Undefined, V::Undefined, V::Undefined).map(|_| ())
            }
            ClassKind::Public => WeexPublicWebsocket::new(h.clone(), V::Undefined).map(|_| ()),
            ClassKind::Private => WeexPrivateWebsocket::new(h.clone(), V::Undefined).map(|_| ()),
        };
        assert!(
            result
                .as_ref()
                .is_err_and(|error| error.contains("native target mismatch")),
            "classes::native mismatched binding"
        );
        assert_eq!(*h.bases.borrow(), 0);
    }
    assert_eq!(
        missing_cases, 80,
        "classes::independent missing-binding floor"
    );
}
