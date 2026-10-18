// Per-case expectation tables: arms that currently coincide are kept separate
// because each names an independent case, and collapsing them merges two
// expectations into one. possible_missing_else: the minified bodies here put
// `}if` straight after a guard `return`, and there is no else to add.
#![allow(clippy::if_same_then_else, clippy::possible_missing_else)]
//! Every family's native assertions precede its actual TypeScript comparisons.
#[path = "support/canonical_value.rs"]
mod canonical_value;
use canonical_value::category;
use safe_cex_exchange::runtime::{ExchangeError, LogSeverity};
use safe_cex_types::{MarginType, PositionMode, PositionSide};
use safe_cex_venue_binance::{
    position_snapshots::*,
    private_event_value::*,
    private_rest::{SnapshotClient, SnapshotCompletion, SnapshotRequestFuture},
    request_weight::Failure,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn ev(v: Value) -> EventValue {
    EventValue::from_json(v)
}
fn array(v: Vec<EventValue>) -> EventValue {
    EventValue::array(v)
}
fn prop(v: &EventValue, k: &str) -> EventValue {
    v.get(k).unwrap()
}
fn path(v: &EventValue, keys: &[&str]) -> EventValue {
    keys.iter().fold(v.clone(), |v, k| {
        if v.nullish() {
            EventValue::Undefined
        } else {
            prop(&v, k)
        }
    })
}
fn wire(v: &EventValue) -> Value {
    match v {
        EventValue::Undefined => json!({"$undefined":true}),
        EventValue::Null => Value::Null,
        EventValue::Bool(x) => json!(x),
        EventValue::String(x) => json!(String::from_utf16(x).unwrap()),
        EventValue::Number(x) => {
            if x.is_nan() {
                json!({"$nan":true})
            } else if x.is_infinite() {
                json!({"$infinity":if x.is_sign_positive(){1}else{-1}})
            } else if *x == 0.0 && x.is_sign_negative() {
                json!({"$negativeZero":true})
            } else {
                json!(x)
            }
        }
        EventValue::Builtin(reference) => {
            if reference.is_callable() {
                json!({"$function":reference.js_string()})
            } else {
                reference
                    .own_fields()
                    .into_iter()
                    .map(|(key, value)| (key, wire(&EventValue::from_prototype(value.into()))))
                    .collect()
            }
        }
        EventValue::Array(a) => array_values(a)
            .expect("fixture array reads")
            .iter()
            .map(wire)
            .collect(),
        EventValue::Object(_) => v
            .spread()
            .iter()
            .map(|(k, v)| (k.clone(), wire(v)))
            .collect(),
        _ => panic!("fixture projection requires decoded data"),
    }
}
fn num(v: EventValue, w: f64, label: &str) {
    let EventValue::Number(x) = v else {
        panic!("{label}: native Number")
    };
    assert!(
        if w.is_nan() { x.is_nan() } else { x == w },
        "{label}: native {x:?} vs expected {w:?}"
    );
}
fn cases(f: &str) -> Vec<String> {
    let v: Value = serde_json::from_str(include_str!("fixtures/position-snapshots.json")).unwrap();
    let a = v[f].as_array().unwrap();
    assert_eq!(
        a.len(),
        if f == "balance" { 26 } else { 24 },
        "independent fixture floor"
    );
    a.iter().map(|x| x.as_str().unwrap().into()).collect()
}
fn error() -> Failure {
    let mut e = ExchangeError::new("position snapshot fixture");
    e.name = "RangeError".into();
    e.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
        -1000
    )));
    e.response = Some(json!({"status":503,"data":{"msg":"position venue detail"}}));
    Rc::new(e)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Futures,
    PM,
    Balance,
}
struct Host {
    kind: Kind,
    name: String,
    risk: EventValue,
    configs: EventValue,
    account: EventValue,
    markets: EventValue,
    tickers: EventValue,
    cached: EventValue,
    alternate: EventValue,
    current: RefCell<EventValue>,
    balance: EventValue,
    ready: Rc<Cell<bool>>,
    hedged: Cell<bool>,
    logged: Cell<bool>,
    gets: RefCell<Vec<(SnapshotClient, &'static str)>>,
    ops: Rc<RefCell<Vec<&'static str>>>,
    ids: RefCell<Vec<(String, EventValue, PositionMode)>>,
    emits: Rc<RefCell<Vec<EventValue>>>,
    logs: Rc<RefCell<Vec<(String, LogSeverity)>>>,
    market_reads: Cell<usize>,
    ticker_reads: Cell<usize>,
    primary: Failure,
    secondary: Failure,
}
impl Host {
    fn new(k: Kind, name: &str) -> Rc<Self> {
        let mut risk = ev(json!([
 {"symbol":"BTC","positionSide":"LONG","positionAmt":"3.6","entryPrice":"12.3","notional":"-44.28","leverage":"4.2","unRealizedProfit":"1.2","liquidationPrice":"9.1","isolatedMargin":"2.5","nested":{"value":8.1}},
 {"symbol":"BTC","positionSide":"SHORT","positionAmt":"-2.5","entryPrice":"9.1","notional":"-22.75","leverage":"10","unRealizedProfit":"-0.5","liquidationPrice":"12.3","isolatedMargin":"0"},
 {"symbol":"BTC","positionSide":"LONG","positionAmt":"8","entryPrice":"88","notional":"8","leverage":"4","unRealizedProfit":"0","liquidationPrice":"0"}]));
        let mut configs = ev(
            json!([{"symbol":"BTC","leverage":"6","marginType":"ISOLATED"},{"symbol":"ETH","leverage":"3.6","marginType":"CROSSED"}]),
        );
        let markets = ev(
            json!([{"symbol":"BTC","base":"BTC","quote":"USDT"},{"symbol":"ETH","base":"ETH","quote":"USDT"}]),
        );
        let tickers =
            ev(json!({"BTC":{"symbol":"BTC","last":4.2},"ETH":{"symbol":"ETH","last":2.5}}));
        let mut account = ev(
            json!({"assets":[{"asset":"USDT","walletBalance":"12.3","availableBalance":"4.2","initialMargin":"2.5","unrealizedProfit":"1.2"},{"asset":"BTC","walletBalance":"3.6","availableBalance":"1.2","initialMargin":"0.5","unrealizedProfit":"-0.2"}],"positions":[{"compact":true}],"totalWalletBalance":"9.1","availableBalance":"2.5","totalUnrealizedProfit":"0.6","totalInitialMargin":"3.6","nested":{"value":8.1}}),
        );
        if k == Kind::Balance && name == "string-price" {
            prop(&tickers, "BTC").assign("last", string("2.125"));
        }
        if k == Kind::Balance && name == "bool-price" {
            prop(&tickers, "BTC").assign("last", EventValue::Bool(true));
        }
        if k != Kind::Balance {
            match name {
                "oneway" => {
                    for i in 0..3 {
                        prop(&risk, &i.to_string()).assign("positionSide", string("BOTH"));
                    }
                }
                "missing-config" => configs = array(vec![]),
                "duplicate-config" => {
                    let EventValue::Array(a) = &configs else {
                        unreachable!()
                    };
                    EventValue::Array(a.clone())
                        .push(ev(
                            json!({"symbol":"BTC","leverage":"2.5","marginType":"CROSSED"}),
                        ))
                        .unwrap()
                }
                "zero-leverage" => {
                    prop(&configs, "0").assign("leverage", string("0"));
                    for i in 0..3 {
                        let p = prop(&risk, &i.to_string());
                        p.assign("leverage", string("0"));
                        p.assign("isolatedMargin", string("0"));
                    }
                }
                "isolated-alias" => {
                    let p = prop(&risk, "0");
                    let EventValue::Object(o) = &p else {
                        unreachable!()
                    };
                    o.props.borrow_mut().shift_remove(
                        safe_cex_exchange::js_runtime::value::utf16("isolatedMargin").as_ref(),
                    );
                    p.assign("isolated_margin", string("3.6"));
                }
                "empty" => risk = array(vec![]),
                "bad-risk" => risk = ev(json!({})),
                "null-configs" => configs = EventValue::Null,
                "pm-infinity" if k == Kind::PM => risk = number(f64::INFINITY),
                "proto-config" => {
                    for i in 0..3 {
                        prop(&risk, &i.to_string()).assign("symbol", string("__proto__"));
                    }
                    prop(&configs, "0").assign("symbol", string("__proto__"));
                    prop(&markets, "0").assign("symbol", string("__proto__"));
                    prop(&markets, "1").assign("symbol", string("leverage"));
                }
                _ => {}
            }
        } else {
            match name{"direct"|"btc-route"|"usdc-route"|"usdt-zero"=>{path(&account,&["assets","1"]).assign("asset",string("ABC"));let EventValue::Array(a)=&markets else{unreachable!()};native_array_fixture::replace(a,if name=="direct"{vec![Some(ev(json!({"symbol":"ABCUSDC","base":"ABC","quote":"USDC"}))),Some(ev(json!({"symbol":"ABCUSDT","base":"ABC","quote":"USDT"})))]}else{vec![Some(ev(json!({"symbol":"ABCBTC","base":"ABC","quote":"BTC"})))]});for(s,last)in[("ABCUSDC",2.125),("ABCUSDT",9.1),("ABCBTC",2.5),("BTCUSDT",4.2),("BTCUSDC",3.6)]{tickers.assign(s,ev(json!({"last":last})));}for s in["BTCUSDT","BTCUSDC"]{prop(&tickers,s).assign("symbol",string(s));}if name=="usdc-route"{let EventValue::Object(o)=&tickers else{unreachable!()};o.props.borrow_mut().shift_remove(safe_cex_exchange::js_runtime::value::utf16("BTCUSDT").as_ref());}if name=="usdt-zero"{prop(&tickers,"BTCUSDT").assign("last",number(0.0));}},"negative-stable"|"negative-nonstable"=>account.assign("assets",ev(json!([{"asset":if name=="negative-stable"{"USDT"}else{"NO"},"walletBalance":"-3.6","availableBalance":"0","initialMargin":"0","unrealizedProfit":"0"}]))),"upnl-only"=>account.assign("assets",ev(json!([{"asset":"BTC","walletBalance":"0","availableBalance":"0","initialMargin":"0","unrealizedProfit":"3.6"}]))),"zero"=>account.assign("assets",array(vec![])),"nan"=>path(&account,&["assets","0"]).assign("walletBalance",string("bad")),"error-log-mutates"|"error-log-error"|"bad-assets"=>account.assign("assets",EventValue::Bool(true)),"null"=>account=EventValue::Null,_=>{}}
        }
        let cached = ev(json!([{"id":"cached","contracts":4.2}]));
        let mut secondary = ExchangeError::new("config first");
        secondary.name = "RangeError".into();
        secondary.response = Some(json!({"status":504,"data":{"msg":"config detail"}}));
        Rc::new(Self {
            kind: k,
            name: name.into(),
            risk,
            configs,
            account,
            markets,
            tickers,
            cached: cached.clone(),
            alternate: ev(json!([{"id":"alternate","contracts":9.1}])),
            current: RefCell::new(cached),
            balance: ev(json!({"total":9.1,"free":4.2,"used":2.5,"upnl":3.6})),
            ready: Rc::new(Cell::new(false)),
            hedged: Cell::new(!matches!(name, "oneway" | "hedged-after-await")),
            logged: Cell::new(name == "logged"),
            gets: Default::default(),
            ops: Default::default(),
            ids: Default::default(),
            emits: Default::default(),
            logs: Default::default(),
            market_reads: Cell::new(0),
            ticker_reads: Cell::new(0),
            primary: error(),
            secondary: Rc::new(secondary),
        })
    }
    fn rejecting(&self) -> bool {
        matches!(
            self.name.as_str(),
            "get-reject" | "emit-error" | "capture-error" | "fallback-error"
        )
    }
}
#[derive(Clone)]
struct Bindings(Rc<Host>);
impl std::ops::Deref for Bindings {
    type Target = Rc<Host>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl PositionSnapshotBindings for Bindings {
    fn get(&self, c: SnapshotClient, ep: &'static str) -> Result<SnapshotRequestFuture, Failure> {
        self.gets.borrow_mut().push((c, ep));
        self.ops.borrow_mut().push("get");
        let n = self.gets.borrow().len();
        if self.name == "get-sync-first" && n == 1 || self.name == "get-sync-second" && n == 2 {
            return Err(self.primary.clone());
        }
        let ready = self.ready.clone();
        let data = if self.kind == Kind::Balance {
            self.account.clone()
        } else if n == 1 {
            self.risk.clone()
        } else {
            self.configs.clone()
        };
        let both = self.name == "second-error-first" && self.kind == Kind::Futures;
        let result = if both {
            Err(if n == 1 {
                self.primary.clone()
            } else {
                self.secondary.clone()
            })
        } else if (self.rejecting() && n == 1) || (self.name == "config-reject" && n == 2) {
            Err(self.primary.clone())
        } else {
            Ok(data)
        };
        Ok(Box::pin(std::future::poll_fn(move |_| {
            if ready.get() {
                Poll::Ready(SnapshotCompletion {
                    reaction_order: if both {
                        if n == 1 { 1 } else { 0 }
                    } else {
                        n as u64
                    },
                    result: result.clone(),
                })
            } else {
                Poll::Pending
            }
        })))
    }
    fn positions(&self) -> Result<EventValue, Failure> {
        self.ops.borrow_mut().push("positions");
        if self.name == "fallback-error" {
            Err(self.primary.clone())
        } else {
            Ok(self.current.borrow().clone())
        }
    }
    fn balance(&self) -> Result<EventValue, Failure> {
        self.ops.borrow_mut().push("balance");
        if self.name == "fallback-error" {
            Err(self.primary.clone())
        } else {
            Ok(self.balance.clone())
        }
    }
    fn markets(&self) -> Result<EventValue, Failure> {
        self.ops.borrow_mut().push("markets");
        self.market_reads.set(self.market_reads.get() + 1);
        if self.market_reads.get() == 1 && self.name == "market-mutates-asset" {
            path(&self.account, &["assets", "1"]).assign("asset", string("ETH"));
        }
        Ok(self.markets.clone())
    }
    fn tickers(&self) -> Result<EventValue, Failure> {
        self.ops.borrow_mut().push("tickers");
        self.ticker_reads.set(self.ticker_reads.get() + 1);
        if self.ticker_reads.get() == 1 && self.name == "ticker-appends" {
            let EventValue::Array(a) = prop(&self.account, "assets") else {
                unreachable!()
            };
            EventValue::Array(a.clone()).push(ev(json!({"asset":"USDC","walletBalance":"1.2","availableBalance":"0.5","initialMargin":"0.1","unrealizedProfit":"0.2"}))).unwrap();
        }
        Ok(self.tickers.clone())
    }
    fn hedged(&self) -> Result<bool, Failure> {
        Ok(self.hedged.get())
    }
    fn id(&self, s: String, side: EventValue, mode: PositionMode) -> Result<String, Failure> {
        self.ids.borrow_mut().push((s.clone(), side.clone(), mode));
        self.ops.borrow_mut().push("id");
        let n = self.ids.borrow().len();
        if self.name == "id-error" {
            return Err(self.primary.clone());
        }
        if self.name == "id-mutates-row" && n == 1 {
            let p = prop(&self.risk, "0");
            p.assign("entryPrice", string("4.2"));
            p.assign("symbol", string("ALT"));
        }
        if self.name == "id-mutates-config" && n == 1 {
            if let EventValue::Array(a) = &self.configs {
                if let Some(Some(c)) = native_array_fixture::slots(&a).first() {
                    c.assign("leverage", string("2.5"));
                    c.assign("marginType", string("CROSSED"));
                }
            }
        }
        if self.name == "markets-append" && n == 4 {
            let EventValue::Array(a) = &self.markets else {
                unreachable!()
            };
            EventValue::Array(a.clone())
                .push(ev(json!({"symbol":"DOGE"})))
                .unwrap();
        }
        let side = category::<PositionSide>(&side);
        Ok(safe_cex_types::model_utils::create_position_id(
            &s,
            Some(side),
            Some(mode),
        ))
    }
    fn capture_emit(&self) -> Result<Emit, Failure> {
        self.ops.borrow_mut().push("capture-emit");
        if self.name == "capture-error" {
            return Err(self.primary.clone());
        }
        let h = self.clone();
        Ok(Rc::new(move |m| {
            h.ops.borrow_mut().push("emit");
            h.emits.borrow_mut().push(m);
            if h.name == "emit-error" {
                Err(h.primary.clone())
            } else {
                Ok(())
            }
        }))
    }
    fn capture_log(&self) -> Result<Log, Failure> {
        self.ops.borrow_mut().push("capture-log");
        if self.name == "log-capture-error" {
            return Err(self.primary.clone());
        }
        let h = self.clone();
        Ok(Rc::new(move |m, s| {
            h.ops.borrow_mut().push("log");
            h.logs.borrow_mut().push((m, s));
            if s == LogSeverity::Info && h.name == "positions-mutated" {
                *h.current.borrow_mut() = h.alternate.clone();
            }
            if s == LogSeverity::Info && h.name == "info-error"
                || s == LogSeverity::Error && h.name == "error-log-error"
            {
                return Err(h.primary.clone());
            }
            if s == LogSeverity::Error && h.name == "error-log-mutates" {
                for (k, v) in [
                    ("totalWalletBalance", "12.3"),
                    ("availableBalance", "4.2"),
                    ("totalUnrealizedProfit", "1.2"),
                    ("totalInitialMargin", "3.6"),
                ] {
                    h.account.assign(k, string(v));
                }
            }
            Ok(())
        }))
    }
}
fn native(k: Kind, name: &str) -> Value {
    let h = Host::new(k, name);
    let b: Rc<dyn PositionSnapshotBindings> = Rc::new(Bindings(h.clone()));
    let mut f: std::pin::Pin<Box<dyn Future<Output = Result<EventValue, Failure>> + '_>> = match k {
        Kind::Futures => Box::pin(fetch_futures_positions(b)),
        Kind::PM => Box::pin(fetch_pm_positions(b)),
        Kind::Balance => Box::pin(fetch_futures_positions_and_balance(&h.logged, b)),
    };
    let mut cx = Context::from_waker(Waker::noop());
    let r = match f.as_mut().poll(&mut cx) {
        Poll::Ready(r) => r,
        Poll::Pending => {
            assert_eq!(
                h.gets.borrow().len(),
                if k == Kind::Futures { 2 } else { 1 },
                "native both HTTP starts before await"
            );
            assert!(
                h.ids.borrow().is_empty() && h.emits.borrow().is_empty(),
                "native no mapping before settlement"
            );
            if name == "hedged-after-await" {
                h.hedged.set(true);
            }
            h.ready.set(true);
            let Poll::Ready(r) = f.as_mut().poll(&mut cx) else {
                panic!("fixture ready")
            };
            r
        }
    };
    assert_eq!(
        h.gets.borrow().len(),
        if k == Kind::Futures && name != "get-sync-first" {
            2
        } else {
            1
        },
        "native independent request count"
    );
    for (i, (client, ep)) in h.gets.borrow().iter().enumerate() {
        assert_eq!(
            *client,
            if k == Kind::PM {
                SnapshotClient::Papi
            } else {
                SnapshotClient::Private
            },
            "native current HTTP client role"
        );
        assert_eq!(
            *ep,
            match (k, i) {
                (Kind::Futures, 0) => "/fapi/v3/positionRisk",
                (Kind::Futures, 1) => "/fapi/v1/symbolConfig",
                (Kind::PM, 0) => "/papi/v1/um/positionRisk",
                (Kind::Balance, 0) => "/fapi/v3/account",
                _ => unreachable!(),
            },
            "native literal endpoint"
        );
    }
    let failed = matches!(name, "emit-error" | "fallback-error" | "capture-error");
    assert_eq!(r.is_err(), failed, "native {k:?} {name}: outcome");
    if let Err(e) = &r {
        assert!(Rc::ptr_eq(e, &h.primary), "native original Error Rc");
        assert_eq!(e.name, "RangeError");
        assert_eq!(e.message, "position snapshot fixture");
        assert_eq!(observed_error_code(&e.code), Some(json!(-1000)));
        assert_eq!(
            e.response,
            Some(json!({"status":503,"data":{"msg":"position venue detail"}}))
        );
    }
    let value = r.as_ref().cloned().unwrap_or(EventValue::Undefined);
    if k != Kind::Balance {
        check_positions(&h, &value, name);
    } else {
        check_balance(&h, &value, name);
    }
    let input = if let EventValue::Array(a) = &h.risk {
        array_values(a).expect("fixture array reads")
    } else {
        vec![]
    };
    let rows = if let EventValue::Array(a) = &value {
        array_values(a).expect("fixture array reads")
    } else {
        vec![]
    };
    json!({"name":name,"gets":h.gets.borrow().iter().map(|(c,e)|json!([if *c==SnapshotClient::Papi{"papi"}else{"private"},e])).collect::<Vec<_>>(),"ops":*h.ops.borrow(),"ids":h.ids.borrow().iter().map(|(s,side,m)|json!([s,wire(side),serde_json::to_value(m).unwrap()])).collect::<Vec<_>>(),"emits":h.emits.borrow().iter().map(wire).collect::<Vec<_>>(),"logs":h.logs.borrow().iter().map(|(m,s)|json!({"message":m,"severity":s.as_str()})).collect::<Vec<_>>(),"logged":h.logged.get(),"value":wire(&value),"failure":r.as_ref().err().map(|e|json!({"original":Rc::ptr_eq(e,&h.primary),"name":e.name,"message":e.message,"code":observed_error_code(&e.code),"response":e.response})),"refs":{"fallback":value.same(&h.cached),"balance":path(&value,&["balance"]).same(&h.balance),"positions":path(&value,&["positions"]).same(&h.current.borrow()),"originalInfo":rows.iter().filter(|p|input.iter().any(|i|prop(p,"info").same(i))).count(),"assets":path(&value,&["balance","info","assets"]).same(&path(&h.account,&["assets"])),"rest":!path(&value,&["balance","info"]).same(&h.account)},"marketCount":if let EventValue::Array(a)=&h.markets{a.len()}else{0}})
}
fn check_positions(h: &Host, v: &EventValue, name: &str) {
    let fallback = h.rejecting()
        || matches!(name, "get-sync-first" | "bad-risk" | "id-error")
        || (h.kind == Kind::Futures
            && matches!(
                name,
                "get-sync-second" | "config-reject" | "second-error-first" | "null-configs"
            ))
        || (h.kind == Kind::PM && name == "pm-infinity");
    if fallback {
        assert_eq!(
            h.emits.borrow().len(),
            usize::from(name != "capture-error"),
            "native independent position emission count"
        );
        if !matches!(name, "emit-error" | "capture-error" | "fallback-error") {
            assert!(v.same(&h.cached), "native original fallback positions");
        }
        let expected = if name == "bad-risk" {
            if h.kind == Kind::PM {
                "Binance PM /papi/v1/um/positionRisk response malformed: expected array, received object"
            } else {
                "data.map is not a function"
            }
        } else if name == "null-configs" {
            "Cannot read properties of null (reading 'map')"
        } else if name == "pm-infinity" {
            "Binance PM /papi/v1/um/positionRisk response malformed: expected array, received number"
        } else if name == "second-error-first" {
            "config detail"
        } else {
            "position venue detail"
        };
        if let Some(m) = h.emits.borrow().first() {
            assert!(
                m.same(&string(expected)),
                "native {name} exact error message: {m:?}"
            );
        }
        assert_eq!(
            h.ids.borrow().len(),
            usize::from(name == "id-error"),
            "native error partial ID count"
        );
        return;
    }
    assert!(h.emits.borrow().is_empty() && h.logs.borrow().is_empty());
    let EventValue::Array(a) = v else {
        panic!("native position array")
    };
    let n = if name == "oneway" { 2 } else { 4 };
    assert_eq!(a.len(), n, "native independent unique position count");
    assert_eq!(
        h.ids.borrow().len(),
        if name == "empty" { 4 } else { 7 },
        "native independent ID count incl discarded duplicates"
    );
    let rows = array_values(&a).expect("fixture array reads");
    let first = &rows[0];
    let real = name != "empty";
    let one = name == "oneway";
    let mode = if one {
        PositionMode::OneWay
    } else {
        PositionMode::Hedged
    };
    assert!(
        matches!(category::<safe_cex_types::PositionMode>(&(prop(first,"positionMode"))), m if m==mode),
        "native position mode enum"
    );
    assert!(
        matches!(
            category::<safe_cex_types::PositionSide>(&(prop(first, "side"))),
            PositionSide::Long
        ),
        "native first position side Long"
    );
    let symbol = if name == "proto-config" {
        "__proto__"
    } else {
        "BTC"
    };
    assert!(
        prop(first, "id").same(&string(if one {
            symbol.into()
        } else {
            format!("{symbol}:long")
        })),
        "native first ID retains real row"
    );
    if real {
        num(
            prop(first, "contracts"),
            3.6,
            "native first position contracts 3.6",
        );
        num(
            prop(first, "entryPrice"),
            if name == "id-mutates-row" { 4.2 } else { 12.3 },
            "native live entry price after ID helper",
        );
        num(prop(first, "notional"), 44.28, "native abs notional");
        num(
            prop(first, "unrealizedPnl"),
            1.2,
            "native first unrealized Pnl",
        );
        num(
            prop(first, "liquidationPrice"),
            9.1,
            "native first liquidation price",
        );
        assert!(
            prop(first, "info").same(&prop(&h.risk, "0")),
            "native original real row info Rc"
        );
        assert!(
            prop(first, "symbol").same(&string(if name == "id-mutates-row" {
                "ALT"
            } else {
                symbol
            })),
            "native live symbol after ID helper"
        );
        let lev = if h.kind == Kind::PM {
            if name == "zero-leverage" { 0.0 } else { 4.2 }
        } else {
            match name {
                "missing-config" | "zero-leverage" => 0.0,
                "duplicate-config" => 2.5,
                _ => 6.0,
            }
        };
        num(
            prop(first, "leverage"),
            lev,
            "native first leverage 6 vs 4.2",
        );
        let size = if h.kind == Kind::PM {
            if lev > 0.0 { Some(44.28 / lev) } else { None }
        } else if name == "isolated-alias" {
            Some(3.6)
        } else if name == "zero-leverage" {
            None
        } else {
            Some(2.5)
        };
        if let Some(x) = size {
            num(
                prop(first, "marginSize"),
                x,
                "native first margin size division",
            )
        } else {
            assert!(matches!(prop(first, "marginSize"), EventValue::Undefined));
            let EventValue::Object(o) = first else {
                unreachable!()
            };
            assert!(
                o.props.borrow().contains_key(
                    safe_cex_exchange::js_runtime::value::utf16("marginSize").as_ref()
                ),
                "native OWN undefined marginSize"
            );
        }
        let margin = if h.kind == Kind::PM || matches!(name, "missing-config" | "duplicate-config")
        {
            MarginType::Cross
        } else {
            MarginType::Isolated
        };
        assert!(
            matches!(category::<safe_cex_types::MarginType>(&(prop(first,"marginType"))), m if m==margin),
            "native real margin enum"
        );
    }
    let real_count = if name == "empty" {
        0
    } else if one {
        1
    } else {
        2
    };
    for (i, row) in rows.iter().enumerate() {
        let expected_symbol = if i < 2 && !one {
            symbol
        } else if one && i == 0 {
            symbol
        } else if name == "proto-config" {
            "leverage"
        } else {
            "ETH"
        };
        let expected_side = if one || i % 2 == 0 {
            PositionSide::Long
        } else {
            PositionSide::Short
        };
        let expected_id = if one {
            expected_symbol.to_owned()
        } else {
            format!(
                "{expected_symbol}:{}",
                if expected_side == PositionSide::Long {
                    "long"
                } else {
                    "short"
                }
            )
        };
        assert!(
            prop(row, "id").same(&string(expected_id)),
            "native independent row {i} ID/order {name}"
        );
        assert!(
            matches!(category::<safe_cex_types::PositionSide>(&(prop(row,"side"))), v if v==expected_side),
            "native EVERY row {i} typed side {name}"
        );
        assert!(
            matches!(category::<safe_cex_types::PositionMode>(&(prop(row,"positionMode"))), v if v==mode),
            "native EVERY row {i} typed mode {name}"
        );
        let expected_margin = if h.kind == Kind::PM
            || i >= 2
            || (one && i == 1)
            || matches!(name, "missing-config" | "duplicate-config")
            || (name == "id-mutates-config" && i == 1)
        {
            MarginType::Cross
        } else {
            MarginType::Isolated
        };
        assert!(
            matches!(category::<safe_cex_types::MarginType>(&(prop(row,"marginType"))), v if v==expected_margin),
            "native EVERY row {i} typed margin {name}"
        );
        if i < real_count {
            assert!(
                prop(row, "info").same(&prop(&h.risk, &i.to_string())),
                "native EACH real original info {i}"
            );
            if i == 1 {
                let lev = if h.kind == Kind::PM {
                    if name == "zero-leverage" { 0.0 } else { 10.0 }
                } else {
                    match name {
                        "missing-config" | "zero-leverage" => 0.0,
                        "duplicate-config" | "id-mutates-config" => 2.5,
                        _ => 6.0,
                    }
                };
                for (key, n) in [
                    ("contracts", 2.5),
                    ("entryPrice", 9.1),
                    ("notional", 22.75),
                    ("leverage", lev),
                    ("unrealizedPnl", -0.5),
                    ("liquidationPrice", 12.3),
                ] {
                    num(prop(row, key), n, "native second real decoded field");
                }
                if lev > 0.0 {
                    num(
                        prop(row, "marginSize"),
                        22.75 / lev,
                        "native second real margin",
                    );
                } else {
                    assert!(matches!(prop(row, "marginSize"), EventValue::Undefined));
                    let EventValue::Object(o) = row else {
                        unreachable!()
                    };
                    assert!(o.props.borrow().contains_key(
                        safe_cex_exchange::js_runtime::value::utf16("marginSize").as_ref()
                    ));
                }
            }
        } else {
            for key in [
                "contracts",
                "entryPrice",
                "notional",
                "unrealizedPnl",
                "liquidationPrice",
            ] {
                num(prop(row, key), 0.0, "native EACH synthetic zero field");
            }
            let lev = if h.kind == Kind::PM {
                10.0
            } else if expected_symbol == "BTC" {
                6.0
            } else if matches!(name, "missing-config" | "proto-config") {
                0.0
            } else {
                3.6
            };
            num(prop(row, "leverage"), lev, "native EACH synthetic leverage");
            let EventValue::Object(o) = row else {
                unreachable!()
            };
            if h.kind == Kind::PM {
                assert!(
                    !o.props.borrow().contains_key(
                        safe_cex_exchange::js_runtime::value::utf16("marginSize").as_ref()
                    ) && !o
                        .props
                        .borrow()
                        .contains_key(safe_cex_exchange::js_runtime::value::utf16("info").as_ref())
                );
            } else {
                num(prop(row, "marginSize"), 0.0, "native EACH synthetic margin");
                let info = prop(row, "info");
                assert!(prop(&info, "synthetic").same(&EventValue::Bool(true)));
                let config = prop(&info, "symbolConfig");
                if matches!(name, "missing-config" | "proto-config") {
                    assert!(matches!(config, EventValue::Undefined));
                } else {
                    assert!(
                        config.same(&prop(
                            &h.configs,
                            if expected_symbol == "BTC" { "0" } else { "1" }
                        )),
                        "native EACH synthetic config Rc"
                    );
                }
            }
        }
    }
    let last = rows.last().unwrap();
    num(prop(last, "contracts"), 0.0, "native synthetic contracts");
    num(prop(last, "entryPrice"), 0.0, "native synthetic entry");
    num(prop(last, "notional"), 0.0, "native synthetic notional");
    num(
        prop(last, "leverage"),
        if h.kind == Kind::PM {
            10.0
        } else if matches!(name, "missing-config" | "proto-config") {
            0.0
        } else {
            3.6
        },
        "native synthetic leverage",
    );
    let EventValue::Object(o) = last else {
        unreachable!()
    };
    if h.kind == Kind::PM {
        assert!(
            !o.props
                .borrow()
                .contains_key(safe_cex_exchange::js_runtime::value::utf16("info").as_ref())
                && !o.props.borrow().contains_key(
                    safe_cex_exchange::js_runtime::value::utf16("marginSize").as_ref()
                ),
            "native PM synthetic missing fields"
        );
    } else {
        num(
            prop(last, "marginSize"),
            0.0,
            "native Futures synthetic marginSize",
        );
        let info = prop(last, "info");
        assert!(prop(&info, "synthetic").same(&EventValue::Bool(true)));
        if matches!(name, "missing-config" | "proto-config") {
            assert!(
                matches!(prop(&info, "symbolConfig"), EventValue::Undefined),
                "native config fromEntries own __proto__ does not leak into leverage symbol"
            );
        } else {
            assert!(
                prop(&info, "symbolConfig").same(&prop(&h.configs, "1")),
                "native original synthetic config Rc"
            );
        }
    }
    assert_eq!(
        if let EventValue::Array(a) = &h.markets {
            a.len()
        } else {
            0
        },
        if name == "markets-append" { 3 } else { 2 },
        "native append affects owner but not snapshot loop"
    );
}
fn check_balance(h: &Host, v: &EventValue, name: &str) {
    let outer = h.rejecting()
        || matches!(
            name,
            "get-sync-first" | "log-capture-error" | "error-log-error" | "null"
        );
    assert_eq!(
        h.emits.borrow().len(),
        usize::from(outer),
        "native independent balance emission count"
    );
    let fallback_failed = matches!(name, "emit-error" | "fallback-error");
    if outer {
        assert_eq!(
            h.logged.get(),
            name == "log-capture-error",
            "native sticky flag on outer fallback"
        );
        assert_eq!(
            h.logs.borrow().len(),
            usize::from(name == "error-log-error"),
            "native partial log count before outer fallback"
        );
        if name == "error-log-error" {
            let logs = h.logs.borrow();
            assert_eq!(
                logs[0].1,
                LogSeverity::Error,
                "native outer partial Error severity"
            );
            assert_eq!(
                logs[0].0,
                "Error aggregating balances: TypeError: boolean true is not iterable (cannot read property Symbol(Symbol.iterator))",
                "native outer partial Error message"
            );
        }
        if !fallback_failed {
            assert!(
                prop(v, "balance").same(&h.balance),
                "native cached balance Rc"
            );
            assert!(
                prop(v, "positions").same(&h.cached),
                "native cached positions Rc"
            );
        }
        if let Some(m) = h.emits.borrow().first() {
            assert!(
                m.same(&string(if name == "null" {
                    "Cannot destructure property 'positions' of 'data' as it is null."
                } else {
                    "position venue detail"
                })),
                "native balance exact outer error"
            );
        }
        return;
    }
    let balance = prop(v, "balance");
    assert!(
        prop(v, "positions").same(&h.current.borrow()),
        "native current positions after log"
    );
    if name == "positions-mutated" {
        assert!(prop(v, "positions").same(&h.alternate));
    }
    let info = prop(&balance, "info");
    assert!(!info.same(&h.account), "native rest fresh object");
    assert!(
        prop(&info, "assets").same(&prop(&h.account, "assets")),
        "native shallow assets reference"
    );
    assert!(
        prop(&info, "nested").same(&prop(&h.account, "nested")),
        "native shallow nested reference"
    );
    let EventValue::Object(o) = &info else {
        unreachable!()
    };
    assert!(
        !o.props
            .borrow()
            .contains_key(safe_cex_exchange::js_runtime::value::utf16("positions").as_ref()),
        "native info omits compact positions"
    );
    let (t, f, u, p) = match name {
        "direct" => (
            12.3 + 3.6 * 2.125,
            4.2 + 1.2 * 2.125,
            2.5 + 0.5 * 2.125,
            1.2 - 0.2 * 2.125,
        ),
        "btc-route" => (
            12.3 + 3.6 * 10.5,
            4.2 + 1.2 * 10.5,
            2.5 + 0.5 * 10.5,
            1.2 - 0.2 * 10.5,
        ),
        "usdc-route" => (
            12.3 + 3.6 * 9.0,
            4.2 + 1.2 * 9.0,
            2.5 + 0.5 * 9.0,
            1.2 - 0.2 * 9.0,
        ),
        "usdt-zero" => (12.3, 4.2, 2.5, 1.2),
        "negative-stable" => (-3.6, 0.0, 0.0, 0.0),
        "negative-nonstable" | "zero" => (0.0, 0.0, 0.0, 0.0),
        "upnl-only" => (0.0, 0.0, 0.0, 3.6 * 4.2),
        "nan" => (f64::NAN, 4.2 + 1.2 * 4.2, 2.5 + 0.5 * 4.2, 1.2 - 0.2 * 4.2),
        "info-error" | "bad-assets" | "string-price" | "bool-price" => (9.1, 2.5, 3.6, 0.6),
        "error-log-mutates" => (12.3, 4.2, 3.6, 1.2),
        "ticker-appends" => (
            12.3 + 3.6 * 4.2 + 1.2,
            4.2 + 1.2 * 4.2 + 0.5,
            2.5 + 0.5 * 4.2 + 0.1,
            1.2 - 0.2 * 4.2 + 0.2,
        ),
        "market-mutates-asset" => (
            12.3 + 3.6 * 2.5,
            4.2 + 1.2 * 2.5,
            2.5 + 0.5 * 2.5,
            1.2 - 0.2 * 2.5,
        ),
        _ => (
            12.3 + 3.6 * 4.2,
            4.2 + 1.2 * 4.2,
            2.5 + 0.5 * 4.2,
            1.2 - 0.2 * 4.2,
        ),
    };
    for (key, x) in [("total", t), ("free", f), ("used", u), ("upnl", p)] {
        num(
            prop(&balance, key),
            x,
            &format!("native balance {name} {key}"),
        );
    }
    let info_log = !matches!(
        name,
        "logged"
            | "zero"
            | "negative-nonstable"
            | "error-log-mutates"
            | "bad-assets"
            | "string-price"
            | "bool-price"
    );
    let error_log = matches!(
        name,
        "info-error" | "error-log-mutates" | "bad-assets" | "string-price" | "bool-price"
    );
    assert_eq!(
        h.logs.borrow().len(),
        usize::from(info_log) + usize::from(error_log),
        "native independent balance logs"
    );
    assert_eq!(
        h.logged.get(),
        info_log || matches!(name, "logged" | "string-price" | "bool-price"),
        "native sticky flag before Info log"
    );
    if let Some((m, s)) = h.logs.borrow().first() {
        assert_eq!(
            *s,
            if info_log {
                LogSeverity::Info
            } else {
                LogSeverity::Error
            },
            "native log severity enum"
        );
        if matches!(name, "normal" | "info-error" | "positions-mutated") {
            assert_eq!(
                m,
                "Balance aggregation:\n  Stablecoins: USDT(12.3)\n  Non-stablecoins: BTC(3.6 @ $4.20 = $15.12)\n  Total: $27.42, Free: $9.24, Used: $4.60, UPnL: $0.36",
                "native exact balance log"
            );
        }
        if name == "direct" {
            assert!(
                m.contains("ABC(3.6 @ $2.13 = $7.65)"),
                "native toFixed tie rounds 2.125 to 2.13"
            );
        }
    }
    if error_log {
        let logs = h.logs.borrow();
        let (message, severity) = logs.last().unwrap();
        assert_eq!(*severity, LogSeverity::Error, "native Error log severity");
        assert_eq!(
            message,
            if name == "info-error" {
                "Error aggregating balances: RangeError: position snapshot fixture"
            } else if matches!(name, "string-price" | "bool-price") {
                "Error aggregating balances: TypeError: d.price?.toFixed is not a function"
            } else {
                "Error aggregating balances: TypeError: boolean true is not iterable (cannot read property Symbol(Symbol.iterator))"
            },
            "native exact aggregation Error string"
        );
    }
    if name == "ticker-appends" {
        let EventValue::Array(a) = prop(&h.account, "assets") else {
            unreachable!()
        };
        assert_eq!(a.len(), 3, "native live for-of consumes appended asset");
    }
    if name == "market-mutates-asset" {
        assert!(
            path(&h.account, &["assets", "1", "asset"]).same(&string("ETH")),
            "native live asset read inside find predicate"
        );
    }
}
fn norm(v: &Value) -> Value {
    match v {
        Value::Number(n) => json!(n.as_f64().unwrap()),
        Value::Array(a) => a.iter().map(norm).collect(),
        Value::Object(o) => o.iter().map(|(k, v)| (k.clone(), norm(v))).collect(),
        x => x.clone(),
    }
}
fn compare(k: Kind) {
    let family = if k == Kind::Balance {
        "balance"
    } else {
        "positions"
    };
    let values: Vec<_> = cases(family).iter().map(|c| native(k, c)).collect();
    let floor = if k == Kind::Balance { 26 } else { 24 };
    assert_eq!(values.len(), floor, "independent native floor");
    let out = Command::new("node")
        .arg(format!(
            "{}/scripts/position-snapshots.mjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .arg(if k == Kind::PM { "pm" } else { "futures" })
        .arg(family)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "source oracle failed {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let source: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        source.as_array().unwrap().len(),
        floor,
        "independent source floor"
    );
    for (i, (a, b)) in values.iter().zip(source.as_array().unwrap()).enumerate() {
        assert_eq!(norm(a), norm(b), "actual source {k:?} {family} case{i}")
    }
}
#[test]
fn futures_positions() {
    compare(Kind::Futures)
}
#[test]
fn pm_positions() {
    compare(Kind::PM)
}
#[test]
fn futures_balance() {
    compare(Kind::Balance)
}

fn observed_error_code(
    code: &Option<safe_cex_exchange::js_runtime::value::Js>,
) -> Option<serde_json::Value> {
    code.as_ref().map(|value| {
        let finite =
            safe_cex_exchange::js_runtime::value::to_json(value).expect("finite JSON code fixture");
        let text = safe_cex_num::js_json_stringify(&finite.into())
            .expect("source JSON code fixture")
            .expect("defined code fixture");
        serde_json::from_str(&text).expect("JSON code observation")
    })
}

#[path = "support/native_array_fixture.rs"]
#[allow(dead_code)] // Different integration binaries consume distinct fixture operations.
mod native_array_fixture;
