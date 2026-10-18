//! Module imports the ported class bodies read by name, bound to the existing
//! Rust ports. A name the bodies reach that is not bound throws a visible
//! ReferenceError rather than resolving to a stand-in.
use super::{
    Realm,
    value::{PropKey, Thrown, Value},
};
use crate::{cloid, types};

fn arg(args: &[Value], i: usize) -> Value {
    args.get(i).cloned().unwrap_or(Value::Undefined)
}
fn table(r: &Realm, rows: &[(&str, &str)]) -> Value {
    r.object_from(rows.iter().map(|(k, v)| (*k, Value::str(v))).collect())
}
fn strings(r: &Realm, items: &[&str]) -> Value {
    r.new_array(items.iter().map(|s| Value::str(s)).collect())
}
/// A frozen TS string enum.
fn enumeration(r: &Realm, rows: &[(&str, &str)]) -> Value {
    let o = table(r, rows);
    if let Some(obj) = o.object() {
        let mut d = obj.borrow_mut();
        d.extensible = false;
        for p in &mut d.props {
            p.writable = false;
            p.configurable = false;
        }
    }
    o
}

pub(crate) fn bind(realm: &Realm) {
    let enums: [(&str, &[(&str, &str)]); 8] = [
        (
            "LogSeverity",
            &[
                ("Debug", "debug"),
                ("Info", "info"),
                ("Warning", "warning"),
                ("Error", "error"),
            ],
        ),
        ("OrderSide", &[("Buy", "buy"), ("Sell", "sell")]),
        (
            "OrderType",
            &[
                ("Market", "market"),
                ("Limit", "limit"),
                ("Conditional", "conditional"),
                ("StopLoss", "stop_market"),
                ("TakeProfit", "take_profit_market"),
                ("TrailingStopLoss", "trailing_stop_market"),
            ],
        ),
        (
            "OrderStatus",
            &[
                ("Open", "open"),
                ("Closed", "closed"),
                ("Canceled", "canceled"),
                ("Creating", "creating"),
                ("Updating", "updating"),
                ("Canceling", "canceling"),
            ],
        ),
        (
            "OrderTimeInForce",
            &[
                ("GoodTillCancel", "GoodTillCancel"),
                ("ImmediateOrCancel", "ImmediateOrCancel"),
                ("FillOrKill", "FillOrKill"),
                ("PostOnly", "PostOnly"),
            ],
        ),
        ("PositionSide", &[("Long", "long"), ("Short", "short")]),
        (
            "PositionMode",
            &[("Hedged", "hedged"), ("OneWay", "oneway")],
        ),
        (
            "MarginType",
            &[("Isolated", "isolated"), ("Cross", "cross")],
        ),
    ];
    for (name, rows) in enums {
        realm.bind_import(name, enumeration(realm, rows));
    }
    realm.bind_import("BASE_URLS", table(realm, types::BASE_URLS));
    realm.bind_import("ENDPOINTS", table(realm, types::ENDPOINTS));
    realm.bind_import("INFO_TYPES", table(realm, types::INFO_TYPES));
    realm.bind_import("EXCHANGE_TYPES", table(realm, types::EXCHANGE_TYPES));
    realm.bind_import(
        "GRAFANA_ENDPOINTS_EVERY",
        strings(realm, types::GRAFANA_ENDPOINTS_EVERY),
    );
    realm.bind_import(
        "GRAFANA_ENDPOINTS_PERIODIC",
        strings(realm, types::GRAFANA_ENDPOINTS_PERIODIC),
    );
    realm.bind_import(
        "SPOT_ASSET_INDEX_OFFSET",
        Value::Number(crate::constants::SPOT_ASSET_INDEX_OFFSET as f64),
    );
    realm.bind_import(
        "PERP_MAX_PRICE_DECIMALS",
        Value::Number(crate::constants::PERP_MAX_PRICE_DECIMALS as f64),
    );
    realm.bind_import(
        "SPOT_MAX_PRICE_DECIMALS",
        Value::Number(crate::constants::SPOT_MAX_PRICE_DECIMALS as f64),
    );

    // The imported singleton retains the shared server clock correction.
    let clock = realm.new_object();
    let now = realm.native("getCurrentTimeMs", 0, |_, _, _| {
        Ok(Value::Number(
            safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms(),
        ))
    });
    realm.define_hidden(&clock, PropKey::str("getCurrentTimeMs"), now);
    realm.bind_import("virtualClock", clock);

    super::host_spot::bind_spot_imports(realm).expect("shared dayjs import exists");
    bind_cloid(realm);
    bind_startup(realm);
    super::imports_utils::bind(realm);
    super::ccxt::bind(realm);
    super::imports_venue::bind(realm);
    super::host_cache::bind(realm);
}

/// Imported async helpers return a promise over the ported body.
fn bind_startup(realm: &Realm) {
    realm.bind_import(
        "fetchCoreMetaAndAssetCtxsWithFallback",
        realm.native("fetchCoreMetaAndAssetCtxsWithFallback", 1, |r, _, a| {
            let observer = crate::market_startup::fetch_core_meta_and_asset_ctxs_with_fallback(
                r.clone(),
                arg(a, 0),
            );
            Ok(r.spawn_value(Box::pin(observer)))
        }),
    );
    realm.bind_import(
        "fetchHip3MetaAndAssetCtxsWithRetry",
        realm.native("fetchHip3MetaAndAssetCtxsWithRetry", 1, |r, _, a| {
            let observer = crate::market_startup::fetch_hip3_meta_and_asset_ctxs_with_retry(
                r.clone(),
                arg(a, 0),
            );
            Ok(r.spawn_value(Box::pin(observer)))
        }),
    );
}

fn bind_cloid(realm: &Realm) {
    let rule = cloid::hyperliquid_client_order_id_rule();
    let o = realm.object_from(vec![
        ("label", Value::str(&rule.label)),
        ("requirement", Value::str(&rule.requirement)),
    ]);
    let is_valid = rule.is_valid.clone();
    let f = realm.native("isValid", 1, move |r, _, a| {
        let id = r.to_rust_string(&arg(a, 0))?;
        Ok(Value::Bool(is_valid(&id)))
    });
    realm.define_hidden(&o, PropKey::str("isValid"), f);
    let shape = rule.is_honorable_shape.clone();
    let f = realm.native("isHonorableShape", 1, move |r, _, a| {
        let opts: safe_cex_types::PreparedOrderOpts = r.to_typed(&arg(a, 0))?;
        Ok(Value::Bool(shape.as_ref().is_none_or(|check| check(&opts))))
    });
    realm.define_hidden(&o, PropKey::str("isHonorableShape"), f);
    let f = realm.native("mint", 0, |r, _, _| mint(r));
    realm.define_hidden(&o, PropKey::str("mint"), f);
    realm.bind_import("hyperliquidClientOrderIdRule", o);
    realm.bind_import(
        "mintHyperliquidCloid",
        realm.native("mintHyperliquidCloid", 0, |r, _, _| mint(r)),
    );
    realm.bind_import(
        "readHyperliquidCloid",
        realm.native("readHyperliquidCloid", 1, |r, _, a| {
            let v = r.to_serde(&arg(a, 0))?;
            Ok(
                cloid::read_hyperliquid_cloid(Some(&v))
                    .map_or(Value::Undefined, |s| Value::str(&s)),
            )
        }),
    );
    realm.bind_import(
        "placementRecordFromWire",
        realm.native("placementRecordFromWire", 3, |r, _, a| {
            let wire = r.to_serde(&arg(a, 0))?;
            let symbol = r.to_rust_string(&arg(a, 1))?;
            let placed_at = r.to_number(&arg(a, 2))?;
            let record = cloid::placement_record_from_wire(&wire, symbol, placed_at)
                .map_err(|e| r.type_error(&e))?;
            r.from_typed(&record)
        }),
    );
    let pattern = realm.new_object();
    let test = realm.native("test", 1, |r, _, a| {
        let s = r.to_rust_string(&arg(a, 0))?;
        Ok(Value::Bool(cloid::is_hyperliquid_cloid(&s)))
    });
    realm.define_hidden(&pattern, PropKey::str("test"), test);
    realm.bind_import("HYPERLIQUID_CLOID_PATTERN", pattern);
}

fn mint(r: &mut Realm) -> Result<Value, Thrown> {
    cloid::mint_hyperliquid_cloid()
        .map(|s| Value::str(&s))
        .map_err(|e| {
            r.error(&format!(
                "platform CSPRNG unavailable for cloid minting: {e}"
            ))
        })
}
