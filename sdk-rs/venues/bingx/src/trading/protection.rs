use super::*;
use safe_cex_engine::error::Protection;
use safe_cex_engine::request_context::HttpRelease;
use safe_cex_engine::store::Store;
use serde::{Deserialize, Serialize};

#[derive(Default)]
pub struct References([Option<(f64, f64)>; 2], [Option<f64>; 2]);
impl References {
    fn slot(source: StopTriggerType) -> Option<usize> {
        match source {
            StopTriggerType::LastPrice => Some(0),
            StopTriggerType::MarkPrice => Some(1),
            StopTriggerType::IndexPrice => None,
        }
    }
    fn observe(&mut self, source: StopTriggerType, value: f64, now: f64) {
        if let Some(slot) = Self::slot(source) {
            if !now.is_finite() {
                self.0[slot] = None;
                return;
            }
            if self.1[slot].is_some_and(|seen| now < seen) {
                return;
            }
            self.1[slot] = Some(now);
            self.0[slot] = (value.is_finite() && value > 0.0).then_some((value, now));
        }
    }
    pub(super) fn received_after(&self, source: StopTriggerType, at: f64) -> bool {
        Self::slot(source)
            .and_then(|slot| self.1[slot])
            .is_some_and(|seen| seen > at)
    }
    pub(super) fn invalidate(&mut self) {
        self.0 = [None; 2];
    }
    fn get(&self, source: StopTriggerType, now: f64) -> Option<f64> {
        let (value, at) = self.0[Self::slot(source)?]?;
        (now.is_finite() && (0.0..=5_000.0).contains(&(now - at))).then_some(value)
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NativeScalar {
    Text(String),
    Boolean(bool),
    Number(serde_json::Number),
}
impl NativeScalar {
    fn add(&self, params: Params, key: &str) -> Option<Params> {
        Some(match self {
            Self::Text(s) => params.str(key, s),
            Self::Boolean(b) => params.boolean(key, *b),
            Self::Number(n) => match n.as_u64() {
                Some(n) => params.integer(key, n),
                None if key != "positionId" => {
                    params.num(key, n.as_f64().filter(|v| v.is_finite())?)
                }
                None => return None,
            },
        })
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Original {
    pub symbol: String,
    #[serde(rename = "type")]
    pub native_kind: String,
    pub side: String,
    pub position_side: String,
    pub quantity: f64,
    pub price: Option<f64>,
    pub stop_price: Option<f64>,
    pub working_type: Option<String>,
    pub reduce_only: Option<bool>,
    #[serde(default)]
    pub close_position: bool,
    pub client_order_id: Option<String>,
    pub time_in_force: Option<String>,
    pub stop_guaranteed: Option<NativeScalar>,
    pub price_protect: Option<NativeScalar>,
    pub position_id: Option<NativeScalar>,
}
impl Original {
    pub(super) fn matches(&self, order: &Order, kind: &str, position_side: &str) -> bool {
        self.symbol.replace('-', "") == order.symbol
            && self.native_kind == kind
            && self.side == side(order.side)
            && self.position_side == position_side
            && self.quantity == order.amount
            && (if order.kind == OrderType::Limit {
                self.price
            } else {
                self.stop_price
            }) == Some(order.price)
            && self.close_position == (order.close == Some(true))
            && (position_side != "BOTH"
                || self.close_position
                || self.reduce_only == Some(order.reduce_only))
            && (order.kind == OrderType::Limit
                || self.working_type.as_deref()
                    == order.trigger_type.and_then(|s| working_type(s).ok()))
    }
    pub(super) fn params(&self) -> Option<Params> {
        if !self.quantity.is_finite() || self.quantity <= 0.0 {
            return None;
        }
        let mut p = Params::new()
            .str("symbol", &self.symbol)
            .str("type", &self.native_kind)
            .str("side", &self.side)
            .str("positionSide", &self.position_side)
            .num("quantity", self.quantity)
            .opt_num("price", self.price)
            .opt_num("stopPrice", self.stop_price)
            .opt_str("workingType", self.working_type.as_deref())
            .opt_str("timeInForce", self.time_in_force.as_deref());
        if self.close_position {
            p = p.str("closePosition", "true");
        } else if let Some(reduce) = self.reduce_only {
            p = p.boolean("reduceOnly", reduce);
        }
        // Trigger orders do not accept caller ids; limit ids have no documented reuse contract.
        if self.client_order_id.as_ref().is_some_and(|s| !s.is_empty()) {
            return None;
        }
        for (key, value) in [
            ("stopGuaranteed", &self.stop_guaranteed),
            ("priceProtect", &self.price_protect),
            ("positionId", &self.position_id),
        ] {
            if let Some(value) = value {
                p = value.add(p, key)?;
            }
        }
        Some(p)
    }
}

#[derive(Clone)]
pub(super) struct Guard {
    pub order: Order,
    pub trigger: Option<f64>,
    pub kind: OrderType,
    pub source: Option<StopTriggerType>,
    pub closing: bool,
}
impl Guard {
    // Admission may obtain missing proof once; the final Claim never does I/O.
    pub(super) fn needs_reference(
        &self,
        store: &Store,
        references: &HashMap<String, References>,
        now: f64,
    ) -> bool {
        self.trigger.is_some()
            && self.source.is_some_and(|source| {
                !references
                    .get(&self.order.symbol)
                    .and_then(|r| r.get(source, now))
                    .is_some_and(|value| {
                        store.ticker(&self.order.symbol).is_some_and(|t| {
                            value
                                == match source {
                                    StopTriggerType::LastPrice => t.last,
                                    StopTriggerType::MarkPrice => t.mark,
                                    StopTriggerType::IndexPrice => t.index,
                                }
                        })
                    })
            })
    }

    pub(super) fn check(
        &self,
        store: &Store,
        references: &HashMap<String, References>,
        now: f64,
        restoring: bool,
    ) -> Result<(), VenueError> {
        let refuse = |cause: &str| {
            let mut error = not_sent(format!(
                "Your {} stop was not changed: {cause}",
                self.order.symbol
            ))
            .kind(ErrorKind::InvalidInput);
            error.presents = true;
            error
        };
        let current = store
            .order(&self.order.id)
            .ok_or_else(|| refuse("order changed"))?;
        let mut expected = self.order.clone();
        if restoring {
            expected.status = OrderStatus::Canceled;
            expected.last_update = current.last_update;
        }
        if *current != expected || current.filled != 0.0 || current.remaining != current.amount {
            return Err(refuse("order changed or filled"));
        }
        if self.closing {
            let side = if self.order.side == OrderSide::Sell {
                PositionSide::Long
            } else {
                PositionSide::Short
            };
            if !store.positions().iter().any(|p| {
                p.symbol == self.order.symbol
                    && p.side == side
                    && p.contracts.is_finite()
                    && p.contracts > 0.0
            }) {
                return Err(refuse("protected position is no longer on that side"));
            }
        }
        if let Some(trigger) = if restoring {
            self.trigger.map(|_| self.order.price)
        } else {
            self.trigger
        } {
            let source = self.source.ok_or_else(|| refuse("reference stale"))?;
            let reference = references
                .get(&self.order.symbol)
                .and_then(|r| r.get(source, now))
                .filter(|value| {
                    store.ticker(&self.order.symbol).is_some_and(|t| {
                        *value
                            == match source {
                                StopTriggerType::LastPrice => t.last,
                                StopTriggerType::MarkPrice => t.mark,
                                StopTriggerType::IndexPrice => t.index,
                            }
                    })
                })
                .ok_or_else(|| refuse("reference stale"))?;
            let crossed = match (self.kind, self.order.side) {
                (OrderType::StopLoss, OrderSide::Sell)
                | (OrderType::TakeProfit, OrderSide::Buy) => trigger >= reference,
                _ => trigger <= reference,
            };
            if crossed {
                return Err(refuse("replacement trigger would execute immediately"));
            }
        }
        Ok(())
    }
}

impl Trading {
    pub(crate) fn observe_reference(
        &mut self,
        symbol: &str,
        source: StopTriggerType,
        value: f64,
        now: f64,
    ) {
        if let Some(receipt) = self.references.get_mut(symbol) {
            receipt.observe(source, value, now);
        } else {
            let mut receipt = References::default();
            receipt.observe(source, value, now);
            self.references.insert(symbol.to_owned(), receipt);
        }
    }

    // Every destructive managed attempt, including a re-sign, checks the same snapshot.
    // No scheduling or mutation is permitted in the final Claim veto.
    pub(crate) fn protective_release(
        &self,
        store: &Store,
        request: &Request,
        release: &HttpRelease,
    ) -> Result<(), VenueError> {
        if !request.protective {
            return Ok(());
        }
        let guard = self
            .jobs
            .get(&request.command)
            .filter(|r| r.plan.owner() == request.plan.owner())
            .and_then(|r| r.job.guard.as_ref())
            .ok_or_else(|| not_sent("Protective edit guard is no longer available"))?;
        guard.check(
            store,
            &self.references,
            release.time.raw_ms,
            request.restoring,
        )
    }
}

impl Job {
    pub(super) fn unknown_words(&self) -> Option<String> {
        self.guard.as_ref().map(|guard| {
            format!(
                "{UNKNOWN_WORDS}. Your {} {} was not confirmed; check protection before re-placing it",
                guard.order.symbol,
                if self.restoring {
                    "original stop put-back"
                } else {
                    "order change"
                }
            )
        })
    }
    pub(super) fn protection_error(&self, mut error: VenueError, restored: bool) -> VenueError {
        let Some(guard) = &self.guard else {
            return error;
        };
        error.presents = true;
        if restored {
            error.protection = Some(Protection::Restored);
            error.message = format!(
                "Your {} stop is unchanged: its replacement was refused, so the original stop was put back as a new order",
                guard.order.symbol
            );
        } else if error.outcome == Some(Outcome::Unknown) {
            error.protection = None;
            error.message = self.unknown_words().unwrap();
        } else {
            error.protection = Some(Protection::Unprotected);
            error.message = format!(
                "Your {} position is unprotected: the replacement or original stop could not be confirmed in place; re-place protection. {}",
                guard.order.symbol, error.message
            );
        }
        error
    }
}

pub(super) const REPLACEMENT_FAILED: safe_cex_engine::classify::CodeTable =
    safe_cex_engine::classify::CodeTable {
        rows: &[safe_cex_engine::classify::Row {
            key: safe_cex_engine::classify::Key::Text("FAILED"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        }],
        success: &[],
        unlisted: Verdict::Unknown(None),
        status: &[],
    };
pub(super) const RESTORE_AUTH: safe_cex_engine::classify::CodeTable =
    safe_cex_engine::classify::CodeTable {
        rows: &[
            safe_cex_engine::classify::Row {
                key: safe_cex_engine::classify::Key::Int(100001),
                verdict: Verdict::Refused(ErrorKind::AuthInvalid),
                only: None,
            },
            safe_cex_engine::classify::Row {
                key: safe_cex_engine::classify::Key::Int(109400),
                verdict: Verdict::Refused(ErrorKind::InvalidInput),
                only: None,
            },
        ],
        success: &[],
        unlisted: Verdict::Unknown(None),
        status: &[],
    };
