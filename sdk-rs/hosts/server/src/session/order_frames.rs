//! Order envelopes reuse the existing call translation, completion and audit.
use super::*;
use crate::order_contract::{self, CancelQueued, OrderSubmit};

impl Session {
    // Admission and receipts must select the same joined registration. A refused
    // init may refresh fallback provenance without replacing that registration.
    // Pending attempts still receive refusals on their own output.
    pub(super) fn order_out(&mut self, name: &str, instance: Option<&str>) -> Out {
        self.forget_disposed();
        if !self.pending_verifications.contains_key(name)
            && let Some(joined) = self.joined.get(name)
            && joined.out.order_binding(instance)
        {
            return joined.out.clone();
        }
        self.handover_out(name)
    }

    pub(super) fn order_admitted(&mut self, name: &str, instance: Option<&str>) -> bool {
        self.forget_disposed();
        self.order_frames_enabled
            && !self.pending_verifications.contains_key(name)
            && self.joined.get(name).is_some_and(|joined| {
                joined.account.delegated.is_some()
                    && !joined.account.is_public()
                    && order_contract::enabled_for(&joined.account.venue)
                    && joined.account.running.io_allowed()
                    && self.registry.holds(&joined.account, self.socket)
                    && joined.out.order_binding(instance)
            })
            && !self.moved_by_drain()
    }

    pub(super) async fn order_submit(&mut self, mut submit: OrderSubmit) {
        let name = submit.account_name.clone();
        let id = Value::String(submit.intent_id.clone());
        let out = self.order_out(&name, submit.instance_id.as_deref());
        if !out.order_receipt_binding_bounded()
            || name.len() > 256
            || submit.intent_id.len() > order_contract::KEY_MAX
            || submit.instance_id.as_ref().is_some_and(|id| id.len() > 128)
        {
            out.send(envelope::error(
                Some(&name),
                "not sent: invalid order binding",
            ));
            return;
        }
        let refused = if !self.order_admitted(&name, submit.instance_id.as_deref()) {
            Some("not sent: account not admitted for order frames")
        } else if let Err(error) =
            order_contract::check_key(&submit.intent_id, crate::accounts::now_ms())
        {
            Some(error)
        } else if submit.seq == 0
            || submit.seq > 9_007_199_254_740_991
            || !submit.age_ms.is_finite()
            || !(0.0..10_000.0).contains(&submit.age_ms)
        {
            Some("not sent: expired")
        } else {
            match order_contract::fingerprint(&name, &submit.intent_id, &submit.params, submit.seq)
            {
                Ok(value) if value == submit.fingerprint => None,
                _ => Some("not sent: intent fingerprint differs"),
            }
        };
        if let Some(message) = refused {
            refuse(&out, &name, &id, submit.seq, message);
            return;
        }
        let Some(params) = submit.params.as_object_mut() else {
            refuse(&out, &name, &id, submit.seq, "not sent: invalid parameters");
            return;
        };
        if params
            .get("clientOrderId")
            .is_some_and(|value| value.as_str() != Some(&submit.intent_id))
        {
            refuse(
                &out,
                &name,
                &id,
                submit.seq,
                "not sent: client order id differs",
            );
            return;
        }
        params.insert("clientOrderId".into(), submit.intent_id.clone().into());
        out.send(envelope::order_frame(
            "order.ack",
            &name,
            json!({"intentId":submit.intent_id,"seq":submit.seq,"received":true}),
        ));
        self.call_inner(
            &name,
            json!({"requestId":submit.intent_id,
            "method":"placeOrderWithClientOrderId","args":[submit.params],"intentSeq":submit.seq}),
            Some(OrderReceipt::Placement(submit.seq)),
        )
        .await;
    }

    pub(super) fn order_cancel_queued(&mut self, cancel: CancelQueued) {
        let out = self.order_out(&cancel.account_name, cancel.instance_id.as_deref());
        if !out.order_receipt_binding_bounded()
            || cancel.account_name.len() > 256
            || cancel.intent_id.len() > order_contract::KEY_MAX
            || cancel.instance_id.as_ref().is_some_and(|id| id.len() > 128)
        {
            out.send(envelope::error(
                Some(&cancel.account_name),
                "not sent: invalid order binding",
            ));
            return;
        }
        if !self.order_admitted(&cancel.account_name, cancel.instance_id.as_deref()) {
            refuse(
                &out,
                &cancel.account_name,
                &Value::String(cancel.intent_id),
                cancel.seq,
                "not sent: account not admitted for order frames",
            );
            return;
        }
        if order_contract::key_time(&cancel.intent_id).is_none()
            || cancel.seq == 0
            || cancel.seq > 9_007_199_254_740_991
            || cancel.fingerprint.len() != 64
            || !cancel
                .fingerprint
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            refuse(
                &out,
                &cancel.account_name,
                &Value::String(cancel.intent_id),
                cancel.seq,
                "not sent: invalid queued cancellation",
            );
            return;
        }
        out.order_outcome(
            &cancel.account_name,
            json!({"intentId":cancel.intent_id,"seq":cancel.seq,
            "outcome":"unknown","venueOrderId":null,"clientOrderId":null,
            "error":{"message":"Queue cancellation must be resolved by the router"}}),
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn call_failure(
        &self,
        name: &str,
        request_id: &Value,
        kind: &str,
        message: &str,
        outcome: Option<&str>,
        order_frame: Option<OrderReceipt>,
    ) {
        if let Some(OrderReceipt::Action { seq, .. }) = order_frame {
            crate::order_actions::refuse(&self.account_out(name), name, request_id, seq, message);
        } else if let Some(OrderReceipt::Placement(seq)) = order_frame {
            let message: String = message.chars().take(512).collect();
            self.account_out(name).order_outcome(name, json!({"intentId":request_id,"seq":seq,"outcome":"refused",
                "venueOrderId":null,"clientOrderId":request_id,"error":{"message":message,"outcome":"notSent"}}));
        } else {
            self.reply_failure(name, request_id, kind, message, outcome);
        }
    }
}

fn refuse(out: &Out, name: &str, id: &Value, seq: u64, message: &str) {
    out.send(envelope::order_frame(
        "order.ack",
        name,
        json!({"intentId":id,"seq":seq,"received":false}),
    ));
    out.order_outcome(
        name,
        json!({"intentId":id,"seq":seq,"outcome":"refused","venueOrderId":null,"clientOrderId":null,
        "error":{"message":message,"outcome":"notSent"}}),
    );
}

pub(super) fn settle_intent(
    out: &Out,
    name: &str,
    id: &Value,
    client_id: &str,
    seq: u64,
    answer: Result<Value, Value>,
) {
    let mut data = json!({"intentId":id,"seq":seq,"outcome":"unknown","venueOrderId":null,"clientOrderId":client_id,"error":null});
    match answer {
        Ok(value) => {
            if let Some(venue_id) = value
                .as_array()
                .filter(|ids| ids.len() == 1)
                .and_then(|ids| ids[0].as_str())
                .filter(|id| !id.is_empty() && id.len() <= 256)
            {
                data["outcome"] = "placed".into();
                data["venueOrderId"] = venue_id.into();
            } else {
                data["error"] = json!({"message":"Order result is not yet known"});
            }
        }
        Err(error) => {
            data["outcome"] = if error["outcome"] == "unknown" {
                "unknown"
            } else {
                "refused"
            }
            .into();
            let message: String = error["message"]
                .as_str()
                .unwrap_or("Order refused")
                .chars()
                .take(512)
                .collect();
            data["error"] = json!({"message":message});
            if matches!(error["outcome"].as_str(), Some("unknown" | "notSent")) {
                data["error"]["outcome"] = error["outcome"].clone();
            }
        }
    }
    out.order_outcome(name, data);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn order_contract_receipts_are_bounded_and_sequenced_per_account() {
        let (out, mut sent, _) = Out::channel(8);
        let a = out.with_instance_id(Some(&"r".repeat(128)));
        let b = out.with_instance_id(Some(&"r".repeat(128)));
        let id = Value::String("018bcfe5-6800-7101-8101-010101010101".into());
        for (out, name) in [(&a, "a"), (&a, "a"), (&b, "b")] {
            settle_intent(
                out,
                name,
                &id,
                id.as_str().unwrap(),
                1,
                Err(
                    json!({"message":"𐐀".repeat(20_000),"outcome":"unknown","unused":"large".repeat(20_000)}),
                ),
            );
        }
        for expected in [1, 2, 1] {
            let text = sent.recv().await.unwrap();
            assert!(text.len() <= order_contract::OUTCOME_MAX);
            let frame: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(frame["t"], "order.outcome");
            assert_eq!(frame["d"]["outcomeSeq"], expected);
            assert_eq!(frame["d"]["instanceId"], "r".repeat(128));
        }
    }
}
