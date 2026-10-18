// Primitive transport, scheduler and Store fixtures. Every venue body is production code.
#[allow(dead_code)]
mod support {
    use crate as safe_cex_venue_asterdex;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/exchange_runtime.rs"
    ));
}
#[allow(dead_code)]
mod data {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/exchange_data.rs"
    ));
}
#[allow(unused_imports)]
use safe_cex_exchange::{
    exchange::Exchange,
    js_runtime::value::{Js, to_json},
};
use serde_json::{Value, json};
use std::{rc::Rc, sync::Arc};
pub fn fixture() -> (
    Rc<crate::native_exchange::AsterDEXExchange>,
    crate::binding::Rt,
    Rc<support::Runtime>,
    Rc<safe_cex_store::store::DefaultStore>,
    Arc<support::Wire>,
) {
    let (venue, runtime, store, wire) =
        support::fixture(json!({"key":"key","secret":"secret","publicOnly":false}));
    data::script(&wire);
    let rt = venue.__accepted_alternate_scope();
    (venue, rt, runtime, store, wire)
}
pub fn drive<F: std::future::Future>(rt: &support::Runtime, f: F) -> F::Output {
    support::drive(rt, f)
}
pub fn native(value: &Js) -> Value {
    match value {
        Js::Undefined => json!({"kind":"undefined"}),
        Js::Number(n) => json!({"bits":format!("{:016x}",n.to_bits())}),
        Js::String(s) => json!({"units":s.as_ref()}),
        _ => to_json(value).unwrap(),
    }
}
pub fn requests(wire: &support::Wire) -> Value {
    json!(
        wire.requests
            .borrow()
            .iter()
            .map(|r| json!({"method":r.method,"url":r.url,"params":r.params}))
            .collect::<Vec<_>>()
    )
}
pub fn setup_markets(
    venue: &crate::native_exchange::AsterDEXExchange,
    runtime: &support::Runtime,
    store: &safe_cex_store::store::DefaultStore,
) {
    let markets = drive(runtime, venue.fetch_markets()).unwrap();
    safe_cex_store::Store::update(
        store,
        safe_cex_store::StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .unwrap();
}
pub fn events(
    base: &safe_cex_exchange::exchange::BaseExchange,
    event: &str,
) -> Rc<std::cell::RefCell<Vec<Value>>> {
    let values = Rc::new(std::cell::RefCell::new(vec![]));
    let captured = values.clone();
    base.on_args(
        event,
        Rc::new(move |_, args| {
            captured.borrow_mut().push(json!(
                args.iter().map(|v| to_json(v).unwrap()).collect::<Vec<_>>()
            ));
            Ok(())
        }),
    );
    values
}
pub fn result<T>(value: Result<T, safe_cex_exchange::runtime::ExchangeError>) -> Value {
    match value {
        Ok(_) => json!({"ok":true}),
        Err(e) => json!({"error":e.message,"name":e.name}),
    }
}
pub fn seed_tickers(
    venue: &crate::native_exchange::AsterDEXExchange,
    runtime: &support::Runtime,
    store: &safe_cex_store::store::DefaultStore,
) {
    let tickers = drive(runtime, venue.fetch_tickers()).unwrap();
    safe_cex_store::Store::update(
        store,
        safe_cex_store::StoreUpdate {
            tickers: Some(tickers),
            ..Default::default()
        },
        false,
    )
    .unwrap();
}
pub fn balance(store: &safe_cex_store::store::DefaultStore) -> Value {
    let value = store.balance();
    json!({"total":format!("{:016x}",value.total.to_bits()),"free":format!("{:016x}",value.free.to_bits()),"used":format!("{:016x}",value.used.to_bits())})
}
pub fn parameter(wire: &support::Wire, key: &str) -> String {
    let requests = wire.requests.borrow();
    let request = requests.last().expect("real terminal HTTP request");
    let query = request.url.split_once('?').expect("signed wire query").1;
    let encoded = query
        .split('&')
        .find_map(|pair| {
            let (name, value) = pair.split_once('=')?;
            (name == key).then_some(value)
        })
        .expect("requested field in real signed query");
    let mut bytes = Vec::new();
    let mut input = encoded.as_bytes().iter().copied();
    while let Some(byte) = input.next() {
        match byte {
            b'%' => {
                let high = input.next().unwrap();
                let low = input.next().unwrap();
                bytes.push(
                    u8::from_str_radix(std::str::from_utf8(&[high, low]).unwrap(), 16).unwrap(),
                );
            }
            b'+' => bytes.push(b' '),
            value => bytes.push(value),
        }
    }
    String::from_utf8(bytes).unwrap()
}
