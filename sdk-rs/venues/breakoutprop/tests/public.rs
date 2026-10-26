mod support;
use safe_cex_engine::{Command, Event, Method, Notice};
use safe_cex_venue_breakoutprop::{Options, parse_session_timeout_ms};
use support::*;

/// Native cadence of the venue's recurring account poll; policy scales it.
const POLL_MS: f64 = 2_000.0;

#[test]
fn documented_market_steps_and_margin_rates_are_not_decimal_counts() {
    let mut rig = Rig::defaults();
    rig.boot();
    let market = rig.engine.store().market("ETHUSD").unwrap();
    assert_eq!(market.precision.price, 0.01);
    assert_eq!(market.precision.amount, 0.01);
    assert_eq!(market.limits.amount.min, 0.01);
    assert_eq!(market.limits.amount.max, 724.0);
    assert_eq!(market.limits.leverage.max, 5.0);
    assert_eq!(market.contract_size, Some(1.0));
    assert_eq!(market.base, "ETH");
    assert!(rig.engine.store().market("XBTUSD").is_none());
    assert_eq!(rig.dispatch_times.len(), 5);
    assert!(rig.dispatch_times[4] - rig.dispatch_times[0] < 10.0);
}

#[test]
fn login_has_basic_body_and_further_requests_use_dxapi_session() {
    let mut rig = Rig::defaults();
    let login = rig.next_request();
    assert_eq!(login.method, Method::Post);
    assert_eq!(
        login.body.as_deref(),
        Some(r#"{"domain":"default","username":"example-user","password":"example-password"}"#)
    );
    assert!(
        login
            .headers
            .iter()
            .all(|(name, _)| *name != "Authorization")
    );
    assert_eq!(login.timeout_ms, Some(20_000));
    rig.reply(&login, 200, LOGIN);
    let user = rig.next_request();
    assert!(
        user.headers
            .contains(&("Authorization", "DXAPI example-session".into()))
    );
    rig.reply(&user, 200, USER);
    let instruments = rig.next_request();
    assert!(
        instruments
            .headers
            .contains(&("Authorization", "DXAPI example-session".into()))
    );
}

#[test]
fn timeout_parsing_is_bounded_positive_and_rejects_malformed_parts() {
    assert_eq!(parse_session_timeout_ms("01:02:03"), Ok(3_723_000.0));
    for timeout in [
        "00:60:00",
        "00:00:60",
        "-1:00:00",
        "x:00:00",
        "00:00",
        "00:00:00",
        "1::2",
        "1.5:00:00",
    ] {
        assert!(parse_session_timeout_ms(timeout).is_err(), "{timeout}");
    }
}

#[test]
fn multiple_market_requests_share_one_login_and_one_discovery() {
    let mut rig = Rig::defaults();
    let login = rig.next_request();
    rig.command(Command::FetchCandles {
        command: 9,
        symbol: "ETHUSD".into(),
        interval: safe_cex_engine::model::Interval::H1,
        from_ms: None,
        to_ms: None,
        limit: None,
    });
    assert!(rig.requests.is_empty());
    rig.reply(&login, 200, LOGIN);
    let user = rig.next_request();
    assert!(user.url.contains("/users/"));
    rig.reply(&user, 200, USER);
    let first = rig.next_request();
    let second = rig.next_request();
    let third = rig.next_request();
    assert!(first.url.contains("instruments/query"));
    assert!(second.url.contains("instruments/query"));
    assert!(third.url.ends_with("marketdata"));
}

#[test]
fn quote_prices_seed_zero_funding_ticker_and_one_placeholder_book_level() {
    let mut rig = Rig::defaults();
    rig.command(Command::SubscribeOrderBook {
        symbol: "ETHUSD".into(),
    });
    rig.boot();
    let ticker = rig.engine.store().ticker("ETHUSD").unwrap();
    assert_eq!(
        (
            ticker.bid,
            ticker.ask,
            ticker.last,
            ticker.mark,
            ticker.index
        ),
        (3656.01, 3656.02, 3656.01, 3656.01, 3656.01)
    );
    assert_eq!(
        (
            ticker.funding_rate,
            ticker.funding_time,
            ticker.volume,
            ticker.percentage
        ),
        (0.0, 0.0, 0.0, 0.0)
    );
    let book = rig.engine.book("ETHUSD").unwrap();
    assert_eq!(book.bids(), &[(3656.01, 1.0)]);
    assert_eq!(book.asks(), &[(3656.02, 1.0)]);
}

#[test]
fn book_subscriptions_are_reference_counted() {
    let mut rig = Rig::defaults();
    for _ in 0..2 {
        rig.command(Command::SubscribeOrderBook {
            symbol: "ETHUSD".into(),
        });
    }
    rig.boot();
    rig.command(Command::UnsubscribeOrderBook {
        symbol: "ETHUSD".into(),
    });
    assert!(rig.engine.book("ETHUSD").is_some());
    rig.command(Command::UnsubscribeOrderBook {
        symbol: "ETHUSD".into(),
    });
    assert!(rig.engine.book("ETHUSD").is_none());
}

#[test]
fn public_only_keeps_polling_authenticated_quotes_without_a_socket() {
    let mut rig = Rig::defaults();
    rig.boot();
    let poll = rig.next_request();
    assert!(poll.url.ends_with("marketdata"));
    assert_eq!(
        poll.body.as_deref(),
        Some(r#"{"eventTypes":[{"type":"Quote"}],"symbols":["ETHUSD"],"account":"default:00002"}"#)
    );
    rig.reply(&poll, 200, QUOTES);
    assert!(rig.next_request().url.ends_with("marketdata"));
}

#[test]
fn malformed_quote_snapshot_keeps_last_known_good_tickers_and_book() {
    let mut rig = Rig::defaults();
    rig.command(Command::SubscribeOrderBook {
        symbol: "ETHUSD".into(),
    });
    rig.boot();
    let poll = rig.next_request();
    rig.reply(&poll, 200, r#"{"events":[{"symbol":"ETHUSD","type":"Quote","bid":10,"ask":11,"time":"2024-12-11T07:55:11Z"},{"symbol":"ETHUSD","type":"Quote","bid":12,"ask":13,"time":"not-a-date"}]}"#);
    assert_eq!(rig.engine.store().ticker("ETHUSD").unwrap().bid, 3656.01);
    assert_eq!(rig.engine.book("ETHUSD").unwrap().best_bid(), Some(3656.01));
}

fn quote_snapshot_rig() -> Rig {
    let mut rig = Rig::defaults();
    for symbol in ["ETHUSD", "BTCUSD"] {
        rig.command(Command::SubscribeOrderBook {
            symbol: symbol.into(),
        });
    }
    for body in [
        LOGIN.to_owned(),
        USER.to_owned(),
        INSTRUMENTS.replace("XBTUSD", "BTCUSD"),
        DETAILS.replace("XBTUSD", "BTCUSD"),
        r#"{"events":[{"symbol":"ETHUSD","type":"Quote","bid":10,"ask":11,"time":"2024-12-11T07:55:11.000Z"},{"symbol":"BTCUSD","type":"Quote","bid":20,"ask":21,"time":"2024-12-11T07:55:11.000Z"}]}"#.into(),
    ] {
        let request = rig.next_request();
        rig.reply(&request, 200, &body);
    }
    rig
}

#[test]
fn shared_quote_timestamp_keeps_all_rows_and_different_invalid_time_rejects_atomically() {
    let mut rig = quote_snapshot_rig();
    assert_eq!(rig.engine.store().tickers().len(), 2);
    assert_eq!(rig.engine.book("ETHUSD").unwrap().best_bid(), Some(10.0));
    assert_eq!(rig.engine.book("BTCUSD").unwrap().best_bid(), Some(20.0));
    let before = rig.engine.store().tickers().to_vec();
    for time in [
        "2024-02-30T07:55:11.000Z",
        "2024-12-11T07:55:11.000+25:00",
        "not-a-date",
        "",
    ] {
        let poll = rig.next_request();
        rig.reply(&poll, 200, &format!(r#"{{"events":[{{"symbol":"ETHUSD","type":"Quote","bid":30,"ask":31,"time":"2024-12-11T07:55:11.000Z"}},{{"symbol":"BTCUSD","type":"Quote","bid":40,"ask":41,"time":"{time}"}}]}}"#));
        assert_eq!(rig.engine.store().tickers(), before, "{time}");
        assert_eq!(rig.engine.book("ETHUSD").unwrap().best_bid(), Some(10.0));
        assert_eq!(rig.engine.book("BTCUSD").unwrap().best_bid(), Some(20.0));
    }
    let poll = rig.next_request();
    rig.reply(&poll, 200, r#"{"events":[{"symbol":"ETHUSD","type":"Quote","bid":30,"ask":31,"time":"2024-12-11T07:55:12Z"},{"symbol":"BTCUSD","type":"Quote","bid":40,"ask":41,"time":"2024-12-11T08:55:12+01:00"}]}"#);
    assert_eq!(rig.engine.book("ETHUSD").unwrap().best_bid(), Some(30.0));
    assert_eq!(rig.engine.book("BTCUSD").unwrap().best_bid(), Some(40.0));
}

#[test]
fn failed_quote_stream_retry_discards_cached_snapshot_rows_and_revalidates_new_time() {
    let mut rig = quote_snapshot_rig();
    let before = rig.engine.store().tickers().to_vec();
    let poll = rig.next_request();
    rig.engine.handle(
        rig.now,
        Event::HttpStart {
            request: poll.id,
            status: 200,
            headers: "",
        },
    );
    rig.engine.handle(rig.now, Event::HttpChunk { request: poll.id, data: br#"{"events":[{"symbol":"ETHUSD","type":"Quote","bid":30,"ask":31,"time":"2024-12-11T07:55:11.000Z"},"# });
    rig.failure(&poll);
    let retry = rig.next_request();
    rig.reply(&retry, 200, r#"{"events":[{"symbol":"ETHUSD","type":"Quote","bid":50,"ask":51,"time":"2024-02-30T07:55:11.000Z"},{"symbol":"BTCUSD","type":"Quote","bid":60,"ask":61,"time":"2024-02-30T07:55:11.000Z"}]}"#);
    assert_eq!(rig.engine.store().tickers(), before);
    assert_eq!(rig.engine.book("ETHUSD").unwrap().best_bid(), Some(10.0));
    let poll = rig.next_request();
    rig.reply(&poll, 200, r#"{"events":[{"symbol":"ETHUSD","type":"Quote","bid":50,"ask":51,"time":"2024-12-11T07:55:11.000Z"},{"symbol":"BTCUSD","type":"Quote","bid":60,"ask":61,"time":"2024-12-11T07:55:11.000Z"}]}"#);
    assert_eq!(rig.engine.store().tickers().len(), 2);
    assert_eq!(rig.engine.book("ETHUSD").unwrap().best_bid(), Some(50.0));
    assert_eq!(rig.engine.book("BTCUSD").unwrap().best_bid(), Some(60.0));
}

#[test]
fn first_401_refreshes_and_resends_once_while_second_fails() {
    let mut rig = Rig::defaults();
    rig.boot();
    rig.command(Command::FetchCandles {
        command: 9,
        symbol: "ETHUSD".into(),
        interval: safe_cex_engine::model::Interval::H1,
        from_ms: None,
        to_ms: None,
        limit: None,
    });
    let original = rig.next_request();
    rig.reply(&original, 401, "{}");
    let login = rig.next_request();
    assert!(login.url.ends_with("login"));
    rig.reply(
        &login,
        200,
        r#"{"sessionToken":"new-session","timeout":"00:30:00"}"#,
    );
    let retry = rig.next_request();
    assert_eq!(retry.body, original.body);
    assert!(
        retry
            .headers
            .contains(&("Authorization", "DXAPI new-session".into()))
    );
    rig.reply(&retry, 401, "{}");
    assert!(rig.result(9).unwrap_err().contains("401"));
}

#[test]
fn late_old_session_401_reuses_new_session_without_second_login() {
    let mut rig = Rig::defaults();
    rig.boot();
    for command in [8, 9] {
        rig.command(Command::FetchCandles {
            command,
            symbol: "ETHUSD".into(),
            interval: safe_cex_engine::model::Interval::H1,
            from_ms: None,
            to_ms: None,
            limit: None,
        });
    }
    let first = rig.next_request();
    let second = rig.next_request();
    rig.reply(&first, 401, "{}");
    let login = rig.next_request();
    assert!(login.url.ends_with("login"));
    rig.reply(
        &login,
        200,
        r#"{"sessionToken":"new-session","timeout":"00:30:00"}"#,
    );
    rig.reply(&second, 401, "{}");
    for _ in 0..2 {
        let retry = rig.next_request();
        assert!(retry.url.ends_with("marketdata"));
        assert!(
            retry
                .headers
                .contains(&("Authorization", "DXAPI new-session".into()))
        );
        rig.reply(&retry, 200, r#"{"events":[]}"#);
    }
    assert!(rig.result(8).is_ok());
    assert!(rig.result(9).is_ok());
}

#[test]
fn simultaneous_401s_share_one_reauthentication() {
    let mut rig = Rig::defaults();
    rig.boot();
    for command in [8, 9] {
        rig.command(Command::FetchCandles {
            command,
            symbol: "ETHUSD".into(),
            interval: safe_cex_engine::model::Interval::H1,
            from_ms: None,
            to_ms: None,
            limit: None,
        });
    }
    let first = rig.next_request();
    let second = rig.next_request();
    rig.reply(&first, 401, "{}");
    rig.reply(&second, 401, "{}");
    let login = rig.next_request();
    assert!(login.url.ends_with("login"));
    rig.reply(&login, 200, LOGIN);
    for _ in 0..2 {
        let retry = rig.next_request();
        assert!(retry.url.ends_with("marketdata"));
        rig.reply(&retry, 200, r#"{"events":[]}"#);
    }
    assert!(rig.result(8).is_ok());
    assert!(rig.result(9).is_ok());
}

#[test]
fn expired_cached_session_logs_in_before_next_poll_and_keeps_account() {
    let mut rig = Rig::defaults();
    let login = rig.next_request();
    rig.reply(
        &login,
        200,
        r#"{"sessionToken":"short","timeout":"00:00:01"}"#,
    );
    for body in [USER, INSTRUMENTS, DETAILS, QUOTES] {
        let request = rig.next_request();
        rig.reply(&request, 200, body);
    }
    let refresh = rig.next_request();
    assert!(refresh.url.ends_with("login"));
    rig.reply(&refresh, 200, LOGIN);
    let poll = rig.next_request();
    assert!(poll.url.ends_with("marketdata"));
    assert!(poll.body.unwrap().contains("default:00002"));
}

#[test]
fn login_401_does_not_loop_and_does_not_leak_password_or_response_body() {
    let mut rig = Rig::defaults();
    rig.command(Command::FetchCandles {
        command: 9,
        symbol: "ETHUSD".into(),
        interval: safe_cex_engine::model::Interval::H1,
        from_ms: None,
        to_ms: None,
        limit: None,
    });
    let login = rig.next_request();
    rig.reply(
        &login,
        401,
        r#"{"message":"example-password secret-token"}"#,
    );
    assert!(rig.requests.is_empty());
    let error = rig.result(9).unwrap_err();
    assert_eq!(error, "DXtrade HTTP 401");
    assert!(!format!("{:?}", rig.notices).contains("example-password"));
}

#[test]
fn network_retries_have_three_attempt_limit() {
    let mut rig = Rig::defaults();
    rig.boot();
    rig.command(Command::FetchCandles {
        command: 9,
        symbol: "ETHUSD".into(),
        interval: safe_cex_engine::model::Interval::H1,
        from_ms: None,
        to_ms: None,
        limit: None,
    });
    for _ in 0..4 {
        loop {
            let request = rig.next_request();
            if request.body.as_ref().unwrap().contains("Candle") {
                rig.failure(&request);
                break;
            }
            rig.reply(&request, 200, QUOTES);
        }
    }
    assert_eq!(rig.result(9).unwrap_err(), "DXtrade transport failed");
}

#[test]
fn exact_subaccount_suffix_cannot_accidentally_select_a_longer_number() {
    let mut rig = Rig::new(Options {
        key: "example-user".into(),
        secret: "password".into(),
        subaccount: Some("2".into()),
        ..Options::default()
    });
    let login = rig.next_request();
    rig.reply(&login, 200, LOGIN);
    let user = rig.next_request();
    rig.reply(&user,200,r#"{"userDetails":[{"login":"example-user","domain":"default","accounts":[{"account":"default:12","accountStatus":"FULL_TRADING"},{"account":"default:2","accountStatus":"FULL_TRADING"},{"account":"default:3","accountStatus":"CLOSE_ONLY"}]}]}"#);
    let instruments = rig.next_request();
    assert!(instruments.url.ends_with("accountCode=default%3A2"));
}

#[test]
fn ambiguous_account_discovery_fails_before_market_requests() {
    let mut rig = Rig::defaults();
    rig.command(Command::FetchCandles {
        command: 9,
        symbol: "ETHUSD".into(),
        interval: safe_cex_engine::model::Interval::H1,
        from_ms: None,
        to_ms: None,
        limit: None,
    });
    let login = rig.next_request();
    rig.reply(&login, 200, LOGIN);
    let user = rig.next_request();
    rig.reply(&user,200,r#"{"userDetails":[{"login":"example-user","domain":"default","accounts":[{"account":"default:12","accountStatus":"FULL_TRADING"},{"account":"default:2","accountStatus":"FULL_TRADING"}]}]}"#);
    assert!(
        rig.result(9)
            .as_ref()
            .unwrap_err()
            .contains("Multiple accounts")
    );
    assert!(rig.requests.is_empty());
}

#[test]
fn username_is_encoded_as_a_single_url_segment() {
    let mut rig = Rig::new(Options {
        key: "user/name?x=y".into(),
        secret: "password".into(),
        ..Options::default()
    });
    let login = rig.next_request();
    rig.reply(&login, 200, LOGIN);
    assert!(
        rig.next_request()
            .url
            .ends_with("users/user%2Fname%3Fx%3Dy")
    );
}

#[test]
fn historical_candles_parse_rfc3339_to_seconds_and_send_account() {
    let mut rig = Rig::defaults();
    rig.boot();
    rig.command(Command::FetchCandles {
        command: 9,
        symbol: "ETHUSD".into(),
        interval: safe_cex_engine::model::Interval::H1,
        from_ms: Some(1_733_907_600_000.0),
        to_ms: None,
        limit: None,
    });
    let request = rig.next_request();
    assert!(
        request
            .body
            .as_ref()
            .unwrap()
            .contains("2024-12-11T09:00:00.000Z")
    );
    assert!(
        request
            .body
            .as_ref()
            .unwrap()
            .contains(r#""account":"default:00002""#)
    );
    rig.reply(&request,200,r#"{"events":[{"symbol":"ETHUSD","type":"Candle","candleType":"h","open":3656.01,"close":3657,"high":3658,"low":3655,"volume":922.778,"time":"2024-12-11T09:00:00.125Z"}]}"#);
    assert!(rig.result(9).as_ref().unwrap().contains("1733907600.125"));
}

#[test]
fn unsupported_interval_fails_before_any_new_http_request() {
    let mut rig = Rig::defaults();
    rig.boot();
    rig.command(Command::FetchCandles {
        command: 9,
        symbol: "ETHUSD".into(),
        interval: safe_cex_engine::model::Interval::M3,
        from_ms: None,
        to_ms: None,
        limit: None,
    });
    assert!(rig.result(9).is_err());
    assert!(rig.requests.is_empty());
}

#[test]
fn partial_stream_failure_discards_accumulated_rows_before_retry() {
    let mut rig = Rig::defaults();
    rig.boot();
    rig.command(Command::FetchCandles {
        command: 9,
        symbol: "ETHUSD".into(),
        interval: safe_cex_engine::model::Interval::H1,
        from_ms: None,
        to_ms: None,
        limit: None,
    });
    let request = rig.next_request();
    rig.engine.handle(
        rig.now,
        Event::HttpStart {
            request: request.id,
            status: 200,
            headers: "",
        },
    );
    rig.engine.handle(rig.now,Event::HttpChunk{request:request.id,data:br#"{"events":[{"symbol":"ETHUSD","type":"Candle","candleType":"h","open":1,"close":1,"high":1,"low":1,"volume":0,"time":"2024-12-11T09:00:00Z"},"#});
    rig.failure(&request);
    let retry = rig.next_request();
    rig.reply(&retry, 200, r#"{"events":[]}"#);
    assert_eq!(rig.result(9).as_ref().unwrap(), "[]");
}

fn synthetic_candle_interval(interval: &str, seconds: f64) {
    let mut rig = Rig::defaults();
    rig.command(Command::SubscribeCandles {
        symbol: "ETHUSD".into(),
        interval: safe_cex_engine::model::Interval::parse(interval).unwrap(),
    });
    rig.boot();
    assert!(
        rig.engine.take_market_frame(rig.now).is_none(),
        "indexed candles wait for the pending market table"
    );
    let frame = rig
        .engine
        .take_frame(rig.now)
        .expect("startup market table and queued candle frame");
    assert!(
        safe_cex_engine::wire::sections(frame)
            .any(|(kind, _)| kind == safe_cex_engine::wire::section::MARKETS)
    );
    let mut at = 16;
    let mut candle = None;
    while at < frame.len() {
        let kind = u16::from_le_bytes(frame[at..at + 2].try_into().unwrap());
        let length = u32::from_le_bytes(frame[at + 4..at + 8].try_into().unwrap()) as usize;
        let body = at + 8;
        if kind == safe_cex_engine::wire::section::CANDLES {
            assert_eq!(
                u32::from_le_bytes(frame[body..body + 4].try_into().unwrap()),
                1
            );
            let values = (body + 12).next_multiple_of(8);
            candle = Some(std::array::from_fn::<_, 6, _>(|index| {
                f64::from_le_bytes(
                    frame[values + index * 8..values + index * 8 + 8]
                        .try_into()
                        .unwrap(),
                )
            }));
        }
        at = body + length;
    }
    let candle = candle.expect("synthetic candle section");
    assert_eq!(candle[0], (rig.now / 1_000.0 / seconds).floor() * seconds);
    assert_eq!(&candle[1..5], &[3656.01; 4]);
    assert_eq!(candle[5], 0.0);
    let request = rig.next_request();
    rig.reply(&request, 200, QUOTES);
    let live = rig
        .engine
        .take_market_frame(rig.now)
        .expect("live candle frame after the market table was consumed");
    assert!(
        safe_cex_engine::wire::sections(live)
            .any(|(kind, _)| kind == safe_cex_engine::wire::section::CANDLES)
    );
    assert!(
        safe_cex_engine::wire::sections(live)
            .all(|(kind, _)| kind != safe_cex_engine::wire::section::MARKETS)
    );
    rig.command(Command::UnsubscribeCandles {
        symbol: "ETHUSD".into(),
        interval: safe_cex_engine::model::Interval::parse(interval).unwrap(),
    });
    let request = rig.next_request();
    rig.reply(&request, 200, QUOTES);
    assert!(rig.engine.take_market_frame(rig.now).is_none());
}

#[test]
fn synthetic_three_minute_candles_do_not_require_a_native_history_interval() {
    synthetic_candle_interval("3m", 180.0);
}

#[test]
fn synthetic_six_hour_candles_do_not_require_a_native_history_interval() {
    synthetic_candle_interval("6h", 21_600.0);
}

#[test]
fn synthetic_twelve_hour_candles_do_not_require_a_native_history_interval() {
    synthetic_candle_interval("12h", 43_200.0);
}

#[test]
fn wrapper_ticker_interest_masks_never_limit_dx_account_quotes() {
    let mut rig = Rig::defaults();
    rig.command(Command::SetTickerSymbols {
        symbols: Vec::new(),
    });
    rig.boot();
    rig.command(Command::SetTickerSymbols {
        symbols: vec!["BTCUSD".into()],
    });
    let poll = rig.next_request();
    assert!(
        poll.body
            .as_ref()
            .unwrap()
            .contains(r#""symbols":["ETHUSD"]"#)
    );
    rig.reply(&poll, 200, QUOTES);
    rig.command(Command::SetTickerSymbols {
        symbols: Vec::new(),
    });
    let poll = rig.next_request();
    assert!(
        poll.body
            .as_ref()
            .unwrap()
            .contains(r#""symbols":["ETHUSD"]"#)
    );
}

#[test]
fn a_delayed_read_retry_cannot_hold_ready_work_past_the_rate_limit() {
    let mut rig = Rig::defaults();
    rig.boot();
    rig.command(Command::FetchCandles {
        command: 8,
        symbol: "ETHUSD".into(),
        interval: safe_cex_engine::model::Interval::H1,
        from_ms: None,
        to_ms: None,
        limit: None,
    });
    let old = rig.next_request();
    assert_eq!(rig.now, 1000.0);
    rig.failure(&old);
    let ready_at = rig.now;
    rig.command(Command::FetchCandles {
        command: 9,
        symbol: "ETHUSD".into(),
        interval: safe_cex_engine::model::Interval::M1,
        from_ms: None,
        to_ms: None,
        limit: None,
    });
    let fresh = rig.next_request();
    assert_eq!(rig.now, ready_at);
    assert!(fresh.body.as_ref().unwrap().contains(r#""candleType":"m""#));
    rig.reply(&fresh, 200, r#"{"events":[]}"#);
    assert!(rig.result(9).is_ok());
    let retry = rig.next_request();
    assert_eq!(rig.now, 2001.0);
    assert_eq!(retry.body, old.body);
}

#[test]
fn burst_budget_never_exceeds_five_requests_in_a_rolling_second() {
    let mut rig = Rig::defaults();
    rig.boot();
    for command in 10..30 {
        rig.command(Command::FetchCandles {
            command,
            symbol: "ETHUSD".into(),
            interval: safe_cex_engine::model::Interval::H1,
            from_ms: None,
            to_ms: None,
            limit: None,
        });
    }
    assert!(rig.requests.is_empty());
    for _ in 0..15 {
        rig.next_request();
    }
    assert!(rig.dispatch_times[5] >= 1_000.0);
    for (index, time) in rig.dispatch_times.iter().enumerate() {
        let in_window = rig.dispatch_times[..=index]
            .iter()
            .filter(|prior| **prior > time - 1_000.0)
            .count();
        assert!(
            in_window <= 5,
            "time {time}: {in_window} requests in one second"
        );
    }
}

#[test]
fn reciprocal_margin_rate_must_be_finite_and_at_least_one() {
    for rate in ["1e-320", "2"] {
        let mut rig = Rig::defaults();
        for body in [LOGIN, USER, INSTRUMENTS] {
            let request = rig.next_request();
            rig.reply(&request, 200, body);
        }
        let request = rig.next_request();
        rig.reply(&request, 200, &DETAILS.replace("0.2", rate));
        assert!(rig.engine.store().markets().is_empty());
        assert!(!rig.engine.store().loaded().markets);
        assert!(rig.requests.is_empty());
    }
}

#[test]
fn rejected_login_deactivates_clears_timers_and_fails_future_reads() {
    for status in [401, 403] {
        let mut rig = Rig::defaults();
        let login = rig.next_request();
        rig.command(Command::FetchCandles {
            command: 8,
            symbol: "ETHUSD".into(),
            interval: safe_cex_engine::model::Interval::H1,
            from_ms: None,
            to_ms: None,
            limit: None,
        });
        rig.reply(&login, status, r#"{"message":"private-server-detail"}"#);
        assert!(rig.requests.is_empty());
        assert!(rig.timers.is_empty());
        assert!(rig.notices.contains(&Notice::Deactivate));
        assert!(rig.notices.iter().any(|notice| matches!(notice, safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. }) if message == &format!("DXtrade startup failed: DXtrade HTTP {status}"))));
        assert!(rig.result(8).is_err());
        rig.command(Command::FetchCandles {
            command: 9,
            symbol: "ETHUSD".into(),
            interval: safe_cex_engine::model::Interval::H1,
            from_ms: None,
            to_ms: None,
            limit: None,
        });
        assert!(rig.result(9).is_err());
        assert!(rig.requests.is_empty());
        assert!(rig.timers.is_empty());
        assert!(!format!("{:?}", rig.notices).contains("private-server-detail"));
    }
}

#[test]
fn ambiguous_account_startup_reports_error_without_auth_deactivation_or_retries() {
    let mut rig = Rig::defaults();
    let login = rig.next_request();
    rig.reply(&login, 200, LOGIN);
    let user = rig.next_request();
    rig.reply(&user, 200, r#"{"userDetails":[{"login":"example-user","domain":"default","accounts":[{"account":"default:12","accountStatus":"FULL_TRADING"},{"account":"default:2","accountStatus":"FULL_TRADING"}]}]}"#);
    assert!(rig.requests.is_empty());
    assert!(rig.timers.is_empty());
    assert!(!rig.notices.contains(&Notice::Deactivate));
    assert!(rig.notices.iter().any(|notice| matches!(notice, safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. }) if message.starts_with("DXtrade startup failed: Multiple accounts"))));
    assert!(rig.notices.iter().any(|notice| matches!(notice, Notice::StartupFailed { error } if error.message.starts_with("DXtrade startup failed: Multiple accounts"))));
}

#[test]
fn responses_already_in_flight_cannot_restart_failed_authentication() {
    let mut rig = Rig::defaults();
    let login = rig.next_request();
    rig.reply(&login, 200, LOGIN);
    let user = rig.next_request();
    rig.reply(&user, 200, USER);
    let instruments = rig.next_request();
    let details = rig.next_request();
    rig.reply(&instruments, 401, "{}");
    let login = rig.next_request();
    rig.reply(&login, 401, "{}");
    rig.reply(&details, 200, DETAILS);
    assert!(rig.requests.is_empty());
    assert!(rig.timers.is_empty());
    assert!(!rig.engine.store().loaded().markets);
}

fn tick_interval(rig: &mut Rig, interval_ms: f64) {
    rig.engine.handle(
        rig.now,
        Event::PollScaleChanged {
            scale: interval_ms / POLL_MS,
        },
    );
    rig.collect();
}

#[test]
fn incident_slowdown_rearms_poll_without_an_immediate_request() {
    let mut rig = Rig::defaults();
    rig.boot();
    let before = rig.timers.clone();
    for invalid in [f64::NAN, f64::NEG_INFINITY, 0.0, -1.0] {
        tick_interval(&mut rig, invalid);
        assert_eq!(rig.timers, before);
    }
    let updated_at = rig.now;
    tick_interval(&mut rig, 10_000.0);
    assert!(rig.requests.is_empty());
    let request = rig.next_request();
    assert!(request.url.ends_with("/marketdata"));
    assert_eq!(rig.now, updated_at + 10_000.0);
}

#[test]
fn pause_stops_polling_after_inflight_read_and_resume_waits_new_interval() {
    let mut rig = Rig::defaults();
    rig.boot();
    let request = rig.next_request();
    tick_interval(&mut rig, f64::INFINITY);
    rig.reply(&request, 200, QUOTES);
    assert!(rig.timers.is_empty());
    assert!(rig.requests.is_empty());
    rig.now += 5_000.0;
    let resumed_at = rig.now;
    tick_interval(&mut rig, 3_000.0);
    assert!(rig.requests.is_empty());
    let request = rig.next_request();
    assert!(request.url.ends_with("/marketdata"));
    assert_eq!(rig.now, resumed_at + 3_000.0);
}

#[test]
fn configured_infinite_interval_pauses_polls_but_preserves_rate_limited_queries() {
    let mut rig = Rig::new(Options {
        key: "example-user".into(),
        secret: "example-password".into(),
        public_data_only: true,
        tick_interval: f64::INFINITY,
        ..Options::default()
    });
    rig.boot();
    assert!(rig.timers.is_empty());
    assert!(rig.requests.is_empty());
    rig.command(Command::FetchCandles {
        command: 9,
        symbol: "ETHUSD".into(),
        interval: safe_cex_engine::model::Interval::H1,
        from_ms: None,
        to_ms: None,
        limit: None,
    });
    assert!(rig.requests.is_empty());
    let request = rig.next_request();
    assert_eq!(rig.now, 1_000.0);
    rig.reply(&request, 200, r#"{"events":[]}"#);
    assert!(rig.result(9).is_ok());
    assert!(rig.timers.is_empty());
}

#[test]
fn unchanged_polling_controls_never_postpone_an_existing_deadline() {
    let mut rig = Rig::defaults();
    rig.boot();
    let deadline = *rig.timers.values().min_by(|a, b| a.total_cmp(b)).unwrap();
    let original = rig.timers.clone();
    for _ in 0..5 {
        rig.now += 100.0;
        tick_interval(&mut rig, 2_000.0);
        assert_eq!(rig.timers, original);
    }
    let quote = rig.next_request();
    assert!(quote.url.ends_with("/marketdata"));
    assert_eq!(rig.now, deadline);
}
