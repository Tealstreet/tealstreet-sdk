//! The actual Hyro reset body over the shared runtime's collection values.
use safe_cex_exchange::js_runtime::{
    scope::bound_member,
    value::{Js, Thrown, error_object},
};
use safe_cex_venue_bybit::hyro_private_session::{self as session, SessionFuture, SessionHost};
use std::{cell::RefCell, rc::Rc};

struct Host(Js);
impl SessionHost for Host {
    type Value = Js;
    type Error = Thrown;
    fn get(&self, owner: &Js, key: &str) -> Result<Js, Thrown> {
        owner.get(key)
    }
    fn optional(&self, owner: &Js, key: &str) -> Result<Js, Thrown> {
        if owner.is_nullish() {
            Ok(Js::Undefined)
        } else {
            owner.get(key)
        }
    }
    fn set(&self, owner: &Js, key: &str, value: Js) -> Result<(), Thrown> {
        owner.set(key, value)
    }
    fn define(&self, owner: &Js, key: &str, value: Js) -> Result<(), Thrown> {
        owner.define(key, value)
    }
    fn truthy(&self, value: &Js) -> bool {
        value.truthy()
    }
    fn equal(&self, a: &Js, b: &Js) -> bool {
        a.strict_equals(b)
    }
    fn undefined(&self) -> Js {
        Js::Undefined
    }
    fn literal(&self, value: &str) -> Js {
        Js::str(value)
    }
    fn boolean(&self, value: bool) -> Js {
        Js::Bool(value)
    }
    fn number(&self, value: f64) -> Js {
        Js::Number(value)
    }
    fn numeric(&self, value: &Js) -> Result<f64, Thrown> {
        value.to_number()
    }
    fn units(&self, value: &Js) -> Result<Vec<u16>, Thrown> {
        Ok(value.to_js_string()?.to_vec())
    }
    fn text(&self, units: Vec<u16>) -> Js {
        Js::utf16(&units)
    }
    fn object(&self, fields: Vec<(&'static str, Js)>) -> Js {
        Js::object(fields)
    }
    fn array(&self, values: Vec<Js>) -> Js {
        Js::array(values)
    }
    fn global(&self) -> Js {
        self.0.clone()
    }
    fn select(&self, owner: Js, key: &str) -> Result<Js, Thrown> {
        bound_member(&owner, key)
    }
    fn receiver_call(&self, owner: Js, callable: Js, args: Vec<Js>) -> Result<Js, Thrown> {
        callable.call(owner, args)
    }
    fn apply(&self, callable: Js, args: Vec<Js>) -> Result<Js, Thrown> {
        callable.call(Js::Undefined, args)
    }
    fn wait(&self, _: Js) -> SessionFuture<'_, Js, Thrown> {
        panic!("resetSession must not await")
    }
    fn intrinsic(&self, name: &str, _: Vec<Js>) -> Result<Js, Thrown> {
        assert_eq!(name, "null");
        Ok(Js::Null)
    }
    fn allocate(&self, _: Js) -> Result<Js, Thrown> {
        panic!("resetSession must not allocate a session")
    }
    fn callback(&self, _: Js, _: &'static str, _: Vec<Js>) -> Result<Js, Thrown> {
        panic!("resetSession must not create callbacks")
    }
    fn promise(&self) -> Result<(Js, Js, Js), Thrown> {
        panic!("resetSession must not create promises")
    }
    fn error(&self, message: Js) -> Result<Thrown, Thrown> {
        Ok(Thrown::new(error_object("Error", &message.to_display())))
    }
    fn error_value(&self, error: Thrown) -> Js {
        error.value
    }
    fn normalize_error(&self, error: Thrown) -> Result<Js, Thrown> {
        Ok(error.value)
    }
}

type Trace = Rc<RefCell<Vec<String>>>;
fn pending(trace: &Trace, label: &'static str, timer: f64, late: Option<(Js, Js)>) -> Js {
    let trace = trace.clone();
    Js::object(vec![
        ("timer", Js::Number(timer)),
        (
            "reject",
            Js::function("reject", move |_, args| {
                trace.borrow_mut().push(format!(
                    "reject:{label}:{}",
                    args[0].get("message")?.to_display()
                ));
                if let Some((map, pending)) = &late {
                    map.call_method("set", vec![Js::str("late"), pending.clone()])?;
                }
                Ok(Js::Undefined)
            }),
        ),
    ])
}

#[test]
fn hyro_reset_session_uses_shared_live_pending_ack_iterator() {
    let output = std::process::Command::new("node")
        .arg("tests/hyro_native_iterator_oracle.cjs")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Node source oracle is required");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<Vec<String>> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(expected.len(), 2);
    for (case, live_addition) in [false, true].into_iter().enumerate() {
        let trace: Trace = Rc::default();
        let cleared = trace.clone();
        let host = Host(Js::object(vec![(
            "clearTimeout",
            Js::function("clearTimeout", move |_, args| {
                cleared
                    .borrow_mut()
                    .push(format!("clear:{}", args[0].to_display()));
                Ok(Js::Undefined)
            }),
        )]));
        let acks = Js::new_map();
        let late = live_addition.then(|| (acks.clone(), pending(&trace, "late", 33., None)));
        acks.call_method(
            "set",
            vec![Js::str("auth"), pending(&trace, "auth", 11., late)],
        )
        .unwrap();
        acks.call_method(
            "set",
            vec![
                Js::str("subscribe"),
                pending(&trace, "subscribe", 22., None),
            ],
        )
        .unwrap();
        let owner = Js::object(vec![
            ("generation", Js::Number(4.)),
            ("stateRevision", Js::Number(9.)),
            ("ready", Js::Bool(true)),
            ("lastReceived", Js::Number(77.)),
            ("pendingAcks", acks.clone()),
        ]);
        let Js::Object(data) = &owner else {
            unreachable!()
        };
        let weak = Rc::downgrade(data);
        let health = trace.clone();
        owner
            .define(
                "parent",
                Js::object(vec![(
                    "syncSocketHealthToStore",
                    Js::function("syncSocketHealthToStore", move |_, _| {
                        let owner = Js::Object(weak.upgrade().expect("live source receiver"));
                        health.borrow_mut().push(format!(
                            "health:{}",
                            owner.get("lastReceived")?.to_display()
                        ));
                        Ok(Js::Undefined)
                    }),
                )]),
            )
            .unwrap();
        session::reset_session(&host, &owner)
            .expect("source pendingAcks.values().next() must execute over shared native iterators");
        assert_eq!(owner.get("generation").unwrap().as_f64(), Some(5.));
        assert_eq!(owner.get("stateRevision").unwrap().as_f64(), Some(10.));
        assert!(!owner.get("ready").unwrap().truthy());
        assert_eq!(
            owner
                .get("lastReceived")
                .unwrap()
                .as_f64()
                .unwrap()
                .to_bits(),
            0
        );
        assert_eq!(acks.get("size").unwrap().as_f64(), Some(0.));
        assert_eq!(
            *trace.borrow(),
            expected[case],
            "actual source reset trace / live_addition={live_addition}"
        );
    }
}
