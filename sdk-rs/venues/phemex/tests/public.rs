//! Oracles: https://phemex-docs.github.io/ USDⓈ-M public response examples.
mod support;
use safe_cex_engine::{
    Action, Command, Engine, Event, Notice, RequestId, Response, SocketId, Venue,
};
use safe_cex_venue_phemex::PhemexPerpetuals;
use support::Options;
const PRODUCTS: &str = r#"{"code":0,"data":{"perpProductsV2":[{"symbol":"BTCUSDT","type":"PerpetualV2","settleCurrency":"USDT","baseCurrency":"BTC","quoteCurrency":"USDT","qtyStepSize":"0.001","tickSize":"0.1","maxOrderQtyRq":"500","maxLeverage":"100"}]}}"#;
const DAILY: &str = r#"{"error":null,"result":[{"symbol":"BTCUSDT","bidRp":"20690.1","askRp":"20690.2","lastRp":"20690.2","markRp":"20690.3","indexRp":"20690","openRp":"20000","openInterestRv":"250","fundingRateRr":"0.0001","volumeRq":"10"}]}"#;
fn respond(e: &mut Engine<PhemexPerpetuals>, id: RequestId, body: &str) {
    respond_at(e, id, body, 0.);
}
fn respond_at(e: &mut Engine<PhemexPerpetuals>, id: RequestId, body: &str, now: f64) {
    e.handle(
        now,
        Event::HttpResponse {
            request: id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    )
}
fn request(e: &mut Engine<PhemexPerpetuals>, path: &str) -> RequestId {
    request_at(e, path, 0.)
}
fn request_at(e: &mut Engine<PhemexPerpetuals>, path: &str, now: f64) -> RequestId {
    safe_cex_engine::test_support::prepared_actions(e, now)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains(path) => Some(r.id),
            _ => None,
        })
        .unwrap()
}
fn started() -> (Engine<PhemexPerpetuals>, SocketId) {
    let mut e = support::engine(Options {
        public_data_only: true,
        ..Options::default()
    });
    e.start(0.);
    let id = request(&mut e, "products");
    respond(&mut e, id, PRODUCTS);
    let id = request(&mut e, "24hr/all");
    respond(&mut e, id, DAILY);
    let socket = e
        .drain_actions()
        .find_map(|a| match a {
            Action::WsConnect {
                socket,
                channel: Some(safe_cex_engine::SocketChannel::Public),
                ..
            } => Some(socket),
            _ => None,
        })
        .unwrap();
    e.handle(0., Event::WsOpened { socket });
    e.drain_actions().for_each(drop);
    (e, socket)
}
#[test]
fn daily_candle_to_preserves_the_callers_day_instead_of_shifting_back() {
    let (mut e, _) = started();
    let to = 1_700_000_000_000.0; // 2023-11-14 22:13:20 UTC.
    e.handle(
        to,
        Event::Command(Command::FetchCandles {
            command: 91,
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::D1,
            limit: Some(2),
            from_ms: None,
            to_ms: Some(to),
        }),
    );
    let req = safe_cex_engine::test_support::prepared_actions(&mut e, to)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(r) if r.url.contains("/kline/list") => Some(r),
            _ => None,
        })
        .unwrap();
    assert!(
        req.url
            .ends_with("resolution=86400&from=1699827200&to=1700000000"),
        "{}",
        req.url
    );
    respond_at(
        &mut e,
        req.id,
        r#"{"code":0,"data":{"rows":[[1699833600,86400,0,"100","110","90","105","2",0],[1699920000,86400,0,"105","115","95","110","3",0]]}}"#,
        to,
    );
    #[derive(serde::Deserialize)]
    struct Candle {
        timestamp: f64,
    }
    let rows = e
        .drain_actions()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandDone {
                command: 91,
                result: json,
            }) => Some(serde_json::from_str::<Vec<Candle>>(&json).unwrap()),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        rows.iter().map(|r| r.timestamp).collect::<Vec<_>>(),
        [1_699_833_600.0, 1_699_920_000.0]
    );
}

#[test]
fn real_value_products_and_packed_ticker() {
    let (mut e, socket) = started();
    assert_eq!(e.store().market("BTCUSDT").unwrap().precision.price, 0.1);
    assert_eq!(e.store().ticker("BTCUSDT").unwrap().bid, 20690.1);
    e.handle(1.,Event::WsMessage{socket,text:r#"{"method":"perp_market24h_pack_p.update","data":[["BTCUSDT","20000",0,0,"21000","12",0,"300","20999","21001","0.0002",0]]}"#});
    let t = e.store().ticker("BTCUSDT").unwrap();
    assert_eq!(t.bid, 20690.1);
    assert_eq!(t.last, 21000.);
    assert_eq!(t.quote_volume, 252000.);
    assert_eq!(t.percentage, 5.);
}

#[test]
fn candle_limit_selects_a_bounded_recent_window_even_with_an_old_from() {
    let now = 1_700_000_000_000.;
    let week_ago = now - 7. * 24. * 60. * 60. * 1000.;
    for (limit, expected_from) in [(500, 1_699_970_000u64), (5_000, 1_699_940_000)] {
        let (mut e, _) = started();
        e.handle(
            now,
            Event::Command(Command::FetchCandles {
                command: 7,
                symbol: "BTCUSDT".into(),
                interval: safe_cex_engine::model::Interval::M1,
                from_ms: Some(week_ago),
                to_ms: Some(now),
                limit: Some(limit),
            }),
        );
        let url = safe_cex_engine::test_support::prepared_actions(&mut e, now)
            .into_iter()
            .find_map(|a| match a {
                Action::Http(r) => Some(r.url),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            url,
            format!(
                "https://api.phemex.com/exchange/public/md/v2/kline/list?symbol=BTCUSDT&resolution=60&from={expected_from}&to=1700000000"
            )
        );
    }
}

#[test]
fn candle_from_only_and_default_500_bar_windows_keep_their_existing_semantics() {
    let now = 1_700_000_000_000.;
    for (limit, from_ms, expected_from) in [
        (None, None, 1_699_970_000u64),
        (None, Some(1_699_395_200_000.), 1_699_395_200),
        (Some(0), None, 1_699_970_000),
        (Some(0), Some(1_699_395_200_000.), 1_699_395_200),
    ] {
        let (mut e, _) = started();
        e.handle(
            now,
            Event::Command(Command::FetchCandles {
                command: 7,
                symbol: "BTCUSDT".into(),
                interval: safe_cex_engine::model::Interval::M1,
                from_ms,
                to_ms: Some(now),
                limit,
            }),
        );
        let url = safe_cex_engine::test_support::prepared_actions(&mut e, now)
            .into_iter()
            .find_map(|a| match a {
                Action::Http(r) => Some(r.url),
                _ => None,
            })
            .unwrap();
        assert!(url.ends_with(&format!("&from={expected_from}&to=1700000000")));
    }
}

#[test]
fn candle_reads_fail_without_http_during_the_shared_rate_limit_pause() {
    let (mut e, _) = started();
    let now = 1_700_000_000_000.;
    let candles = |command| {
        Event::Command(Command::FetchCandles {
            command,
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
            from_ms: None,
            to_ms: None,
            limit: Some(500),
        })
    };
    e.handle(now, candles(7));
    let id = request_at(&mut e, "/kline/list", now);
    e.handle(
        now,
        Event::HttpResponse {
            request: id,
            response: Response {
                status: 429,
                headers: "x-ratelimit-retry-after-contract: 60",
                body: "{}",
            },
        },
    );
    assert!(
        e.drain_actions()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, .. })))
    );
    e.handle(now + 59_999., candles(8));
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandFailed { command: 8, error }) if error.legacy_text().contains("rate limited")
    )));
    e.handle(now + 60_000., candles(9));
    assert!(safe_cex_engine::test_support::prepared_actions(&mut e, now + 60_000.).into_iter().any(|a| matches!(
        a,
        Action::Http(r) if r.url == "https://api.phemex.com/exchange/public/md/v2/kline/list?symbol=BTCUSDT&resolution=60&from=1699970060&to=1700000060"
    )));
}
#[test]
fn delayed_snapshot_applies_newer_diffs_and_discards_old() {
    let (mut e, socket) = started();
    e.handle(
        0.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    e.drain_actions().for_each(drop);
    e.handle(1.,Event::WsMessage{socket,text:r#"{"symbol":"BTCUSDT","sequence":102,"type":"incremental","orderbook_p":{"bids":[["20690","2"]],"asks":[]}}"#});
    let id = request(&mut e, "fullbook");
    respond(
        &mut e,
        id,
        r#"{"result":{"sequence":101,"orderbook_p":{"bids":[["20690","1"],["20689","1"]],"asks":[["20691","1"]]}}}"#,
    );
    assert_eq!(
        e.venue().book("BTCUSDT").unwrap().bids(),
        &[(20690., 2.), (20689., 1.)]
    );
    e.handle(2.,Event::WsMessage{socket,text:r#"{"symbol":"BTCUSDT","sequence":100,"type":"incremental","orderbook_p":{"bids":[["20690","999"]]}}"#});
    assert_eq!(e.venue().book("BTCUSDT").unwrap().bids()[0].1, 2.);
    e.handle(3.,Event::WsMessage{socket,text:r#"{"symbol":"BTCUSDT","sequence":103,"type":"snapshot","orderbook_p":{"bids":[["20688","3"]],"asks":[["20692","4"]]}}"#});
    assert_eq!(e.venue().book("BTCUSDT").unwrap().bids(), &[(20688., 3.)]);
}
#[test]
fn unsubscribed_snapshot_is_fenced() {
    let (mut e, socket) = started();
    e.handle(
        0.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    e.drain_actions().for_each(drop);
    e.handle(
        1.,
        Event::WsMessage {
            socket,
            text: r#"{"symbol":"BTCUSDT","sequence":102,"orderbook_p":{"bids":[],"asks":[]}}"#,
        },
    );
    let id = request(&mut e, "fullbook");
    e.handle(
        2.,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    respond(
        &mut e,
        id,
        r#"{"result":{"sequence":101,"orderbook_p":{"bids":[["1","2"]],"asks":[]}}}"#,
    );
    assert!(e.venue().book("BTCUSDT").is_none());
}
#[test]
fn silent_socket_closes_and_reconnects() {
    let (mut e, socket) = started();
    e.handle(0., Event::WsClosed { socket, code: 1006 });
    let timer = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(1000., Event::TimerFired { timer });
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::WsConnect { .. }))
    );
    let timer = actions
        .into_iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(22000., Event::TimerFired { timer });
    assert!(
        e.drain_actions()
            .any(|a| matches!(a, Action::WsClose { .. }))
    );
}

#[test]
fn incomplete_book_snapshot_is_not_committed() {
    let (mut e, socket) = started();
    e.handle(
        0.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    e.drain_actions().for_each(drop);
    e.handle(
        1.,
        Event::WsMessage {
            socket,
            text: r#"{"symbol":"BTCUSDT","sequence":2,"orderbook_p":{"bids":[],"asks":[]}}"#,
        },
    );
    let id = request(&mut e, "fullbook");
    respond(
        &mut e,
        id,
        r#"{"result":{"sequence":1,"orderbook_p":{"bids":[["1","1"]]}}}"#,
    );
    assert!(e.venue().book("BTCUSDT").is_none());
    assert!(
        e.drain_actions()
            .any(|a| matches!(a, Action::SetTimer { .. }))
    );
}
#[test]
fn invalid_supported_product_rejects_the_entire_seed() {
    let mut e = support::engine(Options {
        public_data_only: true,
        ..Options::default()
    });
    e.start(0.);
    let id = request(&mut e, "products");
    respond(
        &mut e,
        id,
        &PRODUCTS.replace("\"qtyStepSize\":\"0.001\"", "\"qtyStepSize\":\"0\""),
    );
    assert!(!e.store().loaded().markets);
    assert!(!e.drain_actions().any(|a| matches!(a, Action::Http(_))));
}

#[test]
fn malformed_full_ws_snapshot_heals_without_publishing_an_empty_book() {
    let (mut e, socket) = started();
    e.handle(
        0.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    e.drain_actions().for_each(drop);
    e.handle(
        1.,
        Event::WsMessage {
            socket,
            text: r#"{"symbol":"BTCUSDT","sequence":2,"orderbook_p":{"bids":[],"asks":[]}}"#,
        },
    );
    let id = request(&mut e, "fullbook");
    respond(
        &mut e,
        id,
        r#"{"result":{"sequence":1,"orderbook_p":{"bids":[["10","2"]],"asks":[["11","3"]]}}}"#,
    );
    assert_eq!(e.venue().book("BTCUSDT").unwrap().best_bid(), Some(10.));
    e.drain_actions().for_each(drop);
    e.handle(
        3.,
        Event::WsMessage {
            socket,
            text: r#"{"symbol":"BTCUSDT","sequence":3,"type":"snapshot","orderbook_p":{}}"#,
        },
    );
    assert!(e.venue().book("BTCUSDT").is_none());
    let timer = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(1003., Event::TimerFired { timer });
    assert!(
        safe_cex_engine::test_support::prepared_actions(&mut e, 1003.)
            .into_iter()
            .any(|a| matches!(a,Action::Http(r) if r.url.contains("fullbook")))
    );
}

#[test]
fn startup_retry_waits_for_the_complete_rate_limit_pause() {
    let mut e = support::engine(Options {
        public_data_only: true,
        ..Options::default()
    });
    e.start(0.);
    let id = request(&mut e, "products");
    e.handle(
        0.,
        Event::HttpResponse {
            request: id,
            response: Response {
                status: 429,
                headers: "x-ratelimit-retry-after-contract: 60",
                body: "{}",
            },
        },
    );
    let timer = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(2000., Event::TimerFired { timer });
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    let (timer, at) = actions
        .into_iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .unwrap();
    assert_eq!(at, 60000.);
    e.handle(at, Event::TimerFired { timer });
    assert!(
        safe_cex_engine::test_support::prepared_actions(&mut e, at)
            .into_iter()
            .any(|a| matches!(a,Action::Http(r) if r.url.ends_with("/public/products")))
    );
}

#[path = "support/socket.rs"]
mod socket_checks;
#[test]
fn public_supervisor_bounds_handshake_and_backs_off_flapping_connections() {
    let (e, socket) = started();
    socket_checks::flaps(e, socket, 100.);
}

#[test]
fn suppressed_public_feed_is_not_supervised() {
    let mut e = support::engine(Options {
        public_data_only: true,
        ..Options::default()
    });
    e.set_public_feed_policy(safe_cex_engine::PublicFeedPolicy::Shared)
        .unwrap();
    e.start(0.);
    let id = request(&mut e, "products");
    respond(&mut e, id, PRODUCTS);
    let id = request(&mut e, "24hr/all");
    respond(&mut e, id, DAILY);
    e.handle(
        0.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    let mut timers = Vec::new();
    let mut fired = 0;
    loop {
        for action in e.drain_actions() {
            match action {
                Action::SetTimer { timer, at_ms } => timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => timers.retain(|(t, _)| *t != timer),
                Action::WsConnect { .. } | Action::WsClose { .. } => {
                    panic!("suppressed public feed churned: {action:?}")
                }
                _ => {}
            }
        }
        timers.sort_by(|a, b| b.1.total_cmp(&a.1));
        match timers.pop() {
            Some((timer, at)) if at <= 600_000. => {
                fired += 1;
                e.handle(at, Event::TimerFired { timer });
            }
            _ => break,
        }
    }
    assert_eq!(fired, 0);
}

// The live product row, as /public/products served it. The venue's id is lower-case u1000PEPEUSDT;
// U1000PEPEUSDT answers 30008 Symbol not supported.
const U1000: &str = r#"{"code":0,"data":{"perpProductsV2":[{"perpProductSubType":"Normal","symbol":"u1000PEPEUSDT","code":66041,"type":"PerpetualV2","displaySymbol":"1000PEPE / USDT","indexSymbol":".u1000PEPEUSDT","markSymbol":".Mu1000PEPEUSDT","fundingRateSymbol":".u1000PEPEUSDTFR","fundingRate8hSymbol":".u1000PEPEUSDTFR8H","contractUnderlyingAssets":"1000 PEPE","settleCurrency":"USDT","quoteCurrency":"USDT","tickSize":"0.0000001","priceScale":0,"ratioScale":0,"pricePrecision":7,"baseCurrency":"1000 PEPE","description":"1000 PEPE/USDT perpetual contracts are priced on the .u1000PEPEUSDT Index. Each contract is worth 1 1000PEPE. Funding fees are paid and received every 8 hours at UTC time: 00:00, 08:00 and 16:00.","status":"Listed","tipOrderQty":0,"listTime":1683367200000,"majorSymbol":false,"defaultLeverage":"-10","fundingInterval":28800,"maxLeverage":50,"leverageMargin":1016,"maxOI":-1.0,"maxOrderQtyRq":"400000000","maxPriceRp":"2000","minOrderValueRv":"1","minPriceRp":"0.001","qtyPrecision":0,"qtyStepSize":"1.0","tipOrderQtyRq":"80000000","maxOpenPosLeverage":50.0}]}}"#;

#[test]
fn a_u1000_candle_read_sends_the_venues_own_id() {
    let mut e = support::engine(Options {
        public_data_only: true,
        ..Options::default()
    });
    e.start(0.);
    let id = request(&mut e, "products");
    respond(&mut e, id, U1000);
    e.drain_actions().for_each(drop);
    assert!(e.store().market("U1000PEPEUSDT").is_some());
    for command in [Command::FetchCandles {
        command: 1,
        symbol: "U1000PEPEUSDT".into(),
        interval: safe_cex_engine::model::Interval::M30,
        limit: Some(2),
        from_ms: None,
        to_ms: None,
    }] {
        e.handle(0., Event::Command(command));
        let sent: Vec<_> = safe_cex_engine::test_support::prepared_actions(&mut e, 0.)
            .into_iter()
            .filter_map(|a| match a {
                Action::Http(r) if r.url.contains("symbol=") => Some(r.url),
                _ => None,
            })
            .collect();
        assert!(!sent.is_empty());
        for url in sent {
            assert!(url.contains("symbol=u1000PEPEUSDT"), "{url}");
        }
    }
}

fn subscription_receipts(frame: &[u8]) -> Vec<(String, u32, f64)> {
    use safe_cex_engine::wire::{section, sections};
    let stats = sections(frame)
        .find(|(kind, _)| *kind == section::MESSAGE_HANDLER_STATS)
        .unwrap()
        .1;
    let strings = sections(frame)
        .find(|(kind, _)| *kind == section::STRINGS)
        .unwrap()
        .1;
    let count = u32::from_le_bytes(strings[..4].try_into().unwrap()) as usize;
    let text = |index: usize| {
        let start =
            u32::from_le_bytes(strings[4 + index * 4..8 + index * 4].try_into().unwrap()) as usize;
        let end =
            u32::from_le_bytes(strings[8 + index * 4..12 + index * 4].try_into().unwrap()) as usize;
        let base = 4 + (count + 1) * 4;
        std::str::from_utf8(&strings[base + start..base + end])
            .unwrap()
            .to_owned()
    };
    let rows: Vec<_> = stats[8..]
        .as_chunks::<24>()
        .0
        .iter()
        .map(|row| {
            (
                text(u32::from_le_bytes(row[4..8].try_into().unwrap()) as usize),
                u32::from_le_bytes(row[8..12].try_into().unwrap()),
                f64::from_le_bytes(row[16..24].try_into().unwrap()),
            )
        })
        .collect();
    assert_eq!(
        rows.len(),
        u32::from_le_bytes(stats[..4].try_into().unwrap()) as usize
    );
    rows
}

#[test]
fn subscription_receipt_counts_candle_snapshots_without_emitting_live_history() {
    let (mut e, socket) = started();
    e.handle(
        1.,
        Event::Command(Command::SubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, 1.);
    assert!(
        actions.iter().any(
            |a| matches!(a, Action::WsSend { text, .. } if text.contains("kline_p.subscribe"))
        )
    );
    e.handle(2., Event::WsMessage { socket, text: r#"{"symbol":"BTCUSDT","type":"snapshot","kline_p":[[1700000000,60,0,"100","110","90","105","2",0]]}"# });
    let frame = e.take_frame(2.).unwrap_or_default();
    assert!(
        !safe_cex_engine::wire::sections(frame)
            .any(|(kind, _)| kind == safe_cex_engine::wire::section::CANDLES)
    );
    let rows = subscription_receipts(e.snapshot_frame(3.));
    let candle = rows
        .iter()
        .find(|r| r.0.ends_with("2:candle:BTCUSDT:60"))
        .unwrap();
    let snapshot_receipt = (candle.1, candle.2);
    e.handle(4., Event::WsMessage { socket, text: r#"{"symbol":"BTCUSDT","type":"incremental","kline_p":[[1700000060,60,0,"105","115","95","110","3",0]]}"# });
    let live = e.take_frame(4.).expect("valid incremental candle");
    assert!(
        safe_cex_engine::wire::sections(live)
            .any(|(kind, _)| kind == safe_cex_engine::wire::section::CANDLES)
    );
    assert_eq!(snapshot_receipt, (3, 1.));
}
