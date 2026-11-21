//! Native cancellation and standalone provenance for a Partial protective edit.
use super::{Call, Order, OrderType, PartialStopGuard, Planned, StopTriggerType};
use crate::sign::Query;
use safe_cex_engine::{Outcome, Response, Store, VenueError};
use serde::Deserialize;

mod readiness;
pub(super) use readiness::Readiness;

const CREATION_WINDOW_MS: u64 = 5_000;
const TARGET_POLL_DELAY_MS: u32 = 100;
/// The wait before each target history read: about 5 s in all for history to index the cancel.
const HISTORY_POLL_DELAYS_MS: [u32; 6] = [250, 500, 750, 1_000, 1_250, 1_500];
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
/// Proof reads lost in transport that are read again (about 25 s with backoff) before
/// the plan gives up. A failed read may only be followed by another read (engine guard
/// `failed_read_cannot_continue_as_mutation`), so the edit continues once one answers.
const LOST_PROOF_READS: u8 = 20;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Axis {
    Target,
    TargetHistory,
    Live,
    History,
    /// One last orderId read on realtime before acting on missing evidence.
    Final,
}

/// What a failed proof read says about the old stop.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Evidence {
    /// Missing or unreadable: the cancel landed, so the edit still goes out.
    Missing,
    /// Bybit still shows the old stop live: a replacement would make two.
    Live,
    /// Bybit shows the old stop triggered or filled: the position already moved.
    Executed,
}

pub(super) fn proof(category: &str, order: &Order, axis: Axis) -> Planned {
    let query = Query::new()
        .str("category", category)
        .str("symbol", &order.symbol);
    let query = match axis {
        Axis::Target | Axis::TargetHistory | Axis::Final => query.str("orderId", &order.id),
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
    // Ordinary peer orders have no trigger source; target matching still requires it.
    #[serde(default)]
    trigger_by: Option<String>,
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

// Share strict decoding between readiness and fresh post-cancel evidence.
#[inline(never)]
fn decode_page(body: &str) -> Result<Page, &'static str> {
    let super::acknowledgement::Answer::Success {
        result: Some(raw), ..
    } = super::acknowledgement::answer(body).map_err(|_| "envelope")?
    else {
        return Err("envelope");
    };
    serde_json::from_str(raw.get()).map_err(|_| "decode")
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

fn executed(row: &NativeStop) -> bool {
    row.cum_exec_qty.parse::<f64>().is_ok_and(|qty| qty != 0.0)
        || matches!(
            row.order_status.as_str(),
            "Triggered" | "Filled" | "PartiallyFilled" | "PartiallyFilledCanceled"
        )
}

/// What a row naming the old stop's own id says about it; any other row says nothing.
fn evidence(row: &NativeStop, target: &str) -> Evidence {
    if row.order_id != target {
        Evidence::Missing
    } else if executed(row) {
        Evidence::Executed
    } else if matches!(
        row.order_status.as_str(),
        "Untriggered" | "New" | "Active" | "Created"
    ) {
        Evidence::Live
    } else {
        Evidence::Missing
    }
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
                .is_some_and(|by| row.trigger_by.as_deref() == Some(reference(by))),
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

/// Whether Bybit shows the original changed: a field it sent that differs. A missing or
/// unreadable field is no evidence of a change (readiness then says "couldn't confirm").
fn shows_original_changed(row: &NativeStop, guard: &PartialStopGuard) -> bool {
    let order = &guard.original;
    let limit = order
        .extra
        .as_ref()
        .and_then(|extra| extra.conditional_order_price);
    let number_differs = |raw: &str, expected: f64| {
        raw.parse::<f64>()
            .is_ok_and(|n| n.is_finite() && n != expected)
    };
    let text_differs = |raw: Option<&str>, matches: &dyn Fn(&str) -> bool| {
        raw.is_some_and(|raw| !raw.is_empty() && !matches(raw))
    };
    number_differs(&row.qty, order.amount)
        || number_differs(&row.trigger_price, order.price)
        || number_differs(&row.price, limit.unwrap_or(0.0))
        || row.position_idx != guard.position_idx
        || text_differs(Some(&row.side), &|side| {
            side == super::side_text(order.side)
        })
        || text_differs(Some(&row.order_type), &|kind| {
            kind == if limit.is_some() { "Limit" } else { "Market" }
        })
        || order
            .trigger_type
            .is_some_and(|by| text_differs(row.trigger_by.as_deref(), &|raw| raw == reference(by)))
        || text_differs(row.tpsl_mode.as_deref(), &|mode| mode == "Partial")
        || text_differs(row.stop_order_type.as_deref(), &|stop| match order.kind {
            OrderType::StopLoss => matches!(stop, "StopLoss" | "PartialStopLoss"),
            OrderType::TakeProfit => matches!(stop, "TakeProfit" | "PartialTakeProfit"),
            _ => false,
        })
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
    /// A proof read failed after the cancel landed: the edit is placed anyway.
    unproven: bool,
    /// Lost proof reads already read again.
    lost_reads: u8,
    /// The failed check that sent the plan to its final orderId read.
    first_failure: Option<String>,
    /// AGT-57: the old stop's row as stored when the proof retired it. Demo and HyroTrader
    /// then remove it, which must not erase a fill or amount change it already showed.
    retired: Option<Order>,
}

pub(super) enum Observation {
    Complete,
    Retry(Axis),
}

/// A failed proof read and the check that failed it: enum values only, never venue data.
pub(super) struct ProofFailure {
    pub(super) reason: String,
    pub(super) evidence: Evidence,
}

impl ProofFailure {
    fn new(axis: Axis, check: impl std::fmt::Display) -> Self {
        let axis = match axis {
            Axis::Target => "target",
            Axis::TargetHistory => "target-history",
            Axis::Live => "live",
            Axis::History => "history",
            Axis::Final => "final",
        };
        Self {
            reason: format!("axis={axis} {check}"),
            evidence: Evidence::Missing,
        }
    }

    fn showing(mut self, evidence: Evidence) -> Self {
        self.evidence = evidence;
        self
    }

    /// The definite end of an edit whose old stop Bybit still shows live or executed.
    pub(super) fn not_replaced(&self, symbol: &str, kind: OrderType) -> VenueError {
        let what = if kind == OrderType::TakeProfit {
            "take profit"
        } else {
            "stop"
        };
        let shows = if self.evidence == Evidence::Executed {
            "shows the old order already triggered or filled"
        } else {
            "still shows it live after accepting its cancel"
        };
        VenueError {
            kind: Some(safe_cex_engine::ErrorKind::Rejected),
            outcome: Some(Outcome::Rejected),
            presents: true,
            ..VenueError::new(format!(
                "Your {symbol} {what} was not edited: Bybit {shows}, so no replacement was placed. Check your open orders."
            ))
        }
    }

    /// Told to the user while the replacement still goes out.
    pub(super) fn warning(&self, symbol: &str, _earlier: Option<&str>) -> String {
        format!(
            "Your {symbol} replacement was sent before the old order’s cancellation was confirmed. Check your open orders for a duplicate."
        )
    }

    /// The command's own outcome when the edit went out on unconfirmed evidence: the
    /// old stop may still be live beside it, so this is presented, never a silent success.
    pub(super) fn second_stop_may_exist(&self, symbol: &str, _earlier: Option<&str>) -> VenueError {
        VenueError {
            kind: Some(safe_cex_engine::ErrorKind::Rejected),
            presents: true,
            ..VenueError::new(format!(
                "Your {symbol} edit was sent before the old stop’s cancellation was confirmed. Check your open orders for a second stop."
            ))
        }
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
    pub(super) fn symbol(&self) -> &str {
        &self.guard.symbol
    }
    pub(super) fn kind(&self) -> OrderType {
        self.guard.original.kind
    }
    /// The explicit end when neither the replacement nor the old stop can be sent.
    pub(super) fn unprotected(&self, cause: &str) -> VenueError {
        let order = &self.guard.original;
        let position = if order.side == super::OrderSide::Sell {
            "long"
        } else {
            "short"
        };
        let what = if order.kind == OrderType::TakeProfit {
            "take profit"
        } else {
            "stop"
        };
        let gone = format!(
            "Your {} {position} position has no {what}: the old {what} was canceled and {cause}",
            order.symbol
        );
        VenueError {
            kind: Some(safe_cex_engine::ErrorKind::Network),
            outcome: Some(Outcome::NotSent),
            protection: Some(safe_cex_engine::error::Protection::Unprotected),
            presents: true,
            ..VenueError::new(format!("{gone}; re-place protection on Bybit"))
        }
    }
    pub(super) fn note_failure(&mut self, failure: &ProofFailure) {
        self.first_failure
            .get_or_insert_with(|| failure.reason.clone());
    }
    pub(super) fn first_failure(&self) -> Option<&str> {
        self.first_failure.as_deref()
    }
    /// Whether the old stop's own row has been read as cancelled.
    pub(super) fn target_proven(&self) -> bool {
        self.terminal.is_some()
    }
    pub(super) fn target_id(&self) -> &str {
        &self.guard.original.id
    }
    /// Keeps the old stop's stored row before the proof retires it from the store.
    pub(super) fn retire(&mut self, store: &Store) {
        if let Some(row) = store.order(&self.guard.original.id) {
            self.retired = Some(row.clone());
        }
    }
    /// The post-cancel order check against the latest row, still stored or retired.
    fn validate_order(&self, store: &Store) -> Result<(), VenueError> {
        let current = store
            .order(&self.guard.original.id)
            .or(self.retired.as_ref());
        self.guard.validate_order_row(current, store, true)
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
            unproven: false,
            lost_reads: 0,
            first_failure: None,
            retired: None,
        }
    }

    /// Never leave the position with neither stop: once the cancel has landed, a
    /// failed proof stops proving and lets the replacement (or put-back) go out.
    pub(super) fn abandon_proof(&mut self) {
        self.unproven = true;
        self.history_done = true;
        self.complete = false;
    }

    /// Whether a proof read lost in transport may be read again.
    pub(super) fn read_lost_proof_again(&mut self) -> bool {
        self.lost_reads += 1;
        self.lost_reads <= LOST_PROOF_READS
    }

    /// Backoff before reading a lost proof again.
    pub(super) fn lost_read_delay_ms(&self) -> u32 {
        let index =
            usize::from(self.lost_reads.saturating_sub(1)).min(HISTORY_POLL_DELAYS_MS.len() - 1);
        HISTORY_POLL_DELAYS_MS[index]
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
        let mut page = decode_page(response.body).map_err(failed)?;
        self.observed_at = now;
        if page.category != self.guard.category {
            return Err(failed("category"));
        }
        if page.list.len() > 50 {
            return Err(failed("len"));
        }
        if axis == Axis::Final {
            let [row] = page.list.as_slice() else {
                return Err(failed(&format!("final-len={}", page.list.len())));
            };
            if row.order_id != self.guard.original.id || row.symbol != self.guard.symbol {
                return Err(failed("final-field=identity"));
            }
            if !cancelled_terminal(&row.order_status) || executed(row) {
                return Err(failed(&row_reason("final", row, &self.guard))
                    .showing(evidence(row, &self.guard.original.id)));
            }
            // Cancelled with nothing executed: the stop is gone, so its edit goes out.
            self.terminal = page.list.pop();
            self.abandon_proof();
            return Ok(Observation::Complete);
        }
        if matches!(axis, Axis::Target | Axis::TargetHistory) {
            self.target_reads += 1;
            let retry = if self.target_reads < 3 {
                Axis::Target
            } else {
                Axis::TargetHistory
            };
            // AGT-34: this read is filtered by orderId, which names at most one order, so
            // `nextPageCursor` is only a page token. Bybit returns one beside the single
            // row; the former `target-cursor` refusal left a live position unprotected.
            if page.list.is_empty() {
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
            let row = page.list.pop().unwrap();
            if axis == Axis::Target
                && matches!(row.order_status.as_str(), "Untriggered" | "New")
                && matches_original(&row, &self.guard)
            {
                return Ok(Observation::Retry(retry));
            }
            if !cancelled_terminal(&row.order_status) || !matches_original(&row, &self.guard) {
                return Err(failed(&row_reason("target", &row, &self.guard))
                    .showing(evidence(&row, &self.guard.original.id)));
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
                return Err(failed(&row_reason("peer-target", row, &self.guard))
                    .showing(evidence(row, &self.guard.original.id)));
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

    /// Why the replacement would be refused at release; checked before it is sent.
    pub(super) fn replacement_refusal(&self, store: &Store, now: f64) -> Option<String> {
        self.validate(store, now, false).err()?;
        let raw = self
            .validate_order(store)
            .and_then(|()| self.guard.validate(store, now));
        Some(raw.err().map_or_else(
            || "the replacement could not be placed".to_owned(),
            |error| error.message,
        ))
    }

    pub(super) fn validate(
        &self,
        store: &Store,
        now: f64,
        restoring: bool,
    ) -> Result<(), VenueError> {
        // AGT-34: lineage no longer gates the put-back; a cancelled stop always goes back.
        if !self.unproven && (self.terminal.is_none() || !self.history_done) {
            return Err(VenueError::new("The old stop’s cancellation hasn’t been confirmed. Check your open orders before adding protection.").outcome(Outcome::NotSent));
        }
        let validation = self.validate_order(store).and_then(|()| {
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
