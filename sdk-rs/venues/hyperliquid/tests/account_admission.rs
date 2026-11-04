//! Background fan-out must not exhaust every account's managed read deadline.
mod support;

use safe_cex_engine::test_support::prepared_now;
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, Response, TimerId, VenueAction,
};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options};

const ACCOUNT: &str = "0x0000000000000000000000000000000000000001";

struct Admission {
    engine: Engine<Hyperliquid>,
    requests: Vec<HttpRequest>,
    timers: Vec<(TimerId, f64)>,
    now: f64,
    hip: bool,
    done: Vec<u32>,
}
impl Admission {
    fn start() -> Self {
        Self::with_dex(false)
    }
    fn with_dex(hip: bool) -> Self {
        let mut engine = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options {
            key: Some(ACCOUNT.into()),
            application_id: Some(ACCOUNT.into()),
            ..Options::default()
        }));
        engine.start(1000.0);
        let mut h = Self {
            engine,
            requests: Vec::new(),
            timers: Vec::new(),
            now: 1000.0,
            hip,
            done: Vec::new(),
        };
        h.pump();
        h
    }
    fn pump(&mut self) {
        for _ in 0..30 {
            let actions = prepared_now(&mut self.engine)
                .into_iter()
                .collect::<Vec<_>>();
            if actions.is_empty() {
                return;
            }
            for action in actions {
                match action {
                    Action::Http(request) => match support::kind(&request).as_str() {
                        "metaAndAssetCtxs" => self.reply(
                            &request,
                            if request.body.as_deref().unwrap().contains("\"dex\"") {
                                support::HIP3_META
                            } else {
                                support::META
                            },
                        ),
                        "perpDexs" => self.reply(
                            &request,
                            if self.hip {
                                r#"[null,{"name":"xyz"}]"#
                            } else {
                                "[null]"
                            },
                        ),
                        _ => self.requests.push(request),
                    },
                    Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                    Action::Notice(Notice::CommandDone { command, .. }) => self.done.push(command),
                    _ => {}
                }
            }
        }
        panic!("background issuance did not settle");
    }
    fn reply(&mut self, request: &HttpRequest, body: &str) {
        self.engine.handle(
            self.now,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
    }
    fn take(&mut self, kind: &str) -> HttpRequest {
        self.pump();
        let index = self
            .requests
            .iter()
            .position(|request| support::kind(request) == kind)
            .unwrap_or_else(|| panic!("missing {kind}"));
        self.requests.remove(index)
    }
    fn core_batch(&mut self) -> (HttpRequest, HttpRequest, HttpRequest) {
        assert_eq!(
            self.requests.iter().map(support::kind).collect::<Vec<_>>(),
            [
                "userAbstraction",
                "clearinghouseState",
                "frontendOpenOrders"
            ]
        );
        (
            self.take("userAbstraction"),
            self.take("clearinghouseState"),
            self.take("frontendOpenOrders"),
        )
    }
    fn advance(&mut self, until: f64) {
        loop {
            self.pump();
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
}

#[test]
fn background_reads_are_bounded_and_core_bootstrap_is_the_first_batch() {
    let mut h = Admission::start();
    let (mode, state, orders) = h.core_batch();
    assert!(h.requests.is_empty());
    h.reply(&mode, r#""disabled""#);
    let fees = h.take("userFees");
    assert!(
        h.requests.is_empty(),
        "only the completed read releases a slot"
    );
    h.reply(&state, support::STATE);
    let builder = h.take("maxBuilderFee");
    assert!(h.requests.is_empty());
    h.reply(&orders, "[]");
    h.reply(
        &fees,
        r#"{"userCrossRate":"0.0002","userAddRate":"0.00005"}"#,
    );
    h.reply(&builder, "30");
    h.pump();
    assert!(h.requests.is_empty());
}

#[test]
fn failure_releases_one_slot_and_a_late_answer_cannot_release_its_successor() {
    let mut h = Admission::start();
    let (mode, _state, _orders) = h.core_batch();
    h.engine.handle(
        h.now,
        Event::HttpFailed {
            request: mode.id,
            error: "test network failure",
        },
    );
    let fees = h.take("userFees");
    h.reply(&mode, r#""disabled""#);
    h.pump();
    assert!(
        h.requests.is_empty(),
        "late completion cannot release a second slot"
    );
    h.reply(
        &fees,
        r#"{"userCrossRate":"0.0002","userAddRate":"0.00005"}"#,
    );
    h.take("maxBuilderFee");
}

#[test]
fn the_original_twenty_second_deadline_releases_only_its_background_slots() {
    let mut h = Admission::start();
    let (mode, _state, _orders) = h.core_batch();
    h.advance(20_999.0);
    assert!(h.requests.is_empty());
    h.advance(21_001.0);
    let _fees = h.take("userFees");
    let _builder = h.take("maxBuilderFee");
    h.reply(&mode, r#""disabled""#);
    h.pump();
    assert!(h.requests.is_empty());
}

#[test]
fn a_malformed_stream_releases_one_slot_without_replaying_an_old_end() {
    let mut h = Admission::start();
    let (_mode, _state, orders) = h.core_batch();
    h.engine.handle(
        h.now,
        Event::HttpStart {
            request: orders.id,
            status: 200,
            headers: "",
        },
    );
    h.engine.handle(
        h.now,
        Event::HttpChunk {
            request: orders.id,
            data: b"[not-json",
        },
    );
    h.engine
        .handle(h.now, Event::HttpEnd { request: orders.id });
    h.take("userFees");
    h.engine
        .handle(h.now, Event::HttpEnd { request: orders.id });
    h.pump();
    assert!(h.requests.is_empty());
}

#[test]
fn caller_owned_positions_read_does_not_wait_for_a_never_issued_routine_dex_read() {
    let mut h = Admission::with_dex(true);
    let (_mode, core, _orders) = h.core_batch();
    h.engine.handle(
        h.now,
        Event::Command(Command::Venue {
            command: 17,
            action: VenueAction::ReadPositions,
        }),
    );
    let fresh_dex = h.take("clearinghouseState");
    assert!(fresh_dex.body.as_deref().unwrap().contains("\"xyz\""));
    let flat = r#"{"assetPositions":[],"marginSummary":{"accountValue":"0","totalMarginUsed":"0"},"time":1000}"#;
    h.reply(&fresh_dex, flat);
    h.reply(&core, support::STATE);
    let fresh_core = h.take("clearinghouseState");
    assert!(!fresh_core.body.as_deref().unwrap().contains("\"dex\""));
    h.reply(&fresh_core, flat);
    h.pump();
    assert!(
        h.done.contains(&17),
        "only fresh reads from every dex answer the caller"
    );
    assert_eq!(
        h.requests.len(),
        1,
        "foreground completion cannot release an extra ordinary slot"
    );
}
