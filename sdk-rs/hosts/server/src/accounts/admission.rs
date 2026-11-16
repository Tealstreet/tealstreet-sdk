use super::*;
use crate::delegated::{
    Credentials,
    probe::{ProbeRefusal, RefusalStage},
};
use safe_cex_crypto::admission::identity::Projection;

mod lighter;
mod local_proof;

enum ProofStrategy {
    PrivateStartup,
    Local(Arc<Projection>),
    RegisteredSchnorr,
}

use safe_cex_crypto::admission::{Family, PublicKey};

pub(super) struct PublicJoinProof {
    pub(super) account: std::sync::Weak<Account>,
    pub(super) public: PublicKey,
}

#[cfg(test)]
#[path = "admission_deadline_tests.rs"]
mod deadline_tests;

#[cfg(test)]
#[path = "public_join_proof_tests.rs"]
mod public_join_proof_tests;

pub(crate) struct ProbePermit {
    user: String,
    fingerprint: String,
    pub(super) lease: Option<crate::router_lease::Permit>,
    pub(super) token_expires_at: Option<u64>,
    pub(super) current: Arc<AtomicBool>,
    pending: PendingInits,
    registry_name: String,
    reservation: Option<proof_queue::Reservation>,
    strategy: ProofStrategy,
    instance_id: Option<String>,
    pub(super) local_deadline: Option<tokio::time::Instant>,
    verified_socket: Option<u64>,
    pub(super) public_join: Option<PublicJoinProof>,
    pub(super) registered_public: Option<Arc<PublicKey>>,
}
impl ProbePermit {
    pub(super) fn check(&self, user: &str, init: &InitPayload) -> Result<(), ProbeRefusal> {
        if self.user != user
            || identity::registry_name(user, init).as_ref() != Ok(&self.registry_name)
            || self.fingerprint != admission_fingerprint(init)
            || self.instance_id != init.instance_id
        {
            return Err(ProbeRefusal::INVALID);
        }
        if self
            .token_expires_at
            .is_some_and(|expiry| expiry <= now_ms())
        {
            return Err(ProbeRefusal::INVALID);
        }
        if self
            .local_deadline
            .is_some_and(|deadline| tokio::time::Instant::now() >= deadline)
        {
            return Err(ProbeRefusal::TRANSIENT.at(RefusalStage::ProbeTimeout));
        }
        if self.public_join.as_ref().is_some_and(|proof| {
            proof.account.upgrade().is_none_or(|account| {
                account.handle.disposal_cause().is_some() || !account.running.io_allowed()
            })
        }) {
            return Err(ProbeRefusal::TRANSIENT);
        }
        if !self.current.load(Ordering::Acquire)
            || self.lease.as_ref().is_some_and(|lease| !lease.allowed())
        {
            return Err(ProbeRefusal::TRANSIENT);
        }
        Ok(())
    }

    /// Old init metadata cannot stand in for an already installed resident key.
    /// This runs at attachment, after every proof await, before any mode change.
    pub(super) fn check_resident_public(&self, account: &Account) -> Result<(), String> {
        let ProofStrategy::Local(projection) = &self.strategy else {
            return Ok(());
        };
        if account.delegated().is_none() {
            return Ok(());
        }
        let expected = projection.public_key();
        let actual = account
            .signing
            .public_binding(expected.family())
            .map_err(|reason| reason.reason().to_owned())?
            .ok_or_else(|| {
                crate::envelope::ModeTransitionRefusal::Unavailable
                    .reason()
                    .to_owned()
            })?;
        if actual.as_bytes() != expected.as_bytes() {
            return Err(crate::envelope::ModeTransitionRefusal::Unavailable
                .reason()
                .to_owned());
        }
        Ok(())
    }
}

impl Drop for ProbePermit {
    fn drop(&mut self) {
        let key = (self.user.clone(), self.registry_name.clone());
        let mut pending = self.pending.lock().unwrap();
        if let Some(incarnation) = pending.get_mut(&key) {
            incarnation
                .probes
                .retain(|current| !Arc::ptr_eq(current, &self.current));
            if incarnation.attempts.is_empty() && incarnation.probes.is_empty() {
                pending.remove(&key);
            }
        }
    }
}

impl Credentials {
    /// Each handover copy is guarded until trusted native setup consumes it.
    pub(crate) fn pair(&self) -> DelegatedPair {
        let pair = DelegatedPair::new(
            self.key.as_str().to_owned(),
            self.secret.as_str().to_owned(),
            self.passphrase.as_ref().map(|v| v.as_str().to_owned()),
        );
        match &self.signing_key {
            Some(material) => pair.with_signing_key(material.as_str().to_owned()),
            None => pair,
        }
    }
}

impl Registry {
    fn proof_strategy(
        &self,
        user: &str,
        init: &InitPayload,
    ) -> Result<ProofStrategy, ProbeRefusal> {
        #[cfg(test)]
        if let Some(local) = self
            .local_proof_bindings
            .get(&(user.into(), admission_fingerprint(init)))
            .cloned()
        {
            return Ok(ProofStrategy::Local(local));
        }
        if matches!(
            init.exchange_name.as_str(),
            "hyperliquid" | "hyperliquidspot"
        ) && crate::delegated::probe::cosigned_ready(init)
        {
            let projection =
                identity::hyperliquid_projection(user, init).map_err(|_| ProbeRefusal::INVALID)?;
            return Ok(ProofStrategy::Local(Arc::new(projection)));
        }
        if init.exchange_name == "grvt"
            && crate::delegated::probe::cosigned_ready(init)
        {
            let projection = identity::grvt_projection(user, init)
                .map_err(|_| ProbeRefusal::INVALID.at(RefusalStage::Options))?;
            return Ok(ProofStrategy::Local(Arc::new(projection)));
        }
        if init.exchange_name == "backpack"
            && crate::delegated::probe::cosigned_ready(init)
        {
            let projection =
                identity::backpack_projection(user, init).map_err(|_| ProbeRefusal::INVALID)?;
            return Ok(ProofStrategy::Local(Arc::new(projection)));
        }
        if matches!(init.exchange_name.as_str(), "lighter" | "lighter-robinhood")
            && crate::delegated::probe::cosigned_ready(&init.exchange_name)
        {
            return Ok(ProofStrategy::RegisteredSchnorr);
        }
        Ok(ProofStrategy::PrivateStartup)
    }

    pub(crate) fn cosigned_proof_ready(&self, user: &str, init: &InitPayload) -> bool {
        match self.proof_strategy(user, init) {
            Ok(ProofStrategy::Local(_) | ProofStrategy::RegisteredSchnorr) => true,
            Ok(ProofStrategy::PrivateStartup) => {
                crate::delegated::probe::cosigned_ready(init)
            }
            Err(_) => false,
        }
    }

    pub(crate) fn private_join_requires_proof(&self) -> bool {
        #[cfg(any(test, feature = "test-support"))]
        if let Some(enabled) = self.join_admission_override {
            return enabled;
        }
        // Test fixtures may replace private venue IO; production has no proof opt-out.
        true
    }

    pub(crate) fn probe_permit(
        &self,
        user: &str,
        init: &InitPayload,
    ) -> Result<ProbePermit, ProbeRefusal> {
        let lease = self.router_leases.get(user).map(|user| user.permit());
        if self.router_leases.enforced() && lease.is_none() {
            return Err(ProbeRefusal::TRANSIENT);
        }
        let registry_name =
            identity::registry_name(user, init).map_err(|_| ProbeRefusal::INVALID)?;
        let strategy = self.proof_strategy(user, init)?;
        let (reservation, local_deadline) = match &strategy {
            ProofStrategy::PrivateStartup => (self.proof_queues.reserve(user, init)?, None),
            ProofStrategy::RegisteredSchnorr => (
                self.proof_queues.reserve_identity(
                    identity::lighter_queue(user, init).map_err(|_| ProbeRefusal::INVALID)?,
                )?,
                Some(tokio::time::Instant::now() + proof_timeout()),
            ),
            ProofStrategy::Local(projection) => (
                self.proof_queues.reserve_identity(projection.digest())?,
                Some(tokio::time::Instant::now() + proof_timeout()),
            ),
        };
        let fingerprint = admission_fingerprint(init);
        let public_join = if matches!(strategy, ProofStrategy::RegisteredSchnorr) {
            self.capture_public_join(user, init, &registry_name, &fingerprint)?
        } else {
            None
        };
        let current = Arc::new(AtomicBool::new(true));
        let mut pending = self.pending_inits.lock().unwrap();
        pending
            .entry((user.into(), registry_name.clone()))
            .or_insert_with(|| InitIncarnation {
                fingerprint: fingerprint.clone(),
                public_only: false,
                attempts: vec![],
                probes: vec![],
            })
            .probes
            .push(current.clone());
        drop(pending);
        let permit = ProbePermit {
            user: user.into(),
            fingerprint,
            lease,
            token_expires_at: None,
            current,
            pending: self.pending_inits.clone(),
            registry_name,
            reservation: Some(reservation),
            strategy,
            instance_id: init.instance_id.clone(),
            local_deadline,
            verified_socket: None,
            public_join,
            registered_public: None,
        };
        permit.check(user, init)?;
        Ok(permit)
    }

    fn capture_public_join(
        &self,
        user: &str,
        init: &InitPayload,
        registry_name: &str,
        fingerprint: &str,
    ) -> Result<Option<PublicJoinProof>, ProbeRefusal> {
        let Some(family) = Family::for_venue(&init.exchange_name) else {
            return Ok(None);
        };
        let (account, public) = {
            let existing = self
                .accounts
                .lock()
                .unwrap()
                .get(&(user.to_owned(), registry_name.to_owned()))
                .cloned();
            let Some(account) = existing else {
                return Ok(None);
            };
            if account.is_public() || !same_handover_settings(&account.fingerprint, fingerprint) {
                return Err(ProbeRefusal::INVALID);
            }
            if account.handle.disposal_cause().is_some() || !account.running.io_allowed() {
                return Err(ProbeRefusal::TRANSIENT);
            }
            let public = account.signing.public_binding(family).map_err(|reason| {
                if account.delegated().is_none() {
                    ProbeRefusal::transition(reason)
                } else {
                    ProbeRefusal {
                        transient: reason.transient(),
                        ..ProbeRefusal::INVALID
                    }
                }
            })?;
            (Arc::downgrade(&account), public)
        };
        let public = public.ok_or_else(|| {
            ProbeRefusal::transition(crate::envelope::ModeTransitionRefusal::Unavailable)
        })?;
        Ok(Some(PublicJoinProof { account, public }))
    }

    async fn prove_public_join(
        &self,
        init: &InitPayload,
        candidate: &ResidentCopy,
        plan: &RoutingPlan,
        token: Option<(String, u64)>,
        permit: &ProbePermit,
    ) -> Result<Option<PublicJoinProof>, ProbeRefusal> {
        let Some(family) = Family::for_venue(&init.exchange_name) else {
            return Ok(None);
        };
        permit.check(&permit.user, init)?;
        let Some(captured) = self.capture_public_join(
            &permit.user,
            init,
            &permit.registry_name,
            &permit.fingerprint,
        )?
        else {
            return Ok(None);
        };
        let PublicJoinProof { account, public } = captured;
        let captured = account.clone();
        let current = permit.current.clone();
        let lease = permit.lease.clone();
        let expiry = permit.token_expires_at;
        let allowed: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(move || {
            current.load(Ordering::Acquire)
                && lease.as_ref().is_none_or(|lease| lease.allowed())
                && expiry.is_none_or(|expiry| expiry > now_ms())
                && captured.upgrade().is_some_and(|account| {
                    account.handle.disposal_cause().is_none() && account.running.io_allowed()
                })
        });
        if !allowed() {
            return Err(ProbeRefusal::TRANSIENT);
        }
        if family == Family::Schnorr {
            let index = init
                .api_key_index
                .as_deref()
                .and_then(|index| index.parse::<u8>().ok())
                .ok_or_else(|| {
                    ProbeRefusal::transition(crate::envelope::ModeTransitionRefusal::Unavailable)
                })?;
            let account_index = init
                .application_id
                .as_deref()
                .and_then(|account| account.parse::<u64>().ok())
                .ok_or_else(|| {
                    ProbeRefusal::transition(crate::envelope::ModeTransitionRefusal::Unavailable)
                })?;
            let token_provider = ProxyToken::new();
            token_provider.set(token);
            let transport = NetworkTransport::with_routing(plan.clone(), token_provider.provider())
                .with_venue(&init.exchange_name)
                .with_report_user(safe_cex_native::report::ReportUser::new(&permit.user))
                .with_io_guard(allowed.clone());
            #[cfg(test)]
            let transport = crate::delegated_e2e_io::Io::new(transport, self.e2e_venue.clone());
            let network = safe_cex_venues::lighter_registration::Network::from_options(
                init.exchange_name == "lighter-robinhood",
                init.testnet == Some(true),
            );
            let registered = crate::admission::fetch_lighter_registered_key(
                &transport,
                network,
                account_index,
                index,
                allowed.clone(),
            )
            .await
            .map_err(|_| ProbeRefusal::TRANSIENT)?;
            if registered.as_bytes() != public.as_bytes() {
                return Err(ProbeRefusal::INVALID);
            }
        }
        permit.check(&permit.user, init)?;
        if !allowed()
            || !self
                .accounts
                .lock()
                .unwrap()
                .get(&(permit.user.clone(), permit.registry_name.clone()))
                .is_some_and(|current| account.ptr_eq(&Arc::downgrade(current)))
        {
            return Err(ProbeRefusal::TRANSIENT);
        }
        let key = candidate
            .identities()
            .primary
            .as_ref()
            .ok_or(ProbeRefusal::INVALID)?
            .signing_ref()
            .map_err(|_| ProbeRefusal::INVALID)?;
        if !public
            .matches_provider(&candidate.provider(), key)
            .map_err(|_| ProbeRefusal::INVALID)?
        {
            return Err(ProbeRefusal::INVALID);
        }
        permit.check(&permit.user, init)?;
        if !allowed() {
            return Err(ProbeRefusal::TRANSIENT);
        }
        Ok(Some(PublicJoinProof { account, public }))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn probe_join(
        &self,
        user: &str,
        init: &InitPayload,
        token: Option<(String, u64)>,
        candidate: Option<&Credentials>,
        link: Option<Arc<SignerLink>>,
        socket: u64,
        mut permit: ProbePermit,
    ) -> Result<ProbePermit, ProbeRefusal> {
        permit.token_expires_at = Some(token.as_ref().ok_or(ProbeRefusal::INVALID)?.1);
        permit.check(user, init)?;
        if link.as_ref().is_some_and(|link| link.socket() != socket) {
            return Err(ProbeRefusal::INVALID);
        }
        let deadline = permit
            .local_deadline
            .unwrap_or_else(|| tokio::time::Instant::now() + proof_timeout());
        let work = async {
            let active = permit
                .reservation
                .take()
                .ok_or(ProbeRefusal::INVALID)?
                .start()
                .await?;
            permit.check(user, init)?;
            let projection = match &permit.strategy {
                ProofStrategy::Local(projection) => Some(projection.clone()),
                ProofStrategy::RegisteredSchnorr => {
                    self.pacer.admit().await;
                    permit.check(user, init)?;
                    let projection =
                        lighter::resolve(self, user, init, token.clone(), link.clone(), &permit)
                            .await?;
                    let public = projection.public_key();
                    permit.registered_public = Some(Arc::new(
                        PublicKey::new(public.family(), public.as_bytes())
                            .map_err(|_| ProbeRefusal::INVALID)?,
                    ));
                    Some(Arc::new(projection))
                }
                ProofStrategy::PrivateStartup => None,
            };
            if let Some(projection) = projection {
                if matches!(permit.strategy, ProofStrategy::Local(_)) {
                    self.pacer.admit().await;
                }
                permit.check(user, init)?;
                return local_proof::verify(
                    user,
                    init,
                    candidate,
                    link.as_ref(),
                    socket,
                    &projection,
                    &permit,
                )
                .await;
            }
            let mut config = self.engine_config.clone();
            config.trusted_signer_state = None;
            let (plan, options, provider, mode, resident) = match (candidate, link.as_ref()) {
                (Some(candidate), None) if init.signing_mode.as_deref() == Some("delegated") => {
                    let prepared = prepare_delegated(init, candidate.pair())
                        .map_err(|_| ProbeRefusal::INVALID.at(RefusalStage::SignerSetup))?;
                    prepared.resident.configure(&mut config);
                    (
                        prepared.plan,
                        prepared.options,
                        prepared.resident.provider(),
                        "hosted_delegated",
                        Some(prepared.resident),
                    )
                }
                (None, Some(link))
                    if init.signing_mode.as_deref() != Some("delegated")
                        && crate::delegated::probe::cosigned_ready(init) =>
                {
                    let prepared = prepare(init, false, Some(link.clone()))
                        .map_err(|_| ProbeRefusal::INVALID)?;
                    let remote = prepared.remote.ok_or(ProbeRefusal::INVALID)?;
                    (
                        prepared.plan,
                        prepared.options,
                        remote.provider,
                        "hosted_cosign",
                        None,
                    )
                }
                _ => return Err(ProbeRefusal::INVALID),
            };
            #[cfg(test)]
            let plan = self
                .construction_test_plans
                .get(user)
                .cloned()
                .unwrap_or(plan);
            if let Some(candidate) = &resident
                && let Some(proof) = self
                    .prove_public_join(init, candidate, &plan, token.clone(), &permit)
                    .await?
            {
                permit.public_join = Some(proof);
                return Ok(());
            }
            let token_provider = ProxyToken::new();
            token_provider.set(token);
            config.log_label = None;
            let report_user = safe_cex_native::report::ReportUser::new(user);
            config.report_user = Some(report_user.clone());
            config.signing_mode = Some(mode);
            let (mut result, receive) = tokio::sync::oneshot::channel();
            let lease = permit.lease.clone();
            let current = permit.current.clone();
            let expiry = permit.token_expires_at;
            let venue = init.exchange_name.clone();
            #[cfg(test)]
            let e2e_venue = self.e2e_venue.clone();
            self.pacer.admit().await;
            permit.check(user, init)?;
            let placed = self.threads.place(move |placement| {
                tokio::task::spawn_local(async move {
                    // The queued proof owns its erasure authority through native
                    // probe cleanup. Engine provider handles alone cannot retain it.
                    let _resident = resident;
                    let _active = active;
                    let _placement = placement;
                    let live = Arc::new(AtomicBool::new(true));
                    let captured_live = live.clone();
                    let allowed: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(move || {
                        captured_live.load(Ordering::Acquire)
                            && current.load(Ordering::Acquire)
                            && lease.as_ref().is_none_or(|permit| permit.allowed())
                            && expiry.is_none_or(|expiry| expiry > now_ms())
                            && link.as_ref().is_none_or(|link| link.is_available())
                    });
                    if result.is_closed() || !allowed() {
                        let _ = result.send(Err(ProbeRefusal::TRANSIENT));
                        return;
                    }
                    let transport = NetworkTransport::with_routing(plan, token_provider.provider())
                        .with_venue(&venue)
                        .with_report_user(report_user)
                        .with_io_guard(allowed.clone());
                    #[cfg(test)]
                    let transport = crate::delegated_e2e_io::Io::new(transport, e2e_venue);
                    let probe = crate::delegated::probe::run(
                        &venue, &options, provider, transport, config, allowed,
                    );
                    tokio::pin!(probe);
                    tokio::select! {
                        biased;
                        _ = result.closed() => {
                            live.store(false, Ordering::Release);
                            let _ = probe.await;
                        },
                        answer = &mut probe => { let _ = result.send(answer); }
                    }
                });
            });
            if !placed {
                return Err(ProbeRefusal::TRANSIENT);
            }
            receive.await.map_err(|_| ProbeRefusal::TRANSIENT)?
        };
        tokio::time::timeout_at(deadline, work)
            .await
            .map_err(|_| ProbeRefusal::TRANSIENT.at(RefusalStage::ProbeTimeout))??;
        permit.check(user, init)?;
        permit.verified_socket = Some(socket);
        Ok(permit)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn obtain_probed_join(
        self: &Arc<Self>,
        user: &str,
        init: &InitPayload,
        holder: u64,
        token: Option<(String, u64)>,
        outbox: Outbox,
        pair: Option<DelegatedPair>,
        proof: ProbePermit,
    ) -> Result<Arc<Account>, ProbeRefusal> {
        proof.check(user, init)?;
        if proof.verified_socket != Some(holder) {
            return Err(ProbeRefusal::INVALID);
        }
        let delegated = init.signing_mode.as_deref() == Some("delegated");
        if delegated != pair.is_some() {
            return Err(ProbeRefusal::INVALID);
        }
        if let Some(pair) = &pair {
            pair.validate(init).map_err(|_| ProbeRefusal::INVALID)?;
        }
        let existing = self
            .accounts
            .lock()
            .unwrap()
            .get(&(user.into(), proof.registry_name.clone()))
            .cloned();
        if let Some(public) = &proof.public_join
            && existing
                .as_ref()
                .is_none_or(|current| !public.account.ptr_eq(&Arc::downgrade(current)))
        {
            return Err(ProbeRefusal::TRANSIENT);
        }
        if let Some(account) = existing
            && (account.is_public()
                || !same_handover_settings(&account.fingerprint, &proof.fingerprint))
        {
            return Err(ProbeRefusal::INVALID);
        }
        self.obtain_with_pair(user, init, false, holder, token, outbox, pair, Some(proof))
            .await
            .map_err(|error| {
                for reason in [
                    crate::envelope::ModeTransitionRefusal::Busy,
                    crate::envelope::ModeTransitionRefusal::Unavailable,
                ] {
                    if error == reason.reason() {
                        return ProbeRefusal {
                            transient: reason.transient(),
                            transition: Some(reason),
                            ..ProbeRefusal::INVALID
                        };
                    }
                }
                ProbeRefusal::TRANSIENT
            })
    }
}

pub(super) fn admission_fingerprint(init: &InitPayload) -> String {
    if init.signing_mode.as_deref() == Some("delegated") {
        crate::delegated::fingerprint(init)
    } else {
        init.fingerprint()
    }
}

fn proof_timeout() -> Duration {
    safe_cex_remote_signer::SIGN_TIMEOUT
        + Duration::from_millis(u64::from(
            safe_cex_engine::request_budget::RequestClass::Read.budget_ms(),
        ))
}

#[cfg(all(test, feature = "delegated-order-frames"))]
impl ProbePermit {
    pub(crate) fn verified_fixture(mut self, socket: u64) -> Self {
        self.verified_socket = Some(socket);
        self
    }
}
