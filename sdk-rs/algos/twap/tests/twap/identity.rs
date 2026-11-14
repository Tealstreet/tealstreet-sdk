use super::*;

fn identity_owner() -> safe_cex_algo_twap::TwapOwner {
    safe_cex_algo_twap::TwapOwner {
        owner_id: "identity-A".into(),
        generation: 7,
    }
}

#[test]
fn identity_is_recorded_for_new_schedules_and_survives_restore_after_a_rename() {
    let (mut env, view) = (Env::new(), View::default());
    let mut first = TwapBook::new_with_owner("test-account", identity_owner());
    let (id, effects) = start(&mut first, &mut env, &view, json!({}));
    settle_all(&mut first, &mut env, &id, &effects, Ok(vec![]));
    let state = first.serialize_state();
    assert_eq!(state[&id]["ownerId"], "identity-A");
    assert!(state[&id]["state"]["inFlight"].is_null());
    let row = json!({"id":id,"options":state[&id]["options"],"state":state[&id]["state"],"ownerId":state[&id]["ownerId"],"ownerGeneration":state[&id]["ownerGeneration"]});
    let mut renamed = TwapBook::new_with_owner("new-label", identity_owner());
    renamed.handle("twap:state", &json!([row]), &mut env, &view);
    assert_eq!(renamed.ids().collect::<Vec<_>>(), [id.as_str()]);
    assert_eq!(renamed.serialize_state()[&id]["ownerId"], "identity-A");
    assert_eq!(
        renamed.serialize_state()[&id]["options"]["accountName"],
        "new-label"
    );
    assert_eq!(
        renamed.serialize_state()[&id]["state"]["accountName"],
        "new-label"
    );
}

#[test]
fn identity_bound_books_refuse_other_and_untagged_schedules_with_equal_labels() {
    let (mut env, view) = (Env::new(), View::default());
    let mut row = persisted("same-id", "paused", env.now);
    for owner in [Some("identity-B"), None] {
        if let Some(owner) = owner {
            row["ownerId"] = owner.into();
        } else {
            row.as_object_mut().unwrap().remove("ownerId");
        }
        let mut book = TwapBook::new_with_owner("test-account", identity_owner());
        book.handle("twap:state", &json!([row.clone()]), &mut env, &view);
        assert!(book.ids().next().is_none());
    }
    let mut legacy = TwapBook::with_legacy_restore("test-account");
    legacy.handle("twap:state", &json!([row]), &mut env, &view);
    assert_eq!(legacy.ids().collect::<Vec<_>>(), ["same-id"]);
}

#[test]
fn refused_restore_leaves_an_existing_identity_schedule_unchanged() {
    let (mut env, view) = (Env::new(), View::default());
    let mut retained = persisted("retained", "paused", env.now);
    retained["ownerId"] = "identity-A".into();
    let mut book = TwapBook::new_with_owner("test-account", identity_owner());
    book.handle("twap:state", &json!([retained]), &mut env, &view);
    let before = book.serialize_state();
    let mut other = persisted("foreign", "running", env.now);
    other["ownerId"] = "identity-B".into();
    book.handle("twap:state", &json!([other]), &mut env, &view);
    assert_eq!(book.serialize_state(), before);
    assert_eq!(book.ids().collect::<Vec<_>>(), ["retained"]);
}

#[test]
fn matching_identity_does_not_accept_inconsistent_persisted_labels() {
    let (mut env, view) = (Env::new(), View::default());
    let mut row = persisted("inconsistent", "paused", env.now);
    row["ownerId"] = "identity-A".into();
    row["state"]["accountName"] = "another-label".into();
    let mut book = TwapBook::new_with_owner("new-label", identity_owner());
    book.handle("twap:state", &json!([row]), &mut env, &view);
    assert!(book.ids().next().is_none());
}

#[test]
fn fixround_unbound_book_refuses_identity_tagged_schedules() {
    let (mut env, view) = (Env::new(), View::default());
    for status in ["paused", "running"] {
        let mut tagged = persisted("foreign", status, env.now);
        tagged["ownerId"] = "identity-A".into();
        let mut book = TwapBook::new("test-account");
        let effects = book.handle("twap:state", &json!([tagged]), &mut env, &view);
        assert!(book.ids().next().is_none());
        assert!(placements(&effects).is_empty());
        assert!(placements(&book.wake(&mut env, &view)).is_empty());
        book.handle(
            "twap:state",
            &json!([persisted("legacy", "paused", env.now)]),
            &mut env,
            &view,
        );
        assert!(book.ids().next().is_none());
        let mut legacy = TwapBook::with_legacy_restore("test-account");
        let mut row = persisted("legacy", "paused", env.now);
        row.as_object_mut().unwrap().remove("ownerId");
        row.as_object_mut().unwrap().remove("ownerGeneration");
        legacy.handle("twap:state", &json!([row]), &mut env, &view);
        assert_eq!(legacy.ids().collect::<Vec<_>>(), ["legacy"]);
    }
}

#[test]
fn identity_rename_still_requires_the_exact_persisted_generation() {
    for generation in [None, Some(6)] {
        let (mut env, view) = (Env::new(), View::default());
        let mut row = persisted("stale-generation", "running", env.now + 15_000.);
        row["ownerId"] = "identity-A".into();
        if let Some(generation) = generation {
            row["ownerGeneration"] = json!(generation);
        } else {
            row.as_object_mut().unwrap().remove("ownerGeneration");
        }
        let mut book = TwapBook::new_with_owner("renamed", identity_owner());
        let effects = book.handle("twap:state", &json!([row]), &mut env, &view);
        assert!(book.ids().next().is_none());
        assert!(placements(&effects).is_empty());
    }
}
