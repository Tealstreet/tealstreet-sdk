use safe_cex_num::{js_number, js_number_string, js_string, js_truthy, parse_float};
use safe_cex_venue_phemex::{rest_account_control::*, rest_order_control};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    pin::pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
type V = Rc<RefCell<Value>>;
type Rows = Vec<V>;
fn v(value: Value) -> V {
    Rc::new(RefCell::new(value))
}
fn absent() -> Value {
    json!({"$undefined":true})
}
fn units(value: &Value) -> Vec<u16> {
    if let Some(u) = value.get("$units").and_then(Value::as_array) {
        return u.iter().map(|v| v.as_u64().unwrap() as u16).collect();
    }
    value.as_str().unwrap().encode_utf16().collect()
}
fn unitobs(u: Vec<u16>) -> Value {
    json!({"units":u})
}
fn text(s: &str) -> Value {
    unitobs(s.encode_utf16().collect())
}
fn bits(f: f64) -> Value {
    json!({"bits":format!("{:016x}",if f.is_nan(){0x7ff8000000000000u64}else{f.to_bits()})})
}
fn object(fields: Vec<(Vec<u16>, Value)>) -> Value {
    let mut entries: Vec<_> = fields
        .into_iter()
        .map(|(k, v)| json!([unitobs(k), v]))
        .collect();
    entries.sort_by_key(|v| serde_json::to_string(&v[0]).unwrap());
    json!({"object":entries})
}
fn obj(fields: Vec<(&str, Value)>) -> Value {
    object(
        fields
            .into_iter()
            .map(|(k, v)| (k.encode_utf16().collect(), v))
            .collect(),
    )
}
fn obs(value: &Value) -> Value {
    if let Some(u) = value.get("$units") {
        return json!({"units":u});
    }
    if let Some(b) = value.get("$bits").and_then(Value::as_str) {
        return bits(f64::from_bits(u64::from_str_radix(b, 16).unwrap()));
    }
    if value.get("$undefined") == Some(&json!(true)) {
        return json!({"undefined":true});
    }
    match value {
        Value::Null => Value::Null,
        Value::Bool(v) => json!(v),
        Value::Number(v) => bits(v.as_f64().unwrap()),
        Value::String(v) => text(v),
        Value::Array(v) => json!(v.iter().map(obs).collect::<Vec<_>>()),
        Value::Object(v) => object(
            v.iter()
                .map(|(k, v)| (k.encode_utf16().collect(), obs(v)))
                .collect(),
        ),
    }
}
fn num(value: &Value) -> f64 {
    if let Some(b) = value.get("$bits").and_then(Value::as_str) {
        return f64::from_bits(u64::from_str_radix(b, 16).unwrap());
    }
    if value.get("$undefined").is_some() {
        return f64::NAN;
    }
    js_number(Some(value)).unwrap()
}
fn truth(value: &Value) -> bool {
    if value.get("$undefined").is_some() {
        false
    } else if value.get("$bits").is_some() {
        let f = num(value);
        f != 0.0 && !f.is_nan()
    } else {
        js_truthy(Some(value))
    }
}
fn same(a: &Value, b: &Value) -> bool {
    if (a.is_string() || a.get("$units").is_some()) && (b.is_string() || b.get("$units").is_some())
    {
        units(a) == units(b)
    } else {
        a == b
    }
}
fn key(row: &Value, field: &str) -> Value {
    let mut snake = String::new();
    for c in field.chars() {
        if c.is_uppercase() {
            snake.push('_');
            snake.push(c.to_ascii_lowercase());
        } else {
            snake.push(c)
        }
    }
    row.get(field)
        .or_else(|| row.get(&snake))
        .cloned()
        .unwrap_or(absent())
}
fn value_text(value: &Value) -> Vec<u16> {
    if value.get("$units").is_some() {
        return units(value);
    }
    if value.get("$undefined").is_some() {
        return "undefined".encode_utf16().collect();
    }
    if value.get("$bits").is_some() {
        return js_number_string(num(value)).encode_utf16().collect();
    }
    js_string(Some(value)).unwrap().encode_utf16().collect()
}
fn data_units(u: Vec<u16>) -> Value {
    String::from_utf16(&u).map_or_else(|_| json!({"$units":u}), |s| json!(s))
}
#[derive(Debug, Clone)]
struct Error {
    message: Vec<u16>,
    kind: &'static str,
    token: Rc<()>,
}
fn error(message: &str, kind: &'static str) -> Error {
    Error {
        message: message.encode_utf16().collect(),
        kind,
        token: Rc::new(()),
    }
}
async fn suspend() {
    let mut pending = true;
    std::future::poll_fn(|_| {
        if pending {
            pending = false;
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    })
    .await;
}
fn drive<T>(f: impl Future<Output = T>) -> (T, usize) {
    let mut f = pin!(f);
    let mut cx = Context::from_waker(Waker::noop());
    let mut count = 0;
    loop {
        match f.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return (v, count),
            Poll::Pending => {
                count += 1;
                assert!(count < 10, "REST bounded continuation");
            }
        }
    }
}
fn oracle(name: &str) -> Vec<Value> {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/rest-account-order-oracle.cjs"
        ))
        .arg(name)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
struct Host {
    row: Value,
    events: Vec<Value>,
    transport: Error,
    injected: Error,
    created: Vec<Error>,
    context: V,
    market: V,
    new_positions: V,
    store: Rows,
    raw: Rows,
    input: Rc<RefCell<Rows>>,
    cached: Option<V>,
    markets: V,
    stored_reads: usize,
    transport_replaced: bool,
}
impl Host {
    fn new(row: &Value) -> Self {
        Self {
            row: row.clone(),
            events: Vec::new(),
            transport: error("transport failed", "transport"),
            injected: error("injected failure", "injected"),
            created: Vec::new(),
            context: v(row["context"].clone()),
            market: v(row["market"].clone()),
            new_positions: v(json!([{"symbol":row["symbol"],"side":"long","tag":"new"}])),
            store: row["stored"]
                .as_array()
                .unwrap()
                .iter()
                .cloned()
                .map(v)
                .collect(),
            raw: row["raw"]
                .as_array()
                .unwrap()
                .iter()
                .cloned()
                .map(v)
                .collect(),
            input: Rc::new(RefCell::new(
                row["orders"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .cloned()
                    .map(v)
                    .collect(),
            )),
            cached: row["fixture"].get("cached").cloned().map(v),
            markets: v(row["fixture"].get("markets").cloned().unwrap_or(
                json!([{"symbol":"BTCUSDT","limits":{"leverage":{"min":1,"max":100}}}]),
            )),
            stored_reads: 0,
            transport_replaced: false,
        }
    }
    fn flag(&self, name: &str) -> bool {
        self.row["fixture"][name] == true
    }
    fn note(&mut self, name: &str, args: Vec<Value>) -> Result<(), Error> {
        let mut event = vec![json!(name)];
        event.extend(args);
        let event = json!(event);
        assert_eq!(
            event,
            self.row["events"][self.events.len()],
            "REST native effect {name}; {} {}",
            self.row["name"],
            self.row["fixture"]
        );
        self.events.push(event);
        if self.row["fixture"]["fail"] == name {
            Err(self.injected.clone())
        } else {
            Ok(())
        }
    }
    fn report_error(&mut self, e: &Error) -> Result<(), Error> {
        if e.kind == "transport" {
            assert!(
                Rc::ptr_eq(&e.token, &self.transport.token),
                "REST native transport error identity"
            )
        }
        if e.kind == "injected" {
            assert!(
                Rc::ptr_eq(&e.token, &self.injected.token),
                "REST native injected error identity"
            )
        }
        if e.kind == "created" {
            assert!(
                self.created
                    .iter()
                    .any(|created| Rc::ptr_eq(&e.token, &created.token)),
                "REST native created error identity"
            )
        };
        self.note(
            "report",
            vec![
                unitobs(e.message.clone()),
                unitobs(if e.kind == "transport" {
                    "venue failure".encode_utf16().collect()
                } else {
                    e.message.clone()
                }),
            ],
        )
    }
    fn error_value(&mut self, message: Value) -> Result<Error, Error> {
        self.note("error", vec![obs(&message)])?;
        let message = if message.get("$undefined").is_some() {
            Vec::new()
        } else {
            value_text(&message)
        };
        let e = Error {
            message,
            kind: "created",
            token: Rc::new(()),
        };
        self.created.push(e.clone());
        Ok(e)
    }
    fn format_value(&mut self, symbol: &V) -> Result<V, Error> {
        self.note("format", vec![obs(&symbol.borrow())])?;
        if self.flag("switchTransport") && value_text(&symbol.borrow()).starts_with(&[78, 58]) {
            self.transport_replaced = true;
        }
        if self.row["name"] == "cancelOrders" {
            let mut pref = "N:".encode_utf16().collect::<Vec<_>>();
            pref.extend(value_text(&symbol.borrow()));
            Ok(v(data_units(pref)))
        } else {
            Ok(symbol.clone())
        }
    }
    fn check(&self, result: Result<Value, Error>) {
        let observed = match result {
            Ok(value) => json!({"ok":value}),
            Err(e) => json!({"error":unitobs(e.message),"identity":e.kind}),
        };
        assert_eq!(
            observed, self.row["result"],
            "REST native result; {} {}",
            self.row["name"], self.row["fixture"]
        );
        assert_eq!(
            json!(self.events),
            self.row["events"],
            "REST native complete effects"
        );
    }
    fn cache_update(&mut self, symbol: &V, entry: Value) -> Result<(), Error> {
        let mut fields = Vec::new();
        let sym = value_text(&symbol.borrow());
        for (key, value) in self.row["cache"].as_object().unwrap() {
            let key: Vec<_> = key.encode_utf16().collect();
            if key != sym {
                fields.push((key, obs(value)));
            }
        }
        fields.push((sym, entry));
        self.note(
            "update",
            vec![obj(vec![("fetchedLeverage", object(fields))])],
        )
    }
    fn existing(&self, symbol: &V, field: &str) -> Option<Value> {
        let sym = String::from_utf16(&value_text(&symbol.borrow())).ok()?;
        self.row["cache"]
            .get(&sym)?
            .get(field)
            .filter(|v| !v.is_null() && v.get("$undefined").is_none())
            .cloned()
    }
}
impl ContextHost for Host {
    type Symbol = V;
    type Row = V;
    type Rows = Rows;
    type Reply = ();
    type Call = bool;
    type Value = Value;
    type Error = Error;
    fn ends_usdc(&mut self, symbol: &V) -> Result<bool, Error> {
        Ok(value_text(&symbol.borrow()).ends_with(&"USDC".encode_utf16().collect::<Vec<_>>()))
    }
    fn format_out(&mut self, symbol: &V) -> Result<Value, Error> {
        self.note("format", vec![obs(&symbol.borrow())])?;
        if self.flag("switchTransport") {
            self.transport_replaced = true;
        }
        Ok(symbol.borrow().clone())
    }
    fn get_target(&mut self) -> Result<bool, Error> {
        Ok(self.transport_replaced)
    }
    async fn get(
        &mut self,
        call: bool,
        endpoint: &'static str,
        currency: &'static str,
        symbol: Value,
    ) -> Result<(), Error> {
        assert!(!call, "REST native retained GET target");
        self.note(
            "get",
            vec![
                text(endpoint),
                obs(&json!({"params":{"currency":currency,"symbol":symbol}})),
            ],
        )?;
        if self.flag("getError") {
            return Err(self.transport.clone());
        }
        Ok(())
    }
    fn raw_rows(&mut self, _: ()) -> Result<Rows, Error> {
        Ok(self.raw.clone())
    }
    fn stored_rows(&mut self) -> Result<Rows, Error> {
        self.stored_reads += 1;
        if self.flag("storedErrorOnce") && self.stored_reads == 1 {
            return Err(self.transport.clone());
        }
        Ok(self.store.clone())
    }
    fn length(&mut self, rows: &Rows) -> Result<usize, Error> {
        Ok(rows.len())
    }
    fn row(&mut self, rows: &Rows, index: usize) -> Result<Option<V>, Error> {
        Ok(rows.get(index).cloned())
    }
    fn matches(&mut self, row: &V, symbol: &V, formatted: bool) -> Result<bool, Error> {
        let raw = row.borrow().get("symbol").cloned().unwrap_or(absent());
        if formatted {
            self.note("formatIn", vec![obs(&raw)])?;
            let upper = data_units(
                value_text(&raw)
                    .iter()
                    .map(|u| if (97..=122).contains(u) { u - 32 } else { *u })
                    .collect(),
            );
            Ok(same(&upper, &symbol.borrow()))
        } else {
            Ok(same(&raw, &symbol.borrow()))
        }
    }
    fn side_is(&mut self, row: &V, field: &'static str, side: &'static str) -> Result<bool, Error> {
        Ok(key(&row.borrow(), field) == side)
    }
    fn stored_margin(&mut self, row: &V) -> Result<Option<Value>, Error> {
        Ok(row
            .borrow()
            .get("marginType")
            .filter(|v| v.get("$undefined").is_none())
            .cloned())
    }
    fn stored_mode(&mut self, row: &V) -> Result<Option<Value>, Error> {
        Ok(row
            .borrow()
            .get("positionMode")
            .filter(|v| v.get("$undefined").is_none())
            .cloned())
    }
    fn stored_leverage(&mut self, row: &V) -> Result<f64, Error> {
        Ok(num(&row
            .borrow()
            .get("leverage")
            .cloned()
            .unwrap_or(absent())))
    }
    fn raw_leverage(&mut self, row: &V) -> Result<f64, Error> {
        let value = key(&row.borrow(), "leverageRr");
        Ok(parse_float(&String::from_utf16_lossy(&value_text(&value))))
    }
    fn cross_margin(&mut self, row: &V) -> Result<bool, Error> {
        Ok(truth(&key(&row.borrow(), "crossMargin")))
    }
    fn raw_hedged(&mut self, row: &V) -> Result<bool, Error> {
        Ok(key(&row.borrow(), "posMode") == "Hedged")
    }
    fn value(&mut self, label: &'static str) -> Result<Value, Error> {
        Ok(json!(label))
    }
    fn report(&mut self, error: &Error) -> Result<(), Error> {
        self.report_error(error)
    }
}
impl SettingsHost for Host {
    type Symbol = V;
    type Value = Value;
    type Market = V;
    type Context = V;
    type Reply = Value;
    type Positions = V;
    type Call = bool;
    type Error = Error;
    fn symbol_truthy(&mut self, symbol: &V) -> Result<bool, Error> {
        Ok(truth(&symbol.borrow()))
    }
    fn symbol_text(&mut self, symbol: &V) -> Result<Vec<u16>, Error> {
        Ok(value_text(&symbol.borrow()))
    }
    fn message(&mut self, text: Vec<u16>) -> Result<Value, Error> {
        Ok(data_units(text))
    }
    fn emit(&mut self, message: Value) -> Result<(), Error> {
        self.note("emit", vec![obs(&message)])
    }
    fn error(&mut self, message: Value) -> Result<Error, Error> {
        self.error_value(message)
    }
    fn format(&mut self, symbol: &V) -> Result<Value, Error> {
        self.note("format", vec![obs(&symbol.borrow())])?;
        if self.flag("switchTransport") {
            self.transport_replaced = true;
        }
        Ok(symbol.borrow().clone())
    }
    fn market(&mut self, symbol: &V) -> Result<Option<V>, Error> {
        self.note("market", vec![obs(&symbol.borrow())])?;
        Ok(if self.market.borrow().is_null() {
            None
        } else {
            Some(self.market.clone())
        })
    }
    async fn context(&mut self, symbol: &V) -> Result<Option<V>, Error> {
        self.note("context", vec![obs(&symbol.borrow())])?;
        Ok(if self.context.borrow().is_null() {
            None
        } else {
            Some(self.context.clone())
        })
    }
    fn long(&mut self, context: &V) -> Result<f64, Error> {
        Ok(num(&context.borrow()["longLeverage"]))
    }
    fn short(&mut self, context: &V) -> Result<f64, Error> {
        Ok(num(&context.borrow()["shortLeverage"]))
    }
    fn cross(&mut self, context: &V) -> Result<bool, Error> {
        Ok(context.borrow()["marginType"] == "cross")
    }
    fn hedged(&mut self, context: &V) -> Result<bool, Error> {
        Ok(context.borrow()["positionMode"] == "hedged")
    }
    fn min(&mut self, market: &V) -> Result<f64, Error> {
        assert!(
            Rc::ptr_eq(market, &self.market),
            "REST native retained market"
        );
        Ok(num(&market.borrow()["limits"]["leverage"]["min"]))
    }
    fn max(&mut self, market: &V) -> Result<f64, Error> {
        Ok(num(&market.borrow()["limits"]["leverage"]["max"]))
    }
    fn put_target(&mut self) -> Result<bool, Error> {
        Ok(self.transport_replaced)
    }
    async fn put(
        &mut self,
        call: bool,
        endpoint: &'static str,
        symbol: Value,
        mode: Option<&'static str>,
        leverages: Vec<(&'static str, f64)>,
    ) -> Result<Value, Error> {
        assert!(!call, "REST native retained PUT target");
        let mut params = vec![("symbol", obs(&symbol))];
        if let Some(mode) = mode {
            params.push(("targetPosMode", text(mode)))
        }
        for (key, value) in leverages {
            params.push((key, bits(value)))
        }
        self.note(
            "put",
            vec![
                text(endpoint),
                obs(&absent()),
                obj(vec![("params", obj(params))]),
            ],
        )?;
        if self.flag("putError") {
            return Err(self.transport.clone());
        }
        if self.flag("mutateMarket") {
            suspend().await;
            self.market.borrow_mut()["limits"]["leverage"]["max"] = json!(4);
        }
        Ok(self.row["fixture"]
            .get("reply")
            .cloned()
            .unwrap_or(json!({"data":{"code":0}})))
    }
    fn zero_code(&mut self, reply: &Value, _: bool) -> Result<bool, Error> {
        Ok(reply
            .get("data")
            .and_then(|v| v.get("code"))
            .is_some_and(|v| v.is_number() && num(v) == 0.0))
    }
    fn reply_message(&mut self, reply: &Value, _: bool) -> Result<Value, Error> {
        Ok(reply
            .get("data")
            .and_then(|v| v.get("msg"))
            .cloned()
            .unwrap_or(absent()))
    }
    async fn fetch_positions(&mut self, symbol: &V) -> Result<V, Error> {
        self.note("fetchPositions", vec![json!([obs(&symbol.borrow())])])?;
        Ok(self.new_positions.clone())
    }
    fn replace_symbol_positions(&mut self, symbol: &V, positions: V) -> Result<(), Error> {
        assert!(
            Rc::ptr_eq(&positions, &self.new_positions),
            "REST native replacement positions identity"
        );
        let mut updated = self
            .store
            .iter()
            .filter(|row| !same(&row.borrow()["symbol"], &symbol.borrow()))
            .map(|row| obs(&row.borrow()))
            .collect::<Vec<_>>();
        updated.extend(positions.borrow().as_array().unwrap().iter().map(obs));
        self.note("update", vec![obj(vec![("positions", json!(updated))])])
    }
    fn add_positions(&mut self, positions: V) -> Result<(), Error> {
        assert!(
            Rc::ptr_eq(&positions, &self.new_positions),
            "REST native refreshed positions identity"
        );
        self.note("addPositions", vec![obs(&positions.borrow())])
    }
    fn record_mode(&mut self, symbol: &V, context: &V) -> Result<(), Error> {
        let entry = obj(vec![
            (
                "buyLeverage",
                self.existing(symbol, "buyLeverage")
                    .map_or_else(|| obs(&context.borrow()["longLeverage"]), |v| obs(&v)),
            ),
            (
                "sellLeverage",
                self.existing(symbol, "sellLeverage")
                    .map_or_else(|| obs(&context.borrow()["shortLeverage"]), |v| obs(&v)),
            ),
            ("marginType", obs(&context.borrow()["marginType"])),
            ("positionMode", obs(&context.borrow()["positionMode"])),
        ]);
        self.cache_update(symbol, entry)
    }
    fn record_leverage(
        &mut self,
        symbol: &V,
        context: &V,
        side: Option<&Value>,
        applied: f64,
        signed: f64,
    ) -> Result<(), Error> {
        let buy = if side == Some(&json!("short")) {
            self.existing(symbol, "buyLeverage")
                .map_or_else(|| bits(applied), |v| obs(&v))
        } else {
            bits(applied)
        };
        let sell = if side == Some(&json!("long")) {
            self.existing(symbol, "sellLeverage")
                .map_or_else(|| bits(applied), |v| obs(&v))
        } else {
            bits(applied)
        };
        self.cache_update(
            symbol,
            obj(vec![
                ("buyLeverage", buy),
                ("sellLeverage", sell),
                (
                    "marginType",
                    text(if signed < 0.0 { "cross" } else { "isolated" }),
                ),
                ("positionMode", obs(&context.borrow()["positionMode"])),
            ]),
        )
    }
    fn side_truthy(&mut self, side: Option<&Value>) -> Result<bool, Error> {
        Ok(side.is_some_and(truth))
    }
    fn side_long(&mut self, side: Option<&Value>) -> Result<bool, Error> {
        Ok(side == Some(&json!("long")))
    }
    async fn helper(
        &mut self,
        symbol: V,
        leverage: f64,
        side: Option<Value>,
        allow: Option<bool>,
    ) -> Result<(), Error> {
        let mut args = vec![
            obs(&symbol.borrow()),
            bits(leverage),
            side.map_or_else(|| obs(&absent()), |v| obs(&v)),
        ];
        if let Some(allow) = allow {
            args.push(json!(allow))
        }
        self.note("helper", args)
    }
    fn report(&mut self, error: &Error) -> Result<(), Error> {
        self.report_error(error)
    }
}
impl rest_order_control::CancellationHost for Host {
    type Input = Rc<RefCell<Rows>>;
    type Order = V;
    type Value = V;
    type Reply = Value;
    type Call = bool;
    type Error = Error;
    fn input_length(&mut self, input: &Self::Input) -> Result<usize, Error> {
        Ok(input.borrow().len())
    }
    fn input_at(&mut self, input: &Self::Input, index: usize) -> Result<Option<V>, Error> {
        Ok(input.borrow().get(index).cloned())
    }
    fn spread(&mut self, order: &V) -> Result<V, Error> {
        self.note("spread", vec![])?;
        let mut copy = order.borrow().clone();
        copy["extra"] = json!(7);
        Ok(v(copy))
    }
    fn id(&mut self, order: &V) -> Result<Vec<u16>, Error> {
        Ok(units(&order.borrow()["id"]))
    }
    fn set_id(&mut self, order: &mut V, id: Vec<u16>) -> Result<(), Error> {
        order.borrow_mut()["id"] = data_units(id);
        Ok(())
    }
    fn missing_order(&mut self) -> Result<V, Error> {
        Ok(v(absent()))
    }
    async fn position_side(&mut self, order: &V) -> Result<V, Error> {
        self.note("side", vec![obs(&order.borrow())])?;
        if self.flag("mutateCopy") {
            suspend().await;
            order.borrow_mut()["price"] = json!(103);
        }
        let side = order
            .borrow()
            .get("info")
            .and_then(|i| i.get("posSide"))
            .filter(|v| !v.is_null())
            .cloned()
            .unwrap_or(json!("Merged"));
        Ok(v(side))
    }
    fn with_side(&mut self, order: &V, side: V) -> Result<V, Error> {
        let mut copy = order.borrow().clone();
        copy["posSide"] = side.borrow().clone();
        Ok(v(copy))
    }
    fn side(&mut self, order: &V) -> Result<V, Error> {
        Ok(v(order.borrow()["posSide"].clone()))
    }
    fn symbol(&mut self, order: &V) -> Result<V, Error> {
        Ok(v(order.borrow()["symbol"].clone()))
    }
    fn value_text(&mut self, value: V) -> Result<Vec<u16>, Error> {
        Ok(value_text(&value.borrow()))
    }
    fn format(&mut self, symbol: V) -> Result<V, Error> {
        self.format_value(&symbol)
    }
    fn format_value_ref(&mut self, symbol: &V) -> Result<V, Error> {
        self.format_value(symbol)
    }
    fn clone_value_ref(&mut self, value: &V) -> Result<V, Error> {
        Ok(value.clone())
    }
    fn delete_target(&mut self) -> Result<bool, Error> {
        Ok(self.transport_replaced)
    }
    async fn delete(
        &mut self,
        call: bool,
        endpoint: &'static str,
        symbol: V,
        side: V,
        id: Vec<u16>,
    ) -> Result<Value, Error> {
        assert!(!call, "REST native retained DELETE target");
        let params = obj(vec![
            ("symbol", obs(&symbol.borrow())),
            ("posSide", obs(&side.borrow())),
            ("orderID", unitobs(id)),
        ]);
        self.note(
            "delete",
            vec![text(endpoint), obj(vec![("params", params)])],
        )?;
        if self.flag("deleteError") {
            return Err(self.transport.clone());
        }
        if self.flag("mutateInput") {
            suspend().await;
            let mut order = self.row["orders"][0].clone();
            order["id"] = json!("added");
            self.input.borrow_mut().push(v(order));
        }
        Ok(self.row["fixture"]
            .get("reply")
            .cloned()
            .unwrap_or(json!({"data":{"code":0}})))
    }

    fn flatten(&mut self, replies: Vec<Value>, _: bool) -> Result<Vec<Value>, Error> {
        Ok(replies)
    }
    fn zero_code(&mut self, reply: &Value) -> Result<bool, Error> {
        Ok(reply["data"]["code"].is_number() && num(&reply["data"]["code"]) == 0.0)
    }
    fn message(&mut self, reply: &Value) -> Result<V, Error> {
        Ok(v(reply
            .get("data")
            .and_then(|v| v.get("msg"))
            .cloned()
            .unwrap_or(absent())))
    }
    fn error(&mut self, message: V) -> Result<Error, Error> {
        let value = message.borrow().clone();
        self.error_value(value)
    }
    fn status(&mut self, ids: Vec<Option<Vec<u16>>>, status: &'static str) -> Result<(), Error> {
        self.note(
            "status",
            vec![
                json!(
                    ids.into_iter()
                        .map(|id| id.map_or(Value::Null, |id| obj(vec![("id", unitobs(id))])))
                        .collect::<Vec<_>>()
                ),
                text(status),
            ],
        )
    }
    fn report(&mut self, error: &Error) -> Result<(), Error> {
        self.report_error(error)
    }
}
impl rest_order_control::MarketLookupHost for Host {
    type Symbol = V;
    type Market = V;
    type Markets = V;
    type Loaded = V;
    type Error = Error;
    fn cached(&mut self, symbol: &V) -> Result<Option<V>, Error> {
        self.note("market", vec![obs(&symbol.borrow())])?;
        Ok(self.cached.clone())
    }
    async fn fetch(&mut self) -> Result<V, Error> {
        self.note("fetchMarkets", vec![])?;
        if self.flag("replaceCache") {
            suspend().await;
            self.cached = Some(v(
                json!({"symbol":"BTCUSDT","limits":{"leverage":{"min":1,"max":100}},"tag":"replacement"}),
            ));
        }
        Ok(self.markets.clone())
    }
    fn disposed(&mut self) -> Result<bool, Error> {
        Ok(self.flag("disposed"))
    }
    fn is_array(&mut self, markets: &V) -> Result<bool, Error> {
        Ok(markets.borrow().is_array())
    }
    fn length(&mut self, markets: &V) -> Result<usize, Error> {
        Ok(markets.borrow().as_array().unwrap().len())
    }
    fn loaded(&mut self) -> Result<V, Error> {
        self.note("loaded", vec![])?;
        Ok(v(json!({"markets":false,"custom":7})))
    }
    fn update(&mut self, markets: &V, loaded: V, flag: (&'static str, bool)) -> Result<(), Error> {
        assert!(
            Rc::ptr_eq(markets, &self.markets),
            "REST native markets identity"
        );
        let mut loaded = loaded.borrow().clone();
        loaded[flag.0] = json!(flag.1);
        self.note(
            "update",
            vec![obs(&json!({"markets":*markets.borrow(),"loaded":loaded}))],
        )
    }
    fn find(&mut self, markets: &V, symbol: &V) -> Result<Option<V>, Error> {
        let markets = markets.borrow();
        let Some(markets) = markets.as_array() else {
            return Err(Error {
                message: "Cannot read properties of null (reading 'find')"
                    .encode_utf16()
                    .collect(),
                kind: "created",
                token: Rc::new(()),
            });
        };
        Ok(markets
            .iter()
            .find(|m| same(&m["symbol"], &symbol.borrow()))
            .cloned()
            .map(v))
    }
}
fn run(name: &str, count: usize) {
    let cases = oracle(name);
    assert_eq!(cases.len(), count, "REST source case census");
    let mut suspended = 0;
    for row in cases {
        let mut host = Host::new(&row);
        let symbol = v(row["symbol"].clone());
        let leverage = row["fixture"].get("leverage").map_or(15.0, num);
        let side = row["fixture"].get("side").cloned();
        let observed: Result<Value, Error> = match name {
            "resolveLeverageContext" => {
                drive(resolve_leverage_context(&mut host, symbol))
                    .0
                    .map(|ctx| {
                        ctx.map_or_else(
                            || obs(&absent()),
                            |ctx| {
                                obj(vec![
                                    ("marginType", obs(&ctx.margin_type)),
                                    ("positionMode", obs(&ctx.position_mode)),
                                    ("longLeverage", bits(ctx.long_leverage)),
                                    ("shortLeverage", bits(ctx.short_leverage)),
                                ])
                            },
                        )
                    })
            }
            "changePositionMode" => drive(change_position_mode(
                &mut host,
                row["fixture"]["hedged"] != false,
                symbol,
            ))
            .0
            .map(|_| obs(&absent())),
            "changeMarginType" => drive(change_margin_type(
                &mut host,
                row["fixture"]["isolated"] != false,
                symbol,
            ))
            .0
            .map(|_| obs(&absent())),
            "setLeverage" => drive(set_leverage(&mut host, symbol, leverage, side))
                .0
                .map(|_| obs(&absent())),
            "setLeverageHelper" => {
                let (result, pending) = drive(set_leverage_helper(
                    &mut host,
                    symbol,
                    leverage,
                    side,
                    row["fixture"]["allow"] == true,
                ));
                suspended += pending;
                result.map(|_| obs(&absent()))
            }
            "cancelOrders" => {
                let input = host.input.clone();
                let (result, pending) = drive(rest_order_control::cancel_orders(&mut host, input));
                suspended += pending;
                result.map(|_| obs(&absent()))
            }
            "getOrFetchMarketForSymbol" => {
                let (result, pending) =
                    drive(rest_order_control::get_or_fetch_market(&mut host, symbol));
                suspended += pending;
                result.map(|market| {
                    market.map_or_else(
                        || obs(&absent()),
                        |market| {
                            if let Some(cached) = &host.cached {
                                assert!(
                                    Rc::ptr_eq(cached, &market),
                                    "REST native cached market identity"
                                );
                            }
                            obs(&market.borrow())
                        },
                    )
                })
            }
            _ => panic!("Unknown source member"),
        };
        host.check(observed);
    }
    if matches!(
        name,
        "setLeverageHelper" | "cancelOrders" | "getOrFetchMarketForSymbol"
    ) {
        assert!(suspended > 0, "REST native actual Pending");
    }
}
#[test]
fn actual_typescript_context() {
    run("resolveLeverageContext", 18)
}
#[test]
fn actual_typescript_position_mode() {
    run("changePositionMode", 11)
}
#[test]
fn actual_typescript_margin_type() {
    run("changeMarginType", 8)
}
#[test]
fn actual_typescript_leverage_delegate() {
    run("setLeverage", 4)
}
#[test]
fn actual_typescript_leverage_helper() {
    run("setLeverageHelper", 21)
}
#[test]
fn actual_typescript_cancel_orders() {
    run("cancelOrders", 13)
}
#[test]
fn actual_typescript_market_lookup() {
    run("getOrFetchMarketForSymbol", 9)
}
