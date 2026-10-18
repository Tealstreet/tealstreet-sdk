//! Lighter module bindings the ported bodies import by name: the crate's pure
//! helpers wrapped as JS callables, plus the lighter constant tables.
use crate::{
    account_tier::{
        LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_MISSING_MESSAGE,
        LIGHTER_PARTNER_ATTRIBUTION_TIER_REQUIREMENT_MESSAGE, LighterAccountTier,
        extract_lighter_account_tier_from_payload, has_non_zero_lighter_integrator_fee,
        is_lighter_account_tier_eligible_for_partner_attribution,
        is_lighter_partner_attribution_approval_missing_rejection,
        is_lighter_partner_attribution_tier_rejection,
    },
    builder_fee::resolve_lighter_builder_fee_config,
    deployment::{resolve_lighter_deployment_profile, resolve_lighter_interval},
    types::{
        ENDPOINTS, GRAFANA_ENDPOINTS_EVERY, GRAFANA_ENDPOINTS_PERIODIC, ORDER_SIDE, ORDER_STATUS,
        ORDER_TIME_IN_FORCE, ORDER_TYPE,
    },
    utils::{
        get_lighter_amount_decimals, get_lighter_order_minimums, get_lighter_price_decimals,
        is_lighter_l1_address, parse_lighter_api_key_index, parse_lighter_positive_integer_string,
        resolve_lighter_cancel_targets, to_lighter_api_key_index_string, to_lighter_scaled_int,
    },
};
use safe_cex_exchange::js_runtime::value::{Js, Thrown, arg, from_typed, to_json, to_typed};
use safe_cex_num::{JsBigIntError, JsCoercionError};
use safe_cex_types::{ExchangeName, Timeframe};
use std::rc::Rc;

fn coercion(e: JsCoercionError) -> Thrown {
    Thrown::error(e.name, &e.message)
}

fn bigint_error(e: JsBigIntError) -> Thrown {
    Thrown::error(e.kind.name(), &e.message)
}

/// `undefined` stays absent; everything else crosses as its JSON view.
fn optional_json(value: &Js) -> Result<Option<serde_json::Value>, Thrown> {
    if value.is_undefined() {
        Ok(None)
    } else {
        to_json(value).map(Some)
    }
}

fn optional_number(value: &Js) -> Result<Option<f64>, Thrown> {
    match value {
        Js::Undefined | Js::Null => Ok(None),
        other => other.to_number().map(Some),
    }
}

fn optional_bool(value: &Js) -> Option<bool> {
    (!value.is_nullish()).then(|| value.truthy())
}

fn exchange_name(value: &Js) -> Result<Option<ExchangeName>, Thrown> {
    if value.is_nullish() {
        Ok(None)
    } else {
        to_typed(value).map(Some)
    }
}

fn tier(value: &Js) -> Result<Option<LighterAccountTier>, Thrown> {
    Ok(match value.as_str().map(|s| String::from_utf16_lossy(s)) {
        Some(s) if s == "standard" => Some(LighterAccountTier::Standard),
        Some(s) if s == "premium" => Some(LighterAccountTier::Premium),
        Some(s) if s == "plus" => Some(LighterAccountTier::Plus),
        _ => None,
    })
}

fn partner_rejection(
    input: Js,
    classify: fn(
        Option<&serde_json::Value>,
        Option<&serde_json::Value>,
    ) -> Result<bool, JsCoercionError>,
) -> Result<Js, Thrown> {
    let string = |value: Js| -> Result<serde_json::Value, Thrown> {
        Ok(serde_json::Value::String(if value.is_nullish() {
            String::new()
        } else {
            String::from_utf16_lossy(&value.to_js_string()?)
        }))
    };
    let code = string(input.get("code")?)?;
    if classify(Some(&code), None).map_err(coercion)? {
        return Ok(Js::Bool(true));
    }
    let message = string(input.get("message")?)?;
    classify(Some(&code), Some(&message))
        .map(Js::Bool)
        .map_err(coercion)
}

fn helper(name: &str, f: impl Fn(Vec<Js>) -> Result<Js, Thrown> + 'static) -> (String, Js) {
    (name.to_owned(), Js::function(name, move |_, args| f(args)))
}

/// A `Record<string, Enum>` table, values in their serialized (TS enum) form.
fn enum_table<T: serde::Serialize>(entries: &[(&str, T)]) -> Result<Js, Thrown> {
    let table = Js::object(vec![]);
    for (key, value) in entries {
        table.define(key, from_typed(value)?)?;
    }
    Ok(table)
}

/// lighter.exchange.ts module constants the ported bodies resolve by name.
fn exchange_module_constants(globals: &Js) -> Result<(), Thrown> {
    for (name, value) in [
        ("LIGHTER_OHLCV_POLL_INTERVAL_MS", 5_000.0),
        ("LIGHTER_OHLCV_WS_STALE_MS", 120_000.0),
        ("LIGHTER_OHLCV_FALLBACK_403_COOLDOWN_MS", 60_000.0),
        ("LIGHTER_ACCOUNT_LIMITS_CACHE_MS", 5.0 * 60_000.0),
        ("LIGHTER_ACCOUNT_LIMITS_RETRY_MS", 60_000.0),
        ("LIGHTER_ACCOUNT_SNAPSHOT_REUSE_MS", 1_000.0),
        ("LIGHTER_ORDER_RECONCILIATION_COOLDOWN_MS", 3_000.0),
        ("LIGHTER_INITIAL_TICK_MAX_DELAY_MS", 2_000.0),
        ("LIGHTER_TRANSIENT_SNAPSHOT_WARNING_COOLDOWN_MS", 60_000.0),
        ("LIGHTER_MARGIN_FRACTION_TICK", 10_000.0),
        ("LIGHTER_DEFAULT_STOP_SLIPPAGE_PCT", 1.0),
        ("LIGHTER_MAX_STOP_SLIPPAGE_PCT", 50.0),
        ("INVALIDATE_WEBSOCKET_POSITIONS_UPDATE_DELAY", 1_000.0),
    ] {
        globals.define(name, Js::Number(value))?;
    }
    globals.define("LIGHTER_DEFAULT_QUOTE", Js::str("USDC"))?;
    globals.define(
        "LIGHTER_KNOWN_QUOTES",
        string_table(&["USDT", "USDC", "USDG"]),
    )?;
    globals.define(
        "LIGHTER_PARTNER_ATTRIBUTION_TIER_REQUIREMENT_MESSAGE",
        Js::str(LIGHTER_PARTNER_ATTRIBUTION_TIER_REQUIREMENT_MESSAGE),
    )?;
    globals.define(
        "LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_MISSING_MESSAGE",
        Js::str(LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_MISSING_MESSAGE),
    )?;
    globals.define("ORDER_SIDE", enum_table(&ORDER_SIDE)?)?;
    globals.define("ORDER_STATUS", enum_table(&ORDER_STATUS)?)?;
    globals.define("ORDER_TYPE", enum_table(&ORDER_TYPE)?)?;
    globals.define("ORDER_TIME_IN_FORCE", enum_table(&ORDER_TIME_IN_FORCE)?)?;
    Ok(())
}

fn string_table(entries: &[&str]) -> Js {
    Js::array(entries.iter().map(|s| Js::str(s)).collect())
}

pub(crate) fn install(globals: &Js) -> Result<(), Thrown> {
    let helpers = vec![
        helper("toLighterScaledInt", |a| {
            let value = arg(&a, 0).to_number()?;
            let decimals = arg(&a, 1).to_number()?;
            to_lighter_scaled_int(value, decimals)
                .map(|b| Js::BigInt(Rc::new(b)))
                .map_err(bigint_error)
        }),
        helper("parseLighterApiKeyIndex", |a| {
            let value = optional_json(&arg(&a, 0))?;
            Ok(parse_lighter_api_key_index(value.as_ref())
                .map_err(coercion)?
                .map_or(Js::Undefined, Js::Number))
        }),
        helper("toLighterApiKeyIndexString", |a| {
            let value = optional_json(&arg(&a, 0))?;
            let fallback = arg(&a, 1);
            let fallback = if fallback.is_undefined() {
                None
            } else {
                Some(String::from_utf16_lossy(&fallback.to_js_string()?))
            };
            to_lighter_api_key_index_string(value.as_ref(), fallback.as_deref())
                .map(|s| Js::str(&s))
                .map_err(coercion)
        }),
        helper("parseLighterPositiveIntegerString", |a| {
            let value = optional_json(&arg(&a, 0))?;
            Ok(parse_lighter_positive_integer_string(value.as_ref())
                .map_err(coercion)?
                .map_or(Js::Undefined, |s| Js::str(&s)))
        }),
        helper("isLighterL1Address", |a| {
            let value = optional_json(&arg(&a, 0))?;
            is_lighter_l1_address(value.as_ref())
                .map(Js::Bool)
                .map_err(coercion)
        }),
        helper("getLighterAmountDecimals", |a| {
            get_lighter_amount_decimals(&arg(&a, 0)).map(Js::Number)
        }),
        helper("getLighterPriceDecimals", |a| {
            get_lighter_price_decimals(&arg(&a, 0)).map(Js::Number)
        }),
        helper("getLighterOrderMinimums", |a| {
            from_typed(&get_lighter_order_minimums(&arg(&a, 0))?)
        }),
        helper("resolveLighterCancelTargets", |a| {
            resolve_lighter_cancel_targets(&arg(&a, 0), &arg(&a, 1))
        }),
        helper("resolveLighterInterval", |a| {
            let interval: Timeframe = to_typed(&arg(&a, 0))?;
            resolve_lighter_interval(&interval)
                .map(Js::str)
                .map_err(|message| Thrown::error("Error", &message))
        }),
        helper("resolveLighterDeploymentProfile", |a| {
            let input = arg(&a, 0);
            let name = exchange_name(&input.get("exchangeName")?)?;
            let testnet = optional_bool(&input.get("testnet")?);
            from_typed(&resolve_lighter_deployment_profile(name.as_ref(), testnet))
        }),
        helper("resolveLighterBuilderFeeConfig", |a| {
            let input = arg(&a, 0);
            // The source receives ExchangeOptions and the venue name separately.
            // Its internal pure helper takes the projected extra fields.
            let extra = input.get("extra")?;
            let mode = if extra.is_nullish() {
                Js::Undefined
            } else {
                extra.get("lighterBuilderFeeMode")?
            };
            let mode = match mode.as_str() {
                Some(value)
                    if value.as_ref()
                        == safe_cex_exchange::js_runtime::value::utf16("static").as_ref() =>
                {
                    Some(serde_json::Value::String("static".into()))
                }
                _ => None,
            };
            let extra = input.get("extra")?;
            let index = if extra.is_nullish() {
                Js::Undefined
            } else {
                extra.get("lighterIntegratorAccountIndex")?
            };
            let index = match index {
                Js::Undefined | Js::Null => None,
                Js::String(value) => {
                    Some(serde_json::Value::String(String::from_utf16_lossy(&value)))
                }
                // A non-string override is present but invalid; it must not
                // fall back to the default integrator index.
                _ => Some(serde_json::Value::Bool(false)),
            };
            let name = exchange_name(&arg(&a, 1))?;
            let testnet = if name == Some(ExchangeName::LighterRobinhood) {
                None
            } else {
                optional_bool(&input.get("testnet")?)
            };
            Ok(
                match resolve_lighter_builder_fee_config(
                    testnet,
                    mode.as_ref(),
                    index.as_ref(),
                    name.as_ref(),
                ) {
                    None => Js::Undefined,
                    Some(config) => Js::object(vec![
                        (
                            "integratorAccountIndex",
                            Js::BigInt(Rc::new(config.integrator_account_index)),
                        ),
                        (
                            "integratorMakerFee",
                            Js::Number(config.integrator_maker_fee),
                        ),
                        (
                            "integratorTakerFee",
                            Js::Number(config.integrator_taker_fee),
                        ),
                        ("mode", from_typed(&config.mode)?),
                    ]),
                },
            )
        }),
        helper("hasNonZeroLighterIntegratorFee", |a| {
            let input = arg(&a, 0);
            let maker = optional_number(&input.get("integratorMakerFee")?)?;
            let taker = optional_number(&input.get("integratorTakerFee")?)?;
            Ok(Js::Bool(has_non_zero_lighter_integrator_fee(maker, taker)))
        }),
        helper("extractLighterAccountTierFromPayload", |a| {
            let payload = arg(&a, 0);
            Ok(
                match extract_lighter_account_tier_from_payload(Some(&payload))? {
                    None => Js::Undefined,
                    Some(tier) => from_typed(&tier)?,
                },
            )
        }),
        helper("isLighterPartnerAttributionTierRejection", |a| {
            partner_rejection(arg(&a, 0), is_lighter_partner_attribution_tier_rejection)
        }),
        helper("isLighterPartnerAttributionApprovalMissingRejection", |a| {
            partner_rejection(
                arg(&a, 0),
                is_lighter_partner_attribution_approval_missing_rejection,
            )
        }),
        helper("isLighterAccountTierEligibleForPartnerAttribution", |a| {
            Ok(Js::Bool(
                is_lighter_account_tier_eligible_for_partner_attribution(tier(&arg(&a, 0))?),
            ))
        }),
    ];
    for (name, function) in helpers {
        globals.define(&name, function)?;
    }
    globals.define(
        "ENDPOINTS",
        Js::object(ENDPOINTS.iter().map(|(k, v)| (*k, Js::str(v))).collect()),
    )?;
    globals.define(
        "GRAFANA_ENDPOINTS_EVERY",
        string_table(&GRAFANA_ENDPOINTS_EVERY),
    )?;
    globals.define(
        "GRAFANA_ENDPOINTS_PERIODIC",
        string_table(&GRAFANA_ENDPOINTS_PERIODIC),
    )?;
    exchange_module_constants(globals)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(options: Js, name: ExchangeName) -> Js {
        let globals = Js::object(vec![]);
        install(&globals).unwrap();
        globals
            .call_method(
                "resolveLighterBuilderFeeConfig",
                vec![options, from_typed(&name).unwrap()],
            )
            .unwrap()
    }

    #[test]
    fn builder_fee_reads_exchange_options_extra_and_the_venue_argument() {
        let opts = Js::object(vec![(
            "extra",
            Js::object(vec![
                ("lighterBuilderFeeMode", Js::str("static")),
                ("lighterIntegratorAccountIndex", Js::str("007")),
            ]),
        )]);
        let value = config(opts, ExchangeName::Lighter);
        assert_eq!(value.get("mode").unwrap().to_display(), "static");
        assert!(
            value
                .get("integratorAccountIndex")
                .unwrap()
                .strict_equals(&Js::BigInt(Rc::new(7.into())))
        );
        assert_eq!(value.get("integratorMakerFee").unwrap().as_f64(), Some(0.0));
        assert_eq!(
            value.get("integratorTakerFee").unwrap().as_f64(),
            Some(250.0)
        );
        assert!(config(Js::object(vec![]), ExchangeName::LighterRobinhood).is_undefined());
        assert!(
            config(
                Js::object(vec![("testnet", Js::Bool(true))]),
                ExchangeName::Lighter
            )
            .is_undefined()
        );
    }

    #[test]
    fn invalid_present_integrator_override_does_not_select_the_default() {
        for index in [
            Js::Number(7.0),
            Js::BigInt(Rc::new(7.into())),
            Js::str("invalid"),
        ] {
            let opts = Js::object(vec![(
                "extra",
                Js::object(vec![("lighterIntegratorAccountIndex", index)]),
            )]);
            assert!(config(opts, ExchangeName::Lighter).is_undefined());
        }
        let opts = Js::object(vec![(
            "extra",
            Js::object(vec![("lighterIntegratorAccountIndex", Js::Null)]),
        )]);
        let value = config(opts, ExchangeName::Lighter);
        assert!(
            value
                .get("integratorAccountIndex")
                .unwrap()
                .strict_equals(&Js::BigInt(Rc::new(158385.into())))
        );
    }
}
