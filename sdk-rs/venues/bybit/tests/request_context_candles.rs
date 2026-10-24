//! Slice 1a: Bybit candles, the cached balance-info answer and the
//! product-not-enabled refusal on the engine's managed Read authority.
mod support;
use safe_cex_engine::model::Interval;
use safe_cex_engine::{Command, ErrorKind, Event, Notice, Outcome, Query, VenueAction, VenueError};
use safe_cex_venue_bybit::Variant;
use support::{Harness, Options};

const EMPTY: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[],"nextPageCursor":""}}"#;

fn ready() -> Harness {
    ready_with(false)
}

fn ready_with(enable_option_markets: bool) -> Harness {
    let mut h = Harness::started(Options {
        variant: Variant::Linear,
        key: Some("key".into()),
        secret: Some("secret".into()),
        enable_option_markets,
        ..Options::default()
    });
    for path in [
        "/v5/account/info",
        "/v5/account/fee-rate",
        "status=PreLaunch",
    ] {
        if h.has_request(path) {
            h.respond(
                path,
                200,
                if path == "/v5/account/info" {
                    support::ACCOUNT_INFO
                } else {
                    EMPTY
                },
            );
        }
    }
    h.respond("/v5/market/instruments-info", 200, support::INSTRUMENTS);
    h.respond("/v5/market/tickers", 200, support::TICKERS);
    h.requests.clear();
    h
}

fn candles(h: &mut Harness, command: u32, symbol: &str, interval: Interval) {
    h.command(Command::FetchCandles {
        command,
        symbol: symbol.into(),
        interval,
        limit: Some(2),
        from_ms: None,
        to_ms: None,
    });
}

fn failure(h: &Harness, command: u32) -> VenueError {
    h.notices
        .iter()
        .find_map(|notice| match notice {
            Notice::CommandFailed { command: id, error } if *id == command => Some(error.clone()),
            _ => None,
        })
        .expect("one typed failure")
}

#[test]
fn candles_carry_a_native_window_that_expires_as_a_timeout() {
    let mut h = ready();
    candles(&mut h, 41, "BTCUSDT", Interval::M1);
    let kline = h.take("/v5/market/kline");
    assert_eq!(kline.timeout_ms, Some(20_000));
    h.advance(19_999.);
    assert_eq!(h.result(41), None, "the window is still open");
    h.advance(2.);
    let error = failure(&h, 41);
    assert_eq!(error.kind, Some(ErrorKind::Timeout));
    assert_eq!(error.outcome, None, "a read carries no write outcome");
    // A late answer cannot publish or complete again.
    h.reply(
        kline.id,
        200,
        r#"{"retCode":0,"result":{"category":"linear","symbol":"BTCUSDT","list":[["1000","1","2","0.5","1.5","10","15"]]}}"#,
    );
    assert_eq!(h.done.iter().filter(|(id, _)| *id == 41).count(), 1);
}

#[test]
fn candle_refusals_before_any_request_are_typed_not_sent() {
    // An enabled option product reaches the venue's own option refusal; a
    // disabled product is refused before dispatch.
    for (command, options, symbol, interval, kind) in [
        (
            42,
            true,
            "BTC-27MAR26-100000-C",
            Interval::M1,
            ErrorKind::Unsupported,
        ),
        // A disabled product is the account's configuration, not a missing command.
        (43, false, "BTCUSD", Interval::M1, ErrorKind::InvalidInput),
        (44, false, "BTCUSDT", Interval::S1, ErrorKind::Unsupported),
    ] {
        let mut h = ready_with(options);
        h.requests.clear();
        candles(&mut h, command, symbol, interval);
        assert!(h.requests.is_empty(), "{symbol}: nothing is sent");
        let error = failure(&h, command);
        assert_eq!(error.outcome, Some(Outcome::NotSent), "{symbol}");
        assert_eq!(error.kind, Some(kind), "{symbol}");
    }
}

#[test]
fn a_candle_stream_ending_mid_list_is_a_malformed_reply_and_publishes_nothing() {
    let mut h = ready();
    candles(&mut h, 45, "BTCUSDT", Interval::M1);
    let kline = h.take("/v5/market/kline");
    h.event(Event::HttpStart {
        request: kline.id,
        status: 200,
        headers: "",
    });
    h.event(Event::HttpChunk {
        request: kline.id,
        data: br#"{"retCode":0,"result":{"category":"linear","symbol":"BTCUSDT","list":[["1000","1","2","0.5","1.5","10","15"],"#,
    });
    h.event(Event::HttpEnd { request: kline.id });
    let error = failure(&h, 45);
    assert_eq!(error.kind, Some(ErrorKind::Internal));
    assert!(
        !h.done
            .iter()
            .any(|(id, result)| *id == 45 && result.is_ok())
    );
}

#[test]
fn cached_balance_info_and_trade_refusals_at_capacity_need_command_authority() {
    let mut h = ready();
    // Park until admission refuses: background reads hold some slots, so the
    // available capacity is measured, not assumed.
    let reads = support::park_owners(&mut h);
    let refused = h
        .done
        .iter()
        .filter(|(id, result)| (1_000..1_256).contains(id) && result.is_err())
        .count();
    assert!(
        !reads.is_empty() && refused > 0,
        "the park reaches capacity"
    );
    assert_eq!(reads.len() + refused, 256);
    h.requests.clear();
    h.command(Command::Venue {
        command: 46,
        action: VenueAction::FetchBalanceInfo,
    });
    h.command(Command::Query {
        command: 47,
        query: Query::PublicTrades {
            symbol: "BTCUSD".into(),
            limit: Some(1),
        },
    });
    // A write on an enabled product needs a read slot for its plan.
    h.command(Command::SetLeverage {
        command: 49,
        symbol: "BTCUSDT".into(),
        leverage: 2.0,
        side: None,
    });
    assert_eq!(failure(&h, 49).outcome, Some(Outcome::NotSent));
    for command in [46, 47, 49] {
        let error = failure(&h, command);
        assert_eq!(
            error.kind,
            Some(ErrorKind::Internal),
            "{command}: capacity refuses before any answer"
        );
        assert_eq!(h.done.iter().filter(|(id, _)| *id == command).count(), 1);
    }
    assert!(
        h.requests.is_empty(),
        "no HTTP for a cached or refused answer"
    );
}

#[test]
fn a_disabled_product_write_refuses_at_its_gate_before_any_io() {
    let mut h = ready();
    h.command(Command::SetLeverage {
        command: 49,
        symbol: "BTC-27MAR26-100000-C".into(),
        leverage: 2.0,
        side: None,
    });
    let error = failure(&h, 49);
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
    assert_eq!(h.done.iter().filter(|(id, _)| *id == 49).count(), 1);
    assert!(h.requests.is_empty(), "the gate answers before any I/O");
}

#[test]
fn kline_failures_keep_native_and_transport_evidence() {
    // A native retCode refusal is classified from the one code table.
    let mut h = ready();
    candles(&mut h, 50, "BTCUSDT", Interval::M1);
    let kline = h.take("/v5/market/kline");
    h.reply(
        kline.id,
        200,
        r#"{"retCode":10001,"retMsg":"params error"}"#,
    );
    let error = failure(&h, 50);
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
    assert_eq!(error.venue_code.as_deref(), Some("10001"));
    assert!(error.message.starts_with("Bybit klines: "));
    // A bare gateway error carries no native code.
    let mut h = ready();
    candles(&mut h, 51, "BTCUSDT", Interval::M1);
    let kline = h.take("/v5/market/kline");
    h.reply(kline.id, 503, "<html>unavailable</html>");
    let error = failure(&h, 51);
    assert!(error.kind.is_some());
    assert_eq!(error.venue_code, None);
    // Transport loss is Network.
    let mut h = ready();
    candles(&mut h, 52, "BTCUSDT", Interval::M1);
    let kline = h.take("/v5/market/kline");
    h.event(Event::HttpFailed {
        request: kline.id,
        error: "connection reset",
    });
    assert_eq!(failure(&h, 52).kind, Some(ErrorKind::Network));
}
