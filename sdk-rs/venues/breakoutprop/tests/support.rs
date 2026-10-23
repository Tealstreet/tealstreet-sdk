#![allow(dead_code)]
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Response, TimerId};
use safe_cex_venue_breakoutprop::{BreakoutProp, Options};
use std::collections::{HashMap, VecDeque};

pub const LOGIN: &str = r#"{"sessionToken":"example-session","timeout":"00:30:00"}"#;
pub const USER: &str = r#"{"userDetails":[{"login":"example-user","domain":"default","accounts":[{"account":"default:00002","accountStatus":"FULL_TRADING"}]}]}"#;
pub const INSTRUMENTS: &str = r#"{"instruments":[{"symbol":"ETHUSD","priceIncrement":0.01,"lotSize":1},{"symbol":"XBTUSD","priceIncrement":0.5,"lotSize":1}]}"#;
pub const DETAILS: &str = r#"{"instrumentDetails":[{"account":"default:00002","symbol":"ETHUSD","tradingStatus":"FULL","minOrderSize":0.01,"maxOrderSize":{"value":724,"limitType":"LOTS"},"minOrderSizeIncrement":0.01,"marginRate":{"rateType":"FLAT","tiers":[{"endVolume":"Infinity","value":0.2}]}},{"account":"default:00002","symbol":"XBTUSD","tradingStatus":"FULL","minOrderSize":0.01,"maxOrderSize":{"value":724},"minOrderSizeIncrement":0.01,"marginRate":{"tiers":[{"value":0.2}]}}]}"#;
pub const QUOTES: &str = r#"{"events":[{"symbol":"ETHUSD","type":"Quote","ask":3656.02,"bid":3656.01,"time":"2024-12-11T07:55:11Z"}]}"#;

pub enum ReceiptTrace {
    Progress(u32, Vec<safe_cex_engine::OrderIdentityReceipt>),
    Http(safe_cex_engine::RequestId),
}
pub struct Rig {
    pub engine: Engine<BreakoutProp>,
    pub now: f64,
    pub requests: VecDeque<HttpRequest>,
    pub timers: HashMap<TimerId, f64>,
    pub notices: Vec<Notice>,
    pub failures: Vec<safe_cex_engine::VenueError>,
    pub receipt_trace: Vec<ReceiptTrace>,
    pub dispatch_times: Vec<f64>,
}
impl Rig {
    pub fn new(options: Options) -> Self {
        let mut rig = Self {
            engine: safe_cex_engine::test_support::seeded_engine(BreakoutProp::new(options)),
            now: 0.0,
            requests: VecDeque::new(),
            timers: HashMap::new(),
            notices: Vec::new(),
            failures: Vec::new(),
            receipt_trace: Vec::new(),
            dispatch_times: Vec::new(),
        };
        rig.engine.start(0.0);
        rig.collect();
        rig
    }
    pub fn defaults() -> Self {
        Self::new(Options {
            key: "example-user".into(),
            secret: "example-password".into(),
            public_data_only: true,
            ..Options::default()
        })
    }
    pub fn collect(&mut self) {
        for action in safe_cex_engine::test_support::prepared_actions(&mut self.engine, self.now) {
            match &action {
                Action::Http(r) => self.receipt_trace.push(ReceiptTrace::Http(r.id)),
                Action::Notice(Notice::CommandProgress { command, receipts }) => self
                    .receipt_trace
                    .push(ReceiptTrace::Progress(*command, receipts.clone())),
                _ => {}
            }
            match action {
                Action::Http(request) => {
                    self.dispatch_times.push(self.now);
                    self.requests.push_back(request);
                }
                Action::SetTimer { timer, at_ms } => {
                    self.timers.insert(timer, at_ms);
                }
                Action::ClearTimer { timer } => {
                    self.timers.remove(&timer);
                }
                // Failures as the plain text the host renders.
                Action::Notice(Notice::CommandFailed { command, error }) => {
                    self.notices.push(Notice::CommandFailed {
                        command,
                        error: error.clone(),
                    });
                    self.failures.push(error);
                }
                Action::Notice(notice) => self.notices.push(notice),
                action => panic!("REST-only venue emitted socket action: {action:?}"),
            }
        }
    }
    pub fn next_request(&mut self) -> HttpRequest {
        for _ in 0..100 {
            self.collect();
            if let Some(request) = self.requests.pop_front() {
                return request;
            }
            self.fire_next_timer();
        }
        panic!("request never dispatched");
    }
    pub fn fire_next_timer(&mut self) {
        let (&timer, &at) = self
            .timers
            .iter()
            .min_by(|left, right| left.1.total_cmp(right.1))
            .expect("pending timer");
        self.timers.remove(&timer);
        self.now = self.now.max(at);
        self.engine.handle(self.now, Event::TimerFired { timer });
        self.collect();
    }
    pub fn reply(&mut self, request: &HttpRequest, status: u16, body: &str) {
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
    pub fn failure(&mut self, request: &HttpRequest) {
        self.now += 1.0;
        self.engine.handle(
            self.now,
            Event::HttpFailed {
                request: request.id,
                error: "connection reset",
            },
        );
        self.collect();
    }
    pub fn command(&mut self, command: Command) {
        self.engine.handle(self.now, Event::Command(command));
        self.collect();
    }
    pub fn boot(&mut self) {
        for (path, body) in [
            ("/login", LOGIN),
            ("/users/example-user", USER),
            (
                "/instruments/query?accountCode=default%3A00002",
                INSTRUMENTS,
            ),
            ("/accounts/default%3A00002/instruments/query", DETAILS),
            ("/marketdata", QUOTES),
        ] {
            let request = self.next_request();
            assert!(request.url.ends_with(path), "{} != {path}", request.url);
            self.reply(&request, 200, body);
        }
    }
    pub fn result(&self, id: u32) -> Result<String, String> {
        self.notices
            .iter()
            .rev()
            .find_map(|notice| match notice {
                Notice::CommandDone { command, result } if *command == id => {
                    Some(Ok(result.clone()))
                }
                Notice::CommandFailed { command, error } if *command == id => {
                    Some(Err(error.legacy_text()))
                }
                _ => None,
            })
            .expect("command result")
    }
}
