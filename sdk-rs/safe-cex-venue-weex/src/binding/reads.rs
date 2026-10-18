use super::values as v;
use super::*;
use crate::{account_reads as a, income_reads as i, market_reads as m};

pub(super) fn market_runtime() -> m::WeexMarketReadRuntime<Host, Js, Thrown> {
    m::WeexMarketReadRuntime {
        request: |h, request| {
            let config = Js::object(std::vec![]);
            if let Some(params) = request.params {
                config.define("params", params)?;
            }
            if let Some(timeout) = request.timeout {
                config.define("timeout", Js::Number(timeout))?;
            }
            h.request("get", Js::str(request.endpoint), config)
        },
        all: |_, a, b| {
            Ok(Box::pin(
                async move { futures::future::try_join(a, b).await },
            ))
        },
        get: v::get,
        optional_get: v::optional,
        truthy: Js::truthy,
        is_undefined: Js::is_undefined,
        is_nullish: Js::is_nullish,
        equals_text: v::equals,
        equals: Js::strict_equals,
        number: Js::Number,
        text: Js::str,
        undefined: || Js::Undefined,
        object: Js::object,
        array: Js::array,
        to_string: |_, value| Ok(Js::utf16(&value.to_js_string()?)),
        template_string: |_, value| v::units(value),
        to_integer_string: |_, value| v::integer(value),
        to_number: |_, value| v::number(value, 0.0),
        less_than: |_, a, b| safe_cex_exchange::js_runtime::ops::less_than(a, b),
        greater_than: |_, a, b| safe_cex_exchange::js_runtime::ops::less_than(b, a),
        map: |_, array, mut callback| v::map(array, |value, index| callback(value, index, array)),
        filter: |_, array, mut callback| {
            v::filter(array, |value, index| callback(&value, index, array))
        },
        reduce: |_, array, mut accum, mut callback| {
            let n = v::length(array)?;
            for index in 0..n {
                if array.has_own(&index.to_string()) {
                    accum = callback(accum, v::at(array, index)?, index, array)?;
                }
            }
            Ok(accum)
        },
        sort: |_, array, mut compare| {
            let mut values = array
                .array_values()
                .ok_or_else(|| Thrown::type_error("value.sort is not a function"))?;
            // Timestamp normalizers return finite numbers; stable ties retain source order.
            for index in 1..values.len() {
                let mut i = index;
                while i > 0 && compare(&values[i - 1], &values[i])? > 0.0 {
                    values.swap(i - 1, i);
                    i -= 1;
                }
            }
            if let Js::Array(items) = &array {
                *items.borrow_mut() = values.into_iter().map(Some).collect();
            }
            Ok(array)
        },
        is_array: |_, value| Ok(value.is_array()),
        new_map: v::new_map,
        map_getter: |h, map| v::method(h, map, "get"),
        map_lookup: |_, get, key| get.call(Js::Undefined, std::vec![key]),
        push_method: |h, array| v::method(h, array, "push"),
        push: |_, push, value| {
            push.call(Js::Undefined, std::vec![value])?;
            Ok(())
        },
        markets: |h| h.root("store.markets"),
        tickers: |h| h.root("store.tickers"),
        markets_by_symbol: |h| h.root("store.marketsBySymbol"),
        market_lookup: |_, markets, key| markets.get_key(&key.to_js_string()?),
        normalize_market: v::market,
        normalize_ticker: v::ticker,
        normalize_candle: v::candle,
        normalize_order_book: v::book,
        normalize_public_trade: v::public_trade,
        normalize_execution: v::execution,
        normalize_timestamp_ms: v::timestamp,
        interval: |_, value| super::classes::constant("INTERVAL")?.get_key(&value.to_js_string()?),
        supported_interval: |value| {
            crate::types::SUPPORTED_INTERVALS
                .iter()
                .any(|s| v::equals(value, s))
        },
        copy_properties: |_, value| value.spread_object(),
        define_property: v::define,
        extract_array: |h, value| v::extract(h, value, &[]),
        omit_undefined: v::omit,
        public_only: |h| Ok(h.root("isPublicDataOnly")?.truthy()),
        provider: |h| h.root("externalPublicDataProvider"),
        public_websocket: |h| h.root("publicWebsocket"),
        method: v::method,
        call: |_, function, receiver, opts, callback| {
            function.call(receiver, std::vec![opts, callback])
        },
        error_value: |e| e.value.clone(),
        error_sink: |h| v::method(h, &h.this(), "emitAndLogError"),
        emit_error: |_, sink, error| {
            sink.call(Js::Undefined, std::vec![error])?;
            Ok(())
        },
        new_error: |_, s| Thrown::error("Error", &String::from_utf16_lossy(&s)),
    }
}
pub(super) fn account_runtime() -> a::WeexAccountReadRuntime<Host, Js, Thrown> {
    a::WeexAccountReadRuntime {
        request: |h, request| {
            let config = Js::object(std::vec![]);
            if let Some(params) = request.params {
                config.define("params", Js::object(params))?;
            }
            let pending = h.request("get", Js::utf16(&request.endpoint), config)?;
            Ok(Box::pin(async move { pending.await?.get("data") }))
        },
        config: |h, symbol, refetch| {
            let pending = h.call(
                "getOrFetchSymbolConfig",
                std::vec![symbol, Js::Bool(refetch)],
            )?;
            Ok(Box::pin(await_js(&h.0.exchange_runtime(), pending)))
        },
        balance: |h| {
            let pending = h.call("fetchBalance", std::vec![])?;
            Ok(Box::pin(await_js(&h.0.exchange_runtime(), pending)))
        },
        positions: |h| {
            let pending = h.call("fetchPositions", std::vec![])?;
            Ok(Box::pin(await_js(&h.0.exchange_runtime(), pending)))
        },
        all: |_, a, b| {
            Ok(Box::pin(
                async move { futures::future::try_join(a, b).await },
            ))
        },
        settled: |_, a, b| Ok(Box::pin(async move { futures::future::join(a, b).await })),
        public_only: |h| Ok(h.root("isPublicDataOnly")?.truthy()),
        markets_by_symbol: |h| h.root("store.marketsBySymbol"),
        market: |_, markets, key| markets.get_key(&key.to_js_string()?),
        get: v::get,
        optional_get: v::optional,
        truthy: Js::truthy,
        equals_text: v::equals,
        is_undefined: Js::is_undefined,
        to_string: |_, value| v::units(value),
        number: Js::Number,
        undefined: || Js::Undefined,
        null: || Js::Null,
        to_number: |_, value, default| v::number(value, default),
        array: Js::array,
        object: Js::object,
        length: |_, value| v::length(value),
        present: |_, value, i| Ok(value.has_own(&i.to_string())),
        at: |_, value, i| v::at(value, i),
        iterator: v::iterator,
        next: v::next,
        close_on_error: |_, _, _| {},
        extract_array: |h, value, keys| v::extract(h, value.clone(), keys),
        normalize_balance: |_, rows| {
            from_typed(
                &crate::utils::normalize_balance(&v::capture(&rows)?).map_err(v::utility_error)?,
            )
        },
        normalize_order: |h, row, market| {
            crate::trading::normalize_order(h, &super::trading::runtime(), row, market)
        },
        normalize_algo_order: |h, row, market| {
            crate::trading::normalize_algo_order(h, &super::trading::runtime(), row, market)
        },
        is_array: |_, value| Ok(value.is_array()),
        uniq_by_id: |h, value| {
            h.global("uniqBy")?
                .call(Js::Undefined, std::vec![value, Js::str("id")])
        },
        store_updater: |h| {
            h.0.owner_and_member("store.update")
                .map(|(owner, f)| Js::function("update", move |_, a| f.call(owner.clone(), a)))
        },
        fetched_leverage: |h| h.root("store.fetchedLeverage"),
        copy_properties: |_, value| value.spread_object(),
        set_property: |_, object, key, value| object.set_key(&key.to_js_string()?, value),
        store_update: |_, f, value| {
            f.call(Js::Undefined, std::vec![value])?;
            Ok(())
        },
        cache: |h| h.root("symbolSettings"),
        config_setter: |h, cache| v::method(h, cache, "set"),
        set_config: |_, set, key, config| {
            set.call(Js::Undefined, std::vec![key, v::default_config(config)?])?;
            Ok(())
        },
        push_method: |h, array| v::method(h, array, "push"),
        push_spread: |_, push, values| {
            push.call(
                Js::Undefined,
                values
                    .array_values()
                    .ok_or_else(|| Thrown::type_error("value is not iterable"))?,
            )?;
            Ok(())
        },
        error_message: |_, error| v::units(&v::optional_dummy(&error.value, "message")?),
        warning: |h, s| h.warn(Js::utf16(&s)),
    }
}
pub(super) fn income_runtime() -> i::WeexIncomeRuntime<Host, Js, Thrown> {
    i::WeexIncomeRuntime {
        public_only: |h| Ok(h.root("isPublicDataOnly")?.truthy()),
        request_factory: |h| v::method(h, &h.root("xhr")?, "post"),
        post: |h, post, endpoint, payload| {
            let pending = post.call(Js::Undefined, std::vec![Js::str(endpoint), payload])?;
            Ok(Box::pin(await_js(&h.0.exchange_runtime(), pending)))
        },
        optional_get: v::optional,
        get: v::get,
        truthy: Js::truthy,
        nullish: Js::is_nullish,
        undefined: || Js::Undefined,
        null: || Js::Null,
        text: Js::str,
        number: Js::Number,
        numeric: |_, value| value.to_number(),
        object: Js::object,
        array: Js::array,
        extract_array: v::extract,
        map: |_, array, mut callback| v::map(array, |value, _| callback(value)),
        normalize: v::income,
    }
}
pub(super) fn range_runtime() -> i::WeexIncomeRangeRuntime<Host, Js> {
    i::WeexIncomeRangeRuntime {
        fetch_page: |h, params, kind| {
            let opts = Js::object(std::vec![
                ("startTime", Js::Number(params.start_time)),
                ("endTime", Js::Number(params.end_time)),
                ("limit", Js::Number(params.limit)),
                ("incomeType", kind),
            ]);
            let pending = h
                .call("fetchIncomeHistory", std::vec![opts])
                .map_err(|e| e.to_exchange_error())?;
            Ok(Box::pin(async move {
                to_typed(
                    &await_js(&h.0.exchange_runtime(), pending)
                        .await
                        .map_err(|e| e.to_exchange_error())?,
                )
                .map_err(|e| e.to_exchange_error())
            }))
        },
        sleep: |h, ms| Ok(h.0.shared.runtime.sleep(ms.max(0.0) as u64)),
        warning_sink: |h| v::method(h, &h.this(), "log").map_err(|e| e.to_exchange_error()),
        warning: |_, sink, s| {
            sink.call(Js::Undefined, std::vec![Js::utf16(&s), Js::str("warning")])
                .map_err(|e| e.to_exchange_error())?;
            Ok(())
        },
        iso_date: |h, ms| {
            h.global("Date")
                .and_then(|date| date.call(Js::Undefined, std::vec![Js::Number(ms)]))
                .and_then(|date| date.call_method("toISOString", std::vec![]))
                .and_then(|s| v::units(&s))
                .map_err(|e| e.to_exchange_error())
        },
    }
}
