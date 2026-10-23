use safe_cex_engine::{Action, Event, Response, SocketChannel};
use safe_cex_venue_bitunix::{BitunixFutures, Options};

#[test]
fn socket_roles_and_shard_identity_survive_reconnects_and_watchdog_recovery() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(BitunixFutures::new(Options {
        key: Some("replay-key".into()),
        secret: Some("replay-secret".into()),
        ..Options::default()
    }));
    engine.start(0.0);
    let request = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .unwrap();
    let pairs = (0..301)
        .map(|index| format!(r#"{{"symbol":"T{index}USDT","base":"T{index}","quote":"USDT","symbolStatus":"OPEN","basePrecision":3,"quotePrecision":1,"maxLimitOrderVolume":"100"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    engine.handle(
        0.0,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: &format!(r#"{{"code":0,"data":[{pairs}]}}"#),
            },
        },
    );
    let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.);
    let tickers = actions
        .iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.ends_with("/tickers") => Some(request.id),
            _ => None,
        })
        .unwrap();
    let mut connections = actions
        .into_iter()
        .filter_map(|action| match action {
            Action::WsConnect {
                binary,
                socket,
                url,
                channel,
                context,
                headers,
            } => {
                assert!(headers.is_empty(), "Bitunix feeds send no upgrade headers");
                assert!(!binary, "Bitunix feeds use the text protocol");
                Some((socket, url, channel.unwrap(), context.unwrap()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    engine.handle(
        0.0,
        Event::HttpResponse {
            request: tickers,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"code":0,"data":[{"symbol":"T0USDT","last":"1","open":"1","markPrice":"1","baseVol":"1","quoteVol":"1"}]}"#,
            },
        },
    );
    connections.extend(engine.drain_actions().filter_map(|action| match action {
        Action::WsConnect {
            binary,
            socket,
            url,
            channel,
            context,
            headers,
        } => {
            assert!(headers.is_empty(), "Bitunix feeds send no upgrade headers");
            assert!(!binary, "Bitunix feeds use the text protocol");
            Some((socket, url, channel.unwrap(), context.unwrap()))
        }
        _ => None,
    }));
    let mut feeds = connections
        .iter()
        .map(|(_, _, _, context)| context.feed.as_str())
        .collect::<Vec<_>>();
    feeds.sort_unstable();
    assert_eq!(
        feeds,
        [
            "futures",
            "price:0",
            "price:300",
            "private",
            "ticker:0",
            "ticker:300"
        ]
    );
    for (_, url, channel, context) in &connections {
        assert!(!context.recover);
        let (expected_url, expected_channel) = match context.feed.as_str() {
            "private" => ("wss://fapi.bitunix.com/private/", SocketChannel::Private),
            "futures" => (
                "wss://api.bitunix.com/ws-futures/?from=contract&compress=2",
                SocketChannel::Business,
            ),
            _ => ("wss://fapi.bitunix.com/public/", SocketChannel::Public),
        };
        assert_eq!(url, expected_url);
        assert_eq!(*channel, expected_channel);
    }
    let original = connections
        .iter()
        .find(|(_, _, _, context)| context.feed == "ticker:300")
        .unwrap()
        .0;
    engine.handle(
        1.0,
        Event::WsClosed {
            socket: original,
            code: 1006,
        },
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::WsConnect { .. }))
    );
    let (retry, at) = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
            _ => None,
        })
        .unwrap();
    engine.handle(at, Event::TimerFired { timer: retry });
    let actions: Vec<_> = engine.drain_actions().collect();
    let (replacement, context) = actions
        .iter()
        .find_map(|action| match action {
            Action::WsConnect {
                socket,
                context: Some(context),
                ..
            } => Some((*socket, context)),
            _ => None,
        })
        .unwrap();
    assert_eq!(context.feed, "ticker:300");
    assert!(!context.recover);
    engine.handle(
        at,
        Event::WsOpened {
            socket: replacement,
        },
    );
    let watchdog = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .unwrap();
    engine.handle(50_000.0, Event::TimerFired { timer: watchdog });
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(
        actions
            .iter()
            .any(|action| matches!(action, Action::WsClose { socket } if *socket == replacement))
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::WsConnect { .. }))
    );
    let (retry, at) = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
            _ => None,
        })
        .unwrap();
    engine.handle(at, Event::TimerFired { timer: retry });
    let actions: Vec<_> = engine.drain_actions().collect();
    let (recovered, context) = actions
        .iter()
        .find_map(|action| match action {
            Action::WsConnect {
                socket,
                channel: Some(SocketChannel::Public),
                context: Some(context),
                ..
            } => Some((*socket, context)),
            _ => None,
        })
        .unwrap();
    assert_eq!(context.feed, "ticker:300");
    assert!(context.recover);
    engine.handle(
        at + 1.0,
        Event::WsClosed {
            socket: recovered,
            code: 1006,
        },
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::WsConnect { .. }))
    );
    let (retry, at) = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
            _ => None,
        })
        .unwrap();
    engine.handle(at, Event::TimerFired { timer: retry });
    assert!(engine.drain_actions().any(|action| matches!(action,
        Action::WsConnect { context: Some(context), .. } if context.feed == "ticker:300" && !context.recover)));
}
