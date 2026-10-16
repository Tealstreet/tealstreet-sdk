// Accepted-set instrumentation only: use the existing concrete E2E I/O hosts.
// Adapters retain production methods and inspect their native values/effects.
#[allow(dead_code)]
pub mod io {
    use crate as safe_cex_venue_bitget;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/binding_harness.rs"
    ));
}
use safe_cex_exchange::js_runtime::{
    scope::{Scope, ScopeShared},
    value::Js,
};
use std::rc::Rc;

pub fn host() -> crate::binding::rt::Rt {
    let fixture = io::harness(safe_cex_types::ExchangeName::Bitget, io::options(), |_| {});
    let shared = Rc::new_cyclic(|weak| {
        let globals = crate::binding::globals::build(weak.clone());
        crate::binding::hosts::register_all(&globals, weak.clone());
        ScopeShared::new(
            fixture.runtime.clone(),
            fixture.http.clone(),
            fixture.store.clone(),
            globals,
            crate::binding::rt::BitgetExtra {
                variant: crate::binding::rt::Variant::Classic,
                name: safe_cex_types::ExchangeName::Bitget,
                options: io::options(),
                external_public_data: None,
            },
        )
    });
    let this = Js::object(vec![]);
    *shared.exchange.borrow_mut() = this.clone();
    Scope { shared, this }
}

pub fn ready<T>(mut future: std::pin::Pin<Box<dyn std::future::Future<Output = T> + '_>>) -> T {
    use std::task::{Context, Poll, Waker};
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("expected an immediately ready native fixture"),
    }
}

pub fn position(id: &str, marker: f64) -> safe_cex_types::Position {
    safe_cex_types::Position {
        id: id.into(),
        symbol: "NATIVE".into(),
        side: safe_cex_types::PositionSide::Long,
        entry_price: marker,
        notional: 0.0,
        leverage: 1.0,
        margin_type: None,
        position_mode: None,
        unrealized_pnl: 0.0,
        realized_pnl: None,
        contracts: 1.0,
        liquidation_price: 0.0,
        margin_size: None,
        info: None,
        extra: None,
        last_update: None,
        option: None,
    }
}

pub fn market(base: &str) -> safe_cex_types::Market {
    use safe_cex_types::*;
    Market {
        id: format!("{base}USDT"),
        symbol: format!("{base}USDT"),
        pretty_symbol: None,
        base: base.into(),
        quote: "USDT".into(),
        active: true,
        precision: MarketPrecision {
            amount: 0.001,
            price: 0.1,
        },
        limits: MarketLimits {
            amount: MarketLimitsAmount {
                min: 0.001,
                max: 50.0,
            },
            leverage: MarketLimitsLeverage {
                min: 1.0,
                max: 20.0,
            },
            price: None,
        },
        option: None,
        contract_size: None,
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: None,
        is_read_only: None,
        info: None,
        taker_fee: None,
        maker_fee: None,
    }
}

pub fn order() -> safe_cex_types::Order {
    use safe_cex_types::*;
    Order {
        id: "synthetic-order".into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "NATIVE".into(),
        r#type: OrderType::StopLoss,
        side: OrderSide::Buy,
        price: 17.5,
        amount: 2.0,
        filled: 0.0,
        remaining: 2.0,
        reduce_only: true,
        close: None,
        info: None,
        client_order_id: None,
        time_in_force: None,
        last_update: 33.0,
        trigger_type: None,
        tp_trigger_type: None,
        sl_trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    }
}

pub fn ticker() -> safe_cex_types::Ticker {
    safe_cex_types::Ticker {
        id: "native-ticker".into(),
        symbol: "NATIVE".into(),
        bid: 1.0,
        ask: 2.0,
        last: 1.5,
        mark: 0.0,
        index: 0.0,
        mid_price: None,
        percentage: 0.0,
        open_interest: 0.0,
        funding_rate: 0.0,
        funding_time: 0.0,
        volume: 0.0,
        quote_volume: 0.0,
        timestamp: None,
        info: None,
    }
}
