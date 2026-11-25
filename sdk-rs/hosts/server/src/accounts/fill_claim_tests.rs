//! AGT-196 end to end: two Tealstreet logins connect ONE venue account, each
//! with its own native account actor, ledger, algo host and fill tap. User A
//! places; both residents see every fill on the account; only A's claims.

use super::testing::{HeldLoad, account_on, run_algos_notifying};
use super::*;
use safe_cex_engine::model::OrderSide;
use safe_cex_engine::{Command, Cx, Fill, OrderIdentityKind, OrderIdentityReceipt};

/// The venue account both logins hold. Placements answer as the real venues
/// do: an OKX stop completes with its `algoId` after its `algoClOrdId`
/// receipt, or never completes (Unknown); a Hyperliquid amend completes with
/// its replacement's `{orderIds, clientOrderId}` object. `FetchLeverage` stands
/// in for the account's private stream, which both residents receive alike.
struct SharedVenueAccount {
    load: HeldLoad,
    placements: u32,
}

const SYMBOL: &str = "BTCUSDT";

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

fn fill(cx: &mut Cx<'_, ()>, order_id: &str) {
    cx.fill(Fill {
        id: Some(format!("exec-{order_id}")),
        order_id: order_id.into(),
        symbol: SYMBOL.into(),
        side: OrderSide::Sell,
        price: 77_000.,
        amount: 0.001,
    });
}

impl safe_cex_engine::Venue for SharedVenueAccount {
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
            Command::PlaceOrders { command, .. } => {
                self.placements += 1;
                match self.placements {
                    // An OKX stop-market TP/SL: receipt, then its algoId.
                    1 => {
                        receipt(cx, command, "stop-cl-1");
                        cx.done(command, r#"["algo-1"]"#.into());
                    }
                    // An OKX stop whose acknowledgement was lost: receipt only.
                    2 => receipt(cx, command, "stop-cl-2"),
                    // A Hyperliquid limit, amended below.
                    _ => {
                        receipt(cx, command, "0xcloid-hl-1");
                        cx.done(command, r#"["hl-1"]"#.into());
                    }
                }
            }
            Command::AmendOrder { command, .. } => {
                receipt(cx, command, "0xcloid-hl-2");
                cx.done(
                    command,
                    r#"{"orderIds":["hl-2"],"clientOrderId":"0xcloid-hl-2"}"#.into(),
                );
            }
            Command::FetchLeverage { command, .. } => {
                // Each stop's market child fills in its first frame, so the
                // venue stores no row for it; the fill names its algo.
                fill(cx, "okx-child-1");
                cx.order_triggered("okx-child-1", ["algo-1", "stop-cl-1"]);
                fill(cx, "okx-child-2");
                cx.order_triggered("okx-child-2", ["algo-2", "stop-cl-2"]);
                // The amend's replacement, whose filled row HL already removed.
                fill(cx, "hl-2");
                // Nobody placed this one through Tealstreet (a liquidation).
                fill(cx, "liquidation-1");
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

async fn login(user: &str) -> Login {
    let link = SignerLink::new(7, Arc::new(|_| true));
    let venue = SharedVenueAccount {
        load: HeldLoad {
            placed: Arc::default(),
            load_after_ms: Some(0.),
            ticker: true,
        },
        placements: 0,
    };
    // Both logins name the venue account "shared" (`account_on`), whoever they are.
    let (account, inbox) = account_on(
        venue,
        (user, "shared", "okx"),
        Some(link.clone()),
        String::new(),
        Arc::default(),
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

/// Every fill this login's resident reported, by order id: whether it claimed it.
fn claims(login: &Login, order_ids: &[&str]) -> Vec<(String, bool)> {
    let sent: Vec<Value> = login
        .notifier
        .queued()
        .iter()
        .map(|notification| serde_json::to_value(notification).unwrap())
        .filter(|notification| notification["kind"] == "fill")
        .collect();
    assert_eq!(
        sent.len(),
        order_ids.len(),
        "every fill is reported: {sent:?}"
    );
    order_ids
        .iter()
        .map(|id| {
            let key = crate::host_notify::fill_key(&login.scope, SYMBOL, id);
            let claimed = sent
                .iter()
                .find(|notification| notification["fillKey"] == key.as_str())
                .unwrap_or_else(|| panic!("{id}'s fill carries the shared key"))["placedHere"]
                .as_bool()
                .unwrap();
            (id.to_string(), claimed)
        })
        .collect()
}

#[tokio::test(start_paused = true)]
async fn only_the_placing_login_claims_fills_on_a_shared_venue_account() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let a = login("auth0|placer").await;
            let b = login("auth0|other").await;
            tokio::time::sleep(Duration::from_millis(200)).await;
            assert_ne!(a.scope.account_digest, b.scope.account_digest);
            assert_eq!(a.scope.venue_account, b.scope.venue_account);

            let stop = json!({"symbol": SYMBOL, "type": "stop_market", "side": "sell",
                "amount": 0.001, "price": 76_000.0, "reduceOnly": true});
            place(&a, 1, stop.clone());
            place(&a, 2, stop);
            place(
                &a,
                3,
                json!({"symbol": SYMBOL, "type": "limit", "side": "sell", "amount": 0.001, "price": 78_000.0}),
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
            assert!(a.account.handle.command(
                CLIENT,
                Command::AmendOrder {
                    command: 4,
                    id: "hl-1".into(),
                    price: Some(77_000.),
                    amount: None,
                },
            ));
            tokio::time::sleep(Duration::from_millis(100)).await;

            // The venue account's private stream reaches both residents.
            for login in [&a, &b] {
                assert!(login.account.handle.command(
                    CLIENT,
                    Command::FetchLeverage {
                        command: 9,
                        symbol: SYMBOL.into(),
                    },
                ));
            }
            tokio::time::sleep(crate::algo_notify::FILL_DEBOUNCE * 3).await;

            let ids = ["okx-child-1", "okx-child-2", "hl-2", "liquidation-1"];
            for id in ids {
                assert_eq!(
                    crate::host_notify::fill_key(&a.scope, SYMBOL, id),
                    crate::host_notify::fill_key(&b.scope, SYMBOL, id),
                    "one key per venue order on both residents"
                );
            }
            let claimed = |login: &Login| -> Vec<bool> {
                claims(login, &ids).into_iter().map(|(_, claimed)| claimed).collect()
            };
            assert_eq!(
                claimed(&a),
                [true, true, true, false],
                "A claims its stop children (by algoId, and by algoClOrdId alone) and \
                 its amended order, never the liquidation"
            );
            assert_eq!(
                claimed(&b),
                [false; 4],
                "B claims nothing it did not place"
            );
        })
        .await;
}
