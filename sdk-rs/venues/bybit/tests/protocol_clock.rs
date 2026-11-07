//! Native adapter proof: rejection observations reach the shared engine clock.
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, Event, HttpRequest, Response, SocketId, TimerId,
    Venue, book::Book,
};
use safe_cex_venue_bybit::{BybitLinear, Pending};
mod support;
use support::Options;

struct Probe {
    inner: BybitLinear,
    samples: Vec<(f64, f64)>,
}
impl Venue for Probe {
    fn managed_sockets() -> bool {
        true
    }
    type Pending = Pending;
    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.inner.start(cx);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        self.inner.on_open(cx, socket);
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        self.inner.on_message(cx, socket, text);
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
        self.inner.on_close(cx, socket, code);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.inner.on_response(cx, pending, response);
        self.samples.push((cx.now, cx.protocol_now()));
    }
    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        response: Response<'_>,
    ) {
        self.inner.on_stream_error(cx, pending, response);
        self.samples.push((cx.now, cx.protocol_now()));
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.inner.on_request_failed(cx, pending, error);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.inner.on_timer(cx, timer);
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        self.inner.on_command(cx, command);
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.inner.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.inner.books()
    }
}

fn started() -> (Engine<Probe>, HttpRequest) {
    started_at(1_000.0)
}

fn started_at(now: f64) -> (Engine<Probe>, HttpRequest) {
    let (inner, signer) = Options {
        key: Some("clock-probe-key".into()),
        secret: Some("clock-probe-secret".into()),
        ..Options::default()
    }
    .build();
    let mut engine = safe_cex_engine::test_support::seeded_engine(Probe {
        inner,
        samples: Vec::new(),
    });
    engine.set_signer(Some(signer)).unwrap();
    engine.start(now);
    // The fee read is managed: the fixture driver completes its preparation.
    let request = safe_cex_engine::test_support::prepared_actions(&mut engine, now)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("/v5/account/fee-rate") => Some(request),
            _ => None,
        })
        .unwrap();
    (engine, request)
}

fn reject(engine: &mut Engine<Probe>, request: HttpRequest, server: &str) -> HttpRequest {
    let body = format!(
        r#"{{"retCode":10002,"retMsg":"recv_window server_timestamp[{server}]","result":{{}}}}"#
    );
    engine.handle(
        6_000.0,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: &body,
            },
        },
    );
    safe_cex_engine::test_support::prepared_actions(engine, 6_000.0)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("/v5/account/fee-rate") => Some(request),
            _ => None,
        })
        .unwrap()
}

#[test]
fn correction_updates_shared_protocol_time_and_resends_in_the_same_callback() {
    let (mut first, request) = started();
    let (mut other, other_request) = started();
    let resent = reject(&mut first, request, "8000");
    assert_eq!(first.venue().samples.last(), Some(&(6_000.0, 8_000.0)));
    assert!(
        resent
            .headers
            .iter()
            .any(|(name, value)| *name == "X-BAPI-TIMESTAMP" && value == "8000")
    );
    reject(&mut other, other_request, "7000");
    assert_eq!(other.venue().samples.last(), Some(&(6_000.0, 7_000.0)));
    assert_eq!(first.venue().samples.last(), Some(&(6_000.0, 8_000.0)));
    first.handle(
        6_000.0,
        Event::HttpResponse {
            request: resent.id,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"retCode":10002,"retMsg":"recv_window server_timestamp[9000]","result":{}}"#,
            },
        },
    );
    assert!(!safe_cex_engine::test_support::prepared_actions(&mut first, 6_000.0).into_iter().any(|action| matches!(action, Action::Http(request) if request.url.contains("/v5/account/fee-rate"))));
    assert_eq!(first.venue().samples.last(), Some(&(6_000.0, 8_000.0)));
}

#[test]
fn invalid_server_observations_leave_the_shared_clock_and_retry_stamp_raw() {
    for server in ["NaN", "inf", "-inf", "1e999"] {
        let (mut engine, request) = started();
        let resent = reject(&mut engine, request, server);
        assert_eq!(engine.venue().samples.last(), Some(&(6_000.0, 6_000.0)));
        assert!(
            resent
                .headers
                .iter()
                .any(|(name, value)| *name == "X-BAPI-TIMESTAMP" && value == "6000")
        );
    }
}

#[test]
fn fast_713_second_clock_re_signs_once_with_corrected_timestamp_and_signature() {
    use safe_cex_crypto::{Encoding, hmac_sha256};
    let host = 1_793_990_020_473.0;
    let server = 1_793_989_307_022.0;
    let (mut engine, first) = started_at(host);
    let refusal = format!(
        r#"{{"retCode":10002,"retMsg":"invalid request, please check your server timestamp or recv_window param. req_timestamp[{host}],server_timestamp[{server}],recv_window[20000]","result":{{}}}}"#
    );
    engine.handle(
        host,
        Event::HttpResponse {
            request: first.id,
            response: Response {
                status: 200,
                headers: "",
                body: &refusal,
            },
        },
    );
    let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, host);
    let retries: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) if r.url.contains("/v5/account/fee-rate") => Some(r),
            _ => None,
        })
        .collect();
    assert_eq!(retries.len(), 1);
    let second = retries[0];
    let header = |r: &HttpRequest, key: &str| {
        r.headers
            .iter()
            .find(|(name, _)| *name == key)
            .unwrap()
            .1
            .clone()
    };
    assert_eq!(header(&first, "X-BAPI-TIMESTAMP"), host.to_string());
    assert_eq!(header(second, "X-BAPI-TIMESTAMP"), server.to_string());
    assert_ne!(header(second, "X-BAPI-SIGN"), header(&first, "X-BAPI-SIGN"));
    let payload = format!(
        "{server}clock-probe-key20000{}",
        second.url.split_once('?').unwrap().1
    );
    assert_eq!(
        header(second, "X-BAPI-SIGN"),
        hmac_sha256(b"clock-probe-secret", payload.as_bytes(), Encoding::Hex)
    );
    assert_eq!(second.url, first.url);
    assert_eq!(second.body, first.body);
    let clock: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(safe_cex_engine::Notice::Event(e))
                if e.code == "bybit.clock.corrected" =>
            {
                Some(e)
            }
            _ => None,
        })
        .collect();
    assert_eq!(clock.len(), 1, "one typed calibration observation");
    assert_eq!(clock[0].fields, vec![("offsetMs", (server - host).into())]);
    engine.handle(
        host + 1.,
        Event::HttpResponse {
            request: second.id,
            response: Response {
                status: 200,
                headers: "",
                body: &refusal,
            },
        },
    );
    assert!(
        !safe_cex_engine::test_support::prepared_actions(&mut engine, host + 1.)
            .into_iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("/v5/account/fee-rate")))
    );
}
