use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::numeric_ingress::js_from_str;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{Market, StoreSubscriberError};
use safe_cex_venue_bitrue::{leverage_reads::*, wire::OpaqueValue};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::{Future, poll_fn},
    pin::pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn run<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..3 {
        if let Poll::Ready(v) = future.as_mut().poll(&mut cx) {
            return v;
        }
    }
    panic!("unexpected pending read");
}
fn bits(v: f64) -> String {
    format!(
        "{:016x}",
        if v.is_nan() {
            0x7ff8000000000000
        } else {
            v.to_bits()
        }
    )
}
fn native(m: &Market) -> Value {
    json!({"id":m.id,"symbol":m.symbol,"amountMin":bits(m.limits.amount.min),"amountMax":bits(safe_cex_num::numeric_value_to_number(&m.limits.amount.max)),"leverageMin":bits(m.limits.leverage.min),"leverageMax":bits(m.limits.leverage.max)})
}
#[test]
fn leverage_reads_match_actual_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/leverage-reads-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
    println!("capture {}", capture["header"]);
    let rows = capture["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 4860, "BITRUE-LEVERAGE-READ source total");
    let mut counts = [0; 3];
    let mut compared = 0;
    for (index, row) in rows.iter().enumerate() {
        let store = DefaultStore::default();
        Store::update(
            &store,
            StoreUpdate {
                markets: Some(vec![
                    market(
                        "ID-A",
                        "BTC",
                        f64::from_bits(
                            u64::from_str_radix(row["initial"].as_str().unwrap(), 16).unwrap(),
                        ),
                    ),
                    market("ID-B", "ETH", 19.0),
                ]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let notifications = Rc::new(Cell::new(0));
        let later = Rc::new(Cell::new(0));
        let n = notifications.clone();
        let l = later.clone();
        let subscriber_fault = row["fault"] == "subscriber";
        let _first = Store::subscribe(
            &store,
            Box::new(move |_, _| {
                n.set(n.get() + 1);
                if subscriber_fault {
                    Err(StoreSubscriberError::new("subscriber rejected"))
                } else {
                    Ok(())
                }
            }),
        );
        let _second = Store::subscribe(
            &store,
            Box::new(move |_, _| {
                l.set(l.get() + 1);
                Ok(())
            }),
        );
        let before = store.get_markets_version();
        let calls = RefCell::new(Vec::<Value>::new());
        let events = RefCell::new(Vec::<&str>::new());
        let get = |endpoint: &'static str, config: Value| {
            calls
                .borrow_mut()
                .push(json!({"url":endpoint,"config":config}));
            async {
                events.borrow_mut().push("request");
                let mut pending = true;
                poll_fn(|cx| {
                    if pending {
                        pending = false;
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    } else {
                        Poll::Ready(())
                    }
                })
                .await;
                events.borrow_mut().push("response");
                if row["fault"] == "request" {
                    return Err(ExchangeError::new("request rejected"));
                }
                Ok(row["raw"]
                    .as_str()
                    .map(|raw| js_from_str::<OpaqueValue>(raw).unwrap().value))
            }
        };
        let symbol = row["symbol"].as_str().unwrap();
        let public = row["publicOnly"].as_bool().unwrap();
        let get = |url, config| Ok(get(url, config));
        let result = match row["method"].as_str().unwrap() {
            "fetchAndSyncSymbolMaxLeverage" => {
                counts[0] += 1;
                match run(fetch_and_sync_symbol_max_leverage(
                    &store, symbol, public, get,
                )) {
                    Some(v) => json!({"kind":"number","bits":bits(v)}),
                    None => json!({"kind":"null"}),
                }
            }
            "fetchSymbolMaxLeverage" => {
                counts[1] += 1;
                json!({"kind":"number","bits":bits(run(fetch_symbol_max_leverage(&store,symbol,public,get)).unwrap())})
            }
            "updateSymbolMetadata" => {
                counts[2] += 1;
                run(update_symbol_metadata(&store, symbol, public, get));
                json!({"kind":"undefined"})
            }
            _ => panic!("unknown source method"),
        };
        assert_eq!(result, row["result"], "BITRUE-LEVERAGE-READ result {index}");
        assert_eq!(
            json!(*calls.borrow()),
            row["calls"],
            "BITRUE-LEVERAGE-READ native request {index}"
        );
        assert_eq!(
            json!(*events.borrow()),
            row["events"],
            "BITRUE-LEVERAGE-READ awaited events {index}"
        );
        assert_eq!(
            notifications.get(),
            row["notifications"].as_u64().unwrap(),
            "BITRUE-LEVERAGE-READ first subscriber {index}"
        );
        assert_eq!(
            later.get(),
            row["later"].as_u64().unwrap(),
            "BITRUE-LEVERAGE-READ later subscriber {index}"
        );
        assert_eq!(
            store.get_markets_version() - before,
            safe_cex_num::numeric_ingress::js_value_as_f64(&row["version"]).unwrap(),
            "BITRUE-LEVERAGE-READ committed version {index}"
        );
        let expected = row["markets"].as_array().unwrap();
        assert_eq!(
            expected.len(),
            2,
            "BITRUE-LEVERAGE-READ source markets {index}"
        );
        let actual = store.markets().unwrap();
        assert_eq!(
            actual.len(),
            expected.len(),
            "BITRUE-LEVERAGE-READ native markets {index}"
        );
        for i in 0..2 {
            assert_eq!(
                native(&actual[i]),
                expected[i],
                "BITRUE-LEVERAGE-READ committed native fields {index}/{i}"
            );
            compared += 1;
        }
    }
    assert_eq!(
        counts,
        [1620, 1620, 1620],
        "BITRUE-LEVERAGE-READ per-method census"
    );
    assert_eq!(
        compared, 9720,
        "BITRUE-LEVERAGE-READ executed market comparisons"
    );
    typed_utf16::assert_projection(
        capture["surrogateRaw"].as_str().unwrap(),
        &(js_from_str::<String>(capture["surrogateRaw"].as_str().unwrap()))
            .expect("typed surrogate record is retained"),
    );
    let supplements = capture["supplements"].as_array().unwrap();
    assert_eq!(supplements.len(), 6, "CHOSEN read supplement source count");
    assert_eq!(capture["largeCount"], 200000, "CHOSEN read argument count");
    for supplement in supplements {
        let method = supplement["method"].as_str().unwrap();
        let is_update = method == "updateSymbolMetadata";
        if supplement["kind"] == "utf8" {
            assert_eq!(
                supplement["result"],
                if is_update {
                    json!({"kind":"undefined"})
                } else {
                    json!({"kind":"number","bits":bits(31.0)})
                },
                "CHOSEN source accepts surrogate symbol"
            );
            continue;
        }
        assert_eq!(
            supplement["maximum"],
            bits(7.0),
            "CHOSEN source catch preserves Store on argument RangeError"
        );
        assert_eq!(
            supplement["result"],
            if is_update {
                json!({"kind":"undefined"})
            } else if method == "fetchSymbolMaxLeverage" {
                json!({"kind":"number","bits":bits(7.0)})
            } else {
                json!({"kind":"null"})
            },
            "CHOSEN source argument-limit result"
        );
        let store = DefaultStore::default();
        Store::update(
            &store,
            StoreUpdate {
                markets: Some(vec![market("ID-A", "BTC", 7.0)]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let data = json!({"code":0,"data":{"brackets":(0..200000).map(|i|json!({"initialLeverage":2+i%2})).collect::<Vec<_>>()}});
        let get = |_, _| Ok(async { Ok(Some(data)) });
        match method {
            "fetchAndSyncSymbolMaxLeverage" => assert_eq!(
                run(fetch_and_sync_symbol_max_leverage(
                    &store, "BTC", false, get
                )),
                Some(3.0),
                "CHOSEN iterative bracket maximum"
            ),
            "fetchSymbolMaxLeverage" => assert_eq!(
                run(fetch_symbol_max_leverage(&store, "BTC", false, get)).unwrap(),
                3.0,
                "CHOSEN iterative public maximum"
            ),
            "updateSymbolMetadata" => run(update_symbol_metadata(&store, "BTC", false, get)),
            _ => panic!("unknown supplement method"),
        }
        assert_eq!(
            store.markets().unwrap()[0].limits.leverage.max,
            3.0,
            "CHOSEN iterative maximum commits"
        );
    }
}

fn market(id: &str, symbol: &str, max: f64) -> Market {
    let raw=json!({"id":id,"symbol":symbol,"base":"BTC","quote":"USDT","active":true,
      "precision":{"amount":0.01,"price":0.02},"limits":{"amount":{"min":3,"max":71},"leverage":{"min":2,"max":7}},"info":{"sentinel":"preserve"}}).to_string();
    let mut market: Market = js_from_str(&raw).unwrap();
    market.limits.leverage.max = max;
    market
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;
