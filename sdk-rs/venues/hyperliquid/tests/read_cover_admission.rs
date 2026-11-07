//! Read-cover asks and Unknown reconciliation share bounded all-dex admission.
mod support;

use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, Outcome, Response, VenueAction,
};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options};
use std::collections::{HashMap, HashSet, VecDeque};

struct Source {
    engine: Engine<Hyperliquid>,
    now: f64,
    reads: VecDeque<(HttpRequest, f64)>,
    proven: HashMap<String, f64>,
    dexes: Vec<String>,
}
impl Source {
    fn start() -> Self {
        let mut engine = support::private_engine(
            Options {
                key: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
                application_id: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
                ..Default::default()
            },
            support::TEST_SECRET.into(),
            false,
        );
        engine.start(1000.0);
        let dexes: Vec<_> = std::iter::once(String::new())
            .chain((0..10).map(|i| format!("dex{i}")))
            .collect();
        let mut source = Self {
            engine,
            now: 1000.0,
            reads: VecDeque::new(),
            proven: dexes.iter().map(|dex| (dex.clone(), 1000.0)).collect(),
            dexes,
        };
        for _ in 0..100 {
            let actions: Vec<_> = support::prepared_now(&mut source.engine)
                .into_iter()
                .collect();
            if actions.is_empty() {
                assert_eq!(source.cover(), Some(1000.0), "every startup scope proved");
                return source;
            }
            for action in actions {
                if let Action::Http(request) = action {
                    let kind = support::kind(&request);
                    let body = match kind.as_str() {
                        "metaAndAssetCtxs" => {
                            let dex = Self::dex(&request);
                            if dex.is_empty() {
                                support::META.into()
                            } else {
                                support::HIP3_META.replace("xyz:", &format!("{dex}:"))
                            }
                        }
                        "perpDexs" => {
                            let mut rows = vec![serde_json::Value::Null];
                            rows.extend(
                                source
                                    .dexes
                                    .iter()
                                    .skip(1)
                                    .map(|name| serde_json::json!({"name": name})),
                            );
                            serde_json::to_string(&rows).unwrap()
                        }
                        "userAbstraction" => r#""disabled""#.into(),
                        "clearinghouseState" if Self::dex(&request).is_empty() => {
                            support::STATE.into()
                        }
                        "clearinghouseState" => r#"{"assetPositions":[],"time":1000}"#.into(),
                        "frontendOpenOrders" => "[]".into(),
                        "userFees" => {
                            r#"{"userCrossRate":"0.00045","userAddRate":"0.00015"}"#.into()
                        }
                        "maxBuilderFee" => "30".into(),
                        other => panic!("unexpected startup {other}"),
                    };
                    source.reply(&request, &body);
                }
            }
        }
        panic!("eleven-scope startup did not settle");
    }
    fn dex(request: &HttpRequest) -> String {
        serde_json::from_str::<serde_json::Value>(request.body.as_deref().unwrap()).unwrap()["dex"]
            .as_str()
            .unwrap_or("")
            .into()
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
    fn cover(&mut self) -> Option<f64> {
        let frame = self.engine.snapshot_frame(self.now).to_vec();
        safe_cex_engine::wire::sections(&frame)
            .find(|(kind, _)| *kind == safe_cex_engine::wire::section::ORDERS_READ)
            .map(|(_, data)| f64::from_le_bytes(data[8..16].try_into().unwrap()))
            .filter(|value| !value.is_nan())
    }
    fn collect(&mut self) {
        for action in support::prepared_now(&mut self.engine) {
            if let Action::Http(request) = action {
                assert_eq!(support::kind(&request), "frontendOpenOrders");
                assert_eq!(
                    self.engine.http_read_class(request.id),
                    safe_cex_engine::HttpReadClass::Critical
                );
                self.reads.push_back((request, self.now));
            }
        }
        assert!(
            self.reads.len() <= 3,
            "all-dex foreground fan-out bypassed the per-engine cap: {} reads",
            self.reads.len()
        );
    }
    fn ask(&mut self, command: u32) {
        self.engine.handle(
            self.now,
            Event::Command(Command::Venue {
                command,
                action: VenueAction::RefreshOrders {
                    symbols: Some(vec!["BTC-PERP".into()]),
                },
            }),
        );
    }
    fn finish_joined_read(&mut self) {
        self.collect();
        assert_eq!(self.reads.len(), 3, "initial batch fills the cap");
        let first_send = self.now;
        let mut seen = HashSet::new();
        while let Some((request, sent)) = self.reads.pop_front() {
            let dex = Self::dex(&request);
            assert!(seen.insert(dex.clone()), "one physical read per scope");
            self.now += 10.0;
            self.reply(&request, "[]");
            self.proven.insert(dex, sent);
            let oldest = self.proven.values().copied().min_by(f64::total_cmp);
            assert_eq!(
                self.cover(),
                oldest,
                "the account joins only attributable physical scope sends"
            );
            self.collect();
        }
        assert_eq!(seen, self.dexes.iter().cloned().collect());
        assert_eq!(
            self.cover(),
            Some(first_send),
            "account proof uses the earliest physical scope send"
        );
    }
}

#[test]
fn eleven_dex_refresh_asks_obey_the_cap_and_join_every_physical_cover() {
    let mut source = Source::start();
    source.now = 5000.0;
    source.ask(70);
    source.finish_joined_read();
    source.now = 6000.0;
    source.ask(71);
    source.finish_joined_read();
}

#[test]
fn eleven_dex_unknown_reconciliation_cannot_bypass_the_admission_cap() {
    let mut source = Source::start();
    source.now = 5000.0;
    source.engine.handle(
        source.now,
        Event::Command(Command::PlaceOrders {
            command: 80,
            orders: vec![
                serde_json::from_str(
                    r#"{"symbol":"BTC-PERP","type":"limit","side":"buy","price":100,"amount":1}"#,
                )
                .unwrap(),
            ],
        }),
    );
    let request = support::prepared_now(&mut source.engine)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.ends_with("/exchange") => Some(request),
            _ => None,
        })
        .expect("synthetic signed placement");
    source.now = 5100.0;
    source.engine.handle(
        source.now,
        Event::HttpFailed {
            request: request.id,
            error: "synthetic lost reply",
        },
    );
    let actions: Vec<_> = support::prepared_now(&mut source.engine)
        .into_iter()
        .collect();
    assert!(actions.iter().any(|action| matches!(action,
        Action::Notice(Notice::CommandFailed {command: 80, error}) if error.outcome == Some(Outcome::Unknown))));
    for action in actions {
        if let Action::Http(request) = action {
            assert_eq!(support::kind(&request), "frontendOpenOrders");
            source.reads.push_back((request, source.now));
        }
    }
    source.finish_joined_read();
}
