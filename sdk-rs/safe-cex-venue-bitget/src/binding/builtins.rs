//! Builtin prototype members of primitive and intrinsic values (String, Array,
//! Map, Set, Promise, Number). Each call re-reads the live receiver.
use super::{
    globals::arg,
    promise::{Promise, await_js},
    value::{Js, Str, Thrown},
};
use safe_cex_exchange::runtime::ExchangeRuntime;
use std::{cell::RefCell, rc::Rc};

thread_local! {
    static RUNTIME: RefCell<Option<Rc<dyn ExchangeRuntime>>> = const { RefCell::new(None) };
}

pub fn install_runtime(runtime: Rc<dyn ExchangeRuntime>) {
    RUNTIME.with(|r| *r.borrow_mut() = Some(runtime));
}

fn runtime() -> Result<Rc<dyn ExchangeRuntime>, Thrown> {
    RUNTIME
        .with(|r| r.borrow().clone())
        .ok_or_else(|| Thrown::error("Error", "no Bitget runtime installed"))
}

fn method(name: &str, f: impl Fn(Vec<Js>) -> Result<Js, Thrown> + 'static) -> Js {
    Js::function(name, move |_, args| f(args))
}

fn text(v: &Js) -> Result<String, Thrown> {
    Ok(String::from_utf16_lossy(&v.to_js_string()?))
}

fn callback_call(callback: &Js, args: Vec<Js>) -> Result<Js, Thrown> {
    if !callback.is_callable() {
        return Err(Thrown::type_error(&format!("{} is not a function", callback.to_display())));
    }
    callback.call(Js::Undefined, args)
}

pub fn member(receiver: &Js, key: &[u16]) -> Option<Js> {
    let name = String::from_utf16_lossy(key);
    let this = receiver.clone();
    match receiver {
        Js::String(s) => string_member(s.clone(), &name),
        Js::Array(_) => array_member(this, &name),
        Js::Map(_) => map_member(this, &name),
        Js::Set(_) => set_member(this, &name),
        Js::Promise(p) => promise_member(p.clone(), &name),
        Js::Number(n) => {
            let n = *n;
            match name.as_str() {
                "toString" => Some(method("toString", move |_| Ok(Js::str(&safe_cex_num::js_number_string(n))))),
                "toFixed" => Some(method("toFixed", move |a| {
                    let digits = arg(&a, 0).to_number()?.max(0.0) as usize;
                    Ok(Js::str(&format!("{n:.digits$}")))
                })),
                _ => None,
            }
        }
        Js::Bool(b) => {
            let b = *b;
            (name == "toString").then(|| method("toString", move |_| Ok(Js::str(if b { "true" } else { "false" }))))
        }
        _ => None,
    }
}

fn string_member(s: Str, name: &str) -> Option<Js> {
    let st = String::from_utf16_lossy(&s);
    Some(match name {
        "toUpperCase" => method(name, move |_| Ok(Js::str(&st.to_uppercase()))),
        "toLowerCase" => method(name, move |_| Ok(Js::str(&st.to_lowercase()))),
        "trim" => method(name, move |_| Ok(Js::str(safe_cex_num::js_trim(&st)))),
        "toString" | "valueOf" => method(name, move |_| Ok(Js::String(s.clone()))),
        "includes" => method(name, move |a| Ok(Js::Bool(st.contains(&text(&arg(&a, 0))?)))),
        "startsWith" => method(name, move |a| Ok(Js::Bool(st.starts_with(&text(&arg(&a, 0))?)))),
        "endsWith" => method(name, move |a| Ok(Js::Bool(st.ends_with(&text(&arg(&a, 0))?)))),
        "indexOf" => method(name, move |a| {
            let needle = text(&arg(&a, 0))?;
            Ok(Js::Number(st.find(&needle).map_or(-1.0, |i| st[..i].encode_utf16().count() as f64)))
        }),
        "replace" => method(name, move |a| {
            let needle: Vec<u16> = arg(&a, 0).to_js_string()?.to_vec();
            let replacement = arg(&a, 1);
            let source: Vec<u16> = s.to_vec();
            let Some(at) = find_utf16(&source, &needle) else {
                return Ok(Js::String(s.clone()));
            };
            let mut out = source[..at].to_vec();
            if replacement.is_callable() {
                out.extend_from_slice(&replacement.call(Js::Undefined, vec![Js::utf16(&needle)])?.to_js_string()?);
            } else {
                let r = String::from_utf16_lossy(&replacement.to_js_string()?);
                let matched = String::from_utf16_lossy(&needle);
                out.extend(safe_cex_num::js_replace(&matched, &matched, &r).encode_utf16());
            }
            out.extend_from_slice(&source[at + needle.len()..]);
            Ok(Js::utf16(&out))
        }),
        "split" => method(name, move |a| {
            let sep = arg(&a, 0);
            if sep.is_undefined() {
                return Ok(Js::array(vec![Js::String(s.clone())]));
            }
            let sep = text(&sep)?;
            Ok(Js::array(if sep.is_empty() {
                s.iter().map(|c| Js::utf16(&[*c])).collect()
            } else {
                st.split(&sep).map(Js::str).collect()
            }))
        }),
        "slice" | "substring" => {
            let sub = name == "substring";
            method(name, move |a| {
                let len = s.len() as f64;
                let norm = |v: Js, default: f64| -> Result<f64, Thrown> {
                    if v.is_undefined() {
                        return Ok(default);
                    }
                    let n = v.to_number()?;
                    let n = if n.is_nan() { 0.0 } else { n.trunc() };
                    Ok(if sub {
                        n.clamp(0.0, len)
                    } else if n < 0.0 {
                        (len + n).max(0.0)
                    } else {
                        n.min(len)
                    })
                };
                let mut start = norm(arg(&a, 0), 0.0)?;
                let mut end = norm(arg(&a, 1), len)?;
                if sub && start > end {
                    std::mem::swap(&mut start, &mut end);
                }
                Ok(if start >= end {
                    Js::str("")
                } else {
                    Js::utf16(&s[start as usize..end as usize])
                })
            })
        }
        "concat" => method(name, move |a| {
            let mut out = s.to_vec();
            for v in a {
                out.extend_from_slice(&v.to_js_string()?);
            }
            Ok(Js::utf16(&out))
        }),
        _ => return None,
    })
}

fn find_utf16(haystack: &[u16], needle: &[u16]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn items(this: &Js) -> Vec<Option<Js>> {
    this.array_items().unwrap_or_default()
}

fn array_member(this: Js, name: &str) -> Option<Js> {
    Some(match name {
        "push" => method(name, move |a| {
            for v in a {
                this.push(v)?;
            }
            Ok(Js::Number(items(&this).len() as f64))
        }),
        "includes" => method(name, move |a| {
            let needle = arg(&a, 0);
            Ok(Js::Bool(items(&this).iter().any(|v| v.clone().unwrap_or(Js::Undefined).same_value_zero(&needle))))
        }),
        "indexOf" => method(name, move |a| {
            let needle = arg(&a, 0);
            Ok(Js::Number(
                items(&this)
                    .iter()
                    .position(|v| v.as_ref().is_some_and(|v| v.strict_equals(&needle)))
                    .map_or(-1.0, |i| i as f64),
            ))
        }),
        "join" => method(name, move |a| {
            let sep = arg(&a, 0);
            let sep = if sep.is_undefined() { ",".to_owned() } else { text(&sep)? };
            let mut parts = vec![];
            for v in items(&this) {
                parts.push(match v {
                    Some(v) if !v.is_nullish() => text(&v)?,
                    _ => String::new(),
                });
            }
            Ok(Js::str(&parts.join(&sep)))
        }),
        "slice" => method(name, move |a| {
            let all = items(&this);
            let len = all.len() as f64;
            let norm = |v: Js, d: f64| -> Result<usize, Thrown> {
                if v.is_undefined() {
                    return Ok(d as usize);
                }
                let n = v.to_number()?.trunc();
                Ok(if n < 0.0 { (len + n).max(0.0) } else { n.min(len) } as usize)
            };
            let start = norm(arg(&a, 0), 0.0)?;
            let end = norm(arg(&a, 1), len)?;
            Ok(Js::holey_array(if start < end { all[start..end].to_vec() } else { vec![] }))
        }),
        "concat" => method(name, move |a| {
            let mut out = items(&this);
            for v in a {
                match v.array_items() {
                    Some(more) => out.extend(more),
                    None => out.push(Some(v)),
                }
            }
            Ok(Js::holey_array(out))
        }),
        "map" => method(name, move |a| {
            let f = arg(&a, 0);
            let all = items(&this);
            let mut out = Vec::with_capacity(all.len());
            for (i, v) in all.into_iter().enumerate() {
                out.push(match v {
                    Some(v) => Some(callback_call(&f, vec![v, Js::Number(i as f64), this.clone()])?),
                    None => None,
                });
            }
            Ok(Js::holey_array(out))
        }),
        "forEach" => method(name, move |a| {
            let f = arg(&a, 0);
            let len = items(&this).len();
            for i in 0..len {
                if let Some(Some(v)) = items(&this).get(i).cloned() {
                    callback_call(&f, vec![v, Js::Number(i as f64), this.clone()])?;
                }
            }
            Ok(Js::Undefined)
        }),
        "filter" | "some" | "every" | "find" | "findIndex" => {
            let kind = name.to_owned();
            method(name, move |a| {
                let f = arg(&a, 0);
                let len = items(&this).len();
                let mut out = vec![];
                for i in 0..len {
                    let v = items(&this).get(i).cloned().flatten();
                    let present = v.is_some();
                    let v = v.unwrap_or(Js::Undefined);
                    if !present && matches!(kind.as_str(), "filter" | "some" | "every") {
                        continue;
                    }
                    let hit = callback_call(&f, vec![v.clone(), Js::Number(i as f64), this.clone()])?.truthy();
                    match kind.as_str() {
                        "filter" if hit => out.push(v),
                        "some" if hit => return Ok(Js::Bool(true)),
                        "every" if !hit => return Ok(Js::Bool(false)),
                        "find" if hit => return Ok(v),
                        "findIndex" if hit => return Ok(Js::Number(i as f64)),
                        _ => {}
                    }
                }
                Ok(match kind.as_str() {
                    "filter" => Js::array(out),
                    "some" => Js::Bool(false),
                    "every" => Js::Bool(true),
                    "find" => Js::Undefined,
                    _ => Js::Number(-1.0),
                })
            })
        }
        "reduce" => method(name, move |a| {
            let f = arg(&a, 0);
            let all = items(&this);
            let mut iter = all.into_iter().enumerate().filter_map(|(i, v)| v.map(|v| (i, v)));
            let mut acc = if a.len() > 1 {
                a[1].clone()
            } else {
                match iter.next() {
                    Some((_, v)) => v,
                    None => return Err(Thrown::type_error("Reduce of empty array with no initial value")),
                }
            };
            for (i, v) in iter {
                acc = callback_call(&f, vec![acc, v, Js::Number(i as f64), this.clone()])?;
            }
            Ok(acc)
        }),
        "flatMap" => method(name, move |a| {
            let f = arg(&a, 0);
            let mut out = vec![];
            for (i, v) in items(&this).into_iter().enumerate() {
                let Some(v) = v else { continue };
                let mapped = callback_call(&f, vec![v, Js::Number(i as f64), this.clone()])?;
                match mapped.array_items() {
                    Some(inner) => out.extend(inner.into_iter().flatten()),
                    None => out.push(mapped),
                }
            }
            Ok(Js::array(out))
        }),
        "sort" => method(name, move |a| {
            let f = arg(&a, 0);
            let Js::Array(cells) = &this else { return Ok(this.clone()) };
            let mut values: Vec<Js> = cells.borrow().iter().flatten().cloned().collect();
            let holes = cells.borrow().len() - values.len();
            let mut failure = None;
            values.sort_by(|x, y| {
                if failure.is_some() {
                    return std::cmp::Ordering::Equal;
                }
                let order = if x.is_undefined() || y.is_undefined() {
                    Ok(f64::from(u8::from(x.is_undefined())) - f64::from(u8::from(y.is_undefined())))
                } else if f.is_callable() {
                    f.call(Js::Undefined, vec![x.clone(), y.clone()]).and_then(|v| v.to_number())
                } else {
                    (|| Ok(match text(x)?.encode_utf16().cmp(text(y)?.encode_utf16()) {
                        std::cmp::Ordering::Less => -1.0,
                        std::cmp::Ordering::Equal => 0.0,
                        std::cmp::Ordering::Greater => 1.0,
                    }))()
                };
                match order {
                    Ok(n) if n < 0.0 => std::cmp::Ordering::Less,
                    Ok(n) if n > 0.0 => std::cmp::Ordering::Greater,
                    Ok(_) => std::cmp::Ordering::Equal,
                    Err(e) => {
                        failure = Some(e);
                        std::cmp::Ordering::Equal
                    }
                }
            });
            if let Some(e) = failure {
                return Err(e);
            }
            let mut sorted: Vec<Option<Js>> = values.into_iter().map(Some).collect();
            sorted.extend(std::iter::repeat_n(None, holes));
            *cells.borrow_mut() = sorted;
            Ok(this.clone())
        }),
        _ => return None,
    })
}

fn map_entries(this: &Js) -> Rc<RefCell<Vec<(Js, Js)>>> {
    match this {
        Js::Map(m) => m.clone(),
        _ => Rc::default(),
    }
}

fn map_member(this: Js, name: &str) -> Option<Js> {
    let m = map_entries(&this);
    Some(match name {
        "get" => method(name, move |a| {
            let k = arg(&a, 0);
            Ok(m.borrow().iter().find(|(x, _)| x.same_value_zero(&k)).map(|(_, v)| v.clone()).unwrap_or(Js::Undefined))
        }),
        "has" => method(name, move |a| {
            let k = arg(&a, 0);
            Ok(Js::Bool(m.borrow().iter().any(|(x, _)| x.same_value_zero(&k))))
        }),
        "set" => method(name, move |a| {
            let k = arg(&a, 0);
            let v = arg(&a, 1);
            let mut entries = m.borrow_mut();
            match entries.iter_mut().find(|(x, _)| x.same_value_zero(&k)) {
                Some(slot) => slot.1 = v,
                None => entries.push((k, v)),
            }
            drop(entries);
            Ok(this.clone())
        }),
        "delete" => method(name, move |a| {
            let k = arg(&a, 0);
            let mut entries = m.borrow_mut();
            let before = entries.len();
            entries.retain(|(x, _)| !x.same_value_zero(&k));
            Ok(Js::Bool(entries.len() != before))
        }),
        "clear" => method(name, move |_| {
            m.borrow_mut().clear();
            Ok(Js::Undefined)
        }),
        "keys" => method(name, move |_| Ok(Js::array(m.borrow().iter().map(|(k, _)| k.clone()).collect()))),
        "values" => method(name, move |_| Ok(Js::array(m.borrow().iter().map(|(_, v)| v.clone()).collect()))),
        "entries" => method(name, move |_| {
            Ok(Js::array(m.borrow().iter().map(|(k, v)| Js::array(vec![k.clone(), v.clone()])).collect()))
        }),
        "forEach" => method(name, move |a| {
            let f = arg(&a, 0);
            let snapshot = m.borrow().clone();
            for (k, v) in snapshot {
                callback_call(&f, vec![v, k, this.clone()])?;
            }
            Ok(Js::Undefined)
        }),
        _ => return None,
    })
}

fn set_member(this: Js, name: &str) -> Option<Js> {
    let Js::Set(s) = &this else { return None };
    let s = s.clone();
    Some(match name {
        "has" => method(name, move |a| {
            let k = arg(&a, 0);
            Ok(Js::Bool(s.borrow().iter().any(|x| x.same_value_zero(&k))))
        }),
        "add" => method(name, move |a| {
            let k = arg(&a, 0);
            if !s.borrow().iter().any(|x| x.same_value_zero(&k)) {
                s.borrow_mut().push(k);
            }
            Ok(this.clone())
        }),
        "delete" => method(name, move |a| {
            let k = arg(&a, 0);
            let mut entries = s.borrow_mut();
            let before = entries.len();
            entries.retain(|x| !x.same_value_zero(&k));
            Ok(Js::Bool(entries.len() != before))
        }),
        "clear" => method(name, move |_| {
            s.borrow_mut().clear();
            Ok(Js::Undefined)
        }),
        "values" | "keys" => method(name, move |_| Ok(Js::array(s.borrow().clone()))),
        "forEach" => method(name, move |a| {
            let f = arg(&a, 0);
            for v in s.borrow().clone() {
                callback_call(&f, vec![v.clone(), v, this.clone()])?;
            }
            Ok(Js::Undefined)
        }),
        _ => return None,
    })
}

fn promise_member(p: Promise, name: &str) -> Option<Js> {
    let chain = move |on_ok: Js, on_err: Js| -> Result<Js, Thrown> {
        let p = p.clone();
        let rt = runtime()?;
        Ok(Js::Promise(Promise::spawn(
            &rt,
            Box::pin(async move {
                match p.wait().await {
                    Ok(v) if on_ok.is_callable() => await_js(on_ok.call(Js::Undefined, vec![v])?).await,
                    Ok(v) => Ok(v),
                    Err(e) if on_err.is_callable() => await_js(on_err.call(Js::Undefined, vec![e.value])?).await,
                    Err(e) => Err(e),
                }
            }),
        )))
    };
    Some(match name {
        "then" => method(name, move |a| chain(arg(&a, 0), arg(&a, 1))),
        "catch" => method(name, move |a| chain(Js::Undefined, arg(&a, 0))),
        "finally" => method(name, move |a| {
            let f = arg(&a, 0);
            let g = f.clone();
            let on_ok = Js::function("", move |_, v| {
                if f.is_callable() {
                    let p = f.call(Js::Undefined, vec![])?;
                    let value = arg(&v, 0);
                    return Ok(chain_value(p, Ok(value)));
                }
                Ok(arg(&v, 0))
            });
            let on_err = Js::function("", move |_, v| {
                if g.is_callable() {
                    let p = g.call(Js::Undefined, vec![])?;
                    return Ok(chain_value(p, Err(Thrown::new(arg(&v, 0)))));
                }
                Err(Thrown::new(arg(&v, 0)))
            });
            chain(on_ok, on_err)
        }),
        _ => return None,
    })
}

/// Await `p`, then settle with `outcome` (finally keeps the original result).
fn chain_value(p: Js, outcome: Result<Js, Thrown>) -> Js {
    let Ok(rt) = runtime() else { return Js::Undefined };
    Js::Promise(Promise::spawn(
        &rt,
        Box::pin(async move {
            await_js(p).await?;
            outcome
        }),
    ))
}
