//! AGT-196: what the account actor records as its own orders. Two logins'
//! actors share one venue account, so each store holds the other's rows and
//! either can amend the other's orders.

use super::*;
use crate::Events;
use safe_cex_engine::book::Book;
use safe_cex_engine::error::Outcome;
use safe_cex_engine::model::{Order, OrderSide, OrderStatus, OrderType};
use safe_cex_engine::{
    Cx, HttpRequest, OrderIdentityKind, OrderIdentityReceipt, Response, SocketId, TimerId,
};

const BTC: &str = "BTCUSDT";

/// How a venue completes an amend, as its adapter does.
#[derive(Clone, Copy, Debug)]
enum AmendShape {
    /// Cancel and place: a new oid and cloid, completed with the replacement's
    /// object (`venues/hyperliquid/src/trading.rs`).
    Hyperliquid,
    /// `cancelReplace`: a fresh `clientOrderId` receipt, completed with the new
    /// order id (`venues/bingx/src/trading.rs`, `Call::Replace`).
    BingX,
    /// `amend-order`: the same ordId, completed as `[id]` (`Finish::Amend`).
    Okx,
}

/// The shared venue account as one actor's engine sees it. A placement rests
/// as `<n>` with client id `cl-<n>`, unless its client id is `rejected` or
/// `unknown`: then it reports that id and fails with that outcome.
/// `row:<symbol>:<id>:<client>` is another login's order arriving on this
/// actor's private stream.
struct SharedAccount {
    amend: AmendShape,
    prefix: &'static str,
    next: u32,
}

fn resting(id: &str, client: &str) -> Order {
    Order {
        id: id.into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: BTC.into(),
        kind: OrderType::Limit,
        side: OrderSide::Sell,
        price: 78_000.,
        amount: 0.001,
        filled: 0.,
        remaining: 0.001,
        reduce_only: false,
        close: None,
        client_order_id: Some(client.into()),
        time_in_force: None,
        last_update: 0.,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    }
}

fn receipt(symbol: &str, identity: &str) -> Vec<OrderIdentityReceipt> {
    vec![OrderIdentityReceipt {
        symbol: symbol.into(),
        kind: OrderIdentityKind::ClientOrderId,
        identity: identity.into(),
    }]
}

impl Venue for SharedAccount {
    type Pending = ();
    fn start(&mut self, _: &mut Cx<'_, ()>) {}
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, cx: &mut Cx<'_, ()>, _: SocketId, text: &str) {
        let fields: Vec<&str> = text
            .strip_prefix("row:")
            .expect("row:<symbol>:<id>:<client>")
            .split(':')
            .collect();
        let [symbol, id, client] = fields[..] else {
            panic!("row:<symbol>:<id>:<client>")
        };
        // An open-orders snapshot: venue ids can repeat across its symbols.
        let mut orders = cx.store.orders().to_vec();
        orders.push(Order {
            symbol: symbol.into(),
            ..resting(id, client)
        });
        cx.store.set_orders(orders);
    }
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, cx: &mut Cx<'_, ()>, command: Command) {
        self.next += 1;
        let fresh = format!("{}{}", self.prefix, self.next);
        match command {
            Command::PlaceOrders { command, orders } => {
                if let Some(caller) = orders[0].client_order_id.clone() {
                    // A caller-chosen id (`desk`), as Binance reports it before
                    // its venue refuses a duplicate or the response is lost.
                    cx.command_progress(command, receipt(&orders[0].symbol, &caller));
                    let outcome = if orders[0].amount > 0.005 {
                        Outcome::Unknown
                    } else {
                        Outcome::Rejected
                    };
                    cx.fail(command, VenueError::new("refused").outcome(outcome));
                    return;
                }
                let client = format!("cl-{fresh}");
                cx.command_progress(command, receipt(&orders[0].symbol, &client));
                cx.store.upsert_order(resting(&fresh, &client));
                cx.done(command, format!(r#"["{fresh}"]"#));
            }
            Command::AmendOrder { command, id, .. } => match self.amend {
                AmendShape::Hyperliquid => {
                    let cloid = format!("0x{fresh}");
                    cx.command_progress(command, receipt(BTC, &cloid));
                    cx.done(
                        command,
                        format!(r#"{{"orderIds":["{fresh}"],"clientOrderId":"{cloid}"}}"#),
                    );
                }
                AmendShape::BingX => {
                    cx.command_progress(command, receipt(BTC, &format!("bx-{fresh}")));
                    cx.done(command, format!(r#"["{fresh}"]"#));
                }
                AmendShape::Okx => cx.done(command, format!(r#"["{id}"]"#)),
            },
            other => {
                if let Some(id) = other.completion_id() {
                    cx.done(id, "null".into());
                }
            }
        }
    }
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

struct NoTransport;
impl Transport for NoTransport {
    fn connect(&self, _: SocketId, _: String, _: Events) -> mpsc::UnboundedSender<String> {
        mpsc::unbounded_channel().0
    }
    fn request(&self, _: HttpRequest, _: Events) {}
}

const CLIENT: u64 = 1;

fn login(amend: AmendShape, prefix: &'static str) -> Account<SharedAccount, NoTransport> {
    let venue = SharedAccount {
        amend,
        prefix,
        next: 0,
    };
    let mut actor = Account::new(
        Host::new(
            safe_cex_engine::test_support::seeded_engine(venue),
            NoTransport,
        ),
        ServerConfig::default(),
    );
    let (outgoing, receiver) = mpsc::channel(64);
    std::mem::forget(receiver);
    let (cancel, _) = watch::channel(false);
    actor.peers.insert(
        CLIENT,
        Peer {
            outgoing,
            cancel,
            books: HashSet::new(),
            candles: HashSet::new(),
            tickers: None,
            account_auth: false,
        },
    );
    actor
}

fn place(actor: &mut Account<SharedAccount, NoTransport>, command: u32, symbol: &str) {
    let order = serde_json::from_value(serde_json::json!({
        "symbol": symbol, "type": "limit", "side": "sell", "amount": 0.001, "price": 78_000.0
    }))
    .unwrap();
    actor.command(
        CLIENT,
        Command::PlaceOrders {
            command,
            orders: vec![order],
        },
    );
    actor.execute();
}

fn amend(actor: &mut Account<SharedAccount, NoTransport>, command: u32, id: &str) {
    actor.command(
        CLIENT,
        Command::AmendOrder {
            command,
            id: id.into(),
            price: Some(77_000.),
            amount: None,
        },
    );
    actor.execute();
}

/// The other login's row reaches this actor's store on the shared stream.
fn sees(actor: &mut Account<SharedAccount, NoTransport>, id: &str, client: &str) {
    sees_on(actor, BTC, id, client);
}

fn sees_on(actor: &mut Account<SharedAccount, NoTransport>, symbol: &str, id: &str, client: &str) {
    actor.host.engine.handle(
        1.,
        Event::WsMessage {
            socket: SocketId(0),
            text: &format!("row:{symbol}:{id}:{client}"),
        },
    );
    actor.execute();
}

fn claims(actor: &Account<SharedAccount, NoTransport>, identity: &str) -> bool {
    crate::own_placements::lock(&actor.placements).placed(BTC, identity)
}

/// Review B finding 1: login B amending login A's order must not record the
/// order, or its replacement, as B's. A's own amend still records its
/// replacement (Hyperliquid, BingX) or keeps its id (OKX).
#[test]
fn only_an_amend_of_this_logins_own_order_records_its_ids() {
    for shape in [AmendShape::Hyperliquid, AmendShape::BingX, AmendShape::Okx] {
        let mut a = login(shape, "a");
        let mut b = login(shape, "b");
        place(&mut a, 10, BTC);
        assert!(
            claims(&a, "a1") && claims(&a, "cl-a1"),
            "{shape:?}: A's placement"
        );
        sees(&mut b, "a1", "cl-a1");

        amend(&mut b, 20, "a1");
        for id in ["a1", "cl-a1", "b1", "0xb1", "bx-b1"] {
            assert!(
                !claims(&b, id),
                "{shape:?}: B's amend of A's order recorded {id}"
            );
        }

        amend(&mut a, 11, "a1");
        let replacement: &[&str] = match shape {
            AmendShape::Hyperliquid => &["a2", "0xa2"],
            AmendShape::BingX => &["a2", "bx-a2"],
            AmendShape::Okx => &["a1"],
        };
        for id in replacement {
            assert!(claims(&a, id), "{shape:?}: A's own amend records {id}");
        }
    }
}

/// An amend of an order with no row here, or of an order nobody here placed,
/// records nothing.
#[test]
fn an_amend_of_an_unknown_order_records_nothing() {
    let mut b = login(AmendShape::Hyperliquid, "b");
    amend(&mut b, 1, "missing");
    sees(&mut b, "venue-ui-1", "desk");
    amend(&mut b, 2, "venue-ui-1");
    for id in ["missing", "venue-ui-1", "desk", "b1", "0xb1", "b2", "0xb2"] {
        assert!(!claims(&b, id), "{id}");
    }
}

/// Review B finding 2: ids are recorded under the placement's symbol only.
#[test]
fn a_placement_records_its_ids_under_its_own_symbol() {
    let mut a = login(AmendShape::Okx, "a");
    place(&mut a, 1, BTC);
    let placements = crate::own_placements::lock(&a.placements);
    assert!(placements.placed(BTC, "a1") && placements.placed(BTC, "cl-a1"));
    assert!(!placements.placed("ETHUSDT", "a1"));
    assert!(!placements.placed("ETHUSDT", "cl-a1"));
}

fn place_caller(actor: &mut Account<SharedAccount, NoTransport>, command: u32, amount: f64) {
    let order = serde_json::from_value(serde_json::json!({
        "symbol": BTC, "type": "limit", "side": "sell", "amount": amount,
        "price": 78_000.0, "clientOrderId": "desk"
    }))
    .unwrap();
    actor.command(
        CLIENT,
        Command::PlaceOrders {
            command,
            orders: vec![order],
        },
    );
    actor.execute();
}

/// Review B (e49c33ac90): a placement the venue definitively refused (a
/// duplicate caller id) must not keep claiming that id for 24 h, or it claims
/// the other login's resting `desk`. An Unknown outcome may have placed it, so
/// its id stays.
#[test]
fn a_refused_placement_forgets_its_id_and_an_unknown_one_keeps_it() {
    let mut a = login(AmendShape::Okx, "a");
    place_caller(&mut a, 1, 0.001);
    assert!(!claims(&a, "desk"), "rejected: forgotten");
    place_caller(&mut a, 2, 0.01);
    assert!(claims(&a, "desk"), "unknown: kept");
}

/// A refusal forgets only what its own receipts added: an id this login
/// already held from an earlier placement stays.
#[test]
fn a_refusal_keeps_an_id_recorded_before_it() {
    let mut a = login(AmendShape::Okx, "a");
    place_caller(&mut a, 1, 0.01);
    place_caller(&mut a, 2, 0.001);
    assert!(claims(&a, "desk"));
}

/// Review B (e49c33ac90): an id-only amend whose id names rows on two symbols
/// cannot say which it edits, so it records nothing, even though the BTCUSDT
/// row is this login's.
#[test]
fn an_id_only_amend_of_an_id_on_two_symbols_records_nothing() {
    let mut a = login(AmendShape::Hyperliquid, "a");
    place(&mut a, 1, BTC);
    sees_on(&mut a, "ETHUSDT", "a1", "theirs");
    amend(&mut a, 2, "a1");
    for id in ["a2", "0xa2"] {
        assert!(!claims(&a, id), "{id}");
    }
}
