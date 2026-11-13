use super::*;
use crate::accounts::testing;
use crate::delegated::{DelegatedPair, prepare_delegated};
use safe_cex_engine::{HttpRequest, RequestId, SocketId};
use safe_cex_native::{
    BoundHttp, Events, HostEvent, HttpPrepareError, HttpPrepareFuture, PreparedHttp, ServerConfig,
    Transport,
};
use safe_cex_remote_signer::SignerLink;
use std::time::Duration;
use tokio::sync::mpsc;

#[path = "session_delegated_activation_tests.rs"]
mod activation;

#[cfg(feature = "delegated-order-frames")]
#[path = "session_order_frames_tests.rs"]
mod order_frames;

#[cfg(feature = "delegated-order-frames")]
#[path = "session_action_frames_tests.rs"]
mod action_frames;

#[derive(Clone, Default)]
struct Tape {
    writes: Arc<Mutex<Vec<String>>>,
    requests: Arc<Mutex<Vec<Value>>>,
    client_order_ids: Arc<Mutex<Vec<String>>>,
    reduce_only: Arc<Mutex<Vec<bool>>>,
    hold: bool,
    two_orders: bool,
    close_unknown: bool,
    closed_positions: Arc<std::sync::atomic::AtomicBool>,
    defer_positions: Arc<std::sync::atomic::AtomicBool>,
    held_positions: Arc<Mutex<Vec<(RequestId, Events)>>>,
    expected_key: Option<String>,
    waiting: Arc<Mutex<Vec<(RequestId, Events)>>>,
}

fn answer(events: &Events, id: RequestId, body: String) {
    let _ = events.send(HostEvent::HttpStart {
        request: id,
        status: 200,
        headers: String::new(),
    });
    let _ = events.send(HostEvent::HttpChunk(id, body.into_bytes()));
    let _ = events.send(HostEvent::HttpEnd(id));
}

impl Tape {
    #[cfg(feature = "delegated-order-frames")]
    fn release_positions(&self) {
        self.defer_positions
            .store(false, std::sync::atomic::Ordering::SeqCst);
        for (id, events) in self.held_positions.lock().unwrap().drain(..) {
            answer(&events, id, synthetic_positions().to_string());
        }
    }
    fn finish_unknown(&self) {
        for (id, events) in self.waiting.lock().unwrap().drain(..) {
            answer(
                &events,
                id,
                json!({"retCode":0,"retMsg":"OK","result":{}}).to_string(),
            );
        }
    }
}

impl Transport for Tape {
    fn prepare_http(
        &self,
        _: HttpRequest,
        _: safe_cex_engine::request_context::RoutingIntent,
    ) -> HttpPrepareFuture {
        let tape = self.clone();
        Box::pin(async move { Ok(Box::new(PreparedTape(tape)) as Box<dyn PreparedHttp>) })
    }

    fn connect(
        &self,
        socket: SocketId,
        _: String,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let _ = events.send(HostEvent::WsOpened(socket));
        tokio::task::spawn_local(async move {
            while let Some(text) = rx.recv().await {
                let value: Value = serde_json::from_str(&text).unwrap();
                if matches!(value["op"].as_str(), Some("auth" | "subscribe")) {
                    let reply = json!({"success":true,"op":value["op"],"ret_msg":""});
                    let _ = events.send(HostEvent::WsMessage(socket, reply.to_string()));
                }
            }
        });
        tx
    }

    fn request(&self, request: HttpRequest, events: Events) {
        let path = request.url.split('?').next().unwrap();
        let mutation = path.contains("/v5/order/create")
            || path.contains("/v5/order/amend")
            || path.contains("/v5/order/cancel")
            || path.contains("/v5/position/set-leverage")
            || path.contains("/v5/position/switch-mode")
            || path.contains("/v5/account/set-margin-mode")
            || path.contains("/v5/position/trading-stop");
        if mutation {
            self.requests.lock().unwrap().push(json!({"path":path,"body":serde_json::from_str::<Value>(request.body.as_deref().unwrap_or("null")).unwrap()}));
            if path.contains("/create") {
                let body: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
                let rows = body
                    .get("request")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_else(|| vec![body]);
                self.client_order_ids.lock().unwrap().extend(
                    rows.iter()
                        .map(|row| row["orderLinkId"].as_str().unwrap_or_default().to_owned()),
                );
                self.reduce_only.lock().unwrap().extend(
                    rows.iter()
                        .map(|row| row["reduceOnly"].as_bool().unwrap_or(false)),
                );
            }
            assert!(
                request
                    .headers
                    .iter()
                    .any(|(name, value)| name.eq_ignore_ascii_case("X-BAPI-API-KEY")
                        && Some(value) == self.expected_key.as_ref())
            );
            assert!(
                request
                    .headers
                    .iter()
                    .any(|(name, value)| name.eq_ignore_ascii_case("X-BAPI-SIGN")
                        && !value.is_empty())
            );
            self.writes.lock().unwrap().push(
                path.rsplit('/')
                    .next()
                    .unwrap()
                    .split('-')
                    .next()
                    .unwrap()
                    .to_owned(),
            );
            if self.hold {
                self.waiting.lock().unwrap().push((request.id, events));
                return;
            }
            if path.contains("/create") {
                let payload: Value =
                    serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
                let closes = payload["reduceOnly"] == true
                    || payload["request"]
                        .as_array()
                        .is_some_and(|rows| rows.iter().any(|row| row["reduceOnly"] == true));
                if closes && self.close_unknown {
                    answer(
                        &events,
                        request.id,
                        json!({"retCode":0,"retMsg":"OK","result":{}}).to_string(),
                    );
                    return;
                }
                if closes {
                    self.closed_positions
                        .store(true, std::sync::atomic::Ordering::SeqCst);
                }
            }
            let result = if path.ends_with("/cancel-all") {
                json!({"retCode":0,"retMsg":"OK","result":{"list":[{"orderId":"resting-order","orderLinkId":"resting-client"}]}})
            } else if path.ends_with("-batch") {
                let payload: Value =
                    serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
                let rows: Vec<Value> = payload["request"].as_array().unwrap().iter().enumerate().map(|(index, row)| {
                    let fallback = if index == 0 { "native-one".to_owned() } else { format!("native-{}", index + 1) };
                    json!({"orderId":row.get("orderId").and_then(Value::as_str).unwrap_or(&fallback),"orderLinkId":row["orderLinkId"],"symbol":row["symbol"]})
                }).collect();
                let codes: Vec<Value> = rows.iter().map(|_| json!({"code":0,"msg":"OK"})).collect();
                json!({"retCode":0,"retMsg":"OK","result":{"list":rows},"retExtInfo":{"list":codes}})
            } else {
                let payload: Value =
                    serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
                if path.contains("/create") && payload["reduceOnly"] == true {
                    self.closed_positions
                        .store(true, std::sync::atomic::Ordering::SeqCst);
                }
                json!({"retCode":0,"retMsg":"OK","result":{"orderId":payload.get("orderId").and_then(Value::as_str).unwrap_or("native-one")}})
            };
            answer(&events, request.id, result.to_string());
            return;
        }
        let fixtures: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/delegated-bybit-public.json"
        ))
        .unwrap();
        let result = if path.ends_with("/market/instruments-info") {
            if request.url.contains("PreLaunch") {
                json!({"retCode":0,"result":{"category":"linear","list":[]}})
            } else {
                fixtures["instruments"].clone()
            }
        } else if path.ends_with("/market/tickers") {
            fixtures["tickers"].clone()
        } else if path.ends_with("/account/info") {
            json!({"retCode":0,"result":{"marginMode":"REGULAR_MARGIN","unifiedMarginStatus":4}})
        } else if path.ends_with("/account/wallet-balance") {
            json!({"retCode":0,"result":{"list":[{"accountType":"UNIFIED",
                "totalEquity":"10000","totalWalletBalance":"10000","totalMarginBalance":"10000",
                "totalAvailableBalance":"10000","totalPerpUPL":"0",
                "coin":[{"coin":"USDT","walletBalance":"10000","equity":"10000","usdValue":"10000",
                    "availableToWithdraw":"10000","unrealisedPnl":"0","locked":"0"}]}]}})
        } else if path.ends_with("/order/realtime") {
            let mut result = json!({"retCode":0,"result":{"list":[{"orderId":"resting-order","orderLinkId":"resting-client",
                "symbol":"BTCUSDT","orderType":"Limit","side":"Buy","orderStatus":"New",
                "price":"50000","qty":"0.001","cumExecQty":"0","leavesQty":"0.001",
                "reduceOnly":false,"timeInForce":"GTC","positionIdx":0,
                "createdTime":"1000","updatedTime":"1000"}],"nextPageCursor":""}});
            if self.two_orders {
                let mut second = result["result"]["list"][0].clone();
                second["orderId"] = "second-resting-order".into();
                second["orderLinkId"] = "second-resting-client".into();
                result["result"]["list"]
                    .as_array_mut()
                    .unwrap()
                    .push(second);
            }
            result
        } else if path.ends_with("/position/list") {
            if self
                .defer_positions
                .load(std::sync::atomic::Ordering::SeqCst)
            {
                self.held_positions
                    .lock()
                    .unwrap()
                    .push((request.id, events));
                return;
            }
            if self
                .closed_positions
                .load(std::sync::atomic::Ordering::SeqCst)
            {
                json!({"retCode":0,"result":{"list":[],"nextPageCursor":""}})
            } else {
                synthetic_positions()
            }
        } else {
            json!({"retCode":0,"retMsg":"OK","result":{"list":[]}})
        };
        answer(&events, request.id, result.to_string());
    }
}

fn synthetic_positions() -> Value {
    json!({"retCode":0,"result":{"list":[{"symbol":"BTCUSDT","positionIdx":0,
        "side":"Buy","size":"0.001","leverage":"10","avgPrice":"50000",
        "markPrice":"50000","positionValue":"50","positionIM":"5",
        "unrealisedPnl":"0","cumRealisedPnl":"0","liqPrice":"0"}],"nextPageCursor":""}})
}

struct PreparedTape(Tape);
struct BoundTape(Tape, HttpRequest);

impl std::fmt::Debug for PreparedTape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PreparedTape")
    }
}
impl std::fmt::Debug for BoundTape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BoundTape")
    }
}
impl PreparedHttp for PreparedTape {
    fn bind(self: Box<Self>, packet: HttpRequest) -> Result<Box<dyn BoundHttp>, HttpPrepareError> {
        Ok(Box::new(BoundTape(self.0, packet)))
    }
}
impl BoundHttp for BoundTape {
    fn ready(&self) -> bool {
        true
    }
    fn dispatch(self: Box<Self>, events: Events) -> tokio::task::AbortHandle {
        self.0.request(self.1, events);
        tokio::spawn(async {}).abort_handle()
    }
}

fn init(venue: &str) -> InitPayload {
    InitPayload {
        account_name: "synthetic-main".into(),
        exchange_name: venue.into(),
        key: "synthetic-key".into(),
        ..Default::default()
    }
}

fn delegated_account(registry: &Registry, user: &str, venue: &str, tape: Tape) -> Arc<Account> {
    delegated_account_with_algo_inbox(registry, user, venue, tape).0
}

fn delegated_account_with_algos(
    registry: &Registry,
    user: &str,
    venue: &str,
    tape: Tape,
    start_algos: bool,
) -> Arc<Account> {
    let (account, inbox) = delegated_account_with_algo_inbox(registry, user, venue, tape);
    if start_algos {
        testing::run_algos(account, SignerLink::new(0, Arc::new(|_| false)), inbox)
    } else {
        account
    }
}

fn delegated_account_with_algo_inbox(
    registry: &Registry,
    user: &str,
    venue: &str,
    tape: Tape,
) -> (
    Arc<Account>,
    mpsc::UnboundedReceiver<crate::algos::AlgoInput>,
) {
    let mut setup = init("bybit");
    setup.key = format!("synthetic-key-{user}");
    let mut tape = tape;
    tape.expected_key = Some(setup.key.clone());
    let prepared = prepare_delegated(
        &setup,
        DelegatedPair::new(
            setup.key.clone(),
            format!("synthetic-signing-fixture-{user}"),
            None,
        ),
    )
    .unwrap_or_else(|_| panic!("synthetic Bybit setup"));
    let (handle, _) = safe_cex_native::spawn_venue_account(
        "bybit",
        &prepared.options,
        tape,
        Some(prepared.resident.provider()),
        ServerConfig::default(),
    )
    .unwrap();
    let (mut account, inbox) = testing::account_on(
        testing::HeldLoad {
            placed: Arc::default(),
            load_after_ms: None,
            ticker: false,
        },
        (user, "synthetic-main", venue),
        Some(SignerLink::new(0, Arc::new(|_| false))),
        crate::delegated::fingerprint(&setup),
        Arc::new(crate::algos::Running::new(Arc::new(
            tokio::sync::watch::channel(0).0,
        ))),
    );
    let inner = Arc::get_mut(&mut account).unwrap();
    inner.handle.dispose();
    inner.handle = handle;
    inner.link = None;
    inner.delegated = Some(prepared.resident);
    if venue == "bybit" {
        testing::bind_identity(&mut account, &setup);
    }
    testing::register(registry, &account);
    (account, inbox)
}

#[tokio::test]
async fn delegated_fixture_keeps_identity_when_native_algos_are_enabled_or_disabled() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for start_algos in [false, true] {
                let registry = crate::accounts::tests::registry();
                let account = delegated_account_with_algos(
                    &registry,
                    "A",
                    "bybit",
                    Tape::default(),
                    start_algos,
                );
                let mut renamed = init("bybit");
                renamed.key = "synthetic-key-A".into();
                renamed.account_name = "renamed-label".into();
                let identity = crate::accounts::identity::digest("A", &renamed)
                    .unwrap()
                    .unwrap();
                assert_eq!(account.registry_name(), identity);
                assert!(testing::holdings(&registry).contains_key(&("A".into(), identity)));
                let holder = registry.next_id();
                let joined = registry
                    .obtain_delegated(
                        "A",
                        &renamed,
                        holder,
                        None,
                        Arc::new(|_| true),
                        DelegatedPair::new(
                            renamed.key.clone(),
                            "synthetic-signing-fixture-A".into(),
                            None,
                        ),
                    )
                    .await
                    .unwrap();
                assert!(Arc::ptr_eq(&account, &joined));
                assert!(Arc::ptr_eq(
                    &account,
                    &registry.held("A", &renamed.account_name, holder).unwrap()
                ));
                assert!(registry.held("B", &renamed.account_name, holder).is_none());
                assert_eq!(registry.account_count(), 1);
                registry.dispose(&account, Disposal::PageDisposed);
            }
        })
        .await;
}

async fn joined(
    registry: Arc<Registry>,
    user: &str,
    account: Arc<Account>,
) -> (Session, crate::out::Receiver) {
    let (mut session, sent) = joined_without_runtime_opt_in(registry, user, account).await;
    session.order_frames_enabled = crate::order_contract::enabled();
    (session, sent)
}

async fn joined_without_runtime_opt_in(
    registry: Arc<Registry>,
    user: &str,
    account: Arc<Account>,
) -> (Session, crate::out::Receiver) {
    let (out, sent, _) = Out::channel(256);
    let mut session = Session::new(
        registry,
        Arc::new(Metrics::default()),
        Identity {
            user: Some(user.into()),
            chat_handle: None,
            token: None,
            resume_from: None,
            resumable: false,
        },
        out.clone(),
    );
    crate::accounts::tests::hold_live(&account, session.socket, out.outbox());
    session
        .join(account.clone(), out.with_instance_id(None))
        .await;
    if account.delegated.is_some() {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if account
                    .handle
                    .read(|store| {
                        store.loaded().orders
                            && store.loaded().positions
                            && store.positions().iter().any(|p| p.symbol == "BTCUSDT")
                    })
                    .await
                    == Some(true)
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("synthetic private startup completes");
    }
    (session, sent)
}

fn placement() -> Value {
    json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.001,"price":50000})
}

async fn call(session: &mut Session, request: &str, method: &str, args: Value) {
    session
        .handle(
            &json!({"t":"msg","d":{"accountName":"synthetic-main","op":"callMethod",
        "data":{"requestId":request,"method":method,"args":args}}})
            .to_string(),
        )
        .await
        .unwrap();
}

async fn reply(sent: &mut crate::out::Receiver, request: &str) -> Value {
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(text) = sent.recv().await {
            let value: Value = serde_json::from_str(&text).unwrap();
            assert_ne!(value["t"], "sign-req");
            if value["d"]["op"] == "callMethod:done" && value["d"]["data"]["requestId"] == request {
                return value["d"]["data"].clone();
            }
        }
        panic!("call reply missing")
    })
    .await
    .unwrap_or_else(|_| panic!("bounded call reply for {request}"))
}

#[tokio::test]
async fn delegated_manual_writes_require_hosted_venue_support() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let tape = Tape::default();
            let account = delegated_account(&registry, "A", "hyrotrader", tape.clone());
            let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
            call(&mut session, "flag", "placeOrder", json!([placement()])).await;
            let result = reply(&mut sent, "flag").await;
            assert_eq!(result["success"], false);
            assert_eq!(result["result"]["name"], "UnsupportedMethod");
            assert!(tape.writes.lock().unwrap().is_empty());
            registry.dispose(&account, Disposal::PageDisposed);
        })
        .await;
}

#[tokio::test]
async fn delegated_calls_use_the_resident_provider_and_existing_order_audit() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let tape = Tape::default();
            let account = delegated_account(&registry, "A", "bybit", tape.clone());
            let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
            call(&mut session, "place", "placeOrder", json!([placement()])).await;
            {
                let pending = session.joined["synthetic-main"]
                    .client
                    .pending
                    .lock()
                    .unwrap();
                let audit = pending
                    .values()
                    .find_map(|entry| entry.audit.as_ref())
                    .expect("existing audit attached");
                assert_eq!(
                    (&*audit.who.user, &*audit.who.account, &*audit.who.venue),
                    ("A", "synthetic-main", "bybit")
                );
                assert_eq!(audit.fields["op"], "placeOrders");
            }
            assert_eq!(
                reply(&mut sent, "place").await,
                json!({"requestId":"place","success":true,"result":["native-one"]})
            );
            let mut close = placement();
            close["reduceOnly"] = true.into();
            call(&mut session, "close", "placeOrder", json!([close])).await;
            assert_eq!(reply(&mut sent, "close").await["success"], true);
            call(
                &mut session,
                "amend",
                "updateOrder",
                json!([{"order":{"id":"resting-order","symbol":"BTCUSDT"},"update":{"price":51000}}]),
            )
            .await;
            let refused = reply(&mut sent, "amend").await;
            assert_eq!(refused["success"], false);
            assert_eq!(refused["result"]["name"], "DelegatedActionUnavailable");
            call(
                &mut session,
                "cancel",
                "cancelOrders",
                json!([[{"id":"resting-order","symbol":"BTCUSDT"}]]),
            )
            .await;
            let refused = reply(&mut sent, "cancel").await;
            assert_eq!(refused["success"], false);
            assert_eq!(refused["result"]["name"], "DelegatedActionUnavailable");
            assert_eq!(*tape.writes.lock().unwrap(), ["create", "create"]);
            assert_eq!(*tape.reduce_only.lock().unwrap(), [false, true]);
            assert!(account.link.is_none());
            registry.dispose(&account, Disposal::PageDisposed);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn delegated_unknown_is_terminal_without_automatic_resend() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let tape = Tape {
                hold: true,
                ..Default::default()
            };
            let account = delegated_account(&registry, "A", "bybit", tape.clone());
            let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
            call(&mut session, "unknown", "placeOrder", json!([placement()])).await;
            for _ in 0..100 {
                tokio::task::yield_now().await;
                if !tape.waiting.lock().unwrap().is_empty() {
                    break;
                }
            }
            assert_eq!(tape.writes.lock().unwrap().len(), 1);
            tape.finish_unknown();
            let result = reply(&mut sent, "unknown").await;
            assert_eq!(result["success"], false);
            assert_eq!(result["result"]["outcome"], "unknown");
            assert!(
                result["result"]["message"]
                    .as_str()
                    .is_some_and(|m| !m.is_empty())
            );
            tokio::time::advance(Duration::from_secs(120)).await;
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            assert_eq!(tape.writes.lock().unwrap().len(), 1);
            assert!(
                session.joined["synthetic-main"]
                    .client
                    .pending
                    .lock()
                    .unwrap()
                    .is_empty()
            );
            let (_rejoined, _resumed_frames) = joined(registry.clone(), "A", account.clone()).await;
            tokio::time::advance(Duration::from_secs(30)).await;
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            assert_eq!(tape.writes.lock().unwrap().len(), 1);
            registry.dispose(&account, Disposal::PageDisposed);
        })
        .await;
}

#[tokio::test]
async fn delegated_write_completion_keeps_its_user_during_a_login_switch() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let first = Tape {
                hold: true,
                ..Default::default()
            };
            let a = delegated_account(&registry, "A", "bybit", first.clone());
            let (mut old, mut old_frames) = joined(registry.clone(), "A", a.clone()).await;
            call(&mut old, "from-A", "placeOrder", json!([placement()])).await;
            for _ in 0..100 {
                tokio::task::yield_now().await;
                if !first.waiting.lock().unwrap().is_empty() {
                    break;
                }
            }
            assert_eq!(first.writes.lock().unwrap().len(), 1);
            let second = Tape::default();
            let b = delegated_account(&registry, "B", "bybit", second.clone());
            let (mut current, mut current_frames) = joined(registry.clone(), "B", b.clone()).await;
            call(&mut current, "from-B", "placeOrder", json!([placement()])).await;
            assert_eq!(reply(&mut current_frames, "from-B").await["success"], true);
            {
                let pending = old.joined["synthetic-main"].client.pending.lock().unwrap();
                assert_eq!(
                    pending
                        .values()
                        .next()
                        .unwrap()
                        .audit
                        .as_ref()
                        .unwrap()
                        .who
                        .user,
                    "A"
                );
            }
            first.finish_unknown();
            assert_eq!(
                reply(&mut old_frames, "from-A").await["result"]["outcome"],
                "unknown"
            );
            while let Ok(text) = current_frames.try_recv() {
                let value: Value = serde_json::from_str(&text).unwrap();
                assert_ne!(value["d"]["data"]["requestId"], "from-A");
            }
            assert_eq!(first.writes.lock().unwrap().len(), 1);
            assert_eq!(second.writes.lock().unwrap().len(), 1);
            registry.dispose(&a, Disposal::PageDisposed);
            registry.dispose(&b, Disposal::PageDisposed);
        })
        .await;
}

#[tokio::test]
async fn cosigned_call_method_reply_keeps_its_existing_bytes() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let account = testing::trading_account(&registry, "A", &init("bybitspot"));
            let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
            call(&mut session, "legacy", "placeOrder", json!([placement()])).await;
            assert_eq!(
                reply(&mut sent, "legacy").await.to_string(),
                r#"{"requestId":"legacy","result":["o-0"],"success":true}"#
            );
            assert!(account.delegated.is_none());
            assert!(account.link.is_some());
            registry.dispose(&account, Disposal::PageDisposed);
        })
        .await;
}

#[cfg(not(feature = "delegated-order-frames"))]
#[tokio::test]
async fn disabled_order_frames_refuse_even_an_existing_delegated_resident() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = crate::accounts::tests::registry();
            let tape = Tape::default();
            let account = delegated_account(&registry, "A", "bybit", tape.clone());
            let (mut session, mut sent) = joined(registry.clone(), "A", account.clone()).await;
            let f: Value =
                serde_json::from_str(include_str!("../tests/data/delegated-orders-v1.json"))
                    .unwrap();
            let mut submit = f["submit"].clone();
            submit["d"]["accountName"] = "synthetic-main".into();
            session.handle(&submit.to_string()).await.unwrap();
            let outcome = tokio::time::timeout(Duration::from_secs(2), async {
                while let Some(text) = sent.recv().await {
                    let v: Value = serde_json::from_str(&text).unwrap();
                    if v["t"] == "order.outcome" {
                        return v;
                    }
                }
                panic!("missing visible capability refusal")
            })
            .await
            .unwrap();
            assert_eq!(
                outcome["d"]["error"]["message"],
                "not sent: account not admitted for order frames"
            );
            assert!(tape.writes.lock().unwrap().is_empty());
            session.close();
            registry.dispose(&account, Disposal::PageDisposed);
        })
        .await;
}

#[tokio::test]
async fn venue_declaration_controls_existing_order_admission() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for opted_in in [false, true] {
                crate::delegated::with_test_declaration(
                    json!({"venues": {
                        "bybit": {"remoteSigning": {"enabled": true}},
                        "binance": {"remoteSigning": {"enabled": opted_in}}
                    }}),
                    async {
                        let registry = crate::accounts::tests::registry();
                        let account =
                            delegated_account(&registry, "fixture", "binance", Tape::default());
                        let (mut session, _sent) =
                            joined(registry.clone(), "fixture", account.clone()).await;
                        assert_eq!(
                            session.order_admitted("synthetic-main", None),
                            cfg!(feature = "delegated-order-frames") && opted_in
                        );
                        session.close();
                        registry.dispose(&account, Disposal::PageDisposed);
                    },
                )
                .await;
            }
        })
        .await;
}
