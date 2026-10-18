use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::Ticker;
use safe_cex_venue_phemex::ticker_parser::{fetch_tickers, parse_tickers};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    pin::pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("pending fixture"),
    }
}
fn bits(value: f64) -> Value {
    json!({"kind":"number","bits":if value.is_nan(){"NaN".into()}else{format!("{:016x}",value.to_bits())}})
}
fn capture(value: Option<&Value>) -> Value {
    match value {
        None => json!({"kind":"undefined"}),
        Some(Value::Null) => json!({"kind":"null"}),
        Some(Value::String(v)) => json!({"kind":"string","value":v}),
        _ => panic!("emitter fixture outside fixed string/undefined inventory"),
    }
}
struct Failure {
    error: ExchangeError,
    identity: Option<Rc<()>>,
}
impl From<ExchangeError> for Failure {
    fn from(error: ExchangeError) -> Self {
        Self {
            error,
            identity: None,
        }
    }
}
fn compare(
    native: Result<Vec<Ticker>, Failure>,
    case: &Value,
    count: i32,
    totals: &mut (usize, usize, usize, usize),
    identity: Option<&Rc<()>>,
) {
    let expected = &case["expected"];
    match native {
        Ok(rows) => {
            assert_eq!(expected["ok"], true, "PHEMEX-PARSER outcome: {case}");
            let items = expected["value"]["items"].as_array().unwrap();
            assert_eq!(
                items.len() as i32,
                count,
                "PHEMEX-PARSER source per-case output count: {case}"
            );
            assert_eq!(
                rows.len() as i32,
                count,
                "PHEMEX-PARSER independent native output count: {case}"
            );
            assert_eq!(
                rows.len(),
                items.len(),
                "PHEMEX-PARSER expected/native output count: {case}"
            );
            for index in 0..usize::try_from(count).unwrap() {
                let ticker = rows
                    .get(index)
                    .expect("PHEMEX-PARSER fixed native output coverage");
                let entries = items[index]["entries"].as_array().unwrap();
                let names = [
                    "id",
                    "symbol",
                    "bid",
                    "ask",
                    "last",
                    "mark",
                    "index",
                    "percentage",
                    "openInterest",
                    "fundingRate",
                    "fundingTime",
                    "volume",
                    "quoteVolume",
                ];
                assert_eq!(
                    entries
                        .iter()
                        .map(|v| v[0].as_str().unwrap())
                        .collect::<Vec<_>>(),
                    names,
                    "PHEMEX-PARSER fixed field inventory"
                );
                let field = |i: usize| &entries[i][1];
                assert_eq!(
                    &json!({"kind":"string","value":ticker.id}),
                    field(0),
                    "PHEMEX-PARSER native id"
                );
                assert_eq!(
                    &json!({"kind":"string","value":ticker.symbol}),
                    field(1),
                    "PHEMEX-PARSER native symbol"
                );
                for (i, (name, value)) in [
                    ("bid", ticker.bid),
                    ("ask", ticker.ask),
                    ("last", ticker.last),
                    ("mark", ticker.mark),
                    ("index", ticker.index.unwrap()),
                    ("percentage", ticker.percentage),
                    ("openInterest", ticker.open_interest),
                    ("fundingRate", ticker.funding_rate),
                    ("fundingTime", ticker.funding_time),
                    ("volume", ticker.volume),
                    ("quoteVolume", ticker.quote_volume),
                ]
                .into_iter()
                .enumerate()
                {
                    assert_eq!(
                        &bits(value),
                        field(i + 2),
                        "PHEMEX-PARSER native {name}: {case}"
                    );
                    totals.1 += 1;
                }
                assert!(
                    ticker.mid_price.is_none() && ticker.info.is_none(),
                    "PHEMEX-PARSER absent metadata"
                );
                totals.1 += 2;
                totals.0 += 1;
            }
        }
        Err(failure) => {
            assert_eq!(count, -1, "PHEMEX-PARSER expected failure branch");
            assert_eq!(
                expected["ok"], false,
                "PHEMEX-PARSER rejected outcome: {case}"
            );
            assert_eq!(
                failure.error.name, expected["name"],
                "PHEMEX-PARSER error name"
            );
            assert_eq!(
                failure.error.message, expected["message"],
                "PHEMEX-PARSER error message: {case}"
            );
            if let Some(identity) = identity {
                assert!(
                    failure
                        .identity
                        .as_ref()
                        .is_some_and(|v| Rc::ptr_eq(identity, v)),
                    "PHEMEX-PARSER callback rejection identity"
                );
                totals.3 += 1;
            }
            totals.2 += 1;
        }
    }
}
#[test]
fn ticker_parser_and_read_match_actual_typescript() {
    let output = Command::new("node")
        .arg(format!(
            "{}/tests/ticker_parser_oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    eprintln!("ticker parser capture profile: {}", corpus["profile"]);
    assert_eq!(
        corpus["cases"].as_array().unwrap().len(),
        41,
        "PHEMEX-PARSER source case total"
    );
    assert_eq!(
        corpus["reads"].as_array().unwrap().len(),
        6,
        "PHEMEX-PARSER source read total"
    );
    let store = DefaultStore::new();
    Store::update(
        &store,
        StoreUpdate {
            markets: Some(
                safe_cex_num::numeric_ingress::js_from_value(corpus["markets"].clone()).unwrap(),
            ),
            tickers: Some(
                safe_cex_num::numeric_ingress::js_from_value(corpus["cached"].clone()).unwrap(),
            ),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    assert_eq!(
        store.markets().unwrap().len(),
        4,
        "PHEMEX-PARSER fixture markets"
    );
    assert_eq!(
        store.tickers().unwrap().len(),
        2,
        "PHEMEX-PARSER fixture cached tickers"
    );
    let counts = [
        2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 0, 2, 2, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
        -1, -1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, -1, 1,
    ];
    let mut totals = (0, 0, 0, 0);
    let mut cases = 0;
    for index in 0..41 {
        let case = &corpus["cases"][index];
        let data = case["raw"]
            .as_str()
            .map(|raw| serde_json::from_str::<Value>(raw).unwrap());
        compare(
            parse_tickers(data.as_ref(), &store).map_err(Failure::from),
            case,
            counts[index],
            &mut totals,
            None,
        );
        cases += 1;
    }
    assert_eq!(
        totals,
        (49, 637, 15, 0),
        "PHEMEX-PARSER parser comparison census"
    );
    let calls = RefCell::new((0, 0));
    for index in 0..6 {
        let case = &corpus["reads"][index];
        if index >= 3 {
            assert_eq!(
                case["expected"]["readIdentity"],
                index == 4,
                "PHEMEX-PARSER source read identity"
            );
            assert_eq!(
                case["expected"]["emitIdentity"],
                index == 5,
                "PHEMEX-PARSER source emit identity"
            );
        }
        let events = RefCell::new(Vec::<Value>::new());
        let identity = Rc::new(());
        let fail = |value: &Value| {
            let mut error = ExchangeError::new(value["message"].as_str().unwrap());
            error.name = value["name"].as_str().unwrap().into();
            Failure {
                error,
                identity: Some(identity.clone()),
            }
        };
        let result = ready(fetch_tickers(
            &store,
            |url| {
                calls.borrow_mut().0 += 1;
                events.borrow_mut().push(json!(["read", url]));
                let result = case.get("readFailure").map_or_else(
                    || {
                        Ok(case["raw"]
                            .as_str()
                            .map(|raw| serde_json::from_str::<Value>(raw).unwrap()))
                    },
                    |v| Err(fail(v)),
                );
                async move { result }
            },
            |value| {
                calls.borrow_mut().1 += 1;
                events.borrow_mut().push(json!(["emit", capture(value)]));
                case.get("emitFailure").map_or(Ok(()), |v| Err(fail(v)))
            },
        ));
        compare(
            result,
            case,
            [2, 2, 2, -1, -1, -1][index],
            &mut totals,
            if index >= 4 { Some(&identity) } else { None },
        );
        assert_eq!(
            *events.borrow(),
            *case["events"].as_array().unwrap(),
            "PHEMEX-PARSER ordered callbacks"
        );
        cases += 1;
    }
    assert_eq!(
        *calls.borrow(),
        (6, 3),
        "PHEMEX-PARSER callback execution census"
    );
    assert_eq!(
        (totals, cases),
        ((55, 715, 18, 2), 47),
        "PHEMEX-PARSER complete comparison census"
    );
}
