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
    registry.physical.insert(execution, entry);
    assert!(registry.physical_owner(execution));
    assert!(!registry.physical_owner(background));
    assert!(registry.owner_entry(background).is_none());
    assert!(registry.owner_entry_mut(background).is_none());
    assert!(!registry.finish_physical(background));
    let entry = owner_entry(&mut registry, background, 90.);
    registry.physical.insert(background, entry);
    assert!(registry.physical_owner(background));
    registry.owner_entry_mut(background).unwrap().last_deadline = 80.;
    assert!(registry.owner_entry(execution).unwrap().last_deadline == 100.);
    assert!(registry.owner_entry(background).unwrap().last_deadline == 80.);
}

#[test]
fn retained_background_registration_and_retirement_balance_pending() {
    let mut registry = ContextRegistry::default();
    let owner = Owner::Background { generation: 7 };
    let entry = owner_entry(&mut registry, owner, 100.);
    registry.physical.insert(owner, entry);
    let context = registry.contexts[&1].clone();
    registry
        .register_attempt(&context, RequestId(9), 0.)
        .unwrap();
    assert!(registry.owner_entry(owner).unwrap().pending == 1);
    registry.retire_attempt(&context, RequestId(9));
    assert!(registry.owner_entry(owner).unwrap().pending == 0);
}
