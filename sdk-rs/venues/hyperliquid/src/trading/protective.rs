//! Protective edits retain native trigger parameters under the existing amend owner.
use super::*;
use crate::wire::OrderRow;
use safe_cex_engine::Store;
use safe_cex_engine::error::Protection;

pub(super) struct Amend {
    before: Order,
    original: OrderWire,
    target_kind: OrderType,
    target: f64,
    peers: Vec<String>,
}
impl Hyperliquid {
    pub(crate) fn capture_protective_source(
        &mut self,
        cx: &Cx<'_, Pending>,
        id: &str,
        row: &OrderRow<'_>,
    ) -> Result<(), String> {
        let old = cx
            .store
            .order(id)
            .ok_or("Hyperliquid amendment source is no longer open")?;
        if row.oid.to_string() != id
            || self.coin(cx, &old.symbol) != Some(row.coin.as_ref())
            || !matches!(row.side.as_ref(), "A" | "B")
            || (row.side == "B") != (old.side == OrderSide::Buy)
        {
            return Err("Hyperliquid sizing response does not identify the amended order".into());
        }
        if !matches!(old.kind, OrderType::StopLoss | OrderType::TakeProfit) {
            return Ok(());
        }
        let (kind, market) =
            match row.order_type.as_ref() {
                "Stop Market" => (OrderType::StopLoss, true),
                "Stop Limit" => (OrderType::StopLoss, false),
                "Take Profit Market" => (OrderType::TakeProfit, true),
                "Take Profit Limit" => (OrderType::TakeProfit, false),
                _ => return Err(
                    "Hyperliquid protective order execution type is not proven; order unchanged"
                        .into(),
                ),
            };
        let trigger = row
            .trigger_px
            .filter(|v| v.is_finite() && *v > 0.)
            .ok_or("Hyperliquid original trigger is unavailable; order unchanged")?;
        if kind != old.kind
            || trigger != old.price
            || row.reduce_only != old.reduce_only
            || old
                .client_order_id
                .as_deref()
                .is_some_and(|c| row.cloid.as_deref() != Some(c))
            || !row.sz.is_finite()
            || row.sz < 0.
            || !row.orig_sz.is_finite()
            || row.orig_sz < row.sz
        {
            return Err(
                "Hyperliquid original protection changed or is incomplete; order unchanged".into(),
            );
        }
        let job = self
            .trading
            .active
            .as_ref()
            .ok_or("Hyperliquid amendment owner is unavailable")?;
        let opts = job
            .steps
            .iter()
            .find_map(|s| match s {
                Step::Place(plan) => Some(&plan.opts),
                _ => None,
            })
            .ok_or("Hyperliquid replacement is unavailable")?;
        let precision = cx
            .store
            .market(&old.symbol)
            .ok_or("Market not found")?
            .precision
            .amount;
        let original = OrderWire {
            a: *self
                .assets
                .get(&old.symbol)
                .ok_or("Unknown Hyperliquid asset")?,
            b: old.side == OrderSide::Buy,
            p: float_wire(row.limit_px)?,
            s: float_wire(row.sz)?,
            r: row.reduce_only,
            t: WireType::Trigger {
                trigger: TriggerWire {
                    is_market: market,
                    trigger_px: float_wire(trigger)?,
                    tpsl: if kind == OrderType::TakeProfit {
                        "tp"
                    } else {
                        "sl"
                    },
                },
            },
            c: row.cloid.as_deref().unwrap_or_default().to_owned(),
        };
        if row.limit_px <= 0. || (!original.c.is_empty() && !valid_cloid(&original.c)) {
            return Err("Hyperliquid original execution price or client identity is not proven; order unchanged".into());
        }
        if !cx.store.loaded().positions || !has_position(cx.store, old) {
            return Err(
                "Hyperliquid position not loaded or already closed; order unchanged".into(),
            );
        }
        let mut before = old.clone();
        before.amount = row.orig_sz;
        before.filled = (row.orig_sz - row.sz).max(0.);
        before.remaining = row.sz;
        let captured = Amend {
            before,
            original,
            target_kind: opts.kind,
            target: price_wire(opts.price.unwrap_or(0.), precision, false)?
                .parse()
                .map_err(|_| "Invalid trigger")?,
            peers: cx
                .store
                .orders()
                .iter()
                .filter(|o| o.symbol == old.symbol && o.side == old.side && o.kind == old.kind)
                .map(|o| o.id.clone())
                .collect(),
        };
        let job = self.trading.active.as_mut().unwrap();
        job.protection = Some(Box::new(captured));
        Ok(())
    }
    pub(super) fn prepare_protective_replace(
        &self,
        cx: &Cx<'_, Pending>,
        opts: PlaceOrderOpts,
    ) -> Result<(Action, Effect), String> {
        let protected = self
            .trading
            .active
            .as_ref()
            .and_then(|j| j.protection.as_ref())
            .ok_or("Hyperliquid protective source is not proven")?;
        let mut wire = protected.original.clone();
        let precision = cx
            .store
            .market(&opts.symbol)
            .ok_or("Market not found")?
            .precision
            .amount;
        let keep_full = wire.s == "0"
            && self
                .trading
                .active
                .as_ref()
                .is_some_and(|j| j.amend_total.is_none_or(|amount| amount == 0.));
        let size = if keep_full {
            0.
        } else {
            num::adjust(opts.amount, precision)
        };
        if !keep_full && (size <= 0. || size * protected.target < MIN_ORDER_VALUE) {
            return Err("Hyperliquid protective replacement is below the minimum".into());
        }
        wire.s = float_wire(size)?;
        let WireType::Trigger { trigger } = &mut wire.t else {
            unreachable!();
        };
        trigger.trigger_px = float_wire(protected.target)?;
        trigger.tpsl = if opts.kind == OrderType::TakeProfit {
            "tp"
        } else {
            "sl"
        };
        wire.c = opts
            .client_order_id
            .clone()
            .ok_or("Replacement client identity is unavailable")?;
        let mut after = protected.before.clone();
        after.id.clear();
        after.kind = opts.kind;
        after.status = OrderStatus::Creating;
        after.price = protected.target;
        after.amount = size;
        after.remaining = size;
        after.filled = 0.;
        after.last_update = cx.now;
        after.client_order_id = (!wire.c.is_empty()).then(|| wire.c.clone());
        let grouping = if keep_full { "positionTpsl" } else { "na" };
        let market = cx.store.market(&opts.symbol).ok_or("Market not found")?;
        let native = NativePlacement::new(market, vec![wire], grouping);
        validate_placement(&native).map_err(|error| error.message)?;
        let builder = self
            .trading
            .active
            .as_ref()
            .and_then(|job| job.amend_builder.clone())
            .filter(|_| self.account.builder_approved == Some(true));
        Ok((
            Action::Order {
                orders: native.wires,
                grouping,
                builder,
            },
            Effect::Place(vec![Placement {
                at: cx.now,
                order: after,
            }]),
        ))
    }
    pub(crate) fn protective_release_veto(
        &self,
        store: &Store,
        pending: &Pending,
        release: &safe_cex_engine::request_context::HttpRelease,
    ) -> Result<(), VenueError> {
        let Pending::Trade(trade) = pending else {
            return Ok(());
        };
        let Some(job) = self
            .trading
            .active
            .as_ref()
            .filter(|j| j.command == trade.command)
        else {
            return Ok(());
        };
        let Some(protected) = job.protection.as_ref() else {
            return Ok(());
        };
        if !matches!(trade.effect, Effect::Cancel(_)) {
            return Ok(());
        }
        let now = release.time.raw_ms;
        if !store.loaded().positions || !has_position(store, &protected.before) {
            return Err(not_sent(
                "Hyperliquid position closed or is not loaded; original protection unchanged",
            ));
        }
        let source = store.order(&protected.before.id).ok_or_else(|| {
            not_sent("Hyperliquid original protection is no longer open; amendment not sent")
        })?;
        if source.symbol != protected.before.symbol
            || source.side != protected.before.side
            || source.kind != protected.before.kind
            || source.price != protected.before.price
            || source.filled != protected.before.filled
            || source.client_order_id != protected.before.client_order_id
            || !matches!(
                source.status,
                OrderStatus::Open | OrderStatus::Creating | OrderStatus::Canceling
            )
        {
            return Err(not_sent(
                "Hyperliquid protection changed after sizing; order unchanged, retry the edit",
            ));
        }
        let mark = store.reference_seen_at(&source.symbol, StopTriggerType::MarkPrice)
            .filter(|seen| now.is_finite() && *seen <= now)
            .and_then(|_| safe_cex_engine::protective_amend::fresh_reference(store, &source.symbol, StopTriggerType::MarkPrice, now))
            .ok_or_else(|| not_sent("Hyperliquid requires a fresh mark price before editing protection; order unchanged"))?;
        let (kind, trigger) = (protected.target_kind, protected.target);
        let fires = match (source.side, kind) {
            (OrderSide::Sell, OrderType::StopLoss) | (OrderSide::Buy, OrderType::TakeProfit) => {
                trigger >= mark
            }
            _ => trigger <= mark,
        };
        if fires {
            return Err(not_sent(
                "Hyperliquid protective trigger would immediately execute against the fresh mark price; request not sent",
            ));
        }
        Ok(())
    }
}

impl Hyperliquid {
    pub(super) fn protective_replacement_active(&self) -> bool {
        self.trading
            .active
            .as_ref()
            .is_some_and(|j| j.effective && j.protection.is_some())
    }
    pub(super) fn protective_trade_failure(
        &mut self,
        cx: &Cx<'_, Pending>,
        mut error: VenueError,
    ) -> VenueError {
        let Some(job) = self.trading.active.as_mut() else {
            return error;
        };
        let Some(p) = job.protection.as_ref() else {
            return error;
        };
        let definitive = error.kind != Some(ErrorKind::RateLimited)
            && (error.outcome == Some(Outcome::NotSent)
                || (error.outcome == Some(Outcome::Rejected)
                    && custody::known_protective_refusal(&error.message)));
        if !job.effective || !definitive || job.amend_filled {
            return error;
        }
        if restore_obsolete(cx.store, p) {
            job.stop_note = Some("Original protection was not restored because the position closed or newer protection superseded it".into());
            job.replacing_stop = None;
            return error;
        }
        error.message = format!(
            "UNPROTECTED: {} stop removed; please re-place protection. Replacement refused: {}",
            p.before.symbol, error.message
        );
        error.protection = Some(Protection::Unprotected);
        error
    }
}

fn has_position(store: &Store, order: &Order) -> bool {
    store.positions().iter().any(|p| {
        p.symbol == order.symbol
            && p.contracts > 0.
            && ((p.side == PositionSide::Long && order.side == OrderSide::Sell)
                || (p.side == PositionSide::Short && order.side == OrderSide::Buy))
    })
}

fn restore_obsolete(store: &Store, p: &Amend) -> bool {
    !store.loaded().positions
        || !has_position(store, &p.before)
        || store.orders().iter().any(|o| {
            o.symbol == p.before.symbol
                && o.side == p.before.side
                && o.kind == p.before.kind
                && !p.peers.contains(&o.id)
        })
}
