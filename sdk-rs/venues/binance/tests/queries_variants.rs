use support::Options;
mod support;
use safe_cex_engine::{Command, Event, Query};
use safe_cex_venue_binance::Variant;
use support::Harness;

#[test]
fn coin_m_public_trades_use_dapi_and_documented_base_qty() {
    let mut h = Harness::started(Options {
        variant: Variant::CoinM,
        public_data_only: true,
        ..Options::default()
    });
    h.requests.clear();
    h.event(Event::Command(Command::Query {
        command: 1,
        query: Query::PublicTrades {
            symbol: "BTCUSD_PERP".into(),
            limit: Some(2000),
        },
    }));
    let request = h.take("/dapi/v1/trades");
    assert!(request.url.starts_with("https://dapi.binance.com/"));
    assert!(request.url.contains("limit=1000"));
    assert!(!request.url.contains("signature"));
    h.reply(request.id, 200, "", r#"[{"id":456,"price":"10000","qty":"5","baseQty":"0.05","time":1700000000123,"isBuyerMaker":true}]"#);
    assert_eq!(
        h.done.last().unwrap().1.as_ref().unwrap(),
        r#"[{"id":"456","symbol":"BTCUSD_PERP","price":10000.0,"qty":5.0,"quoteQty":0.05,"timestamp":1700000000123.0,"side":"sell"}]"#
    );
}

#[test]
fn spot_and_inverse_history_report_unsupported_before_auth_without_io() {
    for variant in [Variant::Spot, Variant::CoinM] {
        for authenticated in [false, true] {
            let mut h = Harness::started(Options {
                variant,
                public_data_only: !authenticated,
                key: authenticated.then(|| "key".into()),
                secret: authenticated.then(|| "secret".into()),
                ..Options::default()
            });
            h.requests.clear();
            for query in [
                Query::Fills {
                    symbol: Some("BTCUSDT".into()),
                    order_id: None,
                    from_ms: None,
                    to_ms: None,
                    limit: None,
                },
                Query::OrderHistory {
                    symbol: Some("BTCUSDT".into()),
                    from_ms: None,
                    to_ms: None,
                    limit: None,
                },
                Query::Income {
                    symbol: Some("BTCUSDT".into()),
                    kind: None,
                    from_ms: None,
                    to_ms: None,
                    limit: None,
                },
            ] {
                h.event(Event::Command(Command::Query { command: 1, query }));
                let result = &h.done.last().unwrap().1;
                assert!(
                    result
                        .as_ref()
                        .is_err_and(|error| error.contains("not supported")),
                    "{variant:?}/{authenticated}: {result:?}"
                );
                assert!(h.requests.is_empty());
            }
        }
    }
}

#[test]
fn usd_m_and_pm_history_maps_stp_expiration_as_canceled_without_changing_filled() {
    for variant in [Variant::UsdM, Variant::PortfolioMargin] {
        let mut h = Harness::started(Options {
            variant,
            key: Some("key".into()),
            secret: Some("secret".into()),
            ..Options::default()
        });
        h.requests.clear();
        h.event(Event::Command(Command::Query {
            command: 2,
            query: Query::OrderHistory {
                symbol: Some("BTCUSDT".into()),
                from_ms: None,
                to_ms: None,
                limit: None,
            },
        }));
        let request = h.take("/allOrders");
        assert!(
            request
                .url
                .contains(if variant == Variant::PortfolioMargin {
                    "/papi/v1/um/allOrders?"
                } else {
                    "/fapi/v1/allOrders?"
                })
        );
        h.reply(request.id, 200, "", r#"[{"orderId":1,"symbol":"BTCUSDT","status":"EXPIRED_IN_MATCH","type":"LIMIT","side":"BUY","price":"100","origQty":"2","executedQty":"0","time":1700000000000},{"orderId":2,"symbol":"BTCUSDT","status":"FILLED","type":"LIMIT","side":"SELL","price":"101","origQty":"2","executedQty":"2","time":1700000000001}]"#);
        let result = h.done.last().unwrap().1.as_ref().unwrap();
        assert!(
            result.contains(
                r#""id":"1","symbol":"BTCUSDT","side":"buy","type":"limit","status":"canceled""#
            ),
            "{result}"
        );
        assert!(
            result.contains(
                r#""id":"2","symbol":"BTCUSDT","side":"sell","type":"limit","status":"closed""#
            ),
            "{result}"
        );
    }
}

#[test]
fn first_history_failure_rejects_typed_without_capturing_background_logs() {
    let mut h = Harness::started(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    h.respond("/fapi/v1/exchangeInfo", 200, support::EXCHANGE_INFO);
    // A startup ticker failure remains an ordinary diagnostic with no command owner.
    h.respond(
        "/fapi/v1/ticker/24hr",
        500,
        r#"{"code":-1,"msg":"background quote unavailable"}"#,
    );
    assert!(
        h.logs
            .iter()
            .any(|message| message.contains("background quote unavailable"))
    );
    assert!(h.command_errors.is_empty());
    h.event(Event::Command(Command::Query {
        command: 91,
        query: Query::Fills {
            symbol: Some("BTCUSDT".into()),
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    }));
    let request = h.take("/fapi/v1/userTrades");
    h.reply(
        request.id,
        500,
        "",
        r#"{"code":-1,"msg":"history unavailable"}"#,
    );
    assert!(
        h.result(91)
            .unwrap()
            .as_ref()
            .unwrap_err()
            .contains("history unavailable")
    );
    assert!(h.command_errors.is_empty());
    assert_eq!(h.failures.len(), 1);
    assert_eq!(
        h.failures[0].1.kind,
        Some(safe_cex_engine::ErrorKind::Internal)
    );
    assert_eq!(h.command_notices, [(91, "failed")]);
}
