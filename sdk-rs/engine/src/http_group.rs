//! Immutable Plan-group preparation; no member can escape independently.
use super::*;
use crate::request_context::{GroupPacket, GroupPermit};
use request_context_registry::MutationGroupKey;

pub(super) struct Member {
    pub(super) key: MutationGroupKey,
    pub(super) packet: Option<HttpRequest>,
    pub(super) permit: Option<Permit>,
    pub(super) announced: bool,
    pub(super) bound: bool,
}
impl Member {
    pub(super) fn new(key: MutationGroupKey) -> Self {
        Self {
            key,
            packet: None,
            permit: None,
            announced: false,
            bound: false,
        }
    }
    pub(super) fn reset(&mut self) {
        if let Some(packet) = self.packet.take() {
            crate::signing_dispatch::wipe(&mut Action::Http(packet));
        }
        self.permit = None;
        self.announced = false;
        self.bound = false;
    }
}
impl Drop for Member {
    fn drop(&mut self) {
        self.reset();
    }
}

#[path = "http_group_claim.rs"]
mod claim;

impl<V: Venue> Engine<V> {
    fn mutation_group_members(&self, request: RequestId) -> Result<Option<Vec<RequestId>>, Error> {
        let managed = self.io.managed_http.get(&request).ok_or(Error::Revoked)?;
        let key = managed.group.as_ref().ok_or(Error::State)?.key;
        let mut ids = Vec::new();
        ids.try_reserve_exact(key.count)
            .map_err(|_| Error::Capacity)?;
        for (&id, managed) in &self.io.managed_http {
            if let Some(member) = &managed.group
                && member.key.same_group(key)
            {
                if ids.len() == key.count {
                    return Err(Error::Invalid);
                }
                ids.push(id);
            }
        }
        if ids.len() != key.count {
            return Ok(None);
        }
        ids.sort_unstable_by_key(|id| self.io.managed_http[id].group.as_ref().unwrap().key.ordinal);
        for (ordinal, id) in ids.iter().enumerate() {
            let managed = &self.io.managed_http[id];
            let member = managed.group.as_ref().unwrap();
            if member.key.ordinal != ordinal
                || self
                    .io
                    .request_contexts
                    .mutation_group_key(managed.lifecycle.context())?
                    != Some(member.key)
            {
                return Err(Error::Revoked);
            }
        }
        Ok(Some(ids))
    }

    pub(super) fn pump_mutation_group(&mut self, now: f64, request: RequestId) {
        let Some(managed) = self.io.managed_http.get(&request) else {
            return;
        };
        let token = managed.lifecycle.preparation();
        let result = self.prepare_mutation_group(now, request);
        if let Err((failed, error)) = result {
            let preparation = self
                .io
                .managed_http
                .get(&failed)
                .map_or(token, |managed| managed.lifecycle.preparation());
            self.http_preparation_failed(
                now,
                preparation,
                error,
                VenueError::new("HTTP group preparation failed"),
            );
        }
    }

    fn prepare_mutation_group(
        &mut self,
        now: f64,
        request: RequestId,
    ) -> Result<(), (RequestId, Error)> {
        let Some(ids) = self
            .mutation_group_members(request)
            .map_err(|error| (request, error))?
        else {
            return Ok(());
        };
        for id in &ids {
            let managed = &self.io.managed_http[id];
            managed
                .validate(&self.io.request_contexts, now)
                .map_err(|error| (*id, error))?;
            if managed
                .lifecycle
                .expects_preparation(managed.lifecycle.preparation())
            {
                return Ok(());
            }
        }
        let mut activation = Vec::new();
        activation
            .try_reserve_exact(ids.len())
            .map_err(|_| (request, Error::Capacity))?;
        for id in &ids {
            let managed = &self.io.managed_http[id];
            if managed.lifecycle.group_needs_activation() {
                managed
                    .lifecycle
                    .validate_group_activation(now)
                    .map_err(|error| (*id, error))?;
                if let Some(attempt) = managed.signing_attempt {
                    self.io
                        .signing
                        .validate_managed_activation(attempt, managed.lifecycle.deadline())
                        .map_err(|_| (*id, Error::State))?;
                    activation.push((*id, None));
                } else {
                    let mut packet = managed.original.clone();
                    packet.timeout_ms =
                        Some((managed.lifecycle.deadline() - now).ceil().max(1.) as u32);
                    activation.push((*id, Some(packet)));
                }
            }
        }
        // All routes and activation storage/checks precede declaration-order work.
        for (id, packet) in activation {
            let managed = self.io.managed_http.get_mut(&id).expect("validated member");
            if let Some(attempt) = managed.signing_attempt {
                managed
                    .lifecycle
                    .begin_signing()
                    .expect("validated route phase");
                self.io
                    .signing
                    .activate_managed(attempt, managed.lifecycle.deadline())
                    .expect("validated signing phase");
            } else {
                managed
                    .lifecycle
                    .unsigned()
                    .expect("validated unsigned phase");
                let permit = managed.lifecycle.permit().expect("validated permit space");
                let member = managed.group.as_mut().unwrap();
                member.packet = packet;
                member.permit = Some(permit);
            }
        }
        self.announce_mutation_group(ids)
            .map_err(|error| (request, error))
    }

    fn announce_mutation_group(&mut self, ids: Vec<RequestId>) -> Result<(), Error> {
        let mut announce = false;
        for id in &ids {
            let managed = &self.io.managed_http[id];
            let member = managed.group.as_ref().unwrap();
            let Some(permit) = member.permit else {
                return Ok(());
            };
            if !managed.lifecycle.is_current_permit(permit)
                || (member.packet.is_none() && !member.bound)
            {
                return Ok(());
            }
            announce |= !member.announced;
        }
        if !announce {
            return Ok(());
        }
        let mut permits = Vec::new();
        permits
            .try_reserve_exact(ids.len())
            .map_err(|_| Error::Capacity)?;
        permits.extend(ids.iter().map(|id| {
            self.io.managed_http[id]
                .group
                .as_ref()
                .unwrap()
                .permit
                .unwrap()
        }));
        let permit = GroupPermit::new(permits)?;
        let mut members = Vec::new();
        members
            .try_reserve_exact(ids.len())
            .map_err(|_| Error::Capacity)?;
        self.io
            .actions
            .try_reserve(1)
            .map_err(|_| Error::Capacity)?;
        let mut action = Box::new(HttpAction::BindGroup { permit, members });
        let HttpAction::BindGroup { members, .. } = action.as_mut() else {
            unreachable!()
        };
        for id in ids {
            let managed = self.io.managed_http.get_mut(&id).unwrap();
            let member = managed.group.as_mut().unwrap();
            members.push(GroupPacket {
                preparation: managed.lifecycle.preparation(),
                request: member.packet.take(),
            });
            member.announced = true;
        }
        self.io.actions.push(Action::ManagedHttp(action));
        Ok(())
    }
}
