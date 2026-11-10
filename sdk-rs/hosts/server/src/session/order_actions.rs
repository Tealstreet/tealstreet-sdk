use super::*;
use crate::order_actions::{self, CancelQueued, Request};

impl Session {
    pub(super) async fn order_action(&mut self, request: Request) {
        let name = request.account_name.clone();
        let id = Value::String(request.intent_id.clone());
        let out = self.handover_out(&name);
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
            let action = serde_json::to_value(&request.action).unwrap_or(Value::Null);
            match crate::order_contract::action_fingerprint(
                &name,
                &request.intent_id,
                &action,
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
        let (method, args) = match request.action.call() {
            Ok(call) => call,
            Err(message) => {
                order_actions::refuse(&out, &name, &id, request.seq, message);
                return;
            }
        };
        order_actions::acknowledge(&out, &name, &id, request.seq, true);
        self.call_inner(
            &name,
            json!({"requestId":request.intent_id,"method":method,"args":args}),
            Some(OrderReceipt::Action {
                seq: request.seq,
                kind: request.action.kind(),
            }),
        )
        .await;
    }

    pub(super) fn action_cancel_queued(&mut self, cancel: CancelQueued) {
        let out = self.handover_out(&cancel.account_name);
        if !out.order_receipt_binding_bounded()
            || cancel.account_name.len() > 256
            || cancel.intent_id.len() > 128
            || cancel.instance_id.as_ref().is_some_and(|id| id.len() > 128)
        {
            out.send(envelope::error(
                Some(&cancel.account_name),
                "not sent: invalid action binding",
            ));
            return;
        }
        let id = Value::String(cancel.intent_id);
        if cancel.version != 1
            || !self.order_admitted(&cancel.account_name, cancel.instance_id.as_deref())
            || crate::order_contract::key_time(id.as_str().unwrap_or_default()).is_none()
            || cancel.seq == 0
            || cancel.seq > 9_007_199_254_740_991
            || cancel.fingerprint.len() != 64
            || !cancel
                .fingerprint
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            order_actions::refuse(
                &out,
                &cancel.account_name,
                &id,
                cancel.seq,
                "not sent: invalid action queue cancellation",
            );
            return;
        }
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

    pub(super) fn send_action_nuke(&self, name: &str, id: &Value, seq: u64) {
        let account = self.joined[name].account.clone();
        let receipt = crate::order_actions::NukeReceipt::new(
            self.account_out(name),
            name.to_owned(),
            id.clone(),
            seq,
        );
        let Some(algos) = account.algos.clone() else {
            receipt.finish(Err(
                json!({"message":"Nuke is unavailable for this account","outcome":"notSent"}),
            ));
            return;
        };
        let registry = self.registry.clone();
        let socket = self.socket;
        let request_id = id.clone();
        tokio::spawn(async move {
            let loaded =
                methods::until_nuke_can_see(&account.handle, "all", methods::READ_WAIT).await;
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
                mode: "all",
                request_id,
                action: Some(receipt),
            }) {
                if let crate::algos::AlgoInput::Nuke {
                    action: Some(receipt),
                    ..
                } = error.0
                {
                    receipt.finish(Err(json!({"message":"Nuke is unavailable for this account","outcome":"notSent"})));
                }
            }
        });
    }
}
