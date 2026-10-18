//! Small shared utility imports (lodash, utils/*, json-parse), implemented over
//! realm values because several mutate or return their argument's identity.
use super::Host;
use safe_cex_exchange::js_runtime::value::Js;

fn arg(args: &[Js], i: usize) -> Js {
    args.get(i).cloned().unwrap_or(Js::Undefined)
}

pub(crate) fn bind(realm: &Host) {
    // utils/get-key `v`: exact, then snake_case, then camelCase `in` checks.
    realm.bind_import(
        "v",
        realm.native("v", 2, |r, _, a| {
            let obj = arg(a, 0);
            if obj.is_nullish() {
                return Ok(Js::Undefined);
            }
            let key = r.to_rust_string(&arg(a, 1))?;
            for candidate in [
                key.clone(),
                safe_cex_num::lodash_snake_case(&key),
                safe_cex_num::lodash_camel_case(&key),
            ] {
                let k = Js::str(&candidate);
                if r.has_property(&obj, &k)? {
                    return r.get(&obj, &k);
                }
            }
            Ok(Js::Undefined)
        }),
    );
    // lodash/uniqBy: first occurrence wins, keyed by SameValueZero.
    realm.bind_import(
        "uniqBy",
        realm
            .shared
            .globals
            .get("uniqBy")
            .expect("installed shared utility"),
    );

    // lodash/flatten: one level.
    realm.bind_import(
        "flatten",
        realm
            .shared
            .globals
            .get("flatten")
            .expect("installed shared utility"),
    );

    realm.bind_import(
        "omitUndefined",
        realm.native("omitUndefined", 1, |r, _, a| {
            let out = r.new_object();
            let source = arg(a, 0);
            if !source.is_nullish() {
                for (k, v) in r.own_entries(&source)? {
                    if !matches!(v, Js::Undefined) {
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
            let one_way =
                matches!(arg(a, 2), Js::String(ref u) if String::from_utf16_lossy(u) == "oneway");
            if one_way || !side.truthy() {
                return Ok(symbol);
            }
            let mut out = r.template(&symbol)?.to_vec();
            out.push(b':' as u16);
            out.extend_from_slice(&r.template(&side)?);
            Ok(Js::String(out.into()))
        }),
    );
    // utils/loop: sleep the interval, then run fn on the next turn.
    realm.bind_import(
        "loop",
        realm
            .shared
            .globals
            .get("loop")
            .expect("installed shared utility"),
    );

    // @tealstreet/json-parse jsonParse: JSON.parse or null.
    realm.bind_import(
        "jsonParse",
        realm
            .shared
            .globals
            .get("jsonParse")
            .expect("installed shared utility"),
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
                let mut total = Js::Number(0.0);
                for i in 0..len {
                    let level = r.get(&levels, &Js::str(&i.to_string()))?;
                    let amount = r.get_str(&level, "amount")?;
                    total = r.add(&total, &amount)?;
                    r.set_str(&level, "total", total.clone())?;
                }
            }
            Ok(Js::Undefined)
        }),
    );
    realm.bind_import(
        "adjust",
        realm
            .shared
            .globals
            .get("adjust")
            .expect("installed shared utility"),
    );

    // nanoid(): 21 URL-safe characters from the platform CSPRNG.
    realm.bind_import(
        "nanoid",
        realm
            .shared
            .globals
            .get("nanoid")
            .expect("installed shared utility"),
    );
}
