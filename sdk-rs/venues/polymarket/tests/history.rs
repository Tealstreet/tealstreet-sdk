use safe_cex_engine::{Action, Command, Engine, Event, Notice, Query, Response};
use safe_cex_venue_polymarket::{Options, Polymarket};
#[test]
fn unsupported_histories_fail_explicitly_without_network_or_empty_success() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Polymarket::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    for query in [
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
        Query::OrderHistory {
            symbol: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
        Query::Income {
            symbol: None,
            kind: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
        Query::PublicTrades {
            symbol: "anything".into(),
            limit: None,
        },
    ] {
        e.handle(0.0, Event::Command(Command::Query { command: 1, query }));
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(
            matches!(&actions[..],[Action::Notice(Notice::CommandFailed { command: 1, error })] if error.legacy_text().contains("unsupported"))
        );
    }
}
#[test]
fn fixed_one_way_one_x_settings_are_read_only_and_client_lookup_is_not_definitive() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Polymarket::new(Options::default()));
    for command in [
        Command::SetLeverage {
            command: 1,
            symbol: "x".into(),
            leverage: 100.0,
            side: None,
        },
        Command::SetMarginType {
            command: 1,
            symbol: "x".into(),
            isolated: true,
        },
        Command::SetPositionMode {
            command: 1,
            hedged: true,
            symbol: None,
        },
    ] {
        e.handle(0.0, Event::Command(command));
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(
            matches!(&actions[..],[Action::Notice(Notice::CommandDone{result,..})] if result=="[]")
        );
        assert!(!e.store().hedged());
    }
    e.handle(
        0.0,
        Event::Command(Command::FetchLeverage {
            command: 2,
            symbol: "x".into(),
        }),
    );
    assert!(
        matches!(e.drain_actions().next(),Some(Action::Notice(Notice::CommandDone{result,..})) if result==r#"{"buyLeverage":1,"sellLeverage":1}"#)
    );
    e.handle(
        0.0,
        Event::Command(Command::Query {
            command: 3,
            query: Query::OrderByClientId {
                symbol: "x".into(),
                client_order_id: "caller".into(),
            },
        }),
    );
    assert!(
        matches!(e.drain_actions().next(),Some(Action::Notice(Notice::CommandDone{result,..})) if result==r#"{"found":false,"definitive":false}"#)
    );
}

#[test]
fn account_uses_signer_identity_and_never_funder_or_private_io() {
    for (key, expected) in [
        (None, "unknown"),
        (Some("invalid"), "unknown"),
        (
            Some("0000000000000000000000000000000000000000000000000000000000000001"),
            "0x7E5F4552091A69125d5DfCb7b8C2659029395Bdf",
        ),
    ] {
        let mut e = safe_cex_engine::test_support::seeded_engine(Polymarket::new(Options {
            key: key.map(str::to_owned),
            public_data_only: true,
            polymarket: Some(safe_cex_venue_polymarket::PolymarketOptions {
                funder_address: Some("0x1111111111111111111111111111111111111111".into()),
                signer_type: 2,
                ..Default::default()
            }),
        }));
        e.handle(0.0, Event::Command(Command::FetchAccount { command: 7 }));
        let actions: Vec<_> = e.drain_actions().collect();
        let [Action::Notice(Notice::CommandDone { command: 7, result })] = &actions[..] else {
            panic!("unexpected account actions: {actions:?}");
        };
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(result).unwrap(),
            serde_json::json!({"userId": expected, "affiliateId": null})
        );
    }
}

#[test]
fn stp_queries_and_setters_reject_without_network_or_state_mutation() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Polymarket::new(Options::default()));
    for command in [
        Command::FetchStpMode { command: 8 },
        Command::SetStpMode {
            command: 8,
            mode: safe_cex_engine::model::StpMode::ExpireBoth,
        },
    ] {
        e.handle(0.0, Event::Command(command));
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(matches!(
            &actions[..],
            [Action::Notice(Notice::CommandFailed { command: 8, error })]
                if error.legacy_text().contains("unsupported")
        ));
        assert!(!e.store().loaded().balance);
        assert!(!e.store().loaded().positions);
    }
}

fn engine_with_market() -> Engine<Polymarket> {
    use safe_cex_engine::model::{Limits, Market, MinMax, Precision};
    let mut e = safe_cex_engine::test_support::seeded_engine(Polymarket::new(Options::default()));
    e.store_mut().set_markets(vec![Market {
        id: "107505882767731489358349912513945399560393482969656700824895970500493757150417".into(),
        symbol: "election-Yes".into(),
        pretty_symbol: None,
        base: "Yes".into(),
        quote: "pUSD".into(),
        active: true,
        precision: Precision {
            amount: 2.0,
            price: 2.0,
        },
        limits: Limits {
            amount: MinMax { min: 5.0, max: 1e6 },
            leverage: MinMax { min: 1.0, max: 1.0 },
            price: None,
        },
        contract_size: Some(1.0),
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: None,
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }]);
    e
}

fn fetch_book(
    e: &mut Engine<Polymarket>,
    limit: Option<u32>,
    status: u16,
    body: &str,
) -> Result<String, String> {
    e.handle(
        0.0,
        Event::Command(Command::FetchOrderBook {
            command: 9,
            symbol: "election-Yes".into(),
            limit,
        }),
    );
    let actions: Vec<_> = safe_cex_engine::test_support::prepared_actions(e, 0.0)
        .into_iter()
        .filter(|a| !matches!(a, Action::SetTimer { .. } | Action::ClearTimer { .. }))
        .collect();
    let [Action::Http(request)] = &actions[..] else {
        panic!("unexpected book actions: {actions:?}");
    };
    assert_eq!(request.method, safe_cex_engine::Method::Get);
    assert_eq!(
        request.url,
        format!(
            "https://clob.polymarket.com/book?token_id={}",
            e.store().markets()[0].id
        )
    );
    assert!(request.headers.is_empty());
    assert!(request.body.is_none());
    let id = request.id;
    e.handle(
        0.0,
        Event::HttpResponse {
            request: id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
    let actions: Vec<_> = safe_cex_engine::test_support::prepared_actions(e, 0.0)
        .into_iter()
        .filter(|a| !matches!(a, Action::SetTimer { .. } | Action::ClearTimer { .. }))
        .collect();
    match &actions[..] {
        [Action::Notice(Notice::CommandDone { command: 9, result })] => Ok(result.clone()),
        [Action::Notice(Notice::CommandFailed { command: 9, error })] => Err(error.legacy_text()),
        _ => panic!("unexpected book response actions: {actions:?}"),
    }
}

#[test]
fn rest_book_preserves_exact_token_identity_and_sorts_full_depth_without_live_subscription() {
    let mut e = engine_with_market();
    let token = e.store().markets()[0].id.clone();
    let body = format!(
        r#"{{"asset_id":"{token}","bids":[{{"price":"0.2","size":"3"}},{{"price":"0.4","size":"2"}},{{"price":"0.3","size":"4"}},{{"price":"0.5","size":"0"}}],"asks":[{{"price":"0.8","size":"6"}},{{"price":"0.6","size":"5"}},{{"price":"0.7","size":"2"}}]}}"#
    );
    for (limit, bids, asks) in [
        (
            None,
            serde_json::json!([
                {"price":0.4,"amount":2.0,"total":2.0},
                {"price":0.3,"amount":4.0,"total":6.0},
                {"price":0.2,"amount":3.0,"total":9.0}
            ]),
            serde_json::json!([
                {"price":0.6,"amount":5.0,"total":5.0},
                {"price":0.7,"amount":2.0,"total":7.0},
                {"price":0.8,"amount":6.0,"total":13.0}
            ]),
        ),
        (
            Some(1),
            serde_json::json!([
                {"price":0.4,"amount":2.0,"total":2.0}
            ]),
            serde_json::json!([
                {"price":0.6,"amount":5.0,"total":5.0}
            ]),
        ),
    ] {
        let value: serde_json::Value =
            serde_json::from_str(&fetch_book(&mut e, limit, 200, &body).unwrap()).unwrap();
        assert_eq!(value, serde_json::json!({"bids":bids,"asks":asks}));
        assert!(e.book("election-Yes").is_none());
        assert!(!e.store().loaded().tickers);
    }
}

#[test]
fn rest_book_rejects_failed_partial_mismatched_or_malformed_snapshots_even_beyond_limit() {
    let mut e = engine_with_market();
    let token = e.store().markets()[0].id.clone();
    for (status, body) in [
        (503, "unavailable".into()),
        (200, r#"{"error":"unavailable"}"#.into()),
        (200, r#"{"asset_id":"wrong","bids":[],"asks":[]}"#.into()),
        (200, format!(r#"{{"asset_id":"{token}","bids":[]}}"#)),
        (
            200,
            format!(
                r#"{{"asset_id":"{token}","bids":[{{"price":"0.5","size":"1"}},{{"price":"0.1","size":"-1"}}],"asks":[]}}"#
            ),
        ),
        (
            200,
            format!(
                r#"{{"asset_id":"{token}","bids":[{{"price":"0.5","size":"1"}},{{"price":"0.5","size":"2"}}],"asks":[]}}"#
            ),
        ),
        (
            200,
            format!(r#"{{"asset_id":"{token}","bids":[],"asks":[{{"price":"NaN","size":"1"}}]}}"#),
        ),
    ] {
        assert!(
            fetch_book(&mut e, Some(1), status, &body).is_err(),
            "accepted {body}"
        );
        assert!(e.book("election-Yes").is_none());
    }
    assert_eq!(
        fetch_book(
            &mut e,
            None,
            200,
            &format!(r#"{{"asset_id":"{token}","bids":[],"asks":[]}}"#)
        )
        .unwrap(),
        r#"{"bids":[],"asks":[]}"#
    );
}

#[test]
fn invalid_book_requests_reject_before_network() {
    let mut e = engine_with_market();
    for (symbol, limit) in [("election-Yes", Some(0)), ("unknown", None)] {
        e.handle(
            0.0,
            Event::Command(Command::FetchOrderBook {
                command: 10,
                symbol: symbol.into(),
                limit,
            }),
        );
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(matches!(
            &actions[..],
            [Action::Notice(Notice::CommandFailed { command: 10, .. })]
        ));
    }
}

// The profile bit must agree with the real handler, even with valid credentials.
fn history_capability_matches_refusal(public_data_only: bool, axis: &str) {
    let mut advertised = vec![];
    for signer_type in 0..=2 {
        let mut e = safe_cex_engine::test_support::seeded_engine(Polymarket::new(Options {
            public_data_only,
            // Existing synthetic scalar-one fixture; no exchange account.
            key: Some(format!("{:064x}", 1)),
            polymarket: Some(safe_cex_venue_polymarket::PolymarketOptions {
                api_key: Some("fixture".into()),
                api_secret: Some("c2VjcmV0".into()),
                api_passphrase: Some("fixture".into()),
                signer_type,
                funder_address: (signer_type != 0)
                    .then(|| "0x1111111111111111111111111111111111111111".into()),
                market_slugs: None,
            }),
        }));
        let capabilities = e.capabilities();
        let (bit, query, error) = match axis {
            "fills" => (
                capabilities.fill_history,
                Query::Fills {
                    symbol: None,
                    order_id: None,
                    from_ms: None,
                    to_ms: None,
                    limit: None,
                },
                "Polymarket historical fills are unsupported",
            ),
            "orders" => (
                capabilities.order_history,
                Query::OrderHistory {
                    symbol: None,
                    from_ms: None,
                    to_ms: None,
                    limit: None,
                },
                "Polymarket historical orders are unsupported",
            ),
            "income" => (
                capabilities.income_history,
                Query::Income {
                    symbol: None,
                    kind: None,
                    from_ms: None,
                    to_ms: None,
                    limit: None,
                },
                "Polymarket income history is unsupported",
            ),
            _ => panic!("unknown fixture axis"),
        };
        assert_eq!(capabilities.account, safe_cex_engine::AccountLookup::Query);
        assert!(capabilities.order_book_query);
        assert_eq!(capabilities.cancel_all, safe_cex_engine::CancelAll::Native);
        e.handle(
            0.,
            Event::Command(Command::Query {
                command: 971,
                query,
            }),
        );
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert!(
            matches!(&actions[..], [Action::Notice(Notice::CommandFailed { command: 971, error: actual })] if actual.legacy_text() == error),
            "{axis} public={public_data_only} signer={signer_type}: {actions:?}"
        );
        advertised.push(bit);
    }
    assert!(
        advertised.iter().all(|bit| !bit),
        "{axis} public={public_data_only} must not advertise unconditional native refusal: {advertised:?}"
    );
}
#[test]
fn capability_public_fills_matches_native_refusal() {
    history_capability_matches_refusal(true, "fills");
}
#[test]
fn capability_private_fills_matches_native_refusal() {
    history_capability_matches_refusal(false, "fills");
}
#[test]
fn capability_public_orders_matches_native_refusal() {
    history_capability_matches_refusal(true, "orders");
}
#[test]
fn capability_private_orders_matches_native_refusal() {
    history_capability_matches_refusal(false, "orders");
}
#[test]
fn capability_public_income_matches_native_refusal() {
    history_capability_matches_refusal(true, "income");
}
#[test]
fn capability_private_income_matches_native_refusal() {
    history_capability_matches_refusal(false, "income");
}
