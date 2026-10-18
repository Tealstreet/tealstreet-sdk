#[path = "support/repair_runtime.rs"]
mod host;
use host::Runtime;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_types::ExchangeName;
use safe_cex_venue_asterdex::listen_key::{ListenKeyLifecycle, ListenKeyParent, ListenKeyState};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
};

type Response<T> = Rc<RefCell<Option<Result<T, ExchangeError>>>>;
struct Waiting<T>(Response<T>);
impl<T> Future for Waiting<T> {
    type Output = Result<T, ExchangeError>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        self.0
            .borrow_mut()
            .take()
            .map_or(Poll::Pending, Poll::Ready)
    }
}
fn enqueue<T: 'static>(queue: &RefCell<VecDeque<Response<T>>>) -> ExchangeFuture<'static, T> {
    let response = Rc::new(RefCell::new(None));
    queue.borrow_mut().push_back(response.clone());
    Box::pin(Waiting(response))
}
#[derive(Debug)]
struct Request {
    kind: &'static str,
    path: String,
    key: Option<String>,
}
#[derive(Default)]
struct Parent {
    requests: RefCell<Vec<Request>>,
    reconnects: Cell<usize>,
    post: RefCell<VecDeque<Response<String>>>,
    put: RefCell<VecDeque<Response<()>>>,
    connect: RefCell<VecDeque<Response<()>>>,
}
impl ListenKeyParent for Parent {
    fn get_fapi_path(&self, endpoint: &str) -> String {
        format!("/fapi/v3/{endpoint}")
    }
    fn post_listen_key(&self, path: String) -> ExchangeFuture<'static, String> {
        self.requests.borrow_mut().push(Request {
            kind: "post",
            path,
            key: None,
        });
        enqueue(&self.post)
    }
    fn put_listen_key(&self, path: String, key: String) -> ExchangeFuture<'static, ()> {
        self.requests.borrow_mut().push(Request {
            kind: "put",
            path,
            key: Some(key),
        });
        enqueue(&self.put)
    }
    fn reconnect(&self) -> Result<(), ExchangeError> {
        self.reconnects.set(self.reconnects.get() + 1);
        Ok(())
    }
    fn connect_and_subscribe(&self) -> ExchangeFuture<'static, ()> {
        self.requests.borrow_mut().push(Request {
            kind: "connect",
            path: String::new(),
            key: None,
        });
        enqueue(&self.connect)
    }
}
struct Fixture {
    runtime: Rc<Runtime>,
    parent: Rc<Parent>,
    base: Rc<BaseExchange>,
    lifecycle: Rc<ListenKeyLifecycle>,
    results: Rc<RefCell<Vec<String>>>,
}
fn fixture() -> Fixture {
    let runtime = Rc::new(Runtime::default());
    runtime.wall.set(1000.0);
    let parent = Rc::new(Parent::default());
    let base = Rc::new(BaseExchange::new(
        ExchangeName::Asterdex,
        serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
        Rc::new(DefaultStore::new()),
        runtime.clone(),
        ExchangeSettings::default(),
    ));
    let lifecycle = ListenKeyLifecycle::new(base.clone(), parent.clone());
    Fixture {
        runtime,
        parent,
        base,
        lifecycle,
        results: Rc::new(RefCell::new(Vec::new())),
    }
}
fn launch(f: &Fixture, work: ExchangeFuture<'static, ()>) {
    let runtime = f.runtime.clone();
    f.base.start_owned_operation(async move {
        if let Err(error) = work.await {
            runtime.errors.borrow_mut().push(error.message);
        }
    });
}
fn resolve<T>(queue: &RefCell<VecDeque<Response<T>>>, step: &Value, value: T) {
    let response = queue
        .borrow_mut()
        .pop_front()
        .expect("pending native listen-key RPC");
    *response.borrow_mut() = Some(if let Some(error) = step.get("error") {
        Err(ExchangeError::new(error.as_str().unwrap()))
    } else {
        Ok(value)
    });
}
struct Snapshot {
    state: ListenKeyState,
    timers: usize,
    requests: usize,
    reconnects: usize,
}
struct Trace {
    snapshots: Vec<Snapshot>,
    requests: Vec<Request>,
    reconnects: usize,
    results: Vec<String>,
    logs: Vec<(String, String)>,
    errors: Vec<String>,
}
fn run(case: &Value) -> Trace {
    let f = fixture();
    let mut snapshots = Vec::new();
    for step in case["steps"].as_array().unwrap() {
        match step["op"].as_str().unwrap() {
            "fetch" => {
                let work = f.lifecycle.fetch_listen_key();
                let results = f.results.clone();
                launch(
                    &f,
                    Box::pin(async move {
                        let key = work.await?;
                        results.borrow_mut().push(key);
                        Ok(())
                    }),
                );
            }
            "update" => launch(&f, f.lifecycle.update_listen_key()),
            "schedule-refresh" => f
                .lifecycle
                .schedule_listen_key_refresh(step["delay"].as_u64().unwrap())
                .unwrap(),
            "clear-refresh" => f.lifecycle.clear_listen_key_refresh_timer(),
            "start-watchdog" => f.lifecycle.start_listen_key_watchdog().unwrap(),
            "clear-watchdog" => f.lifecycle.clear_listen_key_watchdog_timer(),
            "schedule-connect" => f.lifecycle.schedule_connect_retry().unwrap(),
            "clear-connect" => f.lifecycle.clear_connect_retry_timer(),
            "wall" => f.runtime.wall.set(step["value"].as_f64().unwrap()),
            "tick" => f.runtime.mono.set(step["value"].as_f64().unwrap()),
            "resolve" => match step["kind"].as_str().unwrap() {
                "post" => resolve(
                    &f.parent.post,
                    step,
                    step["key"].as_str().unwrap_or_default().to_owned(),
                ),
                "put" => resolve(&f.parent.put, step, ()),
                "connect" => resolve(&f.parent.connect, step, ()),
                _ => panic!("unknown RPC"),
            },
            "snapshot" => snapshots.push(Snapshot {
                state: f.lifecycle.state(),
                timers: f.runtime.timers(),
                requests: f.parent.requests.borrow().len(),
                reconnects: f.parent.reconnects.get(),
            }),
            _ => panic!("unknown scenario step"),
        }
        f.runtime.pump();
    }
    let trace = Trace {
        snapshots,
        requests: f.parent.requests.borrow_mut().drain(..).collect(),
        reconnects: f.parent.reconnects.get(),
        results: f.results.borrow().clone(),
        logs: f.runtime.logs.borrow().clone(),
        errors: f.runtime.errors.borrow().clone(),
    };
    f.lifecycle.clear_listen_key_refresh_timer();
    f.lifecycle.clear_listen_key_watchdog_timer();
    f.lifecycle.clear_connect_retry_timer();
    f.base.dispose().unwrap();
    trace
}
fn native_assertions(
    name: &str,
    trace: &Trace,
    source: &Value,
    snapshots: usize,
    requests: usize,
    reconnects: usize,
) {
    assert_eq!(trace.snapshots.len(), snapshots, "{name} snapshot floor");
    assert_eq!(
        source["snapshots"].as_array().unwrap().len(),
        snapshots,
        "{name} source snapshot floor"
    );
    assert_eq!(trace.requests.len(), requests, "{name} request count");
    assert_eq!(
        source["requests"].as_array().unwrap().len(),
        requests,
        "{name} source request floor"
    );
    assert_eq!(trace.reconnects, reconnects, "{name} reconnect floor");
    assert_eq!(
        source["reconnects"].as_u64().unwrap() as usize,
        reconnects,
        "{name} source reconnect floor"
    );
    for (actual, expected) in trace
        .snapshots
        .iter()
        .zip(source["snapshots"].as_array().unwrap())
    {
        assert_eq!(
            actual.state.listen_key,
            expected["key"].as_str().unwrap(),
            "{name} native listen key"
        );
        for (field, value) in [
            ("last", actual.state.last_keep_alive_at),
            ("failures", actual.state.consecutive_failures),
            ("retry", actual.state.retry_count),
        ] {
            assert_eq!(
                value,
                expected[field].as_f64().unwrap(),
                "{name} native {field}"
            );
        }
        for (field, value) in [
            ("refresh", actual.state.refresh_timer),
            ("watchdog", actual.state.watchdog_timer),
            ("connect", actual.state.connect_timer),
        ] {
            assert_eq!(
                value,
                expected[field].as_bool().unwrap(),
                "{name} native {field} registration"
            );
        }
        for (field, value) in [
            ("timers", actual.timers),
            ("requests", actual.requests),
            ("reconnects", actual.reconnects),
        ] {
            assert_eq!(
                value,
                expected[field].as_u64().unwrap() as usize,
                "{name} native {field}"
            );
        }
    }
    for (actual, expected) in trace
        .requests
        .iter()
        .zip(source["requests"].as_array().unwrap())
    {
        assert_eq!(
            actual.kind,
            expected["kind"].as_str().unwrap(),
            "{name} native RPC method"
        );
        assert_eq!(
            actual.path,
            expected["path"].as_str().unwrap(),
            "{name} native RPC path"
        );
        assert_eq!(
            actual.key.as_deref(),
            expected.get("key").and_then(Value::as_str),
            "{name} native RPC listen key"
        );
    }
    let expected_results: Vec<_> = source["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(
        trace.results, expected_results,
        "{name} native returned keys"
    );
    let expected_errors: Vec<_> = source["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(trace.errors, expected_errors, "{name} native errors");
}
fn check_family(family: &str) {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/listen-key-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let sources: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let cases: Vec<Value> = serde_json::from_str(include_str!("listen-key-cases.json")).unwrap();
    const NAMES: [&str; 6] = [
        "refresh-success-replacement",
        "refresh-failure-cap",
        "refresh-missing-overlap-and-post-error",
        "watchdog-exact-stale-boundary",
        "connect-retry-cap-and-clear",
        "connect-rejection-retry",
    ];
    const SNAPSHOTS: [usize; 6] = [8, 17, 5, 6, 28, 4];
    const REQUESTS: [usize; 6] = [3, 13, 4, 1, 13, 2];
    const RECONNECTS: [usize; 6] = [0, 7, 1, 1, 0, 0];
    assert_eq!(cases.len(), 6);
    assert_eq!(sources.len(), 6);
    let mut checked = Vec::new();
    for (i, case) in cases.iter().enumerate() {
        assert_eq!(case["name"], NAMES[i]);
        assert_eq!(sources[i]["name"], NAMES[i]);
        if case["family"] != family {
            continue;
        }
        let trace = run(case);
        native_assertions(
            NAMES[i],
            &trace,
            &sources[i],
            SNAPSHOTS[i],
            REQUESTS[i],
            RECONNECTS[i],
        );
        checked.push((i, trace));
    }
    assert_eq!(
        checked.len(),
        match family {
            "refresh" => 3,
            "watchdog" => 1,
            "retry" => 2,
            _ => panic!("family"),
        }
    );
    // Every native state/argument/result assertion in this family precedes logs.
    for (i, trace) in checked {
        let source_logs: Vec<(String, String)> = sources[i]["logs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                (
                    v["severity"].as_str().unwrap().to_owned(),
                    v["message"].as_str().unwrap().to_owned(),
                )
            })
            .collect();
        assert_eq!(trace.logs, source_logs, "{} emitted logs", NAMES[i]);
    }
}
#[test]
#[ignore = "requires live TypeScript compiler"]
fn listen_key_refresh_matches_source() {
    check_family("refresh");
}
#[test]
#[ignore = "requires live TypeScript compiler"]
fn listen_key_watchdog_matches_source() {
    check_family("watchdog");
}
#[test]
#[ignore = "requires live TypeScript compiler"]
fn listen_key_connect_retry_matches_source() {
    check_family("retry");
}
#[test]
fn owner_cancellation_blocks_listen_key_work() {
    let f = fixture();
    f.lifecycle.start_listen_key_watchdog().unwrap();
    f.lifecycle.schedule_connect_retry().unwrap();
    let fetching = f.lifecycle.fetch_listen_key();
    launch(
        &f,
        Box::pin(async move {
            fetching.await?;
            Ok(())
        }),
    );
    f.runtime.pump();
    assert_eq!(f.parent.requests.borrow().len(), 1);
    assert_eq!(f.runtime.timers(), 2);
    f.base.dispose().unwrap();
    resolve(&f.parent.post, &json!({}), "late-key".to_owned());
    f.runtime.mono.set(10_000_000.0);
    f.runtime.pump();
    assert_eq!(f.lifecycle.state().listen_key, "");
    assert_eq!(f.runtime.timers(), 0);
    assert_eq!(f.parent.requests.borrow().len(), 1);
    launch(&f, f.lifecycle.update_listen_key());
    f.lifecycle.schedule_connect_retry().unwrap();
    f.lifecycle.start_listen_key_watchdog().unwrap();
    f.runtime.pump();
    assert_eq!(f.parent.requests.borrow().len(), 1);
    assert_eq!(f.parent.reconnects.get(), 0);
    f.lifecycle.clear_listen_key_watchdog_timer();
    f.lifecycle.clear_connect_retry_timer();
    assert!(!f.lifecycle.state().watchdog_timer);
    assert!(!f.lifecycle.state().connect_timer);
}
