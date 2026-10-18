//! The JS `BaseExchange` superclass the ported HyperliquidExchange class
//! extends. Each inherited member the venue bodies use delegates to the Rust
//! BaseExchange; members the bodies never reach are absent rather than stubbed.
use super::{
    Realm,
    value::{FunctionData, Kind, NativeConstruct, Prop, PropKey, Slot, Thrown, Value},
};
use safe_cex_exchange::{
    error_reporting::ExchangeErrorInput, exchange::BaseExchange,
    js_runtime::base_exchange::ExternalPublicDataFactory,
};
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
/// Keep the argument list distinct from a single argument that is itself an array.
fn payload(
    r: &mut Realm,
    args: &[Value],
) -> Result<Vec<safe_cex_exchange::js_runtime::value::Js>, Thrown> {
    args.iter()
        .map(|value| r.capture_typed_value(value, 0))
        .collect()
}
pub(crate) fn error_input(r: &mut Realm, value: &Value) -> Result<ExchangeErrorInput, Thrown> {
    Ok(match value {
        Value::Undefined => ExchangeErrorInput::Missing,
        Value::String(u) => {
            ExchangeErrorInput::Raw(safe_cex_exchange::js_runtime::value::Js::String(u.clone()))
        }
        v if v
            .object()
            .is_some_and(|o| matches!(o.borrow().kind, Kind::Error)) =>
        {
            ExchangeErrorInput::Error(Box::new(r.exchange_error(v)))
        }
        v => ExchangeErrorInput::Raw(r.capture_typed_value(v, 0)?),
    })
}

/// The JS class; instances are allocated with newTarget's prototype and carry
/// the source constructor's own fields (`options`, `store`, `emitter`).
pub(crate) fn base_exchange_class(
    realm: &mut Realm,
    base: Rc<BaseExchange>,
    external_public_data: Option<ExternalPublicDataFactory>,
) -> Value {
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
                b.emit_args(&event, &data).map_err(|e| thrown(r, e))?;
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
        Rc::new(move |r, b, _, _| {
            let base = b.clone();
            let owner = base.clone();
            let factory = external_public_data.clone();
            let realm = r.clone();
            Ok(r.spawn_value(Box::pin(async move {
                if base.options().external_public_data.is_some() && factory.is_none() {
                    return Err(realm.error(
                        "externalPublicData is configured but no external public data provider was supplied",
                    ));
                }
                let used = base
                    .init_external_public_data(move |config| {
                        factory.expect("configured provider checked above")(config, owner)
                    })
                    .await
                    .map_err(|error| thrown(&realm, error))?;
                Ok(Value::Bool(used))
            })))
        }),
    );
    method(
        realm,
        "foldClosedPositionRecords",
        2,
        Rc::new(|r, base, owner, args| {
            let mut realm = r.clone();
            let owner = owner.clone();
            let base = base.clone();
            let opts = arg(args, 0);
            let config = arg(args, 1);
            Ok(r.spawn_value(Box::pin(async move {
                use safe_cex_exchange::build_records::{
                    BuildRecordsOptions, FoldableExchange, build_folded_position_records,
                };
                let account = realm.get_str(&opts, "account")?;
                let account = realm.to_rust_string(&account)?;
                let to = realm.get_str(&opts, "to")?;
                let to = if to.is_nullish() {
                    let date = realm.global_binding("Date")?;
                    realm.call_method(&date, "now", &[])?
                } else {
                    to
                };
                let to = realm.to_number(&to)?;
                let from = realm.get_str(&opts, "from")?;
                let from = if from.is_nullish() {
                    to - 30.0 * 24.0 * 60.0 * 60.0 * 1000.0
                } else {
                    realm.to_number(&from)?
                };
                let symbol_scoped = if config.is_nullish() {
                    false
                } else {
                    let flag = realm.get_str(&config, "symbolScoped")?;
                    flag.truthy()
                };
                // These are the concrete class's ported own methods, not the
                // inert BaseExchange capability fallbacks.
                let (fills_realm, fills_owner) = (realm.clone(), owner.clone());
                let mut fills = move |options: safe_cex_types::ExecutionsOptions| {
                    let (mut realm, owner) = (fills_realm.clone(), fills_owner.clone());
                    Box::pin(async move {
                        let options = realm
                            .from_typed(&options)
                            .map_err(|error| realm.exchange_error(&error))?;
                        let result = realm
                            .call_method(&owner, "fetchHistoricalFills", &[options])
                            .map_err(|error| realm.exchange_error(&error))?;
                        let result = realm
                            .await_value(result)
                            .await
                            .map_err(|error| realm.exchange_error(&error))?;
                        realm
                            .to_typed(&result)
                            .map_err(|error| realm.exchange_error(&error))
                    })
                        as safe_cex_exchange::runtime::ExchangeFuture<
                            'static,
                            Vec<safe_cex_types::Execution>,
                        >
                };
                let income_realm = realm.clone();
                let mut income = move |from, to| {
                    let (mut realm, owner) = (income_realm.clone(), owner.clone());
                    Box::pin(async move {
                        let result = realm
                            .call_method(
                                &owner,
                                "fetchIncomeHistoryRange",
                                &[Value::Number(from), Value::Number(to)],
                            )
                            .map_err(|error| realm.exchange_error(&error))?;
                        let result = realm
                            .await_value(result)
                            .await
                            .map_err(|error| realm.exchange_error(&error))?;
                        realm
                            .to_typed(&result)
                            .map_err(|error| realm.exchange_error(&error))
                    })
                        as safe_cex_exchange::runtime::ExchangeFuture<
                            'static,
                            Vec<safe_cex_types::IncomeRecord>,
                        >
                };
                let mut settle = move |symbol: &str| {
                    base.store
                        .markets_by_symbol()
                        .get(symbol)
                        .map(|market| market.quote.clone())
                        .filter(|quote| !quote.is_empty())
                        .unwrap_or_else(|| "USDT".into())
                };
                let result = build_folded_position_records(
                    &mut FoldableExchange {
                        fetch_historical_fills: &mut fills,
                        fetch_income_history_range: Some(&mut income),
                    },
                    &mut BuildRecordsOptions {
                        account: &account,
                        from,
                        to,
                        symbol_scoped,
                        resolve_settle_currency: &mut settle,
                    },
                )
                .await
                .map_err(|error| thrown(&realm, error.error))?;
                realm.from_typed(&result.records)
            })))
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
    let public_only = realm.native("get isPublicDataOnly", 0, move |_, _, _| {
        Ok(Value::Bool(
            b.options().public_data_only == Some(true) || !b.can_sign(),
        ))
    });
    realm.define_getter(&prototype, PropKey::str("isPublicDataOnly"), public_only);
    let b = base.clone();
    let name = realm.native("get name", 0, move |r, _, _| r.from_typed(b.name()));
    realm.define_getter(&prototype, PropKey::str("name"), name);
    class
}
