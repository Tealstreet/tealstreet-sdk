use safe_cex_num::numeric_ingress::{JsNumberInput, js_from_str};
use safe_cex_types::{
    BothPositionSide, Execution, ExecutionPositionSide, HistoricalTrade, OrderSide, OrderStatus,
    OrderStopTriggerType, OrderTimeInForce, OrderType,
};
use safe_cex_venue_woo::{
    v3_private::{WooV3AccountPositionMode, WooV3PageMeta, WooV3PaginatedRows, WooV3PositionSide},
    v3_private_orders::*,
};
use serde_json::Value;
use std::process::Command;
fn source() -> Value {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/private-orders-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "PRIVATE-ORDERS source oracle {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn input(row: &Value, key: &str) -> Option<JsNumberInput> {
    row[key].as_str().map(|s| js_from_str(s).unwrap())
}
fn number(actual: f64, e: &Value, label: &str) {
    assert_eq!(
        actual.to_bits(),
        e["$number"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap()
            .to_bits(),
        "PRIVATE-ORDERS native {label}"
    );
}
fn checked<T>(result: Result<T, String>, e: &Value, check: impl FnOnce(T, &Value)) {
    if let Some(error) = e.get("error") {
        assert!(result.is_err(), "PRIVATE-ORDERS native refusal");
        assert_eq!(
            result.err().unwrap(),
            error["message"].as_str().unwrap(),
            "PRIVATE-ORDERS native first error"
        );
    } else {
        assert!(result.is_ok(), "PRIVATE-ORDERS native expected success");
        check(result.ok().unwrap(), &e["ok"]);
    }
}
fn mode(e: &Value) -> WooV3AccountPositionMode {
    match e.as_str().unwrap() {
        "ONE_WAY" => WooV3AccountPositionMode::OneWay,
        "HEDGE_MODE" => WooV3AccountPositionMode::HedgeMode,
        _ => panic!("mode"),
    }
}
fn side(e: &Value) -> WooV3PositionSide {
    match e.as_str().unwrap() {
        "BOTH" => WooV3PositionSide::Both,
        "LONG" => WooV3PositionSide::Long,
        "SHORT" => WooV3PositionSide::Short,
        _ => panic!("side"),
    }
}
fn expected_woo_v3_order_side(s: &str) -> WooV3OrderSide {
    match s {
        "BUY" => WooV3OrderSide::Buy,
        "SELL" => WooV3OrderSide::Sell,
        _ => panic!("source enum value"),
    }
}
fn expected_woo_v3_order_status(s: &str) -> WooV3OrderStatus {
    match s {
        "NEW" => WooV3OrderStatus::New,
        "CANCELLED" => WooV3OrderStatus::Cancelled,
        "PARTIAL_FILLED" => WooV3OrderStatus::PartialFilled,
        "FILLED" => WooV3OrderStatus::Filled,
        "REJECTED" => WooV3OrderStatus::Rejected,
        _ => panic!("source enum value"),
    }
}
fn expected_woo_v3_regular_order_type(s: &str) -> WooV3RegularOrderType {
    match s {
        "LIMIT" => WooV3RegularOrderType::Limit,
        "MARKET" => WooV3RegularOrderType::Market,
        "IOC" => WooV3RegularOrderType::Ioc,
        "POST_ONLY" => WooV3RegularOrderType::PostOnly,
        "AC" => WooV3RegularOrderType::Ac,
        "FOK" => WooV3RegularOrderType::Fok,
        "LIQUIDATE" => WooV3RegularOrderType::Liquidate,
        "BID" => WooV3RegularOrderType::Bid,
        "ASK" => WooV3RegularOrderType::Ask,
        "RPI" => WooV3RegularOrderType::Rpi,
        "LIQUIDATE_BLP" => WooV3RegularOrderType::LiquidateBlp,
        "ADL" => WooV3RegularOrderType::Adl,
        _ => panic!("source enum value"),
    }
}
fn expected_woo_v3_algo_type(s: &str) -> WooV3AlgoType {
    match s {
        "STOP" => WooV3AlgoType::Stop,
        "OCO" => WooV3AlgoType::Oco,
        "TRAILING_STOP" => WooV3AlgoType::TrailingStop,
        "POSITIONAL_TP_SL" => WooV3AlgoType::PositionalTpSl,
        "TP_SL" => WooV3AlgoType::TpSl,
        "BRACKET" => WooV3AlgoType::Bracket,
        "STOP_BRACKET" => WooV3AlgoType::StopBracket,
        "TAKE_PROFIT" => WooV3AlgoType::TakeProfit,
        "STOP_LOSS" => WooV3AlgoType::StopLoss,
        _ => panic!("source enum value"),
    }
}
fn expected_woo_v3_trigger_price_type(s: &str) -> WooV3TriggerPriceType {
    match s {
        "MARKET_PRICE" => WooV3TriggerPriceType::MarketPrice,
        "INDEX_PRICE" => WooV3TriggerPriceType::IndexPrice,
        "LAST_PRICE" => WooV3TriggerPriceType::LastPrice,
        _ => panic!("source enum value"),
    }
}
fn expected_woo_v3_trigger_status(s: &str) -> WooV3TriggerStatus {
    match s {
        "USELESS" => WooV3TriggerStatus::Useless,
        "TRIGGERED" => WooV3TriggerStatus::Triggered,
        "CANCELLED" => WooV3TriggerStatus::Cancelled,
        "SUCCESS" => WooV3TriggerStatus::Success,
        _ => panic!("source enum value"),
    }
}
fn expected_woo_v3_margin_mode(s: &str) -> WooV3MarginMode {
    match s {
        "ISOLATED" => WooV3MarginMode::Isolated,
        "CROSSED" => WooV3MarginMode::Crossed,
        _ => panic!("source enum value"),
    }
}
fn quantity(v: &WooV3GatewayAlgoQuantity, e: &Value) {
    match v {
        WooV3GatewayAlgoQuantity::Decimal(s) => assert_eq!(
            Some(s.as_str()),
            e.as_str(),
            "PRIVATE-ORDERS native quantity variant/text"
        ),
        WooV3GatewayAlgoQuantity::Number(n) => number(*n, e, "quantity bits"),
    }
}
fn amount(v: &WooV3OrderAmount, e: &Value) {
    match v {
        WooV3OrderAmount::Null => assert!(e.is_null(), "PRIVATE-ORDERS native amount null"),
        WooV3OrderAmount::Text(s) => assert_eq!(
            Some(s.as_str()),
            e.as_str(),
            "PRIVATE-ORDERS native amount text"
        ),
    }
}
fn inspect_woo_v3_regular_order(v: &WooV3RegularOrder, e: &Value) {
    assert_eq!(
        &v.symbol,
        e["symbol"].as_str().unwrap(),
        "PRIVATE-ORDERS native symbol"
    );
    assert_eq!(
        v.status,
        expected_woo_v3_order_status(e["status"].as_str().unwrap()),
        "PRIVATE-ORDERS native status"
    );
    assert_eq!(
        v.side,
        expected_woo_v3_order_side(e["side"].as_str().unwrap()),
        "PRIVATE-ORDERS native side"
    );
    assert_eq!(v.position_side, side(&e["positionSide"]));
    number(v.created_time, &e["createdTime"], "createdTime");
    number(v.order_id, &e["orderId"], "orderId");
    assert_eq!(
        &v.order_tag,
        e["orderTag"].as_str().unwrap(),
        "PRIVATE-ORDERS native orderTag"
    );
    assert_eq!(
        &v.price,
        e["price"].as_str().unwrap(),
        "PRIVATE-ORDERS native price"
    );
    assert_eq!(
        v.r#type,
        expected_woo_v3_regular_order_type(e["type"].as_str().unwrap()),
        "PRIVATE-ORDERS native type"
    );
    assert_eq!(
        &v.quantity,
        e["quantity"].as_str().unwrap(),
        "PRIVATE-ORDERS native quantity"
    );
    amount(&v.amount, &e["amount"]);
    assert_eq!(
        &v.visible,
        e["visible"].as_str().unwrap(),
        "PRIVATE-ORDERS native visible"
    );
    assert_eq!(
        &v.executed,
        e["executed"].as_str().unwrap(),
        "PRIVATE-ORDERS native executed"
    );
    assert_eq!(
        &v.total_fee,
        e["totalFee"].as_str().unwrap(),
        "PRIVATE-ORDERS native totalFee"
    );
    assert_eq!(
        &v.fee_asset,
        e["feeAsset"].as_str().unwrap(),
        "PRIVATE-ORDERS native feeAsset"
    );
    number(v.total_rebate, &e["totalRebate"], "totalRebate");
    assert_eq!(
        &v.rebate_currency,
        e["rebateCurrency"].as_str().unwrap(),
        "PRIVATE-ORDERS native rebateCurrency"
    );
    number(v.client_order_id, &e["clientOrderId"], "clientOrderId");
    assert_eq!(
        v.reduce_only,
        e["reduceOnly"].as_bool().unwrap(),
        "PRIVATE-ORDERS native reduceOnly"
    );
    assert_eq!(
        v.realized_pnl.is_some(),
        e.get("realizedPnl").is_some(),
        "PRIVATE-ORDERS native realizedPnl presence"
    );
    if let Some(value) = &v.realized_pnl {
        assert_eq!(
            value,
            e["realizedPnl"].as_str().unwrap(),
            "PRIVATE-ORDERS native realizedPnl"
        );
    }
    assert_eq!(
        &v.average_executed_price,
        e["averageExecutedPrice"].as_str().unwrap(),
        "PRIVATE-ORDERS native averageExecutedPrice"
    );
    assert_eq!(
        v.leverage.is_some(),
        e.get("leverage").is_some(),
        "PRIVATE-ORDERS native leverage presence"
    );
    if let Some(value) = &v.leverage {
        number(*value, &e["leverage"], "leverage");
    }
    assert_eq!(
        v.margin_mode.is_some(),
        e.get("marginMode").is_some(),
        "PRIVATE-ORDERS native marginMode presence"
    );
    if let Some(value) = &v.margin_mode {
        assert_eq!(
            *value,
            expected_woo_v3_margin_mode(e["marginMode"].as_str().unwrap()),
            "PRIVATE-ORDERS native marginMode"
        );
    }
    assert_eq!(
        v.bid_ask_level.is_some(),
        e.get("bidAskLevel").is_some(),
        "PRIVATE-ORDERS native bidAskLevel presence"
    );
    if let Some(value) = &v.bid_ask_level {
        number(*value, &e["bidAskLevel"], "bidAskLevel");
    }
}
fn inspect_woo_v3_algo_order(v: &WooV3AlgoOrder, e: &Value) {
    number(v.algo_order_id, &e["algoOrderId"], "algoOrderId");
    number(
        v.client_algo_order_id,
        &e["clientAlgoOrderId"],
        "clientAlgoOrderId",
    );
    number(
        v.root_algo_order_id,
        &e["rootAlgoOrderId"],
        "rootAlgoOrderId",
    );
    number(
        v.parent_algo_order_id,
        &e["parentAlgoOrderId"],
        "parentAlgoOrderId",
    );
    assert_eq!(
        &v.symbol,
        e["symbol"].as_str().unwrap(),
        "PRIVATE-ORDERS native symbol"
    );
    assert_eq!(
        &v.algo_order_tag,
        e["algoOrderTag"].as_str().unwrap(),
        "PRIVATE-ORDERS native algoOrderTag"
    );
    assert_eq!(
        v.algo_type,
        expected_woo_v3_algo_type(e["algoType"].as_str().unwrap()),
        "PRIVATE-ORDERS native algoType"
    );
    assert_eq!(
        v.side,
        expected_woo_v3_order_side(e["side"].as_str().unwrap()),
        "PRIVATE-ORDERS native side"
    );
    assert_eq!(v.position_side, side(&e["positionSide"]));
    quantity(&v.quantity, &e["quantity"]);
    assert_eq!(
        v.is_triggered,
        e["isTriggered"].as_bool().unwrap(),
        "PRIVATE-ORDERS native isTriggered"
    );
    assert_eq!(
        v.trigger_status,
        expected_woo_v3_trigger_status(e["triggerStatus"].as_str().unwrap()),
        "PRIVATE-ORDERS native triggerStatus"
    );
    assert_eq!(
        v.r#type.is_some(),
        e.get("type").is_some(),
        "PRIVATE-ORDERS native type presence"
    );
    if let Some(value) = &v.r#type {
        assert_eq!(
            *value,
            expected_woo_v3_regular_order_type(e["type"].as_str().unwrap()),
            "PRIVATE-ORDERS native type"
        );
    }
    assert_eq!(
        v.root_algo_status,
        expected_woo_v3_order_status(e["rootAlgoStatus"].as_str().unwrap()),
        "PRIVATE-ORDERS native rootAlgoStatus"
    );
    assert_eq!(
        v.algo_status,
        expected_woo_v3_order_status(e["algoStatus"].as_str().unwrap()),
        "PRIVATE-ORDERS native algoStatus"
    );
    assert_eq!(
        v.trigger_price_type,
        expected_woo_v3_trigger_price_type(e["triggerPriceType"].as_str().unwrap()),
        "PRIVATE-ORDERS native triggerPriceType"
    );
    assert_eq!(
        &v.trigger_time,
        e["triggerTime"].as_str().unwrap(),
        "PRIVATE-ORDERS native triggerTime"
    );
    assert_eq!(
        v.trigger_price.is_some(),
        e.get("triggerPrice").is_some(),
        "PRIVATE-ORDERS native triggerPrice presence"
    );
    if let Some(value) = &v.trigger_price {
        assert_eq!(
            value,
            e["triggerPrice"].as_str().unwrap(),
            "PRIVATE-ORDERS native triggerPrice"
        );
    }
    assert_eq!(
        v.price.is_some(),
        e.get("price").is_some(),
        "PRIVATE-ORDERS native price presence"
    );
    if let Some(value) = &v.price {
        assert_eq!(
            value,
            e["price"].as_str().unwrap(),
            "PRIVATE-ORDERS native price"
        );
    }
    assert_eq!(
        v.visible_quantity.is_some(),
        e.get("visibleQuantity").is_some(),
        "PRIVATE-ORDERS native visibleQuantity presence"
    );
    if let Some(value) = &v.visible_quantity {
        assert_eq!(
            value,
            e["visibleQuantity"].as_str().unwrap(),
            "PRIVATE-ORDERS native visibleQuantity"
        );
    }
    assert_eq!(
        &v.total_executed_quantity,
        e["totalExecutedQuantity"].as_str().unwrap(),
        "PRIVATE-ORDERS native totalExecutedQuantity"
    );
    assert_eq!(
        &v.average_executed_price,
        e["averageExecutedPrice"].as_str().unwrap(),
        "PRIVATE-ORDERS native averageExecutedPrice"
    );
    assert_eq!(
        &v.total_fee,
        e["totalFee"].as_str().unwrap(),
        "PRIVATE-ORDERS native totalFee"
    );
    assert_eq!(
        &v.fee_asset,
        e["feeAsset"].as_str().unwrap(),
        "PRIVATE-ORDERS native feeAsset"
    );
    number(v.total_rebate, &e["totalRebate"], "totalRebate");
    assert_eq!(
        &v.rebate_currency,
        e["rebateCurrency"].as_str().unwrap(),
        "PRIVATE-ORDERS native rebateCurrency"
    );
    assert_eq!(
        v.reduce_only,
        e["reduceOnly"].as_bool().unwrap(),
        "PRIVATE-ORDERS native reduceOnly"
    );
    number(v.created_time, &e["createdTime"], "createdTime");
    number(v.updated_time, &e["updatedTime"], "updatedTime");
    assert_eq!(
        v.is_activated,
        e["isActivated"].as_bool().unwrap(),
        "PRIVATE-ORDERS native isActivated"
    );
    assert_eq!(
        v.callback_rate.is_some(),
        e.get("callbackRate").is_some(),
        "PRIVATE-ORDERS native callbackRate presence"
    );
    if let Some(value) = &v.callback_rate {
        assert_eq!(
            value,
            e["callbackRate"].as_str().unwrap(),
            "PRIVATE-ORDERS native callbackRate"
        );
    }
    assert_eq!(
        v.activated_price.is_some(),
        e.get("activatedPrice").is_some(),
        "PRIVATE-ORDERS native activatedPrice presence"
    );
    if let Some(value) = &v.activated_price {
        assert_eq!(
            value,
            e["activatedPrice"].as_str().unwrap(),
            "PRIVATE-ORDERS native activatedPrice"
        );
    }
    assert_eq!(
        v.leverage.is_some(),
        e.get("leverage").is_some(),
        "PRIVATE-ORDERS native leverage presence"
    );
    if let Some(value) = &v.leverage {
        number(*value, &e["leverage"], "leverage");
    }
    assert_eq!(
        v.margin_mode.is_some(),
        e.get("marginMode").is_some(),
        "PRIVATE-ORDERS native marginMode presence"
    );
    if let Some(value) = &v.margin_mode {
        assert_eq!(
            *value,
            expected_woo_v3_margin_mode(e["marginMode"].as_str().unwrap()),
            "PRIVATE-ORDERS native marginMode"
        );
    }
    let rows = e["childOrders"].as_array().unwrap();
    assert_eq!(
        v.child_orders.len(),
        rows.len(),
        "PRIVATE-ORDERS native childOrders count"
    );
    for (v, e) in v.child_orders.iter().zip(rows) {
        inspect_woo_v3_algo_order(v, e);
    }
}
fn inspect_woo_v3_transaction(v: &WooV3Transaction, e: &Value) {
    number(v.id, &e["id"], "id");
    assert_eq!(
        &v.symbol,
        e["symbol"].as_str().unwrap(),
        "PRIVATE-ORDERS native symbol"
    );
    number(v.order_id, &e["orderId"], "orderId");
    assert_eq!(
        &v.order_tag,
        e["orderTag"].as_str().unwrap(),
        "PRIVATE-ORDERS native orderTag"
    );
    assert_eq!(
        &v.executed_price,
        e["executedPrice"].as_str().unwrap(),
        "PRIVATE-ORDERS native executedPrice"
    );
    assert_eq!(
        &v.executed_quantity,
        e["executedQuantity"].as_str().unwrap(),
        "PRIVATE-ORDERS native executedQuantity"
    );
    number(v.is_maker, &e["isMaker"], "isMaker");
    assert_eq!(
        v.is_match_rpi,
        e["isMatchRpi"].as_bool().unwrap(),
        "PRIVATE-ORDERS native isMatchRpi"
    );
    assert_eq!(
        v.side,
        expected_woo_v3_order_side(e["side"].as_str().unwrap()),
        "PRIVATE-ORDERS native side"
    );
    number(v.fee, &e["fee"], "fee");
    assert_eq!(
        &v.fee_asset,
        e["feeAsset"].as_str().unwrap(),
        "PRIVATE-ORDERS native feeAsset"
    );
    assert_eq!(
        v.realized_pnl.as_deref(),
        e["realizedPnl"].as_str(),
        "PRIVATE-ORDERS native nullable realizedPnl"
    );
    number(
        v.executed_timestamp,
        &e["executedTimestamp"],
        "executedTimestamp",
    );
}
fn inspect_woo_v3_mapped_transaction(v: &WooV3MappedTransaction, e: &Value) {
    execution(&v.execution, &e["execution"]);
    assert_eq!(
        v.historical_trade.is_some(),
        e.get("historicalTrade").is_some(),
        "PRIVATE-ORDERS native historicalTrade presence"
    );
    if let Some(value) = &v.historical_trade {
        historical(value, &e["historicalTrade"]);
    }
    assert_eq!(
        v.deferred_historical_trade_reason.is_some(),
        e.get("deferredHistoricalTradeReason").is_some(),
        "PRIVATE-ORDERS native deferredHistoricalTradeReason presence"
    );
    if let Some(value) = &v.deferred_historical_trade_reason {
        assert_eq!(
            value,
            e["deferredHistoricalTradeReason"].as_str().unwrap(),
            "PRIVATE-ORDERS native deferredHistoricalTradeReason"
        );
    }
    assert_eq!(
        v.maker,
        e["maker"].as_bool().unwrap(),
        "PRIVATE-ORDERS native maker"
    );
    assert_eq!(
        v.is_match_rpi,
        e["isMatchRpi"].as_bool().unwrap(),
        "PRIVATE-ORDERS native isMatchRpi"
    );
}
fn inspect_woo_v3_deferred_algo_order(v: &WooV3DeferredAlgoOrder, e: &Value) {
    inspect_woo_v3_algo_order(&v.order, &e["order"]);
    assert_eq!(
        &v.reason,
        e["reason"].as_str().unwrap(),
        "PRIVATE-ORDERS native reason"
    );
}
fn inspect_woo_v3_algo_order_mapping(v: &WooV3AlgoOrderMapping, e: &Value) {
    let rows = e["orders"].as_array().unwrap();
    assert_eq!(
        v.orders.len(),
        rows.len(),
        "PRIVATE-ORDERS native orders count"
    );
    for (v, e) in v.orders.iter().zip(rows) {
        inspect_mapped_order(v, e);
    }
    let rows = e["deferred"].as_array().unwrap();
    assert_eq!(
        v.deferred.len(),
        rows.len(),
        "PRIVATE-ORDERS native deferred count"
    );
    for (v, e) in v.deferred.iter().zip(rows) {
        inspect_woo_v3_deferred_algo_order(v, e);
    }
}
fn mapped_type(e: &Value) -> OrderType {
    match e.as_str().unwrap() {
        "limit" => OrderType::Limit,
        "market" => OrderType::Market,
        "stop_market" => OrderType::StopLoss,
        "take_profit_market" => OrderType::TakeProfit,
        _ => panic!("type"),
    }
}
fn order_side(e: &Value) -> OrderSide {
    match e.as_str().unwrap() {
        "buy" => OrderSide::Buy,
        "sell" => OrderSide::Sell,
        _ => panic!("order side"),
    }
}
fn time_force(e: &Value) -> OrderTimeInForce {
    match e.as_str().unwrap() {
        "GoodTillCancel" => OrderTimeInForce::GoodTillCancel,
        "ImmediateOrCancel" => OrderTimeInForce::ImmediateOrCancel,
        "FillOrKill" => OrderTimeInForce::FillOrKill,
        "PostOnly" => OrderTimeInForce::PostOnly,
        _ => panic!("TIF"),
    }
}
fn trigger(e: &Value) -> OrderStopTriggerType {
    match e.as_str().unwrap() {
        "index_price" => OrderStopTriggerType::IndexPrice,
        "last_price" => OrderStopTriggerType::LastPrice,
        _ => panic!("trigger"),
    }
}
trait InspectInfo {
    fn inspect(&self, e: &Value);
}
impl InspectInfo for WooV3RegularOrder {
    fn inspect(&self, e: &Value) {
        inspect_woo_v3_regular_order(self, e);
    }
}
impl InspectInfo for WooV3AlgoOrder {
    fn inspect(&self, e: &Value) {
        inspect_woo_v3_algo_order(self, e);
    }
}
fn inspect_mapped_order<T: InspectInfo>(v: &WooV3MappedOrder<T>, e: &Value) {
    assert_eq!(v.id, e["id"].as_str().unwrap());
    assert_eq!(
        v.parent_id.as_deref(),
        e.get("parentId").and_then(Value::as_str),
        "PRIVATE-ORDERS native parentId"
    );
    assert_eq!(v.status, OrderStatus::Open);
    assert_eq!(e["status"], "open");
    assert_eq!(v.symbol, e["symbol"].as_str().unwrap());
    assert_eq!(v.r#type, mapped_type(&e["type"]));
    assert_eq!(v.side, order_side(&e["side"]));
    for (n, key) in [
        (v.price, "price"),
        (v.amount, "amount"),
        (v.filled, "filled"),
        (v.remaining, "remaining"),
        (v.last_update, "lastUpdate"),
    ] {
        number(n, &e[key], key);
    }
    assert_eq!(v.reduce_only, e["reduceOnly"].as_bool().unwrap());
    assert_eq!(
        v.time_in_force,
        e.get("timeInForce").map(time_force),
        "PRIVATE-ORDERS native timeInForce"
    );
    assert_eq!(v.tp_trigger_type, e.get("tpTriggerType").map(trigger));
    assert_eq!(v.sl_trigger_type, e.get("slTriggerType").map(trigger));
    v.info.inspect(&e["info"]);
}
fn execution(v: &Execution, e: &Value) {
    assert_eq!(v.id.as_deref(), e["id"].as_str());
    assert_eq!(v.order_id.as_deref(), e["orderId"].as_str());
    assert_eq!(v.symbol, e["symbol"].as_str().unwrap());
    assert_eq!(v.side, order_side(&e["side"]));
    number(v.timestamp, &e["timestamp"], "execution timestamp");
    number(v.price, &e["price"], "execution price");
    number(v.qty, &e["qty"], "execution qty");
    number(v.commission.unwrap(), &e["commission"], "execution fee");
    assert_eq!(v.commission_asset.as_deref(), e["commissionAsset"].as_str());
    assert!(
        v.position_side.is_none()
            && v.realized_pnl.is_none()
            && v.maker.is_none()
            && v.venue_reported_revenue.is_none()
            && v.venue_reported_revenue_asset.is_none()
    );
}
fn historical(v: &HistoricalTrade, e: &Value) {
    assert_eq!(v.id.as_deref(), e["id"].as_str());
    assert_eq!(v.order_id, e["orderId"].as_str().unwrap());
    assert_eq!(v.symbol, e["symbol"].as_str().unwrap());
    assert_eq!(v.side, order_side(&e["side"]));
    number(v.price, &e["price"], "historical price");
    number(v.qty, &e["qty"], "historical qty");
    assert_eq!(v.realized_pnl.is_some(), e.get("realizedPnl").is_some());
    if let Some(n) = v.realized_pnl {
        number(n, &e["realizedPnl"], "historical PnL");
    }
    assert_eq!(
        v.position_side,
        ExecutionPositionSide::Both(BothPositionSide::Both)
    );
    assert_eq!(e["positionSide"], "both");
    assert_eq!(v.maker, e["maker"].as_bool());
    number(v.commission.unwrap(), &e["commission"], "historical fee");
    assert_eq!(v.commission_asset.as_deref(), e["commissionAsset"].as_str());
    number(v.time, &e["time"], "historical time");
    assert!(v.quote_qty.is_none() && v.buyer.is_none());
}
fn meta(v: &WooV3PageMeta, e: &Value) {
    number(v.total, &e["total"], "meta total");
    number(v.records_per_page, &e["recordsPerPage"], "meta per page");
    number(v.current_page, &e["currentPage"], "meta current page");
}
fn page<T>(v: WooV3PaginatedRows<T>, e: &Value, inspect: impl Fn(&T, &Value)) {
    meta(&v.meta, &e["meta"]);
    let rows = e["rows"].as_array().unwrap();
    assert_eq!(v.rows.len(), rows.len());
    for (v, e) in v.rows.iter().zip(rows) {
        inspect(v, e);
    }
}
#[test]
fn native_declarations() {
    let o = source();
    let rows = o["declarations"].as_array().unwrap();
    assert_eq!(rows.len(), 10);
    let expected = [
        "WooV3RegularOrder",
        "WooV3RegularOrder",
        "WooV3AlgoOrder",
        "WooV3AlgoOrder",
        "WooV3AlgoOrder",
        "WooV3Transaction",
        "WooV3MappedTransaction",
        "WooV3MappedTransaction",
        "WooV3DeferredAlgoOrder",
        "WooV3AlgoOrderMapping",
    ];
    for (row, name) in rows.iter().zip(expected) {
        assert_eq!(row["type"], name);
        let input = row["input"].as_str().unwrap();
        let e = &row["expected"];
        match name {
            "WooV3RegularOrder" => {
                inspect_woo_v3_regular_order(&js_from_str::<WooV3RegularOrder>(input).unwrap(), e)
            }
            "WooV3AlgoOrder" => {
                inspect_woo_v3_algo_order(&js_from_str::<WooV3AlgoOrder>(input).unwrap(), e)
            }
            "WooV3Transaction" => {
                inspect_woo_v3_transaction(&js_from_str::<WooV3Transaction>(input).unwrap(), e)
            }
            "WooV3MappedTransaction" => inspect_woo_v3_mapped_transaction(
                &js_from_str::<WooV3MappedTransaction>(input).unwrap(),
                e,
            ),
            "WooV3DeferredAlgoOrder" => inspect_woo_v3_deferred_algo_order(
                &js_from_str::<WooV3DeferredAlgoOrder>(input).unwrap(),
                e,
            ),
            "WooV3AlgoOrderMapping" => inspect_woo_v3_algo_order_mapping(
                &js_from_str::<WooV3AlgoOrderMapping>(input).unwrap(),
                e,
            ),
            _ => unreachable!(),
        }
    }
    let values = o["enumValues"]["WooV3OrderSide"].as_array().unwrap();
    assert_eq!(values.len(), 2);
    for value in values {
        let wire = value.as_str().unwrap();
        assert_eq!(
            js_from_str::<WooV3OrderSide>(&serde_json::to_string(wire).unwrap()).unwrap(),
            expected_woo_v3_order_side(wire),
            "PRIVATE-ORDERS native enum WooV3OrderSide"
        );
    }
    for bad in ["null", "0", "true", "[]", "{}", "\"bad\""] {
        assert!(js_from_str::<WooV3OrderSide>(bad).is_err());
    }
    let values = o["enumValues"]["WooV3OrderStatus"].as_array().unwrap();
    assert_eq!(values.len(), 5);
    for value in values {
        let wire = value.as_str().unwrap();
        assert_eq!(
            js_from_str::<WooV3OrderStatus>(&serde_json::to_string(wire).unwrap()).unwrap(),
            expected_woo_v3_order_status(wire),
            "PRIVATE-ORDERS native enum WooV3OrderStatus"
        );
    }
    for bad in ["null", "0", "true", "[]", "{}", "\"bad\""] {
        assert!(js_from_str::<WooV3OrderStatus>(bad).is_err());
    }
    let values = o["enumValues"]["WooV3RegularOrderType"].as_array().unwrap();
    assert_eq!(values.len(), 12);
    for value in values {
        let wire = value.as_str().unwrap();
        assert_eq!(
            js_from_str::<WooV3RegularOrderType>(&serde_json::to_string(wire).unwrap()).unwrap(),
            expected_woo_v3_regular_order_type(wire),
            "PRIVATE-ORDERS native enum WooV3RegularOrderType"
        );
    }
    for bad in ["null", "0", "true", "[]", "{}", "\"bad\""] {
        assert!(js_from_str::<WooV3RegularOrderType>(bad).is_err());
    }
    let values = o["enumValues"]["WooV3AlgoType"].as_array().unwrap();
    assert_eq!(values.len(), 9);
    for value in values {
        let wire = value.as_str().unwrap();
        assert_eq!(
            js_from_str::<WooV3AlgoType>(&serde_json::to_string(wire).unwrap()).unwrap(),
            expected_woo_v3_algo_type(wire),
            "PRIVATE-ORDERS native enum WooV3AlgoType"
        );
    }
    for bad in ["null", "0", "true", "[]", "{}", "\"bad\""] {
        assert!(js_from_str::<WooV3AlgoType>(bad).is_err());
    }
    let values = o["enumValues"]["WooV3TriggerPriceType"].as_array().unwrap();
    assert_eq!(values.len(), 3);
    for value in values {
        let wire = value.as_str().unwrap();
        assert_eq!(
            js_from_str::<WooV3TriggerPriceType>(&serde_json::to_string(wire).unwrap()).unwrap(),
            expected_woo_v3_trigger_price_type(wire),
            "PRIVATE-ORDERS native enum WooV3TriggerPriceType"
        );
    }
    for bad in ["null", "0", "true", "[]", "{}", "\"bad\""] {
        assert!(js_from_str::<WooV3TriggerPriceType>(bad).is_err());
    }
    let values = o["enumValues"]["WooV3TriggerStatus"].as_array().unwrap();
    assert_eq!(values.len(), 4);
    for value in values {
        let wire = value.as_str().unwrap();
        assert_eq!(
            js_from_str::<WooV3TriggerStatus>(&serde_json::to_string(wire).unwrap()).unwrap(),
            expected_woo_v3_trigger_status(wire),
            "PRIVATE-ORDERS native enum WooV3TriggerStatus"
        );
    }
    for bad in ["null", "0", "true", "[]", "{}", "\"bad\""] {
        assert!(js_from_str::<WooV3TriggerStatus>(bad).is_err());
    }
    let values = o["enumValues"]["WooV3MarginMode"].as_array().unwrap();
    assert_eq!(values.len(), 2);
    for value in values {
        let wire = value.as_str().unwrap();
        assert_eq!(
            js_from_str::<WooV3MarginMode>(&serde_json::to_string(wire).unwrap()).unwrap(),
            expected_woo_v3_margin_mode(wire),
            "PRIVATE-ORDERS native enum WooV3MarginMode"
        );
    }
    for bad in ["null", "0", "true", "[]", "{}", "\"bad\""] {
        assert!(js_from_str::<WooV3MarginMode>(bad).is_err());
    }
    for (text, e) in [
        ("-0", serde_json::json!({"$number":"-0"})),
        ("1e999", serde_json::json!({"$number":"Infinity"})),
    ] {
        quantity(&js_from_str::<WooV3GatewayAlgoQuantity>(text).unwrap(), &e);
    }
    for bad in ["null", "true", "[]", "{}"] {
        assert!(js_from_str::<WooV3GatewayAlgoQuantity>(bad).is_err());
    }
    // Required nullable PnL differs from an optional field: id=0 is declaration-valid.
    let missing = o["transactions"][4]["input"].as_str().unwrap();
    let error = js_from_str::<WooV3Transaction>(missing)
        .expect_err("PRIVATE-ORDERS native required nullable PnL");
    assert!(
        error.to_string().contains("realizedPnl"),
        "PRIVATE-ORDERS native missing PnL field: {error}"
    );
}
#[test]
fn native_mapping_helpers() {
    let o = source();
    let rows = o["helpers"].as_array().unwrap();
    assert_eq!(rows.len(), 22);
    for (i, row) in rows.iter().enumerate() {
        let value = input(row, "input").unwrap().decode::<String>().unwrap();
        let expected = if i < 2 {
            "mapSide"
        } else if i < 7 {
            "mapPendingStatus"
        } else if i < 19 {
            "mapRegularOrderType"
        } else {
            "mapTriggerPriceType"
        };
        assert_eq!(row["name"], expected);
        match expected {
            "mapSide" => assert_eq!(
                map_side(expected_woo_v3_order_side(&value)),
                order_side(&row["ok"])
            ),
            "mapPendingStatus" => checked(
                map_pending_status(expected_woo_v3_order_status(&value), "probe"),
                row,
                |v, e| {
                    assert_eq!(v, OrderStatus::Open);
                    assert_eq!(e, "open");
                },
            ),
            "mapRegularOrderType" => checked(
                map_regular_order_type(expected_woo_v3_regular_order_type(&value), "probe"),
                row,
                |v, e| {
                    assert_eq!(v.r#type, mapped_type(&e["type"]));
                    assert_eq!(
                        v.time_in_force,
                        e.get("timeInForce").map(time_force),
                        "PRIVATE-ORDERS native timeInForce"
                    );
                },
            ),
            "mapTriggerPriceType" => assert_eq!(
                map_trigger_price_type(expected_woo_v3_trigger_price_type(&value)),
                trigger(&row["ok"])
            ),
            _ => unreachable!(),
        }
    }
}
#[test]
fn native_regular_orders() {
    let o = source();
    let rows = o["regularRows"].as_array().unwrap();
    assert_eq!(rows.len(), 19);
    for row in rows {
        let v = input(row, "input");
        checked(decode_regular_order(v.as_ref(), "probe"), row, |v, e| {
            inspect_woo_v3_regular_order(&v, e)
        });
    }
    let rows = o["regularMaps"].as_array().unwrap();
    assert_eq!(rows.len(), 12);
    for row in rows {
        let v = js_from_str::<Vec<WooV3RegularOrder>>(row["input"].as_str().unwrap()).unwrap();
        checked(
            map_woo_v3_pending_orders(&v, mode(&row["mode"])),
            row,
            |v, e| {
                let e = e.as_array().unwrap();
                assert_eq!(v.len(), e.len());
                for (v, e) in v.iter().zip(e) {
                    inspect_mapped_order(v, e);
                }
            },
        );
    }
    let mut original =
        js_from_str::<Vec<WooV3RegularOrder>>(rows[0]["input"].as_str().unwrap()).unwrap();
    let mapped = map_woo_v3_pending_orders(&original, WooV3AccountPositionMode::OneWay).unwrap();
    original[0].quantity = "999".into();
    assert_eq!(
        mapped[0].info.quantity, "3.25",
        "PRIVATE-ORDERS native owned order info"
    );
}
#[test]
fn native_algo_decoding() {
    let o = source();
    let rows = o["quantities"].as_array().unwrap();
    assert_eq!(rows.len(), 11);
    for row in rows {
        let v = input(row, "input");
        checked(decode_algo_quantity(v.as_ref(), "probe"), row, |v, e| {
            quantity(&v, e)
        });
    }
    let rows = o["algoRows"].as_array().unwrap();
    assert_eq!(rows.len(), 16);
    for row in rows {
        let v = input(row, "input");
        checked(decode_algo_order(v.as_ref(), "probe", 0), row, |v, e| {
            inspect_woo_v3_algo_order(&v, e)
        });
    }
    let rows = o["directDepth"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    for row in rows {
        let v = input(row, "input");
        checked(
            decode_algo_order(v.as_ref(), "probe", row["depth"].as_u64().unwrap() as usize),
            row,
            |v, e| inspect_woo_v3_algo_order(&v, e),
        );
    }
}
#[test]
fn native_algo_mapping() {
    let o = source();
    let rows = o["algoMaps"].as_array().unwrap();
    assert_eq!(rows.len(), 13);
    for row in rows {
        let v = js_from_str::<Vec<WooV3AlgoOrder>>(row["input"].as_str().unwrap()).unwrap();
        checked(
            map_woo_v3_pending_algo_orders(&v, mode(&row["mode"])),
            row,
            |v, e| inspect_woo_v3_algo_order_mapping(&v, e),
        );
    }
}
#[test]
fn native_transactions() {
    let o = source();
    let rows = o["transactions"].as_array().unwrap();
    assert_eq!(rows.len(), 11);
    for row in rows {
        let v = input(row, "input");
        checked(decode_transaction(v.as_ref(), "probe"), row, |v, e| {
            inspect_woo_v3_transaction(&v, e)
        });
    }
    let rows = o["transactionMaps"].as_array().unwrap();
    assert_eq!(rows.len(), 9);
    for row in rows {
        let v = js_from_str::<Vec<WooV3Transaction>>(row["input"].as_str().unwrap()).unwrap();
        checked(
            map_woo_v3_transactions(&v, mode(&row["mode"])),
            row,
            |v, e| {
                let e = e.as_array().unwrap();
                assert_eq!(v.len(), e.len());
                for (v, e) in v.iter().zip(e) {
                    inspect_woo_v3_mapped_transaction(v, e);
                }
            },
        );
    }
}
#[test]
fn native_paginated_order_reads() {
    let o = source();
    let rows = o["pages"].as_array().unwrap();
    assert_eq!(rows.len(), 18);
    for (i, row) in rows.iter().enumerate() {
        let name = if i < 6 {
            "decodeWooV3PendingOrders"
        } else if i < 12 {
            "decodeWooV3PendingAlgoOrders"
        } else {
            "decodeWooV3Transactions"
        };
        assert_eq!(row["name"], name);
        let v = input(row, "input");
        match name {
            "decodeWooV3PendingOrders" => {
                checked(decode_woo_v3_pending_orders(v.as_ref()), row, |v, e| {
                    page(v, e, inspect_woo_v3_regular_order)
                })
            }
            "decodeWooV3PendingAlgoOrders" => checked(
                decode_woo_v3_pending_algo_orders(v.as_ref()),
                row,
                |v, e| page(v, e, inspect_woo_v3_algo_order),
            ),
            "decodeWooV3Transactions" => {
                checked(decode_woo_v3_transactions(v.as_ref()), row, |v, e| {
                    page(v, e, inspect_woo_v3_transaction)
                })
            }
            _ => unreachable!(),
        }
    }
}
