use super::*;
use base64::Engine as _;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::model::{Balance, Order, OrderSide, OrderStatus, OrderType};
use safe_cex_engine::wire::section;
use safe_cex_engine::{Command, Cx, Response, SocketId, TimerId, Venue};
use std::time::Duration;

struct Updates {
    book: Book,
}
impl Venue for Updates {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        crate::accounts::testing::HeldLoad {
            placed: Arc::default(),
            load_after_ms: None,
            ticker: true,
        }
        .on_timer(cx, TimerId(0));
        cx.store.set_balance(Balance {
            free: 1000.,
            total: 1000.,
            ..Balance::default()
        });
        self.book.load_sorted(BookSide::Bids, [(99., 1.)]);
        self.book.load_sorted(BookSide::Asks, [(101., 1.)]);
        cx.book_changed("BTCUSDT");
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, cx: &mut Cx<'_, ()>, command: Command) {
        let Some(id) = command.completion_id() else {
            return;
        };
        match command {
            Command::FetchCandles { limit: Some(1), .. } => {
                // Prices are fixture facts, independent of actor-remapped completion IDs.
                cx.store.update_ticker("BTCUSDT", |t| t.last = 111.);
                self.book.apply(BookSide::Bids, 99., 0.);
                cx.book_changed("BTCUSDT");
            }
            Command::FetchCandles { limit: Some(2), .. } => {
                cx.store.update_ticker("BTCUSDT", |t| t.mark = 222.);
                self.book.apply(BookSide::Bids, 98., 2.);
                cx.book_changed("BTCUSDT");
            }
            Command::PlaceOrders { .. } => {
                cx.store.upsert_order(Order {
                    id: "order".into(),
                    parent_id: None,
                    status: OrderStatus::Open,
                    symbol: "BTCUSDT".into(),
                    kind: OrderType::Limit,
                    side: OrderSide::Buy,
                    price: 100.,
                    amount: 1.,
                    filled: 0.,
                    remaining: 1.,
                    reduce_only: false,
                    close: None,
                    client_order_id: None,
                    time_in_force: None,
                    last_update: cx.now,
                    trigger_type: None,
                    take_profit: None,
                    stop_loss: None,
                    extra: None,
                });
                let mut position = cx.store.positions()[0].clone();
                position.contracts = 2.;
                cx.store.set_positions(vec![position]);
                cx.fill(safe_cex_engine::engine::Fill {
                    id: Some("execution".into()),
                    order_id: "order".into(),
                    symbol: "BTCUSDT".into(),
                    side: OrderSide::Buy,
                    price: 100.,
                    amount: 1.,
                });
                cx.done(id, r#"["order"]"#.to_owned());
                return;
            }
            _ => {}
        }
        cx.done(id, "[]".to_owned());
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        (symbol == "BTCUSDT").then_some(&self.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::once(("BTCUSDT", &self.book)))
    }
}

async fn tick(ms: u64) {
    for _ in 0..ms.div_ceil(5) {
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
        tokio::time::advance(Duration::from_millis(5)).await;
    }
    for _ in 0..16 {
        tokio::task::yield_now().await;
    }
}
fn drain(sent: &mut crate::out::Receiver) -> Vec<Value> {
    std::iter::from_fn(|| sent.try_recv().ok())
        .map(|s| serde_json::from_str(&s).unwrap())
        .collect()
}
fn bytes(frame: &Value) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(frame["d"]["data"].as_str().unwrap())
        .unwrap()
}
fn public_frames(messages: &[Value]) -> Vec<&Value> {
    messages
        .iter()
        .filter(|v| {
            v["d"]["op"] == "frame"
                && safe_cex_engine::wire::sections(&bytes(v)).any(|(kind, _)| {
                    matches!(
                        kind,
                        section::TICKERS
                            | section::TICKER_DELTAS
                            | section::BOOK
                            | section::BOOK_DELTA
                    )
                })
        })
        .collect()
}
async fn first(sent: &mut crate::out::Receiver) -> Value {
    loop {
        let value: Value = serde_json::from_str(&sent.recv().await.unwrap()).unwrap();
        if value["d"]["op"] == "frame" {
            assert_eq!(value["d"]["seq"], 1);
            assert_eq!(value["d"]["snapshot"], true);
            return value;
        }
    }
}
async fn setup(interval: Option<Value>) -> (Session, crate::out::Receiver, Arc<Account>, Value) {
    let registry = crate::accounts::tests::registry();
    let init = acct_init();
    let link = safe_cex_remote_signer::SignerLink::new(0, Arc::new(|_| true));
    let (mut account, _) = crate::accounts::testing::account_on(
        Updates {
            book: Book::default(),
        },
        ("u", "acct", &init.exchange_name),
        Some(link),
        init.fingerprint(),
        Arc::default(),
    );
    crate::accounts::testing::bind_identity(&mut account, &init);
    crate::accounts::testing::register(&registry, &account);
    let (mut session, mut sent) = user_session(registry, "u");
    let mut data = serde_json::to_value(init).unwrap();
    if let Some(value) = interval {
        data["publishIntervalMs"] = value;
    }
    session
        .handle(&json!({"t":"init", "d":data}).to_string())
        .await
        .unwrap();
    let snapshot = first(&mut sent).await;
    drain(&mut sent);
    (session, sent, account, snapshot)
}
fn update(session: &Session, account: &Account, id: u32, part: u32) {
    assert!(account.handle.command(
        session.joined["acct"].client.id,
        Command::FetchCandles {
            command: id,
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
            limit: Some(part),
            from_ms: None,
            to_ms: None
        }
    ));
}
async fn set(session: &mut Session, account: &str, value: Value) {
    session.handle(&json!({"t":"msg","d":{"op":"set-publish-interval","accountName":account,"data":{"publishIntervalMs":value}}}).to_string()).await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn init_interval_coalesces_fields_and_book_deletes_until_the_deadline() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (session, mut sent, account, snapshot) = setup(Some(json!(500))).await;
            update(&session, &account, 111, 1);
            tick(70).await;
            assert!(
                public_frames(&drain(&mut sent)).is_empty(),
                "public change escaped before500ms"
            );
            update(&session, &account, 222, 2);
            tick(70).await;
            assert!(public_frames(&drain(&mut sent)).is_empty());
            tick(400).await;
            let messages = drain(&mut sent);
            let updates = public_frames(&messages);
            assert_eq!(updates.len(), 1);
            let mut store = safe_cex_engine::Store::default();
            let mut state = safe_cex_engine::ingest::IngestState::default();
            let mut book = Book::default();
            for message in [snapshot, updates[0].clone()] {
                let data = bytes(&message);
                let frame = safe_cex_engine::ingest::parse(&data).unwrap();
                safe_cex_engine::ingest::apply(
                    &mut store,
                    &mut state,
                    &frame,
                    message["d"]["snapshot"] == true,
                )
                .unwrap();
                for s in frame
                    .sections
                    .iter()
                    .filter(|s| matches!(s.kind, section::BOOK | section::BOOK_DELTA))
                {
                    let mut c = safe_cex_engine::ingest::Cursor::new(s.payload);
                    assert_eq!(c.u32().unwrap(), 0);
                    let bids = c.u32().unwrap();
                    let asks = c.u32().unwrap();
                    c.u32().unwrap();
                    if s.kind == section::BOOK {
                        book.clear();
                    }
                    for (side, count) in [(BookSide::Bids, bids), (BookSide::Asks, asks)] {
                        for _ in 0..count {
                            let price = c.f64().unwrap();
                            let amount = c.f64().unwrap();
                            if s.kind == section::BOOK {
                                c.f64().unwrap();
                            }
                            book.apply(side, price, amount);
                        }
                    }
                }
            }
            assert_eq!(
                store.ticker("BTCUSDT").map(|t| (t.last, t.mark)),
                Some((111., 222.))
            );
            assert_eq!(book.bids(), &[(98., 2.)]);
            session.close();
            account.handle.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn absent_interval_keeps_the_original_unpaced_stream() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (session, mut sent, account, _) = setup(None).await;
            update(&session, &account, 111, 1);
            tick(70).await;
            assert_eq!(public_frames(&drain(&mut sent)).len(), 1);
            update(&session, &account, 222, 2);
            tick(70).await;
            assert_eq!(public_frames(&drain(&mut sent)).len(), 1);
            session.close();
            account.handle.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn live_interval_updates_only_the_admitted_connection_and_account() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (mut session, mut sent, account, _) = setup(None).await;
            set(&mut session, "acct", json!(500)).await;
            drain(&mut sent);
            update(&session, &account, 111, 1);
            tick(70).await;
            assert!(public_frames(&drain(&mut sent)).is_empty());
            set(&mut session, "acct", json!(50)).await;
            tick(5).await;
            assert_eq!(public_frames(&drain(&mut sent)).len(), 1);
            set(&mut session, "unheld", json!(500)).await;
            let refused = drain(&mut sent);
            assert!(
                refused
                    .iter()
                    .any(|v| v["d"]["data"][1]["kind"] == "not_admitted"
                        && v["d"]["data"][1]["publishIntervalRefused"] == true)
            );
            update(&session, &account, 222, 2);
            tick(70).await;
            assert_eq!(public_frames(&drain(&mut sent)).len(), 1);
            session.close();
            account.handle.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn invalid_live_interval_is_typed_and_keeps_the_current_rate() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (mut session, mut sent, account, _) = setup(Some(json!(500))).await;
            set(&mut session, "acct", json!(1.5)).await;
            let refused = drain(&mut sent);
            assert!(
                refused
                    .iter()
                    .any(|v| v["d"]["data"][1]["kind"] == "invalid_publish_interval")
            );
            update(&session, &account, 111, 1);
            tick(70).await;
            assert!(public_frames(&drain(&mut sent)).is_empty());
            session.close();
            account.handle.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn invalid_init_interval_refuses_without_attaching_or_closing() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for interval in [
                json!(1.5),
                json!("250"),
                json!(null),
                json!(false),
                json!([]),
                json!({"value":250}),
            ] {
                let registry = crate::accounts::tests::registry();
                let init = acct_init();
                // The old server must also use an offline actor when it ignores this new field.
                let account = crate::accounts::testing::trading_account(&registry, "u", &init);
                let (mut session, mut sent) = user_session(registry, "u");
                let mut data = serde_json::to_value(init).unwrap();
                data["publishIntervalMs"] = interval;
                session
                    .handle(&json!({"t":"init","d":data}).to_string())
                    .await
                    .unwrap();
                assert!(session.joined.is_empty() && session.pending_verifications.is_empty());
                let messages = drain(&mut sent);
                assert!(
                    messages
                        .iter()
                        .any(|v| v["d"]["data"][1]["kind"] == "invalid_publish_interval"
                            && v["d"]["data"][1]["initRefused"] == true),
                    "{messages:?}"
                );
                session.close();
                account.handle.dispose();
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn private_orders_positions_fills_and_outcomes_never_wait_for_publication() {
    tokio::task::LocalSet::new().run_until(async {
        let (mut session, mut sent, account, _) = setup(Some(json!(5000))).await;
        update(&session, &account, 111, 1); tick(70).await; drain(&mut sent);
        session.handle(&json!({"t":"msg","d":{"op":"callMethod","accountName":"acct","data":{"requestId":777,"method":"placeOrders","args":[[{"symbol":"BTCUSDT","side":"buy","type":"limit","price":100,"amount":1}]]}}}).to_string()).await.unwrap();
        tick(70).await; let messages = drain(&mut sent);
        assert!(messages.iter().any(|v|v["d"]["op"] == "callMethod:done" && v["d"]["data"]["requestId"] == 777 && v["d"]["data"]["success"] == true), "{messages:?}");
        let mut kinds=std::collections::HashSet::new();
        for v in messages.iter().filter(|v|v["d"]["op"] == "frame") { for (kind, _) in safe_cex_engine::wire::sections(&bytes(v)) { kinds.insert(kind); } }
        assert!(kinds.contains(&section::ORDERS) && kinds.contains(&section::POSITIONS) && kinds.contains(&section::FILLS), "{kinds:?}");
        assert!(!kinds.contains(&section::TICKERS) && !kinds.contains(&section::TICKER_DELTAS) && !kinds.contains(&section::BOOK) && !kinds.contains(&section::BOOK_DELTA));
        session.close(); account.handle.dispose();
    }).await;
}

#[tokio::test(start_paused = true)]
async fn shared_account_holders_keep_independent_publication_rates() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (mut slow, mut slow_sent, account, _) = setup(Some(json!(500))).await;
            let (mut fast, mut fast_sent) = user_session(slow.registry.clone(), "u");
            let mut data = serde_json::to_value(acct_init()).unwrap();
            data["publishIntervalMs"] = json!(50);
            fast.handle(&json!({"t":"init","d":data}).to_string())
                .await
                .unwrap();
            first(&mut fast_sent).await;
            drain(&mut fast_sent);
            assert!(Arc::ptr_eq(
                &slow.joined["acct"].account,
                &fast.joined["acct"].account
            ));
            update(&slow, &account, 111, 1);
            tick(70).await;
            assert_eq!(public_frames(&drain(&mut fast_sent)).len(), 1);
            assert!(public_frames(&drain(&mut slow_sent)).is_empty());
            set(&mut slow, "acct", json!(1000)).await;
            update(&fast, &account, 222, 2);
            tick(70).await;
            assert_eq!(public_frames(&drain(&mut fast_sent)).len(), 1);
            assert!(public_frames(&drain(&mut slow_sent)).is_empty());
            tick(900).await;
            assert_eq!(public_frames(&drain(&mut slow_sent)).len(), 1);
            assert!(Arc::ptr_eq(&slow.joined["acct"].account, &account));
            slow.close();
            fast.close();
            account.handle.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn resnapshot_keeps_rate_and_stops_the_old_pending_publication() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (mut session, mut sent, account, _) = setup(Some(json!(500))).await;
            update(&session, &account, 111, 1);
            tick(70).await;
            assert!(public_frames(&drain(&mut sent)).is_empty());
            // A waiting reply keeps the old pump alive after the new snapshot takes over.
            session.joined["acct"]
                .client
                .pending
                .lock()
                .unwrap()
                .insert(
                    999,
                    Pending {
                        request_id: json!(999),
                        method: "fetchCandles".into(),
                        reply: methods::Reply::Result,
                        group: None,
                        incomplete: false,
                        audit: None,
                    },
                );
            assert!(session.resnapshot("acct", None).await);
            let snapshot = first(&mut sent).await;
            assert_eq!(snapshot["d"]["seq"], 1);
            assert_eq!(session.joined["acct"].old.len(), 1);
            assert_eq!(
                *session.joined["acct"].client.publish_interval.borrow(),
                500
            );
            drain(&mut sent);
            tick(600).await;
            assert!(
                public_frames(&drain(&mut sent)).is_empty(),
                "old pump emitted a stale cached frame"
            );
            update(&session, &account, 222, 2);
            tick(70).await;
            let messages = drain(&mut sent);
            let updates = public_frames(&messages);
            assert_eq!(updates.len(), 1);
            assert_eq!(updates[0]["d"]["seq"], 2);
            session.close();
            account.handle.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn integer_bounds_are_clamped_and_invalid_updates_never_replace_them() {
    tokio::task::LocalSet::new().run_until(async {
        let (mut session, mut sent, account, _) = setup(Some(json!(-1))).await;
        assert_eq!(*session.joined["acct"].client.publish_interval.borrow(), 50);
        for (value, expected) in [(json!(0),50), (json!(49),50), (json!(250),250), (json!(u64::MAX),5000)] {
            set(&mut session, "acct", value).await;
            assert_eq!(*session.joined["acct"].client.publish_interval.borrow(), expected);
        }
        for value in [json!(null),json!(true),json!("50"),json!(500.5),json!([]),json!({})] {
            set(&mut session, "acct", value).await;
            assert_eq!(*session.joined["acct"].client.publish_interval.borrow(),5000);
            assert!(drain(&mut sent).iter().any(|v| v["d"]["data"][1]["kind"]=="invalid_publish_interval"));
        }
        session.handle(&json!({"t":"msg","d":{"op":"set-publish-interval","data":{"publishIntervalMs":50}}}).to_string()).await.unwrap();
        assert!(drain(&mut sent).iter().any(|v| v["d"]["data"][1]["kind"]=="not_admitted"));
        session.close(); account.handle.dispose();
    }).await;
}

#[test]
fn publication_metadata_is_not_an_engine_option_or_account_identity() {
    let plain = acct_init();
    for interval in [50, 5000] {
        let mut paced = plain.clone();
        paced.publish_interval_ms = envelope::RequestedInterval::Milliseconds(interval);
        assert_eq!(plain.fingerprint(), paced.fingerprint());
        assert_eq!(
            serde_json::to_value(&plain).unwrap(),
            serde_json::to_value(&paced).unwrap()
        );
    }
}

async fn saturated_pump(change_interval: bool) {
    let backlog = safe_cex_native::ServerConfig::default().client_queue_capacity;
    let (session, _unused, account, snapshot) = setup(None).await;
    let handle = account.handle.clone();
    let (input, outgoing) = tokio::sync::mpsc::channel(backlog);
    let (_cancel, canceled) = tokio::sync::watch::channel(false);
    let (out, mut sent, _) = Out::channel(backlog + 32);
    let (dropped, _dropped_rx) = tokio::sync::mpsc::unbounded_channel();
    let (rate, rate_rx) = tokio::sync::watch::channel(if change_interval { 5000 } else { 50 });
    let task = tokio::task::spawn_local(pump(
        safe_cex_native::ClientLink { outgoing, canceled },
        1,
        dropped,
        handle,
        "acct".into(),
        "acct".into(),
        out,
        Arc::default(),
        Arc::default(),
        None,
        rate_rx,
    ));
    input
        .send(Outgoing::Frame(bytes(&snapshot).into()))
        .await
        .unwrap();
    first(&mut sent).await;
    drain(&mut sent);
    let mut update = safe_cex_engine::wire::FrameWriter::new(Vec::new(), 10.);
    update.begin(section::TICKER_DELTAS);
    update.f64s([0., 4., 123.]);
    update.end();
    input
        .send(Outgoing::Frame(update.finish().into()))
        .await
        .unwrap();
    for _ in 0..16 {
        tokio::task::yield_now().await;
    }
    assert!(public_frames(&drain(&mut sent)).is_empty());
    let mut control = safe_cex_engine::wire::FrameWriter::new(Vec::new(), 20.);
    control.begin(65000);
    control.u32s([1, 0]);
    control.end();
    let control: Arc<[u8]> = control.finish().into();
    for _ in 0..backlog {
        input.try_send(Outgoing::Frame(control.clone())).unwrap();
    }
    if change_interval {
        rate.send_replace(50);
    }
    tokio::time::advance(Duration::from_millis(60)).await;
    let mut before_public = 0;
    loop {
        let message: Value = serde_json::from_str(&sent.recv().await.unwrap()).unwrap();
        if message["d"]["op"] != "frame" {
            continue;
        }
        let data = bytes(&message);
        if safe_cex_engine::wire::sections(&data).any(|(kind, _)| kind == section::TICKER_DELTAS) {
            break;
        }
        assert!(safe_cex_engine::wire::sections(&data).any(|(kind, _)| kind == 65000));
        before_public += 1;
    }
    assert!(
        before_public <= 1,
        "due publication/live rate waited behind {before_public} ready frames"
    );
    task.abort();
    session.close();
    account.handle.dispose();
}

#[tokio::test(start_paused = true)]
async fn publication_deadline_progresses_while_the_native_inbox_is_ready() {
    tokio::task::LocalSet::new()
        .run_until(saturated_pump(false))
        .await;
}

#[tokio::test(start_paused = true)]
async fn live_interval_progresses_while_the_native_inbox_is_ready() {
    tokio::task::LocalSet::new()
        .run_until(saturated_pump(true))
        .await;
}
