//! Every hosted algo op with the exact payload the web sends (NX call sites named
//! on each test), through the server's algo host, asserting no refusal and the
//! reply and event shapes the web's handlers read. The live e2e cannot exercise
//! algos on a real account, so these are the gate.
use safe_cex_engine::book::Book;
use safe_cex_engine::model::{InstrumentClassification, Limits, Market, MinMax, Precision, Ticker};
use safe_cex_engine::{Command, Cx, HttpRequest, Response, SocketId, TimerId, Venue};
use safe_cex_exchange_server::accounts::PlacedSymbols;
use safe_cex_exchange_server::algos::{self, AlgoInput};
use safe_cex_exchange_server::audit::Who;
use safe_cex_native::{Events, Host, HostEvent, ServerConfig, Transport};
use serde_json::{Value, json};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::LocalSet;

#[derive(Clone, Default)]
struct Quiet;
impl Transport for Quiet {
    fn connect(
        &self,
        socket: SocketId,
        _: String,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        let _ = events.send(HostEvent::WsOpened(socket));
        mpsc::unbounded_channel().0
    }
    fn request(&self, _: HttpRequest, _: Events) {
        panic!("the test venue never requests HTTP")
    }
}

/// One BTCUSDT perp, no orders or positions; accepts and records every command.
struct AlgoVenue {
    book: Book,
    commands: Arc<Mutex<Vec<String>>>,
    hold_placements: bool,
}
impl Venue for AlgoVenue {
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities::BASE
    }
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        cx.store.set_markets(vec![Market {
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
                leverage: MinMax { min: 1., max: 100. },
                price: None,
            },
            contract_size: None,
            is_expiring: None,
            is_inverse: None,
            is_spot: None,
            is_perpetual: Some(true),
            is_read_only: None,
            taker_fee: None,
            maker_fee: None,
        }]);
        cx.store
            .set_instrument_classifications(vec![InstrumentClassification {
                market: 0,
                symbol_type: None,
                market_region: None,
                underlying_ticker: None,
            }]);
        cx.store.set_tickers(vec![Ticker {
            id: "BTCUSDT".into(),
            symbol: "BTCUSDT".into(),
            bid: 60_000.,
            ask: 60_000.1,
            last: 60_000.,
            mark: 60_000.,
            index: 60_000.,
            mid_price: None,
            percentage: 0.,
            open_interest: 0.,
            funding_rate: 0.,
            funding_time: 0.,
            volume: 0.,
            quote_volume: 0.,
        }]);
        cx.store.set_orders(Vec::new());
        cx.store.set_positions(Vec::new());
        cx.connect("test");
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
        let result = match &command {
            Command::PlaceOrders { orders, .. } => {
                let ids: Vec<String> = (0..orders.len()).map(|i| format!("o-{i}")).collect();
                serde_json::to_string(&ids).unwrap()
            }
            _ => "null".into(),
        };
        self.commands.lock().unwrap().push(format!("{command:?}"));
        if !(self.hold_placements && matches!(command, Command::PlaceOrders { .. })) {
            cx.done(id, result);
        }
    }
    fn book(&self, _: &str) -> Option<&Book> {
        Some(&self.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::once(("BTCUSDT", &self.book)))
    }
}

/// The account's algo host, fed as the session feeds it, with every envelope it sends.
struct Algos {
    inbox: mpsc::UnboundedSender<AlgoInput>,
    out: mpsc::UnboundedReceiver<String>,
    seen: Vec<Value>,
    commands: Arc<Mutex<Vec<String>>>,
    owner: Arc<safe_cex_exchange_server::twap_owner::Binding>,
}

impl Algos {
    async fn start() -> Self {
        Self::with_held_placements(false).await
    }
    async fn with_held_placements(hold_placements: bool) -> Self {
        Self::with_binding(
            hold_placements,
            Arc::new(safe_cex_exchange_server::twap_owner::Binding::new("u")),
        )
        .await
    }
    async fn with_binding(
        hold_placements: bool,
        owner: Arc<safe_cex_exchange_server::twap_owner::Binding>,
    ) -> Self {
        let commands: Arc<Mutex<Vec<String>>> = Arc::default();
        let engine = safe_cex_engine::test_support::seeded_engine(AlgoVenue {
            book: Book::default(),
            commands: commands.clone(),
            hold_placements,
        });
        let config = ServerConfig {
            flush_every: Duration::from_millis(1),
            io_timeout: Duration::from_millis(100),
            ..ServerConfig::default()
        };
        let (handle, _task) =
            safe_cex_native::spawn_account(Host::new(engine, Quiet), config).unwrap();
        let (tx, out) = mpsc::unbounded_channel::<String>();
        let (inbox, inbox_rx) = mpsc::unbounded_channel();
        tokio::task::spawn_local(algos::run(
            handle,
            1,
            Who {
                user: "u".into(),
                account: "acct".into(),
                venue: "bybit".into(),
            },
            Box::new({
                let tx = tx.clone();
                move |text| {
                    let _ = tx.send(text);
                }
            }),
            Box::new(move |text| {
                if let Some(text) = text {
                    let _ = tx.send(text);
                }
                safe_cex_exchange_server::algos::Holding {
                    live: vec![1],
                    parked: false,
                }
            }),
            None,
            PlacedSymbols::default(),
            Arc::new(AtomicBool::new(true)),
            Default::default(),
            owner.clone(),
            inbox_rx,
        ));
        // The feed's join snapshot loads the market before any op.
        tokio::time::sleep(Duration::from_millis(100)).await;
        Self {
            inbox,
            out,
            seen: Vec::new(),
            commands,
            owner,
        }
    }

    /// As `session.rs`: the web's `{op, data}` with `data` re-serialized.
    fn op(&self, op: &str, data: Value) {
        self.inbox
            .send(AlgoInput::Op {
                op: op.into(),
                data: data.to_string(),
                origin: "socket:1".into(),
                twap_owner: self.owner.snapshot(),
                action: None,
            })
            .unwrap();
    }

    /// The `d` of the next envelope with `op` whose data matches, never past a refusal.
    async fn next(&mut self, op: &str, matches: impl Fn(&Value) -> bool) -> Value {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let text = self.out.recv().await.expect("algo host alive");
                let message: Value = serde_json::from_str(&text).unwrap();
                let d = message["d"].clone();
                assert_ne!(d["op"], "error", "refused: {d}");
                self.seen.push(d.clone());
                if d["op"] == op && matches(&d["data"]) {
                    return d;
                }
            }
        })
        .await
        .unwrap_or_else(|_| panic!("no {op}; saw {:?}", self.seen))
    }
}

/// The listener's first argument: exchangeAccountController spreads an array
/// `data` and wraps a lone value.
fn first(data: &Value) -> &Value {
    match data {
        Value::Array(items) => items.first().unwrap_or(&Value::Null),
        other => other,
    }
}

/// `chaseOrder.ts`: the order form, hotkeys and ladder chase.
fn web_chase(client_id: Option<&str>) -> Value {
    let mut chase = json!({"symbol":"BTCUSDT","size":0.01,"side":"buy","maxAttempts":1000,
        "reduceOnly":false,"delay":500,"origin":"web"});
    if let Some(id) = client_id {
        chase["clientId"] = id.into();
    }
    chase
}

/// The web `Market` the TWAP forms pass as `activeMarket`.
fn web_market() -> Value {
    json!({"id":"BTCUSDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,
        "precision":{"amount":0.001,"price":0.1},
        "limits":{"amount":{"min":0.001,"max":1000},"leverage":{"min":1,"max":100}},
        "isPerpetual":true})
}

/// `createTWAPOrder.ts` from the classic TWAP form (`TWAPOrder.tsx`).
fn web_twap() -> Value {
    json!({"accountName":"acct","activeMarket":web_market(),"size":0.1,"duration":10,
        "ordersCount":10,"side":"buy","chaserDelay":500,"reduceOnly":false,
        "randomnessPercentage":0,"useLimitOrders":false,"useChaserOrders":false,"origin":"web"})
}

fn active_chasers(store: &Value) -> Vec<Value> {
    store["activeChasers"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

fn active_twaps(store: &Value) -> Vec<Value> {
    store["activeTWAPs"].as_array().cloned().unwrap_or_default()
}

/// The fields ActiveChasers.tsx and useActiveChasersColumns.tsx read (the web adds accountName).
fn assert_chaser_row(row: &Value) {
    for (field, check) in [
        ("id", Value::is_string as fn(&Value) -> bool),
        ("symbol", Value::is_string),
        ("size", Value::is_number),
        ("side", Value::is_string),
        ("maxAttempts", Value::is_number),
        ("reduceOnly", Value::is_boolean),
        ("attempts", Value::is_number),
        ("isStopped", Value::is_boolean),
        ("stopReason", Value::is_string),
        ("currentOrderIds", Value::is_array),
    ] {
        assert!(check(&row[field]), "activeChasers.{field}: {row}");
    }
}

/// The fields ActiveTWAPs.tsx and useTwapColumns.tsx read.
fn assert_twap_row(row: &Value) {
    for (field, check) in [
        ("id", Value::is_string as fn(&Value) -> bool),
        ("accountName", Value::is_string),
        ("symbol", Value::is_string),
        ("side", Value::is_string),
        ("size", Value::is_number),
        ("duration", Value::is_number),
        ("ordersCount", Value::is_number),
        ("ordersExecuted", Value::is_number),
        ("randomnessPercentage", Value::is_number),
        ("reduceOnly", Value::is_boolean),
        ("status", Value::is_string),
        ("origin", Value::is_string),
    ] {
        assert!(check(&row[field]), "activeTWAPs.{field}: {row}");
    }
    for field in ["base", "quote", "symbol"] {
        assert!(
            row["activeMarket"][field].is_string(),
            "activeMarket.{field}: {row}"
        );
    }
}

#[tokio::test]
async fn chaser_start_from_the_order_form_lists_an_active_chaser() {
    LocalSet::new()
        .run_until(async {
            let mut algos = Algos::start().await;
            algos.op("chaser:start", web_chase(None));
            let store = algos
                .next("store", |data| !active_chasers(data).is_empty())
                .await;
            let row = &active_chasers(&store["data"])[0];
            assert_chaser_row(row);
            assert_eq!(row["origin"], "web");
            assert_eq!(row["symbol"], "BTCUSDT");
        })
        .await;
}

#[tokio::test]
async fn chaser_start_from_cross_trade_has_no_origin_and_still_runs() {
    LocalSet::new()
        .run_until(async {
            let mut algos = Algos::start().await;
            // crossTradePlaceOrder.ts.
            algos.op(
                "chaser:start",
                json!({"symbol":"BTCUSDT","size":0.01,"side":"sell","maxAttempts":1000,
                    "reduceOnly":true,"delay":1000}),
            );
            let store = algos
                .next("store", |data| !active_chasers(data).is_empty())
                .await;
            assert_chaser_row(&active_chasers(&store["data"])[0]);
        })
        .await;
}

#[tokio::test]
async fn chaser_terminate_by_id_completes_with_the_tp_sl_client_id() {
    LocalSet::new()
        .run_until(async {
            let mut algos = Algos::start().await;
            algos.op("chaser:start", web_chase(Some("track-1")));
            let store = algos
                .next("store", |data| !active_chasers(data).is_empty())
                .await;
            let id = active_chasers(&store["data"])[0]["id"].clone();
            // ActiveChasers.tsx / useActiveChasersColumns.tsx emit the bare id.
            algos.op("chaser:terminate", id.clone());
            let complete = algos.next("chaser:complete", |_| true).await;
            // The controller spreads `data`: chaseCompletionRegistry reads one ChaserCompleteEvent.
            let event = first(&complete["data"]);
            assert_eq!(event["id"], id);
            assert_eq!(event["clientId"], "track-1");
            for (field, check) in [
                ("symbol", Value::is_string as fn(&Value) -> bool),
                ("side", Value::is_string),
                ("reduceOnly", Value::is_boolean),
                ("size", Value::is_number),
                ("filled", Value::is_number),
                ("stopReason", Value::is_string),
            ] {
                assert!(check(&event[field]), "chaser:complete.{field}: {event}");
            }
            // As TS, the stopped chase stays listed, marked stopped with its reason.
            let stopped = algos
                .seen
                .iter()
                .rev()
                .find(|d| d["op"] == "store" && !active_chasers(&d["data"]).is_empty())
                .map(|d| active_chasers(&d["data"])[0].clone())
                .unwrap();
            assert_eq!(stopped["isStopped"], true);
            assert_eq!(stopped["stopReason"], "User cancelled");
        })
        .await;
}

#[tokio::test]
async fn twap_start_from_the_form_lists_a_twap_and_persists_its_state() {
    LocalSet::new()
        .run_until(async {
            let mut algos = Algos::start().await;
            algos.op("twap:start", web_twap());
            let store = algos
                .next("store", |data| !active_twaps(data).is_empty())
                .await;
            assert_twap_row(&active_twaps(&store["data"])[0]);
            // exchangeAccountController persists a Record<id, {options, state}> per account.
            let update = algos.next("twap:stateUpdate", |_| true).await;
            // ...and discards an array: it must be the record itself.
            let rows = update["data"].as_object().expect("a record by id").clone();
            let (_, row) = rows.iter().next().expect("one TWAP");
            assert_eq!(row["options"]["accountName"], "acct");
            assert!(row["state"]["status"].is_string(), "{row}");
        })
        .await;
}

#[tokio::test]
async fn twap_pause_resume_stop_and_cleanup_take_the_bare_id() {
    LocalSet::new()
        .run_until(async {
            let mut algos = Algos::start().await;
            algos.op("twap:start", web_twap());
            let store = algos
                .next("store", |data| !active_twaps(data).is_empty())
                .await;
            let id = active_twaps(&store["data"])[0]["id"].clone();
            // useTwapColumns.tsx: pause, resume, stop, cleanup with instance.id.
            algos.op("twap:pause", id.clone());
            algos
                .next("store", |data| {
                    active_twaps(data)
                        .first()
                        .is_some_and(|row| row["status"] == "paused")
                })
                .await;
            algos.op("twap:resume", id.clone());
            algos
                .next("store", |data| {
                    active_twaps(data)
                        .first()
                        .is_some_and(|row| row["status"] == "running")
                })
                .await;
            // ActiveTWAPs.tsx stop, then cleanup removes it.
            algos.op("twap:stop", id.clone());
            algos.op("twap:cleanup", id.clone());
            algos
                .next("store", |data| active_twaps(data).is_empty())
                .await;
        })
        .await;
}

#[tokio::test]
async fn bound_twap_host_resumes_only_matching_persisted_ownership() {
    LocalSet::new()
        .run_until(async {
            for case in ["same", "foreign", "stale", "untagged"] {
                let mut algos = Algos::start().await;
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as f64;
                let generation = algos.owner.snapshot().generation;
                let mut row = json!({"id":"saved", "ownerId":"u", "ownerGeneration":generation,
                "options":web_twap(), "state":{
                    "accountName":"acct", "activeMarket":web_market(), "size":0.1,
                    "ordersSizes":[0.01], "side":"buy", "status":"running", "ordersCount":10,
                    "ordersExecuted":9, "nextOrderAt":now + 100.0
                }});
                if case == "foreign" {
                    row["ownerId"] = json!("other-user");
                }
                if case == "stale" {
                    row["ownerGeneration"] = json!(generation + 1);
                }
                if case == "untagged" {
                    row.as_object_mut().unwrap().remove("ownerId");
                    row.as_object_mut().unwrap().remove("ownerGeneration");
                }
                algos.op("twap:state", json!([row]));
                if case == "same" {
                    algos
                        .next("store", |data| !active_twaps(data).is_empty())
                        .await;
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    assert!(
                        algos
                            .commands
                            .lock()
                            .unwrap()
                            .iter()
                            .any(|command| command.contains("PlaceOrders"))
                    );
                } else {
                    let refusal = tokio::time::timeout(Duration::from_secs(2), async {
                        loop {
                            let message: Value =
                                serde_json::from_str(&algos.out.recv().await.unwrap()).unwrap();
                            if message["d"]["op"] == "error" {
                                break message["d"]["data"].clone();
                            }
                        }
                    })
                    .await
                    .unwrap();
                    assert_eq!(refusal[1]["forceToast"], true, "{case}: {refusal}");
                    assert!(
                        refusal
                            .to_string()
                            .contains("persisted TWAP owner or generation does not match"),
                        "{case}: {refusal}"
                    );
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    assert!(
                        algos.commands.lock().unwrap().is_empty(),
                        "{case} placed a stale slice"
                    );
                }
            }
        })
        .await;
}

#[tokio::test]
async fn a_twap_queued_before_rebind_is_dropped_instead_of_adopted() {
    LocalSet::new()
        .run_until(async {
            for rebind in [false, true] {
                let mut algos = Algos::start().await;
                // No yield between enqueue and rebind: the op carries the old snapshot.
                algos.op("twap:start", web_twap());
                if rebind {
                    algos.owner.rebind_owner("other-user");
                    let refusal = tokio::time::timeout(Duration::from_secs(2), async {
                        loop {
                            let message: Value = serde_json::from_str(&algos.out.recv().await.unwrap()).unwrap();
                            if message["d"]["op"] == "error" {
                                break message["d"]["data"].clone();
                            }
                        }
                    }).await.unwrap();
                    assert_eq!(refusal[0], "Algo operation cancelled-for-owner-change: authenticated connection binding changed");
                    assert_eq!(refusal[1]["forceToast"], true);
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    assert!(algos.commands.lock().unwrap().is_empty());
                    // A newly captured op still works under the new generation.
                    algos.op("twap:start", web_twap());
                }
                algos.next("store", |data| !active_twaps(data).is_empty()).await;
                assert_eq!(algos.commands.lock().unwrap().len(), 1);
            }
        }).await;
}

#[tokio::test]
async fn a_same_user_resume_keeps_a_running_twap() {
    LocalSet::new()
        .run_until(async {
            let mut algos = Algos::start().await;
            algos.op("twap:start", web_twap());
            // A same-user reconnect/resume inside the 60-second window is not an owner change.
            assert!(!algos.owner.rebind_owner("u"));
            algos
                .next("store", |data| !active_twaps(data).is_empty())
                .await;
            assert_eq!(algos.commands.lock().unwrap().len(), 1);
            assert!(
                !algos
                    .seen
                    .iter()
                    .any(|event| event.to_string().contains("cancelled-for-owner-change")),
                "a same-user resume cancelled the TWAP"
            );
        })
        .await;
}

#[tokio::test]
async fn twap_state_restores_the_rows_its_own_state_updates_persisted() {
    LocalSet::new()
        .run_until(async {
            let mut first = Algos::start().await;
            first.op("twap:start", web_twap());
            // Once the first slice settled: a row persisted mid-placement is settled on restore.
            let update = first
                .next("twap:stateUpdate", |data| {
                    data.as_object().is_some_and(|rows| {
                        rows.values()
                            .all(|row| row["state"].get("inFlight").is_none())
                    })
                })
                .await;
            let (id, row) = update["data"]
                .as_object()
                .unwrap()
                .iter()
                .next()
                .map(|(id, row)| (id.clone(), row.clone()))
                .unwrap();
            // Persistence retains the original tags; a fresh host for the same binding resumes.
            let mut restored = Algos::with_binding(false, first.owner.clone()).await;
            let mut saved = row.clone();
            saved["id"] = id.clone().into();
            restored.op("twap:state", json!([saved]));
            let store = restored
                .next("store", |data| !active_twaps(data).is_empty())
                .await;
            let twap = &active_twaps(&store["data"])[0];
            assert_eq!(twap["id"], id);
            assert_twap_row(twap);
        })
        .await;
}

#[tokio::test]
async fn exec_from_the_cli_answers_exec_result_with_its_req_id() {
    LocalSet::new()
        .run_until(async {
            let mut algos = Algos::start().await;
            // runCliLine.ts / cliChainRunner.ts.
            algos.op(
                "exec",
                json!({"account":"acct","symbol":"BTCUSDT","command":"echo hi","reqId":"req-1"}),
            );
            let result = algos
                .next("exec-result", |data| first(data)["reqId"] == "req-1")
                .await;
            // The controller spreads `data`; cliChainRunner reads reqId, handled, success.
            let result = first(&result["data"]);
            assert_eq!(result["handled"], true, "{result}");
            assert_eq!(result["success"], true, "{result}");
        })
        .await;
}

#[tokio::test]
async fn an_exec_sleep_cannot_start_a_twap_after_its_binding_is_retired() {
    LocalSet::new()
        .run_until(async {
            for rebind in [false, true] {
                let mut algos = Algos::start().await;
                algos.op(
                    "exec",
                    json!({"symbol":"BTCUSDT", "reqId":"deferred-twap",
                    "command":"echo checkpoint; sleep 100ms; twap buy $1000 into 2 over 1"}),
                );
                algos
                    .next("exec-log", |data| first(data)["message"] == "checkpoint")
                    .await;
                if rebind {
                    algos.owner.rebind_owner("other-user");
                }
                let result = algos
                    .next("exec-result", |data| {
                        first(data)["reqId"] == "deferred-twap"
                    })
                    .await;
                let result = first(&result["data"]);
                assert_eq!(result["success"], !rebind, "{result}");
                if rebind {
                    assert!(
                        result["error"]
                            .as_str()
                            .unwrap()
                            .contains("cancelled-for-owner-change")
                    );
                    assert!(algos.commands.lock().unwrap().is_empty());
                } else {
                    assert!(algos.seen.iter().any(|event| {
                        event["op"] == "twap:stateUpdate"
                            && event["data"]
                                .as_object()
                                .is_some_and(|rows| !rows.is_empty())
                    }));
                    tokio::time::timeout(Duration::from_secs(2), async {
                        while algos.commands.lock().unwrap().is_empty() {
                            tokio::task::yield_now().await;
                        }
                    })
                    .await
                    .unwrap();
                    assert_eq!(algos.commands.lock().unwrap().len(), 1);
                }
            }
        })
        .await;
}

#[tokio::test]
async fn exec_from_a_macro_without_req_id_logs() {
    LocalSet::new()
        .run_until(async {
            let mut algos = Algos::start().await;
            // MacroPanel.tsx: no reqId; the terminal shows exec-log lines.
            algos.op(
                "exec",
                json!({"account":"acct","symbol":"BTCUSDT","command":"echo macro"}),
            );
            let log = algos.next("exec-log", |_| true).await;
            assert!(first(&log["data"])["message"].is_string(), "{log}");
        })
        .await;
}

#[tokio::test]
async fn twap_state_restores_a_row_the_ts_worker_persisted() {
    LocalSet::new()
        .run_until(async {
            let mut algos = Algos::start().await;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as f64;
            // TwapState.ts / TwapOptions.ts as a pre-cutover tab's sessionStorage holds them;
            // TwapInstance.ts shifts one size off ordersSizes per executed slice.
            let options = web_twap();
            let state = json!({"accountName":"acct","activeMarket":web_market(),"size":0.1,
                "ordersSizes":[0.01,0.01,0.01,0.01,0.01,0.01,0.01],
                "side":"buy","status":"running","ordersCount":10,"ordersExecuted":3,
                "nextOrderAt":now + 60_000.0,"origin":"web","createdAt":now - 180_000.0,
                "startedAt":now - 180_000.0});
            algos.op(
                "twap:state",
                json!([{"id":"ts-twap-1","ownerId":"u","ownerGeneration":algos.owner.snapshot().generation,"options":options,"state":state}]),
            );
            let store = algos
                .next("store", |data| !active_twaps(data).is_empty())
                .await;
            let twap = &active_twaps(&store["data"])[0];
            assert_eq!(twap["id"], "ts-twap-1");
            assert_eq!(twap["ordersExecuted"], 3);
            assert_twap_row(twap);
        })
        .await;
}

#[tokio::test]
async fn a_joining_socket_sees_the_running_chases_again() {
    // A resume (or another tab) gets the algo rows even though they did not change.
    LocalSet::new()
        .run_until(async {
            let mut algos = Algos::start().await;
            algos.op("chaser:start", web_chase(None));
            algos
                .next("store", |data| !active_chasers(data).is_empty())
                .await;
            algos.inbox.send(AlgoInput::Joined).unwrap();
            let store = algos
                .next("store", |data| !active_chasers(data).is_empty())
                .await;
            assert_eq!(active_chasers(&store["data"]).len(), 1);
        })
        .await;
}

/// A drain reads the cancel's answer: it ends at once and reports the chase,
/// not after its whole deadline with its cancels called unsettled.
#[tokio::test]
async fn a_dispose_settles_a_running_chase_well_inside_its_deadline() {
    LocalSet::new()
        .run_until(async {
            let mut algos = Algos::start().await;
            algos.op("chaser:start", web_chase(None));
            algos
                .next("store", |data| !active_chasers(data).is_empty())
                .await;
            algos
                .inbox
                .send(AlgoInput::Dispose {
                    reason: "the server is restarting".into(),
                    stopped: None,
                })
                .unwrap();
            // The stop itself is told as an error; a settled one adds no "may rest".
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let text = algos.out.recv().await.expect("algo host alive");
                    assert!(
                        !text.contains("may rest") && !text.contains("may still"),
                        "{text}"
                    );
                    let message: Value = serde_json::from_str(&text).unwrap();
                    if message["d"]["op"] == "chaser:complete" {
                        return;
                    }
                }
            })
            .await
            .expect("the drain reported its chase inside its deadline");
        })
        .await;
}

#[tokio::test]
async fn disposing_for_no_browser_stops_the_chases_and_says_why() {
    LocalSet::new()
        .run_until(async {
            let mut algos = Algos::start().await;
            algos.op("chaser:start", web_chase(None));
            algos
                .next("store", |data| !active_chasers(data).is_empty())
                .await;
            algos
                .inbox
                .send(AlgoInput::Dispose {
                    reason: "no browser for 60s".into(),
                    stopped: None,
                })
                .unwrap();
            let mut ops = Vec::new();
            let _ = tokio::time::timeout(Duration::from_secs(5), async {
                while let Some(text) = algos.out.recv().await {
                    let message: Value = serde_json::from_str(&text).unwrap();
                    ops.push(message["d"].clone());
                }
            })
            .await;
            assert!(
                ops.iter().any(|d| d["op"] == "error"
                    && d["data"][0] == "Chase on BTCUSDT stopped: no browser for 60s."),
                "{ops:?}"
            );
            assert!(
                ops.iter().any(|d| d["op"] == "warning"
                    && d["data"]
                        .as_str()
                        .is_some_and(|text| text.contains("was stopped"))),
                "the chase reports its stop: {ops:?}"
            );
        })
        .await;
}

/// The live run: the chase ended (filled) while no browser held the account, then
/// the account was disposed. Its end is kept for the next init, marked replayed.
#[tokio::test]
async fn a_chase_that_ended_with_no_browser_is_kept_past_the_dispose() {
    LocalSet::new()
        .run_until(async {
            let mut algos = Algos::start().await;
            algos.op("chaser:start", web_chase(None));
            let store = algos
                .next("store", |data| !active_chasers(data).is_empty())
                .await;
            let id = active_chasers(&store["data"])[0]["id"].clone();
            algos.op("chaser:terminate", id.clone());
            algos.next("chaser:complete", |_| true).await;
            let (kept, notice) = tokio::sync::oneshot::channel();
            let kept = Mutex::new(Some(kept));
            algos
                .inbox
                .send(AlgoInput::Dispose {
                    reason: "no browser for 60s".into(),
                    stopped: Some(Box::new(move |notice| {
                        if let Some(kept) = kept.lock().unwrap().take() {
                            let _ = kept.send(notice);
                        }
                    })),
                })
                .unwrap();
            let kept = tokio::time::timeout(Duration::from_secs(5), notice)
                .await
                .unwrap()
                .unwrap();
            let [end] = kept.ends.as_slice() else {
                panic!("one kept chase end: {kept:?}");
            };
            let end: Value = serde_json::from_str(end).unwrap();
            assert_eq!(end["d"]["op"], "chaser:complete");
            let event = first(&end["d"]["data"]);
            assert_eq!(event["id"], id);
            assert_eq!(event["replayed"], true);
        })
        .await;
}

#[tokio::test]
async fn with_no_browser_the_stop_notice_is_kept_instead_of_sent() {
    LocalSet::new()
        .run_until(async {
            let mut algos = Algos::start().await;
            algos.op("chaser:start", web_chase(None));
            let store = algos
                .next("store", |data| !active_chasers(data).is_empty())
                .await;
            let id = active_chasers(&store["data"])[0]["id"].clone();
            let (kept, notice) = tokio::sync::oneshot::channel();
            let kept = Mutex::new(Some(kept));
            algos
                .inbox
                .send(AlgoInput::Dispose {
                    reason: "no browser for 60s".into(),
                    stopped: Some(Box::new(move |text| {
                        if let Some(kept) = kept.lock().unwrap().take() {
                            let _ = kept.send(text);
                        }
                    })),
                })
                .unwrap();
            let kept = tokio::time::timeout(Duration::from_secs(5), notice)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                kept.notices,
                [algos::Notice {
                    text: "Chase on BTCUSDT stopped: no browser for 60s. Its order may still be open: check it.".into(),
                    algo: Some(algos::AlgoRef {
                        kind: "chase",
                        id: id.as_str().unwrap().to_owned(),
                    }),
                }]
            );
            while let Ok(text) = algos.out.try_recv() {
                let message: Value = serde_json::from_str(&text).unwrap();
                assert_ne!(
                    message["d"]["op"], "error",
                    "nothing is sent to an absent socket"
                );
            }
        })
        .await;
}

/// The page dedupes stop notices by algo: each stopped chase and TWAP is its own
/// notice, its text unchanged, naming its kind and the id its store row had.
#[tokio::test]
async fn each_stopped_chase_and_twap_is_its_own_notice_naming_its_algo() {
    LocalSet::new()
        .run_until(async {
            let mut algos = Algos::start().await;
            algos.op("chaser:start", web_chase(None));
            algos.op("chaser:start", web_chase(None));
            algos.op("twap:start", web_twap());
            let store = algos
                .next("store", |data| {
                    active_chasers(data).len() == 2 && active_twaps(data).len() == 1
                })
                .await;
            let chase = "Chase on BTCUSDT stopped: the server is restarting.";
            let mut expected: Vec<(Value, &str)> = active_chasers(&store["data"])
                .iter()
                .map(|row| (json!({"kind": "chase", "id": row["id"]}), chase))
                .collect();
            let twap = &active_twaps(&store["data"])[0];
            expected.push((
                json!({"kind": "twap", "id": twap["id"]}),
                "TWAP on BTCUSDT stopped: the server is restarting.",
            ));
            algos
                .inbox
                .send(AlgoInput::Dispose {
                    reason: "the server is restarting".into(),
                    stopped: None,
                })
                .unwrap();
            let mut notices = Vec::new();
            let _ = tokio::time::timeout(Duration::from_secs(5), async {
                while let Some(text) = algos.out.recv().await {
                    let message: Value = serde_json::from_str(&text).unwrap();
                    let d = &message["d"];
                    if d["op"] == "error"
                        && d["data"][0]
                            .as_str()
                            .is_some_and(|text| text.contains(" stopped: "))
                    {
                        notices.push(d["data"].clone());
                    }
                }
            })
            .await;
            assert_eq!(notices.len(), expected.len(), "one per algo: {notices:?}");
            for (algo, text) in expected {
                let told: Vec<&Value> = notices
                    .iter()
                    .filter(|data| data[1]["algo"] == algo)
                    .collect();
                assert_eq!(told.len(), 1, "{algo} told once: {notices:?}");
                assert_eq!(told[0][0], text);
                assert_eq!(told[0][1]["forceToast"], true);
            }
        })
        .await;
}

/// One notice per user action: a TWAP's slice chase names its TWAP on its row, and
/// a stop tells the TWAP, not the slice too.
#[tokio::test]
async fn a_twap_slice_chase_names_its_twap_and_its_stop_is_told_by_the_twap() {
    LocalSet::new()
        .run_until(async {
            let mut algos = Algos::start().await;
            algos.op("chaser:start", web_chase(None));
            let mut twap = web_twap();
            twap["useChaserOrders"] = true.into();
            algos.op("twap:start", twap);
            let store = algos
                .next("store", |data| {
                    active_chasers(data).len() == 2 && active_twaps(data).len() == 1
                })
                .await;
            let twap_id = active_twaps(&store["data"])[0]["id"].clone();
            let chasers = active_chasers(&store["data"]);
            let (slices, own): (Vec<&Value>, Vec<&Value>) =
                chasers.iter().partition(|row| row.get("twapId").is_some());
            assert_eq!(slices.len(), 1, "{chasers:?}");
            assert_eq!(slices[0]["twapId"], twap_id);
            assert_eq!(own.len(), 1, "{chasers:?}");
            algos
                .inbox
                .send(AlgoInput::Dispose {
                    reason: "the server is restarting".into(),
                    stopped: None,
                })
                .unwrap();
            let mut told = Vec::new();
            let _ = tokio::time::timeout(Duration::from_secs(5), async {
                while let Some(text) = algos.out.recv().await {
                    let message: Value = serde_json::from_str(&text).unwrap();
                    let d = &message["d"];
                    if d["op"] == "error" && !d["data"][1]["algo"].is_null() {
                        told.push(d["data"][1]["algo"].clone());
                    }
                }
            })
            .await;
            assert_eq!(
                told,
                vec![
                    json!({"kind": "chase", "id": own[0]["id"]}),
                    json!({"kind": "twap", "id": twap_id}),
                ]
            );
        })
        .await;
}

#[tokio::test]
async fn router_fence_parks_a_pending_slice_and_chase_without_any_cancel_or_replay() {
    LocalSet::new()
        .run_until(async {
            let mut algos = Algos::with_held_placements(true).await;
            algos.op("twap:start", web_twap());
            algos.op("chaser:start", web_chase(None));
            tokio::time::timeout(Duration::from_secs(5), async {
                while algos.commands.lock().unwrap().len() < 2 {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            })
            .await
            .unwrap();
            let before = algos.commands.lock().unwrap().clone();
            assert!(
                before
                    .iter()
                    .all(|command| command.starts_with("PlaceOrders")),
                "{before:?}"
            );
            let (stopped, stop) = tokio::sync::oneshot::channel();
            let stopped = Mutex::new(Some(stopped));
            algos
                .inbox
                .send(AlgoInput::RouterFence {
                    stopped: Box::new(move |kept| {
                        if let Some(sender) = stopped.lock().unwrap().take() {
                            let _ = sender.send(kept);
                        }
                    }),
                })
                .unwrap();
            let kept = tokio::time::timeout(Duration::from_secs(2), stop)
                .await
                .unwrap()
                .unwrap();
            assert!(kept.summary().contains("Resting orders were kept"));
            let mut frames = Vec::new();
            tokio::time::timeout(Duration::from_secs(2), async {
                while let Some(text) = algos.out.recv().await {
                    frames.push(serde_json::from_str::<Value>(&text).unwrap());
                }
            })
            .await
            .unwrap();
            let notices: Vec<_> = frames
                .iter()
                .filter(|frame| frame["d"]["op"] == "error")
                .collect();
            assert_eq!(notices.len(), 1);
            assert_eq!(notices[0]["d"]["data"][1]["forceToast"], true);
            assert!(
                frames
                    .iter()
                    .any(|frame| frame["d"]["op"] == "twap:stateUpdate"
                        && frame["d"]["data"]
                            .as_object()
                            .is_some_and(|rows| !rows.is_empty()
                                && rows.values().all(|row| row["state"]["status"] == "paused"))),
                "{frames:?}"
            );
            assert!(
                frames.iter().any(|frame| frame["d"]["op"] == "store"
                    && frame["d"]["data"]["activeChasers"] == json!([])
                    && frame["d"]["data"]["activeTWAPs"]
                        .as_array()
                        .is_some_and(|rows| !rows.is_empty()
                            && rows.iter().all(|row| row["status"] == "paused"))),
                "{frames:?}"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
            assert_eq!(
                *algos.commands.lock().unwrap(),
                before,
                "no cancel, amend, remaining slice or continuation after park"
            );
        })
        .await;
}
