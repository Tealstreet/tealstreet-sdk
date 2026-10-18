use safe_cex_num::numeric_ingress::{
    JsNumberOutput, js_from_str, js_from_value, js_from_value_ref,
};
use safe_cex_venue_paradex::{constants::*, types::*};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Map, Value, json};
use std::process::Command;
use std::sync::OnceLock;

trait Observe {
    fn observe(&self, path: &str, out: &mut Map<String, Value>);
}
impl Observe for String {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        o.insert(p.into(), json!({"type":"string","value":self}));
    }
}
impl Observe for bool {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        o.insert(p.into(), json!({"type":"boolean","value":self}));
    }
}
impl Observe for f64 {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        o.insert(p.into(), json!({"type":"number","bits":format!("{:016x}", if self.is_nan(){0x7ff8000000000000} else {self.to_bits()})}));
    }
}
impl<T: Observe> Observe for Option<T> {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        if let Some(v) = self {
            v.observe(p, o)
        } else {
            o.insert(p.into(), json!({"type":"undefined"}));
        }
    }
}
impl<T: Observe> Observe for Vec<T> {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        o.insert(format!("{p}.__cardinality"), json!(self.len()));
        for (i, v) in self.iter().enumerate() {
            v.observe(&format!("{p}.{i}"), o)
        }
    }
}
impl<A: Observe, B: Observe> Observe for (A, B) {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        o.insert(format!("{p}.__cardinality"), json!(2));
        self.0.observe(&format!("{p}.0"), o);
        self.1.observe(&format!("{p}.1"), o);
    }
}
fn field(p: &str, n: &str) -> String {
    if p.is_empty() {
        n.into()
    } else {
        format!("{p}.{n}")
    }
}
fn snapshot<T: Observe>(v: &T) -> Value {
    let mut out = Map::new();
    v.observe("", &mut out);
    out.retain(|key, _| !key.ends_with(".__cardinality"));
    Value::Object(out)
}
// Count native Vec/tuple elements before serialization, independently of leaf observations.
fn native_cardinality<T: Observe>(v: &T) -> Value {
    let mut out = Map::new();
    v.observe("", &mut out);
    Value::Object(
        out.into_iter()
            .filter_map(|(k, v)| k.strip_suffix(".__cardinality").map(|p| (p.to_owned(), v)))
            .collect(),
    )
}
fn source_cardinality(v: &Value, path: &str, out: &mut Map<String, Value>) {
    match v {
        Value::Array(values) => {
            out.insert(path.to_owned(), json!(values.len()));
            for (i, v) in values.iter().enumerate() {
                source_cardinality(v, &format!("{path}.{i}"), out);
            }
        }
        Value::Object(values) => {
            for (k, v) in values {
                source_cardinality(v, &field(path, k), out);
            }
        }
        _ => {}
    }
}
fn census() -> Value {
    serde_json::from_str(include_str!("types_cardinality.json")).unwrap()
}
impl Observe for MarketResponseStatus {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        let value = match self {
            Self::Active => "ACTIVE",
            Self::Inactive => "INACTIVE",
        };
        o.insert(p.into(), json!({"type":"string","value":value}));
    }
}
impl Observe for OrderResponseSide {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        let value = match self {
            Self::Buy => "BUY",
            Self::Sell => "SELL",
        };
        o.insert(p.into(), json!({"type":"string","value":value}));
    }
}
impl Observe for OrderResponseStatus {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        let value = match self {
            Self::New => "NEW",
            Self::Open => "OPEN",
            Self::Pending => "PENDING",
            Self::Filled => "FILLED",
            Self::Canceled => "CANCELED",
            Self::Untriggered => "UNTRIGGERED",
            Self::Rejected => "REJECTED",
        };
        o.insert(p.into(), json!({"type":"string","value":value}));
    }
}
impl Observe for OrderResponseType {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        let value = match self {
            Self::Limit => "LIMIT",
            Self::Market => "MARKET",
            Self::StopLimit => "STOP_LIMIT",
            Self::StopMarket => "STOP_MARKET",
            Self::StopLossLimit => "STOP_LOSS_LIMIT",
            Self::StopLossMarket => "STOP_LOSS_MARKET",
            Self::TakeProfitLimit => "TAKE_PROFIT_LIMIT",
            Self::TakeProfitMarket => "TAKE_PROFIT_MARKET",
        };
        o.insert(p.into(), json!({"type":"string","value":value}));
    }
}
impl Observe for OrderResponseInstruction {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        let value = match self {
            Self::Gtc => "GTC",
            Self::PostOnly => "POST_ONLY",
            Self::Ioc => "IOC",
            Self::Rpi => "RPI",
        };
        o.insert(p.into(), json!({"type":"string","value":value}));
    }
}
impl Observe for OrderResponseStp {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        let value = match self {
            Self::ExpireMaker => "EXPIRE_MAKER",
            Self::ExpireTaker => "EXPIRE_TAKER",
            Self::ExpireBoth => "EXPIRE_BOTH",
        };
        o.insert(p.into(), json!({"type":"string","value":value}));
    }
}
impl Observe for OrderResponseTimeInForce {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        let value = match self {
            Self::Gtt => "GTT",
            Self::Fok => "FOK",
            Self::Ioc => "IOC",
        };
        o.insert(p.into(), json!({"type":"string","value":value}));
    }
}
impl Observe for BatchCancelOrderResultStatus {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        let value = match self {
            Self::QueuedForCancellation => "QUEUED_FOR_CANCELLATION",
            Self::AlreadyClosed => "ALREADY_CLOSED",
            Self::NotFound => "NOT_FOUND",
        };
        o.insert(p.into(), json!({"type":"string","value":value}));
    }
}
impl Observe for PositionResponseStatus {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        let value = match self {
            Self::Open => "OPEN",
            Self::Closed => "CLOSED",
            Self::Liquidated => "LIQUIDATED",
        };
        o.insert(p.into(), json!({"type":"string","value":value}));
    }
}
impl Observe for PositionResponseSide {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        let value = match self {
            Self::Long => "LONG",
            Self::Short => "SHORT",
        };
        o.insert(p.into(), json!({"type":"string","value":value}));
    }
}
impl Observe for TradeResponseSide {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        let value = match self {
            Self::Buy => "BUY",
            Self::Sell => "SELL",
        };
        o.insert(p.into(), json!({"type":"string","value":value}));
    }
}
impl Observe for MarginConfigEntryMarginType {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        let value = match self {
            Self::Cross => "CROSS",
            Self::Isolated => "ISOLATED",
        };
        o.insert(p.into(), json!({"type":"string","value":value}));
    }
}
impl Observe for WsMessageType {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        let value = match self {
            Self::Subscribe => "subscribe",
            Self::Unsubscribe => "unsubscribe",
            Self::Ping => "ping",
            Self::Pong => "pong",
            Self::Error => "error",
            Self::Trades => "trades",
            Self::Orderbook => "orderbook",
            Self::Ticker => "ticker",
            Self::Orders => "orders",
            Self::Positions => "positions",
            Self::Balances => "balances",
        };
        o.insert(p.into(), json!({"type":"string","value":value}));
    }
}
impl Observe for AccountResponse {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        self.account.observe(&field(p, "account"), o);
        self.account_number.observe(&field(p, "account_number"), o);
        self.address.observe(&field(p, "address"), o);
        self.public_key.observe(&field(p, "public_key"), o);
        self.initial_margin_requirement
            .observe(&field(p, "initial_margin_requirement"), o);
        self.maintenance_margin_requirement
            .observe(&field(p, "maintenance_margin_requirement"), o);
        self.account_value.observe(&field(p, "account_value"), o);
        self.total_collateral
            .observe(&field(p, "total_collateral"), o);
        self.free_collateral
            .observe(&field(p, "free_collateral"), o);
        self.margin_cushion.observe(&field(p, "margin_cushion"), o);
        self.settlement_asset
            .observe(&field(p, "settlement_asset"), o);
        self.updated_at.observe(&field(p, "updated_at"), o);
        self.status.observe(&field(p, "status"), o);
        self.seq_no.observe(&field(p, "seq_no"), o);
    }
}
impl Observe for BalanceResponse {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        self.asset.observe(&field(p, "asset"), o);
        self.free.observe(&field(p, "free"), o);
        self.locked.observe(&field(p, "locked"), o);
        self.wallet_balance.observe(&field(p, "wallet_balance"), o);
        self.unrealized_pnl.observe(&field(p, "unrealized_pnl"), o);
        self.margin_balance.observe(&field(p, "margin_balance"), o);
    }
}
impl Observe for MarketResponseDelta1CrossMarginParams {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        self.imf_base.observe(&field(p, "imf_base"), o);
        self.imf_factor.observe(&field(p, "imf_factor"), o);
        self.imf_shift.observe(&field(p, "imf_shift"), o);
        self.mmf_factor.observe(&field(p, "mmf_factor"), o);
    }
}
impl Observe for MarketResponse {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        self.market.observe(&field(p, "market"), o);
        self.symbol.observe(&field(p, "symbol"), o);
        self.status.observe(&field(p, "status"), o);
        self.base_currency.observe(&field(p, "base_currency"), o);
        self.quote_currency.observe(&field(p, "quote_currency"), o);
        self.settlement_currency
            .observe(&field(p, "settlement_currency"), o);
        self.step_size.observe(&field(p, "step_size"), o);
        self.order_size_increment
            .observe(&field(p, "order_size_increment"), o);
        self.tick_size.observe(&field(p, "tick_size"), o);
        self.price_tick_size
            .observe(&field(p, "price_tick_size"), o);
        self.index_price.observe(&field(p, "index_price"), o);
        self.oracle_price.observe(&field(p, "oracle_price"), o);
        self.min_order_size.observe(&field(p, "min_order_size"), o);
        self.min_notional.observe(&field(p, "min_notional"), o);
        self.max_order_size.observe(&field(p, "max_order_size"), o);
        self.max_position_size
            .observe(&field(p, "max_position_size"), o);
        self.position_limit.observe(&field(p, "position_limit"), o);
        self.initial_margin_fraction
            .observe(&field(p, "initial_margin_fraction"), o);
        self.maintenance_margin_fraction
            .observe(&field(p, "maintenance_margin_fraction"), o);
        self.baseline_position_size
            .observe(&field(p, "baseline_position_size"), o);
        self.incremental_position_size
            .observe(&field(p, "incremental_position_size"), o);
        self.incremental_initial_margin_fraction
            .observe(&field(p, "incremental_initial_margin_fraction"), o);
        self.delta1_cross_margin_params
            .observe(&field(p, "delta1_cross_margin_params"), o);
        self.maker_fee.observe(&field(p, "maker_fee"), o);
        self.taker_fee.observe(&field(p, "taker_fee"), o);
        self.asset_kind.observe(&field(p, "asset_kind"), o);
        self.market_kind.observe(&field(p, "market_kind"), o);
        self.option_type.observe(&field(p, "option_type"), o);
        self.strike_price.observe(&field(p, "strike_price"), o);
        self.expiry_at.observe(&field(p, "expiry_at"), o);
        self.open_at.observe(&field(p, "open_at"), o);
        self.max_open_orders
            .observe(&field(p, "max_open_orders"), o);
        self.price_bands_width
            .observe(&field(p, "price_bands_width"), o);
        self.max_slippage.observe(&field(p, "max_slippage"), o);
    }
}
impl Observe for OrderResponse {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        self.id.observe(&field(p, "id"), o);
        self.client_id.observe(&field(p, "client_id"), o);
        self.client_order_id
            .observe(&field(p, "client_order_id"), o);
        self.market.observe(&field(p, "market"), o);
        self.side.observe(&field(p, "side"), o);
        self.size.observe(&field(p, "size"), o);
        self.remaining_size.observe(&field(p, "remaining_size"), o);
        self.price.observe(&field(p, "price"), o);
        self.average_fill_price
            .observe(&field(p, "average_fill_price"), o);
        self.status.observe(&field(p, "status"), o);
        self.r#type.observe(&field(p, "type"), o);
        self.instruction.observe(&field(p, "instruction"), o);
        self.flags.observe(&field(p, "flags"), o);
        self.stp.observe(&field(p, "stp"), o);
        self.on_behalf_of_account
            .observe(&field(p, "on_behalf_of_account"), o);
        self.time_in_force.observe(&field(p, "time_in_force"), o);
        self.created_at.observe(&field(p, "created_at"), o);
        self.updated_at.observe(&field(p, "updated_at"), o);
        self.expires_at.observe(&field(p, "expires_at"), o);
        self.trigger_price.observe(&field(p, "trigger_price"), o);
        self.post_only.observe(&field(p, "post_only"), o);
        self.reduce_only.observe(&field(p, "reduce_only"), o);
    }
}
impl Observe for BatchOrderError {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        self.error.observe(&field(p, "error"), o);
        self.message.observe(&field(p, "message"), o);
    }
}
impl Observe for BatchOrderResponse {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        self.errors.observe(&field(p, "errors"), o);
        self.orders.observe(&field(p, "orders"), o);
    }
}
impl Observe for BatchCancelOrderResult {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        self.account.observe(&field(p, "account"), o);
        self.client_id.observe(&field(p, "client_id"), o);
        self.id.observe(&field(p, "id"), o);
        self.market.observe(&field(p, "market"), o);
        self.status.observe(&field(p, "status"), o);
    }
}
impl Observe for BatchCancelOrderResponse {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        self.results.observe(&field(p, "results"), o);
    }
}
impl Observe for PositionResponse {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        self.id.observe(&field(p, "id"), o);
        self.account.observe(&field(p, "account"), o);
        self.market.observe(&field(p, "market"), o);
        self.status.observe(&field(p, "status"), o);
        self.side.observe(&field(p, "side"), o);
        self.size.observe(&field(p, "size"), o);
        self.average_entry_price
            .observe(&field(p, "average_entry_price"), o);
        self.average_entry_price_usd
            .observe(&field(p, "average_entry_price_usd"), o);
        self.average_exit_price
            .observe(&field(p, "average_exit_price"), o);
        self.unrealized_pnl.observe(&field(p, "unrealized_pnl"), o);
        self.unrealized_funding_pnl
            .observe(&field(p, "unrealized_funding_pnl"), o);
        self.cost.observe(&field(p, "cost"), o);
        self.cost_usd.observe(&field(p, "cost_usd"), o);
        self.cached_funding_index
            .observe(&field(p, "cached_funding_index"), o);
        self.last_updated_at
            .observe(&field(p, "last_updated_at"), o);
        self.created_at.observe(&field(p, "created_at"), o);
        self.last_fill_id.observe(&field(p, "last_fill_id"), o);
        self.seq_no.observe(&field(p, "seq_no"), o);
        self.liquidation_price
            .observe(&field(p, "liquidation_price"), o);
        self.leverage.observe(&field(p, "leverage"), o);
        self.realized_positional_pnl
            .observe(&field(p, "realized_positional_pnl"), o);
        self.realized_positional_funding_pnl
            .observe(&field(p, "realized_positional_funding_pnl"), o);
    }
}
impl Observe for TickerResponseGreeks {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        self.delta.observe(&field(p, "delta"), o);
        self.gamma.observe(&field(p, "gamma"), o);
        self.vega.observe(&field(p, "vega"), o);
        self.rho.observe(&field(p, "rho"), o);
        self.vanna.observe(&field(p, "vanna"), o);
        self.volga.observe(&field(p, "volga"), o);
    }
}
impl Observe for TickerResponse {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        self.symbol.observe(&field(p, "symbol"), o);
        self.mark_price.observe(&field(p, "mark_price"), o);
        self.last_traded_price
            .observe(&field(p, "last_traded_price"), o);
        self.bid.observe(&field(p, "bid"), o);
        self.ask.observe(&field(p, "ask"), o);
        self.volume_24h.observe(&field(p, "volume_24h"), o);
        self.open_interest.observe(&field(p, "open_interest"), o);
        self.funding_rate.observe(&field(p, "funding_rate"), o);
        self.price_change_rate_24h
            .observe(&field(p, "price_change_rate_24h"), o);
        self.underlying_price
            .observe(&field(p, "underlying_price"), o);
        self.delta.observe(&field(p, "delta"), o);
        self.mark_iv.observe(&field(p, "mark_iv"), o);
        self.greeks.observe(&field(p, "greeks"), o);
    }
}
impl Observe for OrderbookResponse {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        self.bids.observe(&field(p, "bids"), o);
        self.asks.observe(&field(p, "asks"), o);
        self.timestamp.observe(&field(p, "timestamp"), o);
    }
}
impl Observe for TradeResponse {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        self.id.observe(&field(p, "id"), o);
        self.price.observe(&field(p, "price"), o);
        self.size.observe(&field(p, "size"), o);
        self.side.observe(&field(p, "side"), o);
        self.created_at.observe(&field(p, "created_at"), o);
    }
}
impl Observe for MarginConfigEntry {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        self.market.observe(&field(p, "market"), o);
        self.leverage.observe(&field(p, "leverage"), o);
        self.margin_type.observe(&field(p, "margin_type"), o);
        self.isolated_margin_leverage
            .observe(&field(p, "isolated_margin_leverage"), o);
    }
}
impl Observe for MarginResponse {
    fn observe(&self, p: &str, o: &mut Map<String, Value>) {
        self.account.observe(&field(p, "account"), o);
        self.margin_methodology
            .observe(&field(p, "margin_methodology"), o);
        self.configs.observe(&field(p, "configs"), o);
    }
}

fn oracle() -> &'static Value {
    // Both tests consume one fresh corpus per test process, including parallel runs.
    static CORPUS: OnceLock<Value> = OnceLock::new();
    CORPUS.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/types_oracle.cjs"
            ))
            .output()
            .expect("node oracle required");
        assert!(
            output.status.success(),
            "actual_typescript_oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
        let arch = match std::env::consts::ARCH {
            "aarch64" => "arm64",
            "x86_64" => "x64",
            other => other,
        };
        assert_eq!(corpus["header"]["arch"], arch, "capture_cpu_architecture");
        assert!(
            corpus["header"]["nodeMajor"].as_u64().unwrap() > 0,
            "capture_node_major"
        );
        println!("capture_header {}", corpus["header"]);
        corpus
    })
}
// JSON projection is supplementary: native observations above retain signed zero.
fn json_equivalent(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| json_equivalent(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, a)| b.get(k).is_some_and(|b| json_equivalent(a, b)))
        }
        _ => a == b,
    }
}
fn check_record<T: DeserializeOwned + Serialize + JsNumberOutput + Observe>(record: &Value) {
    let name = record["name"].as_str().unwrap();
    let counts = &census()[name];
    for key in ["cases", "invalid", "surrogate"] {
        assert_eq!(
            record[key].as_array().unwrap().len() as u64,
            counts[key].as_u64().unwrap(),
            "type_source_cardinality {name} {key}"
        );
    }
    let mut compared = [0_u64; 3];
    for case in record["cases"].as_array().unwrap() {
        let raw = case["raw"].as_str().unwrap();
        let value: Value = serde_json::from_str(raw).unwrap();
        let decode = |result: Result<T, serde_json::Error>| {
            result.unwrap_or_else(|err| {
                panic!(
                    "numeric_ingress_preservation {name} {}: {err}",
                    case["label"]
                )
            })
        };
        let paths = [
            decode(serde_json::from_str::<T>(raw)),
            decode(serde_json::from_value::<T>(value.clone())),
            decode(T::deserialize(&value)),
            decode(js_from_str::<T>(raw)),
            decode(js_from_value::<T>(value.clone())),
            decode(js_from_value_ref::<T>(&value)),
        ];
        let mut source_counts = Map::new();
        source_cardinality(&case["serialized"], "", &mut source_counts);
        for (route, decoded) in paths.iter().enumerate() {
            assert_eq!(
                native_cardinality(decoded),
                Value::Object(source_counts.clone()),
                "native_collection_cardinality {name} {} route {route}",
                case["label"]
            );
            assert_eq!(
                snapshot(decoded),
                case["observed"],
                "native_field_observation {name} {} route {route}",
                case["label"]
            );
            assert!(
                json_equivalent(&serde_json::to_value(decoded).unwrap(), &case["serialized"]),
                "wire_serialization {name} {} route {route}",
                case["label"]
            );
            compared[0] += 1;
        }
    }
    for raw in record["invalid"].as_array().unwrap() {
        let raw = raw.as_str().unwrap();
        let value: Value = serde_json::from_str(raw).unwrap();
        assert!(
            serde_json::from_str::<T>(raw).is_err(),
            "declared_shape_refusal {name} {raw}"
        );
        assert!(
            serde_json::from_value::<T>(value.clone()).is_err(),
            "owned_shape_refusal {name} {raw}"
        );
        assert!(
            js_from_value_ref::<T>(&value).is_err(),
            "borrowed_shape_refusal {name} {raw}"
        );
        compared[1] += 3;
    }
    for raw in record["surrogate"].as_array().unwrap() {
        typed_utf16::assert_projection(
            raw.as_str().unwrap(),
            &(js_from_str::<T>(raw.as_str().unwrap())).expect("typed surrogate record is retained"),
        );
        compared[2] += 1;
    }
    assert_eq!(
        compared,
        [
            counts["cases"].as_u64().unwrap() * 6,
            counts["invalid"].as_u64().unwrap() * 3,
            counts["surrogate"].as_u64().unwrap()
        ],
        "type_completed_comparisons {name}"
    );
}
#[test]
fn actual_typescript_declarations_and_native_fields() {
    let oracle = oracle();
    let records = oracle["records"].as_array().unwrap();
    assert_eq!(records.len(), 15, "type_row_census");
    let names: std::collections::BTreeSet<_> = records
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    let expected = census();
    assert_eq!(
        names,
        expected
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect(),
        "type_unique_row_census"
    );
    for (key, total) in [("cases", 354_u64), ("invalid", 1056), ("surrogate", 166)] {
        let actual: u64 = records
            .iter()
            .map(|r| r[key].as_array().unwrap().len() as u64)
            .sum();
        assert_eq!(actual, total, "type_source_total {key}");
    }
    let mut checked_records = 0;
    for record in records {
        match record["name"].as_str().unwrap() {
            "ACCOUNT_RESPONSE" => check_record::<AccountResponse>(record),
            "BALANCE_RESPONSE" => check_record::<BalanceResponse>(record),
            "MARKET_RESPONSE" => check_record::<MarketResponse>(record),
            "ORDER_RESPONSE" => check_record::<OrderResponse>(record),
            "BATCH_ORDER_ERROR" => check_record::<BatchOrderError>(record),
            "BATCH_ORDER_RESPONSE" => check_record::<BatchOrderResponse>(record),
            "BATCH_CANCEL_ORDER_RESULT" => check_record::<BatchCancelOrderResult>(record),
            "BATCH_CANCEL_ORDER_RESPONSE" => check_record::<BatchCancelOrderResponse>(record),
            "POSITION_RESPONSE" => check_record::<PositionResponse>(record),
            "TICKER_RESPONSE" => check_record::<TickerResponse>(record),
            "ORDERBOOK_RESPONSE" => check_record::<OrderbookResponse>(record),
            "TRADE_RESPONSE" => check_record::<TradeResponse>(record),
            "MARGIN_CONFIG_ENTRY" => check_record::<MarginConfigEntry>(record),
            "MARGIN_RESPONSE" => check_record::<MarginResponse>(record),
            "WS_MESSAGE_TYPE" => check_record::<WsMessageType>(record),
            other => panic!("unreviewed TypeScript declaration {other}"),
        }
        checked_records += 1;
    }
    assert_eq!(checked_records, 15, "type_completed_records");
}
#[test]
fn actual_typescript_constants_and_predicates() {
    let oracle = oracle();
    let c = &oracle["constants"];
    assert_eq!(
        oracle["surrogatePredicates"],
        json!({"trigger":false,"market":false}),
        "predicate_surrogate_source_acceptance"
    );
    typed_utf16::assert_projection(
        r#""\ud800""#,
        &(js_from_str::<String>(r#""\ud800""#)).expect("typed surrogate record is retained"),
    );
    assert_eq!(
        json!({"type":"number","bits":format!("{:016x}",RECV_WINDOW.to_bits())}),
        c["RECV_WINDOW"],
        "RECV_WINDOW_before_serialization"
    );
    assert_eq!(json!(BASE_URL), c["BASE_URL"], "BASE_URL_entries");
    assert_eq!(json!(BASE_WS_URL), c["BASE_WS_URL"], "BASE_WS_URL_entries");
    assert_eq!(json!(ENDPOINTS), c["ENDPOINTS"], "ENDPOINTS_entries");
    assert_eq!(json!(ORDER_TYPE), c["ORDER_TYPE"], "ORDER_TYPE_entries");
    assert_eq!(json!(ORDER_SIDE), c["ORDER_SIDE"], "ORDER_SIDE_entries");
    assert_eq!(
        oracle["predicates"].as_array().unwrap().len(),
        27,
        "predicate_source_cardinality"
    );
    let mut compared = 0;
    for case in oracle["predicates"].as_array().unwrap() {
        let key = case["key"].as_str().unwrap();
        assert_eq!(
            is_paradex_trigger_order_type(key),
            case["trigger"].as_bool().unwrap(),
            "trigger_predicate {key}"
        );
        assert_eq!(
            is_paradex_market_order_type(key),
            case["market"].as_bool().unwrap(),
            "market_predicate {key}"
        );
        compared += 2;
    }
    assert_eq!(compared, 54, "predicate_completed_comparisons");
    check_lookups(ORDER_TYPE, &oracle["lookups"]["ORDER_TYPE"]);
    check_lookups(ORDER_SIDE, &oracle["lookups"]["ORDER_SIDE"]);
}
fn check_lookups<T: Copy + Serialize>(table: &[(&str, T)], cases: &Value) {
    assert_eq!(
        cases.as_array().unwrap().len(),
        27,
        "lookup_source_cardinality"
    );
    let mut compared = 0;
    for case in cases.as_array().unwrap() {
        let key = case["key"].as_str().unwrap();
        let (kind, text, own) = match safe_cex_num::js_table_lookup(table, key) {
            safe_cex_num::TableHit::Mapped(v) => (
                "string",
                serde_json::to_value(v)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_owned(),
                true,
            ),
            safe_cex_num::TableHit::Inherited(v) => (
                if v == safe_cex_num::ProtoMember::Prototype {
                    "object"
                } else {
                    "function"
                },
                v.js_string().into(),
                false,
            ),
            safe_cex_num::TableHit::Missing => ("undefined", "undefined".into(), false),
        };
        assert_eq!(
            json!({"key":key,"type":kind,"string":text,"own":own}),
            *case,
            "prototype_before_serialization {key}"
        );
        compared += 1;
    }
    assert_eq!(compared, 27, "lookup_completed_comparisons");
}

// Aggregate all seven fields so a shared consuming mutant proves each field in
// ONE corpus invocation, instead of stopping after the first failing owner.
#[test]
fn native_collection_lengths_across_all_response_owners() {
    let corpus = oracle();
    let mut failures = Vec::new();
    let mut compared = 0;
    macro_rules! check {
        ($name:literal, $ty:ty, [$($field:literal),+]) => {{
            let record = corpus["records"].as_array().unwrap().iter()
                .find(|r| r["name"] == $name).unwrap();
            let case = record["cases"].as_array().unwrap().iter()
                .find(|c| c["label"] == "all_fields").unwrap();
            let decoded: $ty = js_from_str(case["raw"].as_str().unwrap()).unwrap();
            let native = native_cardinality(&decoded);
            $(
                assert_eq!(case["serialized"][$field].as_array().unwrap().len(), 2,
                    "consuming_source_cardinality {}.{}", $name, $field);
                // Independent literal count: never derive this from decoded data.
                if native[$field] != json!(2) {
                    failures.push(format!("{}.{} native={}", $name, $field, native[$field]));
                }
                compared += 1;
            )+
        }};
    }
    check!("ORDER_RESPONSE", OrderResponse, ["flags"]);
    check!(
        "BATCH_ORDER_RESPONSE",
        BatchOrderResponse,
        ["errors", "orders"]
    );
    check!(
        "BATCH_CANCEL_ORDER_RESPONSE",
        BatchCancelOrderResponse,
        ["results"]
    );
    check!("ORDERBOOK_RESPONSE", OrderbookResponse, ["bids", "asks"]);
    check!("MARGIN_RESPONSE", MarginResponse, ["configs"]);
    assert_eq!(compared, 7, "consuming_completed_field_comparisons");
    assert!(
        failures.is_empty(),
        "consuming_native_collection_cardinality {failures:?}"
    );
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;
