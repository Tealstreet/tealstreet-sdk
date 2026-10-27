//! Binance feeds run on the engine's managed sockets: retirement reasons, a
//! single replay on reopen, and nothing reduced outside an open, live epoch.
use support::Options;
mod support;
use safe_cex_engine::{Command, Event, Method, SocketId};
use safe_cex_venue_binance::Variant;
use support::Harness;

const NEW_ORDER: &str = r#"{"e":"ORDER_TRADE_UPDATE","E":1,"T":1,"o":{"s":"BTCUSDT","c":"late","S":"SELL","o":"LIMIT","f":"GTC","q":"0.002","p":"97000","ap":"0","sp":"0","x":"NEW","X":"NEW","i":4242,"l":"0","z":"0","L":"0","T":1,"t":0,"b":"0","a":"0","m":false,"R":false,"wt":"CONTRACT_PRICE","ot":"LIMIT","ps":"BOTH","cp":false,"rp":"0"}}"#;

/// A USD-M account with every startup read answered and all three feeds open.
fn ready() -> Harness {
    let mut h = Harness::started(Options {
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    h.respond("/fapi/v1/exchangeInfo", 200, support::EXCHANGE_INFO);
    h.respond("/fapi/v1/leverageBracket", 200, "[]");
    h.respond("/fapi/v1/ticker/24hr", 200, support::DAILY);
    h.respond("/fapi/v1/ticker/bookTicker", 200, support::BOOK_TICKERS);
    h.respond("/fapi/v1/premiumIndex", 200, support::PREMIUM);
    h.respond("/fapi/v1/listenKey", 200, r#"{"listenKey":"first"}"#);
    h.respond(
        "/fapi/v1/positionSide/dual",
        200,
        r#"{"dualSidePosition":false}"#,
    );
    h.respond("/fapi/v3/positionRisk", 200, "[]");
    h.respond("/fapi/v1/symbolConfig", 200, "[]");
    h.respond(
        "/fapi/v3/account",
        200,
        r#"{"assets":[{"asset":"USDT","walletBalance":"100","availableBalance":"100","initialMargin":"0","unrealizedProfit":"0"}]}"#,
    );
    h.respond("/fapi/v1/openOrders", 200, "[]");
    h.respond("/fapi/v1/openAlgoOrders", 200, "[]");
    for path in ["/public/ws", "/market/ws", "/private/ws"] {
        let socket = h.socket(path).unwrap();
        h.event(Event::WsOpened { socket });
    }
    h
}

fn sent_on(h: &Harness, socket: SocketId) -> Vec<&str> {
    h.sent
        .iter()
        .filter(|(s, _)| *s == socket)
        .map(|(_, t)| t.as_str())
        .collect()
}

fn logged(h: &Harness, message: &str) -> bool {
    h.logs.iter().any(|log| log == message)
}

#[test]
fn an_expired_listen_key_retires_the_user_stream_for_authentication_and_resyncs_orders() {
    let mut h = ready();
    let private = h.socket("/private/ws").unwrap();
    h.message(
        "/private/ws",
        r#"{"e":"listenKeyExpired","E":1,"listenKey":"first"}"#,
    );
    assert!(h.closed.contains(&private));
    assert!(
        logged(&h, "Binance socket authentication failed; reconnecting"),
        "{:?}",
        h.logs
    );
    h.event(Event::WsMessage {
        socket: private,
        text: NEW_ORDER,
    });
    assert!(h.engine.store().order("4242").is_none(), "retired epoch");

    // The engine's backoff grants the reconnect; the venue fetches a fresh key.
    h.advance(500.0);
    let create = h.take("/fapi/v1/listenKey");
    assert_eq!(create.method, Method::Post);
    h.reply(create.id, 200, "", r#"{"listenKey":"fresh"}"#);
    let socket = h.socket("/private/ws").unwrap();
    assert_ne!(socket, private);
    assert!(h.url_of(socket).ends_with("?listenKey=fresh"));

    h.event(Event::WsMessage {
        socket,
        text: NEW_ORDER,
    });
    assert!(
        h.engine.store().order("4242").is_none(),
        "a frame before the open is not reduced"
    );
    assert!(!h.has_request("/fapi/v1/openOrders"));
    h.event(Event::WsOpened { socket });
    h.advance(10_000.0);
    assert!(
        h.has_request("/fapi/v1/openOrders"),
        "the reopened stream resyncs orders: {:?}",
        h.paths()
    );
}

/// The reconnect notice names the venue's close code, as the legacy socket did.
#[test]
fn a_close_names_its_code_in_the_reconnect_notice() {
    let mut h = Harness::new();
    let socket = h.socket("/public/ws").unwrap();
    h.event(Event::WsClosed { socket, code: 1013 });
    assert!(
        logged(&h, "Binance socket closed (1013); reconnecting"),
        "{:?}",
        h.logs
    );
}

#[test]
fn a_dropped_public_feed_replays_its_current_streams_once_on_reopen() {
    let mut h = Harness::new();
    let old = h.socket("/public/ws").unwrap();
    h.event(Event::WsClosed {
        socket: old,
        code: 1006,
    });
    assert!(
        logged(&h, "Binance socket closed; reconnecting"),
        "{:?}",
        h.logs
    );
    // Streams change while the feed is down; nothing is sent until it reopens.
    h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: "ETHUSDT".into(),
    }));
    h.event(Event::Command(Command::UnsubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    }));
    let before = h.sent.len();
    h.advance(500.0);
    let socket = h.socket("/public/ws").unwrap();
    assert_ne!(socket, old);
    assert_eq!(h.sent.len(), before);

    let snapshots = h.snapshot_requests();
    h.event(Event::WsMessage {
        socket,
        text: r#"{"e":"depthUpdate","E":1,"T":1,"s":"ETHUSDT","U":1,"u":2,"pu":0,"b":[["100","1"]],"a":[]}"#,
    });
    assert_eq!(h.snapshot_requests(), snapshots, "unopened frames ignored");

    h.event(Event::WsOpened { socket });
    h.event(Event::WsOpened { socket });
    let frames = sent_on(&h, socket);
    assert_eq!(
        frames.len(),
        1,
        "one replay, even on a duplicate open: {frames:?}"
    );
    assert!(frames[0].contains("\"method\":\"SUBSCRIBE\""));
    assert!(frames[0].contains("ethusdt@depth"));
    assert!(!frames[0].contains("btcusdt@depth"));
}

#[test]
fn a_frame_at_the_idle_deadline_retires_the_feed_instead_of_publishing() {
    let mut h = Harness::new();
    let market = h.socket("/market/ws").unwrap();
    // The watchdog timer has not been delivered yet when this frame arrives.
    h.now += 20_000.0;
    h.message(
        "/market/ws",
        r#"[{"e":"24hrTicker","s":"BTCUSDT","c":"123","P":"1","v":"1","q":"1"}]"#,
    );
    assert_eq!(h.engine.store().tickers()[0].last, 100.0, "stale frame");
    assert!(h.closed.contains(&market));
    assert!(
        logged(&h, "Binance socket closed (idle timeout); reconnecting"),
        "{:?}",
        h.logs
    );
    h.advance(500.0);
    assert_ne!(
        h.socket("/market/ws"),
        Some(market),
        "the engine reconnects"
    );
}

#[test]
fn a_rejected_spot_user_data_subscription_retires_for_authentication_and_resubscribes() {
    let mut h = Harness::started(Options {
        variant: Variant::Spot,
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    h.respond(
        "/api/v3/exchangeInfo",
        200,
        r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","baseAssetPrecision":8,"quoteAssetPrecision":8,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.01"},{"filterType":"LOT_SIZE","stepSize":"0.00001","minQty":"0.00001","maxQty":"9000"}]}]}"#,
    );
    h.respond("/api/v3/ticker/24hr", 200, r#"[{"symbol":"BTCUSDT","lastPrice":"100","bidPrice":"99","askPrice":"101","priceChangePercent":"2","volume":"10","quoteVolume":"1000"}]"#);
    let private = h.socket("ws-api/v3").unwrap();
    h.event(Event::WsOpened { socket: private });
    assert!(sent_on(&h, private)[0].contains("\"id\":\"tealstreet-user-data-10\""));

    h.message(
        "ws-api/v3",
        r#"{"id":"tealstreet-user-data-10","status":401,"error":{"code":-2015,"msg":"Invalid API-key"}}"#,
    );
    assert!(h.closed.contains(&private));
    assert!(
        logged(&h, "Binance socket authentication failed; reconnecting"),
        "{:?}",
        h.logs
    );

    h.advance(500.0);
    let socket = h.socket("ws-api/v3").unwrap();
    assert_ne!(socket, private);
    h.event(Event::WsOpened { socket });
    let sent = sent_on(&h, socket);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(sent[0].contains("userDataStream.subscribe.signature"));
    assert!(sent[0].contains("\"id\":\"tealstreet-user-data-11\""));
}
