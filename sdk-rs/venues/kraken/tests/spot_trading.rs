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
            .any(|a| matches!(a, Action::Http(r) if !r.url.ends_with("OpenOrders")))
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("OpenOrders")))
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
            unknown,
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

#[test]
fn beta_partial_placement_retains_ids_and_reconciles_uncertainty() {
    for body in [r#"{"error":["EOrder:Insufficient funds"]}"#, "<html>"] {
        let (mut e, _, _) = ready(false);
        e.handle(
            NOW,
            Event::Command(Command::PlaceOrders {
                command: 91,
                orders: vec![opts("limit", 99.), opts("limit", 99.), opts("limit", 99.)],
            }),
        );
        let first = requests(&mut e).pop().unwrap();
        reply(
            &mut e,
            &first,
            r#"{"error":[],"result":{"txid":["accepted"]}}"#,
        );
        let second = requests(&mut e).pop().unwrap();
        e.handle(
            NOW,
            Event::HttpResponse {
                request: second.id,
                response: safe_cex_engine::Response {
                    status: if body == "<html>" { 503 } else { 200 },
                    headers: "",
                    body,
                },
            },
        );
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
        let error = actions
            .iter()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
                _ => None,
            })
            .unwrap();
        assert_eq!(error.partial.as_ref().unwrap().order_ids, ["accepted"]);
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("OpenOrders")))
        );
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("AddOrder")))
        );
    }
}

#[test]
fn beta_one_http_auth_failure_does_not_deactivate_spot() {
    use safe_cex_engine::{ErrorKind, Response};
    for (status, kind) in [
        (401, ErrorKind::AuthInvalid),
        (403, ErrorKind::AuthPermission),
    ] {
        let (mut e, _, _) = ready(false);
        e.handle(
            NOW,
            Event::Command(Command::PlaceOrders {
                command: 91,
                orders: vec![opts("limit", 99.)],
            }),
        );
        let request = requests(&mut e).pop().unwrap();
        e.handle(
            NOW,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status,
                    headers: "",
                    body: "<html>",
                },
            },
        );
        let actions = e.drain_plain().collect::<Vec<_>>();
        let error = actions
            .iter()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
                _ => None,
            })
            .expect("typed auth failure");
        assert_eq!(error.kind, Some(kind));
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::Deactivate)))
        );
        let reconcile = actions
            .iter()
            .find_map(|a| match a {
                Action::Http(r) if r.url.ends_with("OpenOrders") => Some(r.clone()),
                _ => None,
            })
            .expect("the ambiguous write is reconciled before another placement");
        reply(&mut e, &reconcile, r#"{"error":[],"result":{"open":{}}}"#);
        e.handle(
            NOW,
            Event::Command(Command::PlaceOrders {
                command: 92,
                orders: vec![opts("limit", 99.)],
            }),
        );
        let request = requests(&mut e)
            .into_iter()
            .find(|r| r.url.ends_with("AddOrder"))
            .expect("venue still active");
        reply(
            &mut e,
            &request,
            r#"{"error":[],"result":{"txid":["after-auth"]}}"#,
        );
        assert_eq!(done(&mut e, 92).unwrap(), r#"["after-auth"]"#);
    }
}

#[test]
fn beta_an_old_inflight_read_cannot_clear_a_new_uncertain_write() {
    let (mut e, _, _) = ready(false);
    let send = |e: &mut safe_cex_engine::Engine<safe_cex_venue_kraken::KrakenSpot>,
                command,
                symbol: &str| {
        let mut order = opts("limit", 49.);
        order.symbol = symbol.into();
        e.handle(
            NOW,
            Event::Command(Command::PlaceOrders {
                command,
                orders: vec![order],
            }),
        );
        let request = requests(e).pop().unwrap();
        e.handle(
            NOW,
            Event::HttpResponse {
                request: request.id,
                response: safe_cex_engine::Response {
                    status: 503,
                    headers: "",
                    body: "<html>",
                },
            },
        );
    };
    send(&mut e, 81, "ETH/USD");
    let old = requests(&mut e)
        .into_iter()
        .find(|r| r.url.ends_with("OpenOrders"))
        .unwrap();
    send(&mut e, 82, "XBT/USD");
    assert!(
        !requests(&mut e)
            .iter()
            .any(|r| r.url.ends_with("OpenOrders"))
    );
    reply(&mut e, &old, r#"{"error":[],"result":{"open":{}}}"#);
    let fresh = requests(&mut e)
        .into_iter()
        .find(|r| r.url.ends_with("OpenOrders"))
        .unwrap();
    assert_ne!(old.id, fresh.id);
    e.handle(
        NOW,
        Event::Command(Command::CancelSymbolOrders {
            command: 83,
            symbol: "XBT/USD".into(),
        }),
    );
    let actions = e.drain_plain().collect::<Vec<_>>();
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed {command:83, error}) if error.message.contains("uncertain"))));
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    reply(&mut e, &fresh, r#"{"error":[],"result":{"open":{}}}"#);
    e.handle(
        NOW,
        Event::Command(Command::CancelSymbolOrders {
            command: 84,
            symbol: "XBT/USD".into(),
        }),
    );
    assert!(
        e.drain_plain()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 84, .. })))
    );
}

#[test]
fn beta_cancel_prefix_survives_a_later_refusal() {
    let (mut e, _, _) = ready(false);
    e.handle(
        NOW,
        Event::Command(Command::CancelOrders {
            command: 81,
            ids: vec!["a".into(), "b".into(), "c".into()],
        }),
    );
    let first = requests(&mut e).pop().unwrap();
    reply(&mut e, &first, r#"{"error":[],"result":{"count":1}}"#);
    let second = requests(&mut e).pop().unwrap();
    reply(&mut e, &second, r#"{"error":["EOrder:Unknown order"]}"#);
    let actions = e.drain_plain().collect::<Vec<_>>();
    let error = actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
            _ => None,
        })
        .unwrap();
    assert_eq!(error.partial.as_ref().unwrap().order_ids, ["a"]);
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a,Action::Http(r) if r.url.ends_with("CancelOrder")))
    );
}

#[test]
fn beta_unproven_status_never_grants_a_signature_resend() {
    for status in [408, 500, 503] {
        let (mut e, _, _) = ready(false);
        e.handle(
            NOW,
            Event::Command(Command::PlaceOrders {
                command: 81,
                orders: vec![opts("limit", 99.)],
            }),
        );
        let request = requests(&mut e).pop().unwrap();
        e.handle(
            NOW,
            Event::HttpResponse {
                request: request.id,
                response: safe_cex_engine::Response {
                    status,
                    headers: "",
                    body: r#"{"error":["EAPI:Invalid signature"]}"#,
                },
            },
        );
        let actions = e.drain_plain().collect::<Vec<_>>();
        assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed {error, ..}) if error.outcome == Some(safe_cex_engine::Outcome::Unknown))));
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("AddOrder")))
        );
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::Deactivate)))
        );
    }
}

#[test]
fn beta_unknown_placement_fences_retry_and_amend_until_a_fresh_read() {
    let (mut e, _, _) = ready(false);
    let place = |e: &mut safe_cex_engine::Engine<safe_cex_venue_kraken::KrakenSpot>, command| {
        e.handle(
            NOW,
            Event::Command(Command::PlaceOrders {
                command,
                orders: vec![opts("limit", 99.)],
            }),
        );
    };
    let mut other = opts("limit", 49.);
    other.symbol = "ETH/USD".into();
    e.handle(
        NOW,
        Event::Command(Command::PlaceOrders {
            command: 80,
            orders: vec![other],
        }),
    );
    let prior = requests(&mut e)
        .into_iter()
        .find(|r| r.url.ends_with("AddOrder"))
        .unwrap();
    e.handle(
        NOW,
        Event::HttpResponse {
            request: prior.id,
            response: safe_cex_engine::Response {
                status: 503,
                headers: "",
                body: "<html>",
            },
        },
    );
    let old = requests(&mut e)
        .into_iter()
        .find(|r| r.url.ends_with("OpenOrders"))
        .unwrap();
    place(&mut e, 81);
    let unknown = requests(&mut e)
        .into_iter()
        .find(|r| r.url.ends_with("AddOrder"))
        .unwrap();
    e.handle(
        NOW,
        Event::HttpResponse {
            request: unknown.id,
            response: safe_cex_engine::Response {
                status: 503,
                headers: "",
                body: "<html>",
            },
        },
    );
    let sealed = e.drain_plain().collect::<Vec<_>>();
    assert!(sealed.iter().any(|a| matches!(a,
        Action::Notice(Notice::CommandFailed { command: 81, error })
            if error.outcome == Some(safe_cex_engine::Outcome::Unknown))));
    assert!(
        !sealed
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("OpenOrders")))
    );
    let refused =
        |e: &mut safe_cex_engine::Engine<safe_cex_venue_kraken::KrakenSpot>, input, id| {
            e.handle(NOW, Event::Command(input));
            let actions = e.drain_plain().collect::<Vec<_>>();
            assert!(
                !actions.iter().any(
                    |a| matches!(a, Action::Http(r) if r.method != safe_cex_engine::Method::Get)
                ),
                "an uncertain placement must not admit another write: {actions:?}"
            );
            assert!(
                actions.iter().any(|a| matches!(a,
            Action::Notice(Notice::CommandFailed { command, error })
                if *command == id && error.outcome == Some(safe_cex_engine::Outcome::NotSent)
                && error.message.contains("uncertain"))),
                "{actions:?}"
            );
        };
    refused(
        &mut e,
        Command::PlaceOrders {
            command: 82,
            orders: vec![opts("limit", 99.)],
        },
        82,
    );
    refused(
        &mut e,
        Command::AmendOrder {
            command: 83,
            id: "O-100".into(),
            amount: Some(3.),
            price: Some(98.),
        },
        83,
    );
    reply(&mut e, &old, r#"{"error":[],"result":{"open":{}}}"#);
    let fresh = requests(&mut e)
        .into_iter()
        .find(|r| r.url.ends_with("OpenOrders"))
        .unwrap();
    assert_ne!(old.id, fresh.id);
    refused(
        &mut e,
        Command::PlaceOrders {
            command: 84,
            orders: vec![opts("limit", 99.)],
        },
        84,
    );
    reply(&mut e, &fresh, r#"{"error":[],"result":{"open":{}}}"#);
    place(&mut e, 85);
    assert!(requests(&mut e).iter().any(|r| r.url.ends_with("AddOrder")));
}

#[test]
fn beta_slow_reconcile_joins_reasks_and_only_a_clean_retry_releases() {
    let (mut e, _, _) = ready(false);
    let collect = |e: &mut safe_cex_engine::Engine<safe_cex_venue_kraken::KrakenSpot>| {
        e.drain_plain().collect::<Vec<_>>()
    };
    let read = |actions: &[Action]| {
        actions
            .iter()
            .find_map(|a| match a {
                Action::Http(r) if r.url.ends_with("OpenOrders") => Some(r.clone()),
                _ => None,
            })
            .expect("a reconcile read")
    };
    let place =
        |e: &mut safe_cex_engine::Engine<safe_cex_venue_kraken::KrakenSpot>, now, command| {
            e.handle(
                now,
                Event::Command(Command::PlaceOrders {
                    command,
                    orders: vec![opts("limit", 99.)],
                }),
            );
        };
    let respond = |e: &mut safe_cex_engine::Engine<safe_cex_venue_kraken::KrakenSpot>,
                   now,
                   request: &safe_cex_engine::HttpRequest,
                   status,
                   body: &str| {
        e.handle(
            now,
            Event::HttpResponse {
                request: request.id,
                response: safe_cex_engine::Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
    };
    let refused = |actions: &[Action]| {
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("AddOrder")))
        );
        assert!(
            actions.iter().any(|a| matches!(a,
            Action::Notice(Notice::CommandFailed { error, .. })
                if error.outcome == Some(safe_cex_engine::Outcome::NotSent)
                    && error.message.contains("uncertain"))),
            "{actions:?}"
        );
    };
    place(&mut e, NOW, 180);
    let sent = collect(&mut e);
    let write = sent
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with("AddOrder") => Some(r.clone()),
            _ => None,
        })
        .unwrap();
    respond(&mut e, NOW, &write, 503, "<html>");
    let mut log = collect(&mut e);
    let held = read(&log);
    // These are the engine's captured re-ask wakes, while the original read is held.
    let fire = |e: &mut safe_cex_engine::Engine<safe_cex_venue_kraken::KrakenSpot>,
                log: &mut Vec<Action>,
                offset: f64| {
        let timer = log
            .iter()
            .rev()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == NOW + offset => Some(*timer),
                _ => None,
            })
            .expect("the engine reconcile wake");
        e.handle(NOW + offset, Event::TimerFired { timer });
        let out = collect(e);
        log.extend(out.iter().cloned());
        out
    };
    for offset in [1000., 3000., 7000.] {
        let out = fire(&mut e, &mut log, offset);
        assert!(
            !out.iter()
                .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("OpenOrders"))),
            "a re-ask must join the original read: {out:?}"
        );
    }
    // Failure preserves the fence; a later engine wake supplies another read.
    respond(&mut e, NOW + 7100., &held, 503, "<html>");
    log.extend(collect(&mut e));
    place(&mut e, NOW + 7200., 181);
    refused(&collect(&mut e));
    let second = read(&fire(&mut e, &mut log, 15000.));
    respond(
        &mut e,
        NOW + 15100.,
        &second,
        200,
        r#"{"error":[],"result":{"open":{"unknown":{"descr":{}}}}}"#,
    );
    log.extend(collect(&mut e));
    place(&mut e, NOW + 15200., 182);
    refused(&collect(&mut e));
    let third = read(&fire(&mut e, &mut log, 31000.));
    respond(
        &mut e,
        NOW + 31100.,
        &third,
        200,
        r#"{"error":[],"result":{"open":{}}}"#,
    );
    collect(&mut e);
    place(&mut e, NOW + 31200., 183);
    assert!(
        collect(&mut e)
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("AddOrder")))
    );
}
