// Test instrumentation only: the caller supplies the actual venue crate alias.
#[allow(dead_code)]
pub mod io {
    use super::venue_crate as safe_cex_venue_kucoin;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/exchange_runtime.rs"
    ));

    pub fn connected_parts(
        name: ExchangeName,
    ) -> (
        Fixture,
        BaseExchangeParts,
        KucoinDependencies,
        Rc<super::ConnectedRuntime>,
    ) {
        let runtime = Rc::new(Runtime::default());
        let (fixture, parts, mut deps) = parts(name, &runtime, options());
        let connected = Rc::new(super::ConnectedRuntime {
            inner: runtime,
            sockets: RefCell::default(),
        });
        deps.runtime = connected.clone();
        (fixture, parts, deps, connected)
    }
}

use std::{cell::RefCell, rc::Rc};

pub fn market(id: &str, symbol: &str, contract_size: Option<f64>) -> safe_cex_types::Market {
    use safe_cex_types::{
        Market, MarketLimits, MarketLimitsAmount, MarketLimitsLeverage, MarketPrecision,
    };
    Market {
        id: id.into(),
        symbol: symbol.into(),
        pretty_symbol: None,
        base: "NATIVE".into(),
        quote: "USDT".into(),
        active: true,
        precision: MarketPrecision {
            amount: 0.5,
            price: 0.25,
        },
        limits: MarketLimits {
            amount: MarketLimitsAmount {
                min: 0.5,
                max: 1000.0,
            },
            leverage: MarketLimitsLeverage {
                min: 1.0,
                max: 125.0,
            },
            price: None,
        },
        option: None,
        contract_size,
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: None,
        is_read_only: None,
        info: None,
        taker_fee: None,
        maker_fee: None,
    }
}

pub fn seed_market(store: &dyn safe_cex_store::Store, market: safe_cex_types::Market) {
    safe_cex_store::Store::update(
        store,
        safe_cex_store::StoreUpdate {
            markets: Some(vec![market]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
}

pub fn order(value: &safe_cex_types::Order) -> serde_json::Value {
    serde_json::json!({"id":value.id,"symbol":value.symbol,
        "side":format!("{:?}",value.side),"type":format!("{:?}",value.r#type),
        "status":format!("{:?}",value.status),"tif":format!("{:?}",value.time_in_force),
        "bits":([value.price.to_number(),value.amount,value.filled,value.remaining,value.last_update]
            .map(|v|format!("{:016x}",v.to_bits()))),"reduce":value.reduce_only,
        "remainingNativeBits":value.extra.as_ref().and_then(|v|v.remaining_native).map(|v|format!("{:016x}",v.to_bits())),
        "infoId":value.info.as_ref().and_then(|v|v.get("id"))})
}

pub fn ticker(symbol: &str, last: f64) -> safe_cex_types::Ticker {
    safe_cex_types::Ticker {
        id: "API".into(),
        symbol: symbol.into(),
        bid: 0.0,
        ask: 0.0,
        last,
        mark: 0.0,
        index: 0.0,
        mid_price: None,
        percentage: 0.0,
        open_interest: 0.0,
        funding_rate: 0.0,
        funding_time: 0.0,
        volume: 0.0,
        quote_volume: 0.0,
        timestamp: None,
        info: None,
    }
}

/// Scripted WebSocket I/O over the same E2E task/clock implementation.
/// This changes only dial/event delivery; all venue methods remain production.
pub struct ConnectedRuntime {
    pub inner: Rc<io::Runtime>,
    pub sockets: RefCell<Vec<Rc<ConnectedSocket>>>,
}
#[derive(Default)]
pub struct ConnectedSocket {
    pub events: Rc<RefCell<std::collections::VecDeque<safe_cex_exchange::websocket::WsEvent>>>,
    pub sent: RefCell<Vec<String>>,
    pub closed: std::cell::Cell<bool>,
}
impl safe_cex_exchange::websocket::WsSocket for ConnectedSocket {
    fn ready_state(&self) -> u8 {
        if self.closed.get() { 3 } else { 1 }
    }
    fn next_event(
        &self,
    ) -> safe_cex_exchange::runtime::ExchangeFuture<'static, safe_cex_exchange::websocket::WsEvent>
    {
        let events = self.events.clone();
        Box::pin(std::future::poll_fn(move |_| {
            events
                .borrow_mut()
                .pop_front()
                .map_or(std::task::Poll::Pending, |event| {
                    std::task::Poll::Ready(Ok(event))
                })
        }))
    }
    fn send(&self, message: &str) -> Result<(), safe_cex_exchange::runtime::ExchangeError> {
        self.sent.borrow_mut().push(message.to_owned());
        Ok(())
    }
    fn close(&self) {
        self.closed.set(true);
    }
    fn terminate(&self) {
        self.closed.set(true);
    }
    fn reconnect(&self) {}
    fn is_reconnecting(&self) -> bool {
        false
    }
}
impl safe_cex_exchange::runtime::ExchangeRuntime for ConnectedRuntime {
    fn monotonic_now_ms(&self) -> f64 {
        self.inner.monotonic_now_ms()
    }
    fn now_ms(&self) -> f64 {
        self.inner.now_ms()
    }
    fn sleep(&self, duration: u64) -> safe_cex_exchange::runtime::ExchangeFuture<'static, ()> {
        self.inner.sleep(duration)
    }
    fn spawn_local(
        &self,
        future: std::pin::Pin<Box<dyn std::future::Future<Output = ()>>>,
    ) -> Box<dyn safe_cex_exchange::runtime::ExchangeTask> {
        self.inner.spawn_local(future)
    }
    fn console_values(&self, level: &str, message: &[safe_cex_exchange::js_runtime::value::Js]) {
        self.inner.console_values(level, message);
    }
    fn unique_id(&self) -> String {
        self.inner.unique_id()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        self.inner.random()
    }
    fn report_raw_exception(
        &self,
        value: &safe_cex_exchange::js_runtime::value::Js,
        context: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        self.inner.report_raw_exception(value, context);
    }
    fn report_exception(
        &self,
        error: &safe_cex_exchange::runtime::ExchangeError,
        context: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        self.inner.report_exception(error, context);
    }
    fn execute_worker_request(
        &self,
        value: serde_json::Value,
    ) -> safe_cex_exchange::runtime::ExchangeFuture<'static, serde_json::Value> {
        self.inner.execute_worker_request(value)
    }
}
impl safe_cex_exchange::websocket::WsRuntime for ConnectedRuntime {
    fn transport(&self) -> std::sync::Arc<dyn safe_cex_transport::TransportProvider> {
        self.inner.transport()
    }
    fn tunnel_host(&self) -> Rc<dyn safe_cex_transport::socket::TunnelHost> {
        self.inner.tunnel_host()
    }
    fn dial(
        &self,
        url: String,
        _: Vec<String>,
        _: safe_cex_exchange::websocket::WebSocketConnectionOptions,
        _: bool,
    ) -> safe_cex_exchange::runtime::ExchangeFuture<
        'static,
        Rc<dyn safe_cex_exchange::websocket::WsSocket>,
    > {
        self.inner.dials.borrow_mut().push(url);
        let socket = Rc::new(ConnectedSocket::default());
        socket
            .events
            .borrow_mut()
            .push_back(safe_cex_exchange::websocket::WsEvent::Open);
        self.sockets.borrow_mut().push(socket.clone());
        Box::pin(async move { Ok(socket as Rc<dyn safe_cex_exchange::websocket::WsSocket>) })
    }
    fn headers_capable(&self) -> bool {
        false
    }
    fn enable_reconnecting(&self) -> bool {
        false
    }
    fn min_reconnection_delay_ms(&self) -> f64 {
        1000.0
    }
    fn library_min_reconnection_delay_ms(&self) -> f64 {
        3000.0
    }
    fn diagnostics_enabled(&self) -> bool {
        false
    }
    fn lifecycle_logs_enabled(&self) -> bool {
        false
    }
}
