use super::test_runtime::TestRuntime;
use safe_cex_exchange::js_runtime::value::Js;
use std::rc::Rc;

#[test]
fn production_spot_limiter_debits_refills_and_recursively_waits() {
    let runtime = Rc::new(TestRuntime::default());
    let mut realm = crate::host::test_runtime::host(runtime.clone());
    // The same class assembly and concrete host used by the Spot binding.
    let class = crate::spot_client::rate_limiter_class(&mut realm).unwrap();
    let limiter = realm.construct(&class.constructor, &[], None).unwrap();
    let other = realm.construct(&class.constructor, &[], None).unwrap();
    assert_eq!(
        realm.get_str(&limiter, "capacity").unwrap().as_f64(),
        Some(1200.)
    );
    let first = realm.call_method(&limiter, "waitForToken", &[]).unwrap();
    runtime.drain();
    assert!(matches!(
        match &first {
            Js::Promise(p) => p.result(),
            _ => panic!("expected promise"),
        },
        Some(Ok(Js::Undefined))
    ));
    assert_eq!(
        realm.get_str(&limiter, "tokens").unwrap().as_f64(),
        Some(1199.),
        "production spot limiter default token debit"
    );
    assert_eq!(
        realm.get_str(&other, "tokens").unwrap().as_f64(),
        Some(1200.),
        "production spot limiter instances are independent"
    );
    runtime.now.set(59999.);
    realm.call_method(&limiter, "refillTokens", &[]).unwrap();
    assert_eq!(
        realm.get_str(&limiter, "tokens").unwrap().as_f64(),
        Some(1199.)
    );
    runtime.now.set(60000.);
    realm.call_method(&limiter, "refillTokens", &[]).unwrap();
    assert_eq!(
        realm.get_str(&limiter, "tokens").unwrap().as_f64(),
        Some(1200.),
        "production spot limiter refills at the full-minute boundary"
    );
    assert_eq!(
        realm.get_str(&limiter, "lastRefill").unwrap().as_f64(),
        Some(60000.)
    );
    realm
        .call_method(&limiter, "waitForToken", &[Js::Number(1200.)])
        .unwrap();
    runtime.drain();
    let waiting = realm
        .call_method(&limiter, "waitForToken", &[Js::Number(2.)])
        .unwrap();
    assert!(
        matches!(
            match &waiting {
                Js::Promise(p) => p.result(),
                _ => panic!("expected promise"),
            },
            None
        ),
        "production spot limiter waits when depleted"
    );
    runtime.drain();
    assert!(
        matches!(
            match &waiting {
                Js::Promise(p) => p.result(),
                _ => panic!("expected promise"),
            },
            Some(Ok(Js::Undefined))
        ),
        "production spot limiter recursive wait resolves"
    );
    assert_eq!(runtime.now.get(), 120000.);
    assert_eq!(
        realm.get_str(&limiter, "tokens").unwrap().as_f64(),
        Some(1198.),
        "production spot limiter consumes captured weight after timer refill"
    );
}
