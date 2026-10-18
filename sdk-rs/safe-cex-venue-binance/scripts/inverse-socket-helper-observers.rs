// Native observations of the unchanged production helpers. Only primitive IO is scripted.
#[path = "../tests/support/inverse_runtime.rs"]
pub mod host;
use crate::inverse::Rt;
use safe_cex_exchange::js_runtime::value::{Js, Thrown, from_json};
use serde_json::{Value, json};
use std::{cell::Cell, rc::Rc};
pub fn make(private: bool) -> (host::Harness, Rt) {
    let h = host::fixture(true, false);
    let socket = h
        .exchange
        .receiver()
        .get(if private {
            "privateWebsocket"
        } else {
            "publicWebsocket"
        })
        .unwrap();
    let rt = h.exchange.scope.with_this(socket);
    (h, rt)
}
pub fn describe(r: Result<Js, Thrown>, sentinel: &Js, error: &Js) -> Value {
    match r {
        Err(e) => {
            if e.value.strict_equals(error) {
                json!({"error":"original"})
            } else {
                json!({"error":e.value.get("name").unwrap().to_display()})
            }
        }
        Ok(v) => match v {
            Js::Undefined => json!({"kind":"undefined"}),
            Js::Null => json!({"kind":"null"}),
            Js::Bool(b) => json!({"bool":b}),
            Js::Number(n) => json!({"bits":format!("{:016x}",n.to_bits())}),
            Js::String(s) => json!({"units":s}),
            _ => {
                assert!(v.strict_equals(sentinel), "original object identity");
                json!({"kind":"identity"})
            }
        },
    }
}
pub fn raw(case: &str, sentinel: &Js) -> Js {
    match case {
        "undefined" => Js::Undefined,
        "null" => Js::Null,
        "false" => Js::Bool(false),
        "empty" => Js::str(""),
        "zero" => Js::Number(0.),
        "negative-zero" => Js::Number(-0.),
        "nan" => Js::Number(f64::NAN),
        "utf16" => Js::utf16(&[0xd800]),
        _ => sentinel.clone(),
    }
}
pub fn property(case: &str, optional: bool, f: impl Fn(&Js, &str) -> Result<Js, Thrown>) -> Value {
    let sentinel = Js::object(vec![]);
    let error = Js::object(vec![]);
    let owner = match case {
        "null-owner" => Js::Null,
        "undefined-owner" => Js::Undefined,
        _ => Js::object(vec![]),
    };
    match case {
        "missing" | "null-owner" | "undefined-owner" => {}
        "inherited" => {
            let Js::Object(data) = &owner else {
                unreachable!()
            };
            *data.proto.borrow_mut() = Some(Js::object(vec![("slot", sentinel.clone())]));
        }
        "throws" => {
            let e = error.clone();
            owner
                .define_accessor(
                    "slot",
                    Some(Rc::new(move |_| Err(Thrown::new(e.clone())))),
                    None,
                )
                .expect("fresh native receipt accessor target");
        }
        _ => {
            owner.define("slot", raw(case, &sentinel)).unwrap();
        }
    }
    let r = f(&owner, "slot");
    let _ = optional;
    describe(r, &sentinel, &error)
}
pub fn flag(case: &str, private: bool, f: impl Fn(&Rt, &str) -> Result<bool, Thrown>) -> Value {
    let (_h, rt) = make(private);
    let error = Js::object(vec![]);
    let sentinel = Js::object(vec![]);
    if case == "throws" {
        let e = error.clone();
        rt.this
            .define_accessor(
                "probe",
                Some(Rc::new(move |_| Err(Thrown::new(e.clone())))),
                None,
            )
            .expect("fresh native receipt accessor target");
    } else if case != "missing" {
        rt.this.define("probe", raw(case, &sentinel)).unwrap();
    }
    describe(f(&rt, "probe").map(Js::Bool), &sentinel, &error)
}
pub fn call(case: &str, f: impl Fn(&Js, &str, Vec<Js>) -> Result<Js, Thrown>) -> Value {
    let owner = Js::object(vec![]);
    let sentinel = Js::object(vec![]);
    let error = Js::object(vec![]);
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    if case == "getter-throws" {
        let e = error.clone();
        owner
            .define_accessor(
                "method",
                Some(Rc::new(move |_| Err(Thrown::new(e.clone())))),
                None,
            )
            .expect("fresh native receipt accessor target");
    } else if case == "not-callable" {
        owner.define("method", Js::Number(1.)).unwrap();
    } else if case != "missing" {
        let receiver = owner.clone();
        let argument = sentinel.clone();
        let e = error.clone();
        let output = raw(case, &sentinel);
        let fails = case == "throws";
        owner
            .define(
                "method",
                Js::function("primitive capability", move |this, args| {
                    assert!(this.strict_equals(&receiver));
                    assert_eq!(args.len(), 2);
                    assert!(args[0].strict_equals(&argument));
                    assert!(matches!(args[1], Js::Undefined));
                    count.set(count.get() + 1);
                    if fails {
                        Err(Thrown::new(e.clone()))
                    } else {
                        Ok(output.clone())
                    }
                }),
            )
            .unwrap();
    }
    let result = describe(
        f(&owner, "method", vec![sentinel.clone(), Js::Undefined]),
        &sentinel,
        &error,
    );
    json!({"result":result,"calls":calls.get()})
}
pub fn global(
    case: &str,
    private: bool,
    f: impl Fn(&Rt, &str, Vec<Js>) -> Result<Js, Thrown>,
) -> Value {
    let (_h, rt) = make(private);
    let sentinel = Js::object(vec![]);
    let error = Js::object(vec![]);
    if case == "missing" {
        return describe(f(&rt, "MissingSocketImport", vec![]), &sentinel, &error);
    }
    let module = Js::object(vec![]);
    let owner = module.clone();
    let value = sentinel.clone();
    let output = raw(case, &sentinel);
    let e = error.clone();
    let fail = case == "throws";
    module
        .define(
            "method",
            Js::function("actual global port", move |this, args| {
                assert!(this.strict_equals(&owner), "dotted import receiver");
                assert_eq!(args.len(), 1);
                assert!(args[0].strict_equals(&value));
                if fail {
                    Err(Thrown::new(e.clone()))
                } else {
                    Ok(output.clone())
                }
            }),
        )
        .unwrap();
    rt.shared
        .globals
        .define("SocketAcceptedImport", module)
        .unwrap();
    describe(
        f(&rt, "SocketAcceptedImport.method", vec![sentinel.clone()]),
        &sentinel,
        &error,
    )
}
pub fn send(case: &str, f: impl Fn(&Rt, &str, Js) -> Result<(), Thrown>) -> Value {
    let (_h, rt) = make(true);
    let error = Js::object(vec![]);
    let e = error.clone();
    let seen = Rc::new(std::cell::RefCell::new(Vec::new()));
    let capture = seen.clone();
    let receiver = rt.this.clone();
    let fail = case == "throws";
    rt.this
        .define(
            "capture",
            Js::function("terminal send", move |this, args| {
                assert!(this.strict_equals(&receiver));
                assert_eq!(args.len(), 1);
                capture.borrow_mut().push(args[0].clone());
                if fail {
                    Err(Thrown::new(e.clone()))
                } else {
                    Ok(Js::Undefined)
                }
            }),
        )
        .unwrap();
    let value = match case {
        "undefined" => Js::Undefined,
        "utf16" => Js::utf16(&[0xd800]),
        "cycle" => {
            let o = Js::object(vec![]);
            o.define("self", o.clone()).unwrap();
            o
        }
        _ => Js::object(vec![
            ("zero", Js::Number(-0.)),
            ("nan", Js::Number(f64::NAN)),
        ]),
    };
    let result = describe(
        f(&rt, "capture", value).map(|()| Js::Undefined),
        &Js::Null,
        &error,
    );
    let arguments: Vec<_> = seen
        .borrow()
        .iter()
        .map(|v| describe(Ok(v.clone()), &Js::Null, &error))
        .collect();
    json!({"result":result,"arguments":arguments})
}
pub fn timer(case: &str, f: impl Fn(&Rt, &'static str, f64, bool) -> Result<Js, Thrown>) -> Value {
    let (h, rt) = make(true);
    let receiver = Js::object(vec![
        (
            "isDisposed",
            Js::Bool(case == "disposed" || case == "unguarded"),
        ),
        (
            "isForceClosed",
            Js::Bool(case == "closed" || case == "unguarded"),
        ),
    ]);
    let rt = rt.with_this(receiver.clone());
    let calls = Rc::new(Cell::new(0));
    let counter = calls.clone();
    let expected = receiver.clone();
    receiver
        .define(
            "capture",
            Js::function("terminal timer", move |this, args| {
                assert!(this.strict_equals(&expected));
                assert!(args.is_empty());
                counter.set(counter.get() + 1);
                Ok(Js::Undefined)
            }),
        )
        .unwrap();
    let handle = f(&rt, "capture", 250., case != "unguarded").unwrap();
    assert!(matches!(&handle,Js::Object(o) if o.class=="Timeout"));
    h.runtime.pump();
    h.runtime.now.set(h.runtime.now.get() + 249.);
    h.runtime.pump();
    let before = calls.get();
    if case == "cancel" {
        rt.clear_timeout(&handle);
    }
    h.runtime.now.set(h.runtime.now.get() + 1.);
    h.runtime.pump();
    json!({"before":before,"after":calls.get()})
}
pub fn subscription(case: &str, f: impl Fn(&Rt, &str, Js) -> Result<(), Thrown>) -> Value {
    let (_h, rt) = make(false);
    let seen = Rc::new(std::cell::RefCell::new(Vec::new()));
    let capture = seen.clone();
    let receiver = rt.this.clone();
    let error = Js::object(vec![]);
    let e = error.clone();
    let fails = case == "throws";
    rt.this
        .define(
            "send",
            Js::function("terminal wire", move |this, args| {
                assert!(this.strict_equals(&receiver));
                capture.borrow_mut().push(args[0].clone());
                if fails {
                    Err(Thrown::new(e.clone()))
                } else {
                    Ok(Js::Undefined)
                }
            }),
        )
        .unwrap();
    rt.this
        .set(
            "nextSubscriptionId",
            Js::Number(if case == "zero" { -0. } else { 2. }),
        )
        .unwrap();
    let method = if case == "unsubscribe" {
        "UNSUBSCRIBE"
    } else {
        "SUBSCRIBE"
    };
    let topic = if case == "utf16" {
        Js::utf16(&[0xd800])
    } else {
        Js::str("btcusd_perp@depth")
    };
    let result = describe(
        f(&rt, method, topic).map(|()| Js::Undefined),
        &Js::Null,
        &error,
    );
    let wires: Vec<_> = seen.borrow().iter().map(|v| v.to_display()).collect();
    json!({"result":result,"idBits":format!("{:016x}",rt.this.get("nextSubscriptionId").unwrap().to_number().unwrap().to_bits()),"wires":wires})
}
pub fn tickers(
    case: &str,
    f: impl Fn(&Rt, Js, &[(&str, &str)], bool) -> Result<(), Thrown>,
) -> Value {
    let (_h, rt) = make(false);
    let ticker = Js::object(vec![]);
    let state = Js::object(vec![("BTC", ticker.clone())]);
    let writes = Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = writes.clone();
    let original = ticker.clone();
    let error = Js::object(vec![]);
    let e = error.clone();
    let fails = case == "throws";
    // This is the Store capability boundary; no ticker projection or helper is replaced.
    let store = Js::object(vec![("tickersBySymbol", state)]);
    let receiver = store.clone();
    store
        .define(
            "updateTicker",
            Js::function("terminal Store", move |this, args| {
                assert!(this.strict_equals(&receiver));
                assert!(args[0].strict_equals(&original));
                seen.borrow_mut().push(args[1].clone());
                if fails {
                    Err(Thrown::new(e.clone()))
                } else {
                    Ok(Js::Undefined)
                }
            }),
        )
        .unwrap();
    let parent = Js::object(vec![("store", store.clone())]);
    let this = Js::object(vec![("parent", parent), ("store", store)]);
    let rt = rt.with_this(this);
    let row = match case {
        "unknown" => json!({"s":"ABSENT","c":"7"}),
        "quote-fallback" => json!({"s":"BTC","c":"-0","q":"","Q":"12.5"}),
        "quote-zero" => json!({"s":"BTC","c":"Infinity"}),
        _ => json!({"s":"BTC","c":"not-numeric","q":"4"}),
    };
    let result = describe(
        f(
            &rt,
            Js::array(vec![from_json(&row)]),
            &[("last", "c")],
            case != "no-quote",
        )
        .map(|()| Js::Undefined),
        &Js::Null,
        &error,
    );
    let patches:Vec<_>=writes.borrow().iter().map(|p|{let n=p.get("last").unwrap().to_number().unwrap();let q=p.get("quoteVolume").unwrap();json!({"lastBits":if n.is_nan(){"NaN".into()}else{format!("{:016x}",n.to_bits())},"quote":describe(Ok(q),&Js::Null,&error)})}).collect();
    json!({"result":result,"patches":patches})
}
