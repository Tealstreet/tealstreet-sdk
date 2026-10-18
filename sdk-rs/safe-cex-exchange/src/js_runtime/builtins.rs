//! Builtin prototype members of primitive and intrinsic values (String, Array,
//! Map, Set, Promise, Number). Each call re-reads the live receiver.
use super::{
    promise::{Promise, await_js},
    value::{Js, Str, Thrown, arg},
};
use crate::runtime::ExchangeRuntime;
use std::{cell::RefCell, rc::Rc};

fn method(name: &str, f: impl Fn(Vec<Js>) -> Result<Js, Thrown> + 'static) -> Js {
    Js::function(name, move |_, args| f(args))
}

fn require_callback(callback: &Js) -> Result<(), Thrown> {
    if !callback.is_callable() {
        return Err(Thrown::type_error(&format!(
            "{} is not a function",
            callback.to_display()
        )));
    }
    Ok(())
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
                "toString" => Some(method("toString", move |args| {
                    let requested = arg(&args, 0);
                    let radix = if requested.is_undefined() {
                        10.0
                    } else {
                        requested.to_number()?.trunc()
                    };
                    if !(2.0..=36.0).contains(&radix) {
                        return Err(Thrown::error(
                            "RangeError",
                            "toString() radix argument must be between 2 and 36",
                        ));
                    }
                    Ok(Js::str(&super::number_radix::format(n, radix as u32)))
                })),
                "toFixed" => Some(method("toFixed", move |a| {
                    let digits = arg(&a, 0).to_number()?;
                    safe_cex_num::js_to_fixed(n, digits)
                        .map(|text| Js::str(&text))
                        .map_err(|message| Thrown::error("RangeError", message))
                })),
                _ => None,
            }
        }
        Js::BigInt(_) => (name == "toString").then(|| {
            Js::function("toString", |this, args| {
                let Js::BigInt(n) = this else {
                    return Err(Thrown::type_error(
                        "BigInt.prototype.toString requires that 'this' be a BigInt",
                    ));
                };
                let requested = arg(&args, 0);
                let radix = if requested.is_undefined() {
                    10.0
                } else {
                    requested.to_number()?.trunc()
                };
                if !(2.0..=36.0).contains(&radix) {
                    return Err(Thrown::error(
                        "RangeError",
                        "toString() radix argument must be between 2 and 36",
                    ));
                }
                Ok(Js::str(&n.to_str_radix(radix as u32)))
            })
        }),
        Js::Bool(b) => {
            let b = *b;
            (name == "toString").then(|| {
                method("toString", move |_| {
                    Ok(Js::str(if b { "true" } else { "false" }))
                })
            })
        }
        _ => None,
    }
}

fn is_regexp(value: &Js) -> bool {
    matches!(value, Js::Object(o) if o.class == "RegExp")
}

/// replace/split/match/search hand a RegExp argument to its @@ method; match and
/// search build a RegExp from any other pattern, as String.prototype does.
fn string_member(s: Str, name: &str) -> Option<Js> {
    let symbol = match name {
        "replace" => "@@replace",
        "split" => "@@split",
        "match" => "@@match",
        "search" => "@@search",
        _ => return plain_string_member(s, name),
    };
    let plain = plain_string_member(s.clone(), name);
    let receiver = Js::String(s);
    Some(method(name, move |a| {
        let mut pattern = arg(&a, 0);
        if !is_regexp(&pattern) {
            if let Some(plain) = &plain {
                return plain.call(Js::Undefined, a);
            }
            let source = if pattern.is_undefined() {
                Vec::new()
            } else {
                pattern.to_js_string()?.to_vec()
            };
            pattern = super::regexp::regexp(&source, &[])?;
        }
        let mut args = vec![receiver.clone()];
        args.extend(a.into_iter().skip(1));
        pattern.call_method(symbol, args)
    }))
}

fn case_utf16(source: &[u16], uppercase: bool) -> Js {
    let mut result = Vec::with_capacity(source.len());
    let mut text = String::new();
    let flush = |text: &mut String, result: &mut Vec<u16>| {
        let mapped = if uppercase {
            text.to_uppercase()
        } else {
            text.to_lowercase()
        };
        result.extend(mapped.encode_utf16());
        text.clear();
    };
    // Keep whole valid runs for contextual mappings such as Greek sigma.
    for character in char::decode_utf16(source.iter().copied()) {
        match character {
            Ok(character) => text.push(character),
            Err(error) => {
                flush(&mut text, &mut result);
                result.push(error.unpaired_surrogate());
            }
        }
    }
    flush(&mut text, &mut result);
    Js::utf16(&result)
}

fn plain_string_member(s: Str, name: &str) -> Option<Js> {
    Some(match name {
        "toUpperCase" => method(name, move |_| Ok(case_utf16(&s, true))),
        "toLowerCase" => method(name, move |_| Ok(case_utf16(&s, false))),
        "trim" => method(name, move |_| {
            let start = s
                .iter()
                .position(|unit| !super::regexp::is_space(*unit))
                .unwrap_or(s.len());
            let end = s
                .iter()
                .rposition(|unit| !super::regexp::is_space(*unit))
                .map_or(start, |index| index + 1);
            Ok(Js::utf16(&s[start..end]))
        }),
        "toString" | "valueOf" => method(name, move |_| Ok(Js::String(s.clone()))),
        "localeCompare" => method(name, move |a| {
            let st = String::from_utf16_lossy(&s);
            let right = String::from_utf16_lossy(&arg(&a, 0).to_js_string()?);
            let collator = safe_cex_num::JsRootCollator::try_new()
                .map_err(|error| Thrown::error(error.name, error.message))?;
            Ok(Js::Number(collator.compare(&st, &right) as i8 as f64))
        }),
        "indexOf" | "lastIndexOf" | "includes" | "startsWith" | "endsWith" => {
            let operation = name.to_owned();
            method(name, move |a| {
                let needle = arg(&a, 0).to_js_string()?;
                let position = arg(&a, 1);
                let number = position.to_number()?;
                let position = if (operation == "endsWith" && position.is_undefined())
                    || (operation == "lastIndexOf" && number.is_nan())
                {
                    s.len()
                } else {
                    string_position(number, s.len())
                };
                Ok(match operation.as_str() {
                    "includes" => Js::Bool(find_utf16(&s[position..], &needle).is_some()),
                    "startsWith" => Js::Bool(s[position..].starts_with(&needle)),
                    "endsWith" => Js::Bool(s[..position].ends_with(&needle)),
                    "lastIndexOf" => {
                        let found = if needle.len() > s.len() {
                            None
                        } else {
                            let last = position.min(s.len() - needle.len());
                            (0..=last).rev().find(|at| s[*at..].starts_with(&needle))
                        };
                        Js::Number(found.map_or(-1.0, |at| at as f64))
                    }
                    _ => Js::Number(
                        find_utf16(&s[position..], &needle)
                            .map_or(-1.0, |at| (position + at) as f64),
                    ),
                })
            })
        }
        "replace" => method(name, move |a| {
            let needle: Vec<u16> = arg(&a, 0).to_js_string()?.to_vec();
            let replacement = arg(&a, 1);
            let source: Vec<u16> = s.to_vec();
            let Some(at) = find_utf16(&source, &needle) else {
                return Ok(Js::String(s.clone()));
            };
            let mut out = source[..at].to_vec();
            if replacement.is_callable() {
                out.extend_from_slice(
                    &replacement
                        .call(Js::Undefined, vec![Js::utf16(&needle)])?
                        .to_js_string()?,
                );
            } else {
                let r = String::from_utf16_lossy(&replacement.to_js_string()?);
                let matched = String::from_utf16_lossy(&needle);
                out.extend(safe_cex_num::js_replace(&matched, &matched, &r).encode_utf16());
            }
            out.extend_from_slice(&source[at + needle.len()..]);
            Ok(Js::utf16(&out))
        }),
        "split" => method(name, move |a| {
            let separator = arg(&a, 0);
            let limit = super::regexp::split_limit(&arg(&a, 1))?;
            // Source coercion order is limit, then separator, even at limit 0.
            let sep = separator.to_js_string()?;
            if limit == 0 {
                return Ok(Js::array(vec![]));
            }
            if separator.is_undefined() {
                return Ok(Js::array(vec![Js::String(s.clone())]));
            }
            if sep.is_empty() {
                return Ok(Js::array(
                    s.iter().take(limit).map(|c| Js::utf16(&[*c])).collect(),
                ));
            }
            let mut out = vec![];
            let mut start = 0;
            while let Some(offset) = find_utf16(&s[start..], &sep) {
                let end = start + offset;
                out.push(Js::utf16(&s[start..end]));
                if out.len() == limit {
                    return Ok(Js::array(out));
                }
                start = end + sep.len();
            }
            out.push(Js::utf16(&s[start..]));
            Ok(Js::array(out))
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

fn string_position(value: f64, length: usize) -> usize {
    if value.is_nan() {
        0
    } else {
        value.clamp(0.0, length as f64) as usize
    }
}

fn array_length(this: &Js) -> Result<usize,Thrown> { Ok(this.get("length")?.to_number()? as usize) }
fn indexed(this:&Js,index:usize)->Result<Option<Js>,Thrown> {
    let key=super::value::utf16(&index.to_string());
    if this.has_property(&key) {Ok(Some(this.get_key(&key)?))} else {Ok(None)}
}
fn items(this: &Js) -> Result<Vec<Option<Js>>,Thrown> {
    this.array_items()?.ok_or_else(|| Thrown::type_error("array receiver required"))
}

pub(crate) fn array_member(this: Js, name: &str) -> Option<Js> {
    Some(match name {
        "push" => method(name, move |a| {
            let mut len=array_length(&this)?;
            for value in a {this.set(&len.to_string(),value)?;len+=1;}
            this.set("length",Js::Number(len as f64))?;
            Ok(Js::Number(len as f64))
        }),
        "pop" => method(name, move |_| {
            let len=array_length(&this)?;
            if len==0 {this.set("length",Js::Number(0.))?;return Ok(Js::Undefined);}
            let key=(len-1).to_string();let value=this.get(&key)?;
            this.delete(&super::value::utf16(&key))?;this.set("length",Js::Number((len-1) as f64))?;Ok(value)
        }),
        "reverse" => method(name, move |_| {
            let len=array_length(&this)?;
            for lower in 0..len/2 {
                let upper=len-lower-1;
                let left=indexed(&this,lower)?;let right=indexed(&this,upper)?;
                match (left,right) {
                    (Some(l),Some(r))=>{this.set(&lower.to_string(),r)?;this.set(&upper.to_string(),l)?;},
                    (None,Some(r))=>{this.set(&lower.to_string(),r)?;this.delete(&super::value::utf16(&upper.to_string()))?;},
                    (Some(l),None)=>{this.delete(&super::value::utf16(&lower.to_string()))?;this.set(&upper.to_string(),l)?;},
                    _=>{},
                }
            }
            Ok(this.clone())
        }),
        "includes" | "indexOf" => {let includes=name=="includes";method(name,move |a|{
            let len=array_length(&this)?;let needle=arg(&a,0);let start=arg(&a,1).to_number()?;
            let start=if start.is_nan(){0.}else{start.trunc()};let start=if start<0.{(len as f64+start).max(0.)}else{start} as usize;
            for i in start..len {
                let value=if includes {Some(this.get(&i.to_string())?)}else{indexed(&this,i)?};
                if value.is_some_and(|v| if includes{v.same_value_zero(&needle)}else{v.strict_equals(&needle)}) {return Ok(if includes{Js::Bool(true)}else{Js::Number(i as f64)});}
            }
            Ok(if includes{Js::Bool(false)}else{Js::Number(-1.)})
        })},
        "join" => method(name,move |a|{
            let len=array_length(&this)?;let separator=arg(&a,0);
            let sep=if separator.is_undefined(){super::value::utf16(",")}else{separator.to_js_string()?};
            let mut units=vec![];
            for i in 0..len {if i>0{units.extend_from_slice(&sep);}let v=this.get(&i.to_string())?;if !v.is_nullish(){units.extend_from_slice(&v.to_js_string()?);}}
            Ok(Js::utf16(&units))
        }),
        "slice" => method(name,move |a|{
            let len=array_length(&this)?;
            let norm=|v:Js,d:usize|->Result<usize,Thrown>{if v.is_undefined(){return Ok(d);}let n=v.to_number()?.trunc();let n=if n.is_nan(){0.}else{n};Ok(if n<0.{(len as f64+n).max(0.)}else{n.min(len as f64)} as usize)};
            let start=norm(arg(&a,0),0)?;let end=norm(arg(&a,1),len)?;
            let mut out=vec![];for i in start..end{out.push(indexed(&this,i)?);}Ok(Js::holey_array(out))
        }),
        "concat" => method(name,move |a|{
            let mut out=items(&this)?;for v in a{if let Some(more)=v.array_items()?{out.extend(more);}else{out.push(Some(v));}}Ok(Js::holey_array(out))
        }),
        "map" | "forEach" | "filter" | "some" | "every" | "find" | "findIndex" | "flatMap" => {
            let kind=name.to_owned();method(name,move |a|{
                let len=array_length(&this)?;let f=arg(&a,0);require_callback(&f)?;let receiver=arg(&a,1);
                let mut out=vec![];
                for i in 0..len {
                    let value=if matches!(kind.as_str(),"find"|"findIndex"){Some(this.get(&i.to_string())?)}else{indexed(&this,i)?};
                    let Some(value)=value else {if kind=="map"{out.push(None);}continue;};
                    let result=f.call(receiver.clone(),vec![value.clone(),Js::Number(i as f64),this.clone()])?;
                    let hit=result.truthy();
                    match kind.as_str(){
                        "map"=>out.push(Some(result)),"filter" if hit=>out.push(Some(value)),
                        "flatMap"=>{if result.is_array(){flatten_into(&result,0.,&mut out)?;}else{out.push(Some(result));}},
                        "some" if hit=>return Ok(Js::Bool(true)),"every" if !hit=>return Ok(Js::Bool(false)),
                        "find" if hit=>return Ok(value),"findIndex" if hit=>return Ok(Js::Number(i as f64)),_=>{},
                    }
                }
                Ok(match kind.as_str(){"map"|"filter"|"flatMap"=>Js::holey_array(out),"some"=>Js::Bool(false),"every"=>Js::Bool(true),"find"=>Js::Undefined,"findIndex"=>Js::Number(-1.),_=>Js::Undefined})
            })
        },
        "reduce"=>method(name,move |a|{
            let len=array_length(&this)?;let f=arg(&a,0);require_callback(&f)?;let mut at=0;
            let mut acc=if a.len()>1{a[1].clone()}else{loop{if at>=len{return Err(Thrown::type_error("Reduce of empty array with no initial value"));}let next=indexed(&this,at)?;at+=1;if let Some(v)=next{break v;}}};
            while at<len {let i=at;at+=1;if let Some(v)=indexed(&this,i)?{acc=f.call(Js::Undefined,vec![acc,v,Js::Number(i as f64),this.clone()])?;}}
            Ok(acc)
        }),
        "flat"=>method(name,move |a|{
            let depth=arg(&a,0);let depth=if depth.is_undefined(){1.}else{let n=depth.to_number()?;if n.is_nan(){0.}else{n.trunc()}};
            let mut out=vec![];flatten_into(&this,depth,&mut out)?;Ok(Js::holey_array(out))
        }),
        "sort"=>method(name,move |a|{
            let f=arg(&a,0);if !f.is_undefined()&&!f.is_callable(){return Err(Thrown::type_error("The comparison function must be either a function or undefined"));}
            let len=array_length(&this)?;let mut values=vec![];for i in 0..len{if let Some(v)=indexed(&this,i)?{values.push(v);}}
            super::sorting::stable_sort(&mut values,|x,y|{
                if x.is_undefined()||y.is_undefined(){return Ok(x.is_undefined().cmp(&y.is_undefined()));}
                if f.is_callable(){let n=f.call(Js::Undefined,vec![x.clone(),y.clone()])?.to_number()?;Ok(n.partial_cmp(&0.).unwrap_or(std::cmp::Ordering::Equal))}else{Ok(x.to_js_string()?.cmp(&y.to_js_string()?))}
            })?;
            let count=values.len();for (i,value) in values.into_iter().enumerate(){this.set(&i.to_string(),value)?;}
            for i in count..len{this.delete(&super::value::utf16(&i.to_string()))?;}Ok(this.clone())
        }),
        _=>return None,
    })
}
fn flatten_into(source:&Js,depth:f64,out:&mut Vec<Option<Js>>)->Result<(),Thrown>{
    let len=array_length(source)?;
    for i in 0..len{if let Some(value)=indexed(source,i)?{if value.is_array()&&depth>0.{flatten_into(&value,depth-1.,out)?;}else{out.push(Some(value));}}}
    Ok(())
}

fn map_entries(this: &Js) -> Rc<RefCell<super::collections::Collection<(Js, Js)>>> {
    match this {
        Js::Map(m) => m.clone(),
        _ => Rc::default(),
    }
}

fn cached_iterator_member(this: &Js, key: &'static str, member: Js) -> Js {
    let cached = match this {
        Js::Map(entries) => entries
            .borrow()
            .iterator_methods
            .borrow()
            .get(key)
            .and_then(std::rc::Weak::upgrade),
        Js::Set(entries) => entries
            .borrow()
            .iterator_methods
            .borrow()
            .get(key)
            .and_then(std::rc::Weak::upgrade),
        _ => None,
    };
    if let Some(function) = cached {
        return Js::Function(function);
    }
    if let Js::Function(function) = &member {
        match this {
            Js::Map(entries) => {
                entries
                    .borrow()
                    .iterator_methods
                    .borrow_mut()
                    .insert(key, Rc::downgrade(function));
            }
            Js::Set(entries) => {
                entries
                    .borrow()
                    .iterator_methods
                    .borrow_mut()
                    .insert(key, Rc::downgrade(function));
            }
            _ => {}
        }
    }
    member
}

fn map_member(this: Js, name: &str) -> Option<Js> {
    let m = map_entries(&this);
    Some(match name {
        "get" => method(name, move |a| {
            let k = arg(&a, 0);
            Ok(m.borrow()
                .iter()
                .find(|(x, _)| x.same_value_zero(&k))
                .map(|(_, v)| v.clone())
                .unwrap_or(Js::Undefined))
        }),
        "has" => method(name, move |a| {
            let k = arg(&a, 0);
            Ok(Js::Bool(
                m.borrow().iter().any(|(x, _)| x.same_value_zero(&k)),
            ))
        }),
        "set" => method(name, move |a| {
            let k = arg(&a, 0);
            let k = if k.as_f64() == Some(0.0) {
                Js::Number(0.0)
            } else {
                k
            };
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
        "keys" | "values" | "entries" => {
            let kind = match name {
                "keys" => super::collections::Kind::Key,
                "values" => super::collections::Kind::Value,
                _ => super::collections::Kind::Entry,
            };
            let key = match name {
                "keys" => "keys",
                "values" => "values",
                _ => "entries",
            };
            cached_iterator_member(
                &this,
                key,
                Js::function(name, move |receiver, _| {
                    if !matches!(receiver, Js::Map(_)) {
                        return Err(Thrown::type_error(
                            "Map iterator method called on incompatible receiver",
                        ));
                    }
                    Ok(super::collections::collection_iterator(receiver, kind))
                }),
            )
        }
        "forEach" => method(name, move |a| {
            let f = arg(&a, 0);
            require_callback(&f)?;
            let receiver = arg(&a, 1);
            let mut position = 0;
            loop {
                let item = m
                    .borrow()
                    .following(position)
                    .map(|(at, entry)| (at, entry.clone()));
                let Some((at, (key, value))) = item else {
                    break;
                };
                position = at + 1;
                f.call(receiver.clone(), vec![value, key, this.clone()])?;
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
            let k = if k.as_f64() == Some(0.0) {
                Js::Number(0.0)
            } else {
                k
            };
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
        "values" | "keys" | "entries" => {
            let kind = if name == "entries" {
                super::collections::Kind::Entry
            } else {
                super::collections::Kind::Value
            };
            let key = if name == "entries" {
                "entries"
            } else {
                "values"
            };
            cached_iterator_member(
                &this,
                key,
                Js::function(
                    if name == "keys" { "values" } else { name },
                    move |receiver, _| {
                        if !matches!(receiver, Js::Set(_)) {
                            return Err(Thrown::type_error(
                                "Set iterator method called on incompatible receiver",
                            ));
                        }
                        Ok(super::collections::collection_iterator(receiver, kind))
                    },
                ),
            )
        }
        "forEach" => method(name, move |a| {
            let f = arg(&a, 0);
            require_callback(&f)?;
            let receiver = arg(&a, 1);
            let mut position = 0;
            loop {
                let item = s
                    .borrow()
                    .following(position)
                    .map(|(at, value)| (at, value.clone()));
                let Some((at, value)) = item else { break };
                position = at + 1;
                f.call(receiver.clone(), vec![value.clone(), value, this.clone()])?;
            }
            Ok(Js::Undefined)
        }),
        _ => return None,
    })
}

fn promise_member(p: Promise, name: &str) -> Option<Js> {
    let finally_runtime = p.runtime();
    let chain = move |on_ok: Js, on_err: Js| -> Result<Js, Thrown> {
        let p = p.clone();
        let rt = p.runtime();
        Ok(Js::Promise(Promise::spawn(
            &rt,
            Box::pin(async move {
                match p.wait().await {
                    Ok(v) if on_ok.is_callable() => on_ok.call(Js::Undefined, vec![v]),
                    Ok(v) => Ok(v),
                    Err(e) if on_err.is_callable() => on_err.call(Js::Undefined, vec![e.value]),
                    Err(e) => Err(e),
                }
            }),
        )))
    };
    Some(match name {
        "then" => method(name, move |a| chain(arg(&a, 0), arg(&a, 1))),
        "catch" => method(name, move |a| chain(Js::Undefined, arg(&a, 0))),
        "finally" => method(name, move |a| {
            let runtime_ok = finally_runtime.clone();
            let runtime_err = finally_runtime.clone();
            let f = arg(&a, 0);
            let g = f.clone();
            let on_ok = Js::function("", move |_, v| {
                if f.is_callable() {
                    let p = f.call(Js::Undefined, vec![])?;
                    let value = arg(&v, 0);
                    return Ok(chain_value(&runtime_ok, p, Ok(value)));
                }
                Ok(arg(&v, 0))
            });
            let on_err = Js::function("", move |_, v| {
                if g.is_callable() {
                    let p = g.call(Js::Undefined, vec![])?;
                    return Ok(chain_value(&runtime_err, p, Err(Thrown::new(arg(&v, 0)))));
                }
                Err(Thrown::new(arg(&v, 0)))
            });
            chain(on_ok, on_err)
        }),
        _ => return None,
    })
}

/// Await `p`, then settle with `outcome` (finally keeps the original result).
fn chain_value(rt: &Rc<dyn ExchangeRuntime>, p: Js, outcome: Result<Js, Thrown>) -> Js {
    let runtime = rt.clone();
    Js::Promise(Promise::spawn(
        rt,
        Box::pin(async move {
            await_js(&runtime, p).await?;
            outcome
        }),
    ))
}

#[cfg(test)]
#[path = "builtin_iterator_contract_tests.rs"]
mod accepted_contract_tests;
