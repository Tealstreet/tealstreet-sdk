//! Versioned account actions. Placement's four-frame contract is unchanged.
use crate::{envelope, out::Out};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub id: String,
    pub symbol: String,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "type")]
    pub kind: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Update {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "update_number",
        serialize_with = "serialize_update"
    )]
    pub price: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "update_number",
        serialize_with = "serialize_update"
    )]
    pub amount: Option<f64>,
}
// Missing fields are optional; an explicitly present null is not a number.
fn update_number<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<f64>, D::Error> {
    f64::deserialize(deserializer).map(Some)
}
fn serialize_update<S: serde::Serializer>(
    number: &Option<f64>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match number {
        Some(number) => crate::order_contract::serialize_age_ms(number, serializer),
        None => serializer.serialize_none(),
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Action {
    Amend {
        order: Target,
        update: Update,
    },
    Cancel {
        order: Target,
    },
    CancelAll {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        symbols: Option<Vec<String>>,
    },
    Nuke {},
    Place {
        #[serde(deserialize_with = "crate::full_writes::unique_json::deserialize_array")]
        orders: Vec<Value>,
    },
    CancelOrders {
        orders: Vec<Target>,
    },
    CancelSymbol {
        symbol: String,
    },
    SetLeverage {
        symbol: String,
        #[serde(serialize_with = "crate::order_contract::serialize_age_ms")]
        leverage: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        side: Option<String>,
    },
    SetAllLeverage {
        #[serde(serialize_with = "crate::order_contract::serialize_age_ms")]
        leverage: f64,
    },
    SetMarginType {
        isolated: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        symbol: Option<String>,
    },
    SetPositionMode {
        hedged: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        symbol: Option<String>,
    },
    Close {
        #[serde(deserialize_with = "crate::full_writes::unique_json::deserialize")]
        order: Value,
    },
    CloseAll {},
    NukeOrders {},
    StopAlgos {},
    CancelAccountOrders {},
    ScheduleCancel {
        #[serde(deserialize_with = "crate::full_writes::unique_json::deserialize")]
        time: Value,
    },
    SetStpMode {
        mode: String,
    },
    SetAccountTier {
        tier: String,
    },
    Algo {
        op: String,
        #[serde(deserialize_with = "crate::full_writes::unique_json::deserialize")]
        data: Value,
    },
}
impl Action {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Amend { .. } => "amend",
            Self::Cancel { .. } => "cancel",
            Self::CancelAll { .. } => "cancelAll",
            Self::Nuke {} => "nuke",
            Self::Place { .. } => "place",
            Self::CancelOrders { .. } => "cancelOrders",
            Self::CancelSymbol { .. } => "cancelSymbol",
            Self::SetLeverage { .. } => "setLeverage",
            Self::SetAllLeverage { .. } => "setAllLeverage",
            Self::SetMarginType { .. } => "setMarginType",
            Self::SetPositionMode { .. } => "setPositionMode",
            Self::Close { .. } => "close",
            Self::CloseAll {} => "closeAll",
            Self::NukeOrders {} => "nukeOrders",
            Self::StopAlgos {} => "stopAlgos",
            Self::CancelAccountOrders {} => "cancelAccountOrders",
            Self::ScheduleCancel { .. } => "scheduleCancel",
            Self::SetStpMode { .. } => "setStpMode",
            Self::SetAccountTier { .. } => "setAccountTier",
            Self::Algo { .. } => "algo",
        }
    }
    pub fn full_write(&self) -> bool {
        match self {
            Self::Amend { update, .. } => update.amount == Some(0.0),
            Self::Cancel { order } => order.kind.is_some(),
            Self::CancelAll { symbols } => symbols.is_some(),
            Self::Nuke {} => false,
            _ => true,
        }
    }
    pub fn call(&self) -> Result<(&'static str, Vec<Value>), &'static str> {
        let valid_target = |order: &Target| {
            !order.id.is_empty()
                && order.id.len() <= 256
                && !order.symbol.is_empty()
                && order.symbol.len() <= 256
                && order.kind.as_ref().is_none_or(|kind| {
                    matches!(
                        kind.as_str(),
                        "market"
                            | "limit"
                            | "conditional"
                            | "stop_market"
                            | "take_profit_market"
                            | "trailing_stop_market"
                    )
                })
        };
        match self {
            Self::Algo { op, data } => {
                crate::full_writes::algo(op, data)?;
                Ok(("delegatedAlgo", vec![json!(op), data.clone()]))
            }
            Self::Amend { order, update } => {
                if !valid_target(order)
                    || order.kind.is_some()
                    || (update.price.is_none() && update.amount.is_none())
                    || update.price.is_some_and(|v| !v.is_finite() || v <= 0.0)
                    || update.amount.is_some_and(|v| !v.is_finite() || v < 0.0)
                {
                    return Err("not sent: invalid amendment");
                }
                Ok((
                    "updateOrder",
                    vec![json!({"order": order, "update": update})],
                ))
            }
            Self::Cancel { order } => {
                if !valid_target(order) {
                    return Err("not sent: invalid cancellation target");
                }
                Ok(("cancelOrders", vec![json!([order])]))
            }
            Self::CancelAll { symbols } => {
                if symbols.as_ref().is_some_and(|rows| {
                    rows.len() > 32 || rows.iter().any(|s| !crate::full_writes::text(s))
                }) {
                    return Err("not sent: invalid cancellation symbols");
                }
                Ok((
                    "cancelAllOrders",
                    symbols.as_ref().map_or_else(Vec::new, |v| vec![json!(v)]),
                ))
            }
            Self::Nuke {} => Ok(("nuke", vec![])),
            Self::Place { orders } => {
                if orders.is_empty()
                    || orders.len() > 32
                    || orders.iter().any(|v| !crate::full_writes::order(v, false))
                {
                    return Err("not sent: invalid placement batch");
                }
                Ok(("placeOrders", vec![json!(orders)]))
            }
            Self::Close { order } => {
                if !crate::full_writes::order(order, true) {
                    return Err("not sent: close must be reduce-only");
                }
                Ok(("placeOrder", vec![order.clone()]))
            }
            Self::CancelOrders { orders } => {
                if orders.is_empty() || orders.len() > 32 || orders.iter().any(|v| !valid_target(v))
                {
                    return Err("not sent: invalid cancellation batch");
                }
                Ok(("cancelOrders", vec![json!(orders)]))
            }
            Self::CancelSymbol { symbol } => {
                if !crate::full_writes::text(symbol) {
                    return Err("not sent: invalid symbol");
                }
                Ok(("cancelSymbolOrders", vec![json!(symbol)]))
            }
            Self::SetLeverage {
                symbol,
                leverage,
                side,
            } => {
                if !crate::full_writes::text(symbol)
                    || !leverage.is_finite()
                    || *leverage <= 0.0
                    || side
                        .as_ref()
                        .is_some_and(|s| !matches!(s.as_str(), "long" | "short"))
                {
                    return Err("not sent: invalid leverage");
                }
                Ok((
                    "setLeverage",
                    vec![json!(symbol), json!(leverage), json!(side)],
                ))
            }
            Self::SetAllLeverage { leverage } => {
                if !leverage.is_finite() || *leverage <= 0.0 {
                    return Err("not sent: invalid leverage");
                }
                Ok(("setAllLeverage", vec![json!(leverage)]))
            }
            Self::SetMarginType { isolated, symbol } => {
                if symbol
                    .as_ref()
                    .is_some_and(|s| !crate::full_writes::text(s))
                {
                    return Err("not sent: invalid symbol");
                }
                Ok(("changeMarginType", vec![json!(isolated), json!(symbol)]))
            }
            Self::SetPositionMode { hedged, symbol } => {
                if symbol
                    .as_ref()
                    .is_some_and(|s| !crate::full_writes::text(s))
                {
                    return Err("not sent: invalid symbol");
                }
                Ok(("changePositionMode", vec![json!(hedged), json!(symbol)]))
            }
            Self::CloseAll {} => Ok(("nukePositions", vec![])),
            Self::NukeOrders {} => Ok(("nukeOrders", vec![])),
            Self::StopAlgos {} => Ok(("stopAccountAlgos", vec![])),
            Self::CancelAccountOrders {} => Ok(("delegatedCancelAccountOrders", vec![])),
            Self::ScheduleCancel { time } => {
                if !time.is_null()
                    && !time.as_f64().is_some_and(|v| {
                        v.is_finite()
                            && (0.0..=9_007_199_254_740_991.0).contains(&v)
                            && v.fract() == 0.0
                    })
                {
                    return Err("not sent: invalid scheduled cancel time");
                }
                Ok(("scheduleCancel", vec![time.clone()]))
            }
            Self::SetStpMode { mode } => {
                if !matches!(
                    mode.as_str(),
                    "EXPIRE_TAKER" | "EXPIRE_MAKER" | "EXPIRE_BOTH"
                ) {
                    return Err("not sent: invalid STP mode");
                }
                Ok(("delegatedSetStpMode", vec![json!(mode)]))
            }
            Self::SetAccountTier { tier } => {
                if !matches!(tier.as_str(), "standard" | "premium" | "plus") {
                    return Err("not sent: invalid account tier");
                }
                Ok(("delegatedSetAccountTier", vec![json!(tier)]))
            }
        }
    }
}

/// Decode only routing/binding metadata first. Unsupported payloads remain raw
/// until their identity has been checked; unknown fields are never discarded
/// on a path that can execute a native write.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireRequest {
    pub version: u8,
    pub account_name: String,
    pub instance_id: Option<String>,
    pub intent_id: String,
    pub seq: u64,
    #[serde(serialize_with = "crate::order_contract::serialize_age_ms")]
    pub age_ms: f64,
    pub fingerprint: String,
    pub action: Value,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub version: u8,
    pub account_name: String,
    pub instance_id: Option<String>,
    pub intent_id: String,
    pub seq: u64,
    #[serde(serialize_with = "crate::order_contract::serialize_age_ms")]
    pub age_ms: f64,
    pub fingerprint: String,
    pub action: Action,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CancelQueued {
    pub version: u8,
    pub account_name: String,
    pub instance_id: Option<String>,
    pub intent_id: String,
    pub seq: u64,
    pub fingerprint: String,
}

pub fn acknowledge(out: &Out, name: &str, id: &Value, seq: u64, received: bool) {
    out.send(envelope::order_frame(
        "order.actionAck",
        name,
        json!({"version":1,"intentId":id,"seq":seq,"received":received}),
    ));
}
pub fn refuse(out: &Out, name: &str, id: &Value, seq: u64, message: &str) {
    acknowledge(out, name, id, seq, false);
    settle(
        out,
        name,
        id,
        seq,
        "",
        Err(json!({"message":message,"outcome":"notSent"})),
    );
}

/// Decode the original text strictly after admission/hash validation. Never
/// execute a schema normalized through Value (which collapses duplicate tags).
pub(crate) fn decode_request(text: &str) -> Result<Request, serde_json::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Frame {
        #[serde(rename = "t")]
        _kind: String,
        d: Request,
    }
    serde_json::from_str::<Frame>(text).map(|frame| frame.d)
}

/// Schema rejection is terminal but never fabricates replay/no-handoff brands.
/// Only the bound, authenticated pre-dispatch path may emit this outcome.
pub fn unsupported(out: &Out, name: &str, id: &Value, seq: u64) {
    out.action_outcome(
        name,
        json!({"version":1,"intentId":id,"seq":seq,
        "outcome":"unsupported","result":null,
        "error":{"message":"Unsupported v1 action schema","outcome":"unsupported"}}),
    );
}

pub fn settle(
    out: &Out,
    name: &str,
    id: &Value,
    seq: u64,
    kind: &str,
    answer: Result<Value, Value>,
) {
    let mut data = json!({"version":1,"intentId":id,"seq":seq,"outcome":"unknown","result":null,
        "error":{"message":"Action result is not yet known","outcome":"unknown"}});
    match answer {
        Ok(result) => {
            let applied = match kind {
                "place"
                    if result["protectionApplied"] == true
                        && result["orderIds"].as_array().is_some_and(Vec::is_empty) =>
                {
                    Some(json!({"kind":"place","orderIds":[],"protectionApplied":true}))
                }
                "amend" | "place" | "close" => {
                    let ids = result.as_array().or_else(|| result["orderIds"].as_array());
                    ids.filter(|ids| {
                        !ids.is_empty()
                            && ids.len() <= 32
                            && ids.iter().all(|v| {
                                v.as_str()
                                    .is_some_and(|id| !id.is_empty() && id.len() <= 256)
                            })
                    })
                    .map(|ids| json!({"kind":kind,"orderIds":ids}))
                }
                "cancel" | "cancelAll" | "nuke" | "closeAll" | "nukeOrders" | "stopAlgos" => {
                    Some(json!({"kind":kind}))
                }
                "cancelOrders"
                | "cancelSymbol"
                | "cancelAccountOrders"
                | "setLeverage"
                | "setAllLeverage"
                | "setMarginType"
                | "setPositionMode"
                | "scheduleCancel"
                | "setStpMode"
                | "setAccountTier"
                    if result.is_null() =>
                {
                    Some(json!({"kind":kind}))
                }
                _ => None,
            };
            if let Some(result) = applied {
                data["outcome"] = "applied".into();
                data["result"] = result;
                data["error"] = Value::Null;
            }
        }
        Err(error) => {
            // A later unsent leg cannot erase earlier native effects in a batch.
            let partial = error["partialOrderIds"]
                .as_array()
                .is_some_and(|v| !v.is_empty())
                || error["partialOrderCount"].as_u64().is_some_and(|v| v > 0)
                || error["elements"].as_array().is_some_and(|v| !v.is_empty());
            let not_sent = error["outcome"] == "notSent" && !partial;
            data["outcome"] = if not_sent { "refused" } else { "unknown" }.into();
            let message: String = error["message"]
                .as_str()
                .unwrap_or("Action result is not yet known")
                .chars()
                .take(512)
                .collect();
            data["error"] =
                json!({"message":message,"outcome":if not_sent {"notSent"} else {"unknown"}});
        }
    }
    out.action_outcome(name, data);
}

/// Capture both expiry bounds at arrival. Neither actor queues nor private-load
/// waits may reset the budget. The UUID's remaining lifetime also bounds the
/// monotonic deadline, so a wall-clock rollback cannot extend it.
#[derive(Clone, Copy)]
pub(crate) struct ActionDeadline {
    monotonic: std::time::Instant,
    key_expires_at_ms: u64,
}
impl ActionDeadline {
    pub(crate) fn new(id: &str, age_ms: f64, arrived: std::time::Instant) -> Option<Self> {
        if !age_ms.is_finite() || !(0.0..10_000.0).contains(&age_ms) {
            return None;
        }
        let created = crate::order_contract::key_time(id)?;
        let key_age = crate::accounts::now_ms().checked_sub(created)?;
        let key_remaining = 10_000_u64.checked_sub(key_age).filter(|ms| *ms > 0)?;
        let remaining = std::time::Duration::from_secs_f64((10_000.0 - age_ms) / 1000.0)
            .min(std::time::Duration::from_millis(key_remaining));
        Some(Self {
            monotonic: arrived + remaining,
            key_expires_at_ms: created + 10_000,
        })
    }
    pub(crate) fn expired(self) -> bool {
        std::time::Instant::now() >= self.monotonic
            || crate::accounts::now_ms() >= self.key_expires_at_ms
    }
}

#[derive(Clone)]
pub(crate) struct ActionGuard {
    registration: Out,
    deadline: ActionDeadline,
    // Another holder's live account token cannot renew this socket's authority.
    token_expires_at_ms: Option<u64>,
}
impl ActionGuard {
    pub(crate) fn new(
        registration: Out,
        deadline: ActionDeadline,
        token_expires_at_ms: Option<u64>,
    ) -> Self {
        Self {
            registration,
            deadline,
            token_expires_at_ms,
        }
    }
    pub(crate) fn check(&self) -> Result<(), &'static str> {
        crate::auth::Verifier::check_expiry(self.token_expires_at_ms, crate::accounts::now_ms())?;
        if self.deadline.expired() {
            Err("Action expired; no further action command was sent")
        } else if !self.registration.action_active() {
            Err("Action registration retired; no further action command was sent")
        } else {
            Ok(())
        }
    }
}

pub(crate) fn expired_outcome() -> Value {
    // Expiry is a dispatch fence, not whole-intent evidence of non-forwarding.
    json!({"message":"Action expired; check orders and positions","outcome":"unknown"})
}

/// An actor-queued nuke owns its original registration and expiry, including
/// during load waits. Dropped work settles uncertainty on that same output.
pub struct NukeReceipt {
    guard: ActionGuard,
    reply: Option<(String, Value, u64)>,
    kind: &'static str,
}
impl NukeReceipt {
    pub(crate) fn new(
        out: Out,
        name: String,
        id: Value,
        seq: u64,
        deadline: ActionDeadline,
        token_expires_at_ms: Option<u64>,
    ) -> Self {
        Self {
            guard: ActionGuard::new(out, deadline, token_expires_at_ms),
            reply: Some((name, id, seq)),
            kind: "nuke",
        }
    }
    pub(crate) fn with_kind(mut self, kind: &'static str) -> Self {
        self.kind = kind;
        self
    }
    pub(crate) fn guard(&self) -> ActionGuard {
        self.guard.clone()
    }
    pub(crate) fn expired(&self) -> bool {
        self.guard.deadline.expired()
    }
    pub fn active(&self) -> bool {
        self.reply.is_some() && self.guard.registration.action_active()
    }
    pub fn finish(mut self, result: Result<Value, Value>) {
        if let Some((name, id, seq)) = self.reply.take() {
            settle(&self.guard.registration, &name, &id, seq, self.kind, result);
        }
    }
}
impl Drop for NukeReceipt {
    fn drop(&mut self) {
        if let Some((name, id, seq)) = self.reply.take() {
            settle(
                &self.guard.registration,
                &name,
                &id,
                seq,
                self.kind,
                Err(
                    json!({"message":"Action interrupted; check orders and positions","outcome":"unknown"}),
                ),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn amendment_rejects_unused_target_routing_type() {
        let action: Action = serde_json::from_value(json!({
            "kind":"amend", "order":{"id":"resting","symbol":"BTCUSDT","type":"stop_market"},
            "update":{"price":49000}
        }))
        .unwrap();
        assert!(action.call().is_err());
    }
    #[test]
    fn full_write_contract_accepts_each_local_write_and_preserves_exact_payload() {
        let cases: Vec<Value> =
            serde_json::from_str(include_str!("../tests/data/delegated-full-writes-v1.json"))
                .unwrap();
        for case in cases {
            let action: Action = serde_json::from_value(case["action"].clone())
                .unwrap_or_else(|e| panic!("{}: {e}", case["action"]["kind"]));
            assert!(action.full_write());
            assert_eq!(serde_json::to_value(&action).unwrap(), case["action"]);
            assert_eq!(
                crate::order_contract::action_fingerprint(
                    "main",
                    "018bcfe5-6800-7101-8101-010101010101",
                    &case["action"],
                    1
                )
                .unwrap(),
                case["fingerprint"]
            );
            assert_eq!(action.call().unwrap().0, case["method"].as_str().unwrap());
        }
    }
    #[tokio::test]
    async fn registered_action_result_reserves_binding_expansion_before_size_check() {
        let (socket, mut sent, _) = Out::channel(4);
        let result = json!(vec!["x".repeat(240); 32]);
        settle(
            &socket,
            "main",
            &json!("intent"),
            1,
            "amend",
            Ok(result.clone()),
        );
        let unregistered = sent.recv().await.unwrap();
        assert!(unregistered.len() <= crate::order_contract::OUTCOME_MAX);
        let control: Value = serde_json::from_str(&unregistered).unwrap();
        assert_eq!(control["d"]["outcome"], "applied");
        // A bounded 128-byte ID can expand to 768 bytes in JSON.
        let registration = "\u{001f}".repeat(128);
        let bound = socket.with_instance_id(Some(&registration));
        settle(&bound, "main", &json!("intent"), 1, "amend", Ok(result));
        let registered = sent.recv().await.unwrap();
        assert!(registered.len() <= crate::order_contract::OUTCOME_MAX);
        let frame: Value = serde_json::from_str(&registered).unwrap();
        assert_eq!(frame["d"]["instanceId"], registration);
        assert_eq!(frame["d"]["outcome"], "unknown");
        assert_eq!(frame["d"]["error"]["outcome"], "unknown");
        assert!(frame["d"]["result"].is_null());
    }
    #[test]
    fn nuke_deadline_guards_both_monotonic_and_uuid_expiry() {
        use std::time::{Duration, Instant};
        let future = Instant::now() + Duration::from_secs(10);
        let wall = crate::accounts::now_ms();
        assert!(
            !ActionDeadline {
                monotonic: future,
                key_expires_at_ms: wall + 10_000
            }
            .expired()
        );
        // A wall-clock advance expires a key even with monotonic budget left.
        assert!(
            ActionDeadline {
                monotonic: future,
                key_expires_at_ms: wall - 1
            }
            .expired()
        );
        // A wall-clock rollback cannot revive an elapsed monotonic deadline.
        assert!(
            ActionDeadline {
                monotonic: Instant::now() - Duration::from_millis(1),
                key_expires_at_ms: wall + 10_000
            }
            .expired()
        );
    }
    #[tokio::test]
    async fn typed_action_results_preserve_uncertainty_and_not_sent() {
        for (kind, answer, expected) in [
            ("amend", Ok(json!(["changed"])), "applied"),
            ("amend", Ok(json!([])), "unknown"),
            (
                "cancel",
                Err(json!({"message":"connection lost"})),
                "unknown",
            ),
            (
                "cancelAll",
                Err(json!({"message":"ambiguous","outcome":"unknown"})),
                "unknown",
            ),
            (
                "nuke",
                Err(json!({"message":"not admitted","outcome":"notSent"})),
                "refused",
            ),
        ] {
            let (out, mut sent, _) = Out::channel(4);
            settle(&out, "main", &json!("intent"), 1, kind, answer);
            let frame: Value = serde_json::from_str(&sent.recv().await.unwrap()).unwrap();
            assert_eq!(frame["t"], "order.actionOutcome");
            assert_eq!(frame["d"]["outcome"], expected);
            if expected == "unknown" {
                assert_eq!(frame["d"]["error"]["outcome"], "unknown");
                assert!(frame["d"]["result"].is_null());
            }
        }
    }
    #[tokio::test]
    async fn full_write_results_require_completion_and_preserve_partial_uncertainty() {
        for (kind, answer, expected) in [
            ("setLeverage", Ok(Value::Null), "applied"),
            ("setLeverage", Ok(json!(false)), "unknown"),
            ("place", Ok(json!([])), "unknown"),
            ("close", Ok(json!(["native-close"])), "applied"),
            (
                "place",
                Err(json!({"outcome":"notSent","partialOrderIds":["native-first"]})),
                "unknown",
            ),
            (
                "cancelOrders",
                Err(json!({"outcome":"notSent","elements":[{"effect":"cancelled"}]})),
                "unknown",
            ),
            (
                "setMarginType",
                Err(json!({"outcome":"notSent","message":"unsupported"})),
                "refused",
            ),
            ("algo", Ok(Value::Null), "unknown"),
        ] {
            let (out, mut sent, _) = Out::channel(4);
            settle(&out, "main", &json!("intent"), 1, kind, answer);
            let frame: Value = serde_json::from_str(&sent.recv().await.unwrap()).unwrap();
            assert_eq!(frame["d"]["outcome"], expected, "{kind}: {frame}");
        }
    }

    #[tokio::test]
    async fn retirement_rejects_a_deferred_nuke_and_drops_its_result() {
        let (out, mut sent, _) = Out::channel(4);
        let old = out.with_instance_id(Some("old"));
        let receipt = NukeReceipt::new(
            old.clone(),
            "main".into(),
            json!("old-intent"),
            1,
            ActionDeadline {
                monotonic: std::time::Instant::now() + std::time::Duration::from_secs(10),
                key_expires_at_ms: crate::accounts::now_ms() + 10_000,
            },
            None,
        );
        assert!(receipt.active());
        old.retire_order_receipts();
        assert!(!receipt.active());
        drop(receipt);
        assert!(sent.try_recv().is_err());
    }
}
