// Native proof for the two source-inherited constant refusals only.
// Caller supplies the existing scripted I/O module and actual venue crate alias.
use safe_cex_exchange::{exchange::Exchange, runtime::ExchangeFuture};
use safe_cex_store::{Store, StoreUpdate};
use serde_json::{Value, json};
use std::{cell::Cell, rc::Rc};

pub fn observe<'a>(
    h: &'a io::Harness,
    invoke: impl FnOnce(&'a venue_crate::BitgetExchange) -> ExchangeFuture<'a, ()>,
) -> Value {
    // Synthetic finite seeds are independent of the venue being observed.
    let orders = serde_json::from_value(json!([{"id":"seed-order","symbol":"SEED",
        "status":"open","type":"limit","side":"buy","price":17.5,"amount":2,
        "filled":0.5,"remaining":1.5,"reduceOnly":false,"lastUpdate":33}]))
    .unwrap();
    let positions = serde_json::from_value(json!([{"id":"seed-position","symbol":"SEED",
        "side":"long","entryPrice":19.5,"notional":39,"leverage":2,
        "unrealizedPnl":3.25,"contracts":2,"liquidationPrice":7.5}]))
    .unwrap();
    Store::update(
        &*h.store,
        StoreUpdate {
            orders: Some(orders),
            positions: Some(positions),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let notifications = Rc::new(Cell::new(0));
    let watched = notifications.clone();
    let _unsubscribe = Store::subscribe(
        &*h.store,
        Box::new(move |_, _| {
            watched.set(watched.get() + 1);
            Ok(())
        }),
    );
    let events = Rc::new(Cell::new(0));
    for name in ["update", "error", "log"] {
        let watched = events.clone();
        h.exchange.base().on(
            name,
            Rc::new(move |_, _| {
                watched.set(watched.get() + 1);
                Ok(())
            }),
        );
    }
    let snapshot = || {
        let balance = h.store.balance().unwrap();
        (
            balance.clone(),
            [
                balance.used.to_bits(),
                balance.free.to_bits(),
                balance.total.to_bits(),
                balance.upnl.to_bits(),
            ],
            h.store.orders().unwrap().to_vec(),
            h.store.positions().unwrap().to_vec(),
            h.store.markets().unwrap().to_vec(),
            h.store.tickers().unwrap().to_vec(),
        )
    };
    let before = snapshot();
    let options = h.exchange.base().options();
    let reports = h.runtime.reports.borrow().len();
    let now = h.runtime.now.get().to_bits();
    let sockets = h.runtime.sockets.borrow().len();
    let error = h.drive(invoke(h.exchange.as_ref())).unwrap_err();
    json!({"error":error.message,"error_name":error.name,"http":h.http.requests.borrow().len(),
        "state_unchanged":before==snapshot(),"options_unchanged":options==h.exchange.base().options(),
        "notifications":notifications.get(),"events":events.get(),
        "reports_unchanged":reports==h.runtime.reports.borrow().len(),
        "sockets_unchanged":sockets==h.runtime.sockets.borrow().len(),"now_unchanged":now==h.runtime.now.get().to_bits()})
}
