#[path = "support/retained_mutation.rs"]
mod retained_mutation;
use safe_cex_venue_coinbase::{
    market_position_reads::{ReadItem, ReadItems, ReadReply},
    portfolios_balances::{PortfolioFailure, PortfolioThrown},
    public_data::PublicData,
    public_market_listeners::*,
    public_market_reads::*,
    public_stream_dispatch::*,
    starbase_auth::CoinbaseStarbasePromise as Promise,
    starbase_public_frames::CoinbaseStarbaseFrameValue as Frame,
};
#[path = "support/microtasks.rs"]
mod microtasks;
use indexmap::IndexMap;
use microtasks::{Microtasks, completed, is_pending};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    process::Command,
    rc::Rc,
};
const FIXTURE: &str = include_str!("fixtures/public-market-data.json");
const HASH: &str = "566aa180cd18c5862d506da48f6b819748e0e7af22d375473881430da938e3c6";
fn source() -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-market-data.cjs"
        ))
        .env(
            "SAFE_CEX_TYPESCRIPT_PATH",
            std::env::var("SAFE_CEX_TYPESCRIPT_PATH").unwrap_or_else(|_| {
                // CI supplies an isolated compiler; this fallback only
                // serves a local run from the repo root.
                concat!(env!("CARGO_MANIFEST_DIR"), "/../../node_modules/typescript").to_owned()
            }),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "live source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn assert_num(actual: f64, expected: &Value, name: &str) {
    let bits =
        u64::from_str_radix(expected["$bits"].as_str().expect("source numeric bits"), 16).unwrap();
    if f64::from_bits(bits).is_nan() {
        assert!(actual.is_nan(), "{name}: native NaN");
    } else {
        assert_eq!(actual.to_bits(), bits, "{name}: native bits");
    }
}
fn assert_frame(actual: Option<Frame>, expected: &Value, name: &str) {
    if expected.get("$undefined") == Some(&json!(true)) {
        assert!(
            matches!(actual, None | Some(Frame::Undefined)),
            "{name}: native undefined"
        );
        return;
    }
    match actual.expect("native property") {
        Frame::Undefined => assert_eq!(expected["$undefined"], true, "native undefined"),
        Frame::Null => assert!(expected.is_null(), "{name}: null"),
        Frame::Bool(v) => assert_eq!(Some(v), expected.as_bool(), "{name}: bool"),
        Frame::Number(v) => assert_num(v, expected, name),
        Frame::String(v) => assert_eq!(
            v,
            expected
                .as_str()
                .unwrap()
                .encode_utf16()
                .collect::<Vec<_>>(),
            "{name}: UTF16"
        ),
        Frame::Array(v) => {
            let v = v.borrow();
            let e = expected.as_array().unwrap();
            assert_eq!(v.len(), e.len(), "{name}: array source length");
            for (a, e) in v.iter().zip(e) {
                assert_frame(Some(a.clone()), e, name);
            }
        }
        Frame::Object(v) => {
            let v = v.borrow();
            let e = expected.as_object().unwrap();
            assert_eq!(v.len(), e.len(), "{name}: object source length");
            for (k, e) in e {
                assert_frame(
                    v.get(&k.encode_utf16().collect::<Vec<_>>()).cloned(),
                    e,
                    name,
                );
            }
        }
    }
}
fn assert_data(actual: &PublicData, expected: &Value, name: &str) {
    match actual {
        PublicData::Json(v) => assert_frame(Some(Frame::Object(v.clone())), expected, name),
        PublicData::Generated(v) => {
            let v = v.borrow();
            let e = expected.as_object().unwrap();
            assert_eq!(v.len(), e.len(), "{name}: generated own fields");
            for (k, e) in e {
                assert!(
                    v.contains_key(&k.encode_utf16().collect::<Vec<_>>()),
                    "{name}: own {k}"
                );
                assert_frame(v[&k.encode_utf16().collect::<Vec<_>>()].clone(), e, name);
            }
        }
        PublicData::Candle(v) => {
            let v = v.borrow();
            assert_candle(&v, expected, name);
        }
    }
}
fn assert_candle(v: &safe_cex_types::Candle, e: &Value, name: &str) {
    for (k, v) in [
        ("timestamp", v.timestamp),
        ("open", v.open),
        ("high", v.high),
        ("low", v.low),
        ("close", v.close),
        ("volume", v.volume),
    ] {
        assert_num(v, &e[k], &format!("{name}.{k}"));
    }
    assert!(v.last_side.is_none(), "{name}: absent lastSide");
    assert_eq!(
        e.as_object().unwrap().len(),
        6,
        "{name}: independent native candle shape"
    );
}
fn data(v: &Value) -> PublicData {
    match Frame::from_json(v) {
        Frame::Object(v) => PublicData::Json(v),
        _ => panic!("object fixture"),
    }
}
fn same_data(a: &PublicData, b: &PublicData) -> bool {
    match (a, b) {
        (PublicData::Json(a), PublicData::Json(b)) => Rc::ptr_eq(a, b),
        (PublicData::Generated(a), PublicData::Generated(b)) => Rc::ptr_eq(a, b),
        (PublicData::Candle(a), PublicData::Candle(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
fn empty_data() -> PublicData {
    PublicData::Generated(Rc::new(RefCell::new(IndexMap::new())))
}
fn opts(v: &Value) -> PublicOHLCVOptionsRoot {
    Rc::new(RefCell::new(PublicOHLCVOptions {
        symbol: v["symbol"].as_str().unwrap().into(),
        interval: v["interval"].as_str().unwrap().into(),
        limit: v.get("limit").cloned(),
        from: v.get("from").cloned(),
        to: v.get("to").cloned(),
    }))
}
struct ReadHost {
    public: bool,
    types: CatalogSupportedTypes,
    replies: RefCell<VecDeque<PublicReadAsync<ReadReply>>>,
    recovery: RefCell<Vec<(PortfolioFailure, ReadItems)>>,
    calls: RefCell<Vec<(&'static str, Option<PublicMarketQuery>)>>,
    emits: RefCell<Vec<Option<Value>>>,
    emit_failure: RefCell<Option<PortfolioFailure>>,
    cache: CatalogMarkets,
    daily: RefCell<IndexMap<String, PublicData>>,
    shrink: bool,
    options: Option<PublicOHLCVOptionsRoot>,
    mutate: bool,
}
impl PublicMarketReadHost for ReadHost {
    fn public_only(&self) -> bool {
        self.public
    }
    fn get_supported_types(&self) -> PublicReadAsync<CatalogSupportedTypes> {
        Ok(Promise::resolved(Ok(self.types.clone())))
    }
    fn get(&self, e: &'static str, p: Option<PublicMarketQuery>) -> PublicReadAsync<ReadReply> {
        self.calls.borrow_mut().push((e, p));
        if self.shrink {
            self.types.borrow_mut().pop();
        }
        if self.mutate {
            let mut o = self.options.as_ref().unwrap().borrow_mut();
            o.symbol = "CHANGED".into();
            o.interval = "1m".into();
        }
        self.replies
            .borrow_mut()
            .pop_front()
            .expect("independent HTTP plan")
    }
    fn error_response_items(&self, e: &PortfolioFailure) -> Option<ReadItems> {
        self.recovery
            .borrow()
            .iter()
            .find(|(v, _)| Rc::ptr_eq(v, e))
            .map(|(_, v)| v.clone())
    }
    fn cached_markets(
        &self,
    ) -> std::result::Result<Rc<RefCell<Vec<Rc<RefCell<CatalogMarket>>>>>, Rc<PortfolioThrown>>
    {
        Ok(self.cache.clone())
    }
    fn emit_and_log_error(&self, v: Option<Value>) -> Result<(), PortfolioFailure> {
        self.emits.borrow_mut().push(v);
        match self.emit_failure.borrow().clone() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
    fn dayjs_value_of(&self, v: Option<Value>) -> Result<f64, PortfolioFailure> {
        Ok(v.map(|v| v.as_f64().unwrap_or(f64::NAN))
            .unwrap_or(1700000000000.0))
    }
    fn dayjs_subtract(&self, end: f64, n: f64, unit: &str) -> Result<f64, PortfolioFailure> {
        Ok(end
            - n * match unit {
                "m" => 60000.0,
                "d" => 86400000.0,
                _ => panic!("independent dayjs case domain"),
            })
    }
    fn daily_candle(&self, s: &str) -> Option<PublicData> {
        self.daily
            .borrow()
            .get(s)
            .cloned()
            .or_else(|| (s == "constructor").then(empty_data))
    }
    fn set_daily_candle(&self, s: String, c: PublicData) -> Result<(), PortfolioFailure> {
        self.daily.borrow_mut().insert(s, c);
        Ok(())
    }
}
fn read_receiver(
    input: &Value,
    tasks: &Microtasks,
) -> (
    Rc<PublicMarketReadReceiver<ReadHost>>,
    Vec<ReadItem>,
    Option<PublicOHLCVOptionsRoot>,
) {
    let o = input.get("opts").map(opts);
    let mut original = vec![];
    let mut recovery = vec![];
    let replies = input["replies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            if let Some(e) = p.get("error") {
                let e = Rc::new(PortfolioThrown::from_value(Some(e.clone())));
                if let Some(a) = e
                    .value
                    .as_ref()
                    .and_then(|v| v.pointer("/response/data"))
                    .and_then(Value::as_array)
                {
                    let a = Rc::new(RefCell::new(
                        a.iter()
                            .map(|v| {
                                Rc::new(RefCell::new(
                                    safe_cex_types::retained_json::RetainedJsonInput::from(
                                        v.clone(),
                                    ),
                                ))
                            })
                            .collect::<Vec<_>>(),
                    ));
                    original.extend(a.borrow().clone());
                    recovery.push((e.clone(), a));
                }
                Ok(Promise::resolved(Err(e)))
            } else {
                let a = Rc::new(RefCell::new(
                    p["data"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| {
                            Rc::new(RefCell::new(
                                safe_cex_types::retained_json::RetainedJsonInput::from(v.clone()),
                            ))
                        })
                        .collect::<Vec<_>>(),
                ));
                original.extend(a.borrow().clone());
                Ok(Promise::resolved(Ok(ReadReply { items: a })))
            }
        })
        .collect();
    let mut daily = IndexMap::new();
    if let Some(d) = input.get("daily") {
        daily.insert(input["opts"]["symbol"].as_str().unwrap().into(), data(d));
    }
    (
        Rc::new(PublicMarketReadReceiver {
            spawn: tasks.spawner(),
            host: ReadHost {
                public: input["publicOnly"].as_bool().unwrap_or(false),
                types: Rc::new(RefCell::new(
                    input["supported"]
                        .as_array()
                        .map(|v| v.iter().map(|v| v.as_str().unwrap().into()).collect())
                        .unwrap_or_default(),
                )),
                replies: RefCell::new(replies),
                recovery: RefCell::new(recovery),
                calls: RefCell::new(vec![]),
                emits: RefCell::new(vec![]),
                emit_failure: RefCell::new(None),
                cache: Rc::new(RefCell::new(vec![])),
                daily: RefCell::new(daily),
                shrink: input["shrink"].as_bool().unwrap_or(false),
                options: o.clone(),
                mutate: input["mutateOpts"].as_bool().unwrap_or(false),
            },
        }),
        original,
        o,
    )
}
#[test]
fn public_reads_assert_native_catalog_candles_and_query_bits_against_live_source() {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE.as_bytes())),
        HASH,
        "public corpus content pin"
    );
    let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
    let oracle = source();
    assert_eq!(
        fixture["reads"].as_array().unwrap().len(),
        22,
        "independent read floor"
    );
    assert_eq!(
        oracle["reads"].as_array().unwrap().len(),
        22,
        "source read floor"
    );
    for (input, e) in fixture["reads"]
        .as_array()
        .unwrap()
        .iter()
        .zip(oracle["reads"].as_array().unwrap())
    {
        let label = input["label"].as_str().unwrap();
        let tasks = Microtasks::default();
        let (r, original, o) = read_receiver(input, &tasks);
        if input["op"] == "fetchOHLCV" {
            let p = fetch_ohlcv(r.clone(), o.clone().unwrap());
            tasks.drain();
            match completed(p) {
                Ok(v) => {
                    assert!(e["error"].is_null(), "{label}: success");
                    let v = v.borrow();
                    let expected = e["value"].as_array().unwrap();
                    assert_eq!(v.len(), expected.len(), "{label}: source candle length");
                    for (a, e) in v.iter().zip(expected) {
                        assert_candle(&a.borrow(), e, label);
                    }
                    let name = o.as_ref().unwrap().borrow().symbol.clone();
                    let daily = r.host.daily_candle(&name);
                    if e["daily"].get("$undefined") == Some(&json!(true)) {
                        assert!(daily.is_none(), "{label}: no daily cache");
                    } else {
                        assert_data(daily.as_ref().unwrap(), &e["daily"], label);
                    }
                    let index = e["dailyIndex"].as_i64().unwrap();
                    if index >= 0 {
                        assert!(
                            same_data(
                                daily.as_ref().unwrap(),
                                &PublicData::Candle(v[index as usize].clone())
                            ),
                            "{label}: newest cache candle root"
                        );
                    }
                }
                Err(err) => {
                    assert_eq!(
                        err.value.as_ref().unwrap(),
                        &e["error"],
                        "{label}: source error record"
                    );
                }
            }
        } else {
            let p = if input["op"] == "fetchMarkets" {
                fetch_markets(r.clone())
            } else {
                fetch_markets_from_intx_instruments(r.clone())
            };
            tasks.drain();
            let v = completed(p).unwrap();
            assert_eq!(
                Rc::ptr_eq(&v, &r.host.cache),
                e["cached"].as_bool().unwrap(),
                "{label}: cached root"
            );
            let v = v.borrow();
            let expected = e["value"].as_array().unwrap();
            assert_eq!(
                v.len(),
                expected.len(),
                "{label}: market result source length"
            );
            for (i, (a, e)) in v.iter().zip(expected).enumerate() {
                let a = a.borrow();
                let m = &a.fields;
                for (k, a) in [
                    ("id", &m.id),
                    ("symbol", &m.symbol),
                    ("base", &m.base),
                    ("quote", &m.quote),
                ] {
                    assert_eq!(a, e[k].as_str().unwrap(), "{label}: native {k}");
                }
                assert_eq!(m.active, e["active"].as_bool().unwrap(), "{label}: active");
                assert_eq!(
                    m.is_spot,
                    e.get("isSpot").and_then(Value::as_bool),
                    "{label}: own spot flag"
                );
                assert_eq!(
                    m.is_expiring,
                    e.get("isExpiring").and_then(Value::as_bool),
                    "{label}: own expiry flag"
                );
                for (k, a) in [
                    ("precision/amount", m.precision.amount),
                    ("precision/price", m.precision.price),
                    ("limits/amount/min", m.limits.amount.min),
                    ("limits/amount/max", m.limits.amount.max),
                    ("limits/leverage/min", m.limits.leverage.min),
                    ("limits/leverage/max", m.limits.leverage.max),
                    ("contractSize", m.contract_size.unwrap()),
                ] {
                    assert_num(a, e.pointer(&format!("/{k}")).unwrap(), label);
                }
                let index = oracle["reads"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|v| v["label"] == input["label"])
                    .unwrap()["info"][i]
                    .as_u64()
                    .unwrap() as usize;
                assert!(
                    Rc::ptr_eq(&a.info, &original[index]),
                    "{label}: original native market info root"
                );
                assert!(
                    m.info.is_none()
                        && m.is_inverse.is_none()
                        && m.is_perpetual.is_none()
                        && m.is_read_only.is_none()
                        && m.pretty_symbol.is_none()
                        && m.taker_fee.is_none()
                        && m.maker_fee.is_none(),
                    "{label}: native paired projection contains no spurious fields"
                );
            }
        }
        // Native values/roots were asserted above before comparing HTTP boundary calls.
        let calls = r.host.calls.borrow();
        let expected = e["calls"].as_array().unwrap();
        assert_eq!(calls.len(), expected.len(), "{label}: request count");
        for ((endpoint, params), e) in calls.iter().zip(expected) {
            assert_eq!(
                *endpoint,
                e["endpoint"].as_str().unwrap(),
                "{label}: endpoint"
            );
            match params {
                Some(PublicMarketQuery::Catalog(v)) => {
                    assert_frame(Some(Frame::from_json(v)), &e["params"], label)
                }
                Some(PublicMarketQuery::Candles(v)) => {
                    assert_eq!(
                        v.product_id,
                        e["params"]["product_id"].as_str().unwrap(),
                        "{label}: normalized product"
                    );
                    assert_eq!(
                        v.granularity,
                        e["params"]["granularity"].as_str().unwrap(),
                        "{label}: granularity"
                    );
                    assert_num(v.start, &e["params"]["start"], label);
                    assert_num(v.end, &e["params"]["end"], label);
                }
                None => assert_eq!(e["params"]["$undefined"], true, "{label}: omitted params"),
            }
        }
        let emitted = r.host.emits.borrow();
        let expected = e["emits"].as_array().unwrap();
        assert_eq!(emitted.len(), expected.len(), "{label}: emitted count");
        for (a, e) in emitted.iter().zip(expected) {
            assert_frame(a.as_ref().map(Frame::from_json), e, label);
        }
    }
}
enum ListenerCall {
    Send(PublicSubscription),
    Fetch(PublicOHLCVOptionsRoot),
    Timer(f64),
    Warn(String, Option<Frame>),
    Cleanup(String),
}
struct ListenerHost {
    connected: Cell<bool>,
    public: bool,
    daily: RefCell<IndexMap<String, PublicData>>,
    trace: RefCell<Vec<ListenerCall>>,
    timers: RefCell<VecDeque<PublicCleanup>>,
}
impl ListenerHost {
    fn push(&self, v: ListenerCall) {
        self.trace.borrow_mut().push(v);
    }
}
impl PublicStreamHost for ListenerHost {
    fn is_disposed(&self) -> bool {
        false
    }
    fn testnet(&self) -> bool {
        false
    }
    fn reconnecting_enabled(&self) -> bool {
        false
    }
    fn create_websocket(&self, _: &'static str) -> Result<PublicSocket, ListenerFailure> {
        panic!("unexpected listener transport")
    }
    fn add_listener(
        &self,
        _: PublicSocket,
        _: PublicSocketListener,
    ) -> Result<(), ListenerFailure> {
        panic!("unexpected listener transport")
    }
    fn reset_orderbook(&self) -> Result<(), ListenerFailure> {
        panic!("test resets the shared state explicitly")
    }
    fn emit_ws_reconnected(&self) -> Result<(), ListenerFailure> {
        panic!("unexpected listener reconnect")
    }
    fn markets_symbols(&self) -> Result<Vec<String>, ListenerFailure> {
        panic!("unexpected listener markets")
    }
    fn send_subscription(&self, p: PublicSubscription) -> Result<(), ListenerFailure> {
        self.push(ListenerCall::Send(p));
        Ok(())
    }
    fn track_last_received(&self) -> Result<(), ListenerFailure> {
        panic!("unexpected tracking")
    }
    fn json_parse(&self, _: &str) -> Option<Frame> {
        panic!("direct decoded-native listener assertion")
    }
    fn emit_error(&self, _: Option<Frame>) -> Result<(), ListenerFailure> {
        panic!("unexpected emitter")
    }
    fn console_error_text(&self, _: Vec<u16>) -> Result<(), ListenerFailure> {
        panic!("unexpected console error")
    }
    fn console_error_frame(&self, _: Frame) -> Result<(), ListenerFailure> {
        panic!("unexpected console error")
    }
    fn handle_pong_event(&self) -> Result<(), ListenerFailure> {
        panic!("unexpected pong")
    }
    fn record_message_handler_stats(&self, _: &str) -> Result<(), ListenerFailure> {
        panic!("unexpected dispatcher stats")
    }
}
impl PublicMarketListenerHost for ListenerHost {
    fn is_connected(&self) -> bool {
        self.connected.get()
    }
    fn public_data_only_option(&self) -> bool {
        self.public
    }
    fn timeframe_seconds(&self, v: &str) -> Option<f64> {
        match v {
            "1m" => Some(60.0),
            "5m" => Some(300.0),
            "1d" => Some(86400.0),
            _ => None,
        }
    }
    fn daily_candle(&self, s: &str) -> Option<PublicData> {
        self.daily
            .borrow()
            .get(s)
            .cloned()
            .or_else(|| (s == "constructor").then(empty_data))
    }
    fn set_daily_candle(&self, s: String, c: PublicData) -> Result<(), ListenerFailure> {
        self.daily.borrow_mut().insert(s, c);
        Ok(())
    }
    fn utc_end_of_day_ms(&self) -> Result<f64, ListenerFailure> {
        Ok(1700006399999.0)
    }
    fn fetch_ohlcv_detached(&self, o: PublicOHLCVOptionsRoot) -> Result<(), ListenerFailure> {
        self.push(ListenerCall::Fetch(o));
        Ok(())
    }
    fn dayjs_unix(&self, v: Option<Frame>) -> Result<f64, ListenerFailure> {
        let Some(Frame::Number(v)) = v else {
            panic!("independent dayjs numeric-time floor")
        };
        Ok((v / 1000.0).floor())
    }
    fn add(&self, left: Option<Frame>, right: f64) -> Result<f64, ListenerFailure> {
        let Some(Frame::Number(left)) = left else {
            panic!("independent numeric add floor")
        };
        Ok(safe_cex_num::add(left, right))
    }
    fn warn(&self, m: &str, value: Option<Frame>) -> Result<(), ListenerFailure> {
        self.push(ListenerCall::Warn(m.into(), value));
        Ok(())
    }
    fn set_timeout(&self, cb: PublicCleanup, delay: f64) -> Result<(), ListenerFailure> {
        self.push(ListenerCall::Timer(delay));
        self.timers.borrow_mut().push_back(cb);
        Ok(())
    }
    fn cleanup_message_handler_stats(&self, topic: &str) -> Result<(), ListenerFailure> {
        self.push(ListenerCall::Cleanup(topic.into()));
        Ok(())
    }
    fn nanoid(&self) -> Result<String, ListenerFailure> {
        Ok("inner".into())
    }
    fn update_order_book(
        &self,
        b: PublicListenerBook,
        side: &str,
        level: Rc<RefCell<PublicBookLevel>>,
    ) -> Result<(), ListenerFailure> {
        let levels = if side == "bids" {
            b.borrow().bids.clone()
        } else {
            b.borrow().asks.clone()
        };
        let mut levels = levels.borrow_mut();
        let update = level.borrow();
        let index = levels.iter().position(|v| v.borrow().price == update.price);
        if let Some(i) = index {
            if update.amount == 0.0 {
                levels.remove(i);
            } else {
                levels[i].borrow_mut().amount = update.amount;
            }
        } else if update.amount != 0.0 {
            let i = levels
                .iter()
                .position(|v| {
                    if side == "bids" {
                        v.borrow().price < update.price
                    } else {
                        v.borrow().price > update.price
                    }
                })
                .unwrap_or(levels.len());
            drop(update);
            levels.insert(i, level);
        }
        Ok(())
    }
    fn sort_order_book(&self, b: PublicListenerBook) -> Result<(), ListenerFailure> {
        let b = b.borrow();
        b.asks
            .borrow_mut()
            .sort_by(|a, b| a.borrow().price.partial_cmp(&b.borrow().price).unwrap());
        b.bids
            .borrow_mut()
            .sort_by(|a, b| b.borrow().price.partial_cmp(&a.borrow().price).unwrap());
        Ok(())
    }
    fn calc_order_book_total(&self, b: PublicListenerBook) -> Result<(), ListenerFailure> {
        let b = b.borrow();
        for levels in [&b.bids, &b.asks] {
            let mut total = 0.0;
            for level in levels.borrow().iter() {
                let mut level = level.borrow_mut();
                total += level.amount;
                level.total = total;
            }
        }
        Ok(())
    }
}
fn assert_listener_call(a: &ListenerCall, e: &Value, label: &str) {
    match a {
        ListenerCall::Send(p) => {
            assert_eq!(e["kind"], "send", "{label}: native send");
            let e = &e["payload"];
            assert_eq!(
                p.r#type,
                e["type"].as_str().unwrap(),
                "{label}: subscription type"
            );
            assert_eq!(
                p.channel,
                e["channel"].as_str().unwrap(),
                "{label}: subscription channel"
            );
            assert_eq!(
                p.product_ids,
                e["product_ids"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().to_string())
                    .collect::<Vec<_>>(),
                "{label}: native product IDs"
            );
        }
        ListenerCall::Fetch(o) => {
            assert_eq!(e["kind"], "fetch", "{label}: detached fetch");
            let o = o.borrow();
            let e = &e["opts"];
            assert_eq!(
                o.symbol,
                e["symbol"].as_str().unwrap(),
                "{label}: detached symbol"
            );
            assert_eq!(
                o.interval,
                e["interval"].as_str().unwrap(),
                "{label}: detached interval"
            );
            assert_frame(o.limit.as_ref().map(Frame::from_json), &e["limit"], label);
            assert_frame(o.to.as_ref().map(Frame::from_json), &e["to"], label);
            assert!(o.from.is_none(), "{label}: detached from absent");
        }
        ListenerCall::Timer(v) => {
            assert_eq!(e["kind"], "timer", "{label}: timer");
            assert_num(*v, &e["delay"], label);
        }
        ListenerCall::Warn(m, v) => {
            assert_eq!(e["kind"], "warn", "{label}: warn");
            assert_eq!(m, e["message"].as_str().unwrap(), "{label}: warning");
            assert_frame(v.clone(), &e["value"], label);
        }
        ListenerCall::Cleanup(topic) => {
            assert_eq!(e["kind"], "cleanup", "{label}: cleanup");
            assert_eq!(
                topic,
                e["topic"].as_str().unwrap(),
                "{label}: cleanup topic"
            );
        }
    }
}
fn call_parser<H>(
    r: &Rc<PublicListenerReceiver<H>>,
    topic: &str,
    message: PublicStreamMessage,
) -> Result<(), ListenerFailure> {
    let h = r.stream.message_handlers.borrow()[topic].borrow()[0].clone();
    match h {
        PublicStreamHandler::Callback(cb) => cb(message),
        _ => panic!("listener callback"),
    }
}
fn set_high(c: &PublicData, n: f64) {
    match c {
        PublicData::Candle(v) => v.borrow_mut().high = n,
        PublicData::Json(v) => {
            v.borrow_mut()
                .insert("high".encode_utf16().collect(), Frame::Number(n));
        }
        PublicData::Generated(v) => {
            v.borrow_mut()
                .insert("high".encode_utf16().collect(), Some(Frame::Number(n)));
        }
    }
}
fn assert_book(b: &PublicListenerBook, e: &Value, label: &str) {
    let b = b.borrow();
    for (k, levels) in [("bids", &b.bids), ("asks", &b.asks)] {
        let levels = levels.borrow();
        let expected = e[k].as_array().unwrap();
        assert_eq!(levels.len(), expected.len(), "{label}: native side count");
        for (a, e) in levels.iter().zip(expected) {
            let a = a.borrow();
            assert_num(a.price, &e["price"], label);
            assert_num(a.amount, &e["amount"], label);
            assert_num(a.total, &e["total"], label);
        }
    }
}
#[test]
fn public_listeners_assert_native_records_book_levels_and_retained_roots_against_live_source() {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE.as_bytes())),
        HASH,
        "listener corpus content pin"
    );
    let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
    let oracle = source();
    assert_eq!(
        fixture["listeners"].as_array().unwrap().len(),
        16,
        "independent listener floor"
    );
    assert_eq!(
        oracle["listeners"].as_array().unwrap().len(),
        16,
        "source listener floor"
    );
    for (input, e) in fixture["listeners"]
        .as_array()
        .unwrap()
        .iter()
        .zip(oracle["listeners"].as_array().unwrap())
    {
        let label = input["label"].as_str().unwrap().to_string();
        let name = input["symbol"].as_str().unwrap_or("BTC-USDC");
        let normalized = if name == "BTC-USDC" { "BTC-USD" } else { name };
        let mut daily = IndexMap::new();
        if let Some(v) = input.get("daily") {
            daily.insert(name.into(), data(v));
        }
        let r = Rc::new(PublicListenerReceiver {
            connectivity:
                safe_cex_venue_coinbase::public_market_listeners::PublicListenerConnectivity::Host,
            stream: Rc::new(PublicStreamReceiver::new(ListenerHost {
                connected: Cell::new(!input["disconnected"].as_bool().unwrap_or(false)),
                public: input["publicOnly"].as_bool().unwrap_or(false),
                daily: RefCell::new(daily),
                trace: RefCell::new(vec![]),
                timers: RefCell::new(VecDeque::new()),
            })),
            inner_state: RefCell::new(IndexMap::new()),
        });
        let candle_roots = Rc::new(RefCell::new(Vec::<PublicData>::new()));
        let book_roots = Rc::new(RefCell::new(Vec::<PublicListenerBook>::new()));
        let callbacks = Rc::new(Cell::new(0usize));
        let expected = Rc::new(e["callbacks"].as_array().unwrap().clone());
        let cb: PublicCandleCallback = {
            let (roots, count, expected, label) = (
                candle_roots.clone(),
                callbacks.clone(),
                expected.clone(),
                label.clone(),
            );
            let mutate = input["mutateCallback"].as_bool().unwrap_or(false);
            let mutate_timestamp = input["mutateTimestamp"].as_bool().unwrap_or(false);
            Rc::new(move |c| {
                let i = count.get();
                assert_data(&c, expected.get(i).expect("source callback floor"), &label);
                count.set(i + 1);
                roots.borrow_mut().push(c.clone());
                if mutate_timestamp {
                    let PublicData::Generated(v) = &c else {
                        panic!("trade-generated record")
                    };
                    let mut v = v.borrow_mut();
                    let key = "timestamp".encode_utf16().collect::<Vec<_>>();
                    let Some(Some(Frame::Number(n))) = v.get(&key) else {
                        panic!("native numeric timestamp")
                    };
                    let text = safe_cex_num::js_number_string(*n).encode_utf16().collect();
                    v.insert(key, Some(Frame::String(text)));
                }
                if mutate {
                    set_high(&c, 91.0);
                }
                Ok(())
            })
        };
        let book_cb: PublicBookCallback = {
            let (roots, count, expected, label) = (
                book_roots.clone(),
                callbacks.clone(),
                expected.clone(),
                label.clone(),
            );
            Rc::new(move |b| {
                let i = count.get();
                assert_book(
                    &b,
                    expected.get(i).expect("source book callback floor"),
                    &label,
                );
                count.set(i + 1);
                roots.borrow_mut().push(b);
                Ok(())
            })
        };
        let cleanup = if input["kind"] == "book" {
            listen_order_book(r.clone(), name.into(), book_cb).unwrap()
        } else {
            listen_ohlcv(
                r.clone(),
                opts(&json!({"symbol":name,"interval":input["interval"]})),
                cb,
            )
            .unwrap()
        };
        if input["cleanupBeforeRetry"] == true {
            cleanup().unwrap();
        }
        if input["disconnected"] == true {
            r.stream.host.connected.set(true);
            assert_eq!(r.stream.host.timers.borrow().len(), 1, "{label}: one retry");
            let retry = r.stream.host.timers.borrow_mut().pop_front().unwrap();
            retry().unwrap();
        }
        assert_eq!(
            input["steps"].as_array().unwrap().len(),
            e["steps"].as_array().unwrap().len(),
            "{label}: independent step count"
        );
        for (step, e) in input["steps"]
            .as_array()
            .unwrap()
            .iter()
            .zip(e["steps"].as_array().unwrap())
        {
            let error = if step["reset"] == true {
                r.inner_state.borrow()["inner"]
                    .borrow_mut()
                    .is_snapshot_loaded = false;
                Ok(())
            } else {
                let (channel, value) = if let Some(v) = step.get("primary") {
                    ("candles", v)
                } else if let Some(v) = step.get("trade") {
                    ("market_trades", v)
                } else {
                    ("level2", &step["book"])
                };
                call_parser(
                    &r,
                    &format!("{channel}.{normalized}"),
                    PublicStreamMessage::Frame(Frame::from_json(value)),
                )
            };
            match error {
                Ok(()) => assert!(e["error"].is_null(), "{label}: no event error"),
                Err(error) => {
                    assert_eq!(
                        error.name,
                        e["error"]["name"].as_str().unwrap(),
                        "{label}: error name"
                    );
                    assert_eq!(
                        error.message,
                        e["error"]["message"].as_str().unwrap(),
                        "{label}: error message"
                    );
                }
            }
            assert_num(callbacks.get() as f64, &e["callbackCount"], &label);
            let state = r.inner_state.borrow().get("inner").cloned();
            assert_eq!(
                state.map(|s| s.borrow().is_snapshot_loaded),
                e["loaded"].as_bool(),
                "{label}: shared snapshot flag"
            );
        }
        assert_eq!(
            callbacks.get(),
            expected.len(),
            "{label}: independent source callback count"
        );
        let old = r.stream.message_handlers.borrow().clone();
        cleanup().unwrap();
        if input["repeatCleanup"] == true {
            cleanup().unwrap();
        }
        for (k, v) in old {
            let new = r.stream.message_handlers.borrow()[&k].clone();
            if k.contains('.') {
                assert!(
                    !Rc::ptr_eq(&v, &new),
                    "{label}: cleanup replacement callback array"
                );
            }
        }
        let daily = r.stream.host.daily_candle(name);
        if e["daily"].get("$undefined") == Some(&json!(true)) {
            assert!(daily.is_none(), "{label}: missing daily");
        } else {
            assert_data(daily.as_ref().unwrap(), &e["daily"], &label);
        }
        let roots = candle_roots.borrow();
        assert_eq!(
            roots
                .last()
                .zip(daily.as_ref())
                .is_some_and(|(a, b)| same_data(a, b)),
            e["dailyIsLast"].as_bool().unwrap(),
            "{label}: daily latest callback root"
        );
        let distinct = e["candleDistinct"].as_array().unwrap();
        assert_eq!(
            roots.len().saturating_sub(1),
            distinct.len(),
            "{label}: candle root floor"
        );
        for (pair, e) in roots.windows(2).zip(distinct) {
            assert_eq!(
                !same_data(&pair[0], &pair[1]),
                e.as_bool().unwrap(),
                "{label}: replacement candle roots"
            );
        }
        let books = book_roots.borrow();
        assert_eq!(
            !books.is_empty() && books.iter().all(|b| Rc::ptr_eq(b, &books[0])),
            e["bookSame"].as_bool().unwrap(),
            "{label}: persistent outer book root"
        );
        // Native callback values, flags and identities precede wire subscription assertions.
        let topics = r.stream.topics.borrow();
        let expected = e["topics"].as_object().unwrap();
        assert_eq!(
            topics.len(),
            expected.len(),
            "{label}: independent complete topic-key count"
        );
        for (k, v) in expected {
            assert_num(topics[k], v, &label);
        }
        let trace = r.stream.host.trace.borrow();
        let expected = e["trace"].as_array().unwrap();
        assert_eq!(
            trace.len(),
            expected.len(),
            "{label}: independent trace floor"
        );
        for (a, e) in trace.iter().zip(expected) {
            assert_listener_call(a, e, &label);
        }
    }
}
#[test]
fn catalog_launches_children_eagerly_preserves_input_order_and_fails_fast() {
    let tasks = Microtasks::default();
    let input = json!({"supported":["spot","cfm"],"replies":[]});
    let (r, _, _) = read_receiver(&input, &tasks);
    let (first, first_resolve) = Promise::pending();
    let (second, second_resolve) = Promise::pending();
    r.host.replies.borrow_mut().extend([Ok(first), Ok(second)]);
    let mut p = fetch_markets(r.clone());
    assert!(
        r.host.calls.borrow().is_empty(),
        "getSupportedTypes await suspends before GET"
    );
    tasks.drain();
    assert_eq!(
        r.host.calls.borrow().len(),
        2,
        "catalog eagerly launched two independent supported requests"
    );
    let item = |id| {
        Rc::new(RefCell::new(
            json!({"product_id":id,"base_currency_id":"X","quote_currency_id":"USD","status":"TRADING","price_increment":"1","base_increment":"2","base_min_size":"3","base_max_size":"4"}).into(),
        ))
    };
    let a = item("A");
    let b = item("B");
    second_resolve.resolve(Ok(ReadReply {
        items: Rc::new(RefCell::new(vec![b.clone()])),
    }));
    tasks.drain();
    assert!(
        is_pending(&mut p) && is_pending(&mut p),
        "catalog waits unresolved first child after reverse completion"
    );
    // A queued mutation before resuming the first response must affect the retained info root.
    let changed = a.clone();
    tasks.enqueue(move || {
        retained_mutation::set(
            &mut changed.borrow_mut(),
            "base_currency_id",
            json!("CHANGED").into(),
        );
    });
    tasks.drain();
    first_resolve.resolve(Ok(ReadReply {
        items: Rc::new(RefCell::new(vec![a.clone()])),
    }));
    tasks.drain();
    let v = completed(p).unwrap();
    let v = v.borrow();
    assert_eq!(
        v.iter()
            .map(|v| v.borrow().fields.id.clone())
            .collect::<Vec<_>>(),
        ["A", "B"],
        "catalog keeps request input order after reverse completion"
    );
    assert_eq!(
        v[0].borrow().fields.base,
        "CHANGED",
        "retained item read after queued mutation"
    );
    assert!(
        Rc::ptr_eq(&v[0].borrow().info, &a) && Rc::ptr_eq(&v[1].borrow().info, &b),
        "catalog info aliases actual replies"
    );
    drop(v);
    let (first, _first_resolve) = Promise::pending();
    let (second, second_resolve) = Promise::pending();
    r.host.replies.borrow_mut().extend([Ok(first), Ok(second)]);
    let p = fetch_markets(r.clone());
    tasks.drain();
    second_resolve.resolve(Err(Rc::new(PortfolioThrown::from_value(Some(
        json!({"message":"failed child"}),
    )))));
    tasks.drain();
    assert!(
        Rc::ptr_eq(&completed(p).unwrap(), &r.host.cache),
        "Promise.all failure returns cached root before unresolved sibling"
    );
    assert_eq!(
        r.host.emits.borrow().last(),
        Some(&Some(json!("failed child"))),
        "failed child diagnostic"
    );
}
struct Delegates {
    options: PublicOHLCVOptionsRoot,
    candle: PublicCandleCallback,
    book: PublicBookCallback,
    cleanup: PublicCleanup,
}
impl PublicMarketDelegateHost for Delegates {
    fn public_listen_ohlcv(
        &self,
        o: PublicOHLCVOptionsRoot,
        c: PublicCandleCallback,
    ) -> Result<PublicCleanup, ListenerFailure> {
        assert!(
            Rc::ptr_eq(&o, &self.options),
            "delegate original options root"
        );
        assert!(
            Rc::ptr_eq(&c, &self.candle),
            "delegate original candle callback"
        );
        Ok(self.cleanup.clone())
    }
    fn public_listen_order_book(
        &self,
        s: String,
        c: PublicBookCallback,
    ) -> Result<PublicCleanup, ListenerFailure> {
        assert_eq!(s, "RAW-USDC", "delegate unnormalized input symbol");
        assert!(
            Rc::ptr_eq(&c, &self.book),
            "delegate original book callback"
        );
        Ok(self.cleanup.clone())
    }
}
#[test]
fn delegates_and_listener_registration_preserve_native_roots() {
    let o = opts(&json!({"symbol":"RAW-USDC","interval":"5m"}));
    let host = Delegates {
        options: o.clone(),
        candle: Rc::new(|_| Ok(())),
        book: Rc::new(|_| Ok(())),
        cleanup: Rc::new(|| Ok(())),
    };
    assert!(
        Rc::ptr_eq(
            &exchange_listen_ohlcv(&host, o, host.candle.clone()).unwrap(),
            &host.cleanup
        ),
        "delegate candle exact cleanup root"
    );
    assert!(
        Rc::ptr_eq(
            &exchange_listen_order_book(&host, "RAW-USDC".into(), host.book.clone()).unwrap(),
            &host.cleanup
        ),
        "delegate book exact cleanup root"
    );
    let r = Rc::new(PublicListenerReceiver {
        connectivity:
            safe_cex_venue_coinbase::public_market_listeners::PublicListenerConnectivity::Host,
        stream: Rc::new(PublicStreamReceiver::new(ListenerHost {
            connected: Cell::new(true),
            public: true,
            daily: RefCell::new(IndexMap::new()),
            trace: RefCell::new(vec![]),
            timers: RefCell::new(VecDeque::new()),
        })),
        inner_state: RefCell::new(IndexMap::new()),
    });
    let native = Rc::new(RefCell::new(safe_cex_types::Candle {
        timestamp: 1.0,
        open: 2.0,
        high: 3.0,
        low: 1.0,
        close: 2.0,
        volume: 4.0,
        last_side: None,
    }));
    let expected = native.clone();
    let seen = Rc::new(Cell::new(0));
    let count = seen.clone();
    let cleanup = listen_ohlcv(
        r.clone(),
        opts(&json!({"symbol":"X","interval":"5m"})),
        Rc::new(move |c| {
            let PublicData::Candle(c) = c else {
                panic!("native dispatcher candle variant")
            };
            assert!(
                Rc::ptr_eq(&c, &expected),
                "primary candle retains native dispatcher root"
            );
            count.set(count.get() + 1);
            Ok(())
        }),
    )
    .unwrap();
    let old = r.stream.message_handlers.borrow()["candles.X"].clone();
    let _second = listen_ohlcv(
        r.clone(),
        opts(&json!({"symbol":"X","interval":"5m"})),
        Rc::new(|_| Ok(())),
    )
    .unwrap();
    let new = r.stream.message_handlers.borrow()["candles.X"].clone();
    assert!(
        !Rc::ptr_eq(&old, &new),
        "registration replaces callback array root"
    );
    assert_eq!(
        old.borrow().len(),
        1,
        "retained earlier callback array is unchanged"
    );
    assert_eq!(
        new.borrow().len(),
        2,
        "new callback array includes independent second registration"
    );
    match old.borrow()[0].clone() {
        PublicStreamHandler::Callback(cb) => {
            cb(PublicStreamMessage::Candle(native.clone())).unwrap()
        }
        _ => panic!("callback"),
    };
    cleanup().unwrap();
    assert_eq!(
        old.borrow().len(),
        1,
        "cleanup preserves retained active iterator array"
    );
    assert_eq!(
        r.stream.message_handlers.borrow()["candles.X"]
            .borrow()
            .len(),
        1,
        "cleanup removes only original parser by identity"
    );
    match old.borrow()[0].clone() {
        PublicStreamHandler::Callback(cb) => cb(PublicStreamMessage::Candle(native)).unwrap(),
        _ => panic!("callback"),
    };
    assert_eq!(
        seen.get(),
        2,
        "retained parser remains callable after cleanup"
    );
}
#[test]
fn book_snapshots_replace_arrays_and_updates_mutate_retained_level() {
    let r = Rc::new(PublicListenerReceiver {
        connectivity:
            safe_cex_venue_coinbase::public_market_listeners::PublicListenerConnectivity::Host,
        stream: Rc::new(PublicStreamReceiver::new(ListenerHost {
            connected: Cell::new(true),
            public: false,
            daily: RefCell::new(IndexMap::new()),
            trace: RefCell::new(vec![]),
            timers: RefCell::new(VecDeque::new()),
        })),
        inner_state: RefCell::new(IndexMap::new()),
    });
    let roots = Rc::new(RefCell::new(vec![]));
    let sink = roots.clone();
    let _cleanup = listen_order_book(
        r.clone(),
        "BTC-USDC".into(),
        Rc::new(move |b| {
            sink.borrow_mut().push(b);
            Ok(())
        }),
    )
    .unwrap();
    let snapshot =
        json!({"type":"snapshot","updates":[{"side":"bid","price_level":"10","new_quantity":"2"}]});
    call_parser(
        &r,
        "level2.BTC-USD",
        PublicStreamMessage::Frame(Frame::from_json(&snapshot)),
    )
    .unwrap();
    let book = roots.borrow()[0].clone();
    let bids = book.borrow().bids.clone();
    let level = bids.borrow()[0].clone();
    let update =
        json!({"type":"update","updates":[{"side":"bid","price_level":"10","new_quantity":"5"}]});
    call_parser(
        &r,
        "level2.BTC-USD",
        PublicStreamMessage::Frame(Frame::from_json(&update)),
    )
    .unwrap();
    assert!(
        Rc::ptr_eq(&bids, &book.borrow().bids),
        "delta preserves bids array root"
    );
    assert!(
        Rc::ptr_eq(&level, &bids.borrow()[0]),
        "delta preserves original price-level root"
    );
    assert_eq!(
        level.borrow().amount,
        5.0,
        "delta mutates retained native amount"
    );
    assert_eq!(
        level.borrow().total,
        5.0,
        "total mutates retained native level"
    );
    call_parser(
        &r,
        "level2.BTC-USD",
        PublicStreamMessage::Frame(Frame::from_json(&snapshot)),
    )
    .unwrap();
    assert!(
        !Rc::ptr_eq(&bids, &book.borrow().bids),
        "snapshot replaces bids array root"
    );
    assert_eq!(
        bids.borrow()[0].borrow().amount,
        5.0,
        "retained earlier bids array remains unchanged"
    );
    assert_eq!(
        book.borrow().bids.borrow()[0].borrow().amount,
        2.0,
        "new snapshot native amount"
    );
    assert_eq!(roots.borrow().len(), 3, "independent three book callbacks");
    assert!(
        roots.borrow().iter().all(|b| Rc::ptr_eq(b, &book)),
        "all book callbacks retain outer root"
    );
}
#[test]
fn intx_recovery_reads_current_array_length_and_public_adoption_rejects_without_outer_catch() {
    let tasks = Microtasks::default();
    let input = json!({"publicOnly":true,"replies":[{"error":{"message":"original","response":{"data":[{"symbol":"X","trading_state":"TRADING","type":"PERP"}]}}}]});
    let (r, _, _) = read_receiver(&input, &tasks);
    let p = fetch_markets_from_intx_instruments(r.clone());
    r.host.recovery.borrow()[0].1.borrow_mut().clear();
    tasks.drain();
    assert!(
        Rc::ptr_eq(&completed(p).unwrap(), &r.host.cache),
        "emptied INTX recovery returns cached array root"
    );
    assert_eq!(
        &*r.host.emits.borrow(),
        &[Some(json!("original"))],
        "emptied INTX recovery rethrows original error before filter"
    );
    let (r, _, _) = read_receiver(&input, &tasks);
    let failure = Rc::new(PortfolioThrown::from_value(Some(
        json!({"name":"Error","message":"emit threw"}),
    )));
    *r.host.emit_failure.borrow_mut() = Some(failure.clone());
    r.host.recovery.borrow()[0].1.borrow_mut().clear();
    let p = fetch_markets(r.clone());
    assert_eq!(
        r.host.calls.borrow().len(),
        1,
        "public helper starts eagerly before adoption"
    );
    tasks.drain();
    let Err(e) = completed(p) else {
        panic!("helper emitter rejection must escape outer fetchMarkets catch")
    };
    assert!(
        Rc::ptr_eq(&e, &failure),
        "public return helper adopts exact rejected error root"
    );
    assert_eq!(
        r.host.emits.borrow().len(),
        1,
        "public return helper rejection is not caught again by outer fetchMarkets"
    );
}
#[test]
fn data_record_primary_root_and_daily_spread_keep_nested_native_identity() {
    let raw = Frame::from_json(
        &json!({"timestamp":1,"open":2,"high":3,"low":1,"close":2,"volume":4,"marker":{"value":7}}),
    );
    let Frame::Object(object) = raw else {
        panic!("object root")
    };
    let marker = object.borrow()[&"marker".encode_utf16().collect::<Vec<_>>()].clone();
    let Frame::Object(marker) = marker else {
        panic!("nested record")
    };
    let daily = PublicData::Json(object.clone());
    let seen = Rc::new(Cell::new(0usize));
    let count = seen.clone();
    let m = marker.clone();
    let r = Rc::new(PublicListenerReceiver {
        connectivity:
            safe_cex_venue_coinbase::public_market_listeners::PublicListenerConnectivity::Host,
        stream: Rc::new(PublicStreamReceiver::new(ListenerHost {
            connected: Cell::new(true),
            public: true,
            daily: RefCell::new(IndexMap::from([("X".into(), daily)])),
            trace: RefCell::new(vec![]),
            timers: RefCell::new(VecDeque::new()),
        })),
        inner_state: RefCell::new(IndexMap::new()),
    });
    let _cleanup = listen_ohlcv(
        r.clone(),
        opts(&json!({"symbol":"X","interval":"1d"})),
        Rc::new(move |c| {
            let PublicData::Generated(v) = c else {
                panic!("daily spread generated record")
            };
            let v = v.borrow();
            let marker = v[&"marker".encode_utf16().collect::<Vec<_>>()].clone();
            let Some(Frame::Object(marker)) = marker else {
                panic!("spread nested record")
            };
            assert!(
                Rc::ptr_eq(&marker, &m),
                "Data spread retains original nested native record root"
            );
            assert!(
                matches!(
                    v.get(&"close".encode_utf16().collect::<Vec<_>>()),
                    Some(None)
                ),
                "Data spread keeps own undefined close"
            );
            count.set(count.get() + 1);
            Ok(())
        }),
    )
    .unwrap();
    let primary = Frame::from_json(&json!({"volume":4}));
    call_parser(&r, "candles.X", PublicStreamMessage::Frame(primary)).unwrap();
    assert_eq!(seen.get(), 1, "one independent Data spread callback");
    assert!(
        matches!(object.borrow().get(&"close".encode_utf16().collect::<Vec<_>>()),Some(Frame::Number(v)) if *v==2.0),
        "original inert close unchanged"
    );
    assert!(
        r.stream
            .host
            .daily_candle("X")
            .is_some_and(|v| !same_data(&v, &PublicData::Json(object))),
        "daily spread replaces original top-level record"
    );
}

struct ClassParent {
    identity: Frame,
    options: Frame,
    daily: RefCell<IndexMap<String, PublicData>>,
    disposed: Cell<bool>,
    disposed_reads: Cell<usize>,
}
struct ClassHost {
    listener: ListenerHost,
    parent: Rc<ClassParent>,
    socket: PublicSocket,
    socket_calls: RefCell<Vec<ClassSocketCall>>,
    event_order: RefCell<Vec<&'static str>>,
    reset_observer: RefCell<Option<std::rc::Weak<PublicListenerReceiver<ClassHost>>>>,
    expect_reset: Cell<bool>,
}
#[derive(Debug)]
enum ClassSocketCall {
    Create(&'static str),
    Listener(PublicSocketListener),
    Received,
    Stats(String),
    Reconnected,
}
impl ClassHost {
    fn event(&self, kind: &'static str) {
        self.event_order.borrow_mut().push(kind);
    }
}
impl PublicStreamHost for ClassHost {
    fn is_disposed(&self) -> bool {
        self.parent
            .disposed_reads
            .set(self.parent.disposed_reads.get() + 1);
        self.parent.disposed.get()
    }
    fn testnet(&self) -> bool {
        false
    }
    fn reconnecting_enabled(&self) -> bool {
        true
    }
    fn create_websocket(&self, url: &'static str) -> Result<PublicSocket, ListenerFailure> {
        self.event("create");
        self.socket_calls
            .borrow_mut()
            .push(ClassSocketCall::Create(url));
        Ok(self.socket.clone())
    }
    fn add_listener(
        &self,
        socket: PublicSocket,
        event: PublicSocketListener,
    ) -> Result<(), ListenerFailure> {
        assert!(
            Rc::ptr_eq(&socket, &self.socket),
            "class native listener binds original socket"
        );
        self.event("listener");
        self.socket_calls
            .borrow_mut()
            .push(ClassSocketCall::Listener(event));
        Ok(())
    }
    fn reset_orderbook(&self) -> Result<(), ListenerFailure> {
        panic!("class reset must target its owned listener state")
    }
    fn emit_ws_reconnected(&self) -> Result<(), ListenerFailure> {
        self.event("reconnected");
        self.socket_calls
            .borrow_mut()
            .push(ClassSocketCall::Reconnected);
        Ok(())
    }
    fn markets_symbols(&self) -> Result<Vec<String>, ListenerFailure> {
        Ok(vec!["BTC-USD".into()])
    }
    fn send_subscription(&self, value: PublicSubscription) -> Result<(), ListenerFailure> {
        if self.expect_reset.get() {
            let listener = self
                .reset_observer
                .borrow()
                .as_ref()
                .unwrap()
                .upgrade()
                .unwrap();
            for state in listener.inner_state.borrow().values() {
                let state = state.borrow();
                assert!(
                    !state.is_snapshot_loaded,
                    "class native reset happens before subscribe"
                );
                assert!(
                    state.updates.is_empty(),
                    "class native reset clears updates before subscribe"
                );
            }
        }
        self.event("send");
        self.listener.push(ListenerCall::Send(value));
        Ok(())
    }
    fn track_last_received(&self) -> Result<(), ListenerFailure> {
        self.event("received");
        self.socket_calls
            .borrow_mut()
            .push(ClassSocketCall::Received);
        Ok(())
    }
    fn json_parse(&self, raw: &str) -> Option<Frame> {
        serde_json::from_str(raw)
            .ok()
            .as_ref()
            .map(Frame::from_json)
    }
    fn emit_error(&self, _: Option<Frame>) -> Result<(), ListenerFailure> {
        panic!("unexpected class error emitter")
    }
    fn console_error_text(&self, _: Vec<u16>) -> Result<(), ListenerFailure> {
        panic!("unexpected class text error")
    }
    fn console_error_frame(&self, _: Frame) -> Result<(), ListenerFailure> {
        panic!("unexpected class frame error")
    }
    fn handle_pong_event(&self) -> Result<(), ListenerFailure> {
        panic!("unexpected class pong")
    }
    fn record_message_handler_stats(&self, topic: &str) -> Result<(), ListenerFailure> {
        self.event("stats");
        self.socket_calls
            .borrow_mut()
            .push(ClassSocketCall::Stats(topic.into()));
        Ok(())
    }
}
impl PublicMarketListenerHost for ClassHost {
    fn is_connected(&self) -> bool {
        panic!("class connectivity must use the actual receiver socket")
    }
    fn public_data_only_option(&self) -> bool {
        matches!(
            self.parent.options.get("publicDataOnly"),
            Some(Frame::Bool(true))
        )
    }
    fn timeframe_seconds(&self, interval: &str) -> Option<f64> {
        PublicMarketListenerHost::timeframe_seconds(&self.listener, interval)
    }
    fn daily_candle(&self, s: &str) -> Option<PublicData> {
        self.parent.daily.borrow().get(s).cloned()
    }
    fn set_daily_candle(&self, s: String, c: PublicData) -> Result<(), ListenerFailure> {
        self.parent.daily.borrow_mut().insert(s, c);
        Ok(())
    }
    fn utc_end_of_day_ms(&self) -> Result<f64, ListenerFailure> {
        PublicMarketListenerHost::utc_end_of_day_ms(&self.listener)
    }
    fn fetch_ohlcv_detached(&self, opts: PublicOHLCVOptionsRoot) -> Result<(), ListenerFailure> {
        self.event("fetch");
        PublicMarketListenerHost::fetch_ohlcv_detached(&self.listener, opts)
    }
    fn dayjs_unix(&self, value: Option<Frame>) -> Result<f64, ListenerFailure> {
        let Some(Frame::String(text)) = value else {
            panic!("class trade time uses its actual ISO string")
        };
        Ok(
            (safe_cex_num::js_date_parse(&String::from_utf16(&text).unwrap(), |millis| millis)
                / 1000.0)
                .floor(),
        )
    }
    fn add(&self, left: Option<Frame>, right: f64) -> Result<f64, ListenerFailure> {
        PublicMarketListenerHost::add(&self.listener, left, right)
    }
    fn warn(&self, message: &str, value: Option<Frame>) -> Result<(), ListenerFailure> {
        self.event("warn");
        PublicMarketListenerHost::warn(&self.listener, message, value)
    }
    fn set_timeout(&self, callback: PublicCleanup, delay_ms: f64) -> Result<(), ListenerFailure> {
        self.event("timer");
        PublicMarketListenerHost::set_timeout(&self.listener, callback, delay_ms)
    }
    fn cleanup_message_handler_stats(&self, topic: &str) -> Result<(), ListenerFailure> {
        self.event("cleanup");
        PublicMarketListenerHost::cleanup_message_handler_stats(&self.listener, topic)
    }
    fn nanoid(&self) -> Result<String, ListenerFailure> {
        PublicMarketListenerHost::nanoid(&self.listener)
    }
    fn update_order_book(
        &self,
        book: PublicListenerBook,
        side: &str,
        level: Rc<RefCell<PublicBookLevel>>,
    ) -> Result<(), ListenerFailure> {
        PublicMarketListenerHost::update_order_book(&self.listener, book, side, level)
    }
    fn sort_order_book(&self, book: PublicListenerBook) -> Result<(), ListenerFailure> {
        PublicMarketListenerHost::sort_order_book(&self.listener, book)
    }
    fn calc_order_book_total(&self, book: PublicListenerBook) -> Result<(), ListenerFailure> {
        PublicMarketListenerHost::calc_order_book_total(&self.listener, book)
    }
}
#[path = "support/capture_metadata.rs"]
mod class_capture_metadata;
type OwnedPublicClass = safe_cex_venue_coinbase::classes::CoinbasePublicWebsocket<ClassHost>;
fn owned_public_class() -> OwnedPublicClass {
    let identity = Frame::from_json(&json!({"options":{"testnet":false,"publicDataOnly":false}}));
    let options = identity.get("options").unwrap();
    let parent = Rc::new(ClassParent {
        identity: identity.clone(),
        options,
        daily: RefCell::new(IndexMap::new()),
        disposed: Cell::new(false),
        disposed_reads: Cell::new(0),
    });
    let host = ClassHost {
        listener: ListenerHost {
            connected: Cell::new(false),
            public: false,
            daily: RefCell::new(IndexMap::new()),
            trace: RefCell::new(Vec::new()),
            timers: RefCell::new(VecDeque::new()),
        },
        parent,
        socket: Rc::new(RefCell::new(Frame::from_json(&json!({"readyState":0})))),
        socket_calls: RefCell::new(Vec::new()),
        event_order: RefCell::new(Vec::new()),
        reset_observer: RefCell::new(None),
        expect_reset: Cell::new(false),
    };
    let class = OwnedPublicClass::new(host, identity);
    *class.receiver.host.reset_observer.borrow_mut() = Some(Rc::downgrade(&class.listeners));
    assert!(
        Rc::ptr_eq(&class.receiver, &class.listeners.stream),
        "class native listeners share exact dispatcher root"
    );
    match (&class.parent, &class.receiver.host.parent.identity) {
        (Frame::Object(a), Frame::Object(b)) => assert!(
            Rc::ptr_eq(a, b),
            "class native parent capability retains supplied parent identity"
        ),
        _ => panic!("class native parent identity"),
    }
    class
}
fn class_ready(class: &OwnedPublicClass, ready: f64) {
    let socket = class.receiver.ws.borrow().clone().unwrap();
    assert!(
        Rc::ptr_eq(&socket, &class.receiver.host.socket),
        "class native connectivity uses original socket"
    );
    let object = match &*socket.borrow() {
        Frame::Object(object) => object.clone(),
        _ => panic!("class socket object"),
    };
    object
        .borrow_mut()
        .insert("readyState".encode_utf16().collect(), Frame::Number(ready));
}
fn class_options(symbol: &str, interval: &str) -> PublicOHLCVOptionsRoot {
    Rc::new(RefCell::new(PublicOHLCVOptions {
        symbol: symbol.into(),
        interval: interval.into(),
        limit: None,
        from: None,
        to: None,
    }))
}
fn class_candle(class: &OwnedPublicClass, product: &str, close: &str, volume: &str) {
    class.on_message(&json!({"channel":"candles","events":[{"type":"update","candles":[{"product_id":product,"start":"60","open":"1","high":"3","low":"0.5","close":close,"volume":volume}]}]}).to_string()).unwrap();
}
fn class_book(class: &OwnedPublicClass, kind: &str, price: &str, amount: &str) {
    class.on_message(&json!({"channel":"l2_data","events":[{"type":kind,"product_id":"BTC-USD","updates":[{"side":"bid","price_level":price,"new_quantity":amount}]}]}).to_string()).unwrap();
}
fn same_class_data(a: &PublicData, b: &PublicData) -> bool {
    match (a, b) {
        (PublicData::Candle(a), PublicData::Candle(b)) => Rc::ptr_eq(a, b),
        (PublicData::Json(a), PublicData::Json(b)) => Rc::ptr_eq(a, b),
        (PublicData::Generated(a), PublicData::Generated(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
fn class_source() -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/class-public.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "live full public class oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    class_capture_metadata::assert_capture_metadata(&source, 1);
    let mut scenarios = source
        .as_object()
        .unwrap()
        .keys()
        .filter(|key| !key.starts_with("capture"))
        .map(String::as_str)
        .collect::<Vec<_>>();
    scenarios.sort();
    assert_eq!(
        scenarios,
        ["daily", "isolation", "reconnect", "routing"],
        "independent class scenario content pin"
    );
    source
}
#[test]
fn public_class_listeners_dispatch_mutate_reset_and_cleanup_the_owned_native_roots() {
    let source = class_source();
    let mut boundaries = Vec::new();
    {
        let class = owned_public_class();
        let e = &source["routing"];
        let first = Rc::new(RefCell::new(Vec::new()));
        let second = Rc::new(RefCell::new(Vec::new()));
        let captured = first.clone();
        let cleanup1 = class
            .listen_ohlcv(
                class_options("BTC-USDC", "5m"),
                Rc::new(move |value| {
                    captured.borrow_mut().push(value.clone());
                    set_high(&value, 41.0);
                    Ok(())
                }),
            )
            .unwrap();
        assert_eq!(
            class.receiver.host.listener.timers.borrow().len(),
            1,
            "class native disconnected retry floor"
        );
        class.connect_and_subscribe().unwrap();
        let retry = class
            .receiver
            .host
            .listener
            .timers
            .borrow_mut()
            .pop_front()
            .unwrap();
        retry().unwrap();
        assert_eq!(
            class.receiver.host.listener.timers.borrow().len(),
            1,
            "class native actual closed socket retries"
        );
        class_ready(&class, 1.0);
        let retry = class
            .receiver
            .host
            .listener
            .timers
            .borrow_mut()
            .pop_front()
            .unwrap();
        retry().unwrap();
        assert_eq!(
            class.receiver.host.listener.timers.borrow().len(),
            0,
            "class native open socket registers without retry"
        );
        let captured = second.clone();
        let cleanup2 = class
            .listen_ohlcv(
                class_options("BTC-USDC", "5m"),
                Rc::new(move |value| {
                    captured.borrow_mut().push(value);
                    Ok(())
                }),
            )
            .unwrap();
        class_candle(&class, "BTC-USD", "2", "5");
        assert_eq!(
            first.borrow().len(),
            1,
            "class native first candle callback"
        );
        assert_eq!(
            second.borrow().len(),
            1,
            "class native second candle callback"
        );
        assert!(
            same_class_data(&first.borrow()[0], &second.borrow()[0]),
            "class native callbacks share candle root"
        );
        assert_data(
            &second.borrow()[0],
            &e["primary"],
            "class later callback observes native candle mutation",
        );
        class.on_message(&json!({"channel":"market_trades","events":[{"type":"update","trades":[{"product_id":"BTC-USD","price":"4","size":"8","time":"1970-01-01T00:01:01.000Z"}]}]}).to_string()).unwrap();
        assert_eq!(first.borrow().len(), 2, "class native first trade callback");
        assert_eq!(
            second.borrow().len(),
            2,
            "class native second trade callback"
        );
        assert_data(
            &second.borrow()[1],
            &e["trade"],
            "class native trade extends retained candle state",
        );
        cleanup1().unwrap();
        class_candle(&class, "BTC-USD", "2", "5");
        assert_eq!(
            first.borrow().len(),
            2,
            "class native first cleanup stops callback"
        );
        assert_eq!(
            second.borrow().len(),
            3,
            "class native second callback survives first cleanup"
        );
        assert_data(
            &second.borrow()[2],
            &e["final"],
            "class surviving listener receives new native candle",
        );
        cleanup2().unwrap();
        class_candle(&class, "BTC-USD", "2", "5");
        assert_eq!(
            second.borrow().len(),
            3,
            "class native final cleanup stops delivery"
        );
        assert_eq!(
            class.receiver.host.parent.disposed_reads.get(),
            5,
            "class native routing disposed getter floor"
        );
        assert_eq!(
            class.receiver.host.parent.disposed_reads.get() as u64,
            e["disposedReads"].as_u64().unwrap()
        );
        boundaries.push((class.receiver, e.clone()));
    }
    {
        let class = owned_public_class();
        let e = &source["reconnect"];
        class.connect_and_subscribe().unwrap();
        class_ready(&class, 1.0);
        let roots = Rc::new(RefCell::new(Vec::new()));
        let snapshots = Rc::new(RefCell::new(Vec::new()));
        let (retained, captures) = (roots.clone(), snapshots.clone());
        let cleanup = class
            .listen_order_book(
                "BTC-USDC".into(),
                Rc::new(move |book| {
                    let b = book.borrow();
                    let snapshot = [b.bids.clone(), b.asks.clone()].map(|side| {
                        side.borrow()
                            .iter()
                            .map(|level| {
                                let level = level.borrow();
                                [level.price, level.amount, level.total]
                            })
                            .collect::<Vec<_>>()
                    });
                    captures.borrow_mut().push(snapshot);
                    retained.borrow_mut().push(book.clone());
                    b.bids.borrow()[0].borrow_mut().amount = 17.0;
                    Ok(())
                }),
            )
            .unwrap();
        class_book(&class, "snapshot", "10", "2");
        let original = roots.borrow()[0].clone();
        let old_bids = original.borrow().bids.clone();
        let old_level = old_bids.borrow()[0].clone();
        class.listeners.inner_state.borrow()["inner"]
            .borrow_mut()
            .updates
            .push(data(&json!({"sentinel":true})));
        class.receiver.host.expect_reset.set(true);
        class.on_open().unwrap();
        class.receiver.host.expect_reset.set(false);
        assert!(
            !class.listeners.inner_state.borrow()["inner"]
                .borrow()
                .is_snapshot_loaded,
            "class native reconnect resets owned loaded flag"
        );
        assert!(
            class.listeners.inner_state.borrow()["inner"]
                .borrow()
                .updates
                .is_empty(),
            "class native reconnect resets actual pending updates"
        );
        assert!(
            Rc::ptr_eq(&original.borrow().bids, &old_bids),
            "class native reconnect preserves side root"
        );
        assert!(
            Rc::ptr_eq(&old_bids.borrow()[0], &old_level),
            "class native reconnect preserves level root"
        );
        assert_eq!(old_level.borrow().amount.to_bits(), 17_f64.to_bits());
        class_book(&class, "update", "10", "99");
        assert_eq!(
            roots.borrow().len(),
            2,
            "class native pre-snapshot callback floor"
        );
        assert!(
            Rc::ptr_eq(&roots.borrow()[1], &original),
            "class native pre-snapshot book root"
        );
        assert_eq!(
            old_level.borrow().amount.to_bits(),
            17_f64.to_bits(),
            "class native pre-snapshot delta is ignored"
        );
        class_book(&class, "snapshot", "23", "4");
        assert_eq!(
            roots.borrow().len(),
            3,
            "class native new snapshot callback floor"
        );
        assert!(
            Rc::ptr_eq(&roots.borrow()[2], &original),
            "class native new snapshot retains book root"
        );
        assert!(
            !Rc::ptr_eq(&original.borrow().bids, &old_bids),
            "class native fresh snapshot replaces side array"
        );
        assert!(
            Rc::ptr_eq(&old_bids.borrow()[0], &old_level),
            "class native old side survives replacement"
        );
        class.receiver.host.parent.disposed.set(true);
        let calls = class.receiver.host.listener.trace.borrow().len();
        class.on_open().unwrap();
        assert_eq!(
            class.receiver.host.listener.trace.borrow().len(),
            calls,
            "class disposed onOpen does not subscribe"
        );
        assert!(
            class.listeners.inner_state.borrow()["inner"]
                .borrow()
                .is_snapshot_loaded,
            "class disposed onOpen does not reset state"
        );
        cleanup().unwrap();
        class_book(&class, "update", "23", "8");
        assert_eq!(
            roots.borrow().len(),
            3,
            "class native disposed/cleaned callback floor"
        );
        let snapshots = snapshots.borrow();
        assert_eq!(snapshots.len(), 3, "independent class book snapshot floor");
        assert_eq!(
            e["captures"].as_array().unwrap().len(),
            3,
            "independent source book capture floor"
        );
        for (snapshot, e) in snapshots.iter().zip(e["captures"].as_array().unwrap()) {
            for (side, values) in ["bids", "asks"].into_iter().zip(snapshot) {
                assert_eq!(
                    values.len(),
                    e[side].as_array().unwrap().len(),
                    "class native side count"
                );
                for (native, e) in values.iter().zip(e[side].as_array().unwrap()) {
                    for (value, key) in native.iter().zip(["price", "amount", "total"]) {
                        assert_num(*value, &e[key], "class native callback book value");
                    }
                }
            }
        }
        assert_num(
            old_level.borrow().amount,
            &e["oldAmount"],
            "class native retained old level mutation",
        );
        assert_eq!(
            class.receiver.host.parent.disposed_reads.get(),
            7,
            "class native reconnect disposed getter floor"
        );
        assert_eq!(
            class.receiver.host.parent.disposed_reads.get() as u64,
            e["disposedReads"].as_u64().unwrap()
        );
        boundaries.push((class.receiver, e.clone()));
    }
    {
        let class = owned_public_class();
        let e = &source["daily"];
        class.connect_and_subscribe().unwrap();
        class_ready(&class, 1.0);
        let values = Rc::new(RefCell::new(Vec::new()));
        let snapshots = Rc::new(RefCell::new(Vec::new()));
        let (retained, captures) = (values.clone(), snapshots.clone());
        let cleanup = class
            .listen_ohlcv(
                class_options("ETH-USDC", "1d"),
                Rc::new(move |value| {
                    retained.borrow_mut().push(value.clone());
                    let PublicData::Generated(object) = value else {
                        panic!("daily native spread record")
                    };
                    captures.borrow_mut().push(object.borrow().clone());
                    object
                        .borrow_mut()
                        .insert("volume".encode_utf16().collect(), Some(Frame::Number(99.0)));
                    Ok(())
                }),
            )
            .unwrap();
        class_candle(&class, "ETH-USD", "24", "3");
        assert_eq!(
            values.borrow().len(),
            0,
            "class native daily waits for bound parent cache"
        );
        let previous =
            data(&json!({"timestamp":0,"open":10,"high":20,"low":5,"close":10,"volume":11}));
        class
            .receiver
            .host
            .parent
            .daily
            .borrow_mut()
            .insert("ETH-USDC".into(), previous.clone());
        class_candle(&class, "ETH-USD", "24", "3");
        assert_eq!(
            values.borrow().len(),
            1,
            "class native first daily callback floor"
        );
        assert!(
            same_class_data(
                &class.receiver.host.parent.daily.borrow()["ETH-USDC"],
                &values.borrow()[0]
            ),
            "class native parent cache and callback share root"
        );
        assert!(
            !same_class_data(&previous, &values.borrow()[0]),
            "class native daily spread replaces parent record"
        );
        class_candle(&class, "ETH-USD", "25", "4");
        assert_eq!(
            values.borrow().len(),
            2,
            "class native second daily callback floor"
        );
        assert!(
            same_class_data(
                &class.receiver.host.parent.daily.borrow()["ETH-USDC"],
                &values.borrow()[1]
            ),
            "class native later daily cache root"
        );
        assert_eq!(
            snapshots.borrow().len(),
            2,
            "independent native daily snapshot floor"
        );
        assert_eq!(
            e["snapshots"].as_array().unwrap().len(),
            2,
            "independent source daily snapshot floor"
        );
        assert_eq!(
            e["values"].as_array().unwrap().len(),
            2,
            "independent source daily value floor"
        );
        for (native, e) in snapshots
            .borrow()
            .iter()
            .zip(e["snapshots"].as_array().unwrap())
        {
            for (key, value) in e.as_object().unwrap() {
                assert_frame(
                    native[&key.encode_utf16().collect::<Vec<_>>()].clone(),
                    value,
                    "class native daily value before callback mutation",
                );
            }
        }
        for (native, e) in values.borrow().iter().zip(e["values"].as_array().unwrap()) {
            assert_data(native, e, "class native daily callback mutation");
        }
        assert_data(
            &previous,
            &e["previous"],
            "class native previous daily root remains unchanged",
        );
        cleanup().unwrap();
        assert_eq!(
            class.receiver.host.parent.disposed_reads.get(),
            4,
            "class native daily disposed getter floor"
        );
        assert_eq!(
            class.receiver.host.parent.disposed_reads.get() as u64,
            e["disposedReads"].as_u64().unwrap()
        );
        boundaries.push((class.receiver, e.clone()));
    }
    {
        let left = owned_public_class();
        left.connect_and_subscribe().unwrap();
        class_ready(&left, 1.0);
        let first = Rc::new(Cell::new(0));
        let count = first.clone();
        let a = left
            .listen_ohlcv(
                class_options("BTC-USDC", "5m"),
                Rc::new(move |_| {
                    count.set(count.get() + 1);
                    Ok(())
                }),
            )
            .unwrap();
        let right = owned_public_class();
        right.connect_and_subscribe().unwrap();
        class_ready(&right, 1.0);
        let second = Rc::new(Cell::new(0));
        let count = second.clone();
        let b = right
            .listen_ohlcv(
                class_options("BTC-USDC", "5m"),
                Rc::new(move |_| {
                    count.set(count.get() + 1);
                    Ok(())
                }),
            )
            .unwrap();
        assert!(
            !Rc::ptr_eq(&left.receiver, &right.receiver),
            "class native independent dispatcher roots"
        );
        assert!(
            !Rc::ptr_eq(&left.listeners, &right.listeners),
            "class native independent listener roots"
        );
        class_candle(&left, "BTC-USD", "2", "5");
        assert_eq!(first.get(), 1);
        assert_eq!(second.get(), 0, "other class receives no first callback");
        a().unwrap();
        class_candle(&right, "BTC-USD", "2", "5");
        assert_eq!(first.get(), 1);
        assert_eq!(second.get(), 1, "second class survives first cleanup");
        b().unwrap();
        assert_eq!(first.get(), source["isolation"]["first"].as_u64().unwrap());
        assert_eq!(
            second.get(),
            source["isolation"]["second"].as_u64().unwrap()
        );
        assert_eq!(
            left.receiver.host.parent.disposed_reads.get() as u64,
            source["isolation"]["leftDisposedReads"].as_u64().unwrap()
        );
        assert_eq!(
            right.receiver.host.parent.disposed_reads.get() as u64,
            source["isolation"]["rightDisposedReads"].as_u64().unwrap()
        );
    }
    for (receiver, e) in &boundaries {
        if let Some(topics) = e.get("topics") {
            let actual = receiver.topics.borrow();
            assert_eq!(actual.len(), topics.as_object().unwrap().len());
            for (key, value) in topics.as_object().unwrap() {
                assert_num(actual[key], value, "class native topic reference count");
            }
        }
    }
    // All native values, roots and callback effects above precede any wire assertion.
    for (receiver, e) in &boundaries {
        let expected_order = e["trace"]
            .as_array()
            .unwrap()
            .iter()
            .map(|call| call["kind"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            *receiver.host.event_order.borrow(),
            expected_order,
            "class combined boundary event order"
        );
        let expected = e["trace"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|call| {
                matches!(
                    call["kind"].as_str(),
                    Some("send" | "fetch" | "timer" | "warn" | "cleanup")
                )
            })
            .collect::<Vec<_>>();
        let actual = receiver.host.listener.trace.borrow();
        assert_eq!(
            actual.len(),
            expected.len(),
            "class imported boundary call count"
        );
        for (actual, expected) in actual.iter().zip(expected) {
            if !matches!(actual, ListenerCall::Send(_)) {
                assert_listener_call(actual, expected, "class native imported boundary");
            }
        }
        let expected = e["trace"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|call| {
                matches!(
                    call["kind"].as_str(),
                    Some("create" | "listener" | "received" | "stats" | "reconnected")
                )
            })
            .collect::<Vec<_>>();
        let actual = receiver.host.socket_calls.borrow();
        assert_eq!(actual.len(), expected.len(), "class socket call count");
        for (actual, expected) in actual.iter().zip(expected) {
            match actual {
                ClassSocketCall::Create(url) => {
                    assert_eq!(expected["kind"], "create");
                    assert_eq!(*url, expected["url"].as_str().unwrap());
                }
                ClassSocketCall::Listener(event) => {
                    assert_eq!(expected["kind"], "listener");
                    let name = match event {
                        PublicSocketListener::Open => "open",
                        PublicSocketListener::Message => "message",
                        PublicSocketListener::Close => "close",
                    };
                    assert_eq!(name, expected["event"].as_str().unwrap());
                    assert_eq!(expected["bound"], true);
                }
                ClassSocketCall::Received => assert_eq!(expected["kind"], "received"),
                ClassSocketCall::Stats(topic) => {
                    assert_eq!(expected["kind"], "stats");
                    assert_eq!(topic, expected["topic"].as_str().unwrap());
                }
                ClassSocketCall::Reconnected => assert_eq!(expected["kind"], "reconnected"),
            }
        }
    }
    // Subscription payload assertions follow all native state and other call values.
    for (receiver, e) in &boundaries {
        let expected = e["trace"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|call| {
                matches!(
                    call["kind"].as_str(),
                    Some("send" | "fetch" | "timer" | "warn" | "cleanup")
                )
            })
            .collect::<Vec<_>>();
        for (actual, expected) in receiver.host.listener.trace.borrow().iter().zip(expected) {
            if matches!(actual, ListenerCall::Send(_)) {
                assert_listener_call(actual, expected, "class subscription wire boundary");
            }
        }
    }
}
