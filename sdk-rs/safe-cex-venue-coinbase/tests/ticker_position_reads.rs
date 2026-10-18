#[path = "support/retained_info.rs"]
mod retained_info;
#[path = "support/retained_mutation.rs"]
mod retained_mutation;
use safe_cex_types::{MarginType, Market, Position, PositionMode, PositionSide, Ticker};
use safe_cex_venue_coinbase::{
    market_position_reads::*,
    portfolios_balances::{PortfolioFailure, PortfolioThrown},
    starbase_auth::CoinbaseStarbasePromise as Promise,
};
#[path = "support/microtasks.rs"]
mod microtasks;
use microtasks::{Microtasks, completed, is_pending};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{cell::RefCell, collections::VecDeque, process::Command, rc::Rc};
const FIXTURE: &str = include_str!("fixtures/ticker-position-reads.json");
const HASH: &str = "35456220b6a8ca0e71734412ad7171298958bfc7fcb0d32f2aff27dca1e5fb56";
struct Host {
    requests: RefCell<Vec<(&'static str, Option<Value>)>>,
    emits: RefCell<Vec<Vec<Option<Value>>>>,
    replies: RefCell<VecDeque<Promise<Result<ReadReply, PortfolioFailure>>>>,
    errors: Vec<(PortfolioFailure, ReadItems)>,
    response_lists: Vec<ReadItems>,
    trace: RefCell<Vec<Value>>,
    markets: ReadMarkets,
    tickers: ReadTickers,
    positions: ReadPositions,
    supported: Result<Vec<String>, PortfolioFailure>,
    emit_error: Option<PortfolioFailure>,
    hook: RefCell<Option<Rc<dyn Fn(&str)>>>,
}
impl MarketPositionHost for Host {
    fn get(
        &self,
        endpoint: &'static str,
        params: Option<Value>,
    ) -> Promise<Result<ReadReply, PortfolioFailure>> {
        self.requests.borrow_mut().push((endpoint, params.clone()));
        self.trace.borrow_mut().push(json!({"kind":"get","endpoint":endpoint,"params":params,"argc":if params.is_some(){2}else{1}}));
        self.replies
            .borrow_mut()
            .pop_front()
            .expect("independent transport plan")
    }
    fn error_response_items(&self, err: &PortfolioFailure) -> Option<ReadItems> {
        self.errors
            .iter()
            .find(|(v, _)| Rc::ptr_eq(v, err))
            .map(|(_, v)| v.clone())
    }
    fn get_supported_types(&self) -> Promise<Result<Vec<String>, PortfolioFailure>> {
        self.trace.borrow_mut().push(json!({"kind":"supported"}));
        Promise::resolved(self.supported.clone())
    }
    fn markets(
        &self,
    ) -> std::result::Result<
        Rc<RefCell<Vec<Rc<RefCell<safe_cex_types::Market>>>>>,
        Rc<PortfolioThrown>,
    > {
        self.trace.borrow_mut().push(json!({"kind":"markets"}));
        Ok(self.markets.clone())
    }
    fn market_by_id(
        &self,
        id: &str,
    ) -> std::result::Result<Option<Rc<RefCell<safe_cex_types::Market>>>, Rc<PortfolioThrown>> {
        self.trace
            .borrow_mut()
            .push(json!({"kind":"market","id":id}));
        let hook = self.hook.borrow().clone();
        if let Some(hook) = hook {
            hook(id);
        }
        Ok(self
            .markets
            .borrow()
            .iter()
            // last match wins, as the source's lookup does.
            .rfind(|m| m.borrow().id == id)
            .cloned())
    }
    fn cached_tickers(
        &self,
    ) -> std::result::Result<
        Rc<RefCell<Vec<Rc<safe_cex_venue_coinbase::market_position_reads::ReadTicker>>>>,
        Rc<PortfolioThrown>,
    > {
        self.trace
            .borrow_mut()
            .push(json!({"kind":"cachedTickers"}));
        Ok(self.tickers.clone())
    }
    fn cached_positions(
        &self,
    ) -> std::result::Result<
        Rc<RefCell<Vec<Rc<safe_cex_venue_coinbase::market_position_reads::ReadPosition>>>>,
        Rc<PortfolioThrown>,
    > {
        self.trace
            .borrow_mut()
            .push(json!({"kind":"cachedPositions"}));
        Ok(self.positions.clone())
    }
    fn emit_and_log_error(&self, args: &[Option<Value>]) -> Result<(), PortfolioFailure> {
        self.emits.borrow_mut().push(args.to_vec());
        self.trace
            .borrow_mut()
            .push(json!({"kind":"emit","args":args}));
        if let Some(e) = &self.emit_error {
            Err(e.clone())
        } else {
            Ok(())
        }
    }
    fn local_wall_time_to_utc(&self, millis: f64) -> f64 {
        millis
    }
}
fn failure(value: Value) -> PortfolioFailure {
    Rc::new(PortfolioThrown::from_value(Some(value)))
}
fn item(value: &Value) -> ReadItem {
    Rc::new(RefCell::new(value.clone().into()))
}
fn cached_ticker() -> Rc<ReadTicker> {
    Rc::new(ReadTicker {fields:serde_json::from_value::<Ticker>(json!({"id":"cached-ticker","symbol":"CACHED","bid":1,"ask":2,"last":3,"mark":4,"index":5,"percentage":6,"openInterest":7,"fundingRate":8,"fundingTime":9,"volume":10,"quoteVolume":11})).unwrap(),info:None})
}
fn cached_position() -> Rc<ReadPosition> {
    Rc::new(ReadPosition {fields:serde_json::from_value::<Position>(json!({"id":"cached-position","symbol":"CACHED","side":"short","entryPrice":1,"notional":2,"leverage":3,"marginType":"cross","positionMode":"oneway","unrealizedPnl":4,"contracts":5,"liquidationPrice":6})).unwrap(),info:Some(item(&json!({"retained":7})))})
}
fn market(v: &Value) -> ReadMarket {
    let mut full = json!({"base":"BTC","quote":"USD","active":true,"precision":{"amount":0.01,"price":0.1},"limits":{"amount":{"min":0.01,"max":100},"leverage":{"min":1,"max":10}},"isSpot":false});
    full.as_object_mut()
        .unwrap()
        .extend(v.as_object().unwrap().clone());
    Rc::new(RefCell::new(
        serde_json::from_value::<Market>(full).unwrap(),
    ))
}
fn receiver(
    input: &Value,
    tasks: &Microtasks,
) -> (Rc<MarketPositionReceiver<Host>>, Vec<ReadItem>) {
    let mut all_items = vec![];
    let mut response_lists = vec![];
    let mut errors = vec![];
    let replies = input["replies"]
        .as_array()
        .map(|v| {
            v.iter()
                .map(|plan| {
                    let result = if let Some(value) = plan.get("error") {
                        let err = failure(value.clone());
                        let items = value
                            .get("response")
                            .and_then(|v| v.get("data"))
                            .and_then(Value::as_array)
                            .map(|v| v.iter().map(item).collect::<Vec<_>>())
                            .unwrap_or_default();
                        all_items.extend(items.clone());
                        let items = Rc::new(RefCell::new(items));
                        response_lists.push(items.clone());
                        errors.push((err.clone(), items));
                        Err(err)
                    } else {
                        let data = plan["data"].clone();
                        let values = data
                            .as_array()
                            .or_else(|| data.get("products").and_then(Value::as_array))
                            .or_else(|| data.get("positions").and_then(Value::as_array))
                            .unwrap();
                        let items: Vec<_> = values.iter().map(item).collect();
                        all_items.extend(items.clone());
                        let items = Rc::new(RefCell::new(items));
                        response_lists.push(items.clone());
                        Ok(ReadReply { items })
                    };
                    Promise::resolved(result)
                })
                .collect()
        })
        .unwrap_or_default();
    let selected = input
        .get("selected")
        .map(|v| Rc::new(RefCell::new(serde_json::from_value(v.clone()).unwrap())));
    let supported = if let Some(err) = input.get("supportedError") {
        Err(failure(err.clone()))
    } else {
        Ok(input["supported"]
            .as_array()
            .map(|v| v.iter().map(|v| v.as_str().unwrap().into()).collect())
            .unwrap_or_default())
    };
    (
        Rc::new(MarketPositionReceiver {
            host: Host {
                requests: RefCell::new(vec![]),
                emits: RefCell::new(vec![]),
                replies: RefCell::new(replies),
                errors,
                response_lists,
                trace: RefCell::new(vec![]),
                markets: Rc::new(RefCell::new(
                    input["markets"]
                        .as_array()
                        .map(|v| v.iter().map(market).collect())
                        .unwrap_or_default(),
                )),
                tickers: Rc::new(RefCell::new(vec![cached_ticker()])),
                positions: Rc::new(RefCell::new(vec![cached_position()])),
                supported,
                emit_error: input.get("emitError").map(|v| failure(v.clone())),
                hook: RefCell::new(None),
            },
            spawn: tasks.spawner(),
            selected: Rc::new(RefCell::new(selected)),
        }),
        all_items,
    )
}
fn assert_number(actual: f64, e: &Value, label: &str) {
    let bits =
        u64::from_str_radix(e["$bits"].as_str().expect("source numeric expectation"), 16).unwrap();
    if f64::from_bits(bits).is_nan() {
        assert!(actual.is_nan(), "{label}: decoded native NaN");
    } else {
        assert_eq!(
            actual.to_bits(),
            bits,
            "{label}: decoded native number bits"
        );
    }
}
fn assert_json(actual: Option<&Value>, e: &Value, label: &str) {
    if e.get("$undefined") == Some(&Value::Bool(true)) {
        assert!(actual.is_none(), "{label}: native undefined");
        return;
    }
    let actual = actual.unwrap_or_else(|| panic!("{label}: expected native field {e:?}"));
    if e.get("$bits").is_some() {
        assert_number(actual.as_f64().unwrap(), e, label);
    } else {
        match e {
            Value::Array(items) => {
                let a = actual.as_array().unwrap();
                assert_eq!(a.len(), items.len(), "{label}: native array count");
                for (a, e) in a.iter().zip(items) {
                    assert_json(Some(a), e, label);
                }
            }
            Value::Object(fields) => {
                assert_eq!(
                    actual.as_object().unwrap().len(),
                    fields.len(),
                    "{label}: native record fields"
                );
                for (k, e) in fields {
                    assert_json(actual.get(k), e, label);
                }
            }
            _ => assert_eq!(actual, e, "{label}: native value"),
        }
    }
}
fn assert_ticker(v: &ReadTicker, e: &Value, label: &str) {
    let t = &v.fields;
    assert_eq!(t.id, e["id"], "{label}: ticker id");
    assert_eq!(t.symbol, e["symbol"], "{label}: ticker symbol");
    for (name, actual) in [
        ("bid", t.bid),
        ("ask", t.ask),
        ("last", t.last),
        ("mark", t.mark),
        ("index", t.index),
        ("percentage", t.percentage),
        ("openInterest", t.open_interest),
        ("fundingRate", t.funding_rate),
        ("fundingTime", t.funding_time),
        ("volume", t.volume),
        ("quoteVolume", t.quote_volume),
    ] {
        assert_number(actual, &e[name], &format!("{label}.{name}"));
    }
    assert!(
        t.info.is_none() && t.mid_price.is_none() && t.timestamp.is_none(),
        "{label}: absent native optional ticker fields"
    );
    if let Some(info) = &v.info {
        retained_info::assert_info(Some(&info.borrow()), &e["info"], label);
    } else {
        assert!(e.get("info").is_none(), "{label}: absent info");
    }
}
fn assert_position(v: &ReadPosition, e: &Value, label: &str) {
    let p = &v.fields;
    assert_eq!(p.id, e["id"], "{label}: position id");
    assert_eq!(p.symbol, e["symbol"], "{label}: position symbol");
    assert_eq!(
        p.side,
        if e["side"] == "long" {
            PositionSide::Long
        } else {
            PositionSide::Short
        },
        "{label}: position side"
    );
    assert_eq!(
        p.position_mode,
        Some(match e["positionMode"].as_str().unwrap() {
            "oneway" => PositionMode::OneWay,
            "hedged" => PositionMode::Hedged,
            _ => panic!("unknown source mode"),
        }),
        "{label}: decoded position mode"
    );
    assert_eq!(
        p.margin_type,
        Some(match e["marginType"].as_str().unwrap() {
            "cross" => MarginType::Cross,
            "isolated" => MarginType::Isolated,
            _ => panic!("unknown source margin type"),
        }),
        "{label}: decoded margin type"
    );
    for (name, actual) in [
        ("entryPrice", p.entry_price),
        ("notional", p.notional),
        ("leverage", p.leverage),
        ("unrealizedPnl", p.unrealized_pnl),
        ("contracts", p.contracts),
        ("liquidationPrice", p.liquidation_price),
    ] {
        assert_number(actual, &e[name], &format!("{label}.{name}"));
    }
    assert!(
        p.info.is_none()
            && p.last_update.is_none()
            && p.margin_size.is_none()
            && p.realized_pnl.is_none(),
        "{label}: absent native optional position fields"
    );
    if let Some(extra) = p.extra.as_ref() {
        assert_number(
            extra.contracts_native.unwrap(),
            &e["extra"]["contractsNative"],
            label,
        );
        assert!(extra.unrealized_pnl_native.is_none(), "{label}");
    } else {
        assert!(e.get("extra").is_none(), "{label}");
    }
    if let Some(info) = &v.info {
        retained_info::assert_info(Some(&info.borrow()), &e["info"], label);
    } else {
        assert!(e.get("info").is_none(), "{label}");
    }
}
#[test]
fn ticker_position_live_native_corpus() {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE)),
        HASH,
        "independent read corpus content"
    );
    let cases: Vec<Value> = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(cases.len(), 41, "independent native read floor");
    let oracle = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/ticker-position-reads.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        oracle.status.success(),
        "live read oracle: {}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&oracle.stdout).unwrap();
    assert_eq!(expected.len(), 41, "independent source read floor");
    for (input, e) in cases.iter().zip(expected) {
        let label = input["label"].as_str().unwrap();
        let op = input["op"].as_str().unwrap();
        let tasks = Microtasks::default();
        let (r, items) = receiver(input, &tasks);
        let mut failure = None;
        if op.contains("Tickers") {
            let promise = match op {
                "fetchTickers" => fetch_tickers(r.clone()),
                "fetchTickersSpot" => fetch_tickers_spot(r.clone()),
                "fetchTickersCfm" => fetch_tickers_cfm(r.clone()),
                "fetchTickersIntx" => fetch_tickers_intx(r.clone()),
                "_fetchTickersV3" => fetch_tickers_v3(r.clone()),
                _ => panic!("unknown ticker operation"),
            };
            tasks.drain();
            match completed(promise) {
                Ok(v) => {
                    let entries = v.borrow();
                    let expected = e["result"].as_array().unwrap();
                    assert_eq!(
                        entries.len(),
                        expected.len(),
                        "{label}: native ticker count"
                    );
                    for (i, (v, e_item)) in entries.iter().zip(expected).enumerate() {
                        assert_ticker(v, e_item, label);
                        let item = v
                            .info
                            .as_ref()
                            .and_then(|p| items.iter().position(|v| Rc::ptr_eq(v, p)))
                            .map(|i| i as i64)
                            .unwrap_or(-1);
                        assert_eq!(
                            item,
                            e["items"][i].as_i64().unwrap(),
                            "{label}: ticker info original item"
                        );
                        let cached = r
                            .host
                            .tickers
                            .borrow()
                            .iter()
                            .position(|p| Rc::ptr_eq(v, p))
                            .map(|i| i as i64)
                            .unwrap_or(-1);
                        assert_eq!(
                            cached,
                            e["cachedItems"][i].as_i64().unwrap(),
                            "{label}: cached ticker root"
                        );
                    }
                    assert_eq!(
                        Rc::ptr_eq(&v, &r.host.tickers),
                        e["cached"].as_bool().unwrap(),
                        "{label}: cached ticker array identity"
                    );
                }
                Err(err) => failure = Some(err),
            }
        } else {
            let promise = match op {
                "fetchPositions" => fetch_positions(r.clone()),
                "fetchOpenPositions" => fetch_open_positions(r.clone()),
                "fetchOpenPositionsCfm" => fetch_open_positions_cfm(r.clone()),
                "fetchOpenPositionsIntx" => fetch_open_positions_intx(r.clone()),
                _ => panic!("unknown position operation"),
            };
            tasks.drain();
            match completed(promise) {
                Ok(v) => {
                    let entries = v.borrow();
                    let expected = e["result"].as_array().unwrap();
                    assert_eq!(
                        entries.len(),
                        expected.len(),
                        "{label}: native position count"
                    );
                    for (i, (v, e_item)) in entries.iter().zip(expected).enumerate() {
                        assert_position(v, e_item, label);
                        let item = v
                            .info
                            .as_ref()
                            .and_then(|p| items.iter().position(|v| Rc::ptr_eq(v, p)))
                            .map(|i| i as i64)
                            .unwrap_or(-1);
                        assert_eq!(
                            item,
                            e["items"][i].as_i64().unwrap(),
                            "{label}: position info original item"
                        );
                        let cached = r
                            .host
                            .positions
                            .borrow()
                            .iter()
                            .position(|p| Rc::ptr_eq(v, p))
                            .map(|i| i as i64 + 1)
                            .unwrap_or(-1);
                        assert_eq!(
                            cached,
                            e["cachedItems"][i].as_i64().unwrap(),
                            "{label}: cached position root"
                        );
                    }
                    assert_eq!(
                        Rc::ptr_eq(&v, &r.host.positions),
                        e["cached"].as_bool().unwrap(),
                        "{label}: cached position array identity"
                    );
                }
                Err(err) => failure = Some(err),
            }
        }
        if let Some(err) = failure {
            assert_json(err.value.as_ref(), &e["error"], label);
            let transport_identity = r
                .host
                .errors
                .iter()
                .position(|(v, _)| Rc::ptr_eq(v, &err))
                .map(|i| i as i64)
                .unwrap_or(-1);
            assert_eq!(
                transport_identity,
                e["errorIdentity"].as_i64().unwrap(),
                "{label}: original transport error root"
            );
            assert_eq!(
                r.host
                    .supported
                    .as_ref()
                    .err()
                    .is_some_and(|v| Rc::ptr_eq(v, &err)),
                e["thrownIdentity"]["supported"].as_bool().unwrap(),
                "{label}: supported-types error root"
            );
            assert_eq!(
                r.host
                    .emit_error
                    .as_ref()
                    .is_some_and(|v| Rc::ptr_eq(v, &err)),
                e["thrownIdentity"]["emit"].as_bool().unwrap(),
                "{label}: emitted error root"
            );
        } else {
            assert!(e["error"].is_null(), "{label}: expected rejection");
        }
        let trace = r.host.trace.borrow();
        let expected = e["trace"].as_array().unwrap();
        assert_eq!(
            trace.len(),
            expected.len(),
            "{label}: native invocation count"
        );
        let mut emit_index = 0;
        let mut request_index = 0;
        for (actual, e) in trace.iter().zip(expected) {
            if actual["kind"] == "get" {
                let requests = r.host.requests.borrow();
                let (endpoint, params) = &requests[request_index];
                request_index += 1;
                assert_eq!(
                    *endpoint, e["endpoint"],
                    "{label}: decoded native HTTP endpoint"
                );
                assert_json(params.as_ref(), &e["params"], label);
                assert_number(if params.is_some() { 2.0 } else { 1.0 }, &e["argc"], label);
            } else if actual["kind"] == "emit" {
                let emits = r.host.emits.borrow();
                let args = &emits[emit_index];
                emit_index += 1;
                let expected = e["args"].as_array().unwrap();
                assert_eq!(
                    args.len(),
                    expected.len(),
                    "{label}: native error argument count"
                );
                for (a, e) in args.iter().zip(expected) {
                    assert_json(a.as_ref(), e, label);
                }
            } else {
                assert_json(Some(actual), e, label);
            }
        }
    }
}
#[test]
fn concurrent_tickers_wait_all_and_keep_input_order() {
    let tasks = Microtasks::default();
    let input = json!({"supported":["spot","cfm"]});
    let (r, _) = receiver(&input, &tasks);
    let (a, ar) = Promise::pending();
    let (b, br) = Promise::pending();
    r.host.replies.borrow_mut().extend([a, b]);
    let mut promise = fetch_tickers(r.clone());
    tasks.drain();
    assert_eq!(
        r.host
            .trace
            .borrow()
            .iter()
            .filter(|e| e["kind"] == "get")
            .count(),
        2,
        "ticker concurrent launch floor"
    );
    br.resolve(Ok(ReadReply {items:Rc::new(RefCell::new(vec![item(&json!({"product_id":"CFM","price":"2","price_percentage_change_24h":"0","volume_24h":"1"}))]))}));
    tasks.drain();
    assert!(
        is_pending(&mut promise),
        "ticker all waits unfinished first request"
    );
    tasks.enqueue(move || {
        ar.resolve(Ok(ReadReply {items:Rc::new(RefCell::new(vec![item(&json!({"product_id":"SPOT","price":"1","price_percentage_change_24h":"0","volume_24h":"1"}))]))}))
    });
    tasks.drain();
    let result = completed(promise).unwrap();
    assert_eq!(
        result
            .borrow()
            .iter()
            .map(|t| t.fields.id.as_str())
            .collect::<Vec<_>>(),
        vec!["SPOT", "CFM"],
        "ticker reverse completion preserves input order"
    );
}

#[test]
fn cfm_lookup_callback_reloads_product_id_and_retains_info() {
    let tasks = Microtasks::default();
    let input = json!({"markets":[{"id":"A","symbol":"A","isExpiring":true,"contractSize":0.01}],"replies":[{"data":{"positions":[{"product_id":"A","side":"LONG","avg_entry_price":"10","current_price":"12","number_of_contracts":"2","unrealized_pnl":"3"}]}}]});
    let (r, items) = receiver(&input, &tasks);
    let source = items[0].clone();
    *r.host.hook.borrow_mut() = Some(Rc::new(move |id| {
        assert_eq!(id, "A", "lookup reads original product ID");
        retained_mutation::set(&mut source.borrow_mut(), "product_id", json!("B").into());
    }));
    let promise = fetch_open_positions_cfm(r.clone());
    tasks.drain();
    let positions = completed(promise).unwrap();
    let p = positions.borrow()[0].clone();
    assert_eq!(
        p.fields.id, "B",
        "CFM rereads ID after market lookup callback"
    );
    assert_eq!(
        p.fields.symbol, "B",
        "CFM rereads symbol after market lookup callback"
    );
    assert!(
        Rc::ptr_eq(p.info.as_ref().unwrap(), &items[0]),
        "CFM callback info retains original root"
    );
    r.host.markets.borrow_mut()[0].borrow_mut().is_expiring = Some(false);
    retained_mutation::set(&mut items[0].borrow_mut(), "product_id", json!("A").into());
    r.host
        .replies
        .borrow_mut()
        .push_back(Promise::resolved(Ok(ReadReply {
            items: Rc::new(RefCell::new(items.clone())),
        })));
    let promise = fetch_open_positions_cfm(r.clone());
    tasks.drain();
    assert!(
        completed(promise).unwrap().borrow().is_empty(),
        "invalid CFM contract skips item"
    );
    assert_eq!(
        r.host.trace.borrow().last().unwrap()["args"][0],
        "Coinbase CFM position skipped: missing or invalid contract size for B",
        "CFM diagnostic rereads ID after market lookup callback"
    );
}

#[test]
fn cfm_iterates_items_appended_during_market_lookup() {
    let tasks = Microtasks::default();
    let input = json!({"markets":[{"id":"A","symbol":"A","isExpiring":true,"contractSize":0.01},{"id":"C","symbol":"C","isExpiring":true,"contractSize":0.01}],"replies":[{"data":{"positions":[{"product_id":"A","side":"LONG","avg_entry_price":"10","current_price":"12","number_of_contracts":"2","unrealized_pnl":"3"}]}}]});
    let (r, items) = receiver(&input, &tasks);
    let list = r.host.response_lists[0].clone();
    let appended = item(
        &json!({"product_id":"C","side":"SHORT","avg_entry_price":"11","current_price":"13","number_of_contracts":"3","unrealized_pnl":"4"}),
    );
    let new_item = appended.clone();
    *r.host.hook.borrow_mut() = Some(Rc::new(move |id| {
        if id == "A" {
            list.borrow_mut().push(new_item.clone());
        }
    }));
    let promise = fetch_open_positions_cfm(r.clone());
    tasks.drain();
    let result = completed(promise).unwrap();
    let result = result.borrow();
    assert_eq!(
        result
            .iter()
            .map(|p| p.fields.id.as_str())
            .collect::<Vec<_>>(),
        vec!["A", "C"],
        "CFM live for-of sees appended response item"
    );
    assert!(
        Rc::ptr_eq(result[0].info.as_ref().unwrap(), &items[0]),
        "initial response item identity"
    );
    assert!(
        Rc::ptr_eq(result[1].info.as_ref().unwrap(), &appended),
        "appended response item identity"
    );
}
