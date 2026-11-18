//! An account found only by its key must not show as flat while the lookup fails.
mod support;
use safe_cex_engine::test_support::prepared_now;
use safe_cex_engine::{Action, Engine, Event, HttpRequest};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options};
use support::{META, kind, reply};

const AGENT: &str = "0x1111111111111111111111111111111111111111";
const MASTER: &str = "0x2222222222222222222222222222222222222222";

struct Run {
    engine: Engine<Hyperliquid>,
    requests: Vec<HttpRequest>,
    timers: Vec<(safe_cex_engine::TimerId, f64)>,
    errors: Vec<String>,
    sent: Vec<String>,
    now: f64,
}
impl Run {
    fn drain(&mut self) {
        for action in prepared_now(&mut self.engine) {
            match action {
                Action::Http(request) => match kind(&request).as_str() {
                    "metaAndAssetCtxs" => reply(&mut self.engine, &request, 200, META),
                    "perpDexs" => reply(&mut self.engine, &request, 200, "[null]"),
                    _ => self.requests.push(request),
                },
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::WsConnect { socket, .. } => {
                    self.engine.handle(1000.0, Event::WsOpened { socket });
                }
                Action::WsSend { socket, text } => {
                    // Hyperliquid acknowledges each subscription by echoing it.
                    if let Ok(sent) = serde_json::from_str::<serde_json::Value>(&text)
                        && sent["method"] == "subscribe"
                    {
                        let ack = serde_json::json!({"channel":"subscriptionResponse","data":sent});
                        self.engine.handle(
                            1000.0,
                            Event::WsMessage {
                                socket,
                                text: &ack.to_string(),
                            },
                        );
                    }
                    self.sent.push(text);
                }
                Action::Notice(notice) => self.errors.push(format!("{notice:?}")),
                _ => {}
            }
        }
    }
    fn answer(&mut self, request: &HttpRequest, status: u16, body: &str) {
        self.engine.handle(
            self.now,
            Event::HttpResponse {
                request: request.id,
                response: safe_cex_engine::Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
    }
    fn take(&mut self, what: &str) -> Option<HttpRequest> {
        for _ in 0..4 {
            self.drain();
            if let Some(i) = self.requests.iter().position(|r| kind(r) == what) {
                return Some(self.requests.remove(i));
            }
        }
        None
    }
    // Fires due timers in order until `what` is issued, so it is answered inside its own
    // deadline rather than after the stale 60 s discovery deadline of the failed attempt.
    fn fire_until(&mut self, what: &str) -> Option<HttpRequest> {
        self.timers.sort_by(|a, b| b.1.total_cmp(&a.1));
        while let Some((timer, at)) = self.timers.pop() {
            self.now = self.now.max(at);
            self.engine.handle(self.now, Event::TimerFired { timer });
            if let Some(request) = self.take(what) {
                return Some(request);
            }
            self.timers.sort_by(|a, b| b.1.total_cmp(&a.1));
        }
        None
    }
    fn advance(&mut self, until: f64) {
        loop {
            self.drain();
            self.timers.sort_by(|a, b| b.1.total_cmp(&a.1));
            let Some(&(timer, at)) = self.timers.last() else {
                break;
            };
            if at > until {
                break;
            }
            self.timers.pop();
            self.now = self.now.max(at);
            self.engine.handle(self.now, Event::TimerFired { timer });
        }
        self.now = until;
    }
    fn fire_all(&mut self) {
        let mut due: Vec<_> = std::mem::take(&mut self.timers);
        due.sort_by(|a, b| a.1.total_cmp(&b.1));
        for (timer, at) in due {
            self.now = self.now.max(at);
            self.engine.handle(self.now, Event::TimerFired { timer });
            self.drain();
        }
    }
}
fn keyed() -> Run {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options {
        key: Some(AGENT.into()),
        ..Options::default()
    }));
    engine.start(1000.);
    Run {
        engine,
        requests: vec![],
        timers: vec![],
        errors: vec![],
        sent: vec![],
        now: 1000.0,
    }
}

#[test]
fn a_failed_role_lookup_retries_and_does_not_show_the_account_flat() {
    let mut run = keyed();
    let role = run.take("userRole").expect("first lookup");
    reply(&mut run.engine, &role, 500, "busy");
    run.drain();
    let loaded = run.engine.store().loaded();
    assert!(
        !loaded.positions && !loaded.orders,
        "unread rows must not read as flat"
    );
    let again = run.fire_until("userRole").expect("a retry after backoff");
    reply(
        &mut run.engine,
        &again,
        200,
        &format!(r#"{{"role":"agent","data":{{"user":"{MASTER}"}}}}"#),
    );
    let state = run
        .take("clearinghouseState")
        .expect("the resolved account starts its core read");
    let body: serde_json::Value = serde_json::from_str(state.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["user"], MASTER);
}

#[test]
fn a_role_lookup_that_never_answers_says_so_visibly() {
    let mut run = keyed();
    for _ in 0..8 {
        if let Some(role) = run.take("userRole") {
            reply(&mut run.engine, &role, 500, "busy");
        }
        run.fire_all();
    }
    let text = run.errors.join("\n");
    assert!(
        text.contains("Error") && text.contains("could not be resolved"),
        "{text}"
    );
}

#[test]
fn a_failed_margin_mode_read_is_retried_and_a_portfolio_account_reads_its_spot() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options {
        key: Some(AGENT.into()),
        application_id: Some(MASTER.into()),
        ..Options::default()
    }));
    engine.start(1000.);
    let mut run = Run {
        engine,
        requests: vec![],
        timers: vec![],
        errors: vec![],
        sent: vec![],
        now: 1000.0,
    };
    let mode = run.take("userAbstraction").expect("mode read");
    reply(&mut run.engine, &mode, 500, "busy");
    let again = run.fire_until("userAbstraction").expect("a retry");
    run.answer(&again, 200, r#""portfolioMargin""#);
    for _ in 0..6 {
        run.fire_all();
    }
    assert!(
        run.sent.iter().any(|text| text.contains("spotState")),
        "portfolio spot is subscribed: {:?}",
        run.sent
    );
}

#[test]
fn pending_role_is_single_flight_until_its_managed_deadline() {
    let mut run = keyed();
    let role = run.take("userRole").expect("first role read");
    run.advance(16_000.0);
    assert!(
        run.take("userRole").is_none(),
        "quota or network wait must not start a second role lookup"
    );
    run.answer(&role, 500, "busy");
    let retry = run
        .fire_until("userRole")
        .expect("completion failure permits one retry");
    assert_ne!(role.id, retry.id);
}

#[test]
fn pending_mode_is_single_flight_until_its_managed_deadline() {
    let mut run = keyed();
    let role = run.take("userRole").expect("first role read");
    run.answer(
        &role,
        200,
        &format!(r#"{{"role":"agent","data":{{"user":"{MASTER}"}}}}"#),
    );
    let mode = run.take("userAbstraction").expect("first mode read");
    run.advance(16_000.0);
    assert!(
        run.take("userAbstraction").is_none(),
        "quota or network wait must not start a second mode lookup"
    );
    run.answer(&mode, 500, "busy");
    let retry = run
        .fire_until("userAbstraction")
        .expect("completion failure permits one retry");
    assert_ne!(mode.id, retry.id);
}
