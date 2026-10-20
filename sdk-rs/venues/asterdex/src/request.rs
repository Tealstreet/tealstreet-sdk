//! Public and signed request lifecycle. Retries rebuild unsigned parameters; ambiguous
//! trading writes are never repeated after network or upstream failure.
use std::collections::{HashMap, VecDeque};

use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::{Cx, LogLevel, Method, RequestRoute, Response, TimerId, Venue};
use serde::Deserialize;

use crate::sign::{Query, Version};
use crate::{AsterDex, Pending, reset_pending};

const CLOCK_INTERVAL_MS: f64 = 300_000.0;

#[derive(Default)]
pub struct RequestState {
    pub offset_ms: f64,
    last_sync_success: Option<f64>,
    clock_running: bool,
    startup_pending: bool,
    clock_timer: Option<TimerId>,
    retries: HashMap<TimerId, Box<SignedRequest>>,
    awaiting_clock: VecDeque<Box<SignedRequest>>,
    public_queue: VecDeque<Box<SignedRequest>>,
    public_used: u8,
    public_timer: Option<TimerId>,
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
}

#[derive(Clone, Copy)]
pub struct ClockRequest {
    attempt: u8,
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
    fn is_read(&self) -> bool {
        self.method == Method::Get
            || self
                .path
                .split('?')
                .next()
                .unwrap_or(&self.path)
                .ends_with("/listenKey")
    }

    fn reset(&mut self) {
        reset_pending(&mut self.pending);
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
        method: Method,
        path: &str,
        query: Query,
        pending: Pending,
    ) -> bool {
        self.queue_signed(cx, method, path, query, pending, None)
    }

    pub(crate) fn signed_streamed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        path: &str,
        query: Query,
        pending: Pending,
    ) -> bool {
        self.queue_signed(cx, Method::Get, path, query, pending, Some(ArrayAt::Root))
    }

    pub(crate) fn public_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        path: &str,
        pending: Pending,
        stream: Option<ArrayAt>,
    ) {
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
        });
        self.dispatch_signed(cx, request);
    }

    fn queue_signed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
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
        // Public market data shares the TS public client's budget. Its optional
        // unauthenticated leverage-bracket fallback still belongs to primaryAPI.
        if request.public && !matches!(&request.pending, Pending::MarketBrackets { .. }) {
            self.requests.public_queue.push_back(request);
            self.drain_public(cx);
            true
        } else {
            self.issue_request(cx, request)
        }
    }

    fn drain_public(&mut self, cx: &mut Cx<'_, Pending>) {
        while self.requests.public_used < 10 {
            let Some(request) = self.requests.public_queue.pop_front() else {
                break;
            };
            if self.requests.public_used == 0 {
                self.requests.public_timer = Some(cx.set_timer(1000.0));
            }
            self.requests.public_used += 1;
            self.issue_request(cx, request);
        }
    }

    fn issue_request(&mut self, cx: &mut Cx<'_, Pending>, mut request: Box<SignedRequest>) -> bool {
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
        let credentials = if request.method == Method::Get {
            self.read_credentials.as_ref().or(self.credentials.as_ref())
        } else {
            self.credentials.as_ref()
        };
        let Some(credentials) = credentials else {
            self.on_request_failed(cx, request.pending, "Missing required Aster credentials");
            return false;
        };
        request.version = credentials.version;
        let query = match request.query.clone().signed(
            credentials,
            request.method,
            &request.path,
            cx.now + self.requests.offset_ms,
            self.options.testnet,
        ) {
            Ok(query) => query,
            Err(error) => {
                self.on_request_failed(cx, request.pending, &error);
                return false;
            }
        };
        let url = format!("{}{}?{query}", self.urls.rest, request.path);
        let headers = vec![("X-MBX-APIKEY", credentials.key.clone())];
        let method = request.method;
        let stream = request.stream;
        match stream {
            Some(at) => {
                cx.request_streamed(method, url, headers, at, Pending::Signed(request));
            }
            None => {
                cx.request(method, url, headers, None, Pending::Signed(request));
            }
        }
        true
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
            if request.retryable_status(response.status)
                && request.is_read()
                && request.network_retries < 3
            {
                self.schedule_retry(cx, request);
                return;
            }
            if request.public && code.is_some_and(|code| code < 0.0) {
                self.on_request_failed(
                    cx,
                    request.pending,
                    &format!("Aster public API error: {message}"),
                );
                return;
            }
            self.on_response(cx, request.pending, response);
        } else {
            let error = response.unwrap_err();
            if request.is_read() && request.network_retries < 3 {
                self.schedule_retry(cx, request);
            } else {
                self.on_request_failed(cx, request.pending, error);
            }
        }
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
        self.issue_clock(cx, ClockRequest { attempt: 0 });
    }

    fn issue_clock(&self, cx: &mut Cx<'_, Pending>, request: ClockRequest) {
        let version = if request.attempt % 2 == 1 {
            match self.version {
                Version::V1 => Version::V3,
                Version::V3 => Version::V1,
            }
        } else {
            self.version
        };
        let attempt = request.attempt;
        let id = cx.request(
            Method::Get,
            format!("{}{}/time", self.urls.rest, version.prefix()),
            Vec::new(),
            None,
            Pending::Clock(request),
        );
        cx.route_request(
            id,
            if attempt < 2 {
                RequestRoute::Direct
            } else {
                RequestRoute::PublicCors
            },
        );
        cx.request_timeout(id, 5_000);
    }

    pub(crate) fn on_clock_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: ClockRequest,
        response: Result<Response<'_>, &str>,
    ) {
        let time = response
            .ok()
            .filter(|r| (200..300).contains(&r.status))
            .and_then(|r| serde_json::from_str::<ServerTime>(r.body).ok())
            .and_then(|r| r.server_time.value())
            .filter(|time| *time > 0.0);
        if let Some(time) = time {
            self.requests.offset_ms = time - cx.now;
            self.requests.last_sync_success = Some(cx.now);
        } else if request.attempt < 3 {
            self.issue_clock(
                cx,
                ClockRequest {
                    attempt: request.attempt + 1,
                },
            );
            return;
        } else {
            cx.log(
                LogLevel::Warning,
                "Aster time sync failed; retaining the last clock offset",
            );
        }
        self.requests.clock_running = false;
        self.requests.clock_timer = Some(cx.set_timer(CLOCK_INTERVAL_MS));
        let queued = std::mem::take(&mut self.requests.awaiting_clock);
        for request in queued {
            self.dispatch_signed(cx, request);
        }
        if self.requests.startup_pending {
            self.requests.startup_pending = false;
            self.bootstrap(cx);
        }
    }

    pub(crate) fn on_request_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if self.requests.public_timer == Some(timer) {
            self.requests.public_timer = None;
            self.requests.public_used = 0;
            self.drain_public(cx);
            return true;
        }
        if let Some(request) = self.requests.retries.remove(&timer) {
            self.dispatch_signed(cx, request);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Options;
    use safe_cex_engine::book::Book;
    use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, SocketId};

    struct Probe {
        inner: AsterDex,
        method: Method,
        streamed: bool,
        public: bool,
    }

    impl Venue for Probe {
        type Pending = Pending;
        fn start(&mut self, cx: &mut Cx<'_, Pending>) {
            self.inner.requests.last_sync_success = Some(cx.now);
            if self.public {
                for command in 0..11 {
                    self.inner.public_read(
                        cx,
                        &format!("/fapi/v1/public-{command}"),
                        Pending::Klines {
                            command,
                            rows: Vec::new(),
                            malformed: false,
                        },
                        Some(ArrayAt::Root),
                    );
                }
                return;
            }
            self.inner.signed(
                cx,
                self.method,
                "/fapi/v1/probe",
                Query::new().str("symbol", "BTCUSDT"),
                Pending::Klines {
                    command: 7,
                    rows: Vec::new(),
                    malformed: false,
                },
            );
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
                Pending::Clock(request) => self.inner.on_clock_response(cx, request, Ok(response)),
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
                Pending::Clock(request) => self.inner.on_clock_response(cx, request, Err(error)),
                _ => panic!("unexpected probe failure"),
            }
        }
        fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
            self.inner.on_request_timer(cx, timer);
        }
        fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
            if self.public {
                if let Command::FetchLeverage { command, symbol } = command {
                    self.inner.public_read(
                        cx,
                        &format!("/fapi/v1/{symbol}"),
                        Pending::Klines {
                            command,
                            rows: Vec::new(),
                            malformed: false,
                        },
                        Some(ArrayAt::Root),
                    );
                }
                return;
            }
            let pending = Pending::Klines {
                command: 7,
                rows: Vec::new(),
                malformed: false,
            };
            if self.streamed {
                self.inner
                    .signed_streamed(cx, "/fapi/v1/probe", Query::new(), pending);
            } else {
                self.inner
                    .signed(cx, self.method, "/fapi/v1/probe", Query::new(), pending);
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
                Options {
                    key: Some("0x21cF8Ae13Bb72632562c6Fff438652Ba1a151bb0".into()),
                    secret: Some(
                        "4fd0a42218f3eae43a6ce26d22544e986139a01e5b34a62db53757ffca81bae1".into(),
                    ),
                    application_id: Some("0x63DD5aCC6b1aa0f563956C0e534DD30B6dcF7C4e".into()),
                    ..Options::default()
                }
            } else {
                Options {
                    key: Some("primary-key".into()),
                    secret: Some("primary-secret".into()),
                    read_key: Some("read-key".into()),
                    read_secret: Some("read-secret".into()),
                    ..Options::default()
                }
            };
            let mut h = Self {
                engine: Engine::new(Probe {
                    inner: AsterDex::new(options),
                    method,
                    streamed: false,
                    public: false,
                }),
                now: 1_700_000_000_000.0,
                requests: Vec::new(),
                timers: Vec::new(),
                done: Vec::new(),
            };
            h.engine.start(h.now);
            h.collect();
            h
        }
        fn collect(&mut self) {
            for action in self.engine.drain_actions() {
                match action {
                    Action::Http(request) => self.requests.push(request),
                    Action::SetTimer { timer, at_ms } => {
                        self.timers.push((timer, at_ms - self.now))
                    }
                    Action::Notice(Notice::CommandDone { result, .. }) => self.done.push(result),
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
    fn read_policy_includes_listen_key_but_not_order_deletion() {
        let request = |method, path: &str| SignedRequest {
            method,
            path: path.into(),
            query: Query::new(),
            pending: Pending::Klines {
                command: 7,
                rows: Vec::new(),
                malformed: false,
            },
            stream: None,
            network_retries: 0,
            timestamp_retried: false,
            signature_retried: false,
            version: Version::V1,
            public: false,
        };
        for method in [Method::Post, Method::Put, Method::Delete] {
            assert!(request(method, "/fapi/v3/listenKey").is_read());
        }
        assert!(!request(Method::Delete, "/fapi/v3/order").is_read());
        assert!(!request(Method::Post, "/fapi/v3/order").is_read());
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
    fn failed_periodic_clock_sync_retains_the_last_successful_offset() {
        let mut h = Harness::new(Method::Post, false);
        let request = h.requests.remove(0);
        h.reply(request, 400, r#"{"code":-1021,"msg":"Timestamp rejected"}"#);
        let clock = h.requests.remove(0);
        h.reply(clock, 200, r#"{"serverTime":1700000015000}"#);
        let request = h.requests.remove(0);
        h.reply(request, 200, "{}");
        let saved = h.engine.venue().inner.requests.offset_ms;
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
        assert_eq!(h.engine.venue().inner.requests.offset_ms, saved);
    }

    fn public_probe() -> Engine<Probe> {
        Engine::new(Probe {
            inner: AsterDex::new(Options {
                public_data_only: true,
                ..Options::default()
            }),
            method: Method::Get,
            streamed: true,
            public: true,
        })
    }

    #[test]
    fn public_budget_is_ten_per_timer_window_fifo_and_does_not_refund_failures() {
        let mut engine = public_probe();
        engine.start(0.0);
        let mut first = Vec::new();
        let mut reset = None;
        for action in engine.drain_actions() {
            match action {
                Action::Http(request) => first.push(request),
                Action::SetTimer { timer, at_ms } => {
                    assert_eq!(at_ms, 1000.0);
                    reset = Some(timer);
                }
                _ => {}
            }
        }
        assert_eq!(first.len(), 10);
        assert_eq!(engine.venue().inner.requests.public_queue.len(), 1);
        assert!(first.iter().all(|request| request.headers.is_empty()));
        let failed = first.remove(0);
        // This row must not survive the retry, even though the failed attempt
        // already delivered it through the streamed selector.
        engine.handle(
            100.0,
            Event::HttpStart {
                request: failed.id,
                status: 200,
                headers: "",
            },
        );
        engine.handle(
            100.0,
            Event::HttpChunk {
                request: failed.id,
                data: br#"[[1000,"1","2","0.5","1.5","3",0,"0",1,"0","0","0"],"#,
            },
        );
        engine.handle(
            100.0,
            Event::HttpFailed {
                request: failed.id,
                error: "connection reset",
            },
        );
        let retry = engine
            .drain_actions()
            .find_map(|action| match action {
                Action::SetTimer { timer, at_ms } => {
                    assert_eq!(at_ms, 1100.0);
                    Some(timer)
                }
                Action::Http(_) => {
                    panic!("failure must neither refund a slot nor retry immediately")
                }
                _ => None,
            })
            .unwrap();
        engine.handle(
            999.0,
            Event::Command(Command::FetchLeverage {
                command: 99,
                symbol: "later".into(),
            }),
        );
        assert!(
            !engine
                .drain_actions()
                .any(|action| matches!(action, Action::Http(_)))
        );
        let reset = reset.unwrap();
        engine.handle(1000.0, Event::TimerFired { timer: reset });
        let mut admitted = Vec::new();
        for action in engine.drain_actions() {
            match action {
                Action::Http(request) => admitted.push(request),
                Action::SetTimer { at_ms, .. } => assert_eq!(at_ms, 2000.0),
                _ => {}
            }
        }
        assert_eq!(admitted.len(), 2);
        assert!(admitted[0].url.ends_with("/public-10"));
        assert!(admitted[1].url.ends_with("/later"));
        engine.handle(1001.0, Event::TimerFired { timer: reset });
        assert_eq!(
            engine.venue().inner.requests.public_used,
            2,
            "old timer cannot reset the current window"
        );
        assert_eq!(engine.drain_actions().count(), 0);
        engine.handle(1100.0, Event::TimerFired { timer: retry });
        let retry = engine
            .drain_actions()
            .find_map(|action| match action {
                Action::Http(request) => Some(request),
                _ => None,
            })
            .unwrap();
        assert!(retry.url.ends_with("/public-0"));
        assert_eq!(
            engine.venue().inner.requests.public_used,
            3,
            "a retry consumes a fresh slot"
        );
        assert!(retry.headers.is_empty());
        engine.handle(
            1100.0,
            Event::HttpStart {
                request: retry.id,
                status: 200,
                headers: "",
            },
        );
        engine.handle(
            1100.0,
            Event::HttpChunk {
                request: retry.id,
                data: br#"[[2000,"2","3","1","2.5","4",0,"0",1,"0","0","0"]]"#,
            },
        );
        engine.handle(1100.0, Event::HttpEnd { request: retry.id });
        let result = engine
            .drain_actions()
            .find_map(|action| match action {
                Action::Notice(Notice::CommandDone { command: 0, result }) => Some(result.unwrap()),
                _ => None,
            })
            .unwrap();
        #[derive(Deserialize)]
        struct Row {
            timestamp: f64,
        }
        let rows: Vec<Row> = serde_json::from_str(&result).unwrap();
        assert_eq!(rows.len(), 1, "partial failed rows must be reset");
        assert_eq!(rows[0].timestamp, 2.0);
    }

    #[test]
    fn public_retry_waiting_at_boundary_stays_behind_fifo_and_next_window_is_bounded() {
        let mut engine = public_probe();
        engine.start(0.0);
        let mut first = Vec::new();
        let mut reset = None;
        for action in engine.drain_actions() {
            match action {
                Action::Http(request) => first.push(request),
                Action::SetTimer { timer, .. } => reset = Some(timer),
                _ => {}
            }
        }
        engine.handle(
            0.0,
            Event::HttpResponse {
                request: first[0].id,
                response: Response {
                    status: 503,
                    headers: "",
                    body: "upstream timeout",
                },
            },
        );
        let retry = engine
            .drain_actions()
            .find_map(|action| match action {
                Action::SetTimer { timer, .. } => Some(timer),
                _ => None,
            })
            .unwrap();
        // Exercise either ordering of callbacks with the same deadline: the
        // retry joins the existing queue without bypassing its older request.
        engine.handle(1000.0, Event::TimerFired { timer: retry });
        assert_eq!(engine.drain_actions().count(), 0);
        for command in 20..30 {
            engine.handle(
                1000.0,
                Event::Command(Command::FetchLeverage {
                    command,
                    symbol: format!("later-{command}"),
                }),
            );
        }
        assert_eq!(engine.drain_actions().count(), 0);
        engine.handle(
            1000.0,
            Event::TimerFired {
                timer: reset.unwrap(),
            },
        );
        let mut admitted = Vec::new();
        let mut next_reset = None;
        for action in engine.drain_actions() {
            match action {
                Action::Http(request) => admitted.push(request),
                Action::SetTimer { timer, at_ms } => {
                    assert_eq!(at_ms, 2000.0);
                    next_reset = Some(timer);
                }
                _ => {}
            }
        }
        assert_eq!(admitted.len(), 10);
        assert!(admitted[0].url.ends_with("/public-10"));
        assert!(admitted[1].url.ends_with("/public-0"));
        assert!(admitted[9].url.ends_with("/later-27"));
        assert_eq!(engine.venue().inner.requests.public_queue.len(), 2);
        engine.handle(
            2000.0,
            Event::TimerFired {
                timer: next_reset.unwrap(),
            },
        );
        let admitted: Vec<_> = engine
            .drain_actions()
            .filter_map(|action| match action {
                Action::Http(request) => Some(request),
                _ => None,
            })
            .collect();
        assert_eq!(admitted.len(), 2);
        assert!(admitted[0].url.ends_with("/later-28"));
        assert!(admitted[1].url.ends_with("/later-29"));
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
                engine: Engine::new(AsterDex::new(Options {
                    public_data_only: true,
                    ..Options::default()
                })),
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
            for action in self.engine.drain_actions() {
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
            let target = self.now + delay;
            assert!(
                self.timers.iter().any(|(timer, at)| *at == target
                    && self.engine.venue().requests.retries.contains_key(timer)),
                "retry timer"
            );
            // A quota reset can share the retry deadline. Deliver both in
            // causal order instead of assuming the retry is the only timer.
            while let Some((index, _)) = self
                .timers
                .iter()
                .enumerate()
                .filter(|(_, (_, at))| *at <= target)
                .min_by(|(_, (_, a)), (_, (_, b))| a.total_cmp(b))
            {
                let (timer, at) = self.timers.remove(index);
                self.now = at;
                self.event(Event::TimerFired { timer });
            }
            self.now = target;
        }
    }

    #[test]
    fn public_bootstrap_network_retry_discards_partial_rows_before_committing() {
        let mut h = PublicHarness::new();
        let request = h.take("/exchangeInfo");
        assert!(request.headers.is_empty());
        assert_eq!(request.timeout_ms, None);
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
    fn public_bootstrap_has_exactly_three_bounded_status_retries() {
        for (status, body) in [
            (503, "upstream timeout"),
            (503, r#"{"code":-1000,"msg":"Internal error"}"#),
            (429, r#"{"code":-1003,"msg":"Too many requests"}"#),
        ] {
            let mut h = PublicHarness::new();
            for attempt in 1..=3 {
                let request = h.take("/exchangeInfo");
                h.reply(request, status, body);
                assert!(h.requests.is_empty());
                h.retry(f64::from(attempt) * 1000.0);
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
