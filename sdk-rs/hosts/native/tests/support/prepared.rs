//! Explicit preparation for test transports; never a production fallback.
use safe_cex_engine::{HttpRequest, Method, RequestId, RequestRoute};
use safe_cex_native::{BoundHttp, HttpPrepareError, PreparedHttp};
use std::fmt;
use std::time::Duration;
use tokio::time::Instant;

pub struct Boundary {
    request: RequestId,
    method: Method,
    route: Option<RequestRoute>,
    origin: String,
    pub deadline: Instant,
}
impl Boundary {
    pub fn new(request: &HttpRequest) -> Result<Self, HttpPrepareError> {
        Ok(Self {
            request: request.id,
            method: request.method,
            route: request.route,
            origin: origin(&request.url)?,
            deadline: Instant::now()
                + Duration::from_millis(u64::from(request.timeout_ms.unwrap_or(20_000).max(1))),
        })
    }
    pub fn bind_with(
        self,
        bind: impl FnOnce(HttpRequest) -> Result<Box<dyn BoundHttp>, HttpPrepareError> + Send + 'static,
    ) -> Box<dyn PreparedHttp> {
        Box::new(Prepared {
            boundary: self,
            bind: Box::new(bind),
        })
    }
}
fn origin(url: &str) -> Result<String, HttpPrepareError> {
    let mut url =
        reqwest::Url::parse(url).map_err(|_| HttpPrepareError::Invalid("Invalid fixture URL"))?;
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.into())
}
type Binder = Box<dyn FnOnce(HttpRequest) -> Result<Box<dyn BoundHttp>, HttpPrepareError> + Send>;
struct Prepared {
    boundary: Boundary,
    bind: Binder,
}
impl fmt::Debug for Prepared {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RecordedPreparedHttp")
    }
}
impl PreparedHttp for Prepared {
    fn bind(self: Box<Self>, packet: HttpRequest) -> Result<Box<dyn BoundHttp>, HttpPrepareError> {
        if Instant::now() >= self.boundary.deadline {
            return Err(HttpPrepareError::Expired);
        }
        if packet.id != self.boundary.request
            || packet.method != self.boundary.method
            || packet.route != self.boundary.route
            || origin(&packet.url)? != self.boundary.origin
        {
            return Err(HttpPrepareError::Invalid("Fixture packet changed identity"));
        }
        (self.bind)(packet)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use safe_cex_native::Events;
    use std::sync::{Arc, Mutex};

    struct Recorded {
        packet: HttpRequest,
        sent: Arc<Mutex<Vec<HttpRequest>>>,
    }
    impl fmt::Debug for Recorded {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("RecordedControlBound")
        }
    }
    impl BoundHttp for Recorded {
        fn ready(&self) -> bool {
            true
        }
        fn dispatch(self: Box<Self>, _: Events) -> tokio::task::AbortHandle {
            self.sent.lock().unwrap().push(self.packet);
            tokio::spawn(async {}).abort_handle()
        }
    }
    fn request() -> HttpRequest {
        HttpRequest {
            id: RequestId(7),
            method: Method::Post,
            route: None,
            timeout_ms: Some(1_000),
            url: "https://recording.test/order".into(),
            headers: vec![("X-Fixture", "native".into())],
            signer_headers: vec![],
            body: Some("{}".into()),
        }
    }
    fn resource(
        request: &HttpRequest,
        sent: Arc<Mutex<Vec<HttpRequest>>>,
    ) -> Box<dyn PreparedHttp> {
        Boundary::new(request)
            .unwrap()
            .bind_with(move |packet| Ok(Box::new(Recorded { packet, sent })))
    }

    #[tokio::test]
    async fn recorded_preparation_and_bind_do_not_dispatch_or_rebuild_final_bytes() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let request = request();
        let prepared = resource(&request, sent.clone());
        assert!(sent.lock().unwrap().is_empty());
        assert_eq!(format!("{prepared:?}"), "RecordedPreparedHttp");
        let mut packet = request.clone();
        packet.url.push_str("?signature=fixture");
        packet
            .signer_headers
            .push(("X-Proof".into(), "fixture".into()));
        packet.body = Some("{\"amount\":\"1.00\"}".into());
        let bound = prepared.bind(packet.clone()).unwrap();
        assert!(sent.lock().unwrap().is_empty());
        let (events, _) = tokio::sync::mpsc::unbounded_channel();
        bound.dispatch(events);
        let sent = sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].id, packet.id);
        assert_eq!(sent[0].url, packet.url);
        assert_eq!(sent[0].headers, packet.headers);
        assert_eq!(sent[0].signer_headers, packet.signer_headers);
        assert_eq!(sent[0].body, packet.body);
    }

    #[tokio::test]
    async fn recorded_boundaries_reject_changes_before_mapping_and_drop_without_dispatch() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let request = request();
        for case in 0..4 {
            let mut changed = request.clone();
            match case {
                0 => changed.id = RequestId(8),
                1 => changed.method = Method::Get,
                2 => changed.route = Some(RequestRoute::Primary),
                _ => changed.url = "https://other.test/order".into(),
            }
            let prepared = Boundary::new(&request).unwrap().bind_with(|_| {
                panic!("invalid original boundary reached a mapping closure");
            });
            assert!(matches!(
                prepared.bind(changed),
                Err(HttpPrepareError::Invalid(_))
            ));
        }
        drop(resource(&request, sent.clone()));
        drop(resource(&request, sent.clone()).bind(request).unwrap());
        assert!(sent.lock().unwrap().is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn recorded_binding_keeps_the_preparation_deadline() {
        let request = request();
        let prepared = Boundary::new(&request).unwrap().bind_with(|_| {
            panic!("expired preparation reached binding");
        });
        tokio::time::advance(Duration::from_millis(1_000)).await;
        assert!(matches!(
            prepared.bind(request),
            Err(HttpPrepareError::Expired)
        ));
    }
}
