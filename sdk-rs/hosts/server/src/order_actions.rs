//! Versioned account actions. Placement's four-frame contract is unchanged.
use crate::{envelope, out::Out};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub id: String,
    pub symbol: String,
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
    Amend { order: Target, update: Update },
    Cancel { order: Target },
    CancelAll {},
    Nuke {},
}
impl Action {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Amend { .. } => "amend",
            Self::Cancel { .. } => "cancel",
            Self::CancelAll {} => "cancelAll",
            Self::Nuke {} => "nuke",
        }
    }
    pub fn call(&self) -> Result<(&'static str, Vec<Value>), &'static str> {
        let valid_target = |order: &Target| {
            !order.id.is_empty()
                && order.id.len() <= 256
                && !order.symbol.is_empty()
                && order.symbol.len() <= 256
        };
        match self {
            Self::Amend { order, update } => {
                if !valid_target(order)
                    || (update.price.is_none() && update.amount.is_none())
                    || [update.price, update.amount]
                        .into_iter()
                        .flatten()
                        .any(|v| !v.is_finite() || v <= 0.0)
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
            Self::CancelAll {} => Ok(("cancelAllOrders", vec![])),
            Self::Nuke {} => Ok(("nuke", vec![])),
        }
    }
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
                "amend" => {
                    let ids = result.as_array().or_else(|| result["orderIds"].as_array());
                    ids.filter(|ids| {
                        !ids.is_empty()
                            && ids.len() <= 32
                            && ids.iter().all(|v| {
                                v.as_str()
                                    .is_some_and(|id| !id.is_empty() && id.len() <= 256)
                            })
                    })
                    .map(|ids| json!({"kind":"amend","orderIds":ids}))
                }
                "cancel" | "cancelAll" | "nuke" => Some(json!({"kind":kind})),
                _ => None,
            };
            if let Some(result) = applied {
                data["outcome"] = "applied".into();
                data["result"] = result;
                data["error"] = Value::Null;
            }
        }
        Err(error) => {
            let not_sent = error["outcome"] == "notSent";
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

/// An actor-queued nuke owns its original registration, including during a load
/// wait. Dropping queued/aborted work settles uncertainty on that same output.
pub struct NukeReceipt(Option<(Out, String, Value, u64)>);
impl NukeReceipt {
    pub fn new(out: Out, name: String, id: Value, seq: u64) -> Self {
        Self(Some((out, name, id, seq)))
    }
    pub(crate) fn registration(&self) -> Option<Out> {
        self.0.as_ref().map(|(out, ..)| out.clone())
    }
    pub fn active(&self) -> bool {
        self.0.as_ref().is_some_and(|(out, ..)| out.action_active())
    }
    pub fn finish(mut self, result: Result<Value, Value>) {
        if let Some((out, name, id, seq)) = self.0.take() {
            settle(&out, &name, &id, seq, "nuke", result);
        }
    }
}
impl Drop for NukeReceipt {
    fn drop(&mut self) {
        if let Some((out, name, id, seq)) = self.0.take() {
            settle(
                &out,
                &name,
                &id,
                seq,
                "nuke",
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
    async fn retirement_rejects_a_deferred_nuke_and_drops_its_result() {
        let (out, mut sent, _) = Out::channel(4);
        let old = out.with_instance_id(Some("old"));
        let receipt = NukeReceipt::new(old.clone(), "main".into(), json!("old-intent"), 1);
        assert!(receipt.active());
        old.retire_order_receipts();
        assert!(!receipt.active());
        drop(receipt);
        assert!(sent.try_recv().is_err());
    }
}
