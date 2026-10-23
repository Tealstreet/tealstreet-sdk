mod spot_support;
use safe_cex_engine::{Action, Command, Event, Notice, Query, model::PlaceOrderOpts};
use spot_support::*;
fn opts(kind: &str, price: f64) -> PlaceOrderOpts {
    serde_json::from_str(&format!(
        r#"{{"symbol":"XBT/USD","type":"{kind}","side":"buy","amount":1,"price":{price}}}"#
    ))
    .unwrap()
}
fn done(
    e: &mut safe_cex_engine::Engine<safe_cex_venue_kraken::KrakenSpot>,
    command: u32,
) -> Result<String, String> {
    e.drain_plain()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone { command: c, result }) if c == command => {
                Some(Ok(result))
            }
            Action::Notice(Notice::CommandFailed { command: c, error }) if c == command => {
                Some(Err(error.legacy_text()))
            }
            _ => None,
        })
        .unwrap()
}
#[test]
fn every_spot_bracket_leg_receipts_its_exact_native_cl_ord_id_before_dispatch() {
    let (mut e, _, _) = ready(false);
    let mut order = opts("limit", 99.);
    order.stop_loss = Some(95.);
    order.take_profit = Some(105.);
    e.handle(
        NOW,
        Event::Command(Command::PlaceOrders {
            command: 91,
            orders: vec![order],
        }),
    );
    let mut identities = std::collections::HashSet::new();
    for index in 0..3 {
        let actions = e.drain_plain().collect::<Vec<_>>();
        let (at, receipts) = actions
            .iter()
            .enumerate()
            .find_map(|(i, action)| match action {
                Action::Notice(Notice::CommandProgress {
                    command: 91,
                    receipts,
                }) => Some((i, receipts)),
                _ => None,
            })
            .unwrap();
        let (write_at, request) = actions
            .iter()
            .enumerate()
            .find_map(|(i, action)| match action {
                Action::Http(request) if request.url.ends_with("AddOrder") => Some((i, request)),
                _ => None,
            })
            .unwrap();
        assert!(at < write_at);
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].symbol, "XBT/USD");
        assert_eq!(
            receipts[0].kind,
            safe_cex_engine::OrderIdentityKind::ClientOrderId
        );
        let actual = request
            .body
            .as_deref()
            .unwrap()
            .split('&')
            .find_map(|part| part.strip_prefix("cl_ord_id="))
            .unwrap();
        assert_eq!(receipts[0].identity, actual);
        assert!(identities.insert(receipts[0].identity.clone()));
        reply(
            &mut e,
            request,
            &format!(r#"{{"error":[],"result":{{"txid":["accepted-{index}"]}}}}"#),
        );
    }
}
#[test]
fn native_place_types_exact_client_id_and_trailing_distance() {
    for (kind, raw, expected) in [
        ("limit", "limit", "price=98"),
        ("market", "market", "volume=1"),
        ("stop_market", "stop-loss", "price=98"),
        ("take_profit_market", "take-profit", "price=98"),
        ("trailing_stop_market", "trailing-stop", "price=%2B2"),
    ] {
        let (mut e, _, _) = ready(false);
        e.handle(
            NOW,
            Event::Command(Command::PlaceOrders {
                command: 1,
                orders: vec![opts(kind, 98.)],
            }),
        );
        let r = requests(&mut e).pop().unwrap();
        let body = r.body.as_ref().unwrap();
        assert!(body.contains("pair=XXBTZUSD"));
        assert!(body.contains(&format!("ordertype={raw}")));
        assert!(body.contains(expected));
        assert!(!body.contains("userref") && !body.contains("reduce_only"));
        let id = body
            .split('&')
            .find_map(|p| p.strip_prefix("cl_ord_id="))
            .unwrap();
        assert_eq!(id.len(), 32);
        assert!(
            id.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        );
        reply(
            &mut e,
            &r,
            r#"{"error":[],"result":{"txid":["KRAKEN-TXID"]}}"#,
        );
        assert_eq!(done(&mut e, 1).unwrap(), r#"["KRAKEN-TXID"]"#);
    }
}
#[test]
fn caller_ids_bad_brackets_and_zero_trailing_fail_before_any_write() {
    let (mut e, _, _) = ready(false);
    for (mut bad, id) in [
        (opts("limit", 99.), Some("")),
        (opts("limit", 99.), Some("caller")),
        (opts("trailing_stop_market", 100.), None),
    ] {
        bad.client_order_id = id.map(str::to_owned);
        e.handle(
            NOW,
            Event::Command(Command::PlaceOrders {
                command: 2,
                orders: vec![opts("limit", 99.), bad],
            }),
        );
        let actions = e.drain_plain().collect::<Vec<_>>();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 2, .. })))
        );
    }
    let mut bad = opts("limit", 99.);
    bad.stop_loss = Some(-1.);
    e.handle(
        NOW,
        Event::Command(Command::PlaceOrders {
            command: 3,
            orders: vec![bad],
        }),
    );
    assert!(done(&mut e, 3).is_err());
}
#[test]
fn native_amend_preserves_txid_and_uses_total_amount_postonly_price() {
    let (mut e, _, _) = ready(false);
    e.handle(
        NOW,
        Event::Command(Command::AmendOrder {
            command: 4,
            id: "O-100".into(),
            amount: Some(3.),
            price: Some(98.5),
        }),
    );
    let r = requests(&mut e).pop().unwrap();
    assert!(r.url.ends_with("/AmendOrder"));
    let body = r.body.as_ref().unwrap();
    assert!(
        body.contains("txid=O-100")
            && body.contains("order_qty=3")
            && body.contains("limit_price=98.5")
            && body.contains("post_only=true")
    );
    assert!(!body.contains("volume=") && !body.contains("trigger_price"));
    reply(
        &mut e,
        &r,
        r#"{"error":[],"result":{"amend_id":"AMEND-OTHER-ID"}}"#,
    );
    assert_eq!(done(&mut e, 4).unwrap(), r#"["O-100"]"#);
}
#[test]
fn trigger_amend_uses_trigger_price_and_unsafe_types_reject_without_cancel() {
    for raw in [
        "stop-loss",
        "take-profit",
        "trailing-stop",
        "stop-loss-limit",
        "take-profit-limit",
        "market",
    ] {
        let (mut e, _, private) = ready(false);
        let s = private.unwrap();
        message(
            &mut e,
            s,
            &format!(
                r#"{{"channel":"executions","type":"snapshot","data":[{{"order_id":"TRIGGER","symbol":"BTC/USD","side":"sell","order_type":"{raw}","order_qty":2,"cum_qty":0,"trigger_price":99}}]}}"#
            ),
        );
        e.drain_plain().for_each(drop);
        e.handle(
            NOW,
            Event::Command(Command::AmendOrder {
                command: 5,
                id: "TRIGGER".into(),
                price: Some(97.),
                amount: None,
            }),
        );
        let actions = e.drain_plain().collect::<Vec<_>>();
        if matches!(raw, "stop-loss" | "take-profit") {
            let r = actions
                .into_iter()
                .find_map(|a| match a {
                    Action::Http(r) => Some(r),
                    _ => None,
                })
                .unwrap();
            assert!(r.url.ends_with("AmendOrder"));
            assert!(r.body.unwrap().contains("trigger_price=97"));
        } else {
            assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
            assert!(
                actions
                    .iter()
                    .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
            );
        }
    }
}
#[test]
fn queues_serialize_brackets_and_independent_commands() {
    let (mut e, _, _) = ready(false);
    let mut o = opts("limit", 99.);
    o.stop_loss = Some(95.);
    o.take_profit = Some(105.);
    e.handle(
        NOW,
        Event::Command(Command::PlaceOrders {
            command: 6,
            orders: vec![o],
        }),
    );
    let first = requests(&mut e).pop().unwrap();
    e.handle(
        NOW,
        Event::Command(Command::CancelOrders {
            command: 7,
            ids: vec!["O-100".into()],
        }),
    );
    assert!(requests(&mut e).is_empty());
    reply(
        &mut e,
        &first,
        r#"{"error":[],"result":{"txid":["ENTRY"]}}"#,
    );
    let stop = requests(&mut e).pop().unwrap();
    assert!(stop.body.as_ref().unwrap().contains("ordertype=stop-loss"));
    reply(&mut e, &stop, r#"{"error":[],"result":{"txid":["STOP"]}}"#);
    let tp = requests(&mut e).pop().unwrap();
    assert!(tp.body.as_ref().unwrap().contains("ordertype=take-profit"));
    reply(&mut e, &tp, r#"{"error":[],"result":{"txid":["TP"]}}"#);
    let actions = e.drain_plain().collect::<Vec<_>>();
    assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::CommandDone{command:6,result:ids})if ids.contains("ENTRY")&&ids.contains("STOP")&&ids.contains("TP"))));
    assert!(
        actions
            .iter()
            .any(|a| matches!(a,Action::Http(r)if r.url.ends_with("CancelOrder")))
    );
}
#[test]
fn uncertain_write_is_never_replayed_but_signature_rejection_resigns_once() {
    let (mut e, _, _) = ready(false);
    e.handle(
        NOW,
        Event::Command(Command::PlaceOrders {
            command: 8,
            orders: vec![opts("limit", 99.)],
        }),
    );
    let first = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &first,
        r#"{"error":["EAPI:Invalid signature"],"result":{}}"#,
    );
    let second = requests(&mut e).pop().unwrap();
    assert_ne!(first.body, second.body);
    let id = |r: &safe_cex_engine::HttpRequest| {
        r.body
            .as_ref()
            .unwrap()
            .split('&')
            .find(|v| v.starts_with("cl_ord_id="))
            .unwrap()
            .to_owned()
    };
    assert_eq!(id(&first), id(&second));
    e.handle(
        NOW,
        Event::HttpFailed {
            request: second.id,
            error: "timeout after send",
        },
    );
    let actions = e.drain_plain().collect::<Vec<_>>();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::SetTimer { .. }))
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 8, .. })))
    );
}
#[test]
fn failed_cancel_stops_batch_and_does_not_remove_store_orders() {
    let (mut e, _, _) = ready(false);
    e.handle(
        NOW,
        Event::Command(Command::CancelOrders {
            command: 9,
            ids: vec!["O-100".into(), "O-200".into()],
        }),
    );
    let r = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &r,
        r#"{"error":["EOrder:Unknown order"],"result":{}}"#,
    );
    let actions = e.drain_plain().collect::<Vec<_>>();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(e.store().order("O-100").is_some());
}
#[test]
fn inherited_history_lookup_and_settings_settle_explicitly() {
    let (mut e, _, _) = ready(false);
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
    ] {
        e.handle(NOW, Event::Command(Command::Query { command: 10, query }));
        assert_eq!(done(&mut e, 10).unwrap(), "[]");
    }
    e.handle(
        NOW,
        Event::Command(Command::Query {
            command: 11,
            query: Query::OrderByClientId {
                symbol: "XBT/USD".into(),
                client_order_id: "internal".into(),
            },
        }),
    );
    assert_eq!(
        done(&mut e, 11).unwrap(),
        r#"{"found":false,"definitive":false}"#
    );
    e.handle(
        NOW,
        Event::Command(Command::SetLeverage {
            command: 12,
            symbol: "XBT/USD".into(),
            leverage: 2.,
            side: None,
        }),
    );
    assert!(done(&mut e, 12).is_err());
}

#[test]
fn queued_amend_rechecks_reconciled_native_kind_before_write() {
    let (mut e, _, private) = ready(false);
    e.handle(
        NOW,
        Event::Command(Command::PlaceOrders {
            command: 40,
            orders: vec![opts("limit", 99.)],
        }),
    );
    let first = requests(&mut e).pop().unwrap();
    e.handle(
        NOW,
        Event::Command(Command::AmendOrder {
            command: 41,
            id: "O-100".into(),
            price: Some(98.),
            amount: None,
        }),
    );
    assert!(requests(&mut e).is_empty());
    message(
        &mut e,
        private.unwrap(),
        r#"{"channel":"executions","type":"update","data":[{"order_id":"O-100","symbol":"BTC/USD","side":"buy","order_type":"stop-loss-limit","order_qty":2,"cum_qty":0.5,"exec_type":"amended","limit_price":98,"timestamp":"2023-11-14T22:13:21Z"}]}"#,
    );
    e.drain_plain().for_each(drop);
    reply(
        &mut e,
        &first,
        r#"{"error":[],"result":{"txid":["ENTRY"]}}"#,
    );
    let actions = e.drain_plain().collect::<Vec<_>>();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 41, .. })))
    );
}

#[test]
fn trailing_quantizes_the_distance_from_unrounded_chart_price() {
    let (mut e, s, _) = ready(false);
    message(
        &mut e,
        s,
        r#"{"channel":"ticker","type":"update","data":[{"symbol":"BTC/USD","last":100.04}]}"#,
    );
    e.drain_plain().for_each(drop);
    e.handle(
        NOW,
        Event::Command(Command::PlaceOrders {
            command: 42,
            orders: vec![opts("trailing_stop_market", 99.96)],
        }),
    );
    let r = requests(&mut e).pop().unwrap();
    assert!(r.body.unwrap().contains("price=%2B0.1"));
}

#[test]
fn zero_disabled_brackets_match_ts_without_a_protective_write() {
    let (mut e, _, _) = ready(false);
    let mut o = opts("limit", 99.);
    o.stop_loss = Some(0.);
    o.take_profit = Some(0.);
    e.handle(
        NOW,
        Event::Command(Command::PlaceOrders {
            command: 43,
            orders: vec![o],
        }),
    );
    let r = requests(&mut e).pop().unwrap();
    reply(&mut e, &r, r#"{"error":[],"result":{"txid":["ENTRY"]}}"#);
    let actions = e.drain_plain().collect::<Vec<_>>();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone {
            command: 43,
            result: _
        })
    )));
}
#[test]
fn trade_permission_denied_fails_the_order_without_deactivating_a_read_key() {
    let (mut e, _, _) = ready(false);
    e.handle(
        NOW,
        Event::Command(Command::PlaceOrders {
            command: 31,
            orders: vec![opts("limit", 98.)],
        }),
    );
    let r = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &r,
        r#"{"error":["EGeneral:Permission denied"],"result":{}}"#,
    );
    let actions = e.drain_plain().collect::<Vec<_>>();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::Deactivate)))
    );
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 31, error: m }) if m.legacy_text().contains("Permission denied"))));
}
#[test]
fn market_entry_with_tpsl_names_the_unsupported_legs() {
    let (mut e, _, _) = ready(false);
    let mut o = opts("market", 99.);
    o.take_profit = Some(110.);
    e.handle(
        NOW,
        Event::Command(Command::PlaceOrders {
            command: 44,
            orders: vec![opts("limit", 99.), o],
        }),
    );
    let actions = e.drain_plain().collect::<Vec<_>>();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 44, error: m }) if m.legacy_text().contains("take-profit/stop-loss legs on a market entry"))));
}
#[test]
fn market_entry_with_zero_brackets_places_only_the_entry() {
    let (mut e, _, _) = ready(false);
    let mut o = opts("market", 99.);
    o.stop_loss = Some(0.);
    e.handle(
        NOW,
        Event::Command(Command::PlaceOrders {
            command: 45,
            orders: vec![o],
        }),
    );
    let r = requests(&mut e).pop().unwrap();
    reply(&mut e, &r, r#"{"error":[],"result":{"txid":["ENTRY"]}}"#);
    let actions = e.drain_plain().collect::<Vec<_>>();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 45, result: ids }) if ids == r#"["ENTRY"]"#)));
}

#[test]
fn trigger_price_is_refused_unless_it_repeats_the_stop_price() {
    for (kind, trigger) in [("stop_market", 98.), ("limit", 99.)] {
        let (mut e, _, _) = ready(false);
        let mut o = opts(kind, 99.);
        o.trigger_price = Some(trigger);
        e.handle(
            NOW,
            Event::Command(Command::PlaceOrders {
                command: 46,
                orders: vec![o],
            }),
        );
        let actions = e.drain_plain().collect::<Vec<_>>();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 46, error: m }) if m.legacy_text().contains("triggerPrice"))));
    }
    let (mut e, _, _) = ready(false);
    let mut o = opts("stop_market", 99.);
    o.trigger_price = Some(99.);
    e.handle(
        NOW,
        Event::Command(Command::PlaceOrders {
            command: 47,
            orders: vec![o],
        }),
    );
    let r = requests(&mut e).pop().unwrap();
    assert!(r.body.as_ref().unwrap().contains("ordertype=stop-loss"));
}

#[test]
fn write_failures_are_classified_without_changing_their_text() {
    use safe_cex_engine::{ErrorKind, Outcome, Response};
    let rejected = |kind| (Some(kind), Some(Outcome::Rejected));
    let unknown = (None, Some(Outcome::Unknown));
    for (status, body, classified, text) in [
        (
            200,
            r#"{"error":["EOrder:Insufficient funds"]}"#,
            rejected(ErrorKind::Rejected),
            "Kraken Spot: EOrder:Insufficient funds",
        ),
        (
            200,
            r#"{"error":["EAPI:Rate limit exceeded"]}"#,
            rejected(ErrorKind::RateLimited),
            "Kraken Spot: EAPI:Rate limit exceeded",
        ),
        (
            200,
            r#"{"error":["EService:Unavailable"]}"#,
            unknown,
            "Kraken Spot: EService:Unavailable",
        ),
        (
            200,
            r#"{"error":["EFuture:Something"]}"#,
            (None, None),
            "Kraken Spot: EFuture:Something",
        ),
        (
            200,
            r#"{"error":[],"result":{}}"#,
            unknown,
            "Kraken Spot missing placement txids",
        ),
        (503, "<html>", unknown, "Kraken Spot invalid response"),
        (
            429,
            r#"{"error":[]}"#,
            rejected(ErrorKind::RateLimited),
            "Kraken Spot HTTP 429",
        ),
    ] {
        let (mut e, _, _) = ready(false);
        e.handle(
            NOW,
            Event::Command(Command::PlaceOrders {
                command: 8,
                orders: vec![opts("limit", 99.)],
            }),
        );
        let r = requests(&mut e).pop().unwrap();
        e.handle(
            NOW,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
        let [error] = failures(&mut e).try_into().unwrap();
        assert_eq!(
            ((error.kind, error.outcome), error.legacy_text().as_str()),
            (classified, text),
            "{status} {body}"
        );
    }
    // Transport loss on a write is Unknown and never retried.
    let (mut e, _, _) = ready(false);
    e.handle(
        NOW,
        Event::Command(Command::PlaceOrders {
            command: 8,
            orders: vec![opts("limit", 99.)],
        }),
    );
    let r = requests(&mut e).pop().unwrap();
    e.handle(
        NOW,
        Event::HttpFailed {
            request: r.id,
            error: "reset",
        },
    );
    let [error] = failures(&mut e).try_into().unwrap();
    assert_eq!(
        (error.kind, error.outcome),
        (Some(ErrorKind::Network), Some(Outcome::Unknown))
    );
}

mod owned_nonpageable {
    use super::*;
    use safe_cex_engine::{Action, ErrorKind, Event, Level, Notice, Outcome, Value, log::code};

    fn queries() -> [Query; 3] {
        [
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
        ]
    }
    fn incomplete(output: &[Action], command: u32) {
        let [
            Action::Notice(Notice::Event(marker)),
            Action::Notice(Notice::CommandDone {
                command: owner,
                result,
            }),
        ] = output
        else {
            panic!("one coverage marker before one result, and no I/O: {output:?}");
        };
        assert_eq!(*owner, command);
        assert_eq!(result, "[]");
        assert_eq!(marker.code, code::HISTORY_INCOMPLETE);
        assert_eq!(marker.level, Level::Warn);
        assert!(marker.message.contains("INCOMPLETE"));
        assert!(marker.message.contains("Kraken Spot"));
        assert!(
            matches!(marker.fields.as_slice(), [("command", Value::Num(owner))] if *owner == f64::from(command))
        );
    }

    #[test]
    fn history_has_owned_incomplete_coverage_even_with_cached_orders_and_error_log_level() {
        let (mut e, _, _) = ready(false);

        e.drain_actions().for_each(drop);
        let before = e.store().orders().to_vec();
        assert!(
            !before.is_empty(),
            "live orders do not establish history completeness"
        );
        let balance = e.store().balance().clone();
        e.set_log_level(Level::Error);
        for (index, query) in queries().into_iter().enumerate() {
            let command = 100 + index as u32;
            e.handle(5000., Event::Command(Command::Query { command, query }));
            incomplete(&e.drain_actions().collect::<Vec<_>>(), command);
            assert_eq!(e.store().orders(), before);
            assert_eq!(e.store().balance(), &balance);
        }
    }

    #[test]
    fn public_trade_refusal_is_typed_not_sent_with_original_text_and_no_coverage() {
        let (mut e, _, _) = ready(false);
        e.drain_actions().for_each(drop);
        e.handle(
            5000.,
            Event::Command(Command::Query {
                command: 200,
                query: Query::PublicTrades {
                    symbol: "XBT/USD".into(),
                    limit: Some(10),
                },
            }),
        );
        let output: Vec<_> = e.drain_actions().collect();
        let [
            Action::Notice(Notice::CommandFailed {
                command: 200,
                error,
            }),
        ] = output.as_slice()
        else {
            panic!("one refusal, no I/O or unrelated coverage: {output:?}");
        };
        assert_eq!(
            error.message,
            "Kraken Spot public trades are not implemented"
        );
        assert_eq!(error.kind, Some(ErrorKind::Unsupported));
        assert_eq!(error.outcome, Some(Outcome::NotSent));
    }

    #[test]
    fn lookup_miss_remains_nondefinitive_without_a_history_marker_or_io() {
        let (mut e, _, _) = ready(false);
        e.drain_actions().for_each(drop);
        e.handle(
            5000.,
            Event::Command(Command::Query {
                command: 300,
                query: Query::OrderByClientId {
                    symbol: "XBT/USD".into(),
                    client_order_id: "0x11111111111111111111111111111111".into(),
                },
            }),
        );
        let output: Vec<_> = e.drain_actions().collect();
        let [
            Action::Notice(Notice::CommandDone {
                command: 300,
                result,
            }),
        ] = output.as_slice()
        else {
            panic!("one explicit nondefinitive lookup, no history marker or I/O: {output:?}");
        };
        assert_eq!(result, r#"{"found":false,"definitive":false}"#);
    }
}
