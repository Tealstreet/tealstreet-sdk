use safe_cex_exchange::client_order_id::ClientOrderLookup;
use safe_cex_num::JsCoercionError;
use safe_cex_num::{ProtoMember, TableHit};
use safe_cex_transport::{
    RestAdapter, RestFailure, RestFuture, RestRequest, RestResponse, RestResponseBody,
};
use safe_cex_types::{IncomeCategory, Market, Order};
use safe_cex_venue_okx::{
    income::{map_okx_bill_to_income_records, okx_bill_category},
    rest_operations::find_order_by_client_order_id,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};
const MEMBERS: [(&str, ProtoMember); 12] = [
    ("constructor", ProtoMember::Constructor),
    ("__defineGetter__", ProtoMember::DefineGetter),
    ("__defineSetter__", ProtoMember::DefineSetter),
    ("hasOwnProperty", ProtoMember::HasOwnProperty),
    ("__lookupGetter__", ProtoMember::LookupGetter),
    ("__lookupSetter__", ProtoMember::LookupSetter),
    ("isPrototypeOf", ProtoMember::IsPrototypeOf),
    ("propertyIsEnumerable", ProtoMember::PropertyIsEnumerable),
    ("toString", ProtoMember::ToString),
    ("valueOf", ProtoMember::ValueOf),
    ("__proto__", ProtoMember::Prototype),
    ("toLocaleString", ProtoMember::ToLocaleString),
];
fn oracle() -> Value {
    let out = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/finding-oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "actual source findings oracle: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        out["lookup"].as_array().unwrap().len(),
        36,
        "independent lookup continuation floor"
    );
    assert_eq!(
        out["bills"].as_array().unwrap().len(),
        48,
        "independent income continuation floor"
    );
    out
}
fn ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("fixture must complete inline"),
    }
}
struct Wire {
    whole_body: bool,
    row: Value,
    calls: Cell<usize>,
    trace: Rc<RefCell<Vec<&'static str>>>,
}
impl RestAdapter for Wire {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.calls.set(self.calls.get() + 1);
        self.trace.borrow_mut().push("request");
        assert_eq!(request.method, "get", "LOOKUP-NATIVE-METHOD");
        assert_eq!(request.url, "/api/v5/trade/order");
        assert_eq!(
            request.params,
            Some(json!({"instId":"BTC-USDT-SWAP","clOrdId":"client"})),
            "LOOKUP-NATIVE-PARAMS"
        );
        Box::pin(async move {
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: RestResponseBody::Encoded(
                    serde_json::to_vec(&if self.whole_body {
                        self.row.clone()
                    } else {
                        json!({"code":"0","data":[self.row]})
                    })
                    .unwrap(),
                ),
            })
        })
    }
}
fn order() -> Order {
    serde_json::from_value(json!({"id":"mapped","status":"open","symbol":"BTCUSDT","type":"limit","side":"buy","price":1,"amount":1,"filled":0,"remaining":1,"reduceOnly":false,"lastUpdate":1})).unwrap()
}
fn market() -> Market {
    serde_json::from_value(json!({"id":"BTC-USDT-SWAP","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":1,"price":1},"limits":{"amount":{"min":1,"max":10},"leverage":{"min":1,"max":10}}})).unwrap()
}

#[test]
fn inherited_lookup_preserves_native_state_and_deferred_refusal() {
    let source = oracle();
    let market = market();
    let mut seen = 0;
    for (key, member) in MEMBERS {
        for outcome in ["mapped", "empty", "throw"] {
            let trace = Rc::new(RefCell::new(Vec::new()));
            let wire = Wire {
                whole_body: false,
                row: json!({"state":key,"ordId":"fixture"}),
                calls: Cell::new(0),
                trace: trace.clone(),
            };
            let mappings = Cell::new(0);
            let cause = Box::new(json!({"sentinel":"original mapper metadata"}));
            let identity = (&*cause) as *const Value;
            let cause = RefCell::new(Some(cause));
            let mapper = |row: &Value, state: TableHit<&'static str>| {
                mappings.set(mappings.get() + 1);
                trace.borrow_mut().push("mapper");
                assert_eq!(
                    state,
                    TableHit::Inherited(member),
                    "LOOKUP-NATIVE-STATE {key}"
                );
                assert_eq!(
                    row["ordId"], "fixture",
                    "source spread retains remaining fields"
                );
                if outcome == "throw" {
                    return Err(RestFailure {
                        code: Some("mapper sentinel".into()),
                        transport_error: None,
                        message: "mapper sentinel".into(),
                        response: None,
                        request: Box::default(),
                        context: cause.borrow_mut().take(),
                    });
                }
                Ok(if outcome == "empty" {
                    vec![]
                } else {
                    vec![order()]
                })
            };
            let result = ready(find_order_by_client_order_id(
                &wire,
                &[],
                Some(&market),
                "BTCUSDT",
                "client",
                &mapper,
            ));
            assert_eq!(mappings.get(), 1, "LOOKUP-DEFERRED-MAPPER {key}/{outcome}");
            assert_eq!(wire.calls.get(), 1, "independent source request count");
            assert_eq!(
                *trace.borrow(),
                vec!["request", "mapper"],
                "LOOKUP-NATIVE-EFFECT-ORDER"
            );
            match outcome {
                "mapped" => {
                    let Err(error) = result else {
                        panic!("native lookup must refuse");
                    };
                    assert!(
                        error.message.contains("closed typed output"),
                        "LOOKUP-NATIVE-REFUSAL {key}"
                    );
                }
                "empty" => assert!(
                    matches!(
                        result.unwrap(),
                        ClientOrderLookup::NotFound { definitive: false }
                    ),
                    "empty mapper wins over projection"
                ),
                "throw" => {
                    let Err(error) = result else {
                        panic!("native lookup must refuse");
                    };
                    assert_eq!(error.code.as_deref(), Some("mapper sentinel"));
                    assert_eq!(
                        error.context.as_deref().map(|v| v as *const Value),
                        Some(identity),
                        "original mapper cause identity survives"
                    );
                }
                _ => unreachable!(),
            }
            let expected = &source["lookup"][seen];
            assert_eq!(expected["key"], key);
            assert_eq!(expected["outcome"], outcome);
            seen += 1;
        }
    }
    assert_eq!(seen, 36, "stop toLocaleString/throw");
}

#[test]
fn inherited_income_refuses_only_when_category_reaches_output() {
    let source = oracle();
    let mut seen = 0;
    for (key, _) in MEMBERS {
        for shape in ["category", "fee-only", "empty", "later-coercion"] {
            let Err(error) = okx_bill_category(&json!(key)) else {
                panic!("INCOME-NATIVE-CATEGORY inherited member must refuse");
            };
            assert_eq!(
                error.name, "RepresentationError",
                "INCOME-NATIVE-CATEGORY {key}"
            );
            let calls = Cell::new(0);
            let mut row = json!({"ts":"1","instId":"BTC","billId":"bill","ccy":"USDT","type":key,"pnl":if shape=="category"{"2"}else{"0"},"fee":if shape=="fee-only"{"-0.5"}else{"0"},"balChg":if shape=="category"{"3"}else if shape=="fee-only"{"-0.5"}else{"0"}});
            if shape == "later-coercion" {
                row["ccy"] = json!({"toString":null});
            }
            let result = map_okx_bill_to_income_records(row.as_object().unwrap(), |_| {
                calls.set(calls.get() + 1);
                Ok::<_, JsCoercionError>(Some("BTCUSDT".into()))
            });
            assert_eq!(calls.get(), 1, "INCOME-RESOLVER-COUNT {key}/{shape}");
            match shape {
                "category" => assert_eq!(
                    result.unwrap_err().name,
                    "RepresentationError",
                    "INCOME-DEFERRED-REFUSAL"
                ),
                "fee-only" => {
                    let records = result.unwrap();
                    assert_eq!(records.len(), 1, "INCOME-FEE-ONLY native record count");
                    assert_eq!(records[0].category, Some(IncomeCategory::Commission));
                    assert_eq!(records[0].income_type, "COMMISSION");
                    assert_eq!(records[0].income.to_bits(), (-0.5f64).to_bits());
                }
                "empty" => assert!(
                    result.unwrap().is_empty(),
                    "INCOME-EMPTY category never reaches output"
                ),
                "later-coercion" => {
                    let Err(error) = result else {
                        panic!("native lookup must refuse");
                    };
                    assert_eq!(error.name, "TypeError");
                    assert_eq!(
                        error.message, "Cannot convert object to primitive value",
                        "original coercion failure beats projection"
                    );
                }
                _ => unreachable!(),
            }
            let expected = &source["bills"][seen];
            assert_eq!(expected["key"], key);
            assert_eq!(expected["shape"], shape);
            seen += 1;
        }
    }
    assert_eq!(seen, 48, "stop toLocaleString/later-coercion");
    assert_eq!(
        okx_bill_category(&json!("8")).unwrap(),
        IncomeCategory::Funding,
        "ordinary category control"
    );
    assert_eq!(
        okx_bill_category(&json!("new-code")).unwrap(),
        IncomeCategory::Other,
        "genuinely missing fallback control"
    );
}

#[test]
fn lookup_ordinary_status_fallback_and_json_row_spread() {
    use safe_cex_types::OrderStatus;
    let source = oracle();
    assert_eq!(source["ordinary"].as_array().unwrap().len(), 6);
    assert_eq!(source["spread"].as_array().unwrap().len(), 6);
    let market = market();
    for (i, (state, expected)) in [
        ("live", OrderStatus::Canceled),
        ("partially_filled", OrderStatus::Canceled),
        ("new-state", OrderStatus::Canceled),
        ("filled", OrderStatus::Closed),
        ("canceled", OrderStatus::Canceled),
        ("mmp_canceled", OrderStatus::Canceled),
    ]
    .into_iter()
    .enumerate()
    {
        let trace = Rc::new(RefCell::new(Vec::new()));
        let wire = Wire {
            whole_body: false,
            row: json!({"state":state}),
            calls: Cell::new(0),
            trace,
        };
        let mapper = |row: &Value, override_state: TableHit<&'static str>| {
            let expected_state = if state == "partially_filled" {
                "partially_filled"
            } else {
                "live"
            };
            assert_eq!(override_state, TableHit::Mapped(expected_state));
            assert_eq!(row["state"], expected_state);
            let mut result = order();
            result.status = safe_cex_types::ModelField::Value(OrderStatus::Canceled);
            Ok(vec![result])
        };
        let result = ready(find_order_by_client_order_id(
            &wire,
            &[],
            Some(&market),
            "BTCUSDT",
            "client",
            &mapper,
        ))
        .unwrap();
        let ClientOrderLookup::Found(result) = result else {
            panic!("ordinary found")
        };
        assert_eq!(
            result.status, expected,
            "LOOKUP-NATIVE-STATUS-FALLBACK {state}"
        );
        assert_eq!(source["ordinary"][i]["state"], state);
        assert_eq!(
            source["ordinary"][i]["status"],
            if state == "filled" {
                "closed"
            } else {
                "canceled"
            }
        );
    }
    for (i, (row, expected)) in [
        (json!(true), json!({"state":"live"})),
        (json!(1), json!({"state":"live"})),
        (json!("ab"), json!({"0":"a","1":"b","state":"live"})),
        (json!(["x", 2]), json!({"0":"x","1":2,"state":"live"})),
        (json!({}), json!({"state":"live"})),
    ]
    .into_iter()
    .enumerate()
    {
        let wire = Wire {
            whole_body: false,
            row,
            calls: Cell::new(0),
            trace: Rc::default(),
        };
        let calls = Cell::new(0);
        let mapper = |row: &Value, state: TableHit<&'static str>| {
            calls.set(calls.get() + 1);
            assert_eq!(state, TableHit::Mapped("live"));
            assert_eq!(row, &expected, "LOOKUP-NATIVE-ROW-SPREAD");
            Ok(vec![])
        };
        assert!(matches!(
            ready(find_order_by_client_order_id(
                &wire,
                &[],
                Some(&market),
                "BTCUSDT",
                "client",
                &mapper
            ))
            .unwrap(),
            ClientOrderLookup::NotFound { definitive: false }
        ));
        assert_eq!(calls.get(), 1);
        assert_eq!(wire.calls.get(), 1);
        assert_eq!(source["spread"][i]["mapped"], expected);
    }
    let wire = Wire {
        whole_body: false,
        row: json!("😀"),
        calls: Cell::new(0),
        trace: Rc::default(),
    };
    let calls = Cell::new(0);
    let mapper = |_: &Value, _: TableHit<&'static str>| {
        calls.set(calls.get() + 1);
        Ok(vec![])
    };
    let Err(error) = ready(find_order_by_client_order_id(
        &wire,
        &[],
        Some(&market),
        "BTCUSDT",
        "client",
        &mapper,
    )) else {
        panic!("materialized surrogate fields cannot cross Value string surface");
    };
    assert_eq!(error.code, None);
    assert_eq!(
        error.context.as_deref().unwrap()["name"],
        "RepresentationError"
    );
    assert!(error.message.contains("lone UTF-16 unit"));
    assert_eq!(wire.calls.get(), 1);
    assert_eq!(
        calls.get(),
        0,
        "declared whole-lookup refusal before mapper"
    );
}

#[test]
fn lookup_data_indexing_and_null_destructuring_follow_source() {
    let source = oracle();
    assert_eq!(
        source["envelopes"].as_array().unwrap().len(),
        5,
        "independent envelope floor"
    );
    let market = market();
    for (i, (body, expected_row, expected_error)) in [
        (
            json!({"data":{"0":{"state":"filled"}}}),
            Some(json!({"state":"live"})),
            None,
        ),
        (
            json!({"data":"ab"}),
            Some(json!({"0":"a","state":"live"})),
            None,
        ),
        (json!({"data":""}), None, None),
        (Value::Null, None, Some("TypeError")),
        (json!({"data":"😀"}), None, Some("RepresentationError")),
    ]
    .into_iter()
    .enumerate()
    {
        let wire = Wire {
            whole_body: true,
            row: body,
            calls: Cell::new(0),
            trace: Rc::default(),
        };
        let mappings = Cell::new(0);
        let mapper = |row: &Value, state: TableHit<&'static str>| {
            mappings.set(mappings.get() + 1);
            assert_eq!(state, TableHit::Mapped("live"));
            assert_eq!(Some(row), expected_row.as_ref(), "LOOKUP-NATIVE-DATA-INDEX");
            Ok(vec![])
        };
        let result = ready(find_order_by_client_order_id(
            &wire,
            &[],
            Some(&market),
            "BTCUSDT",
            "client",
            &mapper,
        ));
        assert_eq!(wire.calls.get(), 1);
        assert_eq!(
            mappings.get(),
            usize::from(expected_row.is_some()),
            "native envelope mapper count"
        );
        assert_eq!(source["envelopes"][i]["mapped"], i != 2 && i != 3);
        assert_eq!(
            source["envelopes"][i]["error"],
            if i == 3 {
                json!("TypeError")
            } else {
                Value::Null
            }
        );
        if i != 4 {
            assert_eq!(
                source["envelopes"][i]["row"],
                expected_row.clone().unwrap_or(Value::Null)
            );
        }
        if let Some(expected_error) = expected_error {
            let Err(error) = result else {
                panic!("source throw or priced materialized UTF16 refusal");
            };
            assert_eq!(error.code, None);
            assert_eq!(error.context.as_deref().unwrap()["name"], expected_error);
        } else {
            assert!(matches!(
                result.unwrap(),
                ClientOrderLookup::NotFound { definitive: false }
            ));
        }
    }
}

#[test]
fn lookup_store_hit_prices_owned_snapshot_in_both_directions() {
    let reference = oracle();
    assert_eq!(reference["alias"]["sameReference"], true);
    let mut stored = order();
    stored.info = (Some(json!({"clOrdId":"client"}))).into();
    let mut orders = vec![stored];
    let wire = Wire {
        whole_body: false,
        row: Value::Null,
        calls: Cell::new(0),
        trace: Rc::default(),
    };
    let mapper = |_: &Value, _: TableHit<&'static str>| -> Result<Vec<Order>, RestFailure> {
        panic!("store hit must not map")
    };
    let found = ready(find_order_by_client_order_id(
        &wire, &orders, None, "BTCUSDT", "client", &mapper,
    ))
    .unwrap();
    let ClientOrderLookup::Found(mut found) = found else {
        panic!("store hit found")
    };
    assert_eq!(wire.calls.get(), 0, "store hit must not dispatch");
    orders[0].status = (safe_cex_types::OrderStatus::Closed).into();
    assert_eq!(
        found.status,
        safe_cex_types::OrderStatus::Open,
        "LOOKUP-NATIVE-SNAPSHOT caller mutation leaves result frozen"
    );
    found.amount = 17.0;
    assert_eq!(
        orders[0].amount.to_bits(),
        1.0f64.to_bits(),
        "LOOKUP-NATIVE-SNAPSHOT result mutation leaves caller frozen"
    );
    assert_eq!(reference["alias"]["afterInput"], "closed");
    assert_eq!(reference["alias"]["afterOutput"], 17);
}
