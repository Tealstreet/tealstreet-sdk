//! The JS BaseExchange instance a venue's assembled class extends: TS base
//! fields plus members bridged to the Rust core BaseExchange.
use super::{
    promise::Promise,
    sockets::JsSocketContext,
    value::{Getter, Js, Thrown, arg, from_typed, to_json, to_typed},
};
use crate::{
    error_reporting::ExchangeErrorInput,
    exchange::{BaseExchange, Exchange, ExchangeSettings},
    external::ExternalPublicData,
    runtime::ExchangeRuntime,
};
use safe_cex_store::Store;
use safe_cex_types::{
    ExchangeName, ExchangeOptions, ExchangeOptionsExternalPublicData, Market, PlaceOrderOpts,
};
use std::rc::{Rc, Weak};

/// Builds the `options.externalPublicData` provider; the host supplies it.
pub type ExternalPublicDataFactory =
    Rc<dyn Fn(&ExchangeOptionsExternalPublicData) -> Rc<dyn ExternalPublicData>>;

/// Typed inputs of the Rust core, distinct from the JS-shaped constructor arguments.
pub struct BaseExchangeParts {
    pub name: ExchangeName,
    pub options: ExchangeOptions,
    pub store: Rc<dyn Store>,
    pub settings: ExchangeSettings,
    /// Required when `options.externalPublicData` is set.
    pub external_public_data: Option<ExternalPublicDataFactory>,
}

fn assembled_venue(context: &Weak<JsSocketContext>) -> Result<Rc<dyn Exchange>, Thrown> {
    context
        .upgrade()
        .and_then(|c| c.venue.borrow().as_ref().and_then(Weak::upgrade))
        .ok_or_else(|| Thrown::error("Error", "exchange is not assembled"))
}

fn text(v: &Js) -> Result<String, Thrown> {
    Ok(String::from_utf16_lossy(&v.to_js_string()?))
}

/// `new BaseExchange(name, options, store)`; keep the returned core for `Exchange::base()`.
pub fn create(
    context: &Rc<JsSocketContext>,
    parts: BaseExchangeParts,
    name: Js,
    options: Js,
    store: Js,
) -> Result<(Js, Rc<BaseExchange>), Thrown> {
    let runtime: Rc<dyn ExchangeRuntime> = context.runtime.clone();
    if parts.options.external_public_data.is_some() && parts.external_public_data.is_none() {
        return Err(Thrown::error(
            "Error",
            "externalPublicData is configured but no external public data provider was supplied",
        ));
    }
    let external_public_data = parts.external_public_data;
    let base = Rc::new(BaseExchange::new(
        parts.name,
        parts.options,
        parts.store,
        runtime.clone(),
        parts.settings,
    ));
    let this = Js::object_with_class(
        "BaseExchange",
        vec![
            ("name", name.clone()),
            ("_exchangeName", name),
            ("options", options),
            ("store", store),
        ],
    );
    let def = |k: &str, v: Js| {
        let _ = this.define(k, v);
    };
    {
        let base = base.clone();
        this.define_accessor(
            "isDisposed",
            Some(Rc::new(move |_| Ok(Js::Bool(base.is_disposed())))),
            None,
        );
    }
    {
        // Dispatches through the venue's privacy hook, as the source getter does.
        let context = Rc::downgrade(context);
        let getter: Getter = Rc::new(move |_| {
            let venue = context
                .upgrade()
                .and_then(|c| c.venue.borrow().as_ref().and_then(Weak::upgrade))
                .ok_or_else(|| Thrown::error("Error", "exchange is not assembled"))?;
            Ok(Js::Bool(venue.is_public_data_only()))
        });
        this.define_accessor("isPublicDataOnly", Some(getter), None);
    }
    {
        let base = base.clone();
        // Listener ids by JS function identity, so `off(event, fn)` finds its row.
        let registered: Rc<std::cell::RefCell<Vec<(String, Js, String)>>> = Rc::default();
        let (on_base, off_base) = (base.clone(), base.clone());
        let (on_rows, off_rows) = (registered.clone(), registered);
        def(
            "emitter",
            Js::object(vec![
                (
                    "emit",
                    Js::function("emit", move |_, a| {
                        let event = text(&arg(&a, 0))?;
                        let payload = to_json(&Js::array(a.into_iter().skip(1).collect()))?;
                        base.emit(&event, payload)?;
                        Ok(Js::Bool(true))
                    }),
                ),
                (
                    "on",
                    Js::function("on", move |this, a| {
                        let event = text(&arg(&a, 0))?;
                        let listener = arg(&a, 1);
                        let callee = listener.clone();
                        let id = on_base.on(
                            &event,
                            Rc::new(move |_, data| {
                                let args = match super::value::from_json(data) {
                                    Js::Array(items) => items
                                        .borrow()
                                        .iter()
                                        .map(|v| v.clone().unwrap_or(Js::Undefined))
                                        .collect(),
                                    other => vec![other],
                                };
                                callee.call(Js::Undefined, args)?;
                                Ok(())
                            }),
                        );
                        on_rows.borrow_mut().push((event, listener, id));
                        Ok(this)
                    }),
                ),
                (
                    "off",
                    Js::function("off", move |this, a| {
                        let event = text(&arg(&a, 0))?;
                        let listener = arg(&a, 1);
                        let mut rows = off_rows.borrow_mut();
                        if let Some(at) = rows
                            .iter()
                            .position(|(e, f, _)| *e == event && f.same_reference(&listener))
                        {
                            let (_, _, id) = rows.remove(at);
                            off_base.off(&event, Some(&id));
                        }
                        Ok(this)
                    }),
                ),
            ]),
        );
        // ts: base.ts:454 — `this.on/once/off` are the emitter's members bound to it.
        if let Ok(emitter) = this.get("emitter") {
            for name in ["on", "off"] {
                if let Ok(member) = super::scope::bound_member(&emitter, name) {
                    def(name, member);
                }
            }
        }
    }
    {
        let base = base.clone();
        def(
            "log",
            Js::function("log", move |_, a| {
                let severity = arg(&a, 1);
                let severity = if severity.is_undefined() {
                    "info".to_owned()
                } else {
                    text(&severity)?
                };
                base.log(&text(&arg(&a, 0))?, &severity)?;
                Ok(Js::Undefined)
            }),
        );
    }
    {
        let base = base.clone();
        def(
            "emitAndLogError",
            Js::function("emitAndLogError", move |_, a| {
                let error = arg(&a, 0);
                let input = match &error {
                    Js::Undefined => ExchangeErrorInput::Missing,
                    Js::String(s) => ExchangeErrorInput::Message(String::from_utf16_lossy(s)),
                    Js::Object(o) if o.class == "Error" || o.class == "AxiosError" => {
                        ExchangeErrorInput::Error(Box::new(
                            Thrown::new(error.clone()).to_exchange_error(),
                        ))
                    }
                    other => ExchangeErrorInput::Raw(to_json(other)?),
                };
                let msg = arg(&a, 1);
                let msg = if msg.is_undefined() {
                    None
                } else {
                    Some(text(&msg)?)
                };
                let extra = arg(&a, 2);
                let extra = if extra.is_undefined() {
                    None
                } else {
                    Some(to_json(&extra)?)
                };
                base.emit_and_log_error(input, msg.as_deref(), extra.as_ref())?;
                Ok(Js::Undefined)
            }),
        );
    }
    {
        let receiver = this.clone();
        def(
            "createXhrWithSecondaryKey",
            Js::function("createXhrWithSecondaryKey", move |_, a| {
                let other = arg(&a, 2);
                let xhr = arg(&a, 1).call(Js::Undefined, vec![arg(&a, 0), receiver.clone()])?;
                let limit = if other.is_nullish() {
                    Js::Undefined
                } else {
                    other.get("rateLimit")?
                };
                let xhr = if limit.is_callable() {
                    limit.call(Js::Undefined, vec![xhr])?
                } else {
                    xhr
                };
                receiver.set("_primaryXhr", xhr.clone())?;
                receiver.set("_secondaryXhr", xhr.clone())?;
                Ok(xhr)
            }),
        );
    }
    {
        let base = base.clone();
        def(
            "dispose",
            Js::function("dispose", move |_, _| {
                base.dispose()?;
                Ok(Js::Undefined)
            }),
        );
    }
    {
        let base = base.clone();
        def(
            "assertSingleNativeClientOrderId",
            Js::function("assertSingleNativeClientOrderId", move |_, a| {
                let id = arg(&a, 0);
                let id = if id.is_undefined() {
                    None
                } else {
                    Some(text(&id)?)
                };
                base.assert_single_native_client_order_id(
                    id.as_deref(),
                    arg(&a, 1).to_number()? as usize,
                )?;
                Ok(Js::Undefined)
            }),
        );
    }
    {
        let base = base.clone();
        let context = Rc::downgrade(context);
        let runtime = runtime.clone();
        def(
            "getOrFetchPositionsForSymbol",
            Js::function("getOrFetchPositionsForSymbol", move |_, a| {
                let symbol = text(&arg(&a, 0))?;
                let venue = context
                    .upgrade()
                    .and_then(|c| c.venue.borrow().as_ref().and_then(Weak::upgrade))
                    .ok_or_else(|| Thrown::error("Error", "exchange is not assembled"))?;
                let base = base.clone();
                Ok(Js::Promise(Promise::spawn(
                    &runtime,
                    Box::pin(async move {
                        let positions = base
                            .get_or_fetch_positions_for_symbol(venue, &symbol)
                            .await?;
                        from_typed(&positions)
                    }),
                )))
            }),
        );
    }
    {
        let base = base.clone();
        def(
            "getTickInterval",
            Js::function("getTickInterval", move |_, _| {
                Ok(base.get_tick_interval().map_or(Js::Undefined, Js::Number))
            }),
        );
    }
    {
        let base = base.clone();
        let context = Rc::downgrade(context);
        this.define_accessor(
            "needsAccountMarketOverlay",
            Some(Rc::new(move |_| {
                let venue = assembled_venue(&context)?;
                Ok(Js::Bool(base.needs_account_market_overlay(&*venue)))
            })),
            None,
        );
    }
    {
        let base = base.clone();
        let runtime = runtime.clone();
        def(
            "loadAccountMarketOverlay",
            Js::function("loadAccountMarketOverlay", move |_, a| {
                let load = arg(&a, 0);
                let base = base.clone();
                let reporter = runtime.clone();
                Ok(Js::Promise(Promise::spawn(
                    &runtime,
                    Box::pin(async move {
                        let overlay = Box::pin(async move {
                            let value =
                                super::promise::await_js(load.call(Js::Undefined, vec![])?).await?;
                            Ok(market_overlay(value, reporter))
                        });
                        base.load_account_market_overlay(overlay).await?;
                        Ok(Js::Undefined)
                    }),
                )))
            }),
        );
    }
    {
        let base = base.clone();
        let runtime = runtime.clone();
        def(
            "initExternalPublicData",
            Js::function("initExternalPublicData", move |_, _| {
                let base = base.clone();
                let factory = external_public_data.clone();
                Ok(Js::Promise(Promise::spawn(
                    &runtime,
                    Box::pin(async move {
                        let used = base
                            .init_external_public_data(move |config| match factory {
                                Some(factory) => factory(config),
                                None => unreachable!(
                                    "create() refuses externalPublicData without a provider"
                                ),
                            })
                            .await?;
                        Ok(Js::Bool(used))
                    }),
                )))
            }),
        );
    }
    {
        let base = base.clone();
        def(
            "resolveCallerClientOrderId",
            Js::function("resolveCallerClientOrderId", move |_, a| {
                let opts: PlaceOrderOpts = to_typed(&arg(&a, 0))?;
                Ok(base
                    .resolve_caller_client_order_id(&opts)?
                    .map_or(Js::Undefined, |id| Js::str(&id)))
            }),
        );
    }
    {
        let base = base.clone();
        def(
            "assertCallerClientOrderIds",
            Js::function("assertCallerClientOrderIds", move |_, a| {
                let orders: Vec<PlaceOrderOpts> = to_typed(&arg(&a, 0))?;
                base.assert_caller_client_order_ids(&orders)?;
                Ok(Js::Undefined)
            }),
        );
    }
    {
        let base = base.clone();
        def(
            "deriveAlgoOrdersFromNormalOrdersOpts",
            Js::function("deriveAlgoOrdersFromNormalOrdersOpts", move |_, a| {
                let opts: Vec<PlaceOrderOpts> = to_typed(&arg(&a, 0))?;
                from_typed(&base.derive_algo_orders_from_normal_orders_opts(&opts))
            }),
        );
    }
    Ok((this, base))
}

/// A JS `(markets) => markets` overlay as the core's typed overlay; null is none.
/// A throwing or malformed overlay is reported and leaves the markets unchanged.
fn market_overlay(
    value: Js,
    reporter: Rc<dyn ExchangeRuntime>,
) -> Option<crate::snapshots::AccountMarketOverlay> {
    if value.is_nullish() {
        return None;
    }
    Some(Rc::new(move |markets: Vec<Market>| {
        let mapped = from_typed(&markets)
            .and_then(|m| value.call(Js::Undefined, vec![m]))
            .and_then(|m| to_typed::<Vec<Market>>(&m));
        match mapped {
            Ok(mapped) => mapped,
            Err(error) => {
                reporter.report_exception(
                    &error.to_exchange_error(),
                    &serde_json::json!({"source": "accountMarketOverlay"}),
                );
                markets
            }
        }
    }))
}
