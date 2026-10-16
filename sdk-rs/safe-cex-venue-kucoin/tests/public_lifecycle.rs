use safe_cex_venue_kucoin::{public_lifecycle::*, ws_helpers::KucoinPublicRole};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
};
#[path = "support/public_open.rs"]
mod support;
use support::error;
struct Market {
    tag: usize,
    id: Option<Vec<u16>>,
    symbol: &'static str,
}
fn market(tag: usize, id: Option<Vec<u16>>, symbol: &'static str) -> Option<Rc<Market>> {
    Some(Rc::new(Market { tag, id, symbol }))
}
fn added() -> Vec<Option<Rc<Market>>> {
    vec![
        market(0, Some(vec![65]), "ignored"),
        market(1, Some(vec![]), "fallback"),
        market(2, Some(vec![65]), "ignored"),
        market(3, Some(vec![0xd800, 66]), "ignored"),
        market(4, Some(vec![79, 70, 70]), "ignored"),
        market(5, None, "empty"),
        None,
    ]
}
struct Additions<'a> {
    slots: &'a RefCell<Vec<Option<Rc<Market>>>>,
    scenario: &'static str,
    error: &'a support::Error,
    events: &'a RefCell<Vec<String>>,
}
impl MarketAdditions<Market, support::Error> for Additions<'_> {
    fn empty(&self) -> Result<bool, support::Error> {
        if self.scenario == "length-error" {
            return Err(self.error.clone());
        }
        Ok(self.slots.borrow().is_empty())
    }
    fn map_length(&self) -> Result<usize, support::Error> {
        Ok(self.slots.borrow().len())
    }
    fn at(&self, index: usize) -> Result<Option<Rc<Market>>, support::Error> {
        if self.scenario == "index-error" && index == 0 {
            return Err(self.error.clone());
        }
        if self.scenario == "index-getter" && index == 1 {
            self.events.borrow_mut().push("index:1".into());
            self.slots.borrow_mut()[2] = market(8, Some(vec![66]), "ignored");
        }
        let current = self.slots.borrow().get(index).cloned().flatten();
        if current.is_none() && self.scenario == "inherited" && index == 6 {
            return Ok(market(9, Some(vec![66]), "ignored"));
        }
        Ok(current)
    }
}

#[test]
fn public_lifecycle_native_before_source() {
    let (futures, native_futures) = support::open_cases(
        "kucoin",
        &[
            "subscribe",
            "ping",
            "emitter.off:markets:changed",
            "emitter.on:markets:changed",
        ],
        |disposed, invoke, enabled| on_open_futures(disposed, invoke, enabled),
    );
    let (spot, native_spot) = support::open_cases(
        "kucoinspot",
        &["subscribe", "ping"],
        |disposed, invoke, enabled| on_open_spot(disposed, invoke, enabled),
    );
    let mut lookup = vec![];
    let mut native_lookup = vec![];
    for symbol in [
        None,
        Some(vec![]),
        Some(vec![65, 66]),
        Some(vec![0xd800, 65, 0xdc00]),
    ] {
        for present in [false, true] {
            for fail in [false, true] {
                let error = error();
                let calls = Cell::new(0);
                let wanted = Rc::new(17);
                let result = ticker_for_api_symbol(symbol.as_deref(), |actual| {
                    calls.set(calls.get() + 1);
                    assert_eq!(
                        actual,
                        symbol.as_ref().unwrap(),
                        "KU-PUBLIC native raw lookup symbol units"
                    );
                    if fail {
                        Err(error.clone())
                    } else {
                        Ok(present.then(|| wanted.clone()))
                    }
                });
                let should_lookup = symbol.as_ref().is_some_and(|s| !s.is_empty());
                assert_eq!(
                    calls.get(),
                    usize::from(should_lookup),
                    "KU-PUBLIC native lazy lookup count"
                );
                assert_eq!(
                    result.is_err(),
                    should_lookup && fail,
                    "native ticker lookup error branch"
                );
                let outcome = match result {
                    Err(actual) => {
                        assert!(
                            Rc::ptr_eq(&actual, &error),
                            "native ticker lookup error pointer"
                        );
                        assert_eq!(actual, error, "native complete ticker lookup error");
                        json!({"error":{"name":actual.name,"message":actual.message,"same":true}})
                    }
                    Ok(actual) => {
                        assert_eq!(
                            actual.is_some(),
                            should_lookup && present,
                            "KU-PUBLIC native ticker presence"
                        );
                        if let Some(actual) = &actual {
                            assert!(Rc::ptr_eq(actual, &wanted), "native ticker pointer");
                            assert_eq!(**actual, 17, "native ticker pointee");
                        }
                        json!({"value":actual.map(|v|*v)})
                    }
                };
                lookup.push(json!({"symbol":symbol,"present":present,"fail":fail}));
                native_lookup.push(json!({"calls":calls.get(),"outcome":outcome}));
            }
        }
    }
    assert_eq!(lookup.len(), 16, "declared ticker lookup floor");
    let scenarios = [
        "normal",
        "empty",
        "no-ws",
        "disposed",
        "disconnected",
        "detail",
        "shard-error",
        "map-error",
        "api-error",
        "send-error",
        "shard-append",
        "shard-clear",
        "map-append",
        "map-delete",
        "map-replace",
        "shard-flip",
        "connected-dispose",
        "connected-role",
        "ws-error",
        "connected-error",
        "ws-clear",
        "inherited",
        "index-getter",
        "index-error",
        "length-error",
    ];
    let mut changes = vec![];
    let mut native_changes = vec![];
    for scenario in scenarios {
        let added = RefCell::new(if scenario == "empty" { vec![] } else { added() });
        let disposed = Cell::new(scenario == "disposed");
        let role = Cell::new(if scenario == "detail" {
            KucoinPublicRole::Detail
        } else {
            KucoinPublicRole::Snapshot
        });
        let error = error();
        let events = RefCell::new(vec![]);
        let sent = RefCell::new(vec![]);
        let result = on_markets_changed(
            &Additions {
                slots: &added,
                scenario,
                error: &error,
                events: &events,
            },
            || {
                events.borrow_mut().push("ws".to_owned());
                if scenario == "ws-error" {
                    return Err(error.clone());
                }
                if scenario == "ws-clear" {
                    added.borrow_mut().clear();
                }
                Ok(scenario != "no-ws")
            },
            &disposed,
            || {
                events.borrow_mut().push("connected".to_owned());
                if scenario == "connected-error" {
                    return Err(error.clone());
                }
                if scenario == "connected-dispose" {
                    disposed.set(true);
                }
                if scenario == "connected-role" {
                    role.set(KucoinPublicRole::Detail);
                }
                Ok(scenario != "disconnected")
            },
            &role,
            || {
                events.borrow_mut().push("shard".to_owned());
                if scenario == "shard-error" {
                    return Err(error.clone());
                }
                if scenario == "shard-append" {
                    added
                        .borrow_mut()
                        .push(market(7, Some(vec![65]), "ignored"));
                }
                if scenario == "shard-clear" {
                    added.borrow_mut().clear();
                }
                if scenario == "shard-flip" {
                    disposed.set(true);
                    role.set(KucoinPublicRole::Detail);
                }
                Ok(vec![vec![65], vec![66], vec![0xd800, 66], vec![65]])
            },
            |m| {
                events.borrow_mut().push(format!("market:{}", m.tag));
                if scenario == "map-error" && m.tag == 0 {
                    return Err(error.clone());
                }
                if m.tag == 0 {
                    if scenario == "map-append" {
                        added
                            .borrow_mut()
                            .push(market(7, Some(vec![65]), "ignored"));
                    }
                    if scenario == "map-delete" {
                        added.borrow_mut()[1] = None;
                    }
                    if scenario == "map-replace" {
                        added.borrow_mut()[2] = market(8, Some(vec![66]), "ignored");
                    }
                }
                if let Some(id) = &m.id
                    && !id.is_empty()
                {
                    return Ok(id.clone());
                }
                events.borrow_mut().push(format!("api:{}", m.tag));
                if scenario == "api-error" && m.tag == 1 {
                    return Err(error.clone());
                }
                Ok(if m.symbol == "fallback" {
                    vec![66]
                } else {
                    vec![]
                })
            },
            |operation, channels, symbols| {
                assert_eq!(operation, "subscribe", "KU-MARKETS native operation");
                assert_eq!(
                    channels,
                    ["/contractMarket/snapshot"],
                    "KU-MARKETS native channel"
                );
                let expected = match scenario {
                    "map-delete" => vec![vec![65], vec![65], vec![0xd800, 66]],
                    "map-replace" | "index-getter" => {
                        vec![vec![65], vec![66], vec![66], vec![0xd800, 66]]
                    }
                    "inherited" => vec![vec![65], vec![66], vec![65], vec![0xd800, 66], vec![66]],
                    "shard-append" => {
                        vec![vec![65], vec![66], vec![65], vec![0xd800, 66], vec![65]]
                    }
                    _ => vec![vec![65], vec![66], vec![65], vec![0xd800, 66]],
                };
                assert_eq!(
                    symbols, expected,
                    "KU-MARKETS native owned symbols/order/duplicates"
                );
                events.borrow_mut().push("send".to_owned());
                sent.borrow_mut()
                    .push(json!({"operation":operation,"channels":channels,"symbols":symbols}));
                if scenario == "send-error" {
                    Err(error.clone())
                } else {
                    Ok(())
                }
            },
        );
        let expected_events: &[&str] = match scenario {
            "empty" | "length-error" => &[],
            "no-ws" | "disposed" | "ws-error" => &["ws"],
            "disconnected" | "detail" | "connected-role" | "connected-error" => {
                &["ws", "connected"]
            }
            "shard-error" | "shard-clear" | "ws-clear" | "index-error" => {
                &["ws", "connected", "shard"]
            }
            "map-error" => &["ws", "connected", "shard", "market:0"],
            "api-error" => &["ws", "connected", "shard", "market:0", "market:1", "api:1"],
            "map-delete" => &[
                "ws",
                "connected",
                "shard",
                "market:0",
                "market:2",
                "market:3",
                "market:4",
                "market:5",
                "api:5",
                "send",
            ],
            "map-replace" => &[
                "ws",
                "connected",
                "shard",
                "market:0",
                "market:1",
                "api:1",
                "market:8",
                "market:3",
                "market:4",
                "market:5",
                "api:5",
                "send",
            ],
            "index-getter" => &[
                "ws",
                "connected",
                "shard",
                "market:0",
                "index:1",
                "market:1",
                "api:1",
                "market:8",
                "market:3",
                "market:4",
                "market:5",
                "api:5",
                "send",
            ],
            "inherited" => &[
                "ws",
                "connected",
                "shard",
                "market:0",
                "market:1",
                "api:1",
                "market:2",
                "market:3",
                "market:4",
                "market:5",
                "api:5",
                "market:9",
                "send",
            ],
            "shard-append" => &[
                "ws",
                "connected",
                "shard",
                "market:0",
                "market:1",
                "api:1",
                "market:2",
                "market:3",
                "market:4",
                "market:5",
                "api:5",
                "market:7",
                "send",
            ],
            _ => &[
                "ws",
                "connected",
                "shard",
                "market:0",
                "market:1",
                "api:1",
                "market:2",
                "market:3",
                "market:4",
                "market:5",
                "api:5",
                "send",
            ],
        };
        assert_eq!(
            *events.borrow(),
            expected_events,
            "KU-MARKETS native guards/live map order {scenario}"
        );
        let should_send = matches!(
            scenario,
            "normal"
                | "send-error"
                | "shard-append"
                | "map-append"
                | "map-delete"
                | "map-replace"
                | "shard-flip"
                | "connected-dispose"
                | "inherited"
                | "index-getter"
        );
        assert_eq!(
            sent.borrow().len(),
            usize::from(should_send),
            "KU-MARKETS native send count {scenario}"
        );
        let rejected = scenario.ends_with("error");
        assert_eq!(result.is_err(), rejected, "native market-change rejection");
        if let Err(actual) = &result {
            assert!(Rc::ptr_eq(actual, &error), "native market error pointer");
            assert_eq!(actual, &error, "native complete market error");
        }
        changes.push(json!({"scenario":scenario}));
        native_changes.push(json!({"events":*events.borrow(),"sent":*sent.borrow(),"error":result.err().map(|e|json!({"name":e.name,"message":e.message,"same":true}))}));
    }
    assert_eq!(changes.len(), 25, "declared market-change floor");
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-lifecycle.cjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            &serde_json::to_vec(
                &json!({"futures":futures,"spot":spot,"lookup":lookup,"changes":changes}),
            )
            .unwrap(),
        )
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source,
        json!({"futures":native_futures,"spot":native_spot,"lookup":native_lookup,"changes":native_changes}),
        "live source after all native values/pointers"
    );
}
