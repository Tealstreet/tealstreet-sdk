//! Private protocol sends and owned authentication/reconnect orchestration.
use safe_cex_exchange::{runtime::ExchangeError, websocket::BaseWebSocket};
use std::rc::Rc;

pub fn ping(socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
    if !socket.is_disposed() {
        socket.mark_ping_sent();
        socket.send_with_log(r#"{"op":"ping"}"#)?;
    }
    Ok(())
}

pub fn subscribe(socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
    if socket.emitter().name() == "bybitspot" {
        crate::private_ws_lifecycle::spot_subscribe(|frame| socket.send_with_log(frame))
    } else {
        socket.send_with_log(r#"{"op":"subscribe","args":["order","position"]}"#)
    }
}

pub fn connect_and_subscribe(
    socket: &Rc<BaseWebSocket>,
    runtime: &dyn safe_cex_exchange::websocket::WsRuntime,
    is_demo: bool,
    direct_override: Option<&str>,
    has_payload_signer: bool,
    relay_enabled: bool,
) -> Result<(), ExchangeError> {
    if has_payload_signer || socket.is_disposed() {
        return Ok(());
    }
    let parent = socket.emitter();
    let exchange = serde_json::from_value(serde_json::Value::String(parent.name().into()))
        .map_err(|error| ExchangeError::new(error.to_string()))?;
    let transport = runtime.transport();
    let route = transport
        .ws_routing(&exchange, &parent.options().into())
        .ws_private;
    let network = if parent.options().testnet == Some(true) {
        "testnet"
    } else {
        "livenet"
    };
    let urls = if is_demo {
        &*crate::types::DEMO_WS_URL
    } else {
        &*crate::types::BASE_WS_URL
    };
    let direct = direct_override
        .unwrap_or_else(|| urls["private"][network].as_str().expect("static Bybit URL"));
    let primary = route
        .host
        .as_deref()
        .filter(|host| route.tunnelled && !host.is_empty())
        .unwrap_or(direct);
    let relay = route.relay.as_deref().filter(|_| relay_enabled);
    let base = socket
        .direct_fallback
        .borrow_mut()
        .resolve_url(primary, &[relay, route.fallback.as_deref()])
        .to_owned();
    if Some(base.as_str()) == route.fallback.as_deref()
        || Some(base.as_str()) == route.relay.as_deref()
    {
        let kind = if Some(base.as_str()) == route.relay.as_deref() {
            "relay"
        } else {
            "proxy"
        };
        parent.emit(
            "grafana-info",
            &format!("Using tealstreet bybit private {kind}"),
        )?;
    }
    let url = if matches!(
        transport.ws_connection(&base),
        Some(safe_cex_transport::WsConnection::Relay { .. })
    ) {
        base
    } else {
        format!("{base}?requestId={}", runtime.unique_id())
    };
    let options = crate::ws_public::connection_options(&parent.options(), is_demo);
    socket.start_websocket(url, vec![], Some(options));
    Ok(())
}

pub(crate) mod local_auth;
pub(crate) mod protocol;

/// Begin the source's unawaited reconnect sequence. Clock invocation is eager;
/// only its asynchronous continuation belongs to the owned message operation.
/// `_sync_time` is intentionally ignored, as in the TypeScript implementation.
pub fn send_reconnect_sequence(
    socket: &Rc<BaseWebSocket>,
    runtime: Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime>,
    sync_clock: impl FnOnce() -> safe_cex_exchange::runtime::ExchangeFuture<'static, ()>,
    auth: impl FnOnce() -> safe_cex_exchange::runtime::ExchangeFuture<'static, bool> + 'static,
    reconnecting_enabled: bool,
    _sync_time: bool,
) {
    if socket.is_disposed() {
        return;
    }
    // JS invokes the async clock operation before returning to onMessage.
    let sync = sync_clock();
    let owner = socket.clone();
    socket.start_message_operation(async move {
        sync.await?;
        runtime.sleep(25).await?;
        let authed = match auth().await {
            Ok(authed) => authed,
            Err(error) => {
                owner.emit_and_log_error(&error.to_string(), None, None)?;
                false
            }
        };
        if !authed {
            owner.direct_fallback.borrow_mut().disregard_silence();
            owner.reconnect()?;
            return Ok(());
        }
        subscribe(&owner)?;
        ping(&owner)?;
        if reconnecting_enabled {
            // Source starts this snapshot without awaiting its completion.
            let snapshot = owner.emitter().fetch_and_set_private_snapshot(1);
            owner.start_message_operation(snapshot);
        }
        Ok(())
    });
}

pub(crate) mod messages;
