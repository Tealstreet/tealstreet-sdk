// Host I/O recorders only. Protocol, routing and provider decisions use production bodies.
use crate::{
    RestAdapter, RestAdapterRegion, RestFuture, RestRequest, RestResponse, TransportError,
    TransportFuture,
    socket::{
        SocketCapabilities, SocketFrame, SocketRuntime, TunnelDialOptions, TunnelHost,
        TunnelSocket, TunnelTask,
    },
    tealstreet::TealstreetTransportHost,
};
use std::{
    cell::{Cell, RefCell},
    future::{Future, pending},
    pin::Pin,
    rc::Rc,
    sync::Arc,
    task::{Context, Poll, Waker},
};

pub fn ready<T>(future: impl Future<Output = T>) -> T {
    match std::pin::pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("TRANSPORT-CONTRACT scripted host operation unexpectedly pending"),
    }
}
#[derive(Default)]
pub struct Socket {
    pub sent: RefCell<Vec<SocketFrame>>,
    pub closes: RefCell<Vec<(Option<u16>, String)>>,
}
impl TunnelSocket for Socket {
    fn ready_state(&self) -> u16 {
        1
    }
    fn send(&self, frame: SocketFrame) -> Result<(), TransportError> {
        self.sent.borrow_mut().push(frame);
        Ok(())
    }
    fn receive(&self) -> TransportFuture<'static, SocketFrame> {
        Box::pin(pending())
    }
    fn close(&self, code: Option<u16>, reason: &str) -> Result<(), TransportError> {
        self.closes.borrow_mut().push((code, reason.into()));
        Ok(())
    }
}
pub struct Host {
    pub socket: Rc<Socket>,
    pub dials: RefCell<Vec<String>>,
    pub tasks: RefCell<Vec<Pin<Box<dyn Future<Output = ()>>>>>,
    pub capabilities_calls: Cell<u32>,
}
struct Task;
impl TunnelTask for Task {
    fn cancel(&self) {}
}
impl Host {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            socket: Rc::new(Socket::default()),
            dials: RefCell::new(vec![]),
            tasks: RefCell::new(vec![]),
            capabilities_calls: Cell::new(0),
        })
    }
}
impl TunnelHost for Host {
    fn spawn_local(&self, future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn TunnelTask> {
        self.tasks.borrow_mut().push(future);
        Box::new(Task)
    }
    fn capabilities(&self) -> SocketCapabilities {
        self.capabilities_calls
            .set(self.capabilities_calls.get() + 1);
        SocketCapabilities {
            runtime: SocketRuntime::Native,
            upgrade_headers: true,
            alpn: true,
        }
    }
    fn now_ms(&self) -> u64 {
        42
    }
    fn sleep_until(&self, _: u64) -> TransportFuture<'static, ()> {
        Box::pin(pending())
    }
    fn connect(
        &self,
        url: String,
        _: TunnelDialOptions,
    ) -> TransportFuture<'static, Rc<dyn TunnelSocket>> {
        self.dials.borrow_mut().push(url);
        let socket = self.socket.clone() as Rc<dyn TunnelSocket>;
        Box::pin(async move { Ok(socket) })
    }
    fn proxy_token(&self) -> TransportFuture<'_, String> {
        Box::pin(async { Ok("fixture-token".into()) })
    }
    fn is_token_expired(&self, _: &str) -> bool {
        false
    }
    fn sync_server_time(&self, _: u64) {}
}
pub fn sent(socket: &Socket) -> Vec<String> {
    socket
        .sent
        .borrow()
        .iter()
        .map(|frame| match frame {
            SocketFrame::Text(text) => text.clone(),
            _ => panic!("TRANSPORT-CONTRACT expected actual text frame"),
        })
        .collect()
}
pub struct ProviderHost;
struct UnusedAdapter;
impl RestAdapter for UnusedAdapter {
    fn execute(&self, _: RestRequest) -> RestFuture<'_, RestResponse> {
        panic!("TRANSPORT-CONTRACT socket/provider observation unexpectedly performed REST")
    }
}
impl TealstreetTransportHost for ProviderHost {
    fn get_proxy_token(&self) -> TransportFuture<'_, String> {
        panic!("TRANSPORT-CONTRACT routing observation unexpectedly requested a token")
    }
    fn is_node_or_bun_runtime(&self) -> bool {
        true
    }
    fn mark_rest_proxy_header(&self, _: &mut RestRequest, _: &'static str) {
        panic!("TRANSPORT-CONTRACT routing observation unexpectedly marked REST")
    }
    fn default_adapter(&self) -> Arc<dyn RestAdapter> {
        Arc::new(UnusedAdapter)
    }
    fn big_int_adapter(&self) -> Arc<dyn RestAdapter> {
        Arc::new(UnusedAdapter)
    }
    fn regional_adapter(&self, _: Option<RestAdapterRegion>) -> Arc<dyn RestAdapter> {
        Arc::new(UnusedAdapter)
    }
}
