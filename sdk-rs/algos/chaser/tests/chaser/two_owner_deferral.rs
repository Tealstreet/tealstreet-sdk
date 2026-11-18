//! Owner selection lives in the host: a switch supplies a distinct account port/scope.
//! Reuse chase, client and venue order IDs so matching labels cannot isolate these tests.
use super::*;
use safe_cex_engine::engine::Query;
use tokio::sync::oneshot;

async fn bounded_local(test: impl std::future::Future<Output = ()>) {
    local(async {
        tokio::time::timeout(Duration::from_secs(600), test)
            .await
            .expect("owner lifecycle must settle within the paused-clock bound");
    })
    .await;
}

#[derive(Clone, Copy)]
enum Held {
    Placement,
    Cancel,
    Fills,
    Lookup,
}

impl Held {
    fn matches(self, command: &Command) -> bool {
        match self {
            Self::Placement => matches!(command, Command::PlaceOrders { .. }),
            Self::Cancel => is_cancel(command),
            Self::Fills => matches!(
                command,
                Command::Query {
                    query: Query::Fills { .. },
                    ..
                }
            ),
            Self::Lookup => matches!(
                command,
                Command::Query {
                    query: Query::OrderByClientId { .. },
                    ..
                }
            ),
        }
    }
}

struct DeferredPort {
    port: Rc<FakePort>,
    next: RefCell<Option<(Held, oneshot::Receiver<()>)>>,
    waiting: Rc<Cell<bool>>,
    resumed: Rc<Cell<bool>>,
}

impl DeferredPort {
    fn new(port: Rc<FakePort>) -> Rc<Self> {
        Rc::new(Self {
            port,
            next: RefCell::new(None),
            waiting: Rc::new(Cell::new(false)),
            resumed: Rc::new(Cell::new(false)),
        })
    }

    fn hold(&self, kind: Held) -> oneshot::Sender<()> {
        assert!(self.next.borrow().is_none());
        let (send, receive) = oneshot::channel();
        *self.next.borrow_mut() = Some((kind, receive));
        send
    }
}

impl AccountPort for DeferredPort {
    fn store(&self) -> Ref<'_, AlgoStore> {
        self.port.store()
    }

    fn subscribe(&self) -> mpsc::UnboundedReceiver<Change> {
        self.port.subscribe()
    }

    fn next_command_id(&self) -> CommandId {
        self.port.next_command_id()
    }

    fn command(&self, command: Command) -> CommandFuture {
        let gate = {
            let mut next = self.next.borrow_mut();
            if next
                .as_ref()
                .is_some_and(|(kind, _)| kind.matches(&command))
            {
                next.take().map(|(_, receive)| receive)
            } else {
                None
            }
        };
        let answer = self.port.command(command);
        let waiting = self.waiting.clone();
        let resumed = self.resumed.clone();
        Box::pin(async move {
            if let Some(gate) = gate {
                waiting.set(true);
                gate.await.expect("test releases its held command reply");
                resumed.set(true);
            }
            answer.await
        })
    }

    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        self.port.capabilities()
    }

    fn emit(&self, op: &str, data: String) {
        self.port.emit(op, data);
    }
}

fn start_owner(port: Rc<dyn AccountPort>) -> (ChaserFactory, AlgoCx) {
    let cx = AlgoCx::new(port, Scope::new());
    let factory = ChaserFactory::new(cx.clone());
    factory
        .book()
        .start_with_id(
            "shared-chase".into(),
            serde_json::from_str(BUY_ONE).unwrap(),
        )
        .unwrap();
    (factory, cx)
}

fn select_b() -> (Rc<FakePort>, ChaserFactory, AlgoCx) {
    let port = FakePort::new("bybit");
    port.accept_placements();
    port.set_ticker(200.0, 200.5);
    let (factory, cx) = start_owner(port.clone());
    (port, factory, cx)
}

async fn retire_a(factory: &ChaserFactory, cx: &AlgoCx) {
    factory.book().shutdown("owner A retired").await;
    cx.scope.dispose();
    tokio::task::yield_now().await;
}

async fn b_still_works(port: &Rc<FakePort>, factory: &ChaserFactory, cx: &AlgoCx) {
    settle(1100).await;
    assert_eq!(port.placements(), vec![(200.0, 1.0)]);
    assert!(port.cancels().is_empty());
    assert!(port.emitted.borrow().iter().all(|(op, _)| op == "log"));
    assert!(
        port.commands
            .borrow()
            .iter()
            .all(|command| matches!(command, Command::PlaceOrders { .. }))
    );
    let rows = factory.book().active_chasers();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "shared-chase");
    assert_eq!(rows[0].current_order_ids, vec!["o1"]);
    assert_eq!(
        factory
            .book()
            .progress("shared-chase")
            .unwrap()
            .remaining_size,
        1.0
    );

    port.set_ticker(201.0, 201.5);
    settle(1100).await;
    assert_eq!(port.placements(), vec![(200.0, 1.0), (201.0, 1.0)]);
    assert_eq!(port.cancels(), vec!["o1"]);
    port.update_order("o2", |row| {
        row.filled = 1.0;
        row.remaining = 0.0;
        row.status = OrderStatus::Closed;
    });
    settle(10).await;
    assert_eq!(port.completes().len(), 1);
    assert_eq!(port.completes()[0]["filled"], 1.0);
    assert!(factory.book().active_chasers().is_empty());
    cx.scope.dispose();
}

#[tokio::test(start_paused = true)]
async fn owner_switch_during_initial_or_requote_placement_keeps_retirement_cancels_on_a() {
    bounded_local(async {
        for (requote, nuke) in [(false, false), (true, false), (false, true), (true, true)] {
            let a = FakePort::new("bybit");
            a.accept_placements();
            let deferred = DeferredPort::new(a.clone());
            let first = (!requote).then(|| deferred.hold(Held::Placement));
            let (factory, cx) = start_owner(deferred.clone());
            let release = if requote {
                settle(1100).await;
                let release = deferred.hold(Held::Placement);
                a.set_ticker(101.0, 101.5);
                release
            } else {
                first.unwrap()
            };
            settle(10).await;
            assert!(deferred.waiting.get());
            let (b, next, next_cx) = select_b();
            assert!(!Rc::ptr_eq(&cx.port, &next_cx.port));
            let book = factory.book();
            let old_scope = cx.scope.clone();
            let draining = tokio::task::spawn_local(async move {
                if nuke {
                    assert_eq!(
                        book.stop_for_nuke("owner A nuke").await,
                        vec!["shared-chase"]
                    );
                } else {
                    book.shutdown("owner A retired").await;
                }
                old_scope.dispose();
            });
            settle(10).await;
            assert!(
                !draining.is_finished(),
                "shutdown must wait for A's placement reply"
            );
            release.send(()).unwrap();
            draining.await.unwrap();
            assert!(deferred.resumed.get());
            assert_eq!(
                a.cancels(),
                if requote {
                    vec!["o1", "o2"]
                } else {
                    vec!["o1"]
                }
            );
            assert_eq!(a.completes().len(), 1);
            assert!(factory.book().active_chasers().is_empty());
            b_still_works(&b, &next, &next_cx).await;
        }
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn owner_switch_drops_a_held_cancel_reply_without_touching_b() {
    bounded_local(async {
        let a = FakePort::new("bybit");
        a.accept_placements();
        let deferred = DeferredPort::new(a.clone());
        let (factory, cx) = start_owner(deferred.clone());
        settle(1100).await;
        let release = deferred.hold(Held::Cancel);
        a.set_ticker(101.0, 101.5);
        settle(10).await;
        assert!(deferred.waiting.get());
        let (b, next, next_cx) = select_b();
        retire_a(&factory, &cx).await;
        assert!(
            release.send(()).is_err(),
            "retired cancel continuation was dropped"
        );
        assert!(!deferred.resumed.get());
        assert_eq!(a.placements().len(), 1);
        assert_eq!(a.completes().len(), 1);
        b_still_works(&b, &next, &next_cx).await;
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn owner_switch_drops_a_held_reprice_fill_read_without_filling_b() {
    bounded_local(async {
        let a = FakePort::new("hyperliquid");
        accept_with_fills(&a, Ok(r#"[{"orderId":"o1","qty":0.4}]"#));
        let deferred = DeferredPort::new(a.clone());
        let (factory, cx) = start_owner(deferred.clone());
        settle(1100).await;
        let release = deferred.hold(Held::Fills);
        a.set_ticker(101.0, 101.5);
        settle(10).await;
        assert!(deferred.waiting.get());
        let (b, next, next_cx) = select_b();
        retire_a(&factory, &cx).await;
        assert!(release.send(()).is_err());
        assert_eq!(a.placements().len(), 1);
        assert_eq!(a.completes()[0]["filled"], 0.4);
        b_still_works(&b, &next, &next_cx).await;
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn owner_switch_during_lost_placement_delay_or_lookup_keeps_adoption_on_a() {
    bounded_local(async {
        for held_reply in [false, true] {
            let a = FakePort::new("hyperliquid");
            lose_placements(
                &a,
                r#"{"found":true,"order":{"id":"o1","status":"open","filled":0}}"#,
            );
            let deferred = DeferredPort::new(a.clone());
            let release = deferred.hold(Held::Lookup);
            let (factory, cx) = start_owner(deferred.clone());
            settle(if held_reply { 1100 } else { 100 }).await;
            assert_eq!(deferred.waiting.get(), held_reply);
            assert_eq!(lookups(&a), usize::from(held_reply));
            let (b, next, next_cx) = select_b();
            let book = factory.book();
            let old_scope = cx.scope.clone();
            let draining = tokio::task::spawn_local(async move {
                book.shutdown("owner A retired").await;
                old_scope.dispose();
            });
            settle(if held_reply { 10 } else { 1100 }).await;
            assert!(deferred.waiting.get());
            assert!(!draining.is_finished());
            release.send(()).unwrap();
            draining.await.unwrap();
            assert_eq!(lookups(&a), 1);
            assert_eq!(a.placements().len(), 1);
            assert_eq!(a.cancels(), vec!["o1"]);
            assert_eq!(a.completes().len(), 1);
            b_still_works(&b, &next, &next_cx).await;
        }
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn owner_switch_during_cancel_backoff_drops_a_late_reconnect() {
    bounded_local(async {
        let a = FakePort::new("bybit");
        a.accept_placements();
        let (factory, cx) = start_owner(a.clone());
        settle(1100).await;
        let online = cancels_fail_until(&a, transport_down());
        stop(&factory, &cx);
        settle(10).await;
        assert_eq!(cancels_seen(&a), 1);
        assert!(a.completes().is_empty());
        let (b, next, next_cx) = select_b();
        retire_a(&factory, &cx).await;
        assert_eq!(a.completes().len(), 1);
        let sent = a.commands.borrow().len();
        let emitted = a.emitted.borrow().len();
        online.set(true);
        a.publish(Change {
            resync: true,
            ..Change::default()
        });
        b_still_works(&b, &next, &next_cx).await;
        settle(60_000).await;
        assert_eq!(a.commands.borrow().len(), sent);
        assert_eq!(a.emitted.borrow().len(), emitted);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn owner_switch_during_fill_read_backoff_never_releases_a_replacement_on_b() {
    bounded_local(async {
        let a = FakePort::new("hyperliquid");
        let reads = fills_fail(&a, ErrorKind::RateLimited, 2);
        let (factory, cx) = start_owner(a.clone());
        settle(1100).await;
        a.set_ticker(101.0, 101.5);
        settle(10).await;
        assert_eq!(reads.get(), 1);
        let (b, next, next_cx) = select_b();
        retire_a(&factory, &cx).await;
        assert_eq!(a.placements().len(), 1);
        assert_eq!(a.completes().len(), 1);
        let sent = a.commands.borrow().len();
        b_still_works(&b, &next, &next_cx).await;
        settle(30_000).await;
        assert_eq!(a.commands.borrow().len(), sent);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn owner_switch_during_final_or_keep_order_reporting_drops_the_old_fill_reply() {
    bounded_local(async {
        for keep in [false, true] {
            for retire in [false, true] {
                let a = FakePort::new("bybit");
                accept_with_fills(&a, Ok(r#"[{"orderId":"o1","qty":0.4}]"#));
                let deferred = DeferredPort::new(a.clone());
                let (factory, cx) = start_owner(deferred.clone());
                settle(1100).await;
                let release = deferred.hold(Held::Fills);
                if keep {
                    factory.book().remove_keep_order("shared-chase");
                } else {
                    stop(&factory, &cx);
                }
                settle(10).await;
                assert!(deferred.waiting.get());
                assert!(a.completes().is_empty());
                if retire {
                    let (b, next, next_cx) = select_b();
                    retire_a(&factory, &cx).await;
                    assert!(release.send(()).is_err());
                    assert!(!deferred.resumed.get());
                    b_still_works(&b, &next, &next_cx).await;
                    assert!(a.completes().is_empty());
                } else {
                    release.send(()).unwrap();
                    settle(10).await;
                    assert!(deferred.resumed.get());
                    assert_eq!(a.completes().len(), 1);
                    assert_eq!(a.completes()[0]["filled"], 0.4);
                    cx.scope.dispose();
                }
            }
        }
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn owner_switch_drops_a_queued_changes_even_with_colliding_chase_and_order_ids() {
    bounded_local(async {
        let a = FakePort::new("hyperliquid");
        a.accept_placements();
        let (factory, cx) = start_owner(a.clone());
        settle(1100).await;
        let watcher = {
            let subscribers = a.subscribers.borrow();
            assert_eq!(subscribers.len(), 1, "A's order watcher is subscribed");
            subscribers[0].clone()
        };
        assert!(
            !watcher.is_closed(),
            "A's watcher is live before retirement"
        );
        // Enqueue without yielding: A's watcher has not consumed these messages at the switch.
        a.set_ticker(150.0, 150.5);
        a.publish(Change {
            orders: true,
            positions: true,
            resync: true,
            fills: vec![Fill {
                id: Some("A-queued-fill".into()),
                side: OrderSide::Buy,
                price: 100.0,
                symbol: "BTCUSDT".into(),
                order_id: "o1".into(),
                amount: 0.75,
            }],
            ..Change::default()
        });
        let (b, next, next_cx) = select_b();
        retire_a(&factory, &cx).await;
        assert!(
            watcher.is_closed(),
            "retirement must drop A's watcher receiver and its queued changes"
        );
        let emitted = a.emitted.borrow().len();
        assert_eq!(
            a.completes()[0]["filled"],
            0.0,
            "A's retirement completion has no fill"
        );
        a.set_ticker(175.0, 175.5);
        let late = Change {
            fills: vec![Fill {
                id: Some("A-late-fill".into()),
                side: OrderSide::Buy,
                price: 100.0,
                symbol: "BTCUSDT".into(),
                order_id: "o1".into(),
                amount: 1.0,
            }],
            ..Change::default()
        };
        assert!(
            watcher.send(late).is_err(),
            "the retired watcher cannot receive A's late fill"
        );
        b_still_works(&b, &next, &next_cx).await;
        assert_eq!(a.placements().len(), 1);
        assert_eq!(a.emitted.borrow().len(), emitted);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn owner_switch_retires_a_placement_retry_settlement_wake_throttle_and_watchdog() {
    bounded_local(async {
        for stage in [
            "placement retry",
            "settlement",
            "wake",
            "throttle",
            "watchdog",
        ] {
            let a = FakePort::new("bybit");
            if stage == "placement retry" {
                reply_to_placements(&a, "[]");
            } else {
                a.accept_placements();
            }
            let (factory, cx) = start_owner(a.clone());
            settle(if stage == "placement retry" || stage == "settlement" {
                10
            } else {
                1100
            })
            .await;
            if stage == "throttle" {
                a.set_ticker(100.0, 100.5);
                settle(1).await;
                a.set_ticker(101.0, 101.5);
                settle(1).await;
            } else if stage == "watchdog" {
                settle(28_000).await;
            }
            assert_eq!(a.placements().len(), 1, "{stage}");
            let (b, next, next_cx) = select_b();
            retire_a(&factory, &cx).await;
            let sent = a.commands.borrow().len();
            let emitted = a.emitted.borrow().len();
            b_still_works(&b, &next, &next_cx).await;
            settle(60_000).await;
            assert_eq!(a.commands.borrow().len(), sent, "{stage}");
            assert_eq!(a.emitted.borrow().len(), emitted, "{stage}");
        }
    })
    .await;
}
