//! Exact production Host instantiations; no claim for any other host or decoder.
use safe_cex_exchange::js_runtime::value::{Js, Thrown};
use crate::{
    core_exchange_class::{ExchangeError, ExchangeValue},
    private_class::{PrivateError, PrivateValue},
    public_class::{PublicError, PublicValue},
    host::{Host, test_runtime::TestRuntime},
    ws_ping_lifecycle::WsPingHost,
};
use std::{any::TypeId, rc::Rc};

fn require_production_associated_types<H: WsPingHost<Value = Js, Error = Thrown>>() {}

fn native_invariants<A: 'static>(
    as_alias: fn(&mut Js) -> &mut A,
    as_value: fn(&mut A) -> &mut Js,
) {
    require_production_associated_types::<Host>();
    assert_eq!(TypeId::of::<A>(), TypeId::of::<Js>());
    let mut realm = crate::host::test_runtime::host(Rc::new(TestRuntime::default()));
    let object = realm.object_from(vec![("marker", Js::Number(-0.0))]);
    let thrown = realm.type_error("native thrown object");
    let mut cases = [
        Js::Undefined,
        Js::Number(-0.0),
        Js::Number(f64::INFINITY),
        Js::String(Rc::from([0xd800_u16, 0x0061])),
        object.clone(),
        thrown.value.clone(),
    ];
    for (index, value) in cases.iter_mut().enumerate() {
        let address = std::ptr::from_ref(value);
        let actual = as_value(as_alias(value));
        assert!(
            std::ptr::eq(address, actual),
            "alias changed the native slot"
        );
        match index {
            0 => assert!(matches!(actual, Js::Undefined)),
            1 | 2 => {
                let Js::Number(number) = actual else {
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
            3 => assert_eq!(actual.as_str().unwrap().as_ref(), [0xd800_u16, 0x0061]),
            4 => assert!(
                actual.strict_equals(&object),
                "alias detached the native object"
            ),
            5 => assert!(
                actual.strict_equals(&thrown.value),
                "alias replaced the thrown object"
            ),
            _ => unreachable!(),
        }
    }
    // The mutable references are invariant in their referent. These calls
    // authenticate both directions of each concrete compiler type equality.
    let object_alias = as_alias(&mut cases[4]);
    realm
        .set_str(as_value(object_alias), "marker", Js::Number(2.5))
        .unwrap();
    let Js::Number(marker) = realm.get_str(&object, "marker").unwrap() else {
        panic!("alias mutation lost the original native object");
    };
    assert_eq!(marker.to_bits(), 0x4004000000000000);
}

#[test]
fn private_value_realm_is_the_native_value() {
    native_invariants::<PrivateValue<Host>>(|value| value, |value| value);
}

#[test]
fn private_error_realm_is_the_native_value() {
    native_error_invariants::<PrivateError<Host>>(|value| value, |value| value);
}

#[test]
fn public_value_realm_is_the_native_value() {
    native_invariants::<PublicValue<Host>>(|value| value, |value| value);
}

#[test]
fn public_error_realm_is_the_native_value() {
    native_error_invariants::<PublicError<Host>>(|value| value, |value| value);
}

#[test]
fn exchange_value_realm_is_the_native_value() {
    native_invariants::<ExchangeValue<Host>>(|value| value, |value| value);
}

#[test]
fn exchange_error_realm_is_the_native_value() {
    native_error_invariants::<ExchangeError<Host>>(|value| value, |value| value);
}

fn native_error_invariants<A: 'static>(as_alias: fn(&mut Thrown) -> &mut A, as_error: fn(&mut A) -> &mut Thrown) {
    require_production_associated_types::<Host>();
    assert_eq!(TypeId::of::<A>(), TypeId::of::<Thrown>());
    let object = Js::object(vec![("marker",Js::Number(-0.0))]);
    let original_error = safe_cex_exchange::js_runtime::value::error_object("TypeError", "native thrown object");
    let values = [Js::Undefined, Js::Number(-0.0), Js::Number(f64::INFINITY), Js::utf16(&[0xd800,0x0061]), object.clone(), original_error];
    assert_eq!(values.len(), 6);
    for value in values {
        let mut thrown = Thrown::new(value.clone());
        let address = std::ptr::from_ref(&thrown);
        let actual = as_error(as_alias(&mut thrown));
        assert!(std::ptr::eq(address, actual), "error alias changed the canonical completion slot");
        assert!(actual.value.same_reference(&value) || actual.value.strict_equals(&value));
        if let (Js::Number(actual), Js::Number(expected)) = (&actual.value, &value) { assert_eq!(actual.to_bits(),expected.to_bits()); }
        if actual.value.same_reference(&object) {
            actual.value.set("marker",Js::Number(2.5)).unwrap();
            assert_eq!(object.get("marker").unwrap().as_f64().unwrap().to_bits(), 2.5_f64.to_bits());
        }
    }
}
