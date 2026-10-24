//! Actual KuCoin trusted setup and opaque constructor custody.
#![cfg(all(
    feature = "kucoin",
    feature = "kucoinspot",
    feature = "dispatch-signing"
))]
use safe_cex_engine::signing::{ExtraKey, Identities};
use safe_cex_engine::{Action, Command, Engine, ErrorKind, HttpRequest, Venue};
use safe_cex_venues::{EngineOptions, Visit, build, build_with_signer, prepare_local_signer_for};
const NOW: f64 = 1234.9;
fn input() -> EngineOptions {
    serde_json::from_str(r#"{"key":"fixture-primary-key","secret":"fixture-primary-secret","passphrase":"fixture-passphrase","readCredentials":{"key":"fixture-read-key","secret":"fixture-read-secret"},"extra":{"kucoinBroker":{"partner":" fixture-partner ","brokerKey":" fixture-broker-secret ","brokerName":" fixture-name ","verify":false}}}"#).unwrap()
}
fn entropy() -> Option<safe_cex_engine::entropy::Entropy> {
    Some(safe_cex_engine::test_support::seeded_entropy(17))
}
struct Identity;
impl Visit for Identity {
    type Output = (Identities, Vec<Action>);
    fn visit<V: Venue + 'static>(self, mut engine: Engine<V>) -> Self::Output {
        engine
            .handle_command(NOW, Command::FetchAccount { command: 73 }, Some(120_000))
            .unwrap();
        let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, NOW);
        (engine.identities().clone(), actions)
    }
}
fn http(actions: &[Action]) -> &HttpRequest {
    let requests: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 1, "one selected identity call: {actions:?}");
    requests[0]
}
#[test]
fn native_setup_consumes_broker_once_and_secondary_aliases_the_same_ref() {
    for venue in ["kucoin", "kucoinspot"] {
        let mut options = input();
        let (provider, refs) = prepare_local_signer_for(venue, &mut options).unwrap();
        assert!(options.key.is_none() && options.secret.is_none() && options.passphrase.is_none());
        assert!(options.read_credentials.is_none());
        let broker = options
            .extra
            .as_ref()
            .unwrap()
            .kucoin_broker
            .as_ref()
            .unwrap();
        assert!(broker.broker_key.is_empty());
        assert_eq!(broker.partner, " fixture-partner ");
        assert_eq!(broker.verify, Some(false));
        let primary = refs.primary.as_ref().unwrap();
        let secondary = refs.secondary.as_ref().unwrap();
        assert_eq!(primary.passphrase, secondary.passphrase);
        assert_eq!(
            primary.extra.get(&ExtraKey::KucoinBroker),
            secondary.extra.get(&ExtraKey::KucoinBroker)
        );
        assert!(primary.extra.contains_key(&ExtraKey::KucoinBroker));
        options.signing = Some(refs.clone());
        let (registered, actions) =
            build_with_signer(venue, options, entropy(), Some(provider), Identity).unwrap();
        assert_eq!(registered, refs);
        let request = http(&actions);
        assert_eq!(
            request.headers[0],
            ("KC-API-KEY", "fixture-read-key".into())
        );
        assert_eq!(request.headers[1], ("KC-API-TIMESTAMP", "1234".into()));
        assert_eq!(request.headers.len(), 11);
        assert_eq!(
            request.headers[10],
            ("KC-API-PARTNER-VERIFY", "false".into())
        );
    }
}
#[test]
fn opaque_constructor_refuses_every_raw_fallback_and_missing_provider() {
    for venue in ["kucoin", "kucoinspot"] {
        let mut options = input();
        let (provider, refs) = prepare_local_signer_for(venue, &mut options).unwrap();
        for raw in [
            r#"{"key":"fixture-key"}"#,
            r#"{"secret":"fixture-secret"}"#,
            r#"{"passphrase":"fixture-passphrase"}"#,
            r#"{"readCredentials":{"key":"fixture-read-key","secret":"fixture-read-secret"}}"#,
            r#"{"extra":{"kucoinBroker":{"partner":"partner","brokerKey":"fixture-broker","brokerName":"name"}}}"#,
        ] {
            let mut options: EngineOptions = serde_json::from_str(raw).unwrap();
            options.signing = Some(refs.clone());
            let error =
                build_with_signer(venue, options, entropy(), Some(provider.clone()), Identity)
                    .unwrap_err();
            assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
        }
        options.signing = Some(refs);
        assert!(build_with_signer(venue, options, entropy(), None, Identity).is_err());
    }
}
#[test]
fn trusted_legacy_constructor_converges_on_the_selected_provider_wire() {
    for venue in ["kucoin", "kucoinspot"] {
        let (_, actions) = build(venue, input(), entropy(), Identity).unwrap();
        let request = http(&actions);
        assert_eq!(
            request.headers[0],
            ("KC-API-KEY", "fixture-read-key".into())
        );
        assert_eq!(request.headers.len(), 11);
    }
}
#[test]
fn explicit_empty_secondary_passphrase_never_inherits_primary() {
    for venue in ["kucoin", "kucoinspot"] {
        let mut options = input();
        options.read_credentials.as_mut().unwrap().passphrase = Some(String::new());
        assert!(prepare_local_signer_for(venue, &mut options).is_err());
    }
}

struct PublicOnly;
impl Visit for PublicOnly {
    type Output = Vec<HttpRequest>;
    fn visit<V: Venue + 'static>(self, mut engine: Engine<V>) -> Self::Output {
        use safe_cex_engine::{Event, Notice, Response};
        let primary = engine.identities().primary.as_ref().unwrap();
        assert!(primary.passphrase.is_none());
        let mut requests = Vec::new();
        let drain = |engine: &mut Engine<V>, requests: &mut Vec<HttpRequest>| {
            let actions = safe_cex_engine::test_support::prepared_actions(engine, NOW);
            for action in &actions {
                assert!(!matches!(
                    action,
                    Action::Sign { .. } | Action::Notice(Notice::CommandFailed { .. })
                ));
                if let Action::Http(request) = action {
                    assert_eq!(
                        request.headers,
                        vec![
                            ("X-CHANNEL-API-CODE", "3zexp".into()),
                            ("Content-Type", "application/json".into()),
                        ]
                    );
                    requests.push(request.clone());
                }
            }
            actions
        };
        engine.start(NOW);
        drain(&mut engine, &mut requests);
        for body in [
            r#"{"code":"200000","data":[{"symbol":"XBTUSDTM","baseCurrency":"XBT","settleCurrency":"USDT","status":"Open","lotSize":1,"tickSize":0.1,"maxOrderQty":1000000,"maxLeverage":125,"multiplier":0.001}]}"#,
            r#"{"code":"200000","data":[{"symbol":"XBTUSDTM","price":"100","bestBidPrice":"99","bestAskPrice":"101","size":12}]}"#,
        ] {
            let request = requests.last().unwrap().id;
            engine.handle(
                NOW,
                Event::HttpResponse {
                    request,
                    response: Response {
                        status: 200,
                        headers: "",
                        body,
                    },
                },
            );
            drain(&mut engine, &mut requests);
        }
        let bullets: Vec<_> = requests
            .iter()
            .filter(|request| request.url.ends_with("/bullet-public"))
            .cloned()
            .collect();
        assert_eq!(bullets.len(), 2);
        let mut sockets = Vec::new();
        for request in bullets {
            engine.handle(NOW, Event::HttpResponse { request: request.id, response: Response {
                status: 200, headers: "", body: r#"{"code":"200000","data":{"token":"fixture-public-token","instanceServers":[{"endpoint":"wss://fixture.test/ws","pingInterval":18000,"pingTimeout":10000}]}}"#,
            } });
            sockets.extend(
                drain(&mut engine, &mut requests)
                    .into_iter()
                    .filter_map(|action| match action {
                        Action::WsConnect { socket, .. } => Some(socket),
                        _ => None,
                    }),
            );
        }
        assert_eq!(sockets.len(), 2);
        for socket in sockets {
            engine.handle(NOW, Event::WsOpened { socket });
            engine.handle(
                NOW,
                Event::WsMessage {
                    socket,
                    text: r#"{"type":"welcome"}"#,
                },
            );
            drain(&mut engine, &mut requests);
        }
        engine.handle(
            NOW,
            Event::Command(Command::SubscribeOrderBook {
                symbol: "BTCUSDT".into(),
            }),
        );
        drain(&mut engine, &mut requests);
        engine
            .handle_command(
                NOW,
                Command::FetchCandles {
                    command: 283,
                    symbol: "BTCUSDT".into(),
                    interval: safe_cex_engine::model::Interval::H1,
                    limit: Some(1),
                    from_ms: None,
                    to_ms: Some(7_200_000.),
                },
                Some(120_000),
            )
            .unwrap();
        drain(&mut engine, &mut requests);
        requests
    }
}
#[test]
fn public_only_passphrase_less_primary_never_signs_bullet_depth_or_candles() {
    let options: EngineOptions = serde_json::from_str(
        r#"{"key":"fixture-public-key","secret":"fixture-public-secret","publicDataOnly":true}"#,
    )
    .unwrap();
    let requests = build("kucoin", options, entropy(), PublicOnly).unwrap();
    assert!(
        requests
            .iter()
            .any(|request| request.url.contains("/level2/snapshot?"))
    );
    assert!(
        requests
            .iter()
            .any(|request| request.url.contains("/kline/query?"))
    );
    assert!(
        !requests
            .iter()
            .any(|request| request.url.ends_with("/bullet-private"))
    );
}
