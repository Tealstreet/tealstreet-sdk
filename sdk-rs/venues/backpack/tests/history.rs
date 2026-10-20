mod support;
use safe_cex_engine::{Action, Command, Event, Notice, Query};
use support::*;
fn ask(e: &mut Driver, query: Query) {
    e.handle(2000.0, Event::Command(Command::Query { command: 1, query }));
}
fn result(e: &mut Driver) -> Result<String, String> {
    e.drain_actions()
        .find_map(|a| {
            if let Action::Notice(Notice::CommandDone { result, .. }) = a {
                Some(result)
            } else {
                None
            }
        })
        .unwrap()
        .map_err(|error| error.message)
}
#[test]
fn fills_stream_into_execution_shape_with_millisecond_filters_and_seconds_output() {
    let (mut e, _) = ready();
    ask(
        &mut e,
        Query::Fills {
            symbol: Some(SYMBOL.into()),
            order_id: Some("123".into()),
            from_ms: Some(1765805280000.0),
            to_ms: Some(1765805340000.0),
            limit: Some(100),
        },
    );
    let r = requests(&mut e).pop().unwrap();
    assert!(r.url.contains("from=1765805280000"));
    assert!(r.url.contains("sortDirection=Desc"));
    reply(
        &mut e,
        &r,
        200,
        r#"[{"orderId":"123","symbol":"SOL_USDC_PERP","side":"Bid","price":"141","quantity":"2","fee":"-0.1","feeSymbol":"USDC","tradeId":999,"timestamp":"2025-12-15T13:28:00.123Z","isMaker":false}]"#,
    );
    let json = result(&mut e).unwrap();
    assert!(json.contains("\"timestamp\":1765805280.0"));
    assert!(json.contains("\"commission\":0.1"));
    assert!(json.contains("\"qty\":2.0"));
    assert!(e.store().orders().is_empty());
}
#[test]
fn order_history_accepts_iso_created_at_and_computes_average_execution_price() {
    let (mut e, _) = ready();
    ask(
        &mut e,
        Query::OrderHistory {
            symbol: Some(SYMBOL.into()),
            from_ms: None,
            to_ms: None,
            limit: Some(100),
        },
    );
    let r = requests(&mut e).pop().unwrap();
    let order = ORDER
        .replace(
            "\"createdAt\":1000",
            "\"createdAt\":\"2025-12-15T13:28:00Z\"",
        )
        .replace(
            "\"executedQuantity\":\"0\"",
            "\"executedQuantity\":\"2\",\"executedQuoteQuantity\":\"280\"",
        );
    reply(&mut e, &r, 200, &format!("[{order}]"));
    let json = result(&mut e).unwrap();
    assert!(json.contains("\"createdTime\":1765805280000.0"));
    assert!(json.contains("\"averagePrice\":140.0"));
}
#[test]
fn funding_income_preserves_payment_sign_and_unsupported_ledgers_are_explicit() {
    let (mut e, _) = ready();
    ask(
        &mut e,
        Query::Income {
            symbol: Some(SYMBOL.into()),
            kind: Some("FUNDING_FEE".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    let r = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &r,
        200,
        r#"[{"symbol":"SOL_USDC_PERP","quantity":"-1.25","intervalEndTimestamp":"2025-12-15T13:00:00Z","fundingRate":"0.0001"}]"#,
    );
    let json = result(&mut e).unwrap();
    assert!(json.contains("\"income\":-1.25"));
    assert!(json.contains("\"category\":\"funding\""));
    ask(
        &mut e,
        Query::Income {
            symbol: None,
            kind: Some("REALIZED_PNL".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    assert!(
        result(&mut e)
            .unwrap_err()
            .contains("funding payments only")
    );
}
#[test]
fn public_trades_are_unsigned_and_buyer_maker_means_aggressive_sell() {
    let (mut e, _) = ready();
    ask(
        &mut e,
        Query::PublicTrades {
            symbol: SYMBOL.into(),
            limit: Some(1),
        },
    );
    let r = requests(&mut e).pop().unwrap();
    assert!(!r.headers.iter().any(|(k, _)| *k == "X-Signature"));
    reply(
        &mut e,
        &r,
        200,
        r#"[{"id":87992941,"isBuyerMaker":true,"price":"104.12","quantity":"0.1","quoteQuantity":"10.412","timestamp":1792425943313}]"#,
    );
    assert!(result(&mut e).unwrap().contains("\"side\":\"sell\""));
}
#[test]
fn malformed_lists_and_venue_failures_never_claim_empty_success() {
    let (mut e, _) = ready();
    ask(
        &mut e,
        Query::PublicTrades {
            symbol: SYMBOL.into(),
            limit: Some(1),
        },
    );
    let r = requests(&mut e).pop().unwrap();
    reply(&mut e, &r, 200, r#"{"error":{"message":"not an array"}}"#);
    assert!(result(&mut e).is_err());
    ask(
        &mut e,
        Query::OrderHistory {
            symbol: Some(SYMBOL.into()),
            from_ms: None,
            to_ms: None,
            limit: Some(100),
        },
    );
    let r = requests(&mut e).pop().unwrap();
    reply(&mut e, &r, 503, "unavailable");
    assert!(result(&mut e).is_err());
}

#[test]
fn account_wide_fills_page_past_spot_rows_instead_of_claiming_no_perp_fills() {
    let (mut e, _) = ready();
    ask(
        &mut e,
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: Some(1),
        },
    );
    let r = requests(&mut e).pop().unwrap();
    let fill = r#"{"orderId":"123","symbol":"SOL_USDC","side":"Bid","price":"141","quantity":"2","fee":"0.1","feeSymbol":"USDC","tradeId":999,"timestamp":"2025-12-15T13:28:00.123+00:00","isMaker":false}"#;
    reply(&mut e, &r, 200, &format!("[{fill}]"));
    let next = requests(&mut e).pop().unwrap();
    assert!(next.url.contains("offset=1"));
    reply(
        &mut e,
        &next,
        200,
        &format!("[{}]", fill.replace("SOL_USDC", "SOL_USDC_PERP")),
    );
    assert!(result(&mut e).unwrap().contains("SOL_USDC_PERP"));
}
#[test]
fn fractional_fill_boundaries_are_broadened_on_wire_and_kept_exact_locally() {
    let (mut e, _) = ready();
    ask(
        &mut e,
        Query::Fills {
            symbol: Some(SYMBOL.into()),
            order_id: None,
            from_ms: Some(1765805280000.1),
            to_ms: Some(1765805340000.1),
            limit: Some(100),
        },
    );
    let r = requests(&mut e).pop().unwrap();
    assert!(r.url.contains("from=1765805280000"));
    assert!(r.url.contains("to=1765805340001"));
}

#[test]
fn invalid_order_side_fails_and_a_repeated_full_page_is_not_silently_truncated() {
    let (mut e, _) = ready();
    ask(
        &mut e,
        Query::OrderHistory {
            symbol: Some(SYMBOL.into()),
            from_ms: None,
            to_ms: None,
            limit: Some(100),
        },
    );
    let r = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &r,
        200,
        &format!("[{}]", ORDER.replace("Bid", "Unknown")),
    );
    assert!(result(&mut e).is_err());
    ask(
        &mut e,
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: Some(1),
        },
    );
    let r = requests(&mut e).pop().unwrap();
    let spot = r#"[{"orderId":"123","symbol":"SOL_USDC","side":"Bid","price":"141","quantity":"2","fee":"0.1","feeSymbol":"USDC","tradeId":999,"timestamp":"2025-12-15T13:28:00Z"}]"#;
    reply(&mut e, &r, 200, spot);
    let next = requests(&mut e).pop().unwrap();
    reply(&mut e, &next, 200, spot);
    assert!(result(&mut e).unwrap_err().contains("incomplete"));
}
