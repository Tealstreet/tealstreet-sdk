#![cfg(not(target_arch = "wasm32"))]
use safe_cex_exchange::runtime::{ExchangeError, LogSeverity};
use safe_cex_venue_grvt::{
    private_commands::*, public_subscriptions::NumberKey, types::GrvtSubscriptionMethod,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::OnceLock,
};
fn corpus() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/private-commands-source.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "PRIVATE source oracle {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
fn scalar(actual: f64, source: &Value, label: &str) {
    if source["nan"] == true {
        assert!(actual.is_nan(), "{label} NaN");
    } else {
        assert_eq!(
            format!("{:016x}", actual.to_bits()),
            source["bits"].as_str().unwrap(),
            "{label}"
        );
    }
}
fn error(actual: Option<&ExchangeError>, source: &Value, label: &str) {
    if let Some(actual) = actual {
        assert_eq!(
            actual.name,
            source["name"].as_str().unwrap(),
            "{label} native error name"
        );
        assert_eq!(
            actual.message,
            source["message"].as_str().unwrap(),
            "{label} native error message"
        );
    } else {
        assert!(source.is_null(), "{label} unexpected success");
    }
}
fn state() -> PrivateCommands<u64> {
    let s = PrivateCommands::default();
    s.authenticated.set(true);
    s.private_stream_healthy_at.set(Some(123.0));
    s.session_refresh_attempts.set(3.0);
    s.ping_at.set(7.5);
    s.processed_fill_ids
        .borrow_mut()
        .extend(["a".into(), "b".into()]);
    s
}
fn severity(s: Option<LogSeverity>) -> Value {
    s.map(|s| json!(s.as_str()))
        .unwrap_or_else(|| json!({"undefined":true}))
}
fn feed(values: &[&str]) -> StreamFeed {
    Rc::new(RefCell::new(values.iter().map(|s| (*s).into()).collect()))
}
#[derive(Clone)]
struct Observed {
    request: LegacySubscription,
    feed: Vec<String>,
}
fn observe(request: &LegacySubscription) -> Observed {
    Observed {
        request: request.clone(),
        feed: request.feed.borrow().clone(),
    }
}
fn legacy(actual: &Observed, source: &Value, label: &str) {
    scalar(
        actual.request.request_id,
        &source["request_id"],
        &format!("{label} native request id"),
    );
    assert_eq!(
        actual.request.method,
        GrvtSubscriptionMethod::Subscribe,
        "{label} native method"
    );
    assert_eq!(source["method"], "subscribe", "{label} source method");
    assert_eq!(
        actual.request.stream,
        source["stream"].as_str().unwrap(),
        "{label} native stream"
    );
    assert_eq!(
        actual.request.is_full,
        source["is_full"].as_bool().unwrap(),
        "{label} native lite flag"
    );
    assert_eq!(
        actual.feed,
        source["feed"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect::<Vec<_>>(),
        "{label} native feed values"
    );
}
fn frames(actual: &[Observed], source: &Value, count: usize, label: &str) {
    assert_eq!(actual.len(), count, "{label} independent request count");
    assert_eq!(
        source.as_array().unwrap().len(),
        count,
        "{label} independent source request count"
    );
    for (a, b) in actual.iter().zip(source.as_array().unwrap()) {
        legacy(a, b, label);
    }
}
fn snapshot(s: &PrivateCommands<u64>, source: &Value, counts: (usize, usize), label: &str) {
    assert_eq!(
        s.is_disposed.get(),
        source["disposed"].as_bool().unwrap(),
        "{label} native disposed"
    );
    assert_eq!(
        s.authenticated.get(),
        source["authenticated"].as_bool().unwrap(),
        "{label} native authenticated"
    );
    assert_eq!(
        s.authentication_failed.get(),
        source["failed"].as_bool().unwrap(),
        "{label} native failed"
    );
    if let Some(healthy) = s.private_stream_healthy_at.get() {
        assert_ne!(
            source["healthy"]["undefined"], true,
            "{label} native healthy presence"
        );
        scalar(
            healthy,
            &source["healthy"],
            &format!("{label} native healthy time"),
        );
    } else {
        assert_eq!(
            source["healthy"]["undefined"], true,
            "{label} native healthy absent"
        );
    }
    scalar(
        s.session_refresh_attempts.get(),
        &source["attempts"],
        &format!("{label} native attempts"),
    );
    scalar(
        s.request_id.get(),
        &source["next"],
        &format!("{label} native next id"),
    );
    scalar(
        s.ping_at.get(),
        &source["ping"],
        &format!("{label} native ping time"),
    );
    let subs = s.subscriptions.borrow();
    assert_eq!(
        subs.len(),
        counts.0,
        "{label} independent native subscription count"
    );
    assert_eq!(
        source["subscriptions"].as_array().unwrap().len(),
        counts.0,
        "{label} independent source subscription count"
    );
    for ((key, sub), entry) in subs.iter().zip(source["subscriptions"].as_array().unwrap()) {
        scalar(key.value(), &entry[0], &format!("{label} native Map key"));
        legacy(&observe(sub), &entry[1], label);
    }
    let fills = s.processed_fill_ids.borrow();
    assert_eq!(
        fills.len(),
        counts.1,
        "{label} independent native fill count"
    );
    assert_eq!(
        source["fills"].as_array().unwrap().len(),
        counts.1,
        "{label} independent source fill count"
    );
    for (actual, expected) in fills.iter().zip(source["fills"].as_array().unwrap()) {
        assert_eq!(actual, expected.as_str().unwrap(), "{label} native fill id");
    }
    if let Some(timer) = *s.orders_throttle_timeout.borrow() {
        scalar(
            timer as f64,
            &source["timer"],
            &format!("{label} native timer"),
        );
    } else {
        assert_eq!(
            source["timer"]["undefined"], true,
            "{label} native timer absent"
        );
    }
}
fn events(actual: &[Value], source: &Value, count: usize, label: &str) {
    assert_eq!(
        actual.len(),
        count,
        "{label} independent native event count"
    );
    assert_eq!(
        source.as_array().unwrap().len(),
        count,
        "{label} independent source event count"
    );
    assert_eq!(
        actual,
        source.as_array().unwrap(),
        "{label} native event order"
    );
}
fn wire_row(actual: Vec<String>, source: &Value) -> (Vec<String>, Vec<String>) {
    (
        actual,
        source
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect(),
    )
}
#[test]
fn threshold_native() {
    scalar(
        get_watchdog_stale_threshold_ms(),
        &corpus()["threshold"],
        "THRESHOLD native value",
    );
    assert_eq!(
        get_watchdog_stale_threshold_ms(),
        90000.0,
        "THRESHOLD independent value"
    );
}
#[test]
fn ping_native() {
    let names = [
        "normal",
        "disposed",
        "auth-failed",
        "clock-error",
        "send-error",
        "nan",
        "negative-zero",
        "dispose-during-clock",
    ];
    let counts = [2, 0, 0, 1, 2, 2, 2, 2];
    let rows = corpus()["pingRows"].as_array().unwrap();
    assert_eq!(rows.len(), 8, "PING independent floor");
    for (i, row) in rows.iter().enumerate() {
        let name = names[i];
        assert_eq!(row["name"], name, "PING independent order");
        let s = state();
        s.is_disposed.set(name == "disposed");
        s.authentication_failed.set(name == "auth-failed");
        let e = RefCell::new(Vec::new());
        let actual = s.ping(
            || {
                e.borrow_mut().push(json!(["clock"]));
                if name == "clock-error" {
                    return Err(ExchangeError::new("clock-fail"));
                }
                if name == "dispose-during-clock" {
                    s.is_disposed.set(true);
                }
                Ok(if name == "nan" {
                    f64::NAN
                } else if name == "negative-zero" {
                    -0.0
                } else {
                    1011.25
                })
            },
            |text| {
                assert_eq!(text, "ping", "PING native frame");
                e.borrow_mut().push(json!(["send", text]));
                if name == "send-error" {
                    Err(ExchangeError::new("send-fail"))
                } else {
                    Ok(())
                }
            },
        );
        error(actual.as_ref().err(), &row["error"], "PING");
        scalar(s.ping_at.get(), &row["state"]["ping"], "PING native clock");
        snapshot(&s, &row["state"], (0, 2), "PING");
        events(&e.borrow(), &row["events"], counts[i], "PING");
    }
}
#[test]
fn open_native() {
    let names = [
        "normal",
        "disposed",
        "auth-failed",
        "log-error",
        "auth-error",
        "dispose-during-log",
        "replace-auth",
    ];
    let counts = [2, 0, 0, 1, 2, 2, 2];
    let rows = corpus()["openRows"].as_array().unwrap();
    assert_eq!(rows.len(), 7, "PRIVATE OPEN independent floor");
    for (i, row) in rows.iter().enumerate() {
        let name = names[i];
        assert_eq!(row["name"], name, "PRIVATE OPEN independent order");
        let s = state();
        s.is_disposed.set(name == "disposed");
        s.authentication_failed.set(name == "auth-failed");
        let e = RefCell::new(Vec::new());
        let custom = Cell::new(false);
        let actual = s.on_open(
            |text, level| {
                assert!(
                    !s.authenticated.get(),
                    "PRIVATE OPEN native reset before log"
                );
                assert!(
                    s.private_stream_healthy_at.get().is_none(),
                    "PRIVATE OPEN native healthy reset before log"
                );
                e.borrow_mut().push(json!(["log", text, severity(level)]));
                if name == "log-error" {
                    return Err(ExchangeError::new("log-fail"));
                }
                if name == "dispose-during-log" {
                    s.is_disposed.set(true);
                }
                if name == "replace-auth" {
                    custom.set(true);
                }
                Ok(())
            },
            || {
                e.borrow_mut().push(json!([if custom.get() {
                    "custom-auth"
                } else {
                    "authenticate"
                }]));
                if name == "auth-error" {
                    Err(ExchangeError::new("auth-fail"))
                } else {
                    Ok(())
                }
            },
        );
        error(actual.as_ref().err(), &row["error"], "PRIVATE OPEN");
        snapshot(&s, &row["state"], (0, 2), "PRIVATE OPEN");
        events(&e.borrow(), &row["events"], counts[i], "PRIVATE OPEN");
    }
}
#[test]
fn authentication_native() {
    let names = [
        "normal",
        "no-extras",
        "empty-cookie",
        "empty-account",
        "testnet",
        "log-error",
        "send-error",
        "mutate-after-log",
        "account-reread",
    ];
    let rows = corpus()["authRows"].as_array().unwrap();
    assert_eq!(rows.len(), 9, "AUTH independent floor");
    let mut wires_out = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        let name = names[i];
        assert_eq!(row["name"], name, "AUTH independent order");
        let cookie = RefCell::new(if name == "no-extras" {
            None
        } else {
            Some(
                if name == "empty-cookie" {
                    ""
                } else {
                    "cookie-71"
                }
                .to_owned(),
            )
        });
        let account = RefCell::new(if name == "no-extras" {
            None
        } else {
            Some(
                if name == "empty-account" {
                    ""
                } else {
                    "account-31"
                }
                .to_owned(),
            )
        });
        let testnet = Cell::new(name == "testnet");
        let e = RefCell::new(Vec::new());
        let requests = RefCell::new(Vec::new());
        let wires = RefCell::new(Vec::new());
        let actual = authenticate(
            || {
                if name == "account-reread" {
                    *account.borrow_mut() = Some("CHANGED".into());
                }
                cookie.borrow().clone()
            },
            || account.borrow().clone(),
            || testnet.get(),
            |text, level| {
                e.borrow_mut().push(json!(["log", text, severity(level)]));
                if name == "log-error" {
                    return Err(ExchangeError::new("log-fail"));
                }
                if name == "mutate-after-log" {
                    *cookie.borrow_mut() = Some("OTHER".into());
                    *account.borrow_mut() = Some("OTHER".into());
                    testnet.set(true);
                }
                Ok(())
            },
            |request, text| {
                requests.borrow_mut().push(request.clone());
                wires.borrow_mut().push(text.to_owned());
                e.borrow_mut().push(json!(["send"]));
                if name == "send-error" {
                    Err(ExchangeError::new("send-fail"))
                } else {
                    Ok(())
                }
            },
        );
        error(actual.as_ref().err(), &row["error"], "AUTH");
        let count = usize::from(name != "log-error");
        assert_eq!(
            requests.borrow().len(),
            count,
            "AUTH independent native request count"
        );
        assert_eq!(
            row["requests"].as_array().unwrap().len(),
            count,
            "AUTH independent source request count"
        );
        if count == 1 {
            let requests = requests.borrow();
            let actual = &requests[0];
            let source = &row["requests"][0];
            assert_eq!(
                actual.kind,
                source["type"].as_str().unwrap(),
                "AUTH native type"
            );
            assert_eq!(
                actual.session_cookie,
                source["session_cookie"].as_str().unwrap(),
                "AUTH native cookie"
            );
            if let Some(account) = &actual.account_id {
                assert_eq!(
                    account,
                    source["account_id"].as_str().unwrap(),
                    "AUTH native account"
                );
            } else {
                assert_eq!(
                    source["account_id"]["undefined"], true,
                    "AUTH native undefined account slot"
                );
            }
            assert_eq!(
                actual.testnet,
                source["testnet"].as_bool().unwrap(),
                "AUTH native testnet"
            );
        }
        events(&e.borrow(), &row["events"], 1 + count, "AUTH");
        wires_out.push(wire_row(wires.into_inner(), &row["wires"]));
    }
    for (a, b) in wires_out {
        assert_eq!(a, b, "AUTH wire after native assertions");
    }
}
#[test]
fn budget_native() {
    let names = [
        "normal",
        "boundary-before",
        "zero-marker",
        "no-marker",
        "nan-marker",
        "clock-error",
        "clock-clears-marker",
        "clock-refreshes-marker",
        "infinity-now",
    ];
    let rows = corpus()["budgetRows"].as_array().unwrap();
    assert_eq!(rows.len(), 9, "BUDGET independent floor");
    for (i, row) in rows.iter().enumerate() {
        let name = names[i];
        assert_eq!(row["name"], name, "BUDGET independent order");
        let s = state();
        s.private_stream_healthy_at.set(match name {
            "zero-marker" => Some(0.0),
            "no-marker" => None,
            "nan-marker" => Some(f64::NAN),
            _ => Some(100.0),
        });
        let e = RefCell::new(Vec::new());
        let actual = s.reset_retry_budget_after_healthy_session(|| {
            e.borrow_mut().push(json!(["clock"]));
            if name == "clock-error" {
                return Err(ExchangeError::new("clock-fail"));
            }
            if name == "clock-clears-marker" {
                s.private_stream_healthy_at.set(None);
            }
            if name == "clock-refreshes-marker" {
                s.private_stream_healthy_at.set(Some(300100.0));
            }
            Ok(if name == "infinity-now" {
                f64::INFINITY
            } else if name == "zero-marker" {
                300000.0
            } else if name == "boundary-before" {
                300099.999
            } else {
                300100.0
            })
        });
        error(actual.as_ref().err(), &row["error"], "BUDGET");
        scalar(
            s.session_refresh_attempts.get(),
            &row["state"]["attempts"],
            "BUDGET native attempts",
        );
        snapshot(&s, &row["state"], (0, 2), "BUDGET");
        events(
            &e.borrow(),
            &row["events"],
            usize::from(name != "no-marker"),
            "BUDGET",
        );
    }
}
#[test]
fn closed_native() {
    let reasons = [
        AuthenticationFailureReason::ProxyRejected,
        AuthenticationFailureReason::GrvtError1000,
        AuthenticationFailureReason::RefreshFailed,
    ];
    let names = ["normal", "emit-error", "dispose-error"];
    let rows = corpus()["closedRows"].as_array().unwrap();
    assert_eq!(rows.len(), 9, "CLOSED independent floor");
    for (i, row) in rows.iter().enumerate() {
        let reason = reasons[i / 3];
        let name = names[i % 3];
        assert_eq!(row["reason"], reason.as_str(), "CLOSED independent reason");
        assert_eq!(row["name"], name, "CLOSED independent order");
        let s = state();
        let e = RefCell::new(Vec::new());
        let actual = s.fail_closed_for_authentication(
            reason,
            |text| {
                assert!(
                    !s.authenticated.get(),
                    "CLOSED native authenticated before emit"
                );
                assert!(
                    s.authentication_failed.get(),
                    "CLOSED native failed before emit"
                );
                e.borrow_mut().push(json!(["emit", text]));
                if name == "emit-error" {
                    Err(ExchangeError::new("emit-fail"))
                } else {
                    Ok(())
                }
            },
            || {
                e.borrow_mut().push(json!(["dispose"]));
                if name == "dispose-error" {
                    Err(ExchangeError::new("dispose-fail"))
                } else {
                    s.is_disposed.set(true);
                    Ok(())
                }
            },
        );
        error(actual.as_ref().err(), &row["error"], "CLOSED");
        snapshot(&s, &row["state"], (0, 2), "CLOSED");
        events(
            &e.borrow(),
            &row["events"],
            if name == "emit-error" { 1 } else { 2 },
            "CLOSED",
        );
    }
}
#[test]
fn subscriptions_native() {
    let names = [
        "normal",
        "no-account",
        "empty-account",
        "log-error",
        "send-order-error",
        "send-position-error",
        "send-fill-error",
        "feed-alias",
        "mutate-account-in-log",
        "replace-callee",
    ];
    let sent = [3, 0, 0, 0, 1, 2, 3, 3, 3, 1];
    let calls = [3, 0, 0, 0, 1, 2, 3, 3, 3, 3];
    let event_counts = [4, 1, 1, 1, 2, 3, 4, 4, 4, 4];
    let rows = corpus()["subscribeRows"].as_array().unwrap();
    assert_eq!(rows.len(), 10, "SUBSCRIBE independent floor");
    let mut wires_out = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        let name = names[i];
        assert_eq!(row["name"], name, "SUBSCRIBE independent order");
        let s = state();
        let account = RefCell::new(if name == "no-account" {
            None
        } else {
            Some(
                if name == "empty-account" {
                    ""
                } else {
                    "sub-72"
                }
                .to_owned(),
            )
        });
        let e = RefCell::new(Vec::new());
        let requests = RefCell::new(Vec::new());
        let wires = RefCell::new(Vec::new());
        let feed_refs = RefCell::new(Vec::<StreamFeed>::new());
        let custom = Cell::new(false);
        let actual = subscribe(
            || account.borrow().clone(),
            |text, level| {
                e.borrow_mut().push(json!(["log", text, severity(level)]));
                if name == "log-error" {
                    return Err(ExchangeError::new("log-fail"));
                }
                if name == "mutate-account-in-log" {
                    *account.borrow_mut() = Some("OTHER".into());
                }
                Ok(())
            },
            |stream, feed| {
                feed_refs.borrow_mut().push(feed.clone());
                if custom.get() {
                    e.borrow_mut()
                        .push(json!(["custom", stream, feed.borrow().clone()]));
                    return Ok(900.0);
                }
                s.subscribe_to_stream(stream, feed, |request, text| {
                    requests.borrow_mut().push(observe(request));
                    wires.borrow_mut().push(text.to_owned());
                    e.borrow_mut().push(json!(["send"]));
                    let count = wires.borrow().len();
                    if name == "feed-alias" && count == 1 {
                        request.feed.borrow_mut().push("EXTRA".into());
                    }
                    if name == "replace-callee" && count == 1 {
                        custom.set(true);
                    }
                    let fail = match name {
                        "send-order-error" => 1,
                        "send-position-error" => 2,
                        "send-fill-error" => 3,
                        _ => 0,
                    };
                    if count == fail {
                        Err(ExchangeError::new("send-fail"))
                    } else {
                        Ok(())
                    }
                })
            },
        );
        error(actual.as_ref().err(), &row["error"], "SUBSCRIBE");
        frames(&requests.borrow(), &row["requests"], sent[i], "SUBSCRIBE");
        snapshot(&s, &row["state"], (sent[i], 2), "SUBSCRIBE");
        assert_eq!(
            feed_refs.borrow().len(),
            calls[i],
            "SUBSCRIBE independent dispatch count"
        );
        assert_eq!(
            row["feedCount"].as_u64().unwrap() as usize,
            calls[i],
            "SUBSCRIBE independent source dispatch count"
        );
        let feed_refs = feed_refs.borrow();
        let same = !feed_refs.is_empty() && feed_refs.iter().all(|f| Rc::ptr_eq(f, &feed_refs[0]));
        assert_eq!(
            same,
            row["sameFeeds"].as_bool().unwrap(),
            "SUBSCRIBE native shared feed identity"
        );
        events(&e.borrow(), &row["events"], event_counts[i], "SUBSCRIBE");
        wires_out.push(wire_row(wires.into_inner(), &row["wires"]));
    }
    for (a, b) in wires_out {
        assert_eq!(a, b, "SUBSCRIBE wire after native assertions");
    }
}
#[test]
fn stream_native() {
    let names = [
        "normal",
        "empty-feed",
        "send-error",
        "feed-mutation",
        "negative-zero",
        "nan-id",
        "infinity-id",
        "large-id",
    ];
    let rows = corpus()["streamRows"].as_array().unwrap();
    assert_eq!(rows.len(), 8, "STREAM independent floor");
    let mut wires_out = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        let name = names[i];
        assert_eq!(row["name"], name, "STREAM independent order");
        let s = state();
        s.request_id.set(match name {
            "negative-zero" => -0.0,
            "nan-id" => f64::NAN,
            "infinity-id" => f64::INFINITY,
            "large-id" => 9007199254740992.0,
            _ => 1.0,
        });
        let input = if name == "empty-feed" {
            feed(&[])
        } else {
            feed(&["sub-72", "BTC"])
        };
        let e = RefCell::new(Vec::new());
        let requests = RefCell::new(Vec::new());
        let wires = RefCell::new(Vec::new());
        let actual = s.subscribe_to_stream("v1.position", input.clone(), |request, text| {
            assert!(
                Rc::ptr_eq(&request.feed, &input),
                "STREAM native input feed identity"
            );
            requests.borrow_mut().push(observe(request));
            wires.borrow_mut().push(text.to_owned());
            e.borrow_mut().push(json!(["send"]));
            if name == "feed-mutation" {
                input.borrow_mut().push("EXTRA".into());
            }
            if name == "send-error" {
                Err(ExchangeError::new("send-fail"))
            } else {
                Ok(())
            }
        });
        error(actual.as_ref().err(), &row["error"], "STREAM");
        if let Ok(id) = actual {
            scalar(id, &row["result"], "STREAM native returned id");
        }
        frames(&requests.borrow(), &row["requests"], 1, "STREAM");
        snapshot(&s, &row["state"], (1, 2), "STREAM");
        let subs = s.subscriptions.borrow();
        let stored = &subs.values().next().unwrap().feed;
        assert!(
            Rc::ptr_eq(stored, &input),
            "STREAM native stored feed identity"
        );
        assert_eq!(row["sameFeed"], true, "STREAM source stored feed identity");
        assert_eq!(
            input.borrow().len(),
            if name == "empty-feed" {
                0
            } else if name == "feed-mutation" {
                3
            } else {
                2
            },
            "STREAM independent live feed length"
        );
        events(&e.borrow(), &row["events"], 1, "STREAM");
        wires_out.push(wire_row(wires.into_inner(), &row["wires"]));
    }
    for (a, b) in wires_out {
        assert_eq!(a, b, "STREAM wire after native assertions");
    }
}
#[test]
fn dispose_native() {
    let names = [
        "normal",
        "no-timer",
        "zero-timer",
        "clear-error",
        "base-error",
        "clear-replaces-state",
    ];
    let counts = [2, 1, 1, 1, 2, 2];
    let rows = corpus()["disposeRows"].as_array().unwrap();
    assert_eq!(rows.len(), 6, "DISPOSE independent floor");
    for (i, row) in rows.iter().enumerate() {
        let name = names[i];
        assert_eq!(row["name"], name, "DISPOSE independent order");
        let s = state();
        s.subscriptions.borrow_mut().insert(
            NumberKey::new(1.0),
            LegacySubscription {
                request_id: 1.0,
                method: GrvtSubscriptionMethod::Subscribe,
                stream: "v1.order".into(),
                feed: feed(&["sub-72"]),
                is_full: false,
            },
        );
        *s.orders_throttle_timeout.borrow_mut() = match name {
            "no-timer" => None,
            "zero-timer" => Some(0),
            _ => Some(73),
        };
        let e = RefCell::new(Vec::new());
        let actual = s.dispose(
            |timer| *timer != 0,
            |timer| {
                assert_eq!(
                    s.subscriptions.borrow().len(),
                    0,
                    "DISPOSE native subscriptions clear before timer"
                );
                assert_eq!(
                    s.processed_fill_ids.borrow().len(),
                    0,
                    "DISPOSE native fills clear before timer"
                );
                assert_eq!(*timer, 73, "DISPOSE native clear timer");
                e.borrow_mut().push(json!(["clear", timer]));
                if name == "clear-error" {
                    return Err(ExchangeError::new("clear-fail"));
                }
                if name == "clear-replaces-state" {
                    *s.orders_throttle_timeout.borrow_mut() = Some(99);
                    s.private_stream_healthy_at.set(Some(999.0));
                }
                Ok(())
            },
            || {
                e.borrow_mut().push(json!(["base"]));
                if name == "base-error" {
                    Err(ExchangeError::new("base-fail"))
                } else {
                    s.is_disposed.set(true);
                    Ok(())
                }
            },
        );
        error(actual.as_ref().err(), &row["error"], "DISPOSE");
        snapshot(&s, &row["state"], (0, 0), "DISPOSE");
        events(&e.borrow(), &row["events"], counts[i], "DISPOSE");
    }
}
