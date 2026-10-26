//! Venue release domains (docs/plans/RUST_RELEASE_DOMAINS.md): one FIFO of
//! managed REST members in acceptance order. Only the head may Bind; its Claim
//! waits out the registered gap from the previous member's final release.
use super::*;
use crate::request_context::{DomainMembers, Error, Operation, Pacing, ReleaseDomain};

/// A domain must name a registered group at exactly its registered gap.
pub(super) fn validate(domain: ReleaseDomain, registered: &[Pacing]) -> Result<(), Error> {
    if registered.contains(&domain.pacing) {
        Ok(())
    } else {
        Err(Error::Invalid)
    }
}

impl IoCore {
    fn domain_member(&self, operation: Operation) -> Option<Pacing> {
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

    /// The one linearization point: an accepted physical request joins the tail.
    pub(super) fn domain_accept(&mut self, request: RequestId, pacing: Option<Pacing>) {
        if self
            .release_domain
            .is_some_and(|domain| pacing == Some(domain.pacing))
        {
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
            Some((_, held)) => *held = bind,
            None => self.domain_held.push((request, bind)),
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
            self.actions.push(bind);
        }
    }
}
