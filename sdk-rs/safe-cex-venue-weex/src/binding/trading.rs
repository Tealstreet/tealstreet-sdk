use super::values as v;
use super::*;
use crate::trading::WeexTradingRuntime;
use safe_cex_types::*;

pub(super) fn runtime() -> WeexTradingRuntime<Host, Js, Thrown> {
    WeexTradingRuntime {
        get: v::get,
        optional_get: v::optional,
        truthy: Js::truthy,
        nullish: Js::is_nullish,
        is_undefined: Js::is_undefined,
        undefined: || Js::Undefined,
        null: || Js::Null,
        text: Js::str,
        string_units: |v| Js::utf16(&v),
        number: Js::Number,
        boolean: Js::Bool,
        object: Js::object,
        array: Js::array,
        string: |_, value| Ok(Js::utf16(&value.to_js_string()?)),
        template: |_, value| v::units(value),
        uppercase: |_, value| value.call_method("toUpperCase", std::vec![]),
        equals: v::equals,
        strict_true: |v| v.strict_equals(&Js::Bool(true)),
        strict_false: |v| v.strict_equals(&Js::Bool(false)),
        is_string: Js::is_string,
        is_boolean: |v| std::matches!(v, Js::Bool(_)),
        is_object: Js::is_object,
        is_number: |v| std::matches!(v, Js::Number(_)),
        number_value: |v| if let Js::Number(n) = v { *n } else { f64::NAN },
        to_number: |_, value, default| v::number(&value, default.to_number()?).map(Js::Number),
        integer_string: |_, value| v::integer(&value),
        enum_normalize: |_, kind, value| v::normalize(kind, value),
        table: |_, name, key| super::classes::constant(name)?.get_key(&key.to_js_string()?),
        timestamp: |h, value| v::timestamp(h, value).map(Js::Number),
        position_id: |_, symbol, side, mode| {
            let side: PositionSide = to_typed(&side)?;
            let mode: PositionMode = to_typed(&mode)?;
            Ok(Js::str(&model_utils::create_position_id(
                &symbol.to_display(),
                Some(side),
                Some(mode),
            )))
        },
        copy: |_, value| value.spread_object(),
        define: v::define,
        omit: v::omit,
        adjust: |_, value, step| {
            Ok(Js::Number(safe_cex_num::adjust(
                value.to_number()?,
                step.to_number()?,
            )))
        },
        uuid: |h| {
            Ok(Js::str(
                &h.0.shared.runtime.unique_id().replace(['-', '_'], ""),
            ))
        },
        method: |h, name| match name {
            // Resolve the lowered Store operation with its source receiver.
            "updateStoredOrder" => v::method(h, &h.root("store")?, "updateOrder"),
            _ => v::method(h, &h.this(), name),
        },
        call: v::invoke,
        call_async: v::invoke_async,
        request: |h, method, endpoint, data| {
            let pending = h.root("xhr")?.call_method(
                &method.to_ascii_lowercase(),
                std::vec![Js::str(endpoint), data],
            )?;
            Ok(Box::pin(await_js(&h.0.exchange_runtime(), pending)))
        },
        error_value: |e| e.value.clone(),
        error_sink: |h| v::method(h, &h.this(), "emitAndLogError"),
        emit: |_, sink, value| {
            sink.call(Js::Undefined, std::vec![value])?;
            Ok(())
        },
        new_error: |_, value| Thrown::error("Error", &value.to_display()),
        map: |_, value, callback| v::map(value, callback),
        filter: |_, value, callback| v::filter(value, callback),
        some: |_, value, mut callback| {
            let n = v::length(value)?;
            for i in 0..n {
                if value.has_own(&i.to_string()) && callback(v::at(value, i)?, i)? {
                    return Ok(true);
                }
            }
            Ok(false)
        },
        for_each: |_, value, mut callback| {
            let n = v::length(value)?;
            for i in 0..n {
                if value.has_own(&i.to_string()) {
                    callback(v::at(value, i)?, i)?;
                }
            }
            Ok(())
        },
        flat_map: |_, value, callback| {
            let mapped = v::map(value, callback)?;
            mapped.call_method("flat", std::vec![])
        },
        new_set: |h, value| v::set(h, value.clone()),
        set_has: |_, set, key| Ok(set.call_method("has", std::vec![key.clone()])?.truthy()),
        new_map: |h| v::new_map(h, Js::Undefined),
        map_has: |_, map, key| Ok(map.call_method("has", std::vec![key.clone()])?.truthy()),
        map_get: |_, map, key| map.call_method("get", std::vec![key.clone()]),
        map_set: |_, map, key, value| {
            map.call_method("set", std::vec![key, value])?;
            Ok(())
        },
        length: |_, value| v::length(value),
        is_array: |_, value| Ok(value.is_array()),
        includes: |_, value, needle| {
            Ok(value
                .call_method("includes", std::vec![needle.clone()])?
                .truthy())
        },
        set_size: |h, values| Ok(v::set(h, values.clone())?.get("size")?.to_number()? as usize),
        push_method: |h, value| v::method(h, value, "push"),
        push: |_, f, array, value| {
            f.call(array.clone(), std::vec![value])?;
            Ok(())
        },
        spread_push: |_, f, array, values| {
            f.call(
                array.clone(),
                values
                    .array_values()
                    .ok_or_else(|| Thrown::type_error("value is not iterable"))?,
            )?;
            Ok(())
        },
        iterator: v::iterator,
        next: v::next,
        close_on_error: |_, _, _| {},
        contains: |_, value, text| {
            Ok(value
                .call_method("includes", std::vec![Js::str(text)])?
                .truthy())
        },
        has: |_, value, key| Ok(value.has_own(key)),
        valid_order_id: |value| {
            let Js::String(units) = value else {
                return false;
            };
            units.first().is_some_and(|unit| (49..=57).contains(unit))
                && units.iter().all(|unit| (48..=57).contains(unit))
        },
        markets_by_symbol: |h| h.root("store.marketsBySymbol"),
        lookup: |_, map, key| map.get_key(&key.to_js_string()?),
        markets: |h| h.root("store.markets"),
        public_only: |h| Ok(h.root("isPublicDataOnly")?.truthy()),
        positions: |h| h.root("store.positions"),
        account_mode: |h| h.root("accountPositionMode"),
        fake_positions: |_, market, mode, margin, leverage| {
            from_typed(&crate::utils::fake_positions_for_market(
                &to_typed(&market)?,
                to_typed(&mode)?,
                to_typed(&margin)?,
                leverage.to_number()?,
            ))
        },
        extract_array: |h, value| v::extract(h, value, &[]),
        join: |_, value, sep| value.call_method("join", std::vec![Js::str(sep)]),
        unique_id: |h, value| {
            h.global("uniqBy")?
                .call(Js::Undefined, std::vec![value, Js::str("id")])
        },
        concat: |_, first, second| {
            let mut values = first
                .array_values()
                .ok_or_else(|| Thrown::type_error("value is not iterable"))?;
            values.extend(
                second
                    .array_values()
                    .ok_or_else(|| Thrown::type_error("value is not iterable"))?,
            );
            Ok(Js::array(values))
        },
    }
}
