//! Store transitions and owned generation joins; no callbacks under NONCES.
use crate::nonce::handoff::LegacyNonceRequest;
use crate::nonce::{
    NONCES, NonceDomain, NonceError, NonceKey, NonceMap, NonceState, NonceSubjectStrategy,
};
use crate::signing::Algorithm;
use crate::signing::{Capability, Identity};
use crate::{CredentialSlot, Engine, Venue};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
};
type Key = (&'static str, [u8; 32]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Open,
    Fenced,
    Retired,
}
pub(crate) struct Tracking {
    pub(crate) epoch: Arc<()>,
    pub(crate) phase: Phase,
    owners: Vec<Arc<Owner>>,
    untracked: bool,
    target: Option<Key>,
    sources: Vec<Key>,
}
impl Default for Tracking {
    fn default() -> Self {
        Self {
            epoch: Arc::new(()),
            phase: Phase::Open,
            owners: vec![],
            untracked: false,
            target: None,
            sources: vec![],
        }
    }
}
pub(crate) struct Owner {
    key: Key,
    joined: AtomicBool,
    source: bool,
}
impl std::fmt::Debug for Owner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NonceOwner")
    }
}
impl PartialEq for Owner {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}
impl Eq for Owner {}

/// One installed identity's trusted association; no raw digest or subject access.
pub struct LegacyNonceAssociation {
    engine: Arc<()>,
    slot: CredentialSlot,
    identity: Identity,
    domain: &'static NonceDomain,
    source: Key,
    target: Key,
}
/// Host generation ownership. Dropping it never acknowledges retirement.
pub struct NonceOwner {
    owner: Arc<Owner>,
}
/// Legacy reservation authority usable only within the retained generation.
pub struct LegacyNonceIssuer {
    owner: Arc<Owner>,
}
/// An issued nonce, retained through signing, queuing and transport cleanup.
pub struct LegacyNoncePermit {
    owner: Arc<Owner>,
    nonce: u64,
}
/// Poll only the original generation future, whose Ready includes all cleanup.
#[must_use = "retain and poll the original owner through actual cleanup"]
pub struct LegacyNonceWork<F> {
    owner: Arc<Owner>,
    future: Option<Pin<Box<F>>>,
    finished: bool,
}
/// One complete fenced source set and target. Retryable only before commit.
pub struct NonceTakeover {
    engine: Arc<()>,
    epoch: Arc<()>,
    associations: Vec<LegacyNonceAssociation>,
    committed: bool,
}
fn lock() -> Result<std::sync::MutexGuard<'static, NonceMap>, NonceError> {
    NONCES
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| NonceError::Unavailable)
}
fn key(domain: &NonceDomain, id: &Identity) -> Result<Key, NonceError> {
    if domain.subject != NonceSubjectStrategy::StableId || id.validate().is_err() {
        return Err(NonceError::InvalidClaim);
    }
    let subject = id
        .require_stable_id()
        .map_err(|_| NonceError::InvalidClaim)?;
    id.signing_ref().map_err(|_| NonceError::InvalidClaim)?;
    Ok((domain.name, NonceKey::stable_id(subject).0))
}
fn can_track(state: Option<&NonceState>) -> bool {
    state.is_none_or(|s| {
        s.tracking
            .as_ref()
            .map_or(s.reserved == 0 && s.released.is_none(), |t| {
                !t.untracked && t.phase == Phase::Open
            })
    })
}
fn fresh_tracking(store: &NonceMap, key: Key) -> Option<Box<Tracking>> {
    store
        .get(&key)
        .and_then(|s| s.tracking.as_ref())
        .is_none()
        .then(|| Box::new(Tracking::default()))
}
fn install_tracking(store: &mut NonceMap, key: Key, fresh: Option<Box<Tracking>>) {
    if let Some(fresh) = fresh {
        store.entry(key).or_default().tracking = Some(fresh);
    }
}
fn register(store: &mut NonceMap, key: Key, source: bool) -> Result<NonceOwner, NonceError> {
    if !can_track(store.get(&key)) {
        return Err(NonceError::InvalidClaim);
    }
    let mut fresh = fresh_tracking(store, key);
    store.try_reserve(1).map_err(|_| NonceError::Unavailable)?;
    let tracking = match fresh.as_deref_mut() {
        Some(fresh) => fresh,
        None => store.get_mut(&key).unwrap().tracking.as_mut().unwrap(),
    };
    tracking
        .owners
        .try_reserve(1)
        .map_err(|_| NonceError::Unavailable)?;
    let owner = Arc::new(Owner {
        key,
        joined: AtomicBool::new(false),
        source,
    });
    install_tracking(store, key, fresh);
    let tracking = store.get_mut(&key).unwrap().tracking.as_mut().unwrap();
    tracking
        .owners
        .retain(|o| !o.joined.load(Ordering::Acquire));
    tracking.owners.push(owner.clone());
    Ok(NonceOwner { owner })
}
impl NonceOwner {
    /// The factory must return the original owner, including spawned-work joins.
    /// A timeout/abort wrapper's Ready is not this ownership contract.
    pub fn run<F: Future>(
        self,
        factory: impl FnOnce(LegacyNonceIssuer) -> F,
    ) -> LegacyNonceWork<F> {
        let future = factory(LegacyNonceIssuer {
            owner: self.owner.clone(),
        });
        LegacyNonceWork {
            owner: self.owner,
            future: Some(Box::pin(future)),
            finished: false,
        }
    }
}
impl<F: Future> Future for LegacyNonceWork<F> {
    type Output = F::Output;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        assert!(!this.finished, "nonce owner already joined");
        match this
            .future
            .as_mut()
            .expect("original nonce owner")
            .as_mut()
            .poll(cx)
        {
            Poll::Pending => Poll::Pending,
            Poll::Ready(value) => {
                this.finished = true;
                drop(this.future.take());
                this.owner.joined.store(true, Ordering::Release);
                Poll::Ready(value)
            }
        }
    }
}
impl LegacyNonceIssuer {
    pub fn reserve(&self, floor: u64) -> Result<LegacyNoncePermit, NonceError> {
        let mut store = lock()?;
        let state = store
            .get_mut(&self.owner.key)
            .ok_or(NonceError::InvalidClaim)?;
        let tracking = state.tracking.as_ref().ok_or(NonceError::InvalidClaim)?;
        if !self.owner.source
            || self.owner.joined.load(Ordering::Acquire)
            || tracking.phase != Phase::Open
            || !tracking.owners.iter().any(|o| Arc::ptr_eq(o, &self.owner))
        {
            return Err(NonceError::InvalidClaim);
        }
        let nonce = floor.max(state.reserved.checked_add(1).ok_or(NonceError::Exhausted)?);
        state.reserved = nonce;
        Ok(LegacyNoncePermit {
            owner: self.owner.clone(),
            nonce,
        })
    }
}
impl LegacyNoncePermit {
    pub fn nonce(&self) -> u64 {
        self.nonce
    }
    /// Physical release bookkeeping only; never changes the venue outcome.
    pub fn released(self) -> Result<(), NonceError> {
        let mut store = lock()?;
        let state = store
            .get_mut(&self.owner.key)
            .ok_or(NonceError::InvalidClaim)?;
        let tracking = state.tracking.as_ref().ok_or(NonceError::InvalidClaim)?;
        if self.owner.joined.load(Ordering::Acquire)
            || tracking.phase == Phase::Retired
            || !tracking.owners.iter().any(|o| Arc::ptr_eq(o, &self.owner))
        {
            return Err(NonceError::InvalidClaim);
        }
        state.released = Some(state.released.map_or(self.nonce, |n| n.max(self.nonce)));
        Ok(())
    }
}

pub(crate) fn epoch(
    state: Option<&NonceState>,
    owner: Option<&Arc<Owner>>,
) -> Result<Option<Arc<()>>, NonceError> {
    let Some(t) = state.and_then(|s| s.tracking.as_ref()) else {
        return Ok(None);
    };
    if t.phase != Phase::Open || t.target.is_some() || t.untracked {
        return Err(NonceError::InvalidClaim);
    }
    let owner = owner.ok_or(NonceError::InvalidClaim)?;
    if owner.source
        || owner.joined.load(Ordering::Acquire)
        || !t.owners.iter().any(|o| Arc::ptr_eq(o, owner))
    {
        return Err(NonceError::InvalidClaim);
    }
    Ok(Some(t.epoch.clone()))
}
pub(crate) fn current(
    state: Option<&NonceState>,
    wanted: Option<&Arc<()>>,
    owner: Option<&Arc<Owner>>,
) -> bool {
    epoch(state, owner).is_ok_and(|now| match (now.as_ref(), wanted) {
        (None, None) => true,
        (Some(now), Some(wanted)) => Arc::ptr_eq(now, wanted),
        _ => false,
    })
}
pub(crate) fn raw(state: &mut NonceState) -> Result<(), NonceError> {
    if let Some(t) = state.tracking.as_mut() {
        if t.phase != Phase::Open {
            return Err(NonceError::InvalidClaim);
        }
        t.untracked = true;
    }
    Ok(())
}
fn same_identity(a: &Identity, b: &Identity) -> bool {
    a.validate().is_ok()
        && a.credential == b.credential
        && a.signing_ref() == b.signing_ref()
        && a.stable_id == b.stable_id
        && a.credential_absent() == b.credential_absent()
}
impl<V: Venue> Engine<V> {
    pub(super) fn invalidate_nonce_handoff(&mut self) {
        self.io.core.nonce_handoff_issuer = None;
        for id in [
            &mut self.io.core.identities.primary,
            &mut self.io.core.identities.secondary,
        ]
        .into_iter()
        .flatten()
        {
            id.nonce_owner = None;
        }
    }
    fn nonce_identity(&self, slot: CredentialSlot) -> Result<&Identity, NonceError> {
        match slot {
            CredentialSlot::Primary => self.identities().primary.as_ref(),
            CredentialSlot::Secondary => self.identities().secondary.as_ref(),
        }
        .ok_or(NonceError::InvalidClaim)
    }
    fn nonce_engine(&mut self) -> Arc<()> {
        self.io
            .core
            .nonce_handoff_issuer
            .get_or_insert_with(|| Arc::new(()))
            .clone()
    }
    fn association_current(&self, a: &LegacyNonceAssociation) -> bool {
        self.io
            .core
            .nonce_handoff_issuer
            .as_ref()
            .is_some_and(|issuer| Arc::ptr_eq(issuer, &a.engine))
            && self
                .nonce_identity(a.slot)
                .is_ok_and(|id| same_identity(id, &a.identity))
    }
    /// Trusted, pre-start provider call. No business task or public digest is emitted.
    pub fn associate_legacy_nonce(
        &mut self,
        slot: CredentialSlot,
        domain: &'static NonceDomain,
        algorithm: Algorithm,
    ) -> Result<LegacyNonceAssociation, NonceError> {
        if self.has_started() {
            return Err(NonceError::InvalidClaim);
        }
        let identity = self.nonce_identity(slot)?.clone();
        let target = key(domain, &identity)?;
        let signing = identity
            .signing_ref()
            .map_err(|_| NonceError::InvalidClaim)?
            .clone();
        let provider = self.signer().ok_or(NonceError::InvalidClaim)?;
        if provider.can_sign(&signing, algorithm) != Capability::Supported {
            return Err(NonceError::InvalidClaim);
        }
        let seal = Arc::new(());
        let proof = provider
            .legacy_nonce_association(LegacyNonceRequest {
                key: signing,
                algorithm,
                seal: seal.clone(),
            })
            .map_err(|_| NonceError::InvalidClaim)?;
        if !Arc::ptr_eq(&seal, &proof.seal) {
            return Err(NonceError::InvalidClaim);
        }
        let source = (domain.name, proof.digest);
        if source == target {
            return Err(NonceError::InvalidClaim);
        }
        let mut store = lock()?;
        if !can_track(store.get(&source)) || !can_track(store.get(&target)) {
            return Err(NonceError::InvalidClaim);
        }
        if store
            .get(&source)
            .and_then(|s| s.tracking.as_ref())
            .is_some_and(|t| !t.sources.is_empty())
        {
            return Err(NonceError::InvalidClaim);
        }
        if store
            .get(&source)
            .and_then(|s| s.tracking.as_ref())
            .and_then(|t| t.target)
            .is_some_and(|t| t != target)
            || store
                .get(&target)
                .and_then(|s| s.tracking.as_ref())
                .is_some_and(|t| t.target.is_some())
        {
            return Err(NonceError::InvalidClaim);
        }
        store.try_reserve(2).map_err(|_| NonceError::Unavailable)?;
        let fresh_source = fresh_tracking(&store, source);
        let mut fresh_target = fresh_tracking(&store, target);
        let target_tracking = match fresh_target.as_deref_mut() {
            Some(fresh) => fresh,
            None => store.get_mut(&target).unwrap().tracking.as_mut().unwrap(),
        };
        if !target_tracking.sources.contains(&source) {
            target_tracking
                .sources
                .try_reserve(1)
                .map_err(|_| NonceError::Unavailable)?;
        }
        install_tracking(&mut store, source, fresh_source);
        install_tracking(&mut store, target, fresh_target);
        store
            .get_mut(&source)
            .unwrap()
            .tracking
            .as_mut()
            .unwrap()
            .target = Some(target);
        let sources = &mut store
            .get_mut(&target)
            .unwrap()
            .tracking
            .as_mut()
            .unwrap()
            .sources;
        if !sources.contains(&source) {
            sources.push(source);
        }
        drop(store);
        Ok(LegacyNonceAssociation {
            engine: self.nonce_engine(),
            slot,
            identity,
            domain,
            source,
            target,
        })
    }
    /// Register before any target work; retain this generation through real cleanup.
    pub fn register_nonce_owner(
        &mut self,
        slot: CredentialSlot,
        domain: &'static NonceDomain,
    ) -> Result<NonceOwner, NonceError> {
        if self.has_started() {
            return Err(NonceError::InvalidClaim);
        }
        let target = key(domain, self.nonce_identity(slot)?)?;
        let owner = register(&mut *lock()?, target, false)?;
        let identities = &mut self.io.core.identities;
        let id = match slot {
            CredentialSlot::Primary => identities.primary.as_mut(),
            CredentialSlot::Secondary => identities.secondary.as_mut(),
        }
        .ok_or(NonceError::InvalidClaim)?;
        id.nonce_owner = Some(owner.owner.clone());
        Ok(owner)
    }
    pub fn register_legacy_nonce_owner(
        &mut self,
        a: &LegacyNonceAssociation,
    ) -> Result<NonceOwner, NonceError> {
        if self.has_started() || !self.association_current(a) {
            return Err(NonceError::InvalidClaim);
        }
        register(&mut *lock()?, a.source, true)
    }
    pub fn fence_nonce_handoff(
        &mut self,
        associations: &[&LegacyNonceAssociation],
    ) -> Result<NonceTakeover, NonceError> {
        if self.has_started() || associations.is_empty() {
            return Err(NonceError::InvalidClaim);
        }
        let first = associations[0];
        if associations.iter().any(|a| {
            !self.association_current(a) || a.target != first.target || a.domain != first.domain
        }) {
            return Err(NonceError::InvalidClaim);
        }
        let mut store = lock()?;
        let target = store
            .get(&first.target)
            .and_then(|s| s.tracking.as_ref())
            .ok_or(NonceError::InvalidClaim)?;
        if target.sources.len() != associations.len()
            || target
                .sources
                .iter()
                .any(|s| associations.iter().filter(|a| a.source == *s).count() != 1)
        {
            return Err(NonceError::InvalidClaim);
        }
        for k in std::iter::once(first.target).chain(associations.iter().map(|a| a.source)) {
            let t = store
                .get(&k)
                .and_then(|s| s.tracking.as_ref())
                .ok_or(NonceError::InvalidClaim)?;
            if t.untracked
                || t.phase != Phase::Open
                || k != first.target && (t.target != Some(first.target) || t.owners.is_empty())
            {
                return Err(NonceError::InvalidClaim);
            }
        }
        let copies = associations
            .iter()
            .map(|a| LegacyNonceAssociation {
                engine: a.engine.clone(),
                slot: a.slot,
                identity: a.identity.clone(),
                domain: a.domain,
                source: a.source,
                target: a.target,
            })
            .collect();
        let epoch = Arc::new(());
        for k in std::iter::once(first.target).chain(associations.iter().map(|a| a.source)) {
            let t = store.get_mut(&k).unwrap().tracking.as_mut().unwrap();
            t.epoch = epoch.clone();
            t.phase = Phase::Fenced;
        }
        Ok(NonceTakeover {
            engine: first.engine.clone(),
            epoch,
            associations: copies,
            committed: false,
        })
    }
    pub fn commit_nonce_handoff(&mut self, takeover: &mut NonceTakeover) -> Result<(), NonceError> {
        if self.has_started()
            || takeover.committed
            || !self
                .io
                .core
                .nonce_handoff_issuer
                .as_ref()
                .is_some_and(|i| Arc::ptr_eq(i, &takeover.engine))
            || takeover
                .associations
                .iter()
                .any(|a| !self.association_current(a))
        {
            return Err(NonceError::InvalidClaim);
        }
        let mut store = lock()?;
        let first = &takeover.associations[0];
        let keys =
            std::iter::once(first.target).chain(takeover.associations.iter().map(|a| a.source));
        let mut reserved = 0;
        let mut released = None;
        for k in keys.clone() {
            let s = store.get(&k).ok_or(NonceError::InvalidClaim)?;
            let t = s.tracking.as_ref().ok_or(NonceError::InvalidClaim)?;
            if s.released.is_some_and(|released| released > s.reserved) {
                return Err(NonceError::InvalidClaim);
            }
            if t.untracked
                || t.phase != Phase::Fenced
                || !Arc::ptr_eq(&t.epoch, &takeover.epoch)
                || t.owners.iter().any(|o| !o.joined.load(Ordering::Acquire))
            {
                return Err(NonceError::InvalidClaim);
            }
            reserved = reserved.max(s.reserved);
            released = match (released, s.released) {
                (None, value) | (value, None) => value,
                (Some(a), Some(b)) => Some(a.max(b)),
            };
        }
        if reserved >= first.domain.limit {
            return Err(NonceError::Exhausted);
        }
        // Qualified original owners joined, so even an unused legacy reservation
        // is conservatively included in the released floor.
        for a in &takeover.associations {
            released = Some(released.unwrap_or(0).max(store[&a.source].reserved));
        }
        let epoch = Arc::new(());
        for k in keys {
            let t = store.get_mut(&k).unwrap().tracking.as_mut().unwrap();
            t.phase = if k == first.target {
                Phase::Open
            } else {
                Phase::Retired
            };
            t.epoch = epoch.clone();
        }
        let target = store.get_mut(&first.target).unwrap();
        target.reserved = reserved;
        target.released = released;
        takeover.committed = true;
        Ok(())
    }
}
#[cfg(test)]
#[path = "nonce_handoff_tests.rs"]
mod tests;

impl<P> super::Cx<'_, P> {
    /// Current installed nonce identities; construction clones carry no new owner.
    pub fn nonce_identities(&self) -> &crate::signing::Identities {
        &self.io.core.identities
    }
}
