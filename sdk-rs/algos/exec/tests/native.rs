#![cfg(not(target_arch = "wasm32"))]
use safe_cex_algos::{AccountPort, AlgoCx, AlgoFactory, AlgoStore, Change, CommandFuture, Scope};
use safe_cex_engine::engine::{Command, CommandId};
use safe_cex_engine::error::{Outcome, Partial, VenueError};
use safe_cex_exec::native::{ExecFactory, serialize_error, translate};
use serde_json::{Value, json};
use std::cell::{Cell, Ref, RefCell};
use std::rc::Rc;
use tokio::sync::{mpsc, oneshot};
use tokio::task::LocalSet;

#[derive(Default)]
struct Port {
    store: RefCell<AlgoStore>,
    next: Cell<u32>,
    commands: RefCell<Vec<Command>>,
    replies: RefCell<Vec<oneshot::Sender<Result<String, VenueError>>>>,
    events: RefCell<Vec<(String, Value)>>,
    fresh_positions: Cell<bool>,
    nuke_account_cancel: Cell<bool>,
    symbol_cancel_misses_unseen: Cell<bool>,
    fail_cancels: Cell<bool>,
}
impl AccountPort for Port {
    fn store(&self) -> Ref<'_, AlgoStore> {
        self.store.borrow()
    }
    fn subscribe(&self) -> mpsc::UnboundedReceiver<Change> {
        mpsc::unbounded_channel().1
    }
    fn next_command_id(&self) -> CommandId {
        let n = self.next.get() + 1;
        self.next.set(n);
        n
    }
    fn command(&self, command: Command) -> CommandFuture {
        self.commands.borrow_mut().push(command);
        let (tx, rx) = oneshot::channel();
        self.replies.borrow_mut().push(tx);
        Box::pin(async move {
            rx.await
                .unwrap_or_else(|_| Err(VenueError::new("lost").outcome(Outcome::Unknown)))
        })
    }
    fn emit(&self, op: &str, data: String) {
        self.events
            .borrow_mut()
            .push((op.into(), serde_json::from_str(&data).unwrap()));
    }
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            fresh_positions: self.fresh_positions.get(),
            nuke_account_cancel: self.nuke_account_cancel.get(),
            symbol_cancel_misses_unseen: self.symbol_cancel_misses_unseen.get(),
            ..safe_cex_engine::Capabilities::BASE
        }
    }
}

#[tokio::test]
async fn actual_shared_runtime_delivers_typed_engine_commands_and_wire_reply() {
    LocalSet::new().run_until(async {
  let port=Rc::new(Port::default());let cx=AlgoCx::new(port.clone(),Scope::new());
  ExecFactory::default().handle("exec",r#"{"symbol":"BTCUSDT","command":"cancel; lev 3","reqId":"r"}"#,&cx).unwrap();
  tokio::task::yield_now().await;
  assert!(matches!(&port.commands.borrow()[0],Command::CancelSymbolOrders{symbol,..} if symbol=="BTCUSDT"));
  assert_eq!(port.commands.borrow().len(),1);
  port.replies.borrow_mut().remove(0).send(Ok("[]".into())).unwrap();
  tokio::task::yield_now().await;tokio::task::yield_now().await;
  assert!(matches!(&port.commands.borrow()[1],Command::SetLeverage{symbol,leverage,..} if symbol=="BTCUSDT" && *leverage==3.));
  port.replies.borrow_mut().remove(0).send(Ok("[]".into())).unwrap();
  tokio::task::yield_now().await;
  assert!(port.events.borrow().iter().any(|(op,data)|op=="exec-result" && data==&json!({"reqId":"r","handled":true,"success":true})));
 }).await;
}

#[tokio::test]
async fn closing_shared_scope_during_a_command_never_issues_the_successor() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            let cx = AlgoCx::new(port.clone(), Scope::new());
            ExecFactory::default()
                .handle(
                    "exec",
                    r#"{"symbol":"BTCUSDT","command":"cancel; lev 9","reqId":"r"}"#,
                    &cx,
                )
                .unwrap();
            tokio::task::yield_now().await;
            assert_eq!(port.commands.borrow().len(), 1);
            cx.scope.dispose();
            tokio::task::yield_now().await;
            assert!(
                port.replies
                    .borrow_mut()
                    .remove(0)
                    .send(Ok("[]".into()))
                    .is_err()
            );
            tokio::task::yield_now().await;
            assert_eq!(port.commands.borrow().len(), 1);
            assert!(
                port.events
                    .borrow()
                    .iter()
                    .all(|(op, _)| op != "exec-result")
            );
        })
        .await;
}

#[test]
fn cancellation_keeps_routing_symbols_and_partial_errors_keep_real_ids() {
    let symbol_routed = safe_cex_engine::Capabilities {
        cancel_with_symbols: true,
        ..safe_cex_engine::Capabilities::BASE
    };
    let command = translate(
        "cancelOrders",
        &json!([[{"id":"7","symbol":"BTC-PERP"}]]),
        1,
        &symbol_routed,
    )
    .unwrap();
    assert!(
        matches!(command,Command::CancelOrdersWithSymbols{orders,..} if orders[0].symbol=="BTC-PERP" && orders[0].id=="7")
    );
    // A venue without a symbol-qualified cancel (Bybit) refuses that form: cancel by id.
    let command = translate(
        "cancelOrders",
        &json!([[{"id":"7","symbol":"BTC-PERP"}]]),
        1,
        &safe_cex_engine::Capabilities::BASE,
    )
    .unwrap();
    assert!(matches!(command, Command::CancelOrders { ids, .. } if ids == ["7"]));
    let mut error = VenueError::new("lost").outcome(Outcome::Unknown);
    error.partial = Some(Box::new(Partial {
        order_ids: vec!["7".into()],
        total: Some(2),
        failed_index: Some(1),
    }));
    let data = serialize_error(error);
    assert_eq!(data["partialOrderIds"], json!(["7"]));
    assert_eq!(data["totalOrderCount"], 2);
    assert_eq!(data["outcome"], "unknown");
    assert!(
        translate(
            "invented",
            &json!([]),
            2,
            &safe_cex_engine::Capabilities::BASE
        )
        .is_err()
    );
}

#[tokio::test]
async fn parallel_budget_spans_payloads_and_refuses_second_whole_payload() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = ExecFactory::default();
            for req in ["one", "two"] {
                let data = json!({"symbol":"BTCUSDT","command":"@cancel;".repeat(20),"reqId":req});
                factory.handle("exec", &data.to_string(), &cx).unwrap();
            }
            tokio::task::yield_now().await;
            assert_eq!(
                port.commands.borrow().len(),
                20,
                "second payload dispatches nothing"
            );
            assert!(
                port.events
                    .borrow()
                    .iter()
                    .any(|(op, data)| op == "exec-result"
                        && data["reqId"] == "two"
                        && data["success"] == false)
            );
            for reply in port.replies.borrow_mut().drain(..) {
                reply.send(Ok("[]".into())).unwrap();
            }
            tokio::task::yield_now().await;
            factory
                .handle(
                    "exec",
                    &json!({"symbol":"BTCUSDT","command":"@cancel;".repeat(20),"reqId":"three"})
                        .to_string(),
                    &cx,
                )
                .unwrap();
            tokio::task::yield_now().await;
            assert_eq!(
                port.commands.borrow().len(),
                40,
                "settlement returns all credits"
            );
            cx.scope.dispose();
        })
        .await;
}

#[tokio::test]
async fn scope_abort_returns_account_parallel_credits_without_a_replay() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = ExecFactory::default();
            let data = json!({"symbol":"BTCUSDT","command":"@cancel;".repeat(20)}).to_string();
            factory.handle("exec", &data, &cx).unwrap();
            tokio::task::yield_now().await;
            cx.scope.dispose();
            tokio::task::yield_now().await;
            let next = AlgoCx::new(port.clone(), Scope::new());
            factory.handle("exec", &data, &next).unwrap();
            tokio::task::yield_now().await;
            assert_eq!(
                port.commands.borrow().len(),
                40,
                "20 new calls, no replay of old calls"
            );
            next.scope.dispose();
        })
        .await;
}

#[test]
fn a_factory_cannot_share_credits_or_state_across_accounts() {
    let factory = ExecFactory::default();
    let one = AlgoCx::new(Rc::new(Port::default()), Scope::new());
    let two = AlgoCx::new(Rc::new(Port::default()), Scope::new());
    factory
        .handle("exec", r#"{"command":"echo hi"}"#, &one)
        .unwrap();
    assert_eq!(
        factory.handle("exec", r#"{"command":"echo hi"}"#, &two),
        Err("ExecFactory must be account-scoped".into())
    );
}

#[tokio::test]
async fn settling_and_aborting_return_each_credit_once_in_either_order() {
    LocalSet::new()
        .run_until(async {
            for settle_first in [false, true] {
                let port = Rc::new(Port::default());
                let factory = ExecFactory::default();
                let old = AlgoCx::new(port.clone(), Scope::new());
                let payload =
                    json!({"symbol":"BTCUSDT","command":"@cancel;".repeat(32)}).to_string();
                factory.handle("exec", &payload, &old).unwrap();
                tokio::task::yield_now().await;
                assert_eq!(port.commands.borrow().len(), 32);
                for reply in port.replies.borrow_mut().drain(..) {
                    reply.send(Ok("[]".into())).unwrap();
                }
                if settle_first {
                    tokio::task::yield_now().await;
                }
                old.scope.dispose();
                tokio::task::yield_now().await;
                let next = AlgoCx::new(port.clone(), Scope::new());
                factory.handle("exec", &payload, &next).unwrap();
                factory
                    .handle(
                        "exec",
                        &json!({"symbol":"BTCUSDT","command":"@cancel","reqId":"overflow"})
                            .to_string(),
                        &next,
                    )
                    .unwrap();
                tokio::task::yield_now().await;
                assert_eq!(
                    port.commands.borrow().len(),
                    64,
                    "all 32 credits returned exactly once, settle_first={settle_first}"
                );
                assert!(
                    port.events
                        .borrow()
                        .iter()
                        .any(|(op, data)| op == "exec-result"
                            && data["reqId"] == "overflow"
                            && data["success"] == false)
                );
                next.scope.dispose();
                tokio::task::yield_now().await;
            }
        })
        .await;
}

#[tokio::test]
async fn forty_plain_single_step_payloads_share_the_same_account_budget() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = ExecFactory::default();
            for req in 0..40 {
                factory
                    .handle(
                        "exec",
                        &json!({"symbol":"BTCUSDT","command":"cancel","reqId":req}).to_string(),
                        &cx,
                    )
                    .unwrap();
            }
            tokio::task::yield_now().await;
            assert_eq!(port.commands.borrow().len(), 32);
            assert_eq!(
                port.events
                    .borrow()
                    .iter()
                    .filter(|(op, data)| op == "exec-result" && data["success"] == false)
                    .count(),
                8
            );
            for reply in port.replies.borrow_mut().drain(..) {
                reply.send(Ok("[]".into())).unwrap();
            }
            tokio::task::yield_now().await;
            factory
                .handle("exec", r#"{"symbol":"BTCUSDT","command":"cancel"}"#, &cx)
                .unwrap();
            tokio::task::yield_now().await;
            assert_eq!(port.commands.borrow().len(), 33);
            cx.scope.dispose();
        })
        .await;
}

#[tokio::test]
async fn native_buy_uses_engine_store_and_returns_real_ids_without_replaying_unknown() {
    use safe_cex_engine::model::{Limits, Market, MinMax, Precision, Ticker};
    LocalSet::new().run_until(async {
        let port=Rc::new(Port::default());
        port.store.borrow_mut().markets.insert("BTCUSDT".into(),Market{
            id:"BTCUSDT".into(),symbol:"BTCUSDT".into(),pretty_symbol:None,base:"BTC".into(),quote:"USDT".into(),active:true,
            precision:Precision{amount:0.001,price:0.1},limits:Limits{amount:MinMax{min:0.001,max:1000.},leverage:MinMax{min:1.,max:20.},price:None},
            contract_size:None,is_expiring:None,is_inverse:None,is_spot:None,is_perpetual:None,is_read_only:None,taker_fee:None,maker_fee:None,
        });
        port.store.borrow_mut().tickers.insert("BTCUSDT".into(),Ticker{symbol:"BTCUSDT".into(),last:50000.,..Default::default()});
        let cx=AlgoCx::new(port.clone(),Scope::new());let factory=ExecFactory::default();
        factory.handle("exec",r#"{"symbol":"BTCUSDT","command":"buy $100 at 49900 po","reqId":"buy"}"#,&cx).unwrap();
        tokio::task::yield_now().await;
        assert_eq!(port.commands.borrow().len(),1,"{:?}",port.events.borrow());
        assert!(matches!(&port.commands.borrow()[0],Command::PlaceOrders{orders,..} if orders.len()==1 && orders[0].amount==0.002 && orders[0].price==Some(49900.) && orders[0].client_order_id.is_none()));
        port.replies.borrow_mut().remove(0).send(Ok(r#"["native-parent","native-stop"]"#.into())).unwrap();
        tokio::task::yield_now().await;
        assert!(port.events.borrow().iter().any(|(op,data)|op=="exec-result" && data["data"]["orderIds"]==json!(["native-parent","native-stop"])));
        factory.handle("exec",r#"{"symbol":"BTCUSDT","command":"buy $100","reqId":"lost"}"#,&cx).unwrap();
        tokio::task::yield_now().await;
        port.replies.borrow_mut().remove(0).send(Err(VenueError::new("lost").outcome(Outcome::Unknown))).unwrap();
        tokio::task::yield_now().await;tokio::task::yield_now().await;
        assert_eq!(port.commands.borrow().len(),2);
        assert!(port.events.borrow().iter().any(|(op,data)|op=="exec-result" && data["reqId"]=="lost" && data["errorDetails"]["outcome"]=="unknown"));
        cx.scope.dispose();
    }).await;
}

#[test]
fn native_amend_translation_changes_price_only_and_retains_the_real_order_id() {
    let command=translate("updateOrder",&json!([{"order":{"id":"native-id","symbol":"BTCUSDT","side":"sell","amount":7,"reduceOnly":true},"update":{"price":123.5}}]),99,&safe_cex_engine::Capabilities::BASE).unwrap();
    assert!(
        matches!(command,Command::AmendOrder{command:99,id,price:Some(123.5),amount:None} if id=="native-id")
    );
    // A venue whose amend needs the symbol (KuCoin Spot) gets the symbol-routed form.
    let with_symbol = safe_cex_engine::Capabilities {
        amend_with_symbol: true,
        ..safe_cex_engine::Capabilities::BASE
    };
    let command = translate(
        "updateOrder",
        &json!([{"order":{"id":"native-id","symbol":"BTCUSDT"},"update":{"amount":2}}]),
        99,
        &with_symbol,
    )
    .unwrap();
    assert!(
        matches!(command,Command::AmendOrderWithSymbol{id,symbol,price:None,amount:Some(2.0),..} if id=="native-id" && symbol=="BTCUSDT")
    );
}

#[tokio::test]
async fn native_swarm_checks_shared_account_credits_before_any_second_payload_slice() {
    use safe_cex_engine::model::{Limits, Market, MinMax, Precision, Ticker};
    LocalSet::new().run_until(async {
        let port=Rc::new(Port::default());
        port.store.borrow_mut().markets.insert("BTCUSDT".into(),Market{
            id:"BTCUSDT".into(),symbol:"BTCUSDT".into(),pretty_symbol:None,base:"BTC".into(),quote:"USDT".into(),active:true,
            precision:Precision{amount:0.001,price:0.1},limits:Limits{amount:MinMax{min:0.001,max:1000.},leverage:MinMax{min:1.,max:20.},price:None},
            contract_size:None,is_expiring:None,is_inverse:None,is_spot:None,is_perpetual:None,is_read_only:None,taker_fee:None,maker_fee:None,
        });
        port.store.borrow_mut().tickers.insert("BTCUSDT".into(),Ticker{symbol:"BTCUSDT".into(),last:50000.,..Default::default()});
        let cx=AlgoCx::new(port.clone(),Scope::new());let factory=ExecFactory::default();
        for req in ["one","two"]{factory.handle("exec",&json!({"symbol":"BTCUSDT","command":"swarm buy $5000 into 20","reqId":req}).to_string(),&cx).unwrap();}
        tokio::task::yield_now().await;
        assert_eq!(port.commands.borrow().len(),20);
        assert!(port.commands.borrow().iter().all(|command|matches!(command,Command::PlaceOrders{orders,..} if orders.len()==1 && orders[0].reduce_only==Some(false))));
        assert!(port.events.borrow().iter().any(|(op,data)|op=="exec-result"&&data["reqId"]=="two"&&data["success"]==false));
        cx.scope.dispose();tokio::task::yield_now().await;
    }).await;
}

#[tokio::test(start_paused = true)]
async fn cli_twap_starts_exactly_one_real_shared_schedule_and_list_stop_use_it() {
    use safe_cex_algo_twap::factory::TwapFactory;
    use safe_cex_chaser::ChaserBook;
    use safe_cex_engine::model::{Limits, Market, MinMax, Precision, Ticker};
    LocalSet::new().run_until(async {
        let port=Rc::new(Port::default());
        port.store.borrow_mut().markets.insert("BTCUSDT".into(), Market {
            id:"BTCUSDT".into(),symbol:"BTCUSDT".into(),pretty_symbol:None,base:"BTC".into(),quote:"USDT".into(),active:true,
            precision:Precision{amount:0.001,price:0.1},limits:Limits{amount:MinMax{min:0.001,max:1000.},leverage:MinMax{min:1.,max:20.},price:None},
            contract_size:None,is_expiring:None,is_inverse:None,is_spot:None,is_perpetual:None,is_read_only:None,taker_fee:None,maker_fee:None,
        });
        port.store.borrow_mut().tickers.insert("BTCUSDT".into(),Ticker{symbol:"BTCUSDT".into(),last:50000.,bid:49999.,ask:50001.,..Default::default()});
        let cx=AlgoCx::new(port.clone(),Scope::new());let book=ChaserBook::new(cx.clone());
        let twap=TwapFactory::new("trusted-account",cx.clone(),Rc::new(book.clone()));
        let exec=ExecFactory::with_algos("trusted-account".into(),book.clone(),twap.clone());
        exec.handle("exec",r#"{"symbol":"BTCUSDT","command":"twap buy $1000 into 2 over 1","reqId":"start","accountName":"untrusted"}"#,&cx).unwrap();
        for _ in 0..8 {tokio::task::yield_now().await;}
        let active=twap.active_twaps("trusted-account");assert_eq!(active.len(),1,"{:?}",port.events.borrow());
        assert!(twap.active_twaps("untrusted").is_empty());
        assert_eq!(active[0]["duration"],1.);assert_eq!(active[0]["ordersCount"],2);
        assert_eq!(port.commands.borrow().len(),1);
        assert!(matches!(&port.commands.borrow()[0],Command::PlaceOrders{orders,..} if orders.len()==1 && orders[0].amount==0.01));
        exec.handle("exec",r#"{"symbol":"BTCUSDT","command":"twap list","reqId":"list"}"#,&cx).unwrap();
        let id=active[0]["id"].as_str().unwrap();
        assert!(port.events.borrow().iter().any(|(op,e)|op=="exec-log"&&e.to_string().contains(id)));
        exec.handle("exec",r#"{"symbol":"BTCUSDT","command":"twap stop all","reqId":"stop"}"#,&cx).unwrap();
        tokio::time::advance(std::time::Duration::from_secs(61)).await;
        for _ in 0..8 {tokio::task::yield_now().await;}
        assert_eq!(port.commands.borrow().len(),1,"stop cannot release the second slice");
        exec.handle("exec",r#"{"symbol":"BTCUSDT","command":"chase buy $100","reqId":"chase"}"#,&cx).unwrap();
        assert_eq!(book.active_chasers().len(),1);
        assert_eq!(book.active_chasers()[0].origin.as_deref(),Some("cli"));
        exec.handle("exec",r#"{"symbol":"BTCUSDT","command":"chase cancel keep"}"#,&cx).unwrap();
        // The row goes with the completion, once the chase's final read has answered.
        for _ in 0..8 {tokio::task::yield_now().await;}
        assert!(book.active_chasers().is_empty());
        assert_eq!(port.commands.borrow().len(),1,"cancel keep does not cancel the resting order");
        book.shutdown("test cleanup").await;cx.scope.dispose();
    }).await;
}

#[tokio::test(start_paused = true)]
async fn cli_twap_preserves_owner_change_outcomes_before_and_after_dispatch() {
    use safe_cex_algo_twap::{TwapOwner, factory::TwapFactory};
    use safe_cex_chaser::ChaserBook;
    use safe_cex_engine::model::{Limits, Market, MinMax, Precision, Ticker};
    LocalSet::new().run_until(async {
        for (change, after_dispatch) in [(false, false), (true, false), (true, true)] {
            let port = Rc::new(Port::default());
            port.store.borrow_mut().markets.insert("BTCUSDT".into(), Market {
                id:"BTCUSDT".into(), symbol:"BTCUSDT".into(), pretty_symbol:None, base:"BTC".into(), quote:"USDT".into(), active:true,
                precision:Precision{amount:0.001,price:0.1}, limits:Limits{amount:MinMax{min:0.001,max:1000.},leverage:MinMax{min:1.,max:20.},price:None},
                contract_size:None,is_expiring:None,is_inverse:None,is_spot:None,is_perpetual:None,is_read_only:None,taker_fee:None,maker_fee:None,
            });
            port.store.borrow_mut().tickers.insert("BTCUSDT".into(),Ticker{symbol:"BTCUSDT".into(),last:50000.,bid:49999.,ask:50001.,..Default::default()});
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let book = ChaserBook::new(cx.clone());
            let reads = Rc::new(Cell::new(0));
            let twap = TwapFactory::with_owner("acct", TwapOwner{owner_id:"A".into(),generation:7}, cx.clone(), Rc::new(book.clone()))
                .guard_owner(Rc::new({
                    let port=port.clone(); let reads=reads.clone();
                    move || {
                        reads.set(reads.get()+1);
                        if change && if after_dispatch { !port.commands.borrow().is_empty() } else { reads.get()>=4 } {
                            TwapOwner{owner_id:"B".into(),generation:8}
                        } else { TwapOwner{owner_id:"A".into(),generation:7} }
                    }
                }));
            let exec = ExecFactory::with_algos("acct".into(),book,twap);
            exec.handle("exec",r#"{"symbol":"BTCUSDT","command":"twap buy $1000 into 2 over 1","reqId":"typed-start"}"#,&cx).unwrap();
            settle_exec().await;
            let events=port.events.borrow();
            let results:Vec<_>=events.iter().filter(|(op,e)|op=="exec-result" && e["reqId"]=="typed-start").collect();
            assert_eq!(results.len(),1);
            let result=&results[0].1;
            assert_eq!(result["success"],!change);
            if change {
                assert_eq!(result["errorDetails"]["outcome"],if after_dispatch { "unknown" } else { "notSent" });
                assert!(!events.iter().any(|(op,e)|op=="exec-log" && e.to_string().contains("Starting TWAP")));
            } else {
                assert!(events.iter().any(|(op,e)|op=="exec-log" && e.to_string().contains("Starting TWAP")));
            }
            assert_eq!(port.commands.borrow().len(),usize::from(!change || after_dispatch));
            cx.scope.dispose();
        }
    }).await;
}

async fn settle_exec() {
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
}
#[tokio::test(start_paused = true)]
async fn shared_sleep_blocks_sequential_successor_and_scope_dispose_prevents_any_later_write() {
    LocalSet::new().run_until(async {
        let port=Rc::new(Port::default());let cx=AlgoCx::new(port.clone(),Scope::new());let factory=ExecFactory::default();
        factory.handle("exec",r#"{"symbol":"BTCUSDT","command":"sleep 250ms; lev 3","reqId":"r"}"#,&cx).unwrap();settle_exec().await;
        assert!(port.commands.borrow().is_empty());tokio::time::advance(std::time::Duration::from_millis(249)).await;settle_exec().await;assert!(port.commands.borrow().is_empty());
        tokio::time::advance(std::time::Duration::from_millis(1)).await;settle_exec().await;assert_eq!(port.commands.borrow().len(),1);
        assert!(matches!(&port.commands.borrow()[0],Command::SetLeverage{leverage,..} if *leverage==3.));
        port.replies.borrow_mut().remove(0).send(Ok("[]".into())).unwrap();settle_exec().await;
        factory.handle("exec",r#"{"symbol":"BTCUSDT","command":"sleep 1s; lev 7","reqId":"gone"}"#,&cx).unwrap();settle_exec().await;cx.scope.dispose();
        tokio::time::advance(std::time::Duration::from_secs(2)).await;settle_exec().await;
        assert_eq!(port.commands.borrow().len(),1);assert!(!port.events.borrow().iter().any(|(op,e)|op=="exec-result"&&e["reqId"]=="gone"));
    }).await;
}
#[tokio::test(start_paused = true)]
async fn actual_wait_reads_current_store_resets_sustain_and_times_out_without_venue_io() {
    use safe_cex_engine::model::Position;
    LocalSet::new().run_until(async {
        let port=Rc::new(Port::default());let cx=AlgoCx::new(port.clone(),Scope::new());let factory=ExecFactory::default();
        factory.handle("exec",r#"{"symbol":"BTCUSDT","command":"wait long for=200ms timeout=1s; lev 4","reqId":"sustain"}"#,&cx).unwrap();settle_exec().await;
        let position=Position{id:"position".into(),symbol:"BTCUSDT".into(),side:safe_cex_engine::model::PositionSide::Long,contracts:1.,entry_price:50000.,notional:50000.,leverage:2.,margin_type:None,position_mode:None,unrealized_pnl:0.,realized_pnl:None,liquidation_price:0.,margin_size:None,last_update:None};
        port.store.borrow_mut().positions=vec![position.clone()];tokio::time::advance(std::time::Duration::from_millis(100)).await;settle_exec().await;
        port.store.borrow_mut().positions.clear();tokio::time::advance(std::time::Duration::from_millis(100)).await;settle_exec().await;
        port.store.borrow_mut().positions=vec![position];
        for _ in 0..2 {tokio::time::advance(std::time::Duration::from_millis(100)).await;settle_exec().await;}
        assert!(port.commands.borrow().is_empty(),"false poll resets sustained interval");
        tokio::time::advance(std::time::Duration::from_millis(100)).await;settle_exec().await;assert_eq!(port.commands.borrow().len(),1);
        port.replies.borrow_mut().remove(0).send(Ok("[]".into())).unwrap();settle_exec().await;
        factory.handle("exec",r#"{"symbol":"BTCUSDT","command":"wait short timeout=200ms","reqId":"timeout"}"#,&cx).unwrap();settle_exec().await;
        for _ in 0..2 {tokio::time::advance(std::time::Duration::from_millis(100)).await;settle_exec().await;}
        assert!(port.events.borrow().iter().any(|(op,e)|op=="exec-result"&&e["reqId"]=="timeout"&&e["success"]==false&&e["error"]=="chain stopped: wait did not complete: wait timed out (200ms): short"));
        assert_eq!(port.commands.borrow().len(),1,"waiting never requests a venue read");cx.scope.dispose();
    }).await;
}

#[tokio::test(start_paused = true)]
async fn a_failed_wait_is_a_barrier_and_never_releases_the_following_buy() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = ExecFactory::default();
            for (id, command) in [
                ("timeout", "wait position timeout=100ms; buy $1000"),
                ("invalid", "sleep 100m; lev 10"),
            ] {
                factory
                    .handle(
                        "exec",
                        &json!({"symbol":"BTCUSDT","command":command,"reqId":id}).to_string(),
                        &cx,
                    )
                    .unwrap();
                settle_exec().await;
            }
            tokio::time::advance(std::time::Duration::from_millis(100)).await;
            settle_exec().await;
            assert!(
                port.commands.borrow().is_empty(),
                "failed sleep/wait must not issue successors"
            );
            for id in ["timeout", "invalid"] {
                assert!(
                    port.events.borrow().iter().any(|(op, e)| {
                        op == "exec-result"
                            && e["reqId"] == id
                            && e["success"] == false
                            && e["error"].as_str().is_some_and(|s| {
                                s.starts_with("chain stopped: wait did not complete")
                            })
                    }),
                    "{id}: {:?}",
                    port.events.borrow()
                );
            }
            cx.scope.dispose();
        })
        .await;
}

fn wait_market(port: &Port) {
    use safe_cex_engine::model::{Limits, Market, MinMax, Precision, Ticker};
    port.store.borrow_mut().markets.insert(
        "BTCUSDT".into(),
        Market {
            id: "BTCUSDT".into(),
            symbol: "BTCUSDT".into(),
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
                    max: 1000.,
                },
                leverage: MinMax { min: 1., max: 20. },
                price: None,
            },
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
    port.store.borrow_mut().tickers.insert(
        "BTCUSDT".into(),
        Ticker {
            symbol: "BTCUSDT".into(),
            last: 50000.,
            bid: 49999.,
            ask: 50001.,
            ..Default::default()
        },
    );
}
#[tokio::test(start_paused = true)]
async fn a_successful_wait_releases_one_buy_and_an_unsatisfied_wait_releases_none() {
    LocalSet::new().run_until(async {
        let port=Rc::new(Port::default());wait_market(&port);let cx=AlgoCx::new(port.clone(),Scope::new());let factory=ExecFactory::default();
        factory.handle("exec",r#"{"symbol":"BTCUSDT","command":"wait price > 99999 timeout=100ms; buy $1000","reqId":"false"}"#,&cx).unwrap();settle_exec().await;
        tokio::time::advance(std::time::Duration::from_millis(100)).await;settle_exec().await;assert!(port.commands.borrow().is_empty());
        factory.handle("exec",r#"{"symbol":"BTCUSDT","command":"wait price > 50000 timeout=100ms; buy $1000","reqId":"true"}"#,&cx).unwrap();settle_exec().await;
        assert_eq!(port.commands.borrow().len(),1);
        assert!(matches!(&port.commands.borrow()[0],Command::PlaceOrders{orders,..} if orders.len()==1 && orders[0].amount==0.02));
        port.replies.borrow_mut().remove(0).send(Ok(r#"["gated-native"]"#.into())).unwrap();settle_exec().await;
        assert!(port.events.borrow().iter().any(|(op,e)|op=="exec-result"&&e["reqId"]=="true"&&e["data"]["orderIds"]==json!(["gated-native"])));
        cx.scope.dispose();
    }).await;
}

fn native_position(symbol: &str) -> safe_cex_engine::model::Position {
    use safe_cex_engine::model::{Position, PositionSide};
    Position {
        id: symbol.into(),
        symbol: symbol.into(),
        side: PositionSide::Long,
        contracts: 0.02,
        entry_price: 50000.,
        notional: 1000.,
        leverage: 2.,
        margin_type: None,
        position_mode: None,
        unrealized_pnl: 0.,
        realized_pnl: None,
        liquidation_price: 0.,
        margin_size: None,
        last_update: None,
    }
}
fn native_order(symbol: &str) -> safe_cex_engine::model::Order {
    use safe_cex_engine::model::{Order, OrderSide, OrderStatus, OrderType};
    Order {
        id: format!("{symbol}-order"),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: symbol.into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        price: 49000.,
        amount: 0.01,
        filled: 0.,
        remaining: 0.01,
        reduce_only: false,
        close: None,
        client_order_id: None,
        time_in_force: None,
        last_update: 0.,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    }
}
#[tokio::test]
async fn be_does_not_cancel_a_stop_the_venue_already_ended() {
    use safe_cex_engine::model::{OrderSide, OrderStatus, OrderType};
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            wait_market(&port);
            let stop = |id: &str, status| safe_cex_engine::model::Order {
                id: id.into(),
                status,
                kind: OrderType::StopLoss,
                side: OrderSide::Sell,
                ..native_order("BTCUSDT")
            };
            port.store.borrow_mut().venue = "bybit".into();
            port.store.borrow_mut().instrument_classifications = Some(Default::default());
            port.store.borrow_mut().tickers.get_mut("BTCUSDT").unwrap().mark = 50100.;
            port.store.borrow_mut().positions = vec![native_position("BTCUSDT")];
            port.store.borrow_mut().orders =
                vec![stop("ended", OrderStatus::Canceled), stop("fired", OrderStatus::Closed)];
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = ExecFactory::default();
            factory
                .handle("exec", r#"{"symbol":"BTCUSDT","command":"be","reqId":"be"}"#, &cx)
                .unwrap();
            settle_exec().await;
            assert!(matches!(&port.commands.borrow()[0], Command::PlaceOrders { .. }));

            port.store.borrow_mut().orders =
                vec![stop("ended", OrderStatus::Canceled), stop("live", OrderStatus::Open)];
            port.replies.borrow_mut().remove(0).send(Ok(r#"["placed"]"#.into())).unwrap();
            settle_exec().await;
            factory
                .handle("exec", r#"{"symbol":"BTCUSDT","command":"be","reqId":"again"}"#, &cx)
                .unwrap();
            settle_exec().await;
            assert!(matches!(&port.commands.borrow()[1], Command::CancelOrders { ids, .. } if ids == &["live"]));
            cx.scope.dispose();
        })
        .await;
}
#[tokio::test(start_paused = true)]
async fn nuke_cancels_account_symbols_then_retries_only_the_failed_close_and_names_it() {
    LocalSet::new().run_until(async {
        let port=Rc::new(Port::default());wait_market(&port);
        let mut eth=port.store.borrow().markets["BTCUSDT"].clone();eth.symbol="ETHUSDT".into();eth.id="ETHUSDT".into();eth.base="ETH".into();port.store.borrow_mut().markets.insert("ETHUSDT".into(),eth);
        port.store.borrow_mut().positions=vec![native_position("BTCUSDT"),native_position("ETHUSDT")];
        port.store.borrow_mut().orders=vec![native_order("BTCUSDT"),native_order("ETHUSDT")];
        let cx=AlgoCx::new(port.clone(),Scope::new());let factory=ExecFactory::default();
        factory.handle("exec",r#"{"symbol":"BTCUSDT","command":"nuke","reqId":"nuke"}"#,&cx).unwrap();settle_exec().await;
        factory.handle("exec",r#"{"symbol":"BTCUSDT","command":"nuke","reqId":"duplicate"}"#,&cx).unwrap();settle_exec().await;assert_eq!(port.commands.borrow().len(),1);
        assert!(port.events.borrow().iter().any(|(op,e)|op=="exec-result"&&e["reqId"]=="duplicate"&&e["errorDetails"]["outcome"]=="notSent"));
        assert!(matches!(&port.commands.borrow()[0],Command::CancelSymbolOrders{symbol,..} if symbol=="BTCUSDT"));
        port.replies.borrow_mut().remove(0).send(Ok("null".into())).unwrap();settle_exec().await;
        assert!(matches!(&port.commands.borrow()[1],Command::CancelSymbolOrders{symbol,..} if symbol=="ETHUSDT"));
        port.replies.borrow_mut().remove(0).send(Ok("null".into())).unwrap();settle_exec().await;
        assert!(matches!(&port.commands.borrow()[2],Command::PlaceOrders{orders,..} if orders[0].symbol=="BTCUSDT"&&orders[0].reduce_only==Some(true)&&orders[0].amount==0.02));
        port.replies.borrow_mut().remove(0).send(Err(VenueError::new("lost close").outcome(Outcome::Unknown))).unwrap();settle_exec().await;
        assert!(matches!(&port.commands.borrow()[3],Command::PlaceOrders{orders,..} if orders[0].symbol=="ETHUSDT"&&orders[0].reduce_only==Some(true)&&orders[0].amount==0.02));
        port.replies.borrow_mut().remove(0).send(Ok(r#"["real-eth-close"]"#.into())).unwrap();settle_exec().await;
        // ETH closed but the store still shows it: only BTC, reduce-only, is sent again.
        for attempt in 4..7 {
            tokio::time::sleep(std::time::Duration::from_millis(101)).await;settle_exec().await;
            assert!(matches!(&port.commands.borrow()[attempt],Command::PlaceOrders{orders,..} if orders[0].symbol=="BTCUSDT"&&orders[0].reduce_only==Some(true)));
            port.replies.borrow_mut().remove(0).send(Err(VenueError::new("lost close").outcome(Outcome::Unknown))).unwrap();settle_exec().await;
        }
        tokio::time::sleep(std::time::Duration::from_millis(101)).await;settle_exec().await;
        let events=port.events.borrow();let result=&events.iter().find(|(op,e)|op=="exec-result"&&e["reqId"]=="nuke").unwrap().1;
        assert_eq!(result["success"],false);assert_eq!(result["errorDetails"]["outcome"],"unknown");assert_eq!(result["errorDetails"]["failedOperationCount"],1);assert_eq!(result["errorDetails"]["partialOrderIds"],json!(["real-eth-close"]));
        assert_eq!(result["errorDetails"]["failedLegs"][0]["symbol"],"BTCUSDT");
        assert!(result["error"].as_str().unwrap().contains("BTCUSDT close: lost close"));
        assert!(result["error"].as_str().unwrap().contains("3 operation(s), 1 failed"));assert_eq!(port.commands.borrow().len(),7);cx.scope.dispose();
    }).await;
}
#[tokio::test]
async fn nuke_abort_returns_its_account_guard_without_resend_and_modes_do_not_widen() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            wait_market(&port);
            port.store.borrow_mut().positions = vec![native_position("BTCUSDT")];
            port.store.borrow_mut().orders = vec![native_order("BTCUSDT")];
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = ExecFactory::default();
            factory
                .handle(
                    "exec",
                    r#"{"symbol":"BTCUSDT","command":"nuke pos","reqId":"gone"}"#,
                    &cx,
                )
                .unwrap();
            settle_exec().await;
            cx.scope.dispose();
            settle_exec().await;
            assert!(
                port.replies
                    .borrow_mut()
                    .remove(0)
                    .send(Ok("[]".into()))
                    .is_err()
            );
            let next = AlgoCx::new(port.clone(), Scope::new());
            factory
                .handle(
                    "exec",
                    r#"{"symbol":"BTCUSDT","command":"nuke ord","reqId":"orders"}"#,
                    &next,
                )
                .unwrap();
            settle_exec().await;
            assert_eq!(port.commands.borrow().len(), 2);
            assert!(matches!(
                &port.commands.borrow()[1],
                Command::CancelSymbolOrders { .. }
            ));
            port.replies
                .borrow_mut()
                .remove(0)
                .send(Ok("null".into()))
                .unwrap();
            settle_exec().await;
            assert!(
                port.events
                    .borrow()
                    .iter()
                    .any(|(op, e)| op == "exec-result"
                        && e["reqId"] == "orders"
                        && e["success"] == true)
            );
            assert!(
                !port
                    .events
                    .borrow()
                    .iter()
                    .any(|(op, e)| op == "exec-result" && e["reqId"] == "gone")
            );
            next.scope.dispose();
        })
        .await;
}
#[tokio::test(start_paused = true)]
async fn task_list_and_kill_use_injected_chase_and_twap_books_with_ts_ordering() {
    use safe_cex_algo_twap::factory::TwapFactory;
    use safe_cex_chaser::ChaserBook;
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            wait_market(&port);
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let book = ChaserBook::new(cx.clone());
            let twap = TwapFactory::new("a", cx.clone(), Rc::new(book.clone()));
            let factory = ExecFactory::with_algos("a".into(), book.clone(), twap.clone());
            for command in ["chase buy $100", "twap buy $1000 into 2 over 1", "tasks"] {
                factory
                    .handle(
                        "exec",
                        &json!({"symbol":"BTCUSDT","command":command,"reqId":command}).to_string(),
                        &cx,
                    )
                    .unwrap();
            }
            let log = port
                .events
                .borrow()
                .iter()
                .rev()
                .find(|(op, e)| op == "exec-log" && e["reqId"] == "tasks")
                .unwrap()
                .1["message"]
                .as_str()
                .unwrap()
                .to_string();
            assert!(log.find("chase").unwrap() < log.find("twap").unwrap());
            assert!(log.contains("active: 2"));
            factory
                .handle(
                    "exec",
                    r#"{"symbol":"BTCUSDT","command":"kill","reqId":"kill"}"#,
                    &cx,
                )
                .unwrap();
            assert!(twap.active_twaps("a").is_empty());
            assert_eq!(
                book.active_chasers().len(),
                1,
                "bare kill selects last TWAP row"
            );
            factory
                .handle(
                    "exec",
                    r#"{"symbol":"BTCUSDT","command":"terminate all","reqId":"all"}"#,
                    &cx,
                )
                .unwrap();
            assert!(book.active_chasers().iter().all(|c| c.is_stopped));
            book.shutdown("test cleanup").await;
            cx.scope.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn nuke_close_retries_a_transient_failure_and_reports_success() {
    LocalSet::new().run_until(async {
        let port=Rc::new(Port::default());wait_market(&port);
        port.store.borrow_mut().positions=vec![native_position("BTCUSDT")];
        let cx=AlgoCx::new(port.clone(),Scope::new());let factory=ExecFactory::default();
        factory.handle("exec",r#"{"symbol":"BTCUSDT","command":"nuke pos","reqId":"r"}"#,&cx).unwrap();settle_exec().await;
        port.replies.borrow_mut().remove(0).send(Err(VenueError::new("busy"))).unwrap();settle_exec().await;
        tokio::time::sleep(std::time::Duration::from_millis(101)).await;settle_exec().await;
        assert!(matches!(&port.commands.borrow()[1],Command::PlaceOrders{orders,..} if orders[0].reduce_only==Some(true)));
        // The store still lags the accepted close: no further send and no failure.
        port.replies.borrow_mut().remove(0).send(Ok(r#"["close"]"#.into())).unwrap();settle_exec().await;
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;settle_exec().await;
        assert_eq!(port.commands.borrow().len(),2);
        let events=port.events.borrow();let result=&events.iter().find(|(op,e)|op=="exec-result"&&e["reqId"]=="r").unwrap().1;
        assert_eq!(result["success"],true,"{result}");cx.scope.dispose();
    }).await;
}
#[tokio::test(start_paused = true)]
async fn nuke_close_does_not_retry_a_position_already_gone() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            wait_market(&port);
            port.store.borrow_mut().positions = vec![native_position("BTCUSDT")];
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = ExecFactory::default();
            factory
                .handle(
                    "exec",
                    r#"{"symbol":"BTCUSDT","command":"nuke pos","reqId":"r"}"#,
                    &cx,
                )
                .unwrap();
            settle_exec().await;
            port.store.borrow_mut().positions = Vec::new();
            port.replies
                .borrow_mut()
                .remove(0)
                .send(Err(VenueError::new("answer lost").outcome(Outcome::Unknown)))
                .unwrap();
            settle_exec().await;
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            settle_exec().await;
            assert_eq!(port.commands.borrow().len(), 1);
            let events = port.events.borrow();
            let result = &events
                .iter()
                .find(|(op, e)| op == "exec-result" && e["reqId"] == "r")
                .unwrap()
                .1;
            assert_eq!(result["success"], true, "{result}");
            cx.scope.dispose();
        })
        .await;
}
#[tokio::test]
async fn nuke_cancel_attempts_every_symbol_after_an_unknown_second_cancel() {
    LocalSet::new().run_until(async {
        let port=Rc::new(Port::default());port.store.borrow_mut().orders=vec![native_order("AAA"),native_order("BBB"),native_order("CCC")];
        let cx=AlgoCx::new(port.clone(),Scope::new());let factory=ExecFactory::default();
        factory.handle("exec",r#"{"symbol":"AAA","command":"nuke ord","reqId":"r"}"#,&cx).unwrap();settle_exec().await;
        port.replies.borrow_mut().remove(0).send(Ok("null".into())).unwrap();settle_exec().await;
        port.replies.borrow_mut().remove(0).send(Err(VenueError::new("lost cancel").outcome(Outcome::Unknown))).unwrap();settle_exec().await;
        assert_eq!(port.commands.borrow().len(),3,"third symbol must be attempted after second Unknown");
        assert!(matches!(&port.commands.borrow()[2],Command::CancelSymbolOrders{symbol,..} if symbol=="CCC"));
        port.replies.borrow_mut().remove(0).send(Ok("null".into())).unwrap();settle_exec().await;
        // The closing cancel-all sweep reaches the symbol whose cancel was lost.
        assert!(matches!(&port.commands.borrow()[3],Command::CancelSymbolOrders{symbol,..} if symbol=="BBB"));
        port.replies.borrow_mut().remove(0).send(Ok("null".into())).unwrap();settle_exec().await;
        let events=port.events.borrow();let result=&events.iter().find(|(op,e)|op=="exec-result"&&e["reqId"]=="r").unwrap().1;
        assert_eq!(result["errorDetails"]["outcome"],"unknown");assert_eq!(result["errorDetails"]["cancelledSymbolCount"],2);assert_eq!(result["errorDetails"]["failedLegs"][0]["symbol"],"BBB");assert!(result["error"].as_str().unwrap().contains("BBB cancel: lost cancel"));cx.scope.dispose();
    }).await;
}
#[tokio::test]
async fn nuke_cancels_first_and_reads_positions_again_to_close_fills_during_cancellation() {
    LocalSet::new().run_until(async {
        let port=Rc::new(Port::default());wait_market(&port);port.store.borrow_mut().orders=vec![native_order("BTCUSDT")];port.store.borrow_mut().positions=vec![native_position("BTCUSDT")];
        let cx=AlgoCx::new(port.clone(),Scope::new());let factory=ExecFactory::default();factory.handle("exec",r#"{"symbol":"BTCUSDT","command":"nuke all","reqId":"r"}"#,&cx).unwrap();settle_exec().await;
        assert!(matches!(&port.commands.borrow()[0],Command::CancelSymbolOrders{symbol,..} if symbol=="BTCUSDT"),"cancel resting exposure before any market close");
        port.store.borrow_mut().positions[0].contracts=0.03;port.store.borrow_mut().positions[0].notional=1500.;
        port.replies.borrow_mut().remove(0).send(Ok("null".into())).unwrap();settle_exec().await;
        assert!(matches!(&port.commands.borrow()[1],Command::PlaceOrders{orders,..} if orders[0].amount==0.03 && orders[0].reduce_only==Some(true)),"fresh position includes the fill during cancel");
        port.replies.borrow_mut().remove(0).send(Ok(r#"["fresh-close"]"#.into())).unwrap();settle_exec().await;assert_eq!(port.commands.borrow().len(),2);cx.scope.dispose();
    }).await;
}

fn is_fresh_read(command: &Command) -> bool {
    matches!(
        command,
        Command::Venue {
            action: safe_cex_engine::VenueAction::ReadPositions,
            ..
        }
    )
}

/// A `nuke pos` on a venue with the read, one position open, its close accepted.
async fn nuke_closed_once(port: &Rc<Port>, cx: &AlgoCx) {
    wait_market(port);
    port.fresh_positions.set(true);
    port.store.borrow_mut().positions = vec![native_position("BTCUSDT")];
    ExecFactory::default()
        .handle(
            "exec",
            r#"{"symbol":"BTCUSDT","command":"nuke pos","reqId":"r"}"#,
            cx,
        )
        .unwrap();
    settle_exec().await;
    assert!(matches!(
        &port.commands.borrow()[0],
        Command::PlaceOrders { .. }
    ));
    port.replies
        .borrow_mut()
        .remove(0)
        .send(Ok(r#"["close"]"#.into()))
        .unwrap();
    settle_exec().await;
}

const FLAT_ANSWER: &str = r#"{"skipped":[],"open":[]}"#;

fn open_answer(symbol: &str, contracts: f64) -> String {
    json!({"skipped":[],"open":[{"symbol":symbol,"side":"long","contracts":contracts}]}).to_string()
}

fn nuke_result(port: &Port) -> Value {
    port.events
        .borrow()
        .iter()
        .find(|(op, e)| op == "exec-result" && e["reqId"] == "r")
        .expect("the nuke reported")
        .1
        .clone()
}

#[tokio::test(start_paused = true)]
async fn nuke_sends_a_second_close_only_after_a_fresh_read_sized_from_it() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            let cx = AlgoCx::new(port.clone(), Scope::new());
            nuke_closed_once(&port, &cx).await;
            // The store still shows the position, so the read runs when the settle ends.
            assert_eq!(port.commands.borrow().len(), 1);
            tokio::time::sleep(std::time::Duration::from_millis(2_200)).await;
            settle_exec().await;
            assert!(is_fresh_read(&port.commands.borrow()[1]));
            assert_eq!(
                port.commands.borrow().len(),
                2,
                "no close before the answer"
            );
            port.store.borrow_mut().positions[0].contracts = 0.01;
            port.replies
                .borrow_mut()
                .remove(0)
                .send(Ok(open_answer("BTCUSDT", 0.01)))
                .unwrap();
            settle_exec().await;
            assert!(matches!(
                &port.commands.borrow()[2],
                Command::PlaceOrders { orders, .. }
                    if orders[0].amount == 0.01 && orders[0].reduce_only == Some(true)
            ));
            port.store.borrow_mut().positions.clear();
            port.replies
                .borrow_mut()
                .remove(0)
                .send(Ok(r#"["close-2"]"#.into()))
                .unwrap();
            settle_exec().await;
            tokio::time::sleep(std::time::Duration::from_millis(600)).await;
            settle_exec().await;
            assert!(is_fresh_read(&port.commands.borrow()[3]));
            port.replies
                .borrow_mut()
                .remove(0)
                .send(Ok(FLAT_ANSWER.into()))
                .unwrap();
            settle_exec().await;
            assert_eq!(nuke_result(&port)["success"], true);
            assert_eq!(port.commands.borrow().len(), 4);
            cx.scope.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn nuke_sends_no_second_close_when_the_read_fails_and_names_the_position() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            let cx = AlgoCx::new(port.clone(), Scope::new());
            nuke_closed_once(&port, &cx).await;
            tokio::time::sleep(std::time::Duration::from_millis(2_200)).await;
            settle_exec().await;
            assert!(is_fresh_read(&port.commands.borrow()[1]));
            port.replies
                .borrow_mut()
                .remove(0)
                .send(Err(VenueError::new("read timed out")))
                .unwrap();
            settle_exec().await;
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            settle_exec().await;
            assert_eq!(port.commands.borrow().len(), 2, "no further close");
            let result = nuke_result(&port);
            assert_eq!(result["success"], false);
            let error = result["error"].as_str().unwrap();
            assert!(
                error.contains("could not confirm the account is flat"),
                "{error}"
            );
            assert!(error.contains("BTCUSDT"), "{error}");
            assert!(error.contains("read timed out"), "{error}");
            cx.scope.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn nuke_reports_not_confirmed_when_the_read_skipped_a_row() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            let cx = AlgoCx::new(port.clone(), Scope::new());
            nuke_closed_once(&port, &cx).await;
            port.store.borrow_mut().positions.clear();
            tokio::time::sleep(std::time::Duration::from_millis(600)).await;
            settle_exec().await;
            assert!(is_fresh_read(&port.commands.borrow()[1]));
            port.replies
                .borrow_mut()
                .remove(0)
                .send(Ok(r#"{"skipped":["DOGEUSD_PERP"],"open":[]}"#.into()))
                .unwrap();
            settle_exec().await;
            let result = nuke_result(&port);
            assert_eq!(result["success"], false);
            let error = result["error"].as_str().unwrap();
            assert!(
                error.contains("could not confirm the account is flat"),
                "{error}"
            );
            assert!(error.contains("DOGEUSD_PERP"), "{error}");
            assert_eq!(port.commands.borrow().len(), 2);
            cx.scope.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_read_under_the_settle_after_a_close_is_never_issued() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            let cx = AlgoCx::new(port.clone(), Scope::new());
            nuke_closed_once(&port, &cx).await;
            // The store already shows flat; only the settle holds the read back.
            port.store.borrow_mut().positions.clear();
            tokio::time::sleep(std::time::Duration::from_millis(450)).await;
            settle_exec().await;
            assert_eq!(port.commands.borrow().len(), 1, "no read inside the settle");
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            settle_exec().await;
            assert!(is_fresh_read(&port.commands.borrow()[1]));
            port.replies
                .borrow_mut()
                .remove(0)
                .send(Ok(FLAT_ANSWER.into()))
                .unwrap();
            settle_exec().await;
            assert_eq!(nuke_result(&port)["success"], true);
            cx.scope.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn nuke_stays_unconfirmed_when_the_read_is_long_though_the_store_is_flat() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            let cx = AlgoCx::new(port.clone(), Scope::new());
            nuke_closed_once(&port, &cx).await;
            port.store.borrow_mut().positions.clear();
            for _ in 0..4 {
                tokio::time::sleep(std::time::Duration::from_millis(600)).await;
                settle_exec().await;
                let last = port.commands.borrow().last().cloned();
                if last.as_ref().is_some_and(is_fresh_read) && !port.replies.borrow().is_empty() {
                    port.replies
                        .borrow_mut()
                        .remove(0)
                        .send(Ok(open_answer("BTCUSDT", 13.8)))
                        .unwrap();
                    settle_exec().await;
                }
            }
            let result = nuke_result(&port);
            assert_eq!(result["success"], false);
            let error = result["error"].as_str().unwrap();
            assert!(error.contains("did not leave the account flat"), "{error}");
            assert!(error.contains("BTCUSDT long 13.8"), "{error}");
            cx.scope.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn nuke_reads_with_nothing_in_the_store_and_closes_what_the_read_commits() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            let cx = AlgoCx::new(port.clone(), Scope::new());
            wait_market(&port);
            port.fresh_positions.set(true);
            ExecFactory::default()
                .handle(
                    "exec",
                    r#"{"symbol":"BTCUSDT","command":"nuke pos","reqId":"r"}"#,
                    &cx,
                )
                .unwrap();
            settle_exec().await;
            assert!(
                is_fresh_read(&port.commands.borrow()[0]),
                "read with no rows"
            );
            port.store.borrow_mut().positions = vec![native_position("BTCUSDT")];
            port.replies
                .borrow_mut()
                .remove(0)
                .send(Ok(open_answer("BTCUSDT", 0.02)))
                .unwrap();
            settle_exec().await;
            assert!(matches!(
                &port.commands.borrow()[1],
                Command::PlaceOrders { .. }
            ));
            port.store.borrow_mut().positions.clear();
            port.replies
                .borrow_mut()
                .remove(0)
                .send(Ok(r#"["close"]"#.into()))
                .unwrap();
            settle_exec().await;
            tokio::time::sleep(std::time::Duration::from_millis(600)).await;
            settle_exec().await;
            assert!(is_fresh_read(&port.commands.borrow()[2]));
            port.replies
                .borrow_mut()
                .remove(0)
                .send(Ok(FLAT_ANSWER.into()))
                .unwrap();
            settle_exec().await;
            assert_eq!(nuke_result(&port)["success"], true);
            cx.scope.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn an_old_array_answer_is_not_confirmed_flat() {
    for body in [
        "[]",
        "null",
        r#"{"skipped":[],"open":[{"symbol":"BTCUSDT"}]}"#,
    ] {
        LocalSet::new()
            .run_until(async {
                let port = Rc::new(Port::default());
                let cx = AlgoCx::new(port.clone(), Scope::new());
                nuke_closed_once(&port, &cx).await;
                port.store.borrow_mut().positions.clear();
                tokio::time::sleep(std::time::Duration::from_millis(600)).await;
                settle_exec().await;
                port.replies
                    .borrow_mut()
                    .remove(0)
                    .send(Ok(body.into()))
                    .unwrap();
                settle_exec().await;
                let result = nuke_result(&port);
                assert_eq!(result["success"], false, "{body}");
                let error = result["error"].as_str().unwrap();
                assert!(
                    error.contains("could not confirm the account is flat"),
                    "{body}: {error}"
                );
                assert_eq!(port.commands.borrow().len(), 2, "no further close: {body}");
                cx.scope.dispose();
            })
            .await;
    }
}

#[tokio::test(start_paused = true)]
async fn a_nuke_cancels_every_traded_symbol_before_any_close_with_no_order_in_the_store() {
    use safe_cex_exec::native::CancelAllScope;
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            wait_market(&port);
            port.fresh_positions.set(true);
            port.store.borrow_mut().positions = vec![native_position("BTCUSDT")];
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = ExecFactory::default()
                .with_cancel_all(Rc::new(|| CancelAllScope::Symbols(vec!["ETHUSDT".into()])));
            let nuke = factory.nuke("all", &cx).unwrap();
            let port_answers = port.clone();
            let driver = async move {
                loop {
                    settle_exec().await;
                    let next = port_answers.replies.borrow_mut().drain(..).next();
                    if let Some(reply) = next {
                        let last = port_answers.commands.borrow().last().cloned().unwrap();
                        let body = match last {
                            Command::PlaceOrders { .. } => {
                                port_answers.store.borrow_mut().positions.clear();
                                r#"["close"]"#
                            }
                            Command::Venue { .. } => FLAT_ANSWER,
                            _ => "null",
                        };
                        reply.send(Ok(body.into())).unwrap();
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
            };
            let result = tokio::select! { result = nuke => result, _ = driver => unreachable!() };
            assert!(result.is_ok(), "{result:?}");
            let commands = port.commands.borrow();
            let close = commands
                .iter()
                .position(|c| matches!(c, Command::PlaceOrders { .. }))
                .unwrap();
            let cancelled: Vec<&str> = commands[..close]
                .iter()
                .filter_map(|c| match c {
                    Command::CancelSymbolOrders { symbol, .. } => Some(symbol.as_str()),
                    _ => None,
                })
                .collect();
            assert_eq!(cancelled, ["BTCUSDT", "ETHUSDT"]);
            drop(commands);
            cx.scope.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_failed_cancel_is_reported_without_blocking_the_close() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            wait_market(&port);
            port.store.borrow_mut().positions = vec![native_position("BTCUSDT")];
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = ExecFactory::default();
            let nuke = factory.nuke("all", &cx).unwrap();
            let answers = port.clone();
            let driver = async move {
                loop {
                    settle_exec().await;
                    let next = answers.replies.borrow_mut().drain(..).next();
                    if let Some(reply) = next {
                        let last = answers.commands.borrow().last().cloned().unwrap();
                        let body = match last {
                            Command::CancelSymbolOrders { .. } => {
                                Err(VenueError::new("cancel down"))
                            }
                            _ => Ok(r#"["close"]"#.to_owned()),
                        };
                        if body.is_ok() {
                            answers.store.borrow_mut().positions.clear();
                        }
                        reply.send(body).unwrap();
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
            };
            let result = tokio::select! { result = nuke => result, _ = driver => unreachable!() };
            let error = result.unwrap_err();
            assert!(
                error["message"]
                    .as_str()
                    .unwrap()
                    .contains("BTCUSDT cancel: cancel down"),
                "{error}"
            );
            assert!(
                port.commands
                    .borrow()
                    .iter()
                    .any(|c| matches!(c, Command::PlaceOrders { .. }))
            );
            cx.scope.dispose();
        })
        .await;
}

fn is_account_cancel(command: &Command) -> bool {
    matches!(
        command,
        Command::Venue {
            action: safe_cex_engine::VenueAction::CancelAccountOrders,
            ..
        }
    )
}

/// A nuke on a store with one open position, every command answered as accepted.
async fn nuke_answered(port: &Rc<Port>, mode: &str) -> Result<Value, Value> {
    wait_market(port);
    port.store.borrow_mut().positions = vec![native_position("BTCUSDT")];
    let cx = AlgoCx::new(port.clone(), Scope::new());
    let factory = ExecFactory::default();
    let nuke = factory.nuke(mode, &cx).unwrap();
    let answers = port.clone();
    let driver = async move {
        loop {
            settle_exec().await;
            let next = answers.replies.borrow_mut().drain(..).next();
            if let Some(reply) = next {
                let last = answers.commands.borrow().last().cloned().unwrap();
                let body = match last {
                    Command::PlaceOrders { .. } => {
                        answers.store.borrow_mut().positions.clear();
                        Ok(r#"["close"]"#.to_owned())
                    }
                    _ if answers.fail_cancels.get()
                        && !matches!(last, Command::PlaceOrders { .. }) =>
                    {
                        Err(VenueError::new("cancel down"))
                    }
                    _ => Ok("null".to_owned()),
                };
                reply.send(body).unwrap();
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    };
    let result = tokio::select! { result = nuke => result, _ = driver => unreachable!() };
    cx.scope.dispose();
    result
}

#[tokio::test(start_paused = true)]
async fn a_nuke_sends_the_account_wide_cancel_before_any_close_where_the_venue_has_one() {
    LocalSet::new()
        .run_until(async {
            for mode in ["all", "orders"] {
                let port = Rc::new(Port::default());
                port.nuke_account_cancel.set(true);
                port.symbol_cancel_misses_unseen.set(true);
                let result = nuke_answered(&port, mode).await.unwrap();
                let commands = port.commands.borrow();
                let account = commands.iter().position(is_account_cancel).expect(mode);
                if let Some(close) = commands
                    .iter()
                    .position(|c| matches!(c, Command::PlaceOrders { .. }))
                {
                    assert!(account < close, "{mode}: {commands:?}");
                }
                assert!(result.get("warning").is_none(), "{mode}: {result}");
                assert!(
                    !result["summary"]
                        .as_str()
                        .unwrap()
                        .contains("last few seconds"),
                    "{mode}"
                );
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_nuke_sends_no_account_wide_cancel_on_a_venue_without_one() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            nuke_answered(&port, "all").await.unwrap();
            assert!(!port.commands.borrow().iter().any(is_account_cancel));
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_nuke_warns_of_unseen_orders_only_where_no_cancel_reaches_them() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            port.symbol_cancel_misses_unseen.set(true);
            let result = nuke_answered(&port, "all").await.unwrap();
            let summary = result["summary"].as_str().unwrap();
            assert!(
                summary.contains("Orders placed in the last few seconds may not be cancelled"),
                "{summary}"
            );
            assert!(summary.contains("check open orders"), "{summary}");
            assert_eq!(
                result["warning"].as_str().map(|w| summary.ends_with(w)),
                Some(true)
            );

            let failing = Rc::new(Port::default());
            failing.symbol_cancel_misses_unseen.set(true);
            failing.fail_cancels.set(true);
            let error = nuke_answered(&failing, "all").await.unwrap_err();
            let message = error["message"].as_str().unwrap();
            assert!(
                message.contains("may not be cancelled") && message.contains("check open orders"),
                "{message}"
            );

            let plain = Rc::new(Port::default());
            let result = nuke_answered(&plain, "all").await.unwrap();
            assert!(
                !result["summary"]
                    .as_str()
                    .unwrap()
                    .contains("last few seconds")
            );
            assert!(result.get("warning").is_none());
        })
        .await;
}

#[tokio::test]
async fn a_failed_account_wide_cancel_says_orders_may_remain() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            port.nuke_account_cancel.set(true);
            port.fail_cancels.set(true);
            let error = nuke_answered(&port, "all").await.unwrap_err();
            let message = error["message"].as_str().unwrap();
            assert!(
                message.contains("including triggers and TP/SL, may remain on this venue"),
                "{message}"
            );
        })
        .await;
}

#[tokio::test]
async fn cli_chart_uses_one_typed_engine_candle_query_and_its_correlated_result() {
    LocalSet::new().run_until(async {
        let port=Rc::new(Port::default());wait_market(&port);let cx=AlgoCx::new(port.clone(),Scope::new());let factory=ExecFactory::default();
        factory.handle("exec",r#"{"symbol":"BTCUSDT","command":"chart 5m 3","reqId":"chart"}"#,&cx).unwrap();settle_exec().await;
        assert!(matches!(&port.commands.borrow()[0],Command::FetchCandles{symbol,limit:Some(3),..} if symbol=="BTCUSDT"));
        port.replies.borrow_mut().remove(0).send(Ok(json!([{"timestamp":0,"open":49000,"high":51000,"low":48000,"close":50000}]).to_string())).unwrap();settle_exec().await;
        assert!(port.events.borrow().iter().any(|(op,e)|op=="exec-log"&&e["reqId"]=="chart"&&e["message"].as_str().is_some_and(|s|s.contains("BTCUSDT 5m (1 candles)"))));
        assert!(port.events.borrow().iter().any(|(op,e)|op=="exec-result"&&e["reqId"]=="chart"&&e["success"]==true));assert_eq!(port.commands.borrow().len(),1);
        factory.handle("exec",r#"{"symbol":"BTCUSDT","command":"chart","reqId":"bad"}"#,&cx).unwrap();settle_exec().await;port.replies.borrow_mut().remove(0).send(Ok("null".into())).unwrap();settle_exec().await;
        assert!(port.events.borrow().iter().any(|(op,e)|op=="exec-result"&&e["reqId"]=="bad"&&e["success"]==false&&e["error"]=="Invalid candle result"));assert_eq!(port.commands.borrow().len(),2);cx.scope.dispose();
    }).await;
}
#[tokio::test]
async fn chart_disposal_never_releases_the_following_trade_or_retries_the_read() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            wait_market(&port);
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = ExecFactory::default();
            factory
                .handle(
                    "exec",
                    r#"{"symbol":"BTCUSDT","command":"chart; buy $100","reqId":"gone"}"#,
                    &cx,
                )
                .unwrap();
            settle_exec().await;
            cx.scope.dispose();
            settle_exec().await;
            assert!(
                port.replies
                    .borrow_mut()
                    .remove(0)
                    .send(Ok("[]".into()))
                    .is_err()
            );
            assert_eq!(port.commands.borrow().len(), 1);
            assert!(
                !port
                    .events
                    .borrow()
                    .iter()
                    .any(|(op, e)| op == "exec-result" && e["reqId"] == "gone")
            );
        })
        .await;
}

/// Answers every command the port has sent and not yet answered: placements with
/// an id, everything else with `null`, until `run` settles.
async fn answering<T>(port: &Port, run: impl std::future::Future<Output = T>) -> T {
    let answered = Cell::new(0usize);
    tokio::pin!(run);
    loop {
        tokio::select! {
            out = &mut run => return out,
            _ = tokio::time::sleep(std::time::Duration::from_millis(1)) => {
                let replies: Vec<_> = port.replies.borrow_mut().drain(..).collect();
                for reply in replies {
                    let at = answered.get();
                    answered.set(at + 1);
                    let placed = matches!(port.commands.borrow()[at], Command::PlaceOrders { .. });
                    let _ = reply.send(Ok(if placed { format!(r#"["order-{at}"]"#) } else { "null".into() }));
                }
            }
        }
    }
}

#[tokio::test(start_paused = true)]
async fn a_nuke_stops_every_chaser_and_twap_first_and_reports_their_ids() {
    use safe_cex_algo_twap::factory::TwapFactory;
    use safe_cex_chaser::ChaserBook;
    use safe_cex_exec::native::CancelAllScope;
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            wait_market(&port);
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let book = ChaserBook::new(cx.clone());
            let twap = TwapFactory::new("a", cx.clone(), Rc::new(book.clone()));
            let factory = ExecFactory::with_algos("a".into(), book.clone(), twap.clone())
                .with_cancel_all(Rc::new(|| CancelAllScope::Symbols(vec!["ETHUSDT".into()])));
            for command in ["chase buy $100", "twap buy $1000 into 4 over 4"] {
                factory
                    .handle(
                        "exec",
                        &json!({"symbol":"BTCUSDT","command":command,"reqId":command}).to_string(),
                        &cx,
                    )
                    .unwrap();
            }
            answering(&port, tokio::time::sleep(std::time::Duration::from_millis(50))).await;
            let chasers: Vec<String> = book.active_chasers().iter().map(|c| c.id.clone()).collect();
            let twaps: Vec<Value> = twap.active_twaps("a").iter().map(|t| t["id"].clone()).collect();
            assert!(!chasers.is_empty() && twaps.len() == 1, "{chasers:?} {twaps:?}");

            let nuke = factory.nuke("orders", &cx).unwrap();
            let before = port.commands.borrow().len();
            let result = answering(&port, nuke).await.unwrap();
            let mut stopped: Vec<String> = serde_json::from_value(result["stoppedChasers"].clone()).unwrap();
            stopped.sort();
            let mut expected = chasers.clone();
            expected.sort();
            assert_eq!(stopped, expected);
            assert_eq!(result["stoppedTwaps"], Value::Array(twaps));
            assert!(book.active_chasers().iter().all(|c| c.is_stopped));
            assert!(twap.active_twaps("a").is_empty());
            // Nothing is placed once the nuke has started, and the TWAP never slices again.
            tokio::time::advance(std::time::Duration::from_secs(600)).await;
            answering(&port, tokio::time::sleep(std::time::Duration::from_millis(50))).await;
            let commands = port.commands.borrow().clone();
            assert!(
                commands[before..].iter().all(|c| !matches!(c, Command::PlaceOrders { .. })),
                "{:?}",
                &commands[before..]
            );
            // It ends with the cancel-all sweep, reaching the attempted-placement symbol.
            assert!(
                commands[before..].iter().any(|c| matches!(c, Command::CancelSymbolOrders { symbol, .. } if symbol == "ETHUSDT")),
                "{:?}",
                &commands[before..]
            );
            book.shutdown("test cleanup").await;
            cx.scope.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_nuke_ends_with_the_venue_cancel_all_on_a_native_venue() {
    use safe_cex_exec::native::CancelAllScope;
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            wait_market(&port);
            port.store.borrow_mut().orders = vec![native_order("BTCUSDT")];
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = ExecFactory::default().with_cancel_all(Rc::new(|| CancelAllScope::Native));
            let result = answering(&port, factory.nuke("all", &cx).unwrap()).await.unwrap();
            let commands = port.commands.borrow();
            assert!(
                matches!(&commands[0], Command::CancelSymbolOrders { symbol, .. } if symbol.is_empty()),
                "the first cancel is venue-wide: {commands:?}"
            );
            assert!(
                matches!(commands.last().unwrap(), Command::CancelSymbolOrders { symbol, .. } if symbol.is_empty()),
                "the last command is the venue-wide cancel-all: {commands:?}"
            );
            assert_eq!(result["stoppedChasers"], json!([]));
            drop(commands);
            cx.scope.dispose();
        })
        .await;
}

#[tokio::test]
async fn a_symbol_scoped_sweep_skips_symbols_the_nuke_already_cancelled() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            port.store.borrow_mut().orders = vec![native_order("AAA")];
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = ExecFactory::default();
            answering(&port, factory.nuke("orders", &cx).unwrap())
                .await
                .unwrap();
            assert_eq!(
                port.commands.borrow().len(),
                1,
                "{:?}",
                port.commands.borrow()
            );
            cx.scope.dispose();
        })
        .await;
}
