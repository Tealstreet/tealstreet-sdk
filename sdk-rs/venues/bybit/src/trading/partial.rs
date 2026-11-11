//! Native cancellation and standalone provenance for a Partial protective edit.
use super::{Call, Order, OrderType, PartialStopGuard, Planned, StopTriggerType};
use crate::sign::Query;
use safe_cex_engine::{ErrorKind, Outcome, Response, Store, VenueError};
use serde::Deserialize;

const CREATION_WINDOW_MS: u64 = 5_000;
const TARGET_POLL_DELAY_MS: u32 = 100;
/// The wait before each target history read: about 5 s in all for history to index the cancel.
const HISTORY_POLL_DELAYS_MS: [u32; 6] = [250, 500, 750, 1_000, 1_250, 1_500];
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

fn cancelled_terminal(status: &str) -> bool {
    matches!(status, "Cancelled" | "Deactivated")
}

fn matches_partial_kind(row: &NativeStop, kind: OrderType) -> bool {
    row.tpsl_mode.as_deref() == Some("Partial")
        && match kind {
            OrderType::StopLoss => matches!(
                row.stop_order_type.as_deref(),
                Some("StopLoss" | "PartialStopLoss")
            ),
            OrderType::TakeProfit => matches!(
                row.stop_order_type.as_deref(),
                Some("TakeProfit" | "PartialTakeProfit")
            ),
            _ => false,
        }
}

fn matches_original(row: &NativeStop, guard: &PartialStopGuard) -> bool {
    original_mismatch(row, guard).is_none()
}

/// The first field that differs from the original, named for the proof diagnostic.
fn original_mismatch(row: &NativeStop, guard: &PartialStopGuard) -> Option<&'static str> {
    let order = &guard.original;
    let limit = order
        .extra
        .as_ref()
        .and_then(|extra| extra.conditional_order_price);
    let checks = [
        ("orderId", row.order_id == order.id),
        ("symbol", row.symbol == order.symbol),
        ("side", row.side == super::side_text(order.side)),
        ("qty", equals(&row.qty, order.amount)),
        ("cumExecQty", equals(&row.cum_exec_qty, 0.0)),
        ("triggerPrice", equals(&row.trigger_price, order.price)),
        (
            "triggerBy",
            order
                .trigger_type
                .is_some_and(|by| row.trigger_by == reference(by)),
        ),
        (
            "orderType",
            row.order_type == if limit.is_some() { "Limit" } else { "Market" },
        ),
        ("price", equals(&row.price, limit.unwrap_or(0.0))),
        ("positionIdx", row.position_idx == guard.position_idx),
        ("kind", matches_partial_kind(row, order.kind)),
    ];
    checks
        .into_iter()
        .find_map(|(field, matched)| (!matched).then_some(field))
}

/// A venue enum value for the proof diagnostic; anything else is never echoed.
fn enum_token(raw: &str) -> &str {
    if (1..=32).contains(&raw.len()) && raw.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        raw
    } else {
        "other"
    }
}

/// Why a row failed the target check: a non-terminal status, the first mismatched field, or both.
fn row_reason(prefix: &str, row: &NativeStop, guard: &PartialStopGuard) -> String {
    let mut parts = Vec::new();
    if !cancelled_terminal(&row.order_status) {
        parts.push(format!("{prefix}-status={}", enum_token(&row.order_status)));
    }
    match original_mismatch(row, guard) {
        Some("kind") => parts.push(format!(
            "{prefix}-field=kind tpslMode={} stopOrderType={}",
            enum_token(row.tpsl_mode.as_deref().unwrap_or("none")),
            enum_token(row.stop_order_type.as_deref().unwrap_or("none"))
        )),
        Some(field) => parts.push(format!("{prefix}-field={field}")),
        None => {}
    }
    parts.join(" ")
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
    let created_time = created.parse::<u64>().unwrap();
    let created_ms = created_time as f64;
    if !created_ms.is_finite() || created_ms > now || now - created_ms > 86_400_000.0 {
        return false;
    }
    complete
        && matches_original(target, guard)
        && guard.original.parent_id.is_none()
        && target.parent_order_link_id.as_deref() == Some("")
        && matches_partial_kind(target, guard.original.kind)
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
                    && !(peer.side == target.side
                        && (peer.created_time.as_deref() == Some(created)
                            || (peer.tpsl_mode.as_deref() == Some("Partial")
                                && peer.created_time.as_deref().is_some_and(|time| {
                                    time.parse::<u64>().is_ok_and(|time| {
                                        time.abs_diff(created_time) <= CREATION_WINDOW_MS
                                    })
                                })))))
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
    history_reads: u8,
    history_window: Option<(u64, u64)>,
}

pub(super) enum Observation {
    Complete,
    Retry(Axis),
}

/// A failed proof read and the check that failed it: enum values only, never venue data.
pub(super) struct ProofFailure {
    pub(super) reason: String,
}

impl ProofFailure {
    fn new(axis: Axis, check: impl std::fmt::Display) -> Self {
        let axis = match axis {
            Axis::Target => "target",
            Axis::TargetHistory => "target-history",
            Axis::Live => "live",
            Axis::History => "history",
        };
        Self {
            reason: format!("axis={axis} {check}"),
        }
    }

    pub(super) fn error(&self) -> VenueError {
        VenueError::new(format!(
            "The protective cancel could not be confirmed from native orders; check Open Orders before retrying [proof: {}]",
            self.reason
        ))
        .kind(ErrorKind::Network)
        .outcome(Outcome::Unknown)
    }
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

    pub(super) fn skip_history(&mut self) {
        self.history_done = true;
        self.complete = false;
    }

    pub(super) fn history_done(&self) -> bool {
        self.history_done
    }

    pub(super) fn proof(&self, axis: Axis) -> Planned {
        proof(&self.guard.category, &self.guard.original, axis)
    }
    /// How long to wait before the next read on `axis`.
    pub(super) fn poll_delay_ms(&self, axis: Axis) -> u32 {
        match axis {
            Axis::TargetHistory => {
                let index = usize::from(self.history_reads).min(HISTORY_POLL_DELAYS_MS.len() - 1);
                HISTORY_POLL_DELAYS_MS[index]
            }
            _ => TARGET_POLL_DELAY_MS,
        }
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
            history_reads: 0,
            history_window: None,
        }
    }

    pub(super) fn observe(
        &mut self,
        axis: Axis,
        response: &Response<'_>,
        now: f64,
    ) -> Result<Observation, ProofFailure> {
        let failed = |check: &str| ProofFailure::new(axis, check);
        if response.status != 200 {
            return Err(failed(&format!("http={}", response.status)));
        }
        let super::acknowledgement::Answer::Success {
            result: Some(raw), ..
        } = super::acknowledgement::answer(response.body).map_err(|_| failed("envelope"))?
        else {
            return Err(failed("envelope"));
        };
        let mut page: Page = serde_json::from_str(raw.get()).map_err(|_| failed("decode"))?;
        self.observed_at = now;
        if page.category != self.guard.category {
            return Err(failed("category"));
        }
        if page.list.len() > 50 {
            return Err(failed("len"));
        }
        if matches!(axis, Axis::Target | Axis::TargetHistory) {
            self.target_reads += 1;
            let retry = if self.target_reads < 3 {
                Axis::Target
            } else {
                Axis::TargetHistory
            };
            if page.list.is_empty() && page.next_page_cursor.is_empty() {
                if axis == Axis::Target {
                    return Ok(Observation::Retry(retry));
                }
                // A unified account's cancelled order is only in history, which can lag the cancel.
                self.history_reads += 1;
                if usize::from(self.history_reads) < HISTORY_POLL_DELAYS_MS.len() {
                    return Ok(Observation::Retry(Axis::TargetHistory));
                }
            }
            if page.list.len() != 1 {
                return Err(failed(&format!("target-len={}", page.list.len())));
            }
            if !page.next_page_cursor.is_empty() {
                return Err(failed("target-cursor"));
            }
            let row = page.list.pop().unwrap();
            if axis == Axis::Target
                && matches!(row.order_status.as_str(), "Untriggered" | "New")
                && matches_original(&row, &self.guard)
            {
                return Ok(Observation::Retry(retry));
            }
            if !cancelled_terminal(&row.order_status) || !matches_original(&row, &self.guard) {
                return Err(failed(&row_reason("target", &row, &self.guard)));
            }
            self.terminal = Some(row);
        } else {
            if page.list.iter().any(|row| row.symbol != self.guard.symbol) {
                return Err(failed("peer-symbol"));
            }
            if let Some(row) = page.list.iter().find(|row| {
                row.order_id == self.guard.original.id
                    && (!cancelled_terminal(&row.order_status)
                        || !matches_original(row, &self.guard))
            }) {
                return Err(failed(&row_reason("peer-target", row, &self.guard)));
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
