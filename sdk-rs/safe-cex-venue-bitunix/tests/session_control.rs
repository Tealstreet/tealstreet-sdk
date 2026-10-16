use safe_cex_venue_bitunix::{session_control::*, types::BASE_WS_URL};
use serde_json::{Value, json};
use std::{
    cell::Cell,
    collections::HashMap,
    future::{Future, poll_fn},
    pin::{Pin, pin},
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
type Error = Rc<Value>;
fn bits(n: f64) -> Value {
    json!({"bits":format!("{:016x}",if n.is_nan(){0x7ff8_0000_0000_0000}else{n.to_bits()})})
}
fn text(s: &str) -> Value {
    units(&s.encode_utf16().collect::<Vec<_>>())
}
fn units(s: &[u16]) -> Value {
    json!({"units":s})
}
fn obs(v: &Value) -> Value {
    match v {
        Value::Number(n) => bits(n.as_f64().unwrap()),
        Value::String(s) => text(s),
        Value::Array(a) => json!(a.iter().map(obs).collect::<Vec<_>>()),
        Value::Object(o) => {
            json!({"entries":o.iter().map(|(k,v)|json!([k,obs(v)])).collect::<Vec<_>>()})
        }
        _ => v.clone(),
    }
}
fn error(name: &str) -> Error {
    Rc::new(json!({"error":["Error",format!("{name} failed")]}))
}
fn ready<T>(f: impl Future<Output = T>) -> T {
    let mut f = pin!(f);
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..3 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
    }
    panic!("SESSION pending beyond witness floor");
}
type Login = Pin<Box<dyn Future<Output = Result<(), Error>>>>;
struct Host {
    row: Value,
    events: Vec<Value>,
    disposed: bool,
    loaded: Rc<Value>,
    last_mode: String,
    values: HashMap<&'static str, Rc<Value>>,
    state: HashMap<&'static str, Value>,
    flag_reads: HashMap<&'static str, usize>,
    socket: bool,
    closed: bool,
    timer: f64,
    pings: u32,
    login_ready: Rc<Cell<bool>>,
    login: Option<Result<Login, Error>>,
    after: Option<fn(&mut Self) -> Result<(), Error>>,
    ping_after: Option<fn(&mut Self) -> Result<(), Error>>,
}
impl Host {
    fn new(row: &Value) -> Self {
        let f = &row["fixture"];
        Self {
            row: row.clone(),
            events: vec![],
            disposed: f["disposed"] == true,
            loaded: Rc::new(json!({"seed":"kept","orders":false})),
            last_mode: f["lastMode"].as_str().unwrap_or("HEDGE").into(),
            values: HashMap::from([
                ("fetchMarkets", Rc::new(json!([{"tag":"m0"},{"tag":"m1"}]))),
                ("fetchTickers", Rc::new(json!([{"tag":"t0"}]))),
                ("fetchOrders", Rc::new(json!([{"tag":"o0"}]))),
                ("fetchPositions", Rc::new(json!([{"tag":"p0"}]))),
                ("fetchPositionMode", Rc::new(json!(true))),
            ]),
            state: HashMap::from([
                ("isDisposed", json!(f["disposed"] == true)),
                (
                    "connectionAttempts",
                    json!(f["attempts"].as_f64().unwrap_or(0.)),
                ),
                ("MAX_ATTEMPTS", json!(2.)),
                ("lastMessageCount", json!(f["last"].as_f64().unwrap_or(0.))),
                ("messageCount", json!(f["count"].as_f64().unwrap_or(0.))),
                ("useFallbackUrl", json!(f["fallback"] == true)),
                ("testnet", json!(f["testnet"] == true)),
            ]),
            flag_reads: HashMap::new(),
            socket: false,
            closed: f["closed"] == true,
            timer: f["timer"].as_f64().unwrap_or(0.),
            pings: 0,
            login_ready: Rc::new(Cell::new(false)),
            login: None,
            after: None,
            ping_after: None,
        }
    }
    fn note(&mut self, name: &str, args: Vec<Value>) -> Result<(), Error> {
        let mut event = vec![json!(name)];
        event.extend(args);
        let event = json!(event);
        let index = self.events.len();
        assert_eq!(
            event, self.row["events"][index],
            "SESSION native effect {}:{name}:{index}",
            self.row["key"]
        );
        self.events.push(event);
        if self.row["fixture"]["fail"] == name {
            Err(error(name))
        } else {
            Ok(())
        }
    }
    fn dispose_after(&mut self, member: &str) {
        if self.row["fixture"]["disposeAfter"] == member {
            self.disposed = true;
        }
    }
    fn resume(&mut self) -> Result<(), Error> {
        if let Some(login) = self.login.take() {
            match login {
                Err(e) => self.note("unhandled", vec![(*e).clone()])?,
                Ok(mut f) => {
                    self.note("resume", vec![])?;
                    if self.row["fixture"]["disposeResume"] == true {
                        self.disposed = true;
                    }
                    self.login_ready.set(true);
                    let mut cx = Context::from_waker(Waker::noop());
                    match f.as_mut().poll(&mut cx) {
                        Poll::Pending => panic!("SESSION resumed login remained pending"),
                        Poll::Ready(Err(e)) => self.note("unhandled", vec![(*e).clone()])?,
                        Poll::Ready(Ok(())) => {
                            if let Err(e) = self.after.unwrap()(self) {
                                self.note("unhandled", vec![(*e).clone()])?;
                            }
                        }
                    }
                }
            }
        }
        if self.row["fixture"]["invokeTimer"] == true && self.ping_after.is_some() {
            if self.row["fixture"]["disposeTimer"] == true {
                self.disposed = true;
            }
            self.ping_after.unwrap()(self)?;
        }
        Ok(())
    }
}
impl ExchangeSessionHost for Host {
    type Value = Rc<Value>;
    type Socket = String;
    type Error = Error;
    async fn external_public(&mut self) -> Result<bool, Error> {
        self.note("external", vec![])?;
        Ok(self.row["fixture"]["external"] == true)
    }
    async fn fetch(&mut self, member: &'static str) -> Result<Rc<Value>, Error> {
        self.note(member, vec![])?;
        if self.row["fixture"]["yield"] == member {
            let mut yielded = false;
            poll_fn(|cx| {
                if yielded {
                    Poll::Ready(())
                } else {
                    yielded = true;
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            })
            .await;
        }
        self.dispose_after(member);
        Ok(self.values[member].clone())
    }
    async fn fetch_balance(&mut self) -> Result<(Rc<Value>, Rc<Value>), Error> {
        self.note("fetchBalance", vec![])?;
        if self.row["fixture"]["fetchFailure"] == true {
            return Err(Rc::new(json!({"error":["Error","balance failed"]})));
        }
        self.dispose_after("fetchBalance");
        let positions = Rc::new(json!([{"tag":"balance-position"}]));
        self.values.insert("fetchPositions", positions.clone());
        Ok((Rc::new(json!({"tag":"balance"})), positions))
    }
    fn disposed(&mut self) -> Result<bool, Error> {
        self.note("isDisposed", vec![json!(self.disposed)])?;
        Ok(self.disposed)
    }
    fn public_only(&mut self) -> Result<bool, Error> {
        let v = self.row["fixture"]["publicOnly"] == true;
        self.note("isPublicDataOnly", vec![json!(v)])?;
        Ok(v)
    }
    fn loaded(&mut self) -> Result<Rc<Value>, Error> {
        self.note("loaded", vec![])?;
        Ok(self.loaded.clone())
    }
    fn update(
        &mut self,
        fields: Vec<(&'static str, &Rc<Value>)>,
        loaded: Option<(Rc<Value>, Vec<&'static str>)>,
    ) -> Result<(), Error> {
        let mut patch = serde_json::Map::new();
        for (k, v) in fields {
            if k != "balance" {
                let member = match k {
                    "markets" => "fetchMarkets",
                    "tickers" => "fetchTickers",
                    "orders" => "fetchOrders",
                    _ => "fetchPositions",
                };
                assert!(
                    Rc::ptr_eq(v, &self.values[member]),
                    "SESSION native record identity {k}"
                );
            }
            patch.insert(k.into(), (**v).clone());
        }
        if let Some((original, flags)) = loaded {
            assert!(
                Rc::ptr_eq(&original, &self.loaded),
                "SESSION native loaded identity"
            );
            let mut value = (*original).clone();
            for flag in flags {
                value[flag] = json!(true);
            }
            patch.insert("loaded".into(), value);
        }
        self.note("update", vec![obs(&Value::Object(patch.clone()))])?;
        if let Some(loaded) = patch.get("loaded") {
            self.loaded = Rc::new(loaded.clone());
        }
        Ok(())
    }
    fn connect(&mut self, socket: &'static str) -> Result<(), Error> {
        self.note("connect", vec![text(socket)])
    }
    fn configure_shards(&mut self, markets: &Rc<Value>) -> Result<Vec<String>, Error> {
        assert!(
            Rc::ptr_eq(markets, &self.values["fetchMarkets"]),
            "SESSION native original markets"
        );
        self.note("shards", vec![obs(markets)])?;
        Ok(vec!["a".into(), "b".into()])
    }
    fn connect_shard(&mut self, socket: String) -> Result<(), Error> {
        self.note("shard", vec![text(&socket)])
    }
    fn length(&mut self, rows: &Rc<Value>) -> Result<usize, Error> {
        Ok(rows.as_array().unwrap().len())
    }
    fn log(&mut self, s: Vec<u16>) -> Result<(), Error> {
        self.note("log", vec![units(&s)])
    }
    fn set_hedged(&mut self, mode: Rc<Value>) -> Result<(), Error> {
        self.note("setting", vec![text("isHedged"), obs(&mode)])
    }
    async fn initial_tick(&mut self) -> Result<(), Error> {
        self.note("initialTick", vec![])?;
        self.dispose_after("initialTick");
        Ok(())
    }
    fn position_modes_equal(&mut self) -> Result<bool, Error> {
        let mode = self.row["fixture"]["mode"]
            .as_str()
            .unwrap_or("HEDGE")
            .to_owned();
        self.note("lastMode", vec![text(&self.last_mode)])?;
        self.note("mode", vec![text(&mode)])?;
        Ok(self.last_mode == mode)
    }
    fn copy_position_mode(&mut self) -> Result<(), Error> {
        let mode = self.row["fixture"]["mode"]
            .as_str()
            .unwrap_or("HEDGE")
            .to_owned();
        self.note("mode", vec![text(&mode)])?;
        self.note("setLastMode", vec![text(&mode)])?;
        self.last_mode = mode;
        Ok(())
    }
    fn report_message(&mut self, e: Error) -> Result<(), Error> {
        self.note("report", vec![text(e["error"][1].as_str().unwrap())])
    }
    fn tick_interval(&mut self) -> Result<f64, Error> {
        self.note("interval", vec![])?;
        Ok(1234.)
    }
    fn schedule_tick(&mut self, next: f64, interval: f64) -> Result<(), Error> {
        assert_eq!(
            next,
            self.row["fixture"]["count"].as_f64().unwrap_or(0.) + 1.,
            "SESSION native next count"
        );
        self.note("loop", vec![bits(interval)])
    }
}
impl ConnectionHost for Host {
    type Url = Vec<u16>;
    type Socket = ();
    type Timer = f64;
    type Error = Error;
    fn flag(&mut self, field: &'static str) -> Result<bool, Error> {
        let index = self.flag_reads.entry(field).or_default();
        let v = if field == "useFallbackUrl" {
            self.row["fixture"]["useReads"]
                .get(*index)
                .and_then(Value::as_bool)
                .unwrap_or_else(|| self.state[field].as_bool().unwrap())
        } else {
            self.state[field].as_bool().unwrap()
        };
        *index += 1;
        self.note("get", vec![text(field), json!(v)])?;
        Ok(v)
    }
    fn number(&mut self, field: &'static str) -> Result<f64, Error> {
        let v = self.state[field].as_f64().unwrap();
        self.note("get", vec![text(field), bits(v)])?;
        Ok(v)
    }
    fn set_flag(&mut self, field: &'static str, v: bool) -> Result<(), Error> {
        self.note("set", vec![text(field), json!(v)])?;
        self.state.insert(field, json!(v));
        Ok(())
    }
    fn set_number(&mut self, field: &'static str, v: f64) -> Result<(), Error> {
        self.note("set", vec![text(field), bits(v)])?;
        self.state.insert(field, json!(v));
        Ok(())
    }
    fn clear_queue(&mut self) -> Result<(), Error> {
        self.note("queue", vec![])
    }
    fn fallback(&mut self, scope: &'static str) -> Result<Vec<u16>, Error> {
        self.note("routing", vec![text(scope)])?;
        if let Some(units) = self.row["fixture"]["urlUnits"].as_array() {
            Ok(units.iter().map(|v| v.as_u64().unwrap() as u16).collect())
        } else {
            Ok(self.row["fixture"]["url"]
                .as_str()
                .unwrap_or("")
                .encode_utf16()
                .collect())
        }
    }
    fn url_truthy(&self, url: &Vec<u16>) -> bool {
        !url.is_empty()
    }
    fn base_url(&mut self, scope: &'static str, testnet: bool) -> Result<Vec<u16>, Error> {
        let key = if testnet { "testnet" } else { "livenet" };
        self.note("base", vec![text(scope), text(key)])?;
        let table = BASE_WS_URL.iter().find(|(k, _)| *k == scope).unwrap().1;
        Ok(table
            .iter()
            .find(|(k, _)| *k == key)
            .unwrap()
            .1
            .encode_utf16()
            .collect())
    }
    fn proxy_info(&mut self, message: Vec<u16>) -> Result<(), Error> {
        self.note("proxy", vec![text("grafana-info"), units(&message)])
    }
    fn create(&mut self, url: &Vec<u16>) -> Result<(), Error> {
        self.note("create", vec![units(url)])
    }
    fn assign_socket(&mut self, _: ()) -> Result<(), Error> {
        self.note("setSocket", vec![])?;
        self.socket = true;
        Ok(())
    }
    fn socket(&mut self) -> Result<Option<()>, Error> {
        self.note("socket", vec![json!(self.socket)])?;
        Ok(self.socket.then_some(()))
    }
    fn add_listener(
        &mut self,
        _: Option<()>,
        event: &'static str,
        listener: SocketListener,
        kind: SocketKind,
    ) -> Result<(), Error> {
        assert_eq!(
            std::mem::discriminant(&listener),
            std::mem::discriminant(&match event {
                "open" => SocketListener::Open,
                "message" => SocketListener::Message,
                "close" => SocketListener::Close,
                _ => SocketListener::Error,
            }),
            "SESSION native listener identity"
        );
        assert_eq!(
            kind.routing_key(),
            match self.row["key"].as_str().unwrap().split('.').next().unwrap() {
                "public" => "wsPublic",
                "futures" => "wsBusiness",
                _ => "wsPrivate",
            }
        );
        self.note("listener", vec![text(event)])
    }
    fn closed(&mut self, _: Option<()>) -> Result<bool, Error> {
        self.note("readyState", vec![])?;
        Ok(self.closed)
    }
    fn close_socket(&mut self, _: Option<()>) -> Result<(), Error> {
        self.note("closeSocket", vec![])?;
        if self.row["fixture"]["closeThrows"] == true {
            return Err(error("close"));
        }
        self.closed = true;
        Ok(())
    }
    fn ping_timer(&mut self) -> Result<f64, Error> {
        self.note("timer", vec![bits(77.)])?;
        Ok(77.)
    }
    fn clear_timer(&mut self, t: f64) -> Result<(), Error> {
        self.note("clear", vec![bits(t)])
    }
    fn on_close(&mut self) -> Result<(), Error> {
        self.note("onClose", vec![])
    }
}
impl PrivateOpenHost for Host {
    type Timer = f64;
    type Login = Login;
    type Error = Error;
    fn disposed(&mut self) -> Result<bool, Error> {
        self.note("disposed", vec![json!(self.disposed)])?;
        Ok(self.disposed)
    }
    fn ping(&mut self) -> Result<(), Error> {
        self.note("ping", vec![])?;
        self.pings += 1;
        if self.row["fixture"]["disposePing"].as_u64() == Some(u64::from(self.pings)) {
            self.disposed = true;
        }
        Ok(())
    }
    fn timer(&mut self) -> Result<f64, Error> {
        self.note("timer", vec![bits(self.timer)])?;
        Ok(self.timer)
    }
    fn timer_truthy(&self, t: &f64) -> bool {
        *t != 0. && !t.is_nan()
    }
    fn clear(&mut self, t: f64) -> Result<(), Error> {
        self.note("clear", vec![bits(t)])
    }
    fn begin_login(&mut self) -> Result<Login, Error> {
        self.note("login", vec![])?;
        let ready = self.login_ready.clone();
        let rejected = self.row["fixture"]["rejectLogin"] == true;
        Ok(Box::pin(poll_fn(move |_| {
            if ready.get() {
                Poll::Ready(if rejected {
                    Err(Rc::new(json!({"error":["Error","login rejected"]})))
                } else {
                    Ok(())
                })
            } else {
                Poll::Pending
            }
        })))
    }
    fn spawn_login(
        &mut self,
        mut login: Result<Login, Error>,
        after: fn(&mut Self) -> Result<(), Error>,
    ) -> Result<(), Error> {
        if let Ok(f) = &mut login {
            assert!(
                f.as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending(),
                "SESSION native login suspension"
            );
        }
        self.login = Some(login);
        self.after = Some(after);
        Ok(())
    }
    fn schedule_ping(
        &mut self,
        delay: f64,
        after: fn(&mut Self) -> Result<(), Error>,
    ) -> Result<f64, Error> {
        self.note("schedule", vec![bits(delay)])?;
        self.ping_after = Some(after);
        Ok(99.)
    }
    fn assign_timer(&mut self, t: f64) -> Result<(), Error> {
        self.note("setTimer", vec![bits(t)])?;
        self.timer = t;
        Ok(())
    }
}
fn run_family(family: &str, floor: usize) {
    let out = Command::new("node")
        .arg(format!(
            "{}/tests/session-control-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "SESSION actual source {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let corpus: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        corpus["rows"].as_array().unwrap().len(),
        106,
        "SESSION source floor"
    );
    let mut count = 0;
    for row in corpus["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["key"].as_str().unwrap() == family)
    {
        count += 1;
        let mut h = Host::new(row);
        let f = row["fixture"].clone();
        let result = match family {
            "exchange.start" => ready(start(&mut h)),
            "exchange.tick" => ready(tick(&mut h, f["count"].as_f64().unwrap_or(0.))),
            "private.onOpen" => private_on_open(&mut h).and_then(|()| h.resume()),
            "exchange.listenOHLCV" => {
                let events = std::cell::RefCell::new(&mut h);
                let options = Rc::new(());
                let callback = Rc::new(());
                let result_token = Rc::new(());
                listen_ohlcv(
                    options.clone(),
                    callback.clone(),
                    |o| {
                        assert!(Rc::ptr_eq(o, &options), "SESSION native opts identity");
                        events.borrow_mut().note("symbol", vec![])?;
                        Ok("BTCUSDT".encode_utf16().collect())
                    },
                    |s| {
                        events.borrow_mut().note("configured", vec![units(&s)])?;
                        Ok(f["configured"] == true)
                    },
                    |s| events.borrow_mut().note("metadata", vec![units(&s)]),
                    |o, c| {
                        assert!(
                            Rc::ptr_eq(&o, &options) && Rc::ptr_eq(&c, &callback),
                            "SESSION native delegate identity"
                        );
                        events.borrow_mut().note("listen", vec![])?;
                        Ok(result_token.clone())
                    },
                    |_| {
                        events.borrow_mut().note("symbol", vec![])?;
                        if f["fail"] == "secondSymbol" {
                            Err(error("secondSymbol"))
                        } else {
                            Ok("BTCUSDT".encode_utf16().collect())
                        }
                    },
                )
                .map(|r| {
                    assert!(
                        Rc::ptr_eq(&r, &result_token),
                        "SESSION native unlisten identity"
                    );
                })
            }
            _ => {
                let kind = match family.split('.').next().unwrap() {
                    "public" => SocketKind::Public,
                    "futures" => SocketKind::Futures,
                    _ => SocketKind::Private,
                };
                let result = match kind {
                    SocketKind::Public => connect_public(&mut h),
                    SocketKind::Futures => connect_futures(&mut h),
                    SocketKind::Private => ready(connect_private(&mut h)),
                };
                result.and_then(|()| {
                    if f["noSocket"] == true {
                        h.socket = false;
                    }
                    match f["event"].as_str() {
                        Some("close") => connection_close(&mut h, kind),
                        Some("error") => connection_error(&mut h, kind),
                        _ => Ok(()),
                    }
                })
            }
        };
        let result = match result {
            Ok(()) => json!({"ok":true}),
            Err(e) => json!({"error":*e}),
        };
        assert_eq!(result, row["result"], "SESSION native result {family}");
        assert_eq!(
            h.events.len(),
            row["events"].as_array().unwrap().len(),
            "SESSION native effect count {family}"
        );
        if family == "exchange.start" || family == "exchange.tick" {
            assert_eq!(
                json!({"disposed":h.disposed,"loaded":obs(&h.loaded),"lastMode":text(&h.last_mode)}),
                row["state"],
                "SESSION native state {family}"
            );
        } else if family == "private.onOpen" {
            assert_eq!(
                json!({"disposed":h.disposed,"timer":bits(h.timer),"pings":bits(f64::from(h.pings))}),
                row["state"],
                "SESSION native private state"
            );
        } else if family != "exchange.listenOHLCV" {
            for (k, v) in &h.state {
                let found = row["state"]["entries"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|e| e[0] == *k)
                    .unwrap();
                assert_eq!(obs(v), found[1], "SESSION native connection state {k}");
            }
        }
    }
    assert_eq!(count, floor, "SESSION independent family floor {family}");
}
#[test]
fn actual_typescript_start() {
    run_family("exchange.start", 12);
}
#[test]
fn actual_typescript_tick() {
    run_family("exchange.tick", 16);
}
#[test]
fn actual_typescript_public_connection() {
    run_family("public.connectAndSubscribe", 20);
}
#[test]
fn actual_typescript_futures_connection() {
    run_family("futures.connectAndSubscribe", 20);
}
#[test]
fn actual_typescript_private_connection() {
    run_family("private.connectAndSubscribe", 20);
}
#[test]
fn actual_typescript_private_open() {
    run_family("private.onOpen", 12);
}
#[test]
fn actual_typescript_candle_delegate() {
    run_family("exchange.listenOHLCV", 6);
}
