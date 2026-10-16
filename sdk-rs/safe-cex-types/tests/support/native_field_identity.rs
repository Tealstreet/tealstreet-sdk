use safe_cex_types::*;
use serde_json::Value;

pub trait NativeFieldIdentity {
    fn assert_native_fields(&self, expected: &Value);
}
fn equivalent(actual: &Value, expected: &Value) -> bool {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => {
            a.as_f64().unwrap().to_bits() == b.as_f64().unwrap().to_bits()
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equivalent(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, a)| b.get(k).is_some_and(|b| equivalent(a, b)))
        }
        _ => actual == expected,
    }
}
macro_rules! fields {
    ($ty:ty; $($field:ident => $wire:literal),* $(,)?) => {
        impl NativeFieldIdentity for $ty {
            fn assert_native_fields(&self, expected: &Value) {
                $(let actual = serde_json::to_value(&self.$field).unwrap();
                  assert!(equivalent(&actual, &expected[$wire]), concat!("WIRE-NATIVE ", stringify!($ty), ".", stringify!($field)));)*
            }
        }
    };
}
fields!(Order; r#type => "type", id => "id", parent_id => "parentId", status => "status", symbol => "symbol", side => "side", price => "price", amount => "amount", filled => "filled", remaining => "remaining", reduce_only => "reduceOnly", close => "close", info => "info", time_in_force => "timeInForce", last_update => "lastUpdate", trigger_type => "triggerType", tp_trigger_type => "tpTriggerType", sl_trigger_type => "slTriggerType", take_profit => "takeProfit", stop_loss => "stopLoss", extra => "extra");
fields!(Position; id => "id", symbol => "symbol", side => "side", entry_price => "entryPrice", notional => "notional", leverage => "leverage", margin_type => "marginType", position_mode => "positionMode", unrealized_pnl => "unrealizedPnl", realized_pnl => "realizedPnl", contracts => "contracts", liquidation_price => "liquidationPrice", margin_size => "marginSize", info => "info", extra => "extra", last_update => "lastUpdate");
fields!(Market; id => "id", symbol => "symbol", pretty_symbol => "prettySymbol", base => "base", quote => "quote", active => "active", precision => "precision", limits => "limits", contract_size => "contractSize", is_expiring => "isExpiring", is_inverse => "isInverse", is_spot => "isSpot", is_perpetual => "isPerpetual", is_read_only => "isReadOnly", info => "info", taker_fee => "takerFee", maker_fee => "makerFee");
fields!(Ticker; id => "id", symbol => "symbol", bid => "bid", ask => "ask", last => "last", mark => "mark", index => "index", mid_price => "midPrice", percentage => "percentage", open_interest => "openInterest", funding_rate => "fundingRate", funding_time => "fundingTime", volume => "volume", quote_volume => "quoteVolume", info => "info");
fields!(Balance; used => "used", free => "free", total => "total", upnl => "upnl", info => "info", currencies => "currencies", coins => "coins");
fields!(Execution; id => "id", timestamp => "timestamp", symbol => "symbol", order_id => "orderId", price => "price", qty => "qty", side => "side", commission => "commission", commission_asset => "commissionAsset", position_side => "positionSide", realized_pnl => "realizedPnl", maker => "maker", venue_reported_revenue => "venueReportedRevenue", venue_reported_revenue_asset => "venueReportedRevenueAsset");
fields!(OrderBook; bids => "bids", asks => "asks");
fields!(Candle; timestamp => "timestamp", open => "open", high => "high", low => "low", close => "close", volume => "volume", last_side => "lastSide");
fields!(ExchangeAccountConfig; exchange => "exchange", name => "name", testnet => "testnet", api_key => "apiKey", api_secret => "apiSecret", passphrase => "passphrase", secondary_api_key => "secondary_apiKey", secondary_api_secret => "secondary_apiSecret", secondary_passphrase => "secondary_passphrase", enable_spot_markets => "enableSpotMarkets", bybit_site_id => "bybitSiteId", application_id => "applicationId", api_key_index => "apiKeyIndex", vault => "vault", omni_key_seed => "omniKeySeed", subaccount => "subaccount", orderly_exchange_id => "orderlyExchangeId", paradex_address => "paradexAddress", subkey_address => "subkeyAddress", grvt_signer_address => "grvtSignerAddress", grvt_trading_account_id => "grvtTradingAccountId", tealstreet_proxy => "tealstreetProxy", tealstreet_tunnel => "tealstreetTunnel", tealstreet_ws_route => "tealstreetWsRoute", proxy_region => "proxyRegion", use_extension_direct => "useExtensionDirect", user => "user", color => "color", active => "active", pinned => "pinned", telegram_bot_token => "telegramBotToken", last_symbol => "lastSymbol", health_status => "healthStatus", updated_at => "updatedAt", deleted_at => "deletedAt", sync_id => "syncId");
fields!(ExchangeAccount; user_id => "userId", sub_id => "subId", affiliate_id => "affiliateId");
fields!(ExchangeOptions; key => "key", secret => "secret", passphrase => "passphrase", secondary_key => "secondary_key", secondary_secret => "secondary_secret", secondary_passphrase => "secondary_passphrase", secondary_proxy_url => "secondaryProxyUrl", auth => "auth", paradex_address => "paradexAddress", subkey_address => "subkeyAddress", grvt_signer_address => "grvtSignerAddress", grvt_trading_account_id => "grvtTradingAccountId", stark_private_key => "starkPrivateKey", vault => "vault", application_id => "applicationId", api_key_index => "apiKeyIndex", hyperliquid_builder => "hyperliquidBuilder", testnet => "testnet", enable_spot_markets => "enableSpotMarkets", bybit_site_id => "bybitSiteId", public_data_only => "publicDataOnly", private_only => "privateOnly", tealstreet_proxy => "tealstreetProxy", tealstreet_tunnel => "tealstreetTunnel", tealstreet_ws_route => "tealstreetWsRoute", override_proxy_url => "overrideProxyUrl", proxy_region => "proxyRegion", omni_key_seed => "omniKeySeed", subaccount => "subaccount", orderly_exchange_id => "orderlyExchangeId", tuning => "tuning", extra => "extra", remote_signer => "remoteSigner", transport => "transport", payload_signer => "payloadSigner", hosted_mode => "hostedMode", external_public_data => "externalPublicData");
fields!(PublicExchangeOptions; passphrase => "passphrase", secondary_key => "secondary_key", secondary_secret => "secondary_secret", secondary_passphrase => "secondary_passphrase", secondary_proxy_url => "secondaryProxyUrl", auth => "auth", paradex_address => "paradexAddress", subkey_address => "subkeyAddress", grvt_signer_address => "grvtSignerAddress", grvt_trading_account_id => "grvtTradingAccountId", stark_private_key => "starkPrivateKey", vault => "vault", application_id => "applicationId", api_key_index => "apiKeyIndex", hyperliquid_builder => "hyperliquidBuilder", testnet => "testnet", enable_spot_markets => "enableSpotMarkets", bybit_site_id => "bybitSiteId", private_only => "privateOnly", tealstreet_proxy => "tealstreetProxy", tealstreet_tunnel => "tealstreetTunnel", tealstreet_ws_route => "tealstreetWsRoute", override_proxy_url => "overrideProxyUrl", proxy_region => "proxyRegion", omni_key_seed => "omniKeySeed", subaccount => "subaccount", orderly_exchange_id => "orderlyExchangeId", tuning => "tuning", extra => "extra", remote_signer => "remoteSigner", transport => "transport", payload_signer => "payloadSigner", hosted_mode => "hostedMode", external_public_data => "externalPublicData", key => "key", secret => "secret", public_data_only => "publicDataOnly", force_all_proxies_off => "forceAllProxiesOff");
fields!(RequestOutcomeCounts; ok => "ok", rate_limit => "rateLimit", auth => "auth", network => "network", other => "other");
fields!(UpdateOrderOpts; order => "order", update => "update");
