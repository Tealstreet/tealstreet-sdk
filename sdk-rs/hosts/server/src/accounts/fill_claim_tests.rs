//! AGT-196 end to end: two Tealstreet logins connect ONE OKX venue account,
//! each with its own native account actor, ledger, algo host and fill tap.
//! User A places; both residents see every row and fill on the account; only
//! A's claims, and B amending A's orders takes nothing over.

use super::testing::{HeldLoad, account_sharing, run_algos_notifying};
use super::*;
use safe_cex_engine::model::{Order, OrderSide, OrderStatus, OrderType};
use safe_cex_engine::{Command, Cx, Fill, OrderIdentityKind, OrderIdentityReceipt};

// OKX's identities in the shapes its adapter reports them, taken from OKX's
// published `orders` push example (docs-v5 "WS / Order channel", the frame in
// `venues/okx/tests/account.rs` `TRIGGERED_STOP_MARKET_FILLED`): docs-derived,
// not a live capture. That test proves the real OKX parser turns the frame into
// exactly this fill and `OrderTriggered` notice; the second stop, the limit and
// the other orders reuse its id shapes.
const SYMBOL: &str = "BTCUSDT";
const STOP_ALGO: &str = "452197707845865000";
const STOP_ALGO_CLIENT: &str = "0f6b1d2c3e4a5b6c7d8e9f0a1b2c3d4e";
const STOP_CHILD: &str = "452197707845865472";
const LOST_ALGO: &str = "452197707845865100";
const LOST_ALGO_CLIENT: &str = "1f6b1d2c3e4a5b6c7d8e9f0a1b2c3d4e";
const LOST_CHILD: &str = "452197707845865573";
const LIMIT: &str = "452197707845865900";
const LIMIT_CLIENT: &str = "2f6b1d2c3e4a5b6c7d8e9f0a1b2c3d4e";
/// A placed this before the box rolled: it rests on the account in no ledger.
const BEFORE_ROLL: &str = "452197707845866000";
/// An OKX `full_liquidation` fill: nobody placed it.
const LIQUIDATION: &str = "452197707845867000";

/// What the venue account holds, shared by both logins' private streams.
#[derive(Default)]
struct VenueAccount {
    rows: Vec<Order>,
}

/// One login's view of the shared OKX account. Placements answer as OKX's
/// adapter does: a stop-market TP/SL reports its `algoClOrdId` receipt and
/// completes with its `algoId`, or never completes (Unknown); a limit reports
/// its `clOrdId` and completes with its `ordId`; an amend keeps the `ordId`
/// (`Finish::Amend`). `FetchLeverage` stands in for the private stream:
/// "rows" delivers every resting row, `SYMBOL` every fill.
struct SharedOkxAccount {
    load: HeldLoad,
    account: Arc<Mutex<VenueAccount>>,
}

fn receipt(cx: &mut Cx<'_, ()>, command: u32, identity: &str) {
    cx.command_progress(
        command,
        vec![OrderIdentityReceipt {
            symbol: SYMBOL.into(),
            kind: OrderIdentityKind::ClientOrderId,
            identity: identity.into(),
        }],
    );
}

fn fill(cx: &mut Cx<'_, ()>, symbol: &str, order_id: &str, trade: &str) {
    cx.fill(Fill {
        id: Some(trade.into()),
        order_id: order_id.into(),
        symbol: symbol.into(),
        side: OrderSide::Sell,
        price: 31_527.1,
        amount: 0.01,
    });
}

fn resting(id: &str, client: Option<&str>) -> Order {
    Order {
        id: id.into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: SYMBOL.into(),
        kind: OrderType::Limit,
        side: OrderSide::Sell,
        price: 32_000.,
        amount: 0.01,
        filled: 0.,
        remaining: 0.01,
        reduce_only: false,
        close: None,
        client_order_id: client.map(Into::into),
        time_in_force: None,
        last_update: 0.,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    }
}

impl safe_cex_engine::Venue for SharedOkxAccount {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        self.load.start(cx);
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: safe_cex_engine::SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: safe_cex_engine::SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: safe_cex_engine::SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: safe_cex_engine::Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_command(&mut self, cx: &mut Cx<'_, ()>, command: Command) {
        match command {
            Command::PlaceOrders { command, orders } => match orders[0].kind {
                OrderType::StopLoss if orders[0].amount > 0.01 => {
                    receipt(cx, command, LOST_ALGO_CLIENT);
                }
                OrderType::StopLoss => {
                    receipt(cx, command, STOP_ALGO_CLIENT);
                    cx.done(command, format!(r#"["{STOP_ALGO}"]"#));
                }
                _ => {
                    receipt(cx, command, LIMIT_CLIENT);
                    self.account
                        .lock()
                        .unwrap()
                        .rows
                        .push(resting(LIMIT, Some(LIMIT_CLIENT)));
                    cx.done(command, format!(r#"["{LIMIT}"]"#));
                }
            },
            Command::AmendOrder { command, id, .. } => {
                cx.done(command, format!(r#"["{id}"]"#));
            }
            Command::FetchLeverage { command, symbol } if symbol == "rows" => {
                for row in &self.account.lock().unwrap().rows {
                    cx.store.upsert_order(row.clone());
                }
                cx.done(command, "null".into());
            }
            Command::FetchLeverage { command, .. } => {
                // Each stop's market child fills in its first frame, so no row
                // is ever stored for it; its fill names the algo.
                fill(cx, SYMBOL, STOP_CHILD, "242589207");
                cx.order_triggered(SYMBOL, STOP_CHILD, [STOP_ALGO, STOP_ALGO_CLIENT]);
                fill(cx, SYMBOL, LOST_CHILD, "242589208");
                cx.order_triggered(SYMBOL, LOST_CHILD, [LOST_ALGO, LOST_ALGO_CLIENT]);
                fill(cx, SYMBOL, LIMIT, "242589209");
                fill(cx, SYMBOL, BEFORE_ROLL, "242589210");
                fill(cx, SYMBOL, LIQUIDATION, "242589211");
                // Another instrument's order that happens to share LIMIT's id.
                fill(cx, "ETHUSDT", LIMIT, "242589212");
                cx.done(command, "null".into());
            }
            other => {
                if let Some(id) = other.completion_id() {
                    cx.done(id, "null".into());
                }
            }
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, ()>, timer: safe_cex_engine::TimerId) {
        self.load.on_timer(cx, timer);
    }
    fn book(&self, _: &str) -> Option<&safe_cex_engine::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

struct Login {
    account: Arc<Account>,
    _client: safe_cex_native::ClientLink,
    notifier: crate::host_notify::HostNotifier,
    scope: crate::host_notify::Scope,
}

const CLIENT: u64 = 7;

/// Both logins' inits for the shared account: their own labels, one key.
fn okx_init(label: &str, key: &str) -> InitPayload {
    InitPayload {
        account_name: label.into(),
        exchange_name: "okx".into(),
        key: key.into(),
        ..Default::default()
    }
}

async fn login(user: &str, init: &InitPayload, account: Arc<Mutex<VenueAccount>>) -> Login {
    let link = SignerLink::new(7, Arc::new(|_| true));
    let venue = SharedOkxAccount {
        load: HeldLoad {
            placed: Arc::default(),
            load_after_ms: Some(0.),
            ticker: true,
        },
        account,
    };
    // The production derivation, as `build` runs it for each login's init.
    let venue_account = shared_venue_account(user, "okx", &init.account_name, init);
    let (account, inbox) = account_sharing(
        venue,
        (user, &init.account_name, "okx"),
        Some(link.clone()),
        String::new(),
        Arc::default(),
        venue_account,
    );
    let notifier = crate::host_notify::HostNotifier::unstarted();
    let scope = account.notify_scope();
    let notify = crate::algo_notify::AlgoNotify::new(
        notifier.clone(),
        scope.clone(),
        Arc::new(|| true),
        account.held.clone(),
    );
    let account = run_algos_notifying(account, link, inbox, Some(notify));
    let client = account
        .handle
        .join(CLIENT, true)
        .await
        .expect("client joins");
    Login {
        account,
        _client: client,
        notifier,
        scope,
    }
}

fn place(login: &Login, command: u32, order: Value) {
    let order = serde_json::from_value(order).unwrap();
    assert!(login.account.handle.command(
        CLIENT,
        Command::PlaceOrders {
            command,
            orders: vec![order],
        },
    ));
}

fn amend(login: &Login, command: u32, id: &str) {
    assert!(login.account.handle.command(
        CLIENT,
        Command::AmendOrder {
            command,
            id: id.into(),
            price: Some(31_000.),
            amount: None,
        },
    ));
}

fn stream(login: &Login, command: u32, what: &str) {
    assert!(login.account.handle.command(
        CLIENT,
        Command::FetchLeverage {
            command,
            symbol: what.into(),
        },
    ));
}

/// Whether this login's resident claimed each reported fill, by (symbol, id).
fn claims(login: &Login, fills: &[(&str, &str)]) -> Vec<bool> {
    let sent: Vec<Value> = login
        .notifier
        .queued()
        .iter()
        .map(|notification| serde_json::to_value(notification).unwrap())
        .filter(|notification| notification["kind"] == "fill")
        .collect();
    assert_eq!(sent.len(), fills.len(), "every fill is reported: {sent:?}");
    fills
        .iter()
        .map(|(symbol, id)| {
            let key = crate::host_notify::fill_key(&login.scope, symbol, id);
            sent.iter()
                .find(|notification| notification["fillKey"] == key.as_str())
                .unwrap_or_else(|| panic!("{symbol} {id}'s fill carries the shared key"))
                ["placedHere"]
                .as_bool()
                .unwrap()
        })
        .collect()
}

#[tokio::test(start_paused = true)]
async fn only_the_placing_login_claims_fills_on_a_shared_venue_account() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let shared = Arc::new(Mutex::new(VenueAccount {
                rows: vec![resting(BEFORE_ROLL, None)],
            }));
            let key = "synthetic-shared-okx-key";
            let a = login("auth0|placer", &okx_init("A's label", key), shared.clone()).await;
            let b = login("auth0|other", &okx_init("B's label", key), shared).await;
            tokio::time::sleep(Duration::from_millis(200)).await;
            assert_ne!(a.scope.account_digest, b.scope.account_digest);
            assert_eq!(
                a.scope.venue_account, b.scope.venue_account,
                "one key is one venue account, whoever connects it"
            );
            assert_ne!(
                shared_venue_account("auth0|other", "okx", "B's label", &okx_init("B's label", "another-key")),
                a.scope.venue_account,
                "another key is another venue account"
            );

            let stop = |amount: f64| {
                json!({"symbol": SYMBOL, "type": "stop_market", "side": "sell",
                    "amount": amount, "price": 30_000.0, "reduceOnly": true})
            };
            place(&a, 1, stop(0.01));
            place(&a, 2, stop(0.02));
            place(
                &a,
                3,
                json!({"symbol": SYMBOL, "type": "limit", "side": "sell", "amount": 0.01, "price": 32_000.0}),
            );
            tokio::time::sleep(Duration::from_millis(100)).await;

            // B's stream carries A's limit and the pre-roll order; B amends both.
            stream(&b, 8, "rows");
            tokio::time::sleep(Duration::from_millis(100)).await;
            amend(&b, 9, LIMIT);
            amend(&b, 10, BEFORE_ROLL);
            tokio::time::sleep(Duration::from_millis(100)).await;

            for login in [&a, &b] {
                stream(login, 11, SYMBOL);
            }
            tokio::time::sleep(crate::algo_notify::FILL_DEBOUNCE * 3).await;

            let fills = [
                (SYMBOL, STOP_CHILD),
                (SYMBOL, LOST_CHILD),
                (SYMBOL, LIMIT),
                (SYMBOL, BEFORE_ROLL),
                (SYMBOL, LIQUIDATION),
                ("ETHUSDT", LIMIT),
            ];
            for (symbol, id) in fills {
                assert_eq!(
                    crate::host_notify::fill_key(&a.scope, symbol, id),
                    crate::host_notify::fill_key(&b.scope, symbol, id),
                    "one key per venue order on both residents"
                );
            }
            assert_eq!(
                claims(&a, &fills),
                [true, true, true, false, false, false],
                "A claims its stop children (by algoId, and by algoClOrdId alone) and \
                 its limit, even after B amended it; never the pre-roll order, the \
                 liquidation, or another symbol's order with its limit's id"
            );
            assert_eq!(
                claims(&b, &fills),
                [false; 6],
                "B claims nothing it did not place, including the orders it amended"
            );
        })
        .await;
}

/// Review A: a key the venue-account hash cannot read falls back to this
/// login's own account, and says so with a closed cause and no secrets.
#[test]
fn an_unreadable_key_falls_back_per_login_and_is_logged() {
    let capture = crate::state::log_capture::capture();
    let init = okx_init("label", "");
    let a = shared_venue_account("auth0|a", "okx", "label", &init);
    let b = shared_venue_account("auth0|b", "okx", "label", &init);
    assert_ne!(
        a, b,
        "the fallback is per login, so nothing is claimed across"
    );
    let lines = capture.finish();
    assert_eq!(lines.len(), 2, "{lines:?}");
    for line in lines {
        assert!(line.contains("cause=unreadable_key venue=okx"), "{line}");
        assert!(!line.contains("auth0|"), "{line}");
    }
}
