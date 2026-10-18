use safe_cex_types::{MarginType, PositionMode};
use safe_cex_venue_weex::{account_reads::*, symbol_settings::WeexDefaultSymbolConfig};
use serde_json::Value as J;
use std::{
    cell::RefCell,
    future::Future,
    path::Path,
    pin::Pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};

#[derive(Debug, Clone)]
enum V {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(Vec<u16>),
    Object(Rc<RefCell<Vec<(String, V)>>>, String),
    Array(Rc<RefCell<Vec<Option<V>>>>),
    Iterator(Rc<RefCell<(V, usize)>>),
}
impl PartialEq for V {
    fn eq(&self, b: &Self) -> bool {
        match (self, b) {
            (Self::Undefined, Self::Undefined) | (Self::Null, Self::Null) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Number(a), Self::Number(b)) => a.to_bits() == b.to_bits(),
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Object(a, _), Self::Object(b, _)) => *a.borrow() == *b.borrow(),
            (Self::Array(a), Self::Array(b)) => *a.borrow() == *b.borrow(),
            _ => false,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
struct Error {
    name: &'static str,
    units: Vec<u16>,
}
type H = RefCell<State>;
type F<'a, T> = Pin<Box<dyn Future<Output = Result<T, Error>> + 'a>>;
type RT = WeexAccountReadRuntime<H, V, Error>;
struct Task {
    key: String,
    left: u64,
    value: V,
    done: Option<Result<V, Error>>,
}
struct State {
    input: J,
    symbols: V,
    events: Vec<String>,
    requests: Vec<(Vec<u16>, Option<Vec<(&'static str, V)>>)>,
    tasks: Vec<Task>,
    completed: Vec<usize>,
    warnings: Vec<Vec<u16>>,
    config_calls: Vec<(V, bool)>,
    cache: V,
    original_cache: V,
    leverage: V,
    cache_index: usize,
    typed_configs: Vec<WeexDefaultSymbolConfig>,
    market_tag: V,
}
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn text(s: &str) -> V {
    V::String(units(s))
}
fn object(fields: Vec<(&'static str, V)>) -> V {
    V::Object(
        Rc::new(RefCell::new(
            fields.into_iter().map(|(k, v)| (k.into(), v)).collect(),
        )),
        String::new(),
    )
}
fn array(items: Vec<V>) -> V {
    V::Array(Rc::new(RefCell::new(items.into_iter().map(Some).collect())))
}
fn decode(j: &J) -> V {
    match j["kind"].as_str().unwrap() {
        "undefined" => V::Undefined,
        "null" => V::Null,
        "bool" => V::Bool(j["value"].as_bool().unwrap()),
        "number" => V::Number(f64::from_bits(
            u64::from_str_radix(j["bits"].as_str().unwrap(), 16).unwrap(),
        )),
        "string" => V::String(serde_json::from_value(j["units"].clone()).unwrap()),
        "array" => V::Array(Rc::new(RefCell::new(
            j["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| {
                    if v["kind"] == "hole" {
                        None
                    } else {
                        Some(decode(v))
                    }
                })
                .collect(),
        ))),
        "object" => V::Object(
            Rc::new(RefCell::new(
                j["fields"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| (v[0].as_str().unwrap().into(), decode(&v[1])))
                    .collect(),
            )),
            String::new(),
        ),
        other => panic!("unexpected {other}"),
    }
}
fn get_plain(v: &V, key: &str) -> V {
    match v {
        V::Object(fields, _) => fields
            .borrow()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or(V::Undefined),
        V::Array(items) => key
            .parse::<usize>()
            .ok()
            .and_then(|i| items.borrow().get(i).cloned().flatten())
            .unwrap_or(V::Undefined),
        _ => V::Undefined,
    }
}
fn err(stage: &str) -> Error {
    Error {
        name: "RangeError",
        units: units(&format!("fail:{stage}")),
    }
}
fn check(h: &H, stage: &str) -> Result<(), Error> {
    let s = h.borrow();
    if s.input["fail"] == stage
        || s.input["fails"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x == stage)
    {
        Err(err(stage))
    } else {
        Ok(())
    }
}
fn record(h: &H, stage: &str) -> Result<(), Error> {
    h.borrow_mut().events.push(stage.into());
    check(h, stage)
}
fn string(v: &V) -> Vec<u16> {
    match v {
        V::String(u) => u.clone(),
        V::Undefined => units("undefined"),
        V::Null => units("null"),
        V::Bool(b) => units(if *b { "true" } else { "false" }),
        V::Number(n) => units(&safe_cex_num::js_number_string(*n)),
        _ => units("[object Object]"),
    }
}
fn truthy(v: &V) -> bool {
    match v {
        V::Undefined | V::Null => false,
        V::Bool(b) => *b,
        V::Number(n) => *n != 0.0 && !n.is_nan(),
        V::String(s) => !s.is_empty(),
        _ => true,
    }
}
fn equals_text(v: &V, s: &str) -> bool {
    matches!(v,V::String(u) if *u==units(s))
}
fn get(h: &H, v: &V, key: &str) -> Result<V, Error> {
    if matches!(v, V::Null | V::Undefined) {
        return Err(Error {
            name: "TypeError",
            units: units(&format!(
                "Cannot read properties of {} (reading '{key}')",
                if matches!(v, V::Null) {
                    "null"
                } else {
                    "undefined"
                }
            )),
        });
    }
    if let V::Object(_, label) = v {
        if !label.is_empty() && (label != "row" || key == "symbol") {
            let stage = format!("get:{label}:{key}");
            record(h, &stage)?;
            if h.borrow().input["failGet"] == format!("{label}:{key}") {
                return Err(err(&stage));
            }
        }
    }
    if let V::Object(_, label) = v {
        if label == "row" && key == "symbol" {
            let mut state = h.borrow_mut();
            state.market_tag = text("NEW");
            if state.input["replaceCache"] == true {
                state.cache = array(vec![]);
            }
        }
    }
    Ok(get_plain(v, key))
}
fn optional(h: &H, v: &V, key: &str) -> Result<V, Error> {
    if matches!(v, V::Null | V::Undefined) {
        Ok(V::Undefined)
    } else {
        get(h, v, key)
    }
}
fn to_number(_: &H, v: &V, default: f64) -> Result<f64, Error> {
    if matches!(v, V::Null | V::Undefined) || equals_text(v, "") {
        return Ok(default);
    }
    let n = safe_cex_num::parse_float(&String::from_utf16_lossy(&string(v)));
    Ok(if n.is_finite() { n } else { default })
}
fn length(_: &H, v: &V) -> Result<usize, Error> {
    Ok(match v {
        V::Array(a) => a.borrow().len(),
        V::String(s) => s.len(),
        _ => 0,
    })
}
fn present(_: &H, v: &V, i: usize) -> Result<bool, Error> {
    Ok(matches!(v,V::Array(a) if a.borrow().get(i).is_some_and(Option::is_some)))
}
fn at(_: &H, v: &V, i: usize) -> Result<V, Error> {
    Ok(match v {
        V::Array(a) => a.borrow().get(i).cloned().flatten().unwrap_or(V::Undefined),
        _ => V::Undefined,
    })
}
fn iterator(_: &H, v: &V) -> Result<V, Error> {
    Ok(V::Iterator(Rc::new(RefCell::new((v.clone(), 0)))))
}
fn next(h: &H, v: &V) -> Result<Option<V>, Error> {
    let V::Iterator(i) = v else {
        panic!("iterator")
    };
    let mut i = i.borrow_mut();
    if i.1 >= length(h, &i.0)? {
        return Ok(None);
    }
    let value = at(h, &i.0, i.1)?;
    i.1 += 1;
    Ok(Some(value))
}
fn extract(h: &H, value: &V, keys: &[&str]) -> Result<V, Error> {
    if matches!(value, V::Array(_)) {
        return Ok(value.clone());
    }
    if matches!(optional(h, value, "data")?, V::Array(_)) {
        return get(h, value, "data");
    }
    for key in keys {
        if matches!(optional(h, value, key)?, V::Array(_)) {
            return get(h, value, key);
        }
        let data = optional(h, value, "data")?;
        if matches!(optional(h, &data, key)?, V::Array(_)) {
            return get(h, &get(h, value, "data")?, key);
        }
    }
    Ok(array(vec![]))
}
fn market(h: &H, captured: &V, symbol: &V) -> Result<V, Error> {
    record(
        h,
        &format!("market:{}", serde_json::to_string(&string(symbol)).unwrap()),
    )?;
    if h.borrow().input["includeMarket"] == true {
        return Ok(object(vec![("tag", captured.clone())]));
    }
    if h.borrow().input["market"] == true {
        Ok(object(vec![(
            "limits",
            object(vec![(
                "leverage",
                object(vec![("max", decode(&h.borrow().input["max"]))]),
            )]),
        )]))
    } else {
        Ok(V::Undefined)
    }
}
struct Observation<'a> {
    host: &'a H,
    index: usize,
}
impl Future for Observation<'_> {
    type Output = Result<V, Error>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        match &self.host.borrow().tasks[self.index].done {
            Some(v) => Poll::Ready(v.clone()),
            None => Poll::Pending,
        }
    }
}
fn schedule<'a>(h: &'a H, key: String, value: V) -> F<'a, V> {
    let index = {
        let mut s = h.borrow_mut();
        let index = s.tasks.len();
        let left = s.input["delays"][&key].as_u64().unwrap_or(1);
        s.tasks.push(Task {
            key,
            left,
            value,
            done: None,
        });
        index
    };
    Box::pin(Observation { host: h, index })
}
fn request<'a>(h: &'a H, r: WeexAccountReadRequest<V>) -> Result<F<'a, V>, Error> {
    let index = h.borrow().requests.len() + 1;
    let key = format!("request:{index}");
    record(h, &format!("{key}:start"))?;
    h.borrow_mut().requests.push((r.endpoint, r.params));
    let data = {
        let s = h.borrow();
        let replies = decode(&s.input["replies"]);
        let V::Array(a) = replies else {
            panic!("replies")
        };
        let a = a.borrow();
        if a.is_empty() {
            decode(&s.input["data"])
        } else {
            a[(index - 1).min(a.len() - 1)].clone().unwrap()
        }
    };
    if h.borrow().input["includeMarket"] == true || h.borrow().input["replaceCache"] == true {
        if let V::Array(a) = &data {
            for row in a.borrow_mut().iter_mut().flatten() {
                if let V::Object(_, label) = row {
                    *label = "row".into();
                }
            }
        }
    }
    Ok(schedule(h, key, data))
}
fn config<'a>(h: &'a H, symbol: V, refetch: bool) -> Result<F<'a, V>, Error> {
    record(h, "config:1:start")?;
    h.borrow_mut().config_calls.push((symbol, refetch));
    let mut value = decode(&h.borrow().input["config"]);
    if let V::Object(_, label) = &mut value {
        *label = "config".into();
    }
    Ok(schedule(h, "config:1".into(), value))
}
fn balance<'a>(h: &'a H) -> Result<F<'a, V>, Error> {
    record(h, "balance:1:start")?;
    let v = decode(&h.borrow().input["balance"]);
    Ok(schedule(h, "balance:1".into(), v))
}
fn positions<'a>(h: &'a H) -> Result<F<'a, V>, Error> {
    record(h, "positions:1:start")?;
    let v = decode(&h.borrow().input["positions"]);
    Ok(schedule(h, "positions:1".into(), v))
}
struct Pair<'a> {
    host: &'a H,
    a: F<'a, V>,
    b: F<'a, V>,
    results: [Option<Result<V, Error>>; 2],
    indices: [usize; 2],
}
impl Pair<'_> {
    fn advance(&mut self, cx: &mut Context<'_>) {
        let complete = self.host.borrow().completed.clone();
        let mut branches = [0usize, 1];
        branches.sort_by_key(|b| {
            complete
                .iter()
                .position(|i| *i == self.indices[*b])
                .unwrap_or(usize::MAX)
        });
        for branch in branches {
            if self.results[branch].is_some() {
                continue;
            }
            let before = self.host.borrow().tasks.len();
            let polled = if branch == 0 {
                self.a.as_mut().poll(cx)
            } else {
                self.b.as_mut().poll(cx)
            };
            if let Poll::Ready(v) = polled {
                self.results[branch] = Some(v);
            }
            let after = self.host.borrow().tasks.len();
            if after > before {
                self.indices[branch] = after - 1;
            }
        }
    }
}
struct All<'a>(Pair<'a>);
impl Future for All<'_> {
    type Output = Result<(V, V), Error>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0.advance(cx);
        let completed = self.0.host.borrow().completed.clone();
        for index in completed {
            for branch in 0..2 {
                if index == self.0.indices[branch] {
                    if let Some(Err(e)) = &self.0.results[branch] {
                        return Poll::Ready(Err(e.clone()));
                    }
                }
            }
        }
        if self.0.results.iter().all(Option::is_some) {
            let a = self.0.results[0].take().unwrap();
            let b = self.0.results[1].take().unwrap();
            return Poll::Ready(a.and_then(|a| b.map(|b| (a, b))));
        }
        Poll::Pending
    }
}
impl Future for Pair<'_> {
    type Output = (Result<V, Error>, Result<V, Error>);
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.advance(cx);
        if self.results.iter().all(Option::is_some) {
            Poll::Ready((
                self.results[0].take().unwrap(),
                self.results[1].take().unwrap(),
            ))
        } else {
            Poll::Pending
        }
    }
}
fn pair<'a>(h: &'a H, a: F<'a, V>, b: F<'a, V>) -> Pair<'a> {
    let n = h.borrow().tasks.len();
    Pair {
        host: h,
        a,
        b,
        results: [None, None],
        indices: [n.saturating_sub(2), n.saturating_sub(1)],
    }
}
fn all<'a>(h: &'a H, a: F<'a, V>, b: F<'a, V>) -> Result<F<'a, (V, V)>, Error> {
    record(h, "all")?;
    Ok(Box::pin(All(pair(h, a, b))))
}
fn settled<'a>(
    h: &'a H,
    a: F<'a, V>,
    b: F<'a, V>,
) -> Result<Pin<Box<dyn Future<Output = (Result<V, Error>, Result<V, Error>)> + 'a>>, Error> {
    record(h, "settled")?;
    Ok(Box::pin(pair(h, a, b)))
}
fn normalize_balance(h: &H, rows: V) -> Result<V, Error> {
    record(h, "balanceNormalize")?;
    Ok(object(vec![("rows", rows)]))
}
fn normalize_order(h: &H, row: V, market: V) -> Result<V, Error> {
    record(h, "normalNormalize")?;
    let value = get(h, &row, "normal")?;
    if h.borrow().input["includeMarket"] == true {
        Ok(object(vec![
            ("order", value),
            ("market", get_plain(&market, "tag")),
        ]))
    } else {
        Ok(value)
    }
}
fn normalize_algo(h: &H, row: V, market: V) -> Result<V, Error> {
    record(h, "algoNormalize")?;
    let value = get(h, &row, "algo")?;
    if h.borrow().input["includeMarket"] == true {
        let V::Array(a) = value else { panic!("algo") };
        Ok(array(
            a.borrow()
                .iter()
                .flatten()
                .map(|o| {
                    object(vec![
                        ("order", o.clone()),
                        ("market", get_plain(&market, "tag")),
                    ])
                })
                .collect(),
        ))
    } else {
        Ok(value)
    }
}
fn uniq(h: &H, rows: V) -> Result<V, Error> {
    let mut seen = Vec::new();
    let mut out = Vec::new();
    let n = length(h, &rows)?;
    for i in 0..n {
        let v = at(h, &rows, i)?;
        let id = get(h, &v, "id")?;
        if !seen.contains(&id) {
            seen.push(id);
            out.push(v);
        }
    }
    Ok(array(out))
}
fn copy_properties(_: &H, value: &V) -> Result<V, Error> {
    let V::Object(fields, _) = value else {
        panic!("object spread")
    };
    Ok(V::Object(
        Rc::new(RefCell::new(fields.borrow().clone())),
        String::new(),
    ))
}
fn set_property(_: &H, target: &V, key: &V, value: V) -> Result<(), Error> {
    let V::Object(fields, _) = target else {
        panic!("computed property")
    };
    let mut fields = fields.borrow_mut();
    let key = String::from_utf16_lossy(&string(key));
    if let Some((_, v)) = fields.iter_mut().find(|(k, _)| k == &key) {
        *v = value;
    } else {
        fields.push((key, value));
    }
    Ok(())
}
fn store_update(h: &H, _: V, changes: V) -> Result<(), Error> {
    record(h, "updateLeverage")?;
    h.borrow_mut().leverage = get_plain(&changes, "fetchedLeverage");
    Ok(())
}
fn cache(h: &H) -> Result<V, Error> {
    Ok(h.borrow().cache.clone())
}
fn set_config(h: &H, captured: V, key: V, c: WeexDefaultSymbolConfig) -> Result<(), Error> {
    let index = {
        let mut state = h.borrow_mut();
        state.cache_index += 1;
        state.cache_index
    };
    record(h, &format!("cacheSet:{index}"))?;
    let value = object(vec![
        (
            "marginType",
            text(if c.margin_type == MarginType::Isolated {
                "isolated"
            } else {
                "cross"
            }),
        ),
        (
            "positionMode",
            text(if c.position_mode == PositionMode::Hedged {
                "hedged"
            } else {
                "oneway"
            }),
        ),
        ("crossLeverage", V::Number(c.cross_leverage)),
        ("isolatedLongLeverage", V::Number(c.isolated_long_leverage)),
        (
            "isolatedShortLeverage",
            V::Number(c.isolated_short_leverage),
        ),
    ]);
    let V::Array(entries) = captured else {
        panic!("map")
    };
    let mut entries = entries.borrow_mut();
    let existing = entries
        .iter()
        .flatten()
        .find(|entry| matches!(entry,V::Array(e) if e.borrow()[0].as_ref()==Some(&key)))
        .cloned();
    if let Some(V::Array(entry)) = existing {
        entry.borrow_mut()[1] = Some(value);
    } else {
        entries.push(Some(array(vec![key, value])));
    }
    h.borrow_mut().typed_configs.push(c);
    Ok(())
}
fn push_spread(h: &H, captured: V, rows: &V) -> Result<(), Error> {
    let mut values = Vec::new();
    let i = iterator(h, rows)?;
    while let Some(v) = next(h, &i)? {
        values.push(Some(v));
    }
    if let Some(limit) = h.borrow().input["pushLimit"].as_u64() {
        if values.len() > limit as usize {
            return Err(err("pushArguments"));
        }
    }
    let V::Array(out) = captured else {
        panic!("push receiver")
    };
    out.borrow_mut().extend(values);
    Ok(())
}
fn warning(h: &H, message: Vec<u16>) -> Result<(), Error> {
    record(h, "warning")?;
    h.borrow_mut().warnings.push(message);
    Ok(())
}
fn runtime() -> RT {
    RT {
        request,
        config,
        balance,
        positions,
        all,
        settled,
        public_only: |h| Ok(h.borrow().input["publicOnly"] == true),
        markets_by_symbol: |h| Ok(h.borrow().market_tag.clone()),
        market,
        get,
        optional_get: optional,
        truthy,
        equals_text,
        is_undefined: |v| matches!(v, V::Undefined),
        to_string: |_, v| Ok(string(v)),
        number: V::Number,
        undefined: || V::Undefined,
        null: || V::Null,
        to_number,
        array,
        object,
        length,
        present,
        at,
        iterator,
        next,
        close_on_error: |_, _, _| {},
        extract_array: extract,
        normalize_balance,
        normalize_order,
        normalize_algo_order: normalize_algo,
        is_array: |_, v| Ok(matches!(v, V::Array(_))),
        uniq_by_id: uniq,
        store_updater: |_| Ok(V::Undefined),
        fetched_leverage: |h| Ok(h.borrow().leverage.clone()),
        copy_properties,
        set_property,
        store_update,
        cache,
        config_setter: |_, cache| Ok(cache.clone()),
        set_config,
        push_method: |_, out| Ok(out.clone()),
        push_spread,
        error_message: |_, e| Ok(e.units.clone()),
        warning,
    }
}
fn tick(h: &H) {
    let mut due = Vec::new();
    {
        let mut s = h.borrow_mut();
        for (i, t) in s.tasks.iter_mut().enumerate() {
            if t.done.is_none() {
                t.left -= 1;
                if t.left == 0 {
                    due.push(i);
                }
            }
        }
        if s.input["reverseDue"] == true {
            due.reverse();
        }
    }
    for index in due {
        let key = h.borrow().tasks[index].key.clone();
        let stage = format!("{key}:end");
        let result = record(h, &stage);
        if result.is_ok() && key == "request:1" {
            let extra = decode(&h.borrow().input["appendSymbols"]);
            if let V::Array(extra) = extra {
                let s = h.borrow();
                let V::Array(a) = &s.symbols else {
                    panic!("symbols")
                };
                a.borrow_mut().extend(extra.borrow().clone());
            }
        }
        let mut s = h.borrow_mut();
        let result = result.map(|_| s.tasks[index].value.clone());
        s.tasks[index].done = Some(result);
        s.completed.push(index);
    }
}
fn run(h: &H, mut f: F<'_, V>) -> Result<V, Error> {
    let waker = Waker::noop().clone();
    let mut cx = Context::from_waker(&waker);
    let mut ticks = 0;
    let result = loop {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            break v;
        }
        tick(h);
        ticks += 1;
        assert!(ticks < 100, "native_scheduler_floor");
    };
    drop(f);
    while h.borrow().tasks.iter().any(|t| t.done.is_none()) {
        tick(h);
        ticks += 1;
        assert!(ticks < 100, "native_peer_floor");
    }
    result
}
fn decode_error(j: &J) -> Option<Error> {
    if j.is_null() {
        None
    } else {
        Some(Error {
            name: match j["name"].as_str().unwrap() {
                "RangeError" => "RangeError",
                "TypeError" => "TypeError",
                _ => panic!("error kind"),
            },
            units: serde_json::from_value(j["units"].clone()).unwrap(),
        })
    }
}
#[test]
fn native_account_reads_match_typescript() {
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/account-reads-oracle.cjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<J> = serde_json::from_slice(&output.stdout).unwrap();
    let expected_ids = [
        "max_cached",
        "max_remote",
        "max_default",
        "max_invalid",
        "max_failure",
        "leverage_cross",
        "leverage_isolated",
        "leverage_failure",
        "leverage_update_failure",
        "leverage_get_failure",
        "balance_array",
        "balance_nested",
        "balance_empty",
        "balance_failure",
        "balance_normalizer_failure",
        "balance_positions",
        "balance_positions_parallel",
        "balance_positions_failure",
        "balance_positions_dual_failure",
        "configs_public",
        "configs_rows",
        "configs_duplicate",
        "configs_failure",
        "configs_partial",
        "configs_null_row",
        "rows_all",
        "rows_empty_symbols",
        "rows_symbols",
        "rows_algo_page",
        "rows_live_symbols",
        "rows_failure",
        "normal_orders",
        "normal_normalizer_failure",
        "algo_orders",
        "algo_sparse_flatten",
        "algo_normalizer_failure",
        "orders_public",
        "orders_unique",
        "orders_normal_failure",
        "orders_algo_failure",
        "orders_both_failure",
        "max_factory_failure",
        "leverage_factory_failure",
        "balance_factory_failure",
        "configs_factory_failure",
        "rows_factory_failure",
        "balance_positions_first_factory_failure",
        "balance_positions_second_factory_failure",
        "balance_positions_aggregate_failure",
        "normal_capture_market",
        "algo_capture_market",
        "orders_reverse_success",
        "configs_capture_cache",
        "rows_push_failure",
    ];
    assert_eq!(
        cases
            .iter()
            .map(|c| c["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        expected_ids,
        "native_fixture_ids"
    );
    let rt = runtime();
    let mut traces = Vec::new();
    for c in &cases {
        let input = c["input"].clone();
        let symbols = decode(&input["symbols"]);
        let symbol = decode(&input["symbol"]);
        let cached_entries = decode(&input["cached"]);
        let h = RefCell::new(State {
            input: input.clone(),
            symbols: symbols.clone(),
            events: vec![],
            requests: vec![],
            tasks: vec![],
            completed: vec![],
            warnings: vec![],
            config_calls: vec![],
            cache: cached_entries.clone(),
            original_cache: cached_entries,
            leverage: decode(&input["fetchedLeverage"]),
            cache_index: 0,
            typed_configs: vec![],
            market_tag: text("OLD"),
        });
        let f = match c["method"].as_str().unwrap() {
            "fetchSymbolMaxLeverage" => fetch_symbol_max_leverage(&h, &rt, symbol),
            "fetchLeverageForSymbol" => fetch_leverage_for_symbol(&h, &rt, symbol),
            "fetchBalance" => fetch_balance(&h, &rt),
            "fetchBalanceAndPositions" => fetch_balance_and_positions(&h, &rt),
            "fetchSymbolConfigs" => fetch_symbol_configs(&h, &rt, symbol),
            "fetchOpenOrderRows" => {
                fetch_open_order_rows(&h, &rt, string(&decode(&input["endpoint"])), symbols)
            }
            "fetchNormalOrders" => fetch_normal_orders(&h, &rt, symbols),
            "fetchAlgoOrders" => fetch_algo_orders(&h, &rt, symbols),
            "fetchOrders" => fetch_orders(&h, &rt, symbols),
            _ => panic!("method"),
        };
        let prefix = h.borrow().events.clone();
        let result = run(&h, f);
        let id = c["id"].as_str().unwrap();
        assert_eq!(
            result.as_ref().err().cloned(),
            decode_error(&c["error"]),
            "native_error::{id}"
        );
        if let Ok(value) = &result {
            assert_eq!(*value, decode(&c["result"]), "native_result::{id}");
        }
        assert_eq!(
            cache(&h).unwrap(),
            decode(&c["cache"]),
            "native_cache::{id}"
        );
        let s = h.borrow();
        if c["method"] == "fetchSymbolConfigs" {
            if let Ok(V::Array(returned)) = &result {
                let V::Array(cached) = &s.cache else {
                    panic!("map identity")
                };
                assert!(Rc::ptr_eq(returned, cached), "native_cache_identity::{id}");
            }
        }
        assert_eq!(
            s.original_cache,
            decode(&c["originalCache"]),
            "native_captured_cache::{id}"
        );
        assert_eq!(
            s.leverage,
            decode(&c["leverage"]),
            "native_leverage_store::{id}"
        );
        let requests = c["requests"]
            .as_array()
            .unwrap()
            .iter()
            .map(|j| {
                let endpoint: Vec<u16> = serde_json::from_value(j["endpoint"].clone()).unwrap();
                let params = if j["params"].is_null() {
                    None
                } else {
                    Some(
                        j["params"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|j| {
                                let key = match j[0].as_str().unwrap() {
                                    "symbol" => "symbol",
                                    "limit" => "limit",
                                    "page" => "page",
                                    _ => panic!("key"),
                                };
                                (key, decode(&j[1]))
                            })
                            .collect(),
                    )
                };
                (endpoint, params)
            })
            .collect::<Vec<_>>();
        assert_eq!(s.requests, requests, "native_requests::{id}");
        let warnings: Vec<Vec<u16>> = serde_json::from_value(c["warnings"].clone()).unwrap();
        assert_eq!(s.warnings, warnings, "native_warning::{id}");
        let config_calls = c["configCalls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|j| (decode(&j["symbol"]), j["refetch"].as_bool().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(s.config_calls, config_calls, "native_config_calls::{id}");
        assert_eq!(
            s.tasks.iter().filter(|t| t.done.is_none()).count(),
            0,
            "native_peers_complete::{id}"
        );
        if id == "configs_rows" {
            assert_eq!(s.typed_configs.len(), 2, "native_config_floor");
            let first = &s.typed_configs[0];
            assert_eq!(
                first.margin_type,
                MarginType::Isolated,
                "native_config_margin"
            );
            assert_eq!(
                first.position_mode,
                PositionMode::Hedged,
                "native_config_mode"
            );
            assert_eq!(
                first.cross_leverage.to_bits(),
                7.0f64.to_bits(),
                "native_cross_leverage"
            );
            assert_eq!(
                first.isolated_long_leverage.to_bits(),
                8.0f64.to_bits(),
                "native_long_leverage"
            );
            assert_eq!(
                first.isolated_short_leverage.to_bits(),
                9.0f64.to_bits(),
                "native_short_leverage"
            );
            assert_eq!(
                s.typed_configs[1].cross_leverage.to_bits(),
                0.0f64.to_bits(),
                "native_zero_leverage"
            );
            assert_eq!(
                s.typed_configs[1].isolated_long_leverage.to_bits(),
                20.0f64.to_bits(),
                "native_invalid_default"
            );
            assert_eq!(
                s.typed_configs[1].isolated_short_leverage.to_bits(),
                20.0f64.to_bits(),
                "native_undefined_short_default"
            );
        }
        traces.push((
            id.to_owned(),
            prefix,
            s.events.clone(),
            c["prefix"].clone(),
            c["events"].clone(),
        ));
    }
    for (id, prefix, events, want_prefix, want_events) in traces {
        assert_eq!(
            prefix,
            serde_json::from_value::<Vec<String>>(want_prefix).unwrap(),
            "source_eager_effects::{id}"
        );
        assert_eq!(
            events,
            serde_json::from_value::<Vec<String>>(want_events).unwrap(),
            "source_effects::{id}"
        );
    }
}
