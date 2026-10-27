//! Trusted selected subject and private reservation records; no public mint.
use super::{NONCES, NonceDomain, NonceError, NonceKey};
use crate::credential_selection::Selection;
use crate::request_context::Credential;
use crate::signing::{Identity, KeyRef, SignError};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

/// Opaque caller-assigned namespace. It cannot be constructed from venue text.
pub struct NonceSubject(NonceKey);

/// One selected recipe authority. Intentionally not Clone or serializable.
pub struct NonceClaim {
    authority: Arc<ClaimAuthority>,
}

pub(crate) struct ClaimAuthority {
    pub(crate) domain: &'static NonceDomain,
    subject: NonceSubject,
    pub(crate) selection: Selection,
    pub(crate) credential: Credential,
    pub(crate) signing: KeyRef,
    /// From the trusted identity's `credential_absent()`: nothing is presented.
    pub(crate) signing_only: bool,
    bound: AtomicBool,
    epoch: Option<Arc<()>>,
    owner: Option<Arc<super::handoff::Owner>>,
    #[cfg(feature = "address-nonce")]
    address: Option<crate::engine::address_nonce_setup::VerifiedAddress>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NonceSelectionError {
    MissingCredential,
    InvalidSubject,
    MissingSigningKey,
    SigningNotConfigured,
    SetupCapability(SignError),
}
impl NonceSelectionError {
    pub fn sign_error(self) -> SignError {
        match self {
            Self::MissingSigningKey => SignError::MissingKey,
            Self::SigningNotConfigured => SignError::SigningNotConfigured,
            Self::SetupCapability(error) => error,
            Self::MissingCredential | Self::InvalidSubject => SignError::InvalidRequest,
        }
    }
}

impl NonceClaim {
    /// Metadata from the selected identity; never an input for key selection.
    pub fn slot(&self) -> crate::CredentialSlot {
        self.authority.selection.slot()
    }

    pub(crate) fn mint(
        domain: &'static NonceDomain,
        selection: Selection,
        identity: &Identity,
    ) -> Result<Self, NonceSelectionError> {
        let address_domain = domain.subject == super::NonceSubjectStrategy::Address;
        if address_domain {
            identity.signing_ref().map_err(|error| match error {
                SignError::SigningNotConfigured => NonceSelectionError::SigningNotConfigured,
                _ => NonceSelectionError::MissingSigningKey,
            })?;
        }
        identity
            .validate()
            .map_err(|_| NonceSelectionError::InvalidSubject)?;
        #[cfg(feature = "address-nonce")]
        let address = if address_domain {
            Some(
                identity
                    .address
                    .verified(identity, selection.slot(), Some(domain))?,
            )
        } else {
            None
        };
        let subject = if address_domain {
            #[cfg(feature = "address-nonce")]
            {
                super::NonceKey::ethereum_address(&address.as_ref().unwrap().hex())
            }
            #[cfg(not(feature = "address-nonce"))]
            {
                return Err(NonceSelectionError::InvalidSubject);
            }
        } else {
            NonceKey::stable_id(
                identity
                    .require_stable_id()
                    .map_err(|_| NonceSelectionError::InvalidSubject)?,
            )
        };
        let owner = if address_domain {
            None
        } else {
            identity.nonce_owner.clone()
        };
        let epoch = {
            let store = NONCES
                .get_or_init(Mutex::default)
                .lock()
                .map_err(|_| NonceSelectionError::InvalidSubject)?;
            super::handoff::epoch(store.get(&(domain.name, subject.0)), owner.as_ref())
                .map_err(|_| NonceSelectionError::InvalidSubject)?
        };
        Ok(Self {
            authority: Arc::new(ClaimAuthority {
                epoch,
                owner,
                domain,
                subject: NonceSubject(subject),
                #[cfg(feature = "address-nonce")]
                address,
                selection,
                credential: selection.authority(identity),
                signing_only: identity.credential_absent(),
                signing: identity
                    .signing_ref()
                    .map_err(|_| NonceSelectionError::InvalidSubject)?
                    .clone(),
                bound: AtomicBool::new(false),
            }),
        })
    }
    pub(crate) fn authority(&self) -> Arc<ClaimAuthority> {
        self.authority.clone()
    }
}

pub(crate) struct NonceReservation {
    pub(crate) authority: Arc<ClaimAuthority>,
    pub(crate) nonce: u64,
}
impl ClaimAuthority {
    pub(crate) fn matches_identity(&self, identity: &Identity) -> bool {
        if identity.validate().is_err()
            || self.credential.reference != *identity.authority_ref()
            || self.signing_only != identity.credential_absent()
            || !identity.signing_ref().is_ok_and(|key| *key == self.signing)
        {
            return false;
        }
        let owner_current = if self.domain.subject == super::NonceSubjectStrategy::Address {
            self.owner.is_none()
        } else {
            match (self.owner.as_ref(), identity.nonce_owner.as_ref()) {
                (None, None) => true,
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                _ => false,
            }
        };
        if !owner_current
            || !NONCES
                .get_or_init(Mutex::default)
                .lock()
                .is_ok_and(|store| {
                    super::handoff::current(
                        store.get(&(self.domain.name, self.subject.0.0)),
                        self.epoch.as_ref(),
                        self.owner.as_ref(),
                    )
                })
        {
            return false;
        }
        match self.domain.subject {
            super::NonceSubjectStrategy::StableId => identity
                .require_stable_id()
                .is_ok_and(|id| self.subject.0 == NonceKey::stable_id(id)),
            super::NonceSubjectStrategy::Address => {
                #[cfg(feature = "address-nonce")]
                {
                    self.address.as_ref().is_some_and(|address| {
                        identity
                            .address
                            .verified(identity, self.selection.slot(), Some(self.domain))
                            .is_ok_and(|current| current == *address)
                    })
                }
                #[cfg(not(feature = "address-nonce"))]
                {
                    false
                }
            }
        }
    }

    pub(crate) fn bind_once(&self) -> Result<(), SignError> {
        self.bound
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| ())
            .map_err(|_| SignError::InvalidRequest)
    }

    pub(crate) fn reserve(
        self: &Arc<Self>,
        raw_now: f64,
        protocol_now: f64,
    ) -> Result<NonceReservation, NonceError> {
        // Receipt timestamps remain safe integers even when the scaled nonce is larger.
        if !raw_now.is_finite() || !(0. ..=9_007_199_254_740_991.).contains(&raw_now) {
            return Err(NonceError::InvalidTime);
        }
        let (ms, scale) = match self.domain.floor() {
            super::NonceFloor::RawMs { scale } => (raw_now, scale),
            super::NonceFloor::ProtocolMs { scale } => (protocol_now, scale),
        };
        let floor = super::floor(ms, scale, self.domain.limit)?;
        let nonce = super::reserve_claim(
            self.domain.name,
            self.subject.0.0,
            floor,
            self.domain.limit,
            self.epoch.as_ref(),
            self.owner.as_ref(),
        )?;
        Ok(NonceReservation {
            authority: self.clone(),
            nonce,
        })
    }
}
impl NonceReservation {
    /// An epoch revocation is terminal; ordinary nonce supersession may reprepare.
    pub(crate) fn fresh(&self) -> Result<bool, NonceError> {
        let store = NONCES
            .get_or_init(Mutex::default)
            .lock()
            .map_err(|_| NonceError::Unavailable)?;
        if !super::handoff::current(
            store.get(&(self.authority.domain.name, self.authority.subject.0.0)),
            self.authority.epoch.as_ref(),
            self.authority.owner.as_ref(),
        ) {
            return Err(NonceError::InvalidClaim);
        }
        Ok(store
            .get(&(self.authority.domain.name, self.authority.subject.0.0))
            .is_some_and(|state| {
                self.nonce <= state.reserved
                    && state.released.is_none_or(|floor| self.nonce > floor)
            }))
    }

    /// The engine supplies only its private infallible Lifecycle/pacing commit.
    pub(crate) fn commit_release(&self, commit: impl FnOnce()) -> Result<bool, NonceError> {
        let mut store = NONCES
            .get_or_init(Mutex::default)
            .lock()
            .map_err(|_| NonceError::Unavailable)?;
        if !super::handoff::current(
            store.get(&(self.authority.domain.name, self.authority.subject.0.0)),
            self.authority.epoch.as_ref(),
            self.authority.owner.as_ref(),
        ) {
            return Ok(false);
        }
        let Some(state) = store.get_mut(&(self.authority.domain.name, self.authority.subject.0.0))
        else {
            return Err(NonceError::Unavailable);
        };
        if self.nonce > state.reserved || state.released.is_some_and(|floor| self.nonce <= floor) {
            return Ok(false);
        }
        commit();
        state.released = Some(
            state
                .released
                .map_or(self.nonce, |floor| floor.max(self.nonce)),
        );
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credential_selection::{CredentialRole, SlotPolicy, Slots};
    use crate::nonce::KRAKEN;
    fn identity(stable: &str, signing: &str) -> Identity {
        Identity::new(
            KeyRef(format!("credential/{signing}")),
            Some(KeyRef(signing.into())),
        )
        .with_stable_id(Some(stable.into()))
    }
    fn selected(stable: &str) -> Arc<ClaimAuthority> {
        Slots::new(
            Some(identity(stable, "primary")),
            None,
            SlotPolicy::default(),
        )
        .select_with_nonce(CredentialRole::Mutation, &KRAKEN)
        .unwrap()
        .2
        .authority()
    }
    #[test]
    fn same_stable_id_with_different_signing_refs_and_slots_shares_one_sequence() {
        let slots = Slots::new(
            Some(identity("nonce-subject/alias", "primary")),
            Some(identity("nonce-subject/alias", "secondary")),
            SlotPolicy::default(),
        );
        let (p, pi, pc) = slots
            .select_with_nonce(CredentialRole::Mutation, &KRAKEN)
            .unwrap();
        let (s, si, sc) = slots
            .select_with_nonce(CredentialRole::Read { scope: None }, &KRAKEN)
            .unwrap();
        assert_ne!(p.slot(), s.slot());
        assert_eq!(pc.slot(), p.slot());
        assert_eq!(sc.slot(), s.slot());
        assert_ne!(pi.signing_ref(), si.signing_ref());
        let a = pc.authority().reserve(1000., 9000.).unwrap();
        let b = sc.authority().reserve(999., 99000.).unwrap();
        assert_eq!((a.nonce, b.nonce), (1000000, 1000001));
    }
    #[test]
    fn existing_counter_is_exact_for_regression_more_than_one_thousand_and_exhaustion() {
        let claim = selected("nonce-subject/arithmetic");
        let mut previous = claim.reserve(1000., 1000.).unwrap().nonce;
        for _ in 0..2002 {
            let nonce = claim.reserve(999., 999.).unwrap().nonce;
            assert_eq!(nonce, previous + 1);
            previous = nonce;
        }
        let key = (claim.domain.name, claim.subject.0.0);
        NONCES
            .get()
            .unwrap()
            .lock()
            .unwrap()
            .get_mut(&key)
            .unwrap()
            .reserved = 9_007_199_254_740_992;
        let large = claim.reserve(0., 0.).unwrap().nonce;
        assert_eq!(large.to_string(), "9007199254740993");
        NONCES
            .get()
            .unwrap()
            .lock()
            .unwrap()
            .get_mut(&key)
            .unwrap()
            .reserved = u64::MAX - 1;
        assert_eq!(claim.reserve(0., 0.).unwrap().nonce, u64::MAX);
        assert!(matches!(claim.reserve(0., 0.), Err(NonceError::Exhausted)));
    }
    #[test]
    fn reservation_alone_never_supersedes_and_floor_commit_is_max_only() {
        let claim = selected("nonce-subject/release-floor");
        let a = claim.reserve(1000., 1000.).unwrap();
        let b = claim.reserve(1000., 1000.).unwrap();
        assert!(a.fresh().unwrap());
        assert!(a.commit_release(|| {}).unwrap());
        assert!(
            !a.commit_release(|| panic!("duplicate cannot commit"))
                .unwrap()
        );
        assert!(b.commit_release(|| {}).unwrap());
        assert!(!a.fresh().unwrap());
        assert!(!a.commit_release(|| panic!("older cannot commit")).unwrap());
    }
    #[test]
    fn a_claim_cannot_be_bound_to_two_physical_requests() {
        let claim = selected("nonce-subject/one-use");
        assert!(claim.bind_once().is_ok());
        assert_eq!(claim.bind_once(), Err(SignError::InvalidRequest));
    }
    #[test]
    fn missing_invalid_subjects_and_unsafe_raw_clock_refuse() {
        for stable in [None, Some("".into()), Some("bad\nsubject".into())] {
            let mut i = identity("unused", "primary");
            i.stable_id = stable;
            let slots = Slots::new(Some(i), None, SlotPolicy::default());
            assert_eq!(
                slots
                    .select_with_nonce(CredentialRole::Mutation, &KRAKEN)
                    .err(),
                Some(NonceSelectionError::InvalidSubject)
            );
        }
        let claim = selected("nonce-subject/invalid-clock");
        for bad in [f64::NAN, f64::INFINITY, -1., 9_007_199_254_740_992.] {
            assert!(matches!(
                claim.reserve(bad, 1000.),
                Err(NonceError::InvalidTime)
            ));
        }
        assert_eq!(claim.reserve(1000., 1000.).unwrap().nonce, 1000000);
    }
}
