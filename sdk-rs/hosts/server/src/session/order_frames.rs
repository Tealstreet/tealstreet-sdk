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

    pub(super) fn audit_legacy_order_refusal(&self, msg: &AccountMsg, reason: &str) {
        if msg.op == "callMethod"
            && let Some(method) = msg.data["method"]
                .as_str()
                .filter(|method| crate::audit::ORDER_METHODS.contains(method))
        {
            self.audit_order_refusal(msg.account_name.as_deref().unwrap_or(""), method, reason);
        }
    }

    pub(super) fn audit_order_refusal(&self, name: &str, op: &str, message: &str) {
        crate::audit::pre_dispatch_refused(
            self.joined.get(name).map(|j| j.account.venue.as_str()),
            op,
            message,
        );
    }

    pub(super) fn order_admission_refusal(
        &self,
        name: &str,
        instance: Option<&str>,
    ) -> Option<&'static str> {
        if !self.order_frames_enabled {
            return Some("runtime_disabled");
        }
        if self.pending_verifications.contains_key(name) {
            return Some("pending_verification");
        }
        let Some(joined) = self.joined.get(name) else {
            return Some("account_missing");
        };
        if joined.account.delegated().is_none() {
            return Some("wrong_signing_mode");
        }
        if joined.account.is_public() {
            return Some("public_account");
        }
        if !order_contract::enabled_for(&joined.account.venue) {
            return Some("venue_disabled");
        }
        if !joined.account.running.io_allowed() {
            return Some("io_fenced");
        }
        let label = joined.out.account_label(&joined.account.name);
        if !self
            .registry
            .holds_label(&joined.account, self.socket, label)
        {
            return Some("holder_changed");
        }
        if !joined.out.order_binding(instance) {
            return Some("registration_mismatch");
        }
        self.moved_by_drain().then_some("draining")
    }

    pub(super) fn order_admitted(&mut self, name: &str, instance: Option<&str>) -> bool {
        self.forget_disposed();
        self.order_admission_refusal(name, instance).is_none()
    }

    // Expiry refuses this new command; queue cancellation cannot prove the
    // earlier business intent was unsent and keeps its existing handling.
    pub(super) fn refuse_expired_order(
        &mut self,
        name: &str,
        instance: Option<&str>,
        id: &str,
        seq: u64,
        action: bool,
    ) {
        let out = self.order_out(name, instance);
        if !out.order_receipt_binding_bounded()
            || name.len() > 256
            || id.len() > order_contract::KEY_MAX
            || instance.is_some_and(|value| value.len() > 128)
        {
            self.audit_order_refusal(
                name,
                if action {
                    "order.action"
                } else {
                    "order.submit"
                },
                crate::auth::TOKEN_EXPIRED,
            );
            out.send(envelope::error(Some(name), crate::auth::TOKEN_EXPIRED));
            return;
        }
        if let Some(joined) = self.joined.get(name) {
            audit_preflight(
                &joined.account,
                self.socket,
                if action {
                    "order.action"
                } else {
                    "order.submit"
                },
                &[],
                crate::auth::TOKEN_EXPIRED,
            );
        } else {
            self.audit_order_refusal(
                name,
                if action {
                    "order.action"
                } else {
                    "order.submit"
                },
                crate::auth::TOKEN_EXPIRED,
            );
        }
        let id = Value::String(id.to_owned());
        if action {
            crate::order_actions::refuse(&out, name, &id, seq, crate::auth::TOKEN_EXPIRED);
        } else {
            refuse(&out, name, &id, seq, crate::auth::TOKEN_EXPIRED);
        }
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
            self.audit_order_refusal(&name, "order.submit", "not sent: invalid order binding");
            out.send(envelope::error(
                Some(&name),
                "not sent: invalid order binding",
            ));
            return;
        }
        let refused = if !self.order_admitted(&name, submit.instance_id.as_deref()) {
            Some("Order submissions aren’t available on this account connection")
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
            let reason =
                if message == "Order submissions aren’t available on this account connection" {
                    self.order_admission_refusal(&name, submit.instance_id.as_deref())
                        .unwrap_or(message)
                } else {
                    message
                };
            self.audit_order_refusal(&name, "order.submit", reason);
            refuse(&out, &name, &id, submit.seq, message);
            return;
        }
        let Some(params) = submit.params.as_object_mut() else {
            self.audit_order_refusal(&name, "order.submit", "not sent: invalid parameters");
            refuse(&out, &name, &id, submit.seq, "not sent: invalid parameters");
            return;
        };
        if params
            .get("clientOrderId")
            .is_some_and(|value| value.as_str() != Some(&submit.intent_id))
        {
            self.audit_order_refusal(&name, "order.submit", "not sent: client order id differs");
            refuse(
                &out,
                &name,
                &id,
                submit.seq,
                "not sent: client order id differs",
            );
            return;
        }
        let venue = &self.joined[&name].account.venue;
        // Bybit protection and shape-refusing adapters (`trigger_refuses_caller_id`) omit
        // caller IDs; other orders keep the venue's lossless UUID form, or omit it.
        let client_id = if order_contract::bybit_protection(venue, params)
            || order_contract::trigger_refuses_caller_id(venue, params)
        {
            None
        } else {
            order_contract::client_order_id(venue, &submit.intent_id)
        };
        let caller_id = client_id.is_some();
        let method = if let Some(id) = client_id {
            params.insert("clientOrderId".into(), id.into());
            "placeOrderWithClientOrderId"
        } else {
            params.remove("clientOrderId");
            "placeOrder"
        };
        out.send(envelope::order_frame(
            "order.ack",
            &name,
            json!({"intentId":submit.intent_id,"seq":submit.seq,"received":true}),
        ));
        self.call_inner(
            &name,
            json!({"requestId":submit.intent_id,
            "method":method,"args":[submit.params],"intentSeq":submit.seq}),
            Some(OrderReceipt::Placement {
                seq: submit.seq,
                caller_id,
            }),
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
            self.audit_order_refusal(
                &cancel.account_name,
                "order.cancelQueued",
                "not sent: invalid order binding",
            );
            out.send(envelope::error(
                Some(&cancel.account_name),
                "not sent: invalid order binding",
            ));
            return;
        }
        if !self.order_admitted(&cancel.account_name, cancel.instance_id.as_deref()) {
            let reason = self
                .order_admission_refusal(&cancel.account_name, cancel.instance_id.as_deref())
                .unwrap_or("Order submissions aren’t available on this account connection");
            self.audit_order_refusal(&cancel.account_name, "order.cancelQueued", reason);
            refuse(
                &out,
                &cancel.account_name,
                &Value::String(cancel.intent_id),
                cancel.seq,
                "Order submissions aren’t available on this account connection",
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
            self.audit_order_refusal(
                &cancel.account_name,
                "order.cancelQueued",
                "not sent: invalid queued cancellation",
            );
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
            "error":{"message":"The queued order couldn’t be cancelled here. Cancel it from your pending orders."}}),
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
        if let Some(receipt) = order_frame {
            let op = match receipt {
                OrderReceipt::Action { kind, .. } => kind,
                OrderReceipt::Placement { .. } => "order.submit",
            };
            self.audit_order_refusal(name, op, message);
        }
        if let Some(OrderReceipt::Action { seq, .. }) = order_frame {
            crate::order_actions::refuse(&self.account_out(name), name, request_id, seq, message);
        } else if let Some(OrderReceipt::Placement { seq, caller_id }) = order_frame {
            let message: String = message.chars().take(512).collect();
            let venue = self
                .joined
                .get(name)
                .map_or("", |joined| joined.account.venue.as_str());
            // An omitted caller ID was never sent, so the refusal names none (AGT-110).
            let client_id = request_id
                .as_str()
                .filter(|_| caller_id)
                .and_then(|id| order_contract::client_order_id(venue, id));
            self.account_out(name).order_outcome(name, json!({"intentId":request_id,"seq":seq,"outcome":"refused",
                "venueOrderId":null,"clientOrderId":client_id,"error":{"message":message,"outcome":"notSent"}}));
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
    client_id: Option<&str>,
    seq: u64,
    protective_legs: usize,
    answer: Result<Value, Value>,
) -> bool {
    let mut data = json!({"intentId":id,"seq":seq,"outcome":"unknown","venueOrderId":null,"clientOrderId":client_id,"error":null});
    match answer {
        Ok(value) => {
            // One native order, or one entry followed by exactly its separately placed
            // bracket legs (AGT-111). Any other count, such as a split entry, stays unknown;
            // so does a lone ID where legs were expected (AGT-112: a missing leg).
            let valid = |id: &Value| {
                id.as_str()
                    .is_some_and(|id| !id.is_empty() && id.len() <= 256)
            };
            if let Some(venue_id) = value
                .as_array()
                .filter(|ids| ids.len() == 1 + protective_legs)
                .filter(|ids| ids.iter().all(valid))
                .and_then(|ids| ids[0].as_str())
            {
                data["outcome"] = "placed".into();
                data["venueOrderId"] = venue_id.into();
            } else {
                data["error"] = json!({"message":"We couldn’t confirm this order. Check your open orders before trying again."});
            }
        }
        Err(error) => {
            // Only a whole-order proof leaves `unknown`: the engine's notSent, or its
            // `rejected` (every released leg definitively refused by the venue).
            // Any accepted leg, element or protective side effect stays unknown.
            let whole = !partial_effects(&error);
            let outcome = match error["outcome"].as_str() {
                Some(proof @ ("notSent" | "rejected")) if whole => proof,
                _ => "unknown",
            };
            data["outcome"] = if outcome == "unknown" {
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
            data["error"] = json!({"message":message,"outcome":outcome});
            // The receipt stays unknown; the legs' results only let the client say which landed.
            if outcome == "unknown"
                && let Some(elements) = leg_results(&error["elements"])
            {
                data["error"]["elements"] = elements;
            }
        }
    }
    out.order_outcome(name, data)
}

/// The engine's `write_plan::LegRole` wire names.
const LEG_ROLES: [&str; 4] = ["entry", "takeProfit", "stopLoss", "other"];

/// Each leg's result, projected to the fields the client reads, or `None` when any
/// leg is unreadable or the list would not fit inside the receipt limit (AGT-185).
fn leg_results(elements: &Value) -> Option<Value> {
    const LEGS_MAX: usize = 32;
    const LEGS_BYTES_MAX: usize = 4 * 1024;
    const REASON_CHARS_MAX: usize = 160;
    let bounded_id = |id: &Value| {
        id.as_str()
            .is_some_and(|id| !id.is_empty() && id.len() <= 256)
    };
    let legs = elements
        .as_array()
        .filter(|legs| !legs.is_empty() && legs.len() <= LEGS_MAX)?;
    let projected = legs
        .iter()
        .map(|leg| {
            let index = leg["index"].as_u64().filter(|index| *index <= 9_007_199_254_740_991)?;
            let mut out = json!({ "index": index });
            if let Some(id) = leg.get("clientOrderId") {
                out["clientOrderId"] = Some(id).filter(|id| bounded_id(id))?.clone();
            }
            // Display-only (AGT-214): an unrecognised role is left out, never the leg.
            if let Some(role) = leg["role"]
                .as_str()
                .filter(|role| LEG_ROLES.contains(role))
            {
                out["role"] = role.into();
            }
            let reason = || {
                leg["error"]["message"]
                    .as_str()
                    .map(|reason| json!({ "message": reason.chars().take(REASON_CHARS_MAX).collect::<String>() }))
            };
            match leg["status"].as_str()? {
                "placed" => {
                    let ids = leg["orderIds"]
                        .as_array()
                        .filter(|ids| ids.len() <= LEGS_MAX && ids.iter().all(bounded_id))?;
                    out["orderIds"] = Value::Array(ids.clone());
                }
                "applied" => {}
                "notSent" => {
                    if let Some(reason) = reason() {
                        out["error"] = reason;
                    }
                }
                "unknown" => out["error"] = reason()?,
                _ => return None,
            }
            out["status"] = leg["status"].clone();
            Some(out)
        })
        .collect::<Option<Vec<_>>>()?;
    let fits = |legs: &Vec<Value>| Value::from(legs.as_slice()).to_string().len() <= LEGS_BYTES_MAX;
    if fits(&projected) {
        return Some(Value::Array(projected));
    }
    // Roles cost ~20 bytes a leg (AGT-214): shed them before the list that fit without them.
    let roleless = strip_roles(projected);
    fits(&roleless).then_some(Value::Array(roleless))
}

/// The same legs without their display-only roles.
pub(crate) fn strip_roles(mut legs: Vec<Value>) -> Vec<Value> {
    for leg in &mut legs {
        if let Some(leg) = leg.as_object_mut() {
            leg.remove("role");
        }
    }
    legs
}

/// Native evidence that some part of the command took effect at the venue.
fn partial_effects(error: &Value) -> bool {
    error["partial"].is_object()
        || error["partialOrderIds"]
            .as_array()
            .is_some_and(|ids| !ids.is_empty())
        || error["partialOrderCount"].as_u64().is_some_and(|n| n > 0)
        || error["protection"].is_string()
        || error["elements"].as_array().is_some_and(|elements| {
            elements
                .iter()
                .any(|element| element["status"] != "notSent")
        })
}

// Rule 5 (Sam, 2026-11-20): display wording only; outcome and no-retry checks stay.
#[cfg(test)]
mod tests {
    use super::*;

    async fn settled(error: Value) -> Value {
        let (out, mut sent, _) = Out::channel(1);
        let id = Value::String("018bcfe5-6800-7101-8101-010101010101".into());
        settle_intent(&out, "a", &id, None, 1, 0, Err(error));
        let frame: Value = serde_json::from_str(&sent.recv().await.unwrap()).unwrap();
        frame["d"].clone()
    }

    async fn settled_ok(ids: Value, protective_legs: usize) -> Value {
        let (out, mut sent, _) = Out::channel(1);
        let id = Value::String("018bcfe5-6800-7101-8101-010101010101".into());
        settle_intent(&out, "a", &id, None, 1, protective_legs, Ok(ids));
        let frame: Value = serde_json::from_str(&sent.recv().await.unwrap()).unwrap();
        frame["d"].clone()
    }

    // AGT-111 live, okx-t: a limit with an attached stopLoss completed with
    // `[entry, stop-loss algo]` and was reported unknown although both were placed.
    #[tokio::test]
    async fn a_bracket_entry_followed_by_exactly_its_legs_is_placed_as_the_entry() {
        for (ids, legs) in [
            (json!(["entry", "sl"]), 1),
            (json!(["entry", "sl", "tp"]), 2),
        ] {
            let placed = settled_ok(ids, legs).await;
            assert_eq!(placed["outcome"], "placed", "{placed}");
            assert_eq!(placed["venueOrderId"], "entry");
            assert_eq!(placed["error"], Value::Null);
        }
        for (ids, legs) in [
            (json!(["a", "b"]), 0),
            (json!(["lot-1", "lot-2", "sl"]), 1),
            (json!(["entry", "sl", "tp"]), 1),
            // AGT-112: an entry without the leg it requested is not a whole placement.
            (json!(["entry"]), 1),
            (json!(["entry"]), 2),
            (json!(["entry", "sl"]), 2),
            (json!(["entry", ""]), 1),
            (json!([]), 1),
            (json!([]), 0),
        ] {
            let unknown = settled_ok(ids.clone(), legs).await;
            assert_eq!(
                unknown["outcome"], "unknown",
                "{ids} legs {legs}: {unknown}"
            );
            assert_eq!(unknown["venueOrderId"], Value::Null);
        }
        assert_eq!(settled_ok(json!(["only"]), 0).await["outcome"], "placed");
    }

    #[tokio::test]
    async fn only_a_whole_order_venue_rejection_reaches_the_client_as_rejected() {
        let reason = "Bybit inverse requires enabling the settlement asset as collateral";
        let rejected = settled(json!({"name":"ExchangeError","message":reason,
            "kind":"rejected","outcome":"rejected","venueCode":"110007"}))
        .await;
        assert_eq!(rejected["outcome"], "refused");
        assert_eq!(rejected["venueOrderId"], Value::Null);
        assert_eq!(
            rejected["error"],
            json!({"message":reason,"outcome":"rejected"})
        );
        // A single refused element is still the whole order.
        let element = settled(json!({"message":reason,"outcome":"rejected",
            "elements":[{"index":0,"status":"notSent","error":{"message":reason,"outcome":"rejected"}}]}))
        .await;
        assert_eq!(element["error"]["outcome"], "rejected");

        let ambiguous = [
            // A split/multi-leg placement where an earlier leg was accepted.
            json!({"message":"leg 2 rejected","outcome":"rejected",
                "partial":{"orderIds":["a"],"total":2,"failedIndex":1}}),
            json!({"message":"leg 2 rejected","outcome":"rejected","partialOrderIds":["a"],"partialOrderCount":1}),
            json!({"message":"leg 2 rejected","outcome":"rejected","elements":[
                {"index":0,"status":"placed","orderIds":["a"]},
                {"index":1,"status":"notSent","error":{"message":"leg 2 rejected","outcome":"rejected"}}]}),
            json!({"message":"replacement refused","outcome":"rejected","protection":"unprotected"}),
            // Accepted legs plus known refusals: the engine leaves outcome unset.
            json!({"message":"leg 2 rejected","partial":{"orderIds":["a"]}}),
            json!({"message":"timed out","kind":"timeout","outcome":"unknown"}),
            json!({"name":"AuthExpired","message":"token expired"}),
            json!({"message":"venue said no"}),
            json!({"message":"unsent leg","outcome":"notSent","partialOrderIds":["a"]}),
        ];
        for error in ambiguous {
            let frame = settled(error.clone()).await;
            assert_eq!(frame["outcome"], "unknown", "{error}");
            assert_eq!(frame["error"]["outcome"], "unknown", "{error}");
            assert_eq!(frame["error"]["message"], error["message"], "{error}");
        }

        let not_sent =
            settled(json!({"message":"account is busy; command not sent","outcome":"notSent"}))
                .await;
        assert_eq!(not_sent["outcome"], "refused");
        assert_eq!(not_sent["error"]["outcome"], "notSent");
    }

    // AGT-185, okx-t: entry and stop loss placed, take profit refused (51279). The
    // receipt stayed unknown with only the engine's text, so the client could not
    // say which legs landed. It stays unknown and now carries each leg's result.
    #[tokio::test]
    async fn a_partial_placement_carries_each_legs_result_and_stays_unknown() {
        let refusal = "OKX 51279: TP trigger price cannot be lower than the last price";
        let frame = settled(
            json!({"message":"{\"code\":\"1\"}","partialOrderIds":["entry","sl"],
            "elements":[
                {"index":0,"clientOrderId":"c0","status":"placed","orderIds":["entry"]},
                {"index":1,"status":"placed","orderIds":["sl"]},
                {"index":2,"status":"notSent","error":{"message":refusal,"outcome":"rejected",
                    "kind":"rejected","venueCode":"51279","venueMessage":"raw"}},
                {"index":3,"status":"unknown","error":{"message":"timed out","outcome":"unknown"}},
                {"index":4,"status":"applied","effect":"cancel","targetOrderId":"x"}]}),
        )
        .await;
        assert_eq!(frame["outcome"], "unknown");
        assert_eq!(frame["venueOrderId"], Value::Null);
        assert_eq!(frame["error"]["outcome"], "unknown");
        assert_eq!(
            frame["error"]["elements"],
            json!([
                {"index":0,"clientOrderId":"c0","status":"placed","orderIds":["entry"]},
                {"index":1,"status":"placed","orderIds":["sl"]},
                {"index":2,"status":"notSent","error":{"message":refusal}},
                {"index":3,"status":"unknown","error":{"message":"timed out"}},
                {"index":4,"status":"applied"}])
        );
        // A whole-order refusal and an error without elements carry none.
        for error in [
            json!({"message":"no","outcome":"rejected",
                "elements":[{"index":0,"status":"notSent","error":{"message":"no"}}]}),
            json!({"message":"timed out","outcome":"unknown"}),
        ] {
            assert_eq!(settled(error).await["error"].get("elements"), None);
        }
    }

    // AGT-214: the same OKX bracket names each leg, so the client can say "Entry and
    // stop-loss placed. Take-profit not sent". A role the box does not know is left
    // out; the leg and the list stay.
    #[tokio::test]
    async fn each_legs_role_is_carried_and_an_unknown_role_is_left_out() {
        let refusal = "OKX 51279: TP trigger price cannot be lower than the last price";
        let frame = settled(json!({"message":"m","partialOrderIds":["entry","sl"],
            "elements":[
                {"index":0,"clientOrderId":"c0","role":"entry","status":"placed","orderIds":["entry"]},
                {"index":1,"clientOrderId":"c1","role":"stopLoss","status":"placed","orderIds":["sl"]},
                {"index":2,"clientOrderId":"c2","role":"takeProfit","status":"notSent",
                    "error":{"message":refusal,"outcome":"rejected"}},
                {"index":3,"role":"other","status":"applied","effect":"settings"},
                {"index":4,"role":"trailingStop","status":"unknown","error":{"message":"timed out"}},
                {"index":5,"role":7,"status":"applied"}]}))
        .await;
        assert_eq!(frame["outcome"], "unknown");
        assert_eq!(
            frame["error"]["elements"],
            json!([
                {"index":0,"clientOrderId":"c0","role":"entry","status":"placed","orderIds":["entry"]},
                {"index":1,"clientOrderId":"c1","role":"stopLoss","status":"placed","orderIds":["sl"]},
                {"index":2,"clientOrderId":"c2","role":"takeProfit","status":"notSent",
                    "error":{"message":refusal}},
                {"index":3,"role":"other","status":"applied"},
                {"index":4,"status":"unknown","error":{"message":"timed out"}},
                {"index":5,"status":"applied"}])
        );
    }

    #[test]
    fn the_box_reads_every_engine_leg_role() {
        use safe_cex_engine::write_plan::LegRole;
        // `LegRole::ALL` is pinned to every variant by the engine's fixture test.
        let names: Vec<Value> = LegRole::ALL
            .iter()
            .map(|role| serde_json::to_value(role).unwrap())
            .collect();
        assert_eq!(names, LEG_ROLES.map(Value::from));
    }

    #[tokio::test]
    async fn leg_results_that_cannot_be_read_or_bounded_are_left_out() {
        let placed = json!({"index":0,"status":"placed","orderIds":["entry"]});
        for elements in [
            json!([placed, {"index":1,"status":"future"}]),
            json!([placed, {"index":-1,"status":"placed","orderIds":["b"]}]),
            json!([placed, {"index":1,"status":"placed","orderIds":[7]}]),
            json!([placed, {"index":1,"status":"unknown"}]),
            json!(
                (0..40)
                    .map(|index| json!({"index":index,"status":"placed","orderIds":["x"]}))
                    .collect::<Vec<_>>()
            ),
        ] {
            let frame =
                settled(json!({"message":"m","partialOrderIds":["entry"],"elements":elements}))
                    .await;
            assert_eq!(frame["outcome"], "unknown");
            assert_eq!(frame["error"].get("elements"), None, "{elements}");
        }
        // Long reasons are cut so every receipt stays inside the contract limit.
        let (out, mut sent, _) = Out::channel(1);
        let out = out.with_instance_id(Some(&"r".repeat(128)));
        let id = Value::String("018bcfe5-6800-7101-8101-010101010101".into());
        let reason = "\u{0}".repeat(4_000);
        let elements: Vec<Value> = (0..31)
            .map(|index| {
                json!({"index":index,"clientOrderId":"c".repeat(256),"status":"notSent",
                "error":{"message":reason}})
            })
            .chain([json!({"index":31,"status":"placed","orderIds":["e".repeat(256)]})])
            .collect();
        settle_intent(
            &out,
            "a",
            &id,
            Some(id.as_str().unwrap()),
            1,
            0,
            Err(
                json!({"message":"\u{0}".repeat(4_000),"partialOrderIds":["e"],"elements":elements}),
            ),
        );
        let text = sent.recv().await.unwrap();
        assert!(text.len() <= order_contract::OUTCOME_MAX, "{}", text.len());
    }

    // AGT-214 review: roles cost ~20 bytes a leg. A list that fits the 4 KiB bound
    // without them keeps every leg and sheds the roles instead of being dropped.
    #[test]
    fn a_leg_list_that_fits_only_without_roles_keeps_its_legs() {
        let legs = |id_len: usize, role: bool| -> Vec<Value> {
            (0..32)
                .map(|index| {
                    let mut leg =
                        json!({"index":index,"status":"placed","orderIds":["e".repeat(id_len)]});
                    if role {
                        leg["role"] = "takeProfit".into();
                    }
                    leg
                })
                .collect()
        };
        let size = |legs: Vec<Value>| Value::Array(legs).to_string().len();
        let id_len = (1..=256)
            .rev()
            .find(|len| size(legs(*len, false)) <= 4 * 1024)
            .unwrap();
        assert!(
            size(legs(id_len, true)) > 4 * 1024,
            "the control needs roles to overflow"
        );
        let kept = leg_results(&Value::Array(legs(id_len, true))).expect("legs that fit today");
        assert_eq!(kept, Value::Array(legs(id_len, false)));
        // A list that fits with its roles keeps them.
        let kept = leg_results(&Value::Array(legs(id_len - 20, true))).unwrap();
        assert!(
            kept.as_array()
                .unwrap()
                .iter()
                .all(|leg| leg["role"] == "takeProfit")
        );
    }

    // AGT-214 review: near the receipt limit the roles go before the leg list does.
    #[tokio::test]
    async fn a_receipt_near_its_limit_sheds_roles_before_the_leg_list() {
        let id = Value::String("018bcfe5-6800-7101-8101-010101010101".into());
        let elements: Vec<Value> = (0..32)
            .map(|index| {
                json!({"index":index,"role":"takeProfit","status":"placed",
                    "orderIds":["e".repeat(60)]})
            })
            .collect();
        assert_eq!(
            leg_results(&Value::Array(elements.clone())),
            Some(Value::Array(elements.clone()))
        );
        let state = |message: &str| {
            let (out, mut sent, _) = Out::channel(1);
            let out = out.with_instance_id(Some(&"\u{1}".repeat(128)));
            let error = json!({"message":message,"partialOrderIds":["e"],"elements":elements});
            settle_intent(&out, &"\u{1}".repeat(256), &id, None, 1, 0, Err(error));
            let text = sent.try_recv().unwrap();
            assert!(text.len() <= order_contract::OUTCOME_MAX, "{}", text.len());
            let frame: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(frame["d"]["outcome"], "unknown");
            match frame["d"]["error"]["elements"].as_array() {
                Some(legs) if legs.iter().all(|leg| leg["role"] == "takeProfit") => 0,
                Some(legs) if legs.iter().all(|leg| leg.get("role").is_none()) => 1,
                Some(legs) => panic!("partly shed roles: {legs:?}"),
                None => 2,
            }
        };
        let states: Vec<u8> = (0..=512).map(|chars| state(&"𐐀".repeat(chars))).collect();
        assert!(
            states.is_sorted(),
            "roles go first, then the list: {states:?}"
        );
        assert!(states.contains(&0) && states.contains(&1), "{states:?}");
    }

    // A kept list under its own 4 KiB bound can still push maximal, escape-heavy
    // bindings past the receipt limit; the list goes, the unknown outcome stays.
    #[tokio::test]
    async fn a_leg_list_that_would_overflow_the_receipt_is_dropped_and_the_outcome_kept() {
        let id = Value::String("018bcfe5-6800-7101-8101-010101010101".into());
        let elements: Vec<Value> = (0..32)
            .map(|index| json!({"index":index,"status":"placed","orderIds":["e".repeat(75)]}))
            .collect();
        assert!(leg_results(&Value::Array(elements.clone())).is_some());
        let receipt = |instance: &str, name: &str, message: &str| {
            let (out, mut sent, _) = Out::channel(1);
            let out = out.with_instance_id(Some(instance));
            let error = json!({"message":message,"partialOrderIds":["e"],"elements":elements});
            settle_intent(&out, name, &id, None, 1, 0, Err(error));
            sent.try_recv().unwrap()
        };
        let text = receipt(&"\u{1}".repeat(128), &"\u{1}".repeat(256), &"𐐀".repeat(600));
        assert!(text.len() <= order_contract::OUTCOME_MAX, "{}", text.len());
        let frame: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(frame["d"]["outcome"], "unknown");
        assert_eq!(frame["d"]["error"]["outcome"], "unknown");
        assert_eq!(frame["d"]["error"].get("elements"), None);
        // Ordinary bindings keep the same list.
        let frame: Value = serde_json::from_str(&receipt("r", "a", "m")).unwrap();
        assert_eq!(
            frame["d"]["error"]["elements"].as_array().unwrap().len(),
            32
        );
    }

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
                Some(id.as_str().unwrap()),
                1,
                0,
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
