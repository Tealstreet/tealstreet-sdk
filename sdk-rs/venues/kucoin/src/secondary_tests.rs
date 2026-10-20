use super::*;
use safe_cex_engine::{Action, Engine, Event, Notice, Query, ReadCredentials};
fn options() -> Options {
    Options {
        key: Some("primary-key".into()),
        secret: Some("AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=".into()),
        read_credentials: Some(ReadCredentials {
            key: "read-key".into(),
            secret: "AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI=".into(),
            passphrase: Some("read-pass".into()),
        }),
        passphrase: Some("primary-pass".into()),
        ..Options::default()
    }
}

struct Probe(Kucoin);
impl Venue for Probe {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        for method in [Method::Get, Method::Post, Method::Put, Method::Delete] {
            self.0.signed(
                cx,
                method,
                "/secondary-test",
                None,
                Pending::Markets {
                    rows: vec![],
                    invalid: false,
                },
            );
        }
        self.0.signed_streamed(
            cx,
            Method::Get,
            "/secondary-test-streamed",
            None,
            ArrayAt::EnvelopeField("data"),
            Pending::Markets {
                rows: vec![],
                invalid: false,
            },
        );
    }

    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
    fn on_response(&mut self, _: &mut Cx<'_, Pending>, _: Pending, _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, Pending>, _: Pending, _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Pending>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, Pending>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
#[test]
fn secondary_get_keeps_primary_mutations() {
    let mut engine = Engine::new(Probe(Kucoin::new(options())));
    engine.start(1700000000000.0);
    let requests: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .filter(|r| r.url.contains("secondary-test"))
        .collect();
    assert_eq!(requests.len(), 5);
    for request in requests {
        let expected = if request.method == Method::Get {
            "read-key"
        } else {
            "primary-key"
        };
        let opts = options();
        let secret = if request.method == Method::Get {
            opts.read_credentials.as_ref().unwrap().secret.as_str()
        } else {
            opts.secret.as_deref().unwrap()
        };
        let passphrase = if request.method == Method::Get {
            "read-pass"
        } else {
            "primary-pass"
        };
        let credentials = sign::Credentials {
            key: expected.into(),
            secret: secret.into(),
            passphrase: passphrase.into(),
            broker: None,
        };
        assert_eq!(
            request.headers,
            sign::headers(
                &credentials,
                request.method,
                request.url.strip_prefix(REST).unwrap(),
                "",
                1700000000000.0
            )
        );
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| name.eq_ignore_ascii_case("KC-API-KEY") && value == expected),
            "{:?}",
            request
        );
    }
}

#[test]
fn invalid_secondary_identity_stops_startup_before_io() {
    let mut opts = options();
    opts.read_credentials.as_mut().unwrap().secret.clear();
    let mut engine = Engine::new(Kucoin::new(opts));
    engine.start(1700000000000.0);
    assert!(!engine.drain_actions().any(|a| matches!(
        a,
        Action::Http(_) | Action::WsConnect { .. } | Action::WsConnectWithHeaders { .. }
    )));
}

fn public_only_options() -> Options {
    let mut opts = options();
    opts.public_data_only = true;
    opts.broker = Some(sign::BrokerOptions {
        partner: "broker-partner".into(),
        broker_key: "broker-secret".into(),
        broker_name: "broker-name".into(),
        verify: Some(true),
    });
    opts
}
#[test]
fn public_only_signing_helpers_do_not_reintroduce_secondary_identity() {
    let mut engine = Engine::new(Probe(Kucoin::new(public_only_options())));
    engine.start(1700000000000.0);
    let requests = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(requests.len(), 5);
    for request in requests {
        // Attribution remains public; no primary, secondary or partner auth leaks.
        assert_eq!(
            request.headers,
            vec![
                ("X-CHANNEL-API-CODE", "3zexp".into()),
                ("Content-Type", "application/json".into()),
            ],
            "public-only helper authenticated {}",
            request.url
        );
        assert_eq!(request.route, None);
    }
}
#[test]
fn public_only_secondary_trade_history_is_unsigned_and_private_queries_remain_blocked() {
    let mut engine = Engine::new(Kucoin::new(public_only_options()));
    let market = serde_json::from_str::<Instrument>(r#"{"symbol":"XBTUSDTM","baseCurrency":"XBT","settleCurrency":"USDT","status":"Open","lotSize":1,"tickSize":0.1,"maxOrderQty":1000000,"maxLeverage":125,"multiplier":0.001}"#).unwrap().market().unwrap();
    let symbol = market.symbol.clone();
    engine.store_mut().set_markets(vec![market]);
    engine.start(1700000000000.0);
    assert!(engine.drain_actions().all(|a| {
        match a {
            Action::Http(r) => !r
                .headers
                .iter()
                .any(|(name, _)| name.starts_with("KC-API-") || *name == "KC-BROKER-NAME"),
            _ => true,
        }
    }));
    engine.handle(
        1700000000000.0,
        Event::Command(Command::Query {
            command: 73,
            query: Query::PublicTrades {
                symbol: symbol.clone(),
                limit: Some(10),
            },
        }),
    );
    let requests = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.method, Method::Get);
    assert!(
        request
            .url
            .ends_with("/api/v1/trade/history?symbol=XBTUSDTM")
    );
    assert!(
        !request
            .headers
            .iter()
            .any(|(name, _)| name.starts_with("KC-API-") || *name == "KC-BROKER-NAME"),
        "public-only trade history carried authentication"
    );
    // An unsigned public GET still belongs to the host's configured GET/read route.
    assert_eq!(request.route, None);
    engine.handle(
        1700000000000.0,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"code":"200000","data":[]}"#,
            },
        },
    );
    assert!(engine.drain_actions().any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 73, result: Ok(result) }) if result == "[]")));
    let queries = [
        Query::Fills {
            symbol: Some(symbol.clone()),
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: Some(10),
        },
        Query::OrderHistory {
            symbol: Some(symbol.clone()),
            from_ms: None,
            to_ms: None,
            limit: Some(10),
        },
        Query::Income {
            symbol: Some(symbol),
            kind: Some("FUNDING_FEE".into()),
            from_ms: None,
            to_ms: None,
            limit: Some(10),
        },
    ];
    for (index, query) in queries.into_iter().enumerate() {
        let command = 74 + index as u32;
        engine.handle(
            1700000000000.0,
            Event::Command(Command::Query { command, query }),
        );
        let actions = engine.drain_actions().collect::<Vec<_>>();
        assert!(
            matches!(actions.as_slice(), [Action::Notice(Notice::CommandDone { command: done, result: Err(error) })] if *done == command && error.contains("credentials"))
        );
    }
}
