use super::*;
use crate::delegated::probe::ProbeRefusal;

impl Registry {
    pub(crate) async fn probe_cosigned_test(
        &self,
        user: &str,
        init: &InitPayload,
        token: Option<(String, u64)>,
        link: Arc<SignerLink>,
        permit: ProbePermit,
    ) -> Result<ProbePermit, ProbeRefusal> {
        permit.check(user, init)?;
        let prepared =
            prepare(init, false, Some(link.clone())).map_err(|_| ProbeRefusal::INVALID)?;
        let plan = self
            .construction_test_plans
            .get(user)
            .cloned()
            .ok_or(ProbeRefusal::INVALID)?;
        let remote = prepared.remote.ok_or(ProbeRefusal::INVALID)?;
        let prepared = PreparedDelegated {
            plan,
            options: prepared.options,
            resident: ResidentCopy::test_provider(remote.provider),
        };
        let tokens = ProxyToken::new();
        tokens.set(token);
        let mut config = self.engine_config.clone();
        config.log_label = None;
        config.signing_mode = Some("hosted_cosign");
        let lease = permit.lease.clone();
        let allowed: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(move || {
            link.is_available() && lease.as_ref().is_none_or(|permit| permit.allowed())
        });
        let (mut result, receive) = tokio::sync::oneshot::channel();
        let venue = init.exchange_name.clone();
        let work = async {
            self.pacer.admit().await;
            permit.check(user, init)?;
            if !allowed() {
                return Err(ProbeRefusal::TRANSIENT);
            }
            let placed = self.threads.place(move |placement| {
                tokio::task::spawn_local(async move {
                    let _placement = placement;
                    if result.is_closed() || !allowed() { return; }
                    let transport = NetworkTransport::with_routing(prepared.plan.clone(), tokens.provider())
                        .with_venue(&venue).with_io_guard(allowed.clone());
                    tokio::select! {
                        biased;
                        _ = result.closed() => {},
                        answer = crate::delegated::probe::run(&venue, prepared, transport, config, allowed) => {
                            let _ = result.send(answer);
                        }
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
    pub(crate) async fn obtain_probed_cosigned_test(
        self: &Arc<Self>,
        user: &str,
        init: &InitPayload,
        holder: u64,
        token: Option<(String, u64)>,
        outbox: Outbox,
        proof: ProbePermit,
    ) -> Result<Arc<Account>, ProbeRefusal> {
        proof.check(user, init)?;
        if let Some(account) = self
            .accounts
            .lock()
            .unwrap()
            .get(&(user.into(), init.account_name.clone()))
            && (account.is_public()
                || account.delegated.is_some()
                || account.fingerprint != init.fingerprint())
        {
            return Err(ProbeRefusal::INVALID);
        }
        self.obtain_with_pair(user, init, false, holder, token, outbox, None, Some(proof))
            .await
            .map_err(|_| ProbeRefusal::TRANSIENT)
    }
}
