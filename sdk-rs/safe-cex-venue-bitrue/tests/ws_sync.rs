use safe_cex_types::{OrderSide, OrderStatus, OrderType};
use safe_cex_venue_bitrue::{types, ws_sync as bodies};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
};
#[derive(Debug)]
enum Js {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(Vec<u16>),
    Symbol,
    Object(String, Option<String>),
    Market(usize),
    Type(OrderType),
    Side(OrderSide),
    Status(OrderStatus),
}
type Native = Rc<Js>;
type Error = Native;
type Callback = Box<dyn FnMut() -> Result<Native, Error>>;
fn units(v: &Value) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| u16::try_from(v.as_u64().unwrap()).unwrap())
        .collect()
}
fn text(s: &str) -> Native {
    Rc::new(Js::String(s.encode_utf16().collect()))
}
fn input(v: &Value) -> Native {
    Rc::new(if v.is_null() {
        Js::Null
    } else if let Some(v) = v.as_bool() {
        Js::Bool(v)
    } else if let Some(n) = v.as_f64() {
        Js::Number(n)
    } else {
        match v["kind"].as_str().unwrap() {
            "undefined" => Js::Undefined,
            "string" => Js::String(units(&v["units"])),
            "symbol" => Js::Symbol,
            "number" => Js::Number(match v["value"].as_str() {
                Some("NaN") => f64::NAN,
                Some("neg0") => -0.0,
                Some(v) => v.parse().unwrap(),
                None => v["value"].as_f64().unwrap(),
            }),
            "object" => Js::Object(
                v["tag"].as_str().unwrap_or("object").into(),
                Some(v["text"].as_str().unwrap().into()),
            ),
            _ => panic!("bad fixture input"),
        }
    })
}
fn upper(value: Vec<u16>) -> Vec<u16> {
    let mut out = Vec::new();
    for ch in char::decode_utf16(value) {
        match ch {
            Ok(ch) => {
                for c in ch.to_uppercase() {
                    let mut b = [0; 2];
                    out.extend_from_slice(c.encode_utf16(&mut b));
                }
            }
            Err(ch) => out.push(ch.unpaired_surrogate()),
        }
    }
    out
}
fn native(actual: &Native, source: &Value, label: &str) {
    match &**actual {
        Js::Undefined => assert_eq!(
            source["kind"], "undefined",
            "BITRUE-SYNC-NATIVE-UNDEFINED {label}"
        ),
        Js::Null => assert_eq!(source["kind"], "null", "BITRUE-SYNC-NATIVE-NULL {label}"),
        Js::Bool(v) => assert_eq!(
            *v,
            source["value"].as_bool().unwrap(),
            "BITRUE-SYNC-NATIVE-BOOL {label}"
        ),
        Js::Number(v) => assert_eq!(
            v.to_bits(),
            u64::from_str_radix(source["bits"].as_str().unwrap(), 16).unwrap(),
            "BITRUE-SYNC-NATIVE-NUMBER {label}"
        ),
        Js::String(v) => assert_eq!(
            *v,
            units(&source["units"]),
            "BITRUE-SYNC-NATIVE-STRING {label}"
        ),
        Js::Object(tag, _) => assert_eq!(
            *tag,
            source["tag"].as_str().unwrap(),
            "BITRUE-SYNC-NATIVE-OBJECT {label}"
        ),
        Js::Symbol => assert_eq!(source["kind"], "symbol"),
        Js::Type(v) => {
            let expected = match String::from_utf16(&units(&source["units"]))
                .unwrap()
                .as_str()
            {
                "limit" => OrderType::Limit,
                "market" => OrderType::Market,
                "stop_market" => OrderType::StopLoss,
                "take_profit_market" => OrderType::TakeProfit,
                _ => panic!("unexpected source type"),
            };
            assert_eq!(*v, expected, "BITRUE-SYNC-NATIVE-TYPE {label}");
        }
        Js::Side(v) => {
            let expected = match String::from_utf16(&units(&source["units"]))
                .unwrap()
                .as_str()
            {
                "buy" => OrderSide::Buy,
                "sell" => OrderSide::Sell,
                _ => panic!("unexpected source side"),
            };
            assert_eq!(*v, expected, "BITRUE-SYNC-NATIVE-SIDE {label}");
        }
        Js::Status(v) => {
            let expected = match String::from_utf16(&units(&source["units"]))
                .unwrap()
                .as_str()
            {
                "open" => OrderStatus::Open,
                "closed" => OrderStatus::Closed,
                "canceled" => OrderStatus::Canceled,
                "canceling" => OrderStatus::Canceling,
                _ => panic!("unexpected source status"),
            };
            assert_eq!(*v, expected, "BITRUE-SYNC-NATIVE-STATUS {label}");
        }
        Js::Market(_) => panic!("market is not returned by source"),
    }
}
struct Host {
    spec: Value,
    events: RefCell<Vec<String>>,
    frames: RefCell<Vec<bodies::SyncFrame<Native>>>,
    sent: RefCell<Vec<Native>>,
    callbacks: RefCell<Vec<Native>>,
    callback: RefCell<Option<Callback>>,
    ping_reads: Cell<usize>,
    timer_reads: Cell<usize>,
    ping_at: Cell<f64>,
    timer: RefCell<Native>,
    initial_timer: Native,
    new_timer: Native,
    ret: Native,
    error: Native,
    type_error: Native,
    sender: Rc<()>,
    logged_sender: Rc<()>,
    stringifier: Rc<()>,
    clearer: Rc<()>,
    channel_object: Native,
    key: Native,
    pings: Vec<Native>,
    markets: Vec<Native>,
    inherited: Vec<Native>,
    raw: Native,
    found_symbol: Native,
    stringify_ready: Cell<bool>,
    wire_returns: Vec<Native>,
}
impl Host {
    fn event(&self, s: &str) -> Result<(), Error> {
        self.events.borrow_mut().push(s.into());
        if self.spec["fail"] == s {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
    fn fire(&self) -> Result<(), Error> {
        let mut callback = self.callback.borrow_mut().take();
        if let Some(cb) = callback.as_mut() {
            for _ in 0..2 {
                let value = cb()?;
                assert!(
                    Rc::ptr_eq(&value, &self.ret),
                    "BITRUE-SYNC-NATIVE-CALLBACK-RETURN-IDENTITY"
                );
                self.callbacks.borrow_mut().push(value);
            }
        }
        self.callback.replace(callback);
        Ok(())
    }
}
impl bodies::ParseHost for Host {
    type Value = Native;
    type Error = Error;
    fn string(&self, v: &Native) -> Option<Vec<u16>> {
        if let Js::String(v) = &**v {
            Some(v.clone())
        } else {
            None
        }
    }
    fn number(&self, v: &Native) -> Option<f64> {
        if let Js::Number(v) = &**v {
            Some(*v)
        } else {
            None
        }
    }
    fn truthy(&self, v: &Native) -> bool {
        match &**v {
            Js::Undefined | Js::Null => false,
            Js::Bool(v) => *v,
            Js::Number(v) => *v != 0.0 && !v.is_nan(),
            Js::String(v) => !v.is_empty(),
            _ => true,
        }
    }
    fn empty_string(&self) -> Native {
        text("")
    }
    fn template(&self, v: Native) -> Result<Vec<u16>, Error> {
        Ok(match &*v {
            Js::String(v) => v.clone(),
            Js::Undefined => "undefined".encode_utf16().collect(),
            Js::Null => "null".encode_utf16().collect(),
            Js::Bool(v) => v.to_string().encode_utf16().collect(),
            Js::Number(v) => safe_cex_num::js_number_string(*v).encode_utf16().collect(),
            Js::Object(_, Some(v)) => {
                self.event("template")?;
                v.encode_utf16().collect()
            }
            Js::Symbol => return Err(self.type_error.clone()),
            _ => "[object Object]".encode_utf16().collect(),
        })
    }
    fn uppercase(&self, v: Vec<u16>) -> Result<Vec<u16>, Error> {
        self.event("uppercase")?;
        Ok(upper(v))
    }
    fn lookup(&self, table: &'static str, key: Vec<u16>) -> Result<Native, Error> {
        self.event(&format!("lookup-{table}"))?;
        let key = String::from_utf16(&key).unwrap_or_default();
        let found = match table {
            "ORDER_TYPE" => types::ORDER_TYPE
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| Rc::new(Js::Type(*v))),
            "ORDER_SIDE" => types::ORDER_SIDE
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| Rc::new(Js::Side(*v))),
            "ORDER_STATUS" => types::ORDER_STATUS
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| Rc::new(Js::Status(*v))),
            _ => panic!("bad table"),
        };
        Ok(found.unwrap_or_else(|| {
            ["constructor", "toString", "valueOf", "__proto__"]
                .iter()
                .position(|v| *v == key)
                .map(|i| self.inherited[i].clone())
                .unwrap_or_else(|| Rc::new(Js::Undefined))
        }))
    }
    fn order_type(&self, v: OrderType) -> Native {
        Rc::new(Js::Type(v))
    }
    fn order_side(&self, v: OrderSide) -> Native {
        Rc::new(Js::Side(v))
    }
    fn order_status(&self, v: OrderStatus) -> Native {
        Rc::new(Js::Status(v))
    }
}
impl bodies::SymbolHost for Host {
    fn direct_market(&self, symbol: &Native) -> Result<Native, Error> {
        self.event("direct-market")?;
        let proto = matches!(&**symbol,Js::String(v)if ["constructor","toString","valueOf","__proto__"].iter().any(|s|v.iter().copied().eq(s.encode_utf16())));
        Ok(if self.spec["direct"] == true || proto {
            self.ret.clone()
        } else {
            Rc::new(Js::Undefined)
        })
    }
    fn replace_prefix(&self, symbol: Native) -> Result<Native, Error> {
        self.event("prefix")?;
        let Js::String(mut v) = (*symbol).clone_string() else {
            unreachable!()
        };
        if v.len() >= 2 && matches!(v[0], 69 | 101) && matches!(v[1], 45 | 95) {
            v.drain(..2);
        }
        Ok(Rc::new(Js::String(v)))
    }
    fn replace_separators(&self, symbol: Native) -> Result<Native, Error> {
        self.event("separator")?;
        let Js::String(v) = &*symbol else {
            unreachable!()
        };
        Ok(Rc::new(Js::String(
            v.iter()
                .copied()
                .filter(|v| !matches!(v, 45 | 95 | 47))
                .collect(),
        )))
    }
    fn uppercase_value(&self, symbol: Native) -> Result<Native, Error> {
        let Js::String(v) = &*symbol else {
            unreachable!()
        };
        Ok(Rc::new(Js::String(<Self as bodies::ParseHost>::uppercase(
            self,
            v.clone(),
        )?)))
    }
    fn find_market<F: FnMut(Native) -> Result<bool, Error>>(
        &self,
        mut predicate: F,
    ) -> Result<Native, Error> {
        self.event("find")?;
        if self.spec["customFind"] == true {
            return Ok(self.ret.clone());
        }
        for i in 0..self.markets.len() {
            let value = Rc::new(Js::Market(i));
            if predicate(value.clone())? {
                return Ok(value);
            }
        }
        Ok(Rc::new(Js::Undefined))
    }
    fn market_symbol(&self, market: Native) -> Result<Native, Error> {
        self.event("market-symbol")?;
        let Js::Market(i) = &*market else {
            unreachable!()
        };
        Ok(self.markets[*i].clone())
    }
    fn optional_market_symbol(&self, market: Native) -> Result<Native, Error> {
        if matches!(*market, Js::Undefined | Js::Null) {
            Ok(Rc::new(Js::Undefined))
        } else if self.spec["customFind"] == true {
            self.event("found-symbol")?;
            Ok(self.found_symbol.clone())
        } else {
            self.market_symbol(market)
        }
    }
    fn strictly_equal(&self, a: &Native, b: &Native) -> bool {
        match (&**a, &**b) {
            (Js::String(a), Js::String(b)) => a == b,
            _ => Rc::ptr_eq(a, b),
        }
    }
}
impl Js {
    fn clone_string(&self) -> Self {
        if let Self::String(v) = self {
            Self::String(v.clone())
        } else {
            panic!("symbol string")
        }
    }
}
impl bodies::PingHost for Host {
    type Value = Native;
    type Error = Error;
    type Sender = Rc<()>;
    type Stringifier = Rc<()>;
    type Return = Native;
    fn now(&self) -> Result<f64, Error> {
        self.event("now")?;
        Ok(self.spec["clock"].as_f64().unwrap_or(123.5))
    }
    fn set_ping_at(&self, v: f64) -> Result<(), Error> {
        self.event("ping-at")?;
        self.ping_at.set(v);
        Ok(())
    }
    fn ping(&self, json: Native) -> Result<Native, Error> {
        if matches!(*json, Js::Undefined | Js::Null) {
            return Ok(Rc::new(Js::Undefined));
        }
        self.event("ping-get")?;
        if self.spec["stringifyMutatesPing"] == true && self.stringify_ready.get() {
            return Ok(Rc::new(Js::Number(999.0)));
        }
        let i = self.ping_reads.get();
        self.ping_reads.set(i + 1);
        Ok(self
            .pings
            .get(i.min(self.pings.len().saturating_sub(1)))
            .cloned()
            .unwrap_or_else(|| Rc::new(Js::Undefined)))
    }
    fn undefined(&self, v: &Native) -> bool {
        matches!(**v, Js::Undefined)
    }
    fn prepare_send(&self) -> Result<Rc<()>, Error> {
        self.event("send-get")?;
        Ok(self.sender.clone())
    }
    fn prepare_stringify(&self) -> Result<Rc<()>, Error> {
        self.event("stringify-get")?;
        self.stringify_ready.set(true);
        Ok(self.stringifier.clone())
    }
    fn stringify(
        &self,
        prepared: Rc<()>,
        frame: bodies::SyncFrame<Native>,
    ) -> Result<Native, Error> {
        assert!(
            Rc::ptr_eq(&prepared, &self.stringifier),
            "BITRUE-SYNC-NATIVE-STRINGIFIER-IDENTITY"
        );
        self.event("stringify")?;
        let text = self.wire_returns[self.frames.borrow().len()].clone();
        self.frames.borrow_mut().push(frame);
        Ok(text)
    }
    fn pong_text(&self) -> Native {
        text("pong")
    }
    fn send(&self, sender: Rc<()>, value: Native) -> Result<Native, Error> {
        assert!(Rc::ptr_eq(&sender, &self.sender));
        self.event("send")?;
        self.sent.borrow_mut().push(value);
        Ok(self.ret.clone())
    }
    fn handle_pong(&self, delay: f64) -> Result<Native, Error> {
        self.event("pong-get")?;
        self.event("pong")?;
        assert_eq!(delay.to_bits(), 20000.0f64.to_bits());
        Ok(self.ret.clone())
    }
}
impl bodies::SubscriptionHost for Host {
    fn channels_for_each<F: FnMut(Native) -> Result<(), Error>>(
        &self,
        channels: &[&str],
        mut callback: F,
    ) -> Result<Native, Error> {
        self.event("for-each")?;
        assert_eq!(
            channels,
            [
                "user_account_update",
                "user_order_update",
                "user_balance_update"
            ]
        );
        if self.spec["customChannels"] == true {
            for v in [self.channel_object.clone(), text("user_balance_update")] {
                callback(v)?;
                self.callbacks.borrow_mut().push(Rc::new(Js::Undefined));
            }
        } else {
            for channel in channels {
                callback(text(channel))?;
            }
        }
        Ok(self.ret.clone())
    }
    fn prepare_send_with_log(&self) -> Result<Rc<()>, Error> {
        self.event("logged-send-get")?;
        Ok(self.logged_sender.clone())
    }
    fn send_with_log(&self, sender: Rc<()>, value: Native) -> Result<Native, Error> {
        assert!(Rc::ptr_eq(&sender, &self.logged_sender));
        self.event("logged-send")?;
        self.sent.borrow_mut().push(value);
        Ok(self.ret.clone())
    }
}
impl bodies::RefreshTimerHost for Host {
    type Clearer = Rc<()>;
    type Value = Native;
    type Error = Error;
    type Return = Native;
    fn timeout(&self) -> Result<Native, Error> {
        self.event("timer-get")?;
        let n = self.timer_reads.get() + 1;
        self.timer_reads.set(n);
        Ok(if n == 2 && self.spec["replaceTimer"] == true {
            self.new_timer.clone()
        } else {
            self.timer.borrow().clone()
        })
    }
    fn truthy_timeout(&self, v: &Native) -> bool {
        <Self as bodies::ParseHost>::truthy(self, v)
    }
    fn prepare_clear_timeout(&self) -> Result<Rc<()>, Error> {
        self.event("clear-get")?;
        Ok(self.clearer.clone())
    }
    fn clear_timeout(&self, prepared: Rc<()>, v: Native) -> Result<(), Error> {
        assert!(Rc::ptr_eq(&prepared, &self.clearer));
        self.event("clear")?;
        assert!(
            Rc::ptr_eq(
                &v,
                if self.spec["replaceTimer"] == true {
                    &self.new_timer
                } else {
                    &self.initial_timer
                }
            ),
            "BITRUE-SYNC-NATIVE-CLEAR-IDENTITY"
        );
        Ok(())
    }
    fn set_timeout<F: FnMut() -> Result<Native, Error> + 'static>(
        &self,
        mut callback: F,
        delay: f64,
    ) -> Result<Native, Error> {
        self.event("set-timeout")?;
        assert_eq!(
            delay.to_bits(),
            1800000.0f64.to_bits(),
            "BITRUE-SYNC-NATIVE-REFRESH-DELAY"
        );
        if self.spec["immediateCallback"] == true {
            let value = callback()?;
            assert!(Rc::ptr_eq(&value, &self.ret));
            self.callbacks.borrow_mut().push(value);
        }
        self.callback.replace(Some(Box::new(callback)));
        Ok(self.new_timer.clone())
    }
    fn assign_timeout(&self, v: Native) -> Result<(), Error> {
        self.event("timer-set")?;
        self.timer.replace(v);
        Ok(())
    }
    fn update_listen_key(&self, key: Native) -> Result<Native, Error> {
        self.event("update-get")?;
        self.event("update")?;
        assert!(
            Rc::ptr_eq(&key, &self.key),
            "BITRUE-SYNC-NATIVE-KEY-IDENTITY"
        );
        Ok(self.ret.clone())
    }
    fn undefined_timeout(&self) -> Native {
        Rc::new(Js::Undefined)
    }
    fn base_dispose(&self) -> Result<Native, Error> {
        self.event("base-get")?;
        self.event("base-dispose")?;
        Ok(self.ret.clone())
    }
}
fn observed(v: &Value) -> Native {
    match v["kind"].as_str().unwrap() {
        "undefined" => Rc::new(Js::Undefined),
        "null" => Rc::new(Js::Null),
        "number" => Rc::new(Js::Number(f64::from_bits(
            u64::from_str_radix(v["bits"].as_str().unwrap(), 16).unwrap(),
        ))),
        "boolean" => Rc::new(Js::Bool(v["value"].as_bool().unwrap())),
        "string" => Rc::new(Js::String(units(&v["units"]))),
        "symbol" => Rc::new(Js::Symbol),
        "object" => Rc::new(Js::Object(v["tag"].as_str().unwrap().into(), None)),
        _ => panic!("unknown observation"),
    }
}
#[test]
fn sync_bodies_match_actual_typescript_native() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/ws_sync_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "BITRUE-SYNC-SOURCE {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let fixtures: Value = serde_json::from_str(include_str!("ws_sync_fixtures.json")).unwrap();
    for (spec, source) in fixtures
        .as_array()
        .unwrap()
        .iter()
        .zip(corpus["rows"].as_array().unwrap())
    {
        let name = spec["name"].as_str().unwrap();
        let old = Rc::new(Js::Object("old-timer".into(), None));
        let timer = if spec["timer"] == "object" {
            old.clone()
        } else if spec.get("timer").is_none() {
            Rc::new(Js::Undefined)
        } else {
            input(&spec["timer"])
        };
        let host = Rc::new(Host {
            spec: spec.clone(),
            events: RefCell::new(vec![]),
            frames: RefCell::new(vec![]),
            sent: RefCell::new(vec![]),
            callbacks: RefCell::new(vec![]),
            callback: RefCell::new(None),
            ping_reads: Cell::new(0),
            timer_reads: Cell::new(0),
            ping_at: Cell::new(-1.0),
            timer: RefCell::new(timer.clone()),
            initial_timer: timer,
            new_timer: Rc::new(Js::Object("new-timer".into(), None)),
            ret: Rc::new(Js::Object("return".into(), None)),
            error: Rc::new(Js::Object("error".into(), None)),
            type_error: Rc::new(Js::Object("type-error".into(), None)),
            sender: Rc::new(()),
            logged_sender: Rc::new(()),
            stringifier: Rc::new(()),
            clearer: Rc::new(()),
            channel_object: Rc::new(Js::Object("channel-object".into(), None)),
            key: spec
                .get("key")
                .map(input)
                .unwrap_or_else(|| Rc::new(Js::Object("key".into(), Some("key".into())))),
            pings: spec["pings"]
                .as_array()
                .map(|v| v.iter().map(input).collect())
                .unwrap_or_default(),
            markets: spec["markets"]
                .as_array()
                .map(|v| v.iter().map(input).collect())
                .unwrap_or_default(),
            inherited: ["constructor", "toString", "valueOf", "__proto__"]
                .iter()
                .map(|v| Rc::new(Js::Object(format!("inherited-{v}"), None)))
                .collect(),
            raw: spec
                .get("raw")
                .map(input)
                .unwrap_or_else(|| Rc::new(Js::Undefined)),
            stringify_ready: Cell::new(false),
            found_symbol: spec
                .get("foundSymbol")
                .map(input)
                .unwrap_or_else(|| Rc::new(Js::Undefined)),
            wire_returns: source["wireReturns"]
                .as_array()
                .unwrap()
                .iter()
                .map(observed)
                .collect(),
        });
        let json = if spec["jsonUndefined"] == true {
            Rc::new(Js::Undefined)
        } else {
            Rc::new(Js::Object("json".into(), None))
        };
        let mut result = match spec["op"].as_str().unwrap() {
            "type" => bodies::parse_order_type(&*host, host.raw.clone()),
            "side" => bodies::parse_order_side(&*host, host.raw.clone()),
            "status" => bodies::parse_order_status(
                &*host,
                spec.get("execution")
                    .map(input)
                    .unwrap_or_else(|| Rc::new(Js::Undefined)),
                spec.get("status")
                    .map(input)
                    .unwrap_or_else(|| Rc::new(Js::Undefined)),
            ),
            "public-symbol" => bodies::resolve_market_symbol(&*host, host.raw.clone()),
            "public-ping" => {
                bodies::handle_public_ping(&*host, json).map(|_| Rc::new(Js::Undefined))
            }
            "private-ping" => {
                bodies::handle_private_ping(&*host, json).map(|_| Rc::new(Js::Undefined))
            }
            "subscribe" => bodies::subscribe_private(&*host).map(|_| Rc::new(Js::Undefined)),
            "schedule" => bodies::schedule_listen_key_refresh(host.clone(), host.key.clone())
                .map(|_| Rc::new(Js::Undefined)),
            "dispose" => bodies::dispose_private(&*host).map(|_| Rc::new(Js::Undefined)),
            _ => panic!("bad operation"),
        };
        if result.is_ok() {
            assert_eq!(
                *host.events.borrow(),
                source["immediate"]["events"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap())
                    .collect::<Vec<_>>(),
                "BITRUE-SYNC-NATIVE-IMMEDIATE-EFFECTS {name}"
            );
            native(
                &host.timer.borrow(),
                &source["immediate"]["timer"],
                &format!("{name} immediate timer"),
            );
            if spec["fire"] == true {
                if let Err(e) = host.fire() {
                    result = Err(e);
                }
            }
        }
        match result {
            Ok(value) => {
                assert!(
                    source["thrown"].is_null(),
                    "BITRUE-SYNC-NATIVE-RETURN {name}"
                );
                native(&value, &source["result"], name);
                if let Js::Object(tag, _) = &*value {
                    if tag.starts_with("inherited-") {
                        assert!(
                            host.inherited.iter().any(|v| Rc::ptr_eq(&value, v)),
                            "BITRUE-SYNC-NATIVE-INHERITED-IDENTITY {name}"
                        );
                    } else if tag == "found" {
                        assert!(
                            Rc::ptr_eq(&value, &host.found_symbol),
                            "BITRUE-SYNC-NATIVE-FOUND-IDENTITY {name}"
                        );
                    }
                }
            }
            Err(error) => {
                assert!(
                    !source["thrown"].is_null(),
                    "BITRUE-SYNC-NATIVE-RETURN {name}"
                );
                native(&error, &source["thrown"], name);
                assert!(
                    Rc::ptr_eq(
                        &error,
                        if matches!(&*error,Js::Object(tag,_)if tag=="type-error") {
                            &host.type_error
                        } else {
                            &host.error
                        }
                    ),
                    "BITRUE-SYNC-NATIVE-ERROR-IDENTITY {name}"
                );
            }
        }
        native(
            &Rc::new(Js::Number(host.ping_at.get())),
            &source["pingAt"],
            &format!("{name} ping-at"),
        );
        let timer = host.timer.borrow().clone();
        native(&timer, &source["timer"], &format!("{name} timer"));
        assert_eq!(
            Rc::ptr_eq(&timer, &host.initial_timer),
            source["timerOriginal"].as_bool().unwrap(),
            "BITRUE-SYNC-NATIVE-TIMER-IDENTITY {name}"
        );
        if matches!(&*timer,Js::Object(tag,_)if tag=="new-timer") {
            assert!(
                Rc::ptr_eq(&timer, &host.new_timer),
                "BITRUE-SYNC-NATIVE-NEW-TIMER-IDENTITY {name}"
            );
        }
        for (i, (frame, s)) in host
            .frames
            .borrow()
            .iter()
            .zip(source["frames"].as_array().unwrap())
            .enumerate()
        {
            match frame {
                bodies::SyncFrame::Pong(v) => {
                    native(v, &s["pong"], &format!("{name} frame{i} pong"));
                    if matches!(**v, Js::Object(_, _)) {
                        assert!(
                            host.pings.iter().any(|p| Rc::ptr_eq(v, p)),
                            "BITRUE-SYNC-NATIVE-PING-IDENTITY {name}"
                        );
                    }
                }
                bodies::SyncFrame::EventPong => native(
                    &text("pong"),
                    &s["event"],
                    &format!("{name} frame{i} event"),
                ),
                bodies::SyncFrame::PrivateSubscription(v) => {
                    if matches!(&**v, Js::Object(tag, _) if tag == "channel-object") {
                        assert!(
                            Rc::ptr_eq(v, &host.channel_object),
                            "BITRUE-SYNC-NATIVE-CHANNEL-IDENTITY {name}"
                        );
                    }
                    native(
                        v,
                        &s["params"]["channel"],
                        &format!("{name} frame{i} channel"),
                    );
                    native(&text("sub"), &s["event"], &format!("{name} frame{i} event"));
                }
            }
        }
        for (value, s) in host
            .callbacks
            .borrow()
            .iter()
            .zip(source["callbackResults"].as_array().unwrap())
        {
            native(value, s, &format!("{name} callback"));
        }
        assert_eq!(
            *host.events.borrow(),
            source["events"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>(),
            "BITRUE-SYNC-NATIVE-EFFECTS {name}"
        );
        // Native frame/value/state/error assertions precede wire forwarding and inventories.
        for (i, (value, s)) in host
            .sent
            .borrow()
            .iter()
            .zip(source["sent"].as_array().unwrap())
            .enumerate()
        {
            native(value, s, &format!("{name} sent{i}"));
            if !matches!(&**value,Js::String(v)if v.iter().copied().eq("pong".encode_utf16())) {
                assert!(
                    host.wire_returns.iter().any(|wire| Rc::ptr_eq(value, wire)),
                    "BITRUE-SYNC-NATIVE-STRINGIFY-RETURN-IDENTITY {name}"
                );
            }
        }
        assert_eq!(
            host.frames.borrow().len(),
            source["frames"].as_array().unwrap().len(),
            "BITRUE-SYNC-FRAME-FLOOR {name}"
        );
        assert_eq!(
            host.sent.borrow().len(),
            source["sent"].as_array().unwrap().len(),
            "BITRUE-SYNC-SEND-FLOOR {name}"
        );
        assert_eq!(
            host.callbacks.borrow().len(),
            source["callbackResults"].as_array().unwrap().len(),
            "BITRUE-SYNC-CALLBACK-FLOOR {name}"
        );
        assert_eq!(source["name"], spec["name"]);
    }
    assert_eq!(
        fixtures.as_array().unwrap().len(),
        166,
        "independent input floor"
    );
    assert_eq!(
        corpus["rows"].as_array().unwrap().len(),
        166,
        "independent output floor"
    );
    assert_eq!(corpus["rows"][165]["name"], "schedule:immediate");
}
