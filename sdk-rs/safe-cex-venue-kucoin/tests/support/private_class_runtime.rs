use crate::values::*;
use safe_cex_venue_kucoin::{
    order_control::OrderControlOps, order_lifecycle::OrderLifecycleOps, public_streams::StreamOps,
    spot_private_class::*,
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
    task::Poll,
};
type Getter = Rc<dyn Fn() -> Result<V, E>>;
type Setter = Rc<dyn Fn(V) -> Result<(), E>>;
#[derive(Default)]
pub(crate) struct Runtime {
    pub(crate) expected_class: RefCell<String>,
    pub(crate) globals: RefCell<BTreeMap<String, V>>,
    pub(crate) jobs: RefCell<Vec<V>>,
    pub(crate) getters: RefCell<Vec<(V, String, Getter)>>,
    pub(crate) setters: RefCell<Vec<(V, String, Setter)>>,
    pub(crate) metas: RefCell<Vec<(V, String, usize, bool)>>,
    pub(crate) constructors: RefCell<Vec<(V, Initializer<V, E>)>>,
    pub(crate) prototypes: RefCell<Vec<(V, V)>>,
    pub(crate) prototype_parents: RefCell<Vec<(V, V)>>,
    pub(crate) object_prototypes: RefCell<Vec<(V, V)>>,
    pub(crate) stringified: RefCell<Vec<V>>,
    pub(crate) definitions: RefCell<Vec<(V, String)>>,
    pub(crate) trace: RefCell<Vec<String>>,
    pub(crate) timers: RefCell<Vec<(f64, V)>>,
    pub(crate) next_timer: Cell<u32>,
}
impl Runtime {
    pub(crate) fn read(&self, r: &V, k: &str) -> Result<V, E> {
        let special = self
            .getters
            .borrow()
            .iter()
            .find(|(o, key, _)| Rc::ptr_eq(r, o) && key == k)
            .map(|(_, _, f)| f.clone());
        if let Some(f) = special {
            return f();
        }
        let value = get(r, k)?;
        let own =
            matches!(r.as_ref(), Js::O(fields) if fields.borrow().iter().any(|(key, _)| key == k));
        if !own {
            let parent = self
                .object_prototypes
                .borrow()
                .iter()
                .find(|(object, _)| Rc::ptr_eq(object, r))
                .map(|(_, parent)| parent.clone());
            if let Some(parent) = parent {
                if k == "__proto__" && !matches!(parent.as_ref(), Js::Null) {
                    return Ok(parent);
                }
                if !matches!(parent.as_ref(), Js::Null) {
                    return self.read(&parent, k);
                }
            }
        }
        if matches!(value.as_ref(), Js::U) && ["store", "emitter"].contains(&k) {
            let parent = get(r, "parent")?;
            if !matches!(parent.as_ref(), Js::U) {
                return get(&parent, k);
            }
        }
        Ok(value)
    }
    pub(crate) fn write(&self, r: &V, k: &str, v: V) -> Result<(), E> {
        let special = self
            .setters
            .borrow()
            .iter()
            .find(|(o, key, _)| Rc::ptr_eq(r, o) && key == k)
            .map(|(_, _, f)| f.clone());
        if let Some(f) = special {
            return f(v);
        }
        if k == "__proto__"
            && matches!(r.as_ref(), Js::O(fields) if !fields.borrow().iter().any(|(key, _)| key == k))
        {
            let mut current = r.clone();
            let setter = loop {
                let parent = self
                    .object_prototypes
                    .borrow()
                    .iter()
                    .find(|(object, _)| Rc::ptr_eq(object, &current))
                    .map(|(_, parent)| parent.clone());
                match parent {
                    None => break true,
                    Some(parent) if matches!(parent.as_ref(), Js::Null) => break false,
                    Some(parent) => {
                        if matches!(parent.as_ref(), Js::O(fields) if fields.borrow().iter().any(|(key, _)| key == k))
                        {
                            break false;
                        }
                        current = parent;
                    }
                }
            };
            if setter {
                if matches!(v.as_ref(), Js::O(_) | Js::A(_) | Js::F(_) | Js::Null) {
                    let mut current = Some(v.clone());
                    while let Some(parent) = current {
                        if Rc::ptr_eq(&parent, r) {
                            return Err(Rc::new("Cyclic __proto__ value".into()));
                        }
                        current = self
                            .object_prototypes
                            .borrow()
                            .iter()
                            .find(|(object, _)| Rc::ptr_eq(object, &parent))
                            .map(|(_, parent)| parent.clone());
                    }
                    let mut prototypes = self.object_prototypes.borrow_mut();
                    if let Some((_, parent)) = prototypes
                        .iter_mut()
                        .find(|(object, _)| Rc::ptr_eq(object, r))
                    {
                        *parent = v;
                    } else {
                        prototypes.push((r.clone(), v));
                    }
                }
                return Ok(());
            }
        }
        set(r, k, v);
        Ok(())
    }
    pub(crate) fn capture(self: &Rc<Self>, r: &V, path: &str) -> Result<V, E> {
        let parts = path.split('.').collect::<Vec<_>>();
        let mut receiver = u();
        let mut result = if parts[0] == "this" {
            r.clone()
        } else {
            self.globals
                .borrow()
                .get(parts[0])
                .unwrap_or_else(|| panic!("unbound global {path}"))
                .clone()
        };
        for key in &parts[1..] {
            receiver = result.clone();
            result = self.read(&result, key)?;
        }
        if parts.len() > 1 && matches!(result.as_ref(), Js::F(_)) {
            let owner = receiver;
            let callee = result;
            Ok(function(move |_, a| call(callee.clone(), owner.clone(), a)))
        } else {
            Ok(result)
        }
    }
    pub(crate) fn promise(&self, task: Task<V, E>) -> V {
        let value = Rc::new(Js::P(Rc::new(Promise {
            task: RefCell::new(Some(task)),
            result: RefCell::new(None),
        })));
        self.jobs.borrow_mut().push(value.clone());
        match poll(&value) {
            Poll::Ready(Err(error)) => panic!("fixture unhandled rejection: {error}"),
            Poll::Ready(Ok(_)) | Poll::Pending => {}
        };
        value
    }
    pub(crate) fn pump(&self) {
        for _ in 0..16 {
            let jobs = self.jobs.borrow().clone();
            for job in jobs {
                match poll(&job) {
                    Poll::Ready(Err(error)) => panic!("fixture unhandled rejection: {error}"),
                    Poll::Ready(Ok(_)) | Poll::Pending => {}
                }
            }
        }
    }
    pub(crate) fn frame(self: &Rc<Self>, r: &V) -> Rc<StreamOps<V, E>> {
        let reads = self.clone();
        let writes = self.clone();
        let captures = self.clone();
        let receiver = r.clone();
        let intrinsic = self.clone();
        let callbacks = self.clone();
        let spawns = self.clone();
        Rc::new(StreamOps {
            get: Rc::new(move |v, k| reads.read(v, k)),
            set: Rc::new(move |v, k, n| writes.write(v, k, n)),
            capture: Rc::new(move |p| captures.capture(&receiver, p)),
            invoke: Rc::new(|f, a| call(f, u(), a)),
            intrinsic: Rc::new(move |name, a| match name {
                "call-method" => {
                    let f = intrinsic.read(&a[0], &text(&a[1]))?;
                    call(f, a[0].clone(), a[2..].to_vec())
                }
                "call-with-this" => {
                    let Js::A(args) = a[2].as_ref() else {
                        panic!("call arguments")
                    };
                    call(a[0].clone(), a[1].clone(), args.borrow().clone())
                }
                "get-index" => intrinsic.read(&a[0], &text(&a[1])),
                "set-index" => {
                    intrinsic.write(&a[0], &text(&a[1]), a[2].clone())?;
                    Ok(a[2].clone())
                }
                "delete-index" => {
                    let Js::O(fields) = a[0].as_ref() else {
                        panic!("delete object")
                    };
                    fields.borrow_mut().retain(|(key, _)| key != &text(&a[1]));
                    Ok(b(true))
                }
                "spread-append" => {
                    let Js::A(rows) = a[0].as_ref() else {
                        panic!("array spread")
                    };
                    let mut result = rows.borrow().clone();
                    result.push(a[1].clone());
                    Ok(arr(result))
                }
                "destructure-skip-first-take-second" => intrinsic.read(&a[0], "1"),
                "destructure-two" => {
                    let Js::A(rows) = a[0].as_ref() else {
                        panic!("pair iterator")
                    };
                    Ok(arr((0..2)
                        .map(|i| rows.borrow().get(i).cloned().unwrap_or_else(u))
                        .collect()))
                }
                "template-string" => Ok(s(&text(&a[0]))),
                "template-split-colon-index-one" => {
                    Ok(text(&a[0]).split(':').nth(1).map(s).unwrap_or_else(u))
                }
                "candle-topic" => Ok(s(&format!("candle_{}_{}", text(&a[0]), text(&a[1])))),
                "spot-candle-topic" => Ok(s(&format!(
                    "/market/candles:{}_{}",
                    text(&a[0]),
                    text(&a[1])
                ))),
                "book-topic" => Ok(s(&format!("orderBook_{}", text(&a[0])))),
                "spot-book-topic" => Ok(s(&format!("/spotMarket/level2Depth50:{}", text(&a[0])))),
                "Array.isArray" => Ok(b(matches!(a[0].as_ref(), Js::A(_)))),
                "Object.keys-length" => {
                    let Js::O(fields) = a[0].as_ref() else {
                        panic!("keys object")
                    };
                    Ok(n(fields.borrow().len() as f64))
                }
                "Number.isFinite" => Ok(b(matches!(a[0].as_ref(),Js::N(v) if v.is_finite()))),
                "multiply-100" => Ok(n(number(&a[0]) * 100.)),
                "true" => Ok(b(true)),
                "false" => Ok(b(false)),
                "remainder" => Ok(n(number(&a[0]) % number(&a[1]))),
                "less-than" => Ok(b(number(&a[0]) < number(&a[1]))),
                "less-equal" => Ok(b(number(&a[0]) <= number(&a[1]))),
                "greater-than" => Ok(b(number(&a[0]) > number(&a[1]))),
                "greater-last-plus-one" => Ok(b(number(&a[0]) > number(&a[1]) + 1.)),
                "typeof-string" => Ok(b(matches!(a[0].as_ref(), Js::S(_)))),
                "split-colon-index-one" => {
                    Ok(text(&a[0]).split(':').nth(1).map(s).unwrap_or_else(u))
                }
                "split-comma" => Ok(arr(text(&a[0]).split(',').map(s).collect())),
                "futures-candle-topic" => Ok(s(&format!(
                    "/contractMarket/limitCandle:{}_{}",
                    text(&a[0]),
                    text(&a[1])
                ))),
                "futures-book-topic" => Ok(s(&format!("/contractMarket/level2:{}", text(&a[0])))),
                "snapshot-error" => Ok(s(&format!(
                    "Failed to fetch orderbook snapshot for {}",
                    text(&a[0])
                ))),
                "book-error" => Ok(s(&format!("Failed to fetch orderbook {}", text(&a[0])))),
                "array-push" => {
                    let Js::A(rows) = a[0].as_ref() else {
                        panic!("push")
                    };
                    rows.borrow_mut().push(a[1].clone());
                    Ok(u())
                }
                "array-spread" => {
                    let Js::A(rows) = a[0].as_ref() else {
                        panic!("spread")
                    };
                    Ok(arr(rows.borrow().clone()))
                }
                "null" => Ok(Rc::new(Js::Null)),
                "divide-1000" => Ok(n(number(&a[0]) / 1000.)),
                "greater-zero" => Ok(b(number(&a[0]) > 0.)),
                "multiply" => Ok(n(number(&a[0]) * number(&a[1]))),
                "to-property-key" => Ok(s(&text(&a[0]))),
                "define-field" => {
                    set(&a[0], &text(&a[1]), a[2].clone());
                    Ok(a[0].clone())
                }
                "spread-into" => {
                    let Js::O(fields) = a[1].as_ref() else {
                        panic!("unexercised spread input")
                    };
                    for (k, v) in fields.borrow().iter() {
                        set(&a[0], k, v.clone());
                    }
                    Ok(a[0].clone())
                }
                _ => panic!("unexercised intrinsic {name}"),
            }),
            truthy: Rc::new(truthy),
            nullish: Rc::new(|v| matches!(v.as_ref(), Js::U | Js::Null)),
            equal: Rc::new(|a, b| match (a.as_ref(), b.as_ref()) {
                (Js::U, Js::U) | (Js::Null, Js::Null) => true,
                (Js::B(a), Js::B(b)) => a == b,
                (Js::S(a), Js::S(b)) => a == b,
                (Js::N(a), Js::N(b)) => a == b,
                _ => Rc::ptr_eq(a, b),
            }),
            literal: Rc::new(s),
            number: Rc::new(n),
            undefined: Rc::new(u),
            array: Rc::new(arr),
            object: Rc::new(obj),
            callback: Rc::new(move |name, arity, body| {
                let f = function(move |_, args| {
                    body(args.first().cloned().unwrap_or_else(u))?;
                    Ok(u())
                });
                callbacks
                    .metas
                    .borrow_mut()
                    .push((f.clone(), name.into(), arity, false));
                f
            }),
            map: Rc::new(|input, body| {
                let Js::A(rows) = input.as_ref() else {
                    panic!("map array")
                };
                let length = rows.borrow().len();
                let mut out = Vec::new();
                for i in 0..length {
                    let value = rows.borrow().get(i).cloned();
                    if let Some(value) = value {
                        out.push(body(value)?);
                    }
                }
                Ok(arr(out))
            }),
            filter: Rc::new(|input, body| {
                let Js::A(rows) = input.as_ref() else {
                    panic!("filter array")
                };
                let length = rows.borrow().len();
                let mut out = Vec::new();
                for i in 0..length {
                    let value = rows.borrow().get(i).cloned();
                    if let Some(value) = value {
                        if body(value.clone())? {
                            out.push(value);
                        }
                    }
                }
                Ok(arr(out))
            }),
            for_of: Rc::new(|input, f| {
                let Js::A(rows) = input.as_ref() else {
                    panic!("array iterator")
                };
                let mut index = 0;
                loop {
                    let row = rows.borrow().get(index).cloned();
                    let Some(row) = row else { break };
                    f(row)?;
                    index += 1;
                }
                Ok(())
            }),
            await_value: Rc::new(wait),
            spawn: Rc::new(move |task| {
                spawns.promise(Box::pin(async move {
                    task.await?;
                    Ok(u())
                }));
            }),
            error_value: Rc::new(|error| obj(vec![("message", s(error))])),
        })
    }
    pub(crate) fn arrow(
        self: &Rc<Self>,
        name: &str,
        arity: usize,
        asynchronous: bool,
        body: Rc<dyn Fn(Vec<V>) -> Task<V, E>>,
    ) -> V {
        let jobs = self.clone();
        let f = function(move |_, a| Ok(jobs.promise(body(a))));
        self.metas
            .borrow_mut()
            .push((f.clone(), name.into(), arity, asynchronous));
        f
    }
    pub(crate) fn operations(self: &Rc<Self>) -> Rc<PrivateClassOps<V, E>> {
        let frames = self.clone();
        let orders = self.clone();
        let derivation = self.clone();
        let definitions = self.clone();
        let arrows = self.clone();
        let asynchronous = self.clone();
        let methods = self.clone();
        let prototypes = self.clone();
        let supers = self.clone();
        Rc::new(PrivateClassOps {
            value: self.frame(&u()),
            frame: Rc::new(move |r| frames.frame(r)),
            orders: Rc::new(move |r| {
                let frame = orders.frame(r);
                let members = orders.clone();
                Rc::new(OrderLifecycleOps {
                    base: Rc::new(OrderControlOps {
                        value: frame,
                        pair: Rc::new(|_| panic!("unexercised pair")),
                        close_throw: Rc::new(|_, _| panic!("unexercised close")),
                        has: Rc::new(|_, _| panic!("unexercised has")),
                        boolean: Rc::new(b),
                        new_error: Rc::new(|_, _| panic!("unexercised newerror")),
                        iterator: Rc::new(|_| panic!("unexercised iterator")),
                        next: Rc::new(|_| panic!("unexercised next")),
                        some: Rc::new(|_, _| panic!("unexercised some")),
                    }),
                    member: Rc::new(move |v, key| {
                        let member = members.read(&v, key)?;
                        Ok(function(move |_, args| {
                            call(member.clone(), v.clone(), args)
                        }))
                    }),
                    mapper: Rc::new(|body| {
                        function(move |_, args| body(args.first().cloned().unwrap_or_else(u)))
                    }),
                })
            }),
            derive: Rc::new(move |name, base, init| {
                assert_eq!(name, derivation.expected_class.borrow().as_str());
                assert!(Rc::ptr_eq(
                    &base,
                    derivation.globals.borrow().get("BaseWebSocket").unwrap()
                ));
                let parent = get(&base, "prototype")?;
                let class = obj(vec![("base", base)]);
                let proto = obj(vec![]);
                derivation
                    .prototype_parents
                    .borrow_mut()
                    .push((proto.clone(), parent));
                derivation
                    .prototypes
                    .borrow_mut()
                    .push((class.clone(), proto));
                derivation
                    .constructors
                    .borrow_mut()
                    .push((class.clone(), init));
                Ok(class)
            }),
            define: Rc::new(move |r, k, v| {
                definitions
                    .definitions
                    .borrow_mut()
                    .push((r.clone(), k.into()));
                definitions.write(r, k, v)
            }),
            arrow: Rc::new(move |name, arity, body| {
                let f = function(move |_, a| body(a));
                arrows
                    .metas
                    .borrow_mut()
                    .push((f.clone(), name.into(), arity, false));
                f
            }),
            async_arrow: Rc::new(move |name, arity, body| {
                asynchronous.arrow(name, arity, true, body)
            }),
            method: Rc::new(move |name, arity, body| {
                let f = function(move |r, a| body(r, a));
                methods
                    .metas
                    .borrow_mut()
                    .push((f.clone(), name.into(), arity, false));
                f
            }),
            define_prototype: Rc::new(move |class, name, body| {
                let proto = prototypes
                    .prototypes
                    .borrow()
                    .iter()
                    .find(|(c, _)| Rc::ptr_eq(c, class))
                    .unwrap()
                    .1
                    .clone();
                set(&proto, name, body);
                Ok(())
            }),
            super_member: Rc::new(move |home, r, key| {
                assert!(
                    Rc::ptr_eq(home, &supers.constructors.borrow().first().unwrap().0),
                    "KUCOIN-BOUND exact home class"
                );
                let proto = supers
                    .prototypes
                    .borrow()
                    .iter()
                    .find(|(c, _)| Rc::ptr_eq(c, home))
                    .unwrap()
                    .1
                    .clone();
                let parent = supers
                    .prototype_parents
                    .borrow()
                    .iter()
                    .find(|(p, _)| Rc::ptr_eq(p, &proto))
                    .unwrap()
                    .1
                    .clone();
                let member = supers.read(&parent, key)?;
                let receiver = r.clone();
                Ok(function(move |_, a| {
                    call(member.clone(), receiver.clone(), a)
                }))
            }),
            template: Rc::new(|v| Ok(text(v).encode_utf16().collect())),
            string_value: Rc::new(|v| s(&String::from_utf16(&v).expect("finite scalar fixture"))),
            boolean: Rc::new(b),
            null: Rc::new(|| Rc::new(Js::Null)),
            error: Rc::new(|m| Rc::new(m.into())),
        })
    }
    // Explicit-constructor targets share this fixture without its implicit helper.
    #[allow(dead_code)]
    pub(crate) fn construct(self: &Rc<Self>, class: &V, parent: V) -> V {
        let r = obj(vec![("parent", parent), ("isDisposed", b(false))]);
        let initializer = self
            .constructors
            .borrow()
            .iter()
            .find(|(c, _)| Rc::ptr_eq(c, class))
            .unwrap()
            .1
            .clone();
        initializer(r.clone()).unwrap();
        r
    }
    pub(crate) fn method(&self, r: &V, key: &str, args: Vec<V>) -> Result<V, E> {
        call(self.read(r, key)?, r.clone(), args)
    }
}
