//! HyroPrivateWebsocket's retained source session over the concrete socket.
use super::{
    rt::{Rt, venue},
    value::{Js, ObjectData, Thrown, arg},
};
use crate::{hyro_private_session as session, ws_private::protocol::PrivateProtocol};
use safe_cex_exchange::{
    exchange::Exchange, js_runtime::scope::scope_of, runtime::ExchangeError,
    socket_lifecycle::SocketSlot, websocket::BaseWebSocket,
};
use safe_cex_transport::socket::SocketFrame;
use std::rc::{Rc, Weak};

fn weak_object(value: &Js) -> Weak<ObjectData> {
    let Js::Object(data) = value else {
        unreachable!("source session is an object")
    };
    Rc::downgrade(data)
}

/// The source subclass is selected by the same receiver as all private reads.
pub(crate) fn owner(protocol: &PrivateProtocol) -> Option<(Rt, Js)> {
    let parent = protocol.parent.upgrade()?;
    if parent.base().name != safe_cex_types::ExchangeName::Hyrotrader {
        return None;
    }
    let rt = parent.js.get()?.clone();
    let owner = rt.this.get("privateWebsocket").ok()?;
    Some((rt, owner))
}

pub(crate) fn callback(
    rt: &Rt,
    owner: Js,
    code: &'static str,
    captures: Vec<Js>,
) -> Result<Js, Thrown> {
    let weak = Rc::downgrade(&rt.shared);
    let owner = weak_object(&owner);
    Ok(Js::function(code, move |_, args| {
        let Some(owner) = owner.upgrade().map(Js::Object) else {
            return Ok(if code == "current" {
                Js::Bool(false)
            } else {
                Js::Undefined
            });
        };
        let rt = scope_of(&weak)?;
        if code == "current" {
            return session::is_current(&rt, &owner, &captures);
        }
        if code == "reconnect" {
            let body = rt.clone();
            return Ok(rt.spawn(Box::pin(async move {
                session::send_reconnect_sequence(&body, &owner).await?;
                Ok(Js::Undefined)
            })));
        }
        session::dispatch(
            &rt,
            &owner,
            session::Callback::from_name(code)
                .expect("SessionHost routes only registered session callbacks"),
            &captures,
            args,
        )
    }))
}

fn method(
    rt: &Rt,
    owner: &Js,
    name: &'static str,
    body: fn(&Rt, &Js, Vec<Js>) -> Result<Js, Thrown>,
) -> Result<(), Thrown> {
    let shared = Rc::downgrade(&rt.shared);
    let weak = weak_object(owner);
    owner.define(
        name,
        Js::function(name, move |_, args| {
            let Some(owner) = weak.upgrade().map(Js::Object) else {
                return Ok(Js::Undefined);
            };
            body(&scope_of(&shared)?, &owner, args)
        }),
    )
}

fn protocol(rt: &Rt) -> Result<Rc<PrivateProtocol>, Thrown> {
    rt.shared
        .extra
        .private_protocol
        .borrow()
        .upgrade()
        .ok_or_else(|| Thrown::error("Error", "HyroTrader private protocol disposed"))
}

fn typed(rt: &Rt) -> Result<Rc<BaseWebSocket>, Thrown> {
    venue(rt)?
        .base()
        .get_socket(SocketSlot::Private)
        .ok_or_else(|| Thrown::error("Error", "HyroTrader private socket disposed"))
}

pub(crate) fn install(rt: &Rt) -> Result<(), Thrown> {
    let socket = typed(rt)?;
    let base = rt.this.get("privateWebsocket")?;
    // Capture the original bodies directly. Calling the virtual core hooks here
    // would select the wrapper again.
    let weak_protocol = Rc::downgrade(&protocol(rt)?);
    let weak_socket = Rc::downgrade(&socket);
    base.define(
        "connectAndSubscribe",
        Js::function("connectAndSubscribe", move |_, _| {
            if let (Some(protocol), Some(socket)) = (weak_protocol.upgrade(), weak_socket.upgrade())
            {
                protocol.connect_base(socket)?;
            }
            Ok(Js::Undefined)
        }),
    )?;
    let weak = Rc::downgrade(&socket);
    base.define(
        "ping",
        Js::function("ping", move |_, _| {
            if let Some(socket) = weak.upgrade() {
                crate::ws_private::ping(&socket)?;
            }
            Ok(Js::Undefined)
        }),
    )?;
    let weak_protocol = Rc::downgrade(&protocol(rt)?);
    let weak_socket = Rc::downgrade(&socket);
    base.define(
        "onMessage",
        Js::function("onMessage", move |_, args| {
            if let (Some(protocol), Some(socket)) = (weak_protocol.upgrade(), weak_socket.upgrade())
            {
                let data = arg(&args, 0).get("data")?;
                let frame = if let Js::String(text) = data {
                    SocketFrame::Text(String::from_utf16_lossy(&text))
                } else {
                    SocketFrame::Binary(vec![])
                };
                protocol.message(&socket, &frame)?;
            }
            Ok(Js::Undefined)
        }),
    )?;
    let weak = Rc::downgrade(&socket);
    base.define(
        "onClose",
        Js::function("onClose", move |_, args| {
            if let Some(socket) = weak.upgrade() {
                let event = arg(&args, 0);
                let code = event.get("code")?;
                let code = if code.is_undefined() {
                    None
                } else {
                    Some(code.to_number()? as u16)
                };
                let reason = event.get("reason")?;
                let reason = if reason.is_undefined() {
                    String::new()
                } else {
                    super::host::text(&reason)?
                };
                socket.on_close(code, &reason)?;
            }
            Ok(Js::Undefined)
        }),
    )?;
    let weak_protocol = Rc::downgrade(&protocol(rt)?);
    base.define(
        "handleOrderTopic",
        Js::function("handleOrderTopic", move |_, args| {
            if let Some(protocol) = weak_protocol.upgrade() {
                if let Some(parent) = protocol.parent.upgrade() {
                    let rows = arg(&args, 0)
                        .array_values()?
                        .ok_or_else(|| Thrown::type_error("data.forEach is not a function"))?;
                    protocol.handle_order_topic(&parent, rows)?;
                }
            }
            Ok(Js::Undefined)
        }),
    )?;

    let owner = session::allocate_session(rt, base)?;
    let weak = Rc::downgrade(&rt.shared);
    owner.define_accessor(
        "parent",
        Some(Rc::new(move |_| Ok(scope_of(&weak)?.this))),
        None,
    )?;
    let weak = Rc::downgrade(&socket);
    let write = weak.clone();
    owner.define_accessor(
        "lastReceived",
        Some(Rc::new(move |_| {
            Ok(Js::Number(
                weak.upgrade().map_or(0.0, |socket| socket.last_received()),
            ))
        })),
        Some(Rc::new(move |_, value| {
            if let Some(socket) = write.upgrade() {
                socket.set_last_received(value.to_number()?);
            }
            Ok(())
        })),
    )?;
    let weak = Rc::downgrade(&socket);
    owner.define_accessor(
        "isDisposed",
        Some(Rc::new(move |_| {
            Ok(Js::Bool(
                weak.upgrade().is_none_or(|socket| socket.is_disposed()),
            ))
        })),
        None,
    )?;
    owner.define("ws", Js::Undefined)?;
    for (name, body) in [
        (
            "sessionVersion",
            session::session_version::<Rt> as fn(&Rt, &Js) -> Result<Js, Thrown>,
        ),
        ("isRecoveryComplete", session::is_recovery_complete::<Rt>),
    ] {
        let shared = Rc::downgrade(&rt.shared);
        let weak = weak_object(&owner);
        owner.define_accessor(
            name,
            Some(Rc::new(move |_| {
                let owner = weak
                    .upgrade()
                    .map(Js::Object)
                    .ok_or_else(|| Thrown::error("Error", "HyroTrader session disposed"))?;
                body(&scope_of(&shared)?, &owner)
            })),
            None,
        )?;
    }
    method(rt, &owner, "resetSession", |rt, owner, _| {
        session::reset_session(rt, owner)?;
        Ok(Js::Undefined)
    })?;
    method(rt, &owner, "request", |rt, owner, args| {
        session::request(rt, owner, arg(&args, 0), arg(&args, 1))
    })?;
    method(rt, &owner, "sendWithLog", |rt, _, args| {
        typed(rt)?.send_with_log(&super::host::text(&arg(&args, 0))?)?;
        Ok(Js::Undefined)
    })?;
    method(rt, &owner, "trackLastReceived", |rt, _, _| {
        typed(rt)?.track_last_received()?;
        Ok(Js::Undefined)
    })?;
    method(rt, &owner, "reconnect", |rt, _, _| {
        typed(rt)?.reconnect()?;
        Ok(Js::Undefined)
    })?;
    let weak = Rc::downgrade(&rt.shared);
    owner.define(
        "emitAndLogError",
        Js::function("emitAndLogError", move |_, args| {
            scope_of(&weak)?.this.call_method("emitAndLogError", args)
        }),
    )?;
    session::constructor(rt, &owner)?;
    rt.this.define("privateWebsocket", owner)?;
    let weak = rt.shared.extra.venue.borrow().clone();
    rt.this.define(
        "syncSocketHealthToStore",
        Js::function("syncSocketHealthToStore", move |_, _| {
            if let Some(venue) = weak.upgrade() {
                venue.base().sync_socket_health_to_store()?;
            }
            Ok(Js::Undefined)
        }),
    )?;
    Ok(())
}

pub(crate) fn event(owner: &Js, data: Js) -> Result<Js, ExchangeError> {
    Ok(Js::object(vec![
        (
            "currentTarget",
            owner.get("ws").map_err(|error| error.to_exchange_error())?,
        ),
        ("data", data),
    ]))
}
