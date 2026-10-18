#![cfg(not(target_arch = "wasm32"))]
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::numeric_ingress::js_value_as_f64;
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_types::Market;
use safe_cex_venue_grvt::markets::{fetch_markets, refresh_markets};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};

fn run<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("MARKETS fixture unexpectedly pending"),
    }
}

fn oracle(script: &str) -> Value {
    let output = std::process::Command::new("node")
        .arg(format!("{}/tests/{script}", env!("CARGO_MANIFEST_DIR")))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "MARKETS oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn number(actual: f64, expected: &Value, assertion: &str) {
    let bits = if actual.is_nan() {
        0x7ff8000000000000
    } else {
        actual.to_bits()
    };
    assert_eq!(
        format!("{bits:016x}"),
        expected["bits"].as_str().unwrap(),
        "{assertion}"
    );
}

fn native_market(actual: &Market, expected: &Value) {
    assert_eq!(
        actual.id,
        expected["id"].as_str().unwrap(),
        "MARKETS native id"
    );
    assert_eq!(
        actual.symbol,
        expected["symbol"].as_str().unwrap(),
        "MARKETS native symbol"
    );
    assert_eq!(
        actual.base,
        expected["base"].as_str().unwrap(),
        "MARKETS native base"
    );
    assert_eq!(
        actual.quote,
        expected["quote"].as_str().unwrap(),
        "MARKETS native quote"
    );
    assert_eq!(
        actual.active,
        expected["active"].as_bool().unwrap(),
        "MARKETS native active"
    );
    assert_eq!(actual.is_spot, Some(false), "MARKETS native spot");
    assert_eq!(
        actual.is_perpetual,
        expected["isPerpetual"].as_bool(),
        "MARKETS native perpetual branch"
    );
    for (value, expected, name) in [
        (
            actual.precision.amount,
            &expected["precision"]["amount"],
            "amount step",
        ),
        (
            actual.precision.price,
            &expected["precision"]["price"],
            "price step",
        ),
        (
            actual.limits.amount.min,
            &expected["limits"]["amount"]["min"],
            "min amount",
        ),
        (
            actual.limits.amount.max,
            &expected["limits"]["amount"]["max"],
            "max amount",
        ),
        (
            actual.limits.leverage.min,
            &expected["limits"]["leverage"]["min"],
            "min leverage",
        ),
        (
            actual.limits.leverage.max,
            &expected["limits"]["leverage"]["max"],
            "max leverage",
        ),
    ] {
        number(value, expected, &format!("MARKETS native {name}"));
    }
    let info = actual.info.as_ref().unwrap();
    for key in ["ih", "bd", "qd"] {
        if expected["info"][key]["undefined"] == true {
            assert!(info.get(key).is_none(), "MARKETS absent JSON-data {key}");
        } else if expected["info"][key].get("bits").is_some() {
            number(
                js_value_as_f64(&info[key]).unwrap(),
                &expected["info"][key],
                &format!("MARKETS native info {key}"),
            );
        } else {
            assert_eq!(
                info[key], expected["info"][key],
                "MARKETS native info {key}"
            );
        }
    }
    assert!(
        actual.pretty_symbol.is_none()
            && actual.contract_size.is_none()
            && actual.is_expiring.is_none()
            && actual.is_inverse.is_none()
            && actual.is_read_only.is_none()
            && actual.taker_fee.is_none()
            && actual.maker_fee.is_none(),
        "MARKETS unassigned shared optionals"
    );
}

#[test]
fn fetch_markets_native_source_outcomes() {
    let capture = oracle("fetch-markets-source.cjs");
    let rows = capture["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 11, "MARKETS independent runtime floor");
    let names = [
        "missing",
        "null",
        "empty",
        "quoted",
        "one",
        "two",
        "later-null",
        "non-array",
        "defaults",
        "success-log-throws",
        "catch-log-throws",
    ];
    let counts = [
        Some(0),
        Some(0),
        Some(0),
        Some(0),
        Some(1),
        Some(2),
        None,
        None,
        Some(1),
        None,
        None,
    ];
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(
            row["name"], names[index],
            "MARKETS independent runtime input order"
        );
        let effects = RefCell::new(Vec::new());
        let mut logs = 0;
        let failure = match names[index] {
            "success-log-throws" => 1,
            "catch-log-throws" => 2,
            _ => 0,
        };
        let future = fetch_markets(
            |path, body| {
                assert_eq!(
                    path, "/lite/v1/all_instruments",
                    "MARKETS configured post endpoint"
                );
                assert_eq!(body, json!({}), "MARKETS configured post body");
                effects.borrow_mut().extend([
                    json!(["start"]),
                    json!(["request", path, "{}"]),
                    json!(["success"]),
                ]);
                std::future::ready(Ok(safe_cex_transport::how::json_parse(
                    row["raw"].as_str().unwrap(),
                )
                .into_value()
                .unwrap()))
            },
            |message, severity| {
                logs += 1;
                effects
                    .borrow_mut()
                    .push(json!(["log", message, severity.as_str()]));
                if logs <= failure {
                    Err(ExchangeError::new(format!("subscriber-{logs}")))
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(
            effects.borrow().len(),
            3,
            "MARKETS eager configured post hook"
        );
        match run(future) {
            Ok(markets) => {
                let count = counts[index].expect("MARKETS unexpected native success");
                assert_eq!(markets.len(), count, "MARKETS independent native count");
                let expected = row["result"].as_array().unwrap();
                assert_eq!(expected.len(), count, "MARKETS independent source count");
                for (market, expected) in markets.iter().zip(expected) {
                    native_market(market, expected);
                }
                assert!(row["error"].is_null(), "MARKETS source success");
            }
            Err(error) => {
                assert_eq!(counts[index], None, "MARKETS unexpected native rejection");
                assert_eq!(
                    error.js_error_string(),
                    row["error"].as_str().unwrap(),
                    "MARKETS native rejection order"
                );
            }
        }
        assert_eq!(
            *effects.borrow(),
            row["effects"].as_array().unwrap().to_vec(),
            "MARKETS configured hook and log order"
        );
    }
}

#[test]
fn fetch_markets_reports_typed_row_refusal_after_source_logging() {
    let effects = RefCell::new(Vec::new());
    let result = run(fetch_markets(
        |_, _| std::future::ready(Ok(json!({"r":[{"i":"x","b":{},"q":"USD"}]}))),
        |text, severity| {
            effects
                .borrow_mut()
                .push((text.to_owned(), severity.as_str().to_owned()));
            Ok(())
        },
    ));
    assert!(
        result.unwrap().is_empty(),
        "MARKETS refuses the bad row only"
    );
    assert_eq!(
        *effects.borrow(),
        [
            ("Fetched 1 GRVT markets".into(), "debug".into()),
            ("Refused GRVT market row 0 id=\"x\": base={} quote=\"USD\": GRVT market base and quote must be strings for the shared Market model".into(), "warning".into())
        ],
        "MARKETS refusal outside source catch"
    );
}

fn market(id: &str, hash: &str) -> Market {
    serde_json::from_value(json!({"id":id,"symbol":id.to_uppercase(),"base":"B","quote":"Q","active":true,
        "precision":{"amount":0.01,"price":0.2},"limits":{"amount":{"min":0.01,"max":20},"leverage":{"min":1,"max":100}},
        "isSpot":false,"isPerpetual":true,"info":{"ih":hash,"bd":3,"qd":6}})).unwrap()
}

#[test]
fn refresh_markets_declared_owned_old_value_boundary() {
    let capture = oracle("refresh-markets-source.cjs");
    let store = DefaultStore::new();
    Store::update(
        &store,
        StoreUpdate {
            markets: Some(vec![market("a", "old-a")]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let changes = run(refresh_markets(
        &store,
        || {
            Store::update(
                &store,
                StoreUpdate {
                    markets: Some(vec![market("a", "mutated-a")]),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
            std::future::ready(Ok(Vec::new()))
        },
        |_, _| Ok(()),
        |_, _| Ok(()),
    ))
    .unwrap();
    assert_eq!(
        changes.removed.len(),
        1,
        "REFRESH owned native removed count"
    );
    assert_eq!(
        changes.removed[0]
            .info
            .as_ref()
            .unwrap()
            .get("ih")
            .unwrap()
            .text_equals("old-a"),
        true,
        "REFRESH declared owned old metadata"
    );
    assert_eq!(
        capture["aliases"]["removed"].as_array().unwrap().len(),
        1,
        "REFRESH alias source removed count"
    );
    assert_eq!(
        capture["aliases"]["removed"][0]["info"]["ih"], "mutated-a",
        "REFRESH measured source live alias"
    );
}

#[test]
fn refresh_markets_native_store_and_effect_order() {
    let capture = oracle("refresh-markets-source.cjs");
    let rows = capture["rows"].as_array().unwrap();
    let names = [
        "metadata",
        "membership",
        "reorder",
        "fetch-error",
        "store-error",
        "emit-error",
        "log-error",
    ];
    assert_eq!(rows.len(), 7, "REFRESH independent floor");
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(row["name"], names[index], "REFRESH input order");
        let store = DefaultStore::new();
        Store::update(
            &store,
            StoreUpdate {
                markets: Some(vec![market("a", "old-a"), market("b", "old-b")]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let effects = Rc::new(RefCell::new(Vec::new()));
        let observer = effects.clone();
        let _cleanup = Store::subscribe(
            &store,
            Box::new(move |store, _| {
                observer.borrow_mut().push(json!([
                    "store",
                    store.markets.iter().map(|m| &m.id).collect::<Vec<_>>()
                ]));
                if index == 4 {
                    Err(ExchangeError::new("store-fail").into())
                } else {
                    Ok(())
                }
            }),
        );
        let ids: &[&str] = if index == 0 {
            &["a", "b"]
        } else if index == 2 {
            &["b", "a"]
        } else {
            &["b", "c"]
        };
        let fetched: Vec<_> = ids
            .iter()
            .map(|id| market(id, &format!("new-{id}")))
            .collect();
        let result = run(refresh_markets(
            &store,
            || {
                effects.borrow_mut().push(json!(["fetch"]));
                std::future::ready(if index == 3 {
                    Err(ExchangeError::new("fetch-fail"))
                } else {
                    Ok(fetched.clone())
                })
            },
            |event, changes| {
                effects.borrow_mut().push(json!([
                    "emit",
                    event,
                    changes.added.iter().map(|m| &m.id).collect::<Vec<_>>(),
                    changes.removed.iter().map(|m| &m.id).collect::<Vec<_>>()
                ]));
                if index == 5 {
                    Err(ExchangeError::new("emit-fail"))
                } else {
                    Ok(())
                }
            },
            |text, severity| {
                effects
                    .borrow_mut()
                    .push(json!(["log", text, severity.as_str()]));
                if index == 6 {
                    Err(ExchangeError::new("log-fail"))
                } else {
                    Ok(())
                }
            },
        ));
        let current = store.markets();
        let expected = row["markets"].as_array().unwrap();
        assert_eq!(current.len(), 2, "REFRESH independent native count");
        assert_eq!(expected.len(), 2, "REFRESH independent source count");
        for (market, expected) in current.iter().zip(expected) {
            assert_eq!(
                market.id,
                expected["id"].as_str().unwrap(),
                "REFRESH native stored id"
            );
            assert_eq!(
                market.info.as_ref().unwrap().get("ih").unwrap(),
                expected["info"]["ih"],
                "REFRESH native replaced signing hash"
            );
            assert_eq!(
                market.is_perpetual,
                Some(true),
                "REFRESH native shared perpetual"
            );
        }
        match result {
            Ok(changes) => {
                assert_eq!(
                    changes.added.len(),
                    usize::from(index == 1),
                    "REFRESH independent added count"
                );
                assert_eq!(
                    changes.removed.len(),
                    usize::from(index == 1),
                    "REFRESH independent removed count"
                );
                for (market, expected) in changes
                    .added
                    .iter()
                    .zip(row["result"]["added"].as_array().unwrap())
                {
                    assert_eq!(
                        market.id,
                        expected["id"].as_str().unwrap(),
                        "REFRESH native added id"
                    );
                    assert_eq!(
                        market.info.as_ref().unwrap().get("ih").unwrap(),
                        expected["info"]["ih"],
                        "REFRESH native added hash"
                    );
                }
                for (market, expected) in changes
                    .removed
                    .iter()
                    .zip(row["result"]["removed"].as_array().unwrap())
                {
                    assert_eq!(
                        market.id,
                        expected["id"].as_str().unwrap(),
                        "REFRESH native removed id"
                    );
                    assert_eq!(
                        market.info.as_ref().unwrap().get("ih").unwrap(),
                        expected["info"]["ih"],
                        "REFRESH native removed hash"
                    );
                }
                assert!(row["error"].is_null(), "REFRESH source success");
            }
            Err(error) => assert_eq!(
                error.js_error_string(),
                row["error"].as_str().unwrap(),
                "REFRESH native error"
            ),
        }
        assert_eq!(
            *effects.borrow(),
            row["effects"].as_array().unwrap().to_vec(),
            "REFRESH native effect order"
        );
    }
}
