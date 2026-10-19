use safe_cex_engine::model::{Limits, Market, MinMax, Precision};
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Query, Response};
use safe_cex_venue_bitrue::{Bitrue, Options};
const SYMBOL: &str = "E-BTC-USDT";
const ROW: &str = r#"{"tradeId":1558124009467904992,"price":60000,"qty":3,"amount":180,"contractName":"E-BTC-USDT","side":"BUY","fee":"-0.1","bidId":1558124009467904993,"askId":1558124009467904994,"isBuyer":true,"isMaker":true,"ctime":1700000000123}"#;
fn engine() -> Engine<Bitrue> {
    let mut e = Engine::new(Bitrue::new(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    }));
    e.store_mut().set_markets(vec![Market {
        id: SYMBOL.into(),
        symbol: SYMBOL.into(),
        pretty_symbol: None,
        base: "BTC".into(),
        quote: "USDT".into(),
        active: true,
        precision: Precision {
            amount: 0.001,
            price: 0.1,
        },
        limits: Limits {
            amount: MinMax {
                min: 0.001,
                max: 100.0,
            },
            leverage: MinMax {
                min: 1.0,
                max: 100.0,
            },
            price: None,
        },
        contract_size: Some(0.001),
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: Some(true),
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }]);
    e
}
fn fills() -> Query {
    Query::Fills {
        symbol: Some(SYMBOL.into()),
        order_id: None,
        from_ms: None,
        to_ms: None,
        limit: None,
    }
}
fn query(e: &mut Engine<Bitrue>, query: Query) {
    e.handle(
        1700000000000.0,
        Event::Command(Command::Query { command: 99, query }),
    );
}
fn request(e: &mut Engine<Bitrue>) -> HttpRequest {
    e.drain_actions()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains("/myTrades") => Some(r),
            _ => None,
        })
        .unwrap()
}
fn result(e: &mut Engine<Bitrue>) -> Result<String, String> {
    e.drain_actions()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone {
                command: 99,
                result,
            }) => Some(result),
            _ => None,
        })
        .unwrap()
}
fn response(e: &mut Engine<Bitrue>, r: HttpRequest, body: &str) {
    e.handle(
        1700000000000.0,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
#[test]
fn actual_executions_keep_exact_ids_base_units_and_seconds_without_state_changes() {
    let mut e = engine();
    query(&mut e, fills());
    let r = request(&mut e);
    assert!(
        r.url
            .ends_with("/fapi/v2/myTrades?contractName=E-BTC-USDT&limit=200")
    );
    assert!(r.headers.iter().any(|(k, _)| *k == "X-CH-SIGN"));
    let second = ROW
        .replace("1558124009467904992", "1558124009467904995")
        .replace("1700000000123", "1700000000789");
    response(
        &mut e,
        r,
        &format!(r#"{{"code":"0","data":[{ROW},{second},{ROW}]}}"#),
    );
    let out = result(&mut e).unwrap();
    assert_eq!(out.matches("commissionAsset").count(), 2);
    assert!(out.contains(r#""id":"1558124009467904995""#));
    assert!(out.contains(r#""orderId":"1558124009467904993""#));
    assert!(out.contains(r#""timestamp":1700000000.0"#));
    assert!(out.contains(r#""qty":0.003"#));
    assert!(out.contains(r#""positionSide":"both""#));
    assert!(out.contains(r#""commission":0.1"#));
    assert!(out.contains(r#""maker":true"#));
    assert!(!out.contains("realizedPnl"));
    assert!(out.find("1558124009467904995").unwrap() < out.find("1558124009467904992").unwrap());
    assert!(e.store().orders().is_empty());
    assert!(e.store().positions().is_empty());
}
#[test]
fn filters_native_side_order_identity_symbol_and_exact_millisecond_boundaries() {
    let mut e = engine();
    query(
        &mut e,
        Query::Fills {
            symbol: Some(SYMBOL.into()),
            order_id: Some("1558124009467904994".into()),
            from_ms: Some(1700000000123.0),
            to_ms: Some(1700000000123.0),
            limit: Some(5),
        },
    );
    let r = request(&mut e);
    assert!(
        r.url
            .contains("startTime=1700000000123&endTime=1700000000123")
    );
    assert!(!r.url.contains("orderId="));
    let sell = ROW.replace("\"BUY\"", "\"SELL\"");
    let older = sell
        .replace("1558124009467904992", "1558124009467904995")
        .replace("1700000000123", "1700000000122");
    let other = sell
        .replace("E-BTC-USDT", "E-ETH-USDT")
        .replace("1558124009467904992", "1558124009467904996");
    response(
        &mut e,
        r,
        &format!(r#"{{"code":0,"data":[{ROW},{sell},{older},{other}]}}"#),
    );
    let out = result(&mut e).unwrap();
    assert_eq!(out.matches("commissionAsset").count(), 1);
    assert!(out.contains(r#""orderId":"1558124009467904994""#));
    assert!(out.contains(r#""side":"sell""#));
}
#[test]
fn preserves_raw_base_qty_and_quote_inferred_qty_as_ts_does() {
    let mut e = engine();
    query(&mut e, fills());
    let r = request(&mut e);
    let base = ROW.replace("\"qty\":3", "\"qty\":0.003");
    let inferred = ROW
        .replace("\"qty\":3", "\"qty\":0")
        .replace("1558124009467904992", "1558124009467904995");
    response(
        &mut e,
        r,
        &format!(r#"{{"code":"0","data":[{base},{inferred}]}}"#),
    );
    assert_eq!(result(&mut e).unwrap().matches(r#""qty":0.003"#).count(), 2);
}
#[test]
fn malformed_rows_failed_envelopes_and_transport_failure_reject_partial_results() {
    for body in [
        format!(r#"{{"code":"510","msg":"rate limited","data":[{ROW}]}}"#),
        format!(r#"{{"code":"0","data":[{ROW},{{"tradeId":"bad"}}]}}"#),
        format!(
            r#"{{"code":"0","data":[{}]}}"#,
            ROW.replace("\"BUY\"", "\"other\"")
        ),
        r#"{"code":"0","data":[1,]}"#.into(),
        r#"{"code":"0"}"#.into(),
    ] {
        let mut e = engine();
        query(&mut e, fills());
        let r = request(&mut e);
        response(&mut e, r, &body);
        assert!(result(&mut e).is_err(), "{body}");
    }
    let mut e = engine();
    query(&mut e, fills());
    for _ in 0..4 {
        let r = request(&mut e);
        e.handle(
            1700000000000.0,
            Event::HttpFailed {
                request: r.id,
                error: "network",
            },
        );
    }
    assert!(result(&mut e).is_err());
}
#[test]
fn windows_require_known_symbols_and_unsupported_histories_do_not_fabricate_ledgers() {
    for q in [
        Query::OrderHistory {
            symbol: Some(SYMBOL.into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
        Query::Income {
            symbol: Some(SYMBOL.into()),
            kind: Some("FUNDING_FEE".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
        Query::Fills {
            symbol: Some(SYMBOL.into()),
            order_id: None,
            from_ms: Some(2.0),
            to_ms: Some(1.0),
            limit: None,
        },
        Query::Fills {
            symbol: Some(SYMBOL.into()),
            order_id: None,
            from_ms: Some(f64::NAN),
            to_ms: None,
            limit: None,
        },
    ] {
        let mut e = engine();
        query(&mut e, q);
        assert!(result(&mut e).is_err());
    }
}
#[test]
fn public_only_fills_fail_before_private_requests() {
    let mut e = Engine::new(Bitrue::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    query(&mut e, fills());
    assert!(result(&mut e).is_err());
}

#[test]
fn explicit_signature_rejection_resigns_once_and_discards_untrusted_staged_rows() {
    let mut e = engine();
    query(&mut e, fills());
    let first = request(&mut e);
    response(
        &mut e,
        first,
        &format!(r#"{{"data":[{ROW}],"code":"-1022","msg":"bad signature"}}"#),
    );
    let next = request(&mut e);
    response(&mut e, next, r#"{"code":"0","data":[]}"#);
    assert_eq!(result(&mut e).unwrap(), "[]");
}

#[test]
fn interrupted_stream_read_retries_without_carrying_partial_executions() {
    let mut e = engine();
    query(&mut e, fills());
    let first = request(&mut e);
    e.handle(
        1700000000000.0,
        Event::HttpStart {
            request: first.id,
            status: 200,
            headers: "",
        },
    );
    let prefix = format!(r#"{{"code":"0","data":[{ROW},"#);
    e.handle(
        1700000000000.0,
        Event::HttpChunk {
            request: first.id,
            data: prefix.as_bytes(),
        },
    );
    e.handle(
        1700000000000.0,
        Event::HttpFailed {
            request: first.id,
            error: "connection reset",
        },
    );
    let next = request(&mut e);
    response(&mut e, next, r#"{"code":"0","data":[]}"#);
    assert_eq!(result(&mut e).unwrap(), "[]");
}
