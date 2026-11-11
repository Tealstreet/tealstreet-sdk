use super::*;
use crate::delegated::{Credentials, probe::ProbeRefusal};
use safe_cex_crypto::signer::Secret;

pub(crate) struct ProbePermit {
    user: String,
    fingerprint: String,
    pub(super) lease: Option<crate::router_lease::Permit>,
}
impl ProbePermit {
    pub(super) fn check(&self, user: &str, init: &InitPayload) -> Result<(), ProbeRefusal> {
        if self.user != user || self.fingerprint != crate::delegated::fingerprint(init) {
            return Err(ProbeRefusal::INVALID);
        }
        if self.lease.as_ref().is_some_and(|lease| !lease.allowed()) {
            return Err(ProbeRefusal::TRANSIENT);
        }
        Ok(())
    }
}

impl Credentials {
    /// Each temporary signer owns its existing zeroizing Secret allocation.
    pub(crate) fn pair(&self) -> DelegatedPair {
        DelegatedPair::new(
            self.key.as_str().to_owned(),
            Secret::new(self.secret.as_str().to_owned()),
            self.passphrase
                .as_ref()
                .map(|v| Secret::new(v.as_str().to_owned())),
        )
    }
}

impl Registry {
    pub(crate) fn probe_permit(
        &self,
        user: &str,
        init: &InitPayload,
    ) -> Result<ProbePermit, ProbeRefusal> {
        let lease = self.router_leases.get(user).map(|user| user.permit());
        if self.router_leases.enforced() && lease.is_none() {
            return Err(ProbeRefusal::TRANSIENT);
        }
        let permit = ProbePermit {
            user: user.into(),
            fingerprint: crate::delegated::fingerprint(init),
            lease,
        };
        permit.check(user, init)?;
        Ok(permit)
    }

    pub(crate) async fn probe_delegated(
        &self,
        user: &str,
        init: &InitPayload,
        token: Option<(String, u64)>,
        candidate: &Credentials,
        permit: ProbePermit,
    ) -> Result<ProbePermit, ProbeRefusal> {
        permit.check(user, init)?;
        let prepared =
            prepare_delegated(init, candidate.pair()).map_err(|_| ProbeRefusal::INVALID)?;
        #[cfg(test)]
        let prepared = match self.construction_test_plans.get(user) {
            Some(plan) => PreparedDelegated {
                plan: plan.clone(),
                ..prepared
            },
            None => prepared,
        };
        let token_provider = ProxyToken::new();
        token_provider.set(token);
        let mut config = self.engine_config.clone();
        config.log_label = None;
        config.signing_mode = Some("hosted_delegated");
        let (mut result, receive) = tokio::sync::oneshot::channel();
        let lease = permit.lease.clone();
        #[cfg(test)]
        let e2e_venue = self.e2e_venue.clone();
        let work = async {
            self.pacer.admit().await;
            permit.check(user, init)?;
            let placed = self.threads.place(move |placement| {
                tokio::task::spawn_local(async move {
                    let _placement = placement;
                    if result.is_closed() || lease.as_ref().is_some_and(|permit| !permit.allowed()) {
                        let _ = result.send(Err(ProbeRefusal::TRANSIENT));
                        return;
                    }
                    let allowed: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(move || lease.as_ref().is_none_or(|permit| permit.allowed()));
                    let transport = NetworkTransport::with_routing(prepared.plan.clone(), token_provider.provider())
                        .with_venue("bybit").with_io_guard(allowed.clone());
                    #[cfg(test)]
                    let transport = crate::delegated_e2e_io::Io::new(transport, e2e_venue);
                    tokio::select! {
                        biased;
                        _ = result.closed() => {},
                        answer = crate::delegated::probe::run(prepared, transport, config, allowed) => { let _ = result.send(answer); }
                    }
                });
            });
            if !placed {
                return Err(ProbeRefusal::TRANSIENT);
            }
            receive.await.map_err(|_| ProbeRefusal::TRANSIENT)?
        };
        tokio::time::timeout(Duration::from_secs(30), work)
            .await
            .map_err(|_| ProbeRefusal::TRANSIENT)??;
        permit.check(user, init)?;
        Ok(permit)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn obtain_probed_delegated(
        self: &Arc<Self>,
        user: &str,
        init: &InitPayload,
        holder: u64,
        token: Option<(String, u64)>,
        outbox: Outbox,
        pair: DelegatedPair,
        proof: ProbePermit,
    ) -> Result<Arc<Account>, ProbeRefusal> {
        proof.check(user, init)?;
        pair.validate(init).map_err(|_| ProbeRefusal::INVALID)?;
        if let Some(account) = self
            .accounts
            .lock()
            .unwrap()
            .get(&(user.into(), init.account_name.clone()))
            && (account.is_public()
                || account.delegated.is_none()
                || account.fingerprint != proof.fingerprint)
        {
            return Err(ProbeRefusal::INVALID);
        }
        self.obtain_with_pair(
            user,
            init,
            false,
            holder,
            token,
            outbox,
            Some(pair),
            Some(proof),
        )
        .await
        .map_err(|_| ProbeRefusal::TRANSIENT)
    }
}
