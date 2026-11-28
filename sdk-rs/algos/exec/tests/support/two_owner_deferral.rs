use super::*;
use std::time::Duration;

// Native EXEC binds ownership to AccountPort identity, not a login name.
// These owners have separate ports/stores/scopes but colliding request/command IDs.
fn owner(symbol: &str) -> (Rc<Port>, AlgoCx, ExecFactory) {
    let port = Rc::new(Port::default());
    wait_market(&port);
    {
        let mut store = port.store.borrow_mut();
        let mut market = store.markets.remove("BTCUSDT").unwrap();
        market.id = symbol.into();
        market.symbol = symbol.into();
        store.markets.insert(symbol.into(), market);
        let mut ticker = store.tickers.remove("BTCUSDT").unwrap();
        ticker.symbol = symbol.into();
        store.tickers.insert(symbol.into(), ticker);
    }
    let cx = AlgoCx::new(port.clone(), Scope::new());
    (port, cx, ExecFactory::default())
}

fn start(factory: &ExecFactory, cx: &AlgoCx, symbol: &str, command: &str, req: &str) {
    factory
        .handle(
            "exec",
            &json!({"symbol":symbol,"command":command,"reqId":req}).to_string(),
            cx,
        )
        .unwrap();
}

fn results(port: &Port) -> Vec<Value> {
    port.events
        .borrow()
        .iter()
        .filter(|(op, _)| op == "exec-result")
        .map(|(_, data)| data.clone())
        .collect()
}

fn answer(port: &Port, value: &str) {
    port.replies
        .borrow_mut()
        .remove(0)
        .send(Ok(value.into()))
        .unwrap();
}

fn successful_result(port: &Port, req: &str) {
    let replies = results(port);
    assert_eq!(replies.len(), 1, "{replies:?}");
    assert_eq!(replies[0]["reqId"], req);
    assert_eq!(replies[0]["handled"], true);
    assert_eq!(replies[0]["success"], true);
}

#[tokio::test]
async fn two_owner_command_completion_cannot_release_a_successor_or_complete_b_token() {
    LocalSet::new()
        .run_until(async {
            for queued_before_abort_poll in [false, true] {
                let (a, old, factory_a) = owner("AUSDT");
                start(&factory_a, &old, "AUSDT", "cancel; lev 9", "same");
                settle_exec().await;
                assert!(matches!(&a.commands.borrow()[0], Command::CancelSymbolOrders { command: 1, symbol } if symbol == "AUSDT"));
                let delayed_a = a.replies.borrow_mut().remove(0);

                let (b, current, factory_b) = owner("BUSDT");
                assert!(!Rc::ptr_eq(&old.port, &current.port));
                assert_eq!(
                    factory_a.handle("exec", r#"{"command":"cancel"}"#, &current),
                    Err("ExecFactory must be account-scoped".into())
                );
                start(&factory_b, &current, "BUSDT", "cancel; lev 3", "same");
                settle_exec().await;
                assert!(matches!(&b.commands.borrow()[0], Command::CancelSymbolOrders { command: 1, symbol } if symbol == "BUSDT"));
                assert_eq!((factory_a.pending_operations(), factory_b.pending_operations()), (1, 1));
                let old_events = a.events.borrow().clone();
                old.scope.dispose();
                if queued_before_abort_poll {
                    assert!(delayed_a.send(Ok("[]".into())).is_ok());
                } else {
                    settle_exec().await;
                    assert!(delayed_a.send(Ok("[]".into())).is_err());
                }
                settle_exec().await;
                assert_eq!((factory_a.pending_operations(), factory_b.pending_operations()), (0, 1));
                assert_eq!(a.commands.borrow().len(), 1);
                assert_eq!(*a.events.borrow(), old_events);
                assert_eq!(b.commands.borrow().len(), 1);
                assert!(results(&b).is_empty(), "A's answer cannot complete B's colliding token");

                answer(&b, "[]");
                settle_exec().await;
                assert_eq!(b.commands.borrow().len(), 2);
                assert!(matches!(&b.commands.borrow()[1], Command::SetLeverage { symbol, leverage, .. } if symbol == "BUSDT" && *leverage == 3.));
                answer(&b, "[]");
                settle_exec().await;
                successful_result(&b, "same");
                assert_eq!(factory_b.pending_operations(), 0);
                assert_eq!(a.commands.borrow().len(), 1);
                assert_eq!(*a.events.borrow(), old_events);
                current.scope.dispose();
            }
        })
        .await;
}

#[tokio::test]
async fn two_owner_abort_returns_only_a_credits_while_b_remains_at_its_limit() {
    LocalSet::new()
        .run_until(async {
            let (a, old, factory_a) = owner("AUSDT");
            let parallel = "@cancel;".repeat(safe_cex_exec::MAX_PARALLEL);
            start(&factory_a, &old, "AUSDT", &parallel, "same");
            settle_exec().await;
            assert_eq!(a.commands.borrow().len(), 32);
            let (b, current, factory_b) = owner("BUSDT");
            start(&factory_b, &current, "BUSDT", &parallel, "same");
            settle_exec().await;
            assert_eq!(
                b.commands.borrow().len(),
                32,
                "A's reservations do not consume B's budget"
            );
            assert_eq!(
                (
                    factory_a.pending_operations(),
                    factory_b.pending_operations()
                ),
                (32, 32)
            );
            let old_events = a.events.borrow().clone();
            let initial_b_results = results(&b);

            old.scope.dispose();
            settle_exec().await;
            for delayed in a.replies.borrow_mut().drain(..) {
                assert!(delayed.send(Ok("[]".into())).is_err());
            }
            assert_eq!(
                (
                    factory_a.pending_operations(),
                    factory_b.pending_operations()
                ),
                (0, 32)
            );
            assert_eq!(results(&b), initial_b_results);
            start(&factory_b, &current, "BUSDT", "@cancel", "overflow");
            settle_exec().await;
            assert_eq!(
                b.commands.borrow().len(),
                32,
                "returning A's credits cannot admit B's overflow"
            );
            let refusal: Vec<_> = results(&b)
                .into_iter()
                .filter(|r| r["reqId"] == "overflow")
                .collect();
            assert_eq!(refusal.len(), 1);
            assert_eq!(refusal[0]["reqId"], "overflow");
            assert_eq!(refusal[0]["success"], false);
            let pending_b_results = results(&b);
            for reply in b.replies.borrow_mut().drain(..) {
                reply.send(Ok("[]".into())).unwrap();
            }
            settle_exec().await;
            assert_eq!(factory_b.pending_operations(), 0);
            assert_eq!(results(&b), pending_b_results);
            start(&factory_b, &current, "BUSDT", &parallel, "next");
            settle_exec().await;
            assert_eq!(
                b.commands.borrow().len(),
                64,
                "only B's settlements replenish B's budget"
            );
            assert_eq!(*a.events.borrow(), old_events);
            assert_eq!(a.commands.borrow().len(), 32);
            current.scope.dispose();
            settle_exec().await;
            assert_eq!(factory_b.pending_operations(), 0);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn two_owner_sleep_clock_and_condition_waits_keep_their_original_scope_and_store() {
    LocalSet::new()
        .run_until(async {
            for (old_wait, new_wait, condition) in [
                ("sleep 5s", "sleep 100ms", false),
                ("wait time 0000", "wait time 0000", false),
                ("wait price > 60000 for=200ms timeout=10s", "wait price > 60000 for=200ms timeout=10s", true),
            ] {
                let (a, old, factory_a) = owner("AUSDT");
                start(&factory_a, &old, "AUSDT", &format!("{old_wait}; lev 9"), "same");
                settle_exec().await;
                assert!(a.commands.borrow().is_empty());
                assert_eq!(factory_a.pending_operations(), 1);
                let (b, current, factory_b) = owner("BUSDT");
                {
                    let mut store = b.store.borrow_mut();
                    let ticker = store.tickers.get_mut("BUSDT").unwrap();
                    ticker.last = 70000.;
                    ticker.bid = 69999.;
                    ticker.ask = 70001.;
                }
                start(&factory_b, &current, "BUSDT", &format!("{new_wait}; lev 3"), "same");
                settle_exec().await;
                assert_eq!(factory_b.pending_operations(), 1);
                if condition {
                    for _ in 0..3 {
                        tokio::time::advance(Duration::from_millis(100)).await;
                        settle_exec().await;
                    }
                    assert!(a.commands.borrow().is_empty(), "B's matching price cannot satisfy A's live wait");
                    assert_eq!(b.commands.borrow().len(), 1, "B's own store satisfies its sustained condition");
                }
                let old_events = a.events.borrow().clone();
                old.scope.dispose();
                // Exceed every held deadline, including next midnight, without wall-clock sleeps.
                tokio::time::advance(Duration::from_secs(86_401)).await;
                settle_exec().await;
                assert!(a.commands.borrow().is_empty(), "disposed wait cannot release lev 9: {old_wait}");
                assert_eq!(*a.events.borrow(), old_events);
                assert_eq!(factory_a.pending_operations(), 0);
                assert_eq!(b.commands.borrow().len(), 1);
                assert!(matches!(&b.commands.borrow()[0], Command::SetLeverage { symbol, leverage, .. } if symbol == "BUSDT" && *leverage == 3.));
                assert!(results(&b).is_empty());
                answer(&b, "[]");
                settle_exec().await;
                successful_result(&b, "same");
                assert_eq!(factory_b.pending_operations(), 0);
                current.scope.dispose();
            }
        })
        .await;
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum NukePause {
    Cancel,
    CloseRetry,
    FreshReadSettle,
    FreshRead,
}

#[tokio::test(start_paused = true)]
async fn two_owner_nuke_pending_cancel_retry_settle_and_read_never_reach_b_or_sweep_a() {
    LocalSet::new()
        .run_until(async {
            for pause in [NukePause::Cancel, NukePause::CloseRetry, NukePause::FreshReadSettle, NukePause::FreshRead] {
                let (a, old, factory_a) = owner("AUSDT");
                a.store.borrow_mut().positions = vec![native_position("AUSDT")];
                a.store.borrow_mut().orders = vec![native_order("AUSDT")];
                a.fresh_positions.set(matches!(pause, NukePause::FreshReadSettle | NukePause::FreshRead));
                start(&factory_a, &old, "AUSDT", "nuke", "same");
                settle_exec().await;
                assert!(matches!(&a.commands.borrow()[0], Command::CancelSymbolOrders { symbol, .. } if symbol == "AUSDT"));
                if pause != NukePause::Cancel {
                    answer(&a, "null");
                    settle_exec().await;
                    assert!(matches!(&a.commands.borrow()[1], Command::PlaceOrders { orders, .. } if orders[0].symbol == "AUSDT" && orders[0].reduce_only == Some(true)));
                    if pause == NukePause::CloseRetry {
                        a.replies.borrow_mut().remove(0).send(Err(VenueError::new("transient close refusal"))).unwrap();
                    } else {
                        answer(&a, r#"["a-close"]"#);
                    }
                    settle_exec().await;
                }
                if pause == NukePause::FreshRead {
                    for _ in 0..25 {
                        tokio::time::advance(Duration::from_millis(100)).await;
                        settle_exec().await;
                    }
                    assert_eq!(a.commands.borrow().len(), 3);
                    assert!(is_fresh_read(&a.commands.borrow()[2]));
                }
                let delayed_a = a.replies.borrow_mut().pop();
                assert_eq!(delayed_a.is_some(), matches!(pause, NukePause::Cancel | NukePause::FreshRead));
                let old_commands = a.commands.borrow().len();
                let old_events = a.events.borrow().clone();
                assert_eq!(factory_a.pending_operations(), 2, "nuke guard and reservation are held: {pause:?}");

                let (b, current, factory_b) = owner("BUSDT");
                let mut position_b = native_position("BUSDT");
                position_b.contracts = 0.07;
                position_b.notional = 3500.;
                b.store.borrow_mut().positions = vec![position_b];
                b.store.borrow_mut().orders = vec![native_order("BUSDT")];
                start(&factory_b, &current, "BUSDT", "nuke", "same");
                settle_exec().await;
                assert!(matches!(&b.commands.borrow()[0], Command::CancelSymbolOrders { command: 1, symbol } if symbol == "BUSDT"));
                old.scope.dispose();
                settle_exec().await;
                if let Some(delayed) = delayed_a {
                    assert!(delayed.send(Ok(open_answer("AUSDT", 0.01))).is_err());
                }
                a.store.borrow_mut().orders = vec![native_order("A-LATE")];
                tokio::time::advance(Duration::from_secs(5)).await;
                settle_exec().await;
                assert_eq!(a.commands.borrow().len(), old_commands, "no old retry/read/sweep: {pause:?}");
                assert_eq!(*a.events.borrow(), old_events);
                assert_eq!((factory_a.pending_operations(), factory_b.pending_operations()), (0, 2));
                assert_eq!(b.commands.borrow().len(), 1);
                assert!(results(&b).is_empty(), "old nuke cannot complete B's token");

                answer(&b, "null");
                settle_exec().await;
                assert_eq!(b.commands.borrow().len(), 2);
                assert!(matches!(&b.commands.borrow()[1], Command::PlaceOrders { orders, .. } if orders.len() == 1 && orders[0].symbol == "BUSDT" && orders[0].amount == 0.07 && orders[0].reduce_only == Some(true)));
                b.store.borrow_mut().positions.clear();
                b.store.borrow_mut().orders = vec![native_order("B-LATE")];
                answer(&b, r#"["b-close"]"#);
                settle_exec().await;
                assert_eq!(b.commands.borrow().len(), 3);
                assert!(matches!(&b.commands.borrow()[2], Command::CancelSymbolOrders { symbol, .. } if symbol == "B-LATE"));
                answer(&b, "null");
                settle_exec().await;
                successful_result(&b, "same");
                assert_eq!(factory_b.pending_operations(), 0);
                assert_eq!(a.commands.borrow().len(), old_commands);
                assert_eq!(*a.events.borrow(), old_events);
                current.scope.dispose();
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn two_owner_public_nuke_future_created_before_switch_cannot_take_b_guard_or_port() {
    LocalSet::new()
        .run_until(async {
            let (a, old, factory_a) = owner("AUSDT");
            a.store.borrow_mut().orders = vec![native_order("AUSDT")];
            let delayed_a = factory_a.nuke("orders", &old).unwrap();
            assert_eq!(factory_a.pending_operations(), 1);
            assert!(a.commands.borrow().is_empty(), "future has not yet been polled");
            let (b, current, factory_b) = owner("BUSDT");
            b.store.borrow_mut().orders = vec![native_order("BUSDT")];
            let pending_b = factory_b.nuke("orders", &current).unwrap();
            assert_eq!(factory_b.pending_operations(), 1);
            old.scope.dispose();
            let failure = delayed_a.await.unwrap_err();
            assert_eq!(
                failure["message"],
                "Nuke was interrupted when the account closed. Check your orders and positions before trying again."
            );
            assert!(a.commands.borrow().is_empty());
            assert!(a.events.borrow().is_empty());
            assert_eq!((factory_a.pending_operations(), factory_b.pending_operations()), (0, 1));
            assert!(factory_b.nuke("orders", &current).is_err(), "dropping A's guard cannot unlock B's pending nuke");
            let result = answering(&b, pending_b).await;
            assert!(result.is_ok(), "{result:?}");
            assert_eq!(b.commands.borrow().len(), 1);
            assert!(matches!(&b.commands.borrow()[0], Command::CancelSymbolOrders { command: 1, symbol } if symbol == "BUSDT"));
            assert_eq!(factory_b.pending_operations(), 0);
            current.scope.dispose();
        })
        .await;
}
