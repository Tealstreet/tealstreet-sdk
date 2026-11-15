//! Deterministic native producer for the Ticker slice's packed-frame consumer tests.
use safe_cex_engine::{
    Command, Cx, Engine, Response, SocketId, TimerId, Venue,
    book::Book,
    model::{Limits, Market, MinMax, Precision, Ticker},
    public_feed::{PublicFeedPolicy, PublicFeedUpdate},
};
use serde_json::json;

struct FixtureVenue;
impl Venue for FixtureVenue {
    type Pending = ();
    fn start(&mut self, _: &mut Cx<'_, ()>) {}
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn main() {
    let entropy = safe_cex_engine::entropy::Entropy::new(
        Box::new(|bytes| {
            bytes.fill(1);
            Ok(())
        }),
        Box::new(|bytes| {
            bytes.fill(2);
            Ok(())
        }),
    );
    let mut engine = Engine::new(FixtureVenue, Some(entropy)).unwrap();
    engine
        .set_public_feed_policy(PublicFeedPolicy::Shared)
        .unwrap();
    engine.store_mut().set_markets(vec![Market {
        id: "BTC".into(),
        symbol: "BTC-PERP".into(),
        pretty_symbol: None,
        base: "BTC".into(),
        quote: "USD".into(),
        active: true,
        precision: Precision {
            amount: 0.001,
            price: 0.1,
        },
        limits: Limits {
            amount: MinMax {
                min: 0.001,
                max: 100.0,
            },
            leverage: MinMax {
                min: 1.0,
                max: 10.0,
            },
            price: None,
        },
        contract_size: None,
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: None,
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }]);
    engine.store_mut().set_tickers(vec![
        Ticker {
            id: "BTC".into(),
            symbol: "BTC-PERP".into(),
            bid: 0.0,
            ask: 2.0,
            last: 3.0,
            mark: 4.0,
            index: 5.0,
            ..Ticker::default()
        },
        Ticker {
            id: "native-orphan-id".into(),
            symbol: "DELISTED".into(),
            last: 6.0,
            ..Ticker::default()
        },
    ]);
    let update: PublicFeedUpdate = serde_json::from_value(json!({"tickers":[{
        "symbol":"BTC-PERP", "last":3.0, "mark":4.0, "index":5.0,
        "observedAt":[0.0,null,12.5], "midPrice":0.0
    }]}))
    .unwrap();
    engine.apply_public_feed(50_000.0, update).unwrap();
    let first = engine.snapshot_frame(60_000.0).to_vec();
    engine.take_frame(60_000.0);
    let update = serde_json::from_value(json!({"tickers":[{
        "symbol":"BTC-PERP", "midPrice":null
    }]}))
    .unwrap();
    engine.apply_public_feed(70_000.0, update).unwrap();
    let clear = engine.take_frame(70_000.0).unwrap().to_vec();
    let snapshot = engine.snapshot_frame(80_000.0).to_vec();
    println!(
        "{}",
        serde_json::to_string(&[first, clear, snapshot]).unwrap()
    );
}
