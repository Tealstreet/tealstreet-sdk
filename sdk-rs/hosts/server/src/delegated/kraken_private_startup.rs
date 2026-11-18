//! Require the native private challenge handshake in addition to nonce-free REST.
use safe_cex_engine::{HttpRequest, SocketChannel, SocketContext, SocketId};
use safe_cex_native::{Events, HostEvent, HttpPrepareFuture, SocketConnection, Transport};
use serde::Deserialize;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

#[derive(Default)]
pub(super) struct Gate {
    current: Mutex<Option<Arc<Mutex<Handshake>>>>,
}

impl Gate {
    fn connection(&self) -> Arc<Mutex<Handshake>> {
        let handshake = Arc::new(Mutex::new(Handshake::default()));
        if let Ok(mut current) = self.current.lock() {
            *current = Some(handshake.clone());
        }
        handshake
    }

    pub(super) fn accepted(&self) -> bool {
        self.current.lock().ok().is_some_and(|current| {
            current.as_ref().is_some_and(|handshake| {
                handshake.lock().ok().is_some_and(|state| state.accepted())
            })
        })
    }
}

#[derive(Default)]
struct Handshake {
    open: bool,
    key: Option<String>,
    challenge: Option<String>,
    sent: u8,
    acknowledged: u8,
}

#[derive(Deserialize)]
struct Frame {
    event: String,
    api_key: Option<String>,
    message: Option<String>,
    feed: Option<String>,
    original_challenge: Option<String>,
    signed_challenge: Option<String>,
    error: Option<String>,
}

impl Frame {
    fn refused(&self) -> bool {
        matches!(
            self.event.as_str(),
            "error" | "subscribed_failed" | "unsubscribed_failed"
        ) || self.error.as_deref().is_some_and(|error| !error.is_empty())
            || matches!(self.event.as_str(), "alert" | "info")
                && self.message.as_deref().is_some_and(|message| {
                    let message = message.to_ascii_lowercase();
                    message.contains("authenticat") || message.contains("api key")
                })
    }
}

fn frame(text: &str) -> Option<Frame> {
    (text.len() <= 8192)
        .then(|| serde_json::from_str(text).ok())
        .flatten()
}

fn feed(name: Option<&str>) -> u8 {
    match name {
        Some("fills") => 1,
        Some("open_orders") => 2,
        Some("open_positions") => 4,
        _ => 0,
    }
}

impl Handshake {
    fn accepted(&self) -> bool {
        self.open && self.sent == 7 && self.acknowledged == 7
    }

    fn sent(&mut self, text: &str) {
        let Some(frame) = frame(text).filter(|_| self.open) else {
            return;
        };
        if frame.event == "challenge" {
            self.key = frame.api_key.filter(|key| !key.is_empty());
            self.challenge = None;
            self.sent = 0;
            self.acknowledged = 0;
        } else if frame.event == "subscribe"
            && self.key.is_some()
            && frame.api_key == self.key
            && self.challenge.is_some()
            && frame.original_challenge == self.challenge
            && frame
                .signed_challenge
                .is_some_and(|signature| !signature.is_empty())
        {
            self.sent |= feed(frame.feed.as_deref());
        }
    }

    fn received(&mut self, socket: SocketId, event: &HostEvent) {
        match event {
            HostEvent::WsOpened(id) if *id == socket => {
                *self = Self {
                    open: true,
                    ..Self::default()
                };
            }
            HostEvent::WsClosed(id, _) | HostEvent::WsRouteExhausted(id) if *id == socket => {
                *self = Self::default();
            }
            HostEvent::WsMessage(id, text) if *id == socket && self.open => {
                let Some(frame) = frame(text) else {
                    return;
                };
                if frame.refused() {
                    *self = Self::default();
                    return;
                }
                match frame.event.as_str() {
                    "challenge" if self.key.is_some() && self.challenge.is_none() => {
                        self.challenge = frame.message.filter(|message| !message.is_empty());
                    }
                    "subscribed" if self.challenge.is_some() => {
                        self.acknowledged |= self.sent & feed(frame.feed.as_deref());
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

pub(super) struct PrivateStartup<T> {
    pub(super) inner: T,
    pub(super) gate: Arc<Gate>,
    pub(super) allowed: Arc<dyn Fn() -> bool + Send + Sync>,
}

impl<T: Transport> Transport for PrivateStartup<T> {
    fn prepare_http(
        &self,
        request: HttpRequest,
        intent: safe_cex_engine::request_context::RoutingIntent,
    ) -> HttpPrepareFuture {
        self.inner.prepare_http(request, intent)
    }

    fn prepare_http_classified(
        &self,
        request: HttpRequest,
        intent: safe_cex_engine::request_context::RoutingIntent,
        class: safe_cex_engine::request_budget::RequestClass,
    ) -> HttpPrepareFuture {
        self.inner.prepare_http_classified(request, intent, class)
    }

    fn request(&self, request: HttpRequest, events: Events) {
        self.inner.request(request, events);
    }

    fn connect(
        &self,
        socket: SocketId,
        url: String,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        self.inner.connect(socket, url, events)
    }

    #[allow(clippy::too_many_arguments)]
    fn connect_socket(
        &self,
        binary: bool,
        socket: SocketId,
        url: String,
        headers: Vec<(&'static str, String)>,
        channel: Option<SocketChannel>,
        context: Option<SocketContext>,
        events: Events,
    ) -> SocketConnection {
        if channel != Some(SocketChannel::Private) {
            return self
                .inner
                .connect_socket(binary, socket, url, headers, channel, context, events);
        }
        let state = self.gate.connection();
        let (observed, mut incoming) = mpsc::unbounded_channel();
        let connection = self
            .inner
            .connect_socket(binary, socket, url, headers, channel, context, observed);
        let (outgoing, mut queued) = mpsc::unbounded_channel::<String>();
        let allowed = self.allowed.clone();
        tokio::task::spawn_local(async move {
            loop {
                tokio::select! {
                    event = incoming.recv() => {
                        let Some(event) = event else { break; };
                        if !allowed() { break; }
                        if let Ok(mut state) = state.lock() { state.received(socket, &event); }
                        if events.send(event).is_err() { break; }
                    }
                    text = queued.recv() => {
                        let Some(text) = text else { break; };
                        if !allowed() { break; }
                        if let Ok(mut state) = state.lock() { state.sent(&text); }
                        if connection.outgoing.send(text).is_err() { break; }
                    }
                }
            }
            if let Ok(mut state) = state.lock() {
                *state = Handshake::default();
            }
        });
        SocketConnection {
            outgoing,
            writer: connection.writer,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn receive(state: &mut Handshake, value: serde_json::Value) {
        state.received(
            SocketId(1),
            &HostEvent::WsMessage(SocketId(1), value.to_string()),
        );
    }

    fn challenge(state: &mut Handshake) {
        state.received(SocketId(1), &HostEvent::WsOpened(SocketId(1)));
        state.sent(r#"{"event":"challenge","api_key":"synthetic-key"}"#);
        receive(
            state,
            json!({"event":"challenge","message":"fresh-challenge"}),
        );
    }

    fn subscribe(state: &mut Handshake, name: &str, key: &str, original: &str) {
        state.sent(
            &json!({"event":"subscribe","feed":name,"api_key":key,
            "original_challenge":original,"signed_challenge":"synthetic-signature"})
            .to_string(),
        );
    }

    fn acknowledge(state: &mut Handshake, name: &str) {
        receive(state, json!({"event":"subscribed","feed":name}));
    }

    fn complete(state: &mut Handshake) {
        challenge(state);
        for name in ["fills", "open_orders", "open_positions"] {
            subscribe(state, name, "synthetic-key", "fresh-challenge");
            acknowledge(state, name);
        }
    }

    #[test]
    fn private_acknowledgments_require_matching_key_challenge_and_every_sent_feed() {
        let mut state = Handshake::default();
        challenge(&mut state);
        for name in ["fills", "open_orders", "open_positions"] {
            acknowledge(&mut state, name);
            subscribe(&mut state, name, "foreign-key", "fresh-challenge");
            acknowledge(&mut state, name);
            subscribe(&mut state, name, "synthetic-key", "old-challenge");
            acknowledge(&mut state, name);
        }
        assert!(!state.accepted());
        for name in ["fills", "open_orders"] {
            subscribe(&mut state, name, "synthetic-key", "fresh-challenge");
            acknowledge(&mut state, name);
        }
        assert!(
            !state.accepted(),
            "partial private authentication is not proof"
        );
        subscribe(
            &mut state,
            "open_positions",
            "synthetic-key",
            "fresh-challenge",
        );
        acknowledge(&mut state, "open_positions");
        assert!(state.accepted());
    }

    #[test]
    fn duplicate_malformed_oversized_and_foreign_socket_frames_cannot_supply_proof() {
        let mut state = Handshake::default();
        challenge(&mut state);
        for name in ["fills", "open_orders", "open_positions"] {
            subscribe(&mut state, name, "synthetic-key", "fresh-challenge");
            state.received(
                SocketId(1),
                &HostEvent::WsMessage(
                    SocketId(2),
                    json!({"event":"subscribed","feed":name}).to_string(),
                ),
            );
        }
        for text in [
            r#"{"event":"subscribed","feed":"fills","feed":"open_positions"}"#.into(),
            "invalid JSON".into(),
            " ".repeat(8193),
        ] {
            state.received(SocketId(1), &HostEvent::WsMessage(SocketId(1), text));
        }
        assert!(!state.accepted());
        for refusal in [
            json!({"event":"error","message":"Invalid challenge"}),
            json!({"event":"subscribed_failed","feed":"fills"}),
            json!({"event":"unsubscribed_failed","feed":"fills"}),
            json!({"event":"info","message":"Authentication failed"}),
            json!({"event":"alert","message":"Invalid API key"}),
            json!({"event":"subscribed","feed":"fills","error":"refused"}),
        ] {
            complete(&mut state);
            assert!(state.accepted());
            receive(&mut state, refusal);
            assert!(!state.accepted());
        }
    }

    #[test]
    fn closed_and_replaced_private_connections_cannot_reuse_authentication() {
        let gate = Gate::default();
        let old = gate.connection();
        complete(&mut old.lock().unwrap());
        assert!(gate.accepted());
        let current = gate.connection();
        complete(&mut old.lock().unwrap());
        assert!(
            !gate.accepted(),
            "old connection replies do not authenticate a replacement"
        );
        complete(&mut current.lock().unwrap());
        assert!(gate.accepted());
        current
            .lock()
            .unwrap()
            .received(SocketId(1), &HostEvent::WsClosed(SocketId(1), 1006));
        assert!(!gate.accepted());
    }

    #[tokio::test]
    async fn retired_owner_cannot_release_a_queued_authentication_frame() {
        use std::sync::atomic::{AtomicBool, Ordering};
        tokio::task::LocalSet::new()
            .run_until(async {
                struct Tape(Arc<Mutex<Vec<String>>>);
                impl Transport for Tape {
                    fn request(&self, _: HttpRequest, _: Events) {
                        unreachable!()
                    }
                    fn connect(
                        &self,
                        socket: SocketId,
                        _: String,
                        events: Events,
                    ) -> mpsc::UnboundedSender<String> {
                        let (tx, mut rx) = mpsc::unbounded_channel();
                        let sent = self.0.clone();
                        events.send(HostEvent::WsOpened(socket)).unwrap();
                        tokio::task::spawn_local(async move {
                            let _events = events;
                            while let Some(text) = rx.recv().await {
                                sent.lock().unwrap().push(text);
                            }
                        });
                        tx
                    }
                }
                let sent = Arc::new(Mutex::new(Vec::new()));
                let alive = Arc::new(AtomicBool::new(true));
                let owner = alive.clone();
                let transport = PrivateStartup {
                    inner: Tape(sent.clone()),
                    gate: Arc::default(),
                    allowed: Arc::new(move || owner.load(Ordering::SeqCst)),
                };
                let (events, _received) = mpsc::unbounded_channel();
                let connection = transport.connect_socket(
                    false,
                    SocketId(1),
                    "wss://fixture.invalid".into(),
                    vec![],
                    Some(SocketChannel::Private),
                    None,
                    events,
                );
                connection
                    .outgoing
                    .send(r#"{"event":"challenge","api_key":"synthetic-key"}"#.into())
                    .unwrap();
                alive.store(false, Ordering::SeqCst);
                for _ in 0..4 {
                    tokio::task::yield_now().await;
                }
                assert!(sent.lock().unwrap().is_empty());
            })
            .await;
    }
}
