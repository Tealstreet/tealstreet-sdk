//! Read-only account/close diagnostics. This state never authorizes or changes a write.
use crate::{BitgetFutures, Pending};
use safe_cex_engine::log::Value;
use safe_cex_engine::model::*;
use safe_cex_engine::{Cx, Level};
use std::collections::{HashSet, VecDeque};

const PENDING_LIMIT: usize = 512;
const BOOKING_WINDOW_MS: f64 = 300_000.0;

#[derive(Clone)]
pub(crate) struct CloseRequest {
    pub symbol: String,
    pub side: PositionSide,
    pub requested: f64,
    pub submitted: f64,
    pub client: String,
}
struct PendingClose {
    close: CloseRequest,
    native: Option<String>,
    at: f64,
}
struct Diagnostic<'a> {
    reason: &'static str,
    booked: Option<f64>,
    code: Option<&'a str>,
    message: Option<&'a str>,
}
#[derive(Default)]
pub(crate) struct Telemetry {
    startup_reported: bool,
    pending: VecDeque<PendingClose>,
}

impl BitgetFutures {
    pub(crate) fn account_mode_event(&mut self, cx: &mut Cx<'_, Pending>) {
        let loaded = cx.store.loaded();
        if self.telemetry.startup_reported
            || !self.account.detected
            || !loaded.positions
            || !loaded.orders
            || !loaded.balance
        {
            return;
        }
        self.telemetry.startup_reported = true;
        let mut crossed = false;
        let mut isolated = false;
        for market in cx.store.markets() {
            match cx.store.setting(&market.symbol).and_then(|s| s.margin_type) {
                Some(MarginType::Cross) => crossed = true,
                Some(MarginType::Isolated) => isolated = true,
                None => {}
            }
        }
        cx.event(
            Level::Info,
            "bitget.account.mode",
            "Bitget startup account modes",
            vec![
                (
                    "accountMode",
                    if self.account.uta { "uta" } else { "classic" }.into(),
                ),
                (
                    "posMode",
                    if self.account.uta || crossed || isolated {
                        if cx.store.hedged() {
                            "hedge"
                        } else {
                            "one_way"
                        }
                    } else {
                        "unknown"
                    }
                    .into(),
                ),
                (
                    "marginMode",
                    match (crossed, isolated) {
                        (true, false) => "crossed",
                        (false, true) => "isolated",
                        (true, true) => "mixed",
                        (false, false) => "unknown",
                    }
                    .into(),
                ),
            ],
        );
    }

    pub(crate) fn remember_close_requests(&mut self, now: f64, requests: &[CloseRequest]) {
        self.telemetry
            .pending
            .retain(|p| now - p.at <= BOOKING_WINDOW_MS);
        for request in requests {
            if request.client.is_empty()
                || !request.submitted.is_finite()
                || request.submitted <= 0.0
            {
                continue;
            }
            // A signature resend of the same request never adds another observer.
            if self
                .telemetry
                .pending
                .iter()
                .any(|p| p.close.client == request.client && p.close.symbol == request.symbol)
            {
                continue;
            }
            if self.telemetry.pending.len() == PENDING_LIMIT {
                self.telemetry.pending.pop_front();
            }
            self.telemetry.pending.push_back(PendingClose {
                close: request.clone(),
                native: None,
                at: now,
            });
        }
    }

    pub(crate) fn bind_close_identity(&mut self, client: &str, native: &str) {
        if client.is_empty() || native.is_empty() {
            return;
        }
        for p in &mut self.telemetry.pending {
            if p.close.client == client {
                p.native = Some(native.to_owned());
            }
        }
    }

    pub(crate) fn close_rejection_event(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        requests: &[CloseRequest],
        code: Option<&str>,
        message: Option<&str>,
        rejected: bool,
    ) {
        let [request] = requests else {
            return self.unattributed_close_failure(cx, code, message);
        };
        self.close_event(
            cx,
            request,
            Diagnostic {
                reason: if rejected { "rejected" } else { "unconfirmed" },
                booked: None,
                code,
                message,
            },
        );
    }

    pub(crate) fn unattributed_close_failure(
        &self,
        cx: &mut Cx<'_, Pending>,
        code: Option<&str>,
        message: Option<&str>,
    ) {
        // An ambiguous batch row must not borrow another request's position or size.
        cx.event(
            Level::Warn,
            "bitget.close.diagnostic",
            "Bitget close order diagnostic",
            vec![
                ("reason", "unconfirmed".into()),
                ("requestAttribution", "ambiguous".into()),
                (
                    "accountMode",
                    if self.account.uta { "uta" } else { "classic" }.into(),
                ),
                ("nativeCode", code.unwrap_or("").into()),
                ("nativeMsg", message.unwrap_or("").into()),
                ("nativeCodePresent", code.is_some().into()),
                ("nativeMsgPresent", message.is_some().into()),
            ],
        );
    }

    pub(crate) fn observe_close_bookings(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        orders: &[Order],
        observed_at: f64,
    ) {
        self.telemetry
            .pending
            .retain(|p| cx.now - p.at <= BOOKING_WINDOW_MS);
        for order in orders {
            let native = order.parent_id.as_deref().unwrap_or(&order.id);
            let Some(index) = self.telemetry.pending.iter().position(|p| {
                p.at <= observed_at
                    && p.close.symbol == order.symbol
                    && (order.client_order_id.as_deref() == Some(p.close.client.as_str())
                        || p.native.as_deref() == Some(native))
            }) else {
                continue;
            };
            if !order.amount.is_finite() || order.amount <= 0.0 {
                continue;
            }
            let pending = self
                .telemetry
                .pending
                .remove(index)
                .expect("matched booking");
            if order.amount != pending.close.submitted {
                self.close_event(
                    cx,
                    &pending.close,
                    Diagnostic {
                        reason: "booked-size-mismatch",
                        booked: Some(order.amount),
                        code: None,
                        message: None,
                    },
                );
            }
        }
    }

    pub(crate) fn observe_framed_close_bookings(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.telemetry.pending.is_empty() {
            return;
        }
        let orders: Vec<_> = cx
            .store
            .orders()
            .iter()
            .filter(|order| {
                self.telemetry.pending.iter().any(|p| {
                    order.last_update >= p.at
                        && p.close.symbol == order.symbol
                        && (order.client_order_id.as_deref() == Some(p.close.client.as_str())
                            || p.native.as_deref()
                                == Some(order.parent_id.as_deref().unwrap_or(&order.id)))
                })
            })
            .cloned()
            .collect();
        self.observe_close_bookings(cx, &orders, cx.now);
    }

    fn close_event(
        &self,
        cx: &mut Cx<'_, Pending>,
        request: &CloseRequest,
        diagnostic: Diagnostic<'_>,
    ) {
        let Diagnostic {
            reason,
            booked,
            code,
            message,
        } = diagnostic;
        let position = cx
            .store
            .positions()
            .iter()
            .find(|p| p.symbol == request.symbol && p.side == request.side);
        let mut reduce_count = 0;
        let mut reduce_size = 0.0;
        let mut plan_count = 0;
        let mut plan_size = 0.0;
        let mut seen = HashSet::new();
        for order in cx.store.orders().iter().filter(|o| {
            o.symbol == request.symbol
                && o.side
                    == match request.side {
                        PositionSide::Long => OrderSide::Sell,
                        PositionSide::Short => OrderSide::Buy,
                    }
                && matches!(o.status, OrderStatus::Open | OrderStatus::Canceling)
        }) {
            let plan = matches!(
                order.kind,
                OrderType::StopLoss | OrderType::TakeProfit | OrderType::TrailingStopLoss
            ) || self.account.plans.contains_key(&order.id);
            let native = order.parent_id.as_deref().unwrap_or(&order.id);
            if !seen.insert((plan, native)) {
                continue;
            }
            let remaining = if self.inverse() {
                order
                    .extra
                    .as_ref()
                    .and_then(|e| e.remaining_native)
                    .unwrap_or(order.remaining)
            } else {
                order.remaining
            };
            if plan {
                plan_count += 1;
                plan_size += remaining;
            } else if order.reduce_only || order.close == Some(true) {
                reduce_count += 1;
                reduce_size += remaining;
            }
        }
        let mut fields: Vec<(&'static str, Value)> = vec![
            ("reason", reason.into()),
            ("requestAttribution", "exact".into()),
            (
                "accountMode",
                if self.account.uta { "uta" } else { "classic" }.into(),
            ),
            ("symbol", request.symbol.clone().into()),
            (
                "side",
                match request.side {
                    PositionSide::Long => "long",
                    PositionSide::Short => "short",
                }
                .into(),
            ),
            ("requestedSize", request.requested.into()),
            ("submittedSize", request.submitted.into()),
            (
                "positionTotal",
                position.map_or(0.0, |p| p.contracts).into(),
            ),
            (
                "quantityUnits",
                if self.inverse() {
                    "inverse-native"
                } else {
                    "base"
                }
                .into(),
            ),
            ("reduceOnlyOrderCount", (reduce_count as f64).into()),
            ("reduceOnlyOrderSize", reduce_size.into()),
            ("planOrderCount", (plan_count as f64).into()),
            ("planOrderSize", plan_size.into()),
            ("nativeCode", code.unwrap_or("").into()),
            ("nativeMsg", message.unwrap_or("").into()),
            ("nativeCodePresent", code.is_some().into()),
            ("nativeMsgPresent", message.is_some().into()),
        ];
        if let Some(booked) = booked {
            fields.push(("bookedSize", booked.into()));
        }
        // PR A's venue-private available/locked receipt will be bound after it lands.
        cx.event(
            Level::Warn,
            "bitget.close.diagnostic",
            "Bitget close order diagnostic",
            fields,
        );
    }
}

/// The diagnostic reads original response values, independently of numeric classification
/// or user-facing wording. Missing codes must never be inferred from a message.
#[derive(Default)]
pub(crate) struct NativeReply {
    pub code: Option<String>,
    pub message: Option<String>,
    pub failures: Vec<NativeFailure>,
}
pub(crate) struct NativeFailure {
    pub code: Option<String>,
    pub message: Option<String>,
    pub client: Option<String>,
}
impl NativeReply {
    pub fn parse(body: &str) -> Self {
        #[derive(serde::Deserialize)]
        struct Head<'a> {
            #[serde(borrow)]
            code: Option<&'a serde_json::value::RawValue>,
            msg: Option<String>,
            #[serde(borrow)]
            data: Option<&'a serde_json::value::RawValue>,
        }
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Failure<'a> {
            #[serde(borrow)]
            error_code: Option<&'a serde_json::value::RawValue>,
            error_msg: Option<String>,
            #[serde(borrow)]
            client_oid: Option<&'a serde_json::value::RawValue>,
        }
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Data<'a> {
            #[serde(borrow, default)]
            failure_list: Vec<Failure<'a>>,
        }
        fn native_code(value: Option<&serde_json::value::RawValue>) -> Option<String> {
            let raw = value?.get();
            serde_json::from_str::<String>(raw).ok().or_else(|| {
                serde_json::from_str::<serde_json::Number>(raw)
                    .ok()
                    .map(|_| raw.to_owned())
            })
        }
        let Ok(head) = serde_json::from_str::<Head>(body) else {
            return Self::default();
        };
        let failures = head
            .data
            .and_then(|d| serde_json::from_str::<Data>(d.get()).ok())
            .map(|d| {
                d.failure_list
                    .into_iter()
                    .map(|f| NativeFailure {
                        code: native_code(f.error_code),
                        message: f.error_msg,
                        client: native_code(f.client_oid).filter(|s| !s.is_empty()),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self {
            code: native_code(head.code),
            message: head.msg,
            failures,
        }
    }
}
