//! The JS receiver the ported bodies run on: the js_runtime BaseExchange instance
//! extended with BybitExchange's fields (bridged to the typed core) and members.
use super::{
    host::text,
    http::{TypedHttpHost, Wire},
    members,
    promise::await_js,
    rt::{Rt, venue},
    value::{Getter, Js, Setter, Thrown, arg, from_json, from_typed, to_json, to_typed},
};
use crate::{core_snapshot_readers as snapshot, exchange::BybitExchange};
use safe_cex_exchange::{js_runtime::client, socket_lifecycle::SocketSlot};
use std::{cell::Cell, rc::Rc};

fn bool_cell(
    this: &Js,
    rt: &Rt,
    name: &str,
    cell: fn(&BybitExchange) -> &Cell<bool>,
) -> Result<(), Thrown> {
    let (get_rt, set_rt) = (rt.clone(), rt.clone());
    let get: Getter = Rc::new(move |_| Ok(Js::Bool(cell(&*venue(&get_rt)?).get())));
    let set: Setter = Rc::new(move |_, v| {
        cell(&*venue(&set_rt)?).set(v.truthy());
        Ok(())
    });
    this.define_accessor(name, Some(get), Some(set))
}

fn number_cell(
    this: &Js,
    rt: &Rt,
    name: &str,
    cell: fn(&BybitExchange) -> &Cell<f64>,
) -> Result<(), Thrown> {
    let (get_rt, set_rt) = (rt.clone(), rt.clone());
    let get: Getter = Rc::new(move |_| Ok(Js::Number(cell(&*venue(&get_rt)?).get())));
    let set: Setter = Rc::new(move |_, v| {
        cell(&*venue(&set_rt)?).set(v.to_number()?);
        Ok(())
    });
    this.define_accessor(name, Some(get), Some(set))
}

fn method(this: &Js, rt: &Rt, name: &'static str, body: fn(&Rt, Vec<Js>) -> Result<Js, Thrown>) {
    let rt = rt.clone();
    let _ = this.define(name, Js::function(name, move |_, a| body(&rt, a)));
}

fn h(rt: &Rt) -> Rc<Rt> {
    Rc::new(rt.clone())
}

/// A facade over one typed socket slot: the members the exchange bodies call.
fn socket(rt: &Rt, slot: SocketSlot) -> Result<Js, Thrown> {
    let venue = venue(rt)?;
    let Some(typed) = venue.base.get_socket(slot) else {
        return Ok(Js::Undefined);
    };
    let facade = Js::object_with_class("BybitWebsocket", vec![]);
    let connect = typed.clone();
    let _ = facade.define(
        "connectAndSubscribe",
        Js::function("connectAndSubscribe", move |_, _| {
            connect.connect_and_subscribe(false)?;
            Ok(Js::Undefined)
        }),
    );
    let dispose = typed.clone();
    let _ = facade.define(
        "dispose",
        Js::function("dispose", move |_, _| {
            dispose.dispose();
            Ok(Js::Undefined)
        }),
    );
    facade.define_accessor(
        "isConnected",
        Some(Rc::new(move |_| Ok(Js::Bool(typed.is_connected())))),
        None,
    )?;
    Ok(facade)
}

pub(crate) fn install(rt: &Rt) -> Result<(), Thrown> {
    let this = rt.this.clone();
    let typed = venue(rt)?;

    if matches!(
        typed.base.name,
        safe_cex_types::ExchangeName::Bybit
            | safe_cex_types::ExchangeName::Bybitdemo
            | safe_cex_types::ExchangeName::Hyrotrader
    ) {
        method(&this, rt, "fetchClosedPositionRows", |r, a| {
            Ok(crate::ledger_history::fetch_closed_position_rows(
                h(r),
                r.this.clone(),
                arg(&a, 0),
            ))
        });
        method(&this, rt, "fetchHistoricalPositions", |r, a| {
            Ok(crate::ledger_history::fetch_historical_positions(
                h(r),
                r.this.clone(),
                arg(&a, 0),
            ))
        });
    }

    // Constructor fields: the signed clients and the three sockets.
    let runtime = rt.exchange_runtime();
    this.define(
        "xhr",
        client::create(
            Rc::new(TypedHttpHost {
                wire: Wire::Split(typed.xhr().clone()),
                runtime: runtime.clone(),
            }),
            Js::object(vec![]),
        ),
    )?;
    this.define(
        "unlimitedXHR",
        client::create(
            Rc::new(TypedHttpHost {
                wire: Wire::Single(typed.unlimited_xhr().clone()),
                runtime,
            }),
            Js::object(vec![]),
        ),
    )?;
    this.define("publicWebsocket", socket(rt, SocketSlot::Public)?)?;
    this.define("privateWebsocket", socket(rt, SocketSlot::Private)?)?;
    if typed.base.name == safe_cex_types::ExchangeName::Bybitinverse {
        this.define(
            "publicInverseWebsocket",
            socket(rt, SocketSlot::PublicSecondary)?,
        )?;
    }
    // ts: BybitExchange.get inversePublicWebsocket — the secondary public slot.
    this.define(
        "inversePublicWebsocket",
        socket(rt, SocketSlot::PublicSecondary)?,
    )?;

    // Fields owned by the typed core; reads and writes go straight through.
    bool_cell(&this, rt, "needsReinit", |v| &v.needs_reinit)?;
    bool_cell(&this, rt, "forceUnifiedBalance", |v| {
        &v.force_unified_balance
    })?;
    number_cell(&this, rt, "lastReinitAt", |v| &v.last_reinit_at)?;
    number_cell(&this, rt, "reinitRetryMs", |v| &v.reinit_retry_ms)?;
    number_cell(&this, rt, "batchLimit", |v| &v.batch_limit)?;
    {
        let (g, s) = (rt.clone(), rt.clone());
        this.define_accessor(
            "unifiedMarginStatus",
            Some(Rc::new(move |_| {
                Ok(match &*venue(&g)?.unified_margin_status.borrow() {
                    Some(v) => from_json(v),
                    None => Js::Undefined,
                })
            })),
            Some(Rc::new(move |_, v| {
                *venue(&s)?.unified_margin_status.borrow_mut() =
                    (!v.is_undefined()).then(|| to_json(&v)).transpose()?;
                Ok(())
            })),
        )?;
    }
    {
        let (g, s) = (rt.clone(), rt.clone());
        this.define_accessor(
            "lastWebsocketPositionsUpdate",
            Some(Rc::new(move |_| {
                Ok(venue(&g)?
                    .last_websocket_positions_update
                    .get()
                    .map_or(Js::Undefined, Js::Number))
            })),
            Some(Rc::new(move |_, v| {
                let value = if v.is_undefined() {
                    None
                } else {
                    Some(v.to_number()?)
                };
                venue(&s)?.last_websocket_positions_update.set(value);
                Ok(())
            })),
        )?;
    }
    {
        let (g, s) = (rt.clone(), rt.clone());
        this.define_accessor(
            "marginType",
            Some(Rc::new(move |_| from_typed(&venue(&g)?.margin_type.get()))),
            Some(Rc::new(move |_, v| {
                venue(&s)?.margin_type.set(to_typed(&v)?);
                Ok(())
            })),
        )?;
    }
    {
        let (g, s) = (rt.clone(), rt.clone());
        this.define_accessor(
            "positionMode",
            Some(Rc::new(move |_| match venue(&g)?.position_mode.get() {
                Some(mode) => from_typed(&mode),
                None => Ok(Js::Undefined),
            })),
            Some(Rc::new(move |_, v| {
                let mode = if v.is_undefined() {
                    None
                } else {
                    Some(to_typed(&v)?)
                };
                venue(&s)?.position_mode.set(mode);
                Ok(())
            })),
        )?;
    }
    {
        let (g, s) = (rt.clone(), rt.clone());
        this.define_accessor(
            "apiKeyInfo",
            Some(Rc::new(move |_| match &*venue(&g)?.api_key_info.borrow() {
                Some(info) => from_typed(info),
                None => Ok(Js::Undefined),
            })),
            Some(Rc::new(move |_, v| {
                *venue(&s)?.api_key_info.borrow_mut() =
                    (!v.is_nullish()).then(|| to_typed(&v)).transpose()?;
                Ok(())
            })),
        )?;
    }
    {
        let g = rt.clone();
        this.define_accessor(
            "accountType",
            Some(Rc::new(move |_| Ok(Js::str(venue(&g)?.account_type())))),
            None,
        )?;
    }
    {
        let g = rt.clone();
        this.define_accessor(
            "isDemo",
            Some(Rc::new(move |_| {
                Ok(Js::Bool(venue(&g)?.uses_bybit_demo_environment()))
            })),
            None,
        )?;
    }

    // ts: BybitExchange.getSettleCoinsForCategory
    method(&this, rt, "getSettleCoinsForCategory", |rt, a| {
        let coins = venue(rt)?.get_settle_coins_for_category(&text(&arg(&a, 0))?)?;
        Ok(Js::array(coins.into_iter().map(Js::str).collect()))
    });
    method(&this, rt, "validateCandleInterval", |rt, a| {
        venue(rt)?
            .validate_candle_interval(&arg(&a, 0))
            .map_err(Thrown::from)?;
        Ok(Js::Undefined)
    });
    // ts: BybitExchange.getCategoryForSymbol
    method(&this, rt, "getCategoryForSymbol", |rt, a| {
        Ok(Js::str(
            venue(rt)?.get_category_for_symbol(&text(&arg(&a, 0))?),
        ))
    });
    method(&this, rt, "tradesCategoryOf", |rt, a| {
        crate::ws_private::messages::trades_category_of(rt, arg(&a, 0))
    });
    method(&this, rt, "inverseEnabled", |rt, _| {
        Ok(Js::Bool(venue(rt)?.inverse_enabled()))
    });
    method(&this, rt, "optionsEnabled", |rt, _| {
        Ok(Js::Bool(venue(rt)?.options_enabled()))
    });
    // ts: BybitExchange#inferProtectiveOrderSide
    method(
        &this,
        rt,
        "inferProtectiveOrderSide",
        |rt, a| match venue(rt)?
            .infer_protective_order_side(&to_json(&arg(&a, 0))?)
            .map_err(|m| Thrown::error("TypeError", &m))?
        {
            Some(side) => from_typed(&side),
            None => Ok(Js::Undefined),
        },
    );
    // ts: BybitExchange.dedupePositionsById
    method(&this, rt, "dedupePositionsById", |_, a| {
        let positions: Vec<safe_cex_types::Position> = to_typed(&arg(&a, 0))?;
        let deduped = crate::exchange_state::dedupe_positions_by_id(positions)
            .map_err(|m| Thrown::error("Error", m))?;
        from_typed(&deduped)
    });
    // ts: BybitExchange#reinitOnThrow
    method(&this, rt, "reinitOnThrow", |rt, a| {
        let request = arg(&a, 0);
        let rt = rt.clone();
        Ok(rt.clone().spawn(Box::pin(async move {
            match await_js(&rt.exchange_runtime(), request).await {
                Ok(value) => Ok(value),
                Err(error) => {
                    venue(&rt)?.needs_reinit.set(true);
                    Err(error)
                }
            }
        })))
    });
    method(&this, rt, "mapOrder", |rt, a| {
        members::mapping::map_order(rt, &rt.this, &arg(&a, 0))
    });
    method(&this, rt, "mapPosition", |rt, a| {
        members::mapping::map_position(rt, &rt.this, &arg(&a, 0))
    });
    method(&this, rt, "preserveRecentlyPlacedOrders", |rt, a| {
        members::lifecycle::preserve_recently_placed_orders(rt, &arg(&a, 0))
    });
    method(&this, rt, "markOrderSeen", |rt, a| {
        members::lifecycle::mark_order_seen(rt, &text(&arg(&a, 0))?)?;
        Ok(Js::Undefined)
    });

    // Startup (members/startup.rs, primary_startup.rs).
    method(&this, rt, "fetchApiKeyInfo", |rt, _| {
        members::startup::fetch_api_key_info(rt)
    });
    method(&this, rt, "fetchAndCacheApiKeyInfo", |rt, _| {
        members::startup::fetch_and_cache_api_key_info(rt)
    });
    method(&this, rt, "start", |rt, _| Ok(members::startup::start(rt)));

    // Account (members/account.rs).
    method(&this, rt, "getAccount", |rt, _| {
        members::account::get_account(rt)
    });
    method(&this, rt, "fetchMarginAccountInfos", |rt, _| {
        members::account::fetch_margin_account_infos(rt)
    });
    method(&this, rt, "validateAccount", |rt, _| {
        members::account::validate_account(rt)
    });
    method(&this, rt, "fetchBalance", |rt, _| {
        members::account::fetch_balance(rt)
    });
    method(&this, rt, "formatBalance", |rt, a| {
        members::account::format_balance(rt, arg(&a, 0))
    });

    // Position controls (members/controls.rs).
    method(&this, rt, "setLeverage", |rt, a| {
        members::controls::set_leverage(rt, arg(&a, 0), arg(&a, 1))
    });
    method(&this, rt, "changePositionMode", |rt, a| {
        members::controls::change_position_mode(rt, arg(&a, 0), arg(&a, 1))
    });
    method(&this, rt, "changePositionModeHelper", |rt, a| {
        members::controls::change_position_mode_helper(rt, &a)
    });
    method(&this, rt, "changeMarginType", |rt, a| {
        members::controls::change_margin_type(rt, arg(&a, 0), arg(&a, 1))
    });

    this.define(
        "DEFAULT_EXECUTIONS_LOOKBACK_MS",
        Js::Number(typed.default_executions_lookback_ms),
    )?;
    method(
        &this,
        rt,
        "getExecutionQueryWindows",
        members::history::query_windows,
    );
    method(
        &this,
        rt,
        "refuseOptionSymbol",
        members::history::refuse_option_symbol,
    );
    let contexts_rt = rt.clone();
    this.define_accessor(
        "incomeContexts",
        Some(Rc::new(move |_| {
            crate::ledger_history::income_contexts(&contexts_rt, &contexts_rt.this)
        })),
        None,
    )?;
    let context_rt = rt.clone();
    this.define_accessor(
        "incomeContext",
        Some(Rc::new(move |_| {
            crate::inverse_read_surface::income_context(&context_rt, &context_rt.this)
        })),
        None,
    )?;
    method(&this, rt, "fetchIncomeHistory", |rt, a| {
        members::history::fetch_income_history(rt, arg(&a, 0))
    });
    method(&this, rt, "fetchIncomeHistoryRange", |rt, a| {
        members::history::fetch_income_history_range(rt, arg(&a, 0), arg(&a, 1), arg(&a, 2))
    });
    method(&this, rt, "fetchHistoricalFills", |rt, a| {
        if venue(rt)?.base.name == safe_cex_types::ExchangeName::Bybitspot {
            return Ok(Js::array(vec![]));
        }
        crate::rest_fills::fetch_historical_fills(rt, arg(&a, 0))
    });
    method(
        &this,
        rt,
        "fetchExecutionsFallbackFromOrderHistory",
        |rt, a| {
            let inner = rt.clone();
            Ok(rt.spawn(Box::pin(async move {
                crate::rest_fills::fetch_executions_fallback_from_order_history(
                    &inner,
                    arg(&a, 0),
                    arg(&a, 1),
                )
                .await
            })))
        },
    );
    method(&this, rt, "fetchHistoricalOrders", |rt, a| {
        members::history::fetch_historical_orders(rt, arg(&a, 0))
    });
    method(&this, rt, "fetchClosedPnl", |rt, a| {
        members::history::fetch_closed_pnl(rt, arg(&a, 0))
    });
    method(&this, rt, "fetchOHLCV", |rt, a| {
        members::ohlcv::fetch_ohlcv(rt, arg(&a, 0))
    });

    // Public data (members/markets.rs).
    method(&this, rt, "fetchFees", |rt, a| {
        members::markets::fetch_fees(rt, arg(&a, 0))
    });
    method(&this, rt, "fetchTradingMarketsResponse", |rt, a| {
        members::markets::fetch_trading_markets_response(rt, arg(&a, 0))
    });
    method(&this, rt, "fetchMarketsHelper", |rt, a| {
        members::markets::fetch_markets_helper(rt, arg(&a, 0))
    });
    method(&this, rt, "fetchOptionMarkets", |rt, _| {
        members::markets::fetch_option_markets(rt)
    });
    if matches!(
        venue(rt)?.base.name,
        safe_cex_types::ExchangeName::Bybit
            | safe_cex_types::ExchangeName::Bybitdemo
            | safe_cex_types::ExchangeName::Hyrotrader
    ) {
        method(&this, rt, "assertOptionsEnabled", |rt, _| {
            members::options::assert_options_enabled(rt)
        });
        method(&this, rt, "fetchOptionChain", |rt, a| {
            members::options::fetch_option_chain(rt, arg(&a, 0))
        });
        method(&this, rt, "fetchOptionExpiries", |rt, a| {
            members::options::fetch_option_expiries(rt, arg(&a, 0))
        });
        method(&this, rt, "fetchAccountGreeks", |rt, _| {
            members::options::fetch_account_greeks(rt)
        });
    }
    method(&this, rt, "fetchMarkets", |rt, _| {
        members::markets::fetch_markets(rt)
    });
    method(&this, rt, "fetchTickersRawHelper", |rt, a| {
        members::markets::fetch_tickers_raw_helper(rt, arg(&a, 0))
    });
    method(&this, rt, "fetchTickersRaw", |rt, _| {
        members::markets::fetch_tickers_raw(rt)
    });
    method(&this, rt, "fetchTickersWithoutParse", |rt, _| {
        members::markets::fetch_tickers_without_parse(rt)
    });
    method(&this, rt, "fetchTickers", |rt, _| {
        members::markets::fetch_tickers(rt)
    });
    method(&this, rt, "parseTickers", |rt, a| {
        members::markets::parse_tickers(rt, arg(&a, 0))
    });

    // Order placement and the local order lifecycle (members/orders.rs, lifecycle.rs).
    method(&this, rt, "submitCreateOrders", |rt, a| {
        let rt = rt.clone();
        let payloads = arg(&a, 0)
            .array_values()
            .ok_or_else(|| Thrown::type_error("payloads is not an array"))?;
        let opts = arg(&a, 1);
        Ok(rt.clone().spawn(Box::pin(async move {
            members::orders::submit_create_orders_body(&rt, payloads, &opts)
                .await
                .map(Js::array)
        })))
    });
    method(&this, rt, "placeOrder", |rt, a| {
        members::orders::place_order(rt, arg(&a, 0))
    });
    method(&this, rt, "fetchPositionMode", |rt, a| {
        members::orders::fetch_position_mode(rt, arg(&a, 0))
    });
    method(&this, rt, "recordRecentlyPlacedOrder", |rt, a| {
        members::lifecycle::record_recently_placed_order(rt, &arg(&a, 0))?;
        Ok(Js::Undefined)
    });
    method(&this, rt, "recordRecentlyPlacedOrderFromOpts", |rt, a| {
        crate::primary_local_order_writes::record_recently_placed_order_from_opts(
            rt,
            &rt.this,
            arg(&a, 0),
            arg(&a, 1),
            arg(&a, 2),
        )?;
        Ok(Js::Undefined)
    });
    method(
        &this,
        rt,
        "shouldUseLocalRestOrderLifecycleFallback",
        |rt, _| Ok(Js::Bool(members::lifecycle::should_use_local_fallback(rt)?)),
    );
    let recent_rt = rt.clone();
    this.define_accessor(
        "recentlyPlacedOrders",
        Some(Rc::new(move |_| {
            Ok(members::lifecycle::recently_placed_orders(&recent_rt))
        })),
        None,
    )?;
    method(
        &this,
        rt,
        "shouldReconcileLocalReduceOnlyLimitOrder",
        |rt, a| {
            Ok(Js::Bool(members::lifecycle::should_reconcile(
                rt,
                &arg(&a, 0),
            )?))
        },
    );
    method(&this, rt, "getFilledOrderReconciliationTarget", |_, a| {
        Ok(Js::Number(
            members::lifecycle::filled_order_reconciliation_target(&arg(&a, 0))?,
        ))
    });
    method(&this, rt, "reconcileLocalReduceOnlyLimitOrder", |rt, a| {
        Ok(
            crate::local_order_reconciliation::reconcile_local_reduce_only_limit_order(
                h(rt),
                rt.this.clone(),
                arg(&a, 0),
            ),
        )
    });
    method(&this, rt, "resolveOrderSymbolForFills", |rt, a| {
        Ok(
            members::fills::resolve_order_symbol_for_fills(rt, &text(&arg(&a, 0))?)?
                .map_or(Js::Undefined, |s| Js::str(&s)),
        )
    });
    method(&this, rt, "fetchFillsForOrderIds", |rt, a| {
        members::fills::fetch_fills_for_order_ids(rt, arg(&a, 0))
    });
    method(&this, rt, "retireCancelledOrders", |rt, a| {
        members::lifecycle::retire_cancelled_orders(rt, &arg(&a, 0))?;
        Ok(Js::Undefined)
    });
    method(&this, rt, "updateOrder", |rt, a| {
        members::amend::update_order(rt, arg(&a, 0))
    });
    method(&this, rt, "cancelOrders", |rt, a| {
        members::cancel::cancel_orders(rt, arg(&a, 0))
    });
    method(&this, rt, "cancelSymbolOrders", |rt, a| {
        members::cancel::cancel_symbol_orders(rt, arg(&a, 0), arg(&a, 1))
    });
    method(
        &this,
        rt,
        "debouncedPrivateSnapshotRefresh",
        |rt, _| match venue(rt)?.snapshot_refresh.get() {
            Some(refresh) => refresh.call().map_err(Thrown::from),
            None => Ok(Js::Undefined),
        },
    );
    method(
        &this,
        rt,
        "scheduleSetTradingStopSnapshotRefresh",
        |rt, _| {
            crate::private_snapshot_refresh::schedule_set_trading_stop_snapshot_refresh(
                rt,
                rt.this.clone(),
            )
        },
    );

    // Snapshot readers (core_snapshot_readers.rs).
    method(&this, rt, "fetchOrders", |rt, _| {
        Ok(snapshot::fetch_orders(h(rt), rt.this.clone()))
    });
    method(&this, rt, "fetchOrdersHelper", |rt, a| {
        Ok(snapshot::fetch_orders_helper(
            h(rt),
            rt.this.clone(),
            arg(&a, 0),
            arg(&a, 1),
        ))
    });
    method(&this, rt, "fetchPositions", |rt, a| {
        Ok(snapshot::fetch_positions(
            h(rt),
            rt.this.clone(),
            arg(&a, 0),
        ))
    });
    method(&this, rt, "fetchPositionsHelper", |rt, a| {
        Ok(snapshot::fetch_positions_helper(
            h(rt),
            rt.this.clone(),
            arg(&a, 0),
            arg(&a, 1),
            arg(&a, 2),
            arg(&a, 3),
        ))
    });
    method(&this, rt, "fetchOrdersPositionsAndBalance", |rt, _| {
        Ok(snapshot::fetch_orders_positions_and_balance(
            h(rt),
            rt.this.clone(),
        ))
    });
    method(&this, rt, "fetchAndSetPrivateSnapshot", |rt, _| {
        Ok(snapshot::fetch_and_set_private_snapshot(
            h(rt),
            rt.this.clone(),
        ))
    });
    method(&this, rt, "fetchAndSetPrivateSnapshotHelper", |rt, a| {
        Ok(snapshot::fetch_and_set_private_snapshot_helper(
            h(rt),
            rt.this.clone(),
            arg(&a, 0).is_undefined() || arg(&a, 0).truthy(),
        ))
    });
    method(&this, rt, "waitForRemoteSigner", |rt, _| {
        let rt = rt.clone();
        Ok(rt.clone().spawn(Box::pin(async move {
            let venue = venue(&rt)?;
            Ok(Js::Bool(venue.base.wait_for_remote_signer().await?))
        })))
    });
    method(&this, rt, "tick", |rt, _| {
        Ok(snapshot::tick(h(rt), rt.this.clone()))
    });
    method(&this, rt, "fetchRawBalance", |rt, _| {
        Ok(snapshot::fetch_raw_balance(h(rt), rt.this.clone()))
    });
    method(&this, rt, "syncBalanceUpnlFromPositions", |rt, _| {
        snapshot::sync_balance_upnl_from_positions(rt, &rt.this)?;
        Ok(Js::Undefined)
    });
    members::batch::install(rt)?;
    if venue(rt)?.base.name == safe_cex_types::ExchangeName::Bybitspot {
        members::spot::install(rt)?;
    }
    if venue(rt)?.base.name == safe_cex_types::ExchangeName::Bybitinverse {
        members::inverse::install(rt)?;
    }
    if venue(rt)?.base.name == safe_cex_types::ExchangeName::Hyrotrader {
        members::hyro::install(rt)?;
    }
    Ok(())
}
