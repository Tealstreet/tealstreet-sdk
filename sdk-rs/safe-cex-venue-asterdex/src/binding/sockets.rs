use super::{Rt, Shared, error, members, scope};
use safe_cex_exchange::{
    exchange::{BaseExchange, Exchange},
    js_runtime::value::{Js, to_typed},
    runtime::{ExchangeError, ExchangeFuture, LogSeverity},
    socket_lifecycle::SocketSlot,
    websocket::{BaseWebSocket, ReopenResync, WsParent},
    ws_parent::WsExchangeParent,
};
use safe_cex_store::Store;
use safe_cex_types::{ExchangeOptions, Order, Position, Ticker};
use serde_json::{Value, json};
use std::rc::{Rc, Weak};

struct Parent {
    weak: Weak<Shared>,
    inner: Rc<WsExchangeParent>,
}
impl Parent {
    fn rt(&self) -> Result<Rt, ExchangeError> {
        let shared = self
            .weak
            .upgrade()
            .ok_or_else(|| ExchangeError::new("AsterDEX exchange was dropped"))?;
        let this = shared
            .extra
            .owner
            .get()
            .expect("exchange assembled")
            .clone();
        Ok(Rt { shared, this })
    }
}
impl WsParent for Parent {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn is_disposed(&self) -> bool {
        self.inner.is_disposed()
    }
    fn options(&self) -> ExchangeOptions {
        self.inner.options()
    }
    fn store(&self) -> Rc<dyn Store> {
        self.inner.store()
    }
    fn is_private_socket(&self, socket: &BaseWebSocket) -> bool {
        self.inner.is_private_socket(socket)
    }
    fn resync_coordinator(&self) -> Rc<ReopenResync> {
        self.inner.resync_coordinator()
    }
    fn fetch_and_set_private_snapshot(&self, retries: usize) -> ExchangeFuture<'static, ()> {
        self.inner.fetch_and_set_private_snapshot(retries)
    }
    fn log(&self, message: &str, severity: LogSeverity) -> Result<(), ExchangeError> {
        self.inner.log(message, severity)
    }
    fn emit(&self, event: &str, message: &str) -> Result<(), ExchangeError> {
        self.inner.emit(event, message)
    }
    fn sync_socket_health_to_store(&self) -> Result<(), ExchangeError> {
        self.inner.sync_socket_health_to_store()
    }
    fn update_latency(&self, latency: f64) -> Result<(), ExchangeError> {
        self.inner.update_latency(latency)
    }
    fn emit_ws_reconnected(&self) -> Result<(), ExchangeError> {
        self.inner.emit_ws_reconnected()
    }
    fn emit_and_log_error(
        &self,
        e: &str,
        m: Option<&str>,
        extra: Option<&safe_cex_types::retained_json::RetainedJsonInput>,
    ) -> Result<(), ExchangeError> {
        self.inner.emit_and_log_error(e, m, extra)
    }
}
impl crate::public_stream::PublicStreamParent for Parent {
    fn owner(&self) -> Rc<BaseExchange> {
        self.rt().expect("live socket parent").base().clone()
    }
    fn emit_ticker(
        &self,
        event: &str,
        symbol: &Value,
        ticker: &Ticker,
    ) -> Result<(), ExchangeError> {
        self.rt()?.base().emit_args(
            event,
            &[
                safe_cex_exchange::js_runtime::value::from_json(symbol),
                safe_cex_exchange::js_runtime::value::from_typed(ticker)?,
            ],
        )
    }
    fn emit_trade(
        &self,
        trade: &crate::ws_helpers::PublicStreamTradeEvent,
    ) -> Result<(), ExchangeError> {
        use safe_cex_exchange::js_runtime::value::{Js, from_typed};
        self.rt()?.base().emit(
            "trade",
            Js::object(vec![
                ("symbol", Js::str(&trade.symbol)),
                ("price", Js::Number(trade.price)),
                ("qty", Js::Number(trade.qty)),
                ("timestamp", Js::Number(trade.timestamp)),
                ("side", from_typed(&trade.side)?),
            ]),
        )
    }
}
impl crate::private_repairs::RepairParent for Parent {
    fn position_state_version(&self) -> f64 {
        self.rt()
            .ok()
            .and_then(|rt| rt.this.get("positionStateVersion").ok())
            .and_then(|v| v.to_number().ok())
            .unwrap_or(0.0)
    }
    fn balance_state_version(&self) -> f64 {
        self.rt()
            .ok()
            .and_then(|rt| rt.this.get("balanceStateVersion").ok())
            .and_then(|v| v.to_number().ok())
            .unwrap_or(0.0)
    }
    fn fetch_positions(&self) -> ExchangeFuture<'static, Vec<Position>> {
        let rt = self.rt();
        Box::pin(async move {
            let rt = rt?;
            rt.shared
                .extra
                .account
                .get()
                .expect("account assembled")
                .fetch_positions(None)
                .await
        })
    }
    fn fetch_orders(&self) -> ExchangeFuture<'static, Vec<Order>> {
        let rt = self.rt();
        Box::pin(async move {
            let rt = rt?;
            rt.shared
                .extra
                .account
                .get()
                .expect("account assembled")
                .fetch_orders(None)
                .await
        })
    }
    fn reconcile_balance_after_private_update(
        &self,
        version: f64,
    ) -> ExchangeFuture<'static, bool> {
        let rt = self.rt();
        Box::pin(async move {
            let rt = rt?;
            let result = safe_cex_exchange::js_runtime::promise::await_js(&rt.exchange_runtime(), members::invoke(
                &rt,
                "reconcileBalanceAfterPrivateUpdate",
                vec![Js::Number(version)],
            )?)
            .await?;
            Ok(to_typed(&result)?)
        })
    }
}
impl crate::private_stream::PrivateStreamParent for Parent {
    fn owner(&self) -> Rc<BaseExchange> {
        self.rt().expect("live socket parent").base().clone()
    }
    fn get_fapi_path(&self, endpoint: &str) -> String {
        let rt = self.rt().expect("live socket parent");
        crate::exchange_helpers::get_fapi_path(endpoint, &rt.shared.extra.version, None)
    }
    fn post_listen_key(&self, path: String) -> ExchangeFuture<'static, String> {
        let rt = self.rt();
        Box::pin(async move {
            let data = members::request(&rt?, "primaryAPI", "post", path, None).await?;
            data.get("listenKey")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| ExchangeError::new("listenKey response has no string listenKey"))
        })
    }
    fn put_listen_key(&self, path: String, key: String) -> ExchangeFuture<'static, ()> {
        let rt = self.rt();
        Box::pin(async move {
            members::request(
                &rt?,
                "primaryAPI",
                "put",
                path,
                Some(json!({"listenKey":key})),
            )
            .await?;
            Ok(())
        })
    }
    fn virtual_time_ms(&self) -> f64 {
        self.rt()
            .expect("live socket parent")
            .shared
            .extra
            .clock
            .get()
            .expect("clock assembled")
            .get_current_time_ms()
    }
    fn mark_position_state_updated(&self) {
        if let Ok(rt) = self.rt() {
            let _ = members::invoke(&rt, "markPositionStateUpdatedByWebsocket", vec![]);
        }
    }
    fn mark_balance_state_updated(&self) -> f64 {
        self.rt()
            .and_then(|rt| {
                Ok(
                    members::invoke(&rt, "markBalanceStateUpdatedByWebsocket", vec![])?
                        .to_number()?,
                )
            })
            .unwrap_or(0.0)
    }
    fn emit_order(&self, order: &Order) -> Result<(), ExchangeError> {
        self.rt()?.base().emit_args(
            "order",
            &[
                safe_cex_exchange::js_runtime::value::Js::str(&order.id),
                safe_cex_exchange::js_runtime::value::from_typed(order)?,
            ],
        )
    }
    fn emit_fill(&self, fill: &crate::private_stream::PrivateFill) -> Result<(), ExchangeError> {
        use safe_cex_exchange::js_runtime::value::{Js, from_typed};
        self.rt()?.base().emit(
            "fill",
            Js::object(vec![
                ("id", fill.id.as_deref().map_or(Js::Undefined, Js::str)),
                (
                    "orderId",
                    fill.order_id.as_deref().map_or(Js::Undefined, Js::str),
                ),
                (
                    "symbol",
                    fill.symbol.as_deref().map_or(Js::Undefined, Js::str),
                ),
                ("side", from_typed(&fill.side)?),
                ("price", Js::Number(fill.price)),
                ("amount", Js::Number(fill.amount)),
            ]),
        )
    }
    fn emit_positions(&self, positions: &[Position]) -> Result<(), ExchangeError> {
        self.rt()?.base().emit(
            "positions",
            safe_cex_exchange::js_runtime::value::from_typed(positions)?,
        )
    }
    fn emit_account(&self, data: &Value) -> Result<(), ExchangeError> {
        self.rt()?.base().emit(
            "account",
            safe_cex_exchange::js_runtime::value::from_json(data),
        )
    }
}

pub(super) fn initialize(rt: &Rt, venue: &Rc<dyn Exchange>) {
    let parent = Rc::new(Parent {
        weak: Rc::downgrade(&rt.shared),
        inner: WsExchangeParent::new(venue),
    });
    let public =
        crate::public_stream::PublicStream::new(1, parent.clone(), rt.shared.runtime.clone());
    let private = crate::private_stream::PrivateStream::new(2, parent, rt.shared.runtime.clone());
    rt.base()
        .set_borrowed_socket(SocketSlot::Public, &public.socket);
    rt.base()
        .set_borrowed_socket(SocketSlot::Private, &private.socket);
    *rt.shared.extra.listeners.websocket.borrow_mut() = Some(public.clone());
    let _ = rt.shared.extra.public.set(public);
    let _ = rt.shared.extra.private.set(private);
    rt.base().bind_socket_watchdog();
}
pub(super) fn facade(weak: &Weak<Shared>, private: bool) -> Js {
    let this = Js::object(vec![]);
    let w = weak.clone();
    let _ = this.define(
        "connectAndSubscribe",
        Js::function("connectAndSubscribe", move |_, _| {
            let rt = scope(&w, Js::Undefined)?;
            let work = if private {
                rt.shared
                    .extra
                    .private
                    .get()
                    .expect("private socket assembled")
                    .connect_and_subscribe()
            } else {
                rt.shared
                    .extra
                    .public
                    .get()
                    .expect("public socket assembled")
                    .connect_and_subscribe()
            };
            Ok(rt.spawn(Box::pin(async move {
                work.await.map_err(error)?;
                Ok(Js::Undefined)
            })))
        }),
    );
    let w = weak.clone();
    let _ = this.define(
        "dispose",
        Js::function("dispose", move |_, _| {
            let rt = scope(&w, Js::Undefined)?;
            if private {
                rt.shared
                    .extra
                    .private
                    .get()
                    .expect("private socket assembled")
                    .dispose();
            } else {
                rt.shared
                    .extra
                    .public
                    .get()
                    .expect("public socket assembled")
                    .dispose();
            }
            Ok(Js::Undefined)
        }),
    );
    this
}
