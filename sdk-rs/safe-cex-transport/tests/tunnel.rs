use safe_cex_transport::{
    RestAdapter, RestRequest, TransportError, TransportFuture,
    how::HowRestAdapter,
    relay::{RelayEvent, RelayWebSocket},
    socket::{
        SocketCapabilities, SocketFrame, SocketRuntime, TunnelDialOptions, TunnelHost,
        TunnelSocket, TunnelTask,
    },
};
use safe_cex_types::{QueryValue as Q, retained_json::RetainedJsonInput as Input};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::{Future, poll_fn},
    pin::{Pin, pin},
    rc::Rc,
    task::{Context, Poll, Waker},
};

type Incoming = VecDeque<(u64, Result<SocketFrame, TransportError>)>;
#[derive(Default)]
struct FakeVenue {
    now: Rc<Cell<u64>>,
    frames: RefCell<Incoming>,
    requests: RefCell<Vec<Value>>,
    sent: RefCell<Vec<SocketFrame>>,
    lost_response: Cell<bool>,
    latency_queries: RefCell<Vec<Value>>,
    lost_latency_response: Cell<bool>,
    reorder_responses: Cell<bool>,
    response_data: RefCell<Option<Value>>,
    response_data_json: RefCell<Option<String>>,
    raw_response_body: RefCell<Option<String>>,
    fail_next_ping: Cell<bool>,
    fail_auth: Cell<bool>,
    auth_delay: Cell<u64>,
    auth_send_failures: Cell<usize>,
    auth_attempts: RefCell<Vec<u64>>,
    refuse_relay: Cell<bool>,
    closed: Cell<bool>,
    close_error: Cell<bool>,
    close_frame: RefCell<Option<SocketFrame>>,
    close_requests: RefCell<Vec<(Option<u16>, String)>>,
    generation: Cell<u64>,
    buffered_bytes: Cell<u64>,
    socket_not_ready: Cell<bool>,
}
impl FakeVenue {
    fn enqueue(&self, value: Value) {
        self.frames
            .borrow_mut()
            .push_back((self.now.get(), Ok(SocketFrame::Text(value.to_string()))));
    }
}
impl TunnelSocket for FakeVenue {
    fn send(&self, frame: SocketFrame) -> Result<(), TransportError> {
        if self.closed.get() {
            return Err(TransportError::Host("closed".into()));
        }
        self.sent.borrow_mut().push(frame.clone());
        if let SocketFrame::Text(text) = frame {
            let Ok(value) = serde_json::from_str::<Value>(&text) else {
                return Ok(());
            };
            match value["type"].as_str() {
                Some("rpc.auth") => {
                    self.auth_attempts.borrow_mut().push(self.now.get());
                    if self.auth_send_failures.get() > 0 {
                        self.auth_send_failures
                            .set(self.auth_send_failures.get() - 1);
                        return Err(TransportError::Host("WebSocketNotReadyError".into()));
                    }
                    if self.fail_auth.get() {
                        self.enqueue(json!({"type":"rpc.auth", "status":"error", "error":"ExpiredSignature"}));
                    } else {
                        self.enqueue(json!({"type":"rpc.auth", "status":"success"}));
                        self.frames.borrow_mut().back_mut().unwrap().0 += self.auth_delay.get();
                    }
                }
                Some("rpc.relay") => {
                    if self.refuse_relay.get() {
                        self.enqueue(
                            json!({"type":"rpc.relay", "status":"error", "error":"disabled"}),
                        );
                    } else {
                        self.enqueue(json!({"type":"rpc.relay", "status":"open"}));
                    }
                }
                Some("rpc.ping") if self.fail_next_ping.replace(false) => {
                    return Err(TransportError::Host("ping write failed".into()));
                }
                Some("rpc.latency") => {
                    self.latency_queries.borrow_mut().push(value.clone());
                    if !self.lost_latency_response.get() {
                        self.enqueue(json!({"type":"rpc.latency", "domain":value["domain"], "latency_ms":40, "source":"measured", "cached":false, "sample_count":3, "measured_at":777}));
                    }
                }
                Some("rpc.http") => {
                    self.requests.borrow_mut().push(value.clone());
                    if !self.lost_response.get() {
                        if let Some(body) = self.response_data_json.borrow().as_ref() {
                            let nonce = serde_json::to_string(&value["nonce"]).unwrap();
                            let frame = format!(
                                r#"{{"type":"rpc.http","nonce":{nonce},"ignored":"\ud800","data":{{"config":{{"url":{url}}},"status":200,"status_text":"OK","headers":{{}},"data":{body}}}}}"#,
                                url = serde_json::to_string(&value["arg"]["url"]).unwrap()
                            );
                            self.frames
                                .borrow_mut()
                                .push_back((self.now.get(), Ok(SocketFrame::Text(frame))));
                            return Ok(());
                        }
                        self.enqueue(json!({"type":"rpc.http", "nonce":value["nonce"], "data": {
                            "config":{"url":value["arg"]["url"]}, "status":403, "status_text":"Forbidden",
                            "headers":{"X-Quota":"12"}, "data":self.response_data.borrow().clone().unwrap_or_else(|| json!({"accepted":value["arg"]["url"]})),
                            "data_raw": if value["format"] == "raw" { Some(self.raw_response_body.borrow().clone().unwrap_or_else(|| "{\"id\":9007199254740993}".into())) } else { None }
                        }}));
                        if self.reorder_responses.get() {
                            let delay = if self.requests.borrow().len() == 1 {
                                100
                            } else {
                                25
                            };
                            self.frames.borrow_mut().back_mut().unwrap().0 += delay;
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
    fn receive(&self) -> TransportFuture<'static, SocketFrame> {
        unreachable!("only FakeSocket receives venue frames")
    }
    fn close(&self, code: Option<u16>, reason: &str) -> Result<(), TransportError> {
        self.close_requests.borrow_mut().push((code, reason.into()));
        if self.close_error.get() {
            return Err(TransportError::Host("inner.close threw".into()));
        }
        self.closed.set(true);
        if let Some(frame) = self.close_frame.borrow_mut().take() {
            self.frames
                .borrow_mut()
                .push_back((self.now.get(), Ok(frame)));
        }
        Ok(())
    }
}
struct FakeSocket {
    venue: Rc<FakeVenue>,
    generation: u64,
}
impl TunnelSocket for FakeSocket {
    fn ready_state(&self) -> u16 {
        if self.venue.socket_not_ready.get() {
            3
        } else {
            1
        }
    }

    fn buffered_amount(&self) -> u64 {
        self.venue.buffered_bytes.get()
    }
    fn send(&self, frame: SocketFrame) -> Result<(), TransportError> {
        if self.generation != self.venue.generation.get() {
            return Err(TransportError::Host("closed".into()));
        }
        self.venue.send(frame)
    }
    fn receive(&self) -> TransportFuture<'static, SocketFrame> {
        let venue = self.venue.clone();
        let generation = self.generation;
        Box::pin(poll_fn(move |_| {
            if generation != venue.generation.get() {
                return Poll::Ready(Err(TransportError::Host("closed".into())));
            }
            let mut frames = venue.frames.borrow_mut();
            if let Some(index) = frames.iter().position(|(at, _)| *at <= venue.now.get()) {
                Poll::Ready(frames.remove(index).unwrap().1)
            } else if venue.closed.get() {
                Poll::Ready(Err(TransportError::Host("closed".into())))
            } else {
                Poll::Pending
            }
        }))
    }
    fn close(&self, code: Option<u16>, reason: &str) -> Result<(), TransportError> {
        if self.generation == self.venue.generation.get() {
            self.venue.close(code, reason)
        } else {
            Ok(())
        }
    }
}

struct FakeHost {
    venue: Rc<FakeVenue>,
    runtime: Cell<SocketRuntime>,
    header_capable: Cell<bool>,
    token_calls: Cell<usize>,
    token_delay: Cell<u64>,
    token_error: Cell<bool>,
    auto_auth: Cell<bool>,
    dials: RefCell<Vec<(String, TunnelDialOptions)>>,
    sockets: RefCell<Vec<std::rc::Weak<dyn TunnelSocket>>>,
    tasks: RefCell<Vec<FakeScheduled>>,
}
impl FakeHost {
    fn new() -> Rc<Self> {
        Rc::new(Self {
            venue: Rc::new(FakeVenue::default()),
            runtime: Cell::new(SocketRuntime::Native),
            header_capable: Cell::new(true),
            token_calls: Cell::new(0),
            token_delay: Cell::new(0),
            token_error: Cell::new(false),
            auto_auth: Cell::new(false),
            dials: RefCell::new(vec![]),
            sockets: RefCell::new(vec![]),
            tasks: RefCell::new(vec![]),
        })
    }
    fn poll_tasks(&self) {
        let tasks = std::mem::take(&mut *self.tasks.borrow_mut());
        let mut context = Context::from_waker(Waker::noop());
        let mut keep = vec![];
        for mut task in tasks {
            if !task.cancelled.get() && task.future.as_mut().poll(&mut context).is_pending() {
                keep.push(task);
            }
        }
        self.tasks.borrow_mut().extend(keep);
    }
    fn advance(&self, milliseconds: u64) {
        for _ in 0..milliseconds {
            self.venue.now.set(self.venue.now.get() + 1);
            self.poll_tasks();
        }
    }
    fn run<T>(&self, future: impl Future<Output = T>) -> T {
        let mut future = pin!(future);
        let mut context = Context::from_waker(Waker::noop());
        for _ in 0..200_000 {
            if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
                return value;
            }
            self.poll_tasks();
            self.venue.now.set(self.venue.now.get() + 1);
        }
        panic!("test executor exhausted");
    }
}
struct FakeScheduled {
    cancelled: Rc<Cell<bool>>,
    future: Pin<Box<dyn Future<Output = ()>>>,
}
struct FakeTask(Rc<Cell<bool>>);
impl TunnelTask for FakeTask {
    fn cancel(&self) {
        self.0.set(true);
    }
}
impl TunnelHost for FakeHost {
    fn spawn_local(&self, future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn TunnelTask> {
        let cancelled = Rc::new(Cell::new(false));
        self.tasks.borrow_mut().push(FakeScheduled {
            cancelled: cancelled.clone(),
            future,
        });
        Box::new(FakeTask(cancelled))
    }
    fn capabilities(&self) -> SocketCapabilities {
        SocketCapabilities {
            runtime: self.runtime.get(),
            upgrade_headers: self.header_capable.get(),
            alpn: self.header_capable.get(),
        }
    }
    fn now_ms(&self) -> u64 {
        self.venue.now.get()
    }
    fn sleep_until(&self, deadline_ms: u64) -> TransportFuture<'static, ()> {
        let now = self.venue.now.clone();
        Box::pin(poll_fn(move |_| {
            if now.get() >= deadline_ms {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        }))
    }
    fn connect(
        &self,
        url: String,
        options: TunnelDialOptions,
    ) -> TransportFuture<'static, Rc<dyn TunnelSocket>> {
        self.dials.borrow_mut().push((url, options));
        self.venue.closed.set(false);
        self.venue.generation.set(self.venue.generation.get() + 1);
        if self.auto_auth.get() {
            self.venue
                .enqueue(json!({"type":"rpc.auth","status":"success"}));
        }
        self.venue
            .enqueue(json!({"type":"serverTime","serverTime":1000}));
        let socket = Rc::new(FakeSocket {
            venue: self.venue.clone(),
            generation: self.venue.generation.get(),
        }) as Rc<dyn TunnelSocket>;
        self.sockets.borrow_mut().push(Rc::downgrade(&socket));
        Box::pin(async move { Ok(socket) })
    }
    fn proxy_token(&self) -> TransportFuture<'_, String> {
        self.token_calls.set(self.token_calls.get() + 1);
        Box::pin(async move {
            self.sleep_until(self.now_ms() + self.token_delay.get())
                .await?;
            if self.token_error.get() {
                Err(TransportError::Host("proxy token unavailable".into()))
            } else {
                Ok("jwt".into())
            }
        })
    }
    fn is_token_expired(&self, _token: &str) -> bool {
        false
    }
    fn sync_server_time(&self, server_time: u64) {
        assert_eq!(server_time, 1000);
    }
}
fn request(url: &str) -> RestRequest {
    RestRequest {
        method: "post".into(),
        url: url.into(),
        base_url: Some("https://api.bybit.com".into()),
        body: Some(safe_cex_transport::RestBody::Encoded(
            br#"{"qty":"1","tag":"Tealstreet"}"#.to_vec(),
        )),
        headers: indexmap::IndexMap::from([("X-Referer".into(), "broker".into())]),
        timeout_ms: Some(1000),
        params: Default::default(),
        ..Default::default()
    }
}

fn timed_request(url: &str, timeout_ms: i64) -> RestRequest {
    RestRequest {
        timeout_ms: Some(timeout_ms),
        ..request(url)
    }
}

#[test]
fn how_routes_and_preserves_signed_payload_and_resolves_non_2xx() {
    let host = FakeHost::new();
    let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
    let response = host
        .run(leaf_error(
            adapter.execute(timed_request("/order/create", 1000)),
        ))
        .unwrap();
    assert_eq!(response.status, 403);
    assert_eq!(response.reason, None);
    assert_eq!(response.headers["X-Quota"], "12");
    let requests = host.venue.requests.borrow();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["rpc_version"], 4);
    assert_eq!(requests[0]["arg"]["headers"]["X-Referer"], "broker");
    assert_eq!(requests[0]["arg"]["data"]["qty"], "1");
    assert!(host.dials.borrow()[0].0.contains(".asia.tealstreet.io"));
    let options = &host.dials.borrow()[0].1;
    assert_eq!(options.headers()["Origin"], "https://preview.tealstreet.io");
    assert!(options.headers()["User-Agent"].contains("Mozilla/5.0"));
    assert_eq!(options.alpn_protocols(), ["http/1.1"]);
}

#[test]
fn how_reuses_auth_and_honors_whitelisted_ip_without_token() {
    let host = FakeHost::new();
    host.auto_auth.set(true);
    host.token_error.set(true);
    let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
    for _ in 0..3 {
        host.run(leaf_error(
            adapter.execute(timed_request("/order/create", 1000)),
        ))
        .unwrap();
    }
    assert_eq!(host.token_calls.get(), 0);
    assert_eq!(host.dials.borrow().len(), 1);
    assert_eq!(host.venue.requests.borrow().len(), 3);
}

#[test]
fn how_complete_deadline_prevents_stale_send_and_never_replays_lost_write() {
    let host = FakeHost::new();
    host.token_delay.set(2000);
    let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
    assert_eq!(
        host.run(leaf_error(
            adapter.execute(timed_request("/order/create", 100))
        )),
        Err(TransportError::Timeout)
    );
    assert!(host.venue.requests.borrow().is_empty());
    host.token_delay.set(0);
    host.venue.lost_response.set(true);
    assert_eq!(
        host.run(leaf_error(
            adapter.execute(timed_request("/order/create", 100))
        )),
        Err(TransportError::Timeout)
    );
    assert_eq!(host.venue.requests.borrow().len(), 1);
}

#[test]
fn how_raw_mode_retains_large_integer_bytes() {
    let host = FakeHost::new();
    let adapter = HowRestAdapter::new(host.clone(), None, true).unwrap();
    let result = host
        .run(leaf_error(
            adapter.execute(timed_request("/order/create", 1000)),
        ))
        .unwrap();
    assert_eq!(
        result.body,
        safe_cex_transport::RestResponseBody::Encoded(br#"{"id":9007199254740993}"#.to_vec())
    );
    assert_eq!(host.venue.requests.borrow()[0]["rpc_version"], 4);
    assert_eq!(host.venue.requests.borrow()[0]["accept_binary"], true);
    assert_eq!(
        host.venue.requests.borrow()[0]["accept_encoding"],
        json!(["gzip"])
    );
}

#[test]
fn relay_only_opens_after_auth_and_relay_ack_and_then_pipes_binary_verbatim() {
    let host = FakeHost::new();
    let relay = RelayWebSocket::new(
        host.clone(),
        "wss://tunnel/tunnel?ts-relay=wss%3A%2F%2Fvenue%2Fprivate",
    )
    .unwrap();
    for number in 0..102 {
        relay
            .send(SocketFrame::Text(format!("queued-{number}")))
            .unwrap();
    }
    assert_eq!(host.run(relay.next_event()).unwrap(), RelayEvent::Open);
    let sent = host.venue.sent.borrow();
    assert_eq!(
        serde_json::from_str::<Value>(match &sent[0] {
            SocketFrame::Text(s) => s,
            _ => panic!(),
        })
        .unwrap()["type"],
        "rpc.auth"
    );
    assert_eq!(
        serde_json::from_str::<Value>(match &sent[1] {
            SocketFrame::Text(s) => s,
            _ => panic!(),
        })
        .unwrap()["upstream"],
        "wss://venue/private"
    );
    assert_eq!(sent[2], SocketFrame::Text("queued-2".into()));
    assert_eq!(sent.len(), 102);
    drop(sent);
    let bytes = SocketFrame::Binary(vec![0, 255, 12]);
    relay.on_inner_message(bytes.clone()).unwrap();
    assert_eq!(
        host.run(relay.next_event()).unwrap(),
        RelayEvent::Message(bytes.clone())
    );
    relay.send(bytes.clone()).unwrap();
    assert_eq!(host.venue.sent.borrow().last(), Some(&bytes));
}

#[test]
fn relay_auth_rejection_refusal_and_token_timeout_are_silent_errors_and_close() {
    for failure in 0..3 {
        let host = FakeHost::new();
        match failure {
            0 => host.venue.fail_auth.set(true),
            1 => host.venue.refuse_relay.set(true),
            _ => host.token_delay.set(20_000),
        }
        let relay = RelayWebSocket::new(
            host.clone(),
            "wss://tunnel/tunnel?ts-relay=wss%3A%2F%2Fvenue",
        )
        .unwrap();
        relay
            .send(SocketFrame::Text("auth must not escape".into()))
            .unwrap();
        assert!(matches!(
            host.run(relay.next_event()).unwrap(),
            RelayEvent::Error(_)
        ));
        assert!(matches!(
            host.run(relay.next_event()).unwrap(),
            RelayEvent::Close {
                code: 4000,
                was_clean: false,
                ..
            }
        ));
        assert!(host.run(relay.next_event()).is_err());
        assert!(
            !host
                .venue
                .sent
                .borrow()
                .contains(&SocketFrame::Text("auth must not escape".into()))
        );
    }
}

#[test]
fn cancelled_relay_token_fetch_closes_silently_instead_of_wedging_auth() {
    let host = FakeHost::new();
    host.token_delay.set(10_000);
    let relay = RelayWebSocket::new(
        host.clone(),
        "wss://tunnel/tunnel?ts-relay=wss%3A%2F%2Fvenue",
    )
    .unwrap();
    {
        let mut future = pin!(relay.next_event());
        let mut context = Context::from_waker(Waker::noop());
        assert!(future.as_mut().poll(&mut context).is_pending());
    }
    assert!(host.venue.closed.get());
    assert!(matches!(
        host.run(relay.next_event()).unwrap(),
        RelayEvent::Error(_)
    ));
    assert!(matches!(
        host.run(relay.next_event()).unwrap(),
        RelayEvent::Close { .. }
    ));
    assert!(host.venue.sent.borrow().is_empty());
}

#[test]
fn how_converts_signed_form_bodies_like_typescript() {
    let host = FakeHost::new();
    let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
    let mut req = request("/order/create");
    req.body = Some(safe_cex_transport::RestBody::Encoded(
        b"symbol=BTC%2FUSD&tag=Tealstreet+Trader&qty=1".to_vec(),
    ));
    host.run(leaf_error(adapter.execute(req))).unwrap();
    assert_eq!(
        host.venue.requests.borrow()[0]["arg"]["data"],
        json!({"symbol":"BTC/USD","tag":"Tealstreet Trader","qty":"1"})
    );
}

#[test]
fn adapters_share_live_regional_sockets_and_release_distinct_hosts() {
    let host = FakeHost::new();
    let first = HowRestAdapter::new(host.clone(), None, false).unwrap();
    let second = HowRestAdapter::new(host.clone(), None, true).unwrap();
    host.run(leaf_error(first.execute(timed_request("/first", 1000))))
        .unwrap();
    host.run(leaf_error(second.execute(timed_request("/second", 1000))))
        .unwrap();
    assert_eq!(host.dials.borrow().len(), 1);
    assert_eq!(host.token_calls.get(), 1);
    let other = FakeHost::new();
    let isolated = HowRestAdapter::new(other.clone(), None, false).unwrap();
    other
        .run(leaf_error(isolated.execute(timed_request("/other", 1000))))
        .unwrap();
    assert_eq!(other.dials.borrow().len(), 1);
    assert_eq!(other.token_calls.get(), 1);
    let weak_other = Rc::downgrade(&other);
    drop(isolated);
    drop(other);
    assert!(weak_other.upgrade().is_none());
}

#[test]
fn idle_ping_pong_timeout_reconnects_and_disposal_cancels_all_timers() {
    use safe_cex_transport::how::{dispose_how_websockets, get_how_websocket_for_exchange};
    let host = FakeHost::new();
    let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
    host.run(leaf_error(adapter.execute(timed_request("/first", 1000))))
        .unwrap();
    host.advance(5_000);
    let sent = host.venue.sent.borrow();
    assert!(
        sent.iter()
            .any(|frame| matches!(frame, SocketFrame::Text(text) if text.contains("rpc.ping")))
    );
    drop(sent);
    let client = get_how_websocket_for_exchange(host.clone(), "bybit").unwrap();
    client
        .on_message(SocketFrame::Text(
            json!({"type":"rpc.ping","status":"success"}).to_string(),
        ))
        .unwrap();
    host.advance(26_100);
    assert!(
        host.dials.borrow().len() >= 2,
        "missing pong must reconnect even without requests"
    );
    let count = host.venue.sent.borrow().len();
    dispose_how_websockets();
    assert!(host.venue.closed.get());
    host.advance(10_000);
    assert_eq!(host.venue.sent.borrow().len(), count);
    assert!(host.tasks.borrow().is_empty());
}

#[test]
fn monitor_backoff_does_not_replace_a_connection_opened_by_a_request() {
    use safe_cex_transport::how::get_how_websocket_for_exchange;
    let host = FakeHost::new();
    let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
    host.run(leaf_error(adapter.execute(timed_request("/first", 1000))))
        .unwrap();
    let client = get_how_websocket_for_exchange(host.clone(), "bybit").unwrap();
    client.on_close();
    host.poll_tasks(); // Monitor starts its reconnect backoff.
    host.run(leaf_error(
        adapter.execute(timed_request("/reconnect", 1000)),
    ))
    .unwrap();
    assert_eq!(host.dials.borrow().len(), 2);
    host.advance(1_100);
    assert_eq!(host.dials.borrow().len(), 2);
    host.run(leaf_error(
        adapter.execute(timed_request("/still-authenticated", 1000)),
    ))
    .unwrap();
    assert_eq!(host.token_calls.get(), 2);
}

#[test]
fn negotiated_base64_binary_and_corrupt_gzip_complete_the_actual_adapter() {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let fixture = "H4sIAAAAAAAEA6tWyi9KSS3yTFGysjQwMDe0tDQyNTE3MbC0NK4FAERmNdIcAAAA";
    for mode in 0..3 {
        let host = FakeHost::new();
        host.venue.lost_response.set(true);
        let adapter = HowRestAdapter::new(host.clone(), None, true).unwrap();
        let mut future = leaf_error(adapter.execute(timed_request("/test", 1000)));
        let mut context = Context::from_waker(Waker::noop());
        assert!(future.as_mut().poll(&mut context).is_pending());
        let nonce = host.venue.requests.borrow()[0]["nonce"].clone();
        let mut header = json!({"type":"rpc.http","nonce":nonce,"data":{
            "config":{"url":"/test"},"status":200,"status_text":"OK","headers":{},
            "data_encoding":"gzip+base64","data_encoded":if mode==2 { "bm90LWd6aXA=" } else { fixture }
        }});
        let frame = if mode == 1 {
            header["data"]["data_encoding"] = json!("gzip");
            header["data"]
                .as_object_mut()
                .unwrap()
                .remove("data_encoded");
            let bytes = header.to_string().into_bytes();
            let mut frame = b"TSB1".to_vec();
            frame.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
            frame.extend(bytes);
            frame.extend(STANDARD.decode(fixture).unwrap());
            SocketFrame::Binary(frame)
        } else {
            SocketFrame::Text(header.to_string())
        };
        host.venue
            .frames
            .borrow_mut()
            .push_back((host.now_ms(), Ok(frame)));
        let result = host.run(future);
        if mode == 2 {
            assert!(
                matches!(result,Err(TransportError::Host(message)) if message.contains("TradeTunnelResponseDecodeError"))
            );
        } else {
            assert_eq!(
                result.unwrap().body,
                safe_cex_transport::RestResponseBody::Encoded(
                    br#"{"orderId":9007199254740993}"#.to_vec()
                )
            );
        }
        assert_eq!(host.venue.requests.borrow().len(), 1);
    }
}

#[test]
fn concurrent_adapters_share_auth_and_demultiplex_reordered_raw_and_parsed_responses() {
    let host = FakeHost::new();
    host.venue.reorder_responses.set(true);
    let normal = HowRestAdapter::new(host.clone(), None, false).unwrap();
    let raw = HowRestAdapter::new(host.clone(), None, true).unwrap();
    let mut first = leaf_error(normal.execute(timed_request("/first", 1000)));
    let mut second = leaf_error(raw.execute(timed_request("/second", 1000)));
    let mut first_result = None;
    let mut second_result = None;
    host.run(poll_fn(|cx| {
        if first_result.is_none()
            && let Poll::Ready(result) = first.as_mut().poll(cx)
        {
            first_result = Some(result);
        }
        if second_result.is_none()
            && let Poll::Ready(result) = second.as_mut().poll(cx)
        {
            second_result = Some(result);
        }
        if first_result.is_some() && second_result.is_some() {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }));
    let parsed: Value = first_result.unwrap().unwrap().body.json_value().unwrap();
    assert_eq!(parsed["accepted"], "https://api.bybit.com/first");
    assert_eq!(
        second_result.unwrap().unwrap().body,
        safe_cex_transport::RestResponseBody::Encoded(br#"{"id":9007199254740993}"#.to_vec())
    );
    assert_eq!(host.token_calls.get(), 1);
    assert_eq!(host.dials.borrow().len(), 1);
    assert_eq!(host.venue.requests.borrow().len(), 2);
}

#[test]
fn relay_remote_close_preserves_metadata_before_and_after_open() {
    for opened in [false, true] {
        let host = FakeHost::new();
        let relay =
            RelayWebSocket::new(host.clone(), "wss://tunnel/?ts-relay=wss%3A%2F%2Fvenue").unwrap();
        host.run(relay.on_inner_open()).unwrap();
        if opened {
            relay
                .on_inner_message(SocketFrame::Text(
                    r#"{"type":"rpc.auth","status":"success"}"#.into(),
                ))
                .unwrap();
            relay
                .on_inner_message(SocketFrame::Text(
                    r#"{"type":"rpc.relay","status":"open"}"#.into(),
                ))
                .unwrap();
            assert_eq!(host.run(relay.next_event()).unwrap(), RelayEvent::Open);
        }
        relay
            .on_inner_message(SocketFrame::Close {
                code: 1000,
                reason: "upstream finished".into(),
                was_clean: true,
            })
            .unwrap();
        assert_eq!(
            host.run(relay.next_event()).unwrap(),
            RelayEvent::Close {
                code: 1000,
                reason: "upstream finished".into(),
                was_clean: true
            }
        );
        relay.on_inner_close(1006, "duplicate".into(), false);
        assert!(host.run(relay.next_event()).is_err());
    }
}

#[test]
fn prepare_timeout_and_cancellation_do_not_disconnect_another_pending_request() {
    for cancel in [false, true] {
        let host = FakeHost::new();
        let normal = HowRestAdapter::new(host.clone(), None, false).unwrap();
        host.run(leaf_error(normal.execute(timed_request("/warm", 2000))))
            .unwrap();
        host.venue.lost_response.set(true);
        let mut pending = leaf_error(normal.execute(timed_request("/pending", 2000)));
        let mut context = Context::from_waker(Waker::noop());
        assert!(pending.as_mut().poll(&mut context).is_pending());
        let nonce = host.venue.requests.borrow().last().unwrap()["nonce"].clone();
        let client =
            safe_cex_transport::how::get_how_websocket_for_exchange(host.clone(), "bybit").unwrap();
        client
            .on_message(SocketFrame::Text(
                json!({"type":"rpc.auth","error":"expired"}).to_string(),
            ))
            .unwrap();
        host.token_delay.set(1000);
        let short = HowRestAdapter::new(host.clone(), None, false).unwrap();
        let mut preparation = leaf_error(short.execute(timed_request("/short", 600)));
        if cancel {
            assert!(preparation.as_mut().poll(&mut context).is_pending());
            drop(preparation);
        } else {
            assert!(matches!(
                host.run(preparation),
                Err(TransportError::Timeout)
            ));
        }
        assert!(!host.venue.closed.get());
        assert_eq!(host.dials.borrow().len(), 1);
        host.venue
            .enqueue(json!({"type":"rpc.http","nonce":nonce,"data":{
                "config":{"url":"https://api.bybit.com/pending"},"status":200,"status_text":"OK","headers":{},"data":{"survived":true}
            }}));
        let response = host.run(pending).unwrap();
        assert_eq!(response.body.json_value().unwrap()["survived"], true);
    }
}

#[test]
fn cancelled_auth_wait_does_not_resend_a_token_on_the_same_socket() {
    let host = FakeHost::new();
    host.venue.auth_delay.set(100);
    let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
    let mut first = leaf_error(adapter.execute(timed_request("/cancelled", 1000)));
    let mut context = Context::from_waker(Waker::noop());
    assert!(first.as_mut().poll(&mut context).is_pending());
    assert_eq!(host.venue.auth_attempts.borrow().len(), 1);
    drop(first);
    host.run(leaf_error(
        adapter.execute(timed_request("/survivor", 1000)),
    ))
    .unwrap();
    assert_eq!(host.venue.auth_attempts.borrow().len(), 1);
    assert_eq!(host.dials.borrow().len(), 1);
}

#[test]
fn token_send_retries_not_ready_five_times_at_500ms_and_stops_at_the_deadline() {
    for failures in [5, 6] {
        let host = FakeHost::new();
        host.venue.auth_send_failures.set(failures);
        let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
        let result = host.run(leaf_error(adapter.execute(timed_request("/retry", 4000))));
        assert_eq!(result.is_ok(), failures == 5);
        let attempts = host.venue.auth_attempts.borrow();
        assert_eq!(attempts.len(), 6);
        assert!(attempts.windows(2).all(|pair| pair[1] - pair[0] == 500));
        assert!(!host.venue.closed.get());
    }
    let host = FakeHost::new();
    host.venue.auth_send_failures.set(6);
    let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
    assert!(matches!(
        host.run(leaf_error(adapter.execute(timed_request("/timeout", 750)))),
        Err(TransportError::Timeout)
    ));
    assert_eq!(host.venue.auth_attempts.borrow().len(), 2);
    assert!(host.venue.requests.borrow().is_empty());
    assert!(!host.venue.closed.get());
}

#[test]
fn a_joined_preparation_keeps_its_own_deadline_after_the_owner_times_out() {
    let host = FakeHost::new();
    host.token_delay.set(100);
    let short = HowRestAdapter::new(host.clone(), None, false).unwrap();
    let long = HowRestAdapter::new(host.clone(), None, false).unwrap();
    let mut owner = leaf_error(short.execute(timed_request("/short", 50)));
    let mut joiner = leaf_error(long.execute(timed_request("/long", 1000)));
    let mut owner_result = None;
    let mut joiner_result = None;
    host.run(poll_fn(|cx| {
        if owner_result.is_none()
            && let Poll::Ready(result) = owner.as_mut().poll(cx)
        {
            owner_result = Some(result);
        }
        if joiner_result.is_none()
            && let Poll::Ready(result) = joiner.as_mut().poll(cx)
        {
            joiner_result = Some(result);
        }
        if owner_result.is_some() && joiner_result.is_some() {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }));
    assert!(matches!(
        owner_result.unwrap(),
        Err(TransportError::Timeout)
    ));
    assert!(joiner_result.unwrap().is_ok());
    assert_eq!(host.dials.borrow().len(), 1);
    assert_eq!(host.venue.requests.borrow().len(), 1);
}

#[test]
fn raw_string_truthiness_precedes_json_parsing_and_preserves_falsy_values() {
    for base in ["https://api.bybit.com", "https://trades.grvt.io"] {
        for (body, expected) in [
            ("0", json!(0)),
            ("false", json!(false)),
            ("null", Value::Null),
            ("\"\"", json!("")),
            (
                "",
                if base.contains("grvt") {
                    json!("")
                } else {
                    json!({})
                },
            ),
        ] {
            let host = FakeHost::new();
            let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
            let mut call = request("/body");
            call.base_url = Some(base.into());
            call.body = Some(safe_cex_transport::RestBody::Encoded(
                body.as_bytes().to_vec(),
            ));
            host.run(leaf_error(adapter.execute(call))).unwrap();
            assert_eq!(
                host.venue.requests.borrow()[0]["arg"]["data"],
                expected,
                "{base}: {body}"
            );
        }
    }
}

#[test]
fn ordinary_request_numbers_round_like_js_and_raw_adapter_protects_bigints() {
    for raw in [false, true] {
        let host = FakeHost::new();
        let adapter = HowRestAdapter::new(host.clone(), None, raw).unwrap();
        let mut call = request("/number");
        call.body = Some(safe_cex_transport::RestBody::Encoded(
            br#"{"id":9007199254740993,"nested":[-9007199254740993,0.1]}"#.to_vec(),
        ));
        host.run(leaf_error(adapter.execute(call))).unwrap();
        let data = host.venue.requests.borrow()[0]["arg"]["data"].clone();
        if raw {
            assert_eq!(data["id"], "9007199254740993");
            assert_eq!(data["nested"][0], "-9007199254740993");
        } else {
            assert_eq!(data["id"], json!(9007199254740992_u64));
            assert_eq!(data["nested"][0], json!(-9007199254740992_i64));
            let sent = host.venue.sent.borrow();
            assert!(
                matches!(sent.last(), Some(SocketFrame::Text(text)) if text.contains("9007199254740992") && !text.contains("9007199254740993"))
            );
        }
    }
}

#[test]
fn local_relay_close_waits_for_inner_metadata_and_only_close_throw_fabricates_1006() {
    for throws in [false, true] {
        let host = FakeHost::new();
        let relay =
            RelayWebSocket::new(host.clone(), "wss://tunnel/?ts-relay=wss%3A%2F%2Fvenue").unwrap();
        assert_eq!(host.run(relay.next_event()).unwrap(), RelayEvent::Open);
        host.venue.close_error.set(throws);
        relay.close(3001, "local request");
        assert_eq!(
            host.venue.close_requests.borrow().as_slice(),
            &[(Some(3001), "local request".into())]
        );
        if throws {
            assert_eq!(
                host.run(relay.next_event()).unwrap(),
                RelayEvent::Close {
                    code: 1006,
                    reason: "close failed".into(),
                    was_clean: false
                }
            );
        } else {
            host.venue.frames.borrow_mut().push_back((
                host.now_ms(),
                Ok(SocketFrame::Close {
                    code: 1001,
                    reason: "remote going away".into(),
                    was_clean: false,
                }),
            ));
            assert_eq!(
                host.run(relay.next_event()).unwrap(),
                RelayEvent::Close {
                    code: 1001,
                    reason: "remote going away".into(),
                    was_clean: false
                }
            );
        }
        relay.on_inner_close(1000, "duplicate".into(), true);
        assert!(host.run(relay.next_event()).is_err());
    }
}

#[test]
fn relay_receive_error_dispatches_error_and_preserves_the_later_remote_close() {
    for opened in [false, true] {
        let host = FakeHost::new();
        let relay =
            RelayWebSocket::new(host.clone(), "wss://tunnel/?ts-relay=wss%3A%2F%2Fvenue").unwrap();
        if opened {
            assert_eq!(host.run(relay.next_event()).unwrap(), RelayEvent::Open);
        } else {
            host.run(relay.on_inner_open()).unwrap();
            host.venue.frames.borrow_mut().clear();
        }
        host.venue.frames.borrow_mut().push_back((
            host.now_ms(),
            Err(TransportError::Host("socket error".into())),
        ));
        assert_eq!(
            host.run(relay.next_event()).unwrap(),
            RelayEvent::Error("relay transport error".into())
        );
        assert!(host.venue.close_requests.borrow().is_empty());
        host.venue.frames.borrow_mut().push_back((
            host.now_ms(),
            Ok(SocketFrame::Close {
                code: 1011,
                reason: "upstream failure".into(),
                was_clean: false,
            }),
        ));
        assert_eq!(
            host.run(relay.next_event()).unwrap(),
            RelayEvent::Close {
                code: 1011,
                reason: "upstream failure".into(),
                was_clean: false
            }
        );
    }
}

#[test]
fn raw_request_parser_protects_large_token_lexemes_and_returns_null_for_non_json() {
    let host = FakeHost::new();
    let adapter = HowRestAdapter::new(host.clone(), None, true).unwrap();
    for (body, expected) in [
        (
            r#"{"id":123456789012345678901234567890,"items":[-123456789012345678901234567890]}"#,
            json!({"id":"123456789012345678901234567890","items":["-123456789012345678901234567890"]}),
        ),
        ("a=1&b=2", Value::Null),
        ("not json", Value::Null),
    ] {
        let mut call = request("/raw");
        call.body = Some(safe_cex_transport::RestBody::Encoded(
            body.as_bytes().to_vec(),
        ));
        host.run(leaf_error(adapter.execute(call))).unwrap();
        assert_eq!(
            host.venue.requests.borrow().last().unwrap()["arg"]["data"],
            expected
        );
    }
}

#[test]
fn relay_close_before_first_poll_reaches_the_host_close_event() {
    for throws in [false, true] {
        let host = FakeHost::new();
        let relay =
            RelayWebSocket::new(host.clone(), "wss://tunnel/?ts-relay=wss%3A%2F%2Fvenue").unwrap();
        host.venue.close_error.set(throws);
        *host.venue.close_frame.borrow_mut() = Some(SocketFrame::Close {
            code: 1001,
            reason: "aborted connect".into(),
            was_clean: false,
        });
        relay.close(3000, "cancel before poll");
        let event = host.run(relay.next_event()).unwrap();
        assert_eq!(
            event,
            if throws {
                RelayEvent::Close {
                    code: 1006,
                    reason: "close failed".into(),
                    was_clean: false,
                }
            } else {
                RelayEvent::Close {
                    code: 1001,
                    reason: "aborted connect".into(),
                    was_clean: false,
                }
            }
        );
        assert_eq!(
            host.venue.close_requests.borrow().as_slice(),
            &[(Some(3000), "cancel before poll".into())]
        );
        assert!(host.venue.sent.borrow().is_empty());
    }
}

#[test]
fn one_adapter_uses_each_requests_deadline_and_nonpositive_defaults_to_20_seconds() {
    let host = FakeHost::new();
    let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
    host.run(leaf_error(adapter.execute(timed_request("/warm", 1000))))
        .unwrap();
    host.venue.lost_response.set(true);
    let mut short = leaf_error(adapter.execute(timed_request("/short", 10)));
    let mut long = leaf_error(adapter.execute(timed_request("/long", 100)));
    let mut context = Context::from_waker(Waker::noop());
    assert!(short.as_mut().poll(&mut context).is_pending());
    assert!(long.as_mut().poll(&mut context).is_pending());
    host.advance(10);
    assert!(matches!(
        short.as_mut().poll(&mut context),
        Poll::Ready(Err(TransportError::Timeout))
    ));
    assert!(long.as_mut().poll(&mut context).is_pending());
    assert!(!host.venue.closed.get());
    host.advance(90);
    assert!(matches!(
        long.as_mut().poll(&mut context),
        Poll::Ready(Err(TransportError::Timeout))
    ));
    for timeout in [None, Some(0), Some(-10)] {
        let before = host.now_ms();
        let mut call = request("/default");
        call.timeout_ms = timeout;
        assert!(matches!(
            host.run(leaf_error(adapter.execute(call))),
            Err(TransportError::Timeout)
        ));
        assert_eq!(host.now_ms() - before, 20_000);
    }
}

#[test]
fn regional_cache_and_pending_monitor_tasks_do_not_keep_hosts_or_clients_alive() {
    for phase in 0..3 {
        let host = FakeHost::new();
        let weak_host = Rc::downgrade(&host);
        let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
        let client =
            safe_cex_transport::how::get_how_websocket_for_exchange(host.clone(), "bybit").unwrap();
        let weak_client = Rc::downgrade(&client);
        if phase > 0 {
            host.run(leaf_error(adapter.execute(request("/warm"))))
                .unwrap();
            if phase == 2 {
                client.on_close();
            }
            host.poll_tasks();
        }
        let cancelled: Vec<_> = host
            .tasks
            .borrow()
            .iter()
            .map(|task| task.cancelled.clone())
            .collect();
        drop(client);
        assert!(weak_client.upgrade().is_some()); // The live adapter still shares this client.
        drop(adapter);
        assert!(weak_client.upgrade().is_none());
        assert_eq!(
            cancelled.len(),
            usize::from(phase > 0),
            "TUNNEL-CARDINALITY pending monitor tasks per phase"
        );
        assert!(cancelled.iter().all(|flag| flag.get()));
        drop(host);
        assert!(weak_host.upgrade().is_none());
    }
}

#[test]
fn typed_how_request_carries_query_params_and_keeps_ts_batch_body_shape() {
    let host = FakeHost::new();
    let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
    let mut call = request("/batch");
    call.body = Some(safe_cex_transport::RestBody::Encoded(
        br#"[{"qty":"1"}]"#.to_vec(),
    ));
    call.params = Some(Value::Object(serde_json::Map::from_iter([
        ("symbol".into(), json!("BTC/USDT")),
        ("ids".into(), json!([1, 2])),
        ("empty".into(), Value::Null),
    ])));
    host.run(leaf_error(adapter.execute(call))).unwrap();
    let calls = host.venue.requests.borrow();
    assert_eq!(calls[0]["arg"]["data"], json!([{"qty":"1"}]));
    assert_eq!(
        calls[0]["arg"]["params"],
        json!({"symbol":"BTC/USDT","ids":[1,2],"empty":null})
    );
}

#[test]
fn emitted_how_frame_preserves_signed_body_header_and_query_insertion_order() {
    const BODY: &str = r#"{"symbol":"BTCUSDT","side":"Buy","orderType":"Limit","qty":"1","category":"linear","nested":{"z":"first","a":"second"}}"#;
    for raw in [false, true] {
        let host = FakeHost::new();
        let adapter = HowRestAdapter::new(host.clone(), None, raw).unwrap();
        let mut call = request("/v5/order/create");
        call.body = Some(safe_cex_transport::RestBody::Encoded(
            BODY.as_bytes().to_vec(),
        ));
        call.headers = indexmap::IndexMap::from([
            ("X-Z".into(), "first".into()),
            ("X-A".into(), "second".into()),
        ]);
        call.params = Some(serde_json::from_str(r#"{"zeta":"first","alpha":"second"}"#).unwrap());
        host.run(leaf_error(adapter.execute(call))).unwrap();
        let sent = host.venue.sent.borrow();
        let Some(SocketFrame::Text(frame)) = sent.last() else {
            panic!("expected HTTP request frame");
        };
        // ts: BybitExchange signs JSON.stringify(nextConfig.data); sorting invalidates its signature.
        assert!(frame.contains(&format!("\"data\":{BODY}")), "{frame}");
        assert!(
            frame.contains(r#""headers":{"X-Z":"first","X-A":"second"}"#),
            "{frame}"
        );
        assert!(
            frame.contains(r#""params":{"zeta":"first","alpha":"second"}"#),
            "{frame}"
        );
    }
}

#[test]
fn emitted_how_frame_uses_javascript_integer_property_order() {
    const BODY: &str = r#"{"2":"b","1":"a","00":"text","4294967295":"text","4294967294":"index","z":{"2":2,"0":0}}"#;
    const EXPECTED: &str = r#"{"1":"a","2":"b","4294967294":"index","00":"text","4294967295":"text","z":{"0":0,"2":2}}"#;
    for raw in [false, true] {
        let host = FakeHost::new();
        let adapter = HowRestAdapter::new(host.clone(), None, raw).unwrap();
        let mut call = request("/order");
        call.body = Some(safe_cex_transport::RestBody::Encoded(
            BODY.as_bytes().to_vec(),
        ));
        call.params = Some(serde_json::from_str(BODY).unwrap());
        host.run(leaf_error(adapter.execute(call))).unwrap();
        let sent = host.venue.sent.borrow();
        let Some(SocketFrame::Text(frame)) = sent.last() else {
            panic!("expected HTTP frame")
        };
        assert!(
            frame.contains(&format!("\"params\":{EXPECTED}")),
            "HOW integer query property order: {frame}"
        );
        assert!(frame.contains(&format!("\"data\":{EXPECTED}")), "{frame}");
    }
}

#[test]
fn ordinary_response_rounds_numbers_while_raw_keeps_original_digits() {
    for raw in [false, true] {
        let host = FakeHost::new();
        *host.venue.response_data.borrow_mut() =
            Some(serde_json::from_str(r#"{"id":9007199254740993}"#).unwrap());
        let adapter = HowRestAdapter::new(host.clone(), None, raw).unwrap();
        let response = host
            .run(leaf_error(adapter.execute(request("/order"))))
            .unwrap();
        let expected = if raw {
            r#"{"id":9007199254740993}"#
        } else {
            r#"{"id":9007199254740992}"#
        };
        if raw {
            assert_eq!(
                response.body,
                safe_cex_transport::RestResponseBody::Encoded(expected.as_bytes().to_vec())
            );
        } else {
            assert_eq!(
                response.body,
                safe_cex_transport::RestResponseBody::Untransformed(
                    Input::parse_json(&expected.encode_utf16().collect::<Vec<_>>()).unwrap()
                )
            );
        }
    }
}

#[test]
fn old_receive_error_cannot_close_the_replacement_socket() {
    use safe_cex_transport::how::get_how_websocket_for_exchange;
    let host = FakeHost::new();
    let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
    host.run(leaf_error(adapter.execute(timed_request("/first", 1000))))
        .unwrap();
    host.poll_tasks(); // Leave the monitor awaiting a receive on the old socket.
    let client = get_how_websocket_for_exchange(host.clone(), "bybit").unwrap();
    client.on_close();
    host.run(leaf_error(
        adapter.execute(timed_request("/replacement", 1000)),
    ))
    .unwrap();
    assert_eq!(host.dials.borrow().len(), 2);
    host.run(leaf_error(
        adapter.execute(timed_request("/still-open", 1000)),
    ))
    .unwrap();
    assert_eq!(host.dials.borrow().len(), 2);
}

#[test]
fn ping_write_failure_reconnects_and_keeps_idle_monitor_running() {
    let host = FakeHost::new();
    let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
    host.run(leaf_error(adapter.execute(timed_request("/first", 1000))))
        .unwrap();
    host.venue.fail_next_ping.set(true);
    host.advance(6_100);
    assert_eq!(
        host.dials.borrow().len(),
        2,
        "failed ping must reconnect without a request"
    );
    let sent = host.venue.sent.borrow().len();
    host.advance(5_000);
    assert!(
        host.venue.sent.borrow()[sent..]
            .iter()
            .any(|frame| matches!(frame, SocketFrame::Text(text) if text.contains("rpc.ping")))
    );
    assert!(!host.tasks.borrow().is_empty());
}

#[test]
fn grvt_frame_bypasses_proxy_but_other_venues_do_not() {
    for (base, expected) in [
        ("https://trades.grvt.io", Some(true)),
        ("https://api.bybit.com", None),
    ] {
        let host = FakeHost::new();
        let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
        let mut call = request("/order");
        call.base_url = Some(base.into());
        host.run(leaf_error(adapter.execute(call))).unwrap();
        assert_eq!(
            host.venue.requests.borrow()[0]
                .get("bypass_proxy")
                .and_then(Value::as_bool),
            expected
        );
    }
}

#[test]
fn new_socket_resets_a_token_send_latched_while_disconnected() {
    use safe_cex_transport::how::get_how_websocket_for_exchange;
    let host = FakeHost::new();
    let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
    host.run(leaf_error(adapter.execute(timed_request("/first", 1000))))
        .unwrap();
    let client = get_how_websocket_for_exchange(host.clone(), "bybit").unwrap();
    client.on_close();
    assert_eq!(
        client.send_token("stale-token"),
        Err(TransportError::Host("WebSocketNotReadyError".into()))
    );
    host.run(leaf_error(
        adapter.execute(timed_request("/replacement", 1000)),
    ))
    .unwrap();
    assert_eq!(host.venue.auth_attempts.borrow().len(), 2);
}

#[test]
fn remote_close_releases_the_socket_before_relay_drop() {
    let host = FakeHost::new();
    let relay = RelayWebSocket::new(
        host.clone(),
        "wss://tunnel/tunnel?ts-relay=wss%3A%2F%2Fvenue",
    )
    .unwrap();
    assert_eq!(host.run(relay.next_event()), Ok(RelayEvent::Open));
    host.venue.frames.borrow_mut().push_back((
        host.now_ms(),
        Ok(SocketFrame::Close {
            code: 1001,
            reason: "remote done".into(),
            was_clean: true,
        }),
    ));
    assert_eq!(
        host.run(relay.next_event()),
        Ok(RelayEvent::Close {
            code: 1001,
            reason: "remote done".into(),
            was_clean: true
        })
    );
    assert!(
        host.sockets.borrow()[0].upgrade().is_none(),
        "remote close must release the inner socket while the relay is still alive"
    );
    let closes = host.venue.close_requests.borrow().len();
    drop(relay);
    assert_eq!(
        host.venue.close_requests.borrow().len(),
        closes,
        "Drop must not close the already dead inner socket"
    );
}

#[test]
fn configured_authentication_and_clock_callbacks_reach_the_host_socket() {
    use safe_cex_transport::how_config::{HOWConfigPatch, configure_how};
    let host = FakeHost::new();
    host.token_error.set(true);
    let times = Rc::new(RefCell::new(vec![]));
    let observed = times.clone();
    configure_how(HOWConfigPatch {
        get_proxy_token: Some(Rc::new(|| {
            Box::pin(async { Ok("configured-token".into()) })
        })),
        is_token_expired: Some(Rc::new(|token| Ok(token != "configured-token"))),
        sync_server_time: Some(Some(Rc::new(move |time| observed.borrow_mut().push(time)))),
        ..Default::default()
    });
    let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
    host.run(leaf_error(adapter.execute(request("/configured"))))
        .unwrap();
    assert_eq!(host.token_calls.get(), 0);
    assert_eq!(host.venue.requests.borrow().len(), 1);
    assert_eq!(*times.borrow(), [1000.0]);
    assert!(host.venue.sent.borrow().iter().any(
        |frame| matches!(frame, SocketFrame::Text(text) if text.contains("configured-token"))
    ));
}

#[test]
fn latency_monitor_queries_only_known_used_domains_and_obeys_strict_staleness() {
    use safe_cex_transport::{
        how::{get_all_server_latencies, get_how_websocket_for_exchange, get_latency_for_domain},
        how_config::{HOWConfigPatch, configure_how},
    };
    configure_how(HOWConfigPatch {
        get_unique_latency_domains: Some(Some(Rc::new(|| {
            vec!["api.bybit.com".into(), "unused.test".into()]
        }))),
        ..Default::default()
    });
    let host = FakeHost::new();
    let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
    host.run(leaf_error(adapter.execute(request("/order"))))
        .unwrap();
    host.poll_tasks();
    let client = get_how_websocket_for_exchange(host.clone(), "bybit").unwrap();
    assert_eq!(host.venue.latency_queries.borrow().len(), 1);
    assert_eq!(
        host.venue.latency_queries.borrow()[0]["domain"],
        "api.bybit.com"
    );
    let entry = client.get_latency_for_domain("api.bybit.com").unwrap();
    assert_eq!(entry.latency_ms, 40);
    assert_eq!(entry.source, "measured");
    assert_eq!(entry.sample_count, 3);
    assert_eq!(entry.measured_at, 777);
    assert_eq!(
        get_latency_for_domain(host.clone(), "bybit", "api.bybit.com").unwrap(),
        Some(entry.clone())
    );
    let mut snapshot = client.get_all_latencies();
    snapshot.clear();
    assert_eq!(client.get_all_latencies().len(), 1);
    let (sg, jp) = get_all_server_latencies(host.clone()).unwrap();
    assert_eq!(sg.len(), 1);
    assert!(jp.is_empty());
    host.advance(10_000);
    assert_eq!(
        host.venue.latency_queries.borrow().len(),
        1,
        "exactly ten seconds is not stale"
    );
    host.advance(1_000);
    assert_eq!(host.venue.latency_queries.borrow().len(), 2);
    host.run(leaf_error(adapter.execute(request("/again"))))
        .unwrap();
    let mut unknown = request("/unknown");
    unknown.base_url = Some("https://unknown.test".into());
    host.run(leaf_error(adapter.execute(unknown))).unwrap();
    assert_eq!(host.venue.latency_queries.borrow().len(), 2);
    client.stop_latency_checks();
    host.advance(11_000);
    assert_eq!(host.venue.latency_queries.borrow().len(), 2);
    client.start_latency_checks();
    host.advance(1_000);
    assert_eq!(host.venue.latency_queries.borrow().len(), 3);
    client.on_message(SocketFrame::Text(json!({"type":"rpc.latency", "domain":"api.bybit.com", "error":"failed", "latency_ms":999}).to_string())).unwrap();
    assert_eq!(
        client
            .get_latency_for_domain("api.bybit.com")
            .unwrap()
            .latency_ms,
        40
    );
    client.dispose();
    host.advance(12_000);
    assert_eq!(host.venue.latency_queries.borrow().len(), 3);
}

#[test]
fn latency_tracking_happens_only_after_a_successful_socket_send_and_preserves_defaults() {
    use safe_cex_transport::{
        how::get_how_websocket_for_exchange,
        how_config::{HOWConfigPatch, configure_how},
    };
    configure_how(HOWConfigPatch {
        get_unique_latency_domains: Some(Some(Rc::new(|| vec!["known.test".into()]))),
        ..Default::default()
    });
    let host = FakeHost::new();
    let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
    host.run(leaf_error(adapter.execute(request("/open"))))
        .unwrap();
    let client = get_how_websocket_for_exchange(host.clone(), "bybit").unwrap();
    let message =
        json!({"type":"rpc.http", "nonce":"test", "arg":{"url":"https://known.test/order"}});
    host.venue.closed.set(true);
    assert!(client.send(message.clone()).is_err());
    assert!(host.venue.latency_queries.borrow().is_empty());
    host.venue.closed.set(false);
    client.send(message).unwrap();
    assert_eq!(host.venue.latency_queries.borrow().len(), 1);
    host.venue.now.set(1234);
    client.on_message(SocketFrame::Text(json!({"type":"rpc.latency", "domain":"known.test", "latency_ms":null, "source":"", "cached":0, "sample_count":0, "measured_at":0}).to_string())).unwrap();
    let entry = client.get_latency_for_domain("known.test").unwrap();
    assert_eq!(entry.latency_ms, Value::Null);
    assert_eq!(entry.source, "unknown");
    assert_eq!(entry.cached, 0);
    assert_eq!(entry.sample_count, 0);
    assert_eq!(entry.measured_at, 0);
    assert_eq!(entry.updated_at, 1234);
    client
        .on_message(SocketFrame::Text(
            json!({"type":"rpc.latency", "domain":"default.test", "latency_ms":7}).to_string(),
        ))
        .unwrap();
    let entry = client.get_latency_for_domain("default.test").unwrap();
    assert_eq!(entry.cached, false);
    assert_eq!(entry.sample_count, 1);
    assert_eq!(entry.measured_at, 1234);
    assert!(!client.handle_websocket_error(&TransportError::Host("reported".into())));
}

#[test]
fn relay_callbacks_and_buffered_bytes_reflect_inner_events_and_network_buffer() {
    let host = FakeHost::new();
    let relay = Rc::new(
        RelayWebSocket::new(
            host.clone(),
            "wss://tunnel.test?ts-relay=wss%3A%2F%2Fvenue.test",
        )
        .unwrap(),
    );
    let observed = Rc::new(RefCell::new(vec![]));
    for kind in ["open", "message", "error", "close"] {
        let events = observed.clone();
        let callback: Rc<dyn Fn(&RelayEvent)> =
            Rc::new(move |event| events.borrow_mut().push(event.clone()));
        match kind {
            "open" => relay.set_on_open(Some(callback)),
            "message" => relay.set_on_message(Some(callback)),
            "error" => relay.set_on_error(Some(callback)),
            _ => relay.set_on_close(Some(callback)),
        }
    }
    relay.send(SocketFrame::Text("queued".into())).unwrap();
    assert_eq!(
        relay.buffered_amount(),
        0,
        "pre-open relay queue is not network buffered bytes"
    );
    assert_eq!(host.run(relay.next_event()), Ok(RelayEvent::Open));
    host.venue.buffered_bytes.set(172);
    assert_eq!(relay.buffered_amount(), 172);
    relay
        .on_inner_message(SocketFrame::Binary(vec![1, 2]))
        .unwrap();
    assert_eq!(
        host.run(relay.next_event()),
        Ok(RelayEvent::Message(SocketFrame::Binary(vec![1, 2])))
    );
    relay.on_inner_error();
    assert_eq!(
        host.run(relay.next_event()),
        Ok(RelayEvent::Error("relay transport error".into()))
    );
    relay.on_inner_close(1001, "done".into(), true);
    assert_eq!(
        host.run(relay.next_event()),
        Ok(RelayEvent::Close {
            code: 1001,
            reason: "done".into(),
            was_clean: true
        })
    );
    relay.on_inner_close(1006, "duplicate".into(), false);
    relay.on_inner_error();
    assert_eq!(observed.borrow().len(), 4);
    assert_eq!(observed.borrow()[0], RelayEvent::Open);
    assert_eq!(
        observed.borrow()[3],
        RelayEvent::Close {
            code: 1001,
            reason: "done".into(),
            was_clean: true
        }
    );
    assert_eq!(relay.buffered_amount(), 0);
    relay.set_on_open(None);
    relay.set_on_close(None);
    relay.set_on_error(None);
    relay.set_on_message(None);
}

#[test]
fn custom_parser_and_forced_region_are_used_on_both_request_and_raw_response() {
    use safe_cex_transport::how_config::{CreateAxiosHttpOverWebsocketAdapterOptions, HOWRegion};
    let host = FakeHost::new();
    let parsed = Rc::new(RefCell::new(vec![]));
    let observed = parsed.clone();
    let adapter = HowRestAdapter::with_options(
        host.clone(),
        CreateAxiosHttpOverWebsocketAdapterOptions {
            region: Some(HOWRegion::JA),
            custom_parser: Some(Rc::new(move |text| {
                observed.borrow_mut().push(text.to_owned());
                Ok(Input::from_value(json!({"parsed":text})).unwrap())
            })),
        },
    )
    .unwrap();
    let call = request("/custom");
    let response = host
        .run(adapter.execute_with_context(call.clone()))
        .unwrap();
    assert_eq!(response.config, call);
    assert_eq!(response.response.status, 403);
    assert!(host.dials.borrow()[0].0.contains(".jp.tealstreet.io"));
    assert_eq!(parsed.borrow().len(), 2);
    assert_eq!(
        host.venue.requests.borrow()[0]["arg"]["data"]["parsed"],
        parsed.borrow()[0]
    );
    assert_eq!(host.venue.requests.borrow()[0]["format"], "raw");
    assert_eq!(
        response.response.body.json_value().unwrap()["parsed"],
        r#"{"id":9007199254740993}"#
    );
}

#[test]
fn custom_parser_throw_uses_string_fallback_and_response_error_keeps_original_config() {
    use safe_cex_transport::{
        how_config::CreateAxiosHttpOverWebsocketAdapterOptions, how_context::HOWRequestError,
    };
    let host = FakeHost::new();
    let adapter = HowRestAdapter::with_options(
        host.clone(),
        CreateAxiosHttpOverWebsocketAdapterOptions {
            custom_parser: Some(Rc::new(|_| {
                Err(TransportError::Host("parser rejected".into()))
            })),
            ..Default::default()
        },
    )
    .unwrap();
    let call = request("/parser-error");
    let error = host
        .run(adapter.execute_with_context(call.clone()))
        .unwrap_err();
    let HOWRequestError::Transport {
        error,
        config,
        method,
        url,
        ..
    } = error
    else {
        panic!("expected decode error")
    };
    assert_eq!(*config, call);
    assert_eq!(method, "POST");
    assert_eq!(url, "https://api.bybit.com/parser-error");
    assert!(
        error
            .to_string()
            .contains("TradeTunnelResponseDecodeError: parser rejected")
    );
    assert_eq!(
        host.venue.requests.borrow()[0]["arg"]["data"],
        String::from_utf8(call.body.unwrap().wire_bytes().unwrap()).unwrap()
    );
}

#[test]
fn parser_consumed_deadline_does_not_send_or_close_the_shared_connection() {
    use safe_cex_transport::{
        how_config::CreateAxiosHttpOverWebsocketAdapterOptions, how_context::HOWRequestError,
    };
    let host = FakeHost::new();
    let clock = host.venue.now.clone();
    let adapter = HowRestAdapter::with_options(
        host.clone(),
        CreateAxiosHttpOverWebsocketAdapterOptions {
            custom_parser: Some(Rc::new(move |text| {
                if text.contains("expire") {
                    clock.set(1000);
                }
                Ok(Input::from_value(json!({"parsed":text})).unwrap())
            })),
            ..Default::default()
        },
    )
    .unwrap();
    let mut call = timed_request("/expire", 100);
    call.body = Some(safe_cex_transport::RestBody::Encoded(
        br#"{"expire":true}"#.to_vec(),
    ));
    let error = host
        .run(adapter.execute_with_context(call.clone()))
        .unwrap_err();
    let HOWRequestError::Timeout(error) = error else {
        panic!("expected timeout")
    };
    assert_eq!(error.name, "TradeTunnelTimeoutError");
    assert_eq!(error.config, call);
    assert_eq!(error.method, "POST");
    assert_eq!(error.url, "https://api.bybit.com/expire");
    assert_eq!(error.trade_tunnel_route["selectedRegion"], "SG");
    assert_eq!(error.message, "WSoverHTTP request timed out: /expire");
    assert!(host.venue.requests.borrow().is_empty());
    host.run(leaf_error(adapter.execute(request("/later"))))
        .unwrap();
    assert_eq!(host.dials.borrow().len(), 1);
    assert_eq!(host.venue.requests.borrow().len(), 1);
}

struct SimulatedAdapter {
    host: Rc<FakeHost>,
    handed: Rc<Cell<usize>>,
    reached: Rc<Cell<usize>>,
    calls: Rc<RefCell<Vec<RestRequest>>>,
    delay: u64,
}
impl RestAdapter for SimulatedAdapter {
    fn execute(
        &self,
        request: RestRequest,
    ) -> safe_cex_transport::RestFuture<'_, safe_cex_transport::RestResponse> {
        self.handed.set(self.handed.get() + 1);
        Box::pin(async move {
            self.host
                .sleep_until(self.host.now_ms() + self.delay)
                .await
                .map_err(|error| {
                    safe_cex_transport::RestFailure::from_transport(error, request.clone())
                })?;
            self.reached.set(self.reached.get() + 1);
            self.calls.borrow_mut().push(request);
            Ok(safe_cex_transport::RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: safe_cex_transport::RestResponseBody::Encoded(
                    b"{\"simulated\":true}".to_vec(),
                ),
            })
        })
    }
}

#[test]
fn simulation_deadline_distinguishes_handed_request_from_executed_direct_future() {
    use safe_cex_transport::{
        how_config::{HOWConfigPatch, configure_how},
        how_context::HOWRequestError,
    };
    for (delay, timeout) in [(0, 100), (200, 100)] {
        let host = FakeHost::new();
        host.token_error.set(true);
        let handed = Rc::new(Cell::new(0));
        let reached = Rc::new(Cell::new(0));
        let calls = Rc::new(RefCell::new(vec![]));
        configure_how(HOWConfigPatch {
            simulate_http_over_websocket: Some(true),
            direct_adapter: Some(Some(Rc::new(SimulatedAdapter {
                host: host.clone(),
                handed: handed.clone(),
                reached: reached.clone(),
                calls: calls.clone(),
                delay,
            }))),
            ..Default::default()
        });
        let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
        let mut call = timed_request("/simulate", timeout);
        call.body = None;
        let result = host.run(adapter.execute_with_context(call.clone()));
        assert_eq!(handed.get(), 1);
        assert!(host.dials.borrow().is_empty());
        assert!(host.venue.requests.borrow().is_empty());
        assert_eq!(host.token_calls.get(), 0);
        if delay == 0 {
            assert_eq!(result.unwrap().config, call);
            assert_eq!(reached.get(), 1);
            assert_eq!(calls.borrow()[0].method, "POST");
            assert_eq!(calls.borrow()[0].url, "https://api.bybit.com/simulate");
            assert_eq!(
                calls.borrow()[0].body,
                Some(safe_cex_transport::RestBody::Encoded(b"{}".to_vec()))
            );
        } else {
            let HOWRequestError::Timeout(error) = result.unwrap_err() else {
                panic!("expected timeout")
            };
            assert_eq!(error.config, call);
            assert_eq!(error.trade_tunnel_route["simulated"], true);
            assert_eq!(
                reached.get(),
                0,
                "a handed request must not be mistaken for executed I/O"
            );
            assert!(calls.borrow().is_empty());
        }
    }
}

#[test]
fn expired_preparation_skips_parser_even_when_later_send_guard_also_rejects() {
    use safe_cex_transport::how_config::{
        CreateAxiosHttpOverWebsocketAdapterOptions, HOWConfigPatch, configure_how,
    };
    let host = FakeHost::new();
    host.auto_auth.set(true);
    let clock = host.venue.now.clone();
    configure_how(HOWConfigPatch {
        sync_server_time: Some(Some(Rc::new(move |_| clock.set(1000)))),
        ..Default::default()
    });
    let parsed = Rc::new(Cell::new(0));
    let observed = parsed.clone();
    let adapter = HowRestAdapter::with_options(
        host.clone(),
        CreateAxiosHttpOverWebsocketAdapterOptions {
            custom_parser: Some(Rc::new(move |_| {
                observed.set(observed.get() + 1);
                Ok(Input::Object(Vec::new()))
            })),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        host.run(leaf_error(
            adapter.execute(timed_request("/expired-before-parse", 100))
        )),
        Err(TransportError::Timeout)
    );
    assert_eq!(
        parsed.get(),
        0,
        "expired preparation must reject before invoking user parsing"
    );
    assert!(host.venue.requests.borrow().is_empty());
}

#[test]
fn regional_tunnel_latency_getters_preserve_ping_measurements_and_smoothing() {
    use safe_cex_transport::how::{
        get_all_tunnel_latencies, get_how_websocket_for_exchange, get_tunnel_latency_for_exchange,
    };
    let host = FakeHost::new();
    let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
    host.run(leaf_error(adapter.execute(request("/open"))))
        .unwrap();
    let client = get_how_websocket_for_exchange(host.clone(), "bybit").unwrap();
    assert_eq!(
        get_all_tunnel_latencies(host.clone()).unwrap(),
        (None, None)
    );
    for (sent, received, expected) in [(1500, 1540, 40), (1600, 1800, 88)] {
        host.venue.now.set(sent);
        client.ping().unwrap();
        host.venue.now.set(received);
        client
            .on_message(SocketFrame::Text(
                json!({"type":"rpc.ping","status":"success"}).to_string(),
            ))
            .unwrap();
        assert_eq!(
            get_tunnel_latency_for_exchange(host.clone(), "bybit").unwrap(),
            Some(expected)
        );
        assert_eq!(
            get_all_tunnel_latencies(host.clone()).unwrap(),
            (Some(expected), None)
        );
    }
    client.on_close();
    assert_eq!(
        get_tunnel_latency_for_exchange(host.clone(), "bybit").unwrap(),
        Some(88)
    );
}

#[test]
fn relay_fresh_headers_are_lazy_and_resolved_for_each_tunnel_dial() {
    use safe_cex_transport::relay::{UpgradeHeaders, WebSocketConnectionOptions};
    let calls = Rc::new(Cell::new(0));
    let provider_calls = calls.clone();
    let options = WebSocketConnectionOptions {
        headers: Some(UpgradeHeaders::Fresh(Rc::new(move || {
            provider_calls.set(provider_calls.get() + 1);
            indexmap::IndexMap::from([
                (
                    "Authorization".into(),
                    format!("token-{}", provider_calls.get()),
                ),
                ("origin".into(), "forged".into()),
            ])
        }))),
        headers_required: true,
        protocols: vec!["private-v1".into()],
        ..Default::default()
    };
    for expected in [1, 2] {
        let host = FakeHost::new();
        let relay = RelayWebSocket::with_connection_options(
            host.clone(),
            "wss://tunnel.example/tunnel?ts-relay=wss%3A%2F%2Fvenue.example%2Fprivate",
            options.clone(),
        )
        .unwrap();
        assert_eq!(calls.get(), expected - 1);
        host.run(relay.on_inner_open()).unwrap();
        assert_eq!(calls.get(), expected);
        let dials = host.dials.borrow();
        assert_eq!(dials[0].0, "wss://tunnel.example/tunnel");
        assert_eq!(
            dials[0].1.headers()["Authorization"],
            format!("token-{expected}")
        );
        assert_eq!(
            dials[0].1.headers()["Origin"],
            "https://preview.tealstreet.io"
        );
        assert!(!dials[0].1.headers().contains_key("origin"));
        assert!(dials[0].1.headers()["User-Agent"].contains("Mozilla"));
        assert_eq!(dials[0].1.alpn_protocols(), ["http/1.1"]);
        assert_eq!(dials[0].1.protocols(), ["private-v1"]);
    }
}

#[test]
fn relay_browser_ignores_best_effort_headers_but_rejects_required_auth_before_dial() {
    use safe_cex_transport::relay::{UpgradeHeaders, WebSocketConnectionOptions};
    for runtime in [SocketRuntime::Browser, SocketRuntime::BrowserWorker] {
        let host = FakeHost::new();
        host.runtime.set(runtime);
        let options = WebSocketConnectionOptions {
            headers: Some(UpgradeHeaders::Fresh(Rc::new(|| {
                panic!("browser must not evaluate headers")
            }))),
            headers_required: true,
            protocols: vec!["private-v1".into()],
            ..Default::default()
        };
        let url = "wss://tunnel.example/tunnel?ts-relay=wss%3A%2F%2Fvenue.example";
        let error = RelayWebSocket::with_connection_options(host.clone(), url, options.clone())
            .err()
            .unwrap();
        assert_eq!(
            error.to_string(),
            "Required WebSocket upgrade headers need a header-capable ws transport"
        );
        assert!(host.dials.borrow().is_empty());
        let relay = RelayWebSocket::with_connection_options(
            host.clone(),
            url,
            WebSocketConnectionOptions {
                headers_required: false,
                ..options
            },
        )
        .unwrap();
        host.run(relay.on_inner_open()).unwrap();
        let dials = host.dials.borrow();
        assert!(dials[0].1.headers().is_empty());
        assert!(dials[0].1.protocols().is_empty());
    }
}

#[test]
fn relay_required_auth_is_rechecked_before_lazy_dial() {
    use safe_cex_transport::relay::{UpgradeHeaders, WebSocketConnectionOptions};
    let host = FakeHost::new();
    let relay = RelayWebSocket::with_connection_options(
        host.clone(),
        "wss://tunnel.example/tunnel?ts-relay=wss%3A%2F%2Fvenue.example",
        WebSocketConnectionOptions {
            headers: Some(UpgradeHeaders::Fresh(Rc::new(|| {
                panic!("preflight must precede fresh auth")
            }))),
            headers_required: true,
            ..Default::default()
        },
    )
    .unwrap();
    host.runtime.set(SocketRuntime::Browser);
    let error = host.run(relay.on_inner_open()).unwrap_err();
    assert_eq!(
        error.to_string(),
        "Required WebSocket upgrade headers need a header-capable ws transport"
    );
    assert!(host.dials.borrow().is_empty());
    assert_eq!(host.token_calls.get(), 0);
}

#[test]
fn simulation_accepts_direct_only_hosts_and_real_dials_still_fail_closed() {
    use safe_cex_transport::how_config::{HOWConfigPatch, configure_how};
    for runtime in [SocketRuntime::BunNative, SocketRuntime::Native] {
        let host = FakeHost::new();
        host.runtime.set(runtime);
        host.header_capable.set(false);
        let handed = Rc::new(Cell::new(0));
        let reached = Rc::new(Cell::new(0));
        configure_how(HOWConfigPatch {
            simulate_http_over_websocket: Some(true),
            direct_adapter: Some(Some(Rc::new(SimulatedAdapter {
                host: host.clone(),
                handed: handed.clone(),
                reached: reached.clone(),
                calls: Rc::new(RefCell::new(vec![])),
                delay: 0,
            }))),
            ..Default::default()
        });
        let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
        assert!(
            host.run(leaf_error(adapter.execute(request("/direct-only"))))
                .is_ok()
        );
        assert_eq!(handed.get(), 1);
        assert_eq!(reached.get(), 1);
        assert!(host.dials.borrow().is_empty());
        assert_eq!(host.token_calls.get(), 0);
        configure_how(HOWConfigPatch {
            simulate_http_over_websocket: Some(false),
            ..Default::default()
        });
        assert!(matches!(
            host.run(leaf_error(adapter.execute(request("/real")))),
            Err(TransportError::UnavailableTransport(_))
        ));
        assert!(host.dials.borrow().is_empty());
    }
}

#[test]
fn bad_auth_joiners_share_one_failure_and_later_calls_can_retry() {
    let host = FakeHost::new();
    let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
    host.run(leaf_error(adapter.execute(request("/warmup"))))
        .unwrap();
    let client =
        safe_cex_transport::how::get_how_websocket_for_exchange(host.clone(), "bybit").unwrap();
    client
        .on_message(SocketFrame::Text(
            json!({"type":"rpc.auth","error":"expired"}).to_string(),
        ))
        .unwrap();
    let before = host.token_calls.get();
    host.token_error.set(true);
    host.token_delay.set(75);
    let mut futures: Vec<_> = (0..9)
        .map(|index| {
            leaf_error(adapter.execute(timed_request(
                "/shared-error",
                if index == 8 { 10 } else { 1000 },
            )))
        })
        .collect();
    assert_eq!(futures.len(), 9, "AUTH-CARDINALITY requested futures");
    let mut results = vec![None; 9];
    assert_eq!(
        results.len(),
        futures.len(),
        "AUTH-CARDINALITY result slots"
    );
    host.run(poll_fn(|cx| {
        for (future, result) in futures.iter_mut().zip(&mut results) {
            if result.is_none()
                && let Poll::Ready(value) = future.as_mut().poll(cx)
            {
                *result = Some(value);
            }
        }
        if results.iter().all(Option::is_some) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }));
    for (index, result) in results.into_iter().enumerate() {
        let expected = if index == 8 {
            TransportError::Timeout
        } else {
            TransportError::Host("proxy token unavailable".into())
        };
        assert_eq!(result.unwrap(), Err(expected));
    }
    assert_eq!(host.token_calls.get() - before, 1);
    assert_eq!(host.venue.requests.borrow().len(), 1);
    assert!(!host.venue.closed.get());
    assert_eq!(host.dials.borrow().len(), 1);
    host.token_error.set(false);
    assert!(
        host.run(leaf_error(
            adapter.execute(timed_request("/retry-later", 2000))
        ))
        .is_ok()
    );
    assert_eq!(host.venue.requests.borrow().len(), 2);
}

#[test]
fn raw_parser_corruption_sends_null_in_the_outbound_frame() {
    for raw in [false, true] {
        let host = FakeHost::new();
        let adapter = HowRestAdapter::new(host.clone(), None, raw).unwrap();
        let mut call = request("/corrupt");
        call.body = Some(safe_cex_transport::RestBody::Encoded(
            br#"{"text":"x:1234567890123456,"}"#.to_vec(),
        ));
        host.run(leaf_error(adapter.execute(call))).unwrap();
        let expected = if raw {
            Value::Null
        } else {
            json!({"text":"x:1234567890123456,"})
        };
        assert_eq!(host.venue.requests.borrow()[0]["arg"]["data"], expected);
    }
}

#[test]
fn staging_switch_matches_typescript_for_how_and_relay_dials() {
    use safe_cex_transport::{
        routing::{HowEgress, RoutingOptions, resolve_exchange_routing},
        tunnel_endpoints::{build_relay_url, tunnel_egress_url},
    };
    if let Ok(flag) = std::env::var("P2B_STAGING_CHILD") {
        let urls: Vec<String> = serde_json::from_str(
            &std::env::var("P2B_STAGING_EXPECTED_URLS").expect("parent source corpus row"),
        )
        .unwrap();
        assert_eq!(urls.len(), 2, "source staging region count");
        for (index, region) in [HowEgress::Jp, HowEgress::Sg].into_iter().enumerate() {
            let expected = &urls[index];
            assert_eq!(expected.ends_with("/tunnel-staging"), flag == "1");
            assert_eq!(tunnel_egress_url(region), expected);
            let host = FakeHost::new();
            let adapter = HowRestAdapter::new(host.clone(), Some(region), false).unwrap();
            host.run(leaf_error(adapter.execute(request("/staging"))))
                .unwrap();
            assert!(
                host.dials.borrow()[0]
                    .0
                    .starts_with(&format!("{expected}?v=2&t="))
            );
            let relay = RelayWebSocket::new(
                host.clone(),
                &build_relay_url(region, "wss://venue/private"),
            )
            .unwrap();
            assert!(matches!(host.run(relay.next_event()), Ok(RelayEvent::Open)));
            assert_eq!(host.dials.borrow().last().unwrap().0, *expected);
        }
        let route = resolve_exchange_routing(
            &safe_cex_types::ExchangeName::Bybit,
            &RoutingOptions {
                tealstreet_proxy: true,
                tealstreet_ws_route: true,
                ..Default::default()
            },
        );
        assert!(
            route
                .ws_private
                .relay
                .unwrap()
                .starts_with(&format!("{}?", urls[1]))
        );
        return;
    }
    let flags = ["unset", "0", "1", "true"];
    let script = "const load=require('./sdk-rs/safe-cex-transport/scripts/ts-source.cjs'); process.stdout.write(JSON.stringify(process.argv.slice(1).map(flag=>{delete process.env.NEXT_PUBLIC_TRADETUNNEL_STAGING;if(flag!=='unset')process.env.NEXT_PUBLIC_TRADETUNNEL_STAGING=flag;const {tunnelEgressUrl}=load('./packages/http-over-websocket/src/tunnelEndpoints.ts');return ['jp','sg'].map(tunnelEgressUrl)})))";
    let oracle = std::process::Command::new("node")
        .args(["-e", script])
        .args(flags)
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output()
        .unwrap();
    assert!(
        oracle.status.success(),
        "source staging corpus: {}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let corpus: Vec<Vec<String>> = serde_json::from_slice(&oracle.stdout).unwrap();
    assert_eq!(corpus.len(), flags.len(), "source staging flag count");
    for (flag, urls) in flags.into_iter().zip(corpus) {
        assert_eq!(urls.len(), 2, "STAGING-CARDINALITY source region count");
        let mut child = std::process::Command::new(std::env::current_exe().unwrap());
        child
            .args([
                "--exact",
                "staging_switch_matches_typescript_for_how_and_relay_dials",
                "--nocapture",
            ])
            .env(
                "P2B_STAGING_EXPECTED_URLS",
                serde_json::to_string(&urls).unwrap(),
            )
            .env("P2B_STAGING_CHILD", flag)
            .env_remove("NEXT_PUBLIC_TRADETUNNEL_STAGING");
        if flag != "unset" {
            child.env("NEXT_PUBLIC_TRADETUNNEL_STAGING", flag);
        }
        let output = child.output().unwrap();
        assert!(
            output.status.success(),
            "flag={flag}\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn closed_how_ping_substrings_drop_without_throwing_and_still_track_domain() {
    use safe_cex_transport::{
        how::{HOWWebSocket, get_how_websocket_for_exchange},
        how_config::{HOWConfigPatch, configure_how},
    };
    configure_how(HOWConfigPatch {
        get_unique_latency_domains: Some(Some(Rc::new(|| vec!["fapi.asterdex.com".into()]))),
        ..Default::default()
    });
    let host = FakeHost::new();
    let client = get_how_websocket_for_exchange(host.clone(), "bybit").unwrap();
    client.start_latency_checks();
    for path in ["ping", "PiNg", "PONG"] {
        client.send(json!({"type":"rpc.http","arg":{"url":format!("https://fapi.asterdex.com/fapi/v1/{path}")}})).unwrap();
    }
    client.ping().unwrap();
    assert!(host.venue.sent.borrow().is_empty());
    assert!(
        client
            .send(json!({"type":"rpc.http","arg":{"url":"https://fapi.asterdex.com/order"}}))
            .is_err()
    );
    let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
    host.run(leaf_error(adapter.execute(request("/normal"))))
        .unwrap();
    host.advance(1_001);
    assert!(
        host.venue
            .latency_queries
            .borrow()
            .iter()
            .any(|frame| frame["domain"] == "fapi.asterdex.com")
    );
    let standalone = HOWWebSocket::new(host.clone(), "wss://test.invalid/tunnel".into()).unwrap();
    standalone.send(json!({"type":"rpc.http","arg":{"url":"https://venue/order","data":{"note":"shopping"}}})).unwrap();
}

#[test]
fn repeated_latency_starts_preserve_orphan_chain_but_dispose_stops_output() {
    use safe_cex_transport::{
        how::get_how_websocket_for_exchange,
        how_config::{HOWConfigPatch, configure_how},
    };
    configure_how(HOWConfigPatch {
        get_unique_latency_domains: Some(Some(Rc::new(|| vec!["api.bybit.com".into()]))),
        ..Default::default()
    });
    let host = FakeHost::new();
    host.venue.lost_latency_response.set(true);
    let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
    host.run(leaf_error(adapter.execute(request("/order"))))
        .unwrap();
    let client = get_how_websocket_for_exchange(host.clone(), "bybit").unwrap();
    host.advance(100);
    client.start_latency_checks();
    host.advance(900);
    host.advance(100);
    assert_eq!(host.venue.latency_queries.borrow().len(), 3);
    client.stop_latency_checks();
    host.advance(900);
    assert_eq!(host.venue.latency_queries.borrow().len(), 4);
    client.dispose();
    host.advance(1_000);
    assert_eq!(host.venue.latency_queries.borrow().len(), 4);
}

#[test]
fn relay_refusal_strings_use_nullish_fallback_and_javascript_interpolation() {
    for (error, expected) in [
        (None, "unknown"),
        (Some(json!(null)), "unknown"),
        (Some(json!("ExpiredSignature")), "ExpiredSignature"),
        (Some(json!("")), ""),
        (Some(json!(false)), "false"),
        (Some(json!({"message":"x"})), "[object Object]"),
        (Some(json!(["x", null, 2])), "x,,2"),
    ] {
        for (kind, prefix) in [
            ("rpc.auth", "tunnel auth failed: "),
            ("rpc.relay", "relay refused: "),
        ] {
            let host = FakeHost::new();
            let relay = RelayWebSocket::new(
                host.clone(),
                "wss://tunnel.test/ws?ts-relay=wss%3A%2F%2Fvenue.test",
            )
            .unwrap();
            host.run(relay.on_inner_open()).unwrap();
            let mut frame = json!({"type":kind});
            if let Some(error) = error.clone() {
                frame["error"] = error;
            }
            relay
                .on_inner_message(SocketFrame::Text(frame.to_string()))
                .unwrap();
            let expected = format!("{prefix}{expected}");
            assert_eq!(
                host.run(relay.next_event()).unwrap(),
                RelayEvent::Error(expected.clone())
            );
            assert_eq!(
                host.run(relay.next_event()).unwrap(),
                RelayEvent::Close {
                    code: 4000,
                    reason: expected,
                    was_clean: false
                }
            );
        }
    }
}

#[test]
fn late_auth_joiner_does_not_inherit_initiators_retry_exhaustion() {
    let host = FakeHost::new();
    host.venue.fail_auth.set(true);
    let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
    let mut first = leaf_error(adapter.execute(timed_request("/first", 20_000)));
    let mut later = leaf_error(adapter.execute(timed_request("/later", 20_000)));
    let mut first_result = None;
    let mut later_result = None;
    host.run(poll_fn(|cx| {
        if host.now_ms() >= 7_000 {
            host.venue.fail_auth.set(false);
        }
        if first_result.is_none()
            && let Poll::Ready(value) = first.as_mut().poll(cx)
        {
            first_result = Some(value);
        }
        if host.now_ms() >= 5_000
            && later_result.is_none()
            && let Poll::Ready(value) = later.as_mut().poll(cx)
        {
            later_result = Some(value);
        }
        if first_result.is_some() && later_result.is_some() {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }));
    assert_eq!(
        first_result.unwrap(),
        Err(TransportError::Host(
            "Cannot refresh auth token, please reload the terminal".into()
        ))
    );
    assert!(later_result.unwrap().is_ok());
    assert_eq!(host.venue.requests.borrow().len(), 1);
    assert!(
        host.venue.requests.borrow()[0]["arg"]["url"]
            .as_str()
            .unwrap()
            .ends_with("/later")
    );
}

#[test]
fn simulated_how_close_permanently_disables_reconnect() {
    let host = FakeHost::new();
    let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
    host.run(leaf_error(adapter.execute(request("/warmup"))))
        .unwrap();
    let client =
        safe_cex_transport::how::get_how_websocket_for_exchange(host.clone(), "bybit").unwrap();
    let dials = host.dials.borrow().len();
    client.simulate_close();
    host.advance(1_500);
    let start = host.now_ms();
    assert_eq!(
        host.run(leaf_error(adapter.execute(timed_request("/after", 600)))),
        Err(TransportError::Timeout)
    );
    assert_eq!(host.now_ms() - start, 600);
    assert_eq!(host.dials.borrow().len(), dials);
    assert_eq!(host.venue.requests.borrow().len(), 1);
}

#[test]
fn relay_omitted_close_code_and_parked_handshake_timer_follow_inner_lifecycle() {
    let host = FakeHost::new();
    let relay =
        RelayWebSocket::new(host.clone(), "wss://tunnel/?ts-relay=wss%3A%2F%2Fvenue").unwrap();
    host.run(relay.on_inner_open()).unwrap();
    host.venue.frames.borrow_mut().clear();
    let mut next = pin!(relay.next_event());
    let mut cx = Context::from_waker(Waker::noop());
    assert!(next.as_mut().poll(&mut cx).is_pending());
    relay.close(None, "");
    assert_eq!(host.venue.close_requests.borrow().last().unwrap().0, None);
    // Model an inner close still in flight: no close/error callback has arrived.
    host.venue.closed.set(false);
    host.venue.now.set(16_000);
    assert!(next.as_mut().poll(&mut cx).is_pending());
    host.venue.frames.borrow_mut().push_back((
        16_000,
        Ok(SocketFrame::Close {
            code: 1005,
            reason: String::new(),
            was_clean: true,
        }),
    ));
    assert_eq!(
        next.as_mut().poll(&mut cx),
        Poll::Ready(Ok(RelayEvent::Close {
            code: 1005,
            reason: String::new(),
            was_clean: true
        }))
    );
}

#[test]
fn relay_failure_preserves_inner_buffered_amount_until_close_callback() {
    let host = FakeHost::new();
    host.venue.buffered_bytes.set(42);
    let relay =
        RelayWebSocket::new(host.clone(), "wss://tunnel/?ts-relay=wss%3A%2F%2Fvenue").unwrap();
    host.run(relay.on_inner_open()).unwrap();
    relay
        .on_inner_message(SocketFrame::Text(
            json!({"type":"rpc.relay","error":"no"}).to_string(),
        ))
        .unwrap();
    assert_eq!(relay.buffered_amount(), 42);
    relay.on_inner_close(1006, String::new(), false);
    assert_eq!(relay.buffered_amount(), 0);
}

#[test]
fn how_dispose_keeps_request_deadline_and_retained_regional_identity() {
    use safe_cex_transport::how::{dispose_how_websockets, get_how_websocket_for_exchange};
    let host = FakeHost::new();
    let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
    host.run(leaf_error(adapter.execute(request("/warmup"))))
        .unwrap();
    let client = get_how_websocket_for_exchange(host.clone(), "bybit").unwrap();
    host.venue.lost_response.set(true);
    let mut pending = leaf_error(adapter.execute(timed_request("/pending", 20_000)));
    let mut cx = Context::from_waker(Waker::noop());
    assert!(pending.as_mut().poll(&mut cx).is_pending());
    host.advance(1_000);
    dispose_how_websockets();
    assert!(Rc::ptr_eq(
        &client,
        &get_how_websocket_for_exchange(host.clone(), "bybit").unwrap()
    ));
    assert!(pending.as_mut().poll(&mut cx).is_pending());
    assert_eq!(host.run(pending), Err(TransportError::Timeout));
    assert_eq!(host.now_ms(), 20_000);
    let start = host.now_ms();
    assert_eq!(
        host.run(leaf_error(adapter.execute(timed_request("/disposed", 500)))),
        Err(TransportError::Timeout)
    );
    assert_eq!(host.now_ms() - start, 500);
}

#[test]
fn ping_on_not_ready_inner_socket_does_not_fail_an_unrelated_pending_request() {
    let host = FakeHost::new();
    let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
    host.run(leaf_error(adapter.execute(request("/warmup"))))
        .unwrap();
    let client =
        safe_cex_transport::how::get_how_websocket_for_exchange(host.clone(), "bybit").unwrap();
    host.venue.lost_response.set(true);
    let mut pending = leaf_error(adapter.execute(timed_request("/pending", 12_000)));
    let mut cx = Context::from_waker(Waker::noop());
    assert!(pending.as_mut().poll(&mut cx).is_pending());
    let before = host.venue.sent.borrow().len();
    host.venue.socket_not_ready.set(true);
    host.advance(5_001);
    assert!(pending.as_mut().poll(&mut cx).is_pending());
    assert_eq!(host.venue.sent.borrow().len(), before);
    client.ping().unwrap();
    assert!(pending.as_mut().poll(&mut cx).is_pending());
    assert_eq!(host.run(pending), Err(TransportError::Timeout));
}

#[test]
fn latency_entry_serializes_js_binary64_numbers_from_the_wire() {
    use safe_cex_transport::how::get_how_websocket_for_exchange;
    let host = FakeHost::new();
    let client = get_how_websocket_for_exchange(host, "bybit").unwrap();
    client.on_message(SocketFrame::Text(r#"{"type":"rpc.latency","domain":"numbers.test","latency_ms":9007199254740993,"sample_count":1.50,"measured_at":1e3}"#.into())).unwrap();
    let entry = client.get_latency_for_domain("numbers.test").unwrap();
    let value = serde_json::to_value(entry).unwrap();
    assert_eq!(value["latencyMs"].to_string(), "9007199254740992");
    assert_eq!(value["sampleCount"].to_string(), "1.5");
    assert_eq!(value["measuredAt"].to_string(), "1000");
}

#[test]
fn relay_open_send_failure_keeps_open_and_discards_the_drained_tail() {
    let host = FakeHost::new();
    let relay =
        RelayWebSocket::new(host.clone(), "wss://tunnel/?ts-relay=wss%3A%2F%2Fvenue").unwrap();
    host.run(relay.on_inner_open()).unwrap();
    relay
        .send(SocketFrame::Text(r#"{"type":"rpc.ping"}"#.into()))
        .unwrap();
    relay.send(SocketFrame::Text("lost tail".into())).unwrap();
    host.venue.fail_next_ping.set(true);
    let opened = SocketFrame::Text(r#"{"type":"rpc.relay","status":"open"}"#.into());
    assert!(relay.on_inner_message(opened.clone()).is_err());
    relay
        .send(SocketFrame::Text("after failure".into()))
        .unwrap();
    assert_eq!(host.run(relay.next_event()).unwrap(), RelayEvent::Open);
    assert!(host.venue.close_requests.borrow().is_empty());
    relay.on_inner_message(opened).unwrap();
    assert!(
        !host
            .venue
            .sent
            .borrow()
            .contains(&SocketFrame::Text("lost tail".into()))
    );
}

// These legacy wire tests assert leaf timeout/socket identity; REST envelope tests
// separately assert that structured rejections survive every adapter boundary.
fn leaf_error<'a, T: 'a>(future: safe_cex_transport::RestFuture<'a, T>) -> TransportFuture<'a, T> {
    Box::pin(async move {
        future.await.map_err(|error| {
            *error
                .transport_error
                .expect("wire fixture must carry leaf provenance")
        })
    })
}

#[test]
fn relay_token_rejection_and_auth_send_throw_share_the_fixed_error() {
    for token_failure in [true, false] {
        let host = FakeHost::new();
        safe_cex_transport::how_config::configure_how(
            safe_cex_transport::how_config::HOWConfigPatch {
                get_proxy_token: Some(Rc::new(move || {
                    Box::pin(async move {
                        if token_failure {
                            Err(TransportError::Host("specific token rejection".into()))
                        } else {
                            Ok("jwt".into())
                        }
                    })
                })),
                ..Default::default()
            },
        );
        host.venue
            .auth_send_failures
            .set(usize::from(!token_failure));
        let relay =
            RelayWebSocket::new(host.clone(), "wss://tunnel/?ts-relay=wss%3A%2F%2Fvenue").unwrap();
        assert_eq!(
            host.run(relay.next_event()).unwrap(),
            RelayEvent::Error("proxy token unavailable".into())
        );
        assert_eq!(
            host.run(relay.next_event()).unwrap(),
            RelayEvent::Close {
                code: 4000,
                reason: "proxy token unavailable".into(),
                was_clean: false
            }
        );
    }
}

#[test]
fn heartbeat_send_failure_waits_for_the_request_deadline_without_disconnect() {
    let host = FakeHost::new();
    let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
    host.run(adapter.execute(request("/warmup"))).unwrap();
    host.venue.lost_response.set(true);
    let mut pending = leaf_error(adapter.execute(timed_request("/pending", 100)));
    let mut cx = Context::from_waker(Waker::noop());
    assert!(pending.as_mut().poll(&mut cx).is_pending());
    let generation = host.venue.generation.get();
    host.venue.fail_next_ping.set(true);
    host.venue.enqueue(json!({"type":"heartbeat"}));
    assert!(pending.as_mut().poll(&mut cx).is_pending());
    assert!(!host.venue.fail_next_ping.get());
    assert_eq!(host.run(pending), Err(TransportError::Timeout));
    assert_eq!(host.venue.generation.get(), generation);
    assert!(host.venue.close_requests.borrow().is_empty());
}

#[test]
fn http_frame_top_level_order_matches_ts_with_optional_fields() {
    for raw in [false, true] {
        for bypass in [false, true] {
            let host = FakeHost::new();
            let adapter = HowRestAdapter::new(host.clone(), None, raw).unwrap();
            let mut call = request("/order");
            if bypass {
                call.base_url = Some("https://edge.grvt.io".into());
            }
            host.run(adapter.execute(call)).unwrap();
            let frames = host.venue.requests.borrow();
            let keys: Vec<_> = frames[0]
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect();
            let mut expected = vec!["type", "rpc_version"];
            if raw {
                expected.extend(["format", "accept_encoding", "accept_binary"]);
            }
            if bypass {
                expected.push("bypass_proxy");
            }
            expected.extend(["nonce", "arg"]);
            assert_eq!(keys, expected);
        }
    }
}

#[test]
fn overflow_is_projected_only_at_outbound_json_wire_boundary() {
    for raw in [false, true] {
        let host = FakeHost::new();
        let adapter = HowRestAdapter::new(host.clone(), None, raw).unwrap();
        let mut call = request("/overflow");
        call.body = Some(safe_cex_transport::RestBody::Encoded(
            br#"{"amount":1e400}"#.to_vec(),
        ));
        call.params = Some(serde_json::from_str(r#"{"amount":-1e400}"#).unwrap());
        host.run(leaf_error(adapter.execute(call))).unwrap();
        let calls = host.venue.requests.borrow();
        assert_eq!(calls[0]["arg"]["data"], json!({"amount":null}));
        assert_eq!(calls[0]["arg"]["params"], json!({"amount":null}));
    }
}

#[test]
fn ordinary_how_response_preserves_native_shape_without_json_projection() {
    let host = FakeHost::new();
    *host.venue.response_data.borrow_mut() =
        Some(serde_json::from_str(r#"{"z":-0,"a":1e400,"nested":[null,true,"123"]}"#).unwrap());
    let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
    let response = host
        .run(leaf_error(adapter.execute(request("/native"))))
        .unwrap();
    let safe_cex_transport::RestResponseBody::Untransformed(value) = response.body else {
        panic!("ordinary HOW response must retain native untransformed data");
    };
    assert_eq!(
        keys(&value),
        ["z", "a", "nested"]
            .iter()
            .map(|key| key.encode_utf16().collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        "ordinary HOW response retains insertion order"
    );
    assert_eq!(
        bits(field(&value, "z")),
        "8000000000000000",
        "ordinary HOW response retains signed zero bits"
    );
    assert!(
        matches!(field(&value,"a"),Input::Float(value) if *value==f64::INFINITY),
        "ordinary HOW response must not project overflow to null"
    );
    assert_eq!(
        field(&value, "nested").clone().into_value().unwrap(),
        json!([null, true, "123"]),
        "ordinary HOW response retains native nested shape"
    );
}

#[test]
fn how_query_projection_occurs_only_at_the_json_frame_boundary() {
    let source = std::process::Command::new("node")
        .arg("scripts/query-how-source.cjs")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("QUERY HOW source process");
    assert!(
        source.status.success(),
        "QUERY HOW source execution: {}",
        String::from_utf8_lossy(&source.stderr)
    );
    let source: Value = serde_json::from_slice(&source.stdout).expect("QUERY HOW source JSON");
    let host = FakeHost::new();
    let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
    let mut call = request("/query");
    call.query_profile = Some(safe_cex_transport::query::QueryProfile {
        key_order: safe_cex_transport::query::QueryKeyOrder::Preserve,
        arrays: safe_cex_transport::query::QueryArrayFormat::Indexed,
        encode: true,
    });
    call.query_params = Some(Q::Object(
        [
            ("nan".into(), Q::Number(f64::NAN)),
            ("infinity".into(), Q::Number(f64::INFINITY)),
            ("zero".into(), Q::Number(-0.0)),
            ("text".into(), Q::String("-0".into())),
            ("missing".into(), Q::Undefined),
            (
                "list".into(),
                Q::Array(vec![
                    Q::Undefined,
                    Q::Number(f64::NEG_INFINITY),
                    Q::Number(2.25),
                ]),
            ),
        ]
        .into_iter()
        .collect(),
    ));
    let Some(Q::Object(native)) = &call.query_params else {
        panic!("QUERY HOW native object")
    };
    assert_eq!(native.len(), 6, "QUERY HOW native field count");
    assert!(
        matches!(native.get("nan"), Some(Q::Number(n)) if n.is_nan()),
        "QUERY HOW native NaN"
    );
    assert!(
        matches!(native.get("zero"), Some(Q::Number(n)) if n.to_bits() == (-0.0f64).to_bits()),
        "QUERY HOW native zero sign"
    );
    host.run(leaf_error(adapter.execute(call))).unwrap();
    let calls = host.venue.requests.borrow();
    assert_eq!(calls.len(), 1, "QUERY HOW exact frame count");
    let params = calls[0].get("arg").unwrap().get("params").unwrap();
    assert_eq!(
        params,
        source
            .get("params")
            .expect("QUERY HOW required source params"),
        "QUERY HOW actual source JSON boundary"
    );
    assert_eq!(
        params,
        &json!({"nan":null,"infinity":null,"zero":0,"text":"-0","list":[null,null,2.25]}),
        "QUERY HOW explicit JSON projection"
    );
    assert!(
        params.get("missing").is_none(),
        "QUERY HOW undefined property omission"
    );
}

#[test]
fn disposed_how_message_does_not_dispatch_clock() {
    use safe_cex_transport::{
        how::HOWWebSocket,
        how_config::{HOWConfigPatch, configure_how},
    };
    let times = Rc::new(RefCell::new(Vec::new()));
    let observed = times.clone();
    configure_how(HOWConfigPatch {
        sync_server_time: Some(Some(Rc::new(move |time| observed.borrow_mut().push(time)))),
        ..Default::default()
    });
    let client = HOWWebSocket::new(FakeHost::new(), "wss://fixture.invalid/tunnel".into()).unwrap();
    let frame = SocketFrame::Text("{\"type\":\"serverTime\",\"serverTime\":1000}".into());
    client.on_message(frame.clone()).unwrap();
    assert_eq!(
        times.borrow().as_slice(),
        &[1000.0],
        "HOW DISPOSE native active positive control"
    );
    client.dispose();
    times.borrow_mut().clear();
    client.on_message(frame).unwrap();
    assert_eq!(
        times.borrow().len(),
        0,
        "HOW DISPOSE native postdispose callback count"
    );
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/disposed-message-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "HOW DISPOSE actual source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn how_consumes_grvt_config_extensions_like_actual_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/grvt-config-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "GRVT CONFIG SOURCE {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 7, "GRVT CONFIG source corpus count");
    for case in cases {
        let host = FakeHost::new();
        let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
        let mut call = request("/lite/v1/account_summary");
        call.base_url = Some("https://trades.grvt.io".into());
        call.headers = serde_json::from_value(case["headers"].clone()).unwrap();
        call.config_extensions = case["extra"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(key, value)| {
                (
                    key.clone(),
                    safe_cex_types::retained_json::RetainedJsonInput::from_value(value.clone())
                        .unwrap(),
                )
            })
            .collect();
        host.run(leaf_error(adapter.execute(call))).unwrap();
        assert_eq!(
            host.venue.requests.borrow()[0]["arg"]["headers"],
            case["expected"],
            "GRVT CONFIG HOW header projection: {}",
            case["extra"]
        );
    }
}

#[test]
fn how_projects_retained_grvt_native_config_only_at_frame_boundary() {
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    let output = std::process::Command::new("node")
        .env(
            "SAFE_CEX_TYPESCRIPT_PATH",
            std::env::var("SAFE_CEX_TYPESCRIPT_PATH").unwrap_or_else(|_| {
                concat!(env!("CARGO_MANIFEST_DIR"), "/../../node_modules/typescript").into()
            }),
        )
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/grvt-config-source.cjs"
        ))
        .arg("--native")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "native GRVT source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 5, "independent native GRVT source cases");
    let native = [
        Input::Undefined,
        Input::Float(f64::NAN),
        Input::Float(f64::INFINITY),
        Input::Float(-0.0),
        Input::Object(vec![
            ("missing".encode_utf16().collect(), Input::Undefined),
            ("nan".encode_utf16().collect(), Input::Float(f64::NAN)),
            (
                "infinity".encode_utf16().collect(),
                Input::Float(f64::INFINITY),
            ),
        ]),
    ];
    for index in 0..5 {
        let host = FakeHost::new();
        let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
        let mut call = request("/lite/v1/account_summary");
        call.base_url = Some("https://trades.grvt.io".into());
        call.headers = serde_json::from_value(cases[index]["headers"].clone()).unwrap();
        call.config_extensions = [
            ("grvtCookie".into(), native[index].clone()),
            ("grvtAccountId".into(), native[index].clone()),
        ]
        .into();
        host.run(leaf_error(adapter.execute(call))).unwrap();
        assert_eq!(
            host.venue.requests.borrow()[0]["arg"]["headers"],
            cases[index]["expected"],
            "native GRVT truthiness before JSON frame projection {index}"
        );
    }
}

#[test]
fn relay_constructor_preserves_source_url_error_classes_and_query_bytes() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let urls = [
        "relative",
        "wss://[",
        "wss://tunnel.test/a",
        "wss://tunnel.test/a?ts-relay=&ts-relay=wss%3A%2F%2Fvenue",
        "wss://tunnel.test/a?tag=a%20b&ts-relay=wss%3A%2F%2Fvenue&other=%7E#frag",
    ];
    let mut node = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/relay-url-source.cjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    node.stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&urls).unwrap().as_bytes())
        .unwrap();
    let output = node.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(expected.len(), urls.len());
    let mut observed = Vec::new();
    for url in urls {
        let host = FakeHost::new();
        observed.push(match RelayWebSocket::new(host, url) {
            Ok(_) => json!({"ok":safe_cex_transport::relay::parse_relay_url(url).unwrap()}),
            Err(TransportError::JavaScript(error)) => {
                json!({"name":error.name,"message":error.message})
            }
            Err(error) => json!({"name":"Error","message":error.to_string()}),
        });
    }
    assert_eq!(
        observed, expected,
        "real relay constructor must preserve source URL errors and query encoding"
    );
}

fn surrogate_source() -> Value {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/how-surrogate-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual HOW source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn retained_field<'a>(value: &'a Input, key: &[u16]) -> &'a Input {
    let Input::Object(fields) = value else {
        panic!("retained object required: {value:?}")
    };
    fields
        .iter()
        .rev()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value)
        .expect("own field")
}
fn field<'a>(value: &'a Input, key: &str) -> &'a Input {
    retained_field(value, &key.encode_utf16().collect::<Vec<_>>())
}
fn units(value: &Input) -> &[u16] {
    let Input::Text(units) = value else {
        panic!("retained text required")
    };
    units
}
fn bits(value: &Input) -> String {
    let Input::Float(number) = value else {
        panic!("native Float required: {value:?}")
    };
    format!("{:016x}", number.to_bits())
}
fn keys(value: &Input) -> Vec<Vec<u16>> {
    let Input::Object(fields) = value else {
        panic!("retained object keys required")
    };
    safe_cex_types::js_coercion::js_own_keys_utf16(fields.iter().map(|(key, _)| key.as_slice()))
        .into_iter()
        .map(<[u16]>::to_vec)
        .collect()
}
fn retained_observation(value: &Input) -> Value {
    let Input::Array(rows) = field(value, "list") else {
        panic!("both retained rows required")
    };
    assert_eq!(rows.len(), 2, "independent native row census");
    let own = field(value, "own");
    let own_keys = keys(own);
    json!({
        "keys":keys(value), "label":units(field(value,"label")),
        "loneKeyValue":units(retained_field(value,&[0xdfff])),"low":units(field(value,"low")),
        "rows":rows.iter().map(|row|json!({"keys":keys(row).iter().map(|key|String::from_utf16(key).unwrap()).collect::<Vec<_>>(),"id":bits(field(row,"id")),"amount":bits(field(row,"amount"))})).collect::<Vec<_>>(),
        "duplicate":bits(field(value,"dup")),"tiny":bits(field(value,"tiny")),
        "ownKeys":own_keys.iter().map(|key|String::from_utf16(key).unwrap()).collect::<Vec<_>>(),
        "ownValues":own_keys.iter().map(|key|String::from_utf16(units(retained_field(own,key))).unwrap()).collect::<Vec<_>>(),
    })
}

#[test]
fn how_lone_surrogate_ordinary_frame_settles_the_real_request() {
    let source = surrogate_source();
    assert_eq!(source["delivered"], 1);
    assert_eq!(
        source["ordinary"]["rows"].as_array().unwrap().len(),
        2,
        "source row census"
    );
    for name in ["raw", "gzipBinary", "gzipBase64"] {
        assert_eq!(
            source[name], source["ordinary"],
            "actual source path {name}"
        );
    }
    let host = FakeHost::new();
    *host.venue.response_data_json.borrow_mut() = Some(source["body"].as_str().unwrap().into());
    let adapter = HowRestAdapter::new(host.clone(), None, false).unwrap();
    let result = host.run(adapter.execute_with_receipt(request("/surrogate")));
    assert!(
        result.is_ok(),
        "valid source HOW UTF-16 response must settle instead of timing out: {result:?}"
    );
    let receipt = result.unwrap();
    assert_eq!(receipt.config.url, "/surrogate", "caller config receipt");
    assert_eq!(receipt.response.status, 200);
    assert_eq!(receipt.response.reason, None);
    assert!(receipt.response.headers.is_empty());
    assert!(
        receipt.response.body.json_value().is_err(),
        "native delivery must not require a UTF-8 model projection"
    );
    let safe_cex_transport::RestResponseBody::Untransformed(value) = receipt.response.body else {
        panic!("ordinary response provenance")
    };
    assert_eq!(
        retained_observation(&value),
        source["ordinary"],
        "complete retained ordinary response"
    );
}
#[test]
fn how_lone_surrogate_raw_parser_returns_the_complete_object() {
    use safe_cex_transport::how_config::CreateAxiosHttpOverWebsocketAdapterOptions;
    let source = surrogate_source();
    assert_eq!(source["raw"]["rows"].as_array().unwrap().len(), 2);
    let host = FakeHost::new();
    *host.venue.raw_response_body.borrow_mut() = Some(source["body"].as_str().unwrap().into());
    let calls = Rc::new(Cell::new(0));
    let parser_calls = calls.clone();
    let adapter = HowRestAdapter::with_options(
        host.clone(),
        CreateAxiosHttpOverWebsocketAdapterOptions {
            region: None,
            custom_parser: Some(Rc::new(move |text| {
                parser_calls.set(parser_calls.get() + 1);
                Ok(safe_cex_transport::how::json_parse(text))
            })),
        },
    )
    .unwrap();
    let mut call = request("/surrogate-raw");
    call.body = Some(safe_cex_transport::RestBody::Untransformed(Input::Object(
        vec![
            ("nan".encode_utf16().collect(), Input::Float(f64::NAN)),
            ("missing".encode_utf16().collect(), Input::Undefined),
            ("zero".encode_utf16().collect(), Input::Float(-0.0)),
            (
                "slots".encode_utf16().collect(),
                Input::Array(vec![Input::Undefined, Input::Float(f64::INFINITY)]),
            ),
        ],
    )));
    let response = host.run(leaf_error(adapter.execute(call))).unwrap();
    assert_eq!(source["requestParserCalls"], 0);
    assert_eq!(
        host.venue.requests.borrow()[0]["arg"]["data"],
        source["requestWire"],
        "native request bypasses parser and projects only at final HOW frame"
    );
    assert_eq!(
        calls.get(),
        1,
        "only raw response text invokes custom parser"
    );
    assert_eq!(response.status, 403);
    assert_eq!(
        response.headers.get("X-Quota").map(String::as_str),
        Some("12")
    );
    assert!(
        response.body.json_value().is_err(),
        "native custom parser output must precede typed projection"
    );
    let safe_cex_transport::RestResponseBody::Transformed(value) = response.body else {
        panic!("custom parser provenance")
    };
    assert_eq!(
        retained_observation(&value),
        source["raw"],
        "complete retained raw response"
    );
}
