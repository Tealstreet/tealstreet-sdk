use super::*;
use crate::private_feed::{self as feed, WeexPrivateFeedRuntime, WeexRefreshKind};
use std::rc::Rc;
fn timer_name(kind: WeexRefreshKind) -> &'static str {
    match kind {
        WeexRefreshKind::Orders => "refreshOrdersTimeout",
        WeexRefreshKind::Positions => "refreshPositionsTimeout",
        WeexRefreshKind::Balance => "refreshBalanceTimeout",
    }
}
fn auth(h: &Host) -> Result<Js, Thrown> {
    let headers = control::create_auth_headers(
        &mut h.clone(),
        &control::WeexAuthHeaderRuntime {
            time_ms: |_| Ok(safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms()),
            credential: |h, key| parent(h)?.get("options")?.get(key),
            string: |_, s| Js::utf16(s),
            sign: |_, secret, message| {
                Ok(Js::str(&crate::signing::sign_weex_message(
                    &v::units(&secret)?,
                    message,
                )))
            },
        },
    )?;
    Ok(Js::object(headers.into()))
}
fn unsupported(h: &Host, reason: Option<&[u16]>) -> Result<(), Thrown> {
    let mut state = control_state(h)?;
    let result =
        control::log_unsupported_transport(&mut h.clone(), &mut state, reason, &control_runtime());
    save_control(h, &state)?;
    result
}
fn runtime() -> WeexPrivateFeedRuntime<Host, Js, Js, Js, Thrown> {
    WeexPrivateFeedRuntime {
        disposed,
        supported_runtime: |_| Ok(!std::cfg!(target_arch = "wasm32")),
        credential: |h, key| parent(h)?.get("options")?.get(key),
        truthy: Js::truthy,
        undefined: Js::is_undefined,
        string_equals: v::equals,
        is_false: |v| v.strict_equals(&Js::Bool(false)),
        make_string: |_, s| Ok(Js::str(s)),
        template_string: |_, v| v::units(v),
        stringify: |_, v| v::units(v),
        property: v::get,
        is_array: |_, v| Ok(v.is_array()),
        make_string_units: |_, s| Ok(Js::utf16(&s)),
        make_undefined: |_| Ok(Js::Undefined),
        uppercase: |_, s| {
            Js::utf16(s)
                .call_method("toUpperCase", std::vec![])
                .and_then(|s| v::units(&s))
        },
        map_length: |_, v| v::length(v),
        find_length: |_, v| v::length(v),
        array_element: |_, v, index| {
            if v.has_own(&index.to_string()) {
                v::at(v, index).map(Some)
            } else {
                Ok(None)
            }
        },
        empty_array: |_| Ok(Js::array(std::vec![])),
        iterator: v::iterator,
        iterator_next: v::next,
        iterator_close: |_, _| Ok(()),
        parse: |h, s| parse(h, s),
        number: |_, v| v::number(v, 0.0),
        timestamp_seconds: |h, v| {
            let value = v::maybe_capture(v)?;
            crate::utils::normalize_timestamp_to_seconds(value.as_ref(), |ms| {
                (h.0.shared.extra.local_to_utc)(ms)
            })
            .map_err(v::coercion)
        },
        markets_by_symbol: |h| h.root("store.marketsBySymbol"),
        market: |_, markets, key| markets.get_key(&key.to_js_string()?),
        normalize_order: |h, row, market| {
            crate::trading::normalize_order(
                h,
                &super::super::trading::runtime(),
                row.clone(),
                market.clone(),
            )
        },
        normalize_position: |h, row, market| {
            crate::trading::normalize_position(
                h,
                &super::super::trading::runtime(),
                row.clone(),
                market.clone(),
                Js::Undefined,
            )
        },
        stored_orders: |h| h.root("store.orders"),
        strict_equal: Js::strict_equals,
        add_orders: |h, orders| {
            call_unit(
                h,
                "store.addOrUpdateOrders",
                std::vec![Js::array(orders.clone())],
            )
        },
        order_status: |v| v.get("status").ok().and_then(|v| to_typed(&v).ok()),
        update_status: |h, id, status| {
            call_unit(
                h,
                "store.updateOrderStatus",
                std::vec![Js::object(std::vec![("id", id)]), status],
            )
        },
        add_positions: |h, positions| {
            call_unit(
                h,
                "store.addOrUpdatePositions",
                std::vec![Js::array(positions)],
            )
        },
        emit_fill: |h, fill| {
            parent_call(
                h,
                "emitter.emit",
                std::vec![
                    Js::str("fill"),
                    Js::object(std::vec![
                        ("id", Js::utf16(&fill.id)),
                        ("orderId", Js::utf16(&fill.order_id)),
                        ("side", fill.side),
                        ("symbol", fill.symbol),
                        ("price", Js::Number(fill.price)),
                        ("amount", Js::Number(fill.amount)),
                        ("timestamp", Js::Number(fill.timestamp)),
                        ("fee", Js::Number(fill.fee)),
                        ("realizedPnl", Js::Number(fill.realized_pnl)),
                    ]),
                ],
            )?;
            Ok(())
        },
        track_received: |h| call_unit(h, "trackLastReceived", std::vec![]),
        subscription_id: |h| h.root("subscriptionId")?.to_number(),
        set_subscription_id: |h, id| h.this().set("subscriptionId", Js::Number(id)),
        send_pong: |h, id| frame(h, "PONG", None, id, false),
        handle_pong: |h, ms| call_unit(h, "handlePongEvent", std::vec![Js::Number(ms)]),
        warning: |h, message| warn(h, Js::utf16(&message)),
        record_stats: |h, key| call_unit(h, "recordMessageHandlerStats", std::vec![key.clone()]),
        log_unsupported: |h, reason| {
            unsupported(
                h,
                reason
                    .map(|s| s.encode_utf16().collect::<Vec<_>>())
                    .as_deref(),
            )
        },
        auth_headers: auth,
        create_socket: |h, url, protocols, provider, required| {
            let weak = h.weak();
            let headers = Js::function("headers", move |_, _| provider(&weak()?));
            h.call(
                "createWebsocket",
                std::vec![
                    Js::str(url),
                    protocols.map_or(Js::Undefined, |ps| {
                        Js::array(ps.iter().map(|s| Js::str(s)).collect())
                    }),
                    Js::object(std::vec![
                        ("headers", headers),
                        ("headersRequired", Js::Bool(required)),
                    ]),
                ],
            )
        },
        set_socket: |h, ws| h.this().set("ws", ws),
        socket: |h| h.root("ws"),
        add_listener: |h, ws, event| {
            let member = match event {
                "open" => "onOpen",
                "message" => "onMessage",
                _ => "onClose",
            };
            ws.call_method(
                "addEventListener",
                std::vec![Js::str(event), h.root(member)?],
            )?;
            Ok(())
        },
        header_unavailable: |error| {
            error.value.get("name").is_ok_and(|name| {
                name.strict_equals(&Js::str("HeaderCapableWebSocketUnavailableError"))
            })
        },
        timer: |h, kind| {
            let value = h.root(timer_name(kind))?;
            Ok((!value.is_undefined()).then_some(value))
        },
        clear_timeout: |h, timer| {
            h.0.clear_timeout(timer.unwrap_or(&Js::Undefined));
            Ok(())
        },
        set_timeout: |h, kind, ms| {
            let weak = h.weak();
            Ok(h.0.set_timeout(
                Rc::new(move || {
                    let h = weak()?;
                    let task = h.clone();
                    h.spawn(Box::pin(async move {
                        feed::run_refresh_timer(&task, &runtime(), kind).await?;
                        Ok(Js::Undefined)
                    }));
                    Ok(())
                }),
                ms as f64,
            ))
        },
        assign_timer: |h, kind, timer| h.this().set(timer_name(kind), timer),
        fetch_refresh: |h, kind| {
            let method = match kind {
                WeexRefreshKind::Orders => "fetchOrders",
                WeexRefreshKind::Positions => "fetchPositions",
                WeexRefreshKind::Balance => "fetchBalance",
            };
            let pending = parent_call(h, method, std::vec![])?;
            Ok(Box::pin(await_js(&h.0.exchange_runtime(), pending)))
        },
        apply_refresh: |h, kind, value| match kind {
            WeexRefreshKind::Orders => call_unit(
                h,
                "store.update",
                std::vec![Js::object(std::vec![("orders", value)])],
            ),
            WeexRefreshKind::Balance => call_unit(
                h,
                "store.update",
                std::vec![Js::object(std::vec![("balance", value)])],
            ),
            WeexRefreshKind::Positions => {
                call_unit(h, "store.addOrUpdatePositionsFull", std::vec![value])
            }
        },
        base_dispose,
    }
}
pub(super) fn dispatch(mut h: Host, name: &'static str, args: Vec<Js>) -> Result<Js, Thrown> {
    let first = arg(&args, 0);
    let feed = runtime();
    match name {
        "connectAndSubscribe" => feed::connect_and_subscribe(&h, &feed)?,
        "onMessage" => feed::on_message(&h, &feed, &first.get("data")?)?,
        "handleOrders" => feed::handle_orders(&h, &feed, &first)?,
        "handlePositions" => feed::handle_positions(&h, &feed, &first)?,
        "handleFills" => feed::handle_fills(&h, &feed, &first)?,
        "throttledFetchOrders" => feed::throttled_fetch_orders(&h, &feed)?,
        "throttledFetchPositions" => feed::throttled_fetch_positions(&h, &feed)?,
        "throttledFetchBalance" => feed::throttled_fetch_balance(&h, &feed)?,
        "createAuthHeaders" => return auth(&h),
        "dispose" => feed::dispose(&h, &feed)?,
        "logUnsupportedTransport" => unsupported(
            &h,
            if first.is_undefined() {
                None
            } else {
                Some(v::units(&first)?)
            }
            .as_deref(),
        )?,
        _ => {
            let mut state = control_state(&h)?;
            let runtime = control_runtime();
            let result = match name {
                "ping" => control::private_ping(&mut h, &mut state, &runtime),
                "subscribe" => control::private_subscribe(&mut h, &mut state, &runtime),
                "onOpen" => control::private_on_open(&mut h, &mut state, &runtime),
                _ => {
                    return Err(Thrown::type_error(&std::format!(
                        "Unknown WEEX private socket member {name}"
                    )));
                }
            };
            save_control(&h, &state)?;
            result?;
        }
    }
    Ok(Js::Undefined)
}
