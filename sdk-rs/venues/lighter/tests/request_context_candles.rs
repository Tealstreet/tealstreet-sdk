//! Drive actual preparation and claim boundaries; no immediate fixture claim.
mod support;
use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{Claim, Error, HttpAction, Preparation, Prepared};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Notice, Outcome, TimerId, VenueError,
};
use safe_cex_venue_lighter::{Lighter, Options};
use support::{META, NOW, reply_at};

const CANDLES: &str = r#"{"code":200,"c":[{"t":2000,"o":"2","h":"3","l":"1","c":"2","v":"4"},{"t":1000,"o":"1","h":"2","l":"1","c":"2","v":"3"}]}"#;

fn started(options: Options) -> (Engine<Lighter>, TimerId) {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Lighter::new(options));
    engine.start(NOW);
    let metadata = safe_cex_engine::test_support::prepared_actions(&mut engine, NOW)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .unwrap();
    reply_at(&mut engine, NOW, &metadata, 200, META);
    let poll = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if at_ms == NOW + 2_000. => Some(timer),
            _ => None,
        })
        .unwrap();
    engine.take_frame(NOW);
    (engine, poll)
}

fn fetch(engine: &mut Engine<Lighter>, now: f64, command: u32) {
    engine.handle(
        now,
        Event::Command(Command::FetchCandles {
            command,
            symbol: "ETH".into(),
            interval: Interval::M1,
            limit: Some(2),
            from_ms: None,
            to_ms: None,
        }),
    );
}

fn preparation(actions: &[Action]) -> (Preparation, HttpRequest) {
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("/candles?"))),
        "candles cannot use legacy HTTP or send before final claim"
    );
    actions
        .iter()
        .find_map(|a| match a {
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } if request.url.contains("/candles?") => Some((*preparation, request.clone())),
                _ => None,
            },
            _ => None,
        })
        .expect("candle read must await managed route preparation")
}

fn release(engine: &mut Engine<Lighter>, token: Preparation, now: f64) -> HttpRequest {
    engine.http_prepared(
        now,
        token,
        Prepared {
            id: token.attempt().request.0,
            expires_at: None,
        },
    );
    let (permit, request) = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Bind {
                    permit, request, ..
                } => Some((permit, request)),
                _ => None,
            },
            _ => None,
        })
        .expect("prepared candle packet binds before claim");
    assert!(matches!(
        engine.claim_http(now, permit),
        Ok(Claim::Released(_))
    ));
    request
}

fn terminal(actions: &[Action], command: u32) -> Vec<Result<String, VenueError>> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone {
                command: id,
                result,
            }) if *id == command => Some(Ok(result.clone())),
            Action::Notice(Notice::CommandFailed { command: id, error }) if *id == command => {
                Some(Err(error.clone()))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn aliases_park_public_candles_then_return_sorted_data_only_after_claim() {
    for robinhood in [false, true] {
        for testnet in [false, true] {
            let (mut engine, _) = started(Options {
                public_data_only: true,
                robinhood,
                testnet,
                ..Options::default()
            });
            fetch(&mut engine, NOW, 500);
            let actions: Vec<_> = engine.drain_actions().collect();
            let (token, unsigned) = preparation(&actions);
            assert!(unsigned.headers.is_empty());
            assert!(terminal(&actions, 500).is_empty());
            // Neither an early complete response nor streamed callbacks prove release.
            reply_at(&mut engine, NOW + 1., &unsigned, 200, CANDLES);
            engine.handle(
                NOW + 1.,
                Event::HttpStart {
                    request: unsigned.id,
                    status: 200,
                    headers: "",
                },
            );
            engine.handle(
                NOW + 1.,
                Event::HttpChunk {
                    request: unsigned.id,
                    data: CANDLES.as_bytes(),
                },
            );
            engine.handle(
                NOW + 1.,
                Event::HttpEnd {
                    request: unsigned.id,
                },
            );
            assert!(terminal(&engine.drain_actions().collect::<Vec<_>>(), 500).is_empty());
            let request = release(&mut engine, token, NOW + 2.);
            reply_at(&mut engine, NOW + 3., &request, 200, CANDLES);
            let results = terminal(&engine.drain_actions().collect::<Vec<_>>(), 500);
            assert_eq!(results.len(), 1);
            #[derive(serde::Deserialize)]
            struct CandleTime {
                timestamp: f64,
            }
            let rows: Vec<CandleTime> = serde_json::from_str(results[0].as_ref().unwrap()).unwrap();
            assert_eq!(
                rows.iter().map(|r| r.timestamp).collect::<Vec<_>>(),
                vec![1000., 2000.]
            );
        }
    }
}

#[test]
fn preparation_failure_preserves_typed_detail_and_finishes_once() {
    let (mut engine, _) = started(Options {
        public_data_only: true,
        ..Options::default()
    });
    fetch(&mut engine, NOW, 501);
    let (token, _) = preparation(&engine.drain_actions().collect::<Vec<_>>());
    let failure = VenueError::new("route denied").kind(ErrorKind::AuthPermission);
    engine.http_preparation_failed(NOW + 1., token, Error::State, failure.clone());
    let actions: Vec<_> = engine.drain_actions().collect();
    let results = terminal(&actions, 501);
    assert_eq!(results.len(), 1);
    let error = results[0].as_ref().unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::AuthPermission));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert_eq!(error.message, "route denied");
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    engine.http_preparation_failed(NOW + 2., token, Error::State, failure);
    assert!(terminal(&engine.drain_actions().collect::<Vec<_>>(), 501).is_empty());
}

#[test]
fn parked_timeout_and_late_read_cannot_complete_reused_command() {
    for claimed in [false, true] {
        let (mut engine, _) = started(Options {
            public_data_only: true,
            ..Options::default()
        });
        fetch(&mut engine, NOW, 502);
        let actions: Vec<_> = engine.drain_actions().collect();
        let (token, unsigned) = preparation(&actions);
        let timer = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == NOW + 20_000. => Some(*timer),
                _ => None,
            })
            .unwrap();
        if claimed {
            release(&mut engine, token, NOW + 1.);
        }
        engine.handle(NOW + 20_000., Event::TimerFired { timer });
        let results = terminal(&engine.drain_actions().collect::<Vec<_>>(), 502);
        assert_eq!(results.len(), 1);
        assert!(results[0].is_err());
        if !claimed {
            assert_eq!(
                results[0].as_ref().unwrap_err().kind,
                Some(ErrorKind::Timeout)
            );
        }
        fetch(&mut engine, NOW + 20_001., 502);
        let (next, _) = preparation(&engine.drain_actions().collect::<Vec<_>>());
        reply_at(&mut engine, NOW + 20_002., &unsigned, 200, CANDLES);
        assert!(terminal(&engine.drain_actions().collect::<Vec<_>>(), 502).is_empty());
        let request = release(&mut engine, next, NOW + 20_003.);
        reply_at(&mut engine, NOW + 20_004., &request, 200, CANDLES);
        assert!(matches!(
            terminal(&engine.drain_actions().collect::<Vec<_>>(), 502).as_slice(),
            [Ok(_)]
        ));
    }
}

#[test]
fn failed_or_expired_stream_discards_its_rows() {
    for expired in [false, true] {
        let (mut engine, _) = started(Options {
            public_data_only: true,
            ..Options::default()
        });
        fetch(&mut engine, NOW, 503);
        let (token, _) = preparation(&engine.drain_actions().collect::<Vec<_>>());
        let request = release(&mut engine, token, NOW + 1.);
        engine.handle(
            NOW + 2.,
            Event::HttpStart {
                request: request.id,
                status: 200,
                headers: "",
            },
        );
        engine.handle(
            NOW + 2.,
            Event::HttpChunk {
                request: request.id,
                data: br#"{"code":200,"c":[{"t":1000,"o":"1","h":"2","l":"1","c":"2","v":"3"},"#,
            },
        );
        assert!(terminal(&engine.drain_actions().collect::<Vec<_>>(), 503).is_empty());
        let now = if expired { NOW + 20_000. } else { NOW + 3. };
        engine.handle(
            now,
            Event::HttpChunk {
                request: request.id,
                data: if expired { b"]}" } else { b"{}]}" },
            },
        );
        engine.handle(
            now,
            Event::HttpEnd {
                request: request.id,
            },
        );
        assert!(matches!(
            terminal(&engine.drain_actions().collect::<Vec<_>>(), 503).as_slice(),
            [Err(_)]
        ));
    }
}

#[test]
fn background_failure_retires_only_its_generation_and_next_poll_can_recover() {
    for timeout in [false, true] {
        let (mut engine, poll) = started(Options {
            public_data_only: true,
            ..Options::default()
        });
        engine.handle(
            NOW,
            Event::Command(Command::SubscribeCandles {
                symbol: "ETH".into(),
                interval: Interval::M1,
            }),
        );
        engine.drain_actions().for_each(drop);
        engine.handle(NOW + 120_000., Event::TimerFired { timer: poll });
        let actions: Vec<_> = engine.drain_actions().collect();
        let (token, _) = preparation(&actions);
        let next_poll = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == NOW + 122_000. => Some(*timer),
                _ => None,
            })
            .unwrap();
        if timeout {
            let timers: Vec<_> = actions
                .iter()
                .filter_map(|a| match a {
                    Action::SetTimer { timer, at_ms } if *at_ms == NOW + 140_000. => Some(*timer),
                    _ => None,
                })
                .collect();
            assert!(!timers.is_empty());
            for timer in timers {
                engine.handle(NOW + 140_000., Event::TimerFired { timer });
            }
        } else {
            engine.http_preparation_failed(
                NOW + 120_001.,
                token,
                Error::State,
                VenueError::new("route denied"),
            );
        }
        let actions: Vec<_> = engine.drain_actions().collect();
        assert!(!actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
        )));
        let now = if timeout {
            NOW + 140_001.
        } else {
            NOW + 122_000.
        };
        engine.handle(now, Event::TimerFired { timer: next_poll });
        let (next, _) = preparation(&engine.drain_actions().collect::<Vec<_>>());
        assert_ne!(token, next);
        let request = release(&mut engine, next, now + 1.);
        reply_at(&mut engine, now + 2., &request, 200, CANDLES);
        assert!(engine.take_frame(now + 2.).is_some());
    }
}

#[test]
fn old_subscription_response_within_deadline_cannot_publish_to_new_subscriber() {
    for failed in [false, true] {
        let (mut engine, poll) = started(Options {
            public_data_only: true,
            ..Options::default()
        });
        let subscribe = |engine: &mut Engine<Lighter>, now| {
            engine.handle(
                now,
                Event::Command(Command::SubscribeCandles {
                    symbol: "ETH".into(),
                    interval: Interval::M1,
                }),
            );
        };
        subscribe(&mut engine, NOW);
        engine.drain_actions().for_each(drop);
        engine.handle(NOW + 120_000., Event::TimerFired { timer: poll });
        let actions: Vec<_> = engine.drain_actions().collect();
        let (old, _) = preparation(&actions);
        let poll = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == NOW + 122_000. => Some(*timer),
                _ => None,
            })
            .unwrap();
        let old_request = release(&mut engine, old, NOW + 120_001.);
        engine.handle(
            NOW + 120_002.,
            Event::Command(Command::UnsubscribeCandles {
                symbol: "ETH".into(),
                interval: Interval::M1,
            }),
        );
        subscribe(&mut engine, NOW + 120_003.);
        engine.drain_actions().for_each(drop);
        engine.handle(NOW + 122_000., Event::TimerFired { timer: poll });
        let actions: Vec<_> = engine.drain_actions().collect();
        assert!(!actions.iter().any(|a| matches!(a, Action::ManagedHttp(_))));
        let poll = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == NOW + 124_000. => Some(*timer),
                _ => None,
            })
            .unwrap();
        engine.take_frame(NOW + 122_000.);
        if failed {
            engine.handle(
                NOW + 122_001.,
                Event::HttpFailed {
                    request: old_request.id,
                    error: "old request failed",
                },
            );
        } else {
            reply_at(&mut engine, NOW + 122_001., &old_request, 200, CANDLES);
        }
        assert!(engine.take_frame(NOW + 122_001.).is_none());
        engine.drain_actions().for_each(drop);
        // Resubscription gets its own full quiet interval before another fallback.
        engine.handle(NOW + 240_004., Event::TimerFired { timer: poll });
        preparation(&engine.drain_actions().collect::<Vec<_>>());
    }
}

#[test]
fn issuance_capacity_refusal_is_visible_without_consuming_a_later_slot() {
    let (mut engine, _) = started(Options {
        public_data_only: true,
        ..Options::default()
    });
    let mut first = None;
    for command in 1..=256 {
        fetch(&mut engine, NOW, command);
        let (token, _) = preparation(&engine.drain_actions().collect::<Vec<_>>());
        first.get_or_insert(token);
    }
    fetch(&mut engine, NOW, 257);
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(matches!(terminal(&actions, 257).as_slice(), [Err(_)]));
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
    );
    engine.http_preparation_failed(
        NOW + 1.,
        first.unwrap(),
        Error::State,
        VenueError::new("free slot"),
    );
    engine.drain_actions().for_each(drop);
    fetch(&mut engine, NOW + 2., 257);
    preparation(&engine.drain_actions().collect::<Vec<_>>());
}

#[test]
fn command_entry_capacity_is_typed_before_native_validation_and_preserves_peers() {
    for robinhood in [false, true] {
        for testnet in [false, true] {
            let (mut engine, _) = started(Options {
                public_data_only: true,
                robinhood,
                testnet,
                ..Options::default()
            });
            let mut first = None;
            for command in 1..=256 {
                fetch(&mut engine, NOW, command);
                let (token, _) = preparation(&engine.drain_actions().collect::<Vec<_>>());
                first.get_or_insert(token);
            }
            for (command, symbol) in [(257, "ETH"), (258, "UNKNOWN")] {
                engine.handle(
                    NOW,
                    Event::Command(Command::FetchCandles {
                        command,
                        symbol: symbol.into(),
                        interval: Interval::M1,
                        limit: Some(2),
                        from_ms: None,
                        to_ms: None,
                    }),
                );
                let actions: Vec<_> = engine.drain_actions().collect();
                let result = terminal(&actions, command);
                assert_eq!(result.len(), 1, "one correlated entry refusal");
                let error = result[0].as_ref().unwrap_err();
                assert_eq!(error.kind, Some(ErrorKind::Internal));
                assert_eq!(error.outcome, None, "Read refusal has no financial outcome");
                assert_eq!(error.message, "Engine request capacity reached");
                assert_eq!(
                    actions.len(),
                    1,
                    "refusal allocates no HTTP, timer or other I/O"
                );
            }
            let request = release(&mut engine, first.unwrap(), NOW + 1.);
            reply_at(&mut engine, NOW + 2., &request, 200, CANDLES);
            let actions: Vec<_> = engine.drain_actions().collect();
            assert!(matches!(terminal(&actions, 1).as_slice(), [Ok(_)]));
            assert!(terminal(&actions, 258).is_empty());
            fetch(&mut engine, NOW + 3., 258);
            preparation(&engine.drain_actions().collect::<Vec<_>>());
        }
    }
}

#[test]
fn duplicate_live_command_entry_cannot_replace_its_original_candle_owner() {
    let (mut engine, _) = started(Options {
        public_data_only: true,
        ..Options::default()
    });
    fetch(&mut engine, NOW, 700);
    let (original, _) = preparation(&engine.drain_actions().collect::<Vec<_>>());
    fetch(&mut engine, NOW + 1., 700);
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(matches!(
        actions.as_slice(),
        [Action::Notice(Notice::Event(event))]
            if event.code == safe_cex_engine::log::code::COMMAND_DUPLICATE
    ));
    assert!(terminal(&actions, 700).is_empty());
    let request = release(&mut engine, original, NOW + 2.);
    reply_at(&mut engine, NOW + 3., &request, 200, CANDLES);
    assert!(matches!(
        terminal(&engine.drain_actions().collect::<Vec<_>>(), 700).as_slice(),
        [Ok(_)]
    ));
}

#[test]
fn invalid_command_entry_clock_has_one_typed_read_refusal_and_no_owner() {
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let (mut engine, _) = started(Options {
            public_data_only: true,
            ..Options::default()
        });
        fetch(&mut engine, now, 701);
        let actions: Vec<_> = engine.drain_actions().collect();
        let result = terminal(&actions, 701);
        assert_eq!(result.len(), 1);
        let error = result[0].as_ref().unwrap_err();
        assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
        assert_eq!(error.outcome, None);
        assert_eq!(error.message, "Invalid request context");
        assert_eq!(actions.len(), 1, "invalid entry allocates no HTTP or timer");
        fetch(&mut engine, NOW + 1., 701);
        preparation(&engine.drain_actions().collect::<Vec<_>>());
    }
}
