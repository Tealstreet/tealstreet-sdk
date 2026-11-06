//! Native cancellation and standalone provenance for a Partial protective edit.
use super::{Call, Order, OrderType, PartialStopGuard, Planned, StopTriggerType};
use crate::sign::Query;
use safe_cex_engine::{ErrorKind, Outcome, Response, Store, VenueError};
use serde::Deserialize;

const CREATION_WINDOW_MS: u64 = 5_000;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Axis {
    Target,
    TargetHistory,
    Live,
    History,
}

pub(super) fn proof(category: &str, order: &Order, axis: Axis) -> Planned {
    let query = Query::new()
        .str("category", category)
        .str("symbol", &order.symbol);
    let query = match axis {
        Axis::Target | Axis::TargetHistory => query.str("orderId", &order.id),
        Axis::Live => query.str("openOnly", "0").str("limit", "50"),
        Axis::History => query.str("limit", "50"),
    };
    Planned {
        receipts: Vec::new(),
        path: if matches!(axis, Axis::History | Axis::TargetHistory) {
            "/v5/order/history"
        } else {
            "/v5/order/realtime"
        },
        body: query.into_string(),
        call: Call::PartialProof(axis),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeStop {
    order_id: String,
    symbol: String,
    side: String,
    order_status: String,
    qty: String,
    cum_exec_qty: String,
    trigger_price: String,
    trigger_by: String,
    order_type: String,
    price: String,
    position_idx: i64,
    #[serde(default)]
    stop_order_type: Option<String>,
    #[serde(default)]
    tpsl_mode: Option<String>,
    #[serde(default)]
    parent_order_link_id: Option<String>,
    #[serde(default)]
    order_link_id: Option<String>,
    #[serde(default)]
    created_time: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page {
    category: String,
    list: Vec<NativeStop>,
    next_page_cursor: String,
}

fn equals(raw: &str, expected: f64) -> bool {
    raw.parse::<f64>()
        .is_ok_and(|n| n.is_finite() && n == expected)
}

fn reference(reference: StopTriggerType) -> &'static str {
    match reference {
        StopTriggerType::LastPrice => "LastPrice",
        StopTriggerType::IndexPrice => "IndexPrice",
        StopTriggerType::MarkPrice => "MarkPrice",
    }
}

fn matches_original(row: &NativeStop, guard: &PartialStopGuard) -> bool {
    let order = &guard.original;
    let limit = order
        .extra
        .as_ref()
        .and_then(|extra| extra.conditional_order_price);
    row.order_id == order.id
        && row.symbol == order.symbol
        && row.side == super::side_text(order.side)
        && equals(&row.qty, order.amount)
        && equals(&row.cum_exec_qty, 0.0)
        && equals(&row.trigger_price, order.price)
        && order
            .trigger_type
            .is_some_and(|by| row.trigger_by == reference(by))
        && row.order_type == if limit.is_some() { "Limit" } else { "Market" }
        && equals(&row.price, limit.unwrap_or(0.0))
        && row.position_idx == guard.position_idx
        && row.tpsl_mode.as_deref() == Some("Partial")
        && row.stop_order_type.as_deref()
            == Some(if order.kind == OrderType::StopLoss {
                "StopLoss"
            } else {
                "TakeProfit"
            })
}

/// Missing native lineage or an incomplete peer census never authorizes put-back.
fn standalone_provenance(
    target: &NativeStop,
    peers: &[NativeStop],
    complete: bool,
    guard: &PartialStopGuard,
    now: f64,
) -> bool {
    let Some(link) = target
        .order_link_id
        .as_deref()
        .filter(|link| !link.is_empty())
    else {
        return false;
    };
    let Some(created) = target
        .created_time
        .as_deref()
        .filter(|time| time.parse::<u64>().is_ok_and(|n| n > 0))
    else {
        return false;
    };
    let created_ms = created.parse::<f64>().unwrap_or(f64::NAN);
    if !created_ms.is_finite() || created_ms > now || now - created_ms > 86_400_000.0 {
        return false;
    }
    complete
        && matches_original(target, guard)
        && guard.original.parent_id.is_none()
        && target.parent_order_link_id.as_deref() == Some("")
        && target.tpsl_mode.as_deref() == Some("Partial")
        && target.stop_order_type.as_deref()
            == Some(if guard.original.kind == OrderType::StopLoss {
                "StopLoss"
            } else {
                "TakeProfit"
            })
        && peers.iter().all(|peer| {
            peer.order_id == target.order_id
                || (peer.order_link_id.is_some()
                    && peer.parent_order_link_id.is_some()
                    && peer
                        .created_time
                        .as_ref()
                        .is_some_and(|time| time.parse::<u64>().is_ok_and(|n| n > 0))
                    && peer.order_link_id.as_deref() != Some(link)
                    && peer.parent_order_link_id.as_deref() != Some(link)
                    && !(peer.created_time.as_deref() == Some(created) && peer.side == target.side))
        })
}

pub(super) struct Flow {
    guard: PartialStopGuard,
    terminal: Option<NativeStop>,
    peers: Vec<NativeStop>,
    complete: bool,
    history_done: bool,
    observed_at: f64,
    target_reads: u8,
    history_window: Option<(u64, u64)>,
}

pub(super) enum Observation {
    Complete,
    Retry(Axis),
}

impl Flow {
    pub(super) fn history_proof(&mut self) -> Option<Planned> {
        let created = self
            .terminal
            .as_ref()?
            .created_time
            .as_ref()?
            .parse::<u64>()
            .ok()?;
        if created == 0
            || created > MAX_SAFE_INTEGER - CREATION_WINDOW_MS
            || created as f64 > self.observed_at
        {
            return None;
        }
        let start = created.saturating_sub(CREATION_WINDOW_MS).max(1);
        let end = created + CREATION_WINDOW_MS;
        let mut planned = self.proof(Axis::History);
        planned
            .body
            .push_str(&format!("&startTime={start}&endTime={end}"));
        self.history_window = Some((start, end));
        Some(planned)
    }

    pub(super) fn proof(&self, axis: Axis) -> Planned {
        proof(&self.guard.category, &self.guard.original, axis)
    }
    pub(super) fn target_id(&self) -> &str {
        &self.guard.original.id
    }
    pub(super) fn new(guard: PartialStopGuard) -> Self {
        Self {
            guard,
            terminal: None,
            peers: Vec::new(),
            complete: true,
            history_done: false,
            observed_at: 0.0,
            target_reads: 0,
            history_window: None,
        }
    }

    pub(super) fn observe(
        &mut self,
        axis: Axis,
        response: &Response<'_>,
        now: f64,
    ) -> Result<Observation, VenueError> {
        let failed = || {
            VenueError::new("The protective cancel could not be confirmed from native orders; check Open Orders before retrying")
            .kind(ErrorKind::Network).outcome(Outcome::Unknown)
        };
        if response.status != 200 {
            return Err(failed());
        }
        let super::acknowledgement::Answer::Success {
            result: Some(raw), ..
        } = super::acknowledgement::answer(response.body).map_err(|_| failed())?
        else {
            return Err(failed());
        };
        let mut page: Page = serde_json::from_str(raw.get()).map_err(|_| failed())?;
        self.observed_at = now;
        if page.category != self.guard.category || page.list.len() > 50 {
            return Err(failed());
        }
        if matches!(axis, Axis::Target | Axis::TargetHistory) {
            self.target_reads += 1;
            let retry = if self.target_reads < 3 {
                Axis::Target
            } else {
                Axis::TargetHistory
            };
            if page.list.is_empty() && axis == Axis::Target && page.next_page_cursor.is_empty() {
                return Ok(Observation::Retry(retry));
            }
            if page.list.len() != 1 || !page.next_page_cursor.is_empty() {
                return Err(failed());
            }
            let row = page.list.pop().unwrap();
            if axis == Axis::Target
                && matches!(row.order_status.as_str(), "Untriggered" | "New")
                && matches_original(&row, &self.guard)
            {
                return Ok(Observation::Retry(retry));
            }
            if row.order_status != "Cancelled" || !matches_original(&row, &self.guard) {
                return Err(failed());
            }
            self.terminal = Some(row);
        } else {
            if page.list.iter().any(|row| {
                row.symbol != self.guard.symbol
                    || (row.order_id == self.guard.original.id
                        && (row.order_status != "Cancelled" || !matches_original(row, &self.guard)))
            }) {
                return Err(failed());
            }
            self.complete &= page.next_page_cursor.is_empty();
            if axis == Axis::History {
                self.complete &= self.history_window.is_some_and(|(start, end)| {
                    page.list.iter().all(|row| {
                        row.created_time
                            .as_deref()
                            .and_then(|time| time.parse::<u64>().ok())
                            .is_some_and(|time| start <= time && time <= end)
                    })
                });
            }
            self.peers.extend(page.list);
            self.history_done |= axis == Axis::History;
        }
        Ok(Observation::Complete)
    }

    pub(super) fn restorable(&self) -> bool {
        self.history_done
            && self.terminal.as_ref().is_some_and(|target| {
                standalone_provenance(
                    target,
                    &self.peers,
                    self.complete,
                    &self.guard,
                    self.observed_at,
                )
            })
    }

    pub(super) fn validate(
        &self,
        store: &Store,
        now: f64,
        restoring: bool,
    ) -> Result<(), VenueError> {
        if self.terminal.is_none() || !self.history_done || (restoring && !self.restorable()) {
            return Err(VenueError::new("Protection cannot be re-placed without native cancellation and lineage proof; check Open Orders").outcome(Outcome::NotSent));
        }
        let validation = self.guard.validate_order(store, true).and_then(|()| {
            if restoring {
                let mut guard = self.guard.clone();
                guard.kind = guard.original.kind;
                guard.price = guard.original.price;
                guard.reference = guard
                    .original
                    .trigger_type
                    .expect("native proof required reference");
                guard.validate(store, now)
            } else {
                self.guard.validate(store, now)
            }
        });
        validation.map_err(|mut error| {
            error.protection = Some(safe_cex_engine::error::Protection::Unprotected);
            error.presents = true;
            error.message = format!("{}: the reference, order, or protected position changed after cancellation; re-place protection on Bybit", safe_cex_engine::error::stop_gone_text(&self.guard.symbol, self.guard.side, self.guard.original.kind, false));
            error
        })
    }
}
