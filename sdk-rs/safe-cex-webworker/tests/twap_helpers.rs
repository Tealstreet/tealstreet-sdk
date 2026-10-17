#[path = "../../safe-cex-exchange/tests/support/runtime.rs"]
mod runtime;
use safe_cex_exchange::{
    capabilities::{ExchangeCapabilities, PlaceOrderBinding},
    exchange::{BaseExchange, Exchange},
    runtime::ExchangeError,
};
use safe_cex_store::store::DefaultStore;
use safe_cex_types::{ExchangeName, PlaceOrderOpts};
use safe_cex_webworker::{
    task_types::TwapOptions,
    twap::{fire_market_remainder, slice_interval_ms},
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::{Future, poll_fn},
    rc::Rc,
    task::{Context, Poll, Waker},
};
struct Venue {
    base: BaseExchange,
    capabilities: ExchangeCapabilities,
}
impl Exchange for Venue {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
}
fn number(value: &Value) -> f64 {
    let value = value.as_str().unwrap();
    if value == "NaN" {
        f64::NAN
    } else {
        f64::from_bits(u64::from_str_radix(value, 16).unwrap())
    }
}
fn bits(value: f64) -> String {
    if value.is_nan() {
        "NaN".into()
    } else {
        format!("{:016x}", value.to_bits())
    }
}
fn options() -> TwapOptions {
    serde_json::from_value(json!({
        "accountName":"account", "activeMarket":{"id":"BTC","symbol":" BTC🚀\t","base":"BTC","quote":"USD","active":true,"precision":{"amount":0.01,"price":0.01},"limits":{"amount":{"min":0,"max":100},"leverage":{"min":1,"max":100}}},
        "size":1,"duration":1,"ordersCount":1,"side":"buy","chaserDelay":100,"reduceOnly":false,
        "randomnessPercentage":0,"useLimitOrders":false,"useChaserOrders":false
    })).unwrap()
}
#[test]
fn intervals_and_best_effort_remainders_match_live_twap() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/twap_helpers_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    let mut options = options();
    let intervals = data["intervals"].as_array().unwrap();
    assert_eq!(intervals.len(), 2337);
    for (index, row) in intervals.iter().enumerate() {
        options.duration = number(&row["duration"]);
        options.orders_count = number(&row["ordersCount"]);
        assert_eq!(
            bits(slice_interval_ms(&options)),
            row["value"],
            "SLICE-INTERVAL bits row {index}"
        );
    }
    let rows = data["remainders"].as_array().unwrap();
    assert_eq!(rows.len(), 204);
    for (index, row) in rows.iter().enumerate() {
        options.side = serde_json::from_value(row["side"].clone()).unwrap();
        options.reduce_only = row["reduceOnly"].as_bool().unwrap();
        let calls = Rc::new(RefCell::new(Vec::<Value>::new()));
        let released = Rc::new(Cell::new(false));
        let runtime = Rc::new(runtime::Runtime::default());
        runtime.advance(0);
        let store = Rc::new(DefaultStore::new());
        let record = calls.clone();
        let release = released.clone();
        let outcome = row["outcome"].as_str().unwrap().to_owned();
        let capabilities = ExchangeCapabilities {
            place_order: PlaceOrderBinding::implemented::<Venue>(move |venue, token| {
                assert_eq!(venue.base.name, ExchangeName::Bybit, "REMAINDER-TARGET");
                let order = token.options();
                let mut value = serde_json::to_value(order).unwrap();
                value["amount"] = json!(bits(order.amount));
                record.borrow_mut().push(value);
                let release = release.clone();
                let outcome = outcome.clone();
                Box::pin(async move {
                    if outcome == "error" {
                        return Err(ExchangeError::new("placement fixture"));
                    }
                    if outcome == "pending" {
                        poll_fn(|_| {
                            if release.get() {
                                Poll::Ready(())
                            } else {
                                Poll::Pending
                            }
                        })
                        .await;
                    }
                    Ok(vec![])
                })
            }),
            ..Default::default()
        };
        let venue = Venue {
            base: BaseExchange::new(
                ExchangeName::Bybit,
                serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
                store.clone(),
                runtime.clone(),
                Default::default(),
            ),
            capabilities,
        };
        let mut future = Box::pin(fire_market_remainder(
            &venue,
            &options,
            number(&row["amount"]),
        ));
        let mut cx = Context::from_waker(Waker::noop());
        let ready = future.as_mut().poll(&mut cx).is_ready();
        assert_eq!(
            ready,
            row["settledBeforeRelease"].as_bool().unwrap(),
            "REMAINDER-AWAIT row {index}"
        );
        assert_eq!(
            json!(*calls.borrow()),
            row["orders"],
            "REMAINDER-ORDER row {index}"
        );
        if !ready {
            released.set(true);
            assert!(
                future.as_mut().poll(&mut cx).is_ready(),
                "REMAINDER-RELEASE row {index}"
            );
        }
        assert_eq!(row["settled"], true, "REMAINDER-ERROR-RECOVERY row {index}");
        assert!(
            runtime.tasks.borrow().is_empty(),
            "remainder must remain caller-owned"
        );
    }
    assert_eq!(data["loneSurrogate"]["codeUnit"], 0xd800);
    let text = data["loneSurrogate"]["json"].as_str().unwrap();
    assert!(text.contains("\\ud800"));
    assert!(
        serde_json::from_str::<Value>(text).is_err(),
        "shared UTF-8 ingress refuses a valid JS string"
    );
}
