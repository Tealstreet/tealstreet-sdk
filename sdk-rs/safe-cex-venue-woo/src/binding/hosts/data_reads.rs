//! WOOXExchange members ported in `crate::legacy_data_reads`, over js_runtime values.
use crate::{
    binding::{
        promise::{Promise, await_js},
        rt::{Rt, Shared},
        value::{Js, Thrown, arg},
    },
    legacy_data_reads::{self as body, Binding, Transport},
    legacy_metadata::Task,
};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

/// A callee resolved when the source expression evaluates it, with its receiver.
pub(crate) struct Callee {
    pub(crate) f: Js,
    pub(crate) this: Js,
    pub(crate) path: String,
}

impl Callee {
    pub(crate) fn of(owner: Js, member: &str, path: &str) -> Result<Self, Thrown> {
        Ok(Self {
            f: owner.get(member)?,
            this: owner,
            path: path.into(),
        })
    }
    pub(crate) fn global(rt: &Rt, name: &str) -> Result<Self, Thrown> {
        Ok(Self {
            f: rt.global(name)?,
            this: Js::Undefined,
            path: name.into(),
        })
    }
    pub(crate) fn invoke(&self, args: Vec<Js>) -> Result<Js, Thrown> {
        if !self.f.is_callable() {
            return Err(Thrown::type_error(&format!(
                "{} is not a function",
                self.path
            )));
        }
        self.f.call(self.this.clone(), args)
    }
}

/// A member function capturing its receiver; the scope is held weakly so the
/// function stored on the receiver does not keep the shared state alive.
pub(crate) fn member(
    rt: &Rt,
    name: &str,
    body: impl Fn(Rt, Vec<Js>) -> Result<Js, Thrown> + 'static,
) -> Js {
    let weak = Rc::downgrade(&rt.shared);
    let this = rt.this.clone();
    Js::function(name, move |_, args| {
        let shared = weak
            .upgrade()
            .ok_or_else(|| Thrown::error("Error", "exchange runtime disposed"))?;
        body(
            Rt {
                shared,
                this: this.clone(),
            },
            args,
        )
    })
}

/// `a?.b?.c` over a thrown value.
pub(crate) fn error_path(e: &Thrown, path: &[&str]) -> Result<Js, Thrown> {
    let mut v = e.value.clone();
    for k in path {
        if v.is_nullish() {
            return Ok(Js::Undefined);
        }
        v = v.get(k)?;
    }
    Ok(v)
}

pub(crate) fn length(v: &Js) -> Result<usize, Thrown> {
    let n = v.get("length")?.to_number()?;
    Ok(if n.is_finite() && n > 0.0 {
        n as usize
    } else {
        0
    })
}

/// A for...of iterator: arrays iterate live, re-reading length each step.
pub(crate) struct Iter(safe_cex_exchange::js_runtime::iterator::IteratorRecord);
pub(crate) fn iterate(v:Js)->Result<Iter,Thrown>{Ok(Iter(safe_cex_exchange::js_runtime::iterator::IteratorRecord::get(&v)?))}
pub(crate) fn next(it:&mut Iter)->Result<Option<Js>,Thrown>{it.0.step_value()}


/// setTimeout / setInterval whose handle `clear_timer` cancels; a rejected
/// callback task is reported like an unhandled rejection.
pub(crate) fn schedule(
    rt: &Rt,
    delay: f64,
    repeat: bool,
    callback: Rc<dyn Fn() -> Result<Task<Js, Thrown>, Thrown>>,
) -> Js {
    let runtime = rt.shared.runtime.clone();
    let ms = if delay.is_finite() && delay > 0.0 {
        delay as u64
    } else {
        0
    };
    let handle = Js::object_with_class("Timeout", vec![]);
    let weak: Weak<Shared> = Rc::downgrade(&rt.shared);
    let own = handle.clone();
    let source = if repeat { "setInterval" } else { "setTimeout" };
    let task = runtime.clone().spawn_local(Box::pin(async move {
        loop {
            if let Err(e) = runtime.sleep(ms).await {
                runtime.report_exception(&e, &serde_json::json!({ "source": source }).into());
                return;
            }
            match callback() {
                Ok(task) => {
                    let reporter = runtime.clone();
                    let exchange_runtime: Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime> =
                        runtime.clone();
                    Promise::spawn(
                        &exchange_runtime,
                        Box::pin(async move {
                            if let Err(e) = task.await {
                                reporter.report_exception(
                                    &e.to_exchange_error(),
                                    &serde_json::json!({ "source": source }).into(),
                                );
                            }
                            Ok(Js::Undefined)
                        }),
                    );
                }
                Err(e) => runtime.report_exception(
                    &e.to_exchange_error(),
                    &serde_json::json!({ "source": source }).into(),
                ),
            }
            if !repeat {
                if let Some(shared) = weak.upgrade() {
                    shared
                        .timers
                        .borrow_mut()
                        .retain(|(h, _)| !h.same_reference(&own));
                }
                return;
            }
        }
    }));
    rt.shared.timers.borrow_mut().push((handle.clone(), task));
    handle
}

/// clearTimeout / clearInterval: cancels the scheduled task, not just its handle.
pub(crate) fn clear_timer(rt: &Rt, handle: &Js) {
    let mut timers = rt.shared.timers.borrow_mut();
    if let Some(at) = timers.iter().position(|(h, _)| h.same_reference(handle)) {
        let (_, task) = timers.remove(at);
        task.cancel();
    }
}

pub(crate) enum Prepared {
    Call(Callee),
    /// `client.get` captured with its client.
    Get(Callee),
    Interval,
}

pub(crate) struct Frame {
    pub(crate) rt: Rt,
}

impl Frame {
    fn this(&self) -> &Js {
        &self.rt.this
    }
    fn own(&self, member: &str) -> Result<Callee, Thrown> {
        Callee::of(self.this().clone(), member, &format!("this.{member}"))
    }
    fn store(&self, member: &str) -> Result<Callee, Thrown> {
        Callee::of(
            self.this().get("store")?,
            member,
            &format!("this.store.{member}"),
        )
    }
    fn class_constant(&self, key: &str) -> Result<Js, Thrown> {
        self.rt.global("WOOXExchange")?.get(key)
    }
}

fn callee(p: Prepared) -> Callee {
    match p {
        Prepared::Call(c) | Prepared::Get(c) => c,
        Prepared::Interval => Callee {
            f: Js::Undefined,
            this: Js::Undefined,
            path: "setInterval".into(),
        },
    }
}

impl body::Host for Frame {
    type Value = Js;
    type Error = Thrown;
    type Prepared = Prepared;
    type Iterator = Iter;
    fn prepare(&mut self, binding: Binding) -> Result<Prepared, Thrown> {
        Ok(match binding {
            Binding::Get(transport) => {
                let key = match transport {
                    Transport::Main => "xhr",
                    Transport::Proxy => "_proxyEnforcedXhr",
                };
                Prepared::Get(Callee::of(
                    self.this().get(key)?,
                    "get",
                    &format!("this.{key}.get"),
                )?)
            }
            Binding::RefreshFunding => Prepared::Call(self.own("refreshFundingRates")?),
            Binding::PositionMode => Prepared::Call(self.own("getPositionMode")?),
            Binding::OpenPositions => Prepared::Call(self.own("fetchOpenPositions")?),
            Binding::Metadata => Prepared::Call(self.own("fetchPositionMetadata")?),
            Binding::MetadataSymbol => Prepared::Call(self.own("fetchPositionMetadataForSymbol")?),
            Binding::ParseFunding => Prepared::Call(self.own("parseFiniteFundingNumber")?),
            Binding::Normalize => Prepared::Call(Callee::global(&self.rt, "normalizeSymbol")?),
            Binding::Reverse => Prepared::Call(Callee::global(&self.rt, "reverseSymbol")?),
            Binding::CandleStart => Prepared::Call(Callee::global(&self.rt, "getCandleStartTime")?),
            Binding::Now => Prepared::Call(Callee::of(self.rt.global("Date")?, "now", "Date.now")?),
            Binding::ListenCandle | Binding::ListenBook => {
                let name = if binding == Binding::ListenCandle {
                    "listenOHLCV"
                } else {
                    "listenOrderBook"
                };
                Prepared::Call(Callee::of(
                    self.this().get("publicWebsocket")?,
                    name,
                    &format!("this.publicWebsocket.{name}"),
                )?)
            }
            Binding::Emit => Prepared::Call(self.own("emitAndLogError")?),
            Binding::UpdateTicker => Prepared::Call(self.store("updateTicker")?),
            Binding::PositionId => Prepared::Call(Callee::global(&self.rt, "createPositionId")?),
            Binding::SetInterval => Prepared::Interval,
            Binding::Flatten => Prepared::Call(Callee::global(&self.rt, "flatten")?),
        })
    }
    fn call(&mut self, call: Prepared, args: Vec<Js>) -> Result<Task<Js, Thrown>, Thrown> {
        let result = callee(call).invoke(args)?;
        Ok(Box::pin(await_js(&self.rt.exchange_runtime(), result)))
    }
    fn invoke(&mut self, call: Prepared, args: Vec<Js>) -> Result<Js, Thrown> {
        callee(call).invoke(args)
    }
    fn get(
        &mut self,
        call: Prepared,
        url: Js,
        params: Option<Js>,
    ) -> Result<Task<Js, Thrown>, Thrown> {
        let mut args = vec![url];
        if let Some(params) = params {
            args.push(Js::object(vec![("params", params)]));
        }
        let response = callee(call).invoke(args)?;
        Ok(Box::pin(await_js(&self.rt.exchange_runtime(), response)))
    }
    fn root(&mut self, key: &str) -> Result<Js, Thrown> {
        match key {
            "isDisposed" | "_fundingRefreshTimer" | "_fundingRefreshInFlight" => {
                self.this().get(key)
            }
            "FUNDING_REFRESH_INTERVAL_MS" => self.class_constant(key),
            "tickersBySymbol" | "orders" => self.this().get("store")?.get(key),
            "timeframeToSeconds" => self.rt.global(key),
            other => Err(Thrown::error(
                "Error",
                &format!("WOO data-read host has no root `{other}`"),
            )),
        }
    }
    fn set_root(&mut self, key: &str, v: Js) -> Result<(), Thrown> {
        self.this().set(key, v)
    }
    fn field(&mut self, v: &Js, key: &str) -> Result<Js, Thrown> {
        v.get(key)
    }
    fn key_field(&mut self, v: &Js, key: Vec<u16>) -> Result<Js, Thrown> {
        v.get_key(&key)
    }
    fn index(&mut self, v: &Js, i: usize) -> Result<Js, Thrown> {
        v.get(&i.to_string())
    }
    fn length(&mut self, v: &Js) -> Result<usize, Thrown> {
        length(v)
    }
    fn has_index(&mut self, v: &Js, i: usize) -> Result<bool, Thrown> {
        Ok(v.has_own(&i.to_string()))
    }
    fn iterator(&mut self, v: Js) -> Result<Iter, Thrown> {
        iterate(v)
    }
    fn next(&mut self, i: &mut Iter) -> Result<Option<Js>, Thrown> {
        next(i)
    }
    fn close_iterator(&mut self, _: &mut Iter) -> Result<(), Thrown> {
        Ok(())
    }
    fn await_element(&mut self, v: Js) -> Task<Js, Thrown> {
        Box::pin(await_js(&self.rt.exchange_runtime(), v))
    }
    fn is_undefined(&self, v: &Js) -> bool {
        v.is_undefined()
    }
    fn truthy(&self, v: &Js) -> bool {
        v.truthy()
    }
    fn nullish(&self, v: &Js) -> bool {
        v.is_nullish()
    }
    fn is_object(&self, v: &Js) -> bool {
        v.type_of() == "object"
    }
    fn is_string(&self, v: &Js) -> bool {
        v.is_string()
    }
    fn is_array(&self, v: &Js) -> bool {
        v.is_array()
    }
    fn strict_true(&self, v: &Js) -> bool {
        matches!(v, Js::Bool(true))
    }
    fn equals(&self, v: &Js, s: &str) -> bool {
        v.strict_equals(&Js::str(s))
    }
    fn same(&self, a: &Js, b: &Js) -> bool {
        a.strict_equals(b)
    }
    fn same_zero(&self, a: &Js, b: &Js) -> bool {
        a.same_value_zero(b)
    }
    fn number_coerce(&mut self, v: &Js) -> Result<f64, Thrown> {
        v.to_number()
    }
    fn operator_number(&mut self, v: &Js) -> Result<f64, Thrown> {
        v.to_number()
    }
    fn text(&mut self, v: &Js) -> Result<Vec<u16>, Thrown> {
        Ok(v.to_js_string()?.to_vec())
    }
    fn starts_with(&mut self, v: &Js, s: &str) -> Result<bool, Thrown> {
        let prefix: Vec<u16> = s.encode_utf16().collect();
        Ok(v.to_js_string()?.starts_with(&prefix))
    }
    fn number(&self, n: f64) -> Js {
        Js::Number(n)
    }
    fn boolean(&self, b: bool) -> Js {
        Js::Bool(b)
    }
    fn undefined(&self) -> Js {
        Js::Undefined
    }
    fn string(&self, s: Vec<u16>) -> Js {
        Js::utf16(&s)
    }
    fn object(&self, fields: Vec<(&'static str, Js)>) -> Js {
        Js::object(fields)
    }
    fn object_spread(
        &mut self,
        before: Vec<(&'static str, Js)>,
        v: Js,
        after: Vec<(&'static str, Js)>,
    ) -> Result<Js, Thrown> {
        let out = Js::object(before);
        if !v.is_nullish() {
            for (k, value) in v.own_entries()? {
                out.define_key(&k, value)?;
            }
        }
        for (k, value) in after {
            out.define(k, value)?;
        }
        Ok(out)
    }
    fn array(&self, v: Vec<Option<Js>>) -> Js {
        Js::holey_array(v)
    }
    fn type_error(&mut self, message: &str) -> Thrown {
        Thrown::type_error(message)
    }
    fn error(&mut self, message: &str) -> Thrown {
        Thrown::error("Error", message)
    }
    fn error_field(&mut self, e: &Thrown, path: &[&str]) -> Result<Js, Thrown> {
        error_path(e, path)
    }
    fn emit(&mut self, call: Prepared, e: Option<&Thrown>, message: Js) -> Result<(), Thrown> {
        let mut args = vec![];
        if let Some(e) = e {
            args.push(e.value.clone());
        }
        args.push(message);
        callee(call).invoke(args)?;
        Ok(())
    }
    fn get_callable(&mut self) -> Result<bool, Thrown> {
        Ok(self.this().get("xhr")?.get("get")?.is_callable())
    }
    fn interval(
        &mut self,
        _: Prepared,
        delay: Js,
        callback: Rc<dyn Fn() -> Result<Task<Js, Thrown>, Thrown>>,
    ) -> Result<Js, Thrown> {
        Ok(schedule(&self.rt, delay.to_number()?, true, callback))
    }
}

fn host(rt: Rt) -> Rc<RefCell<Frame>> {
    Rc::new(RefCell::new(Frame { rt }))
}

pub(crate) fn bind(rt: &Rt, name: &str) -> Option<Result<Js, Thrown>> {
    let f = match name {
        "startFundingRatesRefresh" => member(rt, name, |rt, _| {
            body::start_funding_rates_refresh(host(rt))?;
            Ok(Js::Undefined)
        }),
        "refreshFundingRates" => member(rt, name, |rt, _| {
            Ok(rt.spawn(body::refresh_funding_rates(host(rt.clone()))))
        }),
        "fetchMarketIocTouchPrice" => member(rt, name, |rt, a| {
            let task = body::fetch_market_ioc_touch_price(host(rt.clone()), arg(&a, 0), arg(&a, 1));
            Ok(rt.spawn(task))
        }),
        "fetchOHLCV" => member(rt, name, |rt, a| {
            Ok(rt.spawn(body::fetch_ohlcv(host(rt.clone()), arg(&a, 0))))
        }),
        "listenOHLCV" => member(rt, name, |rt, a| {
            body::listen_ohlcv(&mut Frame { rt }, arg(&a, 0), arg(&a, 1))
        }),
        "listenOrderBook" => member(rt, name, |rt, a| {
            body::listen_order_book(&mut Frame { rt }, arg(&a, 0), arg(&a, 1))
        }),
        "fetchPositionMetadata" => member(rt, name, |rt, a| {
            Ok(rt.spawn(body::fetch_position_metadata(host(rt.clone()), arg(&a, 0))))
        }),
        "fetchPositions" => member(rt, name, |rt, a| {
            Ok(rt.spawn(body::fetch_positions(host(rt.clone()), Some(arg(&a, 0)))))
        }),
        _ => return None,
    };
    Some(Ok(f))
}

/// `getCandleStartTime` (utils/getCandleStartTime.ts) over `timeframeToSeconds`.
fn candle_start_time(timeframe: &str, time: f64) -> f64 {
    let epoch = if time > 1_000_000_000_000.0 {
        (time / 1000.0).floor()
    } else {
        time
    };
    let seconds = safe_cex_types::TIMEFRAME_TO_SECONDS
        .get(timeframe)
        .copied()
        .unwrap_or(f64::NAN);
    let start = (epoch / seconds).floor() * seconds;
    if time > 1_000_000_000_000.0 {
        start * 1000.0
    } else {
        start
    }
}

pub(crate) fn register(g: &Js, _weak: Weak<Shared>) {
    let _ = g.define(
        "timeframeToSeconds",
        Js::object(
            safe_cex_types::TIMEFRAME_TO_SECONDS
                .iter()
                .map(|(k, v)| (*k, Js::Number(*v)))
                .collect(),
        ),
    );
    let _ = g.define(
        "getCandleStartTime",
        Js::function("getCandleStartTime", |_, a| {
            let timeframe = String::from_utf16_lossy(&arg(&a, 0).to_js_string()?);
            Ok(Js::Number(candle_start_time(
                &timeframe,
                arg(&a, 1).to_number()?,
            )))
        }),
    );
}
