//! HyroTrader receiver overrides, over its retained source capabilities.
use crate::{
    binding::{
        promise::await_js,
        rt::Rt,
        value::{Js, Thrown, arg},
    },
    hyro_capabilities as capabilities, hyro_private_reads as reads, hyro_snapshot as snapshots,
};

pub(crate) fn install(rt: &Rt) -> Result<(), Thrown> {
    for name in [
        "hyrotraderMode",
        "getConfiguredUrls",
        "shouldUseLocalRestOrderLifecycleFallback",
        "getBybitRestBaseUrl",
        "getBybitPublicWsUrl",
        "getBybitPrivateWsUrl",
    ] {
        let weak = std::rc::Rc::downgrade(&rt.shared);
        rt.this.define(
            name,
            Js::function(name, move |owner, args| {
                let rt = safe_cex_exchange::js_runtime::scope::scope_of(&weak)?;
                match name {
                    "hyrotraderMode" => Ok(Js::str(crate::hyro_configuration::hyrotrader_mode(
                        &crate::binding::rt::venue(&rt)?.base.options(),
                    ))),
                    "getConfiguredUrls" => {
                        let venue = crate::binding::rt::venue(&rt)?;
                        let (label, _, _) =
                            crate::hyro_configuration::get_configured_urls(&venue.base.options());
                        let (rest, ws) = if label == "simulator" {
                            ("HYROTRADER_SIMULATOR_URL", "HYROTRADER_SIMULATOR_WS_URL")
                        } else {
                            ("HYROTRADER_PROD_URL", "HYROTRADER_PROD_WS_URL")
                        };
                        Ok(Js::object(vec![
                            ("label", Js::str(label)),
                            ("rest", rt.global(rest)?),
                            ("ws", rt.global(ws)?),
                        ]))
                    }
                    "shouldUseLocalRestOrderLifecycleFallback" => Ok(Js::Bool(
                        crate::hyro_configuration::should_use_local_rest_order_lifecycle_fallback(),
                    )),
                    "getBybitRestBaseUrl" => {
                        snapshots::get_bybit_rest_base_url(&rt, owner, arg(&args, 0))
                    }
                    "getBybitPublicWsUrl" => {
                        snapshots::get_bybit_public_ws_url(&rt, owner, arg(&args, 0), arg(&args, 1))
                    }
                    "getBybitPrivateWsUrl" => {
                        snapshots::get_bybit_private_ws_url(&rt, owner, arg(&args, 0))
                    }
                    _ => unreachable!(),
                }
            }),
        )?;
    }
    for field in ["snapshotVersion", "ordersSnapshotVersion"] {
        rt.this.define(field, Js::Number(0.0))?;
    }
    rt.this.define("activeOrderObservations", Js::new_set())?;
    rt.this.define("pendingOrderLists", Js::new_map())?;
    let recovery = Js::object(vec![("pending", Js::new_map())]);
    for name in [
        "observeOrders",
        "dispose",
        "getClient",
        "isDisposed",
        "assertCanCreate",
        "getPublicationBoundary",
    ] {
        let weak = std::rc::Rc::downgrade(&rt.shared);
        recovery.define(
            name,
            Js::function(name, move |owner, args| {
                let rt = safe_cex_exchange::js_runtime::scope::scope_of(&weak)?;
                match name {
                    "observeOrders" => {
                        crate::hyro_order_publication::observe_orders(&rt, owner, arg(&args, 0))
                    }
                    "dispose" => crate::hyro_order_publication::dispose(&rt, owner),
                    "getClient" => rt.this.get("xhr"),
                    "isDisposed" => rt.this.get("isDisposed"),
                    "assertCanCreate" => rt.this.call_method("assertPrivateReady", vec![]),
                    _ => {
                        let session = rt.this.get("privateWebsocket")?.get("sessionVersion")?;
                        let snapshot = rt.this.get("ordersSnapshotVersion")?;
                        let first =
                            safe_cex_exchange::js_runtime::ops::add(&session, &Js::str(":"))?;
                        safe_cex_exchange::js_runtime::ops::add(&first, &snapshot)
                    }
                }
            }),
        )?;
    }
    let key = super::hyro_recovery::submission_key(rt)?;
    super::hyro_recovery::install(rt, &recovery, key.clone())?;
    rt.this.define("orderRecovery", recovery)?;
    for name in ["assertPrivateReady", "validateOrderCapabilities"] {
        let weak = std::rc::Rc::downgrade(&rt.shared);
        rt.this.define(
            name,
            Js::function(name, move |owner, args| {
                let rt = safe_cex_exchange::js_runtime::scope::scope_of(&weak)?;
                match name {
                    "assertPrivateReady" => capabilities::assert_private_ready(&rt, owner),
                    _ => capabilities::validate_order_capabilities(&rt, owner, arg(&args, 0)),
                }
            }),
        )?;
    }
    for name in ["submitCreateOrders", "cancelOrdersHelper", "placeOrders"] {
        let weak = std::rc::Rc::downgrade(&rt.shared);
        let key = key.clone();
        rt.this.define(
            name,
            Js::function(name, move |owner, args| {
                let rt = safe_cex_exchange::js_runtime::scope::scope_of(&weak)?;
                let body = rt.clone();
                let key = key.clone();
                Ok(rt.spawn(Box::pin(async move {
                    match name {
                        "submitCreateOrders" => {
                            crate::hyro_order_publication::submit_create_orders(
                                &body,
                                owner,
                                arg(&args, 0),
                                arg(&args, 1),
                            )
                            .await
                        }
                        "cancelOrdersHelper" => {
                            crate::hyro_order_publication::cancel_orders_helper(
                                &body,
                                owner,
                                arg(&args, 0),
                                arg(&args, 1),
                            )
                            .await
                        }
                        _ => place_orders(&body, owner, arg(&args, 0), key).await,
                    }
                })))
            }),
        )?;
    }
    let weak = std::rc::Rc::downgrade(&rt.shared);
    for name in ["fetchIncomeHistory", "fetchIncomeHistoryRange"] {
        let weak = weak.clone();
        rt.this.define(
            name,
            Js::function(name, move |_, _| {
                let rt = safe_cex_exchange::js_runtime::scope::scope_of(&weak)?;
                Ok(Js::Promise(super::super::promise::Promise::settled(
                    &rt.exchange_runtime(),
                    Ok(Js::array(vec![])),
                )))
            }),
        )?;
    }
    rt.this.define(
        "validateCandleInterval",
        Js::function("validateCandleInterval", |owner, args| {
            if arg(&args, 0)
                .call_method("endsWith", vec![Js::str("s")])?
                .truthy()
            {
                let error = super::super::value::error_object(
                    "Error",
                    "HyroTrader seconds candles require a supported trade stream",
                );
                owner.call_method("emitAndLogError", vec![error.clone()])?;
                return Err(Thrown::new(error));
            }
            Ok(Js::Undefined)
        }),
    )?;
    let dispose = rt.this.get("dispose")?;
    rt.this.define(
        "dispose",
        Js::function("dispose", move |owner, _| {
            owner.get("orderRecovery")?.call_method("dispose", vec![])?;
            owner
                .get("activeOrderObservations")?
                .call_method("clear", vec![])?;
            owner
                .get("pendingOrderLists")?
                .call_method("clear", vec![])?;
            dispose.call(owner, vec![])
        }),
    )?;
    let weak = std::rc::Rc::downgrade(&rt.shared);
    rt.this.define(
        "observePrivateOrders",
        Js::function("observePrivateOrders", move |owner, args| {
            let rt = safe_cex_exchange::js_runtime::scope::scope_of(&weak)?;
            crate::hyro_order_publication::observe_private_orders(&rt, owner, arg(&args, 0))
        }),
    )?;
    super::super::hyro_socket::install(rt)?;
    rt.this
        .define("supportedCategories", Js::array(vec![Js::str("linear")]))?;
    rt.this.define(
        "getSettleCoinsForCategory",
        Js::function("getSettleCoinsForCategory", |this, args| {
            let category = arg(&args, 0);
            if category.strict_equals(&Js::str("linear")) {
                return Ok(Js::array(vec![Js::str("USDT")]));
            }
            this.call_method(
                "fail",
                vec![Js::str(&format!(
                    "Unknown category {}",
                    category.to_display()
                ))],
            )
        }),
    )?;
    rt.this.define(
        "isFiniteNumeric",
        Js::function("isFiniteNumeric", |_, args| {
            let value = arg(&args, 0);
            if !matches!(value.type_of(), "number" | "string") {
                return Ok(Js::Bool(false));
            }
            let trimmed = Js::String(value.to_js_string()?).call_method("trim", vec![])?;
            Ok(Js::Bool(trimmed.truthy() && value.to_number()?.is_finite()))
        }),
    )?;
    let map_rt = std::rc::Rc::downgrade(&rt.shared);
    rt.this.define(
        "mapPosition",
        Js::function("mapPosition", move |this, args| {
            reads::map_position(
                &safe_cex_exchange::js_runtime::scope::scope_of(&map_rt)?,
                this,
                arg(&args, 0),
            )
        }),
    )?;
    let validate_rt = std::rc::Rc::downgrade(&rt.shared);
    rt.this.define(
        "assertPrivateListResponse",
        Js::function("assertPrivateListResponse", move |this, args| {
            reads::assert_private_list_response(
                &safe_cex_exchange::js_runtime::scope::scope_of(&validate_rt)?,
                this,
                arg(&args, 0),
                arg(&args, 1),
            )?;
            Ok(Js::Undefined)
        }),
    )?;
    let map_rt = std::rc::Rc::downgrade(&rt.shared);
    rt.this.define(
        "mapValidatedOrders",
        Js::function("mapValidatedOrders", move |this, args| {
            reads::map_validated_orders(
                &safe_cex_exchange::js_runtime::scope::scope_of(&map_rt)?,
                this,
                arg(&args, 0),
            )
        }),
    )?;
    let fail_rt = std::rc::Rc::downgrade(&rt.shared);
    rt.this.define(
        "fail",
        Js::function("fail", move |this, args| {
            capabilities::fail(
                &safe_cex_exchange::js_runtime::scope::scope_of(&fail_rt)?,
                this,
                arg(&args, 0),
            )
        }),
    )?;
    for name in [
        "getAccount",
        "fetchApiKeyInfo",
        "fetchAndCacheApiKeyInfo",
        "fetchFees",
        "fetchPositionMode",
        "changePositionMode",
        "changeMarginType",
        "fetchRawBalance",
        "fetchOrders",
        "fetchOrdersHelper",
        "fetchPrivateRows",
        "fetchPositionsHelper",
        "validateAccount",
        "fetchMarginAccountInfos",
        "fetchMarkets",
        "reconcilePrivateState",
        "fetchAndSetPrivateSnapshot",
        "fetchOrdersPositionsAndBalance",
        "fetchAndSetPrivateSnapshotHelper",
    ] {
        let weak = std::rc::Rc::downgrade(&rt.shared);
        rt.this.define(
            name,
            Js::function(name, move |_, args| {
                let rt = safe_cex_exchange::js_runtime::scope::scope_of(&weak)?;
                let body = rt.clone();
                Ok(rt.spawn(Box::pin(async move {
                    let owner = body.this.clone();
                    match name {
                        "fetchMarkets" => snapshots::fetch_markets(&body, owner).await,
                        "reconcilePrivateState" => snapshots::reconcile_private_state(&body, owner, arg(&args, 0), arg(&args, 1)).await,
                        "fetchAndSetPrivateSnapshot" => snapshots::fetch_and_set_private_snapshot(&body, owner).await,
                        "fetchOrdersPositionsAndBalance" => snapshots::fetch_orders_positions_and_balance(&body, owner).await,
                        "fetchAndSetPrivateSnapshotHelper" => snapshots::fetch_and_set_private_snapshot_helper(&body, owner, arg(&args, 0)).await,
                        "validateAccount" => {
                            let balance = match owner.call_method("fetchBalance", vec![]) {
                                Ok(promise) => await_js(&rt.exchange_runtime(), promise).await,
                                Err(error) => Err(error),
                            };
                            match balance {
                                Ok(balance) => Ok(Js::object(vec![("error", Js::str("")), ("balance", balance)])),
                                Err(error) => {
                                    let message = if matches!(&error.value, Js::Object(data) if data.class == "Error") {
                                        error.value.get("message")?
                                    } else {
                                        body.global("String")?.call(Js::Undefined, vec![error.value])?
                                    };
                                    owner.call_method("emitAndLogError", vec![message.clone()])?;
                                    Ok(Js::object(vec![("error", message)]))
                                }
                            }
                        }
                        "fetchMarginAccountInfos" => {
                            let balance = await_js(&rt.exchange_runtime(), owner.call_method("fetchBalance", vec![])?).await?;
                            if !owner.get("isDisposed")?.truthy() {
                                owner.get("store")?.call_method("update", vec![Js::object(vec![("balance", balance)])])?;
                            }
                            Ok(Js::Undefined)
                        }
                        "getAccount" => capabilities::get_account(&body, owner).await,
                        "fetchApiKeyInfo" => capabilities::fetch_api_key_info(&body, owner).await,
                        "fetchAndCacheApiKeyInfo" => {
                            capabilities::fetch_and_cache_api_key_info(&body).await
                        }
                        "fetchFees" => capabilities::fetch_fees(&body, arg(&args, 0)).await,
                        "fetchPositionMode" => {
                            capabilities::fetch_position_mode(&body, arg(&args, 0)).await
                        }
                        "changePositionMode" => {
                            capabilities::change_position_mode(
                                &body,
                                owner,
                                arg(&args, 0),
                                arg(&args, 1),
                            )
                            .await
                        }
                        "changeMarginType" => {
                            capabilities::change_margin_type(&body, owner, arg(&args, 0)).await
                        }
                        "fetchRawBalance" => reads::fetch_raw_balance(&body, owner).await,
                        "fetchOrders" => reads::fetch_orders(&body, owner).await,
                        "fetchOrdersHelper" => {
                            reads::fetch_orders_helper(&body, owner, arg(&args, 0), arg(&args, 1))
                                .await
                        }
                        "fetchPrivateRows" => {
                            reads::fetch_private_rows(&body, owner, arg(&args, 0), arg(&args, 1))
                                .await
                        }
                        "fetchPositionsHelper" => {
                            reads::fetch_positions_helper(
                                &body,
                                owner,
                                arg(&args, 0),
                                arg(&args, 1),
                                arg(&args, 2),
                            )
                            .await
                        }
                        _ => unreachable!(),
                    }
                })))
            }),
        )?;
    }
    Ok(())
}

async fn place_orders(rt: &Rt, owner: Js, orders: Js, key_fn: Js) -> Result<Js, Thrown> {
    owner.call_method("assertCallerClientOrderIds", vec![orders.clone()])?;
    let validate = owner.clone();
    orders.call_method(
        "forEach",
        vec![Js::function("validateOrderCapabilities", move |_, args| {
            validate.call_method("validateOrderCapabilities", vec![arg(&args, 0)])
        })],
    )?;
    let key = key_fn.call(Js::Undefined, vec![orders.clone()])?;
    let pending = owner.get("pendingOrderLists")?;
    let mut current = pending.call_method("get", vec![key.clone()])?;
    if !current.is_nullish() && current.get("running")?.truthy() {
        return await_js(&rt.exchange_runtime(), current.get("running")?).await;
    }
    if !current.truthy() {
        current = Js::object(vec![("index", Js::Number(0.0)), ("ids", Js::array(vec![]))]);
        pending.call_method("set", vec![key.clone(), current.clone()])?;
    }
    let body = current.clone();
    let runtime = rt.clone();
    let running = runtime.spawn(Box::pin(async move {
        while safe_cex_exchange::js_runtime::ops::less_than(
            &body.get("index")?,
            &orders.get("length")?,
        )? {
            let opts = orders.get_key(&body.get("index")?.to_js_string()?)?;
            let ids = await_js(&rt.exchange_runtime(), owner.call_method("placeOrder", vec![opts])?).await?;
            body.get("ids")?
                .call_method("push", ids.iterable_values()?)?;
            body.set("index", Js::Number(body.get("index")?.to_number()? + 1.0))?;
        }
        body.get("ids")
    }));
    current.set("running", running.clone())?;
    let result = await_js(&rt.exchange_runtime(), running).await;
    let result = match result {
        Ok(ids) => pending.call_method("delete", vec![key]).map(|_| ids),
        Err(error) => Err(error),
    };
    current.set("running", Js::Undefined)?;
    result
}
