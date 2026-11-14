use super::*;
use crate::order_actions::{self, CancelQueued, WireRequest};

impl Session {
    pub(super) async fn order_action(&mut self, request: WireRequest, text: &str) {
        let arrived = std::time::Instant::now();
        let name = request.account_name.clone();
        let id = Value::String(request.intent_id.clone());
        let out = self.order_out(&name, request.instance_id.as_deref());
        if !out.order_receipt_binding_bounded()
            || name.len() > 256
            || request.intent_id.len() > 128
            || request
                .instance_id
                .as_ref()
                .is_some_and(|id| id.len() > 128)
        {
            out.send(envelope::error(
                Some(&name),
                "not sent: invalid action binding",
            ));
            return;
        }
        let refused = if request.version != 1 {
            Some("not sent: unsupported action version")
        } else if !self.order_admitted(&name, request.instance_id.as_deref()) {
            Some("not sent: account not admitted for actions")
        } else if let Err(error) =
            crate::order_contract::check_key(&request.intent_id, crate::accounts::now_ms())
        {
            Some(error)
        } else if request.seq == 0
            || request.seq > 9_007_199_254_740_991
            || !request.age_ms.is_finite()
            || !(0.0..10_000.0).contains(&request.age_ms)
        {
            Some("not sent: expired")
        } else {
            match crate::order_contract::action_wire_fingerprint(
                &name,
                &request.intent_id,
                &request.action,
                request.seq,
            ) {
                Ok(fp) if fp == request.fingerprint => None,
                _ => Some("not sent: action fingerprint differs"),
            }
        };
        if let Some(message) = refused {
            order_actions::refuse(&out, &name, &id, request.seq, message);
            return;
        }
        let Ok(decoded) = order_actions::decode_request(text) else {
            order_actions::unsupported(&out, &name, &id, request.seq);
            return;
        };
        let action = decoded.action;
        let (method, args) = match action.call() {
            Ok(call) => call,
            Err(_) => {
                order_actions::unsupported(&out, &name, &id, request.seq);
                return;
            }
        };
        let Some(deadline) =
            order_actions::ActionDeadline::new(&request.intent_id, request.age_ms, arrived)
        else {
            order_actions::refuse(&out, &name, &id, request.seq, "not sent: expired");
            return;
        };
        if let crate::order_actions::Action::Algo { op, data } = &action {
            if !crate::full_writes::algo_account_matches(op, data, &name) {
                order_actions::refuse(
                    &out,
                    &name,
                    &id,
                    request.seq,
                    "not sent: algo account differs",
                );
                return;
            }
            let receipt = crate::order_actions::NukeReceipt::new(
                out.clone(),
                name.clone(),
                id.clone(),
                request.seq,
                deadline,
                self.token_expires_at_ms(),
            )
            .with_kind("algo");
            let Some(algos) = self.joined[&name].account.algos.clone() else {
                receipt.finish(Err(
                    json!({"message":"Algo host unavailable","outcome":"notSent"}),
                ));
                return;
            };
            order_actions::acknowledge(&out, &name, &id, request.seq, true);
            let mut data = data.clone();
            if op == "exec" {
                data["reqId"] = id.clone();
            }
            // The wire label is validated above; the shared TWAP book retains
            // the resident actor's original label.
            if op == "twap:start" {
                data["accountName"] = self.joined[&name].account.name.clone().into();
            }
            if let Err(error) = algos.send(crate::algos::AlgoInput::Op {
                op: op.clone(),
                data: data.to_string(),
                origin: format!("socket:{}", self.socket),
                twap_owner: self.joined[&name].account.twap_owner.snapshot(),
                action: Some(Box::new(receipt)),
            }) && let crate::algos::AlgoInput::Op {
                action: Some(receipt),
                ..
            } = error.0
            {
                receipt.finish(Err(
                    json!({"message":"Algo host unavailable","outcome":"notSent"}),
                ));
            }
            return;
        }
        order_actions::acknowledge(&out, &name, &id, request.seq, true);
        self.call_inner(
            &name,
            json!({"requestId":request.intent_id,"method":method,"args":args}),
            Some(OrderReceipt::Action {
                seq: request.seq,
                kind: action.kind(),
                deadline,
            }),
        )
        .await;
    }

    pub(super) fn action_cancel_queued(&mut self, cancel: CancelQueued) {
        self.forget_disposed();
        let out = match self.joined.get(&cancel.account_name) {
            Some(joined) if joined.out.order_binding(cancel.instance_id.as_deref()) => {
                joined.out.clone()
            }
            _ => self.order_out(&cancel.account_name, cancel.instance_id.as_deref()),
        };
        if !out.order_receipt_binding_bounded()
            || cancel.account_name.len() > 256
            || cancel.intent_id.len() > 128
            || cancel.instance_id.as_ref().is_some_and(|id| id.len() > 128)
        {
            out.send(envelope::error(
                Some(&cancel.account_name),
                "Invalid action queue cancellation binding",
            ));
            return;
        }
        let id = Value::String(cancel.intent_id);
        if cancel.version != 1
            || !self
                .joined
                .get(&cancel.account_name)
                .is_some_and(|joined| joined.out.order_binding(cancel.instance_id.as_deref()))
            || crate::order_contract::key_time(id.as_str().unwrap_or_default()).is_none()
            || cancel.seq == 0
            || cancel.seq > 9_007_199_254_740_991
            || cancel.fingerprint.len() != 64
            || !cancel
                .fingerprint
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            // Reject this frame, not the original business action. A malformed
            // queue-cancel supplies no whole-intent no-handoff evidence.
            out.send(envelope::error(
                Some(&cancel.account_name),
                "Invalid action queue cancellation",
            ));
            return;
        }
        // This router-only operation never dispatches venue work. A still-bound
        // cancellation stays uncertain even if write admission has since changed.
        order_actions::settle(
            &out,
            &cancel.account_name,
            &id,
            cancel.seq,
            "",
            Err(
                json!({"message":"Queue cancellation must be resolved by the router","outcome":"unknown"}),
            ),
        );
    }

    pub(super) fn send_action_nuke(
        &self,
        name: &str,
        id: &Value,
        captured: OrderReceipt,
        mode: &'static str,
    ) {
        let OrderReceipt::Action {
            seq,
            deadline,
            kind,
        } = captured
        else {
            return;
        };
        let account = self.joined[name].account.clone();
        let receipt = crate::order_actions::NukeReceipt::new(
            self.account_out(name),
            name.to_owned(),
            id.clone(),
            seq,
            deadline,
            self.token_expires_at_ms(),
        )
        .with_kind(kind);
        let Some(algos) = account.algos.clone() else {
            receipt.finish(Err(
                json!({"message":"Nuke is unavailable for this account","outcome":"notSent"}),
            ));
            return;
        };
        let registry = self.registry.clone();
        let socket = self.socket;
        let expires_at_ms = self.token_expires_at_ms();
        let request_id = id.clone();
        tokio::spawn(async move {
            let loaded =
                methods::until_nuke_can_see(&account.handle, mode, methods::READ_WAIT).await;
            if let Err(message) = crate::auth::Verifier::check_expiry(expires_at_ms, now_ms()) {
                receipt.finish(Err(json!({"message":message,"outcome":"unknown"})));
                return;
            }
            if receipt.expired() {
                receipt.finish(Err(order_actions::expired_outcome()));
                return;
            }
            if !receipt.active()
                || !account.running.io_allowed()
                || !registry.holds(&account, socket)
            {
                receipt.finish(Err(json!({"message":"Action registration retired before dispatch","outcome":"notSent"})));
                return;
            }
            if let Err(error) = loaded {
                receipt.finish(Err(json!({"message":error.message,"outcome":"notSent"})));
                return;
            }
            if let Err(error) = algos.send(crate::algos::AlgoInput::Nuke {
                mode,
                request_id,
                action: Some(Box::new(receipt)),
            }) && let crate::algos::AlgoInput::Nuke {
                action: Some(receipt),
                ..
            } = error.0
            {
                receipt.finish(Err(
                    json!({"message":"Nuke is unavailable for this account","outcome":"notSent"}),
                ));
            }
        });
    }
}
