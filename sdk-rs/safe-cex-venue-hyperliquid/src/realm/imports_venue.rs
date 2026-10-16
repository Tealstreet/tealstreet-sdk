//! Hyperliquid module helpers the class bodies import, bound to their ports.
use super::{Realm, host_core::js, value::*};
use crate::{
    constants::{self, HyperliquidBuilderConfig, HyperliquidBuilderProvider},
    exchange_helpers, json_boundaries, market_helpers, order_inference, rate_limit,
};
use std::borrow::Cow;

fn arg(args: &[Value], i: usize) -> Value {
    args.get(i).cloned().unwrap_or(Value::Undefined)
}
fn num(r: &mut Realm, v: &Value) -> Result<f64, Thrown> {
    r.to_number(v)
}
fn opt_js(v: Option<crate::spot_json::JsString>) -> Value {
    v.map_or(Value::Undefined, |s| Value::js(&s))
}

/// `Pick<ExchangeOptions, 'hyperliquidBuilder' | 'testnet'>` read from a JS object.
struct BuilderOptions {
    builder: Option<HyperliquidBuilderConfig>,
    testnet: Option<bool>,
}
impl HyperliquidBuilderProvider for BuilderOptions {
    type Error = std::convert::Infallible;
    fn hyperliquid_builder(&self) -> Result<Option<Cow<'_, HyperliquidBuilderConfig>>, Self::Error> {
        Ok(self.builder.as_ref().map(Cow::Borrowed))
    }
    fn testnet(&self) -> Result<Option<bool>, Self::Error> {
        Ok(self.testnet)
    }
}

pub fn bind(realm: &Realm) {
    let b = |name: &'static str, length: u8, body: fn(&mut Realm, &[Value]) -> Result<Value, Thrown>| {
        realm.bind_import(name, realm.native(name, length, move |r, _, a| body(r, a)));
    };
    b("calculatePricePrecision", 3, |r, a| {
        let (p, amount, max) = (num(r, &arg(a, 0))?, num(r, &arg(a, 1))?, num(r, &arg(a, 2))?);
        market_helpers::calculate_price_precision(r, p, amount, max).map(Value::Number)
    });
    b("symbolToCoin", 1, |r, a| {
        let s = js(r.to_string(&arg(a, 0))?);
        market_helpers::symbol_to_coin(r, &s).map(|s| Value::js(&s))
    });
    b("parseHyperliquidOpenInterestNotional", 2, |r, a| {
        let price = num(r, &arg(a, 1))?;
        market_helpers::parse_hyperliquid_open_interest_notional(r, &arg(a, 0), price).map(Value::Number)
    });
    b("parseHyperliquidVolumeFields", 2, |r, a| {
        let price = num(r, &arg(a, 1))?;
        let f = market_helpers::parse_hyperliquid_volume_fields(r, &arg(a, 0), price)?;
        Ok(r.object_from(vec![("volume", Value::Number(f.volume)), ("quoteVolume", Value::Number(f.quote_volume))]))
    });
    b("decodeHyperliquidFastAssetCtxs", 1, |r, a| market_helpers::decode_hyperliquid_fast_asset_ctxs(r, &arg(a, 0)));
    b("calculatePriceChangePercentage", 2, |r, a| {
        let (p, prev) = (num(r, &arg(a, 0))?, num(r, &arg(a, 1))?);
        Ok(Value::Number(exchange_helpers::calculate_price_change_percentage(p, prev)))
    });
    b("parseFiniteNumber", 1, |r, a| {
        let v = r.to_serde(&arg(a, 0))?;
        exchange_helpers::parse_finite_number(&v).map(Value::Number).map_err(|e| r.error_of(e.name, e.message))
    });
    b("parseHyperliquidOrderTimeInForce", 1, |r, a| {
        let v = r.to_serde(&arg(a, 0))?;
        match order_inference::parse_hyperliquid_order_time_in_force(Some(&v)) {
            Some(tif) => r.from_typed(&tif),
            None => Ok(Value::Undefined),
        }
    });
    b("inferHyperliquidOrderTypeAndPrice", 1, |r, a| {
        let v = r.to_serde(&arg(a, 0))?;
        let out = order_inference::infer_hyperliquid_order_type_and_price(&v).map_err(|e| r.error_of(&e.name.clone(), &e.message))?;
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
        Ok(r.object_from(vec![("type", kind), ("price", Value::Number(out.price)), ("source", Value::str(source))]))
    });
    b("getBuilderAddress", 1, |_, a| Ok(Value::str(constants::get_builder_address(arg(a, 0).truthy()))));
    b("builderFeeRateFromTenthsBps", 1, |r, a| {
        let n = num(r, &arg(a, 0))?;
        Ok(Value::str(&constants::builder_fee_rate_from_tenths_bps(n)))
    });
    b("resolveHyperliquidBuilder", 1, |r, a| {
        let options = arg(a, 0);
        let provider = if options.is_nullish() {
            None
        } else {
            let builder = r.get_str(&options, "hyperliquidBuilder")?;
            let builder = if builder.is_nullish() {
                None
            } else {
                let address = r.get_str(&builder, "address")?;
                let fee = r.get_str(&builder, "feeTenthsBps")?;
                Some(HyperliquidBuilderConfig { address: r.to_string(&address)?.to_vec(), fee_tenths_bps: r.to_number(&fee)? })
            };
            let testnet = r.get_str(&options, "testnet")?;
            Some(BuilderOptions { builder, testnet: if testnet.is_nullish() { None } else { Some(testnet.truthy()) } })
        };
        let Ok(resolved) = constants::resolve_hyperliquid_builder(provider.as_ref());
        Ok(r.object_from(vec![
            ("address", Value::units(&resolved.address)),
            ("feeTenthsBps", Value::Number(resolved.fee_tenths_bps)),
        ]))
    });
    b("maskHyperliquidAddress", 1, |r, a| Ok(opt_js(json_boundaries::mask_hyperliquid_address(r, &arg(a, 0)))));
    b("formatHyperliquidWsDiagnostic", 2, |r, a| {
        let scope = js(r.to_string(&arg(a, 0))?);
        json_boundaries::format_hyperliquid_ws_diagnostic(r, &scope, &arg(a, 1)).map(|s| Value::js(&s))
    });
    b("shouldEmitHyperliquidWsReceiveDiagnostic", 2, |r, a| {
        let scope = js(r.to_string(&arg(a, 0))?);
        json_boundaries::should_emit_hyperliquid_ws_receive_diagnostic(r, &scope, &arg(a, 1)).map(Value::Bool)
    });
    b("shouldEmitHyperliquidWsSendDiagnostic", 1, |r, a| {
        json_boundaries::should_emit_hyperliquid_ws_send_diagnostic(r, &arg(a, 0)).map(Value::Bool)
    });
    b("parseHyperliquidExchangePayload", 1, |r, a| {
        Ok(json_boundaries::parse_hyperliquid_exchange_payload(r, &arg(a, 0))?.unwrap_or(Value::Undefined))
    });
    b("getHyperliquidRestRequestWeight", 2, |r, a| {
        let url = r.to_rust_string(&arg(a, 0))?;
        let data = r.to_serde(&arg(a, 1))?;
        rate_limit::get_hyperliquid_rest_request_weight(&url, &data).map(Value::Number).map_err(|e| r.error_of(e.name, e.message))
    });
    b("orderCarriesClientOrderId", 2, |r, a| {
        let order: safe_cex_types::Order = r.to_typed(&arg(a, 0))?;
        let id = r.to_rust_string(&arg(a, 1))?;
        Ok(Value::Bool(safe_cex_exchange::client_order_id::order_carries_client_order_id(&order, &id)))
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
                crate::browser_approval_flows::check_builder_fee_approved(&mut realm, &args[0], &args[1], &args[2], &args[3]).await
            })))
        }),
    );
    b("validateWalletAndPrivateKey", 2, |r, a| {
        let key = r.to_rust_string(&arg(a, 0))?;
        let wallet = r.to_rust_string(&arg(a, 1))?;
        safe_cex_crypto::wallet_utils::validate_wallet_and_private_key(&key, &wallet)
            .map(Value::Bool)
            .map_err(|e| r.error(&e))
    });
}
