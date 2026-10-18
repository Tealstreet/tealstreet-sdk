use safe_cex_types::{DisplayedMargin, ModelField, PositionSide, StoreData};
use std::collections::HashMap;

// ts: DisplayedMarginPostprocessor
pub struct DisplayedMarginPostprocessor;

impl DisplayedMarginPostprocessor {
    pub fn process(&self, store: &mut StoreData) {
        let mut margin = DisplayedMargin {
            long_exposure: 0.0,
            short_exposure: 0.0,
            total_notional: 0.0,
        };
        for displayed in &store.displayed_positions {
            let position = &displayed.position;
            let notional = if position.notional.is_nan() {
                0.0
            } else {
                position.notional.abs()
            };
            margin.total_notional += notional;
            match position.side {
                ModelField::Value(PositionSide::Long) => margin.long_exposure += notional,
                ModelField::Value(PositionSide::Short) => margin.short_exposure += notional,
                ModelField::Absent | ModelField::Undefined => {}
            }
        }
        store.displayed_margin = Some(margin);
    }
}

// ts: ChaserOrderEnrichmentPostprocessor
pub struct ChaserOrderEnrichmentPostprocessor;

impl ChaserOrderEnrichmentPostprocessor {
    pub fn process(&self, store: &mut StoreData) {
        let mut owners = HashMap::new();
        let mut prototype: Option<&safe_cex_types::ActiveChaser> = None;
        for chaser in &store.active_chasers {
            if let Some(ids) = &chaser.current_order_ids {
                for id in ids {
                    // ts-bug: assignment to __proto__ changes the lookup object's
                    // prototype instead of creating an own order-id property.
                    if id == "__proto__" {
                        prototype = Some(chaser);
                    } else {
                        owners.insert(id.as_str(), chaser);
                    }
                }
            }
        }
        for order in &mut store.orders {
            let key = order.id.as_str();
            let owner = owners.get(key).copied();
            let prototype_owner = if key == "__proto__" { prototype } else { None };
            let owner = owner.or(prototype_owner);
            // A truthy inherited scalar, array or Object.prototype function also
            // enters the source branch, but its chaser fields are undefined.
            let inherited_truthy = prototype
                .map(|chaser| match key {
                    "id" => !chaser.id.is_empty(),
                    "symbol" => !chaser.symbol.is_empty(),
                    "size" => chaser.size != 0.0 && !chaser.size.is_nan(),
                    "side" => true,
                    "maxAttempts" => chaser.max_attempts != 0.0 && !chaser.max_attempts.is_nan(),
                    "attempts" => chaser.attempts != 0.0 && !chaser.attempts.is_nan(),
                    "isStopped" => chaser.is_stopped,
                    "stopReason" => !chaser.stop_reason.is_empty(),
                    "currentOrderIds" => chaser.current_order_ids.is_some(),
                    "reduceOnly" => chaser.reduce_only == Some(true),
                    "origin" => chaser.origin.is_some(),
                    _ => false,
                })
                .unwrap_or(false)
                || safe_cex_num::js_table_lookup::<&str, ()>(&[], key).is_present();
            if owner.is_some() || inherited_truthy {
                let extra = order.extra.get_or_insert_with(|| {
                    serde_json::from_str("{}").expect("OrderExtra consists of optional fields")
                });
                extra.is_from_chaser = Some(true);
                extra.chaser_id = owner.map(|owner| owner.id.clone());
                extra.chaser_attempts = owner.map(|owner| owner.attempts);
                extra.chaser_max_attempts = owner.map(|owner| owner.max_attempts);
            }
        }
    }
}

// ts: ChaserOrderPostprocessor. The worker supplies its current manager view.
pub struct ChaserOrderPostprocessor;

impl ChaserOrderPostprocessor {
    pub fn process(
        &self,
        store: &mut StoreData,
        exchange: &std::rc::Rc<dyn safe_cex_exchange::exchange::Exchange>,
        instances: &[std::rc::Rc<crate::chaser::ChaserOrderInstance>],
    ) {
        let mut active_chasers = Vec::new();
        for instance in instances {
            if !std::rc::Rc::ptr_eq(&instance.exchange, exchange) {
                continue;
            }
            let state = instance.state();
            let state = state.borrow();
            if state.is_stopped {
                continue;
            }
            let options = instance.options();
            let options = options.borrow();
            active_chasers.push(safe_cex_types::ActiveChaser {
                id: instance.id().to_owned(),
                symbol: options.symbol.clone(),
                size: options.size,
                side: options.side,
                max_attempts: options.max_attempts,
                reduce_only: options.reduce_only,
                attempts: state.attempts,
                is_stopped: state.is_stopped,
                stop_reason: state.stop_reason.clone(),
                current_order_ids: Some(state.current_orders_ids.clone()),
                origin: options.origin,
            });
        }
        store.active_chasers = active_chasers;
    }
}

// ts: TwapPostprocessor. The worker supplies the current manager's ordered fields.
pub struct TwapPostprocessor;

impl TwapPostprocessor {
    pub fn process(
        &self,
        store: &mut StoreData,
        account_name: &str,
        instances: &[(
            &str,
            &crate::task_types::TwapOptions,
            &crate::task_types::TwapState,
        )],
    ) {
        use crate::task_types::TwapStatus;
        let mut active_twaps = Vec::new();
        for (id, options, state) in instances {
            if options.account_name != account_name || state.status == TwapStatus::Completed {
                continue;
            }
            active_twaps.push(safe_cex_types::ActiveTWAP {
                id: (*id).to_owned(),
                account_name: state.account_name.clone(),
                active_market: state.active_market.clone(),
                size: if options.side == safe_cex_types::OrderSide::Sell {
                    -options.size
                } else {
                    options.size
                },
                symbol: if options.active_market.symbol.is_empty() {
                    "Unknown".into()
                } else {
                    options.active_market.symbol.clone()
                },
                duration: options.duration,
                orders_count: state.orders_count,
                side: state.side,
                chaser_delay: options.chaser_delay,
                reduce_only: options.reduce_only,
                randomness_percentage: options.randomness_percentage,
                use_limit_orders: options.use_limit_orders,
                use_chaser_orders: options.use_chaser_orders,
                timeout_pct: options.timeout_pct,
                timeout_action: options.timeout_action,
                fail_action: options.fail_action,
                origin: state.origin.or(options.origin),
                orders_sizes: state.orders_sizes.clone(),
                status: state.status,
                orders_executed: state.orders_executed,
                next_order_at: state.next_order_at,
                mark_for_cleanup: state.mark_for_cleanup,
                created_at: state.created_at,
                started_at: state.started_at,
            });
        }
        store.active_twaps = active_twaps;
    }
}

// ts: OrdersRiskProfilePostprocessor. Owned StoreData snapshots rebuild symbol maps.
pub struct OrdersRiskProfilePostprocessor;
impl OrdersRiskProfilePostprocessor {
    pub fn process<E>(
        &self,
        store: &mut StoreData,
        report: impl FnOnce(&safe_cex_store::risk::RiskError) -> Result<(), E>,
    ) -> Result<(), E> {
        match safe_cex_store::risk::calculate_orders_risk(
            &store.markets,
            &store.tickers,
            &store.positions,
            &store.orders,
            None,
        ) {
            Ok(risk) => store.orders_risk = risk.into_iter().collect(),
            Err(error) => {
                // Source console.trace/error happen before the explicit replacement.
                // A host reporting error propagates, like a throwing console hook.
                report(&error)?;
                store.orders_risk = Default::default();
            }
        }
        Ok(())
    }
}
