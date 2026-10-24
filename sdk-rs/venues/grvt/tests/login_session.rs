//! The API-key login runs as an engine session write on the primary private
//! egress. No credentials or live requests are used.
use safe_cex_engine::request_context::{HttpAction, RoutingIntent};
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Method, Notice, Response, TimerId,
};
use safe_cex_venue_grvt::{Grvt, Options};

const NOW: f64 = 1_700_000_000_000.;
const LOGIN: &str = "/auth/api_key/login";
const CATALOG: &str = r#"{"r":[{"i":"BTC_USDT_Perp","ih":"0x030501","b":"BTC","q":"USDT","k":"PERPETUAL","ts":"0.1","ms":"0.001","mp":"1000","bd":9,"qd":6}]}"#;

fn engine() -> Engine<Grvt> {
    safe_cex_engine::test_support::seeded_engine(Grvt::new(Options {
        key: Some("fixture-api-key".into()),
        secret: Some(format!("0x{}", "01".repeat(32))),
        trading_account_id: Some("777".into()),
        ..Options::default()
    }))
}

struct Harness {
    engine: Engine<Grvt>,
    now: f64,
    requests: Vec<HttpRequest>,
    timers: Vec<(TimerId, f64)>,
    notices: Vec<Notice>,
}

impl Harness {
    fn started() -> Self {
        let mut h = Self {
            engine: engine(),
            now: NOW,
            requests: vec![],
            timers: vec![],
            notices: vec![],
        };
        h.engine.start(h.now);
        h.collect();
        h
    }
    fn collect(&mut self) {
        for action in safe_cex_engine::test_support::prepared_actions(&mut self.engine, self.now) {
            match action {
                Action::Http(request) => self.requests.push(request),
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
                Action::Notice(notice) => self.notices.push(notice),
                _ => {}
            }
        }
    }
    fn logins(&self) -> usize {
        self.requests
            .iter()
            .filter(|r| r.url.ends_with(LOGIN))
            .count()
    }
    /// Fire every due timer in time order, as a host would.
    fn run_until(&mut self, end: f64, done: impl Fn(&Self) -> bool) {
        while !done(self) {
            let Some(&(timer, at)) = self
                .timers
                .iter()
                .filter(|(_, at)| *at <= end)
                .min_by(|a, b| a.1.total_cmp(&b.1))
            else {
                return;
            };
            self.timers.retain(|(t, _)| *t != timer);
            self.now = self.now.max(at);
            self.engine.handle(self.now, Event::TimerFired { timer });
            self.collect();
        }
    }
}

#[test]
fn login_is_prepared_on_the_primary_private_egress_with_its_bound() {
    let mut engine = engine();
    engine.start(NOW);
    let catalog = safe_cex_engine::test_support::prepared_actions(&mut engine, NOW)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("all_instruments") => Some(request),
            _ => None,
        })
        .expect("catalog read");
    engine.handle(
        NOW,
        Event::HttpResponse {
            request: catalog.id,
            response: Response {
                status: 200,
                headers: "",
                body: CATALOG,
            },
        },
    );
    engine.prepare_dispatch(NOW);
    let login = engine.drain_actions().find_map(|action| match action {
        Action::ManagedHttp(action) => match *action {
            HttpAction::Prepare {
                request, intent, ..
            } if request.url.ends_with(LOGIN) => Some((request, intent)),
            _ => None,
        },
        _ => None,
    });
    let Some((request, intent)) = login else {
        panic!("the login must be prepared as managed session work");
    };
    assert_eq!(
        intent,
        RoutingIntent::Primary,
        "the body carries the API key; public/CORS egress would leak it"
    );
    assert_eq!(request.route, None);
    assert_eq!(request.timeout_ms, Some(20_000));
    assert_eq!(request.method, Method::Post);
    assert_eq!(request.url, "https://edge.grvt.io/auth/api_key/login");
    assert_eq!(
        request.body.as_deref(),
        Some(r#"{"api_key":"fixture-api-key"}"#)
    );
    assert_eq!(
        request.headers,
        [
            ("Content-Type", "application/json".to_owned()),
            ("X-Grvt-Remember-Me", "true".to_owned()),
            ("Cookie", "rm=true".to_owned()),
        ]
    );
}

#[test]
fn a_hung_login_is_reissued_at_its_bound() {
    let mut h = Harness::started();
    if let Some(i) = h
        .requests
        .iter()
        .position(|r| r.url.contains("all_instruments"))
    {
        let catalog = h.requests.remove(i);
        h.engine.handle(
            h.now,
            Event::HttpResponse {
                request: catalog.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: CATALOG,
                },
            },
        );
        h.collect();
    }
    h.engine
        .handle(h.now, Event::Command(Command::FetchAccount { command: 42 }));
    h.collect();
    assert_eq!(h.logins(), 1);
    // The login is never answered and the host reports nothing.
    h.run_until(NOW + 20_000., |h| h.logins() > 1);
    assert_eq!(
        h.logins(),
        2,
        "the engine bounds the login and the venue reissues it"
    );
    // The reissue runs on a new session generation: a late success to the
    // abandoned login installs nothing.
    let logins: Vec<_> = h
        .requests
        .iter()
        .filter(|r| r.url.ends_with(LOGIN))
        .map(|r| r.id)
        .collect();
    let answer = |h: &mut Harness, request| {
        h.engine.handle(
            h.now,
            Event::HttpResponse {
                request,
                response: Response {
                    status: 200,
                    headers: "x-ts-grvt-session: fixture-session\nx-grvt-account-id: 0xaccount",
                    body: r#"{"status":"success","sub_account_id":"777"}"#,
                },
            },
        );
        h.collect();
    };
    let summaries = |h: &Harness| {
        h.requests
            .iter()
            .filter(|r| r.url.ends_with("aggregated_account_summary"))
            .count()
    };
    answer(&mut h, logins[0]);
    assert_eq!(summaries(&h), 0, "the abandoned login's answer is dropped");
    answer(&mut h, logins[1]);
    assert_eq!(summaries(&h), 1, "the current login opens the account");
}
