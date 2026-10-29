//! One client session's signing channel: `sign-req` out, `sign-res` back by id,
//! exactly as the TS exchange-server's `WebSocketRemoteSigner` speaks it.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio::sync::oneshot;

/// `SIGN_TIMEOUT_MS` in services/exchange-server/src/remote-signer.ts.
pub const SIGN_TIMEOUT: Duration = Duration::from_millis(15_000);
/// Largest padded echo, like the TS server's `ECHO_PAD_CAP`.
pub const ECHO_PAD_CAP: usize = 64 * 1024;

/// Writes one text frame to the client socket; false when it was not sent.
pub type Outbox = Arc<dyn Fn(String) -> bool + Send + Sync>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// The socket is gone (closed or disposed).
    Unavailable,
    /// The socket dropped and the session is parked for a resume.
    Suspended,
    SendFailed,
    TimedOut,
    /// The socket went away while this request waited.
    Aborted,
    /// Another request with the same id is still waiting.
    Duplicate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EchoSize {
    Small,
    Large,
}

struct State {
    out: Outbox,
    available: bool,
    suspended: bool,
    unavailable_since: Option<Instant>,
    pending: HashMap<String, (u64, oneshot::Sender<Result<Value, Failure>>)>,
    next_token: u64,
    last_sign_req_bytes: usize,
    echo: Option<(u64, Instant, EchoSize)>,
    echo_seq: u64,
}

pub struct SignerLink {
    state: Mutex<State>,
}

impl SignerLink {
    pub fn new(out: Outbox) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                out,
                available: true,
                suspended: false,
                unavailable_since: None,
                pending: HashMap::new(),
                next_token: 0,
                last_sign_req_bytes: 0,
                echo: None,
                echo_seq: 0,
            }),
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

    /// How long the client has been gone, if it is.
    pub fn absent_for(&self) -> Option<Duration> {
        self.state().unavailable_since.map(|since| since.elapsed())
    }

    /// Send `{t:'sign-req', d: request}` and wait for its `sign-res`.
    pub(crate) async fn round_trip(&self, id: &str, request: &Value) -> Result<Value, Failure> {
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
            state.pending.insert(id.to_owned(), (token, sender));
            // `t` first: the browser recognises frames by prefix.
            let frame = format!(r#"{{"t":"sign-req","d":{request}}}"#);
            state.last_sign_req_bytes = frame.len();
            (state.out.clone(), frame, token)
        };
        let guard = Pending {
            link: self,
            id,
            token,
        };
        if !out(frame) {
            return Err(Failure::SendFailed);
        }
        let result = match tokio::time::timeout(SIGN_TIMEOUT, receiver).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(Failure::Aborted),
            Err(_) => Err(Failure::TimedOut),
        };
        drop(guard);
        result
    }

    /// Route one `sign-res` payload to its waiting request. False when no
    /// request with that id waits (late, unknown or malformed).
    pub fn resolve(&self, response: Value) -> bool {
        let Some(id) = response
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_owned)
        else {
            return false;
        };
        let Some((_, sender)) = self.state().pending.remove(&id) else {
            return false;
        };
        sender.send(Ok(response)).is_ok()
    }

    /// The socket dropped but the session is parked for a resume: abort what
    /// waits; new requests fail as Suspended until `attach`.
    pub fn suspend(&self) {
        let mut state = self.state();
        if !state.available || state.suspended {
            return;
        }
        state.suspended = true;
        state.unavailable_since = Some(Instant::now());
        abort_all(&mut state);
    }

    /// The client came back within the grace window: sign over its new socket.
    pub fn attach(&self, out: Outbox) {
        let mut state = self.state();
        if !state.available {
            return;
        }
        state.out = out;
        state.suspended = false;
        state.unavailable_since = None;
    }

    /// The session is over: abort everything; every later request fails.
    pub fn mark_unavailable(&self) {
        let mut state = self.state();
        if state.available {
            state.unavailable_since = Some(Instant::now());
        }
        state.available = false;
        abort_all(&mut state);
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
    for (_, (_, sender)) in state.pending.drain() {
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
            .is_some_and(|(token, _)| *token == self.token)
        {
            state.pending.remove(self.id);
        }
    }
}
