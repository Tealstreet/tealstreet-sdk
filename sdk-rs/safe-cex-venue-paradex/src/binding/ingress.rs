//! Concrete JS collaborators for the source websocket ingress bodies.
use super::{Rt, host::Host, primitives};
use crate::public_ingress::{FeedCandle, FeedTicker, PublicIngressHost};
use safe_cex_exchange::js_runtime::{
    ops,
    value::{Js, Thrown},
};

pub(super) struct SocketHost<'a> {
    pub rt: &'a Rt,
    pub event: Js,
}
impl SocketHost<'_> {
    fn state(&self, key: &str) -> Result<Js, Thrown> {
        self.rt.this.get(key)
    }
    fn parent(&self) -> Result<Js, Thrown> {
        self.state("parent")
    }
    fn map_get(&self, name: &str, key: Js) -> Result<Js, Thrown> {
        self.state(name)?.call_method("get", vec![key])
    }
    fn map_set(&self, name: &str, key: Js, value: Js) -> Result<(), Thrown> {
        self.state(name)?.call_method("set", vec![key, value])?;
        Ok(())
    }
    fn logger(&self) -> Result<Js, Thrown> {
        Host::new(self.rt).logger()
    }
    fn log(&self, logger: Js, message: Vec<u16>, severity: &str) -> Result<(), Thrown> {
        Host::new(self.rt).log(logger, message, severity)
    }
    fn parse(&self, event: &Js) -> Result<Js, Thrown> {
        let data = event.get("data")?;
        Ok(self
            .rt
            .global("JSON.parse")?
            .call(Js::Undefined, vec![data])
            .unwrap_or(Js::Null))
    }
    fn each(
        &self,
        values: &Js,
        callback: &mut dyn FnMut(Js) -> Result<(), Thrown>,
    ) -> Result<(), Thrown> {
        for value in primitives::iterate(values)? {
            callback(value)?;
        }
        Ok(())
    }
    fn caught(&self, prefix: &str, error: &Thrown) -> Result<(), Thrown> {
        let logger = self.logger()?;
        let mut message = prefix.encode_utf16().collect::<Vec<_>>();
        message.extend(error.value.to_js_string()?.iter().copied());
        self.log(logger, message, "error")
    }
}
impl PublicIngressHost for SocketHost<'_> {
    type Value = Js;
    type Error = Thrown;
    type Log = Js;
    fn get(&self, v: &Js, k: &str) -> Result<Js, Thrown> {
        v.get(k)
    }
    fn optional_get(&self, v: &Js, k: &str) -> Result<Js, Thrown> {
        if v.is_nullish() {
            Ok(Js::Undefined)
        } else {
            v.get(k)
        }
    }
    fn truthy(&self, v: &Js) -> bool {
        v.truthy()
    }
    fn is_object(&self, v: &Js) -> bool {
        v.type_of() == "object"
    }
    fn is_array(&self, v: &Js) -> bool {
        v.is_array()
    }
    fn is_undefined(&self, v: &Js) -> bool {
        v.is_undefined()
    }
    fn is_null(&self, v: &Js) -> bool {
        matches!(v, Js::Null)
    }
    fn text_value(&self, v: Vec<u16>) -> Js {
        Js::utf16(&v)
    }
    fn literal(&self, v: &str) -> Js {
        Js::str(v)
    }
    fn number(&self, v: f64) -> Js {
        Js::Number(v)
    }
    fn equal(&self, a: &Js, b: &Js) -> bool {
        a.strict_equals(b)
    }
    fn string(&self, v: &Js) -> Result<Vec<u16>, Thrown> {
        self.rt
            .global("String")?
            .call(Js::Undefined, vec![v.clone()])?
            .to_js_string()
            .map(|v| v.to_vec())
    }
    fn template(&self, v: &Js) -> Result<Vec<u16>, Thrown> {
        v.to_js_string().map(|v| v.to_vec())
    }
    fn parse_float(&self, v: &Js) -> Result<f64, Thrown> {
        self.rt
            .global("parseFloat")?
            .call(Js::Undefined, vec![v.clone()])?
            .to_number()
    }
    fn to_number(&self, v: &Js) -> Result<f64, Thrown> {
        self.rt
            .global("Number")?
            .call(Js::Undefined, vec![v.clone()])?
            .to_number()
    }
    fn coerce_number(&self, v: &Js) -> Result<f64, Thrown> {
        v.to_number()
    }
    fn add(&self, v: &Js, n: f64) -> Result<Js, Thrown> {
        ops::add(v, &Js::Number(n))
    }
    fn stringify(&self, v: &Js) -> Result<Vec<u16>, Thrown> {
        self.rt
            .global("JSON.stringify")?
            .call(Js::Undefined, vec![v.clone()])?
            .to_js_string()
            .map(|v| v.to_vec())
    }
    fn split_optional(&self, v: &Js, s: char) -> Result<Js, Thrown> {
        if v.is_nullish() {
            Ok(Js::Undefined)
        } else {
            v.call_method("split", vec![Js::str(&s.to_string())])
        }
    }
    fn object_keys_empty(&self, v: &Js) -> Result<bool, Thrown> {
        Ok(self
            .rt
            .global("Object.keys")?
            .call(Js::Undefined, vec![v.clone()])?
            .get("length")?
            .to_number()?
            == 0.)
    }
    fn array(&self, v: Vec<Js>) -> Js {
        Js::array(v)
    }
    fn empty_object(&self) -> Js {
        Js::object(vec![])
    }
    fn each(&self, v: &Js, c: &mut dyn FnMut(Js) -> Result<(), Thrown>) -> Result<(), Thrown> {
        SocketHost::each(self, v, c)
    }
    fn iterate(&self, v: &Js, c: &mut dyn FnMut(Js) -> Result<(), Thrown>) -> Result<(), Thrown> {
        SocketHost::each(self, v, c)
    }
    fn select_log(&self) -> Result<Js, Thrown> {
        self.logger()
    }
    fn log(&self, l: Js, t: Vec<u16>, s: &'static str) -> Result<(), Thrown> {
        SocketHost::log(self, l, t, s)
    }
    fn caught(&self, p: &str, e: Thrown) -> Result<(), Thrown> {
        SocketHost::caught(self, p, &e)
    }
    fn update_ticker(&self, t: FeedTicker) -> Result<(), Thrown> {
        self.parent()?.get("store")?.call_method(
            "updateTicker",
            vec![
                Js::object(vec![("id", Js::utf16(&t.symbol))]),
                Js::object(
                    t.fields
                        .into_iter()
                        .map(|(k, v)| (k, Js::Number(v)))
                        .collect(),
                ),
            ],
        )?;
        Ok(())
    }
    fn now(&self) -> Result<f64, Thrown> {
        self.rt
            .global("Date.now")?
            .call(Js::Undefined, vec![])?
            .to_number()
    }
    fn update_candle_from_trade(&self, s: Vec<u16>, p: f64, z: f64, t: f64) -> Result<(), Thrown> {
        crate::public_ingress::update_ohlcv_from_trade(self, &s, p, z, t)
    }
    fn disposed(&self) -> Result<bool, Thrown> {
        Ok(self.state("isDisposed")?.truthy())
    }
    fn track(&self) -> Result<(), Thrown> {
        self.rt.this.call_method("trackLastReceived", vec![])?;
        Ok(())
    }
    fn parse_event(&self, e: &Js) -> Result<Js, Thrown> {
        self.parse(e)
    }
    fn handlers(&self, t: &Js) -> Result<Js, Thrown> {
        self.state("messageHandlers")?.get_key(&t.to_js_string()?)
    }
    fn stats(&self, t: Js) -> Result<(), Thrown> {
        self.rt
            .this
            .call_method("recordMessageHandlerStats", vec![t])?;
        Ok(())
    }
    fn call_handler(&self, f: Js, d: Js) -> Result<(), Thrown> {
        f.call(Js::Undefined, vec![d])?;
        Ok(())
    }
    fn pending(&self, id: Js) -> Result<Js, Thrown> {
        self.map_get("pendingSubscribeRequests", id)
    }
    fn delete_pending(&self, id: Js) -> Result<(), Thrown> {
        self.state("pendingSubscribeRequests")?
            .call_method("delete", vec![id])?;
        Ok(())
    }
    fn retry(&self, c: Js, a: Js, r: Js) -> Result<(), Thrown> {
        self.rt
            .this
            .call_method("retrySubscribeChannel", vec![c, a, r])?;
        Ok(())
    }
    fn visit_candles(
        &self,
        c: &mut dyn FnMut(Vec<u16>, Js) -> Result<(), Thrown>,
    ) -> Result<(), Thrown> {
        for pair in primitives::iterate(&self.state("ohlcvCallbacks")?)? {
            c(pair.get("0")?.to_js_string()?.to_vec(), pair.get("1")?)?;
        }
        Ok(())
    }
    fn interval(&self, k: &[u16]) -> Result<Option<f64>, Thrown> {
        let v = self.map_get("ohlcvIntervals", Js::utf16(k))?;
        if v.is_undefined() {
            Ok(None)
        } else {
            Ok(Some(v.to_number()?))
        }
    }
    fn write_trade_time(&self, k: &[u16], t: f64) -> Result<(), Thrown> {
        self.map_set("ohlcvLastTradeAt", Js::utf16(k), Js::Number(t))
    }
    fn candle(&self, k: &[u16]) -> Result<Js, Thrown> {
        self.map_get("lastCandleCache", Js::utf16(k))
    }
    fn spread(&self, v: &Js) -> Result<Js, Thrown> {
        v.spread_object()
    }
    fn make_candle(&self, c: FeedCandle<Js>) -> Result<Js, Thrown> {
        let out = c.spread.unwrap_or_else(|| Js::object(vec![]));
        for (k, v) in [("timestamp", c.timestamp), ("open", c.open)] {
            if let Some(v) = v {
                out.set(k, v)?;
            }
        }
        for (k, v) in [
            ("high", c.high),
            ("low", c.low),
            ("close", Js::Number(c.close)),
            ("volume", c.volume),
        ] {
            out.set(k, v)?;
        }
        Ok(out)
    }
    fn write_candle(&self, k: &[u16], v: Js) -> Result<(), Thrown> {
        self.map_set("lastCandleCache", Js::utf16(k), v)
    }
}

impl crate::private_ingress::PrivateIngressHost for SocketHost<'_> {
    type Value = Js;
    type Error = Thrown;
    type Log = Js;
    fn get(&self, v: &Js, k: &str) -> Result<Js, Thrown> {
        v.get(k)
    }
    fn truthy(&self, v: &Js) -> bool {
        v.truthy()
    }
    fn nullish(&self, v: &Js) -> bool {
        v.is_nullish()
    }
    fn undefined(&self, v: &Js) -> bool {
        v.is_undefined()
    }
    fn object(&self, v: &Js) -> bool {
        v.type_of() == "object"
    }
    fn array(&self, v: &Js) -> Result<bool, Thrown> {
        Ok(v.is_array())
    }
    fn boolean(&self, v: &Js) -> Option<bool> {
        if let Js::Bool(b) = v { Some(*b) } else { None }
    }
    fn literal(&self, v: &str) -> Js {
        Js::str(v)
    }
    fn boolean_value(&self, v: bool) -> Js {
        Js::Bool(v)
    }
    fn number_template(&self, n: f64) -> Result<Vec<u16>, Thrown> {
        Js::Number(n).to_js_string().map(|v| v.to_vec())
    }
    fn equals(&self, a: &Js, b: &Js) -> bool {
        a.strict_equals(b)
    }
    fn parse_float(&self, v: &Js) -> Result<f64, Thrown> {
        PublicIngressHost::parse_float(self, v)
    }
    fn string(&self, v: &Js) -> Result<Js, Thrown> {
        self.rt
            .global("String")?
            .call(Js::Undefined, vec![v.clone()])
    }
    fn template(&self, v: &Js) -> Result<Vec<u16>, Thrown> {
        v.to_js_string().map(|v| v.to_vec())
    }
    fn select_log(&self) -> Result<Js, Thrown> {
        self.logger()
    }
    fn log(&self, l: Js, t: Vec<u16>, s: &'static str) -> Result<(), Thrown> {
        SocketHost::log(self, l, t, s)
    }
    fn log_caught(&self, p: &'static str, e: &Thrown) -> Result<(), Thrown> {
        self.caught(p, e)
    }
    fn disposed(&self) -> Result<bool, Thrown> {
        Ok(self.state("isDisposed")?.truthy())
    }
    fn track_received(&self) -> Result<(), Thrown> {
        self.rt.this.call_method("trackLastReceived", vec![])?;
        Ok(())
    }
    fn parse_event(&self) -> Result<Js, Thrown> {
        self.parse(&self.event)
    }
    fn handlers(&self, t: &Js) -> Result<Js, Thrown> {
        self.state("messageHandlers")?.get_key(&t.to_js_string()?)
    }
    fn stats(&self, t: &Js) -> Result<(), Thrown> {
        self.rt
            .this
            .call_method("recordMessageHandlerStats", vec![t.clone()])?;
        Ok(())
    }
    fn for_each_handler(
        &self,
        h: &Js,
        mut c: impl FnMut(&Js) -> Result<(), Thrown>,
    ) -> Result<(), Thrown> {
        self.each(h, &mut |v| c(&v))
    }
    fn call_handler(&self, f: &Js, d: &Js) -> Result<(), Thrown> {
        f.call(Js::Undefined, vec![d.clone()])?;
        Ok(())
    }
    fn auth_id(&self) -> Result<Js, Thrown> {
        self.state("authRequestId")
    }
    fn authenticated(&self) -> Result<bool, Thrown> {
        Ok(self.state("authenticated")?.truthy())
    }
    fn write_authenticated(&self, b: bool) -> Result<(), Thrown> {
        self.rt.this.set("authenticated", Js::Bool(b))
    }
    fn connected(&self) -> Result<bool, Thrown> {
        Ok(self.state("isConnected")?.truthy())
    }
    fn close_socket(&self) -> Result<(), Thrown> {
        let ws = self.state("ws")?;
        if !ws.is_nullish() {
            ws.call_method("close", vec![])?;
        }
        Ok(())
    }
    fn subscribe_private(&self) -> Result<(), Thrown> {
        self.rt
            .this
            .call_method("subscribeToPrivateChannels", vec![])?;
        Ok(())
    }
    fn side_lookup(&self, s: &Js) -> Result<Js, Thrown> {
        primitives::call(self.rt, "ORDER_SIDE", vec![s.clone()])
    }
    fn map_status(&self, o: &Js) -> Result<Js, Thrown> {
        self.rt
            .this
            .call_method("mapOrderStatus", vec![o.get("status")?])
    }
    fn flags_include_reduce_only(&self, v: &Js) -> Result<Js, Thrown> {
        v.call_method("includes", vec![Js::str("REDUCE_ONLY")])
    }
    fn reverse_type(&self, o: &Js, r: &Js) -> Result<Js, Thrown> {
        self.parent()?
            .call_method("reverseMapOrderType", vec![o.get("type")?, r.clone()])
    }
    fn is_trigger_type(&self, v: &Js) -> Result<bool, Thrown> {
        Ok(primitives::call(self.rt, "isTrigger", vec![v.clone()])?.truthy())
    }
    fn now(&self) -> Result<f64, Thrown> {
        PublicIngressHost::now(self)
    }
    fn format_reason(&self, o: &Js) -> Result<Js, Thrown> {
        self.rt
            .this
            .call_method("formatCancelReason", vec![o.get("cancel_reason")?])
    }
    fn emit_rejection(&self, t: Vec<u16>) -> Result<(), Thrown> {
        self.parent()?
            .call_method("emitAndLogError", vec![Js::utf16(&t)])?;
        Ok(())
    }
    fn store_order(&self, o: crate::private_ingress::PrivateOrder<Js>) -> Result<(), Thrown> {
        let row = Js::object(vec![
            ("id", o.id),
            ("symbol", o.symbol),
            ("side", o.side),
            ("type", o.order_type),
            ("status", o.status),
            ("price", Js::Number(o.price)),
            ("amount", Js::Number(o.amount)),
            ("filled", Js::Number(o.filled)),
            ("remaining", Js::Number(o.remaining)),
            ("reduceOnly", o.reduce_only),
            ("timeInForce", o.time_in_force),
            ("lastUpdate", Js::Number(o.last_update)),
            ("info", o.info),
        ]);
        self.parent()?
            .get("store")?
            .call_method("addOrUpdateOrder", vec![row])?;
        Ok(())
    }
    fn refresh_orders(&self) -> Result<(), Thrown> {
        self.rt.this.call_method("throttledFetchOrders", vec![])?;
        Ok(())
    }
    fn write_pending_positions(&self, b: bool) -> Result<(), Thrown> {
        self.rt.this.set("hasPendingPositionEvents", Js::Bool(b))
    }
    fn position_id(
        &self,
        s: &Js,
        side: safe_cex_types::PositionSide,
        mode: safe_cex_types::PositionMode,
    ) -> Result<Js, Thrown> {
        self.rt.global("createPositionId")?.call(
            Js::Undefined,
            vec![
                s.clone(),
                safe_cex_exchange::js_runtime::value::from_typed(&side)?,
                safe_cex_exchange::js_runtime::value::from_typed(&mode)?,
            ],
        )
    }
    fn store_position(&self, p: crate::private_ingress::PrivatePosition<Js>) -> Result<(), Thrown> {
        use safe_cex_exchange::js_runtime::value::from_typed;
        let row = Js::object(vec![
            ("id", p.id),
            ("symbol", p.symbol),
            ("side", from_typed(&p.side)?),
            ("contracts", Js::Number(p.contracts)),
            ("notional", Js::Number(p.notional)),
            ("unrealizedPnl", Js::Number(p.unrealized_pnl)),
            ("realizedPnl", Js::Number(p.realized_pnl)),
            ("leverage", Js::Number(p.leverage)),
            ("entryPrice", Js::Number(p.entry_price)),
            ("liquidationPrice", Js::Number(p.liquidation_price)),
            ("marginType", from_typed(&p.margin_type)?),
            ("positionMode", from_typed(&p.position_mode)?),
            ("lastUpdate", p.last_update),
            ("info", p.info),
        ]);
        self.parent()?
            .get("store")?
            .call_method("addOrUpdatePosition", vec![row])?;
        Ok(())
    }
    fn refresh_positions(&self) -> Result<(), Thrown> {
        self.rt
            .this
            .call_method("throttledFetchPositions", vec![])?;
        Ok(())
    }
    fn find_object(
        &self,
        a: &Js,
        mut p: impl FnMut(&Js) -> Result<bool, Thrown>,
    ) -> Result<Js, Thrown> {
        for v in primitives::iterate(a)? {
            if p(&v)? {
                return Ok(v);
            }
        }
        Ok(Js::Undefined)
    }
    fn stringify_data(&self, d: &Js) -> Result<Vec<u16>, Thrown> {
        PublicIngressHost::stringify(self, d)
    }
    fn positions_length(&self) -> Result<f64, Thrown> {
        self.parent()?
            .get("store")?
            .get("positions")?
            .get("length")?
            .to_number()
    }
    fn store_balance(&self, b: crate::private_ingress::PrivateBalance) -> Result<(), Thrown> {
        self.parent()?.get("store")?.call_method(
            "update",
            vec![Js::object(vec![(
                "balance",
                Js::object(vec![
                    ("total", Js::Number(b.total)),
                    ("free", Js::Number(b.free)),
                    ("used", Js::Number(b.used)),
                    ("upnl", Js::Number(b.upnl)),
                ]),
            )])],
        )?;
        Ok(())
    }
}
