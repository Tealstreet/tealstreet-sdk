//! Predicates and join on the actual Scope retained by a constructed exchange.
use super::accepted_read_receivers::terminal::{harness, options};
use crate::api::ApiHost;
use safe_cex_exchange::js_runtime::value::{Js, Thrown, error_object};
use safe_cex_types::ExchangeName;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn network_error(method: Js, code: Js, response: Js) -> Js {
    Js::object(vec![
        ("config", Js::object(vec![("method", method)])),
        ("code", code),
        ("response", response),
    ])
}
fn result(predicate: &Js, error: Js) -> Result<bool, Thrown> {
    predicate.call(Js::Undefined, vec![error]).map(|value| {
        assert!(matches!(value, Js::Bool(_)));
        value.truthy()
    })
}
#[test]
fn retry_condition_matches_network_exclusions_and_live_source_access_order() {
    let h = harness(ExchangeName::Kraken, options(), |_| {});
    let predicate = ApiHost::retry_condition(&h.exchange.rt, None).unwrap();
    for code in [
        "ERR_CANCELED",
        "ECONNABORTED",
        "ENOTFOUND",
        "CERT_HAS_EXPIRED",
    ] {
        assert!(
            !result(
                &predicate,
                network_error(Js::str("GET"), Js::str(code), Js::Undefined)
            )
            .unwrap(),
            "must not retry {code}"
        );
    }
    for response in [
        Js::Undefined,
        Js::Null,
        Js::Bool(false),
        Js::Number(0.0),
        Js::str(""),
    ] {
        assert!(
            result(
                &predicate,
                network_error(Js::str("GET"), Js::str("ECONNRESET"), response)
            )
            .unwrap()
        );
    }
    // Network codes are compared strictly, never coerced to UTF8 or strings.
    for code in [Js::Number(7.0), Js::utf16(&[0xd800]), Js::object(vec![])] {
        assert!(
            result(
                &predicate,
                network_error(Js::str("GET"), code, Js::Undefined)
            )
            .unwrap()
        );
    }
    for code in [
        Js::Number(-0.0),
        Js::Bool(false),
        Js::Null,
        Js::Undefined,
        Js::str(""),
    ] {
        assert!(
            !result(
                &predicate,
                network_error(Js::str("GET"), code, Js::Undefined)
            )
            .unwrap()
        );
    }
    let trace = Rc::new(RefCell::new(vec![]));
    let config = Js::object(vec![]);
    let seen = trace.clone();
    config
        .define_accessor(
            "method",
            Some(Rc::new(move |_| {
                seen.borrow_mut().push("method");
                Ok(Js::str("GET"))
            })),
            None,
        )
        .unwrap();
    config
        .define_accessor(
            "url",
            Some(Rc::new(|_| panic!("GET must not read URL"))),
            None,
        )
        .unwrap();
    let error = Js::object(vec![]);
    let seen = trace.clone();
    let parent = config.clone();
    error
        .define_accessor(
            "config",
            Some(Rc::new(move |_| {
                seen.borrow_mut().push("config");
                Ok(parent.clone())
            })),
            None,
        )
        .unwrap();
    let seen = trace.clone();
    error
        .define_accessor(
            "response",
            Some(Rc::new(move |_| {
                seen.borrow_mut().push("response");
                Ok(Js::Undefined)
            })),
            None,
        )
        .unwrap();
    let seen = trace.clone();
    error
        .define_accessor(
            "code",
            Some(Rc::new(move |_| {
                seen.borrow_mut().push("code");
                Ok(Js::str("ECONNRESET"))
            })),
            None,
        )
        .unwrap();
    assert!(result(&predicate, error).unwrap());
    assert_eq!(
        *trace.borrow(),
        vec![
            "config", "config", "method", "response", "code", "code", "code"
        ]
    );
    let original = error_object("RangeError", "URL getter");
    let throwing = original.clone();
    let write = Js::object(vec![("method", Js::str("POST"))]);
    write
        .define_accessor(
            "url",
            Some(Rc::new(move |_| Err(Thrown::new(throwing.clone())))),
            None,
        )
        .unwrap();
    let write_error = Js::object(vec![("config", write), ("code", Js::str("ECONNRESET"))]);
    assert!(!result(&predicate, write_error.clone()).unwrap());
    let posted = ApiHost::retry_condition(&h.exchange.rt, Some(vec![])).unwrap();
    assert!(
        result(&posted, write_error)
            .unwrap_err()
            .value
            .strict_equals(&original)
    );
    assert!(result(&predicate, Js::Null).is_err());
    assert!(
        result(
            &predicate,
            network_error(Js::Number(42.0), Js::str("ECONNRESET"), Js::Undefined)
        )
        .is_err()
    );
    let method = Js::object(vec![(
        "toLowerCase",
        Js::function("source method", |_, _| Ok(Js::str("get"))),
    )]);
    assert!(
        result(
            &predicate,
            network_error(method, Js::str("ECONNRESET"), Js::Undefined)
        )
        .unwrap()
    );
}

#[test]
fn spot_retry_condition_short_circuits_and_stamps_original_config_once() {
    use safe_cex_transport::signature_rejection::SIGNATURE_RETRY_CONFIG_KEY;
    let h = harness(ExchangeName::Krakenspot, options(), |_| {});
    let predicate =
        ApiHost::spot_retry_condition(&h.exchange.rt, vec!["/0/private/Balance"]).unwrap();
    let original = error_object("RangeError", "response getter must be skipped");
    let thrown = original.clone();
    let missing = Js::object(vec![]);
    missing
        .define_accessor(
            "response",
            Some(Rc::new(move |_| Err(Thrown::new(thrown.clone())))),
            None,
        )
        .unwrap();
    assert!(!result(&predicate, missing).unwrap());
    let config = Js::object(vec![
        ("method", Js::str("POST")),
        ("url", Js::str("/0/private/AddOrder")),
    ]);
    let data = Js::object(vec![(
        "error",
        Js::array(vec![Js::str("EAPI:Invalid signature")]),
    )]);
    let responses = Rc::new(Cell::new(0));
    let gets = responses.clone();
    let response = Js::object(vec![("data", data.clone())]);
    let error = Js::object(vec![("config", config.clone())]);
    error
        .define_accessor(
            "response",
            Some(Rc::new(move |_| {
                gets.set(gets.get() + 1);
                Ok(response.clone())
            })),
            None,
        )
        .unwrap();
    assert!(result(&predicate, error.clone()).unwrap());
    assert_eq!(responses.get(), 2);
    assert!(
        config
            .get(SIGNATURE_RETRY_CONFIG_KEY)
            .unwrap()
            .strict_equals(&Js::Bool(true))
    );
    assert!(!result(&predicate, error).unwrap());
    assert_eq!(responses.get(), 3);
    assert_eq!(
        data.get("error").unwrap().array_values().unwrap()[0].to_display(),
        "EAPI:Invalid signature"
    );
    let read = Js::object(vec![
        ("method", Js::str("POST")),
        ("url", Js::str("/0/private/Balance?asset=XBT")),
    ]);
    assert!(
        result(
            &predicate,
            Js::object(vec![
                ("config", read.clone()),
                ("code", Js::str("ECONNRESET"))
            ])
        )
        .unwrap()
    );
    assert!(read.get(SIGNATURE_RETRY_CONFIG_KEY).unwrap().is_undefined());
}

#[test]
fn join_calls_one_live_member_with_original_receiver_and_retained_result() {
    let h = harness(ExchangeName::Kraken, options(), |_| {});
    let value = Js::array(vec![
        Js::Undefined,
        Js::Null,
        Js::utf16(&[0xd800]),
        Js::Number(-0.0),
        Js::Number(f64::INFINITY),
    ]);
    let joined = ApiHost::join(&h.exchange.rt, &value, "|").unwrap();
    let expected: Vec<u16> = [
        vec![124, 124, 0xd800, 124, 48, 124],
        "Infinity".encode_utf16().collect(),
    ]
    .concat();
    assert_eq!(joined.as_str().unwrap().as_ref(), expected.as_slice());
    let object = Js::object(vec![]);
    let original = object.clone();
    let gets = Rc::new(Cell::new(0));
    let observed = gets.clone();
    let cell = Js::object(vec![("bits", Js::Number(-0.0))]);
    let returned = cell.clone();
    object
        .define_accessor(
            "join",
            Some(Rc::new(move |_| {
                observed.set(observed.get() + 1);
                let owner = original.clone();
                let output = returned.clone();
                Ok(Js::function("source join", move |this, args| {
                    assert!(this.strict_equals(&owner));
                    assert_eq!(args.len(), 1);
                    assert_eq!(args[0].to_display(), "|");
                    Ok(output.clone())
                }))
            })),
            None,
        )
        .unwrap();
    assert!(
        ApiHost::join(&h.exchange.rt, &object, "|")
            .unwrap()
            .strict_equals(&cell)
    );
    assert_eq!(gets.get(), 1);
    let error = error_object("TypeError", "join getter failed");
    let token = error.clone();
    object
        .define_accessor(
            "join",
            Some(Rc::new(move |_| Err(Thrown::new(token.clone())))),
            None,
        )
        .unwrap();
    assert!(
        ApiHost::join(&h.exchange.rt, &object, "|")
            .unwrap_err()
            .value
            .strict_equals(&error)
    );
    assert!(ApiHost::join(&h.exchange.rt, &Js::Number(1.0), "|").is_err());
}
