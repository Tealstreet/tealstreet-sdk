//! KucoinExchange (futures) class assembly: the class_construction member
//! roots bound to the ported value-generic bodies over `Js`.
use super::{
    globals::regexp,
    host::{W, capture, frame, member, shared, text},
    ops::{bootstrap, construction, control, lifecycle, private_class, super_member},
};
use crate::{
    bootstrap_reads, class_construction::ClassOps, futures_native_accounts as accounts,
    futures_native_formatters as formatters, futures_native_helpers as helpers,
    futures_native_mapping as mapping, futures_native_order_reads as reads, order_control,
    order_lifecycle, position_metadata, rest_reads, sizing, socket_boundaries,
};
use safe_cex_exchange::js_runtime::{
    promise::{JsFuture, Promise, await_js},
    scope::scope_of,
    value::{Js, Thrown, arg, from_typed},
};
use std::rc::Rc;

fn asynchronous(weak: &W, name: &'static str, body: JsFuture<'static>) -> Result<Js, Thrown> {
    let _ = name;
    Ok(Js::Promise(Promise::spawn(
        &scope_of(weak)?.exchange_runtime(),
        body,
    )))
}

const ASYNC: &[&str] = &[
    "getAccount",
    "validateAccount",
    "start",
    "tick",
    "fetchBalance",
    "fetchPositionMetadata",
    "fetchPositions",
    "fetchOpenPositions",
    "fetchMarkets",
    "fetchTickers",
    "fetchOrders",
    "fetchNormalOrders",
    "fetchAlgoOrders",
    "fetchOHLCV",
    "cancelAllOrders",
    "cancelOrders",
    "cancelNormalOrders",
    "cancelSymbolOrders",
    "placeOrder",
    "formatCreateOrder",
    "placeOrderBatch",
    "placeAlgoOrders",
    "updateVirtualTpSlOrder",
    "updateOrder",
    "updateAlgoOrder",
    "setLeverage",
    "changeMarginType",
    "changePositionMode",
];

fn dispose(weak: &W, receiver: &Js) -> Result<Js, Thrown> {
    let home = shared(weak)?.globals.get("KucoinExchange")?;
    socket_boundaries::dispose_futures(
        |path| {
            match path {
                "super.dispose" => {
                    super_member(weak, &home, receiver, "dispose")?.call(Js::Undefined, vec![])?;
                }
                other => {
                    let (owner, method) = other.split_once('.').unwrap_or((other, ""));
                    receiver.get(owner)?.call_method(method, vec![])?;
                }
            }
            Ok(())
        },
        |each| {
            let callback = Js::function("", move |_, a| each(arg(&a, 0)));
            receiver
                .get("snapshotWebsockets")?
                .call_method("forEach", vec![callback])?;
            Ok(())
        },
        Rc::new(|socket: Js| socket.call_method("dispose", vec![])),
        |empty| receiver.set("snapshotWebsockets", Js::array(empty)),
    )?;
    Ok(Js::Undefined)
}

async fn fetch_balance(weak: W, receiver: Js) -> Result<Js, Thrown> {
    let h = frame(&weak, &receiver);
    let get = |v: &Js, k: &str| v.get(k);
    rest_reads::fetch_balance(
        |key| {
            let endpoint = capture(&weak, &receiver, &format!("ENDPOINTS.{key}"))?;
            let fetch = (h.capture)("this.xhr.get")?;
            Ok(Js::function("", move |_, a| {
                fetch.call(Js::Undefined, vec![endpoint.clone(), arg(&a, 0)])
            }))
        },
        || (h.capture)("this.apiMarginCoin"),
        |request: Js, params| -> Result<JsFuture<'static>, Thrown> {
            Ok(Box::pin(await_js(
                &scope_of(&weak)?.exchange_runtime(),
                request.call(Js::Undefined, vec![params])?,
            )))
        },
        get,
        |v, s| matches!(v, Js::String(_)) && v.strict_equals(&Js::str(s)),
        Js::truthy,
        || (h.capture)("this.emitAndLogError"),
        |emit, message| emit.call(Js::Undefined, vec![message]).map(|_| ()),
        || (h.capture)("this.store.balance"),
        |row, key, default| {
            let value = row.get(key)?;
            let value = if value.truthy() {
                value
            } else {
                Js::str(default)
            };
            Ok(Js::Number(value.parse_float()?))
        },
        Js::str,
        Js::object,
    )
    .await
}

/// One own arrow of KucoinExchange, running the Rust body its root names.
fn native_member(weak: W, receiver: Js, key: &'static str) -> Result<Js, Thrown> {
    let is_async = ASYNC.contains(&key);
    Ok(Js::function(key, move |_, a| {
        let weak = weak.clone();
        let r = receiver.clone();
        let x = |i| arg(&a, i);
        let h = frame(&weak, &r);
        let ops = private_class(&weak);
        if is_async {
            let (a0, a1) = (x(0), x(1));
            let body: JsFuture<'static> = match key {
                "getAccount" => Box::pin(accounts::get_account(h)),
                "validateAccount" => Box::pin(accounts::validate_account(h)),
                "start" => Box::pin(bootstrap_reads::start(bootstrap(&weak, &r))),
                "tick" => Box::pin(bootstrap_reads::tick(bootstrap(&weak, &r), a0)),
                "fetchBalance" => Box::pin(fetch_balance(weak.clone(), r.clone())),
                "fetchPositionMetadata" => Box::pin(bootstrap_reads::fetch_position_metadata(
                    bootstrap(&weak, &r),
                    a0,
                )),
                "fetchPositions" => {
                    Box::pin(bootstrap_reads::fetch_positions(bootstrap(&weak, &r), a0))
                }
                "fetchOpenPositions" => {
                    let b = bootstrap(&weak, &r);
                    Box::pin(async move { bootstrap_reads::fetch_open_positions(&b).await })
                }
                "fetchMarkets" => Box::pin(bootstrap_reads::fetch_markets(bootstrap(&weak, &r))),
                "fetchTickers" => {
                    let b = bootstrap(&weak, &r);
                    Box::pin(async move { bootstrap_reads::fetch_tickers(&b).await })
                }
                "fetchOrders" => Box::pin(reads::fetch_orders(ops, h)),
                "fetchNormalOrders" => Box::pin(reads::fetch_normal_orders(h)),
                "fetchAlgoOrders" => Box::pin(reads::fetch_algo_orders(h)),
                "fetchOHLCV" => Box::pin(bootstrap_reads::fetch_ohlcv(bootstrap(&weak, &r), a0)),
                "cancelAllOrders" => {
                    let c = control(h);
                    Box::pin(async move { order_control::cancel_all_orders(&c).await })
                }
                "cancelOrders" => {
                    let c = control(h);
                    Box::pin(async move { order_control::cancel_orders(&c, a0).await })
                }
                "cancelNormalOrders" => {
                    Box::pin(order_control::cancel_normal_orders(control(h), a0))
                }
                "cancelSymbolOrders" => {
                    let c = control(h);
                    Box::pin(async move { order_control::cancel_symbol_orders(&c, a0).await })
                }
                "placeOrder" => {
                    let l = lifecycle(h);
                    Box::pin(async move { order_lifecycle::place_order(&l, a0).await })
                }
                "formatCreateOrder" => {
                    let result = formatters::format_create_order(&ops, &control(h), a0);
                    Box::pin(async move { result })
                }
                "placeOrderBatch" => {
                    let l = lifecycle(h);
                    Box::pin(async move { order_lifecycle::place_order_batch(&l, a0, a1).await })
                }
                "placeAlgoOrders" => {
                    let l = lifecycle(h);
                    Box::pin(async move { order_lifecycle::place_algo_orders(&l, a0).await })
                }
                "updateVirtualTpSlOrder" => Box::pin(order_lifecycle::update_virtual_tp_sl_order(
                    lifecycle(h),
                    a0,
                    a1,
                )),
                "updateOrder" => Box::pin(order_control::update_order(control(h), a0)),
                "updateAlgoOrder" => {
                    let c = control(h);
                    Box::pin(async move { order_control::update_algo_order(&c, a0).await })
                }
                "setLeverage" => {
                    let c = control(h);
                    Box::pin(async move { order_control::set_leverage(&c, a0, a1).await })
                }
                "changeMarginType" => {
                    let c = control(h);
                    Box::pin(async move { order_control::change_margin_type(&c, a0, a1).await })
                }
                "changePositionMode" => {
                    Box::pin(order_control::change_position_mode(control(h), a0))
                }
                other => {
                    return Err(Thrown::error(
                        "Error",
                        &format!("unknown asynchronous KucoinExchange field {other}"),
                    ));
                }
            };
            return asynchronous(&weak, key, body);
        }
        match key {
            "toKucoinSymbol" => Ok(helpers::to_kucoin_symbol(&h, x(0))),
            "toInternalSymbol" => helpers::to_internal_symbol(&h, x(0)),
            "toApiSymbol" => helpers::to_api_symbol(&h, x(0)),
            "contractSizeFor" => helpers::contract_size_for(&h, x(0)),
            "nativeAmountForCancelReplace" => helpers::native_amount_for_cancel_replace(&h, x(0)),
            "parseMarginType" => helpers::parse_margin_type(&ops, &h, x(0), x(1)),
            "parsePositionMode" => helpers::parse_position_mode(&h, x(0)),
            "parseRowPositionMode" => helpers::parse_row_position_mode(&h, x(0)),
            "dispose" => dispose(&weak, &r),
            "reconcileFetchedOrderRemainders" => {
                reads::reconcile_fetched_order_remainders(ops, &construction(), h, x(0))
            }
            "listenOHLCV" => socket_boundaries::listen_ohlcv(
                x(0),
                x(1),
                |owner, name| member(&r.get(owner)?, name),
                |f: Js, opts, callback| f.call(Js::Undefined, vec![opts, callback]),
            ),
            "listenOrderBook" => socket_boundaries::listen_order_book(
                x(0),
                x(1),
                |owner, name| member(&r.get(owner)?, name),
                |f: Js, symbol, callback| f.call(Js::Undefined, vec![symbol, callback]),
            ),
            "formatAlgoOrder" => formatters::format_algo_order(&ops, &control(h), x(0)),
            "getOrderPlanType" => helpers::get_order_plan_type(&ops, &control(h), x(0)),
            "mapOrder" => mapping::map_order(&ops, &h, x(0)),
            "getOrderSideV2" => helpers::get_order_side_v2(&h, x(0), x(1)),
            "getOrderSide" => helpers::get_order_side(&ops, &control(h), x(0), x(1)),
            "getAlgoOrderSide" => helpers::get_algo_order_side(&ops, &control(h), x(0)),
            "getPositionForOrder" => helpers::get_position_for_order(&ops, h, x(0)),
            "getKucoinMarginMode" => helpers::get_kucoin_margin_mode(&h, x(0)),
            "getKucoinPositionSide" => helpers::get_kucoin_position_side(&h, x(0), x(1)),
            "mapStopPriceType" => helpers::map_stop_price_type(&h, x(0)),
            "getKucoinStopDirection" => helpers::get_kucoin_stop_direction(&ops, &control(h), x(0)),
            "isVirtualTPSLOrder" => helpers::is_virtual_tpsl_order(&h, x(0)),
            "isAlgoOrder" => helpers::is_algo_order(&ops, &h, x(0)),
            "getOrderPlanTypeAndAmount" => {
                helpers::get_order_plan_type_and_amount(&ops, control(h), x(0))
            }
            other => Err(Thrown::error(
                "Error",
                &format!("unknown KucoinExchange field {other}"),
            )),
        }
    }))
}

/// apiMarginCoin getter and the getOrFetch* prototype methods.
fn prototype_member(weak: &W, class: &Js, key: &str) -> Result<(), Thrown> {
    let proto = class.get("prototype")?;
    match key {
        "apiMarginCoin" => {
            let weak = weak.clone();
            proto.define_accessor(
                "apiMarginCoin",
                Some(Rc::new(move |_| {
                    let base =
                        shared(&weak)?.base.get().cloned().ok_or_else(|| {
                            Thrown::error("Error", "BaseExchange is not constructed")
                        })?;
                    Ok(Js::str(sizing::api_margin_coin(
                        base.options().testnet == Some(true),
                    )))
                })),
                None,
            )?;
            Ok(())
        }
        "getOrFetchPositionMode" | "getOrFetchMarginType" => {
            let weak = weak.clone();
            let mode = key == "getOrFetchPositionMode";
            proto.define(
                key,
                Js::function(key, move |_, a| {
                    let symbol = text(&arg(&a, 0))?;
                    let venue = shared(&weak)?
                        .sockets
                        .venue
                        .borrow()
                        .as_ref()
                        .and_then(std::rc::Weak::upgrade)
                        .ok_or_else(|| {
                            Thrown::error("Error", "KuCoin exchange is not assembled")
                        })?;
                    asynchronous(
                        &weak,
                        "",
                        Box::pin(async move {
                            if mode {
                                from_typed(
                                    &position_metadata::get_or_fetch_position_mode(venue, &symbol)
                                        .await?,
                                )
                            } else {
                                from_typed(
                                    &position_metadata::get_or_fetch_margin_type(venue, &symbol)
                                        .await?,
                                )
                            }
                        }),
                    )
                }),
            )
        }
        other => Err(Thrown::error(
            "Error",
            &format!("unknown KucoinExchange prototype member {other}"),
        )),
    }
}

pub(crate) fn define(weak: &W) -> Result<Js, Thrown> {
    let construction = construction();
    let members = weak.clone();
    let prototypes = weak.clone();
    let ops = Rc::new(ClassOps {
        value: frame(weak, &Js::Undefined),
        null: Rc::new(|| Js::Null),
        boolean: Rc::new(Js::Bool),
        regexp: Rc::new(regexp),
        define: Rc::new(|r, k, v| r.define(k, v)),
        member: Rc::new(member),
        superclass: construction.superclass.clone(),
        super_construct: construction.super_construct.clone(),
        construct: construction.construct.clone(),
        function: Rc::new(|name, _, body| Js::function(name, move |_, a| body(a))),
        native_member: Rc::new(move |receiver, _, key, _| {
            native_member(members.clone(), receiver.clone(), key)
        }),
        prototype_member: Rc::new(move |class, _, key, _, _, _| {
            prototype_member(&prototypes, class, key)
        }),
        derive: construction.derive.clone(),
    });
    crate::class_construction::define_futures_exchange(ops)
}
