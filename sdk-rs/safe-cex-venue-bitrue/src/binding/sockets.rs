//! Public/private sockets over the shared production BaseWebSocket.
use super::{
    Rt,
    host::{global_call, items},
};
use crate::{
    public_bodies::PublicBodyHost,
    public_streams::StreamHost,
    ws_sync::{
        self, ParseHost, PingHost, RefreshTimerHost, SubscriptionHost, SymbolHost, SyncFrame,
    },
};
use safe_cex_exchange::js_runtime::{
    ops,
    promise::Promise,
    sockets,
    value::{Js, Thrown, arg, from_typed},
};
use std::{cell::RefCell, rc::Rc};

pub(super) fn base(rt: &Rt, parent: Js) -> Result<Js, Thrown> {
    let r = sockets::base_websocket(&rt.shared.sockets, parent);
    let supers = Js::object(vec![("dispose", r.get("dispose")?)]);
    rt.shared
        .extra
        .supers
        .borrow_mut()
        .push((r.clone(), supers));
    Ok(r)
}
pub(super) fn private_member(rt: &Rt, r: Js, name: &'static str) -> Js {
    let weak = Rc::downgrade(&rt.shared);
    Js::function(name, move |_, a| {
        let h = safe_cex_exchange::js_runtime::scope::scope_of(&weak)?.with_this(r.clone());
        let x = |i| arg(&a, i);
        let asynchronous = matches!(
            name,
            "connectAndSubscribe" | "fetchListenKey" | "updateListenKey"
        );
        if asynchronous {
            let runtime = h.exchange_runtime();
            let r = r.clone();
            let a0 = x(0);
            let body = Box::pin(async move {
                match name {
                    "connectAndSubscribe" => crate::private_auth::connect(&h, r).await,
                    "fetchListenKey" => crate::private_auth::fetch_key(&h, r).await,
                    "updateListenKey" => crate::private_auth::update_key(&h, r, a0).await,
                    _ => unreachable!(),
                }
            });
            return Ok(Js::Promise(Promise::spawn(&runtime, body)));
        }
        match name {
            "onOpen" => {
                crate::ws_private::on_open(
                    || Ok(r.get("isDisposed")?.truthy()),
                    || r.call_method("subscribe", vec![]),
                    || r.call_method("ping", vec![]),
                )?;
                Ok(Js::Undefined)
            }
            "onMessage" => crate::private_auth::message(&h, r.clone(), x(0)),
            "parseOrderType" => ws_sync::parse_order_type(&h, x(0)),
            "parseOrderSide" => ws_sync::parse_order_side(&h, x(0)),
            "parseOrderStatus" => ws_sync::parse_order_status(&h, x(0), x(1)),
            "handleOrderTradeUpdate" => crate::private_auth::order_update(&h, r.clone(), x(0)),
            "handleAccountUpdate" => {
                crate::refresh_entrypoints::handle_account_update(x(0), || {
                    let p = r.get("parent")?;
                    let f = p.get("queueFastPrivateRefresh")?;
                    Ok(move || f.call(p, vec![]))
                })?;
                Ok(Js::Undefined)
            }
            "handlePingEvent" => {
                ws_sync::handle_private_ping(&h, x(0))?;
                Ok(Js::Undefined)
            }
            "subscribe" => {
                ws_sync::subscribe_private(&h)?;
                Ok(Js::Undefined)
            }
            "ping" => {
                crate::ws_private::ping();
                Ok(Js::Undefined)
            }
            "isSuccessfulCode" => Ok(Js::Bool(crate::ws_private::is_successful_code(
                match &x(0) {
                    Js::Number(n) => crate::ws_private::SuccessCode::Number(*n),
                    Js::String(s) => crate::ws_private::SuccessCode::String(s),
                    _ => crate::ws_private::SuccessCode::Other,
                },
            ))),
            "scheduleListenKeyRefresh" => {
                ws_sync::schedule_listen_key_refresh(Rc::new(h), x(0))?;
                Ok(Js::Undefined)
            }
            "dispose" => {
                ws_sync::dispose_private(&h)?;
                Ok(Js::Undefined)
            }
            _ => Err(Thrown::error(
                "Error",
                &format!("Unbound Bitrue private member {name}"),
            )),
        }
    })
}
pub(super) fn public_member(rt: &Rt, r: Js, name: &'static str) -> Js {
    let weak = Rc::downgrade(&rt.shared);
    Js::function(name, move |_, a| {
        let h = safe_cex_exchange::js_runtime::scope::scope_of(&weak)?.with_this(r.clone());
        let x = |i| arg(&a, i);
        match name {
            "onMessage" => {
                let runtime = h.exchange_runtime();
                let r = r.clone();
                let event = x(0);
                Ok(Js::Promise(Promise::spawn(
                    &runtime,
                    Box::pin(async move { crate::public_streams::message(&h, r, event).await }),
                )))
            }
            "connectAndSubscribe" => {
                crate::public_bodies::connect_and_subscribe(Rc::new(h), r.clone())
            }
            "onOpen" => crate::public_bodies::on_open(Rc::new(h), r.clone()),
            "onMarketsChanged" => {
                crate::public_bodies::on_markets_changed(Rc::new(h), r.clone(), x(0))
            }
            "ensureTickerSubscription" => {
                crate::public_bodies::ensure_ticker_subscription(Rc::new(h), r.clone(), x(0))
            }
            "subscribe" => crate::public_bodies::subscribe(Rc::new(h), r.clone()),
            "mapKline" => crate::public_bodies::map_kline(Rc::new(h), r.clone(), x(0)),
            "handleTickersEvent" => {
                crate::public_bodies::handle_tickers_event(Rc::new(h), r.clone(), x(0), x(1))
            }
            "makeRequest" => crate::public_bodies::make_request(Rc::new(h), r.clone(), x(0)),
            "listenOHLCV" => crate::public_streams::listen_candles(&h, r.clone(), x(0), x(1)),
            "listenOrderBook" => crate::public_streams::listen_book(&h, r.clone(), x(0), x(1)),
            "convertToWsSymbol" => Ok(Js::utf16(&crate::ws_symbol_helpers::convert_to_ws_symbol(
                &x(0).to_js_string()?,
                |s| {
                    Ok::<_, Thrown>(
                        Js::utf16(s)
                            .call_method("toLowerCase", vec![])?
                            .to_js_string()?
                            .to_vec(),
                    )
                },
            )?)),
            "formatKlineTopic" => Ok(Js::utf16(&crate::ws_symbol_helpers::format_kline_topic(
                &x(0).to_js_string()?,
                x(1),
                |s| {
                    Ok::<_, Thrown>(
                        r.call_method("convertToWsSymbol", vec![Js::utf16(s)])?
                            .to_js_string()?
                            .to_vec(),
                    )
                },
                |interval| {
                    Ok::<_, Thrown>(
                        h.shared
                            .globals
                            .get("INTERVAL")?
                            .get_key(&interval.to_js_string()?)?
                            .to_js_string()?
                            .to_vec(),
                    )
                },
            )?)),
            "resolveMarketSymbol" => ws_sync::resolve_market_symbol(&h, x(0)),
            "handlePingEvent" => {
                ws_sync::handle_public_ping(&h, x(0))?;
                Ok(Js::Undefined)
            }
            "ping" => {
                crate::ws_public::ping();
                Ok(Js::Undefined)
            }
            _ => Err(Thrown::error(
                "Error",
                &format!("Unbound Bitrue public member {name}"),
            )),
        }
    })
}
impl PublicBodyHost for Rt {
    type Value = Js;
    type Error = Thrown;
    fn undefined(&self) -> Js {
        Js::Undefined
    }
    fn text(&self, v: &str) -> Js {
        Js::str(v)
    }
    fn number(&self, v: f64) -> Js {
        Js::Number(v)
    }
    fn boolean(&self, v: bool) -> Js {
        Js::Bool(v)
    }
    fn truthy(&self, v: &Js) -> bool {
        v.truthy()
    }
    fn equal_zero(&self, v: &Js) -> bool {
        v.strict_equals(&Js::Number(0.))
    }
    fn global(&self, name: &'static str) -> Result<Js, Thrown> {
        if name == "WebSocket" {
            safe_cex_exchange::js_runtime::scope::bound_member(&self.this, "createWebsocket")
        } else {
            Rt::global(self, name)
        }
    }
    fn get(&self, r: &Js, key: Js) -> Result<Js, Thrown> {
        r.get_key(&key.to_js_string()?)
    }
    fn set(&self, r: &Js, key: Js, value: Js) -> Result<(), Thrown> {
        r.set_key(&key.to_js_string()?, value)
    }
    fn call(&self, f: Js, r: Js, a: Vec<Js>) -> Result<Js, Thrown> {
        f.call(r, a)
    }
    fn construct(&self, f: Js, a: Vec<Js>) -> Result<Js, Thrown> {
        f.call(Js::Undefined, a)
    }
    fn function(&self, f: crate::public_bodies::NativeCallback<Js, Thrown>) -> Js {
        Js::function("", move |_, a| f(a))
    }
    fn object(&self, v: Vec<(&'static str, Js)>) -> Js {
        Js::object(v)
    }
    fn array(&self, v: Vec<Js>) -> Js {
        Js::array(v)
    }
    fn template(&self, v: Js) -> Result<Js, Thrown> {
        Ok(Js::String(v.to_js_string()?))
    }
    fn add(&self, a: Js, b: Js) -> Result<Js, Thrown> {
        ops::add(&a, &b)
    }
    fn divide(&self, a: Js, b: Js) -> Result<Js, Thrown> {
        ops::divide(&a, &b)
    }
    fn multiply(&self, a: Js, b: Js) -> Result<Js, Thrown> {
        ops::multiply(&a, &b)
    }
    fn unary_plus(&self, v: Js) -> Result<Js, Thrown> {
        Ok(Js::Number(v.to_number()?))
    }
    fn iterate(&self, v: Js, body: &mut dyn FnMut(Js) -> Result<(), Thrown>) -> Result<(), Thrown> {
        for x in items(&v)? {
            body(x)?
        }
        Ok(())
    }
}
impl StreamHost for Rt {
    fn global(&self, n: &'static str) -> Result<Js, Thrown> {
        Rt::global(self, n)
    }
    fn dynamic(&self, v: Js, k: Js) -> Result<Js, Thrown> {
        v.get_key(&k.to_js_string()?)
    }
    fn put(&self, v: Js, k: Js, n: Js) -> Result<(), Thrown> {
        v.set_key(&k.to_js_string()?, n)
    }
    fn delete(&self, v: Js, k: Js) -> Result<(), Thrown> {
        v.delete(&k.to_js_string()?);
        Ok(())
    }
    fn spread(&self, v: Js) -> Result<Vec<Js>, Thrown> {
        match v {
            Js::Set(s) => Ok(s.borrow().clone()),
            _ => items(&v),
        }
    }
    fn new_set(&self) -> Result<Js, Thrown> {
        Ok(Js::new_set())
    }
    fn is_blob(&self, v: &Js) -> Result<bool, Thrown> {
        Ok(matches!(v,Js::Object(o) if o.class=="Blob"))
    }
    fn pair(&self, v: Js) -> Result<(Js, Js), Thrown> {
        let iterator = v.iterator()?;
        let next = iterator.get("next")?;
        let mut done = false;
        let mut step = || {
            if done {
                return Ok(Js::Undefined);
            }
            let result = next.call(iterator.clone(), vec![])?;
            if !result.is_object() {
                return Err(Thrown::type_error("Iterator result is not an object"));
            }
            done = result.get("done")?.truthy();
            if done {
                Ok(Js::Undefined)
            } else {
                result.get("value")
            }
        };
        let pair = (step()?, step()?);
        if !done {
            let close = iterator.get("return")?;
            if !close.is_nullish() && !close.call(iterator, vec![])?.is_object() {
                return Err(Thrown::type_error(
                    "Iterator return result is not an object",
                ));
            }
        }
        Ok(pair)
    }
    fn null(&self) -> Js {
        Js::Null
    }
}
impl ParseHost for Rt {
    type Value = Js;
    type Error = Thrown;
    fn string(&self, v: &Js) -> Option<Vec<u16>> {
        if let Js::String(s) = v {
            Some(s.to_vec())
        } else {
            None
        }
    }
    fn number(&self, v: &Js) -> Option<f64> {
        if let Js::Number(n) = v {
            Some(*n)
        } else {
            None
        }
    }
    fn truthy(&self, v: &Js) -> bool {
        v.truthy()
    }
    fn empty_string(&self) -> Js {
        Js::str("")
    }
    fn template(&self, v: Js) -> Result<Vec<u16>, Thrown> {
        Ok(v.to_js_string()?.to_vec())
    }
    fn uppercase(&self, v: Vec<u16>) -> Result<Vec<u16>, Thrown> {
        Ok(Js::utf16(&v)
            .call_method("toUpperCase", vec![])?
            .to_js_string()?
            .to_vec())
    }
    fn lookup(&self, t: &'static str, k: Vec<u16>) -> Result<Js, Thrown> {
        Rt::global(self, t)?.get_key(&k)
    }
    fn order_type(&self, v: safe_cex_types::OrderType) -> Js {
        from_typed(&v).expect("enum serialization")
    }
    fn order_side(&self, v: safe_cex_types::OrderSide) -> Js {
        from_typed(&v).expect("enum serialization")
    }
    fn order_status(&self, v: safe_cex_types::OrderStatus) -> Js {
        from_typed(&v).expect("enum serialization")
    }
}
impl SymbolHost for Rt {
    fn direct_market(&self, s: &Js) -> Result<Js, Thrown> {
        self.this
            .get("parent")?
            .get("store")?
            .get("marketsBySymbol")?
            .get_key(&s.to_js_string()?)
    }
    fn replace_prefix(&self, s: Js) -> Result<Js, Thrown> {
        let u = s.to_js_string()?;
        Ok(Js::utf16(
            if u.len() >= 2 && matches!(u[0], 69 | 101) && matches!(u[1], 45 | 95) {
                &u[2..]
            } else {
                &u
            },
        ))
    }
    fn replace_separators(&self, s: Js) -> Result<Js, Thrown> {
        Ok(Js::utf16(
            &s.to_js_string()?
                .iter()
                .copied()
                .filter(|c| !matches!(c, 45 | 95 | 47))
                .collect::<Vec<_>>(),
        ))
    }
    fn uppercase_value(&self, s: Js) -> Result<Js, Thrown> {
        s.call_method("toUpperCase", vec![])
    }
    fn find_market<F: FnMut(Js) -> Result<bool, Thrown>>(&self, f: F) -> Result<Js, Thrown> {
        super::host::find(self.this.get("parent")?.get("store")?.get("markets")?, f)
    }
    fn market_symbol(&self, m: Js) -> Result<Js, Thrown> {
        m.get("symbol")
    }
    fn optional_market_symbol(&self, m: Js) -> Result<Js, Thrown> {
        if m.is_nullish() {
            Ok(Js::Undefined)
        } else {
            m.get("symbol")
        }
    }
    fn strictly_equal(&self, a: &Js, b: &Js) -> bool {
        a.strict_equals(b)
    }
}
impl PingHost for Rt {
    type Value = Js;
    type Error = Thrown;
    type Sender = Js;
    type Stringifier = Js;
    type Return = Js;
    fn now(&self) -> Result<f64, Thrown> {
        Ok(self.shared.runtime.now_ms())
    }
    fn set_ping_at(&self, v: f64) -> Result<(), Thrown> {
        self.this.set("pingAt", Js::Number(v))
    }
    fn ping(&self, v: Js) -> Result<Js, Thrown> {
        v.get("ping")
    }
    fn undefined(&self, v: &Js) -> bool {
        v.is_undefined()
    }
    fn prepare_send(&self) -> Result<Js, Thrown> {
        self.this.get("send")
    }
    fn prepare_stringify(&self) -> Result<Js, Thrown> {
        Rt::global(self, "JSON.stringify")
    }
    fn stringify(&self, f: Js, frame: SyncFrame<Js>) -> Result<Js, Thrown> {
        let value = match frame {
            SyncFrame::Pong(p) => Js::object(vec![("pong", p)]),
            SyncFrame::EventPong => Js::object(vec![("event", Js::str("pong"))]),
            SyncFrame::PrivateSubscription(c) => Js::object(vec![
                ("event", Js::str("sub")),
                ("params", Js::object(vec![("channel", c)])),
            ]),
        };
        f.call(Js::Undefined, vec![value])
    }
    fn pong_text(&self) -> Js {
        Js::str("pong")
    }
    fn send(&self, f: Js, v: Js) -> Result<Js, Thrown> {
        f.call(self.this.clone(), vec![v])
    }
    fn handle_pong(&self, d: f64) -> Result<Js, Thrown> {
        self.this
            .call_method("handlePongEvent", vec![Js::Number(d)])
    }
}
impl SubscriptionHost for Rt {
    fn channels_for_each<F: FnMut(Js) -> Result<(), Thrown>>(
        &self,
        channels: &[&str],
        mut f: F,
    ) -> Result<Js, Thrown> {
        for c in channels {
            f(Js::str(c))?;
        }
        Ok(Js::Undefined)
    }
    fn prepare_send_with_log(&self) -> Result<Js, Thrown> {
        self.this.get("sendWithLog")
    }
    fn send_with_log(&self, f: Js, v: Js) -> Result<Js, Thrown> {
        f.call(self.this.clone(), vec![v])
    }
}
impl RefreshTimerHost for Rt {
    type Clearer = Js;
    type Value = Js;
    type Error = Thrown;
    type Return = Js;
    fn timeout(&self) -> Result<Js, Thrown> {
        self.this.get("listenKeyRefreshTimeoutId")
    }
    fn truthy_timeout(&self, v: &Js) -> bool {
        v.truthy()
    }
    fn prepare_clear_timeout(&self) -> Result<Js, Thrown> {
        Rt::global(self, "clearTimeout")
    }
    fn clear_timeout(&self, f: Js, v: Js) -> Result<(), Thrown> {
        f.call(Js::Undefined, vec![v])?;
        Ok(())
    }
    fn set_timeout<F: FnMut() -> Result<Js, Thrown> + 'static>(
        &self,
        f: F,
        delay: f64,
    ) -> Result<Js, Thrown> {
        let f = RefCell::new(f);
        global_call(
            self,
            "setTimeout",
            vec![
                Js::function("", move |_, _| (f.borrow_mut())()),
                Js::Number(delay),
            ],
        )
    }
    fn assign_timeout(&self, v: Js) -> Result<(), Thrown> {
        self.this.set("listenKeyRefreshTimeoutId", v)
    }
    fn update_listen_key(&self, key: Js) -> Result<Js, Thrown> {
        self.this.call_method("updateListenKey", vec![key])
    }
    fn undefined_timeout(&self) -> Js {
        Js::Undefined
    }
    fn base_dispose(&self) -> Result<Js, Thrown> {
        super::classes::super_member(self, &self.this, "dispose")?.call(self.this.clone(), vec![])
    }
}
