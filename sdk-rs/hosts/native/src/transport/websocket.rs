//! One physical socket shared through synchronous polls, never a queued mutation.
use super::{IoGuard, WebSocketWriteError, WebSocketWriter, io_allowed};
use crate::routing::Socket;
use crate::{Events, HostEvent};
use futures_util::{Sink, Stream};
use safe_cex_engine::{RequestId, SocketId};
use std::future::poll_fn;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio_tungstenite::tungstenite::{Error, Message};

struct State {
    socket: Mutex<SocketState>,
    writes: Arc<tokio::sync::Semaphore>,
}

#[derive(Default)]
struct SocketState {
    socket: Option<Socket>,
    epoch: u64,
    ready: bool,
    retired: bool,
}

#[derive(Clone)]
pub(super) struct NetworkWriter {
    state: Arc<State>,
    guard: Option<IoGuard>,
    events: Events,
    socket: SocketId,
}

/// Aborting the transport task also closes a socket held by writer handles.
pub(super) struct Driver(Arc<State>);
impl Drop for Driver {
    fn drop(&mut self) {
        let mut socket = self
            .0
            .socket
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        socket.ready = false;
        socket.retired = true;
        socket.socket.take();
    }
}

impl NetworkWriter {
    pub(super) fn new(socket: SocketId, events: Events, guard: Option<IoGuard>) -> Self {
        Self {
            state: Arc::new(State {
                socket: Mutex::new(SocketState::default()),
                writes: Arc::new(tokio::sync::Semaphore::new(1)),
            }),
            guard,
            events,
            socket,
        }
    }

    pub(super) fn driver(&self) -> Driver {
        Driver(self.state.clone())
    }

    pub(super) fn opened(&self, socket: Socket) {
        let mut state = self.state.socket.lock().expect("new socket writer");
        if !state.retired {
            state.epoch = state.epoch.wrapping_add(1);
            state.ready = false;
            state.socket = Some(socket);
        }
    }

    pub(super) fn enable_writes(&self) {
        let mut state = self.state.socket.lock().expect("new socket writer");
        state.ready = !state.retired && state.socket.is_some();
    }

    // No mutex guard survives an await: reads and control writes poll the same
    // physical socket, leaving it available for a synchronous mutation handoff.
    fn poll_io<T>(
        &self,
        cx: &mut Context<'_>,
        operation: impl FnOnce(&mut Socket, &mut Context<'_>) -> Poll<Result<T, Error>>,
    ) -> Poll<Result<T, String>> {
        if !io_allowed(&self.guard) {
            return Poll::Ready(Err("account IO authority expired".into()));
        }
        let Ok(mut socket) = self.state.socket.lock() else {
            return Poll::Ready(Err("socket writer unavailable".into()));
        };
        let Some(socket) = socket.socket.as_mut() else {
            return Poll::Ready(Err("socket is closed".into()));
        };
        operation(socket, cx).map_err(|error| error.to_string())
    }

    pub(super) async fn next(&self) -> Result<Option<Message>, String> {
        poll_fn(|cx| {
            let allowed = io_allowed(&self.guard);
            let mut state = self.state.socket.lock().expect("socket writer state");
            let result = if !allowed {
                Poll::Ready(Err("account IO authority expired".into()))
            } else if let Some(socket) = state.socket.as_mut() {
                Pin::new(socket)
                    .poll_next(cx)
                    .map(Option::transpose)
                    .map_err(|error| error.to_string())
            } else {
                Poll::Ready(Ok(None))
            };
            if matches!(
                result,
                Poll::Ready(Err(_) | Ok(None | Some(Message::Close(_))))
            ) {
                state.ready = false;
                state.retired = true;
            }
            result
        })
        .await
    }

    pub(super) async fn send(&self, text: String) -> Result<(), String> {
        let _write = self
            .state
            .writes
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| "socket writer retired")?;
        let mut message = Some(Message::text(text));
        poll_fn(|cx| {
            self.poll_io(cx, |socket, cx| {
                std::task::ready!(Pin::new(&mut *socket).poll_ready(cx))?;
                Pin::new(socket).start_send(message.take().expect("one control handoff"))?;
                Poll::Ready(Ok(()))
            })
        })
        .await?;
        self.flush_unlocked().await
    }

    pub(super) async fn flush(&self) -> Result<(), String> {
        let _write = self
            .state
            .writes
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| "socket writer retired")?;
        self.flush_unlocked().await
    }

    async fn flush_unlocked(&self) -> Result<(), String> {
        poll_fn(|cx| self.poll_io(cx, |socket, cx| Pin::new(socket).poll_flush(cx))).await
    }

    pub(super) async fn close(&self) -> Result<(), String> {
        self.retire();
        let _write = self
            .state
            .writes
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| "socket writer retired")?;
        poll_fn(|cx| self.poll_io(cx, |socket, cx| Pin::new(socket).poll_close(cx))).await
    }
}

struct NetworkHandoff<'a> {
    writer: &'a NetworkWriter,
    epoch: u64,
    result: Option<Result<(), WebSocketWriteError>>,
}
impl safe_cex_engine::WebSocketHandoff for NetworkHandoff<'_> {
    fn send(
        &mut self,
        socket: SocketId,
        text: String,
        commit: &mut safe_cex_engine::WebSocketCommit<'_>,
    ) -> bool {
        if self.result.is_some() {
            return false;
        }
        // IO guards may re-enter too; evaluate them before taking either lock.
        let allowed = io_allowed(&self.writer.guard);
        let result = (|| {
            if !allowed || socket != self.writer.socket {
                return Err(WebSocketWriteError::NotReady);
            }
            let mut slot = self
                .writer
                .state
                .socket
                .try_lock()
                .map_err(|_| WebSocketWriteError::NotReady)?;
            if !slot.ready || slot.retired || slot.epoch != self.epoch {
                return Err(WebSocketWriteError::NotReady);
            }
            let socket = slot.socket.as_mut().ok_or(WebSocketWriteError::NotReady)?;
            let mut cx = Context::from_waker(std::task::Waker::noop());
            if !matches!(
                Pin::new(&mut *socket).poll_ready(&mut cx),
                Poll::Ready(Ok(()))
            ) {
                return Err(WebSocketWriteError::NotReady);
            }
            let mut frame = Some(Message::text(text));
            let mut sent = None;
            commit(&mut || {
                let Some(frame) = frame.take() else {
                    return false;
                };
                let result = match Pin::new(&mut *socket).start_send(frame) {
                    Err(Error::WriteBufferFull(_)) => Err(WebSocketWriteError::NotReady),
                    result => result.map_err(|error| WebSocketWriteError::Send(error.to_string())),
                };
                let accepted = !matches!(result, Err(WebSocketWriteError::NotReady));
                sent = Some(result);
                accepted
            });
            sent.unwrap_or(Err(WebSocketWriteError::NotReady))
        })();
        let accepted = !matches!(result, Err(WebSocketWriteError::NotReady));
        self.result = Some(result);
        accepted
    }
}

impl WebSocketWriter for NetworkWriter {
    fn try_write(
        &self,
        request: RequestId,
        claim: &mut dyn FnMut(&mut dyn safe_cex_engine::WebSocketHandoff),
    ) -> Result<(), WebSocketWriteError> {
        if !io_allowed(&self.guard) {
            return Err(WebSocketWriteError::NotReady);
        }
        let Ok(write) = self.state.writes.clone().try_acquire_owned() else {
            return Err(WebSocketWriteError::NotReady);
        };
        let mut cx = Context::from_waker(std::task::Waker::noop());
        let epoch = {
            let Ok(mut slot) = self.state.socket.try_lock() else {
                return Err(WebSocketWriteError::NotReady);
            };
            if !slot.ready || slot.retired {
                return Err(WebSocketWriteError::NotReady);
            }
            let Some(socket) = slot.socket.as_mut() else {
                return Err(WebSocketWriteError::NotReady);
            };
            // Backpressure cannot consume a new write grant.
            if !matches!(Pin::new(socket).poll_ready(&mut cx), Poll::Ready(Ok(()))) {
                return Err(WebSocketWriteError::NotReady);
            }
            slot.epoch
        };
        // Admission callbacks run unlocked; the handoff revalidates the socket
        // and commits the reservation only around the actual start_send.
        let mut handoff = NetworkHandoff {
            writer: self,
            epoch,
            result: None,
        };
        claim(&mut handoff);
        let Some(result) = handoff.result else {
            return Ok(());
        };
        if let Err(error) = result {
            self.retire();
            let _ = self.events.send(HostEvent::WsClosed(self.socket, 1006));
            return Err(error);
        }
        let writer = self.clone();
        tokio::spawn(async move {
            let _write = write;
            let result = tokio::select! {
                biased;
                _ = writer.events.closed() => return,
                result = tokio::time::timeout(Duration::from_secs(5), writer.flush_unlocked()) => result,
            };
            let error = match result {
                Ok(Ok(())) => return,
                Ok(Err(error)) => format!("WebSocket write flush failed: {error}"),
                Err(_) => "WebSocket write flush timed out after 5s".into(),
            };
            writer.retire();
            let _ = writer.events.send(HostEvent::HttpFailed(request, error));
            let _ = writer.events.send(HostEvent::WsClosed(writer.socket, 1006));
        });
        Ok(())
    }

    fn retire(&self) {
        let mut state = self
            .state
            .socket
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state.ready = false;
        state.retired = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::sync::{mpsc, oneshot};

    type Fixture = (
        NetworkWriter,
        Driver,
        Arc<AtomicBool>,
        mpsc::UnboundedReceiver<HostEvent>,
        mpsc::UnboundedReceiver<Message>,
        oneshot::Sender<()>,
    );
    async fn writer_fixture() -> Fixture {
        writer_fixture_with_config(None).await
    }
    async fn writer_fixture_with_config(
        config: Option<tokio_tungstenite::tungstenite::protocol::WebSocketConfig>,
    ) -> Fixture {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (messages, received) = mpsc::unbounded_channel();
        let (close, mut wait_close) = oneshot::channel();
        tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(tcp).await.unwrap();
            loop {
                tokio::select! {
                    Ok(()) = &mut wait_close => {
                        tokio::time::timeout(Duration::from_secs(2), socket.close(None)).await.unwrap().unwrap();
                        break;
                    }
                    message = socket.next() => {
                        let Some(Ok(message)) = message else { break };
                        if messages.send(message).is_err() { break; }
                    }
                }
            }
        });
        let (socket, _) =
            tokio_tungstenite::connect_async_with_config(format!("ws://{address}"), config, false)
                .await
                .unwrap();
        let (events, replies) = mpsc::unbounded_channel();
        let allowed = Arc::new(AtomicBool::new(true));
        let gate = allowed.clone();
        let writer = NetworkWriter::new(
            SocketId(1),
            events,
            Some(Arc::new(move || gate.load(Ordering::Acquire))),
        );
        let driver = writer.driver();
        writer.opened(socket);
        (writer, driver, allowed, replies, received, close)
    }

    fn try_frame(
        writer: &NetworkWriter,
        request: RequestId,
        claim: &mut dyn FnMut() -> Option<String>,
    ) -> Result<(), WebSocketWriteError> {
        writer.try_write(request, &mut |handoff| {
            if let Some(text) = claim() {
                handoff.send(writer.socket, text, &mut |send| send());
            }
        })
    }

    fn refuses_without_claim(writer: &NetworkWriter) {
        let mut claims = 0;
        assert!(matches!(
            try_frame(writer, RequestId(1), &mut || {
                claims += 1;
                Some("must-not-send".into())
            }),
            Err(WebSocketWriteError::NotReady)
        ));
        assert_eq!(claims, 0);
    }

    #[tokio::test]
    async fn physical_writer_buffer_refusal_does_not_commit() {
        let config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default()
            .write_buffer_size(0)
            .max_write_buffer_size(1);
        let (writer, _driver, _allowed, _events, mut received, _close) =
            writer_fixture_with_config(Some(config)).await;
        writer.enable_writes();
        let mut commits = 0;
        let result = writer.try_write(RequestId(8), &mut |handoff| {
            handoff.send(writer.socket, "must-not-send".into(), &mut |send| {
                if !send() {
                    return false;
                }
                commits += 1;
                true
            });
        });
        assert!(matches!(result, Err(WebSocketWriteError::NotReady)));
        assert_eq!(commits, 0);
        assert!(!matches!(
            tokio::time::timeout(Duration::from_millis(20), received.recv()).await,
            Ok(Some(Message::Text(_)))
        ));
    }

    #[tokio::test]
    async fn physical_writer_claim_can_reenter_socket_without_deadlocking() {
        let (writer, _driver, _allowed, _events, mut received, _close) = writer_fixture().await;
        writer.enable_writes();
        let mut claims = 0;
        try_frame(&writer, RequestId(3), &mut || {
            assert!(
                writer.state.socket.try_lock().is_ok(),
                "release callback would deadlock re-entering the socket"
            );
            writer.enable_writes();
            refuses_without_claim(&writer);
            claims += 1;
            Some("reentrant".into())
        })
        .unwrap();
        assert_eq!(claims, 1);
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), received.recv())
                .await
                .unwrap()
                .unwrap(),
            Message::text("reentrant")
        );
    }

    #[tokio::test]
    async fn physical_writer_release_panic_does_not_poison_retirement() {
        let (writer, driver, _allowed, _events, mut received, _close) = writer_fixture().await;
        writer.enable_writes();
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = try_frame(&writer, RequestId(4), &mut || panic!("venue release panic"));
        }));
        assert!(panic.is_err());
        writer.retire();
        refuses_without_claim(&writer);
        drop(driver);
        assert!(writer.state.socket.lock().unwrap().socket.is_none());
        assert!(!matches!(
            tokio::time::timeout(Duration::from_millis(20), received.recv()).await,
            Ok(Some(Message::Text(_)))
        ));
    }

    #[tokio::test]
    async fn physical_writer_revalidates_socket_after_release_callback() {
        for change in ["retire", "drop", "revoke", "replace", "deny"] {
            let (writer, driver, allowed, _events, mut received, _close) = writer_fixture().await;
            let (
                replacement,
                _replacement_driver,
                _,
                _,
                mut replacement_received,
                _replacement_close,
            ) = writer_fixture().await;
            let mut socket = replacement.state.socket.lock().unwrap().socket.take();
            let mut driver = Some(driver);
            writer.enable_writes();
            let mut claims = 0;
            let result = try_frame(&writer, RequestId(5), &mut || {
                assert!(
                    writer.state.socket.try_lock().is_ok(),
                    "{change}: callback lock"
                );
                claims += 1;
                match change {
                    "retire" => writer.retire(),
                    "drop" => drop(driver.take()),
                    "revoke" => allowed.store(false, Ordering::Release),
                    "replace" => {
                        writer.opened(socket.take().unwrap());
                        writer.enable_writes();
                    }
                    _ => return None,
                }
                Some("must-not-send".into())
            });
            assert_eq!(claims, 1);
            if change == "deny" {
                assert!(result.is_ok());
            } else {
                assert!(
                    matches!(result, Err(WebSocketWriteError::NotReady)),
                    "{change}: {result:?}"
                );
            }

            for frames in [&mut received, &mut replacement_received] {
                assert!(
                    !matches!(
                        tokio::time::timeout(Duration::from_millis(20), frames.recv()).await,
                        Ok(Some(Message::Text(_)))
                    ),
                    "{change}: frame escaped revalidation"
                );
            }
        }
    }

    #[tokio::test]
    async fn physical_writer_guard_can_reenter_socket_during_reads_and_writes() {
        let (mut writer, _driver, _allowed, _events, mut received, _close) = writer_fixture().await;
        let state = writer.state.clone();
        writer.guard = Some(Arc::new(move || {
            assert!(
                state.socket.try_lock().is_ok(),
                "IO guard called under socket lock"
            );
            true
        }));
        writer.enable_writes();
        assert!(
            tokio::time::timeout(Duration::from_millis(20), writer.next())
                .await
                .is_err()
        );
        try_frame(&writer, RequestId(6), &mut || Some("guarded".into())).unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), received.recv())
                .await
                .unwrap()
                .unwrap(),
            Message::text("guarded")
        );
    }

    #[tokio::test]
    async fn physical_writer_poisoned_state_still_retires_and_drops_socket() {
        let (writer, driver, _allowed, _events, _received, _close) = writer_fixture().await;
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _state = writer.state.socket.lock().unwrap();
            panic!("socket operation panic");
        }));
        assert!(panic.is_err());
        writer.retire();
        refuses_without_claim(&writer);
        drop(driver);
        let state = writer
            .state
            .socket
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        assert!(state.retired && !state.ready && state.socket.is_none());
    }

    #[tokio::test]
    async fn physical_writer_refuses_connecting_busy_revoked_retired_and_dropped_sockets() {
        let (writer, driver, allowed, _events, mut received, _close) = writer_fixture().await;
        refuses_without_claim(&writer);
        writer.enable_writes();
        let busy = writer.state.writes.clone().acquire_owned().await.unwrap();
        refuses_without_claim(&writer);
        drop(busy);
        allowed.store(false, Ordering::Release);
        refuses_without_claim(&writer);
        allowed.store(true, Ordering::Release);
        // Releasing backpressure cannot revive an earlier refused frame.
        assert!(
            tokio::time::timeout(Duration::from_millis(20), received.recv())
                .await
                .is_err()
        );
        try_frame(&writer, RequestId(2), &mut || Some("control".into())).unwrap();
        let frame = tokio::time::timeout(Duration::from_secs(2), received.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(frame, Message::text("control"));
        writer.retire();
        writer.enable_writes();
        refuses_without_claim(&writer);
        drop(driver);
        refuses_without_claim(&writer);
        assert!(writer.state.socket.lock().unwrap().socket.is_none());
    }

    #[tokio::test]
    async fn physical_writer_reports_uncertain_flush_and_retires_before_host_close_dispatch() {
        let (writer, _driver, allowed, mut events, _received, _close) = writer_fixture().await;
        writer.enable_writes();
        let mut claims = 0;
        try_frame(&writer, RequestId(7), &mut || {
            claims += 1;
            Some("setting".into())
        })
        .unwrap();
        assert_eq!(claims, 1);
        // The physical writer accepted the frame, but authority is lost before
        // its async flush. Completion must report an uncertain send.
        allowed.store(false, Ordering::Release);
        let failure = tokio::time::timeout(Duration::from_secs(2), events.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(failure, HostEvent::HttpFailed(RequestId(7), error)
            if error.contains("account IO authority expired")));
        assert!(matches!(
            events.recv().await,
            Some(HostEvent::WsClosed(SocketId(1), 1006))
        ));
        allowed.store(true, Ordering::Release);
        refuses_without_claim(&writer);
    }

    #[tokio::test]
    async fn physical_writer_rejects_peer_close_before_host_ingress() {
        let (writer, _driver, _allowed, _events, _received, close) = writer_fixture().await;
        writer.enable_writes();
        close.send(()).unwrap();
        let message = tokio::time::timeout(Duration::from_secs(2), writer.next())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(message, Some(Message::Close(_))));
        refuses_without_claim(&writer);
    }
}
