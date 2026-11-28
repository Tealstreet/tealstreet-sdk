//! "Limit orders" slices rest on the book: every end of their TWAP must cancel
//! them or name them to the user, never leave them unmanaged.

use super::*;

const INTERVAL_MS: f64 = 5.0 * 60.0 * 1000.0;

fn quoted() -> View {
    View {
        ticker: Some(TickerQuote {
            bid: Some(49_900.0),
            ask: Some(50_100.0),
            last: Some(50_000.0),
        }),
        ..View::default()
    }
}

fn limit_twap(book: &mut TwapBook, env: &mut Env, view: &View) -> (String, Vec<TwapEffect>) {
    start(
        book,
        env,
        view,
        json!({ "size": 0.002, "duration": 10, "ordersCount": 2, "useLimitOrders": true }),
    )
}

fn cancelled(effects: &[TwapEffect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            TwapEffect::CancelSlices { slices, .. } => Some(slices),
            _ => None,
        })
        .flatten()
        .flat_map(|slice| slice.order_ids.clone())
        .collect()
}

fn logs(effects: &[TwapEffect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            TwapEffect::Log { message } => Some(message.clone()),
            _ => None,
        })
        .collect()
}

fn ended(effects: &[TwapEffect]) -> Option<TwapEnd> {
    effects.iter().find_map(|effect| match effect {
        TwapEffect::Ended { end, .. } => Some(*end),
        _ => None,
    })
}

/// A two-slice limit TWAP whose first slice rests as `first`.
fn one_slice_resting(first: &str) -> (TwapBook, Env, View, String) {
    let (mut env, view, mut book) = (Env::new(), quoted(), owned_book());
    let (id, effects) = limit_twap(&mut book, &mut env, &view);
    settle_all(&mut book, &mut env, &id, &effects, Ok(vec![first.into()]));
    (book, env, view, id)
}

#[test]
fn stopping_cancels_every_resting_limit_slice() {
    let (mut book, mut env, view, id) = one_slice_resting("o1");
    let effects = book.handle("twap:stop", &json!(id), &mut env, &view);
    assert_eq!(cancelled(&effects), ["o1"]);
    assert_eq!(ended(&effects), Some(TwapEnd::Stopped));
    let Some(TwapEffect::CancelSlices {
        symbol,
        side,
        slices,
        ..
    }) = effects
        .iter()
        .find(|effect| matches!(effect, TwapEffect::CancelSlices { .. }))
    else {
        unreachable!()
    };
    assert_eq!((symbol.as_str(), *side), ("BTCUSDT", OrderSide::Buy));
    assert_eq!((slices[0].price, slices[0].size), (49_900.0, 0.001));
}

#[test]
fn typed_stop_cancels_too() {
    let (mut book, _, _, id) = one_slice_resting("o1");
    let (found, effects) = book.stop(&id);
    assert!(found);
    assert_eq!(cancelled(&effects), ["o1"]);
}

#[test]
fn account_disposal_cancels_every_resting_limit_slice() {
    let (mut book, _, _, _) = one_slice_resting("o1");
    assert_eq!(cancelled(&book.terminate_all()), ["o1"]);
}

#[test]
fn finishing_waits_out_the_last_interval_then_cancels_what_still_rests() {
    let (mut book, mut env, view, id) = one_slice_resting("o1");
    env.advance(INTERVAL_MS);
    let second = book.wake(&mut env, &view);
    let after = settle_all(&mut book, &mut env, &id, &second, Ok(vec!["o2".into()]));
    // Every slice is out, but the last one still has its interval to fill.
    assert!(cancelled(&after).is_empty());
    assert_eq!(ended(&after), None);
    let instance = book.get(&id).unwrap();
    assert_eq!(instance.state.status, TwapStatus::Running);
    assert_eq!(instance.state.orders_executed, 2);
    let started_at = instance.state.started_at.unwrap();
    // It ends at its stated duration, not one interval early.
    assert_eq!(book.wake_at(), Some(started_at + 2.0 * INTERVAL_MS));
    env.advance(INTERVAL_MS);
    let closing = book.wake(&mut env, &view);
    assert!(placements(&closing).is_empty());
    assert_eq!(cancelled(&closing), ["o1", "o2"]);
    assert_eq!(ended(&closing), Some(TwapEnd::Completed));
    assert!(book.get(&id).is_none());
}

#[test]
fn a_market_twap_still_ends_with_its_last_slice() {
    let (mut env, view, mut book) = (Env::new(), quoted(), owned_book());
    let (id, effects) = start(
        &mut book,
        &mut env,
        &view,
        json!({ "size": 0.002, "duration": 10, "ordersCount": 2 }),
    );
    settle_all(&mut book, &mut env, &id, &effects, Ok(vec!["m1".into()]));
    env.advance(INTERVAL_MS);
    let second = book.wake(&mut env, &view);
    let after = settle_all(&mut book, &mut env, &id, &second, Ok(vec!["m2".into()]));
    assert!(cancelled(&after).is_empty());
    assert_eq!(ended(&after), Some(TwapEnd::Completed));
}

#[test]
fn a_slice_placed_after_the_stop_is_cancelled_when_it_lands() {
    let (mut env, view, mut book) = (Env::new(), quoted(), owned_book());
    let (id, effects) = limit_twap(&mut book, &mut env, &view);
    let stopped = book.handle("twap:stop", &json!(id), &mut env, &view);
    assert!(cancelled(&stopped).is_empty());
    let landed = settle_all(&mut book, &mut env, &id, &effects, Ok(vec!["late".into()]));
    assert_eq!(cancelled(&landed), ["late"]);
}

#[test]
fn an_unknown_halt_cancels_nothing_and_names_each_resting_slice() {
    let (mut book, mut env, view, id) = one_slice_resting("o1");
    env.advance(INTERVAL_MS);
    let second = book.wake(&mut env, &view);
    let unknown = Err(SliceFailure {
        message: "timed out".into(),
        outcome: Some(safe_cex_engine::error::Outcome::Unknown),
        ..Default::default()
    });
    let halted = settle_all(&mut book, &mut env, &id, &second, unknown);
    assert!(cancelled(&halted).is_empty());
    let named = logs(&halted)
        .into_iter()
        .find(|message| message.contains("still open"))
        .expect("the halt names its resting slices");
    assert!(named.contains("buy 0.001 at 49900"), "{named}");
    assert!(named.contains("o1"), "{named}");
}

#[test]
fn resting_slices_are_persisted_and_a_restored_twap_still_cancels_them() {
    let (book, _, _, id) = one_slice_resting("o1");
    let mut row = book.serialize_state()[&id].clone();
    assert_eq!(
        row["state"]["limitSlices"],
        json!([{ "orderIds": ["o1"], "price": 49900, "size": 0.001 }])
    );
    row["id"] = json!(id);
    let (mut env, view, mut restored) = (Env::new(), quoted(), owned_book());
    restored.handle("twap:state", &json!([row]), &mut env, &view);
    assert_eq!(restored.get(&id).unwrap().current_order_ids(), ["o1"]);
    let effects = restored.handle("twap:stop", &json!(id), &mut env, &view);
    assert_eq!(cancelled(&effects), ["o1"]);
}

#[test]
fn a_twap_restored_in_its_last_interval_completes_and_cancels_once() {
    let (mut book, mut env, view, id) = one_slice_resting("o1");
    env.advance(INTERVAL_MS);
    let second = book.wake(&mut env, &view);
    settle_all(&mut book, &mut env, &id, &second, Ok(vec!["o2".into()]));
    let mut row = book.serialize_state()[&id].clone();
    assert_eq!(row["state"]["ordersSizes"], json!([]));
    row["id"] = json!(id);
    let mut restored = owned_book();
    let adopted = restored.handle("twap:state", &json!([row]), &mut env, &view);
    assert!(logs(&adopted).is_empty(), "{:?}", logs(&adopted));
    env.advance(INTERVAL_MS);
    let closing = restored.wake(&mut env, &view);
    assert_eq!(cancelled(&closing), ["o1", "o2"]);
    assert_eq!(ended(&closing), Some(TwapEnd::Completed));
    // Already cancelled or filled elsewhere: one cancel, then nothing left to do.
    assert!(restored.wake_at().is_none());
    assert!(cancelled(&restored.wake(&mut env, &view)).is_empty());
}
