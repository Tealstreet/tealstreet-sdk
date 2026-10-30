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
        cx.done(id, result);
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
}

impl Algos {
    async fn start() -> Self {
        let engine = safe_cex_engine::test_support::seeded_engine(AlgoVenue {
            book: Book::default(),
            commands: Arc::default(),
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
            Box::new(move |text| {
                let _ = tx.send(text);
            }),
            None,
            PlacedSymbols::default(),
            Arc::new(AtomicBool::new(true)),
            inbox_rx,
        ));
        // The feed's join snapshot loads the market before any op.
        tokio::time::sleep(Duration::from_millis(100)).await;
        Self {
            inbox,
            out,
            seen: Vec::new(),
        }
    }

    /// As `session.rs`: the web's `{op, data}` with `data` re-serialized.
    fn op(&self, op: &str, data: Value) {
        self.inbox
            .send(AlgoInput::Op {
                op: op.into(),
                data: data.to_string(),
                origin: "socket:1".into(),
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
async fn twap_state_restores_the_rows_its_own_state_updates_persisted() {
    LocalSet::new()
        .run_until(async {
            let mut first = Algos::start().await;
            first.op("twap:start", web_twap());
            let update = first.next("twap:stateUpdate", |_| true).await;
            let (id, row) = update["data"]
                .as_object()
                .unwrap()
                .iter()
                .next()
                .map(|(id, row)| (id.clone(), row.clone()))
                .unwrap();
            // TwapWorker.tsx: [{id, options, state}] from sessionStorage, to a fresh host.
            let mut restored = Algos::start().await;
            restored.op(
                "twap:state",
                json!([{"id": id, "options": row["options"], "state": row["state"]}]),
            );
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
                json!([{"id":"ts-twap-1","options":options,"state":state}]),
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
