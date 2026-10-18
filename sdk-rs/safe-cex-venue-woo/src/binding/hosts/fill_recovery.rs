//! WOOXExchange members ported in `crate::legacy_fill_recovery`, over js_runtime
//! values. The same Frame serves the private socket's execution-report members,
//! whose receiver is the socket (`this.parent` is the exchange).
use super::data_reads::{
    Callee, Iter, clear_timer, error_path, iterate, length, member, next, schedule,
};
use crate::{
    binding::{
        promise::await_js,
        rt::{Rt, Shared},
        value::{Js, Thrown, arg},
    },
    legacy_fill_recovery::{self as body, Binding, Map},
    legacy_metadata::Task,
};
use std::{
    cell::RefCell,
    cmp::Ordering,
    rc::{Rc, Weak},
};

pub(crate) enum Prepared {
    Call(Callee),
    /// `this.parent.emitter.emit('fill', ...)`: the event name leads the arguments.
    Fill(Callee),
    /// `super.dispose()`: the JS BaseExchange member, bridged to the Rust core.
    BaseDispose,
    Clear,
    SetTimeout,
}

pub(crate) struct Frame {
    pub(crate) rt: Result<Rt, Thrown>,
}

impl Frame {
    pub(crate) fn of(rt: Rt) -> Self {
        Self { rt: Ok(rt) }
    }
    /// A frame over `receiver` in the scope `weak` refers to.
    pub(crate) fn over(weak: &Weak<Shared>, receiver: &Js) -> Self {
        Self {
            rt: safe_cex_exchange::js_runtime::scope::scope_of(weak)
                .map(|rt| rt.with_this(receiver.clone())),
        }
    }
    fn rt(&self) -> Result<&Rt, Thrown> {
        self.rt.as_ref().map_err(Clone::clone)
    }
    fn this(&self) -> Result<Js, Thrown> {
        Ok(self.rt()?.this.clone())
    }
    fn own(&self, member: &str) -> Result<Callee, Thrown> {
        Callee::of(self.this()?, member, &format!("this.{member}"))
    }
    fn store(&self, member: &str) -> Result<Callee, Thrown> {
        Callee::of(
            self.this()?.get("store")?,
            member,
            &format!("this.store.{member}"),
        )
    }
    fn parent(&self, member: &str) -> Result<Callee, Thrown> {
        Callee::of(
            self.this()?.get("parent")?,
            member,
            &format!("this.parent.{member}"),
        )
    }
    fn collection(&self, m: Map) -> Result<(Js, &'static str), Thrown> {
        let key = match m {
            Map::Timers => "_fillPositionRefreshTimers",
            Map::Flight => "_fillPositionRefreshInFlight",
            Map::Expected => "_fillExpectedPositionSignatureBySymbol",
        };
        Ok((self.this()?.get(key)?, key))
    }
}

fn not_collection(key: &str, method: &str) -> Thrown {
    Thrown::type_error(&format!("this.{key}.{method} is not a function"))
}

fn collection_get(target: &Js, key: &str, k: &Js) -> Result<Js, Thrown> {
    match target {
        Js::Map(map) => Ok(map
            .borrow()
            .iter()
            .find(|(x, _)| x.same_value_zero(k))
            .map_or(Js::Undefined, |(_, v)| v.clone())),
        _ => Err(not_collection(key, "get")),
    }
}

fn collection_has(target: &Js, key: &str, k: &Js) -> Result<bool, Thrown> {
    match target {
        Js::Map(map) => Ok(map.borrow().iter().any(|(x, _)| x.same_value_zero(k))),
        Js::Set(set) => Ok(set.borrow().iter().any(|x| x.same_value_zero(k))),
        _ => Err(not_collection(key, "has")),
    }
}

/// `map.set(k, v)`, or `set.add(k)` for the in-flight Set.
fn collection_set(target: &Js, key: &str, k: Js, v: Js) -> Result<(), Thrown> {
    match target {
        Js::Map(map) => {
            let mut map = map.borrow_mut();
            match map.iter_mut().find(|(x, _)| x.same_value_zero(&k)) {
                Some(entry) => entry.1 = v,
                None => map.push((k, v)),
            }
            Ok(())
        }
        Js::Set(set) => {
            let mut set = set.borrow_mut();
            if !set.iter().any(|x| x.same_value_zero(&k)) {
                set.push(k);
            }
            Ok(())
        }
        _ => Err(not_collection(key, "set")),
    }
}

fn collection_delete(target: &Js, key: &str, k: &Js) -> Result<(), Thrown> {
    match target {
        Js::Map(map) => map.borrow_mut().retain(|(x, _)| !x.same_value_zero(k)),
        Js::Set(set) => set.borrow_mut().retain(|x| !x.same_value_zero(k)),
        _ => return Err(not_collection(key, "delete")),
    }
    Ok(())
}

fn collection_clear(target: &Js, key: &str) -> Result<(), Thrown> {
    match target {
        Js::Map(map) => map.borrow_mut().clear(),
        Js::Set(set) => set.borrow_mut().clear(),
        _ => return Err(not_collection(key, "clear")),
    }
    Ok(())
}

/// ICU root-collation approximation of `String.prototype.localeCompare` for ids:
/// punctuation < digits < letters, case-insensitive first, lowercase first on ties.
fn locale_compare(a: &str, b: &str) -> Ordering {
    let primary = |c: char| {
        let group = if c.is_alphabetic() {
            3
        } else if c.is_numeric() {
            2
        } else if c.is_whitespace() {
            0
        } else {
            1
        };
        (group, c.to_lowercase().collect::<String>())
    };
    let pa: Vec<_> = a.chars().map(primary).collect();
    let pb: Vec<_> = b.chars().map(primary).collect();
    pa.cmp(&pb).then_with(|| {
        let tertiary = |s: &str| s.chars().map(|c| c.is_uppercase()).collect::<Vec<_>>();
        tertiary(a).cmp(&tertiary(b))
    })
}

impl body::Host for Frame {
    type Value = Js;
    type Error = Thrown;
    type Prepared = Prepared;
    type MapIter = (Js, usize);
    type Iterator = Iter;
    type MapTarget = (Js, &'static str);
    fn prepare(&mut self, b: Binding) -> Result<Prepared, Thrown> {
        let rt = self.rt()?;
        Ok(match b {
            Binding::Signature => Prepared::Call(self.own("getPositionSignatureForSymbol")?),
            Binding::Refresh => Prepared::Call(self.own("refreshPositionForSymbolFromFill")?),
            Binding::Positions => Prepared::Call(self.own("fetchPositions")?),
            Binding::Schedule => Prepared::Call(self.own("scheduleFillPositionRefresh")?),
            Binding::Optimistic => {
                Prepared::Call(self.own("applyOptimisticOneWayPositionFromFill")?)
            }
            Binding::OffFill => Prepared::Call(self.own("off")?),
            Binding::BaseDispose => Prepared::BaseDispose,
            Binding::ClearTimeout | Binding::ClearInterval => Prepared::Clear,
            Binding::SetTimeout => Prepared::SetTimeout,
            Binding::MapLimit => Prepared::Call(self.parent("mapLimitOrder")?),
            Binding::MapAlgo => Prepared::Call(self.parent("mapAlgoOrder")?),
            Binding::AddOrder => Prepared::Call(self.store("addOrUpdateOrder")?),
            Binding::OrderStatus => Prepared::Call(self.store("updateOrderStatus")?),
            Binding::Fill => Prepared::Fill(Callee::of(
                self.this()?.get("parent")?.get("emitter")?,
                "emit",
                "this.parent.emitter.emit",
            )?),
            Binding::AddPosition => Prepared::Call(self.store("addOrUpdatePosition")?),
            Binding::UpdatePosition => Prepared::Call(self.store("updatePosition")?),
            Binding::AddPositions => Prepared::Call(self.store("addOrUpdatePositions")?),
            Binding::ParseTimestamp => Prepared::Call(self.own("parseTimestampToSeconds")?),
            Binding::Normalize => Prepared::Call(Callee::global(rt, "normalizeSymbol")?),
            Binding::PositionId => Prepared::Call(Callee::global(rt, "createPositionId")?),
            Binding::Emit => Prepared::Call(self.own("emitAndLogError")?),
        })
    }
    fn invoke(&mut self, c: Prepared, args: Vec<Js>) -> Result<Js, Thrown> {
        match c {
            Prepared::Call(callee) => callee.invoke(args),
            Prepared::Fill(callee) => {
                let mut all = vec![Js::str("fill")];
                all.extend(args);
                callee.invoke(all)
            }
            Prepared::BaseDispose => {
                self.rt()?.base().dispose()?;
                Ok(Js::Undefined)
            }
            Prepared::Clear => {
                clear_timer(self.rt()?, &arg(&args, 0));
                Ok(Js::Undefined)
            }
            Prepared::SetTimeout => Err(Thrown::type_error(
                "setTimeout is scheduled through timeout",
            )),
        }
    }
    fn call(&mut self, c: Prepared, args: Vec<Js>) -> Result<Task<Js, Thrown>, Thrown> {
        let result = self.invoke(c, args)?;
        Ok(Box::pin(await_js(&self.rt()?.exchange_runtime(), result)))
    }
    fn root(&mut self, k: &str) -> Result<Js, Thrown> {
        let this = self.this()?;
        match k {
            "positions" | "tickersBySymbol" => this.get("store")?.get(k),
            "isDisposed"
            | "_accountPositionMode"
            | "_fundingRefreshTimer"
            | "_fundingRefreshInFlight"
            | "handleFillForPositionSync" => this.get(k),
            "MAX_FILL_POSITION_REFRESH_ATTEMPTS" | "FILL_POSITION_REFRESH_BASE_DELAY_MS" => {
                self.rt()?.global("WOOXExchange")?.get(k)
            }
            other => Err(Thrown::error(
                "Error",
                &format!("WOO fill-recovery host has no root `{other}`"),
            )),
        }
    }
    fn set_root(&mut self, k: &str, v: Js) -> Result<(), Thrown> {
        self.this()?.set(k, v)
    }
    fn field(&mut self, v: &Js, k: &str) -> Result<Js, Thrown> {
        v.get(k)
    }
    fn key_field(&mut self, v: &Js, k: Vec<u16>) -> Result<Js, Thrown> {
        v.get_key(&k)
    }
    fn v(&mut self, v: &Js, k: &str) -> Result<Js, Thrown> {
        Callee::global(self.rt()?, "v")?.invoke(vec![v.clone(), Js::str(k)])
    }
    fn length(&mut self, v: &Js) -> Result<usize, Thrown> {
        length(v)
    }
    fn index(&mut self, v: &Js, i: usize) -> Result<Js, Thrown> {
        v.get(&i.to_string())
    }
    fn has_index(&mut self, v: &Js, i: usize) -> Result<bool, Thrown> {
        Ok(v.has_own(&i.to_string()))
    }
    fn is_array(&self, v: &Js) -> bool {
        v.is_array()
    }
    fn truthy(&self, v: &Js) -> bool {
        v.truthy()
    }
    fn nullish(&self, v: &Js) -> bool {
        v.is_nullish()
    }
    fn is_undefined(&self, v: &Js) -> bool {
        v.is_undefined()
    }
    fn equals(&self, v: &Js, s: &str) -> bool {
        v.strict_equals(&Js::str(s))
    }
    fn same(&self, a: &Js, b: &Js) -> bool {
        a.strict_equals(b)
    }
    fn number(&self, n: f64) -> Js {
        Js::Number(n)
    }
    fn boolean(&self, b: bool) -> Js {
        Js::Bool(b)
    }
    fn string(&self, s: Vec<u16>) -> Js {
        Js::utf16(&s)
    }
    fn undefined(&self) -> Js {
        Js::Undefined
    }
    fn object(&self, f: Vec<(&'static str, Js)>) -> Js {
        Js::object(f)
    }
    fn array(&self, v: Vec<Js>) -> Js {
        Js::array(v)
    }
    fn text(&mut self, v: &Js) -> Result<Vec<u16>, Thrown> {
        Ok(v.to_js_string()?.to_vec())
    }
    fn number_coerce(&mut self, v: &Js) -> Result<f64, Thrown> {
        v.to_number()
    }
    fn operator_number(&mut self, v: &Js) -> Result<f64, Thrown> {
        v.to_number()
    }
    fn add(&mut self, a: Js, b: Js) -> Result<Js, Thrown> {
        safe_cex_exchange::js_runtime::ops::add(&a, &b)
    }
    fn sort_by_id(&mut self, v: Vec<Js>) -> Result<Vec<Js>, Thrown> {
        let mut keyed = vec![];
        for p in v {
            let id = p.get("id")?;
            let Js::String(text) = &id else {
                return Err(Thrown::type_error("a.id.localeCompare is not a function"));
            };
            keyed.push((String::from_utf16_lossy(text), p));
        }
        keyed.sort_by(|(a, _), (b, _)| locale_compare(a, b));
        Ok(keyed.into_iter().map(|(_, p)| p).collect())
    }
    fn prepare_map_set(&mut self, m: Map) -> Result<(Js, &'static str), Thrown> {
        self.collection(m)
    }
    fn map_set_prepared(
        &mut self,
        (target, key): (Js, &'static str),
        k: Js,
        v: Js,
    ) -> Result<(), Thrown> {
        collection_set(&target, key, k, v)
    }
    fn map_get(&mut self, m: Map, k: &Js) -> Result<Js, Thrown> {
        let (target, key) = self.collection(m)?;
        collection_get(&target, key, k)
    }
    fn map_has(&mut self, m: Map, k: &Js) -> Result<bool, Thrown> {
        let (target, key) = self.collection(m)?;
        collection_has(&target, key, k)
    }
    fn map_set(&mut self, m: Map, k: Js, v: Js) -> Result<(), Thrown> {
        let (target, key) = self.collection(m)?;
        collection_set(&target, key, k, v)
    }
    fn map_delete(&mut self, m: Map, k: &Js) -> Result<(), Thrown> {
        let (target, key) = self.collection(m)?;
        collection_delete(&target, key, k)
    }
    fn map_clear(&mut self, m: Map) -> Result<(), Thrown> {
        let (target, key) = self.collection(m)?;
        collection_clear(&target, key)
    }
    fn map_iterator(&mut self, m: Map) -> Result<(Js, usize), Thrown> {
        let (target, key) = self.collection(m)?;
        match target {
            Js::Map(_) => Ok((target, 0)),
            _ => Err(not_collection(key, "forEach")),
        }
    }
    fn map_next(&mut self, (target, i): &mut (Js, usize)) -> Result<Option<Js>, Thrown> {
        let Js::Map(map) = target else {
            return Ok(None);
        };
        let entry = map.borrow().get(*i).cloned();
        *i += 1;
        Ok(entry.map(|(_, v)| v))
    }
    fn iterator(&mut self, v: Js) -> Result<Iter, Thrown> {
        iterate(v)
    }
    fn next(&mut self, i: &mut Iter) -> Result<Option<Js>, Thrown> {
        Ok(next(i))
    }
    fn close_iterator(&mut self, _: &mut Iter) -> Result<(), Thrown> {
        Ok(())
    }
    fn timeout(
        &mut self,
        _: Prepared,
        delay: f64,
        callback: Rc<dyn Fn() -> Result<Task<Js, Thrown>, Thrown>>,
    ) -> Result<Js, Thrown> {
        Ok(schedule(self.rt()?, delay, false, callback))
    }
    fn error_field(&mut self, e: &Thrown, p: &[&str]) -> Result<Js, Thrown> {
        error_path(e, p)
    }
    fn emit_error(&mut self, c: Prepared, m: Js) -> Result<(), Thrown> {
        self.invoke(c, vec![m])?;
        Ok(())
    }
    fn order_side(&mut self, v: Js) -> Result<Js, Thrown> {
        self.rt()?.global("ORDER_SIDE")?.get_key(&v.to_js_string()?)
    }
}

fn host(rt: Rt) -> Rc<RefCell<Frame>> {
    Rc::new(RefCell::new(Frame::of(rt)))
}

pub(crate) fn bind(rt: &Rt, name: &str) -> Option<Result<Js, Thrown>> {
    let f = match name {
        "getPositionSignatureForSymbol" => member(rt, name, |rt, a| {
            body::get_position_signature_for_symbol(&mut Frame::of(rt), arg(&a, 0))
        }),
        "scheduleFillPositionRefresh" => member(rt, name, |rt, a| {
            body::schedule_fill_position_refresh(host(rt), arg(&a, 0), arg(&a, 1).to_number()?)?;
            Ok(Js::Undefined)
        }),
        "refreshPositionForSymbolFromFill" => member(rt, name, |rt, a| {
            let task: Task<Js, Thrown> = match arg(&a, 1).to_number() {
                Ok(attempt) => body::refresh_position_for_symbol_from_fill(
                    host(rt.clone()),
                    arg(&a, 0),
                    attempt,
                ),
                Err(e) => Box::pin(std::future::ready(Err(e))),
            };
            Ok(rt.spawn(task))
        }),
        "applyOptimisticOneWayPositionFromFill" => member(rt, name, |rt, a| {
            Ok(Js::Bool(body::apply_optimistic_one_way_position_from_fill(
                &mut Frame::of(rt),
                arg(&a, 0),
            )?))
        }),
        "handleFillForPositionSync" => member(rt, name, |rt, a| {
            body::handle_fill_for_position_sync(&mut Frame::of(rt), arg(&a, 0))?;
            Ok(Js::Undefined)
        }),
        "dispose" => member(rt, name, |rt, _| {
            body::dispose(&mut Frame::of(rt))?;
            Ok(Js::Undefined)
        }),
        _ => return None,
    };
    Some(Ok(f))
}

pub(crate) fn register(_g: &Js, _weak: Weak<Shared>) {}
