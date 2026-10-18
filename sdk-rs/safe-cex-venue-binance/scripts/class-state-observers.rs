// Execute the actual assembled class. Only terminal HTTP/WS/runtime ports are supplied.
#[path = "exchange-native-observers.rs"]
pub mod native;
use crate::exchange_classes::ExchangeOwner;
use native::{Parts, raw, wire};
use safe_cex_exchange::{exchange::Exchange, js_runtime::value::Js, runtime::ExchangeError};
use safe_cex_store::Store;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};
pub fn parts(input: &Value) -> Parts {
    let p = native::parts(input);
    let mut markets = p.store.markets().unwrap().to_vec();
    let mut eth = markets[0].clone();
    eth.id = "ETHUSDT".into();
    eth.symbol = "ETHUSDT".into();
    eth.base = "ETH".into();
    markets.push(eth);
    let positions=["BTCUSDT","ETHUSDT"].map(|symbol| serde_json::from_value::<safe_cex_types::Position>(json!({"id":symbol,"symbol":symbol,"side":"long","contracts":0,"entryPrice":0,"notional":0,"leverage":1,"unrealizedPnl":0,"liquidationPrice":0,"marginSize":0,"marginType":"cross","positionMode":"oneway"})).unwrap()).to_vec();
    p.store
        .update(
            safe_cex_store::StoreUpdate {
                markets: Some(markets),
                positions: Some(positions),
                ..Default::default()
            },
            false,
        )
        .unwrap();
    for path in [
        "/fapi/v3/positionRisk",
        "/fapi/v1/symbolConfig",
        "/papi/v1/um/positionRisk",
        "/fapi/v1/openOrders",
        "/fapi/v1/openAlgoOrders",
        "/papi/v1/um/openOrders",
        "/papi/v1/um/conditional/openOrders",
        "/api/v3/openOrders",
        "/fapi/v1/income",
        "/fapi/v1/trades",
    ] {
        p.http.ok("GET", path, json!([]));
    }
    p.http.ok("GET", "/api/v3/account", json!({"balances":[]}));
    for path in [
        "/fapi/v1/leverage",
        "/papi/v1/um/leverage",
        "/fapi/v1/marginType",
        "/fapi/v1/positionSide/dual",
        "/papi/v1/um/positionSide/dual",
    ] {
        p.http.ok("POST", path, json!({}));
    }
    for path in [
        "/fapi/v1/allOpenOrders",
        "/fapi/v1/algoOpenOrders",
        "/papi/v1/um/allOpenOrders",
        "/papi/v1/um/conditional/allOpenOrders",
        "/api/v3/openOrders",
    ] {
        p.http.ok("DELETE", path, json!({}));
    }
    p
}
pub fn symbol(input: &Value) -> String {
    input["symbol"].as_str().unwrap().into()
}
pub fn flag(input: &Value) -> bool {
    input["flag"].as_bool().unwrap()
}
pub fn events(owner: &ExchangeOwner) -> Rc<RefCell<Vec<Value>>> {
    let rows = Rc::new(RefCell::new(vec![]));
    for event in ["log", "error", "audit"] {
        let seen = rows.clone();
        owner.base.on_args(
            event,
            Rc::new(move |_, args| {
                seen.borrow_mut()
                    .push(json!({"event":event,"args":args.iter().map(raw).collect::<Vec<_>>() }));
                Ok(())
            }),
        );
    }
    rows
}
pub fn result<T, E: std::fmt::Display>(
    value: Result<T, E>,
    project: impl FnOnce(T) -> Value,
) -> Value {
    match value {
        Ok(v) => json!({"ok":project(v)}),
        Err(e) => json!({"error":e.to_string()}),
    }
}
pub fn unit<E: std::fmt::Display>(value: Result<(), E>) -> Value {
    result(value, |()| json!("unit"))
}
pub fn finish(
    p: &Parts,
    owner: &ExchangeOwner,
    events: &RefCell<Vec<Value>>,
    value: Value,
) -> Value {
    let out = json!({"value":value,"wire":wire(&p.http),"hedged":p.store.options().unwrap().is_hedged,"events":*events.borrow()});
    owner.base.dispose().unwrap();
    out
}
pub fn constructor<E: Exchange>(subject: &E, owner: &Rc<ExchangeOwner>, p: &Parts) -> Value {
    assert!(std::ptr::eq(subject.base(), owner.base.as_ref()));
    assert!(Rc::ptr_eq(&owner.public, &owner.state.public_socket()));
    assert!(Rc::ptr_eq(&owner.private, &owner.state.private_socket()));
    assert!(p.http.sent.lock().unwrap().is_empty());
    json!({"family":format!("{:?}",owner.venue),"name":subject.base().name(),"testnet":subject.base().options().testnet,"sameBase":true,"sameChildren":true,"http":0})
}
pub fn order_rows(rows: Vec<safe_cex_types::Order>) -> Value {
    json!(rows.iter().map(|r| json!({"id":r.id,"symbol":r.symbol,"amountBits":format!("{:016x}",r.amount.to_bits())})).collect::<Vec<_>>())
}
pub fn position_rows(rows: Vec<safe_cex_types::Position>) -> Value {
    json!(rows.iter().map(|r| json!({"id":r.id,"symbol":r.symbol,"contractsBits":format!("{:016x}",r.contracts.to_bits())})).collect::<Vec<_>>())
}
pub fn inherited<T>(
    input: &Value,
    value: Result<T, ExchangeError>,
    p: &Parts,
    events: &RefCell<Vec<Value>>,
) -> Value {
    let error = value.err().expect("actual inherited refusal");
    assert!(p.http.sent.lock().unwrap().is_empty());
    assert!(events.borrow().is_empty());
    assert!(p.runtime.reports.borrow().is_empty());
    let _ = input;
    json!({"error":error.message,"error_name":error.name,"http":0,"events":0,"reports":0})
}
pub fn listener_setup(owner: &ExchangeOwner, p: &Parts) {
    owner.public.connect_and_subscribe().unwrap();
    p.runtime.pump();
}
pub fn listener_wire(p: &Parts) -> Value {
    json!(
        p.runtime
            .sockets
            .borrow()
            .iter()
            .flat_map(|(_, s)| s
                .sent
                .borrow()
                .iter()
                .map(|s| serde_json::from_str::<Value>(s).unwrap())
                .collect::<Vec<_>>())
            .filter(|v| v["params"].as_array().is_some_and(|a| a
                .iter()
                .any(|p| p.as_str().is_some_and(|s| s.ends_with("@depth")))))
            .collect::<Vec<_>>()
    )
}
pub fn listen_key(value: crate::listen_key::ListenKeyResponse<Js>) -> Value {
    match value {
        crate::listen_key::ListenKeyResponse::Data(crate::listen_key::ListenKeyData::Present(
            key,
        )) => key.borrow().as_ref().map(raw).unwrap_or(json!("undefined")),
        _ => panic!("actual listen-key wrapper"),
    }
}
pub fn state_mode(owner: &ExchangeOwner) -> Value {
    let lev = owner.base.store.fetched_leverage().unwrap();
    json!({"hedged":owner.base.store.options().unwrap().is_hedged,"leverage":lev.iter().map(|(s,l)|json!([s,format!("{:?}",l.position_mode),format!("{:016x}",l.buy_leverage.to_bits()),format!("{:016x}",l.sell_leverage.to_bits())])).collect::<Vec<_>>()})
}
