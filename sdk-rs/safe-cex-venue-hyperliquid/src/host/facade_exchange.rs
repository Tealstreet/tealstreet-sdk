//! The JS `BaseExchange` superclass the ported HyperliquidExchange class
//! extends. Each inherited member the venue bodies use delegates to the Rust
//! BaseExchange; members the bodies never reach are absent rather than stubbed.
use super::Host;
use safe_cex_exchange::js_runtime::value::{Js, Thrown};
use safe_cex_exchange::{
    error_reporting::ExchangeErrorInput, exchange::BaseExchange,
    js_runtime::base_exchange::ExternalPublicDataFactory,
};
use safe_cex_types::PlaceOrderOpts;
use std::rc::Rc;

fn arg(args: &[Js], i: usize) -> Js {
    args.get(i).cloned().unwrap_or(Js::Undefined)
}
fn thrown(_: &Host, e: safe_cex_exchange::runtime::ExchangeError) -> Thrown {
    e.into()
}
pub(crate) fn error_input(_: &mut Host, value: &Js) -> Result<ExchangeErrorInput, Thrown> {
    Ok(if value.is_undefined() {
        ExchangeErrorInput::Missing
    } else {
        ExchangeErrorInput::Raw(value.clone())
    })
}

/// The JS class; instances are allocated with newTarget's prototype and carry
/// the source constructor's own fields (`options`, `store`, `emitter`).
pub(crate) fn base_exchange_class(
    realm: &mut Host,
    base: Rc<BaseExchange>,
    external_public_data: Option<ExternalPublicDataFactory>,
) -> Js {
    let prototype = realm.new_object();
    let owner = base.clone();
    let class = realm.constructor("BaseExchange", 3, move |r, args, new_target| {
        let instance = new_target.allocate_instance()?;
        r.define_str(&instance, "_exchangeName", arg(args, 0))?;
        r.define_str(&instance, "options", arg(args, 1))?;
        r.define_str(&instance, "store", arg(args, 2))?;
        let b = owner.clone();
        let emit = r.native("emit", 1, move |r, _, args| {
            let event = r.to_rust_string(&arg(args, 0))?;
            b.emit_args(&event, args.get(1..).unwrap_or_default())
                .map_err(Thrown::from)?;
            Ok(Js::Undefined)
        });
        r.define_str(&instance, "emitter", Js::object(vec![("emit", emit)]))?;
        Ok(instance)
    });
    class
        .define_property(
            &"prototype".encode_utf16().collect::<Vec<_>>(),
            &Js::object(vec![("value", prototype.clone())]),
        )
        .expect("fresh base class prototype");

    realm.define_hidden(&prototype, Js::str("constructor"), class.clone());

    let method =
        |realm: &Host,
         name: &str,
         length: u8,
         body: Rc<dyn Fn(&mut Host, &Rc<BaseExchange>, &Js, &[Js]) -> Result<Js, Thrown>>| {
            let b = base.clone();
            let f = realm.native(name, length, move |r, this, args| body(r, &b, this, args));
            realm.define_hidden(&prototype, Js::str(name), f);
        };
    method(
        realm,
        "log",
        2,
        Rc::new(|r, b, _, a| {
            let severity = match arg(a, 1) {
                Js::Undefined => "info".to_owned(),
                s => r.to_rust_string(&s)?,
            };
            b.log_value(&arg(a, 0), &severity)
                .map_err(|e| thrown(r, e))?;
            Ok(Js::Undefined)
        }),
    );
    method(
        realm,
        "emitAndLogError",
        3,
        Rc::new(|r, b, _, a| {
            let input = error_input(r, &arg(a, 0))?;
            let message = match arg(a, 1) {
                Js::Undefined => None,
                m => Some(r.to_rust_string(&m)?),
            };
            let extra = match arg(a, 2) {
                Js::Undefined => None,
                x => Some(
                    safe_cex_exchange::js_runtime::value::to_retained_input(&x.clone())
                        .map_err(|error| thrown(r, error.to_exchange_error()))?,
                ),
            };
            b.emit_and_log_error(input, message.as_deref(), extra.as_ref())
                .map_err(|e| thrown(r, e))?;
            Ok(Js::Undefined)
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
                    Some(id) => Js::str(&id),
                    None => Js::Undefined,
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
            Ok(Js::Undefined)
        }),
    );
    method(
        realm,
        "getTickInterval",
        0,
        Rc::new(|_, b, _, _| Ok(b.get_tick_interval().map_or(Js::Undefined, Js::Number))),
    );
    method(
        realm,
        "getActiveTickerSymbols",
        0,
        Rc::new(|r, b, _, _| {
            let symbols = b.get_active_ticker_symbols();
            r.encode_typed(&symbols)
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
                Ok(Js::Bool(used))
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
                let fills = move |options: safe_cex_types::ExecutionsOptions| {
                    let (mut realm, owner) = (fills_realm.clone(), fills_owner.clone());
                    Box::pin(async move {
                        let options = realm
                            .encode_typed(&options)
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
                let income = move |from, to| {
                    let (mut realm, owner) = (income_realm.clone(), owner.clone());
                    Box::pin(async move {
                        let result = realm
                            .call_method(
                                &owner,
                                "fetchIncomeHistoryRange",
                                &[Js::Number(from), Js::Number(to)],
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
                let settle = move |symbol: &str| {
                    Ok(base
                        .store
                        .markets_by_symbol()?
                        .get(symbol)
                        .map(|market| market.quote.clone())
                        .filter(|quote| !quote.is_empty())
                        .unwrap_or_else(|| "USDT".into()))
                };
                let result = build_folded_position_records(
                    FoldableExchange {
                        runtime: realm.runtime(),
                        fetch_historical_fills: Rc::new(fills),
                        fetch_income_history_range: Some(Rc::new(income)),
                    },
                    BuildRecordsOptions {
                        account,
                        from,
                        to,
                        symbol_scoped,
                        resolve_settle_currency: Rc::new(settle),
                    },
                )
                .await
                .map_err(|error| thrown(&realm, error.error))?;
                realm.encode_typed(&result.records)
            })))
        }),
    );
    method(
        realm,
        "dispose",
        0,
        Rc::new(|r, b, _, _| {
            b.dispose().map_err(|e| thrown(r, e))?;
            Ok(Js::Undefined)
        }),
    );
    let b = base.clone();
    let disposed = realm.native("get isDisposed", 0, move |_, _, _| {
        Ok(Js::Bool(b.is_disposed()))
    });
    let setter = base.clone();
    prototype
        .define_property(
            &"isDisposed".encode_utf16().collect::<Vec<_>>(),
            &Js::object(vec![
                ("get", disposed),
                (
                    "set",
                    realm.native("set isDisposed", 1, move |_, _, args| {
                        setter.set_disposed_flag(arg(args, 0).truthy());
                        Ok(Js::Undefined)
                    }),
                ),
                ("configurable", Js::Bool(true)),
            ]),
        )
        .expect("fresh base prototype getter");

    let b = base.clone();
    let public_only = realm.native("get isPublicDataOnly", 0, move |_, _, _| {
        Ok(Js::Bool(
            b.options().public_data_only == Some(true) || !b.can_sign(),
        ))
    });
    realm.define_getter(&prototype, Js::str("isPublicDataOnly"), public_only);
    let b = base.clone();
    let name = realm.native("get name", 0, move |r, _, _| r.encode_typed(b.name()));
    realm.define_getter(&prototype, Js::str("name"), name);
    class
}
