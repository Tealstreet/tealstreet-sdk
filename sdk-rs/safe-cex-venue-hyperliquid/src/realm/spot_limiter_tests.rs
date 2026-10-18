use super::{
    Realm,
    test_runtime::TestRuntime,
    value::{PromiseState, Value},
};
use std::rc::Rc;

#[test]
fn production_spot_limiter_debits_refills_and_recursively_waits() {
    let runtime = Rc::new(TestRuntime::default());
    let mut realm = Realm::new(runtime.clone());
    // The same class assembly and concrete host used by the Spot binding.
    let class = crate::spot_client::rate_limiter_class(&mut realm).unwrap();
    let limiter = realm.construct(&class.constructor, &[], None).unwrap();
    let other = realm.construct(&class.constructor, &[], None).unwrap();
    assert_eq!(
        realm.get_str(&limiter, "capacity").unwrap().as_number(),
        Some(1200.)
    );
    let first = realm.call_method(&limiter, "waitForToken", &[]).unwrap();
    runtime.drain();
    assert!(matches!(
        &first.promise().unwrap().borrow().state,
        PromiseState::Fulfilled(Value::Undefined)
    ));
    assert_eq!(
        realm.get_str(&limiter, "tokens").unwrap().as_number(),
        Some(1199.),
        "production spot limiter default token debit"
    );
    assert_eq!(
        realm.get_str(&other, "tokens").unwrap().as_number(),
        Some(1200.),
        "production spot limiter instances are independent"
    );
    runtime.now.set(59999.);
    realm.call_method(&limiter, "refillTokens", &[]).unwrap();
    assert_eq!(
        realm.get_str(&limiter, "tokens").unwrap().as_number(),
        Some(1199.)
    );
    runtime.now.set(60000.);
    realm.call_method(&limiter, "refillTokens", &[]).unwrap();
    assert_eq!(
        realm.get_str(&limiter, "tokens").unwrap().as_number(),
        Some(1200.),
        "production spot limiter refills at the full-minute boundary"
    );
    assert_eq!(
        realm.get_str(&limiter, "lastRefill").unwrap().as_number(),
        Some(60000.)
    );
    realm
        .call_method(&limiter, "waitForToken", &[Value::Number(1200.)])
        .unwrap();
    runtime.drain();
    let waiting = realm
        .call_method(&limiter, "waitForToken", &[Value::Number(2.)])
        .unwrap();
    assert!(
        matches!(
            &waiting.promise().unwrap().borrow().state,
            PromiseState::Pending
        ),
        "production spot limiter waits when depleted"
    );
    runtime.drain();
    assert!(
        matches!(
            &waiting.promise().unwrap().borrow().state,
            PromiseState::Fulfilled(Value::Undefined)
        ),
        "production spot limiter recursive wait resolves"
    );
    assert_eq!(runtime.now.get(), 120000.);
    assert_eq!(
        realm.get_str(&limiter, "tokens").unwrap().as_number(),
        Some(1198.),
        "production spot limiter consumes captured weight after timer refill"
    );
}
