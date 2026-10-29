//! Engine-owned PublicAddress setup. A result is not a resource join.
#[cfg(feature = "address-nonce")]
use super::*;
#[cfg(feature = "address-nonce")]
use crate::nonce::{NonceDomain, NonceSelectionError};
#[cfg(feature = "address-nonce")]
use crate::signing::{Algorithm, Identity, KeyRef, SignError};
#[cfg(feature = "address-nonce")]
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AddressSetupSlots {
    #[default]
    None,
    Primary,
    Secondary,
    Both,
}
#[cfg(feature = "address-nonce")]
impl AddressSetupSlots {
    fn contains(self, slot: CredentialSlot) -> bool {
        matches!(
            (self, slot),
            (Self::Both, _)
                | (Self::Primary, CredentialSlot::Primary)
                | (Self::Secondary, CredentialSlot::Secondary)
        )
    }
}

#[derive(Debug)]
pub struct SetupRetirement {
    #[cfg(feature = "address-nonce")]
    issuer: Arc<Issuer>,
    #[cfg(feature = "address-nonce")]
    generation: u64,
    #[cfg(feature = "address-nonce")]
    task: RequestId,
    #[cfg(not(feature = "address-nonce"))]
    unavailable: std::convert::Infallible,
}
#[cfg(feature = "address-nonce")]
#[derive(Debug)]
struct Issuer {
    wire: u64,
}
#[cfg(feature = "address-nonce")]
impl PartialEq for Issuer {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}
#[cfg(feature = "address-nonce")]
impl Eq for Issuer {}
impl SetupRetirement {
    pub fn to_wire(&self) -> String {
        #[cfg(feature = "address-nonce")]
        {
            format!(
                "s1:{}:{}:{}",
                self.issuer.wire, self.generation, self.task.0
            )
        }
        #[cfg(not(feature = "address-nonce"))]
        match self.unavailable {}
    }
}

#[cfg(feature = "address-nonce")]
mod enabled {
    use super::*;
    use crate::signing::{
        Context, Identities, Operation, OwnedSetupExecution, SignReceipt, SignRequest, SignResult,
    };
    use std::task::{Context as TaskContext, Poll};

    const BOUND_MS: f64 = 20_000.;
    static ISSUERS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    fn issuer() -> Result<Arc<Issuer>, VenueError> {
        ISSUERS
            .try_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |value| value.checked_add(1),
            )
            .map(|wire| Arc::new(Issuer { wire }))
            .map_err(|_| SignError::Unavailable.venue_error())
    }
    const MAX_GENERATION: u64 = (1 << 53) - 2;

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub enum SetupHostSupport {
        #[default]
        Unavailable,
        OwnedPublicAddress1,
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum AddressSetupProgress {
        Ready,
        Pending,
        Retiring,
    }

    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub(crate) struct AddressCapability {
        state: Option<Box<AddressState>>,
    }
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum AddressState {
        Refused(SignError),
        Verified(VerifiedAddress),
        Key(VerifiedKey),
    }
    /// A host-answered provider's public key, bound to its signing ref and slot.
    #[derive(Clone, Debug, PartialEq, Eq)]
    struct VerifiedKey {
        bytes: Box<[u8]>,
        algorithm: Algorithm,
        signing: KeyRef,
    }
    #[derive(Clone, PartialEq, Eq)]
    pub(crate) struct VerifiedAddress {
        bytes: [u8; 20],
        signing: KeyRef,
        slot: CredentialSlot,
        domain: Option<&'static NonceDomain>,
        issuer: Arc<Issuer>,
        generation: u64,
    }
    impl std::fmt::Debug for VerifiedAddress {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_tuple("VerifiedAddress").field(&self.hex()).finish()
        }
    }
    impl VerifiedAddress {
        pub(crate) fn hex(&self) -> String {
            let mut value = String::from("0x");
            use std::fmt::Write;
            for byte in self.bytes {
                write!(value, "{byte:02x}").expect("String write");
            }
            value
        }
    }
    impl AddressCapability {
        fn refused(error: SignError) -> Self {
            Self {
                state: Some(Box::new(AddressState::Refused(error))),
            }
        }
        fn bound(value: VerifiedAddress) -> Self {
            Self {
                state: Some(Box::new(AddressState::Verified(value))),
            }
        }
        fn keyed(value: VerifiedKey) -> Self {
            Self {
                state: Some(Box::new(AddressState::Key(value))),
            }
        }
        pub(crate) fn verified(
            &self,
            identity: &Identity,
            slot: CredentialSlot,
            domain: Option<&'static NonceDomain>,
        ) -> Result<VerifiedAddress, NonceSelectionError> {
            match self.state.as_deref() {
                Some(AddressState::Verified(value))
                    if value.slot == slot
                        && value.domain == domain
                        && identity
                            .signing_ref()
                            .is_ok_and(|key| *key == value.signing) =>
                {
                    Ok(value.clone())
                }
                Some(AddressState::Refused(error)) => {
                    Err(NonceSelectionError::SetupCapability(*error))
                }
                _ => Err(NonceSelectionError::InvalidSubject),
            }
        }
    }
    impl Identity {
        pub fn verified_signing_address(&self) -> Result<[u8; 20], SignError> {
            let signing = self.signing_ref()?;
            match self.address.state.as_deref() {
                Some(AddressState::Verified(value)) if value.signing == *signing => Ok(value.bytes),
                Some(AddressState::Refused(error)) => Err(*error),
                _ => Err(SignError::InvalidRequest),
            }
        }
        /// The setup-bound public key for this identity's current signing ref,
        /// at the algorithm's length (32 Stark/Ed25519, 40 Schnorr).
        pub fn verified_setup_public_key(&self, algorithm: Algorithm) -> Result<&[u8], SignError> {
            let signing = self.signing_ref()?;
            match self.address.state.as_deref() {
                Some(AddressState::Key(value))
                    if value.signing == *signing && value.algorithm == algorithm =>
                {
                    Ok(&value.bytes)
                }
                Some(AddressState::Refused(error)) => Err(*error),
                _ => Err(SignError::InvalidRequest),
            }
        }
    }

    struct Task {
        id: RequestId,
        generation: u64,
        slot: CredentialSlot,
        signing: KeyRef,
        work: SignRequest,
        deadline: f64,
        timer: TimerId,
        owner: Option<Box<dyn OwnedSetupExecution>>,
        result: Option<Result<Vec<u8>, SignError>>,
        joined: bool,
        retiring: bool,
        token: Option<Arc<SetupRetirement>>,
    }
    #[derive(Default)]
    pub(in crate::engine) struct AddressNonceSetup {
        issuer: Option<Arc<Issuer>>,
        generation: u64,
        tasks: Vec<Task>,
        initialized: bool,
        sealed: bool,
        replacement: bool,
        support: SetupHostSupport,
        domain: Option<&'static NonceDomain>,
        slots: AddressSetupSlots,
        /// `Some` binds a public key of this algorithm instead of an address.
        key: Option<Algorithm>,
    }
    impl AddressNonceSetup {
        fn operation(&self) -> crate::sign_meta::SetupOperation {
            self.key.map_or(
                crate::sign_meta::SetupOperation::PublicAddress,
                crate::sign_meta::SetupOperation::PublicKey,
            )
        }
        /// The venue's declaration: address slots, or key slots and algorithm.
        fn declare<V: Venue>(&mut self, venue: &V) -> Result<(), SignError> {
            let address = venue.public_address_slots();
            match venue.public_key_setup() {
                Some(_) if address != AddressSetupSlots::None => Err(SignError::InvalidRequest),
                Some((slots, algorithm)) => {
                    if crate::signing::public_key_length(algorithm).is_none() {
                        return Err(SignError::InvalidRequest);
                    }
                    self.slots = slots;
                    self.key = Some(algorithm);
                    Ok(())
                }
                None => {
                    self.slots = address;
                    self.key = None;
                    Ok(())
                }
            }
        }
        fn invalidate(&mut self, actions: &mut Actions, identities: &mut Identities) {
            for identity in [&mut identities.primary, &mut identities.secondary]
                .into_iter()
                .flatten()
            {
                identity.address = Default::default();
            }
            for task in &mut self.tasks {
                if !task.retiring {
                    task.retiring = true;
                    actions.push(Action::ClearTimer { timer: task.timer });
                    if let Some(owner) = &mut task.owner {
                        let _ = owner.cancel();
                    } else {
                        request_retirement(task, self.issuer.as_ref().unwrap(), actions);
                    }
                }
            }
        }
        fn progress(&self) -> AddressSetupProgress {
            if self.tasks.iter().any(|task| task.retiring) {
                AddressSetupProgress::Retiring
            } else if self.tasks.is_empty() {
                AddressSetupProgress::Ready
            } else {
                AddressSetupProgress::Pending
            }
        }
        pub(in crate::engine) fn current_meta(
            &self,
            task: RequestId,
        ) -> Option<crate::sign_meta::SignMeta> {
            let task = self.tasks.iter().find(|entry| {
                entry.id == task
                    && !entry.retiring
                    && entry.result.is_none()
                    && entry.generation == self.generation
            })?;
            Some(crate::sign_meta::SignMeta {
                task: task.id,
                deadline_ms: task.deadline,
                kind: crate::sign_meta::SignKind::Setup {
                    generation: task.generation,
                    operation: self.operation(),
                    slot: task.slot,
                },
            })
        }
    }
    fn identity_mut(identities: &mut Identities, slot: CredentialSlot) -> Option<&mut Identity> {
        match slot {
            CredentialSlot::Primary => identities.primary.as_mut(),
            CredentialSlot::Secondary => identities.secondary.as_mut(),
        }
    }
    fn request_retirement(task: &mut Task, issuer: &Arc<Issuer>, actions: &mut Actions) {
        if task.token.is_none() {
            let token = Arc::new(SetupRetirement {
                issuer: issuer.clone(),
                generation: task.generation,
                task: task.id,
            });
            task.token = Some(token.clone());
            actions.push(Action::RetireAddressSetup {
                task: task.id,
                token,
            });
        }
    }
    /// The one value, at the length its operation declares.
    fn valid_result(
        work: &SignRequest,
        result: Result<SignResult, SignError>,
    ) -> Result<Vec<u8>, SignError> {
        let result = result?;
        work.validate_result(&result)?;
        match result.values.as_slice() {
            [value] => Ok(value.clone()),
            _ => Err(SignError::InvalidResult),
        }
    }

    impl<V: Venue> Engine<V> {
        pub fn set_address_setup_host_support(
            &mut self,
            support: SetupHostSupport,
        ) -> Result<(), VenueError> {
            if self.started {
                return Err(SignError::InvalidRequest.venue_error());
            }
            self.invalidate_address_setup()?;
            self.io.core.address_setup.support = support;
            Ok(())
        }
        pub fn initialize_address_nonce(
            &mut self,
            now: f64,
        ) -> Result<AddressSetupProgress, VenueError> {
            if self.started
                || !now.is_finite()
                || now < 0.
                || !(now + BOUND_MS).is_finite()
                || now + BOUND_MS <= now
            {
                return Err(SignError::InvalidRequest.venue_error());
            }
            if !self.io.core.address_setup.initialized {
                let domain = self.venue.address_nonce_domain();
                if domain.is_some_and(|domain| {
                    *domain != crate::nonce::HYPERLIQUID_AGENT
                        && *domain != crate::nonce::ASTERDEX_V3
                }) {
                    return Err(SignError::InvalidRequest.venue_error());
                }
                let issued = issuer()?;
                let setup = &mut self.io.core.address_setup;
                setup.declare(&self.venue).map_err(SignError::venue_error)?;
                setup.initialized = true;
                setup.generation = 1;
                setup.issuer = Some(issued);
                setup.domain = domain;
                self.launch_address_setup(now);
            }
            self.poll_address_setup(now, &mut TaskContext::from_waker(std::task::Waker::noop()));
            Ok(self.io.core.address_setup.progress())
        }
        pub fn reinitialize_address_nonce(
            &mut self,
            now: f64,
        ) -> Result<AddressSetupProgress, VenueError> {
            if self.started
                || !now.is_finite()
                || now < 0.
                || !(now + BOUND_MS).is_finite()
                || now + BOUND_MS <= now
            {
                return Err(SignError::InvalidRequest.venue_error());
            }
            if !self.io.core.address_setup.initialized {
                return self.initialize_address_nonce(now);
            }
            let core = &mut self.io.core;
            if !core.address_setup.replacement {
                if core.address_setup.generation >= MAX_GENERATION {
                    return Err(SignError::Unavailable.venue_error());
                }
                core.address_setup.generation += 1;
                core.address_setup.replacement = true;
                core.address_setup
                    .invalidate(&mut core.actions, &mut core.identities);
            }
            self.poll_address_setup(now, &mut TaskContext::from_waker(std::task::Waker::noop()));
            Ok(self.io.core.address_setup.progress())
        }
        pub fn address_nonce_setup_ready(&self) -> bool {
            self.io.core.address_setup.initialized && self.io.core.address_setup.tasks.is_empty()
        }
        fn launch_address_setup(&mut self, now: f64) {
            let core = &mut self.io.core;
            for slot in [CredentialSlot::Primary, CredentialSlot::Secondary] {
                if !core.address_setup.slots.contains(slot) {
                    continue;
                }
                let Some(identity) = identity_mut(&mut core.identities, slot) else {
                    continue;
                };
                let signing = match identity.signing_ref() {
                    Ok(key) => key.clone(),
                    Err(error) => {
                        identity.address = AddressCapability::refused(error);
                        continue;
                    }
                };
                let external = core.signer.is_none();
                let capable = if let Some(provider) = &core.signer {
                    provider.owned_public_address_capability()
                        == crate::signing::Capability::Supported
                } else {
                    core.address_setup.support == SetupHostSupport::OwnedPublicAddress1
                };
                if !capable {
                    identity.address = AddressCapability::refused(SignError::Unsupported);
                    continue;
                }
                let operation = match core.address_setup.key {
                    Some(algorithm) => Operation::PublicKey {
                        algorithm,
                        key: signing.clone(),
                    },
                    None => Operation::PublicAddress {
                        key: signing.clone(),
                    },
                };
                let work = SignRequest {
                    receipt: SignReceipt::default(),
                    operations: vec![operation],
                    context: Context::default(),
                };
                let owner = core
                    .signer
                    .as_ref()
                    .and_then(|provider| provider.create_public_address_setup(work.clone()));
                if !external && owner.is_none() {
                    identity.address = AddressCapability::refused(SignError::Unsupported);
                    continue;
                }
                let id = core.signing.next_id();
                let timer = TimerId(core.next_id.next_or_fatal());
                let deadline = now + BOUND_MS;
                core.timers.insert(timer, ());
                core.actions.push(Action::SetTimer {
                    timer,
                    at_ms: deadline,
                });
                core.address_setup.tasks.push(Task {
                    id,
                    generation: core.address_setup.generation,
                    slot,
                    signing,
                    work: work.clone(),
                    deadline,
                    timer,
                    owner,
                    result: None,
                    joined: false,
                    retiring: false,
                    token: None,
                });
                let task = core.address_setup.tasks.last_mut().unwrap();
                if let Some(owner) = &mut task.owner
                    && let Err(error) = owner.start()
                {
                    task.result = Some(Err(error));
                }
                if external {
                    core.actions.push(Action::Sign {
                        attempt: id,
                        work,
                        timeout_ms: BOUND_MS as u32,
                    });
                }
            }
        }
        pub fn poll_address_setup(&mut self, now: f64, cx: &mut TaskContext<'_>) {
            if !now.is_finite() || now < 0. {
                return;
            }
            let core = &mut self.io.core;
            let setup = &mut core.address_setup;
            for task in &mut setup.tasks {
                if now >= task.deadline && !task.retiring {
                    core.timers.remove(&task.timer);
                    core.actions.push(Action::ClearTimer { timer: task.timer });
                    task.result = Some(Err(SignError::TimedOut));
                    task.retiring = true;
                    if let Some(identity) = identity_mut(&mut core.identities, task.slot) {
                        identity.address = AddressCapability::refused(SignError::TimedOut);
                    }
                    if let Some(owner) = &mut task.owner {
                        let _ = owner.cancel();
                    } else {
                        request_retirement(task, setup.issuer.as_ref().unwrap(), &mut core.actions);
                    }
                }
                if let Some(owner) = &mut task.owner {
                    if task.result.is_none()
                        && let Poll::Ready(result) = owner.poll_result(cx)
                    {
                        task.result = Some(valid_result(&task.work, result));
                    }
                    if !task.joined
                        && let Poll::Ready(result) = owner.poll_joined(cx)
                    {
                        task.joined = true;
                        if let Err(error) = result {
                            task.result = Some(Err(error));
                        }
                    }
                }
            }
            let mut index = 0;
            while index < setup.tasks.len() {
                if !setup.tasks[index].joined
                    || (setup.tasks[index].result.is_none() && !setup.tasks[index].retiring)
                {
                    index += 1;
                    continue;
                }
                let task = setup.tasks.remove(index);
                core.timers.remove(&task.timer);
                core.actions.push(Action::ClearTimer { timer: task.timer });
                if !setup.sealed
                    && task.generation == setup.generation
                    && !setup.replacement
                    && let Some(identity) = identity_mut(&mut core.identities, task.slot)
                {
                    identity.address = match task.result.unwrap_or(Err(SignError::InvalidResult)) {
                        Ok(bytes) if now < task.deadline && !task.retiring => match setup.key {
                            Some(algorithm) => AddressCapability::keyed(VerifiedKey {
                                bytes: bytes.into_boxed_slice(),
                                algorithm,
                                signing: task.signing,
                            }),
                            None => match <[u8; 20]>::try_from(bytes.as_slice()) {
                                Ok(bytes) => AddressCapability::bound(VerifiedAddress {
                                    bytes,
                                    signing: task.signing,
                                    slot: task.slot,
                                    domain: setup.domain,
                                    issuer: setup.issuer.as_ref().unwrap().clone(),
                                    generation: setup.generation,
                                }),
                                Err(_) => AddressCapability::refused(SignError::InvalidResult),
                            },
                        },
                        Err(error) => AddressCapability::refused(error),
                        _ => AddressCapability::refused(SignError::TimedOut),
                    };
                }
            }
            if setup.replacement && setup.tasks.is_empty() && !setup.sealed {
                setup.replacement = false;
                self.launch_address_setup(now);
            }
        }
        pub(in crate::engine) fn address_setup_timer(&mut self, now: f64, timer: TimerId) -> bool {
            let core = &mut self.io.core;
            let Some(task) = core
                .address_setup
                .tasks
                .iter_mut()
                .find(|task| task.timer == timer)
            else {
                return false;
            };
            if (!now.is_finite() || now < task.deadline) && !task.retiring {
                core.timers.remove(&timer);
                task.timer = TimerId(core.next_id.next_or_fatal());
                core.timers.insert(task.timer, ());
                core.actions.push(Action::SetTimer {
                    timer: task.timer,
                    at_ms: task.deadline,
                });
            }
            true
        }
        pub(in crate::engine) fn address_setup_signed(
            &mut self,
            now: f64,
            id: RequestId,
            result: Result<SignResult, SignError>,
        ) -> bool {
            let core = &mut self.io.core;
            let setup = &mut core.address_setup;
            let Some(task) = setup.tasks.iter_mut().find(|task| task.id == id) else {
                return false;
            };
            if now.is_finite()
                && now >= 0.
                && now < task.deadline
                && !task.retiring
                && task.result.is_none()
                && task.owner.is_none()
            {
                task.result = Some(valid_result(&task.work, result));
                request_retirement(task, setup.issuer.as_ref().unwrap(), &mut core.actions);
            }
            true
        }
        pub fn address_setup_retired_wire(&mut self, wire: &str, now: f64) -> bool {
            let token = self
                .io
                .core
                .address_setup
                .tasks
                .iter()
                .filter_map(|task| task.token.as_ref())
                .find(|token| token.to_wire() == wire)
                .cloned();
            token.is_some_and(|token| self.address_setup_retired(token, now))
        }
        pub fn address_setup_retired(
            &mut self,
            retirement: Arc<SetupRetirement>,
            now: f64,
        ) -> bool {
            if !now.is_finite() || now < 0. {
                return false;
            }
            let setup = &mut self.io.core.address_setup;
            let Some(task) = setup.tasks.iter_mut().find(|task| {
                task.id == retirement.task
                    && task.generation == retirement.generation
                    && task
                        .token
                        .as_ref()
                        .is_some_and(|token| Arc::ptr_eq(token, &retirement))
                    && setup
                        .issuer
                        .as_ref()
                        .is_some_and(|issuer| Arc::ptr_eq(issuer, &retirement.issuer))
            }) else {
                return false;
            };
            if task.joined {
                return false;
            }
            task.joined = true;
            self.poll_address_setup(now, &mut TaskContext::from_waker(std::task::Waker::noop()));
            true
        }
        pub(in crate::engine) fn seal_address_setup(&mut self) {
            if !self.io.core.address_setup.initialized {
                // A conflicting declaration seals with no slots, refusing nothing.
                if self.io.core.address_setup.declare(&self.venue).is_err() {
                    self.io.core.address_setup.slots = AddressSetupSlots::None;
                }
                self.io.core.address_setup.domain = self.venue.address_nonce_domain();
            }
            let core = &mut self.io.core;
            core.address_setup.sealed = true;
            for task in &mut core.address_setup.tasks {
                task.retiring = true;
                if let Some(owner) = &mut task.owner {
                    let _ = owner.cancel();
                } else {
                    request_retirement(
                        task,
                        core.address_setup.issuer.as_ref().unwrap(),
                        &mut core.actions,
                    );
                }
            }
            for slot in [CredentialSlot::Primary, CredentialSlot::Secondary] {
                if core.address_setup.slots.contains(slot)
                    && let Some(identity) = identity_mut(&mut core.identities, slot)
                    && identity.address.state.is_none()
                {
                    identity.address = AddressCapability::refused(SignError::Unsupported);
                }
            }
        }
        pub(in crate::engine) fn invalidate_address_setup(&mut self) -> Result<(), VenueError> {
            let core = &mut self.io.core;
            if !core.address_setup.initialized {
                return Ok(());
            }
            if core.address_setup.generation >= MAX_GENERATION {
                return Err(SignError::Unavailable.venue_error());
            }
            core.address_setup.generation += 1;
            core.address_setup.replacement = false;
            core.address_setup.support = SetupHostSupport::Unavailable;
            core.address_setup
                .invalidate(&mut core.actions, &mut core.identities);
            Ok(())
        }

        pub fn take_address_setup_cleanup(&mut self) -> AddressSetupCleanup {
            let core = &mut self.io.core;
            core.address_setup.sealed = true;
            core.address_setup
                .invalidate(&mut core.actions, &mut core.identities);
            let owners = core
                .address_setup
                .tasks
                .iter_mut()
                .filter_map(|task| task.owner.take())
                .collect();
            AddressSetupCleanup {
                owners,
                failed: false,
            }
        }
    }
    impl<P> Cx<'_, P> {
        pub fn address_nonce_identities(&self) -> &Identities {
            &self.io.core.identities
        }
        /// A slot's setup-bound public key; see `Identity::verified_setup_public_key`.
        pub fn setup_public_key(
            &self,
            slot: CredentialSlot,
            algorithm: Algorithm,
        ) -> Result<&[u8], SignError> {
            match slot {
                CredentialSlot::Primary => self.io.core.identities.primary.as_ref(),
                CredentialSlot::Secondary => self.io.core.identities.secondary.as_ref(),
            }
            .ok_or(SignError::MissingKey)?
            .verified_setup_public_key(algorithm)
        }
    }

    pub struct AddressSetupCleanup {
        owners: Vec<Box<dyn OwnedSetupExecution>>,
        failed: bool,
    }
    impl std::future::Future for AddressSetupCleanup {
        type Output = Result<(), SignError>;
        fn poll(
            mut self: std::pin::Pin<&mut Self>,
            cx: &mut TaskContext<'_>,
        ) -> Poll<Self::Output> {
            let mut index = 0;
            while index < self.owners.len() {
                match self.owners[index].poll_joined(cx) {
                    Poll::Ready(result) => {
                        self.failed |= result.is_err();
                        self.owners.remove(index);
                    }
                    Poll::Pending => index += 1,
                }
            }
            if self.owners.is_empty() {
                Poll::Ready(if self.failed {
                    Err(SignError::Unavailable)
                } else {
                    Ok(())
                })
            } else {
                Poll::Pending
            }
        }
    }
}
#[cfg(feature = "address-nonce")]
pub(super) use enabled::AddressNonceSetup;
#[cfg(feature = "address-nonce")]
pub(crate) use enabled::{AddressCapability, VerifiedAddress};
#[cfg(feature = "address-nonce")]
pub use enabled::{AddressSetupCleanup, AddressSetupProgress, SetupHostSupport};

#[cfg(all(test, feature = "address-nonce"))]
mod capability_layout_tests {
    use super::AddressCapability;

    #[test]
    fn private_address_capability_fits_one_pointer() {
        assert!(
            size_of::<AddressCapability>() <= size_of::<usize>(),
            "address capability is {} bytes; one pointer is {} bytes",
            size_of::<AddressCapability>(),
            size_of::<usize>(),
        );
    }

    #[test]
    fn unified_address_feature_keeps_read_custody_within_clippy_variant_limit() {
        let admitted = size_of::<crate::selected_read::AdmittedRead>();
        let context = size_of::<crate::request_context::Context>();
        assert!(
            admitted.saturating_sub(context) <= 200,
            "signed read is {admitted} bytes versus unsigned context {context} bytes",
        );
    }
}
