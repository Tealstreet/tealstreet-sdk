//! The JS BaseExchange instance a venue's assembled class extends: TS base
//! fields plus members bridged to the Rust core BaseExchange.
use super::{
    promise::Promise,
    sockets::JsSocketContext,
    value::{Getter, Js, Thrown, arg, from_typed, to_json},
};
use crate::{
    error_reporting::ExchangeErrorInput,
    exchange::{BaseExchange, ExchangeSettings},
    runtime::ExchangeRuntime,
};
use safe_cex_store::Store;
use safe_cex_types::{ExchangeName, ExchangeOptions};
use std::rc::{Rc, Weak};

/// Typed inputs of the Rust core, distinct from the JS-shaped constructor arguments.
pub struct BaseExchangeParts {
    pub name: ExchangeName,
    pub options: ExchangeOptions,
    pub store: Rc<dyn Store>,
    pub settings: ExchangeSettings,
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
        def(
            "emitter",
            Js::object(vec![(
                "emit",
                Js::function("emit", move |_, a| {
                    let event = text(&arg(&a, 0))?;
                    let payload = to_json(&Js::array(a.into_iter().skip(1).collect()))?;
                    base.emit(&event, payload)?;
                    Ok(Js::Bool(true))
                }),
            )]),
        );
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
    Ok((this, base))
}
