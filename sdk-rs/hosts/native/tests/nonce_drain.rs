use futures_util::FutureExt;
use safe_cex_engine::{HttpRequest, Method, RequestId, SocketId};
use safe_cex_native::{
    Events, HostEvent, NetworkTransport, ProxyTokenProvider, RoutingPlan, Transport,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot};

fn request(url: String, id: u64) -> HttpRequest {
    HttpRequest {
        id: RequestId(id),
        method: Method::Post,
        route: None,
        timeout_ms: Some(5_000),
        url,
        headers: vec![],
        signer_headers: vec![],
        body: None,
    }
}

async fn headers(stream: &mut TcpStream) {
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        assert_eq!(stream.read(&mut byte).await.unwrap(), 1);
        bytes.push(byte[0]);
        assert!(bytes.len() < 8_192);
    }
}

async fn started(events: &mut mpsc::UnboundedReceiver<HostEvent>) {
    let event = tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .expect("the real request starts")
        .expect("the host event channel stays open");
    assert!(matches!(event, HostEvent::HttpStart { status: 200, .. }));
}

#[tokio::test]
async fn a_real_legacy_response_body_is_joined_before_retirement() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/private", listener.local_addr().unwrap());
    let (release, wait) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        headers(&mut stream).await;
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{")
            .await
            .unwrap();
        if wait.await.is_ok() {
            stream.write_all(b"}").await.unwrap();
        }
    });
    let transport = NetworkTransport::new();
    let (send, mut receive) = mpsc::unbounded_channel();
    transport.request(request(url, 1), send);
    started(&mut receive).await;
    let mut drain = transport
        .drain()
        .expect("NetworkTransport must expose actual joined retirement");
    assert!(drain.as_mut().now_or_never().is_none());
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), drain)
        .await
        .expect("retirement follows the real body task");
    server.await.unwrap();
}

struct Dropped(Arc<AtomicUsize>);
impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn pending_raw_preparation_is_canceled_and_joined_without_dispatch() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/private", listener.local_addr().unwrap());
    let (entered, entered_wait) = oneshot::channel();
    let (release, wait) = oneshot::channel::<String>();
    let state = Arc::new(Mutex::new(Some((entered, wait))));
    let dropped = Arc::new(AtomicUsize::new(0));
    let watch = dropped.clone();
    let tokens: ProxyTokenProvider = Arc::new(move || {
        let (entered, wait) = state.lock().unwrap().take().unwrap();
        let guard = Dropped(watch.clone());
        Box::pin(async move {
            let _guard = guard;
            entered.send(()).unwrap();
            wait.await.map_err(|_| "token canceled".into())
        })
    });
    let mut routing = RoutingPlan::default();
    routing.http.authorize = true;
    let transport = NetworkTransport::with_routing(routing, tokens);
    let (send, mut receive) = mpsc::unbounded_channel();
    transport.request(request(url, 2), send);
    tokio::time::timeout(Duration::from_secs(5), entered_wait)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(dropped.load(Ordering::SeqCst), 0);
    let drain = transport
        .drain()
        .expect("the real preparation task must participate in retirement");
    tokio::time::timeout(Duration::from_secs(5), drain)
        .await
        .expect("unreleased preparation is canceled and joined");
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    assert!(listener.accept().now_or_never().is_none());
    assert!(!matches!(
        receive.try_recv(),
        Ok(HostEvent::HttpStart { .. })
    ));
    drop(release);
}

#[tokio::test]
async fn sealing_refuses_new_requests_even_before_the_drain_future_is_polled() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/private", listener.local_addr().unwrap());
    let transport = NetworkTransport::new();
    let drain = transport
        .drain()
        .expect("sealing is an explicit capability");
    let (send, mut receive) = mpsc::unbounded_channel();
    transport.request(request(url, 3), send);
    let event = tokio::time::timeout(Duration::from_secs(5), receive.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(event, HostEvent::HttpFailed(RequestId(3), _)));
    assert!(listener.accept().now_or_never().is_none());
    tokio::time::timeout(Duration::from_secs(5), drain)
        .await
        .unwrap();
}

#[tokio::test]
async fn repeated_drain_keeps_the_same_transport_sealed() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/private", listener.local_addr().unwrap());
    let transport = NetworkTransport::new();
    let first = transport.drain().expect("the first drain is supported");
    tokio::time::timeout(Duration::from_secs(5), first)
        .await
        .unwrap();
    let second = transport.drain().expect("a repeated drain stays supported");
    tokio::time::timeout(Duration::from_secs(5), second)
        .await
        .unwrap();
    let (send, mut receive) = mpsc::unbounded_channel();
    transport.request(request(url, 4), send);
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(5), receive.recv())
            .await
            .unwrap()
            .unwrap(),
        HostEvent::HttpFailed(RequestId(4), _)
    ));
    assert!(listener.accept().now_or_never().is_none());
}

struct Untracked(AtomicUsize);
impl Transport for Untracked {
    fn connect(&self, _: SocketId, _: String, _: Events) -> mpsc::UnboundedSender<String> {
        mpsc::unbounded_channel().0
    }
    fn request(&self, _: HttpRequest, _: Events) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn an_ordinary_custom_transport_has_no_false_drain_capability_and_stays_dyn_compatible() {
    let transport = Untracked(AtomicUsize::new(0));
    let dynamic: &dyn Transport = &transport;
    assert!(dynamic.drain().is_none());
    let (send, _) = mpsc::unbounded_channel();
    dynamic.request(request("http://127.0.0.1:1/private".into(), 5), send);
    assert_eq!(transport.0.load(Ordering::SeqCst), 1);
}
