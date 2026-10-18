//! The OKX scope and the closure tables the ported bodies take, built from it.
//! Capture names are TS expressions: `this.a.b` binds the member to its owner,
//! `owner.member:ENDPOINT` is `(...a) => owner.member(ENDPOINTS.ENDPOINT, ...a)`,
//! `this.member:unbound` reads the member without binding.
use super::intrinsics::{IntrinsicHost, intrinsic};
use super::members::Variant;
use crate::family_lifecycle;
use crate::{
    family_lifecycle::LifecycleOps,
    history_stream_api::RuntimeOps,
    market_account_reads::{MarketAccountOps, ReadAwait, ReadMapper},
    order_composition::CompositionOps,
    order_mutations::OrderMutationOps,
    order_reads::OrderReadOps,
    websocket_account_events::SocketEventOps,
};
use safe_cex_exchange::{
    client_order_id::{
        ClientOrderIdRule, ClientOrderIdRuleOverrides, pattern_client_order_id_rule,
    },
    exchange::ExchangeSettings,
    js_runtime::{
        base_exchange::{self, BaseExchangeParts, ExternalPublicDataFactory},
        promise::{all, await_js},
        scope::{Scope, ScopeShared, bound_member, scope_of},
        value::{Js, Thrown, arg, from_typed, to_json},
    },
};
use safe_cex_types::{ExchangeName, ExchangeOptions, exchange_metadata::get_exchange_metadata};
use std::rc::{Rc, Weak};

/// Venue-only shared state: the typed constructor inputs.
pub(crate) struct OkxExtra {
    pub name: ExchangeName,
    pub options: ExchangeOptions,
    pub external_public_data: Option<ExternalPublicDataFactory>,
}

impl OkxExtra {
    pub(crate) fn variant(&self) -> Variant {
        match self.name {
            ExchangeName::Okxdemo => Variant::Demo,
            ExchangeName::Okxeu => Variant::Eu,
            ExchangeName::Okxeudemo => Variant::EuDemo,
            _ => Variant::Global,
        }
    }
}

/// OKX's base.ts settings, read from its exchange metadata.
pub(crate) fn settings(
    name: &ExchangeName,
    shared: Weak<OkxShared>,
) -> Result<ExchangeSettings, Thrown> {
    let key = to_json(&from_typed(name)?)?;
    let meta = get_exchange_metadata(key.as_str().unwrap_or_default(), false)
        .map_err(|e| Thrown::error("Error", &e))?;
    Ok(ExchangeSettings {
        tick_interval: Some(meta.tick_interval),
        positions_supported: Some(meta.features.positions),
        supports_public_only: meta.supports_public_only,
        client_order_id_rule: Some(client_order_id_rule(shared)),
    })
}

// ts: OKXExchange.clientOrderIdRule = patternClientOrderIdRule('OKX', /^[A-Za-z0-9]{1,32}$/, { mint })
fn client_order_id_rule(shared: Weak<OkxShared>) -> ClientOrderIdRule {
    pattern_client_order_id_rule(
        "OKX".into(),
        "/^[A-Za-z0-9]{1,32}$/".into(),
        Rc::new(|id: &str| {
            (1..=32).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric())
        }),
        ClientOrderIdRuleOverrides {
            mint: Some(Rc::new(move || {
                let uuid = scope_of(&shared)
                    .and_then(|s| s.global("uuid"))
                    .and_then(|f| f.call(Js::Undefined, vec![]))
                    .map(|id| id.to_display())
                    .unwrap_or_default();
                format!("{}{uuid}", crate::types::BROKER_ID)
                    .chars()
                    .take(32)
                    .collect()
            })),
            ..Default::default()
        },
    )
}

/// `super(name, opts, store)` inside OKXExchange's constructor: the JS BaseExchange.
fn base_constructor(scope: &OkxScope) -> Js {
    let shared = scope.shared.clone();
    Js::constructor("BaseExchange", None, move |_, a| {
        let options: ExchangeOptions = base_exchange::exchange_options(&arg(&a, 1))?;
        let parts = BaseExchangeParts {
            name: shared.extra.name.clone(),
            options,
            store: shared.store.clone(),
            settings: settings(&shared.extra.name, Rc::downgrade(&shared))?,
            external_public_data: shared.extra.external_public_data.clone(),
        };
        let (this, base) = base_exchange::create(
            &shared.sockets,
            parts,
            arg(&a, 0),
            arg(&a, 1),
            arg(&a, 2),
            |this| {
                // Both maintained demo subclasses initialize isDemo = true.
                if matches!(shared.extra.variant(), Variant::Demo | Variant::EuDemo) {
                    this.define("isDemo", Js::Bool(true))?;
                }
                Ok(())
            },
        )?;
        let _ = shared.base.set(base);
        *shared.exchange.borrow_mut() = this.clone();
        Ok(this)
    })
}

/// `super(...)` from a demo class: the OKXExchange (or OKXEUExchange) constructor.
fn okx_constructor(scope: &OkxScope) -> Js {
    let scope = scope.with_this(Js::Undefined);
    Js::constructor("OKXExchange", None, move |_, a| {
        family_lifecycle::new_exchange(&lifecycle_ops(&scope), arg(&a, 0), arg(&a, 1), arg(&a, 2))
            .map(|state| state.receiver)
    })
}

pub(crate) type OkxScope = Scope<OkxExtra>;
pub(crate) type OkxShared = ScopeShared<OkxExtra>;

impl IntrinsicHost for OkxScope {
    fn this(&self) -> Js {
        self.this.clone()
    }
    // Class fields (arrow members included) initialize right after super().
    fn initialize_okx_fields(&self, receiver: &Js) -> Result<(), Thrown> {
        let scope = self.with_this(receiver.clone());
        super::members::initialize_fields(&scope)?;
        let variant = scope.shared.extra.variant();
        super::members::install(&scope, variant);
        super::typed_members::install(&scope, variant);
        Ok(())
    }
}

fn callable(f: &Js) -> Result<(), Thrown> {
    if f.is_callable() {
        Ok(())
    } else {
        Err(Thrown::type_error(&format!(
            "{} is not a function",
            f.to_display()
        )))
    }
}

fn items(v: &Js) -> Result<Vec<Js>, Thrown> {
    v.array_values()
        .ok_or_else(|| Thrown::type_error(&format!("{} is not iterable", v.to_display())))
}

/// Source for-of advances the shared iterator between body callbacks.
fn for_of(value: &Js, mut body: impl FnMut(Js) -> Result<(), Thrown>) -> Result<(), Thrown> {
    let iterator = value.iterator()?;
    let next = iterator.get("next")?;
    loop {
        let step = next.call(iterator.clone(), vec![])?;
        if !step.is_object() {
            return Err(Thrown::type_error("Iterator result is not an object"));
        }
        if step.get("done")?.truthy() {
            return Ok(());
        }
        body(step.get("value")?)?;
    }
}

/// Resolve a capture name against the scope.
pub(crate) fn capture(scope: &OkxScope, name: &str) -> Result<Js, Thrown> {
    // Constructor bodies run before the receiver exists; super() assembles it.
    let assembled;
    let scope = if scope.this.is_undefined() {
        assembled = scope.with_this(scope.shared.exchange.borrow().clone());
        &assembled
    } else {
        scope
    };
    match name {
        "super" => {
            let class = base_constructor(scope);
            return Ok(Js::function("super", move |_, args| {
                class.construct(args, None)
            }));
        }
        "super:OKXExchange" | "super:OKXEUExchange" => {
            let class = okx_constructor(scope);
            return Ok(Js::function("super", move |_, args| {
                class.construct(args, None)
            }));
        }
        _ => {}
    }
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
        let endpoint = if tail.contains('.') {
            tail.to_owned()
        } else {
            format!("ENDPOINTS.{tail}")
        };
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
        return Ok(Js::function(path, move |_, args| {
            member.call(owner.clone(), args)
        }));
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

fn await_value(runtime: &std::rc::Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime>, v: Js) -> ReadAwait<Js, Thrown> {
    Box::pin(await_js(runtime, v))
}

pub(crate) fn market_ops(scope: &OkxScope) -> Rc<MarketAccountOps<Js, Thrown>> {
    let s = scope.clone();
    let await_runtime = s.exchange_runtime();
    let return_runtime = await_runtime.clone();
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
        map: Rc::new(|v, f| {
            Ok(Js::array(
                items(&v)?
                    .into_iter()
                    .map(|x| f(x))
                    .collect::<Result<_, _>>()?,
            ))
        }),
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
        for_of: Rc::new(|v, f| for_of(&v, |x| f(x))),
        sum_by: Rc::new(|sum, values, mapper| {
            let mut args = vec![values];
            if let Some(m) = mapper {
                args.push(js_callback(m));
            }
            call_js(&sum, args)
        }),
        // p-iteration starts every async callback before awaiting the group.
        parallel_map: Rc::new(move |_, v, f| {
            let started = items(&v).map(|xs| {
                let values = xs.into_iter().map(|x| rt.spawn(f(x))).collect();
                Js::Promise(all(&rt.exchange_runtime(), values))
            });
            { let runtime = rt.exchange_runtime(); Box::pin(async move { await_js(&runtime, started?).await }) }
        }),
        some: Rc::new(|v, f| {
            for x in items(&v)? {
                if f(x)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }),
        await_value: Rc::new(move |v| await_value(&await_runtime, v)),
        error_value: Rc::new(|e| e.value.clone()),
        new_error: Rc::new(|ctor, message| Ok(Thrown::new(call_js(&ctor, vec![message])?))),
    })
}

pub(crate) fn lifecycle_ops(scope: &OkxScope) -> Rc<LifecycleOps<Js, Thrown>> {
    let s = scope.clone();
    let await_runtime = s.exchange_runtime();
    let return_runtime = await_runtime.clone();
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
    let await_runtime = s.exchange_runtime();
    let return_runtime = await_runtime.clone();
    Rc::new(OrderMutationOps {
        value: market_ops(scope),
        has: Rc::new(|k, v| safe_cex_exchange::js_runtime::library::in_operator(v, k)),
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
        adopt_return: Rc::new(move |v| await_value(&return_runtime, v)),
    })
}

pub(crate) fn composition_ops(scope: &OkxScope) -> Rc<CompositionOps<Js, Thrown>> {
    Rc::new(CompositionOps {
        order: mutation_ops(scope),
        // `array.sort(compare)`: the body hands over the bound `sort` member.
        sort: Rc::new(|sort, cmp| {
            let compare = Js::function("compare", move |_, a| cmp(arg(&a, 0), arg(&a, 1)));
            call_js(&sort, vec![compare])
        }),
        // `array.map(this.formatAlgoOrder)`: the bound `map` member and its callback.
        map_bound: Rc::new(|map, f| call_js(&map, vec![f])),
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
        await_value: m.await_value.clone(),
        intrinsic: intr,
        for_of: Rc::new(|v, f| for_of(&v, |x| f(x))),
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
            let optional = |v: &Js, k: &str| {
                if v.is_nullish() {
                    Ok(Js::Undefined)
                } else {
                    v.get(k)
                }
            };
            let msg = optional(&optional(&optional(v, "response")?, "data")?, "msg")?;
            if msg.truthy() {
                Ok(msg)
            } else {
                optional(v, "message")
            }
        }),
        parallel_map: m.parallel_map.clone(),
        // mapOkxPositionHistory(row, { account, toBase, resolveSymbol, resolveSettleCurrency })
        map_position: Rc::new(move |mapper, row, market, account| {
            let this = to_base.this.clone();
            fn by_id(this: &Js, id: &Js) -> Result<Js, Thrown> {
                this.get("store")?
                    .get("marketsById")?
                    .get_key(&id.to_js_string()?)
            }
            let (t1, t2, t3) = (this.clone(), this.clone(), this);
            let (b1, b2, b3) = (by_id, by_id, by_id);
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
                        if m.is_nullish() {
                            Ok(Js::Undefined)
                        } else {
                            m.get("symbol")
                        }
                    }),
                ),
                (
                    "resolveSettleCurrency",
                    Js::function("resolveSettleCurrency", move |_, a| {
                        let m = b3(&t3, &arg(&a, 0))?;
                        if m.is_nullish() {
                            Ok(Js::Undefined)
                        } else {
                            m.get("quote")
                        }
                    }),
                ),
            ]);
            call_js(&mapper, vec![row, opts])
        }),
    }
}
