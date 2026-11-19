//! One client session's signing channel: `sign-req` out, `sign-res` back by id,
//! exactly as the TS exchange-server's `WebSocketRemoteSigner` speaks it.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio::sync::{Notify, oneshot};

/// `SIGN_TIMEOUT_MS` in services/exchange-server/src/remote-signer.ts.
pub const SIGN_TIMEOUT: Duration = Duration::from_millis(15_000);
/// Largest padded echo, like the TS server's `ECHO_PAD_CAP`.
pub const ECHO_PAD_CAP: usize = 64 * 1024;

/// Writes one text frame to the client socket; false when it was not sent.
pub type Outbox = Arc<dyn Fn(String) -> bool + Send + Sync>;
/// The server's identity for one client socket. A `sign-res` resolves only a
/// request sent on the same socket, whatever its id.
pub type SocketId = u64;

/// Metadata only; the host hashes the untrusted ID before journaling it.
pub struct TimeoutDiagnostic {
    pub request_id: String,
    pub socket: SocketId,
    pub sent_socket: Option<SocketId>,
    pub request_age_ms: u64,
    pub attachment_age_ms: u64,
    pub last_heard_age_ms: u64,
    pub failure: Failure,
}
pub type TimeoutObserver = Arc<dyn Fn(TimeoutDiagnostic) + Send + Sync>;
/// Largest accepted `sign-res` text field: Lighter's 31 signatures are ~5 KB.
pub const MAX_RESPONSE_FIELD: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// The socket is gone (closed or disposed).
    Unavailable,
    /// The socket dropped and the session is parked for a resume.
    Suspended,
    SendFailed,
    TimedOut,
    /// The session ended, or the reattached socket could not take the request.
    Aborted,
    /// Another request with the same id is still waiting.
    Duplicate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EchoSize {
    Small,
    Large,
}

enum Reply {
    Signing(Value),
    JoinProof(SocketId, String),
}
type Waiter = oneshot::Sender<Result<Reply, Failure>>;
struct PendingWork {
    token: u64,
    socket: SocketId,
    frame: String,
    waiter: Waiter,
    pinned: bool,
}

struct State {
    socket: SocketId,
    attached_at: Instant,
    out: Outbox,
    available: bool,
    suspended: bool,
    server_owned: bool,
    unavailable_since: Option<Instant>,
    /// The last frame of any kind from the signing socket.
    heard_at: Instant,
    /// The last frame from each recent socket, signing or not.
    heard_by: HashMap<SocketId, Instant>,
    pending: HashMap<String, PendingWork>,
    next_token: u64,
    last_sign_req_bytes: usize,
    echo: Option<(u64, Instant, EchoSize)>,
    echo_seq: u64,
}

/// Sockets `heard_by` remembers; more than a few only on a link flapping for minutes.
const HEARD_SOCKETS: usize = 16;

pub struct SignerLink {
    state: Mutex<State>,
    timeout_observer: Mutex<Option<TimeoutObserver>>,
    /// Wakes requests waiting out a suspension on `attach` or `mark_unavailable`.
    reattached: Notify,
}

impl SignerLink {
    pub fn new(socket: SocketId, out: Outbox) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                socket,
                attached_at: Instant::now(),
                out,
                available: true,
                suspended: false,
                server_owned: false,
                unavailable_since: None,
                heard_at: Instant::now(),
                heard_by: HashMap::new(),
                pending: HashMap::new(),
                next_token: 0,
                last_sign_req_bytes: 0,
                echo: None,
                echo_seq: 0,
            }),
            timeout_observer: Mutex::new(None),
            reattached: Notify::new(),
        })
    }

    /// Host journal hook; never invoked while the link state is locked.
    pub fn observe_timeouts(&self, observer: TimeoutObserver) {
        *self
            .timeout_observer
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = Some(observer);
    }

    pub(crate) fn note_timeout(
        &self,
        id: &str,
        started: tokio::time::Instant,
        sent_socket: Option<SocketId>,
        failure: Failure,
    ) {
        let observer = self
            .timeout_observer
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        let Some(observer) = observer else { return };
        let diagnostic = self.timeout_snapshot(&self.state(), id, started, sent_socket, failure);
        observer(diagnostic);
    }

    fn timeout_snapshot(
        &self,
        state: &State,
        id: &str,
        started: tokio::time::Instant,
        sent_socket: Option<SocketId>,
        failure: Failure,
    ) -> TimeoutDiagnostic {
        TimeoutDiagnostic {
            request_id: id.to_owned(),
            socket: state.socket,
            sent_socket,
            request_age_ms: millis(started.elapsed()),
            attachment_age_ms: millis(state.attached_at.elapsed()),
            last_heard_age_ms: millis(state.heard_at.elapsed()),
            failure,
        }
    }

    fn emit_timeout(&self, diagnostic: TimeoutDiagnostic) {
        let observer = self
            .timeout_observer
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        if let Some(observer) = observer {
            observer(diagnostic);
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Retire browser signing without aborting or replaying any pending request.
    pub fn promote_to_server_owned(&self) -> bool {
        let mut state = self.state();
        if !state.available || !state.pending.is_empty() {
            return false;
        }
        state.server_owned = true;
        state.suspended = false;
        state.unavailable_since = None;
        state.echo = None;
        true
    }

    pub fn is_available(&self) -> bool {
        let state = self.state();
        state.available && !state.suspended
    }

    /// Any frame from `socket`: the browser behind the signing socket is there.
    pub fn heard(&self, socket: SocketId) {
        let mut state = self.state();
        let now = Instant::now();
        if state.socket == socket {
            state.heard_at = now;
        }
        state.heard_by.insert(socket, now);
        if state.heard_by.len() > HEARD_SOCKETS {
            let oldest = state
                .heard_by
                .iter()
                .min_by_key(|(_, at)| **at)
                .map(|(id, _)| *id);
            oldest.map(|id| state.heard_by.remove(&id));
        }
    }

    /// The last frame from the signing socket.
    pub fn heard_at(&self) -> Instant {
        self.state().heard_at
    }

    /// Whether a frame from `socket` followed something sent to it at `sent`.
    pub fn heard_since(&self, socket: SocketId, sent: Instant) -> bool {
        self.state()
            .heard_by
            .get(&socket)
            .is_some_and(|at| *at > sent)
    }

    /// The socket signing goes through now.
    pub fn socket(&self) -> SocketId {
        self.state().socket
    }

    /// How long the client has been gone, if it is.
    pub fn absent_for(&self) -> Option<Duration> {
        self.state().unavailable_since.map(|since| since.elapsed())
    }

    /// Send `{t:'sign-req', d: request}` and wait for its `sign-res`. While the
    /// session is parked it waits for the browser to come back, all within one
    /// `SIGN_TIMEOUT`, then fails as Suspended: never queued past the timeout.
    pub(crate) async fn round_trip(&self, id: &str, request: &Value) -> Result<Value, Failure> {
        match self.request(id, request, false).await? {
            Reply::Signing(value) => Ok(value),
            Reply::JoinProof(_, _) => Err(Failure::Aborted),
        }
    }

    /// Admission waits never resume on a successor socket. Return the actual
    /// receiving socket and untouched JSON for the caller's closed decoder.
    pub async fn join_proof(
        &self,
        id: &str,
        request: &Value,
    ) -> Result<(SocketId, String), Failure> {
        match self.request(id, request, true).await? {
            Reply::JoinProof(socket, raw) => Ok((socket, raw)),
            Reply::Signing(_) => Err(Failure::Aborted),
        }
    }

    async fn request(&self, id: &str, request: &Value, pinned: bool) -> Result<Reply, Failure> {
        let started = tokio::time::Instant::now();
        let deadline = started + SIGN_TIMEOUT;
        loop {
            let reattached = self.reattached.notified();
            {
                let state = self.state();
                if !(state.available && state.suspended) {
                    break;
                }
                if pinned {
                    return Err(Failure::Suspended);
                }
            }
            if tokio::time::timeout_at(deadline, reattached).await.is_err() {
                self.note_timeout(id, started, None, Failure::Suspended);
                return Err(Failure::Suspended);
            }
        }
        let (sender, receiver) = oneshot::channel();
        let (out, frame, token, sent_socket) = {
            let mut state = self.state();
            if state.available && state.suspended {
                return Err(Failure::Suspended);
            }
            if !state.available || state.server_owned {
                return Err(Failure::Unavailable);
            }
            if state.pending.contains_key(id) {
                return Err(Failure::Duplicate);
            }
            state.next_token += 1;
            let token = state.next_token;
            let socket = state.socket;
            // `t` first: the browser recognises frames by prefix.
            let frame = format!(r#"{{"t":"sign-req","d":{request}}}"#);
            state.last_sign_req_bytes = frame.len();
            state.pending.insert(
                id.to_owned(),
                PendingWork {
                    token,
                    socket,
                    frame: frame.clone(),
                    waiter: sender,
                    pinned,
                },
            );
            (state.out.clone(), frame, token, socket)
        };
        let guard = Pending {
            link: self,
            id,
            token,
        };
        let sent_at = Instant::now();
        if !out(frame) {
            return Err(Failure::SendFailed);
        }
        let (result, diagnostic) = match tokio::time::timeout_at(deadline, receiver).await {
            Ok(Ok(result)) => (result, None),
            Ok(Err(_)) => (Err(Failure::Aborted), None),
            // No browser to answer (parked, or a dropped socket not yet seen,
            // silent since the send) is a suspension, not a slow signer.
            Err(_) => {
                let diagnostic = {
                    let state = self.state();
                    let failure = if state.suspended || state.heard_at < sent_at {
                        Failure::Suspended
                    } else {
                        Failure::TimedOut
                    };
                    self.timeout_snapshot(&state, id, started, Some(sent_socket), failure)
                };
                (Err(diagnostic.failure), Some(diagnostic))
            }
        };
        drop(guard);
        if let Some(diagnostic) = diagnostic {
            self.emit_timeout(diagnostic);
        }
        result
    }

    /// Route one `sign-res` payload from `socket` to its waiting request, once.
    /// False when no request with that id was sent on that socket (late,
    /// unknown, foreign or malformed). An oversized answer settles as an error.
    pub fn resolve(&self, socket: SocketId, mut response: Value) -> bool {
        let Some(id) = response
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_owned)
        else {
            return false;
        };
        let sender = {
            let mut state = self.state();
            if !state
                .pending
                .get(&id)
                .is_some_and(|pending| !pending.pinned && pending.socket == socket)
            {
                return false;
            }
            state.pending.remove(&id).map(|pending| pending.waiter)
        };
        let oversized = ["wsAuthSignature", "error"].iter().any(|field| {
            response.get(*field).is_some_and(|value| {
                !value
                    .as_str()
                    .is_some_and(|text| text.len() <= MAX_RESPONSE_FIELD)
            })
        });
        if oversized {
            response = serde_json::json!({ "id": id, "error": "invalid response" });
        }
        sender.is_some_and(|sender| sender.send(Ok(Reply::Signing(response))).is_ok())
    }

    /// Correlation is only routing. The join owner must decode the original
    /// bounded JSON and verify its signature before granting admission.
    pub fn resolve_join_proof(&self, socket: SocketId, raw: &str) -> bool {
        let Ok(value) = serde_json::from_str::<Value>(raw) else {
            return false;
        };
        let Some(id) = value.get("id").and_then(Value::as_str) else {
            return false;
        };
        let sender = {
            let mut state = self.state();
            if !state
                .pending
                .get(id)
                .is_some_and(|pending| pending.pinned && pending.socket == socket)
            {
                return false;
            }
            state.pending.remove(id).map(|pending| pending.waiter)
        };
        // Oversized matching replies settle as invalid, without retaining them.
        let raw = if raw.len() <= 4096 { raw } else { "{}" };
        sender.is_some_and(|sender| {
            sender
                .send(Ok(Reply::JoinProof(socket, raw.into())))
                .is_ok()
        })
    }

    /// The socket dropped but the session is parked for a resume. Requests
    /// already sent keep waiting within their own `SIGN_TIMEOUT`, and `attach`
    /// re-sends them; new requests wait for `attach` (see `round_trip`).
    pub fn suspend(&self) {
        let mut state = self.state();
        if !state.available || state.suspended || state.server_owned {
            return;
        }
        abort_join_proofs(&mut state);
        state.suspended = true;
        state.unavailable_since = Some(Instant::now());
    }

    /// The client came back within the grace window: sign over its new socket.
    /// Requests sent on another socket are re-sent here and only this socket
    /// answers them; one that cannot be re-sent aborts.
    pub fn attach(&self, socket: SocketId, out: Outbox) {
        let mut state = self.state();
        if !state.available {
            return;
        }
        abort_join_proofs(&mut state);
        let mut moved = Vec::new();
        if state.socket != socket {
            for (id, pending) in state.pending.iter_mut() {
                pending.socket = socket;
                moved.push((id.clone(), pending.frame.clone()));
            }
        }
        if state.socket != socket {
            state.attached_at = Instant::now();
        }
        state.socket = socket;
        state.out = out.clone();
        state.suspended = false;
        state.unavailable_since = None;
        state.heard_at = Instant::now();
        drop(state);
        for (id, frame) in moved {
            if !out(frame) {
                let sender = self.state().pending.remove(&id);
                if let Some(pending) = sender {
                    let _ = pending.waiter.send(Err(Failure::Aborted));
                }
            }
        }
        self.reattached.notify_waiters();
    }

    /// The session is over: abort everything; every later request fails.
    pub fn mark_unavailable(&self) {
        let mut state = self.state();
        if state.available {
            state.unavailable_since = Some(Instant::now());
        }
        state.available = false;
        abort_all(&mut state);
        drop(state);
        self.reattached.notify_waiters();
    }

    /// Send one echo probe, alternating bare and padded to the last sign-req.
    /// One outstanding echo at a time; false when one is still out or unsent.
    pub fn send_echo(&self) -> bool {
        let mut state = self.state();
        if state.echo.is_some() || !state.available || state.suspended || state.server_owned {
            return false;
        }
        state.echo_seq += 1;
        let size = if state.echo_seq.is_multiple_of(2) {
            EchoSize::Large
        } else {
            EchoSize::Small
        };
        let pad = match size {
            EchoSize::Large => state.last_sign_req_bytes.min(ECHO_PAD_CAP),
            EchoSize::Small => 0,
        };
        let id = state.echo_seq;
        state.echo = Some((id, Instant::now(), size));
        let frame = format!(
            r#"{{"t":"echo","d":{{"id":{id},"pad":"{}"}}}}"#,
            ".".repeat(pad)
        );
        let out = state.out.clone();
        drop(state);
        out(frame)
    }

    /// An `echo-res` payload: the round trip of the outstanding probe, if it
    /// is the one answered.
    pub fn echo_res(&self, response: &Value) -> Option<(Duration, EchoSize)> {
        let mut state = self.state();
        let (id, sent, size) = state.echo?;
        if response.get("id").and_then(Value::as_u64) != Some(id) {
            return None;
        }
        state.echo = None;
        Some((sent.elapsed(), size))
    }
}

fn abort_all(state: &mut State) {
    for (_, pending) in state.pending.drain() {
        let _ = pending.waiter.send(Err(Failure::Aborted));
    }
}
fn abort_join_proofs(state: &mut State) {
    let ids: Vec<_> = state
        .pending
        .iter()
        .filter(|(_, pending)| pending.pinned)
        .map(|(id, _)| id.clone())
        .collect();
    for id in ids {
        if let Some(pending) = state.pending.remove(&id) {
            let _ = pending.waiter.send(Err(Failure::Aborted));
        }
    }
}

/// A dropped or finished wait leaves nothing behind in the pending map.
struct Pending<'a> {
    link: &'a SignerLink,
    id: &'a str,
    token: u64,
}
impl Drop for Pending<'_> {
    fn drop(&mut self) {
        let mut state = self.link.state();
        if state
            .pending
            .get(self.id)
            .is_some_and(|pending| pending.token == self.token)
        {
            state.pending.remove(self.id);
        }
    }
}

#[cfg(test)]
mod server_owned_tests {
    use super::*;
    use std::{future::Future, task::Poll};
    #[tokio::test]
    async fn promotion_refuses_pending_work_then_never_sends_or_replays_browser_signatures() {
        let (sent, mut received) = tokio::sync::mpsc::unbounded_channel();
        let link = SignerLink::new(1, Arc::new(move |frame| sent.send(frame).is_ok()));
        let request = serde_json::json!({"id":"once"});
        let work = link.round_trip("once", &request);
        tokio::pin!(work);
        assert!(
            std::future::poll_fn(|cx| Poll::Ready(work.as_mut().poll(cx)))
                .await
                .is_pending()
        );
        assert!(received.try_recv().is_ok());
        assert!(!link.promote_to_server_owned());
        assert!(link.resolve(
            1,
            serde_json::json!({"id":"once", "wsAuthSignature":"synthetic"})
        ));
        assert!(work.await.is_ok());
        assert!(link.promote_to_server_owned());
        link.suspend();
        assert!(link.is_available());
        assert!(link.absent_for().is_none());
        link.attach(
            2,
            Arc::new(|_| panic!("retired remote signing must not send")),
        );
        assert_eq!(link.socket(), 2);
        assert_eq!(
            link.round_trip("new", &request).await,
            Err(Failure::Unavailable)
        );
        assert!(!link.send_echo());
        assert!(received.try_recv().is_err());
        link.mark_unavailable();
        assert!(!link.is_available());
        assert!(!link.promote_to_server_owned());
    }
}

fn millis(duration: Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}

#[cfg(test)]
mod timeout_diagnostics_tests {
    use super::*;
    use serde_json::json;

    #[tokio::test(start_paused = true)]
    async fn success_timeout_and_cancel_remove_the_original_pending_state() {
        let (send, mut frames) = tokio::sync::mpsc::unbounded_channel();
        let link = SignerLink::new(7, Arc::new(move |frame| send.send(frame).is_ok()));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let record = seen.clone();
        let weak = Arc::downgrade(&link);
        link.observe_timeouts(Arc::new(move |event| {
            assert!(
                weak.upgrade().unwrap().state.try_lock().is_ok(),
                "no host callback under the state lock"
            );
            assert!(weak.upgrade().unwrap().state().pending.is_empty());
            record.lock().unwrap().push(event);
        }));
        for mode in ["success", "timeout", "cancel"] {
            let running = link.clone();
            let task =
                tokio::spawn(async move { running.round_trip(mode, &json!({"id":mode})).await });
            frames.recv().await.unwrap();
            assert_eq!(link.state().pending.len(), 1);
            match mode {
                "success" => {
                    assert!(link.resolve(7, json!({"id":mode})));
                    assert!(task.await.unwrap().is_ok());
                }
                "timeout" => {
                    link.heard(7);
                    tokio::time::advance(SIGN_TIMEOUT).await;
                    assert_eq!(task.await.unwrap(), Err(Failure::TimedOut));
                }
                _ => {
                    task.abort();
                    assert!(task.await.unwrap_err().is_cancelled());
                }
            }
            assert!(link.state().pending.is_empty(), "cleanup after {mode}");
        }
        assert_eq!(seen.lock().unwrap().len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn deadline_and_late_reply_race_have_exactly_one_observed_outcome() {
        for order in 0..34 {
            let (send, mut frames) = tokio::sync::mpsc::unbounded_channel();
            let link = SignerLink::new(7, Arc::new(move |frame| send.send(frame).is_ok()));
            let seen = Arc::new(Mutex::new(Vec::new()));
            let record = seen.clone();
            link.observe_timeouts(Arc::new(move |event| record.lock().unwrap().push(event)));
            let running = link.clone();
            let task =
                tokio::spawn(
                    async move { running.round_trip("race", &json!({"id":"race"})).await },
                );
            frames.recv().await.unwrap();
            link.heard(7);
            let barrier = Arc::new(std::sync::Barrier::new(2));
            let ready = barrier.clone();
            let answering = link.clone();
            let reply = std::thread::spawn(move || {
                ready.wait();
                answering.resolve(7, json!({"id":"race"}))
            });
            if order == 0 {
                barrier.wait();
                let accepted = reply.join().unwrap();
                assert!(accepted);
                assert!(task.await.unwrap().is_ok());
            } else {
                if order == 1 {
                    tokio::time::advance(SIGN_TIMEOUT).await;
                    let result = task.await.unwrap();
                    barrier.wait();
                    let accepted = reply.join().unwrap();
                    assert_eq!(result, Err(Failure::TimedOut));
                    assert!(!accepted, "late-reply control");
                    assert_eq!(seen.lock().unwrap().len(), 1);
                    assert!(link.state().pending.is_empty());
                    continue;
                }
                barrier.wait();
                tokio::time::advance(SIGN_TIMEOUT).await;
                let accepted = reply.join().unwrap();
                let result = task.await.unwrap();
                if result.is_ok() {
                    assert!(accepted, "receipt requires a successfully queued reply");
                } else {
                    assert_eq!(result, Err(Failure::TimedOut));
                }
                assert_eq!(seen.lock().unwrap().len(), usize::from(result.is_err()));
            }
            assert!(link.state().pending.is_empty());
        }
    }
}
