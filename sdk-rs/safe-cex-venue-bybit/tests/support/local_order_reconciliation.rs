use super::*;
use safe_cex_venue_bybit::local_order_reconciliation as reconciliation;

struct ReconciliationHost {
    host: Rc<Host>,
    scene: &'static str,
    trace: RefCell<Vec<Value>>,
    global: V,
    entry_order: V,
    store_order: V,
    fetched_orders: V,
    original: V,
    reporting: V,
    held: RefCell<Vec<V>>,
    predicate_calls: Cell<usize>,
}
macro_rules! reconciliation_forward {
    ($($name:ident($($arg:ident:$ty:ty),*)->$result:ty;)*) => {$ (
        fn $name(&self,$($arg:$ty),*)->$result { self.host.$name($($arg),*) }
    )*};
}
impl SnapshotHost for ReconciliationHost {
    type Value = V;
    type Error = V;
    reconciliation_forward! {
        get(owner:&V,key:&str)->Result<V,V>;
        optional(owner:&V,key:&str)->Result<V,V>;
        set(owner:&V,key:&str,value:V)->Result<(),V>;
        truthy(value:&V)->bool;
        equal(a:&V,b:&V)->bool;
        undefined()->V;
        literal(value:&str)->V;
        boolean(value:bool)->V;
        number(value:f64)->V;
        numeric(value:&V)->Result<f64,V>;
        object(fields:Vec<(&'static str,V)>)->V;
        array(values:Vec<V>)->V;
        values(array:&V)->Result<Vec<V>,V>;
        is_array(value:&V)->bool;
        spread(value:V,fields:Vec<(&'static str,V)>)->Result<V,V>;
        callback(owner:V,code:&'static str,captures:Vec<V>)->Result<V,V>;
        error(message:V)->Result<V,V>;
        launch(body:Operation<V,V>)->V;
        wait(value:V)->Operation<V,V>;
    }
    fn error_value(&self, error: V) -> V {
        assert!(
            Rc::ptr_eq(&error, &self.original),
            "reconciliation_native_original_caught_error"
        );
        error
    }
    fn select(&self, owner: V, key: &str) -> Result<V, V> {
        Ok(self
            .host
            .function(owner, &format!("reconcile:{key}"), vec![]))
    }
    fn apply(&self, selected: V, args: Vec<V>) -> Result<V, V> {
        let Node::Fn { owner, code, .. } = selected.as_ref() else {
            panic!("reconciliation_native_selected_callable")
        };
        let key = code.strip_prefix("reconcile:").unwrap();
        if matches!(
            key,
            "shouldReconcileLocalReduceOnlyLimitOrder"
                | "fetchFillsForOrderIds"
                | "getFilledOrderReconciliationTarget"
                | "markOrderSeen"
                | "fetchOrders"
                | "log"
        ) {
            assert!(
                Rc::ptr_eq(owner, &self.host.owner),
                "reconciliation_native_selected_owner"
            );
        }
        let mut trace = self.trace.borrow_mut();
        match key {
            "get" => {
                assert_eq!(
                    text(&args[0]),
                    text(&t("id-1")),
                    "reconciliation_native_map_id"
                );
                Ok(field(owner, "id-1"))
            }
            "find" => {
                let Node::Fn { captures, code, .. } = args[0].as_ref() else {
                    panic!("reconciliation_native_predicate")
                };
                assert_eq!(
                    code, "reconciliation_order_matches",
                    "reconciliation_native_predicate_route"
                );
                for (index, row) in items(owner).into_iter().enumerate() {
                    self.predicate_calls.set(self.predicate_calls.get() + 1);
                    let matched = truth(&reconciliation::reconciliation_order_matches(
                        self,
                        &captures[0],
                        &row,
                    )?);
                    assert_eq!(
                        matched,
                        index == 1,
                        "reconciliation_native_predicate_false_then_true"
                    );
                    if matched {
                        return Ok(row);
                    }
                }
                Ok(u())
            }
            "shouldReconcileLocalReduceOnlyLimitOrder" => {
                assert!(
                    Rc::ptr_eq(
                        &args[0],
                        if self.scene == "store-order-preferred" {
                            &self.store_order
                        } else {
                            &self.entry_order
                        }
                    ),
                    "reconciliation_native_retained_order"
                );
                trace.push(json!([
                    "eligible",
                    if self.scene == "store-order-preferred" {
                        "store"
                    } else {
                        "entry"
                    }
                ]));
                Ok(b(self.scene != "not-eligible"))
            }
            "fetchFillsForOrderIds" => {
                assert_eq!(
                    items(&args[0]).len(),
                    1,
                    "reconciliation_native_requested_id_count"
                );
                assert_eq!(
                    text(&items(&args[0])[0]),
                    text(&t("id-1")),
                    "reconciliation_native_requested_id"
                );
                trace.push(json!(["fills", ["id-1"]]));
                if matches!(
                    self.scene,
                    "fills-reject"
                        | "fills-reject-lone-message"
                        | "fills-reject-falsy"
                        | "log-throws"
                ) {
                    return Ok(settled(Err(self.original.clone())));
                }
                if matches!(self.scene, "disposed-after-fills" | "retained-target") {
                    return Ok(self.pending());
                }
                Ok(settled(Ok(obj(vec![("id-1", n(self.filled()))]))))
            }
            "getFilledOrderReconciliationTarget" => {
                assert!(
                    Rc::ptr_eq(
                        &args[0],
                        if self.scene == "store-order-preferred" {
                            &self.store_order
                        } else {
                            &self.entry_order
                        }
                    ),
                    "reconciliation_native_target_alias"
                );
                let amount = field(&args[0], "amount");
                trace.push(json!(["target", jsnum(&amount)]));
                Ok(if self.scene == "zero-target" {
                    n(0.)
                } else {
                    amount
                })
            }
            "max" => {
                let a = jsnum(&args[0]);
                let b = jsnum(&args[1]);
                Ok(n(if a.is_nan() || b.is_nan() {
                    f64::NAN
                } else {
                    a.max(b)
                }))
            }
            "markOrderSeen" => {
                assert_eq!(
                    text(&args[0]),
                    text(&t("id-1")),
                    "reconciliation_native_seen_id"
                );
                trace.push(json!(["seen", "id-1"]));
                Ok(u())
            }
            "removeOrders" => {
                assert!(
                    Rc::ptr_eq(owner, &field(&self.host.owner, "store")),
                    "reconciliation_native_remove_receiver"
                );
                let rows = items(&args[0]);
                assert_eq!(rows.len(), 1, "reconciliation_native_remove_count");
                assert_eq!(
                    text(&field(&rows[0], "id")),
                    text(&t("id-1")),
                    "reconciliation_native_removed_id"
                );
                trace.push(json!(["remove", ["id-1"]]));
                Ok(u())
            }
            "getCurrentTimeMs" => {
                trace.push(json!(["clock"]));
                Ok(n(if self.scene == "within-grace" {
                    7999.
                } else {
                    8000.
                }))
            }
            "fetchOrders" => {
                assert!(
                    args.is_empty(),
                    "reconciliation_native_fetch_orders_arguments"
                );
                trace.push(json!(["orders"]));
                Ok(if self.scene == "orders-reject" {
                    settled(Err(self.original.clone()))
                } else if self.scene == "disposed-after-orders" {
                    self.pending()
                } else {
                    settled(Ok(self.fetched_orders.clone()))
                })
            }
            "update" => {
                assert!(
                    Rc::ptr_eq(owner, &field(&self.host.owner, "store")),
                    "reconciliation_native_update_receiver"
                );
                assert!(
                    Rc::ptr_eq(&field(&args[0], "orders"), &self.fetched_orders),
                    "reconciliation_native_fetched_array_alias"
                );
                let loaded = field(&args[0], "loaded");
                assert!(
                    truth(&field(&loaded, "sentinel")),
                    "reconciliation_native_loaded_sentinel"
                );
                assert!(
                    truth(&field(&loaded, "orders")),
                    "reconciliation_native_loaded_orders"
                );
                assert!(
                    !Rc::ptr_eq(&loaded, &field(&field(&self.host.owner, "store"), "loaded")),
                    "reconciliation_native_loaded_fresh_object"
                );
                trace.push(
                    json!(["update",[{"id":"snapshot-row"}],{"sentinel":true,"orders":true}]),
                );
                Ok(u())
            }
            "log" => {
                let message = text(&args[0]);
                let expected = if self.scene == "fills-reject-falsy" {
                    "Failed to reconcile local Bybit reduce-only limit order id-1: 0"
                } else {
                    "Failed to reconcile local Bybit reduce-only limit order id-1: original REST refusal"
                };
                let mut expected = expected.encode_utf16().collect::<Vec<_>>();
                if self.scene == "fills-reject-lone-message" {
                    expected.push(0xd800);
                }
                assert_eq!(
                    message, expected,
                    "reconciliation_native_original_error_message"
                );
                assert_eq!(
                    text(&args[1]),
                    text(&t("warning")),
                    "reconciliation_native_warning_severity"
                );
                trace.push(
                    json!(["log", {"u16":message}, String::from_utf16(&text(&args[1])).unwrap()]),
                );
                if self.scene == "log-throws" {
                    Err(self.reporting.clone())
                } else {
                    Ok(u())
                }
            }
            _ => panic!("reconciliation_native_unknown_dependency {key}"),
        }
    }
    fn intrinsic(&self, name: &str, args: Vec<V>) -> Result<V, V> {
        match name {
            "globalThis" => Ok(self.global.clone()),
            "template" => {
                assert!(
                    matches!(
                        args[0].as_ref(),
                        Node::Text(_) | Node::Num(_) | Node::Bool(_) | Node::U | Node::Null
                    ),
                    "reconciliation_native_template_primitive_input"
                );
                Ok(Rc::new(Node::Text(text(&args[0]))))
            }
            "string.concat" => Ok(Rc::new(Node::Text(args.iter().flat_map(text).collect()))),
            _ => self.host.intrinsic(name, args),
        }
    }
}
fn settled(result: Result<V, V>) -> V {
    Rc::new(Node::Promise(Promise {
        state: RefCell::new(Some(result)),
        listeners: RefCell::new(vec![]),
    }))
}
impl ReconciliationHost {
    fn new(scene: &'static str) -> Rc<Self> {
        let host = Host::new(&json!({}));
        let entry_order = obj(vec![
            ("id", t("id-1")),
            ("amount", n(10.)),
            ("remaining", n(10.)),
            ("filled", n(0.)),
        ]);
        let store_order = obj(vec![
            ("id", t("id-1")),
            ("amount", n(20.)),
            ("remaining", n(20.)),
            ("filled", n(0.)),
        ]);
        let entry = obj(vec![("placedAt", n(0.)), ("order", entry_order.clone())]);
        set_field(
            &host.owner,
            "recentlyPlacedOrders",
            if scene == "missing-entry" {
                obj(vec![])
            } else {
                obj(vec![("id-1", entry)])
            },
        );
        set_field(&host.owner, "isDisposed", b(scene == "disposed"));
        let store = field(&host.owner, "store");
        set_field(
            &store,
            "orders",
            arr(if scene == "store-order-preferred" {
                vec![
                    obj(vec![
                        ("id", t("unrelated")),
                        ("amount", n(999.)),
                        ("remaining", n(999.)),
                        ("filled", n(0.)),
                    ]),
                    store_order.clone(),
                ]
            } else {
                vec![]
            }),
        );
        set_field(
            &store,
            "loaded",
            obj(vec![("sentinel", b(true)), ("orders", b(false))]),
        );
        Rc::new(Self {
            host,
            scene,
            trace: RefCell::new(vec![]),
            global: obj(vec![
                ("Math", obj(vec![])),
                ("virtualClock", obj(vec![])),
                (
                    "LogSeverity",
                    obj(vec![(
                        "Warning",
                        t(safe_cex_exchange::runtime::LogSeverity::Warning.as_str()),
                    )]),
                ),
            ]),
            entry_order,
            store_order,
            fetched_orders: arr(vec![obj(vec![("id", t("snapshot-row"))])]),
            original: if scene == "fills-reject-falsy" {
                n(0.)
            } else {
                Rc::new(Node::Error {
                    name: "Error".encode_utf16().collect(),
                    message: "original REST refusal"
                        .encode_utf16()
                        .chain((scene == "fills-reject-lone-message").then_some(0xd800))
                        .collect(),
                })
            },
            reporting: Rc::new(Node::Error {
                name: "Error".encode_utf16().collect(),
                message: "warning listener refusal".encode_utf16().collect(),
            }),
            held: RefCell::new(vec![]),
            predicate_calls: Cell::new(0),
        })
    }
    fn turns(&self) {
        for _ in 0..256 {
            if self.host.queue.lock().unwrap().is_empty() {
                return;
            }
            self.host.turn();
        }
        panic!("reconciliation_native_microtask_bound");
    }
    fn pending(&self) -> V {
        let value = Rc::new(Node::Promise(Promise {
            state: RefCell::new(None),
            listeners: RefCell::new(vec![]),
        }));
        self.held.borrow_mut().push(value.clone());
        value
    }
    fn filled(&self) -> f64 {
        match self.scene {
            "full-fill" | "retained-target" => 10.,
            "tolerance-boundary" => 10. - 1e-7,
            "below-tolerance" => 10. - 2e-7,
            "store-order-preferred" => 15.,
            _ => 1.,
        }
    }
    fn release(&self, value: V) {
        let held = std::mem::take(&mut *self.held.borrow_mut());
        assert_eq!(
            held.len(),
            1,
            "reconciliation_native_single_held_dependency"
        );
        let Node::Promise(promise) = held[0].as_ref() else {
            panic!("reconciliation_native_held_promise")
        };
        *promise.state.borrow_mut() = Some(Ok(value));
        for waker in std::mem::take(&mut *promise.listeners.borrow_mut()) {
            waker.wake();
        }
    }
}

#[test]
#[ignore = "requires Node and installed TypeScript for maintained-source comparison"]
fn local_reconciliation_native_source_oracle() {
    let mut actual = vec![];
    let names = [
        "missing-entry",
        "disposed",
        "not-eligible",
        "full-fill",
        "tolerance-boundary",
        "below-tolerance",
        "zero-target",
        "within-grace",
        "grace-boundary",
        "fills-reject",
        "fills-reject-lone-message",
        "fills-reject-falsy",
        "orders-reject",
        "log-throws",
        "disposed-after-fills",
        "disposed-after-orders",
        "retained-target",
        "store-order-preferred",
    ];
    assert_eq!(names.len(), 18, "reconciliation_native_declared_case_floor");
    for name in names {
        let h = ReconciliationHost::new(name);
        let result = reconciliation::reconcile_local_reduce_only_limit_order(
            h.clone(),
            h.host.owner.clone(),
            t("id-1"),
        );
        if name == "disposed-after-fills" {
            set_field(&h.host.owner, "isDisposed", b(true));
            h.release(obj(vec![("id-1", n(10.))]));
        }
        if name == "retained-target" {
            let entry = obj(vec![
                ("placedAt", n(8000.)),
                ("order", h.store_order.clone()),
            ]);
            set_field(&field(&h.host.owner, "recentlyPlacedOrders"), "id-1", entry);
            h.release(obj(vec![("id-1", n(h.filled()))]));
        }
        if name == "disposed-after-orders" {
            h.turns();
            assert_eq!(
                h.trace.borrow().iter().filter(|e| e[0] == "orders").count(),
                1,
                "reconciliation_native_second_await_reached"
            );
            set_field(&h.host.owner, "isDisposed", b(true));
            h.release(arr(vec![obj(vec![("id", t("late-row"))])]));
        }
        h.host.drain();
        let Node::Promise(promise) = result.as_ref() else {
            panic!("reconciliation_native_async_result")
        };
        let mut record = json!({"name":name,"events":h.trace.borrow().clone()});
        match promise.state.borrow().as_ref() {
            Some(Ok(value)) => {
                assert_ne!(name, "log-throws", "reconciliation_native_rejection_floor");
                assert!(
                    matches!(value.as_ref(), Node::U),
                    "reconciliation_native_undefined_result"
                );
            }
            Some(Err(error)) => {
                assert_eq!(
                    name, "log-throws",
                    "reconciliation_native_only_logger_throws"
                );
                assert!(
                    Rc::ptr_eq(error, &h.reporting),
                    "reconciliation_native_original_logger_error"
                );
                record["rejected"] = json!("warning listener refusal");
            }
            None => panic!("reconciliation_native_unsettled_result"),
        }
        let trace = h.trace.borrow();
        let count = |key: &str| trace.iter().filter(|event| event[0] == key).count();
        let guarded = matches!(name, "missing-entry" | "disposed" | "not-eligible");
        let full = matches!(name, "full-fill" | "tolerance-boundary" | "retained-target");
        let orders = matches!(
            name,
            "below-tolerance"
                | "zero-target"
                | "grace-boundary"
                | "orders-reject"
                | "disposed-after-orders"
                | "store-order-preferred"
        );
        assert_eq!(
            count("eligible"),
            usize::from(!matches!(name, "missing-entry" | "disposed")),
            "reconciliation_native_eligible_floor {name}"
        );
        assert_eq!(
            count("fills"),
            usize::from(!guarded),
            "reconciliation_native_fills_floor {name}"
        );
        assert_eq!(
            count("seen"),
            usize::from(full),
            "reconciliation_native_seen_floor {name}"
        );
        assert_eq!(
            count("remove"),
            usize::from(full),
            "reconciliation_native_remove_floor {name}"
        );
        assert_eq!(
            count("orders"),
            usize::from(orders),
            "reconciliation_native_orders_floor {name}"
        );
        assert_eq!(
            count("update"),
            usize::from(orders && !matches!(name, "orders-reject" | "disposed-after-orders")),
            "reconciliation_native_update_floor {name}"
        );
        assert_eq!(
            count("log"),
            usize::from(matches!(
                name,
                "fills-reject"
                    | "fills-reject-lone-message"
                    | "fills-reject-falsy"
                    | "orders-reject"
                    | "log-throws"
            )),
            "reconciliation_native_log_floor {name}"
        );
        assert_eq!(
            count("target"),
            usize::from(
                !guarded
                    && !matches!(
                        name,
                        "fills-reject"
                            | "fills-reject-lone-message"
                            | "fills-reject-falsy"
                            | "log-throws"
                            | "disposed-after-fills"
                    )
            ),
            "reconciliation_native_target_floor {name}"
        );
        assert_eq!(
            count("clock"),
            usize::from(orders || name == "within-grace"),
            "reconciliation_native_clock_floor {name}"
        );
        assert_eq!(
            h.predicate_calls.get(),
            if name == "store-order-preferred" {
                2
            } else {
                0
            },
            "reconciliation_native_predicate_call_floor {name}"
        );
        let expected_keys: &[&str] = match name {
            "missing-entry" | "disposed" => &[],
            "not-eligible" => &["eligible"],
            "full-fill" | "tolerance-boundary" | "retained-target" => {
                &["eligible", "fills", "target", "seen", "remove"]
            }
            "fills-reject" | "fills-reject-lone-message" | "fills-reject-falsy" | "log-throws" => {
                &["eligible", "fills", "log"]
            }
            "orders-reject" => &["eligible", "fills", "target", "clock", "orders", "log"],
            "disposed-after-fills" => &["eligible", "fills"],
            "disposed-after-orders" => &["eligible", "fills", "target", "clock", "orders"],
            "within-grace" => &["eligible", "fills", "target", "clock"],
            _ => &["eligible", "fills", "target", "clock", "orders", "update"],
        };
        assert_eq!(
            trace
                .iter()
                .map(|event| event[0].as_str().unwrap())
                .collect::<Vec<_>>(),
            expected_keys,
            "reconciliation_native_independent_call_order {name}"
        );
        actual.push(record);
    }
    assert_eq!(
        actual.len(),
        18,
        "reconciliation_native_completed_case_floor"
    );
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let output = std::process::Command::new("node")
        .arg(root.join("sdk-rs/safe-cex-venue-bybit/scripts/local-order-reconciliation-source.cjs"))
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "reconciliation source failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        expected.as_array().unwrap().len(),
        18,
        "reconciliation_source_fixed_case_floor"
    );
    compare_observed_json(
        &json!(actual),
        &expected,
        "reconciliation supplemental comparison after all native cases",
    );
}

fn compare_observed_json(native: &Value, source: &Value, path: &str) {
    match (native, source) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(
            a.as_f64().unwrap().to_bits(),
            b.as_f64().unwrap().to_bits(),
            "{path}: binary64"
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}: array count");
            for (index, (a, b)) in a.iter().zip(b).enumerate() {
                compare_observed_json(a, b, &format!("{path}.{index}"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(
                a.keys().collect::<Vec<_>>(),
                b.keys().collect::<Vec<_>>(),
                "{path}: keys"
            );
            for (key, a) in a {
                compare_observed_json(a, &b[key], &format!("{path}.{key}"));
            }
        }
        _ => assert_eq!(native, source, "{path}: scalar"),
    }
}
