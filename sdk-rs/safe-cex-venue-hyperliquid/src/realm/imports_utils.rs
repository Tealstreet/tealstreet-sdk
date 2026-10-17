//! Small shared utility imports (lodash, utils/*, json-parse), implemented over
//! realm values because several mutate or return their argument's identity.
use super::{
    Realm,
    value::{PropKey, Value},
};

fn arg(args: &[Value], i: usize) -> Value {
    args.get(i).cloned().unwrap_or(Value::Undefined)
}

pub(crate) fn bind(realm: &Realm) {
    // p-iteration's mapSeries: read live length/slots, await each input, then
    // await its callback before inspecting the next slot. Keep realm identities.
    let module = realm.singleton("pIterationModule", |r| r.new_object());
    let module_weak = std::rc::Rc::downgrade(module.object().expect("module object"));
    let map_series = realm.native("mapSeries", 3, move |r, _, args| {
        let array = arg(args, 0);
        let callback = arg(args, 1);
        let this_arg = arg(args, 2);
        let receiver = if this_arg.truthy() {
            this_arg
        } else {
            Value::Object(
                module_weak
                    .upgrade()
                    .ok_or_else(|| r.error("p-iteration module unavailable"))?,
            )
        };
        let mut realm = r.clone();
        Ok(r.spawn_value(Box::pin(async move {
            let result = realm.new_array(vec![]);
            let mut index = 0usize;
            loop {
                let length = realm.get_str(&array, "length")?;
                if !realm
                    .binary("<", &Value::Number(index as f64), &length)?
                    .truthy()
                {
                    break;
                }
                let key = PropKey::str(&index.to_string());
                if realm.has_property(&array, &key)? {
                    // Resolve callback.call before evaluating the awaited item.
                    let call = realm.get_str(&callback, "call")?;
                    let value = realm.get(&array, &key)?;
                    let value = realm.await_value(value).await?;
                    let pending = realm.call(
                        &call,
                        &callback,
                        &[
                            receiver.clone(),
                            value,
                            Value::Number(index as f64),
                            array.clone(),
                        ],
                    )?;
                    let value = realm.await_value(pending).await?;
                    realm.set_str(&result, &index.to_string(), value)?;
                }
                index += 1;
            }
            Ok(result)
        })))
    });
    module.object().expect("module object").borrow_mut().put(
        PropKey::str("mapSeries"),
        map_series.clone(),
        true,
    );
    realm.bind_import("mapSeries", map_series);

    // utils/get-key `v`: exact, then snake_case, then camelCase `in` checks.
    realm.bind_import(
        "v",
        realm.native("v", 2, |r, _, a| {
            let obj = arg(a, 0);
            if obj.is_nullish() {
                return Ok(Value::Undefined);
            }
            let key = r.to_rust_string(&arg(a, 1))?;
            for candidate in [
                key.clone(),
                safe_cex_num::lodash_snake_case(&key),
                safe_cex_num::lodash_camel_case(&key),
            ] {
                let k = PropKey::str(&candidate);
                if r.has_property(&obj, &k)? {
                    return r.get(&obj, &k);
                }
            }
            Ok(Value::Undefined)
        }),
    );
    // lodash/uniqBy: first occurrence wins, keyed by SameValueZero.
    realm.bind_import(
        "uniqBy",
        realm.native("uniqBy", 2, |r, _, a| {
            let items = if arg(a, 0).is_nullish() {
                vec![]
            } else {
                r.collect_iterable(&arg(a, 0))?
            };
            let iteratee = arg(a, 1);
            let mut keys: Vec<Value> = vec![];
            let mut out = vec![];
            for item in items {
                let key = if iteratee.is_callable() {
                    r.call(&iteratee, &Value::Undefined, std::slice::from_ref(&item))?
                } else if iteratee.is_nullish() {
                    item.clone()
                } else {
                    let k = r.to_property_key(&iteratee)?;
                    if item.is_nullish() {
                        Value::Undefined
                    } else {
                        r.get(&item, &k)?
                    }
                };
                if !keys.iter().any(|k| k.same_value_zero(&key)) {
                    keys.push(key);
                    out.push(item);
                }
            }
            Ok(r.new_array(out))
        }),
    );
    // lodash/flatten: one level.
    realm.bind_import(
        "flatten",
        realm.native("flatten", 1, |r, _, a| {
            let mut out = vec![];
            if !arg(a, 0).is_nullish() {
                for item in r.collect_iterable(&arg(a, 0))? {
                    if item.is_array() {
                        out.extend(r.collect_iterable(&item)?);
                    } else {
                        out.push(item);
                    }
                }
            }
            Ok(r.new_array(out))
        }),
    );
    realm.bind_import(
        "omitUndefined",
        realm.native("omitUndefined", 1, |r, _, a| {
            let out = r.new_object();
            let source = arg(a, 0);
            if !source.is_nullish() {
                for (k, v) in r.own_entries(&source)? {
                    if !matches!(v, Value::Undefined) {
                        r.define(&out, k, v)?;
                    }
                }
            }
            Ok(out)
        }),
    );
    realm.bind_import(
        "createPositionId",
        realm.native("createPositionId", 3, |r, _, a| {
            let symbol = arg(a, 0);
            let side = arg(a, 1);
            let one_way = matches!(arg(a, 2), Value::String(ref u) if String::from_utf16_lossy(u) == "oneway");
            if one_way || !side.truthy() {
                return Ok(symbol);
            }
            let mut out = r.template(&symbol)?.to_vec();
            out.push(b':' as u16);
            out.extend_from_slice(&r.template(&side)?);
            Ok(Value::String(out.into()))
        }),
    );
    // utils/loop: sleep the interval, then run fn on the next turn.
    realm.bind_import(
        "loop",
        realm.native("loop", 2, |r, _, a| {
            let f = arg(a, 0);
            let interval = arg(a, 1);
            let interval = match interval {
                Value::Number(n) if n == f64::INFINITY => {
                    return Ok(r.settled_promise(Ok(Value::Undefined)));
                }
                Value::Number(n) if n > 0.0 => n,
                _ => {
                    r.runtime().console(
                        "warning",
                        "[safe-cex] loop() called without a minInterval — defaulting to 5000ms.",
                    );
                    5000.0
                }
            };
            let sleep = r.runtime().sleep(interval as u64);
            let realm = r.clone();
            Ok(r.spawn_value(Box::pin(async move {
                let mut realm = realm;
                sleep.await.map_err(|e| realm.error(&e.message))?;
                let set_timeout = realm.global_binding("setTimeout")?;
                let run = {
                    let f = f.clone();
                    realm.native("", 0, move |r, _, _| r.call(&f, &Value::Undefined, &[]))
                };
                realm.call(&set_timeout, &Value::Undefined, &[run, Value::Number(0.0)])?;
                Ok(Value::Undefined)
            })))
        }),
    );
    // @tealstreet/json-parse jsonParse: JSON.parse or null.
    realm.bind_import(
        "jsonParse",
        realm.native("jsonParse", 1, |r, _, a| {
            let text = r.to_string(&arg(a, 0))?;
            Ok(r.parse_json(&text).unwrap_or(Value::Null))
        }),
    );
    // utils/orderbook calcOrderBookTotal: cumulative totals written in place.
    realm.bind_import(
        "calcOrderBookTotal",
        realm.native("calcOrderBookTotal", 1, |r, _, a| {
            let book = arg(a, 0);
            for side in ["bids", "asks"] {
                let levels = r.get_str(&book, side)?;
                let len = r.get_str(&levels, "length")?;
                let len = r.to_number(&len)? as usize;
                let mut total = Value::Number(0.0);
                for i in 0..len {
                    let level = r.get(&levels, &PropKey::str(&i.to_string()))?;
                    let amount = r.get_str(&level, "amount")?;
                    total = r.add(&total, &amount)?;
                    r.set_str(&level, "total", total.clone())?;
                }
            }
            Ok(Value::Undefined)
        }),
    );
    realm.bind_import(
        "adjust",
        realm.native("adjust", 2, |r, _, a| {
            let value = r.to_number(&arg(a, 0))?;
            let step = match arg(a, 1) {
                Value::String(u) => {
                    safe_cex_num::PrecisionInput::String(String::from_utf16_lossy(&u))
                }
                other => safe_cex_num::PrecisionInput::Number(r.to_number(&other)?),
            };
            Ok(Value::Number(safe_cex_num::adjust(value, step)))
        }),
    );
    // nanoid(): 21 URL-safe characters from the platform CSPRNG.
    realm.bind_import(
        "nanoid",
        realm.native("nanoid", 1, |r, _, a| {
            const ALPHABET: &[u8] =
                b"useandom-26T198340PX75pxJACKVERYMINDBUSHWOLF_GQZbfghjklqvwyzrict";
            let size = match arg(a, 0) {
                Value::Undefined => 21,
                n => r.to_number(&n)?.max(0.0) as usize,
            };
            let mut bytes = vec![0u8; size];
            getrandom::getrandom(&mut bytes)
                .map_err(|e| r.error(&format!("nanoid: CSPRNG unavailable: {e}")))?;
            let id: String = bytes
                .iter()
                .map(|b| ALPHABET[(b & 63) as usize] as char)
                .collect();
            Ok(Value::str(&id))
        }),
    );
}
