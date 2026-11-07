mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::command::{HostInput, decode_host_input};
use safe_cex_engine::model::{Balance, Limits, Market, MinMax, Position, PositionSide, Precision};
use safe_cex_engine::wire::{section, sections};
use safe_cex_engine::{
    Action, Command, Cx, Engine, Event, Method, PublicFeedPolicy, PublicFeedUpdate, Response,
    SocketChannel, SocketId, TimerId, Venue,
};

#[derive(Default)]
struct Feeds {
    public_commands: usize,
    feed_updates: usize,
    revalue_positions: bool,
    derived_bid: f64,
    derived_at: f64,
}
impl Feeds {
    fn connect(cx: &mut Cx<'_, ()>) {
        let socket = cx.connect(
            safe_cex_engine::SocketSpec::new("wss://same.test/feed").channel(SocketChannel::Public),
        );
        cx.send(socket, "public");
        cx.close(socket);
        cx.connect(
            safe_cex_engine::SocketSpec::new("wss://binary.test/feed")
                .channel(SocketChannel::Public)
                .binary(),
        );
        cx.connect(
            safe_cex_engine::SocketSpec::new("wss://same.test/feed")
                .headers(vec![("Authorization", "public".into())])
                .channel(SocketChannel::Public),
        );
        cx.connect(
            safe_cex_engine::SocketSpec::new("wss://same.test/feed")
                .channel(SocketChannel::Public)
                .feed("books", true),
        );
        cx.set_timer(1000.);
        cx.connect(
            safe_cex_engine::SocketSpec::new("wss://same.test/feed")
                .channel(SocketChannel::Private),
        );
        cx.connect(
            safe_cex_engine::SocketSpec::new("wss://same.test/feed")
                .headers(vec![("Authorization", "private".into())])
                .channel(SocketChannel::Business),
        );
    }
}
impl Venue for Feeds {
    // Stands in for any private command the venue answers itself.
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            account: safe_cex_engine::AccountLookup::Query,
            ..safe_cex_engine::Capabilities::BASE
        }
    }
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        Self::connect(cx);
        cx.request(
            Method::Get,
            "https://same.test/metadata".into(),
            vec![],
            None,
            (),
        );
    }
    fn on_public_feed(&mut self, cx: &mut Cx<'_, ()>) {
        self.feed_updates += 1;
        self.derived_bid = cx.store.ticker("A").map_or(0., |ticker| ticker.bid);
        self.derived_at = cx.now;
        if self.revalue_positions {
            cx.store.update_positions(|position| {
                position.unrealized_pnl = self.derived_bid - position.entry_price;
            });
        }
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, cx: &mut Cx<'_, ()>, _: SocketId, text: &str) {
        if text == "poll" {
            cx.store.set_tickers(vec![safe_cex_engine::model::Ticker {
                id: "native-A".into(),
                symbol: "A".into(),
                bid: 456.,
                ask: 458.,
                last: 457.,
                mid_price: Some(457.5),
                ..Default::default()
            }]);
            cx.store.set_balance(Balance {
                total: 999.,
                ..Default::default()
            });
        } else if text == "clear" {
            cx.store.set_tickers(Vec::new());
        }
    }
    fn on_close(&mut self, cx: &mut Cx<'_, ()>, _: SocketId, _: u16) {
        Self::connect(cx);
    }
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, cx: &mut Cx<'_, ()>, _: TimerId) {
        Self::connect(cx);
    }
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {
        self.public_commands += 1;
    }
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn market(symbol: &str) -> Market {
    Market {
        id: format!("native-{symbol}"),
        symbol: symbol.into(),
        pretty_symbol: None,
        base: symbol.into(),
        quote: "USD".into(),
        active: true,
        precision: Precision {
            amount: 0.1,
            price: 0.5,
        },
        limits: Limits {
            amount: MinMax { min: 1., max: 100. },
            leverage: MinMax { min: 1., max: 10. },
            price: None,
        },
        contract_size: Some(100.),
        is_expiring: None,
        is_inverse: Some(true),
        is_spot: None,
        is_perpetual: Some(true),
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }
}
fn update(json: &str) -> PublicFeedUpdate {
    match decode_host_input(json).unwrap() {
        HostInput::PublicFeedUpdate(update) => update,
        _ => panic!("expected public feed update"),
    }
}
fn shared() -> Engine<Feeds> {
    let mut engine = support::seeded_engine(Feeds::default());
    engine
        .set_public_feed_policy(PublicFeedPolicy::Shared)
        .unwrap();
    engine
        .store_mut()
        .set_markets(vec![market("B"), market("A")]);
    engine.store_mut().set_balance(Balance {
        total: 123.,
        free: 99.,
        used: 24.,
        upnl: 1.,
    });
    engine.store_mut().set_settings([(
        1,
        safe_cex_engine::store::MarketSetting {
            leverage: 7.,
            ..Default::default()
        },
    )]);
    engine
}

#[test]
fn direct_and_disabled_do_not_run_provider_hooks_when_native_tickers_change() {
    for policy in [PublicFeedPolicy::Direct, PublicFeedPolicy::Disabled] {
        let mut engine = support::seeded_engine(Feeds::default());
        engine.set_public_feed_policy(policy).unwrap();
        engine.store_mut().set_markets(vec![market("A")]);
        engine.start(0.);
        assert_eq!(engine.venue().feed_updates, 0);
        for (at, text) in [(1., "poll"), (2., "clear"), (3., "poll")] {
            engine.handle(
                at,
                Event::WsMessage {
                    socket: SocketId(999),
                    text,
                },
            );
            assert_eq!(engine.venue().feed_updates, 0);
        }
        assert_eq!(engine.store().ticker("A").unwrap().bid, 456.);
        assert_eq!(engine.store().balance().total, 999.);
    }
}

#[test]
fn socket_policy_suppresses_public_roles_on_every_attempt_preserves_same_url_auth_and_http() {
    for policy in [
        PublicFeedPolicy::Direct,
        PublicFeedPolicy::Disabled,
        PublicFeedPolicy::Shared,
    ] {
        let mut engine = support::seeded_engine(Feeds::default());
        engine.set_public_feed_policy(policy).unwrap();
        engine.start(0.);
        assert!(
            engine
                .set_public_feed_policy(PublicFeedPolicy::Direct)
                .is_err()
        );
        for attempt in 0..4 {
            let actions: Vec<_> = engine.drain_actions().collect();
            let roles: Vec<_> = actions
                .iter()
                .filter_map(|action| match action {
                    Action::WsConnect {
                        channel, socket, ..
                    } => {
                        assert_ne!(socket.0, 0);
                        channel.as_ref()
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(
                roles
                    .iter()
                    .filter(|role| ***role == SocketChannel::Public)
                    .count(),
                if policy == PublicFeedPolicy::Direct {
                    4
                } else {
                    0
                }
            );
            assert_eq!(
                roles
                    .iter()
                    .filter(|role| ***role == SocketChannel::Private)
                    .count(),
                1
            );
            assert_eq!(
                roles
                    .iter()
                    .filter(|role| ***role == SocketChannel::Business)
                    .count(),
                1
            );
            if policy != PublicFeedPolicy::Direct {
                assert!(!actions.iter().any(|action| matches!(
                    action,
                    Action::WsSend { .. } | Action::WsClose { .. }
                )));
            }
            if attempt == 0 {
                assert!(
                    actions
                        .iter()
                        .any(|action| matches!(action, Action::Http(_)))
                );
            }
            // Both close recovery and repeated watchdogs must retain the policy.
            if attempt % 2 == 0 {
                let timer = actions
                    .iter()
                    .find_map(|action| match action {
                        Action::SetTimer { timer, .. } => Some(*timer),
                        _ => None,
                    })
                    .unwrap();
                engine.handle(attempt as f64, Event::TimerFired { timer });
            } else {
                engine.handle(
                    attempt as f64,
                    Event::WsClosed {
                        socket: SocketId(999),
                        code: 1006,
                    },
                );
            }
        }
    }
}

#[test]
fn typed_ingest_uses_native_symbols_ids_metadata_settings_and_private_state() {
    let mut engine = shared();
    let private_loaded = engine.store().loaded();
    engine.take_frame(0.);
    engine.apply_public_feed(1000., update(r#"{"op":"publicFeedUpdate","tickers":[
        {"symbol":"UNKNOWN","bid":1},{"symbol":"A","bid":99,"ask":101,"last":100,"mark":100.5,
         "index":100.2,"percentage":-1,"openInterest":2,"fundingRate":-0.01,"fundingTime":12345,"volume":7,"quoteVolume":700}],
        "books":[{"symbol":"UNKNOWN","bids":[[1,2]],"asks":[]},
            {"symbol":"A","bids":[[98,2],[99,1]],"asks":[[102,2],[101,1]]}],
        "candles":[{"symbol":"A","interval":"1m","timestamp":1700000000,"open":99,"high":102,"low":98,"close":100,"volume":7}]}"#)).unwrap();
    assert_eq!(engine.store().markets().len(), 2);
    assert_eq!(engine.store().market_index("A"), Some(1));
    assert_eq!(engine.store().tickers().len(), 1);
    let ticker = engine.store().ticker("A").unwrap();
    assert_eq!(ticker.id, "native-A");
    assert_eq!(
        (
            ticker.bid,
            ticker.ask,
            ticker.last,
            ticker.percentage,
            ticker.volume
        ),
        (99., 101., 100., -1., 7.)
    );
    assert_eq!(engine.store().balance().total, 123.);
    assert_eq!(engine.store().setting("A").unwrap().leverage, 7.);
    assert_eq!(
        engine.store().market("A").unwrap().contract_size,
        Some(100.)
    );
    assert_eq!(engine.store().loaded().balance, private_loaded.balance);
    assert_eq!(engine.store().loaded().orders, private_loaded.orders);
    assert!(engine.store().loaded().tickers);
    assert_eq!(engine.book("A").unwrap().bids(), &[(99., 1.), (98., 2.)]);
    assert_eq!(engine.book("A").unwrap().asks(), &[(101., 1.), (102., 2.)]);
    let frame = engine.take_frame(1.).unwrap();
    let book = sections(frame)
        .find(|(kind, _)| *kind == section::BOOK)
        .unwrap()
        .1;
    assert_eq!(u32::from_le_bytes(book[..4].try_into().unwrap()), 1);
    let candles = sections(frame)
        .find(|(kind, _)| *kind == section::CANDLES)
        .unwrap()
        .1;
    assert!(!candles.is_empty());
    let snapshot = engine.snapshot_frame(2.);
    assert_eq!(
        sections(snapshot)
            .filter(|(kind, _)| *kind == section::BOOK)
            .count(),
        1
    );
    engine
        .apply_public_feed(
            1000.,
            update(r#"{"op":"publicFeedUpdate","tickers":[{"symbol":"A","bid":100,"ask":null}]}"#),
        )
        .unwrap();
    assert_eq!(engine.store().ticker("A").unwrap().ask, 101.);
    assert_eq!(engine.store().ticker("A").unwrap().bid, 100.);
}

#[test]
fn shared_midpoint_add_change_clear_and_omission_preserve_native_last() {
    let mut engine = shared();
    engine.take_frame(0.);
    engine.apply_public_feed(1., update(
        r#"{"op":"publicFeedUpdate","tickers":[{"symbol":"A","last":100,"midPrice":101.25}]}"#,
    )).unwrap();
    assert_eq!(engine.store().ticker("A").unwrap().last, 100.);
    assert_eq!(engine.store().ticker("A").unwrap().mid_price, Some(101.25));
    engine.handle(
        2.,
        Event::WsMessage {
            socket: SocketId(999),
            text: "poll",
        },
    );
    assert_eq!(engine.store().ticker("A").unwrap().mid_price, Some(101.25));
    engine
        .apply_public_feed(
            3.,
            update(r#"{"op":"publicFeedUpdate","tickers":[{"symbol":"A","bid":99}]}"#),
        )
        .unwrap();
    assert_eq!(engine.store().ticker("A").unwrap().mid_price, Some(101.25));
    engine
        .apply_public_feed(
            4.,
            update(r#"{"op":"publicFeedUpdate","tickers":[{"symbol":"A","midPrice":102.5}]}"#),
        )
        .unwrap();
    assert_eq!(engine.store().ticker("A").unwrap().last, 100.);
    assert_eq!(engine.store().ticker("A").unwrap().mid_price, Some(102.5));
    engine
        .apply_public_feed(
            5.,
            update(r#"{"op":"publicFeedUpdate","tickers":[{"symbol":"A","midPrice":null}]}"#),
        )
        .unwrap();
    assert_eq!(engine.store().ticker("A").unwrap().mid_price, None);
    engine
        .apply_public_feed(
            6.,
            update(r#"{"op":"publicFeedUpdate","tickers":[{"symbol":"A","ask":105}]}"#),
        )
        .unwrap();
    assert_eq!(engine.store().ticker("A").unwrap().mid_price, None);
    engine.handle(
        7.,
        Event::WsMessage {
            socket: SocketId(999),
            text: "poll",
        },
    );
    assert_eq!(engine.store().ticker("A").unwrap().mid_price, None);
    assert_eq!(engine.store().ticker("A").unwrap().last, 100.);
}

#[test]
fn shared_midpoint_rejects_nonfinite_and_nonnumeric_json_values() {
    for value in [
        "1e400",
        "-1e400",
        "\"NaN\"",
        "\"Infinity\"",
        "true",
        "{}",
        "[]",
    ] {
        let json = format!(
            r#"{{"op":"publicFeedUpdate","tickers":[{{"symbol":"A","midPrice":{value}}}]}}"#
        );
        assert!(decode_host_input(&json).is_err(), "{json}");
    }
}

#[test]
fn ingress_rejects_private_metadata_payloads_invalid_books_and_unshared_engines() {
    for private_key in [
        "orders",
        "positions",
        "balance",
        "settings",
        "markets",
        "loaded",
    ] {
        assert!(
            decode_host_input(&format!(
                r#"{{"op":"publicFeedUpdate","{private_key}":[]}}"#
            ))
            .is_err()
        );
    }
    assert!(
        decode_host_input(
            r#"{"op":"publicFeedUpdate","tickers":[{"symbol":"A","id":"foreign","bid":2}]}"#
        )
        .is_err()
    );
    for policy in [PublicFeedPolicy::Direct, PublicFeedPolicy::Disabled] {
        let mut engine = support::seeded_engine(Feeds::default());
        engine.set_public_feed_policy(policy).unwrap();
        assert!(
            engine
                .apply_public_feed(1000., PublicFeedUpdate::default())
                .is_err()
        );
    }
    let mut engine = shared();
    assert!(engine.apply_public_feed(1000., update(r#"{"op":"publicFeedUpdate","tickers":[{"symbol":"A","bid":1}],"books":[{"symbol":"A","bids":[[-1,2]],"asks":[]}]}"#)).is_err());
    assert!(
        engine.store().tickers().is_empty(),
        "rejected payload is atomic"
    );
    let mut empty = support::seeded_engine(Feeds::default());
    empty
        .set_public_feed_policy(PublicFeedPolicy::Shared)
        .unwrap();
    empty
        .apply_public_feed(1000., PublicFeedUpdate::default())
        .unwrap();
    assert!(
        !empty.store().loaded().tickers,
        "a feed cannot mark a cold catalog ready"
    );
    engine
        .apply_public_feed(1000., PublicFeedUpdate::default())
        .unwrap();
    assert!(
        !engine.store().loaded().tickers,
        "empty input cannot claim a provider ticker seed"
    );
    engine
        .apply_public_feed(1000., update(
            r#"{"op":"publicFeedUpdate","tickers":[{"symbol":"UNKNOWN","bid":1},{"symbol":"A"}]}"#,
        ))
        .unwrap();
    assert!(
        !engine.store().loaded().tickers,
        "unknown/empty rows cannot claim a provider ticker seed"
    );
}

#[test]
fn non_direct_subscriptions_bypass_venue_public_requests_but_keep_private_commands() {
    for policy in [PublicFeedPolicy::Disabled, PublicFeedPolicy::Shared] {
        let mut engine = shared();
        engine.set_public_feed_policy(policy).unwrap();
        for command in [
            Command::SetTickerSymbols {
                symbols: vec!["A".into()],
            },
            Command::SubscribeOrderBook { symbol: "A".into() },
            Command::UnsubscribeOrderBook { symbol: "A".into() },
            Command::SubscribeCandles {
                symbol: "A".into(),
                interval: safe_cex_engine::model::Interval::M1,
            },
            Command::UnsubscribeCandles {
                symbol: "A".into(),
                interval: safe_cex_engine::model::Interval::M1,
            },
        ] {
            engine.handle(0., Event::Command(command));
        }
        assert_eq!(engine.venue().public_commands, 0);
        engine.handle(0., Event::Command(Command::FetchAccount { command: 1 }));
        assert_eq!(engine.venue().public_commands, 1);
    }
}

#[test]
fn provider_fields_stay_authoritative_after_venue_polls_and_partial_updates() {
    let mut engine = shared();
    engine
        .apply_public_feed(
            1000.,
            update(r#"{"op":"publicFeedUpdate","tickers":[{"symbol":"A","bid":123,"ask":125}]}"#),
        )
        .unwrap();
    engine
        .apply_public_feed(
            1000.,
            update(r#"{"op":"publicFeedUpdate","tickers":[{"symbol":"A","bid":124}]}"#),
        )
        .unwrap();
    engine.handle(
        1.,
        Event::WsMessage {
            socket: SocketId(999),
            text: "poll",
        },
    );
    let ticker = engine.store().ticker("A").unwrap();
    assert_eq!((ticker.bid, ticker.ask, ticker.last), (124., 125., 457.));
    assert_eq!(
        engine.store().balance().total,
        999.,
        "native private state still reconciles"
    );
    assert_eq!(
        engine.venue().derived_bid,
        124.,
        "derived state follows restored provider prices"
    );
    assert_eq!(
        engine.venue().derived_at,
        1.,
        "restoration hook uses the venue event clock"
    );
    engine.handle(
        2.,
        Event::WsMessage {
            socket: SocketId(999),
            text: "clear",
        },
    );
    let ticker = engine.store().ticker("A").unwrap();
    assert_eq!((ticker.bid, ticker.ask), (124., 125.));
    assert_eq!(ticker.id, "native-A");
    assert_eq!(
        engine.store().market("A").unwrap().contract_size,
        Some(100.)
    );
    assert!(sections(engine.snapshot_frame(3.)).any(|(kind, _)| kind == section::TICKERS));
}

#[test]
fn non_ticker_packets_do_not_replay_valuation_or_dirty_positions_but_pending_native_quotes_restore()
{
    let mut engine = support::seeded_engine(Feeds {
        revalue_positions: true,
        ..Default::default()
    });
    engine
        .set_public_feed_policy(PublicFeedPolicy::Shared)
        .unwrap();
    engine.store_mut().set_markets(vec![market("A")]);
    engine.store_mut().set_positions(vec![Position {
        id: "A-long".into(),
        symbol: "A".into(),
        side: PositionSide::Long,
        entry_price: 100.,
        notional: 100.,
        leverage: 1.,
        margin_type: None,
        position_mode: None,
        unrealized_pnl: 0.,
        realized_pnl: None,
        contracts: 1.,
        liquidation_price: 0.,
        margin_size: None,
        last_update: None,
    }]);
    engine
        .apply_public_feed(
            1000.,
            update(r#"{"op":"publicFeedUpdate","tickers":[{"symbol":"A","bid":123}]}"#),
        )
        .unwrap();
    assert_eq!(engine.venue().feed_updates, 1);
    assert_eq!(engine.store().positions()[0].unrealized_pnl, 23.);
    engine.take_frame(1000.).unwrap();

    for json in [
        r#"{"op":"publicFeedUpdate","books":[{"symbol":"A","bids":[[122,2]],"asks":[[124,3]]}]}"#,
        r#"{"op":"publicFeedUpdate","candles":[{"symbol":"A","interval":"1m","timestamp":1000,"open":123,"high":124,"low":122,"close":123,"volume":1}]}"#,
        r#"{"op":"publicFeedUpdate"}"#,
        r#"{"op":"publicFeedUpdate","tickers":[{"symbol":"UNKNOWN","bid":999}]}"#,
        r#"{"op":"publicFeedUpdate","tickers":[{"symbol":"A"}]}"#,
    ] {
        engine.apply_public_feed(1001., update(json)).unwrap();
        assert_eq!(engine.venue().feed_updates, 1, "{json}");
        assert_eq!(engine.venue().derived_at, 1000., "{json}");
        assert_eq!(engine.store().positions()[0].unrealized_pnl, 23.);
        if let Some(frame) = engine.take_frame(1001.) {
            assert!(
                !sections(frame).any(|(kind, _)| matches!(
                    kind,
                    section::POSITIONS | section::TICKERS | section::TICKER_DELTAS
                )),
                "non-ticker packet dirtied account/ticker sections: {json}"
            );
        }
    }

    // Valid provider quotes must still invoke the venue hook.
    engine
        .apply_public_feed(
            1002.,
            update(r#"{"op":"publicFeedUpdate","tickers":[{"symbol":"A","bid":124}]}"#),
        )
        .unwrap();
    assert_eq!(engine.venue().feed_updates, 2);
    assert_eq!(engine.store().positions()[0].unrealized_pnl, 24.);
    assert!(
        sections(engine.take_frame(1002.).unwrap()).any(|(kind, _)| kind == section::POSITIONS)
    );

    // A real venue poll restores provider quotes and valuation after dispatch.
    engine.handle(
        1003.,
        Event::WsMessage {
            socket: SocketId(999),
            text: "poll",
        },
    );
    assert_eq!(engine.venue().feed_updates, 3);
    assert_eq!(engine.store().ticker("A").unwrap().bid, 124.);
    assert_eq!(engine.store().positions()[0].unrealized_pnl, 24.);
    assert_eq!(engine.store().balance().total, 999.);
    engine.take_frame(1003.).unwrap();

    // A pending native revision is independent of whether this packet has quotes.
    engine
        .store_mut()
        .update_ticker("A", |ticker| ticker.bid = 456.);
    engine
        .apply_public_feed(
            1004.,
            update(r#"{"op":"publicFeedUpdate","books":[{"symbol":"A","bids":[],"asks":[]}]}"#),
        )
        .unwrap();
    assert_eq!(engine.venue().feed_updates, 4);
    assert_eq!(engine.venue().derived_at, 1004.);
    assert_eq!(engine.store().ticker("A").unwrap().bid, 124.);
    assert_eq!(engine.store().positions()[0].unrealized_pnl, 24.);
}

#[test]
fn shared_book_snapshots_and_clears_do_not_depend_on_a_venue_native_book() {
    for deltas in [false, true] {
        let mut engine = shared();
        #[cfg(feature = "book-deltas")]
        engine.set_book_deltas(deltas);
        #[cfg(not(feature = "book-deltas"))]
        let _ = deltas;
        assert!(engine.venue().book("A").is_none());
        assert_eq!(engine.venue().books().count(), 0);
        engine.take_frame(0.);
        engine.apply_public_feed(1., update(r#"{"op":"publicFeedUpdate","books":[{"symbol":"A","bids":[[99,2]],"asks":[[101,3]]}]}"#)).unwrap();
        for frame in [
            engine.take_frame(1.).unwrap().to_vec(),
            engine.snapshot_frame(2.).to_vec(),
        ] {
            let payload = sections(&frame)
                .find(|(kind, _)| *kind == section::BOOK)
                .unwrap()
                .1;
            assert_eq!(u32::from_le_bytes(payload[..4].try_into().unwrap()), 1);
            assert_eq!(u32::from_le_bytes(payload[4..8].try_into().unwrap()), 1);
            assert_eq!(u32::from_le_bytes(payload[8..12].try_into().unwrap()), 1);
            assert_eq!(f64::from_le_bytes(payload[16..24].try_into().unwrap()), 99.);
            assert_eq!(f64::from_le_bytes(payload[24..32].try_into().unwrap()), 2.);
            assert_eq!(
                f64::from_le_bytes(payload[40..48].try_into().unwrap()),
                101.
            );
            assert_eq!(f64::from_le_bytes(payload[48..56].try_into().unwrap()), 3.);
        }
        engine
            .apply_public_feed(
                3.,
                update(r#"{"op":"publicFeedUpdate","books":[{"symbol":"A","bids":[],"asks":[]}]}"#),
            )
            .unwrap();
        // The worker sent the invalidation, but a suspended consumer missed it.
        let clear = engine.take_frame(3.).unwrap().to_vec();
        let recovery = engine.snapshot_frame(4.).to_vec();
        for frame in [clear, recovery] {
            let payload = sections(&frame)
                .find(|(kind, _)| *kind == section::BOOK)
                .unwrap()
                .1;
            assert_eq!(u32::from_le_bytes(payload[..4].try_into().unwrap()), 1);
            assert_eq!(&payload[4..], &[0; 12]);
        }
        engine.apply_public_feed(5., update(r#"{"op":"publicFeedUpdate","books":[{"symbol":"A","bids":[],"asks":[[102,4]]}]}"#)).unwrap();
        let frame = engine.snapshot_frame(5.);
        let payload = sections(frame)
            .find(|(kind, _)| *kind == section::BOOK)
            .unwrap()
            .1;
        assert_eq!(u32::from_le_bytes(payload[4..8].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(payload[8..12].try_into().unwrap()), 1);
        assert_eq!(
            f64::from_le_bytes(payload[16..24].try_into().unwrap()),
            102.
        );
    }
}
