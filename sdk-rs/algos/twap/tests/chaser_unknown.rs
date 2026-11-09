//! Stop-after-Unknown regressions through the real chaser and TWAP runtimes.
#![cfg(not(target_arch = "wasm32"))]

use std::cell::{Cell, Ref, RefCell};
use std::rc::Rc;
use std::time::Duration;

use safe_cex_algo_twap::factory::TwapFactory;
use safe_cex_algos::{AccountPort, AlgoCx, AlgoStore, Change, CommandFuture, Scope};
use safe_cex_chaser::ChaserBook;
use safe_cex_engine::engine::{Command, CommandId};
use safe_cex_engine::error::{ErrorKind, Outcome, VenueError};
use safe_cex_engine::model::{Limits, Market, MinMax, Precision, Ticker};
use safe_cex_engine::write_plan::{Element, ElementResult, WritePlan};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::task::LocalSet;

mod support;
use support::{ClockGuard, ENGINE_UNKNOWN_TEXT};

#[derive(Default)]
struct Port {
    store: RefCell<AlgoStore>,
    ids: Cell<CommandId>,
    commands: RefCell<Vec<Command>>,
    emits: RefCell<Vec<(String, Value)>>,
    placement_error: RefCell<Option<VenueError>>,
    cancel_error: RefCell<Option<VenueError>>,
}

impl AccountPort for Port {
    fn store(&self) -> Ref<'_, AlgoStore> {
        self.store.borrow()
    }

    fn subscribe(&self) -> mpsc::UnboundedReceiver<Change> {
        mpsc::unbounded_channel().1
    }

    fn next_command_id(&self) -> CommandId {
        self.ids.set(self.ids.get() + 1);
        self.ids.get()
    }

    fn command(&self, command: Command) -> CommandFuture {
        let reply = match &command {
            Command::PlaceOrders { command, .. } => self
                .placement_error
                .borrow_mut()
                .take()
                .map_or_else(|| Ok(json!([format!("native-{command}")]).to_string()), Err),
            Command::CancelOrders { .. } | Command::CancelOrdersWithSymbols { .. } => self
                .cancel_error
                .borrow()
                .clone()
                .map_or_else(|| Ok("[]".into()), Err),
            _ => Ok("[]".into()),
        };
        self.commands.borrow_mut().push(command);
        Box::pin(async move { reply })
    }

    fn emit(&self, op: &str, data: String) {
        self.emits
            .borrow_mut()
            .push((op.into(), serde_json::from_str(&data).unwrap()));
    }

    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities::BASE
    }
}

impl Port {
    fn placements(&self) -> usize {
        self.commands
            .borrow()
            .iter()
            .filter(|command| matches!(command, Command::PlaceOrders { .. }))
            .count()
    }

    fn cancels(&self) -> usize {
        self.commands
            .borrow()
            .iter()
            .filter(|command| {
                matches!(
                    command,
                    Command::CancelOrders { .. } | Command::CancelOrdersWithSymbols { .. }
                )
            })
            .count()
    }

    fn assert_unknown_report(&self) {
        let emits = self.emits.borrow();
        let outcomes: Vec<_> = emits
            .iter()
            .filter(|(op, _)| op == "twap:outcome")
            .collect();
        assert_eq!(outcomes.len(), 1);
        let error = &outcomes[0].1["error"];
        assert_eq!(error["message"], ENGINE_UNKNOWN_TEXT);
        assert_eq!(error["outcome"], "unknown");
        let rows = error["elements"].as_array().unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["index"], 0);
        assert_eq!(rows[0]["status"], "unknown");
        assert_eq!(rows[0]["error"]["message"], ENGINE_UNKNOWN_TEXT);
        assert!(rows[1..].iter().all(|row| row["status"] == "notSent"));
        let telemetry: Vec<_> = emits.iter().filter(|(op, _)| op == "telemetry").collect();
        assert_eq!(telemetry.len(), 1);
        assert_eq!(telemetry[0].1["kind"], "twap.unknown_outcome");
        assert_eq!(telemetry[0].1["unknown"], 1);
        assert_eq!(telemetry[0].1["twapId"], outcomes[0].1["id"]);
        assert_eq!(telemetry[0].1["error"], *error);
        assert!(
            emits
                .iter()
                .any(|(op, data)| op == "error" && data == ENGINE_UNKNOWN_TEXT)
        );
    }
}

fn fixture() -> (Rc<Port>, AlgoCx, TwapFactory) {
    let port = Rc::new(Port::default());
    {
        let mut store = port.store.borrow_mut();
        store.venue = "binance".into();
        store.markets.insert(
            "BTCUSDT".into(),
            Market {
                id: "BTCUSDT".into(),
                symbol: "BTCUSDT".into(),
                base: "BTC".into(),
                quote: "USDT".into(),
                active: true,
                precision: Precision {
                    amount: 0.001,
                    price: 0.5,
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
                pretty_symbol: None,
                contract_size: None,
                is_expiring: None,
                is_inverse: None,
                is_spot: None,
                is_perpetual: None,
                is_read_only: None,
                taker_fee: None,
                maker_fee: None,
            },
        );
        store.tickers.insert(
            "BTCUSDT".into(),
            Ticker {
                symbol: "BTCUSDT".into(),
                bid: 100.0,
                ask: 101.0,
                last: 100.0,
                ..Default::default()
            },
        );
    }
    let cx = AlgoCx::new(port.clone(), Scope::new());
    let chasers = ChaserBook::new(cx.clone());
    let factory = TwapFactory::new("a", cx.clone(), Rc::new(chasers));
    (port, cx, factory)
}

fn start(factory: &TwapFactory) {
    factory
        .start(
            serde_json::from_value(json!({
                "accountName": "a", "activeMarket": {"symbol": "BTCUSDT"},
                "size": 3, "duration": 0.1, "ordersCount": 3, "side": "buy",
                "chaserDelay": 1000, "reduceOnly": false, "randomnessPercentage": 0,
                "useLimitOrders": false, "useChaserOrders": true,
                "timeoutPct": 50, "timeoutAction": "next", "failAction": "next"
            }))
            .unwrap(),
        )
        .unwrap();
}

async fn advance(ms: u64) {
    for _ in 0..ms / 100 {
        tokio::time::advance(Duration::from_millis(100)).await;
        for _ in 0..12 {
            tokio::task::yield_now().await;
        }
    }
}

fn partial(unknown: bool, outer: Outcome) -> VenueError {
    let mut journal = WritePlan::new(
        (0..2)
            .map(|index| Element {
                index,
                client_order_id: None,
            })
            .collect(),
    );
    journal
        .record(
            0,
            ElementResult::Placed {
                order_ids: vec!["native-accepted".into()],
            },
        )
        .unwrap();
    let refused = VenueError::new("last lot refused").outcome(Outcome::Rejected);
    journal
        .record(
            1,
            if unknown {
                ElementResult::Unknown {
                    error: Box::new(
                        VenueError::new("lost native answer").outcome(Outcome::Unknown),
                    ),
                }
            } else {
                ElementResult::NotSent {
                    error: Some(Box::new(refused)),
                }
            },
        )
        .unwrap();
    journal.attach(
        VenueError::new("partial native placement")
            .outcome(outer)
            .partial_ids(&["native-accepted".into()]),
    )
}

async fn placement_case(unknown: bool, outer: Outcome) {
    LocalSet::new()
        .run_until(async {
            let _clock = ClockGuard::new();
            let (port, cx, factory) = fixture();
            *port.placement_error.borrow_mut() = Some(partial(unknown, outer));
            start(&factory);
            advance(6000).await;
            if unknown || outer == Outcome::Unknown {
                assert_eq!(
                    port.placements(),
                    1,
                    "native Unknown must stop later TWAP children"
                );
                assert_eq!(
                    port.cancels(),
                    0,
                    "do not cancel the accepted native prefix after Unknown"
                );
                port.assert_unknown_report();
                assert!(factory.active_twaps("a").is_empty());
            } else {
                assert_eq!(
                    port.placements(),
                    3,
                    "known partial refusal retains TS continuation"
                );
                assert_eq!(port.cancels(), 3);
                assert!(
                    !port
                        .emits
                        .borrow()
                        .iter()
                        .any(|(op, _)| op == "twap:outcome")
                );
            }
            cx.scope.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn known_native_partial_continues_real_chaser_twap() {
    placement_case(false, Outcome::Rejected).await;
}

#[tokio::test(start_paused = true)]
async fn outer_unknown_partial_stops_real_chaser_twap() {
    placement_case(false, Outcome::Unknown).await;
}

#[tokio::test(start_paused = true)]
async fn nested_native_unknown_stops_real_chaser_twap() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS TwapInstance.ts:181-194,255-280 continues.
    placement_case(true, Outcome::Rejected).await;
}

async fn cancellation_case(kind: Option<ErrorKind>) {
    LocalSet::new()
        .run_until(async {
            let _clock = ClockGuard::new();
            let (port, cx, factory) = fixture();
            *port.cancel_error.borrow_mut() = kind.map(|kind| {
                VenueError::new("lost cancel answer")
                    .kind(kind)
                    .outcome(Outcome::Unknown)
            });
            start(&factory);
            advance(6000).await;
            if kind.is_some() {
                assert_eq!(port.cancels(), 1, "Unknown cancel must NEVER auto-retry");
                assert_eq!(
                    port.placements(),
                    1,
                    "Unknown cancel must stop later TWAP children"
                );
                port.assert_unknown_report();
                assert!(factory.active_twaps("a").is_empty());
                advance(650_000).await;
                assert_eq!(
                    port.cancels(),
                    1,
                    "no delayed retry past the old 600-second bound"
                );
                assert_eq!(port.placements(), 1);
            } else {
                assert_eq!(port.placements(), 3);
                assert_eq!(port.cancels(), 3);
                assert!(
                    !port
                        .emits
                        .borrow()
                        .iter()
                        .any(|(op, _)| op == "twap:outcome")
                );
            }
            cx.scope.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn successful_timeout_cancel_continues_real_chaser_twap() {
    cancellation_case(None).await;
}

#[tokio::test(start_paused = true)]
async fn unknown_timeout_cancel_stops_real_chaser_twap_without_retry() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS TwapInstance.ts:213-220,255-280 continues.
    for kind in [ErrorKind::Network, ErrorKind::Timeout, ErrorKind::Rejected] {
        cancellation_case(Some(kind)).await;
    }
}
