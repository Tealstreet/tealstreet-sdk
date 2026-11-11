mod support;

use safe_cex_engine::model::{OrderSide, OrderType, PlaceOrderOpts};
use safe_cex_engine::request_context::{Claim, HttpAction, Prepared};
use safe_cex_engine::signing::{
    Algorithm, Capability, KeyRef, Provider, SignFuture, SignRequest, Signer,
};
use safe_cex_engine::{Action, Command, Event, HttpRequest, Response};
use std::collections::VecDeque;
use std::sync::Arc;
use support::{MARKETS, Options, TICKERS, TestEngine};

struct Deferred(Provider);

impl Signer for Deferred {
    fn sign(&self, request: SignRequest) -> SignFuture {
        self.0.sign(request)
    }

    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        self.0.can_sign(key, algorithm)
    }
}

struct Fixture {
    engine: TestEngine,
    resident: Provider,
    completions: usize,
}

impl Fixture {
    fn new(cosigned: bool) -> Self {
        let mut engine = support::configured(Options {
            key: Some("replay-key".into()),
            secret: Some("replay-secret".into()),
            ..Default::default()
        });
        // DELEGATED retains a LocalSigner provider; cloning that resident
        // handle must preserve the same native recipe as LOCAL.
        let resident = engine.signer().unwrap();
        engine
            .set_signer(Some(if cosigned {
                Arc::new(Deferred(resident.clone()))
            } else {
                resident.clone()
            }))
            .unwrap();
        Self {
            engine,
            resident,
            completions: 0,
        }
    }

    fn actions(&mut self, now: f64) -> Vec<Action> {
        self.engine.prepare_dispatch(now);
        let mut queue = VecDeque::from(self.engine.drain_actions().collect::<Vec<_>>());
        let mut output = Vec::new();
        let mut count = 0;
        while let Some(action) = queue.pop_front() {
            count += 1;
            assert!(count < 1000, "bounded fixture dispatch");
            match action {
                Action::Sign { attempt, work, .. } => {
                    self.completions += 1;
                    let result = self.resident.sign_ready(&work).unwrap();
                    self.engine.handle(now, Event::Signed { attempt, result });
                }
                Action::ManagedHttp(action) => match *action {
                    HttpAction::Prepare {
                        preparation,
                        request,
                        ..
                    } => {
                        self.engine.http_prepared(
                            now,
                            preparation,
                            Prepared {
                                id: request.id.0,
                                expires_at: None,
                            },
                        );
                    }
                    HttpAction::Bind {
                        permit, request, ..
                    } => {
                        assert!(matches!(
                            self.engine.claim_http(now, permit),
                            Ok(Claim::Released(_))
                        ));
                        output.push(Action::Http(request));
                    }
                    other => output.push(Action::ManagedHttp(Box::new(other))),
                },
                other => output.push(other),
            }
            queue.extend(self.engine.drain_actions());
        }
        output
    }

    fn respond(&mut self, request: &HttpRequest, now: f64, body: &str) {
        self.engine.handle(
            now,
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

    fn ready(cosigned: bool) -> Self {
        let mut fixture = Self::new(cosigned);
        fixture.engine.start(0.);
        let markets = http(&fixture.actions(0.), "/api/v1/contract/detail");
        assert!(markets.headers.iter().all(|(name, _)| *name != "source"));
        fixture.respond(&markets, 100., MARKETS);
        let mut actions = fixture.actions(100.);
        let ticker = http(&actions, "/api/v1/contract/ticker");
        assert!(ticker.headers.iter().all(|(name, _)| *name != "source"));
        fixture.respond(&ticker, 100., TICKERS);
        actions.extend(fixture.actions(100.));
        let mode = http(&actions, "/api/v1/private/position/position_mode");
        fixture.respond(&mode, 100., r#"{"success":true,"code":0,"data":2}"#);
        let reads = fixture.actions(100.);
        for action in reads {
            if let Action::Http(request) = action {
                let body = if request.url.contains("open_positions") {
                    r#"{"success":true,"code":0,"data":[{"positionId":"synthetic-position","symbol":"BTC_USDT","positionType":1,"positionMode":2,"holdVol":60,"holdAvgPrice":60000,"leverage":10,"openType":1}]}"#
                } else {
                    r#"{"success":true,"code":0,"data":[]}"#
                };
                fixture.respond(&request, 100., body);
            }
        }
        fixture.actions(100.);
        fixture
    }
}

fn http(actions: &[Action], path: &str) -> HttpRequest {
    actions
        .iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.ends_with(path) => Some(request.clone()),
            _ => None,
        })
        .expect("real venue request")
}

fn header<'a>(request: &'a HttpRequest, name: &str) -> &'a str {
    let values: Vec<_> = request
        .headers
        .iter()
        .filter(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
        .collect();
    assert_eq!(values.len(), 1, "exactly one {name} header");
    values[0]
}

fn attributed(request: &HttpRequest) {
    assert_eq!(header(request, "source"), "TEAL");
    let stamp = header(request, "Request-Time");
    let payload = request
        .body
        .as_deref()
        .unwrap_or_else(|| request.url.split_once('?').map_or("", |(_, query)| query));
    let expected = safe_cex_crypto::hmac_sha256(
        b"replay-secret",
        format!("replay-key{stamp}{payload}").as_bytes(),
        safe_cex_crypto::Encoding::Hex,
    );
    assert!(
        header(request, "Signature") == expected,
        "attribution is outside the signed payload"
    );
    assert_eq!(header(request, "Recv-Window"), "20000");
    assert!(!payload.contains("TEAL") && !payload.contains("\"source\""));
}

fn create_path(cosigned: bool, case: &str, path: &str) {
    let mut fixture = Fixture::ready(cosigned);
    let before = fixture.completions;
    let mut order: PlaceOrderOpts = serde_json::from_str(
        r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":60,"price":59000}"#,
    )
    .unwrap();
    if case == "attached-tpsl" {
        order.stop_loss = Some(55000.);
        order.take_profit = Some(65000.);
    } else if case.starts_with("plan-") || case.starts_with("position-") {
        let take_profit = case.ends_with("tp");
        order.kind = if take_profit {
            OrderType::TakeProfit
        } else {
            OrderType::StopLoss
        };
        order.price = Some(if take_profit { 65000. } else { 55000. });
        if case.starts_with("position-") {
            order.amount = 0.;
            order.side = OrderSide::Sell;
        }
    }
    let orders = if case == "batch" {
        vec![order.clone(), order]
    } else {
        vec![order]
    };
    fixture.engine.handle(
        200.,
        Event::Command(Command::PlaceOrders {
            command: 91,
            orders,
        }),
    );
    let request = http(&fixture.actions(200.), path);
    attributed(&request);
    assert_eq!(
        fixture.completions > before,
        cosigned,
        "{case} provider path"
    );
}

macro_rules! create_case {
    ($local:ident, $cosigned:ident, $case:literal, $path:literal) => {
        #[test]
        fn $local() {
            create_path(false, $case, $path);
        }
        #[test]
        fn $cosigned() {
            create_path(true, $case, $path);
        }
    };
}

create_case!(
    local_resident_single,
    cosigned_single,
    "single",
    "/api/v1/private/order/submit"
);
create_case!(
    local_resident_batch,
    cosigned_batch,
    "batch",
    "/api/v1/private/order/submit_batch"
);
create_case!(
    local_resident_attached_tpsl,
    cosigned_attached_tpsl,
    "attached-tpsl",
    "/api/v1/private/order/submit"
);
create_case!(
    local_resident_plan_sl,
    cosigned_plan_sl,
    "plan-sl",
    "/api/v1/private/planorder/place/v2"
);
create_case!(
    local_resident_plan_tp,
    cosigned_plan_tp,
    "plan-tp",
    "/api/v1/private/planorder/place/v2"
);
create_case!(
    local_resident_position_sl,
    cosigned_position_sl,
    "position-sl",
    "/api/v1/private/stoporder/place"
);
create_case!(
    local_resident_position_tp,
    cosigned_position_tp,
    "position-tp",
    "/api/v1/private/stoporder/place"
);

#[test]
fn private_reads_match_ccxt() {
    for cosigned in [false, true] {
        let mut fixture = Fixture::new(cosigned);
        fixture.engine.handle(
            1_700_000_000_000.,
            Event::Command(Command::FetchAccount { command: 92 }),
        );
        attributed(&http(
            &fixture.actions(1_700_000_000_000.),
            "/api/v1/private/account/assets",
        ));
    }
}

#[test]
fn public_reads_have_no_broker_header() {
    let mut engine = support::configured(Options {
        public_data_only: true,
        ..Default::default()
    });
    engine.start(0.);
    let requests = support::public_requests(&mut engine, 0.);
    assert!(!requests.is_empty());
    assert!(
        requests
            .iter()
            .all(|request| request.headers.iter().all(|(name, _)| *name != "source"))
    );
}
