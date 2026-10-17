//! Native administrative class members, preserving original caller values.
use crate::{
    native_bindings::{BindingOps, BoundHost},
    socket_handlers::SocketFuture,
};
use std::rc::Rc;
fn arg<V: Clone, E>(h: &BindingOps<V, E>, args: &[V], i: usize) -> V {
    args.get(i)
        .cloned()
        .unwrap_or_else(|| (h.value.undefined)())
}
fn optional<V, E>(h: &BindingOps<V, E>, value: V, key: &str) -> Result<V, E> {
    if (h.value.nullish)(&value) {
        Ok((h.value.undefined)())
    } else {
        (h.value.get)(&value, key)
    }
}
fn post<V: Clone + 'static, E: 'static>(
    h: &BoundHost<V, E>,
    key: &str,
    params: impl FnOnce() -> Result<V, E>,
) -> Result<V, E> {
    let xhr = h.get(&h.receiver, "xhr")?;
    let callee = h.prepare(xhr, "post")?;
    let endpoint = h.get(&(h.ops.value.global)("ENDPOINTS")?, key)?;
    h.call(callee, vec![endpoint, params()?])
}
pub fn dispose<V: Clone + 'static, E: 'static>(h: &BoundHost<V, E>) -> Result<V, E> {
    (h.ops.value.set)(&h.receiver, "isDisposed", (h.ops.value.boolean)(true))?;
    let socket = h.get(&h.receiver, "publicWebsocket")?;
    h.method(socket, "dispose", vec![])?;
    let socket = h.get(&h.receiver, "privateWebsocket")?;
    h.method(socket, "dispose", vec![])?;
    Ok((h.ops.value.undefined)())
}
pub fn get_account<V: Clone + 'static, E: 'static>(h: Rc<BoundHost<V, E>>) -> SocketFuture<V, E> {
    let begun = post(&h, "ACCOUNTS", || Ok((h.ops.value.object)(vec![])));
    Box::pin(async move {
        let response = (h.ops.value.await_value)(begun?).await?;
        let data = h.get(&response, "data")?;
        let account = h.get(&data, "this_account")?;
        let id = h.get(&account, "uuid")?;
        Ok((h.ops.value.object)(vec![
            ("userId", id),
            ("affiliateId", (h.ops.null)()),
        ]))
    })
}
pub fn validate_account<V: Clone + 'static, E: Clone + 'static>(
    h: Rc<BoundHost<V, E>>,
) -> SocketFuture<V, E> {
    let setup = (|| {
        let previous = h.get(&h.receiver, "_suppressCryptocomValidationErrorEmission")?;
        (h.ops.value.set)(
            &h.receiver,
            "_suppressCryptocomValidationErrorEmission",
            (h.ops.value.boolean)(true),
        )?;
        Ok(previous)
    })();
    let begun = setup.map(|previous| {
        let begun = post(&h, "BALANCE", || Ok((h.ops.value.object)(vec![])));
        (previous, begun)
    });
    Box::pin(async move {
        let (previous, begun) = begun?;
        let result: Result<V, E> = async {
            let tried = async {
                (h.ops.value.await_value)(begun?).await?;
                let error = (h.ops.value.literal)("");
                let returned = h.method(h.receiver.clone(), "fetchBalance", vec![])?;
                let catch = h.prepare(returned, "catch")?;
                let undefined = h.ops.value.undefined.clone();
                let callback = (h.ops.value.function)("", 0, Rc::new(move |_| Ok(undefined())));
                let handled = h.call(catch, vec![callback])?;
                let balance = (h.ops.value.await_value)(handled).await?;
                Ok((h.ops.value.object)(vec![
                    ("error", error),
                    ("balance", balance),
                ]))
            }
            .await;
            match tried {
                Ok(value) => Ok(value),
                Err(error) => {
                    let error = (h.ops.value.error_value)(error);
                    let response = optional(&h.ops, error.clone(), "response")?;
                    let data = optional(&h.ops, response, "data")?;
                    let message = optional(&h.ops, data, "message")?;
                    let message = if (h.ops.value.truthy)(&message) {
                        message
                    } else {
                        let fallback = optional(&h.ops, error.clone(), "message")?;
                        if (h.ops.value.truthy)(&fallback) {
                            fallback
                        } else {
                            (h.ops.value.literal)("Invalid API key or secret")
                        }
                    };
                    let check = (h.ops.value.global)("isCryptocomAuthenticationErrorCode")?;
                    let response = optional(&h.ops, error, "response")?;
                    let data = optional(&h.ops, response, "data")?;
                    let code = optional(&h.ops, data, "code")?;
                    let auth = (h.ops.value.invoke)(check, (h.ops.value.undefined)(), vec![code])?;
                    let auth = if (h.ops.value.truthy)(&auth) {
                        true
                    } else {
                        let value = h.global(
                            "isCryptocomAuthenticationFailureMessage",
                            vec![message.clone()],
                        )?;
                        (h.ops.value.truthy)(&value)
                    };
                    if !auth {
                        h.method(h.receiver.clone(), "emitAndLogError", vec![message.clone()])?;
                    }
                    Ok((h.ops.value.object)(vec![("error", message)]))
                }
            }
        }
        .await;
        (h.ops.value.set)(
            &h.receiver,
            "_suppressCryptocomValidationErrorEmission",
            previous,
        )?;
        result
    })
}
pub fn admin_post<V: Clone + 'static, E: Clone + 'static>(
    h: Rc<BoundHost<V, E>>,
    name: &'static str,
    args: Vec<V>,
) -> SocketFuture<V, E> {
    let (key, prefix) = if name == "setLeverage" {
        ("CHANGE_LEVERAGE", "Error setting leverage: ")
    } else {
        ("CANCEL_ALL_ORDERS", "Error canceling all orders: ")
    };
    let begun = post(&h, key, || {
        if name == "setLeverage" {
            let symbol = arg(&h.ops, &args, 0);
            let leverage = arg(&h.ops, &args, 1);
            let string = h.method(leverage, "toString", vec![])?;
            Ok((h.ops.value.object)(vec![
                ("instrument_name", symbol),
                ("leverage", string),
            ]))
        } else {
            Ok((h.ops.value.object)(vec![]))
        }
    });
    Box::pin(async move {
        let result: Result<V, E> = async {
            (h.ops.value.await_value)(begun?).await?;
            Ok((h.ops.value.undefined)())
        }
        .await;
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                let log = h.prepare(h.receiver.clone(), "log")?;
                let value = (h.ops.value.error_value)(error.clone());
                let message = (h.ops.value.utf16)(
                    prefix
                        .encode_utf16()
                        .chain((h.ops.value.text)(&value)?)
                        .collect(),
                );
                let severity = h.get(&(h.ops.value.global)("LogSeverity")?, "Error")?;
                h.call(log, vec![message, severity])?;
                Err(error)
            }
        }
    })
}
pub fn change_position_mode<V: Clone + 'static, E: 'static>(
    h: Rc<BoundHost<V, E>>,
) -> SocketFuture<V, E> {
    let result = (|| {
        let log = h.prepare(h.receiver.clone(), "log")?;
        let message = (h.ops.value.literal)("Position mode is always hedged on Crypto.com");
        let severity = h.get(&(h.ops.value.global)("LogSeverity")?, "Warning")?;
        h.call(log, vec![message, severity])?;
        Ok((h.ops.value.undefined)())
    })();
    Box::pin(async move { result })
}
pub fn change_margin_type<V: Clone + 'static, E: 'static>(
    h: Rc<BoundHost<V, E>>,
    args: Vec<V>,
) -> SocketFuture<V, E> {
    let result = (|| {
        let constructor = (h.ops.value.global)("Error")?;
        let isolated = arg(&h.ops, &args, 0);
        let symbol = arg(&h.ops, &args, 1);
        let symbol = if (h.ops.value.nullish)(&symbol) {
            (h.ops.value.literal)("a position")
        } else {
            symbol
        };
        let mut message = "Crypto.com does not support switching "
            .encode_utf16()
            .collect::<Vec<_>>();
        message.extend((h.ops.value.text)(&symbol)?);
        message.extend(" to ".encode_utf16());
        message.extend(
            if (h.ops.value.truthy)(&isolated) {
                "isolated"
            } else {
                "cross"
            }
            .encode_utf16(),
        );
        message.extend(" margin through Exchange v1".encode_utf16());
        let error = (h.ops.construct)(constructor, vec![(h.ops.value.utf16)(message)])?;
        Err((h.ops.throw_value)(error))
    })();
    Box::pin(async move { result })
}

#[cfg(test)]
mod accepted_control_flow {
    use super::*;
    use safe_cex_exchange::js_runtime::value::{Js, Thrown};
    use std::cell::RefCell;
    #[test]
    fn optional_native_contract() {
        let events = Rc::new(RefCell::new(vec![]));
        let mut ops = crate::accepted_test_support::binding(Js::object(vec![]));
        let t = events.clone();
        Rc::get_mut(&mut ops.value).unwrap().get = Rc::new(move |v, k| {
            t.borrow_mut().push("get");
            v.get(k)
        });
        let mut nullish_executed = 0;
        for value in [Js::Undefined, Js::Null] {
            events.borrow_mut().clear();
            assert!(
                matches!(optional(&ops, value, "field").unwrap(), Js::Undefined),
                "class optional native nullish result"
            );
            assert!(
                events.borrow().is_empty(),
                "class optional nullish skips getter"
            );
            nullish_executed += 1;
        }
        assert_eq!(nullish_executed, 2, "class optional nullish executed cases");
        let parent = Js::object(vec![]);
        let child = Js::object(vec![]);
        parent
            .define_key(&"field".encode_utf16().collect::<Vec<_>>(), child.clone())
            .unwrap();
        assert!(
            optional(&ops, parent, "field")
                .unwrap()
                .same_reference(&child),
            "class optional native child identity"
        );
        assert_eq!(*events.borrow(), ["get"], "class optional native getter");
        let mut falsey_executed = 0;
        for value in [Js::Bool(false), Js::Number(-0.), Js::str("")] {
            events.borrow_mut().clear();
            assert!(matches!(
                optional(&ops, value, "missing").unwrap(),
                Js::Undefined
            ));
            assert_eq!(
                *events.borrow(),
                ["get"],
                "class optional falsey is not nullish"
            );
            falsey_executed += 1;
        }
        assert_eq!(falsey_executed, 3, "class optional falsey executed cases");
        let error = Js::object(vec![]);
        let e = error.clone();
        let parent = Js::object(vec![]);
        parent.define_accessor(
            "field",
            Some(Rc::new(move |_| Err(Thrown::new(e.clone())))),
            None,
        );
        match optional(&ops, parent, "field") {
            Err(e) => assert!(
                e.value.same_reference(&error),
                "class optional native original error"
            ),
            Ok(_) => panic!("class optional swallowed error"),
        }
    }
}
