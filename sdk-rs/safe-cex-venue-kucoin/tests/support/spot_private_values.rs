#![allow(dead_code)]
use safe_cex_venue_kucoin::spot_private_class::Task;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    rc::Rc,
    task::{Context, Poll, Waker},
};
#[derive(Clone)]
pub(crate) enum Js {
    U,
    Null,
    B(bool),
    N(f64),
    S(String),
    O(RefCell<Vec<(String, V)>>),
    A(RefCell<Vec<V>>),
    F(Rc<dyn Fn(V, Vec<V>) -> Result<V, E>>),
    P(Rc<Promise>),
}
pub(crate) type V = Rc<Js>;
pub(crate) type E = Rc<String>;
pub(crate) struct Promise {
    pub(crate) task: RefCell<Option<Task<V, E>>>,
    pub(crate) result: RefCell<Option<Result<V, E>>>,
}
pub(crate) fn u() -> V {
    Rc::new(Js::U)
}
pub(crate) fn n(v: f64) -> V {
    Rc::new(Js::N(v))
}
pub(crate) fn s(v: &str) -> V {
    Rc::new(Js::S(v.into()))
}
pub(crate) fn b(v: bool) -> V {
    Rc::new(Js::B(v))
}
pub(crate) fn obj(v: Vec<(&str, V)>) -> V {
    Rc::new(Js::O(RefCell::new(
        v.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    )))
}
pub(crate) fn arr(v: Vec<V>) -> V {
    Rc::new(Js::A(RefCell::new(v)))
}
pub(crate) fn function(f: impl Fn(V, Vec<V>) -> Result<V, E> + 'static) -> V {
    Rc::new(Js::F(Rc::new(f)))
}
pub(crate) fn call(f: V, r: V, a: Vec<V>) -> Result<V, E> {
    let Js::F(f) = f.as_ref() else {
        panic!("fixture callable")
    };
    f(r, a)
}
pub(crate) fn text(v: &V) -> String {
    match v.as_ref() {
        Js::S(s) => s.clone(),
        Js::N(n) => safe_cex_num::js_number_string(*n),
        Js::U => "undefined".into(),
        _ => panic!("fixture text"),
    }
}
pub(crate) fn number(v: &V) -> f64 {
    match v.as_ref() {
        Js::N(n) => *n,
        Js::S(s) => s.parse().unwrap_or(f64::NAN),
        Js::Null => 0.,
        _ => f64::NAN,
    }
}
pub(crate) fn truthy(v: &V) -> bool {
    match v.as_ref() {
        Js::U | Js::Null => false,
        Js::B(b) => *b,
        Js::N(n) => *n != 0. && !n.is_nan(),
        Js::S(s) => !s.is_empty(),
        _ => true,
    }
}
pub(crate) fn set(v: &V, key: &str, new: V) {
    let Js::O(fields) = v.as_ref() else {
        panic!("fixture set")
    };
    let mut fields = fields.borrow_mut();
    if let Some((_, v)) = fields.iter_mut().find(|(k, _)| k == key) {
        *v = new
    } else {
        fields.push((key.into(), new))
    }
}
pub(crate) fn get(v: &V, key: &str) -> Result<V, E> {
    match v.as_ref() {
        Js::O(fields) => Ok(fields
            .borrow()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(u)),
        Js::A(rows) => match key {
            "length" => Ok(n(rows.borrow().len() as f64)),
            "forEach" => Ok(function(|r, a| {
                let Js::A(rows) = r.as_ref() else {
                    panic!("array forEach")
                };
                let length = rows.borrow().len();
                for index in 0..length {
                    let row = rows.borrow().get(index).cloned();
                    if let Some(row) = row {
                        call(
                            a[0].clone(),
                            a.get(1).cloned().unwrap_or_else(u),
                            vec![row, n(index as f64), r.clone()],
                        )?;
                    }
                }
                Ok(u())
            })),
            "map" | "filter" => {
                let filter = key == "filter";
                Ok(function(move |r, a| {
                    let Js::A(rows) = r.as_ref() else {
                        panic!("map/filter")
                    };
                    let length = rows.borrow().len();
                    let mut out = Vec::new();
                    for i in 0..length {
                        let value = rows.borrow().get(i).cloned();
                        if let Some(value) = value {
                            let mapped = call(
                                a[0].clone(),
                                a.get(1).cloned().unwrap_or_else(u),
                                vec![value.clone(), n(i as f64), r.clone()],
                            )?;
                            if filter {
                                if truthy(&mapped) {
                                    out.push(value);
                                }
                            } else {
                                out.push(mapped);
                            }
                        }
                    }
                    Ok(arr(out))
                }))
            }
            "slice" => Ok(function(|r, a| {
                let Js::A(rows) = r.as_ref() else {
                    panic!("slice")
                };
                let rows = rows.borrow();
                let start = (number(&a[0]) as usize).min(rows.len());
                let end = (number(&a[1]) as usize).min(rows.len());
                Ok(arr(rows[start..end.max(start)].to_vec()))
            })),
            "join" => Ok(function(|r, a| {
                let Js::A(rows) = r.as_ref() else {
                    panic!("join")
                };
                Ok(s(&rows
                    .borrow()
                    .iter()
                    .map(text)
                    .collect::<Vec<_>>()
                    .join(&text(&a[0]))))
            })),
            "push" => Ok(function(|r, a| {
                let Js::A(rows) = r.as_ref() else {
                    panic!("push")
                };
                rows.borrow_mut().extend(a);
                Ok(n(rows.borrow().len() as f64))
            })),
            _ => Ok(key
                .parse::<usize>()
                .ok()
                .and_then(|i| rows.borrow().get(i).cloned())
                .unwrap_or_else(u)),
        },
        Js::S(_) if key == "split" => Ok(function(|r, a| {
            Ok(arr(text(&r).split(&text(&a[0])).map(s).collect()))
        })),
        Js::S(_) if key == "replace" => Ok(function(|r, a| {
            Ok(s(&text(&r).replacen(&text(&a[0]), &text(&a[1]), 1)))
        })),
        Js::S(_) if key == "startsWith" => {
            Ok(function(|r, a| Ok(b(text(&r).starts_with(&text(&a[0]))))))
        }
        Js::U | Js::Null => Err(Rc::new(format!("read {key} on nullish"))),
        _ => Ok(u()),
    }
}
pub(crate) fn literal(v: &V) -> Value {
    match v.as_ref() {
        Js::U => json!({"undefined":true}),
        Js::Null => Value::Null,
        Js::B(v) => json!(v),
        Js::N(v) => {
            if v.is_finite() {
                serde_json::from_str(&safe_cex_num::js_number_string(*v)).unwrap()
            } else {
                Value::Null
            }
        }
        Js::S(v) => json!(v),
        Js::A(v) => Value::Array(v.borrow().iter().map(literal).collect()),
        Js::O(v) => Value::Object(
            v.borrow()
                .iter()
                .map(|(k, v)| (k.clone(), literal(v)))
                .collect(),
        ),
        _ => panic!("fixture literal"),
    }
}
pub(crate) fn decode(v: &Value) -> V {
    match v {
        Value::Null => Rc::new(Js::Null),
        Value::Bool(v) => b(*v),
        Value::Number(v) => n(v.as_f64().unwrap()),
        Value::String(v) => s(v),
        Value::Array(v) => arr(v.iter().map(decode).collect()),
        Value::Object(v) => Rc::new(Js::O(RefCell::new(
            v.iter().map(|(k, v)| (k.clone(), decode(v))).collect(),
        ))),
    }
}
pub(crate) fn poll(v: &V) -> Poll<Result<V, E>> {
    let Js::P(p) = v.as_ref() else {
        return Poll::Ready(Ok(v.clone()));
    };
    if let Some(r) = p.result.borrow().clone() {
        return Poll::Ready(r);
    }
    let mut t = p.task.borrow_mut().take().expect("retained promise task");
    let result = t.as_mut().poll(&mut Context::from_waker(Waker::noop()));
    match result {
        Poll::Ready(r) => {
            *p.result.borrow_mut() = Some(r.clone());
            Poll::Ready(r)
        }
        Poll::Pending => {
            *p.task.borrow_mut() = Some(t);
            Poll::Pending
        }
    }
}
pub(crate) fn pending() -> V {
    Rc::new(Js::P(Rc::new(Promise {
        task: RefCell::new(Some(Box::pin(std::future::pending()))),
        result: RefCell::new(None),
    })))
}
pub(crate) fn resolve(v: &V, result: Result<V, E>) {
    let Js::P(p) = v.as_ref() else {
        panic!("promise")
    };
    assert!(p.result.borrow().is_none());
    *p.result.borrow_mut() = Some(result);
}
pub(crate) fn wait(v: V) -> Task<V, E> {
    let mut yielded = false;
    Box::pin(std::future::poll_fn(move |_| {
        if !yielded {
            yielded = true;
            return Poll::Pending;
        }
        poll(&v)
    }))
}

pub(crate) fn stringify(v: &V) -> String {
    fn value(v: &V) -> safe_cex_num::JsJsonValue {
        use safe_cex_num::JsJsonValue as J;
        match v.as_ref() {
            Js::U => J::Undefined,
            Js::Null => J::Null,
            Js::B(v) => J::Bool(*v),
            Js::N(v) => J::Number(*v),
            Js::S(v) => J::String(v.clone()),
            Js::A(v) => J::Array(v.borrow().iter().map(value).collect()),
            Js::O(v) => J::Object(
                v.borrow()
                    .iter()
                    .map(|(k, v)| (k.clone(), value(v)))
                    .collect(),
            ),
            _ => panic!("unexercised JSON callable/promise"),
        }
    }
    safe_cex_num::js_json_stringify(&value(v))
        .unwrap()
        .expect("object stringify")
}
