//! The OKX scope and the closure tables the ported bodies take, built from it.
//! Capture names are TS expressions: `this.a.b` binds the member to its owner,
//! `owner.member:ENDPOINT` is `(...a) => owner.member(ENDPOINTS.ENDPOINT, ...a)`,
//! `this.member:unbound` reads the member without binding.
use super::intrinsics::{IntrinsicHost, intrinsic};
use crate::{
    family_lifecycle::LifecycleOps,
    history_stream_api::RuntimeOps,
    market_account_reads::{MarketAccountOps, ReadAwait, ReadMapper},
    order_composition::CompositionOps,
    order_mutations::OrderMutationOps,
    order_reads::OrderReadOps,
    websocket_account_events::SocketEventOps,
};
use safe_cex_exchange::js_runtime::{
    promise::await_js,
    scope::{Scope, ScopeShared, bound_member},
    value::{Js, Thrown, arg},
};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use std::rc::Rc;

/// Venue-only shared state: the typed constructor inputs.
pub struct OkxExtra {
    pub name: ExchangeName,
    pub options: ExchangeOptions,
}

pub(crate) type OkxScope = Scope<OkxExtra>;
pub(crate) type OkxShared = ScopeShared<OkxExtra>;

impl IntrinsicHost for OkxScope {
    fn this(&self) -> Js {
        self.this.clone()
    }
    fn global(&self, name: &str) -> Result<Js, Thrown> {
        Scope::global(self, name)
    }
    fn now_ms(&self) -> f64 {
        self.now()
    }
    fn initialize_okx_fields(&self, receiver: &Js) -> Result<(), Thrown> {
        super::members::initialize_fields(&self.with_this(receiver.clone()))
    }
}

fn callable(f: &Js) -> Result<(), Thrown> {
    if f.is_callable() {
        Ok(())
    } else {
        Err(Thrown::type_error(&format!("{} is not a function", f.to_display())))
    }
}

fn items(v: &Js) -> Result<Vec<Js>, Thrown> {
    v.array_values()
        .ok_or_else(|| Thrown::type_error(&format!("{} is not iterable", v.to_display())))
}

/// Resolve a capture name against the scope.
pub(crate) fn capture(scope: &OkxScope, name: &str) -> Result<Js, Thrown> {
    if name == "this" {
        return Ok(scope.this.clone());
    }
    if let Some((path, tail)) = name.split_once(':') {
        if tail == "unbound" {
            return scope.path(path.strip_prefix("this.").unwrap_or(path));
        }
        let path = path.strip_prefix("this.").unwrap_or(path);
        let (owner, member) = scope.owner_and_member(path)?;
        callable(&member)?;
        let endpoint = if tail.contains('.') { tail.to_owned() } else { format!("ENDPOINTS.{tail}") };
        let endpoint = Scope::global(scope, &endpoint)?;
        return Ok(Js::function(path, move |_, args| {
            let mut full = vec![endpoint.clone()];
            full.extend(args);
            member.call(owner.clone(), full)
        }));
    }
    if let Some(path) = name.strip_prefix("this.") {
        let (owner, member) = scope.owner_and_member(path)?;
        if !member.is_callable() {
            return Ok(member);
        }
        return Ok(Js::function(path, move |_, args| member.call(owner.clone(), args)));
    }
    Scope::global(scope, name)
}

fn call_js(f: &Js, args: Vec<Js>) -> Result<Js, Thrown> {
    callable(f)?;
    f.call(Js::Undefined, args)
}

fn js_callback(f: ReadMapper<Js, Thrown>) -> Js {
    Js::function("", move |_, args| f(arg(&args, 0)))
}

fn await_value(v: Js) -> ReadAwait<Js, Thrown> {
    Box::pin(await_js(v))
}

pub(crate) fn market_ops(scope: &OkxScope) -> Rc<MarketAccountOps<Js, Thrown>> {
    let s = scope.clone();
    let cap = s.clone();
    let intr = s.clone();
    let rt = s.clone();
    Rc::new(MarketAccountOps {
        get: Rc::new(|v, k| v.get(k)),
        set: Rc::new(|v, k, x| v.set(k, x)),
        capture: Rc::new(move |name| capture(&cap, name)),
        member: Rc::new(|v, k| bound_member(&v, k)),
        invoke: Rc::new(|f, args| call_js(&f, args)),
        intrinsic: Rc::new(move |name, args| intrinsic(&intr, name, args)),
        truthy: Rc::new(Js::truthy),
        nullish: Rc::new(Js::is_nullish),
        equal: Rc::new(|a, b| a.strict_equals(b)),
        literal: Rc::new(Js::str),
        number: Rc::new(Js::Number),
        boolean: Rc::new(Js::Bool),
        undefined: Rc::new(|| Js::Undefined),
        array: Rc::new(Js::array),
        object: Rc::new(Js::object),
        map: Rc::new(|v, f| Ok(Js::array(items(&v)?.into_iter().map(|x| f(x)).collect::<Result<_, _>>()?))),
        bind_first: Rc::new(|v, f| f(items(&v)?.into_iter().next().unwrap_or(Js::Undefined))),
        flat_map: Rc::new(|v, f| {
            let mut out = vec![];
            for x in items(&v)? {
                let r = f(x)?;
                match r.array_values() {
                    Some(inner) => out.extend(inner),
                    None => out.push(r),
                }
            }
            Ok(Js::array(out))
        }),
        filter: Rc::new(|v, f| {
            let mut out = vec![];
            for x in items(&v)? {
                if f(x.clone())? {
                    out.push(x);
                }
            }
            Ok(Js::array(out))
        }),
        reduce: Rc::new(|v, init, f| items(&v)?.into_iter().try_fold(init, |acc, x| f(acc, x))),
        for_of: Rc::new(|v, f| items(&v)?.into_iter().try_for_each(|x| f(x))),
        sum_by: Rc::new(|sum, values, mapper| {
            let mut args = vec![values];
            if let Some(m) = mapper {
                args.push(js_callback(m));
            }
            call_js(&sum, args)
        }),
        // p-iteration `map(array, async fn)`: every callback starts, results keep order.
        parallel_map: Rc::new(move |map, v, f| {
            let started: Result<Vec<ReadAwait<Js, Thrown>>, Thrown> =
                items(&v).map(|xs| xs.into_iter().map(|x| f(x)).collect());
            let _ = map;
            let rt = rt.clone();
            Box::pin(async move {
                let mut out = vec![];
                for fut in started? {
                    out.push(fut.await?);
                }
                let _ = &rt;
                Ok(Js::array(out))
            })
        }),
        some: Rc::new(|v, f| {
            for x in items(&v)? {
                if f(x)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }),
        await_value: Rc::new(await_value),
        error_value: Rc::new(|e| e.value.clone()),
        new_error: Rc::new(|ctor, message| Ok(Thrown::new(call_js(&ctor, vec![message])?))),
    })
}

pub(crate) fn lifecycle_ops(scope: &OkxScope) -> Rc<LifecycleOps<Js, Thrown>> {
    let s = scope.clone();
    Rc::new(LifecycleOps {
        value: market_ops(scope),
        spawn: Rc::new(move |fut| Ok(s.spawn(fut))),
        callback: Rc::new(|f| Js::function("", move |_, args| f(args))),
    })
}

pub(crate) fn runtime_ops(scope: &OkxScope) -> Rc<RuntimeOps<Js, Thrown>> {
    Rc::new(RuntimeOps {
        value: market_ops(scope),
        callback: Rc::new(|f| Js::function("", move |_, args| f(args))),
    })
}

pub(crate) fn socket_ops(scope: &OkxScope) -> Rc<SocketEventOps<Js, Thrown>> {
    Rc::new(SocketEventOps {
        value: market_ops(scope),
        callback: Rc::new(|f| Js::function("", move |_, args| f(arg(&args, 0)))),
    })
}

pub(crate) fn mutation_ops(scope: &OkxScope) -> Rc<OrderMutationOps<Js, Thrown>> {
    let s = scope.clone();
    Rc::new(OrderMutationOps {
        value: market_ops(scope),
        has: Rc::new(|k, v| Ok(v.has_own(k))),
        filter: Rc::new(|v, f| {
            let mut out = vec![];
            for x in items(&v)? {
                if f(x.clone())?.truthy() {
                    out.push(x);
                }
            }
            Ok(Js::array(out))
        }),
        find: Rc::new(|v, f| {
            for x in items(&v)? {
                if f(x.clone())?.truthy() {
                    return Ok(x);
                }
            }
            Ok(Js::Undefined)
        }),
        optional_find: Rc::new(|v, f| {
            if v.is_nullish() {
                return Ok(Js::Undefined);
            }
            for x in items(&v)? {
                if f(x.clone())?.truthy() {
                    return Ok(x);
                }
            }
            Ok(Js::Undefined)
        }),
        partition: Rc::new(|p, v, f| call_js(&p, vec![v, js_callback(f)])),
        for_each: Rc::new(|v, f| items(&v)?.into_iter().try_for_each(|x| f(x))),
        // forEachSeries / mapSeries over a promise-returning callback, one at a time.
        series: Rc::new(move |series, v, f| {
            let _ = (&series, &s);
            Box::pin(async move {
                let mut out = vec![];
                for x in items(&v)? {
                    out.push(f(x).await?);
                }
                Ok(Js::array(out))
            })
        }),
        adopt_return: Rc::new(await_value),
    })
}

pub(crate) fn composition_ops(scope: &OkxScope) -> Rc<CompositionOps<Js, Thrown>> {
    Rc::new(CompositionOps {
        order: mutation_ops(scope),
        sort: Rc::new(|v, cmp| {
            let mut xs = items(&v)?;
            let mut error = None;
            xs.sort_by(|a, b| {
                if error.is_some() {
                    return std::cmp::Ordering::Equal;
                }
                match cmp(a.clone(), b.clone()).and_then(|r| r.to_number()) {
                    Ok(d) if d > 0.0 => std::cmp::Ordering::Greater,
                    Ok(d) if d < 0.0 => std::cmp::Ordering::Less,
                    Ok(_) => std::cmp::Ordering::Equal,
                    Err(e) => {
                        error = Some(e);
                        std::cmp::Ordering::Equal
                    }
                }
            });
            if let Some(e) = error {
                return Err(e);
            }
            if let Js::Array(cells) = &v {
                *cells.borrow_mut() = xs.into_iter().map(Some).collect();
            }
            Ok(v)
        }),
        map_bound: Rc::new(|v, f| {
            Ok(Js::array(items(&v)?.into_iter().map(|x| call_js(&f, vec![x])).collect::<Result<_, _>>()?))
        }),
    })
}

pub(crate) fn read_ops(scope: &OkxScope) -> OrderReadOps<'static, Js, Thrown> {
    let m = market_ops(scope);
    let (cap, inv, intr) = (m.capture.clone(), m.invoke.clone(), m.intrinsic.clone());
    let to_base = scope.clone();
    OrderReadOps {
        get: m.get.clone(),
        capture: cap,
        invoke: inv,
        await_value: Rc::new(|v| Box::pin(await_js(v))),
        intrinsic: intr,
        for_of: Rc::new(|v, f| items(&v)?.into_iter().try_for_each(|x| f(x))),
        truthy: m.truthy.clone(),
        nullish: m.nullish.clone(),
        equal: m.equal.clone(),
        literal: m.literal.clone(),
        number: m.number.clone(),
        undefined: m.undefined.clone(),
        array: m.array.clone(),
        object: m.object.clone(),
        // err?.response?.data?.msg || err?.message
        error_message: Rc::new(|e| {
            let v = &e.value;
            let optional = |v: &Js, k: &str| if v.is_nullish() { Ok(Js::Undefined) } else { v.get(k) };
            let msg = optional(&optional(&optional(v, "response")?, "data")?, "msg")?;
            if msg.truthy() { Ok(msg) } else { optional(v, "message") }
        }),
        parallel_map: Rc::new(|_, v, f| {
            let started: Result<Vec<_>, Thrown> = items(&v).map(|xs| xs.into_iter().map(|x| f(x)).collect());
            Box::pin(async move {
                let mut out = vec![];
                for fut in started? {
                    out.push(fut.await?);
                }
                Ok(Js::array(out))
            })
        }),
        // mapOkxPositionHistory(row, { account, toBase, resolveSymbol, resolveSettleCurrency })
        map_position: Rc::new(move |mapper, row, market, account| {
            let this = to_base.this.clone();
            let by_id = move |this: &Js, id: &Js| -> Result<Js, Thrown> {
                this.get("store")?.get("marketsById")?.get_key(&id.to_js_string()?)
            };
            let (t1, t2, t3) = (this.clone(), this.clone(), this);
            let (b1, b2, b3) = (by_id.clone(), by_id.clone(), by_id);
            let fallback = market;
            let opts = Js::object(vec![
                ("account", account),
                (
                    "toBase",
                    Js::function("toBase", move |_, a| {
                        let m = b1(&t1, &arg(&a, 1))?;
                        let m = if m.is_nullish() { fallback.clone() } else { m };
                        t1.call_method("contractsToBase", vec![arg(&a, 0), m])
                    }),
                ),
                (
                    "resolveSymbol",
                    Js::function("resolveSymbol", move |_, a| {
                        let m = b2(&t2, &arg(&a, 0))?;
                        if m.is_nullish() { Ok(Js::Undefined) } else { m.get("symbol") }
                    }),
                ),
                (
                    "resolveSettleCurrency",
                    Js::function("resolveSettleCurrency", move |_, a| {
                        let m = b3(&t3, &arg(&a, 0))?;
                        if m.is_nullish() { Ok(Js::Undefined) } else { m.get("quote") }
                    }),
                ),
            ]);
            call_js(&mapper, vec![row, opts])
        }),
    }
}
