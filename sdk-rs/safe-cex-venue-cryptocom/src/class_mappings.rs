//! Native switch bindings preserve imported enum property reads and string identity.
use crate::native_bindings::BoundHost;
fn imported<V: Clone + 'static, E: 'static>(
    h: &BoundHost<V, E>,
    owner: &str,
    key: &str,
) -> Result<V, E> {
    h.get(&(h.ops.value.global)(owner)?, key)
}
pub fn member<V: Clone + 'static, E: 'static>(
    h: &BoundHost<V, E>,
    name: &str,
    value: V,
    private: bool,
) -> Result<V, E> {
    let input_enum = match name {
        "mapOrderType" if !private => Some("OrderType"),
        "mapTimeInForce" if !private => Some("OrderTimeInForce"),
        "mapStopTriggerType" => Some("OrderStopTriggerType"),
        _ => None,
    };
    let (cases, default, output): (&[(&str, &str)], &str, Option<&str>) = match name {
        "mapOrderType" if !private => (
            &[
                ("Market", "MARKET"),
                ("Limit", "LIMIT"),
                ("StopLoss", "STOP_LOSS"),
                ("TakeProfit", "TAKE_PROFIT"),
            ],
            "LIMIT",
            None,
        ),
        "mapTimeInForce" if !private => (
            &[
                ("ImmediateOrCancel", "IMMEDIATE_OR_CANCEL"),
                ("FillOrKill", "FILL_OR_KILL"),
                ("PostOnly", "POST_ONLY"),
            ],
            "GOOD_TILL_CANCEL",
            None,
        ),
        "mapStopTriggerType" => (
            &[("IndexPrice", "INDEX_PRICE"), ("LastPrice", "LAST_PRICE")],
            "MARK_PRICE",
            None,
        ),
        "mapOrderType" | "mapCryptocomOrderType" => (
            &[
                ("MARKET", "Market"),
                ("LIMIT", "Limit"),
                ("STOP_LOSS", "StopLoss"),
                ("STOP_LIMIT", "StopLoss"),
                ("TAKE_PROFIT", "TakeProfit"),
                ("TAKE_PROFIT_LIMIT", "TakeProfit"),
            ],
            "Limit",
            Some("OrderType"),
        ),
        "mapOrderStatus" => (
            &[
                ("ACTIVE", "Open"),
                ("PENDING", "Open"),
                ("FILLED", "Closed"),
                ("CANCELED", "Canceled"),
                ("EXPIRED", "Canceled"),
                ("REJECTED", "Canceled"),
            ],
            "Open",
            Some("OrderStatus"),
        ),
        "mapTimeInForce" | "mapCryptocomTimeInForce" => (
            &[
                ("IMMEDIATE_OR_CANCEL", "ImmediateOrCancel"),
                ("FILL_OR_KILL", "FillOrKill"),
                ("POST_ONLY", "PostOnly"),
            ],
            "GoodTillCancel",
            Some("OrderTimeInForce"),
        ),
        _ => unreachable!("every native mapping binding has a source switch"),
    };
    let mut result = default;
    for (case, target) in cases {
        let test = if let Some(owner) = input_enum {
            imported(h, owner, case)?
        } else {
            (h.ops.value.literal)(case)
        };
        if (h.ops.value.equal)(&value, &test) {
            result = target;
            break;
        }
    }
    if let Some(owner) = output {
        imported(h, owner, result)
    } else {
        Ok((h.ops.value.literal)(result))
    }
}

#[cfg(test)]
mod accepted_control_flow {
    use super::*;
    use safe_cex_exchange::js_runtime::value::{Js, Thrown};
    use std::{cell::RefCell, rc::Rc};
    fn token() -> Js {
        Js::object(vec![])
    }
    #[test]
    fn imported_native_contract() {
        let events = Rc::new(RefCell::new(vec![]));
        let globals = token();
        let enumeration = token();
        let child = token();
        let (expected, v, t) = (enumeration.clone(), child.clone(), events.clone());
        enumeration.define_accessor(
            "Ω.Key",
            Some(Rc::new(move |this| {
                assert!(
                    this.same_reference(&expected),
                    "imported native enum receiver"
                );
                t.borrow_mut().push("field");
                Ok(v.clone())
            })),
            None,
        );
        let (v, t) = (enumeration, events.clone());
        globals.define_accessor(
            "Enum",
            Some(Rc::new(move |_| {
                t.borrow_mut().push("global");
                Ok(v.clone())
            })),
            None,
        );
        let h = BoundHost {
            ops: Rc::new(crate::accepted_test_support::binding(globals)),
            receiver: token(),
        };
        assert!(
            imported(&h, "Enum", "Ω.Key")
                .unwrap()
                .same_reference(&child),
            "imported native value identity"
        );
        assert_eq!(
            *events.borrow(),
            ["global", "field"],
            "imported native evaluation order"
        );
        assert!(
            matches!(imported(&h, "Enum", "missing").unwrap(), Js::Undefined),
            "imported missing property forwarded"
        );
        let mut abrupt_executed = 0;
        for stage in 0..2 {
            let globals = token();
            let enumeration = token();
            let error = token();
            let (e, t) = (error.clone(), Rc::new(RefCell::new(vec![])));
            let events = t.clone();
            if stage == 0 {
                globals.define_accessor(
                    "Enum",
                    Some(Rc::new(move |_| {
                        t.borrow_mut().push("global");
                        Err(Thrown::new(e.clone()))
                    })),
                    None,
                )
            } else {
                enumeration.define_accessor(
                    "Key",
                    Some(Rc::new(move |_| {
                        t.borrow_mut().push("field");
                        Err(Thrown::new(e.clone()))
                    })),
                    None,
                );
                let t = events.clone();
                globals.define_accessor(
                    "Enum",
                    Some(Rc::new(move |_| {
                        t.borrow_mut().push("global");
                        Ok(enumeration.clone())
                    })),
                    None,
                )
            }
            let h = BoundHost {
                ops: Rc::new(crate::accepted_test_support::binding(globals)),
                receiver: token(),
            };
            match imported(&h, "Enum", "Key") {
                Err(e) => assert!(
                    e.value.same_reference(&error),
                    "imported original error identity"
                ),
                Ok(_) => panic!("imported swallowed error"),
            }
            assert_eq!(
                &*events.borrow(),
                if stage == 0 {
                    &["global"][..]
                } else {
                    &["global", "field"][..]
                },
                "imported native abrupt order"
            );
            abrupt_executed += 1;
        }
        assert_eq!(abrupt_executed, 2, "imported native abrupt executed cases");
    }
}
