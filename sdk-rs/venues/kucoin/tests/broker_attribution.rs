use safe_cex_engine::{Action, Engine, Event, HttpRequest, Venue};
use safe_cex_venue_kucoin::{Kucoin, Options};

#[test]
fn public_catalogue_preserves_factory_broker_attribution_without_credentials() {
    let mut engine = Engine::new(Kucoin::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    engine.start(0.0);
    let requests: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect();
    assert!(!requests.is_empty());
    for request in requests {
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| *name == "X-CHANNEL-API-CODE" && value == "3zexp"),
            "{:?}",
            request
        );
    }
}

#[cfg(feature = "spot")]
#[test]
fn spot_public_catalogue_preserves_the_same_broker_header() {
    let mut engine = Engine::new(safe_cex_venue_kucoin::spot::KucoinSpot::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    engine.start(0.0);
    let requests: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect();
    assert!(!requests.is_empty());
    for request in requests {
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| *name == "X-CHANNEL-API-CODE" && value == "3zexp"),
            "{:?}",
            request
        );
    }
}

fn partner_options() -> Options {
    Options {
        key: Some("replay-key".into()),
        secret: Some("replay-secret".into()),
        passphrase: Some("replay-passphrase".into()),
        broker: Some(safe_cex_venue_kucoin::sign::BrokerOptions {
            partner: " partner-test ".into(),
            broker_key: " broker-test-key ".into(),
            broker_name: " Broker Name ".into(),
            verify: None,
        }),
        ..Options::default()
    }
}
fn assert_partner(request: &HttpRequest) {
    for (name, expected) in [
        ("X-CHANNEL-API-CODE", "3zexp"),
        ("KC-API-KEY", "replay-key"),
        ("KC-API-TIMESTAMP", "1700000000123"),
        ("KC-API-PARTNER", "partner-test"),
        (
            "KC-API-PARTNER-SIGN",
            "qr2oPnOafgsiv3qWURUltiI7sSCTQIUK2rwlHEDADnY=",
        ),
        ("KC-BROKER-NAME", "Broker Name"),
        ("KC-API-PARTNER-VERIFY", "true"),
    ] {
        assert!(
            request
                .headers
                .iter()
                .any(|(key, value)| *key == name && value == expected),
            "missing {name}: {request:?}"
        );
    }
    assert!(
        !request
            .headers
            .iter()
            .any(|(_, value)| value.contains("broker-test-key"))
    );
}
fn verify_partner_catalogue_and_followup<V: Venue>(
    mut engine: Engine<V>,
    body: &str,
    assert_headers: impl Fn(&HttpRequest),
) {
    engine.start(1700000000123.0);
    let mut timers = Vec::new();
    let http = |engine: &mut Engine<V>, timers: &mut Vec<(safe_cex_engine::TimerId, f64)>| {
        engine
            .drain_actions()
            .filter_map(|a| match a {
                Action::Http(request) => Some(request),
                Action::SetTimer { timer, at_ms } => {
                    timers.push((timer, at_ms));
                    None
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let initial = http(&mut engine, &mut timers);
    assert!(!initial.is_empty());
    for request in initial {
        assert_headers(&request);
        engine.handle(
            1700000000123.0,
            Event::HttpStart {
                request: request.id,
                status: 200,
                headers: "",
            },
        );
        engine.handle(
            1700000000123.0,
            Event::HttpChunk {
                request: request.id,
                data: body.as_bytes(),
            },
        );
        engine.handle(
            1700000000123.0,
            Event::HttpEnd {
                request: request.id,
            },
        );
    }
    let requests = http(&mut engine, &mut timers);
    let mut saw_public_bullet = false;
    let mut saw_tickers = false;
    for request in requests {
        assert_headers(&request);
        saw_public_bullet |= request.url.ends_with("/bullet-public");
        if request.url.ends_with("/allTickers") {
            saw_tickers = true;
            let tickers = if request.url.contains("/market/") {
                r#"{"code":"200000","data":{"ticker":[{"symbol":"BTC-USDT","last":"100"}]}}"#
            } else {
                r#"{"code":"200000","data":[{"symbol":"XBTUSDTM","price":"100"}]}"#
            };
            engine.handle(
                1700000000123.0,
                Event::HttpStart {
                    request: request.id,
                    status: 200,
                    headers: "",
                },
            );
            engine.handle(
                1700000000123.0,
                Event::HttpChunk {
                    request: request.id,
                    data: tickers.as_bytes(),
                },
            );
            engine.handle(
                1700000000123.0,
                Event::HttpEnd {
                    request: request.id,
                },
            );
        }
    }
    // Startup reads fill the paced budget; tokens lead the next window.
    for _ in 0..8 {
        for request in http(&mut engine, &mut timers) {
            assert_headers(&request);
            saw_public_bullet |= request.url.ends_with("/bullet-public");
        }
        if saw_public_bullet {
            break;
        }
        let next = (0..timers.len())
            .min_by(|&a, &b| timers[a].1.total_cmp(&timers[b].1))
            .expect("budget window timer");
        // Fire on the fixed clock the signature assertions are anchored to.
        let (timer, _) = timers.remove(next);
        engine.handle(1700000000123.0, Event::TimerFired { timer });
    }
    assert!(saw_public_bullet);
    assert!(saw_tickers);
}
#[test]
fn authenticated_futures_public_reads_keep_configured_partner_signatures() {
    verify_partner_catalogue_and_followup(
        Engine::new(Kucoin::new(partner_options())),
        r#"{"code":"200000","data":[{"symbol":"XBTUSDTM","baseCurrency":"XBT","settleCurrency":"USDT","status":"Open","lotSize":1,"tickSize":0.1,"maxOrderQty":1000000,"maxLeverage":125,"multiplier":0.001,"expireDate":null,"isInverse":false}]}"#,
        assert_partner,
    );
}
#[cfg(feature = "spot")]
#[test]
fn authenticated_spot_public_reads_stay_channel_only_with_configured_partner() {
    verify_partner_catalogue_and_followup(
        Engine::new(safe_cex_venue_kucoin::spot::KucoinSpot::new(
            partner_options(),
        )),
        r#"{"code":"200000","data":[{"symbol":"BTC-USDT","baseCurrency":"BTC","quoteCurrency":"USDT","enableTrading":true,"baseIncrement":"0.0001","priceIncrement":"0.1","baseMinSize":"0.001","baseMaxSize":"10"}]}"#,
        |request| {
            if ["/api/v2/symbols", "/market/allTickers", "/bullet-public"]
                .iter()
                .any(|path| request.url.ends_with(path))
            {
                assert!(
                    request
                        .headers
                        .iter()
                        .any(|(name, value)| { *name == "X-CHANNEL-API-CODE" && value == "3zexp" })
                );
                assert!(
                    request.headers.iter().all(|(name, _)| {
                        !name.starts_with("KC-API-") && *name != "KC-BROKER-NAME"
                    }),
                    "spot public request unexpectedly authenticated: {request:?}"
                );
            } else {
                // The private bullet token retains the primary partner signature.
                assert_partner(request);
            }
        },
    );
}
