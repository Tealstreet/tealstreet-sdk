use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::numeric_ingress::js_from_str;
use safe_cex_types::{OrderSide, OrderTimeInForce, OrderType, PlaceOrderOpts};
use safe_cex_venue_backpack::{batch_orders::*, types::*};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn oracle() -> Value {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/batch_orders_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn bits(n: f64) -> String {
    format!(
        "{:016x}",
        if n.is_nan() {
            0x7ff8000000000000
        } else {
            n.to_bits()
        }
    )
}
const LABELS: [&str; 39] = [
    "base",
    "sell",
    "post only",
    "IOC",
    "FOK",
    "reduce false",
    "reduce true",
    "ordinary market",
    "trigger defined",
    "client zero",
    "client fractional",
    "client max",
    "client overflow",
    "client nan",
    "client infinity",
    "missing market",
    "inactive",
    "read only",
    "lookup throws",
    "amount zero",
    "amount nan",
    "amount infinity",
    "below min",
    "above max",
    "missing price",
    "price zero",
    "price nan",
    "price infinity",
    "price precision zero",
    "amount precision nan",
    "tiny amount rounded zero",
    "large amount overflow",
    "scientific precision",
    "half rounding",
    "whole precision",
    "exponent dot precision",
    "negative zero client",
    "both precision invalid",
    "bound equality",
];
fn builder_case(label: &str) -> (PlaceOrderOpts, f64, BatchMarket) {
    let mut opts = js_from_str::<PlaceOrderOpts>(
        r#"{"symbol":"BTC λ","type":"limit","side":"buy","amount":3.125,"price":101.25}"#,
    )
    .unwrap();
    let mut id = 17.0;
    let mut market = BatchMarket {
        id: "VENUE 17".into(),
        active: true,
        read_only: false,
        amount_min: 0.125,
        amount_max: 100.0,
        price_precision: 0.5,
        amount_precision: 0.125,
    };
    match label {
        "sell" => opts.side = OrderSide::Sell,
        "post only" => opts.time_in_force = Some(OrderTimeInForce::PostOnly),
        "IOC" => opts.time_in_force = Some(OrderTimeInForce::ImmediateOrCancel),
        "FOK" => opts.time_in_force = Some(OrderTimeInForce::FillOrKill),
        "reduce false" => opts.reduce_only = Some(false),
        "reduce true" => opts.reduce_only = Some(true),
        "ordinary market" => opts.r#type = OrderType::Market,
        "trigger defined" => opts.trigger_price = Some(0.0),
        "client zero" => id = 0.0,
        "client fractional" => id = 1.5,
        "client max" => id = 4294967295.0,
        "client overflow" => id = 4294967296.0,
        "client nan" => id = f64::NAN,
        "client infinity" => id = f64::INFINITY,
        "negative zero client" => id = -0.0,
        "inactive" => market.active = false,
        "read only" => market.read_only = true,
        "amount zero" => opts.amount = 0.0,
        "amount nan" => opts.amount = f64::NAN,
        "amount infinity" => opts.amount = f64::INFINITY,
        "below min" => opts.amount = 0.1,
        "above max" => opts.amount = 101.0,
        "missing price" => opts.price = None,
        "price zero" => opts.price = Some(0.0),
        "price nan" => opts.price = Some(f64::NAN),
        "price infinity" => opts.price = Some(f64::INFINITY),
        "price precision zero" => market.price_precision = 0.0,
        "amount precision nan" => market.amount_precision = f64::NAN,
        "tiny amount rounded zero" => {
            opts.amount = 0.001;
            market.amount_min = 0.0;
        }
        "large amount overflow" => {
            opts.amount = 1e308;
            market.amount_max = f64::INFINITY;
            market.amount_precision = 0.1;
        }
        "scientific precision" => market.price_precision = 1e-7,
        "half rounding" => {
            opts.price = Some(1.005);
            market.price_precision = 0.01;
        }
        "whole precision" => {
            market.price_precision = 2.0;
            market.amount_precision = 1.0;
        }
        "exponent dot precision" => market.price_precision = 1.25e-7,
        "both precision invalid" => {
            market.price_precision = 0.0;
            market.amount_precision = f64::NAN;
        }
        "bound equality" => opts.amount = 0.125,
        "base" | "missing market" | "lookup throws" => {}
        _ => panic!("independent builder case {label}"),
    }
    (opts, id, market)
}
#[test]
fn actual_typescript_batch_request() {
    let c = oracle();
    let rows = c["builders"].as_array().unwrap();
    assert_eq!(rows.len(), 39, "BATCH-REQUEST independent cases");
    let mut completed = [0; 2];
    for (i, row) in rows.iter().enumerate() {
        let label = LABELS[i];
        let (opts, id, market) = builder_case(label);
        assert_eq!(row["index"], i);
        assert_eq!(row["label"], label);
        let tif = opts.time_in_force.map(|t| match t {
            OrderTimeInForce::PostOnly => "PostOnly",
            OrderTimeInForce::ImmediateOrCancel => "ImmediateOrCancel",
            OrderTimeInForce::FillOrKill => "FillOrKill",
            OrderTimeInForce::GoodTillCancel => "GoodTillCancel",
        });
        assert_eq!(
            row["input"],
            json!({"symbol":opts.symbol,"type":if opts.r#type==OrderType::Limit {"limit"} else {"market"},"side":if opts.side==OrderSide::Buy {"buy"} else {"sell"},"amount":bits(opts.amount),"price":opts.price.map(bits),"trigger":opts.trigger_price.map(bits),"tif":tif,"reduce":opts.reduce_only,"client":bits(id),"market":{"id":market.id,"active":market.active,"readOnly":market.read_only,"min":bits(market.amount_min),"max":bits(market.amount_max),"pp":bits(market.price_precision),"ap":bits(market.amount_precision)}}),
            "BATCH-REQUEST independent complete input {i}"
        );
        let calls = RefCell::new(Vec::new());
        let mut lookup_error = ExchangeError::new("lookup failed");
        lookup_error.name = "LookupError".into();
        lookup_error.store_subscriber_ids = vec![7, 2];
        let result = build_batch_limit_order_request(&opts, id, |symbol| {
            assert_eq!(symbol, "BTC λ");
            calls.borrow_mut().push(json!(["market", symbol]));
            if label == "lookup throws" {
                Err(lookup_error.clone())
            } else {
                Ok(if label == "missing market" {
                    None
                } else {
                    Some(market)
                })
            }
        });
        let early = [7, 8, 9, 10, 12, 13, 14, 36].contains(&i);
        assert_eq!(
            calls.borrow().len(),
            if early { 0 } else { 1 },
            "BATCH-REQUEST independent lookup count {i}"
        );
        let succeeds = [0, 1, 2, 3, 4, 5, 6, 11, 32, 33, 34, 35, 38].contains(&i);
        if succeeds {
            let request = result.unwrap();
            let expected = &row["outcome"]["request"];
            assert_eq!(
                request.symbol, "VENUE 17",
                "BATCH-REQUEST native symbol {i}"
            );
            assert_eq!(
                request.side,
                if label == "sell" {
                    BackpackOrderRequestSide::Ask
                } else {
                    BackpackOrderRequestSide::Bid
                }
            );
            assert_eq!(request.orderType, BackpackOrderRequestOrderType::Limit);
            assert_eq!(
                request.price.as_deref(),
                expected["price"].as_str(),
                "BATCH-REQUEST native price {i}"
            );
            assert_eq!(
                request.quantity.as_deref(),
                expected["quantity"].as_str(),
                "BATCH-REQUEST native quantity {i}"
            );
            assert_eq!(request.clientId, Some(id));
            assert_eq!(bits(request.clientId.unwrap()), expected["clientId"]);
            assert_eq!(request.postOnly, Some(label == "post only"));
            assert_eq!(
                request.reduceOnly,
                if label == "reduce true" {
                    Some(true)
                } else {
                    None
                }
            );
            let expected_tif = match expected["timeInForce"].as_str().unwrap() {
                "GTC" => BackpackOrderRequestTimeInForce::Gtc,
                "IOC" => BackpackOrderRequestTimeInForce::Ioc,
                "FOK" => BackpackOrderRequestTimeInForce::Fok,
                _ => panic!(),
            };
            assert_eq!(request.timeInForce, Some(expected_tif));
            assert!(
                request.triggerPrice.is_none()
                    && request.triggerQuantity.is_none()
                    && request.quoteQuantity.is_none()
                    && request.selfTradePrevention.is_none()
                    && request.triggerBy.is_none()
                    && request.takeProfitTriggerPrice.is_none()
                    && request.takeProfitLimitPrice.is_none()
                    && request.takeProfitTriggerBy.is_none()
                    && request.stopLossTriggerPrice.is_none()
                    && request.stopLossLimitPrice.is_none()
                    && request.stopLossTriggerBy.is_none()
            );
            assert_eq!(row["outcome"]["ok"], true);
            assert_eq!(expected["symbol"], "VENUE 17");
            assert_eq!(
                expected["side"],
                if label == "sell" { "Ask" } else { "Bid" }
            );
            assert_eq!(expected["orderType"], "Limit");
            assert_eq!(expected["postOnly"], label == "post only");
            assert_eq!(expected["reduceOnly"].as_bool(), request.reduceOnly);
            let mut keys = vec![
                "symbol",
                "side",
                "orderType",
                "price",
                "quantity",
                "timeInForce",
                "postOnly",
                "clientId",
            ];
            if label == "reduce true" {
                keys.push("reduceOnly");
            }
            assert_eq!(row["outcome"]["keys"], json!(keys));
            completed[0] += 1;
        } else {
            let error = result.unwrap_err();
            assert_eq!(
                error.name, row["outcome"]["name"],
                "BATCH-REQUEST native error name {i}"
            );
            assert_eq!(
                error.message, row["outcome"]["message"],
                "BATCH-REQUEST native error message {i}"
            );
            if label == "lookup throws" {
                assert_eq!(error, lookup_error);
                assert_eq!(row["outcome"]["sameLookup"], true);
            }
            assert_eq!(row["outcome"]["ok"], false);
            completed[1] += 1;
        }
        assert_eq!(calls.borrow().as_slice(), row["trace"].as_array().unwrap());
    }
    assert_eq!(
        completed,
        [13, 26],
        "BATCH-REQUEST independent completed native assertions"
    );
}
const MODES: [&str; 19] = [
    "empty",
    "normal",
    "all single",
    "pair sync",
    "pair async",
    "single sync",
    "single async",
    "report throws",
    "log throws",
    "format throws",
    "assert throws",
    "length throws",
    "read throws",
    "eligibility throws",
    "sparse",
    "append",
    "shrink",
    "eligibility mutation",
    "single append",
];
struct Item {
    id: &'static str,
    eligible: bool,
}
type Items = Rc<RefCell<Vec<Option<Rc<Item>>>>>;
type Failure = Rc<&'static str>;
type Response<'a> = Pin<Box<dyn Future<Output = Result<Vec<Vec<u16>>, Failure>> + 'a>>;
fn id_units(prefix: &str, id: &str) -> Vec<u16> {
    let mut result = format!("{prefix} {id}").encode_utf16().collect::<Vec<_>>();
    if prefix == "s" && id == "C" {
        result.push(0xd800);
    }
    result
}
fn make_items(mode: &str) -> Items {
    Rc::new(RefCell::new(if mode == "empty" {
        vec![]
    } else {
        ["A", "B", "C"]
            .into_iter()
            .map(|id| {
                if mode == "sparse" && id == "B" {
                    None
                } else {
                    Some(Rc::new(Item {
                        id,
                        eligible: id != "C" && !["all single", "single append"].contains(&mode),
                    }))
                }
            })
            .collect()
    }))
}
fn expected_ids(mode: &str) -> Vec<Vec<u16>> {
    match mode {
        "empty" => vec![],
        "all single" => vec![id_units("s", "A"), id_units("s", "B"), id_units("s", "C")],
        "pair sync" | "pair async" => vec![id_units("s", "C")],
        "single sync" | "single async" | "shrink" => vec![id_units("p", "A"), id_units("p", "B")],
        "sparse" => vec![
            id_units("s", "A"),
            id_units("s", "undefined"),
            id_units("s", "C"),
        ],
        "append" => vec![
            id_units("p", "A"),
            id_units("p", "B"),
            id_units("s", "C"),
            id_units("s", "D"),
        ],
        "single append" => vec![
            id_units("s", "A"),
            id_units("s", "B"),
            id_units("s", "C"),
            id_units("s", "D"),
        ],
        _ => vec![id_units("p", "A"), id_units("p", "B"), id_units("s", "C")],
    }
}
fn run_placement(row: &Value, mode: &str, deferred: bool) {
    let items = make_items(mode);
    let original_items = items.borrow().clone();
    let trace = RefCell::new(Vec::<Value>::new());
    let original = Rc::new("original");
    let report_error = Rc::new("report");
    let log_error = Rc::new("log");
    let format_error = Rc::new("format");
    let released = Cell::new(!deferred);
    let mut action = place_orders(
        || {
            trace.borrow_mut().push(json!(["assert"]));
            if mode == "assert throws" {
                Err(original.clone())
            } else {
                Ok(())
            }
        },
        || {
            trace
                .borrow_mut()
                .push(json!(["length", items.borrow().len()]));
            if mode == "length throws" {
                Err(original.clone())
            } else {
                Ok(items.borrow().len())
            }
        },
        |index| {
            trace.borrow_mut().push(json!(["read", index]));
            if mode == "read throws" && index == 1 {
                Err(original.clone())
            } else {
                Ok(items.borrow().get(index).cloned().flatten())
            }
        },
        |item: &Option<Rc<Item>>| {
            let id = item.as_ref().map_or("undefined", |i| i.id);
            trace.borrow_mut().push(json!(["eligible", id]));
            if mode == "eligibility throws" {
                return Err(original.clone());
            }
            if mode == "eligibility mutation" && id == "A" {
                items.borrow_mut()[1] = Some(Rc::new(Item {
                    id: "X",
                    eligible: false,
                }));
            }
            Ok(item.as_ref().is_some_and(|i| i.eligible))
        },
        |pair: [Option<Rc<Item>>; 2]| -> Result<Response<'_>, Failure> {
            for (index, item) in pair.iter().enumerate() {
                assert!(
                    Rc::ptr_eq(
                        item.as_ref().unwrap(),
                        original_items[index].as_ref().unwrap()
                    ),
                    "PLACE-MANY native pair item identity {mode}/{index}"
                );
            }
            let ids = pair
                .iter()
                .map(|i| i.as_ref().map_or("undefined", |i| i.id))
                .collect::<Vec<_>>();
            trace.borrow_mut().push(json!(["pair", ids]));
            if mode == "append" {
                items.borrow_mut().push(Some(Rc::new(Item {
                    id: "D",
                    eligible: true,
                })));
            }
            if mode == "shrink" {
                items.borrow_mut().truncate(1);
            }
            if mode == "pair sync" {
                return Err(original.clone());
            }
            let original = &original;
            let released = &released;
            Ok(Box::pin(std::future::poll_fn(move |_| {
                if !released.get() {
                    Poll::Pending
                } else if ["pair async", "report throws"].contains(&mode) {
                    Poll::Ready(Err(original.clone()))
                } else {
                    Poll::Ready(Ok(ids.iter().map(|id| id_units("p", id)).collect()))
                }
            })))
        },
        |item: Option<Rc<Item>>| -> Result<Response<'_>, Failure> {
            let id = item.as_ref().map_or("undefined", |i| i.id);
            trace.borrow_mut().push(json!(["single", id]));
            if mode == "single append" && id == "A" {
                items.borrow_mut().push(Some(Rc::new(Item {
                    id: "D",
                    eligible: false,
                })));
            }
            if mode == "single sync" && id == "C" {
                return Err(original.clone());
            }
            Ok(Box::pin(std::future::ready(
                if id == "C" && ["single async", "log throws", "format throws"].contains(&mode) {
                    Err(original.clone())
                } else {
                    Ok(vec![id_units("s", id)])
                },
            )))
        },
        |error, context| {
            assert!(
                Rc::ptr_eq(&error, &original),
                "PLACE-MANY native reported original identity {mode}"
            );
            assert_eq!(context, "Failed to place Backpack native order pair");
            trace.borrow_mut().push(json!(["report", context]));
            if mode == "report throws" {
                Err(report_error.clone())
            } else {
                Ok(())
            }
        },
        |error| {
            assert!(
                Rc::ptr_eq(error, &original),
                "PLACE-MANY native formatted original identity {mode}"
            );
            trace.borrow_mut().push(json!(["format"]));
            if mode == "format throws" {
                Err(format_error.clone())
            } else {
                let mut text = "ActionError: action ".encode_utf16().collect::<Vec<_>>();
                text.push(0xd800);
                text.extend(" failed".encode_utf16());
                Ok(text)
            }
        },
        |message, severity| {
            let mut expected = "Error placing order in batch: ActionError: action "
                .encode_utf16()
                .collect::<Vec<_>>();
            expected.push(0xd800);
            expected.extend(" failed".encode_utf16());
            assert_eq!(
                message, expected,
                "PLACE-MANY native UTF16 log message {mode}"
            );
            assert_eq!(severity, "error");
            trace.borrow_mut().push(json!(["log", message, severity]));
            if mode == "log throws" {
                Err(log_error.clone())
            } else {
                Ok(())
            }
        },
    );
    let eager = match mode {
        "empty" => 2,
        "all single" | "single append" => 6,
        "sparse" => 5,
        "read throws" => 4,
        "pair sync" => 12,
        "assert throws" => 1,
        "length throws" => 2,
        "eligibility throws" => 5,
        _ => 7,
    };
    assert_eq!(
        trace.borrow().len(),
        eager,
        "PLACE-MANY independent eager count {mode}"
    );
    assert_eq!(
        trace.borrow().as_slice(),
        row["atConstruction"].as_array().unwrap()
    );
    let mut context = Context::from_waker(Waker::noop());
    if deferred {
        assert!(
            matches!(action.as_mut().poll(&mut context), Poll::Pending),
            "PLACE-MANY native pending pair"
        );
        assert_eq!(trace.borrow().len(), 7);
        assert_eq!(
            trace.borrow().as_slice(),
            row["whilePending"].as_array().unwrap()
        );
        released.set(true);
    }
    let Poll::Ready(result) = action.as_mut().poll(&mut context) else {
        panic!("PLACE-MANY fixture must complete")
    };
    let expected_error = match mode {
        "report throws" => Some(report_error.clone()),
        "log throws" => Some(log_error.clone()),
        "format throws" => Some(format_error.clone()),
        "assert throws" | "length throws" | "read throws" | "eligibility throws" => {
            Some(original.clone())
        }
        _ => None,
    };
    if let Some(expected) = expected_error {
        let error = result.err().unwrap();
        assert!(
            Rc::ptr_eq(&error, &expected),
            "PLACE-MANY native original/replacement error {mode}"
        );
        assert_eq!(row["outcome"]["label"], **expected);
        assert_eq!(row["outcome"]["ok"], false);
    } else {
        let ids = result.unwrap();
        assert_eq!(ids, expected_ids(mode), "PLACE-MANY native IDs {mode}");
        assert_eq!(row["outcome"]["ok"], true);
        assert_eq!(row["outcome"]["ids"], json!(ids));
    }
    let index = MODES.iter().position(|value| *value == mode).unwrap();
    let totals = [
        2, 12, 16, 13, 13, 14, 14, 8, 13, 12, 1, 2, 4, 5, 15, 17, 8, 12, 21,
    ];
    assert_eq!(
        trace.borrow().len(),
        totals[index],
        "PLACE-MANY independent final count {mode}"
    );
    assert_eq!(
        trace.borrow().as_slice(),
        row["trace"].as_array().unwrap(),
        "PLACE-MANY native trace {mode}"
    );
}
#[test]
fn actual_typescript_multi_order_placement() {
    let c = oracle();
    let rows = c["placements"].as_array().unwrap();
    assert_eq!(rows.len(), 19, "PLACE-MANY independent modes");
    for (i, row) in rows.iter().enumerate() {
        assert_eq!(
            row["mode"], MODES[i],
            "PLACE-MANY independent mode identity {i}"
        );
        run_placement(row, MODES[i], false);
    }
}
#[test]
fn actual_typescript_multi_order_wait() {
    let c = oracle();
    assert_eq!(c["wait"]["mode"], "normal");
    run_placement(&c["wait"], "normal", true);
}
