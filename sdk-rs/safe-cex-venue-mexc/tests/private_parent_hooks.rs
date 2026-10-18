#[path = "support/private_store.rs"]
mod observed_store;
use observed_store::{ObservedStore, WriteCall};
use safe_cex_exchange::runtime::{ExchangeError, ExchangeErrorOrigin};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::OrderStatus;
use safe_cex_venue_mexc::{utils::normalize_market, ws_private::MexcPrivateHandlers};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    error::Error,
    fmt,
    rc::Rc,
};
#[derive(Debug)]
struct HookOrigin(u32);
impl fmt::Display for HookOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("private parent hook failure")
    }
}
impl Error for HookOrigin {}
const ROOTS: [(&str, &str, usize); 10] = [
    ("order", "reject", 1),
    ("order", "reject", 2),
    ("plan", "seen", 1),
    ("plan", "seen", 2),
    ("plan", "shield", 1),
    ("plan", "shield", 2),
    ("stop", "seen", 1),
    ("stop", "seen", 2),
    ("stop", "shield", 1),
    ("stop", "shield", 2),
];
fn fixture(kind: &str) -> Value {
    let rows:Vec<Value>=["a","b"].into_iter().enumerate().map(|(i,id)| {
        let mut row=json!({"symbol":"BTC_USDT","createTime":1000,"vol":2*(i+1)});
        let extra=match kind { "order"=>json!({"orderId":id,"side":1,"state":4,"errorCode":510+i,"dealVol":0,"price":10}),"plan"=>json!({"id":id,"side":1,"state":1,"triggerPrice":21}),"stop"=>json!({"stopPlanOrderId":id,"positionType":1,"state":1,"stopLossPrice":5}),_=>unreachable!() };
        row.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());row
    }).collect();
    json!({"data":rows})
}
#[test]
fn private_parent_hook_errors_abort_at_the_original_call_and_preserve_committed_prefix() {
    let (
        mut getter_total,
        mut first_total,
        mut later_total,
        mut reject_total,
        mut seen_total,
        mut shield_total,
    ) = (0, 0, 0, 0, 0, 0);
    for (kind, hook, fail_at) in ROOTS {
        let store = ObservedStore::new();
        let market = normalize_market(
            &serde_json::Value::Object(
                (json!({"symbol":"BTC_USDT","contractSize":0.5,"maxLeverage":20})
                    .as_object()
                    .unwrap())
                .clone(),
            )
            .into(),
        )
        .unwrap();
        Store::update(
            &store,
            StoreUpdate {
                markets: Some(vec![market]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let first = Rc::new(Cell::new(0));
        let later = Rc::new(Cell::new(0));
        let trace = Rc::new(RefCell::new(Vec::<String>::new()));
        let calls = first.clone();
        let log = trace.clone();
        let _first = Store::subscribe(
            &store,
            Box::new(move |_, _| {
                calls.set(calls.get() + 1);
                log.borrow_mut().push("store".into());
                Ok(())
            }),
        );
        let calls = later.clone();
        let _later = Store::subscribe(
            &store,
            Box::new(move |_, _| {
                calls.set(calls.get() + 1);
                Ok(())
            }),
        );
        let mut original = ExchangeError::new("private parent hook failure");
        original.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(919)));
        original.origin = Some(ExchangeErrorOrigin::new(HookOrigin(919)));
        let rejected = Rc::new(RefCell::new(Vec::<(String, f64)>::new()));
        let seen = Rc::new(RefCell::new(Vec::<String>::new()));
        let shields = Rc::new(RefCell::new(Vec::<String>::new()));
        let calls = rejected.clone();
        let log = trace.clone();
        let origin = original.clone();
        let reject = Rc::new(move |id: String, code: f64| {
            log.borrow_mut().push(format!("reject:{id}"));
            calls.borrow_mut().push((id, code));
            if hook == "reject" && calls.borrow().len() == fail_at {
                Err(origin.clone())
            } else {
                Ok(())
            }
        });
        let calls = seen.clone();
        let log = trace.clone();
        let origin = original.clone();
        let mark = Rc::new(move |id: String| {
            log.borrow_mut().push(format!("seen:{id}"));
            calls.borrow_mut().push(id);
            if hook == "seen" && calls.borrow().len() == fail_at {
                Err(origin.clone())
            } else {
                Ok(())
            }
        });
        let calls = shields.clone();
        let log = trace.clone();
        let origin = original.clone();
        let shield = Rc::new(move |id: &str| {
            log.borrow_mut().push(format!("shield:{id}"));
            calls.borrow_mut().push(id.into());
            if hook == "shield" && calls.borrow().len() == fail_at {
                Err(origin.clone())
            } else {
                Ok(false)
            }
        });
        let handlers = MexcPrivateHandlers::new_with_fallible_hooks(
            Rc::new(|v| v),
            None,
            reject,
            mark,
            shield,
            Rc::new(|_| -> Result<(), ExchangeError> { panic!("no fill") }),
        );
        let frame = fixture(kind);
        let result = match kind {
            "order" => handlers.handle_order(&store, &frame),
            "plan" => handlers.handle_plan_order(&store, &frame),
            "stop" => handlers.handle_stop_order(&store, &frame),
            _ => unreachable!(),
        };
        let error = result.expect_err("private native original parent hook error propagates");
        let cause = error
            .source()
            .unwrap()
            .downcast_ref::<HookOrigin>()
            .expect("private native typed original hook cause");
        let retained = original
            .source()
            .unwrap()
            .downcast_ref::<HookOrigin>()
            .unwrap();
        assert_eq!(cause.0, 919, "private native opaque parent hook code");
        assert!(
            std::ptr::eq(cause, retained),
            "private native exact original parent hook identity"
        );
        assert_eq!(error.name, "Error");
        assert_eq!(error.message, "private parent hook failure");
        assert_eq!(
            observed_error_code(&error.code).as_ref().unwrap().as_u64(),
            Some(919)
        );
        assert_eq!(
            store.market_reads.get(),
            2,
            "private native independent hook market getter count"
        );
        assert_eq!(
            first.get(),
            if kind == "order" { 3 } else { 0 },
            "private native hook first subscriber count"
        );
        assert_eq!(
            later.get(),
            if kind == "order" { 3 } else { 0 },
            "private native hook later subscriber count"
        );
        let ids: Vec<String> = ["a", "b"]
            .into_iter()
            .take(fail_at)
            .map(str::to_owned)
            .collect();
        let expected_reject: Vec<(String, f64)> = if kind == "order" {
            ids.iter()
                .enumerate()
                .map(|(i, id)| (id.clone(), 510. + i as f64))
                .collect()
        } else {
            vec![]
        };
        assert_eq!(
            rejected.borrow().len(),
            expected_reject.len(),
            "private native independent rejected hook cutoff"
        );
        for ((id, code), (want_id, want_code)) in rejected.borrow().iter().zip(&expected_reject) {
            assert_eq!(id, want_id);
            assert_eq!(code.to_bits(), want_code.to_bits());
        }
        assert_eq!(
            *seen.borrow(),
            if hook == "seen" {
                ids.clone()
            } else if hook == "shield" {
                vec!["a".into(), "b".into()]
            } else {
                vec![]
            },
            "private native seen hook cutoff"
        );
        assert_eq!(
            *shields.borrow(),
            if hook == "shield" {
                ids.clone()
            } else {
                vec![]
            },
            "private native shield hook cutoff"
        );
        let expected_trace = if kind == "order" {
            let mut x = vec!["store".into(), "store".into(), "store".into()];
            x.extend(ids.iter().map(|id| format!("reject:{id}")));
            x
        } else if hook == "seen" {
            ids.iter().map(|id| format!("seen:{id}")).collect()
        } else {
            let mut x = vec!["seen:a".into(), "seen:b".into()];
            x.extend(ids.iter().map(|id| format!("shield:{id}")));
            x
        };
        assert_eq!(
            *trace.borrow(),
            expected_trace,
            "private native full independent hook and Store order"
        );
        let expected_writes = if kind == "order" {
            vec![
                WriteCall::Orders(vec!["a".into(), "b".into()]),
                WriteCall::Status("a".into(), OrderStatus::Canceled),
                WriteCall::Status("b".into(), OrderStatus::Canceled),
            ]
        } else {
            vec![]
        };
        assert_eq!(
            *store.writes.borrow(),
            expected_writes,
            "private native parent-hook Store write cutoff"
        );
        let orders = store.orders().unwrap();
        assert_eq!(
            orders.len(),
            if kind == "order" { 2 } else { 0 },
            "private native committed order count before hook throw"
        );
        if kind == "order" {
            for (i, order) in orders.iter().enumerate() {
                assert_eq!(order.id, ["a", "b"][i]);
                assert_eq!(order.status, OrderStatus::Canceled);
                assert_eq!(order.amount.to_bits(), [1f64, 2f64][i].to_bits());
            }
        }
        getter_total += store.market_reads.get();
        first_total += first.get();
        later_total += later.get();
        reject_total += rejected.borrow().len();
        seen_total += seen.borrow().len();
        shield_total += shields.borrow().len();
    }
    assert_eq!(
        (
            getter_total,
            first_total,
            later_total,
            reject_total,
            seen_total,
            shield_total
        ),
        (20, 6, 6, 3, 14, 6),
        "private native independent parent-hook call floors"
    );
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/private_hook_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual hook source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        rows.len(),
        10,
        "private Source independent parent-hook root floor"
    );
    for (i, (kind, hook, fail_at)) in ROOTS.into_iter().enumerate() {
        let row = &rows[i];
        assert_eq!(row["kind"], kind);
        assert_eq!(row["hook"], hook);
        assert_eq!(row["failAt"], fail_at);
        assert_eq!(row["reads"], 2);
        assert_eq!(row["first"], if kind == "order" { 3 } else { 0 });
        assert_eq!(row["later"], if kind == "order" { 3 } else { 0 });
        assert_eq!(row["error"]["code"], 919);
    }
}

fn observed_error_code(
    code: &Option<safe_cex_exchange::js_runtime::value::Js>,
) -> Option<serde_json::Value> {
    code.as_ref().map(|value| {
        let finite =
            safe_cex_exchange::js_runtime::value::to_json(value).expect("finite JSON code fixture");
        let text = safe_cex_num::js_json_stringify(&finite.into())
            .expect("source JSON code fixture")
            .expect("defined code fixture");
        serde_json::from_str(&text).expect("JSON code observation")
    })
}
