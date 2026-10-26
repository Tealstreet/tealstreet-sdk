//! One synchronous group claim; all fallible work precedes every release.
use super::*;
use crate::request_context::group_lifecycle::{ClaimPreparation, PreparedClaim};
use crate::request_context::{GroupClaim, GroupPermit, GroupRelease};

struct PreparedMember {
    request: RequestId,
    context: Context,
    claim: Option<PreparedClaim>,
    observation: crate::SigningRelease,
    retained_recipe: Option<crate::signing_dispatch::Pending>,
}

impl<V: Venue> Engine<V> {
    /// Invalid or retired tokens are inert; they cannot cancel live siblings.
    fn current_group(&self, permit: &GroupPermit) -> Result<(), Error> {
        let head = permit.members().first().ok_or(Error::Invalid)?;
        let managed = self
            .io
            .core
            .managed_http
            .get(&head.attempt().request)
            .ok_or(Error::Revoked)?;
        let key = managed.group.as_ref().ok_or(Error::Revoked)?.key;
        if key.ordinal != 0 || key.count != permit.members().len() {
            return Err(Error::Revoked);
        }
        for (ordinal, &token) in permit.members().iter().enumerate() {
            let managed = self
                .io
                .core
                .managed_http
                .get(&token.attempt().request)
                .ok_or(Error::Revoked)?;
            let member = managed.group.as_ref().ok_or(Error::Revoked)?;
            if !member.key.same_group(key)
                || member.key.ordinal != ordinal
                || !member.announced
                || member.permit != Some(token)
                || !managed.lifecycle.is_current_permit(token)
                || self
                    .io
                    .core
                    .request_contexts
                    .mutation_group_key(managed.lifecycle.context())?
                    != Some(member.key)
            {
                return Err(Error::Revoked);
            }
        }
        Ok(())
    }

    pub fn http_group_pending(&self, permit: &GroupPermit) -> bool {
        if !V::http_mutation_barriers() {
            return false;
        }
        !self.failed() && self.current_group(permit).is_ok()
    }

    pub fn claim_http_group(&mut self, now: f64, permit: GroupPermit) -> Result<GroupClaim, Error> {
        if !V::http_mutation_barriers() {
            return Err(Error::Invalid);
        }
        self.assert_live();
        // Validate complete membership before any expiry or failure callback.
        self.current_group(&permit)?;
        if !now.is_finite() {
            return Err(Error::Invalid);
        }
        let mut ids = Vec::new();
        if ids.try_reserve_exact(permit.members().len()).is_err() {
            let head = permit.members()[0].attempt().request;
            let preparation = self.io.core.managed_http[&head].lifecycle.preparation();
            self.http_preparation_failed(
                now,
                preparation,
                Error::Capacity,
                VenueError::new("HTTP group storage unavailable"),
            );
            return Err(Error::Capacity);
        }
        ids.extend(permit.members().iter().map(|p| p.attempt().request));
        self.io.core.dispatch_now = now;
        self.expire_read_roots(now);
        self.current_group(&permit)?;
        // A current group claim is reached only after the host bound all packets.
        for id in &ids {
            self.io
                .core
                .managed_http
                .get_mut(id)
                .unwrap()
                .group
                .as_mut()
                .unwrap()
                .bound = true;
        }
        let time = ReleaseTime {
            raw_ms: now,
            protocol_ms: self.io.core.protocol_clock.at(now),
            revision: self.io.core.protocol_clock.revision(),
        };
        let mut drafts = Vec::new();
        if drafts.try_reserve_exact(ids.len()).is_err() {
            let preparation = self.io.core.managed_http[&ids[0]].lifecycle.preparation();
            self.http_preparation_failed(
                now,
                preparation,
                Error::Capacity,
                VenueError::new("HTTP group storage unavailable"),
            );
            return Err(Error::Capacity);
        }
        let mut wait: Option<f64> = None;
        let mut earliest = f64::MAX;
        for (&request, &token) in ids.iter().zip(permit.members()) {
            let managed = self
                .io
                .core
                .managed_http
                .get(&request)
                .expect("checked member");
            let result = managed
                .validate(&self.io.core.request_contexts, now)
                .and_then(|()| {
                    managed
                        .lifecycle
                        .prepare_claim(token, time, &self.io.core.http_pacing)
                });
            earliest = earliest.min(managed.lifecycle.deadline());
            match result {
                Ok(ClaimPreparation::Ready(draft)) => drafts.push((request, draft)),
                Ok(ClaimPreparation::WaitUntil(at)) => {
                    wait = Some(wait.map_or(at, |old| old.max(at)))
                }
                Err(reason) => {
                    let preparation = self.io.core.managed_http[&request].lifecycle.preparation();
                    self.http_preparation_failed(
                        now,
                        preparation,
                        reason,
                        VenueError::new("HTTP group was not released"),
                    );
                    return Err(reason);
                }
            }
        }
        if let Some(at) = wait {
            if at < earliest {
                return Ok(GroupClaim::WaitUntil(at));
            }
            let request = *ids
                .iter()
                .min_by(|a, b| {
                    self.io.core.managed_http[a]
                        .lifecycle
                        .deadline()
                        .total_cmp(&self.io.core.managed_http[b].lifecycle.deadline())
                })
                .unwrap();
            let preparation = self.io.core.managed_http[&request].lifecycle.preparation();
            self.http_preparation_failed(
                now,
                preparation,
                Error::TimedOut,
                VenueError::new("HTTP group preparation window expired"),
            );
            return Err(Error::TimedOut);
        }
        self.io.core.http_plan_poison = true;
        let result = self.prepare_group_commit(now, drafts);
        let (prepared, releases) = match result {
            Ok(prepared) => prepared,
            Err((request, reason)) => {
                self.io.core.http_plan_poison = false;
                let preparation = self.io.core.managed_http[&request].lifecycle.preparation();
                self.http_preparation_failed(
                    now,
                    preparation,
                    reason,
                    VenueError::new("HTTP group admission failed"),
                );
                return Err(reason);
            }
        };
        // No callback or fallible storage operation may intervene in this loop.
        for member in &prepared {
            let managed = self.io.core.managed_http.get_mut(&member.request).unwrap();
            managed.group.as_mut().unwrap().bound = true;
        }
        let mut observations = prepared;
        for member in &mut observations {
            let managed = self.io.core.managed_http.get_mut(&member.request).unwrap();
            let claim = member.claim.take().expect("one-use prepared claim");
            let release = managed
                .lifecycle
                .commit_claim(claim, &mut self.io.core.http_pacing);
            *self
                .io
                .core
                .request_contexts
                .release_flag(&member.context, member.request)
                .expect("prevalidated write attempt")
                .expect("Mutation journal") = true;
            member.retained_recipe = managed.recipe.take();
            managed.signing_attempt = None;
            self.io
                .core
                .request_contexts
                .execution_leg_released(&member.context);
            self.io.core.http_write_revision = self.io.core.http_write_revision.saturating_add(1);
            let record = self
                .io
                .core
                .http_responses
                .get_mut(member.request)
                .expect("group storage committed");
            record.release = Some(release);
            record.order_revision = self.store.orders_revision();
            record.write_revision = self.io.core.http_write_revision;
            #[cfg(feature = "order-locks")]
            if let Owner::Command { id, .. } = member.context.owner() {
                self.io.core.order_locks.note_command_write(id);
            }
        }
        // Observers see complete committed release state for every sibling.
        for member in observations {
            let pending = self
                .io
                .pending
                .get_mut(&member.request)
                .expect("prevalidated pending");
            self.venue.on_signing_release(
                SigningTarget::Http {
                    request: member.request,
                    pending: &mut pending.value,
                },
                member.observation,
            );
            drop(member.retained_recipe);
        }
        self.io.core.http_plan_poison = false;
        Ok(GroupClaim::Released(releases))
    }

    fn prepare_group_commit(
        &mut self,
        now: f64,
        drafts: Vec<(
            RequestId,
            crate::request_context::group_lifecycle::ClaimDraft,
        )>,
    ) -> Result<(Vec<PreparedMember>, Vec<GroupRelease>), (RequestId, Error)> {
        let head = drafts[0].0;
        let mut prepared = Vec::new();
        let mut records = Vec::new();
        let mut releases = Vec::new();
        prepared
            .try_reserve_exact(drafts.len())
            .map_err(|_| (head, Error::Capacity))?;
        records
            .try_reserve_exact(drafts.len())
            .map_err(|_| (head, Error::Capacity))?;
        releases
            .try_reserve_exact(drafts.len())
            .map_err(|_| (head, Error::Capacity))?;
        for (request, draft) in drafts {
            let managed = self
                .io
                .core
                .managed_http
                .get(&request)
                .ok_or((request, Error::Revoked))?;
            let context = managed.lifecycle.context().clone();
            if context.operation() != Operation::Mutation
                || !matches!(context.owner(), Owner::Command { .. })
                || self.io.core.streams.contains_key(&request)
            {
                return Err((request, Error::Invalid));
            }
            self.io
                .core
                .request_contexts
                .release_flag(&context, request)
                .map_err(|e| (request, e))?
                .ok_or((request, Error::State))?;
            let pending = self
                .io
                .pending
                .get(&request)
                .ok_or((request, Error::Revoked))?;
            let admission = self
                .venue
                .prepare_group_http_release(&pending.value, draft.release())
                .map_err(|e| (request, e))?;
            if !matches!(admission, crate::request_context::Admission::Unscheduled) {
                return Err((request, Error::Invalid));
            }
            let release = *draft.release();
            let observation = crate::SigningRelease::new(release.signature, release.time)
                .map_err(|e| (request, e))?;
            let send_before = managed
                .lifecycle
                .group_send_before(&release)
                .map_err(|e| (request, e))?;
            let data = self
                .venue
                .capture_http_reconciliation(&pending.value)
                .map_err(|e| (request, e))?;
            records.push(crate::request_context::ReconciliationRecord::new(
                context.owner(),
                context.leg(),
                managed.lifecycle.attempt(),
                managed.lifecycle.deadline(),
                data,
            ));
            releases.push(GroupRelease {
                release,
                send_before,
            });
            prepared.push(PreparedMember {
                request,
                context,
                claim: Some(draft.admit(admission).map_err(|e| (request, e))?),
                observation,
                retained_recipe: None,
            });
        }
        self.io
            .core
            .http_responses
            .insert_group(records, now)
            .map_err(|e| (head, e))?;
        Ok((prepared, releases))
    }
}
