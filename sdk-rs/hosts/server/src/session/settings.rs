use super::*;

impl Session {
    pub(super) fn reset_settings(&mut self, request: envelope::ApplySettings) {
        let name = request.account_name;
        if let Some(pending) = self.pending_verifications.get(&name) {
            pending.out.send(envelope::error(Some(&name), "verifying"));
            return;
        }
        let admitted = request.instance_id.is_some()
            && self
                .identity
                .token
                .as_ref()
                .is_some_and(|(_, expires)| *expires > now_ms())
            && !self.router_fenced()
            && !self.moved_by_drain()
            && self.joined.get(&name).is_some_and(|joined| {
                joined.out.action_active()
                    && joined.out.order_binding(request.instance_id.as_deref())
                    && self
                        .registry
                        .held(self.user(), &name, self.socket)
                        .is_some_and(|current| Arc::ptr_eq(&current, &joined.account))
            });
        if !admitted
            || !self.registry.reset_settings(
                &self.joined[&name].account,
                self.socket,
                self.joined[&name].out.outbox(),
            )
        {
            self.handover_out(&name).send(envelope::error(
                Some(&name),
                "apply settings refused: account not admitted",
            ));
            return;
        }
        let joined = self.joined.remove(&name).expect("admitted account");
        self.handovers.forget(&name);
        self.drop_early(&name, "settings changed");
        self.refused.remove(&name);
        // Stop notices keep the old output; normal init retires its generation.
        joined.out.send(envelope::msg(
            &name,
            "applySettings:done",
            json!({"success":true,"requiresInit":true}),
        ));
        joined.out.retire_registration();
        Self::leave_engine(joined);
    }
}
