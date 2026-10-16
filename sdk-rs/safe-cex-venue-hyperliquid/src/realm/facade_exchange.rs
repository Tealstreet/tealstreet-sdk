//! The JS `BaseExchange` superclass the ported HyperliquidExchange class
//! extends. Each inherited member the venue bodies use delegates to the Rust
//! BaseExchange; members the bodies never reach are absent rather than stubbed.
use super::{Realm, value::*};
use safe_cex_exchange::{error_reporting::ExchangeErrorInput, exchange::BaseExchange};
use safe_cex_types::PlaceOrderOpts;
use std::rc::Rc;

fn arg(args: &[Value], i: usize) -> Value {
    args.get(i).cloned().unwrap_or(Value::Undefined)
}
fn thrown(r: &Realm, e: safe_cex_exchange::runtime::ExchangeError) -> Thrown {
    let error = r.error(&e.message);
    if e.name != "Error" {
        r.define_hidden(&error, PropKey::str("name"), Value::str(&e.name));
    }
    error
}
/// TinyEmitter.emit arguments as the Rust listener payload: one argument as
/// itself, several as an array (the shape `BaseExchange::log` emits).
fn payload(r: &mut Realm, args: &[Value]) -> Result<serde_json::Value, Thrown> {
    match args {
        [] => Ok(serde_json::Value::Null),
        [one] => r.to_serde(one),
        many => {
            let array = r.new_array(many.to_vec());
            r.to_serde(&array)
        }
    }
}
pub fn error_input(r: &mut Realm, value: &Value) -> Result<ExchangeErrorInput, Thrown> {
    Ok(match value {
        Value::Undefined => ExchangeErrorInput::Missing,
        Value::String(u) => ExchangeErrorInput::Message(String::from_utf16_lossy(u)),
        v if v
            .object()
            .is_some_and(|o| matches!(o.borrow().kind, Kind::Error)) =>
        {
            ExchangeErrorInput::Error(Box::new(r.exchange_error(v)))
        }
        v => ExchangeErrorInput::Raw(r.to_serde(v)?),
    })
}

/// The JS class; instances are allocated with newTarget's prototype and carry
/// the source constructor's own fields (`options`, `store`, `emitter`).
pub fn base_exchange_class(realm: &mut Realm, base: Rc<BaseExchange>) -> Value {
    let prototype = realm.new_object();
    let construct: NativeConstruct = {
        let fallback = prototype.object().unwrap().clone();
        let base = base.clone();
        Rc::new(move |r, args, new_target| {
            let proto = r.prototype_of_target(new_target, &fallback)?;
            let instance = r.alloc(Kind::Ordinary, Some(&proto));
            r.define_str(&instance, "_exchangeName", arg(args, 0))?;
            r.define_str(&instance, "options", arg(args, 1))?;
            r.define_str(&instance, "store", arg(args, 2))?;
            let emitter = r.new_object();
            let b = base.clone();
            let emit = r.native("emit", 1, move |r, _, a| {
                let event = r.to_rust_string(&arg(a, 0))?;
                let data = payload(r, a.get(1..).unwrap_or_default())?;
                b.emit(&event, data).map_err(|e| thrown(r, e))?;
                Ok(Value::Undefined)
            });
            r.define_str(&emitter, "emit", emit)?;
            r.define_str(&instance, "emitter", emitter)?;
            Ok(instance)
        })
    };
    let class = realm.function_data(FunctionData {
        name: realm.js_text("BaseExchange"),
        length: 3,
        call: None,
        construct: Some(construct),
    });
    if let Some(o) = class.object() {
        o.borrow_mut().props.push(Prop {
            key: PropKey::str("prototype"),
            slot: Slot::Data(prototype.clone()),
            enumerable: false,
            writable: false,
            configurable: false,
        });
    }
    realm.define_hidden(&prototype, PropKey::str("constructor"), class.clone());

    let method = |realm: &Realm,
                  name: &str,
                  length: u8,
                  body: Rc<
        dyn Fn(&mut Realm, &Rc<BaseExchange>, &Value, &[Value]) -> Result<Value, Thrown>,
    >| {
        let b = base.clone();
        let f = realm.native(name, length, move |r, this, args| body(r, &b, this, args));
        realm.define_hidden(&prototype, PropKey::str(name), f);
    };
    method(
        realm,
        "log",
        2,
        Rc::new(|r, b, _, a| {
            let message = r.to_rust_string(&arg(a, 0))?;
            let severity = match arg(a, 1) {
                Value::Undefined => "Info".to_owned(),
                s => r.to_rust_string(&s)?,
            };
            b.log(&message, &severity).map_err(|e| thrown(r, e))?;
            Ok(Value::Undefined)
        }),
    );
    method(
        realm,
        "emitAndLogError",
        3,
        Rc::new(|r, b, _, a| {
            let input = error_input(r, &arg(a, 0))?;
            let message = match arg(a, 1) {
                Value::Undefined => None,
                m => Some(r.to_rust_string(&m)?),
            };
            let extra = match arg(a, 2) {
                Value::Undefined => None,
                x => Some(r.to_serde(&x)?),
            };
            b.emit_and_log_error(input, message.as_deref(), extra.as_ref())
                .map_err(|e| thrown(r, e))?;
            Ok(Value::Undefined)
        }),
    );
    method(
        realm,
        "resolveCallerClientOrderId",
        1,
        Rc::new(|r, b, _, a| {
            let opts: PlaceOrderOpts = r.to_typed(&arg(a, 0))?;
            Ok(
                match b
                    .resolve_caller_client_order_id(&opts)
                    .map_err(|e| thrown(r, e))?
                {
                    Some(id) => Value::str(&id),
                    None => Value::Undefined,
                },
            )
        }),
    );
    method(
        realm,
        "assertCallerClientOrderIds",
        1,
        Rc::new(|r, b, _, a| {
            let orders: Vec<PlaceOrderOpts> = r.to_typed(&arg(a, 0))?;
            b.assert_caller_client_order_ids(&orders)
                .map_err(|e| thrown(r, e))?;
            Ok(Value::Undefined)
        }),
    );
    method(
        realm,
        "getTickInterval",
        0,
        Rc::new(|_, b, _, _| {
            Ok(b.get_tick_interval()
                .map_or(Value::Undefined, Value::Number))
        }),
    );
    method(
        realm,
        "getActiveTickerSymbols",
        0,
        Rc::new(|r, b, _, _| {
            let symbols = b.get_active_ticker_symbols();
            r.from_typed(&symbols)
        }),
    );
    method(
        realm,
        "initExternalPublicData",
        0,
        Rc::new(|r, b, _, _| {
            // External public data is a hosted provider the Rust binding does not
            // attach; a configured account must hear that, not silently go direct.
            let configured = b.options().external_public_data.is_some();
            Ok(r.settled_promise(if configured {
                Err(r.error(
                    "Hyperliquid (Rust): external public data is not supported by this binding",
                ))
            } else {
                Ok(Value::Bool(false))
            }))
        }),
    );
    method(
        realm,
        "foldClosedPositionRecords",
        2,
        Rc::new(|r, _, _, _| {
            Ok(r.settled_promise(Err(r.error(
                "Hyperliquid (Rust): fetchHistoricalPositions is not bound by this realm",
            ))))
        }),
    );
    method(
        realm,
        "dispose",
        0,
        Rc::new(|r, b, _, _| {
            b.dispose().map_err(|e| thrown(r, e))?;
            Ok(Value::Undefined)
        }),
    );
    // The Rust base owns disposal (super.dispose() reaches it above); no venue
    // body assigns isDisposed directly, so the setter has nothing to forward.
    let b = base.clone();
    let disposed = realm.native("get isDisposed", 0, move |_, _, _| {
        Ok(Value::Bool(b.is_disposed()))
    });
    if let Some(o) = prototype.object() {
        o.borrow_mut().props.push(Prop {
            key: PropKey::str("isDisposed"),
            slot: Slot::Accessor {
                get: Some(disposed),
                set: Some(realm.native("set isDisposed", 1, |_, _, _| Ok(Value::Undefined))),
            },
            enumerable: false,
            writable: false,
            configurable: true,
        });
    }
    let b = base.clone();
    let name = realm.native("get name", 0, move |r, _, _| r.from_typed(b.name()));
    realm.define_getter(&prototype, PropKey::str("name"), name);
    class
}
