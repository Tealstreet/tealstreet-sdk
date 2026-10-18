//! Source socket bodies over BaseWebSocket's shared facade and concrete wire host.
use super::{Rt, error};
use crate::{account_reads::AccountHost, ws_dispatch as dispatch, ws_lifecycle as lifecycle};
use safe_cex_exchange::js_runtime::{
    scope::Scope,
    value::{Js, Thrown, arg},
};
use std::{cell::RefCell, rc::Rc};

fn unit(r: Result<Js, Thrown>) -> Result<(), Thrown> {
    r.map(|_| ())
}
fn call_member(rt: &Rt, n: &str, a: Vec<Js>) -> Result<(), Thrown> {
    unit(rt.this.call_method(n, a))
}
fn base(rt: &Rt, n: &str, a: Vec<Js>) -> Result<(), Thrown> {
    unit(
        rt.global("BaseWebSocket.prototype")?
            .get(n)?
            .call(rt.this.clone(), a),
    )
}
fn clear_ping(rt: &Rt) -> Result<(), Thrown> {
    let h = rt.this.get("pingInterval")?;
    if h.truthy() {
        rt.clear_timeout(&h);
        rt.this.set("pingInterval", Js::Undefined)?;
    }
    Ok(())
}
pub(crate) fn call(rt: Rc<Rt>, class: &str, n: &str, a: Vec<Js>) -> Result<Js, Thrown> {
    let private = class == "PolymarketPrivateWebSocket";
    match n {
        "mapOrder" => {
            return Ok(super::host::order(
                &crate::order_adapters::map_private_order(rt.as_ref(), &arg(&a, 0))?,
            ));
        }
        "connectAndSubscribe" => {
            if private {
                dispatch::connect_private(rt)?
            } else {
                dispatch::connect_public(rt)?
            }
        }
        "onOpen" => {
            if private {
                lifecycle::private_on_open(rt.as_ref())?
            } else {
                lifecycle::public_on_open(rt.as_ref())?
            }
        }
        "onClose" => {
            if private {
                lifecycle::private_on_close(rt)?
            } else {
                lifecycle::public_on_close(rt)?
            }
        }
        "ping" => {
            let slot = RefCell::new(None);
            if private {
                lifecycle::ping_private(rt.clone(), &slot)?
            } else {
                lifecycle::ping_public(rt.clone(), &slot)?
            };
            if let Some(h) = slot.into_inner() {
                rt.this.set("pingInterval", h.as_ref().clone())?;
            }
        }
        "clearPingInterval" => clear_ping(&rt)?,
        "authenticate" => lifecycle::authenticate_private(rt.as_ref(), || {
            let c = rt.this.call_method("getCredentials", vec![])?;
            if !c.truthy() {
                return Ok(None);
            }
            Ok(Some(lifecycle::WsCredentials {
                api_key: c.get("apiKey")?.to_js_string()?.to_vec(),
                secret: c.get("secret")?.to_js_string()?.to_vec(),
                passphrase: c.get("passphrase")?.to_js_string()?.to_vec(),
            }))
        })?,
        "sendSubscription" => lifecycle::send_public_subscription(rt.as_ref(), || {
            let mut i = super::Iter::new(rt.this.get("subscribedAssets")?)?;
            let mut out = vec![];
            while let Some(v) = i.next()? {
                out.push(v.to_js_string()?.to_vec());
            }
            Ok(out)
        })?,
        "onMessage" => {
            if private {
                dispatch::message_private(rt.as_ref(), &arg(&a, 0))?
            } else {
                dispatch::message_public(rt.as_ref(), &arg(&a, 0))?
            }
        }
        "subscribeToAsset" => dispatch::subscribe_asset(rt.as_ref(), &arg(&a, 0).to_js_string()?)?,
        "unsubscribeFromAsset" => {
            dispatch::unsubscribe_asset(rt.as_ref(), &arg(&a, 0).to_js_string()?)?
        }
        "subscribeToOrderBook" => {
            let cleanup = dispatch::subscribe_book(rt, &arg(&a, 0).to_js_string()?, arg(&a, 1))?;
            return Ok(Js::function("unsubscribe", move |_, _| {
                cleanup()?;
                Ok(Js::Undefined)
            }));
        }
        "handleLastTradePrice" => dispatch::last_trade(rt.as_ref(), &arg(&a, 0))?,
        "handleOrderUpdate" => {
            crate::order_adapters::handle_order_update(rt.as_ref(), &arg(&a, 0))?
        }
        "handleTradeUpdate" => {
            crate::state_sync::handle_trade_update(rt.as_ref(), arg(&a, 0))?;
        }
        "refreshPositions" => crate::state_sync::refresh_positions(rt, arg(&a, 0))?,
        "dispose" => {
            clear_ping(&rt)?;
            if !private {
                rt.this
                    .get("subscribedAssets")?
                    .call_method("clear", vec![])?;
                rt.this
                    .get("orderbookCallbacks")?
                    .call_method("clear", vec![])?;
            }
            base(&rt, "dispose", vec![])?;
        }
        _ => return Err(error(&format!("Unknown Polymarket socket member {n}"))),
    }
    Ok(Js::Undefined)
}
impl lifecycle::SocketHost for Rt {
    type Timer = Js;
    type Error = Thrown;
    fn is_disposed(&self) -> bool {
        self.this.get("isDisposed").is_ok_and(|v| v.truthy())
    }
    fn is_connected(&self) -> bool {
        self.this.get("isConnected").is_ok_and(|v| v.truthy())
    }
    fn log(&self, m: &str, s: &str) -> Result<(), Thrown> {
        unit(
            self.this
                .get("parent")?
                .call_method("log", vec![Js::str(m), Js::str(s)]),
        )
    }
    fn send(&self, m: &str, _: Option<&lifecycle::SubscriptionFrame>) -> Result<(), Thrown> {
        call_member(self, "send", vec![Js::str(m)])
    }
    fn set_timeout(&self, d: f64, f: Rc<dyn Fn() -> Result<(), Thrown>>) -> Result<Rc<Js>, Thrown> {
        Ok(Rc::new(Scope::set_timeout(self, f, d)))
    }
    fn record_close_diagnostics(&self, _: Option<()>, s: bool) -> Result<(), Thrown> {
        call_member(
            self,
            "recordCloseDiagnostics",
            vec![
                Js::Undefined,
                Js::object(vec![("syntheticIfNoSocket", Js::Bool(s))]),
            ],
        )
    }
    fn clear_ping_interval(&self) -> Result<(), Thrown> {
        call_member(self, "clearPingInterval", vec![])
    }
    fn clear_socket_field(&self) -> Result<(), Thrown> {
        self.this.set("ws", Js::Undefined)
    }
    fn ping(&self) -> Result<(), Thrown> {
        call_member(self, "ping", vec![])
    }
    fn connect_and_subscribe(&self) -> Result<(), Thrown> {
        call_member(self, "connectAndSubscribe", vec![])
    }
    fn send_subscription(&self) -> Result<(), Thrown> {
        call_member(self, "sendSubscription", vec![])
    }
    fn authenticate(&self) -> Result<(), Thrown> {
        call_member(self, "authenticate", vec![])
    }
    fn emit_reconnected(&self) -> Result<(), Thrown> {
        call_member(self, "emitWsReconnected", vec![])
    }
}
impl dispatch::ConnectHost for Rt {
    type Socket = Js;
    type Listener = Js;
    fn credentials(&self) -> Result<Js, Thrown> {
        self.this.call_method("getCredentials", vec![])
    }
    fn disposed(&self) -> bool {
        lifecycle::SocketHost::is_disposed(self)
    }
    fn socket(&self) -> Result<Option<Js>, Thrown> {
        let s = self.this.get("ws")?;
        Ok(if s.truthy() { Some(s) } else { None })
    }
    fn url(&self) -> Result<Vec<u16>, Thrown> {
        Ok(self.this.get("connectUrl")?.to_js_string()?.to_vec())
    }
    fn create_socket(&self, u: &[u16]) -> Result<Js, Thrown> {
        self.this.call_method("createWebsocket", vec![Js::utf16(u)])
    }
    fn assign_socket(&self, s: Js) -> Result<(), Thrown> {
        self.this.set("ws", s)
    }
    fn listener(&self, e: &'static str) -> Result<Js, Thrown> {
        self.this.get(match e {
            "open" => "onOpen",
            "message" => "onMessage",
            "close" => "onClose",
            _ => return Err(error("Unknown socket event")),
        })
    }
    fn add_listener(&self, s: Js, e: &'static str, l: Js) -> Result<(), Thrown> {
        unit(s.call_method("addEventListener", vec![Js::str(e), l]))
    }
    fn add_error_listener(
        &self,
        s: Js,
        f: Rc<dyn Fn(Js) -> Result<(), Thrown>>,
    ) -> Result<(), Thrown> {
        unit(s.call_method(
            "addEventListener",
            vec![
                Js::str("error"),
                Js::function("error", move |_, a| {
                    f(arg(&a, 0))?;
                    Ok(Js::Undefined)
                }),
            ],
        ))
    }
    fn missing_socket_error(&self) -> Thrown {
        Thrown::type_error("Cannot read properties of undefined (reading 'addEventListener')")
    }
}
impl dispatch::MessageHost for Rt {
    type Event = Js;
    type Handlers = Js;
    type Handler = Js;
    fn track_received(&self) -> Result<(), Thrown> {
        call_member(self, "trackLastReceived", vec![])
    }
    fn event_data(&self, e: &Js) -> Result<Js, Thrown> {
        e.get("data")
    }
    fn json_parse(&self, v: &Js) -> Result<Js, Thrown> {
        self.global("jsonParse")?
            .call(Js::Undefined, vec![v.clone()])
    }
    fn pong(&self) -> Result<(), Thrown> {
        call_member(self, "handlePongEvent", vec![])
    }
    fn handlers(&self, t: &Js) -> Result<Option<Js>, Thrown> {
        let h = self
            .this
            .get("messageHandlers")?
            .get_key(&t.to_js_string()?)?;
        Ok(if h.truthy() { Some(h) } else { None })
    }
    fn stats(&self, t: &Js) -> Result<(), Thrown> {
        call_member(self, "recordMessageHandlerStats", vec![t.clone()])
    }
    fn handler_length(&self, h: &Js) -> Result<usize, Thrown> {
        AccountHost::length(self, h)
    }
    fn handler_at(&self, h: &Js, i: usize) -> Result<Option<Js>, Thrown> {
        AccountHost::at(self, h, i)
    }
    fn invoke(&self, h: Js, v: &Js) -> Result<(), Thrown> {
        unit(h.call(Js::Undefined, vec![v.clone()]))
    }
    fn refresh_positions(&self, s: &'static str) -> Result<(), Thrown> {
        crate::state_sync::refresh_positions(Rc::new(self.clone()), Js::str(s))
    }
}
impl dispatch::SubscriptionHost for Rt {
    type Callback = Js;
    type Callbacks = Js;
    fn has_asset(&self, t: &[u16]) -> Result<bool, Thrown> {
        Ok(self
            .this
            .get("subscribedAssets")?
            .call_method("has", vec![Js::utf16(t)])?
            .truthy())
    }
    fn insert_asset(&self, t: &[u16]) -> Result<(), Thrown> {
        unit(
            self.this
                .get("subscribedAssets")?
                .call_method("add", vec![Js::utf16(t)]),
        )
    }
    fn delete_asset(&self, t: &[u16]) -> Result<(), Thrown> {
        unit(
            self.this
                .get("subscribedAssets")?
                .call_method("delete", vec![Js::utf16(t)]),
        )
    }
    fn connected(&self) -> bool {
        lifecycle::SocketHost::is_connected(self)
    }
    fn send_operation(&self, o: dispatch::AssetOperation) -> Result<(), Thrown> {
        let frame = Js::object(vec![
            (
                "assets_ids",
                Js::array(o.assets_ids.iter().map(|a| Js::utf16(a)).collect()),
            ),
            ("operation", Js::str(o.operation)),
        ]);
        let text = self
            .global("JSON.stringify")?
            .call(Js::Undefined, vec![frame])?;
        call_member(self, "send", vec![text])
    }
    fn has_callbacks(&self, t: &[u16]) -> Result<bool, Thrown> {
        Ok(self
            .this
            .get("orderbookCallbacks")?
            .call_method("has", vec![Js::utf16(t)])?
            .truthy())
    }
    fn insert_callback_set(&self, t: &[u16]) -> Result<(), Thrown> {
        unit(
            self.this
                .get("orderbookCallbacks")?
                .call_method("set", vec![Js::utf16(t), Js::new_set()]),
        )
    }
    fn callbacks(&self, t: &[u16]) -> Result<Option<Js>, Thrown> {
        let s = self
            .this
            .get("orderbookCallbacks")?
            .call_method("get", vec![Js::utf16(t)])?;
        Ok(if s.is_undefined() { None } else { Some(s) })
    }
    fn add_callback(&self, s: &Js, c: Js) -> Result<(), Thrown> {
        unit(s.call_method("add", vec![c]))
    }
    fn delete_callback(&self, s: &Js, c: &Js) -> Result<(), Thrown> {
        unit(s.call_method("delete", vec![c.clone()]))
    }
    fn callback_size(&self, s: &Js) -> usize {
        s.get("size").and_then(|v| v.to_number()).unwrap_or(0.) as usize
    }
    fn delete_callback_set(&self, t: &[u16]) -> Result<(), Thrown> {
        unit(
            self.this
                .get("orderbookCallbacks")?
                .call_method("delete", vec![Js::utf16(t)]),
        )
    }
    fn subscribe_asset(&self, t: &[u16]) -> Result<(), Thrown> {
        call_member(self, "subscribeToAsset", vec![Js::utf16(t)])
    }
    fn unsubscribe_asset(&self, t: &[u16]) -> Result<(), Thrown> {
        call_member(self, "unsubscribeFromAsset", vec![Js::utf16(t)])
    }
    fn missing_callbacks_error(&self) -> Thrown {
        Thrown::type_error("Missing orderbook callbacks")
    }
}
impl dispatch::TradeHost for Rt {
    fn token_symbol(&self, t: &Js) -> Result<Js, Thrown> {
        let map = self.this.get("parent")?.get("tokenToSymbol")?;
        if map.is_nullish() {
            Ok(Js::Undefined)
        } else {
            map.call_method("get", vec![t.clone()])
        }
    }
    fn update_ticker(&self, id: &Js, p: dispatch::LastTradePatch<Js>) -> Result<(), Thrown> {
        unit(self.this.get("parent")?.get("store")?.call_method(
            "updateTicker",
            vec![
                Js::object(vec![("id", id.clone())]),
                Js::object(vec![
                    ("id", p.id),
                    ("symbol", p.symbol),
                    ("last", Js::Number(p.last)),
                ]),
            ],
        ))
    }
}
