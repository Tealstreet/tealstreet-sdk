//! Native Order output fixtures consumed by orderFrames.test.ts.
#[path = "../src/order_output.rs"]
mod order_output;
use safe_cex_engine::model::{
    Limits, Market, MinMax, Order, OrderExtra, OrderSide, OrderStatus, OrderType, Precision,
    RejectReason, StopTriggerType,
};
use safe_cex_engine::{QuantityUnits, store::Store, wire::FrameWriter};
use safe_cex_engine::{model, num, wire};
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

fn main() {
    let mut frames = Vec::new();
    for (profile, (units, inverse)) in [
        (QuantityUnits::Base, false),
        (QuantityUnits::NativePlacement, false),
        (QuantityUnits::NativePlacement, true),
        (QuantityUnits::InverseNative, true),
        (QuantityUnits::ContractCount, false),
        (QuantityUnits::ContractCount, true),
        (QuantityUnits::InverseContractSized, true),
        (QuantityUnits::InverseNative, false),
    ]
    .into_iter()
    .enumerate()
    {
        let mut store = Store::default();
        store.set_quantity_units(units);
        let mut m = market("BTC", None);
        m.contract_size = Some(0.1);
        m.is_inverse = Some(inverse);
        if profile < 7 {
            store.set_markets(vec![m]);
        }
        let mut base = order("BTC", "ordinary", None);
        base.amount = 10.;
        base.filled = 0.3;
        base.remaining = 0.7;
        base.take_profit = None;
        base.stop_loss = None;
        let mut exact = base.clone();
        exact.id = "exact".into();
        exact.parent_id = Some("parent".into());
        exact.client_order_id = Some("client".into());
        exact.close = Some(false);
        exact.kind = OrderType::StopLoss;
        exact.trigger_type = Some(StopTriggerType::MarkPrice);
        exact.take_profit = Some(0.);
        exact.stop_loss = Some(0.);
        exact.extra = Some(OrderExtra {
            remaining_native: Some(7.),
            filled_in_placement_units: Some(3.),
            remaining_notional: Some(700.),
            conditional_order_price: Some(0.),
            reject_reason: Some(RejectReason::PostOnly),
            ..Default::default()
        });
        let mut terminal = exact.clone();
        terminal.id = "terminal".into();
        terminal.status = OrderStatus::Canceled;
        let mut catalogue = Store::default();
        catalogue.set_quantity_units(units);
        catalogue.set_markets(store.markets().to_vec());
        store.set_orders(vec![base, exact, terminal]);
        let pending = catalogue.take_pending();
        let mut writer = FrameWriter::new(Vec::new(), 1000.);
        catalogue.write(&pending, &mut writer);
        order_output::write(
            store.orders(),
            |symbol| store.market(symbol),
            |symbol| store.market_index(symbol).unwrap_or(wire::NONE),
            units as u32,
            &mut writer,
        );
        frames.push(writer.finish());
    }
    println!("{}", serde_json::to_string(&frames).unwrap());
}
