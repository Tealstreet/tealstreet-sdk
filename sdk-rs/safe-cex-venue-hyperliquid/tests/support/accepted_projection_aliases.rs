//! Exact production Realm instantiations; no claim for any other host or decoder.
use crate::{
    core_exchange_class::{ExchangeError, ExchangeValue},
    private_class::{PrivateError, PrivateValue},
    public_class::{PublicError, PublicValue},
    realm::{Realm, Value, test_runtime::TestRuntime},
    ws_ping_lifecycle::WsPingHost,
};
use std::{any::TypeId, rc::Rc};

fn require_production_associated_types<H: WsPingHost<Value = Value, Error = Value>>() {}

fn native_invariants<A: 'static>(
    as_alias: fn(&mut Value) -> &mut A,
    as_value: fn(&mut A) -> &mut Value,
) {
    require_production_associated_types::<Realm>();
    assert_eq!(TypeId::of::<A>(), TypeId::of::<Value>());
    let mut realm = Realm::new(Rc::new(TestRuntime::default()));
    let object = realm.object_from(vec![("marker", Value::Number(-0.0))]);
    let thrown = realm.type_error("native thrown object");
    let mut cases = [
        Value::Undefined,
        Value::Number(-0.0),
        Value::Number(f64::INFINITY),
        Value::String(Rc::from([0xd800_u16, 0x0061])),
        object.clone(),
        thrown.clone(),
    ];
    for (index, value) in cases.iter_mut().enumerate() {
        let address = std::ptr::from_ref(value);
        let actual = as_value(as_alias(value));
        assert!(
            std::ptr::eq(address, actual),
            "alias changed the native slot"
        );
        match index {
            0 => assert!(matches!(actual, Value::Undefined)),
            1 | 2 => {
                let Value::Number(number) = actual else {
                    panic!("alias changed the native number variant");
                };
                assert_eq!(
                    number.to_bits(),
                    if index == 1 {
                        0x8000000000000000
                    } else {
                        0x7ff0000000000000
                    }
                );
            }
            3 => assert_eq!(actual.as_units().unwrap(), [0xd800_u16, 0x0061]),
            4 => assert!(
                actual.strict_equal(&object),
                "alias detached the native object"
            ),
            5 => assert!(
                actual.strict_equal(&thrown),
                "alias replaced the thrown object"
            ),
            _ => unreachable!(),
        }
    }
    // The mutable references are invariant in their referent. These calls
    // authenticate both directions of each concrete compiler type equality.
    let object_alias = as_alias(&mut cases[4]);
    realm
        .set_str(as_value(object_alias), "marker", Value::Number(2.5))
        .unwrap();
    let Value::Number(marker) = realm.get_str(&object, "marker").unwrap() else {
        panic!("alias mutation lost the original native object");
    };
    assert_eq!(marker.to_bits(), 0x4004000000000000);
}

#[test]
fn private_value_realm_is_the_native_value() {
    native_invariants::<PrivateValue<Realm>>(|value| value, |value| value);
}

#[test]
fn private_error_realm_is_the_native_value() {
    native_invariants::<PrivateError<Realm>>(|value| value, |value| value);
}

#[test]
fn public_value_realm_is_the_native_value() {
    native_invariants::<PublicValue<Realm>>(|value| value, |value| value);
}

#[test]
fn public_error_realm_is_the_native_value() {
    native_invariants::<PublicError<Realm>>(|value| value, |value| value);
}

#[test]
fn exchange_value_realm_is_the_native_value() {
    native_invariants::<ExchangeValue<Realm>>(|value| value, |value| value);
}

#[test]
fn exchange_error_realm_is_the_native_value() {
    native_invariants::<ExchangeError<Realm>>(|value| value, |value| value);
}
