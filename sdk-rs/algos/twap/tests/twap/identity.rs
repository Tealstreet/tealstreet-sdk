use super::*;

#[test]
fn identity_is_recorded_for_new_schedules_and_survives_restore_after_a_rename() {
    let (mut env, view) = (Env::new(), View::default());
    let mut first = TwapBook::new_with_owner("test-account", "identity-A".into());
    let (id, _) = start(&mut first, &mut env, &view, json!({}));
    let state = first.serialize_state();
    assert_eq!(state[&id]["ownerId"], "identity-A");
    let row = json!({"id":id,"options":state[&id]["options"],"state":state[&id]["state"],"ownerId":state[&id]["ownerId"]});
    let mut renamed = TwapBook::new_with_owner("new-label", "identity-A".into());
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
        let mut book = TwapBook::new_with_owner("test-account", "identity-A".into());
        book.handle("twap:state", &json!([row.clone()]), &mut env, &view);
        assert!(book.ids().next().is_none());
    }
    let mut legacy = TwapBook::new("test-account");
    legacy.handle("twap:state", &json!([row]), &mut env, &view);
    assert_eq!(legacy.ids().collect::<Vec<_>>(), ["same-id"]);
}

#[test]
fn refused_restore_leaves_an_existing_identity_schedule_unchanged() {
    let (mut env, view) = (Env::new(), View::default());
    let mut retained = persisted("retained", "paused", env.now);
    retained["ownerId"] = "identity-A".into();
    let mut book = TwapBook::new_with_owner("test-account", "identity-A".into());
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
    let mut book = TwapBook::new_with_owner("new-label", "identity-A".into());
    book.handle("twap:state", &json!([row]), &mut env, &view);
    assert!(book.ids().next().is_none());
}
