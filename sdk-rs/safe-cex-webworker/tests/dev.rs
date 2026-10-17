#[path = "../../safe-cex-exchange/tests/support/runtime.rs"]
mod runtime;
#[path = "support/ws_runtime.rs"]
mod ws_runtime;
use runtime::Runtime;
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange},
    socket_lifecycle::SocketSlot,
    websocket::{BaseWebSocket, WsProtocol},
    ws_parent::WsExchangeParent,
};
use safe_cex_store::store::DefaultStore;
use safe_cex_types::ExchangeName;
use safe_cex_webworker::{dev::DevHandler, payload::StoreWorkerPayload};
use serde_json::{Value, json};
use std::rc::Rc;
struct Venue {
    base: BaseExchange,
    capabilities: ExchangeCapabilities,
}
impl Exchange for Venue {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
}
fn venue() -> (Rc<dyn Exchange>, Rc<Runtime>) {
    let runtime = Rc::new(Runtime::default());
    runtime.discard_spawn.set(true);
    runtime.advance(1);
    let exchange = Rc::new(Venue {
        base: BaseExchange::new(
            ExchangeName::Bybit,
            serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
            Rc::new(DefaultStore::new()),
            runtime.clone(),
            Default::default(),
        ),
        capabilities: Default::default(),
    });
    (exchange, runtime)
}
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_transport::{
    TransportError, TransportFuture,
    how::{HOWWebSocket, HowRestAdapter, get_how_websocket_for_exchange},
    provider::{RestAdapter, RestRequest},
    routing::HowEgress,
    socket::{
        SocketCapabilities, SocketFrame, SocketRuntime, TunnelDialOptions, TunnelHost,
        TunnelSocket, TunnelTask,
    },
};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    task::{Context, Waker},
};
struct Protocol {
    id: usize,
    fail: i64,
    events: Rc<RefCell<Vec<String>>>,
}
impl WsProtocol for Protocol {
    fn connect_and_subscribe(
        &self,
        _: Rc<BaseWebSocket>,
        reconnect: bool,
    ) -> Result<(), ExchangeError> {
        assert!(!reconnect);
        self.events.borrow_mut().push(format!("open:{}", self.id));
        if self.fail == self.id as i64 {
            Err(ExchangeError::new("fixture failure"))
        } else {
            Ok(())
        }
    }
}
struct Host {
    connects: Cell<usize>,
    events: Rc<RefCell<Vec<String>>>,
}
struct Task;
impl TunnelTask for Task {
    fn cancel(&self) {}
}
struct Socket {
    region: String,
    events: Rc<RefCell<Vec<String>>>,
}
impl TunnelSocket for Socket {
    fn send(&self, _: SocketFrame) -> Result<(), TransportError> {
        Ok(())
    }
    fn receive(&self) -> TransportFuture<'static, SocketFrame> {
        Box::pin(std::future::pending())
    }
    fn close(&self, _: Option<u16>, _: &str) -> Result<(), TransportError> {
        self.events.borrow_mut().push(self.region.clone());
        Ok(())
    }
}
impl TunnelHost for Host {
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn TunnelTask> {
        Box::new(Task)
    }
    fn capabilities(&self) -> SocketCapabilities {
        SocketCapabilities {
            runtime: SocketRuntime::BrowserWorker,
            upgrade_headers: false,
            alpn: false,
        }
    }
    fn now_ms(&self) -> u64 {
        1
    }
    fn sleep_until(&self, _: u64) -> TransportFuture<'static, ()> {
        Box::pin(std::future::pending())
    }
    fn connect(
        &self,
        _url: String,
        _: TunnelDialOptions,
    ) -> TransportFuture<'static, Rc<dyn TunnelSocket>> {
        let call = self.connects.get();
        self.connects.set(call + 1);
        assert!(call < 2);
        let region = if call == 0 { "ja" } else { "sg" };
        let socket: Rc<dyn TunnelSocket> = Rc::new(Socket {
            region: region.into(),
            events: self.events.clone(),
        });
        Box::pin(async move { Ok(socket) })
    }
    fn proxy_token(&self) -> TransportFuture<'_, String> {
        Box::pin(async { Ok("fixture".into()) })
    }
    fn is_token_expired(&self, _: &str) -> bool {
        false
    }
    fn sync_server_time(&self, _: u64) {}
}
#[test]
fn dev_dispatch_reaches_real_socket_and_regional_how_targets() {
    let output = std::process::Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/dev_oracle.mjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 100);
    for (index, row) in rows.iter().enumerate() {
        let (exchange, runtime) = venue();
        let events = Rc::new(RefCell::new(vec![]));
        let host = Rc::new(Host {
            connects: Cell::new(0),
            events: events.clone(),
        });
        let ja_adapter = HowRestAdapter::new(host.clone(), Some(HowEgress::Jp), false).unwrap();
        let sg_adapter = HowRestAdapter::new(host.clone(), Some(HowEgress::Sg), false).unwrap();
        let ja: Rc<HOWWebSocket> = get_how_websocket_for_exchange(host.clone(), "binance").unwrap();
        let sg = get_how_websocket_for_exchange(host.clone(), "bybit").unwrap();
        assert!(!Rc::ptr_eq(&ja, &sg));
        let request = || RestRequest {
            method: "GET".into(),
            url: "https://fixture.invalid/example".into(),
            ..Default::default()
        };
        let mut ja_pending = ja_adapter.execute(request());
        let mut sg_pending = sg_adapter.execute(request());
        let mut cx = Context::from_waker(Waker::noop());
        assert!(ja_pending.as_mut().poll(&mut cx).is_pending());
        assert!(sg_pending.as_mut().poll(&mut cx).is_pending());
        assert_eq!(host.connects.get(), 2, "both regional sockets installed");
        let mut sockets = vec![];
        for (id, slot) in [
            SocketSlot::Private,
            SocketSlot::Public,
            SocketSlot::PublicSecondary,
            SocketSlot::PublicInverse,
            SocketSlot::Futures,
            SocketSlot::Business,
            SocketSlot::SecondsTickers,
            SocketSlot::ThirdTickers,
        ]
        .into_iter()
        .enumerate()
        {
            let socket = if row["mask"].as_u64().unwrap() & (1 << id) != 0 {
                Some(BaseWebSocket::new(
                    id as u64,
                    WsExchangeParent::new(&exchange),
                    runtime.clone(),
                    Rc::new(Protocol {
                        id,
                        fail: row["fail"].as_i64().unwrap(),
                        events: events.clone(),
                    }),
                ))
            } else {
                None
            };
            exchange.base().set_socket(slot, socket.clone());
            sockets.push(socket);
        }
        let handler = DevHandler::new(exchange, ja, sg);
        let payload: StoreWorkerPayload = serde_json::from_value(json!({"op":row["op"]})).unwrap();
        let result = handler.handle(payload);
        assert_eq!(
            json!(result.err().map(|e| e.message)),
            row["error"],
            "error {index}"
        );
        assert_eq!(
            json!(*events.borrow()),
            row["events"],
            "CALL-TARGET {index}"
        );
        assert_eq!(
            json!(
                sockets
                    .iter()
                    .map(|s| s.as_ref().is_some_and(|s| s.is_force_closed()))
                    .collect::<Vec<_>>()
            ),
            row["closed"],
            "force closed {index}"
        );
        assert_eq!(
            json!(
                sockets
                    .iter()
                    .map(|s| s.as_ref().map_or(0, |s| s.diagnostics().on_close_count))
                    .collect::<Vec<_>>()
            ),
            row["closes"],
            "reconnect {index}"
        );
    }
}
