#[path = "support/cardinality.rs"]
mod cardinality;
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{
    JsJsonValue,
    numeric_ingress::{JsNumberInput, js_from_value, js_value_as_f64},
};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_types::{
    IncomeCategory, IncomeRecord, MarginType, Market, Position, PositionMode, PositionSide,
};
use safe_cex_venue_kraken::reads;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    error::Error,
    future::{Future, poll_fn, ready},
    process::Command,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/reads-cardinality-oracle.cjs"
            ))
            .env("TZ", "UTC")
            .output()
            .expect("actual TS read oracle");
        assert!(
            output.status.success(),
            "actual TS read oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        eprintln!("REST read capture header: {}", value["header"]);
        assert!(
            value["header"]["nodeMajor"].as_u64().unwrap() > 0,
            "Node major capture"
        );
        assert!(
            !value["header"]["arch"].as_str().unwrap().is_empty(),
            "CPU architecture capture"
        );
        cardinality::source_census("reads", &value);
        value
    })
}
fn complete<F: Future>(future: F) -> F::Output {
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    match std::pin::pin!(future).as_mut().poll(&mut context) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("test read was already released"),
    }
}
fn number(v: f64) -> Value {
    json!({"number":format!("{:016x}",if v.is_nan(){0x7ff8000000000000}else{v.to_bits()})})
}
fn opt_string(v: &Option<String>) -> Value {
    v.as_ref()
        .map_or_else(|| json!({"undefined":true}), |v| json!(v))
}
#[path = "../../parity/evidence/retained_model_values.rs"]
pub mod retained_model_values;
fn capture(value: &(impl serde::Serialize + ?Sized)) -> Value {
    retained_model_values::observe(value, number)
}

fn request(params: JsJsonValue) -> Value {
    let JsJsonValue::Object(entries) = params else {
        panic!("query native object")
    };
    Value::Object(
        entries
            .into_iter()
            .map(|(k, v)| {
                let JsJsonValue::Number(n) = v else {
                    panic!("query native number")
                };
                (k, number(n))
            })
            .collect(),
    )
}
fn response(case: &Value) -> Result<Option<JsNumberInput>, Box<dyn Error>> {
    if case["fail"] == true {
        let mut error = ExchangeError::new("read failure");
        error.metadata.code = Some(safe_cex_exchange::js_runtime::value::Js::str("EPRIVATE"));
        error.metadata.store_subscriber_ids = vec![13, 29];
        error.metadata.response = Some(json!({"status":429}));
        return Err(Box::new(error));
    }
    Ok(case["raw"]
        .as_str()
        .map(|raw| serde_json::from_str(raw).unwrap()))
}
fn error(error: Box<dyn Error>, identity: Option<*const ExchangeError>) -> Value {
    let native = error
        .downcast::<ExchangeError>()
        .expect("native exchange failure");
    let same = identity.is_some_and(|pointer| std::ptr::eq(pointer, native.as_ref()));
    if same {
        assert_eq!(
            native.metadata.store_subscriber_ids,
            [13, 29],
            "read failure inner-to-outer subscriber metadata retained"
        );
        assert!(
            native
                .metadata
                .code
                .as_ref()
                .unwrap()
                .strict_equals(&safe_cex_exchange::js_runtime::value::Js::str("EPRIVATE")),
            "read failure metadata retained"
        );
        assert_eq!(
            native.metadata.response,
            Some(json!({"status":429})),
            "read failure response retained"
        );
    }
    json!({"error":native.name,"message":native.message,"identity":same})
}
fn failure_pointer(
    result: &Result<Option<JsNumberInput>, Box<dyn Error>>,
) -> Option<*const ExchangeError> {
    result
        .as_ref()
        .err()
        .map(|error| error.downcast_ref::<ExchangeError>().unwrap() as *const _)
}
fn install(store: &DefaultStore, values: &Value) {
    let markets: Vec<Market> = js_from_value(values.clone()).unwrap();
    Store::update(
        store,
        StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .unwrap();
}
fn record(v: &IncomeRecord) -> Value {
    let category = match v.category.as_ref().expect("income category present") {
        IncomeCategory::RealizedPnl => "realized_pnl",
        IncomeCategory::Funding => "funding",
        IncomeCategory::Commission => "commission",
        IncomeCategory::Liquidation => "liquidation",
        IncomeCategory::Transfer => "transfer",
        IncomeCategory::Other => "other",
        IncomeCategory::Rebate => "rebate",
    };
    json!({"symbol":v.symbol,"incomeType":v.income_type,"asset":v.asset,"timestamp":number(v.timestamp),
        "tradeId":opt_string(&v.trade_id),"tranId":v.tran_id,"income":number(v.income),"category":category})
}

fn position(v: &Position) -> Value {
    assert!(
        v.realized_pnl.is_none()
            && v.margin_size.is_none()
            && v.extra.is_none()
            && v.last_update.is_none(),
        "Spot absent native position fields"
    );
    json!({"id":v.id,"symbol":v.symbol,"side":match v.side.value().expect("present source enum in fixture"){PositionSide::Long=>"long",PositionSide::Short=>"short"},
 "entryPrice":number(v.entry_price),"notional":number(v.notional),"leverage":number(v.leverage),"unrealizedPnl":number(v.unrealized_pnl),"contracts":number(v.contracts),"liquidationPrice":number(v.liquidation_price),
 "marginType":match v.margin_type.unwrap(){MarginType::Isolated=>"isolated",MarginType::Cross=>"cross"},"positionMode":match v.position_mode.unwrap(){PositionMode::OneWay=>"oneway",PositionMode::Hedged=>"hedged"},"info":v.info})
}
#[test]
fn account_log_live_oracle() {
    for case in cardinality::cases("reads", "accounts", oracle()) {
        let store = DefaultStore::new();
        let calls = RefCell::new(Vec::new());
        let mut utc = |ms| ms;
        let received = response(case);
        let pointer = failure_pointer(&received);
        let released = Cell::new(false);
        let mut received = Some(received);
        let future = reads::fetch_account_log_page(
            serde_json::from_str(case["paramsRaw"].as_str().unwrap()).unwrap(),
            &store,
            |endpoint, params| {
                calls
                    .borrow_mut()
                    .push(json!({"endpoint":endpoint,"options":{"params":request(params)}}));
                poll_fn(|_| {
                    if released.get() {
                        Poll::Ready(received.take().unwrap())
                    } else {
                        Poll::Pending
                    }
                })
            },
            &mut utc,
        );
        assert_eq!(
            json!(calls.borrow().len()),
            case["prefixCalls"],
            "account-log synchronous read prefix"
        );
        let mut future = Box::pin(future);
        let waker = Waker::noop();
        assert!(
            future
                .as_mut()
                .poll(&mut Context::from_waker(waker))
                .is_pending(),
            "account-log read suspension"
        );
        install(&store, &oracle()["markets"]);
        released.set(true);
        let actual = complete(future)
            .map(|values| {
                cardinality::native_len(
                    values.len(),
                    &case["result"],
                    "account-log native records and error",
                );
                Value::Array(values.iter().map(record).collect())
            })
            .unwrap_or_else(|e| error(e, pointer));
        assert_eq!(
            actual, case["result"],
            "account-log native records and error raw={}",
            case["raw"]
        );
        assert_eq!(
            json!(*calls.borrow()),
            case["calls"],
            "account-log native since/count query params={}",
            case["paramsRaw"]
        );
    }
}
#[test]
fn token_live_oracle() {
    for case in cardinality::cases("reads", "tokens", oracle()).chain(cardinality::cases(
        "reads",
        "tokenSupplement",
        oracle(),
    )) {
        let calls = RefCell::new(Vec::new());
        let received = response(case);
        let pointer = failure_pointer(&received);
        let future = reads::fetch_ws_token(|endpoint, body| {
            calls
                .borrow_mut()
                .push(json!({"endpoint":endpoint,"body":capture(&body)}));
            ready(received)
        });
        assert_eq!(
            json!(calls.borrow().len()),
            case["prefixCalls"],
            "token synchronous read prefix"
        );
        let actual = complete(future)
            .map(|v| {
                if let Value::Array(rows) = &v {
                    cardinality::native_len(
                        rows.len(),
                        &case["result"],
                        "token native value/error",
                    );
                }
                capture(&v)
            })
            .unwrap_or_else(|e| error(e, pointer));
        assert_eq!(
            actual, case["result"],
            "token native value/error raw={}",
            case["raw"]
        );
        assert_eq!(
            json!(*calls.borrow()),
            case["calls"],
            "token native POST endpoint/body"
        );
    }
}
#[test]
fn spot_positions_live_oracle() {
    for case in cardinality::cases("reads", "positions", oracle()) {
        let store = DefaultStore::new();
        install(&store, &case["markets"]);
        let positions = reads::fetch_spot_positions(&store);
        cardinality::native_len(
            positions.len(),
            &case["result"],
            "Spot positions native fields and order",
        );
        install(&store, &json!([]));
        assert_eq!(
            Value::Array(positions.iter().map(position).collect()),
            case["result"],
            "Spot positions native fields and order"
        );
    }
}
#[test]
fn read_chosen_boundaries() {
    let boundary = &oracle()["boundaries"];
    assert_eq!(
        boundary["params"]["calls"][0]["options"]["params"],
        json!({"since":"123","count":"17"}),
        "source untyped query arguments accepted"
    );
    let store = DefaultStore::new();
    let mut utc = |ms| ms;
    let future = reads::fetch_account_log_page(
        serde_json::from_str(boundary["params"]["paramsRaw"].as_str().unwrap()).unwrap(),
        &store,
        |_, _| {
            panic!("typed params rejected before read");
            #[allow(unreachable_code)]
            ready(Ok(None))
        },
        &mut utc,
    );
    assert!(
        complete(future).is_err(),
        "CHOSEN numeric query input refusal"
    );
    assert_eq!(
        boundary["surrogate"]["units"],
        json!([55296]),
        "source token surrogate acceptance"
    );
    assert!(
        serde_json::from_str::<JsNumberInput>(boundary["surrogate"]["raw"].as_str().unwrap())
            .is_err(),
        "CHOSEN token UTF8 refusal"
    );
    assert_eq!(
        boundary["depth"]["result"],
        json!([]),
        "source ignored deep response acceptance"
    );
    assert!(
        serde_json::from_str::<JsNumberInput>(boundary["depth"]["raw"].as_str().unwrap()).is_err(),
        "CHOSEN shared depth refusal"
    );
}

#[test]
fn consuming_account_decode_cardinality() {
    let case = &oracle()["accounts"][15];
    let store = DefaultStore::new();
    install(&store, &oracle()["markets"]);
    let params: JsNumberInput = serde_json::from_str(case["paramsRaw"].as_str().unwrap()).unwrap();
    let received = response(case);
    let mut utc = |ms| ms;
    let values = complete(reads::fetch_account_log_page(
        params,
        &store,
        |_, _| ready(received),
        &mut utc,
    ))
    .unwrap();
    assert_eq!(
        values.len(),
        8,
        "KRAKEN-CONSUMING account independent native length"
    );
    cardinality::native_len(
        values.len(),
        &case["result"],
        "KRAKEN-CONSUMING account source/native length",
    );
    let mut comparisons = 0;
    for index in 0..8 {
        let value = values
            .get(index)
            .expect("KRAKEN-CONSUMING account native index coverage");
        assert_eq!(
            record(value),
            case["result"][index],
            "KRAKEN-CONSUMING account native record"
        );
        comparisons += 1;
    }
    assert_eq!(comparisons, 8, "KRAKEN-CONSUMING account comparison count");
}

#[test]
fn consuming_token_decode_cardinality() {
    let case = &oracle()["tokenSupplement"][0];
    let received = response(case);
    let value = complete(reads::fetch_ws_token(|_, _| ready(received))).unwrap();
    let values = value
        .as_array()
        .expect("KRAKEN-CONSUMING token native array");
    assert_eq!(
        values.len(),
        4,
        "KRAKEN-CONSUMING token independent native length"
    );
    cardinality::native_len(
        values.len(),
        &case["result"],
        "KRAKEN-CONSUMING token source/native length",
    );
    let nested = values
        .get(2)
        .and_then(Value::as_array)
        .expect("KRAKEN-CONSUMING token native index coverage");
    assert_eq!(
        nested.len(),
        2,
        "KRAKEN-CONSUMING token nested native length"
    );
    let mut comparisons = 0;
    for index in 0..4 {
        let value = values
            .get(index)
            .expect("KRAKEN-CONSUMING token native index coverage");
        assert_eq!(
            capture(value),
            case["result"][index],
            "KRAKEN-CONSUMING token native value"
        );
        comparisons += 1;
    }
    assert_eq!(comparisons, 4, "KRAKEN-CONSUMING token comparison count");
}

#[test]
fn leverage_settings_live_oracle() {
    let mut comparisons = 0;
    for case in cardinality::cases("reads", "leverages", oracle()) {
        let calls = RefCell::new(Vec::new());
        let effects = RefCell::new(Vec::new());
        let reports = RefCell::new(Vec::new());
        let cache = RefCell::new(vec![(json!("OLD"), json!(3.125))]);
        let released = Cell::new(false);
        let mut console_failure = ExchangeError::new("console failed");
        console_failure.metadata.store_subscriber_ids = vec![43, 61];
        let metadata_identity = console_failure.metadata.as_ref() as *const _;
        let future = reads::fetch_leverage_settings(
            |endpoint| {
                calls.borrow_mut().push(endpoint);
                if case["failure"] == "sync" {
                    return Err(ExchangeError::new(case["message"].as_str().unwrap()));
                }
                Ok(poll_fn(|_| {
                    if !released.get() {
                        Poll::Pending
                    } else if case["failure"] == "async" {
                        Poll::Ready(Err(ExchangeError::new(case["message"].as_str().unwrap())))
                    } else {
                        Poll::Ready(Ok(case["raw"]
                            .as_str()
                            .map(|raw| serde_json::from_str(raw).unwrap())))
                    }
                }))
            },
            || {
                effects.borrow_mut().push(json!(["clear"]));
                if case["clearFailure"] == true {
                    return Err(ExchangeError::new("clear failed"));
                }
                cache.borrow_mut().clear();
                Ok(())
            },
            |symbol, leverage| {
                // Capture native callback arguments before cache insertion or Serialize.
                effects
                    .borrow_mut()
                    .push(json!(["set", capture(&symbol), capture(&leverage)]));
                if case["setFailureAt"].as_u64() == Some(effects.borrow().len() as u64) {
                    return Err(ExchangeError::new("set failed"));
                }
                // Distinct JSON object/array inputs are distinct Map keys.
                // Only primitive keys compare by value.
                let mut cache = cache.borrow_mut();
                if let Some((_, value)) = cache
                    .iter_mut()
                    .find(|(key, _)| !symbol.is_object() && !symbol.is_array() && *key == symbol)
                {
                    *value = leverage;
                } else {
                    cache.push((symbol, leverage));
                }
                Ok(())
            },
            |prefix, message| {
                reports.borrow_mut().push(json!([prefix, message]));
                if case["consoleFailure"] == true {
                    Err(console_failure)
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(
            json!({"calls":calls.borrow().len(),"effects":effects.borrow().len(),"reports":reports.borrow().len()}),
            case["prefix"],
            "leverage synchronous prefix"
        );
        let mut future = Box::pin(future);
        if case["failure"] != "sync" {
            assert!(
                future
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending(),
                "leverage pending read"
            );
            assert!(
                effects.borrow().is_empty(),
                "leverage cache unchanged while suspended"
            );
        }
        released.set(true);
        let result = match complete(future) {
            Ok(()) => json!({"undefined":true}),
            Err(error) => {
                assert_eq!(
                    error.metadata.store_subscriber_ids,
                    [43, 61],
                    "leverage console failure subscriber metadata"
                );
                let identity = std::ptr::eq(error.metadata.as_ref(), metadata_identity);
                assert!(identity, "leverage console failure metadata allocation");
                json!({"error":error.name,"message":error.message,"consoleIdentity":identity})
            }
        };
        assert_eq!(result, case["result"], "leverage native completion/error");
        assert_eq!(
            json!(*calls.borrow()),
            case["calls"],
            "leverage read endpoint"
        );
        assert_eq!(
            json!(*effects.borrow()),
            case["effects"],
            "leverage native cache mutation trace raw={}",
            case["raw"]
        );
        assert_eq!(
            json!(*reports.borrow()),
            case["reports"],
            "leverage console reports raw={}",
            case["raw"]
        );
        cardinality::native_len(
            effects.borrow().len(),
            &case["effects"],
            "leverage native cache mutation trace",
        );
        cardinality::native_len(
            cache.borrow().len(),
            &case["cache"],
            "leverage ordered partial cache state",
        );
        let native_cache: Vec<_> = cache
            .borrow()
            .iter()
            .map(|(key, value)| json!([capture(key), capture(value)]))
            .collect();
        assert_eq!(
            json!(native_cache),
            case["cache"],
            "leverage ordered partial cache state"
        );
        comparisons += 1;
    }
    assert_eq!(
        comparisons, 33,
        "KRAKEN-LEVERAGE native case comparison count"
    );
}

#[test]
fn leverage_chosen_boundary() {
    let case = &oracle()["leverageBoundary"];
    assert_eq!(
        case["symbolUnits"],
        json!([55296]),
        "source leverage surrogate symbol accepted"
    );
    assert!(
        serde_json::from_str::<JsNumberInput>(case["raw"].as_str().unwrap()).is_err(),
        "CHOSEN leverage UTF8 boundary refusal"
    );
}

#[path = "support/history.rs"]
mod history;

#[path = "support/ohlcv.rs"]
mod ohlcv;

#[path = "support/tickers.rs"]
mod ticker_reads;

#[path = "support/futures_candles.rs"]
mod futures_candle_reads;
