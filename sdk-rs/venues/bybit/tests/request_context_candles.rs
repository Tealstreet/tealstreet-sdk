//! Slice 1a: Bybit candles, the cached balance-info answer and the
//! product-not-enabled refusal on the engine's managed Read authority.
mod support;
use safe_cex_engine::model::{Interval, OrderSide, OrderType, PlaceOrderOpts};
use safe_cex_engine::{Command, ErrorKind, Event, Notice, Outcome, Query, VenueAction, VenueError};
use safe_cex_venue_bybit::Variant;
use support::{Harness, Options};

const EMPTY: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[],"nextPageCursor":""}}"#;

fn ready() -> Harness {
    let mut h = Harness::started(Options {
        variant: Variant::Linear,
        key: Some("key".into()),
        secret: Some("secret".into()),
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

fn order() -> PlaceOrderOpts {
    PlaceOrderOpts {
        symbol: "BTCUSDT".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        amount: 0.01,
        price: Some(100.0),
        trigger_price: None,
        stop_loss: None,
        take_profit: None,
        reduce_only: None,
        time_in_force: None,
        trigger_type: None,
        tp_trigger_type: None,
        sl_trigger_type: None,
        client_order_id: None,
        close: None,
        stp_mode: None,
        implied_volatility: None,
        max_slippage_pct: None,
        bybit_bbo: None,
        bybit_slippage_tolerance_ticks: None,
    }
}

/// Park the engine's command owners with ordinary placements whose mode
/// reads stay unanswered; returns the parked mode reads.
fn park_owners(h: &mut Harness) -> Vec<safe_cex_engine::HttpRequest> {
    let mut reads = Vec::new();
    for command in 1_000..1_256 {
        h.command(Command::PlaceOrders {
            command,
            orders: vec![order()],
        });
        if h.has_request("/v5/position/list") {
            reads.push(h.take("/v5/position/list"));
        }
    }
    reads
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
    for (command, symbol, interval) in [
        (42, "BTC-27MAR26-100000-C", Interval::M1),
        (43, "BTCUSD", Interval::M1),
        (44, "BTCUSDT", Interval::S1),
    ] {
        let mut h = ready();
        candles(&mut h, command, symbol, interval);
        assert!(h.requests.is_empty(), "{symbol}: nothing is sent");
        let error = failure(&h, command);
        assert_eq!(error.outcome, Some(Outcome::NotSent), "{symbol}");
        assert!(error.kind.is_some(), "{symbol}: typed refusal");
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
fn cached_balance_info_and_disabled_product_refusals_need_command_authority() {
    let mut h = ready();
    let reads = park_owners(&mut h);
    assert!(!reads.is_empty());
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
    for command in [46, 47] {
        let error = failure(&h, command);
        assert_eq!(
            error.kind,
            Some(ErrorKind::Internal),
            "{command}: the owner cap refuses before any answer"
        );
        assert_eq!(h.done.iter().filter(|(id, _)| *id == command).count(), 1);
    }
    assert!(
        h.requests.is_empty(),
        "no HTTP for a cached or refused answer"
    );
    // Freeing one owner lets the cached answer complete with no I/O.
    let read = reads.into_iter().next().unwrap();
    h.reply(read.id, 200, r#"{"retCode":10001,"retMsg":"params error"}"#);
    h.requests.clear();
    h.command(Command::Venue {
        command: 48,
        action: VenueAction::FetchBalanceInfo,
    });
    assert_eq!(h.result(48), Some(&Ok("null".to_owned())));
    assert!(h.requests.is_empty());
}
