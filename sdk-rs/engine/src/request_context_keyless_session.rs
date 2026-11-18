//! Issuer-bound Session authority for explicitly authenticated serial calls.
use super::*;
use crate::request_context::Session;

impl ContextRegistry {
    pub(in crate::engine) fn keyless_session(&self, owner: Owner) -> Option<&Session> {
        match &self.owner_entry(owner)?.phase {
            CommandPhase::Preflight(state) => state.session_root(),
            CommandPhase::Planned(state) => state.session_root(),
            _ => None,
        }
    }

    pub(in crate::engine) fn validate_session_root(
        &self,
        session: &Session,
        write: Option<&crate::request_context::Credential>,
        identities: &crate::signing::Identities,
    ) -> Result<u64, Error> {
        let generation = self.validate_session(session)?;
        if let Some(credential) = self.sessions.credential(generation) {
            let identity = identities.primary.as_ref().ok_or(Error::Invalid)?;
            if !identity.accepts_session_root(credential, write) {
                return Err(Error::Invalid);
            }
        } else if write.is_some() || self.sessions.egress(generation).is_none() {
            return Err(Error::Invalid);
        }
        Ok(generation)
    }

    pub(in crate::engine) fn validate_keyless_session(
        &self,
        owner: Owner,
        identities: &crate::signing::Identities,
    ) -> Result<Option<u64>, Error> {
        let Some(entry) = self.owner_entry(owner) else {
            return Ok(None);
        };
        let (session, write) = match &entry.phase {
            CommandPhase::Preflight(state) => (state.session_root(), state.session_write()),
            CommandPhase::Planned(state) => (state.session_root(), state.session_write()),
            _ => (None, None),
        };
        session
            .map(|session| self.validate_session_root(session, write, identities))
            .transpose()
    }

    /// Whether this leg rides the login Session: every Preflight leg of a
    /// Session root, and each Plan leg as its call declares.
    pub(in crate::engine) fn session_call(&self, context: &Context) -> bool {
        match self.owner_entry(context.owner()).map(|entry| &entry.phase) {
            Some(CommandPhase::Preflight(state)) => state.session_root().is_some(),
            Some(CommandPhase::Planned(state)) => state.session_call(context),
            _ => false,
        }
    }

    /// The validated root's egress; the caller keeps it only for a Session leg.
    pub(in crate::engine) fn keyless_session_egress(
        &self,
        owner: Owner,
        identities: &crate::signing::Identities,
    ) -> Result<Option<Egress>, Error> {
        let generation = self.validate_keyless_session(owner, identities)?;
        Ok(generation.map(|generation| {
            Egress::Session(crate::request_context::SessionEgress::new(generation))
        }))
    }

    #[cfg(test)]
    pub(in crate::engine) fn keyless_scan_visits(&self) -> usize {
        self.keyless_scan_visits.get()
    }

    #[cfg(test)]
    pub(in crate::engine) fn keyless_root_count(&self) -> usize {
        self.keyless_roots
    }

    fn revoked_session_root(
        &self,
        identities: &crate::signing::Identities,
    ) -> Option<(Preflight, Option<Plan>)> {
        if self.keyless_roots == 0 {
            return None;
        }
        self.commands.iter().find_map(|(id, owner)| {
            #[cfg(any(test, feature = "test-support"))]
            self.keyless_scan_visits
                .set(self.keyless_scan_visits.get() + 1);
            let handle = Preflight::new(&self.issuer, *id, owner.generation);
            if owner.pending != 0
                || owner.deliveries != 0
                || self
                    .validate_keyless_session(handle.owner(), identities)
                    .is_ok()
            {
                return None;
            }
            match &owner.phase {
                CommandPhase::Preflight(_) => Some((handle, None)),
                CommandPhase::Planned(state) => Some((handle, Some(state.handle()))),
                _ => None,
            }
        })
    }
}

impl<V: crate::Venue> crate::Engine<V> {
    pub(in crate::engine) fn flush_keyless_session_roots(&mut self, now: f64) {
        if self.io.core.request_contexts.keyless_roots == 0
            || !now.is_finite()
            || self.io.core.command_entry.is_some()
            || self.io.core.http_plan_expiring.is_some()
            || self.io.core.http_delivery.is_some()
            || self.io.core.http_plan_ready.is_some()
            || self.io.core.http_preflight_ready.is_some()
            || !self.io.core.request_contexts.plan_scope_clear()
        {
            return;
        }
        while let Some((preflight, plan)) = self
            .io
            .core
            .request_contexts
            .revoked_session_root(&self.io.core.identities)
        {
            let error = Error::Revoked.venue_error();
            let mut cx = Cx {
                now,
                store: &mut self.store,
                io: &mut self.io,
            };
            let sealed = if let Some(plan) = plan {
                cx.fail_plan_owned(&plan, error)
            } else {
                cx.fail_preflight(&preflight, error)
            };
            if let Err(error) = sealed {
                debug_assert!(false, "retired Session root seal refused: {error:?}");
                break;
            }
            self.flush_http_owner_retirements();
            self.finish_deferred_commands(now);
        }
    }
}

// Length-delimited codec fields exclude the Session bundle refreshed at Bind.
pub(super) fn wire_fingerprint(
    method: crate::Method,
    url: &str,
    headers: &[(&str, &str)],
    body: Option<&str>,
) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    fn field(hash: &mut Sha256, bytes: &[u8]) {
        hash.update((bytes.len() as u64).to_be_bytes());
        hash.update(bytes);
    }
    let mut hash = Sha256::new();
    field(&mut hash, method.as_str().as_bytes());
    field(&mut hash, url.as_bytes());
    hash.update([u8::from(body.is_some())]);
    field(&mut hash, body.unwrap_or_default().as_bytes());
    hash.update((headers.len() as u64).to_be_bytes());
    for (name, value) in headers {
        field(&mut hash, name.as_bytes());
        field(&mut hash, value.as_bytes());
    }
    hash.finalize().into()
}
