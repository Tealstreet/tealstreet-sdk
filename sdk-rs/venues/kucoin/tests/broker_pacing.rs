//! Public factory attribution must be applied when the shared budget dispatches.
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest};
use safe_cex_venue_kucoin::{Kucoin, Options, sign::BrokerOptions};

fn assert_public_headers(request: &HttpRequest, now: u64, public_only: bool) {
    assert!(
        request
            .headers
            .contains(&("X-CHANNEL-API-CODE", "3zexp".into()))
    );
    assert!(
        request
            .headers
            .contains(&("Content-Type", "application/json".into()))
    );
    if public_only {
        assert!(
            request
                .headers
                .iter()
                .all(|(name, _)| { !name.starts_with("KC-API-") && *name != "KC-BROKER-NAME" })
        );
        return;
    }
    let path = request
        .url
        .strip_prefix("https://api-futures.kucoin.com")
        .unwrap();
    let request_signature = hmac_sha256(
        b"primary-secret",
        format!("{now}GET{path}").as_bytes(),
        Encoding::Base64,
    );
    let partner_signature = hmac_sha256(
        b"broker-secret",
        format!("{now}partnerprimary-key").as_bytes(),
        Encoding::Base64,
    );
    for (name, value) in [
        ("KC-API-KEY", "primary-key".into()),
        ("KC-API-TIMESTAMP", now.to_string()),
        ("KC-API-SIGN", request_signature),
        ("KC-API-PARTNER", "partner".into()),
        ("KC-API-PARTNER-SIGN", partner_signature),
        ("KC-BROKER-NAME", "broker".into()),
        ("KC-API-PARTNER-VERIFY", "true".into()),
    ] {
        assert!(request.headers.contains(&(name, value)), "{request:?}");
    }
}

#[test]
fn queued_futures_public_headers_use_primary_or_unsigned_policy_at_dispatch() {
    const START: u64 = 1_700_000_000_123;
    for public_only in [false, true] {
        let mut engine = Engine::new(Kucoin::new(Options {
            public_data_only: public_only,
            key: Some("primary-key".into()),
            secret: Some("primary-secret".into()),
            passphrase: Some("primary-passphrase".into()),
            read_credentials: Some(safe_cex_engine::ReadCredentials {
                key: "secondary-key".into(),
                secret: "secondary-secret".into(),
                passphrase: Some("secondary-passphrase".into()),
            }),
            broker: Some(BrokerOptions {
                partner: "partner".into(),
                broker_key: "broker-secret".into(),
                broker_name: "broker".into(),
                verify: None,
            }),
            ..Options::default()
        }));
        engine.start(START as f64);
        let mut actions: Vec<_> = engine.drain_actions().collect();
        let reset = actions
            .iter()
            .find_map(|action| match action {
                Action::SetTimer { timer, at_ms } if *at_ms == (START + 1000) as f64 => {
                    Some(*timer)
                }
                _ => None,
            })
            .expect("the catalogue consumes the first shared-budget slot");
        for command in 1..=3 {
            engine.handle(
                START as f64,
                Event::Command(Command::FetchCandles {
                    command,
                    symbol: "XBTUSDTM".into(),
                    interval: "1m".into(),
                    limit: Some(1),
                    from_ms: None,
                    to_ms: Some(START as f64),
                }),
            );
            actions.extend(engine.drain_actions());
        }
        let immediate: Vec<_> = actions
            .iter()
            .filter_map(|action| match action {
                Action::Http(request) => Some(request),
                _ => None,
            })
            .collect();
        assert_eq!(
            immediate.len(),
            3,
            "the fourth public read must remain queued"
        );
        for request in immediate {
            assert_public_headers(request, START, public_only);
        }
        engine.handle((START + 1000) as f64, Event::TimerFired { timer: reset });
        let dispatched: Vec<_> = engine
            .drain_actions()
            .filter_map(|action| match action {
                Action::Http(request) => Some(request),
                _ => None,
            })
            .collect();
        assert_eq!(dispatched.len(), 1);
        assert!(dispatched[0].url.contains("/api/v1/kline/query?"));
        assert_public_headers(&dispatched[0], START + 1000, public_only);
    }
}
