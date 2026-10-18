#[path = "support/retained_assert.rs"]
mod retained_assert;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_num::numeric_ingress::js_from_value;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_transport::{
    RestAdapter, RestFailure, RestFuture, RestRequest, RestResponse, RestResponseBody,
};
use safe_cex_types::{
    Balance, ClosedPnlEntry, ClosedPnlEntryContracts, ClosedPnlEntryEntryPrice,
    ClosedPnlEntryExitPrice, ClosedPnlEntryFees, ClosedPnlEntryLeverage, ClosedPnlEntryNotional,
    ClosedPnlEntryUnrealizedPnl, DataProvenance, HistoricalTradeOptions, IncomeCategory,
    IncomeHistoryOptions, IncomeRecord, MarginType, Position, PositionCloseReason, PositionMode,
    PositionProvenance, PositionRecord, PositionRecordLeg, PositionRecordState, PositionSide,
    PositionSourceAxis,
};
use safe_cex_venue_blofin::modes::AccountModes;
use safe_cex_venue_blofin::{
    order_services::{ServiceError, ServiceHost},
    position_reads::*,
    utility::UtilityInput,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, VecDeque},
    future::Future,
    process::Command,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
fn corpus() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/position_reads_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "actual_source_oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let v: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            v["cases"].as_array().unwrap().len(),
            56,
            "independent_case_floor"
        );
        assert_eq!(v["stoppedAfter"], 56);
        let arch = match std::env::consts::ARCH {
            "aarch64" => "arm64",
            "x86_64" => "x64",
            v => v,
        };
        assert_eq!(v["header"]["architecture"], arch);
        eprintln!("position-read source header: {}", v["header"]);
        v
    })
}
fn ready<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    for _ in 0..1000 {
        if let Poll::Ready(v) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return v;
        }
    }
    panic!("pending fixture iteration floor1000 exhausted")
}
fn error(e: ServiceError<String>) -> String {
    match e {
        ServiceError::Input(v) | ServiceError::Host(v) => v,
        ServiceError::Rest(v) => v.message,
        ServiceError::Ingress(v) => v.to_string(),
        ServiceError::Coercion(v) => v.message.into(),
        ServiceError::Store(v) => v.to_string(),
    }
}
fn cleaned(v: &Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .filter(|(_, v)| v.get("__undefined") != Some(&Value::Bool(true)))
                .map(|(k, v)| (k.clone(), cleaned(v)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(cleaned).collect()),
        v => v.clone(),
    }
}
fn input(case: &Value) -> Value {
    cleaned(&serde_json::from_str::<Value>(case["inputRaw"].as_str().unwrap()).unwrap())
}
fn wire_values(actual: &Value, expected: &Value) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(e)) => assert_eq!(
            a.as_f64().unwrap().to_bits(),
            e.as_f64().unwrap().to_bits(),
            "wire numeric bits"
        ),
        (Value::Array(a), Value::Array(e)) => {
            assert_eq!(a.len(), e.len(), "wire independent count");
            for (a, e) in a.iter().zip(e) {
                wire_values(a, e)
            }
        }
        (Value::Object(a), Value::Object(e)) => {
            assert_eq!(a.len(), e.len(), "wire own property count");
            for (k, e) in e {
                wire_values(a.get(k).expect("wire field present"), e)
            }
        }
        _ => assert_eq!(actual, expected, "wire field value"),
    }
}
fn store(input: &Value) -> DefaultStore {
    let store = DefaultStore::new();
    let markets=input.get("markets").cloned().unwrap_or_else(||json!([{"id":"BTC-USDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":0.01,"price":0.25},"limits":{"amount":{"min":0.01,"max":100},"leverage":{"min":1,"max":50}},"contractSize":0.01,"info":{"contractValue":"0.01"}}]));
    Store::update(
        &store,
        StoreUpdate {
            markets: Some(js_from_value(markets).unwrap()),
            positions: Some(
                js_from_value(input.get("stored").cloned().unwrap_or_else(|| json!([]))).unwrap(),
            ),
            balance: input
                .get("balance")
                .map(|v| js_from_value(v.clone()).unwrap()),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    store
}
struct Adapter<'a> {
    queues: RefCell<BTreeMap<String, VecDeque<Value>>>,
    trace: &'a RefCell<Vec<Value>>,
    variant: u8,
    auth: &'a Cell<f64>,
    disposed: &'a Cell<bool>,
    store: &'a DefaultStore,
}
impl RestAdapter for Adapter<'_> {
    fn execute(&self, req: RestRequest) -> RestFuture<'_, RestResponse> {
        assert_eq!(req.method, "GET", "native request method");
        assert!(req.body.is_none(), "native request body absence");
        if let Some(params) = &req.params {
            assert!(params.is_object(), "native own query object")
        }
        self.trace.borrow_mut().push(json!([
            "get",
            req.url,
            req.params
                .clone()
                .unwrap_or_else(|| json!({"__undefined":true}))
        ]));
        let value = self
            .queues
            .borrow_mut()
            .get_mut(&req.url)
            .expect("independent endpoint queue")
            .pop_front()
            .expect("independent response queue");
        let mut yielded = false;
        Box::pin(std::future::poll_fn(move |cx| {
            if self.variant < 3 && !yielded {
                yielded = true;
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            if let Some(markets) = value.get("replaceMarkets") {
                Store::update(
                    self.store,
                    StoreUpdate {
                        markets: Some(js_from_value(markets.clone()).unwrap()),
                        ..Default::default()
                    },
                    false,
                )
                .unwrap();
            }
            if let Some(auth) = value.get("setAuth") {
                self.auth.set(auth.as_f64().unwrap())
            }
            if let Some(disposed) = value.get("setDisposed") {
                self.disposed.set(disposed.as_bool().unwrap())
            }
            if let Some(message) = value.get("throw").and_then(Value::as_str) {
                let mut err = RestFailure::pre_transmit(message.into(), req.clone());
                if let Some(hint) = value.get("hint") {
                    err.response = Some(Box::new(RestResponse {
                        status: 503,
                        reason: None,
                        headers: Default::default(),
                        body: RestResponseBody::Untransformed(
                            safe_cex_types::retained_json::RetainedJsonInput::from_value(
                                json!({"msg":hint}),
                            )
                            .unwrap(),
                        ),
                    }));
                }
                return Poll::Ready(Err(err));
            }
            Poll::Ready(Ok(RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: match self.variant % 3 {
                    0 => RestResponseBody::Encoded(serde_json::to_vec(&value).unwrap()),
                    1 => RestResponseBody::Untransformed(
                        safe_cex_types::retained_json::RetainedJsonInput::from_value(value.clone())
                            .unwrap(),
                    ),
                    _ => RestResponseBody::Transformed(
                        safe_cex_types::retained_json::RetainedJsonInput::from_value(value.clone())
                            .unwrap(),
                    ),
                },
            }))
        }))
    }
}
fn optional(v: Option<&Value>) -> Option<&Value> {
    v.filter(|v| v.get("__undefined").is_none())
}
fn text(v: Option<&Value>) -> Option<&str> {
    optional(v).map(|v| v.as_str().unwrap())
}
fn numeric(v: &Value) -> f64 {
    if let Some(n) = v.as_f64() {
        return n;
    }
    match v["__nativeNumber"].as_str().unwrap() {
        "NaN" => f64::NAN,
        "Infinity" => f64::INFINITY,
        "-Infinity" => f64::NEG_INFINITY,
        "-0" => -0.0,
        _ => panic!("independent numeric profile"),
    }
}
fn number(actual: f64, expected: &Value) {
    let expected = numeric(expected);
    if expected.is_nan() {
        assert!(actual.is_nan(), "native NaN")
    } else {
        assert_eq!(actual.to_bits(), expected.to_bits(), "native numeric bits")
    }
}
fn opt_number(actual: Option<f64>, expected: Option<&Value>) {
    match (actual, optional(expected)) {
        (Some(a), Some(e)) => number(a, e),
        (None, None) => (),
        _ => panic!("native number presence"),
    }
}
fn side(v: &Value) -> PositionSide {
    match v.as_str().unwrap() {
        "long" => PositionSide::Long,
        "short" => PositionSide::Short,
        _ => panic!("independent side floor"),
    }
}
fn mode(v: Option<&Value>) -> Option<PositionMode> {
    text(v).map(|v| match v {
        "oneway" => PositionMode::OneWay,
        "hedged" => PositionMode::Hedged,
        _ => panic!("independent mode floor"),
    })
}
fn margin(v: Option<&Value>) -> Option<MarginType> {
    text(v).map(|v| match v {
        "cross" => MarginType::Cross,
        "isolated" => MarginType::Isolated,
        _ => panic!("independent margin floor"),
    })
}
fn native_json(actual: &Value, expected: &Value) {
    match (actual, expected) {
        (Value::Number(_), Value::Number(_)) => number(actual.as_f64().unwrap(), expected),
        (Value::Array(a), Value::Array(e)) => {
            assert_eq!(a.len(), e.len(), "native JSON array count");
            for (a, e) in a.iter().zip(e) {
                native_json(a, e)
            }
        }
        (Value::Object(a), Value::Object(e)) => {
            assert_eq!(a.len(), e.len(), "native JSON own field count");
            for (k, e) in e {
                native_json(a.get(k).expect("native JSON own field"), e)
            }
        }
        _ => assert_eq!(actual, expected, "native retained JSON value"),
    }
}
fn opt_json(
    actual: Option<&safe_cex_types::retained_json::RetainedJsonInput>,
    expected: Option<&Value>,
) {
    match (actual, optional(expected)) {
        (Some(a), Some(e)) => retained_assert::assert_value(a, e),
        (None, None) => (),
        _ => panic!("native JSON presence"),
    }
}
fn native_positions(actual: &[Position], expected: &Value) {
    let expected = expected.as_array().unwrap();
    assert_eq!(
        actual.len(),
        expected.len(),
        "independent native position count"
    );
    for (p, e) in actual.iter().zip(expected) {
        assert_eq!(p.id, e["id"].as_str().unwrap(), "native position id");
        assert_eq!(p.symbol, e["symbol"].as_str().unwrap());
        assert_eq!(p.side, side(&e["side"]));
        assert_eq!(p.margin_type, margin(e.get("marginType")));
        assert_eq!(p.position_mode, mode(e.get("positionMode")));
        for (a, key) in [
            (p.entry_price, "entryPrice"),
            (p.notional, "notional"),
            (p.leverage, "leverage"),
            (p.unrealized_pnl, "unrealizedPnl"),
            (p.contracts, "contracts"),
            (p.liquidation_price, "liquidationPrice"),
        ] {
            number(a, &e[key])
        }
        opt_number((p.realized_pnl).into_value(), e.get("realizedPnl"));
        opt_number((p.margin_size).into_value(), e.get("marginSize"));
        opt_number((p.last_update).into_value(), e.get("lastUpdate"));
        opt_json(p.info.as_ref(), e.get("info"));
        match (p.extra.as_ref(), optional(e.get("extra"))) {
            (Some(a), Some(e)) => {
                opt_number(a.contracts_native, e.get("contractsNative"));
                opt_number(a.unrealized_pnl_native, e.get("unrealizedPnlNative"));
            }
            (None, None) => (),
            _ => panic!("native position extra presence"),
        }
    }
}
fn native_balance(b: &Balance, e: &Value) {
    for (a, key) in [
        (b.used, "used"),
        (b.free, "free"),
        (b.total, "total"),
        (b.upnl, "upnl"),
    ] {
        number(a, &e[key])
    }
    opt_json(b.info.as_ref(), e.get("info"));
    assert!(b.currencies.is_none(), "native currencies absence");
    assert!(b.coins.is_none(), "native coins absence");
}
fn native_income(actual: &[IncomeRecord], expected: &Value) {
    let expected = expected.as_array().unwrap();
    assert_eq!(
        actual.len(),
        expected.len(),
        "independent native income count"
    );
    for (r, e) in actual.iter().zip(expected) {
        assert_eq!(r.symbol, e["symbol"].as_str().unwrap());
        assert_eq!(r.income_type, e["incomeType"].as_str().unwrap());
        assert_eq!(r.asset, e["asset"].as_str().unwrap());
        assert_eq!(r.tran_id, e["tranId"].as_str().unwrap());
        assert_eq!(r.trade_id.as_deref(), text(e.get("tradeId")));
        number(r.income, &e["income"]);
        number(r.timestamp, &e["timestamp"]);
        assert_eq!(
            r.category,
            Some(match e["category"].as_str().unwrap() {
                "realized_pnl" => IncomeCategory::RealizedPnl,
                "commission" => IncomeCategory::Commission,
                "rebate" => IncomeCategory::Rebate,
                _ => panic!("independent category floor"),
            })
        );
    }
}
fn native_pnl(actual: &[ClosedPnlEntry], expected: &Value) {
    let expected = expected.as_array().unwrap();
    assert_eq!(
        actual.len(),
        expected.len(),
        "independent native closed count"
    );
    for (r, e) in actual.iter().zip(expected) {
        assert_eq!(e["provenance"], "venue", "source provenance floor");
        assert_eq!(r.provenance, DataProvenance::Venue);
        assert_eq!(r.symbol, e["symbol"].as_str().unwrap());
        assert_eq!(r.account, e["account"].as_str().unwrap());
        assert_eq!(r.side, e["side"].as_str().unwrap());
        number(r.time, &e["time"]);
        number(r.closed_time, &e["closedTime"]);
        let ClosedPnlEntryNotional::Variant1(n) = r.notional else {
            panic!("native numeric notional")
        };
        number(n, &e["notional"]);
        let ClosedPnlEntryContracts::Variant1(n) = r.contracts else {
            panic!("native numeric contracts")
        };
        number(n, &e["contracts"]);
        let ClosedPnlEntryEntryPrice::Variant1(n) = r.entry_price else {
            panic!("native numeric entry")
        };
        number(n, &e["entryPrice"]);
        let ClosedPnlEntryExitPrice::Variant1(n) = r.exit_price else {
            panic!("native numeric exit")
        };
        number(n, &e["exitPrice"]);
        let ClosedPnlEntryUnrealizedPnl::Variant1(n) = r.unrealized_pnl else {
            panic!("native numeric pnl")
        };
        number(n, &e["unrealizedPnl"]);
        let ClosedPnlEntryFees::Variant1(n) = r.fees else {
            panic!("native numeric fees")
        };
        number(n, &e["fees"]);
        let ClosedPnlEntryLeverage::Variant0(n) = r.leverage else {
            panic!("native numeric leverage")
        };
        number(n, &e["leverage"]);
    }
}
fn native_records(actual: &[PositionRecord], expected: &Value) {
    let expected = expected.as_array().unwrap();
    assert_eq!(
        actual.len(),
        expected.len(),
        "independent native record count"
    );
    for (r, e) in actual.iter().zip(expected) {
        for (a, key) in [
            (&r.key, "key"),
            (&r.account, "account"),
            (&r.symbol, "symbol"),
            (&r.settle_currency, "settleCurrency"),
        ] {
            assert_eq!(a, e[key].as_str().unwrap())
        }
        assert_eq!(r.side, side(&e["side"]));
        assert_eq!(
            r.position_side,
            match e["positionSide"].as_str().unwrap() {
                "long" => PositionRecordLeg::Long,
                "short" => PositionRecordLeg::Short,
                _ => panic!("independent leg floor"),
            }
        );
        assert_eq!(e["state"], "closed", "source state floor");
        assert_eq!(r.state, PositionRecordState::Closed);
        for (a, key) in [
            (r.closed_size, "closedSize"),
            (r.avg_entry_price, "avgEntryPrice"),
            (r.realized_pnl, "realizedPnl"),
            (r.fees, "fees"),
            (r.funding, "funding"),
            (r.close_count, "closeCount"),
        ] {
            number(a, &e[key])
        }
        for (a, key) in [
            (r.opened_at, "openedAt"),
            (r.closed_at, "closedAt"),
            (r.max_size, "maxSize"),
            (r.leverage, "leverage"),
            (r.closed_notional, "closedNotional"),
            (r.avg_exit_price, "avgExitPrice"),
        ] {
            opt_number(a, e.get(key))
        }
        assert_eq!(r.venue_id.as_deref(), text(e.get("venueId")));
        assert_eq!(
            r.funding_attributed,
            e["fundingAttributed"].as_bool().unwrap()
        );
        assert_eq!(
            r.close_reason,
            Some(match e["closeReason"].as_str().unwrap() {
                "user" => PositionCloseReason::User,
                "liquidation" => PositionCloseReason::Liquidation,
                _ => panic!("independent reason floor"),
            })
        );
        assert_eq!(e["provenance"], "reported", "source reported floor");
        assert_eq!(e["sourceAxis"], "closed-positions", "source axis floor");
        assert_eq!(r.provenance, PositionProvenance::Reported);
        assert_eq!(r.source_axis, PositionSourceAxis::ClosedPositions);
    }
}
fn income_opts(v: &Value) -> IncomeHistoryOptions {
    IncomeHistoryOptions {
        symbol: text(v.get("symbol")).map(str::to_owned),
        income_type: text(v.get("incomeType")).map(str::to_owned),
        start_time: optional(v.get("startTime")).map(numeric),
        end_time: optional(v.get("endTime")).map(numeric),
        limit: optional(v.get("limit")).map(numeric),
    }
}
fn historical_opts(v: &Value) -> HistoricalTradeOptions {
    HistoricalTradeOptions {
        symbol: text(v.get("symbol")).unwrap_or("").into(),
        order_id: None,
        from: optional(v.get("from")).map(numeric),
        to: optional(v.get("to")).map(numeric),
        limit: None,
        from_id: None,
        account_name: text(v.get("accountName")).map(str::to_owned),
    }
}
enum Output {
    Positions(Vec<Position>),
    Metadata(Vec<PositionMetadata>),
    Balance(Box<BalanceAndPositions>),
    Income(Vec<IncomeRecord>),
    Rows(Vec<UtilityInput>),
    Records(Vec<PositionRecord>),
    Pnl(Vec<ClosedPnlEntry>),
}
fn run(case: &Value, variant: u8, boundary: bool) {
    let input = input(case);
    let store = store(&input);
    let trace = RefCell::new(Vec::new());
    let auth = Cell::new(input.get("authFailureStreak").map_or(0.0, numeric));
    let disposed = Cell::new(input["disposed"] == true);
    let modes = AccountModes {
        margin_type: Cell::new(margin(input.get("marginMode"))),
        position_mode: Cell::new(mode(input.get("positionMode"))),
    };
    let service: ServiceHost<'_, String> = ServiceHost {
        emit_and_log_error: Box::new(|v| {
            trace.borrow_mut().push(json!([
                "emit",
                v.cloned().unwrap_or_else(|| json!({"__undefined":true}))
            ]));
            Ok(())
        }),
        log: Box::new(|text, severity| {
            trace
                .borrow_mut()
                .push(json!(["log", text, severity.as_str()]));
            Ok(())
        }),
    };
    let host = PositionReadHost {
        service: &service,
        modes: &modes,
        auth_failure_streak: &auth,
        disposed: &disposed,
        console_warn: Box::new(|message| {
            trace.borrow_mut().push(json!(["consoleWarn", message]));
            Ok(())
        }),
        emit_error_with_detail: Box::new(|e, detail| {
            let message = match e {
                ServiceError::Rest(e) => {
                    assert_eq!(e.message, "second batch", "native original error message");
                    let Some(response) = &e.response else {
                        panic!("native original error response")
                    };
                    let RestResponseBody::Untransformed(value) = &response.body else {
                        panic!("native original response phase")
                    };
                    assert_eq!(
                        value.clone().into_value().unwrap()["msg"],
                        "batch hint",
                        "native original error hint"
                    );
                    e.message.clone()
                }
                ServiceError::Input(v) => {
                    assert!(boundary, "unexpected native metadata refusal");
                    assert_eq!(v, "Metadata symbol is outside the native string model");
                    v.clone()
                }
                _ => panic!("native original error kind"),
            };
            trace.borrow_mut().push(json!(["emit",{"errorMessage":message},detail.cloned().unwrap_or_else(||json!({"__undefined":true}))]));
            Ok(())
        }),
    };
    let adapter = Adapter {
        queues: RefCell::new(
            input["queues"]
                .as_object()
                .unwrap_or(&serde_json::Map::new())
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        value.as_array().unwrap().iter().cloned().collect(),
                    )
                })
                .collect(),
        ),
        trace: &trace,
        variant,
        auth: &auth,
        disposed: &disposed,
        store: &store,
    };
    let args = &input["args"];
    let name = case["name"].as_str().unwrap();
    let public = input["publicOnly"] == true;
    let result = match name {
        "fetchPositionsHelper" => ready(fetch_positions_helper(
            text(args.get(0)),
            &store,
            &adapter,
            &host,
        ))
        .map(Output::Positions),
        "fetchPositions" => {
            let symbols = args.get(0).map(|v| {
                v.as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().to_owned())
                    .collect::<Vec<_>>()
            });
            ready(fetch_positions(symbols.as_deref(), &store, &adapter, &host))
                .map(Output::Positions)
        }
        "fetchPositionMetadata" => {
            let symbols = args[0]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<Vec<_>>();
            ready(fetch_position_metadata(&symbols, &store, &adapter, &host)).map(Output::Metadata)
        }
        "fetchBalanceAndPositions" => ready(fetch_balance_and_positions(&store, &adapter, &host))
            .map(|v| Output::Balance(Box::new(v))),
        "fetchIncomeHistory" => {
            let opts = args.get(0).map(income_opts);
            ready(fetch_income_history(
                opts.as_ref(),
                public,
                &store,
                &adapter,
                &host,
            ))
            .map(Output::Income)
        }
        "fetchIncomeHistoryRange" => ready(fetch_income_history_range(
            numeric(&args[0]),
            numeric(&args[1]),
            text(args.get(2)),
            public,
            &store,
            &adapter,
            &host,
        ))
        .map(Output::Income),
        "fetchClosedPositionRows" => ready(fetch_closed_position_rows(
            &historical_opts(&args[0]),
            public,
            &store,
            &adapter,
        ))
        .map(Output::Rows),
        "fetchHistoricalPositions" => ready(fetch_historical_positions(
            &HistoricalPositionsOptions {
                account: args[0]["account"].as_str().unwrap().into(),
                from: optional(args[0].get("from")).map(numeric),
                to: optional(args[0].get("to")).map(numeric),
            },
            public,
            &store,
            &adapter,
        ))
        .map(Output::Records),
        "fetchClosedPnl" => ready(fetch_closed_pnl(
            &historical_opts(&args[0]),
            public,
            &store,
            &adapter,
            &host,
        ))
        .map(Output::Pnl),
        _ => panic!("outside method floor"),
    };
    if boundary {
        match result.unwrap() {
            Output::Income(v) => assert!(v.is_empty(), "native nonfinite income query refusal"),
            Output::Pnl(v) => assert!(v.is_empty(), "native nonfinite pnl query refusal"),
            Output::Metadata(v) => assert!(v.is_empty(), "native symbol representation refusal"),
            Output::Balance(v) => {
                native_balance(&v.balance, &case["balance"]);
                native_positions(&v.positions, &case["positions"])
            }
            _ => panic!("outside boundary floor"),
        };
        let trace = trace.borrow();
        assert_eq!(
            trace.last().unwrap()[0],
            "emit",
            "native explicit boundary diagnostic"
        );
        if name == "fetchIncomeHistory" || name == "fetchClosedPnl" {
            assert_eq!(trace.len(), 1, "native refusal before request");
            assert_eq!(
                trace[0][1],
                "Nonfinite history query time is outside native JSON"
            )
        }
        return;
    }
    if let Some(e) = case["result"].get("error") {
        assert_eq!(
            result.map_err(error).err().unwrap(),
            e.as_str().unwrap(),
            "native propagated error"
        )
    } else {
        let expected = &case["result"]["value"];
        match result.map_err(error).unwrap() {
            Output::Positions(mut v) => {
                native_positions(&v, expected);
                if case["aliases"]["firstPosition"] == true {
                    let before = store.positions()[0].leverage;
                    v[0].leverage += 1000.0;
                    assert_eq!(
                        store.positions()[0].leverage,
                        before,
                        "native owned position snapshot"
                    );
                    if let Some(safe_cex_types::retained_json::RetainedJsonInput::Object(info)) =
                        v[0].info.as_mut()
                    {
                        info.push((
                            "__native_probe".encode_utf16().collect(),
                            json!(true).into(),
                        ));
                        assert!(
                            store.positions()[0]
                                .info
                                .as_ref()
                                .unwrap()
                                .get("__native_probe")
                                .is_none(),
                            "native owned nested info"
                        );
                    }
                }
            }
            Output::Income(v) => native_income(&v, expected),
            Output::Pnl(v) => native_pnl(&v, expected),
            Output::Records(v) => native_records(&v, expected),
            Output::Rows(v) => {
                let expected = expected.as_array().unwrap();
                assert_eq!(v.len(), expected.len(), "native retained row count");
                for (a, e) in v.iter().zip(expected) {
                    native_json(a.json(), e)
                }
            }
            Output::Balance(mut v) => {
                native_balance(&v.balance, &expected["balance"]);
                native_positions(&v.positions, &expected["positions"]);
                if case["aliases"]["balance"] == true {
                    let before = store.balance().free;
                    v.balance.free += 1000.0;
                    assert_eq!(
                        store.balance().free,
                        before,
                        "native owned balance snapshot"
                    );
                }
            }
            Output::Metadata(v) => {
                let expected = expected.as_array().unwrap();
                assert_eq!(v.len(), expected.len(), "native metadata count");
                for (p, e) in v.iter().zip(expected) {
                    assert_eq!(p.symbol, e["symbol"].as_str().unwrap());
                    assert_eq!(p.margin_type, margin(e.get("marginType")).unwrap());
                    assert_eq!(p.position_mode, mode(e.get("positionMode")).unwrap());
                    assert_eq!(p.side, side(&e["side"]));
                    number(p.leverage, &e["leverage"])
                }
            }
        }
    }
    let native_markets = store.markets();
    let expected_markets = case["markets"].as_array().unwrap();
    assert_eq!(
        native_markets.len(),
        expected_markets.len(),
        "independent native market state count"
    );
    for (m, e) in native_markets.iter().zip(expected_markets) {
        assert_eq!(
            m.id,
            e["id"].as_str().unwrap(),
            "native reentrant market ID"
        );
        assert_eq!(m.symbol, e["symbol"].as_str().unwrap());
        assert_eq!(
            m.quote,
            e["quote"].as_str().unwrap(),
            "native reentrant quote"
        );
        number(m.precision.price, &e["precision"]["price"]);
        opt_number(m.contract_size, e.get("contractSize"));
    }
    native_positions(&store.positions(), &case["positions"]);
    native_balance(&store.balance(), &case["balance"]);
    assert_eq!(
        modes.margin_type.get(),
        margin(case.get("marginMode")),
        "native margin cache"
    );
    assert_eq!(
        modes.position_mode.get(),
        mode(case.get("positionMode")),
        "native position cache"
    );
    assert_eq!(disposed.get(), case["disposed"].as_bool().unwrap());
    number(auth.get(), &case["authFailureStreak"]);
    // All decoded native fields/cache/store values precede every wire assertion.
    wire_values(&Value::Array(trace.borrow().clone()), &case["trace"]);
    assert!(
        adapter.queues.borrow().values().all(VecDeque::is_empty),
        "independent queues consumed"
    );
}
fn family(name: &str, expected: usize) {
    let cases: Vec<_> = corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["name"] == name)
        .collect();
    assert_eq!(cases.len(), expected, "independent method floor");
    for case in cases {
        for variant in 0..6 {
            run(case, variant, false)
        }
    }
}
#[test]
fn native_fetch_positions_helper() {
    family("fetchPositionsHelper", 6)
}
#[test]
fn native_fetch_positions() {
    family("fetchPositions", 5)
}
#[test]
fn native_fetch_position_metadata() {
    family("fetchPositionMetadata", 8)
}
#[test]
fn native_fetch_balance_and_positions() {
    family("fetchBalanceAndPositions", 6)
}
#[test]
fn native_fetch_income_history() {
    family("fetchIncomeHistory", 11)
}
#[test]
fn native_fetch_income_history_range() {
    family("fetchIncomeHistoryRange", 2)
}
#[test]
fn native_fetch_closed_position_rows() {
    family("fetchClosedPositionRows", 7)
}
#[test]
fn native_fetch_historical_positions() {
    family("fetchHistoricalPositions", 4)
}
#[test]
fn native_fetch_closed_pnl() {
    family("fetchClosedPnl", 7)
}
#[test]
fn native_position_read_boundaries() {
    assert_eq!(
        corpus()["boundaries"].as_array().unwrap().len(),
        4,
        "independent boundary floor"
    );
    for case in corpus()["boundaries"].as_array().unwrap() {
        for variant in 0..6 {
            run(case, variant, true)
        }
    }
}
