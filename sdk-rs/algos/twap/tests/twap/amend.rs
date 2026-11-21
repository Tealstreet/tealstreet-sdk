use super::*;
use safe_cex_algo_twap::TwapAmendment;
use safe_cex_engine::error::{Outcome, VenueError};
use safe_cex_engine::write_plan::ElementResult;

#[test]
fn amend_retains_issued_outcome_journal_and_never_revives_unknown_work() {
    let (mut book, mut env, view) = (owned_book(), Env::new(), View::default());
    let (id, first) = start(&mut book, &mut env, &view, json!({"ordersCount":4}));
    let token = placements(&first)[0].0.unwrap();
    book.placement_settled(&id, token, Ok(vec!["original-child".into()]), &mut env);
    let amendment =
        TwapAmendment::parse(&json!({"id":id,"update":{"remainingSlices":2,"intervalMs":1000}}))
            .unwrap();
    let (result, effects) = book.amend(amendment, &mut env, &view);
    result.unwrap();
    assert!(placements(&effects).is_empty());
    let instance = book.get(&id).unwrap();
    assert_eq!(instance.options.size, 1.0);
    assert_eq!(instance.state.orders_executed, 1);
    assert_eq!(instance.state.orders_sizes, vec![0.375, 0.375]);
    env.advance(1001.0);
    let next = book.wake(&mut env, &view);
    let token = placements(&next)[0].0.unwrap();
    let effects = book.placement_settled(
        &id,
        token,
        Err(VenueError::new("lost answer").outcome(Outcome::Unknown)),
        &mut env,
    );
    let error = effects
        .iter()
        .find_map(|effect| match effect {
            TwapEffect::Outcome { error, .. } => Some(error),
            _ => None,
        })
        .unwrap();
    let rows = error.elements.as_ref().unwrap();
    assert!(
        matches!(&rows[0].result, ElementResult::Placed { order_ids } if order_ids == &["original-child"])
    );
    assert!(matches!(rows[1].result, ElementResult::Unknown { .. }));
    assert_eq!(
        rows.len(),
        3,
        "only the two unissued slots were repartitioned"
    );
    let amendment = TwapAmendment::parse(&json!({"id":id,"update":{"intervalMs":1000}})).unwrap();
    assert!(
        book.amend(amendment, &mut env, &view)
            .0
            .unwrap_err()
            .contains("not amendable")
    );
    env.advance(100_000.0);
    assert!(placements(&book.wake(&mut env, &view)).is_empty());
}

#[test]
fn amend_is_atomic_at_schedule_bounds_and_preserves_paused_restore_shape() {
    let (mut book, mut env, view) = (owned_book(), Env::new(), View::default());
    let (id, first) = start(&mut book, &mut env, &view, json!({"ordersCount":4}));
    let token = placements(&first)[0].0.unwrap();
    book.placement_settled(&id, token, Ok(vec!["first".into()]), &mut env);
    book.handle("twap:pause", &json!(id), &mut env, &view);
    let before = book.serialize_state();
    let amendment =
        TwapAmendment::parse(&json!({"id":id,"update":{"remainingSlices":1000,"intervalMs":1000}}))
            .unwrap();
    assert!(
        book.amend(amendment, &mut env, &view).0.is_err(),
        "issued+remaining must still fit the original 1000 bound"
    );
    assert_eq!(book.serialize_state(), before);
    let amendment =
        TwapAmendment::parse(&json!({"id":id,"update":{"remainingSlices":2,"intervalMs":1000}}))
            .unwrap();
    book.amend(amendment, &mut env, &view).0.unwrap();
    let retained = book.serialize_state();
    assert_eq!(
        retained[&id]["options"]["size"],
        before[&id]["options"]["size"]
    );
    assert_eq!(retained[&id]["state"]["status"], "paused");
    env.advance(5000.0);
    assert!(placements(&book.wake(&mut env, &view)).is_empty());
    let mut restored = owned_book();
    let mut row = retained[&id].clone();
    row["id"] = json!(id);
    restored.handle("twap:state", &json!([row]), &mut env, &view);
    assert_eq!(restored.serialize_state(), retained);
    restored.handle("twap:resume", &json!(id), &mut env, &view);
    assert_eq!(market_amounts(&restored.wake(&mut env, &view)), vec![0.375]);
}

#[test]
fn amendment_decoders_refuse_empty_null_unknown_and_nonpositive_fields() {
    for update in [
        json!({}),
        json!({"size":2}),
        json!({"side":"sell"}),
        json!({"symbol":"other"}),
        json!({"intervalMs":null}),
        json!({"intervalMs":0}),
        json!({"remainingSlices":0}),
        json!({"remainingSlices":1001}),
        json!({"remainingSlices":1.5}),
    ] {
        assert!(
            TwapAmendment::parse(&json!({"id":"job","update":update})).is_err(),
            "{update}"
        );
    }
}

#[test]
fn a_slice_count_only_amendment_keeps_the_existing_schedule() {
    let (mut book, mut env, view) = (owned_book(), Env::new(), View::default());
    let (id, first) = start(&mut book, &mut env, &view, json!({"ordersCount":4}));
    let token = placements(&first)[0].0.unwrap();
    book.placement_settled(&id, token, Ok(vec!["child".into()]), &mut env);
    let scheduled = book.get(&id).unwrap().state.next_order_at;
    env.advance(1.0);
    let amendment = TwapAmendment::parse(&json!({"id":id,"update":{"remainingSlices":2}})).unwrap();
    book.amend(amendment, &mut env, &view).0.unwrap();
    let instance = book.get(&id).unwrap();
    assert_eq!(instance.state.orders_sizes, vec![0.375, 0.375]);
    assert_eq!(
        instance.state.next_order_at, scheduled,
        "only a changed interval re-anchors the next slice"
    );
}
