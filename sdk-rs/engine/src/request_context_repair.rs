//! Journal owners for repair roots. A root's legs are ordinary managed
//! Contexts; only their owner, intent and retirement differ from a command.
use super::{CommandOwner, CommandPhase, ContextRegistry};
use crate::request_context::{Context, Error, Owner, Policy, WriteResult};

impl ContextRegistry {
    /// Issue one leg of repair root `generation` with the step's intent. The
    /// root's journal owner is created with its first leg.
    pub(in crate::engine) fn issue_repair(
        &mut self,
        identity: u64,
        generation: u64,
        intent: crate::classify::Intent,
        policy: Policy,
        now: f64,
    ) -> Result<Context, Error> {
        if !now.is_finite() || policy.timeout_ms == 0 {
            return Err(Error::Invalid);
        }
        self.prune(now);
        if !self.repairs.contains_key(&generation) {
            self.repairs.try_reserve(1).map_err(|_| Error::Capacity)?;
        }
        let context = self.commit(
            Owner::Repair { generation },
            identity,
            policy,
            now,
            None,
            None,
            None,
        )?;
        match self.repairs.get_mut(&generation) {
            Some(owner) => {
                owner.intent = Some(intent);
                owner.last_deadline = owner.last_deadline.max(context.deadline());
            }
            None => self.repairs.insert(
                generation,
                CommandOwner {
                    generation,
                    phase: CommandPhase::Context {
                        root: context.clone(),
                        entry_owned: false,
                        tracked: false,
                        expired: false,
                    },
                    last_deadline: context.deadline(),
                    writes: None,
                    mutation_declared: true,
                    pending: 0,
                    deliveries: 0,
                    waits: 0,
                    intent: Some(intent),
                    leg_intents: Vec::new(),
                    credentialed: true,
                    inherited_uncertain: false,
                },
            ),
        }
        Ok(context)
    }

    /// The primary write credential of this command's admitted WriteAfterReads
    /// Preflight or Mutation Plan. `None` for any other handle or phase.
    pub(in crate::engine) fn admitted_write_credential(
        &self,
        preflight: &crate::request_context::Preflight,
    ) -> Option<crate::request_context::Credential> {
        let Owner::Command { id, generation } = preflight.owner() else {
            return None;
        };
        if !preflight.issued_by(&self.issuer) {
            return None;
        }
        let owner = self
            .commands
            .get(&id)
            .filter(|owner| owner.generation == generation)?;
        match &owner.phase {
            CommandPhase::Preflight(state) => state.repair_write_credential().cloned(),
            CommandPhase::Planned(state) => state.repair_write_credential().cloned(),
            _ => None,
        }
    }

    /// The latest attempt of a root leg: whether it was released, and its result.
    pub(in crate::engine) fn repair_leg_outcome(
        &self,
        generation: u64,
        leg: u64,
    ) -> Option<(bool, Option<&WriteResult>)> {
        self.repairs
            .get(&generation)?
            .writes
            .as_ref()?
            .leg_outcome(leg)
    }

    /// Whether one more leg fits, checked before any ID is reserved.
    pub(in crate::engine) fn repair_leg_capacity(&self) -> bool {
        self.contexts.len() + self.reserved_context_count() < super::CAPACITY
    }

    /// Whether this root leg is still an issued Context.
    pub(in crate::engine) fn repair_leg_live(&self, context: &Context) -> bool {
        self.contexts.get(&context.leg()) == Some(context)
    }

    pub(in crate::engine) fn remove_repair_leg(&mut self, context: &Context) {
        self.contexts.remove(&context.leg());
    }

    /// Live legs of a root, including a successor issued in its predecessor's
    /// delivery.
    pub(in crate::engine) fn repair_live_legs(&self, generation: u64) -> usize {
        self.contexts
            .values()
            .filter(|context| context.owner() == Owner::Repair { generation })
            .count()
    }

    /// Drop a finished root's journal owner and any Context it still holds.
    pub(in crate::engine) fn close_repair(&mut self, generation: u64) {
        self.contexts
            .retain(|_, context| context.owner() != Owner::Repair { generation });
        self.repairs.remove(&generation);
    }
}
