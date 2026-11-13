//! Strict payload checks for the additive DELEGATED write vocabulary.
//! Keep the caller's JSON intact: adding serde defaults would change its hash.
use crate::methods::{Plan, Refused, Reply};
use safe_cex_engine::{Capabilities, Store};
use serde_json::{Value, json};

pub(crate) mod unique_json;

pub(crate) fn text(v: &str) -> bool {
    !v.is_empty() && v.len() <= 256
}
pub(crate) fn order(v: &Value, close: bool) -> bool {
    const FIELDS: &[&str] = &[
        "symbol",
        "type",
        "side",
        "amount",
        "price",
        "triggerPrice",
        "stopLoss",
        "takeProfit",
        "maxSlippagePct",
        "reduceOnly",
        "timeInForce",
        "triggerType",
        "tpTriggerType",
        "slTriggerType",
        "clientOrderId",
        "close",
        "stpMode",
        "bybitBbo",
        "bybitSlippageToleranceTicks",
        "impliedVolatility",
    ];
    let Some(fields) = v.as_object() else {
        return false;
    };
    if fields.keys().any(|k| !FIELDS.contains(&k.as_str()))
        || !v["symbol"].as_str().is_some_and(text)
        || !matches!(
            v["type"].as_str(),
            Some(
                "market"
                    | "limit"
                    | "conditional"
                    | "stop_market"
                    | "take_profit_market"
                    | "trailing_stop_market"
            )
        )
        || !matches!(v["side"].as_str(), Some("buy" | "sell"))
        || !v["amount"]
            .as_f64()
            .is_some_and(|n| n.is_finite() && n >= 0.0)
        || (close
            && (v["reduceOnly"] != true || !matches!(v["type"].as_str(), Some("market" | "limit"))))
    {
        return false;
    }
    for key in [
        "price",
        "triggerPrice",
        "stopLoss",
        "takeProfit",
        "maxSlippagePct",
        "bybitSlippageToleranceTicks",
        "impliedVolatility",
    ] {
        if let Some(v) = fields.get(key)
            && !v.as_f64().is_some_and(|n| n.is_finite() && n >= 0.0)
        {
            return false;
        }
    }
    for key in ["reduceOnly", "close"] {
        if fields.get(key).is_some_and(|v| !v.is_boolean()) {
            return false;
        }
    }
    for (key, values) in [
        (
            "timeInForce",
            &[
                "GoodTillCancel",
                "ImmediateOrCancel",
                "FillOrKill",
                "PostOnly",
            ][..],
        ),
        (
            "triggerType",
            &["mark_price", "last_price", "index_price"][..],
        ),
        (
            "tpTriggerType",
            &["mark_price", "last_price", "index_price"][..],
        ),
        (
            "slTriggerType",
            &["mark_price", "last_price", "index_price"][..],
        ),
        (
            "stpMode",
            &["EXPIRE_TAKER", "EXPIRE_MAKER", "EXPIRE_BOTH"][..],
        ),
    ] {
        if fields
            .get(key)
            .is_some_and(|v| !v.as_str().is_some_and(|s| values.contains(&s)))
        {
            return false;
        }
    }
    if fields
        .get("clientOrderId")
        .is_some_and(|v| !v.as_str().is_some_and(text))
    {
        return false;
    }
    if let Some(bbo) = fields.get("bybitBbo") {
        let Some(bbo) = bbo.as_object() else {
            return false;
        };
        if bbo.len() != 2
            || !bbo
                .get("sideType")
                .is_some_and(|v| matches!(v.as_str(), Some("Queue" | "Counterparty")))
            || !bbo.get("level").is_some_and(|v| {
                v.as_f64()
                    .is_some_and(|n| n.is_finite() && n.fract() == 0.0 && (1.0..=5.0).contains(&n))
            })
        {
            return false;
        }
    }
    serde_json::from_value::<safe_cex_engine::model::PlaceOrderOpts>(v.clone()).is_ok()
}

fn command(op: &str, mut fields: Value) -> Plan {
    fields["op"] = op.into();
    Plan::Command {
        command: fields,
        reply: Reply::Unit,
    }
}
fn refused(message: &str) -> Refused {
    Refused {
        name: "UnsupportedMethod",
        message: message.into(),
    }
}
/// These names are internal to typed DELEGATED actions; legacy COSIGNED
/// translation deliberately never recognizes them.
pub(crate) fn translate(
    method: &str,
    args: &[Value],
    caps: &Capabilities,
) -> Option<Result<Vec<Plan>, Refused>> {
    let value = args.first().cloned().unwrap_or(Value::Null);
    Some(match method {
        "delegatedCancelAccountOrders" if caps.nuke_account_cancel => {
            Ok(vec![command("cancelAccountOrders", json!({}))])
        }
        "delegatedCancelAccountOrders" => Err(refused(
            "Account-wide cancellation is unavailable for this venue",
        )),
        "delegatedSetStpMode"
            if caps.stp_controls
                && matches!(
                    value.as_str(),
                    Some("EXPIRE_TAKER" | "EXPIRE_MAKER" | "EXPIRE_BOTH")
                ) =>
        {
            Ok(vec![command("setStpMode", json!({"mode":value}))])
        }
        "delegatedSetAccountTier"
            if caps.account_tier
                && matches!(value.as_str(), Some("standard" | "premium" | "plus")) =>
        {
            Ok(vec![command("setAccountTier", json!({"tier":value}))])
        }
        "delegatedSetStpMode" | "delegatedSetAccountTier" => {
            Err(refused("Setting is unavailable for this venue"))
        }
        _ => return None,
    })
}

/// The same fan-out as RustEngineExchange.setAllLeverage: hedge sides are
/// independent; flat symbols with published rails are included once.
pub(crate) fn all_leverage(store: &Store, leverage: f64) -> Result<Vec<Plan>, Refused> {
    if !store.loaded().markets || !store.loaded().positions {
        return Err(refused(
            "Positions have not loaded; no leverage changes were sent",
        ));
    }
    let mut plans = Vec::new();
    let mut live = std::collections::HashSet::new();
    for position in store.positions() {
        live.insert(position.symbol.as_str());
        plans.push(command(
            "setLeverage",
            json!({"symbol":position.symbol,"side":position.side,"leverage":leverage}),
        ));
    }
    for symbol in setting_symbols(store)? {
        if !live.contains(symbol.as_str()) {
            plans.push(command(
                "setLeverage",
                json!({"symbol":symbol,"leverage":leverage}),
            ));
        }
    }
    if plans.is_empty() {
        plans.push(Plan::Reply(Value::Null));
    }
    Ok(plans)
}

/// Validate the existing algo payloads before the router claims/forwards them.
/// These are typed op variants, never arbitrary callMethod or raw engine JSON.
pub(crate) fn algo(op: &str, data: &Value) -> Result<(), &'static str> {
    let valid = match op {
        "exec" => data.as_object().is_some_and(|v| {
            v.keys().all(|k| matches!(k.as_str(), "command" | "symbol"))
                && v.get("command")
                    .and_then(Value::as_str)
                    .is_some_and(|v| !v.is_empty() && v.len() <= safe_cex_exec::MAX_COMMAND_BYTES)
                && v.get("symbol").is_none_or(|v| v.as_str().is_some_and(text))
        }),
        "chaser:start" => {
            chaser_options(data)
                && serde_json::from_value::<safe_cex_chaser::ChaserOptions>(data.clone()).is_ok()
        }
        "twap:start" => {
            twap_options(data)
                && serde_json::from_value::<safe_cex_algo_twap::TwapOptions>(data.clone()).is_ok()
        }
        "twap:state" => {
            data.as_array().is_some_and(|rows| {
                rows.len() <= 32
                    && rows.iter().all(|row| {
                        // Preserve supplied generation evidence; never backfill it from the host.
                        only(
                            row,
                            &["id", "ownerId", "ownerGeneration", "options", "state"],
                        ) && row.get("ownerGeneration").is_none_or(|generation| {
                            generation
                                .as_u64()
                                .is_some_and(|value| value <= 9_007_199_254_740_991)
                        }) && twap_options(&row["options"])
                            && twap_state(&row["state"])
                    })
            }) && serde_json::from_value::<Vec<safe_cex_algo_twap::PersistedTwap>>(data.clone())
                .is_ok()
        }
        "chaser:terminate" | "twap:resume" | "twap:pause" | "twap:stop" | "twap:cleanup" => {
            data.as_str().is_some_and(text)
        }
        "chaser:terminateAll" => data.as_object().is_some_and(|v| v.is_empty()),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err("not sent: invalid algo operation")
    }
}
pub(crate) fn algo_account_matches(op: &str, data: &Value, account: &str) -> bool {
    match op {
        "twap:start" => data["accountName"] == account,
        "chaser:start" => data.get("accountName").is_none_or(|v| v == account),
        "twap:state" => data.as_array().is_some_and(|rows| {
            rows.iter().all(|row| {
                row["options"]["accountName"] == account && row["state"]["accountName"] == account
            })
        }),
        _ => true,
    }
}

fn only(v: &Value, fields: &[&str]) -> bool {
    v.as_object()
        .is_some_and(|v| v.keys().all(|k| fields.contains(&k.as_str())))
}
/// The typed v1 boundary is closed even where legacy native serde is lenient.
/// Validate compatibility fields (including delay) before native decoding can
/// discard them; keep existing native chaser cadence and legacy parsing intact.
fn chaser_options(v: &Value) -> bool {
    if !only(
        v,
        &[
            "symbol",
            "size",
            "side",
            "maxAttempts",
            "reduceOnly",
            "delay",
            "origin",
            "accountName",
            "terminate",
            "clientId",
        ],
    ) || !v["symbol"].as_str().is_some_and(text)
        || !nonnegative(&v["size"])
        || !nonnegative(&v["maxAttempts"])
        || !matches!(v["side"].as_str(), Some("buy" | "sell"))
    {
        return false;
    }
    if v.get("delay").is_some_and(|v| !nonnegative(v))
        || v.get("reduceOnly").is_some_and(|v| !v.is_boolean())
        || v.get("origin")
            .is_some_and(|v| !matches!(v.as_str(), Some("web" | "cli" | "webhook" | "automation")))
    {
        return false;
    }
    for key in ["accountName", "clientId"] {
        if v.get(key).is_some_and(|v| !v.as_str().is_some_and(text)) {
            return false;
        }
    }
    v.get("terminate").is_none_or(|terminate| {
        only(terminate, &["type", "value", "anchorPrice"])
            && matches!(
                terminate["type"].as_str(),
                Some("price" | "percent" | "usd")
            )
            && nonnegative(&terminate["value"])
            && terminate.get("anchorPrice").is_none_or(nonnegative)
    })
}
fn nonnegative(v: &Value) -> bool {
    v.as_f64().is_some_and(|n| n.is_finite() && n >= 0.0)
}
fn market(v: &Value) -> bool {
    only(v, &["symbol"]) && v["symbol"].as_str().is_some_and(text)
}
fn twap_options(v: &Value) -> bool {
    only(
        v,
        &[
            "accountName",
            "activeMarket",
            "size",
            "duration",
            "ordersCount",
            "side",
            "chaserDelay",
            "reduceOnly",
            "randomnessPercentage",
            "useLimitOrders",
            "useChaserOrders",
            "timeoutPct",
            "timeoutAction",
            "failAction",
            "origin",
        ],
    ) && market(&v["activeMarket"])
}
fn twap_state(v: &Value) -> bool {
    only(
        v,
        &[
            "accountName",
            "activeMarket",
            "size",
            "ordersSizes",
            "side",
            "status",
            "ordersCount",
            "ordersExecuted",
            "nextOrderAt",
            "markForCleanup",
            "origin",
            "createdAt",
            "startedAt",
            "inFlight",
        ],
    ) && market(&v["activeMarket"])
        && v.get("inFlight")
            .is_none_or(|v| only(v, &["size", "clientOrderId"]))
}

/// Read the same published rails as the LOCAL frame consumer, including a
/// delisted symbol retained in settings. No engine API/wasm bytes change.
fn setting_symbols(store: &Store) -> Result<Vec<String>, Refused> {
    use safe_cex_engine::wire::{FrameWriter, section, sections};
    let mut writer = FrameWriter::new(Vec::new(), 0.0);
    store.write(&store.account_snapshot(), &mut writer);
    let frame = writer.finish();
    let empty = sections(&frame)
        .find(|(kind, _)| *kind == section::SETTINGS)
        .is_some_and(|(_, payload)| payload.get(..4) == Some(&[0, 0, 0, 0]));
    if empty {
        return Ok(Vec::new());
    }
    let strings = sections(&frame)
        .find(|(kind, _)| *kind == section::STRINGS)
        .map(|(_, v)| v)
        .ok_or_else(|| refused("Settings string table unavailable"))?;
    let settings = sections(&frame)
        .find(|(kind, _)| *kind == section::SETTINGS)
        .map(|(_, v)| v)
        .ok_or_else(|| refused("Settings unavailable"))?;
    let word = |bytes: &[u8], at: usize| -> Result<usize, Refused> {
        bytes
            .get(at..at + 4)
            .and_then(|v| v.try_into().ok())
            .map(u32::from_le_bytes)
            .map(|v| v as usize)
            .ok_or_else(|| refused("Invalid native settings frame"))
    };
    let count = word(settings, 0)?;
    let string_count = word(strings, 0)?;
    let text_start = 4 + 4 * (string_count + 1);
    let refs_start = 8 + 32 * count;
    let mut symbols = Vec::with_capacity(count);
    for row in 0..count {
        let index = word(settings, refs_start + 4 * row)?;
        if index >= string_count {
            return Err(refused("Invalid native settings symbol"));
        }
        let start = word(strings, 4 + index * 4)?;
        let end = word(strings, 8 + index * 4)?;
        let symbol = strings
            .get(text_start + start..text_start + end)
            .and_then(|v| std::str::from_utf8(v).ok())
            .ok_or_else(|| refused("Invalid native settings symbol"))?;
        symbols.push(symbol.to_owned());
    }
    Ok(symbols)
}

pub(crate) fn protection_only(command: &Value) -> bool {
    command["orders"].as_array().is_some_and(|orders| {
        !orders.is_empty()
            && orders.iter().all(|order| {
                order["reduceOnly"] == true
                    && matches!(
                        order["type"].as_str(),
                        Some("stop_market" | "take_profit_market" | "trailing_stop_market")
                    )
            })
    })
}

/// Match LOCAL's execution-control admission before translating any batch leg.
/// A venue must never silently drop a requested execution constraint.
pub(crate) fn placement_capabilities(
    method: &str,
    args: &[Value],
    caps: &Capabilities,
) -> Result<(), Refused> {
    let first = args.first().unwrap_or(&Value::Null);
    let orders = match method {
        "placeOrders" => first.as_array().map(Vec::as_slice).unwrap_or(&[]),
        "placeOrder" | "placeOrderWithClientOrderId" => std::slice::from_ref(first),
        _ => return Ok(()),
    };
    for order in orders {
        if order.get("stpMode").is_some() && !caps.stp_controls {
            return Err(refused("This venue does not support stpMode"));
        }
        if order.get("impliedVolatility").is_some() && !caps.options {
            return Err(refused("Options are not enabled on this account"));
        }
        let bybit_controls =
            order.get("bybitBbo").is_some() || order.get("bybitSlippageToleranceTicks").is_some();
        if bybit_controls && !caps.bybit_execution_controls {
            return Err(refused(
                "This venue does not support Bybit execution controls",
            ));
        }
        if caps.reject_client_slippage && (bybit_controls || order.get("maxSlippagePct").is_some())
        {
            return Err(refused(
                "This venue does not support client-defined slippage",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn twap_restore_generation_is_optional_wire_evidence_but_never_coerced() {
        let cases: Vec<Value> =
            serde_json::from_str(include_str!("../tests/data/delegated-full-writes-v1.json"))
                .unwrap();
        let data = cases
            .iter()
            .find(|case| case["action"]["op"] == "twap:state")
            .unwrap()["action"]["data"]
            .clone();
        assert!(algo("twap:state", &data).is_ok());
        for generation in [json!(0), json!(7), json!(9_007_199_254_740_991u64)] {
            let mut owned = data.clone();
            owned[0]["ownerGeneration"] = generation;
            assert!(algo("twap:state", &owned).is_ok());
        }
        for generation in [
            Value::Null,
            json!("7"),
            json!(true),
            json!(-1),
            json!(0.5),
            json!(9_007_199_254_740_992u64),
        ] {
            let mut invalid = data.clone();
            invalid[0]["ownerGeneration"] = generation;
            assert!(algo("twap:state", &invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn j6_chaser_closes_nested_and_scalar_fields_without_changing_legacy_decode() {
        let valid = json!({"symbol":"BTCUSDT","size":0.001,"side":"buy","maxAttempts":3,"delay":500,
            "reduceOnly":false,"origin":"cli","accountName":"main","clientId":"caller",
            "terminate":{"type":"price","value":60000,"anchorPrice":50000}});
        assert!(algo("chaser:start", &valid).is_ok());
        let mut future = valid.clone();
        future["terminate"]["future"] = json!(true);
        // Legacy serde deliberately remains permissive; only typed WS tightens.
        assert!(serde_json::from_value::<safe_cex_chaser::ChaserOptions>(future.clone()).is_ok());
        assert!(
            algo("chaser:start", &future).is_err(),
            "unknown termination field accepted"
        );
        for (field, value) in [
            ("symbol", json!("")),
            ("size", json!(-1)),
            ("maxAttempts", json!(-1)),
            ("delay", json!("invalid")),
            ("delay", Value::Null),
            ("delay", json!(-1)),
            ("reduceOnly", Value::Null),
            ("origin", json!("future")),
            ("origin", Value::Null),
            ("accountName", json!("")),
            ("accountName", Value::Null),
            ("clientId", json!("")),
            ("clientId", Value::Null),
        ] {
            let mut invalid = valid.clone();
            invalid[field] = value;
            assert!(
                algo("chaser:start", &invalid).is_err(),
                "{field}: {invalid}"
            );
        }
        for (field, value) in [
            ("type", json!("future")),
            ("value", json!(-1)),
            ("anchorPrice", json!(-1)),
            ("anchorPrice", Value::Null),
            ("value", json!("60000")),
        ] {
            let mut invalid = valid.clone();
            invalid["terminate"][field] = value;
            assert!(
                algo("chaser:start", &invalid).is_err(),
                "terminate.{field}: {invalid}"
            );
        }
        let mut invalid = valid.clone();
        invalid["terminate"] = Value::Null;
        assert!(algo("chaser:start", &invalid).is_err());
        // Existing v1 optional shapes and every documented termination variant remain additive.
        for kind in ["price", "percent", "usd"] {
            let mut control = valid.clone();
            control["terminate"]["type"] = json!(kind);
            assert!(algo("chaser:start", &control).is_ok());
        }
        let mut minimal = valid.clone();
        for field in [
            "delay",
            "reduceOnly",
            "origin",
            "accountName",
            "clientId",
            "terminate",
        ] {
            minimal.as_object_mut().unwrap().remove(field);
        }
        assert!(algo("chaser:start", &minimal).is_ok());
        let mut no_anchor = valid;
        no_anchor["terminate"]
            .as_object_mut()
            .unwrap()
            .remove("anchorPrice");
        assert!(algo("chaser:start", &no_anchor).is_ok());
    }

    #[test]
    fn full_write_internal_methods_are_reachable_only_from_typed_actions() {
        let caps = Capabilities {
            nuke_account_cancel: true,
            stp_controls: true,
            account_tier: true,
            ..Default::default()
        };
        for (method, args) in [
            ("delegatedCancelAccountOrders", vec![]),
            ("delegatedSetStpMode", vec![json!("EXPIRE_TAKER")]),
            ("delegatedSetAccountTier", vec![json!("premium")]),
        ] {
            assert!(
                crate::methods::translate_delegated_action(
                    "bybit",
                    method,
                    &args,
                    &caps,
                    &[],
                    false
                )
                .is_ok(),
                "typed action must translate {method}"
            );
            assert!(
                crate::methods::translate_delegated("bybit", method, &args, &caps, &[], false)
                    .is_err(),
                "legacy DELEGATED must refuse {method}"
            );
            assert!(
                crate::methods::translate(method, &args, &caps, &[], false).is_err(),
                "LOCAL/COSIGNED must refuse {method}"
            );
        }
    }
    #[test]
    fn full_write_account_cancel_uses_only_the_native_capability() {
        let base = Capabilities::default();
        assert!(
            translate("delegatedCancelAccountOrders", &[], &base)
                .unwrap()
                .is_err()
        );
        let caps = Capabilities {
            nuke_account_cancel: true,
            ..base
        };
        let plans = translate("delegatedCancelAccountOrders", &[], &caps)
            .unwrap()
            .unwrap();
        assert_eq!(plans.len(), 1);
        match &plans[0] {
            Plan::Command {
                command,
                reply: Reply::Unit,
            } => {
                assert_eq!(command, &json!({"op":"cancelAccountOrders"}));
                let mut wire = command.clone();
                wire["command"] = 1.into();
                assert!(matches!(
                    safe_cex_engine::command::decode_command(&wire.to_string()).unwrap(),
                    safe_cex_engine::Command::Venue {
                        action: safe_cex_engine::VenueAction::CancelAccountOrders,
                        ..
                    }
                ));
            }
            _ => panic!("account cancel must use a native unit-completion command"),
        }
        assert!(
            crate::methods::translate("delegatedCancelAccountOrders", &[], &caps, &[], false)
                .is_err(),
            "legacy LOCAL/COSIGNED must not acquire the typed-only method"
        );
    }
    #[test]
    fn full_write_leverage_fanout_retains_delisted_published_settings() {
        let mut store = Store::default();
        store.update_setting("DELISTED", |setting| setting.leverage = 10.0);
        store.update_setting("UNKNOWN", |_| {});
        assert_eq!(setting_symbols(&store).unwrap(), vec!["DELISTED"]);
    }
}
