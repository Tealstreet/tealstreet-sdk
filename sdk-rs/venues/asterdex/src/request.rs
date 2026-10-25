//! Public and signed request lifecycle. Retries rebuild unsigned parameters; ambiguous
//! trading writes are never repeated after network or upstream failure.
use std::collections::{HashMap, VecDeque};

use safe_cex_engine::Level;
use safe_cex_engine::credential_selection::CredentialRole;
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::request_context::{
    Context, Egress, Operation, Policy, RetryEvidence, RetryPolicy, Scope,
};
use safe_cex_engine::{Cx, ErrorKind, Method, Response, TimerId, Venue, VenueError};
use serde::Deserialize;

use crate::sign::{Query, Recipe, Version};
use crate::{AsterDex, Pending, reset_pending};

const CLOCK_INTERVAL_MS: f64 = 300_000.0;
/// One `/time` attempt's own bound, as the host timeout it replaces.
const CLOCK_TIMEOUT_MS: u32 = 5_000;
const CLOCK_ADMISSION_RETRY_MS: f64 = 1_000.0;
const READ_TIMEOUT_MS: u32 = 20_000;

/// Unsigned public reads: the engine owns the bound and retry eligibility.
pub(crate) fn public_read_policy() -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: READ_TIMEOUT_MS,
        pacing: None,
        retry: RetryPolicy::ReadFailure,
    }
}

#[derive(Default)]
pub struct RequestState {
    last_sync_success: Option<f64>,
    clock_running: bool,
    startup_pending: bool,
    clock_timer: Option<TimerId>,
    /// A refused clock admission retries the same attempt; it is not a failure.
    clock_admission: Option<(TimerId, u8)>,
    retries: HashMap<TimerId, Box<SignedRequest>>,
    awaiting_clock: VecDeque<Box<SignedRequest>>,
}

pub struct SignedRequest {
    method: Method,
    path: String,
    query: Query,
    pub pending: Pending,
    stream: Option<ArrayAt>,
    network_retries: u8,
    timestamp_retried: bool,
    signature_retried: bool,
    version: Version,
    public: bool,
    role: CredentialRole,
    /// Engine authority of a dispatched listenKey session write.
    context: Option<Context>,
    /// Engine custody of a managed public read: the command's own Context, or
    /// a Background one retired before its native callback.
    read: Option<Context>,
}

pub struct ClockRequest {
    attempt: u8,
    context: Context,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Number {
    Number(f64),
    Text(String),
}

impl Number {
    fn value(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            Self::Text(value) if !value.trim().is_empty() => value.trim().parse().ok(),
            Self::Text(_) => None,
        }
        .filter(|value| value.is_finite())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServerTime {
    server_time: Number,
}

#[derive(Deserialize)]
struct ErrorBody {
    #[serde(default)]
    code: Option<Number>,
    #[serde(default)]
    msg: String,
}

impl SignedRequest {
    /// Reads, and listenKey writes: their repeat-safe session makes each
    /// retry a new operation.
    fn is_read(&self) -> bool {
        self.method == Method::Get || self.listen_key()
    }

    fn listen_key(&self) -> bool {
        matches!(
            self.pending,
            Pending::Account(crate::account::Request::ListenKey { .. })
                | Pending::Account(crate::account::Request::Keepalive { .. })
        )
    }

    fn reset(&mut self) {
        reset_pending(&mut self.pending);
    }

    /// Startup discovery: never transport-retried, refused on a negative code.
    fn discovery(&self) -> bool {
        self.public
            && matches!(
                self.pending,
                Pending::ExchangeInfo { .. } | Pending::Daily { .. }
            )
    }

    fn command_read(&self) -> bool {
        matches!(
            self.pending,
            Pending::DepthRead { .. } | Pending::Klines { .. }
        )
    }

    /// Retire a Background read before its native callback; a command read
    /// settles through its own terminal.
    fn settle(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(context) = self.read.take()
            && !self.command_read()
        {
            let _ = cx.finish_http_context(&context);
        }
    }

    fn retryable_status(&self, status: u16) -> bool {
        matches!(status, 503 | 504)
            || (matches!(self.method, Method::Get | Method::Put | Method::Delete)
                && (status == 429 || (500..=599).contains(&status)))
    }
}

impl AsterDex {
    pub(crate) fn start_requests(&mut self, cx: &mut Cx<'_, Pending>) {
        self.requests.startup_pending = true;
        self.begin_clock(cx);
    }

    pub(crate) fn signed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        role: CredentialRole,
        method: Method,
        path: &str,
        query: Query,
        pending: Pending,
    ) -> bool {
        self.queue_signed(cx, role, method, path, query, pending, None)
    }

    pub(crate) fn signed_streamed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        path: &str,
        query: Query,
        pending: Pending,
    ) -> bool {
        let role = CredentialRole::Read { scope: None };
        self.queue_signed(
            cx,
            role,
            Method::Get,
            path,
            query,
            pending,
            Some(ArrayAt::Root),
        )
    }

    pub(crate) fn public_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        path: &str,
        pending: Pending,
        stream: Option<ArrayAt>,
    ) {
        // History reads keep the native public path until their own slice.
        let read = match &pending {
            Pending::DepthRead { context } | Pending::Klines { context, .. } => {
                Some(Ok(context.clone()))
            }
            Pending::History(_) => None,
            _ => Some(cx.http_context(Scope::Background, public_read_policy())),
        };
        let read = match read.transpose() {
            Ok(read) => read,
            Err(error) => {
                self.on_read_failed(cx, pending, error.read_venue_error());
                return;
            }
        };
        let request = Box::new(SignedRequest {
            method: Method::Get,
            path: path.into(),
            query: Query::new(),
            pending,
            stream,
            network_retries: 0,
            timestamp_retried: false,
            signature_retried: false,
            version: self.version,
            public: true,
            role: CredentialRole::Read { scope: None },
            context: None,
            read,
        });
        self.dispatch_signed(cx, request);
    }

    #[allow(clippy::too_many_arguments)]
    fn queue_signed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        role: CredentialRole,
        method: Method,
        path: &str,
        query: Query,
        pending: Pending,
        stream: Option<ArrayAt>,
    ) -> bool {
        let request = Box::new(SignedRequest {
            method,
            path: path.into(),
            query,
            pending,
            stream,
            network_retries: 0,
            timestamp_retried: false,
            signature_retried: false,
            version: self.version,
            public: false,
            role,
            context: None,
            read: None,
        });
        let stale = self
            .requests
            .last_sync_success
            .is_none_or(|last| cx.now - last >= CLOCK_INTERVAL_MS);
        if self.requests.clock_running || stale {
            self.requests.awaiting_clock.push_back(request);
            self.begin_clock(cx);
            true
        } else {
            self.dispatch_signed(cx, request)
        }
    }

    fn dispatch_signed(&mut self, cx: &mut Cx<'_, Pending>, request: Box<SignedRequest>) -> bool {
        if let Some(read) = request.read.clone() {
            let path = &request.path;
            let url = format!("{}{path}", self.urls.rest);
            let stream = request.stream;
            match cx.request_managed(read, None, Method::Get, url, None, Pending::Signed(request)) {
                Ok(id) => {
                    if let Some(at) = stream {
                        cx.stream_response(id, at);
                    }
                }
                Err((Pending::Signed(mut unsent), error)) => {
                    unsent.settle(cx);
                    let error = error.read_venue_error();
                    self.on_read_failed(cx, unsent.pending, error);
                }
                Err(_) => unreachable!("a refused read returns its own continuation"),
            }
            return true;
        }
        if request.public {
            let url = format!("{}{}", self.urls.rest, request.path);
            match request.stream {
                Some(at) => {
                    cx.request_streamed(Method::Get, url, Vec::new(), at, Pending::Signed(request));
                }
                None => {
                    cx.request(Method::Get, url, Vec::new(), None, Pending::Signed(request));
                }
            }
            return true;
        }
        let Ok((selection, slot)) = self.credentials.select(request.role) else {
            self.on_request_failed(cx, request.pending, "Missing required Aster credentials");
            return false;
        };
        let recipe = self.auth(slot).and_then(|auth| {
            Recipe::new(
                slot.identity.clone(),
                auth,
                request.method,
                self.urls.rest,
                &request.path,
                request.query.clone(),
                self.options.testnet,
            )
        });
        let recipe = match recipe {
            Ok(recipe) => recipe,
            Err(error) => {
                self.on_request_failed(cx, request.pending, &error);
                return false;
            }
        };
        // The recipe writes the signed query and API-key header at dispatch.
        let path = &request.path;
        let url = format!("{}{path}", self.urls.rest);
        if request.listen_key() {
            // The reducer ignores a closed stream's create or keepalive, and
            // sending it would rotate the current stream's session.
            if !self.listen_key_current(&request.pending) {
                return false;
            }
            let credential = selection.credential(slot.identity.credential.clone());
            return self.listen_key_session_write(cx, request, url, credential, recipe);
        }
        let headers = Vec::new();
        let method = request.method;
        let stream = request.stream;
        let id = match stream {
            Some(at) => cx.request_streamed(method, url, headers, at, Pending::Signed(request)),
            None => cx.request(method, url, headers, None, Pending::Signed(request)),
        };
        selection.sign(cx, id, Box::new(recipe));
        true
    }

    /// One session operation per physical listenKey create or keepalive.
    fn listen_key_session_write(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Box<SignedRequest>,
        url: String,
        credential: safe_cex_engine::request_context::Credential,
        recipe: Recipe,
    ) -> bool {
        use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy};
        let create = request.method == Method::Post;
        let context = self
            .listen_key_session(cx, create, &credential)
            .and_then(|session| {
                cx.session_context(
                    &session,
                    Policy {
                        operation: Operation::Mutation,
                        credential: Some(credential),
                        timeout_ms: LISTEN_KEY_TIMEOUT_MS,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    },
                )
                .ok()
            });
        let Some(context) = context else {
            self.on_request_failed(cx, request.pending, "listenKey session is unavailable");
            return false;
        };
        let method = request.method;
        request.context = Some(context.clone());
        match cx.request_signed_managed(
            context,
            None,
            method,
            url,
            None,
            Box::new(recipe),
            Pending::Signed(request),
        ) {
            Ok(_) => true,
            Err((pending, _)) => {
                let mut pending = pending;
                if let Pending::Signed(request) = &mut pending
                    && let Some(context) = request.context.take()
                {
                    let _ = cx.finish_http_context(&context);
                }
                if let Pending::Signed(request) = pending {
                    self.on_request_failed(cx, request.pending, "listenKey session is unavailable");
                }
                false
            }
        }
    }

    pub(crate) fn on_signed_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut SignedRequest,
        item: &str,
    ) {
        self.on_item(cx, &mut request.pending, item);
    }

    pub(crate) fn on_signed_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Box<SignedRequest>,
        response: Result<Response<'_>, &str>,
    ) {
        if let Some(context) = request.context.take() {
            let accepted = response.as_ref().is_ok_and(|response| {
                (200..300).contains(&response.status) && !negative_code(response.body)
            });
            if accepted {
                let _ = cx.http_write_fact(
                    &context,
                    safe_cex_engine::classify::WriteFact::Accepted { order_ids: vec![] },
                    None,
                );
            }
            // Repeat-safe: an unproven write does not block the next operation.
            let _ = cx.finish_http_context(&context);
        }
        if let (Some(read), Ok(_)) = (&request.read, &response) {
            let delivered = if request.command_read() {
                cx.validate_read_http_delivery(read, read)
            } else {
                cx.validate_background_read_http_delivery(read)
            };
            if let Err(error) = delivered {
                request.settle(cx);
                self.on_read_failed(cx, request.pending, error.read_venue_error());
                return;
            }
        }
        let discovery = request.discovery();
        if let Ok(response) = response {
            let error = serde_json::from_str::<ErrorBody>(response.body).ok();
            let code = error
                .as_ref()
                .and_then(|error| error.code.as_ref()?.value());
            let message = error.as_ref().map_or("", |error| error.msg.as_str());
            let signature_rejected = code == Some(-1022.0)
                || message
                    .to_ascii_lowercase()
                    .contains("signature for this request is not valid");
            if !request.public
                && request.version == Version::V1
                && signature_rejected
                && !request.signature_retried
            {
                request.signature_retried = true;
                request.reset();
                self.dispatch_signed(cx, request);
                return;
            }
            if !request.public
                && code == Some(-1021.0)
                && message.contains("Timestamp")
                && !request.timestamp_retried
            {
                request.timestamp_retried = true;
                request.reset();
                self.requests.awaiting_clock.push_back(request);
                self.begin_clock(cx);
                return;
            }
            if discovery
                && (!(200..300).contains(&response.status) || code.is_some_and(|value| value < 0.0))
            {
                request.settle(cx);
                self.on_request_failed(
                    cx,
                    request.pending,
                    "Aster public discovery response failed",
                );
                return;
            }
            if request.retryable_status(response.status)
                && request.is_read()
                && request.network_retries < 3
            {
                self.retry_read(cx, request);
                return;
            }
            if request.public && code.is_some_and(|code| code < 0.0) {
                request.settle(cx);
                self.on_request_failed(
                    cx,
                    request.pending,
                    &format!("Aster public API error: {message}"),
                );
                return;
            }
            request.settle(cx);
            self.on_response(cx, request.pending, response);
        } else {
            let error = response.unwrap_err();
            if !discovery && request.is_read() && request.network_retries < 3 {
                self.retry_read(cx, request);
            } else {
                request.settle(cx);
                self.on_request_failed(cx, request.pending, error);
            }
        }
    }

    /// A typed transport, deadline or decode failure. Only a network loss is
    /// retried for a managed read; the rest keep their kind to the terminal.
    pub(crate) fn on_signed_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Box<SignedRequest>,
        error: VenueError,
    ) {
        if request.read.is_none() {
            self.on_signed_response(cx, request, Err(&error.message));
            return;
        }
        if error.kind == Some(ErrorKind::Network)
            && !request.discovery()
            && request.network_retries < 3
        {
            self.retry_read(cx, request);
            return;
        }
        request.settle(cx);
        self.on_read_failed(cx, request.pending, error);
    }

    /// The engine grants a managed read's resend under its original bound; the
    /// native 1/2/3 s delay stays.
    fn retry_read(&mut self, cx: &mut Cx<'_, Pending>, mut request: Box<SignedRequest>) {
        if let Some(read) = request.read.take() {
            match cx.http_retry_context(&read, RetryEvidence::ReadFailure) {
                Ok(read) => request.read = Some(read),
                Err(error) => {
                    request.read = Some(read);
                    request.settle(cx);
                    self.on_read_failed(cx, request.pending, error.read_venue_error());
                    return;
                }
            }
        }
        self.schedule_retry(cx, request);
    }

    pub(crate) fn on_signed_stream_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Box<SignedRequest>,
        response: Response<'_>,
    ) {
        let business_error = serde_json::from_str::<ErrorBody>(response.body)
            .ok()
            .and_then(|error| error.code.and_then(|code| code.value()))
            .is_some_and(|code| code < 0.0);
        if business_error || !(200..300).contains(&response.status) {
            self.on_signed_response(cx, request, Ok(response));
        } else if request.public {
            let mut request = request;
            request.settle(cx);
            self.on_stream_error(cx, request.pending, response);
        } else {
            self.on_request_failed(cx, request.pending, "missing expected Aster response list");
        }
    }

    fn schedule_retry(&mut self, cx: &mut Cx<'_, Pending>, mut request: Box<SignedRequest>) {
        request.network_retries += 1;
        request.reset();
        let timer = cx.set_timer(f64::from(request.network_retries) * 1000.0);
        self.requests.retries.insert(timer, request);
    }

    fn begin_clock(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.requests.clock_running {
            return;
        }
        self.requests.clock_running = true;
        if let Some(timer) = self.requests.clock_timer.take() {
            cx.clear_timer(timer);
        }
        self.issue_clock(cx, 0);
    }

    /// Each attempt is its own Background read with its own bound; the last
    /// two leave through the host's public CORS egress.
    fn issue_clock(&mut self, cx: &mut Cx<'_, Pending>, attempt: u8) {
        let version = if attempt % 2 == 1 {
            match self.version {
                Version::V1 => Version::V3,
                Version::V3 => Version::V1,
            }
        } else {
            self.version
        };
        let policy = Policy {
            timeout_ms: CLOCK_TIMEOUT_MS,
            retry: RetryPolicy::Never,
            ..public_read_policy()
        };
        let egress = if attempt < 2 {
            Egress::Direct
        } else {
            Egress::PublicCors
        };
        let Ok(context) = cx.http_context_with_egress(Scope::Background, policy, egress) else {
            self.clock_refused(cx, attempt);
            return;
        };
        let url = format!("{}{}/time", self.urls.rest, version.prefix());
        let pending = Pending::Clock(ClockRequest {
            attempt,
            context: context.clone(),
        });
        if cx
            .request_managed(context.clone(), None, Method::Get, url, None, pending)
            .is_err()
        {
            let _ = cx.finish_http_context(&context);
            self.clock_refused(cx, attempt);
        }
    }

    fn clock_refused(&mut self, cx: &mut Cx<'_, Pending>, attempt: u8) {
        let timer = cx.set_timer(CLOCK_ADMISSION_RETRY_MS);
        self.requests.clock_admission = Some((timer, attempt));
    }

    pub(crate) fn on_clock_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: ClockRequest,
        response: Result<Response<'_>, &str>,
    ) {
        let delivered = cx
            .validate_background_read_http_delivery(&request.context)
            .is_ok();
        let _ = cx.finish_http_context(&request.context);
        let time = response
            .ok()
            .filter(|_| delivered)
            .filter(|r| (200..300).contains(&r.status))
            .and_then(|r| serde_json::from_str::<ServerTime>(r.body).ok())
            .and_then(|r| r.server_time.value())
            .filter(|time| *time > 0.0)
            .filter(|time| cx.observe_protocol_time(*time));
        self.clock_settled(cx, request.attempt, time);
    }

    fn clock_settled(&mut self, cx: &mut Cx<'_, Pending>, attempt: u8, time: Option<f64>) {
        if time.is_some() {
            self.requests.last_sync_success = Some(cx.now);
        } else if attempt < 3 {
            self.issue_clock(cx, attempt + 1);
            return;
        } else {
            cx.notice(
                Level::Warn,
                code::REQUEST_FAILED,
                "Aster time sync failed; retaining the last clock offset",
            );
        }
        self.requests.clock_running = false;
        self.requests.clock_timer = Some(cx.set_timer(CLOCK_INTERVAL_MS));
        let queued = std::mem::take(&mut self.requests.awaiting_clock);
        for unsent in queued {
            self.dispatch_signed(cx, unsent);
        }
        if self.requests.startup_pending {
            self.requests.startup_pending = false;
            self.bootstrap(cx);
        }
    }

    /// Fixtures that saturate managed capacity start with a fresh clock, so
    /// their writes are not gated behind a clock read waiting for a slot.
    #[cfg(test)]
    pub(crate) fn mark_clock_synced(&mut self, now: f64) {
        self.requests.last_sync_success = Some(now);
    }

    pub(crate) fn on_request_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if let Some(request) = self.requests.retries.remove(&timer) {
            self.dispatch_signed(cx, request);
            return true;
        }
        if let Some((pending, attempt)) = self.requests.clock_admission
            && pending == timer
        {
            self.requests.clock_admission = None;
            self.issue_clock(cx, attempt);
            return true;
        }
        if self.requests.clock_timer == Some(timer) {
            self.requests.clock_timer = None;
            self.begin_clock(cx);
            return true;
        }
        false
    }
}

/// Same raw bound as the host default the unmanaged listenKey relied on.
const LISTEN_KEY_TIMEOUT_MS: u32 = 20_000;

fn negative_code(body: &str) -> bool {
    serde_json::from_str::<ErrorBody>(body)
        .ok()
        .and_then(|error| error.code.and_then(|code| code.value()))
        .is_some_and(|code| code < 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::Options;
    use safe_cex_engine::book::Book;
    use safe_cex_engine::{
        Action, Command, Engine, Event, HttpRequest, Notice, RequestRoute, SocketId,
    };

    fn role(method: Method) -> CredentialRole {
        match method {
            Method::Get => CredentialRole::Read { scope: None },
            _ => CredentialRole::Mutation,
        }
    }

    /// Retry delays are jittered into `[0.75, 1.0] * nominal`.
    fn jittered(actual: f64, nominal: f64) -> bool {
        (0.75 * nominal..=nominal).contains(&actual)
    }

    struct Probe {
        inner: AsterDex,
        method: Method,
        streamed: bool,
        clock_samples: Vec<(f64, f64)>,
    }

    impl Venue for Probe {
        type Pending = Pending;
        fn start(&mut self, cx: &mut Cx<'_, Pending>) {
            self.inner.requests.last_sync_success = Some(cx.now);
        }
        fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
        fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
        fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
        fn on_response(
            &mut self,
            cx: &mut Cx<'_, Pending>,
            pending: Pending,
            response: Response<'_>,
        ) {
            match pending {
                Pending::Signed(request) => {
                    self.inner.on_signed_response(cx, request, Ok(response))
                }
                Pending::Clock(request) => {
                    self.inner.on_clock_response(cx, request, Ok(response));
                    self.clock_samples.push((cx.now, cx.protocol_now()));
                }
                _ => panic!("unexpected probe response"),
            }
        }
        fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
            if let Pending::Signed(request) = pending {
                self.inner.on_signed_item(cx, request, item);
            }
        }
        fn on_stream_error(
            &mut self,
            cx: &mut Cx<'_, Pending>,
            pending: Pending,
            response: Response<'_>,
        ) {
            if let Pending::Signed(request) = pending {
                self.inner.on_signed_stream_error(cx, request, response);
            }
        }
        fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
            match pending {
                Pending::Signed(request) => self.inner.on_signed_response(cx, request, Err(error)),
                Pending::Clock(request) => {
                    self.inner.on_clock_response(cx, request, Err(error));
                    self.clock_samples.push((cx.now, cx.protocol_now()));
                }
                _ => panic!("unexpected probe failure"),
            }
        }
        fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
            self.inner.on_request_timer(cx, timer);
        }
        // The carrier's terminal needs a genuine command Context.
        fn on_command(&mut self, cx: &mut Cx<'_, Pending>, _: Command) {
            let Ok(context) = cx.admit_http_command(public_read_policy()) else {
                return;
            };
            let pending = Pending::Klines {
                context,
                rows: Vec::new(),
                invalid: None,
            };
            if self.streamed {
                self.inner
                    .signed_streamed(cx, "/fapi/v1/probe", Query::new(), pending);
            } else {
                self.inner.signed(
                    cx,
                    role(self.method),
                    self.method,
                    "/fapi/v1/probe",
                    Query::new().str("symbol", "BTCUSDT"),
                    pending,
                );
            }
        }
        fn book(&self, _: &str) -> Option<&Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }

    struct Harness {
        engine: Engine<Probe>,
        now: f64,
        requests: Vec<HttpRequest>,
        timers: Vec<(TimerId, f64)>,
        done: Vec<Result<String, String>>,
    }

    impl Harness {
        fn new(method: Method, v3: bool) -> Self {
            let options = if v3 {
                Options::v3_wallet()
            } else {
                Options::v1_with_read_agent()
            };
            let (inner, signer) = options.build();
            let mut engine = safe_cex_engine::test_support::seeded_engine(Probe {
                inner,
                method,
                streamed: false,
                clock_samples: Vec::new(),
            });
            engine.set_signer(Some(signer)).unwrap();
            let mut h = Self {
                engine,
                now: 1_700_000_000_000.0,
                requests: Vec::new(),
                timers: Vec::new(),
                done: Vec::new(),
            };
            h.engine.start(h.now);
            h.command();
            h
        }
        fn command(&mut self) {
            self.engine.handle(
                self.now,
                Event::Command(Command::FetchCandles {
                    command: 7,
                    symbol: "BTCUSDT".into(),
                    interval: safe_cex_engine::model::Interval::M1,
                    limit: None,
                    from_ms: None,
                    to_ms: None,
                }),
            );
            self.collect();
        }
        fn collect(&mut self) {
            for action in
                safe_cex_engine::test_support::prepared_actions(&mut self.engine, self.now)
            {
                match action {
                    Action::Http(request) => self.requests.push(request),
                    Action::SetTimer { timer, at_ms } => {
                        self.timers.push((timer, at_ms - self.now))
                    }
                    Action::ClearTimer { timer } => self.timers.retain(|(id, _)| *id != timer),
                    Action::Notice(Notice::CommandDone { result, .. }) => {
                        self.done.push(Ok(result))
                    }
                    Action::Notice(Notice::CommandFailed { error, .. }) => {
                        self.done.push(Err(error.legacy_text()))
                    }
                    _ => {}
                }
            }
        }
        fn reply(&mut self, request: HttpRequest, status: u16, body: &str) {
            self.now += 1.0;
            self.engine.handle(
                self.now,
                Event::HttpResponse {
                    request: request.id,
                    response: Response {
                        status,
                        headers: "",
                        body,
                    },
                },
            );
            self.collect();
        }
        fn fail(&mut self, request: HttpRequest) {
            self.now += 1.0;
            self.engine.handle(
                self.now,
                Event::HttpFailed {
                    request: request.id,
                    error: "connection lost",
                },
            );
            self.collect();
        }
    }

    #[test]
    fn ambiguous_writes_never_retry() {
        for method in [Method::Post, Method::Put, Method::Delete] {
            for network in [false, true] {
                let mut h = Harness::new(method, false);
                let request = h.requests.remove(0);
                if network {
                    h.fail(request);
                } else {
                    h.reply(request, 503, "upstream timeout");
                }
                assert!(h.requests.is_empty());
                assert!(h.timers.is_empty());
                assert_eq!(h.done.len(), 1);
                assert!(h.done[0].is_err());
            }
        }
    }

    #[test]
    fn read_retries_three_times_with_linear_delays_and_fresh_signatures() {
        let mut h = Harness::new(Method::Get, false);
        for attempt in 1..=3 {
            let request = h.requests.remove(0);
            let old_query = request.url;
            // Reconstruct the action after retaining its URL for comparison.
            let request = HttpRequest {
                url: old_query.clone(),
                ..request
            };
            h.reply(request, 503, "upstream timeout");
            assert!(h.requests.is_empty());
            let (timer, delay) = h.timers.remove(0);
            assert_eq!(delay, f64::from(attempt) * 1000.0);
            h.now += delay;
            h.engine.handle(h.now, Event::TimerFired { timer });
            h.collect();
            assert_eq!(h.requests.len(), 1);
            assert_ne!(h.requests[0].url, old_query);
        }
        let request = h.requests.remove(0);
        h.reply(request, 503, "upstream timeout");
        assert!(h.requests.is_empty());
        assert!(h.timers.is_empty());
        assert_eq!(h.done.len(), 1);
    }

    #[test]
    fn v1_signature_rejection_resends_once_but_v3_never_does() {
        for v3 in [false, true] {
            let mut h = Harness::new(Method::Post, v3);
            let request = h.requests.remove(0);
            h.reply(
                request,
                200,
                r#"{"code":"-1022","msg":"Signature for this request is not valid."}"#,
            );
            if v3 {
                assert!(h.requests.is_empty());
            } else {
                assert_eq!(h.requests.len(), 1);
                let request = h.requests.remove(0);
                h.reply(
                    request,
                    400,
                    r#"{"code":-1022,"msg":"Signature for this request is not valid."}"#,
                );
                assert!(h.requests.is_empty());
            }
            assert_eq!(h.done.len(), 1);
        }
    }

    #[test]
    fn timestamp_rejection_resyncs_with_alternate_version_and_resends_once() {
        let mut h = Harness::new(Method::Post, false);
        let request = h.requests.remove(0);
        h.reply(
            request,
            400,
            r#"{"code":-1021,"msg":"Timestamp for this request was outside of the recvWindow."}"#,
        );
        let clock = h.requests.remove(0);
        assert!(clock.url.ends_with("/fapi/v1/time"));
        h.reply(clock, 404, "not found");
        let clock = h.requests.remove(0);
        assert!(clock.url.ends_with("/fapi/v3/time"));
        h.reply(clock, 200, r#"{"serverTime":"1700000012345"}"#);
        let request = h.requests.remove(0);
        assert!(request.url.contains("timestamp=1700000012345"));
        h.reply(
            request,
            400,
            r#"{"code":-1021,"msg":"Timestamp for this request was outside of the recvWindow."}"#,
        );
        assert!(h.requests.is_empty());
        assert_eq!(h.done.len(), 1);
    }

    #[test]
    fn read_credentials_are_used_only_for_get_requests() {
        for method in [Method::Get, Method::Post, Method::Put, Method::Delete] {
            let mut h = Harness::new(method, false);
            let request = h.requests.remove(0);
            let expected = if method == Method::Get {
                "read-key"
            } else {
                "primary-key"
            };
            assert_eq!(request.headers, vec![("X-MBX-APIKEY", expected.into())]);
            assert!(request.body.is_none());
            assert_eq!(request.timeout_ms, None);
        }
    }

    #[test]
    fn read_policy_includes_listen_key_sessions_but_not_order_writes() {
        use crate::account::Request;
        let signed = |method, path: &str, pending| SignedRequest {
            method,
            path: path.into(),
            query: Query::new(),
            pending,
            stream: None,
            network_retries: 0,
            timestamp_retried: false,
            signature_retried: false,
            version: Version::V1,
            public: false,
            role: CredentialRole::Mutation,
            context: None,
            read: None,
        };
        let klines = || Pending::MarketBrackets {
            rows: HashMap::new(),
        };
        let create = Pending::Account(Request::ListenKey { epoch: 0 });
        let keepalive = Pending::Account(Request::Keepalive { epoch: 0 });
        assert!(signed(Method::Post, "/fapi/v3/listenKey", create).is_read());
        assert!(signed(Method::Put, "/fapi/v3/listenKey", keepalive).is_read());
        // The listenKey path alone grants nothing; the session write does.
        assert!(!signed(Method::Post, "/fapi/v3/listenKey", klines()).is_read());
        assert!(!signed(Method::Delete, "/fapi/v3/order", klines()).is_read());
        assert!(!signed(Method::Post, "/fapi/v3/order", klines()).is_read());
    }

    #[test]
    fn clock_exhausts_direct_versions_before_public_cors_and_only_then_releases_writes() {
        for v3 in [false, true] {
            let mut h = Harness::new(Method::Post, v3);
            let request = h.requests.remove(0);
            h.reply(request, 400, r#"{"code":-1021,"msg":"Timestamp rejected"}"#);
            for attempt in 0..4 {
                assert_eq!(
                    h.requests.len(),
                    1,
                    "only the clock may run during recovery"
                );
                let clock = h.requests.remove(0);
                let expected_version = if (attempt % 2 == 0) == v3 { "v3" } else { "v1" };
                assert!(
                    clock
                        .url
                        .ends_with(&format!("/fapi/{expected_version}/time"))
                );
                assert_eq!(
                    clock.route,
                    Some(if attempt < 2 {
                        RequestRoute::Direct
                    } else {
                        RequestRoute::PublicCors
                    })
                );
                assert!(clock.headers.is_empty());
                assert_eq!(clock.timeout_ms, Some(5_000));
                if attempt < 3 {
                    h.fail(clock);
                } else {
                    h.reply(clock, 200, r#"{"serverTime":1700000015000}"#);
                }
            }
            assert_eq!(h.requests.len(), 1);
            assert!(h.requests[0].url.contains("timestamp=1700000015000"));
        }
    }

    #[test]
    fn time_response_updates_shared_protocol_clock_before_draining_v1_and_v3() {
        for v3 in [false, true] {
            let mut h = Harness::new(Method::Post, v3);
            let request = h.requests.remove(0);
            h.reply(request, 400, r#"{"code":-1021,"msg":"Timestamp rejected"}"#);
            let clock = h.requests.remove(0);
            assert!(clock.headers.is_empty());
            assert_eq!(clock.timeout_ms, Some(5_000));
            h.reply(clock, 200, r#"{"serverTime":1700000015000}"#);
            assert_eq!(
                h.engine.venue().clock_samples.last(),
                Some(&(h.now, 1_700_000_015_000.0)),
                "calibration must be visible through the shared clock in this callback"
            );
            assert_eq!(
                h.engine.venue().inner.requests.last_sync_success,
                Some(h.now)
            );
            assert_eq!(h.timers.last().unwrap().1, CLOCK_INTERVAL_MS);
            assert_eq!(
                h.requests.len(),
                1,
                "queued request drains in the same callback"
            );
            if v3 {
                let nonce: u64 = h.requests[0]
                    .url
                    .split("nonce=")
                    .nth(1)
                    .unwrap()
                    .split('&')
                    .next()
                    .unwrap()
                    .parse()
                    .unwrap();
                assert!(nonce >= 1_700_000_015_000_000);
            } else {
                assert!(h.requests[0].url.contains("timestamp=1700000015000"));
            }
        }
    }

    #[test]
    fn later_clock_observation_replaces_the_offset_without_moving_raw_sync_age() {
        let mut h = Harness::new(Method::Post, false);
        let request = h.requests.remove(0);
        h.reply(request, 400, r#"{"code":-1021,"msg":"Timestamp rejected"}"#);
        let clock = h.requests.remove(0);
        h.reply(clock, 200, r#"{"serverTime":1700000015000}"#);
        let request = h.requests.remove(0);
        h.reply(request, 200, "{}");
        let (timer, delay) = h.timers.remove(0);
        h.now += delay;
        h.engine.handle(h.now, Event::TimerFired { timer });
        h.collect();
        let clock = h.requests.remove(0);
        h.reply(clock, 200, r#"{"serverTime":1700000400000}"#);
        assert_eq!(
            h.engine.venue().clock_samples.last(),
            Some(&(h.now, 1_700_000_400_000.0))
        );
        assert_eq!(
            h.engine.venue().inner.requests.last_sync_success,
            Some(h.now)
        );
        assert_eq!(h.timers.last().unwrap().1, CLOCK_INTERVAL_MS);
    }

    #[test]
    fn failed_periodic_clock_sync_retains_the_last_successful_offset() {
        let mut h = Harness::new(Method::Post, false);
        let request = h.requests.remove(0);
        h.reply(request, 400, r#"{"code":-1021,"msg":"Timestamp rejected"}"#);
        let clock = h.requests.remove(0);
        h.reply(clock, 200, r#"{"serverTime":1700000015000}"#);
        let request = h.requests.remove(0);
        h.reply(request, 200, "{}");
        let (raw, protocol) = *h.engine.venue().clock_samples.last().unwrap();
        let saved = protocol - raw;
        assert_ne!(saved, 0.0);
        let (timer, delay) = h.timers.remove(0);
        assert_eq!(delay, CLOCK_INTERVAL_MS);
        h.now += delay;
        h.engine.handle(h.now, Event::TimerFired { timer });
        h.collect();
        for _ in 0..4 {
            let clock = h.requests.remove(0);
            h.reply(clock, 200, r#"{"serverTime":"not-a-time"}"#);
        }
        assert!(h.requests.is_empty());
        let (raw, protocol) = *h.engine.venue().clock_samples.last().unwrap();
        assert_eq!(protocol - raw, saved);
        h.engine.handle(
            h.now,
            Event::Command(Command::FetchCandles {
                command: 8,
                symbol: "BTCUSDT".into(),
                interval: safe_cex_engine::model::Interval::M1,
                limit: None,
                from_ms: None,
                to_ms: None,
            }),
        );
        h.collect();
        // Failed synchronization leaves the last success stale, so a new signed
        // request must exhaust its own bounded clock cycle before it drains.
        for _ in 0..4 {
            assert_eq!(h.requests.len(), 1);
            let clock = h.requests.remove(0);
            assert!(clock.url.ends_with("/time"));
            assert!(clock.headers.is_empty());
            assert_eq!(clock.timeout_ms, Some(5_000));
            h.reply(clock, 200, r#"{"serverTime":"not-a-time"}"#);
        }
        assert_eq!(h.requests.len(), 1);
        assert!(h.requests[0].url.contains("/fapi/v1/probe?"));
        let (raw, protocol) = *h.engine.venue().clock_samples.last().unwrap();
        assert_eq!(raw, h.now);
        assert_eq!(protocol - raw, saved);
        assert!(h.requests[0].url.contains(&format!(
            "timestamp={}",
            safe_cex_engine::num::format(protocol)
        )));
    }

    struct PublicHarness {
        engine: Engine<AsterDex>,
        now: f64,
        requests: Vec<HttpRequest>,
        timers: Vec<(TimerId, f64)>,
    }
    impl PublicHarness {
        fn new() -> Self {
            let mut h = Self {
                engine: safe_cex_engine::test_support::seeded_engine(AsterDex::new(
                    crate::Options {
                        public_data_only: true,
                        ..Default::default()
                    },
                )),
                now: 1700000000000.0,
                requests: Vec::new(),
                timers: Vec::new(),
            };
            h.engine.start(h.now);
            h.collect();
            let clock = h.take("/time");
            h.reply(clock, 200, r#"{"serverTime":1700000000000}"#);
            h
        }
        fn collect(&mut self) {
            for action in
                safe_cex_engine::test_support::prepared_actions(&mut self.engine, self.now)
            {
                match action {
                    Action::Http(request) => self.requests.push(request),
                    Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                    Action::ClearTimer { timer } => self.timers.retain(|(id, _)| *id != timer),
                    _ => {}
                }
            }
        }
        fn take(&mut self, suffix: &str) -> HttpRequest {
            let index = self
                .requests
                .iter()
                .position(|request| request.url.ends_with(suffix))
                .expect("expected public request");
            self.requests.remove(index)
        }
        fn event(&mut self, event: Event<'_>) {
            self.engine.handle(self.now, event);
            self.collect();
        }
        fn reply(&mut self, request: HttpRequest, status: u16, body: &str) {
            self.event(Event::HttpResponse {
                request: request.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            });
        }
        fn retry(&mut self, delay: f64) {
            let index = self
                .timers
                .iter()
                .position(|(_, at)| jittered(*at - self.now, delay))
                .expect("retry timer");
            let (timer, at) = self.timers.remove(index);
            self.now = at;
            self.event(Event::TimerFired { timer });
        }
    }

    #[test]
    fn public_bootstrap_network_retry_discards_partial_rows_before_committing() {
        let mut h = PublicHarness::new();
        let request = h.take("/exchangeInfo");
        assert!(request.headers.is_empty());
        // The managed read's own bound.
        assert_eq!(request.timeout_ms, Some(READ_TIMEOUT_MS));
        h.event(Event::HttpStart {
            request: request.id,
            status: 200,
            headers: "",
        });
        h.event(Event::HttpChunk { request: request.id, data: br#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","pricePrecision":2,"quantityPrecision":3},"# });
        h.event(Event::HttpFailed {
            request: request.id,
            error: "connection reset",
        });
        assert!(h.engine.store().markets().is_empty());
        assert!(h.requests.is_empty());
        h.retry(1000.0);
        let request = h.take("/exchangeInfo");
        assert!(request.headers.is_empty());
        assert!(!request.url.contains("signature"));
        h.reply(request, 200, r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","pricePrecision":2,"quantityPrecision":3}]}"#);
        assert_eq!(
            h.engine.store().markets().len(),
            1,
            "failed attempt's row cannot leak into the replacement"
        );
        assert!(h.engine.store().loaded().markets);
    }

    #[test]
    fn public_bootstrap_uses_capped_discovery_backoff_until_recovery() {
        for (status, body) in [
            (503, "upstream timeout"),
            (503, r#"{"code":-1000,"msg":"Internal error"}"#),
            (429, r#"{"code":-1003,"msg":"Too many requests"}"#),
        ] {
            let mut h = PublicHarness::new();
            for delay in [1000., 2000., 4000., 8000., 16000., 30000., 30000.] {
                let request = h.take("/exchangeInfo");
                h.reply(request, status, body);
                assert!(h.requests.is_empty());
                h.retry(delay);
            }
            let request = h.take("/exchangeInfo");
            h.reply(request, status, body);
            assert!(h.requests.is_empty());
            assert!(h.engine.store().markets().is_empty());
            assert!(!h.engine.store().loaded().markets);
        }
    }

    #[test]
    fn public_business_error_never_commits_a_successful_empty_bootstrap() {
        let mut h = PublicHarness::new();
        let request = h.take("/exchangeInfo");
        h.reply(request, 200, r#"{"code":-1121,"msg":"Invalid symbol"}"#);
        assert!(h.requests.is_empty());
        assert!(h.engine.store().markets().is_empty());
        assert!(!h.engine.store().loaded().markets);
    }
}
