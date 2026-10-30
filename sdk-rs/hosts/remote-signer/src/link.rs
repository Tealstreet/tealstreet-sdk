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

type Waiter = oneshot::Sender<Result<Value, Failure>>;

struct State {
    socket: SocketId,
    out: Outbox,
    available: bool,
    suspended: bool,
    unavailable_since: Option<Instant>,
    /// The last frame of any kind from the signing socket.
    heard_at: Instant,
    /// `heard_at` of the socket the last `attach` replaced.
    heard_before_attach: Instant,
    /// When the signing socket last changed.
    attached_at: Instant,
    /// id → (token, socket it was sent on, its sign-req frame, waiter).
    pending: HashMap<String, (u64, SocketId, String, Waiter)>,
    next_token: u64,
    last_sign_req_bytes: usize,
    echo: Option<(u64, Instant, EchoSize)>,
    echo_seq: u64,
}

pub struct SignerLink {
    state: Mutex<State>,
    /// Wakes requests waiting out a suspension on `attach` or `mark_unavailable`.
    reattached: Notify,
}

impl SignerLink {
    pub fn new(socket: SocketId, out: Outbox) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                socket,
                out,
                available: true,
                suspended: false,
                unavailable_since: None,
                heard_at: Instant::now(),
                heard_before_attach: Instant::now(),
                attached_at: Instant::now(),
                pending: HashMap::new(),
                next_token: 0,
                last_sign_req_bytes: 0,
                echo: None,
                echo_seq: 0,
            }),
            reattached: Notify::new(),
        })
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn is_available(&self) -> bool {
        let state = self.state();
        state.available && !state.suspended
    }

    /// Any frame from `socket`: the browser behind the signing socket is there.
    pub fn heard(&self, socket: SocketId) {
        let mut state = self.state();
        if state.socket == socket {
            state.heard_at = Instant::now();
        }
    }

    /// The last frame from the signing socket.
    pub fn heard_at(&self) -> Instant {
        self.state().heard_at
    }

    /// Whether something sent at `sent` may never have reached the browser: no
    /// frame followed it on the socket it went to, as far as this link can tell.
    pub fn unheard_since(&self, sent: Instant) -> bool {
        let state = self.state();
        (sent > state.heard_before_attach && sent <= state.attached_at) || sent > state.heard_at
    }

    /// How long the client has been gone, if it is.
    pub fn absent_for(&self) -> Option<Duration> {
        self.state().unavailable_since.map(|since| since.elapsed())
    }

    /// Send `{t:'sign-req', d: request}` and wait for its `sign-res`. While the
    /// session is parked it waits for the browser to come back, all within one
    /// `SIGN_TIMEOUT`, then fails as Suspended: never queued past the timeout.
    pub(crate) async fn round_trip(&self, id: &str, request: &Value) -> Result<Value, Failure> {
        let deadline = tokio::time::Instant::now() + SIGN_TIMEOUT;
        loop {
            let reattached = self.reattached.notified();
            {
                let state = self.state();
                if !(state.available && state.suspended) {
                    break;
                }
            }
            if tokio::time::timeout_at(deadline, reattached).await.is_err() {
                return Err(Failure::Suspended);
            }
        }
        let (sender, receiver) = oneshot::channel();
        let (out, frame, token) = {
            let mut state = self.state();
            if state.available && state.suspended {
                return Err(Failure::Suspended);
            }
            if !state.available {
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
            state
                .pending
                .insert(id.to_owned(), (token, socket, frame.clone(), sender));
            (state.out.clone(), frame, token)
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
        let result = match tokio::time::timeout_at(deadline, receiver).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(Failure::Aborted),
            // No browser to answer (parked, or a dropped socket not yet seen,
            // silent since the send) is a suspension, not a slow signer.
            Err(_) => {
                let state = self.state();
                if state.suspended || state.heard_at < sent_at {
                    Err(Failure::Suspended)
                } else {
                    Err(Failure::TimedOut)
                }
            }
        };
        drop(guard);
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
                .is_some_and(|(_, sent, _, _)| *sent == socket)
            {
                return false;
            }
            state.pending.remove(&id).map(|(_, _, _, sender)| sender)
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
        sender.is_some_and(|sender| sender.send(Ok(response)).is_ok())
    }

    /// The socket dropped but the session is parked for a resume. Requests
    /// already sent keep waiting within their own `SIGN_TIMEOUT`, and `attach`
    /// re-sends them; new requests wait for `attach` (see `round_trip`).
    pub fn suspend(&self) {
        let mut state = self.state();
        if !state.available || state.suspended {
            return;
        }
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
        let mut moved = Vec::new();
        if state.socket != socket {
            for (id, (_, sent, frame, _)) in state.pending.iter_mut() {
                *sent = socket;
                moved.push((id.clone(), frame.clone()));
            }
        }
        if state.socket != socket {
            state.heard_before_attach = state.heard_at;
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
                if let Some((_, _, _, sender)) = sender {
                    let _ = sender.send(Err(Failure::Aborted));
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
        if state.echo.is_some() || !state.available || state.suspended {
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
    for (_, (_, _, _, sender)) in state.pending.drain() {
        let _ = sender.send(Err(Failure::Aborted));
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
            .is_some_and(|(token, _, _, _)| *token == self.token)
        {
            state.pending.remove(self.id);
        }
    }
}
