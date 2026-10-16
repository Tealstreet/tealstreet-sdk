// Accepted-set instrumentation: retain the existing scripted I/O and production
// language operations. No venue method or decoder is replaced by this fixture.
#[allow(dead_code)]
pub mod io {
    use crate as safe_cex_venue_kucoin;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/exchange_runtime.rs"
    ));
}
use safe_cex_exchange::js_runtime::{
    globals::{install_builtins, install_enums},
    library::install_library,
    scope::{Scope, ScopeShared},
    value::Js,
};
use std::{cell::RefCell, rc::Rc};

pub fn host() -> Scope<crate::binding::Extra> {
    let runtime = Rc::new(io::Runtime::default());
    let options = serde_json::from_value(serde_json::json!({
        "key":"synthetic-key","secret":"synthetic-secret","passphrase":"synthetic-pass"
    }))
    .unwrap();
    let api = Rc::new_cyclic(|me| crate::binding::http::ApiHost {
        me: me.clone(),
        runtime: runtime.clone(),
        options,
        hosted_signer: None,
        spot: false,
        http: Rc::new(|_| panic!("pure language fixture must not request HTTP")),
        registry: Rc::new(Default::default()),
        clock: Default::default(),
        retry_clock: runtime.clone(),
        api: RefCell::new(None),
    });
    let shared = Rc::new(ScopeShared::new(
        runtime.clone(),
        api.clone(),
        Rc::new(safe_cex_store::store::DefaultStore::new()),
        Js::object(vec![]),
        crate::binding::Extra {
            supers: RefCell::default(),
            parts: RefCell::new(None),
            api,
        },
    ));
    let weak = Rc::downgrade(&shared);
    install_builtins(&shared.globals, weak.clone());
    install_enums(&shared.globals);
    install_library(&shared.globals, weak.clone());
    let ws: Rc<dyn safe_cex_exchange::websocket::WsRuntime> = runtime;
    crate::binding::globals::install(&shared.globals, weak, &ws).unwrap();
    let this = Js::object(vec![(
        "store",
        safe_cex_exchange::js_runtime::store::facade(shared.store.clone()),
    )]);
    *shared.exchange.borrow_mut() = this.clone();
    Scope { shared, this }
}

/// Inspect the actual primitive VM result without serializing/redecoding it.
pub fn primitive(
    result: Result<Js, safe_cex_exchange::js_runtime::value::Thrown>,
) -> serde_json::Value {
    use serde_json::json;
    match result {
        Ok(Js::Undefined) => json!({"kind":"undefined"}),
        Ok(Js::Null) => json!({"kind":"null"}),
        Ok(Js::Bool(value)) => json!({"kind":"boolean","value":value}),
        Ok(Js::String(value)) => json!({"kind":"string","units":value.as_ref()}),
        Ok(Js::Number(value)) => json!({"kind":"number","nan":value.is_nan(),
            "bits":if value.is_nan(){None}else{Some(format!("{:016x}",value.to_bits()))}}),
        Ok(_) => panic!("primitive native observation requires a primitive result"),
        Err(error) => json!({"kind":"error","name":error.value.get("name").unwrap().to_display(),
            "message":error.value.get("message").unwrap().to_display()}),
    }
}

pub fn market(id: &str, symbol: &str, contract_size: Option<f64>) -> safe_cex_types::Market {
    use safe_cex_types::{
        Market, MarketLimits, MarketLimitsAmount, MarketLimitsLeverage, MarketPrecision,
    };
    Market {
        id: id.into(),
        symbol: symbol.into(),
        pretty_symbol: None,
        base: "NATIVE".into(),
        quote: "USDT".into(),
        active: true,
        precision: MarketPrecision {
            amount: 0.5,
            price: 0.25,
        },
        limits: MarketLimits {
            amount: MarketLimitsAmount {
                min: 0.5,
                max: 1000.0,
            },
            leverage: MarketLimitsLeverage {
                min: 1.0,
                max: 125.0,
            },
            price: None,
        },
        option: None,
        contract_size,
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

pub fn seed_market(store: &dyn safe_cex_store::Store, market: safe_cex_types::Market) {
    safe_cex_store::Store::update(
        store,
        safe_cex_store::StoreUpdate {
            markets: Some(vec![market]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
}

pub fn order(value: &safe_cex_types::Order) -> serde_json::Value {
    serde_json::json!({"id":value.id,"symbol":value.symbol,
        "side":format!("{:?}",value.side),"type":format!("{:?}",value.r#type),
        "status":format!("{:?}",value.status),"tif":format!("{:?}",value.time_in_force),
        "bits":([value.price,value.amount,value.filled,value.remaining,value.last_update]
            .map(|v|format!("{:016x}",v.to_bits()))),"reduce":value.reduce_only,
        "remainingNativeBits":value.extra.as_ref().and_then(|v|v.remaining_native).map(|v|format!("{:016x}",v.to_bits())),
        "infoId":value.info.as_ref().and_then(|v|v.get("id"))})
}

pub fn connect_read(
    value: &Js,
    key: &str,
) -> Result<Js, safe_cex_exchange::js_runtime::value::Thrown> {
    match key {
        "instanceServers?.0" => {
            let servers = value.get("instanceServers")?;
            if servers.is_nullish() {
                Ok(Js::Undefined)
            } else {
                servers.get("0")
            }
        }
        "?.endpoint" if value.is_nullish() => Ok(Js::Undefined),
        "?.endpoint" => value.get("endpoint"),
        _ => value.get(key),
    }
}

pub fn ticker(symbol: &str, last: f64) -> safe_cex_types::Ticker {
    safe_cex_types::Ticker {
        id: "API".into(),
        symbol: symbol.into(),
        bid: 0.0,
        ask: 0.0,
        last,
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
