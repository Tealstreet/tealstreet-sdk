//! Hyperliquid module helpers the class bodies import, bound to their ports.
use safe_cex_exchange::js_runtime::{value::{Js, Thrown}};
use super::{
    Host,
    host_core::js,
};
use crate::{
    constants, exchange_helpers, income, json_boundaries, market_helpers, order_inference,
    rate_limit,
};
use std::borrow::Cow;

fn arg(args: &[Js], i: usize) -> Js {
    args.get(i).cloned().unwrap_or(Js::Undefined)
}
fn num(r: &mut Host, v: &Js) -> Result<f64, Thrown> {
    r.to_number(v)
}
fn opt_js(v: Option<crate::spot_json::JsString>) -> Js {
    v.map_or(Js::Undefined, |s| Js::utf16(&s.0))
}

struct WalletKey(String);
struct SignedView(safe_cex_num::JsJsonValue);
impl crate::signing::SigningInput for SignedView {
    fn encoding_view(&self) -> Cow<'_, safe_cex_num::JsJsonValue> {
        Cow::Borrowed(&self.0)
    }
}

pub(super) fn signing_error(
    r: &mut Host,
    cause: safe_cex_exchange::js_runtime::value::owned::OwnedCallbackError,
) -> Thrown {
    let properties = cause.value().to_js();
    let restore = |r: &mut Host| -> Result<Thrown, Thrown> {
        let name = properties.get("name")?.to_js_string()?;
        let message = properties.get("message")?.to_js_string()?;
        let error = safe_cex_exchange::js_runtime::value::error_object(&String::from_utf16_lossy(&name), "");
        r.define_hidden(&error, Js::str("name"), Js::String(name));
        r.define_hidden(&error, Js::str("message"), Js::String(message));
        for (key, value) in properties.own_entries()? {
            if key.as_ref() != "name".encode_utf16().collect::<Vec<_>>() && key.as_ref() != "message".encode_utf16().collect::<Vec<_>>() {
                error.define_key(&key, value)?;
            }
        }
        Ok(Thrown::new(error))
    };
    restore(r).unwrap_or_else(std::convert::identity)
}

fn resolve_builder(r: &mut Host, options: Js) -> Result<Js, Thrown> {
    let builder = if options.is_nullish() {
        Js::Undefined
    } else {
        r.get_str(&options, "hyperliquidBuilder")?
    };
    if !builder.is_nullish() {
        return Ok(builder);
    }
    let testnet = if options.is_nullish() {
        Js::Undefined
    } else {
        r.get_str(&options, "testnet")?
    };
    Ok(r.object_from(vec![
        (
            "address",
            Js::str(constants::get_builder_address(testnet.truthy())),
        ),
        (
            "feeTenthsBps",
            Js::Number(constants::TEALSTREET_BUILDER_FEE),
        ),
    ]))
}

// Source signing helpers operate on native values before the HTTP JSON boundary.
fn order_wire_float(r: &mut Host, value: Js) -> Result<Js, Thrown> {
    let rounded = r.call_method(&value, "toFixed", &[Js::Number(8.)])?;
    let parse = r.global_binding("parseFloat")?;
    let parsed = r.call(&parse, &Js::Undefined, std::slice::from_ref(&rounded))?;
    let difference = r.binary("-", &parsed, &value)?;
    if r.to_number(&difference)?.abs() >= 1e-9 {
        let value = r.to_rust_string(&value)?;
        return Err(r.error_of("Error", &format!("floatToWire causes rounding: {value}")));
    }
    let suffix = r.regexp(r"/\.?0+$/")?;
    let normalized = r.call_method(&rounded, "replace", &[suffix, Js::str("")])?;
    Ok(if normalized.strict_equals(&Js::str("-0")) {
        Js::str("0")
    } else {
        normalized
    })
}
fn order_wire_type(r: &mut Host, order_type: Js) -> Result<Js, Thrown> {
    if r.get_str(&order_type, "limit")?.truthy() {
        let limit = r.get_str(&order_type, "limit")?;
        return Ok(r.object_from(vec![("limit", limit)]));
    }
    if r.get_str(&order_type, "trigger")?.truthy() {
        let trigger = r.get_str(&order_type, "trigger")?;
        let is_market = r.get_str(&trigger, "isMarket")?;
        let trigger = r.get_str(&order_type, "trigger")?;
        let price = r.get_str(&trigger, "triggerPx")?;
        let price = order_wire_float(r, price)?;
        let trigger = r.get_str(&order_type, "trigger")?;
        let tpsl = r.get_str(&trigger, "tpsl")?;
        let trigger = r.object_from(vec![
            ("isMarket", is_market),
            ("triggerPx", price),
            ("tpsl", tpsl),
        ]);
        return Ok(r.object_from(vec![("trigger", trigger)]));
    }
    Err(r.error_of("Error", "Invalid order type"))
}
fn order_request_to_wire(r: &mut Host, order: Js, asset: Js) -> Result<Js, Thrown> {
    let buy = r.get_str(&order, "is_buy")?;
    let price = r.get_str(&order, "limit_px")?;
    let price = order_wire_float(r, price)?;
    let size = r.get_str(&order, "sz")?;
    let size = order_wire_float(r, size)?;
    let reduce_only = r.get_str(&order, "reduce_only")?;
    let order_type = r.get_str(&order, "order_type")?;
    let order_type = order_wire_type(r, order_type)?;
    let wire = r.object_from(vec![
        ("a", asset),
        ("b", buy),
        ("p", price),
        ("s", size),
        ("r", reduce_only),
        ("t", order_type),
    ]);
    if !matches!(r.get_str(&order, "cloid")?, Js::Undefined) {
        let cloid = r.get_str(&order, "cloid")?;
        r.set_str(&wire, "c", cloid)?;
    }
    Ok(wire)
}

/// `class HyperliquidUnreadableOrderResponseError extends Error`.
fn unreadable_error_class(realm: &Host) -> Js {
    realm.singleton("HyperliquidUnreadableOrderResponseError", |realm| {
        let mut r = realm.clone();
        let base = r.global_binding("Error").expect("Error intrinsic");
        let (class, _) = r
            .derived_class(
                "HyperliquidUnreadableOrderResponseError",
                1,
                &base,
                std::rc::Rc::new(|r, class, args, new_target| {
                    let e = r.super_construct(class, args, new_target)?;
                    r.set_str(
                        &e,
                        "name",
                        Js::str("HyperliquidUnreadableOrderResponseError"),
                    )?;
                    Ok(e)
                }),
            )
            .expect("derived error class");
        class
    })
}

pub(crate) fn bind(realm: &Host) {
    realm.bind_import(
        "HyperliquidUnreadableOrderResponseError",
        unreadable_error_class(realm),
    );
    let b = |name: &'static str,
             length: u8,
             body: fn(&mut Host, &[Js]) -> Result<Js, Thrown>| {
        realm.bind_import(name, realm.native(name, length, move |r, _, a| body(r, a)));
    };
    b("isOnlyHyperliquidTerminalCancelError", 1, |_, a| {
        let Js::String(message) = arg(a, 0) else {
            return Ok(Js::Bool(false));
        };
        Ok(Js::Bool(
            crate::order_response::is_only_hyperliquid_terminal_cancel_error(
                &String::from_utf16_lossy(&message),
            ),
        ))
    });
    b("deriveHyperliquidClosedPositions", 3, |r, a| {
        income::derive_hyperliquid_closed_positions(r, &arg(a, 0), &arg(a, 1), &arg(a, 2))
    });
    b("collapseHyperliquidOrderHistory", 1, |r, a| {
        let rows = r.to_serde(&arg(a, 0))?;
        let rows = rows
            .as_array()
            .ok_or_else(|| r.type_error("historical order rows must be an array"))?;
        let collapsed = crate::order_history::collapse_hyperliquid_order_history(rows)
            .map_err(|error| r.error_of(&error.name, &error.message))?;
        Ok(r.from_serde(&serde_json::Value::Array(collapsed)))
    });
    b("mapHyperliquidHistoricalOrder", 2, |r, a| {
        let row = r.to_serde(&arg(a, 0))?;
        let resolve = arg(a, 1);
        let mut original_throw = None;
        let mapped = crate::order_history::map_hyperliquid_historical_order(&row, |coin| {
            let value = r
                .call(&resolve, &Js::Undefined, &[Js::str(coin)])
                .and_then(|value| {
                    if value.truthy() {
                        r.to_rust_string(&value).map(Some)
                    } else {
                        Ok(None)
                    }
                });
            value.map_err(|error| {
                original_throw = Some(error);
                // Only unwind the native DTO mapper. Never inspect the original
                // thrown value: its identity and getters belong to the Host.
                safe_cex_exchange::runtime::ExchangeError::new("historical order resolver threw")
            })
        });
        if let Some(error) = original_throw {
            return Err(error);
        }
        let mapped = mapped.map_err(|error| r.error_of(&error.name, &error.message))?;
        r.from_typed(&mapped)
    });
    b("mapHyperliquidFillToIncomeRecords", 2, |r, a| {
        income::map_hyperliquid_fill_to_income_records(r, &arg(a, 0), &arg(a, 1))
    });
    b("mapHyperliquidFundingToIncomeRecord", 2, |r, a| {
        income::map_hyperliquid_funding_to_income_record(r, &arg(a, 0), &arg(a, 1))
    });
    b("paginateHyperliquidByTime", 6, |r, a| {
        let start = r.to_number(&arg(a, 2))?;
        let max_pages = match arg(a, 6) {
            Js::Undefined => None,
            value => Some(r.to_number(&value)?),
        };
        let observer = income::paginate_hyperliquid_by_time(
            r.clone(),
            arg(a, 0),
            arg(a, 1),
            start,
            arg(a, 3),
            arg(a, 4),
            arg(a, 5),
            max_pages,
        );
        Ok(r.spawn_value(Box::pin(observer)))
    });
    b("calculatePricePrecision", 3, |r, a| {
        let (p, amount, max) = (
            num(r, &arg(a, 0))?,
            num(r, &arg(a, 1))?,
            num(r, &arg(a, 2))?,
        );
        market_helpers::calculate_price_precision(r, p, amount, max).map(Js::Number)
    });
    b("symbolToCoin", 1, |r, a| {
        let s = js(r.to_string(&arg(a, 0))?);
        market_helpers::symbol_to_coin(r, &s).map(|s| Js::utf16(&s.0))
    });
    b("parseHyperliquidOpenInterestNotional", 2, |r, a| {
        let price = num(r, &arg(a, 1))?;
        market_helpers::parse_hyperliquid_open_interest_notional(r, &arg(a, 0), price)
            .map(Js::Number)
    });
    b("parseHyperliquidVolumeFields", 2, |r, a| {
        let price = num(r, &arg(a, 1))?;
        let f = market_helpers::parse_hyperliquid_volume_fields(r, &arg(a, 0), price)?;
        Ok(r.object_from(vec![
            ("volume", Js::Number(f.volume)),
            ("quoteVolume", Js::Number(f.quote_volume)),
        ]))
    });
    b("decodeHyperliquidFastAssetCtxs", 1, |r, a| {
        market_helpers::decode_hyperliquid_fast_asset_ctxs(r, &arg(a, 0))
    });
    b("calculatePriceChangePercentage", 2, |r, a| {
        let (p, prev) = (num(r, &arg(a, 0))?, num(r, &arg(a, 1))?);
        Ok(Js::Number(
            exchange_helpers::calculate_price_change_percentage(p, prev),
        ))
    });
    b("isFinitePositiveNumber", 1, |r, a| {
        // This predicate never coerces a non-number or reads object properties.
        let Js::Number(number) = arg(a, 0) else {
            return Ok(Js::Bool(false));
        };
        let Some(number) = serde_json::Number::from_f64(number) else {
            return Ok(Js::Bool(false));
        };
        exchange_helpers::is_finite_positive_number(&serde_json::Value::Number(number))
            .map(Js::Bool)
            .map_err(|error| r.error_of(error.name, &error.message))
    });
    b(
        "hyperliquidOrderResponseHasDeferredOrderStatus",
        1,
        |r, a| {
            let response = r.to_serde(&arg(a, 0))?;
            Ok(Js::Bool(
                crate::order_response::hyperliquid_order_response_has_deferred_order_status(
                    &response,
                ),
            ))
        },
    );
    b("parseFiniteNumber", 1, |r, a| {
        let v = r.to_serde(&arg(a, 0))?;
        exchange_helpers::parse_finite_number(&v)
            .map(Js::Number)
            .map_err(|e| r.error_of(e.name, e.message))
    });
    b("parseHyperliquidOrderTimeInForce", 1, |r, a| {
        let v = r.to_serde(&arg(a, 0))?;
        match order_inference::parse_hyperliquid_order_time_in_force(Some(&v)) {
            Some(tif) => r.from_typed(&tif),
            None => Ok(Js::Undefined),
        }
    });
    b("inferHyperliquidOrderTypeAndPrice", 1, |r, a| {
        let v = r.to_serde(&arg(a, 0))?;
        let out = order_inference::infer_hyperliquid_order_type_and_price(&v)
            .map_err(|e| r.error_of(&e.name.clone(), &e.message))?;
        let kind = r.from_typed(&out.r#type)?;
        use order_inference::HyperliquidOrderInferenceSource as S;
        let source = match out.source {
            S::OrderTypeLimit => "orderType=Limit",
            S::OrderTypeTp => "orderType=TP",
            S::OrderTypeSl => "orderType=SL",
            S::TriggerPxTpslTp => "triggerPx+tpsl=tp",
            S::TriggerPxTpslSl => "triggerPx+tpsl=sl",
            S::TriggerPxOnly => "triggerPx-only",
            S::NoOrderTypeFallback => "no-orderType-fallback",
            S::FallbackMarket => "fallback-market",
        };
        Ok(r.object_from(vec![
            ("type", kind),
            ("price", Js::Number(out.price)),
            ("source", Js::str(source)),
        ]))
    });
    b("getBuilderAddress", 1, |_, a| {
        Ok(Js::str(constants::get_builder_address(
            arg(a, 0).truthy(),
        )))
    });
    b("builderFeeRateFromTenthsBps", 1, |r, a| {
        let n = num(r, &arg(a, 0))?;
        Ok(Js::str(&constants::builder_fee_rate_from_tenths_bps(n)))
    });
    b("resolveHyperliquidBuilder", 1, |r, a| {
        resolve_builder(r, arg(a, 0))
    });
    b("maskHyperliquidAddress", 1, |r, a| {
        Ok(opt_js(json_boundaries::mask_hyperliquid_address(
            r,
            &arg(a, 0),
        )))
    });
    b("formatHyperliquidWsDiagnostic", 2, |r, a| {
        let scope = js(r.to_string(&arg(a, 0))?);
        json_boundaries::format_hyperliquid_ws_diagnostic(r, &scope, &arg(a, 1))
            .map(|s| Js::utf16(&s.0))
    });
    b("shouldEmitHyperliquidWsReceiveDiagnostic", 2, |r, a| {
        let scope = js(r.to_string(&arg(a, 0))?);
        json_boundaries::should_emit_hyperliquid_ws_receive_diagnostic(r, &scope, &arg(a, 1))
            .map(Js::Bool)
    });
    b("shouldEmitHyperliquidWsSendDiagnostic", 1, |r, a| {
        json_boundaries::should_emit_hyperliquid_ws_send_diagnostic(r, &arg(a, 0)).map(Js::Bool)
    });
    b("parseHyperliquidExchangePayload", 1, |r, a| {
        Ok(
            json_boundaries::parse_hyperliquid_exchange_payload(r, &arg(a, 0))?
                .unwrap_or(Js::Undefined),
        )
    });
    b("getHyperliquidRestRequestWeight", 2, |r, a| {
        let url = r.to_rust_string(&arg(a, 0))?;
        let data = r.to_serde(&arg(a, 1))?;
        rate_limit::get_hyperliquid_rest_request_weight(&url, &data)
            .map(Js::Number)
            .map_err(|e| r.error_of(e.name, e.message))
    });
    b("orderCarriesClientOrderId", 2, |r, a| {
        let order: safe_cex_types::Order = r.to_typed(&arg(a, 0))?;
        let id = r.to_rust_string(&arg(a, 1))?;
        safe_cex_exchange::client_order_id::order_carries_client_order_id(&order, &id)
            .map(Js::Bool)
            .map_err(|error| r.error_of(error.name, error.message))
    });
    // Object.assign(orderIds, { clientOrderId }): the same array, now carrying the ID.
    b("withReplacementClientOrderId", 2, |r, a| {
        let ids = arg(a, 0);
        r.set_str(&ids, "clientOrderId", arg(a, 1))?;
        Ok(ids)
    });
    // checkBuilderFeeApproved(user, builder?, testnet?, requiredFeeTenthsBps?) → Promise<boolean>.
    realm.bind_import(
        "checkBuilderFeeApproved",
        realm.native("checkBuilderFeeApproved", 4, |r, _, a| {
            let realm = r.clone();
            let args = [arg(a, 0), arg(a, 1), arg(a, 2), arg(a, 3)];
            Ok(r.spawn_value(Box::pin(async move {
                let mut realm = realm;
                crate::browser_approval_flows::check_builder_fee_approved(
                    &mut realm, &args[0], &args[1], &args[2], &args[3],
                )
                .await
            })))
        }),
    );
    // ethers.Wallet stand-in: the address is public; the key stays native.
    b("createWalletFromPrivateKey", 1, |r, a| {
        let key = r.to_rust_string(&arg(a, 0))?;
        let signer = safe_cex_crypto::wallet_utils::create_wallet_from_private_key(&key)
            .map_err(|e| r.error(&e))?;
        let wallet = r.host_object(std::rc::Rc::new(WalletKey(key)));
        r.define_str(
            &wallet,
            "address",
            Js::str(&signer.address().to_string()),
        )?;
        Ok(wallet)
    });
    b("orderRequestToOrderWire", 2, |r, a| {
        order_request_to_wire(r, arg(a, 0), arg(a, 1))
    });
    for (name, grouping) in [
        ("orderWiresToOrderAction", "na"),
        ("orderWiresToNormalTPSLAction", "normalTpsl"),
        ("orderWiresToPositionalTPSLAction", "positionTpsl"),
    ] {
        // The source object literal: wires aliased, builder only when truthy.
        realm.bind_import(
            name,
            realm.native(name, 2, move |r, _, a| {
                let action = r.object_from(vec![
                    ("type", Js::str("order")),
                    ("orders", arg(a, 0)),
                    ("grouping", Js::str(grouping)),
                ]);
                if arg(a, 1).truthy() {
                    r.set_str(&action, "builder", arg(a, 1))?;
                }
                Ok(action)
            }),
        );
    }
    b("signL1Action", 5, |r, a| {
        let wallet = arg(a, 0).resource::<WalletKey>().ok_or_else(|| {
            r.type_error("signL1Action expects a wallet from createWalletFromPrivateKey")
        })?;
        let action = SignedView(r.to_js_json(&arg(a, 1))?);
        let vault = match arg(a, 2) {
            v if v.is_nullish() => None,
            v => Some(r.to_rust_string(&v)?),
        };
        let nonce = r.to_serde(&arg(a, 3))?;
        let mainnet = match arg(a, 4) {
            Js::Undefined => true,
            v => v.truthy(),
        };
        let result =
            crate::signing::sign_l1_action(&wallet.0, &action, vault.as_deref(), &nonce, mainnet)
                .map_err(|cause| signing_error(r, cause))
                .and_then(|sig| r.from_typed(&sig));
        Ok(r.settled_promise(result))
    });
    b("parseHyperliquidOrderResponse", 1, |r, a| {
        use crate::order_response::HyperliquidOrderResponseError as E;
        let data = r.to_serde(&arg(a, 0))?;
        match crate::order_response::parse_hyperliquid_order_response(&data) {
            Ok(parsed) => {
                let ids = r.from_typed(&parsed.order_ids)?;
                Ok(r.object_from(vec![
                    ("orderIds", ids),
                    ("waitingForTrigger", Js::Bool(parsed.waiting_for_trigger)),
                ]))
            }
            Err(E::Unreadable(e)) => {
                let class = unreadable_error_class(r);
                Err(Thrown::new(r.construct(&class, &[Js::str(&e.message)], None)?))
            }
            Err(E::Venue(message)) => Err(r.error(&message)),
            Err(E::Coercion(e)) => Err(r.error_of(e.name, e.message)),
        }
    });
    b("validateWalletAndPrivateKey", 2, |r, a| {
        let key = r.to_rust_string(&arg(a, 0))?;
        let wallet = r.to_rust_string(&arg(a, 1))?;
        safe_cex_crypto::wallet_utils::validate_wallet_and_private_key(&key, &wallet)
            .map(Js::Bool)
            .map_err(|e| r.error(&e))
    });
}

#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
#[path = "../../tests/support/byteslike_signing.rs"]
mod byteslike_signing;
