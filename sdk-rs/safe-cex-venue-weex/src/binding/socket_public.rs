use super::*;
use crate::public_feed::{self as feed, WeexPublicCallback, WeexPublicFeedRuntime};
fn callback(h: &Host, callback: WeexPublicCallback<Js>) -> Result<Js, Thrown> {
    let retained = std::matches!(
        callback,
        WeexPublicCallback::CandleUnsubscribe { .. } | WeexPublicCallback::BookUnsubscribe { .. }
    )
    .then(|| h.clone());
    let weak = h.weak();
    Ok(Js::function("", move |_, args| {
        let h = retained.clone().map_or_else(|| weak(), Ok)?;
        feed::run_public_callback(&h, &runtime(), &callback, arg(&args, 0))?;
        Ok(Js::Undefined)
    }))
}
pub(super) fn runtime() -> WeexPublicFeedRuntime<Host, Js, Thrown> {
    WeexPublicFeedRuntime {
        base_construct: |h, parent| base(h, ClassKind::Public, parent),
        define: v::define,
        bind_field: |h, receiver, name| v::method(h, receiver, name),
        new_set: |h| v::set(h, Js::Undefined),
        object: Js::object,
        array: Js::array,
        number: Js::Number,
        boolean: Js::Bool,
        text: Js::str,
        string_units: |s| Js::utf16(&s),
        undefined: || Js::Undefined,
        this_property: |h, name| h.root(name),
        property: v::get,
        optional_property: v::optional,
        lookup: |_, o, k| o.get_key(&k.to_js_string()?),
        assign: v::define,
        remove: |_, o, k| {
            o.delete(&k.to_js_string()?);
            Ok(())
        },
        truthy: Js::truthy,
        is_undefined: Js::is_undefined,
        equals_text: v::equals,
        is_false: |v| v.strict_equals(&Js::Bool(false)),
        is_array: |_, v| Ok(v.is_array()),
        string_constructor: |_, v| v::units(v),
        template_string: |_, v| v::units(v),
        to_number: |_, value, default| v::number(&value, default.to_number()?).map(Js::Number),
        number_value: |v| if let Js::Number(n) = v { *n } else { f64::NAN },
        abs: |_, v| Ok(v.to_number()?.abs()),
        multiply: |_, v, n| Ok(Js::Number(v.to_number()? * n)),
        to_integer_string: |_, v| v::integer(&v),
        method: v::method,
        call_event: |_, f, this, event, handler| {
            f.call(this, std::vec![Js::str(event), handler])?;
            Ok(())
        },
        disposed,
        create_socket: |h, url, protocols, options| {
            h.call("createWebsocket", std::vec![url, protocols, options])
        },
        set_socket: |h, socket| h.this().set("ws", socket),
        add_listener: |_, f, ws, event, handler| {
            f.call(ws, std::vec![Js::str(event), handler])?;
            Ok(())
        },
        track_received: |h| call_unit(h, "trackLastReceived", std::vec![]),
        parse: |h, units| parse(h, &units),
        subscription_id: |h| h.root("subscriptionId")?.to_number(),
        set_subscription_id: |h, id| h.this().set("subscriptionId", Js::Number(id)),
        send_pong: |h, id| frame(h, "PONG", None, id, false),
        handle_pong: |h, ms| call_unit(h, "handlePongEvent", std::vec![Js::Number(ms)]),
        warning_sink: |h| v::method(h, &parent(h)?, "log"),
        warning: |_, sink, message| {
            sink.call(
                Js::Undefined,
                std::vec![Js::utf16(&message), Js::str("warning")],
            )?;
            Ok(())
        },
        record_stats: |h, key| call_unit(h, "recordMessageHandlerStats", std::vec![key]),
        route_method: |h| v::method(h, &h.this(), "handleRoutedMessage"),
        route: |_, route, key, message| {
            route.call(Js::Undefined, std::vec![key, message.clone()])?;
            Ok(())
        },
        interval: |_, value| {
            super::super::classes::constant("INTERVAL")?.get_key(&value.to_js_string()?)
        },
        supported_interval: |v| {
            crate::types::SUPPORTED_WS_INTERVALS
                .iter()
                .any(|s| v::equals(v, s))
        },
        make_callback: callback,
        add_handler: |h, key, handler| {
            call_unit(h, "addHandler", std::vec![key.clone(), handler.clone()])
        },
        remove_handler: |h, key, handler| {
            call_unit(h, "removeHandler", std::vec![key.clone(), handler.clone()])
        },
        subscribe: |h, topic| call_unit(h, "subscribeTopic", std::vec![topic.clone()]),
        unsubscribe: |h, topic| call_unit(h, "unsubscribeTopic", std::vec![topic.clone()]),
        snapshot: |h, symbol, limit| {
            parent_call(
                h,
                "fetchOrderBook",
                std::vec![symbol.clone(), Js::Number(limit)],
            )
        },
        observe_snapshot: |h, pending, success, failure| {
            let retained = h.clone();
            h.spawn(Box::pin(async move {
                let result = async {
                    let value = await_js(pending).await?;
                    feed::run_public_callback(&retained, &runtime(), &success, value)
                }
                .await;
                if let Err(error) = result {
                    feed::run_public_callback(&retained, &runtime(), &failure, error.value)?;
                }
                Ok(Js::Undefined)
            }));
            Ok(())
        },
        books: |h| h.root("orderbooks"),
        put_book: |_, books, symbol, book| books.set_key(&symbol.to_js_string()?, book),
        invoke: |_, callback, value| {
            callback.call(Js::Undefined, std::vec![value])?;
            Ok(())
        },
        normalize_candle: v::candle,
        iterator: v::iterator,
        next: v::next,
        close_on_error: |_, _, _| {},
        pair: |_, row| Ok((row.get("0")?, row.get("1")?)),
        tickers_by_symbol: |h| h.root("store.tickersBySymbol"),
        ticker_updater: |h| v::method(h, &h.root("store")?, "updateTicker"),
        update_ticker: |_, update, symbol, patch| {
            update.call(Js::Undefined, std::vec![symbol, patch])?;
            Ok(())
        },
        update_price: |h, book, side, price, amount| {
            // The shared update operation preserves existing level identity and
            // overwrites amount; a new level starts with total=0 as ByPrice does.
            h.global("updateOrderBook")?.call(
                Js::Undefined,
                std::vec![
                    book.clone(),
                    Js::str(side),
                    Js::object(std::vec![
                        ("price", price),
                        ("amount", amount),
                        ("total", Js::Number(0.0)),
                    ]),
                ],
            )?;
            Ok(())
        },
        calc_totals: |h, book| {
            h.global("calcOrderBookTotal")?
                .call(Js::Undefined, std::vec![book.clone()])?;
            Ok(())
        },
        base_dispose,
        new_error: |_, message| Thrown::error("Error", &String::from_utf16_lossy(&message)),
    }
}
