//! Immediate fixed-setting completion, with no Context or write journal.
use super::*;
use crate::request_context::CommandEntryError;
use std::sync::Arc;

struct PreparedLocalSetting {
    issuer: Arc<()>,
    owner: Owner,
}

impl ContextRegistry {
    fn reserve_local_setting(&mut self, command: CommandId) -> Result<(), Error> {
        if self.commands.contains_key(&command) || self.is_observer(command) {
            return Err(Error::Revoked);
        }
        if self.commands.len() >= CAPACITY {
            return Err(Error::Capacity);
        }
        self.commands.try_reserve(1).map_err(|_| Error::Capacity)
    }

    fn issue_local_setting(
        &mut self,
        command: CommandId,
        generation: u64,
        now: f64,
    ) -> PreparedLocalSetting {
        assert_eq!(
            self.dispatch,
            Some((command, crate::classify::Intent::Setting))
        );
        let previous = self.commands.insert(
            command,
            CommandOwner {
                generation,
                phase: CommandPhase::LocalSetting,
                last_deadline: now,
                writes: None,
                mutation_declared: true,
                pending: 0,
                deliveries: 0,
                waits: 0,
                intent: Some(crate::classify::Intent::Setting),
                leg_intents: Vec::new(),
                credentialed: false,
                inherited_uncertain: false,
            },
        );
        assert!(previous.is_none());
        PreparedLocalSetting {
            issuer: self.issuer.clone(),
            owner: Owner::Command {
                id: command,
                generation,
            },
        }
    }

    fn commit_local_setting(&self, prepared: PreparedLocalSetting) -> (CommandId, Seal) {
        assert!(Arc::ptr_eq(&self.issuer, &prepared.issuer));
        let Owner::Command { id, generation } = prepared.owner else {
            unreachable!("private local Setting origin");
        };
        let owner = self
            .commands
            .get(&id)
            .expect("prepared local Setting owner");
        assert!(
            owner.generation == generation
                && matches!(owner.phase, CommandPhase::LocalSetting)
                && owner.writes.is_none()
                && owner.mutation_declared
                && owner.pending == 0
                && owner.deliveries == 0
                && owner.waits == 0
        );
        (id, Seal::Success)
    }
}

impl<P> Cx<'_, P> {
    /// Complete a fixed native setting without credentials or physical work.
    /// Only the genuine Fresh Setting entry may project Store and finish once.
    /// Ok means completed; Rejected already emitted its typed refusal.
    pub fn done_local_setting(
        &mut self,
        result: String,
        project: impl FnOnce(&mut crate::Store),
    ) -> Result<(), CommandEntryError> {
        if self.io.core.http_owner_poison || !self.io.fresh_command_entry() {
            return Err(CommandEntryError::Revoked);
        }
        let command = self.io.command_entry_id().expect("fresh command entry");
        if self.io.core.request_contexts.dispatch
            != Some((command, crate::classify::Intent::Setting))
        {
            return Err(CommandEntryError::Revoked);
        }
        #[cfg(feature = "http-plans")]
        if self.io.core.http_plan_ready.is_some() || self.io.core.http_plan_expiring.is_some() {
            return Err(CommandEntryError::Revoked);
        }
        #[cfg(feature = "order-locks")]
        if self.io.core.order_locks.contains(command) {
            return Err(CommandEntryError::Revoked);
        }
        let receipt = self.io.command_receipt();
        let readiness = if !self.now.is_finite() {
            Err(Error::Invalid)
        } else {
            receipt.and_then(|(_, deadline)| {
                if deadline - self.now < 1. {
                    Err(Error::TimedOut)
                } else {
                    Ok(())
                }
            })
        };
        self.with_command_entry(Operation::Mutation, |cx, command| {
            readiness?;
            if !cx.io.core.next_id.available(1) {
                return Err(Error::Capacity);
            }
            cx.io.core.request_contexts.reserve_local_setting(command)?;
            cx.io
                .core
                .actions
                .try_reserve(1)
                .map_err(|_| Error::Capacity)?;
            let generation = cx
                .io
                .reserve_ids(1)
                .expect("checked local generation")
                .start;
            let prepared = cx
                .io
                .core
                .request_contexts
                .issue_local_setting(command, generation, cx.now);
            // No owned HTTP, read-root timer or order lock exists for this
            // exact Fresh generation; common cleanup needs only one notice.
            cx.io.core.http_owner_poison = true;
            project(cx.store);
            let (command, seal) = cx.io.core.request_contexts.commit_local_setting(prepared);
            cx.finish_sealed_command(command, Some(result), seal)
                .expect("prepared local Setting terminal");
            cx.io.core.http_owner_poison = false;
            Ok(())
        })
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::*;
    use crate::engine::Io;
    use crate::{Action, Notice, Outcome, Store};

    fn fresh(deadline: Option<f64>) -> Io<()> {
        let mut io = Io::default();
        io.core.command_waits.receive(7, deadline);
        io.begin_command_entry(Some(7));
        io.core.request_contexts.dispatch = Some((7, crate::classify::Intent::Setting));
        io
    }

    fn refusal(io: &mut Io<()>, now: f64, expected: Error) {
        let before = io.core.next_id;
        let mut store = Store::default();
        store.set_hedged(true);
        let result = Cx {
            now,
            io,
            store: &mut store,
        }
        .done_local_setting("[]".into(), |s| s.set_hedged(false));
        assert_eq!(result, Err(CommandEntryError::Rejected(expected)));
        assert!(store.hedged());
        assert_eq!(io.core.next_id, before);
        assert!(io.core.request_contexts.commands.is_empty());
        assert!(
            matches!(io.core.actions.as_slice(), [Action::Notice(Notice::CommandFailed { command: 7, error })] if error == &expected.venue_error() && error.outcome == Some(Outcome::NotSent))
        );
    }

    #[test]
    fn expired_receipt_refuses_before_projection_or_id_spend() {
        refusal(&mut fresh(Some(1000.)), 1000., Error::TimedOut);
    }
    #[test]
    fn sub_ms_receipt_refuses_without_rounding_or_renewal() {
        refusal(&mut fresh(Some(1000.25)), 999.75, Error::TimedOut);
    }
    #[test]
    fn invalid_clock_has_typed_initial_refusal() {
        refusal(&mut fresh(Some(1000.)), f64::NAN, Error::Invalid);
    }
    #[test]
    fn absent_receipt_has_no_fresh_default() {
        refusal(&mut fresh(None), 0., Error::Revoked);
    }
    #[test]
    fn exhausted_receipt_has_capacity_cause() {
        let mut io = Io::default();
        io.core.command_waits.exhaust_receipts();
        io.core.command_waits.receive(7, Some(1000.));
        io.begin_command_entry(Some(7));
        io.core.request_contexts.dispatch = Some((7, crate::classify::Intent::Setting));
        refusal(&mut io, 0., Error::Capacity);
    }
    #[test]
    fn exhausted_generation_allocator_refuses_without_ghost_owner() {
        let mut io = fresh(Some(1000.));
        io.end_command_entry();
        io.core.next_id =
            crate::resource_ids::ResourceIds::starting_at(crate::resource_ids::MAX_RESOURCE_ID + 1);
        io.begin_command_entry(Some(7));
        refusal(&mut io, 0., Error::Capacity);
    }
    #[test]
    fn last_generation_id_completes_without_allocating_cleanup_ids() {
        let mut io = fresh(Some(1000.));
        io.end_command_entry();
        io.core.next_id =
            crate::resource_ids::ResourceIds::starting_at(crate::resource_ids::MAX_RESOURCE_ID);
        io.begin_command_entry(Some(7));
        let mut store = Store::default();
        assert_eq!(
            Cx {
                now: 0.,
                io: &mut io,
                store: &mut store
            }
            .done_local_setting("[]".into(), |_| {}),
            Ok(())
        );
        assert!(!io.core.next_id.available(1));
        assert!(!io.core.next_id.failed());
        assert!(io.core.request_contexts.commands.is_empty());
        assert!(
            matches!(io.core.actions.as_slice(), [Action::Notice(Notice::CommandDone { command: 7, result })] if result == "[]")
        );
    }
    #[test]
    fn original_duplicate_stamp_survives_rejection() {
        let mut io = fresh(Some(1000.));
        let stamp = io.core.command_waits.stamp(7);
        io.end_command_entry();
        io.core.command_waits.receive(7, Some(2000.));
        io.begin_command_entry(Some(7));
        refusal(&mut io, 1., Error::Revoked);
        assert_eq!(io.core.command_waits.stamp(7), stamp);
    }
    #[test]
    fn foreign_dispatch_cannot_borrow_the_fresh_entry() {
        let mut io = fresh(Some(1000.));
        io.core.request_contexts.dispatch = Some((8, crate::classify::Intent::Setting));
        let before = io.core.next_id;
        let mut store = Store::default();
        assert_eq!(
            Cx {
                now: 0.,
                io: &mut io,
                store: &mut store
            }
            .done_local_setting("[]".into(), |_| panic!("foreign projection")),
            Err(CommandEntryError::Revoked)
        );
        assert_eq!(io.core.next_id, before);
        assert!(io.core.actions.is_empty());
    }
    #[test]
    fn spent_entry_cannot_project_or_emit_twice() {
        let mut io = fresh(Some(1000.));
        let mut store = Store::default();
        let mut cx = Cx {
            now: 0.,
            io: &mut io,
            store: &mut store,
        };
        assert_eq!(cx.done_local_setting("[]".into(), |_| {}), Ok(()));
        assert_eq!(
            cx.done_local_setting("[]".into(), |_| panic!("second projection")),
            Err(CommandEntryError::Revoked)
        );
        assert_eq!(io.core.actions.len(), 1);
    }
}
