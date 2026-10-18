use super::{CrossTradWeightMode, CrossTradeConfig, CrossTradeCopier, CrossTradeStore, truthy};
use safe_cex_num::sizing::{
    convert_from_margin_to_quote, get_margin_currency, get_total_balance_from_balance,
};
use safe_cex_types::symbol_converter::convert_symbol;
use safe_cex_types::{ExchangeName, Market, object_map::JsObjectMap};

pub(super) fn symbol_mapping(
    map: &JsObjectMap<String>,
    key: &str,
) -> safe_cex_num::TableHit<String> {
    use safe_cex_num::{TableHit, js_table_lookup};
    if let Some(value) = map.get(key) {
        return TableHit::Mapped(value.clone());
    }
    match js_table_lookup::<&str, ()>(&[], key) {
        TableHit::Inherited(member) => TableHit::Inherited(member),
        _ => TableHit::Missing,
    }
}
pub fn get_copier_symbol_from_master(
    copier: &CrossTradeCopier,
    copier_exchange: Option<&ExchangeName>,
    master_exchange: &ExchangeName,
    master_symbol: &str,
) -> safe_cex_num::TableHit<String> {
    let mapped = symbol_mapping(&copier.symbol_mappings, master_symbol);
    if mapped.is_truthy_by(|value| !value.is_empty()) {
        return mapped;
    }
    safe_cex_num::TableHit::Mapped(copier_exchange.map_or_else(
        || master_symbol.into(),
        |exchange| convert_symbol(master_symbol, master_exchange, exchange),
    ))
}
pub fn get_master_symbol_from_copier(
    config: &CrossTradeConfig,
    copier: &CrossTradeCopier,
    master_exchange: Option<&ExchangeName>,
    copier_exchange: &ExchangeName,
    copier_symbol: &str,
) -> safe_cex_num::TableHit<String> {
    let mut reversed = JsObjectMap::<String>::new();
    for (key, value) in &copier.symbol_mappings {
        // lodash assigns primitive strings into {}, invoking its __proto__ setter.
        if value != "__proto__" {
            reversed.insert(value.clone(), key.clone());
        }
    }
    if let Some(symbol) = config.master_symbol.as_ref().filter(|v| !v.is_empty()) {
        return safe_cex_num::TableHit::Mapped(symbol.clone());
    }
    let mapped = symbol_mapping(&reversed, copier_symbol);
    if mapped.is_truthy_by(|value| !value.is_empty()) {
        return mapped;
    }
    safe_cex_num::TableHit::Mapped(master_exchange.map_or_else(
        || copier_symbol.into(),
        |exchange| convert_symbol(copier_symbol, copier_exchange, exchange),
    ))
}
// Source-owned currencyUtils rules. Keep one native implementation for callers.
pub fn is_btc_like_symbol(symbol: &str) -> bool {
    symbol.starts_with("BTC") || symbol.starts_with("XBT")
}
pub fn is_equivalent_currency(a: &str, b: &str) -> bool {
    let usd = |v: &str| {
        matches!(
            v,
            "USD" | "USDT" | "BUSD" | "USDC" | "FDUSD" | "USDY" | "USDD" | "USDP"
        )
    };
    a == b || (usd(a) && usd(b)) || (matches!(a, "BTC" | "XBT") && matches!(b, "BTC" | "XBT"))
}
pub fn get_copier_weight_ratio(
    config: &CrossTradeConfig,
    copier: &CrossTradeCopier,
    master: &CrossTradeStore,
    copy: &CrossTradeStore,
    master_market: &Market,
    copier_market: &Market,
) -> f64 {
    let master_weight = if truthy(config.master_weight) {
        config.master_weight
    } else {
        1.0
    };
    let mut ratio = copier.weight / master_weight;
    if copier.weight_mode == CrossTradWeightMode::Balance {
        let copier_margin =
            get_total_balance_from_balance(copy.balance.as_ref(), Some(copier_market));
        let master_margin =
            get_total_balance_from_balance(master.balance.as_ref(), Some(master_market));
        if !truthy(copier_margin) || !truthy(master_margin) {
            return 0.0;
        }
        if is_equivalent_currency(
            get_margin_currency(Some(copier_market), ""),
            get_margin_currency(Some(master_market), ""),
        ) {
            ratio *= copier_margin / master_margin;
        } else {
            let quote = |amount, market: &Market, store: &CrossTradeStore| {
                let btc = if market
                    .info
                    .as_ref()
                    .and_then(|value| value.get("isQuanto"))
                    .is_some_and(safe_cex_types::retained_json::RetainedJsonInput::is_truthy)
                {
                    store
                        .tickers
                        .iter()
                        .find(|ticker| is_btc_like_symbol(&ticker.symbol))
                        .map(|ticker| ticker.last)
                } else {
                    None
                };
                convert_from_margin_to_quote(
                    amount,
                    market,
                    store
                        .tickers_by_symbol
                        .get(&market.symbol)
                        .map(|ticker| ticker.last),
                    btc,
                )
            };
            ratio *= quote(copier_margin, copier_market, copy)
                / quote(master_margin, master_market, master);
        }
    }
    if ratio.is_finite() { ratio.abs() } else { 0.0 }
}
