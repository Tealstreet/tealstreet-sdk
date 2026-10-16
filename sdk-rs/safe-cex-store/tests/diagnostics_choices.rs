use safe_cex_store::{remaining_utils::merge_updates_using_last_update, venue_rest_diagnostics::*};
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

const T0: &str = "safeCexRestDiagStartedAt";
const SIGN_MS: &str = "safeCexRestDiagSignMs";
const SIGNED_AT: &str = "safeCexRestDiagSignedAt";
const AGE: &str = "safeCexRestDiagSignatureAgeMs";

fn sealed_config(numbers: &[(&str, f64)]) -> RestDiagnosticConfig {
    RestDiagnosticConfig {
        sealed: true,
        numbers: numbers
            .iter()
            .map(|(key, value)| ((*key).into(), *value))
            .collect(),
        ..Default::default()
    }
}

fn timestamp_getter(
    config: &mut RestDiagnosticConfig,
    result: Result<Option<serde_json::Value>, String>,
) -> Rc<Cell<usize>> {
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    config.headers.get = Some(Rc::new(move |name| {
        assert_eq!(
            name, "x-bapi-timestamp",
            "REST SEALED native accessor argument"
        );
        observed.set(observed.get() + 1);
        result.clone()
    }));
    calls
}

#[test]
fn sealed_request_start_overwrites_existing_field() {
    for initial in [1.0, 0.0, -0.0, f64::NAN] {
        let mut config = sealed_config(&[(T0, initial)]);
        let identity = &config as *const _;
        assert_eq!(
            mark_rest_request_start(&mut config, 2.0) as *const _,
            identity
        );
        assert_eq!(
            config.numbers[T0], 2.0,
            "REST SEALED native existing request stamp"
        );
    }
    let mut absent = sealed_config(&[]);
    mark_rest_request_start(&mut absent, 2.0);
    assert!(
        absent.numbers.is_empty(),
        "REST SEALED native missing request stamp stays absent"
    );
}

#[test]
fn sealed_proxy_stamp_overwrites_existing_field() {
    for initial in ["empty", ""] {
        let mut config = sealed_config(&[]);
        config.proxy_header = Some(initial.into());
        let identity = &config as *const _;
        assert_eq!(
            mark_rest_proxy_header(&mut config, "present") as *const _,
            identity
        );
        assert_eq!(
            config.proxy_header.as_deref(),
            Some("present"),
            "REST SEALED native existing proxy stamp"
        );
    }
    let mut absent = sealed_config(&[]);
    mark_rest_proxy_header(&mut absent, "present");
    assert!(
        absent.proxy_header.is_none(),
        "REST SEALED native missing proxy stamp stays absent"
    );
}

#[test]
fn sealed_direct_signature_stamps_keep_order_and_error_precedence() {
    for initial in [[1.0, 2.0, 3.0], [0.0, -0.0, f64::NAN]] {
        let mut complete = sealed_config(&[
            (SIGN_MS, initial[0]),
            (SIGNED_AT, initial[1]),
            (AGE, initial[2]),
        ]);
        let calls = timestamp_getter(&mut complete, Ok(Some(json!("900"))));
        let result = stamp_signed(&mut complete, 9.0, 1100.0);
        assert_eq!(
            complete.numbers[SIGN_MS], 9.0,
            "REST SEALED native direct first write"
        );
        assert!(
            result.is_ok(),
            "REST SEALED native existing signature fields succeed"
        );
        assert_eq!(complete.numbers[SIGNED_AT], 1100.0);
        assert_eq!(complete.numbers[AGE], 200.0);
        assert_eq!(
            calls.get(),
            1,
            "REST SEALED native bound accessor invocation count"
        );
    }

    for (initial, expected, reads) in [
        (vec![], vec![], 0),
        (vec![(SIGN_MS, 1.0)], vec![(SIGN_MS, 9.0)], 0),
        (
            vec![(SIGN_MS, 1.0), (SIGNED_AT, 2.0)],
            vec![(SIGN_MS, 9.0), (SIGNED_AT, 1100.0)],
            1,
        ),
    ] {
        let mut config = sealed_config(&initial);
        let calls = timestamp_getter(&mut config, Ok(Some(json!("900"))));
        assert_eq!(
            stamp_signed(&mut config, 9.0, 1100.0).unwrap_err(),
            "sealed config"
        );
        assert_eq!(
            config
                .numbers
                .iter()
                .map(|(k, v)| (k.as_str(), *v))
                .collect::<Vec<_>>(),
            expected,
            "REST SEALED native rejected assignment retains ordered prefix"
        );
        assert_eq!(
            calls.get(),
            reads,
            "REST SEALED native rejection accessor order"
        );
    }

    let mut failed = sealed_config(&[(SIGN_MS, 1.0), (SIGNED_AT, 2.0), (AGE, 17.0)]);
    let calls = timestamp_getter(&mut failed, Err("header failed".into()));
    assert_eq!(
        stamp_signed(&mut failed, 9.0, 1100.0).unwrap_err(),
        "header failed",
        "REST SEALED native original accessor failure wins"
    );
    assert_eq!(failed.numbers[SIGN_MS], 9.0);
    assert_eq!(failed.numbers[SIGNED_AT], 1100.0);
    assert_eq!(failed.numbers[AGE], 17.0);
    assert_eq!(calls.get(), 1);

    for initial_age in [None, Some(17.0)] {
        let mut config = sealed_config(&[(SIGN_MS, 1.0), (SIGNED_AT, 2.0)]);
        if let Some(age) = initial_age {
            config.numbers.insert(AGE.into(), age);
        }
        let calls = timestamp_getter(&mut config, Ok(Some(json!("invalid"))));
        assert!(
            stamp_signed(&mut config, 9.0, 1100.0).is_ok(),
            "REST SEALED native invalid timestamp skips age assignment"
        );
        assert_eq!(config.numbers.get(AGE).copied(), initial_age);
        assert_eq!(config.numbers[SIGN_MS], 9.0);
        assert_eq!(config.numbers[SIGNED_AT], 1100.0);
        assert_eq!(calls.get(), 1);
    }
}

#[test]
fn sealed_public_signature_wrapper_keeps_partial_writes() {
    let mut config = sealed_config(&[(SIGN_MS, 1.0), (SIGNED_AT, 2.0)]);
    let calls = timestamp_getter(&mut config, Ok(Some(json!("900"))));
    let identity = &config as *const _;
    assert_eq!(
        mark_rest_signed(&mut config, 9.0, 1100.0) as *const _,
        identity
    );
    assert_eq!(
        config.numbers[SIGN_MS], 9.0,
        "REST SEALED native public wrapper keeps first write"
    );
    assert_eq!(config.numbers[SIGNED_AT], 1100.0);
    assert!(!config.numbers.contains_key(AGE));
    assert_eq!(calls.get(), 1);
}

#[derive(Clone)]
struct Row {
    id: &'static str,
    label: &'static str,
    last: Cell<Option<f64>>,
}

fn row(id: &'static str, label: &'static str) -> Rc<Row> {
    Rc::new(Row {
        id,
        label,
        last: Cell::new(Some(1500.0)),
    })
}

#[test]
fn merge_reads_truthy_last_update_twice() {
    for second in [Some(0.0), None] {
        let update = row("X", "U");
        let existing = row("X", "E");
        let reads = Cell::new(0);
        let result = merge_updates_using_last_update(
            std::slice::from_ref(&update),
            std::slice::from_ref(&existing),
            2000.0,
            1000.0,
            |row| row.id,
            |_| {
                reads.set(reads.get() + 1);
                if reads.get() == 1 {
                    Some(1500.0)
                } else {
                    second
                }
            },
        );
        let expected = if second.is_some() { &update } else { &existing };
        assert!(
            Rc::ptr_eq(&result[0], expected),
            "MERGE native selected identity after second lastUpdate read"
        );
        assert_eq!(reads.get(), 2, "MERGE native independent two-read floor");
    }
}

#[test]
fn merge_finishes_mapping_before_deduplication_reads() {
    let ua = row("A", "U_A");
    let ub = row("B", "U_B");
    let ea = row("A", "E_A");
    let eb = row("B", "E_B");
    let trace = RefCell::new(Vec::new());
    let a_reads = Cell::new(0);
    let result = merge_updates_using_last_update(
        &[ua, ub],
        &[ea.clone(), eb.clone()],
        2000.0,
        1000.0,
        |row| {
            trace.borrow_mut().push(format!("id:{}", row.label));
            if row.label == "E_A" {
                a_reads.set(a_reads.get() + 1);
                if a_reads.get() == 3 {
                    eb.last.set(Some(0.0));
                }
            }
            row.id
        },
        |row| {
            trace.borrow_mut().push(format!("last:{}", row.label));
            row.last.get()
        },
    );
    assert!(
        Rc::ptr_eq(&result[1], &eb),
        "MERGE native selected second existing before dedupe effect"
    );
    assert!(Rc::ptr_eq(&result[0], &ea));
    assert_eq!(
        a_reads.get(),
        4,
        "MERGE native independent existing-A read floor"
    );
    let expected = [
        "id:E_A", "id:U_A", "last:E_A", "last:E_A", "id:E_A", "id:U_B", "id:E_B", "id:U_B",
        "last:E_B", "last:E_B", "id:E_A", "id:E_B", "id:E_A", "id:E_B",
    ]
    .map(str::to_owned);
    assert_eq!(
        trace.borrow().as_slice(),
        &expected,
        "MERGE native eager map then dedupe trace"
    );
}
