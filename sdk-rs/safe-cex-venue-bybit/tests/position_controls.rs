use safe_cex_venue_bybit::position_controls::*;
use serde_json::{Map, Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};
type Error = Rc<Value>;
fn drive<F: Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..5 {
        if let Poll::Ready(result) = future.as_mut().poll(&mut cx) {
            return result;
        }
    }
    panic!("POSITION native future not settled at poll 5")
}
async fn pending_once() {
    let mut pending = true;
    std::future::poll_fn(|cx| {
        if pending {
            pending = false;
            cx.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    })
    .await;
}
fn undefined() -> Value {
    json!({"undefined":true})
}
fn optional(v: Option<Value>) -> Value {
    v.unwrap_or_else(undefined)
}
fn number(v: Option<&Value>, fallback: f64) -> f64 {
    v.map(|v| match v.as_str().unwrap() {
        "NaN" => f64::NAN,
        "Infinity" => f64::INFINITY,
        "-Infinity" => f64::NEG_INFINITY,
        s => s.parse().unwrap(),
    })
    .unwrap_or(fallback)
}
fn batch_value(v: Option<&Positions>) -> Value {
    optional(v.map(|v| Value::Array(v.as_ref().clone())))
}
struct Host {
    input: Value,
    family: String,
    events: RefCell<Vec<Value>>,
    errors: BTreeMap<String, Error>,
    batches: [Positions; 2],
    fetches: Cell<usize>,
    posts: Cell<usize>,
    maps: Cell<usize>,
    margin: RefCell<String>,
    updated: RefCell<Option<Positions>>,
    upserted: RefCell<Option<Positions>>,
}
impl Host {
    fn new(row: &Value) -> Self {
        let i = &row["input"];
        Self{input:i.clone(),family:row["family"].as_str().unwrap().into(),events:RefCell::new(vec![]),errors:["market","positions","category","postSync","postAsync","fetchSync","fetchAsync","upsert","update","emit","margin","getSync","getAsync","map","helperSync","helperAsync"].into_iter().map(|p|(p.into(),Rc::new(json!({"name":format!("{p}Error"),"message":format!("{p} failed"),"code":17})))).collect(),batches:[Rc::new(i.get("fetched").cloned().unwrap_or_else(||json!([{"symbol":"BTC","leverage":2,"positionMode":"hedged","tag":"F0"}])).as_array().unwrap().clone()),Rc::new(i.get("refreshed").cloned().unwrap_or_else(||json!([{"symbol":"BTC","leverage":4,"positionMode":"oneway","tag":"F1"}])).as_array().unwrap().clone())],fetches:Cell::new(0),posts:Cell::new(0),maps:Cell::new(0),margin:RefCell::new("cross".into()),updated:RefCell::new(None),upserted:RefCell::new(None)}
    }
    fn event(&self, phase: &str, value: Value) -> Result<(), Error> {
        self.events.borrow_mut().push(value);
        if self.input["phase"] == phase {
            Err(Rc::clone(&self.errors[phase]))
        } else {
            Ok(())
        }
    }
    fn positions(&self) -> Result<Positions, Error> {
        let p = self
            .input
            .get("stored")
            .cloned()
            .unwrap_or_else(|| json!([{"symbol":"BTC","leverage":2}]));
        self.event("positions", json!(["positions", p]))?;
        Ok(Rc::new(p.as_array().unwrap().clone()))
    }
    fn market(&self, s: &str) -> Result<Option<(f64, f64)>, Error> {
        // The source getter logs its fixed BTC map, then the body indexes it by symbol.
        self.event("market", json!(["market", "BTC"]))?;
        Ok(if self.input["noMarket"] == true || s != "BTC" {
            None
        } else {
            Some((
                number(self.input.get("min"), 1.),
                number(self.input.get("max"), 10.),
            ))
        })
    }
    fn category(&self, s: &str) -> Result<String, Error> {
        self.event("category", json!(["category", s]))?;
        Ok(if self.family == "primary" {
            "linear"
        } else {
            "inverse"
        }
        .into())
    }
    fn post<'a>(
        &'a self,
        endpoint: &str,
        payload: Map<String, Value>,
    ) -> Result<impl Future<Output = Result<Rc<Value>, Error>> + 'a + use<'a>, Error> {
        self.posts.set(self.posts.get() + 1);
        self.event("postSync", json!(["post", endpoint, payload]))?;
        let switching = endpoint == "/v5/position/switch-mode";
        Ok(async move {
            pending_once().await;
            self.event("postAsync", json!(["postResolved"]))?;
            Ok(Rc::new(self.input.get("data").cloned().unwrap_or_else(||json!({"retMsg":if switching{"All symbols switched successfully."}else{"OK"},"retCode":0}))))
        })
    }
    fn get<'a>(
        &'a self,
        endpoint: &str,
        params: Map<String, Value>,
    ) -> Result<impl Future<Output = Result<Rc<Value>, Error>> + 'a + use<'a>, Error> {
        self.event("getSync", json!(["get", endpoint, params]))?;
        Ok(async move {
            pending_once().await;
            self.event("getAsync", json!(["getResolved"]))?;
            Ok(Rc::new(self.input.get("data").cloned().unwrap_or_else(||json!({"retMsg":"OK","retCode":0,"result":{"list":[{"positionMode":"hedged","tag":"RAW"}]}}))))
        })
    }
    fn fetch(
        &self,
        filter: Option<Vec<String>>,
    ) -> Result<impl Future<Output = Result<Positions, Error>> + '_, Error> {
        self.fetches.set(self.fetches.get() + 1);
        self.event(
            "fetchSync",
            json!(["fetch", optional(filter.map(|v| json!(v)))]),
        )?;
        Ok(async move {
            pending_once().await;
            self.event("fetchAsync", json!(["fetchResolved", self.fetches.get()]))?;
            Ok(Rc::clone(&self.batches[self.fetches.get() - 1]))
        })
    }
    fn write(&self, kind: &str, rows: Option<Positions>) -> Result<(), Error> {
        let id = rows
            .as_ref()
            .and_then(|r| self.batches.iter().position(|b| Rc::ptr_eq(b, r)))
            .map(|v| v as i64)
            .unwrap_or(-1);
        self.event(kind, json!([kind, batch_value(rows.as_ref()), id]))?;
        if kind == "upsert" {
            *self.upserted.borrow_mut() = rows
        } else {
            *self.updated.borrow_mut() = rows
        };
        Ok(())
    }
    fn emit(&self, message: Option<Value>) -> Result<(), Error> {
        self.event("emit", json!(["emit", optional(message)]))
    }
    fn map(&self, row: &Value, index: usize) -> Result<Value, Error> {
        self.maps.set(self.maps.get() + 1);
        self.event("map", json!(["map", row, index]))?;
        Ok(self.input.get("mapped").cloned().unwrap_or_else(|| {
            let mut value = json!({"symbol":"BTC","tag":"MAPPED"});
            if let Some(mode) = row.get("positionMode") {
                value
                    .as_object_mut()
                    .unwrap()
                    .insert("positionMode".into(), mode.clone());
            }
            value
        }))
    }
    fn helper<'a>(
        &'a self,
        hedged: bool,
        category: &str,
        coin: &str,
    ) -> Result<impl Future<Output = Result<(), Error>> + 'a + use<'a>, Error> {
        self.event("helperSync", json!(["helper", hedged, category, coin]))?;
        Ok(async move {
            pending_once().await;
            self.event("helperAsync", json!(["helperResolved"]))
        })
    }
    fn state(&self) -> Value {
        json!({"fetches":self.fetches.get(),"posts":self.posts.get(),"maps":self.maps.get(),"margin":self.margin.borrow().as_str(),"updated":batch_value(self.updated.borrow().as_ref()),"upserted":batch_value(self.upserted.borrow().as_ref())})
    }
}
// Inspect decoded native leaves directly. No serialization, re-encoding or wire assertion.
fn native(actual: Option<&Value>, expected: &Value, label: &str) {
    if expected.get("undefined") == Some(&Value::Bool(true)) {
        assert!(
            actual.is_none_or(|v| v == &undefined()),
            "{label}: expected native absence, got {actual:?}"
        );
        return;
    }
    let actual =
        actual.unwrap_or_else(|| panic!("{label}: absent native field, expected {expected:?}"));
    if let Some(bits) = expected.get("number").and_then(Value::as_str) {
        let n = actual
            .as_f64()
            .unwrap_or_else(|| panic!("{label}: native number required, got {actual:?}"));
        if bits == "nan" {
            assert!(n.is_nan(), "{label}: NaN required")
        } else {
            assert_eq!(
                format!("{:016x}", n.to_bits()),
                bits,
                "{label}: native number"
            )
        };
        return;
    }
    match expected {
        Value::Array(items) => {
            let a = actual
                .as_array()
                .unwrap_or_else(|| panic!("{label}: native array required"));
            assert_eq!(a.len(), items.len(), "{label}: native array count");
            for (a, e) in a.iter().zip(items) {
                native(Some(a), e, label)
            }
        }
        Value::Object(fields) => {
            let a = actual
                .as_object()
                .unwrap_or_else(|| panic!("{label}: native object required"));
            for (key, e) in fields {
                native(a.get(key), e, label)
            }
            for key in a.keys() {
                assert!(
                    fields.contains_key(key),
                    "{label}: unexpected native key {key}"
                )
            }
        }
        _ => assert_eq!(actual, expected, "{label}: decoded native value"),
    }
}
#[test]
#[ignore = "requires the maintained TypeScript oracle"]
fn position_controls_match_maintained_bodies() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/position-controls-fixtures.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 188, "POSITION independent fixture floor");
    let mut floors = BTreeMap::<String, usize>::new();
    for (iteration, row) in rows.iter().enumerate() {
        let h = Host::new(row);
        let kind = row["kind"].as_str().unwrap();
        *floors.entry(kind.into()).or_default() += 1;
        let i = &h.input;
        let hedged = i["hedged"] == true;
        let s = i.get("symbol").and_then(Value::as_str);
        let result: Result<Option<Value>, Error> = match kind {
            "leverage" => {
                let n = number(i.get("input"), 4.);
                let r = if h.family == "primary" {
                    drive(set_leverage(
                        s.unwrap_or("BTC"),
                        n,
                        |s| h.market(s),
                        || h.positions(),
                        |s| h.category(s),
                        |e, p| h.post(e, p),
                        |f| h.fetch(f),
                        |p| h.write("upsert", Some(p)),
                        |m| h.emit(m),
                    ))
                } else {
                    drive(set_inverse_leverage(
                        s.unwrap_or("BTC"),
                        n,
                        |s| h.market(s),
                        || h.positions(),
                        |s| h.category(s),
                        |e, p| h.post(e, p),
                        |f| h.fetch(f),
                        |p| h.write("upsert", Some(p)),
                        |m| h.emit(m),
                    ))
                };
                r.map(|()| None)
            }
            "mode" if h.family == "primary" => {
                drive(change_primary_position_mode(hedged, s, |a, b, c| {
                    h.helper(a, b, c)
                }))
                .map(|()| None)
            }
            "mode" => {
                drive(change_position_mode(hedged, s, |a, b, c| h.helper(a, b, c))).map(|()| None)
            }
            "helper" => drive(change_position_mode_helper(
                hedged,
                i.get("category").and_then(Value::as_str),
                i.get("coin").and_then(Value::as_str),
                s,
                |e, p| h.post(e, p),
                |f| h.fetch(f),
                |p| h.write("update", Some(p)),
                |p| h.write("upsert", Some(p)),
                |m| h.emit(m),
            ))
            .map(|()| None),
            "margin" => drive(change_margin_type(
                i["isolated"] == true,
                s,
                |e, p| h.post(e, p),
                |m| h.emit(m),
                |m| {
                    let m = match m {
                        safe_cex_types::MarginType::Isolated => "isolated",
                        safe_cex_types::MarginType::Cross => "cross",
                    };
                    h.event("margin", json!(["margin", m]))?;
                    *h.margin.borrow_mut() = m.into();
                    Ok(())
                },
                |f| h.fetch(f),
                |p| h.write("update", Some(p)),
            ))
            .map(|()| None),
            "fetch" => {
                let emit =
                    |m: Option<safe_cex_types::retained_json::RetainedJsonInput>, context: Option<&str>, extra: Option<safe_cex_types::retained_json::RetainedJsonInput>| {
                        let m = m.map(|value| value.into_value().expect("finite source fixture message"));
                        let extra = extra.map(|value| value.into_value().expect("finite source fixture diagnostic"));
                        if let Some(context) = context {
                            h.event("emit", json!(["emit", optional(m), context, extra]))
                        } else {
                            h.emit(m)
                        }
                    };
                if h.family == "primary" {
                    drive(fetch_position_mode(
                        "BTC",
                        || h.positions(),
                        |s| h.category(s),
                        |e, p| h.get(e, p),
                        |r, i| h.map(r, i),
                        |p| h.write("upsert", p),
                        emit,
                    ))
                } else {
                    drive(fetch_inverse_position_mode(
                        "BTC",
                        || h.positions(),
                        |s| h.category(s),
                        |e, p| h.get(e, p),
                        |r, i| h.map(r, i),
                        |p| h.write("upsert", p),
                        emit,
                    ))
                }
            }
            _ => panic!("unexpected family"),
        };
        let label = format!("POSITION native {kind} event iteration {iteration} {row}");
        native(
            Some(&Value::Array(h.events.borrow().clone())),
            &row["events"],
            &label,
        );
        assert_eq!(
            result.is_ok(),
            row["result"].get("value").is_some(),
            "POSITION native rejection iteration {iteration}"
        );
        match result {
            Ok(value) => native(
                Some(&optional(value)),
                &row["result"]["value"],
                &format!("POSITION native {kind} result iteration {iteration}"),
            ),
            Err(error) => {
                native(
                    error.get("name"),
                    &row["result"]["error"],
                    "POSITION native error name",
                );
                native(
                    error.get("message"),
                    &row["result"]["message"],
                    "POSITION native error message",
                );
                let origin = h
                    .errors
                    .iter()
                    .find(|(_, e)| Rc::ptr_eq(e, &error))
                    .map(|(key, _)| key.as_str());
                assert_eq!(
                    origin,
                    row["result"]["origin"].as_str(),
                    "POSITION native original error identity iteration {iteration}"
                )
            }
        }
        native(
            Some(&h.state()),
            &row["state"],
            "POSITION native store state",
        );
    }
    assert_eq!(
        floors,
        BTreeMap::from([
            ("leverage".into(), 54),
            ("mode".into(), 14),
            ("helper".into(), 30),
            ("margin".into(), 40),
            ("fetch".into(), 50)
        ]),
        "POSITION independent family floors"
    );
}
