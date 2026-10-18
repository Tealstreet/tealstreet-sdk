#[path = "support/retained_info.rs"]
mod retained_info;
use safe_cex_venue_coinbase::{
    portfolios_balances::*,
    starbase_auth::{CoinbaseStarbasePromise as Promise, CoinbaseStarbasePromiseResolver},
};
#[path = "support/microtasks.rs"]
mod microtasks;
use microtasks::{Microtasks, completed, is_pending};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    process::Command,
    rc::Rc,
};
const FIXTURE: &str = include_str!("fixtures/portfolios-balances.json");
const HASH: &str = "1bc2ff2a0dfbc6c7234bc561622e13408a77fcceca3b8378e90347c50dacd55b";
type GetHook = Rc<dyn Fn(&str)>;
struct Host {
    on_get: RefCell<Option<GetHook>>,
    events: RefCell<Vec<&'static str>>,
    replies: RefCell<VecDeque<Promise<Result<PortfolioReply, PortfolioFailure>>>>,
    calls: RefCell<Vec<(&'static str, Option<String>)>>,
    emits: RefCell<Vec<Vec<Option<Value>>>>,
    cached: Balance,
    upnls: Vec<f64>,
    supported: Option<Vec<String>>,
}
impl PortfolioHost for Host {
    fn get(
        &self,
        endpoint: &'static str,
        uuid: Option<String>,
    ) -> Promise<Result<PortfolioReply, PortfolioFailure>> {
        self.events.borrow_mut().push("get");
        self.calls.borrow_mut().push((endpoint, uuid));
        let hook = self.on_get.borrow().clone();
        if let Some(hook) = hook {
            hook(endpoint);
        }
        self.replies
            .borrow_mut()
            .pop_front()
            .expect("independent transport plan")
    }
    fn emit_and_log_error(&self, args: &[Option<Value>]) -> Result<(), PortfolioFailure> {
        self.events.borrow_mut().push("emit");
        self.emits.borrow_mut().push(args.to_vec());
        Ok(())
    }
    fn cached_balance(&self) -> Balance {
        self.cached.clone()
    }
    fn position_upnls(&self) -> Vec<f64> {
        self.upnls.clone()
    }
    fn supported_types_override(
        &self,
    ) -> Option<Promise<Result<SupportedTypes, PortfolioFailure>>> {
        self.supported
            .as_ref()
            .map(|v| Promise::resolved(Ok(Rc::new(RefCell::new(v.clone())))))
    }
}
fn portfolio(v: &Value) -> Portfolio {
    Rc::new(RefCell::new(
        serde_json::from_value::<PortfolioState>(v.clone()).unwrap(),
    ))
}
fn thrown(v: Value) -> PortfolioFailure {
    Rc::new(PortfolioThrown::from_value(Some(v)))
}
fn cached() -> Balance {
    let mut b = PortfolioBalance {
        total: 11.0,
        free: 9.0,
        used: 2.0,
        upnl: 4.0,
        ..Default::default()
    };
    let mut currencies = PortfolioCurrencies::default();
    currencies.own.insert(
        "cached".into(),
        Rc::new(RefCell::new(PortfolioBalance {
            total: 7.0,
            free: 6.0,
            used: 1.0,
            ..Default::default()
        })),
    );
    b.has_currencies = true;
    b.currencies = Some(Rc::new(RefCell::new(currencies)));
    b.extra.insert(
        "info".into(),
        Rc::new(RefCell::new(json!({"retained":23}).into())),
    );
    b.extra
        .insert("primitive".into(), Rc::new(RefCell::new(json!(5).into())));
    Rc::new(RefCell::new(b))
}
fn receiver(
    input: &Value,
    tasks: &Microtasks,
) -> (
    Rc<PortfolioReceiver<Host>>,
    Portfolios,
    Vec<PortfolioFailure>,
) {
    let portfolios = Rc::new(RefCell::new(
        input["portfolios"]
            .as_array()
            .map(|v| v.iter().map(portfolio).collect())
            .unwrap_or_default(),
    ));
    let mut errors = vec![];
    let replies = input["replies"]
        .as_array()
        .map(|v| {
            v.iter()
                .map(|p| {
                    Promise::resolved(if let Some(err) = p.get("error") {
                        let e = thrown(err.clone());
                        errors.push(e.clone());
                        Err(e)
                    } else {
                        Ok(PortfolioReply {
                            data: p["data"].clone().into(),
                            portfolios: Some(portfolios.clone()),
                        })
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    (
        Rc::new(PortfolioReceiver {
            host: Host {
                on_get: RefCell::new(None),
                events: RefCell::new(vec![]),
                replies: RefCell::new(replies),
                calls: RefCell::new(vec![]),
                emits: RefCell::new(vec![]),
                cached: {
                    let b = cached();
                    if input["noCurrencies"].as_bool() == Some(true) {
                        b.borrow_mut().currencies = None;
                        b.borrow_mut().has_currencies = false;
                    }
                    b
                },
                upnls: input["upnls"]
                    .as_array()
                    .map(|v| v.iter().map(|v| v.as_f64().unwrap()).collect())
                    .unwrap_or_default(),
                supported: input["override"]
                    .as_array()
                    .map(|v| v.iter().map(|v| v.as_str().unwrap().into()).collect()),
            },
            spawn: tasks.spawner(),
            selected: Rc::new(RefCell::new(input.get("selected").map(portfolio))),
            public_only: Cell::new(input["publicOnly"].as_bool().unwrap_or(false)),
            support_spot: Cell::new(input["supportSpot"].as_bool().unwrap_or(false)),
            spot: matches!(input["op"].as_str(), Some("spotFind" | "spotTypes")),
            class: None,
        }),
        portfolios,
        errors,
    )
}
fn assert_number(actual: f64, expected: &Value, label: &str) {
    let bits = u64::from_str_radix(
        expected["$bits"]
            .as_str()
            .expect("source numeric expectation"),
        16,
    )
    .unwrap();
    if f64::from_bits(bits).is_nan() {
        assert!(actual.is_nan(), "{label}: native NaN");
    } else {
        assert_eq!(actual.to_bits(), bits, "{label}: native number bits");
    }
}
fn assert_json(actual: Option<&Value>, expected: &Value, label: &str) {
    if expected.get("$undefined") == Some(&Value::Bool(true)) {
        assert!(actual.is_none(), "{label}: native undefined");
        return;
    }
    let actual = actual.expect("expected native value");
    if expected.get("$bits").is_some() {
        assert_number(actual.as_f64().unwrap(), expected, label);
    } else {
        match expected {
            Value::Array(items) => {
                let a = actual.as_array().unwrap();
                assert_eq!(a.len(), items.len(), "{label}");
                for (a, e) in a.iter().zip(items) {
                    assert_json(Some(a), e, label);
                }
            }
            Value::Object(fields) => {
                assert_eq!(actual.as_object().unwrap().len(), fields.len(), "{label}");
                for (k, e) in fields {
                    assert_json(actual.get(k), e, label);
                }
            }
            _ => assert_eq!(actual, expected, "{label}"),
        }
    }
}
fn assert_portfolio(actual: &Portfolio, expected: &Value, label: &str) {
    let p = actual.borrow();
    assert_eq!(
        p.metadata[&"name".encode_utf16().collect::<Vec<_>>()],
        expected["name"].clone().into(),
        "{label}"
    );
    assert_eq!(p.uuid, expected["uuid"], "{label}");
    assert_eq!(p.r#type, expected["type"], "{label}: native portfolio type");
    assert_eq!(
        p.metadata[&"deleted".encode_utf16().collect::<Vec<_>>()],
        expected["deleted"].clone().into(),
        "{label}"
    );
}
fn assert_balance(actual: &Balance, e: &Value, label: &str) {
    let b = actual.borrow();
    assert_number(b.total, &e["total"], label);
    assert_number(b.free, &e["free"], label);
    assert_number(b.used, &e["used"], label);
    assert_number(b.upnl, &e["upnl"], label);
    if let Some(currencies) = &b.currencies {
        assert_currencies(currencies, &e["currencies"], label);
    } else {
        if b.has_currencies {
            assert_json(None, &e["currencies"], label);
        } else {
            assert!(
                e.get("currencies").is_none(),
                "{label}: balance currency presence"
            );
        }
    }
    for (k, v) in &b.extra {
        retained_info::assert_info(Some(&v.borrow()), &e[k], label);
    }
    let expected_keys = e.as_object().unwrap().len();
    assert_eq!(
        4 + usize::from(b.has_currencies) + b.extra.len(),
        expected_keys,
        "{label}: balance own key floor"
    );
}
fn assert_currencies(actual: &Currencies, e: &Value, label: &str) {
    let c = actual.borrow();
    let keys: Vec<_> = c.own.keys().map(String::as_str).collect();
    let expected: Vec<_> = e.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(keys, expected, "{label}: native currency own keys");
    for (k, b) in &*c.own {
        assert_balance(b, &e[k], label);
    }
}
#[test]
fn portfolio_balance_live_native_corpus() {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE)),
        HASH,
        "independent portfolio corpus content"
    );
    let fixture: Vec<Value> = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(fixture.len(), 47, "independent corpus floor");
    let oracle = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/portfolios-balances.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        oracle.status.success(),
        "live source oracle: {}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&oracle.stdout).unwrap();
    assert_eq!(expected.len(), 47, "independent oracle floor");
    for (input, e) in fixture.iter().zip(&expected) {
        let label = input["label"].as_str().unwrap();
        let op = input["op"].as_str().unwrap();
        let tasks = Microtasks::default();
        let (r, portfolios, errors) = receiver(input, &tasks);
        let mut failure = None;
        match op {
            "predicate" => {
                match should_rethrow_portfolio_probe_error(&thrown(input["error"].clone())) {
                    Ok(v) => assert_eq!(v, e["result"], "{label}: native probe predicate"),
                    Err(err) => failure = Some(err),
                }
            }
            "fetchPortfolios" => {
                let pending = fetch_portfolios(r.clone());
                tasks.drain();
                match completed(pending) {
                    Ok(list) => {
                        assert_eq!(
                            list.borrow().len(),
                            e["result"].as_array().unwrap().len(),
                            "{label}"
                        );
                        for (p, e) in list.borrow().iter().zip(e["result"].as_array().unwrap()) {
                            assert_portfolio(p, e, label);
                        }
                        assert_eq!(
                            Rc::ptr_eq(&list, &portfolios),
                            e["identity"]["list"].as_bool().unwrap(),
                            "{label}: list identity"
                        );
                    }
                    Err(err) => failure = Some(err),
                }
            }
            "findSelectedPortfolio" | "spotFind" => {
                let pending = if op == "spotFind" {
                    find_selected_spot_portfolio(r.clone())
                } else {
                    find_selected_portfolio(r.clone())
                };
                tasks.drain();
                match completed(pending) {
                    Ok(p) => {
                        assert_portfolio(&p, &e["result"], label);
                        let index = portfolios
                            .borrow()
                            .iter()
                            .position(|v| Rc::ptr_eq(v, &p))
                            .unwrap();
                        assert_eq!(
                            index as i64,
                            e["identity"]["portfolio"].as_i64().unwrap(),
                            "{label}: native selected identity"
                        );
                    }
                    Err(err) => failure = Some(err),
                }
            }
            "getSupportedTypes" | "spotTypes" => {
                let pending = if op == "spotTypes" {
                    get_spot_supported_types(r.clone())
                } else {
                    get_supported_types(r.clone())
                };
                tasks.drain();
                match completed(pending) {
                    Ok(v) => assert_eq!(
                        *v.borrow(),
                        e["result"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|v| v.as_str().unwrap())
                            .collect::<Vec<_>>(),
                        "{label}: native supported types"
                    ),
                    Err(err) => failure = Some(err),
                }
            }
            "fetchSpotCurrencies" => {
                let pending = fetch_spot_currencies(r.clone());
                tasks.drain();
                match completed(pending) {
                    Ok(Some(v)) => {
                        assert_currencies(&v, &e["result"], label);
                        assert_eq!(
                            Rc::ptr_eq(&v, r.host.cached.borrow().currencies.as_ref().unwrap()),
                            e["identity"]["currencies"].as_bool().unwrap(),
                            "{label}: currency fallback identity"
                        );
                        let proto = v.borrow().prototype.clone();
                        if let Some(p) = proto {
                            assert_balance(&p, &e["prototype"], label);
                        } else {
                            assert!(e["prototype"].is_null(), "{label}");
                        }
                    }
                    Ok(None) => assert_json(None, &e["result"], label),
                    Err(err) => failure = Some(err),
                }
            }
            "fetchBalance" | "fetchBalanceCfm" | "fetchBalanceIntx" => {
                let pending = match op {
                    "fetchBalance" => fetch_balance(r.clone()),
                    "fetchBalanceCfm" => fetch_balance_cfm(r.clone()),
                    _ => fetch_balance_intx(r.clone()),
                };
                tasks.drain();
                match completed(pending) {
                    Ok(v) => {
                        assert_balance(&v, &e["result"], label);
                        assert_eq!(
                            Rc::ptr_eq(&v, &r.host.cached),
                            e["identity"]["cached"].as_bool().unwrap(),
                            "{label}: balance fallback identity"
                        );
                        if v.borrow().extra.contains_key("info") {
                            assert!(
                                Rc::ptr_eq(
                                    &v.borrow().extra["info"],
                                    &r.host.cached.borrow().extra["info"]
                                ),
                                "{label}: shallow info alias"
                            );
                            if !Rc::ptr_eq(&v, &r.host.cached) {
                                assert!(
                                    !Rc::ptr_eq(
                                        &v.borrow().extra["primitive"],
                                        &r.host.cached.borrow().extra["primitive"]
                                    ),
                                    "{label}: primitive fields shallow-copy by value"
                                );
                            }
                        }
                    }
                    Err(err) => failure = Some(err),
                }
            }
            _ => panic!("unknown operation"),
        }
        if let Some(err) = failure {
            assert_json(err.value.as_ref(), &e["error"], label);
            let index = errors
                .iter()
                .position(|v| Rc::ptr_eq(v, &err))
                .map(|i| i as i64)
                .unwrap_or(-1);
            assert_eq!(
                index,
                e["identity"]["error"].as_i64().unwrap(),
                "{label}: native original thrown root"
            );
        } else {
            assert!(e["error"].is_null(), "{label}: unexpected success");
        }
        if let Some(p) = r.selected.borrow().as_ref() {
            assert_portfolio(p, &e["selected"], label);
        } else {
            assert_json(None, &e["selected"], label);
        }
        for (p, e) in portfolios
            .borrow()
            .iter()
            .zip(e["portfolios"].as_array().unwrap_or(&vec![]))
        {
            assert_portfolio(p, e, label);
        }
        let trace = e["trace"].as_array().unwrap();
        assert_eq!(
            *r.host.events.borrow(),
            trace
                .iter()
                .map(|v| if v.get("endpoint").is_some() {
                    "get"
                } else {
                    "emit"
                })
                .collect::<Vec<_>>(),
            "{label}: native invocation order"
        );
        let expected_calls: Vec<_> = trace
            .iter()
            .filter(|v| v.get("endpoint").is_some())
            .collect();
        assert_eq!(
            r.host.calls.borrow().len(),
            expected_calls.len(),
            "{label}: native request count"
        );
        for ((endpoint, uuid), e) in r.host.calls.borrow().iter().zip(expected_calls) {
            assert_eq!(*endpoint, e["endpoint"], "{label}: native endpoint");
            assert_json(
                uuid.as_ref().map(|v| Value::String(v.clone())).as_ref(),
                &e["uuid"],
                label,
            );
        }
        let expected_emits: Vec<_> = trace.iter().filter_map(|v| v.get("emit")).collect();
        assert_eq!(
            r.host.emits.borrow().len(),
            expected_emits.len(),
            "{label}: native emit count"
        );
        for (args, e) in r.host.emits.borrow().iter().zip(expected_emits) {
            assert_eq!(
                args.len(),
                e.as_array().unwrap().len(),
                "{label}: native error argument count"
            );
            for (a, e) in args.iter().zip(e.as_array().unwrap()) {
                assert_json(a.as_ref(), e, label);
            }
        }
    }
}
#[test]
fn base_probe_waits_all_and_selects_input_order() {
    let tasks = Microtasks::default();
    let input = json!({"op":"findSelectedPortfolio","portfolios":[{"name":"a","uuid":"a","type":"INTX","deleted":false},{"name":"b","uuid":"b","type":"INTX","deleted":false}],"replies":[{"data":{}}]});
    let (r, ps, _) = receiver(&input, &tasks);
    let (a, ar) = Promise::pending();
    let (b, br) = Promise::pending();
    r.host.replies.borrow_mut().extend([a, b]);
    let mut pending = find_selected_portfolio(r.clone());
    tasks.drain();
    assert_eq!(
        r.host.calls.borrow().len(),
        3,
        "base concurrent probes launch"
    );
    br.resolve(Ok(PortfolioReply {
        data: json!({}).into(),
        portfolios: None,
    }));
    tasks.drain();
    assert!(is_pending(&mut pending), "base waits every probe");
    tasks.enqueue(move || {
        ar.resolve(Ok(PortfolioReply {
            data: json!({}).into(),
            portfolios: None,
        }))
    });
    tasks.drain();
    let selected = completed(pending).unwrap();
    assert!(
        Rc::ptr_eq(&selected, &ps.borrow()[0]),
        "base selects first input despite reverse completion"
    );
}
#[test]
fn spot_probes_are_sequential_and_hot_without_await() {
    let tasks = Microtasks::default();
    let input = json!({"op":"spotFind","portfolios":[{"name":"a","uuid":"a","type":"DEFAULT","deleted":false},{"name":"b","uuid":"b","type":"DEFAULT","deleted":false}],"replies":[{"data":{}}]});
    let (r, _, _) = receiver(&input, &tasks);
    let (a, ar): (
        _,
        CoinbaseStarbasePromiseResolver<Result<PortfolioReply, PortfolioFailure>>,
    ) = Promise::pending();
    let (b, br) = Promise::pending();
    r.host.replies.borrow_mut().extend([a, b]);
    drop(find_selected_spot_portfolio(r.clone()));
    tasks.drain();
    assert_eq!(
        r.host.calls.borrow().len(),
        2,
        "spot only first probe launches"
    );
    ar.resolve(Err(thrown(json!({"response":{"status":403}}))));
    tasks.drain();
    assert_eq!(
        r.host.calls.borrow().len(),
        3,
        "spot later probe starts after masked error even unawaited"
    );
    br.resolve(Ok(PortfolioReply {
        data: json!({}).into(),
        portfolios: None,
    }));
    tasks.drain();
    assert!(
        r.host.emits.borrow().is_empty(),
        "spot masked permission has no emit"
    );
}

#[test]
fn base_probe_callback_can_mutate_live_portfolios() {
    let tasks = Microtasks::default();
    let input = json!({"op":"findSelectedPortfolio","portfolios":[{"name":"a","uuid":"a","type":"INTX","deleted":false},{"name":"b","uuid":"b","type":"INTX","deleted":false}],"replies":[{"data":{}},{"data":{}},{"data":{}}]});
    let (r, ps, _) = receiver(&input, &tasks);
    let once = Cell::new(false);
    let list = ps.clone();
    *r.host.on_get.borrow_mut() = Some(Rc::new(move |endpoint| {
        if endpoint.ends_with("/{portfolio_uuid}") && !once.replace(true) {
            let first = list.borrow()[0].clone();
            first.borrow_mut().uuid = "mut-a".into();
            list.borrow()[1].borrow_mut().uuid = "mut-b".into();
            list.borrow_mut().push(portfolio(
                &json!({"name":"extra","uuid":"extra","type":"INTX","deleted":false}),
            ));
        }
    }));
    let promise = find_selected_portfolio(r.clone());
    tasks.drain();
    let selected = completed(promise).unwrap();
    assert!(
        Rc::ptr_eq(&selected, &ps.borrow()[0]),
        "callback mutation retains selected root"
    );
    assert_eq!(
        r.host.calls.borrow().len(),
        3,
        "map snapshots initial length before eager mutation"
    );
    assert_eq!(
        r.host.calls.borrow()[1].1.as_deref(),
        Some("a"),
        "current UUID is read before callback"
    );
    assert_eq!(
        r.host.calls.borrow()[2].1.as_deref(),
        Some("mut-b"),
        "later UUID is read after callback"
    );
}

#[test]
fn supported_types_promise_retains_one_mutable_native_array() {
    let source = source_class_policy();
    for (source_index, mode, initial) in [
        (9, "base", vec!["cfm", "spot"]),
        (11, "public-only", vec!["intx"]),
        (12, "spot", vec!["spot"]),
    ] {
        let expected = &source["cases"][source_index];
        assert_eq!(expected["samePromiseValue"], true);
        assert_eq!(expected["distinctInvocation"], true);
        let tasks = Microtasks::default();
        let (receiver, _, _) = receiver(
            &json!({"selected":{"name":"named","uuid":"cfm","type":"CFM","deleted":false},"supportSpot":true,"publicOnly":mode=="public-only","op":if mode=="spot" {"spotTypes"}else{"getSupportedTypes"}}),
            &tasks,
        );
        let invoke = {
            let receiver = receiver.clone();
            move || {
                if mode == "spot" {
                    get_spot_supported_types(receiver.clone())
                } else {
                    get_supported_types(receiver.clone())
                }
            }
        };
        let promise = invoke();
        let first = completed(promise.clone()).unwrap();
        assert_eq!(
            *first.borrow(),
            initial,
            "{mode}: first native supported types value"
        );
        first.borrow_mut().push("intx".into());
        let second = completed(promise).unwrap();
        assert!(
            Rc::ptr_eq(&first, &second),
            "{mode}: same supported-types Promise retains one native array root"
        );
        let mut changed = initial.clone();
        changed.push("intx");
        assert_eq!(
            *second.borrow(),
            changed,
            "{mode}: same supported-types Promise retains the native array mutation"
        );
        let fresh = completed(invoke()).unwrap();
        assert!(
            !Rc::ptr_eq(&fresh, &first),
            "{mode}: new supported-types invocation allocates a distinct native root"
        );
        assert_eq!(*fresh.borrow(), initial);
        assert_eq!(
            receiver.host.calls.borrow().len(),
            0,
            "{mode}: no imported HTTP for retained selection"
        );
        // Compare decoded source values only after all native identity/value assertions.
        assert_eq!(
            *second.borrow(),
            expected["second"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            *fresh.borrow(),
            expected["fresh"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>()
        );
    }
}

fn source_class_policy() -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/live-class-policy.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "live class policy source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "no-credentials",
            "key-without-signer",
            "local-selection-and-account",
            "remote-signer",
            "replace-options-slot",
            "live-support-spot",
            "spot-override-and-captured-base-field",
            "signed-captured-base-calls-spot-finder",
            "retained-base-finder-versus-current-spot",
            "supported-types-promise-retains-array-root",
            "account-promise-retains-object-root",
            "public-only-promise-retains-array-root",
            "spot-promise-retains-array-root",
            "live-policy-after-pending-selection"
        ],
        "independent source policy content floor"
    );
    source
}
