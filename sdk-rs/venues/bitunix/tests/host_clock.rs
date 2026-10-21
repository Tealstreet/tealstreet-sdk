mod support;
use safe_cex_engine::{Action, Event, Response, SocketChannel, Venue};
use safe_cex_venue_bitunix::{BitunixFutures, Options};

#[test]
fn bitunix_requires_the_synchronized_host_clock() {
    let venue = BitunixFutures::new(Options::default());
    let mut expected = safe_cex_engine::Capabilities::BASE;
    expected.host_clock = true;
    assert_eq!(venue.capabilities(), expected);
}

#[test]
fn first_authenticated_rest_and_private_login_use_the_supplied_engine_time() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(BitunixFutures::new(Options {
        key: Some("replay-key".into()),
        secret: Some("replay-secret".into()),
        ..Options::default()
    }));
    engine.start(1_760_000_000_000.0);
    let catalogue = support::requests(&mut engine).remove(0);
    assert!(
        !catalogue
            .headers
            .iter()
            .any(|(name, _)| *name == "timestamp")
    );
    engine.handle(
        1_760_000_001_234.75,
        Event::HttpResponse {
            request: catalogue.id,
            response: Response {
                status: 200,
                headers: "",
                body: support::PAIRS,
            },
        },
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    let mut signed = 0;
    let mut private = None;
    for action in actions {
        match action {
            Action::Http(request) => {
                if let Some((_, timestamp)) = request
                    .headers
                    .iter()
                    .find(|(name, _)| *name == "timestamp")
                {
                    assert_eq!(timestamp, "1760000001234");
                    signed += 1;
                }
            }
            Action::WsConnect {
                socket,
                channel: Some(SocketChannel::Private),
                ..
            } => private = Some(socket),
            _ => {}
        }
    }
    assert!(signed > 0, "catalogue starts authenticated account reads");
    engine.handle(
        1_760_000_009_876.9,
        Event::WsOpened {
            socket: private.unwrap(),
        },
    );
    let login = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::WsSend { text, .. } if text.contains("\"login\"") => Some(text),
            _ => None,
        })
        .expect("private open signs login immediately");
    let value: serde_json::Value = serde_json::from_str(&login).unwrap();
    assert_eq!(value["args"][0]["timestamp"], "1760000009876");
}
