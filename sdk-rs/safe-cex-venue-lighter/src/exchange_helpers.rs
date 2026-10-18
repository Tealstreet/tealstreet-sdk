use crate::types::LighterQuoteCurrency;

// ts: parseLighterMarketSymbol
pub fn parse_lighter_market_symbol(
    symbol: &str,
    default_quote: Option<LighterQuoteCurrency>,
) -> (String, String) {
    let default_quote = match default_quote.unwrap_or(LighterQuoteCurrency::Usdc) {
        LighterQuoteCurrency::Usdc => "USDC",
        LighterQuoteCurrency::Usdg => "USDG",
    };
    if let Some((base, tail)) = symbol.split_once('/') {
        let quote = tail
            .split('/')
            .next()
            .filter(|quote| !quote.is_empty())
            .unwrap_or(default_quote);
        return (base.to_owned(), quote.to_owned());
    }
    for quote in ["USDT", "USDC", "USDG"] {
        if let Some(base) = symbol.strip_suffix(quote) {
            return (base.to_owned(), quote.to_owned());
        }
    }
    (symbol.to_owned(), default_quote.to_owned())
}

// ts: isLighterPerpMarket
pub fn is_lighter_perp_market(
    market: &serde_json::Value,
) -> Result<bool, safe_cex_num::JsCoercionError> {
    let value = market
        .get("market_type")
        .filter(|v| !v.is_null())
        .or_else(|| market.get("marketType").filter(|v| !v.is_null()));
    let kind = match value {
        Some(value) => safe_cex_num::js_string(Some(value))?,
        None => String::new(),
    };
    // Coercion intentionally happens before the status test, including errors.
    Ok(
        market.get("status").and_then(serde_json::Value::as_str) == Some("active")
            && kind.to_lowercase() != "spot",
    )
}

// ts: getLighterMarketLeverageMax
pub fn get_lighter_market_leverage_max(
    market: &serde_json::Value,
) -> Result<f64, safe_cex_num::JsCoercionError> {
    let value = [
        "min_initial_margin_fraction",
        "minInitialMarginFraction",
        "default_initial_margin_fraction",
        "defaultInitialMarginFraction",
    ]
    .into_iter()
    .find_map(|key| market.get(key).filter(|v| !v.is_null()));
    let text = match value {
        Some(value) => safe_cex_num::js_string(Some(value))?,
        None => String::new(),
    };
    let fraction = safe_cex_num::parse_float(&text);
    Ok(if !fraction.is_finite() || fraction <= 0.0 {
        50.0
    } else {
        10_000.0 / fraction
    })
}

// ts: parseLighterMarginFractionRatio
pub fn parse_lighter_margin_fraction_ratio(
    value: Option<&serde_json::Value>,
) -> Result<Option<f64>, safe_cex_num::JsCoercionError> {
    let text = match value.filter(|v| !v.is_null()) {
        Some(value) => safe_cex_num::js_string(Some(value))?,
        None => String::new(),
    };
    let parsed = safe_cex_num::parse_float(&text);
    Ok(if !parsed.is_finite() || parsed <= 0.0 {
        None
    } else if parsed > 100.0 {
        Some(parsed / 10_000.0)
    } else if parsed > 1.0 {
        Some(parsed / 100.0)
    } else {
        Some(parsed)
    })
}

// ts: normalizeLighterStopSlippagePct
pub fn normalize_lighter_stop_slippage_pct(
    value: Option<&serde_json::Value>,
) -> Result<Option<f64>, safe_cex_num::JsCoercionError> {
    let text = match value.filter(|v| !v.is_null()) {
        Some(value) => safe_cex_num::js_string(Some(value))?,
        None => String::new(),
    };
    let parsed = safe_cex_num::parse_float(safe_cex_num::js_trim(&text));
    Ok(if !parsed.is_finite() || parsed < 0.0 {
        None
    } else {
        Some(parsed.min(50.0))
    })
}

// ts: LighterExchange.isWasmRuntimeIssueMessage
pub fn is_wasm_runtime_issue_message(message: &str) -> bool {
    let normalized = message.to_lowercase();
    normalized.contains("wasm not initialized")
        || normalized.contains("failed to fetch dynamically imported module")
        || normalized.contains("signerclient export not found")
        || normalized.contains("leverage signer unavailable in runtime")
        || normalized.contains("node:fs")
}

// ts: LighterExchange.isInvalidMarginModeOrderError
pub fn is_invalid_margin_mode_order_error(
    error: Option<&serde_json::Value>,
    message: &str,
) -> Result<bool, safe_cex_num::JsCoercionError> {
    let code = error
        .and_then(|v| v.get("code"))
        .filter(|v| !v.is_null())
        .or_else(|| {
            error
                .and_then(|v| v.get("response"))
                .and_then(|v| v.get("data"))
                .and_then(|v| v.get("code"))
                .filter(|v| !v.is_null())
        });
    let code = match code {
        Some(value) => safe_cex_num::js_string(Some(value))?,
        None => String::new(),
    }
    .to_lowercase();
    let normalized_message = message.to_lowercase();
    Ok(code == "21613" || normalized_message.contains("invalid margin mode"))
}

// ts: LighterExchange.isPartnerAttributionTierOrderError
pub fn is_partner_attribution_tier_order_error(
    error: Option<&serde_json::Value>,
    message: &str,
) -> Result<bool, safe_cex_num::JsCoercionError> {
    let data = error
        .and_then(|v| v.get("response"))
        .and_then(|v| v.get("data"));
    let code = error
        .and_then(|v| v.get("code"))
        .filter(|v| !v.is_null())
        .or_else(|| data.and_then(|v| v.get("code")).filter(|v| !v.is_null()))
        .or_else(|| {
            data.and_then(|v| v.get("error"))
                .and_then(|v| v.get("code"))
        });
    crate::account_tier::is_lighter_partner_attribution_tier_rejection(
        code,
        Some(&serde_json::Value::String(message.to_owned())),
    )
}

// ts: LighterExchange.isPartnerAttributionApprovalMissingOrderError
pub fn is_partner_attribution_approval_missing_order_error(
    error: Option<&serde_json::Value>,
    message: &str,
) -> Result<bool, safe_cex_num::JsCoercionError> {
    let data = error
        .and_then(|v| v.get("response"))
        .and_then(|v| v.get("data"));
    let code = error
        .and_then(|v| v.get("code"))
        .filter(|v| !v.is_null())
        .or_else(|| data.and_then(|v| v.get("code")).filter(|v| !v.is_null()))
        .or_else(|| {
            data.and_then(|v| v.get("error"))
                .and_then(|v| v.get("code"))
        });
    crate::account_tier::is_lighter_partner_attribution_approval_missing_rejection(
        code,
        Some(&serde_json::Value::String(message.to_owned())),
    )
}

// ts: LighterExchange.getPositivePriceCandidate
pub fn get_positive_price_candidate(values: &[Option<f64>]) -> Option<f64> {
    values
        .iter()
        .flatten()
        .copied()
        .find(|value| value.is_finite() && *value > 0.0)
}

// ts: LighterExchange.shouldUseBatchedCreateOrders
pub fn should_use_batched_create_orders(orders: &[safe_cex_types::PlaceOrderOpts]) -> bool {
    if orders.len() <= 1 {
        return false;
    }
    let symbol = &orders[0].symbol;
    orders.iter().all(|order| {
        order.symbol == *symbol
            && order.r#type == safe_cex_types::OrderType::Limit
            && !order.reduce_only.unwrap_or(false)
            && order.stop_loss.is_none()
            && order.take_profit.is_none()
            && order
                .price
                .is_some_and(|price| price.is_finite() && price > 0.0)
    })
}

// ts: LighterExchange.snapToLeverageTier
pub fn snap_to_leverage_tier(requested: f64) -> f64 {
    // Source order decides exact midpoint ties; NaN/infinity retain the first tier.
    let tiers = [1.0_f64, 2.0, 3.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 50.0];
    let mut closest = tiers[0];
    let mut closest_dist = (requested - closest).abs();
    for tier in tiers {
        let dist = (requested - tier).abs();
        if dist < closest_dist {
            closest = tier;
            closest_dist = dist;
        }
    }
    closest
}

// ts: LighterExchange.getInitialTickDelay
// The runtime supplies the result of exactly one getTickInterval call.
pub fn get_initial_tick_delay(tick_interval: f64) -> f64 {
    const LIGHTER_INITIAL_TICK_MAX_DELAY_MS: f64 = 2_000.0;
    if tick_interval > 0.0 {
        tick_interval.min(LIGHTER_INITIAL_TICK_MAX_DELAY_MS)
    } else {
        LIGHTER_INITIAL_TICK_MAX_DELAY_MS
    }
}

// ts: LighterExchange.parseMarginMode
pub fn parse_margin_mode(
    value: Option<&safe_cex_types::retained_json::RetainedJsonInput>,
) -> Result<Option<crate::signer_types::LighterSignerMarginMode>, safe_cex_num::JsCoercionError> {
    use crate::signer_types::LighterSignerMarginMode as Mode;
    let Some(value) = value.filter(|v| !v.is_nullish()) else {
        return Ok(None);
    };
    if value.as_f64() == Some(0.0) || value.text_equals("0") {
        return Ok(Some(Mode::CROSS));
    }
    if value.as_f64() == Some(1.0) || value.text_equals("1") {
        return Ok(Some(Mode::ISOLATED));
    }
    let units = safe_cex_types::js_coercion::js_retained_string_utf16(value)?;
    let normalized = String::from_utf16(&units)
        .ok()
        .map(|text| text.to_lowercase());
    Ok(match normalized.as_deref() {
        Some("cross") => Some(Mode::CROSS),
        Some("isolated") => Some(Mode::ISOLATED),
        _ => None,
    })
}

// ts: LighterExchange.marginModeToMarginType
pub fn margin_mode_to_margin_type(
    margin_mode: Option<crate::signer_types::LighterSignerMarginMode>,
) -> safe_cex_types::MarginType {
    if margin_mode.is_some_and(|mode| f64::from(mode) == 1.0) {
        safe_cex_types::MarginType::Isolated
    } else {
        safe_cex_types::MarginType::Cross
    }
}

// ts: LighterExchange.marginTypeToMarginMode
pub fn margin_type_to_margin_mode(
    margin_type: Option<safe_cex_types::MarginType>,
) -> crate::signer_types::LighterSignerMarginMode {
    if margin_type == Some(safe_cex_types::MarginType::Isolated) {
        crate::signer_types::LighterSignerMarginMode::ISOLATED
    } else {
        crate::signer_types::LighterSignerMarginMode::CROSS
    }
}

// ts: LighterExchange.parseAccountIndex
pub fn parse_account_index(
    value: crate::builder_approval::LighterApprovalValue<'_>,
) -> Option<num_bigint::BigInt> {
    // The two source bodies are identical: nullish guard, String, BigInt, catch.
    crate::builder_approval::to_big_int(value)
}

// ts: LighterExchange.extractL1AddressCandidate
pub fn extract_l1_address_candidate(value: Option<&str>) -> Option<String> {
    let value = value.filter(|value| !value.is_empty())?;
    let trimmed = safe_cex_num::js_trim(value);
    if trimmed.len() == 42
        && trimmed.starts_with("0x")
        && trimmed.as_bytes()[2..].iter().all(u8::is_ascii_hexdigit)
    {
        return Some(trimmed.to_owned());
    }
    // Literal, case-sensitive 0x followed by forty ASCII hex digits, unanchored.
    let start = trimmed.as_bytes().windows(42).position(|candidate| {
        candidate.starts_with(b"0x") && candidate[2..].iter().all(u8::is_ascii_hexdigit)
    })?;
    trimmed.get(start..start + 42).map(str::to_owned)
}

// ts: LighterExchange.getReduceOnlyClosingPositionSide
pub fn get_reduce_only_closing_position_side(
    order_side: safe_cex_types::OrderSide,
) -> safe_cex_types::PositionSide {
    if order_side == safe_cex_types::OrderSide::Buy {
        safe_cex_types::PositionSide::Short
    } else {
        safe_cex_types::PositionSide::Long
    }
}

// ts: LighterExchange.hasOpenReduceOnlyTargetPosition
pub fn has_open_reduce_only_target_position(
    symbol: &str,
    order_side: safe_cex_types::OrderSide,
    positions: &[safe_cex_types::Position],
) -> bool {
    let closing_position_side = get_reduce_only_closing_position_side(order_side);
    positions.iter().any(|position| {
        position.symbol == symbol
            && position.side == closing_position_side
            && position.contracts > 0.0
    })
}

// ts: LighterExchange.buildNoPositionReduceOnlyMessage
pub fn build_no_position_reduce_only_message(
    symbol: &str,
    order_side: safe_cex_types::OrderSide,
) -> String {
    let closing_side_label = if order_side == safe_cex_types::OrderSide::Buy {
        "short"
    } else {
        "long"
    };
    format!(
        "Lighter rejected reduce-only limit order for {symbol}: no open {closing_side_label} position to reduce."
    )
}

// ts: LighterExchange.getOrderMinimumViolationReasons
pub fn get_order_minimum_violation_reasons(
    amount: f64,
    execution_price: f64,
    min_base_amount: f64,
    min_quote_amount: f64,
) -> Option<String> {
    let quote_amount = amount * execution_price;
    let is_below_base_minimum = min_base_amount > 0.0 && amount < min_base_amount;
    let is_below_quote_minimum = min_quote_amount > 0.0 && quote_amount < min_quote_amount;
    if !is_below_base_minimum && !is_below_quote_minimum {
        return None;
    }
    let mut reasons = Vec::new();
    if is_below_base_minimum {
        reasons.push(format!(
            "base {} < min_base_amount {}",
            safe_cex_num::js_number_string(amount),
            safe_cex_num::js_number_string(min_base_amount)
        ));
    }
    if is_below_quote_minimum {
        reasons.push(format!(
            "quote {} < min_quote_amount {}",
            safe_cex_num::js_number_string(quote_amount),
            safe_cex_num::js_number_string(min_quote_amount)
        ));
    }
    Some(reasons.join(", "))
}

// ts: LighterExchange.isRateLimitOrderError
pub fn is_rate_limit_order_error(
    error: Option<&serde_json::Value>,
    message: &str,
) -> Result<bool, safe_cex_num::JsCoercionError> {
    let response = error.and_then(|v| v.get("response"));
    let status = error
        .and_then(|v| v.get("statusCode"))
        .filter(|v| !v.is_null())
        .or_else(|| response.and_then(|v| v.get("status")));
    let status_code = safe_cex_num::js_number(status)?;
    let code = error
        .and_then(|v| v.get("code"))
        .filter(|v| !v.is_null())
        .or_else(|| {
            response
                .and_then(|v| v.get("data"))
                .and_then(|v| v.get("code"))
                .filter(|v| !v.is_null())
        });
    let code = match code {
        Some(value) => safe_cex_num::js_string(Some(value))?,
        None => String::new(),
    }
    .to_lowercase();
    let normalized_message = message.to_lowercase();
    Ok(status_code == 429.0
        || status_code == 405.0
        || code == "429"
        || code == "405"
        || normalized_message.contains("rate limit")
        || normalized_message.contains("too many requests")
        || normalized_message.contains("too many websocket messages")
        || normalized_message.contains("volume quota"))
}

// ts: LighterExchange.resolveStopSlippagePct
pub fn resolve_stop_slippage_pct(
    max_slippage_pct: Option<f64>,
    extra_slippage_pct: Option<&serde_json::Value>,
) -> Result<f64, safe_cex_num::JsCoercionError> {
    let explicit = max_slippage_pct
        .map(|value| serde_json::Value::String(safe_cex_num::js_number_string(value)));
    if let Some(value) = normalize_lighter_stop_slippage_pct(explicit.as_ref())? {
        return Ok(value);
    }
    if let Some(value) = normalize_lighter_stop_slippage_pct(extra_slippage_pct)? {
        return Ok(value);
    }
    Ok(1.0)
}

// ts: LighterExchange.getMarketConfiguredMarginMode
pub fn get_market_configured_margin_mode(
    market: Option<&safe_cex_types::Market>,
) -> Result<Option<crate::signer_types::LighterSignerMarginMode>, safe_cex_num::JsCoercionError> {
    let info = market.and_then(|m| m.info.as_ref());
    let value = info
        .and_then(|v| v.get("market_config"))
        .and_then(|v| v.get("market_margin_mode"))
        .filter(|v| !v.is_nullish())
        .or_else(|| {
            info.and_then(|v| v.get("marketConfig"))
                .and_then(|v| v.get("marketMarginMode"))
                .filter(|v| !v.is_nullish())
        })
        .or_else(|| {
            info.and_then(|v| v.get("market_margin_mode"))
                .filter(|v| !v.is_nullish())
        })
        .or_else(|| info.and_then(|v| v.get("marketMarginMode")));
    parse_margin_mode(value)
}

// ts: LighterExchange.buildInvalidMarginModeOrderMessage
pub fn build_invalid_margin_mode_order_message(
    symbol: &str,
    market: Option<&safe_cex_types::Market>,
) -> Result<String, safe_cex_num::JsCoercionError> {
    let mode = get_market_configured_margin_mode(market)?;
    let required_mode = match mode {
        Some(mode) if f64::from(mode) == 1.0 => "isolated",
        Some(_) => "cross",
        None => "the intended margin mode",
    };
    Ok(format!(
        "Lighter rejected this order because the {symbol} margin mode appears out of sync. This market requires {required_mode} to trade."
    ))
}

// ts: LighterExchange.getOhlcvFallbackKey
pub fn get_ohlcv_fallback_key(symbol: &str, interval: safe_cex_types::Timeframe) -> String {
    use safe_cex_types::Timeframe;
    let interval = match interval {
        Timeframe::M1 => "1m",
        Timeframe::M3 => "3m",
        Timeframe::M5 => "5m",
        Timeframe::M15 => "15m",
        Timeframe::M30 => "30m",
        Timeframe::H1 => "1h",
        Timeframe::H2 => "2h",
        Timeframe::H4 => "4h",
        Timeframe::H6 => "6h",
        Timeframe::H12 => "12h",
        Timeframe::D1 => "1d",
        Timeframe::W1 => "1w",
    };
    format!("{symbol}:{interval}")
}

// ts: LighterExchange.inferStopLossTypeFromReduceOnlyLimit
pub fn infer_stop_loss_type_from_reduce_only_limit(
    order: &safe_cex_types::Order,
    ticker: Option<&safe_cex_types::Ticker>,
) -> Option<safe_cex_types::OrderType> {
    use safe_cex_types::{OrderSide, OrderType};
    if order.r#type != OrderType::Limit
        || order.reduce_only != true
        || !order.price.to_number().is_finite()
        || order.price.to_number() <= 0.0
    {
        return None;
    }
    let reference_price = get_positive_price_candidate(&[
        ticker.map(|t| t.mark),
        ticker.map(|t| t.last),
        ticker.map(|t| t.index),
    ])?;
    if order.side == OrderSide::Sell {
        (order.price.to_number() <= reference_price).then_some(OrderType::StopLoss)
    } else {
        (order.price.to_number() >= reference_price).then_some(OrderType::StopLoss)
    }
}

// ts: LighterExchange.resolveMarketOrderReferencePrice
pub fn resolve_market_order_reference_price(
    symbol: &str,
    side: safe_cex_types::OrderSide,
    ticker: Option<&safe_cex_types::Ticker>,
) -> Result<f64, safe_cex_exchange::runtime::ExchangeError> {
    use safe_cex_exchange::runtime::ExchangeError;
    use safe_cex_types::OrderSide;
    let ticker = ticker.ok_or_else(|| {
        ExchangeError::new(format!("Ticker {symbol} not found for market-style order"))
    })?;
    let midpoint =
        if ticker.bid > 0.0 && ticker.ask > 0.0 && ticker.bid.is_finite() && ticker.ask.is_finite()
        {
            Some((ticker.bid + ticker.ask) / 2.0)
        } else {
            None
        };
    let (side_price, opposite_price) = if side == OrderSide::Buy {
        (ticker.ask, ticker.bid)
    } else {
        (ticker.bid, ticker.ask)
    };
    get_positive_price_candidate(&[
        Some(side_price),
        Some(ticker.mark),
        Some(ticker.last),
        Some(ticker.index),
        midpoint,
        Some(opposite_price),
    ])
    .ok_or_else(|| {
        ExchangeError::new(format!(
            "No live Lighter price available for market-style order ({symbol})"
        ))
    })
}
