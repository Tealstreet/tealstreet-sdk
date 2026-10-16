//! OKXExchange class members installed on the receiver, each running its
//! ported Rust body over the scope's closure tables.
use super::ops::{
    OkxScope, composition_ops, lifecycle_ops, market_ops, mutation_ops, read_ops, runtime_ops,
};
use crate::{
    family_lifecycle, history_stream_api, market_account_reads, order_composition, order_mutations,
    order_reads,
};
use safe_cex_exchange::js_runtime::{
    promise::JsFuture,
    value::{Js, Thrown, arg},
};
use std::rc::Rc;

/// Which concrete class the receiver is; EU classes override some members.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Variant {
    Global,
    Demo,
    Eu,
    EuDemo,
}

impl Variant {
    pub(crate) fn is_eu(self) -> bool {
        matches!(self, Variant::Eu | Variant::EuDemo)
    }
}

/// Install `name` on `scope.this` unless an own field already holds it.
pub(crate) fn define(scope: &OkxScope, name: &'static str, value: Js) {
    if !scope.this.has_own(name) {
        let _ = scope.this.define(name, value);
    }
}

/// A synchronous member whose body runs with `this` bound to the receiver.
pub(crate) fn sync_member(
    scope: &OkxScope,
    name: &'static str,
    body: impl Fn(&OkxScope, Vec<Js>) -> Result<Js, Thrown> + 'static,
) {
    let s = scope.clone();
    define(scope, name, Js::function(name, move |_, a| body(&s, a)));
}

/// An async member: the body starts eagerly and the runtime owns the rest.
pub(crate) fn async_member(
    scope: &OkxScope,
    name: &'static str,
    body: impl Fn(OkxScope, Vec<Js>) -> JsFuture<'static> + 'static,
) {
    let s = scope.clone();
    define(
        scope,
        name,
        Js::function(name, move |_, a| Ok(s.spawn(body(s.clone(), a)))),
    );
}

/// TS class-field initializers of OKXExchange (run inside the constructor).
pub(crate) fn initialize_fields(scope: &OkxScope) -> Result<(), Thrown> {
    let this = &scope.this;
    this.define(
        "grafanaEndpointsEvery",
        scope.global("GRAFANA_ENDPOINTS_EVERY")?,
    )?;
    this.define(
        "grafanaEndpointsPeriodic",
        scope.global("GRAFANA_ENDPOINTS_PERIODIC")?,
    )?;
    this.define("_accountPositionMode", Js::Null)?;
    this.define("_accountLevel", Js::Null)?;
    this.define("markPriceMarketIds", Js::new_set())?;
    this.define("indexPriceMarketIds", Js::new_set())?;
    Ok(())
}

fn market_members(scope: &OkxScope) {
    async_member(scope, "fetchMarkets", |s, _| {
        Box::pin(async move { market_account_reads::fetch_markets(market_ops(&s)).await })
    });
    async_member(scope, "fetchTickersRaw", |s, _| {
        Box::pin(async move { market_account_reads::fetch_tickers_raw(market_ops(&s)).await })
    });
    sync_member(scope, "parseTickers", |s, a| {
        market_account_reads::parse_tickers(market_ops(s), arg(&a, 0))
    });
    async_member(scope, "fetchPositionMetadata", |s, a| {
        Box::pin(async move {
            market_account_reads::fetch_position_metadata(market_ops(&s), arg(&a, 0)).await
        })
    });
    async_member(scope, "fetchBalanceAndPositions", |s, a| {
        Box::pin(async move {
            market_account_reads::fetch_balance_and_positions(market_ops(&s), arg(&a, 0)).await
        })
    });
    async_member(scope, "fetchBalanceAndOpenPositions", |s, _| {
        Box::pin(async move {
            market_account_reads::fetch_balance_and_open_positions(market_ops(&s)).await
        })
    });
    async_member(scope, "formatBalanceUsedAvail", |s, a| {
        Box::pin(async move {
            market_account_reads::format_balance_used_avail(
                market_ops(&s),
                arg(&a, 0),
                arg(&a, 1),
                arg(&a, 2),
            )
            .await
        })
    });
    async_member(scope, "fetchLeverages", |s, a| {
        Box::pin(
            async move { market_account_reads::fetch_leverages(market_ops(&s), arg(&a, 0)).await },
        )
    });
}

fn lifecycle_members(scope: &OkxScope, variant: Variant) {
    async_member(scope, "start", |s, _| {
        Box::pin(async move { family_lifecycle::start(lifecycle_ops(&s)).await })
    });
    async_member(scope, "tick", |s, _| {
        Box::pin(async move { family_lifecycle::tick(lifecycle_ops(&s)).await })
    });
    if variant.is_eu() {
        async_member(scope, "fetchAccountMarketOverlay", |s, _| {
            Box::pin(async move {
                family_lifecycle::fetch_eu_account_market_overlay(lifecycle_ops(&s)).await
            })
        });
    }
}

fn history_members(scope: &OkxScope) {
    async_member(scope, "fetchIncomeHistory", |s, a| {
        Box::pin(async move {
            history_stream_api::fetch_income_history(runtime_ops(&s), arg(&a, 0)).await
        })
    });
    async_member(scope, "fetchOHLCV", |s, a| {
        Box::pin(async move { history_stream_api::fetch_ohlcv(runtime_ops(&s), arg(&a, 0)).await })
    });
}

fn composition_members(scope: &OkxScope) {
    async_member(scope, "placeOrder", |s, a| {
        Box::pin(
            async move { order_composition::place_order(composition_ops(&s), arg(&a, 0)).await },
        )
    });
    async_member(scope, "placeOrders", |s, a| {
        Box::pin(
            async move { order_composition::place_orders(composition_ops(&s), arg(&a, 0)).await },
        )
    });
    sync_member(scope, "getReduceOnlyTriggerOrderType", |s, a| {
        order_composition::get_reduce_only_trigger_order_type(
            &composition_ops(s),
            arg(&a, 0),
            arg(&a, 1),
        )
    });
    sync_member(scope, "formatAlgoOrder", |s, a| {
        order_composition::format_algo_order(&composition_ops(s), arg(&a, 0))
    });
    sync_member(scope, "formatCreateOrder", |s, a| {
        order_composition::format_create_order(&composition_ops(s), arg(&a, 0))
    });
    sync_member(scope, "mapOrders", |s, a| {
        order_composition::map_orders(composition_ops(s), arg(&a, 0))
    });
    sync_member(scope, "mapPositions", |s, a| {
        order_composition::map_positions(composition_ops(s), arg(&a, 0))
    });
    async_member(scope, "fetchHistoricalFills", |s, a| {
        Box::pin(async move {
            order_composition::fetch_historical_fills(composition_ops(&s), arg(&a, 0)).await
        })
    });
}

fn mutation_members(scope: &OkxScope) {
    type Body = fn(
        Rc<order_mutations::OrderMutationOps<Js, Thrown>>,
        Js,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Js, Thrown>>>>;
    let one: [(&'static str, Body); 7] = [
        ("updateOrder", |h, x| {
            Box::pin(order_mutations::update_order(h, x))
        }),
        ("updateAlgoOrder", |h, x| {
            Box::pin(order_mutations::update_algo_order(h, x))
        }),
        ("updateTriggerAlgoOrder", |h, x| {
            Box::pin(order_mutations::update_trigger_algo_order(h, x))
        }),
        ("cancelOrders", |h, x| {
            Box::pin(order_mutations::cancel_orders(h, x))
        }),
        ("cancelSymbolOrders", |h, x| {
            Box::pin(order_mutations::cancel_symbol_orders(h, x))
        }),
        ("cancelNormalOrders", |h, x| {
            Box::pin(order_mutations::cancel_normal_orders(h, x))
        }),
        ("cancelAlgoOrders", |h, x| {
            Box::pin(order_mutations::cancel_algo_orders(h, x))
        }),
    ];
    for (name, body) in one {
        async_member(scope, name, move |s, a| body(mutation_ops(&s), arg(&a, 0)));
    }
    async_member(scope, "setLeverage", |s, a| {
        Box::pin(async move {
            order_mutations::set_leverage(mutation_ops(&s), arg(&a, 0), arg(&a, 1)).await
        })
    });
    async_member(scope, "changePositionMode", |s, a| {
        Box::pin(async move {
            order_mutations::change_position_mode(mutation_ops(&s), arg(&a, 0)).await
        })
    });
}

fn read_members(scope: &OkxScope) {
    async_member(scope, "fetchOrders", |s, _| {
        Box::pin(async move { order_reads::fetch_orders(Rc::new(read_ops(&s))).await })
    });
    async_member(scope, "fetchNormalOrders", |s, _| {
        Box::pin(async move { order_reads::fetch_normal_orders(Rc::new(read_ops(&s))).await })
    });
    async_member(scope, "fetchAlgoOrders", |s, a| {
        Box::pin(
            async move { order_reads::fetch_algo_orders(Rc::new(read_ops(&s)), arg(&a, 0)).await },
        )
    });
    type Body = fn(
        Rc<order_reads::OrderReadOps<'static, Js, Thrown>>,
        Js,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Js, Thrown>>>>;
    let one: [(&'static str, Body); 4] = [
        ("fetchHistoricalOrders", |h, x| {
            Box::pin(order_reads::fetch_historical_orders(h, x))
        }),
        ("fetchClosedPositionRows", |h, x| {
            Box::pin(order_reads::fetch_closed_position_rows(h, x))
        }),
        ("fetchHistoricalPositions", |h, x| {
            Box::pin(order_reads::fetch_historical_positions(h, x))
        }),
        ("fetchClosedPnl", |h, x| {
            Box::pin(order_reads::fetch_closed_pnl(h, x))
        }),
    ];
    for (name, body) in one {
        async_member(scope, name, move |s, a| {
            body(Rc::new(read_ops(&s)), arg(&a, 0))
        });
    }
    async_member(scope, "fetchIncomeHistoryRange", |s, a| {
        Box::pin(async move {
            order_reads::fetch_income_history_range(
                Rc::new(read_ops(&s)),
                arg(&a, 0),
                arg(&a, 1),
                arg(&a, 2),
            )
            .await
        })
    });
}

/// Every OKXExchange member backed by a closure-table body.
pub(crate) fn install(scope: &OkxScope, variant: Variant) {
    market_members(scope);
    lifecycle_members(scope, variant);
    history_members(scope);
    composition_members(scope);
    mutation_members(scope);
    read_members(scope);
}
