//! TS start()/ensureSignerAdapter(): remote callbacks never sign Lighter transactions.
mod support;
use safe_cex_engine::{Action, Command, Engine, Event, Notice, Query, TimerId};
use safe_cex_venue_lighter::Lighter;
use serde_json::json;
use support::Options;
use support::*;

struct Startup {
    engine: Engine<Lighter>,
    sockets: usize,
    private_requests: usize,
    timers: Vec<TimerId>,
}
fn startup(options: Options) -> Startup {
    let mut result = Startup {
        engine: support::engine(options),
        sockets: 0,
        private_requests: 0,
        timers: Vec::new(),
    };
    result.engine.start(NOW);
    for _ in 0..30 {
        let actions = safe_cex_engine::test_support::prepared_actions(&mut result.engine, NOW);
        if actions.is_empty() {
            break;
        }
        for action in actions {
            match action {
                Action::Http(request) => {
                    let body = if request.url.contains("orderBookDetails") {
                        META
                    } else if request.url.contains("exchangeStats") {
                        r#"{"code":200,"order_book_stats":[]}"#
                    } else if request.url.contains("accountActiveOrders") {
                        result.private_requests += 1;
                        OPEN
                    } else if request.url.contains("/account?") {
                        result.private_requests += 1;
                        ACCOUNT
                    } else {
                        panic!("unexpected startup {}", request.url)
                    };
                    reply(&mut result.engine, &request, 200, body);
                }
                Action::WsConnect { socket, .. } => {
                    result.sockets += 1;
                    result.engine.handle(NOW, Event::WsOpened { socket });
                }
                Action::SetTimer { timer, at_ms } if at_ms == NOW + 2_000.0 => {
                    result.timers.push(timer)
                }
                _ => {}
            }
        }
    }
    result
}
fn reject_write(engine: &mut Engine<Lighter>) {
    engine.handle(
        NOW,
        Event::Command(Command::PlaceOrders {
            command: 9,
            orders: vec![
                serde_json::from_value(
                    json!({"symbol":"ETH","type":"limit","side":"buy","amount":0.2,"price":90}),
                )
                .unwrap(),
            ],
        }),
    );
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
    assert!(actions.iter().any(|action| matches!(action,
        Action::Notice(Notice::CommandFailed { command: 9, error }) if error.outcome == Some(safe_cex_engine::Outcome::NotSent))));
}

#[test]
fn remote_only_and_unavailable_local_signers_keep_public_account_snapshots_without_private_signing()
{
    for opts in [
        Options {
            secret: None,
            remote_signer: true,
            ..options()
        },
        Options {
            secret: None,
            remote_signer: true,
            api_key_index: Some("999".into()),
            ..options()
        },
    ] {
        let mut result = startup(opts);
        assert_eq!(result.sockets, 1, "only public websocket connects");
        assert_eq!(
            result.private_requests, 2,
            "identity lookup and account snapshot"
        );
        assert_eq!(result.engine.store().positions().len(), 1);
        assert_eq!(result.engine.store().balance().free, 900.0);
        assert!(result.engine.store().orders().is_empty());
        assert!(result.engine.store().loaded().orders);
        assert!(result.engine.store().loaded().positions);
        assert!(result.engine.store().loaded().balance);
        reject_write(&mut result.engine);
        // Public fallback and account polling share the 2s period; exercise both.
        // Unavailable signers must never create authenticated order reads.
        for timer in result.timers {
            result
                .engine
                .handle(NOW + 2_000.0, Event::TimerFired { timer });
        }
        let reads = requests_at(&mut result.engine, NOW + 2_000.0);
        assert!(reads.iter().any(|read| read.url.contains("/account?")));
        assert!(
            reads
                .iter()
                .all(|read| !read.url.contains("auth=") && read.headers.is_empty())
        );
        assert!(
            !reads
                .iter()
                .any(|read| read.url.contains("accountActiveOrders"))
        );
    }
}

#[test]
fn remote_presence_does_not_override_valid_local_signer_or_supported_provider() {
    for provider in [None, Some(""), Some("tealstreet-sdk")] {
        let result = startup(Options {
            remote_signer: true,
            signer_provider: provider.map(str::to_owned),
            ..options()
        });
        assert_eq!(result.sockets, 2);
        assert_eq!(result.private_requests, 3);
        assert_eq!(result.engine.store().orders()[0].id, "42");
    }
}

#[test]
fn payload_only_and_explicit_public_or_missing_api_index_skip_account_startup() {
    for opts in [
        Options {
            secret: None,
            ..options()
        },
        Options {
            remote_signer: true,
            public_data_only: true,
            ..options()
        },
        Options {
            remote_signer: true,
            api_key_index: None,
            ..options()
        },
    ] {
        let result = startup(opts);
        assert_eq!(result.sockets, 1);
        assert_eq!(result.private_requests, 0);
        assert!(result.engine.store().loaded().orders);
        assert!(result.engine.store().loaded().positions);
        assert!(result.engine.store().loaded().balance);
    }
}

#[test]
fn remote_only_history_never_sends_private_requests_and_keeps_public_trades_available() {
    for opts in [
        Options {
            secret: None,
            remote_signer: true,
            ..options()
        },
        Options {
            secret: Some(String::new()),
            remote_signer: true,
            ..options()
        },
    ] {
        let mut result = startup(opts);
        for query in [
            Query::Fills {
                symbol: Some("ETH".into()),
                order_id: None,
                from_ms: None,
                to_ms: None,
                limit: None,
            },
            Query::OrderHistory {
                symbol: Some("ETH".into()),
                from_ms: None,
                to_ms: None,
                limit: None,
            },
            Query::Income {
                symbol: Some("ETH".into()),
                kind: None,
                from_ms: None,
                to_ms: None,
                limit: None,
            },
        ] {
            result
                .engine
                .handle_command(NOW, Command::Query { command: 10, query }, Some(20_000))
                .unwrap();
            let actions = result.engine.drain_actions().collect::<Vec<_>>();
            assert!(
                !actions
                    .iter()
                    .any(|action| matches!(action, Action::Http(_)))
            );
            assert!(actions.iter().any(|action| matches!(action, Action::Notice(Notice::CommandDone { command: 10, result: value }) if value == "[]")));
        }
        result
            .engine
            .handle_command(
                NOW,
                Command::Query {
                    command: 11,
                    query: Query::PublicTrades {
                        symbol: "ETH".into(),
                        limit: None,
                    },
                },
                Some(20_000),
            )
            .unwrap();
        let reads = requests(&mut result.engine);
        assert_eq!(reads.len(), 1);
        assert!(reads[0].url.contains("/trades?"));
        assert!(!reads[0].url.contains("account_index"));
        assert!(reads[0].headers.is_empty());
    }
}
