#![cfg(not(target_arch = "wasm32"))]
use indexmap::IndexMap;
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::{Candle, OrderBook, Position};
use safe_cex_venue_grvt::public_feeds::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::OnceLock,
};
#[derive(Clone)]
enum Data {
    MapFunction(&'static str),
    Number(f64),
    AccessorLevels,
    Market(MarketRef),
    Undefined,
    Primitive(Value),
    Object(Rc<RefCell<IndexMap<String, Data>>>),
    Array(Rc<RefCell<Vec<Option<Data>>>>),
}
impl Data {
    fn from(v: Value) -> Self {
        match v {
            Value::Object(o) => Self::Object(Rc::new(RefCell::new(
                o.into_iter().map(|(k, v)| (k, Self::from(v))).collect(),
            ))),
            Value::Array(a) => Self::Array(Rc::new(RefCell::new(
                a.into_iter().map(|v| Some(Self::from(v))).collect(),
            ))),
            v => Self::Primitive(v),
        }
    }
    fn plain(&self) -> Option<Value> {
        match self {
            Self::Number(n) => Some(json!(n)),
            Self::AccessorLevels => None,
            Self::MapFunction(name) => {
                assert!(!name.is_empty(), "named map function");
                None
            }
            Self::Market(m) => Some(json!({"id":m.borrow().id,"symbol":m.borrow().symbol})),
            Self::Undefined => None,
            Self::Primitive(v) => Some(v.clone()),
            Self::Object(o) => Some(Value::Object(
                o.borrow()
                    .iter()
                    .filter_map(|(k, v)| v.plain().map(|v| (k.clone(), v)))
                    .collect(),
            )),
            Self::Array(a) => Some(Value::Array(
                a.borrow()
                    .iter()
                    .map(|v| v.as_ref().and_then(Self::plain).unwrap_or(Value::Null))
                    .collect(),
            )),
        }
    }
    fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Object(a), Self::Object(b)) => Rc::ptr_eq(a, b),
            (Self::Array(a), Self::Array(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
    fn set(&self, key: &str, value: Data) {
        let Self::Object(o) = self else {
            panic!("object")
        };
        o.borrow_mut().insert(key.into(), value);
    }
    fn get(&self, key: &str) -> Data {
        match self {
            Self::Array(a) => key
                .parse::<usize>()
                .ok()
                .and_then(|i| a.borrow().get(i).cloned().flatten())
                .unwrap_or(Self::Undefined),
            Self::Market(m) => match key {
                "id" => Self::from(json!(m.borrow().id)),
                "symbol" => Self::from(json!(m.borrow().symbol)),
                _ => Self::Undefined,
            },
            Self::Object(o) => o.borrow().get(key).cloned().unwrap_or(Self::Undefined),
            _ => Self::Undefined,
        }
    }
}
fn corpus() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        let out = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/public-feeds-source.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "FEEDS actual source {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    })
}
fn scalar(a: f64, b: &Value, label: &str) {
    if b["nan"] == true {
        assert!(a.is_nan(), "{label} native NaN");
    } else {
        assert_eq!(
            format!("{:016x}", a.to_bits()),
            b["bits"].as_str().unwrap(),
            "{label}"
        );
    }
}
fn error(a: Option<&ExchangeError>, b: &Value, label: &str) {
    match a {
        None => assert!(b.is_null(), "{label} unexpected success"),
        Some(a) => {
            assert_eq!(
                a.name,
                b["name"].as_str().unwrap(),
                "{label} native error name"
            );
            assert_eq!(
                a.message,
                b["message"].as_str().unwrap(),
                "{label} native error message"
            );
        }
    }
}
fn event_counts(h: &Host, row: &Value, count: usize, label: &str) {
    assert_eq!(
        h.events.borrow().len(),
        count,
        "{label} independent event count"
    );
    assert_eq!(
        row["events"].as_array().unwrap().len(),
        count,
        "{label} independent source event count"
    );
    assert_eq!(
        *h.events.borrow(),
        row["events"].as_array().unwrap().clone(),
        "{label} native effects"
    );
}
fn kind(k: FeedKind) -> &'static str {
    match k {
        FeedKind::Book => "book",
        FeedKind::Candle => "candle",
        FeedKind::Ticker => "ticker",
    }
}
struct Host {
    name: String,
    registry: FeedRegistry<Data>,
    events: Rc<RefCell<Vec<Value>>>,
    updates: RefCell<Vec<TickerChanges<Data>>>,
    dispatches: RefCell<Vec<FeedKind>>,
    candles: RefCell<Vec<Candle>>,
    books: RefCell<Vec<OrderBook>>,
    timers: RefCell<Vec<FeedCleanup>>,
    market: MarketRef,
    ticker: Data,
    connected: Cell<bool>,
    connection_reads: Cell<u32>,
    error_reads: Cell<u32>,
    last_reads: Cell<u32>,
    symbol_reads: Cell<u32>,
    size_reads: Cell<u32>,
    positions_data: Rc<RefCell<Vec<Option<Rc<RefCell<Position>>>>>>,
    expected_feed: RefCell<Option<Data>>,
    first: RefCell<Option<Data>>,
    bids: RefCell<Option<Data>>,
    current_topic: RefCell<String>,
    retained: RefCell<Option<LevelProjector<Data>>>,
    index_comparisons: Cell<usize>,
    output_reads: Cell<usize>,
    asks_replaced: Cell<bool>,
    alias_result: RefCell<Option<Data>>,
}
impl Host {
    fn new(name: &str) -> Rc<Self> {
        let market = serde_json::from_value(json!({"id":"BTC","symbol":"BTC/USDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":1,"price":1},"limits":{"amount":{"min":1,"max":9},"leverage":{"min":1,"max":9}}})).unwrap();
        let positions = if [
            "sync-error",
            "market-update-mutation",
            "position-contract-mutation",
        ]
        .contains(&name)
        {
            let position = serde_json::from_value(json!({"id":"p","symbol":"BTC/USDT","side":"long","entryPrice":7,"notional":8,"leverage":7,"unrealizedPnl":0,"contracts":if name=="position-contract-mutation"{0}else{-2},"liquidationPrice":1})).unwrap();
            vec![Some(Rc::new(RefCell::new(position)))]
        } else {
            vec![]
        };
        Rc::new(Self {
            name: name.into(),
            registry: FeedRegistry::default(),
            events: Rc::new(RefCell::new(vec![])),
            updates: RefCell::new(vec![]),
            dispatches: RefCell::new(vec![]),
            candles: RefCell::new(vec![]),
            books: RefCell::new(vec![]),
            timers: RefCell::new(vec![]),
            market: Rc::new(RefCell::new(market)),
            ticker: Data::from(
                json!({"last":if ["mark-fallback","changing-last","market-ticker-mutation"].contains(&name){0}else{5}}),
            ),
            connected: Cell::new(name != "disconnected"),
            connection_reads: Cell::new(0),
            error_reads: Cell::new(0),
            last_reads: Cell::new(0),
            symbol_reads: Cell::new(0),
            size_reads: Cell::new(0),
            positions_data: Rc::new(RefCell::new(positions)),
            expected_feed: RefCell::new(None),
            first: RefCell::new(None),
            bids: RefCell::new(None),
            current_topic: RefCell::new(String::new()),
            retained: RefCell::new(None),
            index_comparisons: Cell::new(0),
            output_reads: Cell::new(0),
            asks_replaced: Cell::new(false),
            alias_result: RefCell::new(None),
        })
    }
    fn log(&self, value: Value) {
        self.events.borrow_mut().push(value);
    }
    fn fail(&self, name: &str) -> Result<(), ExchangeError> {
        if self.name == format!("{name}-error") {
            Err(ExchangeError::new(format!("{name}-fail")))
        } else {
            Ok(())
        }
    }
}
impl PublicFeedHost for Host {
    type Value = Data;
    fn property(&self, v: &Data, key: &str) -> Result<Data, ExchangeError> {
        if matches!(v, Data::AccessorLevels) && key == "price" {
            self.output_reads.set(self.output_reads.get() + 1);
            return Ok(Data::Number(12.0));
        }
        if key == "map" && matches!(v, Data::Array(_)) {
            let selected = self.bids.borrow().as_ref().is_some_and(|b| b.same(v));
            if self.name.starts_with("map-") {
                self.log(json!(if selected {
                    "get-bids"
                } else if self.asks_replaced.get() {
                    "get-asks-changed"
                } else {
                    "get-asks"
                }));
            }
            if selected && self.name == "map-getter-throw" {
                return Err(ExchangeError::new("map-fail"));
            }
            if selected && self.name == "map-noncallable" {
                return Ok(Data::Number(0.0));
            }
            return Ok(Data::MapFunction(if selected {
                match self.name.as_str() {
                    "map-empty" => "empty",
                    "map-throw" => "throw",
                    "map-retained" => "retained",
                    "map-arbitrary" => "arbitrary",
                    "map-accessor-output" => "accessor-output",
                    "map-result-alias" => "result-alias",
                    "map-replace-asks" => "replace-asks",
                    _ => "ordinary",
                }
            } else if self.asks_replaced.get() {
                "changed"
            } else {
                "ordinary"
            }));
        }
        if self.nullish(v) {
            let mut error = ExchangeError::new(format!(
                "Cannot read properties of {} (reading '{key}')",
                if self.undefined(v) {
                    "undefined"
                } else {
                    "null"
                }
            ));
            error.name = "TypeError".into();
            return Err(error);
        }
        if self.name == "position-contract-mutation"
            && key == "symbol"
            && matches!(v, Data::Market(_))
        {
            let n = self.symbol_reads.get() + 1;
            self.symbol_reads.set(n);
            if n == 2 {
                self.positions_data.borrow()[0]
                    .as_ref()
                    .unwrap()
                    .borrow_mut()
                    .contracts = -2.0;
            }
        }
        if self.name == "changing-error" && key == "error" {
            let n = self.error_reads.get() + 1;
            self.error_reads.set(n);
            return Ok(Data::from(json!({"code":if n==1{1100}else{42}})));
        }
        if self.name == "changing-last" && key == "last" && v.same(&self.ticker) {
            let n = self.last_reads.get() + 1;
            self.last_reads.set(n);
            return Ok(Data::from(json!(if n == 1 { 1 } else { 0 })));
        }
        if self.name == "prior-fallback"
            && key == "s"
            && self
                .first
                .borrow()
                .as_ref()
                .is_some_and(|first| v.same(first))
        {
            let n = self.size_reads.get() + 1;
            self.size_reads.set(n);
            if n == 2 {
                if let Some(Data::Array(a)) = self.bids.borrow().as_ref() {
                    a.borrow_mut()[0] = Some(Data::from(json!({"p":"12","size":"8"})));
                }
                return Ok(Data::from(json!(0)));
            }
        }
        if key == "p"
            && self
                .first
                .borrow()
                .as_ref()
                .is_some_and(|first| v.same(first))
        {
            if let Some(Data::Array(a)) = self.bids.borrow().as_ref() {
                match self.name.as_str() {
                    "live-replace" => {
                        a.borrow_mut()[1] = Some(Data::from(json!({"p":"21","s":"9"})))
                    }
                    "prior-replace" => {
                        a.borrow_mut()[0] = Some(Data::from(json!({"p":"12","s":"8"})))
                    }
                    "trailing-hole" => a.borrow_mut()[2] = None,
                    _ => {}
                }
            }
        }
        Ok(v.get(key))
    }
    fn truthy(&self, v: &Data) -> bool {
        match v {
            Data::Number(n) => *n != 0.0 && !n.is_nan(),
            Data::MapFunction(_) | Data::AccessorLevels => true,
            _ => safe_cex_num::js_truthy(v.plain().as_ref()),
        }
    }
    fn undefined(&self, v: &Data) -> bool {
        matches!(v, Data::Undefined)
    }
    fn nullish(&self, v: &Data) -> bool {
        matches!(v, Data::Undefined | Data::Primitive(Value::Null))
    }
    fn string_type(&self, v: &Data) -> bool {
        matches!(v, Data::Primitive(Value::String(_)))
    }
    fn number_type(&self, v: &Data) -> bool {
        matches!(v, Data::Number(_) | Data::Primitive(Value::Number(_)))
    }
    fn string(&self, v: &Data) -> Result<String, ExchangeError> {
        if let Data::Number(n) = v {
            return Ok(safe_cex_num::js_number_string(*n));
        }
        safe_cex_num::js_string(v.plain().as_ref()).map_err(|e| {
            let mut error = ExchangeError::new(e.message);
            error.name = e.name.into();
            error
        })
    }
    fn number(&self, v: &Data) -> Result<f64, ExchangeError> {
        if let Data::Number(n) = v {
            return Ok(*n);
        }
        safe_cex_num::js_number(v.plain().as_ref()).map_err(|e| {
            let mut error = ExchangeError::new(e.message);
            error.name = e.name.into();
            error
        })
    }
    fn float(&self, v: &Data) -> Result<f64, ExchangeError> {
        Ok(safe_cex_num::parse_float(&self.string(v)?))
    }
    fn integer(&self, v: &Data) -> Result<f64, ExchangeError> {
        Ok(safe_cex_num::parse_int(&self.string(v)?, None))
    }
    fn string_value(&self, v: &str) -> Data {
        Data::from(json!(v))
    }
    fn split(&self, v: &Data, sep: &str) -> Result<Vec<String>, ExchangeError> {
        if !self.string_type(v) {
            let mut error = ExchangeError::new("selector.split is not a function");
            error.name = "TypeError".into();
            return Err(error);
        }
        Ok(self.string(v)?.split(sep).map(str::to_owned).collect())
    }
    fn call_map(
        &self,
        method: Data,
        receiver: Data,
        projector: LevelProjector<Data>,
    ) -> Result<Data, ExchangeError> {
        let Data::MapFunction(mode) = method else {
            let mut e = ExchangeError::new("rawBids.map is not a function");
            e.name = "TypeError".into();
            return Err(e);
        };
        let selected = self
            .bids
            .borrow()
            .as_ref()
            .is_some_and(|b| b.same(&receiver));
        if self.name.starts_with("map-") {
            self.log(json!(if selected {
                "call-bids"
            } else if mode == "changed" {
                "call-asks-changed"
            } else {
                "call-asks"
            }));
        }
        match mode {
            "empty" | "changed" => return Ok(self.empty_array()),
            "throw" => return Err(ExchangeError::new("map-fail")),
            "retained" => {
                *self.retained.borrow_mut() = Some(projector);
                return Ok(self.empty_array());
            }
            "arbitrary" => return Ok(Data::Number(42.0)),
            "accessor-output" => {
                return Ok(Data::Array(Rc::new(RefCell::new(vec![Some(
                    Data::AccessorLevels,
                )]))));
            }
            "result-alias" => {
                let result = Data::Array(Rc::new(RefCell::new(vec![Some(
                    self.level_value(12.0, 2.0, 2.0),
                )])));
                *self.alias_result.borrow_mut() = Some(result.clone());
                return Ok(result);
            }
            "replace-asks" => {
                self.asks_replaced.set(true);
                return Ok(self.empty_array());
            }
            "ordinary" => (),
            other => panic!("unclassified map {other}"),
        }
        let Data::Array(array) = &receiver else {
            panic!("map receiver")
        };
        let length = array.borrow().len();
        let mut mapped = Vec::with_capacity(length);
        for index in 0..length {
            let level = array.borrow().get(index).cloned().flatten();
            mapped.push(match level {
                Some(level) => Some(projector(
                    level,
                    Data::Number(index as f64),
                    receiver.clone(),
                )?),
                None => None,
            });
        }
        Ok(Data::Array(Rc::new(RefCell::new(mapped))))
    }
    fn index_less_than(&self, index: f64, limit: &Data) -> Result<bool, ExchangeError> {
        if let Data::Object(o) = limit
            && o.borrow().contains_key("index-object")
        {
            self.index_comparisons.set(self.index_comparisons.get() + 1);
            return Ok(index < 1.0);
        }
        Ok(index < self.number(limit)?)
    }
    fn level_value(&self, price: f64, amount: f64, total: f64) -> Data {
        Data::Object(Rc::new(RefCell::new(IndexMap::from([
            ("price".into(), Data::Number(price)),
            ("amount".into(), Data::Number(amount)),
            ("total".into(), Data::Number(total)),
        ]))))
    }
    fn book_levels(
        &self,
        value: Data,
    ) -> Result<Vec<safe_cex_types::OrderBookOrders>, ExchangeError> {
        let representation = || {
            let mut e =
                ExchangeError::new("Unsupported GRVT orderbook map result for shared OrderBook");
            e.name = "RepresentationError".into();
            e
        };
        let Data::Array(array) = value else {
            return Err(representation());
        };
        let mut levels = Vec::new();
        for entry in array.borrow().iter() {
            let Some(entry) = entry else {
                return Err(ExchangeError::new(
                    "Sparse GRVT orderbook levels cannot be represented by shared OrderBook",
                ));
            };
            let Data::Object(fields) = entry else {
                return Err(representation());
            };
            let fields = fields.borrow();
            let number = |key: &str| match fields.get(key) {
                Some(Data::Number(n)) => Ok(*n),
                _ => Err(representation()),
            };
            levels.push(safe_cex_types::OrderBookOrders {
                price: number("price")?,
                amount: number("amount")?,
                total: number("total")?,
            });
        }
        Ok(levels)
    }
    fn empty_array(&self) -> Data {
        Data::from(json!([]))
    }
    fn json_parse(&self, data: &str) -> Result<Option<Data>, ExchangeError> {
        Ok(serde_json::from_str(data).ok().map(Data::from))
    }
    fn disposed(&self) -> bool {
        self.name == "disposed"
    }
    fn connected(&self) -> bool {
        let n = self.connection_reads.get() + 1;
        self.connection_reads.set(n);
        if self.name == "connected-handler" && n == 2 {
            let topic = self.current_topic.borrow().clone();
            let array = self
                .registry
                .handlers
                .borrow()
                .get(&topic)
                .cloned()
                .unwrap();
            array
                .borrow_mut()
                .push(HandlerSlot::Callback(Rc::new(|_| Ok(()))));
        }
        self.connected.get()
    }
    fn track_received(&self) -> Result<(), ExchangeError> {
        self.log(json!(["track"]));
        self.fail("track")
    }
    fn dispatch(&self, k: FeedKind, message: Data) -> Result<(), ExchangeError> {
        self.dispatches.borrow_mut().push(k);
        self.log(json!(["dispatch", kind(k)]));
        self.fail("dispatch")?;
        if self.name == "hole-spread" {
            handle_kline_update(self, message)
        } else {
            Ok(())
        }
    }
    fn emit_error(&self, text: &str) -> Result<(), ExchangeError> {
        self.log(json!(["emit", text]));
        self.fail("emit")
    }
    fn registry(&self) -> &FeedRegistry<Data> {
        &self.registry
    }
    fn record_stats(&self, topic: &str) -> Result<(), ExchangeError> {
        self.log(json!(["stats", topic]));
        self.fail("stats")?;
        let array = self.registry.handlers.borrow().get(topic).cloned().unwrap();
        if self.name == "stats-append" {
            let events = self.events.clone();
            let expected = self.expected_feed.borrow().clone().unwrap();
            array
                .borrow_mut()
                .push(HandlerSlot::Callback(Rc::new(move |feed| {
                    events.borrow_mut().push(json!([
                        "handler",
                        2,
                        feed.same(&expected),
                        feed.get("i")
                            .plain()
                            .unwrap_or_else(|| feed.get("instrument").plain().unwrap())
                    ]));
                    Ok(())
                })));
        }
        if self.name == "replace-array" {
            self.registry
                .handlers
                .borrow_mut()
                .insert(topic.into(), Rc::new(RefCell::new(vec![])));
        }
        Ok(())
    }
    fn cleanup_stats(&self, topic: &str) -> Result<(), ExchangeError> {
        self.log(json!(["cleanup", topic]));
        Ok(())
    }
    fn market_by_instrument(&self, instrument: &Data) -> Result<Option<Data>, ExchangeError> {
        assert_eq!(
            self.string(instrument)?,
            if self.name == "constructor" {
                "constructor"
            } else {
                "BTC"
            },
            "TICKER native instrument lookup key"
        );
        Ok(if self.name == "no-market" {
            None
        } else if self.name == "false-market" {
            Some(Data::from(json!(false)))
        } else if self.name == "constructor" {
            Some(Data::from(json!({})))
        } else {
            Some(Data::Market(self.market.clone()))
        })
    }
    fn market_by_symbol(&self, symbol: &str) -> Result<Option<MarketRef>, ExchangeError> {
        assert_eq!(symbol, "BTC/USDT", "LISTENER native market lookup symbol");
        Ok(if self.name == "no-market" {
            None
        } else {
            Some(self.market.clone())
        })
    }
    fn current_ticker(&self, id: &Data) -> Result<Option<Data>, ExchangeError> {
        if self.name == "constructor" {
            assert!(self.undefined(id), "TICKER native inherited market id");
            return Ok(None);
        }
        assert_eq!(self.string(id)?, "BTC", "TICKER native fallback lookup key");
        if self.name == "null-ticker" {
            return Ok(Some(Data::from(Value::Null)));
        }
        if self.name == "market-ticker-mutation" {
            self.market.borrow_mut().id = "CHANGED".into();
        }
        Ok(Some(self.ticker.clone()))
    }
    fn positions(&self) -> Result<Rc<RefCell<Vec<Option<Rc<RefCell<Position>>>>>>, ExchangeError> {
        Ok(self.positions_data.clone())
    }
    fn update_ticker(&self, id: &Data, changes: &TickerChanges<Data>) -> Result<(), ExchangeError> {
        if self.name == "constructor" {
            assert!(
                self.undefined(id),
                "TICKER native update selector undefined"
            );
        } else {
            assert_eq!(
                self.string(id)?,
                if ["market-callback-mutation", "market-ticker-mutation"]
                    .contains(&self.name.as_str())
                {
                    "CHANGED"
                } else {
                    "BTC"
                },
                "TICKER native update selector"
            );
        }
        self.log(json!([
            "update",
            if self.undefined(id) {
                json!({"undefined":true})
            } else {
                id.plain().unwrap()
            }
        ]));
        self.updates.borrow_mut().push(changes.clone());
        self.fail("update")?;
        if self.name == "market-update-mutation" {
            self.market.borrow_mut().symbol = "CHANGED".into();
        }
        Ok(())
    }
    fn sync_upnl(&self) -> Result<(), ExchangeError> {
        self.log(json!(["sync"]));
        self.fail("sync")
    }
    fn map_interval(&self, interval: &str) -> Result<String, ExchangeError> {
        self.log(json!(["map", interval]));
        Ok("CI_1_M".into())
    }
    fn subscribe(
        &self,
        k: FeedKind,
        id: &str,
        interval: Option<&str>,
    ) -> Result<f64, ExchangeError> {
        self.log(if let Some(interval) = interval {
            json!(["subscribe", kind(k), id, interval])
        } else {
            json!(["subscribe", kind(k), id])
        });
        Ok(17.0)
    }
    fn unsubscribe(&self, k: FeedKind, id: f64) -> Result<(), ExchangeError> {
        self.log(json!(["unsubscribe", kind(k), id as u64]));
        Ok(())
    }
    fn schedule(&self, callback: FeedCleanup, delay: f64) -> Result<(), ExchangeError> {
        assert_eq!(delay, 100.0, "LISTENER native retry delay");
        self.log(json!(["schedule", delay as u64]));
        self.timers.borrow_mut().push(callback);
        Ok(())
    }
}
fn assert_updates(h: &Host, row: &Value, count: usize) {
    let updates = h.updates.borrow();
    assert_eq!(updates.len(), count, "TICKER independent native count");
    assert_eq!(
        row["updates"].as_array().unwrap().len(),
        count,
        "TICKER independent source count"
    );
    for (a, b) in updates.iter().zip(row["updates"].as_array().unwrap()) {
        let symbol = a.symbol.as_ref().expect("TICKER own native symbol slot");
        if b["symbol"]["undefined"] == true {
            assert!(h.undefined(symbol), "TICKER native own undefined symbol");
        } else {
            assert_eq!(
                h.string(symbol).unwrap(),
                b["symbol"].as_str().unwrap(),
                "TICKER native symbol"
            );
        }
        for (value, name) in [
            (a.mark, "mark"),
            (a.index, "index"),
            (a.last, "last"),
            (a.bid, "bid"),
            (a.ask, "ask"),
            (a.funding_rate, "fundingRate"),
            (a.open_interest, "openInterest"),
            (a.volume, "volume"),
            (a.quote_volume, "quoteVolume"),
            (a.percentage, "percentage"),
            (a.funding_time, "fundingTime"),
        ] {
            match value {
                Some(value) => {
                    assert!(b.get(name).is_some(), "TICKER native field presence {name}");
                    scalar(value, &b[name], &format!("TICKER native {name}"));
                }
                None => assert!(b.get(name).is_none(), "TICKER native absent {name}"),
            }
        }
        assert!(
            a.info
                .as_ref()
                .unwrap()
                .same(h.expected_feed.borrow().as_ref().unwrap()),
            "TICKER native info identity"
        );
    }
}
#[test]
fn message_native() {
    let names = [
        "disposed",
        "invalid",
        "null",
        "confirmation",
        "zero-id",
        "unsupported",
        "unknown-error",
        "changing-error",
        "book",
        "ticker",
        "dispatch-error",
        "track-error",
    ];
    let counts = [0, 1, 1, 1, 2, 1, 2, 2, 2, 2, 3, 1];
    let rows = corpus()["messageRows"].as_array().unwrap();
    assert_eq!(rows.len(), 12, "MESSAGE fixed floor");
    for (i, row) in rows.iter().enumerate() {
        assert_eq!(row["name"], names[i]);
        let h = Host::new(names[i]);
        let result = on_message(h.as_ref(), row["data"].as_str().unwrap());
        error(result.as_ref().err(), &row["error"], "MESSAGE");
        let expected: Vec<FeedKind> = match i {
            4 | 10 => vec![FeedKind::Candle],
            8 => vec![FeedKind::Book],
            9 => vec![FeedKind::Ticker],
            _ => vec![],
        };
        assert_eq!(*h.dispatches.borrow(), expected, "MESSAGE native route");
        event_counts(&h, row, counts[i], "MESSAGE");
    }
}
fn handler_matrix(k: FeedKind) {
    let rows = corpus()[if k == FeedKind::Candle {
        "klineRows"
    } else {
        "bookRows"
    }]
    .as_array()
    .unwrap();
    let names = [
        "lite",
        "full",
        "no-feed",
        "no-instrument",
        "bad-selector",
        "stats-error",
        "callback-error",
        "array-mutation",
        "stats-append",
        "replace-array",
        "feed-mutation",
    ];
    let counts = if k == FeedKind::Candle {
        [3, 3, 0, 0, 0, 2, 3, 2, 4, 3, 3]
    } else {
        [3, 3, 0, 0, 3, 2, 3, 2, 4, 3, 3]
    };
    assert_eq!(rows.len(), 11, "HANDLER fixed floor");
    for (i, row) in rows.iter().enumerate() {
        assert_eq!(row["name"], names[i]);
        let h = Host::new(names[i]);
        let input = Data::from(row["input"].clone());
        let feed = if names[i] == "full" {
            input.get("feed")
        } else {
            input.get("f")
        };
        *h.expected_feed.borrow_mut() = Some(feed.clone());
        let topic = if k == FeedKind::Candle {
            "kline.CI_1_M.BTC"
        } else {
            "orderbook.BTC"
        };
        let array: HandlerArray<Data> = Rc::new(RefCell::new(vec![]));
        for n in 0..2 {
            let host = h.clone();
            let expected = feed.clone();
            let captured = array.clone();
            let parser: FeedHandler<Data> = Rc::new(move |value| {
                assert!(value.same(&expected), "HANDLER native feed identity");
                let instrument = value.get("i");
                let instrument = if host.truthy(&instrument) {
                    instrument
                } else {
                    value.get("instrument")
                };
                host.log(json!(["handler", n, true, instrument.plain().unwrap()]));
                if n == 0 {
                    host.fail("callback")?;
                    if host.name == "array-mutation" {
                        captured.borrow_mut()[1] = HandlerSlot::Hole;
                        let host2 = host.clone();
                        captured
                            .borrow_mut()
                            .push(HandlerSlot::Callback(Rc::new(move |_| {
                                host2.log(json!(["unexpected-new-handler"]));
                                Ok(())
                            })));
                    }
                    if host.name == "feed-mutation" {
                        value.set("i", Data::from(json!("CHANGED")));
                    }
                }
                Ok(())
            });
            array.borrow_mut().push(HandlerSlot::Callback(parser));
        }
        h.registry.handlers.borrow_mut().insert(topic.into(), array);
        let result = if k == FeedKind::Candle {
            handle_kline_update(h.as_ref(), input)
        } else {
            handle_orderbook_update(h.as_ref(), input)
        };
        error(result.as_ref().err(), &row["error"], "HANDLER");
        event_counts(&h, row, counts[i], "HANDLER");
    }
}
#[test]
fn kline_native() {
    handler_matrix(FeedKind::Candle)
}
#[test]
fn book_native() {
    handler_matrix(FeedKind::Book)
}
#[test]
fn ticker_native() {
    let rows = corpus()["tickerRows"].as_array().unwrap();
    let names = [
        "lite",
        "full",
        "zero-lite",
        "null-lite",
        "partial-volume",
        "mark-fallback",
        "mark-existing",
        "changing-last",
        "market-ticker-mutation",
        "no-market",
        "invalid",
        "numeric-funding",
        "update-error",
        "sync-error",
        "market-update-mutation",
        "constructor",
        "position-contract-mutation",
        "false-market",
        "null-ticker",
    ];
    let counts = [1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 2, 3, 1, 1, 2, 0, 1];
    assert_eq!(rows.len(), 19, "TICKER fixed floor");
    for (i, row) in rows.iter().enumerate() {
        assert_eq!(row["name"], names[i]);
        let h = Host::new(names[i]);
        let input = Data::from(row["input"].clone());
        *h.expected_feed.borrow_mut() = Some(input.get("f"));
        let result = handle_ticker_update(h.as_ref(), input);
        error(result.as_ref().err(), &row["error"], "TICKER");
        assert_updates(&h, row, if i == 9 || i == 17 { 0 } else { 1 });
        assert_eq!(
            row["infoIdentity"].as_array().unwrap().len(),
            if i == 9 || i == 17 { 0 } else { 1 },
            "TICKER independent source identity count"
        );
        assert!(
            row["infoIdentity"]
                .as_array()
                .unwrap()
                .iter()
                .all(|v| v == true),
            "TICKER source info identity"
        );
        event_counts(&h, row, counts[i], "TICKER");
    }
}
fn candle_values(h: &Host, row: &Value, count: usize) {
    let candles = h.candles.borrow();
    assert_eq!(candles.len(), count, "CANDLE independent callback count");
    assert_eq!(
        row["candles"].as_array().unwrap().len(),
        count,
        "CANDLE source callback count"
    );
    for (a, b) in candles.iter().zip(row["candles"].as_array().unwrap()) {
        for (value, key) in [
            (a.timestamp, "timestamp"),
            (a.open, "open"),
            (a.high, "high"),
            (a.low, "low"),
            (a.close, "close"),
            (a.volume, "volume"),
        ] {
            scalar(value, &b[key], &format!("CANDLE native {key}"));
        }
        assert_eq!(a.last_side, None, "CANDLE native optional last side");
    }
}
fn book_values(h: &Host, row: &Value, bid_count: usize, ask_count: usize, count: usize) {
    let books = h.books.borrow();
    assert_eq!(books.len(), count, "BOOK independent callback count");
    assert_eq!(
        row["books"].as_array().unwrap().len(),
        count,
        "BOOK source callback count"
    );
    for (a, b) in books.iter().zip(row["books"].as_array().unwrap()) {
        for (levels, key, expected) in [(&a.bids, "bids", bid_count), (&a.asks, "asks", ask_count)]
        {
            assert_eq!(
                levels.len(),
                expected,
                "BOOK independent native level count"
            );
            assert_eq!(
                b[key].as_array().unwrap().len(),
                expected,
                "BOOK independent source level count"
            );
            for (level, source) in levels.iter().zip(b[key].as_array().unwrap()) {
                scalar(level.price, &source["price"], "BOOK native price");
                scalar(level.amount, &source["amount"], "BOOK native amount");
                scalar(level.total, &source["total"], "BOOK native total");
            }
        }
    }
}
fn listeners_matrix(k: FeedKind) {
    let rows = corpus()[if k == FeedKind::Candle {
        "candleListenerRows"
    } else {
        "bookListenerRows"
    }]
    .as_array()
    .unwrap();
    let names = if k == FeedKind::Candle {
        vec![
            "lite",
            "full",
            "numeric-time",
            "zero-fallback",
            "invalid",
            "wrong-instrument",
            "no-market",
            "disconnected",
            "callback-error",
            "market-callback-mutation",
            "connected-handler",
            "hole-spread",
        ]
    } else {
        vec![
            "lite",
            "full",
            "empty",
            "zero-fallback",
            "rounding-order",
            "live-replace",
            "prior-replace",
            "wrong-instrument",
            "no-market",
            "disconnected",
            "callback-error",
            "trailing-hole",
            "prior-fallback",
        ]
    };
    let events = if k == FeedKind::Candle {
        vec![6, 6, 6, 6, 5, 4, 1, 7, 3, 6, 4, 4]
    } else {
        vec![4, 4, 4, 4, 4, 4, 4, 3, 1, 5, 2, 4, 4]
    };
    let callbacks = if k == FeedKind::Candle {
        vec![1, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0]
    } else {
        vec![1, 1, 1, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1]
    };
    assert_eq!(
        rows.len(),
        if k == FeedKind::Candle { 12 } else { 13 },
        "LISTENER fixed floor"
    );
    for (i, row) in rows.iter().enumerate() {
        assert_eq!(row["name"], names[i]);
        let h = Host::new(names[i]);
        let feed = Data::from(row["input"].clone());
        let topic = if k == FeedKind::Candle {
            "kline.CI_1_M.BTC"
        } else {
            "orderbook.BTC"
        };
        *h.current_topic.borrow_mut() = topic.into();
        if names[i] == "hole-spread" {
            let host = h.clone();
            h.registry.handlers.borrow_mut().insert(
                topic.into(),
                Rc::new(RefCell::new(vec![
                    HandlerSlot::Hole,
                    HandlerSlot::Callback(Rc::new(move |_| {
                        host.log(json!(["old"]));
                        Ok(())
                    })),
                ])),
            );
        }
        if [
            "live-replace",
            "prior-replace",
            "trailing-hole",
            "prior-fallback",
        ]
        .contains(&names[i])
        {
            let bids = feed.get("b");
            let Data::Array(a) = &bids else { panic!() };
            *h.first.borrow_mut() = a.borrow()[0].clone();
            *h.bids.borrow_mut() = Some(bids);
        }
        let host = h.clone();
        let cleanup = if k == FeedKind::Candle {
            listen_ohlcv(
                h.clone(),
                "BTC/USDT",
                "1m",
                Rc::new(move |value| {
                    host.log(json!(["candle"]));
                    host.candles.borrow_mut().push(value);
                    host.fail("callback")?;
                    if host.name == "market-callback-mutation" {
                        host.market.borrow_mut().id = "CHANGED".into();
                    }
                    Ok(())
                }),
            )
        } else {
            listen_order_book(
                h.clone(),
                "BTC/USDT",
                Rc::new(move |value| {
                    host.log(json!(["book"]));
                    host.books.borrow_mut().push(value);
                    host.fail("callback")
                }),
            )
        };
        let result = (|| {
            let cleanup = cleanup?;
            if names[i] == "disconnected" {
                cleanup()?;
                h.connected.set(true);
                let timer = h.timers.borrow_mut().remove(0);
                timer()?;
            }
            if names[i] == "hole-spread" {
                let message = Data::from(json!({"s1":"BTC@CI_1_M-TRADE"}));
                message.set("f", feed.clone());
                handle_kline_update(h.as_ref(), message)?;
            } else {
                let callback = h
                    .registry
                    .handlers
                    .borrow()
                    .get(topic)
                    .and_then(|a| a.borrow().last().cloned());
                if let Some(HandlerSlot::Callback(callback)) = callback {
                    callback(feed.clone())?;
                }
            }
            cleanup()
        })();
        if names[i] == "trailing-hole" {
            assert!(row["error"].is_null(), "SPARSE source listener success");
            assert_eq!(
                row["books"].as_array().unwrap().len(),
                1,
                "SPARSE source callback count"
            );
            assert_eq!(
                row["books"][0]["bids"].as_array().unwrap().len(),
                3,
                "SPARSE source length"
            );
            assert!(
                row["books"][0]["bids"][2].is_null(),
                "SPARSE source trailing hole"
            );
            assert_eq!(
                row["sourceOwnTrailing"], false,
                "SPARSE source absent own trailing slot"
            );
            assert_eq!(
                row["handlerLength"], 0,
                "SPARSE source cleanup leaves no handlers"
            );
            assert_eq!(
                h.registry
                    .handlers
                    .borrow()
                    .get(topic)
                    .unwrap()
                    .borrow()
                    .len(),
                1,
                "SPARSE native rejection leaves registered handler"
            );
            assert_eq!(
                result.unwrap_err().message,
                "Sparse GRVT orderbook levels cannot be represented by shared OrderBook",
                "SPARSE native visible representation failure"
            );
            assert_eq!(
                h.books.borrow().len(),
                0,
                "SPARSE native no fabricated callback"
            );
            continue;
        }
        error(result.as_ref().err(), &row["error"], "LISTENER");
        if k == FeedKind::Candle {
            candle_values(&h, row, callbacks[i]);
            let count = if [0, 1, 2, 3, 7, 9, 10].contains(&i) {
                1
            } else {
                0
            };
            assert_eq!(
                h.updates.borrow().len(),
                count,
                "CANDLE independent ticker-update count"
            );
            assert_eq!(
                row["updates"].as_array().unwrap().len(),
                count,
                "CANDLE independent source ticker-update count"
            );
            for update in h.updates.borrow().iter() {
                assert!(update.symbol.is_none(), "CANDLE ticker symbol absent");
                assert!(update.info.is_none(), "CANDLE ticker info absent");
                assert!(
                    [
                        update.mark,
                        update.index,
                        update.bid,
                        update.ask,
                        update.funding_rate,
                        update.open_interest,
                        update.volume,
                        update.quote_volume,
                        update.percentage,
                        update.funding_time
                    ]
                    .iter()
                    .all(Option::is_none),
                    "CANDLE native unrelated ticker fields absent"
                );
                scalar(
                    update.last.unwrap(),
                    &row["updates"][0]["last"],
                    "CANDLE native ticker last",
                );
            }
        } else {
            let sizes = [
                (3, 2),
                (2, 1),
                (0, 0),
                (1, 0),
                (3, 0),
                (3, 2),
                (3, 2),
                (0, 0),
                (0, 0),
                (3, 2),
                (3, 2),
                (3, 2),
                (3, 2),
            ];
            book_values(&h, row, sizes[i].0, sizes[i].1, callbacks[i]);
        }
        let handler_len = h
            .registry
            .handlers
            .borrow()
            .get(topic)
            .map(|a| a.borrow().len())
            .unwrap_or(0);
        let expected_handlers = if k == FeedKind::Candle {
            vec![0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 2]
        } else {
            vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0]
        };
        assert_eq!(
            handler_len, expected_handlers[i],
            "LISTENER independent native handler count"
        );
        assert_eq!(
            row["handlerLength"].as_u64().unwrap() as usize,
            expected_handlers[i],
            "LISTENER independent source handler count"
        );
        if let Some(actual) = h.registry.topics.borrow().get(topic) {
            scalar(
                *actual,
                &row["topics"][topic],
                "LISTENER native topic count",
            );
        } else {
            assert!(
                row["topics"].get(topic).is_none(),
                "LISTENER native absent topic"
            );
        }
        event_counts(&h, row, events[i], "LISTENER");
    }
}
#[test]
fn candle_listener_native() {
    listeners_matrix(FeedKind::Candle)
}
#[test]
fn book_listener_native() {
    listeners_matrix(FeedKind::Book)
}

#[test]
fn custom_orderbook_map_native() {
    let names = [
        "ordinary",
        "empty",
        "throw",
        "noncallable",
        "getter-throw",
        "retained",
        "arbitrary",
        "accessor-output",
        "replace-asks",
        "result-alias",
    ];
    let mut completed = 0;
    for name in names {
        let host = Host::new(&format!("map-{name}"));
        let feed = Data::from(json!({"i":"BTC","b":[{"p":"12","s":"2"}],"a":[]}));
        *host.bids.borrow_mut() = Some(feed.get("b"));
        let target = host.clone();
        let _cleanup = listen_order_book(
            host.clone(),
            "BTC/USDT",
            Rc::new(move |book| {
                target.events.borrow_mut().push(json!("callback"));
                target.books.borrow_mut().push(book);
                Ok(())
            }),
        )
        .unwrap();
        host.events.borrow_mut().clear();
        let handlers = host.registry.handlers.borrow()["orderbook.BTC"].clone();
        let HandlerSlot::Callback(parser) = handlers.borrow()[0].clone() else {
            panic!("native parser")
        };
        let result = parser(feed);
        if matches!(name, "throw" | "getter-throw") {
            assert_eq!(
                result.unwrap_err().message,
                "map-fail",
                "GRVT MAP native captured error"
            );
        } else if name == "noncallable" {
            assert_eq!(
                result.unwrap_err().name,
                "TypeError",
                "GRVT MAP native noncallability"
            );
        } else if matches!(name, "arbitrary" | "accessor-output") {
            assert_eq!(
                result.unwrap_err().name,
                "RepresentationError",
                "GRVT MAP native whole-callback refusal"
            );
            assert_eq!(
                host.books.borrow().len(),
                0,
                "GRVT MAP native refused callback count"
            );
            assert_eq!(
                host.output_reads.get(),
                0,
                "GRVT MAP native output getter remains uncalled"
            );
        } else {
            result.unwrap();
            assert_eq!(
                host.books.borrow().len(),
                1,
                "GRVT MAP native callback count"
            );
            assert_eq!(
                host.books.borrow()[0].bids.len(),
                usize::from(matches!(name, "ordinary" | "result-alias")),
                "GRVT MAP native overridden map result"
            );
            assert_eq!(
                host.books.borrow()[0].asks.len(),
                0,
                "GRVT MAP native asks result"
            );
            if matches!(name, "ordinary" | "result-alias") {
                let books = host.books.borrow();
                assert_eq!(
                    books[0].bids[0].price.to_bits(),
                    12.0f64.to_bits(),
                    "GRVT MAP native price"
                );
                assert_eq!(
                    books[0].bids[0].amount.to_bits(),
                    2.0f64.to_bits(),
                    "GRVT MAP native amount"
                );
                assert_eq!(
                    books[0].bids[0].total.to_bits(),
                    2.0f64.to_bits(),
                    "GRVT MAP native total"
                );
            }
        }
        let expected: &[&str] = match name {
            "getter-throw" | "noncallable" => &["get-bids"],
            "throw" => &["get-bids", "call-bids"],
            "arbitrary" | "accessor-output" => &["get-bids", "call-bids", "get-asks", "call-asks"],
            "replace-asks" => &[
                "get-bids",
                "call-bids",
                "get-asks-changed",
                "call-asks-changed",
                "callback",
            ],
            _ => &["get-bids", "call-bids", "get-asks", "call-asks", "callback"],
        };
        assert_eq!(
            host.events.borrow().len(),
            expected.len(),
            "GRVT MAP native independent effect count {name}"
        );
        for (i, event) in expected.iter().enumerate() {
            assert_eq!(
                host.events.borrow()[i],
                *event,
                "GRVT MAP native effect order {name}"
            );
        }
        if name == "result-alias" {
            let alias = host.alias_result.borrow_mut().take().unwrap();
            let Data::Array(rows) = alias else {
                panic!("retained result array");
            };
            rows.borrow()[0]
                .as_ref()
                .unwrap()
                .set("total", Data::Number(9.0));
            assert_eq!(
                host.books.borrow()[0].bids[0].total.to_bits(),
                2.0f64.to_bits(),
                "GRVT MAP native owned callback snapshot"
            );
        }
        let retained = host.retained.borrow_mut().take();
        if name == "retained" {
            let level = retained.expect("retained callback")(
                Data::from(json!({"p":"5","s":"2"})),
                Data::from(json!({"index-object":true})),
                Data::from(json!([{"s":"7"}])),
            )
            .unwrap();
            assert_eq!(
                host.number(&level.get("price")).unwrap().to_bits(),
                5.0f64.to_bits(),
                "GRVT MAP native retained price"
            );
            assert_eq!(
                host.number(&level.get("amount")).unwrap().to_bits(),
                2.0f64.to_bits(),
                "GRVT MAP native retained amount"
            );
            assert_eq!(
                host.number(&level.get("total")).unwrap().to_bits(),
                9.0f64.to_bits(),
                "GRVT MAP native alternate array"
            );
            assert_eq!(
                host.index_comparisons.get(),
                2,
                "GRVT MAP native repeated comparison"
            );
        } else {
            assert!(
                retained.is_none(),
                "GRVT MAP no unrequested retained callback"
            );
        }
        completed += 1;
    }
    assert_eq!(completed, 10, "GRVT MAP native completed case count");
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../parity/scripts/grvt-map-source-gate.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "GRVT MAP actual-source guard: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
