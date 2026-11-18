use super::*;
use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};

fn events<P>(cx: &mut Cx<'_, P>) -> Vec<LogEvent> {
    cx.io
        .core
        .actions
        .drain(..)
        .map(|action| match action {
            Action::Notice(Notice::Event(event)) => event,
            other => panic!("unexpected diagnostic action: {other:?}"),
        })
        .collect()
}

fn context<'a, P>(store: &'a mut Store, io: &'a mut Io<P>) -> Cx<'a, P> {
    io.core.min_level = Level::Debug;
    Cx {
        now: 10.,
        store,
        io,
    }
}

#[test]
fn ascii_digits_share_a_site_shape_without_sliding_its_error_deadline() {
    let (mut store, mut io) = (Store::default(), Io::<()>::default());
    let mut cx = context(&mut store, &mut io);
    cx.retried_failure("balance", "request 123 failed");
    assert_eq!(events(&mut cx)[0].level, Level::Error);
    assert_eq!(
        cx.io.core.retried_failures["balance"],
        (10., "request ### failed".into())
    );
    cx.now = 90_009.;
    cx.retried_failure("balance", "request 987 failed");
    let repeat = events(&mut cx);
    assert_eq!(
        (repeat[0].level, repeat[0].message.as_str()),
        (Level::Warn, "request 987 failed")
    );
    assert_eq!(cx.io.core.retried_failures["balance"].0, 10.);
    cx.now = 90_010.;
    cx.retried_failure("balance", "request 456 failed");
    assert_eq!(events(&mut cx)[0].level, Level::Error);
    assert_eq!(cx.io.core.retried_failures["balance"].0, 90_010.);
    cx.retried_failure("orders", "request 456 failed");
    assert_eq!(events(&mut cx)[0].level, Level::Error);
    assert_eq!(cx.io.core.retried_failures.len(), 2);
    cx.retried_failure("balance", "request 456 refused");
    assert_eq!(events(&mut cx)[0].level, Level::Error);
    assert_eq!(
        cx.io.core.retried_failures["orders"].1,
        "request ### failed"
    );
}

#[test]
fn unicode_digits_and_ascii_digit_count_remain_distinct_shapes() {
    let (mut store, mut io) = (Store::default(), Io::<()>::default());
    let mut cx = context(&mut store, &mut io);
    for (message, shape) in [
        ("拒否١２3", "拒否١２#"),
        ("拒否٢２4", "拒否٢２#"),
        ("拒否٢２45", "拒否٢２##"),
    ] {
        cx.retried_failure("site", message);
        assert_eq!(events(&mut cx)[0].level, Level::Error);
        assert_eq!(cx.io.core.retried_failures["site"].1, shape);
    }
    cx.retried_failure("site", "拒否٢２67");
    assert_eq!(events(&mut cx)[0].level, Level::Warn);
}

#[test]
fn backwards_and_nonfinite_clocks_keep_the_original_severity_comparisons() {
    let (mut store, mut io) = (Store::default(), Io::<()>::default());
    let mut cx = context(&mut store, &mut io);
    for (now, level) in [
        (10., Level::Error),
        (0., Level::Warn),
        (f64::NEG_INFINITY, Level::Warn),
        (f64::NAN, Level::Error),
        (11., Level::Error),
        (f64::INFINITY, Level::Error),
        (12., Level::Warn),
    ] {
        cx.now = now;
        cx.retried_failure("site", "busy 1");
        assert_eq!(events(&mut cx)[0].level, level);
    }
}

struct Conversion<'a> {
    calls: &'a Cell<usize>,
    panic: bool,
}

impl From<Conversion<'_>> for String {
    fn from(value: Conversion<'_>) -> Self {
        value.calls.set(value.calls.get() + 1);
        assert!(
            !value.panic,
            "conversion stopped before diagnostic bookkeeping"
        );
        "converted 12".into()
    }
}

#[test]
fn public_message_conversion_runs_once_before_any_map_or_action_change() {
    let (mut store, mut io) = (Store::default(), Io::<()>::default());
    let mut cx = context(&mut store, &mut io);
    let calls = Cell::new(0);
    assert!(
        catch_unwind(AssertUnwindSafe(|| cx.retried_failure(
            "site",
            Conversion {
                calls: &calls,
                panic: true
            }
        )))
        .is_err()
    );
    assert_eq!(calls.get(), 1);
    assert!(cx.io.core.retried_failures.is_empty());
    assert!(cx.io.core.actions.is_empty());
    for level in [Level::Error, Level::Warn] {
        let before = calls.get();
        cx.retried_failure(
            "site",
            Conversion {
                calls: &calls,
                panic: false,
            },
        );
        assert_eq!(calls.get(), before + 1);
        assert_eq!(events(&mut cx)[0].level, level);
    }
    let before = cx.io.core.retried_failures.clone();
    assert!(
        catch_unwind(AssertUnwindSafe(|| cx.retried_failure(
            "site",
            Conversion {
                calls: &calls,
                panic: true
            }
        )))
        .is_err()
    );
    assert_eq!(cx.io.core.retried_failures, before);
    assert!(cx.io.core.actions.is_empty());
}

#[test]
fn classified_fields_actions_and_auth_counting_keep_their_original_order() {
    let (mut store, mut io) = (Store::default(), Io::<()>::default());
    let mut cx = context(&mut store, &mut io);
    let error = VenueError {
        venue_code: Some("10010".into()),
        venue_message: Some("denied".into()),
        retry_after_ms: Some(125.),
        ..VenueError::new("credential refused").kind(ErrorKind::AuthPermission)
    };
    cx.notice_error(Level::Warn, log_code::ORDER_REJECTED, &error);
    assert!(!cx.io.core.auth_failure_counted);
    cx.retried_failure_error("balance", &error);
    assert!(cx.io.core.auth_failure_counted);
    let once = cx.io.core.health.clone();
    cx.retried_failure_error("balance", &error);
    let output = events(&mut cx);
    assert_eq!(output.len(), 3);
    assert_eq!(
        (output[0].code, output[0].level),
        (log_code::ORDER_REJECTED, Level::Warn)
    );
    assert_eq!(
        (output[1].code, output[1].level),
        (log_code::AUTH_REJECTED, Level::Error)
    );
    assert_eq!(
        (output[2].code, output[2].level),
        (log_code::AUTH_REJECTED, Level::Warn)
    );
    for event in &output {
        assert_eq!(event.message, "credential refused");
        assert_eq!(
            event.fields,
            vec![
                ("kind", "authPermission".into()),
                ("venueCode", "10010".into()),
                ("venueMessage", "denied".into()),
                ("retryAfterMs", 125.0.into())
            ]
        );
    }
    assert_eq!(format!("{:?}", cx.io.core.health), format!("{once:?}"));
    cx.retried_failure_error("network", &VenueError::new("network down"));
    let plain = events(&mut cx);
    assert_eq!(plain[0].code, log_code::REQUEST_FAILED);
    assert!(plain[0].fields.is_empty());
}

#[test]
fn hidden_warning_preserves_site_state_without_renewing_the_error_deadline() {
    let (mut store, mut io) = (Store::default(), Io::<()>::default());
    let mut cx = context(&mut store, &mut io);
    cx.io.core.min_level = Level::Error;
    cx.retried_failure("site", "failure 1");
    assert_eq!(events(&mut cx).len(), 1);
    let health = format!("{:?}", cx.io.core.health);
    cx.now = 1_000.;
    cx.retried_failure("site", "failure 2");
    assert!(events(&mut cx).is_empty());
    assert_eq!(format!("{:?}", cx.io.core.health), health);
    assert_eq!(
        cx.io.core.retried_failures["site"],
        (10., "failure #".into())
    );
    cx.now = 90_010.;
    cx.retried_failure("site", "failure 3");
    assert_eq!(events(&mut cx)[0].level, Level::Error);
}

#[test]
fn diagnostic_records_are_independent_of_the_pending_type() {
    fn records<P>() -> String {
        let (mut store, mut io) = (Store::default(), Io::<P>::default());
        let mut cx = context(&mut store, &mut io);
        cx.retried_failure("one", "request 123 failed");
        cx.now = 90_009.;
        cx.retried_failure("one", "request 987 failed");
        cx.now = 90_010.;
        cx.retried_failure_error(
            "two",
            &VenueError::new("rejected").kind(ErrorKind::AuthInvalid),
        );
        let actions = events(&mut cx);
        let sites: std::collections::BTreeMap<_, _> = cx.io.core.retried_failures.iter().collect();
        format!("{actions:?}|{sites:?}|{:?}", cx.io.core.health)
    }
    let unit = records::<()>();
    let bytes = records::<Vec<u8>>();
    assert_eq!(unit, bytes);
    println!("DIAGNOSTIC_RECORDS={unit}");
}
