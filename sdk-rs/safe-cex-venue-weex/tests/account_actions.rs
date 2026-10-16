use safe_cex_types::{MarginType, OrderStatus, OrderType, PositionMode, PositionSide};
use safe_cex_venue_weex::{account_actions::*, order_routing::is_algo_order};
use serde_json::Value;
use std::{
    cell::RefCell,
    future::Future,
    path::Path,
    pin::Pin,
    process::Command,
    task::{Context, Poll, Waker},
};

type Host = RefCell<State>;
type Runtime = WeexAccountActionRuntime<Host, usize, usize, (), Vec<Vec<u16>>, NativeError>;
type NativeFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, NativeError>> + 'a>>;
#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeError {
    name: &'static str,
    units: Vec<u16>,
}
#[derive(Debug, Clone, PartialEq)]
struct NativeConfig {
    margin: MarginType,
    mode: PositionMode,
    cross: f64,
    long: f64,
    short: f64,
    extra: Vec<u16>,
}
enum TaskValue {
    Unit,
    Config(usize),
    Positions(Vec<Vec<u16>>),
}
struct Task {
    key: String,
    left: usize,
    done: Option<Result<(), NativeError>>,
    value: TaskValue,
}
struct State {
    input: Value,
    events: Vec<String>,
    requests: Vec<WeexActionRequest>,
    tasks: Vec<Task>,
    completed: Vec<usize>,
    ids: Vec<Vec<u16>>,
    stored: Vec<usize>,
    limits: [f64; 2],
    configs: Vec<NativeConfig>,
    config_calls: Vec<(Vec<u16>, bool)>,
    statuses: Vec<(Vec<Vec<u16>>, OrderStatus)>,
    cache: Vec<(Vec<u16>, NativeConfig)>,
    mode: PositionMode,
    hedged: Option<bool>,
    positions: Option<Vec<Vec<u16>>>,
}
fn units(v: &Value) -> Vec<u16> {
    serde_json::from_value(v.clone()).unwrap()
}
fn indices(v: &Value) -> Vec<usize> {
    serde_json::from_value(v.clone()).unwrap()
}
fn number(v: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap())
}
fn position_mode(v: &Value) -> PositionMode {
    match v.as_str().unwrap() {
        "oneway" => PositionMode::OneWay,
        "hedged" => PositionMode::Hedged,
        other => panic!("unknown mode {other}"),
    }
}
fn margin_type(v: &Value) -> MarginType {
    match v.as_str().unwrap() {
        "cross" => MarginType::Cross,
        "isolated" => MarginType::Isolated,
        other => panic!("unknown margin {other}"),
    }
}
fn config(v: &Value) -> NativeConfig {
    NativeConfig {
        margin: margin_type(&v["marginType"]),
        mode: position_mode(&v["positionMode"]),
        cross: number(&v["crossLeverage"]),
        long: number(&v["isolatedLongLeverage"]),
        short: number(&v["isolatedShortLeverage"]),
        extra: units(&v["extra"]),
    }
}
fn fail(state: &State, key: &str) -> Result<(), NativeError> {
    if state.input["fail"] == key
        || state.input["fails"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == key)
    {
        Err(NativeError {
            name: "RangeError",
            units: format!("fail:{key}").encode_utf16().collect(),
        })
    } else {
        Ok(())
    }
}
fn record(host: &Host, event: &str) -> Result<(), NativeError> {
    let mut state = host.borrow_mut();
    state.events.push(event.into());
    fail(&state, event)
}
fn schedule(state: &mut State, key: String, value: TaskValue) -> usize {
    let left = state.input["delays"][&key].as_u64().unwrap_or(1) as usize;
    let index = state.tasks.len();
    state.tasks.push(Task {
        key,
        left,
        done: None,
        value,
    });
    index
}
struct Observation<'a, T> {
    host: &'a Host,
    index: usize,
    convert: fn(&TaskValue) -> T,
}
impl<T> Future for Observation<'_, T> {
    type Output = Result<T, NativeError>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        let state = self.host.borrow();
        let task = &state.tasks[self.index];
        match &task.done {
            None => Poll::Pending,
            Some(Err(error)) => Poll::Ready(Err(error.clone())),
            Some(Ok(())) => Poll::Ready(Ok((self.convert)(&task.value))),
        }
    }
}
fn request(host: &Host, request: WeexActionRequest) -> Result<NativeFuture<'_, ()>, NativeError> {
    let mut state = host.borrow_mut();
    let index = state.requests.len() + 1;
    let key = format!("request:{index}");
    state.events.push(format!("{key}:start"));
    state.requests.push(request);
    fail(&state, &format!("{key}:start"))?;
    let task = schedule(&mut state, key, TaskValue::Unit);
    Ok(Box::pin(Observation {
        host,
        index: task,
        convert: |_| (),
    }))
}
struct Aggregate<'a> {
    host: &'a Host,
    first: NativeFuture<'a, ()>,
    second: NativeFuture<'a, ()>,
    indices: [usize; 2],
    already_ready: Vec<usize>,
    done: [bool; 2],
}
impl Future for Aggregate<'_> {
    type Output = Result<(), NativeError>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let a = if self.done[0] {
            Poll::Ready(Ok(()))
        } else {
            self.first.as_mut().poll(cx)
        };
        let b = if self.done[1] {
            Poll::Ready(Ok(()))
        } else {
            self.second.as_mut().poll(cx)
        };
        let completion = self.host.borrow().completed.clone();
        let order = self
            .already_ready
            .iter()
            .chain(completion.iter())
            .copied()
            .collect::<Vec<_>>();
        let mut a_error = None;
        let mut b_error = None;
        match a {
            Poll::Ready(Err(error)) => a_error = Some(error),
            Poll::Ready(Ok(())) => self.done[0] = true,
            Poll::Pending => {}
        }
        match b {
            Poll::Ready(Err(error)) => b_error = Some(error),
            Poll::Ready(Ok(())) => self.done[1] = true,
            Poll::Pending => {}
        }
        for index in order {
            if index == self.indices[0] {
                if let Some(error) = a_error.take() {
                    return Poll::Ready(Err(error));
                }
            }
            if index == self.indices[1] {
                if let Some(error) = b_error.take() {
                    return Poll::Ready(Err(error));
                }
            }
        }
        assert!(
            a_error.is_none() && b_error.is_none(),
            "native_aggregate_completion_metadata"
        );
        if self.done == [true, true] {
            Poll::Ready(Ok(()))
        } else {
            Poll::Pending
        }
    }
}
fn promise_all<'a>(
    host: &'a Host,
    first: NativeFuture<'a, ()>,
    second: NativeFuture<'a, ()>,
) -> Result<NativeFuture<'a, ()>, NativeError> {
    record(host, "all")?;
    let state = host.borrow();
    let indices = [state.tasks.len() - 2, state.tasks.len() - 1];
    let already_ready = indices
        .iter()
        .copied()
        .filter(|&index| state.tasks[index].done.is_some())
        .collect();
    Ok(Box::pin(Aggregate {
        host,
        first,
        second,
        indices,
        already_ready,
        done: [false, false],
    }))
}
fn get_config<'a>(
    host: &'a Host,
    symbol: &[u16],
    refetch: bool,
) -> Result<NativeFuture<'a, usize>, NativeError> {
    let mut state = host.borrow_mut();
    let index = state.config_calls.len() + 1;
    let key = format!("config:{index}");
    state.events.push(format!("{key}:start"));
    state.config_calls.push((symbol.to_vec(), refetch));
    fail(&state, &format!("{key}:start"))?;
    let config = (index - 1).min(state.configs.len() - 1);
    let task = schedule(&mut state, key, TaskValue::Config(config));
    Ok(Box::pin(Observation {
        host,
        index: task,
        convert: |v| match v {
            TaskValue::Config(index) => *index,
            _ => panic!("not config"),
        },
    }))
}
fn order_id(host: &Host, index: &usize) -> Result<Vec<u16>, NativeError> {
    let mut state = host.borrow_mut();
    state.events.push(format!("id:{index}"));
    fail(&state, "id")?;
    Ok(state.ids[*index].clone())
}
fn order_symbol(host: &Host, index: &usize) -> Result<Vec<u16>, NativeError> {
    let mut state = host.borrow_mut();
    state.events.push(format!("symbol:{index}"));
    Ok(units(&state.input["orders"][*index]["symbol"]))
}
fn algo(host: &Host, index: &usize) -> Result<bool, NativeError> {
    host.borrow_mut().events.push(format!("algo:{index}"));
    let ty = match host.borrow().input["orders"][*index]["type"]
        .as_str()
        .unwrap()
    {
        "limit" => OrderType::Limit,
        "market" => OrderType::Market,
        "conditional" => OrderType::Conditional,
        "take_profit_market" => OrderType::TakeProfit,
        "stop_market" => OrderType::StopLoss,
        "trailing_stop_market" => OrderType::TrailingStopLoss,
        other => panic!("unknown order type {other}"),
    };
    if matches!(
        ty,
        OrderType::Conditional
            | OrderType::TakeProfit
            | OrderType::StopLoss
            | OrderType::TrailingStopLoss
    ) {
        Ok(is_algo_order(Some(ty), &[]))
    } else {
        Ok(is_algo_order(Some(ty), &order_id(host, index)?))
    }
}
fn orders(host: &Host) -> Result<Vec<usize>, NativeError> {
    record(host, "orders")?;
    Ok(host.borrow().stored.clone())
}
fn status(host: &Host, ids: Vec<Vec<u16>>, status: OrderStatus) -> Result<(), NativeError> {
    record(host, "status")?;
    host.borrow_mut().statuses.push((ids, status));
    Ok(())
}
fn market(host: &Host, _: &[u16]) -> Result<Option<()>, NativeError> {
    record(host, "market")?;
    Ok(if host.borrow().input["market"] == true {
        Some(())
    } else {
        None
    })
}
fn min(host: &Host, _: &()) -> Result<f64, NativeError> {
    record(host, "limit:min")?;
    Ok(host.borrow().limits[0])
}
fn max(host: &Host, _: &()) -> Result<f64, NativeError> {
    record(host, "limit:max")?;
    Ok(host.borrow().limits[1])
}
fn margin(host: &Host, index: &usize) -> Result<MarginType, NativeError> {
    record(host, "configField:marginType")?;
    Ok(host.borrow().configs[*index].margin)
}
fn mode(host: &Host, index: &usize) -> Result<PositionMode, NativeError> {
    record(host, "configField:positionMode")?;
    Ok(host.borrow().configs[*index].mode)
}
fn long(host: &Host, index: &usize) -> Result<f64, NativeError> {
    record(host, "configField:isolatedLongLeverage")?;
    Ok(host.borrow().configs[*index].long)
}
fn short(host: &Host, index: &usize) -> Result<f64, NativeError> {
    record(host, "configField:isolatedShortLeverage")?;
    Ok(host.borrow().configs[*index].short)
}
fn position_symbols(host: &Host) -> Result<Vec<Vec<u16>>, NativeError> {
    record(host, "positionSymbols")?;
    Ok(host.borrow().input["positionSymbols"]
        .as_array()
        .unwrap()
        .iter()
        .map(units)
        .collect())
}
fn market_symbols(host: &Host) -> Result<Vec<Vec<u16>>, NativeError> {
    record(host, "marketSymbols")?;
    Ok(host.borrow().input["marketSymbols"]
        .as_array()
        .unwrap()
        .iter()
        .map(units)
        .collect())
}
fn cache(
    host: &Host,
    symbol: &[u16],
    index: &usize,
    mode: PositionMode,
) -> Result<(), NativeError> {
    for key in [
        "marginType",
        "positionMode",
        "crossLeverage",
        "isolatedLongLeverage",
        "isolatedShortLeverage",
        "extra",
    ] {
        record(host, &format!("configField:{key}"))?;
    }
    record(host, "cache")?;
    let mut state = host.borrow_mut();
    let mut config = state.configs[*index].clone();
    config.mode = mode;
    state.cache.push((symbol.to_vec(), config));
    Ok(())
}
fn set_mode(host: &Host, mode: PositionMode) -> Result<(), NativeError> {
    record(host, "accountMode")?;
    host.borrow_mut().mode = mode;
    Ok(())
}
fn set_hedged(host: &Host, value: bool) -> Result<(), NativeError> {
    record(host, "setting")?;
    host.borrow_mut().hedged = Some(value);
    Ok(())
}
fn fetch_positions(host: &Host) -> Result<NativeFuture<'_, Vec<Vec<u16>>>, NativeError> {
    record(host, "positions:1:start")?;
    let mut state = host.borrow_mut();
    let positions = state.input["positions"]
        .as_array()
        .unwrap()
        .iter()
        .map(units)
        .collect();
    let index = schedule(
        &mut state,
        "positions:1".into(),
        TaskValue::Positions(positions),
    );
    Ok(Box::pin(Observation {
        host,
        index,
        convert: |v| match v {
            TaskValue::Positions(p) => p.clone(),
            _ => panic!("not positions"),
        },
    }))
}
fn update_positions(host: &Host, positions: Vec<Vec<u16>>) -> Result<(), NativeError> {
    record(host, "updatePositions")?;
    host.borrow_mut().positions = Some(positions);
    Ok(())
}
fn new_error(_: &Host, units: Vec<u16>) -> NativeError {
    NativeError {
        name: "Error",
        units,
    }
}
fn runtime() -> Runtime {
    Runtime {
        request,
        promise_all,
        is_algo_order: algo,
        order_id,
        order_symbol,
        orders,
        update_orders_status: status,
        market,
        leverage_min: min,
        leverage_max: max,
        config: get_config,
        margin_type: margin,
        position_mode: mode,
        isolated_long_leverage: long,
        isolated_short_leverage: short,
        position_symbols,
        market_symbols,
        cache_with_mode: cache,
        set_account_mode: set_mode,
        set_hedged,
        fetch_positions,
        update_positions,
        new_error,
    }
}
fn tick(host: &Host) {
    let mut state = host.borrow_mut();
    let mut due = Vec::new();
    for (index, task) in state.tasks.iter_mut().enumerate() {
        if task.done.is_none() {
            task.left -= 1;
            if task.left == 0 {
                due.push(index);
            }
        }
    }
    if state.input["reverseDue"] == true {
        due.reverse();
    }
    for index in due {
        let key = state.tasks[index].key.clone();
        let stage = format!("{key}:end");
        state.events.push(stage.clone());
        let result = fail(&state, &stage);
        if result.is_ok() {
            if let Some(request_index) = key.strip_prefix("request:") {
                let mutation = &state.input["mutateIds"][request_index];
                if !mutation.is_null() {
                    let index = mutation["index"].as_u64().unwrap() as usize;
                    let id = units(&mutation["id"]);
                    state.ids[index] = id;
                }
                if !state.input["afterOrders"].is_null() {
                    state.stored = indices(&state.input["afterOrders"]);
                }
            }
            if key.starts_with("config:") && !state.input["afterLimits"].is_null() {
                state.limits = [
                    number(&state.input["afterLimits"][0]),
                    number(&state.input["afterLimits"][1]),
                ];
            }
        }
        state.tasks[index].done = Some(result);
        state.completed.push(index);
    }
}
fn run<T>(host: &Host, mut future: NativeFuture<'_, T>) -> Result<T, NativeError> {
    let waker = Waker::noop().clone();
    let mut cx = Context::from_waker(&waker);
    let mut iterations = 0;
    let result = loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut cx) {
            break value;
        }
        tick(host);
        iterations += 1;
        assert!(iterations < 100, "native_scheduler_floor");
    };
    drop(future);
    while host.borrow().tasks.iter().any(|t| t.done.is_none()) {
        tick(host);
        iterations += 1;
        assert!(iterations < 100, "native_detached_peer_floor");
    }
    result
}
fn field_key(key: &str) -> &'static str {
    match key {
        "symbol" => "symbol",
        "orderIdList" => "orderIdList",
        "orderId" => "orderId",
        "marginType" => "marginType",
        "crossLeverage" => "crossLeverage",
        "isolatedLongLeverage" => "isolatedLongLeverage",
        "isolatedShortLeverage" => "isolatedShortLeverage",
        "separatedType" => "separatedType",
        other => panic!("unknown key {other}"),
    }
}
fn entries(v: &Value) -> Option<Vec<(&'static str, WeexActionValue)>> {
    if v.is_null() {
        None
    } else {
        Some(
            v.as_array()
                .unwrap()
                .iter()
                .map(|entry| {
                    let value = &entry[1];
                    let native = match value["kind"].as_str().unwrap() {
                        "string" => WeexActionValue::String(units(&value["units"])),
                        "strings" => WeexActionValue::Strings(
                            value["items"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .map(units)
                                .collect(),
                        ),
                        other => panic!("unknown value {other}"),
                    };
                    (field_key(entry[0].as_str().unwrap()), native)
                })
                .collect(),
        )
    }
}
fn endpoint(v: &Value) -> &'static str {
    match v.as_str().unwrap() {
        "/capi/v3/batchOrders" => "/capi/v3/batchOrders",
        "/capi/v3/algoOrder" => "/capi/v3/algoOrder",
        "/capi/v3/allOpenOrders" => "/capi/v3/allOpenOrders",
        "/capi/v3/algoOpenOrders" => "/capi/v3/algoOpenOrders",
        "/capi/v3/account/leverage" => "/capi/v3/account/leverage",
        "/capi/v3/account/marginType" => "/capi/v3/account/marginType",
        "/capi/v3/closePositions" => "/capi/v3/closePositions",
        other => panic!("unknown endpoint {other}"),
    }
}
fn decode_request(v: &Value) -> WeexActionRequest {
    WeexActionRequest {
        method: match v["method"].as_str().unwrap() {
            "delete" => WeexActionMethod::Delete,
            "post" => WeexActionMethod::Post,
            other => panic!("unknown method {other}"),
        },
        endpoint: endpoint(&v["endpoint"]),
        body: entries(&v["body"]),
        params: entries(&v["params"]),
    }
}
#[test]
fn native_account_actions_match_typescript() {
    let oracle = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/account-actions-oracle.cjs"))
        .output()
        .unwrap();
    assert!(
        oracle.status.success(),
        "oracle failed: {}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&oracle.stdout).unwrap();
    let expected_ids = [
        "cancel_empty",
        "cancel_mixed",
        "cancel_batches",
        "cancel_normal_failure",
        "cancel_algo_failure",
        "cancel_start_failure",
        "cancel_mutated_id",
        "cancel_status_failure",
        "symbol_cancel",
        "symbol_parallel",
        "symbol_failure_peer_survives",
        "symbol_latest_store",
        "symbol_factory_failure",
        "symbol_surrogate",
        "all_cancel",
        "all_parallel",
        "all_failure",
        "all_latest_store",
        "all_empty_store",
        "leverage_cross",
        "leverage_lower",
        "leverage_upper",
        "leverage_isolated_both",
        "leverage_isolated_long",
        "leverage_isolated_short",
        "leverage_nan",
        "leverage_zero",
        "leverage_missing_market",
        "leverage_after_fetch_limits",
        "leverage_config_failure",
        "leverage_post_failure",
        "leverage_metadata_failure",
        "margin_isolated",
        "margin_cross",
        "margin_missing",
        "margin_empty",
        "margin_post_failure",
        "margin_metadata_failure",
        "mode_explicit",
        "mode_position_unique",
        "mode_market_duplicates",
        "mode_no_targets",
        "mode_second_post_failure",
        "mode_cache_failure",
        "mode_setting_failure",
        "mode_oneway",
        "close_positions",
        "close_post_failure",
        "close_fetch_failure",
        "close_update_failure",
        "nuke_sequence",
        "nuke_close_failure",
        "nuke_cancel_failure",
        "metadata_refetch",
        "metadata_failure",
        "symbol_dual_rejection",
        "all_dual_rejection",
        "all_second_factory_failure",
    ];
    assert_eq!(
        cases
            .iter()
            .map(|c| c["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        expected_ids,
        "native_case_floor"
    );
    let runtime = runtime();
    let mut traces = Vec::new();
    for case in &cases {
        let id = case["id"].as_str().unwrap();
        let input = &case["input"];
        let host = RefCell::new(State {
            input: input.clone(),
            events: vec![],
            requests: vec![],
            tasks: vec![],
            completed: vec![],
            ids: input["orders"]
                .as_array()
                .unwrap()
                .iter()
                .map(|o| units(&o["id"]))
                .collect(),
            stored: indices(&input["stored"]),
            limits: [number(&input["min"]), number(&input["max"])],
            configs: input["configs"]
                .as_array()
                .unwrap()
                .iter()
                .map(config)
                .collect(),
            config_calls: vec![],
            statuses: vec![],
            cache: vec![],
            mode: PositionMode::OneWay,
            hedged: None,
            positions: None,
        });
        let symbol = input["symbol"].as_array().map(|_| units(&input["symbol"]));
        let target = symbol.as_deref();
        let selected = indices(&input["selected"]);
        let flag = input["flag"] == true;
        let future = match input["method"].as_str().unwrap() {
            "cancelOrders" => cancel_orders(&host, &runtime, &selected),
            "cancelSymbolOrders" => cancel_symbol_orders(&host, &runtime, target.unwrap()),
            "cancelAllOrders" => cancel_all_orders(&host, &runtime),
            "setLeverage" => set_leverage(
                &host,
                &runtime,
                target.unwrap(),
                number(&input["leverage"]),
                match input["side"].as_str() {
                    Some("long") => Some(PositionSide::Long),
                    Some("short") => Some(PositionSide::Short),
                    None => None,
                    other => panic!("unknown side {other:?}"),
                },
            ),
            "changeMarginType" => change_margin_type(&host, &runtime, flag, target),
            "changePositionMode" => change_position_mode(&host, &runtime, flag, target),
            "nukePositions" => nuke_positions(&host, &runtime),
            "nuke" => nuke(&host, &runtime),
            "updateSymbolMetadata" => update_symbol_metadata(&host, &runtime, target.unwrap()),
            other => panic!("unknown action {other}"),
        };
        let prefix = host.borrow().events.clone();
        assert!(
            host.try_borrow_mut().is_ok(),
            "native_host_borrow_released::{id}"
        );
        let result = run(&host, future);
        let state = host.into_inner();
        if case["error"].is_null() {
            assert_eq!(result, Ok(()), "native_result::{id}");
        } else {
            let error = result.unwrap_err();
            assert_eq!(
                error.name,
                case["error"]["name"].as_str().unwrap(),
                "native_error_name::{id}"
            );
            assert_eq!(
                error.units,
                units(&case["error"]["units"]),
                "native_error_message::{id}"
            );
        }
        assert_eq!(
            state.requests,
            case["requests"]
                .as_array()
                .unwrap()
                .iter()
                .map(decode_request)
                .collect::<Vec<_>>(),
            "native_requests::{id}"
        );
        let expected_statuses = case["statuses"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                assert_eq!(v["status"], "canceled");
                (
                    v["ids"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(units)
                        .collect::<Vec<_>>(),
                    OrderStatus::Canceled,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(state.statuses, expected_statuses, "native_statuses::{id}");
        assert_eq!(
            state.config_calls,
            case["configCalls"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| (units(&v["symbol"]), v["refetch"] == true))
                .collect::<Vec<_>>(),
            "native_config_calls::{id}"
        );
        assert_eq!(
            state.cache,
            case["cache"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| (units(&v["symbol"]), config(&v["config"])))
                .collect::<Vec<_>>(),
            "native_cached_configs::{id}"
        );
        assert_eq!(
            state.mode,
            position_mode(&case["accountMode"]),
            "native_account_mode::{id}"
        );
        assert_eq!(
            state.hedged,
            case["hedged"].as_bool(),
            "native_hedged_setting::{id}"
        );
        assert_eq!(
            state.positions,
            case["updatedPositions"]
                .as_array()
                .map(|p| p.iter().map(units).collect::<Vec<_>>()),
            "native_positions::{id}"
        );
        let pending = state.tasks.iter().filter(|t| t.done.is_none()).count();
        assert_eq!(pending, 0, "native_detached_tasks_complete::{id}");
        if id == "cancel_batches" {
            assert_eq!(state.requests.len(), 6, "native_batch_request_floor");
            assert_eq!(state.statuses[0].0.len(), 24, "native_batch_status_floor");
        }
        traces.push((
            id.to_string(),
            prefix,
            state.events,
            case["prefix"].clone(),
            case["events"].clone(),
        ));
    }
    for (id, prefix, events, expected_prefix, expected_events) in traces {
        assert_eq!(
            prefix,
            expected_prefix
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect::<Vec<_>>(),
            "eager_prefix::{id}"
        );
        assert_eq!(
            events,
            expected_events
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect::<Vec<_>>(),
            "effect_trace::{id}"
        );
    }
}
