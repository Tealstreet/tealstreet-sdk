//! Original native entry controls; production is deliberately unchanged.
#[path = "support/account.rs"]
mod support;
use safe_cex_engine::model::{Interval, StpMode};
use safe_cex_engine::{Action, Command, Engine, ErrorKind, Event, Notice, Outcome, Query};
use safe_cex_venue_txflow::{Options, TxFlow};

#[derive(Clone, Copy, Debug)]
enum Case {
    PublicTrades,
    ClientLookup,
    HistoryIdentity,
    HistoryRange,
    HistorySymbol,
    EmptyIncome,
    CandleSymbol,
    CandleInterval,
    CandleRange,
    CachedLeverage,
    MissingLeverage,
    Account,
    Book,
    ReadStp,
    WriteStp,
    WriteIdentity,
    WriteValidation,
}
const CASES: [Case; 17] = [
    Case::PublicTrades,
    Case::ClientLookup,
    Case::HistoryIdentity,
    Case::HistoryRange,
    Case::HistorySymbol,
    Case::EmptyIncome,
    Case::CandleSymbol,
    Case::CandleInterval,
    Case::CandleRange,
    Case::CachedLeverage,
    Case::MissingLeverage,
    Case::Account,
    Case::Book,
    Case::ReadStp,
    Case::WriteStp,
    Case::WriteIdentity,
    Case::WriteValidation,
];
fn fresh() -> Engine<TxFlow> {
    safe_cex_engine::test_support::seeded_engine(TxFlow::new(Options {
        testnet: true,
        ..Default::default()
    }))
}
fn fixture(case: Case) -> Engine<TxFlow> {
    if matches!(
        case,
        Case::HistoryRange
            | Case::HistorySymbol
            | Case::EmptyIncome
            | Case::CandleInterval
            | Case::CandleRange
            | Case::CachedLeverage
            | Case::WriteValidation
    ) {
        let (mut engine, _, reads, _) = support::bootstrap();
        support::answer(&mut engine, reads);
        engine.drain_actions().for_each(drop);
        engine
    } else {
        fresh()
    }
}
fn query(query: Query) -> Command {
    Command::Query { command: 91, query }
}
fn command(case: Case) -> Command {
    match case {
        Case::PublicTrades => query(Query::PublicTrades {
            symbol: "BTC-USDC".into(),
            limit: None,
        }),
        Case::ClientLookup => query(Query::OrderByClientId {
            symbol: "BTC-USDC".into(),
            client_order_id: "client".into(),
        }),
        Case::HistoryIdentity | Case::HistoryRange | Case::HistorySymbol => {
            query(Query::OrderHistory {
                symbol: matches!(case, Case::HistorySymbol).then(|| "UNKNOWN".into()),
                from_ms: None,
                to_ms: None,
                limit: matches!(case, Case::HistoryRange).then_some(0),
            })
        }
        Case::EmptyIncome => query(Query::Income {
            symbol: Some("BTC-USDC".into()),
            kind: Some("OTHER".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        }),
        Case::CandleSymbol | Case::CandleInterval | Case::CandleRange => Command::FetchCandles {
            command: 91,
            symbol: if matches!(case, Case::CandleSymbol) {
                "UNKNOWN".into()
            } else {
                "BTC-USDC".into()
            },
            interval: if matches!(case, Case::CandleInterval) {
                Interval::H3
            } else {
                Interval::M1
            },
            limit: Some(1),
            from_ms: matches!(case, Case::CandleRange).then_some(2.),
            to_ms: matches!(case, Case::CandleRange).then_some(1.),
        },
        Case::CachedLeverage | Case::MissingLeverage => Command::FetchLeverage {
            command: 91,
            symbol: "BTC-USDC".into(),
        },
        Case::Account => Command::FetchAccount { command: 91 },
        Case::Book => Command::FetchOrderBook {
            command: 91,
            symbol: "BTC-USDC".into(),
            limit: None,
        },
        Case::ReadStp => Command::FetchStpMode { command: 91 },
        Case::WriteStp => Command::SetStpMode {
            command: 91,
            mode: StpMode::ExpireMaker,
        },
        Case::WriteIdentity | Case::WriteValidation => Command::SetLeverage {
            command: 91,
            symbol: "BTC-USDC".into(),
            leverage: 0.,
            side: None,
        },
    }
}
fn terminal(actions: &[Action]) -> &Notice {
    let terminals: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(
                notice @ (Notice::CommandDone { .. } | Notice::CommandFailed { .. }),
            ) => Some(notice),
            _ => None,
        })
        .collect();
    assert_eq!(terminals.len(), 1, "one exact terminal: {actions:?}");
    assert_eq!(actions.len(), 1, "no physical I/O, timer or follow-up");
    terminals[0]
}
fn invalid(case: Case) {
    let mut engine = fixture(case);
    let before = engine.store().positions().to_vec();
    engine.handle(f64::NAN, Event::Command(command(case)));
    let actions: Vec<_> = engine.drain_actions().collect();
    let Notice::CommandFailed { command, error } = terminal(&actions) else {
        panic!("invalid entry succeeded: {actions:?}");
    };
    assert_eq!(*command, 91);
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
    assert_eq!(
        error.outcome,
        matches!(
            case,
            Case::WriteStp | Case::WriteIdentity | Case::WriteValidation
        )
        .then_some(Outcome::NotSent)
    );
    assert_eq!(engine.store().positions(), before);
}
macro_rules! entry_control {
    ($name:ident, $case:ident) => {
        #[test]
        fn $name() {
            invalid(Case::$case);
        }
    };
}
entry_control!(public_trades_requires_valid_entry, PublicTrades);
entry_control!(client_lookup_requires_valid_entry, ClientLookup);
entry_control!(history_identity_requires_valid_entry, HistoryIdentity);
entry_control!(history_range_requires_valid_entry, HistoryRange);
entry_control!(history_symbol_requires_valid_entry, HistorySymbol);
entry_control!(empty_income_requires_valid_entry, EmptyIncome);
entry_control!(candle_symbol_requires_valid_entry, CandleSymbol);
entry_control!(candle_interval_requires_valid_entry, CandleInterval);
entry_control!(candle_range_requires_valid_entry, CandleRange);
entry_control!(cached_leverage_requires_valid_entry, CachedLeverage);
entry_control!(missing_leverage_requires_valid_entry, MissingLeverage);
entry_control!(book_query_requires_valid_entry, Book);
entry_control!(read_stp_requires_valid_entry, ReadStp);
entry_control!(write_stp_requires_valid_entry, WriteStp);
entry_control!(write_identity_requires_valid_entry, WriteIdentity);
entry_control!(write_validation_requires_valid_entry, WriteValidation);

fn native_message(case: Case) -> &'static str {
    match case {
        Case::PublicTrades => {
            "TxFlow does not document a REST public trades history query; use the trades stream"
        }
        Case::ClientLookup => "TxFlow indexed lookup cannot prove non-placement",
        Case::HistoryIdentity => "TxFlow testnet account identity required",
        Case::HistoryRange => "Invalid TxFlow history range or limit",
        Case::HistorySymbol => "Unknown or ambiguous TxFlow history symbol",
        Case::CandleSymbol => "Market not found",
        Case::CandleInterval => "TxFlow does not support 3h candles",
        Case::CandleRange => "Invalid candle time range",
        Case::MissingLeverage => "TxFlow account leverage is not confirmed for BTC-USDC",
        Case::Account => "This action is not supported on this venue",
        Case::Book => "TxFlow order book queries are not implemented; use the book subscription",
        Case::ReadStp | Case::WriteStp => {
            "TxFlow does not document configurable self-trade prevention"
        }
        Case::WriteIdentity => "TxFlow trading requires an authenticated testnet account",
        Case::WriteValidation => "Invalid TxFlow leverage",
        Case::EmptyIncome | Case::CachedLeverage => unreachable!(),
    }
}
#[test]
fn finite_native_branches_keep_exact_results_errors_and_no_io() {
    for case in CASES {
        let mut engine = fixture(case);
        engine.handle(support::NOW + 30., Event::Command(command(case)));
        let actions: Vec<_> = engine.drain_actions().collect();
        match (case, terminal(&actions)) {
            (Case::EmptyIncome, Notice::CommandDone { command, result }) => {
                assert_eq!(*command, 91);
                assert_eq!(result, "[]");
            }
            (Case::CachedLeverage, Notice::CommandDone { command, result }) => {
                assert_eq!(*command, 91);
                assert_eq!(
                    result,
                    r#"{"buyLeverage":10.0,"sellLeverage":10.0,"marginType":"cross"}"#
                );
            }
            (_, Notice::CommandFailed { command, error }) => {
                assert_eq!(*command, 91);
                assert_eq!(error.message, native_message(case), "{case:?}");
                let unsupported = matches!(case, Case::Book | Case::ReadStp | Case::WriteStp);
                assert_eq!(
                    error.kind,
                    if unsupported {
                        Some(ErrorKind::Unsupported)
                    } else if matches!(case, Case::WriteIdentity) {
                        Some(ErrorKind::AuthInvalid)
                    } else {
                        None
                    }
                );
                assert_eq!(
                    error.outcome,
                    (unsupported || matches!(case, Case::WriteIdentity | Case::WriteValidation))
                        .then_some(Outcome::NotSent)
                );
            }
            _ => panic!("wrong native terminal for {case:?}: {actions:?}"),
        }
    }
}
#[test]
fn settled_local_command_id_can_be_reused_without_physical_work() {
    let mut engine = fresh();
    for case in [Case::Account, Case::ReadStp, Case::WriteStp] {
        engine.handle(support::NOW, Event::Command(command(case)));
        let actions: Vec<_> = engine.drain_actions().collect();
        assert!(matches!(
            terminal(&actions),
            Notice::CommandFailed { command: 91, .. }
        ));
    }
}

#[test]
fn unsupported_account_capability_keeps_the_shared_prevenue_refusal() {
    let mut engine = fresh();
    engine.handle(f64::NAN, Event::Command(command(Case::Account)));
    let actions: Vec<_> = engine.drain_actions().collect();
    let Notice::CommandFailed { command, error } = terminal(&actions) else {
        panic!("unsupported account capability succeeded");
    };
    assert_eq!(*command, 91);
    assert_eq!(error.message, "This action is not supported on this venue");
    assert_eq!(error.kind, None);
    assert_eq!(error.outcome, None);
}
