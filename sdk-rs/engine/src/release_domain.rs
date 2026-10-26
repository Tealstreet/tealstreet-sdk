//! Venue release domains (docs/plans/RUST_RELEASE_DOMAINS.md): one FIFO of
//! managed REST members in acceptance order. Only the head may Bind; its Claim
//! waits out the registered gap from the previous member's final release.
use super::*;
use crate::request_context::{DomainMembers, Error, Operation, Pacing, ReleaseDomain};

/// Members are managed requests, so the managed limit bounds the queue and
/// the held Binds; both are reserved once at construction.
pub(super) const CAPACITY: usize = 256;

/// A domain must name a registered group at exactly its registered gap.
pub(super) fn validate(domain: ReleaseDomain, registered: &[Pacing]) -> Result<(), Error> {
    if registered.contains(&domain.pacing) {
        Ok(())
    } else {
        Err(Error::Invalid)
    }
}

/// A non-head member's latest Bind. It may carry signed headers, a body or a
/// Session bearer, so every exit other than its release wipes it.
pub(super) struct Held(Option<Action>);

impl Held {
    fn release(mut self) -> Action {
        self.0.take().expect("held Bind")
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        if let Some(action) = self.0.as_mut() {
            crate::signing_dispatch::wipe(action);
        }
    }
}

/// The queue and held storage, reserved at construction for a declared
/// domain, so acceptance never allocates.
pub(super) type Storage = (
    std::collections::VecDeque<RequestId>,
    Vec<(RequestId, Held)>,
);
pub(super) fn reserve(declared: bool) -> Result<Storage, Error> {
    let (mut queue, mut held) = Storage::default();
    if declared {
        queue
            .try_reserve_exact(CAPACITY)
            .map_err(|_| Error::Capacity)?;
        held.try_reserve_exact(CAPACITY)
            .map_err(|_| Error::Capacity)?;
    }
    Ok((queue, held))
}

impl IoCore {
    pub(super) fn domain_member(&self, operation: Operation) -> Option<Pacing> {
        let domain = self.release_domain?;
        match domain.members {
            DomainMembers::AllRest => Some(domain.pacing),
            DomainMembers::Mutations => (operation == Operation::Mutation).then_some(domain.pacing),
        }
    }

    /// The pacing a member Context carries: the domain's own. A member naming
    /// any other pacing would bypass or split the domain, so it is refused.
    pub(super) fn domain_pacing(
        &self,
        operation: Operation,
        pacing: Option<Pacing>,
    ) -> Result<Option<Pacing>, Error> {
        match (self.domain_member(operation), pacing) {
            (None, pacing) => Ok(pacing),
            (Some(domain), None) => Ok(Some(domain)),
            (Some(domain), Some(own)) if own == domain => Ok(Some(own)),
            (Some(_), Some(_)) => Err(Error::Invalid),
        }
    }

    /// The managed boundary's check, before any ID is spent. A member must
    /// already carry the domain pacing from whichever path built its Context,
    /// must not be an atomic group member (a group releases together, so it
    /// cannot take one FIFO turn per member), and must fit the queue.
    pub(super) fn domain_admits(
        &self,
        operation: Operation,
        pacing: Option<Pacing>,
        grouped: bool,
    ) -> Result<(), Error> {
        let Some(domain) = self.domain_member(operation) else {
            return Ok(());
        };
        if pacing != Some(domain) || grouped {
            return Err(Error::Invalid);
        }
        if self.domain_queue.len() >= CAPACITY {
            return Err(Error::Capacity);
        }
        Ok(())
    }

    /// The one linearization point: an accepted physical request joins the tail.
    pub(super) fn domain_accept(&mut self, request: RequestId, pacing: Option<Pacing>) {
        if self
            .release_domain
            .is_some_and(|domain| pacing == Some(domain.pacing))
        {
            debug_assert!(self.domain_queue.len() < CAPACITY);
            self.domain_queue.push_back(request);
        }
    }

    /// A prepared member binds only as the domain head; any other holds its
    /// latest Bind until the members ahead of it are released or gone.
    pub(super) fn domain_bind(&mut self, request: RequestId, bind: Action) -> Option<Action> {
        if !self.domain_queue.contains(&request) || self.domain_queue.front() == Some(&request) {
            return Some(bind);
        }
        match self
            .domain_held
            .iter_mut()
            .find(|(held, _)| *held == request)
        {
            // The replaced Bind is wiped as its holder drops.
            Some((_, held)) => *held = Held(Some(bind)),
            None => {
                debug_assert!(self.domain_held.len() < CAPACITY);
                self.domain_held.push((request, Held(Some(bind))));
            }
        }
        None
    }

    /// The member was released or reached any terminal: it leaves the queue,
    /// and a held next head binds.
    pub(super) fn domain_advance(&mut self, request: RequestId) {
        let Some(index) = self
            .domain_queue
            .iter()
            .position(|member| *member == request)
        else {
            return;
        };
        self.domain_queue.remove(index);
        self.domain_held.retain(|(held, _)| *held != request);
        let Some(&head) = self.domain_queue.front() else {
            return;
        };
        if let Some(index) = self.domain_held.iter().position(|(held, _)| *held == head) {
            let (_, bind) = self.domain_held.remove(index);
            self.actions.push(bind.release());
        }
    }
}
