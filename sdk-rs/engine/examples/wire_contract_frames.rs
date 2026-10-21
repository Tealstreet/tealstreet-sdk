//! Actual writer frames for the JS/TypeScript wire contract regressions.
//! Run with `cargo run -q -p safe-cex-engine --example wire_contract_frames`.
#[path = "../tests/support/mod.rs"]
mod support;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::model::{
    Candle, Limits, MarginType, Market, MinMax, Order, OrderExtra, OrderSide, OrderStatus,
    OrderType, Position, PositionMode, PositionSide, Precision, StopTriggerType, Ticker,
};
use safe_cex_engine::store::MarketSetting;
use safe_cex_engine::{
    Command, Cx, Event, Fill, FillFinancials, Response, SocketId, TimerId, Venue,
};

fn market(symbol: &str, pretty: Option<&str>) -> Market {
    Market {
        id: format!("native-{symbol}"),
        symbol: symbol.into(),
        pretty_symbol: pretty.map(str::to_owned),
        base: symbol.into(),
        quote: "USD".into(),
        active: true,
        precision: Precision {
            amount: 0.01,
            price: 0.1,
        },
        limits: Limits {
            amount: MinMax {
                min: 0.01,
                max: 100.,
            },
            leverage: MinMax { min: 1., max: 50. },
            price: None,
        },
        contract_size: Some(2.),
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: Some(true),
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }
}

fn order(symbol: &str, id: &str, extra: Option<OrderExtra>) -> Order {
    Order {
        id: id.into(),
        symbol: symbol.into(),
        parent_id: None,
        status: OrderStatus::Open,
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        price: 101.,
        amount: 10.,
        filled: 3.,
        remaining: 7.,
        reduce_only: false,
        close: None,
        client_order_id: None,
        time_in_force: None,
        last_update: 1000.,
        trigger_type: None,
        take_profit: Some(120.),
        stop_loss: Some(90.),
        extra,
    }
}

fn position(symbol: &str) -> Position {
    Position {
        id: format!("position-{symbol}"),
        symbol: symbol.into(),
        side: PositionSide::Long,
        entry_price: 100.,
        notional: 1000.,
        leverage: 5.,
        margin_type: Some(MarginType::Cross),
        position_mode: Some(PositionMode::Oneway),
        unrealized_pnl: 10.,
        realized_pnl: Some(2.),
        contracts: 10.,
        liquidation_price: 80.,
        margin_size: Some(200.),
        last_update: Some(1000.),
    }
}

fn executions(cx: &mut Cx<'_, ()>) {
    for (symbol, order_id) in [("A", "tracked-order"), ("ORPHAN", "orphan-order")] {
        cx.fill_with_financials(
            Fill {
                id: (symbol == "A").then(|| "9007199254740997".into()),
                order_id: order_id.into(),
                symbol: symbol.into(),
                side: OrderSide::Buy,
                price: 101.,
                amount: 0.25,
            },
            if symbol == "A" {
                FillFinancials {
                    realized_pnl: Some(-3.5),
                    fee: Some(-0.25),
                }
            } else {
                FillFinancials::default()
            },
        );
    }
}

struct FixtureVenue {
    book: Book,
}

impl Venue for FixtureVenue {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        cx.store.set_markets(vec![
            market("A", Some("Native A / USD")),
            market("B", Some("")),
            market("C", None),
        ]);
        cx.store.set_tickers(vec![
            Ticker {
                id: "native-A".into(),
                symbol: "A".into(),
                last: 101.,
                mid_price: Some(101.25),
                ..Ticker::default()
            },
            Ticker {
                id: "native-ORPHAN".into(),
                symbol: "ORPHAN".into(),
                last: 7.,
                ..Ticker::default()
            },
        ]);
        cx.store.set_orders(vec![
            order(
                "A",
                "tracked-order",
                Some(OrderExtra {
                    remaining_native: Some(7.),
                    filled_in_placement_units: Some(3.),
                    remaining_notional: Some(42.),
                    conditional_order_price: Some(125.),
                    tp_trigger_type: Some(StopTriggerType::IndexPrice),
                    sl_trigger_type: Some(StopTriggerType::LastPrice),
                    ..OrderExtra::default()
                }),
            ),
            order("ORPHAN", "orphan-order", None),
        ]);
        cx.store
            .set_positions(vec![position("A"), position("ORPHAN")]);
        cx.store.set_settings([
            (
                0,
                MarketSetting {
                    leverage: 5.,
                    buy_leverage: Some(6.),
                    sell_leverage: Some(7.),
                    margin_type: Some(MarginType::Cross),
                },
            ),
            (
                1,
                MarketSetting {
                    leverage: 9.,
                    ..MarketSetting::default()
                },
            ),
        ]);
        cx.store.update_setting("ORPHAN", |setting| {
            setting.leverage = 11.;
            setting.margin_type = Some(MarginType::Isolated);
        });
        executions(cx);
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, cx: &mut Cx<'_, ()>, _: SocketId, text: &str) {
        match text {
            "mid" => cx
                .store
                .update_ticker("A", |ticker| ticker.mid_price = Some(102.5)),
            "clear" => cx
                .store
                .update_ticker("A", |ticker| ticker.mid_price = None),
            "reorder" => {
                cx.store.set_markets(vec![
                    market("B", Some("")),
                    market("A", Some("Native A / USD")),
                ]);
                self.book.apply(BookSide::Bids, 100., 1.);
                self.book.apply(BookSide::Asks, 102., 2.);
                cx.book_changed("A");
                cx.candle(
                    "A",
                    safe_cex_engine::model::Interval::M1,
                    Candle {
                        timestamp: 1000.,
                        open: 100.,
                        high: 103.,
                        low: 99.,
                        close: 101.,
                        volume: 4.,
                    },
                );
                executions(cx);
            }
            "emit" => executions(cx),
            _ => unreachable!(),
        }
    }
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn book(&self, symbol: &str) -> Option<&Book> {
        (symbol == "A").then_some(&self.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::once(("A", &self.book)))
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut out, "{byte:02x}").unwrap();
    }
    out
}

fn main() {
    let mut engine = support::seeded_engine(FixtureVenue {
        book: Book::default(),
    });
    engine.start(1000.);
    let initial = hex(engine.take_frame(1000.).unwrap());
    engine.handle(
        1001.,
        Event::WsMessage {
            socket: SocketId(1),
            text: "mid",
        },
    );
    let delta_mid = hex(engine.take_frame(1001.).unwrap());
    engine.handle(
        1002.,
        Event::WsMessage {
            socket: SocketId(1),
            text: "clear",
        },
    );
    let clear_mid = hex(engine.take_frame(1002.).unwrap());
    engine.handle(
        1003.,
        Event::WsMessage {
            socket: SocketId(1),
            text: "reorder",
        },
    );
    // Pending fills promote the market flush to a full frame: MARKETS first,
    // then every queued market-flush row, so none is published ahead of it.
    let reordered = hex(engine.take_market_frame(1003.).unwrap());
    assert!(engine.take_frame(1004.).is_none(), "nothing is sent twice");
    engine.handle(
        1005.,
        Event::WsMessage {
            socket: SocketId(1),
            text: "emit",
        },
    );
    let resumed = hex(engine.take_market_frame(1005.).unwrap());
    println!(
        "{}",
        serde_json::json!({"initial": initial, "deltaMid": delta_mid, "clearMid": clear_mid, "reordered": reordered, "resumed": resumed})
    );
}
