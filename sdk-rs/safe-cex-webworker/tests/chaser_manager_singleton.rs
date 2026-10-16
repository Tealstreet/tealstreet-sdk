use safe_cex_webworker::chaser_manager::{ChaserOrderManager, chaser_order_manager_instance};
use safe_cex_webworker::twap_manager::{TwapManager, twap_manager};
use std::rc::Rc;

#[test]
fn accessor_preserves_native_singleton_identity() {
    let first = chaser_order_manager_instance();
    let second = chaser_order_manager_instance();
    assert!(
        Rc::ptr_eq(&first, &second),
        "CHASER native singleton identity"
    );
    assert!(
        !Rc::ptr_eq(&first, &ChaserOrderManager::new()),
        "CHASER fresh manager remains independent"
    );
}

#[test]
fn twap_accessor_preserves_native_singleton_identity() {
    let first = twap_manager();
    assert!(
        Rc::ptr_eq(&first, &twap_manager()),
        "TWAP native singleton identity"
    );
    assert!(
        !Rc::ptr_eq(&first, &Rc::new(TwapManager::default())),
        "TWAP fresh manager remains independent"
    );
}
