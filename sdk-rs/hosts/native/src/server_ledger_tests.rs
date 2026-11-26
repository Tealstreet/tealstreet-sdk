//! AGT-196: what the account actor records as its own orders. Two logins'
//! actors share one venue account, so each store holds the other's rows and
//! either can amend the other's orders.

use super::*;
use crate::Events;
use safe_cex_engine::book::Book;
use safe_cex_engine::error::Outcome;
use safe_cex_engine::model::{Order, OrderSide, OrderStatus, OrderType};
use safe_cex_engine::write_plan::{Element, ElementOutcome, ElementOutcomes, ElementResult};
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

/// SYNTHETIC: a stub venue, not a recorded frame shape; the Binance portfolio
/// margin test in `tests/own_placements_binance.rs` uses real frame shapes.
///
/// The shared venue account as one actor's engine sees it. A placement rests
/// as `<n>` with a generated client id `cl-<n>`. Its amount picks an outcome:
/// - with a caller-chosen client id: 0.001 rejected; 0.01 Unknown (as Bybit,
///   BingX and BloFin classify a duplicate id); 0.02 placed under that id, as
///   Binance portfolio margin keys orders by client id; 0.025 acked the same
///   way but with no row yet (on Binance only the stream creates it);
/// - without one: 0.03 rejected and 0.04 Unknown, after its receipt.
///
/// A two-order batch reports both legs' generated client ids, then fails
/// Unknown with its first leg proven placed under a native row index that is
/// not its batch position. `row:<symbol>:<id>:<client>` is another login's
/// order arriving on this actor's private stream; `close:<id>` closes a row;
/// `open:<id>` is a live row under that id (its `NEW` frame);
/// `drop:<id>` is an open-orders snapshot without it (after a stream gap).
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
        if let Some(id) = text.strip_prefix("close:") {
            cx.store.set_order_status(id, OrderStatus::Closed);
            return;
        }
        if let Some(id) = text.strip_prefix("open:") {
            cx.store.upsert_order(resting(id, id));
            return;
        }
        if let Some(id) = text.strip_prefix("drop:") {
            let open = cx
                .store
                .orders()
                .iter()
                .filter(|order| order.id != id)
                .cloned()
                .collect();
            cx.store.set_orders(open);
            return;
        }
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
                if orders.len() == 2 {
                    let clients = [format!("cl-{fresh}-0"), format!("cl-{fresh}-1")];
                    for client in &clients {
                        cx.command_progress(command, receipt(BTC, client));
                    }
                    let leg = |index: usize, client: &str, result: ElementResult| ElementOutcome {
                        element: Element {
                            index,
                            client_order_id: Some(client.into()),
                            role: None,
                        },
                        result,
                    };
                    let mut error = VenueError::new("second leg lost").outcome(Outcome::Unknown);
                    error.elements = Some(Box::new(ElementOutcomes(vec![
                        leg(
                            5,
                            &clients[0],
                            ElementResult::Placed {
                                order_ids: vec![format!("{fresh}-0")],
                            },
                        ),
                        leg(
                            0,
                            &clients[1],
                            ElementResult::Unknown {
                                error: Box::new(VenueError::new("lost")),
                            },
                        ),
                    ])));
                    cx.fail(command, error);
                    return;
                }
                let amount = orders[0].amount;
                let symbol = orders[0].symbol.clone();
                if let Some(caller) = orders[0].client_order_id.clone() {
                    cx.command_progress(command, receipt(&symbol, &caller));
                    if amount > 0.015 {
                        if amount < 0.022 {
                            cx.store.upsert_order(resting(&caller, &caller));
                        }
                        cx.done(command, format!(r#"["{caller}"]"#));
                    } else {
                        let outcome = if amount > 0.005 {
                            Outcome::Unknown
                        } else {
                            Outcome::Rejected
                        };
                        cx.fail(command, VenueError::new("refused").outcome(outcome));
                    }
                    return;
                }
                let client = format!("cl-{fresh}");
                cx.command_progress(command, receipt(&symbol, &client));
                if amount > 0.025 {
                    let outcome = if amount > 0.035 {
                        Outcome::Unknown
                    } else {
                        Outcome::Rejected
                    };
                    cx.fail(command, VenueError::new("refused").outcome(outcome));
                    return;
                }
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

fn place_with(actor: &mut Account<SharedAccount, NoTransport>, command: u32, amount: f64) {
    let order = serde_json::from_value(serde_json::json!({
        "symbol": BTC, "type": "limit", "side": "sell", "amount": amount, "price": 78_000.0
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

/// A generated client id cannot name another login's order: it is claimed from
/// its receipt, kept on Unknown, and forgotten only on a definitive refusal.
#[test]
fn a_generated_id_is_forgotten_on_refusal_and_kept_on_unknown() {
    let mut a = login(AmendShape::Okx, "a");
    place_with(&mut a, 1, 0.03);
    assert!(!claims(&a, "cl-a1"), "rejected: forgotten");
    place_with(&mut a, 2, 0.04);
    assert!(claims(&a, "cl-a2"), "unknown: kept");
}

/// Review B (942d22190f), synthetic venue: a caller-chosen id is claimed only
/// once a placement under it is proven. Bybit reports a duplicate `orderLinkId` as Unknown, and
/// the duplicate is login B's resting `desk`: A must not claim it.
#[test]
fn a_caller_id_is_claimed_only_once_proven() {
    let mut a = login(AmendShape::Okx, "a");
    place_caller(&mut a, 1, 0.01);
    assert!(!claims(&a, "desk"), "Unknown (a Bybit duplicate id)");
    place_caller(&mut a, 2, 0.001);
    assert!(!claims(&a, "desk"), "rejected");
    place_caller(&mut a, 3, 0.02);
    assert!(
        claims(&a, "desk"),
        "placed: the venue acked it under that id"
    );
}

/// Review B (942d22190f, e6e681d490), synthetic venue: a batch claims only the
/// legs the venue proved accepted, matched by client id rather than by row
/// index; its Unknown leg's native id claims nothing.
#[test]
fn a_batch_claims_only_its_proven_legs() {
    let mut a = login(AmendShape::Okx, "a");
    let leg = |price: f64| {
        serde_json::from_value(serde_json::json!({
            "symbol": BTC, "type": "limit", "side": "sell", "amount": 0.001, "price": price
        }))
        .unwrap()
    };
    a.command(
        CLIENT,
        Command::PlaceOrders {
            command: 1,
            orders: vec![leg(78_000.), leg(79_000.)],
        },
    );
    a.execute();
    assert!(claims(&a, "a1-0"), "the proven leg");
    assert!(!claims(&a, "a1-1"), "the Unknown leg");
}

/// Review B (942d22190f), synthetic venue: once a caller-chosen id's order closes, the id stops
/// claiming after `TERMINAL_GRACE`, so B reusing `desk` (Binance allows it
/// after close) is not claimed by A. A's own late fills inside the grace are.
#[test]
fn a_reused_caller_id_is_not_claimed_after_its_order_closed() {
    use crate::own_placements::TERMINAL_GRACE;
    let mut a = login(AmendShape::Okx, "a");
    place_caller(&mut a, 1, 0.02);
    let later = std::time::Instant::now() + TERMINAL_GRACE + Duration::from_secs(1);
    assert!(crate::own_placements::lock(&a.placements).placed_at(BTC, "desk", later));
    a.host.engine.handle(
        1.,
        Event::WsMessage {
            socket: SocketId(0),
            text: "close:desk",
        },
    );
    a.execute();
    let placements = crate::own_placements::lock(&a.placements);
    assert!(placements.placed(BTC, "desk"), "its own last fills");
    assert!(!placements.placed_at(BTC, "desk", later), "a later reuse");
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

/// Review B (e6e681d490), synthetic venue: an order that closed during a
/// stream gap is simply absent from the reconnect's open-orders snapshot. Its
/// caller id retires then too, not after 24 h.
#[test]
fn a_caller_id_retires_when_a_snapshot_drops_its_order() {
    use crate::own_placements::TERMINAL_GRACE;
    let mut a = login(AmendShape::Okx, "a");
    place_caller(&mut a, 1, 0.02);
    sees(&mut a, "unrelated", "x");
    let later = std::time::Instant::now() + TERMINAL_GRACE + Duration::from_secs(1);
    assert!(crate::own_placements::lock(&a.placements).placed_at(BTC, "desk", later));
    a.host.engine.handle(
        1.,
        Event::WsMessage {
            socket: SocketId(0),
            text: "drop:desk",
        },
    );
    a.execute();
    let placements = crate::own_placements::lock(&a.placements);
    assert!(placements.placed(BTC, "desk"), "its own last fills");
    assert!(!placements.placed_at(BTC, "desk", later), "a later reuse");
}

/// Review A (5f75821a55), synthetic venue: the stream drops between the ack
/// and the order's first frame, so no row is ever stored; the order fills and
/// closes in the gap and the reconnect snapshot omits it. The caller id still
/// retires on that first read after it was recorded, not after 24 h.
#[test]
fn a_caller_id_with_no_row_retires_when_the_next_snapshot_omits_it() {
    use crate::own_placements::TERMINAL_GRACE;
    let mut a = login(AmendShape::Okx, "a");
    place_caller(&mut a, 1, 0.025);
    let later = std::time::Instant::now() + TERMINAL_GRACE + Duration::from_secs(1);
    assert!(
        crate::own_placements::lock(&a.placements).placed_at(BTC, "desk", later),
        "no read since the ack: still claimed"
    );
    a.host.engine.handle(
        1.,
        Event::WsMessage {
            socket: SocketId(0),
            text: "drop:desk",
        },
    );
    a.execute();
    let placements = crate::own_placements::lock(&a.placements);
    assert!(placements.placed(BTC, "desk"), "its own last fills");
    assert!(!placements.placed_at(BTC, "desk", later), "a later reuse");
}

fn btc_market() -> safe_cex_engine::model::Market {
    use safe_cex_engine::model::{Limits, Market, MinMax, Precision};
    Market {
        id: BTC.into(),
        symbol: BTC.into(),
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
    }
}

/// An empty account-wide open-orders read released at `seq`, committed now.
fn commit_empty_read(actor: &mut Account<SharedAccount, NoTransport>, seq: u64) {
    use safe_cex_engine::order_read_cover::{OrderReadMark, OrderReadScope};
    let read = OrderReadMark {
        seq,
        sent: 0.,
        epoch: 0,
    };
    let scope = OrderReadScope::Account {
        key: "orders",
        account: &["orders"],
    };
    actor
        .host
        .engine
        .store_mut()
        .commit_orders_read(read, scope, |_| true, Vec::new(), &[]);
    actor.execute();
}

/// Review B (1acdf351b7), synthetic venue: an open-orders read released BEFORE
/// a caller-id placement, committed after its REST ack (which writes no row),
/// must not retire the id: the ack fences such reads for its symbol. A read
/// released after the ack that omits the order still retires it.
#[test]
fn a_read_released_before_the_ack_does_not_retire_the_caller_id() {
    use crate::own_placements::TERMINAL_GRACE;
    let mut a = login(AmendShape::Okx, "a");
    a.host.engine.store_mut().set_markets(vec![btc_market()]);
    let stale = a.host.engine.store().order_read_sequence();
    place_caller(&mut a, 1, 0.025);
    let later = std::time::Instant::now() + TERMINAL_GRACE + Duration::from_secs(1);
    commit_empty_read(&mut a, stale);
    assert!(
        crate::own_placements::lock(&a.placements).placed_at(BTC, "desk", later),
        "a stale read speaks for no order the ack placed"
    );
    let fresh = a.host.engine.store().order_read_sequence();
    commit_empty_read(&mut a, fresh);
    assert!(
        !crate::own_placements::lock(&a.placements).placed_at(BTC, "desk", later),
        "a read released after the ack omits it: retired"
    );
}

fn stream(actor: &mut Account<SharedAccount, NoTransport>, text: &str) {
    actor.host.engine.handle(
        1.,
        Event::WsMessage {
            socket: SocketId(0),
            text,
        },
    );
    actor.execute();
}

/// Review B (10e8ae20c6), synthetic venue: `desk` #1 closed and its row stays
/// until the next snapshot (as Binance keeps it). `desk` #2's REST ack lands
/// before its `NEW` frame, so the only `desk` row is #1's closed one: it must
/// not retire #2. #2's own row, then its close, does.
#[test]
fn an_earlier_orders_closed_row_does_not_retire_a_reused_caller_id() {
    use crate::own_placements::TERMINAL_GRACE;
    let mut a = login(AmendShape::Okx, "a");
    place_caller(&mut a, 1, 0.02);
    stream(&mut a, "close:desk");
    place_caller(&mut a, 2, 0.025);
    let later = std::time::Instant::now() + TERMINAL_GRACE + Duration::from_secs(1);
    let claimed = |a: &Account<SharedAccount, NoTransport>| {
        crate::own_placements::lock(&a.placements).placed_at(BTC, "desk", later)
    };
    assert!(claimed(&a), "#1's leftover closed row");
    stream(&mut a, "open:desk");
    assert!(claimed(&a), "#2 rests");
    stream(&mut a, "close:desk");
    assert!(!claimed(&a), "#2 closed");
}
