use super::*;
use crate::request_context::RetryPolicy;

fn owner_entry(registry: &mut ContextRegistry, owner: Owner, deadline: f64) -> CommandOwner {
    let policy = Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 100,
        pacing: None,
        retry: RetryPolicy::Never,
    };
    registry.contexts.remove(&1);
    registry.issue(1, Scope::Command(7), policy, 0.).unwrap();
    let mut entry = registry.commands.remove(&7).unwrap();
    entry.generation = 7;
    entry.physical = Some(owner);
    entry.last_deadline = deadline;
    let mut root = Context::new(owner, 1, Operation::Read, None, 0., deadline, None).unwrap();
    root.bind_issuer(&registry.issuer);
    entry.phase = CommandPhase::Context {
        root: root.clone(),
        entry_owned: true,
        tracked: false,
        expired: false,
    };
    registry.contexts.insert(1, root);
    entry
}

#[test]
fn background_and_execution_physical_owners_never_alias_a_generation() {
    let mut registry = ContextRegistry::default();
    let execution = Owner::Execution { generation: 7 };
    let background = Owner::Background { generation: 7 };
    let entry = owner_entry(&mut registry, execution, 100.);
    registry.physical.insert(7, entry);
    assert!(registry.physical_owner(execution));
    assert!(!registry.physical_owner(background));
    assert!(registry.owner_entry(background).is_none());
    assert!(registry.owner_entry_mut(background).is_none());
    assert!(!registry.finish_physical(background));
    let entry = owner_entry(&mut registry, background, 90.);
    registry.physical.insert(7, entry);
    assert!(registry.physical_owner(background));
    registry.owner_entry_mut(background).unwrap().last_deadline = 80.;
    assert!(!registry.physical_owner(execution));
    assert!(registry.owner_entry(execution).is_none());
    assert!(registry.owner_entry_mut(execution).is_none());
    assert!(!registry.finish_physical(execution));
    assert!(registry.owner_entry(background).unwrap().last_deadline == 80.);
}

#[test]
fn retained_background_registration_and_retirement_balance_pending() {
    let mut registry = ContextRegistry::default();
    let owner = Owner::Background { generation: 7 };
    let entry = owner_entry(&mut registry, owner, 100.);
    registry.physical.insert(7, entry);
    let context = registry.contexts[&1].clone();
    registry
        .register_attempt(&context, RequestId(9), 0.)
        .unwrap();
    assert!(registry.owner_entry(owner).unwrap().pending == 1);
    registry.retire_attempt(&context, RequestId(9));
    assert!(registry.owner_entry(owner).unwrap().pending == 0);
}

#[test]
fn pool_pressure_is_deferred_only_when_registered_background_slots_cause_it() {
    let mut registry = ContextRegistry::default();
    assert!(registry.background_context_count(None).unwrap() == 0);
    assert!(
        registry
            .check_context_capacity(Some(CAPACITY), None)
            .is_ok()
    );
    assert!(registry.check_context_capacity(Some(CAPACITY + 1), None) == Err(Error::Capacity));
    assert!(registry.check_context_capacity(None, None) == Err(Error::Capacity));
    let owner = Owner::Background { generation: 7 };
    let entry = owner_entry(&mut registry, owner, 100.);
    registry.physical.insert(7, entry);
    for leg in 2..=BACKGROUND_CONTEXT_CAP as u64 {
        let mut context = Context::new(owner, leg, Operation::Read, None, 0., 100., None).unwrap();
        context.bind_issuer(&registry.issuer);
        registry.contexts.insert(leg, context);
    }
    assert!(registry.background_context_count(None).unwrap() == BACKGROUND_CONTEXT_CAP);
    assert!(
        registry
            .check_context_capacity(Some(CAPACITY), None)
            .is_ok()
    );
    assert!(
        registry.check_context_capacity(Some(CAPACITY + 1), None) == Err(Error::CapacityDeferred)
    );
    assert!(
        registry.check_context_capacity(Some(CAPACITY + BACKGROUND_CONTEXT_CAP + 1), None)
            == Err(Error::Capacity)
    );
    assert!(
        registry.check_context_capacity(Some(CAPACITY + 1), Some(100.)) == Err(Error::Capacity)
    );
    registry.contexts.clear();
    assert!(registry.background_context_count(None).unwrap() == 0);
    assert!(registry.check_context_capacity(Some(CAPACITY + 1), None) == Err(Error::Capacity));
}

#[test]
fn deferred_capacity_uses_typed_advisory_retry_without_read_outcome() {
    let read = Error::CapacityDeferred.read_venue_error();
    assert!(read.kind == Some(crate::ErrorKind::RateLimited));
    assert!(read.retry_after_ms == Some(1000.));
    assert!(read.outcome.is_none());
    let mutation = Error::CapacityDeferred.venue_error();
    assert!(mutation.kind == Some(crate::ErrorKind::RateLimited));
    assert!(mutation.retry_after_ms == Some(1000.));
    assert!(mutation.outcome == Some(crate::Outcome::NotSent));
    assert!(Error::Capacity.read_venue_error().kind == Some(crate::ErrorKind::Internal));
    assert!(Error::Capacity.read_venue_error().retry_after_ms.is_none());
}
