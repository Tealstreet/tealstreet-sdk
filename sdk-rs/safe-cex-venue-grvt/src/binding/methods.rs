//! Dispatch the assembled exchange members to their existing native bodies.
use super::runtime::{Host, thrown};
use crate::{account_runtime, trading_runtime};
use safe_cex_exchange::{
    js_runtime::{
        promise::await_js,
        value::{Js, Thrown, arg, from_json, from_typed, to_json, to_retained_input, to_typed},
    },
    runtime::{ExchangeError, LogSeverity},
};
use safe_cex_types::{
    Market, OHLCVOptions, Order, PlaceOrderOpts, Position, PositionMode, UpdateOrderOpts,
};
use std::rc::Rc;

fn error(e: ExchangeError) -> Thrown {
    e.into()
}
fn log(h: &Host, text: &str, level: LogSeverity) -> Result<(), ExchangeError> {
    h.log(text, level.as_str())
        .map_err(|e| e.to_exchange_error())
}
fn account(h: &Host) -> Option<String> {
    h.0.this
        .get("apiExtras")
        .and_then(|v| v.get("subAccountId"))
        .ok()
        .filter(Js::truthy)
        .and_then(|v| v.as_str().map(|v| String::from_utf16_lossy(v)))
}
fn has_client(h: &Host) -> bool {
    h.0.this.get("grvtClient").is_ok_and(|v| v.truthy())
}
fn call(h: &Host, name: &str, args: Vec<Js>) -> Result<Js, Thrown> {
    h.0.this.call_method(name, args)
}
async fn request(
    h: Host,
    slot: &str,
    member: &str,
    args: Vec<Js>,
) -> Result<serde_json::Value, ExchangeError> {
    let result = async {
        let v = await_js(
            &h.0.exchange_runtime(),
            h.0.this.get(slot)?.call_method(member, args)?,
        )
        .await?;
        to_json(&v.get("data")?)
    }
    .await;
    result.map_err(|e| e.to_exchange_error())
}
async fn endpoint(h: Host) -> Result<serde_json::Value, ExchangeError> {
    let result = async {
        to_json(
            &await_js(
                &h.0.exchange_runtime(),
                h.0.this
                    .get("grvtClient")?
                    .call_method("authenticatedEndpoint", vec![])?,
            )
            .await?,
        )
    }
    .await;
    result.map_err(|e| e.to_exchange_error())
}
async fn tdg(
    h: Host,
    member: &'static str,
    payload: serde_json::Value,
    config: serde_json::Value,
) -> Result<serde_json::Value, ExchangeError> {
    let result = async {
        to_json(
            &await_js(
                &h.0.exchange_runtime(),
                h.0.this
                    .get("grvtClient")?
                    .get("tdgClient")?
                    .call_method(member, vec![from_json(&payload), from_json(&config)])?,
            )
            .await?,
        )
    }
    .await;
    result.map_err(|e| e.to_exchange_error())
}

pub(super) fn invoke(h: Host, name: &'static str, args: Vec<Js>) -> Result<Js, Thrown> {
    let a = arg(&args, 0);
    let host = Rc::new(h.clone());
    let future = match name {
        "start" => Some(account_runtime::start(host)),
        "tick" => Some(account_runtime::tick(host)),
        "authenticate" => Some(account_runtime::authenticate(host)),
        "validateAccount" => Some(account_runtime::validate_account(host)),
        "fetchBuilderAuthorization" => Some(account_runtime::fetch_builder_authorization(host)),
        "getBuilderInfo" => Some(account_runtime::get_builder_info(host)),
        "fetchBalance" => Some(account_runtime::fetch_balance(host)),
        "fetchPositions" => Some(account_runtime::fetch_positions(host, a.clone())),
        "fetchOpenOrders" => Some(account_runtime::fetch_open_orders(host, a.clone())),
        "placeOrder" => Some(trading_runtime::place_order(host, a.clone())),
        "fetchFillsForOrderIds" => {
            Some(trading_runtime::fetch_fills_for_order_ids(host, a.clone()))
        }
        _ => None,
    };
    if let Some(future) = future {
        return Ok(h.spawn(future));
    }
    match name {
        "mapOrder" => thrown(account_runtime::map_order(&h, a)),
        "getPositionMode" => {
            Ok(h.0.spawn(Box::pin(async { from_typed(&PositionMode::OneWay) })))
        }
        "symbolToInstrumentId" => Ok(Js::str(&crate::order_semantics::symbol_to_instrument_id(
            &String::from_utf16_lossy(&a.to_js_string()?),
        ))),
        "mapTimeInForceToEnum" => Ok(Js::Number(
            crate::order_semantics::map_time_in_force_to_enum(to_typed(&a)?),
        )),
        "convertInterval" => match crate::order_semantics::convert_interval(
            &String::from_utf16_lossy(&a.to_js_string()?),
        ) {
            safe_cex_num::TableHit::Mapped(v) => from_typed(&v),
            safe_cex_num::TableHit::Missing => Ok(Js::Undefined),
            safe_cex_num::TableHit::Inherited(member) => Ok(Js::Builtin(
                safe_cex_num::PrototypeRealm::current().reference(member),
            )),
        },
        "mapOrderStatus" => from_typed(&crate::order_semantics::map_order_status(Some(&to_json(
            &a,
        )?))),
        "getOrderCreationTimeMs" => Ok(crate::order_semantics::get_order_creation_time_ms(Some(
            &to_json(&a)?,
        ))
        .map_or(Js::Undefined, Js::Number)),
        "getInstrumentSigningMetadata" => {
            let market: Market = to_typed(&a)?;
            let (hash, base, quote) = crate::signing_metadata::get_instrument_signing_metadata(
                &market.id,
                market.info.as_ref(),
            )
            .map_err(|e| Thrown::error("Error", &e))?;
            Ok(Js::object(vec![
                ("ih", Js::str(&hash)),
                ("bd", Js::Number(base)),
                ("qd", Js::Number(quote)),
            ]))
        }
        "recordNonce" => super::nonce::record(&h, args),
        "getFakePositions" => from_typed(
            &crate::snapshots::get_fake_positions(
                h.0.shared.store.as_ref(),
                h.0.this.get("defaultLeverage")?.to_number()?,
            )
            .map_err(error)?,
        ),
        "calculatePositionUpnlFromLastPrice" => {
            Ok(crate::snapshots::calculate_position_upnl_from_last_price(
                &to_typed(&a)?,
                h.0.shared.store.as_ref(),
            )
            .map_or(Js::Null, Js::Number))
        }
        "calculateUpnlFromPositions" => {
            Ok(Js::Number(crate::snapshots::calculate_upnl_from_positions(
                &to_typed::<Vec<Position>>(&a)?,
                h.0.shared.store.as_ref(),
            )))
        }
        "syncBalanceUpnlFromPositions" => {
            let positions = if a.is_undefined() {
                None
            } else {
                Some(to_typed::<Vec<Position>>(&a)?)
            };
            crate::snapshots::sync_balance_upnl_from_positions(
                h.0.shared.store.as_ref(),
                positions.as_deref(),
            )
            .map_err(error)?;
            Ok(Js::Undefined)
        }
        "fetchOrders" => {
            let symbol = if a.is_array() && a.get("length")?.to_number()? == 1.0 {
                a.get("0")?
            } else {
                Js::Undefined
            };
            call(&h, "fetchOpenOrders", vec![symbol])
        }
        "fetchTickers" => {
            let future =
                crate::snapshots::fetch_tickers(h.0.shared.store.as_ref(), |text, level| {
                    log(&h, text, level)
                });
            Ok(h.0.spawn(Box::pin(async move {
                from_typed(&future.await.map_err(error)?)
            })))
        }
        "fetchMarkets" => {
            let scope = h.0.clone();
            Ok(scope.spawn(Box::pin(async move {
                from_typed(
                    &crate::markets::fetch_markets(
                        |path, body| {
                            let response = h.0.this.get("xhr").and_then(|xhr| {
                                xhr.call_method("post", vec![Js::str(path), from_json(&body)])
                            });
                            {
                                let await_runtime = h.0.exchange_runtime();
                                async move {
                                    let response = await_js(
                                        &await_runtime,
                                        response.map_err(|cause| cause.to_exchange_error())?,
                                    )
                                    .await
                                    .map_err(|cause| cause.to_exchange_error())?;
                                    to_retained_input(
                                        &response
                                            .get("data")
                                            .map_err(|cause| cause.to_exchange_error())?,
                                    )
                                    .map_err(|cause| cause.to_exchange_error())
                                }
                            }
                        },
                        |text, level| log(&h, text, level),
                    )
                    .await
                    .map_err(error)?,
                )
            })))
        }
        "getAccount" => {
            let scope = h.0.clone();
            Ok(scope.spawn(Box::pin(async move {
                let sub = account(&h);
                let (id, affiliate) = crate::snapshots::get_account(
                    sub.as_deref(),
                    || {
                        h.0.this
                            .get("apiExtras")
                            .and_then(|v| v.get("accountId"))
                            .ok()
                            .filter(Js::truthy)
                            .and_then(|v| v.as_str().map(|v| String::from_utf16_lossy(v)))
                    },
                    |path, body| {
                        request(
                            h.clone(),
                            "tradingXhr",
                            "post",
                            vec![Js::str(path), from_json(&body)],
                        )
                    },
                    |text, level| log(&h, text, level),
                )
                .await
                .map_err(error)?;
                Ok(Js::object(vec![
                    ("userId", Js::str(&id)),
                    ("affiliateId", affiliate.map_or(Js::Null, |v| Js::str(&v))),
                ]))
            })))
        }
        "refreshMarkets" => {
            let scope = h.0.clone();
            Ok(scope.spawn(Box::pin(async move {
                let changes = crate::markets::refresh_markets(
                    h.0.shared.store.as_ref(),
                    || {
                        let result = call(&h, "fetchMarkets", vec![]);
                        {
                            let await_runtime = h.0.exchange_runtime();
                            async move {
                                let v = await_js(
                                    &await_runtime,
                                    result.map_err(|e| e.to_exchange_error())?,
                                )
                                .await
                                .map_err(|e| e.to_exchange_error())?;
                                to_typed(&v).map_err(|e| e.to_exchange_error())
                            }
                        }
                    },
                    |event, changes| {
                        h.0.this
                            .get("emitter")
                            .and_then(|v| {
                                v.call_method("emit", vec![Js::str(event), changes_js(changes)?])
                            })
                            .map(|_| ())
                            .map_err(|e| e.to_exchange_error())
                    },
                    |text, level| log(&h, text, level),
                )
                .await
                .map_err(error)?;
                changes_js(&changes)
            })))
        }
        "fetchOHLCV" | "fetchOrderBook" => {
            let scope = h.0.clone();
            Ok(scope.spawn(Box::pin(async move {
                if name == "fetchOHLCV" {
                    let opts: OHLCVOptions = to_typed(&a)?;
                    from_typed(
                        &crate::data_reads::fetch_ohlcv(
                            h.0.shared.store.as_ref(),
                            &opts,
                            |path, body| {
                                request(
                                    h.clone(),
                                    "xhr",
                                    "post",
                                    vec![
                                        Js::str(path),
                                        from_typed(&body).expect("data request serialization"),
                                    ],
                                )
                            },
                            |text, level| log(&h, text, level),
                        )
                        .await
                        .map_err(error)?,
                    )
                } else {
                    from_typed(
                        &crate::data_reads::fetch_order_book(
                            h.0.shared.store.as_ref(),
                            &String::from_utf16_lossy(&a.to_js_string()?),
                            |path, body| {
                                request(
                                    h.clone(),
                                    "xhr",
                                    "post",
                                    vec![
                                        Js::str(path),
                                        from_typed(&body).expect("data request serialization"),
                                    ],
                                )
                            },
                            |text, level| log(&h, text, level),
                        )
                        .await
                        .map_err(error)?,
                    )
                }
            })))
        }
        "listenOHLCV" | "listenOrderBook" => {
            h.0.this.get("publicWebsocket")?.call_method(name, args)
        }
        "placeOrders" => place_orders(h, a),
        "updateOrder" => {
            let scope = h.0.clone();
            Ok(scope.spawn(Box::pin(async move {
                let field_count = a.get("update")?.own_keys().len();
                let opts: UpdateOrderOpts = to_typed(&a)?;
                let ids = crate::order_controls::update_order_with_field_count(
                    &opts,
                    field_count,
                    |order| {
                        let result =
                            from_typed(order).and_then(|v| call(&h, "cancelOrder", vec![v]));
                        {
                            let await_runtime = h.0.exchange_runtime();
                            async move {
                                await_js(&await_runtime, result.map_err(|e| e.to_exchange_error())?)
                                    .await
                                    .map(|_| ())
                                    .map_err(|e| e.to_exchange_error())
                            }
                        }
                    },
                    |order| {
                        let result =
                            from_typed(&order).and_then(|v| call(&h, "placeOrder", vec![v]));
                        {
                            let await_runtime = h.0.exchange_runtime();
                            async move {
                                let v = await_js(
                                    &await_runtime,
                                    result.map_err(|e| e.to_exchange_error())?,
                                )
                                .await
                                .map_err(|e| e.to_exchange_error())?;
                                to_typed(&v).map_err(|e| e.to_exchange_error())
                            }
                        }
                    },
                    |text, level| log(&h, text, level),
                    |text| {
                        call(&h, "emitAndLogError", vec![Js::str(text)])
                            .map(|_| ())
                            .map_err(|e| e.to_exchange_error())
                    },
                )
                .await
                .map_err(error)?;
                from_typed(&ids)
            })))
        }
        "dispose" => {
            // The shared disposal cell is also read by every snapshot continuation.
            h.0.this.set("isDisposed", Js::Bool(true))?;
            h.0.this
                .get("publicWebsocket")?
                .call_method("dispose", vec![])?;
            h.0.this
                .get("privateWebsocket")?
                .call_method("dispose", vec![])?;
            Ok(Js::Undefined)
        }
        "fetchLeverageForSymbol" | "setLeverage" | "updateMarketLeverageLimits" => {
            leverage(h, name, args)
        }
        "cancelOrder" | "cancelOrders" | "cancelAllOrders" | "cancelSymbolOrders" => {
            cancel(h, name, args)
        }
        _ => Err(Thrown::error(
            "Error",
            &format!("unwired GRVT member: {name}"),
        )),
    }
}

fn leverage(h: Host, name: &'static str, args: Vec<Js>) -> Result<Js, Thrown> {
    let symbol = String::from_utf16_lossy(&arg(&args, 0).to_js_string()?);
    let scope = h.0.clone();
    Ok(scope.spawn(Box::pin(async move {
        let store = h.0.shared.store.clone();
        let exists = has_client(&h);
        let acc = || account(&h);
        let default = || {
            h.0.this
                .get("defaultLeverage")
                .and_then(|v| v.to_number())
                .unwrap_or(f64::NAN)
        };
        let auth = || endpoint(h.clone());
        let get = |body, config| tdg(h.clone(), "getAllInitialLeverage", body, config);
        if name == "fetchLeverageForSymbol" {
            let result = crate::data_reads::fetch_leverage_for_symbol(
                store.as_ref(),
                &symbol,
                exists,
                acc,
                default,
                auth,
                get,
                |text, level| log(&h, text, level),
            )
            .await
            .map_err(error)?;
            result.map_or(Ok(Js::Undefined), |v| from_typed(&v))
        } else if name == "setLeverage" {
            crate::data_reads::set_leverage(
                store.as_ref(),
                &symbol,
                arg(&args, 1).to_number()?,
                exists,
                acc,
                auth,
                |body, config| tdg(h.clone(), "setInitialLeverage", body, config),
                |text, level| log(&h, text, level),
            )
            .await
            .map_err(error)?;
            Ok(Js::Undefined)
        } else {
            let markets: Vec<Market> = to_typed(&arg(&args, 0))?;
            let result = crate::data_reads::update_market_leverage_limits(
                &markets,
                exists,
                acc,
                auth,
                get,
                |text, level| log(&h, text, level),
            )
            .await
            .map_err(error)?;
            from_typed(result.as_ref())
        }
    })))
}
fn cancel(h: Host, name: &'static str, args: Vec<Js>) -> Result<Js, Thrown> {
    let scope = h.0.clone();
    Ok(scope.spawn(Box::pin(async move {
        let exists = has_client(&h);
        let acc = || account(&h);
        let auth = || endpoint(h.clone());
        let cancel = |body, config| async {
            tdg(
                h.clone(),
                if name == "cancelOrder" {
                    "cancelOrder"
                } else {
                    "cancelAllOrders"
                },
                body,
                config,
            )
            .await
            .map(|_| ())
        };
        let logger = |text: &str, level| log(&h, text, level);
        let emit = |text: &str| {
            call(&h, "emitAndLogError", vec![Js::str(text)])
                .map(|_| ())
                .map_err(|e| e.to_exchange_error())
        };
        match name {
            "cancelOrder" => {
                let order: Order = to_typed(&arg(&args, 0))?;
                let real =
                    h.0.this
                        .get("_nonceToOrderId")?
                        .call_method("get", vec![Js::str(&order.id)])?;
                let real = real.as_str().map(|v| String::from_utf16_lossy(v));
                crate::order_controls::cancel_order(
                    &order,
                    exists,
                    acc,
                    real.as_deref(),
                    auth,
                    cancel,
                    logger,
                    emit,
                )
                .await
                .map_err(error)?;
            }
            "cancelAllOrders" => {
                crate::order_controls::cancel_all_orders(exists, acc, auth, cancel, logger, emit)
                    .await
                    .map_err(error)?
            }
            "cancelSymbolOrders" => crate::order_controls::cancel_symbol_orders(
                &String::from_utf16_lossy(&arg(&args, 0).to_js_string()?),
                exists,
                acc,
                auth,
                cancel,
                logger,
                emit,
            )
            .await
            .map_err(error)?,
            _ => {
                let orders: Vec<Order> = to_typed(&arg(&args, 0))?;
                crate::order_controls::cancel_orders(
                    &orders,
                    |order| {
                        let result =
                            from_typed(order).and_then(|v| call(&h, "cancelOrder", vec![v]));
                        {
                            let await_runtime = h.0.exchange_runtime();
                            async move {
                                await_js(&await_runtime, result.map_err(|e| e.to_exchange_error())?)
                                    .await
                                    .map(|_| ())
                                    .map_err(|e| e.to_exchange_error())
                            }
                        }
                    },
                    logger,
                )
                .await
                .map_err(error)?;
            }
        }
        Ok(Js::Undefined)
    })))
}

fn place_orders(h: Host, input: Js) -> Result<Js, Thrown> {
    let scope = h.0.clone();
    Ok(scope.spawn(Box::pin(async move {
        use crate::order_controls::BatchOrderError;
        let orders: Vec<PlaceOrderOpts> = to_typed(&input)?;
        let original = Rc::new(std::cell::RefCell::new(None::<Thrown>));
        let result = crate::order_controls::place_orders(
            &orders,
            |_| {
                call(&h, "assertCallerClientOrderIds", vec![input.clone()])
                    .map(|_| ())
                    .map_err(|e| e.to_exchange_error())
            },
            |order| {
                let pending = from_typed(order).and_then(|v| call(&h, "placeOrder", vec![v]));
                let host = h.clone();
                let original = original.clone();
                {
                    let await_runtime = h.0.exchange_runtime();
                    async move {
                        let result =
                            async { to_typed(&await_js(&await_runtime, pending?).await?) }.await;
                        result.map_err(|cause: Thrown| {
                            if crate::trading_runtime::TradingHost::is_builder_error(
                                &host,
                                &cause.value,
                            ) {
                                let v = &cause.value;
                                let builder = crate::builder::GrvtBuilderUnauthorizedError::new(
                                    to_typed(&v.get("reason").unwrap_or(Js::Undefined)).unwrap_or(
                                        crate::types::GrvtBuilderUnauthorizedReason::Unauthorized,
                                    ),
                                    v.get("requiredRaw")
                                        .and_then(|v| v.to_number())
                                        .unwrap_or(f64::NAN),
                                    v.get("authorizedRaw").ok().and_then(|v| v.as_f64()),
                                    None,
                                );
                                *original.borrow_mut() = Some(cause);
                                BatchOrderError::Builder(builder)
                            } else {
                                BatchOrderError::Exchange(cause.to_exchange_error())
                            }
                        })
                    }
                }
            },
            |text, level| log(&h, text, level),
        )
        .await;
        match result {
            Ok(ids) => from_typed(&ids),
            Err(BatchOrderError::Exchange(e)) => Err(error(e)),
            Err(BatchOrderError::Builder(_)) => Err(original
                .borrow_mut()
                .take()
                .expect("builder identity retained")),
        }
    })))
}

fn changes_js(changes: &safe_cex_exchange::snapshots::MarketChanges) -> Result<Js, Thrown> {
    Ok(Js::object(vec![
        ("added", from_typed(&changes.added)?),
        ("removed", from_typed(&changes.removed)?),
    ]))
}
