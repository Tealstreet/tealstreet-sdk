//! Pure helpers from asterdex.types.ts.
use crate::{
    tables::{ASTERDEX_MAIN_WALLET_REQUIRED_HINT, ASTERDEX_STP_MODES},
    types::{
        AsterDEXAPIVersion, AsterDEXFapiVersion, AsterDEXUserTradeResponse, AsterDEXV3AuthContext,
    },
};
use safe_cex_types::{ExchangeOptions, ExchangeOptionsExtraAsterdexApiVersion};
use serde_json::Value;
use std::collections::HashMap;

pub fn is_aster_dex_stp_mode(value: &Value) -> bool {
    value
        .as_str()
        .is_some_and(|s| ASTERDEX_STP_MODES.contains(&s))
}

pub fn trim_non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(safe_cex_num::js_trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

pub fn is_aster_dex_v3_private_key(value: Option<&str>) -> bool {
    value.is_some_and(|s| {
        let hex = s.strip_prefix("0x").unwrap_or(s);
        hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit())
    })
}

pub fn is_aster_dex_address(value: Option<&str>) -> bool {
    value
        .and_then(|s| s.strip_prefix("0x"))
        .is_some_and(|s| s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit()))
}

pub fn is_aster_dex_main_wallet_error(message: &Value) -> bool {
    message.as_str().is_some_and(|s| {
        let lower = s.to_lowercase();
        lower.contains(&ASTERDEX_MAIN_WALLET_REQUIRED_HINT.to_lowercase())
            || lower.contains("auth.useraddress")
            || lower.contains("v3 keys require")
    })
}

pub fn resolve_aster_dex_v3_auth_context(options: &ExchangeOptions) -> AsterDEXV3AuthContext {
    let auth = options.auth.as_ref();
    AsterDEXV3AuthContext {
        user_address: trim_non_empty(auth.and_then(|a| a.user_address.as_deref()))
            .or_else(|| trim_non_empty(options.application_id.as_deref())),
        signer_address: trim_non_empty(auth.and_then(|a| a.signer_address.as_deref()))
            .or_else(|| trim_non_empty(Some(&options.key))),
        signer_private_key: trim_non_empty(auth.and_then(|a| a.private_key.as_deref()))
            .or_else(|| trim_non_empty(Some(&options.secret))),
    }
}

pub fn has_aster_dex_v3_auth_context(options: &ExchangeOptions) -> bool {
    let auth = resolve_aster_dex_v3_auth_context(options);
    is_aster_dex_address(auth.user_address.as_deref())
        && is_aster_dex_address(auth.signer_address.as_deref())
        && is_aster_dex_v3_private_key(auth.signer_private_key.as_deref())
}

pub fn resolve_aster_dex_api_version(options: &ExchangeOptions) -> AsterDEXAPIVersion {
    match options
        .extra
        .as_ref()
        .and_then(|e| e.asterdex_api_version.as_ref())
    {
        Some(ExchangeOptionsExtraAsterdexApiVersion::V1) => AsterDEXAPIVersion::V1,
        Some(ExchangeOptionsExtraAsterdexApiVersion::V3) => AsterDEXAPIVersion::V3,
        None if has_aster_dex_v3_auth_context(options) => AsterDEXAPIVersion::V3,
        None => AsterDEXAPIVersion::V1,
    }
}

pub fn get_aster_dex_fapi_path(endpoint: &str, version: &AsterDEXFapiVersion) -> String {
    let version = match version {
        AsterDEXFapiVersion::V1 => "v1",
        AsterDEXFapiVersion::V3 => "v3",
        AsterDEXFapiVersion::V4 => "v4",
    };
    format!("/fapi/{version}/{}", endpoint.trim_start_matches('/'))
}

pub fn get_aster_dex_realized_pnl_key(trade: &AsterDEXUserTradeResponse) -> String {
    let side = trade
        .position_side
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("BOTH");
    format!("{}:{}", trade.symbol, side.to_uppercase())
}

pub fn normalize_aster_dex_realized_pnl_deltas(
    trades: &[AsterDEXUserTradeResponse],
) -> Vec<Option<f64>> {
    let mut normalized = vec![None; trades.len()];
    let mut grouped: HashMap<String, Vec<(usize, f64, f64, f64)>> = HashMap::new();
    for (index, trade) in trades.iter().enumerate() {
        let Some(raw) = trade.realized_pnl.as_deref() else {
            continue;
        };
        let pnl = safe_cex_num::parse_float(raw);
        if !pnl.is_finite() {
            continue;
        }
        grouped
            .entry(get_aster_dex_realized_pnl_key(trade))
            .or_default()
            .push((
                index,
                if trade.id.is_finite() { trade.id } else { 0.0 },
                if trade.time.is_finite() {
                    trade.time
                } else {
                    0.0
                },
                pnl,
            ));
    }
    for bucket in grouped.values_mut() {
        bucket.sort_by(|a, b| {
            a.2.partial_cmp(&b.2)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                .then(a.0.cmp(&b.0))
        });
        let mut previous = None;
        for &(index, _, _, pnl) in bucket.iter() {
            let delta = previous.map_or(0.0, |p: f64| pnl - p);
            normalized[index] = Some(if delta.is_finite() { delta } else { 0.0 });
            previous = Some(pnl);
        }
    }
    normalized
}
