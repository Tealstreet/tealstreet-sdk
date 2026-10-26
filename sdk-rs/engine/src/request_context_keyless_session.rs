//! Issuer-bound Session authority for credential-free serial command roots.
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

    pub(in crate::engine) fn validate_keyless_session(
        &self,
        owner: Owner,
    ) -> Result<Option<u64>, Error> {
        self.keyless_session(owner)
            .map(|session| self.validate_session(session))
            .transpose()
    }

    pub(in crate::engine) fn keyless_session_egress(
        &self,
        owner: Owner,
    ) -> Result<Option<Egress>, Error> {
        Ok(self
            .validate_keyless_session(owner)?
            .and_then(|generation| self.sessions.egress(generation)))
    }

    fn revoked_session_root(&self) -> Option<(Preflight, Option<Plan>)> {
        self.commands.iter().find_map(|(id, owner)| {
            let handle = Preflight::new(&self.issuer, *id, owner.generation);
            if owner.pending != 0
                || owner.deliveries != 0
                || self.validate_keyless_session(handle.owner()).is_ok()
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
        if !now.is_finite()
            || self.io.core.http_delivery.is_some()
            || self.io.core.http_plan_ready.is_some()
            || self.io.core.http_preflight_ready.is_some()
            || !self.io.core.request_contexts.plan_scope_clear()
        {
            return;
        }
        while let Some((preflight, plan)) = self.io.core.request_contexts.revoked_session_root() {
            let error = Error::Revoked.venue_error();
            let mut cx = Cx {
                now,
                store: &mut self.store,
                io: &mut self.io,
            };
            if let Some(plan) = plan {
                cx.fail_plan_owned(&plan, error)
            } else {
                cx.fail_preflight(&preflight, error)
            }
            .expect("current retired Session root");
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
