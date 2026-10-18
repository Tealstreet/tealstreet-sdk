include!("support/private_account_values.rs");
use safe_cex_venue_kraken::private_accounts::{
    self as account, AccountHost, IncomeHost, PageCallback,
};
fn copy_raw(v: &Raw) -> Rc<Raw> {
    Rc::new(match v {
        Raw::Undefined => Raw::Undefined,
        Raw::Null => Raw::Null,
        Raw::Bool(b) => Raw::Bool(*b),
        Raw::Number(n) => Raw::Number(*n),
        Raw::Text(s) => Raw::Text(s.clone()),
        Raw::Object(o) => Raw::Object(RefCell::new(o.borrow().clone())),
        Raw::Array(a) => Raw::Array(a.clone()),
    })
}
fn contains(root: &Raw, target: &Raw) -> bool {
    std::ptr::eq(root, target)
        || match root {
            Raw::Object(o) => o.borrow().iter().any(|(_, v)| contains(v, target)),
            Raw::Array(a) => a.iter().any(|v| contains(v, target)),
            _ => false,
        }
}
impl Host {
    fn effect(&self, kind: &str, mut pairs: Vec<(&str, Rc<Raw>)>) -> Result<(), E> {
        pairs.insert(0, ("kind", text(kind)));
        self.event(pairs);
        if self.strict_eq(&get(&self.cfg, "effectFail"), &text(kind)) {
            return Err(self.original.clone());
        }
        if self.events.borrow().len() == 1 {
            if let Raw::Object(o) = &*get(&self.cfg, "afterEffect") {
                for (k, v) in o.borrow().iter() {
                    self.assign(&self.store, k, v.clone())?;
                }
            }
        }
        Ok(())
    }
    fn info_identity(&self, o: &Raw) -> Rc<Raw> {
        self.boolean(contains(&get(&self.cfg, "input"), &get(o, "info")))
    }
    fn update_existing(&self, selector: &Raw, patch: &Raw) -> Result<(), E> {
        for o in self.items(&get(&self.store, "orders"))? {
            if self.strict_eq(&get(&o, "id"), &get(selector, "id")) {
                for (k, v) in self.entries(patch)? {
                    self.assign(&o, &k, v)?;
                }
                break;
            }
        }
        Ok(())
    }
}
impl AccountHost for Host {
    fn add(&self, a: &Raw, b: &Raw) -> Result<Rc<Raw>, E> {
        let primitive = |v: &Raw| {
            if matches!(v, Raw::Object(_) | Raw::Array(_)) {
                self.string(v).map(|s| self.text(&s))
            } else {
                Ok(copy_raw(v))
            }
        };
        let a = primitive(a)?;
        let b = primitive(b)?;
        if matches!(&*a, Raw::Text(_)) || matches!(&*b, Raw::Text(_)) {
            Ok(self.text(&[self.string(&a)?, self.string(&b)?].concat()))
        } else {
            Ok(self.number(self.numeric(&a)? + self.numeric(&b)?))
        }
    }
    fn to_string_method(&self, v: &Raw) -> Result<Rc<Raw>, E> {
        if self.nullish(v) {
            self.field(v, &units("toString"), false)?;
        }
        Ok(self.text(&self.string(v)?))
    }
    fn date_milliseconds(&self, v: &Raw) -> Result<f64, E> {
        let n = match v {
            Raw::Text(s) => safe_cex_num::js_date_parse(&String::from_utf16(s).unwrap(), |n| n),
            _ => self.numeric(v)?,
        };
        Ok(if !n.is_finite() || n.abs() > 8640000000000000.0 {
            f64::NAN
        } else {
            n.trunc() + 0.0
        })
    }
    fn position_id(&self, symbol: &Raw, side: &str) -> Result<Rc<Raw>, E> {
        Ok(self.text(&[self.string(symbol)?, units(":"), units(side)].concat()))
    }
    fn optional_preference(&self, cache: &str, symbol: &Raw) -> Result<Rc<Raw>, E> {
        if flag(
            &self.cfg,
            if cache == "margin" {
                "missingMargin"
            } else {
                "missingLeverage"
            },
        ) {
            Ok(Rc::new(Raw::Undefined))
        } else {
            self.preference(cache, symbol)
        }
    }
    fn optional_set_margin(&self, s: Rc<Raw>, m: Rc<Raw>) -> Result<(), E> {
        if flag(&self.cfg, "missingMargin") {
            Ok(())
        } else {
            self.set_preference(s, m)
        }
    }
    fn update_order(&self, selector: Rc<Raw>, patch: Rc<Raw>) -> Result<(), E> {
        self.effect(
            "updateOrder",
            vec![("selector", selector.clone()), ("patch", patch.clone())],
        )?;
        self.update_existing(&selector, &patch)
    }
    fn order_status(&self, selector: Rc<Raw>, status: Rc<Raw>) -> Result<(), E> {
        self.effect(
            "status",
            vec![("selector", selector.clone()), ("status", status.clone())],
        )?;
        self.update_existing(&selector, &*self.object(vec![(units("status"), status)])?)
    }
    fn add_order(&self, order: Rc<Raw>) -> Result<(), E> {
        self.effect(
            "addOrder",
            vec![
                ("order", order.clone()),
                ("infoIdentity", self.info_identity(&order)),
            ],
        )?;
        let mut rows = self.items(&get(&self.store, "orders"))?;
        if let Some(i) = rows
            .iter()
            .position(|o| self.strict_eq(&get(o, "id"), &get(&order, "id")))
        {
            rows[i] = order;
        } else {
            rows.push(order);
        }
        self.assign(&self.store, &units("orders"), self.array(rows))
    }
    fn full_orders(&self, orders: Rc<Raw>) -> Result<(), E> {
        self.effect(
            "fullOrders",
            vec![
                ("orders", orders.clone()),
                (
                    "infoIdentity",
                    self.boolean(
                        self.items(&orders)?
                            .iter()
                            .all(|o| matches!(&*self.info_identity(o), Raw::Bool(true))),
                    ),
                ),
            ],
        )?;
        self.assign(&self.store, &units("orders"), orders)
    }
    fn full_positions(&self, p: Rc<Raw>) -> Result<(), E> {
        self.effect(
            "fullPositions",
            vec![
                ("positions", p.clone()),
                (
                    "infoIdentity",
                    self.boolean(
                        self.items(&p)?
                            .iter()
                            .all(|o| matches!(&*self.info_identity(o), Raw::Bool(true))),
                    ),
                ),
            ],
        )?;
        self.assign(&self.store, &units("positions"), p)
    }
    fn fill(&self, payload: Rc<Raw>) -> Result<(), E> {
        self.effect("fill", vec![("payload", payload)])
    }
    fn throttle(&self) -> Result<(), E> {
        self.effect("throttle", vec![])
    }
    fn parse_order(&self, item: Rc<Raw>) -> Result<Rc<Raw>, E> {
        self.effect(
            "parse",
            vec![(
                "identity",
                self.boolean(contains(&get(&self.cfg, "input"), &item)),
            )],
        )?;
        if flag(&self.cfg, "overrideParse") {
            self.object(vec![
                (units("id"), text("override")),
                (units("symbol"), text("Z")),
                (units("info"), item),
            ])
        } else {
            account::spot_parse_order(self, item)
        }
    }
    fn null(&self) -> Rc<Raw> {
        Rc::new(Raw::Null)
    }
}
impl IncomeHost for Host {
    fn date_iso(&self, v: &Raw) -> Result<Vec<u16>, E> {
        self.iso(self.date_milliseconds(v)?)
    }
    fn paginate<'a>(
        &'a self,
        options: Rc<Raw>,
        page: PageCallback<'static, Self>,
    ) -> Result<impl Future<Output = Result<Rc<Raw>, E>> + 'a, E> {
        self.effect("paginate", vec![("options", options.clone())])?;
        if flag(&self.cfg, "pagerSyncFail") {
            return Err(self.original.clone());
        }
        Ok(async move {
            let calls = get(&self.cfg, "pageCalls");
            let calls = if self.undefined(&calls) {
                1
            } else {
                self.numeric(&calls)? as usize
            };
            for i in 0..calls {
                page(self.object(vec![
                    (
                        units("startTime"),
                        self.add(&get(&options, "startTime"), &self.number(i as f64))?,
                    ),
                    (units("limit"), self.number(17.25)),
                ])?)?
                .await?;
            }
            if flag(&self.cfg, "pagerFail") {
                return Err(self.original.clone());
            }
            self.object(vec![
                (units("records"), get(&self.cfg, "records")),
                (
                    units("truncated"),
                    self.boolean(flag(&self.cfg, "truncated")),
                ),
                (
                    units("exhaustedChunks"),
                    if self.undefined(&get(&self.cfg, "exhaustedChunks")) {
                        self.number(0.0)
                    } else {
                        get(&self.cfg, "exhaustedChunks")
                    },
                ),
            ])
        })
    }
    fn account_log_page(
        &self,
        params: Rc<Raw>,
    ) -> Result<impl Future<Output = Result<Rc<Raw>, E>>, E> {
        self.effect("page", vec![("params", params)])?;
        Ok(async move {
            yield_once().await;
            if flag(&self.cfg, "pageFail") {
                Err(self.original.clone())
            } else {
                Ok(self.array(vec![]))
            }
        })
    }
    fn income_range(
        &self,
        start: Rc<Raw>,
        end: Rc<Raw>,
        income_type: Rc<Raw>,
    ) -> Result<impl Future<Output = Result<Rc<Raw>, E>>, E> {
        self.effect(
            "range",
            vec![("start", start), ("end", end), ("type", income_type)],
        )?;
        if flag(&self.cfg, "rangeSyncFail") {
            return Err(self.original.clone());
        }
        Ok(async move {
            yield_once().await;
            if let Raw::Object(o) = &*get(&self.cfg, "afterOptions") {
                let input = get(&self.cfg, "input");
                for (k, v) in o.borrow().iter() {
                    self.assign(&input, k, v.clone())?;
                }
            }
            if flag(&self.cfg, "rangeFail") {
                Err(self.original.clone())
            } else {
                Ok(get(&self.cfg, "records"))
            }
        })
    }
    fn warning(&self, message: Rc<Raw>) -> Result<(), E> {
        self.effect("warning", vec![("message", message)])
    }
    fn income_error(&self, message: Rc<Raw>) -> Result<(), E> {
        self.effect("incomeError", vec![("message", message)])
    }
}
fn native(actual: &Raw, expected: &Value, label: &str) {
    match actual {
        Raw::Undefined => assert_eq!(expected["undefined"], true, "{label}: undefined"),
        Raw::Null => assert!(expected.is_null(), "{label}: null"),
        Raw::Bool(b) => assert_eq!(expected.as_bool(), Some(*b), "{label}: boolean"),
        Raw::Text(s) => assert_eq!(
            expected.as_str().map(units),
            Some(s.clone()),
            "{label}: UTF16"
        ),
        Raw::Number(n) => {
            let bits = expected["number"]
                .as_str()
                .unwrap_or_else(|| panic!("{label}: expected native numeric {expected}"));
            let n2 = f64::from_bits(u64::from_str_radix(bits, 16).unwrap());
            if n2.is_nan() {
                assert!(n.is_nan(), "{label}: native NaN")
            } else {
                assert_eq!(n.to_bits(), n2.to_bits(), "{label}: native bits");
            }
        }
        Raw::Array(a) => {
            let e = expected
                .as_array()
                .unwrap_or_else(|| panic!("{label}: expected array {expected}"));
            assert_eq!(a.len(), e.len(), "{label}: native length");
            for (i, (a, e)) in a.iter().zip(e).enumerate() {
                native(a, e, &format!("{label}[{i}]"));
            }
        }
        Raw::Object(o) => {
            let o = o.borrow();
            let e = expected
                .as_object()
                .unwrap_or_else(|| panic!("{label}: expected object {expected}"));
            assert_eq!(o.len(), e.len(), "{label}: native fields");
            assert_eq!(
                o.iter()
                    .map(|(k, _)| String::from_utf16(k).unwrap())
                    .collect::<Vec<_>>(),
                e.keys().cloned().collect::<Vec<_>>(),
                "{label}: native own-key order"
            );
            for (k, a) in o.iter() {
                let k = String::from_utf16(k).unwrap();
                native(
                    a,
                    e.get(&k)
                        .unwrap_or_else(|| panic!("{label}.{k}: missing source field")),
                    &format!("{label}.{k}"),
                );
            }
        }
    }
}
async fn yield_once() {
    let mut first = true;
    std::future::poll_fn(move |_| {
        if first {
            first = false;
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    })
    .await;
}
fn run<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..20 {
        if let Poll::Ready(v) = future.as_mut().poll(&mut cx) {
            return v;
        }
    }
    panic!("finite continuation floor exceeded");
}
fn oracle() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        let output = Command::new("node")
            .env("TZ", "UTC")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/private-accounts-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let v: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(v["header"]["hashes"].as_object().unwrap().len(), 4);
        assert_eq!(v["header"]["tz"], "UTC");
        v
    })
}
fn check(family: &str, floor: usize) {
    let rows = oracle()["rows"][family].as_array().unwrap();
    assert_eq!(rows.len(), floor, "independent finite case floor");
    for (i, row) in rows.iter().enumerate() {
        let cfg = row["cfg"].clone();
        let owner = Rc::new(Host::new(&cfg));
        let h = owner.as_ref();
        let input = get(&h.cfg, "input");
        let result = match family {
            "fills" => account::futures_fills(h, input).map(|_| Rc::new(Raw::Undefined)),
            "orders" => account::futures_open_orders(h, input).map(|_| Rc::new(Raw::Undefined)),
            "positions" => {
                account::futures_open_positions(h, input).map(|_| Rc::new(Raw::Undefined))
            }
            "balances" => account::futures_balances(h, input).map(|_| Rc::new(Raw::Undefined)),
            "parse" => account::spot_parse_order(h, input),
            "executions" => account::spot_executions(h, input).map(|_| Rc::new(Raw::Undefined)),
            "range" => run(account::income_range(
                owner.clone(),
                get(&input, "start"),
                get(&input, "end"),
                get(&input, "type"),
            )),
            "history" => run(account::income_history(h, input)),
            _ => unreachable!(),
        };
        let label = format!("{family}[{i}] native");
        match result {
            Ok(value) => {
                assert!(
                    row["error"].is_null(),
                    "{label}: source error {}",
                    row["error"]
                );
                native(&value, &row["result"], &label);
                if family == "parse" && !matches!(&*value, Raw::Null) {
                    assert!(
                        Rc::ptr_eq(&get(&value, "info"), &get(&h.cfg, "input")),
                        "native original info"
                    );
                    assert_eq!(row["resultInfoIdentity"], true);
                }
            }
            Err(error) => {
                assert!(
                    !row["error"].is_null(),
                    "{label}: unexpected {}",
                    to_value(&error.message)
                );
                assert_eq!(
                    Rc::ptr_eq(&error, &h.original),
                    row["error"]["identity"].as_bool().unwrap(),
                    "native error identity"
                );
                assert_eq!(
                    error.name,
                    row["error"]["name"].as_str().unwrap(),
                    "native error name"
                );
                native(&error.message, &row["error"]["message"], &label);
            }
        }
        native(
            &Raw::Array(h.events.borrow().clone()),
            &row["events"],
            &label,
        );
        for (cache, key, missing) in [
            (&h.margin, "margin", "missingMargin"),
            (&h.leverage, "leverage", "missingLeverage"),
        ] {
            if flag(&h.cfg, missing) {
                assert_eq!(row[key]["undefined"], true)
            } else {
                native(
                    &Raw::Object(RefCell::new(cache.borrow().clone())),
                    &row[key],
                    &label,
                );
            }
        }
    }
}
#[test]
fn fills_native() {
    check("fills", 18);
}
#[test]
fn open_orders_native() {
    check("orders", 23);
}
#[test]
fn open_positions_native() {
    check("positions", 14);
}
#[test]
fn balances_native() {
    check("balances", 15);
}
#[test]
fn parse_order_native() {
    check("parse", 15);
}
#[test]
fn executions_native() {
    check("executions", 15);
}
#[test]
fn income_range_native() {
    check("range", 14);
}
#[test]
fn income_history_native() {
    check("history", 15);
}
