//! Complete formatCryptocomApiError body at native property/record/template boundaries.
//! isRecord/stringifyResponseBody and JS operators remain explicit dependencies.
pub trait ApiErrorHost {
    type Value: Clone;
    type Error;
    fn undefined_value(&self) -> Self::Value;
    fn nullish(&self, value: &Self::Value) -> bool;
    fn undefined(&self, value: &Self::Value) -> bool;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn zero_number(&self, value: &Self::Value) -> bool;
    fn primitive_string(&self, value: &Self::Value) -> bool;
    fn get(&self, value: &Self::Value, key: &'static str) -> Result<Self::Value, Self::Error>;
    fn is_record(&self, value: &Self::Value) -> Result<bool, Self::Error>;
    fn stringify_response_body(&self, value: Self::Value) -> Result<Self::Value, Self::Error>;
    fn template(&self, value: Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn at_least_500(&self, value: &Self::Value) -> Result<bool, Self::Error>;
    fn string_value(&self, units: Vec<u16>) -> Self::Value;
}
fn optional_get<H: ApiErrorHost>(
    h: &H,
    value: H::Value,
    key: &'static str,
) -> Result<H::Value, H::Error> {
    if h.nullish(&value) {
        Ok(h.undefined_value())
    } else {
        h.get(&value, key)
    }
}
fn append(text: &mut Vec<u16>, literal: &str) {
    text.extend(literal.encode_utf16());
}
fn http_message<H: ApiErrorHost>(
    h: &H,
    prefix: &str,
    status: H::Value,
    body: H::Value,
) -> Result<H::Value, H::Error> {
    let mut message = prefix.encode_utf16().collect::<Vec<_>>();
    message.extend(h.template(status)?);
    append(&mut message, "): ");
    message.extend(h.template(body)?);
    Ok(h.string_value(message))
}
pub fn format_cryptocom_api_error<H: ApiErrorHost>(
    h: &H,
    error: H::Value,
) -> Result<H::Value, H::Error> {
    let response = optional_get(h, error.clone(), "response")?;
    let status = optional_get(h, response, "status")?;
    let response = optional_get(h, error, "response")?;
    let body = optional_get(h, response, "data")?;
    if h.undefined(&body) {
        return Ok(h.undefined_value());
    }
    if h.is_record(&body)? {
        let code = h.get(&body, "code")?;
        let message_test = h.get(&body, "message")?;
        let message = if h.primitive_string(&message_test) {
            h.get(&body, "message")?
        } else {
            h.undefined_value()
        };
        if !h.undefined(&code) || h.truthy(&message) {
            let mut text = "Crypto.com API Error".encode_utf16().collect::<Vec<_>>();
            if !h.undefined(&code) && !h.zero_number(&code) {
                append(&mut text, " ");
                text.extend(h.template(code)?);
            }
            if h.truthy(&message) {
                append(&mut text, ": ");
                text.extend(h.template(message)?);
            }
            return Ok(h.string_value(text));
        }
    }
    let body_string = h.stringify_response_body(body)?;
    if h.truthy(&status) && h.at_least_500(&status)? {
        return http_message(h, "Crypto.com proxy/transport error (", status, body_string);
    }
    if h.truthy(&status) {
        return http_message(h, "Crypto.com HTTP error (", status, body_string);
    }
    let mut message = "Crypto.com API error: ".encode_utf16().collect::<Vec<_>>();
    message.extend(h.template(body_string)?);
    Ok(h.string_value(message))
}

#[cfg(test)]
mod accepted_helpers {
    use super::*;
    use safe_cex_exchange::js_runtime::value::{Js, Thrown};
    use std::{cell::RefCell, rc::Rc};
    type Trace = Rc<RefCell<Vec<&'static str>>>;
    struct Host {
        events: Trace,
        template: Rc<dyn Fn(Js) -> Result<Vec<u16>, Thrown>>,
    }
    impl ApiErrorHost for Host {
        type Value = Js;
        type Error = Thrown;
        fn undefined_value(&self) -> Js {
            self.events.borrow_mut().push("undefined");
            Js::Undefined
        }
        fn nullish(&self, value: &Js) -> bool {
            self.events.borrow_mut().push("nullish");
            value.is_nullish()
        }
        fn undefined(&self, _: &Js) -> bool {
            panic!("unused undefined")
        }
        fn truthy(&self, _: &Js) -> bool {
            panic!("unused truthy")
        }
        fn zero_number(&self, _: &Js) -> bool {
            panic!("unused zero_number")
        }
        fn primitive_string(&self, _: &Js) -> bool {
            panic!("unused primitive_string")
        }
        fn get(&self, value: &Js, key: &'static str) -> Result<Js, Thrown> {
            self.events.borrow_mut().push("get");
            value.get(key)
        }
        fn is_record(&self, _: &Js) -> Result<bool, Thrown> {
            panic!("unused is_record")
        }
        fn stringify_response_body(&self, _: Js) -> Result<Js, Thrown> {
            panic!("unused stringify")
        }
        fn template(&self, value: Js) -> Result<Vec<u16>, Thrown> {
            (self.template)(value)
        }
        fn at_least_500(&self, _: &Js) -> Result<bool, Thrown> {
            panic!("unused comparison")
        }
        fn string_value(&self, units: Vec<u16>) -> Js {
            self.events.borrow_mut().push("string");
            Js::utf16(&units)
        }
    }
    fn trace() -> Trace {
        Rc::new(RefCell::new(vec![]))
    }
    fn token() -> Js {
        Js::object(vec![])
    }
    fn thrown<T>(result: Result<T, Thrown>, expected: &Js, label: &str) {
        match result {
            Err(e) => assert!(e.value.same_reference(expected), "{label}"),
            Ok(_) => panic!("{label}: swallowed error"),
        }
    }
    #[test]
    fn append_native_contract() {
        let mut units = vec![0xd800, 65];
        append(&mut units, "Ω🙂");
        assert_eq!(
            units,
            [0xd800, 65, 937, 0xd83d, 0xde42],
            "api_error append native UTF16"
        );
        append(&mut units, "");
        assert_eq!(
            units,
            [0xd800, 65, 937, 0xd83d, 0xde42],
            "api_error append empty preserves prefix"
        );
        append(&mut units, ": ");
        assert_eq!(
            units,
            [0xd800, 65, 937, 0xd83d, 0xde42, 58, 32],
            "api_error append native order"
        );
    }
    #[test]
    fn optional_get_native_contract() {
        let events = trace();
        let h = Host {
            events: events.clone(),
            template: Rc::new(|_| panic!("unused template")),
        };
        for value in [Js::Undefined, Js::Null] {
            events.borrow_mut().clear();
            assert!(
                matches!(optional_get(&h, value, "field").unwrap(), Js::Undefined),
                "api_error optional_get nullish result"
            );
            assert_eq!(
                *events.borrow(),
                ["nullish", "undefined"],
                "api_error optional_get nullish short circuit"
            );
        }
        let value = token();
        let child = token();
        value
            .define_key(&"field".encode_utf16().collect::<Vec<_>>(), child.clone())
            .unwrap();
        events.borrow_mut().clear();
        assert!(
            optional_get(&h, value, "field")
                .unwrap()
                .same_reference(&child),
            "api_error optional_get native identity"
        );
        assert_eq!(
            *events.borrow(),
            ["nullish", "get"],
            "api_error optional_get native getter"
        );
        for value in [Js::Bool(false), Js::Number(0.), Js::str("")] {
            events.borrow_mut().clear();
            assert!(matches!(
                optional_get(&h, value, "missing").unwrap(),
                Js::Undefined
            ));
            assert_eq!(
                *events.borrow(),
                ["nullish", "get"],
                "api_error optional_get falsey is not nullish"
            );
        }
        let error = token();
        let e = error.clone();
        let value = token();
        value.define_accessor(
            "field",
            Some(Rc::new(move |_| Err(Thrown::new(e.clone())))),
            None,
        );
        thrown(
            optional_get(&h, value, "field"),
            &error,
            "api_error optional_get original error",
        );
    }
    #[test]
    fn http_message_native_contract() {
        for stage in 0..3 {
            let events = trace();
            let status = token();
            let body = token();
            let error = token();
            let (s, b, e, t) = (status.clone(), body.clone(), error.clone(), events.clone());
            let h = Host {
                events: events.clone(),
                template: Rc::new(move |value| {
                    if value.same_reference(&s) {
                        t.borrow_mut().push("status");
                        if stage == 1 {
                            return Err(Thrown::new(e.clone()));
                        }
                        Ok(vec![53, 48, 48])
                    } else {
                        assert!(
                            value.same_reference(&b),
                            "api_error http_message original body"
                        );
                        t.borrow_mut().push("body");
                        if stage == 2 {
                            Err(Thrown::new(e.clone()))
                        } else {
                            Ok(vec![0xd800, 65])
                        }
                    }
                }),
            };
            let result = http_message(&h, "HTTP (", status, body);
            if stage == 0 {
                let actual = result.unwrap();
                assert!(
                    actual.strict_equals(&Js::utf16(&[
                        72, 84, 84, 80, 32, 40, 53, 48, 48, 41, 58, 32, 0xd800, 65
                    ])),
                    "api_error http_message native UTF16"
                );
                assert_eq!(
                    *events.borrow(),
                    ["status", "body", "string"],
                    "api_error http_message evaluation order"
                );
            } else {
                thrown(result, &error, "api_error http_message original error");
                assert_eq!(
                    &*events.borrow(),
                    if stage == 1 {
                        &["status"][..]
                    } else {
                        &["status", "body"][..]
                    },
                    "api_error http_message abrupt order"
                );
            }
        }
    }
}
