#[path = "../../safe-cex-exchange/tests/support/runtime.rs"]
mod runtime;
use runtime::Runtime;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange},
};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{ExchangeName, Market, Ticker};
use safe_cex_webworker::{
    chaser::ChaserOrderInstance, chaser_manager::find_best_price, task_types::ChaserOrderOptions,
};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};
struct Venue {
    base: BaseExchange,
    capabilities: ExchangeCapabilities,
}
impl Exchange for Venue {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
}
fn number(v: &Value) -> f64 {
    if v == "NaN" {
        f64::NAN
    } else {
        f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap())
    }
}
fn bits(v: f64) -> String {
    if v.is_nan() {
        "NaN".into()
    } else {
        format!("{:016x}", v.to_bits())
    }
}
fn run(row: &Value) -> Value {
    let symbol = row["symbol"].as_str().unwrap();
    let mut market:Market=serde_json::from_value(json!({"id":"market","symbol":if row["poison"]==true {"__proto__"} else {symbol},"base":"BTC","quote":"USD","active":true,"precision":{"amount":1,"price":1},"limits":{"amount":{"min":0,"max":100},"leverage":{"min":1,"max":100}}})).unwrap();
    market.precision.price = number(&row["precision"]);
    let mut ticker:Ticker=serde_json::from_value(json!({"id":"ticker","symbol":symbol,"bid":0,"ask":0,"last":0,"mark":0,"index":0,"percentage":0,"openInterest":0,"fundingRate":0,"fundingTime":0,"volume":0,"quoteVolume":0})).unwrap();
    ticker.bid = number(&row["prices"][0]);
    ticker.ask = number(&row["prices"][1]);
    ticker.last = number(&row["prices"][2]);
    ticker.mark = ticker.last;
    ticker.index = ticker.last;
    let availability = row["availability"].as_str().unwrap();
    let store = Rc::new(DefaultStore::new());
    let mut markets = if availability == "ticker" || availability == "none" {
        vec![]
    } else {
        vec![market.clone()]
    };
    let mut tickers = if availability == "market" || availability == "none" {
        vec![]
    } else {
        vec![ticker.clone()]
    };
    if row["duplicate"] == true {
        market.precision.price *= 2.0;
        markets.push(market);
        ticker.bid = 80.0;
        ticker.ask = 85.0;
        ticker.last = 82.0;
        ticker.mark = 82.0;
        ticker.index = 82.0;
        tickers.push(ticker);
    }
    let runtime = Rc::new(Runtime::default());
    runtime.advance(0);
    Store::update(
        store.as_ref(),
        StoreUpdate {
            markets: Some(markets),
            tickers: Some(tickers),
            ..Default::default()
        },
        true,
    )
    .unwrap();
    let venue = Rc::new(Venue {
        base: BaseExchange::new(
            ExchangeName::Bybit,
            serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
            store,
            runtime,
            Default::default(),
        ),
        capabilities: Default::default(),
    });
    let options:ChaserOrderOptions=serde_json::from_value(json!({"symbol":symbol,"side":row["side"],"size":1,"maxAttempts":1,"delay":1,"reduceOnly":true})).unwrap();
    let options = Rc::new(RefCell::new(options));
    let mutate = row["mutate"] == true;
    let events = Rc::new(RefCell::new(Vec::<Value>::new()));
    for event in ["log", "error"] {
        let events = events.clone();
        let options = options.clone();
        let throws = row["listenerThrow"] == event;
        venue.base.on(
            event,
            Rc::new(move |event, data| {
                events.borrow_mut().push(json!([
                    event,
                    safe_cex_exchange::js_runtime::value::to_json(data).unwrap()
                ]));
                if mutate {
                    options.borrow_mut().symbol = if event == "log" {
                        "from-log"
                    } else {
                        "from-error"
                    }
                    .into();
                }
                if throws {
                    let mut error =
                        safe_cex_exchange::runtime::ExchangeError::new("listener failed");
                    error.name = "TypeError".into();
                    return Err(error);
                }
                Ok(())
            }),
        );
    }
    let instance = ChaserOrderInstance::new(venue, options).unwrap();
    instance.set_min_tick_distance(number(&row["distance"]));
    let result = match find_best_price(&instance) {
        Ok(v) => json!({"price":bits(v)}),
        Err(e) => json!({"error":{"name":e.name,"message":e.message}}),
    };
    json!({"result":result,"events":*events.borrow(),"stopped":instance.is_stopped(),"reason":instance.state().borrow().stop_reason})
}
#[test]
fn chaser_price_matches_live_store_lookup_and_numeric_paths() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/chaser_price_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = source["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 479);
    let precision_identity: Vec<_> = rows
        .iter()
        .filter(|row| {
            number(&row["precision"]) == 0.01
                && number(&row["prices"][0]) == 100.0
                && number(&row["prices"][1]) == 102.0
                && matches!(number(&row["distance"]), 1.0 | 2.5)
        })
        .collect();
    assert_eq!(
        precision_identity.len(),
        4,
        "CHASER-PRECISION corpus identity"
    );
    for row in precision_identity {
        assert_eq!(
            run(row)["result"]["price"],
            row["result"]["price"],
            "CHASER-PRECISION native price step differs from amount step: {} distance {}",
            row["side"],
            row["distance"]
        );
    }
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(
            run(row),
            json!({"result":row["result"],"events":row["events"],"stopped":row["stopped"],"reason":row["reason"]}),
            "CHASER-PRICE {index}"
        );
    }
    assert!(
        serde_json::from_str::<Value>(source["loneSurrogate"].as_str().unwrap()).is_err(),
        "shared UTF-8 boundary refuses source-accepted lone surrogate"
    );
    for row in rows.iter().filter(|r| {
        r["availability"] == "none"
            && r["symbol"] != "missing"
            && r["symbol"] != ""
            && r["symbol"] != " BTC🚀\t"
    }) {
        assert_eq!(
            row["lookupKinds"],
            if row["symbol"] == "__proto__" {
                json!(["object", "object"])
            } else {
                json!(["function", "function"])
            }
        );
    }
    let poisoned = &source["poisoned"];
    assert_eq!(poisoned["result"]["error"]["name"], "TypeError");
    assert_eq!(poisoned["stopped"], false);
    assert_eq!(poisoned["events"], json!([]));
    let ours = run(poisoned);
    assert_eq!(
        ours["result"]["error"]["name"], "Error",
        "FINDING shared Store prototype writes are not represented"
    );
    assert_eq!(ours["stopped"], true);
    assert_eq!(
        ours["events"][1],
        json!(["error", "Chaser no market or ticker found for precision"]),
        "POISONED-MAP visible error"
    );
    assert_eq!(
        ours["result"]["error"]["message"], "No market or ticker found for precision",
        "POISONED-MAP caller receives Err"
    );
}

#[test]
fn chaser_price_preserves_fallible_listener_order() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/chaser_price_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let listener_rows = source["listenerThrows"].as_array().unwrap();
    assert_eq!(
        listener_rows
            .iter()
            .map(|row| row["listenerThrow"].clone())
            .collect::<Vec<_>>(),
        vec![json!("log"), json!("error")],
        "LISTENER-THROW corpus inventory"
    );
    for row in listener_rows {
        let log = row["listenerThrow"] == "log";
        assert_eq!(
            row["result"]["error"],
            json!({"name":"TypeError","message":"listener failed"})
        );
        assert_eq!(
            row["events"].as_array().unwrap().len(),
            if log { 1 } else { 2 }
        );
        assert_eq!(row["stopped"], !log, "LISTENER-THROW stop ordering");
        assert_eq!(
            row["reason"],
            if log { "" } else { "No market or ticker found" }
        );
        assert_eq!(
            run(row),
            json!({"result":row["result"],"events":row["events"],"stopped":row["stopped"],"reason":row["reason"]}),
            "LISTENER-THROW native error and stop ordering"
        );
    }
}
