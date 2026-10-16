use safe_cex_types::PositionMode;
use safe_cex_venue_phemex::{position_control::*, session_control::*};
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
type Rows = Rc<RefCell<Vec<Option<V>>>>;
fn v(value: Value) -> V {
    Rc::new(RefCell::new(value))
}
fn absent() -> Value {
    json!({"$undefined":true})
}
fn bits(value: f64) -> Value {
    json!({"bits":format!("{:016x}",if value.is_nan(){0x7ff8000000000000u64}else{value.to_bits()})})
}
fn units(value: &Value) -> Vec<u16> {
    if let Some(units) = value.get("$units").and_then(Value::as_array) {
        units.iter().map(|v| v.as_u64().unwrap() as u16).collect()
    } else {
        value.as_str().unwrap().encode_utf16().collect()
    }
}
fn obs(value: &Value) -> Value {
    if let Some(units) = value.get("$units") {
        return json!({"units":units});
    }
    if value.get("$undefined") == Some(&json!(true)) {
        return json!({"undefined":true});
    }
    match value {
        Value::Null => Value::Null,
        Value::Bool(v) => json!(v),
        Value::Number(v) => bits(v.as_f64().unwrap()),
        Value::String(v) => json!({"units":v.encode_utf16().collect::<Vec<_>>()}),
        Value::Array(v) => json!(v.iter().map(obs).collect::<Vec<_>>()),
        Value::Object(v) => Value::Object(v.iter().map(|(k, v)| (k.clone(), obs(v))).collect()),
    }
}
fn text(value: &str) -> Value {
    obs(&json!(value))
}
fn same_value(a: &Value, b: &Value) -> bool {
    if (a.is_string() || a.get("$units").is_some()) && (b.is_string() || b.get("$units").is_some())
    {
        units(a) == units(b)
    } else {
        a == b
    }
}
#[derive(Debug, Clone)]
struct Error {
    message: &'static str,
    venue: bool,
    token: Rc<()>,
}
fn failure() -> Error {
    Error {
        message: "injected failure",
        venue: true,
        token: Rc::new(()),
    }
}
fn oracle(name: &str) -> Vec<Value> {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/session-position-oracle.cjs"
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
fn drive<T>(future: impl Future<Output = T>) -> (T, usize) {
    let mut future = pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    let mut pending = 0;
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(value) => return (value, pending),
            Poll::Pending => {
                pending += 1;
                assert!(pending <= 3, "PH session bounded suspension");
            }
        }
    }
}
async fn suspend() {
    let mut first = true;
    std::future::poll_fn(|_| {
        if first {
            first = false;
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    })
    .await;
}
struct Host {
    row: Value,
    events: Vec<Value>,
    disposed: bool,
    error: Error,
    balance: V,
    positions: V,
    loaded: V,
    options: V,
    pinned: Option<V>,
    info_reads: usize,
    callback: Rc<()>,
    socket: Rc<()>,
    first_socket: Rc<()>,
    second_socket: Rc<()>,
    handler: Rc<()>,
    listener: Rc<()>,
    scheduled: bool,
    raw_rows: Rows,
    mode_positions: V,
    mapper: Rc<()>,
    original_mapper: Rc<()>,
}
impl Host {
    fn new(row: &Value) -> Self {
        let socket = Rc::new(());
        let mapper = Rc::new(());
        let data_rows = row["data"]
            .get("positions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let options = v(row["options"].clone());
        let pinned = options
            .borrow()
            .get("info")
            .and_then(|v| v.get("posSide"))
            .cloned()
            .map(v);
        Self {
            row: row.clone(),
            events: Vec::new(),
            disposed: row["fixture"]["disposed"] == true,
            error: failure(),
            balance: v(json!({"token":"balance"})),
            positions: v(json!({"token":"positions"})),
            loaded: v(row["loaded"].clone()),
            options,
            pinned,
            info_reads: 0,
            callback: Rc::new(()),
            socket: socket.clone(),
            first_socket: socket,
            second_socket: Rc::new(()),
            handler: Rc::new(()),
            listener: Rc::new(()),
            scheduled: false,
            raw_rows: Rc::new(RefCell::new(
                data_rows.into_iter().map(|r| Some(v(r))).collect(),
            )),
            mode_positions: v(row["positions"].clone()),
            mapper: mapper.clone(),
            original_mapper: mapper,
        }
    }
    fn flag(&self, key: &str) -> bool {
        self.row["fixture"][key] == true
    }
    fn note(&mut self, name: &str, args: Vec<Value>) -> Result<(), Error> {
        let mut event = vec![json!(name)];
        event.extend(args);
        let event = json!(event);
        assert_eq!(
            event,
            self.row["events"][self.events.len()],
            "PH native effect {name}; {} {}",
            self.row["name"],
            self.row["fixture"]
        );
        self.events.push(event);
        if self.row["fixture"]["fail"] == name {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
    fn end(&self) {
        assert_eq!(
            json!(self.events),
            self.row["events"],
            "PH native complete effects"
        );
    }
    fn symbol_value(&self) -> V {
        v(self.row["symbol"].clone())
    }
    fn reason(&self, error: &Error) -> Value {
        text(if error.venue {
            "venue failure"
        } else {
            error.message
        })
    }
    fn check_result(&self, output: Result<Value, Error>) {
        let result = match output {
            Ok(value) => json!({"ok":value}),
            Err(e) => {
                let identity = Rc::ptr_eq(&e.token, &self.error.token);
                assert_eq!(
                    json!(identity),
                    self.row["outcome"]["identity"],
                    "PH native original error identity"
                );
                json!({"error":text(e.message),"identity":identity})
            }
        };
        assert_eq!(
            result, self.row["outcome"],
            "PH native result; {} {}",
            self.row["name"], self.row["fixture"]
        );
    }
}
impl SessionHost for Host {
    type Value = V;
    type Error = Error;
    fn base_dispose(&mut self) -> Result<(), Error> {
        self.note("base", vec![])
    }
    fn socket_dispose(&mut self, socket: &'static str) -> Result<(), Error> {
        self.note("socketDispose", vec![text(socket)])
    }
    fn connect_private(&mut self) -> Result<(), Error> {
        self.note("connect", vec![])
    }
    fn set_setting(&mut self, key: &'static str, value: bool) -> Result<(), Error> {
        self.note("setting", vec![text(key), json!(value)])
    }
    async fn initial_tick(&mut self) -> Result<(), Error> {
        self.note("initialTick", vec![])?;
        if self.flag("disposeAfterTick") {
            suspend().await;
            self.disposed = true;
        }
        Ok(())
    }
    fn disposed(&mut self) -> Result<bool, Error> {
        Ok(self.disposed)
    }
    fn log(&mut self, text_value: &'static str) -> Result<(), Error> {
        self.note("log", vec![text(text_value)])
    }
    fn empty_array(&mut self) -> Result<V, Error> {
        Ok(v(json!([])))
    }
    fn zero_balance(&mut self, fields: [(&'static str, f64); 4]) -> Result<V, Error> {
        Ok(v(Value::Object(
            fields
                .into_iter()
                .map(|(key, value)| (key.into(), json!(value)))
                .collect(),
        )))
    }
    fn loaded(&mut self) -> Result<V, Error> {
        self.note("loaded", vec![])?;
        Ok(self.loaded.clone())
    }
    fn update(
        &mut self,
        fields: Vec<(&'static str, V)>,
        loaded: V,
        flags: Vec<&'static str>,
    ) -> Result<(), Error> {
        assert!(
            Rc::ptr_eq(&loaded, &self.loaded),
            "PH native loaded identity"
        );
        if fields.len() == 1 {
            assert!(
                Rc::ptr_eq(&fields[0].1, &self.balance),
                "PH native balance identity"
            );
        } else {
            assert!(
                !Rc::ptr_eq(&fields[0].1, &fields[2].1),
                "PH native independently allocated arrays"
            );
        }
        let mut patch = serde_json::Map::new();
        for (key, value) in fields {
            patch.insert(key.into(), value.borrow().clone());
        }
        let mut loaded = loaded.borrow().clone();
        for flag in flags {
            loaded[flag] = json!(true);
        }
        patch.insert("loaded".into(), loaded);
        self.note("update", vec![obs(&Value::Object(patch))])
    }
    async fn fetch_positions_balance(&mut self) -> Result<(V, V), Error> {
        self.note("fetchBalance", vec![])?;
        if self.flag("fetchError") {
            return Err(self.error.clone());
        }
        if self.flag("disposeAfterFetch") {
            suspend().await;
            self.disposed = true;
        }
        Ok((self.balance.clone(), self.positions.clone()))
    }
    fn add_positions(&mut self, positions: V) -> Result<(), Error> {
        assert!(
            Rc::ptr_eq(&positions, &self.positions),
            "PH native positions identity"
        );
        self.note("addPositions", vec![obs(&positions.borrow())])
    }
    fn report_message(&mut self, error: &Error) -> Result<(), Error> {
        assert!(
            Rc::ptr_eq(&error.token, &self.error.token),
            "PH native caught error identity"
        );
        self.note("report", vec![text(error.message)])?;
        if self.flag("disposeAfterError") {
            self.disposed = true;
        }
        Ok(())
    }
    fn tick_interval(&mut self) -> Result<V, Error> {
        self.note("interval", vec![])?;
        Ok(v(json!(987)))
    }
    fn schedule_tick(&mut self, interval: V, method: &'static str) -> Result<(), Error> {
        assert_eq!(method, "tick", "PH native lazy tick target");
        self.note("schedule", vec![obs(&interval.borrow())])?;
        self.scheduled = true;
        Ok(())
    }
}
impl ListenerHost for Host {
    type Options = V;
    type Symbol = V;
    type Callback = Rc<()>;
    type Socket = Rc<()>;
    type Handler = Rc<()>;
    type Result = Rc<()>;
    type Error = Error;
    fn public_socket(&mut self) -> Result<Rc<()>, Error> {
        self.note("socket", vec![])?;
        Ok(self.socket.clone())
    }
    fn handler(&mut self, socket: &Rc<()>, member: &'static str) -> Result<Rc<()>, Error> {
        assert!(Rc::ptr_eq(socket, &self.first_socket));
        self.note("handler", vec![text(member)])?;
        Ok(self.handler.clone())
    }
    fn spread_options(&mut self, options: &V) -> Result<V, Error> {
        assert!(Rc::ptr_eq(options, &self.options));
        self.note("spread", vec![])?;
        let mut fields = options.borrow().clone();
        fields["extra"] = json!(7);
        Ok(v(fields))
    }
    fn symbol(&mut self, options: &V) -> Result<V, Error> {
        Ok(v(options.borrow()["symbol"].clone()))
    }
    fn format_symbol(&mut self, symbol: V) -> Result<V, Error> {
        self.note("format", vec![obs(&symbol.borrow())])?;
        if self.flag("switchSocket") {
            self.socket = self.second_socket.clone();
        }
        Ok(symbol)
    }
    fn assign_symbol(&mut self, options: &mut V, symbol: V) -> Result<(), Error> {
        assert!(
            !Rc::ptr_eq(options, &self.options),
            "PH native copied options"
        );
        options.borrow_mut()["symbol"] = symbol.borrow().clone();
        Ok(())
    }
    fn call_ohlcv(
        &mut self,
        socket: Rc<()>,
        handler: Rc<()>,
        options: V,
        callback: Rc<()>,
    ) -> Result<Rc<()>, Error> {
        assert!(
            Rc::ptr_eq(&socket, &self.first_socket)
                && Rc::ptr_eq(&handler, &self.handler)
                && Rc::ptr_eq(&callback, &self.callback),
            "PH native receiver handler callback identity"
        );
        self.note(
            "listen",
            vec![text("first"), text("listenOHLCV"), obs(&options.borrow())],
        )?;
        Ok(self.listener.clone())
    }
    fn call_book(
        &mut self,
        socket: Rc<()>,
        handler: Rc<()>,
        symbol: V,
        callback: Rc<()>,
    ) -> Result<Rc<()>, Error> {
        assert!(
            Rc::ptr_eq(&socket, &self.first_socket)
                && Rc::ptr_eq(&handler, &self.handler)
                && Rc::ptr_eq(&callback, &self.callback),
            "PH native receiver handler callback identity"
        );
        self.note(
            "listen",
            vec![
                text("first"),
                text("listenOrderBook"),
                obs(&symbol.borrow()),
            ],
        )?;
        Ok(self.listener.clone())
    }
}
impl PositionQueryHost for Host {
    type Value = V;
    type Response = V;
    type Rows = Rows;
    type Record = V;
    type Position = V;
    type Mapper = Rc<()>;
    type Error = Error;
    fn ends_usdc(&mut self, symbol: &V) -> Result<bool, Error> {
        let s = symbol.borrow();
        if s.get("$undefined").is_some() || s.is_null() {
            return Ok(false);
        }
        Ok(units(&s).ends_with(&"USDC".encode_utf16().collect::<Vec<_>>()))
    }
    fn format_out(&mut self, symbol: &V) -> Result<V, Error> {
        self.note("format", vec![obs(&symbol.borrow())])?;
        Ok(symbol.clone())
    }
    async fn get(
        &mut self,
        endpoint: &'static str,
        currency: &'static str,
        symbol: V,
    ) -> Result<V, Error> {
        let config = json!({"params":{"currency":currency,"symbol":*symbol.borrow()}});
        self.note("get", vec![text(endpoint), obs(&config)])?;
        if self.flag("getError") {
            return Err(self.error.clone());
        }
        Ok(v(self.row["data"].clone()))
    }
    fn response_data(&mut self, response: V) -> Result<Option<Rows>, Error> {
        Ok(if safe_cex_num::js_truthy(Some(&response.borrow())) {
            Some(self.raw_rows.clone())
        } else {
            None
        })
    }
    fn length(&mut self, rows: &Rows) -> Result<usize, Error> {
        Ok(rows.borrow().len())
    }
    fn row(&mut self, rows: &Rows, index: usize) -> Result<Option<V>, Error> {
        Ok(rows.borrow().get(index).cloned().flatten())
    }
    fn bind_mapper(&mut self) -> Result<Rc<()>, Error> {
        self.note("bind", vec![])?;
        Ok(self.mapper.clone())
    }
    fn map_position(&mut self, mapper: &Rc<()>, record: V) -> Result<Option<V>, Error> {
        assert!(
            Rc::ptr_eq(mapper, &self.original_mapper),
            "PH native retained bound mapper"
        );
        if self.flag("switchMapper") {
            self.mapper = Rc::new(());
        }
        self.note("map", vec![obs(&record.borrow())])?;
        Ok(if record.borrow()["skip"] == true {
            None
        } else {
            Some(record.clone())
        })
    }
    fn formatted_row_symbol(&mut self, row: Option<&V>) -> Result<V, Error> {
        let Some(row) = row else {
            return Err(Error {
                message: "Cannot read properties of undefined (reading 'symbol')",
                venue: false,
                token: Rc::new(()),
            });
        };
        if row.borrow().is_null() {
            return Err(Error {
                message: "Cannot read properties of null (reading 'symbol')",
                venue: false,
                token: Rc::new(()),
            });
        }
        let symbol = row.borrow().get("symbol").cloned().unwrap_or(absent());
        self.note("formatIn", vec![obs(&symbol)])?;
        let upper = if symbol.get("$units").is_some() {
            json!({"$units":units(&symbol).iter().map(|u|if (97..=122).contains(u){u-32}else{*u}).collect::<Vec<_>>()})
        } else {
            json!(symbol.as_str().unwrap_or("").to_uppercase())
        };
        Ok(v(upper))
    }
    fn same_symbol(&mut self, left: &V, right: &V) -> Result<bool, Error> {
        Ok(same_value(&left.borrow(), &right.borrow()))
    }
    fn mode_is(&mut self, row: &V, mode: &'static str) -> Result<bool, Error> {
        Ok(row.borrow()["posMode"] == mode)
    }
    fn report(&mut self, error: &Error) -> Result<(), Error> {
        let reason = self.reason(error);
        self.note("report", vec![text(error.message), reason])
    }
}
impl OrderSideHost for Host {
    type Options = V;
    type Value = V;
    type Positions = V;
    type Error = Error;
    fn info_side(&mut self, options: &V, optional: bool) -> Result<V, Error> {
        assert!(Rc::ptr_eq(options, &self.options));
        self.info_reads += 1;
        assert_eq!(
            optional,
            self.info_reads == 1,
            "PH native optional then required info path"
        );
        if self.flag("infoDisappears") {
            return if optional {
                Ok(v(json!("Pinned")))
            } else {
                Err(Error {
                    message: "Cannot read properties of null (reading 'posSide')",
                    venue: false,
                    token: Rc::new(()),
                })
            };
        }
        if self.flag("infoChanges") {
            return Ok(v(json!(if self.info_reads == 1 {
                "Pinned"
            } else {
                "Changed"
            })));
        }
        Ok(self.pinned.clone().unwrap_or_else(|| v(absent())))
    }
    fn truthy(&mut self, value: &V) -> Result<bool, Error> {
        Ok(value.borrow().get("$undefined").is_none()
            && safe_cex_num::js_truthy(Some(&value.borrow())))
    }
    fn symbol(&mut self, options: &V) -> Result<V, Error> {
        Ok(v(options.borrow()["symbol"].clone()))
    }
    async fn positions(&mut self, symbol: V) -> Result<V, Error> {
        self.note("positions", vec![obs(&symbol.borrow())])?;
        if self.flag("mutateOptions") || self.flag("mutateSymbol") {
            suspend().await;
            if self.flag("mutateOptions") {
                self.options.borrow_mut()["side"] = json!("sell");
            }
            if self.flag("mutateSymbol") {
                self.options.borrow_mut()["symbol"] = json!("AFTER");
            }
        }
        Ok(self.mode_positions.clone())
    }
    fn length(&mut self, positions: &V) -> Result<usize, Error> {
        Ok(positions.borrow().as_array().unwrap().len())
    }
    fn position_oneway(&mut self, positions: &V, index: usize) -> Result<Option<bool>, Error> {
        Ok(positions
            .borrow()
            .get(index)
            .map(|v| v["positionMode"] == "oneway"))
    }
    async fn raw_mode(&mut self, symbol: V) -> Result<Option<PositionMode>, Error> {
        self.note("mode", vec![obs(&symbol.borrow())])?;
        Ok(match self.row["fixture"]["mode"].as_str() {
            Some("oneway") => Some(PositionMode::OneWay),
            Some("hedged") => Some(PositionMode::Hedged),
            _ => None,
        })
    }
    fn is_buy(&mut self, options: &V) -> Result<bool, Error> {
        Ok(options.borrow()["side"] == "buy")
    }
    fn is_type(&mut self, options: &V, kind: &'static str) -> Result<bool, Error> {
        Ok(options.borrow()["type"] == kind)
    }
    fn reduce_only(&mut self, options: &V) -> Result<bool, Error> {
        Ok(safe_cex_num::js_truthy(options.borrow().get("reduceOnly")))
    }
    fn value(&mut self, side: &'static str) -> Result<V, Error> {
        Ok(v(json!(side)))
    }
}
fn run(name: &str, count: usize) {
    let rows = oracle(name);
    assert_eq!(rows.len(), count, "PH source case census");
    let mut suspensions = 0;
    for row in rows {
        let mut host = Host::new(&row);
        let options = host.options.clone();
        let callback = host.callback.clone();
        let symbol = host.symbol_value();
        let output: Result<Value, Error> = match name {
            "dispose" => dispose(&mut host).map(|_| obs(&absent())),
            "_startPrivate" => {
                let (r, pending) = drive(start_private(&mut host));
                suspensions += pending;
                r.map(|_| obs(&absent()))
            }
            "_startPublicDataOnly" => drive(start_public_only(&mut host))
                .0
                .map(|_| obs(&absent())),
            "tick" => {
                let (r, pending) = drive(tick(&mut host));
                suspensions += pending;
                r.map(|_| obs(&absent()))
            }
            "listenOHLCV" => listen_ohlcv(&mut host, options, callback).map(|r| {
                assert!(
                    Rc::ptr_eq(&r, &host.listener),
                    "PH native listener identity"
                );
                obs(&json!({"tag":"listener"}))
            }),
            "listenOrderBook" => listen_order_book(&mut host, symbol, callback).map(|r| {
                assert!(
                    Rc::ptr_eq(&r, &host.listener),
                    "PH native listener identity"
                );
                obs(&json!({"tag":"listener"}))
            }),
            "fetchPositionForSymbol" => {
                drive(fetch_position_for_symbol(&mut host, symbol))
                    .0
                    .map(|positions| {
                        for p in &positions {
                            assert!(
                                host.raw_rows
                                    .borrow()
                                    .iter()
                                    .flatten()
                                    .any(|row| Rc::ptr_eq(row, p)),
                                "PH native mapped position identity"
                            );
                        }
                        json!(
                            positions
                                .iter()
                                .map(|p| obs(&p.borrow()))
                                .collect::<Vec<_>>()
                        )
                    })
            }
            "fetchRawPositionModeForSymbol" => drive(fetch_raw_position_mode(&mut host, symbol))
                .0
                .map(|mode| match mode {
                    Some(PositionMode::OneWay) => text("oneway"),
                    Some(PositionMode::Hedged) => text("hedged"),
                    None => obs(&absent()),
                }),
            "getOrderPositionSide" => {
                let (r, pending) = drive(get_order_position_side(&mut host, options));
                suspensions += pending;
                r.map(|value| {
                    if !host.flag("infoChanges") {
                        if let Some(pinned) = &host.pinned {
                            if safe_cex_num::js_truthy(Some(&pinned.borrow())) {
                                assert!(
                                    Rc::ptr_eq(pinned, &value),
                                    "PH native opaque pinned side identity"
                                );
                            }
                        }
                    }
                    obs(&value.borrow())
                })
            }
            _ => panic!("Unknown source member"),
        };
        host.check_result(output);
        if host.scheduled {
            host.note("callbackTick", vec![]).unwrap();
        }
        host.end();
    }
    if matches!(name, "_startPrivate" | "tick" | "getOrderPositionSide") {
        assert!(suspensions > 0, "PH native actual Pending continuation");
    }
}
#[test]
fn actual_typescript_dispose() {
    run("dispose", 4)
}
#[test]
fn actual_typescript_private_start() {
    run("_startPrivate", 7)
}
#[test]
fn actual_typescript_public_start() {
    run("_startPublicDataOnly", 4)
}
#[test]
fn actual_typescript_tick() {
    run("tick", 12)
}
#[test]
fn actual_typescript_ohlcv_delegate() {
    run("listenOHLCV", 8)
}
#[test]
fn actual_typescript_book_delegate() {
    run("listenOrderBook", 7)
}
#[test]
fn actual_typescript_position_read() {
    run("fetchPositionForSymbol", 13)
}
#[test]
fn actual_typescript_mode_read() {
    run("fetchRawPositionModeForSymbol", 10)
}
#[test]
fn actual_typescript_order_side() {
    run("getOrderPositionSide", 20)
}
