use safe_cex_num::{BuiltinReference, PrototypeRealm, PrototypeScalar, PrototypeValue};
use std::rc::Rc;

fn current_to_string() -> BuiltinReference {
    match PrototypeRealm::current().inherited("toString") {
        PrototypeValue::Reference(reference) => reference,
        value => panic!("expected builtin reference, received {value:?}"),
    }
}

#[test]
fn realm_slot_shares_identity_and_restores_nested_bindings() {
    let original = PrototypeRealm::current();
    assert!(
        Rc::ptr_eq(&original, &PrototypeRealm::current()),
        "REALM-TLS-SHARED"
    );

    let outer = Rc::new(PrototypeRealm::default());
    let outer_binding = outer.bind();
    assert!(Rc::ptr_eq(&outer, &PrototypeRealm::current()));
    let retained = current_to_string();
    assert!(Rc::ptr_eq(retained.realm(), &outer));
    retained.set("slot-proof", PrototypeScalar::Number(7.0));

    let inner = Rc::new(PrototypeRealm::default());
    let inner_binding = inner.bind();
    assert!(Rc::ptr_eq(&inner, &PrototypeRealm::current()));
    assert!(matches!(
        current_to_string().get("slot-proof"),
        PrototypeValue::Undefined
    ));
    drop(inner_binding);

    assert!(Rc::ptr_eq(&outer, &PrototypeRealm::current()));
    assert!(matches!(
        current_to_string().get("slot-proof"),
        PrototypeValue::Number(value) if value.to_bits() == 7.0_f64.to_bits()
    ));
    drop(outer_binding);

    assert!(Rc::ptr_eq(&original, &PrototypeRealm::current()));
    assert!(Rc::ptr_eq(retained.realm(), &outer));
    assert!(matches!(
        retained.get("slot-proof"),
        PrototypeValue::Number(value) if value.to_bits() == 7.0_f64.to_bits()
    ));
}
