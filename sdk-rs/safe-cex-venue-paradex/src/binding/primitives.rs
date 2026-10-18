use super::{Rt, text};
use num_bigint::BigInt;
use safe_cex_exchange::js_runtime::{
    ops,
    value::{Js, Thrown, arg},
};
use std::rc::Rc;
pub(super) fn call(rt: &Rt, n: &str, a: Vec<Js>) -> Result<Js, Thrown> {
    let x = arg(&a, 0);
    let y = arg(&a, 1);
    Ok(match n {
        "signerHeaders" => Js::array(
            crate::request_helpers::signer_headers(x.truthy().then_some(&x))
                .into_iter()
                .map(|config| Js::object(vec![("headers", config.headers.clone())]))
                .collect(),
        ),
        "entries" => Js::array(
            iterate(&x)?
                .into_iter()
                .enumerate()
                .map(|(index, value)| Js::array(vec![Js::Number(index as f64), value]))
                .collect(),
        ),
        "filter.join" => Js::array(iterate(&x)?.into_iter().filter(Js::truthy).collect())
            .call_method("join", vec![y])?,
        "literal.includes.instructions" => Js::array(
            ["GTC", "POST_ONLY", "IOC", "RPI"]
                .into_iter()
                .map(Js::str)
                .collect(),
        )
        .call_method("includes", vec![x])?,
        "literal.includes.legacy" => {
            Js::array(["GTT", "FOK", "IOC"].into_iter().map(Js::str).collect())
                .call_method("includes", vec![x])?
        }
        "destructureUpdate" => Js::object(vec![
            ("order", x.get("order")?),
            ("update", x.get("update")?),
        ]),
        "destructureExecution" => Js::object(
            ["symbol", "orderId", "from", "to", "limit"]
                .into_iter()
                .map(|key| Ok((key, x.get(key)?)))
                .collect::<Result<_, Thrown>>()?,
        ),
        "JSON.stringify.ownNames" => rt.global("JSON.stringify")?.call(
            Js::Undefined,
            vec![
                x.clone(),
                rt.global("Object.getOwnPropertyNames")?
                    .call(Js::Undefined, vec![x])?,
            ],
        )?,
        "addOrUpdatePositions" => rt
            .this
            .get("store")?
            .call_method("addOrUpdatePositions", vec![x])?,
        "callbacks.has" | "callbacks.get" | "callbacks.delete" => rt
            .this
            .get("orderbookCallbacks")?
            .call_method(n.split('.').nth(1).unwrap(), vec![x])?,
        "callbacks.setNew" => rt
            .this
            .get("orderbookCallbacks")?
            .call_method("set", vec![x, Js::new_set()])?,
        "starknetKeccak" => {
            let hash = starknet_core::utils::starknet_keccak(text(&y)?.as_bytes());
            Js::BigInt(Rc::new(BigInt::from_bytes_be(
                num_bigint::Sign::Plus,
                &hash.to_bytes_be(),
            )))
        }

        "null" => Js::Null,
        "text" => Js::utf16(
            &x.array_values()
                .ok_or_else(|| Thrown::type_error("text units must be an array"))?
                .iter()
                .map(|v| v.to_number().map(|n| n as u16))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        "add" => ops::add(&x, &y)?,
        "subtract" => ops::subtract(&x, &y)?,
        "multiply" => ops::multiply(&x, &y)?,
        "member" => x.get_key(&y.to_js_string()?)?,
        "assignMember" => {
            x.set_key(&y.to_js_string()?, arg(&a, 2))?;
            Js::Undefined
        }
        "spread" => x.spread_object()?,
        "spread.concat" => {
            let mut out = x
                .array_values()
                .ok_or_else(|| Thrown::type_error("value is not iterable"))?;
            out.extend(
                y.array_values()
                    .ok_or_else(|| Thrown::type_error("value is not iterable"))?,
            );
            Js::array(out)
        }
        "new Map" => rt.global("Map")?.construct(vec![], None)?,
        "new Set" => Js::new_set(),
        "Set.from" => rt.global("Set")?.construct(vec![x], None)?,
        "Array.from.values" => {
            let values = x.call_method("values", vec![])?;
            Js::array(iterate(&values)?)
        }
        "Map.set" | "Map.get" | "Map.has" | "Set.add" | "Set.has" | "Set.delete" => {
            let method = n.split('.').nth(1).unwrap();
            x.call_method(method, a.into_iter().skip(1).collect())?
        }
        "leverage.get" | "leverage.set" => rt
            .this
            .get("leverageSettings")?
            .call_method(n.split('.').nth(1).unwrap(), a)?,
        "includes" | "array.includes" => x.call_method("includes", vec![y])?,
        "optional.endsWith" => {
            if x.is_nullish() {
                Js::Undefined
            } else {
                x.call_method("endsWith", vec![y])?
            }
        }
        "endsWith" => x.call_method("endsWith", vec![y])?,
        "String.upper" => x.call_method("toUpperCase", vec![])?,
        "replace" => x.call_method("replace", vec![y, arg(&a, 2)])?,
        "split.member" => x
            .call_method("split", vec![y])?
            .get_key(&arg(&a, 2).to_js_string()?)?,
        "keys.join" => rt
            .global("Object.keys")?
            .call(Js::Undefined, vec![x])?
            .call_method("join", vec![y])?,
        "flat" => x.call_method("flat", vec![])?,
        "push" | "push.spread" => {
            let rest = if n == "push.spread" {
                iterate(&y)?
            } else {
                a.into_iter().skip(1).collect()
            };
            x.call_method("push", rest)?
        }
        "join" => x.call_method("join", vec![y])?,
        "slice" => x.call_method("slice", a.into_iter().skip(1).collect())?,
        "trim" => x.call_method("trim", vec![])?,
        "number.toString" | "method.toString" | "bigint.toString" => {
            x.call_method("toString", vec![])?
        }
        "radix16" => x.call_method("toString", vec![Js::Number(16.)])?,
        "virtualClock" => rt
            .global("virtualClock.getCurrentTimeMs")?
            .call(Js::Undefined, vec![])?,
        "connectPublic" => rt
            .this
            .get("publicWebsocket")?
            .call_method("connectAndSubscribe", vec![])?,
        "connectPrivate" => rt
            .this
            .get("privateWebsocket")?
            .call_method("connectAndSubscribe", vec![])?,
        "destructurePair" => {
            let mut it = iterate(&x)?.into_iter();
            Js::array(vec![
                it.next().unwrap_or(Js::Undefined),
                it.next().unwrap_or(Js::Undefined),
            ])
        }
        "describeError" => match rt
            .global("JSON.stringify")?
            .call(Js::Undefined, vec![x.clone()])
        {
            Ok(v) => v,
            Err(_) => rt.global("String")?.call(Js::Undefined, vec![x])?,
        },
        "timeframe" => {
            let key = text(&x)?;
            Js::Number(
                *safe_cex_types::TIMEFRAME_TO_SECONDS
                    .get(key.as_str())
                    .unwrap_or(&f64::NAN),
            )
        }
        "tradingViewResolution" => Js::str(
            &crate::exchange_helpers::convert_interval_to_trading_view_resolution(&text(&x)?)
                .map_err(|e| Thrown::error("Error", &e))?,
        ),
        "String" | "Number" | "Number.isFinite" | "JSON.stringify" | "JSON.parse" | "Math.abs"
        | "Math.floor" | "Math.max" | "Math.min" | "Date.now" | "sumBy" | "adjust"
        | "omitUndefined" | "createPositionId" => rt.global(n)?.call(Js::Undefined, a)?,
        "toFixed" => x.call_method("toFixed", vec![y])?,
        "Object.create" => {
            let out = Js::object(vec![]);
            if let Js::Object(o) = &out {
                *o.proto.borrow_mut() = Some(x);
            }
            out
        }
        "RegExp" => {
            safe_cex_exchange::js_runtime::regexp::regexp(&x.to_js_string()?, &y.to_js_string()?)?
        }
        "broadcast" => {
            for callback in iterate(&x)? {
                callback.call(Js::Undefined, vec![y.clone()])?;
            }
            Js::Undefined
        }
        "ORDER_SIDE" => {
            let table = Js::object(
                crate::constants::ORDER_SIDE
                    .iter()
                    .map(|(k, v)| Ok((*k, safe_cex_exchange::js_runtime::value::from_typed(v)?)))
                    .collect::<Result<_, Thrown>>()?,
            );
            table.get_key(&x.to_js_string()?)?
        }
        "isTrigger" => Js::Bool(crate::constants::is_paradex_trigger_order_type(&text(&x)?)),
        "has" => Js::Bool(x.has_own(&text(&y)?)),
        "TextEncoder.encode" => Js::array(
            text(&x)?
                .as_bytes()
                .iter()
                .map(|b| Js::Number(*b as f64))
                .collect(),
        ),
        "BigInt.zero" => Js::BigInt(Rc::new(BigInt::from(0))),
        "BigInt.hex" => Js::BigInt(Rc::new(
            ops::string_to_bigint(&x.to_js_string()?)
                .ok_or_else(|| Thrown::error("SyntaxError", "Cannot convert value to a BigInt"))?,
        ))
        .call_method("toString", vec![Js::Number(16.)])?,
        "BigInt.shift8.or" => {
            let Js::BigInt(x) = x else {
                return Err(Thrown::type_error("value is not BigInt"));
            };
            Js::BigInt(Rc::new(
                (x.as_ref() << 8) | BigInt::from(y.to_number()? as u8),
            ))
        }
        "regex.hex" | "regex.decimal" => {
            let pattern = if n == "regex.hex" {
                "^0x[0-9a-fA-F]+$"
            } else {
                "^[0-9]+$"
            };
            safe_cex_exchange::js_runtime::regexp::regexp(
                &pattern.encode_utf16().collect::<Vec<_>>(),
                &[],
            )?
            .call_method("test", vec![x])?
        }
        "parseInt10" => rt
            .global("parseInt")?
            .call(Js::Undefined, vec![x, Js::Number(10.0)])?,
        "roundPriceToStep" => Js::Number(crate::exchange_helpers::round_price_to_step(
            x.to_number()?,
            y.to_number()?,
            &text(&arg(&a, 2))?,
        )),
        "observeInit" => {
            x.call_method(
                "catch",
                vec![Js::function("observedInit", |_, _| Ok(Js::Undefined))],
            )?;
            Js::Undefined
        }
        _ => rt.global(n)?.call(Js::Undefined, a)?,
    })
}

pub(super) fn iterate(v: &Js) -> Result<Vec<Js>, Thrown> {
    v.iterable_values()
}
